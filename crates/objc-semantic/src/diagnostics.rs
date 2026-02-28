//! Clang diagnostics → LSP diagnostics conversion.
//!
//! Two diagnostic sources are supported:
//!   - `objc-lsp` — normal compiler diagnostics from the cached TU.
//!   - `clang-analyzer` — static analysis diagnostics re-parsed with `--analyze`.

use std::ffi::CStr;
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::crash_guard::with_crash_guard;
use crate::index::ClangIndex;
impl ClangIndex {
    /// Collect diagnostics for an already-parsed file.
    ///
    /// Returns an empty `Vec` if the file has not been parsed yet.
    pub fn diagnostics_for(&self, path: &Path) -> Result<Vec<Diagnostic>> {
        let tu = {
            let units = self.units.lock().unwrap();
            match units.get(path) {
                Some(tu) => *tu,
                None => return Ok(Vec::new()),
            }
        };

        with_crash_guard(|| {
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
        })
    }

    /// Run the Clang static analyzer on `path` and return analyzer diagnostics.
    ///
    /// This re-parses the file in a **temporary** TU with `--analyze` appended to
    /// the compile flags, then harvests its diagnostics tagged with source
    /// `"clang-analyzer"`.  The temporary TU is disposed immediately.
    ///
    /// Returns an empty `Vec` if parsing fails (e.g. file not on disk).
    pub fn analyzer_diagnostics_for(
        &self,
        path: &Path,
        extra_args: &[String],
    ) -> Result<Vec<Diagnostic>> {
        use std::ffi::CString;

        let path_str = path.to_string_lossy().into_owned();
        let argv_cstrings: Vec<CString> = {
            let mut v: Vec<CString> = extra_args
                .iter()
                .filter_map(|a| CString::new(a.as_str()).ok())
                .collect();
            if let Ok(a) = CString::new("--analyze") {
                v.push(a);
            }
            v
        };
        let cx = self.cx;

        with_crash_guard(move || {
            let c_path = match CString::new(path_str.as_str()) {
                Ok(p) => p,
                Err(_) => return Ok(Vec::new()),
            };
            let argv_ptrs: Vec<*const i8> = argv_cstrings.iter().map(|s| s.as_ptr()).collect();

            // Parse into a temporary TU — don't cache it.
            let flags = 0; // CXTranslationUnit_None
            let tu = unsafe {
                clang_parseTranslationUnit(
                    cx,
                    c_path.as_ptr(),
                    argv_ptrs.as_ptr(),
                    argv_ptrs.len() as i32,
                    std::ptr::null_mut(),
                    0,
                    flags,
                )
            };
            if tu.is_null() {
                return Ok(Vec::new());
            }

            let num = unsafe { clang_getNumDiagnostics(tu) };
            let mut diags = Vec::with_capacity(num as usize);
            for i in 0..num {
                let cx_diag = unsafe { clang_getDiagnostic(tu, i) };
                if let Some(mut d) = cx_diagnostic_to_lsp(cx_diag) {
                    d.source = Some("clang-analyzer".to_owned());
                    diags.push(d);
                }
                unsafe { clang_disposeDiagnostic(cx_diag) };
            }

            unsafe { clang_disposeTranslationUnit(tu) };
            Ok(diags)
        })
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
