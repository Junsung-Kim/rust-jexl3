// Typed value encoding shared with the Java oracle (Oracle.encode); see oracle/PROTOCOL.md.
#![allow(dead_code)]

use rust_jexl::value::Value;

use super::json::Json;

fn typed(t: &str, v: Option<Json>) -> Json {
    let mut kv = vec![("t".to_string(), Json::str(t))];
    if let Some(v) = v {
        kv.push(("v".to_string(), v));
    }
    Json::Obj(kv)
}

fn with(mut o: Json, key: &str, v: Json) -> Json {
    if let Json::Obj(kv) = &mut o {
        kv.push((key.to_string(), v));
    }
    o
}

/// Encodes a value exactly as the Java oracle does.
pub fn encode(v: &Value) -> Json {
    match v {
        Value::Null => typed("null", None),
        Value::Boolean(b) => typed("Boolean", Some(Json::str(if *b { "true" } else { "false" }))),
        Value::Byte(_) | Value::Short(_) | Value::Integer(_) | Value::Long(_) | Value::BigInteger(_) | Value::BigDecimal(_) => {
            typed(&v.simple_name(), Some(Json::str(&v.java_to_string())))
        }
        Value::Double(d) => with(
            typed("Double", Some(Json::str(&v.java_to_string()))),
            "bits",
            Json::str(&format!("{:x}", rust_jexl::value::double_bits(*d))),
        ),
        Value::Float(f) => with(
            typed("Float", Some(Json::str(&v.java_to_string()))),
            "bits",
            Json::str(&format!("{:x}", rust_jexl::value::float_bits(*f))),
        ),
        Value::Character(c) => typed("Character", Some(Json::Str(vec![*c]))),
        Value::String(s) => typed("String", Some(Json::Str(s.units().to_vec()))),
        Value::Map(m) => {
            let entries: Vec<Json> = m
                .snapshot()
                .iter()
                .map(|(k, val)| Json::Arr(vec![encode(k), encode(val)]))
                .collect();
            let class = m.kind().class_name();
            with(typed("Map", Some(Json::Arr(entries))), "c", Json::str(class))
        }
        other => with(typed("Object", None), "c", Json::str(&other.class_name())),
    }
}
