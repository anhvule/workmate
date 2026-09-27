//! Workspaces: a named binding to exactly one directory.
//!
//! The binding is by **id**, never by path. Runs, grants, automations and
//! memory scopes all reference `workspace.id`, so a folder the user moves is
//! *repaired* — one row updated — rather than orphaning everything that
//! pointed at the old location (ticket 003).
//!
//! Directories are canonicalised on the way in. Without it `/tmp/p` and
//! `/private/tmp/p` are two workspaces over one folder on macOS, and the
//! `UNIQUE` constraint that is meant to prevent exactly that never fires.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::db::{path_text, now_ms, Db, DbError};
use crate::ids::new_id;

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("no workspace with id {0}")]
    NotFound(String),
    #[error("{0} is already bound to a workspace")]
    AlreadyBound(PathBuf),
    #[error("{0} is not a directory")]
    NotADirectory(PathBuf),
    #[error("{0} is not a usable path: workmate needs UTF-8")]
    NotUtf8(PathBuf),
    #[error(transparent)]
    Db(#[from] DbError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub directory: PathBuf,
    pub created_at: i64,
}

/// Whether the bound directory is still where the workspace says it is.
///
/// A missing directory is **not** an error: the row survives so the user can
/// re-point it. Deleting the workspace instead would take its runs, grants and
/// memory associations with it because someone unplugged an external drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Binding {
    Present,
    Missing,
}

const COLUMNS: &str = "id, name, directory, created_at";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: r.get(0)?,
        name: r.get(1)?,
        directory: PathBuf::from(r.get::<_, String>(2)?),
        created_at: r.get(3)?,
    })
}

/// A directory as workmate will store it, or why it cannot be stored.
fn usable_dir(p: &Path) -> Result<PathBuf, WorkspaceError> {
    let canonical =
        std::fs::canonicalize(p).map_err(|_| WorkspaceError::NotADirectory(p.to_owned()))?;
    if !canonical.is_dir() {
        return Err(WorkspaceError::NotADirectory(p.to_owned()));
    }
    if path_text(&canonical).is_none() {
        return Err(WorkspaceError::NotUtf8(p.to_owned()));
    }
    Ok(canonical)
}

