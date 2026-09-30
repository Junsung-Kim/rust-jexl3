// port of: org.apache.commons.jexl3.JexlFeatures
use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

/// A namespace predicate (JexlFeatures.namespaceTest).
pub type NamespaceTest = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// A set of language feature options, used to restrict the grammar accepted by a script.
#[derive(Clone)]
pub struct JexlFeatures {
    flags: i64,
    reserved_names: BTreeSet<String>,
    /// None stands for JexlFeatures.TEST_STR_FALSE (the identity-compared default)
    name_spaces: Option<NamespaceTest>,
}

const F_NAMES: [&str; 16] = [
    "register",
    "reserved variable",
    "local variable",
    "assign/modify",
    "global assign/modify",
    "array reference",
    "create instance",
    "loop",
    "function",
    "method call",
    "set/map/array literal",
    "pragma",
    "annotation",
    "script",
    "lexical",
    "lexicalShade",
];

const REGISTER: i32 = 0;
pub const RESERVED: i32 = 1;
pub const LOCAL_VAR: i32 = 2;
pub const SIDE_EFFECT: i32 = 3;
pub const SIDE_EFFECT_GLOBAL: i32 = 4;
pub const ARRAY_REF_EXPR: i32 = 5;
pub const NEW_INSTANCE: i32 = 6;
pub const LOOP: i32 = 7;
pub const LAMBDA: i32 = 8;
pub const METHOD_CALL: i32 = 9;
pub const STRUCTURED_LITERAL: i32 = 10;
pub const PRAGMA: i32 = 11;
pub const ANNOTATION: i32 = 12;
pub const SCRIPT: i32 = 13;
pub const LEXICAL: i32 = 14;
pub const LEXICAL_SHADE: i32 = 15;

impl Default for JexlFeatures {
    fn default() -> Self {
        JexlFeatures::new()
    }
}

impl PartialEq for JexlFeatures {
    // port of: JexlFeatures.equals (flags and reserved names; the namespace predicate is ignored)
    fn eq(&self, other: &Self) -> bool {
        self.flags == other.flags && self.reserved_names == other.reserved_names
    }
}
impl Eq for JexlFeatures {}

impl std::hash::Hash for JexlFeatures {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.flags.hash(state);
        self.reserved_names.hash(state);
    }
}

impl fmt::Debug for JexlFeatures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JexlFeatures({:#x}, {:?})", self.flags, self.reserved_names)
    }
}

impl JexlFeatures {
    // port of: JexlFeatures()
    pub fn new() -> JexlFeatures {
        JexlFeatures {
            flags: (1 << LOCAL_VAR)
                | (1 << SIDE_EFFECT)
                | (1 << SIDE_EFFECT_GLOBAL)
                | (1 << ARRAY_REF_EXPR)
                | (1 << NEW_INSTANCE)
                | (1 << LOOP)
                | (1 << LAMBDA)
                | (1 << METHOD_CALL)
                | (1 << STRUCTURED_LITERAL)
                | (1 << PRAGMA)
                | (1 << ANNOTATION)
                | (1 << SCRIPT),
            reserved_names: BTreeSet::new(),
            name_spaces: None,
        }
    }

