// port of: org.apache.commons.jexl3.internal.TemplateEngine
//
// Java models unified expressions as five inner classes of TemplateEngine sharing an abstract
// base; the base's `source` field points at `this` when there is none. Here they are one struct
// with an `ExprKind` payload, and `source: None` means "this" (`get_source` resolves it against
// the `Arc`). The inner classes' implicit outer `this` becomes `params`, a small shared record of
// what those methods actually read from the engine -- keeping the engine out of the expressions
// also keeps the cache (engine -> expression -> engine) from being a reference cycle.
use std::sync::{Arc, Mutex, OnceLock};

use crate::internal::frame::Frame;
use crate::internal::interpreter::Interpreter;
use crate::internal::template_interpreter::{self, TemplateState};
use crate::internal::template_script::TemplateScript;
use crate::java::string::{JString, JStringBuilder};
use crate::jexl_context::JexlContext;
use crate::jexl_engine::JexlEngine;
use crate::jexl_exception::JexlException;
use crate::jexl_info::JexlInfo;
use crate::parser::jexl_node::{Ast, Parsed};
use crate::parser::string_parser;
use crate::value::Value;

/// What `TemplateEngine`'s inner classes read from their outer instance.
pub struct EngineParams {
    /// port of: TemplateEngine.jexl
    pub jexl: Arc<JexlEngine>,
    /// port of: TemplateEngine.immediateChar
    pub immediate_char: u16,
    /// port of: TemplateEngine.deferredChar
    pub deferred_char: u16,
    /// port of: TemplateEngine.noscript
    pub noscript: bool,
}

/// port of: org.apache.commons.jexl3.internal.TemplateEngine (a `JxltEngine` implementation)
pub struct TemplateEngine {
    pub(crate) params: Arc<EngineParams>,
    /// port of: TemplateEngine.cache — a SoftCache<String, TemplateExpression>
    cache: Mutex<ExprCache>,
}

/// port of: org.apache.commons.jexl3.internal.SoftCache used with String keys.
/// A LinkedHashMap in access order that evicts its eldest entry past `capacity`; a capacity of 0
/// therefore evicts whatever was just put, which is what `Engine.jxlt()` relies on.
struct ExprCache {
    capacity: usize,
    entries: Vec<(JString, Arc<TemplateExpression>)>,
}

impl ExprCache {
    fn get(&mut self, key: &JString) -> Option<Arc<TemplateExpression>> {
        let pos = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(pos);
        let v = entry.1.clone();
        self.entries.push(entry);
        Some(v)
    }

    fn put(&mut self, key: JString, value: Arc<TemplateExpression>) {
        if self.capacity == 0 {
            return;
        }
        if let Some(pos) = self.entries.iter().position(|(k, _)| *k == key) {
            self.entries.remove(pos);
        }
        self.entries.push((key, value));
        if self.entries.len() > self.capacity {
            self.entries.remove(0);
        }
    }
}


/// port of: java.lang.Number.intValue() — the narrowing `jexl:print(n)` applies to its argument.
/// Rust's `as i32` on a float saturates and maps NaN to 0, exactly like a Java `(int)` cast.
pub(crate) fn int_value(v: &Value) -> Option<i32> {
    match v {
        Value::Byte(b) => Some(*b as i32),
        Value::Short(s) => Some(*s as i32),
        Value::Integer(i) => Some(*i),
        Value::Long(l) => Some(*l as i32),
        Value::Float(f) => Some(*f as i32),
        Value::Double(d) => Some(*d as i32),
        Value::BigInteger(b) => Some(crate::java::number::big_integer_int_value(b)),
        Value::BigDecimal(b) => Some(b.int_value()),
        _ => None,
    }
}

/// port of: TemplateEngine.ExpressionType
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpressionType {
    /// Constant TemplateExpression, count index 0.
    Constant,
    /// Immediate TemplateExpression, count index 1.
    Immediate,
    /// Deferred TemplateExpression, count index 2.
    Deferred,
    /// Nested (which are deferred) expressions, count index 2.
    Nested,
    /// Composite expressions are not counted, index -1.
    Composite,
}

impl ExpressionType {
    fn index(self) -> Option<usize> {
        match self {
            ExpressionType::Constant => Some(0),
            ExpressionType::Immediate => Some(1),
            ExpressionType::Deferred | ExpressionType::Nested => Some(2),
            ExpressionType::Composite => None,
        }
    }
}

