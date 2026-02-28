//! Selector database and multi-part completion engine.
//!
//! ObjC selectors like `tableView:cellForRowAtIndexPath:` are multi-part.
//! This module builds a trie-like structure for fast prefix lookup so that
//! completions can fill in the entire skeleton:
//!   `[obj tableView:<#(UITableView *)tableView#> cellForRowAtIndexPath:<#(NSIndexPath *)indexPath#>]`

use std::collections::HashMap;

/// A parsed ObjC selector.
///
/// `tableView:cellForRowAtIndexPath:` → parts: `["tableView", "cellForRowAtIndexPath"]`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Selector {
    pub parts: Vec<String>,
}

impl Selector {
    pub fn parse(raw: &str) -> Self {
        if !raw.contains(':') {
            // Unary selector.
            return Self {
                parts: vec![raw.to_owned()],
            };
        }
        let parts = raw
            .split(':')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        Self { parts }
    }

    pub fn is_unary(&self) -> bool {
        self.parts.len() == 1
    }

    /// Reconstruct the full selector string (e.g. `foo:bar:`).
    pub fn to_string(&self) -> String {
        if self.is_unary() {
            self.parts[0].clone()
        } else {
            self.parts.iter().map(|p| format!("{p}:")).collect()
        }
    }
}

/// An entry in the selector database.
#[derive(Debug, Clone)]
pub struct SelectorEntry {
    pub selector: Selector,
    /// The class or protocol that defines this selector.
    pub defined_in: String,
    pub is_class_method: bool,
    /// Full signature for completion display, e.g.
    /// `- (UITableViewCell *)tableView:(UITableView *)tableView cellForRowAtIndexPath:(NSIndexPath *)indexPath`
    pub signature: String,
}

/// In-memory selector lookup table.
#[derive(Default)]
pub struct SelectorDb {
    /// first_part → list of entries with that first part.
    index: HashMap<String, Vec<SelectorEntry>>,
}

impl SelectorDb {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, entry: SelectorEntry) {
        if let Some(first) = entry.selector.parts.first() {
            self.index.entry(first.clone()).or_default().push(entry);
        }
    }

    /// All selectors whose first part starts with `prefix`.
    pub fn complete(&self, prefix: &str) -> Vec<&SelectorEntry> {
        self.index
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .flat_map(|(_, v)| v.iter())
            .collect()
    }

    /// Exact lookup by selector string.
    pub fn find(&self, selector_str: &str) -> Vec<&SelectorEntry> {
        let sel = Selector::parse(selector_str);
        if let Some(first) = sel.parts.first() {
            self.index
                .get(first)
                .map(|v| v.iter().filter(|e| e.selector == sel).collect::<Vec<_>>())
                .unwrap_or_default()
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unary() {
        let s = Selector::parse("description");
        assert_eq!(s.parts, vec!["description"]);
        assert!(s.is_unary());
    }

    #[test]
    fn parse_multi() {
        let s = Selector::parse("tableView:cellForRowAtIndexPath:");
        assert_eq!(s.parts, vec!["tableView", "cellForRowAtIndexPath"]);
        assert!(!s.is_unary());
    }

    #[test]
    fn roundtrip() {
        let raw = "tableView:cellForRowAtIndexPath:";
        assert_eq!(Selector::parse(raw).to_string(), raw);
    }
}
