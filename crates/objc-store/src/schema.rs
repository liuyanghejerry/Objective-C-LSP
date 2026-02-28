//! SQLite schema creation and migrations.

use anyhow::Result;
use rusqlite::Connection;

/// Create all tables if they do not yet exist.
pub fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        -- One row per source file tracked.
        CREATE TABLE IF NOT EXISTS files (
            id       INTEGER PRIMARY KEY,
            path     TEXT    NOT NULL UNIQUE,
            mtime    INTEGER NOT NULL DEFAULT 0,
            indexed  INTEGER NOT NULL DEFAULT 0  -- unix timestamp of last index
        );

        -- Symbol definitions (classes, methods, properties, protocols, …).
        CREATE TABLE IF NOT EXISTS symbols (
            id        INTEGER PRIMARY KEY,
            file_id   INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            name      TEXT    NOT NULL,
            kind      TEXT    NOT NULL,  -- 'class' | 'method' | 'property' | 'protocol' | 'category'
            selector  TEXT,              -- full ObjC selector string for methods
            line      INTEGER NOT NULL,
            col       INTEGER NOT NULL,
            end_line  INTEGER NOT NULL,
            end_col   INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_symbols_name     ON symbols(name);
        CREATE INDEX IF NOT EXISTS idx_symbols_selector ON symbols(selector);
        CREATE INDEX IF NOT EXISTS idx_symbols_file     ON symbols(file_id);

        -- Cross-references: where each symbol is used.
        CREATE TABLE IF NOT EXISTS xrefs (
            id         INTEGER PRIMARY KEY,
            symbol_id  INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
            file_id    INTEGER NOT NULL REFERENCES files(id)   ON DELETE CASCADE,
            line       INTEGER NOT NULL,
            col        INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_xrefs_symbol ON xrefs(symbol_id);
        CREATE INDEX IF NOT EXISTS idx_xrefs_file   ON xrefs(file_id);

        -- Category membership: maps a category name back to its base class.
        CREATE TABLE IF NOT EXISTS categories (
            id         INTEGER PRIMARY KEY,
            base_class TEXT    NOT NULL,
            category   TEXT    NOT NULL,
            file_id    INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_categories_base ON categories(base_class);
    "#)?;
    Ok(())
}
