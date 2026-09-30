// port of: org.apache.commons.jexl3.internal.introspection (Uberspect, Introspector, ClassMap,
// MethodKey, MethodExecutor, ConstructorMethod and the Get/Set executors) for the JDK types.
//
// Java resolves everything through reflection over real classes. There is no reflection here, so
// the JDK classes scripts reach are modeled explicitly: `TABLES` is the `ClassMap` (what
// `Class.getDeclaredMethods()` would return, per declaring class), `FIELDS` is its field cache and
// `CTORS` is `Class.getConstructors()`. Everything above that - applicability, the most-specific
// rule, varargs packing, the resolver order - is a line-by-line port and is shared by every type.
//
// Host objects (user Rust types) plug in through `HostIntrospector`; see `JdkShim::with_hosts`.
use std::any::Any;
use std::sync::{Arc, Mutex};

use num_bigint::{BigInt, Sign};
use num_traits::{One, Signed, Zero};

use crate::internal::range::{Direction, Range, Width};
use crate::java::big_decimal::{BigDecimal, MathError, RoundingMode};
use crate::java::hash_map::{JHashMap, JHashSet};
use crate::java::map_view::{MapEntry, MapView, ViewKind};
use crate::java::number;
use crate::java::regex;
use crate::java::string::{JString, JStringBuilder};
use crate::jexl_exception::JexlException;
use crate::jexl_operator::JexlOperator;
use crate::value::{Component, JArray, JList, JMap, JSet, ListKind, MapKind, SetKind, Value};

use super::{JexlMethod, JexlPropertyGet, JexlPropertySet, JexlUberspect, PropertyResolver, ResolverStrategy, TryResult};

// ---------------------------------------------------------------------------------------------
// Java throwables the shim raises (JexlException::java keeps the Java class name and message).

fn jthrow<T>(class: &str, msg: &str) -> Result<T, JexlException> {
    Err(JexlException::java(class, Some(msg.to_string())))
}

fn npe<T>(msg: &str) -> Result<T, JexlException> {
    jthrow("java.lang.NullPointerException", msg)
}

/// A NullPointerException with no message (Java's `null` detail).
fn npe0<T>() -> Result<T, JexlException> {
    Err(JexlException::java("java.lang.NullPointerException", None))
}

fn iae<T>(msg: &str) -> Result<T, JexlException> {
    jthrow("java.lang.IllegalArgumentException", msg)
}

fn arithmetic<T>(msg: &str) -> Result<T, JexlException> {
    jthrow("java.lang.ArithmeticException", msg)
}

fn ioobe<T>(index: i64, length: usize) -> Result<T, JexlException> {
    jthrow(
        "java.lang.IndexOutOfBoundsException",
        &format!("Index {} out of bounds for length {}", index, length),
    )
}

fn sioobe_index<T>(index: i64, length: usize) -> Result<T, JexlException> {
    jthrow(
        "java.lang.StringIndexOutOfBoundsException",
        &format!("Index {} out of bounds for length {}", index, length),
    )
}

fn sioobe_range<T>(from: i64, to: i64, length: usize) -> Result<T, JexlException> {
    jthrow(
        "java.lang.StringIndexOutOfBoundsException",
        &format!("Range [{}, {}) out of bounds for length {}", from, to, length),
    )
}

/// java.lang.ClassCastException, as the JDK words it for two java.base classes.
fn cce<T>(from: &str, to: &str) -> Result<T, JexlException> {
    jthrow(
        "java.lang.ClassCastException",
        &format!(
            "class {} cannot be cast to class {} ({} and {} are in module java.base of loader 'bootstrap')",
            from, to, from, to
        ),
    )
}

fn nfe<T>(e: number::NumberFormatException) -> Result<T, JexlException> {
    jthrow("java.lang.NumberFormatException", &e.0)
}

fn math<T>(e: MathError) -> Result<T, JexlException> {
    match e {
        MathError::Arithmetic(m) => jthrow("java.lang.ArithmeticException", &m.to_rust()),
        MathError::NumberFormat(m) => jthrow("java.lang.NumberFormatException", &m.to_rust()),
    }
}

// ---------------------------------------------------------------------------------------------
// java.lang.Class as a value (what `getClass()` and the `class` property return).

/// port of: java.lang.Class (only the identity JEXL can observe: its name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassValue {
    /// Class.getName()
    pub name: String,
    /// primitive classes (Integer.TYPE) print as their name, not "class <name>"
    pub primitive: bool,
}

impl ClassValue {
    pub fn of(name: &str) -> Value {
        Value::object(ClassValue { name: name.to_string(), primitive: false })
    }
    fn primitive(name: &str) -> Value {
        Value::object(ClassValue { name: name.to_string(), primitive: true })
    }
}