/// The payload of each of TemplateEngine's five TemplateExpression subclasses.
pub(crate) enum ExprKind {
    /// port of: TemplateEngine.ConstantExpression
    Constant { value: Value },
    /// port of: TemplateEngine.ImmediateExpression — `${jexl}`
    Immediate { expr: JString, node: Arc<Parsed> },
    /// port of: TemplateEngine.DeferredExpression — `#{jexl}`
    Deferred { expr: JString, node: Arc<Parsed> },
    /// port of: TemplateEngine.NestedExpression — `#{...${jexl}...}`
    Nested { expr: JString, node: Arc<Parsed> },
    /// port of: TemplateEngine.CompositeExpression
    Composite { meta: i32, exprs: Vec<Arc<TemplateExpression>> },
}

/// port of: TemplateEngine.TemplateExpression (the `JxltEngine.Expression` implementation)
pub struct TemplateExpression {
    pub(crate) params: Arc<EngineParams>,
    /// port of: TemplateExpression.source — `None` is Java's `this`
    pub(crate) source: Option<Arc<TemplateExpression>>,
    pub(crate) kind: ExprKind,
}

/// The tree a `TemplateExpression` with no JEXL node of its own hands an interpreter: nothing is
/// ever looked up in it (a constant never interprets), it only satisfies `Interpreter`'s arena.
fn empty_ast() -> Arc<Ast> {
    static EMPTY: OnceLock<Arc<Ast>> = OnceLock::new();
    EMPTY.get_or_init(|| Arc::new(Ast::new())).clone()
}

// ------------------------------------------------------------------------------ the expressions

impl TemplateExpression {
    fn new(params: Arc<EngineParams>, source: Option<Arc<TemplateExpression>>, kind: ExprKind) -> Arc<TemplateExpression> {
        Arc::new(TemplateExpression { params, source, kind })
    }

    /// port of: ConstantExpression(Object, TemplateExpression) — a String constant is read as a
    /// JEXL string with respect to escaping.
    pub(crate) fn constant(
        params: Arc<EngineParams>,
        value: Value,
        source: Option<Arc<TemplateExpression>>,
    ) -> Arc<TemplateExpression> {
        let value = match &value {
            Value::String(s) => Value::String(JString::new(string_parser::build_template(s.units(), false))),
            other => other.clone(),
        };
        Self::new(params, source, ExprKind::Constant { value })
    }

    // port of: TemplateExpression.getType
    pub fn get_type(&self) -> ExpressionType {
        match &self.kind {
            ExprKind::Constant { .. } => ExpressionType::Constant,
            ExprKind::Immediate { .. } => ExpressionType::Immediate,
            ExprKind::Deferred { .. } => ExpressionType::Deferred,
            ExprKind::Nested { .. } => ExpressionType::Nested,
            ExprKind::Composite { .. } => ExpressionType::Composite,
        }
    }

    // port of: TemplateExpression.isImmediate
    pub fn is_immediate(&self) -> bool {
        match &self.kind {
            ExprKind::Deferred { .. } | ExprKind::Nested { .. } => false,
            // immediate if no deferred
            ExprKind::Composite { meta, .. } => (meta & 2) == 0,
            _ => true,
        }
    }

    // port of: TemplateExpression.isDeferred
    pub fn is_deferred(&self) -> bool {
        !self.is_immediate()
    }

    /// port of: CompositeExpression.exprs — the parts `TemplateInterpreter.printComposite` walks.
    pub(crate) fn composite_parts(&self) -> Option<&[Arc<TemplateExpression>]> {
        match &self.kind {
            ExprKind::Composite { exprs, .. } => Some(exprs),
            _ => None,
        }
    }

    /// The JEXL tree of a `${...}` / `#{...}` expression.
    pub(crate) fn node(&self) -> Option<&Arc<Parsed>> {
        match &self.kind {
            ExprKind::Immediate { node, .. } | ExprKind::Deferred { node, .. } | ExprKind::Nested { node, .. } => Some(node),
            _ => None,
        }
    }

