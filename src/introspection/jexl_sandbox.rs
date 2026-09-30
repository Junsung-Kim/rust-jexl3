// port of: org.apache.commons.jexl3.introspection.JexlSandbox
// (and org.apache.commons.jexl3.internal.introspection.SandboxUberspect)
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::introspection::{JexlMethod, JexlPropertyGet, JexlPropertySet, JexlUberspect, PropertyResolver};
use crate::jexl_operator::JexlOperator;
use crate::value::Value;

/// port of: JexlSandbox.NULL — the marker for "explicitly denied", distinct from "not restricted".
pub const NULL: &str = "?";

/// What a name resolves to after the sandbox has had its say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// the (possibly aliased) name to use
    Name(String),
    /// JexlSandbox.NULL: the null property/method is denied
    Denied,
    /// Java's `null`: the access is blocked
    Blocked,
}

/// port of: JexlSandbox.Names
#[derive(Clone, Debug)]
enum Names {
    /// ALLOW_NAMES: everything passes through unchanged
    AllowAll,
    /// BLOCK_NAMES: nothing passes
    BlockAll,
    /// AllowSet: only the listed names, possibly aliased
    Allow(Option<HashMap<String, String>>),
    /// BlockSet: everything but the listed names
    Block(Option<HashSet<String>>),
}

impl Names {
    // port of: Names.add
    fn add(&mut self, name: &str) -> bool {
        match self {
            Names::Allow(names) => {
                let map = names.get_or_insert_with(HashMap::new);
                map.insert(name.to_string(), name.to_string()).is_none()
            }
            Names::Block(names) => names.get_or_insert_with(HashSet::new).insert(name.to_string()),
            _ => false,
        }
    }

    // port of: Names.alias
    fn alias(&mut self, name: &str, alias: &str) -> bool {
        match self {
            Names::Allow(names) => {
                let map = names.get_or_insert_with(HashMap::new);
                map.insert(alias.to_string(), name.to_string()).is_none()
            }
            _ => false,
        }
    }

    /// port of: Names.get — `name` None is Java's null name
    fn get(&self, name: Option<&str>) -> Resolved {
        match self {
            Names::AllowAll => match name {
                Some(n) => Resolved::Name(n.to_string()),
                None => Resolved::Blocked,
            },
            Names::BlockAll => match name {
                Some(_) => Resolved::Blocked,
                None => Resolved::Denied,
            },
            Names::Allow(None) => match name {
                Some(n) => Resolved::Name(n.to_string()),
                None => Resolved::Blocked,
            },
            Names::Allow(Some(names)) => {
                let key = name.unwrap_or("");
                let actual = if name.is_some() { names.get(key) } else { names.get("") };
                match actual {
                    Some(a) => Resolved::Name(a.clone()),
                    // a null name that was never listed is denied, not merely blocked
                    None if name.is_none() && !names.contains_key("") => Resolved::Denied,
                    None => Resolved::Blocked,
                }
            }
            Names::Block(names) => match (names, name) {
                (Some(set), Some(n)) if !set.contains(n) => Resolved::Name(n.to_string()),
                (_, Some(_)) => Resolved::Blocked,
                (_, None) => Resolved::Denied,
            },
        }
    }
}

/// port of: JexlSandbox.Permissions
#[derive(Clone, Debug)]
pub struct Permissions {
    inheritable: bool,
    read: Names,
    write: Names,
    execute: Names,
}

impl Permissions {
    fn new(inherit: bool, read_flag: bool, write_flag: bool, execute_flag: bool) -> Permissions {
        let set = |flag: bool| if flag { Names::Allow(None) } else { Names::Block(None) };
        Permissions { inheritable: inherit, read: set(read_flag), write: set(write_flag), execute: set(execute_flag) }
    }

    fn allow_all() -> Permissions {
        Permissions { inheritable: false, read: Names::AllowAll, write: Names::AllowAll, execute: Names::AllowAll }
    }

    fn block_all() -> Permissions {
        Permissions { inheritable: false, read: Names::BlockAll, write: Names::BlockAll, execute: Names::BlockAll }
    }

