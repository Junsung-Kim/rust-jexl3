// port of: org.apache.commons.jexl3.parser.ASTIdentifierAccess (and the Safe / Jxlt variants' data)
use std::sync::OnceLock;

use crate::java::string::JString;
use crate::value::Value;

#[derive(Default)]
pub struct ASTIdentifierAccess {
    pub(crate) name: JString,
    identifier: Option<i32>,
    /// ASTIdentifierAccessJxlt.jxltExpr (set lazily by the interpreter)
    pub(crate) jxlt_expr: OnceLock<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
}

impl ASTIdentifierAccess {
    // port of: ASTIdentifierAccess.setIdentifier
    pub(crate) fn set_identifier(&mut self, id: JString) {
        self.identifier = Self::parse_identifier(&id);
        self.name = id;
    }

    // port of: ASTIdentifierAccess.parseIdentifier: a non-negative decimal without leading zeros
    pub fn parse_identifier(id: &JString) -> Option<i32> {
        let units = id.units();
        let length = units.len();
        let mut val: i32 = 0;
        for &c in units {
            if c == b'0' as u16 {
                if length == 1 {
                    return Some(0);
                }
                if val == 0 {
                    return None;
                }
            } else if !(b'0' as u16..=b'9' as u16).contains(&c) {
                return None;
            }
            val = val.wrapping_mul(10);
            val = val.wrapping_add((c - b'0' as u16) as i32);
        }
        Some(val)
    }

    // port of: ASTIdentifierAccess.getIdentifier: the Integer index or the String name
    pub fn get_identifier(&self) -> Value {
        match self.identifier {
            Some(i) => Value::Integer(i),
            None => Value::String(self.name.clone()),
        }
    }

    pub fn get_name(&self) -> &JString {
        &self.name
    }
}
