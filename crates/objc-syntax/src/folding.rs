//! Folding ranges for Objective-C source files.
//!
//! Implements `textDocument/foldingRange` using tree-sitter to identify
//! collapsible regions:
//!
//! - `@interface ... @end` blocks
//! - `@implementation ... @end` blocks
//! - `@protocol ... @end` blocks
//! - Method/function bodies `{ ... }`
//! - Multi-line comments `/* ... */` and `/** ... */`
//! - Consecutive `#import` lines
//! - `#pragma mark` sections
//! - `NS_ASSUME_NONNULL_BEGIN ... NS_ASSUME_NONNULL_END` regions

use anyhow::Result;
use lsp_types::{FoldingRange, FoldingRangeKind};
use tree_sitter::Node;

use crate::parser::ParsedFile;

/// Compute all folding ranges in the parsed file.
pub fn folding_ranges(file: &ParsedFile) -> Result<Vec<FoldingRange>> {
    let src = file.source_bytes();
    let mut ranges = Vec::new();

    collect_folding(file.root(), src, &mut ranges);
    collect_import_folds(src, &mut ranges);
    collect_pragma_mark_folds(src, &mut ranges);
    collect_nonnull_folds(src, &mut ranges);

    // Sort by start line for consistent output.
    ranges.sort_by_key(|r| r.start_line);
    Ok(ranges)
}

// ---------------------------------------------------------------------------
// AST-driven folding (tree-sitter walk)
// ---------------------------------------------------------------------------

fn collect_folding(node: Node<'_>, src: &[u8], out: &mut Vec<FoldingRange>) {
    let kind = node.kind();
    let start_line = node.start_position().row as u32;
    let end_line = node.end_position().row as u32;

    // Only fold multi-line nodes.
    if end_line > start_line {
        match kind {
            // ObjC declaration blocks: fold from first line to @end.
            "class_interface"
            | "class_implementation"
            | "protocol_declaration"
            | "category_interface"
            | "category_implementation" => {
                out.push(FoldingRange {
                    start_line,
                    start_character: None,
                    end_line,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Region),
                    collapsed_text: Some(format!("{}...", kind_display_name(kind))),
                });
            }
            // Method / function bodies.
            "method_definition" | "function_definition" => {
                out.push(FoldingRange {
                    start_line,
                    start_character: None,
                    end_line,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Region),
                    collapsed_text: None,
                });
            }
            // Compound statement (braces).
            "compound_statement" => {
                // Only fold top-level or direct method body compounds
                // (avoid double-folding with method_definition above).
                let parent_kind = node.parent().map(|p| p.kind());
                if parent_kind != Some("method_definition")
                    && parent_kind != Some("function_definition")
                {
                    out.push(FoldingRange {
                        start_line,
                        start_character: None,
                        end_line,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: None,
                    });
                }
            }
            // Multi-line comments.
            "comment" => {
                let text = node_text(node, src);
                if text.starts_with("/*") {
                    out.push(FoldingRange {
                        start_line,
                        start_character: None,
                        end_line,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Comment),
                        collapsed_text: Some("/* ... */".to_owned()),
                    });
                }
            }
            _ => {}
        }
    }

    // Recurse into children.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_folding(child, src, out);
    }
}

// ---------------------------------------------------------------------------
// Line-based folding (import blocks, pragma marks, nonnull regions)
// ---------------------------------------------------------------------------

/// Fold consecutive `#import` / `#include` lines into a single region.
fn collect_import_folds(src: &[u8], out: &mut Vec<FoldingRange>) {
    let text = String::from_utf8_lossy(src);
    let lines: Vec<&str> = text.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("#import") || trimmed.starts_with("#include") {
            let start = i;
            while i + 1 < lines.len() {
                let next = lines[i + 1].trim();
                if next.starts_with("#import") || next.starts_with("#include") {
                    i += 1;
                } else {
                    break;
                }
            }
            // Only fold if there are 2+ consecutive import lines.
            if i > start {
                out.push(FoldingRange {
                    start_line: start as u32,
                    start_character: None,
                    end_line: i as u32,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Imports),
                    collapsed_text: Some("#import ...".to_owned()),
                });
            }
        }
        i += 1;
    }
}

/// Fold `#pragma mark` sections.
///
/// Each `#pragma mark` starts a section that extends to either the next
/// `#pragma mark` or the next `@end` (whichever comes first).
fn collect_pragma_mark_folds(src: &[u8], out: &mut Vec<FoldingRange>) {
    let text = String::from_utf8_lossy(src);
    let lines: Vec<&str> = text.lines().collect();

    let mut pragma_starts: Vec<usize> = Vec::new();
    let mut end_lines: Vec<usize> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("#pragma mark") {
            pragma_starts.push(i);
        }
        if trimmed == "@end" {
            end_lines.push(i);
        }
    }

    for (idx, &start) in pragma_starts.iter().enumerate() {
        // End of this section: next #pragma mark or next @end, whichever is first.
        let next_pragma = pragma_starts.get(idx + 1).copied();
        let next_end = end_lines.iter().find(|&&e| e > start).copied();

        let end = match (next_pragma, next_end) {
            (Some(p), Some(e)) => p.min(e).saturating_sub(1),
            (Some(p), None) => p.saturating_sub(1),
            (None, Some(e)) => e.saturating_sub(1),
            (None, None) => lines.len().saturating_sub(1),
        };

        if end > start {
            out.push(FoldingRange {
                start_line: start as u32,
                start_character: None,
                end_line: end as u32,
                end_character: None,
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: Some(lines[start].trim().to_owned()),
            });
        }
    }
}

