// port of: org.apache.commons.jexl3.internal.Interpreter and InterpreterBase
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use crate::internal::builders::{ArrayBuilder, MapBuilder, SetBuilder};
use crate::internal::frame::{create_frame, Frame, Slot};
use crate::internal::lexical_frame::LexicalFrame;
use crate::internal::operators::{is_try_failed, Operators};
use crate::internal::script::Closure;
use crate::introspection::JexlUberspect;
use crate::java::string::JString;
use crate::jexl_arithmetic::{ArithError, JexlArithmetic};
use crate::jexl_context::JexlContext;
use crate::jexl_exception::{ExceptionKind, JexlException, VariableIssue};
use crate::jexl_operator::JexlOperator;
use crate::jexl_options::JexlOptions;
use crate::parser::ast_identifier::ASTIdentifier;
use crate::parser::jexl_node::{Ast, NodeHandle, NodeId, NodeRef};
use crate::parser::parser_tree_constants::*;
use crate::value::{HostObject, Value};

/// port of: JexlEngine.TRY_FAILED — an identity sentinel meaning "the cached callable did not apply"
#[derive(Debug)]
pub struct TryFailed;

impl HostObject for TryFailed {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.JexlEngine$FailObject".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some("tryExecute failed".into())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

static TRY_FAILED_CELL: OnceLock<Value> = OnceLock::new();

/// The single TRY_FAILED instance (Java compares it by identity).
#[allow(non_snake_case)]
pub fn TRY_FAILED_VALUE() -> &'static Value {
    TRY_FAILED_CELL.get_or_init(|| Value::object(TryFailed))
}

/// Convenience so the ported code reads like Java's `JexlEngine.TRY_FAILED`.
pub struct TryFailedRef;

impl std::ops::Deref for TryFailedRef {
    type Target = Value;
    fn deref(&self) -> &Value {
        TRY_FAILED_VALUE()
    }
}

pub const TRY_FAILED: TryFailedRef = TryFailedRef;

/// The engine state an interpreter needs (the fields `Engine` exposes to `InterpreterBase`).
pub struct EngineRef {
    /// the engine itself: `Engine.jxlt()` needs the parser and the features, which the fields
    /// below do not carry (`Interpreter.jexl` is the Engine in Java).
    pub engine: Arc<crate::jexl_engine::JexlEngine>,
    pub uberspect: Arc<dyn JexlUberspect>,
    pub arithmetic: JexlArithmetic,
    pub functions: Arc<HashMap<String, Value>>,
    pub cache: bool,
    pub stack_overflow: i32,
    pub strict: bool,
    pub safe: bool,
    pub silent: bool,
    pub cancellable: bool,
    pub debug: bool,
    pub options: JexlOptions,
}

/// port of: Interpreter (the evaluator) with InterpreterBase folded in
pub struct Interpreter {
    pub(crate) jexl: Arc<EngineRef>,
    pub(crate) uberspect: Arc<dyn JexlUberspect>,
    pub(crate) arithmetic: JexlArithmetic,
    pub(crate) context: Arc<dyn JexlContext>,
    pub(crate) options: JexlOptions,
    pub(crate) cache: bool,
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) functions: Arc<HashMap<String, Value>>,
    pub(crate) ast: Arc<Ast>,
    pub(crate) frame: Option<Frame>,
    pub(crate) block: Option<LexicalFrame>,
    /// the interpreter nesting depth (Interpreter.fp)
    pub(crate) fp: i32,
    /// port of: TemplateInterpreter — Java subclasses Interpreter to add `jexl:print`,
    /// `jexl:include` and `$jexl`; this port carries the subclass state instead.
    /// See `crate::internal::template_interpreter`.
    pub(crate) tmpl: Option<Arc<crate::internal::template_interpreter::TemplateState>>,
}

type R = Result<Value, JexlException>;

impl Interpreter {
    // port of: Interpreter(Engine, JexlOptions, JexlContext, Frame)
    pub fn new(
        jexl: Arc<EngineRef>,
        ast: Arc<Ast>,
        options: JexlOptions,
        context: Arc<dyn JexlContext>,
        frame: Option<Frame>,
    ) -> Interpreter {
        let arithmetic = jexl.arithmetic.with_options(
            options.is_strict_arithmetic(),
            options.get_math_context().copied(),
            options.get_math_scale(),
        );
        let cancelled = context.get_cancellation().unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        let functions = if options.get_namespaces().is_empty() {
            jexl.functions.clone()
        } else {
            options.shared_namespaces()
        };
        Interpreter {
            uberspect: jexl.uberspect.clone(),
            cache: jexl.cache,
            jexl,
            arithmetic,
            context,
            options,
            cancelled,
            functions,
            ast,
            frame,
            block: None,
            fp: 0,
            tmpl: None,
        }
    }

    /// `accept` for the TemplateInterpreter overrides (see `internal::template_interpreter`)
    pub(crate) fn accept_node(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        self.accept(node, data)
    }

    /// `cancelCheck` for the TemplateInterpreter overrides
    pub(crate) fn cancel_check_node(&mut self, node: NodeRef<'_>) -> Result<(), JexlException> {
        self.cancel_check(node)
    }

    pub(crate) fn handle(&self, node: NodeRef<'_>) -> NodeHandle {
        NodeHandle::new(self.ast.clone(), node.id)
    }

