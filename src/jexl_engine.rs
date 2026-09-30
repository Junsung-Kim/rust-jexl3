// port of: org.apache.commons.jexl3.JexlEngine and org.apache.commons.jexl3.internal.Engine
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::internal::engine::{get_variables_mode, pragmas_as_map};
use crate::internal::debugger::Debugger;
use crate::internal::frame::{create_frame, Frame};
use crate::internal::script::Closure;
use crate::internal::interpreter::{EngineRef, Interpreter};
use crate::introspection::uberspect::Uberspect;
use crate::introspection::JexlUberspect;
use crate::java::string::JString;
use crate::jexl_arithmetic::JexlArithmetic;
use crate::jexl_context::{EmptyContext, JexlContext};
use crate::jexl_exception::JexlException;
use crate::jexl_features::JexlFeatures;
use crate::jexl_info::JexlInfo;
use crate::jexl_options::JexlOptions;
use crate::parser::jexl_node::Parsed;
use crate::parser::parser::Parser;
use crate::value::Value;

/// port of: Engine.PRAGMA_OPTIONS
const PRAGMA_OPTIONS: &str = "jexl.options";
/// port of: Engine.PRAGMA_JEXLNS
const PRAGMA_JEXLNS: &str = "jexl.namespace.";

/// port of: org.apache.commons.jexl3.JexlEngine (the `Engine` implementation)
pub struct JexlEngine {
    pub(crate) uberspect: Arc<dyn JexlUberspect>,
    pub(crate) arithmetic: JexlArithmetic,
    pub(crate) functions: Arc<HashMap<String, Value>>,
    pub(crate) stack_overflow: i32,
    pub(crate) strict: bool,
    pub(crate) safe: bool,
    pub(crate) silent: bool,
    pub(crate) cancellable: bool,
    pub(crate) debug: bool,
    pub(crate) script_features: JexlFeatures,
    pub(crate) expression_features: JexlFeatures,
    pub(crate) cache_size: i32,
    pub(crate) cache_threshold: i32,
    pub(crate) collect_mode: i32,
    pub(crate) options: JexlOptions,
    /// the shared parser (JEXL reuses one per engine, state leaks and all)
    parser: Mutex<Parser>,
    /// port of: Engine.cache — a bounded LRU of (features, source) -> tree
    cache: Mutex<SoftCache>,
}

/// String.length() — the number of UTF-16 code units, without encoding the whole string.
fn utf16_len(s: &str) -> usize {
    if s.is_ascii() {
        return s.len();
    }
    s.encode_utf16().count()
}

/// port of: org.apache.commons.jexl3.internal.SoftCache
///
/// Java's is a LinkedHashMap in access order that evicts its eldest entry past the capacity. This
/// keeps the same semantics with the same complexity: a map from source text to the trees parsed
/// from it (one per feature set, and one feature set is the usual case), plus the access order.
struct SoftCache {
    capacity: usize,
    entries: HashMap<String, Vec<(JexlFeatures, Arc<Parsed>)>>,
    /// least-recently-used first, one entry per (source, feature set) pair
    order: std::collections::VecDeque<(String, usize)>,
}

impl SoftCache {
    fn new(capacity: usize) -> SoftCache {
        SoftCache { capacity, entries: HashMap::new(), order: std::collections::VecDeque::new() }
    }

    fn get(&mut self, features: &JexlFeatures, src: &str) -> Option<Arc<Parsed>> {
        let variants = self.entries.get(src)?;
        let at = variants.iter().position(|(f, _)| f == features)?;
        let parsed = variants[at].1.clone();
        // LinkedHashMap(accessOrder = true): a read moves the entry to the end
        if let Some(pos) = self.order.iter().position(|(s, i)| s == src && *i == at) {
            let e = self.order.remove(pos).expect("found above");
            self.order.push_back(e);
        }
        Some(parsed)
    }

