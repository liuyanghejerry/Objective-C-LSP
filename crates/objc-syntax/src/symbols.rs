//! Extracts document symbols from a parsed Objective-C file.
//!
//! Implements `textDocument/documentSymbol` using a recursive tree-sitter
//! walk. This is faster and more correct than query-based capture for ObjC,
//! because the grammar stores method names as plain `identifier` children
//! rather than named fields.

use anyhow::Result;
use lsp_types::{DocumentSymbol, Position, Range, SymbolKind};
use tree_sitter::Node;

use crate::parser::ParsedFile;

/// Extract a flat list of document symbols from the parsed file.
///
/// Categories (`@interface Foo (Cat)`) are aggregated into the base class
/// symbol as a nested group rather than appearing as separate top-level entries.
pub fn document_symbols(file: &ParsedFile) -> Result<Vec<DocumentSymbol>> {
    let src = file.source_bytes();
    let mut symbols: Vec<DocumentSymbol> = Vec::new();
    collect_symbols(file.root(), src, &mut symbols);
    aggregate_categories(&mut symbols);
    Ok(symbols)
}

// ---------------------------------------------------------------------------
// Category aggregation
// ---------------------------------------------------------------------------

/// Merge category symbols into their base class symbols.
///
/// After the tree walk, categories appear as top-level `MODULE` symbols
/// named `"ClassName (CategoryName)"`.  This function folds each category's
/// children into the matching `CLASS` symbol's children so that the IDE shows
/// a single, unified outline for the class.
///
/// If no matching base-class symbol exists the category is left in place.
fn aggregate_categories(symbols: &mut Vec<DocumentSymbol>) {
    // Separate categories from everything else.
    let mut categories: Vec<DocumentSymbol> = Vec::new();
    let mut rest: Vec<DocumentSymbol> = Vec::new();

    for sym in symbols.drain(..) {
        if sym.kind == SymbolKind::MODULE && sym.name.contains('(') {
            categories.push(sym);
        } else {
            rest.push(sym);
        }
    }

    // For each category, find the base class name (the part before " (").
    let mut orphans: Vec<DocumentSymbol> = Vec::new();

    for cat in categories {
        let base_name = cat
            .name
            .split_once(" (")
            .map(|(base, _)| base)
            .unwrap_or(&cat.name)
            .to_owned();

        // Look for a CLASS symbol with that name in `rest`.
        let found = rest.iter_mut().find(|s| {
            s.kind == SymbolKind::CLASS && s.name == base_name
        });

        if let Some(base) = found {
            // Append the category's children to the base class.
            let cat_children = cat.children.unwrap_or_default();
            match base.children.as_mut() {
                Some(existing) => existing.extend(cat_children),
                None if !cat_children.is_empty() => {
                    base.children = Some(cat_children);
                }
                None => {}
            }
        } else {
            // No base class in scope — keep the category as-is.
            orphans.push(cat);
        }
    }

    *symbols = rest;
    symbols.extend(orphans);
}

// ---------------------------------------------------------------------------
// Recursive walker
// ---------------------------------------------------------------------------

fn collect_symbols(node: Node<'_>, src: &[u8], out: &mut Vec<DocumentSymbol>) {
    match node.kind() {
        "class_interface" => {
            // A `class_interface` node with a `(` child is actually a category
            // (e.g. `@interface Person (Greeting)`). Route it to category_symbol.
            if node_has_child_kind(node, "(") {
                if let Some(sym) = category_symbol(node, src) {
                    out.push(sym);
                }
            } else if let Some(sym) = class_symbol(node, src, SymbolKind::CLASS) {
                out.push(sym);
            }
            // Don't recurse — children are already captured inside the builders.
            return;
        }
        "class_implementation" => {
            if let Some(sym) = impl_symbol(node, src) {
                out.push(sym);
            }
            return;
        }
        "category_interface" | "category_implementation" => {
            if let Some(sym) = category_symbol(node, src) {
                out.push(sym);
            }
            return;
        }
        "protocol_declaration" => {
            if let Some(sym) = protocol_symbol(node, src) {
                out.push(sym);
            }
            return;
        }
        _ => {}
    }

    // Default: recurse.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_symbols(child, src, out);
    }
}

// ---------------------------------------------------------------------------
// Node → DocumentSymbol builders
// ---------------------------------------------------------------------------

/// Build a CLASS symbol for `@interface Foo : NSObject … @end`.
fn class_symbol(node: Node<'_>, src: &[u8], kind: SymbolKind) -> Option<DocumentSymbol> {
    let name_node = first_identifier(node, src)?;
    let name = name_node.utf8_text(src).ok()?.to_owned();
    let range = node_to_range(node);
    let selection_range = node_to_range(name_node);

    // Collect methods and properties as children.
    let children = collect_children_symbols(node, src);

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    })
}

