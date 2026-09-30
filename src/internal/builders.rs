// port of: org.apache.commons.jexl3.internal.ArrayBuilder, MapBuilder and SetBuilder
use crate::java::hash_map::{JHashMap, JHashSet};
use crate::value::{Component, JArray, JList, JMap, JSet, ListKind, MapKind, SetKind, Value};

/// port of: ArrayBuilder — an array literal gets the most specific component type of its elements.
pub struct ArrayBuilder {
    /// the common element class so far; None until the first non-null element
    common_class: Option<String>,
    is_number: bool,
    unboxing: bool,
    untyped: Vec<Value>,
}

/// The boxed class name of a value, or None for null.
fn class_of(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        other => Some(other.class_name()),
    }
}

fn is_number_class(c: &str) -> bool {
    matches!(
        c,
        "java.lang.Byte"
            | "java.lang.Short"
            | "java.lang.Integer"
            | "java.lang.Long"
            | "java.lang.Float"
            | "java.lang.Double"
            | "java.math.BigInteger"
            | "java.math.BigDecimal"
            | "java.lang.Number"
    )
}

// port of: ArrayBuilder.unboxingClass
fn unboxing_class(parm: &str) -> Component {
    match parm {
        "java.lang.Boolean" => Component::Boolean,
        "java.lang.Byte" => Component::Byte,
        "java.lang.Character" => Component::Char,
        "java.lang.Double" => Component::Double,
        "java.lang.Float" => Component::Float,
        "java.lang.Integer" => Component::Int,
        "java.lang.Long" => Component::Long,
        "java.lang.Short" => Component::Short,
        other => Component::Class(other.to_string()),
    }
}

/// The nearest common superclass, as `Class.getSuperclass()` walks it for the classes we model.
fn superclass(c: &str) -> Option<&'static str> {
    match c {
        "java.lang.Byte" | "java.lang.Short" | "java.lang.Integer" | "java.lang.Long" | "java.lang.Float"
        | "java.lang.Double" | "java.math.BigInteger" | "java.math.BigDecimal" => Some("java.lang.Number"),
        "java.lang.Number" | "java.lang.String" | "java.lang.Boolean" | "java.lang.Character" => Some("java.lang.Object"),
        "java.util.ArrayList" | "java.util.LinkedList" => Some("java.util.AbstractList"),
        "java.util.AbstractList" => Some("java.util.AbstractCollection"),
        "java.util.HashSet" | "java.util.TreeSet" => Some("java.util.AbstractSet"),
        "java.util.LinkedHashSet" => Some("java.util.HashSet"),
        "java.util.AbstractSet" => Some("java.util.AbstractCollection"),
        "java.util.AbstractCollection" => Some("java.lang.Object"),
        "java.util.HashMap" | "java.util.TreeMap" => Some("java.util.AbstractMap"),
        "java.util.LinkedHashMap" => Some("java.util.HashMap"),
        "java.util.AbstractMap" => Some("java.lang.Object"),
        "java.lang.Object" => None,
        _ => Some("java.lang.Object"),
    }
}

/// `commonClass.isAssignableFrom(eclass)` for the classes we model.
fn assignable_from(common: &str, mut eclass: &str) -> bool {
    if common == "java.lang.Object" {
        return true;
    }
    loop {
        if common == eclass {
            return true;
        }
        match superclass(eclass) {
            Some(s) => eclass = s,
            None => return false,
        }
    }
}

impl ArrayBuilder {
    // port of: ArrayBuilder(int)
    pub fn new(size: usize) -> ArrayBuilder {
        ArrayBuilder { common_class: None, is_number: true, unboxing: true, untyped: Vec::with_capacity(size) }
    }

    // port of: ArrayBuilder.add
    pub fn add(&mut self, value: Value) {
        // for all children after first...
        if self.common_class.as_deref() != Some("java.lang.Object") {
            match class_of(&value) {
                None => {
                    self.is_number = false;
                    self.unboxing = false;
                }
                Some(eclass) => match &self.common_class {
                    None => {
                        self.is_number = self.is_number && is_number_class(&eclass);
                        self.common_class = Some(eclass);
                    }
                    Some(common) if common != &eclass => {
                        // if both are numbers, use number, if not, use Object
                        if self.is_number && is_number_class(&eclass) {
                            self.common_class = Some("java.lang.Number".into());
                        } else {
                            let mut walk = eclass.as_str();
                            loop {
                                match superclass(walk) {
                                    None => {
                                        self.common_class = Some("java.lang.Object".into());
                                        break;
                                    }
                                    Some(s) => {
                                        walk = s;
                                        if assignable_from(common, walk) {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(_) => {}
                },
            }
        }
        self.untyped.push(value);
    }

    // port of: ArrayBuilder.create
    pub fn create(mut self, extended: bool) -> Value {
        if extended {
            return Value::List(JList::new(ListKind::ArrayList, self.untyped));
        }
        let common = match &self.common_class {
            None => return Value::Array(JArray::new(Component::object(), self.untyped)),
            Some(c) if c == "java.lang.Object" => {
                return Value::Array(JArray::new(Component::object(), self.untyped))
            }
            Some(c) => c.clone(),
        };
        let component = if self.unboxing { unboxing_class(&common) } else { Component::Class(common) };
        // Array.set on a primitive array stores the unboxed value; our Value already is that.
        let items = std::mem::take(&mut self.untyped);
        Value::Array(JArray::new(component, items))
    }
}

/// port of: SetBuilder — `new HashSet<>(size)`
pub struct SetBuilder {
    set: JHashSet<Value>,
}

impl SetBuilder {
    pub fn new(size: usize) -> SetBuilder {
        SetBuilder { set: JHashSet::with_capacity(size) }
    }
    pub fn add(&mut self, value: Value) {
        self.set.add(value);
    }
    pub fn create(self) -> Value {
        Value::Set(JSet::new(SetKind::HashSet, self.set))
    }
}

/// port of: MapBuilder — `new HashMap<>(size)`
pub struct MapBuilder {
    map: JHashMap<Value, Value>,
}

impl MapBuilder {
    pub fn new(size: usize) -> MapBuilder {
        MapBuilder { map: JHashMap::with_capacity(size) }
    }
    pub fn put(&mut self, key: Value, value: Value) {
        self.map.put(key, value);
    }
    pub fn create(self) -> Value {
        Value::Map(JMap::new(MapKind::HashMap, self.map))
    }
}
