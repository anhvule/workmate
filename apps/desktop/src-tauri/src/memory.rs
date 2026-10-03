//! The memory store: claims with provenance, associated with scopes.
//!
//! Memory rows are never owned by a workspace. The `memory_scope` table
//! associates them, so removing a workspace detaches and never deletes
//! (tickets 003 and 006). Capture is an explicit `remember` call; nothing here
//! mines a transcript. A contradicting claim **supersedes** the old one rather
//! than overwriting it, so what workmate believed stays inspectable.

use std::collections::HashMap;
use std::fmt::Write as _;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use crate::db::{now_ms, Db};
use crate::ids::new_id;

const MAX_SUBJECT: usize = 120;
const MAX_CLAIM: usize = 600;

/// A scope as the association table stores it: `(kind, id)`, with `id` absent
/// for global.
pub type ScopeKey = (String, Option<String>);

fn parse_scope(v: &Value) -> Result<ScopeKey, String> {
    let id_of = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("scope is missing `{k}`"))
    };
    match v.get("kind").and_then(Value::as_str) {
        Some("global") => Ok(("global".into(), None)),
        Some("workspace") => Ok(("workspace".into(), Some(id_of("workspaceId")?))),
        Some("role") => Ok(("role".into(), Some(id_of("roleId")?))),
        _ => Err("unknown scope kind".into()),
    }
}

fn scope_json((kind, id): &ScopeKey) -> Value {
    match kind.as_str() {
        "workspace" => json!({"kind": "workspace", "workspaceId": id}),
        "role" => json!({"kind": "role", "roleId": id}),
        _ => json!({"kind": "global"}),
    }
}

fn specificity(kind: &str) -> u8 {
    match kind {
        "role" => 2,
        "workspace" => 1,
        _ => 0,
    }
}

fn parse_scopes(args: &Value) -> Result<Vec<ScopeKey>, String> {
    args.get("scopes")
        .and_then(Value::as_array)
        .map_or_else(|| Ok(vec![]), |a| a.iter().map(parse_scope).collect())
}

/// Text that looks like a credential. Memory is a claim, never a vault
/// (ticket 006): refusing at the writer means no caller can forget to.
#[must_use]
pub fn looks_like_secret(text: &str) -> bool {
    const PREFIXES: &[&str] = &["sk-", "ghp_", "gho_", "github_pat_", "xoxb-", "xoxp-", "AKIA", "AIza"];
    const MARKERS: &[&str] = &["-----BEGIN", "password=", "password:", "api_key=", "apikey=", "secret=", "token="];
    let lower = text.to_ascii_lowercase();
    if MARKERS.iter().any(|m| lower.contains(&m.to_ascii_lowercase())) {
        return true;
    }
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .any(|w| w.len() >= 16 && PREFIXES.iter().any(|p| w.starts_with(p)))
}

fn validate(subject: &str, claim: &str) -> Result<(), String> {
    if subject.trim().is_empty() || claim.trim().is_empty() {
        return Err("a memory needs a subject and a claim".into());
    }
    if subject.chars().count() > MAX_SUBJECT || claim.chars().count() > MAX_CLAIM {
        return Err(format!(
            "too long: subject up to {MAX_SUBJECT} and claim up to {MAX_CLAIM} characters. Memory is a claim, not an archive"
        ));
    }
    if looks_like_secret(subject) || looks_like_secret(claim) {
        return Err("this looks like a credential; secrets are never remembered".into());
    }
    Ok(())
}

