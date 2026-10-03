---
id: 027
title: Escape paths that look like globs
type: task
mode: AFK
status: closed
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

## Resolution

Checked against the pinned `1.18.32` binary rather than assumed: its matcher
turns `*` into `.*` and `?` into `.`, regex-escapes everything else, and anchors.
So `[ ] { } ( )` are already literal and need nothing. `*` and `?` **cannot be
escaped at all** — a backslash is rewritten to `/` before matching.

- Patterns are built through `literal()`, which spells `*` as `?`: a one-character
  wildcard instead of an unbounded one. That is the tightest the engine allows,
  and it still matches one stray character at that position.
- Because of that residue, an explicit `grant` for a path containing `*` or `?`
  is **refused** with a named error. Only paths workmate did not choose (the
  workspace root, the worktree) can reach a rule with one, and the root is
  read-only.
- Tests reproduce the engine's matcher and assert `foo[1]` does not match
  `foo1`, `report*` does not match `report_final`, and `draft?` does not match
  `draft-final`; the worktree rules are covered too.
