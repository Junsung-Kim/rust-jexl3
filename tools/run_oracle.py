#!/usr/bin/env python3
"""Feeds cases to the Java oracle and restarts it when a case wedges the JVM.

A JEXL script can loop forever (`while(true)`, a range whose max is MAX_VALUE, ...). The oracle
cancels such a case after its own timeout, but the stuck thread cannot be killed, so it exits.
This driver restarts it with the remaining cases and emits `{"id":..., "timeout":true}` for the
case that did not finish, so the result file always lines up with the case file.

Usage: run_oracle.py CASES OUT [ORACLE]
"""
import json
import subprocess
import sys


def main():
    cases_path, out_path = sys.argv[1], sys.argv[2]
    oracle = sys.argv[3] if len(sys.argv) > 3 else "oracle/target/oracle"
    with open(cases_path, encoding="utf-8", errors="surrogatepass") as f:
        cases = f.read().splitlines()
    ids = [json.loads(c)["id"] for c in cases]
    results = {}
    start = 0
    restarts = 0
    while start < len(cases):
        chunk = "\n".join(cases[start:]) + "\n"
        proc = subprocess.run(
            [oracle],
            input=chunk.encode("utf-8", "surrogatepass"),
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        produced = 0
        for line in proc.stdout.decode("utf-8", "surrogatepass").splitlines():
            if not line.strip():
                continue
            o = json.loads(line)
            results[o["id"]] = line
            produced += 1
        if produced == 0:
            # the very first case of the chunk killed the JVM before printing anything
            results[ids[start]] = json.dumps({"id": ids[start], "timeout": True})
            produced = 1
            restarts += 1
        elif start + produced < len(cases):
            restarts += 1
        start += produced
    with open(out_path, "w", encoding="utf-8", errors="surrogatepass") as f:
        for i in ids:
            f.write(results.get(i, json.dumps({"id": i, "timeout": True})) + "\n")
    print("cases=%d restarts=%d" % (len(cases), restarts), file=sys.stderr)


if __name__ == "__main__":
    main()
