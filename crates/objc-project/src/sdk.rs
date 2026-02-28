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

    if flags.is_empty() {
        None
    } else {
        Some(flags)
    }
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
        // Inject project prefix header (e.g. MyApp-Prefix.pch) so that
        // headers that rely on globally-imported UIKit / Foundation types
        // (injected by Xcode via GCC_PREFIX_HEADER) resolve correctly.
        if let Some(pch) = find_prefix_header(root) {
            // libclang treats `-include <file.pch>` as a pre-compiled binary PCH,
            // causing CXError_ASTReadError (4). Work around by copying the source
            // text to a `.h` file so clang treats it as normal source text.
            let include_path = if pch.extension().and_then(|e| e.to_str()) == Some("pch") {
                // Write to a stable path in our temp headers area, converting
                // `@import Foo` to `#import <Foo/Foo.h>` so the prefix header
                // works without -fmodules.
                let dest = std::env::temp_dir()
                    .join("objc-lsp-headers")
                    .join("prefix_header_src.h");
                if let Ok(text) = std::fs::read_to_string(&pch) {
                    let converted = convert_at_imports(&text);
                    let _ = std::fs::create_dir_all(dest.parent().unwrap());
                    if std::fs::write(&dest, &converted).is_ok() {
                        dest
                    } else {
                        pch
                    }
                } else {
                    pch
                }
            } else {
                pch
            };
            flags.push("-include".to_owned());
            flags.push(include_path.to_string_lossy().into_owned());
        }
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
            // NOTE: -fmodules causes CXError_ASTReadError (err=4) with Xcode's
            // libclang — the module compilation pipeline fails silently and
            // returns a null TU. SDK headers (UIKit, Foundation, etc.) resolve
            // correctly via -isysroot alone without modules.
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

/// Locate the project prefix header (`.pch`) so we can inject it with
/// `-include <path>`, matching Xcode's `GCC_PREFIX_HEADER` behaviour.
///
/// Search strategy (workspace_root and one level of subdirectories):
/// 1. `<root>/<Name>-Prefix.pch`   — CocoaPods example app pattern
/// 2. `<root>/**/*-Prefix.pch`     — common Xcode template pattern
/// 3. Any `*.pch` that imports UIKit (most likely the app prefix header)
///
/// Returns the first `.pch` found that contains `#import <UIKit` or
/// `#import <AppKit`, so we don't accidentally pick up test prefix headers.
pub fn find_prefix_header(workspace_root: &std::path::Path) -> Option<std::path::PathBuf> {
    // Walk at most 3 levels deep — prefix headers are always near the root.
    find_pch_recursive(workspace_root, 0, 3)
}

fn find_pch_recursive(
    dir: &std::path::Path,
    depth: usize,
    max_depth: usize,
) -> Option<std::path::PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut subdirs: Vec<std::path::PathBuf> = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Skip hidden dirs and build artefacts.
        if name_str.starts_with('.') {
            continue;
        }
        let ft = entry.file_type().ok()?;
        if ft.is_dir() {
            if depth < max_depth
                && name_str != "Pods"
                && name_str != "build"
                && name_str != "DerivedData"
                && name_str != "node_modules"
            {
                subdirs.push(path);
            }
        } else if name_str.ends_with(".pch") {
            // Only inject prefix headers that pull in a UI framework —
            // those are the ones that provide implicit UIViewController etc.
            if let Ok(text) = std::fs::read_to_string(&path) {
                if text.contains("#import <UIKit")
                    || text.contains("#import <AppKit")
                    || text.contains("@import UIKit")
                    || text.contains("@import AppKit")
                {
                    return Some(path);
                }
            }
        }
    }
    // BFS: recurse into subdirectories after scanning current level.
    for sub in subdirs {
        if let Some(found) = find_pch_recursive(&sub, depth + 1, max_depth) {
            return Some(found);
        }
    }
    None
}

