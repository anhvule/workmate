---
id: 027
title: Escape paths that look like globs
type: task
mode: AFK
status: open
assignee:
blocked-by: []
---

## Question

`permissions::compile` interpolates a filesystem path straight into a glob
pattern. A directory named `foo[1]`, `report*` or `draft?` therefore compiles
into a rule matching more than the folder the user actually granted — a
permission surface wider than the one they agreed to.

- Find out what OpenCode's matcher treats as a metacharacter and whether it
  offers an escape, before inventing one.
- Escape on the way into a pattern, not on the way into the table: the stored
  grant is a path and should stay readable as one.
- The same question applies to the synthetic workspace-root and worktree rules,
  which interpolate paths workmate did not choose either.
- Test with a workspace whose folder name contains each metacharacter.
