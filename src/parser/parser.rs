// port of: org.apache.commons.jexl3.parser.Parser (the hand-written parse() entry point and the
// javacc token plumbing: jj_consume_token, jj_scan_token, getToken). The productions and lookahead
// routines are in parser_gen.rs (mechanically translated); JexlParser's helpers are in jexl_parser.rs.
use std::collections::BTreeMap;
use std::collections::HashSet;

use crate::internal::scope::ScopeId;
use crate::java::string::JString;
use crate::jexl_exception::JexlException;
use crate::jexl_features::JexlFeatures;
use crate::jexl_info::JexlInfo;
use crate::parser::feature_controller::FeatureController;
use crate::parser::jexl_node::{Ast, NodeData, NodeId, NodeValue, Parsed};
use crate::parser::jjt_parser_state::JJTParserState;
use crate::parser::parse_exception::ParseException;
use crate::parser::parser_constants::{DEFAULT, REGISTERS};
use crate::parser::parser_token_manager::ParserTokenManager;
use crate::parser::simple_char_stream::SimpleCharStream;
use crate::parser::token::Token;
use crate::parser::token_mgr_exception::TokenMgrException;
use crate::value::Value;

/// A token: an index into the parser's token arena (Token objects linked by `next`).
pub(crate) type Tok = usize;

/// What a production can throw.
#[derive(Debug)]
pub(crate) enum PErr {
    Parse(ParseException),
    Lex(TokenMgrException),
    Jexl(JexlException),
}

impl From<TokenMgrException> for PErr {
    fn from(e: TokenMgrException) -> Self {
        PErr::Lex(e)
    }
}
impl From<JexlException> for PErr {
    fn from(e: JexlException) -> Self {
        PErr::Jexl(e)
    }
}

/// What a lookahead routine can throw: the LookaheadSuccess signal or a lexical error.
#[derive(Debug)]
pub(crate) enum ScanErr {
    Success,
    Lex(TokenMgrException),
}

impl From<TokenMgrException> for ScanErr {
    fn from(e: TokenMgrException) -> Self {
        ScanErr::Lex(e)
    }
}

pub struct Parser {
    // javacc state
    pub(crate) token_source: ParserTokenManager,
    pub(crate) tokens: Vec<Token>,
    pub(crate) token: Tok,
    pub(crate) jj_nt: Tok,
    pub(crate) jj_scanpos: Tok,
    pub(crate) jj_lastpos: Tok,
    pub(crate) jj_la: i32,
    pub(crate) jj_looking_ahead: bool,
    pub(crate) jj_sem_la: bool,
    pub(crate) jjtree: JJTParserState,
    pub(crate) ast: Ast,
    // JexlParser state
    pub(crate) feature_controller: FeatureController,
    pub(crate) info: Option<JexlInfo>,
    pub(crate) source: Option<String>,
    pub(crate) frame: Option<ScopeId>,
    pub(crate) frames: Vec<ScopeId>,
    pub(crate) pragmas: Option<BTreeMap<JString, Value>>,
    pub(crate) namespaces: Option<HashSet<String>>,
    pub(crate) loop_count: i32,
    pub(crate) loop_counts: Vec<i32>,
    pub(crate) block: Option<NodeId>,
    pub(crate) blocks: Vec<NodeId>,
}

// port of: JexlParser.stringify
pub(crate) fn stringify(lstr: &[String]) -> String {
    lstr.join(".")
}

impl Default for Parser {
    fn default() -> Self {
        Parser::new()
    }
}

