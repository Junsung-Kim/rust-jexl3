//! Ports of the upstream engine-level test classes of Apache Commons JEXL 3.2.1
//! (`src/test/java/org/apache/commons/jexl3/`).
//!
//! Sources: `JexlTest`, `BuilderTest`, `CacheTest`, `FeaturesTest`, `StrategyTest`,
//! `ExceptionTest`, `ParseFailuresTest` and `ScriptTest`. All eight classes were compiled against
//! `commons-jexl3-3.2.1.jar` and run green under JUnit 4.13.2 on Corretto 25 before a single
//! assertion was transcribed; anything the Java source could not settle (the resolver order a
//! `HashMap` gets for `i.class`, what `size` reports for a `BitSet`) was measured separately
//! through `oracle/target/oracle`.
//!
//! Every `#[test]` is one Java test method, named in snake_case, with the Java class and method in
//! a comment above it. `JexlTestCase` installs `JexlOptions.setDefaultFlags("-safe", "+lexical")`
//! for the whole upstream suite; `builder()` below is that default. Java discovers the test beans
//! (`Foo`, `Duck`, `Cached0`...) by reflection; this port registers them through
//! `HostIntrospector`, the SPI an embedder uses for its own types, modelling only the members the
//! ported tests touch.
#![allow(clippy::bool_assert_comparison)]

mod common;

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use common::upstream::{assert_java_eq, class_of, string_array, HostGet, HostMethod, HostSet, JexlEvalContext};
use rust_jexl::introspection::jdk_shim::{HostIntrospector, JdkShim};
use rust_jexl::introspection::uberspect::Uberspect;
use rust_jexl::introspection::{
    JexlMethod, JexlPropertyGet, JexlPropertySet, JexlUberspect, PropertyResolver, ResolverStrategy,
};
use rust_jexl::jexl_arithmetic::JexlArithmetic;
use rust_jexl::jexl_context::{JexlContext, MapContext};
use rust_jexl::jexl_engine::{empty_context, JexlBuilder, JexlEngine};
use rust_jexl::jexl_exception::{ExceptionKind, JexlException};
use rust_jexl::jexl_features::JexlFeatures;
use rust_jexl::value::{Component, HostObject, JArray, JList, JMap, ListKind, Value};

// --------------------------------------------------------------------------------------- harness

/// port of: JexlTestCase's static initializer, `JexlOptions.setDefaultFlags("-safe", "+lexical")`.
fn builder() -> JexlBuilder {
    JexlBuilder::new().uberspect(beans()).safe(false).lexical(true)
}

/// port of: `JexlTestCase(String)` — `new JexlBuilder().cache(128).create()`.
fn jexl() -> Arc<JexlEngine> {
    builder().cache(128).create()
}

/// port of: `JexlTestCase.createEngine(boolean lenient)`.
fn create_engine(lenient: bool) -> Arc<JexlEngine> {
    builder().arithmetic(JexlArithmetic::new(!lenient, None, i32::MIN)).cache(128).create()
}

/// The uberspect every ported test uses: the JDK shim plus the test beans below.
fn beans() -> Arc<dyn JexlUberspect> {
    let shim = JdkShim::new(ResolverStrategy::Jexl).with_hosts(Arc::new(Beans));
    Arc::new(Uberspect::new().with_shim(Arc::new(shim)))
}

fn ctx() -> Arc<MapContext> {
    Arc::new(MapContext::new())
}

