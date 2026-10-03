---
id: 019
title: Cron automations
type: task
mode: AFK
status: closed
assignee:
blocked-by: [014, 016]
---

## Question

Scheduled, unattended runs — and the reason worktree isolation had to land first.

- Scheduling surface and persistence; one run per fire, bounded concurrency.
- Unattended runs work in their own worktree and never touch the user's checkout.
- What a permission prompt means with nobody watching: deny by default, or a
  pre-granted profile attached to the automation. Decide and justify.
- Where findings surface, and how an automation that finds nothing stays quiet.
- Run history, failure handling, and what happens when the machine was asleep.

## Resolution

- **Scheduling.** A hand-written five-field cron (`cron.rs`, 9 tests) in the
  user's local time. Hand-written because the surface is small and mis-firing an
  agent is the failure that matters: the day-of-month/day-of-week OR rule, month
  ends, 29 February and a schedule that never fires (`0 0 31 2 *`, refused at
  creation) are all pinned. No names or `@daily` shorthands.
- **Persistence (migration v4).** `automation` and `automation_fire`. V1 had
  reserved an `automation` table nothing ever wrote to; it is replaced, not
  altered. A fire is *history*, so `run_id` has no foreign key and survives the
  run being archived. Removing a workspace takes its automations and history.
- **A scheduler in Rust.** A thread ticks every 20s, calls the pure `plan`, and
  starts runs through the sidecar's `run.start`. In Rust, not the webview, so a
  throttled `WKWebView` cannot make an automation late. If the runtime is down
  the tick is skipped and the slot stays due.
- **One run per fire, bounded.** At most 2 automation runs at once; overflow
  *defers* (stays due) rather than being lost. An automation never overlaps
  itself: a fire that finds the last one still running is recorded as
  `skipped_overlap`. On startup any fire still `started` is closed as failed
  ("interrupted"), or the automation would skip itself forever behind a ghost.
- **A sleeping machine coalesces.** Late by under 24h: fire *once*, now.
  Older: record `missed`. Either way the next slot is computed from now, so
  waking never replays every slot that was slept through.
- **Permission prompts when nobody watches: deny.** An unattended run's ruleset
  turns every *ask* into a *deny* (`without_prompts`): same shape, same order,
  nothing widened, the hard denies untouched, and an MCP tool cannot run
  unattended. Asking would block forever or invite auto-approval; refusing is the
  only default that is never worse than not running. The summary is where the
  user finds out what was refused. A pre-granted profile was considered and not
  built: it would be a second grant mechanism that bypasses the "only read/edit
  are storable" rule, which is [ticket 028](028-durable-shell-allowance.md)'s to
  settle.
- **Quiet by default.** Unattended runs are told to answer exactly
  `NOTHING_TO_REPORT` when they find nothing. That outcome is a history row and
  nothing else — the worktree and branch are discarded. Anything else is a
  *finding*: kept, unseen until read (`automation_unseen` for a badge),
  announced as `automation.finding`.
- **Failure.** A run that cannot start, or blocks (a missing key), closes its fire
  as `failed`/`blocked` with the reason. There is no automatic retry: an
  unattended retry loop against a missing credential helps nobody.
- **Not built:** OS notifications (the finding event and `automation_unseen` are
  there for the shell to use), and wake-from-sleep scheduling — workmate must be
  running; a closed app records nothing and is not pretending otherwise.
