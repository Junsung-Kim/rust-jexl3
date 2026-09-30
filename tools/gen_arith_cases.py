#!/usr/bin/env python3
"""Generate JexlArithmetic differential cases: every method against a value pool that covers each
Java type, null, boundary numbers, numeric strings, Korean/emoji text and the collection kinds.

Usage: gen_arith_cases.py N SEED > cases.jsonl
"""
import json
import random
import sys

UNARY = ["negate", "positivize", "complement", "not", "empty", "isEmpty", "size", "toBoolean",
         "toInteger", "toLong", "toDouble", "toBigInteger", "toBigDecimal", "toString", "narrow",
         "isFloatingPointNumber", "isNumberable", "isFloatingPoint"]
BINARY = ["add", "subtract", "multiply", "divide", "mod", "and", "or", "xor", "equals", "lessThan",
          "greaterThan", "lessThanOrEqual", "greaterThanOrEqual", "contains", "startsWith",
          "endsWith", "createRange"]

INTS = [0, 1, -1, 2, 3, 7, 10, 127, -128, 128, 255, 32767, -32768, 65535,
        2147483647, -2147483648, 1000000, -1000000]
LONGS = [0, 1, -1, 2147483648, -2147483649, 9223372036854775807, -9223372036854775808,
         4294967296, 1000000000000]
DOUBLES = ["0.0", "-0.0", "1.0", "-1.0", "0.5", "1.5", "2.5", "-2.5", "3.14159", "1.0E10", "1.0E-5",
           "1.0E308", "1.0E-308", "4.9E-324", "1.7976931348623157E308", "NaN", "Infinity", "-Infinity",
           "100.0", "0.1", "1.0E-3", "9.999999E-4"]
FLOATS = ["0.0", "-0.0", "1.0", "1.5", "3.4028235E38", "1.4E-45", "NaN", "Infinity", "-Infinity", "0.1"]
BIGINTS = ["0", "1", "-1", "9223372036854775808", "-9223372036854775809", "123456789012345678901234567890",
           "2147483648", "-2147483649"]
BIGDECS = ["0", "0.0", "1", "1.0", "-1.5", "0.1", "1E+10", "1E-10", "123456789.123456789",
           "1.00", "3.141592653589793238462643383279", "-0.000001"]
STRINGS = ["", " ", "0", "1", "-1", "2", "10", "1.5", "1e3", " 1 ", "0x1F", "007", "true", "false",
           "null", "NaN", "Infinity", "abc", "ABC", "a", "한글", "별+", "😀", "a😀b", "2147483648",
           "9223372036854775808", "1.0E10", "+5", "-", ".", "1_000"]
CHARS = ["a", "0", "9", "한", "\ud83d"]


def val(rnd):
    k = rnd.randint(0, 15)
    if k == 0:
        return {"t": "null"}
    if k == 1:
        return {"t": "Boolean", "v": rnd.choice(["true", "false"])}
    if k == 2:
        return {"t": "Integer", "v": str(rnd.choice(INTS))}
    if k == 3:
        return {"t": "Long", "v": str(rnd.choice(LONGS + INTS))}
    if k == 4:
        return {"t": "Byte", "v": str(rnd.randint(-128, 127))}
    if k == 5:
        return {"t": "Short", "v": str(rnd.randint(-32768, 32767))}
    if k == 6:
        return {"t": "Double", "v": rnd.choice(DOUBLES)}
    if k == 7:
        return {"t": "Float", "v": rnd.choice(FLOATS)}
    if k == 8:
        return {"t": "BigInteger", "v": rnd.choice(BIGINTS)}
    if k == 9:
        return {"t": "BigDecimal", "v": rnd.choice(BIGDECS)}
    if k == 10:
        return {"t": "Character", "v": rnd.choice(CHARS)}
    if k in (11, 12, 13):
        return {"t": "String", "v": rnd.choice(STRINGS)}
    if k == 14:
        n = rnd.randint(0, 3)
        kind = rnd.choice(["List", "Set", "Array"])
        items = [val(rnd) for _ in range(n)]
        if kind == "Array":
            return {"t": "Array", "c": "Object", "v": items}
        return {"t": kind, "v": items, "c": "java.util.ArrayList" if kind == "List" else "java.util.LinkedHashSet"}
    n = rnd.randint(0, 2)
    return {"t": "Map", "c": "java.util.LinkedHashMap", "v": [[val(rnd), val(rnd)] for _ in range(n)]}


def arith_conf(rnd):
    c = {}
    if rnd.random() < 0.25:
        c["strict"] = rnd.random() < 0.5
    if rnd.random() < 0.15:
        c["mathContext"] = rnd.choice(["DECIMAL32", "DECIMAL64", "DECIMAL128", "UNLIMITED", "5:HALF_UP", "3:FLOOR"])
    if rnd.random() < 0.15:
        c["mathScale"] = str(rnd.randint(0, 6))
    return c


def main():
    n, seed = int(sys.argv[1]), int(sys.argv[2])
    rnd = random.Random(seed)
    for i in range(n):
        unary = rnd.random() < 0.35
        op = rnd.choice(UNARY if unary else BINARY)
        args = [val(rnd)] if unary else [val(rnd), val(rnd)]
        if op == "narrow":
            # narrow takes a Number
            args = [rnd.choice([
                {"t": "Integer", "v": str(rnd.choice(INTS))},
                {"t": "Long", "v": str(rnd.choice(LONGS + INTS))},
                {"t": "Double", "v": rnd.choice(DOUBLES)},
                {"t": "Float", "v": rnd.choice(FLOATS)},
                {"t": "BigInteger", "v": rnd.choice(BIGINTS)},
                {"t": "BigDecimal", "v": rnd.choice(BIGDECS)},
                {"t": "Byte", "v": str(rnd.randint(-128, 127))},
                {"t": "Short", "v": str(rnd.randint(-32768, 32767))},
            ])]
        case = {"id": "ar%d_%d" % (seed, i), "kind": "arith", "op": op, "args": args}
        conf = arith_conf(rnd)
        if conf:
            case["arith"] = conf
        # ensure_ascii keeps lone surrogates (\ud83d) representable, like the Java oracle writer
        print(json.dumps(case))


if __name__ == "__main__":
    main()
