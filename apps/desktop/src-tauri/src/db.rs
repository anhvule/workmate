//! Database handle.
//!
//! One connection behind a mutex, owned by Rust. The webview never touches
//! this: every write arrives through a Tauri command, because a throttled
//! background webview cannot be trusted to finish a write it started
//! (ticket 010).

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::migrations;

/// The clock durable rows are stamped with: Unix milliseconds.
///
/// Milliseconds rather than seconds because two handoffs inside one second are
/// ordinary, and the handoff graph is ordered by time (ticket 010).
#[must_use]
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// A path as a storable string, or `None` if it is not UTF-8.
///
/// Columns are text, and a lossy conversion would bind a row to a directory
/// that does not exist — so a path that cannot round-trip is refused instead.
#[must_use]
pub fn path_text(p: &Path) -> Option<&str> {
    p.to_str()
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database is locked by another operation")]
    Poisoned,
}

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// Open the database at `path`, creating and migrating it if needed.
    ///
    /// # Errors
    /// Returns [`DbError::Sqlite`] if the file cannot be opened or the
    /// migration ladder fails.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let mut conn = Connection::open(path)?;
        Self::prepare(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// An in-memory database, for tests.
    ///
    /// # Errors
    /// Returns [`DbError::Sqlite`] if the migration ladder fails.
    pub fn open_in_memory() -> Result<Self, DbError> {
        let mut conn = Connection::open_in_memory()?;
        Self::prepare(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn prepare(conn: &mut Connection) -> Result<(), DbError> {
        // WAL so a long read cannot block a write; foreign keys because the
        // schema's cascade rules are load-bearing rather than decorative.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        migrations::migrate(conn)?;
        Ok(())
    }

    /// Run `f` against the connection.
    ///
    /// # Errors
    /// Returns [`DbError::Poisoned`] if another thread panicked while holding
    /// the lock, or whatever `f` returns.
    pub fn with<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, DbError> {
        let guard = self.conn.lock().map_err(|_| DbError::Poisoned)?;
        Ok(f(&guard)?)
    }

    /// The schema version currently applied.
    ///
    /// # Errors
    /// Returns [`DbError`] if the version row cannot be read.
    pub fn schema_version(&self) -> Result<i64, DbError> {
        self.with(|c| c.query_row("SELECT version FROM schema_meta WHERE id = 1", [], |r| r.get(0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_database_is_migrated_to_the_target_version() {
        let db = Db::open_in_memory().expect("open");
        assert_eq!(db.schema_version().expect("version"), migrations::target_version());
    }

    #[test]
    fn foreign_keys_are_enforced_so_the_cascade_rules_actually_apply() {
        let db = Db::open_in_memory().expect("open");
        let orphan = db.with(|c| {
            c.execute(
                "INSERT INTO run (id,workspace_id,objective,branch,state,created_at)
                 VALUES ('r','nope','x','b','running',0)",
                [],
            )
        });
        assert!(orphan.is_err(), "a run must not outlive a missing workspace");
    }

    #[test]
    fn reopening_a_file_database_preserves_its_rows() {
        let dir = std::env::temp_dir().join(format!("workmate-db-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("workmate.sqlite3");

        {
            let db = Db::open(&path).expect("open");
            db.with(|c| {
                c.execute(
                    "INSERT INTO workspace (id,name,directory,created_at)
                     VALUES ('w1','ws','/tmp/w1',0)",
                    [],
                )
            })
            .expect("insert");
        }

        let db = Db::open(&path).expect("reopen");
        let n: i64 = db
            .with(|c| c.query_row("SELECT count(*) FROM workspace", [], |r| r.get(0)))
            .expect("count");
        assert_eq!(n, 1);

        std::fs::remove_dir_all(&dir).ok();
    }
}
