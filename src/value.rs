// port of: java.lang.Object as seen by JEXL — the value model.
//
// JEXL's rules branch on the boxed Java type of every operand, so each boxed type is its own variant.
// Mutable Java objects (arrays, collections, maps) are shared references: cloning a Value clones
// the reference, exactly like assigning a Java reference. Locks are never held while calling back
// into other values (snapshots are taken first), so a list containing itself cannot deadlock.
use std::any::Any;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use num_bigint::BigInt;

use crate::java::big_decimal::BigDecimal;
use crate::java::hash_map::{JHashMap, JHashSet, JavaHash};
use crate::java::number;
use crate::java::string::{JString, JStringBuilder};

/// A Java value.
#[derive(Clone)]
pub enum Value {
    Null,
    Boolean(bool),
    Byte(i8),
    Short(i16),
    Integer(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Character(u16),
    String(JString),
    BigInteger(Arc<BigInt>),
    BigDecimal(Arc<BigDecimal>),
    AtomicBoolean(Arc<AtomicBool>),
    Array(JArray),
    List(JList),
    Map(JMap),
    Set(JSet),
    /// A JEXL-internal or host object (ranges, closures, scripts, iterators, patterns, user types).
    Object(Arc<dyn HostObject>),
}

/// The primitive or reference component type of a Java array.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Component {
    Boolean,
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
    Char,
    /// a reference type, by its Java binary name (e.g. "java.lang.Object", "java.lang.String")
    Class(String),
}

impl Component {
    pub fn object() -> Component {
        Component::Class("java.lang.Object".into())
    }
    pub fn is_primitive(&self) -> bool {
        !matches!(self, Component::Class(_))
    }
    /// Class.getSimpleName() of the component
    pub fn simple_name(&self) -> String {
        match self {
            Component::Boolean => "boolean".into(),
            Component::Byte => "byte".into(),
            Component::Short => "short".into(),
            Component::Int => "int".into(),
            Component::Long => "long".into(),
            Component::Float => "float".into(),
            Component::Double => "double".into(),
            Component::Char => "char".into(),
            Component::Class(n) => n.rsplit(['.', '$']).next().unwrap_or(n).to_string(),
        }
    }
    /// JVM descriptor used by Class.getName() of the array ("[I", "[Ljava.lang.String;")
    pub fn descriptor(&self) -> String {
        match self {
            Component::Boolean => "Z".into(),
            Component::Byte => "B".into(),
            Component::Short => "S".into(),
            Component::Int => "I".into(),
            Component::Long => "J".into(),
            Component::Float => "F".into(),
            Component::Double => "D".into(),
            Component::Char => "C".into(),
            Component::Class(n) if n.starts_with('[') => n.clone(),
            Component::Class(n) => format!("L{};", n),
        }
    }
}

fn read<T>(l: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(|p| p.into_inner())
}
fn write<T>(l: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    l.write().unwrap_or_else(|p| p.into_inner())
}

/// A Java array: fixed length, typed component, shared and mutable in place.
#[derive(Clone)]
pub struct JArray {
    pub component: Component,
    items: Arc<RwLock<Vec<Value>>>,
}

impl JArray {
    pub fn new(component: Component, items: Vec<Value>) -> JArray {
        JArray { component, items: Arc::new(RwLock::new(items)) }
    }
    pub fn len(&self) -> usize {
        read(&self.items).len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, i: usize) -> Option<Value> {
        read(&self.items).get(i).cloned()
    }
    pub fn set(&self, i: usize, v: Value) -> bool {
        match write(&self.items).get_mut(i) {
            Some(slot) => {
                *slot = v;
                true
            }
            None => false,
        }
    }
    pub fn snapshot(&self) -> Vec<Value> {
        read(&self.items).clone()
    }
    pub fn ptr_eq(&self, o: &JArray) -> bool {
        Arc::ptr_eq(&self.items, &o.items)
    }
    fn addr(&self) -> usize {
        Arc::as_ptr(&self.items) as *const () as usize
    }
}

