// port of: org.apache.commons.jexl3.JexlContext (and MapContext, ObjectContext)
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

use crate::java::hash_map::{str_hash_code, JHashMap};

/// Whether a map key is the Java String equal to `name`.
fn is_str(key: &Value, name: &str) -> bool {
    matches!(key, Value::String(s) if s.units().iter().copied().eq(name.encode_utf16()))
}
use crate::java::string::JString;
use crate::jexl_exception::JexlException;
use crate::jexl_options::JexlOptions;
use crate::value::Value;

/// port of: org.apache.commons.jexl3.JexlContext
///
/// The optional Java sub-interfaces (`NamespaceResolver`, `AnnotationProcessor`, `OptionsHandle`,
/// `PragmaProcessor`, `CancellationHandle`) are optional methods here: a context that does not
/// implement one returns None, exactly like an `instanceof` that fails.
pub trait JexlContext: Send + Sync {
    /// port of: JexlContext.get
    fn get(&self, name: &str) -> Option<Value>;

    /// port of: JexlContext.set; Err is Java's UnsupportedOperationException
    fn set(&self, name: &str, value: Value) -> Result<(), String>;

    /// port of: JexlContext.has
    fn has(&self, name: &str) -> bool;

    /// port of: MapContext.clear -- a public method a script can call as `clear()`, like any
    /// public method of the context object. None: this context has no such method.
    fn clear(&self) -> Option<()> {
        None
    }

    /// port of: JexlContext.NamespaceResolver.resolveNamespace
    fn resolve_namespace(&self, _name: Option<&str>) -> Option<Value> {
        None
    }

    /// whether this context implements NamespaceResolver at all
    fn is_namespace_resolver(&self) -> bool {
        false
    }

    /// port of: JexlContext.OptionsHandle.getEngineOptions
    fn get_engine_options(&self) -> Option<JexlOptions> {
        None
    }

    /// port of: JexlContext.PragmaProcessor.processPragma
    fn process_pragma(&self, _key: &JString, _value: &Value) {}

    fn is_pragma_processor(&self) -> bool {
        false
    }

    /// port of: JexlContext.CancellationHandle.getCancellation
    fn get_cancellation(&self) -> Option<Arc<AtomicBool>> {
        None
    }

    /// port of: JexlContext.AnnotationProcessor.processAnnotation
    ///
    /// `statement` runs the annotated block; returning None means "not an AnnotationProcessor",
    /// so the caller just runs the statement.
    fn process_annotation(
        &self,
        _name: &str,
        _args: Option<&[Value]>,
        _statement: &mut dyn FnMut() -> Result<Value, JexlException>,
    ) -> Option<Result<Value, JexlException>> {
        None
    }
}

/// port of: org.apache.commons.jexl3.JexlEngine.EmptyContext
pub struct EmptyContext;

impl JexlContext for EmptyContext {
    fn get(&self, _name: &str) -> Option<Value> {
        None
    }
    fn set(&self, _name: &str, _value: Value) -> Result<(), String> {
        Err("Not supported in void context.".into())
    }
    fn has(&self, _name: &str) -> bool {
        false
    }
}

/// port of: org.apache.commons.jexl3.MapContext
///
/// Java's MapContext wraps a `HashMap`, so iteration and `toString` follow HashMap order.
pub struct MapContext {
    map: RwLock<JHashMap<Value, Value>>,
    namespaces: HashMap<String, Value>,
    options: Option<JexlOptions>,
}

impl Default for MapContext {
    fn default() -> Self {
        MapContext::new()
    }
}

impl MapContext {
    // port of: MapContext()
    pub fn new() -> MapContext {
        MapContext { map: RwLock::new(JHashMap::new()), namespaces: HashMap::new(), options: None }
    }

    /// A context whose `resolveNamespace` answers from this map (JexlBuilder.namespaces puts them
    /// on the engine, but a test context can carry them too).
    pub fn with_namespaces(mut self, ns: HashMap<String, Value>) -> Self {
        self.namespaces = ns;
        self
    }

    pub fn with_options(mut self, options: JexlOptions) -> Self {
        self.options = Some(options);
        self
    }

    // port of: MapContext.clear
    pub fn clear(&self) {
        self.map.write().unwrap_or_else(|p| p.into_inner()).clear();
    }

    /// The bindings, in Java HashMap iteration order.
    pub fn entries(&self) -> Vec<(JString, Value)> {
        self.map
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|(k, v)| (k.java_to_jstring(), v.clone()))
            .collect()
    }
}

impl JexlContext for MapContext {
    fn clear(&self) -> Option<()> {
        self.map.write().unwrap_or_else(|p| p.into_inner()).clear();
        Some(())
    }

    // port of: MapContext.get
    fn get(&self, name: &str) -> Option<Value> {
        // Every variable read lands here, so it must not allocate a key to look one up.
        let map = self.map.read().unwrap_or_else(|p| p.into_inner());
        match map.get_hashed(str_hash_code(name), |k| is_str(k, name)) {
            Some(found) => found.cloned(),
            None => map.get(&Value::string(name)).cloned(),
        }
    }

    // port of: MapContext.set
    fn set(&self, name: &str, value: Value) -> Result<(), String> {
        let mut map = self.map.write().unwrap_or_else(|p| p.into_inner());
        // HashMap.put on an existing key replaces the value and keeps the key object, so a
        // variable that is already bound needs no new key -- a loop variable is rebound per turn.
        if let Some(Some(slot)) = map.get_hashed_mut(str_hash_code(name), |k| is_str(k, name)) {
            *slot = value;
            return Ok(());
        }
        map.put(Value::string(name), value);
        Ok(())
    }

    // port of: MapContext.has
    fn has(&self, name: &str) -> bool {
        let map = self.map.read().unwrap_or_else(|p| p.into_inner());
        match map.get_hashed(str_hash_code(name), |k| is_str(k, name)) {
            Some(found) => found.is_some(),
            None => map.contains_key(&Value::string(name)),
        }
    }

    fn resolve_namespace(&self, name: Option<&str>) -> Option<Value> {
        name.and_then(|n| self.namespaces.get(n)).cloned()
    }

    fn is_namespace_resolver(&self) -> bool {
        !self.namespaces.is_empty()
    }

    fn get_engine_options(&self) -> Option<JexlOptions> {
        self.options.clone()
    }
}
