// port of: org.apache.commons.jexl3.introspection (package)
//
// Java resolves properties, methods, constructors and iteration through reflection behind the
// JexlUberspect SPI. This port keeps the SPI and replaces reflection with an explicit model:
// `jdk_shim` implements the JDK types scripts can call, and host objects register their own.
pub mod jdk_shim;
pub mod jexl_sandbox;
pub mod uberspect;

use std::sync::Arc;

use crate::jexl_exception::JexlException;
use crate::jexl_operator::JexlOperator;
use crate::value::Value;

/// The outcome of a `tryInvoke`: Java returns the sentinel `JexlEngine.TRY_FAILED` when the
/// cached callable does not apply to the new arguments.
#[derive(Clone, Debug)]
pub enum TryResult {
    Value(Value),
    Failed,
}

/// port of: org.apache.commons.jexl3.introspection.JexlMethod
pub trait JexlMethod: Send + Sync {
    /// port of: JexlMethod.invoke
    fn invoke(&self, obj: &Value, params: &[Value]) -> Result<Value, JexlException>;

    /// port of: JexlMethod.tryInvoke
    fn try_invoke(&self, name: &str, obj: &Value, params: &[Value]) -> Result<TryResult, JexlException> {
        let _ = (name, obj, params);
        Ok(TryResult::Failed)
    }

    /// port of: JexlMethod.isCacheable
    fn is_cacheable(&self) -> bool {
        false
    }

    /// port of: JexlMethod.getReturnType (the Java class name, or None for void/unknown)
    fn return_type(&self) -> Option<String> {
        None
    }
}

/// port of: org.apache.commons.jexl3.introspection.JexlPropertyGet
pub trait JexlPropertyGet: Send + Sync {
    /// port of: JexlPropertyGet.invoke
    fn invoke(&self, obj: &Value) -> Result<Value, JexlException>;

    /// port of: JexlPropertyGet.tryInvoke
    fn try_invoke(&self, obj: &Value, key: &Value) -> Result<TryResult, JexlException> {
        let _ = (obj, key);
        Ok(TryResult::Failed)
    }

    /// port of: JexlPropertyGet.isCacheable
    fn is_cacheable(&self) -> bool {
        false
    }
}

/// port of: org.apache.commons.jexl3.introspection.JexlPropertySet
pub trait JexlPropertySet: Send + Sync {
    /// port of: JexlPropertySet.invoke
    fn invoke(&self, obj: &Value, arg: &Value) -> Result<Value, JexlException>;

    /// port of: JexlPropertySet.tryInvoke
    fn try_invoke(&self, obj: &Value, key: &Value, value: &Value) -> Result<TryResult, JexlException> {
        let _ = (obj, key, value);
        Ok(TryResult::Failed)
    }

    /// port of: JexlPropertySet.isCacheable
    fn is_cacheable(&self) -> bool {
        false
    }
}

/// port of: JexlUberspect.JexlResolver
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyResolver {
    Property,
    Map,
    List,
    Duck,
    Field,
    Container,
}

/// port of: JexlUberspect.POJO
pub const POJO: [PropertyResolver; 6] = [
    PropertyResolver::Property,
    PropertyResolver::Map,
    PropertyResolver::List,
    PropertyResolver::Duck,
    PropertyResolver::Field,
    PropertyResolver::Container,
];

/// port of: JexlUberspect.MAP
pub const MAP: [PropertyResolver; 6] = [
    PropertyResolver::Map,
    PropertyResolver::List,
    PropertyResolver::Duck,
    PropertyResolver::Property,
    PropertyResolver::Field,
    PropertyResolver::Container,
];

/// port of: JexlUberspect.ResolverStrategy
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolverStrategy {
    /// JexlUberspect.JEXL_STRATEGY
    Jexl,
    /// JexlUberspect.MAP_STRATEGY
    Map,
}

impl ResolverStrategy {
    // port of: JexlUberspect.JEXL_STRATEGY / MAP_STRATEGY
    pub fn apply(&self, operator: Option<JexlOperator>, obj: &Value) -> &'static [PropertyResolver] {
        match operator {
            Some(JexlOperator::ArrayGet) | Some(JexlOperator::ArraySet) => &MAP,
            None if matches!(obj, Value::Map(_)) => &MAP,
            _ => match self {
                ResolverStrategy::Map if matches!(obj, Value::Map(_)) => &MAP,
                _ => &POJO,
            },
        }
    }
}

/// port of: org.apache.commons.jexl3.introspection.JexlUberspect
pub trait JexlUberspect: Send + Sync {
    /// port of: JexlUberspect.getResolvers
    fn get_resolvers(&self, op: Option<JexlOperator>, obj: &Value) -> &'static [PropertyResolver];

    /// port of: JexlUberspect.getVersion
    fn get_version(&self) -> i32 {
        0
    }

    /// port of: JexlUberspect.getConstructor
    fn get_constructor(&self, ctor_handle: &Value, args: &[Value]) -> Option<Arc<dyn JexlMethod>>;

    /// port of: JexlUberspect.getMethod
    fn get_method(&self, obj: &Value, method: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>>;

    /// port of: JexlUberspect.getPropertyGet(Object, Object)
    fn get_property_get(&self, obj: &Value, identifier: &Value) -> Option<Arc<dyn JexlPropertyGet>> {
        let resolvers = self.get_resolvers(None, obj);
        self.get_property_get_with(resolvers, obj, identifier)
    }

    /// port of: JexlUberspect.getPropertyGet(List, Object, Object)
    fn get_property_get_with(
        &self,
        resolvers: &[PropertyResolver],
        obj: &Value,
        identifier: &Value,
    ) -> Option<Arc<dyn JexlPropertyGet>>;

    /// port of: JexlUberspect.getPropertySet(Object, Object, Object)
    fn get_property_set(&self, obj: &Value, identifier: &Value, arg: &Value) -> Option<Arc<dyn JexlPropertySet>> {
        let resolvers = self.get_resolvers(None, obj);
        self.get_property_set_with(resolvers, obj, identifier, arg)
    }

    /// port of: JexlUberspect.getPropertySet(List, Object, Object, Object)
    fn get_property_set_with(
        &self,
        resolvers: &[PropertyResolver],
        obj: &Value,
        identifier: &Value,
        arg: &Value,
    ) -> Option<Arc<dyn JexlPropertySet>>;

    /// port of: JexlUberspect.getIterator — the values `for(x : obj)` walks
    fn get_iterator(&self, obj: &Value) -> Option<Box<dyn Iterator<Item = Value> + Send>>;

    /// port of: JexlUberspect.getArithmetic — operator overloads a custom arithmetic provides
    fn get_operator(&self, operator: JexlOperator, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        let _ = (operator, args);
        None
    }

    /// port of: JexlArithmetic.Uberspect.overloads
    fn overloads(&self, operator: JexlOperator) -> bool {
        let _ = operator;
        false
    }
}
