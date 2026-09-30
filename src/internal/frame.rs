// port of: org.apache.commons.jexl3.internal.Frame
use std::sync::{Arc, RwLock};

use crate::internal::scope::{Scope, ScopeId};
use crate::value::Value;

/// A slot of the frame stack: Java fills unassigned slots with the `Scope.UNDECLARED` sentinel.
#[derive(Clone, Debug)]
pub enum Slot {
    /// Scope.UNDECLARED, printed as "??"
    Undeclared,
    /// Scope.UNDEFINED, printed as "?"
    Undefined,
    Value(Value),
}

impl Slot {
    pub fn value(&self) -> Option<&Value> {
        match self {
            Slot::Value(v) => Some(v),
            _ => None,
        }
    }
}

/// The local variables of one script or closure activation. Java mutates the array in place and
/// shares it with the closures created while it runs, so the stack is behind a lock.
#[derive(Clone, Debug)]
pub struct Frame {
    scope: ScopeId,
    stack: Arc<RwLock<Vec<Slot>>>,
    curried: usize,
}

impl Frame {
    // port of: Frame(Scope, Object[], int)
    pub fn new(scope: ScopeId, stack: Vec<Slot>, curried: usize) -> Frame {
        Frame { scope, stack: Arc::new(RwLock::new(stack)), curried }
    }

    pub fn scope(&self) -> ScopeId {
        self.scope
    }

    pub fn curried(&self) -> usize {
        self.curried
    }

    // port of: Frame.getUnboundParameters
    pub fn get_unbound_parameters(&self, scope: &Scope) -> Vec<String> {
        scope.get_parameters_bound(self.curried as i32)
    }

    // port of: Frame.get
    pub fn get(&self, s: usize) -> Slot {
        self.stack.read().unwrap_or_else(|p| p.into_inner()).get(s).cloned().unwrap_or(Slot::Undeclared)
    }

    // port of: Frame.has
    pub fn has(&self, s: i32) -> bool {
        if s < 0 {
            return false;
        }
        let stack = self.stack.read().unwrap_or_else(|p| p.into_inner());
        match stack.get(s as usize) {
            Some(Slot::Undeclared) | None => false,
            Some(_) => true,
        }
    }

    // port of: Frame.set
    pub fn set(&self, r: usize, value: Slot) {
        let mut stack = self.stack.write().unwrap_or_else(|p| p.into_inner());
        if r < stack.len() {
            stack[r] = value;
        }
    }

    pub fn len(&self) -> usize {
        self.stack.read().unwrap_or_else(|p| p.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// port of: Frame.assign — a *copy* of the stack with the arguments bound
    pub fn assign(&self, scope: &Scope, values: Option<&[Value]>) -> Frame {
        let stack = self.stack.read().unwrap_or_else(|p| p.into_inner());
        let nparm = scope.get_arg_count().max(0) as usize;
        let mut copy = stack.clone();
        let mut ncopy = 0usize;
        if let Some(values) = values {
            if !values.is_empty() {
                ncopy = (nparm.saturating_sub(self.curried)).min(nparm).min(values.len());
                for (i, v) in values.iter().take(ncopy).enumerate() {
                    if self.curried + i < copy.len() {
                        copy[self.curried + i] = Slot::Value(v.clone());
                    }
                }
            }
        }
        // Arrays.fill(copy, curried + ncopy, nparm, null)
        for slot in copy.iter_mut().take(nparm).skip(self.curried + ncopy) {
            *slot = Slot::Value(Value::Null);
        }
        Frame { scope: self.scope, stack: Arc::new(RwLock::new(copy)), curried: self.curried + ncopy }
    }

    // port of: Frame.hashCode (Arrays.deepHashCode)
    pub fn java_hash_code(&self) -> i32 {
        let stack = self.stack.read().unwrap_or_else(|p| p.into_inner());
        stack.iter().fold(1i32, |h, s| {
            let e = match s {
                Slot::Value(v) => v.java_hash_code(),
                _ => 0,
            };
            h.wrapping_mul(31).wrapping_add(e)
        })
    }

    // port of: Frame.equals (Arrays.deepEquals)
    pub fn java_equals(&self, other: &Frame) -> bool {
        let a = self.stack.read().unwrap_or_else(|p| p.into_inner());
        let b = other.stack.read().unwrap_or_else(|p| p.into_inner());
        a.len() == b.len()
            && a.iter().zip(b.iter()).all(|(x, y)| match (x, y) {
                (Slot::Value(p), Slot::Value(q)) => p.java_equals(q),
                (Slot::Undeclared, Slot::Undeclared) | (Slot::Undefined, Slot::Undefined) => true,
                _ => false,
            })
    }
}

/// port of: Scope.createFrame(Frame, Object...)
pub fn create_frame(scopes: &crate::internal::scope::Scopes, scope: ScopeId, caller: Option<&Frame>, args: Option<&[Value]>) -> Option<Frame> {
    let s = scopes.get(scope);
    let size = s.named_count()?;
    let mut arguments = vec![Slot::Undeclared; size];
    if let Some(caller) = caller {
        if !s.captured_variables().is_empty() && s.parent().is_some() {
            for (target, source) in s.captured_variables() {
                arguments[*target as usize] = caller.get(*source as usize);
            }
        }
    }
    Some(Frame::new(scope, arguments, 0).assign(s, args))
}
