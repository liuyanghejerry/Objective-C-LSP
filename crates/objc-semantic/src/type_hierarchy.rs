//! Type hierarchy for Objective-C classes and protocols.
//!
//! Implements the three LSP methods:
//! - `typeHierarchy/prepare` — resolve the class/protocol at a position
//! - `typeHierarchy/supertypes` — parent classes and adopted protocols
//! - `typeHierarchy/subtypes` — derived classes and protocol adopters
//!
//! Uses libclang cursor traversal to walk the ObjC class hierarchy
//! (`@interface Foo : Bar`) and protocol conformance (`<Proto1, Proto2>`).

use std::ffi::CStr;
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{Position, Range, SymbolKind, TypeHierarchyItem, Uri};

use crate::crash_guard::with_crash_guard;
use crate::index::ClangIndex;

impl ClangIndex {
    /// Prepare the type hierarchy item at the given position.
    ///
    /// Returns a `TypeHierarchyItem` if the cursor is on a class or protocol
    /// declaration/reference.
    pub fn type_hierarchy_prepare(
        &self,
        path: &Path,
        pos: Position,
    ) -> Result<Option<TypeHierarchyItem>> {
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
            let location = unsafe { clang_getLocation(tu, file, pos.line + 1, pos.character + 1) };
            let cursor = unsafe { clang_getCursor(tu, location) };
            if unsafe { clang_Cursor_isNull(cursor) } != 0 {
                return Ok(None);
            }

            // Resolve to the declaration.
            let referenced = unsafe { clang_getCursorReferenced(cursor) };
            let target = if unsafe { clang_Cursor_isNull(referenced) } != 0 {
                cursor
            } else {
                referenced
            };

            let kind = unsafe { clang_getCursorKind(target) };
            if !is_type_kind(kind) {
                return Ok(None);
            }

            Ok(cursor_to_type_item(target))
        })
    }

    /// Find supertypes of the given type hierarchy item.
    ///
    /// For a class: the superclass (e.g. `Foo : Bar` → `Bar`) and adopted protocols.
    /// For a protocol: inherited protocols (e.g. `@protocol Foo <Bar, Baz>` → `Bar`, `Baz`).
    pub fn type_hierarchy_supertypes(
        &self,
        item: &TypeHierarchyItem,
    ) -> Result<Vec<TypeHierarchyItem>> {
        let item_path = uri_to_path(&item.uri);
        let item_path = match item_path {
            Some(p) => p,
            None => return Ok(vec![]),
        };

        let tu = {
            let units = self.units.lock().unwrap();
            match units.get(&item_path) {
                Some(tu) => *tu,
                None => return Ok(vec![]),
            }
        };

        with_crash_guard(|| {
            let path_cstr = path_to_cstr(&item_path);
            let file = unsafe { clang_getFile(tu, path_cstr.as_ptr()) };
            if file.is_null() {
                return Ok(vec![]);
            }

            let start = item.selection_range.start;
            let loc = unsafe { clang_getLocation(tu, file, start.line + 1, start.character + 1) };
            let cursor = unsafe { clang_getCursor(tu, loc) };
            if unsafe { clang_Cursor_isNull(cursor) } != 0 {
                return Ok(vec![]);
            }

            // Resolve reference to declaration.
            let referenced = unsafe { clang_getCursorReferenced(cursor) };
            let target = if unsafe { clang_Cursor_isNull(referenced) } != 0 {
                cursor
            } else {
                referenced
            };

            let mut supertypes: Vec<TypeHierarchyItem> = Vec::new();

            // Visit children to find ObjCSuperClassRef and ObjCProtocolRef.
            unsafe {
                clang_visitChildren(
                    target,
                    supertype_visitor,
                    &mut supertypes as *mut Vec<TypeHierarchyItem> as *mut std::ffi::c_void,
                );
            }

            Ok(supertypes)
        })
    }

    /// Find subtypes of the given type hierarchy item.
    ///
    /// For a class: all classes that directly inherit from it.
    /// For a protocol: all classes/protocols that adopt/inherit it.
    pub fn type_hierarchy_subtypes(
        &self,
        item: &TypeHierarchyItem,
    ) -> Result<Vec<TypeHierarchyItem>> {
        let target_name = &item.name;
        let is_protocol = item.kind == SymbolKind::INTERFACE;
        let units = self.units.lock().unwrap();
        let mut results: Vec<TypeHierarchyItem> = Vec::new();

        for (_path, &tu) in units.iter() {
            let root = unsafe { clang_getTranslationUnitCursor(tu) };
            let mut ctx = SubtypeCtx {
                target_name: target_name.clone(),
                is_protocol,
                results: &mut results,
            };

            unsafe {
                clang_visitChildren(
                    root,
                    subtype_visitor,
                    &mut ctx as *mut SubtypeCtx as *mut std::ffi::c_void,
                );
            }
        }

        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// Visitor callbacks
// ---------------------------------------------------------------------------

extern "C" fn supertype_visitor(
    cursor: CXCursor,
    _parent: CXCursor,
    client_data: CXClientData,
) -> CXChildVisitResult {
    let results = unsafe { &mut *(client_data as *mut Vec<TypeHierarchyItem>) };

    let kind = unsafe { clang_getCursorKind(cursor) };

    match kind {
        CXCursor_ObjCSuperClassRef | CXCursor_ObjCProtocolRef => {
            let referenced = unsafe { clang_getCursorReferenced(cursor) };
            if unsafe { clang_Cursor_isNull(referenced) } == 0 {
                if let Some(item) = cursor_to_type_item(referenced) {
                    results.push(item);
                }
            }
        }
        _ => {}
    }

    // Don't recurse — superclass/protocol refs are direct children.
    CXChildVisit_Continue
}

struct SubtypeCtx<'a> {
    target_name: String,
    is_protocol: bool,
    results: &'a mut Vec<TypeHierarchyItem>,
}

