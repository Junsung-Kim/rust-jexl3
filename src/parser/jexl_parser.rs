// port of: org.apache.commons.jexl3.parser.JexlParser (the semantic actions of Parser.jjt)
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::sync::Arc;

use crate::internal::scope::ScopeId;
use crate::java::string::JString;
use crate::jexl_exception::JexlException;
use crate::jexl_features::{self, JexlFeatures};
use crate::jexl_info::JexlInfo;
use crate::parser::jexl_node::{NodeData, NodeId};
use crate::parser::parser::{PErr, Parser, Tok};
use crate::parser::parser_tree_constants::*;
use crate::parser::string_parser;
use crate::value::Value;

/// port of: JexlParser.PRAGMA_JEXLNS
pub(crate) const PRAGMA_JEXLNS: &str = "jexl.namespace.";

impl Parser {
    // port of: JexlParser.cleanup
    pub(crate) fn cleanup(&mut self, features: JexlFeatures) {
        self.info = None;
        self.source = None;
        self.frame = None;
        self.frames.clear();
        self.pragmas = None;
        self.namespaces = None;
        self.loop_counts.clear();
        self.loop_count = 0;
        self.blocks.clear();
        self.block = None;
        self.set_features(features);
    }

    // port of: JexlParser.readSourceLine
    fn read_source_line(&self, lineno: i32) -> JString {
        let src = match &self.source {
            Some(s) => s,
            None => return JString::empty(),
        };
        if lineno < 0 {
            return JString::empty();
        }
        let mut msg = JString::empty();
        for (l, line) in crate::jexl_exception::java_lines(src).iter().enumerate() {
            if (l as i32) < lineno {
                msg = JString::from(*line);
            }
        }
        msg
    }

    // port of: JexlParser.setFeatures
    pub(crate) fn set_features(&mut self, features: JexlFeatures) {
        self.feature_controller.set_features(features);
    }

    // port of: JexlParser.getFeatures
    pub(crate) fn get_features(&self) -> &JexlFeatures {
        self.feature_controller.get_features()
    }

    // port of: JexlParser.pushFrame
    pub(crate) fn push_frame(&mut self) {
        if let Some(f) = self.frame {
            self.frames.push(f);
        }
        self.frame = Some(self.ast.scopes.create(self.frame, None));
        self.loop_counts.push(self.loop_count);
        self.loop_count = 0;
    }

    // port of: JexlParser.popFrame
    pub(crate) fn pop_frame(&mut self) {
        self.frame = self.frames.pop();
        if let Some(c) = self.loop_counts.pop() {
            self.loop_count = c;
        }
    }

    // port of: JexlParser.pushUnit
    pub(crate) fn push_unit(&mut self, unit: NodeId) {
        if let Some(b) = self.block {
            self.blocks.push(b);
        }
        self.block = Some(unit);
    }

    // port of: JexlParser.popUnit
    pub(crate) fn pop_unit(&mut self, unit: NodeId) {
        if self.block == Some(unit) {
            self.block = self.blocks.pop();
        }
    }

    /// LexicalUnit.hasSymbol on a node of this tree
    fn unit_has_symbol(&self, unit: NodeId, symbol: i32) -> bool {
        self.ast.node(unit).lexical_scope().map(|l| l.has_symbol(symbol)).unwrap_or(false)
    }

    /// LexicalUnit.declareSymbol on a node of this tree
    fn unit_declare_symbol(&mut self, unit: NodeId, symbol: i32) -> bool {
        match &mut self.ast.n_mut(unit).data {
            NodeData::Lexical(l) => l.get_or_insert_with(Default::default).add_symbol(symbol),
            NodeData::Script(s) => s.declare_symbol(symbol),
            _ => true,
        }
    }

    // port of: JexlParser.isSymbolDeclared — walks the enclosing nodes of a JexlNode.Info
    fn is_symbol_declared(&self, info: &JexlInfo, symbol: i32) -> bool {
        let mut walk = info.node.clone();
        while let Some(h) = walk {
            let node = h.node();
            if node.is_lexical_unit() {
                if let Some(scope) = node.lexical_scope() {
                    if scope.has_symbol(symbol) {
                        return true;
                    }
                }
                if node.is(JJTJEXLLAMBDA) {
                    break;
                }
            }
            walk = node.parent().map(|p| crate::parser::jexl_node::NodeHandle::new(h.ast.clone(), p.id));
        }
        false
    }

    // port of: JexlParser.isVariable
    pub(crate) fn is_variable(&mut self, name: &str) -> bool {
        match self.frame {
            Some(f) => self.ast.scopes.get_symbol(f, name).is_some(),
            None => false,
        }
    }