fn supersede_same_subject(
    tx: &Connection,
    new_id: &str,
    subject: &str,
    scopes: &[ScopeKey],
) -> rusqlite::Result<Vec<String>> {
    let mut hit = Vec::new();
    for (kind, sid) in scopes {
        let mut stmt = tx.prepare(
            "SELECT m.id FROM memory m JOIN memory_scope s ON s.memory_id = m.id
             WHERE m.superseded_by IS NULL AND m.id != ?1
               AND lower(trim(m.subject)) = lower(trim(?2))
               AND s.scope_kind = ?3 AND s.scope_id IS ?4",
        )?;
        let ids = stmt
            .query_map(params![new_id, subject, kind, sid], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        hit.extend(ids);
    }
    hit.sort();
    hit.dedup();
    for old in &hit {
        tx.execute("UPDATE memory SET superseded_by = ?1 WHERE id = ?2", params![new_id, old])?;
    }
    Ok(hit)
}

/// Record a claim, superseding live claims with the same subject in a shared
/// scope. Returns `{id, superseded}`.
///
/// # Errors
/// A message if the claim is empty, too long, looks like a credential, has no
/// scope, or the write fails.
pub fn remember(db: &Db, args: &Value) -> Result<Vec<Value>, String> {
    let get = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or_default().trim().to_owned();
    let (subject, claim) = (get("subject"), get("claim"));
    validate(&subject, &claim)?;
    let scopes = parse_scopes(args)?;
    if scopes.is_empty() {
        return Err("a memory must be associated with at least one scope".into());
    }
    let id = new_id("mem");
    let run = args.get("runId").and_then(Value::as_str);
    let pinned = args.get("pinned").and_then(Value::as_bool).unwrap_or(false);
    let superseded = db
        .with(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO memory (id,subject,claim,recorded_at,recorded_by_run,pinned)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![id, subject, claim, now_ms(), run, pinned],
            )?;
            for (kind, sid) in &scopes {
                tx.execute(
                    "INSERT INTO memory_scope (memory_id,scope_kind,scope_id) VALUES (?1,?2,?3)",
                    params![id, kind, sid],
                )?;
            }
            let hit = supersede_same_subject(&tx, &id, &subject, &scopes)?;
            tx.commit()?;
            Ok(hit)
        })
        .map_err(|e| e.to_string())?;
    Ok(vec![json!({"id": id, "superseded": superseded})])
}

struct Row {
    json: Value,
    scopes: Vec<ScopeKey>,
    live: bool,
}

