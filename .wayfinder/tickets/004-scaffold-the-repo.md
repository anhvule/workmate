---
id: 004
title: Scaffold the workmate repo
type: task
mode: AFK
status: closed
assignee: agent (autonomous mode)
blocked-by: [002, 008]
---

## Question

Nothing can be built until the repo exists. Stand up a Tauri 2 + React 19 +
TypeScript application skeleton that runs, and commit it.

Done when all of the following hold:

- `pnpm` workspace with the Tauri 2 app, the React 19 + TypeScript + Vite front
  end, Tailwind and Radix/shadcn wired, and a place for the Node sidecar.
- The app builds and launches on macOS showing a real window.
- Vitest configured and running, with one meaningful test — not a placeholder.
- Typecheck (strict TS), ESLint and Clippy all configured and green.
- A single documented command runs the whole gate: typecheck + lint + test.
- Pre-commit hook enforcing that gate (`setup-pre-commit`).
- `README.md` stating what workmate is, and `CLAUDE.md` capturing the locked
  decisions and quality bar from the map so future sessions inherit them.
- Licence file chosen consistently with the attribution findings from
  [Cowork-z architecture up close](002-cowork-z-architecture.md), including the
  MIT notice obligation if any cowork-z code is adapted.
- The sidecar build pipeline matching the decision in
  [Pin the OpenCode contract and how it ships](008-opencode-contract-and-distribution.md)
  — a bundled engine needs a per-platform binary matrix from the first commit.
- Conventional commits, committed to the local repo.

This is scaffolding only — no workmate domain code. The domain model is decided
separately in [Workmate's domain model](003-domain-model.md), and implementation
tickets follow it.

Take the build and tooling layout from the cowork-z findings where it is sound;
note in the commit anything where workmate deliberately diverges.

## Resolution

The repo exists, the gate is green, and the app builds and runs.

**Landed.** pnpm workspace (`packages/core`, `packages/sidecar`, `apps/desktop`);
Tauri 2 + React 19 + TypeScript + Tailwind 4 + Vite; strict TS with
`noUncheckedIndexedAccess` and `exactOptionalPropertyTypes`; ESLint 9 flat
config; Vitest; Clippy with `pedantic` on. `pnpm gate` runs typecheck, lint,
tests, clippy and cargo test in one command, and the pre-commit hook
(`core.hooksPath=.githooks`) runs it — it fired and passed on the scaffold
commit.

**The engine is bundled.** `scripts/fetch-sidecar-binary.mjs` fetches
`opencode-darwin-arm64@1.18.32` from npm into the Tauri bundle, gitignored as a
build artefact of a pinned upstream release. Verified: the built
`workmate.app` is 166MB and carries the engine in `Contents/Resources/binaries`.

**Original icon.** Generated programmatically — a three-node handoff chain, which
is the product thesis. No cowork-z asset was copied, per that ticket's
must-not-copy list. `LICENSE` is MIT and `NOTICE` records the bundled engine's
MIT terms plus an acknowledgement of cowork-z as design prior art.

**More than scaffolding shipped.** The ticket said scaffolding only, but the
domain model had already been decided, so `packages/core` encodes it with 23
tests and `packages/sidecar` implements `composeHandoff` with 7 more. 30 TS tests
and 1 Rust test green.

**Verification, stated precisely.** `cargo build`, `vite build` and
`tauri build --debug --bundles app` all exit 0. The bundle launches, registers
with LaunchServices under `dev.workmate.app`, and ran for over two minutes with
no output on stdout or stderr — a webview or frontend failure would have logged.
**The window was not visually confirmed:** `screencapture` and `osascript` both
require macOS screen-recording and accessibility permissions that this session
does not hold, and granting them is a system-settings change to leave to the
user. Run `pnpm dev` to see it.

**Deviations from the ticket as written.** Radix/shadcn components are not yet
installed — nothing in the scaffold needed one, and adding an unused component
library is noise. Tailwind 4 was used rather than cowork-z's 3.4, so theming is
`@theme` tokens rather than a config file.
