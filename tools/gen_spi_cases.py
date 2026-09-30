#!/usr/bin/env python3
"""Generates deterministic `kind:"spi"` cases for the JDK-shim uberspect.

    python3 tools/gen_spi_cases.py [COUNT] [SEED] > cases.jsonl

Covers, over a value pool with one entry per Java type the oracle can encode (plus null,
boundary numbers, Korean/emoji text, empty and populated collections):
  * every method the shim models, called with sensible arguments AND with random arguments
    drawn from the whole pool (so wrong-arity / wrong-type failure paths are covered too),
  * a few names the shim models nowhere (so "unsolvable method" is covered),
  * property get/set through every resolver (PROPERTY / MAP / LIST / DUCK / FIELD / CONTAINER),
    by bean name, by index, by bracket ('[x]' switches the engine to the ARRAY_GET strategy),
  * iteration of every pool value,
  * construction of every modeled class plus a few unmodeled ones.
"""
import json
import random
import sys

# ----------------------------------------------------------------- typed values


def V(t, v=None, c=None, **kw):
    d = {"t": t}
    if v is not None:
        d["v"] = v
    if c is not None:
        d["c"] = c
    d.update(kw)
    return d


NULL = V("null")
B = lambda b: V("Boolean", "true" if b else "false")
BY = lambda n: V("Byte", str(n))
SH = lambda n: V("Short", str(n))
IN = lambda n: V("Integer", str(n))
LO = lambda n: V("Long", str(n))
FL = lambda s, bits=None: V("Float", s, bits=bits) if bits else V("Float", s)
DO = lambda s, bits=None: V("Double", s, bits=bits) if bits else V("Double", s)
CH = lambda c: V("Character", c)
ST = lambda s: V("String", s)
BI = lambda s: V("BigInteger", s)
BD = lambda s: V("BigDecimal", s)
LI = lambda *e: V("List", list(e))
LL = lambda *e: V("List", list(e), c="java.util.LinkedList")
SE = lambda *e: V("Set", list(e), c="java.util.LinkedHashSet")
HS = lambda *e: V("Set", list(e), c="java.util.HashSet")
MA = lambda *kv: V("Map", [list(x) for x in kv], c="java.util.LinkedHashMap")
HM = lambda *kv: V("Map", [list(x) for x in kv], c="java.util.HashMap")
AR = lambda comp, *e: V("Array", list(e), c=comp)

# ----------------------------------------------------------------- pools

# arguments the generator throws at every method (wrong types included)
ARGS = [
    NULL, B(True), B(False), BY(1), BY(-128), SH(2), IN(0), IN(1), IN(-1), IN(3), IN(2147483647),
    IN(-2147483648), LO(4), LO(-9223372036854775808), FL("1.5"), DO("2.5"), DO("NaN"), DO("0.1"),
    CH("a"), CH("5"), CH("가"), ST(""), ST("a"), ST("Hello"), ST("l"), ST("0"), ST("h."),
    ST("한글"), ST("😀"), ST("a,b"), BI("7"), BI("0"), BD("1.5"), BD("0"),
    LI(IN(1), IN(2)), LI(), MA((ST("a"), IN(1))), SE(IN(1)), AR("int", IN(1), IN(2)),
    AR("Object", ST("a")),
]

# the receivers: one per Java type the protocol can encode
TARGETS = {
    "String": [ST(""), ST("Hello"), ST("a,b"), ST("한글 😀"), ST("  hi "), ST("\ud83d"),
               ST("😀")],
    "Character": [CH("a"), CH("5"), CH("가"), CH(" ")],
    "Boolean": [B(True), B(False)],
    "Byte": [BY(1), BY(-128)],
    "Short": [SH(2), SH(-32768)],
    "Integer": [IN(0), IN(5), IN(-2147483648), IN(2147483647)],
    "Long": [LO(5), LO(-9223372036854775808)],
    "Float": [FL("1.5"), FL("NaN")],
    "Double": [DO("2.5"), DO("NaN"), DO("Infinity")],
    "BigInteger": [BI("12"), BI("0"), BI("-170141183460469231731687303715884105728")],
    "BigDecimal": [BD("1.50"), BD("0"), BD("-3.14159")],
    "List": [LI(IN(1), IN(2)), LI(), LL(ST("a"), NULL)],
    "Set": [SE(IN(1), IN(2)), HS(ST("a"))],
    "Map": [MA((ST("a"), IN(1))), HM((IN(0), ST("z"))), MA()],
    "Array": [AR("int", IN(1), IN(2)), AR("Object", ST("a"), NULL), AR("String", ST("x")),
              AR("long", LO(1)), AR("double", DO("1.5")), AR("boolean", B(True)), AR("char", CH("a")),
              AR("byte", BY(1)), AR("short", SH(2)), AR("float", FL("1.5")), AR("Integer", IN(1)),
              AR("Number", IN(1))],
}

