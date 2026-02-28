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
pub mod hover;
pub mod index;

pub use index::ClangIndex;
