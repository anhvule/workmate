---
id: 023
title: The engine event stream
type: task
mode: AFK
status: open
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
