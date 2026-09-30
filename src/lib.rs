//! A Rust port of [Apache Commons JEXL](https://commons.apache.org/proper/commons-jexl/) 3.2.1.
//!
//! The behavior is the 3.2.1 jar's, not this code's: every subsystem is replayed case by case
//! against `commons-jexl3-3.2.1.jar` on a JVM, and a difference is a bug. The API is the Java one
//! with method names in snake_case, and the types Java keeps in `org.apache.commons.jexl3` are
//! re-exported here at the crate root.
//!
//! ```
//! use std::sync::Arc;
//! use rust_jexl3::{JexlBuilder, JexlContext, MapContext, Value};
//!
//! let jexl = JexlBuilder::new().strict(true).create();
//! let context = Arc::new(MapContext::new());
//! context.set("x", Value::Integer(20)).unwrap();
//!
//! let script = jexl.create_script("var y = x * 2; y + 2").unwrap();
//! assert_eq!(script.execute(context).unwrap().java_to_string(), "42");
//! ```
//!
//! Where to go next:
//! - [`JexlBuilder`] and [`JexlEngine`]: engine options, the parse cache, templates
//!   ([`JexlEngine::create_jxlt_engine`]).
//! - [`introspection`]: exposing your own Rust types to scripts (`HostIntrospector`).
//! - [`guard`]: cheap checks to run before parsing untrusted input.
//! - [COMPATIBILITY.md](https://github.com/Junsung-Kim/rust-jexl3/blob/main/COMPATIBILITY.md):
//!   what is ported, what is left out, and why.
pub mod guard;
pub mod internal;
pub mod java;
pub mod introspection;
pub mod jexl_arithmetic;
pub mod jexl_context;
pub mod jexl_engine;
pub mod jexl_exception;
pub mod jexl_features;
pub mod jexl_info;
pub mod jexl_operator;
pub mod jexl_options;
pub mod jxlt_engine;
pub mod parser;
pub mod value;

pub use jexl_arithmetic::JexlArithmetic;
pub use jexl_context::{EmptyContext, JexlContext, MapContext};
pub use jexl_engine::{JexlBuilder, JexlEngine, JexlScript};
pub use jexl_exception::JexlException;
pub use jexl_features::JexlFeatures;
pub use jexl_info::JexlInfo;
pub use jexl_operator::JexlOperator;
pub use jexl_options::JexlOptions;
pub use value::{HostObject, Value};

/// The README's examples compile and run as doctests, so they cannot drift from the API.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