    #[allow(dead_code)] // the Ast accessor the visitors mirror; kept beside node_ref for symmetry
    fn node(&self, id: NodeId) -> NodeRef<'_> {
        self.ast.node(id)
    }

    // port of: InterpreterBase.isStrictEngine
    fn is_strict_engine(&self) -> bool {
        self.options.is_strict()
    }
    // port of: InterpreterBase.isSafe
    fn is_safe(&self) -> bool {
        self.options.is_safe()
    }
    // port of: InterpreterBase.isSilent
    fn is_silent(&self) -> bool {
        self.options.is_silent()
    }
    // port of: InterpreterBase.isCancellable
    fn is_cancellable(&self) -> bool {
        self.options.is_cancellable()
    }
    // port of: InterpreterBase.isCancelled
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
    // port of: InterpreterBase.cancelCheck
    fn cancel_check(&self, node: NodeRef<'_>) -> Result<(), JexlException> {
        if self.is_cancelled() {
            return Err(JexlException::cancel(Some(self.handle(node))));
        }
        Ok(())
    }

    /// Wraps an arithmetic throwable as the Java exception object it is.
    pub(crate) fn arith_exception(&self, _node: NodeRef<'_>, e: ArithError) -> JexlException {
        match e {
            ArithError::NullOperand => JexlException::java_msg("JexlArithmetic$NullOperand", None),
            other => JexlException::java_msg(other.class_name(), other.message()),
        }
    }

    // port of: InterpreterBase.variableError
    fn variable_error(&self, node: NodeRef<'_>, var: &JString, issue: VariableIssue) -> R {
        if self.is_strict_engine() && !node.is_ternary_protected() {
            return Err(JexlException::variable(Some(self.handle(node)), var, issue));
        }
        Ok(Value::Null)
    }

    // port of: InterpreterBase.unsolvableVariable
    fn unsolvable_variable(&self, node: NodeRef<'_>, var: &JString, undef: bool) -> R {
        self.variable_error(node, var, if undef { VariableIssue::Undefined } else { VariableIssue::NullValue })
    }

    // port of: InterpreterBase.undefinedVariable
    fn undefined_variable(&self, node: NodeRef<'_>, var: &JString) -> R {
        self.variable_error(node, var, VariableIssue::Undefined)
    }

    // port of: InterpreterBase.redefinedVariable
    fn redefined_variable(&self, node: NodeRef<'_>, var: &JString) -> R {
        self.variable_error(node, var, VariableIssue::Redefined)
    }

    // port of: InterpreterBase.unsolvableMethod
    fn unsolvable_method(&self, node: NodeRef<'_>, method: &str, args: Option<&[Value]>) -> R {
        if self.is_strict_engine() {
            return Err(JexlException::method(Some(self.handle(node)), method, args, None));
        }
        Ok(Value::Null)
    }

    // port of: InterpreterBase.unsolvableProperty
    pub(crate) fn unsolvable_property(&self, node: NodeRef<'_>, property: &str, undef: bool, cause: Option<JexlException>) -> R {
        if self.is_strict_engine() && !node.is_ternary_protected() {
            return Err(JexlException::property(Some(self.handle(node)), property, undef, cause));
        }
        Ok(Value::Null)
    }

    // port of: InterpreterBase.operatorError
    pub(crate) fn operator_error(&self, node: NodeRef<'_>, operator: JexlOperator, cause: Option<JexlException>) -> R {
        if self.is_strict_engine() {
            return Err(JexlException::operator(Some(self.handle(node)), operator.get_operator_symbol(), cause));
        }
        Ok(Value::Null)
    }

    // port of: InterpreterBase.annotationError
    fn annotation_error(&self, node: NodeRef<'_>, annotation: &str, cause: Option<JexlException>) -> R {
        if self.is_strict_engine() {
            return Err(JexlException::annotation(Some(self.handle(node)), annotation, cause));
        }
        Ok(Value::Null)
    }

    // port of: InterpreterBase.findNullOperand
    fn find_null_operand<'a>(&self, e: &JexlException, node: NodeRef<'a>, left: &Value, right: &Value) -> NodeRef<'a> {
        if matches!(e.kind(), ExceptionKind::Java { class } if class == "JexlArithmetic$NullOperand") {
            if left.is_null() {
                return node.child(0);
            }
            if right.is_null() {
                return node.child(1);
            }
        }
        node
    }

    /// `throw new JexlException(node, msg, xrt)` — the unary operators blame their own node.
    fn node_exception(&self, node: NodeRef<'_>, msg: &str, e: ArithError) -> JexlException {
        let cause = self.arith_exception(node, e);
        JexlException::new(Some(self.handle(node)), msg, Some(cause))
    }

    /// Wraps an arithmetic failure the way `*`, `/`, `%`, `!=` and `..` do: the null operand's
    /// own node carries the blame.
    fn op_exception(&self, node: NodeRef<'_>, msg: &str, e: ArithError, left: &Value, right: &Value) -> JexlException {
        let cause = self.arith_exception(node, e);
        let xnode = self.find_null_operand(&cause, node, left, right);
        JexlException::new(Some(self.handle(xnode)), msg, Some(cause))
    }

    // port of: InterpreterBase.setContextVariable
    fn set_context_variable(&self, node: NodeRef<'_>, name: &str, value: Value) -> Result<(), JexlException> {
        if self.options.is_lexical_shade() && !self.context.has(name) {
            return Err(JexlException::variable(Some(self.handle(node)), &JString::from(name), VariableIssue::Undefined));
        }
        match self.context.set(name, value) {
            Ok(()) => Ok(()),
            Err(msg) => Err(JexlException::new(
                Some(self.handle(node)),
                "context is readonly",
                Some(JexlException::java("java.lang.UnsupportedOperationException", Some(msg))),
            )),
        }
    }

    // port of: InterpreterBase.isVariableDefined
    fn is_variable_defined(&self, name: &str) -> bool {
        if let (Some(frame), Some(block)) = (&self.frame, &self.block) {
            let scope = self.ast.scope(frame.scope());
            // Scope.getSymbol is read-only here: the scope is frozen after parsing
            if let Some(symbol) = scope.get_symbols().iter().position(|n| n == name) {
                let symbol = symbol as i32;
                if block.has_symbol(symbol) {
                    return matches!(frame.get(symbol as usize), Slot::Value(_));
                }
            }
        }
        self.context.has(name)
    }

    // port of: InterpreterBase.defineVariable
    fn define_variable(&mut self, var: &ASTIdentifier) -> bool {
        let symbol = var.get_symbol();
        if symbol < 0 || var.is_redefined() {
            return false;
        }
        let captured = var.is_captured();
        match &mut self.block {
            Some(block) => block.define_symbol(symbol, captured),
            None => false,
        }
    }

    // port of: InterpreterBase.getVariable
    fn get_variable(&self, node: NodeRef<'_>, identifier: &ASTIdentifier) -> R {
        let symbol = identifier.get_symbol();
        let name = identifier.get_name();
        if self.options.is_lexical_shade() && identifier.is_shaded() {
            return self.undefined_variable(node, &JString::from(name));
        }
        if symbol >= 0 {
            let frame = match &self.frame {
                None => return Err(npe_frame("has(int)", "frame")),
                Some(f) => f,
            };
            {
                if let Some(slot) = frame.lookup(symbol) {
                    match slot {
                        Slot::Undefined => {}
                        Slot::Undeclared => {}
                        Slot::Value(value) => {
                            let strict_parent = node.parent().map(|p| is_strict_operator(p)).unwrap_or(false);
                            if value.is_null() && self.arithmetic.is_strict() && strict_parent {
                                return self.unsolvable_variable(node, &JString::from(name), false);
                            }
                            return Ok(value);
                        }
                    }
                }
            }
        }
        let value = self.context.get(name).unwrap_or(Value::Null);
        if value.is_null() {
            if !self.context.has(name) {
                let parent_is_assignment = node.parent().map(|p| p.is(JJTASSIGNMENT)).unwrap_or(false);
                let parent_is_reference = node.parent().map(|p| p.is(JJTREFERENCE)).unwrap_or(false);
                let ignore = (self.is_safe() && (symbol >= 0 || parent_is_assignment)) || parent_is_reference;
                if !ignore {
                    return self.undefined_variable(node, &JString::from(name));
                }
            } else {
                let strict_parent = node.parent().map(|p| is_strict_operator(p)).unwrap_or(false);
                if self.arithmetic.is_strict() && strict_parent {
                    return self.unsolvable_variable(node, &JString::from(name), false);
                }
            }
        }
        Ok(value)
    }

    // ---------------------------------------------------------------- attributes

    // port of: InterpreterBase.getAttribute
    fn get_attribute(&self, object: &Value, attribute: &Value, node: Option<NodeRef<'_>>) -> R {
        if object.is_null() {
            let n = node.expect("a null object always has a node here");
            return Err(JexlException::new(Some(self.handle(n)), "object is null", None));
        }
        if let Some(n) = node {
            self.cancel_check(n)?;
        }
        let operator = match node.and_then(|n| n.parent()) {
            Some(p) if p.is(JJTARRAYACCESS) => JexlOperator::ArrayGet,
            _ => JexlOperator::PropertyGet,
        };
        if let Some(n) = node {
            let result = Operators::try_overload(self, n, operator, &[object.clone(), attribute.clone()])?;
            if !is_try_failed(&result) {
                return Ok(result);
            }
        }
        let mut xcause: Option<JexlException> = None;
        let resolvers = self.uberspect.get_resolvers(Some(operator), object);
        if let Some(vg) = self.uberspect.get_property_get_with(resolvers, object, attribute) {
            match vg.invoke(object) {
                Ok(v) => return Ok(v),
                Err(e) => xcause = Some(e),
            }
        }
        let n = match node {
            None => {
                return Err(JexlException::java(
                    "java.lang.UnsupportedOperationException",
                    Some(format!(
                        "unable to get object property, class: {}, property: {}",
                        object.class_name(),
                        attribute.java_to_string()
                    )),
                ))
            }
            Some(n) => n,
        };
        if n.is_identifier_access() && n.is_safe() {
            return Ok(Value::Null);
        }
        // port of: `attribute != null ? attribute.toString() : null`, and JexlException's own
        // `msg != null ? msg : ""` -- so a null property reads as '' , not 'null'.
        let attr_str = if attribute.is_null() { String::new() } else { attribute.java_to_string() };
        self.unsolvable_property(n, &attr_str, true, xcause)
    }

    // port of: InterpreterBase.setAttribute
    fn set_attribute(&self, object: &Value, attribute: &Value, value: &Value, node: Option<NodeRef<'_>>) -> Result<(), JexlException> {
        if let Some(n) = node {
            self.cancel_check(n)?;
        }
        let operator = match node.and_then(|n| n.parent()) {
            Some(p) if p.is(JJTARRAYACCESS) => JexlOperator::ArraySet,
            _ => JexlOperator::PropertySet,
        };
        if let Some(n) = node {
            let result = Operators::try_overload(self, n, operator, &[object.clone(), attribute.clone(), value.clone()])?;
            if !is_try_failed(&result) {
                return Ok(());
            }
        }
        let mut xcause: Option<JexlException> = None;
        let resolvers = self.uberspect.get_resolvers(Some(operator), object);
        let mut vs = self.uberspect.get_property_set_with(resolvers, object, attribute, value);
        if vs.is_none() {
            // try again with a narrow argument
            let mut narrow = [value.clone()];
            if self.arithmetic.narrow_arguments(&mut narrow) {
                vs = self.uberspect.get_property_set_with(resolvers, object, attribute, &narrow[0]);
            }
        }
        if let Some(vs) = vs {
            match vs.invoke(object, value) {
                Ok(_) => return Ok(()),
                Err(e) => xcause = Some(e),
            }
        }
        let n = match node {
            None => {
                return Err(JexlException::java(
                    "java.lang.UnsupportedOperationException",
                    Some(format!(
                        "unable to set object property, class: {}, property: {}, argument: {}",
                        object.class_name(),
                        attribute.java_to_string(),
                        value.simple_name()
                    )),
                ))
            }
            Some(n) => n,
        };
        let attr_str = if attribute.is_null() { String::new() } else { attribute.java_to_string() };
        self.unsolvable_property(n, &attr_str, true, xcause)?;
        Ok(())
    }

    // ---------------------------------------------------------------- entry points

    // port of: Interpreter.interpret
    pub fn interpret(&mut self, node: NodeRef<'_>) -> R {
        if self.fp > self.jexl.stack_overflow {
            return Err(JexlException::stack_overflow(
                node.jexl_info(),
                &format!("jexl ({})", self.jexl.stack_overflow),
                None,
            ));
        }
        self.cancel_check(node)?;
        match self.accept(node, None) {
            Ok(v) => Ok(v),
            Err(e) => match e.kind() {
                ExceptionKind::Return { value } => Ok(value.clone()),
                ExceptionKind::Cancel => {
                    if self.is_cancellable() {
                        Err(e)
                    } else {
                        Ok(Value::Null)
                    }
                }
                // Java catches JexlException here; a raw JDK throwable escapes even when silent
                _ if e.is_jexl() && self.is_silent() => Ok(Value::Null),
                _ => Err(e),
            },
        }
    }

    /// port of: Interpreter.runClosure
    pub fn run_closure(&mut self, closure: &Closure) -> R {
        // the tree outlives this borrow of self
        let ast = self.ast.clone();
        let argc = closure.arg_count(&ast);
        self.block = Some(LexicalFrame::new(self.frame.clone()).define_args(argc));
        let script = ast.node(closure.script);
        // jjtree leaves `children` null until the first jjtAddChild, so a script with no
        // statements (a lone `#pragma`) makes Java's jjtGetChild(-1) dereference null.
        if script.num_children() == 0 {
            self.block = None;
            return Err(JexlException::java(
                "java.lang.NullPointerException",
                Some("Cannot load from object array because \"this.children\" is null".into()),
            ));
        }
        let body = script.child(script.num_children() - 1);
        let result = self.interpret(body);
        if let Some(b) = &mut self.block {
            b.pop();
        }
        self.block = None;
        result
    }

    /// The visitor dispatch (`node.jjtAccept(this, data)`).
    fn accept(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        match node.kind() {
            JJTJEXLSCRIPT | JJTJEXLLAMBDA => self.visit_script(node, data),
            JJTBLOCK => self.visit_block_node(node, data),
            JJTIFSTATEMENT => self.visit_if(node),
            JJTWHILESTATEMENT => self.visit_while(node, data),
            JJTDOWHILESTATEMENT => self.visit_do_while(node, data),
            JJTFOREACHSTATEMENT => self.visit_foreach(node, data),
            JJTRETURNSTATEMENT => {
                let val = self.accept(node.child(0), data)?;
                self.cancel_check(node)?;
                Err(JexlException::return_(Some(self.handle(node)), "", val))
            }
            JJTCONTINUE => Err(JexlException::continue_(Some(self.handle(node)))),
            JJTBREAK => Err(JexlException::break_(Some(self.handle(node)))),
            JJTVAR => self.visit_var(node),
            JJTANNOTATEDSTATEMENT => self.process_annotation(node, 0, data),
            JJTANNOTATION => Err(JexlException::java(
                "java.lang.UnsupportedOperationException",
                Some("org.apache.commons.jexl3.parser.ASTAnnotation: Not supported.".into()),
            )),
            // literals
            JJTNULLLITERAL => Ok(Value::Null),
            JJTTRUENODE => Ok(Value::Boolean(true)),
            JJTFALSENODE => Ok(Value::Boolean(false)),
            JJTNUMBERLITERAL => {
                let literal = node.number().expect("number").get_literal_value();
                if let Some(data) = data {
                    if node.number().expect("number").is_integer() {
                        return self.get_attribute(data, &literal, Some(node));
                    }
                }
                Ok(literal)
            }
            JJTSTRINGLITERAL => {
                let literal = Value::String(node.literal().expect("literal").clone());
                if let Some(d) = data {
                    return self.get_attribute(d, &literal, Some(node));
                }
                Ok(literal)
            }
            JJTREGEXLITERAL => Ok(Value::object(crate::value::PatternValue(node.regex().expect("regex").clone()))),
            JJTARRAYLITERAL => self.visit_array_literal(node, data),
            JJTEXTENDEDLITERAL => Ok(Value::Null),
            JJTSETLITERAL => self.visit_set_literal(node, data),
            JJTMAPLITERAL => self.visit_map_literal(node, data),
            JJTMAPENTRY => {
                // Java returns an Object[]{key, value}; the map literal unpacks it
                let key = self.accept(node.child(0), data)?;
                let value = self.accept(node.child(1), data)?;
                Ok(Value::Array(crate::value::JArray::new(crate::value::Component::object(), vec![key, value])))
            }
            JJTJXLTLITERAL => self.visit_jxlt_literal(node),
            // operators
            JJTADDNODE => self.binary(node, data, JexlOperator::Add, "+"),
            JJTSUBNODE => self.binary(node, data, JexlOperator::Subtract, "-"),
            JJTMULNODE => self.binary(node, data, JexlOperator::Multiply, "*"),
            JJTDIVNODE => self.binary(node, data, JexlOperator::Divide, "/"),
            JJTMODNODE => self.binary(node, data, JexlOperator::Mod, "%"),
            JJTBITWISEANDNODE => self.binary(node, data, JexlOperator::And, "&"),
            JJTBITWISEORNODE => self.binary(node, data, JexlOperator::Or, "|"),
            JJTBITWISEXORNODE => self.binary(node, data, JexlOperator::Xor, "^"),
            JJTEQNODE => self.binary(node, data, JexlOperator::Eq, "=="),
            JJTNENODE => self.visit_ne(node, data),
            JJTGENODE => self.binary(node, data, JexlOperator::Gte, ">="),
            JJTGTNODE => self.binary(node, data, JexlOperator::Gt, ">"),
            JJTLENODE => self.binary(node, data, JexlOperator::Lte, "<="),
            JJTLTNODE => self.binary(node, data, JexlOperator::Lt, "<"),
            JJTSWNODE => {
                let (l, r) = self.operands(node, data)?;
                Ok(Value::Boolean(Operators::starts_with(self, node, "^=", &l, &r)?))
            }
            JJTNSWNODE => {
                let (l, r) = self.operands(node, data)?;
                Ok(Value::Boolean(!Operators::starts_with(self, node, "^!", &l, &r)?))
            }
            JJTEWNODE => {
                let (l, r) = self.operands(node, data)?;
                Ok(Value::Boolean(Operators::ends_with(self, node, "$=", &l, &r)?))
            }
            JJTNEWNODE => {
                let (l, r) = self.operands(node, data)?;
                Ok(Value::Boolean(!Operators::ends_with(self, node, "$!", &l, &r)?))
            }
            JJTERNODE => {
                let (l, r) = self.operands(node, data)?;
                Ok(Value::Boolean(Operators::contains(self, node, "=~", &r, &l)?))
            }
            JJTNRNODE => {
                let (l, r) = self.operands(node, data)?;
                Ok(Value::Boolean(!Operators::contains(self, node, "!~", &r, &l)?))
            }
            JJTRANGENODE => {
                let (l, r) = self.operands(node, data)?;
                match self.arithmetic.create_range(&l, &r) {
                    Ok(range) => Ok(Value::object(range)),
                    // Java catches only ArithmeticException here
                    Err(e) if !is_arithmetic_exception(&e) => Err(self.arith_exception(node, e)),
                    Err(e) => Err(self.op_exception(node, ".. error", e, &l, &r)),
                }
            }
            JJTUNARYMINUSNODE => self.visit_unary_minus(node, data),
            JJTUNARYPLUSNODE => self.visit_unary_plus(node, data),
            JJTBITWISECOMPLNODE => {
                let arg = self.accept(node.child(0), data)?;
                let result = Operators::try_overload(self, node, JexlOperator::Complement, std::slice::from_ref(&arg))?;
                if !is_try_failed(&result) {
                    return Ok(result);
                }
                match self.arithmetic.complement(&arg) {
                    Ok(v) => Ok(v),
                    Err(e) if !is_arithmetic_exception(&e) => Err(self.arith_exception(node, e)),
                    Err(e) => Err(self.node_exception(node, "~ error", e)),
                }
            }
            JJTNOTNODE => {
                let val = self.accept(node.child(0), data)?;
                let result = Operators::try_overload(self, node, JexlOperator::Not, std::slice::from_ref(&val))?;
                if !is_try_failed(&result) {
                    return Ok(result);
                }
                match self.arithmetic.not(&val) {
                    Ok(v) => Ok(v),
                    Err(e) if !is_arithmetic_exception(&e) => Err(self.arith_exception(node, e)),
                    Err(e) => Err(self.node_exception(node, "! error", e)),
                }
            }
            JJTANDNODE => self.visit_and(node, data),
            JJTORNODE => self.visit_or(node, data),
            JJTTERNARYNODE => self.visit_ternary(node, data),
            JJTNULLPNODE => self.visit_nullp(node, data),
            JJTSIZEFUNCTION => match self.accept(node.child(0), data) {
                Ok(val) => Operators::size(self, node, &val),
                Err(e) if e.is_jexl() => Ok(Value::Integer(0)),
                Err(e) => Err(e),
            },
            JJTEMPTYFUNCTION => match self.accept(node.child(0), data) {
                Ok(val) => Operators::empty(self, node, &val),
                Err(e) if e.is_jexl() => Ok(Value::Boolean(true)),
                Err(e) => Err(e),
            },
            // references
            JJTREFERENCEEXPRESSION => self.accept(node.child(0), data),
            JJTIDENTIFIER | JJTNAMESPACEIDENTIFIER => {
                self.cancel_check(node)?;
                if let Some(w) = crate::internal::template_interpreter::visit_identifier(self, node) {
                    return Ok(w);
                }
                match data {
                    Some(d) => {
                        let name = Value::string(node.identifier().expect("identifier").get_name());
                        self.get_attribute(d, &name, Some(node))
                    }
                    None => self.get_variable(node, node.identifier().expect("identifier")),
                }
            }
            JJTARRAYACCESS => self.visit_array_access(node, data),
            k if is_identifier_access(k) => match data {
                None => Ok(Value::Null),
                Some(d) => {
                    let id = self.eval_identifier(node)?;
                    self.get_attribute(d, &id, Some(node))
                }
            },
            JJTREFERENCE => self.visit_reference(node, data),
            // assignment
            JJTASSIGNMENT => self.execute_assign(node, None, data),
            JJTSETADDNODE => self.execute_assign(node, Some(JexlOperator::SelfAdd), data),
            JJTSETSUBNODE => self.execute_assign(node, Some(JexlOperator::SelfSubtract), data),
            JJTSETMULTNODE => self.execute_assign(node, Some(JexlOperator::SelfMultiply), data),
            JJTSETDIVNODE => self.execute_assign(node, Some(JexlOperator::SelfDivide), data),
            JJTSETMODNODE => self.execute_assign(node, Some(JexlOperator::SelfMod), data),
            JJTSETANDNODE => self.execute_assign(node, Some(JexlOperator::SelfAnd), data),
            JJTSETORNODE => self.execute_assign(node, Some(JexlOperator::SelfOr), data),
            JJTSETXORNODE => self.execute_assign(node, Some(JexlOperator::SelfXor), data),
            // calls
            JJTMETHODNODE => self.visit_method(node, None, data),
            JJTFUNCTIONNODE => self.visit_function(node, data),
            JJTCONSTRUCTORNODE => self.visit_constructor(node, data),
            JJTARGUMENTS => {
                // only reached through visit_arguments
                let argv = self.visit_arguments(node, data)?;
                Ok(Value::Array(crate::value::JArray::new(crate::value::Component::object(), argv)))
            }
            other => Err(JexlException::java(
                "java.lang.UnsupportedOperationException",
                Some(format!("node {} not supported", other)),
            )),
        }
    }

    fn operands(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> Result<(Value, Value), JexlException> {
        let left = self.accept(node.child(0), data)?;
        let right = self.accept(node.child(1), data)?;
        Ok((left, right))
    }

    /// The shared shape of the binary arithmetic/comparison visits.
    fn binary(&mut self, node: NodeRef<'_>, data: Option<&Value>, operator: JexlOperator, symbol: &str) -> R {
        let (left, right) = self.operands(node, data)?;
        // Operators.tryOverload answers TRY_FAILED at once when nothing overloads the operator;
        // asking first spares cloning both operands and the sentinel on every `+`, `<`, `==`...
        if self.uberspect.overloads(operator) {
            let result = Operators::try_overload(self, node, operator, &[left.clone(), right.clone()])?;
            if !is_try_failed(&result) {
                return Ok(result);
            }
        }
        let a = &self.arithmetic;
        let outcome = match operator {
            JexlOperator::Add => a.add(&left, &right),
            JexlOperator::Subtract => a.subtract(&left, &right),
            JexlOperator::Multiply => a.multiply(&left, &right),
            JexlOperator::Divide => a.divide(&left, &right),
            JexlOperator::Mod => a.modulo(&left, &right),
            JexlOperator::And => a.and(&left, &right),
            JexlOperator::Or => a.or(&left, &right),
            JexlOperator::Xor => a.xor(&left, &right),
            JexlOperator::Eq => a.equals(&left, &right).map(Value::Boolean),
            JexlOperator::Gte => a.greater_than_or_equal(&left, &right).map(Value::Boolean),
            JexlOperator::Gt => a.greater_than(&left, &right).map(Value::Boolean),
            JexlOperator::Lte => a.less_than_or_equal(&left, &right).map(Value::Boolean),
            JexlOperator::Lt => a.less_than(&left, &right).map(Value::Boolean),
            other => unreachable!("binary {:?}", other),
        };
        match outcome {
            Ok(v) => Ok(v),
            Err(e) => {
                // Java only catches ArithmeticException here; the other throwables propagate
                if !is_arithmetic_exception(&e) {
                    return Err(self.arith_exception(node, e));
                }
                // `/` and `%` return 0.0 when the arithmetic is lenient
                if matches!(operator, JexlOperator::Divide | JexlOperator::Mod) && !self.arithmetic.is_strict() {
                    return Ok(Value::Double(0.0));
                }
                // only `*`, `/`, `%` and `!=` look for the null operand; the rest mark the node
                let msg = format!("{} error", symbol);
                match operator {
                    JexlOperator::Multiply | JexlOperator::Divide | JexlOperator::Mod => {
                        Err(self.op_exception(node, &msg, e, &left, &right))
                    }
                    _ => {
                        let cause = self.arith_exception(node, e);
                        Err(JexlException::new(Some(self.handle(node)), &msg, Some(cause)))
                    }
                }
            }
        }
    }

    // port of: Interpreter.visit(ASTNENode)
    fn visit_ne(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let (left, right) = self.operands(node, data)?;
        let result = Operators::try_overload(self, node, JexlOperator::Eq, &[left.clone(), right.clone()])?;
        let outcome = if !is_try_failed(&result) {
            self.arithmetic.to_boolean(&result).map(|b| Value::Boolean(!b))
        } else {
            self.arithmetic.equals(&left, &right).map(|b| Value::Boolean(!b))
        };
        match outcome {
            Ok(v) => Ok(v),
            Err(e) if !is_arithmetic_exception(&e) => Err(self.arith_exception(node, e)),
            Err(e) => Err(self.op_exception(node, "!= error", e, &left, &right)),
        }
    }

    // port of: Interpreter.visit(ASTUnaryMinusNode)
    fn visit_unary_minus(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let val_node = node.child(0);
        let val = self.accept(val_node, data)?;
        let result = Operators::try_overload(self, node, JexlOperator::Negate, std::slice::from_ref(&val))?;
        if !is_try_failed(&result) {
            return Ok(result);
        }
        match self.arithmetic.negate(&val) {
            Ok(number) => {
                // attempt to recoerce to the literal's type
                if number.is_number() && val_node.is(JJTNUMBERLITERAL) {
                    let narrow = val_node.number().and_then(|n| n.get_literal_class()).map(num_class);
                    return Ok(self.arithmetic.narrow_number(&number, narrow));
                }
                Ok(number)
            }
            Err(e) => {
                let cause = self.arith_exception(val_node, e);
                Err(JexlException::new(Some(self.handle(val_node)), "- error", Some(cause)))
            }
        }
    }

    // port of: Interpreter.visit(ASTUnaryPlusNode)
    fn visit_unary_plus(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let val_node = node.child(0);
        let val = self.accept(val_node, data)?;
        let result = Operators::try_overload(self, node, JexlOperator::Positivize, std::slice::from_ref(&val))?;
        if !is_try_failed(&result) {
            return Ok(result);
        }
        match self.arithmetic.positivize(&val) {
            Ok(v) => Ok(v),
            Err(e) => {
                let cause = self.arith_exception(val_node, e);
                Err(JexlException::new(Some(self.handle(val_node)), "- error", Some(cause)))
            }
        }
    }

    // port of: Interpreter.visit(ASTAndNode)
    fn visit_and(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let left = self.accept(node.child(0), data)?;
        match self.arithmetic.to_boolean(&left) {
            Ok(false) => return Ok(Value::Boolean(false)),
            Ok(true) => {}
            Err(e) => {
                let cause = self.arith_exception(node, e);
                return Err(JexlException::new(Some(self.handle(node.child(0))), "boolean coercion error", Some(cause)));
            }
        }
        let right = self.accept(node.child(1), data)?;
        match self.arithmetic.to_boolean(&right) {
            Ok(false) => Ok(Value::Boolean(false)),
            Ok(true) => Ok(Value::Boolean(true)),
            Err(e) => {
                let cause = self.arith_exception(node, e);
                Err(JexlException::new(Some(self.handle(node.child(1))), "boolean coercion error", Some(cause)))
            }
        }
    }

    // port of: Interpreter.visit(ASTOrNode)
    fn visit_or(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let left = self.accept(node.child(0), data)?;
        match self.arithmetic.to_boolean(&left) {
            Ok(true) => return Ok(Value::Boolean(true)),
            Ok(false) => {}
            Err(e) => {
                let cause = self.arith_exception(node, e);
                return Err(JexlException::new(Some(self.handle(node.child(0))), "boolean coercion error", Some(cause)));
            }
        }
        let right = self.accept(node.child(1), data)?;
        match self.arithmetic.to_boolean(&right) {
            Ok(true) => Ok(Value::Boolean(true)),
            Ok(false) => Ok(Value::Boolean(false)),
            Err(e) => {
                let cause = self.arith_exception(node, e);
                Err(JexlException::new(Some(self.handle(node.child(1))), "boolean coercion error", Some(cause)))
            }
        }
    }

    // port of: Interpreter.visit(ASTTernaryNode)
    fn visit_ternary(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let condition = match self.accept(node.child(0), data) {
            Ok(v) => Some(v),
            Err(e) => {
                if !is_null_operand_cause(&e) {
                    return Err(e);
                }
                None
            }
        };
        let truthy = match &condition {
            Some(c) if !c.is_null() => self.arithmetic.to_boolean(c).map_err(|e| self.arith_exception(node, e))?,
            _ => false,
        };
        if node.num_children() == 3 {
            if truthy {
                return self.accept(node.child(1), data);
            }
            return self.accept(node.child(2), data);
        }
        if truthy {
            return Ok(condition.expect("condition"));
        }
        self.accept(node.child(1), data)
    }

    // port of: Interpreter.visit(ASTNullpNode)
    fn visit_nullp(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let lhs = match self.accept(node.child(0), data) {
            Ok(v) => v,
            Err(e) => {
                if !is_null_operand_cause(&e) {
                    return Err(e);
                }
                Value::Null
            }
        };
        if !lhs.is_null() {
            return Ok(lhs);
        }
        self.accept(node.child(1), data)
    }

    // port of: Interpreter.visit(ASTIfStatement) — the Java try block wraps the branches too
    fn visit_if(&mut self, node: NodeRef<'_>) -> R {
        match self.visit_if_body(node) {
            Err(e) if is_arithmetic_throwable(&e) => {
                Err(JexlException::new(Some(self.handle(node.child(0))), "if error", Some(e)))
            }
            other => other,
        }
    }

    fn visit_if_body(&mut self, node: NodeRef<'_>) -> R {
        let num_children = node.num_children();
        let mut if_else = 0;
        while if_else + 1 < num_children {
            let condition = self.accept(node.child(if_else), None)?;
            match self.arithmetic.to_boolean(&condition) {
                Ok(true) => return self.accept(node.child(if_else + 1), None),
                Ok(false) => {}
                Err(e) => return Err(self.arith_exception(node, e)),
            }
            if_else += 2;
        }
        if num_children & 1 == 1 {
            return self.accept(node.child(num_children - 1), None);
        }
        Ok(Value::Null)
    }

    // port of: Interpreter.visit(ASTVar)
    fn visit_var(&mut self, node: NodeRef<'_>) -> R {
        let identifier = node.identifier().expect("var").clone();
        let symbol = identifier.get_symbol();
        if !self.options.is_lexical() {
            let frame = match &self.frame {
                None => return Err(npe_frame("has(int)", "this.frame")),
                Some(f) => f,
            };
            {
                if let Some(slot) = frame.lookup(symbol) {
                    if let Slot::Value(v) = slot {
                        return Ok(v);
                    }
                    return Ok(Value::Null);
                }
            }
        } else if !self.define_variable(&identifier) {
            return self.redefined_variable(node, &JString::from(identifier.get_name()));
        }
        match &self.frame {
            None => return Err(npe_frame("set(int, Object)", "this.frame")),
            Some(frame) => frame.set(symbol as usize, Slot::Value(Value::Null)),
        }
        Ok(Value::Null)
    }

    // port of: Interpreter.visit(ASTBlock)
    fn visit_block_node(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let cnt = node.symbol_count();
        if !self.options.is_lexical() || cnt <= 0 {
            return self.visit_block(node, data);
        }
        let saved = self.block.take();
        self.block = Some(LexicalFrame::new(self.frame.clone()));
        let result = self.visit_block(node, data);
        if let Some(b) = &mut self.block {
            b.pop();
        }
        self.block = saved;
        result
    }

    // port of: Interpreter.visitBlock
    fn visit_block(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let mut result = Value::Null;
        for i in 0..node.num_children() {
            self.cancel_check(node)?;
            result = self.accept(node.child(i), data)?;
        }
        Ok(result)
    }

    // port of: Interpreter.visit(ASTJexlScript)
    fn visit_script(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        if !(node.is(JJTJEXLLAMBDA) && !node.is_top_level()) {
            if let Some(r) = crate::internal::template_interpreter::visit_script(self, node, data) {
                return r;
            }
        }
        if node.is(JJTJEXLLAMBDA) && !node.is_top_level() {
            let frame = node
                .script()
                .and_then(|s| s.get_scope())
                .and_then(|scope| create_frame(self.ast.scopes_ref(), scope, self.frame.as_ref(), None));
            // a lambda a TemplateInterpreter evaluates is an anonymous Closure subclass in Java
            return Ok(Value::object(if self.tmpl.is_some() {
                Closure::from_template(self.ast.clone(), node.id, frame)
            } else {
                Closure::new(self.ast.clone(), node.id, frame)
            }));
        }
        let argc = node.get_scope().map(|s| s.get_arg_count()).unwrap_or(0);
        let saved = self.block.take();
        self.block = Some(LexicalFrame::new(self.frame.clone()).define_args(argc));
        let mut result = Ok(Value::Null);
        for i in 0..node.num_children() {
            let child = node.child(i);
            result = self.accept(child, data);
            if result.is_err() {
                break;
            }
            if let Err(e) = self.cancel_check(child) {
                result = Err(e);
                break;
            }
        }
        if let Some(b) = &mut self.block {
            b.pop();
        }
        self.block = saved;
        result
    }

    // port of: Interpreter.visit(ASTWhileStatement)
    fn visit_while(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let mut result = Value::Null;
        loop {
            let c = self.accept(node.child(0), data)?;
            match self.arithmetic.to_boolean(&c) {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => return Err(self.arith_exception(node, e)),
            }
            self.cancel_check(node)?;
            if node.num_children() > 1 {
                match self.accept(node.child(1), data) {
                    Ok(v) => result = v,
                    Err(e) => match e.kind() {
                        ExceptionKind::Break => break,
                        ExceptionKind::Continue => continue,
                        _ => return Err(e),
                    },
                }
            }
        }
        Ok(result)
    }

    // port of: Interpreter.visit(ASTDoWhileStatement)
    fn visit_do_while(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let mut result = Value::Null;
        let nc = node.num_children();
        loop {
            self.cancel_check(node)?;
            if nc > 1 {
                match self.accept(node.child(0), data) {
                    Ok(v) => result = v,
                    Err(e) => match e.kind() {
                        ExceptionKind::Break => break,
                        ExceptionKind::Continue => {}
                        _ => return Err(e),
                    },
                }
            }
            let c = self.accept(node.child(nc - 1), data)?;
            match self.arithmetic.to_boolean(&c) {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => return Err(self.arith_exception(node, e)),
            }
        }
        Ok(result)
    }

    // port of: Interpreter.visit(ASTForeachStatement)
    #[allow(clippy::explicit_counter_loop)] // the count is the loop's own state, not an index
    fn visit_foreach(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let result;
        let loop_reference = node.child(0);
        let loop_variable_node = loop_reference.child(0);
        let loop_variable = loop_variable_node.identifier().expect("loop variable").clone();
        let symbol = loop_variable.get_symbol();
        let lexical = self.options.is_lexical();
        let loop_symbol = symbol >= 0 && loop_variable_node.is(JJTVAR);
        let saved = self.block.take();
        if lexical {
            self.block = Some(LexicalFrame::new(self.frame.clone()));
            if loop_symbol && !self.define_variable(&loop_variable) {
                let r = self.redefined_variable(node, &JString::from(loop_variable.get_name()));
                self.block = saved;
                return r;
            }
        } else {
            self.block = saved.clone();
        }
        let outcome = (|| -> R {
            let iterable_value = self.accept(node.child(1), data)?;
            if iterable_value.is_null() {
                return Ok(Value::Null);
            }
            let statement = if node.num_children() >= 3 { Some(node.child(2)) } else { None };
            let for_each = Operators::try_overload(self, node, JexlOperator::ForEach, std::slice::from_ref(&iterable_value))?;
            let items: Vec<Value> = if !is_try_failed(&for_each) {
                match self.uberspect.get_iterator(&for_each) {
                    Some(it) => it.collect(),
                    None => return Ok(Value::Null),
                }
            } else {
                match self.uberspect.get_iterator(&iterable_value) {
                    Some(it) => it.collect(),
                    None => return Ok(Value::Null),
                }
            };
            let mut result = Value::Null;
            // ponytail: the index is the loop body's own state (the lexical block is pushed only
            // from the second iteration on), so `enumerate` would not read any better here.
            let mut cnt = 0;
            for value in items {
                self.cancel_check(node)?;
                if lexical && cnt > 0 {
                    if let Some(b) = &mut self.block {
                        b.pop();
                    }
                    if loop_symbol && !self.define_variable(&loop_variable) {
                        return self.redefined_variable(node, &JString::from(loop_variable.get_name()));
                    }
                }
                cnt += 1;
                if symbol < 0 {
                    self.set_context_variable(node, loop_variable.get_name(), value)?;
                } else {
                    match &self.frame {
                        None => return Err(npe_frame("set(int, Object)", "this.frame")),
                        Some(frame) => frame.set(symbol as usize, Slot::Value(value)),
                    }
                }
                if let Some(statement) = statement {
                    match self.accept(statement, data) {
                        Ok(v) => result = v,
                        Err(e) => match e.kind() {
                            ExceptionKind::Break => break,
                            ExceptionKind::Continue => continue,
                            _ => return Err(e),
                        },
                    }
                }
            }
            Ok(result)
        })();
        if lexical {
            if let Some(b) = &mut self.block {
                b.pop();
            }
        }
        self.block = saved;
        match outcome {
            Ok(v) => {
                result = v;
                Ok(result)
            }
            Err(e) => Err(e),
        }
    }

    // port of: Interpreter.visit(ASTArrayLiteral)
    fn visit_array_literal(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let child_count = node.num_children();
        let mut ab = ArrayBuilder::new(child_count);
        let mut extended = false;
        for i in 0..child_count {
            self.cancel_check(node)?;
            let child = node.child(i);
            if child.is(JJTEXTENDEDLITERAL) {
                extended = true;
            } else {
                let entry = self.accept(child, data)?;
                ab.add(entry);
            }
        }
        Ok(ab.create(extended))
    }

    // port of: Interpreter.visit(ASTSetLiteral)
    fn visit_set_literal(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let child_count = node.num_children();
        let mut mb = SetBuilder::new(child_count);
        for i in 0..child_count {
            self.cancel_check(node)?;
            let entry = self.accept(node.child(i), data)?;
            mb.add(entry);
        }
        Ok(mb.create())
    }

    // port of: Interpreter.visit(ASTMapLiteral)
    fn visit_map_literal(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let child_count = node.num_children();
        let mut mb = MapBuilder::new(child_count);
        for i in 0..child_count {
            self.cancel_check(node)?;
            let entry = self.accept(node.child(i), data)?;
            match &entry {
                Value::Array(a) if a.len() == 2 => {
                    mb.put(a.get(0).expect("key"), a.get(1).expect("value"));
                }
                _ => {
                    return Err(JexlException::java(
                        "java.lang.ClassCastException",
                        Some("map entry is not a pair".into()),
                    ))
                }
            }
        }
        Ok(mb.create())
    }

    // port of: Interpreter.visit(ASTJxltLiteral)
    fn visit_jxlt_literal(&mut self, node: NodeRef<'_>) -> R {
        crate::internal::template_interpreter::visit_jxlt_literal(self, node)
    }

    // port of: Interpreter.evalIdentifier
    fn eval_identifier(&mut self, node: NodeRef<'_>) -> R {
        if !node.is_expression() {
            return Ok(node.identifier_access().expect("access").get_identifier());
        }
        crate::internal::template_interpreter::eval_identifier_jxlt(self, node)
    }

    // port of: Interpreter.visit(ASTArrayAccess)
    fn visit_array_access(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        let mut object = data.cloned().unwrap_or(Value::Null);
        for i in 0..node.num_children() {
            let nindex = node.child(i);
            if object.is_null() {
                let pty = self.stringify_property(nindex);
                return self.unsolvable_property(nindex, &pty, false, None);
            }
            let index = self.accept(nindex, None)?;
            self.cancel_check(node)?;
            object = self.get_attribute(&object, &index, Some(nindex))?;
        }
        Ok(object)
    }

    // port of: InterpreterBase.stringifyProperty
    fn stringify_property(&self, node: NodeRef<'_>) -> String {
        if node.is(JJTARRAYACCESS) {
            return format!("[{}]", self.stringify_property_value(node.child(0)));
        }
        if node.is(JJTMETHODNODE) || node.is(JJTFUNCTIONNODE) {
            return self.stringify_property_value(node.child(0));
        }
        if node.is_identifier() {
            return node.identifier().expect("identifier").get_name().to_string();
        }
        if node.is(JJTREFERENCE) {
            return self.stringify_property(node.child(0));
        }
        self.stringify_property_value(node)
    }

    // port of: InterpreterBase.stringifyPropertyValue (a Debugger rendering at depth 1)
    fn stringify_property_value(&self, node: NodeRef<'_>) -> String {
        debug_render(node, 1).unwrap_or_else(|| "???".into())
    }

    /// port of: InterpreterBase.invocationException — a non-JEXL throwable from a call is wrapped
    fn invoked(&self, node: NodeRef<'_>, method_name: &str, r: R) -> R {
        match r {
            Ok(v) => Ok(v),
            Err(e) if e.is_jexl() => Err(e),
            Err(e) => Err(JexlException::new(Some(self.handle(node)), method_name, Some(e))),
        }
    }

    /// port of: Interpreter(Interpreter, JexlArithmetic) — a nested interpreter for a closure call
    pub(crate) fn fork(&self, ast: Arc<Ast>, frame: Option<Frame>) -> Interpreter {
        Interpreter {
            jexl: self.jexl.clone(),
            uberspect: self.uberspect.clone(),
            arithmetic: self.arithmetic.clone(),
            context: self.context.clone(),
            options: self.options.clone(),
            cache: self.cache,
            cancelled: self.cancelled.clone(),
            functions: self.functions.clone(),
            ast,
            frame,
            block: None,
            fp: self.fp + 1,
            tmpl: self.tmpl.clone(),
        }
    }

    // port of: Interpreter.visit(ASTReference)
    // Java assigns the flag on every exit path, even where nothing reads it afterwards
    #[allow(unused_assignments)]
    fn visit_reference(&mut self, node: NodeRef<'_>, _data: Option<&Value>) -> R {
        self.cancel_check(node)?;
        let num_children = node.num_children();
        let parent = node.parent();
        let mut object = Value::Null;
        let mut object_node: Option<NodeRef<'_>> = None;
        let mut pty_node: Option<NodeRef<'_>> = None;
        let mut ant: Option<String> = None;
        let mut antish = !parent.map(|p| p.is(JJTREFERENCE)).unwrap_or(false);
        let mut v = 1usize;
        let mut broke_main = false;
        let mut c = 0usize;
        while c < num_children {
            let onode = node.child(c);
            object_node = Some(onode);
            if onode.is(JJTMETHODNODE) {
                antish = false;
                if object.is_null() {
                    if let Some(a) = &mut ant {
                        let child = onode.child(0);
                        if child.is_identifier_access() {
                            let alen = a.len();
                            a.push('.');
                            a.push_str(&child.identifier_access().expect("access").get_name().to_rust());
                            let got = self.context.get(a).unwrap_or(Value::Null);
                            if !got.is_null() {
                                object = self.visit_method(onode, Some(got), None)?;
                                c += 1;
                                continue;
                            }
                            a.truncate(alen);
                            pty_node = Some(onode);
                        }
                    }
                    break;
                }
            } else if onode.is(JJTARRAYACCESS) {
                antish = false;
                if object.is_null() {
                    pty_node = Some(onode);
                    break;
                }
            }
            let arg = if object.is_null() { None } else { Some(object.clone()) };
            object = self.accept(onode, arg.as_ref())?;
            self.cancel_check(node)?;
            if !object.is_null() {
                antish = false;
            } else if antish {
                if ant.is_none() {
                    let first = node.child(0);
                    if !first.is_identifier() {
                        pty_node = Some(onode);
                        broke_main = true;
                        break;
                    }
                    ant = Some(first.identifier().expect("identifier").get_name().to_string());
                    if !self.options.is_antish() {
                        antish = false;
                        c += 1;
                        continue;
                    }
                    if c == 0 {
                        c += 1;
                        continue;
                    }
                }
                let a = ant.as_mut().expect("ant");
                while v <= c {
                    let child = node.child(v);
                    if !child.is_identifier_access() {
                        pty_node = Some(onode);
                        broke_main = true;
                        break;
                    }
                    if child.is_safe() || child.is_expression() {
                        broke_main = true;
                        break;
                    }
                    a.push('.');
                    a.push_str(&child.identifier_access().expect("access").get_name().to_rust());
                    v += 1;
                }
                if broke_main {
                    break;
                }
                object = self.context.get(a).unwrap_or(Value::Null);
            } else if c != num_children - 1 {
                pty_node = Some(onode);
                break;
            }
            c += 1;
        }
        if object.is_null() {
            if let Some(pn) = pty_node {
                if pn.is_safe_lhs(self.is_safe()) {
                    return Ok(Value::Null);
                }
                if let Some(a) = &ant {
                    let defined = self.is_variable_defined(a);
                    return self.unsolvable_variable(node, &JString::from(a.as_str()), !defined);
                }
                let pty = self.stringify_property(pn);
                let same = object_node.map(|o| o == pn).unwrap_or(false);
                return self.unsolvable_property(node, &pty, same, None);
            }
            if antish {
                if node.is_safe_lhs(self.is_safe()) {
                    return Ok(Value::Null);
                }
                let aname = ant.clone().unwrap_or_else(|| "?".into());
                let defined = self.is_variable_defined(&aname);
                let strict_parent = node.parent().map(is_strict_operator).unwrap_or(false);
                if defined && (!self.arithmetic.is_strict() || !strict_parent) {
                    return Ok(Value::Null);
                }
                return self.unsolvable_variable(node, &JString::from(aname.as_str()), !defined);
            }
        }
        Ok(object)
    }

    // port of: Interpreter.executeAssign
    // the flag and the `ant != null` guard are Java's own, kept verbatim
    #[allow(unused_assignments, clippy::unnecessary_unwrap)]
    fn execute_assign(&mut self, node: NodeRef<'_>, assignop: Option<JexlOperator>, data: Option<&Value>) -> R {
        self.cancel_check(node)?;
        let left = node.child(0);
        let mut var: Option<ASTIdentifier> = None;
        let mut var_node = left;
        let mut object = Value::Null;
        let mut symbol = -1;
        if left.is_identifier() {
            let identifier = left.identifier().expect("identifier").clone();
            symbol = identifier.get_symbol();
            if symbol >= 0 && self.options.is_lexical() {
                if left.is(JJTVAR) {
                    if !self.define_variable(&identifier) {
                        return self.redefined_variable(left, &JString::from(identifier.get_name()));
                    }
                } else if self.options.is_lexical_shade() && identifier.is_shaded() {
                    return self.undefined_variable(left, &JString::from(identifier.get_name()));
                }
            }
            var = Some(identifier);
            var_node = left;
        }
        let mut antish = self.options.is_antish();
        let last = left.num_children() as isize - 1;
        let mut right = self.accept(node.child(1), data)?;

        if let Some(v) = &var {
            if symbol >= 0 {
                if last < 0 {
                    if let Some(op) = assignop {
                        let this_self = self.get_variable(var_node, v)?;
                        right = Operators::try_assign_overload(self, node, op, &[this_self.clone(), right])?;
                        if is_assign_marker(&right) {
                            return Ok(this_self);
                        }
                    }
                    match &self.frame {
                        None => return Err(npe_frame("set(int, Object)", "this.frame")),
                        Some(frame) => frame.set(symbol as usize, Slot::Value(right.clone())),
                    }
                    if let Some(closure) = right.as_host::<Closure>() {
                        closure.set_captured(&self.ast, symbol, right.clone());
                    }
                    return Ok(right);
                }
                object = self.get_variable(var_node, v)?;
                antish = false;
            } else {
                if last < 0 {
                    if let Some(op) = assignop {
                        let this_self = self.context.get(v.get_name()).unwrap_or(Value::Null);
                        right = Operators::try_assign_overload(self, node, op, &[this_self.clone(), right])?;
                        if is_assign_marker(&right) {
                            return Ok(this_self);
                        }
                    }
                    self.set_context_variable(node, v.get_name(), right.clone())?;
                    return Ok(right);
                }
                object = self.context.get(v.get_name()).unwrap_or(Value::Null);
                if !object.is_null() {
                    antish = false;
                }
            }
        } else if !left.is(JJTREFERENCE) {
            return Err(JexlException::new(Some(self.handle(left)), "illegal assignment form 0", None));
        }

        let mut object_node: Option<NodeRef<'_>> = None;
        let mut ant: Option<String> = None;
        let mut v_index = 1usize;
        let mut c = if symbol >= 0 { 1isize } else { 0isize };
        let mut broke_main = false;
        while c < last {
            let onode = left.child(c as usize);
            object_node = Some(onode);
            let arg = if object.is_null() { None } else { Some(object.clone()) };
            object = self.accept(onode, arg.as_ref())?;
            if !object.is_null() {
                antish = false;
            } else if antish {
                if ant.is_none() {
                    let first = left.child(0);
                    let first_id = if first.is_identifier() { first.identifier() } else { None };
                    match first_id {
                        None => {
                            antish = false;
                            broke_main = true;
                            break;
                        }
                        Some(fid) if fid.get_symbol() >= 0 => {
                            antish = false;
                            broke_main = true;
                            break;
                        }
                        Some(fid) => ant = Some(fid.get_name().to_string()),
                    }
                }
                let a = ant.as_mut().expect("ant");
                while v_index <= c as usize {
                    let child = left.child(v_index);
                    if !child.is_identifier_access() || child.is_safe() || child.is_expression() {
                        antish = false;
                        broke_main = true;
                        break;
                    }
                    a.push('.');
                    a.push_str(&child.identifier_access().expect("access").get_name().to_rust());
                    v_index += 1;
                }
                if broke_main {
                    break;
                }
                object = self.context.get(a).unwrap_or(Value::Null);
            } else {
                return Err(JexlException::new(Some(self.handle(onode)), "illegal assignment form", None));
            }
            c += 1;
        }

        let mut property;
        let mut property_node = left.child(last.max(0) as usize);
        if property_node.is_identifier_access() {
            let property_id = property_node.identifier_access().expect("access");
            if antish && ant.is_some() && object.is_null() && !property_node.is_safe() && !property_node.is_expression() {
                // the guard above is Java's own `ant != null` test, kept verbatim
                let a = ant.as_mut().expect("checked above");
                if last > 0 {
                    a.push('.');
                }
                a.push_str(&property_id.get_name().to_rust());
                if let Some(op) = assignop {
                    let this_self = self.context.get(a).unwrap_or(Value::Null);
                    right = Operators::try_assign_overload(self, node, op, &[this_self.clone(), right])?;
                    if is_assign_marker(&right) {
                        return Ok(this_self);
                    }
                }
                let name = a.clone();
                self.set_context_variable(property_node, &name, right.clone())?;
                return Ok(right);
            }
            property = self.eval_identifier(property_node)?;
        } else if property_node.is(JJTARRAYACCESS) {
            let num_children = property_node.num_children() - 1;
            for i in 0..num_children {
                let nindex = property_node.child(i);
                let index = self.accept(nindex, None)?;
                object = self.get_attribute(&object, &index, Some(nindex))?;
            }
            property_node = property_node.child(num_children);
            property = self.accept(property_node, None)?;
        } else {
            let n = object_node.unwrap_or(left);
            return Err(JexlException::new(Some(self.handle(n)), "illegal assignment form", None));
        }

        if object.is_null() {
            let n = object_node.unwrap_or(left);
            return self.unsolvable_property(n, "<null>.<?>", true, None);
        }
        if let Some(op) = assignop {
            let this_self = self.get_attribute(&object, &property, Some(property_node))?;
            right = Operators::try_assign_overload(self, node, op, &[this_self.clone(), right])?;
            if is_assign_marker(&right) {
                return Ok(this_self);
            }
        }
        let _ = &mut property;
        self.set_attribute(&object, &property, &right, Some(property_node))?;
        Ok(right)
    }

    // port of: Interpreter.visit(ASTArguments)
    pub(crate) fn visit_arguments(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> Result<Vec<Value>, JexlException> {
        let argc = node.num_children();
        let mut argv = Vec::with_capacity(argc);
        for i in 0..argc {
            argv.push(self.accept(node.child(i), data)?);
        }
        Ok(argv)
    }

    // port of: Interpreter.visit(ASTMethodNode, Object, Object)
    fn visit_method(&mut self, node: NodeRef<'_>, object: Option<Value>, data: Option<&Value>) -> R {
        let method_node = node.child(0);
        let mut object = object;
        let mut method_is_node = false;
        let mut method_value = Value::Null;
        if method_node.is_identifier_access() {
            method_is_node = true;
            if object.is_none() {
                object = data.cloned();
                if object.as_ref().map(Value::is_null).unwrap_or(true) {
                    return if node.is_safe_lhs(self.is_safe()) {
                        Ok(Value::Null)
                    } else {
                        self.unsolvable_method(method_node, "<null>.<?>(...)", None)
                    };
                }
            } else {
                method_is_node = false;
                method_value = object.clone().expect("object");
            }
        } else {
            method_value = self.accept(method_node, data)?;
        }
        let mut result = if method_is_node { Value::Null } else { method_value.clone() };
        let mut result_is_node = method_is_node;
        for a in 1..node.num_children() {
            if !result_is_node && result.is_null() {
                return if node.is_safe_lhs(self.is_safe()) {
                    Ok(Value::Null)
                } else {
                    self.unsolvable_method(method_node, "<?>.<null>(...)", None)
                };
            }
            let arg_node = node.child(a);
            let functor = if result_is_node { Callee::Node(method_node) } else { Callee::Value(result.clone()) };
            result = self.call(node, object.clone().unwrap_or(Value::Null), functor, arg_node)?;
            result_is_node = false;
            object = Some(result.clone());
        }
        Ok(result)
    }

    // port of: Interpreter.visit(ASTFunctionNode)
    fn visit_function(&mut self, node: NodeRef<'_>, _data: Option<&Value>) -> R {
        if let Some(r) = crate::internal::template_interpreter::visit_function(self, node) {
            return r;
        }
        let function_node = node.child(0);
        let nsid = function_node.identifier().and_then(|i| i.get_namespace().map(str::to_string));
        let namespace = match &nsid {
            Some(ns) => self.resolve_namespace(Some(ns), node)?,
            None => Value::Null,
        };
        let arg_node = node.child(1);
        self.call(node, namespace, Callee::Node(function_node), arg_node)
    }

    // port of: InterpreterBase.resolveNamespace
    fn resolve_namespace(&mut self, prefix: Option<&str>, node: NodeRef<'_>) -> R {
        if let Some(ns) = self.context.resolve_namespace(prefix) {
            return Ok(ns);
        }
        let namespace = match prefix.and_then(|p| self.functions.get(p)) {
            Some(v) => v.clone(),
            None => {
                return match prefix {
                    Some(p) => Err(JexlException::new(
                        Some(self.handle(node)),
                        &format!("no such function namespace {}", p),
                        None,
                    )),
                    None => Ok(Value::Null),
                }
            }
        };
        // A namespace named by a class -- `#pragma jexl.namespace.i java.lang.Integer` -- is first
        // asked for an instance (a functor), then used as the class itself, a namespace of static
        // methods. Without this the String is the namespace and `i:valueOf(x)` silently calls
        // String.valueOf.
        let is_class = matches!(&namespace, Value::Object(o) if o.class_name() == "java.lang.Class");
        if let Value::String(name) = &namespace {
            let name = name.to_rust();
            if let Some(ctor) = self.uberspect.get_constructor(&namespace, &[]) {
                if let Ok(functor) = ctor.invoke(&namespace, &[]) {
                    return Ok(functor);
                }
            }
            return Ok(self.uberspect.load_class(&name).unwrap_or(Value::Null));
        }
        if is_class {
            if let Some(ctor) = self.uberspect.get_constructor(&namespace, &[]) {
                if let Ok(functor) = ctor.invoke(&namespace, &[]) {
                    return Ok(functor);
                }
            }
        }
        Ok(namespace)
    }

    /// port of: Interpreter.CallDispatcher.isArithmeticMethod — `uberspect.getMethod(arithmetic, ...)`.
    ///
    /// Java reflects over the JexlArithmetic instance, so every public method of it is callable as
    /// `x.empty()`, `x.size()`, `x.toBoolean()`, `a.add(b)` and so on. The signatures are the ones
    /// `javap` reports for 3.2.1; all but a handful take Object, so the arity decides.
    fn arithmetic_method(&self, name: &str, args: &[Value]) -> Option<R> {
        let a = &self.arithmetic;
        let b = |r: crate::jexl_arithmetic::R<bool>| Some(r.map(Value::Boolean).map_err(|e| self.bare(e)));
        let ob = |r: crate::jexl_arithmetic::R<Option<bool>>| {
            Some(r.map(|v| v.map(Value::Boolean).unwrap_or(Value::Null)).map_err(|e| self.bare(e)))
        };
        let v = |r: crate::jexl_arithmetic::R<Value>| Some(r.map_err(|e| self.bare(e)));
        match (name, args.len()) {
            ("empty", 1) => b(a.empty(&args[0])),
            // isEmpty(Object) is isEmpty(object, object == null)
            ("isEmpty", 1) => ob(a.is_empty(&args[0], Some(args[0].is_null()))),
            ("isEmpty", 2) => match &args[1] {
                Value::Boolean(d) => ob(a.is_empty(&args[0], Some(*d))),
                Value::Null => ob(a.is_empty(&args[0], None)),
                _ => None,
            },
            // size(Object) is size(object, object == null ? 0 : 1)
            ("size", 1) => Some(
                a.size(&args[0], Some(if args[0].is_null() { 0 } else { 1 }))
                    .map(|v| v.map(Value::Integer).unwrap_or(Value::Null))
                    .map_err(|e| self.bare(e)),
            ),
            ("size", 2) => match &args[1] {
                Value::Integer(d) => Some(
                    a.size(&args[0], Some(*d))
                        .map(|v| v.map(Value::Integer).unwrap_or(Value::Null))
                        .map_err(|e| self.bare(e)),
                ),
                _ => None,
            },
            ("toBoolean", 1) => b(a.to_boolean(&args[0])),
            ("toInteger", 1) => Some(a.to_integer(&args[0]).map(Value::Integer).map_err(|e| self.bare(e))),
            ("toLong", 1) => Some(a.to_long(&args[0]).map(Value::Long).map_err(|e| self.bare(e))),
            ("toDouble", 1) => Some(a.to_double(&args[0]).map(Value::Double).map_err(|e| self.bare(e))),
            ("toBigInteger", 1) => Some(a.to_big_integer(&args[0]).map(Value::big_integer).map_err(|e| self.bare(e))),
            ("toBigDecimal", 1) => Some(a.to_big_decimal(&args[0]).map(Value::big_decimal).map_err(|e| self.bare(e))),
            ("toString", 1) => Some(a.to_jstring(&args[0]).map(Value::String).map_err(|e| self.bare(e))),
            ("negate", 1) => v(a.negate(&args[0])),
            ("positivize", 1) => v(a.positivize(&args[0])),
            ("complement", 1) => v(a.complement(&args[0])),
            ("not", 1) | ("logicalNot", 1) => v(a.not(&args[0])),
            ("narrow", 1) if args[0].is_number() => Some(Ok(a.narrow(&args[0]))),
            ("add", 2) => v(a.add(&args[0], &args[1])),
            ("subtract", 2) => v(a.subtract(&args[0], &args[1])),
            ("multiply", 2) => v(a.multiply(&args[0], &args[1])),
            ("divide", 2) => v(a.divide(&args[0], &args[1])),
            ("mod", 2) => v(a.modulo(&args[0], &args[1])),
            ("and", 2) | ("bitwiseAnd", 2) => v(a.and(&args[0], &args[1])),
            ("or", 2) | ("bitwiseOr", 2) => v(a.or(&args[0], &args[1])),
            ("xor", 2) | ("bitwiseXor", 2) => v(a.xor(&args[0], &args[1])),
            ("equals", 2) => b(a.equals(&args[0], &args[1])),
            ("lessThan", 2) => b(a.less_than(&args[0], &args[1])),
            ("lessThanOrEqual", 2) => b(a.less_than_or_equal(&args[0], &args[1])),
            ("greaterThan", 2) => b(a.greater_than(&args[0], &args[1])),
            ("greaterThanOrEqual", 2) => b(a.greater_than_or_equal(&args[0], &args[1])),
            ("contains", 2) => ob(a.contains(&args[0], &args[1])),
            ("startsWith", 2) => ob(a.starts_with(&args[0], &args[1])),
            ("endsWith", 2) => ob(a.ends_with(&args[0], &args[1])),
            ("createRange", 2) => v(a.create_range(&args[0], &args[1]).map(Value::object)),
            // java.lang.Object.equals, inherited by the arithmetic instance: `'abc'.equals()` misses
            // String.equals(Object) and lands here with the target as the argument. A script can
            // never hand it the arithmetic object itself, so the answer is always false.
            ("equals", 1) => Some(Ok(Value::Boolean(false))),
            ("isStrict", 0) => Some(Ok(Value::Boolean(a.is_strict()))),
            ("isNegateStable", 0) => Some(Ok(Value::Boolean(a.is_negate_stable()))),
            ("isPositivizeStable", 0) => Some(Ok(Value::Boolean(a.is_positivize_stable()))),
            ("getMathScale", 0) => Some(Ok(Value::Integer(a.get_math_scale()))),
            _ => None,
        }
    }

    /// port of: Interpreter.CallDispatcher.isContextMethod -- the context object's own public
    /// methods, which a script reaches like any other: JexlContext.get/set/has, MapContext.clear,
    /// and the equals(Object) it inherits. Measured: `has('x')`, `set('k', 5)` and `clear()` work
    /// as plain function calls, and `x.set(v)` with x a String binds the variable x names.
    fn context_method(&self, name: &str, args: &[Value]) -> Option<R> {
        // a String parameter takes null too; the context's names are Strings
        let text = |v: &Value| match v {
            Value::String(s) => Some(s.to_rust()),
            _ => None,
        };
        match (name, args.len()) {
            ("get", 1) if args[0].is_null() => Some(Ok(Value::Null)),
            ("get", 1) => text(&args[0]).map(|n| Ok(self.context.get(&n).unwrap_or(Value::Null))),
            ("has", 1) if args[0].is_null() => Some(Ok(Value::Boolean(false))),
            ("has", 1) => text(&args[0]).map(|n| Ok(Value::Boolean(self.context.has(&n)))),
            // ponytail: set(null, v) would bind a null key, which a &str-keyed context cannot hold
            ("set", 2) => text(&args[0]).map(|n| match self.context.set(&n, args[1].clone()) {
                Ok(()) => Ok(Value::Null),
                Err(m) => Err(JexlException::java("java.lang.UnsupportedOperationException", Some(m))),
            }),
            ("clear", 0) => self.context.clear().map(|()| Ok(Value::Null)),
            ("equals", 1) => Some(Ok(Value::Boolean(false))),
            _ => None,
        }
    }

    /// An arithmetic failure escaping a *method* call is the raw throwable, not an operator error.
    fn bare(&self, e: ArithError) -> JexlException {
        match e {
            ArithError::NullOperand => JexlException::java_msg("JexlArithmetic$NullOperand", None),
            other => JexlException::java_msg(other.class_name(), other.message()),
        }
    }

    // port of: Interpreter.call
    fn call(&mut self, node: NodeRef<'_>, target: Value, functor: Callee<'_>, arg_node: NodeRef<'_>) -> R {
        self.cancel_check(node)?;
        let mut argv = self.visit_arguments(arg_node, None)?;
        let is_context_target = target.is_null();
        let mut functor_value: Option<Value> = None;
        let method_name: Option<String>;
        let mut isavar = false;
        match functor {
            Callee::Node(n) if n.is_identifier() => {
                let id = n.identifier().expect("identifier");
                let symbol = id.get_symbol();
                method_name = Some(id.get_name().to_string());
                if is_context_target {
                    if let Some(frame) = &self.frame {
                        if let Some(Slot::Value(v)) = frame.lookup(symbol) {
                            if !v.is_null() {
                                functor_value = Some(v);
                                isavar = true;
                            }
                        }
                    }
                    if functor_value.is_none() {
                        let name = method_name.clone().expect("name");
                        if self.context.has(&name) {
                            let v = self.context.get(&name).unwrap_or(Value::Null);
                            if !v.is_null() {
                                functor_value = Some(v);
                                isavar = true;
                            }
                        }
                    }
                }
            }
            Callee::Node(n) if n.is_identifier_access() => {
                method_name = Some(n.identifier_access().expect("access").get_name().to_rust());
            }
            Callee::Node(_) => {
                method_name = None;
            }
            Callee::Value(v) if !v.is_null() => {
                functor_value = Some(v);
                method_name = None;
            }
            Callee::Value(_) => {
                return if !node.is_safe_lhs(self.is_safe()) {
                    self.unsolvable_method(node, "?(...)", None)
                } else {
                    Ok(Value::Null)
                }
            }
        }
        let _ = isavar;
        let mut narrow = false;
        // Java wraps the whole dispatch in `catch (JexlException.Method xmethod) { }`: a method
        // missing *inside* what this call invoked (a lambda's body, say) is not this call's
        // answer -- control falls through to the call site's own unsolvableMethod below.
        'dispatch: loop {
            // the target's own method
            if functor_value.is_none() {
                if let Some(name) = &method_name {
                    let recv = if is_context_target { Value::Null } else { target.clone() };
                    if !is_context_target {
                        if let Some(vm) = self.uberspect.get_method(&recv, name, &argv) {
                            match self.invoked(node, name, vm.invoke(&recv, &argv)) {

                                Err(e) if e.is_method_error() => break 'dispatch,

                                r => return r,

                            }
                        }
                        // ...then, with the target prepended, a method of the context itself
                        // (`x.set(v)` binds the variable x names), and an arithmetic function:
                        // `x.empty()` reaches JexlArithmetic.empty(Object).
                        let mut pargv = Vec::with_capacity(argv.len() + 1);
                        pargv.push(target.clone());
                        pargv.extend(argv.iter().cloned());
                        if let Some(r) = self.context_method(name, &pargv) {
                            match self.invoked(node, name, r) {
                                Err(e) if e.is_method_error() => break 'dispatch,
                                r => return r,
                            }
                        }
                        if let Some(r) = self.arithmetic_method(name, &pargv) {
                            match self.invoked(node, name, r) {

                                Err(e) if e.is_method_error() => break 'dispatch,

                                r => return r,

                            }
                        }
                        // ...or a functor stored in a property of the target: `m.a()` where the
                        // map holds a lambda under "a".
                        if !narrow {
                            let resolvers = self.uberspect.get_resolvers(None, &target);
                            let id = Value::string(name);
                            if let Some(get) = self.uberspect.get_property_get_with(resolvers, &target, &id) {
                                if let Ok(v) = get.invoke(&target) {
                                    if !v.is_null() {
                                        functor_value = Some(v);
                                        continue;
                                    }
                                }
                            }
                        }
                    } else {
                        // a function call with no namespace: the context object's own methods
                        // first (isTargetMethod on the context), then the default namespace
                        if let Some(r) = self.context_method(name, &argv) {
                            match self.invoked(node, name, r) {
                                Err(e) if e.is_method_error() => break 'dispatch,
                                r => return r,
                            }
                        }
                        let namespace = self.resolve_namespace(None, node)?;
                        if !namespace.is_null() {
                            if let Some(vm) = self.uberspect.get_method(&namespace, name, &argv) {
                                match self.invoked(node, name, vm.invoke(&namespace, &argv)) {

                                    Err(e) if e.is_method_error() => break 'dispatch,

                                    r => return r,

                                }
                            }
                        }
                        // ...then solve it as an arithmetic function
                        if let Some(r) = self.arithmetic_method(name, &argv) {
                            match self.invoked(node, name, r) {

                                Err(e) if e.is_method_error() => break 'dispatch,

                                r => return r,

                            }
                        }
                    }
                }
            }
            if let Some(f) = &functor_value {
                if let Some(script) = f.as_host::<Closure>() {
                    let name = method_name.clone().unwrap_or_default();
                    match self.invoked(node, &name, script.execute(self, &argv)) {

                        Err(e) if e.is_method_error() => break 'dispatch,

                        r => return r,

                    }
                }
                if let Some(name) = &method_name {
                    if let Some(vm) = self.uberspect.get_method(f, name, &argv) {
                        match self.invoked(node, name, vm.invoke(f, &argv)) {

                            Err(e) if e.is_method_error() => break 'dispatch,

                            r => return r,

                        }
                    }
                }
                if let Some(vm) = self.uberspect.get_method(f, "call", &argv) {
                    match self.invoked(node, "call", vm.invoke(f, &argv)) {

                        Err(e) if e.is_method_error() => break 'dispatch,

                        r => return r,

                    }
                }
            }
            if narrow || !self.arithmetic.narrow_arguments(&mut argv) {
                break;
            }
            narrow = true;
        }
        if node.is_safe_lhs(self.is_safe()) {
            return Ok(Value::Null);
        }
        match method_name {
            Some(name) => self.unsolvable_method(node, &name, Some(&argv)),
            // `(lambda)(args)` has no name. JexlException.Method builds its message eagerly with
            // methodSignature(name, args), whose `new StringBuilder(name)` throws on a null name as
            // soon as there are arguments -- only when the engine is strict, the one path that
            // builds the exception at all.
            None if !argv.is_empty() && self.is_strict_engine() => Err(JexlException::java(
                "java.lang.NullPointerException",
                Some("Cannot invoke \"String.length()\" because \"str\" is null".into()),
            )),
            None => self.unsolvable_method(node, "", Some(&argv)),
        }
    }

    // port of: Interpreter.visit(ASTConstructorNode)
    fn visit_constructor(&mut self, node: NodeRef<'_>, data: Option<&Value>) -> R {
        if self.is_cancelled() {
            return Err(JexlException::cancel(Some(self.handle(node))));
        }
        let target = self.accept(node.child(0), data)?;
        let argc = node.num_children() - 1;
        let mut argv = Vec::with_capacity(argc);
        for i in 0..argc {
            argv.push(self.accept(node.child(i + 1), data)?);
        }
        let mut narrow = false;
        loop {
            if let Some(ctor) = self.uberspect.get_constructor(&target, &argv) {
                return match ctor.invoke(&target, &argv) {
                    Ok(v) => Ok(v),
                    // port of: InterpreterBase.invocationException
                    Err(e) if e.is_jexl() => Err(e),
                    Err(e) => Err(JexlException::new(Some(self.handle(node)), &target.java_to_string(), Some(e))),
                };
            }
            if !narrow && self.arithmetic.narrow_arguments(&mut argv) {
                narrow = true;
                continue;
            }
            break;
        }
        // port of: `final String tstr = target != null ? target.toString() : "?";`
        let tstr = if target.is_null() { "?".to_string() } else { target.java_to_string() };
        self.unsolvable_method(node, &tstr, Some(&argv))
    }

    // port of: Interpreter.processAnnotation
    fn process_annotation(&mut self, stmt: NodeRef<'_>, index: usize, data: Option<&Value>) -> R {
        let last = stmt.num_children() - 1;
        if index == last {
            return self.accept(stmt.child(last), data);
        }
        let anode = stmt.child(index);
        let aname = anode.annotation_name().unwrap_or("").to_string();
        let argv = if anode.num_children() > 0 {
            Some(self.visit_arguments(anode.child(0), None)?)
        } else {
            None
        };
        // port of: Interpreter.AnnotatedCall — the processor decides whether the statement runs,
        // and not running it is an error.
        let context = self.context.clone();
        let called = std::cell::Cell::new(false);
        // Java hands Return/Break/Continue to the processor as a *value*; here they travel beside
        // it, which is the same outcome unless a processor swallows one.
        let mut escaped: Option<JexlException> = None;
        let result = {
            let called = &called;
            let escaped = &mut escaped;
            let mut run = || {
                called.set(true);
                match self.process_annotation(stmt, index + 1, data) {
                    Ok(v) => Ok(v),
                    Err(e) if e.is_control_flow() => {
                        *escaped = Some(e);
                        Ok(Value::Null)
                    }
                    Err(e) => Err(e),
                }
            };
            match context.process_annotation(&aname, argv.as_deref(), &mut run) {
                Some(r) => r,
                // no AnnotationProcessor: `stmt.call()`
                None => run(),
            }
        };
        if let Some(e) = escaped {
            return Err(e);
        }
        match result {
            Ok(v) if called.get() => Ok(v),
            // the processor never called the statement
            Ok(_) => self.annotation_error(anode, &aname, None),
            Err(e) if e.is_jexl() => Err(e),
            Err(e) => self.annotation_error(anode, &aname, Some(e)),
        }
    }
}

