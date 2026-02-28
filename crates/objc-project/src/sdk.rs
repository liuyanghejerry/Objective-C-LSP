//! Apple SDK and GNUstep include-path discovery.

use std::path::PathBuf;

/// Detected SDK environment.
#[derive(Debug, Clone)]
pub struct SdkInfo {
    /// Path to the SDK root (e.g. `/Applications/Xcode.app/…/SDKs/iPhoneOS17.0.sdk`).
    pub sdk_root: PathBuf,
    /// Compiler flags that should be prepended to every translation unit.
    pub flags: Vec<String>,
}

/// Attempt to locate the macOS SDK via `xcrun`.
#[cfg(target_os = "macos")]
pub fn find_macos_sdk() -> Option<SdkInfo> {
    let output = std::process::Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?;
    let sdk_root = PathBuf::from(path.trim());

    Some(SdkInfo {
        flags: vec![
            format!("-isysroot"),
            sdk_root.to_string_lossy().into_owned(),
        ],
        sdk_root,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn find_macos_sdk() -> Option<SdkInfo> {
    None
}

/// Attempt to locate GNUstep headers on Linux.
/// Attempt to locate GNUstep headers on Linux / non-macOS.
///
/// Detection strategy (in priority order):
/// 1. `gnustep-config --objc-flags` — the official way
/// 2. `GNUSTEP_SYSTEM_ROOT` environment variable
/// 3. Well-known installation paths: `/usr/share/GNUstep`, `/usr/lib/GNUstep`,
///    `/usr/local/share/GNUstep`
pub fn find_gnustep_flags() -> Option<Vec<String>> {
    // Strategy 1: gnustep-config
    if let Some(flags) = gnustep_config_flags() {
        return Some(flags);
    }

    // Strategy 2: GNUSTEP_SYSTEM_ROOT env var
    if let Some(flags) = gnustep_env_flags() {
        return Some(flags);
    }

    // Strategy 3: well-known paths
    gnustep_fallback_flags()
}

fn gnustep_config_flags() -> Option<Vec<String>> {
    let output = std::process::Command::new("gnustep-config")
        .arg("--objc-flags")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let flags_str = String::from_utf8(output.stdout).ok()?;
    let flags: Vec<String> = flags_str.split_whitespace().map(str::to_owned).collect();
    Some(flags)
}

fn gnustep_env_flags() -> Option<Vec<String>> {
    let root = std::env::var("GNUSTEP_SYSTEM_ROOT").ok()?;
    let root_path = std::path::Path::new(&root);
    build_gnustep_flags(root_path)
}

fn gnustep_fallback_flags() -> Option<Vec<String>> {
    // Common GNUstep installation prefixes.
    let candidates = [
        "/usr/share/GNUstep",
        "/usr/lib/GNUstep",
        "/usr/local/share/GNUstep",
        "/usr/local/lib/GNUstep",
        "/opt/GNUstep/System",
        "/GNUstep/System",
    ];

    for candidate in &candidates {
        let path = std::path::Path::new(candidate);
        if path.exists() {
            if let Some(flags) = build_gnustep_flags(path) {
                return Some(flags);
            }
        }
    }
    None
}

/// Build `-I` include flags for a GNUstep root directory.
///
/// Looks for the standard GNUstep header locations relative to `root`:
/// - `<root>/Headers`
/// - `<root>/Library/Headers`
fn build_gnustep_flags(root: &std::path::Path) -> Option<Vec<String>> {
    let candidates = [
        root.join("Headers"),
        root.join("Library").join("Headers"),
        root.to_path_buf(),
    ];

    let mut flags: Vec<String> = Vec::new();
    for dir in &candidates {
        if dir.exists() {
            flags.push("-I".to_owned());
            flags.push(dir.to_string_lossy().into_owned());
        }
    }

    // Also add the ObjC runtime header path if present.
    let runtime_headers = root.join("Library").join("Headers").join("GNUstepBase");
    if runtime_headers.exists() {
        flags.push("-I".to_owned());
        flags.push(runtime_headers.to_string_lossy().into_owned());
    }

    if flags.is_empty() { None } else { Some(flags) }
}

/// Best-effort list of include flags for the current environment.
pub fn default_include_flags() -> Vec<String> {
    let mut flags = Vec::new();

    if let Some(sdk) = find_macos_sdk() {
        flags.extend(sdk.flags);
    } else if let Some(gnustep) = find_gnustep_flags() {
        flags.extend(gnustep);
    }

    flags
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_gnustep_flags_returns_none_for_missing_dir() {
        let path = std::path::Path::new("/nonexistent/gnustep/root/xyz123");
        assert!(build_gnustep_flags(path).is_none());
    }

    #[test]
    fn build_gnustep_flags_returns_flags_for_existing_dir() {
        // Use a temp directory that has a Headers subdirectory.
        let tmp = tempfile::tempdir().expect("tempdir");
        let headers = tmp.path().join("Headers");
        std::fs::create_dir_all(&headers).unwrap();

        let flags = build_gnustep_flags(tmp.path()).expect("flags");
        assert!(
            flags.iter().any(|f| f.contains("Headers")),
            "flags should include the Headers directory: {flags:?}"
        );
    }

    #[test]
    fn default_include_flags_returns_vec() {
        // Just assert it doesn't panic. On macOS it returns xcrun flags;
        // on CI without Xcode or GNUstep it returns an empty Vec.
        let flags = default_include_flags();
        // flags may be empty — that's fine
        let _ = flags;
    }
}
