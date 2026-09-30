//! Shared fixtures for the ported upstream Apache Commons JEXL 3.2.1 test suite.
//!
//! Java's tests lean on three things this port has to spell out: the process-wide default options
//! `JexlTestCase` installs, the `Asserter` / `JexlEvalContext` pair every operator test drives, and
//! the bean fixtures (`Foo`, `Aggregate`, ...) reflection gives Java for free. Each item names the
//! Java class it stands for.
#![allow(dead_code)]

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use rust_jexl::introspection::jdk_shim::{HostIntrospector, JdkShim};
use rust_jexl::introspection::uberspect::Uberspect;
use rust_jexl::introspection::{
    JexlMethod, JexlPropertyGet, JexlPropertySet, JexlUberspect, PropertyResolver, ResolverStrategy,
};
use rust_jexl::java::hash_map::{JHashMap, JHashSet};
use rust_jexl::java::string::JString;
use rust_jexl::jexl_arithmetic::JexlArithmetic;
use rust_jexl::jexl_context::{JexlContext, MapContext};
use rust_jexl::jexl_engine::{JexlBuilder, JexlEngine};
use rust_jexl::jexl_exception::JexlException;
use rust_jexl::jexl_operator::JexlOperator;
use rust_jexl::jexl_options::JexlOptions;
use rust_jexl::value::{Component, HostObject, JArray, JList, JMap, JSet, ListKind, MapKind, SetKind, Value};

// =============================================================================== value shorthands

pub fn s(text: &str) -> Value {
    Value::string(text)
}

pub fn i(n: i32) -> Value {
    Value::Integer(n)
}

pub fn l(n: i64) -> Value {
    Value::Long(n)
}

pub fn d(n: f64) -> Value {
    Value::Double(n)
}

pub fn big_int(text: &str) -> Value {
    Value::big_integer(rust_jexl::java::number::parse_big_integer(text, 10).expect("BigInteger"))
}

pub fn big_dec(text: &str) -> Value {
    Value::big_decimal(rust_jexl::java::big_decimal::BigDecimal::parse(text).expect("BigDecimal"))
}

pub fn list(items: Vec<Value>) -> Value {
    Value::List(JList::new(ListKind::ArrayList, items))
}

pub fn hash_set(items: Vec<Value>) -> Value {
    let mut set = JHashSet::new();
    for v in items {
        set.add(v);
    }
    Value::Set(JSet::new(SetKind::HashSet, set))
}

pub fn hash_map(entries: Vec<(Value, Value)>) -> Value {
    let mut map = JHashMap::new();
    for (k, v) in entries {
        map.put(k, v);
    }
    Value::Map(JMap::new(MapKind::HashMap, map))
}

pub fn object_array(items: Vec<Value>) -> Value {
    Value::Array(JArray::new(Component::object(), items))
}

pub fn int_array(items: &[i32]) -> Value {
    Value::Array(JArray::new(Component::Int, items.iter().map(|n| Value::Integer(*n)).collect()))
}

/// `SomeClass.class` as a value.
pub fn class_of(name: &str) -> Value {
    rust_jexl::introspection::jdk_shim::ClassValue::of(name)
}

pub fn string_array(items: &[&str]) -> Value {
    Value::Array(JArray::new(
        Component::Class("java.lang.String".into()),
        items.iter().map(|t| Value::string(t)).collect(),
    ))
}

/// `Assert.assertEquals(expected, actual)`: Java's `Object.equals`.
#[track_caller]
pub fn assert_java_eq(expected: &Value, actual: &Value) {
    assert!(
        expected.java_equals(actual),
        "expected {:?} ({}), got {:?} ({})",
        expected.java_to_string(),
        expected.class_name(),
        actual.java_to_string(),
        actual.class_name()
    );
}

/// `Assert.assertArrayEquals`: same length, element-wise `equals`.
#[track_caller]
pub fn assert_array_eq(expected: &[Value], actual: &Value) {
    let got = match actual {
        Value::Array(a) => a.snapshot(),
        other => panic!("not an array: {:?}", other.class_name()),
    };
    assert_eq!(expected.len(), got.len(), "array size");
    for (n, (e, g)) in expected.iter().zip(got.iter()).enumerate() {
        assert!(e.java_equals(g), "value@[]{}: expected {:?}, got {:?}", n, e.java_to_string(), g.java_to_string());
    }
}

/// `Assert.assertEquals(double, double, delta)`.
#[track_caller]
pub fn assert_double_eq(expected: f64, actual: &Value, epsilon: f64) {
    let got = match actual {
        Value::Double(v) => *v,
        Value::Float(v) => *v as f64,
        other => panic!("not a floating point value: {:?}", other.class_name()),
    };
    assert!((expected - got).abs() <= epsilon, "expected {}, got {}", expected, got);
}

/// The small delta ArithmeticTest compares doubles with.
pub const EPSILON: f64 = 1.0e-6;

// ====================================================================== JexlTestCase / JexlOptions

/// port of: JexlTestCase's static initializer, `JexlOptions.setDefaultFlags("-safe", "+lexical")`.
///
/// Java mutates `JexlOptions.DEFAULT` process-wide, which the port deliberately does not model;
/// every engine and context the upstream tests build starts from these flags instead.
pub fn default_options() -> JexlOptions {
    let mut options = JexlOptions::new();
    options.set_flags(&["-safe", "+lexical"]);
    options
}

