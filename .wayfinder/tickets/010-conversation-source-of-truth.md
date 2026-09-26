---
id: 010
title: Who owns the conversation
type: grilling
mode: HITL
status: open
assignee:
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
