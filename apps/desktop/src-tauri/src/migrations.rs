//! The migration ladder.
//!
//! Linear and append-only: each step runs in its own transaction, and the
//! applied version lives in `schema_meta`. Cowork-z arrived at this shape after
//! a review finding, and the reasoning holds — a half-applied migration that
//! cannot be rolled back is unrecoverable on a user's machine.
//!
//! **Never edit a migration that has shipped.** Add the next one.

use rusqlite::{Connection, Transaction};

/// One step of the ladder. `sql` is applied inside its own transaction.
struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

/// Schema v1.
///
/// The load-bearing detail is what is *absent*: `memory` has no `workspace_id`
/// and no cascade. Association runs through `memory_scope`, so removing a
/// workspace detaches memories instead of deleting them. Cowork-z cascades
/// every table off `workspace_id`; doing the same here would delete workmate's
/// differentiator along with a removed folder (ticket 003).
const V1: &str = r"
CREATE TABLE workspace (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    directory   TEXT NOT NULL UNIQUE,
    created_at  INTEGER NOT NULL
);

CREATE TABLE role (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    system_prompt  TEXT NOT NULL,
    provider_id    TEXT,
    model_id       TEXT,
    tool_allowlist TEXT NOT NULL DEFAULT '[]',
    created_at     INTEGER NOT NULL
);

