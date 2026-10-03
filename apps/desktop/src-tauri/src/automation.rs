//! Cron automations: scheduled, unattended runs (ticket 019).
//!
//! The unattended part is what makes this the reason worktree isolation landed
//! first: every fire is an ordinary run in its own worktree, so it can never
//! touch the user's checkout, and what it finds waits for them to review.
//!
//! - **One run per fire**, bounded: at most [`MAX_IN_FLIGHT`] automation runs at
//!   once, and an automation never overlaps itself (the late fire is recorded
//!   as skipped, not queued).
//! - **Nobody is watching, so nothing asks.** An unattended run's ruleset turns
//!   every *ask* into a *deny*; the hard denies are untouched. Asking would
//!   either block forever or invite auto-approval, and the run's summary says
//!   what was refused.
//! - **A machine that slept** coalesces: if the slot was missed by less than
//!   [`CATCH_UP`], it fires once now; older, it is recorded as missed. Never a
//!   burst of every slot that was skipped.
//! - **Quiet by default.** The run is told to answer [`NOTHING`] when it finds
//!   nothing; that outcome is history only and its worktree is discarded.

use std::collections::HashSet;

use chrono::{Local, TimeZone};
use rusqlite::params;
use serde::Serialize;
use serde_json::{json, Value};

use crate::cron::Cron;
use crate::db::{now_ms, Db, DbError};
use crate::ids::new_id;

pub const MAX_IN_FLIGHT: usize = 2;
/// A slot later than this counts as the machine having been asleep.
const GRACE_MS: i64 = 10 * 60 * 1000;
/// Beyond this a missed slot is not worth running late.
const CATCH_UP_MS: i64 = 24 * 60 * 60 * 1000;
/// What an unattended run answers when there is nothing to report.
pub const NOTHING: &str = "NOTHING_TO_REPORT";