/// A `JexlBuilder` carrying the upstream default flags and the test fixtures' introspector.
pub fn builder() -> JexlBuilder {
    JexlBuilder::new().uberspect(uberspect()).safe(false).lexical(true)
}

/// port of: `JexlTestCase(String)` — `new JexlBuilder().cache(128).create()`.
pub fn jexl() -> Arc<JexlEngine> {
    builder().cache(128).create()
}

/// port of: `JexlTestCase.createEngine()` — `new JexlBuilder().create()`.
pub fn create_engine() -> Arc<JexlEngine> {
    builder().create()
}

/// A fresh `MapContext` with no bindings.
pub fn context() -> Arc<MapContext> {
    Arc::new(MapContext::new())
}

// ================================================================================ JexlEvalContext

/// port of: `org.apache.commons.jexl3.JexlEvalContext` (a test-source class).
///
/// A context wrapping variables, a namespace and a *mutable* `JexlOptions`; the operator tests
/// flip strictness between assertions through `getEngineOptions()`.
pub struct JexlEvalContext {
    vars: MapContext,
    options: RwLock<JexlOptions>,
    namespace: Option<Value>,
}

impl Default for JexlEvalContext {
    fn default() -> Self {
        JexlEvalContext::new()
    }
}

impl JexlEvalContext {
    pub fn new() -> JexlEvalContext {
        JexlEvalContext { vars: MapContext::new(), options: RwLock::new(default_options()), namespace: None }
    }

    /// Resolves every namespace (including the default one) to `ns`, the way the tests'
    /// `EmptyTestContext`/`DateContext` resolve to themselves.
    pub fn with_namespace(mut self, ns: Value) -> Self {
        self.namespace = Some(ns);
        self
    }

    /// port of: `JexlEvalContext.getEngineOptions()` followed by a setter.
    pub fn set_option(&self, f: impl FnOnce(&mut JexlOptions)) {
        f(&mut self.options.write().unwrap_or_else(|p| p.into_inner()));
    }
}

impl JexlContext for JexlEvalContext {
    fn get(&self, name: &str) -> Option<Value> {
        self.vars.get(name)
    }
    fn set(&self, name: &str, value: Value) -> Result<(), String> {
        self.vars.set(name, value)
    }
    fn has(&self, name: &str) -> bool {
        self.vars.has(name)
    }
    fn resolve_namespace(&self, _name: Option<&str>) -> Option<Value> {
        self.namespace.clone()
    }
    fn is_namespace_resolver(&self) -> bool {
        self.namespace.is_some()
    }
    fn get_engine_options(&self) -> Option<JexlOptions> {
        Some(self.options.read().unwrap_or_else(|p| p.into_inner()).clone())
    }
}

// ======================================================================================= Asserter

/// port of: `org.apache.commons.jexl3.junit.Asserter`.
pub struct Asserter {
    engine: Arc<JexlEngine>,
    context: Arc<JexlEvalContext>,
}

impl Asserter {
    pub fn new(jexl: Arc<JexlEngine>) -> Asserter {
        Asserter { engine: jexl, context: Arc::new(JexlEvalContext::new()) }
    }

    pub fn get_context(&self) -> Arc<JexlEvalContext> {
        self.context.clone()
    }

    // port of: Asserter.setStrict(boolean)
    pub fn set_strict(&self, strict: bool) {
        self.context.set_option(|o| o.set_strict(strict));
    }

    // port of: Asserter.setStrict(boolean, boolean)
    pub fn set_strict2(&self, engine_strict: bool, arithmetic_strict: bool) {
        self.context.set_option(|o| {
            o.set_strict(engine_strict);
            o.set_strict_arithmetic(arithmetic_strict);
        });
    }

    // port of: Asserter.setSilent
    pub fn set_silent(&self, silent: bool) {
        self.context.set_option(|o| o.set_silent(silent));
    }

    // port of: Asserter.setVariable
    pub fn set_variable(&self, name: &str, value: Value) {
        self.context.set(name, value).expect("set");
    }

    // port of: Asserter.getVariable
    pub fn get_variable(&self, name: &str) -> Value {
        self.context.get(name).unwrap_or(Value::Null)
    }

    /// Evaluates `expression` the way `Asserter.assertExpression` does, without asserting.
    pub fn evaluate(&self, expression: &str) -> Result<Value, JexlException> {
        self.engine.create_script(expression)?.execute(self.context.clone())
    }

    // port of: Asserter.assertExpression
    #[track_caller]
    pub fn assert_expression(&self, expression: &str, expected: &Value) {
        let value = match self.evaluate(expression) {
            Ok(v) => v,
            Err(e) => panic!("expression: {}: {}", expression, e.message()),
        };
        if let Value::BigDecimal(expected) = expected {
            let arithmetic = self.engine.get_arithmetic();
            let got = arithmetic.to_big_decimal(&value).expect("toBigDecimal");
            assert_eq!(std::cmp::Ordering::Equal, expected.compare_to(&got), "expression: {}", expression);
        }
        if !expected.is_null() && !value.is_null() {
            if let (Value::Array(e), Value::Array(_)) = (expected, &value) {
                assert_array_eq(&e.snapshot(), &value);
                return;
            }
        }
        assert!(
            expected.java_equals(&value),
            "expression: {}, {} ?= {}: expected {:?}, got {:?}",
            expression,
            expected.simple_name(),
            value.simple_name(),
            expected.java_to_string(),
            value.java_to_string()
        );
    }