/// The java.util.List implementation behind a list value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    ArrayList,
    LinkedList,
    /// Arrays.asList: fixed size, settable
    ArraysAsList,
    /// Collections.unmodifiableList / List.of
    Unmodifiable,
}

impl ListKind {
    pub fn class_name(&self) -> &'static str {
        match self {
            ListKind::ArrayList => "java.util.ArrayList",
            ListKind::LinkedList => "java.util.LinkedList",
            ListKind::ArraysAsList => "java.util.Arrays$ArrayList",
            ListKind::Unmodifiable => "java.util.Collections$UnmodifiableRandomAccessList",
        }
    }
}

pub struct ListData {
    pub kind: ListKind,
    pub items: Vec<Value>,
}

/// A shared java.util.List.
#[derive(Clone)]
pub struct JList(Arc<RwLock<ListData>>);

impl JList {
    pub fn new(kind: ListKind, items: Vec<Value>) -> JList {
        JList(Arc::new(RwLock::new(ListData { kind, items })))
    }
    pub fn array_list(items: Vec<Value>) -> JList {
        JList::new(ListKind::ArrayList, items)
    }
    pub fn read(&self) -> RwLockReadGuard<'_, ListData> {
        read(&self.0)
    }
    pub fn write(&self) -> RwLockWriteGuard<'_, ListData> {
        write(&self.0)
    }
    pub fn kind(&self) -> ListKind {
        self.read().kind
    }
    pub fn len(&self) -> usize {
        self.read().items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, i: usize) -> Option<Value> {
        self.read().items.get(i).cloned()
    }
    pub fn snapshot(&self) -> Vec<Value> {
        self.read().items.clone()
    }
    pub fn ptr_eq(&self, o: &JList) -> bool {
        Arc::ptr_eq(&self.0, &o.0)
    }
    fn addr(&self) -> usize {
        Arc::as_ptr(&self.0) as *const () as usize
    }
}

/// The java.util.Map implementation behind a map value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapKind {
    HashMap,
    LinkedHashMap,
    TreeMap,
    Unmodifiable,
    /// Collections.emptyMap()
    Empty,
}

impl MapKind {
    pub fn class_name(&self) -> &'static str {
        match self {
            MapKind::HashMap => "java.util.HashMap",
            MapKind::LinkedHashMap => "java.util.LinkedHashMap",
            MapKind::TreeMap => "java.util.TreeMap",
            MapKind::Unmodifiable => "java.util.Collections$UnmodifiableMap",
            MapKind::Empty => "java.util.Collections$EmptyMap",
        }
    }
}

pub struct MapData {
    pub kind: MapKind,
    /// HashMap / LinkedHashMap order is kept by JHashMap.
    // ponytail: TreeMap order is not modeled yet; add a sorted variant when a test needs it.
    pub map: JHashMap<Value, Value>,
}

/// A shared java.util.Map.
#[derive(Clone)]
pub struct JMap(Arc<RwLock<MapData>>);

impl JMap {
    pub fn new(kind: MapKind, map: JHashMap<Value, Value>) -> JMap {
        JMap(Arc::new(RwLock::new(MapData { kind, map })))
    }
    pub fn hash_map() -> JMap {
        JMap::new(MapKind::HashMap, JHashMap::new())
    }
    pub fn read(&self) -> RwLockReadGuard<'_, MapData> {
        read(&self.0)
    }
    pub fn write(&self) -> RwLockWriteGuard<'_, MapData> {
        write(&self.0)
    }
    pub fn kind(&self) -> MapKind {
        self.read().kind
    }
    pub fn len(&self) -> usize {
        self.read().map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, k: &Value) -> Option<Value> {
        self.read().map.get(k).cloned()
    }
    pub fn contains_key(&self, k: &Value) -> bool {
        self.read().map.contains_key(k)
    }
    pub fn put(&self, k: Value, v: Value) -> Option<Value> {
        self.write().map.put(k, v)
    }
    /// entries in Java iteration order
    pub fn snapshot(&self) -> Vec<(Value, Value)> {
        self.read().map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
    pub fn ptr_eq(&self, o: &JMap) -> bool {
        Arc::ptr_eq(&self.0, &o.0)
    }
    fn addr(&self) -> usize {
        Arc::as_ptr(&self.0) as *const () as usize
    }
}

