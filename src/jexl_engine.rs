// port of: org.apache.commons.jexl3.JexlEngine and org.apache.commons.jexl3.internal.Engine
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::internal::engine::{get_variables_mode, pragmas_as_map};
use crate::internal::frame::create_frame;
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
    pub(crate) functions: HashMap<String, Value>,
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

/// port of: org.apache.commons.jexl3.internal.SoftCache — a bounded LRU.
struct SoftCache {
    capacity: usize,
    entries: Vec<(JexlFeatures, String, Arc<Parsed>)>,
}

impl SoftCache {
    fn new(capacity: usize) -> SoftCache {
        SoftCache { capacity, entries: Vec::new() }
    }

    fn get(&mut self, features: &JexlFeatures, src: &str) -> Option<Arc<Parsed>> {
        let pos = self.entries.iter().position(|(f, s, _)| f == features && s == src)?;
        let entry = self.entries.remove(pos);
        let parsed = entry.2.clone();
        self.entries.push(entry);
        Some(parsed)
    }

    fn put(&mut self, features: JexlFeatures, src: String, parsed: Arc<Parsed>) {
        if self.capacity == 0 {
            return;
        }
        if self.entries.len() >= self.capacity {
            self.entries.remove(0);
        }
        self.entries.push((features, src, parsed));
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

/// port of: org.apache.commons.jexl3.internal.Script
pub struct JexlScript {
    engine: Arc<JexlEngine>,
    source: Option<String>,
    parsed: Arc<Parsed>,
}

impl JexlEngine {
    /// port of: JexlEngine.createExpression(JexlInfo, String)
    pub fn create_expression(self: &Arc<Self>, info: Option<JexlInfo>, expression: &str) -> Result<JexlScript, JexlException> {
        let features = self.expression_features.clone();
        self.create_script_features(features, info, expression, None)
    }

    /// port of: JexlEngine.createScript(String)
    pub fn create_script(self: &Arc<Self>, script_text: &str) -> Result<JexlScript, JexlException> {
        let features = self.script_features.clone();
        self.create_script_features(features, None, script_text, None)
    }

    /// port of: JexlEngine.createScript(String, String...)
    pub fn create_script_named(self: &Arc<Self>, script_text: &str, names: &[String]) -> Result<JexlScript, JexlException> {
        let features = self.script_features.clone();
        self.create_script_features(features, None, script_text, Some(names))
    }

    /// port of: JexlEngine.createScript(JexlInfo, String, String...)
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
    pub fn create_script_features(
        self: &Arc<Self>,
        features: JexlFeatures,
        info: Option<JexlInfo>,
        script_text: &str,
        names: Option<&[String]>,
    ) -> Result<JexlScript, JexlException> {
        let source = crate::internal::engine::trim_source(script_text);
        let parsed = self.parse(info, &features, &source, names)?;
        Ok(JexlScript { engine: self.clone(), source: Some(source), parsed })
    }

    /// port of: Engine.parse(JexlInfo, JexlFeatures, String, Scope)
    fn parse(
        self: &Arc<Self>,
        info: Option<JexlInfo>,
        features: &JexlFeatures,
        src: &str,
        names: Option<&[String]>,
    ) -> Result<Arc<Parsed>, JexlException> {
        let cached = self.cache_size > 0 && (src.encode_utf16().count() as i32) < self.cache_threshold;
        if cached && names.is_none() {
            if let Some(hit) = self.cache.lock().unwrap_or_else(|p| p.into_inner()).get(features, src) {
                return Ok(hit);
            }
        }
        let mut parser = self.parser.lock().unwrap_or_else(|p| p.into_inner());
        let parsed = Arc::new(parser.parse(info, features, src, names)?);
        drop(parser);
        if cached && names.is_none() {
            self.cache
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .put(features.clone(), src.to_string(), parsed.clone());
        }
        Ok(parsed)
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
    fn options_for(&self, context: &dyn JexlContext) -> JexlOptions {
        match context.get_engine_options() {
            Some(o) => o,
            None => self.options.clone(),
        }
    }

    // port of: Engine.options(ASTJexlScript, JexlContext) and Engine.processPragmas
    fn options_for_script(&self, parsed: &Parsed, context: &dyn JexlContext) -> JexlOptions {
        let mut opts = self.options_for(context);
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
                                let map = ns.get_or_insert_with(|| self.functions.clone());
                                map.insert(nsname.to_string(), value.clone());
                            }
                        }
                    }
                    if context.is_pragma_processor() {
                        context.process_pragma(key, value);
                    }
                }
                if let Some(map) = ns {
                    opts.set_namespaces(map);
                }
            }
        }
        opts
    }

    fn engine_ref(&self, options: &JexlOptions) -> Arc<EngineRef> {
        Arc::new(EngineRef {
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

    /// port of: Script.execute(JexlContext, Object...)
    pub fn execute_args(&self, context: Arc<dyn JexlContext>, args: &[Value]) -> Result<Value, JexlException> {
        let options = self.engine.options_for_script(&self.parsed, context.as_ref());
        let jexl = self.engine.engine_ref(&options);
        let node = self.parsed.node();
        let frame = node
            .script()
            .and_then(|s| s.get_scope())
            .and_then(|scope| create_frame(self.parsed.ast.scopes_ref(), scope, None, if args.is_empty() { None } else { Some(args) }));
        let mut interpreter = Interpreter::new(jexl, self.parsed.ast.clone(), options, context, frame);
        let ast = self.parsed.ast.clone();
        let root = ast.node(self.parsed.root);
        interpreter.interpret(root)
    }

    /// port of: Script.evaluate(JexlContext) — the JexlExpression face of the same object
    pub fn evaluate(&self, context: Arc<dyn JexlContext>) -> Result<Value, JexlException> {
        self.execute(context)
    }
}

/// port of: JexlEngine.EMPTY_CONTEXT
pub fn empty_context() -> Arc<dyn JexlContext> {
    Arc::new(EmptyContext)
}

/// port of: org.apache.commons.jexl3.JexlBuilder
pub struct JexlBuilder {
    uberspect: Option<Arc<dyn JexlUberspect>>,
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
        Arc::new(JexlEngine {
            uberspect,
            arithmetic,
            functions: self.namespaces,
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
