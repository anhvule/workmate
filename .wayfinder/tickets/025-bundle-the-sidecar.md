---
id: 025
title: Bundle and supervise the Node sidecar
type: task
mode: AFK
status: closed
assignee: agent (autonomous mode)
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

## Resolution

The TypeScript orchestration is a real process now. 36 Rust tests and 41
TypeScript tests green, including both children actually starting.

**`pkg` was tried and rejected on evidence.** The decision in
[Where orchestration runs](024-where-orchestration-runs.md) named Bun first with
`@yao-pkg/pkg` as the fallback; the fallback was attempted because Bun was not
installed. It has no prebuilt runtime for `node20-macos-arm64` and silently falls
back to **compiling Node and V8 from source** — it was still building after ten
minutes, which would be 30-90 minutes on every build machine and every CI run.
Bun compiles the same entry point in **214ms**. Bun is now a build dependency;
its installer adds `~/.bun/bin` to the shell profile, which is worth knowing.

**Landed.**

- `packages/sidecar/src/protocol.ts` — newline-delimited JSON with a `LineFramer`
  that carries a partial line across chunk boundaries and treats an unterminated
  line beyond 1MiB as a malformed stream rather than buffering forever. 14 tests,
  including recovery after a malformed stream rather than staying wedged.
- `redact` strips passwords, tokens and API keys from anything logged. The engine
  password crosses this pipe and must go no further.
- `bin.ts` is a separate entry point from `main.ts`, so importing the message
  loop for a test does not start it — a module that boots on import cannot be
  tested honestly.
- `sidecar.rs` supervises the second child: writes `hello` carrying the engine
  address, then **waits for the sidecar's own `ready`**, never for the spawn.
  `classify` is a standalone decision table so the handshake is testable without
  spawning anything.
- `runtime.rs` owns the ordering. Start is engine then sidecar, because the
  sidecar is handed the engine's address. Stop is sidecar then engine, so the
  sidecar is never left talking to a dead engine. If the sidecar fails to start,
  the engine is stopped again rather than orphaned. Starting twice is idempotent.
- `start_runtime` is the Tauri command that brings the pair up; it returns the
  base URL and deliberately **not** the password.

**Verified end to end**, not just unit tested: the compiled 59MB sidecar binary
handshakes over a real pipe, and `the_bundled_pair_starts_and_stops_in_order`
starts the real engine and the real sidecar together.

**Not done here.** The persistence round trip is defined in the protocol but
nothing is behind it — split out as [The persistence IPC](026-persistence-ipc.md).
The per-platform matrix now carries two binaries;
[Packaging, signing and release](021-packaging-and-release.md) must account for
both.