# ----------------------------------------------------------------- modeled methods
#
# name -> the argument tuples that exercise it meaningfully; the generator adds random
# tuples from ARGS on top of these.

OBJECT = {
    "toString": [()],
    "hashCode": [()],
    "equals": [(ST("Hello"),), (NULL,), (IN(5),)],
    "getClass": [()],
}

STRING = {
    "length": [()], "isEmpty": [()], "isBlank": [()],
    "charAt": [(IN(0),), (IN(9),), (IN(-1),)],
    "codePointAt": [(IN(0),)],
    "indexOf": [(ST("l"),), (ST("l"), IN(3)), (IN(108),), (IN(108), IN(3)), (IN(128512),),
                (IN(128512), IN(0)), (IN(-1),), (IN(1114112),)],
    "lastIndexOf": [(ST("l"),), (ST("l"), IN(3)), (IN(108),), (IN(108), IN(3)), (IN(128512),),
                    (IN(128512), IN(4))],
    "substring": [(IN(1),), (IN(1), IN(3)), (IN(9),), (IN(3), IN(1))],
    "subSequence": [(IN(0), IN(1))],
    "concat": [(ST("x"),)],
    "contains": [(ST("l"),)],
    "startsWith": [(ST("H"),), (ST("e"), IN(1))],
    "endsWith": [(ST("o"),)],
    "equalsIgnoreCase": [(ST("HELLO"),), (ST("\ud83d\ude00"),), (ST("\ud801\udc28"),)],
    "compareTo": [(ST("Hello"),), (ST("a"),)],
    "compareToIgnoreCase": [(ST("HELLO"),), (ST("\ud83d\ude00"),), (ST("\ud83d"),), (ST("stra\u00dfe"),),
                            (ST("\ud83d\udc00"),), (ST("\ud83d\ude00x"),)],
    "toUpperCase": [()], "toLowerCase": [()],
    "trim": [()], "strip": [()], "stripLeading": [()], "stripTrailing": [()],
    "replace": [(CH("l"), CH("L")), (ST("l"), ST("L")), (ST(""), ST("-")), (ST("Hello"), ST(""))],
    "replaceAll": [(ST("l"), ST("L")), (ST("(l)"), ST("[$1]")), (ST("(l)"), ST("$9")), (ST("["), ST("x"))],
    "replaceFirst": [(ST("l"), ST("L"))],
    "matches": [(ST("h."),), (ST("H.*"),)],
    "split": [(ST("l"),), (ST("l"), IN(2))],
    "toCharArray": [()],
    "repeat": [(IN(2),), (IN(-1),), (IN(0),), (IN(2147483647),)],
    "intern": [()],
    "regionMatches": [(IN(0), ST("Hello"), IN(0), IN(2)), (B(True), IN(0), ST("HE"), IN(0), IN(2)),
                      (B(True), IN(0), ST("\ud83d\ude00"), IN(0), IN(2)), (IN(0), ST("x"), IN(0), IN(9))],
    "valueOf": [(IN(3),), (LO(3),), (DO("1.5"),), (B(True),), (CH("a"),), (NULL,), (ST("x"),)],
    "join": [(ST("-"), ST("a"), ST("b")), (ST("-"),), (ST("-"), ST("a")), (ST("-"), LI(ST("a"), ST("b"))),
             (ST("-"), LI()), (NULL, LI(ST("a")))],
}