fn i(n: i32) -> Value {
    Value::Integer(n)
}
fn s(t: &str) -> Value {
    Value::string(t)
}
fn b(flag: bool) -> Value {
    Value::Boolean(flag)
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

fn ok(r: Result<Value, JexlException>) -> Value {
    r.unwrap_or_else(|e| panic!("unexpected exception: {}", e.message()))
}

fn thrown(r: Result<Value, JexlException>) -> JexlException {
    match r {
        Ok(v) => panic!("should have thrown, got {:?}", v),
        Err(e) => e,
    }
}

#[track_caller]
fn eq(want: &Value, got: &Value) {
    assert_java_eq(want, got);
}

/// port of: `JexlTest.assertExpression(JexlContext, String, Object)`.
#[track_caller]
fn assert_expression(jexl: &Arc<JexlEngine>, jc: Arc<dyn JexlContext>, expression: &str, expected: &Value) {
    let e = jexl.create_expression(None, expression).expect("createExpression");
    let actual = match e.evaluate(jc) {
        Ok(v) => v,
        Err(x) => panic!("{}: {}", expression, x.message()),
    };
    assert!(
        expected.java_equals(&actual),
        "{}: expected {:?} ({}), got {:?} ({})",
        expression,
        expected.java_to_string(),
        expected.class_name(),
        actual.java_to_string(),
        actual.class_name()
    );
}

// ------------------------------------------------------------------------------ the upstream beans
//
// Java discovers these by reflection; the port registers them through `HostIntrospector`, the SPI
// an embedder uses for its own types. Only the members the ported tests touch are modelled.
//
// `HostIntrospector` is consulted after every `PropertyResolver` has failed, so a bean that Java
// resolves through more than one resolver (`Cached3`, a `TreeMap` subclass whose `flag` comes from
// `isflag()` but whose `value` comes from the map) models the *outcome* of that order here. The
// outcome was measured on the JVM, not assumed.

/// port of: `org.apache.commons.jexl3.JexlTest.METHOD_STRING`.
const METHOD_STRING: &str = "Method string";
/// port of: `org.apache.commons.jexl3.JexlTest.GET_METHOD_STRING`.
const GET_METHOD_STRING: &str = "GetMethod string";

/// port of: `org.apache.commons.jexl3.Foo`.
#[derive(Debug, Default)]
struct Foo {
    been_modified: AtomicBool,
    property1: Mutex<String>,
}

impl Foo {
    fn new() -> Value {
        Value::object(Foo { been_modified: AtomicBool::new(false), property1: Mutex::new("some value".into()) })
    }
    fn modified(v: &Value) -> bool {
        v.as_host::<Foo>().expect("Foo").been_modified.load(Ordering::Relaxed)
    }
}

impl HostObject for Foo {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.Foo".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `JexlTest.Duck` — duck-typed `get(String)` / `set(String, Object)`.
#[derive(Debug)]
struct Duck {
    user: AtomicI32,
}

impl Duck {
    fn new() -> Value {
        Value::object(Duck { user: AtomicI32::new(10) })
    }
    // port of: Duck.get(String)
    fn get(&self, val: &str) -> Value {
        match val {
            "zero" => i(0),
            "one" => i(1),
            "user" => i(self.user.load(Ordering::Relaxed)),
            _ => i(-1),
        }
    }
    // port of: Duck.set(String, Object)
    fn set(&self, val: &str, value: &Value) {
        if val == "user" {
            let n = match value {
                Value::String(t) if t.to_rust() == "zero" => 0,
                Value::String(t) if t.to_rust() == "one" => 1,
                Value::Integer(n) => *n,
                _ => -1,
            };
            self.user.store(n, Ordering::Relaxed);
        }
    }
}

impl HostObject for Duck {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.JexlTest$Duck".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `java.util.BitSet` — `new BitSet(5)`, whose `size()` is the 64-bit word it lives in.
#[derive(Debug)]
struct BitSet;

impl HostObject for BitSet {
    fn class_name(&self) -> String {
        "java.util.BitSet".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `ScriptTest.Tester` — `getCode()` / `setCode(String)`.
#[derive(Debug)]
struct Tester(Mutex<Value>);

impl Tester {
    fn new() -> Value {
        Value::object(Tester(Mutex::new(Value::Null)))
    }
    fn code(v: &Value) -> Value {
        v.as_host::<Tester>().expect("Tester").0.lock().expect("lock").clone()
    }
    fn set_code(v: &Value, code: Value) {
        *v.as_host::<Tester>().expect("Tester").0.lock().expect("lock") = code;
    }
}

impl HostObject for Tester {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.ScriptTest$Tester".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `ExceptionTest.ThrowNPE` — every member throws a `NullPointerException` on demand.
#[derive(Debug)]
struct ThrowNPE {
    do_throw: AtomicBool,
}

impl ThrowNPE {
    fn new() -> Value {
        Value::object(ThrowNPE { do_throw: AtomicBool::new(false) })
    }
}

impl HostObject for ThrowNPE {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.ExceptionTest$ThrowNPE".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn npe(msg: &str) -> JexlException {
    JexlException::java("java.lang.NullPointerException", Some(msg.to_string()))
}

/// port of: `CacheTest.Cached0` .. `CacheTest.Cached4` — five beans that expose the same two
/// properties (`value`, `flag`) through five different accessor shapes.
///
/// `flavour` is the `Cached<n>` the bean's `getClass().getSimpleName()` reports, which is what
/// every `CacheTest` assertion compares against.
#[derive(Debug)]
struct Cached {
    flavour: usize,
    value: Mutex<Value>,
    flag: Mutex<Value>,
}

impl Cached {
    fn new(flavour: usize) -> Value {
        Value::object(Cached {
            flavour,
            value: Mutex::new(Value::string(&format!("Cached{}:new", flavour))),
            flag: Mutex::new(Value::Boolean(false)),
        })
    }
    fn of(v: &Value) -> &Cached {
        v.as_host::<Cached>().expect("Cached")
    }
    /// The value every `set` stores: `"Cached<n>:" + (arg == null ? "na" : arg)`.
    fn store_value(&self, arg: &Value) {
        let text = if arg.is_null() { "na".to_string() } else { arg.java_to_string() };
        *self.value.lock().expect("lock") = Value::string(&format!("Cached{}:{}", self.flavour, text));
    }
    // port of: Cached.compute / Cached.COMPUTE, whose result names the receiver and the signature
    fn compute(prefix: &str, args: &[Value]) -> Option<Value> {
        let tag = |v: &Value| match v {
            Value::String(_) => Some(format!("s#{}", v.java_to_string())),
            Value::Integer(_) => Some(format!("i#{}", v.java_to_string())),
            _ => None,
        };
        // a null argument is ambiguous between compute(String) and compute(Integer): no method
        let tags: Option<Vec<String>> = args.iter().map(tag).collect();
        let tags = tags?;
        // compute(String, String) and compute(int, int) exist; mixing them does not
        if tags.len() == 2 && tags[0].as_bytes()[0] != tags[1].as_bytes()[0] {
            return None;
        }
        Some(Value::string(&format!("{}@{}", prefix, tags.join(","))))
    }
}

impl HostObject for Cached {
    fn class_name(&self) -> String {
        format!("org.apache.commons.jexl3.CacheTest$Cached{}", self.flavour)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `Cached.class`, `Cached1.class`, `Cached2.class` used as JEXL namespaces, which
/// resolves the *static* `Cached.COMPUTE` on all three.
#[derive(Debug)]
struct CachedClass(usize);

impl HostObject for CachedClass {
    fn class_name(&self) -> String {
        "java.lang.Class".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some(format!("class org.apache.commons.jexl3.CacheTest$Cached{}", self.0))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: the reflective discovery Java performs over the test bean classes.
struct Beans;

fn to_i32(v: &Value) -> Option<i32> {
    match v {
        Value::Byte(n) => Some(*n as i32),
        Value::Short(n) => Some(*n as i32),
        Value::Integer(n) => Some(*n),
        _ => None,
    }
}

impl HostIntrospector for Beans {
    fn get_method(&self, obj: &Value, name: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        if obj.as_host::<Foo>().is_some() {
            return match (name, args.len()) {
                ("bar", 0) => Some(HostMethod::new("java.lang.String", |_, _| Ok(s(METHOD_STRING)))),
                ("getBar", 0) => Some(HostMethod::new("java.lang.String", |_, _| Ok(s(GET_METHOD_STRING)))),
                ("getQuux", 0) => Some(HostMethod::new("java.lang.String", |_, _| Ok(s("String : quux")))),
                ("getInnerFoo", 0) => Some(HostMethod::new("org.apache.commons.jexl3.Foo", |_, _| Ok(Foo::new()))),
                ("repeat", 1) => Some(HostMethod::new("java.lang.String", |_, a| {
                    Ok(s(&format!("Repeat : {}", a[0].java_to_string())))
                })),
                ("convertBoolean", 1) => Some(HostMethod::new("java.lang.String", |_, a| {
                    Ok(s(&format!("Boolean : {}", matches!(a[0], Value::Boolean(true)))))
                })),
                ("getCount", 0) => Some(HostMethod::new("int", |_, _| Ok(i(5)))),
                ("getSize", 0) => Some(HostMethod::new("int", |_, _| Ok(i(22)))),
                ("square", 1) => Some(HostMethod::new("int", |_, a| {
                    let n = to_i32(&a[0]).expect("int");
                    Ok(i(n.wrapping_mul(n)))
                })),
                ("isSimple", 0) => Some(HostMethod::new("boolean", |_, _| Ok(b(true)))),
                ("getCheeseList", 0) => Some(HostMethod::new("java.util.List", |_, _| {
                    Ok(list(vec![s("cheddar"), s("edam"), s("brie")]))
                })),
                ("getTrueAndModify", 0) => Some(HostMethod::new("boolean", |o, _| {
                    o.as_host::<Foo>().expect("Foo").been_modified.store(true, Ordering::Relaxed);
                    Ok(b(true))
                })),
                ("getModified", 0) => Some(HostMethod::new("boolean", |o, _| Ok(b(Foo::modified(o))))),
                ("getArray", 0) => {
                    Some(HostMethod::new("[Ljava.lang.String;", |_, _| Ok(string_array(&["One", "Two", "Three"]))))
                }
                ("getProperty1", 0) => Some(HostMethod::new("java.lang.String", |o, _| {
                    Ok(s(&o.as_host::<Foo>().expect("Foo").property1.lock().expect("lock")))
                })),
                ("setProperty1", 1) => Some(HostMethod::new("void", |o, a| {
                    *o.as_host::<Foo>().expect("Foo").property1.lock().expect("lock") = a[0].java_to_string();
                    Ok(Value::Null)
                })),
                _ => None,
            };
        }
        if obj.as_host::<BitSet>().is_some() {
            // port of: BitSet.size() — the 64-bit word `new BitSet(5)` lives in
            return match (name, args.len()) {
                ("size", 0) => Some(HostMethod::new("int", |_, _| Ok(i(64)))),
                _ => None,
            };
        }
        if obj.as_host::<Tester>().is_some() {
            return match (name, args.len()) {
                ("getCode", 0) => Some(HostMethod::new("java.lang.String", |o, _| Ok(Tester::code(o)))),
                ("setCode", 1) => Some(HostMethod::new("void", |o, a| {
                    Tester::set_code(o, a[0].clone());
                    Ok(Value::Null)
                })),
                _ => None,
            };
        }
        if obj.as_host::<ThrowNPE>().is_some() {
            // port of: ThrowNPE.npe(); anything else is simply not a method
            return match (name, args.len()) {
                ("npe", 0) => Some(HostMethod::new("java.lang.String", |_, _| Err(npe("ThrowNPE")))),
                _ => None,
            };
        }
        if let Some(c) = obj.as_host::<Cached>() {
            let prefix = format!("Cached{}", c.flavour);
            return match name {
                // compute(String) / compute(String, String) / compute(Integer) / compute(int, int)
                "compute" if !args.is_empty() && args.len() < 3 => {
                    Cached::compute(&prefix, args).map(|r| HostMethod::new("java.lang.String", move |_, _| Ok(r.clone())))
                }
                // ambiguous(Integer, int) and ambiguous(int, Integer) are never more specific
                _ => None,
            };
        }
        if let Some(c) = obj.as_host::<CachedClass>() {
            let _ = c;
            return match name {
                "COMPUTE" if !args.is_empty() && args.len() < 3 => {
                    Cached::compute("CACHED", args).map(|r| HostMethod::new("java.lang.String", move |_, _| Ok(r.clone())))
                }
                _ => None,
            };
        }
        None
    }

    fn get_property_get(&self, obj: &Value, identifier: &Value) -> Option<Arc<dyn JexlPropertyGet>> {
        let name = identifier.java_to_string();
        if obj.as_host::<Foo>().is_some() {
            return match name.as_str() {
                "bar" => Some(HostGet::new(|_| Ok(s(GET_METHOD_STRING)))),
                "quux" => Some(HostGet::new(|_| Ok(s("String : quux")))),
                "count" => Some(HostGet::new(|_| Ok(i(5)))),
                "size" => Some(HostGet::new(|_| Ok(i(22)))),
                "simple" => Some(HostGet::new(|_| Ok(b(true)))),
                "cheeseList" => Some(HostGet::new(|_| Ok(list(vec![s("cheddar"), s("edam"), s("brie")])))),
                "innerFoo" => Some(HostGet::new(|_| Ok(Foo::new()))),
                "array" => Some(HostGet::new(|_| Ok(string_array(&["One", "Two", "Three"])))),
                "trueAndModify" => Some(HostGet::new(|o| {
                    o.as_host::<Foo>().expect("Foo").been_modified.store(true, Ordering::Relaxed);
                    Ok(b(true))
                })),
                "modified" => Some(HostGet::new(|o| Ok(b(Foo::modified(o))))),
                "property1" => Some(HostGet::new(|o| {
                    Ok(s(&o.as_host::<Foo>().expect("Foo").property1.lock().expect("lock")))
                })),
                _ => None,
            };
        }
        if obj.as_host::<Duck>().is_some() {
            // the DUCK resolver: Duck.get(String)
            return Some(HostGet::new(move |o| Ok(o.as_host::<Duck>().expect("Duck").get(&name))));
        }
        if obj.as_host::<Tester>().is_some() && name == "code" {
            return Some(HostGet::new(|o| Ok(Tester::code(o))));
        }
        if obj.as_host::<ThrowNPE>().is_some() {
            // port of: ThrowNPE.getFail(); `foo` is not a property at all
            return match name.as_str() {
                "fail" => Some(HostGet::new(|o| {
                    let t = o.as_host::<ThrowNPE>().expect("ThrowNPE");
                    if t.do_throw.load(Ordering::Relaxed) {
                        return Err(npe("ThrowNPE/get"));
                    }
                    Ok(b(false))
                })),
                _ => None,
            };
        }
        if obj.as_host::<Cached>().is_some() {
            return match name.as_str() {
                "value" => Some(HostGet::new(|o| Ok(Cached::of(o).value.lock().expect("lock").clone()))),
                "flag" => Some(HostGet::new(|o| Ok(Cached::of(o).flag.lock().expect("lock").clone()))),
                _ => None,
            };
        }
        None
    }

    fn get_property_set(&self, obj: &Value, identifier: &Value, arg: &Value) -> Option<Arc<dyn JexlPropertySet>> {
        let name = identifier.java_to_string();
        if obj.as_host::<Foo>().is_some() && name == "property1" {
            return Some(HostSet::new(|o, v| {
                *o.as_host::<Foo>().expect("Foo").property1.lock().expect("lock") = v.java_to_string();
                Ok(v.clone())
            }));
        }
        if obj.as_host::<Duck>().is_some() {
            // the DUCK resolver: Duck.set(String, Object)
            return Some(HostSet::new(move |o, v| {
                o.as_host::<Duck>().expect("Duck").set(&name, v);
                Ok(v.clone())
            }));
        }
        if obj.as_host::<Tester>().is_some() && name == "code" {
            return Some(HostSet::new(|o, v| {
                Tester::set_code(o, v.clone());
                Ok(v.clone())
            }));
        }
        if obj.as_host::<ThrowNPE>().is_some() {
            return match name.as_str() {
                "fail" => Some(HostSet::new(|o, v| {
                    let t = o.as_host::<ThrowNPE>().expect("ThrowNPE");
                    let flag = matches!(v, Value::Boolean(true));
                    t.do_throw.store(flag, Ordering::Relaxed);
                    if flag {
                        return Err(npe("ThrowNPE/set"));
                    }
                    Ok(v.clone())
                })),
                _ => None,
            };
        }
        if obj.as_host::<Cached>().is_some() {
            let _ = arg;
            return match name.as_str() {
                "value" => Some(HostSet::new(|o, v| {
                    Cached::of(o).store_value(v);
                    Ok(v.clone())
                })),
                "flag" => Some(HostSet::new(|o, v| {
                    *Cached::of(o).flag.lock().expect("lock") = Value::Boolean(matches!(v, Value::Boolean(true)));
                    Ok(v.clone())
                })),
                _ => None,
            };
        }
        None
    }

    fn get_constructor(&self, handle: &Value, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        // port of: `new('org.apache.commons.jexl3.Foo')`
        match (handle.java_to_string().as_str(), args.len()) {
            ("org.apache.commons.jexl3.Foo", 0) => {
                Some(HostMethod::new("org.apache.commons.jexl3.Foo", |_, _| Ok(Foo::new())))
            }
            _ => None,
        }
    }
}

// ====================================================================================== JexlTest

// port of: JexlTest.testProperty
#[test]
fn test_property() {
    let jexl = jexl();
    let e = jexl.create_expression(None, "foo.bar").expect("createExpression");
    let jc = ctx();
    jc.set("foo", Foo::new()).expect("set");
    let o = ok(e.evaluate(jc));
    assert!(matches!(o, Value::String(_)), "o not instanceof String");
    eq(&s(GET_METHOD_STRING), &o);
}

// port of: JexlTest.testBoolean
#[test]
fn test_boolean() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("foo", Foo::new()).expect("set");
    jc.set("a", b(true)).expect("set");
    jc.set("b", b(false)).expect("set");

    assert_expression(&jexl, jc.clone(), "foo.convertBoolean(a==b)", &s("Boolean : false"));
    assert_expression(&jexl, jc.clone(), "foo.convertBoolean(a==true)", &s("Boolean : true"));
    assert_expression(&jexl, jc.clone(), "foo.convertBoolean(a==false)", &s("Boolean : false"));
    assert_expression(&jexl, jc.clone(), "foo.convertBoolean(true==false)", &s("Boolean : false"));
    assert_expression(&jexl, jc.clone(), "true eq false", &b(false));
    assert_expression(&jexl, jc, "true ne false", &b(true));
}

// port of: JexlTest.testStringLit
#[test]
fn test_string_lit() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("foo", Foo::new()).expect("set");
    assert_expression(&jexl, jc, "foo.repeat(\"woogie\")", &s("Repeat : woogie"));
}

// port of: JexlTest.testExpression
// skipped (four assertions of it): the `now` / `now2` comparisons, which need java.util.Date.
#[test]
fn test_expression() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("foo", Foo::new()).expect("set");
    jc.set("a", b(true)).expect("set");
    jc.set("b", b(false)).expect("set");
    jc.set("num", i(5)).expect("set");
    jc.set("bdec", common::upstream::big_dec("7")).expect("set");
    jc.set("bint", common::upstream::big_int("7")).expect("set");

    for (src, want) in [
        ("a == b", false),
        ("a==true", true),
        ("a==false", false),
        ("true==false", false),
        ("2 < 3", true),
        ("num < 5", false),
        ("num < num", false),
        ("num < null", false),
        ("num < 2.5", false),
        ("'6' <= '5'", false),
        ("num <= 5", true),
        ("num <= num", true),
        ("num <= null", false),
        ("num <= 2.5", false),
        ("'6' >= '5'", true),
        ("num >= 5", true),
        ("num >= num", true),
        ("num >= null", false),
        ("num >= 2.5", true),
        ("'6' > '5'", true),
        ("num > 4", true),
        ("num > num", false),
        ("num > null", false),
        ("num > 2.5", true),
        ("\"foo\" + \"bar\" == \"foobar\"", true),
        ("bdec > num", true),
        ("bdec >= num", true),
        ("num <= bdec", true),
        ("num < bdec", true),
        ("bint > num", true),
        ("bint == bdec", true),
        ("bint >= num", true),
        ("num <= bint", true),
        ("num < bint", true),
    ] {
        assert_expression(&jexl, jc.clone(), src, &b(want));
    }
}

// port of: JexlTest.testEmpty
#[test]
fn test_empty() {
    let jexl = jexl();
    let jc = Arc::new(JexlEvalContext::new());
    jc.set_option(|o| o.set_strict(false));
    jc.set("string", s("")).expect("set");
    jc.set("array", Value::Array(JArray::new(Component::object(), Vec::new()))).expect("set");
    jc.set("map", map(vec![])).expect("set");
    jc.set("list", list(vec![])).expect("set");
    jc.set("set", common::upstream::hash_set(vec![])).expect("set");
    jc.set("longstring", s("thingthing")).expect("set");

    for (src, want) in [
        ("empty nullthing", true),
        ("empty string", true),
        ("empty array", true),
        ("empty map", true),
        ("empty set", true),
        ("empty list", true),
        ("empty longstring", false),
        ("not empty longstring", true),
    ] {
        assert_expression(&jexl, jc.clone(), src, &b(want));
    }
}

// port of: JexlTest.testSize
#[test]
fn test_size() {
    let jexl = jexl();
    let jc = Arc::new(JexlEvalContext::new());
    jc.set_option(|o| o.set_strict(false));
    jc.set("s", s("five!")).expect("set");
    jc.set("array", Value::Array(JArray::new(Component::object(), vec![Value::Null; 5]))).expect("set");
    jc.set("map", map((1..=5).map(|n| (s(&n.to_string()), i(n))).collect())).expect("set");
    let items: Vec<Value> = (1..=5).map(|n| s(&n.to_string())).collect();
    jc.set("list", list(items.clone())).expect("set");
    jc.set("set", common::upstream::hash_set(items)).expect("set");
    jc.set("bitset", Value::object(BitSet)).expect("set");

    assert_expression(&jexl, jc.clone(), "size(s)", &i(5));
    assert_expression(&jexl, jc.clone(), "size(array)", &i(5));
    assert_expression(&jexl, jc.clone(), "size(list)", &i(5));
    assert_expression(&jexl, jc.clone(), "size(map)", &i(5));
    assert_expression(&jexl, jc.clone(), "size(set)", &i(5));
    assert_expression(&jexl, jc.clone(), "size(bitset)", &i(64));
    assert_expression(&jexl, jc.clone(), "list.size()", &i(5));
    assert_expression(&jexl, jc.clone(), "map.size()", &i(5));
    assert_expression(&jexl, jc.clone(), "set.size()", &i(5));
    assert_expression(&jexl, jc.clone(), "bitset.size()", &i(64));

    assert_expression(&jexl, jc.clone(), "list.get(size(list) - 1)", &s("5"));
    assert_expression(&jexl, jc.clone(), "list[size(list) - 1]", &s("5"));
    assert_expression(&jexl, jc, "list.get(list.size() - 1)", &s("5"));
}

// port of: JexlTest.testSizeAsProperty
#[test]
fn test_size_as_property() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("map", map(vec![(s("size"), s("cheese")), (s("si & ze"), s("cheese"))])).expect("set");
    jc.set("foo", Foo::new()).expect("set");

    assert_expression(&jexl, jc.clone(), "map['size']", &s("cheese"));
    assert_expression(&jexl, jc.clone(), "map['si & ze']", &s("cheese"));
    assert_expression(&jexl, jc.clone(), "map.'si & ze'", &s("cheese"));
    assert_expression(&jexl, jc.clone(), "map.size()", &i(2));
    assert_expression(&jexl, jc.clone(), "size(map)", &i(2));
    assert_expression(&jexl, jc.clone(), "foo.getSize()", &i(22));
    assert_expression(&jexl, jc, "foo.'size'", &i(22));
}

// port of: JexlTest.testNew
#[test]
fn test_new() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("double", class_of("java.lang.Double")).expect("set");
    jc.set("foo", s("org.apache.commons.jexl3.Foo")).expect("set");
    assert_expression(&jexl, jc.clone(), "new(double, 1)", &Value::Double(1.0));
    assert_expression(&jexl, jc.clone(), "new('java.lang.Float', 100)", &Value::Float(100.0));
    assert_expression(&jexl, jc, "new(foo).quux", &s("String : quux"));
}

// port of: JexlTest.testCalculations
#[test]
fn test_calculations() {
    let jexl = jexl();
    let jc = Arc::new(JexlEvalContext::new());
    jc.set_option(|o| {
        o.set_strict(false);
        o.set_strict_arithmetic(false);
    });

    jc.set("stringy", s("thingy")).expect("set");
    assert_expression(&jexl, jc.clone(), "stringy + 2", &s("thingy2"));

    jc.set("imanull", Value::Null).expect("set");
    assert_expression(&jexl, jc.clone(), "imanull + 2", &i(2));
    assert_expression(&jexl, jc.clone(), "imanull + imanull", &i(0));

    jc.set("n", i(0)).expect("set");
    assert_expression(&jexl, jc, "n != null && n != 0", &b(false));
}

// port of: JexlTest.testConditions
#[test]
fn test_conditions() {
    let jexl = jexl();
    let jc = Arc::new(JexlEvalContext::new());
    jc.set("foo", i(2)).expect("set");
    jc.set("aFloat", Value::Float(1.0)).expect("set");
    jc.set("aDouble", Value::Double(2.0)).expect("set");
    jc.set("aChar", Value::Character('A' as u16)).expect("set");
    jc.set("aBool", b(true)).expect("set");
    // `new StringBuilder("abc")`, built through the engine the port models it in
    let buffer = ok(jexl.create_script("new('java.lang.StringBuilder', 'abc')").expect("parse").execute(ctx()));
    jc.set("aBuffer", buffer).expect("set");
    jc.set("aList", list(vec![])).expect("set");
    jc.set("bList", Value::List(JList::new(ListKind::LinkedList, vec![]))).expect("set");

    for (src, want) in [
        ("foo == 2", true),
        ("2 == 3", false),
        ("3 == foo", false),
        ("3 != foo", true),
        ("foo != 2", false),
        ("aFloat eq aDouble", false),
        ("aFloat ne aDouble", true),
        ("aFloat == aDouble", false),
        ("aFloat != aDouble", true),
        ("foo == aChar", false),
        ("foo != aChar", true),
        ("aBool == 'true'", true),
        ("aBool == 'false'", false),
        ("aBool != 'false'", true),
    ] {
        assert_expression(&jexl, jc.clone(), src, &b(want));
    }
    // test null and boolean
    jc.set_option(|o| o.set_strict(false));
    assert_expression(&jexl, jc.clone(), "aBool == notThere", &b(false));
    assert_expression(&jexl, jc.clone(), "aBool != notThere", &b(true));
    // anything and string as a string comparison
    jc.set_option(|o| o.set_strict(true));
    assert_expression(&jexl, jc.clone(), "aBuffer == 'abc'", &b(true));
    assert_expression(&jexl, jc.clone(), "aBuffer != 'abc'", &b(false));
    // arbitrary equals
    assert_expression(&jexl, jc.clone(), "aList == bList", &b(true));
    assert_expression(&jexl, jc, "aList != bList", &b(false));
}

// port of: JexlTest.testNotConditions
#[test]
fn test_not_conditions() {
    let jexl = jexl();
    let jc = ctx();
    let foo = Foo::new();
    jc.set("x", b(true)).expect("set");
    jc.set("foo", foo).expect("set");
    jc.set("bar", s("true")).expect("set");

    for (src, want) in [
        ("!x", false),
        ("x", true),
        ("!bar", false),
        ("!foo.isSimple()", false),
        ("foo.isSimple()", true),
        ("!foo.simple", false),
        ("foo.simple", true),
        ("foo.getCheeseList().size() == 3", true),
        ("foo.cheeseList.size() == 3", true),
    ] {
        assert_expression(&jexl, jc.clone(), src, &b(want));
    }

    jc.set("string", s("")).expect("set");
    for src in ["not empty string", "not(empty string)", "not empty(string)", "! empty string", "!(empty string)", "! empty(string)"] {
        assert_expression(&jexl, jc.clone(), src, &b(false));
    }
}

// port of: JexlTest.testNotConditionsWithDots
#[test]
fn test_not_conditions_with_dots() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("x.a", b(true)).expect("set");
    jc.set("x.b", b(false)).expect("set");

    assert_expression(&jexl, jc.clone(), "x.a", &b(true));
    assert_expression(&jexl, jc.clone(), "!x.a", &b(false));
    assert_expression(&jexl, jc, "!x.b", &b(true));
}

// port of: JexlTest.testComparisons
#[test]
fn test_comparisons() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("foo", s("the quick and lazy fox")).expect("set");

    assert_expression(&jexl, jc.clone(), "foo.indexOf('quick') > 0", &b(true));
    assert_expression(&jexl, jc.clone(), "foo.indexOf('bar') >= 0", &b(false));
    assert_expression(&jexl, jc, "foo.indexOf('bar') < 0", &b(true));
}

// port of: JexlTest.testNull
#[test]
fn test_null() {
    let jexl = jexl();
    let jc = Arc::new(JexlEvalContext::new());
    jc.set_option(|o| o.set_strict(false));
    jc.set("bar", i(2)).expect("set");

    for (src, want) in [
        ("empty foo", true),
        ("bar == null", false),
        ("foo == null", true),
        ("bar != null", true),
        ("foo != null", false),
        ("empty(bar)", false),
        ("empty(foo)", true),
    ] {
        assert_expression(&jexl, jc.clone(), src, &b(want));
    }
}

// port of: JexlTest.testStringQuoting
#[test]
fn test_string_quoting() {
    let jexl = jexl();
    assert_expression(&jexl, ctx(), "'\"Hello\"'", &s("\"Hello\""));
    assert_expression(&jexl, ctx(), "\"I'm testing\"", &s("I'm testing"));
}

// port of: JexlTest.testBlankStrings
#[test]
fn test_blank_strings() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("bar", s("")).expect("set");

    for src in ["bar == ''", "empty bar", "bar.length() == 0", "size(bar) == 0"] {
        assert_expression(&jexl, jc.clone(), src, &b(true));
    }
}

// port of: JexlTest.testLogicExpressions
#[test]
fn test_logic_expressions() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("foo", s("abc")).expect("set");
    jc.set("bar", s("def")).expect("set");

    for (src, want) in [
        ("foo == 'abc' || bar == 'abc'", true),
        ("foo == 'abc' or bar == 'abc'", true),
        ("foo == 'abc' && bar == 'abc'", false),
        ("foo == 'abc' and bar == 'abc'", false),
        ("foo == 'def' || bar == 'abc'", false),
        ("foo == 'def' or bar == 'abc'", false),
        ("foo == 'abc' && bar == 'def'", true),
        ("foo == 'abc' and bar == 'def'", true),
    ] {
        assert_expression(&jexl, jc.clone(), src, &b(want));
    }
}

// port of: JexlTest.testVariableNames
#[test]
fn test_variable_names() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("foo_bar", s("123")).expect("set");
    assert_expression(&jexl, jc, "foo_bar", &s("123"));
}

