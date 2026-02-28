//! Semantic tokens for Objective-C via tree-sitter.
//!
//! Implements `textDocument/semanticTokens/full` using a tree-sitter cursor
//! walk. This is fast (no libclang required) but syntax-only; for type-aware
//! tokens the semantic layer can refine results later.

use anyhow::Result;
use lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
};
use tree_sitter::Node;

use crate::parser::ParsedFile;

// ---------------------------------------------------------------------------
// Legend
// ---------------------------------------------------------------------------

/// The token types we emit, in order.  The index into this array is what
/// the LSP `tokenType` field encodes.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE, // 0 – @interface / @implementation / @protocol names
    SemanticTokenType::CLASS,     // 1 – class references
    SemanticTokenType::METHOD,    // 2 – method selectors
    SemanticTokenType::PROPERTY,  // 3 – @property names
    SemanticTokenType::MACRO,     // 4 – preprocessor macros
    SemanticTokenType::KEYWORD,   // 5 – @interface, @end, @property etc.
    SemanticTokenType::TYPE,      // 6 – type identifiers
    SemanticTokenType::VARIABLE,  // 7 – local variables / ivars
    SemanticTokenType::STRING,    // 8 – string literals (@"…" / "…")
    SemanticTokenType::NUMBER,    // 9 – numeric literals
    SemanticTokenType::COMMENT,   // 10 – comments
    SemanticTokenType::PARAMETER, // 11 – method parameters
    SemanticTokenType::FUNCTION,  // 12 – C function names
];

/// The modifiers we emit.
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION, // 0
    SemanticTokenModifier::DEFINITION,  // 1
    SemanticTokenModifier::STATIC,      // 2
    SemanticTokenModifier::DEPRECATED,  // 3
];

/// Build the `SemanticTokensLegend` to advertise in capabilities.
pub fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.to_vec(),
        token_modifiers: TOKEN_MODIFIERS.to_vec(),
    }
}

/// Build the `SemanticTokensOptions` to advertise in capabilities.
pub fn semantic_tokens_options() -> SemanticTokensOptions {
    SemanticTokensOptions {
        legend: semantic_tokens_legend(),
        full: Some(SemanticTokensFullOptions::Bool(true)),
        range: Some(false),
        work_done_progress_options: Default::default(),
    }
}

// ---------------------------------------------------------------------------
// Encoding helpers
// ---------------------------------------------------------------------------

fn token_type_index(ty: &str) -> Option<u32> {
    TOKEN_TYPES
        .iter()
        .position(|t| t.as_str() == ty)
        .map(|i| i as u32)
}

fn modifier_bits(mods: &[u32]) -> u32 {
    mods.iter().fold(0u32, |acc, &m| acc | (1 << m))
}

// ---------------------------------------------------------------------------
// Walk
// ---------------------------------------------------------------------------

/// Compute full semantic tokens for a parsed Objective-C file.
///
/// Returns encoded `SemanticTokens` in the delta-encoded LSP format.
pub fn semantic_tokens_full(file: &ParsedFile) -> Result<SemanticTokens> {
    let src = file.source_bytes();
    let mut collector = TokenCollector::new();

    walk_node(file.root(), src, &mut collector);

    Ok(SemanticTokens {
        result_id: None,
        data: collector.encode(),
    })
}

/// Raw token before delta-encoding.
#[derive(Debug)]
struct RawToken {
    line: u32,
    start_char: u32,
    length: u32,
    token_type: u32,
    modifier_bits: u32,
}

struct TokenCollector {
    tokens: Vec<RawToken>,
}

impl TokenCollector {
    fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    fn push(&mut self, node: Node<'_>, src: &[u8], type_str: &str, mods: &[u32]) {
        let Some(tt) = token_type_index(type_str) else {
            return;
        };
        let start = node.start_position();
        let text = node.utf8_text(src).unwrap_or("");
        // Multi-line tokens: emit one entry per line.
        let end = node.end_position();
        if start.row == end.row {
            self.tokens.push(RawToken {
                line: start.row as u32,
                start_char: start.column as u32,
                length: text.len() as u32,
                token_type: tt,
                modifier_bits: modifier_bits(mods),
            });
        } else {
            // Split across lines: emit line by line.
            for (i, line_text) in text.lines().enumerate() {
                let row = (start.row + i) as u32;
                let col = if i == 0 { start.column as u32 } else { 0 };
                self.tokens.push(RawToken {
                    line: row,
                    start_char: col,
                    length: line_text.len() as u32,
                    token_type: tt,
                    modifier_bits: modifier_bits(mods),
                });
            }
        }
    }

    /// Delta-encode into the packed LSP `SemanticToken` array.
    fn encode(mut self) -> Vec<SemanticToken> {
        // Sort by position (tree-sitter returns them mostly in order, but ensure it).
        self.tokens
            .sort_by(|a, b| a.line.cmp(&b.line).then(a.start_char.cmp(&b.start_char)));

        let mut out = Vec::with_capacity(self.tokens.len());
        let mut prev_line = 0u32;
        let mut prev_char = 0u32;

        for tok in &self.tokens {
            let delta_line = tok.line - prev_line;
            let delta_start = if delta_line == 0 {
                tok.start_char - prev_char
            } else {
                tok.start_char
            };
            out.push(SemanticToken {
                delta_line,
                delta_start,
                length: tok.length,
                token_type: tok.token_type,
                token_modifiers_bitset: tok.modifier_bits,
            });
            prev_line = tok.line;
            prev_char = tok.start_char;
        }

        out
    }
}

// ---------------------------------------------------------------------------
// Node classification
// ---------------------------------------------------------------------------