    /// The tree an interpreter built for this expression should start on.
    fn own_ast(&self) -> Arc<Ast> {
        match &self.kind {
            ExprKind::Immediate { node, .. } | ExprKind::Deferred { node, .. } | ExprKind::Nested { node, .. } => node.ast.clone(),
            ExprKind::Composite { exprs, .. } => exprs.iter().find_map(|e| e.node().map(|n| n.ast.clone())).unwrap_or_else(empty_ast),
            ExprKind::Constant { .. } => empty_ast(),
        }
    }

    // port of: TemplateExpression.asString(StringBuilder)
    pub fn as_string_into(&self, strb: &mut JStringBuilder) {
        match &self.kind {
            ExprKind::Constant { value } => {
                if !value.is_null() {
                    strb.jstr(&value.java_to_jstring());
                }
            }
            ExprKind::Immediate { expr, .. } | ExprKind::Deferred { expr, .. } => {
                let c = if self.is_immediate() { self.params.immediate_char } else { self.params.deferred_char };
                strb.units(&[c]).str("{").jstr(expr).str("}");
            }
            // NestedExpression keeps the whole `#{...${...}...}` source
            ExprKind::Nested { expr, .. } => {
                strb.jstr(expr);
            }
            ExprKind::Composite { exprs, .. } => {
                for e in exprs {
                    e.as_string_into(strb);
                }
            }
        }
    }

    // port of: TemplateExpression.asString
    pub fn as_string(&self) -> JString {
        let mut strb = JStringBuilder::new();
        self.as_string_into(&mut strb);
        strb.build()
    }

    // port of: TemplateExpression.toString
    pub fn java_to_jstring(self: &Arc<Self>) -> JString {
        let mut strb = JStringBuilder::new();
        self.as_string_into(&mut strb);
        if let Some(src) = &self.source {
            strb.str(" /*= ").jstr(&src.java_to_jstring()).str(" */");
        }
        strb.build()
    }

    // port of: TemplateExpression.getSource
    pub fn get_source(self: &Arc<Self>) -> Arc<TemplateExpression> {
        self.source.clone().unwrap_or_else(|| self.clone())
    }

    // port of: TemplateExpression.getInfo
    pub(crate) fn get_info(&self) -> Option<JexlInfo> {
        self.node().and_then(|p| p.node().jexl_info())
    }

    // port of: TemplateExpression.getVariables
    pub fn get_variables(&self) -> Vec<Vec<JString>> {
        let mut out: Vec<Vec<JString>> = Vec::new();
        self.collect_variables(&mut out);
        out
    }

    /// port of: TemplateExpression.getVariables(Engine.VarCollector). Java shares one collector
    /// (a LinkedHashSet) across a composite's parts; merging per part with the same de-duplication
    /// is equivalent, because the walker always flushes the collector before it returns.
    pub(crate) fn collect_variables(&self, out: &mut Vec<Vec<JString>>) {
        match &self.kind {
            // DeferredExpression.getVariables(collector) is a no-op; a constant has none
            ExprKind::Constant { .. } | ExprKind::Deferred { .. } => {}
            ExprKind::Immediate { node, .. } | ExprKind::Nested { node, .. } => {
                for r in crate::internal::engine::get_variables_mode(node, self.params.jexl.collect_mode) {
                    if !out.contains(&r) {
                        out.push(r);
                    }
                }
            }
            ExprKind::Composite { exprs, .. } => {
                for e in exprs {
                    e.collect_variables(out);
                }
            }
        }
    }

    /// port of: TemplateExpression.options(JexlContext)
    fn options(&self, context: &dyn JexlContext) -> crate::jexl_options::JexlOptions {
        match self.node() {
            // JexlBasedExpression: the node is always an ASTJexlScript, so its pragmas apply
            Some(p) => self.params.jexl.options_for_script(p, context),
            None => self.params.jexl.options_no_script(context),
        }
    }

    // port of: TemplateExpression.prepare(JexlContext)
    pub fn prepare(
        self: &Arc<Self>,
        context: Arc<dyn JexlContext>,
    ) -> Result<Option<Arc<TemplateExpression>>, JexlException> {
        self.prepare_frame(empty_ast(), None, context)
    }

