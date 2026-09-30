#!/usr/bin/env python3
"""Generates JXLT (unified expression / template) differential cases for tests/jxlt_oracle.rs.

Usage: gen_jxlt_cases.py N [SEED] > cases.jsonl

Two kinds are emitted, both understood by oracle/src/rustjexl/oracle/Oracle.java:
  {"kind":"jxlt",     "src": "<unified expression>", ...}  -> JxltEngine.createExpression/evaluate
  {"kind":"template", "src": "<template source>",    ...}  -> JxltEngine.createTemplate/evaluate

The first CORPUS cases are hand-written: every shape of the TemplateEngine parse state machine
(CONST / IMMEDIATE0 / DEFERRED0 / IMMEDIATE1 / DEFERRED1 / ESCAPE), composite and nested
expressions, the `$$` directive prefix, jexl:print / jexl:include / $jexl, pragmas and the
malformed-expression messages. The rest are random compositions of the same pieces.
"""
import json
import random
import sys

# --------------------------------------------------------------------------- context

CTX_VALUES = [
    {"t": "null"},
    {"t": "Boolean", "v": "true"},
    {"t": "Boolean", "v": "false"},
    {"t": "Integer", "v": "0"},
    {"t": "Integer", "v": "1"},
    {"t": "Integer", "v": "2"},
    {"t": "Integer", "v": "-1"},
    {"t": "Integer", "v": "2147483647"},
    {"t": "Long", "v": "3"},
    {"t": "Double", "v": "1.5"},
    {"t": "Double", "v": "NaN"},
    {"t": "Float", "v": "2.5"},
    {"t": "BigInteger", "v": "123456789012345678901234567890"},
    {"t": "BigDecimal", "v": "1.25"},
    {"t": "Character", "v": "a"},
    {"t": "String", "v": ""},
    {"t": "String", "v": "abc"},
    {"t": "String", "v": "1"},
    {"t": "String", "v": "b"},
    {"t": "String", "v": "true"},
    {"t": "String", "v": "한글"},
    {"t": "String", "v": "😀"},
    {"t": "String", "v": "\ud800"},          # lone high surrogate
    {"t": "String", "v": "\udfff"},          # lone low surrogate
    {"t": "String", "v": "${a}"},
    {"t": "String", "v": "a + 1"},
    {"t": "List", "c": "java.util.ArrayList", "v": [{"t": "Integer", "v": "1"}, {"t": "String", "v": "a"}]},
    {"t": "List", "c": "java.util.ArrayList", "v": []},
    {"t": "Set", "c": "java.util.LinkedHashSet", "v": [{"t": "Integer", "v": "1"}]},
    {"t": "Map", "c": "java.util.LinkedHashMap", "v": [[{"t": "String", "v": "b"}, {"t": "Integer", "v": "7"}]]},
    {"t": "Array", "c": "Object", "v": [{"t": "Integer", "v": "1"}, {"t": "Integer", "v": "2"}]},
    {"t": "Host", "v": "bean"},
    {"t": "Host", "v": "jsonNull"},
]

CTX_NAMES = ["a", "b", "c", "x", "y", "z", "s", "l", "m", "n", "i", "v", "foo", "bar", "obj"]

# --------------------------------------------------------------------------- JEXL fragments

EXPRS = [
    "a", "b", "x", "y", "s", "n", "i", "foo",
    "a + 1", "a + b", "a * 2", "a - b", "a / 2", "a % 3", "-a", "!a", "~a",
    "a > b", "a == b", "a =~ b", "a ?: b", "a ?? b", "a ? x : y",
    "a && b", "a || b",
    "'lit'", "\"lit\"", "1", "2.5", "1_000", "0x1f", "null", "true",
    "[1, 2, 3]", "{1 : 2}", "{ 1, 2 }", "[]",
    "l[0]", "m['b']", "m.b",
    "s.length()", "size(l)", "empty(l)", "s + ''",
    "l.0", "a.b.c",
    "(a)", "(a + b) * 2",
    "x = 1", "x += 1",
    "a.`${b}`",
    "'a}b'", "\"a}b\"", "'{'", "'\\\\'", "'it\\'s'",
    "'한'", "'😀'",
    "undefinedvar", "undefined.thing", "nosuch()", "ns:nosuch()",
    "1/0", "a.missingMethod()",
    "(z)->{ z + 1 }", "()->1",
    "var q = 1",                 # rejected in a ${}: pins the parse error
    "if (a) { b }",              # ditto
    "for(q : l) { q }",          # ditto
]

