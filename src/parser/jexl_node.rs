// port of: org.apache.commons.jexl3.parser.JexlNode (with SimpleNode and JexlLexicalNode)
//
// Java builds a tree of node objects with parent pointers. Here nodes live in an arena (`Ast`);
// a node is an index, `NodeRef` borrows the arena and `NodeHandle` shares it (closures, marks).
use std::any::Any;
use std::fmt;
use std::sync::{Arc, RwLock};

use crate::internal::lexical_scope::LexicalScope;
use crate::internal::scope::{Scope, ScopeId, Scopes};
use crate::jexl_info::JexlInfo;
use crate::parser::ast_identifier::ASTIdentifier;
use crate::parser::ast_identifier_access::ASTIdentifierAccess;
use crate::parser::ast_jexl_script::ASTJexlScript;
use crate::parser::number_parser::NumberParser;
use crate::parser::parser_tree_constants::*;

pub type NodeId = u32;

/// Per-class node payloads (the fields of the AST* classes).
pub enum NodeData {
    None,
    Identifier(ASTIdentifier),
    IdentifierAccess(ASTIdentifierAccess),
    NumberLiteral(NumberParser),
    /// ASTStringLiteral / ASTJxltLiteral literal (UTF-16)
    Literal(crate::java::string::JString),
    /// ASTRegexLiteral
    Regex(Arc<crate::java::regex::Pattern>),
    /// ASTArrayLiteral / ASTMapLiteral / ASTSetLiteral `constant`
    Constant(bool),
    /// ASTAnnotation name
    Annotation(String),
    /// ASTJexlScript / ASTJexlLambda
    Script(Box<ASTJexlScript>),
    /// ASTBlock / ASTForeachStatement (JexlLexicalNode.locals)
    Lexical(Option<LexicalScope>),
}

/// The value slot of a node (SimpleNode.value): the root's JexlInfo, or a runtime cache.
#[derive(Clone)]
pub enum NodeValue {
    Info(JexlInfo),
    Cache(Arc<dyn Any + Send + Sync>),
}

pub struct Node {
    pub(crate) id: i32,
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
    pub(crate) lc: i32,
    pub(crate) data: NodeData,
    pub(crate) value: RwLock<Option<NodeValue>>,
}

/// A parsed tree: the node arena and the scopes it refers to.
pub struct Ast {
    pub(crate) nodes: Vec<Node>,
    pub(crate) scopes: Scopes,
}

impl Ast {
    pub(crate) fn new() -> Ast {
        Ast { nodes: Vec::new(), scopes: Scopes::new() }
    }

    pub(crate) fn add(&mut self, id: i32) -> NodeId {
        let data = match id {
            JJTIDENTIFIER | JJTVAR | JJTNAMESPACEIDENTIFIER => NodeData::Identifier(ASTIdentifier::default()),
            JJTIDENTIFIERACCESS | JJTIDENTIFIERACCESSSAFE | JJTIDENTIFIERACCESSJXLT | JJTIDENTIFIERACCESSSAFEJXLT => {
                NodeData::IdentifierAccess(ASTIdentifierAccess::default())
            }
            JJTNUMBERLITERAL => NodeData::NumberLiteral(NumberParser::default()),
            JJTSTRINGLITERAL | JJTJXLTLITERAL => NodeData::Literal(crate::java::string::JString::empty()),
            JJTARRAYLITERAL | JJTMAPLITERAL | JJTSETLITERAL => NodeData::Constant(false),
            JJTANNOTATION => NodeData::Annotation(String::new()),
            JJTJEXLSCRIPT | JJTJEXLLAMBDA => NodeData::Script(Box::default()),
            JJTBLOCK | JJTFOREACHSTATEMENT => NodeData::Lexical(None),
            _ => NodeData::None,
        };
        self.nodes.push(Node { id, parent: None, children: Vec::new(), lc: -1, data, value: RwLock::new(None) });
        (self.nodes.len() - 1) as NodeId
    }

    pub fn node(&self, id: NodeId) -> NodeRef<'_> {
        NodeRef { ast: self, id }
    }

    pub(crate) fn n(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub(crate) fn n_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id as usize]
    }

    pub fn scope(&self, id: ScopeId) -> &Scope {
        self.scopes.get(id)
    }
}

