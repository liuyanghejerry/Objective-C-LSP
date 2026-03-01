use std::fs;
use zed_extension_api::{
    self as zed, current_platform, download_file, latest_github_release, make_file_executable,
    set_language_server_installation_status, Architecture, Command, DownloadedFileType,
    GithubReleaseOptions, LanguageServerInstallationStatus, Os, Result,
};

const GITHUB_REPO: &str = "objc-lsp/objc-lsp";
const BINARY_NAME: &str = "objc-lsp";

struct ObjcLspExtension {
    cached_binary_path: Option<String>,
}

impl ObjcLspExtension {
    /// Maps the Zed platform tuple to the asset name suffix used in GitHub Releases.
    ///
    /// Must match the naming convention in `.github/workflows/release.yml`:
    ///   `objc-lsp-{os}-{arch}` (e.g. `objc-lsp-darwin-arm64`)
    fn platform_asset_suffix() -> Result<(&'static str, &'static str)> {
        let (os, arch) = current_platform();
        let os_str = match os {
            Os::Mac => "darwin",
            Os::Linux => "linux",
            Os::Windows => return Err("Objective-C development is not supported on Windows".into()),
        };
        let arch_str = match arch {
            Architecture::Aarch64 => "arm64",
            Architecture::X8664 => "x64",
            Architecture::X86 => return Err("32-bit x86 is not supported".into()),
        };
        Ok((os_str, arch_str))
    }

    /// Returns the expected asset name for the current platform.
    fn asset_name() -> Result<String> {
        let (os, arch) = Self::platform_asset_suffix()?;
        Ok(format!("{BINARY_NAME}-{os}-{arch}"))
    }

    /// Ensures the language server binary is downloaded and returns its path.
    fn ensure_binary(&mut self, language_server_id: &zed::LanguageServerId) -> Result<String> {
        // Return cached path if the binary still exists on disk.
        if let Some(ref path) = self.cached_binary_path {
            if fs::metadata(path).is_ok() {
                return Ok(path.clone());
            }
        }

        set_language_server_installation_status(
            language_server_id,
            &LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = latest_github_release(
            GITHUB_REPO,
            GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let asset_name = Self::asset_name()?;
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| {
                format!(
                    "No binary asset '{}' found in release {}",
                    asset_name, release.version
                )
            })?;

        // Version-stamped directory so upgrades get a fresh download.
        let version_dir = format!("objc-lsp-{}", release.version);
        let binary_path = format!("{version_dir}/{BINARY_NAME}");

        // Skip download if this exact version is already on disk.
        if fs::metadata(&binary_path).is_ok() {
            self.cached_binary_path = Some(binary_path.clone());
            return Ok(binary_path);
        }

        set_language_server_installation_status(
            language_server_id,
            &LanguageServerInstallationStatus::Downloading,
        );

        // The release assets are raw binaries (not archives).
        download_file(
            &asset.download_url,
            &binary_path,
            DownloadedFileType::Uncompressed,
        )
        .map_err(|e| format!("Failed to download {}: {}", asset_name, e))?;

        make_file_executable(&binary_path)?;

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

impl zed::Extension for ObjcLspExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<Command> {
        let binary_path = self.ensure_binary(language_server_id)?;

        Ok(Command {
            command: binary_path,
            args: vec![],
            env: vec![],
        })
    }
}

zed::register_extension!(ObjcLspExtension);