    fn put(&mut self, features: JexlFeatures, src: String, parsed: Arc<Parsed>) {
        if self.capacity == 0 {
            return;
        }
        if self.get(&features, &src).is_some() {
            return;
        }
        while self.order.len() >= self.capacity {
            // removeEldestEntry
            if let Some((s, i)) = self.order.pop_front() {
                if let Some(v) = self.entries.get_mut(&s) {
                    if i < v.len() {
                        v.remove(i);
                    }
                    if v.is_empty() {
                        self.entries.remove(&s);
                    }
                }
                // the indexes behind the removed variant shifted down
                for (os, oi) in self.order.iter_mut() {
                    if *os == s && *oi > i {
                        *oi -= 1;
                    }
                }
            }
        }
        let variants = self.entries.entry(src.clone()).or_default();
        variants.push((features, parsed));
        self.order.push_back((src, variants.len() - 1));
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }
}

/// port of: org.apache.commons.jexl3.internal.Script
#[derive(Clone)]
pub struct JexlScript {
    engine: Arc<JexlEngine>,
    source: Option<String>,
    parsed: Arc<Parsed>,
    /// port of: Closure.frame — Some once `curry` has bound arguments, which is exactly what makes
    /// a Java `Script` a `Closure`.
    frame: Option<Frame>,
    /// The engine view this script runs with when the context brings no options of its own:
    /// then it depends on the engine and the script alone, so it is worked out once.
    engine_ref: std::sync::OnceLock<Arc<EngineRef>>,
}

impl JexlEngine {
    /// port of: JexlEngine.createExpression(JexlInfo, String)
    #[track_caller]
    pub fn create_expression(self: &Arc<Self>, info: Option<JexlInfo>, expression: &str) -> Result<JexlScript, JexlException> {
        let features = self.expression_features.clone();
        self.create_script_features(features, info, expression, None)
    }

    /// port of: JexlEngine.createScript(String)
    #[track_caller]
    pub fn create_script(self: &Arc<Self>, script_text: &str) -> Result<JexlScript, JexlException> {
        let features = self.script_features.clone();
        self.create_script_features(features, None, script_text, None)
    }

    /// port of: JexlEngine.createScript(String, String...)
    #[track_caller]
    pub fn create_script_named(self: &Arc<Self>, script_text: &str, names: &[String]) -> Result<JexlScript, JexlException> {
        let features = self.script_features.clone();
        self.create_script_features(features, None, script_text, Some(names))
    }

    /// port of: JexlEngine.createScript(JexlInfo, String, String...)
    #[track_caller]
    pub fn create_script_info(
        self: &Arc<Self>,
        info: Option<JexlInfo>,
        script_text: &str,
        names: Option<&[String]>,
    ) -> Result<JexlScript, JexlException> {
        let features = self.script_features.clone();
        self.create_script_features(features, info, script_text, names)
    }

    /// port of: Engine.createScript(JexlFeatures, JexlInfo, String, String[])
    #[track_caller]
    pub fn create_script_features(
        self: &Arc<Self>,
        features: JexlFeatures,
        info: Option<JexlInfo>,
        script_text: &str,
        names: Option<&[String]>,
    ) -> Result<JexlScript, JexlException> {
        let source = crate::internal::engine::trim_source(script_text);
        // port of: `info == null ? createInfo() : info`, built only on a cache miss since a hit
        // never reads it; the location is taken here because a closure does not carry #[track_caller]
        let caller = std::panic::Location::caller();
        let info = || Some(info.unwrap_or_else(|| self.create_info_at(caller)));
        let parsed = self.parse(info, &features, &source, names)?;
        Ok(JexlScript { engine: self.clone(), source: Some(source), parsed, frame: None, engine_ref: std::sync::OnceLock::new() })
    }

    /// port of: Engine.parse(JexlInfo, JexlFeatures, String, Scope)
    fn parse(
        self: &Arc<Self>,
        info: impl FnOnce() -> Option<JexlInfo>,
        features: &JexlFeatures,
        src: &str,
        names: Option<&[String]>,
    ) -> Result<Arc<Parsed>, JexlException> {
        let cached = self.cache_size > 0 && (utf16_len(src) as i32) < self.cache_threshold;
        if cached && names.is_none() {
            if let Some(hit) = self.cache.lock().unwrap_or_else(|p| p.into_inner()).get(features, src) {
                return Ok(hit);
            }
        }
        let mut parser = self.parser.lock().unwrap_or_else(|p| p.into_inner());
        let parsed = Arc::new(parser.parse(info(), features, src, names)?);
        drop(parser);
        if cached && names.is_none() {
            self.cache
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .put(features.clone(), src.to_string(), parsed.clone());
        }
        Ok(parsed)
    }