/// The java.util.Set implementation behind a set value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetKind {
    HashSet,
    LinkedHashSet,
    TreeSet,
    Unmodifiable,
}

impl SetKind {
    pub fn class_name(&self) -> &'static str {
        match self {
            SetKind::HashSet => "java.util.HashSet",
            SetKind::LinkedHashSet => "java.util.LinkedHashSet",
            SetKind::TreeSet => "java.util.TreeSet",
            SetKind::Unmodifiable => "java.util.Collections$UnmodifiableSet",
        }
    }
}

pub struct SetData {
    pub kind: SetKind,
    pub set: JHashSet<Value>,
}

/// A shared java.util.Set.
#[derive(Clone)]
pub struct JSet(Arc<RwLock<SetData>>);

impl JSet {
    pub fn new(kind: SetKind, set: JHashSet<Value>) -> JSet {
        JSet(Arc::new(RwLock::new(SetData { kind, set })))
    }
    pub fn read(&self) -> RwLockReadGuard<'_, SetData> {
        read(&self.0)
    }
    pub fn write(&self) -> RwLockWriteGuard<'_, SetData> {
        write(&self.0)
    }
    pub fn kind(&self) -> SetKind {
        self.read().kind
    }
    pub fn len(&self) -> usize {
        self.read().set.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn contains(&self, v: &Value) -> bool {
        self.read().set.contains(v)
    }
    pub fn snapshot(&self) -> Vec<Value> {
        self.read().set.iter().cloned().collect()
    }
    pub fn ptr_eq(&self, o: &JSet) -> bool {
        Arc::ptr_eq(&self.0, &o.0)
    }
    fn addr(&self) -> usize {
        Arc::as_ptr(&self.0) as *const () as usize
    }
}

/// A non-JDK-value object: JEXL internals (ranges, closures, scripts, iterators, patterns) and
/// user host objects. The introspection side (methods, properties) lives in the uberspect.
pub trait HostObject: Any + Send + Sync {
    /// Class.getName()
    fn class_name(&self) -> String;
    /// Class.getSimpleName()
    fn simple_name(&self) -> String {
        let n = self.class_name();
        n.rsplit(['.', '$']).next().unwrap_or(&n).to_string()
    }
    /// Object.toString(); None means the identity-hash default (nondeterministic in Java)
    fn java_to_string(&self) -> Option<String> {
        None
    }
    /// Object.equals(other); default is identity
    fn java_equals(&self, _other: &Value) -> Option<bool> {
        None
    }
    /// Object.hashCode(); None means identity hash
    fn java_hash_code(&self) -> Option<i32> {
        None
    }
    /// `instanceof java.util.Collection`: whether Java also sees it as a Set, and what it holds
    /// right now. A map view answers here; everything else is not a collection.
    fn as_collection(&self) -> Option<(bool, Vec<Value>)> {
        None
    }
    fn as_any(&self) -> &dyn Any;
}

impl Value {
    pub fn string(s: &str) -> Value {
        Value::String(JString::from(s))
    }
    pub fn big_integer(b: BigInt) -> Value {
        Value::BigInteger(Arc::new(b))
    }
    pub fn big_decimal(b: BigDecimal) -> Value {
        Value::BigDecimal(Arc::new(b))
    }
    pub fn object<T: HostObject>(o: T) -> Value {
        Value::Object(Arc::new(o))
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
    /// instanceof Number
    pub fn is_number(&self) -> bool {
        matches!(
            self,
            Value::Byte(_)
                | Value::Short(_)
                | Value::Integer(_)
                | Value::Long(_)
                | Value::Float(_)
                | Value::Double(_)
                | Value::BigInteger(_)
                | Value::BigDecimal(_)
        )
    }
    pub fn as_host<T: 'static>(&self) -> Option<&T> {
        match self {
            Value::Object(o) => o.as_any().downcast_ref::<T>(),
            _ => None,
        }
    }

