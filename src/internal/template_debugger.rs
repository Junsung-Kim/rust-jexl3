// port of: org.apache.commons.jexl3.internal.TemplateDebugger
//
// Java subclasses Debugger and overrides `acceptStatement` and `visit(ASTBlock)`; here those two
// overrides are a `DebuggerHook` the base Debugger consults (see `internal::debugger`), and the
// rest -- the `TemplateExpression` visitor and the `$$` / newline insertion -- is below.
use std::sync::Arc;

use crate::internal::debugger::{Debugger, DebuggerHook};
use crate::internal::template_engine::{ExpressionType, TemplateExpression};
use crate::internal::template_script::TemplateScript;
use crate::java::string::JString;
use crate::parser::jexl_node::NodeRef;
use crate::parser::parser_tree_constants::{JJTARGUMENTS, JJTFUNCTIONNODE};

/// port of: org.apache.commons.jexl3.internal.TemplateDebugger
pub struct TemplateDebugger {
    dbg: Debugger,
}

/// The overridden half of the debugger: it holds what `TemplateDebugger.exprs` holds.
struct TemplateHook {
    exprs: Arc<Vec<Option<Arc<TemplateExpression>>>>,
}

impl Default for TemplateDebugger {
    fn default() -> Self {
        TemplateDebugger::new()
    }
}

impl TemplateDebugger {
    // port of: TemplateDebugger()
    pub fn new() -> TemplateDebugger {
        TemplateDebugger { dbg: Debugger::new() }
    }

    // port of: TemplateDebugger.reset
    pub fn reset(&mut self) {
        self.dbg.reset();
    }

    pub fn to_jstring(&self) -> JString {
        self.dbg.to_jstring()
    }

    pub fn start(&self) -> i32 {
        self.dbg.start
    }

    pub fn end(&self) -> i32 {
        self.dbg.end
    }

    pub fn set_indentation(&mut self, level: i32) {
        self.dbg.set_indentation(level);
    }

    /// port of: TemplateDebugger.debug(JxltEngine.Expression)
    pub fn debug_expression(&mut self, je: &Arc<TemplateExpression>) -> bool {
        // `exprs` stays null: the base Debugger behaviour is used for the JEXL nodes
        visit(&mut self.dbg, je);
        true
    }

    /// port of: TemplateDebugger.debug(JxltEngine.Template)
    pub fn debug_template(&mut self, jt: &TemplateScript) -> bool {
        // ensure expr is not null for templates
        let hook = Arc::new(TemplateHook { exprs: jt.get_expressions().clone() });
        let script = jt.get_script().clone();
        self.dbg.start = 0;
        self.dbg.end = 0;
        self.dbg.indent_level = 0;
        self.dbg.builder.clear();
        self.dbg.cause = Some(script.root);
        self.dbg.tmpl = Some(hook);
        let root = script.node();
        for i in 0..root.num_children() {
            self.dbg.accept_statement(root.child(i));
        }
        // the last line
        if !self.dbg.builder.is_empty() && *self.dbg.builder.last().expect("last") != b'\n' as u16 {
            self.dbg.ch('\n');
        }
        self.dbg.end = self.dbg.len();
        self.dbg.tmpl = None;
        self.dbg.end > 0
    }
}

impl DebuggerHook for TemplateHook {
    // port of: TemplateDebugger.acceptStatement
    fn accept_statement(&self, dbg: &mut Debugger, child: NodeRef<'_>) -> bool {
        match self.get_print_statement(child) {
            Some(te) => {
                // if the statement is a jexl:print(...), may need to prepend '\n'
                new_jxlt_line(dbg);
                visit(dbg, &te);
                true
            }
            None => {
                // if the statement is not a jexl:print(...), need to prepend '$$'
                new_jexl_line(dbg);
                false
            }
        }
    }

    // port of: TemplateDebugger.visit(ASTBlock) — before we close this block node, $$ might be needed
    fn close_block(&self, dbg: &mut Debugger) {
        new_jexl_line(dbg);
    }
}

impl TemplateHook {
    /// port of: TemplateDebugger.getPrintStatement — in a template, any statement that is not
    /// 'jexl:print(n)' must be prefixed by "$$".
    fn get_print_statement(&self, child: NodeRef<'_>) -> Option<Arc<TemplateExpression>> {
        if !child.is(JJTFUNCTIONNODE) || child.num_children() < 2 {
            return None;
        }
        let ns = child.child(0);
        let args = child.child(1);
        let id = ns.identifier()?;
        if id.get_namespace() == Some("jexl")
            && id.get_name() == "print"
            && args.is(JJTARGUMENTS)
            && args.num_children() == 1
        {
            let exprn = args.child(0).number()?;
            let n = crate::internal::template_engine::int_value(&exprn.get_literal_value())?;
            if n >= 0 && (n as usize) < self.exprs.len() {
                return self.exprs[n as usize].clone();
            }
        }
        None
    }
}

/// port of: TemplateDebugger.newJexlLine — insert $$ and \n when needed
fn new_jexl_line(dbg: &mut Debugger) {
    let length = dbg.builder.len();
    if length == 0 {
        dbg.s("$$ ");
        return;
    }
    for i in (0..length).rev() {
        match dbg.builder[i] {
            c if c == b'\n' as u16 => {
                dbg.s("$$ ");
                return;
            }
            c if c == b'}' as u16 => {
                dbg.s("\n$$ ");
                return;
            }
            c if c == b' ' as u16 || c == b';' as u16 => return,
            _ => {}
        }
    }
}

/// port of: TemplateDebugger.newJxltLine — insert \n when needed
fn new_jxlt_line(dbg: &mut Debugger) {
    let length = dbg.builder.len();
    for i in (0..length).rev() {
        match dbg.builder[i] {
            c if c == b'\n' as u16 || c == b';' as u16 => return,
            c if c == b'}' as u16 => {
                dbg.ch('\n');
                return;
            }
            _ => {}
        }
    }
}

/// port of: TemplateDebugger.visit(TemplateExpression, Object)
fn visit(dbg: &mut Debugger, expr: &Arc<TemplateExpression>) {
    match expr.get_type() {
        // port of: visit(ConstantExpression)
        ExpressionType::Constant => {
            let mut sb = crate::java::string::JStringBuilder::new();
            expr.as_string_into(&mut sb);
            dbg.u(sb.build().units());
        }
        // port of: visit(ImmediateExpression) / visit(DeferredExpression)
        ExpressionType::Immediate | ExpressionType::Deferred => {
            dbg.ch(if expr.is_immediate() { '$' } else { '#' });
            dbg.ch('{');
            if let Some(node) = expr.node() {
                dbg.accept(node.node());
            }
            dbg.ch('}');
        }
        // port of: visit(NestedExpression)
        ExpressionType::Nested => {
            if let Some(node) = expr.node() {
                dbg.accept(node.node());
            }
        }
        // port of: visit(CompositeExpression)
        ExpressionType::Composite => {
            if let Some(parts) = expr.composite_parts() {
                for ce in parts {
                    visit(dbg, ce);
                }
            }
        }
    }
}
