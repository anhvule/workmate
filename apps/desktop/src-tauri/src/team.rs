//! Teams and roles: the data a run is started from (ticket 003).
//!
//! A role is first-class data — a system prompt, a model, a tool allowlist —
//! not a prompt buried in code, so it can be edited, listed and shipped in a
//! starter pack. A team is an ordered list of them.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::{now_ms, Db, DbError};
use crate::ids::new_id;

#[derive(Debug, thiserror::Error)]
pub enum TeamError {
    #[error("a team needs a name and at least one role")]
    Empty,
    #[error("a role needs an id and a name")]
    BadRole,
    #[error("no such team")]
    NotFound,
    #[error(transparent)]
    Db(#[from] DbError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub tool_allowlist: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: String,
    pub name: String,
    pub roles: Vec<Role>,
}

fn upsert_role(c: &rusqlite::Connection, r: &Role) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO role (id,name,system_prompt,provider_id,model_id,tool_allowlist,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(id) DO UPDATE SET name=?2, system_prompt=?3, provider_id=?4, model_id=?5, tool_allowlist=?6",
        params![
            r.id,
            r.name,
            r.system_prompt,
            r.provider_id,
            r.model_id,
            serde_json::to_string(&r.tool_allowlist).unwrap_or_else(|_| "[]".into()),
            now_ms()
        ],
    )?;
    Ok(())
}

/// Create a team from roles, in order. Roles are created or updated by id.
///
/// # Errors
/// [`TeamError::Empty`] or [`TeamError::BadRole`] for invalid input.
pub fn create(db: &Db, name: &str, roles: &[Role]) -> Result<Team, TeamError> {
    if name.trim().is_empty() || roles.is_empty() {
        return Err(TeamError::Empty);
    }
    if roles.iter().any(|r| r.id.trim().is_empty() || r.name.trim().is_empty()) {
        return Err(TeamError::BadRole);
    }
    let id = new_id("team");
    db.with(|c| {
        let tx = c.unchecked_transaction()?;
        tx.execute("INSERT INTO team (id,name,created_at) VALUES (?1,?2,?3)", params![id, name.trim(), now_ms()])?;
        for (i, r) in roles.iter().enumerate() {
            upsert_role(&tx, r)?;
            tx.execute(
                "INSERT INTO team_role (team_id,role_id,position) VALUES (?1,?2,?3)",
                params![id, r.id, i64::try_from(i).unwrap_or(i64::MAX)],
            )?;
        }
        tx.commit()
    })?;
    Ok(Team { id, name: name.trim().to_owned(), roles: roles.to_vec() })
}

fn roles_of(c: &rusqlite::Connection, team: &str) -> rusqlite::Result<Vec<Role>> {
    let mut s = c.prepare(
        "SELECT r.id,r.name,r.system_prompt,r.provider_id,r.model_id,r.tool_allowlist
         FROM team_role t JOIN role r ON r.id = t.role_id WHERE t.team_id = ?1 ORDER BY t.position",
    )?;
    let rows = s.query_map([team], |r| {
        Ok(Role {
            id: r.get(0)?,
            name: r.get(1)?,
            system_prompt: r.get(2)?,
            provider_id: r.get(3)?,
            model_id: r.get(4)?,
            tool_allowlist: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default(),
        })
    })?;
    rows.collect()
}

/// Every team with its roles in order, oldest first.
///
/// # Errors
/// [`TeamError::Db`] on a read failure.
pub fn list(db: &Db) -> Result<Vec<Team>, TeamError> {
    Ok(db.with(|c| {
        let mut s = c.prepare("SELECT id,name FROM team ORDER BY created_at, id")?;
        let heads = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        heads
            .into_iter()
            .map(|(id, name)| Ok(Team { roles: roles_of(c, &id)?, id, name }))
            .collect::<rusqlite::Result<Vec<_>>>()
    })?)
}

/// Update one role in place (its prompt, model or allowlist).
///
/// # Errors
/// [`TeamError::BadRole`] for an empty id or name.
pub fn update_role(db: &Db, role: &Role) -> Result<(), TeamError> {
    if role.id.trim().is_empty() || role.name.trim().is_empty() {
        return Err(TeamError::BadRole);
    }
    db.with(|c| upsert_role(c, role))?;
    Ok(())
}

/// Delete a team. Roles stay: past runs' sessions refer to them.
///
/// # Errors
/// [`TeamError::NotFound`] for an unknown id.
pub fn remove(db: &Db, id: &str) -> Result<(), TeamError> {
    let n = db.with(|c| c.execute("DELETE FROM team WHERE id=?1", [id]))?;
    if n == 0 { Err(TeamError::NotFound) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(id: &str) -> Role {
        Role {
            id: id.into(),
            name: id.to_uppercase(),
            system_prompt: format!("you are {id}"),
            provider_id: None,
            model_id: None,
            tool_allowlist: vec!["read".into()],
        }
    }

    #[test]
    fn a_team_keeps_its_roles_in_order_with_their_settings() {
        let db = Db::open_in_memory().unwrap();
        let t = create(&db, "Trio", &[role("c"), role("a"), role("b")]).unwrap();
        let got = list(&db).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, t.id);
        assert_eq!(got[0].roles.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["c", "a", "b"]);
        assert_eq!(got[0].roles[0].tool_allowlist, ["read"]);
    }

    #[test]
    fn editing_a_role_changes_it_for_every_team_that_uses_it() {
        let db = Db::open_in_memory().unwrap();
        create(&db, "One", &[role("shared")]).unwrap();
        create(&db, "Two", &[role("shared")]).unwrap();
        let mut r = role("shared");
        r.system_prompt = "new prompt".into();
        r.model_id = Some("m".into());
        update_role(&db, &r).unwrap();
        for t in list(&db).unwrap() {
            assert_eq!(t.roles[0].system_prompt, "new prompt");
            assert_eq!(t.roles[0].model_id.as_deref(), Some("m"));
        }
    }

    #[test]
    fn deleting_a_team_keeps_the_roles_past_runs_point_at() {
        let db = Db::open_in_memory().unwrap();
        let t = create(&db, "T", &[role("keep")]).unwrap();
        remove(&db, &t.id).unwrap();
        assert!(list(&db).unwrap().is_empty());
        let n: i64 = db.with(|c| c.query_row("SELECT count(*) FROM role WHERE id='keep'", [], |r| r.get(0))).unwrap();
        assert_eq!(n, 1);
        assert!(matches!(remove(&db, &t.id), Err(TeamError::NotFound)));
    }

    #[test]
    fn empty_teams_and_nameless_roles_are_refused() {
        let db = Db::open_in_memory().unwrap();
        assert!(matches!(create(&db, "T", &[]), Err(TeamError::Empty)));
        assert!(matches!(create(&db, " ", &[role("a")]), Err(TeamError::Empty)));
        let mut bad = role("a");
        bad.name = String::new();
        assert!(matches!(create(&db, "T", &[bad]), Err(TeamError::BadRole)));
    }
}