    /// port of: Engine.createInfo() — `new JexlInfo()` when debugging, null otherwise. A null
    /// info would make `TemplateEngine.parseExpression` throw, so the zero info stands in for it.
    #[track_caller]
    pub(crate) fn create_info(&self) -> JexlInfo {
        self.create_info_at(std::panic::Location::caller())
    }

    fn create_info_at(&self, caller: &std::panic::Location<'_>) -> JexlInfo {
        if self.debug {
            JexlInfo::at_location(caller)
        } else {
            JexlInfo::new(None, 0, 0)
        }
    }

    /// port of: Engine.options(null, JexlContext) — the options with no script to take pragmas
    /// from (what `TemplateEngine.TemplateExpression.options` uses).
    pub(crate) fn options_no_script(&self, context: &dyn JexlContext) -> JexlOptions {
        let mut opts = self.options_for(context);
        if self.script_features.is_lexical() {
            opts.set_lexical(true);
        }
        if self.script_features.is_lexical_shade() {
            opts.set_lexical_shade(true);
        }
        opts
    }

    /// port of: Engine.parse(JexlInfo, boolean, String, Scope) — the entry `TemplateEngine` uses.
    /// `expr` picks the expression (no-script) features; `scope` is the arena and frame of the
    /// template script the sub-expression belongs to.
    pub(crate) fn parse_jxlt(
        self: &Arc<Self>,
        info: Option<JexlInfo>,
        expr: bool,
        src: &str,
        scope: Option<(&crate::internal::scope::Scopes, crate::internal::scope::ScopeId)>,
    ) -> Result<Arc<Parsed>, JexlException> {
        let features = if expr { self.expression_features.clone() } else { self.script_features.clone() };
        let cached = self.cache_size > 0 && (utf16_len(src) as i32) < self.cache_threshold;
        if cached {
            if let Some(hit) = self.cache.lock().unwrap_or_else(|p| p.into_inner()).get(&features, src) {
                // Java only reuses a cached tree whose Scope equals the one asked for
                let f = hit.node().get_scope();
                let matches = match (f, scope) {
                    (None, None) => true,
                    (Some(f), Some((scopes, id))) => *f == *scopes.get(id),
                    _ => false,
                };
                if matches {
                    return Ok(hit);
                }
            }
        }
        let mut parser = self.parser.lock().unwrap_or_else(|p| p.into_inner());
        let parsed = match scope {
            Some((scopes, id)) => Arc::new(parser.parse_in_scope(info, &features, src, scopes, Some(id))?),
            None => Arc::new(parser.parse(info, &features, src, None)?),
        };
        drop(parser);
        if cached {
            self.cache
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .put(features, src.to_string(), parsed.clone());
        }
        Ok(parsed)
    }

    /// port of: JexlEngine.createJxltEngine()
    pub fn create_jxlt_engine(self: &Arc<Self>) -> Arc<crate::internal::template_engine::TemplateEngine> {
        crate::jxlt_engine::create_jxlt_engine(self)
    }

    /// port of: JexlEngine.createJxltEngine(boolean, int, char, char)
    pub fn create_jxlt_engine_with(
        self: &Arc<Self>,
        noscript: bool,
        cache_size: i32,
        immediate: char,
        deferred: char,
    ) -> Arc<crate::internal::template_engine::TemplateEngine> {
        crate::jxlt_engine::create_jxlt_engine_with(self, noscript, cache_size, immediate, deferred)
    }

    /// port of: Engine.jxlt() — the lazily built default template engine. Its cache size is 0,
    /// so (unlike Java) there is nothing to keep between calls and a fresh one is built each time.
    pub(crate) fn jxlt(self: &Arc<Self>) -> Arc<crate::internal::template_engine::TemplateEngine> {
        crate::jxlt_engine::create_jxlt_engine_with(self, true, 0, '$', '#')
    }

