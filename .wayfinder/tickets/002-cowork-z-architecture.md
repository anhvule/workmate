---
id: 002
title: Cowork-z architecture up close
type: research
mode: AFK
status: open
assignee:
blocked-by: []
---

## Question

Workmate is a similar app to cowork-z, not a fork — but cowork-z has already
solved the unglamorous problems workmate will hit on day one. Read the repo and
report how it actually works, so later tickets decide rather than re-derive.

Cover:

1. **Repo and build layout.** Workspace structure, pnpm/cargo setup, Vite config,
   how the Tauri app and the Node sidecar are built and bundled together.
2. **Tauri command surface.** What crosses the Rust↔web boundary: the commands,
   events and their payload shapes. What lives in Rust versus TypeScript, and why.
3. **Sidecar supervision.** How the OpenCode sidecar is located, spawned,
   health-checked, restarted and shut down. How streaming output reaches the UI.
4. **Persistence.** The SQLite schema — tables, keys, migration approach, which
   crate/driver, and what is stored in SQLite versus on disk versus in memory.
5. **Workspace on-disk layout.** What a workspace folder contains and how sessions,
   history and per-workspace config are laid out.
6. **Permission model.** How folder-level grants are represented and enforced, and
   how a runtime permission prompt flows from a tool call to the UI and back.
7. **Credentials.** Which OS Keychain integration is used, what is stored, and how
   provider credentials are scoped.
8. **MCP, automations, starter packs, skills catalog.** For each: roughly how much
   is inherited from OpenCode versus built in the app, and where it lives.
9. **Licence and reuse.** What licence is cowork-z under, and what does that permit
   workmate to reuse, adapt, or take inspiration from? State the attribution
   obligations concretely. Flag anything we must **not** copy.

Prefer file paths and concrete shapes over prose. Note deliberately what looks
like a design workmate should *diverge* from, given the three differentiators.
