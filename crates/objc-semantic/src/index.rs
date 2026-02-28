//! libclang index management.
//!
//! Owns the `CXIndex` and manages per-file translation units.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Result};
use clang_sys::*;

/// RAII wrapper for a `CXIndex`.
pub struct ClangIndex {
    cx: CXIndex,
    /// Cache of parsed translation units keyed by file path.
    pub(crate) units: Arc<Mutex<HashMap<PathBuf, CXTranslationUnit>>>,
}

// Safety: `CXIndex` is a pointer-sized handle; libclang itself is
// thread-safe for read-only operations on translation units.
unsafe impl Send for ClangIndex {}
unsafe impl Sync for ClangIndex {}

impl ClangIndex {
    /// Create a new `CXIndex`.
    ///
    /// `exclude_declarations_from_pch = 1` skips PCH decls (faster),
    /// `display_diagnostics = 0` silences the console.
    pub fn new() -> Result<Self> {
        let cx = unsafe { clang_createIndex(1, 0) };
        if cx.is_null() {
            bail!("clang_createIndex returned null — is libclang installed?");
        }
        Ok(Self {
            cx,
            units: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Parse (or re-parse) a file and return its translation unit.
    ///
    /// `extra_args` should include any flags from `compile_commands.json`
    /// or the Xcode project (e.g. `-isysroot`, `-I`, `-D`).
    pub fn parse_file(&self, path: &Path, extra_args: &[String]) -> Result<()> {
        use std::ffi::CString;

        let path_str = path.to_string_lossy();
        let c_path =
            CString::new(path_str.as_ref()).map_err(|e| anyhow::anyhow!("bad path: {e}"))?;

        // Build argv: ["clang", <extra_args>..., <file>]
        let mut argv_cstrings: Vec<CString> = extra_args
            .iter()
            .filter_map(|a| CString::new(a.as_str()).ok())
            .collect();

        let argv_ptrs: Vec<*const i8> = argv_cstrings.iter().map(|s| s.as_ptr()).collect();

        let flags =
            CXTranslationUnit_DetailedPreprocessingRecord | CXTranslationUnit_SkipFunctionBodies; // faster for indexing

        let tu = unsafe {
            clang_parseTranslationUnit(
                self.cx,
                c_path.as_ptr(),
                argv_ptrs.as_ptr(),
                argv_ptrs.len() as i32,
                std::ptr::null_mut(),
                0,
                flags,
            )
        };

        if tu.is_null() {
            bail!("clang_parseTranslationUnit failed for {:?}", path);
        }

        self.units.lock().unwrap().insert(path.to_path_buf(), tu);
        Ok(())
    }

    /// Dispose a cached translation unit (e.g. when a file is closed).
    pub fn dispose_file(&self, path: &Path) {
        if let Some(tu) = self.units.lock().unwrap().remove(path) {
            unsafe { clang_disposeTranslationUnit(tu) };
        }
    }
}

impl Drop for ClangIndex {
    fn drop(&mut self) {
        // Dispose all cached TUs first.
        let units = self.units.lock().unwrap();
        for tu in units.values() {
            unsafe { clang_disposeTranslationUnit(*tu) };
        }
        drop(units);
        unsafe { clang_disposeIndex(self.cx) };
    }
}

impl Default for ClangIndex {
    fn default() -> Self {
        Self::new().expect("libclang must be available")
    }
}
