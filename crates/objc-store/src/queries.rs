//! High-level query helpers over the index store.

use crate::IndexStore;
use anyhow::Result;

/// A lightweight symbol record returned from the store.
#[derive(Debug, Clone)]
pub struct SymbolRecord {
    pub id: i64,
    pub file_path: String,
    pub name: String,
    pub kind: String,
    pub selector: Option<String>,
    pub line: u32,
    pub col: u32,
}

impl IndexStore {
    /// Look up symbols by exact name across the whole workspace.
    pub fn find_symbols_by_name(&self, name: &str) -> Result<Vec<SymbolRecord>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT s.id, f.path, s.name, s.kind, s.selector, s.line, s.col
            FROM symbols s
            JOIN files f ON f.id = s.file_id
            WHERE s.name = ?1
            "#,
        )?;

        let rows = stmt.query_map([name], |row| {
            Ok(SymbolRecord {
                id: row.get(0)?,
                file_path: row.get(1)?,
                name: row.get(2)?,
                kind: row.get(3)?,
                selector: row.get(4)?,
                line: row.get::<_, u32>(5)?,
                col: row.get::<_, u32>(6)?,
            })
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Fuzzy-search symbols by name prefix (for workspace/symbol).
    pub fn search_symbols(&self, query: &str) -> Result<Vec<SymbolRecord>> {
        let pattern = format!("{query}%");
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT s.id, f.path, s.name, s.kind, s.selector, s.line, s.col
            FROM symbols s
            JOIN files f ON f.id = s.file_id
            WHERE s.name LIKE ?1
            LIMIT 100
            "#,
        )?;

        let rows = stmt.query_map([&pattern], |row| {
            Ok(SymbolRecord {
                id: row.get(0)?,
                file_path: row.get(1)?,
                name: row.get(2)?,
                kind: row.get(3)?,
                selector: row.get(4)?,
                line: row.get::<_, u32>(5)?,
                col: row.get::<_, u32>(6)?,
            })
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Upsert a file record, returning its rowid.
    pub fn upsert_file(&self, path: &str, mtime: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO files(path, mtime) VALUES(?1, ?2)
             ON CONFLICT(path) DO UPDATE SET mtime = excluded.mtime",
            rusqlite::params![path, mtime],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
}
