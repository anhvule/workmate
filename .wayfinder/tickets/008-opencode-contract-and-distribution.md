---
id: 008
title: Pin the OpenCode contract and how it ships
type: grilling
mode: HITL
status: open
assignee:
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
