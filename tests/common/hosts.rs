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

/// A bean with properties and methods.
#[derive(Debug)]
pub struct Bean {
    pub name: String,
    pub value: i32,
}

impl HostObject for Bean {
    fn class_name(&self) -> String {
        "rustjexl.oracle.Hosts$Bean".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some(format!("Bean({},{})", self.name, self.value))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub fn create(name: &str) -> Value {
    match name {
        "jsonNull" => Value::object(JsonNull),
        "ns" => Value::object(Ns),
        "bean" => Value::object(Bean { name: "bean".into(), value: 0 }),
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

fn is_string_arg(v: &Value) -> bool {
    matches!(v, Value::String(_) | Value::Null)
}

fn is_null_like(v: &Value) -> bool {
    v.is_null() || v.as_host::<JsonNull>().is_some()
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
                        Ok(Value::string(&format!("hello {} from {}", a[0].java_to_string(), b.name)))
                    },
                },
                ("twice", 1) => HostMethod {
                    ret: "int", call: |_, a| to_int(&a[0]).map(|i| Value::Integer(i.wrapping_mul(2))) },
                ("getName", 0) => HostMethod {
                    ret: "java.lang.String",
                    call: |o, _| Ok(Value::string(&o.as_host::<Bean>().expect("bean").name)),
                },
                ("getValue", 0) => HostMethod {
                    ret: "int",
                    call: |o, _| Ok(Value::Integer(o.as_host::<Bean>().expect("bean").value)),
                },
                _ => return None,
            };
            return Some(Arc::new(m));
        }
        None
    }

    fn get_property_get(&self, obj: &Value, identifier: &Value) -> Option<Arc<dyn JexlPropertyGet>> {
        let _ = (obj, identifier);
        None
    }

    fn get_property_set(&self, obj: &Value, identifier: &Value, arg: &Value) -> Option<Arc<dyn JexlPropertySet>> {
        let _ = (obj, identifier, arg);
        None
    }

    fn get_constructor(&self, handle: &Value, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        let _ = (handle, args);
        None
    }
}
