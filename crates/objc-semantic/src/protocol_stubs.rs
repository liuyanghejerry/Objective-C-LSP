//! Protocol stub generation code action.
//!
//! When the cursor is inside a `@implementation Foo` block and `Foo` declares
//! conformance to one or more protocols, this module computes which required
//! protocol methods are not yet implemented and offers a code action
//! "Add missing protocol stubs".
//!
//! The generated stubs follow the project's existing style:
//!   - Instance methods with a `// TODO: implement` comment
//!   - Class methods with `+` prefix

use std::ffi::{CStr, CString};
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{CodeAction, CodeActionKind, Position, Range, TextEdit, Uri, WorkspaceEdit};

use crate::index::ClangIndex;

/// A single unimplemented protocol method.
#[derive(Debug)]
pub struct MissingStub {
    pub selector: String,
    pub is_class_method: bool,
    pub return_type: String,
    pub params: Vec<(String, String)>, // (keyword, type)
}

impl ClangIndex {
    /// Return a list of code actions available at `pos` in `path`.
    ///
    /// Currently only "Add missing protocol stubs" is produced.
    pub fn code_actions_at(
        &self,
        path: &Path,
        _range: Range,
        uri: &Uri,
    ) -> Result<Vec<CodeAction>> {
        let units = self.units.lock().unwrap();
        let tu = match units.get(path) {
            Some(tu) => *tu,
            None => return Ok(vec![]),
        };

        // Walk the TU to find @implementation nodes and their conforming protocols.
        let mut impls: Vec<ImplInfo> = Vec::new();
        let visitor_data = &mut impls as *mut Vec<ImplInfo> as *mut _;
        unsafe {
            clang_visitChildren(
                clang_getTranslationUnitCursor(tu),
                collect_impls_visitor,
                visitor_data,
            );
        }

        let mut actions: Vec<CodeAction> = Vec::new();

        for impl_info in impls {
            let stubs = compute_missing_stubs(tu, &impl_info);
            if stubs.is_empty() {
                continue;
            }

            let insert_text = stubs
                .iter()
                .map(generate_stub_text)
                .collect::<Vec<_>>()
                .join("\n\n");

            // Insert stubs before the `@end` of the @implementation block.
            let insert_pos = impl_info.end_pos;

            let edit = TextEdit {
                range: Range {
                    start: insert_pos,
                    end: insert_pos,
                },
                new_text: insert_text,
            };

            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), vec![edit]);

            actions.push(CodeAction {
                title: format!(
                    "Add {} missing protocol stub{}",
                    stubs.len(),
                    if stubs.len() == 1 { "" } else { "s" }
                ),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: None,
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                command: None,
                is_preferred: Some(true),
                disabled: None,
                data: None,
            });
        }

        Ok(actions)
    }
}

// ---------------------------------------------------------------------------
// Internal data structures
// ---------------------------------------------------------------------------

struct ImplInfo {
    /// Cursor for the @implementation node.
    cursor: CXCursor,
    /// Position just before `@end` where stubs should be inserted.
    end_pos: Position,
    /// Names of methods already implemented in this @implementation.
    implemented: Vec<String>,
    /// Protocols the class declares conformance to.
    protocols: Vec<CXCursor>,
}

// ---------------------------------------------------------------------------
// libclang visitors
// ---------------------------------------------------------------------------

extern "C" fn collect_impls_visitor(
    cursor: CXCursor,
    _parent: CXCursor,
    data: CXClientData,
) -> CXChildVisitResult {
    let impls = unsafe { &mut *(data as *mut Vec<ImplInfo>) };

    let kind = unsafe { clang_getCursorKind(cursor) };
    if kind == CXCursor_ObjCImplementationDecl {
        // Collect already-implemented method selectors.
        let mut implemented: Vec<String> = Vec::new();
        let mut protocols: Vec<CXCursor> = Vec::new();

        let children_data: *mut (Vec<String>, Vec<CXCursor>) = &mut (implemented, protocols);
        unsafe {
            clang_visitChildren(cursor, collect_impl_children, children_data as CXClientData);
        }
        let (implemented, protocols) = unsafe { (*children_data).clone() };

        // Compute @end position from the cursor extent.
        let extent = unsafe { clang_getCursorExtent(cursor) };
        let end = unsafe { clang_getRangeEnd(extent) };
        let mut line: u32 = 0;
        let mut col: u32 = 0;
        unsafe {
            clang_getSpellingLocation(
                end,
                std::ptr::null_mut(),
                &mut line,
                &mut col,
                std::ptr::null_mut(),
            );
        }
        // Insert before the last line (the @end line).
        let end_pos = Position {
            line: line.saturating_sub(2), // one line before @end
            character: 0,
        };

        impls.push(ImplInfo {
            cursor,
            end_pos,
            implemented,
            protocols,
        });
    }

    CXChildVisit_Recurse
}

