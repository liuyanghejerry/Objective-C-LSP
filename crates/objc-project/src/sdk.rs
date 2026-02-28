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

/// Best-effort list of include flags for the current environment (macOS default).
pub fn default_include_flags() -> Vec<String> {
    let mut flags = Vec::new();

    if let Some(sdk) = find_macos_sdk() {
        flags.extend(sdk.flags);
    } else if let Some(gnustep) = find_gnustep_flags() {
        flags.extend(gnustep);
    }

    flags
}

/// Detect the target platform from a workspace root and return the best set
/// of compiler flags (SDK sysroot + CocoaPods headers + GNUstep fallback).
///
/// Detection priority:
/// 1. `compile_commands.json` is preferred (caller handles that via FlagResolver).
/// 2. `Podfile` with `platform :ios` → iPhoneSimulator SDK
/// 3. `.xcodeproj` SDKROOT hint → iPhoneSimulator or macOS SDK
/// 4. macOS SDK fallback
#[cfg(target_os = "macos")]
pub fn workspace_include_flags(workspace_root: Option<&std::path::Path>) -> Vec<String> {
    let mut flags = Vec::new();

    // Always inject the clang built-in resource dir first so that stdarg.h,
    // stdbool.h, etc. are found regardless of the active SDK.
    if let Some(res_dir) = find_clang_resource_dir() {
        flags.push("-resource-dir".to_owned());
        flags.push(res_dir);
    }

    // Detect iOS vs macOS from Podfile / xcodeproj.
    let is_ios = workspace_root.map(detect_ios_project).unwrap_or(false);

    if is_ios {
        if let Some(sdk) = find_ios_simulator_sdk() {
            flags.extend(sdk.flags);
        } else if let Some(sdk) = find_macos_sdk() {
            // Fallback: macOS SDK is better than nothing.
            flags.extend(sdk.flags);
        }
    } else if let Some(sdk) = find_macos_sdk() {
        flags.extend(sdk.flags);
    } else if let Some(gnustep) = find_gnustep_flags() {
        flags.extend(gnustep);
    }

    // Add CocoaPods public headers if Pods/ directory exists.
    if let Some(root) = workspace_root {
        flags.extend(cocoapods_flags(root));
    }

    flags
}

#[cfg(not(target_os = "macos"))]
pub fn workspace_include_flags(workspace_root: Option<&std::path::Path>) -> Vec<String> {
    let mut flags = Vec::new();
    if let Some(gnustep) = find_gnustep_flags() {
        flags.extend(gnustep);
    }
    if let Some(root) = workspace_root {
        flags.extend(cocoapods_flags(root));
    }
    flags
}