// port of: JexlTest.testMapDot
#[test]
fn test_map_dot() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("foo", map(vec![(s("bar"), s("123"))])).expect("set");
    assert_expression(&jexl, jc, "foo.bar", &s("123"));
}

// port of: JexlTest.testStringLiterals
#[test]
fn test_string_literals() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("foo", s("bar")).expect("set");
    assert_expression(&jexl, jc.clone(), "foo == \"bar\"", &b(true));
    assert_expression(&jexl, jc, "foo == 'bar'", &b(true));
}

// port of: JexlTest.testIntProperty
#[test]
fn test_int_property() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("foo", Foo::new()).expect("set");

    assert_expression(&jexl, jc.clone(), "foo.count", &i(5));
    assert_expression(&jexl, jc.clone(), "foo.square(2)", &i(4));
    assert_expression(&jexl, jc, "foo.square(-2)", &i(4));
}

// port of: JexlTest.testNegativeIntComparison
#[test]
fn test_negative_int_comparison() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("foo", Foo::new()).expect("set");

    assert_expression(&jexl, jc.clone(), "foo.count != -1", &b(true));
    assert_expression(&jexl, jc.clone(), "foo.count == 5", &b(true));
    assert_expression(&jexl, jc, "foo.count == -1", &b(false));
}

