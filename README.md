# rust-jexl3

[![ci](https://github.com/Junsung-Kim/rust-jexl3/actions/workflows/ci.yml/badge.svg)](https://github.com/Junsung-Kim/rust-jexl3/actions/workflows/ci.yml)
[![MSRV 1.80](https://img.shields.io/badge/MSRV-1.80-blue)](Cargo.toml)
[![license](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

A Rust port of [Apache Commons JEXL](https://commons.apache.org/proper/commons-jexl/) **3.2.1**,
behaviorally compatible with the Java library: the same grammar, the same results, the same result
*types*, the same side effects, and the same exception messages.

> **Not the other JEXL.** The `jexl` package on npm and the `jexl-eval` crate implement
> [TomFrost/Jexl](https://github.com/TomFrost/Jexl), a different expression language that borrowed
> the name. This is the Apache one: `var`, lambdas, loops, namespaces, pragmas, JXLT templates.

> rust-jexl3 is an independent port. It is not affiliated with, endorsed by, or sponsored by The
> Apache Software Foundation.

Compatibility is not a claim, it is a test: every subsystem is measured against the official
`commons-jexl3-3.2.1.jar` running on a JVM, case by case, and a difference is a bug. See
[PROGRESS.md](PROGRESS.md) for what is proven and how to re-run it, and
[COMPATIBILITY.md](COMPATIBILITY.md) for the few intended divergences.

```toml
[dependencies]
rust-jexl3 = "0.1"
```

## Quick start

If you know the Java API, you already know this one; the names are the same, in snake_case.

```rust
use std::sync::Arc;
use rust_jexl3::{JexlBuilder, JexlContext, MapContext, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let jexl = JexlBuilder::new().strict(true).cache(512).create();

    let script = jexl.create_script("a.b > 1 && name == '한글'")?;

    // every ant-ish path the script reads, the way JexlScript.getVariables() reports them
    for path in script.get_variables() {
        println!("{:?}", path.iter().map(|p| p.to_rust()).collect::<Vec<_>>());
    }

    let context = Arc::new(MapContext::new());
    context.set("a.b", Value::Integer(2))?;
    context.set("name", Value::string("한글"))?;

    assert!(matches!(script.execute(context)?, Value::Boolean(true)));
    Ok(())
}
```

## Using your own types

Java JEXL reaches an object's properties and methods by reflection. Rust has none, so your type
tells the engine what it offers, through the same `JexlUberspect` SPI Java uses: implement
`HostIntrospector` and install it next to the modelled JDK. [`examples/host_object.rs`](examples/host_object.rs)
is a complete, runnable one -- properties, a setter and a method on a Rust struct:

```sh
cargo run --example host_object
```

The JDK types scripts commonly touch -- `String`, the boxed numbers, `BigInteger`/`BigDecimal`,
`StringBuilder`, `List`/`Set`/`Map` and their views, iterators, `Class` -- are modelled already, so
`name.length()`, `list.size()` or `map.keySet()` work as in Java.

## Untrusted input

JEXL 3.2.1's parser backtracks exponentially on deeply nested unterminated literals, and a script
can loop forever; the port keeps both behaviours. `rust_jexl3::guard::nesting_depth` bounds the
first before you parse, and a cancellable engine bounds the second. See [SECURITY.md](SECURITY.md).

## Why it behaves the way it does

JEXL's value is its lenient arithmetic, and that is where ports usually drift:

| expression | result | |
|---|---|---|
| `"2" == 2` | `true` | strings coerce to numbers for comparison |
| `"10" > 1` | `true` | |
| `"x" + 3` | `"x3"` | with a strict arithmetic, a String operand concatenates |
| `2 * "3"` | `6` (Integer) | |
| `a * "3"` with `a` a Long | Long | the operand types decide the result type |
| `a.b =! 0` | `true`, and binds `a.b` | a common typo: it parses as `a.b = !0` |

All of these are pinned by the differential suites, not by hand-written expectations.

## What is different from the Java library

There is no JVM, so reflection over arbitrary Java classes cannot exist. Properties, methods,
constructors and iteration go through the same `JexlUberspect` SPI Java uses; the JDK types a
script can touch are modeled, and your own Rust types plug in as host objects.
[COMPATIBILITY.md](COMPATIBILITY.md) lists the full set of exclusions and the handful of pinned
divergences.

## Performance

Measured against the jar on Corretto 25 (`cargo run --release --example bench` and
`rustjexl.oracle.Bench`, same scripts, same context):

| | parse | evaluate |
|---|---|---|
| short expressions | 110-122 ns (jar: 31-136, from its cache) | 320-538 ns (jar: 143-316) |
| a script with a loop and a local | 119 ns (jar: 15,338 — it cannot cache a script with locals) | ~4,900 ns (jar: 1,335) |

So parsing is on a par or far faster, and **evaluation is still 1.6-3.7x slower** than the JIT-warmed
JVM. It was 3-7x before a first profiling pass; what remains is locking on variable access, and
the plan for it is in [PROGRESS.md](PROGRESS.md).

## Trust

Every behaviour here is checked against the real `commons-jexl3-3.2.1.jar`, case by case, in
committed fixtures that `cargo test` replays without a JVM. The few known differences are listed
with their reasons in [MISMATCHES.md](MISMATCHES.md); the test fails on any difference that is not
listed, and on a listed one that has been fixed. `sh tools/verify.sh --full` adds a fresh
8,000-case sample against the jar.

## Contributing

Differences from Java JEXL are the most valuable report: [CONTRIBUTING.md](CONTRIBUTING.md) shows
how to pin one down against the jar. This project follows the
[Contributor Covenant](CODE_OF_CONDUCT.md).

## Publishing

Not published yet. [PUBLISHING.md](PUBLISHING.md) is the runbook: the repository URL, the GitHub
remote and the crates.io release, each as the command that does it.

## License

Apache-2.0, like the original. This is a derivative work of Apache Commons JEXL; see
[NOTICE](NOTICE).
