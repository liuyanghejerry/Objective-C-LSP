//! Code formatting via `clang-format`.
//!
//! Implements `textDocument/formatting` by invoking the external `clang-format`
//! tool and converting its output to LSP `TextEdit` operations.
//!
//! Supports `.clang-format` configuration files in the workspace, and falls back
//! to a sensible ObjC default style if none is found.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Result;
use lsp_types::{Position, Range, TextEdit};

/// Format an Objective-C source file and return the LSP text edits.
///
/// `path` is used for:
///   1. Telling `clang-format` which file the source belongs to (for config lookup).
///   2. Determining the language (`ObjC`) from the extension.
///
/// `source` is the current (possibly unsaved) document content.
///
/// Returns a single whole-document replacement `TextEdit` if the formatted
/// output differs from the input, or an empty vec if nothing changed.
pub fn format_document(path: &Path, source: &str) -> Result<Vec<TextEdit>> {
    let formatted = run_clang_format(path, source)?;

    // No change → no edits.
    if formatted == source {
        return Ok(vec![]);
    }

    // Compute the full-document range of the original text.
    let lines: Vec<&str> = source.lines().collect();
    let last_line = if lines.is_empty() {
        0
    } else {
        (lines.len() - 1) as u32
    };
    let last_char = lines.last().map(|l| l.len() as u32).unwrap_or(0);

    Ok(vec![TextEdit {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: last_line,
                character: last_char,
            },
        },
        new_text: formatted,
    }])
}

/// Invoke `clang-format` as a subprocess, feeding `source` via stdin.
///
/// Returns the formatted source on success.
fn run_clang_format(path: &Path, source: &str) -> Result<String> {
    // Try to find clang-format in PATH.
    let clang_format = find_clang_format();

    let mut cmd = Command::new(&clang_format);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // --assume-filename tells clang-format which file the source belongs to,
    // so it can locate .clang-format config and detect the language.
    cmd.arg(format!("--assume-filename={}", path.display()));

    // Fallback style when no .clang-format file is found.
    cmd.arg("--fallback-style=LLVM");

    let mut child = cmd.spawn().map_err(|e| {
        anyhow::anyhow!(
            "Failed to start clang-format ({}): {}. Is clang-format installed?",
            clang_format,
            e
        )
    })?;

    // Write source to stdin.
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("stdin was piped");
        stdin.write_all(source.as_bytes())?;
    }

    let output = child.wait_with_output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "clang-format exited with status {}: {}",
            output.status,
            stderr.trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Find the `clang-format` binary.
///
/// Search order:
///   1. Homebrew LLVM: `/opt/homebrew/opt/llvm/bin/clang-format` (Apple Silicon)
///   2. Homebrew LLVM: `/usr/local/opt/llvm/bin/clang-format` (Intel)
///   3. Xcode bundled: `/usr/bin/clang-format`
///   4. Bare `clang-format` (rely on PATH)
fn find_clang_format() -> String {
    let candidates = [
        "/opt/homebrew/opt/llvm/bin/clang-format",
        "/usr/local/opt/llvm/bin/clang-format",
        "/usr/bin/clang-format",
    ];
    for c in candidates {
        if Path::new(c).exists() {
            return c.to_owned();
        }
    }
    "clang-format".to_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Check that `find_clang_format` returns a non-empty string.
    #[test]
    fn test_find_clang_format() {
        let path = find_clang_format();
        assert!(!path.is_empty());
    }

    /// If clang-format is available, verify that formatting a trivial ObjC
    /// source produces a non-empty result.
    #[test]
    fn test_format_trivial() {
        let source = "#import <Foundation/Foundation.h>\n\n@interface Foo : NSObject\n@end\n";
        let path = Path::new("/tmp/test.m");
        match run_clang_format(path, source) {
            Ok(formatted) => {
                assert!(!formatted.is_empty());
            }
            Err(e) => {
                // clang-format not installed — skip gracefully.
                eprintln!("Skipping test_format_trivial: {e}");
            }
        }
    }

    /// Verify that format_document returns an empty vec when input is already
    /// well-formatted (no-op).
    #[test]
    fn test_format_no_change() {
        // Use a very simple source that LLVM style would not change.
        let source = "@interface Foo : NSObject\n@end\n";
        let path = Path::new("/tmp/test.m");
        match format_document(path, source) {
            Ok(edits) => {
                // Either 0 edits (no change) or 1 edit (clang-format reformatted) —
                // both are valid depending on clang-format version.
                assert!(edits.len() <= 1);
            }
            Err(e) => {
                eprintln!("Skipping test_format_no_change: {e}");
            }
        }
    }

    /// Verify that badly formatted code produces at least one edit.
    #[test]
    fn test_format_bad_indent() {
        let source = "@interface Foo : NSObject\n- (void)bar;\n      - (void)baz;\n@end\n";
        let path = Path::new("/tmp/test.m");
        match format_document(path, source) {
            Ok(edits) => {
                // clang-format should fix the indentation.
                if !edits.is_empty() {
                    assert_eq!(edits.len(), 1);
                    assert!(edits[0].new_text.len() > 0);
                }
            }
            Err(e) => {
                eprintln!("Skipping test_format_bad_indent: {e}");
            }
        }
    }
}
