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
pub mod crash_guard;
pub mod diagnostics;
pub mod goto_def;
pub mod hover;
pub mod index;
pub mod references;
pub mod implementation;
pub mod protocol_stubs;
pub mod rename;
pub mod formatting;
pub mod call_hierarchy;
pub mod type_hierarchy;

pub use index::ClangIndex;
