---
id: 004
title: Scaffold the workmate repo
type: task
mode: AFK
status: open
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
