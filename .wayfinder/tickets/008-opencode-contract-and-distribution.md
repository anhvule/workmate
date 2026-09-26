---
id: 008
title: Pin the OpenCode contract and how it ships
type: grilling
mode: HITL
status: closed
assignee: agent (autonomous mode)
blocked-by: []
---

## Question

Two findings turned OpenCode's distribution from a detail into a decision that has
to be made before anything is scaffolded.

First, **OpenCode ships prebuilt per-platform binaries under MIT, suitable for
Tauri sidecar bundling** — so workmate is not obliged to copy cowork-z's
requirement that the user `npm i -g opencode-ai` before the app works at all.
Second, **the published API is mid-migration in one place that matters**: the
prompt body's `tools` field is deprecated in favour of permissions set on the
session, the runtime already reads `session.permission`, but the published
`SessionUpdateData.body` still accepts only `{ title? }`. Building on the wrong
side of that costs a rewrite.

Decide:

- **Bundle or require.** Does workmate vendor OpenCode's binary as a Tauri
  sidecar, require a user-installed global, or detect-and-offer-to-install? Bundling
  buys a first-run experience cowork-z cannot match, and costs a per-platform build
  matrix, a binary in the installer, and ownership of upgrades.
- **If bundled: upgrades.** Does the engine version move only when workmate ships,
  or can it update independently? What happens to a session recorded against an
  older engine.
- **Which version to pin**, and the policy for moving it — given the session
  permissions surface above, pin a specific version and state what must be
  re-checked before it moves.
- **Which org is canonical.** `sst/opencode` still resolves, but its own README
  badges and Homebrew tap point at `anomalyco/opencode`. Confirm where the project
  actually lives now and pin against that; a stale origin is a supply-chain
  question, not a cosmetic one.
- **Contract discipline.** OpenCode's types are OpenAPI-generated. Does workmate
  vendor generated client types and diff them on upgrade — the thing that would
  have caught the `tools` deprecation early — or hand-write the surface it uses?
  Note that cowork-z's `opencode-api.json` is OpenCode's artefact and must not be
  copied from cowork-z; generate it from source.

Grounded in [Can OpenCode drive a collaborating agent team?](001-opencode-agent-team.md)
§1 and §7, and cowork-z's sidecar build pipeline in
[Cowork-z architecture up close](002-cowork-z-architecture.md) §1.

Blocks [Scaffold the workmate repo](004-scaffold-the-repo.md): the answer decides
whether the build pipeline needs a per-platform binary matrix on day one.

## Resolution

> **Agent-made decision.** Taken autonomously at the user's instruction; no human
> ratified these. Each is cheap to overturn before scaffolding lands.

**Bundle.** OpenCode ships as a Tauri `externalBin` sidecar. `opencode-ai` is MIT
and publishes per-platform binary packages as optional dependencies
(`opencode-darwin-arm64`, `opencode-darwin-x64`, `opencode-windows-x64`,
`opencode-linux-x64`, plus musl and baseline variants), so the binary can be
fetched at build time and renamed to the Rust target triple. Workmate therefore
works on first launch, where cowork-z requires `npm i -g opencode-ai` first. This
is a real, cheap product differentiator and the reason to accept the cost.

**Cost accepted:** a per-platform build matrix from the first commit, a larger
installer, and ownership of engine upgrades. macOS arm64 and x64 only for now;
Windows and Linux remain in the map's fog.

**Pinned to `opencode-ai@1.18.32`** — exact version, no range, enforced by lockfile
and asserted in CI. The engine moves only when workmate ships; there is no
independent update channel, because an engine that changes under a recorded
session is a support problem workmate cannot debug.

**Version recorded per session.** Every session row stores the engine version that
produced it, so resuming across an upgrade can warn rather than silently
misbehave. See [Who owns the conversation](010-conversation-source-of-truth.md).

**Before the pin moves, re-check:** whether `SessionUpdateData.body` accepts
session-level permissions yet (the `tools` field is deprecated in favour of them
but the published type still only takes `{ title? }`), and whether worktree
isolation is still `experimental_`-prefixed.

**Canonical org: `anomalyco`.** The npm package carries no `repository` field, so
the registry does not settle it; cowork-z's own reading and OpenCode's README
badges and Homebrew tap point at `anomalyco/opencode`. Pin git-sourced references
there and treat `sst/opencode` as a legacy mirror. *Confidence: moderate — this is
the one decision here resting on secondary evidence.*

**Contract discipline.** Generate the client types from the pinned engine's own
OpenAPI document at `/doc` into `packages/opencode-client`, commit the generated
output, and fail CI on an undiffed regeneration. That is the mechanism that would
have caught the `tools` deprecation early. Do not copy cowork-z's
`opencode-api.json`; it is OpenCode's artefact and must be generated from source.
