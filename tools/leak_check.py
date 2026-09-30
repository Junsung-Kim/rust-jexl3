#!/usr/bin/env python3
"""Does anything in the git history carry a value from the private corpus?

Reads the corpus only through JEXL_PRIVATE_CORPUS. Compares its distinctive tokens (quoted
literals, dotted paths) with every blob of every commit, JSON escapes decoded -- the oracle and
Python both write non-ASCII as \\uXXXX, which a plain grep never sees. Tokens that also occur in
the Apache sources are public by definition and not reported.

    JEXL_PRIVATE_CORPUS=/path/to/exprs.jsonl python3 tools/leak_check.py [apache-src-dir]

Exit status 1 if an overlap longer than GENERIC is found. Overlaps are printed: this runs locally.
"""
import json
import os
import re
import subprocess
import sys

TOK = re.compile(r"""'([^'\\\n]{4,80})'|"([^"\\\n]{4,80})"|`([^`\\\n]{4,80})`"""
                 r"""|\b([A-Za-z_][A-Za-z0-9_]{2,}(?:\.[A-Za-z0-9_]{2,})+)\b""")
UESC = re.compile(r"\\u([0-9a-fA-F]{4})")
# ordinary words and numbers that any code base contains; reviewed, not secrets
GENERIC = {"100000", "join", "level"}


def tokens(text):
    return {next(g for g in m.groups() if g) for m in TOK.finditer(text)}


def unescape(text):
    return UESC.sub(lambda m: chr(int(m.group(1), 16)), text).replace('\\"', '"')


def main():
    corpus_path = os.environ.get("JEXL_PRIVATE_CORPUS")
    if not corpus_path:
        print("JEXL_PRIVATE_CORPUS not set: skipped")
        return 0
    corpus = set()
    for line in open(corpus_path, encoding="utf-8"):
        if line.strip():
            corpus |= tokens(json.loads(line)["expr"])
    apache = set()
    root = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/src/commons-jexl-3.2.1/src")
    for base, _, names in os.walk(root):
        for n in names:
            if n.endswith(".java"):
                apache |= tokens(open(os.path.join(base, n), encoding="utf-8", errors="replace").read())
    blobs = {}
    listing = subprocess.run(["git", "rev-list", "--objects", "--all"], capture_output=True, text=True).stdout
    for line in listing.split("\n"):
        if " " in line:
            sha, path = line.split(" ", 1)
            blobs.setdefault(sha, path)
    hits = {}
    for sha, path in blobs.items():
        text = subprocess.run(["git", "cat-file", "-p", sha], capture_output=True).stdout.decode("utf-8", "replace")
        for t in ((tokens(text) | tokens(unescape(text))) & corpus) - apache - GENERIC:
            hits.setdefault(t, set()).add(path)
    print("private-corpus values in history: %d (of %d distinctive tokens, %d blobs)" % (len(hits), len(corpus), len(blobs)))
    for t, paths in sorted(hits.items()):
        print("  %r in %s" % (t, sorted(paths)[:3]))
    return 1 if hits else 0


if __name__ == "__main__":
    sys.exit(main())
