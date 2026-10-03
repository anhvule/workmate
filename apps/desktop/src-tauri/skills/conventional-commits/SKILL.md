---
name: conventional-commits
description: Write commit messages in the Conventional Commits style. Use when committing, or when asked to tidy a commit history.
---

# Conventional commits

Format: `type(scope): summary`, then a blank line, then a body when the *why* is not obvious.

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`.

- Summary in the imperative, under about 70 characters, no trailing full stop.
- The body explains why, not what; the diff already shows what.
- One logical change per commit. If the summary needs "and", split it.
- Mark a breaking change with `!` after the type and a `BREAKING CHANGE:` line in the body.
