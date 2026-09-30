#!/usr/bin/env python3
"""Turn the private production corpus into differential cases, under the consumer-profile engine.

The corpus is private: it is read only through the `JEXL_PRIVATE_CORPUS` environment variable and
the generated cases contain real expressions, so they must never be written inside the repository.
This script refuses to write anywhere under the repo for that reason.

Two passes, because a context can only be built once the variables are known:

    export JEXL_PRIVATE_CORPUS=~/husky-fixtures/jexl/exprs.jsonl
    python3 tools/gen_private_cases.py vars  /tmp/priv_vars.jsonl
    python3 tools/run_oracle.py             /tmp/priv_vars.jsonl /tmp/priv_vars_out.jsonl
    python3 tools/gen_private_cases.py cases /tmp/priv_vars.jsonl /tmp/priv_vars_out.jsonl /tmp/priv.jsonl
    python3 tools/run_oracle.py             /tmp/priv.jsonl /tmp/priv_exp.jsonl
    EXEC_CASES=/tmp/priv.jsonl EXEC_EXPECTED=/tmp/priv_exp.jsonl cargo test --release --test exec_oracle
"""
import json
import os
import random
import sys

ENGINE = {"namespaces": {"ns": "ns"}, "strict": True, "cache": "4096", "cacheThreshold": "256"}

# the value pool a real binding can hold, including the host object that stands for "JSON null"
VALUES = [
    {"t": "null"},
    {"t": "Host", "v": "jsonNull"},
    {"t": "Boolean", "v": "true"}, {"t": "Boolean", "v": "false"},
    {"t": "Integer", "v": "0"}, {"t": "Integer", "v": "1"}, {"t": "Integer", "v": "2"},
    {"t": "Integer", "v": "16"}, {"t": "Integer", "v": "32"}, {"t": "Integer", "v": "600"},
    {"t": "Long", "v": "9223372036854775807"}, {"t": "Double", "v": "1.5"},
    {"t": "String", "v": ""}, {"t": "String", "v": "0"}, {"t": "String", "v": "1"},
    {"t": "String", "v": "true"}, {"t": "String", "v": "false"},
    {"t": "String", "v": "RegionA"}, {"t": "String", "v": "RegionB"},
    {"t": "String", "v": "kiwi"}, {"t": "String", "v": "mango"},
    {"t": "String", "v": "별+"}, {"t": "String", "v": "하늘-"},
    {"t": "String", "v": "10.1.2.3"}, {"t": "String", "v": "::1"},
]


def check_outside_repo(path):
    repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if os.path.abspath(path).startswith(repo + os.sep):
        raise SystemExit("refusing to write the private corpus inside the repository: %s" % path)


def read_corpus():
    path = os.environ.get("JEXL_PRIVATE_CORPUS")
    if not path:
        raise SystemExit("set JEXL_PRIVATE_CORPUS to the corpus path")
    out = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                out.append(json.loads(line)["expr"])
    return out


def main():
    mode = sys.argv[1]
    if mode == "vars":
        out_path = sys.argv[2]
        check_outside_repo(out_path)
        with open(out_path, "w", encoding="utf-8") as out:
            for i, expr in enumerate(read_corpus()):
                out.write(json.dumps({"id": "pv%d" % i, "kind": "script", "src": expr,
                                      "engine": ENGINE, "ops": ["vars"]}) + "\n")
        return
    if mode == "cases":
        vars_cases, vars_out, out_path = sys.argv[2], sys.argv[3], sys.argv[4]
        check_outside_repo(out_path)
        rnd = random.Random(20260930)
        srcs = {}
        for line in open(vars_cases, encoding="utf-8"):
            o = json.loads(line)
            srcs[o["id"]] = o["src"]
        with open(out_path, "w", encoding="utf-8") as out:
            for line in open(vars_out, encoding="utf-8"):
                o = json.loads(line)
                if "vars" not in o:
                    # the expression does not parse: keep it, the parse outcome is compared too
                    ctx = {}
                else:
                    ctx = {}
                    for path in o["vars"]:
                        # the consumer binds each ant-ish path under its dotted name
                        ctx[".".join(path)] = rnd.choice(VALUES)
                out.write(json.dumps({"id": o["id"], "kind": "script", "src": srcs[o["id"]],
                                      "engine": ENGINE, "ctx": ctx,
                                      "ops": ["vars", "exec", "ctx"]}) + "\n")
        return
    raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
