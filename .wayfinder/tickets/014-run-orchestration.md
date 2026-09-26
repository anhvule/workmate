---
id: 014
title: Run and team orchestration
type: task
mode: AFK
status: open
assignee:
blocked-by: [011, 012, 016, 017]
---

## Question

The differentiator itself: run a team of roles on one task, with real handoffs.

- A run owns its sessions. **Nothing may cull a session because another run
  started** — the cowork-z behaviour this design exists to avoid.
- Concurrency is a bounded pool keyed on run, not a global boolean.
- Each role gets its own session, its own worktree directory, its own system
  prompt, model and tool allowlist, resolved from the role record.
- Handoffs are composed by `composeHandoff` in `@workmate/sidecar`, persisted,
  and rendered as cards.
- Pause, amend a pending handoff, redirect to a different role, and veto — per
  [What the agent team looks and feels like](005-agent-team-ux.md).
- A one-role run must take the same code path and render as a plain chat.
- Run timeline ordering comes from `runTimeline`; the UI must not re-sort.