CREATE TABLE team (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE team_role (
    team_id  TEXT NOT NULL REFERENCES team(id) ON DELETE CASCADE,
    role_id  TEXT NOT NULL REFERENCES role(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    PRIMARY KEY (team_id, position)
);

CREATE TABLE run (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    team_id      TEXT REFERENCES team(id) ON DELETE SET NULL,
    objective    TEXT NOT NULL,
    branch       TEXT NOT NULL,
    state        TEXT NOT NULL CHECK (state IN
                   ('running','paused','blocked','done','archived')),
    created_at   INTEGER NOT NULL
);

-- An index into OpenCode's session store, never a copy of the transcript
-- (ticket 010). `engine_version` lets a resume across an upgrade warn rather
-- than silently misbehave.
CREATE TABLE session_index (
    id             TEXT PRIMARY KEY,
    run_id         TEXT NOT NULL REFERENCES run(id) ON DELETE CASCADE,
    role_id        TEXT NOT NULL REFERENCES role(id),
    directory      TEXT NOT NULL,
    engine_version TEXT NOT NULL,
    started_at     INTEGER NOT NULL
);

CREATE TABLE handoff (
    id           TEXT PRIMARY KEY,
    run_id       TEXT NOT NULL REFERENCES run(id) ON DELETE CASCADE,
    from_session TEXT NOT NULL REFERENCES session_index(id) ON DELETE CASCADE,
    to_session   TEXT NOT NULL REFERENCES session_index(id) ON DELETE CASCADE,
    context      TEXT NOT NULL,
    at           INTEGER NOT NULL
);

-- Deliberately free of any workspace column. See the note above.
CREATE TABLE memory (
    id              TEXT PRIMARY KEY,
    subject         TEXT NOT NULL,
    claim           TEXT NOT NULL,
    recorded_at     INTEGER NOT NULL,
    recorded_by_run TEXT,
    superseded_by   TEXT REFERENCES memory(id),
    pinned          INTEGER NOT NULL DEFAULT 0
);

-- The association. Deleting a workspace deletes rows *here* and nowhere else.
CREATE TABLE memory_scope (
    memory_id  TEXT NOT NULL REFERENCES memory(id) ON DELETE CASCADE,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('global','workspace','role')),
    scope_id   TEXT,
    PRIMARY KEY (memory_id, scope_kind, scope_id)
);

CREATE INDEX memory_scope_by_scope ON memory_scope(scope_kind, scope_id);
CREATE INDEX memory_live ON memory(superseded_by) WHERE superseded_by IS NULL;

CREATE TABLE permission_grant (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    path         TEXT NOT NULL,
    operation    TEXT NOT NULL,
    source       TEXT NOT NULL CHECK (source IN ('workspace','user','adhoc')),
    created_at   INTEGER NOT NULL
);

CREATE TABLE provider (
    id      TEXT PRIMARY KEY,
    name    TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE automation (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    team_id      TEXT REFERENCES team(id) ON DELETE SET NULL,
    cron         TEXT NOT NULL,
    objective    TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   INTEGER NOT NULL
);

-- Records the keychain account-name format in force, so a future change can
-- find the entries it needs to rewrite (ticket 009).
CREATE TABLE app_settings (
    id                        INTEGER PRIMARY KEY CHECK (id = 1),
    credential_key_version    TEXT NOT NULL DEFAULT 'v1',
    active_provider_id        TEXT REFERENCES provider(id)
);

INSERT INTO app_settings (id) VALUES (1);
";

/// Memory staleness (ticket 006): a "last used" stamp, set whenever `recall`
/// returns a memory, so the panel can show what is never consulted.
const V2: &str = "ALTER TABLE memory ADD COLUMN last_used_at INTEGER;";

/// MCP server configuration (ticket 018). `workspace_id` NULL means every
/// workspace. Config is owned by its workspace — unlike memory, a removed
/// folder's server list has no reason to outlive it.
const V3: &str = r"
CREATE TABLE mcp_server (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT REFERENCES workspace(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('local','remote')),
    command      TEXT NOT NULL DEFAULT '[]',
    environment  TEXT NOT NULL DEFAULT '{}',
    url          TEXT,
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   INTEGER NOT NULL
);
-- A name is a tool-name prefix, so it must be unique where it applies.
CREATE UNIQUE INDEX mcp_server_name ON mcp_server(coalesce(workspace_id, ''), name);
";

/// Cron automations (ticket 019). A fire is history, not state: it survives its
/// run being archived, so `run_id` carries no foreign key.
///
/// V1 reserved an `automation` table before the design existed. Nothing ever
/// wrote to it, so it is replaced rather than altered into shape.
const V4: &str = r"
DROP TABLE automation;
CREATE TABLE automation (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    schedule     TEXT NOT NULL,
    objective    TEXT NOT NULL,
    roles        TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1,
    next_fire_at INTEGER,
    created_at   INTEGER NOT NULL
);

CREATE TABLE automation_fire (
    id            TEXT PRIMARY KEY,
    automation_id TEXT NOT NULL REFERENCES automation(id) ON DELETE CASCADE,
    scheduled_for INTEGER NOT NULL,
    started_at    INTEGER NOT NULL,
    run_id        TEXT,
    outcome       TEXT NOT NULL CHECK (outcome IN
                    ('started','completed','quiet','blocked','failed','skipped_overlap','missed')),
    summary       TEXT NOT NULL DEFAULT '',
    seen          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX automation_fire_recent ON automation_fire(automation_id, started_at DESC);
";

/// Skill catalog (ticket 020). A source is a git repo workmate syncs into its
/// cache; an install is a skill copied into a workspace, with the checksum it
/// had when it was copied so local edits are detectable.
const V5: &str = r"
CREATE TABLE skill_source (
    id             TEXT PRIMARY KEY,
    url            TEXT NOT NULL UNIQUE,
    last_synced_at INTEGER,
    created_at     INTEGER NOT NULL
);

CREATE TABLE skill_install (
    workspace_id TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    origin       TEXT NOT NULL,
    checksum     TEXT NOT NULL,
    installed_at INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, name)
);
";

/// The model a role falls back to when it names none (ticket 022). Credential
/// and model are separate choices; this is only the model half.
const V6: &str = r"
ALTER TABLE app_settings ADD COLUMN default_provider TEXT;
ALTER TABLE app_settings ADD COLUMN default_model TEXT;
";

/// Durable allowances for shell commands and MCP servers, per workspace
/// (ticket 028). A separate table from `permission_grant` on purpose: grants are
/// paths, these are commands, and only these are screened for dangerous verbs.
const V7: &str = r"
CREATE TABLE allowance (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL CHECK (kind IN ('bash','mcp')),
    value        TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    UNIQUE (workspace_id, kind, value)
);
";

const LADDER: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial schema",
        sql: V1,
    },
    Migration {
        version: 2,
        name: "memory last-used stamp",
        sql: V2,
    },
    Migration {
        version: 3,
        name: "mcp servers",
        sql: V3,
    },
    Migration {
        version: 4,
        name: "automations",
        sql: V4,
    },
    Migration {
        version: 5,
        name: "skill catalog",
        sql: V5,
    },
    Migration {
        version: 6,
        name: "default model",
        sql: V6,
    },
    Migration {
        version: 7,
        name: "allowances",
        sql: V7,
    },
];

/// The version the code in this binary expects.
#[must_use]
pub fn target_version() -> i64 {
    LADDER.last().map_or(0, |m| m.version)
}

fn current_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_meta (
            id      INTEGER PRIMARY KEY CHECK (id = 1),
            version INTEGER NOT NULL
         );
         INSERT OR IGNORE INTO schema_meta (id, version) VALUES (1, 0);",
    )?;
    conn.query_row("SELECT version FROM schema_meta WHERE id = 1", [], |r| {
        r.get(0)
    })
}

fn apply(tx: &Transaction<'_>, m: &Migration) -> rusqlite::Result<()> {
    tx.execute_batch(m.sql)?;
    tx.execute("UPDATE schema_meta SET version = ?1 WHERE id = 1", [m.version])?;
    Ok(())
}

