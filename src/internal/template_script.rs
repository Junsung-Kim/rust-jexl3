// port of: org.apache.commons.jexl3.internal.TemplateScript
//
// A template is compiled into one JEXL script whose verbatim blocks became `jexl:print(n)` calls,
// plus one unified expression per verbatim block parsed in the scope that surrounds its call.
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::internal::frame::create_frame;
use crate::internal::scope::{ScopeId, Scopes};
use crate::internal::template_engine::{Block, BlockType, TemplateEngine, TemplateExpression};
use crate::internal::template_interpreter::{self, TemplateState};
use crate::java::string::{JString, JStringBuilder};
use crate::jexl_context::JexlContext;
use crate::jexl_exception::JexlException;
use crate::jexl_info::JexlInfo;
use crate::parser::jexl_node::{NodeRef, Parsed};
use crate::parser::parser_tree_constants::{JJTARGUMENTS, JJTFUNCTIONNODE};
use crate::value::{HostObject, Value};

/// port of: org.apache.commons.jexl3.internal.TemplateScript (a `JxltEngine.Template`)
pub struct TemplateScript {
    /// port of: TemplateScript.prefix
    prefix: String,
    /// port of: TemplateScript.source
    source: Arc<Vec<Block>>,
    /// port of: TemplateScript.script
    script: Arc<Parsed>,
    /// port of: TemplateScript.exprs — `None` entries are Java's nulls (a prepared `${null}`)
    exprs: Arc<Vec<Option<Arc<TemplateExpression>>>>,
    /// port of: TemplateScript.jxlt
    jxlt: Arc<TemplateEngine>,
}

/// The NPE Java raises when a prepared template's expression slot is null.
fn null_expression() -> JexlException {
    JexlException::java(
        "java.lang.NullPointerException",
        Some("Cannot invoke \"org.apache.commons.jexl3.internal.TemplateEngine$TemplateExpression.asString(java.lang.StringBuilder)\" because \"<local4>\" is null".into()),
    )
}

impl TemplateScript {
    /// port of: TemplateScript(TemplateEngine, JexlInfo, String, Reader, String...)
    pub fn new(
        engine: &Arc<TemplateEngine>,
        info: Option<JexlInfo>,
        directive: &str,
        reader: &JString,
        parms: Option<&[String]>,
    ) -> Result<Arc<TemplateScript>, JexlException> {
        let imm = String::from_utf16_lossy(&[engine.get_immediate_char()]);
        let def = String::from_utf16_lossy(&[engine.get_deferred_char()]);
        if imm == directive
            || def == directive
            || format!("{}{{", imm) == directive
            || format!("{}{{", def) == directive
        {
            return Err(JexlException::java(
                "java.lang.IllegalArgumentException",
                Some(format!("{}: is not a valid directive pattern", directive)),
            ));
        }
        let blocks = engine.read_template(directive, reader);
        // build the script source: one jexl:print(n) per verbatim block, comments for the gaps
        let mut strb: Vec<u16> = Vec::new();
        let mut nuexpr = 0i32;
        let mut line = 1i32;
        for block in &blocks {
            let bl = block.get_line();
            while line < bl {
                strb.extend("//\n".encode_utf16());
                line += 1;
            }
            if block.get_type() == BlockType::Verbatim {
                strb.extend(format!("jexl:print({});\n", nuexpr).encode_utf16());
                nuexpr += 1;
                line += 1;
            } else {
                let body = block.get_body();
                strb.extend_from_slice(body.units());
                line += body.units().iter().filter(|&&c| c == b'\n' as u16).count() as i32;
            }
        }
        let jexl = engine.get_engine();
        let info = info.unwrap_or_else(|| jexl.create_info());
        // allow lambda defining params
        let mut scopes = Scopes::new();
        let scope = parms.map(|p| scopes.create(None, Some(p)));
        let script = jexl.parse_jxlt(
            Some(info.at(1, 1)),
            false,
            &String::from_utf16_lossy(&strb),
            scope.map(|id| (&scopes, id)),
        )?;
        // seek the map of expression number to scope so we can parse Unified expression blocks
        // with the appropriate symbols
        let mut minfo: BTreeMap<i32, JexlInfo> = BTreeMap::new();
        collect_print_scope(&script, script.node(), &mut minfo);
        // jexl:print(...) expression counter
        let mut jpe = 0i32;
        let mut uexprs: Vec<Option<Arc<TemplateExpression>>> = Vec::new();
        for block in &blocks {
            if block.get_type() == BlockType::Verbatim {
                let te = match minfo.get(&jpe) {
                    Some(ji) => {
                        let sc = scope_of(&script, ji);
                        engine.parse_expression(ji, block.get_body(), sc.map(|id| (script.ast.scopes_ref(), id)))?
                    }
                    // no node info means this verbatim is surrounded by comment markers;
                    // the expr at this index is never called
                    None => TemplateExpression::constant(
                        engine.params.clone(),
                        Value::String(block.get_body().clone()),
                        None,
                    ),
                };
                uexprs.push(Some(te));
                jpe += 1;
            }
        }
        Ok(Arc::new(TemplateScript {
            prefix: directive.to_string(),
            source: Arc::new(blocks),
            script,
            exprs: Arc::new(uexprs),
            jxlt: engine.clone(),
        }))
    }