/// Build an implementation symbol and collect its method definitions.
fn impl_symbol(node: Node<'_>, src: &[u8]) -> Option<DocumentSymbol> {
    let name_node = first_identifier(node, src)?;
    let name = name_node.utf8_text(src).ok()?.to_owned();
    let range = node_to_range(node);
    let selection_range = node_to_range(name_node);

    let children = collect_children_symbols(node, src);

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind: SymbolKind::CLASS,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    })
}

/// Build a category symbol: `@interface Foo (Category)`.
fn category_symbol(node: Node<'_>, src: &[u8]) -> Option<DocumentSymbol> {
    // First identifier is the base class name, second is the category name.
    let mut ids = identifiers(node, src);
    let class_name = ids.next()?.utf8_text(src).ok()?.to_owned();
    let cat_name = ids
        .next()
        .and_then(|n| n.utf8_text(src).ok())
        .unwrap_or("")
        .to_owned();
    let name = if cat_name.is_empty() {
        class_name
    } else {
        format!("{class_name} ({cat_name})")
    };

    let range = node_to_range(node);
    let name_node = first_identifier(node, src)?;
    let selection_range = node_to_range(name_node);

    let children = collect_children_symbols(node, src);

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind: SymbolKind::MODULE,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    })
}

/// Build a protocol symbol.
fn protocol_symbol(node: Node<'_>, src: &[u8]) -> Option<DocumentSymbol> {
    let name_node = first_identifier(node, src)?;
    let name = name_node.utf8_text(src).ok()?.to_owned();
    let range = node_to_range(node);
    let selection_range = node_to_range(name_node);

    let children = collect_children_symbols(node, src);

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind: SymbolKind::INTERFACE,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    })
}

/// Collect direct-child symbols (methods, properties) inside a class/impl/protocol node.
fn collect_children_symbols(parent: Node<'_>, src: &[u8]) -> Vec<DocumentSymbol> {
    let mut out = Vec::new();
    let mut cursor = parent.walk();
    for child in parent.children(&mut cursor) {
        match child.kind() {
            "method_declaration" => {
                if let Some(sym) = method_symbol(child, src, false) {
                    out.push(sym);
                }
            }
            "method_definition" => {
                if let Some(sym) = method_symbol(child, src, true) {
                    out.push(sym);
                }
            }
            "implementation_definition" => {
                // Recurse into @implementation body.
                let mut c2 = child.walk();
                for gc in child.children(&mut c2) {
                    if gc.kind() == "method_definition" {
                        if let Some(sym) = method_symbol(gc, src, true) {
                            out.push(sym);
                        }
                    }
                }
            }
            "property_declaration" => {
                if let Some(sym) = property_symbol(child, src) {
                    out.push(sym);
                }
            }
            _ => {}
        }
    }
    out
}

/// Build a METHOD symbol from a `method_declaration` or `method_definition` node.
///
/// The tree-sitter-objc grammar structures method declarations as:
///   `(-|+) method_type? identifier (method_parameter identifier method_parameter …)* ;`
///
/// We reconstruct the full ObjC selector (e.g. `initWithName:age:`) by
/// collecting identifier + method_parameter pairs.
fn method_symbol(node: Node<'_>, src: &[u8], is_definition: bool) -> Option<DocumentSymbol> {
    let selector = method_selector_text(node, src);
    if selector.is_empty() {
        return None;
    }

    let range = node_to_range(node);
    // Use the first identifier as the selection range.
    let sel_node = first_identifier(node, src).unwrap_or(node);
    let selection_range = node_to_range(sel_node);

    let kind = SymbolKind::METHOD;
    let detail = if is_definition {
        Some("impl".to_owned())
    } else {
        None
    };

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name: selector,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: None,
    })
}

/// Reconstruct the full ObjC selector string from a method node.
///
/// Handles both simple selectors (`greet`) and compound selectors
/// (`initWithName:age:`).
fn method_selector_text(node: Node<'_>, src: &[u8]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut has_params = false;

    let mut cursor = node.walk();
    let mut prev_was_identifier = false;

    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                let text = child.utf8_text(src).unwrap_or("?").to_owned();
                parts.push(text);
                prev_was_identifier = true;
            }
            "method_parameter" => {
                has_params = true;
                // The identifier before this method_parameter is the keyword part.
                // Append `:` to the last pushed identifier.
                if prev_was_identifier {
                    if let Some(last) = parts.last_mut() {
                        last.push(':');
                    }
                }
                prev_was_identifier = false;
            }
            "-" | "+" | "method_type" | "compound_statement" | ";" => {
                prev_was_identifier = false;
            }
            _ => {
                prev_was_identifier = false;
            }
        }
    }

    // Join selector parts: for `initWithName:(NSString*)n age:(int)a`
    // parts = ["initWithName:", "age:"]  → "initWithName:age:"
    if has_params {
        // Only keep parts that end with ':' or are the first part.
        parts.retain(|p| p.ends_with(':'));
    }
    // If no params, parts = ["greet"] → "greet"
    parts.join("")
}

