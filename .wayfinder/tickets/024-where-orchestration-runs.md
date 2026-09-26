---
id: 024
title: Where orchestration runs
type: grilling
mode: HITL
status: closed
assignee: agent (autonomous mode)
blocked-by: []
---

## Question

An inconsistency surfaced while building, and everything downstream depends on
resolving it.

The map locked "OpenCode via Node sidecar, same as cowork-z". But
[OpenCode engine client and sidecar supervision](012-engine-client.md) bundled
the engine binary and supervises it **from Rust directly** — there is no Node
sidecar process. Meanwhile `packages/core` and `packages/sidecar` hold the
domain logic and the handoff composer, tested, and **nothing currently executes
them**.

So: which process owns the engine connection, composes turns, and runs the team?

- If **Rust** owns it, the TypeScript domain code is duplicated logic and should
  not exist.
- If a **Node sidecar** owns it, it must be bundled, and the write path to SQLite
  has to be settled — cowork-z's two-writer transcript problem is exactly what
  [Who owns the conversation](010-conversation-source-of-truth.md) forbids.
- If the **webview** owns it, that contradicts the same ticket outright.

Decide, and say what happens to the code already written either way.

## Resolution

> **Agent-made decision.** Taken autonomously at the user's instruction. This one
> revises a locked decision on the map, so it deserves a second opinion more than
> most.

**A bundled Node sidecar owns orchestration. Rust remains the only writer of
SQLite and exposes a narrow persistence IPC to it. The webview persists nothing
and talks only to Rust.**

Why not Rust-only, which is where the build had drifted: the differentiator logic
— scope resolution, memory selection and the bounded digest, handoff composition
— is written and tested in TypeScript, and it is the part of workmate most likely
to change weekly. Reimplementing it in Rust means two implementations of the
thing that *is* the product, and the engine's own client types are generated as
TypeScript, so a Rust client would need a second generated surface.

Why not let the sidecar write SQLite directly: that is two writers over one
store, which is precisely the bug
[Who owns the conversation](010-conversation-source-of-truth.md) exists to
prevent. The round trip through Rust is a real cost and is accepted deliberately.

**Consequences.**

- `packages/core` and `packages/sidecar` stay, and become the sidecar's body
  rather than unexecuted libraries.
- `packages/opencode-client` moves with them — the engine connection belongs to
  the sidecar, so `OpenCodeClient` is used there and not from the webview.
- Rust keeps process supervision, the window, the keychain and SQLite. The
  engine-spawn code in `engine.rs` stands; what changes is that Rust hands the
  address and launch password to the sidecar rather than using them itself.
- A second supervised process appears, so `engine.rs` generalises from "supervise
  the engine" to "supervise two children", and the readiness handshake
  established there applies to both.
- Bundling: compile the sidecar to a single binary with **Bun**'s `--compile`
  rather than cowork-z's `@yao-pkg/pkg`. OpenCode itself is built with Bun, so
  it is a known-good path for this codebase's shape, and it avoids pkg's Node
  version pinning. *This is the weakest link in the decision — if Bun compile
  proves awkward with the workspace layout, `pkg` is the fallback and cowork-z
  proves it works.*
- The map's Notes should be amended: the sidecar is **bundled**, not assumed
  present, matching the engine decision in
  [Pin the OpenCode contract and how it ships](008-opencode-contract-and-distribution.md).

**Nothing already written is wasted**, which is the main reason this way round
rather than the other: the domain model, the handoff composer and the typed
client all keep their place.

Split out as [Bundle and supervise the Node sidecar](025-bundle-the-sidecar.md).
