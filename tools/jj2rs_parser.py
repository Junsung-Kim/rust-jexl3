"""jj2rs parser mode: translates the productions and lookahead routines of the javacc-generated
Parser.java into Rust (see jj2rs.py). Semantic actions are mapped statement by statement from an
explicit table; an unknown statement aborts the translation."""
import re

import jj2rs as J

# Java statement text (tokens joined by single spaces) -> Rust statement(s).
# `{TRI}` marks a fallible call site: replaced by `?` or by a break to the enclosing try block.
ACTIONS = {
    "pushUnit ( jjtn000 ) ;": "self.push_unit(jjtn000);",
    "popUnit ( jjtn000 ) ;": "self.pop_unit(jjtn000);",
    "jjtn000 . setScope ( frame ) ;": "self.set_scope(jjtn000, frame);",
    "loopCount += 1 ;": "self.loop_count += 1;",
    "loopCount -= 1 ;": "self.loop_count -= 1;",
    "pushFrame ( ) ;": "self.push_frame();",
    "jjtn000 . setName ( t . image ) ;": "self.set_annotation_name(jjtn000, t);",
    "declareVariable ( jjtn000 , t ) ;": "{TRI:self.declare_variable(jjtn000, t)};",
    "declarePragma ( stringify ( lstr ) , value ) ;": "{TRI:self.declare_pragma(&stringify(&lstr), value)};",
    "lstr . add ( t . image ) ;": "lstr.push(self.image(t));",
    "result = NumberParser . parseInteger ( v . image ) ;": "result = {TRI:NumberParser::parse_integer(&self.image(v))};",
    "result = NumberParser . parseDouble ( v . image ) ;": "result = {TRI:NumberParser::parse_double(&self.image(v))};",
    "result = Parser . buildString ( v . image , true ) ;": "result = Value::String(JString::new(string_parser::build_string(&self.tokens[v].image, true)));",
    "result = stringify ( lstr ) ;": "result = Value::string(&stringify(&lstr));",
    "result = true ;": "result = Value::Boolean(true);",
    "result = false ;": "result = Value::Boolean(false);",
    "result = null ;": "result = Value::Null;",
    "result = Double . NaN ;": "result = Value::Double(f64::NAN);",
    "jjtn000 . setSymbol ( top ? checkVariable ( jjtn000 , t . image ) : t . image ) ;":
        "{ let __name = if top { {TRI:self.check_variable(jjtn000, &self.image(t))} } else { self.image(t) }; self.set_symbol_name(jjtn000, &__name); }",
    "jjtn000 . setSymbol ( t . image ) ;": "{ let __name = self.image(t); self.set_symbol_name(jjtn000, &__name); }",
    "jjtn000 . setNamespace ( ns . image , id . image ) ;": "self.set_namespace(jjtn000, ns, id);",
    "jjtn000 . setReal ( \"NaN\" ) ;": "{TRI:self.set_real(jjtn000, \"NaN\")};",
    "jjtn000 . setReal ( t . image ) ;": "{ let __s = self.image(t); {TRI:self.set_real(jjtn000, &__s)}; }",
    "jjtn000 . setNatural ( t . image ) ;": "{ let __s = self.image(t); {TRI:self.set_natural(jjtn000, &__s)}; }",
    "jjtn000 . setLiteral ( Parser . buildString ( t . image , true ) ) ;": "self.set_string_literal(jjtn000, t);",
    "jjtn000 . setLiteral ( Parser . buildRegex ( t . image ) ) ;": "{TRI:self.set_regex_literal(jjtn000, t)};",
    "declareParameter ( t ) ;": "{TRI:self.declare_parameter(t)};",
    "if ( loopCount == 0 ) { throwParsingException ( null , t ) ; }":
        "if self.loop_count == 0 { {THROW:self.throw_parsing_exception(false, Some(t))} }",
}
for n in range(1, 11):
    ACTIONS["jjtn%03d . setIdentifier ( t . image ) ;" % n] = "self.set_identifier(jjtn%03d, t, false);" % n
    ACTIONS["jjtn%03d . setIdentifier ( Parser . buildString ( t . image , true ) ) ;" % n] = "self.set_identifier(jjtn%03d, t, true);" % n

