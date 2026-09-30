// port of: org.apache.commons.jexl3.JexlOptions
use std::collections::HashMap;

use crate::java::big_decimal::MathContext;
use crate::value::Value;

const SHARED: i32 = 7;
const SHADE: i32 = 6;
const ANTISH: i32 = 5;
const LEXICAL: i32 = 4;
const SAFE: i32 = 3;
const SILENT: i32 = 2;
const STRICT: i32 = 1;
const CANCELLABLE: i32 = 0;

const NAMES: [&str; 8] = ["cancellable", "strict", "silent", "safe", "lexical", "antish", "lexicalShade", "sharedInstance"];

/// JexlOptions.DEFAULT: `1 | 1 << STRICT | 1 << ANTISH | 1 << SAFE`
// ponytail: Java lets a static setDefaultFlags() change this process-wide; not modeled.
const DEFAULT: i32 = 1 | (1 << STRICT) | (1 << ANTISH) | (1 << SAFE);

/// Flags and properties that can alter the evaluation behavior.
#[derive(Clone, Debug)]
pub struct JexlOptions {
    math_context: Option<MathContext>,
    math_scale: i32,
    strict_arithmetic: bool,
    flags: i32,
    /// shared, not copied: every execution clones the options, almost none changes them
    namespaces: std::sync::Arc<HashMap<String, Value>>,
}

impl Default for JexlOptions {
    fn default() -> Self {
        JexlOptions::new()
    }
}

fn set(ordinal: i32, mask: i32, value: bool) -> i32 {
    if value {
        mask | (1 << ordinal)
    } else {
        mask & !(1 << ordinal)
    }
}

fn is_set(ordinal: i32, mask: i32) -> bool {
    (mask & (1 << ordinal)) != 0
}

impl JexlOptions {
    // port of: JexlOptions()
    pub fn new() -> JexlOptions {
        JexlOptions {
            math_context: None,
            math_scale: i32::MIN,
            strict_arithmetic: true,
            flags: DEFAULT,
            namespaces: std::sync::Arc::new(HashMap::new()),
        }
    }

    // port of: JexlOptions.parseFlags
    pub fn parse_flags(mut mask: i32, flags: &[&str]) -> i32 {
        for name in flags {
            let mut name: &str = name;
            let mut b = true;
            if let Some(rest) = name.strip_prefix('+') {
                name = rest;
            } else if let Some(rest) = name.strip_prefix('-') {
                name = rest;
                b = false;
            }
            for (flag, n) in NAMES.iter().enumerate() {
                if *n == name {
                    if b {
                        mask |= 1 << flag;
                    } else {
                        mask &= !(1 << flag);
                    }
                    break;
                }
            }
        }
        mask
    }

    // port of: JexlOptions.setFlags
    pub fn set_flags(&mut self, opts: &[&str]) {
        self.flags = Self::parse_flags(self.flags, opts);
    }

    pub fn get_math_context(&self) -> Option<&MathContext> {
        self.math_context.as_ref()
    }
    pub fn get_math_scale(&self) -> i32 {
        self.math_scale
    }
    pub fn is_antish(&self) -> bool {
        is_set(ANTISH, self.flags)
    }
    pub fn is_cancellable(&self) -> bool {
        is_set(CANCELLABLE, self.flags)
    }
    pub fn is_lexical(&self) -> bool {
        is_set(LEXICAL, self.flags)
    }
    pub fn is_lexical_shade(&self) -> bool {
        is_set(SHADE, self.flags)
    }
    pub fn is_safe(&self) -> bool {
        is_set(SAFE, self.flags)
    }
    pub fn is_silent(&self) -> bool {
        is_set(SILENT, self.flags)
    }
    pub fn is_strict(&self) -> bool {
        is_set(STRICT, self.flags)
    }
    pub fn is_strict_arithmetic(&self) -> bool {
        self.strict_arithmetic
    }
    pub fn is_shared_instance(&self) -> bool {
        is_set(SHARED, self.flags)
    }
    pub fn set_antish(&mut self, flag: bool) {
        self.flags = set(ANTISH, self.flags, flag);
    }
    pub fn set_cancellable(&mut self, flag: bool) {
        self.flags = set(CANCELLABLE, self.flags, flag);
    }
    pub fn set_lexical(&mut self, flag: bool) {
        self.flags = set(LEXICAL, self.flags, flag);
    }
    pub fn set_lexical_shade(&mut self, flag: bool) {
        self.flags = set(SHADE, self.flags, flag);
        if flag {
            self.flags = set(LEXICAL, self.flags, true);
        }
    }
    pub fn set_math_context(&mut self, mcontext: Option<MathContext>) {
        self.math_context = mcontext;
    }
    pub fn set_math_scale(&mut self, mscale: i32) {
        self.math_scale = mscale;
    }
    pub fn set_safe(&mut self, flag: bool) {
        self.flags = set(SAFE, self.flags, flag);
    }
    pub fn set_silent(&mut self, flag: bool) {
        self.flags = set(SILENT, self.flags, flag);
    }
    pub fn set_strict(&mut self, flag: bool) {
        self.flags = set(STRICT, self.flags, flag);
    }
    pub fn set_strict_arithmetic(&mut self, stricta: bool) {
        self.strict_arithmetic = stricta;
    }
    pub fn set_shared_instance(&mut self, flag: bool) {
        self.flags = set(SHARED, self.flags, flag);
    }
    pub(crate) fn shared_namespaces(&self) -> std::sync::Arc<HashMap<String, Value>> {
        self.namespaces.clone()
    }
    pub fn get_namespaces(&self) -> &HashMap<String, Value> {
        &self.namespaces
    }
    pub fn set_namespaces(&mut self, ns: HashMap<String, Value>) {
        self.namespaces = std::sync::Arc::new(ns);
    }
    // port of: JexlOptions.set(JexlOptions)
    pub fn set(&mut self, src: &JexlOptions) {
        *self = src.clone();
    }
    // port of: JexlOptions.copy
    pub fn copy(&self) -> JexlOptions {
        self.clone()
    }
}
