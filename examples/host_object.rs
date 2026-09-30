//! Plugging a Rust type into JEXL scripts.
//!
//! Java JEXL finds properties and methods by reflection. Rust has none, so a host type says what it
//! offers through a `HostIntrospector`: here an `Order` with two properties (`id`, `total`), a
//! setter, and a method `discounted(percent)`.
//!
//!   cargo run --example host_object
use std::any::Any;
use std::sync::{Arc, Mutex};

use rust_jexl3::introspection::jdk_shim::{HostIntrospector, JdkShim};
use rust_jexl3::introspection::uberspect::Uberspect;
use rust_jexl3::introspection::{JexlMethod, JexlPropertyGet, JexlPropertySet, ResolverStrategy};
use rust_jexl3::jexl_context::{JexlContext, MapContext};
use rust_jexl3::jexl_engine::JexlBuilder;
use rust_jexl3::jexl_exception::JexlException;
use rust_jexl3::value::{HostObject, Value};

/// The domain type. Interior mutability, because a script may assign `order.total = ...`.
struct Order {
    id: String,
    total: Mutex<i64>,
}

impl HostObject for Order {
    /// What `getClass().getName()` would say; it also appears in error messages.
    fn class_name(&self) -> String {
        "example.Order".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some(format!("Order({}, {})", self.id, self.total.lock().unwrap()))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// `order.id`, `order.total`
struct Getter(&'static str);

impl JexlPropertyGet for Getter {
    fn invoke(&self, obj: &Value) -> Result<Value, JexlException> {
        let order = obj.as_host::<Order>().expect("an Order");
        Ok(match self.0 {
            "id" => Value::string(&order.id),
            _ => Value::Long(*order.total.lock().unwrap()),
        })
    }
}

/// `order.total = 120`
struct TotalSetter;

impl JexlPropertySet for TotalSetter {
    fn invoke(&self, obj: &Value, arg: &Value) -> Result<Value, JexlException> {
        let order = obj.as_host::<Order>().expect("an Order");
        let v = match arg {
            Value::Integer(i) => *i as i64,
            Value::Long(l) => *l,
            other => return Err(JexlException::java("java.lang.IllegalArgumentException", Some(other.java_to_string()))),
        };
        *order.total.lock().unwrap() = v;
        Ok(arg.clone())
    }
}

/// `order.discounted(10)` -> the total less 10%
struct Discounted;

impl JexlMethod for Discounted {
    fn invoke(&self, obj: &Value, params: &[Value]) -> Result<Value, JexlException> {
        let order = obj.as_host::<Order>().expect("an Order");
        let percent = match params.first() {
            Some(Value::Integer(p)) => *p as i64,
            _ => 0,
        };
        let total = *order.total.lock().unwrap();
        Ok(Value::Long(total - total * percent / 100))
    }
    fn return_type(&self) -> Option<String> {
        Some("long".into())
    }
}

/// What JEXL may do with an `Order`. Returning None means "no such property/method", which the
/// engine reports the way Java JEXL would.
struct Hosts;

impl HostIntrospector for Hosts {
    fn get_method(&self, obj: &Value, name: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        obj.as_host::<Order>()?;
        match (name, args) {
            ("discounted", [Value::Integer(_)]) => Some(Arc::new(Discounted)),
            _ => None,
        }
    }
    fn get_property_get(&self, obj: &Value, identifier: &Value) -> Option<Arc<dyn JexlPropertyGet>> {
        obj.as_host::<Order>()?;
        match identifier.java_to_string().as_str() {
            "id" => Some(Arc::new(Getter("id"))),
            "total" => Some(Arc::new(Getter("total"))),
            _ => None,
        }
    }
    fn get_property_set(&self, obj: &Value, identifier: &Value, _arg: &Value) -> Option<Arc<dyn JexlPropertySet>> {
        obj.as_host::<Order>()?;
        (identifier.java_to_string() == "total").then(|| Arc::new(TotalSetter) as Arc<dyn JexlPropertySet>)
    }
    fn get_constructor(&self, _handle: &Value, _args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        None
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // the JDK model plus our types, behind the same Uberspect SPI Java JEXL uses
    let shim = JdkShim::new(ResolverStrategy::Jexl).with_hosts(Arc::new(Hosts));
    let uberspect = Uberspect::new().with_shim(Arc::new(shim));
    let jexl = JexlBuilder::new().uberspect(Arc::new(uberspect)).strict(true).create();

    let context = Arc::new(MapContext::new());
    context.set("order", Value::object(Order { id: "A-17".into(), total: Mutex::new(200) }))?;

    for src in [
        "order.id",
        "order.total > 100",
        "order.discounted(10)",
        "order.total = order.total + 50; order",
        "order.nope",
    ] {
        let script = jexl.create_script(src)?;
        match script.execute(context.clone()) {
            Ok(v) => println!("{:<42} => {}", src, v.java_to_string()),
            Err(e) => println!("{:<42} !! {}", src, e.message()),
        }
    }
    Ok(())
}