    /// getClass().getName()
    pub fn class_name(&self) -> String {
        match self {
            Value::Null => "null".into(),
            Value::Boolean(_) => "java.lang.Boolean".into(),
            Value::Byte(_) => "java.lang.Byte".into(),
            Value::Short(_) => "java.lang.Short".into(),
            Value::Integer(_) => "java.lang.Integer".into(),
            Value::Long(_) => "java.lang.Long".into(),
            Value::Float(_) => "java.lang.Float".into(),
            Value::Double(_) => "java.lang.Double".into(),
            Value::Character(_) => "java.lang.Character".into(),
            Value::String(_) => "java.lang.String".into(),
            Value::BigInteger(_) => "java.math.BigInteger".into(),
            Value::BigDecimal(_) => "java.math.BigDecimal".into(),
            Value::AtomicBoolean(_) => "java.util.concurrent.atomic.AtomicBoolean".into(),
            Value::Array(a) => format!("[{}", a.component.descriptor()),
            Value::List(l) => l.kind().class_name().into(),
            Value::Map(m) => m.kind().class_name().into(),
            Value::Set(s) => s.kind().class_name().into(),
            Value::Object(o) => o.class_name(),
        }
    }

    /// getClass().getSimpleName()
    pub fn simple_name(&self) -> String {
        match self {
            Value::Array(a) => format!("{}[]", a.component.simple_name()),
            Value::Object(o) => o.simple_name(),
            Value::List(l) => match l.kind() {
                ListKind::ArraysAsList => "ArrayList".into(),
                ListKind::Unmodifiable => "UnmodifiableRandomAccessList".into(),
                k => k.class_name().rsplit('.').next().unwrap_or("").to_string(),
            },
            _ => {
                let n = self.class_name();
                n.rsplit(['.', '$']).next().unwrap_or(&n).to_string()
            }
        }
    }