/// port of: `new Debugger().depth(d).data(node)`
pub(crate) fn debug_render(node: NodeRef<'_>, depth: i32) -> Option<String> {
    let mut dbg = crate::internal::debugger::Debugger::new();
    if depth > 0 {
        dbg.depth(depth);
    }
    Some(dbg.data(node).to_rust())
}

/// What is being called: a syntactic node, or an already-evaluated functor.
enum Callee<'a> {
    Node(NodeRef<'a>),
    Value(Value),
}

/// Whether an escaped throwable is a `java.lang.ArithmeticException` (what a catch block sees).
fn is_arithmetic_throwable(e: &JexlException) -> bool {
    matches!(e.kind(), ExceptionKind::Java { class }
        if class == "java.lang.ArithmeticException" || class == "JexlArithmetic$NullOperand")
}

/// The JDK's helpful NullPointerException when a script with no scope touches a register.
/// `receiver` is how the JDK names the null reference: `frame` for InterpreterBase.getVariable's
/// parameter, `this.frame` everywhere the interpreter reads its own field.
fn npe_frame(method: &str, receiver: &str) -> JexlException {
    JexlException::java(
        "java.lang.NullPointerException",
        Some(
            format!(
                "Cannot invoke \"org.apache.commons.jexl3.internal.Frame.{}\" because \"{}\" is null",
                method, receiver
            ),
        ),
    )
}

