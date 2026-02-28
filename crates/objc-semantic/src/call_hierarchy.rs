//! Call hierarchy for Objective-C methods and functions.
//!
//! Implements the three LSP methods:
//! - `callHierarchy/prepare` — resolve the symbol at a position
//! - `callHierarchy/incomingCalls` — who calls this method?
//! - `callHierarchy/outgoingCalls` — what does this method call?
//!
//! Uses libclang cursor traversal to find `ObjCMessageExpr` and `CallExpr`
//! nodes in translation units.

use std::collections::HashMap;
use std::ffi::CStr;
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, Position, Range,
    SymbolKind, Uri,
};

use crate::crash_guard::with_crash_guard;
use crate::index::ClangIndex;

impl ClangIndex {
    /// Prepare the call hierarchy item at the given position.
    ///
    /// Returns the `CallHierarchyItem` for the method/function under the cursor,
    /// or `None` if the cursor is not on a callable symbol.
    pub fn call_hierarchy_prepare(
        &self,
        path: &Path,
        pos: Position,
    ) -> Result<Option<CallHierarchyItem>> {
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
            if !is_callable_kind(kind) {
                return Ok(None);
            }

            Ok(cursor_to_call_item(target))
        })
    }

    /// Find incoming calls (callers) of the given call hierarchy item.
    ///
    /// Traverses all cached translation units looking for message sends /
    /// function calls whose referenced cursor matches `item`.
    pub fn call_hierarchy_incoming(
        &self,
        item: &CallHierarchyItem,
    ) -> Result<Vec<CallHierarchyIncomingCall>> {
        let target_name = &item.name;
        let units = self.units.lock().unwrap();
        let mut results: HashMap<String, (CallHierarchyItem, Vec<Range>)> = HashMap::new();

        for (_path, &tu) in units.iter() {
            let root = unsafe { clang_getTranslationUnitCursor(tu) };

            unsafe {
                clang_visitChildren(
                    root,
                    incoming_visitor,
                    &IncomingCtx {
                        target_name: target_name.clone(),
                        results: &mut results as *mut _ as *mut std::ffi::c_void,
                    } as *const IncomingCtx as *mut std::ffi::c_void,
                );
            }
        }

        Ok(results
            .into_values()
            .map(|(from, from_ranges)| CallHierarchyIncomingCall { from, from_ranges })
            .collect())
    }

    /// Find outgoing calls (callees) from the given call hierarchy item.
    ///
    /// Traverses the body of the method/function identified by `item`,
    /// collecting all message sends and function calls.
    pub fn call_hierarchy_outgoing(
        &self,
        item: &CallHierarchyItem,
    ) -> Result<Vec<CallHierarchyOutgoingCall>> {
        // Find the TU and cursor for the item.
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

            let mut callees: HashMap<String, (CallHierarchyItem, Vec<Range>)> = HashMap::new();

            unsafe {
                clang_visitChildren(
                    cursor,
                    outgoing_visitor,
                    &mut callees as *mut _ as *mut std::ffi::c_void,
                );
            }

            Ok(callees
                .into_values()
                .map(|(to, from_ranges)| CallHierarchyOutgoingCall { to, from_ranges })
                .collect())
        })
    }
}

// ---------------------------------------------------------------------------
// Visitor context and callbacks
// ---------------------------------------------------------------------------

struct IncomingCtx {
    target_name: String,
    results: *mut std::ffi::c_void,
}

