//! Incremental, error-tolerant Objective-C parser using tree-sitter.

use anyhow::Result;
use tree_sitter::{Node, Parser, Tree};

/// A parsed snapshot of a single source file.
pub struct ParsedFile {
    pub source: String,
    pub tree: Tree,
}

impl ParsedFile {
    /// Root node of the syntax tree.
    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }

    /// Source bytes (tree-sitter cursor methods need `&[u8]`).
    pub fn source_bytes(&self) -> &[u8] {
        self.source.as_bytes()
    }
}

/// Wrapper around a tree-sitter `Parser` configured for Objective-C.
pub struct ObjcParser {
    inner: Parser,
}

impl ObjcParser {
    pub fn new() -> Result<Self> {
        let mut inner = Parser::new();
        inner
            .set_language(&tree_sitter_objc::LANGUAGE.into())
            .map_err(|e| anyhow::anyhow!("Failed to load ObjC grammar: {e}"))?;
        Ok(Self { inner })
    }

    /// Parse a source string from scratch.
    pub fn parse(&mut self, source: impl Into<String>) -> Result<ParsedFile> {
        let source = source.into();
        let tree = self
            .inner
            .parse(&source, None)
            .ok_or_else(|| anyhow::anyhow!("tree-sitter parse returned None"))?;
        Ok(ParsedFile { source, tree })
    }

    /// Re-parse with an incremental edit (cheaper than full re-parse).
    pub fn reparse(&mut self, file: &mut ParsedFile, new_source: impl Into<String>) -> Result<()> {
        let new_source = new_source.into();
        let new_tree = self
            .inner
            .parse(&new_source, Some(&file.tree))
            .ok_or_else(|| anyhow::anyhow!("tree-sitter reparse returned None"))?;
        file.source = new_source;
        file.tree = new_tree;
        Ok(())
    }
}

impl Default for ObjcParser {
    fn default() -> Self {
        Self::new().expect("ObjC grammar must be available")
    }
}

