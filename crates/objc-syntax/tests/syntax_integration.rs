//! Integration tests for `objc-syntax`:
//! - `textDocument/documentSymbol`
//! - `textDocument/semanticTokens/full`
//! - header language detection

use objc_syntax::{
    header_detect::{detect_header_language, HeaderLanguage},
    symbols::document_symbols,
    tokens::{semantic_tokens_full, semantic_tokens_legend, TOKEN_TYPES},
    ObjcParser,
};

fn detect_language(src: &str) -> &'static str {
    use std::path::PathBuf;
    match detect_header_language(&PathBuf::from("test.h"), src) {
        HeaderLanguage::ObjC | HeaderLanguage::ObjCPlusPlus => "objective-c",
        HeaderLanguage::C => "c",
        HeaderLanguage::Cpp => "c++",
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse(src: &str) -> objc_syntax::parser::ParsedFile {
    ObjcParser::new().unwrap().parse(src).unwrap()
}

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read fixture {name}: {e}"))
}

// ─── header_detect ───────────────────────────────────────────────────────────

#[test]
fn header_detect_objc_class() {
    let src = fixture("Person.h");
    assert_eq!(detect_language(&src), "objective-c");
}

#[test]
fn header_detect_protocol() {
    let src = fixture("Robot.h");
    assert_eq!(detect_language(&src), "objective-c");
}

#[test]
fn header_detect_plain_c() {
    let src = "int add(int a, int b) { return a + b; }";
    assert_eq!(detect_language(src), "c");
}

// ─── documentSymbol ──────────────────────────────────────────────────────────

#[test]
fn document_symbol_finds_class() {
    let src = fixture("Person.h");
    let file = parse(&src);
    let syms = document_symbols(&file).unwrap();
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Person"),
        "expected 'Person' in symbols, got: {names:?}"
    );
}

#[test]
fn document_symbol_finds_methods() {
    let src = fixture("Person.h");
    let file = parse(&src);
    let syms = document_symbols(&file).unwrap();
    let all_names: Vec<&str> = syms.iter()
        .flat_map(|s| {
            std::iter::once(s.name.as_str()).chain(
                s.children.as_deref().unwrap_or(&[]).iter().map(|c| c.name.as_str())
            )
        })
        .collect();
    // At least one method selector should appear.
    assert!(
        all_names
            .iter()
            .any(|n| n.contains("greet") || n.contains("initWithName")),
        "expected method symbols, got: {all_names:?}"
    );
}

#[test]
fn document_symbol_finds_protocol() {
    let src = fixture("Robot.h");
    let file = parse(&src);
    let syms = document_symbols(&file).unwrap();
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Greetable") || names.contains(&"Robot"),
        "expected 'Greetable' or 'Robot' in symbols, got: {names:?}"
    );
}

#[test]
fn document_symbol_empty_source() {
    let file = parse("");
    let syms = document_symbols(&file).unwrap();
    assert!(syms.is_empty());
}

#[test]
fn document_symbol_implementation_file() {
    let src = fixture("Person.m");
    let file = parse(&src);
    let syms = document_symbols(&file).unwrap();
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Person"),
        "expected 'Person' implementation in symbols, got: {names:?}"
    );
}

// ─── semanticTokens ──────────────────────────────────────────────────────────

#[test]
fn semantic_tokens_legend_contains_required_types() {
    let legend = semantic_tokens_legend();
    let type_names: Vec<&str> = legend.token_types.iter().map(|t| t.as_str()).collect();
    // LSP requires at least these standard types for ObjC.
    for required in &["class", "method", "property", "keyword"] {
        assert!(
            type_names.contains(required),
            "legend missing token type '{required}', got: {type_names:?}"
        );
    }
}

#[test]
fn semantic_tokens_nonempty_for_interface() {
    let src = fixture("Person.h");
    let file = parse(&src);
    let toks = semantic_tokens_full(&file).unwrap();
    assert!(
        !toks.data.is_empty(),
        "expected semantic tokens for Person.h, got none"
    );
}

#[test]
fn semantic_tokens_nonempty_for_implementation() {
    let src = fixture("Person.m");
    let file = parse(&src);
    let toks = semantic_tokens_full(&file).unwrap();
    assert!(
        !toks.data.is_empty(),
        "expected semantic tokens for Person.m, got none"
    );
}

#[test]
fn semantic_tokens_all_have_positive_length() {
    let src = fixture("Person.h");
    let file = parse(&src);
    let toks = semantic_tokens_full(&file).unwrap();
    for (i, tok) in toks.data.iter().enumerate() {
        assert!(tok.length > 0, "token #{i} has zero length: {tok:?}");
    }
}

#[test]
fn semantic_tokens_positions_are_monotonically_nondecreasing() {
    let src = fixture("Person.h");
    let file = parse(&src);
    let toks = semantic_tokens_full(&file).unwrap();

    // Reconstruct absolute positions from deltas and verify ordering.
    let mut line = 0u32;
    let mut col = 0u32;
    let mut prev_line = 0u32;
    let mut prev_col = 0u32;

    for (i, tok) in toks.data.iter().enumerate() {
        line += tok.delta_line;
        col = if tok.delta_line == 0 {
            col + tok.delta_start
        } else {
            tok.delta_start
        };

        if i > 0 {
            let is_after = (line, col) >= (prev_line, prev_col);
            assert!(
                is_after,
                "token #{i} at ({line},{col}) is before previous ({prev_line},{prev_col})"
            );
        }
        prev_line = line;
        prev_col = col;
    }
}

#[test]
fn semantic_tokens_types_are_valid_indices() {
    let src = fixture("Person.h");
    let file = parse(&src);
    let toks = semantic_tokens_full(&file).unwrap();
    let max_type = TOKEN_TYPES.len() as u32;
    for (i, tok) in toks.data.iter().enumerate() {
        assert!(
            tok.token_type < max_type,
            "token #{i} has out-of-range type {}: max is {max_type}",
            tok.token_type
        );
    }
}
