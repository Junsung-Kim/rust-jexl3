// port of: org.apache.commons.jexl3.internal.Script and org.apache.commons.jexl3.internal.Closure
use std::any::Any;
use std::sync::Arc;

use crate::internal::frame::Frame;
use crate::internal::interpreter::Interpreter;
use crate::jexl_exception::JexlException;
use crate::parser::jexl_node::{Ast, NodeId};
use crate::value::{HostObject, Value};

/// port of: Closure — a lambda that captured its enclosing frame.
pub struct Closure {
    pub ast: Arc<Ast>,
    /// the ASTJexlLambda node
    pub script: NodeId,
    pub frame: Option<Frame>,
}

impl Closure {
    // port of: Closure(Interpreter, ASTJexlLambda)
    pub fn new(ast: Arc<Ast>, script: NodeId, frame: Option<Frame>) -> Closure {
        Closure { ast, script, frame }
    }

    pub fn arg_count(&self, ast: &Ast) -> i32 {
        ast.node(self.script).get_scope().map(|s| s.get_arg_count()).unwrap_or(0)
    }

    // port of: Closure.setCaptured
    pub fn set_captured(&self, ast: &Ast, symbol: i32, value: Value) {
        if let (Some(scope), Some(frame)) = (ast.node(self.script).get_scope(), &self.frame) {
            if let Some(reg) = scope.get_captured(symbol) {
                frame.set(reg as usize, crate::internal::frame::Slot::Value(value));
            }
        }
    }

    // port of: Closure.execute(JexlContext, Object...)
    pub fn execute(&self, caller: &Interpreter, args: &[Value]) -> Result<Value, JexlException> {
        let scope_id = self.ast.node(self.script).script().and_then(|s| s.get_scope());
        let local = match (&self.frame, scope_id) {
            (Some(f), Some(sid)) => Some(f.assign(self.ast.scopes_ref().get(sid), Some(args))),
            _ => None,
        };
        let mut it = caller.fork(self.ast.clone(), local);
        it.run_closure(self)
    }
}

impl HostObject for Closure {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.internal.Closure".into()
    }

    // port of: Script.toString — the Debugger rendering when there is no source
    fn java_to_string(&self) -> Option<String> {
        crate::internal::interpreter::debug_render(self.ast.node(self.script), 0)
    }

    // port of: Closure.equals
    fn java_equals(&self, other: &Value) -> Option<bool> {
        match other.as_host::<Closure>() {
            Some(o) => Some(
                Arc::ptr_eq(&self.ast, &o.ast)
                    && self.script == o.script
                    && match (&self.frame, &o.frame) {
                        (None, None) => true,
                        (Some(a), Some(b)) => a.java_equals(b),
                        _ => false,
                    },
            ),
            None => Some(false),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
