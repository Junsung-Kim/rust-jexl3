// port of: org.apache.commons.jexl3.parser.NumberParser
use std::sync::Arc;

use crate::java::big_decimal::{BigDecimal, RoundingMode};
use crate::java::number;
use crate::jexl_exception::JexlException;
use crate::value::Value;

/// The Java class of a number literal (NumberParser.clazz).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumClass {
    Integer,
    Long,
    BigInteger,
    Float,
    Double,
    BigDecimal,
}

impl NumClass {
    pub fn simple_name(&self) -> &'static str {
        match self {
            NumClass::Integer => "Integer",
            NumClass::Long => "Long",
            NumClass::BigInteger => "BigInteger",
            NumClass::Float => "Float",
            NumClass::Double => "Double",
            NumClass::BigDecimal => "BigDecimal",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NumberParser {
    literal: Option<Value>,
    clazz: Option<NumClass>,
}

fn nfe(e: number::NumberFormatException) -> JexlException {
    JexlException::java("java.lang.NumberFormatException", Some(e.0))
}

fn math(e: crate::java::big_decimal::MathError) -> JexlException {
    match e {
        crate::java::big_decimal::MathError::NumberFormat(m) => JexlException::java_msg("java.lang.NumberFormatException", Some(m)),
        crate::java::big_decimal::MathError::Arithmetic(m) => JexlException::java_msg("java.lang.ArithmeticException", Some(m)),
    }
}

impl NumberParser {
    // port of: NumberParser.toString
    pub fn to_java_string(&self) -> String {
        let (lit, clazz) = match (&self.literal, self.clazz) {
            (Some(l), Some(c)) => (l, c),
            _ => return "NaN".into(),
        };
        match lit {
            Value::Double(d) if d.is_nan() => return "NaN".into(),
            Value::Float(f) if f.is_nan() => return "NaN".into(),
            _ => {}
        }
        if clazz == NumClass::BigDecimal {
            // DecimalFormat("0.0b"): one fraction digit, HALF_EVEN, no grouping
            if let Value::BigDecimal(b) = lit {
                return match b.set_scale(1, RoundingMode::HalfEven) {
                    Ok(r) => format!("{}b", r.to_plain_string()),
                    Err(_) => format!("{}b", b.to_plain_string()),
                };
            }
        }
        let mut strb = lit.java_to_string();
        match clazz {
            NumClass::Float => strb.push('f'),
            NumClass::Double => strb.push('d'),
            NumClass::BigInteger => strb.push('h'),
            NumClass::Long => strb.push('l'),
            _ => {}
        }
        strb
    }

    pub fn get_literal_class(&self) -> Option<NumClass> {
        self.clazz
    }

    pub fn is_integer(&self) -> bool {
        self.clazz == Some(NumClass::Integer)
    }

    pub fn get_literal_value(&self) -> Value {
        self.literal.clone().unwrap_or(Value::Null)
    }

    // port of: NumberParser.parseInteger
    pub fn parse_integer(s: &str) -> Result<Value, JexlException> {
        let mut np = NumberParser::default();
        np.set_natural(s)?;
        Ok(np.get_literal_value())
    }

    // port of: NumberParser.parseDouble
    pub fn parse_double(s: &str) -> Result<Value, JexlException> {
        let mut np = NumberParser::default();
        np.set_real(s)?;
        Ok(np.get_literal_value())
    }

    // port of: NumberParser.setNatural
    pub fn set_natural(&mut self, s: &str) -> Result<(), JexlException> {
        let mut s = s;
        let base: u32 = if s.starts_with('0') {
            if s.len() > 1 && (s.as_bytes()[1] == b'x' || s.as_bytes()[1] == b'X') {
                s = &s[2..];
                16
            } else {
                8
            }
        } else {
            10
        };
        let last = s.len() - 1;
        let (result, rclass) = match s.as_bytes()[last] {
            b'l' | b'L' => (Value::Long(number::parse_long(&s[..last], base).map_err(nfe)?), NumClass::Long),
            b'h' | b'H' => (Value::BigInteger(Arc::new(number::parse_big_integer(&s[..last], base).map_err(nfe)?)), NumClass::BigInteger),
            _ => {
                let v = match number::parse_int(s, base) {
                    Ok(i) => Value::Integer(i),
                    Err(_) => match number::parse_long(s, base) {
                        Ok(l) => Value::Long(l),
                        Err(_) => Value::BigInteger(Arc::new(number::parse_big_integer(s, base).map_err(nfe)?)),
                    },
                };
                (v, NumClass::Integer)
            }
        };
        self.literal = Some(result);
        self.clazz = Some(rclass);
        Ok(())
    }

    // port of: NumberParser.setReal
    pub fn set_real(&mut self, s: &str) -> Result<(), JexlException> {
        let (result, rclass) = if s == "#NaN" || s == "NaN" {
            (Value::Double(f64::NAN), NumClass::Double)
        } else {
            let last = s.len() - 1;
            match s.as_bytes()[last] {
                b'b' | b'B' => (Value::BigDecimal(Arc::new(BigDecimal::parse(&s[..last]).map_err(math)?)), NumClass::BigDecimal),
                b'f' | b'F' => (Value::Float(number::parse_float(&s[..last]).map_err(nfe)?), NumClass::Float),
                b'd' | b'D' => (Value::Double(number::parse_double(&s[..last]).map_err(nfe)?), NumClass::Double),
                _ => {
                    let v = match number::parse_double(s) {
                        Ok(d) => Value::Double(d),
                        Err(_) => Value::BigDecimal(Arc::new(BigDecimal::parse(s).map_err(math)?)),
                    };
                    (v, NumClass::Double)
                }
            }
        };
        self.literal = Some(result);
        self.clazz = Some(rclass);
        Ok(())
    }
}
