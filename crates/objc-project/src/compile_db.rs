//! compile_commands.json loader.

use crate::{CompileFlags, FlagResolver};
use anyhow::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct Entry {
    directory: String,
    file: String,
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default)]
    command: Option<String>,
}

/// Resolves compile flags from a `compile_commands.json` file.
pub struct CompileCommandsDb {
    entries: Vec<Entry>,
}

impl CompileCommandsDb {
    pub fn load(path: &Path) -> Result<Self> {
        let data = std::fs::read_to_string(path)?;
        let entries: Vec<Entry> = serde_json::from_str(&data)?;
        Ok(Self { entries })
    }

    /// Search upward from `start` for a `compile_commands.json`.
    pub fn find_and_load(start: &Path) -> Option<Self> {
        let mut dir = if start.is_file() {
            start.parent()?.to_path_buf()
        } else {
            start.to_path_buf()
        };

        loop {
            let candidate = dir.join("compile_commands.json");
            if candidate.exists() {
                return Self::load(&candidate).ok();
            }
            let build = dir.join("build").join("compile_commands.json");
            if build.exists() {
                return Self::load(&build).ok();
            }
            if !dir.pop() {
                return None;
            }
        }
    }
}

impl FlagResolver for CompileCommandsDb {
    fn flags_for(&self, file: &Path) -> Option<CompileFlags> {
        let file_str = file.to_string_lossy();
        let entry = self
            .entries
            .iter()
            .find(|e| Path::new(&e.file) == file || e.file.ends_with(file_str.as_ref()))?;

        let args = if !entry.arguments.is_empty() {
            entry.arguments.clone()
        } else if let Some(cmd) = &entry.command {
            shell_words_split(cmd)
        } else {
            return None;
        };

        Some(CompileFlags {
            args,
            working_dir: PathBuf::from(&entry.directory),
        })
    }
}

/// Minimal shell word splitter (handles quoted strings).
fn shell_words_split(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = '"';

    for ch in s.chars() {
        match ch {
            '"' | '\'' if !in_quote => {
                in_quote = true;
                quote_char = ch;
            }
            c if in_quote && c == quote_char => {
                in_quote = false;
            }
            ' ' | '\t' if !in_quote => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}
