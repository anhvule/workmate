---
id: 011
title: Persistence layer and migration v1
type: task
mode: AFK
status: closed
assignee: agent (autonomous mode)
blocked-by: [004]
---

## Question

Stand up SQLite in the Rust shell with the schema the domain model fixed.

- `rusqlite` bundled, single connection behind a mutex, WAL and foreign keys on —
  adopted from cowork-z, which has the scars to justify each.
- A linear migration ladder with **each step in its own transaction** and a
  rollback test, version in a `schema_meta` row.
- Tables for workspace, role, team, run, session index, handoff, memory,
  memory_scope, permission_grant, provider, automation.
- **The load-bearing constraint:** `memory` has no `workspace_id` and no cascade.
  Association is through `memory_scope`; deleting a workspace deletes scope rows
  only. Write a migration test that deletes a workspace and asserts the memory
  survives — this is the differentiator's guardrail, per
  [Workmate's domain model](003-domain-model.md).
- Session rows carry the engine version, per
  [Pin the OpenCode contract and how it ships](008-opencode-contract-and-distribution.md).
- Rust is the only writer. No persistence command may be reachable from the
  webview except through a Tauri command.

## Resolution

Schema v1 is in, with 10 Rust tests green.

`apps/desktop/src-tauri/src/migrations.rs` holds the ladder — linear,
append-only, one transaction per step, version in `schema_meta`, and the step
name logged as it applies because the first question on a failed upgrade is
always "which one?". `db.rs` owns a single connection behind a mutex with WAL,
`foreign_keys = ON` and a busy timeout, opened from the app data directory in
Tauri's `setup`.

**The two guardrail tests are the point of this ticket:**

- `deleting_a_workspace_detaches_memory_instead_of_deleting_it` — `memory` has
  no `workspace_id` and no cascade; association runs through `memory_scope`, so
  removing a workspace deletes scope rows and nothing else.
- `a_failing_step_rolls_back_and_leaves_the_version_untouched` — asserts both
  that the version is unchanged and that the partial step left no table behind.

Also covered: a run owns its sessions and takes them with it; `run.state` is
constrained to the lifecycle; foreign keys are genuinely enforced, so the
cascade rules are load-bearing rather than decorative; and a file database
survives reopening.

Rust is the only writer — the webview reaches persistence solely through Tauri
commands.
