// port of: org.apache.commons.jexl3.internal.LexicalFrame
use crate::internal::frame::{Frame, Slot};
use crate::internal::lexical_scope::LexicalScope;

/// The set of symbols declared by a lexical unit, and the values it shadowed.
#[derive(Clone, Debug, Default)]
pub struct LexicalFrame {
    scope: LexicalScope,
    frame: Option<Frame>,
    /// the captured values to restore on pop, as a stack of (symbol, value)
    stack: Vec<(i32, Slot)>,
}

impl LexicalFrame {
    // port of: LexicalFrame(Frame, LexicalFrame)
    pub fn new(script_frame: Option<Frame>) -> LexicalFrame {
        LexicalFrame { scope: LexicalScope::new(), frame: script_frame, stack: Vec::new() }
    }

    pub fn scope(&self) -> &LexicalScope {
        &self.scope
    }

    pub fn has_symbol(&self, symbol: i32) -> bool {
        self.scope.has_symbol(symbol)
    }

    // port of: LexicalFrame.defineArgs
    pub fn define_args(mut self, argc: i32) -> LexicalFrame {
        for a in 0..argc {
            self.scope.add_symbol(a);
        }
        self
    }

    // port of: LexicalFrame.defineSymbol
    pub fn define_symbol(&mut self, symbol: i32, capture: bool) -> bool {
        let declared = self.scope.add_symbol(symbol);
        if declared && capture {
            if let Some(frame) = &self.frame {
                // Java pushes the symbol then the value; the value `null` is marked by `this`
                self.stack.push((symbol, frame.get(symbol as usize)));
            }
        }
        declared
    }

    /// port of: LexicalFrame.pop — undefines this unit's symbols and restores the captured ones
    pub fn pop(&mut self) {
        if let Some(frame) = &self.frame {
            let frame = frame.clone();
            self.scope.clear_symbols(|s| frame.set(s as usize, Slot::Undefined));
            while let Some((symbol, value)) = self.stack.pop() {
                let restored = match value {
                    Slot::Undeclared => Slot::Undefined,
                    other => other,
                };
                frame.set(symbol as usize, restored);
            }
        } else {
            self.scope.clear_symbols(|_| {});
            self.stack.clear();
        }
    }
}
