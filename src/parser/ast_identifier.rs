// port of: org.apache.commons.jexl3.parser.ASTIdentifier (also ASTVar and ASTNamespaceIdentifier data)

const REDEFINED: i32 = 0;
const SHADED: i32 = 1;
const CAPTURED: i32 = 2;

#[derive(Clone, Debug)]
pub struct ASTIdentifier {
    pub(crate) name: String,
    pub(crate) symbol: i32,
    flags: i32,
    /// ASTNamespaceIdentifier.namespace
    pub(crate) namespace: Option<String>,
}

impl Default for ASTIdentifier {
    fn default() -> Self {
        ASTIdentifier { name: String::new(), symbol: -1, flags: 0, namespace: None }
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

impl ASTIdentifier {
    // port of: ASTIdentifier.setSymbol(String)
    pub(crate) fn set_symbol_name(&mut self, identifier: &str) {
        if let Some(reg) = identifier.strip_prefix('#') {
            // Integer.parseInt on the digits of a REGISTER token (always digits)
            self.symbol = reg.parse::<i32>().unwrap_or(-1);
        }
        self.name = identifier.to_string();
    }
    // port of: ASTIdentifier.setSymbol(int, String)
    pub(crate) fn set_symbol(&mut self, r: i32, identifier: &str) {
        self.symbol = r;
        self.name = identifier.to_string();
    }
    pub fn get_symbol(&self) -> i32 {
        self.symbol
    }
    pub fn get_name(&self) -> &str {
        &self.name
    }
    pub fn get_namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }
    // port of: ASTNamespaceIdentifier.setNamespace
    pub(crate) fn set_namespace(&mut self, ns: &str, id: &str) {
        self.namespace = Some(ns.to_string());
        self.name = id.to_string();
    }
    pub fn set_redefined(&mut self, f: bool) {
        self.flags = set(REDEFINED, self.flags, f);
    }
    pub fn is_redefined(&self) -> bool {
        is_set(REDEFINED, self.flags)
    }
    pub fn set_shaded(&mut self, f: bool) {
        self.flags = set(SHADED, self.flags, f);
    }
    pub fn is_shaded(&self) -> bool {
        is_set(SHADED, self.flags)
    }
    pub fn set_captured(&mut self, f: bool) {
        self.flags = set(CAPTURED, self.flags, f);
    }
    pub fn is_captured(&self) -> bool {
        is_set(CAPTURED, self.flags)
    }
}