CONSTS = [
    "", "abc", " ", "\n", "x\ny", "text ", " and ", "-", ".", ":",
    "$", "#", "$$", "##", "#$", "$#", "{", "}", "{}", "\\", "\\\\",
    "\\$", "\\#", "\\$}", "\\n", "\\t", "\\u0041", "\\q",
    "$x", "#x", "$ {", "# {", "${", "#{",
    "한글", "😀", "\ud800", "\udfff",
    "'", "\"", "a'b", 'a"b',
]


def imm(e):
    return "${" + e + "}"


def dfr(e):
    return "#{" + e + "}"


def nested(e):
    return "#{" + "a + " + imm(e) + "}"


# --------------------------------------------------------------------------- hand-written corpus

CORPUS_JXLT = [
    # constant only
    "", "abc", "   ", "\n", "a\nb\n",
    # immediate
    "${a}", "${ a }", "${a + 1}", "${'x'}", "${null}", "${}", "${ }",
    "${a}${b}", "x${a}y${b}z", "${a}${a}",
    # deferred
    "#{a}", "#{ a }", "#{a + 1}", "#{}", "#{a}#{b}", "#{a}${b}", "${a}#{b}",
    # nested (immediate inside deferred)
    "#{${a}}", "#{x${a}y}", "#{${a}${b}}", "#{'p' + ${a}}", "x#{${a}}y",
    # braces / balance
    "${{1 : 2}}", "${ {1,2} }", "${[1,2]}", "#{{1:2}}", "#{ {1,2} }",
    "${a ? '{' : '}'}", "${'}'}", "${\"}\"}", "#{'}'}", "#{\"}\"}",
    "${'{'}", "#{'{'}", "${'\\\\'}", "${'a\\'b'}",
    # escapes
    "\\${a}", "\\#{a}", "\\\\${a}", "\\x", "\\\\", "\\", "a\\", "a\\\\",
    "$${a}", "##{a}", "$ {a}", "# {a}", "$", "#", "a$", "a#", "$a", "#a",
    # malformed
    "${", "#{", "${a", "#{a", "${a + ", "#{${a}", "${}}", "#{}}",
    "${a}}", "#{a}}", "${'unterminated}", "#{'unterminated}",
    # parse errors inside
    "${var q = 1}", "${if (a) { b }}", "${a +}", "${(}", "${1 2}",
    "${@ann x}", "${#pragma a b}",
    # unicode
    "${'한'}", "${a}한글${b}", "\ud800${a}", "${a}\udfff",
    # long-ish composite
    "p${a}q#{b}r${c}s", "#{${a}}#{${b}}",
]

