//! Fast syntax layer for Objective-C using tree-sitter.
//!
//! Provides millisecond-latency, error-tolerant parsing for:
//! - Document symbols (outline)
//! - Syntax highlighting tokens
//! - Folding ranges
//! - Header language detection (.h → ObjC vs C/C++)

pub mod header_detect;
pub mod parser;
pub mod symbols;
pub mod tokens;
pub mod inlay_hints;
pub mod folding;

pub use parser::ObjcParser;
pub use symbols::{flat_symbols, FlatSymbol};