NUMBER = {
    "intValue": [()], "longValue": [()], "doubleValue": [()], "floatValue": [()],
    "byteValue": [()], "shortValue": [()],
    # Comparable.compareTo(Object) is a bridge: the same call either compares or throws a CCE
    "compareTo": [(BY(1),), (SH(2),), (IN(3),), (LO(3),), (FL("1"),), (DO("1"),), (BI("1"),), (BD("1"),),
                  (CH("a"),), (B(True),), (ST("a"),), (NULL,)],
}

INTEGRAL = {
    "compareTo": [(IN(3),), (LO(3),)],
    "parseInt": [(ST("42"),), (ST("x"),), (ST("ff"), IN(16))],
    "parseLong": [(ST("42"),), (ST("x"),)],
    "parseShort": [(ST("42"),)], "parseByte": [(ST("42"),)],
    "valueOf": [(ST("42"),), (ST("ff"), IN(16)), (IN(7),), (LO(7),)],
    "toString": [(IN(255),), (IN(255), IN(16)), (LO(255),)],
    "toHexString": [(IN(255),)], "toOctalString": [(IN(255),)], "toBinaryString": [(IN(5),)],
    "compare": [(IN(1), IN(2))], "max": [(IN(1), IN(2))], "min": [(IN(1), IN(2))],
    "sum": [(IN(1), IN(2))], "signum": [(IN(-3),)], "bitCount": [(IN(7),)],
}

FLOATING = {
    "isNaN": [(), (DO("NaN"),), (FL("NaN"),)], "isInfinite": [(), (DO("1"),), (FL("1"),)],
    "isFinite": [(DO("1"),), (FL("1"),)],
    "compareTo": [(DO("1.5"),), (FL("1.5"),), (FL("NaN"),), (DO("NaN"),)],
    "floatToRawIntBits": [(FL("NaN"),)],
    "parseDouble": [(ST("4.2"),), (ST("x"),)], "parseFloat": [(ST("4.2"),)],
    "valueOf": [(ST("4.2"),), (DO("1.5"),)],
    "toString": [(DO("1.5"),)],
    "compare": [(DO("1"), DO("2")), (FL("1"), FL("2")), (FL("NaN"), FL("1"))],
    "max": [(DO("1"), DO("2")), (DO("NaN"), DO("1")), (DO("1"), DO("NaN")), (DO("-0.0"), DO("0.0")),
            (DO("0.0"), DO("-0.0")), (FL("1"), FL("2")), (FL("NaN"), FL("1")), (FL("-0.0"), FL("0.0"))],
    "min": [(DO("1"), DO("2")), (DO("NaN"), DO("1")), (DO("1"), DO("NaN")), (DO("-0.0"), DO("0.0")),
            (DO("0.0"), DO("-0.0")), (FL("1"), FL("2")), (FL("NaN"), FL("1")), (FL("-0.0"), FL("0.0"))],
    "sum": [(DO("1"), DO("2")), (FL("1"), FL("2"))],
    "doubleToLongBits": [(DO("1.5"),)], "doubleToRawLongBits": [(DO("NaN"),)],
    "longBitsToDouble": [(LO(4609434218613702656),)],
    "floatToIntBits": [(FL("1.5"),)], "intBitsToFloat": [(IN(1069547520),)],
}

CHARACTER = {
    "charValue": [()], "compareTo": [(CH("b"),)],
    "isDigit": [(CH("5"),), (CH("a"),)], "isLetter": [(CH("a"),), (CH("5"),)],
    "isLetterOrDigit": [(CH("a"),), (IN(97),), (IN(53),), (IN(32),)], "isWhitespace": [(CH(" "),)], "isSpaceChar": [(CH(" "),)],
    "isUpperCase": [(CH("A"),)], "isLowerCase": [(CH("a"),)],
    "toUpperCase": [(CH("a"),)], "toLowerCase": [(CH("A"),)],
    "getNumericValue": [(CH("a"),), (CH("5"),)],
    "valueOf": [(CH("a"),)], "toString": [(CH("a"),), (IN(128512),), (IN(97),), (IN(-1),)],
    "compare": [(CH("a"), CH("b"))], "digit": [(CH("f"), IN(16))], "forDigit": [(IN(15), IN(16))],
}