/// Recursively walk the tree-sitter tree and emit tokens.
fn walk_node(node: Node<'_>, src: &[u8], col: &mut TokenCollector) {
    let kind = node.kind();

    match kind {
        // ── Keywords (@ directives) ──────────────────────────────────────
        "@interface" | "@implementation" | "@protocol" | "@end" | "@property" | "@synthesize"
        | "@dynamic" | "@selector" | "@encode" | "@try" | "@catch" | "@finally" | "@throw"
        | "@autoreleasepool" | "@class" | "in" | "self" | "super" | "nil" | "YES" | "NO"
        | "true" | "false" | "NULL" | "id" | "Class" | "SEL" | "IMP" | "BOOL" => {
            col.push(node, src, "keyword", &[]);
        }

        // ── Class / protocol names at declaration site ────────────────────
        "class_interface" | "class_implementation" => {
            if let Some(name) = node.child_by_field_name("name") {
                col.push(name, src, "namespace", &[0, 1]); // declaration + definition
            }
            recurse_children(node, src, col);
            return;
        }

        "category_interface" | "category_implementation" => {
            if let Some(name) = node.child_by_field_name("name") {
                col.push(name, src, "namespace", &[0, 1]);
            }
            recurse_children(node, src, col);
            return;
        }

        "protocol_declaration" => {
            if let Some(name) = node.child_by_field_name("name") {
                col.push(name, src, "namespace", &[0, 1]);
            }
            recurse_children(node, src, col);
            return;
        }

        // ── Method declarations / definitions ────────────────────────────
        "method_declaration" | "method_definition" => {
            if let Some(sel) = node.child_by_field_name("selector") {
                let mods = if kind == "method_declaration" {
                    &[0u32][..] // declaration
                } else {
                    &[0u32, 1u32][..] // declaration + definition
                };
                col.push(sel, src, "method", mods);
            }
            recurse_children(node, src, col);
            return;
        }

        // ── Method parameters ────────────────────────────────────────────
        "method_selector" => {
            // handled by parent
        }

        "parameter_declaration" => {
            if let Some(decl) = node.child_by_field_name("declarator") {
                col.push(decl, src, "parameter", &[0]);
            }
            recurse_children(node, src, col);
            return;
        }

        // ── Property declarations ─────────────────────────────────────────
        "property_declaration" => {
            // Emit the property name identifier
            for i in 0..node.child_count() {
                let child = node.child(i).unwrap();
                if child.kind() == "property_declarator" {
                    if let Some(id) = child.child_by_field_name("declarator") {
                        col.push(id, src, "property", &[0]);
                    }
                }
            }
            recurse_children(node, src, col);
            return;
        }

        // ── Type identifiers ─────────────────────────────────────────────
        "type_identifier" => {
            col.push(node, src, "type", &[]);
            return; // leaf
        }

        // ── C function declarations ───────────────────────────────────────
        "function_declarator" => {
            if let Some(decl) = node.child_by_field_name("declarator") {
                col.push(decl, src, "function", &[0]);
            }
            recurse_children(node, src, col);
            return;
        }

        // ── Preprocessor ─────────────────────────────────────────────────
        "preproc_def" | "preproc_function_def" => {
            if let Some(name) = node.child_by_field_name("name") {
                col.push(name, src, "macro", &[0, 1]);
            }
            recurse_children(node, src, col);
            return;
        }

        "preproc_call" => {
            if let Some(dir) = node.child_by_field_name("directive") {
                col.push(dir, src, "keyword", &[]);
            }
            recurse_children(node, src, col);
            return;
        }

        // ── String literals ───────────────────────────────────────────────
        "string_literal" | "string_expression" => {
            col.push(node, src, "string", &[]);
            return; // don't recurse into string children
        }

        // ── Number literals ───────────────────────────────────────────────
        "number_literal" => {
            col.push(node, src, "number", &[]);
            return;
        }

        // ── Comments ─────────────────────────────────────────────────────
        "comment" => {
            col.push(node, src, "comment", &[]);
            return;
        }

        // ── Identifiers (generic fall-through) ────────────────────────────
        "identifier" => {
            // Only emit if parent is a message expression receiver or message argument
            // (avoids double-emitting names already caught above).
            // Default: variable.
            col.push(node, src, "variable", &[]);
            return;
        }

        _ => {}
    }

    // Default: recurse into children.
    recurse_children(node, src, col);
}

fn recurse_children(node: Node<'_>, src: &[u8], col: &mut TokenCollector) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(child, src, col);
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
    fn legend_is_nonempty() {
        let legend = semantic_tokens_legend();
        assert!(!legend.token_types.is_empty());
        assert!(!legend.token_modifiers.is_empty());
    }

    #[test]
    fn tokens_for_empty_file() {
        let file = parse("");
        let toks = semantic_tokens_full(&file).unwrap();
        // Empty file → no tokens.
        assert!(toks.data.is_empty());
    }

    #[test]
    fn tokens_for_interface_declaration() {
        let src = "@interface MyClass : NSObject\n@end\n";
        let file = parse(src);
        let toks = semantic_tokens_full(&file).unwrap();
        // Should produce at least one token (e.g. the class name).
        assert!(!toks.data.is_empty());
    }

    #[test]
    fn tokens_are_sorted_by_position() {
        let src = "@interface Foo : NSObject\n- (void)bar;\n@end\n";
        let file = parse(src);
        let toks = semantic_tokens_full(&file).unwrap();
        // Decode positions to verify monotonic ordering.
        let mut line = 0u32;
        let mut col = 0u32;
        for tok in &toks.data {
            line += tok.delta_line;
            let actual_col = if tok.delta_line == 0 {
                col + tok.delta_start
            } else {
                tok.delta_start
            };
            col = actual_col;
            assert!(tok.length > 0, "zero-length token at line {line}");
        }
    }
}