// port of: JexlTest.testCharAtBug
#[test]
fn test_char_at_bug() {
    let jexl = jexl();
    let jc = Arc::new(JexlEvalContext::new());
    jc.set_option(|o| o.set_silent(true));
    jc.set("foo", s("abcdef")).expect("set");

    assert_expression(&jexl, jc.clone(), "foo.substring(2,4)", &s("cd"));
    assert_expression(&jexl, jc.clone(), "foo.charAt(2)", &Value::Character('c' as u16));
    assert_expression(&jexl, jc, "foo.charAt(-2)", &Value::Null);
}

// port of: JexlTest.testEmptyDottedVariableName
#[test]
fn test_empty_dotted_variable_name() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("this.is.a.test", s("")).expect("set");
    assert_expression(&jexl, jc, "empty(this.is.a.test)", &b(true));
}

// port of: JexlTest.testEmptySubListOfMap
#[test]
fn test_empty_sub_list_of_map() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("aMap", map(vec![(s("aList"), list(vec![]))])).expect("set");
    assert_expression(&jexl, jc, "empty( aMap.aList )", &b(true));
}

// port of: JexlTest.testCoercionWithComparisionOperators
#[test]
fn test_coercion_with_comparision_operators() {
    let jexl = jexl();
    for (src, want) in [
        ("'2' > 1", true),
        ("'2' >= 1", true),
        ("'2' >= 2", true),
        ("'2' < 1", false),
        ("'2' <= 1", false),
        ("'2' <= 2", true),
        ("2 > '1'", true),
        ("2 >= '1'", true),
        ("2 >= '2'", true),
        ("2 < '1'", false),
        ("2 <= '1'", false),
        ("2 <= '2'", true),
    ] {
        assert_expression(&jexl, ctx(), src, &b(want));
    }
}

// port of: JexlTest.testBooleanShortCircuitAnd
#[test]
fn test_boolean_short_circuit_and() {
    let jexl = jexl();
    let mut tester = Foo::new();
    let jc = ctx();
    jc.set("first", b(false)).expect("set");
    jc.set("foo", tester.clone()).expect("set");
    let expr = jexl.create_expression(None, "first and foo.trueAndModify").expect("createExpression");
    ok(expr.evaluate(jc.clone()));
    assert!(!Foo::modified(&tester), "Short circuit failure: rhs evaluated when lhs FALSE");
    // handle true for the left arg of 'and'
    tester = Foo::new();
    jc.set("first", b(true)).expect("set");
    jc.set("foo", tester.clone()).expect("set");
    ok(expr.evaluate(jc));
    assert!(Foo::modified(&tester), "Short circuit failure: rhs not evaluated when lhs TRUE");
}

// port of: JexlTest.testBooleanShortCircuitOr
#[test]
fn test_boolean_short_circuit_or() {
    let jexl = jexl();
    let mut tester = Foo::new();
    let jc = ctx();
    jc.set("first", b(false)).expect("set");
    jc.set("foo", tester.clone()).expect("set");
    let expr = jexl.create_expression(None, "first or foo.trueAndModify").expect("createExpression");
    ok(expr.evaluate(jc.clone()));
    assert!(Foo::modified(&tester), "Short circuit failure: rhs not evaluated when lhs FALSE");
    // handle true for the left arg of 'or'
    tester = Foo::new();
    jc.set("first", b(true)).expect("set");
    jc.set("foo", tester.clone()).expect("set");
    ok(expr.evaluate(jc));
    assert!(!Foo::modified(&tester), "Short circuit failure: rhs evaluated when lhs TRUE");
}

// port of: JexlTest.testStringConcatenation
#[test]
fn test_string_concatenation() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("first", s("Hello")).expect("set");
    jc.set("second", s("World")).expect("set");
    assert_expression(&jexl, jc, "first + ' ' + second", &s("Hello World"));
}

// port of: JexlTest.testToString
#[test]
fn test_to_string() {
    let code = "abcd";
    let expr = jexl().create_expression(None, code).expect("createExpression");
    assert_eq!(code, expr.java_to_jstring().to_rust(), "Bad expression value");
}

// port of: JexlTest.testBadParse
#[test]
fn test_bad_parse() {
    let e = jexl().create_expression(None, "empty()");
    assert!(e.is_err(), "Bad expression didn't throw ParseException");
    assert!(e.err().expect("exception").is_jexl());
}

// port of: JexlTest.testComment
#[test]
fn test_comment() {
    assert_expression(&jexl(), ctx(), "## double or nothing\n 1 + 1", &i(2));
}

// port of: JexlTest.testAssignment
// skipped (one statement of it): the direct `new Parser(";").parse(...)` call, which reaches into
// the generated parser rather than the engine.
#[test]
fn test_assignment() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("aString", s("Hello")).expect("set");
    jc.set("foo", Foo::new()).expect("set");

    assert_expression(&jexl, jc.clone(), "hello = 'world'", &s("world"));
    eq(&s("world"), &jc.get("hello").expect("hello variable not changed"));
    assert_expression(&jexl, jc.clone(), "result = 1 + 1", &i(2));
    eq(&i(2), &jc.get("result").expect("result variable not changed"));
}

