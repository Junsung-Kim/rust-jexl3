// port of: org.apache.commons.jexl3.internal.Debugger
//
// Rebuilds an expression string from the tree, plus the start/end offsets of a "cause" node in
// that string. Java dispatches through ParserVisitor/jjtAccept; here the arena gives every node a
// ParserTreeConstants kind, so `visit` is one match whose arms follow Debugger.java in order.
//
// Two deliberate simplifications:
//  * the visitor pattern's `data` argument is dropped: no visit method ever reads it, it is only
//    threaded through and returned (ASTAnnotation returns null, which nothing observes).
//  * the `check(node, image, data)` null-image branch (`builder.append(node.toString())`) is
//    dropped: every call site in Debugger passes a non-null image.
use crate::java::string::JString;
use crate::jexl_info::Detail;
use crate::parser::jexl_node::{NodeHandle, NodeId, NodeRef};
use crate::parser::parser_tree_constants::*;
use crate::parser::string_parser;

pub struct Debugger {
    /// The builder to compose messages (UTF-16, like Java's StringBuilder).
    builder: Vec<u16>,
    /// The cause of the issue to debug.
    cause: Option<NodeId>,
    /// The starting character location offset of the cause in the builder.
    start: i32,
    /// The ending character location offset of the cause in the builder.
    end: i32,
    /// The indentation level.
    indent_level: i32,
    /// Perform indentation?.
    indent: i32,
    /// accept() relative depth.
    depth: i32,
}

impl Default for Debugger {
    fn default() -> Self {
        Debugger::new()
    }
}

impl Debugger {
    // port of: Debugger()
    pub fn new() -> Debugger {
        Debugger { builder: Vec::new(), cause: None, start: 0, end: 0, indent_level: 0, indent: 2, depth: i32::MAX }
    }

    // port of: Debugger.reset
    pub fn reset(&mut self) {
        self.builder.clear();
        self.cause = None;
        self.start = 0;
        self.end = 0;
        self.indent_level = 0;
        self.indent = 2;
        self.depth = i32::MAX;
    }

    // port of: Debugger.debug(JexlNode)
    pub fn debug(&mut self, node: NodeRef<'_>) -> bool {
        self.debug_r(node, true)
    }

    // port of: Debugger.debug(JexlNode, boolean)
    pub fn debug_r(&mut self, node: NodeRef<'_>, r: bool) -> bool {
        self.start = 0;
        self.end = 0;
        self.indent_level = 0;
        self.builder.clear();
        self.cause = Some(node.id);
        // make arg cause become the root cause
        let mut walk = node;
        if r {
            while let Some(parent) = walk.parent() {
                walk = parent;
            }
        }
        self.accept(walk);
        self.end > 0
    }

    // port of: Debugger.toString
    pub fn to_jstring(&self) -> JString {
        JString::from_units(&self.builder)
    }

    // port of: Debugger.data(JexlNode)
    pub fn data(&mut self, node: NodeRef<'_>) -> JString {
        self.start = 0;
        self.end = 0;
        self.indent_level = 0;
        self.builder.clear();
        self.cause = Some(node.id);
        self.accept(node);
        self.to_jstring()
    }

    /// port of: Script.getParsedText(int) — set the indentation, render, return the text.
    pub fn data_indent(&mut self, node: NodeRef<'_>, indent: i32) -> JString {
        self.set_indentation(indent);
        self.debug_r(node, false);
        self.to_jstring()
    }

    // port of: Debugger.start
    pub fn start(&self) -> i32 {
        self.start
    }

    // port of: Debugger.end
    pub fn end(&self) -> i32 {
        self.end
    }

    // port of: Debugger.setIndentation
    pub fn set_indentation(&mut self, level: i32) {
        self.indentation(level);
    }

    // port of: Debugger.indentation
    pub fn indentation(&mut self, level: i32) -> &mut Self {
        self.indent = level.max(0);
        self.indent_level = 0;
        self
    }

    // port of: Debugger.depth
    pub fn depth(&mut self, rdepth: i32) -> &mut Self {
        self.depth = rdepth;
        self
    }