    /// port of: JexlEngine.clearCache
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }

    pub fn get_arithmetic(&self) -> &JexlArithmetic {
        &self.arithmetic
    }

    pub fn is_strict(&self) -> bool {
        self.strict
    }
    pub fn is_silent(&self) -> bool {
        self.silent
    }
    pub fn is_debug(&self) -> bool {
        self.debug
    }
    pub fn is_cancellable(&self) -> bool {
        self.cancellable
    }

    // port of: Engine.options(JexlContext)
    pub(crate) fn options_for(&self, context: &dyn JexlContext) -> JexlOptions {
        match context.get_engine_options() {
            Some(o) => o,
            None => self.options.clone(),
        }
    }

    // port of: Engine.options(ASTJexlScript, JexlContext) and Engine.processPragmas
    pub(crate) fn options_for_script(&self, parsed: &Parsed, context: &dyn JexlContext) -> JexlOptions {
        self.script_options(self.options_for(context), parsed, Some(context))
    }

    /// The options a script runs with, from `opts`; `processor` hears each pragma (None when the
    /// caller notifies it separately).
    fn script_options(&self, mut opts: JexlOptions, parsed: &Parsed, processor: Option<&dyn JexlContext>) -> JexlOptions {
        if self.script_features.is_lexical() {
            opts.set_lexical(true);
        }
        if self.script_features.is_lexical_shade() {
            opts.set_lexical_shade(true);
        }
        let node = parsed.node();
        if let Some(pragmas) = node.script().and_then(|s| s.get_pragmas()) {
            if !pragmas.is_empty() {
                let mut ns: Option<HashMap<String, Value>> = None;
                for (key, value) in pragmas {
                    let key_s = key.to_rust();
                    if let Value::String(text) = value {
                        if key_s == PRAGMA_OPTIONS {
                            let text = text.to_rust();
                            let flags: Vec<&str> = text.split(' ').collect();
                            opts.set_flags(&flags);
                        } else if let Some(nsname) = key_s.strip_prefix(PRAGMA_JEXLNS) {
                            if !nsname.is_empty() {
                                let map = ns.get_or_insert_with(|| (*self.functions).clone());
                                map.insert(nsname.to_string(), value.clone());
                            }
                        }
                    }
                    if let Some(context) = processor {
                        if context.is_pragma_processor() {
                            context.process_pragma(key, value);
                        }
                    }
                }
                if let Some(map) = ns {
                    opts.set_namespaces(map);
                }
            }
        }
        opts
    }

    /// port of: the `processPragma` half of Engine.processPragmas, for a context that implements
    /// JexlContext.PragmaProcessor.
    fn notify_pragmas(&self, parsed: &Parsed, context: &dyn JexlContext) {
        if let Some(pragmas) = parsed.node().script().and_then(|s| s.get_pragmas()) {
            for (key, value) in pragmas {
                context.process_pragma(key, value);
            }
        }
    }

    pub(crate) fn engine_ref(self: &Arc<Self>, options: &JexlOptions) -> Arc<EngineRef> {
        Arc::new(EngineRef {
            engine: self.clone(),
            uberspect: self.uberspect.clone(),
            arithmetic: self.arithmetic.clone(),
            functions: self.functions.clone(),
            cache: self.cache_size > 0,
            stack_overflow: self.stack_overflow,
            strict: self.strict,
            safe: self.safe,
            silent: self.silent,
            cancellable: self.cancellable,
            debug: self.debug,
            options: options.clone(),
        })
    }
}