extern "C" fn subtype_visitor(
    cursor: CXCursor,
    _parent: CXCursor,
    client_data: CXClientData,
) -> CXChildVisitResult {
    let ctx = unsafe { &mut *(client_data as *mut SubtypeCtx) };
    let kind = unsafe { clang_getCursorKind(cursor) };

    match kind {
        CXCursor_ObjCInterfaceDecl => {
            // Check if this class's superclass or protocols match the target.
            let mut found = false;
            unsafe {
                clang_visitChildren(
                    cursor,
                    check_supertype_visitor,
                    &CheckSupertypeCtx {
                        target_name: ctx.target_name.clone(),
                        is_protocol: ctx.is_protocol,
                        found: &mut found as *mut bool,
                    } as *const CheckSupertypeCtx as *mut std::ffi::c_void,
                );
            }
            if found {
                if let Some(item) = cursor_to_type_item(cursor) {
                    ctx.results.push(item);
                }
            }
        }
        CXCursor_ObjCProtocolDecl if ctx.is_protocol => {
            // Check if this protocol inherits from the target protocol.
            let mut found = false;
            unsafe {
                clang_visitChildren(
                    cursor,
                    check_supertype_visitor,
                    &CheckSupertypeCtx {
                        target_name: ctx.target_name.clone(),
                        is_protocol: true,
                        found: &mut found as *mut bool,
                    } as *const CheckSupertypeCtx as *mut std::ffi::c_void,
                );
            }
            if found {
                if let Some(item) = cursor_to_type_item(cursor) {
                    ctx.results.push(item);
                }
            }
        }
        _ => {}
    }

    // Only visit top-level declarations, don't recurse into method bodies etc.
    CXChildVisit_Continue
}

struct CheckSupertypeCtx {
    target_name: String,
    is_protocol: bool,
    found: *mut bool,
}

