// port of: org.apache.commons.jexl3.internal.TemplateInterpreter
//
// Java subclasses Interpreter to add four overrides (`resolveNamespace`, `visit(ASTIdentifier)`,
// `visit(ASTFunctionNode)` and the closure factory in `visit(ASTJexlScript)`) plus two fields.
// Rust has no inheritance, so the fields live on `Interpreter.tmpl` and the overrides are the
// free functions below, which `Interpreter` calls at the same three points.
//
// `resolveNamespace("jexl")` is not ported: it can only be reached from `visit(ASTFunctionNode)`,
// and that override already handles every `jexl:` call before the namespace is resolved (a
// function node always has exactly two children in the 3.2.1 grammar).
use std::sync::{Arc, Mutex};

use crate::internal::frame::Frame;
use crate::internal::interpreter::{EngineRef, Interpreter};
use crate::internal::lexical_frame::LexicalFrame;
use crate::internal::template_engine::{self, TemplateExpression};
use crate::internal::template_script::TemplateScript;
use crate::java::string::JString;
use crate::jexl_context::JexlContext;
use crate::jexl_exception::JexlException;
use crate::jexl_info::JexlInfo;
use crate::jexl_options::JexlOptions;
use crate::parser::ast_identifier_access::ASTIdentifierAccess;
use crate::parser::jexl_node::{Ast, NodeRef, Parsed};
use crate::parser::parser_tree_constants::JJTARGUMENTS;
use crate::value::{HostObject, Value};

/// port of: TemplateInterpreter's two fields (`exprs` and `writer`).
/// `exprs: None` is Java's null array: only `TemplateScript.evaluate` ever supplies one.
pub struct TemplateState {
    pub exprs: Option<Arc<Vec<Option<Arc<TemplateExpression>>>>>,
    /// the `Writer`, exposed to scripts as `$jexl`
    pub writer: Option<Value>,
}

/// port of: java.io.StringWriter — the sink `JxltEngine.Template.evaluate` writes into.
pub struct StringWriter {
    buf: Mutex<Vec<u16>>,
}

impl StringWriter {
    pub fn new() -> Arc<StringWriter> {
        Arc::new(StringWriter { buf: Mutex::new(Vec::new()) })
    }

    pub fn write(&self, s: &JString) {
        self.buf.lock().unwrap_or_else(|p| p.into_inner()).extend_from_slice(s.units());
    }