// port of: JexlTest.testAntPropertiesWithMethods
#[test]
fn test_ant_properties_with_methods() {
    let jexl = jexl();
    let jc = ctx();
    let value = "Stinky Cheese";
    jc.set("maven.bob.food", s(value)).expect("set");
    assert_expression(&jexl, jc.clone(), "maven.bob.food.length()", &i(value.len() as i32));
    assert_expression(&jexl, jc.clone(), "empty(maven.bob.food)", &b(false));
    assert_expression(&jexl, jc.clone(), "size(maven.bob.food)", &i(value.len() as i32));
    assert_expression(&jexl, jc, "maven.bob.food + ' is good'", &s(&format!("{} is good", value)));
}

// port of: JexlTest.testUnicodeSupport
#[test]
fn test_unicode_support() {
    let jexl = jexl();
    assert_expression(&jexl, ctx(), "'x' == '\\u0032?ytkownik'", &b(false));
    assert_expression(&jexl, ctx(), "'c:\\some\\windows\\path'", &s("c:\\some\\windows\\path"));
    assert_expression(&jexl, ctx(), "'foo\\u0020bar'", &s("foo\u{0020}bar"));
    assert_expression(&jexl, ctx(), "'foo\\u0020\\u0020bar'", &s("foo\u{0020}\u{0020}bar"));
    assert_expression(&jexl, ctx(), "'\\u0020foobar\\u0020'", &s("\u{0020}foobar\u{0020}"));
}

// port of: JexlTest.testDuck
#[test]
fn test_duck() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("duck", Duck::new()).expect("set");
    for (src, want) in [
        ("duck.zero", i(0)),
        ("duck.one", i(1)),
        ("duck.user = 20", i(20)),
        ("duck.user", i(20)),
        ("duck.user = 'zero'", s("zero")),
        ("duck.user", i(0)),
    ] {
        assert_expression(&jexl, jc.clone(), src, &want);
    }
}

// port of: JexlTest.testArray
#[test]
fn test_array() {
    let jexl = jexl();
    let jc = ctx();
    jc.set("array", common::upstream::int_array(&[100, 101, 102])).expect("set");
    assert_expression(&jexl, jc.clone(), "array.1", &i(101));
    assert_expression(&jexl, jc.clone(), "array[1] = 1010", &i(1010));
    assert_expression(&jexl, jc, "array.0", &i(100));
}

// ==================================================================================== ScriptTest

/// port of: `src/test/scripts/test1.jexl`, inlined — the port has no `createScript(File)`, and the
/// point of the fixture is its comment shapes (an AL block header, `##` lines, a trailing block).
const TEST1: &str = r#"/*
 * Licensed to the Apache Software Foundation (ASF) under one or more
 * contributor license agreements.  See the NOTICE file distributed with
 * this work for additional information regarding copyright ownership.
 * The ASF licenses this file to You under the Apache License, Version 2.0
 * (the "License"); you may not use this file except in compliance with
 * the License.  You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

// This tests for JEXL-47. AL header above tests for block comments.

##
## This is a test script
##
if (out != null) out.println('Starting test script');
x = 1;
y = 2;
result = x * y + 5;
if (out != null) out.println("The result is " + result);
## return the result.
result; // JEXL-44 should ignore "quotes" here

/*
   Trailing comments are also ignored
*/
"#;

/// port of: `src/test/scripts/testAdd.jexl`, inlined for the same reason.
const TEST_ADD: &str = r#"/*
 * Licensed to the Apache Software Foundation (ASF) under one or more
 * contributor license agreements.  See the NOTICE file distributed with
 * this work for additional information regarding copyright ownership.
 * The ASF licenses this file to You under the Apache License, Version 2.0
 * (the "License"); you may not use this file except in compliance with
 * the License.  You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
//(x, y)->{ x + y }
x + y
"#;

// port of: ScriptTest.testSpacesScript
#[test]
fn test_spaces_script() {
    assert!(jexl().create_script(" ").is_ok());
}

// port of: ScriptTest.testSimpleScript
#[test]
fn test_simple_script() {
    let code = "while (x < 10) x = x + 1;";
    let script = jexl().create_script(code).expect("createScript");
    let jc = ctx();
    jc.set("x", i(1)).expect("set");
    let o = ok(script.execute(jc));
    eq(&i(10), &o);
    assert_eq!(Some(code), script.get_source_text(), "getText is wrong");
}

// port of: ScriptTest.testScriptFromFile
#[test]
fn test_script_from_file() {
    let script = jexl().create_script(TEST1).expect("createScript");
    let jc = ctx();
    jc.set("out", Value::Null).expect("set");
    eq(&i(7), &ok(script.execute(jc)));
}

// port of: ScriptTest.testArgScriptFromFile
#[test]
fn test_arg_script_from_file() {
    let script = jexl()
        .create_script_named(TEST_ADD, &["x".to_string(), "y".to_string()])
        .expect("createScript");
    let jc = ctx();
    jc.set("out", Value::Null).expect("set");
    eq(&i(42), &ok(script.execute_args(jc, &[i(13), i(29)])));
}

// skipped: ScriptTest.testScriptFromURL — `JexlEngine.createScript(URL)` has no port, and the
// script it loads is the one testScriptFromFile already runs.
// skipped: ScriptTest.testArgScriptFromURL — same, for testArgScriptFromFile's script.

// port of: ScriptTest.testScriptUpdatesContext
#[test]
fn test_script_updates_context() {
    let jexl = jexl();
    let jexl_code = "resultat.setCode('OK')";
    let e = jexl.create_expression(None, jexl_code).expect("createExpression");
    let script = jexl.create_script(jexl_code).expect("createScript");

    let resultat_jexl = Tester::new();
    let jc = ctx();
    jc.set("resultat", resultat_jexl.clone()).expect("set");

    Tester::set_code(&resultat_jexl, s(""));
    ok(e.evaluate(jc.clone()));
    eq(&s("OK"), &Tester::code(&resultat_jexl));
    Tester::set_code(&resultat_jexl, s(""));
    ok(script.execute(jc));
    eq(&s("OK"), &Tester::code(&resultat_jexl));
}

// ============================================================================= ParseFailuresTest

// port of: ParseFailuresTest.testMalformedExpression1
#[test]
fn test_malformed_expression1() {
    let bad = "eq";
    let x = jexl().create_expression(None, bad).err().unwrap_or_else(|| panic!("Parsing \"{}\" should result in a JexlException", bad));
    assert!(x.is_jexl());
}

// port of: ParseFailuresTest.testMalformedExpression2
#[test]
fn test_malformed_expression2() {
    let bad = "?";
    let x = jexl().create_expression(None, bad).err().unwrap_or_else(|| panic!("Parsing \"{}\" should result in a JexlException", bad));
    assert!(x.is_jexl());
}

// port of: ParseFailuresTest.testMalformedScript1
#[test]
fn test_malformed_script1() {
    let bad = "eq";
    let x = jexl().create_script(bad).err().unwrap_or_else(|| panic!("Parsing \"{}\" should result in a JexlException", bad));
    assert!(x.is_jexl());
}

// port of: ParseFailuresTest.testMalformedScript2
#[test]
fn test_malformed_script2() {
    let bad = "?";
    let x = jexl().create_script(bad).err().unwrap_or_else(|| panic!("Parsing \"{}\" should result in a JexlException", bad));
    assert!(x.is_jexl());
}

// port of: ParseFailuresTest.testMalformedScript3
#[test]
fn test_malformed_script3() {
    let bad = "foo=1;bar=2;a?b:c:d;";
    let x = jexl().create_script(bad).err().unwrap_or_else(|| panic!("Parsing \"{}\" should result in a JexlException", bad));
    assert!(x.is_jexl());
}

// ================================================================================== FeaturesTest

/// port of: `FeaturesTest.jexl` — `new JexlBuilder().create()`.
fn features_engine() -> Arc<JexlEngine> {
    builder().create()
}

/// port of: FeaturesTest.checkFeature — valid with every feature on, a parse failure without.
#[track_caller]
fn check_feature(features: &JexlFeatures, scripts: &[&str]) {
    let control = jexl();
    let jexl = features_engine();
    for script in scripts {
        control.create_script(script).unwrap_or_else(|e| panic!("{}: {}", script, e.message()));
        match jexl.create_script_features(features.clone(), None, script, None) {
            Ok(_) => panic!("should fail parse: {}", script),
            Err(x) => {
                assert!(
                    matches!(x.kind(), ExceptionKind::Parsing | ExceptionKind::Feature { .. }),
                    "{}: not a parse failure: {}",
                    script,
                    x.class_name()
                );
                assert!(x.get_message().is_some(), "{}: no message", script);
            }
        }
    }
}

/// port of: FeaturesTest.assertOk — which, as upstream wrote it, parses with the engine's own
/// features and not with the restricted set it is handed.
#[track_caller]
fn assert_ok(_features: &JexlFeatures, scripts: &[&str]) {
    let jexl = features_engine();
    for script in scripts {
        jexl.create_script(script).unwrap_or_else(|e| panic!("{} :: should not fail parse: {}", script, e.message()));
    }
}

// port of: FeaturesTest.testNoScript
#[test]
fn test_no_script() {
    let f = JexlFeatures::new().script(false);
    check_feature(
        &f,
        &["if (false) { block(); }", "{ noway(); }", "while(true);", "for(var i : {0 .. 10}) { bar(i); }"],
    );
}

// port of: FeaturesTest.testNoLoop
#[test]
fn test_no_loop() {
    let f = JexlFeatures::new().loops(false);
    check_feature(&f, &["while(true);", "for(var i : {0 .. 10}) { bar(i); }"]);
}

// port of: FeaturesTest.testNoLambda
#[test]
fn test_no_lambda() {
    let f = JexlFeatures::new().lambda(false);
    check_feature(
        &f,
        &[
            "var x  = ()->{ return 0 };",
            "()->{ return 0 };",
            "(x, y)->{ return 0 };",
            "function() { return 0 };",
            "function(x, y) { return 0 };",
            "if (false) { (function(x, y) { return x + y })(3, 4) }",
        ],
    );
}

// port of: FeaturesTest.testNoNew
#[test]
fn test_no_new() {
    let f = JexlFeatures::new().new_instance(false);
    check_feature(&f, &["return new(clazz);", "new('java.math.BigDecimal', 12) + 1"]);
}

/// port of: the script array `testNoSideEffects` and `testNoSideEffectsGlobal` share.
const SIDE_EFFECTS: [&str; 12] = [
    "x = 1",
    "x.y = 1",
    "x().y = 1",
    "x += 1",
    "x.y += 1",
    "x().y += 1",
    "x -= 1",
    "x *= 1",
    "x /= 1",
    "x ^= 1",
    "x &= 1",
    "x |= 1",
];

