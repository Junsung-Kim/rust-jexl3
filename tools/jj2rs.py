#!/usr/bin/env python3
"""jj2rs: mechanical Java -> Rust translator for the javacc-generated JEXL 3.2.1 parser sources.

It understands exactly the Java subset that ParserGeneratorCC emits for Parser.jjt (statements,
switch, labeled loops, try/catch/finally, int/long arithmetic) and fails loudly on anything else.
Generated functions keep their javacc names in snake_case so a maintainer can read the Rust next
to target/generated-sources/.../Parser.java and ParserTokenManager.java.

Usage:
  jj2rs.py tm  ParserTokenManager.java > src/parser/parser_token_manager_gen.rs
  jj2rs.py parser Parser.java          > src/parser/parser_gen.rs
"""
import re
import sys

# ----------------------------------------------------------------------------- lexing

TOKEN_RE = re.compile(r"""
    (?P<ws>\s+|//[^\n]*|/\*.*?\*/)
  | (?P<num>0[xX][0-9a-fA-F]+[lL]?|\d+[lL]?)
  | (?P<chr>'(?:\\.|[^'\\])+')
  | (?P<str>"(?:\\.|[^"\\])*")
  | (?P<id>[A-Za-z_$][A-Za-z_$0-9]*)
  | (?P<op>>>>=|<<=|>>=|>>>|\+\+|--|&&|\|\||==|!=|<=|>=|\+=|-=|\*=|/=|%=|&=|\|=|\^=|<<|>>|[{}()\[\];,.?:=<>!~+\-*/%&|^@])
""", re.S | re.X)


def lex(src):
    out = []
    i = 0
    while i < len(src):
        m = TOKEN_RE.match(src, i)
        if not m:
            raise SyntaxError("cannot lex at %r" % src[i:i + 40])
        i = m.end()
        kind = m.lastgroup
        if kind == "ws":
            continue
        out.append((kind, m.group(kind)))
    out.append(("eof", ""))
    return out


