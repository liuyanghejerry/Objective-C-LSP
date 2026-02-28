//! Semantic analysis layer backed by libclang.
//!
//! Provides full Objective-C type-aware intelligence:
//! - Completions
//! - Hover (types, doc comments)
//! - Diagnostics
//! - Go-to-definition / declaration
//! - Find references
//! - Rename (selectors, properties)

pub mod completion;
pub mod diagnostics;
pub mod goto_def;
pub mod hover;
pub mod index;
pub mod references;
pub mod implementation;
pub mod protocol_stubs;
pub mod rename;

pub use index::ClangIndex;