/// Detect the clang resource directory that should be passed as `-resource-dir`.
/// This is needed so that libclang can find its own built-in headers
/// (e.g. `stdarg.h`, `stdbool.h`) even when the SDK sysroot doesn't include them.
///
/// Detection priority:
/// 1. `clang --print-resource-dir` via xcrun (picks up Xcode's bundled clang)
/// 2. Well-known Xcode toolchain path (hardcoded fallback)
#[cfg(target_os = "macos")]
pub fn find_clang_resource_dir() -> Option<String> {
    // Strategy 1: ask xcrun / clang directly.
    let output = std::process::Command::new("xcrun")
        .args(["clang", "--print-resource-dir"])
        .output()
        .ok()?;
    if output.status.success() {
        let path = String::from_utf8(output.stdout).ok()?;
        let path = path.trim().to_owned();
        if !path.is_empty() && std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }

    // Strategy 2: well-known Xcode toolchain path.
    let candidates = [
        "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/clang/17.0.0",
        "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/clang/17",
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return Some(c.to_string());
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
pub fn find_clang_resource_dir() -> Option<String> {
    None
}

/// Attempt to locate the iPhone Simulator SDK via `xcrun`.
/// Using the simulator SDK avoids needing a connected device.
#[cfg(target_os = "macos")]
pub fn find_ios_simulator_sdk() -> Option<SdkInfo> {
    let output = std::process::Command::new("xcrun")
        .args(["--sdk", "iphonesimulator", "--show-sdk-path"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?;
    let sdk_root = PathBuf::from(path.trim());

    Some(SdkInfo {
        flags: vec![
            "-isysroot".to_owned(),
            sdk_root.to_string_lossy().into_owned(),
            // iOS simulator arch
            "-arch".to_owned(),
            "arm64".to_owned(),
            // Suppress warnings about non-portable Apple-specific pragmas
            "-Wno-unknown-pragmas".to_owned(),
            "-Wno-error".to_owned(),
        ],
        sdk_root,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn find_ios_simulator_sdk() -> Option<SdkInfo> {
    None
}

/// Heuristically decide whether a workspace is an iOS project.
///
/// Checks (in order):
/// 1. `Podfile` contains `platform :ios`
/// 2. `.podspec` contains `s.platform = :ios` or `s.ios`
/// 3. Presence of `*.xcodeproj` with iOS-specific SDKROOT keys
pub fn detect_ios_project(root: &std::path::Path) -> bool {
    // 1. Podfile
    let podfile = root.join("Podfile");
    if let Ok(text) = std::fs::read_to_string(&podfile) {
        if text.contains("platform :ios") || text.contains("platform :tvos") {
            return true;
        }
    }

    // 2. .podspec in root
    if let Ok(rd) = std::fs::read_dir(root) {
        for entry in rd.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().ends_with(".podspec") {
                if let Ok(text) = std::fs::read_to_string(entry.path()) {
                    if text.contains(".ios") || text.contains("platform = :ios") {
                        return true;
                    }
                }
            }
        }
    }

    // 3. .xcodeproj pbxproj SDKROOT hint
    if let Ok(rd) = std::fs::read_dir(root) {
        for entry in rd.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().ends_with(".xcodeproj") {
                let pbx = entry.path().join("project.pbxproj");
                if let Ok(text) = std::fs::read_to_string(&pbx) {
                    if text.contains("iphoneos") || text.contains("iphonesimulator") {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Return `-I` flags for CocoaPods public headers.
///
/// Looks for `Pods/Headers/Public` relative to `workspace_root`.
/// This is the standard CocoaPods layout; both regular and xcfilelist-patch
/// setups expose headers here after `pod install`.
pub fn cocoapods_flags(workspace_root: &std::path::Path) -> Vec<String> {
    let mut flags = Vec::new();

    // Standard CocoaPods layout: Pods/Headers/Public/
    let public_headers = workspace_root.join("Pods").join("Headers").join("Public");
    if public_headers.exists() {
        // Add the Public/ root (for `#import <Pod/Header.h>` style).
        flags.push("-I".to_owned());
        flags.push(public_headers.to_string_lossy().into_owned());
        // Also add each pod's subdirectory.
        if let Ok(rd) = std::fs::read_dir(&public_headers) {
            for entry in rd.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    flags.push("-I".to_owned());
                    flags.push(entry.path().to_string_lossy().into_owned());
                }
            }
        }
    }

    // Some setups also use Pods/Headers/Private/
    let private_headers = workspace_root.join("Pods").join("Headers").join("Private");
    if private_headers.exists() {
        flags.push("-I".to_owned());
        flags.push(private_headers.to_string_lossy().into_owned());
    }

    // XCFramework / xcfilelist-patch layout: Pods/<PodName>/ directly.
    // Don't recurse deeply to avoid bloat — just top-level.
    let pods_dir = workspace_root.join("Pods");
    if pods_dir.exists() && !public_headers.exists() {
        if let Ok(rd) = std::fs::read_dir(&pods_dir) {
            for entry in rd.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    // Skip hidden dirs, Local Podspecs dir, etc.
                    if name_str.starts_with('.') || name_str == "Headers" {
                        continue;
                    }
                    flags.push("-I".to_owned());
                    flags.push(entry.path().to_string_lossy().into_owned());
                }
            }
        }
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