/// Fold `NS_ASSUME_NONNULL_BEGIN ... NS_ASSUME_NONNULL_END` regions.
fn collect_nonnull_folds(src: &[u8], out: &mut Vec<FoldingRange>) {
    let text = String::from_utf8_lossy(src);
    let lines: Vec<&str> = text.lines().collect();

    let mut begin_stack: Vec<usize> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == "NS_ASSUME_NONNULL_BEGIN" {
            begin_stack.push(i);
        } else if trimmed == "NS_ASSUME_NONNULL_END" {
            if let Some(start) = begin_stack.pop() {
                out.push(FoldingRange {
                    start_line: start as u32,
                    start_character: None,
                    end_line: i as u32,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Region),
                    collapsed_text: Some("NS_ASSUME_NONNULL...".to_owned()),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn node_text<'a>(node: Node<'a>, src: &'a [u8]) -> &'a str {
    std::str::from_utf8(&src[node.byte_range()]).unwrap_or("")
}

fn kind_display_name(kind: &str) -> &str {
    match kind {
        "class_interface" => "@interface",
        "class_implementation" => "@implementation",
        "protocol_declaration" => "@protocol",
        "category_interface" => "@interface (category)",
        "category_implementation" => "@implementation (category)",
        _ => kind,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ObjcParser;

    fn parse_and_fold(source: &str) -> Vec<FoldingRange> {
        let mut parser = ObjcParser::new().unwrap();
        let parsed = parser.parse(source).unwrap();
        folding_ranges(&parsed).unwrap()
    }

    #[test]
    fn test_interface_fold() {
        let src = "\
@interface Foo : NSObject
- (void)bar;
- (void)baz;
@end
";
        let ranges = parse_and_fold(src);
        assert!(
            ranges.iter().any(|r| r.start_line == 0 && r.end_line == 3),
            "Expected @interface..@end fold: {ranges:?}"
        );
    }

    #[test]
    fn test_implementation_fold() {
        let src = "\
@implementation Foo
- (void)bar {
    NSLog(@\"hello\");
}
@end
";
        let ranges = parse_and_fold(src);
        assert!(
            ranges.iter().any(|r| r.start_line == 0 && r.end_line == 4),
            "Expected @implementation..@end fold: {ranges:?}"
        );
    }

    #[test]
    fn test_method_body_fold() {
        let src = "\
@implementation Foo
- (void)bar {
    NSLog(@\"hello\");
    NSLog(@\"world\");
}
@end
";
        let ranges = parse_and_fold(src);
        // Should have a fold for the method_definition (lines 1-4).
        assert!(
            ranges.iter().any(|r| r.start_line == 1 && r.end_line >= 4),
            "Expected method body fold: {ranges:?}"
        );
    }

    #[test]
    fn test_import_fold() {
        let src = "\
#import <Foundation/Foundation.h>
#import <UIKit/UIKit.h>
#import \"MyClass.h\"

@interface Foo : NSObject
@end
";
        let ranges = parse_and_fold(src);
        assert!(
            ranges.iter().any(|r| r.start_line == 0
                && r.end_line == 2
                && r.kind == Some(FoldingRangeKind::Imports)),
            "Expected #import block fold: {ranges:?}"
        );
    }

    #[test]
    fn test_multiline_comment_fold() {
        let src = "\
/*
 * This is a multi-line comment
 * spanning several lines
 */
@interface Foo : NSObject
@end
";
        let ranges = parse_and_fold(src);
        assert!(
            ranges.iter().any(|r| r.start_line == 0
                && r.end_line == 3
                && r.kind == Some(FoldingRangeKind::Comment)),
            "Expected multiline comment fold: {ranges:?}"
        );
    }

    #[test]
    fn test_nonnull_fold() {
        let src = "\
NS_ASSUME_NONNULL_BEGIN
@interface Foo : NSObject
- (void)bar;
@end
NS_ASSUME_NONNULL_END
";
        let ranges = parse_and_fold(src);
        assert!(
            ranges.iter().any(|r| r.start_line == 0 && r.end_line == 4),
            "Expected NS_ASSUME_NONNULL fold: {ranges:?}"
        );
    }

    #[test]
    fn test_pragma_mark_fold() {
        let src = "\
@implementation Foo
#pragma mark - Lifecycle
- (instancetype)init {
    return self;
}
#pragma mark - Public
- (void)doSomething {
}
@end
";
        let ranges = parse_and_fold(src);
        // Should have at least one pragma mark fold region.
        assert!(
            ranges.iter().any(|r| {
                r.collapsed_text
                    .as_deref()
                    .map_or(false, |t| t.contains("#pragma mark"))
            }),
            "Expected #pragma mark fold: {ranges:?}"
        );
    }

    #[test]
    fn test_protocol_fold() {
        let src = "\
@protocol MyProtocol <NSObject>
- (void)requiredMethod;
@optional
- (void)optionalMethod;
@end
";
        let ranges = parse_and_fold(src);
        assert!(
            ranges.iter().any(|r| r.start_line == 0 && r.end_line == 4),
            "Expected @protocol..@end fold: {ranges:?}"
        );
    }
}
