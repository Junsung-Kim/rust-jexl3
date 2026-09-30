#![allow(clippy::redundant_closure, dead_code)] // helpers shared by suites that use different subsets
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
        Json::Obj(kv) => {
            let out: Vec<(String, Json)> = kv.iter().map(|(k, x)| (k.clone(), normalize(x))).collect();
            let obj = Json::Obj(out);
            if unordered(&obj) {
                return sort_elements(&obj);
            }
            obj
        }
        other => other.clone(),
    }
}

/// Whether this encoded collection's iteration order is not reproducible.
///
/// `java.util.HashMap` and `HashSet` iterate in hash order, and an object that does not override
/// `hashCode` hashes by identity — a number the JVM picks per object, per run. Two runs of the
/// *same* Java program disagree, so there is nothing here for the port to match. Insertion-ordered
/// collections (`LinkedHashMap`, `LinkedHashSet`) and hash-ordered ones holding only values with a
/// defined `hashCode` stay ordered and stay compared.
fn unordered(v: &Json) -> bool {
    let class = match v.get("c").and_then(Json::string) {
        Some(c) => c,
        None => return false,
    };
    if !class.contains("Hash") || class.contains("Linked") {
        return false;
    }
    match v.get("v") {
        Some(Json::Arr(items)) => items.iter().any(identity_hashed),
        _ => false,
    }
}

/// An encoded value whose Java `hashCode` is its identity hash.
fn identity_hashed(v: &Json) -> bool {
    match v {
        Json::Obj(_) => match v.get("t").and_then(Json::string).as_deref() {
            // a host object or a script: neither overrides hashCode
            Some("Object") | Some("Script") | Some("Host") => true,
            _ => matches!(v.get("v"), Some(Json::Arr(items)) if items.iter().any(identity_hashed)),
        },
        Json::Arr(items) => items.iter().any(identity_hashed),
        _ => false,
    }
}

fn sort_elements(v: &Json) -> Json {
    let Json::Obj(kv) = v else { return v.clone() };
    Json::Obj(
        kv.iter()
            .map(|(k, x)| {
                if k != "v" {
                    return (k.clone(), x.clone());
                }
                match x {
                    Json::Arr(items) => {
                        let mut sorted = items.clone();
                        sorted.sort_by_key(|i| json::to_string(i));
                        (k.clone(), Json::Arr(sorted))
                    }
                    other => (k.clone(), other.clone()),
                }
            })
            .collect(),
    )
}
