# workmate — working agreement

Read `.wayfinder/map.md` first. It holds the destination, the locked decisions
and links to the ticket behind each one. Do not re-open a decision recorded
there without saying so explicitly.

## Locked decisions

| | |
|---|---|
| Shape | Tauri 2 desktop, macOS first |
| UI | React 19 + TypeScript + Tailwind 4 + Radix/shadcn |
| Agent engine | OpenCode, **bundled** as a binary, pinned to `1.18.32` |
| Differentiators | collaborating agent team · cross-workspace memory · repo-native git |
| Git | local repo, conventional commits |

## Rules that carry weight

**Rust is the only writer of durable state.** The webview never persists.
Optimistic UI is local component state, discarded when the authoritative event
arrives. This is not a preference — cowork-z had to retrofit it after WKWebView
throttled backgrounded listeners.

**Nothing cascades into memory.** Memory rows are associated with workspaces
through a scope table and are never owned by them. Deleting a workspace detaches;
it must never delete a memory.

**A run works in its own git worktree**, on `workmate/run-<id>`, never the user's
checkout. `.git/` is never writable by an agent. Push always asks.

**Credentials are keyed `v1:<scope>:<provider>`.** Never construct that string
outside `packages/core/src/domain/credentials.ts`. Credential and model choice
are separate fields.

**OpenCode owns transcripts.** workmate stores an index, never a copy.

**The engine version is pinned exactly, never a range**, and is recorded on every
session so a resume across an upgrade can warn.

**A one-role run must read as a plain chat.** If the simple case feels heavier
than a normal chat window, the team feature has become a tax.

## Quality bar

Test-first for the domain layer, engine adapter, memory store and git
integration. UI gets a thin smoke pass, not exhaustive component tests.

`pnpm gate` must be green before every commit. The pre-commit hook enforces it.

## Prior art

`.wayfinder/research/` holds two long, citation-dense documents on OpenCode's
integration surface and cowork-z's internals. Consult them before re-deriving
anything about either — they cost real effort to produce.
