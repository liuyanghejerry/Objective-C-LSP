//! Hover information from libclang cursors.

use std::ffi::CStr;
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::crash_guard::with_crash_guard;
use crate::index::ClangIndex;

impl ClangIndex {
    /// Return hover information for the given position in a file.
    pub fn hover_at(&self, path: &Path, pos: Position) -> Result<Option<Hover>> {
        // Extract TU pointer without holding the lock during clang calls.
        // (longjmp past a held Mutex would deadlock.)
        let tu = {
            let units = self.units.lock().unwrap();
            match units.get(path) {
                Some(tu) => *tu,
                None => return Ok(None),
            }
        };

        with_crash_guard(|| {
            // Clang positions are 1-based.
            let path_cstr = path_to_cstr(path);
            let file = unsafe { clang_getFile(tu, path_cstr.as_ptr()) };
            if file.is_null() {
                // TU doesn't know this file (path mismatch) — nothing to hover.
                return Ok(None);
            }
            let location = unsafe {
                clang_getLocation(
                    tu,
                    file,
                    pos.line + 1,
                    pos.character + 1,
                )
            };
            let cursor = unsafe { clang_getCursor(tu, location) };
            if unsafe { clang_Cursor_isNull(cursor) } != 0 {
                return Ok(None);
            }

            // Skip preprocessor / invalid cursors — calling type-spelling or
            // comment-text APIs on macro expansions with undefined macros (from
            // missing headers) causes a SIGSEGV inside libclang.
            let kind = unsafe { clang_getCursorKind(cursor) };
            if kind == CXCursor_InvalidCode || kind == CXCursor_NoDeclFound {
                return Ok(None);
            }
            // Preprocessor cursors: MacroExpansion=103, MacroDefinition=102, InclusionDirective=104
            // MacroExpansion cursors for undefined macros can SIGSEGV inside clang_getCursorDisplayName.
            let is_preprocessor = kind >= 100 && kind <= 110;
            if is_preprocessor {
                return Ok(None);
            }

            // Build a markdown string: type spelling + brief comment.
            let mut parts: Vec<String> = Vec::new();

            // Cursor kind as readable label.
            let kind_str = cursor_kind_label(kind);

            // Display name (includes selector for ObjC methods).
            let display = cx_string_owned(unsafe { clang_getCursorDisplayName(cursor) });
            if !display.is_empty() {
                parts.push(format!("**{kind_str}** `{display}`"));
            }

            // Type spelling — unsafe for preprocessor cursors with undefined macros.
            if !is_preprocessor {
                let ty = unsafe { clang_getCursorType(cursor) };
                // Only spell non-invalid types.
                if ty.kind != CXType_Invalid {
                    let ty_str = cx_string_owned(unsafe { clang_getTypeSpelling(ty) });
                    if !ty_str.is_empty() && ty_str != "void" {
                        parts.push(format!("*Type:* `{ty_str}`"));
                    }
                }
            }
            // Brief doc comment — also unsafe for preprocessor cursors.
            if !is_preprocessor {
                let comment = unsafe { clang_Cursor_getBriefCommentText(cursor) };
                let comment_str = cx_string_owned(comment);
                if !comment_str.is_empty() {
                    parts.push(comment_str);
                } else {
                    // Fall back to the full raw comment text — this covers Apple
                    // HeaderDoc `/*!` blocks and multi-paragraph doc comments.
                    let raw = cx_string_owned(unsafe { clang_Cursor_getRawCommentText(cursor) });
                    let cleaned = clean_raw_comment(&raw);
                    if !cleaned.is_empty() {
                        parts.push(cleaned);
                    }
                }
            }

            if parts.is_empty() {
                return Ok(None);
            }

            Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: parts.join("\n\n"),
                }),
                range: None,
            }))
        })
}

}

