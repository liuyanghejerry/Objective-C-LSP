//! Property-coordinated rename.
//!
//! When the user renames a property `foo`, this module rewrites:
//!   - the `@property` declaration name
//!   - the synthesized getter `foo` / custom getter
//!   - the synthesized setter `setFoo:` / custom setter
//!   - the backing ivar `_foo` (if auto-synthesized)
//!   - all dot-syntax accesses (`obj.foo`)
//!   - all explicit message sends (`[obj foo]`, `[obj setFoo:…]`)
//!
//! The implementation delegates the actual reference discovery to
//! `clang_findReferencesInFile` (same machinery as `references.rs`),
//! then groups the results by their cursor kind so each rename variant
//! (getter / setter / ivar) can be given its own substitution string.

use std::collections::HashMap;
use std::ffi::CString;
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{Position, PrepareRenameResponse, Range, TextEdit, Uri, WorkspaceEdit};

use crate::index::ClangIndex;

impl ClangIndex {
    /// Check whether the symbol at `pos` can be renamed.
    ///
    /// Returns `Some(range)` with the current name's range if renameable,
    /// `None` if the cursor is not a renameable symbol.
    pub fn prepare_rename_at(
        &self,
        path: &Path,
        pos: Position,
    ) -> Result<Option<PrepareRenameResponse>> {
        let units = self.units.lock().unwrap();
        let tu = match units.get(path) {
            Some(tu) => *tu,
            None => return Ok(None),
        };

        let cx_file = unsafe { clang_getFile(tu, path_to_cstr(path).as_ptr()) };
        let loc = unsafe { clang_getLocation(tu, cx_file, pos.line + 1, pos.character + 1) };
        let cursor = unsafe { clang_getCursor(tu, loc) };
        if unsafe { clang_Cursor_isNull(cursor) } != 0 {
            return Ok(None);
        }

        let kind = unsafe { clang_getCursorKind(cursor) };

        // Only allow renaming properties, methods, ivars, and plain variables/functions.
        let renameable = matches!(
            kind,
            CXCursor_ObjCPropertyDecl
                | CXCursor_ObjCInstanceMethodDecl
                | CXCursor_ObjCClassMethodDecl
                | CXCursor_ObjCIvarDecl
                | CXCursor_VarDecl
                | CXCursor_FunctionDecl
                | CXCursor_TypedefDecl
        );

        if !renameable {
            return Ok(None);
        }

        let extent = unsafe { clang_getCursorExtent(cursor) };
        let range = cx_range_to_lsp(extent);
        Ok(Some(PrepareRenameResponse::Range(range)))
    }

    /// Rename all occurrences of the symbol at `pos` to `new_name`.
    ///
    /// For `@property` cursors the rename is coordinated: the getter,
    /// setter, and backing ivar are all updated consistently.
    pub fn rename_at(
        &self,
        path: &Path,
        pos: Position,
        new_name: &str,
    ) -> Result<Option<WorkspaceEdit>> {
        let units = self.units.lock().unwrap();
        let tu = match units.get(path) {
            Some(tu) => *tu,
            None => return Ok(None),
        };

        let cx_file = unsafe { clang_getFile(tu, path_to_cstr(path).as_ptr()) };
        let loc = unsafe { clang_getLocation(tu, cx_file, pos.line + 1, pos.character + 1) };
        let cursor = unsafe { clang_getCursor(tu, loc) };
        if unsafe { clang_Cursor_isNull(cursor) } != 0 {
            return Ok(None);
        }

        // Canonicalize to the referenced / definition cursor.
        let referenced = unsafe { clang_getCursorReferenced(cursor) };
        let target = if unsafe { clang_Cursor_isNull(referenced) } != 0 {
            cursor
        } else {
            referenced
        };

        let kind = unsafe { clang_getCursorKind(target) };

        // For @property, compute all derived names.
        let is_property = kind == CXCursor_ObjCPropertyDecl;
        let old_name = cx_string_owned(unsafe { clang_getCursorSpelling(target) });

        // Collect raw reference locations.
        let mut raw_refs: Vec<(CXCursor, CXSourceRange)> = Vec::new();
        let visitor = CXCursorAndRangeVisitor {
            context: &mut raw_refs as *mut Vec<(CXCursor, CXSourceRange)> as *mut _,
            visit: Some(visit_with_cursor),
        };
        unsafe { clang_findReferencesInFile(target, cx_file, visitor) };

        // Build LSP edits.
        let uri: Uri = format!("file://{}", path.to_string_lossy())
            .parse()
            .map_err(|e| anyhow::anyhow!("bad URI: {e}"))?;

        let mut edits: Vec<TextEdit> = Vec::new();

        for (ref_cursor, ref_range) in &raw_refs {
            let ref_kind = unsafe { clang_getCursorKind(*ref_cursor) };
            let replacement = if is_property {
                // Derive appropriate replacement based on the reference kind.
                derive_property_replacement(&old_name, new_name, ref_kind)
            } else {
                new_name.to_owned()
            };

            // Only rewrite the "name" portion, not the full extent.
            // Use the spelling location for precision.
            let start = unsafe { clang_getRangeStart(*ref_range) };
            let end = unsafe { clang_getRangeEnd(*ref_range) };

            let start_pos = spelling_pos(start);
            let end_pos = spelling_pos(end);

            edits.push(TextEdit {
                range: Range {
                    start: start_pos,
                    end: end_pos,
                },
                new_text: replacement,
            });
        }

        if edits.is_empty() {
            return Ok(None);
        }

        let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();
        changes.insert(uri, edits);

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }))
    }
}

