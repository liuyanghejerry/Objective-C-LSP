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
pub fn find_gnustep_flags() -> Option<Vec<String>> {
    // Try `gnustep-config --objc-flags`
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