// port of: FeaturesTest.testNoSideEffects
#[test]
fn test_no_side_effects() {
    let f = JexlFeatures::new().side_effect(false);
    check_feature(&f, &SIDE_EFFECTS);
}

// port of: FeaturesTest.testNoSideEffectsGlobal
#[test]
fn test_no_side_effects_global() {
    let f = JexlFeatures::new().side_effect_global(false);
    let mut scripts = SIDE_EFFECTS.to_vec();
    scripts.push("4 + (x.y = 1)");
    scripts.push("if (true) x.y.z = 4");
    // these should all fail with x undeclared as local, thus x as global
    check_feature(&f, &scripts);
    // same ones with x as local should work
    let jexl = features_engine();
    for script in &scripts {
        let src = format!("var x = foo(); {}", script);
        jexl.create_script(&src).unwrap_or_else(|e| panic!("{} :: should not fail parse: {}", script, e.message()));
    }
}

// port of: FeaturesTest.testNoLocals
#[test]
fn test_no_locals() {
    let f = JexlFeatures::new().local_var(false);
    check_feature(&f, &["var x = 0;", "(x)->{ x }"]);
}

// port of: FeaturesTest.testReservedVars
#[test]
fn test_reserved_vars() {
    let f = JexlFeatures::new().reserved_names(["foo", "bar"]);
    check_feature(&f, &["var foo = 0;", "(bar)->{ bar }", "var f = function(bar) { bar; }"]);
    assert_ok(&f, &["var foo0 = 0;", "(bar1)->{ bar }", "var f = function(bar2) { bar2; }"]);
}

// port of: FeaturesTest.testArrayRefs
#[test]
fn test_array_refs() {
    let f = JexlFeatures::new().array_reference_expr(false);
    let scripts = ["x[y]", "x['a'][b]", "x()['a'][b]", "x.y['a'][b]"];
    check_feature(&f, &scripts);
    assert_ok(&f, &scripts);
    // same ones with constant array refs should work
    assert_ok(&f, &["x['y']", "x['a'][1]", "x()['a']['b']", "x.y['a']['b']"]);
}

// port of: FeaturesTest.testMethodCalls
#[test]
fn test_method_calls() {
    let f = JexlFeatures::new().method_call(false);
    check_feature(&f, &["x.y(z)", "x['a'].m(b)", "x()['a'](b)", "x.y['a'](b)"]);
    assert_ok(&f, &["x('y')", "x('a')[1]", "x()['a']['b']"]);
}

// port of: FeaturesTest.testStructuredLiterals
#[test]
fn test_structured_literals() {
    let f = JexlFeatures::new().structured_literal(false);
    let scripts = ["{1, 2, 3}", "[1, 2, 3]", "{ 1 :'one', 2 : 'two', 3 : 'three' }", "(1 .. 5)"];
    check_feature(&f, &scripts);
    assert_ok(&f, &scripts);
}

// port of: FeaturesTest.testAnnotations
#[test]
fn test_annotations() {
    let f = JexlFeatures::new().annotation(false);
    check_feature(&f, &["@synchronized(2) { return 42; }", "@two var x = 3;"]);
}

// port of: FeaturesTest.testPragma
#[test]
fn test_pragma() {
    let f = JexlFeatures::new().pragma(false);
    check_feature(&f, &["#pragma foo 42", "#pragma foo 'bar'\n@two var x = 3;"]);
}

// port of: FeaturesTest.testMixedFeatures
#[test]
fn test_mixed_features() {
    // no new, no local, no lambda, no loops, no-side effects
    let f = JexlFeatures::new()
        .new_instance(false)
        .local_var(false)
        .lambda(false)
        .loops(false)
        .side_effect_global(false);
    check_feature(
        &f,
        &[
            "return new(clazz);",
            "()->{ return 0 };",
            "var x = 0;",
            "(x, y)->{ return 0 };",
            "for(var i : {0 .. 10}) { bar(i); }",
            "x += 1",
            "x.y += 1",
        ],
    );
}

// =================================================================================== BuilderTest

// port of: BuilderTest.testFlags
// skipped (half of it): `JexlBuilder` has no getters in the port, so each flag is read back off the
// engine it builds; `antish`, `safe`, `lexical` and `lexicalShade` reach no engine getter at all.
#[test]
fn test_flags() {
    assert!(JexlBuilder::new().cancellable(true).create().is_cancellable());
    assert!(!JexlBuilder::new().cancellable(false).create().is_cancellable());
    assert!(JexlBuilder::new().silent(true).create().is_silent());
    assert!(!JexlBuilder::new().silent(false).create().is_silent());
    assert!(JexlBuilder::new().strict(true).create().is_strict());
    assert!(!JexlBuilder::new().strict(false).create().is_strict());
}

// skipped: BuilderTest.testValues — `collectMode()`, `cacheThreshold()` and `stackOverflow()` are
// write-only in the port; nothing reads them back.
// skipped: BuilderTest.testOther — ClassLoader, Charset and reference equality of a
// JexlUberspect / JexlSandbox, none of which the port models.

// ================================================================================== StrategyTest

/// port of: `StrategyTest.MapArithmetic` — an arithmetic that routes every map property access
/// through `arrayGet` / `arraySet`.
///
/// Java overrides `propertyGet(Map, Object)` on a `JexlArithmetic` subclass; the port registers the
/// same four operators through the `JexlArithmetic.Uberspect` hook `Operators.tryOverload` uses.
struct MapArithmetic;

impl common::upstream::ArithmeticOverloads for MapArithmetic {
    fn overloads(&self, operator: rust_jexl::jexl_operator::JexlOperator) -> bool {
        use rust_jexl::jexl_operator::JexlOperator::*;
        matches!(operator, PropertyGet | PropertySet | ArrayGet | ArraySet)
    }

    fn get_operator(
        &self,
        operator: rust_jexl::jexl_operator::JexlOperator,
        args: &[Value],
    ) -> Option<Arc<dyn JexlMethod>> {
        use rust_jexl::jexl_operator::JexlOperator::*;
        if !matches!(args.first(), Some(Value::Map(_))) {
            return None;
        }
        match operator {
            PropertyGet | ArrayGet if args.len() == 2 => Some(HostMethod::new("java.lang.Object", |_, a| {
                let m = match &a[0] {
                    Value::Map(m) => m,
                    _ => return Ok(Value::Null),
                };
                Ok(m.get(&a[1]).unwrap_or(Value::Null))
            })),
            PropertySet | ArraySet if args.len() == 3 => Some(HostMethod::new("java.lang.Object", |_, a| {
                if let Value::Map(m) = &a[0] {
                    m.put(a[1].clone(), a[2].clone());
                }
                Ok(a[2].clone())
            })),
            _ => None,
        }
    }
}

/// port of: StrategyTest.run171.
#[track_caller]
fn run171(jexl: &Arc<JexlEngine>, std: bool) {
    let m = JMap::hash_map();
    let run = |src: &str| {
        let script = jexl.create_script_named(src, &["i".to_string()]).expect("createScript");
        ok(script.execute_args(empty_context(), &[Value::Map(m.clone())]))
    };

    m.put(s("class"), i(42));
    eq(&i(42), &run("i['class'] "));
    eq(&i(28), &run("i['class'] = 28"));
    eq(&i(28), &m.get(&s("class")).expect("class"));
    if std {
        eq(&class_of("java.util.HashMap"), &run("i.class"));
        eq(&class_of("java.util.HashMap"), &run("i.'class'"));
    } else {
        eq(&i(28), &run("i.class"));
        eq(&i(28), &run("i.'class'"));
    }

    m.put(s("size"), i(4242));
    eq(&i(4242), &run("i['size'] "));
    eq(&i(2828), &run("i['size'] = 2828"));
    eq(&i(2828), &m.get(&s("size")).expect("size"));
    eq(&i(2828), &run("i.'size'"));
    eq(&i(2), &run("size i"));

    m.put(s("empty"), i(424242));
    eq(&i(424242), &run("i['empty'] "));
    eq(&i(282828), &run("i['empty'] = 282828"));
    eq(&i(282828), &m.get(&s("empty")).expect("empty"));
    let value = run("i.'empty'");
    if std {
        // measured: the JVM resolves `empty` through `HashMap.isEmpty()` here, and answers false
        assert!(!i(282828).java_equals(&value), "i.'empty' should not be 282828, got {:?}", value.java_to_string());
    } else {
        eq(&i(282828), &value);
    }
    eq(&b(false), &run("empty i"));
}

// port of: StrategyTest.testRawResolvers
#[test]
fn test_raw_resolvers() {
    let map = map(vec![]);
    let uberspect = beans();
    let key = s("key");
    let value = s("value");
    assert!(uberspect.get_property_get_with(&[PropertyResolver::Field], &map, &key).is_none());
    assert!(uberspect.get_property_set_with(&[PropertyResolver::Field], &map, &key, &value).is_none());
    assert!(uberspect.get_property_get_with(&[PropertyResolver::Map], &map, &key).is_some());
    assert!(uberspect.get_property_set_with(&[PropertyResolver::Map], &map, &key, &value).is_some());
}

// port of: StrategyTest.testJexlStrategy
#[test]
fn test_jexl_strategy() {
    run171(&builder().create(), true);
}

// port of: StrategyTest.testMyMapStrategy
#[test]
fn test_my_map_strategy() {
    let uberspect = Arc::new(common::upstream::OverloadUberspect::new(Arc::new(MapArithmetic)));
    run171(&JexlBuilder::new().uberspect(uberspect).safe(false).lexical(true).create(), false);
}

// port of: StrategyTest.testMapStrategy
#[test]
fn test_map_strategy() {
    let shim = JdkShim::new(ResolverStrategy::Map).with_hosts(Arc::new(Beans));
    let uberspect: Arc<dyn JexlUberspect> =
        Arc::new(Uberspect::with_strategy(ResolverStrategy::Map).with_shim(Arc::new(shim)));
    run171(&JexlBuilder::new().uberspect(uberspect).safe(false).lexical(true).create(), false);
}

// ================================================================================= ExceptionTest

/// port of: `org.apache.commons.jexl3.ObjectContext` — a context backed by one bean, which is also
/// the namespace every prefix-less function call resolves to.
struct ObjectContext {
    jexl: Arc<JexlEngine>,
    object: Value,
}

