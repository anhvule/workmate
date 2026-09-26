---
id: 003
title: Workmate's domain model
type: grilling
mode: HITL
status: open
assignee:
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
