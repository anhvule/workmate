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

**This map carries execution.** Wayfinder's plan-only default is overridden for
this effort: the destination is a built app, so implementation tickets are
in-scope alongside decision tickets. Decisions still come first — do not open an
implementation ticket for a question that hasn't been decided.

**Locked decisions** (settled while charting, not re-litigated in tickets):

| | |
|---|---|
| Shape | Tauri 2 desktop, same as cowork-z — Rust shell, native installer |
| UI | React 19 + TypeScript + Tailwind + Radix/shadcn |
| Agent engine | OpenCode via Node sidecar, same as cowork-z. Differentiators are a layer **above** the engine |
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

**Skills every session should consult:** `superpowers:brainstorming` before
creative work · `superpowers:test-driven-development` · `grilling` and
`domain-modeling` for decision tickets · `prototype` for prototype tickets ·
`research` for research tickets · `token-budget` before fanning out subagents.

## Decisions so far

<!-- one line per closed ticket: gist + link. Detail lives in the ticket. -->

_none yet_

## Not yet specified

- **Provider credentials UX** — how 12+ providers are added, stored in the OS
  Keychain, and selected per workspace or per agent role.
- **Cron automations** — scheduling surface, how unattended runs are supervised,
  and where findings surface (notifications? an inbox?).
- **Starter packs and skills catalog** — what ships in the box, the install
  mechanism, and whether skills are shared with the memory layer.
- **MCP configuration surface** — how servers are registered, scoped to
  workspaces, and permission-gated.
- **Packaging and release** — signing, notarization, update channel, installer.
- **Cross-platform** — Windows and Linux builds beyond macOS.
- **First-run onboarding** — what a brand-new user sees before any workspace exists.
- **Telemetry and error reporting** — or a deliberate decision to ship without any.

## Out of scope

- **Team-shared workspaces and any sync backend.** Workmate stays single-user and
  local-first; considered as a differentiator during charting and not chosen.
- **Mobile and browser builds.** The destination is a desktop app.
