//! Category aggregation.
//!
//! Objective-C allows splitting a class across multiple `@interface Foo (Cat)`
//! blocks in separate files.  This module tracks which categories belong to
//! which base class so that `documentSymbol` and `findReferences` can present
//! a unified view.

use std::collections::HashMap;

/// A single category entry.
#[derive(Debug, Clone)]
pub struct CategoryInfo {
    pub base_class: String,
    pub category_name: String,
    pub file: String,
    pub line: u32,
}

/// In-memory category registry (backed by `objc-store` for persistence).
#[derive(Default)]
pub struct CategoryRegistry {
    /// base_class → list of categories.
    map: HashMap<String, Vec<CategoryInfo>>,
}

impl CategoryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a category discovered during indexing.
    pub fn register(&mut self, info: CategoryInfo) {
        self.map
            .entry(info.base_class.clone())
            .or_default()
            .push(info);
    }

    /// All categories for `base_class`.
    pub fn categories_for(&self, base_class: &str) -> &[CategoryInfo] {
        self.map
            .get(base_class)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// All registered base classes.
    pub fn all_base_classes(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }
}
