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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_info(base_class: &str, category_name: &str) -> CategoryInfo {
        CategoryInfo {
            base_class: base_class.to_owned(),
            category_name: category_name.to_owned(),
            file: format!("{base_class}+{category_name}.h"),
            line: 1,
        }
    }

    #[test]
    fn new_registry_is_empty() {
        let reg = CategoryRegistry::new();
        assert!(reg.categories_for("Foo").is_empty());
        assert_eq!(reg.all_base_classes().count(), 0);
    }

    #[test]
    fn register_and_retrieve_single_category() {
        let mut reg = CategoryRegistry::new();
        reg.register(make_info("NSString", "Extras"));
        let cats = reg.categories_for("NSString");
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].category_name, "Extras");
    }

    #[test]
    fn register_multiple_categories_for_same_base() {
        let mut reg = CategoryRegistry::new();
        reg.register(make_info("UIView", "Animations"));
        reg.register(make_info("UIView", "Layout"));
        reg.register(make_info("UIView", "Shadows"));
        let cats = reg.categories_for("UIView");
        assert_eq!(cats.len(), 3);
        let names: Vec<&str> = cats.iter().map(|c| c.category_name.as_str()).collect();
        assert!(names.contains(&"Animations"), "{names:?}");
        assert!(names.contains(&"Layout"), "{names:?}");
        assert!(names.contains(&"Shadows"), "{names:?}");
    }

    #[test]
    fn categories_for_unknown_class_returns_empty() {
        let mut reg = CategoryRegistry::new();
        reg.register(make_info("Foo", "Bar"));
        assert!(reg.categories_for("NotFoo").is_empty());
    }

    #[test]
    fn all_base_classes_enumerates_all() {
        let mut reg = CategoryRegistry::new();
        reg.register(make_info("Alpha", "A"));
        reg.register(make_info("Beta", "B"));
        reg.register(make_info("Alpha", "C")); // second category for Alpha
        let mut bases: Vec<&str> = reg.all_base_classes().collect();
        bases.sort();
        assert_eq!(bases, ["Alpha", "Beta"]);
    }
}