impl JexlContext for ObjectContext {
    fn get(&self, name: &str) -> Option<Value> {
        get_property(&self.jexl, &self.object, name).ok()
    }
    fn set(&self, name: &str, value: Value) -> Result<(), String> {
        set_property(&self.jexl, &self.object, name, &value).map_err(|e| e.message())
    }
    fn has(&self, name: &str) -> bool {
        get_property(&self.jexl, &self.object, name).is_ok()
    }
    // port of: ObjectContext.resolveNamespace — `name == null || name.isEmpty() ? object : null`
    fn resolve_namespace(&self, name: Option<&str>) -> Option<Value> {
        match name {
            None => Some(self.object.clone()),
            Some(n) if n.is_empty() => Some(self.object.clone()),
            Some(_) => None,
        }
    }
    fn is_namespace_resolver(&self) -> bool {
        true
    }
}

/// port of: `Engine.getProperty(Object, String)`, which parses `#0.<expr>` and interprets it, so
/// that a throwing accessor surfaces as a `JexlException.Property` carrying the cause.
fn get_property(jexl: &Arc<JexlEngine>, bean: &Value, expr: &str) -> Result<Value, JexlException> {
    let script = jexl.create_script_named(&format!("p0.{}", expr), &["p0".to_string()])?;
    script.execute_args(empty_context(), &[bean.clone()])
}

/// port of: `Engine.setProperty(Object, String, Object)`.
fn set_property(jexl: &Arc<JexlEngine>, bean: &Value, expr: &str, value: &Value) -> Result<(), JexlException> {
    let script =
        jexl.create_script_named(&format!("p0.{} = p1", expr), &["p0".to_string(), "p1".to_string()])?;
    script.execute_args(empty_context(), &[bean.clone(), value.clone()]).map(|_| ())
}

/// port of: `Engine.invokeMethod(Object, String, Object...)`.
fn invoke_method(jexl: &Arc<JexlEngine>, obj: &Value, meth: &str, args: &[Value]) -> Result<Value, JexlException> {
    let uberspect = beans();
    let mut argv = args.to_vec();
    let mut method = uberspect.get_method(obj, meth, &argv);
    if method.is_none() && jexl.get_arithmetic().narrow_arguments(&mut argv) {
        method = uberspect.get_method(obj, meth, &argv);
    }
    match method {
        None => Err(JexlException::method_info(None, meth, Some(&argv), None)),
        Some(m) => m.invoke(obj, &argv).map_err(|x| {
            if x.is_jexl() {
                x
            } else {
                JexlException::method_info(None, meth, Some(&argv), Some(x))
            }
        }),
    }
}

// port of: ExceptionTest.testWrappedEx
#[test]
fn test_wrapped_ex() {
    let jexl = builder().create();
    let e = jexl.create_expression(None, "npe()").expect("createExpression");
    let jc = Arc::new(ObjectContext { jexl: jexl.clone(), object: ThrowNPE::new() });
    let xany = thrown(e.evaluate(jc));
    let xth = xany.get_cause().expect("Should have thrown NPE");
    assert_eq!("java.lang.NullPointerException", xth.class_name());
}

// port of: ExceptionTest.testWrappedExmore
#[test]
fn test_wrapped_exmore() {
    let jexl = builder().create();
    let npe_bean = ThrowNPE::new();

    let xany = thrown(get_property(&jexl, &npe_bean, "foo"));
    assert!(matches!(xany.kind(), ExceptionKind::Property { .. }), "{}", xany.class_name());
    assert!(xany.get_cause().is_none());

    let xany = thrown(set_property(&jexl, &npe_bean, "foo", &i(42)).map(|_| Value::Null));
    assert!(matches!(xany.kind(), ExceptionKind::Property { .. }), "{}", xany.class_name());
    assert!(xany.get_cause().is_none());

    eq(&b(false), &ok(get_property(&jexl, &npe_bean, "fail")));

    set_property(&jexl, &npe_bean, "fail", &b(false)).expect("setFail(false)");
    let xany = thrown(set_property(&jexl, &npe_bean, "fail", &b(true)).map(|_| Value::Null));
    assert!(matches!(xany.kind(), ExceptionKind::Property { .. }), "{}", xany.class_name());
    assert_eq!("java.lang.NullPointerException", xany.get_cause().expect("cause").class_name());

    let xany = thrown(get_property(&jexl, &npe_bean, "fail"));
    assert!(matches!(xany.kind(), ExceptionKind::Property { .. }), "{}", xany.class_name());
    assert_eq!("java.lang.NullPointerException", xany.get_cause().expect("cause").class_name());

    let xany = thrown(invoke_method(&jexl, &npe_bean, "foo", &[i(42)]));
    assert!(matches!(xany.kind(), ExceptionKind::Method), "{}", xany.class_name());
    assert!(xany.get_cause().is_none());

    let xany = thrown(invoke_method(&jexl, &npe_bean, "npe", &[]));
    assert!(matches!(xany.kind(), ExceptionKind::Method), "{}", xany.class_name());
    assert_eq!("java.lang.NullPointerException", xany.get_cause().expect("cause").class_name());
}

// port of: ExceptionTest.testEx — unknown vars and properties versus null operands (JEXL-73)
#[test]
fn test_ex() {
    let jexl = create_engine(false);
    let e = jexl.create_expression(None, "c.e * 6").expect("createExpression");
    let ctxt = Arc::new(JexlEvalContext::new());
    // ensure errors will throw, make unknown vars throw
    ctxt.set_option(|o| {
        o.set_silent(false);
        o.set_strict(true);
    });
    // empty context
    let xjexl = thrown(e.evaluate(ctxt.clone()));
    assert!(matches!(xjexl.kind(), ExceptionKind::Variable { .. }), "{}", xjexl.class_name());
    let msg = xjexl.get_message().expect("message").to_rust();
    assert!(msg.contains("variable 'c.e'"), "{}", msg);

    // disallow null operands
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    ctxt.set("c.e", Value::Null).expect("set");
    let xjexl = thrown(e.evaluate(ctxt.clone()));
    assert!(matches!(xjexl.kind(), ExceptionKind::Variable { .. }), "{}", xjexl.class_name());
    let msg = xjexl.get_message().expect("message").to_rust();
    assert!(msg.contains("variable 'c.e'"), "{}", msg);

    // allow null operands
    ctxt.set_option(|o| o.set_strict_arithmetic(false));
    ok(e.evaluate(ctxt.clone()));

    // ensure c.e is not a defined property
    ctxt.set("c", s("{ 'a' : 3, 'b' : 5}")).expect("set");
    ctxt.set("e", i(2)).expect("set");
    let xjexl = thrown(e.evaluate(ctxt));
    assert!(matches!(xjexl.kind(), ExceptionKind::Property { .. }), "{}", xjexl.class_name());
    let msg = xjexl.get_message().expect("message").to_rust();
    assert!(msg.contains("property 'e"), "{}", msg);
}

// port of: ExceptionTest.testExVar — null local vars and strict arithmetic effects
#[test]
fn test_ex_var() {
    let jexl = create_engine(false);
    let e = jexl.create_script("(x)->{ x * 6 }").expect("createScript");
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| {
        o.set_silent(false);
        o.set_strict(true);
        o.set_strict_arithmetic(true);
    });
    // empty context
    let xjexl = thrown(e.execute(ctxt.clone()));
    let msg = xjexl.get_message().expect("message").to_rust();
    assert!(msg.contains("null"), "x is null, should throw: {}", msg);

    // allow null operands
    ctxt.set_option(|o| o.set_strict_arithmetic(false));
    ok(e.execute_args(ctxt, &[Value::Null]));
}

// port of: ExceptionTest.testExMethod
#[test]
fn test_ex_method() {
    let jexl = create_engine(false);
    let e = jexl.create_expression(None, "c.e.foo()").expect("createExpression");
    let ctxt = Arc::new(JexlEvalContext::new());
    ctxt.set_option(|o| {
        o.set_silent(false);
        o.set_strict(true);
    });
    let xjexl = thrown(e.evaluate(ctxt.clone()));
    assert!(matches!(xjexl.kind(), ExceptionKind::Variable { .. }), "{}", xjexl.class_name());
    let msg = xjexl.get_message().expect("message").to_rust();
    assert!(msg.contains("variable 'c.e'"), "{}", msg);

    // disallow null operands
    ctxt.set_option(|o| o.set_strict_arithmetic(true));
    ctxt.set("c.e", Value::Null).expect("set");
    let xjexl = thrown(e.evaluate(ctxt));
    let msg = xjexl.get_message().expect("message").to_rust();
    assert!(msg.contains("variable 'c.e'"), "{}", msg);
}

// port of: ExceptionTest.test206
// skipped (the log half of it): `doTest206` counts `warn` / `debug` calls on a `CaptureLog`, and
// the engine takes no logger. What is left is the throw / no-throw contract and the returned 42.
#[test]
fn test206() {
    for src in ["null.1 = 2; return 42", "x = null.1; return 42", "x = y.1; return 42"] {
        for strict in [false, true] {
            for silent in [false, true] {
                do_test206(src, strict, silent);
            }
        }
    }
}

#[track_caller]
fn do_test206(src: &str, strict: bool, silent: bool) {
    let jc = ctx();
    let jexl = builder().strict(strict).silent(silent).create();
    let e = jexl.create_script(src).expect("createScript");
    match e.execute(jc) {
        Ok(r) => {
            assert!(!(strict && !silent), "{}: should have thrown an exception", src);
            if !strict {
                eq(&i(42), &r);
            }
        }
        Err(_) => assert!(strict && !silent, "{}: should not have thrown an exception", src),
    }
}

// ===================================================================================== CacheTest

/// port of: CacheTest.LOOPS
const LOOPS: usize = 4096;
/// port of: CacheTest.NTHREADS
const NTHREADS: usize = 4;
/// port of: CacheTest.MIX — a pseudo random mix of accessors
const MIX: [usize; 42] = [
    0, 0, 3, 3, 4, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 2,
    2, 3, 3, 0,
];

/// port of: CacheTest.jexlCache / CacheTest.jexlNoCache.
fn cache_engine(cache: bool) -> Arc<JexlEngine> {
    builder().cache(if cache { 1024 } else { 0 }).debug(true).strict(true).create()
}

