# Security

## Reporting

Please report a vulnerability privately, through GitHub's
[security advisories](https://github.com/Junsung-Kim/rust-jexl3/security/advisories/new) for this
repository, rather than in a public issue.

## Running untrusted scripts

rust-jexl3 reproduces Apache Commons JEXL 3.2.1's behaviour, limits included:

- **Parsing** backtracks exponentially on deeply nested *unterminated* literals -- in the jar as in
  the port (`"8%" + "{" * 13` takes the jar 27 seconds). Bound the size and nesting of source text
  you accept; `rust_jexl3::guard::nesting_depth` counts the deepest bracket nesting cheaply before
  you parse.
- **Evaluation** can loop forever (`while (true);`), as in Java. Build the engine with
  `JexlBuilder::cancellable(true)` and give the script a context whose `get_cancellation()`
  returns a flag you set from a watchdog; see `fuzz/fuzz_targets/eval.rs`.
- JEXL scripts can call methods on the values you hand them. Only put into a context what a script
  may use, or restrict it with `JexlSandbox`.