/// The result of a parse: the tree and its root (ASTJexlScript, or the lone lambda `script()`
/// unwraps to).
pub struct Parsed {
    pub ast: Arc<Ast>,
    pub root: NodeId,
}

impl Parsed {
    pub fn node(&self) -> NodeRef<'_> {
        self.ast.node(self.root)
    }
    /// the root as a shared handle (exception marks, closures)
    pub fn handle(&self) -> NodeHandle {
        NodeHandle::new(self.ast.clone(), self.root)
    }
}

/// A borrowed node.
#[derive(Clone, Copy)]
pub struct NodeRef<'a> {
    pub ast: &'a Ast,
    pub id: NodeId,
}

/// A shared node: the tree it belongs to and its index.
#[derive(Clone)]
pub struct NodeHandle {
    pub ast: Arc<Ast>,
    pub id: NodeId,
}

impl NodeHandle {
    pub fn new(ast: Arc<Ast>, id: NodeId) -> NodeHandle {
        NodeHandle { ast, id }
    }
    pub fn node(&self) -> NodeRef<'_> {
        NodeRef { ast: &self.ast, id: self.id }
    }
}

impl PartialEq for NodeHandle {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.ast, &o.ast) && self.id == o.id
    }
}
impl Eq for NodeHandle {}
impl fmt::Debug for NodeHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node#{}({})", self.id, self.node().node_name())
    }
}

impl PartialEq for NodeRef<'_> {
    fn eq(&self, o: &Self) -> bool {
        std::ptr::eq(self.ast, o.ast) && self.id == o.id
    }
}

