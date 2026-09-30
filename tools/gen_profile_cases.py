#!/usr/bin/env python3
"""Generate the consumer-profile suite: the expression shapes the primary consumer runs, with
neutral names, under its engine configuration.

The profile (see the project brief):
  engine  : namespaces {"ns": obj}, strict(true), cache(4096), cacheThreshold(256)
  usage   : one createScript per expression, then getVariables(); each ant-ish path is bound in a
            MapContext under its dotted name, and the script is executed
  values  : include a host object that stands for "JSON null" (non-null, so `x == null` is false)
  shapes  : dotted ant-ish paths; == != < > <= >= && || ! + - * and parentheses; string literals in
            '...', "..." and backticks (with and without ${}) containing Korean; integer, boolean
            and null literals; ns:fn(args) calls including varargs; typo assignments (=!, =) mixed
            into && / ||.

Usage: gen_profile_cases.py N SEED > cases.jsonl
"""
import json
import random
import sys

# neutral stand-ins for the production event/field names
EVENTS = ["EventA", "EventB", "Flow_C", "Log_D", "NW_E", "TLog_F", "Char_G", "User_H"]
FIELDS = ["field1", "value", "level", "step", "code", "name", "region", "kind", "count", "flag",
          "t_id", "t_level", "arg0", "platform", "zone", "world"]
KOREAN = ["별+", "별-", "하늘+", "하늘-", "파란하늘+", "파란하늘-", "초록색하늘+", "한글", "가나다"]
PLAIN = ["RegionA", "RegionB", "kiwi", "mango", "marketx", "true", "false", "1", "0",
         "test", "a,b", "ABC"]
NUMBERS = ["0", "1", "2", "3", "4", "5", "6", "9", "16", "32", "100", "400", "500", "600", "1000",
           "2147483647", "-1"]
NSFUNCS = [("isNull", 1), ("castToInt", 1), ("joinWithPipe", 3), ("isIpv4", 1), ("isIpv6", 1),
           ("getIpVersion", 1), ("size", 3), ("nvl", 2)]

CMP = ["==", "!=", "<", ">", "<=", ">="]
BOOL = ["&&", "||"]


def path(rnd):
    """A dotted ant-ish path, or the bare `input`."""
    if rnd.random() < 0.25:
        return "input"
    n = rnd.randint(1, 3)
    parts = [rnd.choice(EVENTS)] + [rnd.choice(FIELDS) for _ in range(n - 1)]
    return ".".join(parts)


def literal(rnd):
    k = rnd.random()
    if k < 0.35:
        return rnd.choice(NUMBERS)
    if k < 0.45:
        return rnd.choice(["true", "false", "null"])
    text = rnd.choice(KOREAN + PLAIN)
    q = rnd.random()
    if q < 0.45:
        return "'%s'" % text
    if q < 0.8:
        return '"%s"' % text
    if q < 0.92:
        return "`%s`" % text
    return "`${%s}%s`" % (path(rnd), text)


def operand(rnd, depth):
    k = rnd.random()
    if k < 0.45:
        return path(rnd)
    if k < 0.75:
        return literal(rnd)
    if k < 0.85 and depth > 0:
        name, argc = rnd.choice(NSFUNCS)
        args = ", ".join(operand(rnd, depth - 1) for _ in range(argc))
        return "ns:%s(%s)" % (name, args)
    if k < 0.93 and depth > 0:
        return "(%s)" % expr(rnd, depth - 1)
    return "%s %s %s" % (operand(rnd, 0), rnd.choice(["+", "-", "*"]), operand(rnd, 0))


def comparison(rnd, depth):
    if rnd.random() < 0.08:
        # the production typo: `a.b =! 0` parses as the assignment `a.b = !0`
        return "%s %s %s" % (path(rnd), rnd.choice(["=!", "="]), operand(rnd, 0))
    if rnd.random() < 0.08:
        return "!%s" % operand(rnd, max(depth - 1, 0))
    return "%s %s %s" % (operand(rnd, depth), rnd.choice(CMP), operand(rnd, depth))


def expr(rnd, depth):
    n = rnd.randint(1, 4 if depth > 0 else 1)
    parts = [comparison(rnd, depth)]
    for _ in range(n - 1):
        parts.append(rnd.choice(BOOL))
        parts.append(comparison(rnd, depth))
    return " ".join(parts)


CTX_VALUES = [
    {"t": "null"},
    {"t": "Host", "v": "jsonNull"},
    {"t": "Boolean", "v": "true"}, {"t": "Boolean", "v": "false"},
    {"t": "Integer", "v": "0"}, {"t": "Integer", "v": "1"}, {"t": "Integer", "v": "16"},
    {"t": "Integer", "v": "600"}, {"t": "Long", "v": "9223372036854775807"},
    {"t": "Double", "v": "1.5"},
    {"t": "String", "v": ""}, {"t": "String", "v": "1"}, {"t": "String", "v": "0"},
    {"t": "String", "v": "true"}, {"t": "String", "v": "false"},
    {"t": "String", "v": "RegionA"}, {"t": "String", "v": "kiwi"},
    {"t": "String", "v": "별+"}, {"t": "String", "v": "하늘-"}, {"t": "String", "v": "한글"},
    {"t": "String", "v": "10.1.2.3"}, {"t": "String", "v": "::1"},
]


def main():
    n, seed = int(sys.argv[1]), int(sys.argv[2])
    rnd = random.Random(seed)
    for i in range(n):
        src = expr(rnd, 2)
        # bind every dotted path the expression mentions, plus a few it does not
        names = set()
        token = ""
        for ch in src + " ":
            if ch.isalnum() or ch in "._":
                token += ch
            else:
                if token and not token[0].isdigit() and token not in ("true", "false", "null", "ns"):
                    names.add(token)
                token = ""
        ctx = {}
        for name in sorted(names):
            if rnd.random() < 0.85:
                ctx[name] = rnd.choice(CTX_VALUES)
        print(json.dumps({
            "id": "p%d_%d" % (seed, i),
            "kind": "script",
            "src": src,
            "engine": {"namespaces": {"ns": "ns"}, "strict": True, "cache": "4096", "cacheThreshold": "256"},
            "ctx": ctx,
            "ops": ["vars", "exec", "ctx"],
        }))


if __name__ == "__main__":
    main()
