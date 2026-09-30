#![no_main]
//! Parsing arbitrary text must fail cleanly: a JexlException, never a panic.
//!
//! The Debugger runs on every input that parses, because the exception messages the port has to
//! reproduce byte-for-byte are rendered by it, and it walks the whole tree.
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;

fuzz_target!(|data: &str| {
    // ponytail: the grammar is line-oriented, so a few KB already reaches every production
    if data.len() > 4096 {
        return;
    }
    let engine = engine();
    if let Ok(script) = engine.create_script(data) {
        let _ = script.get_source_text();
        let _ = script.get_parsed_text();
        let _ = script.get_variables();
        let _ = script.get_parameters();
        let _ = script.get_pragmas();
    }
    let _ = engine.create_expression(None, data);
});

fn engine() -> Arc<rust_jexl3::jexl_engine::JexlEngine> {
    use std::sync::OnceLock;
    static ENGINE: OnceLock<Arc<rust_jexl3::jexl_engine::JexlEngine>> = OnceLock::new();
    ENGINE.get_or_init(|| rust_jexl3::jexl_engine::JexlBuilder::new().debug(true).cache(0).create()).clone()
}
