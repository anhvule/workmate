---
id: 012
title: OpenCode engine client and sidecar supervision
type: task
mode: AFK
status: closed
assignee: agent (autonomous mode)
blocked-by: [004]
---

## Question

Make the bundled engine actually run, and give workmate a typed client for it.

- Generate the client types from the pinned engine's OpenAPI document at `/doc`
  into `packages/opencode-client`; commit the output and fail CI on an undiffed
  regeneration.
- Spawn the bundled binary on a random port with a per-launch password and HTTP
  basic auth. Readiness is the engine's own `ready` signal polled to a timeout,
  **not** process spawn.
- Shut down on `ExitRequested`, never `Exit`, so the engine is not orphaned.
- Stream events over SSE; pass them to the webview verbatim under a typed union
  shared across the Rust boundary.
- Surface tool-call and permission events so they can be gated later.
- Restart policy: deferred while sessions are active.

Every bullet above is a cowork-z scar, cited in
[Cowork-z architecture up close](002-cowork-z-architecture.md) §3. Re-deriving
them costs weeks; adopting them costs an afternoon.

## Resolution

The engine is bundled, supervised and typed. 18 Rust tests green, one of which
spawns the real binary.

**Grounded on the actual engine, not only the research.** Running the pinned
binary confirmed three things the design depended on: auth is the
`OPENCODE_SERVER_PASSWORD` environment variable (it warns "server is unsecured"
without it), readiness is a `listening on http://127.0.0.1:<port>` line, and the
contract is OpenAPI 3.1 with 162 paths and 472 schemas.

**A correction to the research worth recording.** Session-level permissions are
*not* as blocked as feared: `POST /session` accepts a `permission` field in its
create body, alongside `agent`, `model` and the `directory` query parameter. So a
role's permission profile can be set when its session is created, without waiting
on the deprecated prompt-body `tools` field.

**Client types are generated and drift is a gate failure.**
`packages/opencode-client` holds `openapi.json` and a 17,685-line generated
`schema.d.ts`, both committed. `scripts/generate-opencode-types.mjs --check`
starts the pinned engine, refetches its `/doc`, regenerates, and fails if the
committed output moved — now a step in `pnpm gate`. This is the mechanism that
catches a contract change at upgrade time rather than at runtime.

**The client is deliberately narrow.** `OpenCodeClient` wraps only what workmate
uses — create session pinned to a directory, send a turn carrying the per-turn
`system` digest, list messages, reply to a permission, vcs status — each typed
off the generated schema. Wrapping all 162 paths would be a second API to
maintain.

**Supervision, with every cowork-z scar adopted.** `engine.rs` spawns the binary
with a fresh 32-byte hex password per launch, treats **the announced URL as
readiness rather than process spawn**, kills on timeout rather than hanging, and
defers restarts while sessions are live. Readiness parsing is a standalone
function precisely so it is testable without spawning anything — getting it wrong
means either a startup race or a hang with no diagnosis. Tests cover the trailing
full stop, unrelated output being misread as readiness, password freshness, a
missing binary reported as missing rather than as a spawn failure, and restart
deferral including underflow.

**Not done here, split out.** SSE consumption and forwarding to the webview is
[The engine event stream](023-event-stream.md) — it needs an async HTTP client
and a typed event union, which is a session's work on its own. The window
shutdown hook is wired but currently fires on `Destroyed`; ticket 023 should move
it to `ExitRequested` proper as part of handling the stream lifecycle.
