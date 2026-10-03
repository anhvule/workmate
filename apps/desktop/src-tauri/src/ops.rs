//! Named persistence operations served to the sidecar.
//!
//! The sidecar orchestrates but never writes SQLite, and it never sends SQL
//! either: raw statements over the pipe would make it a second author of the
//! schema. It names an operation and passes arguments; the statements live here
//! and nowhere else (tickets 024 and 026).

use rusqlite::params;
use serde_json::{json, Value};

use crate::db::{now_ms, Db};

type Rows = Result<Vec<Value>, String>;

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string argument `{key}`"))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn exec(db: &Db, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Result<usize, String> {
    db.with(|c| c.execute(sql, p)).map_err(|e| e.to_string())
}

fn role_upsert(db: &Db, args: &Value) -> Rows {
    let id = str_arg(args, "id")?;
    let allow = args.get("toolAllowlist").cloned().unwrap_or_else(|| json!([])).to_string();
    exec(
        db,
        "INSERT INTO role (id,name,system_prompt,provider_id,model_id,tool_allowlist,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(id) DO UPDATE SET name=?2, system_prompt=?3,
           provider_id=?4, model_id=?5, tool_allowlist=?6",
        params![
            id,
            str_arg(args, "name").unwrap_or(id),
            str_arg(args, "systemPrompt").unwrap_or(""),
            opt_str(args, "providerId"),
            opt_str(args, "modelId"),
            allow,
            now_ms()
        ],
    )?;
    Ok(vec![])
}

fn run_create(db: &Db, args: &Value) -> Rows {
    exec(
        db,
        "INSERT INTO run (id,workspace_id,team_id,objective,branch,state,created_at)
         VALUES (?1,?2,?3,?4,?5,'running',?6)",
        params![
            str_arg(args, "id")?,
            str_arg(args, "workspaceId")?,
            opt_str(args, "teamId"),
            str_arg(args, "objective")?,
            str_arg(args, "branch")?,
            now_ms()
        ],
    )?;
    Ok(vec![])
}

fn run_set_state(db: &Db, args: &Value) -> Rows {
    let n = exec(
        db,
        "UPDATE run SET state=?2 WHERE id=?1",
        params![str_arg(args, "id")?, str_arg(args, "state")?],
    )?;
    if n == 0 {
        return Err("no such run".into());
    }
    Ok(vec![])
}

fn session_record(db: &Db, args: &Value) -> Rows {
    exec(
        db,
        "INSERT INTO session_index (id,run_id,role_id,directory,engine_version,started_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            str_arg(args, "id")?,
            str_arg(args, "runId")?,
            str_arg(args, "roleId")?,
            str_arg(args, "directory")?,
            str_arg(args, "engineVersion")?,
            now_ms()
        ],
    )?;
    Ok(vec![])
}

fn handoff_append(db: &Db, args: &Value) -> Rows {
    exec(
        db,
        "INSERT INTO handoff (id,run_id,from_session,to_session,context,at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            str_arg(args, "id")?,
            str_arg(args, "runId")?,
            str_arg(args, "from")?,
            str_arg(args, "to")?,
            str_arg(args, "context")?,
            now_ms()
        ],
    )?;
    Ok(vec![])
}

