//! Go-to-implementation for protocols and method declarations.
//!
//! Implements `textDocument/implementation` by:
//!   - For a `@protocol` cursor: finding all `@interface` / `@implementation`
//!     classes that declare conformance.
//!   - For an instance/class method *declaration*: finding the corresponding
//!     `@implementation` method definition.

use std::ffi::{CStr, CString};
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{Location, Position, Range, Uri};

use crate::index::ClangIndex;

impl ClangIndex {
    /// Find implementation locations for the symbol at `pos`.
    ///
    /// Returns `Location` values pointing at concrete implementations:
    /// - `@protocol Foo` → all `@interface X <Foo>` declarations
    /// - method declaration → the method definition in `@implementation`
    pub fn implementations_of(&self, path: &Path, pos: Position) -> Result<Vec<Location>> {
        let units = self.units.lock().unwrap();
        let tu = match units.get(path) {
            Some(tu) => *tu,
            None => return Ok(vec![]),
        };

        let path_cstr = path_to_cstr(path);
        let cx_file = unsafe { clang_getFile(tu, path_cstr.as_ptr()) };
        let loc = unsafe { clang_getLocation(tu, cx_file, pos.line + 1, pos.character + 1) };
        let cursor = unsafe { clang_getCursor(tu, loc) };
        if unsafe { clang_Cursor_isNull(cursor) } != 0 {
            return Ok(vec![]);
        }

        // Canonicalize.
        let referenced = unsafe { clang_getCursorReferenced(cursor) };
        let target = if unsafe { clang_Cursor_isNull(referenced) } != 0 {
            cursor
        } else {
            referenced
        };

        let kind = unsafe { clang_getCursorKind(target) };

        match kind {
            CXCursor_ObjCProtocolDecl => find_protocol_implementors(tu, target),
            CXCursor_ObjCInstanceMethodDecl | CXCursor_ObjCClassMethodDecl => {
                find_method_definition(tu, target)
            }
            _ => {
                // For anything else, fall back to the canonical definition.
                let def = unsafe { clang_getCursorDefinition(target) };
                if unsafe { clang_Cursor_isNull(def) } != 0 {
                    Ok(vec![])
                } else {
                    Ok(cursor_to_location(def).into_iter().collect())
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol implementors
// ---------------------------------------------------------------------------

/// Find all `@interface` declarations that list the given protocol in their
/// conformance list (`@interface Foo <ProtocolName>`).
fn find_protocol_implementors(
    tu: CXTranslationUnit,
    proto_cursor: CXCursor,
) -> Result<Vec<Location>> {
    let proto_name = cx_string_owned(unsafe { clang_getCursorSpelling(proto_cursor) });

    let mut results: Vec<Location> = Vec::new();
    let data: *mut (&str, &mut Vec<Location>) = &mut (proto_name.as_str(), &mut results);

    unsafe {
        clang_visitChildren(
            clang_getTranslationUnitCursor(tu),
            find_implementors_visitor,
            data as *mut _,
        );
    }

    Ok(results)
}

extern "C" fn find_implementors_visitor(
    cursor: CXCursor,
    _parent: CXCursor,
    data: CXClientData,
) -> CXChildVisitResult {
    let pair = unsafe { &mut *(data as *mut (&str, &mut Vec<Location>)) };
    let (proto_name, results) = pair;

    let kind = unsafe { clang_getCursorKind(cursor) };
    if kind != CXCursor_ObjCInterfaceDecl {
        return CXChildVisit_Recurse;
    }

    // Walk children looking for ObjCProtocolRef matching our protocol name.
    let mut found = false;
    let found_ptr = &mut found as *mut bool;
    let proto_name_ptr = *proto_name as *const str;
    // Use a tuple pointer to carry both.
    let inner_data: *mut (*const str, *mut bool) = &mut (proto_name_ptr, found_ptr);

    unsafe {
        clang_visitChildren(cursor, check_protocol_ref, inner_data as CXClientData);
    }

    if found {
        if let Some(loc) = cursor_to_location(cursor) {
            results.push(loc);
        }
    }

    CXChildVisit_Continue
}

extern "C" fn check_protocol_ref(
    cursor: CXCursor,
    _parent: CXCursor,
    data: CXClientData,
) -> CXChildVisitResult {
    let pair = unsafe { &mut *(data as *mut (*const str, *mut bool)) };
    let (proto_name_ptr, found_ptr) = pair;
    let proto_name = unsafe { &**proto_name_ptr };
    let found = unsafe { &mut **found_ptr };

    if unsafe { clang_getCursorKind(cursor) } == CXCursor_ObjCProtocolRef {
        let name = cx_string_owned(unsafe { clang_getCursorSpelling(cursor) });
        if name == proto_name {
            *found = true;
            return CXChildVisit_Break;
        }
    }

    CXChildVisit_Continue
}

// ---------------------------------------------------------------------------
// Method definition finder
// ---------------------------------------------------------------------------

/// For a method *declaration* cursor, find the corresponding *definition*
/// (i.e. the method body in a `@implementation`).
fn find_method_definition(tu: CXTranslationUnit, decl_cursor: CXCursor) -> Result<Vec<Location>> {
    // clang_getCursorDefinition navigates from declaration → definition.
    let def = unsafe { clang_getCursorDefinition(decl_cursor) };
    if unsafe { clang_Cursor_isNull(def) } != 0 {
        // The definition might be in a different TU; return empty for now.
        return Ok(vec![]);
    }

    // If decl == def the cursor already IS the definition; return it.
    Ok(cursor_to_location(def).into_iter().collect())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn cursor_to_location(cursor: CXCursor) -> Option<Location> {
    let extent = unsafe { clang_getCursorExtent(cursor) };
    let start = unsafe { clang_getRangeStart(extent) };

    let mut file: CXFile = std::ptr::null_mut();
    let mut line: u32 = 0;
    let mut col: u32 = 0;
    unsafe {
        clang_getSpellingLocation(start, &mut file, &mut line, &mut col, std::ptr::null_mut());
    }

    if file.is_null() || line == 0 {
        return None;
    }

    let cx_filename = unsafe { clang_getFileName(file) };
    let filename = cx_string_owned(cx_filename);
    if filename.is_empty() {
        return None;
    }

    let uri: Uri = format!("file://{filename}").parse().ok()?;

    Some(Location {
        uri,
        range: Range {
            start: Position {
                line: line.saturating_sub(1),
                character: col.saturating_sub(1),
            },
            end: Position {
                line: line.saturating_sub(1),
                character: col.saturating_sub(1),
            },
        },
    })
}

fn path_to_cstr(path: &Path) -> CString {
    CString::new(path.to_string_lossy().as_ref()).expect("path must not contain null bytes")
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