CORPUS_TEMPLATE = [
    "", "\n", "abc", "abc\n", "a\nb\n", "  \n",
    "${a}", "${a}\n", "hello ${a} world\n", "#{a}\n", "#{${a}}\n",
    # directives
    "$$ var q = 1;\n${q}\n",
    "$$var q = 1;\n${q}\n",
    "$$   var q = 1;\n${q}\n",
    "$$ var q = 1;\n$$ q = q + 1;\n${q}\n",
    "$$ if (a) {\nyes ${a}\n$$ } else {\nno\n$$ }\n",
    "$$ for(q : [1,2,3]) {\n${q}\n$$ }\n",
    "$$ while(false) {\nnever\n$$ }\n",
    "$$ var q = 0;\n$$ do {\n${q}\n$$ q = q + 1;\n$$ } while (q < 3);\n",
    "$$ return 1;\nunreachable\n",
    "$$ var f = (q)->{ q * 2 };\n${f(3)}\n",
    "$$ var f = (q)->{\n${q}\n$$ };\n$$ f(1); f(2);\n",
    # jexl: functions and $jexl
    "$$ jexl:print(0);\n",
    "$$ jexl:print(-1);\n",
    "$$ jexl:print(99);\n",
    "$$ jexl:print('x');\n",
    "$$ jexl:nosuch();\n",
    "$$ jexl:include(1);\n",
    "${$jexl}abc",
    "abc${$jexl}",
    "$$ var w = $jexl;\n${w}\n",
    # comments and empty directives
    "$$\n${a}\n",
    "$$ // c\n${a}\n",
    "$$ /* c */\n${a}\n",
    "// not a directive\n${a}\n",
    # deferred in template (prepare path)
    "#{a}\n", "#{a + 1}\n", "#{${a}}\n", "x#{a}y${b}z\n",
    "$$ var q = 2;\n#{q}\n",
    "$$ var q = 2;\n#{${q}}\n",
    # errors
    "${\n", "#{\n", "${a\n", "$$ var\n", "$$ }\n", "$$ {\n",
    "${undefinedvar}\n", "${1/0}\n", "${nosuch()}\n",
    # unicode / surrogates
    "${'한'}\n", "\ud800${a}\udfff\n",
    # verbatim-only multi block
    "a\n$$ ;\nb\n$$ ;\nc\n",
]

# templates that take parameters
CORPUS_PARAMS = [
    ("${p0}\n", ["p0"]),
    ("${p0 + p1}\n", ["p0", "p1"]),
    ("$$ var q = p0;\n${q}\n", ["p0"]),
    ("#{p0}\n", ["p0"]),
    ("#{${p0}}\n", ["p0"]),
    ("${p0}${p1}${p0}\n", ["p0", "p1"]),
    ("$$ if (p0) {\n${p1}\n$$ }\n", ["p0", "p1"]),
    ("$$ var f = (q)->{ q + p0 };\n${f(1)}\n", ["p0"]),
]

# Pragma templates are emitted LAST, on purpose. `Engine.options(JexlContext)` hands out the
# engine's own mutable JexlOptions when the context is not an OptionsHandle, and processPragmas
# writes into it -- so in Java a `#pragma jexl.namespace.x` (or `jexl.options`) permanently
# rewrites the engine every later script on it sees. The oracle caches one engine per config, so
# these cases would change the expected output of everything that follows them.
CORPUS_PRAGMA = [
    "$$#pragma jexl.options +strict\n${a}\n",
    "$$ #pragma jexl.options -strict\n${a}\n",
    "$$ #pragma jexl.namespace.ns 'java.lang.Math'\n${a}\n",
    "$$ #pragma unknown.pragma 1\n${a}\n",
]

ENGINES = [
    None,
    {"strict": False},
    {"strict": True},
    {"silent": True},
    {"silent": True, "strict": False},
    {"safe": False},
    {"safe": False, "strict": True},
    {"lexical": True},
    {"lexical": True, "lexicalShade": True},
    {"antish": False},
    {"cache": "32"},
    {"cache": "32", "cacheThreshold": "4"},
    {"collectMode": "0"},
    {"collectMode": "2"},
    {"debug": False},
    {"arithmetic": {"strict": False}},
    {"arithmetic": {"strict": True, "mathContext": "DECIMAL64", "mathScale": "2"}},
    {"namespaces": {"ns": "ns"}},
]

OPS_JXLT = [
    ["vars", "parsed", "exec", "ctx"],
    ["parsed"],
    ["vars"],
    ["exec"],
    ["parsed", "exec"],
]

OPS_TEMPLATE = [
    ["vars", "params", "pragmas", "parsed", "exec", "ctx"],
    ["parsed"],
    ["vars", "params"],
    ["exec"],
    ["pragmas", "parsed", "exec"],
]


def context(r, names=CTX_NAMES, density=0.5):
    ctx = {}
    for name in names:
        if r.random() < density:
            ctx[name] = r.choice(CTX_VALUES)
    return ctx


