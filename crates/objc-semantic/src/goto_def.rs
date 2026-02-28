//! Go-to-definition and go-to-declaration via libclang cursors.

use std::ffi::CStr;
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{GotoDefinitionResponse, Location, Position, Range, Uri};

use crate::crash_guard::with_crash_guard;
use crate::index::ClangIndex;

impl ClangIndex {
    /// Return the definition location for the symbol under the cursor.
    ///
    /// Uses `clang_getCursorDefinition` — follows through to the `@implementation`
    /// body for ObjC methods, or the struct/function body for C/C++.
    pub fn definition_at(
        &self,
        path: &Path,
        pos: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let tu = {
            let units = self.units.lock().unwrap();
            match units.get(path) {
                Some(tu) => *tu,
                None => return Ok(None),
            }
        };

        with_crash_guard(|| {
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

            let def_cursor = unsafe { clang_getCursorDefinition(cursor) };
            if unsafe { clang_Cursor_isNull(def_cursor) } != 0 {
                return Ok(None);
            }

            cursor_to_location(def_cursor).map(|loc| loc.map(GotoDefinitionResponse::Scalar))
        })
    }

    /// Return the declaration location for the symbol under the cursor.
    ///
    /// Uses `clang_getCursorReferenced` — for ObjC methods this jumps to the
    /// `@interface` / `@protocol` declaration rather than the `@implementation`.
    pub fn declaration_at(
        &self,
        path: &Path,
        pos: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let tu = {
            let units = self.units.lock().unwrap();
            match units.get(path) {
                Some(tu) => *tu,
                None => return Ok(None),
            }
        };

        with_crash_guard(|| {
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

            // `clang_getCursorReferenced` gives the canonical declaration.
            let decl_cursor = unsafe { clang_getCursorReferenced(cursor) };
            if unsafe { clang_Cursor_isNull(decl_cursor) } != 0 {
                return Ok(None);
            }

            cursor_to_location(decl_cursor).map(|loc| loc.map(GotoDefinitionResponse::Scalar))
        })
}

}
fn cursor_to_location(cursor: CXCursor) -> Result<Option<Location>> {
    let extent = unsafe { clang_getCursorExtent(cursor) };
    let start = unsafe { clang_getRangeStart(extent) };

    let mut line: u32 = 0;
    let mut col: u32 = 0;
    let mut file: CXFile = std::ptr::null_mut();
    unsafe {
        clang_getSpellingLocation(start, &mut file, &mut line, &mut col, std::ptr::null_mut());
    }

    if file.is_null() || line == 0 {
        return Ok(None);
    }

    // Get the file path from the CXFile.
    let cx_filename = unsafe { clang_getFileName(file) };
    let filename = cx_string_owned(cx_filename);
    if filename.is_empty() {
        return Ok(None);
    }

    // Build a file:// URI.
    let uri: Uri = format!("file://{filename}")
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid URI for {filename}: {e}"))?;

    // Also compute end position for the selection range.
    let end_loc = unsafe { clang_getRangeEnd(extent) };
    let mut end_line: u32 = line;
    let mut end_col: u32 = col;
    unsafe {
        clang_getSpellingLocation(
            end_loc,
            std::ptr::null_mut(),
            &mut end_line,
            &mut end_col,
            std::ptr::null_mut(),
        );
    }

    let range = Range {
        start: lsp_types::Position {
            line: line.saturating_sub(1),
            character: col.saturating_sub(1),
        },
        end: lsp_types::Position {
            line: end_line.saturating_sub(1),
            character: end_col.saturating_sub(1),
        },
    };

    Ok(Some(Location { uri, range }))
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
