#!/bin/sh
# One command, one short report: everything that decides whether the port is correct.
#
#   tools/verify.sh            # the committed suites (a few minutes)
#   tools/verify.sh --full     # adds the upstream corpus and a fresh 8k differential sample
#
# Needs oracle/build.sh to have been run once (it builds the JVM oracle from the real jar).
set -e
cd "$(dirname "$0")/.."
full=${1:-}

line() { printf '%-34s %s\n' "$1" "$2"; }

echo "== build =="
cargo build --release 2>&1 | grep -E "^(error|warning: unused)" || line "cargo build" "clean"
cargo clippy --release --all-targets -- -D warnings > /dev/null 2>&1 \
  && line "clippy -D warnings" "clean" || line "clippy -D warnings" "FAILED"
cargo package --quiet > /dev/null 2>&1 && line "cargo package" "ok" || line "cargo package" "FAILED"

echo "== suites =="
out=$(cargo test --release --no-fail-fast 2>&1 || true)
printf '%s\n' "$out" | grep -E "^test result:" | nl -ba | sed 's/^/  /' | head -20
printf '%s\n' "$out" | grep -E "of [0-9]+ (execution|script api) cases differ" | sed 's/^/  /'

echo "== upstream tests =="
line "ported @Test" "$(grep -ch '#\[test\]' tests/upstream_*.rs | paste -sd+ | bc) of 678"

if [ "$full" = "--full" ]; then
  echo "== upstream expression corpus =="
  sh tools/upstream_corpus.sh 2>&1 | grep -E "of [0-9]+ execution|test result" | sed 's/^/  /'

  echo "== fresh differential sample (8k, seed from the clock) =="
  seed=$(date +%s)
  tmp=$(mktemp -d)
  python3 tools/fuzz_gen.py 8000 "$seed" --ops exec > "$tmp/c.jsonl"
  python3 tools/run_oracle.py "$tmp/c.jsonl" "$tmp/e.jsonl" 2>&1 | sed 's/^/  /'
  EXEC_CASES="$tmp/c.jsonl" EXEC_EXPECTED="$tmp/e.jsonl" \
    cargo test --release --test exec_oracle execution_matches 2>&1 \
    | grep -E "of [0-9]+ execution|test result" | sed 's/^/  /'
  rm -rf "$tmp"
fi

echo "== privacy =="
git status --short | grep -q . && line "working tree" "DIRTY" || line "working tree" "clean"
git log --all --name-only --format= | sort -u | grep -icE "exprs\.jsonl" > /dev/null \
  && line "private corpus in history" "PRESENT" || line "private corpus in history" "absent"
