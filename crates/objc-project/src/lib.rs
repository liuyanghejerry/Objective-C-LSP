//! Project and build system integration.
//!
//! Resolves compilation flags for each source file from:
//! 1. .xcodeproj / .xcworkspace (pbxproj format)
//! 2. compile_commands.json (CMake / Bazel / xcode-build-server)
//! 3. Apple SDK framework header discovery
//! 4. GNUstep include path detection

pub mod compile_db;
pub mod sdk;
pub mod xcodeproj;

use std::path::PathBuf;

/// Compilation flags for a single source file.
#[derive(Debug, Clone, Default)]
pub struct CompileFlags {
    pub args: Vec<String>,
    pub working_dir: PathBuf,
}

/// Resolves compile flags for source files in a workspace.
pub trait FlagResolver: Send + Sync {
    fn flags_for(&self, file: &std::path::Path) -> Option<CompileFlags>;
}