class P:
    """Recursive-descent parser for the Java subset."""

    def __init__(self, toks):
        self.t = toks
        self.i = 0

    def peek(self, k=0):
        return self.t[self.i + k]

    def val(self, k=0):
        return self.t[self.i + k][1]

    def next(self):
        tok = self.t[self.i]
        self.i += 1
        return tok

    def accept(self, v):
        if self.val() == v:
            self.i += 1
            return True
        return False

    def expect(self, v):
        if not self.accept(v):
            raise SyntaxError("expected %r got %r near %r" % (v, self.val(), [x[1] for x in self.t[self.i - 5:self.i + 5]]))

    # --------------------------------------------------------------- statements
    TYPES = ("int", "long", "boolean", "char", "Token", "String", "Object", "LinkedList")

    def is_decl(self):
        k = 0
        if self.val(k) == "final":
            k += 1
        v = self.val(k)
        if self.peek(k)[0] != "id" or v in ("return", "throw", "new", "break", "continue", "case", "else"):
            return False
        if v == "LinkedList" and self.val(k + 1) == "<":
            return True
        return self.peek(k + 1)[0] == "id" and self.val(k + 2) in ("=", ";", ",")

    def block(self):
        self.expect("{")
        body = []
        while not self.accept("}"):
            body.append(self.stmt())
        return ("block", body)

    def stmt(self):
        v = self.val()
        k = self.peek()[0]
        if v == "{":
            return self.block()
        if v == ";":
            self.next()
            return ("block", [])
        if k == "id" and self.val(1) == ":" and v not in ("default", "case"):
            self.next(); self.next()
            return ("label", v, self.stmt())
        if v == "if":
            self.next(); self.expect("(")
            c = self.expr(); self.expect(")")
            th = self.stmt()
            el = self.stmt() if self.accept("else") else None
            return ("if", c, th, el)
        if v == "while":
            self.next(); self.expect("(")
            c = self.expr(); self.expect(")")
            return ("while", c, self.stmt())
        if v == "do":
            self.next()
            b = self.stmt()
            self.expect("while"); self.expect("(")
            c = self.expr(); self.expect(")"); self.expect(";")
            return ("dowhile", b, c)
        if v == "for":
            self.next(); self.expect("(")
            if self.is_decl():
                init = ("stmt", self.stmt())
            else:
                init = None if self.val() == ";" else self.expr()
                self.expect(";")
            cond = None if self.val() == ";" else self.expr()
            self.expect(";")
            upd = None if self.val() == ")" else self.expr()
            self.expect(")")
            return ("for", init, cond, upd, self.stmt())
        if v == "switch":
            self.next(); self.expect("(")
            e = self.expr(); self.expect(")"); self.expect("{")
            cases = []  # list of (labels, stmts)
            while not self.accept("}"):
                labels = []
                while self.val() in ("case", "default"):
                    if self.accept("default"):
                        labels.append(None)
                    else:
                        self.next()
                        labels.append(self.expr())
                    self.expect(":")
                body = []
                while self.val() not in ("case", "default", "}"):
                    body.append(self.stmt())
                cases.append((labels, body))
            return ("switch", e, cases)
        if v == "try":
            self.next()
            b = self.block()
            catches = []
            fin = None
            while self.accept("catch"):
                self.expect("(")
                ty = []
                while self.val(1) != ")":
                    ty.append(self.next()[1])
                name = self.next()[1]
                self.expect(")")
                catches.append(("".join(ty), name, self.block()))
            if self.accept("finally"):
                fin = self.block()
            return ("try", b, catches, fin)
        if v == "break":
            self.next()
            lab = None if self.val() == ";" else self.next()[1]
            self.expect(";")
            return ("break", lab)
        if v == "continue":
            self.next()
            lab = None if self.val() == ";" else self.next()[1]
            self.expect(";")
            return ("continue", lab)
        if v == "return":
            self.next()
            e = None if self.val() == ";" else self.expr()
            self.expect(";")
            return ("return", e)
        if v == "throw":
            self.next()
            e = self.expr(); self.expect(";")
            return ("throw", e)
        if self.is_decl():
            self.accept("final")
            ty = self.next()[1]
            if self.accept("<"):
                ty += "<" + self.next()[1] + ">"
                self.expect(">")
            decls = []
            while True:
                name = self.next()[1]
                init = self.expr() if self.accept("=") else None
                decls.append((name, init))
                if not self.accept(","):
                    break
            self.expect(";")
            return ("decl", ty, decls)
        e = self.expr()
        self.expect(";")
        return ("expr", e)

    # --------------------------------------------------------------- expressions
    BIN = [
        ("?",),
        ("||",), ("&&",), ("|",), ("^",), ("&",), ("==", "!="), ("<", ">", "<=", ">=", "instanceof"),
        ("<<", ">>", ">>>"), ("+", "-"), ("*", "/", "%"),
    ]
    ASSIGN = ("=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=")

    def expr(self):
        left = self.ternary()
        if self.val() in self.ASSIGN:
            op = self.next()[1]
            right = self.expr()
            return ("assign", op, left, right)
        return left

    def ternary(self):
        c = self.binary(1)
        if self.accept("?"):
            a = self.expr(); self.expect(":")
            b = self.expr()
            return ("cond", c, a, b)
        return c

    def binary(self, lvl):
        if lvl >= len(self.BIN):
            return self.unary()
        left = self.binary(lvl + 1)
        while self.val() in self.BIN[lvl] and self.peek()[0] in ("op", "id"):
            op = self.next()[1]
            if op == "instanceof":
                ty = self.next()[1]
                left = ("instanceof", left, ty)
                continue
            right = self.binary(lvl + 1)
            left = ("bin", op, left, right)
        return left

    def unary(self):
        v = self.val()
        if v in ("!", "~", "-", "+"):
            self.next()
            return ("un", v, self.unary())
        if v in ("++", "--"):
            self.next()
            return ("preinc", v, self.unary())
        if v == "(" and self.peek(1)[1] in ("int", "long", "char", "ParseException", "RuntimeException", "Error") and self.val(2) == ")":
            self.next(); ty = self.next()[1]; self.next()
            return ("cast", ty, self.unary())
        return self.postfix(self.primary())

    def primary(self):
        k, v = self.next()
        if k == "num":
            return ("num", v)
        if k == "chr":
            return ("chr", v)
        if k == "str":
            return ("str", v)
        if v == "(":
            e = self.expr(); self.expect(")")
            return ("paren", e)
        if v == "new":
            ty = self.next()[1]
            while self.val() == "." and self.peek(1)[0] == "id":
                self.next()
                ty += "." + self.next()[1]
            if self.accept("["):
                n = self.expr(); self.expect("]")
                return ("newarr", ty, n)
            if self.val() == "<":
                while self.next()[1] != ">":
                    pass
            self.expect("(")
            args = self.args()
            return ("new", ty, args)
        if k == "id":
            return ("id", v)
        raise SyntaxError("unexpected %r near %r" % (v, [x[1] for x in self.t[self.i - 6:self.i + 6]]))

    def args(self):
        a = []
        if self.accept(")"):
            return a
        while True:
            a.append(self.expr())
            if self.accept(")"):
                return a
            self.expect(",")

    def postfix(self, e):
        while True:
            if self.accept("."):
                name = self.next()[1]
                if self.accept("("):
                    e = ("call", e, name, self.args())
                else:
                    e = ("field", e, name)
            elif self.val() == "(" and e[0] == "id":
                self.next()
                e = ("call", None, e[1], self.args())
            elif self.accept("["):
                idx = self.expr(); self.expect("]")
                e = ("index", e, idx)
            elif self.val() in ("++", "--"):
                e = ("postinc", self.next()[1], e)
            else:
                return e


