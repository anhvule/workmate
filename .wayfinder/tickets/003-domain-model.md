---
id: 003
title: Workmate's domain model
type: grilling
mode: HITL
status: closed
assignee: agent (autonomous mode)
blocked-by: [001, 002]
---

## Question

What are workmate's nouns, how do they relate, and what persists where?

The candidates, to be confirmed, renamed, merged or rejected: **Workspace**,
**Session**, **Turn**, **Agent Role**, **Team**, **Memory**, **Repo**,
**Permission Grant**, **Automation**, **Skill**, **Starter Pack**, **Provider**,
**MCP Server**.

The decisions this ticket must settle:

- Which concepts are first-class and what each one is *for* — in the ubiquitous
  language, not in implementation terms.
- The relationships and cardinalities, especially: does a Session belong to one
  Agent Role or does a Team share a Session? Is Memory scoped to a Workspace, to a
  Repo, to the user, or to several of those at once?
- Which concepts cowork-z has that workmate deliberately reshapes — Arena being
  the obvious one, replaced by Team.
- What is persisted in SQLite, what lives on disk in the workspace folder, what is
  OpenCode's own state that workmate must not duplicate, and what is derived.
- The identity and lifecycle of each entity: how it's created, what invalidates it,
  what happens when the underlying folder or repo moves or disappears.

Use `domain-modeling` and record the result as the project's domain model, since
every subsequent ticket and every schema depends on this vocabulary.

Read the resolutions of [Can OpenCode drive a collaborating agent team?](001-opencode-agent-team.md)
and [Cowork-z architecture up close](002-cowork-z-architecture.md) first — they
constrain what is representable.

## Resolution

> **Agent-made decision.** Taken autonomously at the user's instruction.

The vocabulary, and where each concept lives. Terms in **bold** are the ubiquitous
language; every schema, command and UI label should use them.

**Workspace** — a named working context bound to exactly one directory. It may or
may not be a git repository. It owns permissions, automations and MCP config. It
does **not** own memory. Identity is a stable id, not the path, so a moved folder
is repaired rather than orphaned.

**Repo** — the git facts of a workspace's directory: branch, dirty state, tracked
files. *Derived, never authored.* Not a separate persisted entity; cached with an
invalidation stamp. A workspace has zero or one repo.

**Run** — one unit of work handed to a **Team**. This replaces cowork-z's `Task`
and is the aggregate that fixes its worst constraint: a run **owns** its member
sessions, so sessions end when their run ends rather than being culled by a
"stale session" heuristic when the next task starts. Concurrency is bounded by a
pool keyed on run, not cowork-z's single global boolean.

**Role** — a named agent persona: system prompt, model, tool allowlist, permission
profile. **First-class data in its own table.** Cowork-z has no such entity — one
persona baked into a prompt string — and roles-as-data is the precondition for the
agent-team differentiator.

**Team** — an ordered arrangement of roles, saveable as a preset. A one-role team
is the degenerate case and must not feel heavier than a plain chat.

**Session** — one OpenCode session, belonging to exactly one run and one role, and
pinned to one directory (its worktree). Workmate stores an **index** — id, run,
role, directory, engine version — and not the transcript. See
[Who owns the conversation](010-conversation-source-of-truth.md).

**Turn** — a message within a session, attributed to its role. Ordering across a
run is by handoff edge first, wall-clock second.

**Handoff** — an explicit edge from one session to another carrying selected
context. A first-class record, because it is both the audit trail and the thing
the UI renders; OpenCode's native delegation carries only a result string, so
workmate composes and stores this itself.

**Memory** — a fact workmate retains, with provenance and an age.
**Deliberately not owned by a workspace.** Memory rows live in their own table;
a separate scope table associates a memory with workspaces, repos or roles.
Deleting a workspace deletes scope rows and **never cascades into memory**. This
is the single most important schema decision on the map — cowork-z cascades
everything off `workspace_id`, which would delete the differentiator along with a
removed folder.

**PermissionGrant** — an allowance over a path or operation, with a source
(workspace, user, adhoc). Workspace root is synthetic and never stored — adopted
from cowork-z.

**Credential** — a secret in the OS Keychain, identified by
`(provider, scope, key-format-version)`. Model choice is a *separate* field from
credential, so a role can use a different model on the same key. See
[How credentials are scoped](009-credential-scoping.md).

**Provider**, **McpServer**, **Automation**, **Skill**, **StarterPack** — retained
from cowork-z with its shapes, since nothing about the differentiators disturbs
them.

**Rejected.** *Arena* — replaced wholesale by Team; competing clones are not the
product. *Task* — renamed to Run, because it now has members. *The
`Input/Output/Misc/Artefacts` folder taxonomy* — see
[What "repo-native" actually means](007-repo-native-surface.md).

**Storage split.** SQLite owns workspaces, roles, teams, runs, the session index,
handoffs, memory and its scopes, grants, automations, providers. The OS Keychain
owns secrets, and nothing else. OpenCode owns transcripts and its own session
state — workmate must not duplicate them. The workspace directory owns the user's
actual files, and workmate writes nothing into it that is not the user's work.