    /// port of: TemplateScript(TemplateEngine, String, Block[], ASTJexlScript, TemplateExpression[])
    fn expanded(&self, exprs: Vec<Option<Arc<TemplateExpression>>>) -> Arc<TemplateScript> {
        Arc::new(TemplateScript {
            prefix: self.prefix.clone(),
            source: self.source.clone(),
            script: self.script.clone(),
            exprs: Arc::new(exprs),
            jxlt: self.jxlt.clone(),
        })
    }

    /// port of: TemplateScript.getScript
    pub(crate) fn get_script(&self) -> &Arc<Parsed> {
        &self.script
    }

    /// port of: TemplateScript.getExpressions
    pub(crate) fn get_expressions(&self) -> &Arc<Vec<Option<Arc<TemplateExpression>>>> {
        &self.exprs
    }

    // port of: TemplateScript.toString
    pub fn java_to_jstring(&self) -> JString {
        let mut strb = JStringBuilder::new();
        for block in self.source.iter() {
            block.to_string_into(&mut strb, &self.prefix);
        }
        strb.build()
    }

    // port of: TemplateScript.asString
    pub fn as_string(&self) -> JString {
        match self.try_as_string() {
            Ok(s) => s,
            Err(_) => JString::empty(),
        }
    }

    /// `asString()` with Java's NPE on a null (prepared-away) expression made explicit.
    pub fn try_as_string(&self) -> Result<JString, JexlException> {
        let mut strb = JStringBuilder::new();
        let mut e = 0usize;
        for block in self.source.iter() {
            if block.get_type() == BlockType::Directive {
                strb.str(&self.prefix).jstr(block.get_body());
            } else {
                match self.exprs.get(e).and_then(|x| x.as_ref()) {
                    Some(x) => x.as_string_into(&mut strb),
                    None => return Err(null_expression()),
                }
                e += 1;
            }
        }
        Ok(strb.build())
    }

    // port of: TemplateScript.getVariables
    pub fn get_variables(&self) -> Vec<Vec<JString>> {
        let mut out: Vec<Vec<JString>> = Vec::new();
        for expr in self.exprs.iter().flatten() {
            expr.collect_variables(&mut out);
        }
        out
    }

    // port of: TemplateScript.getParameters
    pub fn get_parameters(&self) -> Vec<String> {
        self.script.node().get_scope().map(|s| s.get_parameters()).unwrap_or_default()
    }

    // port of: TemplateScript.getPragmas
    pub fn get_pragmas(&self) -> Value {
        let pragmas = self.script.node().script().and_then(|s| s.get_pragmas()).cloned().unwrap_or_default();
        Value::Map(crate::internal::engine::pragmas_as_map(&pragmas))
    }