    // port of: JexlParser.checkVariable
    pub(crate) fn check_variable(&mut self, identifier: NodeId, name: &str) -> Result<String, JexlException> {
        if let Some(frame) = self.frame {
            if let Some(symbol) = self.ast.scopes.get_symbol(frame, name) {
                let mut declared = true;
                if self.ast.scopes.get(frame).is_captured_symbol(symbol) {
                    self.set_identifier_captured(identifier, true);
                } else {
                    declared = self.block.map(|b| self.unit_has_symbol(b, symbol)).unwrap_or(false);
                    if !declared {
                        // the block chain is pushed most-recent-first, like the Java Deque
                        declared = self.blocks.iter().rev().any(|&u| self.unit_has_symbol(u, symbol));
                    }
                    if !declared {
                        if let Some(info) = &self.info {
                            if info.node.is_some() {
                                declared = self.is_symbol_declared(info, symbol);
                            }
                        }
                    }
                }
                self.set_symbol(identifier, symbol, name);
                if !declared {
                    self.set_identifier_shaded(identifier, true);
                    if self.get_features().is_lexical_shade() {
                        let info = self.ast.node(identifier).jexl_info();
                        return Err(JexlException::with_info(info, &format!("{}: variable is not defined", name), None));
                    }
                }
            }
        }
        Ok(name.to_string())
    }

    // port of: JexlParser.allowVariable
    fn allow_variable(&self, image: &str) -> bool {
        let features = self.get_features();
        if !features.supports_local_var() {
            return false;
        }
        !features.is_reserved_name(image)
    }

    // port of: JexlParser.declareSymbol
    fn declare_symbol(&mut self, symbol: i32) -> bool {
        for &lu in self.blocks.iter().rev().collect::<Vec<_>>() {
            if self.unit_has_symbol(lu, symbol) {
                return false;
            }
            if self.ast.node(lu).is(JJTJEXLLAMBDA) {
                break;
            }
        }
        match self.block {
            None => true,
            Some(b) => self.unit_declare_symbol(b, symbol),
        }
    }

    // port of: JexlParser.declareVariable
    pub(crate) fn declare_variable(&mut self, variable: NodeId, token: Tok) -> Result<(), PErr> {
        let name = self.image(token);
        if !self.allow_variable(&name) {
            return Err(self.throw_feature_exception_token(jexl_features::LOCAL_VAR, Some(token)));
        }
        if self.frame.is_none() {
            self.frame = Some(self.ast.scopes.create(None, None));
        }
        let frame = self.frame.expect("frame");
        let symbol = self.ast.scopes.declare_variable(frame, &name);
        self.set_symbol(variable, symbol, &name);
        if self.ast.scopes.get(frame).is_captured_symbol(symbol) {
            self.set_identifier_captured(variable, true);
        }
        // check if we have a redefinition in this lexical unit
        if !self.declare_symbol(symbol) {
            if self.get_features().is_lexical() {
                let info = self.ast.node(variable).jexl_info();
                return Err(PErr::Jexl(JexlException::with_info(
                    info,
                    &format!("{}: variable is already declared", name),
                    None,
                )));
            }
            self.set_identifier_redefined(variable, true);
        }
        Ok(())
    }

    // port of: JexlParser.declarePragma
    pub(crate) fn declare_pragma(&mut self, key: &str, value: Value) -> Result<(), PErr> {
        if !self.get_features().supports_pragma() {
            let tok = self.get_token(0).ok();
            return Err(self.throw_feature_exception_token(jexl_features::PRAGMA, tok));
        }
        if self.pragmas.is_none() {
            self.pragmas = Some(BTreeMap::new());
        }
        // declaring a namespace through a pragma makes it known to the parser
        if let Some(nsname) = key.strip_prefix(PRAGMA_JEXLNS) {
            if !nsname.is_empty() {
                self.namespaces.get_or_insert_with(HashSet::new).insert(nsname.to_string());
            }
        }
        self.pragmas.as_mut().expect("pragmas").insert(JString::from(key), value);
        Ok(())
    }

    // port of: JexlParser.isDeclaredNamespace
    pub(crate) fn is_declared_namespace(&mut self, token: Tok, colon: Tok) -> bool {
        // syntactic hint, the colon must be adjacent to the identifier
        let c = &self.tokens[colon];
        if c.image == [b':' as u16] && c.begin_column - 1 == self.tokens[token].end_column {
            return true;
        }
        let name = self.image(token);
        if !self.is_variable(&name) {
            if let Some(ns) = &self.namespaces {
                if ns.contains(&name) {
                    return true;
                }
            }
            if self.get_features().test_namespace(&name) {
                return true;
            }
        }
        false
    }

