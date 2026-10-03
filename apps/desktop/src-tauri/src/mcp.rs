//! MCP server configuration.
//!
//! The runtime is the engine's; the configuration is workmate's. A server is
//! stored per workspace (or for all), delivered to the engine as a session
//! starts, and its tools are gated by the permission surface: they always ask.
//!
//! Adding a server is **not** a sidecar operation. A local server is a command
//! the machine will run, so only the user — through a Tauri command — can
//! register one. The sidecar can read what is enabled and nothing more
//! (ticket 018).

use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::db::{now_ms, Db, DbError};
use crate::ids::new_id;

/// Names workmate itself registers with the engine.
const RESERVED: &[&str] = &["workmate-memory", "workmate_memory"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Transport {
    Local {
        command: Vec<String>,
        #[serde(default)]
        environment: std::collections::BTreeMap<String, String>,
    },
    Remote {
        url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub id: String,
    /// `None` applies to every workspace.
    pub workspace_id: Option<String>,
    pub name: String,
    #[serde(flatten)]
    pub transport: Transport,
    pub enabled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("a server name is 1-32 characters of a-z, 0-9, `_` or `-`, starting with a letter: it becomes the prefix of every tool it provides")]
    BadName,
    #[error("`{0}` is reserved for workmate's own tools")]
    Reserved(String),
    #[error("a local server needs a command")]
    NoCommand,
    #[error("a remote server needs an http:// or https:// URL")]
    BadUrl,
    #[error("a server named `{0}` already exists here")]
    Duplicate(String),
    #[error("no such server")]
    NotFound,
    #[error(transparent)]
    Db(#[from] DbError),
}

fn valid_name(n: &str) -> bool {
    let mut chars = n.chars();
    n.len() <= 32
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn validate(name: &str, t: &Transport) -> Result<(), McpError> {
    if !valid_name(name) {
        return Err(McpError::BadName);
    }
    if RESERVED.contains(&name) {
        return Err(McpError::Reserved(name.to_owned()));
    }
    match t {
        Transport::Local { command, .. } if !command.first().is_some_and(|c| !c.trim().is_empty()) => {
            Err(McpError::NoCommand)
        }
        Transport::Remote { url } if !(url.starts_with("http://") || url.starts_with("https://")) => {
            Err(McpError::BadUrl)
        }
        _ => Ok(()),
    }
}

/// Register a server.
///
/// # Errors
/// A [`McpError`] for an invalid name or transport, a duplicate, or a failed write.
pub fn add(db: &Db, workspace_id: Option<&str>, name: &str, t: &Transport) -> Result<Server, McpError> {
    validate(name, t)?;
    let id = new_id("mcp");
    let (kind, command, env, url) = match t {
        Transport::Local { command, environment } => (
            "local",
            json!(command).to_string(),
            json!(environment).to_string(),
            None,
        ),
        Transport::Remote { url } => ("remote", "[]".to_owned(), "{}".to_owned(), Some(url.clone())),
    };
    db.with(|c| {
        c.execute(
            "INSERT INTO mcp_server (id,workspace_id,name,kind,command,environment,url,enabled,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,1,?8)",
            params![id, workspace_id, name, kind, command, env, url, now_ms()],
        )
    })
    .map_err(|e| match &e {
        DbError::Sqlite(rusqlite::Error::SqliteFailure(f, _))
            if f.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            McpError::Duplicate(name.to_owned())
        }
        _ => McpError::Db(e),
    })?;
    Ok(Server {
        id,
        workspace_id: workspace_id.map(str::to_owned),
        name: name.to_owned(),
        transport: t.clone(),
        enabled: true,
    })
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Server> {
    let kind: String = r.get(3)?;
    let transport = if kind == "remote" {
        Transport::Remote { url: r.get::<_, Option<String>>(6)?.unwrap_or_default() }
    } else {
        Transport::Local {
            command: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
            environment: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default(),
        }
    };
    Ok(Server {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        name: r.get(2)?,
        transport,
        enabled: r.get::<_, i64>(7)? != 0,
    })
}

const COLS: &str = "id,workspace_id,name,kind,command,environment,url,enabled";

/// Servers that apply to `workspace_id`: its own, plus the global ones. A
/// workspace's server shadows a global one of the same name.
///
/// # Errors
/// [`McpError::Db`] on a read failure.
pub fn applicable(db: &Db, workspace_id: &str) -> Result<Vec<Server>, McpError> {
    let all = db.with(|c| {
        let mut s = c.prepare(&format!(
            "SELECT {COLS} FROM mcp_server WHERE workspace_id = ?1 OR workspace_id IS NULL
             ORDER BY workspace_id IS NULL, created_at, id"
        ))?;
        let rows = s.query_map([workspace_id], row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })?;
    // Own servers sort first, so the first of each name is the one that wins.
    let mut seen = std::collections::HashSet::new();
    Ok(all.into_iter().filter(|s| seen.insert(s.name.clone())).collect())
}

/// Every server, for the settings screen.
///
/// # Errors
/// [`McpError::Db`] on a read failure.
pub fn list(db: &Db) -> Result<Vec<Server>, McpError> {
    Ok(db.with(|c| {
        let mut s = c.prepare(&format!("SELECT {COLS} FROM mcp_server ORDER BY created_at, id"))?;
        let rows = s.query_map([], row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })?)
}

/// Names of the enabled servers that apply, for the permission floor.
///
/// # Errors
/// [`McpError::Db`] on a read failure.
pub fn enabled_names(db: &Db, workspace_id: &str) -> Result<Vec<String>, McpError> {
    Ok(applicable(db, workspace_id)?.into_iter().filter(|s| s.enabled).map(|s| s.name).collect())
}

/// Enable or disable a server.
///
/// # Errors
/// [`McpError::NotFound`] for an unknown id.
pub fn set_enabled(db: &Db, id: &str, enabled: bool) -> Result<(), McpError> {
    let n = db.with(|c| c.execute("UPDATE mcp_server SET enabled=?2 WHERE id=?1", params![id, enabled]))?;
    if n == 0 { Err(McpError::NotFound) } else { Ok(()) }
}

/// Remove a server.
///
/// # Errors
/// [`McpError::NotFound`] for an unknown id.
pub fn remove(db: &Db, id: &str) -> Result<(), McpError> {
    let n = db.with(|c| c.execute("DELETE FROM mcp_server WHERE id=?1", [id]))?;
    if n == 0 { Err(McpError::NotFound) } else { Ok(()) }
}

/// What the sidecar is told: enabled servers as the engine's `POST /mcp` takes
/// them. Nothing here can be used to add or change a server.
///
/// # Errors
/// A message on a read failure.
pub fn engine_configs(db: &Db, args: &Value) -> Result<Vec<Value>, String> {
    let ws = args.get("workspaceId").and_then(Value::as_str).ok_or("missing `workspaceId`")?;
    Ok(applicable(db, ws)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| {
            let config = match s.transport {
                Transport::Local { command, environment } => {
                    json!({"type": "local", "command": command, "environment": environment})
                }
                Transport::Remote { url } => json!({"type": "remote", "url": url}),
            };
            json!({"name": s.name, "config": config})
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{compile_with_mcp, Action};
    use std::path::Path;

    fn ws(db: &Db, id: &str) {
        db.with(|c| {
            c.execute(
                "INSERT INTO workspace (id,name,directory,created_at) VALUES (?1,?1,?2,0)",
                params![id, format!("/{id}")],
            )
        })
        .unwrap();
    }

    fn local(cmd: &[&str]) -> Transport {
        Transport::Local { command: cmd.iter().map(|s| (*s).to_owned()).collect(), environment: std::collections::BTreeMap::new() }
    }

    #[test]
    fn names_become_tool_prefixes_so_they_are_strict() {
        let db = Db::open_in_memory().unwrap();
        for bad in ["", "Git", "1x", "a b", "a*", "x".repeat(33).as_str(), "../x"] {
            assert!(matches!(add(&db, None, bad, &local(&["x"])), Err(McpError::BadName)), "{bad:?}");
        }
        assert!(matches!(add(&db, None, "workmate-memory", &local(&["x"])), Err(McpError::Reserved(_))));
        assert!(add(&db, None, "github", &local(&["npx", "gh-mcp"])).is_ok());
    }

    #[test]
    fn transports_are_validated() {
        let db = Db::open_in_memory().unwrap();
        assert!(matches!(add(&db, None, "a", &local(&[])), Err(McpError::NoCommand)));
        assert!(matches!(add(&db, None, "a", &local(&["  "])), Err(McpError::NoCommand)));
        assert!(matches!(add(&db, None, "a", &Transport::Remote { url: "ftp://x".into() }), Err(McpError::BadUrl)));
        assert!(add(&db, None, "a", &Transport::Remote { url: "https://x.example/mcp".into() }).is_ok());
    }

    #[test]
    fn a_workspace_server_shadows_a_global_one_and_other_workspaces_do_not_see_it() {
        let db = Db::open_in_memory().unwrap();
        ws(&db, "w1");
        ws(&db, "w2");
        add(&db, None, "docs", &Transport::Remote { url: "https://global/mcp".into() }).unwrap();
        add(&db, Some("w1"), "docs", &Transport::Remote { url: "https://w1/mcp".into() }).unwrap();
        add(&db, Some("w1"), "only1", &local(&["x"])).unwrap();

        let w1 = applicable(&db, "w1").unwrap();
        let mut names: Vec<_> = w1.iter().map(|s| s.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["docs", "only1"]);
        let docs = w1.iter().find(|s| s.name == "docs").unwrap();
        assert!(matches!(&docs.transport, Transport::Remote { url } if url == "https://w1/mcp"));
        let w2 = applicable(&db, "w2").unwrap();
        assert_eq!(w2.len(), 1, "w2 sees only the global one");
    }

    #[test]
    fn the_same_name_twice_in_one_scope_is_refused() {
        let db = Db::open_in_memory().unwrap();
        add(&db, None, "a", &local(&["x"])).unwrap();
        assert!(matches!(add(&db, None, "a", &local(&["y"])), Err(McpError::Duplicate(_))));
    }

    #[test]
    fn a_disabled_server_is_neither_delivered_nor_gated() {
        let db = Db::open_in_memory().unwrap();
        ws(&db, "w1");
        let s = add(&db, Some("w1"), "a", &local(&["x"])).unwrap();
        assert_eq!(engine_configs(&db, &json!({"workspaceId": "w1"})).unwrap().len(), 1);
        set_enabled(&db, &s.id, false).unwrap();
        assert!(engine_configs(&db, &json!({"workspaceId": "w1"})).unwrap().is_empty());
        assert!(enabled_names(&db, "w1").unwrap().is_empty());
        assert!(matches!(set_enabled(&db, "nope", true), Err(McpError::NotFound)));
    }

    #[test]
    fn removing_a_workspace_takes_its_servers_but_not_the_global_ones() {
        let db = Db::open_in_memory().unwrap();
        ws(&db, "w1");
        add(&db, None, "g", &local(&["x"])).unwrap();
        add(&db, Some("w1"), "mine", &local(&["x"])).unwrap();
        crate::workspace::remove(&db, "w1").unwrap();
        assert_eq!(list(&db).unwrap().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["g"]);
    }

    #[test]
    fn every_enabled_servers_tools_ask_and_no_grant_can_outrank_that() {
        let rules = compile_with_mcp(Path::new("/p"), Path::new("/wt"), &[], &["github".into()]);
        let idx = rules.iter().position(|r| r.permission == "github_*").expect("floor entry");
        assert_eq!(rules[idx].action, Action::Ask);
        let hard = rules.iter().position(|r| r.pattern == "**/.git").unwrap();
        assert!(idx < hard, "hard denies stay last");
        assert!(rules.iter().skip(idx + 1).all(|r| r.permission != "github_*"), "nothing widens it");
    }

    #[test]
    fn the_sidecar_is_told_only_enabled_configs() {
        let db = Db::open_in_memory().unwrap();
        ws(&db, "w1");
        add(&db, Some("w1"), "gh", &local(&["npx", "x"])).unwrap();
        let out = engine_configs(&db, &json!({"workspaceId": "w1"})).unwrap();
        assert_eq!(out[0]["name"], "gh");
        assert_eq!(out[0]["config"]["type"], "local");
        assert_eq!(out[0]["config"]["command"], json!(["npx", "x"]));
    }
}
