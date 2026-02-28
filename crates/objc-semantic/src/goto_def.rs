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

    /// Strategy:

    /// 1. `clang_getCursorDefinition` — finds the `@implementation` body for ObjC

    ///    methods, or the struct/function body for C.

    /// 2. Fall back to `clang_getCursorReferenced` — ObjC classes/protocols declared

    ///    in SDK headers have no "definition" in the C++ sense; the reference cursor

    ///    resolves to their `@interface` declaration in the physical `.h` file.

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

            let file = unsafe { clang_getFile(tu, path_cstr.as_ptr()) };

            if file.is_null() {

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



            // 1. Try definition (e.g. @implementation body, function body).

            let def_cursor = unsafe { clang_getCursorDefinition(cursor) };

            if unsafe { clang_Cursor_isNull(def_cursor) } == 0 {

                if let Some(loc) = cursor_to_location(def_cursor)? {

                    return Ok(Some(GotoDefinitionResponse::Scalar(loc)));

                }

            }



            // 2. Fall back to canonical declaration (covers SDK @interface / @protocol

            //    types that have no @implementation in the current TU).

            let decl_cursor = unsafe { clang_getCursorReferenced(cursor) };

            if unsafe { clang_Cursor_isNull(decl_cursor) } != 0 {

                return Ok(None);

            }

            cursor_to_location(decl_cursor).map(|loc| loc.map(GotoDefinitionResponse::Scalar))

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
            let file = unsafe { clang_getFile(tu, path_cstr.as_ptr()) };
            if file.is_null() {
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

            // `clang_getCursorReferenced` gives the canonical declaration.
            let decl_cursor = unsafe { clang_getCursorReferenced(cursor) };
            if unsafe { clang_Cursor_isNull(decl_cursor) } != 0 {
                return Ok(None);
            }

            cursor_to_location(decl_cursor).map(|loc| loc.map(GotoDefinitionResponse::Scalar))
        })
}

}
/// Convert a libclang cursor to an LSP `Location` pointing at its spelling location.

///

/// Uses `clang_getCursorLocation` (the cursor’s canonical source location) rather than

/// the extent range-start.  For SDK declarations resolved via module caches the spelling

/// location resolves to the physical `.h` file; the extent start may point at a PCM binary.

fn cursor_to_location(cursor: CXCursor) -> Result<Option<Location>> {

    // Prefer the cursor’s own spelling location over the extent start.

    let loc = unsafe { clang_getCursorLocation(cursor) };



    let mut file: CXFile = std::ptr::null_mut();

    let mut line: u32 = 0;

    let mut col: u32 = 0;

    unsafe {

        clang_getSpellingLocation(loc, &mut file, &mut line, &mut col, std::ptr::null_mut());

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



    // Derive the selection-range end from the cursor extent, but only when the

    // extent end lives in the same file (not a macro expansion or PCM offset).

    let extent = unsafe { clang_getCursorExtent(cursor) };

    let end_loc = unsafe { clang_getRangeEnd(extent) };

    let mut end_line = line;

    let mut end_col = col;

    let mut end_file: CXFile = std::ptr::null_mut();

    unsafe {

        clang_getSpellingLocation(

            end_loc,

            &mut end_file,

            &mut end_line,

            &mut end_col,

            std::ptr::null_mut(),

        );

    }

    if end_file != file {

        // Extent is in a different file (PCM, macro expansion) — use zero-width range.

        end_line = line;

        end_col = col;

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
    let ptr = unsafe { clang_getCString(s) };
    let result = if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
    };
    unsafe { clang_disposeString(s) };
    result
}