    // port of: JexlParser.declareParameter
    pub(crate) fn declare_parameter(&mut self, token: Tok) -> Result<(), PErr> {
        let identifier = self.image(token);
        if !self.allow_variable(&identifier) {
            return Err(self.throw_feature_exception_token(jexl_features::LOCAL_VAR, Some(token)));
        }
        if self.frame.is_none() {
            self.frame = Some(self.ast.scopes.create(None, None));
        }
        let frame = self.frame.expect("frame");
        let symbol = match self.ast.scopes.declare_parameter(frame, &identifier) {
            Ok(s) => s,
            Err(msg) => return Err(PErr::Jexl(JexlException::java("java.lang.IllegalStateException", Some(msg)))),
        };
        // not sure how declaring a parameter could fail...
        let declared = self.block.map(|b| self.unit_declare_symbol(b, symbol)).unwrap_or(true);
        if !declared && self.get_features().is_lexical() {
            let t = &self.tokens[token];
            let xinfo = self.info.as_ref().map(|i| i.at(t.begin_line, t.begin_column));
            return Err(PErr::Jexl(JexlException::with_info(
                xinfo,
                &format!("{}: variable is already declared", identifier),
                None,
            )));
        }
        Ok(())
    }

    // port of: JexlParser.jjtreeCloseNodeScope
    pub(crate) fn jjtree_close_node_scope(&mut self, node: NodeId) -> Result<(), PErr> {
        let kind = self.ast.node(node).kind();
        if kind == JJTAMBIGUOUS {
            return Err(self.throw_ambiguous_exception(node));
        }
        if kind == JJTJEXLSCRIPT || kind == JJTJEXLLAMBDA {
            if kind == JJTJEXLLAMBDA && !self.get_features().supports_lambda() {
                let info = self.ast.node(node).jexl_info();
                return Err(PErr::Jexl(self.throw_feature_exception(jexl_features::LAMBDA, info)));
            }
            // update the script's scope after the parameters are known
            let current = self.ast.node(node).script().and_then(|s| s.scope);
            if current != self.frame {
                let frame = self.frame;
                self.set_scope(node, frame);
            }
            self.pop_frame();
        } else if matches!(
            kind,
            JJTASSIGNMENT | JJTSETADDNODE | JJTSETMULTNODE | JJTSETDIVNODE | JJTSETANDNODE | JJTSETORNODE | JJTSETXORNODE | JJTSETSUBNODE
        ) {
            // NOTE: like the Java ASSIGN_NODES set, ASTSetModNode is absent
            let lv = self.ast.node(node).child(0);
            if !lv.is_left_value() {
                return Err(self.throw_parsing_exception(true, None));
            }
        }
        self.feature_controller.control_node(&self.ast, node).map_err(PErr::Jexl)
    }

    // port of: JexlParser.throwAmbiguousException
    fn throw_ambiguous_exception(&mut self, node: NodeId) -> PErr {
        let begin = self.ast.node(node).jexl_info();
        let t = match self.get_token(0) {
            Ok(t) => t,
            Err(e) => return PErr::Lex(e),
        };
        let (bl, ec) = (self.tokens[t].begin_line, self.tokens[t].end_column);
        let end = self.info.as_ref().map(|i| i.at(bl, ec));
        let msg = self.read_source_line(end.as_ref().map(|e| e.get_line()).unwrap_or(0));
        PErr::Jexl(JexlException::ambiguous(begin, end, &msg))
    }

    // port of: JexlParser.throwFeatureException(int, JexlInfo)
    pub(crate) fn throw_feature_exception(&self, feature: i32, info: Option<JexlInfo>) -> JexlException {
        let msg = info.as_ref().map(|i| self.read_source_line(i.get_line()));
        JexlException::feature(info, feature, msg.as_ref())
    }

    // port of: JexlParser.throwFeatureException(int, Token)
    pub(crate) fn throw_feature_exception_token(&mut self, feature: i32, token: Option<Tok>) -> PErr {
        let token = match token {
            Some(t) => Some(t),
            None => match self.get_token(0) {
                Ok(t) => Some(t),
                Err(e) => return PErr::Lex(e),
            },
        };
        match token {
            None => PErr::Jexl(JexlException::parsing(None, &JString::from(JexlFeatures::stringify(feature)))),
            Some(t) => {
                let tk = &self.tokens[t];
                let xinfo = self.info.as_ref().map(|i| i.at(tk.begin_line, tk.begin_column));
                PErr::Jexl(self.throw_feature_exception(feature, xinfo))
            }
        }
    }

    // port of: JexlParser.throwParsingException(Class, Token)
    pub(crate) fn throw_parsing_exception(&mut self, assignment: bool, tok: Option<Tok>) -> PErr {
        let tok = match tok {
            Some(t) => Some(t),
            None => self.get_token(0).ok(),
        };
        let (xinfo, msg) = match tok {
            None => (None, JString::from("unrecoverable state")),
            Some(t) => {
                let tk = &self.tokens[t];
                let info = self.info.as_ref().map(|i| i.at(tk.begin_line, tk.begin_column));
                (info, JString::from_units(&tk.image))
            }
        };
        PErr::Jexl(if assignment {
            JexlException::assignment(xinfo, &msg)
        } else {
            JexlException::parsing(xinfo, &msg)
        })
    }

