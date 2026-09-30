// Rust replacements for the parts of the JDK that JEXL semantics depend on.
// These are not JEXL classes; each file names the JDK class whose behavior it reproduces.
pub mod big_decimal;
pub mod character_data;
pub mod hash_map;
pub mod map_view;
pub mod number;
pub mod string;

pub mod regex;