/// Build a PROPERTY symbol.
fn property_symbol(node: Node<'_>, src: &[u8]) -> Option<DocumentSymbol> {
    // Structure: @property property_attributes_declaration? struct_declaration
    // struct_declaration: type struct_declarator(identifier) ;
    let prop_name = find_property_name(node, src)?;
    let range = node_to_range(node);

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name: prop_name,
        detail: None,
        kind: SymbolKind::PROPERTY,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: None,
    })
}

/// Extract property name from the `struct_declaration` subtree.
/// The identifier may be directly in `struct_declarator` or nested inside
/// a `pointer_declarator` or other declarator wrapping node.
fn find_property_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "struct_declaration" {
            // DFS: find the deepest `identifier` that is the declarator name.
            return find_identifier_in(child, src);
        }
    }
    None
}

/// Depth-first search for the first `identifier` node inside `node`.
fn find_identifier_in(node: Node<'_>, src: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(src).ok().map(str::to_owned);
        }
        if let Some(found) = find_identifier_in(child, src) {
            return Some(found);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------


/// Returns true if any direct child of `node` has the given `kind`.
fn node_has_child_kind(node: Node<'_>, kind: &str) -> bool {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).any(|c| c.kind() == kind);
    found
}

/// Return the first `identifier` child of a node (skips keywords like `-`, `+`, `@interface`).
fn first_identifier<'a>(node: Node<'a>, _src: &[u8]) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some(child);
        }
    }
    None
}

/// Iterator over all direct `identifier` children of a node.
struct IdentifierIter<'a> {
    children: Vec<Node<'a>>,
    index: usize,
}

fn identifiers<'a>(node: Node<'a>, _src: &[u8]) -> IdentifierIter<'a> {
    let mut children = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            children.push(child);
        }
    }
    IdentifierIter { children, index: 0 }
}

impl<'a> Iterator for IdentifierIter<'a> {
    type Item = Node<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.children.len() {
            let item = self.children[self.index];
            self.index += 1;
            Some(item)
        } else {
            None
        }
    }
}

