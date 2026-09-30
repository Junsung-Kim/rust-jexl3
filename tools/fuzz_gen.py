#!/usr/bin/env python3
"""Grammar-driven JEXL 3.2.1 case generator (productions follow Parser.jjt), seeded and reproducible.

Usage: fuzz_gen.py N SEED [--ops ast|exec] [--mutate P] > cases.jsonl

Each case varies engine options/features, kind (script/expression), parameters, and (for exec)
draws context values from pools covering every Java type, null, the JSON-null host object,
boundary numbers, numeric strings, Korean/emoji strings, and the expression's own literals
injected both as strings and as numbers so equality branches fire.
"""
import json
import random
import sys

IDENTS = ["a", "b", "c", "x", "y", "z", "foo", "bar", "i", "n", "s", "m", "l", "t", "v", "input", "obj", "list", "map"]
PARAMS = ["p", "q", "r"]
PROPS = ["b", "c", "x", "size", "empty", "length", "name", "value", "0", "1", "if", "new", "var", "items", "flag", "class"]
METHODS = ["size", "isEmpty", "toString", "length", "substring", "charAt", "indexOf", "contains", "startsWith",
           "endsWith", "toUpperCase", "toLowerCase", "trim", "equals", "compareTo", "get", "put", "add", "remove",
           "containsKey", "keySet", "values", "intValue", "longValue", "doubleValue", "hashCode", "split", "replace",
           "matches", "concat", "greet", "twice", "getName", "getValue", "isFlag", "getItems", "entrySet", "iterator",
           "hasNext", "next", "clear", "set", "subList", "isNaN", "floatValue", "scale", "signum", "abs", "negate"]
NSFUNCS = ["isNull", "castToInt", "joinWithPipe", "isIpv4", "isIpv6", "getIpVersion", "absAsInt64", "absAsDouble",
           "concat", "kind", "sum", "size", "nvl"]
STRINGS = ["", "a", "abc", "ABC", "true", "false", "null", "0", "1", "2", "10", "-1", "1.5", "1e3", " 1 ", "x",
           "한글", "가나다", "😀", "a😀b", "\\n", "\\t", "\\u0041", "\\\\", "\\'", '\\"', "a.b", "1,2", "NaN", "Infinity",
           "0x1F", "007", "1L", "3h", "별+", "하늘-", "\\ud83d"]
KOR = ["별+", "별-", "하늘+", "파란하늘-", "한글", "이벤트"]

BINOPS = ["+", "-", "*", "/", "%", "div", "mod", "==", "!=", "eq", "ne", "<", ">", "<=", ">=", "lt", "gt", "le", "ge",
          "&&", "||", "and", "or", "&", "|", "^", "=~", "!~", "=^", "=$", "!^", "!$", "..", "?:", "??"]
ASSIGNOPS = ["=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^="]
UNOPS = ["-", "+", "~", "!", "not ", "empty ", "size ", "empty", "size"]