# ----------------------------------------------------------------------------- helpers

RUST_KEYWORDS = {"continue", "break", "loop", "match", "type", "fn", "mod", "use", "impl", "move", "ref", "self", "where"}


def snake(name):
    s = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name)
    s = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1_\2", s)
    s = s.lower()
    return "r#" + s if s in RUST_KEYWORDS else s


def upper(name):
    return snake(name).upper()


def java_int_literal(text, want=None):
    """Returns (rust_text, type)."""
    t = text
    is_long = t[-1] in "lL"
    if is_long:
        t = t[:-1]
    if t.lower().startswith("0x"):
        v = int(t, 16)
    elif len(t) > 1 and t[0] == "0":
        v = int(t, 8)
    else:
        v = int(t)
    if is_long:
        if v > 0x7fffffffffffffff:
            return ("(0x%xu64 as i64)" % v, "i64")
        return ("0x%xi64" % v if t.lower().startswith("0x") else "%di64" % v, "i64")
    if v > 0x7fffffff:
        return ("(0x%xu32 as i32)" % v, "i32")
    return ("%d" % v, "i32")


def char_literal(text):
    body = text[1:-1]
    if body.startswith("\\"):
        esc = {"n": 10, "r": 13, "t": 9, "b": 8, "f": 12, "0": 0, "\\": 92, "'": 39, '"': 34}
        if body[1] == "u":
            return int(body[2:], 16)
        return esc[body[1]]
    return ord(body)


def java_string_literal(text):
    """Decode a Java string literal (octal escapes as javacc emits them) into a python str."""
    body = text[1:-1]
    out = []
    i = 0
    while i < len(body):
        c = body[i]
        if c == "\\":
            n = body[i + 1]
            if n in "01234567":
                j = i + 1
                while j < len(body) and j < i + 4 and body[j] in "01234567":
                    j += 1
                out.append(chr(int(body[i + 1:j], 8)))
                i = j
                continue
            if n == "u":
                out.append(chr(int(body[i + 2:i + 6], 16)))
                i += 6
                continue
            out.append({"n": "\n", "t": "\t", "r": "\r", "b": "\b", "f": "\f", "\\": "\\", '"': '"', "'": "'"}[n])
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


def rust_str(s):
    return '"' + "".join(c if 0x20 <= ord(c) < 0x7f and c not in '"\\' else "\\u{%x}" % ord(c) for c in s) + '"'


# ----------------------------------------------------------------------------- emitter

