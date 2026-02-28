//! Clang diagnostics → LSP diagnostics conversion.

use std::ffi::CStr;
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::index::ClangIndex;

impl ClangIndex {
    /// Collect diagnostics for an already-parsed file.
    ///
    /// Returns an empty `Vec` if the file has not been parsed yet.
    pub fn diagnostics_for(&self, path: &Path) -> Result<Vec<Diagnostic>> {
        let units = self.units.lock().unwrap();
        let tu = match units.get(path) {
            Some(tu) => *tu,
            None => return Ok(Vec::new()),
        };

        let num = unsafe { clang_getNumDiagnostics(tu) };
        let mut diags = Vec::with_capacity(num as usize);

        for i in 0..num {
            let cx_diag = unsafe { clang_getDiagnostic(tu, i) };
            if let Some(d) = cx_diagnostic_to_lsp(cx_diag) {
                diags.push(d);
            }
            unsafe { clang_disposeDiagnostic(cx_diag) };
        }

        Ok(diags)
    }
}

fn cx_diagnostic_to_lsp(cx: CXDiagnostic) -> Option<Diagnostic> {
    let severity = match unsafe { clang_getDiagnosticSeverity(cx) } {
        CXDiagnostic_Note => DiagnosticSeverity::HINT,
        CXDiagnostic_Warning => DiagnosticSeverity::WARNING,
        CXDiagnostic_Error | CXDiagnostic_Fatal => DiagnosticSeverity::ERROR,
        _ => return None, // CXDiagnostic_Ignored
    };

    let loc = unsafe { clang_getDiagnosticLocation(cx) };
    let range = location_to_range(loc);

    let cx_msg = unsafe { clang_getDiagnosticSpelling(cx) };
    let message = cx_string_to_owned(cx_msg);

    Some(Diagnostic {
        range,
        severity: Some(severity),
        message,
        source: Some("objc-lsp".to_owned()),
        ..Default::default()
    })
}

fn location_to_range(loc: CXSourceLocation) -> Range {
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
    // LSP positions are 0-based; clang is 1-based.
    let pos = Position {
        line: line.saturating_sub(1),
        character: col.saturating_sub(1),
    };
    Range {
        start: pos,
        end: pos,
    }
}

fn cx_string_to_owned(s: CXString) -> String {
    let result = unsafe {
        CStr::from_ptr(clang_getCString(s))
            .to_string_lossy()
            .into_owned()
    };
    unsafe { clang_disposeString(s) };
    result
}