    // port of: TemplateScript.prepare(JexlContext)
    pub fn prepare(&self, context: Arc<dyn JexlContext>) -> Result<Option<Arc<TemplateScript>>, JexlException> {
        let jexl = self.jxlt.get_engine();
        let options = jexl.options_for_script(&self.script, context.as_ref());
        let frame = self.create_frame(&[], false);
        let mut interpreter = template_interpreter::new_template_interpreter(
            jexl.engine_ref(&options),
            self.script.ast.clone(),
            options,
            context,
            frame,
            Arc::new(TemplateState { exprs: None, writer: None }),
        );
        let mut immediates: Vec<Option<Arc<TemplateExpression>>> = Vec::with_capacity(self.exprs.len());
        for expr in self.exprs.iter() {
            let expr = match expr {
                Some(x) => x,
                None => return Err(null_expression()),
            };
            match expr.prepare_in(&mut interpreter) {
                Ok(v) => immediates.push(v),
                Err(e) if e.is_jexl() => {
                    let xuel = crate::internal::template_engine::create_exception(e.get_info(), "prepare", Some(expr), &e);
                    if jexl.is_silent() {
                        return Ok(None);
                    }
                    return Err(xuel);
                }
                Err(e) => return Err(e),
            }
        }
        Ok(Some(self.expanded(immediates)))
    }

    /// port of: ASTJexlScript.createFrame(Object...)
    fn create_frame(&self, args: &[Value], with_args: bool) -> Option<crate::internal::frame::Frame> {
        let scope = self.script.node().script().and_then(|s| s.get_scope())?;
        create_frame(
            self.script.ast.scopes_ref(),
            scope,
            None,
            if with_args && !args.is_empty() { Some(args) } else { None },
        )
    }

    /// port of: TemplateScript.evaluate(JexlContext, Writer, Object...)
    pub fn evaluate(
        &self,
        context: Arc<dyn JexlContext>,
        writer: Option<Arc<template_interpreter::StringWriter>>,
        args: &[Value],
    ) -> Result<(), JexlException> {
        self.evaluate_value(context, writer.map(|w| Value::Object(w as Arc<dyn HostObject>)), args)
    }

    pub(crate) fn evaluate_value(
        &self,
        context: Arc<dyn JexlContext>,
        writer: Option<Value>,
        args: &[Value],
    ) -> Result<(), JexlException> {
        let jexl = self.jxlt.get_engine();
        let options = jexl.options_for_script(&self.script, context.as_ref());
        let frame = self.create_frame(args, true);
        let mut interpreter = template_interpreter::new_template_interpreter(
            jexl.engine_ref(&options),
            self.script.ast.clone(),
            options,
            context,
            frame,
            Arc::new(TemplateState { exprs: Some(self.exprs.clone()), writer }),
        );
        let node = self.script.node();
        interpreter.interpret(node).map(|_| ())
    }
}

impl HostObject for TemplateScript {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.internal.TemplateScript".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some(self.java_to_jstring().to_rust())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// port of: TemplateScript.scopeOf(JexlNode.Info)
fn scope_of(script: &Arc<Parsed>, info: &JexlInfo) -> Option<ScopeId> {
    let handle = info.node.as_ref()?;
    let mut walk = Some(handle.node());
    while let Some(n) = walk {
        if n.is_script() {
            return n.script().and_then(|s| s.get_scope());
        }
        walk = n.parent();
    }
    let _ = script;
    None
}

/// port of: TemplateScript.collectPrintScope(JexlNode, Map<Integer, JexlNode.Info>)
fn collect_print_scope(parsed: &Arc<Parsed>, node: NodeRef<'_>, minfo: &mut BTreeMap<i32, JexlInfo>) {
    let nc = node.num_children();
    if node.is(JJTFUNCTIONNODE) && nc == 2 {
        // 0 must be the prefix jexl:
        let name_node = node.child(0);
        if let Some(id) = name_node.identifier() {
            if id.get_name() == "print" && id.get_namespace() == Some("jexl") {
                let arg_node = node.child(1);
                if arg_node.is(JJTARGUMENTS) && arg_node.num_children() == 1 {
                    // seek the expression number
                    let arg0 = arg_node.child(0);
                    if let Some(n) = arg0.number() {
                        if let Some(expr_number) = crate::internal::template_engine::int_value(&n.get_literal_value()) {
                            let info = name_node
                                .jexl_info()
                                .unwrap_or_else(|| JexlInfo::new(None, 0, 0))
                                .with_node(crate::parser::jexl_node::NodeHandle::new(parsed.ast.clone(), name_node.id));
                            minfo.insert(expr_number, info);
                            return;
                        }
                    }
                }
            }
        }
    }
    for c in 0..nc {
        collect_print_scope(parsed, node.child(c), minfo);
    }
}
