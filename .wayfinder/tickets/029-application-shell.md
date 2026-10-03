---
id: 029
title: The application shell
type: task
mode: AFK
status: closed
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

## Resolution

React 19 + Tailwind 4 + Radix (dialog, tabs), no state library. Five tabs per
project — Runs, Memory, Automations, Library, Settings — a first-run welcome, and
a global permission modal.

**The rules that carry weight, in the code:**

- *The webview persists nothing.* Everything durable is read from Rust through a
  `useLoad` hook that drops stale answers. The only state held here is in-flight:
  streaming text, a handoff awaiting a decision, a block (`lib/live.ts`), each
  discarded when the authoritative event arrives. Which project is open is
  component state; after a restart it is the first one.
- *One-role run = plain chat.* Whether a run is solo comes from its **team**, not
  its session count (counting sessions called a team run "solo" until the second
  role started, hiding the strip — found in the browser and fixed). A solo run has
  no role strip, no role chips and no handoff cards, and takes follow-ups; a smoke
  test asserts all three.
- *The handoff is visible and editable.* Between roles, a collapsed card ("see what
  was passed"); paused, a card with the exact text the next role will be told,
  editable, with Continue / Redirect / End run. Order comes from `runTimeline`; the
  UI never re-sorts.
- *Blocked is recoverable in place.* A missing key shows a key field inside the run
  and resumes it on save; an engine error shows its message and Retry.
- *Merging is deliberate.* Changes panel → Merge… → a confirmation that says it
  refuses on a dirty checkout or a conflict and changes nothing. Abandon names what
  it deletes and asks about uncommitted work only if that is what stopped it.
- *No secret ever returns to the webview.* Keys are write-only; there is no reveal.

**Also added to the backend because the screens exposed gaps:** run listing,
follow-up messages (`run.say`) and reviving a run after a restart, the credential
and default-model commands, `session opened` events (so a permission prompt can
name the role asking), a diff against the derived merge-base, and the dialog
plugin plus a capability file (without one the folder picker and event listeners
would have been denied at runtime).

**Fixed on the way:** light mode never applied. Tailwind hoists a `@theme` placed
inside `@media`, so the dark values were unconditional; tokens now live on `:root`
with a real media query.

**Verified how:** a jsdom smoke pass (5 tests: first run, a team thread with
handoffs, a one-role chat, pausing and amending, forgetting a memory), and the
screens walked by hand in both colour schemes against the mock bridge. A mock
backend (`lib/mock.ts`) stands in when there is no Tauri and speaks the same
command and event names. **Not done: the native Tauri window was not launched
here**, so window chrome, the real folder picker and real events were exercised
only through the unit and Rust integration layers.

**Limits, stated:** permission prompts offer *allow once* or *deny* (durable folder
access is granted in Settings, where it can be seen and revoked); a revived run can
be continued from its last session but not resumed mid-handoff; no OS notifications
(`automation.finding` and the unseen count are there to hang one on); no keyboard
shortcut layer; the model list comes from the engine when it is up and is typed
otherwise.