class Emitter:
    def __init__(self, cfg):
        self.cfg = cfg          # dict: fields{name:type}, consts{name:type}, methods{name:(ret, fallible)}
        self.out = []
        self.ind = 1
        self.locals = {}
        self.breakables = []    # stack of (kind, rust_label or None)
        self.tries = []         # stack of try labels ('try label', ret_type)
        self.nlabel = 0
        self.ret = None         # current method rust return type (inner)
        self.fallible = False

    def w(self, line):
        self.out.append("    " * self.ind + line)

    def label(self, base):
        self.nlabel += 1
        return "'%s%d" % (base, self.nlabel)

    # ---------------------------------------------------------------- expressions: return (text, type)
    def ty_of_name(self, name):
        if name in self.locals:
            return self.locals[name]
        if name in self.cfg["fields"]:
            return self.cfg["fields"][name]
        if name in self.cfg["consts"]:
            return self.cfg["consts"][name]
        return None

    def name_ref(self, name):
        if name in self.locals:
            return snake(name)
        if name in self.cfg["fields"]:
            return "self." + snake(name)
        if name in self.cfg["consts"]:
            return upper(name)
        if name in self.cfg.get("constants", {}):
            return self.cfg["constants"][name]
        raise SyntaxError("unknown name %s" % name)

    def coerce(self, text, ty, want):
        if ty == want or want is None:
            return text
        if ty == "i32" and want == "i64":
            return "(%s as i64)" % text
        if ty == "i64" and want == "i32":
            return "(%s as i32)" % text
        raise SyntaxError("cannot coerce %s:%s to %s" % (text, ty, want))

    def call_method(self, name, args, recv_text=None):
        m = self.cfg["methods"].get(name)
        if m is None:
            raise SyntaxError("unknown method %s" % name)
        ret, fallible, params = m
        targs = []
        for a, pt in zip(args, params):
            at, aty = self.e(a)
            if pt in ("i32", "i64"):
                at = self.coerce(at, aty, pt)
            targs.append(at)
        # evaluate arguments that contain calls first (Java left-to-right order, and no double &mut self borrow)
        pre = []
        for n, a in enumerate(targs):
            if "(" in a and "self." in a:
                pre.append("let __a%d = %s;" % (n, a))
                targs[n] = "__a%d" % n
        text = "%s(%s)" % (recv_text or ("self." + snake(name)), ", ".join(targs))
        if pre:
            text = "{ %s %s }" % (" ".join(pre), text + ("?" if fallible and not self.tries else ""))
            if fallible and self.tries:
                text = self.tri(text)
            return text, ret
        if fallible:
            text = self.tri(text)
        return text, ret

    def tri(self, text):
        """Propagate an Err from a fallible call: to the enclosing try block if any, else return."""
        if self.tries:
            lab = self.tries[-1]
            return "match %s { Ok(__v) => __v, Err(__e) => break %s Err(__e.into()) }" % (text, lab)
        return "%s?" % text

    def e(self, x, want=None):
        t, ty = self.e0(x)
        if want is not None and ty in ("i32", "i64") and want in ("i32", "i64"):
            t = self.coerce(t, ty, want)
            ty = want
        return t, ty

    def e0(self, x):
        k = x[0]
        hook = self.cfg.get("expr_hook")
        if hook:
            r = hook(self, x)
            if r is not None:
                return r
        if k == "num":
            return java_int_literal(x[1])
        if k == "chr":
            return str(char_literal(x[1])), "i32"
        if k == "paren":
            t, ty = self.e(x[1])
            return "(%s)" % t, ty
        if k == "id":
            if x[1] in ("true", "false"):
                return x[1], "bool"
            if x[1] == "null":
                return "None", "null"
            return self.name_ref(x[1]), self.ty_of_name(x[1])
        if k == "un":
            t, ty = self.e(x[2])
            if x[1] == "!":
                return "!%s" % t, "bool"
            if x[1] == "~":
                return "!%s" % t, ty
            return "%s%s" % (x[1], t), ty
        if k == "cast":
            t, ty = self.e(x[2])
            rt = {"int": "i32", "long": "i64", "char": "i32"}[x[1]]
            if x[1] == "char":
                return "(%s as u16 as i32)" % t, "i32"
            return "(%s as %s)" % (t, rt), rt
        if k == "bin":
            op, a, b = x[1], x[2], x[3]
            if op in ("&&", "||"):
                return "%s %s %s" % (self.e(a)[0], op, self.e(b)[0]), "bool"
            at, aty = self.e(a)
            bt, bty = self.e(b)
            if op in ("<<", ">>", ">>>"):
                if op == ">>>":
                    return "((%s as %s) >> %s) as %s" % (at, {"i32": "u32", "i64": "u64"}[aty], bt, aty), aty
                return "(%s %s %s)" % (at, op, bt), aty
            if aty == "null" or bty == "null":
                if op == "==":
                    return "%s.is_none()" % (bt if aty == "null" else at), "bool"
                return "%s.is_some()" % (bt if aty == "null" else at), "bool"
            if aty in ("i32", "i64") and bty in ("i32", "i64") and aty != bty:
                at, bt = self.coerce(at, aty, "i64"), self.coerce(bt, bty, "i64")
                aty = "i64"
            rty = "bool" if op in ("==", "!=", "<", ">", "<=", ">=") else aty
            return "(%s %s %s)" % (at, op, bt), rty
        if k == "index":
            at, aty = self.e(x[1])
            it, _ = self.e(x[2])
            ety = aty[2:-1] if aty and aty.startswith("[") else None
            return "%s[(%s) as usize]" % (at, it), ety
        if k == "preinc" or k == "postinc":
            lv, lty = self.lvalue(x[2])
            op = "+=" if x[1] == "++" else "-="
            if k == "preinc":
                return "{ %s %s 1; %s }" % (lv, op, lv), lty
            return "{ let __t = %s; %s %s 1; __t }" % (lv, lv, op), lty
        if k == "assign":
            lv, lty = self.lvalue(x[2])
            rt, rty = self.e(x[3], lty)
            if x[1] == "=":
                return "{ let __v = %s; %s = __v; __v }" % (rt, lv), lty
            return "{ %s %s %s; %s }" % (lv, x[1], rt, lv), lty
        if k == "call":
            recv, name, args = x[1], x[2], x[3]
            if recv is None:
                return self.call_method(name, args)
            raise SyntaxError("unsupported call %r" % (x,))
        if k == "field":
            raise SyntaxError("unsupported field %r" % (x,))
        if k == "cond":
            c = self.e(x[1])[0]
            a, aty = self.e(x[2])
            b, bty = self.e(x[3])
            return "if %s { %s } else { %s }" % (c, a, b), aty
        raise SyntaxError("unsupported expr %r" % (x,))

    def lvalue(self, x):
        if x[0] == "id":
            return self.name_ref(x[1]), self.ty_of_name(x[1])
        if x[0] == "index":
            t, ty = self.e(x)
            return t, ty
        hook = self.cfg.get("lvalue_hook")
        if hook:
            r = hook(self, x)
            if r is not None:
                return r
        raise SyntaxError("bad lvalue %r" % (x,))

    # ---------------------------------------------------------------- statements
    def s(self, st):
        k = st[0]
        hook = self.cfg.get("stmt_hook")
        if hook and hook(self, st):
            return
        if k == "block":
            for x in st[1]:
                self.s(x)
        elif k == "label":
            lab = "'" + st[1]
            inner = st[2]
            if inner[0] in ("while", "for", "dowhile"):
                self.loop(inner, lab)
            else:
                raise SyntaxError("label on non-loop")
        elif k == "if":
            self.w("if %s {" % self.e(st[1])[0])
            self.ind += 1; self.s(st[2]); self.ind -= 1
            el = st[3]
            while el is not None and el[0] == "if":
                self.w("} else if %s {" % self.e(el[1])[0])
                self.ind += 1; self.s(el[2]); self.ind -= 1
                el = el[3]
            if el is not None:
                self.w("} else {")
                self.ind += 1; self.s(el); self.ind -= 1
            self.w("}")
        elif k in ("while", "for", "dowhile"):
            self.loop(st, None)
        elif k == "switch":
            self.switch(st)
        elif k == "break":
            if st[1] is not None:
                self.w("break '%s;" % st[1])
            else:
                kind, lab = self.breakables[-1]
                self.w("break %s;" % lab if lab else "break;")
        elif k == "continue":
            kind, lab = [b for b in self.breakables if b[0] != "switch"][-1]
            if kind == "dowhile":
                raise SyntaxError("continue in do-while")
            self.w("continue;")
        elif k == "return":
            if st[1] is None:
                val = "()"
            else:
                val = self.e(st[1], self.ret if self.ret in ("i32", "i64") else None)[0]
            if self.tries:
                self.w("break %s Ok(Some(%s));" % (self.tries[-1], val))
            elif self.fallible:
                self.w("return Ok(%s);" % val)
            else:
                self.w("return %s;" % ("" if st[1] is None else val))
        elif k == "throw":
            t = self.throw_expr(st[1])
            if self.tries:
                self.w("break %s Err(%s);" % (self.tries[-1], t))
            else:
                self.w("return Err(%s);" % t)
        elif k == "decl":
            ty = {"int": "i32", "long": "i64", "boolean": "bool", "Token": "Tok"}.get(st[1], st[1])
            for name, init in st[2]:
                self.locals[name] = ty
                rty = {"Tok": "usize"}.get(ty, ty)
                if init is None:
                    self.w("let mut %s: %s;" % (snake(name), rty))
                else:
                    self.w("let mut %s: %s = %s;" % (snake(name), rty, self.e(init, ty)[0]))
        elif k == "expr":
            x = st[1]
            if x[0] == "assign":
                lv, lty = self.lvalue(x[2])
                rt, _ = self.e(x[3], lty)
                self.w("%s %s %s;" % (lv, x[1], rt))
            elif x[0] in ("preinc", "postinc"):
                lv, _ = self.lvalue(x[2])
                self.w("%s %s 1;" % (lv, "+=" if x[1] == "++" else "-="))
            else:
                t, ty = self.e(x)
                self.w("%s;" % t if ty in (None, "()", "unit") else "let _ = %s;" % t)
        elif k == "try":
            self.try_stmt(st)
        else:
            raise SyntaxError("unsupported stmt %r" % (st,))

    def throw_expr(self, x):
        hook = self.cfg.get("throw_hook")
        return hook(self, x)

    def loop(self, st, lab):
        k = st[0]
        prefix = (lab + ": ") if lab else ""
        if k == "while":
            c = self.e(st[1])[0]
            self.breakables.append(("loop", lab))
            self.w(prefix + ("loop {" if c == "true" else "while %s {" % c))
            self.ind += 1; self.s(st[2]); self.ind -= 1
            self.w("}")
            self.breakables.pop()
        elif k == "dowhile":
            self.breakables.append(("dowhile", lab))
            self.w(prefix + "loop {")
            self.ind += 1
            self.s(st[1])
            self.w("if !(%s) { break; }" % self.e(st[2])[0])
            self.ind -= 1
            self.w("}")
            self.breakables.pop()
        elif k == "for":
            init, cond, upd, body = st[1:]
            if init is not None and init[0] == "stmt":
                self.s(init[1])
            elif init is not None:
                self.s(("expr", init))
            self.breakables.append(("for", lab))
            if cond is None:
                self.w(prefix + "loop {")
            else:
                self.w(prefix + "while %s {" % self.e(cond)[0])
            self.ind += 1
            self.s(body)
            if upd is not None:
                self.s(("expr", upd))
            self.ind -= 1
            self.w("}")
            self.breakables.pop()

    def switch(self, st):
        e, cases = st[1], st[2]
        lab = self.label("sw")
        et, ety = self.e(e)
        self.breakables.append(("switch", lab))
        self.w("%s: {" % lab)
        self.ind += 1
        self.w("match %s {" % et)
        self.ind += 1
        has_default = False
        for labels, body in cases:
            pats = []
            for l in labels:
                if l is None:
                    has_default = True
                    pats.append("_")
                else:
                    pats.append(self.e(l)[0])
            if "_" in pats:
                pats = ["_"]
            # detect fall-through into the next case
            if body and not self.terminates(body[-1]) and cases.index((labels, body)) != len(cases) - 1:
                raise SyntaxError("switch fall-through not supported")
            self.w("%s => {" % " | ".join(pats))
            self.ind += 1
            for x in body:
                self.s(x)
            self.ind -= 1
            self.w("}")
        if not has_default:
            self.w("_ => {}")
        self.ind -= 1
        self.w("}")
        self.ind -= 1
        self.w("};")
        self.breakables.pop()

    def terminates(self, st):
        k = st[0]
        if k in ("break", "return", "throw", "continue"):
            return True
        if k == "block":
            return bool(st[1]) and self.terminates(st[1][-1])
        if k == "if":
            return st[3] is not None and self.terminates(st[2]) and self.terminates(st[3])
        return False

    def try_stmt(self, st):
        hook = self.cfg.get("try_hook")
        if hook and hook(self, st):
            return
        raise SyntaxError("unsupported try")


