---
id: 007
title: What "repo-native" actually means
type: grilling
mode: HITL
status: closed
assignee: agent (autonomous mode)
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

## Known before starting

From [Can OpenCode drive a collaborating agent team?](001-opencode-agent-team.md):
OpenCode's git awareness is read-level only — `GET /vcs` returns `{ branch }`,
sessions expose diff summaries and snapshot revert. Branching, staging and commits
go through the `bash` tool, but `permission.bash` takes last-match-wins glob rules
and every request surfaces as an event with an explicit `once`/`always`/`reject`
reply, so workmate can gate git precisely — it just has to implement it.

Both research tickets converge on the **isolation** bullet from opposite
directions: OpenCode's worktree isolation is `experimental_`-prefixed, so workmate
should create git worktrees itself and pass each session a different `?directory=`,
which is the stable API. That makes "a role works in its own worktree" the cheap
option rather than the expensive one.

From [Cowork-z architecture up close](002-cowork-z-architecture.md): cowork-z's
`Input/Output/Misc/Artefacts` convention is prompt-enforced and its own code
concedes bash bypasses the edit rules — so it is advisory governance over a source
tree. Deriving the write surface from git state instead is the divergence this
ticket exists to specify.

## Resolution

> **Agent-made decision.** Taken autonomously at the user's instruction.

**First-class in v1** — displayed and acted on: branch, worktree, diff, commit,
working-tree status. **Ignored:** stash. **Ruled out of scope:** pull requests and
CI status; v1 stops at local git, and GitHub integration is recorded on the map's
*Out of scope* rather than left as fog.

**A workspace is bound to one directory and has zero or one repo.** Repo state is
derived and cached, never authored. If the branch changes underneath, the
workspace is unaffected — it is bound to the directory, not the branch — the cache
invalidates and the UI says so.

**Isolation is the decision that makes the rest safe: a run works in its own git
worktree on its own branch**, `workmate/run-<id>`, never the user's checkout. This
is what makes unattended cron automations defensible, and it is cheap rather than
expensive, because passing each session a different `?directory=` is OpenCode's
stable API while its own worktree isolation is `experimental_`-prefixed. The user
reviews and merges; workmate never merges silently.

**What agents may do unasked:** read, edit, stage and commit **inside their own
worktree**. Branch and worktree creation belong to workmate, not the agent. Push
always asks. `.git/` internals are never writable.

**The permission surface is derived from git, not from folders.** Cowork-z's
`Input/Output/Misc/Artefacts` taxonomy is dropped entirely: it is enforced by
prompting the agent to `mkdir -p`, it litters a repo root with folders that do not
belong in the tree, and its own code concedes bash bypasses the edit rules. The
worktree is writable, `.git/` is not, and destructive operations are mediated by
gated commands. Bash gating uses `permission.bash` last-match-wins globs.

**Review surface:** workmate's own per-run diff view, plus the worktree path and
an open-in-editor action. Merging is an explicit workmate action that asks.

**Library: `git2`** (libgit2) in-process for status and diff, because those run on
every change and a subprocess per query does not survive that. Shelling out is
retained only for credentialled clone, where cowork-z's approach is sound.
*`gitoxide` is the plausible alternative; `git2` wins on maturity of worktree
support.*
