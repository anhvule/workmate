---
id: 029
title: The application shell
type: task
mode: AFK
status: open
assignee: claude
blocked-by: [014, 015, 018, 019, 020]
---

## Question

Every backend ticket delivered commands and events; the webview is still the
scaffold's one-line status page. Build the screens, and in doing so settle
[First run](022-first-run.md), which cannot be decided apart from them.

This ticket was missing from the plan. The map listed the *capabilities* (runs,
memory, MCP, automations, packs) but nothing that put a window in front of them;
the memory panel (015) and the run views (005) were each deferred "to the shell",
and the shell had no ticket.

Scope:

- Workspaces: add (folder picker), switch, repair a moved folder, remove.
- Runs: start from a team, the attributed thread, handoff cards, the per-run
  header strip (team runs only), pause / amend / redirect / veto, blocked states,
  permission prompts naming the role, the diff with merge / archive / abandon.
- Memory panel: read, correct, delete, export.
- Automations: create, pause, run now, history, the findings inbox.
- Library: starter packs (with preview) and skills.
- Settings: provider keys, MCP servers.
- The rules that carry weight: the webview never persists (optimistic UI is local
  component state, discarded when the authoritative event arrives), and a
  one-role run must read as a plain chat.
