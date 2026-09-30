// port of: org.apache.commons.jexl3.internal.introspection.Uberspect
//
// The Java class drives reflection through an Introspector; this port delegates to the JDK shim
// (the types scripts can call) and to host objects registered by the embedder.
use std::sync::Arc;

use crate::introspection::{JexlMethod, JexlPropertyGet, JexlPropertySet, JexlUberspect, PropertyResolver, ResolverStrategy};
use crate::jexl_operator::JexlOperator;
use crate::value::Value;

pub struct Uberspect {
    strategy: ResolverStrategy,
    /// the JDK shim: the types a script can call methods and properties on
    shim: Option<Arc<dyn JexlUberspect>>,
}

impl Default for Uberspect {
    fn default() -> Self {
        Uberspect::new()
    }
}

impl Uberspect {
    // port of: Uberspect(Log, ResolverStrategy)
    pub fn new() -> Uberspect {
        Uberspect { strategy: ResolverStrategy::Jexl, shim: Some(Arc::new(crate::introspection::jdk_shim::JdkShim::new(ResolverStrategy::Jexl))) }
    }

    pub fn with_strategy(strategy: ResolverStrategy) -> Uberspect {
        Uberspect { strategy, shim: Some(Arc::new(crate::introspection::jdk_shim::JdkShim::new(strategy))) }
    }

    /// Installs the JDK shim (or any other introspector).
    pub fn with_shim(mut self, shim: Arc<dyn JexlUberspect>) -> Uberspect {
        self.shim = Some(shim);
        self
    }
}

impl JexlUberspect for Uberspect {
    fn get_resolvers(&self, op: Option<JexlOperator>, obj: &Value) -> &'static [PropertyResolver] {
        self.strategy.apply(op, obj)
    }

    fn get_constructor(&self, ctor_handle: &Value, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        self.shim.as_ref()?.get_constructor(ctor_handle, args)
    }

    fn get_method(&self, obj: &Value, method: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        self.shim.as_ref()?.get_method(obj, method, args)
    }

    fn get_property_get_with(
        &self,
        resolvers: &[PropertyResolver],
        obj: &Value,
        identifier: &Value,
    ) -> Option<Arc<dyn JexlPropertyGet>> {
        self.shim.as_ref()?.get_property_get_with(resolvers, obj, identifier)
    }

    fn get_property_set_with(
        &self,
        resolvers: &[PropertyResolver],
        obj: &Value,
        identifier: &Value,
        arg: &Value,
    ) -> Option<Arc<dyn JexlPropertySet>> {
        self.shim.as_ref()?.get_property_set_with(resolvers, obj, identifier, arg)
    }

    fn get_iterator(&self, obj: &Value) -> Option<Box<dyn Iterator<Item = Value> + Send>> {
        self.shim.as_ref()?.get_iterator(obj)
    }
}