    // port of: JexlFeatures.stringify
    pub fn stringify(feature: i32) -> &'static str {
        if (0..F_NAMES.len() as i32).contains(&feature) {
            F_NAMES[feature as usize]
        } else {
            "unsupported feature"
        }
    }

    // port of: JexlFeatures.reservedNames
    pub fn reserved_names<I: IntoIterator<Item = S>, S: Into<String>>(mut self, names: I) -> Self {
        self.reserved_names = names.into_iter().map(Into::into).collect();
        let f = !self.reserved_names.is_empty();
        self.set_feature(RESERVED, f);
        self
    }

    pub fn get_reserved_names(&self) -> &BTreeSet<String> {
        &self.reserved_names
    }

    // port of: JexlFeatures.isReservedName
    pub fn is_reserved_name(&self, name: &str) -> bool {
        self.reserved_names.contains(name)
    }

    // port of: JexlFeatures.namespaceTest(Predicate)
    pub fn namespace_test(mut self, names: Option<NamespaceTest>) -> Self {
        self.name_spaces = names;
        self
    }

    /// The namespace predicate; None is TEST_STR_FALSE.
    pub fn get_namespace_test(&self) -> Option<&NamespaceTest> {
        self.name_spaces.as_ref()
    }

    // port of: JexlFeatures.namespaceTest().test(name)
    pub fn test_namespace(&self, name: &str) -> bool {
        match &self.name_spaces {
            Some(t) => t(name),
            None => false,
        }
    }

    // port of: JexlFeatures.setFeature — note the Java code sets with an int shift
    fn set_feature(&mut self, feature: i32, flag: bool) {
        if flag {
            self.flags |= (1i32 << feature) as i64;
        } else {
            self.flags &= !(1i64 << feature);
        }
    }

    fn get_feature(&self, feature: i32) -> bool {
        (self.flags & (1i64 << feature)) != 0
    }

    pub fn register(mut self, flag: bool) -> Self {
        self.set_feature(REGISTER, flag);
        self
    }
    pub fn supports_register(&self) -> bool {
        self.get_feature(REGISTER)
    }
    pub fn local_var(mut self, flag: bool) -> Self {
        self.set_feature(LOCAL_VAR, flag);
        self
    }
    pub fn supports_local_var(&self) -> bool {
        self.get_feature(LOCAL_VAR)
    }
    pub fn side_effect_global(mut self, flag: bool) -> Self {
        self.set_feature(SIDE_EFFECT_GLOBAL, flag);
        self
    }
    pub fn supports_side_effect_global(&self) -> bool {
        self.get_feature(SIDE_EFFECT_GLOBAL)
    }
    pub fn side_effect(mut self, flag: bool) -> Self {
        self.set_feature(SIDE_EFFECT, flag);
        self
    }
    pub fn supports_side_effect(&self) -> bool {
        self.get_feature(SIDE_EFFECT)
    }
    pub fn array_reference_expr(mut self, flag: bool) -> Self {
        self.set_feature(ARRAY_REF_EXPR, flag);
        self
    }
    pub fn supports_array_reference_expr(&self) -> bool {
        self.get_feature(ARRAY_REF_EXPR)
    }
    pub fn method_call(mut self, flag: bool) -> Self {
        self.set_feature(METHOD_CALL, flag);
        self
    }
    pub fn supports_method_call(&self) -> bool {
        self.get_feature(METHOD_CALL)
    }
    pub fn structured_literal(mut self, flag: bool) -> Self {
        self.set_feature(STRUCTURED_LITERAL, flag);
        self
    }
    pub fn supports_structured_literal(&self) -> bool {
        self.get_feature(STRUCTURED_LITERAL)
    }
    pub fn new_instance(mut self, flag: bool) -> Self {
        self.set_feature(NEW_INSTANCE, flag);
        self
    }
    pub fn supports_new_instance(&self) -> bool {
        self.get_feature(NEW_INSTANCE)
    }
    pub fn loops(mut self, flag: bool) -> Self {
        self.set_feature(LOOP, flag);
        self
    }
    pub fn supports_loops(&self) -> bool {
        self.get_feature(LOOP)
    }
    pub fn lambda(mut self, flag: bool) -> Self {
        self.set_feature(LAMBDA, flag);
        self
    }
    pub fn supports_lambda(&self) -> bool {
        self.get_feature(LAMBDA)
    }
    pub fn pragma(mut self, flag: bool) -> Self {
        self.set_feature(PRAGMA, flag);
        self
    }
    pub fn supports_pragma(&self) -> bool {
        self.get_feature(PRAGMA)
    }
    pub fn annotation(mut self, flag: bool) -> Self {
        self.set_feature(ANNOTATION, flag);
        self
    }
    pub fn supports_annotation(&self) -> bool {
        self.get_feature(ANNOTATION)
    }
    pub fn script(mut self, flag: bool) -> Self {
        self.set_feature(SCRIPT, flag);
        self
    }
    pub fn supports_script(&self) -> bool {
        self.get_feature(SCRIPT)
    }
    pub fn supports_expression(&self) -> bool {
        !self.get_feature(SCRIPT)
    }
    pub fn lexical(mut self, flag: bool) -> Self {
        self.set_feature(LEXICAL, flag);
        self
    }
    pub fn is_lexical(&self) -> bool {
        self.get_feature(LEXICAL)
    }
    pub fn lexical_shade(mut self, flag: bool) -> Self {
        self.set_feature(LEXICAL_SHADE, flag);
        if flag {
            self.set_feature(LEXICAL, true);
        }
        self
    }
    pub fn is_lexical_shade(&self) -> bool {
        self.get_feature(LEXICAL_SHADE)
    }
}