BOOLEAN = {
    "booleanValue": [()], "compareTo": [(B(True),)],
    "parseBoolean": [(ST("TRUE"),), (ST("x"),)],
    "valueOf": [(ST("true"),), (B(True),)],
    "toString": [(B(True),)],
    "compare": [(B(True), B(False))],
    "logicalAnd": [(B(True), B(False))], "logicalOr": [(B(True), B(False))],
    "logicalXor": [(B(True), B(False))],
}

BIGINTEGER = {
    "add": [(BI("3"),)], "subtract": [(BI("3"),)], "multiply": [(BI("3"),)],
    "divide": [(BI("3"),), (BI("0"),)], "mod": [(BI("5"),), (BI("0"),), (BI("-5"),)],
    "remainder": [(BI("5"),), (BI("0"),)],
    "pow": [(IN(3),), (IN(-1),), (IN(2147483647),)], "negate": [()], "abs": [()], "gcd": [(BI("8"),)],
    "signum": [()], "compareTo": [(BI("3"),)], "min": [(BI("3"),)], "max": [(BI("3"),)],
    "shiftLeft": [(IN(3),), (IN(2147483647),)], "shiftRight": [(IN(3),), (IN(-2147483648),)],
    "and": [(BI("6"),)], "or": [(BI("6"),)], "xor": [(BI("6"),)], "not": [()],
    "testBit": [(IN(2),), (IN(-1),)], "bitLength": [()], "bitCount": [()],
    "toString": [(), (IN(16),)], "valueOf": [(LO(7),)],
}

BIGDECIMAL = {
    "add": [(BD("2.5"),)], "subtract": [(BD("2.5"),)], "multiply": [(BD("2.5"),)],
    "divide": [(BD("3"),), (BD("0"),), (BD("7"),), (BD("7"), IN(4)), (BD("7"), IN(2), IN(4)), (BD("7"), IN(9))],
    "plus": [()],
    "remainder": [(BD("0.4"),), (BD("0"),)],
    "negate": [()], "abs": [()], "pow": [(IN(2),), (IN(-1),)],
    "scale": [()], "precision": [()], "signum": [()], "unscaledValue": [()],
    "stripTrailingZeros": [()], "toPlainString": [()], "toBigInteger": [()],
    "movePointLeft": [(IN(1),)], "movePointRight": [(IN(1),)],
    "setScale": [(IN(0),), (IN(4),), (IN(0), IN(4)), (IN(0), IN(7)), (IN(0), IN(99)), (IN(1), IN(2)),
                 (IN(1), IN(3)), (IN(1), IN(0)), (IN(1), IN(1)), (IN(1), IN(5)), (IN(1), IN(6))],
    "compareTo": [(BD("1.5"),)], "min": [(BD("1"),)], "max": [(BD("1"),)], "ulp": [()],
    "toString": [()], "valueOf": [(LO(7),), (DO("0.1"),)],
}

LIST = {
    "size": [()], "isEmpty": [()],
    "get": [(IN(0),), (IN(5),), (IN(-1),)],
    "set": [(IN(0), ST("z")), (IN(5), ST("z"))],
    "add": [(IN(9),), (ST("z"),), (NULL,), (IN(0), ST("z")), (IN(9), ST("z"))],
    "remove": [(IN(0),), (IN(9),), (ST("z"),), (ST("a"),), (NULL,)],
    "clear": [()], "contains": [(IN(1),), (NULL,)],
    "indexOf": [(IN(2),), (NULL,)], "lastIndexOf": [(IN(2),)],
    "addAll": [(LI(IN(3)),), (IN(0), LI(IN(3)))],
    "containsAll": [(LI(IN(1)),)], "removeAll": [(LI(IN(1)),)], "retainAll": [(LI(IN(1)),)],
    "toArray": [(), (AR("Object", ST("a")),), (AR("Object", ST("a"), ST("b"), ST("c")),)], "iterator": [()],
    "removeFirst": [()], "removeLast": [()], "getFirst": [()], "getLast": [()],
    "addFirst": [(IN(9),)], "addLast": [(IN(9),)],
}