fn cursor_kind_label(kind: CXCursorKind) -> &'static str {
    match kind {
        CXCursor_ObjCInterfaceDecl => "@interface",
        CXCursor_ObjCImplementationDecl => "@implementation",
        CXCursor_ObjCProtocolDecl => "@protocol",
        CXCursor_ObjCCategoryDecl => "category",
        CXCursor_ObjCInstanceMethodDecl => "instance method",
        CXCursor_ObjCClassMethodDecl => "class method",
        CXCursor_ObjCPropertyDecl => "@property",
        CXCursor_ObjCIvarDecl => "ivar",
        CXCursor_FunctionDecl => "function",
        CXCursor_VarDecl => "variable",
        CXCursor_TypedefDecl => "typedef",
        CXCursor_StructDecl => "struct",
        CXCursor_EnumDecl => "enum",
        CXCursor_MacroDefinition => "macro",
        _ => "symbol",
    }
}

fn path_to_cstr(path: &Path) -> std::ffi::CString {
    std::ffi::CString::new(path.to_string_lossy().as_ref())
        .expect("path must not contain null bytes")
}

fn cx_string_owned(s: CXString) -> String {
    let ptr = unsafe { clang_getCString(s) };
    let result = if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
    };
    unsafe { clang_disposeString(s) };
    result
}

/// Strip comment delimiters from a raw Clang comment string and return
/// clean prose suitable for Markdown hover rendering.
///
/// Handles all common Apple/Doxygen doc comment styles:
/// - `/*!` … `*/`  (Apple HeaderDoc)
/// - `/**` … `*/`  (Doxygen block)
/// - `///` / `//!` line comments
fn clean_raw_comment(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }

    let lines: Vec<&str> = raw.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());

    for line in &lines {
        let t = line.trim();
        // Strip opening markers.
        let t = t
            .strip_prefix("/*!")
            .or_else(|| t.strip_prefix("/**"))
            .or_else(|| t.strip_prefix("//!"))
            .or_else(|| t.strip_prefix("///"))
            .unwrap_or(t);
        // Strip closing marker.
        let t = t.trim_end_matches("*/").trim();
        // Strip leading ` * ` from block comment body lines.
        let t = t.strip_prefix("* ").unwrap_or(
            t.strip_prefix('*').unwrap_or(t)
        );
        // Convert @param / @return tags to Markdown bold.
        let line_out = convert_doc_tag(t.trim());
        if !line_out.is_empty() {
            out.push(line_out);
        }
    }

    out.join("\n")
}

/// Convert `@param foo desc` / `@return desc` to Markdown-friendly text.
fn convert_doc_tag(line: &str) -> String {
    if let Some(rest) = line.strip_prefix("@param ").or_else(|| line.strip_prefix("\\param ")) {
        let mut parts = rest.splitn(2, ' ');
        let name = parts.next().unwrap_or("");
        let desc = parts.next().unwrap_or("").trim();
        if desc.is_empty() {
            return format!("**Parameter** `{name}`");
        }
        return format!("**Parameter** `{name}`: {desc}");
    }
    if let Some(rest) = line.strip_prefix("@return ").or_else(|| line.strip_prefix("\\return ")) {
        return format!("**Returns**: {}", rest.trim());
    }
    if let Some(rest) = line.strip_prefix("@abstract ") {
        return rest.trim().to_owned();
    }
    if let Some(rest) = line.strip_prefix("@discussion ") {
        return rest.trim().to_owned();
    }
    line.to_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_apple_headerdoc_block() {
        let raw = "/*!\n * @abstract Returns the name.\n * @param index The index.\n * @return The name string.\n */";
        let result = clean_raw_comment(raw);
        assert!(result.contains("Returns the name"), "got: {result}");
        assert!(result.contains("Parameter"), "got: {result}");
        assert!(result.contains("**Returns**"), "got: {result}");
    }

    #[test]
    fn cleans_triple_slash_comments() {
        let raw = "/// First line.\n/// Second line.";
        let result = clean_raw_comment(raw);
        assert!(result.contains("First line"), "got: {result}");
        assert!(result.contains("Second line"), "got: {result}");
    }

    #[test]
    fn empty_raw_returns_empty() {
        assert_eq!(clean_raw_comment(""), "");
    }

    #[test]
    fn converts_param_tag() {
        let result = convert_doc_tag("@param name The name of the object.");
        assert!(result.contains("**Parameter**"), "got: {result}");
        assert!(result.contains("`name`"), "got: {result}");
    }

    #[test]
    fn converts_return_tag() {
        let result = convert_doc_tag("@return The resulting value.");
        assert!(result.contains("**Returns**"), "got: {result}");
    }
}
