//! Find all references to a symbol via libclang.
//!
//! Uses `clang_findReferencesInFile` to walk every occurrence of the
//! symbol under the cursor within a single translation unit's primary file.
//! For cross-file references the caller should iterate over all open TUs.

use std::ffi::{CStr, CString};
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{Location, Position, Range, Uri};

use crate::crash_guard::with_crash_guard;
use crate::index::ClangIndex;
impl ClangIndex {
    /// Find all in-file references to the symbol at `pos` in `path`.
    ///
    /// Returns `Location` values covering every use site found in the same
    /// file's translation unit.  For workspace-wide results the server should
    /// call this once per indexed file that might contain the symbol.
    pub fn references_at(
        &self,
        path: &Path,
        pos: Position,
        include_declaration: bool,
    ) -> Result<Vec<Location>> {
        let tu = {
            let units = self.units.lock().unwrap();
            match units.get(path) {
                Some(tu) => *tu,
                None => return Ok(Vec::new()),
            }
        };

        with_crash_guard(|| {
            // Resolve the cursor at the requested position.
            let path_cstr = path_to_cstr(path);
            let cx_file = unsafe { clang_getFile(tu, path_cstr.as_ptr()) };
            let source_loc = unsafe { clang_getLocation(tu, cx_file, pos.line + 1, pos.character + 1) };
            let cursor = unsafe { clang_getCursor(tu, source_loc) };
            if unsafe { clang_Cursor_isNull(cursor) } != 0 {
                return Ok(Vec::new());
            }

            // Canonicalize to the referenced symbol (handles message sends etc.)
            let referenced = unsafe { clang_getCursorReferenced(cursor) };
            let target = if unsafe { clang_Cursor_isNull(referenced) } != 0 {
                cursor
            } else {
                referenced
            };

            // Collect all reference locations via the visitor callback.
            let mut locations: Vec<Location> = Vec::new();
            let visitor = CXCursorAndRangeVisitor {
                context: &mut locations as *mut Vec<Location> as *mut _,
                visit: Some(visit_reference),
            };

            unsafe { clang_findReferencesInFile(target, cx_file, visitor) };

            if !include_declaration {
                // Remove the declaration itself (the first hit is usually the decl).
                locations.retain(|loc| {
                    // The declaration location matches the target cursor's extent start.
                    let extent = unsafe { clang_getCursorExtent(target) };
                    let start = unsafe { clang_getRangeStart(extent) };
                    let mut decl_line: u32 = 0;
                    let mut decl_col: u32 = 0;
                    unsafe {
                        clang_getSpellingLocation(
                            start,
                            std::ptr::null_mut(),
                            &mut decl_line,
                            &mut decl_col,
                            std::ptr::null_mut(),
                        )
                    };
                    let decl_pos = Position {
                        line: decl_line.saturating_sub(1),
                        character: decl_col.saturating_sub(1),
                    };
                    loc.range.start != decl_pos
                });
            }

            Ok(locations)
        })
}

// ---------------------------------------------------------------------------
// libclang visitor callback
// ---------------------------------------------------------------------------

}

extern "C" fn visit_reference(
    context: *mut ::std::os::raw::c_void,
    cursor: CXCursor,
    range: CXSourceRange,
) -> CXVisitorResult {
    let locations = unsafe { &mut *(context as *mut Vec<Location>) };

    let start = unsafe { clang_getRangeStart(range) };
    let end = unsafe { clang_getRangeEnd(range) };

    let mut file: CXFile = std::ptr::null_mut();
    let mut start_line: u32 = 0;
    let mut start_col: u32 = 0;
    unsafe {
        clang_getSpellingLocation(
            start,
            &mut file,
            &mut start_line,
            &mut start_col,
            std::ptr::null_mut(),
        );
    }

    if file.is_null() || start_line == 0 {
        return CXVisit_Continue;
    }

    let mut end_line: u32 = start_line;
    let mut end_col: u32 = start_col;
    unsafe {
        clang_getSpellingLocation(
            end,
            std::ptr::null_mut(),
            &mut end_line,
            &mut end_col,
            std::ptr::null_mut(),
        );
    }

    let cx_filename = unsafe { clang_getFileName(file) };
    let filename = cx_str_to_owned(cx_filename);
    if filename.is_empty() {
        return CXVisit_Continue;
    }

    let uri: Uri = match format!("file://{filename}").parse() {
        Ok(u) => u,
        Err(_) => return CXVisit_Continue,
    };

    // Suppress the unused variable warning — the cursor is part of the API
    // signature but we only use the range here.
    let _ = cursor;

    locations.push(Location {
        uri,
        range: Range {
            start: Position {
                line: start_line.saturating_sub(1),
                character: start_col.saturating_sub(1),
            },
            end: Position {
                line: end_line.saturating_sub(1),
                character: end_col.saturating_sub(1),
            },
        },
    });

    CXVisit_Continue
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn path_to_cstr(path: &Path) -> CString {
    CString::new(path.to_string_lossy().as_ref()).expect("path must not contain null bytes")
}

fn cx_str_to_owned(s: CXString) -> String {
    let result = unsafe {
        CStr::from_ptr(clang_getCString(s))
            .to_string_lossy()
            .into_owned()
    };
    unsafe { clang_disposeString(s) };
    result
}