    // port of: Asserter.failExpression
    #[track_caller]
    pub fn fail_expression(&self, expression: &str, match_exception: Option<&str>) {
        match self.evaluate(expression) {
            Ok(_) => panic!("expression: {}", expression),
            Err(e) if !e.is_jexl() => panic!("expression: {}: not a JexlException: {}", expression, e.message()),
            Err(e) => {
                if let Some(pattern) = match_exception {
                    let message = e.get_message().unwrap_or_else(JString::empty).to_rust();
                    let p = rust_jexl::java::regex::Pattern::compile(pattern).expect("pattern");
                    assert!(
                        p.matches(&message),
                        "expression: {}, expected: {}, got {}",
                        expression,
                        pattern,
                        message
                    );
                }
            }
        }
    }
}

// ============================================================ Engine.getProperty / setProperty SPI

/// port of: `Engine.getProperty(Object, String)` — the public API the port does not expose yet.
pub fn get_property(jexl: &JexlEngine, obj: &Value, name: &str) -> Result<Value, JexlException> {
    let uber = uberspect();
    let ident = Value::string(name);
    let resolvers = uber.get_resolvers(None, obj);
    match uber.get_property_get_with(resolvers, obj, &ident) {
        Some(g) => g.invoke(obj),
        None => Err(JexlException::property(None, name, true, None)),
    }
    .map_err(|e| {
        let _ = jexl;
        e
    })
}

/// port of: `Engine.setProperty(Object, String, Object)`.
pub fn set_property(jexl: &JexlEngine, obj: &Value, name: &str, value: &Value) -> Result<(), JexlException> {
    let _ = jexl;
    let uber = uberspect();
    let ident = Value::string(name);
    let resolvers = uber.get_resolvers(None, obj);
    match uber.get_property_set_with(resolvers, obj, &ident, value) {
        Some(setter) => setter.invoke(obj, value).map(|_| ()),
        None => Err(JexlException::property(None, name, true, None)),
    }
}

// ====================================================================== uberspect / host fixtures

/// The uberspect every ported test uses: the JDK shim plus the fixtures below.
pub fn uberspect() -> Arc<dyn JexlUberspect> {
    let shim = JdkShim::new(ResolverStrategy::Jexl).with_hosts(Arc::new(UpstreamHosts));
    Arc::new(Uberspect::new().with_shim(Arc::new(shim)))
}

/// port of: `JexlArithmetic.Uberspect` — the operators an arithmetic subclass overloads.
///
/// `JexlArithmetic` is a struct here, so a test that adds `add(Var, Var)` registers it through the
/// uberspect, which is exactly the path `Operators.tryOverload` takes in Java.
pub trait ArithmeticOverloads: Send + Sync {
    fn overloads(&self, operator: JexlOperator) -> bool;
    fn get_operator(&self, operator: JexlOperator, args: &[Value]) -> Option<Arc<dyn JexlMethod>>;
}

/// An uberspect that adds a custom arithmetic's operator overloads to the standard one.
pub struct OverloadUberspect {
    inner: Uberspect,
    overloads: Arc<dyn ArithmeticOverloads>,
}

impl OverloadUberspect {
    pub fn new(overloads: Arc<dyn ArithmeticOverloads>) -> OverloadUberspect {
        let shim = JdkShim::new(ResolverStrategy::Jexl).with_hosts(Arc::new(UpstreamHosts));
        OverloadUberspect { inner: Uberspect::new().with_shim(Arc::new(shim)), overloads }
    }
}

impl JexlUberspect for OverloadUberspect {
    fn get_resolvers(&self, op: Option<JexlOperator>, obj: &Value) -> &'static [PropertyResolver] {
        self.inner.get_resolvers(op, obj)
    }
    fn get_constructor(&self, ctor_handle: &Value, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        self.inner.get_constructor(ctor_handle, args)
    }
    fn get_method(&self, obj: &Value, method: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        self.inner.get_method(obj, method, args)
    }
    fn get_property_get_with(
        &self,
        resolvers: &[PropertyResolver],
        obj: &Value,
        identifier: &Value,
    ) -> Option<Arc<dyn JexlPropertyGet>> {
        self.inner.get_property_get_with(resolvers, obj, identifier)
    }
    fn get_property_set_with(
        &self,
        resolvers: &[PropertyResolver],
        obj: &Value,
        identifier: &Value,
        arg: &Value,
    ) -> Option<Arc<dyn JexlPropertySet>> {
        self.inner.get_property_set_with(resolvers, obj, identifier, arg)
    }
    fn get_iterator(&self, obj: &Value) -> Option<Box<dyn Iterator<Item = Value> + Send>> {
        self.inner.get_iterator(obj)
    }
    fn get_operator(&self, operator: JexlOperator, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        self.overloads.get_operator(operator, args)
    }
    fn overloads(&self, operator: JexlOperator) -> bool {
        self.overloads.overloads(operator)
    }
}

/// One fixture method; `ret` is the Java return type the interpreter reports.
pub struct HostMethod {
    pub ret: &'static str,
    pub call: Box<dyn Fn(&Value, &[Value]) -> Result<Value, JexlException> + Send + Sync>,
}