/// Bring the database up to [`target_version`], one transaction per step.
///
/// # Errors
/// Returns the underlying `rusqlite` error if a step fails; that step is rolled
/// back and the recorded version is left at the last one that fully applied.
pub fn migrate(conn: &mut Connection) -> rusqlite::Result<i64> {
    let start = current_version(conn)?;
    let mut version = start;
    // The ladder is ascending, so filtering against the starting version is
    // equivalent to filtering against the running one — and keeps the closure
    // from borrowing a value the loop reassigns.
    for m in LADDER.iter().filter(|m| m.version > start) {
        // Named in the log because the first question on a failed upgrade is
        // always "which step?", and by then the user is not at a debugger.
        crate::logs::warn(&format!("applying migration {} ({})", m.version, m.name));
        let tx = conn.transaction()?;
        apply(&tx, m)?;
        tx.commit()?;
        version = m.version;
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn migrated() -> Connection {
        let mut c = Connection::open_in_memory().expect("open");
        c.execute_batch("PRAGMA foreign_keys = ON;").expect("fk");
        migrate(&mut c).expect("migrate");
        c
    }

    #[test]
    fn migrating_twice_is_a_no_op() {
        let mut c = migrated();
        assert_eq!(migrate(&mut c).expect("second"), target_version());
    }

    #[test]
    fn every_ladder_version_is_unique_and_ascending() {
        let versions: Vec<i64> = LADDER.iter().map(|m| m.version).collect();
        let mut sorted = versions.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(versions, sorted, "ladder must be unique and ascending");
        assert!(LADDER.iter().all(|m| !m.name.is_empty()));
    }

    #[test]
    fn a_failing_step_rolls_back_and_leaves_the_version_untouched() {
        let mut c = migrated();
        let before: i64 = c
            .query_row("SELECT version FROM schema_meta WHERE id = 1", [], |r| r.get(0))
            .expect("version");
        let bad = Migration {
            version: 99,
            name: "deliberately broken",
            sql: "CREATE TABLE ok (id TEXT); CREATE TABLE ok (id TEXT);",
        };
        let tx = c.transaction().expect("tx");
        assert!(apply(&tx, &bad).is_err());
        drop(tx);
        let after: i64 = c
            .query_row("SELECT version FROM schema_meta WHERE id = 1", [], |r| r.get(0))
            .expect("version");
        assert_eq!(before, after);
        let leaked: i64 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'ok'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(leaked, 0, "a rolled-back migration must leave nothing behind");
    }

    /// The guardrail for workmate's second differentiator.
    #[test]
    fn deleting_a_workspace_detaches_memory_instead_of_deleting_it() {
        let c = migrated();
        c.execute(
            "INSERT INTO workspace (id, name, directory, created_at) VALUES ('w1','ws','/tmp/w1',0)",
            [],
        )
        .expect("workspace");
        c.execute(
            "INSERT INTO memory (id, subject, claim, recorded_at, pinned)
             VALUES ('m1','build','uses pnpm',0,1)",
            [],
        )
        .expect("memory");
        c.execute(
            "INSERT INTO memory_scope (memory_id, scope_kind, scope_id)
             VALUES ('m1','workspace','w1')",
            [],
        )
        .expect("scope");

        c.execute("DELETE FROM workspace WHERE id = 'w1'", []).expect("delete");

        let memories: i64 = c
            .query_row("SELECT count(*) FROM memory", [], |r| r.get(0))
            .expect("count");
        assert_eq!(memories, 1, "memory must survive its workspace");
    }

    #[test]
    fn deleting_a_run_takes_its_sessions_with_it() {
        let c = migrated();
        c.execute(
            "INSERT INTO workspace (id,name,directory,created_at) VALUES ('w1','ws','/tmp/w1',0)",
            [],
        )
        .expect("workspace");
        c.execute(
            "INSERT INTO role (id,name,system_prompt,created_at) VALUES ('r1','builder','',0)",
            [],
        )
        .expect("role");
        c.execute(
            "INSERT INTO run (id,workspace_id,objective,branch,state,created_at)
             VALUES ('run1','w1','do it','workmate/run-run1','running',0)",
            [],
        )
        .expect("run");
        c.execute(
            "INSERT INTO session_index (id,run_id,role_id,directory,engine_version,started_at)
             VALUES ('s1','run1','r1','/tmp/wt','1.18.32',0)",
            [],
        )
        .expect("session");

        c.execute("DELETE FROM run WHERE id = ?1", params!["run1"]).expect("delete");

        let sessions: i64 = c
            .query_row("SELECT count(*) FROM session_index", [], |r| r.get(0))
            .expect("count");
        assert_eq!(sessions, 0, "a run owns its sessions");
    }

    #[test]
    fn a_run_state_outside_the_lifecycle_is_rejected() {
        let c = migrated();
        c.execute(
            "INSERT INTO workspace (id,name,directory,created_at) VALUES ('w1','ws','/tmp/w1',0)",
            [],
        )
        .expect("workspace");
        let bad = c.execute(
            "INSERT INTO run (id,workspace_id,objective,branch,state,created_at)
             VALUES ('run1','w1','x','b','galloping',0)",
            [],
        );
        assert!(bad.is_err(), "run.state is constrained to the lifecycle");
    }
}