extern "C" fn check_supertype_visitor(
    cursor: CXCursor,
    _parent: CXCursor,
    client_data: CXClientData,
) -> CXChildVisitResult {
    let ctx = unsafe { &*(client_data as *const CheckSupertypeCtx) };

    let kind = unsafe { clang_getCursorKind(cursor) };

    let matches = match (kind, ctx.is_protocol) {
        (CXCursor_ObjCSuperClassRef, false) => {
            let referenced = unsafe { clang_getCursorReferenced(cursor) };
            if unsafe { clang_Cursor_isNull(referenced) } == 0 {
                cursor_spelling(referenced) == ctx.target_name
            } else {
                false
            }
        }
        (CXCursor_ObjCProtocolRef, true) => {
            let referenced = unsafe { clang_getCursorReferenced(cursor) };
            if unsafe { clang_Cursor_isNull(referenced) } == 0 {
                cursor_spelling(referenced) == ctx.target_name
            } else {
                false
            }
        }
        _ => false,
    };

    if matches {
        unsafe { *ctx.found = true };
        return CXChildVisit_Break;
    }

    CXChildVisit_Continue
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_type_kind(kind: CXCursorKind) -> bool {
    matches!(
        kind,
        CXCursor_ObjCInterfaceDecl | CXCursor_ObjCProtocolDecl | CXCursor_ObjCCategoryDecl
    )
}

fn cursor_spelling(cursor: CXCursor) -> String {
    let cx_str = unsafe { clang_getCursorSpelling(cursor) };
    let s = unsafe { CStr::from_ptr(clang_getCString(cx_str)) }
        .to_string_lossy()
        .into_owned();
    unsafe { clang_disposeString(cx_str) };
    s
}

fn cursor_range(cursor: CXCursor) -> Range {
    let extent = unsafe { clang_getCursorExtent(cursor) };
    let start = unsafe { clang_getRangeStart(extent) };
    let end = unsafe { clang_getRangeEnd(extent) };

    let (mut start_line, mut start_col, mut end_line, mut end_col) = (0u32, 0u32, 0u32, 0u32);
    unsafe {
        clang_getSpellingLocation(
            start,
            std::ptr::null_mut(),
            &mut start_line,
            &mut start_col,
            std::ptr::null_mut(),
        );
        clang_getSpellingLocation(
            end,
            std::ptr::null_mut(),
            &mut end_line,
            &mut end_col,
            std::ptr::null_mut(),
        );
    }

    Range {
        start: Position {
            line: start_line.saturating_sub(1),
            character: start_col.saturating_sub(1),
        },
        end: Position {
            line: end_line.saturating_sub(1),
            character: end_col.saturating_sub(1),
        },
    }
}

#[allow(deprecated)]
fn cursor_to_type_item(cursor: CXCursor) -> Option<TypeHierarchyItem> {
    let name = cursor_spelling(cursor);
    if name.is_empty() {
        return None;
    }

    let kind = unsafe { clang_getCursorKind(cursor) };
    let symbol_kind = match kind {
        CXCursor_ObjCInterfaceDecl => SymbolKind::CLASS,
        CXCursor_ObjCProtocolDecl => SymbolKind::INTERFACE,
        CXCursor_ObjCCategoryDecl => SymbolKind::MODULE,
        _ => SymbolKind::CLASS,
    };

    let location = unsafe { clang_getCursorLocation(cursor) };
    let mut file: CXFile = std::ptr::null_mut();
    let mut line = 0u32;
    let mut col = 0u32;
    unsafe {
        clang_getSpellingLocation(
            location,
            &mut file,
            &mut line,
            &mut col,
            std::ptr::null_mut(),
        );
    }

    if file.is_null() {
        return None;
    }

    let file_name_cx = unsafe { clang_getFileName(file) };
    let file_path = unsafe { CStr::from_ptr(clang_getCString(file_name_cx)) }
        .to_string_lossy()
        .into_owned();
    unsafe { clang_disposeString(file_name_cx) };

    let uri: Uri = format!("file://{file_path}").parse().ok()?;
    let range = cursor_range(cursor);
    let sel_range = Range {
        start: Position {
            line: line.saturating_sub(1),
            character: col.saturating_sub(1),
        },
        end: Position {
            line: line.saturating_sub(1),
            character: col.saturating_sub(1) + name.len() as u32,
        },
    };

    Some(TypeHierarchyItem {
        name,
        kind: symbol_kind,
        tags: None,
        detail: None,
        uri,
        range,
        selection_range: sel_range,
        data: None,
    })
}

fn path_to_cstr(path: &Path) -> std::ffi::CString {
    std::ffi::CString::new(path.to_string_lossy().as_ref()).unwrap_or_default()
}

fn uri_to_path(uri: &Uri) -> Option<std::path::PathBuf> {
    let s = uri.as_str();
    let p = s.strip_prefix("file://")?;
    Some(std::path::PathBuf::from(p))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_type_kind() {
        assert!(is_type_kind(CXCursor_ObjCInterfaceDecl));
        assert!(is_type_kind(CXCursor_ObjCProtocolDecl));
        assert!(is_type_kind(CXCursor_ObjCCategoryDecl));
        assert!(!is_type_kind(CXCursor_FunctionDecl));
        assert!(!is_type_kind(CXCursor_VarDecl));
    }
}
