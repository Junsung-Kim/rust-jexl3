//! The same scripts as oracle/src/rustjexl/oracle/Bench.java, timed the same way.
//!
//!   cargo run --release --example bench
use std::sync::Arc;
use std::time::Instant;

use rust_jexl3::jexl_context::{JexlContext, MapContext};
use rust_jexl3::jexl_engine::JexlBuilder;
use rust_jexl3::value::Value;

const SCRIPTS: [&str; 4] = [
    "a.b > 1 && name == '한글'",
    "x * 3 + y / 2 - 1",
    "a.b == null || (x > 10 ? 'big' : 'small') == 'small'",
    "var t = 0; for (i : 1..20) { t = t + i * 2; } t",
];

fn main() {
    let parse_n = 20_000;
    let exec_n = 200_000;
    let jexl = JexlBuilder::new().strict(true).cache(512).create();
    let context = Arc::new(MapContext::new());
    context.set("a.b", Value::Integer(2)).expect("set");
    context.set("name", Value::string("한글")).expect("set");
    context.set("x", Value::Long(7)).expect("set");
    context.set("y", Value::Integer(3)).expect("set");

    for src in SCRIPTS {
        // `ns:` needs a namespace the example does not ship; time what parses
        let script = match jexl.create_script(src) {
            Ok(s) => s,
            Err(e) => {
                println!("{:<58} skipped: {}", format!("{:?}", src), e.message());
                continue;
            }
        };
        for _ in 0..20_000 {
            let _ = jexl.create_script(src);
            let _ = script.execute(context.clone());
        }
        let t0 = Instant::now();
        for _ in 0..parse_n {
            let _ = jexl.create_script(src);
        }
        let parse = t0.elapsed().as_nanos() as f64 / parse_n as f64;
        let t0 = Instant::now();
        let mut last = Value::Null;
        for _ in 0..exec_n {
            last = script.execute(context.clone()).unwrap_or(Value::Null);
        }
        let exec = t0.elapsed().as_nanos() as f64 / exec_n as f64;
        println!("{:<58} parse {:8.0} ns  exec {:8.0} ns  ({})", format!("{:?}", src), parse, exec, last.java_to_string());
    }
}
