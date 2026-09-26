---
id: 018
title: MCP server configuration
type: task
mode: AFK
status: open
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
