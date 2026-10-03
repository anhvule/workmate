//! What an agent may touch, and how that reaches the engine.
//!
//! Two halves. Stored **grants** are durable widenings the user made — a
//! folder outside the project, a shared assets directory. The **ruleset** is
//! what the engine is actually given: an ordered `PermissionRule[]` that
//! `OpenCode` evaluates last-match-wins, so position in the list *is* the
//! policy.
//!
//! The shape of that policy follows ticket 007. There is no
//! `Input/Output/Misc/Artefacts` convention: the permission surface is derived
//! from git. The user's checkout is readable and never writable, the run's own
//! worktree is writable, `.git/` is writable by nobody, and bash — which the
//! edit rules do not cover at all — is gated by its own globs.
//!
//! Two deliberate limits. A directory whose name contains a glob
//! metacharacter (`*`, `?`, `[`) would compile into a pattern that matches
//! more than itself; workmate does not escape them yet. And a reply of
//! *always* is stored **here** and answered to the engine as `once`, so that
//! `OpenCode`'s own saved-permission store never becomes a second authority on
//! a question workmate has to be able to show, audit and revoke.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::{now_ms, path_text, Db, DbError};
use crate::ids::new_id;
use crate::workspace::Workspace;

#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("unknown permission operation: {0}")]
    UnknownOperation(String),
    #[error("unknown grant source: {0}")]
    UnknownSource(String),
    #[error("{0} is not a usable path: workmate needs UTF-8")]
    NotUtf8(PathBuf),
    #[error("{0} contains `*` or `?`, which the engine cannot match literally; rename the folder or grant its parent")]
    GlobMetacharacter(PathBuf),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Mcp(#[from] crate::mcp::McpError),
}

/// The operations a grant can widen.
///
/// Filesystem only. Bash is **not** here: a shell allowance is compiled from
/// the baseline policy below, never stored, because "push always asks" has to
/// stay true by construction rather than by a user not having clicked *always*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Read,
    Edit,
}

impl Operation {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Edit => "edit",
        }
    }

    /// # Errors
    /// [`PermissionError::UnknownOperation`] if the stored value is not one of
    /// the known operations — which means the row predates this build.
    pub fn parse(s: &str) -> Result<Self, PermissionError> {
        match s {
            "read" => Ok(Self::Read),
            "edit" => Ok(Self::Edit),
            other => Err(PermissionError::UnknownOperation(other.to_owned())),
        }
    }
}

/// Where a grant came from, which is what makes it explainable later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Configured on the workspace.
    Workspace,
    /// Set by the user in settings.
    User,
    /// Answered *always* at a runtime prompt.
    Adhoc,
}

