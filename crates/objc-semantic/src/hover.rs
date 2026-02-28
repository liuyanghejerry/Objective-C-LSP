//! Hover information from libclang cursors.

use std::ffi::CStr;
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::index::ClangIndex;

impl ClangIndex {
    /// Return hover information for the given position in a file.
    pub fn hover_at(&self, path: &Path, pos: Position) -> Result<Option<Hover>> {
        let units = self.units.lock().unwrap();
        let tu = match units.get(path) {
            Some(tu) => *tu,
            None => return Ok(None),
        };

        // Clang positions are 1-based.
        let path_cstr = path_to_cstr(path);
        let location = unsafe {
            clang_getLocation(
                tu,
                clang_getFile(tu, path_cstr.as_ptr()),
                pos.line + 1,
                pos.character + 1,
            )
        };

        let cursor = unsafe { clang_getCursor(tu, location) };
        if unsafe { clang_Cursor_isNull(cursor) } != 0 {
            return Ok(None);
        }

        // Build a markdown string: type spelling + brief comment.
        let mut parts: Vec<String> = Vec::new();

        // Cursor kind as readable label.
        let kind_str = cursor_kind_label(unsafe { clang_getCursorKind(cursor) });

        // Display name (includes selector for ObjC methods).
        let display = cx_string_owned(unsafe { clang_getCursorDisplayName(cursor) });
        if !display.is_empty() {
            parts.push(format!("**{kind_str}** `{display}`"));
        }

        // Type spelling.
        let ty = unsafe { clang_getCursorType(cursor) };
        let ty_str = cx_string_owned(unsafe { clang_getTypeSpelling(ty) });
        if !ty_str.is_empty() && ty_str != "void" {
            parts.push(format!("*Type:* `{ty_str}`"));
        }

        // Brief doc comment from clang (covers Doxygen-style brief).
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
    let result = unsafe {
        CStr::from_ptr(clang_getCString(s))
            .to_string_lossy()
            .into_owned()
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
