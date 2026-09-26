---
id: 012
title: OpenCode engine client and sidecar supervision
type: task
mode: AFK
status: open
assignee:
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