# ----------------------------------------------------------------------------- class scanning

def members(src):
    """Split the class body into (kind, name, text) for methods and static arrays."""
    toks = lex(src)
    p = P(toks)
    # find class body start
    while not (p.val() == "class"):
        p.next()
    while p.val() != "{":
        p.next()
    p.next()
    out = []
    while p.val() != "}" and p.peek()[0] != "eof":
        start = p.i
        # collect modifiers/type/name up to '(' or '=' or ';' or '{'
        while p.val() not in ("(", "=", ";", "{"):
            p.next()
        head = [t[1] for t in toks[start:p.i]]
        if p.val() == "(":
            name = head[-1]
            p.next()
            params = []
            depth = 1
            buf = []
            while depth:
                v = p.next()[1]
                if v == "(":
                    depth += 1
                elif v == ")":
                    depth -= 1
                    if depth == 0:
                        break
                if v == "," and depth == 1:
                    params.append(buf); buf = []
                else:
                    buf.append(v)
            if buf:
                params.append(buf)
            while p.val() != "{" and p.val() != ";":
                p.next()
            if p.accept(";"):
                continue
            bstart = p.i
            body = p.block()
            out.append(("method", name, head, params, body))
        elif p.val() == "=":
            name = head[-1]
            p.next()
            if p.val() == "{":
                depth = 0
                vals = []
                while True:
                    v = p.next()
                    if v[1] == "{":
                        depth += 1
                    elif v[1] == "}":
                        depth -= 1
                        if depth == 0:
                            break
                    elif v[1] != ",":
                        vals.append(v)
                p.expect(";")
                out.append(("array", name, head, vals))
            else:
                e = p.expr()
                p.expect(";")
                out.append(("field", name, head, e))
        elif p.val() == ";":
            p.next()
            out.append(("field", head[-1], head, None))
        else:  # '{' : initializer or inner class
            depth = 0
            while True:
                v = p.next()[1]
                if v == "{":
                    depth += 1
                elif v == "}":
                    depth -= 1
                    if depth == 0:
                        break
    return out


