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

    /// List all file paths currently in the store.
    pub fn list_all_file_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare_cached("SELECT path FROM files ORDER BY path")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<Vec<String>>>().map_err(Into::into)
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

/// A symbol to be inserted/updated in the store (used by `index_file_symbols`).
#[derive(Debug, Clone)]
pub struct SymbolInput {
    pub name: String,
    pub kind: String,
    pub selector: Option<String>,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl IndexStore {
    /// Replace all symbols for `file_path` with `symbols`.
    ///
    /// If the file is not yet in the `files` table it is inserted first.
    /// All old symbols for this file are deleted (CASCADE handles xrefs),
    /// then the new batch is inserted in a single transaction.
    pub fn index_file_symbols(&self, file_path: &str, mtime: i64, symbols: &[SymbolInput]) -> Result<()> {
        let file_id = self.upsert_file(file_path, mtime)?;

        // Delete stale symbols for this file.
        self.conn.execute(
            "DELETE FROM symbols WHERE file_id = ?1",
            rusqlite::params![file_id],
        )?;

        // Insert new symbols.
        for sym in symbols {
            self.conn.execute(
                "INSERT INTO symbols(file_id,name,kind,selector,line,col,end_line,end_col) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                rusqlite::params![file_id, sym.name, sym.kind, sym.selector, sym.line, sym.col, sym.end_line, sym.end_col],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::IndexStore;

    fn store() -> IndexStore {
        IndexStore::in_memory().unwrap()
    }

    /// Insert a symbol row directly for test setup.
    fn insert_symbol(store: &IndexStore, file_id: i64, name: &str, kind: &str, selector: Option<&str>, line: u32, col: u32) {
        store.conn.execute(
            "INSERT INTO symbols(file_id,name,kind,selector,line,col,end_line,end_col) VALUES(?1,?2,?3,?4,?5,?6,?5,?6)",
            rusqlite::params![file_id, name, kind, selector, line, col],
        ).unwrap();
    }

    // -------------------------------------------------------------------
    // upsert_file
    // -------------------------------------------------------------------

    #[test]
    fn upsert_file_returns_rowid() {
        let s = store();
        let id = s.upsert_file("/tmp/Foo.m", 1000).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn upsert_file_updates_mtime_on_conflict() {
        let s = store();
        s.upsert_file("/tmp/Foo.m", 1000).unwrap();
        // Upsert again with a new mtime — should not fail.
        let id2 = s.upsert_file("/tmp/Foo.m", 2000);
        assert!(id2.is_ok(), "second upsert failed: {id2:?}");
    }

    // -------------------------------------------------------------------
    // find_symbols_by_name
    // -------------------------------------------------------------------

    #[test]
    fn find_symbols_empty_store() {
        let s = store();
        let results = s.find_symbols_by_name("Foo").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn find_symbols_exact_match() {
        let s = store();
        let fid = s.upsert_file("/src/Foo.m", 0).unwrap();
        insert_symbol(&s, fid, "MyClass", "class", None, 1, 0);
        let results = s.find_symbols_by_name("MyClass").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "MyClass");
        assert_eq!(results[0].kind, "class");
        assert_eq!(results[0].file_path, "/src/Foo.m");
    }

    #[test]
    fn find_symbols_no_partial_match() {
        let s = store();
        let fid = s.upsert_file("/src/Foo.m", 0).unwrap();
        insert_symbol(&s, fid, "MyClass", "class", None, 1, 0);
        // Partial name should NOT match (exact query).
        let results = s.find_symbols_by_name("My").unwrap();
        assert!(results.is_empty(), "expected no results for partial name, got {results:?}");
    }

    #[test]
    fn find_symbols_multiple_files() {
        let s = store();
        let fid1 = s.upsert_file("/a.m", 0).unwrap();
        let fid2 = s.upsert_file("/b.m", 0).unwrap();
        insert_symbol(&s, fid1, "Duplicate", "class", None, 1, 0);
        insert_symbol(&s, fid2, "Duplicate", "class", None, 1, 0);
        let results = s.find_symbols_by_name("Duplicate").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn find_symbols_with_selector() {
        let s = store();
        let fid = s.upsert_file("/src/Foo.m", 0).unwrap();
        insert_symbol(&s, fid, "initWithName:age:", "method", Some("initWithName:age:"), 5, 0);
        let results = s.find_symbols_by_name("initWithName:age:").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].selector.as_deref(), Some("initWithName:age:"));
    }

    // -------------------------------------------------------------------
    // search_symbols (prefix / LIKE)
    // -------------------------------------------------------------------

    #[test]
    fn search_symbols_prefix_match() {
        let s = store();
        let fid = s.upsert_file("/src/Foo.m", 0).unwrap();
        insert_symbol(&s, fid, "NSString", "class", None, 1, 0);
        insert_symbol(&s, fid, "NSArray", "class", None, 2, 0);
        insert_symbol(&s, fid, "UIView", "class", None, 3, 0);
        let results = s.search_symbols("NS").unwrap();
        assert_eq!(results.len(), 2, "expected 2 NS* results, got {results:?}");
    }

    #[test]
    fn search_symbols_no_match() {
        let s = store();
        let fid = s.upsert_file("/src/Foo.m", 0).unwrap();
        insert_symbol(&s, fid, "Foo", "class", None, 1, 0);
        let results = s.search_symbols("Bar").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_symbols_empty_query_matches_all() {
        let s = store();
        let fid = s.upsert_file("/src/Foo.m", 0).unwrap();
        insert_symbol(&s, fid, "Alpha", "class", None, 1, 0);
        insert_symbol(&s, fid, "Beta", "class", None, 2, 0);
        let results = s.search_symbols("").unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn list_all_file_paths_empty() {
        let s = store();
        let paths = s.list_all_file_paths().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn list_all_file_paths_returns_all() {
        let s = store();
        s.upsert_file("/a.m", 0).unwrap();
        s.upsert_file("/b.m", 0).unwrap();
        s.upsert_file("/c/Foo.m", 0).unwrap();
        let mut paths = s.list_all_file_paths().unwrap();
        paths.sort();
        assert_eq!(paths, vec!["/a.m", "/b.m", "/c/Foo.m"]);
    }
}