    pub fn to_jstring(&self) -> JString {
        JString::from_units(&self.buf.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

impl HostObject for StringWriter {
    fn class_name(&self) -> String {
        "java.io.StringWriter".into()
    }
    // port of: StringWriter.toString — the buffer's current content
    fn java_to_string(&self) -> Option<String> {
        Some(self.to_jstring().to_rust())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// port of: TemplateInterpreter(Arguments) — an Interpreter that also carries `exprs` and
/// `writer`, and whose block is opened by the constructor.
pub(crate) fn new_template_interpreter(
    jexl: Arc<EngineRef>,
    ast: Arc<Ast>,
    options: JexlOptions,
    context: Arc<dyn JexlContext>,
    frame: Option<Frame>,
    state: Arc<TemplateState>,
) -> Interpreter {
    let mut it = Interpreter::new(jexl, ast, options, context, frame.clone());
    it.block = Some(LexicalFrame::new(frame));
    it.tmpl = Some(state);
    it
}

/// Interprets a tree that is not the interpreter's own: every unified expression is its own parse,
/// while the frame, the context and the lexical state stay the template's. The scope arena was
/// copied into the sub-parse (see `Parser::parse_in_scope`), so symbol numbers still agree.
pub(crate) fn interpret_parsed(it: &mut Interpreter, parsed: &Arc<Parsed>) -> Result<Value, JexlException> {
    let saved = std::mem::replace(&mut it.ast, parsed.ast.clone());
    let node = parsed.node();
    let result = it.interpret(node);
    it.ast = saved;
    result
}

// ------------------------------------------------------------------- the Interpreter overrides

/// port of: TemplateInterpreter.visit(ASTJexlScript) — unlike `Interpreter.visit(ASTJexlScript)`
/// it does *not* open a LexicalFrame around the statements. That matters: popping one undefines
/// its symbols in the frame, which would wipe the template's parameters between expressions.
/// `None` means "not a template interpreter, carry on with Interpreter.visit".
pub(crate) fn visit_script(
    it: &mut Interpreter,
    node: NodeRef<'_>,
    data: Option<&Value>,
) -> Option<Result<Value, JexlException>> {
    it.tmpl.as_ref()?;
    let mut result = Value::Null;
    for i in 0..node.num_children() {
        let child = node.child(i);
        match it.accept_node(child, data) {
            Ok(v) => result = v,
            Err(e) => return Some(Err(e)),
        }
        if let Err(e) = it.cancel_check_node(child) {
            return Some(Err(e));
        }
    }
    Some(Ok(result))
}

/// port of: TemplateInterpreter.visit(ASTIdentifier) — `$jexl` is the writer
pub(crate) fn visit_identifier(it: &Interpreter, node: NodeRef<'_>) -> Option<Value> {
    let state = it.tmpl.as_ref()?;
    if node.identifier()?.get_name() == "$jexl" {
        return Some(state.writer.clone().unwrap_or(Value::Null));
    }
    None
}

/// port of: TemplateInterpreter.visit(ASTFunctionNode) — print() and include() must be decoded
/// here since delegating to the Uberspect may be sandboxing the interpreter itself.
/// `None` means "not a template function, carry on with Interpreter.visit".
pub(crate) fn visit_function(it: &mut Interpreter, node: NodeRef<'_>) -> Option<Result<Value, JexlException>> {
    it.tmpl.as_ref()?;
    if node.num_children() != 2 {
        return None;
    }
    let function_node = node.child(0);
    let id = function_node.identifier()?;
    if id.get_namespace() != Some("jexl") {
        return None;
    }
    let function_name = id.get_name().to_string();
    let arg_node = node.child(1);
    if !arg_node.is(JJTARGUMENTS) {
        return None;
    }
    if function_name == "print" {
        let argv = match it.visit_arguments(arg_node, None) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        if let Some(n) = argv.first().and_then(template_engine::int_value) {
            return Some(print(it, n).map(|()| Value::Null));
        }
    }
    if function_name == "include" {
        let argv = match it.visit_arguments(arg_node, None) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        if let Some(first) = argv.first() {
            if let Some(script) = first.as_host::<TemplateScript>() {
                let rest: Vec<Value> = argv[1..].to_vec();
                return Some(include(it, script, &rest).map(|()| Value::Null));
            }
        }
    }
    // fail safe
    Some(Err(JexlException::jxlt(
        node.jexl_info(),
        &format!("no callable template function {}", function_name),
        None,
    )))
}

/// port of: TemplateInterpreter.print(int)
pub(crate) fn print(it: &mut Interpreter, e: i32) -> Result<(), JexlException> {
    let state = match it.tmpl.clone() {
        Some(s) => s,
        None => return Ok(()),
    };
    let exprs = match &state.exprs {
        // Java would dereference a null array here
        None => {
            return Err(JexlException::java(
                "java.lang.NullPointerException",
                Some("Cannot read the array length because \"this.exprs\" is null".into()),
            ))
        }
        Some(x) => x.clone(),
    };
    if e < 0 || e as usize >= exprs.len() {
        return Ok(());
    }
    let mut expr = match &exprs[e as usize] {
        Some(x) => x.clone(),
        None => {
            return Err(JexlException::java(
                "java.lang.NullPointerException",
                Some("Cannot invoke \"org.apache.commons.jexl3.internal.TemplateEngine$TemplateExpression.getInfo()\" because \"expr\" is null".into()),
            ))
        }
    };
    if expr.is_deferred() {
        match expr.prepare_frame(it.ast.clone(), it.frame.clone(), it.context.clone())? {
            Some(p) => expr = p,
            // a silent engine returns null from prepare; Java then dereferences it
            None => {
                return Err(JexlException::java(
                    "java.lang.NullPointerException",
                    Some("Cannot invoke \"org.apache.commons.jexl3.internal.TemplateEngine$TemplateExpression.getInfo()\" because \"expr\" is null".into()),
                ))
            }
        }
    }
    if expr.get_type() == template_engine::ExpressionType::Composite {
        // port of: TemplateInterpreter.printComposite
        for cexpr in expr.composite_parts().expect("composite") {
            let value = cexpr.evaluate_in(it)?;
            do_print(it, cexpr.get_info(), &value)?;
        }
        Ok(())
    } else {
        let value = expr.evaluate_in(it)?;
        do_print(it, expr.get_info(), &value)
    }
}

/// port of: TemplateInterpreter.include(JxltEngine.Template, Object...)
fn include(it: &mut Interpreter, script: &TemplateScript, args: &[Value]) -> Result<(), JexlException> {
    let writer = it.tmpl.as_ref().and_then(|s| s.writer.clone());
    script.evaluate_value(it.context.clone(), writer, args)
}

/// port of: TemplateInterpreter.doPrint(JexlInfo, Object)
fn do_print(it: &Interpreter, info: Option<JexlInfo>, arg: &Value) -> Result<(), JexlException> {
    let writer = match it.tmpl.as_ref().and_then(|s| s.writer.clone()) {
        Some(w) => w,
        None => return Ok(()),
    };
    let sink = writer.as_host::<StringWriter>();
    if let Value::String(s) = arg {
        if let Some(w) = sink {
            w.write(s);
        }
        return Ok(());
    }
    if arg.is_null() {
        return Ok(());
    }
    let value = [arg.clone()];
    if let Some(method) = it.uberspect.get_method(&writer, "print", &value) {
        return match method.invoke(&writer, &value) {
            Ok(_) => Ok(()),
            Err(e) => Err(template_engine::create_exception(info, "invoke print", None, &e)),
        };
    }
    if let Some(w) = sink {
        w.write(&arg.java_to_jstring());
    }
    Ok(())
}

// ------------------------------------------------------------------ the two Interpreter stubs

/// port of: Interpreter.visit(ASTJxltLiteral) — a backtick literal
pub(crate) fn visit_jxlt_literal(it: &mut Interpreter, node: NodeRef<'_>) -> Result<Value, JexlException> {
    // ponytail: Java memoizes the parsed expression in the node (jjtSetValue); the node's value
    // slot is shared with JexlInfo and the operator cache here, and the memo is not observable,
    // so the expression is re-parsed instead.
    let jxlt = it.jexl.engine.jxlt();
    let mut info = node.jexl_info().unwrap_or_else(|| JexlInfo::new(None, 0, 0));
    if it.block.is_some() {
        info = info.with_node(it.handle(node));
    }
    let literal = node.literal().cloned().unwrap_or_else(JString::empty);
    let scopes = it.ast.clone();
    let scope = it.frame.as_ref().map(|f| (scopes.scopes_ref(), f.scope()));
    let tp = jxlt.parse_expression(&info, &literal, scope)?;
    tp.evaluate_frame(it.ast.clone(), it.frame.clone(), it.context.clone())
}

/// port of: Interpreter.evalIdentifier's ASTIdentifierAccessJxlt branch
pub(crate) fn eval_identifier_jxlt(it: &mut Interpreter, node: NodeRef<'_>) -> Result<Value, JexlException> {
    let src = node.identifier_access().expect("access").get_name().clone();
    let jxlt = it.jexl.engine.jxlt();
    let info = node.jexl_info().unwrap_or_else(|| JexlInfo::new(None, 0, 0));
    let scopes = it.ast.clone();
    let scope = it.frame.as_ref().map(|f| (scopes.scopes_ref(), f.scope()));
    let mut cause: Option<JexlException> = None;
    match jxlt.parse_expression(&info, &src, scope) {
        Err(e) => {
            // only a JxltEngine.Exception is caught; a parse error propagates
            if !e.is_jxlt() {
                return Err(e);
            }
            cause = Some(e);
        }
        Ok(expr) => match expr.evaluate_frame(it.ast.clone(), it.frame.clone(), it.context.clone()) {
            Err(e) => {
                if !e.is_jxlt() {
                    return Err(e);
                }
                cause = Some(e);
            }
            Ok(name) => {
                if !name.is_null() {
                    let text = name.java_to_jstring();
                    return Ok(match ASTIdentifierAccess::parse_identifier(&text) {
                        Some(id) => Value::Integer(id),
                        None => name,
                    });
                }
            }
        },
    }
    if node.is_safe() {
        return Ok(Value::Null);
    }
    it.unsolvable_property(node, &src.to_rust(), true, cause)
}
