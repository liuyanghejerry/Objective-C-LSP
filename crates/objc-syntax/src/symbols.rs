//! Extracts document symbols from a parsed Objective-C file.
//!
//! Implements `textDocument/documentSymbol` using tree-sitter queries,
//! providing millisecond-latency outlines without requiring a clang
//! compilation unit.

use anyhow::Result;
use streaming_iterator::StreamingIterator;
use lsp_types::{DocumentSymbol, Position, Range, SymbolKind};
use tree_sitter::{Node, Query, QueryCursor};

use crate::parser::ParsedFile;

// tree-sitter query that captures the major ObjC declaration nodes.
const SYMBOLS_QUERY: &str = r#"
; @interface declarations
(class_interface
  name: (type_identifier) @class.name) @class.def

; @implementation blocks
(class_implementation
  name: (type_identifier) @impl.name) @impl.def

; @protocol declarations
(protocol_declaration
  name: (type_identifier) @protocol.name) @protocol.def

; Method declarations inside @interface / @protocol
(method_declaration
  (method_selector) @method.name) @method.def

; Method definitions inside @implementation
(method_definition
  (method_selector) @method_impl.name) @method_impl.def

; @property declarations
(property_declaration
  (type_identifier) @prop.type
  (property_declarator
    (identifier) @prop.name)) @prop.def

; Category interfaces: @interface Foo (CategoryName)
(category_interface
  name: (type_identifier) @cat.class
  category: (identifier) @cat.name) @cat.def
"#;

/// Extract a flat list of document symbols from the parsed file.
///
/// Returns `DocumentSymbol` with nested children where applicable
/// (methods nested under their class/implementation).
pub fn document_symbols(file: &ParsedFile) -> Result<Vec<DocumentSymbol>> {
    let language: tree_sitter::Language = tree_sitter_objc::LANGUAGE.into();
    let query = Query::new(&language, SYMBOLS_QUERY)
        .map_err(|e| anyhow::anyhow!("bad symbols query: {e:?}"))?;

    let mut cursor = QueryCursor::new();
    let src = file.source_bytes();

    // Collect raw matches first, then build hierarchy.
    let mut symbols: Vec<DocumentSymbol> = Vec::new();

    let mut matches = cursor.matches(&query, file.root(), src);
    while let Some(m) = matches.next() {
        // We only process the "container" captures (*.def) here; name
        // captures are read together with their container.
        let capture_names = query.capture_names();

        // Find the def capture in this match.
        let def_cap = m.captures.iter().find(|c| {
            let name = &capture_names[c.index as usize];
            name.ends_with(".def") || name.ends_with(".def")
        });
        let name_cap = m.captures.iter().find(|c| {
            let name = &capture_names[c.index as usize];
            name.ends_with(".name")
        });

        let (Some(def), Some(name_node)) = (def_cap, name_cap) else {
            continue;
        };

        let name_text = name_node.node.utf8_text(src).unwrap_or("<?>").to_owned();

        let cap_name = &capture_names[def.index as usize];
        let kind = capture_name_to_symbol_kind(cap_name);

        let range = node_to_range(def.node);
        let selection_range = node_to_range(name_node.node);

        #[allow(deprecated)]
        symbols.push(DocumentSymbol {
            name: name_text,
            detail: None,
            kind,
            tags: None,
            deprecated: None,
            range,
            selection_range,
            children: None,
        });
    }

    Ok(symbols)
}

fn capture_name_to_symbol_kind(name: &str) -> SymbolKind {
    if name.starts_with("class") || name.starts_with("impl") {
        SymbolKind::CLASS
    } else if name.starts_with("protocol") {
        SymbolKind::INTERFACE
    } else if name.starts_with("method") {
        SymbolKind::METHOD
    } else if name.starts_with("prop") {
        SymbolKind::PROPERTY
    } else if name.starts_with("cat") {
        SymbolKind::MODULE
    } else {
        SymbolKind::OBJECT
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