    // ------------------------------------------------------------------ node mutators

    // port of: ASTJexlScript.setScope
    pub(crate) fn set_scope(&mut self, node: NodeId, scope: Option<ScopeId>) {
        let argc = scope.map(|s| self.ast.scopes.get(s).get_arg_count()).unwrap_or(0);
        if let NodeData::Script(s) = &mut self.ast.n_mut(node).data {
            s.scope = scope;
            for a in 0..argc {
                s.declare_symbol(a);
            }
        }
    }

    fn with_identifier(&mut self, node: NodeId, f: impl FnOnce(&mut crate::parser::ast_identifier::ASTIdentifier)) {
        if let NodeData::Identifier(i) = &mut self.ast.n_mut(node).data {
            f(i);
        }
    }

    // port of: ASTIdentifier.setSymbol(String)
    pub(crate) fn set_symbol_name(&mut self, node: NodeId, name: &str) {
        self.with_identifier(node, |i| i.set_symbol_name(name));
    }

    // port of: ASTIdentifier.setSymbol(int, String)
    pub(crate) fn set_symbol(&mut self, node: NodeId, symbol: i32, name: &str) {
        self.with_identifier(node, |i| i.set_symbol(symbol, name));
    }

    pub(crate) fn set_identifier_captured(&mut self, node: NodeId, f: bool) {
        self.with_identifier(node, |i| i.set_captured(f));
    }
    pub(crate) fn set_identifier_shaded(&mut self, node: NodeId, f: bool) {
        self.with_identifier(node, |i| i.set_shaded(f));
    }
    pub(crate) fn set_identifier_redefined(&mut self, node: NodeId, f: bool) {
        self.with_identifier(node, |i| i.set_redefined(f));
    }

    // port of: ASTNamespaceIdentifier.setNamespace
    pub(crate) fn set_namespace(&mut self, node: NodeId, ns: Tok, id: Tok) {
        let (ns, id) = (self.image(ns), self.image(id));
        self.with_identifier(node, |i| i.set_namespace(&ns, &id));
    }

    // port of: ASTAnnotation.setName
    pub(crate) fn set_annotation_name(&mut self, node: NodeId, token: Tok) {
        let image = self.image(token);
        let name = image.strip_prefix('@').unwrap_or(&image).to_string();
        if let NodeData::Annotation(a) = &mut self.ast.n_mut(node).data {
            *a = name;
        }
    }

    // port of: ASTIdentifierAccess.setIdentifier
    pub(crate) fn set_identifier(&mut self, node: NodeId, token: Tok, build: bool) {
        let name = if build {
            JString::new(string_parser::build_string(&self.tokens[token].image, true))
        } else {
            JString::from_units(&self.tokens[token].image)
        };
        if let NodeData::IdentifierAccess(i) = &mut self.ast.n_mut(node).data {
            i.set_identifier(name);
        }
    }

    // port of: ASTStringLiteral.setLiteral / ASTJxltLiteral.setLiteral
    pub(crate) fn set_string_literal(&mut self, node: NodeId, token: Tok) {
        let lit = string_parser::build_string(&self.tokens[token].image, true);
        if let NodeData::Literal(l) = &mut self.ast.n_mut(node).data {
            *l = JString::new(lit);
        }
    }

    // port of: ASTRegexLiteral.setLiteral (Pattern.compile)
    pub(crate) fn set_regex_literal(&mut self, node: NodeId, token: Tok) -> Result<(), PErr> {
        let lit = string_parser::build_regex(&self.tokens[token].image);
        let src = String::from_utf16_lossy(&lit);
        match crate::java::regex::Pattern::compile(&src) {
            Ok(p) => {
                self.ast.n_mut(node).data = NodeData::Regex(Arc::new(p));
                Ok(())
            }
            Err(e) => Err(PErr::Jexl(JexlException::java(
                "java.util.regex.PatternSyntaxException",
                Some(e.get_message()),
            ))),
        }
    }

    // port of: ASTNumberLiteral.setNatural
    pub(crate) fn set_natural(&mut self, node: NodeId, s: &str) -> Result<(), PErr> {
        if let NodeData::NumberLiteral(n) = &mut self.ast.n_mut(node).data {
            n.set_natural(s).map_err(PErr::Jexl)?;
        }
        Ok(())
    }

    // port of: ASTNumberLiteral.setReal
    pub(crate) fn set_real(&mut self, node: NodeId, s: &str) -> Result<(), PErr> {
        if let NodeData::NumberLiteral(n) = &mut self.ast.n_mut(node).data {
            n.set_real(s).map_err(PErr::Jexl)?;
        }
        Ok(())
    }
}