impl crate::value::HostObject for ClassValue {
    fn class_name(&self) -> String {
        "java.lang.Class".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some(if self.primitive { self.name.clone() } else { format!("class {}", self.name) })
    }
    fn java_equals(&self, other: &Value) -> Option<bool> {
        Some(matches!(other.as_host::<ClassValue>(), Some(o) if o.name == self.name))
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: IntegerRange.Ascending / LongRange.Descending.
///
/// `Range::iter()` borrows its range, and `getIterator` has to hand back an owned iterator, so the
/// eight-line cursor rule is repeated here. It must stay in step with `internal::range::RangeIter`
/// (post-increment, so a range whose max is the type's MAX_VALUE never ends - as in Java).
fn range_iterator(r: &Range) -> Box<dyn Iterator<Item = Value> + Send> {
    let (width, direction, min, max) = (r.width, r.direction, r.min, r.max);
    let wrap = move |v: i64| match width {
        Width::Integer => Value::Integer(v as i32),
        Width::Long => Value::Long(v),
    };
    let mut cursor = match direction {
        Direction::Ascending => min,
        Direction::Descending => max,
    };
    Box::new(std::iter::from_fn(move || match direction {
        Direction::Ascending => {
            if cursor > max {
                return None;
            }
            let v = wrap(cursor);
            cursor = match width {
                Width::Integer => (cursor as i32).wrapping_add(1) as i64,
                Width::Long => cursor.wrapping_add(1),
            };
            Some(v)
        }
        Direction::Descending => {
            if cursor < min {
                return None;
            }
            let v = wrap(cursor);
            cursor = match width {
                Width::Integer => (cursor as i32).wrapping_sub(1) as i64,
                Width::Long => cursor.wrapping_sub(1),
            };
            Some(v)
        }
    }))
}

/// port of: the JDK collection iterators (java.util.ArrayList$Itr and friends).
///
/// Java's iterators are live views that fail fast; this one walks a snapshot taken at
/// `iterator()` time - see COMPATIBILITY.md.
pub struct JIterator {
    class: &'static str,
    items: Mutex<std::vec::IntoIter<Value>>,
}

impl JIterator {
    #[allow(clippy::new_ret_no_self)] // mirrors the Java constructor: it hands back a value
    fn new(class: &'static str, items: Vec<Value>) -> Value {
        Value::object(JIterator { class, items: Mutex::new(items.into_iter()) })
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, std::vec::IntoIter<Value>> {
        self.items.lock().unwrap_or_else(|p| p.into_inner())
    }
    fn has_next(&self) -> bool {
        !self.lock().as_slice().is_empty()
    }
    fn next(&self) -> Option<Value> {
        self.lock().next()
    }
}

impl crate::value::HostObject for JIterator {
    fn class_name(&self) -> String {
        self.class.into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: java.lang.StringBuilder (the mutable character sequence `new` can build).
pub struct JavaStringBuilder(Mutex<Vec<u16>>);

impl JavaStringBuilder {
    #[allow(clippy::new_ret_no_self)] // mirrors the Java constructor: it hands back a value
    fn new(units: Vec<u16>) -> Value {
        Value::object(JavaStringBuilder(Mutex::new(units)))
    }
    fn units(&self) -> Vec<u16> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    fn append(&self, s: &JString) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).extend_from_slice(s.units());
    }
    fn set_length(&self, n: usize) {
        let mut w = self.0.lock().unwrap_or_else(|p| p.into_inner());
        w.resize(n, 0);
    }
    fn set_char_at(&self, i: usize, c: u16) {
        self.0.lock().unwrap_or_else(|p| p.into_inner())[i] = c;
    }
}

impl crate::value::HostObject for JavaStringBuilder {
    fn class_name(&self) -> String {
        "java.lang.StringBuilder".into()
    }
    fn java_to_string(&self) -> Option<String> {
        Some(JString::new(self.units()).to_rust())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------------------------
// java.lang.Class as MethodKey sees it.

/// port of: java.lang.Class, reduced to what MethodKey compares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JClass {
    /// a primitive type (int, long, char, ...)
    Prim(&'static str),
    /// a reference type, by binary name
    Named(String),
    Array(Box<JClass>),
    /// Void.class: MethodKey's marker for a null argument
    Null,
}

impl JClass {
    fn named(n: &str) -> JClass {
        JClass::Named(n.to_string())
    }
    fn is_primitive(&self) -> bool {
        matches!(self, JClass::Prim(_))
    }
    fn is_array(&self) -> bool {
        matches!(self, JClass::Array(_))
    }
    fn is_object(&self) -> bool {
        matches!(self, JClass::Named(n) if n == "java.lang.Object")
    }
    fn component(&self) -> Option<&JClass> {
        match self {
            JClass::Array(c) => Some(c),
            _ => None,
        }
    }
    /// Class.getName()
    fn name(&self) -> String {
        match self {
            JClass::Prim(p) => (*p).to_string(),
            JClass::Named(n) => n.clone(),
            JClass::Null => "java.lang.Void".into(),
            JClass::Array(c) => format!("[{}", c.descriptor()),
        }
    }
    fn descriptor(&self) -> String {
        match self {
            JClass::Prim("boolean") => "Z".into(),
            JClass::Prim("byte") => "B".into(),
            JClass::Prim("short") => "S".into(),
            JClass::Prim("int") => "I".into(),
            JClass::Prim("long") => "J".into(),
            JClass::Prim("float") => "F".into(),
            JClass::Prim("double") => "D".into(),
            JClass::Prim("char") => "C".into(),
            JClass::Array(c) => format!("[{}", c.descriptor()),
            other => format!("L{};", other.name()),
        }
    }

    /// The runtime class of a value (`obj.getClass()`; null maps to MethodKey's Void marker).
    pub fn of(v: &Value) -> JClass {
        match v {
            Value::Null => JClass::Null,
            Value::Array(a) => JClass::Array(Box::new(component_class(&a.component))),
            other => JClass::Named(other.class_name()),
        }
    }
}

fn component_class(c: &Component) -> JClass {
    match c {
        Component::Boolean => JClass::Prim("boolean"),
        Component::Byte => JClass::Prim("byte"),
        Component::Short => JClass::Prim("short"),
        Component::Int => JClass::Prim("int"),
        Component::Long => JClass::Prim("long"),
        Component::Float => JClass::Prim("float"),
        Component::Double => JClass::Prim("double"),
        Component::Char => JClass::Prim("char"),
        Component::Class(n) => JClass::named(n),
    }
}

fn class_component(c: &JClass) -> Component {
    match c {
        JClass::Prim("boolean") => Component::Boolean,
        JClass::Prim("byte") => Component::Byte,
        JClass::Prim("short") => Component::Short,
        JClass::Prim("int") => Component::Int,
        JClass::Prim("long") => Component::Long,
        JClass::Prim("float") => Component::Float,
        JClass::Prim("double") => Component::Double,
        JClass::Prim("char") => Component::Char,
        other => Component::Class(other.name()),
    }
}

/// Parses a parameter descriptor used in the tables ("int", "java.lang.String", "java.lang.Object[]").
fn desc(d: &str) -> JClass {
    if let Some(base) = d.strip_suffix("[]") {
        return JClass::Array(Box::new(desc(base)));
    }
    match d {
        "boolean" => JClass::Prim("boolean"),
        "byte" => JClass::Prim("byte"),
        "short" => JClass::Prim("short"),
        "int" => JClass::Prim("int"),
        "long" => JClass::Prim("long"),
        "float" => JClass::Prim("float"),
        "double" => JClass::Prim("double"),
        "char" => JClass::Prim("char"),
        "void" => JClass::Prim("void"),
        other => JClass::named(other),
    }
}

/// The transitive supertypes of a modeled class (Class.isAssignableFrom).
fn supers(name: &str) -> &'static [&'static str] {
    const OBJ: &[&str] = &["java.lang.Object"];
    const CMP: &[&str] = &["java.lang.Comparable", "java.io.Serializable", "java.lang.Object"];
    const NUM: &[&str] = &["java.lang.Number", "java.lang.Comparable", "java.io.Serializable", "java.lang.Object"];
    const COLL: &[&str] = &["java.util.Collection", "java.lang.Iterable", "java.lang.Object"];
    const LIST: &[&str] = &[
        "java.util.List",
        "java.util.Collection",
        "java.lang.Iterable",
        "java.util.AbstractList",
        "java.util.AbstractCollection",
        "java.lang.Object",
    ];
    const SET: &[&str] =
        &["java.util.Set", "java.util.Collection", "java.lang.Iterable", "java.util.AbstractCollection", "java.lang.Object"];
    const MAP: &[&str] = &["java.util.Map", "java.util.AbstractMap", "java.lang.Object"];
    match name {
        "java.lang.Object" => &[],
        "java.lang.String" => &["java.lang.CharSequence", "java.lang.Comparable", "java.io.Serializable", "java.lang.Object"],
        "java.lang.StringBuilder" => &["java.lang.CharSequence", "java.lang.Appendable", "java.lang.Object"],
        "java.lang.Character" | "java.lang.Boolean" => CMP,
        "java.lang.Byte" | "java.lang.Short" | "java.lang.Integer" | "java.lang.Long" | "java.lang.Float"
        | "java.lang.Double" | "java.math.BigInteger" | "java.math.BigDecimal" => NUM,
        "java.lang.Number" => &["java.io.Serializable", "java.lang.Object"],
        "java.util.Collection" | "java.util.List" | "java.util.Set" => COLL,
        "java.util.AbstractList" | "java.util.AbstractCollection" => COLL,
        "java.util.ArrayList" | "java.util.LinkedList" | "java.util.Arrays$ArrayList"
        | "java.util.Collections$UnmodifiableRandomAccessList" => LIST,
        "java.util.HashSet" | "java.util.LinkedHashSet" | "java.util.TreeSet" | "java.util.Collections$UnmodifiableSet" => SET,
        "java.util.HashMap" | "java.util.LinkedHashMap" | "java.util.TreeMap" | "java.util.Collections$UnmodifiableMap"
        | "java.util.Collections$EmptyMap" => MAP,
        _ => OBJ,
    }
}

// port of: Class.isAssignableFrom
fn is_assignable(formal: &JClass, actual: &JClass) -> bool {
    match (formal, actual) {
        (JClass::Prim(a), JClass::Prim(b)) => a == b,
        (JClass::Array(f), JClass::Array(a)) => is_assignable(f, a),
        (JClass::Named(f), JClass::Array(_)) => {
            f == "java.lang.Object" || f == "java.lang.Cloneable" || f == "java.io.Serializable"
        }
        (JClass::Named(f), JClass::Named(a)) => f == a || supers(a).contains(&f.as_str()),
        _ => false,
    }
}

/// port of: MethodKey.CONVERTIBLES (a primitive formal accepting boxed actuals)
fn convertibles(formal: &str) -> &'static [&'static str] {
    match formal {
        "boolean" => &["java.lang.Boolean"],
        "char" => &["java.lang.Character"],
        "byte" => &["java.lang.Byte"],
        "short" => &["java.lang.Short", "java.lang.Byte"],
        "int" => &["java.lang.Integer", "java.lang.Short", "java.lang.Byte"],
        "long" => &["java.lang.Long", "java.lang.Integer", "java.lang.Short", "java.lang.Byte"],
        "float" => &["java.lang.Float", "java.lang.Long", "java.lang.Integer", "java.lang.Short", "java.lang.Byte"],
        "double" => &[
            "java.lang.Double",
            "java.lang.Float",
            "java.lang.Long",
            "java.lang.Integer",
            "java.lang.Short",
            "java.lang.Byte",
        ],
        _ => &[],
    }
}

/// port of: MethodKey.STRICT_CONVERTIBLES (a primitive formal accepting widened primitives)
fn strict_convertibles(formal: &str) -> &'static [&'static str] {
    match formal {
        "short" => &["byte"],
        "int" => &["short", "byte"],
        "long" => &["int", "short", "byte"],
        "float" => &["long", "int", "short", "byte"],
        "double" => &["float", "long", "int", "short", "byte"],
        _ => &[],
    }
}

// port of: MethodKey.isInvocationConvertible
fn is_invocation_convertible(formal: &JClass, actual: Option<&JClass>, strict: bool, possible_vararg: bool) -> bool {
    if actual.is_none() && !formal.is_primitive() {
        return true;
    }
    if let Some(a) = actual {
        if is_assignable(formal, a) && a.is_array() == formal.is_array() {
            return true;
        }
    }
    if !strict && formal.is_object() {
        return true;
    }
    if let JClass::Prim(p) = formal {
        let list: Vec<&str> = if strict { strict_convertibles(p).to_vec() } else { convertibles(p).to_vec() };
        return match actual {
            Some(a) => list.contains(&a.name().as_str()),
            None => false,
        };
    }
    if possible_vararg {
        if let JClass::Array(comp) = formal {
            let inner = actual.and_then(|a| a.component()).or(actual);
            return is_invocation_convertible(comp, inner, strict, false);
        }
    }
    false
}

fn convertible(formal: &JClass, actual: &JClass, possible_vararg: bool) -> bool {
    let a = if matches!(actual, JClass::Null) { None } else { Some(actual) };
    is_invocation_convertible(formal, a, false, possible_vararg)
}

fn strict_convertible(formal: &JClass, actual: &JClass, possible_vararg: bool) -> bool {
    let a = if matches!(actual, JClass::Null) { None } else { Some(actual) };
    is_invocation_convertible(formal, a, true, possible_vararg)
}

// ---------------------------------------------------------------------------------------------
// The method model: one entry per method a modeled class declares.

type Body = fn(&Value, &[Value]) -> Result<Value, JexlException>;

/// port of: java.lang.reflect.Method restricted to what MethodKey and MethodExecutor read.
pub struct Sig {
    /// Method.getDeclaringClass().getName()
    declaring: &'static str,
    name: &'static str,
    params: &'static [&'static str],
    var_args: bool,
    /// Method.getReturnType().getName()
    returns: &'static str,
    body: Body,
}

impl Sig {
    fn param_classes(&self) -> Vec<JClass> {
        self.params.iter().map(|p| desc(p)).collect()
    }
}

macro_rules! sigs {
    ($decl:literal; $($name:literal ( $($p:literal),* ) $(...)? -> $ret:literal = $body:expr ;)*) => {
        &[$(Sig { declaring: $decl, name: $name, params: &[$($p),*], var_args: false, returns: $ret, body: $body }),*]
    };
}

macro_rules! vsigs {
    ($decl:literal; $($name:literal ( $($p:literal),* ) -> $ret:literal = $body:expr ;)*) => {
        &[$(Sig { declaring: $decl, name: $name, params: &[$($p),*], var_args: true, returns: $ret, body: $body }),*]
    };
}

// ---------------------------------------------------------------------------------------------
// Argument accessors: applicability guarantees the shape, so these are total.

fn arg_jstring(v: &Value) -> JString {
    match v {
        Value::String(s) => s.clone(),
        other => other.java_to_jstring(),
    }
}

fn arg_i64(v: &Value) -> i64 {
    match v {
        Value::Byte(b) => *b as i64,
        Value::Short(s) => *s as i64,
        Value::Integer(i) => *i as i64,
        Value::Long(l) => *l,
        Value::Character(c) => *c as i64,
        Value::Float(f) => d2l(*f as f64),
        Value::Double(d) => d2l(*d),
        _ => 0,
    }
}

/// A Java throwable escaping a JDK call.
fn java_error(class: &str, message: String) -> Result<Value, JexlException> {
    Err(JexlException::java(class, Some(message)))
}

/// Allocating an array longer than the VM allows fails before the heap is even consulted:
/// `OutOfMemoryError: Requested array size exceeds VM limit`. HotSpot's limit is
/// `Integer.MAX_VALUE - 2` (measured on Corretto 25: 2147483645 allocates, 2147483646 does not).
/// Below it an allocation can still fail for want of heap, but that depends on -Xmx, not on JEXL.
fn vm_array(length: i32) -> Result<(), JexlException> {
    if length > i32::MAX - 2 {
        return Err(JexlException::java(
            "java.lang.OutOfMemoryError",
            Some("Requested array size exceeds VM limit".into()),
        ));
    }
    Ok(())
}

/// `HashMap(int initialCapacity, float loadFactor)`'s own checks. The capacity decides the table
/// size and the load factor when it doubles -- both show in the iteration order.
fn capacity_and_load(capacity: &Value, load: Option<&Value>) -> Result<(usize, f32), JexlException> {
    let n = arg_i32(capacity);
    if n < 0 {
        return Err(JexlException::java(
            "java.lang.IllegalArgumentException",
            Some(format!("Illegal initial capacity: {}", n)),
        ));
    }
    let f = load.map(arg_f32).unwrap_or(0.75);
    if f <= 0.0 || f.is_nan() {
        return Err(JexlException::java(
            "java.lang.IllegalArgumentException",
            Some(format!("Illegal load factor: {}", crate::java::number::float_to_string(f))),
        ));
    }
    Ok((n as usize, f))
}

fn arg_i32(v: &Value) -> i32 {
    match v {
        Value::Float(f) => d2i(*f as f64),
        Value::Double(d) => d2i(*d),
        other => arg_i64(other) as i32,
    }
}

/// Java's (int) narrowing of a double: NaN is 0 and the range saturates.
fn d2i(d: f64) -> i32 {
    if d.is_nan() {
        0
    } else if d >= i32::MAX as f64 {
        i32::MAX
    } else if d <= i32::MIN as f64 {
        i32::MIN
    } else {
        d as i32
    }
}

/// Java's (long) narrowing of a double.
fn d2l(d: f64) -> i64 {
    if d.is_nan() {
        0
    } else if d >= i64::MAX as f64 {
        i64::MAX
    } else if d <= i64::MIN as f64 {
        i64::MIN
    } else {
        d as i64
    }
}

fn arg_f64(v: &Value) -> f64 {
    match v {
        Value::Float(f) => *f as f64,
        Value::Double(d) => *d,
        other => arg_i64(other) as f64,
    }
}

fn arg_f32(v: &Value) -> f32 {
    match v {
        Value::Float(f) => *f,
        Value::Double(d) => *d as f32,
        other => arg_i64(other) as f32,
    }
}

fn arg_bool(v: &Value) -> bool {
    matches!(v, Value::Boolean(true))
}

fn arg_char(v: &Value) -> u16 {
    match v {
        Value::Character(c) => *c,
        _ => 0,
    }
}

fn arg_bigint(v: &Value) -> BigInt {
    match v {
        Value::BigInteger(b) => (**b).clone(),
        _ => BigInt::zero(),
    }
}

fn arg_bigdec(v: &Value) -> BigDecimal {
    match v {
        Value::BigDecimal(b) => (**b).clone(),
        _ => BigDecimal::zero(),
    }
}

fn view_of(v: &Value) -> &MapView {
    v.as_host::<MapView>().expect("map view")
}

fn entry_of(v: &Value) -> &MapEntry {
    v.as_host::<MapEntry>().expect("map entry")
}

fn arg_values(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.snapshot(),
        Value::List(l) => l.snapshot(),
        Value::Set(s) => s.snapshot(),
        Value::Object(o) => o.as_collection().map(|(_, e)| e).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The argument must not be null: Java would dereference it (the message the VM attaches is
/// JIT-dependent, see tests/spi_oracle.rs).
fn nn(v: &Value) -> Result<(), JexlException> {
    if v.is_null() {
        return npe0().map(|_: Value| ());
    }
    Ok(())
}

fn int(v: i32) -> Result<Value, JexlException> {
    Ok(Value::Integer(v))
}

fn boolean(v: bool) -> Result<Value, JexlException> {
    Ok(Value::Boolean(v))
}

fn jstring(s: JString) -> Result<Value, JexlException> {
    Ok(Value::String(s))
}

// ---------------------------------------------------------------------------------------------
// Lone surrogates cannot ride through a Rust &str, so the regex-backed String methods swap each
// unpaired surrogate for a plane-15 private-use char and swap it back on the way out.

const PUA: u32 = 0xF0000;

fn to_rust_lossless(s: &JString) -> (String, bool) {
    let u = s.units();
    let mut out = String::with_capacity(u.len());
    let mut mapped = false;
    let mut i = 0;
    while i < u.len() {
        let c = u[i];
        if (0xD800..0xDC00).contains(&c) && i + 1 < u.len() && (0xDC00..0xE000).contains(&u[i + 1]) {
            let cp = 0x10000 + (((c as u32 - 0xD800) << 10) | (u[i + 1] as u32 - 0xDC00));
            out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
            i += 2;
            continue;
        }
        if (0xD800..0xE000).contains(&c) {
            mapped = true;
            out.push(char::from_u32(PUA + c as u32).unwrap_or('\u{FFFD}'));
        } else {
            out.push(char::from_u32(c as u32).unwrap_or('\u{FFFD}'));
        }
        i += 1;
    }
    (out, mapped)
}

fn from_rust_lossless(s: &str, mapped: bool) -> JString {
    if !mapped {
        return JString::from(s);
    }
    let mut out: Vec<u16> = Vec::with_capacity(s.len());
    for c in s.chars() {
        let cp = c as u32;
        if (PUA..PUA + 0x10000).contains(&cp) {
            out.push((cp - PUA) as u16);
        } else {
            let mut buf = [0u16; 2];
            out.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    JString::new(out)
}

fn regex_error<T>(e: regex::JavaRegexError) -> Result<T, JexlException> {
    match e {
        regex::JavaRegexError::Syntax(p) => {
            Err(JexlException::java("java.util.regex.PatternSyntaxException", Some(p.get_message())))
        }
        regex::JavaRegexError::IllegalArgument(m) => Err(JexlException::java("java.lang.IllegalArgumentException", Some(m))),
        regex::JavaRegexError::IndexOutOfBounds(m) => {
            Err(JexlException::java("java.lang.IndexOutOfBoundsException", Some(m)))
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The class map: which declared-method tables a runtime class sees, in Java's walk order.


/// port of: ClassMap.create - the tables visible on a value, most-derived first.
/// The methods of a class named at runtime — what a `java.lang.Class` value stands for.
fn tables_for_class(name: &str) -> Vec<&'static [Sig]> {
    match name {
        "java.lang.String" => vec![STRING, STRING_JOIN_ITERABLE, STRING_STATIC, CHARSEQUENCE, COMPARABLE],
        "java.lang.Character" => vec![CHARACTER, COMPARABLE],
        "java.lang.Boolean" => vec![BOOLEAN, COMPARABLE],
        "java.lang.Byte" => vec![BYTE_S, NUMBER, COMPARABLE],
        "java.lang.Short" => vec![SHORT_S, NUMBER, COMPARABLE],
        "java.lang.Integer" => vec![INTEGER_S, NUMBER, COMPARABLE],
        "java.lang.Long" => vec![LONG_S, NUMBER, COMPARABLE],
        "java.lang.Float" => vec![FLOAT_S, NUMBER, COMPARABLE],
        "java.lang.Double" => vec![DOUBLE_S, NUMBER, COMPARABLE],
        "java.math.BigInteger" => vec![BIGINTEGER, NUMBER, COMPARABLE],
        "java.math.BigDecimal" => vec![BIGDECIMAL, NUMBER, COMPARABLE],
        "java.lang.StringBuilder" => vec![STRINGBUILDER, CHARSEQUENCE],
        _ => vec![],
    }
}

fn tables_for(v: &Value) -> Vec<&'static [Sig]> {
    match v {
        Value::Null => vec![],
        Value::String(_) => vec![STRING, STRING_JOIN_ITERABLE, STRING_STATIC, CHARSEQUENCE, COMPARABLE, OBJECT],
        Value::Character(_) => vec![CHARACTER, COMPARABLE, OBJECT],
        Value::Boolean(_) => vec![BOOLEAN, COMPARABLE, OBJECT],
        Value::Byte(_) => vec![BYTE_S, NUMBER, COMPARABLE, OBJECT],
        Value::Short(_) => vec![SHORT_S, NUMBER, COMPARABLE, OBJECT],
        Value::Integer(_) => vec![INTEGER_S, NUMBER, COMPARABLE, OBJECT],
        Value::Long(_) => vec![LONG_S, NUMBER, COMPARABLE, OBJECT],
        Value::Float(_) => vec![FLOAT_S, NUMBER, COMPARABLE, OBJECT],
        Value::Double(_) => vec![DOUBLE_S, NUMBER, COMPARABLE, OBJECT],
        Value::BigInteger(_) => vec![BIGINTEGER, NUMBER, COMPARABLE, OBJECT],
        Value::BigDecimal(_) => vec![BIGDECIMAL, NUMBER, COMPARABLE, OBJECT],
        Value::List(l) => match l.kind() {
            ListKind::LinkedList => vec![LINKED_LIST, LIST, SEQUENCED, COLLECTION, OBJECT],
            _ => vec![LIST, SEQUENCED, COLLECTION, OBJECT],
        },
        Value::Set(_) => vec![SET, COLLECTION, OBJECT],
        Value::Map(_) => vec![MAP, OBJECT],
        // an array class declares nothing; MethodExecutor.discover falls back to ArrayListWrapper
        Value::Array(_) => vec![OBJECT],
        Value::Object(o) => match o.class_name().as_str() {
            // measured: a Class receiver resolves java.lang.Class's own methods *and* the static
            // methods of the class it stands for (`x.class.getName()` and `x.class.parseInt('42')`
            // both work), with Class's own winning a name clash.
            "java.lang.Class" => {
                let mut tables = vec![CLASS, OBJECT];
                if let Some(c) = o.as_any().downcast_ref::<ClassValue>() {
                    tables.extend(tables_for_class(&c.name));
                }
                tables
            }
            "java.lang.StringBuilder" => vec![STRINGBUILDER, CHARSEQUENCE, OBJECT],
            // IntegerRange / LongRange -- not every class in that package: a Closure lives there too
            _ if o.as_any().is::<Range>() => vec![RANGE, COLLECTION, OBJECT],
            _ => match o.as_any().downcast_ref::<MapView>() {
                Some(view) if view.kind() == ViewKind::Values => vec![MAPVIEW, COLLECTION, OBJECT],
                Some(_) => vec![MAPVIEW, SET, COLLECTION, OBJECT],
                None if o.as_any().is::<MapEntry>() => vec![MAPENTRY, OBJECT],
                None if o.as_any().is::<IndexedContainer>() => vec![INDEXED_CONTAINER, OBJECT],
                // java.util.Iterator for an iterator the shim handed out; any other host object
                // has only Object's methods here (its own come from its HostIntrospector)
                None if o.as_any().is::<JIterator>() => vec![ITERATOR, OBJECT],
                None => vec![OBJECT],
            },
        },
        Value::AtomicBoolean(_) => vec![OBJECT],
    }
}

fn candidates(v: &Value, name: &str) -> Vec<&'static Sig> {
    let mut out: Vec<&'static Sig> = Vec::new();
    for table in tables_for(v) {
        for sig in table {
            if sig.name == name && !out.iter().any(|s| s.params == sig.params) {
                out.push(sig);
            }
        }
    }
    out
}

/// port of: `is.getMethod(ArrayListWrapper.class, key)` - what an array borrows for method calls.
/// The walk order is ArrayListWrapper, AbstractList, List, AbstractCollection, Collection, so the
/// declaring class each name lands on is the one reflection names when it rejects the receiver.
fn array_candidates(name: &str) -> Vec<&'static Sig> {
    let mut out: Vec<&'static Sig> = Vec::new();
    for table in [ARRAY_LIST_WRAPPER, ARRAY_INHERITED, ARRAY_INHERITED_LIST, ARRAY_INHERITED_COLLECTION] {
        for sig in table {
            if sig.name == name && !out.iter().any(|s| s.params == sig.params) {
                out.push(sig);
            }
        }
    }
    out
}

/// port of: the argument check `Method.invoke` runs before the call. Applicability lets a trailing
/// array formal swallow a non-array argument (MethodKey's `possibleVarArg`), but reflection does
/// not, and a non-varargs method then fails with "argument type mismatch".
fn reflect_check(formals: &[JClass], args: &[Value]) -> Result<(), JexlException> {
    if formals.len() != args.len() {
        return iae("wrong number of arguments").map(|_: Value| ());
    }
    for (f, a) in formals.iter().zip(args) {
        let ok = match (f, a) {
            (JClass::Prim(p), _) => !a.is_null() && convertibles(p).contains(&JClass::of(a).name().as_str()),
            (_, Value::Null) => true,
            _ => is_assignable(f, &JClass::of(a)),
        };
        if !ok {
            return iae("argument type mismatch").map(|_: Value| ());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// MethodKey: applicability and the most-specific rule (JLS 15.12.2 as JEXL implements it).

const MORE_SPECIFIC: i32 = 0;
const LESS_SPECIFIC: i32 = 1;
const INCOMPARABLE: i32 = 2;

// port of: MethodKey.Parameters.isApplicable
fn is_applicable(sig: &Sig, actuals: &[JClass]) -> bool {
    let formals = sig.param_classes();
    if formals.len() == actuals.len() {
        for (i, a) in actuals.iter().enumerate() {
            if !convertible(&formals[i], a, false) {
                if i == actuals.len() - 1 && formals[i].is_array() {
                    return convertible(&formals[i], a, true);
                }
                return false;
            }
        }
        return true;
    }
    if !sig.var_args {
        return false;
    }
    if formals.len() > actuals.len() {
        if formals.len() - actuals.len() > 1 {
            return false;
        }
        return actuals.iter().enumerate().all(|(i, a)| convertible(&formals[i], a, false));
    }
    if !formals.is_empty() && !actuals.is_empty() {
        for i in 0..formals.len() - 1 {
            if !convertible(&formals[i], &actuals[i], false) {
                return false;
            }
        }
        let vararg = match formals[formals.len() - 1].component() {
            Some(c) => c.clone(),
            None => return false,
        };
        return actuals[formals.len() - 1..].iter().all(|a| convertible(&vararg, a, false));
    }
    false
}

// port of: MethodKey.Parameters.isPrimitive
fn key_is_primitive(c: &JClass, possible_vararg: bool) -> bool {
    if c.is_primitive() {
        return true;
    }
    if possible_vararg {
        return matches!(c.component(), Some(t) if t.is_primitive());
    }
    false
}

// port of: MethodKey.Parameters.moreSpecific
fn more_specific(a: &[JClass], c1: &[JClass], c2: &[JClass]) -> i32 {
    if c1.len() > a.len() {
        return LESS_SPECIFIC;
    }
    if c2.len() > a.len() {
        return MORE_SPECIFIC;
    }
    if c1.len() > c2.len() {
        return MORE_SPECIFIC;
    }
    if c2.len() > c1.len() {
        return LESS_SPECIFIC;
    }
    let ultimate = c1.len().wrapping_sub(1);
    for i in 0..c1.len() {
        if c1[i] != c2[i] {
            let last = i == ultimate;
            if matches!(a[i], JClass::Null) {
                if c1[i].is_object() && !c2[i].is_object() {
                    return MORE_SPECIFIC;
                }
                if !c1[i].is_object() && c2[i].is_object() {
                    return LESS_SPECIFIC;
                }
            }
            let c1s = key_is_primitive(&c1[i], last);
            let c2s = key_is_primitive(&c2[i], last);
            if c1s != c2s {
                return if c1s == !matches!(a[i], JClass::Null) { MORE_SPECIFIC } else { LESS_SPECIFIC };
            }
            let c1s = strict_convertible(&c2[i], &c1[i], last);
            let c2s = strict_convertible(&c1[i], &c2[i], last);
            if c1s != c2s {
                return if c1s { MORE_SPECIFIC } else { LESS_SPECIFIC };
            }
        }
    }
    INCOMPARABLE
}

/// port of: MethodKey.Parameters.getMostSpecific (an ambiguity resolves to None, like
/// Introspector.getMethod swallowing AmbiguousException).
fn most_specific(args: &[JClass], methods: &[&'static Sig]) -> Option<&'static Sig> {
    let applicable: Vec<&'static Sig> = methods.iter().copied().filter(|m| is_applicable(m, args)).collect();
    if applicable.is_empty() {
        return None;
    }
    if applicable.len() == 1 {
        return Some(applicable[0]);
    }
    let mut maximals: Vec<&'static Sig> = Vec::new();
    for app in applicable {
        let parms = app.param_classes();
        let mut less_specific = false;
        let mut i = 0;
        while !less_specific && i < maximals.len() {
            match more_specific(args, &parms, &maximals[i].param_classes()) {
                MORE_SPECIFIC => {
                    maximals.remove(i);
                }
                LESS_SPECIFIC => less_specific = true,
                _ => i += 1,
            }
        }
        if !less_specific {
            maximals.push(app);
        }
    }
    if maximals.len() > 1 {
        return None;
    }
    maximals.into_iter().next()
}

fn arg_classes(args: &[Value]) -> Vec<JClass> {
    args.iter().map(JClass::of).collect()
}

// ---------------------------------------------------------------------------------------------
// MethodExecutor

/// port of: MethodExecutor
pub struct MethodExec {
    object_class: JClass,
    name: &'static str,
    key: Vec<JClass>,
    sig: &'static Sig,
    /// set when the receiver is an array and the method comes from ArrayListWrapper's map
    wrapped_array: bool,
}

impl MethodExec {
    // port of: MethodExecutor.handleVarArg
    fn handle_var_arg(&self, actual: &[Value]) -> Vec<Value> {
        let formals = self.sig.param_classes();
        let va_start = formals.len() - 1;
        let va_class = match formals[va_start].component() {
            Some(c) => c.clone(),
            None => return actual.to_vec(),
        };
        let mut actual = actual.to_vec();
        let varargc = actual.len() - va_start;
        if varargc == 1 {
            if !actual[va_start].is_null() {
                let aclazz = JClass::of(&actual[va_start]);
                let matches_array = matches!(aclazz.component(), Some(c) if is_assignable(&va_class, c));
                if !aclazz.is_array() || !matches_array {
                    let last = actual[va_start].clone();
                    actual[va_start] = Value::Array(JArray::new(class_component(&va_class), vec![last]));
                }
            }
        } else {
            let tail: Vec<Value> = actual[va_start..].to_vec();
            actual.truncate(va_start);
            actual.push(Value::Array(JArray::new(class_component(&va_class), tail)));
        }
        actual
    }
}

impl JexlMethod for MethodExec {
    fn invoke(&self, obj: &Value, params: &[Value]) -> Result<Value, JexlException> {
        let args = if self.sig.var_args { self.handle_var_arg(params) } else { params.to_vec() };
        if self.wrapped_array && self.sig.declaring != "org.apache.commons.jexl3.internal.introspection.ArrayListWrapper" {
            return iae(&format!(
                "object of type {} is not an instance of {}",
                JClass::of(obj).name(),
                self.sig.declaring
            ));
        }
        reflect_check(&self.sig.param_classes(), &args)?;
        (self.sig.body)(obj, &args)
    }

    // port of: MethodExecutor.tryInvoke
    fn try_invoke(&self, name: &str, obj: &Value, params: &[Value]) -> Result<TryResult, JexlException> {
        if self.object_class == JClass::of(obj) && self.name == name && self.key == arg_classes(params) {
            return self.invoke(obj, params).map(TryResult::Value);
        }
        Ok(TryResult::Failed)
    }

    fn is_cacheable(&self) -> bool {
        true
    }

    fn return_type(&self) -> Option<String> {
        Some(self.sig.returns.to_string())
    }
}

// ---------------------------------------------------------------------------------------------
// The property executors

/// port of: PropertyGetExecutor / BooleanGetExecutor (getFoo() / isFoo())
struct PropertyGet {
    object_class: JClass,
    property: String,
    sig: &'static Sig,
}

impl JexlPropertyGet for PropertyGet {
    fn invoke(&self, obj: &Value) -> Result<Value, JexlException> {
        (self.sig.body)(obj, &[])
    }
    fn try_invoke(&self, obj: &Value, key: &Value) -> Result<TryResult, JexlException> {
        if cast_string(key).as_deref() == Some(self.property.as_str()) && self.object_class == JClass::of(obj) {
            return self.invoke(obj).map(TryResult::Value);
        }
        Ok(TryResult::Failed)
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

/// port of: MapGetExecutor
struct MapGet {
    object_class: JClass,
    property: Value,
}

impl JexlPropertyGet for MapGet {
    fn invoke(&self, obj: &Value) -> Result<Value, JexlException> {
        match obj {
            Value::Map(m) => Ok(m.get(&self.property).unwrap_or(Value::Null)),
            _ => Ok(Value::Null),
        }
    }
    fn try_invoke(&self, obj: &Value, key: &Value) -> Result<TryResult, JexlException> {
        let same_kind = (self.property.is_null() && key.is_null())
            || (!self.property.is_null() && !key.is_null() && JClass::of(&self.property) == JClass::of(key));
        if self.object_class == JClass::of(obj) && same_kind {
            if let Value::Map(m) = obj {
                return Ok(TryResult::Value(m.get(key).unwrap_or(Value::Null)));
            }
        }
        Ok(TryResult::Failed)
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

/// port of: ListGetExecutor (both the array and the List branch)
struct ListGet {
    object_class: JClass,
    index: i32,
}

/// ArrayList checks an element index with Objects.checkIndex, LinkedList with its own message.
fn elem_oob<T>(kind: ListKind, index: i64, len: usize) -> Result<T, JexlException> {
    match kind {
        ListKind::LinkedList => jthrow(
            "java.lang.IndexOutOfBoundsException",
            &format!("Index: {}, Size: {}", index, len),
        ),
        _ => ioobe(index, len),
    }
}

fn list_get(obj: &Value, index: i32) -> Result<Value, JexlException> {
    match obj {
        Value::Array(a) => match usize::try_from(index).ok().and_then(|i| a.get(i)) {
            Some(v) => Ok(v),
            None => Err(JexlException::java("java.lang.ArrayIndexOutOfBoundsException", None)),
        },
        Value::List(l) => match usize::try_from(index).ok().and_then(|i| l.get(i)) {
            Some(v) => Ok(v),
            None => elem_oob(l.kind(), index as i64, l.len()),
        },
        _ => Ok(Value::Null),
    }
}

impl JexlPropertyGet for ListGet {
    fn invoke(&self, obj: &Value) -> Result<Value, JexlException> {
        list_get(obj, self.index)
    }
    fn try_invoke(&self, obj: &Value, identifier: &Value) -> Result<TryResult, JexlException> {
        match cast_integer(identifier) {
            Some(i) if self.object_class == JClass::of(obj) => list_get(obj, i).map(TryResult::Value),
            _ => Ok(TryResult::Failed),
        }
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

/// port of: DuckGetExecutor (`get(identifier)`)
struct DuckGet {
    object_class: JClass,
    property: Value,
    sig: &'static Sig,
}

impl JexlPropertyGet for DuckGet {
    fn invoke(&self, obj: &Value) -> Result<Value, JexlException> {
        (self.sig.body)(obj, std::slice::from_ref(&self.property))
    }
    fn try_invoke(&self, obj: &Value, key: &Value) -> Result<TryResult, JexlException> {
        let same = (self.property.is_null() && key.is_null()) || self.property.java_equals(key);
        if self.object_class == JClass::of(obj) && same {
            return self.invoke(obj).map(TryResult::Value);
        }
        Ok(TryResult::Failed)
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

/// port of: FieldGetExecutor (a public field, which for the JDK types means a static constant)
struct FieldGet {
    object_class: JClass,
    name: String,
    value: Value,
}

impl JexlPropertyGet for FieldGet {
    fn invoke(&self, _obj: &Value) -> Result<Value, JexlException> {
        Ok(self.value.clone())
    }
    fn try_invoke(&self, obj: &Value, key: &Value) -> Result<TryResult, JexlException> {
        if JClass::of(obj) == self.object_class && cast_string(key).as_deref() == Some(self.name.as_str()) {
            return Ok(TryResult::Value(self.value.clone()));
        }
        Ok(TryResult::Failed)
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

/// port of: PropertySetExecutor (setFoo(value))
struct PropertySet {
    object_class: JClass,
    property: String,
    value_class: JClass,
    sig: &'static Sig,
}

impl JexlPropertySet for PropertySet {
    fn invoke(&self, obj: &Value, arg: &Value) -> Result<Value, JexlException> {
        (self.sig.body)(obj, std::slice::from_ref(arg))?;
        Ok(arg.clone())
    }
    fn try_invoke(&self, obj: &Value, key: &Value, value: &Value) -> Result<TryResult, JexlException> {
        if cast_string(key).as_deref() == Some(self.property.as_str())
            && self.object_class == JClass::of(obj)
            && self.value_class == class_of_arg(value)
        {
            return self.invoke(obj, value).map(TryResult::Value);
        }
        Ok(TryResult::Failed)
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

/// port of: MapSetExecutor
struct MapSet {
    object_class: JClass,
    property: Value,
    value_class: JClass,
}

impl JexlPropertySet for MapSet {
    fn invoke(&self, obj: &Value, arg: &Value) -> Result<Value, JexlException> {
        map_put(obj, self.property.clone(), arg.clone())?;
        Ok(arg.clone())
    }
    fn try_invoke(&self, obj: &Value, key: &Value, value: &Value) -> Result<TryResult, JexlException> {
        let same_kind = (self.property.is_null() && key.is_null())
            || (!self.property.is_null() && !key.is_null() && JClass::of(&self.property) == JClass::of(key));
        if self.object_class == JClass::of(obj) && same_kind && self.value_class == class_of_arg(value) {
            map_put(obj, key.clone(), value.clone())?;
            return Ok(TryResult::Value(value.clone()));
        }
        Ok(TryResult::Failed)
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

/// port of: ListSetExecutor (both the array and the List branch)
struct ListSet {
    object_class: JClass,
    index: i32,
}

fn list_set(obj: &Value, index: i32, value: &Value) -> Result<Value, JexlException> {
    match obj {
        Value::Array(a) => {
            // Array.set unwraps the value for a primitive component before it checks the index
            if value.is_null() && a.component.is_primitive() {
                return Err(JexlException::java("java.lang.IllegalArgumentException", None));
            }
            let i = match usize::try_from(index) {
                Ok(i) if i < a.len() => i,
                _ => return Err(JexlException::java("java.lang.ArrayIndexOutOfBoundsException", None)),
            };
            let coerced = coerce_array_element(&a.component, value)?;
            a.set(i, coerced);
            Ok(value.clone())
        }
        Value::List(l) => {
            let mut w = l.write();
            match usize::try_from(index) {
                Ok(i) if i < w.items.len() => {
                    w.items[i] = value.clone();
                    Ok(value.clone())
                }
                _ => elem_oob(w.kind, index as i64, w.items.len()),
            }
        }
        _ => Ok(value.clone()),
    }
}

/// port of: java.lang.reflect.Array.set - the widening it accepts and the two ways it fails.
fn coerce_array_element(component: &Component, value: &Value) -> Result<Value, JexlException> {
    let reference = matches!(component, Component::Class(_));
    let bad = || {
        if reference {
            iae::<Value>("array element type mismatch")
        } else if value.is_null() {
            Err(JexlException::java("java.lang.IllegalArgumentException", None))
        } else {
            iae::<Value>("argument type mismatch")
        }
    };
    match component {
        Component::Class(n) => {
            if value.is_null() || is_assignable(&JClass::named(n), &JClass::of(value)) {
                Ok(value.clone())
            } else {
                bad()
            }
        }
        Component::Boolean => match value {
            Value::Boolean(_) => Ok(value.clone()),
            _ => bad(),
        },
        Component::Char => match value {
            Value::Character(_) => Ok(value.clone()),
            _ => bad(),
        },
        _ => {
            // Array.set applies JLS widening, which (unlike MethodKey's table) accepts a char
            let widens: &[&str] = match component {
                Component::Byte => &["java.lang.Byte"],
                Component::Short => &["java.lang.Byte", "java.lang.Short"],
                Component::Int => &["java.lang.Byte", "java.lang.Short", "java.lang.Character", "java.lang.Integer"],
                Component::Long => &[
                    "java.lang.Byte",
                    "java.lang.Short",
                    "java.lang.Character",
                    "java.lang.Integer",
                    "java.lang.Long",
                ],
                Component::Float => &[
                    "java.lang.Byte",
                    "java.lang.Short",
                    "java.lang.Character",
                    "java.lang.Integer",
                    "java.lang.Long",
                    "java.lang.Float",
                ],
                Component::Double => &[
                    "java.lang.Byte",
                    "java.lang.Short",
                    "java.lang.Character",
                    "java.lang.Integer",
                    "java.lang.Long",
                    "java.lang.Float",
                    "java.lang.Double",
                ],
                _ => return bad(),
            };
            if !widens.contains(&JClass::of(value).name().as_str()) {
                return bad();
            }
            Ok(match component {
                Component::Byte => Value::Byte(arg_i64(value) as i8),
                Component::Short => Value::Short(arg_i64(value) as i16),
                Component::Int => Value::Integer(arg_i32(value)),
                Component::Long => Value::Long(arg_i64(value)),
                Component::Float => Value::Float(arg_f32(value)),
                _ => Value::Double(arg_f64(value)),
            })
        }
    }
}

impl JexlPropertySet for ListSet {
    fn invoke(&self, obj: &Value, arg: &Value) -> Result<Value, JexlException> {
        list_set(obj, self.index, arg)
    }
    fn try_invoke(&self, obj: &Value, key: &Value, value: &Value) -> Result<TryResult, JexlException> {
        match cast_integer(key) {
            Some(i) if self.object_class == JClass::of(obj) => list_set(obj, i, value).map(TryResult::Value),
            _ => Ok(TryResult::Failed),
        }
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

/// port of: DuckSetExecutor (`set(key, value)` then `put(key, value)`)
struct DuckSet {
    object_class: JClass,
    property: Value,
    value_class: JClass,
    sig: &'static Sig,
}

impl JexlPropertySet for DuckSet {
    fn invoke(&self, obj: &Value, arg: &Value) -> Result<Value, JexlException> {
        (self.sig.body)(obj, &[self.property.clone(), arg.clone()])?;
        Ok(arg.clone())
    }
    fn try_invoke(&self, obj: &Value, key: &Value, value: &Value) -> Result<TryResult, JexlException> {
        let same = (self.property.is_null() && key.is_null()) || self.property.java_equals(key);
        if self.object_class == JClass::of(obj) && same && self.value_class == class_of_arg(value) {
            return self.invoke(obj, value).map(TryResult::Value);
        }
        Ok(TryResult::Failed)
    }
    fn is_cacheable(&self) -> bool {
        true
    }
}

// port of: AbstractExecutor.classOf
fn class_of_arg(v: &Value) -> JClass {
    match v {
        Value::Null => JClass::named("java.lang.Object"),
        other => JClass::of(other),
    }
}

// port of: AbstractExecutor.castString
fn cast_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.to_rust()),
        Value::Integer(i) => Some(i.to_string()),
        Value::Object(o) if o.class_name() == "java.lang.StringBuilder" => Some(v.java_to_string()),
        _ => None,
    }
}

// port of: AbstractExecutor.castInteger
fn cast_integer(v: &Value) -> Option<i32> {
    match v {
        Value::Byte(b) => Some(*b as i32),
        Value::Short(s) => Some(*s as i32),
        Value::Integer(i) => Some(*i),
        Value::Long(l) => Some(*l as i32),
        Value::Float(f) => Some(d2i(*f as f64)),
        Value::Double(d) => Some(d2i(*d)),
        Value::BigInteger(b) => Some(number::big_integer_int_value(b)),
        Value::BigDecimal(b) => Some(b.int_value()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// The uberspect

/// The extension point for host objects: a user type registers its methods and properties here
/// and the shim defers to it before failing. The orchestrator wires the engine's registry in.
pub trait HostIntrospector: Send + Sync {
    fn get_method(&self, obj: &Value, name: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>>;
    fn get_property_get(&self, obj: &Value, identifier: &Value) -> Option<Arc<dyn JexlPropertyGet>>;
    fn get_property_set(&self, obj: &Value, identifier: &Value, arg: &Value) -> Option<Arc<dyn JexlPropertySet>>;
    fn get_constructor(&self, handle: &Value, args: &[Value]) -> Option<Arc<dyn JexlMethod>>;
}

/// port of: org.apache.commons.jexl3.internal.introspection.Uberspect, over the modeled JDK types.
pub struct JdkShim {
    strategy: ResolverStrategy,
    hosts: Option<Arc<dyn HostIntrospector>>,
}

impl Default for JdkShim {
    fn default() -> Self {
        JdkShim::new(ResolverStrategy::Jexl)
    }
}

impl JdkShim {
    pub fn new(strategy: ResolverStrategy) -> JdkShim {
        JdkShim { strategy, hosts: None }
    }

    /// Registers the host-object introspector; it is consulted before the JDK tables fail.
    pub fn with_hosts(mut self, hosts: Arc<dyn HostIntrospector>) -> JdkShim {
        self.hosts = Some(hosts);
        self
    }

    /// port of: the in-place half of MethodExecutor.handleVarArg.
    ///
    /// Java packs a single trailing vararg into the very array the caller passed, so anything that
    /// reads those arguments afterwards - notably `JexlException.methodSignature` - sees the packed
    /// array. `JexlMethod::invoke` only borrows the arguments, so a caller that will report them on
    /// failure must run this first. Calling it twice is a no-op, exactly as in Java.
    pub fn pack_varargs(&self, obj: &Value, method: &str, args: &mut [Value]) {
        if obj.is_null() || args.is_empty() {
            return;
        }
        let key = arg_classes(args);
        let sig = most_specific(&key, &candidates(obj, method));
        let sig = match sig {
            Some(sig) if sig.var_args => sig,
            _ => return,
        };
        let formals = sig.param_classes();
        let va_start = formals.len() - 1;
        if args.len() - va_start != 1 || args[va_start].is_null() {
            return;
        }
        let va_class = match formals[va_start].component() {
            Some(c) => c.clone(),
            None => return,
        };
        let actual = JClass::of(&args[va_start]);
        let already = matches!(actual.component(), Some(c) if is_assignable(&va_class, c));
        if !actual.is_array() || !already {
            let last = args[va_start].clone();
            args[va_start] = Value::Array(JArray::new(class_component(&va_class), vec![last]));
        }
    }
}

/// The public `get*` methods JEXL's IndexedType can turn into an indexed property, per class:
/// measured with JEXL's own Introspector (getMethodNames) on Corretto 25, keeping the names that have
/// no no-argument getX()/isX() -- those let the PROPERTY resolver answer first. Maps are left out
/// (the MAP resolver answers every property of a map before CONTAINER is asked), and so are
/// java.lang.Class and AtomicBoolean, whose getters the shim does not model.
fn indexed_getters(class: &str) -> &'static [&'static str] {
    match class {
        "java.lang.String" | "java.lang.StringBuilder" => &["getChars"],
        "java.lang.Character" => &["getDirectionality", "getName", "getNumericValue", "getType"],
        "java.lang.Boolean" => &["getBoolean"],
        "java.lang.Integer" => &["getInteger"],
        "java.lang.Long" => &["getLong"],
        _ => &[],
    }
}

/// port of: IndexedType.discover -- `name.substring(0, 1).toUpperCase() + name.substring(1)`
fn discover_container(claz: &JClass, property: &str) -> Option<Arc<dyn JexlPropertyGet>> {
    let mut chars = property.chars();
    let first = chars.next()?;
    let getter = format!("get{}{}", first.to_uppercase(), chars.as_str());
    let getter = *indexed_getters(&claz.name()).iter().find(|g| **g == getter)?;
    Some(Arc::new(ContainerGet { getter, container: property.to_string() }))
}

/// port of: IndexedType as a JexlPropertyGet -- invoking it hands out the container
struct ContainerGet {
    getter: &'static str,
    container: String,
}

impl JexlPropertyGet for ContainerGet {
    fn invoke(&self, obj: &Value) -> Result<Value, JexlException> {
        Ok(Value::object(IndexedContainer { object: obj.clone(), getter: self.getter, container: self.container.clone() }))
    }
}

/// port of: IndexedType.IndexedContainer -- `c.name` where c's class has a getName(...) but no
/// getName(): an object whose get(key) calls that getter with the key.
pub struct IndexedContainer {
    object: Value,
    getter: &'static str,
    container: String,
}

impl crate::value::HostObject for IndexedContainer {
    fn class_name(&self) -> String {
        "org.apache.commons.jexl3.internal.introspection.IndexedType$IndexedContainer".into()
    }
    // Object.toString: an identity hash, as in Java
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn container_of(v: &Value) -> &IndexedContainer {
    v.as_host::<IndexedContainer>().expect("indexed container")
}

/// port of: IndexedType.invokeGet / invokeSet's failure: `"property get error: " +
/// object.getClass().toString() + "@" + key.toString()` -- and a null key fails on the toString.
fn container_error(what: &str, object: &Value, key: &Value) -> Result<Value, JexlException> {
    if key.is_null() {
        return Err(JexlException::java(
            "java.lang.NullPointerException",
            Some("Cannot invoke \"Object.toString()\" because \"key\" is null".into()),
        ));
    }
    Err(JexlException::java(
        "java.beans.IntrospectionException",
        Some(format!("property {} error: class {}@{}", what, object.class_name(), key.java_to_string())),
    ))
}

const INDEXED_CONTAINER: &[Sig] = sigs!("org.apache.commons.jexl3.internal.introspection.IndexedType$IndexedContainer";
    // the most specific getter for (key), as IndexedType picks it with a MethodKey
    "get"("java.lang.Object") -> "java.lang.Object" = |o, a| {
        let c = container_of(o);
        match most_specific(&arg_classes(&a[..1]), &candidates(&c.object, c.getter)) {
            Some(sig) => (sig.body)(&c.object, &a[..1]),
            None => container_error("get", &c.object, &a[0]),
        }
    };
    // none of the modeled classes has a matching setter
    "set"("java.lang.Object", "java.lang.Object") -> "java.lang.Object" =
        |o, a| container_error("set", &container_of(o).object, &a[0]);
    "getContainerName"() -> "java.lang.String" = |o, _| Ok(Value::string(&container_of(o).container));
    "getContainerClass"() -> "java.lang.Class" = |o, _| Ok(ClassValue::of(&container_of(o).object.class_name()));
);

/// port of: the class loader behind JexlUberspect.getClassLoader() — the classes this shim models.
pub fn load_class(name: &str) -> Option<Value> {
    if !tables_for_class(name).is_empty() || CTORS.iter().any(|c| c.name == name) {
        return Some(ClassValue::of(name));
    }
    None
}

impl JexlUberspect for JdkShim {
    fn load_class(&self, name: &str) -> Option<Value> {
        load_class(name)
    }

    fn get_resolvers(&self, op: Option<JexlOperator>, obj: &Value) -> &'static [PropertyResolver] {
        self.strategy.apply(op, obj)
    }

    // port of: MethodExecutor.discover
    fn get_method(&self, obj: &Value, method: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        if obj.is_null() {
            return None;
        }
        let key = arg_classes(args);
        let object_class = JClass::of(obj);
        let mut wrapped_array = false;
        let mut sig = most_specific(&key, &candidates(obj, method));
        if sig.is_none() && object_class.is_array() {
            sig = most_specific(&key, &array_candidates(method));
            wrapped_array = sig.is_some();
        }
        match sig {
            Some(sig) => Some(Arc::new(MethodExec {
                object_class,
                name: sig.name,
                key,
                sig,
                wrapped_array,
            })),
            None => self.hosts.as_ref().and_then(|h| h.get_method(obj, method, args)),
        }
    }

    // port of: Uberspect.getPropertyGet(List, Object, Object)
    fn get_property_get_with(
        &self,
        resolvers: &[PropertyResolver],
        obj: &Value,
        identifier: &Value,
    ) -> Option<Arc<dyn JexlPropertyGet>> {
        let claz = JClass::of(obj);
        let property = cast_string(identifier);
        for resolver in resolvers {
            let executor: Option<Arc<dyn JexlPropertyGet>> = match resolver {
                PropertyResolver::Property => property
                    .as_deref()
                    .and_then(|p| discover_get(obj, &claz, "get", p))
                    .or_else(|| property.as_deref().and_then(|p| discover_boolean_get(obj, &claz, p))),
                PropertyResolver::Map => match obj {
                    Value::Map(_) => Some(Arc::new(MapGet { object_class: claz.clone(), property: identifier.clone() })),
                    _ => None,
                },
                PropertyResolver::List => match (cast_integer(identifier), obj) {
                    (Some(index), Value::Array(_)) | (Some(index), Value::List(_)) => {
                        Some(Arc::new(ListGet { object_class: claz.clone(), index }))
                    }
                    _ => None,
                },
                PropertyResolver::Duck => discover_duck_get(obj, &claz, identifier).or_else(|| {
                    match (&property, identifier) {
                        (Some(p), Value::String(_)) => {
                            let _ = p;
                            None
                        }
                        (Some(p), _) => discover_duck_get(obj, &claz, &Value::string(p)),
                        _ => None,
                    }
                }),
                PropertyResolver::Field => property.as_deref().and_then(|p| discover_field(obj, &claz, p)),
                // port of: IndexedType.discover -- a getter is enough, the setter is optional
                PropertyResolver::Container => property.as_deref().and_then(|p| discover_container(&claz, p)),
            };
            if executor.is_some() {
                return executor;
            }
        }
        self.hosts.as_ref().and_then(|h| h.get_property_get(obj, identifier))
    }

    // port of: Uberspect.getPropertySet(List, Object, Object, Object)
    fn get_property_set_with(
        &self,
        resolvers: &[PropertyResolver],
        obj: &Value,
        identifier: &Value,
        arg: &Value,
    ) -> Option<Arc<dyn JexlPropertySet>> {
        let claz = JClass::of(obj);
        let property = cast_string(identifier);
        for resolver in resolvers {
            let executor: Option<Arc<dyn JexlPropertySet>> = match resolver {
                PropertyResolver::Property => {
                    property.as_deref().filter(|p| !p.is_empty()).and_then(|p| discover_set(obj, &claz, p, arg))
                }
                PropertyResolver::Map => match obj {
                    Value::Map(_) => Some(Arc::new(MapSet {
                        object_class: claz.clone(),
                        property: identifier.clone(),
                        value_class: class_of_arg(arg),
                    })),
                    _ => None,
                },
                PropertyResolver::List => match (cast_integer(identifier), obj) {
                    (Some(index), Value::Array(_)) | (Some(index), Value::List(_)) => {
                        Some(Arc::new(ListSet { object_class: claz.clone(), index }))
                    }
                    _ => None,
                },
                PropertyResolver::Duck => discover_duck_set(obj, &claz, identifier, arg).or_else(|| {
                    match (&property, identifier) {
                        (Some(_), Value::String(_)) => None,
                        (Some(p), _) => discover_duck_set(obj, &claz, &Value::string(p), arg),
                        _ => None,
                    }
                }),
                PropertyResolver::Field => None,
                PropertyResolver::Container => None,
            };
            if executor.is_some() {
                return executor;
            }
        }
        self.hosts.as_ref().and_then(|h| h.get_property_set(obj, identifier, arg))
    }

    // port of: Uberspect.getIterator
    fn get_iterator(&self, obj: &Value) -> Option<Box<dyn Iterator<Item = Value> + Send>> {
        match obj {
            Value::Object(o) => {
                if let Some(it) = o.as_any().downcast_ref::<JIterator>() {
                    let mut rest: Vec<Value> = Vec::new();
                    while let Some(v) = it.next() {
                        rest.push(v);
                    }
                    return Some(Box::new(rest.into_iter()));
                }
                if let Some(r) = o.as_any().downcast_ref::<Range>() {
                    // lazily, like Java: a range whose max is the type's MAX_VALUE never ends
                    return Some(range_iterator(r));
                }
                o.as_collection().map(|(_, e)| Box::new(e.into_iter()) as Box<dyn Iterator<Item = Value> + Send>)
            }
            Value::Array(a) => Some(Box::new(a.snapshot().into_iter())),
            // Java iterates a Map's values
            Value::Map(m) => Some(Box::new(m.snapshot().into_iter().map(|(_, v)| v))),
            Value::List(l) => Some(Box::new(l.snapshot().into_iter())),
            Value::Set(s) => Some(Box::new(s.snapshot().into_iter())),
            _ => None,
        }
    }

    // port of: ConstructorMethod.discover
    fn get_constructor(&self, ctor_handle: &Value, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        if ctor_handle.is_null() {
            return None;
        }
        let class_name = match ctor_handle.as_host::<ClassValue>() {
            Some(c) => c.name.clone(),
            None => ctor_handle.java_to_string(),
        };
        let table: Vec<&'static Sig> = CTORS.iter().filter(|s| s.name == class_name).collect();
        if table.is_empty() {
            return self.hosts.as_ref().and_then(|h| h.get_constructor(ctor_handle, args));
        }
        let key = arg_classes(args);
        match most_specific(&key, &table) {
            Some(sig) => Some(Arc::new(MethodExec {
                object_class: JClass::named(&class_name),
                name: sig.name,
                key,
                sig,
                wrapped_array: false,
            })),
            None => None,
        }
    }
}

/// port of: PropertyGetExecutor.discoverGet's name building - `getFoo` then `getfoo`.
fn cased_names(which: &str, property: &str) -> Vec<String> {
    if property.is_empty() {
        return Vec::new();
    }
    let mut chars: Vec<char> = property.chars().collect();
    let first = chars[0];
    let mut out = Vec::with_capacity(2);
    for cased in [first.to_uppercase().next().unwrap_or(first), first.to_lowercase().next().unwrap_or(first)] {
        chars[0] = cased;
        out.push(format!("{}{}", which, chars.iter().collect::<String>()));
    }
    out
}

// port of: PropertyGetExecutor.discoverGet
fn discover_get(obj: &Value, claz: &JClass, which: &str, property: &str) -> Option<Arc<dyn JexlPropertyGet>> {
    for name in cased_names(which, property) {
        if let Some(sig) = most_specific(&[], &candidates(obj, &name)) {
            return Some(Arc::new(PropertyGet { object_class: claz.clone(), property: property.to_string(), sig }));
        }
    }
    None
}

// port of: BooleanGetExecutor.discover - an isFoo() that actually returns a boolean
fn discover_boolean_get(obj: &Value, claz: &JClass, property: &str) -> Option<Arc<dyn JexlPropertyGet>> {
    let sig = boolean_sig(obj, property)?;
    Some(Arc::new(PropertyGet { object_class: claz.clone(), property: property.to_string(), sig }))
}

fn boolean_sig(obj: &Value, property: &str) -> Option<&'static Sig> {
    for name in cased_names("is", property) {
        if let Some(sig) = most_specific(&[], &candidates(obj, &name)) {
            return (sig.returns == "boolean" || sig.returns == "java.lang.Boolean").then_some(sig);
        }
    }
    None
}

// port of: DuckGetExecutor.discover
fn discover_duck_get(obj: &Value, claz: &JClass, identifier: &Value) -> Option<Arc<dyn JexlPropertyGet>> {
    let sig = most_specific(&[JClass::of(identifier)], &candidates(obj, "get"))?;
    Some(Arc::new(DuckGet { object_class: claz.clone(), property: identifier.clone(), sig }))
}

// port of: PropertySetExecutor.discoverSet
fn discover_set(obj: &Value, claz: &JClass, property: &str, arg: &Value) -> Option<Arc<dyn JexlPropertySet>> {
    for name in cased_names("set", property) {
        if let Some(sig) = most_specific(&[JClass::of(arg)], &candidates(obj, &name)) {
            return Some(Arc::new(PropertySet {
                object_class: claz.clone(),
                property: property.to_string(),
                value_class: class_of_arg(arg),
                sig,
            }));
        }
    }
    None
}

// port of: DuckSetExecutor.discover
fn discover_duck_set(obj: &Value, claz: &JClass, key: &Value, value: &Value) -> Option<Arc<dyn JexlPropertySet>> {
    let args = [JClass::of(key), JClass::of(value)];
    let sig = most_specific(&args, &candidates(obj, "set")).or_else(|| most_specific(&args, &candidates(obj, "put")))?;
    Some(Arc::new(DuckSet {
        object_class: claz.clone(),
        property: key.clone(),
        value_class: class_of_arg(value),
        sig,
    }))
}

// port of: FieldGetExecutor.discover (the JDK's public fields are its static constants)
fn discover_field(_obj: &Value, claz: &JClass, name: &str) -> Option<Arc<dyn JexlPropertyGet>> {
    let value = static_field(&claz.name(), name)?;
    Some(Arc::new(FieldGet { object_class: claz.clone(), name: name.to_string(), value }))
}

// ---------------------------------------------------------------------------------------------
// Character helpers (java.lang.Character, on UTF-16 code units)

fn to_char(c: u16) -> Option<char> {
    char::from_u32(c as u32)
}

/// Character.toUpperCase(char): the 1:1 UnicodeData mapping only.
fn char_upper(c: u16) -> u16 {
    match to_char(c) {
        Some(ch) => {
            let mut it = ch.to_uppercase();
            match (it.next(), it.next()) {
                (Some(u), None) if (u as u32) <= 0xFFFF => u as u16,
                _ => c,
            }
        }
        None => c,
    }
}

fn char_lower(c: u16) -> u16 {
    match to_char(c) {
        Some(ch) => {
            let mut it = ch.to_lowercase();
            match (it.next(), it.next()) {
                (Some(u), None) if (u as u32) <= 0xFFFF => u as u16,
                _ => c,
            }
        }
        None => c,
    }
}

/// Character.isWhitespace: Unicode space minus the non-breaking ones.
fn char_is_whitespace(c: i32) -> bool {
    match cp_char(c) {
        Some('\u{00A0}') | Some('\u{2007}') | Some('\u{202F}') => false,
        Some(ch) => ch.is_whitespace(),
        None => false,
    }
}

/// Character.isSpaceChar: the Zs, Zl and Zp categories.
fn char_is_space(c: i32) -> bool {
    matches!(
        cp_char(c),
        Some(' ')
            | Some('\u{00A0}')
            | Some('\u{1680}')
            | Some('\u{2000}'..='\u{200A}')
            | Some('\u{2028}')
            | Some('\u{2029}')
            | Some('\u{202F}')
            | Some('\u{205F}')
            | Some('\u{3000}')
    )
}

fn cp_char(cp: i32) -> Option<char> {
    u32::try_from(cp).ok().and_then(char::from_u32)
}

fn char_is_digit(c: i32) -> bool {
    matches!(cp_char(c), Some(ch) if ch.is_ascii_digit())
}

fn char_is_letter(c: i32) -> bool {
    matches!(cp_char(c), Some(ch) if ch.is_alphabetic())
}

// port of: Character.digit
fn char_digit(c: i32, radix: i32) -> i32 {
    if !(2..=36).contains(&radix) || !(0..=0xFFFF).contains(&c) {
        return -1;
    }
    number::char_digit(c as u16, radix as u32)
}

// port of: Character.getNumericValue (the decimal-digit and Latin-letter part)
fn char_numeric_value(c: i32) -> i32 {
    match cp_char(c) {
        Some(ch) if ch.is_ascii_digit() => ch as i32 - '0' as i32,
        Some(ch) if ch.is_ascii_alphabetic() => ch.to_ascii_lowercase() as i32 - 'a' as i32 + 10,
        _ => -1,
    }
}

// port of: Character.forDigit
fn char_for_digit(digit: i32, radix: i32) -> u16 {
    if !(2..=36).contains(&radix) || digit < 0 || digit >= radix {
        return 0;
    }
    if digit < 10 {
        b'0' as u16 + digit as u16
    } else {
        b'a' as u16 + (digit - 10) as u16
    }
}

// ---------------------------------------------------------------------------------------------
// String helpers

fn s_len(o: &Value) -> usize {
    arg_jstring(o).len()
}

fn check_index(i: i64, len: usize) -> Result<usize, JexlException> {
    if i < 0 || i >= len as i64 {
        return sioobe_index(i, len);
    }
    Ok(i as usize)
}

fn check_range(b: i64, e: i64, len: usize) -> Result<(usize, usize), JexlException> {
    if b < 0 || b > e || e > len as i64 {
        return sioobe_range(b, e, len);
    }
    Ok((b as usize, e as usize))
}

fn eq_ignore_case(a: u16, b: u16) -> bool {
    a == b || {
        let (ua, ub) = (char_upper(a), char_upper(b));
        ua == ub || char_lower(ua) == char_lower(ub)
    }
}

/// A String is LATIN1-coded when every unit fits in a byte; the case-insensitive comparisons
/// branch on the coder, and only the UTF16/UTF16 pair folds whole code points.
fn is_latin1(u: &[u16]) -> bool {
    u.iter().all(|c| *c <= 0xFF)
}

// port of: StringUTF16.compareCodePointCI
fn compare_code_point_ci(cp1: i32, cp2: i32) -> i32 {
    let (u1, u2) = (cp_upper(cp1), cp_upper(cp2));
    if u1 != u2 {
        let (l1, l2) = (cp_lower(u1), cp_lower(u2));
        if l1 != l2 {
            return l1 - l2;
        }
    }
    0
}

// port of: StringUTF16.codePointIncluding (a negative result means the pair consumed the next unit)
fn code_point_including(ba: &[u16], cp: i32, index: usize, start: usize, end: usize) -> i32 {
    let c = cp as u16;
    if !(0xD800..0xE000).contains(&c) {
        return cp;
    }
    if (0xDC00..0xE000).contains(&c) {
        if index > start {
            let p = ba[index - 1];
            if (0xD800..0xDC00).contains(&p) {
                return 0x10000 + (((p as i32 - 0xD800) << 10) | (c as i32 - 0xDC00));
            }
        }
    } else if index + 1 < end {
        let n = ba[index + 1];
        if (0xDC00..0xE000).contains(&n) {
            return -(0x10000 + (((c as i32 - 0xD800) << 10) | (n as i32 - 0xDC00)));
        }
    }
    cp
}

// port of: StringUTF16.compareToCIImpl
fn utf16_ci(v: &[u16], toff: usize, tlen: usize, o: &[u16], ooff: usize, olen: usize) -> i32 {
    let (tlast, olast) = (toff + tlen, ooff + olen);
    let (mut k1, mut k2) = (toff, ooff);
    while k1 < tlast && k2 < olast {
        let mut cp1 = v[k1] as i32;
        let mut cp2 = o[k2] as i32;
        if cp1 == cp2 || compare_code_point_ci(cp1, cp2) == 0 {
            k1 += 1;
            k2 += 1;
            continue;
        }
        cp1 = code_point_including(v, cp1, k1, toff, tlast);
        if cp1 < 0 {
            k1 += 1;
            cp1 = -cp1;
        }
        cp2 = code_point_including(o, cp2, k2, ooff, olast);
        if cp2 < 0 {
            k2 += 1;
            cp2 = -cp2;
        }
        let diff = compare_code_point_ci(cp1, cp2);
        if diff != 0 {
            return diff;
        }
        k1 += 1;
        k2 += 1;
    }
    tlen as i32 - olen as i32
}

// port of: StringLatin1.compareToCI / compareToCI_UTF16 (both are this unit-wise loop)
fn latin1_ci(a: &[u16], b: &[u16]) -> i32 {
    for k in 0..a.len().min(b.len()) {
        let (mut c1, mut c2) = (a[k], b[k]);
        if c1 != c2 {
            c1 = char_upper(c1);
            c2 = char_upper(c2);
            if c1 != c2 {
                c1 = char_lower(c1);
                c2 = char_lower(c2);
                if c1 != c2 {
                    return c1 as i32 - c2 as i32;
                }
            }
        }
    }
    a.len() as i32 - b.len() as i32
}

/// port of: String.CASE_INSENSITIVE_ORDER.compare
fn compare_ignore_case(a: &JString, b: &JString) -> i32 {
    let (x, y) = (a.units(), b.units());
    match (is_latin1(x), is_latin1(y)) {
        (false, false) => utf16_ci(x, 0, x.len(), y, 0, y.len()),
        (true, _) => latin1_ci(x, y),
        (false, true) => -latin1_ci(y, x),
    }
}

/// port of: String.regionMatches(true, ...)
fn region_matches_ci(a: &[u16], toff: usize, b: &[u16], ooff: usize, len: usize) -> bool {
    if !is_latin1(a) && !is_latin1(b) {
        return utf16_ci(a, toff, len, b, ooff, len) == 0;
    }
    (0..len).all(|i| eq_ignore_case(a[toff + i], b[ooff + i]))
}

/// String.indexOf(int ch, int from) over code points.
fn index_of_cp(s: &JString, cp: i32, from: i32) -> i32 {
    let u = s.units();
    let start = from.max(0) as usize;
    if cp < 0 {
        return -1;
    }
    if cp <= 0xFFFF {
        for (i, c) in u.iter().enumerate().skip(start) {
            if *c as i32 == cp {
                return i as i32;
            }
        }
        return -1;
    }
    let cp = cp as u32;
    if cp > 0x10FFFF {
        return -1;
    }
    let hi = (0xD800 + ((cp - 0x10000) >> 10)) as u16;
    let lo = (0xDC00 + ((cp - 0x10000) & 0x3FF)) as u16;
    let mut i = start;
    while i + 1 < u.len() {
        if u[i] == hi && u[i + 1] == lo {
            return i as i32;
        }
        i += 1;
    }
    -1
}

fn last_index_of_cp(s: &JString, cp: i32, from: i32) -> i32 {
    let u = s.units();
    let mut i = from.min(u.len() as i32 - 1);
    while i >= 0 {
        if index_of_cp(s, cp, i) == i {
            return i;
        }
        i -= 1;
    }
    -1
}

/// String.lastIndexOf(String, int)
fn last_index_of_str(s: &JString, t: &JString, from: i32) -> i32 {
    let (h, n) = (s.units(), t.units());
    if n.len() > h.len() {
        return -1;
    }
    let mut k = from.min((h.len() - n.len()) as i32);
    while k >= 0 {
        if h[k as usize..k as usize + n.len()] == *n {
            return k;
        }
        k -= 1;
    }
    -1
}

fn regex_split(s: &JString, re: &JString, limit: i32) -> Result<Value, JexlException> {
    let (input, mapped) = to_rust_lossless(s);
    let (pattern, _) = to_rust_lossless(re);
    match regex::string_split(&input, &pattern, limit) {
        Ok(parts) => Ok(Value::Array(JArray::new(
            Component::Class("java.lang.String".into()),
            parts.iter().map(|p| Value::String(from_rust_lossless(p, mapped))).collect(),
        ))),
        Err(e) => Err(JexlException::java("java.util.regex.PatternSyntaxException", Some(e.get_message()))),
    }
}

// ---------------------------------------------------------------------------------------------
// Collection helpers

fn map_put(obj: &Value, k: Value, v: Value) -> Result<Value, JexlException> {
    match obj {
        Value::Map(m) => Ok(m.put(k, v).unwrap_or(Value::Null)),
        _ => Ok(Value::Null),
    }
}

fn list_of(o: &Value) -> JList {
    match o {
        Value::List(l) => l.clone(),
        _ => JList::array_list(Vec::new()),
    }
}

fn set_of(o: &Value) -> JSet {
    match o {
        Value::Set(s) => s.clone(),
        _ => JSet::new(SetKind::HashSet, JHashSet::new()),
    }
}

fn map_of(o: &Value) -> JMap {
    match o {
        Value::Map(m) => m.clone(),
        _ => JMap::hash_map(),
    }
}

fn index_of_in(items: &[Value], v: &Value) -> i32 {
    items.iter().position(|e| e.java_equals(v)).map(|i| i as i32).unwrap_or(-1)
}

fn object_array(items: Vec<Value>) -> Value {
    Value::Array(JArray::new(Component::object(), items))
}

/// `toArray(T[])` is not varargs, so reflection only accepts a real array here.
fn to_array_typed(o: &Value, a: &[Value]) -> Result<Value, JexlException> {
    nn(&a[0])?;
    let items = arg_values(o);
    let dest = match &a[0] {
        Value::Array(d) => d,
        _ => return Ok(object_array(items)),
    };
    if dest.len() < items.len() {
        return Ok(Value::Array(JArray::new(dest.component.clone(), items)));
    }
    for (i, v) in items.iter().enumerate() {
        dest.set(i, v.clone());
    }
    if dest.len() > items.len() {
        dest.set(items.len(), Value::Null);
    }
    Ok(a[0].clone())
}

fn list_iterator_class(kind: ListKind) -> &'static str {
    match kind {
        ListKind::LinkedList => "java.util.LinkedList$ListItr",
        _ => "java.util.ArrayList$Itr",
    }
}

fn set_iterator_class(kind: SetKind) -> &'static str {
    match kind {
        SetKind::LinkedHashSet => "java.util.LinkedHashMap$LinkedKeyIterator",
        SetKind::TreeSet => "java.util.TreeMap$KeyIterator",
        _ => "java.util.HashMap$KeyIterator",
    }
}

// ---------------------------------------------------------------------------------------------
// The declared-method tables (what Class.getDeclaredMethods() would return, per declaring class)

const OBJECT: &[Sig] = sigs!("java.lang.Object";
    "toString"() -> "java.lang.String" = |o, _| jstring(o.java_to_jstring());
    "hashCode"() -> "int" = |o, _| int(o.java_hash_code());
    "equals"("java.lang.Object") -> "boolean" = |o, a| boolean(o.java_equals(&a[0]));
    "getClass"() -> "java.lang.Class" = |o, _| Ok(ClassValue::of(&o.class_name()));
);

const COMPARABLE: &[Sig] = sigs!("java.lang.Comparable";
    "compareTo"("java.lang.Object") -> "int" = |o, a| compare_to(o, &a[0]);
);

/// port of: the Comparable bridge - it casts before comparing, so a foreign argument throws.
fn compare_to(o: &Value, a: &Value) -> Result<Value, JexlException> {
    let want = JClass::of(o).name();
    if a.is_null() {
        return match o {
            Value::String(_) => npe("Cannot read field \"value\" because \"anotherString\" is null"),
            _ => npe0(),
        };
    }
    if JClass::of(a).name() != want {
        return cce(&JClass::of(a).name(), &want);
    }
    int(match (o, a) {
        (Value::String(x), Value::String(y)) => x.compare_to(y),
        (Value::Character(x), Value::Character(y)) => *x as i32 - *y as i32,
        (Value::Boolean(x), Value::Boolean(y)) => (*x as i32) - (*y as i32),
        (Value::Byte(x), Value::Byte(y)) => (*x as i32) - (*y as i32),
        (Value::Short(x), Value::Short(y)) => (*x as i32) - (*y as i32),
        (Value::Integer(x), Value::Integer(y)) => x.cmp(y) as i32,
        (Value::Long(x), Value::Long(y)) => x.cmp(y) as i32,
        (Value::Float(x), Value::Float(y)) => float_compare(*x, *y),
        (Value::Double(x), Value::Double(y)) => double_compare(*x, *y),
        (Value::BigInteger(x), Value::BigInteger(y)) => x.cmp(y) as i32,
        (Value::BigDecimal(x), Value::BigDecimal(y)) => x.compare_to(y) as i32,
        _ => 0,
    })
}

// port of: Double.compare (total order: -0.0 < 0.0 and NaN is greatest)
fn double_compare(a: f64, b: f64) -> i32 {
    if a < b {
        return -1;
    }
    if a > b {
        return 1;
    }
    let (x, y) = (crate::value::double_bits(a) as i64, crate::value::double_bits(b) as i64);
    match x.cmp(&y) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Equal => 0,
    }
}

fn float_compare(a: f32, b: f32) -> i32 {
    if a < b {
        return -1;
    }
    if a > b {
        return 1;
    }
    let (x, y) = (crate::value::float_bits(a) as i32, crate::value::float_bits(b) as i32);
    match x.cmp(&y) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Equal => 0,
    }
}

const CHARSEQUENCE: &[Sig] = sigs!("java.lang.CharSequence";
    "length"() -> "int" = |o, _| int(s_len(o) as i32);
    "isEmpty"() -> "boolean" = |o, _| boolean(s_len(o) == 0);
    "charAt"("int") -> "char" = |o, a| {
        let s = arg_jstring(o);
        let i = check_index(arg_i64(&a[0]), s.len())?;
        Ok(Value::Character(s.units()[i]))
    };
);

const STRING: &[Sig] = sigs!("java.lang.String";
    "isBlank"() -> "boolean" = |o, _| boolean(arg_jstring(o).units().iter().all(|c| char_is_whitespace(*c as i32)));
    "codePointAt"("int") -> "int" = |o, a| {
        let s = arg_jstring(o);
        let i = check_index(arg_i64(&a[0]), s.len())?;
        let u = s.units();
        let c = u[i];
        if (0xD800..0xDC00).contains(&c) && i + 1 < u.len() && (0xDC00..0xE000).contains(&u[i + 1]) {
            return int(0x10000 + (((c as i32 - 0xD800) << 10) | (u[i + 1] as i32 - 0xDC00)));
        }
        int(c as i32)
    };
    "indexOf"("java.lang.String") -> "int" = |o, a| {
        nn(&a[0])?;
        int(arg_jstring(o).index_of(&arg_jstring(&a[0]), 0))
    };
    "indexOf"("java.lang.String", "int") -> "int" = |o, a| {
        nn(&a[0])?;
        int(arg_jstring(o).index_of(&arg_jstring(&a[0]), arg_i32(&a[1])))
    };
    "indexOf"("int") -> "int" = |o, a| int(index_of_cp(&arg_jstring(o), arg_i32(&a[0]), 0));
    "indexOf"("int", "int") -> "int" = |o, a| int(index_of_cp(&arg_jstring(o), arg_i32(&a[0]), arg_i32(&a[1])));
    "lastIndexOf"("java.lang.String") -> "int" = |o, a| {
        nn(&a[0])?;
        let s = arg_jstring(o);
        int(last_index_of_str(&s, &arg_jstring(&a[0]), s.len() as i32))
    };
    "lastIndexOf"("java.lang.String", "int") -> "int" = |o, a| {
        nn(&a[0])?;
        int(last_index_of_str(&arg_jstring(o), &arg_jstring(&a[0]), arg_i32(&a[1])))
    };
    "lastIndexOf"("int") -> "int" = |o, a| {
        let s = arg_jstring(o);
        int(last_index_of_cp(&s, arg_i32(&a[0]), s.len() as i32))
    };
    "lastIndexOf"("int", "int") -> "int" =
        |o, a| int(last_index_of_cp(&arg_jstring(o), arg_i32(&a[0]), arg_i32(&a[1])));
    "substring"("int") -> "java.lang.String" = |o, a| {
        let s = arg_jstring(o);
        let (b, e) = check_range(arg_i64(&a[0]), s.len() as i64, s.len())?;
        jstring(s.substring(b, e))
    };
    "substring"("int", "int") -> "java.lang.String" = |o, a| {
        let s = arg_jstring(o);
        let (b, e) = check_range(arg_i64(&a[0]), arg_i64(&a[1]), s.len())?;
        jstring(s.substring(b, e))
    };
    "subSequence"("int", "int") -> "java.lang.CharSequence" = |o, a| {
        let s = arg_jstring(o);
        let (b, e) = check_range(arg_i64(&a[0]), arg_i64(&a[1]), s.len())?;
        jstring(s.substring(b, e))
    };
    "concat"("java.lang.String") -> "java.lang.String" = |o, a| {
        nn(&a[0])?;
        jstring(arg_jstring(o).concat(&arg_jstring(&a[0])))
    };
    "contains"("java.lang.CharSequence") -> "boolean" = |o, a| {
        nn(&a[0])?;
        boolean(arg_jstring(o).index_of(&arg_jstring(&a[0]), 0) >= 0)
    };
    "startsWith"("java.lang.String") -> "boolean" = |o, a| {
        nn(&a[0])?;
        boolean(arg_jstring(o).starts_with(&arg_jstring(&a[0])))
    };
    "startsWith"("java.lang.String", "int") -> "boolean" = |o, a| {
        // `toffset < 0` short-circuits, so a null prefix is not dereferenced
        let off = arg_i64(&a[1]);
        if off < 0 {
            return boolean(false);
        }
        nn(&a[0])?;
        let (s, p) = (arg_jstring(o), arg_jstring(&a[0]));
        if off + p.len() as i64 > s.len() as i64 {
            return boolean(false);
        }
        boolean(s.units()[off as usize..off as usize + p.len()] == *p.units())
    };
    "endsWith"("java.lang.String") -> "boolean" = |o, a| {
        nn(&a[0])?;
        boolean(arg_jstring(o).ends_with(&arg_jstring(&a[0])))
    };
    "equalsIgnoreCase"("java.lang.String") -> "boolean" = |o, a| {
        if a[0].is_null() {
            return boolean(false);
        }
        let (s, t) = (arg_jstring(o), arg_jstring(&a[0]));
        boolean(s.len() == t.len() && region_matches_ci(s.units(), 0, t.units(), 0, s.len()))
    };
    "compareToIgnoreCase"("java.lang.String") -> "int" = |o, a| {
        if a[0].is_null() {
            return npe("Cannot read field \"value\" because \"s2\" is null");
        }
        int(compare_ignore_case(&arg_jstring(o), &arg_jstring(&a[0])))
    };
    "toUpperCase"() -> "java.lang.String" = |o, _| {
        let (s, m) = to_rust_lossless(&arg_jstring(o));
        jstring(from_rust_lossless(&s.to_uppercase(), m))
    };
    "toLowerCase"() -> "java.lang.String" = |o, _| {
        let (s, m) = to_rust_lossless(&arg_jstring(o));
        jstring(from_rust_lossless(&s.to_lowercase(), m))
    };
    "toUpperCase"("java.util.Locale") -> "java.lang.String" = |_, _| npe0();
    "toLowerCase"("java.util.Locale") -> "java.lang.String" = |_, _| npe0();
    "trim"() -> "java.lang.String" = |o, _| {
        let s = arg_jstring(o);
        let u = s.units();
        let mut b = 0;
        let mut e = u.len();
        while b < e && u[b] <= 0x20 {
            b += 1;
        }
        while e > b && u[e - 1] <= 0x20 {
            e -= 1;
        }
        jstring(s.substring(b, e))
    };
    "strip"() -> "java.lang.String" = |o, _| {
        let s = arg_jstring(o);
        let u = s.units();
        let mut b = 0;
        let mut e = u.len();
        while b < e && char_is_whitespace(u[b] as i32) {
            b += 1;
        }
        while e > b && char_is_whitespace(u[e - 1] as i32) {
            e -= 1;
        }
        jstring(s.substring(b, e))
    };
    "stripLeading"() -> "java.lang.String" = |o, _| {
        let s = arg_jstring(o);
        let u = s.units();
        let mut b = 0;
        while b < u.len() && char_is_whitespace(u[b] as i32) {
            b += 1;
        }
        jstring(s.substring(b, u.len()))
    };
    "stripTrailing"() -> "java.lang.String" = |o, _| {
        let s = arg_jstring(o);
        let u = s.units();
        let mut e = u.len();
        while e > 0 && char_is_whitespace(u[e - 1] as i32) {
            e -= 1;
        }
        jstring(s.substring(0, e))
    };
    "replace"("char", "char") -> "java.lang.String" = |o, a| {
        let (old, new) = (arg_char(&a[0]), arg_char(&a[1]));
        let u: Vec<u16> = arg_jstring(o).units().iter().map(|c| if *c == old { new } else { *c }).collect();
        jstring(JString::new(u))
    };
    "replace"("java.lang.CharSequence", "java.lang.CharSequence") -> "java.lang.String" = |o, a| {
        nn(&a[0])?;
        nn(&a[1])?;
        let (s, t, r) = (arg_jstring(o), arg_jstring(&a[0]), arg_jstring(&a[1]));
        let mut out: Vec<u16> = Vec::new();
        let u = s.units();
        let mut i = 0;
        if t.is_empty() {
            out.extend_from_slice(r.units());
            while i < u.len() {
                out.push(u[i]);
                out.extend_from_slice(r.units());
                i += 1;
            }
            return jstring(JString::new(out));
        }
        while i < u.len() {
            if i + t.len() <= u.len() && u[i..i + t.len()] == *t.units() {
                out.extend_from_slice(r.units());
                i += t.len();
            } else {
                out.push(u[i]);
                i += 1;
            }
        }
        jstring(JString::new(out))
    };
    "replaceAll"("java.lang.String", "java.lang.String") -> "java.lang.String" = |o, a| {
        nn(&a[0])?;
        nn(&a[1])?;
        let (s, m) = to_rust_lossless(&arg_jstring(o));
        let (re, _) = to_rust_lossless(&arg_jstring(&a[0]));
        let (rep, _) = to_rust_lossless(&arg_jstring(&a[1]));
        match regex::string_replace_all(&s, &re, &rep) {
            Ok(v) => jstring(from_rust_lossless(&v, m)),
            Err(e) => regex_error(e),
        }
    };
    "replaceFirst"("java.lang.String", "java.lang.String") -> "java.lang.String" = |o, a| {
        nn(&a[0])?;
        nn(&a[1])?;
        let (s, m) = to_rust_lossless(&arg_jstring(o));
        let (re, _) = to_rust_lossless(&arg_jstring(&a[0]));
        let (rep, _) = to_rust_lossless(&arg_jstring(&a[1]));
        match regex::string_replace_first(&s, &re, &rep) {
            Ok(v) => jstring(from_rust_lossless(&v, m)),
            Err(e) => regex_error(e),
        }
    };
    "matches"("java.lang.String") -> "boolean" = |o, a| {
        nn(&a[0])?;
        let (s, _) = to_rust_lossless(&arg_jstring(o));
        let (re, _) = to_rust_lossless(&arg_jstring(&a[0]));
        match regex::string_matches(&s, &re) {
            Ok(b) => boolean(b),
            Err(e) => Err(JexlException::java("java.util.regex.PatternSyntaxException", Some(e.get_message()))),
        }
    };
    "split"("java.lang.String") -> "java.lang.String[]" = |o, a| {
        nn(&a[0])?;
        regex_split(&arg_jstring(o), &arg_jstring(&a[0]), 0)
    };
    "split"("java.lang.String", "int") -> "java.lang.String[]" = |o, a| {
        nn(&a[0])?;
        regex_split(&arg_jstring(o), &arg_jstring(&a[0]), arg_i32(&a[1]))
    };
    "toCharArray"() -> "char[]" = |o, _| {
        let items: Vec<Value> = arg_jstring(o).units().iter().map(|c| Value::Character(*c)).collect();
        Ok(Value::Array(JArray::new(Component::Char, items)))
    };
    "repeat"("int") -> "java.lang.String" = |o, a| {
        let n = arg_i32(&a[0]);
        if n < 0 {
            return iae(&format!("count is negative: {}", n));
        }
        let u = arg_jstring(o);
        if n == 1 {
            return jstring(u);
        }
        if n == 0 || u.is_empty() {
            return jstring(JString::empty());
        }
        // String.repeat's count guard, then the array limit of the coder the result would use
        // (StringUTF16.MAX_LENGTH is Integer.MAX_VALUE >> 1)
        let limit = if is_latin1(u.units()) { i32::MAX as i64 } else { (i32::MAX >> 1) as i64 };
        if i32::MAX / n < u.len() as i32 || u.len() as i64 * n as i64 > limit {
            return jthrow("java.lang.OutOfMemoryError", "Required length exceeds implementation limit");
        }
        let mut out: Vec<u16> = Vec::with_capacity(u.len() * n as usize);
        for _ in 0..n {
            out.extend_from_slice(u.units());
        }
        jstring(JString::new(out))
    };
    "intern"() -> "java.lang.String" = |o, _| jstring(arg_jstring(o));
    "regionMatches"("int", "java.lang.String", "int", "int") -> "boolean" =
        |o, a| region_matches(o, false, arg_i64(&a[0]), &a[1], arg_i64(&a[2]), arg_i64(&a[3]));
    "regionMatches"("boolean", "int", "java.lang.String", "int", "int") -> "boolean" =
        |o, a| region_matches(o, arg_bool(&a[0]), arg_i64(&a[1]), &a[2], arg_i64(&a[3]), arg_i64(&a[4]));
    "valueOf"("java.lang.Object") -> "java.lang.String" = |_, a| jstring(a[0].java_to_jstring());
    "valueOf"("int") -> "java.lang.String" = |_, a| jstring(JString::from(arg_i32(&a[0]).to_string()));
    "valueOf"("long") -> "java.lang.String" = |_, a| jstring(JString::from(arg_i64(&a[0]).to_string()));
    "valueOf"("float") -> "java.lang.String" =
        |_, a| jstring(JString::from(number::float_to_string(arg_f32(&a[0]))));
    "valueOf"("double") -> "java.lang.String" =
        |_, a| jstring(JString::from(number::double_to_string(arg_f64(&a[0]))));
    "valueOf"("boolean") -> "java.lang.String" = |_, a| jstring(JString::from(arg_bool(&a[0]).to_string()));
    "valueOf"("char") -> "java.lang.String" = |_, a| jstring(JString::from_units(&[arg_char(&a[0])]));
);

fn region_matches(o: &Value, ignore: bool, toff: i64, other: &Value, ooff: i64, len: i64) -> Result<Value, JexlException> {
    if other.is_null() {
        return npe0();
    }
    let (s, t) = (arg_jstring(o), arg_jstring(other));
    if toff < 0 || ooff < 0 || len < 0 || toff + len > s.len() as i64 || ooff + len > t.len() as i64 {
        return boolean(false);
    }
    if ignore {
        return boolean(region_matches_ci(s.units(), toff as usize, t.units(), ooff as usize, len as usize));
    }
    let (a, b) = (&s.units()[toff as usize..], &t.units()[ooff as usize..]);
    boolean((0..len as usize).all(|i| a[i] == b[i]))
}

const STRING_JOIN_ITERABLE: &[Sig] = sigs!("java.lang.String";
    "join"("java.lang.CharSequence", "java.lang.Iterable") -> "java.lang.String" = |_, a| {
        nn(&a[0])?;
        nn(&a[1])?;
        join_with(&arg_jstring(&a[0]), &arg_values(&a[1]))
    };
);

fn join_with(sep: &JString, parts: &[Value]) -> Result<Value, JexlException> {
    let mut b = JStringBuilder::new();
    for (i, e) in parts.iter().enumerate() {
        // the loop variable is a CharSequence, so every element is cast
        if !matches!(e, Value::Null | Value::String(_)) && !is_char_sequence(e) {
            return cce(&JClass::of(e).name(), "java.lang.CharSequence");
        }
        if i > 0 {
            b.jstr(sep);
        }
        b.jstr(&e.java_to_jstring());
    }
    jstring(b.build())
}

pub(crate) fn is_char_sequence(v: &Value) -> bool {
    is_assignable(&JClass::named("java.lang.CharSequence"), &JClass::of(v))
}

const STRING_STATIC: &[Sig] = vsigs!("java.lang.String";
    "join"("java.lang.CharSequence", "java.lang.CharSequence[]") -> "java.lang.String" = |_, a| {
        nn(&a[0])?;
        join_with(&arg_jstring(&a[0]), &arg_values(&a[1]))
    };
);

const CHARACTER: &[Sig] = sigs!("java.lang.Character";
    "charValue"() -> "char" = |o, _| Ok(o.clone());
    "isDigit"("char") -> "boolean" = |_, a| boolean(char_is_digit(arg_char(&a[0]) as i32));
    "isDigit"("int") -> "boolean" = |_, a| boolean(char_is_digit(arg_i32(&a[0])));
    "isLetter"("char") -> "boolean" = |_, a| boolean(char_is_letter(arg_char(&a[0]) as i32));
    "isLetter"("int") -> "boolean" = |_, a| boolean(char_is_letter(arg_i32(&a[0])));
    "isLetterOrDigit"("char") -> "boolean" = |_, a| {
        let c = arg_char(&a[0]) as i32;
        boolean(char_is_letter(c) || char_is_digit(c))
    };
    "isLetterOrDigit"("int") -> "boolean" = |_, a| {
        let c = arg_i32(&a[0]);
        boolean(char_is_letter(c) || char_is_digit(c))
    };
    "isWhitespace"("char") -> "boolean" = |_, a| boolean(char_is_whitespace(arg_char(&a[0]) as i32));
    "isWhitespace"("int") -> "boolean" = |_, a| boolean(char_is_whitespace(arg_i32(&a[0])));
    "isSpaceChar"("char") -> "boolean" = |_, a| boolean(char_is_space(arg_char(&a[0]) as i32));
    "isSpaceChar"("int") -> "boolean" = |_, a| boolean(char_is_space(arg_i32(&a[0])));
    "isUpperCase"("char") -> "boolean" =
        |_, a| boolean(matches!(to_char(arg_char(&a[0])), Some(c) if c.is_uppercase()));
    "isUpperCase"("int") -> "boolean" =
        |_, a| boolean(matches!(cp_char(arg_i32(&a[0])), Some(c) if c.is_uppercase()));
    "isLowerCase"("char") -> "boolean" =
        |_, a| boolean(matches!(to_char(arg_char(&a[0])), Some(c) if c.is_lowercase()));
    "isLowerCase"("int") -> "boolean" =
        |_, a| boolean(matches!(cp_char(arg_i32(&a[0])), Some(c) if c.is_lowercase()));
    "toUpperCase"("char") -> "char" = |_, a| Ok(Value::Character(char_upper(arg_char(&a[0]))));
    "toUpperCase"("int") -> "int" = |_, a| int(cp_upper(arg_i32(&a[0])));
    "toLowerCase"("char") -> "char" = |_, a| Ok(Value::Character(char_lower(arg_char(&a[0]))));
    "toLowerCase"("int") -> "int" = |_, a| int(cp_lower(arg_i32(&a[0])));
    "getNumericValue"("char") -> "int" = |_, a| int(char_numeric_value(arg_char(&a[0]) as i32));
    "getNumericValue"("int") -> "int" = |_, a| int(char_numeric_value(arg_i32(&a[0])));
    "valueOf"("char") -> "java.lang.Character" = |_, a| Ok(a[0].clone());
    "toString"("char") -> "java.lang.String" = |_, a| jstring(JString::from_units(&[arg_char(&a[0])]));
    "toString"("int") -> "java.lang.String" = |_, a| {
        let cp = arg_i32(&a[0]);
        match cp_char(cp) {
            Some(c) => {
                let mut buf = [0u16; 2];
                jstring(JString::from_units(c.encode_utf16(&mut buf)))
            }
            None => iae(&format!("Not a valid Unicode code point: 0x{:X}", cp)),
        }
    };
    "hashCode"("char") -> "int" = |_, a| int(arg_char(&a[0]) as i32);
    "compare"("char", "char") -> "int" = |_, a| int(arg_char(&a[0]) as i32 - arg_char(&a[1]) as i32);
    "digit"("char", "int") -> "int" = |_, a| int(char_digit(arg_char(&a[0]) as i32, arg_i32(&a[1])));
    "digit"("int", "int") -> "int" = |_, a| int(char_digit(arg_i32(&a[0]), arg_i32(&a[1])));
    "forDigit"("int", "int") -> "char" = |_, a| Ok(Value::Character(char_for_digit(arg_i32(&a[0]), arg_i32(&a[1]))));
);

fn cp_upper(cp: i32) -> i32 {
    match cp_char(cp) {
        Some(ch) => {
            let mut it = ch.to_uppercase();
            match (it.next(), it.next()) {
                (Some(u), None) => u as i32,
                _ => cp,
            }
        }
        None => cp,
    }
}

fn cp_lower(cp: i32) -> i32 {
    match cp_char(cp) {
        Some(ch) => {
            let mut it = ch.to_lowercase();
            match (it.next(), it.next()) {
                (Some(u), None) => u as i32,
                _ => cp,
            }
        }
        None => cp,
    }
}

const BOOLEAN: &[Sig] = sigs!("java.lang.Boolean";
    // backed by System.getProperty; a Rust process has no Java system properties, so every name
    // is unset -- the answer Java gives for any property it does not have
    "getBoolean"("java.lang.String") -> "boolean" = |_, _| boolean(false);
    "booleanValue"() -> "boolean" = |o, _| Ok(o.clone());
    "parseBoolean"("java.lang.String") -> "boolean" = |_, a| {
        if a[0].is_null() {
            return boolean(false);
        }
        boolean(compare_ignore_case(&arg_jstring(&a[0]), &JString::from("true")) == 0)
    };
    "valueOf"("java.lang.String") -> "java.lang.Boolean" = |_, a| {
        if a[0].is_null() {
            return boolean(false);
        }
        boolean(compare_ignore_case(&arg_jstring(&a[0]), &JString::from("true")) == 0)
    };
    "valueOf"("boolean") -> "java.lang.Boolean" = |_, a| boolean(arg_bool(&a[0]));
    "toString"("boolean") -> "java.lang.String" = |_, a| jstring(JString::from(arg_bool(&a[0]).to_string()));
    "compare"("boolean", "boolean") -> "int" = |_, a| int(arg_bool(&a[0]) as i32 - arg_bool(&a[1]) as i32);
    "logicalAnd"("boolean", "boolean") -> "boolean" = |_, a| boolean(arg_bool(&a[0]) && arg_bool(&a[1]));
    "logicalOr"("boolean", "boolean") -> "boolean" = |_, a| boolean(arg_bool(&a[0]) || arg_bool(&a[1]));
    "logicalXor"("boolean", "boolean") -> "boolean" = |_, a| boolean(arg_bool(&a[0]) != arg_bool(&a[1]));
    "hashCode"("boolean") -> "int" = |_, a| int(if arg_bool(&a[0]) { 1231 } else { 1237 });
);

const NUMBER: &[Sig] = sigs!("java.lang.Number";
    "intValue"() -> "int" = |o, _| int(num_int(o));
    "longValue"() -> "long" = |o, _| Ok(Value::Long(num_long(o)));
    "doubleValue"() -> "double" = |o, _| Ok(Value::Double(num_double(o)));
    "floatValue"() -> "float" = |o, _| Ok(Value::Float(num_float(o)));
    "byteValue"() -> "byte" = |o, _| Ok(Value::Byte(num_int(o) as i8));
    "shortValue"() -> "short" = |o, _| Ok(Value::Short(num_int(o) as i16));
);

fn num_int(o: &Value) -> i32 {
    match o {
        Value::BigInteger(b) => number::big_integer_int_value(b),
        Value::BigDecimal(b) => b.int_value(),
        other => arg_i32(other),
    }
}

fn num_long(o: &Value) -> i64 {
    match o {
        Value::BigInteger(b) => number::big_integer_long_value(b),
        Value::BigDecimal(b) => b.long_value(),
        other => arg_i64(other),
    }
}

fn num_double(o: &Value) -> f64 {
    match o {
        Value::BigInteger(b) => number::big_integer_double_value(b),
        Value::BigDecimal(b) => b.double_value(),
        other => arg_f64(other),
    }
}

fn num_float(o: &Value) -> f32 {
    match o {
        Value::BigInteger(b) => number::big_integer_double_value(b) as f32,
        Value::BigDecimal(b) => b.float_value(),
        other => arg_f32(other),
    }
}

const INTEGER_S: &[Sig] = sigs!("java.lang.Integer";
    // System.getProperty-backed, like Boolean.getBoolean: every name is unset here
    "getInteger"("java.lang.String") -> "java.lang.Integer" = |_, _| Ok(Value::Null);
    "getInteger"("java.lang.String", "int") -> "java.lang.Integer" = |_, a| int(arg_i32(&a[1]));
    "getInteger"("java.lang.String", "java.lang.Integer") -> "java.lang.Integer" = |_, a| Ok(a[1].clone());
    "parseInt"("java.lang.String") -> "int" = |_, a| parse_int(&a[0], 10);
    "parseInt"("java.lang.String", "int") -> "int" = |_, a| parse_int(&a[0], arg_i32(&a[1]));
    "valueOf"("java.lang.String") -> "java.lang.Integer" = |_, a| parse_int(&a[0], 10);
    "valueOf"("java.lang.String", "int") -> "java.lang.Integer" = |_, a| parse_int(&a[0], arg_i32(&a[1]));
    "valueOf"("int") -> "java.lang.Integer" = |_, a| int(arg_i32(&a[0]));
    "toString"("int") -> "java.lang.String" = |_, a| jstring(JString::from(arg_i32(&a[0]).to_string()));
    "toString"("int", "int") -> "java.lang.String" =
        |_, a| jstring(JString::from(radix_string(arg_i32(&a[0]) as i64, arg_i32(&a[1]))));
    "toHexString"("int") -> "java.lang.String" = |_, a| jstring(JString::from(format!("{:x}", arg_i32(&a[0]) as u32)));
    "toOctalString"("int") -> "java.lang.String" = |_, a| jstring(JString::from(format!("{:o}", arg_i32(&a[0]) as u32)));
    "toBinaryString"("int") -> "java.lang.String" = |_, a| jstring(JString::from(format!("{:b}", arg_i32(&a[0]) as u32)));
    "compare"("int", "int") -> "int" = |_, a| int(arg_i32(&a[0]).cmp(&arg_i32(&a[1])) as i32);
    "max"("int", "int") -> "int" = |_, a| int(arg_i32(&a[0]).max(arg_i32(&a[1])));
    "min"("int", "int") -> "int" = |_, a| int(arg_i32(&a[0]).min(arg_i32(&a[1])));
    "sum"("int", "int") -> "int" = |_, a| int(arg_i32(&a[0]).wrapping_add(arg_i32(&a[1])));
    "signum"("int") -> "int" = |_, a| int(arg_i32(&a[0]).signum());
    "bitCount"("int") -> "int" = |_, a| int(arg_i32(&a[0]).count_ones() as i32);
    "hashCode"("int") -> "int" = |_, a| int(arg_i32(&a[0]));
);

const LONG_S: &[Sig] = sigs!("java.lang.Long";
    "getLong"("java.lang.String") -> "java.lang.Long" = |_, _| Ok(Value::Null);
    "getLong"("java.lang.String", "long") -> "java.lang.Long" = |_, a| Ok(Value::Long(arg_i64(&a[1])));
    "getLong"("java.lang.String", "java.lang.Long") -> "java.lang.Long" = |_, a| Ok(a[1].clone());
    "parseLong"("java.lang.String") -> "long" = |_, a| parse_long(&a[0], 10);
    "parseLong"("java.lang.String", "int") -> "long" = |_, a| parse_long(&a[0], arg_i32(&a[1]));
    "valueOf"("java.lang.String") -> "java.lang.Long" = |_, a| parse_long(&a[0], 10);
    "valueOf"("java.lang.String", "int") -> "java.lang.Long" = |_, a| parse_long(&a[0], arg_i32(&a[1]));
    "valueOf"("long") -> "java.lang.Long" = |_, a| Ok(Value::Long(arg_i64(&a[0])));
    "toString"("long") -> "java.lang.String" = |_, a| jstring(JString::from(arg_i64(&a[0]).to_string()));
    "toString"("long", "int") -> "java.lang.String" =
        |_, a| jstring(JString::from(radix_string(arg_i64(&a[0]), arg_i32(&a[1]))));
    "toHexString"("long") -> "java.lang.String" = |_, a| jstring(JString::from(format!("{:x}", arg_i64(&a[0]) as u64)));
    "toOctalString"("long") -> "java.lang.String" = |_, a| jstring(JString::from(format!("{:o}", arg_i64(&a[0]) as u64)));
    "toBinaryString"("long") -> "java.lang.String" = |_, a| jstring(JString::from(format!("{:b}", arg_i64(&a[0]) as u64)));
    "compare"("long", "long") -> "int" = |_, a| int(arg_i64(&a[0]).cmp(&arg_i64(&a[1])) as i32);
    "max"("long", "long") -> "long" = |_, a| Ok(Value::Long(arg_i64(&a[0]).max(arg_i64(&a[1]))));
    "min"("long", "long") -> "long" = |_, a| Ok(Value::Long(arg_i64(&a[0]).min(arg_i64(&a[1]))));
    "sum"("long", "long") -> "long" = |_, a| Ok(Value::Long(arg_i64(&a[0]).wrapping_add(arg_i64(&a[1]))));
    "signum"("long") -> "int" = |_, a| int(arg_i64(&a[0]).signum() as i32);
    "bitCount"("long") -> "int" = |_, a| int(arg_i64(&a[0]).count_ones() as i32);
    "hashCode"("long") -> "int" = |_, a| int(number::long_hash_code(arg_i64(&a[0])));
);

const SHORT_S: &[Sig] = sigs!("java.lang.Short";
    "parseShort"("java.lang.String") -> "short" = |_, a| parse_short(&a[0], 10);
    "parseShort"("java.lang.String", "int") -> "short" = |_, a| parse_short(&a[0], arg_i32(&a[1]));
    "valueOf"("java.lang.String") -> "java.lang.Short" = |_, a| parse_short(&a[0], 10);
    "valueOf"("java.lang.String", "int") -> "java.lang.Short" = |_, a| parse_short(&a[0], arg_i32(&a[1]));
    "valueOf"("short") -> "java.lang.Short" = |_, a| Ok(Value::Short(arg_i64(&a[0]) as i16));
    "toString"("short") -> "java.lang.String" = |_, a| jstring(JString::from(arg_i64(&a[0]).to_string()));
    "compare"("short", "short") -> "int" = |_, a| int(arg_i64(&a[0]) as i32 - arg_i64(&a[1]) as i32);
    "hashCode"("short") -> "int" = |_, a| int(arg_i64(&a[0]) as i32);
);

const BYTE_S: &[Sig] = sigs!("java.lang.Byte";
    "parseByte"("java.lang.String") -> "byte" = |_, a| parse_byte(&a[0], 10);
    "parseByte"("java.lang.String", "int") -> "byte" = |_, a| parse_byte(&a[0], arg_i32(&a[1]));
    "valueOf"("java.lang.String") -> "java.lang.Byte" = |_, a| parse_byte(&a[0], 10);
    "valueOf"("java.lang.String", "int") -> "java.lang.Byte" = |_, a| parse_byte(&a[0], arg_i32(&a[1]));
    "valueOf"("byte") -> "java.lang.Byte" = |_, a| Ok(Value::Byte(arg_i64(&a[0]) as i8));
    "toString"("byte") -> "java.lang.String" = |_, a| jstring(JString::from(arg_i64(&a[0]).to_string()));
    "compare"("byte", "byte") -> "int" = |_, a| int(arg_i64(&a[0]) as i32 - arg_i64(&a[1]) as i32);
    "hashCode"("byte") -> "int" = |_, a| int(arg_i64(&a[0]) as i32);
);

fn null_string(v: &Value) -> Result<String, JexlException> {
    if v.is_null() {
        return jthrow("java.lang.NumberFormatException", "Cannot parse null string");
    }
    Ok(arg_jstring(v).to_rust())
}

/// Double.parseDouble dereferences its argument, so a null is an NPE, not a NumberFormatException.
fn parse_double(v: &Value) -> Result<Value, JexlException> {
    nn(v)?;
    number::parse_double(&arg_jstring(v).to_rust()).map(Value::Double).or_else(nfe)
}

fn parse_float(v: &Value) -> Result<Value, JexlException> {
    nn(v)?;
    number::parse_float(&arg_jstring(v).to_rust()).map(Value::Float).or_else(nfe)
}

/// port of: Integer.parseInt's radix guard (checked before the string is looked at)
fn check_radix(radix: i32) -> Result<u32, JexlException> {
    if radix < 2 {
        return jthrow("java.lang.NumberFormatException", &format!("radix {} less than Character.MIN_RADIX", radix));
    }
    if radix > 36 {
        return jthrow("java.lang.NumberFormatException", &format!("radix {} greater than Character.MAX_RADIX", radix));
    }
    Ok(radix as u32)
}

fn parse_int(v: &Value, radix: i32) -> Result<Value, JexlException> {
    let text = null_string(v)?;
    number::parse_int(&text, check_radix(radix)?).map(Value::Integer).or_else(nfe)
}

fn parse_long(v: &Value, radix: i32) -> Result<Value, JexlException> {
    let text = null_string(v)?;
    number::parse_long(&text, check_radix(radix)?).map(Value::Long).or_else(nfe)
}

fn parse_short(v: &Value, radix: i32) -> Result<Value, JexlException> {
    let text = null_string(v)?;
    number::parse_short(&text, check_radix(radix)?).map(Value::Short).or_else(nfe)
}

fn parse_byte(v: &Value, radix: i32) -> Result<Value, JexlException> {
    let text = null_string(v)?;
    number::parse_byte(&text, check_radix(radix)?).map(Value::Byte).or_else(nfe)
}

/// Integer.toString(int, int) / Long.toString(long, int)
fn radix_string(v: i64, radix: i32) -> String {
    if !(2..=36).contains(&radix) {
        return v.to_string();
    }
    let neg = v < 0;
    let mut n = (v as i128).unsigned_abs();
    let mut out: Vec<u8> = Vec::new();
    if n == 0 {
        out.push(b'0');
    }
    while n > 0 {
        let d = (n % radix as u128) as u8;
        out.push(if d < 10 { b'0' + d } else { b'a' + d - 10 });
        n /= radix as u128;
    }
    if neg {
        out.push(b'-');
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

// port of: Math.min/Math.max for doubles (NaN wins, -0.0 < 0.0)
fn jmin(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && b.is_sign_negative() {
        return b;
    }
    if a <= b {
        a
    } else {
        b
    }
}

fn jmax(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && a.is_sign_negative() {
        return b;
    }
    if a >= b {
        a
    } else {
        b
    }
}

const DOUBLE_S: &[Sig] = sigs!("java.lang.Double";
    "isNaN"() -> "boolean" = |o, _| boolean(num_double(o).is_nan());
    "isInfinite"() -> "boolean" = |o, _| boolean(num_double(o).is_infinite());
    "isNaN"("double") -> "boolean" = |_, a| boolean(arg_f64(&a[0]).is_nan());
    "isInfinite"("double") -> "boolean" = |_, a| boolean(arg_f64(&a[0]).is_infinite());
    "isFinite"("double") -> "boolean" = |_, a| boolean(arg_f64(&a[0]).is_finite());
    "parseDouble"("java.lang.String") -> "double" = |_, a| parse_double(&a[0]);
    "valueOf"("java.lang.String") -> "java.lang.Double" = |_, a| parse_double(&a[0]);
    "valueOf"("double") -> "java.lang.Double" = |_, a| Ok(Value::Double(arg_f64(&a[0])));
    "toString"("double") -> "java.lang.String" =
        |_, a| jstring(JString::from(number::double_to_string(arg_f64(&a[0]))));
    "compare"("double", "double") -> "int" = |_, a| int(double_compare(arg_f64(&a[0]), arg_f64(&a[1])));
    "max"("double", "double") -> "double" = |_, a| Ok(Value::Double(jmax(arg_f64(&a[0]), arg_f64(&a[1]))));
    "min"("double", "double") -> "double" = |_, a| Ok(Value::Double(jmin(arg_f64(&a[0]), arg_f64(&a[1]))));
    "sum"("double", "double") -> "double" = |_, a| Ok(Value::Double(arg_f64(&a[0]) + arg_f64(&a[1])));
    "doubleToLongBits"("double") -> "long" =
        |_, a| Ok(Value::Long(crate::value::double_bits(arg_f64(&a[0])) as i64));
    "doubleToRawLongBits"("double") -> "long" =
        |_, a| Ok(Value::Long(crate::value::double_raw_bits(arg_f64(&a[0])) as i64));
    "longBitsToDouble"("long") -> "double" = |_, a| Ok(Value::Double(f64::from_bits(arg_i64(&a[0]) as u64)));
    "hashCode"("double") -> "int" = |_, a| int(number::double_hash_code(arg_f64(&a[0])));
);

const FLOAT_S: &[Sig] = sigs!("java.lang.Float";
    "isNaN"() -> "boolean" = |o, _| boolean(num_double(o).is_nan());
    "isInfinite"() -> "boolean" = |o, _| boolean(num_double(o).is_infinite());
    "isNaN"("float") -> "boolean" = |_, a| boolean(arg_f32(&a[0]).is_nan());
    "isInfinite"("float") -> "boolean" = |_, a| boolean(arg_f32(&a[0]).is_infinite());
    "isFinite"("float") -> "boolean" = |_, a| boolean(arg_f32(&a[0]).is_finite());
    "parseFloat"("java.lang.String") -> "float" = |_, a| parse_float(&a[0]);
    "valueOf"("java.lang.String") -> "java.lang.Float" = |_, a| parse_float(&a[0]);
    "valueOf"("float") -> "java.lang.Float" = |_, a| Ok(Value::Float(arg_f32(&a[0])));
    "toString"("float") -> "java.lang.String" =
        |_, a| jstring(JString::from(number::float_to_string(arg_f32(&a[0]))));
    "compare"("float", "float") -> "int" = |_, a| int(float_compare(arg_f32(&a[0]), arg_f32(&a[1])));
    "max"("float", "float") -> "float" = |_, a| Ok(Value::Float(jmax(arg_f32(&a[0]) as f64, arg_f32(&a[1]) as f64) as f32));
    "min"("float", "float") -> "float" = |_, a| Ok(Value::Float(jmin(arg_f32(&a[0]) as f64, arg_f32(&a[1]) as f64) as f32));
    "sum"("float", "float") -> "float" = |_, a| Ok(Value::Float(arg_f32(&a[0]) + arg_f32(&a[1])));
    "floatToIntBits"("float") -> "int" = |_, a| int(crate::value::float_bits(arg_f32(&a[0])) as i32);
    "floatToRawIntBits"("float") -> "int" = |_, a| int(crate::value::float_raw_bits(arg_f32(&a[0])) as i32);
    "intBitsToFloat"("int") -> "float" = |_, a| Ok(Value::Float(f32::from_bits(arg_i32(&a[0]) as u32)));
    "hashCode"("float") -> "int" = |_, a| int(number::float_hash_code(arg_f32(&a[0])));
);

const BIGINTEGER: &[Sig] = sigs!("java.math.BigInteger";
    "add"("java.math.BigInteger") -> "java.math.BigInteger" = |o, a| bi2(o, &a[0], |x, y| Ok(x + y));
    "subtract"("java.math.BigInteger") -> "java.math.BigInteger" = |o, a| bi2(o, &a[0], |x, y| Ok(x - y));
    "multiply"("java.math.BigInteger") -> "java.math.BigInteger" = |o, a| bi2(o, &a[0], |x, y| Ok(x * y));
    "divide"("java.math.BigInteger") -> "java.math.BigInteger" = |o, a| bi2(o, &a[0], |x, y| {
        if y.is_zero() {
            return arithmetic("BigInteger divide by zero");
        }
        Ok(x / y)
    });
    "remainder"("java.math.BigInteger") -> "java.math.BigInteger" = |o, a| bi2(o, &a[0], |x, y| {
        if y.is_zero() {
            return arithmetic("BigInteger divide by zero");
        }
        Ok(x % y)
    });
    "mod"("java.math.BigInteger") -> "java.math.BigInteger" = |o, a| bi2(o, &a[0], |x, y| {
        if !y.is_positive() {
            return arithmetic("BigInteger: modulus not positive");
        }
        let r = x % y;
        Ok(if r.is_negative() { r + y } else { r })
    });
    "gcd"("java.math.BigInteger") -> "java.math.BigInteger" =
        |o, a| bi2(o, &a[0], |x, y| Ok(num_integer::Integer::gcd(x, y)));
    "min"("java.math.BigInteger") -> "java.math.BigInteger" = |o, a| bi2(o, &a[0], |x, y| Ok(x.min(y).clone()));
    "max"("java.math.BigInteger") -> "java.math.BigInteger" = |o, a| bi2(o, &a[0], |x, y| Ok(x.max(y).clone()));
    "and"("java.math.BigInteger") -> "java.math.BigInteger" = |o, a| bi2(o, &a[0], |x, y| Ok(x & y));
    "or"("java.math.BigInteger") -> "java.math.BigInteger" = |o, a| {
        if a[0].is_null() {
            return npe("Cannot invoke \"java.math.BigInteger.intLength()\" because \"val\" is null");
        }
        bi2(o, &a[0], |x, y| Ok(x | y))
    };
    "xor"("java.math.BigInteger") -> "java.math.BigInteger" = |o, a| bi2(o, &a[0], |x, y| Ok(x ^ y));
    "not"() -> "java.math.BigInteger" = |o, _| Ok(Value::big_integer(!arg_bigint(o)));
    "negate"() -> "java.math.BigInteger" = |o, _| Ok(Value::big_integer(-arg_bigint(o)));
    "abs"() -> "java.math.BigInteger" = |o, _| Ok(Value::big_integer(arg_bigint(o).abs()));
    "signum"() -> "int" = |o, _| int(match arg_bigint(o).sign() {
        Sign::Minus => -1,
        Sign::NoSign => 0,
        Sign::Plus => 1,
    });
    "pow"("int") -> "java.math.BigInteger" = |o, a| {
        let n = arg_i32(&a[0]);
        if n < 0 {
            return arithmetic("Negative exponent");
        }
        let b = arg_bigint(o);
        if n == 0 {
            return Ok(Value::big_integer(BigInt::one()));
        }
        if b.is_zero() || b.is_one() {
            return Ok(Value::big_integer(b));
        }
        // BigInteger.pow's own magnitude guard, so a huge exponent cannot exhaust memory
        if b.bits() as i64 * n as i64 > i32::MAX as i64 {
            return arithmetic("BigInteger would overflow supported range");
        }
        Ok(Value::big_integer(b.pow(n as u32)))
    };
    "shiftLeft"("int") -> "java.math.BigInteger" = |o, a| shift_checked(arg_bigint(o), arg_i32(&a[0]) as i64);
    "shiftRight"("int") -> "java.math.BigInteger" = |o, a| shift_checked(arg_bigint(o), -(arg_i32(&a[0]) as i64));
    "testBit"("int") -> "boolean" = |o, a| {
        let n = arg_i32(&a[0]);
        if n < 0 {
            return arithmetic("Negative bit address");
        }
        boolean(!(shift(arg_bigint(o), -n) & BigInt::one()).is_zero())
    };
    "bitLength"() -> "int" = |o, _| {
        let b = arg_bigint(o);
        int(if b.is_negative() { (-b - BigInt::one()).bits() as i32 } else { b.bits() as i32 })
    };
    "bitCount"() -> "int" = |o, _| {
        let b = arg_bigint(o);
        let mag = if b.is_negative() { -&b - BigInt::one() } else { b.clone() };
        let ones = mag.to_bytes_le().1.iter().map(|x| x.count_ones()).sum::<u32>() as i32;
        int(ones)
    };
    "toString"("int") -> "java.lang.String" = |o, a| {
        let r = arg_i32(&a[0]);
        let radix = if (2..=36).contains(&r) { r as u32 } else { 10 };
        jstring(JString::from(number::big_integer_to_string(&arg_bigint(o), radix)))
    };
    "valueOf"("long") -> "java.math.BigInteger" = |_, a| Ok(Value::big_integer(BigInt::from(arg_i64(&a[0]))));
);

fn shift(b: BigInt, n: i32) -> BigInt {
    if n >= 0 {
        b << (n as u32)
    } else {
        b >> ((-(n as i64)) as u32)
    }
}

/// BigInteger keeps its magnitude in an int[], so a shift past Integer.MAX_VALUE bits overflows.
fn shift_checked(b: BigInt, n: i64) -> Result<Value, JexlException> {
    if b.is_zero() {
        return Ok(Value::big_integer(BigInt::zero()));
    }
    if n > 0 && b.bits() as i64 + n > i32::MAX as i64 {
        return arithmetic("BigInteger would overflow supported range");
    }
    Ok(Value::big_integer(if n >= 0 { b << (n as u64) } else { b >> ((-n) as u64) }))
}

fn bi2(o: &Value, a: &Value, f: fn(&BigInt, &BigInt) -> Result<BigInt, JexlException>) -> Result<Value, JexlException> {
    if a.is_null() {
        return npe0();
    }
    f(&arg_bigint(o), &arg_bigint(a)).map(Value::big_integer)
}

const BIGDECIMAL: &[Sig] = sigs!("java.math.BigDecimal";
    "add"("java.math.BigDecimal") -> "java.math.BigDecimal" = |o, a| bd2(o, a, |x, y| Ok(x.add(y)));
    "subtract"("java.math.BigDecimal") -> "java.math.BigDecimal" = |o, a| bd2(o, a, |x, y| Ok(x.subtract(y)));
    "multiply"("java.math.BigDecimal") -> "java.math.BigDecimal" = |o, a| bd2(o, a, |x, y| Ok(x.multiply(y)));
    "divide"("java.math.BigDecimal") -> "java.math.BigDecimal" = |o, a| bd2(o, a, |x, y| x.divide(y));
    "remainder"("java.math.BigDecimal") -> "java.math.BigDecimal" = |o, a| bd2(o, a, |x, y| x.remainder(y));
    "min"("java.math.BigDecimal") -> "java.math.BigDecimal" = |o, a| bd2(o, a, |x, y| Ok(x.min(y)));
    "max"("java.math.BigDecimal") -> "java.math.BigDecimal" = |o, a| bd2(o, a, |x, y| Ok(x.max(y)));
    "negate"() -> "java.math.BigDecimal" = |o, _| Ok(Value::big_decimal(arg_bigdec(o).negate()));
    "abs"() -> "java.math.BigDecimal" = |o, _| Ok(Value::big_decimal(arg_bigdec(o).abs()));
    "ulp"() -> "java.math.BigDecimal" = |o, _| Ok(Value::big_decimal(arg_bigdec(o).ulp()));
    "pow"("int") -> "java.math.BigDecimal" = |o, a| {
        let x = arg_bigdec(o);
        let n = arg_i64(&a[0]);
        if (0..=999_999_999).contains(&n) && x.unscaled_value().bits() as i64 * n > i32::MAX as i64 {
            return arithmetic("BigInteger would overflow supported range");
        }
        match x.pow(arg_i32(&a[0])) {
            Ok(v) => Ok(Value::big_decimal(v)),
            Err(e) => math(e),
        }
    };
    "scale"() -> "int" = |o, _| int(arg_bigdec(o).scale());
    "precision"() -> "int" = |o, _| int(arg_bigdec(o).precision() as i32);
    "signum"() -> "int" = |o, _| int(arg_bigdec(o).signum());
    "unscaledValue"() -> "java.math.BigInteger" = |o, _| Ok(Value::big_integer(arg_bigdec(o).unscaled_value().clone()));
    "stripTrailingZeros"() -> "java.math.BigDecimal" =
        |o, _| Ok(Value::big_decimal(arg_bigdec(o).strip_trailing_zeros()));
    "toPlainString"() -> "java.lang.String" = |o, _| jstring(JString::from(arg_bigdec(o).to_plain_string()));
    "toBigInteger"() -> "java.math.BigInteger" = |o, _| Ok(Value::big_integer(arg_bigdec(o).to_big_integer()));
    "movePointLeft"("int") -> "java.math.BigDecimal" = |o, a| {
        let x = arg_bigdec(o);
        let new_scale = x.scale() as i64 + arg_i64(&a[0]);
        if let Some(v) = ten_power_ok(&x, -new_scale, new_scale.max(0))? {
            return Ok(v);
        }
        match x.move_point_left(arg_i32(&a[0])) {
            Ok(v) => Ok(Value::big_decimal(v)),
            Err(e) => math(e),
        }
    };
    "movePointRight"("int") -> "java.math.BigDecimal" = |o, a| {
        let x = arg_bigdec(o);
        let new_scale = x.scale() as i64 - arg_i64(&a[0]);
        if let Some(v) = ten_power_ok(&x, -new_scale, new_scale.max(0))? {
            return Ok(v);
        }
        match x.move_point_right(arg_i32(&a[0])) {
            Ok(v) => Ok(Value::big_decimal(v)),
            Err(e) => math(e),
        }
    };
    "setScale"("int", "int") -> "java.math.BigDecimal" = |o, a| {
        let mode = rounding_mode(arg_i32(&a[1]))?;
        let x = arg_bigdec(o);
        let new_scale = arg_i64(&a[0]);
        if let Some(v) = ten_power_ok(&x, new_scale - x.scale() as i64, new_scale)? {
            return Ok(v);
        }
        match x.set_scale(arg_i32(&a[0]), mode) {
            Ok(v) => Ok(Value::big_decimal(v)),
            Err(e) => math(e),
        }
    };
    "setScale"("int", "java.math.RoundingMode") -> "java.math.BigDecimal" = context_arg;
    "divide"("java.math.BigDecimal", "int") -> "java.math.BigDecimal" = |o, a| {
        nn(&a[0])?;
        let x = arg_bigdec(o);
        let scale = x.scale();
        match x.divide_scale(&arg_bigdec(&a[0]), scale, rounding_mode(arg_i32(&a[1]))?) {
            Ok(v) => Ok(Value::big_decimal(v)),
            Err(e) => math(e),
        }
    };
    "divide"("java.math.BigDecimal", "int", "int") -> "java.math.BigDecimal" = |o, a| {
        nn(&a[0])?;
        let mode = rounding_mode(arg_i32(&a[2]))?;
        let x = arg_bigdec(o);
        let scale = arg_i64(&a[1]);
        if arg_bigdec(&a[0]).signum() != 0 {
            if let Some(v) = ten_power_ok(&x, scale - x.scale() as i64, scale)? {
                return Ok(v);
            }
        }
        match x.divide_scale(&arg_bigdec(&a[0]), arg_i32(&a[1]), mode) {
            Ok(v) => Ok(Value::big_decimal(v)),
            Err(e) => math(e),
        }
    };
    "divide"("java.math.BigDecimal", "java.math.RoundingMode") -> "java.math.BigDecimal" = context_arg;
    "divide"("java.math.BigDecimal", "int", "java.math.RoundingMode") -> "java.math.BigDecimal" = context_arg;
    "divide"("java.math.BigDecimal", "java.math.MathContext") -> "java.math.BigDecimal" = context_arg;
    "add"("java.math.BigDecimal", "java.math.MathContext") -> "java.math.BigDecimal" = context_arg;
    "subtract"("java.math.BigDecimal", "java.math.MathContext") -> "java.math.BigDecimal" = context_arg;
    "multiply"("java.math.BigDecimal", "java.math.MathContext") -> "java.math.BigDecimal" = context_arg;
    "remainder"("java.math.BigDecimal", "java.math.MathContext") -> "java.math.BigDecimal" = context_arg;
    "pow"("int", "java.math.MathContext") -> "java.math.BigDecimal" = context_arg;
    "abs"("java.math.MathContext") -> "java.math.BigDecimal" = context_arg;
    "negate"("java.math.MathContext") -> "java.math.BigDecimal" = context_arg;
    "plus"() -> "java.math.BigDecimal" = |o, _| Ok(Value::big_decimal(arg_bigdec(o)));
    "plus"("java.math.MathContext") -> "java.math.BigDecimal" = context_arg;
    "round"("java.math.MathContext") -> "java.math.BigDecimal" = context_arg;
    "sqrt"("java.math.MathContext") -> "java.math.BigDecimal" = context_arg;
    "setScale"("int") -> "java.math.BigDecimal" = |o, a| {
        let x = arg_bigdec(o);
        let new_scale = arg_i64(&a[0]);
        if let Some(v) = ten_power_ok(&x, new_scale - x.scale() as i64, new_scale)? {
            return Ok(v);
        }
        match x.set_scale(arg_i32(&a[0]), RoundingMode::Unnecessary) {
            Ok(v) => Ok(Value::big_decimal(v)),
            Err(e) => math(e),
        }
    };
    "valueOf"("long") -> "java.math.BigDecimal" = |_, a| Ok(Value::big_decimal(BigDecimal::from_i64(arg_i64(&a[0]))));
    "valueOf"("long", "int") -> "java.math.BigDecimal" =
        |_, a| Ok(Value::big_decimal(BigDecimal::new(BigInt::from(arg_i64(&a[0])), arg_i32(&a[1]))));
    "valueOf"("double") -> "java.math.BigDecimal" = |_, a| match BigDecimal::value_of_double(arg_f64(&a[0])) {
        Ok(v) => Ok(Value::big_decimal(v)),
        Err(e) => math(e),
    };
);

/// BigDecimal's MathContext / RoundingMode overloads exist so that overload resolution matches
/// Java's; no value in the model can supply one, so they are only ever reached with a null.
fn context_arg(_obj: &Value, _args: &[Value]) -> Result<Value, JexlException> {
    npe0()
}

/// A BigInteger magnitude is an `int[]`, so it tops out at 2^31 bits - about this many decimal
/// digits. Raising a BigDecimal's scale multiplies by a power of ten, and Java reports the
/// overflow rather than attempting the allocation; `java::big_decimal` does not check, so the
/// guard lives here (see COMPATIBILITY.md).
const MAX_TEN_POWER: i64 = 646_456_993;

/// Zero needs no magnitude at all, so Java never overflows on it: the caller gets the answer
/// straight back (`BigDecimal(0, scale)`) instead of the error.
fn ten_power_ok(x: &BigDecimal, n: i64, scale: i64) -> Result<Option<Value>, JexlException> {
    if n <= MAX_TEN_POWER {
        return Ok(None);
    }
    if x.signum() == 0 {
        return Ok(Some(Value::big_decimal(BigDecimal::new(BigInt::zero(), scale.clamp(0, i32::MAX as i64) as i32))));
    }
    arithmetic("BigInteger would overflow supported range").map(|_: Value| None)
}

/// port of: RoundingMode.valueOf(int) as BigDecimal's legacy int overloads use it
fn rounding_mode(mode: i32) -> Result<RoundingMode, JexlException> {
    Ok(match mode {
        0 => RoundingMode::Up,
        1 => RoundingMode::Down,
        2 => RoundingMode::Ceiling,
        3 => RoundingMode::Floor,
        4 => RoundingMode::HalfUp,
        5 => RoundingMode::HalfDown,
        6 => RoundingMode::HalfEven,
        7 => RoundingMode::Unnecessary,
        _ => return iae("Invalid rounding mode").map(|_: Value| RoundingMode::Up),
    })
}

fn bd2(
    o: &Value,
    a: &[Value],
    f: fn(&BigDecimal, &BigDecimal) -> Result<BigDecimal, MathError>,
) -> Result<Value, JexlException> {
    if a[0].is_null() {
        return npe0();
    }
    match f(&arg_bigdec(o), &arg_bigdec(&a[0])) {
        Ok(v) => Ok(Value::big_decimal(v)),
        Err(e) => math(e),
    }
}

const COLLECTION: &[Sig] = sigs!("java.util.Collection";
    "size"() -> "int" = |o, _| int(arg_values(o).len() as i32);
    "isEmpty"() -> "boolean" = |o, _| boolean(arg_values(o).is_empty());
    "contains"("java.lang.Object") -> "boolean" = |o, a| boolean(arg_values(o).iter().any(|e| e.java_equals(&a[0])));
    "containsAll"("java.util.Collection") -> "boolean" = |o, a| {
        nn(&a[0])?;
        let mine = arg_values(o);
        boolean(arg_values(&a[0]).iter().all(|e| mine.iter().any(|m| m.java_equals(e))))
    };
    "toArray"() -> "java.lang.Object[]" = |o, _| Ok(object_array(arg_values(o)));
    "toArray"("java.lang.Object[]") -> "java.lang.Object[]" = to_array_typed;
    // Collection.toArray(IntFunction) exists only so a null argument resolves the way Java does
    "toArray"("java.util.function.IntFunction") -> "java.lang.Object[]" = |_, _| npe0();
);

const LIST: &[Sig] = sigs!("java.util.List";
    "get"("int") -> "java.lang.Object" = |o, a| list_get(o, arg_i32(&a[0]));
    "set"("int", "java.lang.Object") -> "java.lang.Object" = |o, a| {
        let l = list_of(o);
        let i = arg_i32(&a[0]);
        let mut w = l.write();
        match usize::try_from(i) {
            Ok(u) if u < w.items.len() => {
                let old = w.items[u].clone();
                w.items[u] = a[1].clone();
                Ok(old)
            }
            _ => elem_oob(w.kind, i as i64, w.items.len()),
        }
    };
    "add"("java.lang.Object") -> "boolean" = |o, a| {
        list_of(o).write().items.push(a[0].clone());
        boolean(true)
    };
    "add"("int", "java.lang.Object") -> "void" = |o, a| {
        let l = list_of(o);
        let i = arg_i32(&a[0]);
        let mut w = l.write();
        let n = w.items.len();
        match usize::try_from(i) {
            Ok(u) if u <= n => {
                w.items.insert(u, a[1].clone());
                Ok(Value::Null)
            }
            _ => jthrow("java.lang.IndexOutOfBoundsException", &format!("Index: {}, Size: {}", i, n)),
        }
    };
    "remove"("int") -> "java.lang.Object" = |o, a| {
        let l = list_of(o);
        let i = arg_i32(&a[0]);
        let mut w = l.write();
        match usize::try_from(i) {
            Ok(u) if u < w.items.len() => Ok(w.items.remove(u)),
            _ => elem_oob(w.kind, i as i64, w.items.len()),
        }
    };
    "remove"("java.lang.Object") -> "boolean" = |o, a| {
        let l = list_of(o);
        let mut w = l.write();
        match w.items.iter().position(|e| e.java_equals(&a[0])) {
            Some(i) => {
                w.items.remove(i);
                boolean(true)
            }
            None => boolean(false),
        }
    };
    "clear"() -> "void" = |o, _| {
        list_of(o).write().items.clear();
        Ok(Value::Null)
    };
    "indexOf"("java.lang.Object") -> "int" = |o, a| int(index_of_in(&arg_values(o), &a[0]));
    "lastIndexOf"("java.lang.Object") -> "int" = |o, a| {
        let items = arg_values(o);
        int(items.iter().rposition(|e| e.java_equals(&a[0])).map(|i| i as i32).unwrap_or(-1))
    };
    "addAll"("java.util.Collection") -> "boolean" = |o, a| {
        nn(&a[0])?;
        let add = arg_values(&a[0]);
        let n = add.len();
        list_of(o).write().items.extend(add);
        boolean(n > 0)
    };
    "addAll"("int", "java.util.Collection") -> "boolean" = |o, a| {
        let l = list_of(o);
        let i = arg_i32(&a[0]);
        let n = l.len();
        let u = match usize::try_from(i) {
            Ok(u) if u <= n => u,
            _ => return jthrow("java.lang.IndexOutOfBoundsException", &format!("Index: {}, Size: {}", i, n)),
        };
        nn(&a[1])?;
        let add = arg_values(&a[1]);
        let empty = add.is_empty();
        l.write().items.splice(u..u, add);
        boolean(!empty)
    };
    "removeAll"("java.util.Collection") -> "boolean" = |o, a| {
        nn(&a[0])?;
        let l = list_of(o);
        let drop = arg_values(&a[0]);
        let mut w = l.write();
        let before = w.items.len();
        w.items.retain(|e| !drop.iter().any(|d| d.java_equals(e)));
        boolean(w.items.len() != before)
    };
    "retainAll"("java.util.Collection") -> "boolean" = |o, a| {
        nn(&a[0])?;
        let l = list_of(o);
        let keep = arg_values(&a[0]);
        let mut w = l.write();
        let before = w.items.len();
        w.items.retain(|e| keep.iter().any(|d| d.java_equals(e)));
        boolean(w.items.len() != before)
    };
    "iterator"() -> "java.util.Iterator" =
        |o, _| Ok(JIterator::new(list_iterator_class(list_of(o).kind()), arg_values(o)));
);

/// port of: java.util.LinkedList's own declarations (an ArrayList has none of these).
const LINKED_LIST: &[Sig] = sigs!("java.util.LinkedList";
    "remove"() -> "java.lang.Object" = |o, _| linked_pop(o, true);
);

/// port of: java.util.SequencedCollection, which java.util.List extends since JDK 21
const SEQUENCED: &[Sig] = sigs!("java.util.List";
    "removeFirst"() -> "java.lang.Object" = |o, _| linked_pop(o, true);
    "removeLast"() -> "java.lang.Object" = |o, _| linked_pop(o, false);
    "getFirst"() -> "java.lang.Object" = |o, _| linked_peek(o, true);
    "getLast"() -> "java.lang.Object" = |o, _| linked_peek(o, false);
    "addFirst"("java.lang.Object") -> "void" = |o, a| {
        list_of(o).write().items.insert(0, a[0].clone());
        Ok(Value::Null)
    };
    "addLast"("java.lang.Object") -> "void" = |o, a| {
        list_of(o).write().items.push(a[0].clone());
        Ok(Value::Null)
    };
);

fn linked_pop(o: &Value, first: bool) -> Result<Value, JexlException> {
    let l = list_of(o);
    let mut w = l.write();
    if w.items.is_empty() {
        return Err(JexlException::java("java.util.NoSuchElementException", None));
    }
    Ok(if first { w.items.remove(0) } else { w.items.pop().unwrap_or(Value::Null) })
}

fn linked_peek(o: &Value, first: bool) -> Result<Value, JexlException> {
    let items = arg_values(o);
    match if first { items.first() } else { items.last() } {
        Some(v) => Ok(v.clone()),
        None => Err(JexlException::java("java.util.NoSuchElementException", None)),
    }
}

/// The mutators a map view supports: it writes through to the map it came from.
const MAPVIEW: &[Sig] = sigs!("java.util.Collection";
    "remove"("java.lang.Object") -> "boolean" = |o, a| boolean(view_of(o).remove(&a[0]));
    "clear"() -> "void" = |o, _| {
        let view = view_of(o);
        let map = view.map().clone();
        for (k, _) in view.entries() {
            map.write().map.remove(&k);
        }
        Ok(Value::Null)
    };
);

const MAPENTRY: &[Sig] = sigs!("java.util.Map$Entry";
    "getKey"() -> "java.lang.Object" = |o, _| Ok(entry_of(o).key());
    "getValue"() -> "java.lang.Object" = |o, _| Ok(entry_of(o).value());
    "setValue"("java.lang.Object") -> "java.lang.Object" = |o, a| Ok(entry_of(o).set_value(a[0].clone()));
);

const SET: &[Sig] = sigs!("java.util.Set";
    "add"("java.lang.Object") -> "boolean" = |o, a| boolean(set_of(o).write().set.add(a[0].clone()));
    "remove"("java.lang.Object") -> "boolean" = |o, a| boolean(set_of(o).write().set.remove(&a[0]));
    "clear"() -> "void" = |o, _| {
        let s = set_of(o);
        let mut w = s.write();
        for e in w.set.iter().cloned().collect::<Vec<_>>() {
            w.set.remove(&e);
        }
        Ok(Value::Null)
    };
    "addAll"("java.util.Collection") -> "boolean" = |o, a| {
        nn(&a[0])?;
        let s = set_of(o);
        let mut changed = false;
        for e in arg_values(&a[0]) {
            changed |= s.write().set.add(e);
        }
        boolean(changed)
    };
    "removeAll"("java.util.Collection") -> "boolean" = |o, a| {
        nn(&a[0])?;
        let s = set_of(o);
        let mut changed = false;
        for e in arg_values(&a[0]) {
            changed |= s.write().set.remove(&e);
        }
        boolean(changed)
    };
    "retainAll"("java.util.Collection") -> "boolean" = |o, a| {
        nn(&a[0])?;
        let s = set_of(o);
        let keep = arg_values(&a[0]);
        let drop: Vec<Value> = s.snapshot().into_iter().filter(|e| !keep.iter().any(|k| k.java_equals(e))).collect();
        let mut w = s.write();
        for e in &drop {
            w.set.remove(e);
        }
        boolean(!drop.is_empty())
    };
    "iterator"() -> "java.util.Iterator" =
        |o, _| Ok(JIterator::new(set_iterator_class(set_of(o).kind()), arg_values(o)));
);

const MAP: &[Sig] = sigs!("java.util.Map";
    "size"() -> "int" = |o, _| int(map_of(o).len() as i32);
    "isEmpty"() -> "boolean" = |o, _| boolean(map_of(o).is_empty());
    "get"("java.lang.Object") -> "java.lang.Object" = |o, a| Ok(map_of(o).get(&a[0]).unwrap_or(Value::Null));
    "put"("java.lang.Object", "java.lang.Object") -> "java.lang.Object" =
        |o, a| map_put(o, a[0].clone(), a[1].clone());
    "remove"("java.lang.Object") -> "java.lang.Object" =
        |o, a| Ok(map_of(o).write().map.remove(&a[0]).unwrap_or(Value::Null));
    "remove"("java.lang.Object", "java.lang.Object") -> "boolean" = |o, a| {
        let m = map_of(o);
        match m.get(&a[0]) {
            Some(v) if v.java_equals(&a[1]) => {
                m.write().map.remove(&a[0]);
                boolean(true)
            }
            _ => boolean(false),
        }
    };
    "keySet"() -> "java.util.Set" = |o, _| Ok(Value::object(MapView::new(map_of(o), ViewKind::Keys)));
    "values"() -> "java.util.Collection" = |o, _| Ok(Value::object(MapView::new(map_of(o), ViewKind::Values)));
    "entrySet"() -> "java.util.Set" = |o, _| Ok(Value::object(MapView::new(map_of(o), ViewKind::Entries)));
    "containsKey"("java.lang.Object") -> "boolean" = |o, a| boolean(map_of(o).contains_key(&a[0]));
    // the Map defaults, as java.util.HashMap implements them
    "getOrDefault"("java.lang.Object", "java.lang.Object") -> "java.lang.Object" =
        |o, a| Ok(map_of(o).get(&a[0]).unwrap_or_else(|| a[1].clone()));
    // a key mapped to null counts as absent
    "putIfAbsent"("java.lang.Object", "java.lang.Object") -> "java.lang.Object" = |o, a| {
        match map_of(o).get(&a[0]) {
            Some(v) if !v.is_null() => Ok(v),
            _ => map_put(o, a[0].clone(), a[1].clone()),
        }
    };
    "replace"("java.lang.Object", "java.lang.Object") -> "java.lang.Object" = |o, a| {
        if map_of(o).contains_key(&a[0]) {
            map_put(o, a[0].clone(), a[1].clone())
        } else {
            Ok(Value::Null)
        }
    };
    // Objects.equals(current, old), and an absent key never matches even when old is null
    "replace"("java.lang.Object", "java.lang.Object", "java.lang.Object") -> "boolean" = |o, a| {
        match map_of(o).get(&a[0]) {
            Some(cur) if (cur.is_null() && a[1].is_null()) || cur.java_equals(&a[1]) => {
                map_put(o, a[0].clone(), a[2].clone())?;
                boolean(true)
            }
            _ => boolean(false),
        }
    };
    "containsValue"("java.lang.Object") -> "boolean" =
        |o, a| boolean(map_of(o).snapshot().iter().any(|(_, v)| v.java_equals(&a[0])));
    "clear"() -> "void" = |o, _| {
        let m = map_of(o);
        for (k, _) in m.snapshot() {
            m.write().map.remove(&k);
        }
        Ok(Value::Null)
    };
    "putAll"("java.util.Map") -> "void" = |o, a| {
        nn(&a[0])?;
        if let Value::Map(src) = &a[0] {
            for (k, v) in src.snapshot() {
                map_of(o).put(k, v);
            }
        }
        Ok(Value::Null)
    };
    "getOrDefault"("java.lang.Object", "java.lang.Object") -> "java.lang.Object" =
        |o, a| Ok(map_of(o).get(&a[0]).unwrap_or_else(|| a[1].clone()));
    "putIfAbsent"("java.lang.Object", "java.lang.Object") -> "java.lang.Object" = |o, a| {
        let m = map_of(o);
        match m.get(&a[0]) {
            Some(v) if !v.is_null() => Ok(v),
            _ => {
                m.put(a[0].clone(), a[1].clone());
                Ok(Value::Null)
            }
        }
    };
);

const ITERATOR: &[Sig] = sigs!("java.util.Iterator";
    "hasNext"() -> "boolean" = |o, _| boolean(matches!(o.as_host::<JIterator>(), Some(i) if i.has_next()));
    "next"() -> "java.lang.Object" = |o, _| match o.as_host::<JIterator>().and_then(|i| i.next()) {
        Some(v) => Ok(v),
        None => jthrow("java.util.NoSuchElementException", "null"),
    };
);

const CLASS: &[Sig] = sigs!("java.lang.Class";
    "getName"() -> "java.lang.String" =
        |o, _| jstring(JString::from(o.as_host::<ClassValue>().map(|c| c.name.clone()).unwrap_or_default()));
    "getSimpleName"() -> "java.lang.String" = |o, _| {
        let n = o.as_host::<ClassValue>().map(|c| c.name.clone()).unwrap_or_default();
        jstring(JString::from(n.rsplit(['.', '$']).next().unwrap_or(&n).to_string()))
    };
);

const STRINGBUILDER: &[Sig] = sigs!("java.lang.StringBuilder";
    "setLength"("int") -> "void" = |o, a| {
        let n = arg_i32(&a[0]);
        if n < 0 {
            return jthrow("java.lang.StringIndexOutOfBoundsException", &format!("Negative length: {}", n));
        }
        if n as i64 > (i32::MAX as i64) / 2 {
            return jthrow("java.lang.OutOfMemoryError", "Required length exceeds implementation limit");
        }
        if let Some(b) = o.as_host::<JavaStringBuilder>() {
            b.set_length(n as usize);
        }
        Ok(Value::Null)
    };
    "setCharAt"("int", "char") -> "void" = |o, a| {
        let b = match o.as_host::<JavaStringBuilder>() {
            Some(b) => b,
            None => return Ok(Value::Null),
        };
        let i = check_index(arg_i64(&a[0]), b.units().len())?;
        b.set_char_at(i, arg_char(&a[1]));
        Ok(Value::Null)
    };
    "append"("java.lang.Object") -> "java.lang.StringBuilder" = |o, a| {
        if let Some(b) = o.as_host::<JavaStringBuilder>() {
            b.append(&a[0].java_to_jstring());
        }
        Ok(o.clone())
    };
    "reverse"() -> "java.lang.StringBuilder" = |o, _| {
        if let Some(b) = o.as_host::<JavaStringBuilder>() {
            let mut u = b.units();
            u.reverse();
            let rev = JString::new(u);
            let mut w = b.0.lock().unwrap_or_else(|p| p.into_inner());
            w.clear();
            w.extend_from_slice(rev.units());
        }
        Ok(o.clone())
    };
);

const RANGE: &[Sig] = sigs!("org.apache.commons.jexl3.internal.IntegerRange";
    "getMin"() -> "java.lang.Number" =
        |o, _| Ok(o.as_host::<Range>().map(|r| r.get_min()).unwrap_or(Value::Null));
    "getMax"() -> "java.lang.Number" =
        |o, _| Ok(o.as_host::<Range>().map(|r| r.get_max()).unwrap_or(Value::Null));
    "size"() -> "int" = |o, _| int(o.as_host::<Range>().map(|r| r.size()).unwrap_or(0));
    "isEmpty"() -> "boolean" = |o, _| boolean(o.as_host::<Range>().map(|r| r.size() <= 0).unwrap_or(true));
    "contains"("java.lang.Object") -> "boolean" =
        |o, a| boolean(o.as_host::<Range>().map(|r| r.contains(&a[0])).unwrap_or(false));
    "iterator"() -> "java.util.Iterator" = |o, _| {
        let items: Vec<Value> = match o.as_host::<Range>() {
            // ponytail: a range iterator taken as a value is snapshot-bounded; the `for` loop
            // path (getIterator) is lazy, which is the one that matters
            Some(r) => r.iter().take(1024).collect(),
            None => Vec::new(),
        };
        Ok(JIterator::new("java.util.Iterator", items))
    };
);

/// port of: ArrayListWrapper - the five methods an array borrows, plus what it inherits and
/// therefore resolves to but cannot actually be invoked with (reflection rejects the receiver).
const ARRAY_LIST_WRAPPER: &[Sig] = sigs!("org.apache.commons.jexl3.internal.introspection.ArrayListWrapper";
    "get"("int") -> "java.lang.Object" = |o, a| list_get(o, arg_i32(&a[0]));
    "set"("int", "java.lang.Object") -> "java.lang.Object" = |o, a| {
        let i = arg_i32(&a[0]);
        let old = list_get(o, i)?;
        list_set(o, i, &a[1])?;
        Ok(old)
    };
    "size"() -> "int" = |o, _| int(arg_values(o).len() as i32);
    "indexOf"("java.lang.Object") -> "int" = |o, a| int(index_of_in(&arg_values(o), &a[0]));
    "contains"("java.lang.Object") -> "boolean" = |o, a| boolean(index_of_in(&arg_values(o), &a[0]) >= 0);
);

/// The methods an array inherits through ArrayListWrapper: they resolve, then
/// `Method.invoke(array, ...)` rejects the receiver.
/// These resolve on an array but never run: MethodExecutor.invoke passes the array itself as the
/// receiver and reflection rejects it, so the body is unreachable by construction.
fn not_on_an_array(_obj: &Value, _args: &[Value]) -> Result<Value, JexlException> {
    iae("object of type array is not an instance of a collection")
}

const ARRAY_INHERITED: &[Sig] = sigs!("java.util.AbstractList";
    "add"("java.lang.Object") -> "boolean" = not_on_an_array;
    "add"("int", "java.lang.Object") -> "void" = not_on_an_array;
    "remove"("int") -> "java.lang.Object" = not_on_an_array;
    "clear"() -> "void" = not_on_an_array;
    "iterator"() -> "java.util.Iterator" = not_on_an_array;
    "listIterator"() -> "java.util.ListIterator" = not_on_an_array;
    "lastIndexOf"("java.lang.Object") -> "int" = not_on_an_array;
    "subList"("int", "int") -> "java.util.List" = not_on_an_array;
    "addAll"("int", "java.util.Collection") -> "boolean" = not_on_an_array;
);

const ARRAY_INHERITED_LIST: &[Sig] = sigs!("java.util.List";
    "isEmpty"() -> "boolean" = not_on_an_array;
    "addAll"("java.util.Collection") -> "boolean" = not_on_an_array;
    "containsAll"("java.util.Collection") -> "boolean" = not_on_an_array;
    "removeAll"("java.util.Collection") -> "boolean" = not_on_an_array;
    "retainAll"("java.util.Collection") -> "boolean" = not_on_an_array;
    "remove"("java.lang.Object") -> "boolean" = not_on_an_array;
    "toArray"() -> "java.lang.Object[]" = not_on_an_array;
    "toArray"("java.lang.Object[]") -> "java.lang.Object[]" = not_on_an_array;
);

const ARRAY_INHERITED_COLLECTION: &[Sig] = sigs!("java.util.Collection";
    "toArray"("java.util.function.IntFunction") -> "java.lang.Object[]" = not_on_an_array;
    "stream"() -> "java.util.stream.Stream" = not_on_an_array;
);

// ---------------------------------------------------------------------------------------------
// Constructors

const CTORS: &[Sig] = sigs!("";
    "java.util.ArrayList"() -> "java.util.ArrayList" = |_, _| Ok(Value::List(JList::array_list(Vec::new())));
    "java.util.ArrayList"("int") -> "java.util.ArrayList" = |_, a| {
        // `new Object[initialCapacity]`, allocated there and then
        let n = arg_i32(&a[0]);
        if n < 0 {
            return java_error("java.lang.IllegalArgumentException", format!("Illegal Capacity: {}", n));
        }
        vm_array(n)?;
        Ok(Value::List(JList::array_list(Vec::new())))
    };
    "java.util.ArrayList"("java.util.Collection") -> "java.util.ArrayList" = |_, a| {
        if a[0].is_null() {
            return npe("Cannot invoke \"java.util.Collection.toArray()\" because \"c\" is null");
        }
        Ok(Value::List(JList::array_list(arg_values(&a[0]))))
    };
    "java.util.LinkedList"() -> "java.util.LinkedList" =
        |_, _| Ok(Value::List(JList::new(ListKind::LinkedList, Vec::new())));
    "java.util.LinkedList"("java.util.Collection") -> "java.util.LinkedList" = |_, a| {
        if a[0].is_null() {
            return npe("Cannot invoke \"java.util.Collection.toArray()\" because \"c\" is null");
        }
        Ok(Value::List(JList::new(ListKind::LinkedList, arg_values(&a[0]))))
    };
    "java.util.HashMap"() -> "java.util.HashMap" = |_, _| Ok(Value::Map(JMap::hash_map()));
    "java.util.HashMap"("int") -> "java.util.HashMap" =
        |_, a| Ok(Value::Map(JMap::new(MapKind::HashMap, { let (c, f) = capacity_and_load(&a[0], None)?; JHashMap::with_capacity_and_load_factor(c, f, false) })));
    "java.util.HashMap"("int", "float") -> "java.util.HashMap" =
        |_, a| Ok(Value::Map(JMap::new(MapKind::HashMap, { let (c, f) = capacity_and_load(&a[0], Some(&a[1]))?; JHashMap::with_capacity_and_load_factor(c, f, false) })));
    "java.util.HashMap"("java.util.Map") -> "java.util.HashMap" = |_, a| copy_map(&a[0], MapKind::HashMap);
    "java.util.LinkedHashMap"() -> "java.util.LinkedHashMap" =
        |_, _| Ok(Value::Map(JMap::new(MapKind::LinkedHashMap, JHashMap::new_linked())));
    "java.util.LinkedHashMap"("int") -> "java.util.LinkedHashMap" =
        |_, a| Ok(Value::Map(JMap::new(MapKind::LinkedHashMap, { let (c, f) = capacity_and_load(&a[0], None)?; JHashMap::with_capacity_and_load_factor(c, f, true) })));
    "java.util.LinkedHashMap"("int", "float") -> "java.util.LinkedHashMap" =
        |_, a| Ok(Value::Map(JMap::new(MapKind::LinkedHashMap, { let (c, f) = capacity_and_load(&a[0], Some(&a[1]))?; JHashMap::with_capacity_and_load_factor(c, f, true) })));
    "java.util.LinkedHashMap"("java.util.Map") -> "java.util.LinkedHashMap" =
        |_, a| copy_map(&a[0], MapKind::LinkedHashMap);
    "java.util.HashSet"() -> "java.util.HashSet" = |_, _| Ok(Value::Set(JSet::new(SetKind::HashSet, JHashSet::new())));
    "java.util.HashSet"("int") -> "java.util.HashSet" = |_, a| {
        let (c, f) = capacity_and_load(&a[0], None)?;
        Ok(Value::Set(JSet::new(SetKind::HashSet, JHashSet::with_capacity_and_load_factor(c, f, false))))
    };
    "java.util.HashSet"("int", "float") -> "java.util.HashSet" = |_, a| {
        let (c, f) = capacity_and_load(&a[0], Some(&a[1]))?;
        Ok(Value::Set(JSet::new(SetKind::HashSet, JHashSet::with_capacity_and_load_factor(c, f, false))))
    };
    "java.util.HashSet"("java.util.Collection") -> "java.util.HashSet" = |_, a| copy_set(&a[0], SetKind::HashSet);
    "java.util.LinkedHashSet"() -> "java.util.LinkedHashSet" =
        |_, _| Ok(Value::Set(JSet::new(SetKind::LinkedHashSet, JHashSet::new_linked())));
    "java.util.LinkedHashSet"("int") -> "java.util.LinkedHashSet" = |_, a| {
        let (c, f) = capacity_and_load(&a[0], None)?;
        Ok(Value::Set(JSet::new(SetKind::LinkedHashSet, JHashSet::with_capacity_and_load_factor(c, f, true))))
    };
    "java.util.LinkedHashSet"("int", "float") -> "java.util.LinkedHashSet" = |_, a| {
        let (c, f) = capacity_and_load(&a[0], Some(&a[1]))?;
        Ok(Value::Set(JSet::new(SetKind::LinkedHashSet, JHashSet::with_capacity_and_load_factor(c, f, true))))
    };
    "java.util.LinkedHashSet"("java.util.Collection") -> "java.util.LinkedHashSet" =
        |_, a| copy_set(&a[0], SetKind::LinkedHashSet);
    "java.lang.StringBuilder"() -> "java.lang.StringBuilder" = |_, _| Ok(JavaStringBuilder::new(Vec::new()));
    "java.lang.StringBuilder"("int") -> "java.lang.StringBuilder" = |_, a| {
        // `value = new byte[capacity]`
        let n = arg_i32(&a[0]);
        if n < 0 {
            return java_error("java.lang.NegativeArraySizeException", n.to_string());
        }
        vm_array(n)?;
        Ok(JavaStringBuilder::new(Vec::new()))
    };
    "java.lang.StringBuilder"("java.lang.String") -> "java.lang.StringBuilder" = |_, a| {
        if a[0].is_null() {
            return npe("Cannot invoke \"String.length()\" because \"str\" is null");
        }
        Ok(JavaStringBuilder::new(arg_jstring(&a[0]).units().to_vec()))
    };
    "java.lang.String"() -> "java.lang.String" = |_, _| jstring(JString::empty());
    "java.lang.String"("java.lang.String") -> "java.lang.String" = |_, a| {
        if a[0].is_null() {
            return npe0();
        }
        jstring(arg_jstring(&a[0]))
    };
    "java.lang.String"("byte[]") -> "java.lang.String" = |_, a| {
        nn(&a[0])?;
        let bytes: Vec<u8> = arg_values(&a[0]).iter().map(|v| arg_i64(v) as u8).collect();
        jstring(JString::from(String::from_utf8_lossy(&bytes).to_string()))
    };
    "java.lang.String"("java.lang.StringBuilder") -> "java.lang.String" = |_, a| jstring(a[0].java_to_jstring());
    "java.lang.String"("char[]") -> "java.lang.String" = |_, a| {
        let u: Vec<u16> = arg_values(&a[0]).iter().map(arg_char).collect();
        jstring(JString::new(u))
    };
    "java.lang.Integer"("int") -> "java.lang.Integer" = |_, a| int(arg_i32(&a[0]));
    "java.lang.Integer"("java.lang.String") -> "java.lang.Integer" = |_, a| parse_int(&a[0], 10);
    "java.lang.Long"("long") -> "java.lang.Long" = |_, a| Ok(Value::Long(arg_i64(&a[0])));
    "java.lang.Long"("java.lang.String") -> "java.lang.Long" = |_, a| parse_long(&a[0], 10);
    "java.lang.Short"("short") -> "java.lang.Short" = |_, a| Ok(Value::Short(arg_i64(&a[0]) as i16));
    "java.lang.Short"("java.lang.String") -> "java.lang.Short" = |_, a| parse_short(&a[0], 10);
    "java.lang.Byte"("byte") -> "java.lang.Byte" = |_, a| Ok(Value::Byte(arg_i64(&a[0]) as i8));
    "java.lang.Byte"("java.lang.String") -> "java.lang.Byte" = |_, a| parse_byte(&a[0], 10);
    "java.lang.Double"("double") -> "java.lang.Double" = |_, a| Ok(Value::Double(arg_f64(&a[0])));
    "java.lang.Double"("java.lang.String") -> "java.lang.Double" = |_, a| {
        parse_double(&a[0])
    };
    "java.lang.Float"("float") -> "java.lang.Float" = |_, a| Ok(Value::Float(arg_f32(&a[0])));
    "java.lang.Float"("double") -> "java.lang.Float" = |_, a| Ok(Value::Float(arg_f64(&a[0]) as f32));
    "java.lang.Float"("java.lang.String") -> "java.lang.Float" = |_, a| {
        parse_float(&a[0])
    };
    "java.lang.Boolean"("boolean") -> "java.lang.Boolean" = |_, a| boolean(arg_bool(&a[0]));
    "java.lang.Boolean"("java.lang.String") -> "java.lang.Boolean" = |_, a| {
        if a[0].is_null() {
            return boolean(false);
        }
        boolean(compare_ignore_case(&arg_jstring(&a[0]), &JString::from("true")) == 0)
    };
    "java.lang.Character"("char") -> "java.lang.Character" = |_, a| Ok(Value::Character(arg_char(&a[0])));
    "java.math.BigInteger"("java.lang.String") -> "java.math.BigInteger" = |_, a| {
        if a[0].is_null() {
            return npe("Cannot invoke \"String.length()\" because \"val\" is null");
        }
        number::parse_big_integer(&arg_jstring(&a[0]).to_rust(), 10).map(Value::big_integer).or_else(nfe)
    };
    "java.math.BigInteger"("byte[]") -> "java.math.BigInteger" = |_, a| {
        nn(&a[0])?;
        let bytes: Vec<u8> = arg_values(&a[0]).iter().map(|v| arg_i64(v) as u8).collect();
        if bytes.is_empty() {
            return jthrow("java.lang.NumberFormatException", "Zero length BigInteger");
        }
        Ok(Value::big_integer(BigInt::from_signed_bytes_be(&bytes)))
    };
    "java.math.BigInteger"("int", "byte[]") -> "java.math.BigInteger" = |_, _| npe0();
    "java.math.BigDecimal"("java.lang.String") -> "java.math.BigDecimal" = |_, a| {
        if a[0].is_null() {
            return npe0();
        }
        match BigDecimal::parse(&arg_jstring(&a[0]).to_rust()) {
            Ok(v) => Ok(Value::big_decimal(v)),
            Err(e) => math(e),
        }
    };
    "java.math.BigDecimal"("int") -> "java.math.BigDecimal" =
        |_, a| Ok(Value::big_decimal(BigDecimal::from_i64(arg_i32(&a[0]) as i64)));
    "java.math.BigDecimal"("long") -> "java.math.BigDecimal" =
        |_, a| Ok(Value::big_decimal(BigDecimal::from_i64(arg_i64(&a[0]))));
    "java.math.BigDecimal"("double") -> "java.math.BigDecimal" =
        |_, a| match BigDecimal::from_double_exact(arg_f64(&a[0])) {
            Ok(v) => Ok(Value::big_decimal(v)),
            Err(e) => math(e),
        };
    "java.math.BigDecimal"("char[]") -> "java.math.BigDecimal" = |_, a| {
        nn(&a[0])?;
        let u: Vec<u16> = arg_values(&a[0]).iter().map(arg_char).collect();
        match BigDecimal::parse_units(&u) {
            Ok(v) => Ok(Value::big_decimal(v)),
            Err(e) => math(e),
        }
    };
    "java.math.BigDecimal"("java.math.BigInteger") -> "java.math.BigDecimal" =
        |_, a| Ok(Value::big_decimal(BigDecimal::from_bigint(&arg_bigint(&a[0]))));
    "java.lang.Object"() -> "java.lang.Object" = |_, _| Ok(Value::object(PlainObject));
);

/// port of: java.lang.Object - `new Object()` with nothing but its identity.
pub struct PlainObject;

impl crate::value::HostObject for PlainObject {
    fn class_name(&self) -> String {
        "java.lang.Object".into()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn copy_map(src: &Value, kind: MapKind) -> Result<Value, JexlException> {
    if src.is_null() {
        return npe("Cannot invoke \"java.util.Map.size()\" because \"m\" is null");
    }
    let mut m = if kind == MapKind::LinkedHashMap { JHashMap::new_linked() } else { JHashMap::new() };
    if let Value::Map(s) = src {
        for (k, v) in s.snapshot() {
            m.put(k, v);
        }
    }
    Ok(Value::Map(JMap::new(kind, m)))
}

fn copy_set(src: &Value, kind: SetKind) -> Result<Value, JexlException> {
    if src.is_null() {
        return npe("Cannot invoke \"java.util.Collection.size()\" because \"c\" is null");
    }
    let mut s = if kind == SetKind::LinkedHashSet { JHashSet::new_linked() } else { JHashSet::new() };
    for e in arg_values(src) {
        s.add(e);
    }
    Ok(Value::Set(JSet::new(kind, s)))
}

// ---------------------------------------------------------------------------------------------
// Public static fields (ClassMap's field cache for the JDK types)

fn static_field(class: &str, name: &str) -> Option<Value> {
    let v = match (class, name) {
        ("java.lang.Integer", "MAX_VALUE") => Value::Integer(i32::MAX),
        ("java.lang.Integer", "MIN_VALUE") => Value::Integer(i32::MIN),
        ("java.lang.Integer", "SIZE") => Value::Integer(32),
        ("java.lang.Integer", "BYTES") => Value::Integer(4),
        ("java.lang.Integer", "TYPE") => ClassValue::primitive("int"),
        ("java.lang.Long", "MAX_VALUE") => Value::Long(i64::MAX),
        ("java.lang.Long", "MIN_VALUE") => Value::Long(i64::MIN),
        ("java.lang.Long", "SIZE") => Value::Integer(64),
        ("java.lang.Long", "BYTES") => Value::Integer(8),
        ("java.lang.Long", "TYPE") => ClassValue::primitive("long"),
        ("java.lang.Short", "MAX_VALUE") => Value::Short(i16::MAX),
        ("java.lang.Short", "MIN_VALUE") => Value::Short(i16::MIN),
        ("java.lang.Short", "SIZE") => Value::Integer(16),
        ("java.lang.Short", "BYTES") => Value::Integer(2),
        ("java.lang.Short", "TYPE") => ClassValue::primitive("short"),
        ("java.lang.Byte", "MAX_VALUE") => Value::Byte(i8::MAX),
        ("java.lang.Byte", "MIN_VALUE") => Value::Byte(i8::MIN),
        ("java.lang.Byte", "SIZE") => Value::Integer(8),
        ("java.lang.Byte", "BYTES") => Value::Integer(1),
        ("java.lang.Byte", "TYPE") => ClassValue::primitive("byte"),
        ("java.lang.Double", "MAX_VALUE") => Value::Double(f64::MAX),
        ("java.lang.Double", "MIN_VALUE") => Value::Double(f64::from_bits(1)),
        ("java.lang.Double", "POSITIVE_INFINITY") => Value::Double(f64::INFINITY),
        ("java.lang.Double", "NEGATIVE_INFINITY") => Value::Double(f64::NEG_INFINITY),
        ("java.lang.Double", "SIZE") => Value::Integer(64),
        ("java.lang.Double", "BYTES") => Value::Integer(8),
        ("java.lang.Double", "TYPE") => ClassValue::primitive("double"),
        ("java.lang.Float", "MAX_VALUE") => Value::Float(f32::MAX),
        ("java.lang.Float", "MIN_VALUE") => Value::Float(f32::from_bits(1)),
        ("java.lang.Float", "POSITIVE_INFINITY") => Value::Float(f32::INFINITY),
        ("java.lang.Float", "NEGATIVE_INFINITY") => Value::Float(f32::NEG_INFINITY),
        ("java.lang.Float", "SIZE") => Value::Integer(32),
        ("java.lang.Float", "BYTES") => Value::Integer(4),
        ("java.lang.Float", "TYPE") => ClassValue::primitive("float"),
        ("java.lang.Character", "MAX_VALUE") => Value::Character(0xFFFF),
        ("java.lang.Character", "MIN_VALUE") => Value::Character(0),
        ("java.lang.Character", "SIZE") => Value::Integer(16),
        ("java.lang.Character", "BYTES") => Value::Integer(2),
        ("java.lang.Character", "TYPE") => ClassValue::primitive("char"),
        ("java.lang.Boolean", "TRUE") => Value::Boolean(true),
        ("java.lang.Boolean", "FALSE") => Value::Boolean(false),
        ("java.lang.Boolean", "TYPE") => ClassValue::primitive("boolean"),
        ("java.math.BigInteger", "ZERO") => Value::big_integer(BigInt::zero()),
        ("java.math.BigInteger", "ONE") => Value::big_integer(BigInt::one()),
        ("java.math.BigInteger", "TWO") => Value::big_integer(BigInt::from(2)),
        ("java.math.BigInteger", "TEN") => Value::big_integer(BigInt::from(10)),
        ("java.math.BigDecimal", "ZERO") => Value::big_decimal(BigDecimal::zero()),
        ("java.math.BigDecimal", "ONE") => Value::big_decimal(BigDecimal::one()),
        ("java.math.BigDecimal", "TEN") => Value::big_decimal(BigDecimal::ten()),
        _ => return None,
    };
    Some(v)
}

// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::introspection::POJO;

    fn shim() -> JdkShim {
        JdkShim::default()
    }

    fn call(obj: &Value, name: &str, args: &[Value]) -> Result<Value, JexlException> {
        let s = shim();
        match s.get_method(obj, name, args) {
            Some(m) => m.invoke(obj, args),
            None => jthrow("unsolvable", name),
        }
    }

    fn get(obj: &Value, id: Value) -> Option<Value> {
        shim().get_property_get(obj, &id).and_then(|g| g.invoke(obj).ok())
    }

    #[test]
    fn shim_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<JdkShim>();
        assert_send_sync::<ClassValue>();
        assert_send_sync::<JIterator>();
        assert_send_sync::<JavaStringBuilder>();
    }

    // The oracle protocol cannot encode a range as a target, so ranges are covered here.
    #[test]
    fn ranges_iterate_and_answer_their_bean_properties() {
        let r = Value::object(Range::create(Width::Integer, 1, 3));
        let values: Vec<Value> = shim().get_iterator(&r).expect("iterator").collect();
        assert_eq!(values.len(), 3);
        assert_eq!(values[0].java_to_string(), "1");
        assert_eq!(values[2].java_to_string(), "3");
        assert_eq!(call(&r, "size", &[]).unwrap().java_to_string(), "3");
        assert_eq!(call(&r, "contains", &[Value::Integer(2)]).unwrap().java_to_string(), "true");
        assert_eq!(call(&r, "contains", &[Value::Integer(9)]).unwrap().java_to_string(), "false");
        // the bean properties JEXL exposes on a range
        assert_eq!(get(&r, Value::string("min")).unwrap().java_to_string(), "1");
        assert_eq!(get(&r, Value::string("max")).unwrap().java_to_string(), "3");
        assert_eq!(get(&r, Value::string("empty")).unwrap().java_to_string(), "false");
        let it = call(&r, "iterator", &[]).unwrap();
        assert_eq!(call(&it, "next", &[]).unwrap().java_to_string(), "1");

        let d = Value::object(Range::create(Width::Long, 3, 1));
        assert!(matches!(d.as_host::<Range>().unwrap().direction, Direction::Descending));
        let values: Vec<Value> = shim().get_iterator(&d).expect("iterator").collect();
        assert_eq!(values.iter().map(|v| v.java_to_string()).collect::<Vec<_>>(), ["3", "2", "1"]);
    }

    #[test]
    fn iterators_are_values_and_iterate_once() {
        let list = Value::List(JList::array_list(vec![Value::Integer(1), Value::Integer(2)]));
        let it = call(&list, "iterator", &[]).unwrap();
        assert_eq!(it.class_name(), "java.util.ArrayList$Itr");
        assert_eq!(call(&it, "hasNext", &[]).unwrap().java_to_string(), "true");
        assert_eq!(call(&it, "next", &[]).unwrap().java_to_string(), "1");
        // getIterator over an Iterator hands back what is left of it
        let rest: Vec<Value> = shim().get_iterator(&it).expect("iterator").collect();
        assert_eq!(rest.len(), 1);
        assert_eq!(call(&it, "hasNext", &[]).unwrap().java_to_string(), "false");
        assert!(call(&it, "next", &[]).is_err());
    }

    // StringBuilder can be constructed through the oracle but never handed back as a target.
    #[test]
    fn string_builder_appends_in_place() {
        let b = shim()
            .get_constructor(&Value::string("java.lang.StringBuilder"), &[Value::string("ab")])
            .expect("ctor")
            .invoke(&Value::string("java.lang.StringBuilder"), &[Value::string("ab")])
            .expect("new");
        assert_eq!(b.java_to_string(), "ab");
        let same = call(&b, "append", &[Value::Integer(7)]).unwrap();
        assert!(same.same_instance(&b));
        assert_eq!(b.java_to_string(), "ab7");
        call(&b, "reverse", &[]).unwrap();
        assert_eq!(b.java_to_string(), "7ba");
        assert_eq!(call(&b, "length", &[]).unwrap().java_to_string(), "3");
        assert_eq!(call(&b, "charAt", &[Value::Integer(0)]).unwrap().java_to_string(), "7");
    }

    #[test]
    fn class_values_render_like_java() {
        let c = call(&Value::string("x"), "getClass", &[]).unwrap();
        assert_eq!(c.java_to_string(), "class java.lang.String");
        assert_eq!(call(&c, "getName", &[]).unwrap().java_to_string(), "java.lang.String");
        assert_eq!(call(&c, "getSimpleName", &[]).unwrap().java_to_string(), "String");
        let t = get(&Value::Integer(1), Value::string("TYPE")).unwrap();
        assert_eq!(t.java_to_string(), "int");
        assert!(c.java_equals(&call(&Value::string("y"), "getClass", &[]).unwrap()));
        assert!(!c.java_equals(&t));
        assert!(!c.java_equals(&Value::Integer(1)));
    }

    struct Host;

    impl HostIntrospector for Host {
        fn get_method(&self, _obj: &Value, name: &str, _args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
            (name == "hostOnly").then(|| Arc::new(Answer) as Arc<dyn JexlMethod>)
        }
        fn get_property_get(&self, _obj: &Value, id: &Value) -> Option<Arc<dyn JexlPropertyGet>> {
            (cast_string(id).as_deref() == Some("hostOnly")).then(|| Arc::new(Answer) as Arc<dyn JexlPropertyGet>)
        }
        fn get_property_set(&self, _obj: &Value, id: &Value, _arg: &Value) -> Option<Arc<dyn JexlPropertySet>> {
            (cast_string(id).as_deref() == Some("hostOnly")).then(|| Arc::new(Answer) as Arc<dyn JexlPropertySet>)
        }
        fn get_constructor(&self, _handle: &Value, _args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
            None
        }
    }

    struct Answer;

    impl JexlMethod for Answer {
        fn invoke(&self, _obj: &Value, _params: &[Value]) -> Result<Value, JexlException> {
            Ok(Value::Integer(42))
        }
    }

    impl JexlPropertyGet for Answer {
        fn invoke(&self, _obj: &Value) -> Result<Value, JexlException> {
            Ok(Value::Integer(42))
        }
    }

    impl JexlPropertySet for Answer {
        fn invoke(&self, _obj: &Value, _arg: &Value) -> Result<Value, JexlException> {
            Ok(Value::Integer(42))
        }
    }

    #[test]
    fn host_introspector_is_consulted_after_the_jdk_tables() {
        let s = JdkShim::default().with_hosts(Arc::new(Host));
        let v = Value::string("x");
        // the JDK tables still win
        assert_eq!(s.get_method(&v, "length", &[]).expect("length").invoke(&v, &[]).unwrap().java_to_string(), "1");
        assert_eq!(s.get_method(&v, "hostOnly", &[]).expect("host").invoke(&v, &[]).unwrap().java_to_string(), "42");
        assert!(s.get_method(&v, "neither", &[]).is_none());
        assert_eq!(
            s.get_property_get(&v, &Value::string("hostOnly")).expect("host").invoke(&v).unwrap().java_to_string(),
            "42"
        );
        assert!(s.get_constructor(&Value::string("com.example.Nope"), &[]).is_none());
        assert_eq!(
            s.get_property_set(&v, &Value::string("hostOnly"), &Value::Null)
                .expect("host set")
                .invoke(&v, &Value::Null)
                .unwrap()
                .java_to_string(),
            "42"
        );
        assert!(s.get_property_set(&v, &Value::string("neither"), &Value::Null).is_none());
    }

    #[test]
    fn resolver_order_follows_the_strategy() {
        let map = Value::Map(JMap::hash_map());
        if let Value::Map(m) = &map {
            m.put(Value::string("empty"), Value::string("mapped"));
        }
        let s = shim();
        // PROPERTY_GET keeps the POJO order, so isEmpty() shadows the "empty" key
        assert_eq!(s.get_resolvers(Some(JexlOperator::PropertyGet), &map), &POJO);
        let pojo = s.get_resolvers(Some(JexlOperator::PropertyGet), &map);
        let g = s.get_property_get_with(pojo, &map, &Value::string("empty")).expect("bean get");
        assert_eq!(g.invoke(&map).unwrap().java_to_string(), "false");
        // ARRAY_GET - and a bare getPropertyGet on a Map - switch to the MAP order, so the key wins
        assert_eq!(s.get_resolvers(Some(JexlOperator::ArrayGet), &map), &super::super::MAP);
        let resolvers = s.get_resolvers(Some(JexlOperator::ArrayGet), &map);
        let g = s.get_property_get_with(resolvers, &map, &Value::string("empty")).expect("map get");
        assert_eq!(g.invoke(&map).unwrap().java_to_string(), "mapped");
        assert_eq!(get(&map, Value::string("empty")).unwrap().java_to_string(), "mapped");
    }

    #[test]
    fn the_duck_and_bean_resolvers_answer_when_the_strategy_asks_for_them() {
        let s = shim();
        let map = Value::Map(JMap::hash_map());
        if let Value::Map(m) = &map {
            m.put(Value::string("a"), Value::Integer(1));
        }
        // DUCK: get(identifier) / put(key, value) by name, ahead of the MAP executor
        let duck = [PropertyResolver::Duck];
        let g = s.get_property_get_with(&duck, &map, &Value::string("a")).expect("duck get");
        assert_eq!(g.invoke(&map).unwrap().java_to_string(), "1");
        assert!(matches!(g.try_invoke(&map, &Value::string("a")), Ok(TryResult::Value(_))));
        assert!(matches!(g.try_invoke(&map, &Value::string("b")), Ok(TryResult::Failed)));
        let p = s.get_property_set_with(&duck, &map, &Value::string("b"), &Value::Integer(2)).expect("duck set");
        p.invoke(&map, &Value::Integer(2)).unwrap();
        assert_eq!(map.java_to_string(), "{a=1, b=2}");
        assert!(matches!(p.try_invoke(&map, &Value::string("b"), &Value::Integer(3)), Ok(TryResult::Value(_))));
        assert!(matches!(p.try_invoke(&map, &Value::string("b"), &Value::string("x")), Ok(TryResult::Failed)));
        assert!(p.is_cacheable() && g.is_cacheable());
        // a non-String identifier reaches DuckGetExecutor through the property spelling
        assert!(s.get_property_get_with(&duck, &map, &Value::Integer(0)).is_some());

        // PROPERTY: setLength(int) is a JavaBean setter, so `sb.length = 2` resolves
        let sb = JavaStringBuilder::new("abcd".encode_utf16().collect());
        let set = s
            .get_property_set_with(&[PropertyResolver::Property], &sb, &Value::string("length"), &Value::Integer(2))
            .expect("bean set");
        set.invoke(&sb, &Value::Integer(2)).unwrap();
        assert_eq!(sb.java_to_string(), "ab");
        assert!(matches!(set.try_invoke(&sb, &Value::string("length"), &Value::Integer(1)), Ok(TryResult::Value(_))));
        assert_eq!(sb.java_to_string(), "a");
        assert!(matches!(set.try_invoke(&sb, &Value::string("nope"), &Value::Integer(1)), Ok(TryResult::Failed)));
        assert!(set.is_cacheable());
        assert!(call(&sb, "setCharAt", &[Value::Integer(0), Value::Character(b'z' as u16)]).is_ok());
        assert_eq!(sb.java_to_string(), "z");
        assert!(call(&sb, "setCharAt", &[Value::Integer(9), Value::Character(0)]).is_err());
        assert!(call(&sb, "setLength", &[Value::Integer(-1)]).is_err());

        // FIELD: a public static constant, and the cached retry
        let f = s.get_property_get_with(&[PropertyResolver::Field], &Value::Integer(0), &Value::string("MAX_VALUE"));
        let f = f.expect("field");
        assert!(matches!(f.try_invoke(&Value::Integer(7), &Value::string("MAX_VALUE")), Ok(TryResult::Value(_))));
        assert!(matches!(f.try_invoke(&Value::Long(7), &Value::string("MAX_VALUE")), Ok(TryResult::Failed)));
        // CONTAINER needs a get<X>(...) with no get<X>() beside it: Integer has getInteger(String),
        // so `integer` is an indexed property, and `x` is nothing
        assert!(s
            .get_property_get_with(&[PropertyResolver::Container], &Value::Integer(0), &Value::string("x"))
            .is_none());
        assert!(s
            .get_property_get_with(&[PropertyResolver::Container], &Value::Integer(0), &Value::string("integer"))
            .is_some());
    }

    #[test]
    fn an_index_identifier_may_be_any_number() {
        let s = shim();
        let list = Value::List(JList::array_list(vec![Value::Integer(7), Value::Integer(8)]));
        for id in [
            Value::Byte(1),
            Value::Short(1),
            Value::Integer(1),
            Value::Long(1),
            Value::Float(1.9),
            Value::Double(1.9),
            Value::big_integer(BigInt::one()),
            Value::big_decimal(BigDecimal::one()),
        ] {
            let g = s.get_property_get(&list, &id).expect("list get");
            assert_eq!(g.invoke(&list).unwrap().java_to_string(), "8", "{:?}", id);
        }
        assert!(s.get_property_get(&list, &Value::Boolean(true)).is_none());
    }

    #[test]
    fn array_classes_name_themselves_like_java() {
        for (c, name, simple) in [
            (Component::Boolean, "[Z", "boolean"),
            (Component::Byte, "[B", "byte"),
            (Component::Short, "[S", "short"),
            (Component::Int, "[I", "int"),
            (Component::Long, "[J", "long"),
            (Component::Float, "[F", "float"),
            (Component::Double, "[D", "double"),
            (Component::Char, "[C", "char"),
            (Component::object(), "[Ljava.lang.Object;", "java.lang.Object"),
        ] {
            let a = Value::Array(JArray::new(c.clone(), vec![]));
            let jc = JClass::of(&a);
            assert_eq!(jc.name(), name);
            assert_eq!(jc.component().unwrap().name(), simple);
            assert_eq!(class_component(jc.component().unwrap()), c);
        }
        // an array of arrays
        let nested = JClass::Array(Box::new(JClass::Array(Box::new(JClass::Prim("int")))));
        assert_eq!(nested.name(), "[[I");
    }

    #[test]
    fn nothing_panics_on_nonsense() {
        let s = shim();
        let weird = [
            Value::Null,
            Value::string(""),
            Value::Array(JArray::new(Component::Int, vec![])),
            Value::List(JList::array_list(vec![])),
            Value::Map(JMap::hash_map()),
            Value::object(PlainObject),
        ];
        for obj in &weird {
            for name in ["", "get", "toString", "size", "equals", "\u{1f600}"] {
                for args in [vec![], vec![Value::Null], vec![Value::Null, Value::Null]] {
                    if let Some(m) = s.get_method(obj, name, &args) {
                        let _ = m.invoke(obj, &args);
                        let _ = m.try_invoke(name, obj, &args);
                    }
                    if let Some(g) = s.get_property_get(obj, &Value::string(name)) {
                        let _ = g.invoke(obj);
                        let _ = g.try_invoke(obj, &Value::string(name));
                    }
                    if let Some(p) = s.get_property_set(obj, &Value::string(name), &Value::Null) {
                        let _ = p.invoke(obj, &Value::Null);
                        let _ = p.try_invoke(obj, &Value::string(name), &Value::Null);
                    }
                    let _ = s.get_constructor(obj, &args);
                    let _ = s.get_iterator(obj);
                }
            }
        }
    }

    #[test]
    fn cached_executors_retry_through_try_invoke() {
        let s = shim();
        let list = Value::List(JList::array_list(vec![Value::Integer(1), Value::Integer(2)]));
        let m = s.get_method(&list, "get", &[Value::Integer(0)]).expect("get");
        assert!(matches!(m.try_invoke("get", &list, &[Value::Integer(1)]), Ok(TryResult::Value(_))));
        // a different name or a different argument shape falls back to a fresh resolution
        assert!(matches!(m.try_invoke("set", &list, &[Value::Integer(1)]), Ok(TryResult::Failed)));
        assert!(matches!(m.try_invoke("get", &list, &[Value::string("x")]), Ok(TryResult::Failed)));
        assert_eq!(m.return_type().as_deref(), Some("java.lang.Object"));
        assert!(m.is_cacheable());
        // JexlMethod::invoke packs a trailing vararg itself when the caller did not
        let text = Value::string("x");
        let join = s.get_method(&text, "join", &[Value::string("-"), Value::string("a")]).expect("join");
        let joined = join.invoke(&text, &[Value::string("-"), Value::string("a")]).unwrap();
        assert_eq!(joined.java_to_string(), "a");

        let g = s.get_property_get(&list, &Value::Integer(0)).expect("list get");
        assert!(matches!(g.try_invoke(&list, &Value::Integer(1)), Ok(TryResult::Value(_))));
        assert!(matches!(g.try_invoke(&list, &Value::string("x")), Ok(TryResult::Failed)));
        assert!(g.is_cacheable());

        // a bean getter (isEmpty) and the map/list executors all cache and retry
        let bean = s
            .get_property_get_with(&[PropertyResolver::Property], &list, &Value::string("empty"))
            .expect("bean get");
        assert!(matches!(bean.try_invoke(&list, &Value::string("empty")), Ok(TryResult::Value(_))));
        assert!(matches!(bean.try_invoke(&list, &Value::string("other")), Ok(TryResult::Failed)));
        assert!(bean.is_cacheable());
        let ls = s.get_property_set(&list, &Value::Integer(0), &Value::Integer(5)).expect("list set");
        assert!(matches!(ls.try_invoke(&list, &Value::Integer(1), &Value::Integer(6)), Ok(TryResult::Value(_))));
        assert!(matches!(ls.try_invoke(&list, &Value::string("x"), &Value::Integer(6)), Ok(TryResult::Failed)));
        assert!(ls.is_cacheable());
        assert_eq!(list.java_to_string(), "[1, 6]");

        let map = Value::Map(JMap::hash_map());
        let mg = s.get_property_get(&map, &Value::string("a")).expect("map get");
        assert!(matches!(mg.try_invoke(&map, &Value::string("b")), Ok(TryResult::Value(_))));
        assert!(matches!(mg.try_invoke(&map, &Value::Integer(1)), Ok(TryResult::Failed)));
        assert!(mg.is_cacheable());
        let fg = s.get_property_get(&Value::Integer(0), &Value::string("MAX_VALUE")).expect("field");
        assert!(fg.is_cacheable());
        let p = s.get_property_set(&map, &Value::string("a"), &Value::Integer(1)).expect("map set");
        assert!(matches!(p.try_invoke(&map, &Value::string("b"), &Value::Integer(2)), Ok(TryResult::Value(_))));
        assert!(matches!(p.try_invoke(&map, &Value::Integer(3), &Value::Integer(2)), Ok(TryResult::Failed)));
        assert_eq!(map.java_to_string(), "{b=2}");
    }
}
