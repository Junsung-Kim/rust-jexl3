#!/usr/bin/env python3
"""Collect the JEXL sources embedded in the upstream regression tests into differential cases.

The upstream @Test methods are ported by hand where they carry assertions the oracle protocol
cannot express; this collects the *expressions* from the issue-regression suites, which is where
the interesting edge cases live, and compares them against the jar directly.

    python3 tools/gen_upstream_corpus.py ~/src/commons-jexl-3.2.1 > tests/data/upstream/cases.jsonl
    python3 tools/run_oracle.py tests/data/upstream/cases.jsonl tests/data/upstream/expected.jsonl
    cargo test --release --test upstream_oracle
"""
import json
import os
import re
import sys

FILES = ["IssuesTest", "Issues100Test", "Issues200Test", "Issues300Test", "JXLTTest", "ScriptCallableTest"]

CTX = {
    "x": {"t": "Integer", "v": "1"}, "y": {"t": "Integer", "v": "2"}, "z": {"t": "Integer", "v": "3"},
    "a": {"t": "String", "v": "a"}, "b": {"t": "String", "v": "b"}, "c": {"t": "Boolean", "v": "true"},
    "i": {"t": "Long", "v": "10"},
    "l": {"t": "List", "c": "java.util.ArrayList", "v": [{"t": "Integer", "v": "1"}, {"t": "Integer", "v": "2"}]},
    "m": {"t": "Map", "c": "java.util.LinkedHashMap", "v": [[{"t": "String", "v": "k"}, {"t": "Integer", "v": "7"}]]},
    "s": {"t": "String", "v": "str"}, "n": {"t": "null"}, "d": {"t": "Double", "v": "1.5"},
    "foo": {"t": "Host", "v": "bean"}, "bar": {"t": "Host", "v": "bean"}, "obj": {"t": "Host", "v": "bean"},
}

LITERAL = re.compile(r'"((?:[^"\\]|\\.)*)"')


def main():
    root = os.path.join(sys.argv[1], "src/test/java/org/apache/commons/jexl3")
    seen, n = set(), 0
    for name in FILES:
        text = open(os.path.join(root, name + ".java"), encoding="utf-8").read()
        for match in LITERAL.finditer(text):
            try:
                src = json.loads('"' + match.group(1) + '"')
            except ValueError:
                continue
            if not src or len(src) > 300 or src in seen:
                continue
            # messages, class names and format strings are not JEXL
            if src.startswith(("org.apache", "java.", "%")) or " should " in src:
                continue
            if not re.search(r"[a-zA-Z0-9_'\"\[{(]", src):
                continue
            seen.add(src)
            print(json.dumps({"id": "iss%d" % n, "kind": "script", "src": src,
                              "engine": {"namespaces": {"ns": "ns"}}, "ctx": CTX,
                              "ops": ["vars", "parsed", "exec", "ctx"]}))
            n += 1


if __name__ == "__main__":
    main()
