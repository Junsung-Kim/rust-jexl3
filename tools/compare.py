#!/usr/bin/env python3
"""Compare two result JSONL files (expected = oracle, actual = rust) case by case.

Mismatches are grouped by a root-cause signature so one bug shows up as one line.
Usage: compare.py CASES EXPECTED ACTUAL [--show N] [--json OUT]
Exit status is 0 only when there are no mismatches.
"""
import json
import re
import sys
from collections import OrderedDict, defaultdict


def load(path):
    out = OrderedDict()
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                o = json.loads(line)
                out[o["id"]] = o
    return out


def strip_nd(v):
    """Values flagged nondeterministic compare by type/class only."""
    if isinstance(v, dict):
        if v.get("nd"):
            return {"t": v.get("t"), "c": v.get("c"), "nd": True}
        return {k: strip_nd(x) for k, x in v.items()}
    if isinstance(v, list):
        return [strip_nd(x) for x in v]
    return v


def tag(v):
    if isinstance(v, dict) and "t" in v:
        return v["t"] + ("<%s>" % v["c"] if "c" in v and v["t"] in ("List", "Set", "Map", "Array", "Object") else "")
    return type(v).__name__


def msg_shape(m):
    """Message with quoted fragments and numbers masked: groups same-shaped messages."""
    if m is None:
        return None
    m = re.sub(r"'[^']*'", "'_'", m)
    return re.sub(r"\d+", "N", m)


def signature(field, e, a):
    if e is None or a is None:
        return "%s: %s vs %s" % (field, "absent" if e is None else "present", "absent" if a is None else "present")
    if field in ("error", "parse", "engine_error"):
        if e.get("class") != a.get("class"):
            return "%s.class: %s vs %s" % (field, e.get("class"), a.get("class"))
        if e.get("msg") != a.get("msg"):
            return "%s.msg: %s | %s" % (field, e.get("class"), msg_shape(e.get("msg")))
        return "%s.cause: %s vs %s" % (field, (e.get("cause") or {}).get("class"), (a.get("cause") or {}).get("class"))
    if field in ("result", "ctx"):
        if tag(e) != tag(a):
            return "%s.type: %s vs %s" % (field, tag(e), tag(a))
        return "%s.value: %s" % (field, tag(e))
    return "%s differs" % field


FIELDS = ("engine_error", "parse", "vars", "params", "locals", "pragmas", "parsed", "result", "error", "output", "ctx")


def compare(cases, exp, act):
    groups = defaultdict(list)
    skipped = 0
    for cid, e in exp.items():
        if e.get("timeout") or e.get("harness_error"):
            skipped += 1
            continue
        a = act.get(cid)
        if a is None:
            groups["missing in actual"].append((cid, None))
            continue
        for f in FIELDS:
            ev, av = strip_nd(e.get(f)), strip_nd(a.get(f))
            if ev != av:
                groups[signature(f, ev, av)].append((cid, f))
                break
    return groups, skipped


def main():
    args = sys.argv[1:]
    show = 3
    out_json = None
    if "--show" in args:
        i = args.index("--show"); show = int(args[i + 1]); del args[i:i + 2]
    if "--json" in args:
        i = args.index("--json"); out_json = args[i + 1]; del args[i:i + 2]
    cases_path, exp_path, act_path = args
    cases = load(cases_path)
    exp, act = load(exp_path), load(act_path)
    groups, skipped = compare(cases, exp, act)
    total = sum(len(v) for v in groups.values())
    print("cases=%d compared=%d skipped(timeout/harness)=%d mismatches=%d classes=%d"
          % (len(cases), len(exp) - skipped, skipped, total, len(groups)))
    for sig, items in sorted(groups.items(), key=lambda kv: -len(kv[1])):
        print("\n[%d] %s" % (len(items), sig))
        for cid, f in items[:show]:
            c = cases.get(cid, {})
            print("  id=%s src=%r" % (cid, c.get("src")))
            if f:
                print("    expected: %s" % json.dumps(exp[cid].get(f), ensure_ascii=False)[:400])
                print("    actual:   %s" % json.dumps(act.get(cid, {}).get(f), ensure_ascii=False)[:400])
    if out_json:
        with open(out_json, "w") as fo:
            json.dump({s: [c for c, _ in v] for s, v in groups.items()}, fo, indent=1)
    sys.exit(0 if total == 0 else 1)


if __name__ == "__main__":
    main()