    /// port of: TemplateExpression.prepare(Frame, JexlContext). `ast` is the tree `frame`'s
    /// symbols belong to (Java's Frame carries its Scope object directly).
    pub(crate) fn prepare_frame(
        self: &Arc<Self>,
        ast: Arc<Ast>,
        frame: Option<Frame>,
        context: Arc<dyn JexlContext>,
    ) -> Result<Option<Arc<TemplateExpression>>, JexlException> {
        let options = self.params.jexl.options_for(context.as_ref());
        let jexl = self.params.jexl.engine_ref(&options);
        let mut interpreter = Interpreter::new(jexl, ast, options, context, frame);
        match self.prepare_in(&mut interpreter) {
            Ok(v) => Ok(v),
            Err(e) if e.is_jexl() => {
                let xuel = create_exception(e.get_info(), "prepare", Some(self), &e);
                if self.params.jexl.is_silent() {
                    Ok(None)
                } else {
                    Err(xuel)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// port of: TemplateExpression.prepare(Interpreter)
    pub(crate) fn prepare_in(
        self: &Arc<Self>,
        interpreter: &mut Interpreter,
    ) -> Result<Option<Arc<TemplateExpression>>, JexlException> {
        match &self.kind {
            // the base implementation: prepared expressions are themselves
            ExprKind::Constant { .. } => Ok(Some(self.clone())),
            // ImmediateExpression: evaluate immediate as constant
            ExprKind::Immediate { .. } => {
                let value = self.evaluate_in(interpreter)?;
                Ok(if value.is_null() {
                    None
                } else {
                    Some(TemplateExpression::constant(self.params.clone(), value, Some(self.get_source())))
                })
            }
            // DeferredExpression: becomes immediate
            ExprKind::Deferred { expr, node } => Ok(Some(TemplateExpression::new(
                self.params.clone(),
                Some(self.get_source()),
                ExprKind::Immediate { expr: expr.clone(), node: node.clone() },
            ))),
            // NestedExpression: interpret, re-parse the result, become immediate over it
            ExprKind::Nested { node, .. } => {
                let value = template_interpreter::interpret_parsed(interpreter, node)?;
                if value.is_null() {
                    // Java calls toString() on the result
                    return Err(JexlException::java(
                        "java.lang.NullPointerException",
                        Some("Cannot invoke \"Object.toString()\" because the return value of \"org.apache.commons.jexl3.internal.Interpreter.interpret(org.apache.commons.jexl3.parser.JexlNode)\" is null".into()),
                    ));
                }
                let text = value.java_to_jstring();
                let dnode = self.params.jexl.parse_jxlt(
                    node.node().jexl_info(),
                    self.params.noscript,
                    &text.to_rust(),
                    None,
                )?;
                Ok(Some(TemplateExpression::new(
                    self.params.clone(),
                    Some(self.clone()),
                    ExprKind::Immediate { expr: text, node: dnode },
                )))
            }
            ExprKind::Composite { exprs, .. } => {
                // if this composite is not its own source, it is already prepared
                if self.source.is_some() {
                    return Ok(Some(self.clone()));
                }
                let mut builder = ExpressionBuilder::new(exprs.len());
                let mut eq = true;
                for expr in exprs {
                    let prepared = expr.prepare_in(interpreter)?;
                    match &prepared {
                        Some(p) => {
                            eq &= Arc::ptr_eq(expr, p);
                            builder.add(p.clone());
                        }
                        None => eq = false,
                    }
                }
                Ok(Some(if eq { self.clone() } else { builder.build(&self.params, Some(self.clone())) }))
            }
        }
    }

    // port of: TemplateExpression.evaluate(JexlContext)
    pub fn evaluate(self: &Arc<Self>, context: Arc<dyn JexlContext>) -> Result<Value, JexlException> {
        self.evaluate_frame(self.own_ast(), None, context)
    }

    /// port of: TemplateExpression.evaluate(Frame, JexlContext)
    pub(crate) fn evaluate_frame(
        self: &Arc<Self>,
        ast: Arc<Ast>,
        frame: Option<Frame>,
        context: Arc<dyn JexlContext>,
    ) -> Result<Value, JexlException> {
        let options = self.options(context.as_ref());
        let jexl = self.params.jexl.engine_ref(&options);
        let mut interpreter = template_interpreter::new_template_interpreter(
            jexl,
            ast,
            options,
            context,
            frame,
            Arc::new(TemplateState { exprs: None, writer: None }),
        );
        match self.evaluate_in(&mut interpreter) {
            Ok(v) => Ok(v),
            Err(e) if e.is_jexl() => {
                let xuel = create_exception(e.get_info(), "evaluate", Some(self), &e);
                if self.params.jexl.is_silent() {
                    Ok(Value::Null)
                } else {
                    Err(xuel)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// port of: TemplateExpression.evaluate(Interpreter)
    pub(crate) fn evaluate_in(self: &Arc<Self>, interpreter: &mut Interpreter) -> Result<Value, JexlException> {
        match &self.kind {
            ExprKind::Constant { value } => Ok(value.clone()),
            ExprKind::Immediate { node, .. } | ExprKind::Deferred { node, .. } => {
                template_interpreter::interpret_parsed(interpreter, node)
            }
            ExprKind::Nested { .. } => match self.prepare_in(interpreter)? {
                Some(p) => p.evaluate_in(interpreter),
                None => Ok(Value::Null),
            },
            ExprKind::Composite { exprs, .. } => {
                // common case: evaluate all expressions & concatenate them as a string
                let mut strb = JStringBuilder::new();
                for e in exprs {
                    let value = e.evaluate_in(interpreter)?;
                    if !value.is_null() {
                        strb.jstr(&value.java_to_jstring());
                    }
                }
                Ok(Value::String(strb.build()))
            }
        }
    }
}

/// port of: TemplateEngine.ExpressionBuilder
struct ExpressionBuilder {
    counts: [i32; 3],
    expressions: Vec<Arc<TemplateExpression>>,
}

impl ExpressionBuilder {
    fn new(size: usize) -> ExpressionBuilder {
        ExpressionBuilder { counts: [0, 0, 0], expressions: Vec::with_capacity(if size == 0 { 3 } else { size }) }
    }

    // port of: ExpressionBuilder.add
    fn add(&mut self, expr: Arc<TemplateExpression>) {
        if let Some(i) = expr.get_type().index() {
            self.counts[i] += 1;
        }
        self.expressions.push(expr);
    }

    /// port of: ExpressionBuilder.build — the "parsing algorithm error" check cannot fire (only
    /// counted types are ever added), so only the two real outcomes are ported.
    fn build(mut self, params: &Arc<EngineParams>, source: Option<Arc<TemplateExpression>>) -> Arc<TemplateExpression> {
        // if only one sub-expr, no need to create a composite
        if self.expressions.len() == 1 {
            return self.expressions.remove(0);
        }
        let meta = (if self.counts[2] > 0 { 2 } else { 0 }) | (if self.counts[1] > 0 { 1 } else { 0 });
        TemplateExpression::new(params.clone(), source, ExprKind::Composite { meta, exprs: self.expressions })
    }
}

/// port of: TemplateEngine.createException(JexlInfo, String, TemplateExpression, Exception)
pub(crate) fn create_exception(
    info: Option<JexlInfo>,
    action: &str,
    expr: Option<&Arc<TemplateExpression>>,
    xany: &JexlException,
) -> JexlException {
    let mut strb = JStringBuilder::new();
    strb.str("failed to ").str(action);
    if let Some(e) = expr {
        strb.str(" '").jstr(&e.java_to_jstring()).str("'");
    }
    if let Some(cause) = xany.get_cause() {
        if let Some(msg) = cause.get_message() {
            strb.str(", ").jstr(&msg);
        }
    }
    JexlException::jxlt_msg(info, &strb.build(), Some(xany.clone()))
}

// -------------------------------------------------------------------------------- the parser

/// port of: TemplateEngine.ParseState
#[derive(Clone, Copy, PartialEq, Eq)]
enum ParseState {
    Const,
    Immediate0,
    Deferred0,
    Immediate1,
    Deferred1,
    Escape,
}

/// port of: TemplateEngine.append — copies `c`, and the rest of an embedded string if `c` opens one
fn append(strb: &mut Vec<u16>, expr: &[u16], position: usize, c: u16) -> usize {
    strb.push(c);
    if c != b'"' as u16 && c != b'\'' as u16 {
        return position;
    }
    // read thru strings
    let end = expr.len();
    let mut escape = false;
    let mut index = position + 1;
    while index < end {
        let ec = expr[index];
        strb.push(ec);
        if ec == b'\\' as u16 {
            escape = !escape;
        } else if escape {
            escape = false;
        } else if ec == c {
            break;
        }
        index += 1;
    }
    index
}

impl TemplateEngine {
    /// port of: TemplateEngine(Engine, boolean, int, char, char)
    pub fn new(jexl: Arc<JexlEngine>, noscript: bool, cache_size: i32, immediate: char, deferred: char) -> TemplateEngine {
        TemplateEngine {
            params: Arc::new(EngineParams {
                jexl,
                immediate_char: immediate as u32 as u16,
                deferred_char: deferred as u32 as u16,
                noscript,
            }),
            cache: Mutex::new(ExprCache { capacity: cache_size.max(0) as usize, entries: Vec::new() }),
        }
    }

    // port of: TemplateEngine.getEngine
    pub fn get_engine(&self) -> &Arc<JexlEngine> {
        &self.params.jexl
    }

    // port of: TemplateEngine.getImmediateChar
    pub fn get_immediate_char(&self) -> u16 {
        self.params.immediate_char
    }

    // port of: TemplateEngine.getDeferredChar
    pub fn get_deferred_char(&self) -> u16 {
        self.params.deferred_char
    }

    // port of: TemplateEngine.clearCache
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap_or_else(|p| p.into_inner()).entries.clear();
    }

    /// port of: TemplateEngine.createExpression(JexlInfo, String).
    /// `Ok(None)` is Java's `null`: a silent engine logs the failure instead of throwing.
    pub fn create_expression(
        self: &Arc<Self>,
        info: Option<JexlInfo>,
        expression: &JString,
    ) -> Result<Option<Arc<TemplateExpression>>, JexlException> {
        let info = info.unwrap_or_else(|| self.params.jexl.create_info());
        let cached = self.cache.lock().unwrap_or_else(|p| p.into_inner()).get(expression);
        if let Some(stmt) = cached {
            return Ok(Some(stmt));
        }
        match self.parse_expression(&info, expression, None) {
            Ok(stmt) => {
                self.cache.lock().unwrap_or_else(|p| p.into_inner()).put(expression.clone(), stmt.clone());
                Ok(Some(stmt))
            }
            Err(xjexl) if xjexl.is_jexl() => {
                let mut msg = JStringBuilder::new();
                msg.str("failed to parse '").jstr(expression).str("'");
                let xuel = JexlException::jxlt_msg(xjexl.get_info(), &msg.build(), Some(xjexl));
                if self.params.jexl.is_silent() {
                    Ok(None)
                } else {
                    Err(xuel)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// port of: TemplateEngine.parseExpression(JexlInfo, String, Scope)
    pub fn parse_expression(
        self: &Arc<Self>,
        info: &JexlInfo,
        expr: &JString,
        scope: Option<(&crate::internal::scope::Scopes, crate::internal::scope::ScopeId)>,
    ) -> Result<Arc<TemplateExpression>, JexlException> {
        let params = &self.params;
        let immediate_char = params.immediate_char;
        let deferred_char = params.deferred_char;
        let src_units = expr.units();
        let size = src_units.len();
        let mut builder = ExpressionBuilder::new(0);
        let mut strb: Vec<u16> = Vec::with_capacity(size);
        let mut state = ParseState::Const;
        let mut immediate1 = 0i32;
        let mut deferred1 = 0i32;
        let mut inner1 = 0i32;
        let mut nested = false;
        let mut inested: i32 = -1;
        let mut lineno = info.get_line();
        let lf = b'\n' as u16;
        let lbrace = b'{' as u16;
        let rbrace = b'}' as u16;
        let bslash = b'\\' as u16;
        let mut column = 0usize;
        'scan: while column < size {
            let c = src_units[column];
            match state {
                ParseState::Const => {
                    if c == immediate_char {
                        state = ParseState::Immediate0;
                    } else if c == deferred_char {
                        inested = column as i32;
                        state = ParseState::Deferred0;
                    } else if c == bslash {
                        state = ParseState::Escape;
                    } else {
                        strb.push(c);
                    }
                }
                ParseState::Immediate0 => {
                    if c == lbrace {
                        state = ParseState::Immediate1;
                        if !strb.is_empty() {
                            builder.add(TemplateExpression::constant(
                                params.clone(),
                                Value::String(JString::from_units(&strb)),
                                None,
                            ));
                            strb.clear();
                        }
                    } else {
                        strb.push(immediate_char);
                        strb.push(c);
                        state = ParseState::Const;
                    }
                }
                ParseState::Deferred0 => {
                    if c == lbrace {
                        state = ParseState::Deferred1;
                        if !strb.is_empty() {
                            builder.add(TemplateExpression::constant(
                                params.clone(),
                                Value::String(JString::from_units(&strb)),
                                None,
                            ));
                            strb.clear();
                        }
                    } else {
                        strb.push(deferred_char);
                        strb.push(c);
                        state = ParseState::Const;
                    }
                }
                ParseState::Immediate1 => {
                    if c == rbrace {
                        if immediate1 > 0 {
                            immediate1 -= 1;
                            strb.push(c);
                        } else {
                            // materialize the immediate expr
                            let src = JString::from_units(&strb);
                            let node = params.jexl.parse_jxlt(
                                Some(info.at(lineno, column as i32)),
                                params.noscript,
                                &src.to_rust(),
                                scope,
                            )?;
                            builder.add(TemplateExpression::new(
                                params.clone(),
                                None,
                                ExprKind::Immediate { expr: src, node },
                            ));
                            strb.clear();
                            state = ParseState::Const;
                        }
                    } else {
                        if c == lbrace {
                            immediate1 += 1;
                        }
                        column = append(&mut strb, src_units, column, c);
                    }
                }
                ParseState::Deferred1 => {
                    // skip inner strings (for '}')
                    if c == b'"' as u16 || c == b'\'' as u16 {
                        strb.push(c);
                        column = string_parser::read_string(&mut strb, src_units, column + 1, c);
                        column += 1;
                        continue 'scan;
                    }
                    // nested immediate in deferred; need to balance count of '{' & '}'
                    if c == lbrace {
                        if column > 0 && src_units[column - 1] == immediate_char {
                            inner1 += 1;
                            strb.pop();
                            nested = true;
                        } else {
                            deferred1 += 1;
                            strb.push(c);
                        }
                        column += 1;
                        continue 'scan;
                    }
                    if c == rbrace {
                        // balance nested immediate
                        if deferred1 > 0 {
                            deferred1 -= 1;
                            strb.push(c);
                        } else if inner1 > 0 {
                            inner1 -= 1;
                        } else {
                            // materialize the nested/deferred expr
                            let src = JString::from_units(&strb);
                            let node = params.jexl.parse_jxlt(
                                Some(info.at(lineno, column as i32)),
                                params.noscript,
                                &src.to_rust(),
                                scope,
                            )?;
                            let dexpr = if nested {
                                TemplateExpression::new(
                                    params.clone(),
                                    None,
                                    ExprKind::Nested {
                                        expr: JString::from_units(&src_units[inested.max(0) as usize..column + 1]),
                                        node,
                                    },
                                )
                            } else {
                                TemplateExpression::new(params.clone(), None, ExprKind::Deferred { expr: src, node })
                            };
                            builder.add(dexpr);
                            strb.clear();
                            nested = false;
                            state = ParseState::Const;
                        }
                    } else {
                        column = append(&mut strb, src_units, column, c);
                    }
                }
                ParseState::Escape => {
                    if c == deferred_char {
                        strb.push(deferred_char);
                    } else if c == immediate_char {
                        strb.push(immediate_char);
                    } else {
                        strb.push(bslash);
                        strb.push(c);
                    }
                    state = ParseState::Const;
                }
            }
            if c == lf {
                lineno += 1;
            }
            column += 1;
        }
        // we should be in that state
        match state {
            ParseState::Const => {}
            // otherwise, we ended a line with a \, $ or #
            ParseState::Escape => {
                strb.push(bslash);
                strb.push(bslash);
            }
            ParseState::Deferred0 => strb.push(deferred_char),
            ParseState::Immediate0 => strb.push(immediate_char),
            _ => {
                let mut msg = JStringBuilder::new();
                msg.str("malformed expression: ").jstr(expr);
                return Err(JexlException::jxlt_msg(Some(info.at(lineno, 0)), &msg.build(), None));
            }
        }
        // if any chars were buffered, add them as a constant
        if !strb.is_empty() {
            builder.add(TemplateExpression::constant(params.clone(), Value::String(JString::from_units(&strb)), None));
        }
        Ok(builder.build(params, None))
    }

    /// port of: TemplateEngine.createTemplate(JexlInfo, String, Reader, String...)
    pub fn create_template(
        self: &Arc<Self>,
        info: Option<JexlInfo>,
        prefix: &str,
        source: &JString,
        parms: Option<&[String]>,
    ) -> Result<Arc<TemplateScript>, JexlException> {
        TemplateScript::new(self, info, prefix, source, parms)
    }
}

// -------------------------------------------------------------------------------- template blocks

/// port of: TemplateEngine.BlockType
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockType {
    /// Block is to be output "as is" but may be a unified expression.
    Verbatim,
    /// Block is a directive, ie a fragment of JEXL code.
    Directive,
}

/// port of: TemplateEngine.Block
#[derive(Clone, Debug)]
pub struct Block {
    pub(crate) block_type: BlockType,
    pub(crate) line: i32,
    pub(crate) body: JString,
}

impl Block {
    pub fn get_type(&self) -> BlockType {
        self.block_type
    }
    pub fn get_line(&self) -> i32 {
        self.line
    }
    pub fn get_body(&self) -> &JString {
        &self.body
    }

    /// port of: Block.toString(StringBuilder, String) — a directive gets its prefix per line
    pub(crate) fn to_string_into(&self, strb: &mut JStringBuilder, prefix: &str) {
        if self.block_type == BlockType::Verbatim {
            strb.jstr(&self.body);
        } else {
            for line in read_lines(self.body.units()) {
                strb.str(prefix).units(line);
            }
        }
    }
}

/// port of: TemplateEngine.readLines — keeps all new-lines and line-feeds
pub(crate) fn read_lines(units: &[u16]) -> Vec<&[u16]> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, &c) in units.iter().enumerate() {
        if c == b'\n' as u16 {
            out.push(&units[start..=i]);
            start = i + 1;
        }
    }
    if start < units.len() {
        out.push(&units[start..]);
    }
    out
}

impl TemplateEngine {
    /// port of: TemplateEngine.startsWith — the first position after `pattern`, or -1
    pub(crate) fn starts_with(sequence: &[u16], pattern: &[u16]) -> i32 {
        let length = sequence.len();
        let mut s = 0usize;
        while s < length && crate::internal::engine::is_space_char(sequence[s]) {
            s += 1;
        }
        if s < length && pattern.len() <= (length - s) {
            let seq = &sequence[s..length];
            if seq[..pattern.len()] == *pattern {
                return (s + pattern.len()) as i32;
            }
        }
        -1
    }

    /// port of: TemplateEngine.readTemplate(String, Reader)
    pub(crate) fn read_template(&self, prefix: &str, source: &JString) -> Vec<Block> {
        let pattern: Vec<u16> = prefix.encode_utf16().collect();
        let mut blocks: Vec<Block> = Vec::new();
        let mut strb: Vec<u16> = Vec::new();
        let mut block_type: Option<BlockType> = None;
        let mut lineno = 1i32;
        let mut start = 0i32;
        for line in read_lines(source.units()) {
            let prefix_len = Self::starts_with(line, &pattern);
            match block_type {
                None => {
                    if prefix_len >= 0 {
                        block_type = Some(BlockType::Directive);
                        strb.extend_from_slice(&line[prefix_len as usize..]);
                    } else {
                        block_type = Some(BlockType::Verbatim);
                        strb.extend_from_slice(line);
                    }
                    start = lineno;
                }
                Some(BlockType::Directive) => {
                    if prefix_len < 0 {
                        blocks.push(Block {
                            block_type: BlockType::Directive,
                            line: start,
                            body: JString::from_units(&strb),
                        });
                        strb.clear();
                        block_type = Some(BlockType::Verbatim);
                        strb.extend_from_slice(line);
                        start = lineno;
                    } else {
                        strb.extend_from_slice(&line[prefix_len as usize..]);
                    }
                }
                Some(BlockType::Verbatim) => {
                    if prefix_len >= 0 {
                        blocks.push(Block {
                            block_type: BlockType::Verbatim,
                            line: start,
                            body: JString::from_units(&strb),
                        });
                        strb.clear();
                        block_type = Some(BlockType::Directive);
                        strb.extend_from_slice(&line[prefix_len as usize..]);
                        start = lineno;
                    } else {
                        strb.extend_from_slice(line);
                    }
                }
            }
            lineno += 1;
        }
        // input may be null
        if let Some(t) = block_type {
            if !strb.is_empty() {
                blocks.push(Block { block_type: t, line: start, body: JString::from_units(&strb) });
            }
        }
        blocks
    }
}