/// Convert `@import Foo;` and `@import Foo.Bar;` lines to `#import <Foo/Foo.h>`
/// so the prefix header works without `-fmodules`.
fn convert_at_imports(src: &str) -> String {
    src.lines()
        .map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("@import ") {
                // rest looks like "UIKit;" or "Foundation;" or "Foo.Bar;"
                let module = rest.trim_end_matches(';');
                // Take the top-level framework name (before any `.`)
                let framework = module.split('.').next().unwrap_or(module);
                format!("#import <{framework}/{framework}.h>")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Return `-I` flags for CocoaPods public headers.
///
/// Looks for `Pods/Headers/Public` relative to `workspace_root`.
/// This is the standard CocoaPods layout; both regular and xcfilelist-patch
/// setups expose headers here after `pod install`.
///
/// When `Pods/` does not exist (i.e. `pod install` has never been run),
/// falls back to scanning the workspace source tree for directories that
/// contain `.h` files and adding their *parent* as an `-I` path.  This
/// makes framework-style imports like `#import <PodName/Header.h>` resolve
/// against the project's own source tree.
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

    // ── Fallback: no Pods/ directory (pod install not yet run) ──────────────
    // Synthesise a CocoaPods-style flat header directory so that framework-
    // style imports like `#import <PodName/Header.h>` resolve from the
    // project's own source tree without requiring pod install.
    if !pods_dir.exists() {
        if let Some(synth_dir) = synthetic_pod_headers_dir(workspace_root) {
            // synth_dir/  ←  add this as -I so <PodName/Foo.h> resolves
            flags.push("-I".to_owned());
            flags.push(synth_dir.to_string_lossy().into_owned());
        }
    }

    flags
}

/// Create (or reuse) a synthetic flat-headers directory that mirrors what
/// `pod install` would create in `Pods/Headers/Public/`.
///
/// Scans the workspace source tree for `.h` files, groups them by the
/// top-level subdirectory of `workspace_root` they belong to (treated as the
/// pod name), then creates symlinks in a temp directory:
///
/// ```text
/// /tmp/objc-lsp-headers/<hash>/
///   SAKIdentityCardRecognizer/   ←  all *.h under SAKIdentityCardRecognizer/**
///     SPKNfcIdentifyCommand.h   ←  symlink → actual path
/// ```
///
/// Returns `None` on any I/O error.  The temp directory is reused across
/// calls for the same workspace (identified by a hash of the path).
fn synthetic_pod_headers_dir(workspace_root: &std::path::Path) -> Option<PathBuf> {
    use std::collections::HashMap;

    // Stable hash of workspace_root so the same project reuses the same dir.
    let hash = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        workspace_root.hash(&mut h);
        h.finish()
    };
    let synth_root = std::env::temp_dir()
        .join("objc-lsp-headers")
        .join(format!("{hash:x}"));

    // Collect all .h files grouped by their first path component
    // relative to workspace_root (= the pod name).
    let mut pod_headers: HashMap<String, Vec<PathBuf>> = HashMap::new();
    collect_headers_for_synth(workspace_root, workspace_root, 0, 6, &mut pod_headers);

    if pod_headers.is_empty() {
        return None;
    }

    // Create the synthetic directory structure and symlinks.
    for (pod_name, headers) in &pod_headers {
        let pod_dir = synth_root.join(pod_name);
        if std::fs::create_dir_all(&pod_dir).is_err() {
            continue;
        }
        for header_path in headers {
            let file_name = match header_path.file_name() {
                Some(n) => n,
                None => continue,
            };
            let link_path = pod_dir.join(file_name);
            // Skip if already exists (idempotent).
            if link_path.exists() || link_path.symlink_metadata().is_ok() {
                continue;
            }
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(header_path, &link_path);
            #[cfg(not(unix))]
            let _ = std::fs::copy(header_path, &link_path);
        }
    }

    Some(synth_root)
}

/// Recursively collect all `.h` files under `dir`, grouped by the first path
/// component of each file relative to `workspace_root` (= the pod name).
fn collect_headers_for_synth(
    workspace_root: &std::path::Path,
    dir: &std::path::Path,
    depth: usize,
    max_depth: usize,
    out: &mut std::collections::HashMap<String, Vec<PathBuf>>,
) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if ft.is_file() && name_str.ends_with(".h") {
            // Determine the pod name = first component of the relative path.
            if let Ok(rel) = path.strip_prefix(workspace_root) {
                if let Some(pod) = rel.components().next() {
                    let pod_name = pod.as_os_str().to_string_lossy().into_owned();
                    out.entry(pod_name).or_default().push(path.clone());
                }
            }
        } else if ft.is_dir() {
            if name_str.starts_with('.')
                || name_str == "build"
                || name_str == "DerivedData"
                || name_str == "Pods"
                || name_str == "node_modules"
                || name_str == "vendor"
            {
                continue;
            }
            collect_headers_for_synth(workspace_root, &path, depth + 1, max_depth, out);
        }
    }
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

    #[test]
    fn synthetic_pod_headers_finds_header_files() {
        // Create a fake project tree:
        //   tmp/
        //     MyPod/
        //       Classes/
        //         Foo.h      ← header here
        let tmp = tempfile::tempdir().expect("tempdir");
        let classes = tmp.path().join("MyPod").join("Classes");
        std::fs::create_dir_all(&classes).unwrap();
        std::fs::write(classes.join("Foo.h"), "// header").unwrap();

        let synth = synthetic_pod_headers_dir(tmp.path())
            .expect("expected synthetic dir");
        // synth/MyPod/Foo.h should exist (symlink or copy)
        assert!(
            synth.join("MyPod").join("Foo.h").exists(),
            "expected synth/MyPod/Foo.h to exist: {synth:?}"
        );
    }

    #[test]
    fn cocoapods_flags_fallback_when_no_pods_dir() {
        // Create a fake project: tmp/MyPod/Classes/Foo.h (no Pods/ dir)
        let tmp = tempfile::tempdir().expect("tempdir");
        let classes = tmp.path().join("MyPod").join("Classes");
        std::fs::create_dir_all(&classes).unwrap();
        std::fs::write(classes.join("Foo.h"), "// header").unwrap();

        let flags = cocoapods_flags(tmp.path());
        // The fallback should produce at least one -I flag pointing into a
        // synthetic temp directory that contains a MyPod/ subdirectory.
        let has_synth = flags.windows(2).any(|w| {
            w[0] == "-I"
                && std::path::Path::new(&w[1]).join("MyPod").join("Foo.h").exists()
        });
        assert!(
            has_synth,
            "expected fallback -I <synth-dir> with MyPod/Foo.h when no Pods dir: {flags:?}"
        );
    }
}