PRODUCTIONS_WITH_ARGS = {"JexlScript": [("frame", "Option<ScopeId>")], "JexlExpression": [("frame", "Option<ScopeId>")],
                         "Identifier": [("top", "bool")], "pragmaKey": [("lstr", "&mut Vec<String>")]}
RETURNS = {"JexlScript": "NodeId", "JexlExpression": "NodeId", "dotName": "Tok", "pragmaValue": "Value"}


def stmt_text(toks):
    return " ".join(t[1] for t in toks)


class ParserEmitter(J.Emitter):
    """Emitter aware of jjtree scopes and the parser's fallible calls."""

    def __init__(self, cfg):
        super().__init__(cfg)
        self.jjtc = set()
        self.scan = False  # inside a jj_3 routine

    def fill(self, rust):
        """Expand {TRI:call} and {THROW:expr} markers of an action."""
        def tri(m):
            return self.tri(m.group(1))

        def throw(m):
            if self.tries:
                return "break %s Err(%s.into());" % (self.tries[-1], m.group(1))
            return "return Err(%s.into());" % m.group(1)
        out = re.sub(r"\{TRI:([^{}]*(?:\{[^{}]*\}[^{}]*)*)\}", tri, rust)
        out = re.sub(r"\{THROW:([^{}]*)\}", throw, out)
        return out


