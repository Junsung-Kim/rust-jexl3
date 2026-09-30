//! A faithful Rust port of Apache Commons JEXL 3.2.1.
//!
//! Every module names the Java class it ports on its first line; method names are the Java ones
//! in snake_case. Behavior is defined by the 3.2.1 jar, not by this code: see PROGRESS.md for the
//! differential harness that proves it.
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
pub mod parser;
pub mod value;
