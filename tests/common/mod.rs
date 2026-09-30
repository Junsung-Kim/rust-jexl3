pub mod decode;
pub mod encode;
pub mod hosts;
pub mod json;
pub mod upstream;

use json::Json;
use rust_jexl::java::big_decimal::{MathContext, RoundingMode};

/// Parses the oracle's math-context spelling ("DECIMAL64", "5:HALF_UP", ...).
pub fn math_context(spec: &str) -> MathContext {
    match spec {
        "DECIMAL32" => MathContext::DECIMAL32,
        "DECIMAL64" => MathContext::DECIMAL64,
        "UNLIMITED" => MathContext::UNLIMITED,
        "DECIMAL128" => MathContext::DECIMAL128,
        other => {
            let (p, r) = other.split_once(':').expect("precision:ROUNDING");
            MathContext { precision: p.parse().expect("precision"), rounding_mode: RoundingMode::value_of(r).expect("rounding") }
        }
    }
}

/// Masks Java identity hashes (`ClassName@1a2b3c`), which are nondeterministic.
pub fn normalize(v: &Json) -> Json {
    match v {
        Json::Str(s) => {
            let text = String::from_utf16_lossy(s);
            if !text.contains('@') {
                return v.clone();
            }
            let mut out = String::new();
            let mut it = text.chars().peekable();
            while let Some(c) = it.next() {
                out.push(c);
                if c == '@' {
                    let mut n = 0;
                    while it.peek().map(|c| c.is_ascii_hexdigit()).unwrap_or(false) {
                        it.next();
                        n += 1;
                    }
                    if n > 0 {
                        out.push_str("ID");
                    }
                }
            }
            Json::str(&out)
        }
        Json::Arr(a) => Json::Arr(a.iter().map(normalize).collect()),
        Json::Obj(kv) => Json::Obj(kv.iter().map(|(k, x)| (k.clone(), normalize(x))).collect()),
        other => other.clone(),
    }
}
