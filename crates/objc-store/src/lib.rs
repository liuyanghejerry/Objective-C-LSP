//! Persistent index store backed by SQLite.
//!
//! Caches cross-file symbol and reference data so that workspace-wide
//! queries (find-references, workspace/symbol) don't require a full
//! re-parse on every request.

pub mod queries;
pub mod schema;

use rusqlite::Connection;
use std::path::Path;
use anyhow::Result;

/// The on-disk symbol/reference database.
pub struct IndexStore {
    conn: Connection,
}

impl IndexStore {
    /// Open (or create) the store at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    /// Open an in-memory store (useful for tests).
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        schema::create_tables(&self.conn)
    }
}