    /// port of: JexlException.detailedInfo — the JexlInfo.Detail a Debugger run yields for a node,
    /// or None when the cause could not be located.
    pub fn detail_of(node: &NodeHandle) -> Option<Detail> {
        let mut dbg = Debugger::new();
        if dbg.debug(node.node()) {
            Some(Detail { start: dbg.start, end: dbg.end, text: dbg.to_jstring() })
        } else {
            None
        }
    }

    // ------------------------------------------------------------------------------ the builder

    fn len(&self) -> i32 {
        self.builder.len() as i32
    }

    fn ch(&mut self, c: char) {
        self.builder.push(c as u16);
    }

    fn s(&mut self, s: &str) {
        self.builder.extend(s.encode_utf16());
    }

    fn u(&mut self, u: &[u16]) {
        self.builder.extend_from_slice(u);
    }

    fn is_cause(&self, node: NodeRef<'_>) -> bool {
        self.cause == Some(node.id)
    }

    // ------------------------------------------------------------------------ the visitor plumbing

    // port of: Debugger.accept
    fn accept(&mut self, node: NodeRef<'_>) {
        if self.depth <= 0 {
            self.s("...");
            return;
        }
        if self.is_cause(node) {
            self.start = self.len();
        }
        self.depth -= 1;
        self.visit(node);
        self.depth += 1;
        if self.is_cause(node) {
            self.end = self.len();
        }
    }

    // port of: Debugger.acceptStatement
    fn accept_statement(&mut self, child: NodeRef<'_>) {
        let parent = child.parent();
        if self.indent > 0 && parent.map(|p| p.is(JJTBLOCK) || p.is_script()).unwrap_or(false) {
            for _ in 0..self.indent_level {
                for _ in 0..self.indent {
                    self.ch(' ');
                }
            }
        }
        self.depth -= 1;
        self.accept(child);
        self.depth += 1;
        // blocks, if, for & while don't need a ';' at end
        if !(child.is_script()
            || child.is(JJTBLOCK)
            || child.is(JJTIFSTATEMENT)
            || child.is(JJTFOREACHSTATEMENT)
            || child.is(JJTWHILESTATEMENT)
            || child.is(JJTDOWHILESTATEMENT)
            || child.is(JJTANNOTATION))
        {
            self.ch(';');
            if self.indent > 0 {
                self.ch('\n');
            } else {
                self.ch(' ');
            }
        }
    }

    // port of: Debugger.check (the image is never null at any Debugger call site)
    fn check(&mut self, node: NodeRef<'_>, image: &[u16]) {
        if self.is_cause(node) {
            self.start = self.len();
        }
        self.u(image);
        if self.is_cause(node) {
            self.end = self.len();
        }
    }

    fn check_str(&mut self, node: NodeRef<'_>, image: &str) {
        let units: Vec<u16> = image.encode_utf16().collect();
        self.check(node, &units);
    }

    // port of: Debugger.infixChildren
    // `paren` is dead in the 3.2.1 grammar: a parenthesised sub-expression is always wrapped in an
    // ASTReferenceExpression, so an ASTOrNode/ASTBitwiseOrNode/ASTBitwiseXorNode is never a direct
    // child of ASTAndNode/ASTBitwiseAndNode. Ported as-is.
    fn infix_children(&mut self, node: NodeRef<'_>, infix: &str, paren: bool) {
        let num = node.num_children();
        if paren {
            self.ch('(');
        }
        for i in 0..num {
            if i > 0 {
                self.s(infix);
            }
            self.accept(node.child(i));
        }
        if paren {
            self.ch(')');
        }
    }

    // port of: Debugger.prefixChild
    fn prefix_child(&mut self, node: NodeRef<'_>, prefix: &str) {
        let paren = node.child(0).num_children() > 1;
        self.s(prefix);
        if paren {
            self.ch('(');
        }
        self.accept(node.child(0));
        if paren {
            self.ch(')');
        }
    }

    // port of: Debugger.additiveNode
    // As in infix_children, `paren` is dead: `(a + b) * c` parses as Mul(ReferenceExpression(Add), c).
    fn additive_node(&mut self, node: NodeRef<'_>, op: &str) {
        // need parenthesis if not in operator precedence order
        let paren = node
            .parent()
            .map(|p| p.is(JJTMULNODE) || p.is(JJTDIVNODE) || p.is(JJTMODNODE))
            .unwrap_or(false);
        let num = node.num_children();
        if paren {
            self.ch('(');
        }
        self.accept(node.child(0));
        for i in 1..num {
            self.s(op);
            self.accept(node.child(i));
        }
        if paren {
            self.ch(')');
        }
    }

