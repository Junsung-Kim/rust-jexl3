// port of: org.apache.commons.jexl3.parser.ASTJexlScript (and ASTJexlLambda data)
use std::collections::BTreeMap;

use crate::internal::lexical_scope::LexicalScope;
use crate::internal::scope::ScopeId;
use crate::jexl_features::JexlFeatures;
use crate::java::string::JString;
use crate::value::Value;

#[derive(Default)]
pub struct ASTJexlScript {
    /// TreeMap<String, Object>: ordered by String.compareTo (UTF-16 order)
    pub(crate) pragmas: Option<BTreeMap<JString, Value>>,
    pub(crate) features: Option<JexlFeatures>,
    pub(crate) scope: Option<ScopeId>,
    /// JexlLexicalNode.locals
    pub(crate) locals: Option<LexicalScope>,
}

impl ASTJexlScript {
    pub fn get_pragmas(&self) -> Option<&BTreeMap<JString, Value>> {
        self.pragmas.as_ref()
    }
    pub fn get_features(&self) -> Option<&JexlFeatures> {
        self.features.as_ref()
    }
    pub fn get_scope(&self) -> Option<ScopeId> {
        self.scope
    }
    // port of: JexlLexicalNode.declareSymbol
    pub(crate) fn declare_symbol(&mut self, symbol: i32) -> bool {
        self.locals.get_or_insert_with(LexicalScope::new).add_symbol(symbol)
    }
    pub fn has_symbol(&self, symbol: i32) -> bool {
        self.locals.as_ref().map(|l| l.has_symbol(symbol)).unwrap_or(false)
    }
}
