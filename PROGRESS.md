# PROGRESS — rust-jexl (resume point)

Target: behavioral identity with `commons-jexl3-3.2.1.jar` (oracle JDK: Corretto 25). Spec: `~/rust-jexl-PROMPT.md`.
Method: TDD. Each subsystem: oracle-derived cases / ported upstream tests committed and seen RED first, then ported code, then GREEN. Coverage measured with `cargo llvm-cov`.

## Baselines (measured)
- Upstream test suite at tag, built and run on JDK 25 (copy in scratch, `mvn test` with rat/japicmp/etc skipped): **792 tests run, 0 failures, 0 errors, 0 skipped**.
- javacc output used for the mechanical translation (`target/generated-sources` of the upstream build, ParserGeneratorCC) vs classes in the released jar, normalized `javap -c`:
  - `ParserTokenManager`: identical.
  - `Parser`: differs only in dead code the old javac kept (`if ("" != null)` folding, unreachable finally copy and `Missing return statement` throw in the 4 value-returning productions). Semantically identical.

## Harness
- `oracle/` (Java, `oracle/build.sh` → `oracle/target/oracle`): JSONL in/out; modes: script, expression, template, jxlt, tokens.
- `tools/compare.py CASES EXPECTED ACTUAL`: groups mismatches by signature.
- `tests/common/json.rs`: UTF-16-preserving JSON for tests + harness.

## Subsystems
| subsystem | status | evidence |
|---|---|---|
| lexer (ParserTokenManager + SimpleCharStream) | GREEN | `cargo test --test lexer_oracle` 6,000 CI cases; local 200,000-case campaign 0 mismatches (`tools/gen_lexer_cases.py 200000 777`) |
| java.math.BigDecimal / number text | in progress (subagent) | |
| java.util.HashMap order | in progress (subagent) | |

## How to rerun
```
oracle/build.sh
python3 tools/gen_lexer_cases.py 200000 777 > /tmp/lex.jsonl && oracle/target/oracle < /tmp/lex.jsonl > /tmp/lex_exp.jsonl
LEXER_CASES=/tmp/lex.jsonl LEXER_EXPECTED=/tmp/lex_exp.jsonl cargo test --release --test lexer_oracle
```
