//! @interface ↔ @implementation navigation.
//!
//! Resolves the "counterpart" of a header or implementation file.
//! Xcode calls this "Open Counterpart" — clangd has no equivalent.

use std::path::{Path, PathBuf};

/// Find the counterpart of a `.h` or `.m` / `.mm` file.
///
/// Strategy:
/// 1. Same directory, change extension.
/// 2. Walk sibling directories (common in large projects where headers
///    live in `include/` and sources in `src/`).
pub fn find_counterpart(file: &Path) -> Option<PathBuf> {
    let ext = file.extension()?.to_str()?;
    let stem = file.file_stem()?;
    let dir = file.parent()?;

    let candidates: &[&str] = match ext {
        "h" => &["m", "mm", "c", "cpp"],
        "m" | "mm" | "c" | "cpp" => &["h"],
        _ => return None,
    };

    // 1. Same directory.
    for target_ext in candidates {
        let candidate = dir.join(stem).with_extension(target_ext);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // 2. Sibling `include/` ↔ `src/` swap.
    if let Some(parent) = dir.parent() {
        let dir_name = dir.file_name()?.to_str()?;
        let sibling = match dir_name {
            "include" | "Include" | "Headers" => Some("src"),
            "src" | "Src" | "Sources" => Some("include"),
            _ => None,
        };
        if let Some(sib) = sibling {
            for target_ext in candidates {
                let candidate = parent.join(sib).join(stem).with_extension(target_ext);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

/// Return `true` if `file` is a header (`.h`).
pub fn is_header(file: &Path) -> bool {
    file.extension().map_or(false, |e| e == "h")
}

/// Return `true` if `file` is an implementation file.
pub fn is_implementation(file: &Path) -> bool {
    file.extension()
        .and_then(|e| e.to_str())
        .map_or(false, |e| matches!(e, "m" | "mm"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // -----------------------------------------------------------------------
    // is_header
    // -----------------------------------------------------------------------

    #[test]
    fn dot_h_is_header() {
        assert!(is_header(Path::new("Foo.h")));
    }

    #[test]
    fn dot_m_is_not_header() {
        assert!(!is_header(Path::new("Foo.m")));
    }

    #[test]
    fn dot_mm_is_not_header() {
        assert!(!is_header(Path::new("Foo.mm")));
    }

    #[test]
    fn no_extension_is_not_header() {
        assert!(!is_header(Path::new("Makefile")));
    }

    // -----------------------------------------------------------------------
    // is_implementation
    // -----------------------------------------------------------------------

    #[test]
    fn dot_m_is_implementation() {
        assert!(is_implementation(Path::new("Foo.m")));
    }

    #[test]
    fn dot_mm_is_implementation() {
        assert!(is_implementation(Path::new("Foo.mm")));
    }

    #[test]
    fn dot_h_is_not_implementation() {
        assert!(!is_implementation(Path::new("Foo.h")));
    }

    #[test]
    fn dot_cpp_is_not_implementation() {
        // .cpp is handled by find_counterpart but NOT by is_implementation
        assert!(!is_implementation(Path::new("Foo.cpp")));
    }

    // -----------------------------------------------------------------------
    // find_counterpart — same directory
    // -----------------------------------------------------------------------

    #[test]
    fn find_counterpart_header_to_impl() {
        let dir = tempfile::tempdir().unwrap();
        let h = dir.path().join("Foo.h");
        let m = dir.path().join("Foo.m");
        std::fs::write(&h, "").unwrap();
        std::fs::write(&m, "").unwrap();
        let found = find_counterpart(&h);
        assert_eq!(found.as_deref(), Some(m.as_path()), "expected Foo.m as counterpart");
    }

    #[test]
    fn find_counterpart_impl_to_header() {
        let dir = tempfile::tempdir().unwrap();
        let h = dir.path().join("Bar.h");
        let m = dir.path().join("Bar.m");
        std::fs::write(&h, "").unwrap();
        std::fs::write(&m, "").unwrap();
        let found = find_counterpart(&m);
        assert_eq!(found.as_deref(), Some(h.as_path()), "expected Bar.h as counterpart");
    }

    #[test]
    fn find_counterpart_returns_none_when_counterpart_missing() {
        let dir = tempfile::tempdir().unwrap();
        let h = dir.path().join("Only.h");
        std::fs::write(&h, "").unwrap();
        // No .m file created.
        assert!(find_counterpart(&h).is_none());
    }

    #[test]
    fn find_counterpart_prefers_dot_mm_over_dot_m() {
        let dir = tempfile::tempdir().unwrap();
        let h = dir.path().join("Widget.h");
        // Only a .mm file exists (no .m).
        let mm = dir.path().join("Widget.mm");
        std::fs::write(&h, "").unwrap();
        std::fs::write(&mm, "").unwrap();
        // .m is first in candidates list, .mm is second — but .m doesn't exist
        // so .mm should be returned.
        let found = find_counterpart(&h);
        assert_eq!(found.as_deref(), Some(mm.as_path()), "expected Widget.mm");
    }

    // -----------------------------------------------------------------------
    // find_counterpart — sibling directories (include/ ↔ src/)
    // -----------------------------------------------------------------------

    #[test]
    fn find_counterpart_include_to_src() {
        let root = tempfile::tempdir().unwrap();
        let inc = root.path().join("include");
        let src = root.path().join("src");
        std::fs::create_dir(&inc).unwrap();
        std::fs::create_dir(&src).unwrap();
        let h = inc.join("Engine.h");
        let m = src.join("Engine.m");
        std::fs::write(&h, "").unwrap();
        std::fs::write(&m, "").unwrap();
        let found = find_counterpart(&h);
        assert_eq!(found.as_deref(), Some(m.as_path()), "expected src/Engine.m");
    }

    #[test]
    fn find_counterpart_src_to_include() {
        let root = tempfile::tempdir().unwrap();
        let inc = root.path().join("include");
        let src = root.path().join("src");
        std::fs::create_dir(&inc).unwrap();
        std::fs::create_dir(&src).unwrap();
        let h = inc.join("Model.h");
        let m = src.join("Model.m");
        std::fs::write(&h, "").unwrap();
        std::fs::write(&m, "").unwrap();
        let found = find_counterpart(&m);
        assert_eq!(found.as_deref(), Some(h.as_path()), "expected include/Model.h");
    }
}
