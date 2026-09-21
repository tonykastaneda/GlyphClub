use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

use crate::font::FontRecord;

#[derive(Debug, Clone)]
pub struct Folder {
    pub id: i64,
    pub path: String,
}

/// One font face as read back from the catalog, including the id needed to
/// reference it (e.g. when the UI asks to load its bytes for a live preview)
/// and the folder it was scanned from, for the sidebar's per-library filter.
pub struct FontRow {
    pub id: i64,
    pub folder_id: i64,
    pub record: FontRecord,
}

/// Persistent cache of everything we've already parsed, keyed by (path, mtime,
/// size), so relaunching the app doesn't mean re-parsing 20k font files.
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS folders (
                id   INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS fonts (
                id         INTEGER PRIMARY KEY,
                folder_id  INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                path       TEXT NOT NULL,
                face_index INTEGER NOT NULL,
                family     TEXT NOT NULL,
                subfamily  TEXT NOT NULL,
                weight     INTEGER NOT NULL,
                italic     INTEGER NOT NULL,
                monospace  INTEGER NOT NULL,
                mtime      INTEGER NOT NULL,
                size       INTEGER NOT NULL,
                UNIQUE(path, face_index)
            );

            CREATE INDEX IF NOT EXISTS idx_fonts_family ON fonts(family);
            CREATE INDEX IF NOT EXISTS idx_fonts_path   ON fonts(path);
            CREATE INDEX IF NOT EXISTS idx_fonts_folder ON fonts(folder_id);

            CREATE TABLE IF NOT EXISTS activations (
                font_id        INTEGER PRIMARY KEY REFERENCES fonts(id) ON DELETE CASCADE,
                installed_path TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS favorites (
                font_id INTEGER PRIMARY KEY REFERENCES fonts(id) ON DELETE CASCADE
            );
            ",
        )?;
        Ok(Self { conn })
    }

    pub fn add_folder(&self, path: &str) -> Result<i64> {
        self.conn
            .execute("INSERT OR IGNORE INTO folders (path) VALUES (?1)", [path])?;
        let id: i64 =
            self.conn
                .query_row("SELECT id FROM folders WHERE path = ?1", [path], |row| {
                    row.get(0)
                })?;
        Ok(id)
    }

    pub fn remove_folder(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM folders WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn list_folders(&self) -> Result<Vec<Folder>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path FROM folders ORDER BY path")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Folder {
                    id: row.get(0)?,
                    path: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// All files already indexed for a folder, as `path -> (mtime, size)`, so a
    /// rescan can diff the filesystem against this in memory instead of hitting
    /// SQLite once per file.
    pub fn known_files(&self, folder_id: i64) -> Result<HashMap<String, (i64, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT path, mtime, size FROM fonts WHERE folder_id = ?1")?;
        let rows = stmt
            .query_map([folder_id], |row| {
                Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?)))
            })?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        Ok(rows)
    }

    /// Replaces every face previously stored for `path` with `records` (a file's
    /// face count can change if it was edited, e.g. a collection gaining a face).
    pub fn replace_faces(
        &mut self,
        folder_id: i64,
        path: &str,
        records: &[FontRecord],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM fonts WHERE path = ?1", [path])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO fonts
                    (folder_id, path, face_index, family, subfamily, weight, italic, monospace, mtime, size)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for r in records {
                stmt.execute(rusqlite::params![
                    folder_id,
                    r.path,
                    r.face_index,
                    r.family,
                    r.subfamily,
                    r.weight,
                    r.italic as i64,
                    r.monospace as i64,
                    r.mtime,
                    r.size,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_path(&self, path: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM fonts WHERE path = ?1", [path])?;
        Ok(())
    }

    /// Full catalog, ordered for display (family, then weight/style).
    pub fn all_fonts(&self) -> Result<Vec<FontRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, folder_id, path, face_index, family, subfamily, weight, italic, monospace, mtime, size
             FROM fonts
             ORDER BY family COLLATE NOCASE, weight, italic",
        )?;
        let rows = stmt
            .query_map([], row_to_font)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn search_fonts(&self, query: &str) -> Result<Vec<FontRow>> {
        let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = self.conn.prepare(
            "SELECT id, folder_id, path, face_index, family, subfamily, weight, italic, monospace, mtime, size
             FROM fonts
             WHERE family LIKE ?1 ESCAPE '\\'
             ORDER BY family COLLATE NOCASE, weight, italic",
        )?;
        let rows = stmt
            .query_map([pattern], row_to_font)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn font_path(&self, id: i64) -> Result<Option<(String, u32)>> {
        let result = self
            .conn
            .query_row(
                "SELECT path, face_index FROM fonts WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        Ok(result)
    }

    /// Every font id currently activated, for marking status dots in bulk
    /// instead of querying per tile.
    pub fn active_ids(&self) -> Result<HashSet<i64>> {
        let mut stmt = self.conn.prepare("SELECT font_id FROM activations")?;
        let rows = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        Ok(rows)
    }

    pub fn record_activation(&self, font_id: i64, installed_path: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO activations (font_id, installed_path) VALUES (?1, ?2)",
            rusqlite::params![font_id, installed_path],
        )?;
        Ok(())
    }

    /// Removes the activation record and returns the installed path it had,
    /// so the caller can delete/unlink the actual installed font file.
    pub fn clear_activation(&self, font_id: i64) -> Result<Option<String>> {
        let path = self
            .conn
            .query_row(
                "SELECT installed_path FROM activations WHERE font_id = ?1",
                [font_id],
                |row| row.get(0),
            )
            .ok();
        self.conn
            .execute("DELETE FROM activations WHERE font_id = ?1", [font_id])?;
        Ok(path)
    }

    /// Every font id currently starred, for filtering the Starred sidebar tab
    /// and marking star icons in bulk instead of querying per tile.
    pub fn favorite_ids(&self) -> Result<HashSet<i64>> {
        let mut stmt = self.conn.prepare("SELECT font_id FROM favorites")?;
        let rows = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        Ok(rows)
    }

    pub fn set_favorite(&self, font_id: i64, favorite: bool) -> Result<()> {
        if favorite {
            self.conn.execute(
                "INSERT OR IGNORE INTO favorites (font_id) VALUES (?1)",
                [font_id],
            )?;
        } else {
            self.conn
                .execute("DELETE FROM favorites WHERE font_id = ?1", [font_id])?;
        }
        Ok(())
    }
}

fn row_to_font(row: &rusqlite::Row) -> rusqlite::Result<FontRow> {
    Ok(FontRow {
        id: row.get(0)?,
        folder_id: row.get(1)?,
        record: FontRecord {
            path: row.get(2)?,
            face_index: row.get(3)?,
            family: row.get(4)?,
            subfamily: row.get(5)?,
            weight: row.get(6)?,
            italic: row.get::<_, i64>(7)? != 0,
            monospace: row.get::<_, i64>(8)? != 0,
            mtime: row.get(9)?,
            size: row.get(10)?,
        },
    })
}