    /// port of: Debugger.QUOTED_IDENTIFIER — `[\s]|[\p{Punct}&&[^@#\$_]]`, i.e. the ASCII spaces
    /// and the ASCII punctuation but for `@`, `#`, `$` and `_`. (The code units are pinned against
    /// the JVM's own Matcher in tests/debugger_oracle.rs.)
    fn quoted_identifier_char(c: u16) -> bool {
        matches!(c, 9..=13 | 32..=34 | 37..=47 | 58..=63 | 91..=94 | 96 | 123..=126)
    }

    // port of: Debugger.needQuotes
    pub fn need_quotes(str: &JString) -> bool {
        str.units().iter().any(|&c| Self::quoted_identifier_char(c)) || str.eq_str("size") || str.eq_str("empty")
    }

    // ------------------------------------------------------------------------------ the visits
    //
    // The arms follow the order of the visit(ASTxxx, Object) methods in Debugger.java.

    fn visit(&mut self, node: NodeRef<'_>) {
        match node.kind() {
            JJTADDNODE => self.additive_node(node, " + "),
            JJTSUBNODE => self.additive_node(node, " - "),
            JJTANDNODE => self.infix_children(node, " && ", false),
            // visit(ASTArrayAccess)
            JJTARRAYACCESS => {
                for i in 0..node.num_children() {
                    self.ch('[');
                    self.accept(node.child(i));
                    self.ch(']');
                }
            }
            JJTEXTENDEDLITERAL => self.s("..."),
            // visit(ASTArrayLiteral)
            JJTARRAYLITERAL => {
                let num = node.num_children();
                self.s("[ ");
                if num > 0 {
                    self.accept(node.child(0));
                    for i in 1..num {
                        self.s(", ");
                        self.accept(node.child(i));
                    }
                }
                self.s(" ]");
            }
            JJTRANGENODE => self.infix_children(node, " .. ", false),
            JJTASSIGNMENT => self.infix_children(node, " = ", false),
            JJTBITWISEANDNODE => self.infix_children(node, " & ", false),
            JJTBITWISECOMPLNODE => self.prefix_child(node, "~"),
            JJTBITWISEORNODE => {
                let paren = node.parent().map(|p| p.is(JJTBITWISEANDNODE)).unwrap_or(false);
                self.infix_children(node, " | ", paren);
            }
            JJTBITWISEXORNODE => {
                let paren = node.parent().map(|p| p.is(JJTBITWISEANDNODE)).unwrap_or(false);
                self.infix_children(node, " ^ ", paren);
            }
            // visit(ASTBlock)
            JJTBLOCK => {
                self.ch('{');
                if self.indent > 0 {
                    self.indent_level += 1;
                    self.ch('\n');
                } else {
                    self.ch(' ');
                }
                for i in 0..node.num_children() {
                    self.accept_statement(node.child(i));
                }
                if self.indent > 0 {
                    self.indent_level -= 1;
                    for _ in 0..self.indent_level {
                        for _ in 0..self.indent {
                            self.ch(' ');
                        }
                    }
                }
                self.ch('}');
            }
            JJTDIVNODE => self.infix_children(node, " / ", false),
            // visit(ASTEmptyFunction)
            JJTEMPTYFUNCTION => {
                self.s("empty ");
                self.accept(node.child(0));
            }
            JJTEQNODE => self.infix_children(node, " == ", false),
            JJTERNODE => self.infix_children(node, " =~ ", false),
            JJTSWNODE => self.infix_children(node, " =^ ", false),
            JJTEWNODE => self.infix_children(node, " =$ ", false),
            JJTNSWNODE => self.infix_children(node, " !^ ", false),
            JJTNEWNODE => self.infix_children(node, " !$ ", false),
            JJTFALSENODE => self.check_str(node, "false"),
            JJTCONTINUE => self.check_str(node, "continue"),
            JJTBREAK => self.check_str(node, "break"),
            // visit(ASTForeachStatement)
            JJTFOREACHSTATEMENT => {
                self.s("for(");
                self.accept(node.child(0));
                self.s(" : ");
                self.accept(node.child(1));
                self.s(") ");
                if node.num_children() > 2 {
                    self.accept_statement(node.child(2));
                } else {
                    self.ch(';');
                }
            }
            JJTGENODE => self.infix_children(node, " >= ", false),
            JJTGTNODE => self.infix_children(node, " > ", false),
            // visit(ASTIdentifier) — ASTNamespaceIdentifier inherits its jjtAccept
            JJTIDENTIFIER | JJTNAMESPACEIDENTIFIER => {
                let id = node.identifier().expect("identifier data");
                let image = string_parser::escape_identifier(id.get_name());
                match id.get_namespace() {
                    None => self.check_str(node, &image),
                    Some(ns) => {
                        let nsid = format!("{}:{}", string_parser::escape_identifier(ns), image);
                        self.check_str(node, &nsid);
                    }
                }
            }
            // visit(ASTIdentifierAccess) — the Safe / Jxlt variants inherit its jjtAccept
            JJTIDENTIFIERACCESS | JJTIDENTIFIERACCESSSAFE | JJTIDENTIFIERACCESSJXLT | JJTIDENTIFIERACCESSSAFEJXLT => {
                let dot = if node.is_safe() { "?." } else { "." };
                self.s(dot);
                let image = node.identifier_access().expect("access data").get_name();
                if node.is_expression() {
                    self.ch('`');
                    let escaped = replace(image.units(), '`', "\\`");
                    self.u(&escaped);
                    self.ch('`');
                } else if Self::need_quotes(image) {
                    // quote it
                    self.ch('\'');
                    let escaped = replace(image.units(), '\'', "\\'");
                    self.u(&escaped);
                    self.ch('\'');
                } else {
                    self.u(image.units());
                }
            }
            // visit(ASTIfStatement)
            JJTIFSTATEMENT => {
                let num_children = node.num_children() as isize;
                // if (...) ...
                self.s("if (");
                self.accept(node.child(0));
                self.s(") ");
                self.accept_statement(node.child(1));
                //.. else if (...) ...
                let mut c = 2isize;
                while c < num_children - 1 {
                    self.s(" else if (");
                    self.accept(node.child(c as usize));
                    self.s(") ");
                    self.accept_statement(node.child(c as usize + 1));
                    c += 2;
                }
                // else... (if odd)
                if (num_children & 1) == 1 {
                    self.s(" else ");
                    self.accept_statement(node.child(num_children as usize - 1));
                }
            }
            // visit(ASTNumberLiteral)
            JJTNUMBERLITERAL => {
                let image = node.number().expect("number data").to_java_string();
                self.check_str(node, &image);
            }
            // visit(ASTJexlScript) — ASTJexlLambda inherits its jjtAccept; visitParameter is the
            // identity
            JJTJEXLSCRIPT | JJTJEXLLAMBDA => {
                let lambda = node.is(JJTJEXLLAMBDA);
                // if lambda, produce parameters
                if lambda {
                    // use lambda syntax if not assigned
                    let named = node.parent().map(|p| p.is(JJTASSIGNMENT)).unwrap_or(false);
                    if named {
                        self.s("function");
                    }
                    self.ch('(');
                    let params = node.get_scope().map(|s| s.get_parameters()).unwrap_or_default();
                    if !params.is_empty() {
                        self.s(&params[0]);
                        for p in &params[1..] {
                            self.s(", ");
                            self.s(p);
                        }
                    }
                    self.ch(')');
                    if named {
                        self.ch(' ');
                    } else {
                        self.s("->");
                    }
                    // we will need a block...
                }
                // no parameters or done with them
                let num = node.num_children();
                if num == 1 && !lambda {
                    self.accept(node.child(0));
                } else {
                    for i in 0..num {
                        self.accept_statement(node.child(i));
                    }
                }
            }
            JJTLENODE => self.infix_children(node, " <= ", false),
            JJTLTNODE => self.infix_children(node, " < ", false),
            // visit(ASTMapEntry)
            JJTMAPENTRY => {
                self.accept(node.child(0));
                self.s(" : ");
                self.accept(node.child(1));
            }
            // visit(ASTSetLiteral)
            JJTSETLITERAL => {
                let num = node.num_children();
                self.s("{ ");
                if num > 0 {
                    self.accept(node.child(0));
                    for i in 1..num {
                        self.ch(',');
                        self.accept(node.child(i));
                    }
                }
                self.s(" }");
            }
            // visit(ASTMapLiteral)
            JJTMAPLITERAL => {
                let num = node.num_children();
                self.s("{ ");
                if num > 0 {
                    self.accept(node.child(0));
                    for i in 1..num {
                        self.ch(',');
                        self.accept(node.child(i));
                    }
                } else {
                    self.ch(':');
                }
                self.s(" }");
            }
            // visit(ASTConstructorNode)
            JJTCONSTRUCTORNODE => {
                let num = node.num_children();
                self.s("new(");
                if num > 0 {
                    self.accept(node.child(0));
                    for i in 1..num {
                        self.s(", ");
                        self.accept(node.child(i));
                    }
                }
                self.s(")");
            }
            // visit(ASTFunctionNode) — the grammar is `#FunctionNode(2)` in both alternatives, so
            // the 3-child branch is dead in 3.2.1. Ported as-is.
            JJTFUNCTIONNODE => {
                let num = node.num_children();
                if num == 3 {
                    self.accept(node.child(0));
                    self.s(":");
                    self.accept(node.child(1));
                    self.accept(node.child(2));
                } else if num == 2 {
                    self.accept(node.child(0));
                    self.accept(node.child(1));
                }
            }
            // visit(ASTMethodNode)
            JJTMETHODNODE => {
                if node.num_children() == 2 {
                    self.accept(node.child(0));
                    self.accept(node.child(1));
                }
            }
            // visit(ASTArguments)
            JJTARGUMENTS => {
                let num = node.num_children();
                self.s("(");
                if num > 0 {
                    self.accept(node.child(0));
                    for i in 1..num {
                        self.s(", ");
                        self.accept(node.child(i));
                    }
                }
                self.s(")");
            }
            JJTMODNODE => self.infix_children(node, " % ", false),
            JJTMULNODE => self.infix_children(node, " * ", false),
            JJTNENODE => self.infix_children(node, " != ", false),
            JJTNRNODE => self.infix_children(node, " !~ ", false),
            // visit(ASTNotNode)
            JJTNOTNODE => {
                self.s("!");
                self.accept(node.child(0));
            }
            JJTNULLLITERAL => self.check_str(node, "null"),
            // visit(ASTOrNode) — need parenthesis if not in operator precedence order
            JJTORNODE => {
                let paren = node.parent().map(|p| p.is(JJTANDNODE)).unwrap_or(false);
                self.infix_children(node, " || ", paren);
            }
            // visit(ASTReference)
            JJTREFERENCE => {
                for i in 0..node.num_children() {
                    self.accept(node.child(i));
                }
            }
            // visit(ASTReferenceExpression) — the grammar is `#ReferenceExpression(1)`, so the
            // trailing `[...]` loop is dead in 3.2.1 (`(a)[0]` becomes Reference[RefExpr, ArrayAccess]).
            JJTREFERENCEEXPRESSION => {
                self.ch('(');
                self.accept(node.child(0));
                self.ch(')');
                for i in 1..node.num_children() {
                    self.s("[");
                    self.accept(node.child(i));
                    self.s("]");
                }
            }
            // visit(ASTReturnStatement)
            JJTRETURNSTATEMENT => {
                self.s("return ");
                self.accept(node.child(0));
            }
            // visit(ASTSizeFunction)
            JJTSIZEFUNCTION => {
                self.s("size ");
                self.accept(node.child(0));
            }
            // visit(ASTStringLiteral)
            JJTSTRINGLITERAL => {
                let img = replace(node.literal().expect("literal data").units(), '\'', "\\'");
                let mut image = vec![b'\'' as u16];
                image.extend_from_slice(&img);
                image.push(b'\'' as u16);
                self.check(node, &image);
            }
            // visit(ASTRegexLiteral) — ASTRegexLiteral.toString is the pattern (or "" if unset)
            JJTREGEXLITERAL => {
                let pattern: Vec<u16> = node.regex().map(|p| p.pattern()).unwrap_or("").encode_utf16().collect();
                let img = replace(&pattern, '/', "\\/");
                let mut image: Vec<u16> = "~/".encode_utf16().collect();
                image.extend_from_slice(&img);
                image.push(b'/' as u16);
                self.check(node, &image);
            }
            // visit(ASTTernaryNode)
            JJTTERNARYNODE => {
                self.accept(node.child(0));
                if node.num_children() > 2 {
                    self.s("? ");
                    self.accept(node.child(1));
                    self.s(" : ");
                    self.accept(node.child(2));
                } else {
                    self.s("?: ");
                    self.accept(node.child(1));
                }
            }
            // visit(ASTNullpNode)
            JJTNULLPNODE => {
                self.accept(node.child(0));
                self.s("??");
                self.accept(node.child(1));
            }
            JJTTRUENODE => self.check_str(node, "true"),
            JJTUNARYMINUSNODE => self.prefix_child(node, "-"),
            JJTUNARYPLUSNODE => self.prefix_child(node, "+"),
            // visit(ASTVar) — the name is *not* escaped, unlike visit(ASTIdentifier)
            JJTVAR => {
                self.s("var ");
                let name = node.identifier().expect("identifier data").get_name();
                self.check_str(node, name);
            }
            // visit(ASTWhileStatement)
            JJTWHILESTATEMENT => {
                self.s("while (");
                self.accept(node.child(0));
                self.s(") ");
                if node.num_children() > 1 {
                    self.accept_statement(node.child(1));
                } else {
                    self.ch(';');
                }
            }
            // visit(ASTDoWhileStatement)
            JJTDOWHILESTATEMENT => {
                self.s("do ");
                let nc = node.num_children();
                if nc > 1 {
                    self.accept_statement(node.child(0));
                } else {
                    self.s(";");
                }
                self.s(" while (");
                self.accept(node.child(nc - 1));
                self.s(")");
            }
            JJTSETADDNODE => self.infix_children(node, " += ", false),
            JJTSETSUBNODE => self.infix_children(node, " -= ", false),
            JJTSETMULTNODE => self.infix_children(node, " *= ", false),
            JJTSETDIVNODE => self.infix_children(node, " /= ", false),
            JJTSETMODNODE => self.infix_children(node, " %= ", false),
            JJTSETANDNODE => self.infix_children(node, " &= ", false),
            JJTSETORNODE => self.infix_children(node, " |= ", false),
            JJTSETXORNODE => self.infix_children(node, " ^= ", false),
            // visit(ASTJxltLiteral)
            JJTJXLTLITERAL => {
                let img = replace(node.literal().expect("literal data").units(), '`', "\\`");
                let mut image = vec![b'`' as u16];
                image.extend_from_slice(&img);
                image.push(b'`' as u16);
                self.check(node, &image);
            }
            // visit(ASTAnnotation)
            JJTANNOTATION => {
                self.ch('@');
                let name = node.annotation_name().expect("annotation data");
                self.s(name);
                if node.num_children() > 0 {
                    self.accept(node.child(0)); // zut
                }
            }
            // visit(ASTAnnotatedStatement)
            JJTANNOTATEDSTATEMENT => {
                for i in 0..node.num_children() {
                    if i > 0 {
                        self.ch(' ');
                    }
                    self.accept_statement(node.child(i));
                }
            }
            // port of: ParserVisitor.visit(SimpleNode) / visit(ASTAmbiguous) — both throw. Neither
            // node kind can appear in a tree a successful parse returns.
            other => panic!("AST{} : not supported yet.", JJT_NODE_NAME[other as usize]),
        }
    }
}

/// port of: java.lang.String.replace(CharSequence, CharSequence) for a single code unit target.
fn replace(s: &[u16], from: char, to: &str) -> Vec<u16> {
    let from = from as u16;
    if !s.contains(&from) {
        return s.to_vec();
    }
    let mut out = Vec::with_capacity(s.len() + 8);
    for &c in s {
        if c == from {
            out.extend(to.encode_utf16());
        } else {
            out.push(c);
        }
    }
    out
}