    pub fn is_inheritable(&self) -> bool {
        self.inheritable
    }

    // port of: Permissions.read(String...)
    pub fn read(&mut self, names: &[&str]) -> &mut Self {
        for n in names {
            self.read.add(n);
        }
        self
    }

    // port of: Permissions.write(String...)
    pub fn write(&mut self, names: &[&str]) -> &mut Self {
        for n in names {
            self.write.add(n);
        }
        self
    }

    // port of: Permissions.execute(String...)
    pub fn execute(&mut self, names: &[&str]) -> &mut Self {
        for n in names {
            self.execute.add(n);
        }
        self
    }

    /// port of: Names.alias on the read set
    pub fn alias_read(&mut self, name: &str, alias: &str) -> &mut Self {
        self.read.alias(name, alias);
        self
    }

    /// port of: Names.alias on the write set
    pub fn alias_write(&mut self, name: &str, alias: &str) -> &mut Self {
        self.write.alias(name, alias);
        self
    }
}

/// port of: org.apache.commons.jexl3.introspection.JexlSandbox
///
/// Java keys the sandbox by `Class.getName()`; this port keys it by the Java class name a value
/// reports (`Value::class_name`), which is the same string.
#[derive(Clone, Debug)]
pub struct JexlSandbox {
    sandbox: HashMap<String, Permissions>,
    inherit: bool,
    allow: bool,
}

impl Default for JexlSandbox {
    fn default() -> Self {
        JexlSandbox::new(true, false)
    }
}

impl JexlSandbox {
    // port of: JexlSandbox(boolean, boolean)
    pub fn new(allow: bool, inherit: bool) -> JexlSandbox {
        JexlSandbox { sandbox: HashMap::new(), inherit, allow }
    }

    // port of: JexlSandbox.copy
    pub fn copy(&self) -> JexlSandbox {
        self.clone()
    }

    // port of: JexlSandbox.permissions(String, boolean, boolean, boolean, boolean)
    pub fn permissions(&mut self, clazz: &str, inhf: bool, readf: bool, writef: bool, execf: bool) -> &mut Permissions {
        self.sandbox.insert(clazz.to_string(), Permissions::new(inhf, readf, writef, execf));
        self.sandbox.get_mut(clazz).expect("just inserted")
    }

    // port of: JexlSandbox.allow
    pub fn allow(&mut self, clazz: &str) -> &mut Permissions {
        let inherit = self.inherit;
        self.permissions(clazz, inherit, true, true, true)
    }

    // port of: JexlSandbox.block
    pub fn block(&mut self, clazz: &str) -> &mut Permissions {
        let inherit = self.inherit;
        self.permissions(clazz, inherit, false, false, false)
    }

    /// port of: JexlSandbox.get(Class) — without a class hierarchy, only the exact name matches.
    // ponytail: `inherit` needs a supertype chain the value model does not carry; a host object
    // can declare its own name only. Interfaces/superclasses are recorded in COMPATIBILITY.md.
    pub fn get(&self, clazz: &str) -> &Permissions {
        static ALLOW: std::sync::OnceLock<Permissions> = std::sync::OnceLock::new();
        static BLOCK: std::sync::OnceLock<Permissions> = std::sync::OnceLock::new();
        match self.sandbox.get(clazz) {
            Some(p) => p,
            None if self.allow => ALLOW.get_or_init(Permissions::allow_all),
            None => BLOCK.get_or_init(Permissions::block_all),
        }
    }

    // port of: JexlSandbox.read
    pub fn read(&self, clazz: &str, name: Option<&str>) -> Resolved {
        self.get(clazz).read.get(name)
    }

    // port of: JexlSandbox.write
    pub fn write(&self, clazz: &str, name: Option<&str>) -> Resolved {
        self.get(clazz).write.get(name)
    }

    // port of: JexlSandbox.execute
    pub fn execute(&self, clazz: &str, name: Option<&str>) -> Resolved {
        let m = self.get(clazz).execute.get(name);
        // execute("") means "the constructor": it resolves to the class itself
        if name == Some("") {
            return match m {
                Resolved::Blocked => Resolved::Blocked,
                _ => Resolved::Name(clazz.to_string()),
            };
        }
        m
    }
}

