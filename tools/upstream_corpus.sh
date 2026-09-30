#!/bin/sh
# Replays the upstream issue-regression corpus through the execution suite.
set -e
cd "$(dirname "$0")/.."
EXEC_CASES=tests/data/upstream/cases.jsonl \
EXEC_EXPECTED=tests/data/upstream/expected.jsonl \
  cargo test --release --test exec_oracle execution_matches
