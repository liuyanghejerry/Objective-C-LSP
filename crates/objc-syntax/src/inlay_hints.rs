//! Inlay hints for Objective-C message send argument labels.
//!
//! Implements `textDocument/inlayHint` by walking the tree-sitter parse tree
//! for `message_expression` nodes and emitting a parameter-label hint at the
//! start of each argument value.
//!
//! Example: `[obj initWithName:name age:42]`
//! Emits hints `name:` before `name` and `age:` before `42`.

use anyhow::Result;
use lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position, Range};
use tree_sitter::Node;

use crate::parser::ParsedFile;

/// Return inlay hints for all message-send argument labels inside `range`.
///
/// If `range` is `None`, hints for the entire file are returned.
pub fn inlay_hints(file: &ParsedFile, range: Option<Range>) -> Result<Vec<InlayHint>> {
    let src = file.source_bytes();
    let mut hints = Vec::new();
    collect_hints(file.root(), src, range, &mut hints);
    Ok(hints)
}

// ---------------------------------------------------------------------------
// Recursive walker
// ---------------------------------------------------------------------------

fn collect_hints(node: Node<'_>, src: &[u8], range: Option<Range>, out: &mut Vec<InlayHint>) {
    if let Some(r) = range {
        // Skip subtrees entirely outside the requested range.
        let node_end = node.end_position();
        let node_start = node.start_position();
        if node_end.row < r.start.line as usize || node_start.row > r.end.line as usize {
            return;
        }
    }

    if node.kind() == "message_expression" {
        extract_message_hints(node, src, out);
        // Don't return — nested sends (e.g. `[[Foo alloc] initWithName:…]`) are
        // children and will be visited by the recursive walk below.
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_hints(child, src, range, out);
    }
}

/// Extract `keyword:` inlay hints from a single `message_expression` node.
///
/// Grammar (tree-sitter-objc):
///   message_expression = '[' receiver (keyword_argument)+ ']'
///                      | '[' receiver identifier ']'
///
/// keyword_argument = identifier ':' expression
///
/// We want to emit a hint labelled `keyword:` just before the expression.
fn extract_message_hints(node: Node<'_>, src: &[u8], out: &mut Vec<InlayHint>) {
    // tree-sitter-objc represents a message send as a flat sequence of children:
    //   '[' receiver (identifier ':' expr)+ ']'
    // There is NO keyword_argument wrapper node. We scan direct children for the
    // pattern `identifier ':'` and emit a hint before the following expression.
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.children(&mut cursor).collect();

    // Skip '[' (index 0) and receiver (index 1, usually an identifier).
    // Scan from index 2 onwards for keyword:expr pairs.
    let mut i = 2;
    while i < children.len() {
        let child = &children[i];
        if child.kind() == "identifier" {
            // Peek ahead for a ':'
            if let Some(colon) = children.get(i + 1) {
                if colon.kind() == ":" {
                    // The token after ':' is the argument expression.
                    if let Some(expr) = children.get(i + 2) {
                        let kw = child.utf8_text(src).unwrap_or("");
                        let sp = expr.start_position();
                        out.push(InlayHint {
                            position: Position {
                                line: sp.row as u32,
                                character: sp.column as u32,
                            },
                            label: InlayHintLabel::String(format!("{kw}:")),
                            kind: Some(InlayHintKind::PARAMETER),
                            text_edits: None,
                            tooltip: None,
                            padding_left: Some(false),
                            padding_right: Some(true),
                            data: None,
                        });
                        // Advance past identifier + ':' + expr
                        i += 3;
                        continue;
                    }
                }
            }
        }
        i += 1;
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

    #[test]
    fn no_hints_for_nullary_send() {
        // `[obj greet]` has no keyword arguments.
        let file = parse("[obj greet];");
        let hints = inlay_hints(&file, None).unwrap();
        assert!(hints.is_empty(), "expected no hints, got: {hints:?}");
    }

    #[test]
    fn hints_for_compound_send() {
        // `[obj initWithName:n age:42]` should yield hints for `name:` and `age:`.
        let src = "void f(id obj, NSString *n) { [obj initWithName:n age:42]; }";
        let file = parse(src);
        let hints = inlay_hints(&file, None).unwrap();
        let labels: Vec<&str> = hints
            .iter()
            .filter_map(|h| {
                if let InlayHintLabel::String(s) = &h.label {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            labels.contains(&"initWithName:"),
            "expected 'initWithName:' hint, got: {labels:?}"
        );
        assert!(
            labels.contains(&"age:"),
            "expected 'age:' hint, got: {labels:?}"
        );
    }
}