impl Source {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::User => "user",
            Self::Adhoc => "adhoc",
        }
    }

    /// # Errors
    /// [`PermissionError::UnknownSource`] if the stored value is unrecognised.
    pub fn parse(s: &str) -> Result<Self, PermissionError> {
        match s {
            "workspace" => Ok(Self::Workspace),
            "user" => Ok(Self::User),
            "adhoc" => Ok(Self::Adhoc),
            other => Err(PermissionError::UnknownSource(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Allow,
    Deny,
    Ask,
}

/// One entry of `OpenCode`'s `PermissionRuleset`, as `POST /session` takes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rule {
    pub permission: String,
    pub pattern: String,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub id: String,
    pub workspace_id: String,
    pub path: PathBuf,
    pub operation: Operation,
    pub source: Source,
    pub created_at: i64,
}

/// A user's answer to a runtime permission prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reply {
    Once,
    Always,
    Reject,
}

const COLUMNS: &str = "id, workspace_id, path, operation, source, created_at";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Grant> {
    let operation: String = r.get(3)?;
    let source: String = r.get(4)?;
    Ok(Grant {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        path: PathBuf::from(r.get::<_, String>(2)?),
        // A row this build cannot read is a corrupt row, not a recoverable
        // one: treating it as "no grant" would silently narrow the surface.
        operation: Operation::parse(&operation)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?,
        source: Source::parse(&source)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?,
        created_at: r.get(5)?,
    })
}

/// Store a grant.
///
/// # Errors
/// [`PermissionError::NotUtf8`] if the path cannot be stored, or
/// [`PermissionError::Db`] if the write fails.
pub fn grant(
    db: &Db,
    workspace_id: &str,
    path: &Path,
    operation: Operation,
    source: Source,
) -> Result<Grant, PermissionError> {
    let text = path_text(path).ok_or_else(|| PermissionError::NotUtf8(path.to_owned()))?;
    // A grant is an explicit act, so a path the engine would match more widely
    // than the user meant is refused rather than quietly widened (ticket 027).
    if text.contains(['*', '?']) {
        return Err(PermissionError::GlobMetacharacter(path.to_owned()));
    }
    let g = Grant {
        id: new_id("pg"),
        workspace_id: workspace_id.to_owned(),
        path: path.to_owned(),
        operation,
        source,
        created_at: now_ms(),
    };
    db.with(|c| {
        c.execute(
            "INSERT INTO permission_grant (id, workspace_id, path, operation, source, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                &g.id,
                &g.workspace_id,
                text,
                operation.as_str(),
                source.as_str(),
                g.created_at
            ],
        )
    })?;
    Ok(g)
}