impl JexlScript {
    /// port of: Script.getSourceText
    pub fn get_source_text(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// port of: Script.getParsedText()
    pub fn get_parsed_text(&self) -> JString {
        self.get_parsed_text_indent(2)
    }

    /// port of: Script.getParsedText(int)
    pub fn get_parsed_text_indent(&self, indent: i32) -> JString {
        let mut debugger = Debugger::new();
        debugger.set_indentation(indent);
        debugger.debug_r(self.parsed.node(), false);
        debugger.to_jstring()
    }

    /// port of: Script.toString — the source if there is one, the rendered tree otherwise.
    pub fn java_to_jstring(&self) -> JString {
        match &self.source {
            Some(source) => JString::from(source.as_str()),
            None => {
                let mut debugger = Debugger::new();
                debugger.debug_r(self.parsed.node(), false);
                debugger.to_jstring()
            }
        }
    }

    /// The Java class a script reports: currying turns a `Script` into a `Closure`.
    pub fn class_name(&self) -> String {
        if self.frame.is_some() {
            "org.apache.commons.jexl3.internal.Closure".into()
        } else {
            "org.apache.commons.jexl3.internal.Script".into()
        }
    }

    /// port of: Script.curry(Object...) — `new Closure(this, args)`, or `this` when the script
    /// declares no parameters to bind.
    pub fn curry(&self, args: &[Value]) -> JexlScript {
        let scopes = self.parsed.ast.scopes_ref();
        let scope = self.parsed.node().script().and_then(|s| s.get_scope());
        let parameters = scope.map(|s| scopes.get(s).get_parameters()).unwrap_or_default();
        if parameters.is_empty() {
            return self.clone();
        }
        // port of: Closure(Script, Object[]) — a closure re-curries its own frame
        let frame = match &self.frame {
            Some(f) => Some(f.assign(scopes.get(f.scope()), Some(args))),
            None => scope.and_then(|s| create_frame(scopes, s, None, Some(args))),
        };
        JexlScript {
            engine: self.engine.clone(),
            source: self.source.clone(),
            parsed: self.parsed.clone(),
            frame,
            engine_ref: self.engine_ref.clone(),
        }
    }

    /// port of: Script.getUnboundParameters / Closure.getUnboundParameters
    pub fn get_unbound_parameters(&self) -> Vec<String> {
        match &self.frame {
            Some(f) => f.get_unbound_parameters(self.parsed.ast.scopes_ref().get(f.scope())),
            None => self.get_parameters(),
        }
    }

    /// port of: Script.getVariables
    pub fn get_variables(&self) -> Vec<Vec<JString>> {
        get_variables_mode(&self.parsed, self.engine.collect_mode)
    }

    /// port of: Script.getParameters
    pub fn get_parameters(&self) -> Vec<String> {
        self.parsed.node().get_scope().map(|s| s.get_parameters()).unwrap_or_default()
    }

    /// port of: Script.getLocalVariables
    pub fn get_local_variables(&self) -> Vec<String> {
        self.parsed.node().get_scope().map(|s| s.get_local_variables()).unwrap_or_default()
    }

    /// port of: Script.getPragmas
    pub fn get_pragmas(&self) -> Value {
        let pragmas = self.parsed.node().script().and_then(|s| s.get_pragmas()).cloned().unwrap_or_default();
        Value::Map(pragmas_as_map(&pragmas))
    }

    pub fn parsed(&self) -> &Arc<Parsed> {
        &self.parsed
    }

    /// port of: Script.execute(JexlContext)
    pub fn execute(&self, context: Arc<dyn JexlContext>) -> Result<Value, JexlException> {
        self.execute_args(context, &[])
    }

    /// port of: Script.execute(JexlContext, Object...) and Closure.execute(JexlContext, Object...)
    pub fn execute_args(&self, context: Arc<dyn JexlContext>, args: &[Value]) -> Result<Value, JexlException> {
        let (jexl, options) = if context.get_engine_options().is_none() {
            // A PragmaProcessor still hears every pragma on every execution, as in Java.
            if context.is_pragma_processor() {
                self.engine.notify_pragmas(&self.parsed, context.as_ref());
            }
            let jexl = self
                .engine_ref
                .get_or_init(|| {
                    let options = self.engine.script_options(self.engine.options.clone(), &self.parsed, None);
                    self.engine.engine_ref(&options)
                })
                .clone();
            let options = jexl.options.clone();
            (jexl, options)
        } else {
            let options = self.engine.options_for_script(&self.parsed, context.as_ref());
            (self.engine.engine_ref(&options), options)
        };
        let ast = &self.parsed.ast;
        let mut interpreter = Interpreter::new(jexl, ast.clone(), options, context, self.local_frame(args));
        if self.frame.is_some() {
            // Closure.execute runs the lambda body, not the script
            return interpreter.run_closure(&Closure::new(ast.clone(), self.parsed.root, None));
        }
        interpreter.interpret(ast.node(self.parsed.root))
    }

    /// The frame the interpreter starts with: `frame.assign(args)` for a closure, a fresh frame
    /// from the script's own scope otherwise.
    fn local_frame(&self, args: &[Value]) -> Option<Frame> {
        let scopes = self.parsed.ast.scopes_ref();
        match &self.frame {
            Some(f) => Some(f.assign(scopes.get(f.scope()), Some(args))),
            None => self
                .parsed
                .node()
                .script()
                .and_then(|s| s.get_scope())
                .and_then(|scope| {
                    create_frame(scopes, scope, None, if args.is_empty() { None } else { Some(args) })
                }),
        }
    }

    /// port of: Script.evaluate(JexlContext) — the JexlExpression face of the same object
    pub fn evaluate(&self, context: Arc<dyn JexlContext>) -> Result<Value, JexlException> {
        self.execute(context)
    }

    /// port of: Script.callable(JexlContext)
    pub fn callable_ctx(&self, context: Arc<dyn JexlContext>) -> ScriptCallable {
        self.callable(context, &[])
    }

    /// port of: Script.callable(JexlContext, Object...)
    pub fn callable(&self, context: Arc<dyn JexlContext>, args: &[Value]) -> ScriptCallable {
        ScriptCallable {
            script: self.clone(),
            context,
            args: args.to_vec(),
            result: Mutex::new(None),
        }
    }
}

/// port of: org.apache.commons.jexl3.internal.Script.Callable
///
/// Java's Callable holds the Interpreter it was built with so `cancel()` can reach it; this one
/// holds the pieces and builds the interpreter on `call()`, which is observable only through
/// cancellation, so `cancel()` goes through the context's handle the same way the interpreter does.
pub struct ScriptCallable {
    script: JexlScript,
    context: Arc<dyn JexlContext>,
    args: Vec<Value>,
    /// Java memoizes: the second `call()` returns the first result without re-interpreting.
    result: Mutex<Option<Result<Value, JexlException>>>,
}

impl ScriptCallable {
    /// port of: Script.Callable.call
    pub fn call(&self) -> Result<Value, JexlException> {
        let mut cell = self.result.lock().unwrap_or_else(|p| p.into_inner());
        if cell.is_none() {
            *cell = Some(self.script.execute_args(self.context.clone(), &self.args));
        }
        cell.clone().expect("just computed")
    }

