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
| Interpreter, Operators, Engine, contexts, public API | 5,962 cases, **5 differ** (38 skipped: JVM timeout/OOM) | `cargo test --test exec_oracle` |
| java.util.regex | GREEN for the suites that use it | `cargo test --test java_regex` |
| JDK shim (introspection) + JexlSandbox | GREEN | `cargo test --test spi_oracle` |
| JXLT template engine (JxltEngine, TemplateEngine, TemplateInterpreter, TemplateDebugger) | GREEN | `cargo test --test jxlt_oracle`: 8,364 protocol cases + 1,917 API cases, 0 mismatches |
| JexlScript API (getParsedText, toString, getUnboundParameters, curry, callable) | 2,977 cases, **3 differ** | `cargo test --test exec_oracle script_api` |
| upstream test suite | **352 of 678 `@Test` ported, 0 failing** | `cargo test --test upstream_arithmetic --test upstream_literals --test upstream_statements --test upstream_lexical --test upstream_engine` |
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

## Performance (measured, `cargo run --release --example bench` vs `rustjexl.oracle.Bench`)

| script | parse (jar) | parse (port) | exec (jar) | exec (port) |
|---|---|---|---|---|
| `a.b > 1 && name == '한글'` | 138 ns | 120 ns | 317 ns | 976 ns |
| `x * 3 + y / 2 - 1` | 32 ns | 105 ns | 141 ns | 711 ns |
| `a.b == null \|\| (x > 10 ? 'big' : 'small') == 'small'` | 47 ns | 115 ns | 279 ns | 1045 ns |
| `var t = 0; for (i : 1..20) { t = t + i * 2; } t` | 15179 ns | 118 ns | 1362 ns | 9646 ns |

The port parses faster than the jar except on the two short scripts the jar serves from its cache.
Execution is still 2-7x slower: nothing in the interpreter has been tuned, and the value model
clones more than Java's references do. The fourth script has local variables, so its tree has a
Scope and neither engine can reuse a cached parse -- that is the jar's real parse cost.

JEXL's parser backtracks exponentially on deeply nested unterminated literals. Measured on the jar:
`"8%" + "{" * n` takes 66 ms at n=8, 429 ms at n=10, 6.8 s at n=12, 27 s at n=13. The port is
within 2x of that. It is the original's behaviour, reproduced; bound the size of untrusted input.

## Open work
- Upstream test suite: 352 of 678 `@Test` ported. Remaining files are the issue-regression suites
  (IssuesTest, Issues100/200/300Test) and the JXLT/introspection test classes.
- 5 exec + 3 script-api + 1 profile mismatch, all explained in [MISMATCHES.md](MISMATCHES.md).
- 1,000,000-case differential campaign and the cargo-fuzz runs are the last gates.
