---
id: 023
title: The engine event stream
type: task
mode: AFK
status: closed
assignee:
blocked-by: [012]
---

## Question

Consume the engine's SSE stream in Rust and surface it to the webview. Split out
of [OpenCode engine client and sidecar supervision](012-engine-client.md), which
delivered spawn, readiness, auth and shutdown; this is the other half.

- Subscribe to `GET /global/event` with the launch password, in Rust. Cowork-z
  documents that `/event` is the wrong path on this engine line — use
  `/global/event`.
- Emit to the webview as `sidecar:{type}`, verbatim, with a typed union shared
  across the Rust boundary so a contract change is a type error.
- Reconnect on drop without losing ordering guarantees, and say what happens to
  events that arrive while disconnected.
- Intercept what must not reach the webview — anything carrying a credential —
  and handle completion in Rust rather than the webview, because a backgrounded
  `WKWebView` is throttled and will miss it.
- Surface permission requests so [Workspaces and the permission surface](013-workspaces-and-permissions.md)
  can gate them, and tool calls so runs can be rendered.

## Resolution

`events.rs` subscribes to `/global/event` from Rust with the launch password
(Basic `opencode:<password>`), on its own thread, via `ureq` with TLS stripped —
the engine is plain local HTTP.

- **Delivery.** A pure `SseParser` turns lines into events; `classify` accepts
  only `{ payload: { type } }` envelopes and drops anything else, so the webview
  sees only the typed union. Names are `sidecar:<type>` with `.` → `_`, because
  Tauri rejects dots in event names. TypeScript carries the matching
  `SidecarEvent` whose payload is the engine's generated `Event`, so a contract
  change is a type error; a test pins the naming rule on both sides.
- **Ordering and gaps.** Each event carries a `seq` that never resets and an
  `epoch` that increments per connection. After a reconnect a synthetic
  `sidecar:stream_resumed {missed:true}` is delivered first. Events sent while
  disconnected are gone from the stream; since the engine owns the transcript,
  the consumer's answer is to resync from it, never to trust the stream as a log.
  Reconnect backs off exponentially from 500ms to 60s.
- **Credentials.** `scrub` redacts any key containing password, token, secret,
  apikey or authorization at any depth, before the sink sees the event.
- **Completion in Rust.** The sink runs on the subscriber thread whether or not a
  window exists, and each event is tagged `Kind::Permission`, `Idle` or `Other`.
  Reacting to `Idle` (marking a run done, triggering the next handoff) is the
  orchestrator's job in [run orchestration](014-run-orchestration.md); the seam
  is the sink. Permission asks are surfaced the same way for the gate.
- **Limit worth knowing.** There is no body timeout (the stream must stay open),
  so a silently stalled connection is only noticed when the engine closes it.
  The engine heartbeats, so this is a monitoring upgrade rather than a gap, but
  it is one.
- **Tested** with a real local SSE server: reconnect, ordering, the gap marker
  and the auth header on every connection.
