//! Ports of the upstream statement, assignment, method and namespace test classes of
//! Apache Commons JEXL 3.2.1 (`src/test/java/org/apache/commons/jexl3/`).
//!
//! Every `#[test]` is one Java test method, named in snake_case, with the Java class and method
//! in a comment above it. Expected values were produced by running the same script, engine
//! configuration and context through the real `commons-jexl3-3.2.1.jar` (`oracle/target/oracle`);
//! where the oracle could not express a fixture (a Java bean, a context that resolves a
//! namespace) the script shape was kept and the fixture modelled as a host object, which is the
//! `JexlUberspect` path an embedder uses.
//!
//! `JexlTestCase` installs `JexlOptions.setDefaultFlags("-safe", "+lexical")` for the whole
//! upstream suite; `builder()` below is that default.
//! The upstream test methods of these classes that are not here are listed, with their
//! reason, in COMPATIBILITY.md.
#![allow(clippy::cloned_ref_to_slice_refs)] // ported Java test code
#![allow(clippy::bool_assert_comparison)]

use std::any::Any;
use std::sync::Arc;

use rust_jexl3::introspection::jdk_shim::{HostIntrospector, JdkShim};
use rust_jexl3::introspection::uberspect::Uberspect;
use rust_jexl3::introspection::{JexlMethod, JexlPropertyGet, JexlPropertySet, ResolverStrategy};
use rust_jexl3::jexl_context::{JexlContext, MapContext};
use rust_jexl3::jexl_engine::{empty_context, JexlBuilder, JexlEngine, JexlScript};
use rust_jexl3::jexl_exception::JexlException;
use rust_jexl3::value::{Component, HostObject, JArray, JList, JMap, Value};

// ------------------------------------------------------------------ harness

/// port of: JexlTestCase's static initializer, `JexlOptions.setDefaultFlags("-safe", "+lexical")`.
fn builder() -> JexlBuilder {
    JexlBuilder::new().safe(false).lexical(true)
}

/// port of: `new JexlBuilder().cache(128).create()`, the JEXL field of every JexlTestCase.
fn jexl() -> Arc<JexlEngine> {
    hosts(builder().cache(128)).create()
}

/// Registers the test beans (`Foo`, `Froboz`, ...) the way an embedder registers its own types.
fn hosts(b: JexlBuilder) -> JexlBuilder {
    let shim = JdkShim::new(ResolverStrategy::Jexl).with_hosts(Arc::new(Beans));
    b.uberspect(Arc::new(Uberspect::new().with_shim(Arc::new(shim))))
}

fn ctx() -> Arc<MapContext> {
    Arc::new(MapContext::new())
}

fn ok(r: Result<Value, JexlException>) -> Value {
    r.unwrap_or_else(|e| panic!("unexpected exception: {}", e.message()))
}

fn thrown(r: Result<Value, JexlException>) -> JexlException {
    match r {
        Ok(v) => panic!("should have thrown, got {:?}", v),
        Err(e) => e,
    }
}

fn parse_err(r: Result<JexlScript, JexlException>) -> JexlException {
    match r {
        Ok(_) => panic!("should not have been parsed"),
        Err(e) => e,
    }
}

#[track_caller]
fn eq(got: &Value, want: &Value) {
    assert!(got.java_equals(want), "expected {:?}, got {:?}", want, got);
}

fn i(n: i32) -> Value {
    Value::Integer(n)
}
fn l(n: i64) -> Value {
    Value::Long(n)
}
fn s(t: &str) -> Value {
    Value::string(t)
}
fn array(items: Vec<Value>) -> Value {
    Value::Array(JArray::new(Component::object(), items))
}
fn list(items: Vec<Value>) -> Value {
    Value::List(JList::array_list(items))
}
fn map(entries: Vec<(Value, Value)>) -> Value {
    let m = JMap::hash_map();
    for (k, v) in entries {
        m.put(k, v);
    }
    Value::Map(m)
}

// ------------------------------------------------------------------ the upstream test beans
//
// Java discovers these by reflection; this port registers them through `HostIntrospector`, the
// SPI an embedder uses for its own types. Only the members the ported tests touch are modelled.

/// port of: org.apache.commons.jexl3.Foo
#[derive(Debug)]
struct Foo;