/// port of: org.apache.commons.jexl3.internal.introspection.SandboxUberspect
pub struct SandboxUberspect {
    uberspect: Arc<dyn JexlUberspect>,
    sandbox: JexlSandbox,
}

impl SandboxUberspect {
    // port of: SandboxUberspect(JexlUberspect, JexlSandbox)
    pub fn new(uberspect: Arc<dyn JexlUberspect>, sandbox: &JexlSandbox) -> SandboxUberspect {
        SandboxUberspect { uberspect, sandbox: sandbox.copy() }
    }
}

impl JexlUberspect for SandboxUberspect {
    fn get_resolvers(&self, op: Option<JexlOperator>, obj: &Value) -> &'static [PropertyResolver] {
        self.uberspect.get_resolvers(op, obj)
    }

    fn get_version(&self) -> i32 {
        self.uberspect.get_version()
    }

    // port of: SandboxUberspect.getConstructor
    fn get_constructor(&self, ctor_handle: &Value, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        let class_name = match ctor_handle {
            Value::Null => return None,
            other => other.java_to_string(),
        };
        match self.sandbox.execute(&class_name, Some("")) {
            Resolved::Name(n) => self.uberspect.get_constructor(&Value::string(&n), args),
            _ => None,
        }
    }

    // port of: SandboxUberspect.getMethod
    fn get_method(&self, obj: &Value, method: &str, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        if obj.is_null() {
            return None;
        }
        match self.sandbox.execute(&obj.class_name(), Some(method)) {
            Resolved::Name(actual) => self.uberspect.get_method(obj, &actual, args),
            _ => None,
        }
    }

    // port of: SandboxUberspect.getPropertyGet
    fn get_property_get_with(
        &self,
        resolvers: &[PropertyResolver],
        obj: &Value,
        identifier: &Value,
    ) -> Option<Arc<dyn JexlPropertyGet>> {
        if obj.is_null() {
            return None;
        }
        if identifier.is_null() {
            return match self.sandbox.read(&obj.class_name(), None) {
                Resolved::Denied => None,
                _ => self.uberspect.get_property_get_with(resolvers, obj, identifier),
            };
        }
        let property = identifier.java_to_string();
        match self.sandbox.read(&obj.class_name(), Some(&property)) {
            Resolved::Name(actual) => {
                // Java keeps the original identifier object when the name is unchanged
                let pty = if actual == property { identifier.clone() } else { Value::string(&actual) };
                self.uberspect.get_property_get_with(resolvers, obj, &pty)
            }
            _ => None,
        }
    }

    // port of: SandboxUberspect.getPropertySet
    fn get_property_set_with(
        &self,
        resolvers: &[PropertyResolver],
        obj: &Value,
        identifier: &Value,
        arg: &Value,
    ) -> Option<Arc<dyn JexlPropertySet>> {
        if obj.is_null() {
            return None;
        }
        if identifier.is_null() {
            return match self.sandbox.write(&obj.class_name(), None) {
                Resolved::Denied => None,
                _ => self.uberspect.get_property_set_with(resolvers, obj, identifier, arg),
            };
        }
        let property = identifier.java_to_string();
        match self.sandbox.write(&obj.class_name(), Some(&property)) {
            Resolved::Name(actual) => {
                let pty = if actual == property { identifier.clone() } else { Value::string(&actual) };
                self.uberspect.get_property_set_with(resolvers, obj, &pty, arg)
            }
            _ => None,
        }
    }

    fn get_iterator(&self, obj: &Value) -> Option<Box<dyn Iterator<Item = Value> + Send>> {
        self.uberspect.get_iterator(obj)
    }

    fn get_operator(&self, operator: JexlOperator, args: &[Value]) -> Option<Arc<dyn JexlMethod>> {
        self.uberspect.get_operator(operator, args)
    }

    fn overloads(&self, operator: JexlOperator) -> bool {
        self.uberspect.overloads(operator)
    }
}