/// Whether the throwable is a `java.lang.ArithmeticException` (what the operator visits catch).
fn is_arithmetic_exception(e: &ArithError) -> bool {
    matches!(e, ArithError::NullOperand | ArithError::Arithmetic(_))
}

/// port of: JexlNode.isStrictOperator (OperatorController)
fn is_strict_operator(node: NodeRef<'_>) -> bool {
    !matches!(node.kind(), JJTEQNODE | JJTNENODE)
        && matches!(
            node.kind(),
            JJTNOTNODE
                | JJTADDNODE
                | JJTSETADDNODE
                | JJTMULNODE
                | JJTSETMULTNODE
                | JJTMODNODE
                | JJTSETMODNODE
                | JJTDIVNODE
                | JJTSETDIVNODE
                | JJTBITWISEANDNODE
                | JJTSETANDNODE
                | JJTBITWISEORNODE
                | JJTSETORNODE
                | JJTBITWISEXORNODE
                | JJTSETXORNODE
                | JJTBITWISECOMPLNODE
                | JJTSUBNODE
                | JJTSETSUBNODE
                | JJTGTNODE
                | JJTGENODE
                | JJTLTNODE
                | JJTLENODE
                | JJTSWNODE
                | JJTNSWNODE
                | JJTEWNODE
                | JJTNEWNODE
                | JJTERNODE
                | JJTNRNODE
        )
}

fn is_identifier_access(kind: i32) -> bool {
    matches!(
        kind,
        JJTIDENTIFIERACCESS | JJTIDENTIFIERACCESSSAFE | JJTIDENTIFIERACCESSJXLT | JJTIDENTIFIERACCESSSAFEJXLT
    )
}

/// Java compares the assign-overload result against the `JexlOperator.ASSIGN` enum constant.
fn is_assign_marker(_v: &Value) -> bool {
    false
}

fn is_null_operand_cause(e: &JexlException) -> bool {
    matches!(e.get_cause().map(|c| c.kind()), Some(ExceptionKind::Java { class }) if class == "JexlArithmetic$NullOperand")
}

fn num_class(c: crate::parser::number_parser::NumClass) -> crate::jexl_arithmetic::NumClass {
    use crate::jexl_arithmetic::NumClass as N;
    use crate::parser::number_parser::NumClass as P;
    match c {
        P::Integer => N::Integer,
        P::Long => N::Long,
        P::Float => N::Float,
        P::Double => N::Double,
        // BigInteger/BigDecimal have no narrowing class in JexlArithmetic
        P::BigInteger => N::Long,
        P::BigDecimal => N::Double,
    }
}
