---
id: 018
title: MCP server configuration
type: task
mode: AFK
status: closed
assignee:
blocked-by: [012, 013]
---

## Question

The MCP runtime is the engine's; the configuration surface is workmate's.

- Register servers, scope them to a workspace, and gate them by the permission
  model.
- Deliver config to the engine the way it expects, and handle the restart that a
  config change implies without dropping active sessions.
- Decide what a role-scoped MCP server means, if anything, given roles can differ
  in tool allowlist.

## Resolution

- **Storage (`mcp.rs`, migration v3).** A server is local (command + environment)
  or remote (URL), scoped to one workspace or to all (`workspace_id` NULL). A
  workspace's server shadows a global one of the same name. It cascades with its
  workspace, deliberately unlike memory: a removed folder's server list has no
  reason to outlive it.
- **Names are tool prefixes, so they are strict** (`[a-z][a-z0-9_-]{0,31}`) —
  which also means none can carry a glob character into a permission rule. The
  names workmate registers itself are reserved.
- **Only the user can add a server.** A local server is a command this machine
  will run. `mcp_add` is a Tauri command; the sidecar's only route is the
  read-only `mcp.configs`, which returns enabled servers shaped for the engine.
  Test-pinned: it cannot add, change or enable.
- **Gated by the permission surface: MCP tools always ask.** Each enabled
  server adds a `<server>_*` ask rule to the ruleset's floor, ahead of the
  widenings and well before the hard denies. Like `bash`, no stored grant can
  widen it (the table holds `read` and `edit` only). That is the safe default and
  it will be annoying with a chatty server; whether it is tolerable is exactly
  [ticket 028](028-durable-shell-allowance.md)'s question, which now covers MCP
  as well as shell.
- **Delivery without a restart.** The engine takes `POST /mcp?directory=` at
  runtime, so a config change needs no engine restart and drops no session:
  servers are registered for the run's worktree before each turn.
  Registering again replaces (assumption, confirm live).
- **Role-scoped means allowlisted.** A role's allowlist names servers (`github`
  or `github_*`). The engine enables any tool not mentioned, so an allowlist now
  spells exclusivity out: every built-in and every configured server's tools are
  set explicitly, listed ones on, the rest off. This also fixes a gap in
  [014](014-run-orchestration.md), whose allowlist only switched tools on.
  Memory tools are the engine's `workmate-memory_*` names and always on.
- **Not covered:** secret-bearing headers or environment values. They would need
  keychain accounts, and a plaintext token in SQLite is worse than no feature.
  Remote servers that use OAuth are the engine's to authenticate.
