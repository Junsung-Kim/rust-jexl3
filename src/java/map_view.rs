// port of: the collection views java.util.Map returns — HashMap$KeySet / $Values / $EntrySet and
// their LinkedHashMap and TreeMap counterparts, plus the Map.Entry each entry set holds.
//
// These are *views*: they read the map they came from, so a change to the map shows through. The
// class names are the ones the JVM reports (measured, not guessed).
use std::any::Any;

use crate::value::{HostObject, JMap, MapKind, Value};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewKind {
    Keys,
    Values,
    Entries,
}

/// port of: AbstractMap's keySet / values / entrySet views
pub struct MapView {
    map: JMap,
    kind: ViewKind,
}

impl MapView {
    pub fn new(map: JMap, kind: ViewKind) -> MapView {
        MapView { map, kind }
    }

    pub fn entries(&self) -> Vec<(Value, Value)> {
        self.map.read().map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    fn elements(&self) -> Vec<Value> {
        self.entries()
            .into_iter()
            .map(|(k, v)| match self.kind {
                ViewKind::Keys => k,
                ViewKind::Values => v,
                ViewKind::Entries => Value::object(MapEntry { map: self.map.clone(), key: k }),
            })
            .collect()
    }

    pub fn map(&self) -> &JMap {
        &self.map
    }

    pub fn kind(&self) -> ViewKind {
        self.kind
    }

    /// Collection.remove: the views are backed by the map, so removing removes the mapping.
    pub fn remove(&self, e: &Value) -> bool {
        let entries = self.entries();
        let key = match self.kind {
            ViewKind::Keys => entries.iter().find(|(k, _)| k.java_equals(e)).map(|(k, _)| k.clone()),
            ViewKind::Values => entries.iter().find(|(_, v)| v.java_equals(e)).map(|(k, _)| k.clone()),
            ViewKind::Entries => match e.as_host::<MapEntry>() {
                Some(entry) => entries.iter().find(|(k, _)| k.java_equals(&entry.key)).map(|(k, _)| k.clone()),
                None => None,
            },
        };
        match key {
            Some(k) => {
                self.map.write().map.remove(&k);
                true
            }
            None => false,
        }
    }
}

/// The view class the JVM reports for a map of this kind.
fn view_class(kind: MapKind, view: ViewKind) -> &'static str {
    match (kind, view) {
        (MapKind::LinkedHashMap, ViewKind::Keys) => "java.util.LinkedHashMap$LinkedKeySet",
        (MapKind::LinkedHashMap, ViewKind::Values) => "java.util.LinkedHashMap$LinkedValues",
        (MapKind::LinkedHashMap, ViewKind::Entries) => "java.util.LinkedHashMap$LinkedEntrySet",
        (MapKind::TreeMap, ViewKind::Keys) => "java.util.TreeMap$KeySet",
        (MapKind::TreeMap, ViewKind::Values) => "java.util.TreeMap$Values",
        (MapKind::TreeMap, ViewKind::Entries) => "java.util.TreeMap$EntrySet",
        (MapKind::Unmodifiable, ViewKind::Keys) => "java.util.Collections$UnmodifiableMap$UnmodifiableEntrySet",
        (MapKind::Unmodifiable, ViewKind::Values) => "java.util.Collections$UnmodifiableCollection",
        (MapKind::Unmodifiable, ViewKind::Entries) => "java.util.Collections$UnmodifiableMap$UnmodifiableEntrySet",
        (MapKind::Empty, ViewKind::Keys) => "java.util.Collections$EmptySet",
        (MapKind::Empty, ViewKind::Values) => "java.util.Collections$EmptyList",
        (MapKind::Empty, ViewKind::Entries) => "java.util.Collections$EmptySet",
        (_, ViewKind::Keys) => "java.util.HashMap$KeySet",
        (_, ViewKind::Values) => "java.util.HashMap$Values",
        (_, ViewKind::Entries) => "java.util.HashMap$EntrySet",
    }
}

/// port of: AbstractCollection.toString — "[a, b]", with `(this Collection)` for self-reference.
fn collection_to_string(elements: &[Value]) -> String {
    let mut out = String::from("[");
    for (i, e) in elements.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&e.java_to_string());
    }
    out.push(']');
    out
}

impl HostObject for MapView {
    fn class_name(&self) -> String {
        view_class(self.map.read().kind, self.kind).to_string()
    }

    fn java_to_string(&self) -> Option<String> {
        Some(collection_to_string(&self.elements()))
    }

    // a key set and an entry set are Sets, so Set.equals/hashCode apply; values() is a plain
    // Collection and keeps Object identity.
    fn java_equals(&self, other: &Value) -> Option<bool> {
        if self.kind == ViewKind::Values {
            return None;
        }
        let mine = self.elements();
        match other.as_host::<MapView>() {
            Some(o) if o.kind != ViewKind::Values => {
                let theirs = o.elements();
                Some(mine.len() == theirs.len() && mine.iter().all(|e| theirs.iter().any(|t| t.java_equals(e))))
            }
            _ => match other {
                Value::Set(s) => {
                    let theirs = s.snapshot();
                    Some(mine.len() == theirs.len() && mine.iter().all(|e| theirs.iter().any(|t| t.java_equals(e))))
                }
                _ => Some(false),
            },
        }
    }

    fn java_hash_code(&self) -> Option<i32> {
        if self.kind == ViewKind::Values {
            return None;
        }
        // AbstractSet.hashCode: the sum of the elements' hashes
        Some(self.elements().iter().fold(0i32, |h, e| h.wrapping_add(e.java_hash_code())))
    }

    fn as_collection(&self) -> Option<(bool, Vec<Value>)> {
        Some((self.kind != ViewKind::Values, self.elements()))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// port of: java.util.Map.Entry — a live view of one mapping.
pub struct MapEntry {
    map: JMap,
    key: Value,
}

impl MapEntry {
    pub fn key(&self) -> Value {
        self.key.clone()
    }

    pub fn value(&self) -> Value {
        self.map.read().map.get(&self.key).cloned().unwrap_or(Value::Null)
    }

    /// port of: Map.Entry.setValue — returns the value it replaced
    pub fn set_value(&self, v: Value) -> Value {
        let old = self.value();
        self.map.write().map.put(self.key.clone(), v);
        old
    }
}

impl HostObject for MapEntry {
    fn class_name(&self) -> String {
        match self.map.read().kind {
            MapKind::LinkedHashMap => "java.util.LinkedHashMap$Entry",
            MapKind::TreeMap => "java.util.TreeMap$Entry",
            MapKind::Unmodifiable => "java.util.Collections$UnmodifiableMap$UnmodifiableEntrySet$UnmodifiableEntry",
            _ => "java.util.HashMap$Node",
        }
        .to_string()
    }

    // port of: HashMap.Node.toString
    fn java_to_string(&self) -> Option<String> {
        Some(format!("{}={}", self.key.java_to_string(), self.value().java_to_string()))
    }

    // port of: Map.Entry.equals — key and value both equal
    fn java_equals(&self, other: &Value) -> Option<bool> {
        match other.as_host::<MapEntry>() {
            Some(o) => Some(self.key.java_equals(&o.key) && self.value().java_equals(&o.value())),
            None => Some(false),
        }
    }

    // port of: Map.Entry.hashCode — key.hashCode() ^ value.hashCode()
    fn java_hash_code(&self) -> Option<i32> {
        Some(self.key.java_hash_code() ^ self.value().java_hash_code())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