class Gen:
    def __init__(self, rnd, profile=False):
        self.r = rnd
        self.profile = profile
        self.literals = []  # literal texts seen (for value injection)

    def pick(self, xs):
        return self.r.choice(xs)

    def chance(self, p):
        return self.r.random() < p

    # ------------------------------------------------------------------ literals
    def string_lit(self):
        s = self.pick(STRINGS + KOR)
        self.literals.append(("str", s))
        q = self.pick(["'", '"'])
        s = s.replace(q, "\\" + q) if q not in ("\\'", '\\"') else s
        return q + s + q

    def number_lit(self):
        k = self.r.randint(0, 20)
        if k < 8:
            v = self.pick([0, 1, 2, 3, 7, 10, 16, 32, 100, 255, 256, 1000, 65535, 2147483647, 2147483648,
                           9223372036854775807, 9223372036854775808, 12345678901234567890])
            txt = str(v)
        elif k == 8:
            txt = self.pick(["0x1F", "0xff", "0X7FFFFFFF", "0x80000000", "0xFFFFFFFFFFFFFFFF"])
        elif k == 9:
            txt = self.pick(["00", "07", "017", "0777", "037777777777"])
        elif k == 10:
            txt = str(self.r.randint(0, 99)) + self.pick(["l", "L", "h", "H"])
        elif k in (11, 12, 13):
            txt = self.pick(["1.5", "0.1", "2.0", "3.14159", "1e3", "1E-3", "1.5e10", "0.0", "123.456", "1e308",
                             "1e309", "4.9e-324", "1.0e-400", "100.0"])
        elif k == 14:
            txt = self.pick(["1.5f", "2F", "0.1f", "3.4e38f", "1e39f", "1e-46f"])
        elif k == 15:
            txt = self.pick(["1.5d", "2D", "1e5d"])
        elif k == 16:
            txt = self.pick(["1.5b", "2B", "0.1b", "1.25b", "1.35b", "123456789.123456789b", "1e3b", "-0.5b"])
        elif k == 17:
            txt = self.pick(["NaN", "#NaN"])
        else:
            txt = str(self.r.randint(-5, 50))
            if txt.startswith("-"):
                txt = "(" + txt + ")"
        self.literals.append(("num", txt))
        return txt

    def literal(self):
        k = self.r.randint(0, 12)
        if k < 4:
            return self.number_lit()
        if k < 8:
            return self.string_lit()
        if k == 8:
            return self.pick(["true", "false", "null"])
        if k == 9:
            return self.jxlt()
        if k == 10:
            return self.pick(["~/a.*/", "~/^[0-9]+$/", "~/x\\/y/", "~/(?i)abc/", "~/\\d+/", "~/[/"])
        return self.number_lit()

    def jxlt(self):
        parts = []
        for _ in range(self.r.randint(0, 3)):
            if self.chance(0.5):
                parts.append(self.pick(["text", "한글", " ", "a-b", "\\`", "$", "#"]))
            else:
                parts.append("${" + self.expr(1) + "}")
        return "`" + "".join(parts) + "`"

    # ------------------------------------------------------------------ expressions
    def ident(self):
        return self.pick(IDENTS)

    def reference(self, d):
        base = self.ident() if self.chance(0.97) else "#" + str(self.r.randint(0, 2))
        parts = [base]
        for _ in range(self.r.randint(0, 3)):
            k = self.r.randint(0, 9)
            if k < 5:
                parts.append("." + self.pick(PROPS))
            elif k == 5:
                parts.append("?." + self.pick(PROPS))
            elif k == 6:
                parts.append("[" + (self.expr(d - 1) if d > 0 else self.literal()) + "]")
            elif k == 7:
                parts.append("." + self.pick(METHODS) + "(" + self.args(d - 1) + ")")
            elif k == 8:
                parts.append("." + self.string_lit())
            else:
                parts.append("['" + self.pick(PROPS) + "']")
        return "".join(parts)

    def args(self, d):
        n = self.r.randint(0, 3)
        return ", ".join(self.expr(max(d, 0)) for _ in range(n))

    def primary(self, d):
        if d <= 0:
            return self.pick([self.literal, self.ident, self.ident])()
        k = self.r.randint(0, 22)
        if k < 5:
            return self.reference(d)
        if k < 9:
            return self.literal()
        if k == 9:
            return "(" + self.expr(d - 1) + ")"
        if k == 10:
            return "[" + ", ".join(self.expr(d - 1) for _ in range(self.r.randint(0, 3))) + self.pick(["", "", ", ..."]) + "]"
        if k == 11:
            return "{" + ", ".join(self.expr(d - 1) for _ in range(self.r.randint(0, 3))) + "}"
        if k == 12:
            n = self.r.randint(0, 3)
            if n == 0:
                return "{:}"
            return "{" + ", ".join(self.expr(d - 1) + " : " + self.expr(d - 1) for _ in range(n)) + "}"
        if k == 13:
            return "ns:" + self.pick(NSFUNCS) + "(" + self.args(d - 1) + ")"
        if k == 14:
            return self.pick(IDENTS + ["size", "empty"]) + "(" + self.args(d - 1) + ")"
        if k == 15:
            return self.lambda_(d - 1)
        if k == 16:
            return "new(" + self.pick(["'java.lang.StringBuilder'", "'java.util.ArrayList'", "'java.lang.Integer'", "x"]) + \
                   ("" if self.chance(0.5) else ", " + self.expr(d - 1)) + ")"
        if k == 17:
            return "(" + self.lambda_(d - 1) + ")(" + self.args(d - 1) + ")"
        if k == 18:
            return self.ident() + "." + self.pick(METHODS) + "(" + self.args(d - 1) + ")"
        if k == 19:
            return self.expr(d - 1) + " ? " + self.expr(d - 1) + " : " + self.expr(d - 1)
        return self.reference(d)

    def lambda_(self, d):
        ps = self.r.sample(PARAMS, self.r.randint(0, 2))
        body = self.block(max(d, 0))
        k = self.r.randint(0, 2)
        if k == 0:
            return "function(" + ", ".join(("var " if self.chance(0.2) else "") + p for p in ps) + ") " + body
        if k == 1 or len(ps) != 1:
            return "(" + ", ".join(ps) + ") -> " + body
        return ps[0] + " -> " + body

    def unary(self, d):
        if self.chance(0.15) and d > 0:
            op = self.pick(UNOPS)
            if op in ("empty", "size"):
                return op + "(" + self.expr(d - 1) + ")"
            return op + self.unary(d - 1)
        return self.primary(d)

    def expr(self, d):
        if d <= 0:
            return self.unary(0)
        k = self.r.randint(0, 9)
        if k < 5:
            return self.unary(d) + " " + self.pick(BINOPS) + " " + self.expr(d - 1)
        if k == 5:
            return self.reference(d - 1) + " " + self.pick(ASSIGNOPS) + " " + self.expr(d - 1)
        if k == 6:
            return self.expr(d - 1) + " ? " + self.expr(d - 1) + " : " + self.expr(d - 1)
        if k == 7:
            return self.reference(d - 1) + " " + self.pick(["=!", "= !", "=="]) + " " + self.unary(d - 1)
        return self.unary(d)

    # ------------------------------------------------------------------ statements
    def block(self, d):
        return "{ " + " ".join(self.stmt(d - 1) for _ in range(self.r.randint(0, 3))) + " }"

    def stmt(self, d):
        if d <= 0:
            return self.expr(1) + ";"
        k = self.r.randint(0, 20)
        if k < 7:
            return self.expr(d) + self.pick([";", ";", ""])
        if k == 7:
            return "var " + self.ident() + ("" if self.chance(0.3) else " = " + self.expr(d - 1)) + ";"
        if k == 8:
            s = "if (" + self.expr(d - 1) + ") " + self.stmt_or_block(d - 1)
            if self.chance(0.4):
                s += " else " + self.stmt_or_block(d - 1)
            return s
        if k == 9:
            v = ("var " if self.chance(0.6) else "") + self.ident()
            return "for (" + v + " : " + self.expr(d - 1) + ") " + self.loop_body(d - 1)
        if k == 10:
            return "while (" + self.expr(d - 1) + ") " + self.loop_body(d - 1)
        if k == 11:
            return "do " + self.loop_body(d - 1) + " while (" + self.expr(d - 1) + ")"
        if k == 12:
            return "return " + self.expr(d - 1) + ";"
        if k == 13:
            return self.block(d)
        if k == 14:
            return "#pragma " + self.pick(["jexl.options", "jexl.namespace.ns", "a.b", "x", "script.mode"]) + " " + \
                self.pick(["'+strict'", "'-safe'", "1", "1.5", "true", "false", "null", "NaN", "foo.bar", "'java.lang.Math'", "'+strict -silent'"])
        if k == 15:
            return "@" + self.pick(["silent", "strict", "synchronized", "scale", "unsafe", "lexical"]) + \
                ("(" + self.args(d - 1) + ")" if self.chance(0.5) else "") + " " + self.stmt_or_block(d - 1)
        if k == 16:
            return self.pick(["break;", "continue;"])
        if k == 17:
            return "function " + self.ident() + "(" + ", ".join(self.r.sample(PARAMS, 1)) + ") " + self.block(d - 1)
        return self.expr(d) + ";"

    def stmt_or_block(self, d):
        return self.block(d) if self.chance(0.5) else self.stmt(d)

    def loop_body(self, d):
        if self.chance(0.5):
            return "{ " + self.pick(["break;", "continue;", ""]) + " " + self.stmt(d) + " }"
        return self.stmt_or_block(d)

    def script(self):
        n = self.r.randint(1, 4)
        return " ".join(self.stmt(self.r.randint(0, 3)) for _ in range(n))