impl HostObject for Foo {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.Foo".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: org.apache.commons.jexl3.AssignTest$Froboz
#[derive(Debug)]
struct Froboz(std::sync::Mutex<i32>);

impl HostObject for Froboz {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.AssignTest$Froboz".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: org.apache.commons.jexl3.SideEffectTest$Foo (`setValue(long)`, `getValue()`)
#[derive(Debug)]
struct SeFoo(std::sync::Mutex<i32>);

impl HostObject for SeFoo {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.SideEffectTest$Foo".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some(self.0.lock().unwrap().to_string())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: org.apache.commons.jexl3.VarTest$NumbersContext, as the object its null namespace
/// resolves to (Java returns the context itself, which carries `numbers()`).
#[derive(Debug)]
struct Numbers;

impl HostObject for Numbers {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.VarTest$NumbersContext".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: the namespace object of ContextNamespaceTest's JEXL-348 tests (`Ns348.func(int)`),
/// standing in as the oracle's registered `Bean` host with its `twice(int)`.
#[derive(Debug)]
struct Bean;

impl HostObject for Bean {
    fn class_name(&self) -> String {
        "rustjexl.oracle.Hosts$Bean".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some("Bean(bean,0)".into())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

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

struct Getter(fn(&Value) -> Result<Value, JexlException>);

impl JexlPropertyGet for Getter {
    fn invoke(&self, obj: &Value) -> Result<Value, JexlException> {
        (self.0)(obj)
    }
}

struct Setter(fn(&Value, &Value) -> Result<Value, JexlException>);

impl JexlPropertySet for Setter {
    fn invoke(&self, obj: &Value, arg: &Value) -> Result<Value, JexlException> {
        (self.0)(obj, arg)
    }
}

/// Java's `(int) v` on a Number argument.
fn to_int(v: &Value) -> Option<i32> {
    match v {
        Value::Byte(b) => Some(*b as i32),
        Value::Short(x) => Some(*x as i32),
        Value::Integer(x) => Some(*x),
        Value::Long(x) => Some(*x as i32),
        Value::Float(x) => Some(*x as i32),
        Value::Double(x) => Some(*x as i32),
        _ => None,
    }
}

fn cheese_list() -> Value {
    list(vec![s("cheddar"), s("edam"), s("brie")])
}

struct Beans;

impl HostIntrospector for Beans {
    fn get_method(&self, obj: &Value, name: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        if obj.as_host::<Foo>().is_some() {
            let m = match (name, args.len()) {
                ("bar", 0) => HostMethod { ret: "java.lang.String", call: |_, _| Ok(s("Method string")) },
                ("getBar", 0) => HostMethod { ret: "java.lang.String", call: |_, _| Ok(s("GetMethod string")) },
                ("getInnerFoo", 0) => HostMethod { ret: "org.apache.commons.jexl3.Foo", call: |_, _| Ok(Value::object(Foo)) },
                ("getCheeseList", 0) => HostMethod { ret: "java.util.List", call: |_, _| Ok(cheese_list()) },
                _ => return None,
            };
            return Some(Arc::new(m));
        }
        if obj.as_host::<Bean>().is_some() && name == "twice" && args.len() == 1 {
            return Some(Arc::new(HostMethod {
                ret: "int",
                call: |_, a| Ok(i(to_int(&a[0]).unwrap_or(0).wrapping_mul(2))),
            }));
        }
        if obj.as_host::<Numbers>().is_some() && name == "numbers" && args.is_empty() {
            return Some(Arc::new(HostMethod {
                ret: "java.lang.Object",
                call: |_, _| Ok(Value::Array(JArray::new(Component::Int, vec![i(5), i(17), i(20)]))),
            }));
        }
        None
    }

    fn get_property_get(&self, obj: &Value, identifier: &Value) -> Option<Arc<dyn JexlPropertyGet>> {
        let name = identifier.java_to_string();
        if obj.as_host::<Foo>().is_some() {
            match name.as_str() {
                "cheeseList" => return Some(Arc::new(Getter(|_| Ok(cheese_list())))),
                "innerFoo" => return Some(Arc::new(Getter(|_| Ok(Value::object(Foo))))),
                "bar" => return Some(Arc::new(Getter(|_| Ok(s("GetMethod string"))))),
                _ => {}
            }
        }
        if obj.as_host::<Froboz>().is_some() && name == "value" {
            return Some(Arc::new(Getter(|o| Ok(i(*o.as_host::<Froboz>().expect("froboz").0.lock().unwrap())))));
        }
        if obj.as_host::<SeFoo>().is_some() && name == "value" {
            return Some(Arc::new(Getter(|o| Ok(i(*o.as_host::<SeFoo>().expect("foo").0.lock().unwrap())))));
        }
        None
    }

    fn get_property_set(&self, obj: &Value, identifier: &Value, arg: &Value) -> Option<Arc<dyn JexlPropertySet>> {
        let name = identifier.java_to_string();
        if obj.as_host::<Froboz>().is_some() && name == "value" && to_int(arg).is_some() {
            return Some(Arc::new(Setter(|o, a| {
                *o.as_host::<Froboz>().expect("froboz").0.lock().unwrap() = to_int(a).expect("int");
                Ok(a.clone())
            })));
        }
        if obj.as_host::<SeFoo>().is_some() && name == "value" && to_int(arg).is_some() {
            return Some(Arc::new(Setter(|o, a| {
                *o.as_host::<SeFoo>().expect("foo").0.lock().unwrap() = to_int(a).expect("int");
                Ok(a.clone())
            })));
        }
        None
    }

    fn get_constructor(&self, _handle: &Value, _args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        None
    }
}

/// port of: org.apache.commons.jexl3.JexlEvalContext — variables plus mutable engine options.
struct EvalContext {
    vars: MapContext,
    options: std::sync::RwLock<rust_jexl3::jexl_options::JexlOptions>,
    namespaces: std::collections::HashMap<String, Value>,
}

impl EvalContext {
    fn build() -> EvalContext {
        let mut o = rust_jexl3::jexl_options::JexlOptions::new();
        // JexlTestCase: JexlOptions.setDefaultFlags("-safe", "+lexical")
        o.set_safe(false);
        o.set_lexical(true);
        EvalContext {
            vars: MapContext::new(),
            options: std::sync::RwLock::new(o),
            namespaces: std::collections::HashMap::new(),
        }
    }

    fn new() -> Arc<EvalContext> {
        Arc::new(EvalContext::build())
    }

    /// port of: ContextNamespaceTest.ContextNs348 — a context that resolves one namespace.
    fn with_namespace(name: &str, ns: Value) -> Arc<EvalContext> {
        let mut c = EvalContext::build();
        c.namespaces.insert(name.to_string(), ns);
        Arc::new(c)
    }

    fn options(&self, f: impl FnOnce(&mut rust_jexl3::jexl_options::JexlOptions)) {
        f(&mut self.options.write().unwrap())
    }
}

impl JexlContext for EvalContext {
    fn get(&self, name: &str) -> Option<Value> {
        self.vars.get(name)
    }
    fn set(&self, name: &str, value: Value) -> Result<(), String> {
        self.vars.set(name, value)
    }
    fn has(&self, name: &str) -> bool {
        self.vars.has(name)
    }
    fn get_engine_options(&self) -> Option<rust_jexl3::jexl_options::JexlOptions> {
        Some(self.options.read().unwrap().clone())
    }
    fn resolve_namespace(&self, name: Option<&str>) -> Option<Value> {
        name.and_then(|n| self.namespaces.get(n)).cloned()
    }
    fn is_namespace_resolver(&self) -> bool {
        !self.namespaces.is_empty()
    }
}

// ================================================================== IfTest

// port of: IfTest.testSimpleIfTrue
#[test]
fn test_simple_if_true() {
    let e = jexl().create_script("if (true) 1").expect("parse");
    eq(&ok(e.execute(ctx())), &i(1));
}

// port of: IfTest.testSimpleIfFalse
#[test]
fn test_simple_if_false() {
    let e = jexl().create_script("if (false) 1").expect("parse");
    eq(&ok(e.execute(ctx())), &Value::Null);
}

// port of: IfTest.testSimpleElse
#[test]
fn test_simple_else() {
    let e = jexl().create_script("if (false) 1 else 2;").expect("parse");
    eq(&ok(e.execute(ctx())), &i(2));
}

// port of: IfTest.testBlockIfTrue
#[test]
fn test_block_if_true() {
    let e = jexl().create_script("if (true) { 'hello'; }").expect("parse");
    eq(&ok(e.execute(ctx())), &s("hello"));
}

// port of: IfTest.testBlockElse
#[test]
fn test_block_else() {
    let e = jexl().create_script("if (false) {1} else {2 ; 3}").expect("parse");
    eq(&ok(e.execute(ctx())), &i(3));
}

// port of: IfTest.testIfWithSimpleExpression
#[test]
fn test_if_with_simple_expression() {
    let e = jexl().create_script("if (x == 1) true;").expect("parse");
    let jc = ctx();
    jc.set("x", i(1)).expect("set");
    eq(&ok(e.execute(jc)), &Value::Boolean(true));
}

// port of: IfTest.testIfElseIfExpression
#[test]
fn test_if_else_if_expression() {
    let e = jexl()
        .create_script_named("if (x == 1) { 10; } else if (x == 2) 20  else 30", &["x".into()])
        .expect("parse");
    eq(&ok(e.execute_args(empty_context(), &[i(1)])), &i(10));
    eq(&ok(e.execute_args(empty_context(), &[i(2)])), &i(20));
    eq(&ok(e.execute_args(empty_context(), &[i(4)])), &i(30));
}

// port of: IfTest.testIfElseIfReturnExpression0
#[test]
fn test_if_else_if_return_expression0() {
    let e = jexl()
        .create_script_named(
            "if (x == 1) return 10; if (x == 2)  return 20; else if (x == 3) return 30  else { return 40 }",
            &["x".into()],
        )
        .expect("parse");
    eq(&ok(e.execute_args(empty_context(), &[i(1)])), &i(10));
    eq(&ok(e.execute_args(empty_context(), &[i(2)])), &i(20));
    eq(&ok(e.execute_args(empty_context(), &[i(3)])), &i(30));
    eq(&ok(e.execute_args(empty_context(), &[i(4)])), &i(40));
}

// port of: IfTest.testIfElseIfReturnExpression
#[test]
fn test_if_else_if_return_expression() {
    let e = jexl()
        .create_script_named(
            "if (x == 1) return 10;  if (x == 2) return 20  else if (x == 3) return 30; else return 40;",
            &["x".into()],
        )
        .expect("parse");
    eq(&ok(e.execute_args(empty_context(), &[i(1)])), &i(10));
    eq(&ok(e.execute_args(empty_context(), &[i(2)])), &i(20));
    eq(&ok(e.execute_args(empty_context(), &[i(3)])), &i(30));
    eq(&ok(e.execute_args(empty_context(), &[i(4)])), &i(40));
}

// port of: IfTest.testIfWithArithmeticExpression
#[test]
fn test_if_with_arithmetic_expression() {
    let e = jexl().create_script("if ((x * 2) + 1 == 5) true;").expect("parse");
    let jc = ctx();
    jc.set("x", i(2)).expect("set");
    eq(&ok(e.execute(jc)), &Value::Boolean(true));
}

// port of: IfTest.testIfWithDecimalArithmeticExpression
#[test]
fn test_if_with_decimal_arithmetic_expression() {
    let e = jexl().create_script("if ((x * 2) == 5) true").expect("parse");
    let jc = ctx();
    jc.set("x", Value::Float(2.5)).expect("set");
    eq(&ok(e.execute(jc)), &Value::Boolean(true));
}

// port of: IfTest.testIfWithAssignment
#[test]
fn test_if_with_assignment() {
    let e = jexl().create_script("if ((x * 2) == 5) {y = 1} else {y = 2;}").expect("parse");
    let jc = ctx();
    jc.set("x", Value::Float(2.5)).expect("set");
    ok(e.execute(jc.clone()));
    eq(&jc.get("y").expect("y"), &i(1));
}

// port of: IfTest.testTernary
#[test]
fn test_ternary() {
    let jexl = jexl();
    let jc = EvalContext::new();
    let e = jexl.create_expression(None, "x.y.z = foo ?'bar':'quux'").expect("parse");

    for l in 0..4 {
        jc.options(|o| {
            o.set_strict(l & 1 == 0);
            o.set_silent(l & 2 != 0);
        });
        eq(&ok(e.evaluate(jc.clone())), &s("quux"));
        eq(&jc.get("x.y.z").expect("x.y.z"), &s("quux"));
    }

    jc.set("foo", Value::Null).expect("set");
    for l in 0..4 {
        jc.options(|o| {
            o.set_strict(l & 1 == 0);
            o.set_silent(l & 2 != 0);
        });
        eq(&ok(e.evaluate(jc.clone())), &s("quux"));
        eq(&jc.get("x.y.z").expect("x.y.z"), &s("quux"));
    }

    jc.set("foo", Value::Boolean(false)).expect("set");
    for l in 0..4 {
        jc.options(|o| {
            o.set_strict(l & 1 == 0);
            o.set_silent(l & 2 != 0);
        });
        eq(&ok(e.evaluate(jc.clone())), &s("quux"));
        eq(&jc.get("x.y.z").expect("x.y.z"), &s("quux"));
    }

    jc.set("foo", Value::Boolean(true)).expect("set");
    for l in 0..4 {
        jc.options(|o| {
            o.set_strict(l & 1 == 0);
            o.set_silent(l & 2 != 0);
        });
        eq(&ok(e.evaluate(jc.clone())), &s("bar"));
        eq(&jc.get("x.y.z").expect("x.y.z"), &s("bar"));
    }
}

// port of: IfTest.testTernaryShorthand
#[test]
fn test_ternary_shorthand() {
    let jexl = jexl();
    let jc = EvalContext::new();
    let e = jexl.create_expression(None, "x.y.z = foo?:'quux'").expect("parse");
    let f = jexl.create_expression(None, "foo??'quux'").expect("parse");

    // foo, then the value `f` must answer for it
    let steps: Vec<(Option<Value>, Value, Value)> = vec![
        (None, s("quux"), s("quux")),
        (Some(Value::Null), s("quux"), s("quux")),
        (Some(Value::Boolean(false)), s("quux"), Value::Boolean(false)),
        (Some(Value::Double(f64::NAN)), s("quux"), Value::Double(f64::NAN)),
        (Some(s("")), s("quux"), s("")),
        (Some(s("false")), s("quux"), s("false")),
        (Some(Value::Double(0.0)), s("quux"), Value::Double(0.0)),
        (Some(i(0)), s("quux"), i(0)),
    ];
    for (foo, want_e, want_f) in steps {
        if let Some(v) = foo {
            jc.set("foo", v).expect("set");
        }
        for l in 0..4 {
            jc.options(|o| {
                o.set_strict(l & 1 == 0);
                o.set_silent(l & 2 != 0);
            });
            eq(&ok(e.evaluate(jc.clone())), &want_e);
            eq(&jc.get("x.y.z").expect("x.y.z"), &want_e);
            eq(&ok(f.evaluate(jc.clone())), &want_f);
        }
    }

    jc.set("foo", s("bar")).expect("set");
    for l in 0..4 {
        jc.options(|o| {
            o.set_strict(l & 1 == 0);
            o.set_silent(l & 2 != 0);
        });
        eq(&ok(e.evaluate(jc.clone())), &s("bar"));
        eq(&jc.get("x.y.z").expect("x.y.z"), &s("bar"));
    }
}

// port of: IfTest.testNullCoaelescing
#[test]
fn test_null_coaelescing() {
    let jexl = jexl();
    let jc = EvalContext::new();
    let xtrue = jexl.create_expression(None, "x??true").expect("parse");
    eq(&ok(xtrue.evaluate(jc.clone())), &Value::Boolean(true));
    jc.set("x", Value::Boolean(false)).expect("set");
    eq(&ok(xtrue.evaluate(jc.clone())), &Value::Boolean(false));
    let yone = jexl.create_expression(None, "y??1").expect("parse");
    eq(&ok(yone.evaluate(jc.clone())), &i(1));
    jc.set("y", i(0)).expect("set");
    eq(&ok(yone.evaluate(jc.clone())), &i(0));
}

// port of: IfTest.testNullCoaelescingScript
#[test]
fn test_null_coaelescing_script() {
    let jexl = jexl();
    let jc = EvalContext::new();
    let xtrue = jexl.create_script("x??true").expect("parse");
    eq(&ok(xtrue.execute(jc.clone())), &Value::Boolean(true));
    jc.set("x", Value::Boolean(false)).expect("set");
    eq(&ok(xtrue.execute(jc.clone())), &Value::Boolean(false));
    let yone = jexl.create_script("y??1").expect("parse");
    eq(&ok(yone.execute(jc.clone())), &i(1));
    jc.set("y", i(0)).expect("set");
    eq(&ok(yone.execute(jc.clone())), &i(0));
}

// port of: IfTest.testTernaryFail
#[test]
fn test_ternary_fail() {
    let jc = EvalContext::new();
    let e = jexl().create_expression(None, "false ? bar : quux").expect("parse");
    jc.options(|o| {
        o.set_strict(true);
        o.set_silent(false);
    });
    let x = thrown(e.evaluate(jc));
    assert!(x.message().contains("quux"), "{}", x.message());
}

// ================================================================== ForEachTest

// port of: ForEachTest.testForEachWithEmptyStatement
#[test]
fn test_for_each_with_empty_statement() {
    let e = jexl().create_script("for(item : list) ;").expect("parse");
    let jc = ctx();
    jc.set("list", list(vec![])).expect("set");
    eq(&ok(e.execute(jc)), &Value::Null);
}

// port of: ForEachTest.testForEachWithEmptyList
#[test]
fn test_for_each_with_empty_list() {
    let e = jexl().create_script("for(item : list) 1+1").expect("parse");
    let jc = ctx();
    jc.set("list", list(vec![])).expect("set");
    eq(&ok(e.execute(jc)), &Value::Null);
}

// port of: ForEachTest.testForEachWithArray
#[test]
fn test_for_each_with_array() {
    let e = jexl().create_script("for(item : list) item").expect("parse");
    let jc = ctx();
    jc.set("list", array(vec![s("Hello"), s("World")])).expect("set");
    eq(&ok(e.execute(jc)), &s("World"));
}

// port of: ForEachTest.testForEachWithCollection
#[test]
fn test_for_each_with_collection() {
    let e = jexl().create_script("for(var item : list) item").expect("parse");
    let jc = ctx();
    jc.set("list", list(vec![s("Hello"), s("World")])).expect("set");
    eq(&ok(e.execute(jc)), &s("World"));
}

// port of: ForEachTest.testForEachWithIterator
// adapted: Java binds `Arrays.asList(...).iterator()` as a context value; a java.util.Iterator is
// not a value this port can be handed from outside, so the script asks the list for it.
#[test]
fn test_for_each_with_iterator() {
    let e = jexl().create_script("for(var item : list.iterator()) item").expect("parse");
    let jc = ctx();
    jc.set("list", list(vec![s("Hello"), s("World")])).expect("set");
    eq(&ok(e.execute(jc)), &s("World"));
}

/// The stand-in for `System.getProperties()`: a HashMap, whose iteration order this port models.
fn properties() -> Value {
    map(vec![
        (s("java.version"), s("25")),
        (s("os.name"), s("Linux")),
        (s("user.name"), s("ec2-user")),
    ])
}

// port of: ForEachTest.testForEachWithMap
// adapted: `System.getProperties()` is JVM state; a HashMap with the same iteration model is used.
#[test]
fn test_for_each_with_map() {
    let e = jexl().create_script("for(item : list) item").expect("parse");
    let jc = ctx();
    let props = properties();
    jc.set("list", props.clone()).expect("set");
    // the last value the map iterates, computed the way the Java test computes it
    let last = match &props {
        Value::Map(m) => m.snapshot().last().expect("entries").1.clone(),
        _ => unreachable!(),
    };
    eq(&last, &s("ec2-user"));
    eq(&ok(e.execute(jc)), &last);
}

// port of: ForEachTest.testForEachWithBlock
#[test]
fn test_for_each_with_block() {
    let exs0 = jexl().create_script("for(var in : list) { x = x + in; }").expect("parse");
    let jc = ctx();
    jc.set("list", array(vec![i(2), i(3)])).expect("set");
    jc.set("x", i(1)).expect("set");
    eq(&ok(exs0.execute(jc.clone())), &i(6));
    eq(&jc.get("x").expect("x"), &i(6));
}

// port of: ForEachTest.testForEachWithProperty
#[test]
fn test_for_each_with_property() {
    let e = jexl().create_script("for(var item : list.cheeseList) item").expect("parse");
    let jc = ctx();
    jc.set("list", Value::object(Foo)).expect("set");
    eq(&ok(e.execute(jc)), &s("brie"));
}

// port of: ForEachTest.testForEachBreakMethod
#[test]
fn test_for_each_break_method() {
    let e = jexl()
        .create_script("var rr = -1; for(var item : [1, 2, 3 ,4 ,5, 6]) { if (item == 3) { rr = item; break; }} rr")
        .expect("parse");
    let jc = ctx();
    jc.set("list", Value::object(Foo)).expect("set");
    eq(&ok(e.execute(jc)), &i(3));
}

// port of: ForEachTest.testForEachContinueMethod
#[test]
fn test_for_each_continue_method() {
    let e = jexl()
        .create_script("var rr = 0; for(var item : [1, 2, 3 ,4 ,5, 6]) { if (item <= 3) continue; rr = rr + item;}")
        .expect("parse");
    let jc = ctx();
    jc.set("list", Value::object(Foo)).expect("set");
    eq(&ok(e.execute(jc)), &i(15));
}

// port of: ForEachTest.testForEachContinueBroken
#[test]
fn test_for_each_continue_broken() {
    let x = parse_err(jexl().create_script("var rr = 0; continue;"));
    assert_eq!(x.class_name(), "JexlException$Parsing");
    assert!(x.message().contains("continue"), "{}", x.message());
}

// port of: ForEachTest.testForEachBreakBroken
#[test]
fn test_for_each_break_broken() {
    let x = parse_err(jexl().create_script("if (true) { break; }"));
    assert_eq!(x.class_name(), "JexlException$Parsing");
    assert!(x.message().contains("break"), "{}", x.message());
}

// ================================================================== WhileTest

// port of: WhileTest.testSimpleWhileFalse
#[test]
fn test_simple_while_false() {
    let e = jexl().create_script("while (false) ;").expect("parse");
    eq(&ok(e.execute(ctx())), &Value::Null);
}

// port of: WhileTest.testWhileExecutesExpressionWhenLooping
#[test]
fn test_while_executes_expression_when_looping() {
    let e = jexl().create_script("while (x < 10) x = x + 1;").expect("parse");
    let jc = ctx();
    jc.set("x", i(1)).expect("set");
    eq(&ok(e.execute(jc)), &i(10));
}

// port of: WhileTest.testWhileWithBlock
#[test]
fn test_while_with_block() {
    let e = jexl().create_script("while (x < 10) { x = x + 1; y = y * 2; }").expect("parse");
    let jc = ctx();
    jc.set("x", i(1)).expect("set");
    jc.set("y", i(1)).expect("set");
    eq(&ok(e.execute(jc.clone())), &i(512));
    eq(&jc.get("x").expect("x"), &i(10));
    eq(&jc.get("y").expect("y"), &i(512));
}

// ================================================================== DoWhileTest

// port of: DoWhileTest.testSimpleWhileFalse
#[test]
fn test_do_simple_while_false() {
    let jexl = jexl();
    let jc = ctx();
    let mut e = jexl.create_script("do {} while (false)").expect("parse");
    eq(&ok(e.execute(jc.clone())), &Value::Null);
    e = jexl.create_script("do {} while (false); 23").expect("parse");
    eq(&ok(e.execute(jc)), &i(23));
}

// port of: DoWhileTest.testWhileExecutesExpressionWhenLooping
#[test]
fn test_do_while_executes_expression_when_looping() {
    let jexl = jexl();
    let jc = ctx();
    let mut e = jexl.create_script("do x = x + 1 while (x < 10)").expect("parse");
    jc.set("x", i(1)).expect("set");
    eq(&ok(e.execute(jc.clone())), &i(10));
    eq(&jc.get("x").expect("x"), &i(10));

    e = jexl.create_script("var x = 0; do x += 1; while (x < 23)").expect("parse");
    eq(&ok(e.execute(jc.clone())), &i(23));

    jc.set("x", i(1)).expect("set");
    e = jexl.create_script("do x += 1; while (x < 23); return 42;").expect("parse");
    let o = ok(e.execute(jc.clone()));
    eq(&jc.get("x").expect("x"), &i(23));
    eq(&o, &i(42));
}

// port of: DoWhileTest.testWhileWithBlock
#[test]
fn test_do_while_with_block() {
    let e = jexl().create_script("do { x = x + 1; y = y * 2; } while (x < 10)").expect("parse");
    let jc = ctx();
    jc.set("x", i(1)).expect("set");
    jc.set("y", i(1)).expect("set");
    eq(&ok(e.execute(jc.clone())), &i(512));
    eq(&jc.get("x").expect("x"), &i(10));
    eq(&jc.get("y").expect("y"), &i(512));
}

// port of: DoWhileTest.testForEachBreakInsideFunction
#[test]
fn test_for_each_break_inside_function() {
    let x = parse_err(jexl().create_script("for (i : 1..2) {  y = function() { break; } }"));
    assert_eq!(x.class_name(), "JexlException$Parsing");
    assert!(x.message().contains("break"), "{}", x.message());
}

// port of: DoWhileTest.testForEachContinueInsideFunction
#[test]
fn test_for_each_continue_inside_function() {
    let x = parse_err(jexl().create_script("for (i : 1..2) {  y = function() { continue; } }"));
    assert_eq!(x.class_name(), "JexlException$Parsing");
    assert!(x.message().contains("continue"), "{}", x.message());
}

// port of: DoWhileTest.testForEachLambda
#[test]
fn test_for_each_lambda() {
    let e = jexl().create_script("(x)->{ for (i : 1..2) {  continue; var y = function() { 42; } break; } }");
    assert!(e.is_ok(), "{}", e.err().map(|x| x.message()).unwrap_or_default());
}

// port of: DoWhileTest.testEmptyBody
#[test]
fn test_empty_body() {
    let e = jexl().create_script("var i = 0; do ; while((i+=1) < 10); i").expect("parse");
    eq(&ok(e.execute(ctx())), &i(10));
}

// port of: DoWhileTest.testEmptyStmtBody
#[test]
fn test_empty_stmt_body() {
    let e = jexl().create_script("var i = 0; do {} while((i+=1) < 10); i").expect("parse");
    eq(&ok(e.execute(ctx())), &i(10));
}

// port of: DoWhileTest.testWhileEmptyBody
#[test]
fn test_while_empty_body() {
    let e = jexl().create_script("var i = 0; while((i+=1) < 10); i").expect("parse");
    eq(&ok(e.execute(ctx())), &i(10));
}

// port of: DoWhileTest.testWhileEmptyStmtBody
#[test]
fn test_while_empty_stmt_body() {
    let e = jexl().create_script("var i = 0; while((i+=1) < 10) {}; i").expect("parse");
    eq(&ok(e.execute(ctx())), &i(10));
}

// ================================================================== BlockTest

// port of: BlockTest.testBlockSimple
#[test]
fn test_block_simple() {
    let e = jexl().create_script("if (true) { 'hello'; }").expect("parse");
    eq(&ok(e.execute(ctx())), &s("hello"));
}

// port of: BlockTest.testBlockExecutesAll
#[test]
fn test_block_executes_all() {
    let e = jexl().create_script("if (true) { x = 'Hello'; y = 'World';}").expect("parse");
    let jc = ctx();
    let o = ok(e.execute(jc.clone()));
    eq(&jc.get("x").expect("x"), &s("Hello"));
    eq(&jc.get("y").expect("y"), &s("World"));
    eq(&o, &s("World"));
}

// port of: BlockTest.testEmptyBlock
#[test]
fn test_empty_block() {
    let e = jexl().create_script("if (true) { }").expect("parse");
    eq(&ok(e.execute(ctx())), &Value::Null);
}

// port of: BlockTest.testBlockLastExecuted01
#[test]
fn test_block_last_executed01() {
    let e = jexl().create_script("if (true) { x = 1; } else { x = 2; }").expect("parse");
    eq(&ok(e.execute(ctx())), &i(1));
}

// port of: BlockTest.testBlockLastExecuted02
#[test]
fn test_block_last_executed02() {
    let e = jexl().create_script("if (false) { x = 1; } else { x = 2; }").expect("parse");
    eq(&ok(e.execute(ctx())), &i(2));
}

// port of: BlockTest.testNestedBlock
#[test]
fn test_nested_block() {
    let e = jexl()
        .create_script("if (true) { x = 'hello'; y = 'world'; if (true) { x; } y; }")
        .expect("parse");
    eq(&ok(e.execute(ctx())), &s("world"));
}

// ================================================================== AssignTest

/// port of: `new JexlBuilder().cache(512).strict(true).silent(false).create()`
fn assign_jexl() -> Arc<JexlEngine> {
    hosts(builder().cache(512).strict(true).silent(false)).create()
}

// port of: AssignTest.testAntish
#[test]
fn test_antish() {
    let jexl = assign_jexl();
    let assign = jexl.create_expression(None, "froboz.value = 10").expect("parse");
    let check = jexl.create_expression(None, "froboz.value").expect("parse");
    let jc = ctx();
    eq(&ok(assign.evaluate(jc.clone())), &i(10));
    eq(&ok(check.evaluate(jc)), &i(10));
}

// port of: AssignTest.testAntishInteger
#[test]
fn test_antish_integer() {
    let jexl = assign_jexl();
    let assign = jexl.create_expression(None, "froboz.0 = 10").expect("parse");
    let check = jexl.create_expression(None, "froboz.0").expect("parse");
    let jc = ctx();
    eq(&ok(assign.evaluate(jc.clone())), &i(10));
    eq(&ok(check.evaluate(jc)), &i(10));
}

// port of: AssignTest.testBeanish
#[test]
fn test_beanish() {
    let jexl = assign_jexl();
    let assign = jexl.create_expression(None, "froboz.value = 10").expect("parse");
    let check = jexl.create_expression(None, "froboz.value").expect("parse");
    let jc = ctx();
    jc.set("froboz", Value::object(Froboz(std::sync::Mutex::new(-169)))).expect("set");
    eq(&ok(assign.evaluate(jc.clone())), &i(10));
    eq(&ok(check.evaluate(jc)), &i(10));
}

// port of: AssignTest.testAmbiguous
#[test]
fn test_ambiguous() {
    let assign = assign_jexl().create_expression(None, "froboz.nosuchbean = 10").expect("parse");
    let jc = ctx();
    jc.set("froboz", Value::object(Froboz(std::sync::Mutex::new(-169)))).expect("set");
    let x = thrown(assign.evaluate(jc));
    assert!(x.message().contains("nosuchbean"), "{}", x.message());
}

// port of: AssignTest.testArray
#[test]
fn test_array_assign() {
    let jexl = assign_jexl();
    let assign = jexl.create_expression(None, "froboz[\"value\"] = 10").expect("parse");
    let check = jexl.create_expression(None, "froboz[\"value\"]").expect("parse");
    let jc = ctx();
    jc.set("froboz", Value::object(Froboz(std::sync::Mutex::new(0)))).expect("set");
    eq(&ok(assign.evaluate(jc.clone())), &i(10));
    eq(&ok(check.evaluate(jc)), &i(10));
}

// port of: AssignTest.testMini
#[test]
fn test_mini() {
    let assign = assign_jexl().create_expression(None, "quux = 10").expect("parse");
    eq(&ok(assign.evaluate(ctx())), &i(10));
}

// port of: AssignTest.testRejectLocal
#[test]
fn test_reject_local() {
    let jexl = assign_jexl();
    let jc = ctx();
    let assign = jexl.create_script("var quux = null; quux.froboz.value = 10").expect("parse");
    let x = thrown(assign.execute(jc.clone()));
    assert_eq!(x.class_name(), "JexlException$Property");
    // quux is a global antish var
    let assign = jexl.create_script("quux.froboz.value = 10").expect("parse");
    eq(&ok(assign.execute(jc)), &i(10));
}

// ================================================================== SideEffectTest
//
// The Asserter of the upstream tests evaluates scripts against a JexlEvalContext.

/// (script, expected result, expected value left in `foo`)
const SIDE_EFFECTS: [(&str, i64, bool); 8] = [
    ("+= 2", 4143, false),
    ("-= 2", 4139, false),
    ("*= 2", 8282, false),
    ("/= 2", 2070, false),
    ("%= 2", 1, false),
    ("&= 3", 1, true),
    ("|= 2", 4143, true),
    ("^= 2", 4143, true),
];

fn side_effect(n: i64, long: bool) -> Value {
    if long {
        l(n)
    } else {
        i(n as i32)
    }
}

// port of: SideEffectTest.testSideEffectVar
#[test]
fn test_side_effect_var() {
    let jexl = jexl();
    let jc = EvalContext::new();
    for (op, want, long) in SIDE_EFFECTS {
        jc.set("foo", i(4141)).expect("set");
        let e = jexl.create_script(&format!("foo {}", op)).expect("parse");
        eq(&ok(e.execute(jc.clone())), &side_effect(want, long));
        eq(&jc.get("foo").expect("foo"), &side_effect(want, long));
    }
}

// port of: SideEffectTest.testSideEffectVarDots
#[test]
fn test_side_effect_var_dots() {
    let jexl = jexl();
    let jc = EvalContext::new();
    for (op, want, long) in SIDE_EFFECTS {
        jc.set("foo.bar.quux", i(4141)).expect("set");
        let e = jexl.create_script(&format!("foo.bar.quux {}", op)).expect("parse");
        eq(&ok(e.execute(jc.clone())), &side_effect(want, long));
        eq(&jc.get("foo.bar.quux").expect("foo.bar.quux"), &side_effect(want, long));
    }
}

fn foo_array() -> Value {
    array(vec![Value::Null, i(42), i(43)])
}

fn at(v: &Value, n: usize) -> Value {
    match v {
        Value::Array(a) => a.get(n).expect("index"),
        _ => unreachable!(),
    }
}

fn set_at(v: &Value, n: usize, x: Value) {
    match v {
        Value::Array(a) => assert!(a.set(n, x)),
        _ => unreachable!(),
    }
}

// port of: SideEffectTest.testSideEffectArray
#[test]
fn test_side_effect_array() {
    let jexl = jexl();
    let jc = EvalContext::new();
    let foo = foo_array();
    jc.set("foo", foo.clone()).expect("set");
    for (op, want, long) in SIDE_EFFECTS {
        set_at(&foo, 0, i(4141));
        let e = jexl.create_script(&format!("foo[0] {}", op)).expect("parse");
        eq(&ok(e.execute(jc.clone())), &side_effect(want, long));
        eq(&at(&foo, 0), &side_effect(want, long));
    }
}

// port of: SideEffectTest.testSideEffectDotArray
#[test]
fn test_side_effect_dot_array() {
    let jexl = jexl();
    let jc = EvalContext::new();
    let foo = foo_array();
    jc.set("foo", foo.clone()).expect("set");
    for (op, want, long) in SIDE_EFFECTS {
        set_at(&foo, 0, i(4141));
        let e = jexl.create_script(&format!("foo.0 {}", op)).expect("parse");
        eq(&ok(e.execute(jc.clone())), &side_effect(want, long));
        eq(&at(&foo, 0), &side_effect(want, long));
    }
}

// port of: SideEffectTest.testSideEffectAntishArray
#[test]
fn test_side_effect_antish_array() {
    let jexl = jexl();
    let jc = EvalContext::new();
    let foo = foo_array();
    jc.set("foo.bar", foo.clone()).expect("set");
    for (op, want, long) in SIDE_EFFECTS {
        set_at(&foo, 0, i(4141));
        let e = jexl.create_script(&format!("foo.bar[0] {}", op)).expect("parse");
        eq(&ok(e.execute(jc.clone())), &side_effect(want, long));
        eq(&at(&foo, 0), &side_effect(want, long));
    }
}

// port of: SideEffectTest.testSideEffectBean
#[test]
fn test_side_effect_bean() {
    let jexl = jexl();
    let jc = EvalContext::new();
    let bean = Arc::new(SeFoo(std::sync::Mutex::new(0)));
    jc.set("foo", Value::Object(bean.clone())).expect("set");
    for (op, want, long) in SIDE_EFFECTS {
        *bean.0.lock().unwrap() = 4141;
        let e = jexl.create_script(&format!("foo.value {}", op)).expect("parse");
        eq(&ok(e.execute(jc.clone())), &side_effect(want, long));
        // the bean stores an int, so the Long the bitwise operators produce is narrowed
        assert_eq!(*bean.0.lock().unwrap(), want as i32);
    }
}

// ================================================================== MethodTest

// port of: MethodTest.testMethod
#[test]
fn test_method() {
    let e = jexl().create_script("foo.bar()").expect("parse");
    let jc = EvalContext::new();
    jc.set("foo", Value::object(Foo)).expect("set");
    eq(&ok(e.execute(jc)), &s("Method string"));
}

// port of: MethodTest.testMulti
#[test]
fn test_multi() {
    let e = jexl().create_script("foo.innerFoo.bar()").expect("parse");
    let jc = EvalContext::new();
    jc.set("foo", Value::object(Foo)).expect("set");
    eq(&ok(e.execute(jc)), &s("Method string"));
}

// port of: MethodTest.testStringMethods
#[test]
fn test_string_methods() {
    let jexl = jexl();
    let jc = EvalContext::new();
    jc.set("foo", s("abcdef")).expect("set");
    for (src, want) in [
        ("foo.substring(3)", "def"),
        ("foo.substring(0,(size(foo)-3))", "abc"),
        ("foo.substring(0,size(foo)-3)", "abc"),
        ("foo.substring(0,foo.length()-3)", "abc"),
        ("foo.substring(0, 1+1)", "ab"),
    ] {
        let e = jexl.create_script(src).expect("parse");
        eq(&ok(e.execute(jc.clone())), &s(want));
    }
}

// port of: MethodTest.testStaticMethodInvocation
#[test]
fn test_static_method_invocation() {
    let e = jexl().create_script("aBool.valueOf('true')").expect("parse");
    let jc = EvalContext::new();
    jc.set("aBool", Value::Boolean(false)).expect("set");
    eq(&ok(e.execute(jc)), &Value::Boolean(true));
}

// ================================================================== AntishCallTest

// port of: AntishCallTest.testSafeAnt
#[test]
fn test_safe_ant() {
    let jexl = jexl();
    let ctxt = EvalContext::new();
    ctxt.set("x.y.z", i(42)).expect("set");

    let script = jexl.create_script("x.y.z").expect("parse");
    eq(&ok(script.execute(ctxt.clone())), &i(42));
    eq(&ctxt.get("x.y.z").expect("x.y.z"), &i(42));

    ctxt.options(|o| o.set_antish(false));
    let x = thrown(script.execute(ctxt.clone()));
    assert_eq!(x.class_name(), "JexlException$Variable");
    assert_eq!(x.get_detail().expect("var").to_rust(), "x");
    ctxt.options(|o| o.set_antish(true));

    let script = jexl.create_script("x?.y?.z").expect("parse");
    eq(&ok(script.execute(ctxt.clone())), &Value::Null); // safe navigation, null

    for src in ["x?.y?.z = 3", "x.y?.z", "x.y?.z = 3", "x.`'y'`.z = 3"] {
        let script = jexl.create_script(src).expect("parse");
        let x = thrown(script.execute(ctxt.clone()));
        assert!(x.is_jexl(), "{}: {}", src, x.message());
    }
}

// ================================================================== ContextNamespaceTest

// The JEXL-348 tests below name `ContextNamespaceTest$Ns348.func(int)`, a static method of a Java
// class; the registered test host's `twice(int)` stands in for it, so 42 * y becomes 2 * y.
fn run348a(jexl: &Arc<JexlEngine>, ctxt: Arc<dyn JexlContext>, ns: &str) {
    let src = format!("empty(x) ? {}twice(y) : z", ns);
    let script = jexl
        .create_script_named(&src, &["x".into(), "y".into(), "z".into()])
        .expect("parse");
    eq(&ok(script.execute_args(ctxt.clone(), &[Value::Null, i(1), i(169)])), &i(2));
    eq(&ok(script.execute_args(ctxt, &[s("42"), i(1), i(169)])), &i(169));
}

fn run348b(jexl: &Arc<JexlEngine>, ctxt: Arc<dyn JexlContext>, ns: &str) {
    let src = format!("empty(x) ? {}twice(y) : z", ns);
    let script = jexl.create_script(&src).expect("parse");
    ctxt.set("x", Value::Null).expect("set");
    ctxt.set("y", i(1)).expect("set");
    ctxt.set("z", i(169)).expect("set");
    eq(&ok(script.execute(ctxt.clone())), &i(2));
    ctxt.set("x", s("42")).expect("set");
    eq(&ok(script.execute(ctxt)), &i(169));
}

fn run348c(jexl: &Arc<JexlEngine>, ctxt: Arc<dyn JexlContext>, ns: &str) {
    let src = format!("empty(x) ? z : {}twice(y)", ns);
    let script = jexl
        .create_script_named(&src, &["x".into(), "z".into(), "y".into()])
        .expect("parse");
    eq(&ok(script.execute_args(ctxt.clone(), &[Value::Null, i(169), i(1)])), &i(169));
    eq(&ok(script.execute_args(ctxt, &[s("42"), i(169), i(1)])), &i(2));
}

fn run348d(jexl: &Arc<JexlEngine>, ctxt: Arc<dyn JexlContext>, ns: &str) {
    let src = format!("empty(x) ? z : {}twice(y)", ns);
    let script = jexl.create_script(&src).expect("parse");
    ctxt.set("x", Value::Null).expect("set");
    ctxt.set("z", i(169)).expect("set");
    ctxt.set("y", i(1)).expect("set");
    eq(&ok(script.execute(ctxt.clone())), &i(169));
    ctxt.set("x", s("42")).expect("set");
    eq(&ok(script.execute(ctxt)), &i(2));
}

fn ns_map() -> std::collections::HashMap<String, Value> {
    let mut ns = std::collections::HashMap::new();
    ns.insert("ns".to_string(), Value::object(Bean));
    ns
}

// port of: ContextNamespaceTest.testNamespace348a
#[test]
fn test_namespace348a() {
    let ctxt = ctx();
    let jexl = hosts(builder().safe(false).namespaces(ns_map())).create();
    run348a(&jexl, ctxt.clone(), "ns : ");
    run348b(&jexl, ctxt.clone(), "ns : ");
    run348c(&jexl, ctxt.clone(), "ns : ");
    run348d(&jexl, ctxt, "ns : ");
}

// port of: ContextNamespaceTest.testNamespace348b
#[test]
fn test_namespace348b() {
    let jexl = hosts(builder().safe(false)).create();
    // no space for ns name as syntactic hint; the context resolves the namespace
    run348a(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns:");
    run348b(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns:");
    run348c(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns:");
    run348d(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns:");
}

// port of: ContextNamespaceTest.testNamespace348c
#[test]
fn test_namespace348c() {
    let f = rust_jexl3::jexl_features::JexlFeatures::new().namespace_test(Some(Arc::new(|_| true)));
    let jexl = hosts(builder().namespaces(ns_map()).features(f).safe(false)).create();
    run348a(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns : ");
    run348b(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns : ");
    run348c(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns : ");
    run348d(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns : ");
}

// port of: ContextNamespaceTest.testNamespace348d
#[test]
fn test_namespace348d() {
    let f = rust_jexl3::jexl_features::JexlFeatures::new().namespace_test(Some(Arc::new(|_| true)));
    let jexl = hosts(builder().features(f).safe(false)).create();
    run348a(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns : ");
    run348b(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns : ");
    run348c(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns : ");
    run348d(&jexl, EvalContext::with_namespace("ns", Value::object(Bean)), "ns : ");
}



// ------------------------------------------------------------------ SideEffectTest.SelfArithmetic
//
// Java finds the overloads by reflecting on a JexlArithmetic subclass; the port's equivalent is
// `JexlUberspect::get_operator`, so the same four property overloads are registered there.

/// port of: SideEffectTest.Var
#[derive(Debug)]
struct Var(std::sync::Mutex<i32>);

impl HostObject for Var {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.SideEffectTest$Var".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some(self.0.lock().unwrap().to_string())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: SideEffectTest.SelfArithmetic's propertyGet/propertySet/arrayGet/arraySet overloads.
struct SelfUberspect(Uberspect);

impl rust_jexl3::introspection::JexlUberspect for SelfUberspect {
    fn get_resolvers(
        &self,
        op: Option<rust_jexl3::jexl_operator::JexlOperator>,
        obj: &Value,
    ) -> &'static [rust_jexl3::introspection::PropertyResolver] {
        self.0.get_resolvers(op, obj)
    }
    fn get_constructor(&self, h: &Value, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        self.0.get_constructor(h, args)
    }
    fn get_method(&self, obj: &Value, m: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        self.0.get_method(obj, m, args)
    }
    fn get_property_get_with(
        &self,
        r: &[rust_jexl3::introspection::PropertyResolver],
        obj: &Value,
        id: &Value,
    ) -> Option<Arc<dyn JexlPropertyGet>> {
        self.0.get_property_get_with(r, obj, id)
    }
    fn get_property_set_with(
        &self,
        r: &[rust_jexl3::introspection::PropertyResolver],
        obj: &Value,
        id: &Value,
        arg: &Value,
    ) -> Option<Arc<dyn JexlPropertySet>> {
        self.0.get_property_set_with(r, obj, id, arg)
    }
    fn get_iterator(&self, obj: &Value) -> Option<Box<dyn Iterator<Item = Value> + Send>> {
        self.0.get_iterator(obj)
    }
    fn overloads(&self, operator: rust_jexl3::jexl_operator::JexlOperator) -> bool {
        use rust_jexl3::jexl_operator::JexlOperator::*;
        matches!(operator, PropertyGet | PropertySet | ArrayGet | ArraySet)
    }
    fn get_operator(
        &self,
        operator: rust_jexl3::jexl_operator::JexlOperator,
        args: &[Value],
    ) -> Option<Arc<dyn JexlMethod>> {
        use rust_jexl3::jexl_operator::JexlOperator::*;
        let key = match operator {
            PropertyGet | PropertySet => "value",
            ArrayGet | ArraySet => "VALUE",
            _ => return None,
        };
        if args.first()?.as_host::<Var>().is_none() || args.get(1)?.java_to_string() != key {
            return None;
        }
        match operator {
            PropertyGet | ArrayGet => Some(Arc::new(HostMethod {
                ret: "java.lang.Object",
                call: |_, a| Ok(i(*a[0].as_host::<Var>().expect("var").0.lock().unwrap())),
            })),
            _ => {
                to_int(args.get(2)?)?;
                Some(Arc::new(HostMethod {
                    ret: "java.lang.Object",
                    call: |_, a| {
                        let v = to_int(&a[2]).expect("int");
                        *a[0].as_host::<Var>().expect("var").0.lock().unwrap() = v;
                        Ok(i(v))
                    },
                }))
            }
        }
    }
}

// port of: SideEffectTest.testOverrideGetSet
#[test]
fn test_override_get_set() {
    let uber = SelfUberspect(Uberspect::new());
    let jexl = builder()
        .cache(64)
        .arithmetic(rust_jexl3::jexl_arithmetic::JexlArithmetic::new(false, None, i32::MIN))
        .uberspect(Arc::new(uber))
        .create();
    let jc = empty_context();
    let v0 = Value::object(Var(std::sync::Mutex::new(3115)));

    let script = jexl.create_script("(x)->{ x.value}").expect("parse");
    eq(&ok(script.execute_args(jc.clone(), &[v0.clone()])), &i(3115));
    let script = jexl.create_script("(x)->{ x['VALUE']}").expect("parse");
    eq(&ok(script.execute_args(jc.clone(), &[v0.clone()])), &i(3115));
    let script = jexl.create_script("(x,y)->{ x.value = y}").expect("parse");
    eq(&ok(script.execute_args(jc.clone(), &[v0.clone(), i(42)])), &i(42));
    let script = jexl.create_script("(x,y)->{ x['VALUE'] = y}").expect("parse");
    eq(&ok(script.execute_args(jc, &[v0, i(169)])), &i(169));
}
