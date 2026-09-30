// port of: org.apache.commons.jexl3.internal.Engine (the parts that do not need an interpreter yet)
use std::collections::BTreeMap;

use crate::java::hash_map::JHashMap;
use crate::java::string::JString;
use crate::parser::jexl_node::{NodeRef, Parsed};
use crate::parser::parser_tree_constants::*;
use crate::value::{JMap, MapKind, Value};

/// The pragmas of a script: `Collections.unmodifiableMap(TreeMap)`, or `Collections.emptyMap()`
/// when the parser saw none (Parser.parse decides between the two).
pub fn pragmas_as_map(pragmas: &BTreeMap<JString, Value>) -> JMap {
    if pragmas.is_empty() {
        return JMap::new(MapKind::Empty, JHashMap::new_linked());
    }
    let mut m = JHashMap::new_linked();
    for (k, v) in pragmas {
        m.put(Value::String(k.clone()), v.clone());
    }
    JMap::new(MapKind::Unmodifiable, m)
}

/// Characters java.lang.Character.isSpaceChar accepts (measured on JDK 25).
pub(crate) fn is_space_char(c: u16) -> bool {
    matches!(c, 0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000)
}

// port of: Engine.trimSource
//
// Java trims UTF-16 code units; a Rust &str holds no lone surrogates and every space char is in
// the BMP, so trimming chars is the same thing without materialising the UTF-16.
pub fn trim_source(str: &str) -> String {
    str.trim_matches(|c: char| {
        let mut units = [0u16; 2];
        let encoded = c.encode_utf16(&mut units);
        encoded.len() == 1 && is_space_char(encoded[0])
    })
    .to_string()
}

/// port of: Engine.VarCollector
struct VarCollector {
    refs: Vec<Vec<JString>>,
    reference: Vec<JString>,
    /// whether the current root is an ASTIdentifier (VarCollector.isCollecting)
    collecting: bool,
    mode: i32,
}

impl VarCollector {
    fn new(mode: i32) -> Self {
        VarCollector { refs: Vec::new(), reference: Vec::new(), collecting: false, mode }
    }

    // port of: VarCollector.collect
    fn collect(&mut self, identifier: bool) {
        if !self.reference.is_empty() {
            // LinkedHashSet: keep insertion order, drop duplicates
            if !self.refs.contains(&self.reference) {
                self.refs.push(self.reference.clone());
            }
            self.reference = Vec::new();
        }
        self.collecting = identifier;
    }

    fn add(&mut self, name: JString) {
        self.reference.push(name);
    }
}

/// port of: Engine.getVariables(ASTJexlScript) with the default collectMode of 1
pub fn get_variables(parsed: &Parsed) -> Vec<Vec<JString>> {
    get_variables_mode(parsed, 1)
}

pub fn get_variables_mode(parsed: &Parsed, mode: i32) -> Vec<Vec<JString>> {
    let mut collector = VarCollector::new(mode);
    let root = parsed.ast.node(parsed.root);
    collect(parsed, root, &mut collector);
    collector.collect(false);
    collector.refs
}

// port of: Engine.getVariables(ASTJexlScript, JexlNode, VarCollector)
fn collect(parsed: &Parsed, node: NodeRef<'_>, collector: &mut VarCollector) {
    if node.is_identifier() {
        let parent_is_call = node
            .parent()
            .map(|p| p.is(JJTMETHODNODE) || p.is(JJTFUNCTIONNODE))
            .unwrap_or(false);
        if parent_is_call {
            // skip identifiers for methods and functions
            collector.collect(false);
            return;
        }
        let identifier = node.identifier().expect("identifier");
        let symbol = identifier.get_symbol();
        // records the variable name in the collector
        let captured = parsed
            .ast
            .node(parsed.root)
            .get_scope()
            .map(|s| s.is_captured_symbol(symbol))
            .unwrap_or(false);
        if symbol >= 0 && !captured {
            collector.collect(false);
        } else {
            collector.collect(true);
            collector.add(JString::from(identifier.get_name()));
        }
    } else if node.is_identifier_access() {
        let parent_is_call = node
            .parent()
            .map(|p| p.is(JJTMETHODNODE) || p.is(JJTFUNCTIONNODE))
            .unwrap_or(false);
        if parent_is_call {
            // skip identifiers for methods and functions
            collector.collect(false);
            return;
        }
        if collector.collecting {
            collector.add(node.identifier_access().expect("access").get_name().clone());
        }
    } else if node.is(JJTARRAYACCESS) && collector.mode > 0 {
        let num = node.num_children();
        // collect only if array access is const and follows an identifier
        let mut collecting = collector.collecting;
        for i in 0..num {
            let child = node.child(i);
            if collecting && child.is_constant() {
                let do_collect = collector.mode > 1 || child.is(JJTSTRINGLITERAL) || child.is(JJTNUMBERLITERAL);
                if do_collect {
                    collector.add(node_image(child));
                }
            } else {
                collecting = false;
                collector.collect(false);
                collect(parsed, child, collector);
                collector.collect(false);
            }
        }
    } else {
        for child in node.children() {
            collect(parsed, child, collector);
        }
        collector.collect(false);
    }
}

/// JexlNode.toString() of the constant nodes an array access can hold
fn node_image(node: NodeRef<'_>) -> JString {
    if let Some(n) = node.number() {
        return JString::from(n.to_java_string());
    }
    if let Some(l) = node.literal() {
        return l.clone();
    }
    if let Some(p) = node.regex() {
        return JString::from(p.pattern());
    }
    // ASTArrayLiteral, ASTMapLiteral and ASTSetLiteral are the only nodes that override
    // toString(); they render themselves through the Debugger. Everything else falls back to
    // SimpleNode.toString(), which is jjtNodeName[id].
    if matches!(node.kind(), JJTARRAYLITERAL | JJTMAPLITERAL | JJTSETLITERAL) {
        return crate::internal::debugger::Debugger::new().data(node);
    }
    JString::from(node.node_name())
}
