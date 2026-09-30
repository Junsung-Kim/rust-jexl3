#![no_main]
//! Evaluating arbitrary scripts must fail cleanly: a JexlException, never a panic.
//!
//! JEXL itself can loop forever -- a range whose max is the type's MAX_VALUE never terminates, and
//! the port reproduces that Java behavior deliberately -- so the script runs under a cancellation
//! flag a watchdog trips. A hang is the library being faithful, not a finding.
use libfuzzer_sys::fuzz_target;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rust_jexl3::jexl_context::{JexlContext, MapContext};
use rust_jexl3::value::Value;

/// A MapContext that is also a `JexlContext.CancellationHandle`.
struct Cancellable {
    map: MapContext,
    cancel: Arc<AtomicBool>,
}

impl JexlContext for Cancellable {
    fn get(&self, name: &str) -> Option<Value> {
        self.map.get(name)
    }
    fn set(&self, name: &str, value: Value) -> Result<(), String> {
        self.map.set(name, value)
    }
    fn has(&self, name: &str) -> bool {
        self.map.has(name)
    }
    fn get_cancellation(&self) -> Option<Arc<AtomicBool>> {
        Some(self.cancel.clone())
    }
}

fuzz_target!(|data: &str| {
    // ponytail: the grammar is line-oriented, so a couple of KB already reaches every production
    if data.len() > 2048 {
        return;
    }
    // JEXL 3.2.1's parser backtracks exponentially on deeply nested unterminated literals, in Java
    // as much as here (see COMPATIBILITY.md); skip those so the fuzzer keeps moving.
    if data.bytes().filter(|b| matches!(b, b'{' | b'[' | b'(')).count() > 6 {
        return;
    }
    let engine = rust_jexl3::jexl_engine::JexlBuilder::new()
        .debug(true)
        .cache(0)
        .cancellable(true)
        .stack_overflow(64)
        .create();
    let script = match engine.create_script(data) {
        Ok(s) => s,
        Err(_) => return,
    };
    let context = Cancellable { map: MapContext::new(), cancel: Arc::new(AtomicBool::new(false)) };
    for (name, value) in [
        ("a", Value::Integer(1)),
        ("b", Value::string("x")),
        ("c", Value::Null),
        ("d", Value::Boolean(true)),
    ] {
        let _ = context.set(name, value);
    }
    let watchdog = context.cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        watchdog.store(true, Ordering::SeqCst);
    });
    let _ = script.execute(Arc::new(context));
});