/// port of: CacheTest.runThreaded — the same task in NTHREADS in parallel, each returning `loops`.
#[track_caller]
fn run_threaded(task: fn(&Arc<JexlEngine>, usize, usize) -> usize, loops: usize, cache: bool) {
    let jexl = cache_engine(cache);
    let handles: Vec<_> = (0..NTHREADS)
        .map(|t| {
            let jexl = jexl.clone();
            std::thread::spawn(move || task(&jexl, loops, t))
        })
        .collect();
    for h in handles {
        assert_eq!(loops, h.join().expect("task"));
    }
}

/// port of: CacheTest.TestCacheArguments — one fresh instance of each bean per task.
fn cache_beans() -> Vec<Value> {
    (0..5).map(Cached::new).collect()
}

/// port of: CacheTest.Task.runAssign — assigns and reads `cache.value` against five beans.
fn run_assign(jexl: &Arc<JexlEngine>, loops: usize, px: usize, value: Value) -> usize {
    let ca = cache_beans();
    let jc = ctx();
    let cache_get_value = jexl.create_expression(None, "cache.value").expect("createExpression");
    let cache_set_value = jexl.create_expression(None, "cache.value = value").expect("createExpression");
    for l in 0..loops {
        let mix = MIX[(l + px) % MIX.len()];
        jc.set("cache", ca[mix].clone()).expect("set");
        jc.set("value", value.clone()).expect("set");
        let result = ok(cache_set_value.evaluate(jc.clone()));
        if value.is_null() {
            assert!(result.is_null(), "cache.value = value");
        } else {
            eq(&value, &result);
        }
        let result = ok(cache_get_value.evaluate(jc.clone()));
        let want = if value.is_null() {
            format!("Cached{}:na", mix)
        } else {
            format!("Cached{}:{}", mix, value.java_to_string())
        };
        eq(&s(&want), &result);
    }
    loops
}

// port of: CacheTest.testNullAssignNoCache
#[test]
fn test_null_assign_no_cache() {
    run_threaded(|j, n, px| run_assign(j, n, px, Value::Null), LOOPS, false);
}

// port of: CacheTest.testNullAssignCache
#[test]
fn test_null_assign_cache() {
    run_threaded(|j, n, px| run_assign(j, n, px, Value::Null), LOOPS, true);
}

// port of: CacheTest.testAssignNoCache
#[test]
fn test_assign_no_cache() {
    run_threaded(|j, n, px| run_assign(j, n, px, s("foo")), LOOPS, false);
}

// port of: CacheTest.testAssignCache
#[test]
fn test_assign_cache() {
    run_threaded(|j, n, px| run_assign(j, n, px, s("foo")), LOOPS, true);
}

/// port of: CacheTest.AssignBooleanTask.runAssignBoolean.
fn run_assign_boolean(jexl: &Arc<JexlEngine>, loops: usize, px: usize) -> usize {
    let ca = cache_beans();
    let jc = ctx();
    let value = b(true);
    let cache_get_value = jexl.create_expression(None, "cache.flag").expect("createExpression");
    let cache_set_value = jexl.create_expression(None, "cache.flag = value").expect("createExpression");
    for l in 0..loops {
        let mix = MIX[(l + px) % MIX.len()];
        jc.set("cache", ca[mix].clone()).expect("set");
        jc.set("value", value.clone()).expect("set");
        eq(&value, &ok(cache_set_value.evaluate(jc.clone())));
        eq(&value, &ok(cache_get_value.evaluate(jc.clone())));
    }
    loops
}

// port of: CacheTest.testAssignBooleanNoCache
#[test]
fn test_assign_boolean_no_cache() {
    run_threaded(run_assign_boolean, LOOPS, false);
}

// port of: CacheTest.testAssignBooleanCache
#[test]
fn test_assign_boolean_cache() {
    run_threaded(run_assign_boolean, LOOPS, true);
}

/// port of: CacheTest.AssignListTask.runAssignList — a `String[]` and an `ArrayList`.
fn run_assign_list(jexl: &Arc<JexlEngine>, loops: usize, px: usize) -> usize {
    let value = s("foo");
    let ca = [string_array(&["one", "two"]), list(vec![s("foo"), s("bar")])];
    let jc = ctx();
    let cache_get_value = jexl.create_expression(None, "cache.0").expect("createExpression");
    let cache_set_value = jexl.create_expression(None, "cache[0] = value").expect("createExpression");
    for l in 0..loops {
        let mix = MIX[(l + px) % MIX.len()] % ca.len();
        jc.set("cache", ca[mix].clone()).expect("set");
        jc.set("value", value.clone()).expect("set");
        eq(&value, &ok(cache_set_value.evaluate(jc.clone())));
        eq(&value, &ok(cache_get_value.evaluate(jc.clone())));
    }
    loops
}

// port of: CacheTest.testAssignListNoCache
#[test]
fn test_assign_list_no_cache() {
    run_threaded(run_assign_list, LOOPS, false);
}

// port of: CacheTest.testAssignListCache
#[test]
fn test_assign_list_cache() {
    run_threaded(run_assign_list, LOOPS, true);
}

/// port of: CacheTest.ComputeTask.call — overload selection under concurrency.
///
/// skipped (one assertion of it): `sany.startsWith(tname)` on the ambiguous-call message asserts
/// that debug mode carries the *Java caller's* class name into `JexlInfo`, and the port has no
/// stack to read it from.
fn compute_task(jexl: &Arc<JexlEngine>, loops: usize, _px: usize) -> usize {
    let beans = cache_beans();
    let ca = &beans[0..3];
    let values = [i(2), s("quux")];
    let jc = ctx();
    let compute2 = jexl.create_expression(None, "cache.compute(a0, a1)").expect("createExpression");
    let compute1 = jexl.create_expression(None, "cache.compute(a0)").expect("createExpression");
    let compute1null = jexl.create_expression(None, "cache.compute(a0)").expect("createExpression");
    let ambiguous = jexl.create_expression(None, "cache.ambiguous(a0, a1)").expect("createExpression");

    for l in 0..loops {
        let mix = MIX[l % MIX.len()] % ca.len();
        let value = &values[l % values.len()];

        jc.set("cache", ca[mix].clone()).expect("set");
        let expected = match value {
            Value::String(_) => {
                jc.set("a0", s("S0")).expect("set");
                jc.set("a1", s("S1")).expect("set");
                format!("Cached{}@s#S0,s#S1", mix)
            }
            _ => {
                jc.set("a0", i(7)).expect("set");
                jc.set("a1", i(9)).expect("set");
                format!("Cached{}@i#7,i#9", mix)
            }
        };
        eq(&s(&expected), &ok(compute2.evaluate(jc.clone())));

        if !matches!(value, Value::String(_)) {
            jc.set("a0", Value::Short(17)).expect("set");
            jc.set("a1", Value::Short(19)).expect("set");
            thrown(ambiguous.evaluate(jc.clone()));
        }

        let expected = match value {
            Value::String(_) => {
                jc.set("a0", s("X0")).expect("set");
                format!("Cached{}@s#X0", mix)
            }
            _ => {
                jc.set("a0", i(5)).expect("set");
                format!("Cached{}@i#5", mix)
            }
        };
        eq(&s(&expected), &ok(compute1.evaluate(jc.clone())));

        jc.set("a0", Value::Null).expect("set");
        thrown(compute1null.evaluate(jc.clone()));
    }
    loops
}

// port of: CacheTest.testComputeNoCache
#[test]
fn test_compute_no_cache() {
    run_threaded(compute_task, LOOPS, false);
}

// port of: CacheTest.testComputeCache
#[test]
fn test_compute_cache() {
    run_threaded(compute_task, LOOPS, true);
}

/// port of: `CacheTest.JexlContextNS` — a context whose namespaces come from a mutable map.
struct JexlContextNS {
    vars: MapContext,
    funcs: Mutex<HashMap<String, Value>>,
}

impl JexlContextNS {
    fn new() -> JexlContextNS {
        JexlContextNS { vars: MapContext::new(), funcs: Mutex::new(HashMap::new()) }
    }
    fn put_ns(&self, name: &str, value: Value) {
        self.funcs.lock().expect("lock").insert(name.to_string(), value);
    }
}

impl JexlContext for JexlContextNS {
    fn get(&self, name: &str) -> Option<Value> {
        self.vars.get(name)
    }
    fn set(&self, name: &str, value: Value) -> Result<(), String> {
        self.vars.set(name, value)
    }
    fn has(&self, name: &str) -> bool {
        self.vars.has(name)
    }
    fn resolve_namespace(&self, name: Option<&str>) -> Option<Value> {
        self.funcs.lock().expect("lock").get(name?).cloned()
    }
    fn is_namespace_resolver(&self) -> bool {
        true
    }
}

/// port of: CacheTest.doCOMPUTE — the namespaced static calls; not MT.
fn do_compute(loops: usize, cache: bool) {
    let jexl = cache_engine(cache);
    if !cache {
        jexl.clear_cache();
    }
    // port of: `{Cached.class, Cached1.class, Cached2.class}`; COMPUTE is static on Cached
    let ca: Vec<Value> = [0usize, 1, 2].iter().map(|n| Value::object(CachedClass(*n))).collect();
    let values = [i(2), s("quux")];
    let jc = Arc::new(JexlContextNS::new());
    let compute2 = jexl.create_expression(None, "cached:COMPUTE(a0, a1)").expect("createExpression");
    let compute1 = jexl.create_expression(None, "cached:COMPUTE(a0)").expect("createExpression");

    for l in 0..loops {
        let mix = MIX[l % MIX.len()] % ca.len();
        let value = &values[l % values.len()];

        jc.put_ns("cached", ca[mix].clone());
        let expected = match value {
            Value::String(_) => {
                jc.set("a0", s("S0")).expect("set");
                jc.set("a1", s("S1")).expect("set");
                "CACHED@s#S0,s#S1"
            }
            _ => {
                jc.set("a0", i(7)).expect("set");
                jc.set("a1", i(9)).expect("set");
                "CACHED@i#7,i#9"
            }
        };
        eq(&s(expected), &ok(compute2.evaluate(jc.clone())));

        let expected = match value {
            Value::String(_) => {
                jc.set("a0", s("X0")).expect("set");
                "CACHED@s#X0"
            }
            _ => {
                jc.set("a0", i(5)).expect("set");
                "CACHED@i#5"
            }
        };
        eq(&s(expected), &ok(compute1.evaluate(jc.clone())));
    }
}

// port of: CacheTest.testCOMPUTENoCache
#[test]
fn test_computenocache() {
    do_compute(LOOPS, false);
}

// port of: CacheTest.testCOMPUTECache
#[test]
fn test_computecache() {
    do_compute(LOOPS, true);
}
