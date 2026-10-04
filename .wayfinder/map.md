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
| Agent engine | OpenCode, **bundled** as a binary and supervised by Rust. Orchestration runs in a **bundled Node sidecar**; Rust is the only writer of SQLite. Revised while building — see [Where orchestration runs](tickets/024-where-orchestration-runs.md) |
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

- [Where orchestration runs](tickets/024-where-orchestration-runs.md) — revises a
  locked decision: a bundled Node sidecar owns orchestration and the engine
  connection, Rust stays the sole writer of SQLite behind a narrow IPC, the
  webview persists nothing. Keeps the TypeScript domain logic as the single
  implementation of the differentiators.

- [Credentials in the OS Keychain](tickets/017-credentials-keychain.md) —
  `v1:<scope>:<provider>` accounts resolved role → workspace → global in Rust,
  which owns the keychain outright; a miss pauses the run and reports what it
  tried. The TypeScript credential module was deleted rather than left to drift.
- [Bundle and supervise the Node sidecar](tickets/025-bundle-the-sidecar.md) —
  orchestration compiled to a binary with Bun (pkg rejected: it builds V8 from
  source). Rust starts engine-then-sidecar and stops sidecar-then-engine, with
  readiness by handshake at both hops.

- [Workspaces and the permission surface](tickets/013-workspaces-and-permissions.md)
  — a workspace is an id bound to a canonicalised directory, so a moved folder is
  repaired with one `UPDATE` and a missing one is a reported state, not an error.
  Removing a workspace has to detach memory by hand: `memory_scope.scope_id` is
  polymorphic, carries no foreign key, and the association was outliving the
  workspace. Permissions compile into `OpenCode`'s ordered ruleset where position
  is the policy — the user's checkout readable but never writable, the worktree
  writable, `.git/` and `git push` unreachable by any grant. Bash allowances are
  not storable at all, so "push always asks" holds by construction.
- [Git worktrees and the repo surface](tickets/016-git-worktrees.md) — a run gets
  `workmate/run-<id>` in a worktree under workmate's data dir, addressed by ids
  rather than paths. Merge is explicit and computed in memory first, so a conflict
  changes nothing; archive keeps the branch, abandon deletes it, and neither
  discards uncommitted work without `force`.
- [The persistence IPC](tickets/026-persistence-ipc.md) — the sidecar names
  operations (`run.create`, `handoff.append`, …) and Rust runs the SQL on a
  single in-order pump thread. Calls time out rather than hang, and a reply with
  no waiter stays a fault.
- [Escape paths that look like globs](tickets/027-escape-globs-in-patterns.md) —
  the engine's matcher has no escape for `*` or `?`, so they are spelled `?` (one
  character, not unbounded) and an explicit grant for such a path is refused.
  Brackets and braces were already literal.
- [The engine event stream](tickets/023-event-stream.md) — Rust subscribes to
  `/global/event`, scrubs credentials, tags permission and completion events for
  Rust to act on, and emits `sidecar:<type>` with a gap-detecting `seq`/`epoch`.
  After a reconnect the engine's transcript, not the stream, is the truth.
- [Memory store, remember and recall](tickets/015-memory-store.md) — Rust stores
  and guards (no secrets, bounded claims, supersede same-subject-same-scope),
  TypeScript builds the bounded digest and serves `remember`/`recall` over a
  token-guarded loopback MCP endpoint that carries run/role identity in its path.
  Survival across workspace deletion is tested. The panel UI waits on the shell.
- [Run and team orchestration](tickets/014-run-orchestration.md) — one loop for
  every run, so a solo run is a plain chat by construction. Roles share the run's
  worktree (teams are sequential). Pause bites at the next handoff, where the user
  can amend, redirect or veto; missing keys and engine errors block rather than
  fail. Credentials are provisioned to the engine by Rust and never cross the
  pipe. Proven against fakes; the first live model turn will confirm two engine
  assumptions.
- [MCP server configuration](tickets/018-mcp-servers.md) — servers are scoped to a
  workspace or global, only the user can add one, tools always ask, and delivery
  is `POST /mcp` per run so no restart is needed. Role scoping means the
  allowlist is genuinely exclusive now. Secret-bearing headers are not stored.
- [Cron automations](tickets/019-automations.md) — a Rust scheduler over a
  hand-written cron; one bounded run per fire, never overlapping itself; a slept
  machine fires once or records a miss. Unattended runs get a ruleset where every
  ask is a deny, answer `NOTHING_TO_REPORT` to stay quiet, and discard their
  worktree when they do. Findings are kept and unseen until read.
- [Starter packs and the skills catalog](tickets/020-starter-packs-and-skills.md)
  — a pack ships a *team* (solo, plan-build-review, docs-pair), previewable and
  never overwriting. Skills install where the engine looks, are SHA-256 checked
  so edits are never silently replaced, and sync over a hardened `git` shell-out
  from sources the user adds. Teams got real CRUD along the way.
- [First run](tickets/022-first-run.md) — asks for a folder and nothing else; solo
  is the default with the team one click away; the key, model and git are asked
  for in place at the first message, when the reason is obvious.
- [The application shell](tickets/029-application-shell.md) — the screens were a
  missing ticket. Five tabs, no webview persistence, a solo run that is a chat by
  construction, an editable handoff, in-place recovery from blocks. Building it
  found that the engine was reading the user's own OpenCode directories (now
  isolated; see ticket 012) and that light mode never applied (fixed).
- [Packaging, signing and release](tickets/021-packaging-and-release.md) — the
  built app was launched and runs the engine and sidecar from its bundle; a 74 MB
  dmg, so no first-run download; no updater in v1. Packaging found two real bugs
  (globs flattening the packs; the engine outliving a Cmd-Q) now fixed and tested.
  Signing, notarization and CI are written but unexecuted until the first tag.
- [Telemetry and error reporting](tickets/030-telemetry-and-diagnostics.md) —
  none, deliberately: nothing about a user's work leaves the machine. Local
  rotating logs instead, which fixed a real hang: the engine's pipes were not
  being drained after startup.

## Not yet specified

- **Cross-platform** — Windows and Linux. The binary matrix is decided and the
  engine is already isolated through XDG variables, which Linux honours as-is;
  what remains is CI runners, the Windows keychain backend in practice, path
  handling, and an opener for "reveal". Nothing has been run on either.

<!-- Graduated since charting: provider credentials UX (built in the shell, 029,
     and first run, 022 — keys asked for in place, write-only, keychain-backed)
     and telemetry (030: none, with local logs). -->

<!-- Cron automations, starter packs, MCP config, packaging and first-run all
     graduated into tickets 018-022 once the domain model fixed their vocabulary. -->

## Out of scope

- **Team-shared workspaces and any sync backend.** Workmate stays single-user and
  local-first; considered as a differentiator during charting and not chosen.
- **Mobile and browser builds.** The destination is a desktop app.
- **Pull requests and CI status.** v1 stops at local git; GitHub as a hosting
  integration is past the destination. Ruled out while resolving
  [What "repo-native" actually means](tickets/007-repo-native-surface.md).
