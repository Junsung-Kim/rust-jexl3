// Typed value decoding shared with the Java oracle (Oracle.decode); see oracle/PROTOCOL.md.
#![allow(dead_code)]

use std::sync::Arc;

use rust_jexl3::java::big_decimal::BigDecimal;
use rust_jexl3::java::hash_map::{JHashMap, JHashSet};
use rust_jexl3::java::number;
use rust_jexl3::java::string::JString;
use rust_jexl3::value::{Component, JArray, JList, JMap, JSet, ListKind, MapKind, SetKind, Value};

use super::json::Json;


fn text(spec: &Json) -> String {
    spec.get("v").and_then(Json::string).unwrap_or_default()
}

fn units(spec: &Json) -> Vec<u16> {
    spec.get("v").and_then(Json::str16).map(<[u16]>::to_vec).unwrap_or_default()
}

/// Decodes a typed value exactly as the Java oracle does.
pub fn decode(spec: &Json) -> Value {
    let t = spec.get("t").and_then(Json::string).expect("type tag");
    let class = spec.get("c").and_then(Json::string);
    match t.as_str() {
        "null" => Value::Null,
        "Boolean" => Value::Boolean(text(spec) == "true"),
        "Byte" => Value::Byte(number::parse_byte(&text(spec), 10).expect("byte")),
        "Short" => Value::Short(number::parse_short(&text(spec), 10).expect("short")),
        "Integer" => Value::Integer(number::parse_int(&text(spec), 10).expect("int")),
        "Long" => Value::Long(number::parse_long(&text(spec), 10).expect("long")),
        "Float" => Value::Float(match spec.get("bits").and_then(Json::string) {
            Some(b) => f32::from_bits(u32::from_str_radix(&b, 16).expect("bits")),
            None => number::parse_float(&text(spec)).expect("float"),
        }),
        "Double" => Value::Double(match spec.get("bits").and_then(Json::string) {
            Some(b) => f64::from_bits(u64::from_str_radix(&b, 16).expect("bits")),
            None => number::parse_double(&text(spec)).expect("double"),
        }),
        "BigInteger" => Value::BigInteger(Arc::new(number::parse_big_integer(&text(spec), 10).expect("bigint"))),
        "BigDecimal" => Value::BigDecimal(Arc::new(BigDecimal::parse(&text(spec)).expect("bigdec"))),
        "Character" => Value::Character(units(spec)[0]),
        "String" => Value::String(JString::new(units(spec))),
        "List" => {
            let items: Vec<Value> = spec.get("v").and_then(Json::arr).unwrap_or(&[]).iter().map(decode).collect();
            let kind = match class.as_deref() {
                Some("java.util.LinkedList") => ListKind::LinkedList,
                _ => ListKind::ArrayList,
            };
            Value::List(JList::new(kind, items))
        }
        "Set" => {
            let kind = match class.as_deref() {
                Some("java.util.LinkedHashSet") => SetKind::LinkedHashSet,
                Some("java.util.TreeSet") => SetKind::TreeSet,
                _ => SetKind::HashSet,
            };
            let mut set = if kind == SetKind::LinkedHashSet { JHashSet::new_linked() } else { JHashSet::new() };
            for e in spec.get("v").and_then(Json::arr).unwrap_or(&[]) {
                set.add(decode(e));
            }
            Value::Set(JSet::new(kind, set))
        }
        "Map" => {
            let kind = match class.as_deref() {
                Some("java.util.LinkedHashMap") => MapKind::LinkedHashMap,
                Some("java.util.TreeMap") => MapKind::TreeMap,
                _ => MapKind::HashMap,
            };
            let mut map = if kind == MapKind::LinkedHashMap { JHashMap::new_linked() } else { JHashMap::new() };
            for e in spec.get("v").and_then(Json::arr).unwrap_or(&[]) {
                let kv = e.arr().expect("entry");
                map.put(decode(&kv[0]), decode(&kv[1]));
            }
            Value::Map(JMap::new(kind, map))
        }
        "Array" => {
            let items: Vec<Value> = spec.get("v").and_then(Json::arr).unwrap_or(&[]).iter().map(decode).collect();
            let component = match class.as_deref().unwrap_or("Object") {
                "int" => Component::Int,
                "long" => Component::Long,
                "short" => Component::Short,
                "byte" => Component::Byte,
                "char" => Component::Char,
                "float" => Component::Float,
                "double" => Component::Double,
                "boolean" => Component::Boolean,
                "String" => Component::Class("java.lang.String".into()),
                "Integer" => Component::Class("java.lang.Integer".into()),
                "Long" => Component::Class("java.lang.Long".into()),
                "Double" => Component::Class("java.lang.Double".into()),
                "Number" => Component::Class("java.lang.Number".into()),
                _ => Component::object(),
            };
            Value::Array(JArray::new(component, items))
        }
        "Host" => super::hosts::create(&text(spec)),
        other => panic!("unknown type {}", other),
    }
}
