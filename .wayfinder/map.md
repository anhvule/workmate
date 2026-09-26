---
label: wayfinder:map
---

# Map — Build workmate

## Destination

A working, installable **workmate**: a Tauri 2 local-first AI workspace at cowork-z
parity — workspace-per-project with folder-level permissions and OS Keychain
credentials, MCP servers, cron automations, starter packs and a skills catalog —
differentiated by a **collaborating agent team**, **persistent cross-workspace
memory**, and a **repo-native git workflow**. Built test-first and committed to
this git repo.

Product thesis in one line: *an AI dev team that remembers your projects and lives
in your repo.*

## Notes

**Domain.** Local-first desktop AI workspace tooling. Prior art and reference
point: [kevinlin/cowork-z](https://github.com/kevinlin/cowork-z) — workmate is a
similar app, not a fork.

**Autonomous mode.** The user instructed the agent to decide and build without
being asked. Tickets originally typed HITL were resolved by the agent; each such
resolution is headed *Agent-made decision* and is cheap to overturn. Treat them as
defaults with rationale, not as ratified product decisions.

**This map carries execution.** Wayfinder's plan-only default is overridden for
this effort: the destination is a built app, so implementation tickets are
in-scope alongside decision tickets. Decisions still come first — do not open an
implementation ticket for a question that hasn't been decided.

**Locked decisions** (settled while charting, not re-litigated in tickets):

| | |
|---|---|
| Shape | Tauri 2 desktop, same as cowork-z — Rust shell, native installer |
| UI | React 19 + TypeScript + Tailwind + Radix/shadcn |
| Agent engine | OpenCode via Node sidecar, same as cowork-z. Differentiators are a layer **above** the engine — confirmed viable, nothing blocked |
| Differentiators | collaborating agent team · persistent cross-workspace memory · repo-native git workflow |
| v1 parity | workspaces + permissions + credentials · MCP · cron automations · starter packs + skills catalog |
| Git | local repo, no remote yet; conventional commits |

**Quality bar — every implementation ticket.** Test-first (`superpowers:test-driven-development`)
for the domain layer, engine adapter, memory store and git integration. UI covered
by a thin smoke/E2E pass, not exhaustive component tests. Typecheck, lint and tests
green before every commit.

**Token budget is a standing constraint.** Quality of code is not the thing to
trade away — breadth of exploration is. Prefer one targeted read over a survey;
prefer resolving a ticket to re-deriving context another ticket already holds.

**Prior-art findings live in `.wayfinder/research/`** — two long, citation-dense
documents on OpenCode's integration surface and cowork-z's internals. Do not
re-derive what they already answer; zoom into them from the resolved tickets.

**Skills every session should consult:** `superpowers:brainstorming` before
creative work · `superpowers:test-driven-development` · `grilling` and
`domain-modeling` for decision tickets · `prototype` for prototype tickets ·
`research` for research tickets · `token-budget` before fanning out subagents.

## Decisions so far

<!-- one line per closed ticket: gist + link. Detail lives in the ticket. -->

- [Can OpenCode drive a collaborating agent team?](tickets/001-opencode-agent-team.md)
  — nothing is blocked; all three differentiators are sidecar layers over native
  primitives. Named agents with own prompt/model/tools and concurrent sessions are
  native; handoff carries only one string, so the collaboration itself is
  workmate's. Per-turn `system` injection is the memory hook, and it is auditable.
  Git is read-level only, but `permission.bash` glob rules make it gateable.
  OpenCode is MIT with prebuilt binaries — bundling is possible.
- [Cowork-z architecture up close](tickets/002-cowork-z-architecture.md) — MIT,
  reusable with notice in repo and bundle; eight named divergences where copying it
  would obstruct workmate, chiefly one-session-at-a-time, no agent-role entity,
  everything cascading off `workspace_id`, and prompt-enforced folder governance.
  Seven hard-won details worth adopting outright.

- [Pin the OpenCode contract and how it ships](tickets/008-opencode-contract-and-distribution.md)
  — bundle `opencode-ai@1.18.32` as a Tauri sidecar from per-platform prebuilt
  binaries, so workmate works on first launch where cowork-z needs a global
  install. Engine moves only when workmate ships; generated client types committed
  and diffed in CI.
- [Workmate's domain model](tickets/003-domain-model.md) — Run owns its Sessions
  (killing the stale-session cull), Role is first-class data, Team replaces Arena,
  and Memory is associated with workspaces rather than owned by them so nothing
  cascades into it.
- [What the agent team looks and feels like](tickets/005-agent-team-ux.md) — one
  attributed thread with handoff as an expandable card, not lanes; a one-role team
  must read as a plain chat.
- [How workmate remembers](tickets/006-persistent-memory.md) — explicit `remember`
  tool, no silent extraction; bounded pinned digest injected per turn plus a
  `recall` tool; SQLite canonical, markdown export read-only.
- [What "repo-native" actually means](tickets/007-repo-native-surface.md) — a run
  works in its own git worktree on `workmate/run-<id>`, never the user's checkout;
  permission surface derived from git, the four-folder convention dropped; `git2`
  in-process.
- [How credentials are scoped](tickets/009-credential-scoping.md) — keychain
  account `v1:<scope>:<provider>`, credential separate from model choice,
  role → workspace → global resolution, missing key pauses the run.
- [Who owns the conversation](tickets/010-conversation-source-of-truth.md) —
  OpenCode owns transcripts, workmate stores only an index, Rust is the sole
  writer, ordering follows the handoff graph.

- [Scaffold the workmate repo](tickets/004-scaffold-the-repo.md) — pnpm workspace,
  Tauri 2 + React 19 + Tailwind 4, one `pnpm gate` behind a pre-commit hook, the
  pinned engine fetched into the bundle, MIT licence and NOTICE. Builds, launches,
  31 tests green.

- [Persistence layer and migration v1](tickets/011-persistence-layer.md) —
  SQLite schema v1 with WAL, enforced foreign keys and a transactional migration
  ladder. Memory has no `workspace_id`: deleting a workspace detaches it, proved
  by test.

- [OpenCode engine client and sidecar supervision](tickets/012-engine-client.md) —
  bundled engine spawned with a per-launch password, readiness by announced URL,
  restarts deferred while sessions live; generated client types committed and
  drift checked in the gate. Confirmed `POST /session` takes a `permission` field,
  softening the mid-migration risk.

## Not yet specified

- **Provider credentials UX** — how 12+ providers are added and presented, and
  what a user does when a model they picked has no key. The *scoping* half of this
  has graduated to [How credentials are scoped](tickets/009-credential-scoping.md).
- **Telemetry and error reporting** — or a deliberate decision to ship without any.
- **Cross-platform** — Windows and Linux. The binary matrix is decided, so what
  remains is CI runners and platform-specific keychain and path handling.

<!-- Cron automations, starter packs, MCP config, packaging and first-run all
     graduated into tickets 018-022 once the domain model fixed their vocabulary. -->

## Out of scope

- **Team-shared workspaces and any sync backend.** Workmate stays single-user and
  local-first; considered as a differentiator during charting and not chosen.
- **Mobile and browser builds.** The destination is a desktop app.
- **Pull requests and CI status.** v1 stops at local git; GitHub as a hosting
  integration is past the destination. Ruled out while resolving
  [What "repo-native" actually means](tickets/007-repo-native-surface.md).