    /// port of: Script.Callable.cancel
    pub fn cancel(&self) -> bool {
        match self.context.get_cancellation() {
            Some(flag) => !flag.swap(true, std::sync::atomic::Ordering::SeqCst),
            None => false,
        }
    }

    /// port of: Script.Callable.isCancelled
    pub fn is_cancelled(&self) -> bool {
        self.context
            .get_cancellation()
            .map(|f| f.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(false)
    }
}

/// port of: JexlEngine.EMPTY_CONTEXT
pub fn empty_context() -> Arc<dyn JexlContext> {
    Arc::new(EmptyContext)
}

/// port of: org.apache.commons.jexl3.JexlBuilder
pub struct JexlBuilder {
    uberspect: Option<Arc<dyn JexlUberspect>>,
    sandbox: Option<crate::introspection::jexl_sandbox::JexlSandbox>,
    arithmetic: Option<JexlArithmetic>,
    features: Option<JexlFeatures>,
    options: JexlOptions,
    debug: Option<bool>,
    cancellable: Option<bool>,
    collect_mode: i32,
    cache: i32,
    cache_threshold: i32,
    stack_overflow: i32,
    namespaces: HashMap<String, Value>,
}

/// port of: JexlBuilder.CACHE_THRESHOLD
const CACHE_THRESHOLD: i32 = 64;

impl Default for JexlBuilder {
    fn default() -> Self {
        JexlBuilder::new()
    }
}

impl JexlBuilder {
    pub fn new() -> JexlBuilder {
        JexlBuilder {
            uberspect: None,
            sandbox: None,
            arithmetic: None,
            features: None,
            options: JexlOptions::new(),
            debug: None,
            cancellable: None,
            collect_mode: 1,
            cache: -1,
            cache_threshold: CACHE_THRESHOLD,
            stack_overflow: i32::MAX,
            namespaces: HashMap::new(),
        }
    }

    pub fn uberspect(mut self, u: Arc<dyn JexlUberspect>) -> Self {
        self.uberspect = Some(u);
        self
    }

