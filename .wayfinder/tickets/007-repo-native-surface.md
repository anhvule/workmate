---
id: 007
title: What "repo-native" actually means
type: grilling
mode: HITL
status: open
assignee:
blocked-by: [003]
---

## Question

Cowork-z's workspace is a folder of files. Workmate's third differentiator makes
git first-class — but "git-aware" spans everything from showing a branch name to
running agents against worktrees and opening pull requests. Fix the line.

Decide:

- **Which git objects are first-class** in the UI and the domain model: branch,
  worktree, diff, commit, stash, PR, CI run. For each: does workmate display it,
  act on it, or ignore it in v1.
- **The relationship between a workspace and a repo.** Is a workspace a repo, does
  it contain several, or can it contain none? What happens to a workspace when the
  repo's branch changes underneath it.
- **Isolation.** Does an agent team work on the user's checkout, or in its own
  worktree or branch that the user reviews and merges? This is the decision that
  determines whether workmate is safe to leave running unattended — and cron
  automations are in v1 scope, so it must answer for them too.
- **What agents may do to a repo** without asking: read, edit, stage, commit,
  branch, push. How that intersects with the folder-level permission model
  inherited from cowork-z.
- **Review surface.** How a user sees what the team changed — workmate's own diff
  view, or handing off to the user's editor and `git` itself.
- **Remotes and hosting.** Whether GitHub (PRs, CI status) is in v1 at all, or
  whether v1 stops at local git. Note the decision either way — if it's out, it
  belongs in the map's *Out of scope*, not its fog.

Depends on the workspace/repo relationship settled in
[Workmate's domain model](003-domain-model.md).
