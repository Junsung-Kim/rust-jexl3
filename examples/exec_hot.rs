//! One script, executed in a tight loop — the thing to put under a profiler.
//!   cargo run --release --example exec_hot -- 'x * 3 + y / 2 - 1' 2000000
use std::sync::Arc;
use rust_jexl3::jexl_context::{JexlContext, MapContext};
use rust_jexl3::jexl_engine::JexlBuilder;
use rust_jexl3::value::Value;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = args.get(1).map(String::as_str).unwrap_or("x * 3 + y / 2 - 1");
    let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2_000_000);
    let jexl = JexlBuilder::new().strict(true).cache(512).create();
    let context = Arc::new(MapContext::new());
    context.set("a.b", Value::Integer(2)).unwrap();
    context.set("name", Value::string("한글")).unwrap();
    context.set("x", Value::Long(7)).unwrap();
    context.set("y", Value::Integer(3)).unwrap();
    let script = jexl.create_script(src).unwrap();
    let t0 = std::time::Instant::now();
    let mut last = Value::Null;
    for _ in 0..n {
        last = script.execute(context.clone()).unwrap();
    }
    eprintln!("{:.0} ns/exec ({})", t0.elapsed().as_nanos() as f64 / n as f64, last.java_to_string());
}
