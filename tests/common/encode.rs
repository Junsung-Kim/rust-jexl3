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
            Json::str(&format!("{:x}", rust_jexl::value::double_raw_bits(*d))),
        ),
        Value::Float(f) => with(
            typed("Float", Some(Json::str(&v.java_to_string()))),
            "bits",
            Json::str(&format!("{:x}", rust_jexl::value::float_raw_bits(*f))),
        ),
        Value::Character(c) => typed("Character", Some(Json::Str(vec![*c]))),
        Value::String(s) => typed("String", Some(Json::Str(s.units().to_vec()))),
        Value::Array(a) => {
            let items: Vec<Json> = a.snapshot().iter().map(encode).collect();
            with(typed("Array", Some(Json::Arr(items))), "c", Json::str(&a.component.simple_name()))
        }
        Value::List(l) => {
            let items: Vec<Json> = l.snapshot().iter().map(encode).collect();
            with(typed("List", Some(Json::Arr(items))), "c", Json::str(l.kind().class_name()))
        }
        Value::Set(set) => {
            let items: Vec<Json> = set.snapshot().iter().map(encode).collect();
            with(typed("Set", Some(Json::Arr(items))), "c", Json::str(set.kind().class_name()))
        }
        Value::AtomicBoolean(_) => with(typed("Object", Some(Json::str(&v.java_to_string()))), "c", Json::str(&v.class_name())),
        // a Map's keySet()/entrySet() really are java.util.Set, so Oracle.encode takes that branch
        Value::Object(o) if matches!(o.as_collection(), Some((true, _))) => {
            let (_, elements) = o.as_collection().expect("set view");
            let items: Vec<Json> = elements.iter().map(encode).collect();
            with(typed("Set", Some(Json::Arr(items))), "c", Json::str(&v.class_name()))
        }
        Value::Map(m) => {
            let entries: Vec<Json> = m
                .snapshot()
                .iter()
                .map(|(k, val)| Json::Arr(vec![encode(k), encode(val)]))
                .collect();
            let class = m.kind().class_name();
            with(typed("Map", Some(Json::Arr(entries))), "c", Json::str(class))
        }
        Value::Object(o) if o.as_any().downcast_ref::<rust_jexl::internal::range::Range>().is_some() => {
            let r = o.as_any().downcast_ref::<rust_jexl::internal::range::Range>().expect("range");
            let items = Json::Arr(vec![encode(&r.get_min()), encode(&r.get_max())]);
            with(typed("Range", Some(items)), "c", Json::str(&r.class_name()))
        }
        Value::Object(o) if o.as_any().downcast_ref::<rust_jexl::internal::script::Closure>().is_some() => {
            let c = o.as_any().downcast_ref::<rust_jexl::internal::script::Closure>().expect("closure");
            let text = rust_jexl::internal::debugger::Debugger::new().data_indent(c.ast.node(c.script), 2);
            with(typed("Script", Some(Json::Str(text.units().to_vec()))), "c", Json::str("internal.Closure"))
        }
        Value::Object(o) if o.class_name() == "java.lang.Class" => {
            // Oracle.encode: a Class encodes as its binary name
            let text = o.java_to_string().unwrap_or_default();
            typed("Class", Some(Json::str(text.strip_prefix("class ").or_else(|| text.strip_prefix("interface ")).unwrap_or(&text))))
        }
        Value::Object(o) => {
            let class = o.class_name();
            let short = class.rsplit(['.', '$']).next().unwrap_or(&class).to_string();
            if matches!(short.as_str(), "JsonNull" | "Ns" | "Bean") {
                return with(typed("Host", o.java_to_string().map(|s| Json::str(&s))), "c", Json::str(&short));
            }
            // Oracle.encode falls through to the generic Object shape for everything else
            let name = class.strip_prefix("org.apache.commons.jexl3.").unwrap_or(&class).to_string();
            let mut m = typed("Object", None);
            m = with(m, "c", Json::str(&name));
            match o.java_to_string() {
                Some(v) => with(m, "v", Json::str(&v)),
                None => with(m, "nd", Json::Bool(true)),
            }
        }
        other => with(typed("Object", None), "c", Json::str(&other.class_name())),
    }
}