    /// Reference identity (Java ==) for shared objects; for boxed immutables, Java identity is
    /// unobservable through JEXL except where equal values compare equal anyway.
    pub fn same_instance(&self, o: &Value) -> bool {
        match (self, o) {
            (Value::Null, Value::Null) => true,
            (Value::Array(a), Value::Array(b)) => a.ptr_eq(b),
            (Value::List(a), Value::List(b)) => a.ptr_eq(b),
            (Value::Map(a), Value::Map(b)) => a.ptr_eq(b),
            (Value::Set(a), Value::Set(b)) => a.ptr_eq(b),
            (Value::AtomicBoolean(a), Value::AtomicBoolean(b)) => Arc::ptr_eq(a, b),
            (Value::Object(a), Value::Object(b)) => Arc::ptr_eq(a, b),
            (Value::BigInteger(a), Value::BigInteger(b)) => Arc::ptr_eq(a, b),
            (Value::BigDecimal(a), Value::BigDecimal(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Object.equals
    pub fn java_equals(&self, o: &Value) -> bool {
        match (self, o) {
            (Value::Null, Value::Null) => true,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Byte(a), Value::Byte(b)) => a == b,
            (Value::Short(a), Value::Short(b)) => a == b,
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Long(a), Value::Long(b)) => a == b,
            // Float/Double.equals compare the bits (NaN equals NaN, 0.0 != -0.0)
            (Value::Float(a), Value::Float(b)) => float_bits(*a) == float_bits(*b),
            (Value::Double(a), Value::Double(b)) => double_bits(*a) == double_bits(*b),
            (Value::Character(a), Value::Character(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::BigInteger(a), Value::BigInteger(b)) => a == b,
            (Value::BigDecimal(a), Value::BigDecimal(b)) => a.java_equals(b),
            (Value::AtomicBoolean(a), Value::AtomicBoolean(b)) => Arc::ptr_eq(a, b),
            (Value::Array(a), Value::Array(b)) => a.ptr_eq(b),
            (Value::List(a), Value::List(b)) => {
                if a.ptr_eq(b) {
                    return true;
                }
                let (x, y) = (a.snapshot(), b.snapshot());
                x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| p.java_equals(q))
            }
            (Value::Set(a), Value::Set(b)) => {
                if a.ptr_eq(b) {
                    return true;
                }
                let y = b.snapshot();
                a.len() == y.len() && y.iter().all(|e| a.contains(e))
            }
            (Value::Map(a), Value::Map(b)) => {
                if a.ptr_eq(b) {
                    return true;
                }
                let y = b.snapshot();
                a.len() == y.len()
                    && y.iter().all(|(k, v)| match a.get(k) {
                        Some(av) => av.java_equals(v),
                        None => v.is_null() && a.contains_key(k),
                    })
            }
            (Value::Object(a), _) => match a.java_equals(o) {
                Some(b) => b,
                None => matches!(o, Value::Object(b) if Arc::ptr_eq(a, b)),
            },
            _ => false,
        }
    }

    /// Object.hashCode; identity hashes are replaced by a stable address-derived value.
    pub fn java_hash_code(&self) -> i32 {
        match self {
            Value::Null => 0,
            Value::Boolean(b) => {
                if *b {
                    1231
                } else {
                    1237
                }
            }
            Value::Byte(b) => *b as i32,
            Value::Short(s) => *s as i32,
            Value::Integer(i) => *i,
            Value::Long(l) => number::long_hash_code(*l),
            Value::Float(f) => number::float_hash_code(*f),
            Value::Double(d) => number::double_hash_code(*d),
            Value::Character(c) => *c as i32,
            Value::String(s) => s.hash_code(),
            Value::BigInteger(b) => number::big_integer_hash_code(b),
            Value::BigDecimal(b) => b.java_hash_code(),
            Value::AtomicBoolean(a) => Arc::as_ptr(a) as usize as i32,
            Value::Array(a) => a.addr() as i32,
            Value::List(l) => {
                let mut h: i32 = 1;
                for e in l.snapshot() {
                    let eh = if matches!(&e, Value::List(x) if x.ptr_eq(l)) { 0 } else { e.java_hash_code() };
                    h = h.wrapping_mul(31).wrapping_add(eh);
                }
                h
            }
            Value::Set(s) => s.snapshot().iter().fold(0i32, |h, e| h.wrapping_add(e.java_hash_code())),
            Value::Map(m) => m
                .snapshot()
                .iter()
                .fold(0i32, |h, (k, v)| h.wrapping_add(k.java_hash_code() ^ v.java_hash_code())),
            Value::Object(o) => o.java_hash_code().unwrap_or(Arc::as_ptr(o) as *const () as usize as i32),
        }
    }

    /// String.valueOf(Object) / Object.toString()
    pub fn java_to_string(&self) -> String {
        self.java_to_jstring().to_rust()
    }

    /// Whether java_to_string() would print an identity hash (nondeterministic in Java).
    pub fn has_identity_to_string(&self) -> bool {
        match self {
            Value::Array(_) => true,
            Value::Object(o) => o.java_to_string().is_none(),
            _ => false,
        }
    }

    /// Object.toString() as a Java String (UTF-16, so lone surrogates survive).
    pub fn java_to_jstring(&self) -> JString {
        match self {
            Value::Null => JString::from("null"),
            Value::Boolean(b) => JString::from(b.to_string()),
            Value::Byte(b) => JString::from(b.to_string()),
            Value::Short(s) => JString::from(s.to_string()),
            Value::Integer(i) => JString::from(i.to_string()),
            Value::Long(l) => JString::from(l.to_string()),
            Value::Float(f) => JString::from(number::float_to_string(*f)),
            Value::Double(d) => JString::from(number::double_to_string(*d)),
            Value::Character(c) => JString::from_units(&[*c]),
            Value::String(s) => s.clone(),
            Value::BigInteger(b) => JString::from(b.to_string()),
            Value::BigDecimal(b) => JString::from(b.to_java_string()),
            Value::AtomicBoolean(a) => JString::from(a.load(AtomicOrdering::SeqCst).to_string()),
            Value::Array(a) => JString::from(format!("{}@{:x}", self.class_name(), a.addr() as u32)),
            Value::List(l) => {
                let mut b = JStringBuilder::new();
                b.str("[");
                for (n, e) in l.snapshot().iter().enumerate() {
                    if n > 0 {
                        b.str(", ");
                    }
                    match e {
                        Value::List(x) if x.ptr_eq(l) => b.str("(this Collection)"),
                        _ => b.jstr(&e.java_to_jstring()),
                    };
                }
                b.str("]").build()
            }
            Value::Set(s) => {
                let mut b = JStringBuilder::new();
                b.str("[");
                for (n, e) in s.snapshot().iter().enumerate() {
                    if n > 0 {
                        b.str(", ");
                    }
                    match e {
                        Value::Set(x) if x.ptr_eq(s) => b.str("(this Collection)"),
                        _ => b.jstr(&e.java_to_jstring()),
                    };
                }
                b.str("]").build()
            }
            Value::Map(m) => {
                let mut b = JStringBuilder::new();
                b.str("{");
                for (n, (k, v)) in m.snapshot().iter().enumerate() {
                    if n > 0 {
                        b.str(", ");
                    }
                    match k {
                        Value::Map(x) if x.ptr_eq(m) => b.str("(this Map)"),
                        _ => b.jstr(&k.java_to_jstring()),
                    };
                    b.str("=");
                    match v {
                        Value::Map(x) if x.ptr_eq(m) => b.str("(this Map)"),
                        _ => b.jstr(&v.java_to_jstring()),
                    };
                }
                b.str("}").build()
            }
            Value::Object(o) => JString::from(o.java_to_string().unwrap_or_else(|| {
                format!("{}@{:x}", o.class_name(), Arc::as_ptr(o) as *const () as usize as u32)
            })),
        }
    }
}

/// Float.floatToRawIntBits: the exact bit pattern (NaN sign and payload preserved)
/// java.util.regex.Pattern as a JEXL value (a regex literal evaluates to one).
pub struct PatternValue(pub Arc<crate::java::regex::Pattern>);

impl HostObject for PatternValue {
    fn class_name(&self) -> String {
        "java.util.regex.Pattern".into()
    }
    // port of: Pattern.toString — the pattern source
    fn java_to_string(&self) -> Option<String> {
        Some(self.0.pattern().to_string())
    }
    // java.util.regex.Pattern does not override equals: identity
    fn java_equals(&self, other: &Value) -> Option<bool> {
        Some(match other {
            Value::Object(o) => std::sync::Arc::as_ptr(o) as *const () == self as *const _ as *const (),
            _ => false,
        })
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub fn float_raw_bits(f: f32) -> u32 {
    f.to_bits()
}

/// Double.doubleToRawLongBits
pub fn double_raw_bits(d: f64) -> u64 {
    d.to_bits()
}

pub fn float_bits(f: f32) -> u32 {
    // Float.floatToIntBits: canonical NaN
    if f.is_nan() {
        0x7fc00000
    } else {
        f.to_bits()
    }
}

pub fn double_bits(d: f64) -> u64 {
    // Double.doubleToLongBits: canonical NaN
    if d.is_nan() {
        0x7ff8000000000000
    } else {
        d.to_bits()
    }
}

impl JavaHash for Value {
    fn java_hash_code(&self) -> i32 {
        Value::java_hash_code(self)
    }
    fn java_equals(&self, other: &Self) -> bool {
        Value::java_equals(self, other)
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::String(s) => write!(f, "{:?}", s),
            _ => write!(f, "{}:{}", self.simple_name(), self.java_to_string()),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.java_to_string())
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Boolean(b)
    }
}
impl From<i32> for Value {
    fn from(i: i32) -> Value {
        Value::Integer(i)
    }
}
impl From<i64> for Value {
    fn from(l: i64) -> Value {
        Value::Long(l)
    }
}
impl From<f64> for Value {
    fn from(d: f64) -> Value {
        Value::Double(d)
    }
}
impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::string(s)
    }
}
impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::String(JString::from(s))
    }
}
impl From<JString> for Value {
    fn from(s: JString) -> Value {
        Value::String(s)
    }
}
