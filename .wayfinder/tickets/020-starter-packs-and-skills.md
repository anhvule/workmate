---
id: 020
title: Starter packs and the skills catalog
type: task
mode: AFK
status: closed
assignee:
blocked-by: [013]
---

## Question

Reduce setup friction, without copying cowork-z's content.

- Starter packs as workmate's own file trees — cowork-z's corpora have unclear
  provenance and must not be copied, per
  [Cowork-z architecture up close](002-cowork-z-architecture.md) §9.
- Skills: discovery is the engine's; git sync, frontmatter parsing, checksums and
  the curated repo list are workmate's.
- Decide whether a starter pack can ship a **team** — the obvious workmate-native
  extension, since a pack that configures a planner/builder/reviewer is worth far
  more than one that configures folders.

## Resolution

**A pack can ship a team — and does.** That is the point of it. A pack is a
directory with a `pack.json` (id, description, a team of roles, files to write)
plus the files. Three ship, all written for workmate and none copied from
cowork-z, whose corpora have unclear provenance:

- `solo` — one Builder. The plain-chat pack, and the proof the degenerate case is
  first-class.
- `plan-build-review` — Planner and Reviewer are read-only by allowlist, Builder
  is unrestricted. The pack encodes the idea instead of describing it.
- `docs-pair` — Writer and Editor.

**Teams needed real CRUD first** (`team.rs`; there was none): roles are data
(prompt, model, allowlist), ordered into a team; editing a role changes it for
every team that uses it; deleting a team keeps its roles because past runs'
sessions point at them. Applying a pack namespaces role ids (`<pack>.<role>`), so
two packs' `builder`s never collide.

**Applying a pack writes into the user's checkout** — the one place agents may
not — so it is the user's explicit action, previewable (`pack_preview` lists what
it will write and skip), and *never overwrites*: an existing file is skipped and
reported. Every path is validated before the first write (a hostile pack writes
nothing), `.git` and `..` are refused, a symlinked directory cannot carry a write
out of the project, and `create_new` closes the check-then-write race.

**Skills: discovery stays the engine's** (`.opencode/skills/<name>/SKILL.md`,
confirmed in the pinned binary). Workmate owns the rest:

- *Frontmatter* parsed by hand (`name` as a slug, `description` ≤ 1024) rather than
  pulling in a YAML parser for untrusted input; a skill whose name does not match
  its folder is skipped.
- *Checksum* — SHA-256 over every file's path and bytes. Recorded at install, so a
  skill the user (or an agent) edited is detected: `update` and `uninstall` refuse
  a modified skill unless `force`.
- *Git sync* by shelling out to `git` — the one place workmate does, because it
  is the only way to inherit the user's credentials and ssh config (ticket 007).
  `GIT_TERMINAL_PROMPT=0` so a credential prompt can never hang a background
  sync; `--` before the URL; sources limited to `https://`, `ssh://`, `git@` or an
  absolute local path, and never a leading `-`. Tested against a real local repo,
  including picking up a later commit.
- *The curated list* is the bundled `skills/` (two, workmate-authored) plus
  whatever sources the user adds. No remote list is baked in: shipping a URL we do
  not control into everyone's catalog is a supply-chain decision, and the user
  can add any source in one call.
- *The webview never names a path.* Install is by `name` + `origin` from the
  catalog.

Packs and skills are bundled as app resources, with the source tree as the
fallback under `tauri dev`.

Not built: remote pack registries, and signature checking of synced skills — a
checksum detects change after install, it does not vouch for the source. A skill
is instructions an agent will follow, so adding a source is a trust decision the
UI must say out loud.