fn run_load(db: &Db, args: &Value) -> Rows {
    let id = str_arg(args, "id")?;
    db.with(|c| {
        let run = c
            .query_row(
                "SELECT id,workspace_id,team_id,objective,branch,state,created_at FROM run WHERE id=?1",
                [id],
                |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?, "workspaceId": r.get::<_, String>(1)?,
                        "teamId": r.get::<_, Option<String>>(2)?, "objective": r.get::<_, String>(3)?,
                        "branch": r.get::<_, String>(4)?, "state": r.get::<_, String>(5)?,
                        "createdAt": r.get::<_, i64>(6)?,
                    }))
                },
            )
            .ok();
        let Some(mut run) = run else { return Ok(vec![]) };
        let mut s = c.prepare(
            "SELECT id,role_id,directory,engine_version,started_at FROM session_index
             WHERE run_id=?1 ORDER BY started_at,id",
        )?;
        let sessions = s
            .query_map([id], |r| {
                Ok(json!({"id": r.get::<_, String>(0)?, "role": r.get::<_, String>(1)?,
                    "directory": r.get::<_, String>(2)?, "engineVersion": r.get::<_, String>(3)?,
                    "startedAt": r.get::<_, i64>(4)?}))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut h = c.prepare(
            "SELECT from_session,to_session,context,at FROM handoff WHERE run_id=?1 ORDER BY at,rowid",
        )?;
        let handoffs = h
            .query_map([id], |r| {
                Ok(json!({"from": r.get::<_, String>(0)?, "to": r.get::<_, String>(1)?,
                    "context": r.get::<_, String>(2)?, "at": r.get::<_, i64>(3)?}))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        run["sessions"] = Value::Array(sessions);
        run["handoffs"] = Value::Array(handoffs);
        Ok(vec![run])
    })
    .map_err(|e| e.to_string())
}

/// Run one named operation, returning its rows.
///
/// # Errors
/// A message for an unknown operation, a missing argument, or a database
/// failure. It is returned to the sidecar as a `db.error`, never panicked on:
/// the peer is another process.
pub fn dispatch(db: &Db, op: &str, args: &Value) -> Rows {
    match op {
        "role.upsert" => role_upsert(db, args),
        "run.create" => run_create(db, args),
        "run.setState" => run_set_state(db, args),
        "session.record" => session_record(db, args),
        "handoff.append" => handoff_append(db, args),
        "run.load" => run_load(db, args),
        other => Err(format!("unknown operation `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| {
            c.execute("INSERT INTO workspace (id,name,directory,created_at) VALUES ('w','w','/w',0)", [])
        })
        .unwrap();
        dispatch(&db, "role.upsert", &json!({"id":"r1","name":"Planner","systemPrompt":"plan"})).unwrap();
        dispatch(&db, "role.upsert", &json!({"id":"r2","name":"Coder","systemPrompt":"code"})).unwrap();
        dispatch(
            &db,
            "run.create",
            &json!({"id":"run1","workspaceId":"w","objective":"ship","branch":"workmate/run-1"}),
        )
        .unwrap();
        db
    }

    #[test]
    fn a_run_round_trips_with_its_sessions_and_handoffs_in_order() {
        let db = seeded();
        for (id, role) in [("s1", "r1"), ("s2", "r2")] {
            dispatch(&db, "session.record", &json!({"id":id,"runId":"run1","roleId":role,
                "directory":"/wt","engineVersion":"1.18.32"})).unwrap();
        }
        dispatch(&db, "handoff.append", &json!({"id":"h1","runId":"run1","from":"s1","to":"s2","context":"go"})).unwrap();
        let rows = dispatch(&db, "run.load", &json!({"id":"run1"})).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["state"], "running");
        assert_eq!(rows[0]["sessions"].as_array().unwrap().len(), 2);
        assert_eq!(rows[0]["handoffs"][0]["context"], "go");
    }

    #[test]
    fn state_changes_are_validated_by_the_schema_not_trusted_from_the_peer() {
        let db = seeded();
        dispatch(&db, "run.setState", &json!({"id":"run1","state":"paused"})).unwrap();
        assert!(dispatch(&db, "run.setState", &json!({"id":"run1","state":"bogus"})).is_err());
        assert!(dispatch(&db, "run.setState", &json!({"id":"nope","state":"done"})).is_err());
    }

    #[test]
    fn an_unknown_operation_is_an_error_not_a_panic() {
        let db = seeded();
        assert!(dispatch(&db, "drop.everything", &json!({})).unwrap_err().contains("unknown"));
        assert!(dispatch(&db, "run.load", &json!({})).is_err());
    }

    #[test]
    fn loading_a_missing_run_returns_no_rows() {
        let db = seeded();
        assert!(dispatch(&db, "run.load", &json!({"id":"zzz"})).unwrap().is_empty());
    }
}
