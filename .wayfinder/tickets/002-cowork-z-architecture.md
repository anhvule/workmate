---
id: 002
title: Cowork-z architecture up close
type: research
mode: AFK
status: closed
assignee: jack.le@zuhlke.com
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

## Resolution

Full findings, with a file-path citation per claim, the real SQLite DDL, the Tauri
command surface and the permission-prompt flow traced end to end:
[.wayfinder/research/cowork-z-architecture.md](../research/cowork-z-architecture.md).

**Licence — clean.** MIT, "Copyright (c) 2025-present Kevin Lin and contributors",
no CLA, no NOTICE file. Verbatim reuse in a differently-licensed workmate is
permitted; the single obligation is shipping the copyright and permission notice
with any substantial copied or adapted portion, **in the repo and in the app
bundle**. Must not copy: vendor logo SVGs (trademarks), DM Sans (OFL), app icons,
screenshots, bundle identifier, keychain service name, updater public key, the
starter-pack corpora (unclear provenance), and `opencode-api.json` (OpenCode's own
artefact).

**Shape of the thing.** Single pnpm package; the sidecar is a separate project
compiled to a single-file node20 binary named by Rust target triple and consumed
via Tauri `externalBin`, with a 5-way CI matrix because it cannot be
cross-compiled. 118 Tauri commands in one handler, all returning
`Result<T, String>`; anything the sidecar owns returns `()` and answers by event.
rusqlite bundled, one `Mutex<Connection>`, WAL, a hand-rolled linear migration
ladder to v8 with each step in its own transaction.

**Where copying cowork-z would obstruct workmate** — eight divergences are set out
in the findings' *Divergence notes*; the four that bear on the differentiators:

- *Agent team has nowhere to live.* The session manager deletes every other
  session when a new task starts, and automation concurrency is capped at one by a
  global `AtomicBool`. Arena is the sole exception and is three isolated clones
  racing the same prompt with no channel between them — its UI is worth studying,
  its execution model is not. There is no agent-role abstraction anywhere: no
  `agents` table, one persona baked into a ~100-line prompt string, because
  OpenCode ignored custom agent names at the version they integrated. Roles-as-data
  is new construction.
- *Memory has no home.* Every table cascades off `workspace_id`; there is no table,
  index or code path where knowledge outlives a workspace. Workmate needs a
  workspace-independent store from migration v1, referenced by workspaces rather
  than owned by them, or cascade semantics will delete the differentiator with a
  removed folder.
- *The four-folder convention is repo-hostile.* `Input/Output/Misc/Artefacts` is
  enforced by prompting the agent to `mkdir -p`, and the code concedes bash is not
  gated by the edit rules — advisory governance over a user's source tree. Derive
  the write surface from git instead.
- *Git is only a skill-repo fetcher.* `git_ops.rs` shells out to clone and pull
  skill repositories into a cache and never touches the workspace. Nothing models a
  branch, commit, diff or PR. Repo-native is greenfield, and shelling out per query
  is the wrong starting point for status/diff on every keystroke.

**One finding with a deadline.** Keychain entries are keyed by provider id alone,
globally, with one active model for the whole app. Widening to `(provider, scope)`
after shipping means migrating across the OS keychain, which is materially worse
than a SQLite migration — so it must be settled before any credential code ships.
Ticketed as [How credentials are scoped](009-credential-scoping.md).

**Worth adopting outright.** Each of these is a scar from a dated review finding,
and re-deriving them would cost weeks: the api-key fingerprint bridge that keeps
real keys out of the webview; readiness by the sidecar's own `ready` event rather
than process spawn; shutdown on `ExitRequested` not `Exit`, so `opencode serve` is
not orphaned; per-migration transactions with a rollback test; random port plus
per-launch password and HTTP basic auth on the engine; an empty static
`assetProtocol.scope` populated at runtime from granted folders; and the verbatim
`sidecar:{type}` event passthrough with a typed union shared across the Rust
boundary.

**Sourcing caveat.** `docs/architecture/architecture.md` in cowork-z is stale in
places — it still documents task-scoped `folder_permissions`, dropped in migration
v6. The findings cite code over that doc throughout.
