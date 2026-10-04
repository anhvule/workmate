---
id: 028
title: Whether a shell allowance can ever be durable
type: grilling
mode: HITL
status: closed
assignee: claude
blocked-by: [014]
---

## Question

> Widened by [MCP server configuration](018-mcp-servers.md): MCP tools always ask
> by the same construction, so this decision now covers them too.

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

## Resolution

> **Decided by the user** (2026-10-04): a per-workspace allowlist.

**Yes, a shell or MCP allowance can be durable — as a screened, per-project
allowlist, kept apart from the path grants.** Built as `allowance.rs`
(migration v7), compiled by `permissions::with_allowances`.

- **What can be stored.** A *command prefix* (`pnpm test`) or an *MCP server name*
  (`github`), for one workspace, going away with it. Never a pattern: workmate
  compiles `pnpm test *` itself.
- **Screened at write time, so the table can only hold what passed.** Refused,
  with the reason shown: shell syntax of any kind (`; & | $ \` > * …`); dangerous
  verbs on the first word or first two (`git push`, `git reset`, `npm publish`,
  `rm`, `sudo`, `curl`, `ssh`, `gh`, `docker` …, matched on the basename so
  `/bin/rm` counts); anything whose job is running other code (`npx`,
  `pnpm exec`/`dlx`, `bash`, `node`, `python`, `env`, `-c`/`-e`); and a bare
  multiplexer (`pnpm` alone would allow `pnpm publish`).
- **Placement.** Allowances go after the floor and the MCP asks and before the hard
  denies; then `git push *` is re-asserted as *ask* after every allowance, so no
  stored prefix outranks it, and the `.git` denies stay last.
- **Scope answer: this workspace.** Not per run (the prompt fatigue is per
  project), not everywhere (a command safe in one repo's scripts is not in
  another's).
- **Unattended runs.** `without_prompts` leaves allows as allows, so an automation
  can now run the project's tests; the re-asserted push becomes a deny.
- **Where it is set.** The permission prompt offers "Always allow `pnpm test`
  here" (from the engine's own suggestion) or "Always allow github tools here";
  Rust screens it, and only then is the engine told `always` for the rest of the
  session. Settings lists them with Revoke.

**The question this rested on was checked, not assumed.** A prefix allowance is
only safe if the engine judges each command in a chain separately. The real-engine
test now proves it on the pinned version: with only `git status *` allowed,
`git status` runs, while `git status; touch x`, `&& touch`, `| touch` and
`$(touch …)` are each explicitly denied and the file never appears. The same fact
was already load-bearing for the shell baseline (`git status*` and friends); it is
now pinned, and an engine upgrade that changes it fails `pnpm test:e2e`.

**What the "answer from a real run" part became:** the friction was obvious from
the design (every test run asks; automations could run nothing that asked), and
the user chose to fix it rather than wait for usage data.
