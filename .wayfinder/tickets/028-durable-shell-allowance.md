---
id: 028
title: Whether a shell allowance can ever be durable
type: grilling
mode: HITL
status: open
assignee:
blocked-by: [014]
---

## Question

[Workspaces and the permission surface](013-workspaces-and-permissions.md) made
bash allowances unstorable: the grant table holds `read` and `edit` only, so
"push always asks" is true by construction rather than by a user not having
clicked *always*. The cost is that every `pnpm test` in a run asks again.

- Is ask-every-time actually intolerable in use, or does the worktree isolation
  make a run's shell prompts rare enough to live with? Answer from a real run,
  not from first principles — hence the block on
  [Run orchestration](014-run-orchestration.md).
- If durable shell allowances are wanted: what keeps the dangerous verbs gated
  when the mechanism that gates them is the same table the user can widen?
  A per-command allowlist that push simply cannot be added to is one shape.
- Scope matters: allowed for this run, this workspace, or everywhere?
- Whatever the answer, the compiled hard denies must stay last.