impl<'a> NodeRef<'a> {
    fn raw(&self) -> &'a Node {
        &self.ast.nodes[self.id as usize]
    }
    /// SimpleNode.id: the ParserTreeConstants kind
    pub fn kind(&self) -> i32 {
        self.raw().id
    }
    pub fn is(&self, kind: i32) -> bool {
        self.raw().id == kind
    }
    /// jjtNodeName: the production name ("JexlScript", "AddNode", ...)
    pub fn node_name(&self) -> &'static str {
        JJT_NODE_NAME[self.raw().id as usize]
    }
    /// getClass().getSimpleName() of the Java node class ("ASTJexlScript", ...)
    pub fn class_name(&self) -> String {
        format!("AST{}", self.node_name())
    }
    pub fn data(&self) -> &'a NodeData {
        &self.raw().data
    }
    // port of: SimpleNode.jjtGetParent
    pub fn parent(&self) -> Option<NodeRef<'a>> {
        self.raw().parent.map(|p| NodeRef { ast: self.ast, id: p })
    }
    // port of: SimpleNode.jjtGetNumChildren
    pub fn num_children(&self) -> usize {
        self.raw().children.len()
    }
    // port of: SimpleNode.jjtGetChild
    pub fn child(&self, i: usize) -> NodeRef<'a> {
        NodeRef { ast: self.ast, id: self.raw().children[i] }
    }
    pub fn children(&self) -> impl Iterator<Item = NodeRef<'a>> + 'a {
        let ast = self.ast;
        self.raw().children.iter().map(move |&c| NodeRef { ast, id: c })
    }
    pub fn line(&self) -> i32 {
        ((self.raw().lc as u32) >> 0xc) as i32
    }
    pub fn column(&self) -> i32 {
        self.raw().lc & 0xfff
    }
    // port of: SimpleNode.jjtGetValue
    pub fn value(&self) -> Option<NodeValue> {
        self.raw().value.read().unwrap_or_else(|p| p.into_inner()).clone()
    }
    // port of: SimpleNode.jjtSetValue
    pub fn set_value(&self, v: Option<NodeValue>) {
        *self.raw().value.write().unwrap_or_else(|p| p.into_inner()) = v;
    }

    pub fn identifier(&self) -> Option<&'a ASTIdentifier> {
        match &self.raw().data {
            NodeData::Identifier(i) => Some(i),
            _ => None,
        }
    }
    pub fn identifier_access(&self) -> Option<&'a ASTIdentifierAccess> {
        match &self.raw().data {
            NodeData::IdentifierAccess(i) => Some(i),
            _ => None,
        }
    }
    pub fn script(&self) -> Option<&'a ASTJexlScript> {
        match &self.raw().data {
            NodeData::Script(s) => Some(s),
            _ => None,
        }
    }
    pub fn number(&self) -> Option<&'a NumberParser> {
        match &self.raw().data {
            NodeData::NumberLiteral(n) => Some(n),
            _ => None,
        }
    }
    /// ASTStringLiteral / ASTJxltLiteral literal
    pub fn literal(&self) -> Option<&'a crate::java::string::JString> {
        match &self.raw().data {
            NodeData::Literal(s) => Some(s),
            _ => None,
        }
    }
    pub fn regex(&self) -> Option<&'a Arc<crate::java::regex::Pattern>> {
        match &self.raw().data {
            NodeData::Regex(p) => Some(p),
            _ => None,
        }
    }
    pub fn annotation_name(&self) -> Option<&'a str> {
        match &self.raw().data {
            NodeData::Annotation(n) => Some(n),
            _ => None,
        }
    }

    // port of: JexlLexicalNode.getLexicalScope (also ASTJexlScript)
    pub fn lexical_scope(&self) -> Option<&'a LexicalScope> {
        match &self.raw().data {
            NodeData::Lexical(l) => l.as_ref(),
            NodeData::Script(s) => s.locals.as_ref(),
            _ => None,
        }
    }

    // port of: JexlLexicalNode.getSymbolCount
    pub fn symbol_count(&self) -> i32 {
        self.lexical_scope().map(|l| l.get_symbol_count()).unwrap_or(0)
    }

    /// instanceof JexlParser.LexicalUnit
    pub fn is_lexical_unit(&self) -> bool {
        matches!(self.raw().data, NodeData::Lexical(_) | NodeData::Script(_))
    }

    /// instanceof ASTJexlScript (lambdas included)
    pub fn is_script(&self) -> bool {
        matches!(self.raw().data, NodeData::Script(_))
    }

    /// instanceof ASTIdentifier (Var and NamespaceIdentifier included)
    pub fn is_identifier(&self) -> bool {
        matches!(self.kind(), JJTIDENTIFIER | JJTVAR | JJTNAMESPACEIDENTIFIER)
    }

    /// instanceof ASTIdentifierAccess (safe and jxlt variants included)
    pub fn is_identifier_access(&self) -> bool {
        matches!(self.kind(), JJTIDENTIFIERACCESS | JJTIDENTIFIERACCESSSAFE | JJTIDENTIFIERACCESSJXLT | JJTIDENTIFIERACCESSSAFEJXLT)
    }

    // port of: JexlNode.jexlInfo
    pub fn jexl_info(&self) -> Option<JexlInfo> {
        let mut info = None;
        let mut node = Some(*self);
        while let Some(n) = node {
            if let Some(NodeValue::Info(i)) = n.value() {
                info = Some(i);
                break;
            }
            node = n.parent();
        }
        let lc = self.raw().lc;
        if lc >= 0 {
            let c = lc & 0xfff;
            let l = lc >> 0xc;
            return Some(match info {
                Some(i) => i.at(i.get_line() + l - 1, c),
                None => JexlInfo::new(None, l, c),
            });
        }
        info
    }

    // port of: JexlNode.isConstant()
    pub fn is_constant(&self) -> bool {
        match self.kind() {
            JJTARRAYLITERAL | JJTMAPLITERAL | JJTSETLITERAL => matches!(self.raw().data, NodeData::Constant(true)),
            JJTNUMBERLITERAL | JJTSTRINGLITERAL | JJTREGEXLITERAL => true,
            _ => false,
        }
    }

    // port of: JexlNode.isConstant(boolean)
    pub fn is_constant_literal(&self, literal: bool) -> bool {
        match self.kind() {
            JJTARRAYLITERAL | JJTMAPLITERAL | JJTSETLITERAL => matches!(self.raw().data, NodeData::Constant(true)),
            JJTNUMBERLITERAL | JJTSTRINGLITERAL | JJTREGEXLITERAL => true,
            _ => {
                if literal {
                    for child in self.children() {
                        if child.is(JJTREFERENCE) || child.is(JJTMAPENTRY) {
                            if !child.is_constant_literal(true) {
                                return false;
                            }
                        } else if !child.is_constant() {
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            }
        }
    }

    // port of: JexlNode.isLeftValue
    pub fn is_left_value(&self) -> bool {
        let mut walk = *self;
        loop {
            if walk.is_identifier() || walk.is_identifier_access() || walk.is(JJTARRAYACCESS) {
                return true;
            }
            let nc = walk.num_children();
            if nc == 0 {
                return walk.parent().map(|p| p.is(JJTREFERENCE)).unwrap_or(false);
            }
            walk = walk.child(nc - 1);
        }
    }

    // port of: JexlNode.isGlobalVar (and the ASTIdentifierAccess override)
    pub fn is_global_var(&self) -> bool {
        if self.is(JJTVAR) {
            return false;
        }
        if self.is_identifier() {
            return self.identifier().map(|i| i.get_symbol() < 0).unwrap_or(false);
        }
        if self.is_identifier_access() {
            return !self.is_safe() && !self.is_expression();
        }
        if self.num_children() > 0 {
            return self.child(0).is_global_var();
        }
        self.parent().map(|p| p.is(JJTREFERENCE)).unwrap_or(false)
    }

    // port of: JexlNode.isLocalVar
    pub fn is_local_var(&self) -> bool {
        self.is_identifier() && self.identifier().map(|i| i.get_symbol() >= 0).unwrap_or(false)
    }

    /// ASTIdentifierAccess.isSafe
    pub fn is_safe(&self) -> bool {
        matches!(self.kind(), JJTIDENTIFIERACCESSSAFE | JJTIDENTIFIERACCESSSAFEJXLT)
    }

    /// ASTIdentifierAccess.isExpression
    pub fn is_expression(&self) -> bool {
        matches!(self.kind(), JJTIDENTIFIERACCESSJXLT | JJTIDENTIFIERACCESSSAFEJXLT)
    }

    // port of: JexlNode.isSafeLhs
    pub fn is_safe_lhs(&self, safe: bool) -> bool {
        if self.is(JJTREFERENCE) {
            return self.child(0).is_safe_lhs(safe);
        }
        if self.is(JJTMETHODNODE)
            && self.num_children() > 1
            && self.child(0).is_identifier_access()
            && (self.child(0).is_safe() || safe)
        {
            return true;
        }
        let parent = match self.parent() {
            None => return false,
            Some(p) => p,
        };
        let nsiblings = parent.num_children();
        let mut rhs: isize = -1;
        for s in 0..nsiblings {
            if parent.child(s) == *self {
                rhs = s as isize + 1;
                break;
            }
        }
        if rhs >= 0 && (rhs as usize) < nsiblings {
            let mut rsibling = parent.child(rhs as usize);
            if rsibling.is(JJTMETHODNODE) || rsibling.is(JJTFUNCTIONNODE) {
                rsibling = rsibling.child(0);
            }
            if rsibling.is_identifier_access() && (rsibling.is_safe() || safe) {
                return true;
            }
            if rsibling.is(JJTARRAYACCESS) {
                return safe;
            }
        }
        false
    }

    // port of: JexlNode.isTernaryProtected
    pub fn is_ternary_protected(&self) -> bool {
        let mut node = *self;
        let mut walk = node.parent();
        while let Some(w) = walk {
            if w.is(JJTTERNARYNODE) || w.is(JJTNULLPNODE) {
                return node == w.child(0);
            }
            if !(w.is(JJTREFERENCE) || w.is(JJTARRAYACCESS)) {
                break;
            }
            node = w;
            walk = w.parent();
        }
        false
    }

    // port of: JexlNode.clearCache
    pub fn clear_cache(&self) {
        if let Some(NodeValue::Cache(_)) = self.value() {
            self.set_value(None);
        }
        for c in self.children() {
            c.clear_cache();
        }
    }

    /// ASTJexlScript.getScope
    pub fn get_scope(&self) -> Option<&'a Scope> {
        self.script().and_then(|s| s.scope).map(|id| self.ast.scope(id))
    }

    /// ASTJexlLambda.isTopLevel
    pub fn is_top_level(&self) -> bool {
        self.parent().is_none()
    }
}
