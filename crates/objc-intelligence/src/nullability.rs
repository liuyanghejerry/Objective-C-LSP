//! Nullability annotation checker.
//!
//! Scans Objective-C header / implementation source for pointer-typed
//! method parameters and return types that lack explicit nullability
//! annotations (`_Nullable`, `_Nonnull`, `nullable`, `nonnull`) **and**
//! are **not** inside an `NS_ASSUME_NONNULL_BEGIN` / `NS_ASSUME_NONNULL_END`
//! region.
//!
//! The check is intentionally lightweight and text-based — it does not
//! require libclang and works on files that may not be on disk yet.
//!
//! # Diagnostics produced
//!
//! | Code | Severity | Message |
//! |------|----------|---------|
//! | `missing-nonnull-region` | Warning | Emitted once per file when there is no `NS_ASSUME_NONNULL_BEGIN` anywhere |
//! | `unannotated-pointer` | Warning | Emitted per parameter / return type that carries a bare `*` without nullability |

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Run the nullability checker on raw source text.
///
/// Returns a (possibly empty) list of LSP [`Diagnostic`]s.
///
/// `extension` should be `"h"`, `"m"`, or `"mm"` — we only warn about
/// `.h` files missing the region macro (`.m` files may legitimately omit it).
pub fn nullability_diagnostics(source: &str, extension: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    let has_region = source.contains("NS_ASSUME_NONNULL_BEGIN");

    // Warn once if a header lacks NS_ASSUME_NONNULL_BEGIN entirely.
    if !has_region && extension == "h" {
        diags.push(Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            },
            severity: Some(DiagnosticSeverity::WARNING),
            code: None,
            code_description: None,
            source: Some("objc-lsp/nullability".to_owned()),
            message: "Header is missing NS_ASSUME_NONNULL_BEGIN/END — all pointer types are \
                      implicitly unaudited. Consider adding NS_ASSUME_NONNULL_BEGIN/END or \
                      annotating each pointer with _Nullable / _Nonnull."
                .to_owned(),
            related_information: None,
            tags: None,
            data: None,
        });
    }

    // Walk line-by-line, tracking whether we are inside a nonnull region.
    // For any method declaration line outside the region that has an
    // unannotated pointer (`*` without a nearby nullability keyword),
    // emit a per-line diagnostic.
    let mut in_nonnull_region = false;
    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.contains("NS_ASSUME_NONNULL_BEGIN") {
            in_nonnull_region = true;
            continue;
        }
        if trimmed.contains("NS_ASSUME_NONNULL_END") {
            in_nonnull_region = false;
            continue;
        }

        // Only check method declarations outside nonnull regions.
        if in_nonnull_region {
            continue;
        }

        // A method declaration line starts with `-` or `+` (instance/class).
        if !trimmed.starts_with('-') && !trimmed.starts_with('+') {
            continue;
        }

        // Check for pointer types (*) without nullability annotation.
        if has_unannotated_pointer(trimmed) {
            // Point at the `*` character for a precise diagnostic location.
            let col = line.find('*').unwrap_or(0) as u32;
            diags.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: line_idx as u32,
                        character: col,
                    },
                    end: Position {
                        line: line_idx as u32,
                        character: col + 1,
                    },
                },
                severity: Some(DiagnosticSeverity::WARNING),
                code: None,
                code_description: None,
                source: Some("objc-lsp/nullability".to_owned()),
                message: "Pointer type lacks nullability annotation (_Nullable or _Nonnull). \
                          Wrap the file with NS_ASSUME_NONNULL_BEGIN/END or annotate explicitly."
                    .to_owned(),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }

    diags
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` if `line` contains a bare `*` (pointer) that is not
/// accompanied by a nullability keyword on the same line.
fn has_unannotated_pointer(line: &str) -> bool {
    if !line.contains('*') {
        return false;
    }
    // If the line already has a nullability annotation, it is considered
    // annotated.
    let nullability_keywords = [
        "_Nullable",
        "_Nonnull",
        "_Null_unspecified",
        "nullable",
        "nonnull",
        "__nullable",
        "__nonnull",
    ];
    !nullability_keywords.iter().any(|kw| line.contains(kw))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // helper: collect message strings
    fn messages(diags: &[Diagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.message.as_str()).collect()
    }

    #[test]
    fn no_diags_for_file_with_nonnull_region() {
        let src = r#"
#import <Foundation/Foundation.h>
NS_ASSUME_NONNULL_BEGIN
@interface Foo : NSObject
- (NSString *)name;
- (void)setItems:(NSArray *)items;
@end
NS_ASSUME_NONNULL_END
"#;
        let diags = nullability_diagnostics(src, "h");
        assert!(
            diags.is_empty(),
            "should have no diagnostics inside nonnull region, got: {:?}",
            messages(&diags)
        );
    }

    #[test]
    fn warns_once_for_header_missing_region() {
        let src = r#"
@interface Bar : NSObject
- (void)doSomething;
@end
"#;
        let diags = nullability_diagnostics(src, "h");
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("NS_ASSUME_NONNULL_BEGIN")),
            "expected region-missing diagnostic, got: {:?}",
            messages(&diags)
        );
    }

    #[test]
    fn no_region_warning_for_m_files() {
        let src = r#"
@implementation Bar
- (void)doSomething {}
@end
"#;
        let diags = nullability_diagnostics(src, "m");
        // Should not warn about missing region in .m files.
        assert!(
            diags
                .iter()
                .all(|d| !d.message.contains("NS_ASSUME_NONNULL_BEGIN")),
            "should not warn about missing region in .m files, got: {:?}",
            messages(&diags)
        );
    }

    #[test]
    fn warns_for_unannotated_pointer_outside_region() {
        let src = r#"
@interface Baz : NSObject
- (NSString *)firstName;
@end
"#;
        let diags = nullability_diagnostics(src, "h");
        // Should have an unannotated pointer warning for the method line.
        let pointer_diags: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("nullability annotation"))
            .collect();
        assert!(
            !pointer_diags.is_empty(),
            "expected unannotated pointer diagnostic, got: {:?}",
            messages(&diags)
        );
    }

    #[test]
    fn no_pointer_warning_when_annotated_nullable() {
        let src = r#"
NS_ASSUME_NONNULL_BEGIN
@interface Qux : NSObject
- (NSString * _Nullable)optionalName;
@end
NS_ASSUME_NONNULL_END
"#;
        let diags = nullability_diagnostics(src, "h");
        assert!(
            diags.is_empty(),
            "should not warn for annotated pointer in nonnull region, got: {:?}",
            messages(&diags)
        );
    }

    #[test]
    fn no_pointer_warning_when_annotated_nonnull() {
        let src = r#"@interface Quux : NSObject
- (NSString * _Nonnull)name;
@end
"#;
        let diags = nullability_diagnostics(src, "h");
        let pointer_diags: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("nullability annotation"))
            .collect();
        assert!(
            pointer_diags.is_empty(),
            "should not warn when _Nonnull is present, got: {:?}",
            messages(&diags)
        );
    }

    #[test]
    fn no_diag_for_non_method_line_with_pointer() {
        let src = r#"
@interface Corge : NSObject
@property (nonatomic, strong) NSString *name;
@end
"#;
        // @property lines don't start with - or +, so no pointer diag should be emitted.
        let diags = nullability_diagnostics(src, "h");
        let pointer_diags: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("nullability annotation"))
            .collect();
        assert!(
            pointer_diags.is_empty(),
            "should not warn for @property lines, got: {:?}",
            messages(&diags)
        );
    }
}
