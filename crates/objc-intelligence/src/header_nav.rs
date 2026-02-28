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
