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
| java.math.BigDecimal / number text | GREEN | `cargo test --test java_number` (9 tests over JVM-generated fixtures) |
| java.util.HashMap / HashSet order | GREEN | `cargo test --test java_hash_map` (6 tests, 20,700 JVM sequences) |
| parser (Parser.jjt productions, JexlParser, FeatureController, getVariables) | 8,000-case suite: 164 differ, **all 164 blocked on the regex stub** (163 regex messages + 1 knock-on) | `cargo test --test parser_oracle` |
| java.util.regex | in progress (subagent) | |

### Parser mismatch ledger (see MISMATCHES.md)
Classes found and fixed while driving 4,191 -> 164:
1. node class names (`ASTxxx` vs jjtNodeName) — test-side dump.
2. `Collections.emptyMap()` vs `unmodifiableMap` for a script with no pragmas.
3. `Engine.trimSource` (Character.isSpaceChar only) not applied before parsing.
4. `Engine.collectMode` ignored by `getVariables`.
5. Lone surrogates lost: `ASTIdentifierAccess.name`, `getVariables` entries and every
   `JexlException` message are Java Strings (UTF-16), not Rust `String`.
6. **Parser state leaks across parses**: a lexical error raised inside a semantic lookahead
   (`isDeclaredNamespace(getToken(1), getToken(2))`) leaves `jj_lookingAhead` true, so the next
   parse reads `jj_scanpos` from the previous token chain and the root node gets a stale
   line/column. Reproduced and ported (the token arena keeps that chain alive).

## How to rerun
```
oracle/build.sh
python3 tools/gen_lexer_cases.py 200000 777 > /tmp/lex.jsonl && oracle/target/oracle < /tmp/lex.jsonl > /tmp/lex_exp.jsonl
LEXER_CASES=/tmp/lex.jsonl LEXER_EXPECTED=/tmp/lex_exp.jsonl cargo test --release --test lexer_oracle
```