FEATURE_FLAGS = ["register", "localVar", "sideEffect", "sideEffectGlobal", "arrayReferenceExpr", "newInstance", "loops",
                 "lambda", "methodCall", "structuredLiteral", "pragma", "annotation", "lexical", "lexicalShade"]


# Typed values for execution contexts (same encoding as tools/gen_arith_cases.py).
CTX_VALUES = [
    {"t": "null"}, {"t": "Boolean", "v": "true"}, {"t": "Boolean", "v": "false"},
    {"t": "Integer", "v": "0"}, {"t": "Integer", "v": "1"}, {"t": "Integer", "v": "2"},
    {"t": "Integer", "v": "-1"}, {"t": "Integer", "v": "2147483647"},
    {"t": "Long", "v": "9223372036854775807"}, {"t": "Long", "v": "3"},
    {"t": "Double", "v": "1.5"}, {"t": "Double", "v": "NaN"}, {"t": "Double", "v": "0.0"},
    {"t": "Float", "v": "2.5"}, {"t": "BigInteger", "v": "123456789012345678901234567890"},
    {"t": "BigDecimal", "v": "1.25"}, {"t": "Character", "v": "a"},
    {"t": "String", "v": ""}, {"t": "String", "v": "abc"}, {"t": "String", "v": "1"},
    {"t": "String", "v": "10"}, {"t": "String", "v": "true"}, {"t": "String", "v": "한글"},
    {"t": "String", "v": "\ud83d\ude00"}, {"t": "String", "v": "별+"},
    {"t": "List", "c": "java.util.ArrayList", "v": [{"t": "Integer", "v": "1"}, {"t": "String", "v": "a"}]},
    {"t": "List", "c": "java.util.ArrayList", "v": []},
    {"t": "Set", "c": "java.util.LinkedHashSet", "v": [{"t": "Integer", "v": "1"}]},
    {"t": "Map", "c": "java.util.LinkedHashMap", "v": [[{"t": "String", "v": "b"}, {"t": "Integer", "v": "7"}]]},
    {"t": "Map", "c": "java.util.LinkedHashMap", "v": []},
    {"t": "Array", "c": "Object", "v": [{"t": "Integer", "v": "1"}, {"t": "Integer", "v": "2"}]},
    {"t": "Array", "c": "int", "v": [{"t": "Integer", "v": "5"}]},
    {"t": "Host", "v": "jsonNull"}, {"t": "Host", "v": "bean"},
]