SET = {
    "size": [()], "isEmpty": [()], "add": [(IN(9),), (NULL,)], "remove": [(IN(1),)],
    "contains": [(IN(1),), (NULL,)], "clear": [()],
    "addAll": [(LI(IN(3)),)], "containsAll": [(LI(IN(1)),)],
    "removeAll": [(LI(IN(1)),)], "retainAll": [(LI(IN(1)),)],
    "toArray": [()], "iterator": [()],
}

MAP = {
    "size": [()], "isEmpty": [()],
    "get": [(ST("a"),), (ST("z"),), (NULL,), (IN(0),)],
    "put": [(ST("b"), IN(2)), (NULL, NULL), (IN(1), ST("x"))],
    "remove": [(ST("a"),), (ST("z"),), (ST("a"), IN(1)), (ST("a"), IN(9))],
    "containsKey": [(ST("a"),)], "containsValue": [(IN(1),)],
    "clear": [()], "putAll": [(MA((ST("q"), IN(5))),)],
    "getOrDefault": [(ST("z"), IN(0))], "putIfAbsent": [(ST("z"), IN(0)), (ST("a"), IN(0))],
}

# only these five are declared by ArrayListWrapper, so only these work on an array;
# the others resolve but blow up inside reflection - both are covered.
ARRAY = {
    "size": [()], "get": [(IN(0),), (IN(5),)], "set": [(IN(0), IN(4)), (IN(0), ST("z"))],
    "indexOf": [(IN(1),), (NULL,)], "contains": [(IN(1),)],
    "isEmpty": [()], "add": [(IN(1),)], "clear": [()], "iterator": [()], "toArray": [()],
    "lastIndexOf": [(IN(1),)], "addAll": [(LI(IN(1)),)], "containsAll": [(LI(IN(1)),)],
    "removeAll": [(LI(IN(1)),)], "retainAll": [(LI(IN(1)),)], "remove": [(IN(0),)],
}

UNSOLVABLE = {"noSuchMethod": [(), (IN(1),)], "get": [(B(True), B(True), B(True))]}


# java.util.regex indexes UTF-16 units; the Rust port indexes code points, so a zero-width match
# inside a surrogate pair diverges (see COMPATIBILITY.md). Those combinations are left out.
REGEX_METHODS = {"matches", "replaceAll", "replaceFirst", "split"}


def has_pair(spec):
    if not isinstance(spec, dict):
        return False
    v = spec.get("v")
    if isinstance(v, str):
        # python keeps astral characters as one code point; json.dumps re-emits them as a pair
        return any(ord(c) > 0xFFFF for c in v)
    if isinstance(v, list):
        return any(has_pair(e) for e in v)
    return False


def merged(*tables):
    out = {}
    for t in tables:
        for k, v in t.items():
            out.setdefault(k, [])
            out[k] += v
    return out


METHODS = {
    "String": merged(OBJECT, STRING, UNSOLVABLE),
    "Character": merged(OBJECT, CHARACTER, UNSOLVABLE),
    "Boolean": merged(OBJECT, BOOLEAN, UNSOLVABLE),
    "Byte": merged(OBJECT, NUMBER, INTEGRAL, UNSOLVABLE),
    "Short": merged(OBJECT, NUMBER, INTEGRAL, UNSOLVABLE),
    "Integer": merged(OBJECT, NUMBER, INTEGRAL, UNSOLVABLE),
    "Long": merged(OBJECT, NUMBER, INTEGRAL, UNSOLVABLE),
    "Float": merged(OBJECT, NUMBER, FLOATING, UNSOLVABLE),
    "Double": merged(OBJECT, NUMBER, FLOATING, UNSOLVABLE),
    "BigInteger": merged(OBJECT, NUMBER, BIGINTEGER, UNSOLVABLE),
    "BigDecimal": merged(OBJECT, NUMBER, BIGDECIMAL, UNSOLVABLE),
    "List": merged(OBJECT, LIST, UNSOLVABLE),
    "Set": merged(OBJECT, SET, UNSOLVABLE),
    "Map": merged(OBJECT, MAP, UNSOLVABLE),
    # an array's identity hashCode is nondeterministic in Java, so it is not comparable
    "Array": merged({k: v for k, v in OBJECT.items() if k != "hashCode"}, ARRAY, UNSOLVABLE),
}