impl Parser {
    // port of: Parser(Provider) with the StringProvider(";") the Engine uses
    pub fn new() -> Parser {
        Parser {
            token_source: ParserTokenManager::new(SimpleCharStream::new(&[])),
            tokens: vec![Token::default()],
            token: 0,
            jj_nt: 0,
            jj_scanpos: 0,
            jj_lastpos: 0,
            jj_la: 0,
            jj_looking_ahead: false,
            jj_sem_la: false,
            jjtree: JJTParserState::new(),
            ast: Ast::new(),
            feature_controller: FeatureController::new(JexlFeatures::new()),
            info: None,
            source: None,
            frame: None,
            frames: Vec::new(),
            pragmas: None,
            namespaces: None,
            loop_count: 0,
            loop_counts: Vec::new(),
            block: None,
            blocks: Vec::new(),
        }
    }

    // port of: Parser.parse(JexlInfo, JexlFeatures, String, Scope)
    // `scope_params` are the names of the top-level Scope (Engine's `new Scope(null, names)`).
    pub fn parse(
        &mut self,
        jexl_info: Option<JexlInfo>,
        jexl_features: &JexlFeatures,
        jexl_src: &str,
        scope_params: Option<&[String]>,
    ) -> Result<Parsed, JexlException> {
        self.ast = Ast::new();
        let scope = scope_params.map(|p| self.ast.scopes.create(None, Some(p)));
        self.parse_prepared(jexl_info, jexl_features, jexl_src, scope)
    }

    /// port of: Parser.parse(JexlInfo, JexlFeatures, String, Scope) with a caller-supplied Scope.
    /// Java hands `TemplateEngine.parseExpression` the *same* Scope object the template script was
    /// parsed with, so a `${x}` sees the template's symbols. Scopes live in the tree's arena here,
    /// so the caller's whole arena is copied in first: every ScopeId (and every symbol number)
    /// then means the same thing in both trees, and the sub-parse only appends to it.
    pub fn parse_in_scope(
        &mut self,
        jexl_info: Option<JexlInfo>,
        jexl_features: &JexlFeatures,
        jexl_src: &str,
        scopes: &crate::internal::scope::Scopes,
        scope: Option<ScopeId>,
    ) -> Result<Parsed, JexlException> {
        self.ast = Ast::new();
        self.ast.scopes = scopes.clone();
        self.parse_prepared(jexl_info, jexl_features, jexl_src, scope)
    }

    fn parse_prepared(
        &mut self,
        jexl_info: Option<JexlInfo>,
        jexl_features: &JexlFeatures,
        jexl_src: &str,
        scope: Option<ScopeId>,
    ) -> Result<Parsed, JexlException> {
        let previous = self.get_features().clone();
        self.set_features(jexl_features.clone());
        // If registers are allowed, the default parser state has to be REGISTERS.
        if jexl_features.supports_register() {
            self.token_source.default_lex_state = REGISTERS;
        }
        // lets do the 'Unique Init' in here to be safe - it's a pain to remember
        self.info = Some(jexl_info.unwrap_or_else(JexlInfo::from_caller));
        self.source = Some(jexl_src.to_string());
        self.pragmas = None;
        self.frame = scope;
        let result = self.re_init(jexl_src).and_then(|_| {
            if jexl_features.supports_script() {
                self.jexl_script(scope)
            } else {
                self.jexl_expression(scope)
            }
        });
        let info = self.info.clone();
        let out = match result {
            Ok(root) => {
                let script = root;
                self.ast.node(script).set_value(Some(NodeValue::Info(info.clone().expect("info").detach())));
                let pragmas = self.pragmas.take();
                if let NodeData::Script(s) = &mut self.ast.n_mut(script).data {
                    s.features = Some(jexl_features.clone());
                    s.pragmas = Some(pragmas.unwrap_or_default());
                }
                Ok(Parsed { ast: std::sync::Arc::new(std::mem::replace(&mut self.ast, Ast::new())), root: script })
            }
            Err(PErr::Lex(xtme)) => {
                let after = JString::from_units(xtme.get_after());
                Err(JexlException::tokenization(info, xtme.get_line(), xtme.get_column(), after))
            }
            Err(PErr::Parse(_)) => {
                let lastpos = Some(self.jj_lastpos);
                let scanpos = Some(self.jj_scanpos);
                let next = self.tokens[self.token].next;
                let errortok = self.error_token(&[lastpos, scanpos, next, Some(self.token)]);
                let (l, c, image) = match errortok {
                    Some(t) => (
                        self.tokens[t].begin_line,
                        self.tokens[t].begin_column,
                        JString::from_units(&self.tokens[t].image),
                    ),
                    None => (0, 0, JString::empty()),
                };
                Err(JexlException::parsing(info.map(|i| i.at(l, c)), &image))
            }
            Err(PErr::Jexl(x)) => Err(x),
        };
        self.token_source.default_lex_state = DEFAULT;
        self.cleanup(previous);
        self.jjtree.reset();
        out
    }

