---
id: 016
title: Git worktrees and the repo surface
type: task
mode: AFK
status: open
assignee:
blocked-by: [011, 013]
---

## Question

Implement the repo-native surface decided in
[What "repo-native" actually means](007-repo-native-surface.md).

- `git2` in-process for status and diff; shelling out only for credentialled clone.
- Creating a run makes a worktree on `workmate/run-<id>`; the run works only there.
- Workmate owns branch and worktree creation; agents never create them.
- `.git/` is never writable. Push always asks.
- Per-run diff view, worktree path, open-in-editor, and an explicit merge action.
- Cleanup: what happens to a worktree when a run is archived or abandoned.