// ---------------------------------------------------------------------------
// Property rename helpers
// ---------------------------------------------------------------------------

/// Given the old property name and new name, compute the correct replacement
/// string for a reference of the given cursor kind.
fn derive_property_replacement(old: &str, new: &str, kind: CXCursorKind) -> String {
    match kind {
        // The property declaration and dot-syntax access use the bare name.
        CXCursor_ObjCPropertyDecl | CXCursor_MemberRefExpr => new.to_owned(),

        // Getter method: same as bare name (ObjC default getter == property name).
        CXCursor_ObjCInstanceMethodDecl => {
            // Detect whether this is a getter (no args) or setter.
            // Setters are named `setFoo:` — we check the old spelling.
            let setter_prefix = format!("set{}", capitalize_first(old));
            if old.starts_with(&setter_prefix) || old.ends_with(':') {
                // It's a setter — compute new setter name.
                format!("set{}:", capitalize_first(new))
            } else {
                new.to_owned()
            }
        }

        // Ivar: _foo → _newName
        CXCursor_ObjCIvarDecl => format!("_{new}"),

        // Default: just substitute the new name.
        _ => new.to_owned(),
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

// ---------------------------------------------------------------------------
// libclang helpers
// ---------------------------------------------------------------------------

extern "C" fn visit_with_cursor(
    context: *mut ::std::os::raw::c_void,
    cursor: CXCursor,
    range: CXSourceRange,
) -> CXVisitorResult {
    let refs = unsafe { &mut *(context as *mut Vec<(CXCursor, CXSourceRange)>) };
    refs.push((cursor, range));
    CXVisit_Continue
}

fn spelling_pos(loc: CXSourceLocation) -> Position {
    let mut line: u32 = 0;
    let mut col: u32 = 0;
    unsafe {
        clang_getSpellingLocation(
            loc,
            std::ptr::null_mut(),
            &mut line,
            &mut col,
            std::ptr::null_mut(),
        );
    }
    Position {
        line: line.saturating_sub(1),
        character: col.saturating_sub(1),
    }
}

fn cx_range_to_lsp(range: CXSourceRange) -> Range {
    let start = unsafe { clang_getRangeStart(range) };
    let end = unsafe { clang_getRangeEnd(range) };
    Range {
        start: spelling_pos(start),
        end: spelling_pos(end),
    }
}

fn path_to_cstr(path: &Path) -> CString {
    CString::new(path.to_string_lossy().as_ref()).expect("path must not contain null bytes")
}

fn cx_string_owned(s: CXString) -> String {
    use std::ffi::CStr;
    let result = unsafe {
        CStr::from_ptr(clang_getCString(s))
            .to_string_lossy()
            .into_owned()
    };
    unsafe { clang_disposeString(s) };
    result
}
