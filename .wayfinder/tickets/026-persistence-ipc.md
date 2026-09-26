---
id: 026
title: The persistence IPC
type: task
mode: AFK
status: open
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
