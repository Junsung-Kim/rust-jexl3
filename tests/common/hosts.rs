// The named test host objects, mirroring oracle/src/rustjexl/oracle/Hosts.java.
#![allow(dead_code)]

use std::any::Any;

use rust_jexl::value::{HostObject, Value};

/// Stands for "JSON null": a non-null object, so `x == null` is false for it.
#[derive(Debug)]
pub struct JsonNull;

impl HostObject for JsonNull {
    fn class_name(&self) -> String {
        "rustjexl.oracle.Hosts$JsonNull".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some("null".into())
    }
    fn java_equals(&self, other: &Value) -> Option<bool> {
        Some(other.as_host::<JsonNull>().is_some())
    }
    fn java_hash_code(&self) -> Option<i32> {
        Some(7)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The namespace object of the primary consumer profile.
#[derive(Debug)]
pub struct Ns;

impl HostObject for Ns {
    fn class_name(&self) -> String {
        "rustjexl.oracle.Hosts$Ns".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some("Ns".into())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A bean with properties and methods: getName/setName, getValue/setValue, isFlag/setFlag and a
/// getItems() that hands back the same List every time, like the Java Hosts$Bean it stands for.
pub struct Bean {
    name: std::sync::Mutex<String>,
    value: std::sync::Mutex<i32>,
    flag: std::sync::Mutex<bool>,
    items: rust_jexl::value::JList,
}

impl Bean {
    pub fn new(name: &str, value: i32) -> Bean {
        Bean {
            name: std::sync::Mutex::new(name.to_string()),
            value: std::sync::Mutex::new(value),
            flag: std::sync::Mutex::new(false),
            items: rust_jexl::value::JList::array_list(Vec::new()),
        }
    }
    pub fn name(&self) -> String {
        self.name.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    pub fn value(&self) -> i32 {
        *self.value.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// The Java bean properties, by the name the introspector derives from the accessor.
    fn get(&self, property: &str) -> Option<Value> {
        match property {
            "name" => Some(Value::string(&self.name())),
            "value" => Some(Value::Integer(self.value())),
            "flag" => Some(Value::Boolean(*self.flag.lock().unwrap_or_else(|p| p.into_inner()))),
            "items" => Some(Value::List(self.items.clone())),
            _ => None,
        }
    }
    /// Returns None when there is no such setter; Err when Java's would not accept the argument.
    fn set(&self, property: &str, arg: &Value) -> Option<Result<(), JexlException>> {
        match (property, arg) {
            ("name", Value::String(s)) => {
                *self.name.lock().unwrap_or_else(|p| p.into_inner()) = s.to_rust();
                Some(Ok(()))
            }
            ("name", Value::Null) => {
                *self.name.lock().unwrap_or_else(|p| p.into_inner()) = String::new();
                Some(Ok(()))
            }
            ("value", Value::Integer(i)) => {
                *self.value.lock().unwrap_or_else(|p| p.into_inner()) = *i;
                Some(Ok(()))
            }
            ("flag", Value::Boolean(b)) => {
                *self.flag.lock().unwrap_or_else(|p| p.into_inner()) = *b;
                Some(Ok(()))
            }
            ("name" | "value" | "flag", _) => None,
            _ => None,
        }
    }
}

impl HostObject for Bean {
    fn class_name(&self) -> String {
        "rustjexl.oracle.Hosts$Bean".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some(format!("Bean({},{})", self.name(), self.value()))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub fn create(name: &str) -> Value {
    match name {
        "jsonNull" => Value::object(JsonNull),
        "ns" => Value::object(Ns),
        "bean" => Value::object(Bean::new("bean", 0)),
        other => panic!("unknown host {}", other),
    }
}

// ------------------------------------------------------------------ introspection

use std::sync::Arc;

use rust_jexl::introspection::jdk_shim::HostIntrospector;
use rust_jexl::introspection::{JexlMethod, JexlPropertyGet, JexlPropertySet};
use rust_jexl::jexl_exception::JexlException;

/// One method of a test host object.
struct HostMethod {
    ret: &'static str,
    call: fn(&Value, &[Value]) -> Result<Value, JexlException>,
}

impl JexlMethod for HostMethod {
    fn invoke(&self, obj: &Value, params: &[Value]) -> Result<Value, JexlException> {
        (self.call)(obj, params)
    }
    fn return_type(&self) -> Option<String> {
        Some(self.ret.to_string())
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

fn nfe(text: &str) -> JexlException {
    JexlException::java("java.lang.NumberFormatException", Some(format!("For input string: \"{}\"", text)))
}

/// Java widens to `int` from Byte, Short, Integer and Character, and from nothing else.
fn is_int_arg(v: &Value) -> bool {
    matches!(v, Value::Byte(_) | Value::Short(_) | Value::Integer(_) | Value::Character(_))
}

fn is_string_arg(v: &Value) -> bool {
    matches!(v, Value::String(_) | Value::Null)
}

fn is_null_like(v: &Value) -> bool {
    v.is_null() || v.as_host::<JsonNull>().is_some()
}

/// `String.valueOf(o)` for the `concat` head; null is the string "null" only for the varargs tail,
/// but Java's `new StringBuilder(String)` NPEs on a null head.
pub fn head_string(v: &Value) -> Result<String, JexlException> {
    match v {
        Value::Null => Err(JexlException::java(
            "java.lang.NullPointerException",
            Some("Cannot invoke \"String.length()\" because \"str\" is null".into()),
        )),
        other => Ok(other.java_to_string()),
    }
}

fn to_long(v: &Value) -> Result<i64, JexlException> {
    match v {
        Value::Byte(b) => Ok(*b as i64),
        Value::Short(s) => Ok(*s as i64),
        Value::Integer(i) => Ok(*i as i64),
        Value::Long(l) => Ok(*l),
        Value::Float(f) => Ok(*f as i64),
        Value::Double(d) => Ok(*d as i64),
        other => {
            let text = other.java_to_string();
            let t = text.trim();
            t.parse::<i64>().map_err(|_| nfe(t))
        }
    }
}

fn to_double(v: &Value) -> Result<f64, JexlException> {
    match v {
        Value::Byte(b) => Ok(*b as f64),
        Value::Short(s) => Ok(*s as f64),
        Value::Integer(i) => Ok(*i as f64),
        Value::Long(l) => Ok(*l as f64),
        Value::Float(f) => Ok(*f as f64),
        Value::Double(d) => Ok(*d),
        other => {
            let text = other.java_to_string();
            let t = text.trim();
            t.parse::<f64>().map_err(|_| nfe(t))
        }
    }
}

fn to_int(v: &Value) -> Result<i32, JexlException> {
    match v {
        Value::Byte(b) => Ok(*b as i32),
        Value::Short(s) => Ok(*s as i32),
        Value::Integer(i) => Ok(*i),
        Value::Long(l) => Ok(*l as i32),
        Value::Float(f) => Ok(*f as i32),
        Value::Double(d) => Ok(*d as i32),
        other => {
            let text = other.java_to_string();
            let t = text.trim();
            t.parse::<i32>().map_err(|_| nfe(t))
        }
    }
}

fn ip_version(s: &str) -> i32 {
    let v4 = s.split('.').count() == 4
        && s.split('.').all(|p| !p.is_empty() && p.len() <= 3 && p.chars().all(|c| c.is_ascii_digit()));
    if v4 {
        return 4;
    }
    if s.contains(':') && s.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.') && !s.is_empty() {
        return 6;
    }
    0
}

/// Mirrors the methods of `rustjexl.oracle.Hosts$Ns` and `$Bean`.
pub struct TestHosts;

impl HostIntrospector for TestHosts {
    fn get_method(&self, obj: &Value, name: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        if obj.as_host::<Ns>().is_some() {
            let m: HostMethod = match (name, args.len()) {
                ("isNull", 1) => HostMethod { ret: "boolean", call: |_, a| Ok(Value::Boolean(is_null_like(&a[0]))) },
                ("castToInt", 1) => HostMethod {
                    ret: "int", call: |_, a| to_int(&a[0]).map(Value::Integer) },
                ("joinWithPipe", _) => HostMethod {
                    ret: "java.lang.String",
                    call: |_, a| {
                        // Object... : a lone null argument *is* the array, so Java NPEs on length
                        if a.len() == 1 && a[0].is_null() {
                            return Err(JexlException::java(
                                "java.lang.NullPointerException",
                                Some("Cannot read the array length because \"args\" is null".into()),
                            ));
                        }
                        let parts: Vec<String> = a.iter().map(|v| v.java_to_string()).collect();
                        Ok(Value::string(&parts.join("|")))
                    },
                },
                // these three declare a String parameter: nothing else is applicable
                ("isIpv4", 1) if is_string_arg(&args[0]) => HostMethod {
                    ret: "boolean", call: |_, a| Ok(Value::Boolean(ip_version(&a[0].java_to_string()) == 4)) },
                ("isIpv6", 1) if is_string_arg(&args[0]) => HostMethod {
                    ret: "boolean", call: |_, a| Ok(Value::Boolean(ip_version(&a[0].java_to_string()) == 6)) },
                ("getIpVersion", 1) if is_string_arg(&args[0]) => HostMethod {
                    ret: "int", call: |_, a| Ok(Value::Integer(ip_version(&a[0].java_to_string()))) },
                ("size", _) => HostMethod {
                    ret: "int", call: |_, a| Ok(Value::Integer(a.len() as i32)) },
                ("nvl", 2) => HostMethod {
                    ret: "java.lang.Object",
                    call: |_, a| Ok(if is_null_like(&a[0]) { a[1].clone() } else { a[0].clone() }),
                },
                // long absAsInt64(Object): Number.longValue(), else Long.parseLong(String.valueOf(o).trim())
                ("absAsInt64", 1) => HostMethod {
                    ret: "long",
                    call: |_, a| Ok(Value::Long(to_long(&a[0])?.wrapping_abs())),
                },
                // double absAsDouble(Object): Number.doubleValue(), else Double.parseDouble(...)
                ("absAsDouble", 1) => HostMethod {
                    ret: "double",
                    call: |_, a| Ok(Value::Double(to_double(&a[0])?.abs())),
                },
                // String concat(String head, Object... rest)
                ("concat", n) if n >= 1 && is_string_arg(&args[0]) => HostMethod {
                    ret: "java.lang.String",
                    call: |_, a| {
                        let mut out = crate::common::hosts::head_string(&a[0])?;
                        for r in &a[1..] {
                            out.push_str(&r.java_to_string());
                        }
                        Ok(Value::string(&out))
                    },
                },
                // String kind(int|long|double|String|Object): the overload Java picks by argument type
                ("kind", 1) => HostMethod {
                    ret: "java.lang.String",
                    call: |_, a| {
                        Ok(Value::string(match &a[0] {
                            Value::Byte(_) | Value::Short(_) | Value::Integer(_) | Value::Character(_) => "int",
                            Value::Long(_) => "long",
                            Value::Float(_) | Value::Double(_) => "double",
                            Value::String(_) => "String",
                            _ => "Object",
                        }))
                    },
                },
                // int sum(int, int) / double sum(double, double): both parameters are primitive,
                // so Java's introspector applies widening and nothing else — a String argument
                // makes the method unsolvable rather than parsed.
                ("sum", 2) if is_int_arg(&args[0]) && is_int_arg(&args[1]) => HostMethod {
                    ret: "int",
                    call: |_, a| Ok(Value::Integer(to_int(&a[0])?.wrapping_add(to_int(&a[1])?))),
                },
                ("sum", 2) if args[0].is_number() && args[1].is_number() => HostMethod {
                    ret: "double",
                    call: |_, a| Ok(Value::Double(to_double(&a[0])? + to_double(&a[1])?)),
                },
                _ => return None,
            };
            return Some(Arc::new(m));
        }
        if obj.as_host::<Bean>().is_some() {
            let m: HostMethod = match (name, args.len()) {
                ("greet", 1) => HostMethod {
                    ret: "java.lang.String",
                    call: |o, a| {
                        let b = o.as_host::<Bean>().expect("bean");
                        Ok(Value::string(&format!("hello {} from {}", a[0].java_to_string(), b.name())))
                    },
                },
                ("twice", 1) => HostMethod {
                    ret: "int", call: |_, a| to_int(&a[0]).map(|i| Value::Integer(i.wrapping_mul(2))) },
                ("getName", 0) => HostMethod {
                    ret: "java.lang.String",
                    call: |o, _| Ok(Value::string(&o.as_host::<Bean>().expect("bean").name())),
                },
                ("getValue", 0) => HostMethod {
                    ret: "int",
                    call: |o, _| Ok(Value::Integer(o.as_host::<Bean>().expect("bean").value())),
                },
                _ => return None,
            };
            return Some(Arc::new(m));
        }
        None
    }

    fn get_property_get(&self, obj: &Value, identifier: &Value) -> Option<Arc<dyn JexlPropertyGet>> {
        let bean = obj.as_host::<Bean>()?;
        let property = identifier.java_to_string();
        bean.get(&property)?;
        Some(Arc::new(BeanGet { property }))
    }

    fn get_property_set(&self, obj: &Value, identifier: &Value, arg: &Value) -> Option<Arc<dyn JexlPropertySet>> {
        let bean = obj.as_host::<Bean>()?;
        let property = identifier.java_to_string();
        bean.set(&property, arg)?;
        Some(Arc::new(BeanSet { property }))
    }

    fn get_constructor(&self, handle: &Value, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        let _ = (handle, args);
        None
    }
}

/// port of: the PropertyGet a Java introspector builds for a bean accessor
struct BeanGet {
    property: String,
}

impl JexlPropertyGet for BeanGet {
    fn invoke(&self, obj: &Value) -> Result<Value, JexlException> {
        match obj.as_host::<Bean>().and_then(|b| b.get(&self.property)) {
            Some(v) => Ok(v),
            None => Err(JexlException::java("java.lang.IllegalArgumentException", Some(self.property.clone()))),
        }
    }
}

struct BeanSet {
    property: String,
}

impl JexlPropertySet for BeanSet {
    fn invoke(&self, obj: &Value, arg: &Value) -> Result<Value, JexlException> {
        match obj.as_host::<Bean>().and_then(|b| b.set(&self.property, arg)) {
            Some(Ok(())) => Ok(arg.clone()),
            Some(Err(e)) => Err(e),
            None => Err(JexlException::java("java.lang.IllegalArgumentException", Some(self.property.clone()))),
        }
    }
}