fn load_all(c: &Connection) -> rusqlite::Result<Vec<Row>> {
    let mut scopes: HashMap<String, Vec<ScopeKey>> = HashMap::new();
    let mut s = c.prepare("SELECT memory_id,scope_kind,scope_id FROM memory_scope ORDER BY rowid")?;
    for r in s.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))))? {
        let (mid, key) = r?;
        scopes.entry(mid).or_default().push(key);
    }
    let mut m = c.prepare(
        "SELECT id,subject,claim,recorded_at,recorded_by_run,superseded_by,pinned,last_used_at
         FROM memory ORDER BY recorded_at DESC, id",
    )?;
    let rows = m
        .query_map([], |r| {
            let id: String = r.get(0)?;
            let sup: Option<String> = r.get(5)?;
            let sc = scopes.remove(&id).unwrap_or_default();
            Ok(Row {
                live: sup.is_none(),
                json: json!({
                    "id": id, "subject": r.get::<_, String>(1)?, "claim": r.get::<_, String>(2)?,
                    "recordedAt": r.get::<_, i64>(3)?, "recordedByRun": r.get::<_, Option<String>>(4)?,
                    "supersededBy": sup, "pinned": r.get::<_, i64>(6)? != 0,
                    "lastUsedAt": r.get::<_, Option<i64>>(7)?,
                    "scopes": sc.iter().map(scope_json).collect::<Vec<_>>(),
                }),
                scopes: sc,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The narrowest matching specificity, or `None` if no scope applies.
fn best(row: &Row, wanted: &[ScopeKey]) -> Option<u8> {
    row.scopes.iter().filter(|s| wanted.contains(s)).map(|s| specificity(&s.0)).max()
}

/// Memories, optionally filtered to a resolution order of scopes. Live only,
/// unless `includeSuperseded`. Narrowest scope first, newest breaking ties.
///
/// # Errors
/// A message for a malformed scope or a database failure.
pub fn list(db: &Db, args: &Value) -> Result<Vec<Value>, String> {
    let wanted = parse_scopes(args)?;
    let all = args.get("includeSuperseded").and_then(Value::as_bool).unwrap_or(false);
    let mut rows: Vec<(u8, Row)> = db
        .with(load_all)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|r| all || r.live)
        .filter_map(|r| {
            if wanted.is_empty() { Some((0, r)) } else { best(&r, &wanted).map(|b| (b, r)) }
        })
        .collect();
    // Stable: `load_all` is already newest-first, so ties stay newest-first.
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
    Ok(rows.into_iter().map(|(_, r)| r.json).collect())
}

fn relevance(row: &Value, terms: &[String]) -> usize {
    let subject = row["subject"].as_str().unwrap_or_default().to_lowercase();
    let claim = row["claim"].as_str().unwrap_or_default().to_lowercase();
    terms
        .iter()
        .map(|t| usize::from(claim.contains(t.as_str())) + 2 * usize::from(subject.contains(t.as_str())))
        .sum()
}

/// On-demand retrieval for the `recall` tool. Stamps `last_used_at` on what it
/// returns, so staleness is measurable.
///
/// # Errors
/// A message for a malformed scope or a database failure.
pub fn recall(db: &Db, args: &Value) -> Result<Vec<Value>, String> {
    let query = args.get("query").and_then(Value::as_str).unwrap_or_default().to_lowercase();
    let limit = usize::try_from(args.get("limit").and_then(Value::as_u64).unwrap_or(10)).unwrap_or(10);
    let terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(str::to_owned)
        .collect();
    let mut scored: Vec<(usize, Value)> = list(db, &json!({"scopes": args.get("scopes")}))?
        .into_iter()
        .map(|r| (relevance(&r, &terms), r))
        .filter(|(s, _)| terms.is_empty() || *s > 0)
        .collect();
    scored.sort_by_key(|r| std::cmp::Reverse(r.0)); // stable: scope order and recency survive ties
    let hits: Vec<Value> = scored.into_iter().take(limit).map(|(_, r)| r).collect();
    db.with(|c| {
        for h in &hits {
            c.execute("UPDATE memory SET last_used_at = ?1 WHERE id = ?2", params![now_ms(), h["id"].as_str()])?;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    Ok(hits)
}

/// Correct a memory in place (the panel). Corrections keep the same id.
///
/// # Errors
/// A message if the id is unknown, the new text is invalid, or the write fails.
pub fn update(db: &Db, args: &Value) -> Result<Vec<Value>, String> {
    let id = args.get("id").and_then(Value::as_str).ok_or("missing `id`")?;
    let cur = db
        .with(|c| c.query_row("SELECT subject,claim,pinned FROM memory WHERE id=?1", [id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)? != 0))
        }))
        .map_err(|_| "no such memory".to_owned())?;
    let subject = args.get("subject").and_then(Value::as_str).map_or(cur.0, |s| s.trim().to_owned());
    let claim = args.get("claim").and_then(Value::as_str).map_or(cur.1, |s| s.trim().to_owned());
    let pinned = args.get("pinned").and_then(Value::as_bool).unwrap_or(cur.2);
    validate(&subject, &claim)?;
    db.with(|c| {
        c.execute("UPDATE memory SET subject=?2, claim=?3, pinned=?4 WHERE id=?1", params![id, subject, claim, pinned])
    })
    .map_err(|e| e.to_string())?;
    Ok(vec![])
}

/// Forget a memory, and the older versions it superseded: forgetting a belief
/// means forgetting its history too, or the panel would resurrect it.
///
/// # Errors
/// A message if the id is unknown or the delete fails.
pub fn delete(db: &Db, args: &Value) -> Result<Vec<Value>, String> {
    let id = args.get("id").and_then(Value::as_str).ok_or("missing `id`")?;
    let n = db
        .with(|c| {
            let tx = c.unchecked_transaction()?;
            // Newest first so the self-referencing foreign key is never violated.
            let mut chain = vec![id.to_owned()];
            let mut i = 0;
            while i < chain.len() {
                let mut s = tx.prepare("SELECT id FROM memory WHERE superseded_by = ?1")?;
                let older = s.query_map([&chain[i]], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
                chain.extend(older);
                i += 1;
            }
            let mut removed = 0;
            for m in chain.iter().rev() {
                removed += tx.execute("DELETE FROM memory WHERE id = ?1", [m])?;
            }
            tx.commit()?;
            Ok(removed)
        })
        .map_err(|e| e.to_string())?;
    if n == 0 { Err("no such memory".into()) } else { Ok(vec![]) }
}

/// A read-only markdown rendering. SQLite stays canonical; editing a file would
/// make two writers over one store (ticket 006).
///
/// # Errors
/// A message on a database failure.
pub fn export_markdown(db: &Db) -> Result<Vec<Value>, String> {
    let names: HashMap<String, String> = db
        .with(|c| {
            let mut s = c.prepare("SELECT id,name FROM workspace")?;
            let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()
        })
        .map_err(|e| e.to_string())?;
    let mut out = String::from("# workmate memory\n\n_Read-only export. Edit in workmate._\n");
    for m in list(db, &json!({}))? {
        let scopes: Vec<String> = m["scopes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| match s["kind"].as_str() {
                Some("workspace") => names.get(s["workspaceId"].as_str().unwrap_or_default()).cloned().unwrap_or_else(|| "a removed workspace".into()),
                Some("role") => format!("role {}", s["roleId"].as_str().unwrap_or("?")),
                _ => "everywhere".into(),
            })
            .collect();
        let pin = if m["pinned"].as_bool() == Some(true) { " 📌" } else { "" };
        let _ = write!(
            out,
            "\n- **{}**{} — {} _({})_",
            m["subject"].as_str().unwrap_or_default(),
            pin,
            m["claim"].as_str().unwrap_or_default(),
            scopes.join(", ")
        );
    }
    out.push('\n');
    Ok(vec![json!({"markdown": out})])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace;

    fn db_with_workspace() -> (Db, String) {
        let db = Db::open_in_memory().unwrap();
        let dir = std::env::temp_dir();
        let ws = workspace::create(&db, "acme", &dir).unwrap();
        (db, ws.id)
    }

    fn ws_scope(id: &str) -> Value {
        json!({"kind":"workspace","workspaceId":id})
    }

    fn remember_in(db: &Db, subject: &str, claim: &str, scopes: &Value, pinned: bool) -> String {
        let r = remember(db, &json!({"subject":subject,"claim":claim,"scopes":scopes,"pinned":pinned})).unwrap();
        r[0]["id"].as_str().unwrap().to_owned()
    }

    #[test]
    fn memory_survives_the_deletion_of_its_workspace() {
        let (db, ws) = db_with_workspace();
        remember_in(&db, "build", "uses pnpm", &json!([ws_scope(&ws)]), true);
        remember_in(&db, "style", "terse commits", &json!([ws_scope(&ws), {"kind":"global"}]), false);

        workspace::remove(&db, &ws).unwrap();

        let all = list(&db, &json!({})).unwrap();
        assert_eq!(all.len(), 2, "nothing cascades into memory");
        let build = all.iter().find(|m| m["subject"] == "build").unwrap();
        assert_eq!(build["scopes"].as_array().unwrap().len(), 0, "detached, not deleted");
        let style = all.iter().find(|m| m["subject"] == "style").unwrap();
        assert_eq!(style["scopes"][0]["kind"], "global", "other associations are untouched");
    }

    #[test]
    fn a_contradicting_claim_supersedes_and_the_history_is_kept() {
        let (db, ws) = db_with_workspace();
        let old = remember_in(&db, "Package manager", "uses npm", &json!([ws_scope(&ws)]), true);
        let r = remember(&db, &json!({"subject":" package manager ","claim":"uses pnpm","scopes":[ws_scope(&ws)]})).unwrap();
        assert_eq!(r[0]["superseded"], json!([old]));

        let live = list(&db, &json!({})).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0]["claim"], "uses pnpm");
        let history = list(&db, &json!({"includeSuperseded": true})).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history.iter().find(|m| m["id"] == old.as_str()).unwrap()["supersededBy"], live[0]["id"]);
    }

    #[test]
    fn the_same_subject_in_a_different_scope_is_not_a_contradiction() {
        let (db, ws) = db_with_workspace();
        remember_in(&db, "editor", "vim", &json!([{"kind":"global"}]), false);
        remember_in(&db, "editor", "helix", &json!([ws_scope(&ws)]), false);
        assert_eq!(list(&db, &json!({})).unwrap().len(), 2);
    }

    #[test]
    fn the_narrowest_scope_comes_first() {
        let (db, ws) = db_with_workspace();
        remember_in(&db, "g", "global fact", &json!([{"kind":"global"}]), false);
        remember_in(&db, "w", "workspace fact", &json!([ws_scope(&ws)]), false);
        remember_in(&db, "r", "role fact", &json!([{"kind":"role","roleId":"coder"}]), false);
        let order = json!([{"kind":"role","roleId":"coder"}, ws_scope(&ws), {"kind":"global"}]);
        let got: Vec<_> = list(&db, &json!({"scopes": order})).unwrap().iter().map(|m| m["subject"].as_str().unwrap().to_owned()).collect();
        assert_eq!(got, ["r", "w", "g"]);
        let only_global = list(&db, &json!({"scopes": [{"kind":"global"}]})).unwrap();
        assert_eq!(only_global.len(), 1, "another workspace's or role's memory does not leak in");
    }

    #[test]
    fn secrets_and_archives_are_refused_at_the_writer() {
        let (db, ws) = db_with_workspace();
        let try_it = |claim: &str| remember(&db, &json!({"subject":"s","claim":claim,"scopes":[ws_scope(&ws)]}));
        assert!(try_it("the key is sk-abcdefghijklmnopqrstuvwx").unwrap_err().contains("credential"));
        assert!(try_it("password=hunter2").is_err());
        assert!(try_it("-----BEGIN PRIVATE KEY-----").is_err());
        assert!(try_it(&"x".repeat(601)).unwrap_err().contains("archive"));
        assert!(try_it("   ").is_err());
        assert!(try_it("uses pnpm; tests in vitest").is_ok(), "ordinary claims pass");
        assert!(remember(&db, &json!({"subject":"s","claim":"c","scopes":[]})).is_err());
    }

    #[test]
    fn recall_ranks_by_relevance_and_stamps_last_used() {
        let (db, ws) = db_with_workspace();
        remember_in(&db, "testing", "vitest, run with pnpm test", &json!([ws_scope(&ws)]), false);
        remember_in(&db, "deploys", "friday freeze", &json!([ws_scope(&ws)]), false);
        let hits = recall(&db, &json!({"query":"how do we run tests","scopes":[ws_scope(&ws)]})).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["subject"], "testing");
        let after = list(&db, &json!({})).unwrap();
        let t = after.iter().find(|m| m["subject"] == "testing").unwrap();
        let d = after.iter().find(|m| m["subject"] == "deploys").unwrap();
        assert!(t["lastUsedAt"].is_i64());
        assert!(d["lastUsedAt"].is_null(), "unrecalled memories stay unstamped");
    }

    #[test]
    fn the_panel_can_correct_and_forget() {
        let (db, ws) = db_with_workspace();
        let old = remember_in(&db, "db", "uses mysql", &json!([ws_scope(&ws)]), false);
        let newer = remember(&db, &json!({"subject":"db","claim":"uses postgres","scopes":[ws_scope(&ws)]})).unwrap()[0]["id"].as_str().unwrap().to_owned();
        update(&db, &json!({"id":newer,"claim":"uses postgres 16","pinned":true})).unwrap();
        let live = list(&db, &json!({})).unwrap();
        assert_eq!((live[0]["claim"].as_str(), live[0]["pinned"].as_bool()), (Some("uses postgres 16"), Some(true)));
        assert!(update(&db, &json!({"id":newer,"claim":"token=abc123"})).is_err(), "corrections are guarded too");

        delete(&db, &json!({"id":newer})).unwrap();
        assert!(list(&db, &json!({"includeSuperseded": true})).unwrap().iter().all(|m| m["id"] != old.as_str()),
            "forgetting a belief forgets what it replaced");
        assert!(delete(&db, &json!({"id":"nope"})).is_err());
    }

    #[test]
    fn the_markdown_export_names_workspaces_and_never_resurrects_deleted_ones() {
        let (db, ws) = db_with_workspace();
        remember_in(&db, "build", "uses pnpm", &json!([ws_scope(&ws)]), true);
        let md = export_markdown(&db).unwrap()[0]["markdown"].as_str().unwrap().to_owned();
        assert!(md.contains("**build**") && md.contains("acme") && md.contains("Read-only"));
        workspace::remove(&db, &ws).unwrap();
        let md = export_markdown(&db).unwrap()[0]["markdown"].as_str().unwrap().to_owned();
        assert!(md.contains("**build**"));
    }
}
