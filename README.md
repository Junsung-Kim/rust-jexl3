# rust-jexl

A Rust port of [Apache Commons JEXL](https://commons.apache.org/proper/commons-jexl/) **3.2.1**,
behaviorally compatible with the Java library: the same grammar, the same results, the same result
*types*, the same side effects, and the same exception messages.

Compatibility is not a claim, it is a test: every subsystem is measured against the official
`commons-jexl3-3.2.1.jar` running on a JVM, case by case, and a difference is a bug. See
[PROGRESS.md](PROGRESS.md) for what is proven and how to re-run it, and
[COMPATIBILITY.md](COMPATIBILITY.md) for the few intended divergences.

```toml
[dependencies]
rust-jexl = "0.1"
```

## Quick start

If you know the Java API, you already know this one; the names are the same, in snake_case.

```rust
use std::sync::Arc;
use rust_jexl::jexl_context::MapContext;
use rust_jexl::jexl_engine::JexlBuilder;
use rust_jexl::value::Value;

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
# Ok::<(), Box<dyn std::error::Error>>(())
```

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

## License

Apache-2.0, like the original. This is a derivative work of Apache Commons JEXL; see
[NOTICE](NOTICE).