    // port of: Parser.ReInit(String)
    fn re_init(&mut self, src: &str) -> Result<(), PErr> {
        let units: Vec<u16> = src.encode_utf16().collect();
        self.token_source.re_init(SimpleCharStream::new(&units));
        // In Java the previous parse's Token objects stay reachable through jj_scanpos, which
        // getToken() reads while jj_lookingAhead is set -- and a lexical error raised inside a
        // semantic lookahead leaves that flag set across parses. Keep exactly that chain alive.
        let keep: Vec<Token> = if self.jj_looking_ahead {
            let mut chain = Vec::new();
            let mut cur = Some(self.jj_scanpos);
            while let Some(i) = cur {
                let t = self.tokens[i].clone();
                cur = t.next;
                chain.push(t);
            }
            let last = chain.len();
            for (n, t) in chain.iter_mut().enumerate() {
                t.next = if n + 1 < last { Some(n + 1) } else { None };
            }
            chain
        } else {
            Vec::new()
        };
        self.tokens = keep;
        self.jj_scanpos = 0;
        self.jj_lastpos = 0;
        let dummy = self.push_token(Token::default());
        self.token = dummy;
        let first = self.token_source.get_next_token()?;
        let first_idx = self.push_token(first);
        self.tokens[dummy].next = Some(first_idx);
        self.jj_nt = first_idx;
        Ok(())
    }

    fn push_token(&mut self, t: Token) -> Tok {
        self.tokens.push(t);
        self.tokens.len() - 1
    }

    /// the image of a token as a Rust string (identifiers, keywords and numbers are ASCII)
    pub(crate) fn image(&self, t: Tok) -> String {
        String::from_utf16_lossy(&self.tokens[t].image)
    }

    // port of: Parser.jj_consume_token
    pub(crate) fn jj_consume_token(&mut self, kind: i32) -> Result<Tok, PErr> {
        let old_token = self.token;
        self.token = self.jj_nt;
        match self.tokens[self.token].next {
            Some(n) => self.jj_nt = n,
            None => {
                let t = self.token_source.get_next_token()?;
                let idx = self.push_token(t);
                self.tokens[self.jj_nt].next = Some(idx);
                self.jj_nt = idx;
            }
        }
        if self.tokens[self.token].kind == kind {
            return Ok(self.token);
        }
        self.jj_nt = self.token;
        self.token = old_token;
        Err(PErr::Parse(ParseException::new()))
    }

    // port of: Parser.jj_scan_token
    pub(crate) fn jj_scan_token(&mut self, kind: i32) -> Result<bool, ScanErr> {
        if self.jj_scanpos == self.jj_lastpos {
            self.jj_la -= 1;
            match self.tokens[self.jj_scanpos].next {
                None => {
                    let t = self.token_source.get_next_token()?;
                    let idx = self.push_token(t);
                    self.tokens[self.jj_scanpos].next = Some(idx);
                    self.jj_scanpos = idx;
                    self.jj_lastpos = idx;
                }
                Some(n) => {
                    self.jj_scanpos = n;
                    self.jj_lastpos = n;
                }
            }
        } else {
            // behind jj_lastpos the chain is already lexed
            self.jj_scanpos = self.tokens[self.jj_scanpos].next.unwrap_or(self.jj_scanpos);
        }
        if self.tokens[self.jj_scanpos].kind != kind {
            return Ok(true);
        }
        if self.jj_la == 0 && self.jj_scanpos == self.jj_lastpos {
            return Err(ScanErr::Success);
        }
        Ok(false)
    }