impl HostMethod {
    pub fn new(
        ret: &'static str,
        call: impl Fn(&Value, &[Value]) -> Result<Value, JexlException> + Send + Sync + 'static,
    ) -> Arc<dyn JexlMethod> {
        Arc::new(HostMethod { ret, call: Box::new(call) })
    }
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

/// A fixture property read.
pub struct HostGet(Box<dyn Fn(&Value) -> Result<Value, JexlException> + Send + Sync>);

impl HostGet {
    pub fn new(f: impl Fn(&Value) -> Result<Value, JexlException> + Send + Sync + 'static) -> Arc<dyn JexlPropertyGet> {
        Arc::new(HostGet(Box::new(f)))
    }
}

impl JexlPropertyGet for HostGet {
    fn invoke(&self, obj: &Value) -> Result<Value, JexlException> {
        (self.0)(obj)
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

/// A fixture property write.
pub struct HostSet(Box<dyn Fn(&Value, &Value) -> Result<Value, JexlException> + Send + Sync>);

impl HostSet {
    pub fn new(
        f: impl Fn(&Value, &Value) -> Result<Value, JexlException> + Send + Sync + 'static,
    ) -> Arc<dyn JexlPropertySet> {
        Arc::new(HostSet(Box::new(f)))
    }
}

impl JexlPropertySet for HostSet {
    fn invoke(&self, obj: &Value, arg: &Value) -> Result<Value, JexlException> {
        (self.0)(obj, arg)
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

// ------------------------------------------------------------------------------ the bean fixtures

/// port of: `org.apache.commons.jexl3.JexlTest.GET_METHOD_STRING`.
pub const GET_METHOD_STRING: &str = "GetMethod string";
/// port of: `org.apache.commons.jexl3.JexlTest.METHOD_STRING`.
pub const METHOD_STRING: &str = "Method string";
/// port of: `ArrayAccessTest.GET_METHOD_ARRAY`.
pub const GET_METHOD_ARRAY: [&str; 3] = ["One", "Two", "Three"];
/// port of: `ArrayAccessTest.GET_METHOD_ARRAY2`.
pub const GET_METHOD_ARRAY2: [[&str; 3]; 2] = [["One", "Two", "Three"], ["Four", "Five", "Six"]];

/// port of: `org.apache.commons.jexl3.Foo`.
#[derive(Debug)]
// ponytail: only the members ArrayAccessTest reaches (`bar`, `array`, `array2`) are modeled.
pub struct Foo;

impl Foo {
    pub fn new() -> Value {
        Value::object(Foo)
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

/// port of: `ArrayAccessTest.Sample` — a bean whose `foo` property is an `int[]`.
#[derive(Debug)]
pub struct Sample(Mutex<Value>);

impl Sample {
    pub fn new(array: Value) -> Value {
        Value::object(Sample(Mutex::new(array)))
    }
}

impl HostObject for Sample {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.ArrayAccessTest$Sample".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `ArithmeticTest.Var` — the value type `ArithmeticPlus` overloads every operator for.
#[derive(Debug)]
pub struct Var {
    pub value: AtomicI32,
}

impl Var {
    pub fn new(v: i32) -> Value {
        Value::object(Var { value: AtomicI32::new(v) })
    }
    pub fn get(v: &Value) -> i32 {
        v.as_host::<Var>().expect("Var").value.load(Ordering::Relaxed)
    }
}

impl HostObject for Var {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.ArithmeticTest$Var".into()
    }
    // port of: Var.toString
    fn java_to_string(&self) -> Option<String> {
        Some(self.value.load(Ordering::Relaxed).to_string())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `ArithmeticOperatorTest.MatchingContainer` — duck-typed `contains(int)` only.
#[derive(Debug)]
pub struct MatchingContainer {
    pub values: Vec<i32>,
}

impl MatchingContainer {
    pub fn new(is: &[i32]) -> Value {
        Value::object(MatchingContainer { values: is.to_vec() })
    }
}

impl HostObject for MatchingContainer {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.ArithmeticOperatorTest$MatchingContainer".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `ArithmeticOperatorTest.IterableContainer` — an `Iterable` over a `TreeSet<Integer>`
/// that also declares `contains`, `startsWith` and `endsWith` for `int` and `int[]`.
#[derive(Debug)]
pub struct IterableContainer {
    /// the TreeSet's ascending order
    pub values: Vec<i32>,
}

impl IterableContainer {
    pub fn new(is: &[i32]) -> Value {
        let mut values = is.to_vec();
        values.sort_unstable();
        values.dedup();
        Value::object(IterableContainer { values })
    }
}

impl HostObject for IterableContainer {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.ArithmeticOperatorTest$IterableContainer".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `ArithmeticOperatorTest.Aggregate` — a namespace with one static method.
#[derive(Debug)]
pub struct Aggregate;

impl Aggregate {
    pub fn new() -> Value {
        Value::object(Aggregate)
    }
}

impl HostObject for Aggregate {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.ArithmeticOperatorTest$Aggregate".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `ArithmeticTest.Callable173` — `call(String...)` and `call(Integer...)`.
#[derive(Debug)]
pub struct Callable173;

impl Callable173 {
    pub fn new() -> Value {
        Value::object(Callable173)
    }
}

impl HostObject for Callable173 {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.ArithmeticTest$Callable173".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `ArithmeticTest.EmptyTestContext`'s namespace half (`log`), which the context resolves
/// itself to. `Value::object` cannot be the context, so the methods live on this handle.
#[derive(Debug)]
pub struct EmptyTestNs;

impl HostObject for EmptyTestNs {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.ArithmeticTest$EmptyTestContext".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `ArithmeticTest.EmptyTestContext` — a MapContext that resolves every namespace to
/// itself, so a bare `log(...)` call finds the static `log` methods.
pub fn empty_test_context() -> Arc<JexlEvalContext> {
    Arc::new(JexlEvalContext::new().with_namespace(Value::object(EmptyTestNs)))
}

/// port of: `PublicFieldsTest.Inner` — `public double aDouble` (and the static `NOT42`).
#[derive(Debug)]
pub struct Inner {
    pub a_double: Mutex<Value>,
}

/// port of: `PublicFieldsTest.Struct` — all fields public.
#[derive(Debug)]
pub struct Struct {
    pub inner: Value,
    pub an_int: Mutex<Value>,
    pub a_string: Mutex<Value>,
}

impl Struct {
    pub fn new() -> Value {
        Value::object(Struct {
            inner: Value::object(Inner { a_double: Mutex::new(Value::Double(42.0)) }),
            an_int: Mutex::new(Value::Integer(42)),
            a_string: Mutex::new(Value::string("fourty-two")),
        })
    }
}

impl HostObject for Inner {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.PublicFieldsTest$Inner".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl HostObject for Struct {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.PublicFieldsTest$Struct".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `PropertyAccessTest.PromptValue`.
#[derive(Debug)]
pub struct PromptValue(pub Mutex<Value>);

impl HostObject for PromptValue {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.PropertyAccessTest$PromptValue".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: `PropertyAccessTest.Prompt` — duck-typed `get(String)` / `set(String, Object)`.
#[derive(Debug)]
pub struct Prompt(Mutex<Vec<(String, Value)>>);

impl Prompt {
    pub fn new() -> Value {
        Value::object(Prompt(Mutex::new(Vec::new())))
    }

    // port of: Prompt.set
    pub fn set(v: &Value, name: &str, value: Value) {
        let p = v.as_host::<Prompt>().expect("Prompt");
        let mut values = p.0.lock().expect("lock");
        let entry = (name.to_string(), Value::object(PromptValue(Mutex::new(value))));
        match values.iter().position(|(k, _)| k == name) {
            Some(n) => values[n] = entry,
            None => values.push(entry),
        }
    }

    // port of: Prompt.get
    pub fn get(&self, name: &str) -> Value {
        match self.0.lock().expect("lock").iter().find(|(k, _)| k == name) {
            Some((_, v)) => v.as_host::<PromptValue>().expect("PromptValue").0.lock().expect("lock").clone(),
            None => Value::Null,
        }
    }
}

impl HostObject for Prompt {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.PropertyAccessTest$Prompt".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ---------------------------------------------------------------------- the fixtures' introspector

fn to_i32(v: &Value) -> Option<i32> {
    match v {
        Value::Byte(b) => Some(*b as i32),
        Value::Short(s) => Some(*s as i32),
        Value::Integer(n) => Some(*n),
        _ => None,
    }
}

fn int_arg_array(v: &Value) -> Option<Vec<i32>> {
    match v {
        Value::Array(a) if a.component == Component::Int => Some(a.snapshot().iter().filter_map(to_i32).collect()),
        _ => None,
    }
}

fn ic_of(o: &Value) -> &IterableContainer {
    o.as_host::<IterableContainer>().expect("IterableContainer")
}

fn nfe(text: &str) -> JexlException {
    JexlException::java("java.lang.NumberFormatException", Some(format!("For input string: \"{}\"", text)))
}

/// port of: the reflective discovery Java performs over the test bean classes.
pub struct UpstreamHosts;

impl HostIntrospector for UpstreamHosts {
    fn get_method(&self, obj: &Value, name: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        if obj.as_host::<Foo>().is_some() {
            return match (name, args.len()) {
                ("bar", 0) => Some(HostMethod::new("java.lang.String", |_, _| Ok(s(METHOD_STRING)))),
                ("getBar", 0) => Some(HostMethod::new("java.lang.String", |_, _| Ok(s(GET_METHOD_STRING)))),
                ("getArray", 0) => Some(HostMethod::new("[Ljava.lang.String;", |_, _| Ok(string_array(&GET_METHOD_ARRAY)))),
                ("getArray2", 0) => Some(HostMethod::new("[[Ljava.lang.String;", |_, _| {
                    Ok(Value::Array(JArray::new(
                        Component::Class("[Ljava.lang.String;".into()),
                        GET_METHOD_ARRAY2.iter().map(|row| string_array(row)).collect(),
                    )))
                })),
                _ => None,
            };
        }
        if obj.as_host::<Aggregate>().is_some() {
            // port of: Aggregate.sum(Iterable<Integer>)
            return match (name, args.len()) {
                ("sum", 1) => Some(HostMethod::new("int", |_, a| {
                    let mut sum: i32 = 0;
                    for v in iterate(&a[0]) {
                        sum = sum.wrapping_add(to_i32(&v).unwrap_or(0));
                    }
                    Ok(Value::Integer(sum))
                })),
                _ => None,
            };
        }
        if obj.as_host::<Callable173>().is_some() {
            // port of: Callable173.call(String...) / call(Integer...)
            return match name {
                "call" if args.iter().all(|v| matches!(v, Value::String(_))) => {
                    Some(HostMethod::new("java.lang.Object", |_, _| Ok(Value::Integer(42))))
                }
                "call" if args.iter().all(|v| matches!(v, Value::Integer(_))) => {
                    Some(HostMethod::new("java.lang.Object", |_, a| {
                        Ok(Value::Integer(to_i32(&a[0]).unwrap_or(0).wrapping_mul(to_i32(&a[1]).unwrap_or(0))))
                    }))
                }
                _ => None,
            };
        }
        if obj.as_host::<EmptyTestNs>().is_some() {
            // port of: EmptyTestContext.log(Object fmt, Object... arr) — returns `arr.length`,
            // or 0 when Java passes the lone trailing argument straight through as the array
            return match name {
                "log" => Some(HostMethod::new("int", |_, a| {
                    Ok(Value::Integer(match a.len() {
                        2 => match &a[1] {
                            Value::Null => 0,
                            Value::Array(arr) => arr.len() as i32,
                            _ => 1,
                        },
                        n => n as i32 - 1,
                    }))
                })),
                _ => None,
            };
        }
        if obj.as_host::<MatchingContainer>().is_some() {
            // port of: MatchingContainer.contains(int)
            return match (name, args.len()) {
                ("contains", 1) if to_i32(&args[0]).is_some() => Some(HostMethod::new("boolean", |o, a| {
                    let c = o.as_host::<MatchingContainer>().expect("MatchingContainer");
                    Ok(Value::Boolean(c.values.contains(&to_i32(&a[0]).expect("int"))))
                })),
                _ => None,
            };
        }
        if obj.as_host::<IterableContainer>().is_some() {
            return match (name, args.len()) {
                // contains(int) / contains(int[])
                ("contains", 1) if to_i32(&args[0]).is_some() => Some(HostMethod::new("boolean", |o, a| {
                    Ok(Value::Boolean(ic_of(o).values.contains(&to_i32(&a[0]).expect("int"))))
                })),
                ("contains", 1) if int_arg_array(&args[0]).is_some() => Some(HostMethod::new("boolean", |o, a| {
                    // Java's `values.containsAll(singletonList(int[]))`: the array itself is the element
                    let _ = (o, a);
                    Ok(Value::Boolean(false))
                })),
                // startsWith(int): the TreeSet's first element
                ("startsWith", 1) if to_i32(&args[0]).is_some() => Some(HostMethod::new("boolean", |o, a| {
                    Ok(Value::Boolean(ic_of(o).values.first() == Some(&to_i32(&a[0]).expect("int"))))
                })),
                ("endsWith", 1) if to_i32(&args[0]).is_some() => Some(HostMethod::new("boolean", |o, a| {
                    Ok(Value::Boolean(ic_of(o).values.last() == Some(&to_i32(&a[0]).expect("int"))))
                })),
                // startsWith(int[]): compares headSet(i.length) element-wise
                ("startsWith", 1) if int_arg_array(&args[0]).is_some() => Some(HostMethod::new("boolean", |o, a| {
                    let want = int_arg_array(&a[0]).expect("int[]");
                    let values = &ic_of(o).values;
                    let head: Vec<i32> = values.iter().copied().filter(|v| *v < want.len() as i32).collect();
                    Ok(Value::Boolean(head.iter().zip(want.iter()).all(|(x, y)| x == y) && head.len() <= want.len()))
                })),
                ("endsWith", 1) if int_arg_array(&args[0]).is_some() => Some(HostMethod::new("boolean", |o, a| {
                    let want = int_arg_array(&a[0]).expect("int[]");
                    let values = &ic_of(o).values;
                    let from = values.len() as i32 - want.len() as i32;
                    let tail: Vec<i32> = values.iter().copied().filter(|v| *v >= from).collect();
                    Ok(Value::Boolean(tail.iter().zip(want.iter()).all(|(x, y)| x == y) && tail.len() <= want.len()))
                })),
                _ => None,
            };
        }
        if obj.as_host::<Prompt>().is_some() {
            // port of: Prompt.get(String) / Prompt.set(String, Object)
            return match (name, args.len()) {
                ("get", 1) => Some(HostMethod::new("java.lang.Object", |o, a| {
                    Ok(o.as_host::<Prompt>().expect("Prompt").get(&a[0].java_to_string()))
                })),
                _ => None,
            };
        }
        let _ = (name, args);
        None
    }

    fn get_property_get(&self, obj: &Value, identifier: &Value) -> Option<Arc<dyn JexlPropertyGet>> {
        let name = identifier.java_to_string();
        if obj.as_host::<Foo>().is_some() {
            return match name.as_str() {
                "bar" => Some(HostGet::new(|_| Ok(s(GET_METHOD_STRING)))),
                "array" => Some(HostGet::new(|_| Ok(string_array(&GET_METHOD_ARRAY)))),
                "array2" => Some(HostGet::new(|_| {
                    Ok(Value::Array(JArray::new(
                        Component::Class("[Ljava.lang.String;".into()),
                        GET_METHOD_ARRAY2.iter().map(|row| string_array(row)).collect(),
                    )))
                })),
                _ => None,
            };
        }
        if obj.as_host::<Sample>().is_some() {
            return match name.as_str() {
                "foo" => Some(HostGet::new(|o| {
                    Ok(o.as_host::<Sample>().expect("Sample").0.lock().expect("lock").clone())
                })),
                _ => None,
            };
        }
        if obj.as_host::<Struct>().is_some() {
            return match name.as_str() {
                "inner" => Some(HostGet::new(|o| Ok(o.as_host::<Struct>().expect("Struct").inner.clone()))),
                "anInt" => {
                    Some(HostGet::new(|o| Ok(o.as_host::<Struct>().expect("Struct").an_int.lock().expect("lock").clone())))
                }
                "aString" => Some(HostGet::new(|o| {
                    Ok(o.as_host::<Struct>().expect("Struct").a_string.lock().expect("lock").clone())
                })),
                _ => None,
            };
        }
        if obj.as_host::<Inner>().is_some() {
            return match name.as_str() {
                "aDouble" => Some(HostGet::new(|o| {
                    Ok(o.as_host::<Inner>().expect("Inner").a_double.lock().expect("lock").clone())
                })),
                _ => None,
            };
        }
        if obj.as_host::<PromptValue>().is_some() {
            return match name.as_str() {
                "value" => Some(HostGet::new(|o| {
                    Ok(o.as_host::<PromptValue>().expect("PromptValue").0.lock().expect("lock").clone())
                })),
                _ => None,
            };
        }
        if obj.as_host::<Prompt>().is_some() {
            // the Duck resolver: Prompt.get(String)
            let key = name;
            return Some(HostGet::new(move |o| Ok(o.as_host::<Prompt>().expect("Prompt").get(&key))));
        }
        None
    }

    fn get_property_set(&self, obj: &Value, identifier: &Value, arg: &Value) -> Option<Arc<dyn JexlPropertySet>> {
        let name = identifier.java_to_string();
        if obj.as_host::<Sample>().is_some() && name == "foo" {
            // port of: Sample.setFoo(int[])
            return match arg {
                Value::Array(_) => Some(HostSet::new(|o, v| {
                    *o.as_host::<Sample>().expect("Sample").0.lock().expect("lock") = v.clone();
                    Ok(v.clone())
                })),
                _ => None,
            };
        }
        if obj.as_host::<Struct>().is_some() {
            // a public field only accepts an assignable value; anything else is an error
            return match (name.as_str(), arg) {
                ("anInt", v) if to_i32(v).is_some() => Some(HostSet::new(|o, v| {
                    *o.as_host::<Struct>().expect("Struct").an_int.lock().expect("lock") = Value::Integer(to_i32(v).expect("int"));
                    Ok(v.clone())
                })),
                ("aString", Value::String(_)) => Some(HostSet::new(|o, v| {
                    *o.as_host::<Struct>().expect("Struct").a_string.lock().expect("lock") = v.clone();
                    Ok(v.clone())
                })),
                _ => None,
            };
        }
        if obj.as_host::<Inner>().is_some() {
            return match (name.as_str(), arg) {
                ("aDouble", Value::Double(_) | Value::Float(_) | Value::Integer(_) | Value::Long(_) | Value::Short(_) | Value::Byte(_)) => {
                    Some(HostSet::new(|o, v| {
                        let d = match v {
                            Value::Double(x) => *x,
                            Value::Float(x) => *x as f64,
                            Value::Long(x) => *x as f64,
                            Value::Integer(x) => *x as f64,
                            Value::Short(x) => *x as f64,
                            Value::Byte(x) => *x as f64,
                            _ => return Err(nfe(&v.java_to_string())),
                        };
                        *o.as_host::<Inner>().expect("Inner").a_double.lock().expect("lock") = Value::Double(d);
                        Ok(v.clone())
                    }))
                }
                _ => None,
            };
        }
        if obj.as_host::<PromptValue>().is_some() && name == "value" {
            return Some(HostSet::new(|o, v| {
                *o.as_host::<PromptValue>().expect("PromptValue").0.lock().expect("lock") = v.clone();
                Ok(v.clone())
            }));
        }
        if obj.as_host::<Prompt>().is_some() {
            // the Duck resolver: Prompt.set(String, Object)
            let key = name;
            return Some(HostSet::new(move |o, v| {
                Prompt::set(o, &key, v.clone());
                Ok(v.clone())
            }));
        }
        let _ = arg;
        None
    }

    fn get_constructor(&self, handle: &Value, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        let _ = (handle, args);
        None
    }
}

/// The values `for(x : v)` walks, for the fixtures that are `Iterable`.
pub fn iterate(v: &Value) -> Vec<Value> {
    if let Some(c) = v.as_host::<IterableContainer>() {
        return c.values.iter().map(|n| Value::Integer(*n)).collect();
    }
    uberspect().get_iterator(v).map(|it| it.collect()).unwrap_or_default()
}

/// A namespace map for `JexlBuilder::namespaces`.
pub fn namespaces(entries: &[(&str, Value)]) -> HashMap<String, Value> {
    entries.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

// ------------------------------------------------------------------------- ArithmeticTest.ArithmeticPlus

/// port of: `ArithmeticTest.ArithmeticPlus` — an arithmetic that knows how to subtract strings and
/// that overloads every operator for `Var`.
///
/// Java discovers the overloads reflectively on the subclass; here they are registered through the
/// same `JexlArithmetic.Uberspect` hook `Operators.tryOverload` consults.
pub struct ArithmeticPlus;

fn is_var(v: &Value) -> bool {
    v.as_host::<Var>().is_some()
}

fn is_str(v: &Value) -> bool {
    matches!(v, Value::String(_))
}

fn var_bool(f: fn(i32, i32) -> bool) -> Arc<dyn JexlMethod> {
    Arc::new(HostMethod {
        ret: "boolean",
        call: Box::new(move |_, a| Ok(Value::Boolean(f(Var::get(&a[0]), Var::get(&a[1]))))),
    })
}

fn var_var(f: fn(i32, i32) -> i32) -> Arc<dyn JexlMethod> {
    Arc::new(HostMethod {
        ret: "org.apache.commons.jexl3.ArithmeticTest$Var",
        call: Box::new(move |_, a| Ok(Var::new(f(Var::get(&a[0]), Var::get(&a[1]))))),
    })
}

fn var_text(f: fn(&str, &str) -> bool) -> Arc<dyn JexlMethod> {
    Arc::new(HostMethod {
        ret: "java.lang.Boolean",
        call: Box::new(move |_, a| Ok(Value::Boolean(f(&a[0].java_to_string(), &a[1].java_to_string())))),
    })
}

impl ArithmeticOverloads for ArithmeticPlus {
    fn overloads(&self, operator: JexlOperator) -> bool {
        use JexlOperator::*;
        matches!(
            operator,
            Eq | Lt | Lte | Gt | Gte | Add | Subtract | Divide | Multiply | Mod | Negate | And | Or | Xor
                | Contains | StartsWith | EndsWith | Complement | Not
        )
    }

    fn get_operator(&self, operator: JexlOperator, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        use JexlOperator::*;
        let vv = args.len() == 2 && is_var(&args[0]) && is_var(&args[1]);
        let ss = args.len() == 2 && is_str(&args[0]) && is_str(&args[1]);
        let v1 = args.len() == 1 && is_var(&args[0]);
        let s1 = args.len() == 1 && is_str(&args[0]);
        Some(match operator {
            Eq if vv => var_bool(|a, b| a == b),
            Lt if vv => var_bool(|a, b| a < b),
            Lte if vv => var_bool(|a, b| a <= b),
            Gt if vv => var_bool(|a, b| a > b),
            Gte if vv => var_bool(|a, b| a >= b),
            Add if vv => var_var(|a, b| a.wrapping_add(b)),
            Subtract if vv => var_var(|a, b| a.wrapping_sub(b)),
            Divide if vv => var_var(|a, b| a.wrapping_div(b)),
            Multiply if vv => var_var(|a, b| a.wrapping_mul(b)),
            // port of: ArithmeticPlus.mod — yes, it divides
            Mod if vv => var_var(|a, b| a.wrapping_div(b)),
            And if vv => var_var(|a, b| a & b),
            Or if vv => var_var(|a, b| a | b),
            Xor if vv => var_var(|a, b| a ^ b),
            Contains if vv => var_text(|a, b| a.contains(b)),
            StartsWith if vv => var_text(|a, b| a.starts_with(b)),
            EndsWith if vv => var_text(|a, b| a.ends_with(b)),
            Negate if v1 => Arc::new(HostMethod {
                ret: "org.apache.commons.jexl3.ArithmeticTest$Var",
                call: Box::new(|_, a| Ok(Var::new(Var::get(&a[0]).wrapping_neg()))),
            }),
            Complement if v1 => Arc::new(HostMethod {
                ret: "org.apache.commons.jexl3.ArithmeticTest$Var",
                call: Box::new(|_, a| Ok(Var::new(!Var::get(&a[0])))),
            }),
            // port of: ArithmeticPlus.not(Var) — throws, on purpose
            Not if v1 => Arc::new(HostMethod {
                ret: "java.lang.Object",
                call: Box::new(|_, _| {
                    Err(JexlException::java("java.lang.NullPointerException", Some("make it fail".into())))
                }),
            }),
            // port of: ArithmeticPlus.subtract(String, String) — removes the first occurrence
            Subtract if ss => Arc::new(HostMethod {
                ret: "java.lang.Object",
                call: Box::new(|_, a| {
                    let x = a[0].java_to_string();
                    let y = a[1].java_to_string();
                    Ok(match x.find(&y) {
                        None => s(&x),
                        Some(ix) => s(&format!("{}{}", &x[..ix], &x[ix + y.len()..])),
                    })
                }),
            }),
            // port of: ArithmeticPlus.negate(String) — reverses it
            Negate if s1 => Arc::new(HostMethod {
                ret: "java.lang.Object",
                call: Box::new(|_, a| {
                    let text = a[0].java_to_string();
                    Ok(s(&text.chars().rev().collect::<String>()))
                }),
            }),
            _ => return None,
        })
    }
}

/// An engine whose arithmetic carries `ArithmeticPlus`'s overloads.
pub fn arithmetic_plus_engine(cache: i32) -> Arc<JexlEngine> {
    JexlBuilder::new()
        .uberspect(Arc::new(OverloadUberspect::new(Arc::new(ArithmeticPlus))))
        .safe(false)
        .lexical(true)
        .cache(cache)
        .arithmetic(JexlArithmetic::new(false, None, i32::MIN))
        .create()
}
