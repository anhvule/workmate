---
id: 026
title: The persistence IPC
type: task
mode: AFK
status: closed
assignee:
blocked-by: [025]
---

## Question

The sidecar orchestrates but must not write SQLite — Rust is the only writer
(tickets 010 and 024). The protocol already carries `db.query`, `db.result` and
`db.error`; nothing is wired behind them.

- Route a `db.query` from the sidecar to the Rust connection and return rows by
  request id; unmatched ids are already a fault and must stay one.
- Decide the query surface: raw SQL over the pipe is simple but makes the
  sidecar a second author of the schema. Prefer named operations — `createRun`,
  `appendHandoff`, `recallMemories` — so the schema stays Rust's.
- Back-pressure and timeouts: a sidecar awaiting a reply that never comes must
  fail its run rather than hang it.
- Ordering: replies are correlated by id, so out-of-order delivery is fine, but
  writes within a run must not reorder.
- Test the round trip against a real in-memory database, with both processes.

## Resolution

Named operations, not SQL. The protocol's `db.query{sql,params}` became
`db.call{id,op,args}`; the statements live in `ops.rs` (`role.upsert`,
`run.create`, `run.setState`, `session.record`, `handoff.append`, `run.load`),
and later tickets add operations there rather than letting the sidecar author
SQL. An unknown op or bad argument is a `db.error`, never a panic.

- **Rust side.** After `ready`, `Sidecar` keeps a single pump thread reading the
  sidecar's stdout and answering each `db.call` on its stdin. One thread means
  calls are served in send order, so writes within a run cannot reorder; a slow
  operation delays later ones, the stated price. Failures answer too, so nothing
  waits on a silent error. The handler is injected, which is what lets the tests
  run without Tauri.
- **TypeScript side.** `DbClient` correlates replies by id (out-of-order is fine),
  and fails a call after 10s instead of hanging the run. A reply with no waiter
  — including one that lands after the timeout — remains a `fault`, because it
  means Rust committed a write the run believes failed.
- **Round trip.** Tested over real pipes with a stand-in sidecar script, against
  the dispatcher on an in-memory database. The compiled sidecar has no caller of
  `db.call` until run orchestration exists, so the real-binary round trip is
  exercised there.