extern "C" fn incoming_visitor(
    cursor: CXCursor,
    _parent: CXCursor,
    client_data: CXClientData,
) -> CXChildVisitResult {
    let ctx = unsafe { &*(client_data as *const IncomingCtx) };
    let results =
        unsafe { &mut *(ctx.results as *mut HashMap<String, (CallHierarchyItem, Vec<Range>)>) };

    let kind = unsafe { clang_getCursorKind(cursor) };

    // Check for message expressions and call expressions.
    if kind == CXCursor_ObjCMessageExpr || kind == CXCursor_CallExpr {
        let referenced = unsafe { clang_getCursorReferenced(cursor) };
        if unsafe { clang_Cursor_isNull(referenced) } == 0 {
            let ref_name = cursor_spelling(referenced);
            if ref_name == ctx.target_name {
                // Find the containing callable (the "from" side).
                let container = find_enclosing_callable(cursor);
                if let Some(from_item) = container.and_then(|c| cursor_to_call_item(c)) {
                    let call_range = cursor_range(cursor);
                    let key = format!("{}:{}", from_item.name, from_item.uri.as_str());
                    results
                        .entry(key)
                        .and_modify(|(_, ranges)| ranges.push(call_range))
                        .or_insert((from_item, vec![call_range]));
                }
            }
        }
    }

    CXChildVisit_Recurse
}

extern "C" fn outgoing_visitor(
    cursor: CXCursor,
    _parent: CXCursor,
    client_data: CXClientData,
) -> CXChildVisitResult {
    let callees =
        unsafe { &mut *(client_data as *mut HashMap<String, (CallHierarchyItem, Vec<Range>)>) };

    let kind = unsafe { clang_getCursorKind(cursor) };

    if kind == CXCursor_ObjCMessageExpr || kind == CXCursor_CallExpr {
        let referenced = unsafe { clang_getCursorReferenced(cursor) };
        if unsafe { clang_Cursor_isNull(referenced) } == 0 {
            if let Some(to_item) = cursor_to_call_item(referenced) {
                let call_range = cursor_range(cursor);
                let key = format!("{}:{}", to_item.name, to_item.uri.as_str());
                callees
                    .entry(key)
                    .and_modify(|(_, ranges)| ranges.push(call_range))
                    .or_insert((to_item, vec![call_range]));
            }
        }
    }

    CXChildVisit_Recurse
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_callable_kind(kind: CXCursorKind) -> bool {
    matches!(
        kind,
        CXCursor_ObjCInstanceMethodDecl
            | CXCursor_ObjCClassMethodDecl
            | CXCursor_FunctionDecl
            | CXCursor_CXXMethod
    )
}

fn find_enclosing_callable(cursor: CXCursor) -> Option<CXCursor> {
    let mut current = unsafe { clang_getCursorSemanticParent(cursor) };
    loop {
        if unsafe { clang_Cursor_isNull(current) } != 0 {
            return None;
        }
        let kind = unsafe { clang_getCursorKind(current) };
        if is_callable_kind(kind) {
            return Some(current);
        }
        // Stop at translation unit level.
        if kind == CXCursor_TranslationUnit {
            return None;
        }
        current = unsafe { clang_getCursorSemanticParent(current) };
    }
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
fn cursor_to_call_item(cursor: CXCursor) -> Option<CallHierarchyItem> {
    let name = cursor_spelling(cursor);
    if name.is_empty() {
        return None;
    }

    let kind = unsafe { clang_getCursorKind(cursor) };
    let symbol_kind = match kind {
        CXCursor_ObjCInstanceMethodDecl | CXCursor_ObjCClassMethodDecl => SymbolKind::METHOD,
        CXCursor_FunctionDecl => SymbolKind::FUNCTION,
        CXCursor_CXXMethod => SymbolKind::METHOD,
        _ => SymbolKind::FUNCTION,
    };

    // Get the location.
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

    Some(CallHierarchyItem {
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
    fn test_is_callable_kind() {
        assert!(is_callable_kind(CXCursor_ObjCInstanceMethodDecl));
        assert!(is_callable_kind(CXCursor_ObjCClassMethodDecl));
        assert!(is_callable_kind(CXCursor_FunctionDecl));
        assert!(!is_callable_kind(CXCursor_ObjCInterfaceDecl));
        assert!(!is_callable_kind(CXCursor_VarDecl));
    }
}
