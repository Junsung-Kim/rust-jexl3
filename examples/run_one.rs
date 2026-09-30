//! Runs one script against a MapContext; a debugging aid for the differential suites.
//! Usage: cargo run --example run_one -- "1 + 1"
use std::sync::Arc;

use rust_jexl3::jexl_context::MapContext;
use rust_jexl3::jexl_engine::JexlBuilder;

fn main() {
    let src = std::env::args().nth(1).expect("a script");
    let engine = JexlBuilder::new().create();
    match engine.create_script(&src) {
        Err(e) => println!("parse: {} {}", e.class_name(), e.message()),
        Ok(script) => match script.execute(Arc::new(MapContext::new())) {
            Ok(v) => println!("result: {:?}", v),
            Err(e) => println!("error: {} {}", e.class_name(), e.message()),
        },
    }
}
