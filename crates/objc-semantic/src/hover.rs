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
        let location = unsafe {
            clang_getLocation(
                tu,
                clang_getFile(tu, path_to_cstr(path).as_ptr()),
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

        // Brief doc comment from clang.
        let comment = unsafe { clang_Cursor_getBriefCommentText(cursor) };
        let comment_str = cx_string_owned(comment);
        if !comment_str.is_empty() {
            parts.push(comment_str);
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
