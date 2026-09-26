---
id: 013
title: Workspaces and the permission surface
type: task
mode: AFK
status: open
assignee: jale
blocked-by: [011]
---

## Question

Create, open, list and remove workspaces, and enforce what an agent may touch.

- A workspace binds to one directory by stable id, not by path, so a moved folder
  is repaired rather than orphaned.
- **No `Input/Output/Misc/Artefacts` convention.** Dropped deliberately — see
  [What "repo-native" actually means](007-repo-native-surface.md).
- Grants in the permission table with a source of workspace, user or adhoc; the
  workspace root is synthetic and never stored.
- The runtime prompt flow: a tool call raises a permission event, the UI asks,
  the reply persists an adhoc grant and answers the engine.
- `assetProtocol.scope` stays empty in config and is populated at runtime from
  granted folders only.
- Compile grants into the engine's permission ruleset, and remember bash is not
  covered by edit rules — gate it with `permission.bash` globs instead.