fn node_to_range(node: Node<'_>) -> Range {
    let start = node.start_position();
    let end = node.end_position();
    Range {
        start: Position {
            line: start.row as u32,
            character: start.column as u32,
        },
        end: Position {
            line: end.row as u32,
            character: end.column as u32,
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ObjcParser;

    fn parse(src: &str) -> ParsedFile {
        ObjcParser::new().unwrap().parse(src).unwrap()
    }

    fn names(syms: &[DocumentSymbol]) -> Vec<&str> {
        syms.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn finds_class_interface() {
        let file = parse("@interface Foo : NSObject\n@end\n");
        let syms = document_symbols(&file).unwrap();
        assert!(names(&syms).contains(&"Foo"), "{:?}", names(&syms));
    }

    #[test]
    fn finds_simple_method() {
        let file = parse("@interface Foo : NSObject\n- (void)greet;\n@end\n");
        let syms = document_symbols(&file).unwrap();
        let all: Vec<&str> = syms
            .iter()
            .flat_map(|s| {
                std::iter::once(s.name.as_str()).chain(
                    s.children
                        .as_deref()
                        .unwrap_or(&[])
                        .iter()
                        .map(|c| c.name.as_str()),
                )
            })
            .collect();
        assert!(
            all.contains(&"greet"),
            "expected 'greet' in symbols, got: {all:?}"
        );
    }

    #[test]
    fn finds_compound_selector() {
        let src = "@interface Foo : NSObject\n- (id)initWithName:(NSString*)n age:(int)a;\n@end\n";
        let file = parse(src);
        let syms = document_symbols(&file).unwrap();
        let all: Vec<&str> = syms
            .iter()
            .flat_map(|s| {
                std::iter::once(s.name.as_str()).chain(
                    s.children
                        .as_deref()
                        .unwrap_or(&[])
                        .iter()
                        .map(|c| c.name.as_str()),
                )
            })
            .collect();
        assert!(
            all.contains(&"initWithName:age:"),
            "expected 'initWithName:age:' in symbols, got: {all:?}"
        );
    }

    #[test]
    fn finds_property() {
        let src = "@interface Foo : NSObject\n@property (nonatomic) NSString *name;\n@end\n";
        let file = parse(src);
        let syms = document_symbols(&file).unwrap();
        let all: Vec<&str> = syms
            .iter()
            .flat_map(|s| {
                std::iter::once(s.name.as_str()).chain(
                    s.children
                        .as_deref()
                        .unwrap_or(&[])
                        .iter()
                        .map(|c| c.name.as_str()),
                )
            })
            .collect();
        assert!(
            all.contains(&"name"),
            "expected 'name' property in symbols, got: {all:?}"
        );
    }

    #[test]
    fn empty_source_gives_no_symbols() {
        let file = parse("");
        let syms = document_symbols(&file).unwrap();
        assert!(syms.is_empty());
    }

    #[test]
    fn finds_protocol() {
        let src = "@protocol Greetable <NSObject>\n- (void)greet;\n@end\n";
        let file = parse(src);
        let syms = document_symbols(&file).unwrap();
        assert!(names(&syms).contains(&"Greetable"), "{:?}", names(&syms));
    }

    #[test]
    fn category_methods_aggregated_into_base_class() {
        let src = concat!(
            "@interface Person : NSObject\n",
            "- (void)walk;\n",
            "@end\n",
            "@interface Person (Greeting)\n",
            "- (void)sayHello;\n",
            "@end\n",
        );
        let file = parse(src);
        let syms = document_symbols(&file).unwrap();

        // Only one top-level symbol: Person (category merged, not separate).
        assert_eq!(
            syms.iter().filter(|s| s.name == "Person").count(),
            1,
            "expected exactly one 'Person' symbol, got: {:?}",
            names(&syms),
        );

        // The standalone 'Person (Greeting)' MODULE entry must be gone.
        assert!(
            !names(&syms).contains(&"Person (Greeting)"),
            "category symbol should have been merged, got: {:?}",
            names(&syms),
        );

        // 'sayHello' must appear in Person's children.
        let person = syms.iter().find(|s| s.name == "Person").unwrap();
        let child_names: Vec<&str> = person
            .children
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert!(
            child_names.contains(&"sayHello"),
            "expected 'sayHello' in Person children, got: {child_names:?}",
        );
    }

    #[test]
    fn impl_only_no_interface() {
        // A bare @implementation without a matching @interface.
        // The class should still appear as a CLASS symbol.
        let src = "@implementation Bar\n- (void)run {}\n@end\n";
        let file = parse(src);
        let syms = document_symbols(&file).unwrap();
        assert!(names(&syms).contains(&"Bar"), "expected 'Bar' symbol, got: {:?}", names(&syms));
        let bar = syms.iter().find(|s| s.name == "Bar").unwrap();
        let child_names: Vec<&str> = bar
            .children
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert!(child_names.contains(&"run"), "expected 'run' in Bar children, got: {child_names:?}");
    }

    #[test]
    fn multiple_categories_aggregated_into_base() {
        let src = concat!(
            "@interface Animal : NSObject\n",
            "- (void)breathe;\n",
            "@end\n",
            "@interface Animal (Eating)\n",
            "- (void)eat;\n",
            "@end\n",
            "@interface Animal (Moving)\n",
            "- (void)move;\n",
            "@end\n",
        );
        let file = parse(src);
        let syms = document_symbols(&file).unwrap();
        assert_eq!(
            syms.iter().filter(|s| s.name == "Animal").count(),
            1,
            "expected one 'Animal' symbol: {:?}", names(&syms)
        );
        assert!(!names(&syms).contains(&"Animal (Eating)"), "{:?}", names(&syms));
        assert!(!names(&syms).contains(&"Animal (Moving)"), "{:?}", names(&syms));
        let animal = syms.iter().find(|s| s.name == "Animal").unwrap();
        let child_names: Vec<&str> = animal
            .children
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert!(child_names.contains(&"eat"),  "expected 'eat': {child_names:?}");
        assert!(child_names.contains(&"move"), "expected 'move': {child_names:?}");
    }

    #[test]
    fn anonymous_category_extension() {
        let src = concat!(
            "@interface Foo : NSObject\n",
            "@end\n",
            "@interface Foo ()\n",
            "- (void)privateHelper;\n",
            "@end\n",
        );
        let file = parse(src);
        let syms = document_symbols(&file).unwrap();
        let foo_count = syms.iter().filter(|s| s.name == "Foo").count();
        assert!(foo_count >= 1, "expected at least one 'Foo': {:?}", names(&syms));
        assert!(
            !names(&syms).contains(&"Foo ()"),
            "anonymous category should not appear separately: {:?}", names(&syms)
        );
    }

    #[test]
    fn orphan_category_without_base_class_is_kept() {
        let src = "@interface NSString (Utils)\n- (NSString *)trimmed;\n@end\n";
        let file = parse(src);
        let syms = document_symbols(&file).unwrap();
        let has_utils = syms.iter().any(|s| s.name.contains("NSString"));
        assert!(has_utils, "expected NSString-related symbol: {:?}", names(&syms));
    }
}