    // port of: Parser.getToken
    pub(crate) fn get_token(&mut self, index: i32) -> Result<Tok, TokenMgrException> {
        let mut t = if self.jj_looking_ahead { self.jj_scanpos } else { self.token };
        for _ in 0..index {
            t = match self.tokens[t].next {
                Some(n) => n,
                None => {
                    let nt = self.token_source.get_next_token()?;
                    let idx = self.push_token(nt);
                    self.tokens[t].next = Some(idx);
                    idx
                }
            };
        }
        Ok(t)
    }

    // port of: JexlParser.errorToken
    fn error_token(&self, tokens: &[Option<Tok>]) -> Option<Tok> {
        tokens.iter().flatten().copied().find(|&t| !self.tokens[t].image.is_empty())
    }

    // port of: `new ASTxxx(JJTXXX)`
    pub(crate) fn new_node(&mut self, id: i32) -> NodeId {
        self.ast.add(id)
    }

    // port of: JexlNode.jjtSetFirstToken
    pub(crate) fn set_first_token(&mut self, n: NodeId, t: Tok) {
        let tk = &self.tokens[t];
        self.ast.n_mut(n).lc = (tk.begin_line << 0xc) | (0xfff & tk.begin_column);
    }

    // port of: JJTParserState.closeNodeScope(Node, int)
    pub(crate) fn close_node_scope_num(&mut self, n: NodeId, num: i32) {
        let children = self.jjtree.close_node_scope_num(num);
        self.attach(n, children);
    }

    // port of: JJTParserState.closeNodeScope(Node, boolean)
    pub(crate) fn close_node_scope_cond(&mut self, n: NodeId, condition: bool) {
        if let Some(children) = self.jjtree.close_node_scope_cond(condition) {
            self.attach(n, children);
        }
    }

    fn attach(&mut self, n: NodeId, children: Vec<NodeId>) {
        for &c in &children {
            self.ast.n_mut(c).parent = Some(n);
        }
        self.ast.n_mut(n).children = children;
        self.jjt_close(n);
        self.jjtree.push_node(n);
    }

    // port of: ASTArrayLiteral / ASTMapLiteral / ASTSetLiteral .jjtClose
    fn jjt_close(&mut self, n: NodeId) {
        use crate::parser::parser_tree_constants::*;
        let kind = self.ast.n(n).id;
        if kind != JJTARRAYLITERAL && kind != JJTMAPLITERAL && kind != JJTSETLITERAL {
            return;
        }
        let node = self.ast.node(n);
        let mut constant = true;
        for c in 0..node.num_children() {
            if !constant {
                break;
            }
            let child = node.child(c);
            if kind == JJTARRAYLITERAL && child.is(JJTREFERENCE) {
                constant = child.is_constant_literal(true);
            } else if kind == JJTMAPLITERAL && child.is(JJTMAPENTRY) {
                constant = child.is_constant_literal(true);
            } else if !child.is_constant() {
                constant = false;
            }
        }
        self.ast.n_mut(n).data = NodeData::Constant(constant);
    }

    // port of: ASTJexlScript.script
    pub(crate) fn script_of(&mut self, n: NodeId) -> NodeId {
        use crate::parser::parser_tree_constants::JJTJEXLLAMBDA;
        let scope_none = self.ast.node(n).script().map(|s| s.scope.is_none()).unwrap_or(false);
        let node = self.ast.node(n);
        if scope_none && node.num_children() == 1 && node.child(0).is(JJTJEXLLAMBDA) {
            let lambda = node.child(0).id;
            self.ast.n_mut(lambda).parent = None;
            return lambda;
        }
        n
    }
}