/// A workspace's stored grants, oldest first — the order they compile in.
///
/// # Errors
/// [`PermissionError::Db`] if the read fails, including a row this build
/// cannot interpret.
pub fn grants(db: &Db, workspace_id: &str) -> Result<Vec<Grant>, PermissionError> {
    let out = db.with(|c| {
        let mut stmt = c.prepare(&format!(
            "SELECT {COLUMNS} FROM permission_grant WHERE workspace_id = ?1
             ORDER BY created_at, id"
        ))?;
        let rows = stmt.query_map([workspace_id], row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })?;
    Ok(out)
}

/// Withdraw a grant.
///
/// # Errors
/// [`PermissionError::Db`] if the delete fails.
pub fn revoke(db: &Db, grant_id: &str) -> Result<(), PermissionError> {
    db.with(|c| c.execute("DELETE FROM permission_grant WHERE id = ?1", [grant_id]))?;
    Ok(())
}

/// The folders the asset protocol may serve from.
///
/// `assetProtocol.scope` is empty in `tauri.conf.json` and filled in here at
/// runtime, from the bound root and the granted directories only. A grant that
/// names a single *file* contributes nothing: widening it to the parent would
/// expose every sibling the user never granted.
///
/// # Errors
/// [`PermissionError::Db`] if the grants cannot be read.
pub fn granted_directories(db: &Db, ws: &Workspace) -> Result<Vec<PathBuf>, PermissionError> {
    let mut out = Vec::new();
    if ws.directory.is_dir() {
        out.push(ws.directory.clone());
    }
    for g in grants(db, &ws.id)? {
        if g.path.is_dir() && !out.contains(&g.path) {
            out.push(g.path);
        }
    }
    Ok(out)
}

/// Record the user's answer to a runtime prompt.
///
/// Only *always* leaves a trace. The engine is answered `once` either way —
/// see the module note on keeping one authority over the question.
///
/// # Errors
/// As [`grant`].
pub fn record_reply(
    db: &Db,
    workspace_id: &str,
    path: &Path,
    operation: Operation,
    reply: Reply,
) -> Result<Option<Grant>, PermissionError> {
    match reply {
        Reply::Always => grant(db, workspace_id, path, operation, Source::Adhoc).map(Some),
        Reply::Once | Reply::Reject => Ok(None),
    }
}

/// The ruleset for a run of `ws` working in `worktree`.
///
/// # Errors
/// [`PermissionError::Db`] if the grants cannot be read.
pub fn ruleset(db: &Db, ws: &Workspace, worktree: &Path) -> Result<Vec<Rule>, PermissionError> {
    let servers = crate::mcp::enabled_names(db, &ws.id)?;
    Ok(compile_with_mcp(&ws.directory, worktree, &grants(db, &ws.id)?, &servers))
}

fn rule(permission: &str, pattern: impl Into<String>, action: Action) -> Rule {
    Rule {
        permission: permission.to_owned(),
        pattern: pattern.into(),
        action,
    }
}

/// A path as a glob that matches that path and, as nearly as the engine allows,
/// nothing else.
///
/// The engine turns `*` into `.*` and `?` into `.` and regex-escapes everything
/// else, so `[ ] { } ( )` are already literal — but there is no way to escape
/// `*` or `?` (checked against the pinned `1.18.32` matcher). The tightest
/// available spelling of either is `?`: a single-character wildcard instead of
/// an unbounded one. That still matches one stray character at that position,
/// which is why [`grant`] refuses such paths outright; only paths workmate
/// did not choose (the workspace root, the worktree) reach here with one.
fn literal(p: &str) -> String {
    p.replace('*', "?")
}

/// `<p>/**`, without doubling the separator when `p` is a root.
fn under(p: &Path) -> String {
    format!("{}/**", literal(p.to_string_lossy().trim_end_matches('/')))
}

/// Compile a policy into the ordered ruleset the engine evaluates.
///
/// Last match wins, so this reads as four bands: a floor that asks or denies
/// everything, the shell baseline, the widenings (synthetic root, worktree,
/// then stored grants oldest-first), and finally the hard denies that no grant
/// can outrank.
///
/// The workspace root grant is **synthetic** — never a row — so revoking every
/// stored grant cannot lock a user out of their own project.
#[must_use]
pub fn compile(root: &Path, worktree: &Path, grants: &[Grant]) -> Vec<Rule> {
    compile_with_mcp(root, worktree, grants, &[])
}

/// [`compile`], plus a floor entry per MCP server: its tools **always ask**.
///
/// MCP tools are `<server>_<tool>` and run code workmate did not write, with
/// reach workmate cannot see. Like `bash`, they cannot be made durable by a
/// stored grant: the table holds `read` and `edit` only. Whether that is
/// tolerable in use is ticket 028's question.
#[must_use]
pub fn compile_with_mcp(
    root: &Path,
    worktree: &Path,
    grants: &[Grant],
    mcp_servers: &[String],
) -> Vec<Rule> {
    let mut rules = vec![
        // The floor.
        rule("read", "**", Action::Ask),
        rule("edit", "**", Action::Deny),
        rule("bash", "*", Action::Ask),
        // The shell baseline. Enumerated verb by verb rather than as `git *`,
        // because a blanket git allow silently swallows `git push` — and push
        // always asks (ticket 007).
        rule("bash", "git status*", Action::Allow),
        rule("bash", "git diff*", Action::Allow),
        rule("bash", "git log*", Action::Allow),
        rule("bash", "git add*", Action::Allow),
        rule("bash", "git commit*", Action::Allow),
        // The synthetic workspace root: the user's checkout is readable and
        // never writable, because a run works in its own worktree.
        rule("read", under(root), Action::Allow),
        // The run's worktree.
        rule("read", under(worktree), Action::Allow),
        rule("edit", under(worktree), Action::Allow),
    ];
    // Ahead of the widenings and well before the hard denies. Names are
    // validated to `[a-z0-9_-]`, so none can carry a glob character.
    rules.splice(3..3, mcp_servers.iter().map(|n| rule(&format!("{n}_*"), "*", Action::Ask)));
    for g in grants {
        // Both forms, so one grant covers a named file and a named directory
        // without the caller having to say which it meant.
        rules.push(rule(g.operation.as_str(), literal(&g.path.to_string_lossy()), Action::Allow));
        rules.push(rule(g.operation.as_str(), under(&g.path), Action::Allow));
    }
    // The hard denies, last, so no grant above can outrank them.
    rules.push(rule("edit", "**/.git", Action::Deny));
    rules.push(rule("edit", "**/.git/**", Action::Deny));
    rules.push(rule("bash", "git push*", Action::Ask));
    rules.push(rule("bash", "rm -rf*", Action::Deny));
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(root: &str) -> Workspace {
        Workspace {
            id: "ws_1".to_owned(),
            name: "acme".to_owned(),
            directory: PathBuf::from(root),
            created_at: 0,
        }
    }

    /// The engine's matcher, reproduced from the pinned binary: `*` -> `.*`,
    /// `?` -> `.`, every other regex metacharacter escaped, anchored, dotall.
    fn engine_matches(pattern: &str, target: &str) -> bool {
        let mut re = String::from("(?s)^");
        for ch in pattern.chars() {
            match ch {
                '*' => re.push_str(".*"),
                '?' => re.push('.'),
                c if ".+^${}()|[]\\".contains(c) => {
                    re.push('\\');
                    re.push(c);
                }
                c => re.push(c),
            }
        }
        re.push('$');
        regex::Regex::new(&re).expect("valid").is_match(target)
    }

    fn a_grant(path: &str, operation: Operation) -> Grant {
        Grant {
            id: "g_1".to_owned(),
            workspace_id: "ws_1".to_owned(),
            path: PathBuf::from(path),
            operation,
            source: Source::Adhoc,
            created_at: 0,
        }
    }

    /// Under last-match-wins, position *is* meaning: the index of the final
    /// rule that could match a target decides the verdict. These tests assert
    /// order rather than re-implementing the engine's glob matcher, which
    /// would be a second authority on the same question.
    fn last_index(rules: &[Rule], permission: &str, pattern: &str) -> usize {
        rules
            .iter()
            .rposition(|r| r.permission == permission && r.pattern == pattern)
            .unwrap_or_else(|| panic!("no {permission} rule for {pattern} in {rules:#?}"))
    }

    fn action_at(rules: &[Rule], i: usize) -> Action {
        rules[i].action
    }

    #[test]
    fn a_fresh_workspace_allows_its_root_with_nothing_stored() {
        let rules = compile(Path::new("/proj"), Path::new("/wt"), &[]);
        // The workspace root grant is synthetic: it is never written to the
        // permission table, so removing every stored grant cannot lock a user
        // out of their own project.
        let read = last_index(&rules, "read", "/proj/**");
        assert_eq!(action_at(&rules, read), Action::Allow);
    }

    #[test]
    fn the_users_checkout_is_readable_but_never_writable() {
        let rules = compile(Path::new("/proj"), Path::new("/wt"), &[]);

        let root_edit = last_index(&rules, "edit", "**");
        let worktree_edit = last_index(&rules, "edit", "/wt/**");
        assert_eq!(action_at(&rules, root_edit), Action::Deny);
        assert_eq!(action_at(&rules, worktree_edit), Action::Allow);
        assert!(
            rules.iter().all(|r| !(r.permission == "edit"
                && r.pattern == "/proj/**"
                && r.action == Action::Allow)),
            "a run edits its worktree, never the user's checkout",
        );
    }

    #[test]
    fn a_stored_grant_widens_the_surface_beyond_the_worktree() {
        let g = a_grant("/shared/assets", Operation::Read);
        let rules = compile(Path::new("/proj"), Path::new("/wt"), &[g]);

        let dir = last_index(&rules, "read", "/shared/assets/**");
        let exact = last_index(&rules, "read", "/shared/assets");
        assert_eq!(action_at(&rules, dir), Action::Allow);
        assert_eq!(action_at(&rules, exact), Action::Allow, "a grant may name one file");
    }

    #[test]
    fn git_internals_are_never_writable_whatever_was_granted() {
        // The widest grant a user could possibly give.
        let g = a_grant("/", Operation::Edit);
        let rules = compile(Path::new("/proj"), Path::new("/wt"), &[g]);

        let deny = last_index(&rules, "edit", "**/.git/**");
        let widest = last_index(&rules, "edit", "/**");
        assert_eq!(action_at(&rules, deny), Action::Deny);
        assert!(deny > widest, "the .git deny must outrank every grant above it");
        assert!(
            rules[deny..].iter().all(|r| r.action != Action::Allow),
            "nothing may be allowed after the hard denies",
        );
    }

    #[test]
    fn push_always_asks_even_though_commit_is_allowed() {
        let rules = compile(Path::new("/proj"), Path::new("/wt"), &[]);

        let commit = last_index(&rules, "bash", "git commit*");
        let push = last_index(&rules, "bash", "git push*");
        assert_eq!(action_at(&rules, commit), Action::Allow);
        assert_eq!(action_at(&rules, push), Action::Ask, "push always asks");
        assert!(push > commit, "a blanket git allow above would swallow the push rule");
    }

    #[test]
    fn bash_is_gated_by_its_own_globs_and_not_by_the_edit_rules() {
        // Cowork-z's own code concedes bash bypasses the edit taxonomy. Here a
        // filesystem grant, however wide, buys no shell latitude.
        let rules = compile(Path::new("/proj"), Path::new("/wt"), &[a_grant("/", Operation::Edit)]);

        let default = last_index(&rules, "bash", "*");
        assert_eq!(action_at(&rules, default), Action::Ask);
        assert!(
            rules[default + 1..]
                .iter()
                .all(|r| r.permission != "bash" || r.pattern != "*"),
            "the catch-all is the floor, not a later override",
        );
        let destructive = last_index(&rules, "bash", "rm -rf*");
        assert_eq!(action_at(&rules, destructive), Action::Deny);
    }

    #[test]
    fn a_reply_of_always_persists_an_adhoc_grant_and_once_does_not() {
        let db = Db::open_in_memory().expect("open");
        db.with(|c| {
            c.execute(
                "INSERT INTO workspace (id,name,directory,created_at)
                 VALUES ('ws_1','acme','/proj',0)",
                [],
            )
        })
        .expect("workspace");

        let once = record_reply(&db, "ws_1", Path::new("/tmp/x"), Operation::Edit, Reply::Once)
            .expect("once");
        assert!(once.is_none(), "a one-off answer leaves no durable trace");

        let rejected =
            record_reply(&db, "ws_1", Path::new("/tmp/x"), Operation::Edit, Reply::Reject)
                .expect("reject");
        assert!(rejected.is_none());

        let always =
            record_reply(&db, "ws_1", Path::new("/tmp/x"), Operation::Edit, Reply::Always)
                .expect("always")
                .expect("a grant");
        assert_eq!(always.source, Source::Adhoc);
        assert_eq!(grants(&db, "ws_1").expect("grants"), vec![always]);
    }

    #[test]
    fn the_asset_scope_is_the_granted_folders_and_nothing_wider() {
        let db = Db::open_in_memory().expect("open");
        let dir = std::env::temp_dir();
        db.with(|c| {
            c.execute(
                "INSERT INTO workspace (id,name,directory,created_at) VALUES ('ws_1','acme',?1,0)",
                [dir.to_str().expect("utf8")],
            )
        })
        .expect("workspace");
        let w = Workspace {
            id: "ws_1".to_owned(),
            name: "acme".to_owned(),
            directory: dir.clone(),
            created_at: 0,
        };

        // A grant naming a file contributes no directory to the asset scope:
        // widening it to the parent would expose every sibling.
        let file = dir.join("workmate-scope-probe");
        std::fs::write(&file, b"x").expect("write");
        grant(&db, "ws_1", &file, Operation::Read, Source::User).expect("grant");

        let dirs = granted_directories(&db, &w).expect("dirs");
        assert_eq!(dirs, vec![dir], "the bound root, and only real directories");
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn a_grant_round_trips_through_the_table_and_can_be_revoked() {
        let db = Db::open_in_memory().expect("open");
        db.with(|c| {
            c.execute(
                "INSERT INTO workspace (id,name,directory,created_at)
                 VALUES ('ws_1','acme','/proj',0)",
                [],
            )
        })
        .expect("workspace");

        let g = grant(&db, "ws_1", Path::new("/proj/docs"), Operation::Read, Source::Workspace)
            .expect("grant");
        assert_eq!(grants(&db, "ws_1").expect("grants"), vec![g.clone()]);

        revoke(&db, &g.id).expect("revoke");
        assert!(grants(&db, "ws_1").expect("grants").is_empty());
    }

    #[test]
    fn the_ruleset_reads_the_stored_grants_for_that_workspace_only() {
        let db = Db::open_in_memory().expect("open");
        db.with(|c| {
            c.execute(
                "INSERT INTO workspace (id,name,directory,created_at)
                 VALUES ('ws_1','acme','/proj',0),('ws_2','other','/other',0)",
                [],
            )
        })
        .expect("workspaces");
        grant(&db, "ws_2", Path::new("/elsewhere"), Operation::Edit, Source::User).expect("grant");

        let rules = ruleset(&db, &ws("/proj"), Path::new("/wt")).expect("ruleset");
        assert!(
            rules.iter().all(|r| !r.pattern.starts_with("/elsewhere")),
            "one workspace's grant must not leak into another's ruleset",
        );
    }

    #[test]
    fn brackets_and_braces_in_a_folder_name_stay_literal() {
        let rules = compile(Path::new("/p/foo[1]"), Path::new("/wt"), &[]);
        let root = rules.iter().find(|r| r.permission == "read" && r.pattern.contains("foo")).unwrap();
        assert!(engine_matches(&root.pattern, "/p/foo[1]/src/a.rs"));
        assert!(!engine_matches(&root.pattern, "/p/foo1/src/a.rs"), "[1] must not become a class");
    }

    #[test]
    fn a_star_in_a_folder_name_cannot_swallow_its_siblings() {
        let rules = compile(Path::new("/p/report*"), Path::new("/wt"), &[]);
        let root = rules.iter().find(|r| r.permission == "read" && r.pattern.contains("report")).unwrap();
        assert!(engine_matches(&root.pattern, "/p/report*/a.txt"));
        assert!(!engine_matches(&root.pattern, "/p/report_final/a.txt"));
        assert!(!engine_matches(&root.pattern, "/p/reports-2024/a.txt"));
    }

    #[test]
    fn a_question_mark_widens_by_one_character_and_no_more() {
        let rules = compile(Path::new("/p/draft?"), Path::new("/wt"), &[]);
        let root = rules.iter().find(|r| r.permission == "read" && r.pattern.contains("draft")).unwrap();
        assert!(engine_matches(&root.pattern, "/p/draft?/a.txt"));
        assert!(!engine_matches(&root.pattern, "/p/draft-final/a.txt"));
    }

    #[test]
    fn the_worktree_rules_are_escaped_too() {
        let rules = compile(Path::new("/p"), Path::new("/data/we*ird/run"), &[]);
        let edit = rules.iter().find(|r| r.permission == "edit" && r.pattern.contains("run")).unwrap();
        assert!(!engine_matches(&edit.pattern, "/data/weird-and-long/run/x"));
    }

    #[test]
    fn an_explicit_grant_for_a_glob_looking_path_is_refused() {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| c.execute("INSERT INTO workspace (id,name,directory,created_at) VALUES ('w','w','/w',0)", []))
            .unwrap();
        for bad in ["/p/a*", "/p/b?"] {
            assert!(matches!(
                grant(&db, "w", Path::new(bad), Operation::Read, Source::User),
                Err(PermissionError::GlobMetacharacter(_))
            ));
        }
        assert!(grant(&db, "w", Path::new("/p/ok[1]"), Operation::Read, Source::User).is_ok());
    }
}