def gen_parser(path):
    src = open(path).read()
    # raw statement texts for the action table
    toks_all = J.lex(src)
    mems = J.members(src)
    productions = []
    jj2 = []
    jj3 = []
    for m in mems:
        if m[0] != "method":
            continue
        name = m[1]
        if re.match(r"^jj_2_\d+$", name):
            jj2.append(name)
        elif re.match(r"^jj_3(R)?_\d+$", name):
            jj3.append(m)
        elif name in ("parse", "ReInit", "Parser", "jj_consume_token", "jj_scan_token", "getNextToken", "getToken",
                      "generateParseException", "trace_enabled", "enable_tracing", "disable_tracing"):
            continue
        else:
            productions.append(m)

    methods = {}
    for m in productions:
        methods[m[1]] = (RETURNS.get(m[1], "()"), True, [t for _, t in PRODUCTIONS_WITH_ARGS.get(m[1], [])])
    for n in jj2:
        methods[n] = ("bool", True, ["i32"])
    for m in jj3:
        methods[m[1]] = ("bool", True, [])
    methods["jj_consume_token"] = ("Tok", True, ["i32"])
    methods["jj_scan_token"] = ("bool", True, ["i32"])
    methods["getToken"] = ("Tok", True, ["i32"])
    methods["isDeclaredNamespace"] = ("bool", False, ["Tok", "Tok"])
    fields = {"jj_scanpos": "Tok", "jj_lastpos": "Tok", "token": "Tok", "jj_la": "i32", "jj_lookingAhead": "bool",
              "jj_semLA": "bool", "loopCount": "i32"}
    consts = {}
    constants = {}
    for line in open(path.replace("Parser.java", "ParserConstants.java")):
        mm = re.match(r"\s*int ([A-Za-z_]+) = (\d+);", line)
        if mm:
            # a token named like a Rust keyword (`mod`) needs a raw identifier
            constants[mm.group(1)] = "r#" + mm.group(1) if mm.group(1) in J.RUST_KEYWORDS else mm.group(1)
    tconst = open(path.replace("/java/org", "/jjtree/org").replace("Parser.java", "ParserTreeConstants.java")).read()
    for mm in re.finditer(r"public int (JJT[A-Z]+) = (\d+);", tconst):
        constants[mm.group(1)] = mm.group(1)

    def expr_hook(em, x):
        k = x[0]
        # jj_nt.kind
        if x == ("field", ("id", "jj_nt"), "kind"):
            return "self.tokens[self.jj_nt].kind", "i32"
        if k == "field" and x[2] == "next" and x[1][0] == "id":
            raise SyntaxError("token next access in generated code")
        if k == "field" and x[2] == "image":
            return "self.image(%s)" % em.e(x[1])[0], "String"
        if k == "call" and x[1] is None and x[2] == "isDeclaredNamespace":
            a = em.e(x[3][0])[0]
            b = em.e(x[3][1])[0]
            return "{ let __t1 = %s; let __t2 = %s; self.is_declared_namespace(__t1, __t2) }" % (a, b), "bool"
        if k == "call" and x[1] == ("id", "jjtree") and x[2] == "nodeCreated":
            return "self.jjtree.node_created()", "bool"
        if k == "call" and x[1] == ("id", "jjtree") and x[2] == "nodeArity":
            return "self.jjtree.node_arity()", "i32"
        if k == "call" and x[1] == ("id", "jjtn000") and x[2] == "script":
            return "self.script_of(jjtn000)", "NodeId"
        if k == "str" and x[1] == '""':
            return "\"\"", "str"
        if k == "bin" and x[1] == "!=" and x[2] == ("str", '""') and x[3] == ("id", "null"):
            return "true", "bool"
        if k == "id" and x[1] in constants and x[1] not in em.locals:
            return constants[x[1]], "i32"
        if k == "un" and x[1] == "-" and x[2][0] == "num":
            return "-" + x[2][1], "i32"
        return None

    def lvalue_hook(em, x):
        return None

    def throw_hook(em, x):
        if x == ("new", "ParseException", []):
            return "PErr::Parse(ParseException::new())"
        if x[0] == "new" and x[1] == "IllegalStateException":
            return "PErr::Jexl(JexlException::java(\"java.lang.IllegalStateException\", Some(\"Missing return statement in function\".into())))"
        raise SyntaxError("throw %r" % (x,))

    cfg = {"fields": fields, "consts": consts, "methods": methods, "constants": constants,
           "expr_hook": expr_hook, "lvalue_hook": lvalue_hook, "throw_hook": throw_hook}

    # ---------------------------------------------------------------- statement hooks
    def stmt_hook(em, st):
        k = st[0]
        # jjtree boilerplate
        if k == "decl" and st[1].startswith("AST"):
            name, init = st[2][0]
            em.locals[name] = "Node"
            em.w("let %s: NodeId = self.new_node(%s);" % (name, init[2][0][1]))
            return True
        if k == "decl" and st[1] == "boolean" and st[2][0][0].startswith("jjtc"):
            em.locals[st[2][0][0]] = "bool"
            em.w("let mut %s: bool = true;" % st[2][0][0])
            return True
        if k == "decl" and st[1] == "LinkedList<String>":
            em.locals["lstr"] = "list"
            em.w("let mut lstr: Vec<String> = Vec::new();")
            return True
        if k == "decl" and st[1] == "Object":
            name = st[2][0][0]
            em.locals[name] = "Value"
            em.w("let mut %s: Value;" % name)
            return True
        if k == "expr":
            x = st[1]
            if x[0] == "call" and x[1] == ("id", "jjtree"):
                m, args = x[2], x[3]
                a0 = em.e(args[0])[0] if args else None
                if m == "openNodeScope":
                    em.w("self.jjtree.open_node_scope(%s);" % a0)
                elif m == "clearNodeScope":
                    em.w("self.jjtree.clear_node_scope(%s);" % a0)
                elif m == "popNode":
                    em.w("self.jjtree.pop_node();")
                elif m == "closeNodeScope":
                    a1 = args[1]
                    if a1 == ("id", "true"):
                        em.w("self.close_node_scope_cond(%s, true);" % a0)
                    elif a1[0] == "num":
                        em.w("self.close_node_scope_num(%s, %s);" % (a0, a1[1]))
                    elif a1[0] == "bin":
                        em.w("{ let __c = self.jjtree.node_arity() > %s; self.close_node_scope_cond(%s, __c); }" % (a1[3][1], a0))
                    else:
                        raise SyntaxError("closeNodeScope %r" % (a1,))
                else:
                    raise SyntaxError("jjtree.%s" % m)
                return True
            if x[0] == "call" and x[1] is None and x[2] == "jjtreeOpenNodeScope":
                return True  # JexlParser.jjtreeOpenNodeScope does nothing
            if x[0] == "call" and x[1] is None and x[2] == "jjtreeCloseNodeScope":
                em.w(em.tri("self.jjtree_close_node_scope(%s)" % em.e(x[3][0])[0]) + ";")
                return True
            if x[0] == "call" and x[1] is not None and x[1][0] == "id" and x[1][1].startswith("jjtn"):
                if x[2] == "jjtSetFirstToken":
                    em.w("{ let __t = %s; self.set_first_token(%s, __t); }" % (em.tri("self.get_token(1)"), x[1][1]))
                    return True
                if x[2] == "jjtSetLastToken":
                    return True  # JexlNode.jjtSetLastToken does nothing; getToken(0) has no side effect
            if x[0] == "call" and x[1] is None and x[2] == "jj_consume_token":
                em.w("let _ = %s;" % em.call_method("jj_consume_token", x[3])[0])
                return True
            if x[0] == "assign" and x[3][0] == "call" and x[3][1] is None and x[3][2] in ("jj_consume_token", "dotName"):
                lv = J.snake(x[2][1])
                em.w("%s = %s;" % (lv, em.call_method(x[3][2], x[3][3])[0]))
                return True
            if x[0] == "assign" and x[3][0] == "call" and x[3][2] == "pragmaValue":
                em.w("%s = %s;" % (x[2][1], em.call_method("pragmaValue", [])[0]))
                return True
            if x[0] == "call" and x[1] is None and x[2] == "pragmaKey":
                em.w(em.tri("self.pragma_key(lstr)") + ";" if em.locals.get("lstr") == "&mut" else em.tri("self.pragma_key(&mut lstr)") + ";")
                return True
        if k == "decl" and st[1] == "Token":
            for name, init in st[2]:
                em.locals[name] = "Tok"
                if init is None:
                    em.w("let mut %s: Tok = 0;" % J.snake(name))
                else:
                    em.w("let mut %s: Tok = %s;" % (J.snake(name), em.e(init)[0]))
            return True
        if k == "decl" and st[1] == "String":
            raise SyntaxError("String decl")
        # semantic actions
        text = em.raw_text(st)
        if text in ACTIONS:
            em.w(em.fill(ACTIONS[text]))
            return True
        if k == "block" and len(st[1]) == 1 and st[1][0][0] == "if" and st[1][0][1] == ("bin", "!=", ("str", '""'), ("id", "null")):
            ret = st[1][0][2]
            em.s(ret)
            return True
        if k == "if" and st[1] == ("bin", "!=", ("str", '""'), ("id", "null")):
            em.s(st[2])
            return True
        return False

    def try_hook(em, st):
        body, catches, fin = st[1], st[2], st[3]
        assert fin is not None and (not catches or (len(catches) == 1 and catches[0][0] == "Throwable"))
        lab = em.label("try")
        em.w("let mut %s: Result<Option<%s>, PErr> = %s: {" % ("__r" + lab[4:], em.ret_rust, lab))
        em.ind += 1
        em.tries.append(lab)
        em.s(body)
        em.tries.pop()
        em.w("Ok(None)")
        em.ind -= 1
        em.w("};")
        rv = "__r" + lab[4:]
        # catch (Throwable): the jjtree cleanup, then rethrow
        if catches:
            em.w("if %s.is_err() {" % rv)
            em.ind += 1
            cst = catches[0][2][1]
            for c in cst:
                if c[0] == "if" and c[1][0] == "instanceof":
                    continue  # rethrow: the pending error in __r is kept
                if c[0] == "throw":
                    continue
                em.s(c)
            em.ind -= 1
            em.w("}")
        # finally
        em.s(fin)
        em.w("match %s {" % rv)
        em.w("    Ok(Some(__v)) => return Ok(__v),")
        em.w("    Ok(None) => {}")
        em.w("    Err(__e) => return Err(__e),")
        em.w("}")
        return True

    cfg["stmt_hook"] = stmt_hook
    cfg["try_hook"] = try_hook

    def raw_text(self, st):
        return self._raw.get(id(st))
    ParserEmitter.raw_text = raw_text

    out = []
    out.append("// port of: org.apache.commons.jexl3.parser.Parser (javacc-generated productions and lookahead routines)")
    out.append("// GENERATED by tools/jj2rs.py from target/generated-sources/java/.../Parser.java")
    out.append("// (ParserGeneratorCC output of Parser.jjt at rel/commons-jexl-3.2.1). Do not edit by hand.")
    out.append("#![allow(clippy::all, dead_code, non_snake_case, unused_mut, unused_parens, unused_variables, unreachable_code, unused_labels, unused_assignments, unused_braces, unreachable_patterns, redundant_semicolons, non_upper_case_globals)]")
    out.append("use super::parser::{Parser, PErr, ScanErr, Tok, stringify};")
    out.append("use super::parse_exception::ParseException;")
    out.append("use super::parser_constants::*;")
    out.append("use super::parser_tree_constants::*;")
    out.append("use super::jexl_node::NodeId;")
    out.append("use super::number_parser::NumberParser;")
    out.append("use super::string_parser;")
    out.append("use crate::internal::scope::ScopeId;")
    out.append("use crate::java::string::JString;")
    out.append("use crate::jexl_exception::JexlException;")
    out.append("use crate::value::Value;")
    out.append("")
    out.append("impl Parser {")

    # raw statement text map: re-parse to capture token spans of statements
    for m in productions:
        name, head, params, body = m[1], m[2], m[3], m[4]
        em = ParserEmitter(cfg)
        em._raw = {}
        body = capture_raw(src, name, em)
        ret = RETURNS.get(name, "()")
        em.ret_rust = {"Tok": "Tok"}.get(ret, ret)
        em.ret = None
        em.fallible = True
        pl = []
        for pname, pty in PRODUCTIONS_WITH_ARGS.get(name, []):
            em.locals[pname] = pty
            pl.append("%s: %s" % (pname, pty))
            if pname == "lstr":
                em.locals["lstr"] = "&mut"
        out.append("    // port of: Parser.%s" % name)
        out.append("    pub(crate) fn %s(&mut self%s) -> Result<%s, PErr> {" % (J.snake(name), "".join(", " + p for p in pl), em.ret_rust))
        em.s(body)
        for l in em.out:
            out.append(l)
        if em.ret_rust == "()":
            # javacc emits no trailing statement for a void production
            out.append("        Ok(())")
        out.append("    }")
        out.append("")
    for n in jj2:
        k = n[len("jj_2_"):]
        out.append("    // port of: Parser.%s" % n)
        out.append("    pub(crate) fn jj_2_%s(&mut self, xla: i32) -> Result<bool, PErr> {" % k)
        out.append("        self.jj_la = xla;")
        out.append("        self.jj_scanpos = self.token;")
        out.append("        self.jj_lastpos = self.token;")
        out.append("        match self.jj_3_%s() {" % k)
        out.append("            Ok(b) => Ok(!b),")
        out.append("            Err(ScanErr::Success) => Ok(true),")
        out.append("            Err(ScanErr::Lex(e)) => Err(PErr::Lex(e)),")
        out.append("        }")
        out.append("    }")
        out.append("")
    for m in jj3:
        name, body = m[1], m[4]
        em = ParserEmitter(cfg)
        em._raw = {}
        em.fallible = True
        em.ret = None
        em.scan = True
        out.append("    // port of: Parser.%s" % name)
        out.append("    pub(crate) fn %s(&mut self) -> Result<bool, ScanErr> {" % J.snake(name).replace("jj_3_r_", "jj_3r_"))
        em.s(body)
        for l in em.out:
            out.append(l)
        out.append("    }")
        out.append("")
    out.append("}")
    text = "\n".join(out)
    text = text.replace("self.jj_3_r_", "self.jj_3r_")
    return text


def capture_raw(src, name, em):
    """Record the raw token text of every statement of method `name` (for the ACTIONS table)."""
    toks = J.lex(src)
    p = J.P(toks)
    # locate the method body
    for i in range(len(toks) - 2):
        if toks[i][1] == name and toks[i + 1][1] == "(" and i > 0 and toks[i - 1][1] in ("void", "ASTJexlScript", "Token", "Object"):
            j = i
            while toks[j][1] != "{":
                j += 1
            p.i = j
            break
    orig_stmt = J.P.stmt

    def stmt(self):
        start = self.i
        st = orig_stmt(self)
        em._raw[id(st)] = stmt_text(self.t[start:self.i])
        return st
    J.P.stmt = stmt
    try:
        body = p.block()
    finally:
        J.P.stmt = orig_stmt
    return body