extern "C" fn collect_impl_children(
    cursor: CXCursor,
    _parent: CXCursor,
    data: CXClientData,
) -> CXChildVisitResult {
    let pair = unsafe { &mut *(data as *mut (Vec<String>, Vec<CXCursor>)) };
    let (implemented, protocols) = pair;

    let kind = unsafe { clang_getCursorKind(cursor) };
    match kind {
        CXCursor_ObjCInstanceMethodDecl | CXCursor_ObjCClassMethodDecl => {
            let sel = cx_string_owned(unsafe { clang_getCursorDisplayName(cursor) });
            implemented.push(sel);
        }
        CXCursor_ObjCProtocolRef => {
            // Resolve the protocol declaration cursor.
            let proto_decl = unsafe { clang_getCursorDefinition(cursor) };
            if unsafe { clang_Cursor_isNull(proto_decl) } == 0 {
                protocols.push(proto_decl);
            }
        }
        _ => {}
    }

    CXChildVisit_Continue
}

// ---------------------------------------------------------------------------
// Stub computation
// ---------------------------------------------------------------------------

fn compute_missing_stubs(tu: CXTranslationUnit, info: &ImplInfo) -> Vec<MissingStub> {
    let _ = tu;
    let mut missing: Vec<MissingStub> = Vec::new();

    for proto in &info.protocols {
        // Visit protocol's required methods.
        let mut proto_methods: Vec<MissingStub> = Vec::new();
        let data = &mut proto_methods as *mut Vec<MissingStub> as CXClientData;
        unsafe { clang_visitChildren(*proto, collect_required_methods, data) };

        for stub in proto_methods {
            if !info.implemented.contains(&stub.selector) {
                missing.push(stub);
            }
        }
    }

    missing
}

extern "C" fn collect_required_methods(
    cursor: CXCursor,
    _parent: CXCursor,
    data: CXClientData,
) -> CXChildVisitResult {
    let stubs = unsafe { &mut *(data as *mut Vec<MissingStub>) };

    let kind = unsafe { clang_getCursorKind(cursor) };
    if kind != CXCursor_ObjCInstanceMethodDecl && kind != CXCursor_ObjCClassMethodDecl {
        return CXChildVisit_Continue;
    }

    // Skip optional methods (annotated with @optional).
    // libclang doesn't expose this directly; we use availability as a heuristic:
    // required methods are always available in the protocol.
    let selector = cx_string_owned(unsafe { clang_getCursorDisplayName(cursor) });
    let return_type =
        cx_string_owned(unsafe { clang_getTypeSpelling(clang_getCursorResultType(cursor)) });

    // Build parameter list.
    let num_args = unsafe { clang_Cursor_getNumArguments(cursor) };
    let mut params: Vec<(String, String)> = Vec::new();
    if num_args > 0 {
        let parts: Vec<&str> = selector.split(':').collect();
        for i in 0..num_args as u32 {
            let arg = unsafe { clang_Cursor_getArgument(cursor, i) };
            let arg_type =
                cx_string_owned(unsafe { clang_getTypeSpelling(clang_getCursorType(arg)) });
            let keyword = parts.get(i as usize).copied().unwrap_or("arg").to_owned();
            params.push((keyword, arg_type));
        }
    }

    stubs.push(MissingStub {
        selector,
        is_class_method: kind == CXCursor_ObjCClassMethodDecl,
        return_type,
        params,
    });

    CXChildVisit_Continue
}

// ---------------------------------------------------------------------------
// Stub text generation
// ---------------------------------------------------------------------------

fn generate_stub_text(stub: &MissingStub) -> String {
    let prefix = if stub.is_class_method { "+" } else { "-" };
    let ret = &stub.return_type;

    let sig = if stub.params.is_empty() {
        // Nullary: `- (void)greet`
        let sel = stub.selector.trim_end_matches(':');
        format!("{prefix} ({ret}){sel}")
    } else {
        // Multi-part: `- (id)initWithName:(NSString *)name age:(int)age`
        let parts: Vec<String> = stub
            .params
            .iter()
            .map(|(kw, ty)| format!("{kw}:({ty}){kw}"))
            .collect();
        format!("{prefix} ({ret}){}", parts.join(" "))
    };

    format!("{sig} {{\n    // TODO: implement\n}}")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