JTYPES = {"int": "i32", "long": "i64", "boolean": "bool", "void": "()", "char": "i32"}


# ----------------------------------------------------------------------------- token manager

def gen_tm(path):
    src = open(path).read()
    mems = members(src)
    fields = {"curChar": "i32", "jjnewStateCnt": "i32", "jjround": "i32", "jjmatchedPos": "i32",
              "jjmatchedKind": "i32", "curLexState": "i32", "defaultLexState": "i32",
              "jjrounds": "[i32]", "jjstateSet": "[i32]"}
    consts = {}
    arrays_out = []
    methods = {}
    wanted = []
    for m in mems:
        if m[0] == "array":
            name, head, vals = m[1], m[2], m[3]
            if "long" in head:
                items = [java_int_literal(v[1])[0] for v in vals]
                consts[name] = "[i64]"
                arrays_out.append("pub(crate) const %s: [i64; %d] = [%s];" % (upper(name), len(items), ", ".join(items)))
            elif "int" in head:
                items = [java_int_literal(v[1] if v[0] == "num" else v[1])[0] if v[1] != "-" else None for v in vals]
                # handle negative numbers (-1)
                merged = []
                neg = False
                for v in vals:
                    if v[1] == "-":
                        neg = True
                        continue
                    t = java_int_literal(v[1])[0]
                    merged.append("-" + t if neg else t)
                    neg = False
                consts[name] = "[i32]"
                arrays_out.append("pub(crate) const %s: [i32; %d] = [%s];" % (upper(name), len(merged), ", ".join(merged)))
            elif "String" in head:
                items = []
                for v in vals:
                    if v[1] == "null":
                        items.append("None")
                    else:
                        items.append("Some(%s)" % rust_str(java_string_literal(v[1])))
                consts[name] = "[str]"
                arrays_out.append("pub(crate) const %s: [Option<&str>; %d] = [%s];" % (upper(name), len(items), ", ".join(items)))
        elif m[0] == "method":
            name, head, params, body = m[1], m[2], m[3], m[4]
            ret = JTYPES.get(head[-2], head[-2])
            ptypes = [JTYPES.get(p[-2], p[-2]) for p in params]
            methods[name] = (ret, False, ptypes)
            if re.match(r"^jj(StopStringLiteralDfa|StartNfa|StopAtPos|MoveStringLiteralDfa|StartNfaWithStates|MoveNfa|CanMove)", name):
                wanted.append(m)
    # hand-written helpers the generated code calls
    for h, sig in {"jjCheckNAdd": ("()", False, ["i32"]), "jjAddStates": ("()", False, ["i32", "i32"]),
                   "jjCheckNAddTwoStates": ("()", False, ["i32", "i32"]),
                   "jjCheckNAddStates": ("()", False, ["i32", "i32"]), "ReInitRounds": ("()", False, [])}.items():
        methods[h] = sig

    def expr_hook(em, x):
        if x[0] == "call" and x[1] == ("id", "input_stream") and x[2] == "readChar":
            return "__READ_CHAR__", "i32"
        return None

    def try_hook(em, st):
        body, catches, fin = st[1], st[2], st[3]
        # only: try { curChar = input_stream.readChar(); } catch(java.io.IOException e) { ...; return X; }
        stmts = body[1]
        if (len(stmts) == 1 and stmts[0][0] == "expr" and stmts[0][1][0] == "assign"
                and stmts[0][1][3][0] == "call" and stmts[0][1][3][2] == "readChar" and len(catches) == 1 and fin is None):
            lv, _ = em.lvalue(stmts[0][1][2])
            em.w("match self.input_stream.read_char() {")
            em.ind += 1
            em.w("Ok(__c) => %s = __c," % lv)
            em.w("Err(_) => {")
            em.ind += 1
            em.s(catches[0][2])
            em.ind -= 1
            em.w("}")
            em.ind -= 1
            em.w("}")
            return True
        return False

    cfg = {"fields": fields, "consts": consts, "methods": methods, "expr_hook": expr_hook, "try_hook": try_hook}
    out = []
    out.append("// port of: org.apache.commons.jexl3.parser.ParserTokenManager (javacc-generated DFA/NFA part)")
    out.append("// GENERATED by tools/jj2rs.py from target/generated-sources/java/.../ParserTokenManager.java")
    out.append("// (ParserGeneratorCC output of Parser.jjt at rel/commons-jexl-3.2.1). Do not edit by hand.")
    out.append("#![allow(clippy::all, dead_code, non_snake_case, unused_mut, unused_parens, unused_variables, unreachable_code, unused_labels, unused_assignments, unused_braces)]")
    out.append("use super::parser_token_manager::ParserTokenManager;")
    out.append("")
    out.extend(arrays_out)
    out.append("")
    out.append("impl ParserTokenManager {")
    for m in wanted:
        name, head, params, body = m[1], m[2], m[3], m[4]
        em = Emitter(cfg)
        ret = JTYPES.get(head[-2], head[-2])
        em.ret = ret
        pl = []
        for p in params:
            pt = JTYPES[p[-2]]
            em.locals[p[-1]] = pt
            pl.append("mut %s: %s" % (snake(p[-1]), pt))
        static = "static" in head
        sig = "fn %s(%s%s) -> %s {" % (snake(name), "" if static else "&mut self, ", ", ".join(pl), ret) if not static else \
              "fn %s(%s) -> %s {" % (snake(name), ", ".join(pl), ret)
        out.append("    // port of: ParserTokenManager.%s" % name)
        out.append("    pub(crate) " + sig)
        em.s(body)
        body_lines = [l.replace("__READ_CHAR__", "self.input_stream.read_char()") for l in em.out]
        out.extend(body_lines)
        out.append("    }")
        out.append("")
    out.append("}")
    text = "\n".join(out)
    # static calls: jjCanMove_0 is static
    for m in wanted:
        if "static" in m[2]:
            text = text.replace("self.%s(" % snake(m[1]), "Self::%s(" % snake(m[1]))
    return text


# ----------------------------------------------------------------------------- main

if __name__ == "__main__":
    mode, path = sys.argv[1], sys.argv[2]
    if mode == "tm":
        print(gen_tm(path))
    elif mode == "parser":
        import jj2rs_parser
        print(jj2rs_parser.gen_parser(path))
    else:
        raise SystemExit("usage")
