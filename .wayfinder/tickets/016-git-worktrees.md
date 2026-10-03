---
id: 016
title: Git worktrees and the repo surface
type: task
mode: AFK
status: closed
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

## Resolution

Built as `apps/desktop/src-tauri/src/repo.rs` on `git2` 0.21 (no default
features: no network, no ssh — clone stays a shell-out for a later ticket),
fifteen tests written alongside, exposed through six Tauri commands.

- **Where worktrees live.** `<app data>/worktrees/<workspace id>/<run id>`,
  derived and never stored, outside the user's tree. Commands take workspace and
  run ids, never a path, so the webview cannot point git at an arbitrary folder.
- **Branch.** `workmate/run-<id>`, with the id's `run_` prefix dropped. Created
  from the checkout's current commit; the commit is returned as `base`, and the
  per-run diff is taken against it, so it covers committed and uncommitted work.
  A failed worktree add deletes the branch it just made so a retry is not blocked.
- **Merge** is explicit and conservative: fast-forward when possible, otherwise a
  merge commit computed *in memory first*, so a conflict changes nothing. It
  refuses on a dirty tracked checkout or a detached HEAD. Workmate never resolves
  a conflict for the user.
- **Cleanup.** Archive removes the directory and keeps the branch, so the work
  stays mergeable. Abandon also deletes the branch. Either refuses a worktree
  with uncommitted changes unless `force` — the one thing deleting a directory
  loses for good.
- **Not done here, deliberately.** Open-in-editor is the UI's job once the path
  is shown. Run rows and `run.branch` are written by
  [Run and team orchestration](014-run-orchestration.md), which now composes
  these calls. `.git/` write-protection and push-asks already live in the
  permission ruleset ([013](013-workspaces-and-permissions.md)); this module only
  decides where things go.
- **Gotcha.** git2 0.21 returns `Result`, not `Option`, from `path()`, `name()`
  and `shorthand()`.
