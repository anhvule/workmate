---
id: 014
title: Run and team orchestration
type: task
mode: AFK
status: closed
assignee:
blocked-by: [011, 012, 016, 017]
---

## Question

The differentiator itself: run a team of roles on one task, with real handoffs.

- A run owns its sessions. **Nothing may cull a session because another run
  started** — the cowork-z behaviour this design exists to avoid.
- Concurrency is a bounded pool keyed on run, not a global boolean.
- Each role gets its own session, its own worktree directory, its own system
  prompt, model and tool allowlist, resolved from the role record.
- Handoffs are composed by `composeHandoff` in `@workmate/sidecar`, persisted,
  and rendered as cards.
- Pause, amend a pending handoff, redirect to a different role, and veto — per
  [What the agent team looks and feels like](005-agent-team-ux.md).
- A one-role run must take the same code path and render as a plain chat.
- Run timeline ordering comes from `runTimeline`; the UI must not re-sort.

## Resolution

Orchestration is `packages/sidecar/src/orchestrator.ts`; everything with an
effect outside the process is a named operation Rust serves.

- **One loop for every run.** `Run.drive` walks the roles in order. A one-role
  run takes the same path and simply has no boundary: no `handoff.*` event, no
  `handoff.append`, the objective as its prompt. A test asserts exactly that, so
  "a one-role run reads as a plain chat" is enforced rather than hoped for.
- **A run owns its sessions.** Nothing deletes a session because another run
  started (asserted). Concurrency is `TurnPool`, a fair bounded set of *turn*
  slots, default 3; it bounds engine work, not runs, so a paused or blocked run
  holds no slot.
- **Roles.** Each gets its own session in the run's worktree, its system prompt
  plus the memory digest in `system`, its model, and its allowlist as the
  engine's `tools` map, with `remember`/`recall` always on. Roles are upserted
  from the command, so the role record, not a prompt, is the source.
  *Deviation from the ticket text:* roles share the run's **one** worktree
  rather than one each. Teams are sequential (ticket 005), so a later role has
  to see the earlier role's changes; per-role worktrees would make every handoff
  a merge. Parallel roles would reopen this.
- **Handoffs** are composed by `composeHandoff` from the engine's own transcript,
  proposed as `run.handoff.proposed`, and only persisted (`handoff.append`)
  after the receiving session exists, with the context that was actually
  delivered, amended or not.
- **Intervention.** `pause` takes effect at the next boundary (a turn in flight
  is not interrupted). At a boundary the user can `resume`, `amend` the context,
  `redirect` to another role on the team, or `veto`, which ends the run. Sessions
  stay alive while paused.
- **Blocking, not failing.** A missing credential or an engine error moves the
  run to `blocked` with a structured reason (the accounts tried; the message) and
  waits for `resume`, which retries the same role on the same session. Finished
  roles' work is never discarded.
- **Credentials.** `credentials.provision` runs in Rust: resolve role →
  workspace → global, read the key, `PUT /auth/{provider}` on the engine. The
  reply is `ok`/`missing` and never contains the key; errors redact it.
  *Known limit:* engine auth is per provider, not per session, so two roles that
  resolve different keys for the same provider cannot run concurrently. Teams are
  sequential today; revisit with parallel roles.
- **Host operations (`host.rs`).** `repo.*` (worktree, merge, diff, status),
  `permission.ruleset` and `credentials.provision`, addressed by id, never by
  path; unknown names fall through to the database ops.
- **Commands.** The webview calls `sidecar_command` → Rust → sidecar `cmd` →
  `cmd.result`, with a timeout. The sidecar's events reach the webview as
  `workmate:<name>`. `run.get` returns the persisted run in Rust's order for the
  UI to feed `runTimeline`; the UI does not re-sort.
- **Cleanup.** `run.archive` keeps the branch and refuses uncommitted work;
  `run.abandon` deletes the branch and needs `force` to discard changes. Merge is
  only ever the user's explicit `run.merge`.
- **Also fixed:** the two real-engine tests contended and one exited during
  startup under `cargo test`; they now serialise on a test-only lock.
- **Not yet proven live.** The orchestrator is tested against fake engine and
  database ports (14 tests) and the pipes are tested against a stand-in sidecar,
  but no test drives a real model turn: that needs a provider key. Two engine
  behaviours are assumptions to confirm on first live run: that `POST /mcp` with
  an existing name replaces the registration, and that a session `permission`
  ruleset accepts the compiled array as sent.

### Addendum — verified against the real engine

The two assumptions above were checked with `pnpm test:e2e`
(`packages/sidecar/src/engine.e2e.test.ts`): the real bundled engine, a fake
OpenAI-compatible model, and the sidecar's own memory endpoint. Both hold:

- A session accepts the compiled permission ruleset exactly as workmate sends it,
  runs a real turn, keeps the transcript, and receives the per-turn `system` field.
- `POST /mcp` under an existing name replaces it (one entry afterwards).
- The engine connects to the loopback memory endpoint with its bearer token, gives
  the model `workmate-memory_remember` / `_recall` (the names the allowlist already
  assumed), and a model-issued tool call reaches `memory.remember` attributed to
  the right run and scope.

The test is opt-in rather than in the gate: it spawns the engine and, on a cold
cache, fetches a provider package.