#[derive(Debug, thiserror::Error)]
pub enum AutomationError {
    #[error("schedule: {0}")]
    Schedule(#[from] crate::cron::CronError),
    #[error("that schedule never fires")]
    NeverFires,
    #[error("a name and an objective are required")]
    Empty,
    #[error("an automation needs at least one role")]
    NoRoles,
    #[error("no such automation")]
    NotFound,
    #[error(transparent)]
    Db(#[from] DbError),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Automation {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub schedule: String,
    pub objective: String,
    pub roles: Value,
    pub enabled: bool,
    pub next_fire_at: Option<i64>,
}

fn local(ms: i64) -> chrono::DateTime<Local> {
    Local.timestamp_millis_opt(ms).single().unwrap_or_else(Local::now)
}

/// The next fire strictly after `after_ms`, in the user's local time.
fn next_fire(schedule: &str, after_ms: i64) -> Result<Option<i64>, AutomationError> {
    Ok(Cron::parse(schedule)?.next_after(&local(after_ms)).map(|d| d.timestamp_millis()))
}

/// Create an automation.
///
/// # Errors
/// [`AutomationError`] for a bad schedule, empty fields or no roles.
pub fn create(
    db: &Db,
    workspace_id: &str,
    name: &str,
    schedule: &str,
    objective: &str,
    roles: &Value,
) -> Result<Automation, AutomationError> {
    if name.trim().is_empty() || objective.trim().is_empty() {
        return Err(AutomationError::Empty);
    }
    if !roles.as_array().is_some_and(|a| !a.is_empty()) {
        return Err(AutomationError::NoRoles);
    }
    let next = next_fire(schedule, now_ms())?.ok_or(AutomationError::NeverFires)?;
    let a = Automation {
        id: new_id("auto"),
        workspace_id: workspace_id.to_owned(),
        name: name.trim().to_owned(),
        schedule: schedule.trim().to_owned(),
        objective: objective.trim().to_owned(),
        roles: roles.clone(),
        enabled: true,
        next_fire_at: Some(next),
    };
    db.with(|c| {
        c.execute(
            "INSERT INTO automation (id,workspace_id,name,schedule,objective,roles,enabled,next_fire_at,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,1,?7,?8)",
            params![a.id, a.workspace_id, a.name, a.schedule, a.objective, a.roles.to_string(), next, now_ms()],
        )
    })?;
    Ok(a)
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Automation> {
    Ok(Automation {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        name: r.get(2)?,
        schedule: r.get(3)?,
        objective: r.get(4)?,
        roles: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(Value::Null),
        enabled: r.get::<_, i64>(6)? != 0,
        next_fire_at: r.get(7)?,
    })
}

const COLS: &str = "id,workspace_id,name,schedule,objective,roles,enabled,next_fire_at";

/// Every automation, oldest first.
///
/// # Errors
/// [`AutomationError::Db`] on a read failure.
pub fn list(db: &Db) -> Result<Vec<Automation>, AutomationError> {
    Ok(db.with(|c| {
        let mut s = c.prepare(&format!("SELECT {COLS} FROM automation ORDER BY created_at, id"))?;
        let rows = s.query_map([], row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })?)
}

/// Enable or pause. Re-enabling computes the next slot from *now*, so turning
/// an automation back on never fires for the time it was off.
///
/// # Errors
/// [`AutomationError::NotFound`] for an unknown id.
pub fn set_enabled(db: &Db, id: &str, enabled: bool) -> Result<(), AutomationError> {
    let sched: String = db
        .with(|c| c.query_row("SELECT schedule FROM automation WHERE id=?1", [id], |r| r.get(0)))
        .map_err(|_| AutomationError::NotFound)?;
    let next = if enabled { next_fire(&sched, now_ms())? } else { None };
    db.with(|c| c.execute("UPDATE automation SET enabled=?2, next_fire_at=?3 WHERE id=?1", params![id, enabled, next]))?;
    Ok(())
}

/// Make an automation due now. The scheduler's overlap and pool rules still
/// apply, so "run now" cannot stack a second run on a running one.
///
/// # Errors
/// [`AutomationError::NotFound`] for an unknown id.
pub fn run_now(db: &Db, id: &str) -> Result<(), AutomationError> {
    let n = db.with(|c| c.execute("UPDATE automation SET next_fire_at=?2 WHERE id=?1 AND enabled=1", params![id, now_ms() - 1]))?;
    if n == 0 { Err(AutomationError::NotFound) } else { Ok(()) }
}

/// Delete an automation and its history.
///
/// # Errors
/// [`AutomationError::NotFound`] for an unknown id.
pub fn remove(db: &Db, id: &str) -> Result<(), AutomationError> {
    let n = db.with(|c| c.execute("DELETE FROM automation WHERE id=?1", [id]))?;
    if n == 0 { Err(AutomationError::NotFound) } else { Ok(()) }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fire {
    pub id: String,
    pub automation_id: String,
    pub scheduled_for: i64,
    pub started_at: i64,
    pub run_id: Option<String>,
    pub outcome: String,
    pub summary: String,
    pub seen: bool,
}

/// Recent fires, newest first. `automation_id` `None` means all of them (the inbox).
///
/// # Errors
/// [`AutomationError::Db`] on a read failure.
pub fn history(db: &Db, automation_id: Option<&str>, limit: i64) -> Result<Vec<Fire>, AutomationError> {
    Ok(db.with(|c| {
        let mut s = c.prepare(
            "SELECT id,automation_id,scheduled_for,started_at,run_id,outcome,summary,seen FROM automation_fire
             WHERE ?1 IS NULL OR automation_id = ?1 ORDER BY started_at DESC, rowid DESC LIMIT ?2",
        )?;
        let rows = s.query_map(params![automation_id, limit], |r| {
            Ok(Fire {
                id: r.get(0)?,
                automation_id: r.get(1)?,
                scheduled_for: r.get(2)?,
                started_at: r.get(3)?,
                run_id: r.get(4)?,
                outcome: r.get(5)?,
                summary: r.get(6)?,
                seen: r.get::<_, i64>(7)? != 0,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })?)
}

/// Mark a finding as read.
///
/// # Errors
/// [`AutomationError::Db`] on a write failure.
pub fn mark_seen(db: &Db, fire_id: &str) -> Result<(), AutomationError> {
    db.with(|c| c.execute("UPDATE automation_fire SET seen=1 WHERE id=?1", [fire_id]))?;
    Ok(())
}

/// Findings the user has not looked at: anything that is not quiet history.
///
/// # Errors
/// [`AutomationError::Db`] on a read failure.
pub fn unseen_findings(db: &Db) -> Result<i64, AutomationError> {
    Ok(db.with(|c| {
        c.query_row(
            "SELECT count(*) FROM automation_fire WHERE seen=0 AND outcome IN ('completed','blocked','failed')",
            [],
            |r| r.get(0),
        )
    })?)
}

/// What the scheduler decided for one due automation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Fire,
    /// Record it and move on: the previous fire is still going.
    SkipOverlap,
    /// Record it and move on: the slot is too old to run late.
    Missed,
    /// Do nothing and leave the slot due: the pool is full.
    Defer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub automation: String,
    pub scheduled_for: i64,
    pub action: Action,
}

/// Decide what to do with everything due at `now`. Pure, so the sleep, overlap
/// and bound rules are tested without a clock or a database.
#[must_use]
pub fn plan<S: std::hash::BuildHasher>(
    now: i64,
    all: &[Automation],
    running: &HashSet<String, S>,
    in_flight: usize,
) -> Vec<Decision> {
    let mut free = MAX_IN_FLIGHT.saturating_sub(in_flight);
    let mut due: Vec<&Automation> = all
        .iter()
        .filter(|a| a.enabled && a.next_fire_at.is_some_and(|t| t <= now))
        .collect();
    due.sort_by_key(|a| a.next_fire_at);
    due.into_iter()
        .map(|a| {
            let scheduled_for = a.next_fire_at.unwrap_or(now);
            let late = now - scheduled_for;
            let action = if running.contains(&a.id) {
                Action::SkipOverlap
            } else if late > CATCH_UP_MS {
                Action::Missed
            } else if free == 0 {
                Action::Defer
            } else {
                // Late by more than GRACE_MS is a catch-up: still one fire.
                let _ = late > GRACE_MS;
                free -= 1;
                Action::Fire
            };
            Decision { automation: a.id.clone(), scheduled_for, action }
        })
        .collect()
}

fn running_ids(db: &Db) -> Result<HashSet<String>, AutomationError> {
    Ok(db.with(|c| {
        let mut s = c.prepare("SELECT DISTINCT automation_id FROM automation_fire WHERE outcome='started'")?;
        let rows = s.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<HashSet<_>>>()
    })?)
}

fn in_flight(db: &Db) -> Result<usize, AutomationError> {
    let n: i64 = db.with(|c| c.query_row("SELECT count(*) FROM automation_fire WHERE outcome='started'", [], |r| r.get(0)))?;
    Ok(usize::try_from(n).unwrap_or(0))
}

/// On startup: a fire left `started` belongs to a run that did not survive the
/// restart. Close it out, or the automation would skip itself forever.
///
/// # Errors
/// [`AutomationError::Db`] on a write failure.
pub fn recover(db: &Db) -> Result<usize, AutomationError> {
    Ok(db.with(|c| {
        c.execute(
            "UPDATE automation_fire SET outcome='failed', summary='interrupted: workmate was closed while this was running'
             WHERE outcome='started'",
            [],
        )
    })?)
}

fn record(db: &Db, auto: &str, scheduled: i64, outcome: &str, summary: &str) -> Result<String, AutomationError> {
    let id = new_id("fire");
    db.with(|c| {
        c.execute(
            "INSERT INTO automation_fire (id,automation_id,scheduled_for,started_at,outcome,summary)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![id, auto, scheduled, now_ms(), outcome, summary],
        )
    })?;
    Ok(id)
}

/// How the scheduler starts a run. Returns the run id.
pub type Starter<'a> = dyn Fn(&Automation, &str) -> Result<String, String> + 'a;

/// The objective an unattended run is actually given.
#[must_use]
pub fn unattended_objective(objective: &str) -> String {
    format!(
        "{objective}\n\nThis is a scheduled run and nobody is watching. Anything that needs approval \
         will be refused, so work within what you are allowed to do. If there is nothing to report, \
         answer with exactly {NOTHING} and nothing else. Otherwise answer with a short summary of what you found."
    )
}

/// Run one scheduler tick at `now`. Returns how many runs were started.
///
/// # Errors
/// [`AutomationError::Db`] if the schedule cannot be read or advanced.
pub fn tick(db: &Db, now: i64, start: &Starter<'_>) -> Result<usize, AutomationError> {
    let all = list(db)?;
    let decisions = plan(now, &all, &running_ids(db)?, in_flight(db)?);
    let mut started = 0;
    for d in decisions {
        let Some(a) = all.iter().find(|a| a.id == d.automation) else { continue };
        match d.action {
            Action::Defer => continue,
            Action::SkipOverlap => {
                record(db, &a.id, d.scheduled_for, "skipped_overlap", "the previous run was still going")?;
            }
            Action::Missed => {
                record(db, &a.id, d.scheduled_for, "missed", "workmate was not running when this was due")?;
            }
            Action::Fire => {
                let fire = record(db, &a.id, d.scheduled_for, "started", "")?;
                match start(a, &fire) {
                    Ok(run_id) => {
                        db.with(|c| c.execute("UPDATE automation_fire SET run_id=?2 WHERE id=?1", params![fire, run_id]))?;
                        started += 1;
                    }
                    Err(e) => finish(db, &fire, "failed", &e)?,
                }
            }
        }
        // Advance from now, not from the slot: a late fire must not queue up
        // every slot it slept through.
        let next = next_fire(&a.schedule, now)?;
        db.with(|c| c.execute("UPDATE automation SET next_fire_at=?2 WHERE id=?1", params![a.id, next]))?;
    }
    Ok(started)
}

/// Close a fire out. `quiet` is a completed run that found nothing.
///
/// # Errors
/// [`AutomationError::Db`] on a write failure.
pub fn finish(db: &Db, fire_id: &str, outcome: &str, summary: &str) -> Result<(), AutomationError> {
    let short: String = summary.chars().take(2000).collect();
    db.with(|c| {
        c.execute("UPDATE automation_fire SET outcome=?2, summary=?3 WHERE id=?1", params![fire_id, outcome, short])
    })?;
    Ok(())
}

/// The sidecar reports a finished run. Returns whether it was quiet, so the
/// sidecar can discard the worktree.
///
/// # Errors
/// A message for a missing argument or a write failure.
pub fn finish_op(db: &Db, args: &Value) -> Result<Vec<Value>, String> {
    let s = |k: &str| args.get(k).and_then(Value::as_str).ok_or(format!("missing `{k}`"));
    let fire = s("fireId")?;
    let (outcome, summary) = match s("outcome")? {
        "completed" => {
            let text = s("summary").unwrap_or_default();
            if text.trim() == NOTHING { ("quiet", String::new()) } else { ("completed", text.to_owned()) }
        }
        "blocked" => ("blocked", s("summary").unwrap_or_default().to_owned()),
        other => return Err(format!("unknown outcome `{other}`")),
    };
    finish(db, fire, outcome, &summary).map_err(|e| e.to_string())?;
    Ok(vec![json!({"quiet": outcome == "quiet"})])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn auto(id: &str, next: i64) -> Automation {
        Automation {
            id: id.into(),
            workspace_id: "w".into(),
            name: id.into(),
            schedule: "0 * * * *".into(),
            objective: "o".into(),
            roles: json!([{"id": "r", "name": "r"}]),
            enabled: true,
            next_fire_at: Some(next),
        }
    }

    const MIN: i64 = 60_000;
    const HOUR: i64 = 60 * MIN;

    #[test]
    fn something_due_just_now_fires() {
        let d = plan(1_000, &[auto("a", 1_000)], &HashSet::new(), 0);
        assert_eq!(d, [Decision { automation: "a".into(), scheduled_for: 1_000, action: Action::Fire }]);
    }

    #[test]
    fn something_in_the_future_or_disabled_is_left_alone() {
        let mut off = auto("b", 0);
        off.enabled = false;
        assert!(plan(1_000, &[auto("a", 2_000), off], &HashSet::new(), 0).is_empty());
    }

    #[test]
    fn a_machine_that_slept_fires_once_for_a_recent_slot_and_records_an_old_one_as_missed() {
        let now = 100 * HOUR;
        let recent = plan(now, &[auto("a", now - 3 * HOUR)], &HashSet::new(), 0);
        assert_eq!(recent[0].action, Action::Fire, "coalesced into one catch-up run");
        let old = plan(now, &[auto("a", now - 30 * HOUR)], &HashSet::new(), 0);
        assert_eq!(old[0].action, Action::Missed);
    }

    #[test]
    fn an_automation_never_overlaps_itself() {
        let running = HashSet::from(["a".to_owned()]);
        assert_eq!(plan(5, &[auto("a", 1)], &running, 1)[0].action, Action::SkipOverlap);
    }

    #[test]
    fn the_pool_is_bounded_and_the_overflow_waits_rather_than_being_lost() {
        let all = [auto("a", 1), auto("b", 2), auto("c", 3)];
        let d = plan(10, &all, &HashSet::new(), 0);
        let fired = d.iter().filter(|d| d.action == Action::Fire).count();
        assert_eq!(fired, MAX_IN_FLIGHT);
        assert_eq!(d[2].action, Action::Defer);
        assert!(plan(10, &all, &HashSet::new(), MAX_IN_FLIGHT).iter().all(|d| d.action == Action::Defer));
    }

    #[test]
    fn the_oldest_slot_gets_the_free_place() {
        let d = plan(10, &[auto("late", 5), auto("early", 1)], &HashSet::new(), MAX_IN_FLIGHT - 1);
        assert_eq!(d[0].automation, "early");
        assert_eq!(d[0].action, Action::Fire);
        assert_eq!(d[1].action, Action::Defer);
    }

    fn db_with_automation() -> (Db, Automation) {
        let db = Db::open_in_memory().unwrap();
        db.with(|c| c.execute("INSERT INTO workspace (id,name,directory,created_at) VALUES ('w','w','/w',0)", [])).unwrap();
        let a = create(&db, "w", "nightly", "0 3 * * *", "review the diff", &json!([{"id":"r","name":"r"}])).unwrap();
        (db, a)
    }

    fn make_due(db: &Db, id: &str, at: i64) {
        db.with(|c| c.execute("UPDATE automation SET next_fire_at=?2 WHERE id=?1", params![id, at])).unwrap();
    }

    #[test]
    fn a_tick_starts_one_run_per_fire_and_advances_from_now_not_from_the_slot() {
        let (db, a) = db_with_automation();
        let now = now_ms();
        make_due(&db, &a.id, now - 5 * HOUR);
        let calls = RefCell::new(vec![]);
        let started = tick(&db, now, &|auto, fire| {
            calls.borrow_mut().push((auto.id.clone(), fire.to_owned()));
            Ok("run_1".into())
        })
        .unwrap();
        assert_eq!(started, 1);
        assert_eq!(calls.borrow().len(), 1);
        let after = list(&db).unwrap();
        assert!(after[0].next_fire_at.unwrap() > now, "no queue of slept-through slots");
        let h = history(&db, Some(&a.id), 10).unwrap();
        assert_eq!((h[0].outcome.as_str(), h[0].run_id.as_deref()), ("started", Some("run_1")));
        // The same tick again must not fire it twice.
        assert_eq!(tick(&db, now, &|_, _| Ok("x".into())).unwrap(), 0);
    }

    #[test]
    fn a_run_that_cannot_start_is_a_recorded_failure_not_a_lost_fire() {
        let (db, a) = db_with_automation();
        make_due(&db, &a.id, now_ms() - 1000);
        tick(&db, now_ms(), &|_, _| Err("the runtime is not started".into())).unwrap();
        let h = history(&db, None, 10).unwrap();
        assert_eq!((h[0].outcome.as_str(), h[0].summary.as_str()), ("failed", "the runtime is not started"));
    }

    #[test]
    fn a_fire_in_progress_makes_the_next_one_a_recorded_skip() {
        let (db, a) = db_with_automation();
        make_due(&db, &a.id, now_ms() - 1000);
        tick(&db, now_ms(), &|_, _| Ok("run_1".into())).unwrap();
        make_due(&db, &a.id, now_ms() - 1000);
        assert_eq!(tick(&db, now_ms(), &|_, _| panic!("must not start")).unwrap(), 0);
        let outcomes: Vec<_> = history(&db, None, 10).unwrap().into_iter().map(|f| f.outcome).collect();
        assert!(outcomes.contains(&"skipped_overlap".to_owned()));
    }

    #[test]
    fn nothing_to_report_is_quiet_history_and_a_finding_is_unseen_until_read() {
        let (db, a) = db_with_automation();
        for (summary, want_quiet) in [("NOTHING_TO_REPORT", true), ("2 stale branches", false)] {
            make_due(&db, &a.id, now_ms() - 1000);
            tick(&db, now_ms(), &|_, _| Ok("r".into())).unwrap();
            let fire = history(&db, None, 1).unwrap().remove(0);
            let out = finish_op(&db, &json!({"fireId": fire.id, "outcome": "completed", "summary": summary})).unwrap();
            assert_eq!(out[0]["quiet"], want_quiet);
        }
        assert_eq!(unseen_findings(&db).unwrap(), 1, "only the finding, not the quiet run");
        let finding = history(&db, None, 5).unwrap().into_iter().find(|f| f.outcome == "completed").unwrap();
        mark_seen(&db, &finding.id).unwrap();
        assert_eq!(unseen_findings(&db).unwrap(), 0);
    }

    #[test]
    fn a_restart_closes_out_fires_whose_runs_did_not_survive() {
        let (db, a) = db_with_automation();
        make_due(&db, &a.id, now_ms() - 1000);
        tick(&db, now_ms(), &|_, _| Ok("r".into())).unwrap();
        assert_eq!(recover(&db).unwrap(), 1);
        assert_eq!(history(&db, None, 1).unwrap()[0].outcome, "failed");
        make_due(&db, &a.id, now_ms() - 1000);
        assert_eq!(tick(&db, now_ms(), &|_, _| Ok("again".into())).unwrap(), 1, "no longer blocked by a ghost");
    }

    #[test]
    fn creation_validates_and_pausing_stops_the_schedule() {
        let (db, a) = db_with_automation();
        assert!(matches!(create(&db, "w", "x", "bad", "o", &json!([{}])), Err(AutomationError::Schedule(_))));
        assert!(matches!(create(&db, "w", "x", "0 0 31 2 *", "o", &json!([{}])), Err(AutomationError::NeverFires)));
        assert!(matches!(create(&db, "w", "x", "0 3 * * *", "o", &json!([])), Err(AutomationError::NoRoles)));
        assert!(matches!(create(&db, "w", " ", "0 3 * * *", "o", &json!([{}])), Err(AutomationError::Empty)));
        set_enabled(&db, &a.id, false).unwrap();
        assert_eq!(list(&db).unwrap()[0].next_fire_at, None);
        set_enabled(&db, &a.id, true).unwrap();
        assert!(list(&db).unwrap()[0].next_fire_at.unwrap() > now_ms());
        assert!(matches!(set_enabled(&db, "nope", true), Err(AutomationError::NotFound)));
    }

    #[test]
    fn removing_a_workspace_takes_its_automations_and_their_history() {
        let (db, a) = db_with_automation();
        make_due(&db, &a.id, now_ms() - 1000);
        tick(&db, now_ms(), &|_, _| Ok("r".into())).unwrap();
        crate::workspace::remove(&db, "w").unwrap();
        assert!(list(&db).unwrap().is_empty());
        assert!(history(&db, None, 10).unwrap().is_empty());
    }

    #[test]
    fn the_unattended_objective_says_how_to_stay_quiet() {
        let o = unattended_objective("check deps");
        assert!(o.starts_with("check deps") && o.contains(NOTHING) && o.contains("nobody is watching"));
    }
}
