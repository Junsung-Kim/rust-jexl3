#!/usr/bin/env python3
"""Generate lexer differential cases: random token soups that exercise every JEXL token kind,
lexical state switch (DOT_ID / REGISTERS), comments, literals and lexical errors.
Usage: gen_lexer_cases.py N SEED > cases.jsonl
"""
import json
import random
import sys

FRAGS = [
    "if", "else", "for", "while", "do", "new", "var", "empty", "size", "null", "true", "false",
    "return", "function", "->", "break", "continue", "#pragma", "(", ")", "{", "}", "[", "]", ";",
    ":", ",", ".", "?.", "...", "?", "?:", "??", "&&", "and", "||", "or", "==", "eq", "!=", "ne",
    ">", "gt", ">=", "ge", "<", "lt", "<=", "le", "=~", "!~", "=^", "=$", "!^", "!$", "+=", "-=",
    "*=", "/=", "%=", "&=", "|=", "^=", "=", "+", "-", "*", "/", "div", "%", "mod", "!", "not",
    "&", "|", "^", "~", "..", "NaN", "#NaN", "@anno", "@", "#0", "#12", "#", "x", "abc", "_a1",
    "$b", "a\\ b", "ifx", "sizeof", "a.b", "a.0", "a.1b", "x.size", "x.if", "0", "1", "123",
    "08", "017", "0x1F", "0Xab", "1l", "2L", "3h", "4H", "1.5", "1.", ".5", ".5f", "1e5", "1E+5",
    "1.5e-3d", "2f", "3d", "4b", "5B", "1.5b", "7.0F", "0x", "'s'", '"d"', "'a\\'b'", '"a\\"b"',
    "'\\u0041'", "'unterminated", '"unterm', "`t`", "`a${b}c`", "`\\``", "~/re/", "~/a\\/b/",
    "~/", "/* c */", "/* unterminated", "// line\n", "## hash\n", "#", "\\", "'", '"', "`",
    "한글", "'한글'", "😀", "'😀'", "é", "\t", "\n", "\r\n", "\r", "\f", " ", "  ",
    " ", "' '", "\x00", "\x01", "\x7f", "a.'b'", "a.`c`", "a?.b", "a.#1", "...",
]
CHARS = "abcxyzABC_$@#0123456789 .,;:+-*/%!=<>&|^~?()[]{}'\"`\\\n\t한😀é"


def soup(rnd):
    n = rnd.randint(1, 12)
    parts = []
    for _ in range(n):
        r = rnd.random()
        if r < 0.75:
            parts.append(rnd.choice(FRAGS))
        else:
            parts.append("".join(rnd.choice(CHARS) for _ in range(rnd.randint(1, 4))))
        if rnd.random() < 0.5:
            parts.append(rnd.choice([" ", "", "", "\n", "\t"]))
    return "".join(parts)


def main():
    n, seed = int(sys.argv[1]), int(sys.argv[2])
    rnd = random.Random(seed)
    for i in range(n):
        case = {"id": "lex%d" % i, "kind": "tokens", "src": soup(rnd)}
        if rnd.random() < 0.2:
            case["registers"] = True
        print(json.dumps(case))


if __name__ == "__main__":
    main()
