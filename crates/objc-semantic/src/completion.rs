//! Code completion via libclang.

use std::ffi::{CStr, CString};
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, Position};

use crate::index::ClangIndex;

impl ClangIndex {
    /// Request completions at `pos` in `file`.
    ///
    /// `unsaved_content` is the current (possibly unsaved) buffer content
    /// so clang doesn't need to re-read from disk.
    pub fn completions_at(
        &self,
        path: &Path,
        pos: Position,
        unsaved_content: Option<&str>,
    ) -> Result<Vec<CompletionItem>> {
        use std::mem::MaybeUninit;

        let path_cstr =
            CString::new(path.to_string_lossy().as_ref()).map_err(|e| anyhow::anyhow!("{e}"))?;

        // Build unsaved files array.
        let unsaved_buf;
        let (unsaved_ptr, unsaved_len) = if let Some(content) = unsaved_content {
            unsaved_buf = CXUnsavedFile {
                Filename: path_cstr.as_ptr(),
                Contents: content.as_ptr() as *const i8,
                Length: content.len() as u64,
            };
            (&unsaved_buf as *const _ as *mut _, 1u32)
        } else {
            (std::ptr::null_mut(), 0u32)
        };

        // Clang lines/columns are 1-based.
        let results = unsafe {
            clang_codeCompleteAt(
                // We need to re-parse for completion; use the cached TU's index.
                // For now, pass a fresh parse via the index pointer.
                // A production implementation would use clang_reparseTranslationUnit.
                {
                    let units = self.units.lock().unwrap();
                    match units.get(path) {
                        Some(&tu) => tu,
                        None => return Ok(Vec::new()),
                    }
                },
                path_cstr.as_ptr(),
                pos.line + 1,
                pos.character + 1,
                unsaved_ptr,
                unsaved_len,
                CXCodeComplete_IncludeCodePatterns | CXCodeComplete_IncludeBriefComments,
            )
        };

        if results.is_null() {
            return Ok(Vec::new());
        }

        let num = unsafe { (*results).NumResults };
        let mut items = Vec::with_capacity(num as usize);

        for i in 0..num {
            let result = unsafe { &(*(*results).Results.add(i as usize)) };
            if let Some(item) = cx_completion_to_lsp(result) {
                items.push(item);
            }
        }

        unsafe { clang_disposeCodeCompleteResults(results) };
        Ok(items)
    }
}

fn cx_completion_to_lsp(result: &CXCompletionResult) -> Option<CompletionItem> {
    let string = result.CompletionString;
    if string.is_null() {
        return None;
    }

    let kind = cx_cursor_kind_to_completion_kind(result.CursorKind);

    // Build label from typed-text chunk.
    let num_chunks = unsafe { clang_getNumCompletionChunks(string) };
    let mut label = String::new();
    // snippet holds the insertText in VSCode snippet format.
    let mut snippet = String::new();
    let mut snippet_count: u32 = 0;
    let mut detail = String::new();
    for i in 0..num_chunks {
        let chunk_kind = unsafe { clang_getCompletionChunkKind(string, i) };
        let chunk_text_cx = unsafe { clang_getCompletionChunkText(string, i) };
        let chunk_text = cx_str_to_owned(chunk_text_cx);

        match chunk_kind {
            CXCompletionChunk_TypedText => {
                label.push_str(&chunk_text);
                snippet.push_str(&chunk_text);
            }
            CXCompletionChunk_ResultType => {
                detail = chunk_text;
            }
            CXCompletionChunk_Placeholder => {
                // Convert clang placeholder `<#type#>` → VSCode snippet `${N:type}`.
                let inner = chunk_text
                    .trim_start_matches("<#")
                    .trim_end_matches("#>")
                    .trim();
                snippet_count += 1;
                snippet.push_str(&format!("${{{}:{inner}}}", snippet_count));
                label.push_str(&chunk_text);
            }
            CXCompletionChunk_Text | CXCompletionChunk_Informative => {
                label.push_str(&chunk_text);
                snippet.push_str(&chunk_text);
            }
            CXCompletionChunk_CurrentParameter => {
                label.push_str(&chunk_text);
            }
            _ => {}
        }
    }

    if label.is_empty() {
        return None;
    }

    // Brief comment as documentation.
    let comment_cx = unsafe { clang_getCompletionBriefComment(string) };
    let doc = cx_str_to_owned(comment_cx);
    let documentation = if doc.is_empty() {
        None
    } else {
        Some(Documentation::String(doc))
    };

    Some(CompletionItem {
        label,
        kind: Some(kind),
        detail: if detail.is_empty() { None } else { Some(detail) },
        documentation,
        insert_text: Some(snippet),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    })
}

fn cx_cursor_kind_to_completion_kind(kind: CXCursorKind) -> CompletionItemKind {
    match kind {
        CXCursor_ObjCInstanceMethodDecl | CXCursor_ObjCClassMethodDecl => {
            CompletionItemKind::METHOD
        }
        CXCursor_ObjCPropertyDecl => CompletionItemKind::PROPERTY,
        CXCursor_ObjCInterfaceDecl | CXCursor_ObjCImplementationDecl => CompletionItemKind::CLASS,
        CXCursor_ObjCProtocolDecl => CompletionItemKind::INTERFACE,
        CXCursor_FunctionDecl => CompletionItemKind::FUNCTION,
        CXCursor_VarDecl | CXCursor_ObjCIvarDecl => CompletionItemKind::VARIABLE,
        CXCursor_MacroDefinition => CompletionItemKind::CONSTANT,
        CXCursor_TypedefDecl | CXCursor_StructDecl | CXCursor_EnumDecl => {
            CompletionItemKind::STRUCT
        }
        _ => CompletionItemKind::TEXT,
    }
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