def rand_jxlt(r):
    n = r.randint(1, 5)
    parts = []
    for _ in range(n):
        k = r.random()
        if k < 0.30:
            parts.append(r.choice(CONSTS))
        elif k < 0.60:
            parts.append(imm(r.choice(EXPRS)))
        elif k < 0.80:
            parts.append(dfr(r.choice(EXPRS)))
        elif k < 0.90:
            parts.append(nested(r.choice(EXPRS)))
        else:
            # a truncated piece: exercises the malformed paths
            parts.append(r.choice(["${", "#{", "${" + r.choice(EXPRS), "\\", "$", "#"]))
    return "".join(parts)


def rand_template(r):
    lines = []
    for _ in range(r.randint(1, 6)):
        k = r.random()
        if k < 0.45:
            lines.append(rand_jxlt(r).replace("\n", " ") + "\n")
        elif k < 0.80:
            lines.append("$$ " + r.choice([
                "var q = 1;", "q = 2;", "a = 1;", ";", "// c", "/* c */",
                "if (a) {", "}", "for(q : [1,2]) {", "while(false) {",
                "jexl:print(0);", "jexl:print(1);", "var f = (w)->{ w };",
                "return 1;", "continue;", "break;",
            ]) + "\n")
        else:
            lines.append(r.choice(CORPUS_TEMPLATE))
    return "".join(lines)


def main():
    total = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 20250930
    r = random.Random(seed)
    out = sys.stdout
    n = 0

    def emit(case):
        nonlocal n
        case["id"] = "j%d" % n
        out.write(json.dumps(case) + "\n")
        n += 1

    # 1. the hand-written corpus, each against every engine config
    for src in CORPUS_JXLT:
        for eng in ENGINES:
            c = {"kind": "jxlt", "src": src, "ctx": context(r), "ops": ["vars", "parsed", "exec", "ctx"]}
            if eng is not None:
                c["engine"] = eng
            emit(c)
    for src in CORPUS_TEMPLATE:
        for eng in ENGINES:
            c = {
                "kind": "template",
                "src": src,
                "ctx": context(r),
                "ops": ["vars", "params", "pragmas", "parsed", "exec", "ctx"],
            }
            if eng is not None:
                c["engine"] = eng
            emit(c)
    for src, parms in CORPUS_PARAMS:
        for eng in ENGINES:
            c = {
                "kind": "template",
                "src": src,
                "params": parms,
                "args": [r.choice(CTX_VALUES) for _ in parms],
                "ctx": context(r),
                "ops": ["vars", "params", "pragmas", "parsed", "exec", "ctx"],
            }
            if eng is not None:
                c["engine"] = eng
            emit(c)
    # the same corpus with no arguments at all (unbound parameters)
    for src, parms in CORPUS_PARAMS:
        emit({
            "kind": "template", "src": src, "params": parms, "ctx": context(r),
            "ops": ["vars", "params", "parsed", "exec", "ctx"],
        })

    # 2. random compositions
    while n < total - len(CORPUS_PRAGMA) * len(ENGINES):
        if r.random() < 0.5:
            c = {"kind": "jxlt", "src": rand_jxlt(r), "ctx": context(r), "ops": r.choice(OPS_JXLT)}
        else:
            c = {"kind": "template", "src": rand_template(r), "ctx": context(r), "ops": r.choice(OPS_TEMPLATE)}
            if r.random() < 0.25:
                parms = ["p0", "p1"][: r.randint(1, 2)]
                c["params"] = parms
                if r.random() < 0.8:
                    c["args"] = [r.choice(CTX_VALUES) for _ in parms]
        eng = r.choice(ENGINES)
        if eng is not None:
            c["engine"] = eng
        emit(c)


    # 3. the pragma corpus, last (see CORPUS_PRAGMA)
    for src in CORPUS_PRAGMA:
        for eng in ENGINES:
            c = {
                "kind": "template",
                "src": src,
                "ctx": context(r),
                "ops": ["vars", "params", "pragmas", "parsed", "exec", "ctx"],
            }
            if eng is not None:
                c["engine"] = eng
            emit(c)


if __name__ == "__main__":
    main()