    // port of: JexlBuilder.sandbox
    pub fn sandbox(mut self, box_: crate::introspection::jexl_sandbox::JexlSandbox) -> Self {
        self.sandbox = Some(box_);
        self
    }
    pub fn arithmetic(mut self, a: JexlArithmetic) -> Self {
        self.options.set_strict_arithmetic(a.is_strict());
        self.options.set_math_context(Some(*a.get_math_context()));
        self.options.set_math_scale(a.get_math_scale());
        self.arithmetic = Some(a);
        self
    }
    pub fn features(mut self, f: JexlFeatures) -> Self {
        if f.is_lexical() {
            self.options.set_lexical(true);
        }
        if f.is_lexical_shade() {
            self.options.set_lexical_shade(true);
        }
        self.features = Some(f);
        self
    }
    pub fn strict(mut self, flag: bool) -> Self {
        self.options.set_strict(flag);
        self
    }
    pub fn silent(mut self, flag: bool) -> Self {
        self.options.set_silent(flag);
        self
    }
    pub fn safe(mut self, flag: bool) -> Self {
        self.options.set_safe(flag);
        self
    }
    pub fn lexical(mut self, flag: bool) -> Self {
        self.options.set_lexical(flag);
        self
    }
    pub fn lexical_shade(mut self, flag: bool) -> Self {
        self.options.set_lexical_shade(flag);
        self
    }
    pub fn antish(mut self, flag: bool) -> Self {
        self.options.set_antish(flag);
        self
    }
    pub fn cancellable(mut self, flag: bool) -> Self {
        self.cancellable = Some(flag);
        self.options.set_cancellable(flag);
        self
    }
    pub fn debug(mut self, flag: bool) -> Self {
        self.debug = Some(flag);
        self
    }
    pub fn collect_mode(mut self, mode: i32) -> Self {
        self.collect_mode = mode;
        self
    }
    pub fn cache(mut self, size: i32) -> Self {
        self.cache = size;
        self
    }
    pub fn cache_threshold(mut self, length: i32) -> Self {
        self.cache_threshold = if length > 0 { length } else { CACHE_THRESHOLD };
        self
    }
    pub fn stack_overflow(mut self, size: i32) -> Self {
        self.stack_overflow = size;
        self
    }
    pub fn namespaces(mut self, ns: HashMap<String, Value>) -> Self {
        self.namespaces = ns.clone();
        self.options.set_namespaces(ns);
        self
    }

    /// port of: JexlBuilder.create / Engine(JexlBuilder)
    pub fn create(self) -> Arc<JexlEngine> {
        let mut options = self.options.clone();
        let strict = options.is_strict();
        let safe = options.is_safe();
        let silent = options.is_silent();
        let cancellable = self.cancellable.unwrap_or(!silent && strict);
        options.set_cancellable(cancellable);
        let debug = self.debug.unwrap_or(true);
        let stack_overflow = if self.stack_overflow > 0 { self.stack_overflow } else { i32::MAX };
        let arithmetic = self.arithmetic.unwrap_or_else(|| JexlArithmetic::new(strict, None, i32::MIN));
        options.set_math_context(Some(*arithmetic.get_math_context()));
        options.set_math_scale(arithmetic.get_math_scale());
        options.set_strict_arithmetic(arithmetic.is_strict());
        let features = self.features.unwrap_or_default();
        // Engine ORs the declared namespaces into the features' namespace test
        let ns_names: Vec<String> = self.namespaces.keys().cloned().collect();
        let features = if ns_names.is_empty() {
            features
        } else {
            let existing = features.get_namespace_test().cloned();
            features.namespace_test(Some(Arc::new(move |n: &str| {
                ns_names.iter().any(|x| x == n) || existing.as_ref().map(|t| t(n)).unwrap_or(false)
            })))
        };
        let expression_features = features.clone().script(false);
        let script_features = features.script(true);
        let uberspect = self.uberspect.unwrap_or_else(|| Arc::new(Uberspect::new()));
        // port of: Engine(JexlBuilder) — a sandbox wraps the uberspect
        let uberspect: Arc<dyn JexlUberspect> = match &self.sandbox {
            Some(sb) => Arc::new(crate::introspection::jexl_sandbox::SandboxUberspect::new(uberspect, sb)),
            None => uberspect,
        };
        Arc::new(JexlEngine {
            uberspect,
            arithmetic,
            functions: Arc::new(self.namespaces),
            stack_overflow,
            strict,
            safe,
            silent,
            cancellable,
            debug,
            script_features,
            expression_features,
            cache_size: self.cache.max(0),
            cache_threshold: self.cache_threshold,
            collect_mode: self.collect_mode,
            options,
            parser: Mutex::new(Parser::new()),
            cache: Mutex::new(SoftCache::new(self.cache.max(0) as usize)),
        })
    }
}
