//! Durable "always allow" for shell commands and MCP servers (ticket 028).
//!
//! Until this, every `bash` call and every MCP tool asked, every time: "push
//! always asks" was true by construction because nothing about the shell could be
//! stored. This keeps that construction and widens it carefully:
//!
//! - **Per workspace.** An allowance belongs to one project and goes with it.
//! - **A command prefix, never a pattern.** The user stores `pnpm test`; workmate
//!   compiles `pnpm test *`, which the engine reads as "this command, with any
//!   arguments". The engine judges every command in a chain on its own, so
//!   `pnpm test; rm -rf ~` does not ride on the allowance — verified against the
//!   pinned engine in `engine.e2e.test.ts`, which will fail if an upgrade
//!   changes that.
//! - **Screened at write time.** A prefix that names a dangerous verb (push,
//!   publish, rm, sudo, curl …), a bare tool (`pnpm` alone would allow
//!   `pnpm publish`), an "anything goes" subcommand (`npx`, `pnpm exec`) or any
//!   shell syntax is refused with a reason. The table can only ever hold things
//!   that passed.
//! - **Push still asks.** Whatever is stored, `git push *` is compiled *after*
//!   every allowance, so no prefix can outrank it.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::{now_ms, Db, DbError};
use crate::ids::new_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bash,
    Mcp,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Mcp => "mcp",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Allowance {
    pub id: String,
    pub workspace_id: String,
    pub kind: Kind,
    pub value: String,
    pub created_at: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Refusal {
    #[error("an allowance needs a command")]
    Empty,
    #[error("that is too long to be a command prefix")]
    TooLong,
    #[error("`{0}` is shell syntax; allow one plain command, like `pnpm test`")]
    ShellSyntax(char),
    #[error("name the command and what it does, like `pnpm test` — `{0}` alone would allow everything it can do")]
    BareTool(String),
    #[error("`{0}` runs arbitrary code, so it can never be always-allowed")]
    RunsAnything(String),
    #[error("`{0}` can change things outside the project or publish them, so it always asks")]
    Dangerous(String),
    #[error("`{0}` is not a valid MCP server name")]
    BadServer(String),
}

#[derive(Debug, thiserror::Error)]
pub enum AllowanceError {
    #[error(transparent)]
    Refused(#[from] Refusal),
    #[error("no such allowance")]
    NotFound,
    #[error(transparent)]
    Db(#[from] DbError),
}

/// Verbs that change things beyond the worktree, reach the network, escalate,
/// or publish. Matched on the first word, or the first two for multiplexers.
const DANGEROUS: &[&str] = &[
    "git push", "git remote", "git config", "git clean", "git reset", "git checkout", "git switch",
    "git rebase", "git filter-branch", "git update-ref", "git gc", "git worktree", "git branch",
    "git tag", "git submodule", "git credential",
    "npm publish", "pnpm publish", "yarn publish", "cargo publish", "gem push", "twine",
    "rm", "rmdir", "mv", "dd", "mkfs", "chmod", "chown", "chgrp", "ln", "truncate", "shred",
    "sudo", "su", "doas", "kill", "pkill", "killall", "launchctl", "systemctl", "crontab",
    "curl", "wget", "ssh", "scp", "sftp", "rsync", "nc", "ncat", "telnet", "ftp",
    "open", "osascript", "defaults", "security", "networksetup", "docker", "kubectl", "gh",
];

/// Commands, or subcommands, whose whole job is to run something else.
const RUNS_ANYTHING: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "eval", "exec", "env", "xargs", "nohup", "time", "watch",
    "npx", "pnpx", "bunx", "pnpm exec", "pnpm dlx", "npm exec", "yarn dlx", "yarn exec", "bun x",
    "node", "deno", "bun", "python", "python3", "ruby", "perl", "php", "lua",
];

/// Tools that are only meaningful with a subcommand: alone, they allow it all.
const MULTIPLEXERS: &[&str] = &[
    "git", "npm", "pnpm", "yarn", "bun", "cargo", "go", "make", "just", "mvn", "gradle", "pip",
    "uv", "poetry", "dotnet", "swift", "xcodebuild", "rake", "bundle", "composer", "mix",
];

const SHELL_CHARS: &[char] = &[';', '&', '|', '>', '<', '$', '`', '(', ')', '{', '}', '*', '?', '[', ']', '\\', '\n', '\r', '\'', '"', '#', '~'];

/// The normalised prefix workmate would store for `raw`, or why not.
///
/// # Errors
/// A [`Refusal`] naming the reason.
pub fn screen_command(raw: &str) -> Result<String, Refusal> {
    // A trailing ` *` is what the engine suggests; the user means the command.
    let raw = raw.trim();
    let trimmed = raw.strip_suffix(" *").unwrap_or(raw).trim();
    if trimmed.is_empty() {
        return Err(Refusal::Empty);
    }
    if trimmed.len() > 120 {
        return Err(Refusal::TooLong);
    }
    if let Some(c) = trimmed.chars().find(|c| SHELL_CHARS.contains(c)) {
        return Err(Refusal::ShellSyntax(c));
    }
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    let normal = words.join(" ");
    let first = words[0].rsplit('/').next().unwrap_or(words[0]);
    let two = words.get(1).map(|w| format!("{first} {w}"));

    let hits = |list: &[&str]| list.iter().any(|v| *v == first || Some(*v) == two.as_deref());
    // `exec`, `dlx` and `x` only matter as the subcommand; `-c`/`-e` anywhere.
    let sub_runs = words.get(1).is_some_and(|w| matches!(*w, "exec" | "dlx" | "x"));
    let flag_runs = words.iter().skip(1).any(|w| matches!(*w, "-c" | "-e" | "--eval"));
    if hits(DANGEROUS) {
        return Err(Refusal::Dangerous(normal));
    }
    if hits(RUNS_ANYTHING) || sub_runs || flag_runs {
        return Err(Refusal::RunsAnything(normal));
    }
    if words.len() == 1 && MULTIPLEXERS.contains(&first) {
        return Err(Refusal::BareTool(normal));
    }
    Ok(normal)
}

fn screen(kind: Kind, value: &str) -> Result<String, Refusal> {
    match kind {
        Kind::Bash => screen_command(value),
        Kind::Mcp => {
            let v = value.trim();
            let ok = v.len() <= 32
                && v.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && v.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
            if ok { Ok(v.to_owned()) } else { Err(Refusal::BadServer(v.to_owned())) }
        }
    }
}

/// Store an allowance, after screening it.
///
/// # Errors
/// [`AllowanceError::Refused`] with the reason, or a database failure. Adding
/// one that already exists returns the existing row.
pub fn add(db: &Db, workspace_id: &str, kind: Kind, value: &str) -> Result<Allowance, AllowanceError> {
    let value = screen(kind, value)?;
    db.with(|c| {
        c.execute(
            "INSERT OR IGNORE INTO allowance (id, workspace_id, kind, value, created_at) VALUES (?1,?2,?3,?4,?5)",
            params![new_id("allow"), workspace_id, kind.as_str(), value, now_ms()],
        )
    })?;
    list(db, workspace_id)?
        .into_iter()
        .find(|a| a.kind == kind && a.value == value)
        .ok_or(AllowanceError::NotFound)
}

/// A workspace's allowances, oldest first.
///
/// # Errors
/// [`AllowanceError::Db`] on a read failure.
pub fn list(db: &Db, workspace_id: &str) -> Result<Vec<Allowance>, AllowanceError> {
    Ok(db.with(|c| {
        let mut s = c.prepare(
            "SELECT id, workspace_id, kind, value, created_at FROM allowance WHERE workspace_id=?1 ORDER BY created_at, id",
        )?;
        let rows = s.query_map([workspace_id], |r| {
            let kind: String = r.get(2)?;
            Ok(Allowance {
                id: r.get(0)?,
                workspace_id: r.get(1)?,
                kind: if kind == "mcp" { Kind::Mcp } else { Kind::Bash },
                value: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })?)
}

/// Revoke an allowance.
///
/// # Errors
/// [`AllowanceError::NotFound`] for an unknown id.
pub fn remove(db: &Db, id: &str) -> Result<(), AllowanceError> {
    let n = db.with(|c| c.execute("DELETE FROM allowance WHERE id=?1", [id]))?;
    if n == 0 { Err(AllowanceError::NotFound) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_project_commands_are_accepted_and_normalised() {
        for (raw, want) in [
            ("pnpm test", "pnpm test"),
            ("  cargo   test ", "cargo test"),
            ("pnpm test *", "pnpm test"),
            ("make check", "make check"),
            ("pytest", "pytest"),
            ("git status", "git status"),
            ("git diff --stat", "git diff --stat"),
            ("./scripts/lint.sh", "./scripts/lint.sh"),
        ] {
            assert_eq!(screen_command(raw).as_deref(), Ok(want), "{raw:?}");
        }
    }

    #[test]
    fn dangerous_verbs_are_refused_whatever_follows() {
        for raw in ["git push", "git push origin main", "git reset --hard", "npm publish", "rm -rf build", "sudo make install", "curl example.com", "/bin/rm x", "gh pr create", "docker run x"] {
            assert!(matches!(screen_command(raw), Err(Refusal::Dangerous(_))), "{raw:?}");
        }
    }

    #[test]
    fn anything_whose_job_is_running_other_code_is_refused() {
        for raw in ["npx something", "pnpm exec vitest", "pnpm dlx x", "node script.js", "bash build.sh", "python -c print", "env FOO=1 make", "xargs rm", "bun x tsc"] {
            assert!(matches!(screen_command(raw), Err(Refusal::RunsAnything(_))), "{raw:?}");
        }
    }

    #[test]
    fn a_bare_tool_would_allow_all_it_can_do() {
        for raw in ["pnpm", "git", "cargo", "make", "npm"] {
            assert!(matches!(screen_command(raw), Err(Refusal::BareTool(_))), "{raw:?}");
        }
    }

    #[test]
    fn shell_syntax_is_refused_so_a_prefix_cannot_smuggle_a_second_command() {
        for raw in ["pnpm test; rm -rf ~", "pnpm test && curl x", "pnpm test | sh", "pnpm test $(id)", "pnpm test `id`", "pnpm te*", "pnpm test > out", "pnpm test 'x'", "pnpm test ~"] {
            assert!(matches!(screen_command(raw), Err(Refusal::ShellSyntax(_))), "{raw:?}");
        }
        assert_eq!(screen_command(""), Err(Refusal::Empty));
        assert_eq!(screen_command(&"a ".repeat(70)), Err(Refusal::TooLong));
    }

    fn db_with(ws: &[&str]) -> Db {
        let db = Db::open_in_memory().unwrap();
        for w in ws {
            db.with(|c| c.execute("INSERT INTO workspace (id,name,directory,created_at) VALUES (?1,?1,?2,0)", params![w, format!("/{w}")])).unwrap();
        }
        db
    }

    #[test]
    fn allowances_belong_to_one_workspace_and_go_with_it() {
        let db = db_with(&["w1", "w2"]);
        add(&db, "w1", Kind::Bash, "pnpm test").unwrap();
        add(&db, "w1", Kind::Mcp, "github").unwrap();
        assert_eq!(list(&db, "w1").unwrap().len(), 2);
        assert!(list(&db, "w2").unwrap().is_empty(), "another project sees none of them");
        let again = add(&db, "w1", Kind::Bash, "pnpm test *").unwrap();
        assert_eq!(list(&db, "w1").unwrap().len(), 2, "the same command once");
        remove(&db, &again.id).unwrap();
        assert_eq!(list(&db, "w1").unwrap().len(), 1);
        crate::workspace::remove(&db, "w1").unwrap();
        assert!(list(&db, "w1").unwrap().is_empty());
        assert!(matches!(remove(&db, "nope"), Err(AllowanceError::NotFound)));
    }

    #[test]
    fn a_refused_allowance_is_never_stored() {
        let db = db_with(&["w1"]);
        assert!(matches!(add(&db, "w1", Kind::Bash, "git push"), Err(AllowanceError::Refused(Refusal::Dangerous(_)))));
        assert!(matches!(add(&db, "w1", Kind::Mcp, "Bad Name"), Err(AllowanceError::Refused(Refusal::BadServer(_)))));
        assert!(list(&db, "w1").unwrap().is_empty());
    }
}
