---
id: 010
title: Who owns the conversation
type: grilling
mode: HITL
status: closed
assignee: agent (autonomous mode)
blocked-by: [003]
---

## Question

Cowork-z persists every transcript **twice**: OpenCode independently owns the real
session, while cowork-z keeps a render-ready copy in its own tables, re-persisted
from the frontend. With one agent this is merely redundant. With N collaborating
agents it becomes a consistency problem — whose ordering wins, what a sort order
means across concurrent sessions, and what happens when the webview is throttled.
Cowork-z already hit the last of these and had to move completion handling into
Rust because WKWebView throttles backgrounded listeners.

Decide:

- **Single source of truth.** Is OpenCode's own session store authoritative with
  workmate holding only an index, does workmate own the transcript outright, or is
  there a defensible reason to keep both?
- **Who writes.** Confirm the writer is Rust and never the webview — cowork-z's
  throttling scar argues this is not optional — and say what that means for
  optimistic UI updates.
- **Ordering across a team.** With several roles producing turns concurrently,
  what establishes the canonical order the user reads: wall-clock, a logical clock,
  or the handoff graph itself? This is the same object
  [What the agent team looks and feels like](005-agent-team-ux.md) has to render,
  so the two should agree.
- **Retention and cost.** Transcripts are the raw material the memory layer
  extracts from (see [How workmate remembers](006-persistent-memory.md)). State
  what is kept, for how long, and whether deleting a workspace deletes the
  transcripts a memory was derived from.
- **Reconciliation.** What happens when workmate's view and OpenCode's store
  disagree — after a crash, an engine upgrade, or a session resumed from a
  different app version.

Surfaced by [Cowork-z architecture up close](002-cowork-z-architecture.md) §4.

## Resolution

> **Agent-made decision.** Taken autonomously at the user's instruction.

**OpenCode's session store is authoritative for transcripts. Workmate stores an
index, never a copy.** The index holds session id, run, role, directory and engine
version, plus derived per-run summaries. Cowork-z's double-persist is merely
redundant with one agent and becomes an ordering problem with a team.

**Rust is the only writer.** The webview never persists. Cowork-z had to move
completion handling into Rust because WKWebView throttles backgrounded listeners —
that scar is evidence, not preference. Optimistic UI updates are local component
state, discarded when the authoritative event arrives.

**Ordering across a run is the handoff graph first, wall-clock second.** Within a
session, wall-clock. This is deliberately the same object
[What the agent team looks and feels like](005-agent-team-ux.md) renders, so the
stored order and the read order cannot drift.

**Retention.** Transcripts live as long as OpenCode keeps them. Workmate's index
and its memories survive independently — deleting a workspace deletes index rows
and never memories, per [Workmate's domain model](003-domain-model.md).

**Reconciliation.** On resume, workmate compares the recorded engine version and
checks the session still exists. A missing session marks its run archived and
read-only with the index retained, rather than deleting the user's history or
pretending the session is live.