/// A `UNIQUE` failure on `directory` is the one constraint a user can trip by
/// normal use, so it is translated rather than surfaced as SQLite noise.
fn is_unique_violation(e: &DbError) -> bool {
    matches!(
        e,
        DbError::Sqlite(rusqlite::Error::SqliteFailure(f, _))
            if f.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

/// Bind `directory` to a new workspace.
///
/// # Errors
/// [`WorkspaceError::NotADirectory`] if the path is missing or is a file,
/// [`WorkspaceError::AlreadyBound`] if another workspace holds it, or
/// [`WorkspaceError::Db`] if the write fails.
pub fn create(db: &Db, name: &str, directory: &Path) -> Result<Workspace, WorkspaceError> {
    let directory = usable_dir(directory)?;
    let ws = Workspace {
        id: new_id("ws"),
        name: name.to_owned(),
        directory,
        created_at: now_ms(),
    };
    let text = path_text(&ws.directory).ok_or_else(|| WorkspaceError::NotUtf8(ws.directory.clone()))?;
    db.with(|c| {
        c.execute(
            "INSERT INTO workspace (id, name, directory, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![&ws.id, &ws.name, text, ws.created_at],
        )
    })
    .map_err(|e| {
        if is_unique_violation(&e) {
            WorkspaceError::AlreadyBound(ws.directory.clone())
        } else {
            WorkspaceError::Db(e)
        }
    })?;
    Ok(ws)
}

/// Every workspace, oldest first.
///
/// # Errors
/// Returns [`WorkspaceError::Db`] if the read fails.
pub fn list(db: &Db) -> Result<Vec<Workspace>, WorkspaceError> {
    let out = db.with(|c| {
        let mut stmt =
            c.prepare(&format!("SELECT {COLUMNS} FROM workspace ORDER BY created_at, id"))?;
        let rows = stmt.query_map([], row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })?;
    Ok(out)
}

/// One workspace by id.
///
/// # Errors
/// [`WorkspaceError::NotFound`] if no such workspace exists.
pub fn open(db: &Db, id: &str) -> Result<Workspace, WorkspaceError> {
    db.with(|c| {
        c.query_row(&format!("SELECT {COLUMNS} FROM workspace WHERE id = ?1"), [id], row)
    })
    .map_err(|e| match e {
        DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows) => {
            WorkspaceError::NotFound(id.to_owned())
        }
        other => WorkspaceError::Db(other),
    })
}

/// Re-point a workspace at a directory that moved.
///
/// The id is unchanged, which is the whole point: every row that referenced
/// this workspace still does.
///
/// # Errors
/// As [`create`], plus [`WorkspaceError::NotFound`] if the id is unknown.
pub fn relocate(db: &Db, id: &str, directory: &Path) -> Result<Workspace, WorkspaceError> {
    let directory = usable_dir(directory)?;
    let text =
        path_text(&directory).ok_or_else(|| WorkspaceError::NotUtf8(directory.clone()))?;
    let changed = db
        .with(|c| {
            c.execute(
                "UPDATE workspace SET directory = ?1 WHERE id = ?2",
                rusqlite::params![text, id],
            )
        })
        .map_err(|e| {
            if is_unique_violation(&e) {
                WorkspaceError::AlreadyBound(directory.clone())
            } else {
                WorkspaceError::Db(e)
            }
        })?;
    if changed == 0 {
        return Err(WorkspaceError::NotFound(id.to_owned()));
    }
    open(db, id)
}

/// Remove a workspace.
///
/// Its runs, grants and automations go with it. Its **memories do not**: they
/// are associated through `memory_scope` and merely detach, which is the
/// difference between forgetting a folder and forgetting what was learned in
/// it (ticket 003).
///
/// # Errors
/// [`WorkspaceError::NotFound`] if the id is unknown.
pub fn remove(db: &Db, id: &str) -> Result<(), WorkspaceError> {
    let changed = db.with(|c| {
        // `memory_scope.scope_id` is polymorphic — a workspace id, a role id,
        // or null for global — so it carries no foreign key and no cascade can
        // reach it. Detaching is therefore this function's job, in the same
        // transaction as the delete: a crash between the two would leave
        // memories scoped to a workspace that no longer exists.
        let tx = c.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM memory_scope WHERE scope_kind = 'workspace' AND scope_id = ?1",
            [id],
        )?;
        let changed = tx.execute("DELETE FROM workspace WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(changed)
    })?;
    if changed == 0 {
        return Err(WorkspaceError::NotFound(id.to_owned()));
    }
    Ok(())
}

/// Whether the bound directory is present on disk right now.
#[must_use]
pub fn binding(ws: &Workspace) -> Binding {
    if ws.directory.is_dir() {
        Binding::Present
    } else {
        Binding::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real directory on disk, removed when the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir()
                .join(format!("workmate-ws-{}-{tag}", std::process::id()));
            std::fs::create_dir_all(&p).expect("mkdir");
            // Canonicalised so comparisons hold on macOS, where the temp
            // directory is reached through a symlink.
            Self(std::fs::canonicalize(&p).expect("canonicalize"))
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    #[test]
    fn a_workspace_binds_to_a_directory_and_is_listed() {
        let db = Db::open_in_memory().expect("open");
        let dir = TempDir::new("bind");
        let ws = create(&db, "acme", dir.path()).expect("create");

        assert_eq!(ws.name, "acme");
        assert_eq!(ws.directory, dir.path());
        assert!(ws.id.starts_with("ws_"), "ids are opaque and prefixed");
        assert_eq!(list(&db).expect("list"), vec![ws]);
    }

    #[test]
    fn a_directory_is_bound_to_at_most_one_workspace() {
        let db = Db::open_in_memory().expect("open");
        let dir = TempDir::new("once");
        create(&db, "first", dir.path()).expect("create");

        let err = create(&db, "second", dir.path()).expect_err("must refuse");
        assert!(
            matches!(err, WorkspaceError::AlreadyBound(p) if p == dir.path()),
            "a second binding must be reported as such, not as a raw constraint failure",
        );
    }

    #[test]
    fn a_path_that_is_not_a_directory_is_refused_up_front() {
        let db = Db::open_in_memory().expect("open");
        let dir = TempDir::new("file");
        let file = dir.path().join("not-a-dir");
        std::fs::write(&file, b"x").expect("write");

        let err = create(&db, "nope", &file).expect_err("must refuse");
        assert!(matches!(err, WorkspaceError::NotADirectory(_)));
    }

    #[test]
    fn a_moved_folder_is_repaired_rather_than_orphaned() {
        let db = Db::open_in_memory().expect("open");
        let from = TempDir::new("from");
        let to = TempDir::new("to");
        let ws = create(&db, "acme", from.path()).expect("create");

        // Rows downstream address the workspace by id, so repairing the
        // binding must leave that id — and therefore every reference — intact.
        db.with(|c| {
            c.execute(
                "INSERT INTO run (id,workspace_id,objective,branch,state,created_at)
                 VALUES ('r1',?1,'ship it','workmate/run-r1','running',0)",
                [&ws.id],
            )
        })
        .expect("run");

        let moved = relocate(&db, &ws.id, to.path()).expect("relocate");
        assert_eq!(moved.id, ws.id, "repair must not mint a new identity");
        assert_eq!(moved.directory, to.path());

        let run_owner: String = db
            .with(|c| c.query_row("SELECT workspace_id FROM run WHERE id='r1'", [], |r| r.get(0)))
            .expect("owner");
        assert_eq!(run_owner, ws.id, "the run is still attached to its workspace");
    }

    #[test]
    fn a_missing_directory_is_reported_rather_than_hidden() {
        let db = Db::open_in_memory().expect("open");
        let dir = TempDir::new("gone");
        let ws = create(&db, "acme", dir.path()).expect("create");
        assert_eq!(binding(&ws), Binding::Present);

        std::fs::remove_dir_all(dir.path()).expect("rm");
        // The row survives so the user can repair it; only the binding is lost.
        let reopened = open(&db, &ws.id).expect("still there");
        assert_eq!(binding(&reopened), Binding::Missing);
    }

    #[test]
    fn opening_an_unknown_workspace_is_a_named_error() {
        let db = Db::open_in_memory().expect("open");
        let err = open(&db, "ws_nope").expect_err("must fail");
        assert!(matches!(err, WorkspaceError::NotFound(id) if id == "ws_nope"));
    }

    #[test]
    fn removing_a_workspace_detaches_memory_instead_of_deleting_it() {
        let db = Db::open_in_memory().expect("open");
        let dir = TempDir::new("detach");
        let ws = create(&db, "acme", dir.path()).expect("create");

        db.with(|c| {
            c.execute(
                "INSERT INTO memory (id,subject,claim,recorded_at) VALUES ('m1','pg','uses 5433',0)",
                [],
            )?;
            c.execute(
                "INSERT INTO memory_scope (memory_id,scope_kind,scope_id)
                 VALUES ('m1','workspace',?1)",
                [&ws.id],
            )
        })
        .expect("memory");

        remove(&db, &ws.id).expect("remove");

        let memories: i64 = db
            .with(|c| c.query_row("SELECT count(*) FROM memory", [], |r| r.get(0)))
            .expect("count");
        let scopes: i64 = db
            .with(|c| c.query_row("SELECT count(*) FROM memory_scope", [], |r| r.get(0)))
            .expect("count");
        assert_eq!(memories, 1, "removing a folder must never delete what was learned");
        assert_eq!(scopes, 0, "only the association goes");
        assert!(list(&db).expect("list").is_empty());
    }
}