CTX_NAMES = IDENTS + PARAMS + ["a.b", "b.c", "x.value", "input.size", "obj.name", "A.b", "ConnectFlow.step"]


def context(r):
    """A MapContext binding: each name is bound with probability 0.6."""
    ctx = {}
    for name in CTX_NAMES:
        if r.random() < 0.6:
            ctx[name] = r.choice(CTX_VALUES)
    return ctx


def engine_conf(r):
    conf = {}
    if r.random() < 0.7:
        conf["namespaces"] = {"ns": "ns"}
    for flag in ("strict", "silent", "safe", "lexical", "lexicalShade", "antish", "cancellable"):
        if r.random() < 0.15:
            conf[flag] = r.random() < 0.5
    if r.random() < 0.3:
        f = {}
        for flag in r.sample(FEATURE_FLAGS, r.randint(1, 4)):
            f[flag] = r.random() < 0.5 if flag not in ("register",) else True
        if r.random() < 0.2:
            f["reservedNames"] = r.sample(IDENTS, 2)
        conf["features"] = f
    if r.random() < 0.2:
        conf["arithmetic"] = {"strict": r.random() < 0.5}
        if r.random() < 0.3:
            conf["arithmetic"]["mathContext"] = r.choice(["DECIMAL32", "DECIMAL64", "UNLIMITED", "5:HALF_UP"])
        if r.random() < 0.3:
            conf["arithmetic"]["mathScale"] = str(r.randint(0, 4))
    if r.random() < 0.1:
        conf["collectMode"] = str(r.randint(0, 2))
    return conf


def mutate(r, src):
    toks = src.split(" ")
    if len(toks) < 2:
        return src + r.choice([")", "(", ";", "}", "=", "+", ".", ":", ","])
    i = r.randrange(len(toks))
    k = r.randint(0, 3)
    if k == 0:
        del toks[i]
    elif k == 1:
        toks.insert(i, r.choice(["(", ")", "{", "}", "[", "]", ";", ",", ":", "=", "+", ".", "?", "var", "if", "@a", "->"]))
    elif k == 2:
        toks[i] = toks[i][:-1] if len(toks[i]) > 1 else ""
    else:
        j = r.randrange(len(toks))
        toks[i], toks[j] = toks[j], toks[i]
    return " ".join(toks)


def main():
    args = sys.argv[1:]
    ops = "ast"
    mut = 0.25
    if "--ops" in args:
        i = args.index("--ops"); ops = args[i + 1]; del args[i:i + 2]
    if "--mutate" in args:
        i = args.index("--mutate"); mut = float(args[i + 1]); del args[i:i + 2]
    n, seed = int(args[0]), int(args[1])
    r = random.Random(seed)
    for i in range(n):
        g = Gen(r)
        kind = "script" if r.random() < 0.75 else "expression"
        src = g.script() if kind == "script" else g.expr(r.randint(0, 4))
        if r.random() < mut:
            src = mutate(r, src)
        case = {"id": "f%d_%d" % (seed, i), "kind": kind, "src": src, "engine": engine_conf(r)}
        if "#" in src.replace("#pragma", "") and r.random() < 0.8:
            case["engine"].setdefault("features", {})["register"] = True
        if kind == "script" and r.random() < 0.2:
            case["params"] = r.sample(PARAMS + IDENTS[:3], r.randint(1, 2))
        if ops == "ast":
            case["ops"] = ["ast", "vars", "params", "locals", "pragmas"]
        elif ops == "parsed":
            case["ops"] = ["parsed"]
        else:
            case["ops"] = ["vars", "exec", "ctx"]
            case["ctx"] = context(r)
            if case.get("params"):
                case["args"] = [r.choice(CTX_VALUES) for _ in case["params"]]
        # ensure_ascii keeps lone surrogates representable, like the Java oracle writer
        print(json.dumps(case))


if __name__ == "__main__":
    main()
