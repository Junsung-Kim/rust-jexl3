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
| parser (Parser.jjt productions, JexlParser, FeatureController, getVariables) | GREEN | `cargo test --test parser_oracle` 8,000 cases, 0 mismatches |
| Debugger (getParsedText, exception snippets) | GREEN | `cargo test --test debugger_oracle` 17,500 CI cases (8,000 parsed + 8,000 round-trip + 1,500 API over 19,927 nodes); local campaign 112,000+; llvm-cov 97.4% |
| Interpreter, Operators, Engine, contexts, public API | 5,962 cases, **9 differ** (38 skipped: JVM timeout/OOM) | `cargo test --test exec_oracle` |
| java.util.regex | GREEN for the suites that use it | `cargo test --test java_regex` |
| JDK shim (introspection) + JexlSandbox | GREEN | `cargo test --test spi_oracle` |
| JXLT template engine (JxltEngine, TemplateEngine, TemplateInterpreter, TemplateDebugger) | GREEN | `cargo test --test jxlt_oracle`: 8,364 protocol cases + 1,917 API cases, 0 mismatches |
| consumer-profile suite | 4,000 cases, **1 differs** | `tools/gen_profile_cases.py`, replayed through `exec_oracle` |
| private corpus (15,453 production expressions, never committed) | replays clean through `exec_oracle`; see `tools/gen_private_cases.py` | |
| JexlArithmetic (+ IntegerRange/LongRange) | GREEN | `cargo test --test arith_oracle` 12,000 CI cases; local 150,000-case campaign 0 mismatches |

### Parser mismatch ledger (see MISMATCHES.md)
Classes found and fixed while driving 4,191 -> 164:
1. node class names (`ASTxxx` vs jjtNodeName) — test-side dump.
2. `Collections.emptyMap()` vs `unmodifiableMap` for a script with no pragmas.
3. `Engine.trimSource` (Character.isSpaceChar only) not applied before parsing.
4. `Engine.collectMode` ignored by `getVariables`.
5. Lone surrogates lost: `ASTIdentifierAccess.name`, `getVariables` entries and every
   `JexlException` message are Java Strings (UTF-16), not Rust `String`.
6. Java `Object.toString()` of collections, and every arithmetic error message, are UTF-16.
7. `Double.toString` in an encoded value uses `doubleToRawLongBits` (NaN sign is observable),
   while `Double.equals`/`hashCode` use the canonical `doubleToLongBits`.
8. `IntegerRange`/`LongRange` iterators post-increment a bounded cursor, so a range whose max is
   the type's MAX_VALUE never terminates. Ported as-is (Java hangs the same way).
9. Java catches only `ArithmeticException` around each operator: a `NumberFormatException`,
   `ClassCastException` or `NullPointerException` from a coercion escapes the interpreter raw.
10. `Interpreter.interpret` swallows a `JexlException` when the engine is silent, but a raw JDK
   throwable still propagates.
11. Touching a register in a script that has no scope raises the JDK's *helpful* NullPointerException,
   which names the exact call and how the receiver was reached. Verified with `javap -c`:
   `InterpreterBase.getVariable` reaches the frame through a parameter (`"frame"`), while
   `executeAssign`, `visit(ASTVar)` and `visit(ASTForeachStatement)` read the field
   (`"this.frame"`); only `call()` null-checks it first.
12. The `if` statement's `catch (ArithmeticException)` wraps its *branches* too, not just the test.
13. **Parser state leaks across parses**: a lexical error raised inside a semantic lookahead
   (`isDeclaredNamespace(getToken(1), getToken(2))`) leaves `jj_lookingAhead` true, so the next
   parse reads `jj_scanpos` from the previous token chain and the root node gets a stale
   line/column. Reproduced and ported (the token arena keeps that chain alive).

## How to rerun
### Execution
```
python3 tools/fuzz_gen.py 6000 31 --ops exec > /tmp/ex.jsonl && python3 tools/run_oracle.py /tmp/ex.jsonl /tmp/ex_exp.jsonl
EXEC_CASES=/tmp/ex.jsonl EXEC_EXPECTED=/tmp/ex_exp.jsonl cargo test --release --test exec_oracle
```
(`tools/run_oracle.py` restarts the JVM when a script wedges it and marks that case `timeout`.)

### Arithmetic
```
python3 tools/gen_arith_cases.py 150000 99 > /tmp/ar.jsonl && oracle/target/oracle < /tmp/ar.jsonl > /tmp/ar_exp.jsonl
ARITH_CASES=/tmp/ar.jsonl ARITH_EXPECTED=/tmp/ar_exp.jsonl cargo test --release --test arith_oracle
```

```
oracle/build.sh
python3 tools/gen_lexer_cases.py 200000 777 > /tmp/lex.jsonl && oracle/target/oracle < /tmp/lex.jsonl > /tmp/lex_exp.jsonl
LEXER_CASES=/tmp/lex.jsonl LEXER_EXPECTED=/tmp/lex_exp.jsonl cargo test --release --test lexer_oracle
```

### JXLT
```
python3 tools/gen_jxlt_cases.py 8000 41 > /tmp/jx.jsonl && python3 tools/run_oracle.py /tmp/jx.jsonl /tmp/jx_exp.jsonl
JXLT_CASES=/tmp/jx.jsonl JXLT_EXPECTED=/tmp/jx_exp.jsonl cargo test --release --test jxlt_oracle
tools/gen_jxlt_api.sh   # regenerates the committed API fixtures straight from the jar
```

### Consumer profile
```
python3 tools/gen_profile_cases.py 4000 7 > /tmp/pf.jsonl && python3 tools/run_oracle.py /tmp/pf.jsonl /tmp/pf_exp.jsonl
EXEC_CASES=/tmp/pf.jsonl EXEC_EXPECTED=/tmp/pf_exp.jsonl cargo test --release --test exec_oracle
```

### Private corpus (never written inside the repo)
```
export JEXL_PRIVATE_CORPUS=~/husky-fixtures/jexl/exprs.jsonl
python3 tools/gen_private_cases.py vars  /tmp/priv_vars.jsonl
python3 tools/run_oracle.py              /tmp/priv_vars.jsonl /tmp/priv_vars_out.jsonl
python3 tools/gen_private_cases.py cases /tmp/priv_vars.jsonl /tmp/priv_vars_out.jsonl /tmp/priv.jsonl
python3 tools/run_oracle.py              /tmp/priv.jsonl /tmp/priv_exp.jsonl
EXEC_CASES=/tmp/priv.jsonl EXEC_EXPECTED=/tmp/priv_exp.jsonl cargo test --release --test exec_oracle
```

## Open work
- Upstream test suite port (54 files / 678 `@Test`): in progress.
- 9 exec + 1 profile mismatches still open (identity-hash ordering, property-error ordering,
  an `@strict` pragma case, a JVM OutOfMemoryError case).
- `cargo clippy -- -D warnings`: 540 of the ~565 warnings are `result_large_err`.
  Measured: `JexlException` is 184 bytes, so `Result<Value, JexlException>` is 184 and
  `Result<Value, Box<JexlException>>` is 32 -- boxing the error is the fix, not an `allow`.
  Deferred until the upstream-test port lands, to avoid a crate-wide type change mid-flight.
- `cargo-fuzz` 30 min on parser and evaluator; 1,000,000-case differential campaign.
- Benchmark against the oracle; `cargo package`; `MISMATCHES.md`.