# ----------------------------------------------------------------- properties

# bean-style names (PROPERTY / BooleanGet), map keys (MAP / DUCK), indices (LIST),
# public static fields (FIELD) and names nothing resolves.
PROPERTY_NAMES = [
    "class", "empty", "blank", "a", "b", "z", "0", "1", "5", "['a']", "['z']", "[0]", "[1]",
    "['empty']", "['class']", "MAX_VALUE", "MIN_VALUE", "TRUE", "FALSE", "ZERO", "ONE", "TEN",
    "TYPE", "SIZE", "BYTES", "POSITIVE_INFINITY", "noSuchProperty",
]
SET_VALUES = [IN(9), ST("z"), NULL, DO("1.5"), LI(IN(1)), BY(1), SH(2), CH("a"), B(True), LO(3), FL("1.5")]

CTORS = [
    "java.util.ArrayList", "java.util.LinkedList", "java.util.HashMap", "java.util.LinkedHashMap",
    "java.util.HashSet", "java.util.LinkedHashSet", "java.lang.StringBuilder", "java.lang.String",
    "java.lang.Integer", "java.lang.Long", "java.lang.Short", "java.lang.Byte", "java.lang.Double",
    "java.lang.Float", "java.lang.Boolean", "java.lang.Character", "java.math.BigInteger",
    "java.math.BigDecimal", "java.lang.Math", "java.util.NoSuchClass", "java.lang.Object",
]
CTOR_ARGS = [
    (), (IN(3),), (ST("42"),), (ST("x"),), (LI(IN(1), IN(2)),), (MA((ST("a"), IN(1))),),
    (CH("a"),), (B(True),), (DO("1.5"),), (LO(3),), (NULL,), (IN(1), IN(2)),
    (AR("char", CH("a"), CH("b")),), (AR("byte", BY(65)),), (SE(IN(1)),), (BI("7"),),
]


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 20250930
    rng = random.Random(seed)
    cases = []

    def emit(**kw):
        cases.append(kw)

    # invoke: every modeled method x every receiver of its class, with the table's argument
    # tuples plus random ones (wrong arity / wrong type).
    for cls, targets in TARGETS.items():
        table = METHODS[cls]
        for target in targets:
            for name in sorted(table):
                tuples = list(table[name])
                for arity in (0, 1, 2):
                    tuples.append(tuple(rng.choice(ARGS) for _ in range(arity)))
                for args in tuples:
                    if name in REGEX_METHODS and (has_pair(target) or any(has_pair(a) for a in args)):
                        continue
                    emit(op="invoke", target=target, name=name, args=list(args))

    # get / set: every name x every receiver
    for cls, targets in TARGETS.items():
        for target in targets:
            for name in PROPERTY_NAMES:
                emit(op="get", target=target, name=name)
                emit(op="set", target=target, name=name, args=[rng.choice(SET_VALUES)])

    # iterate
    for targets in TARGETS.values():
        for target in targets:
            emit(op="iterate", target=target)
    emit(op="iterate", target=NULL)

    # construct
    for cname in CTORS:
        for args in CTOR_ARGS:
            emit(op="construct", name=cname, args=list(args))

    # a null receiver: only invoke/iterate are in the shim's scope (get/set on null is an
    # interpreter concern, see COMPATIBILITY.md)
    for name in ("toString", "hashCode", "size"):
        emit(op="invoke", target=NULL, name=name, args=[])

    rng.shuffle(cases)
    if count:
        while len(cases) < count:
            cases.append(dict(cases[rng.randrange(len(cases))]))
        cases = cases[:count]
    out = sys.stdout
    for i, c in enumerate(cases):
        c = dict(kind="spi", id="s%d" % i, **c)
        out.write(json.dumps(c) + "\n")


if __name__ == "__main__":
    main()
