---
id: 025
title: Bundle and supervise the Node sidecar
type: task
mode: AFK
status: open
assignee:
blocked-by: [024]
---

## Question

Make the TypeScript orchestration an actual running process, per
[Where orchestration runs](024-where-orchestration-runs.md).

- Compile `packages/sidecar` (with `core` and `opencode-client`) to a single
  binary with Bun's `--compile`, per host target, into the Tauri bundle beside
  the engine. Fall back to `@yao-pkg/pkg` if the workspace layout fights it.
- Generalise `engine.rs` from supervising one child to supervising two. The
  readiness handshake — wait for the child's own announcement, never treat spawn
  as ready — applies to both.
- Rust hands the sidecar the engine address and launch password over its stdin
  handshake; the password must never reach the webview, mirroring the api-key
  fingerprint bridge.
- Define the persistence IPC: the sidecar asks Rust to read and write, so Rust
  stays the only writer. Keep the surface narrow and typed on both sides.
- Shutdown ordering: sidecar first, then engine, so the sidecar is never talking
  to a dead engine. Both on `ExitRequested`.
- The per-platform matrix now has two binaries; update the build accordingly and
  note it in [Packaging, signing and release](021-packaging-and-release.md).
