---
id: 013
title: Workspaces and the permission surface
type: task
mode: AFK
status: closed
assignee: jale
blocked-by: [011]
---

## Question

Create, open, list and remove workspaces, and enforce what an agent may touch.

- A workspace binds to one directory by stable id, not by path, so a moved folder
  is repaired rather than orphaned.
- **No `Input/Output/Misc/Artefacts` convention.** Dropped deliberately — see
  [What "repo-native" actually means](007-repo-native-surface.md).
- Grants in the permission table with a source of workspace, user or adhoc; the
  workspace root is synthetic and never stored.
- The runtime prompt flow: a tool call raises a permission event, the UI asks,
  the reply persists an adhoc grant and answers the engine.
- `assetProtocol.scope` stays empty in config and is populated at runtime from
  granted folders only.
- Compile grants into the engine's permission ruleset, and remember bash is not
  covered by edit rules — gate it with `permission.bash` globs instead.

## Resolution

> **Agent-made decision.** Taken autonomously at the user's instruction.

Implemented in [`workspace.rs`](../../apps/desktop/src-tauri/src/workspace.rs) and
[`permissions.rs`](../../apps/desktop/src-tauri/src/permissions.rs), with the
webview surface in [`lib.rs`](../../apps/desktop/src-tauri/src/lib.rs). 54 Rust
tests green, gate green.

**A workspace is an id bound to a canonicalised directory.** Canonicalising on
the way in is what makes the `UNIQUE` constraint mean anything — without it
`/tmp/p` and `/private/tmp/p` are two workspaces over one folder on macOS.
Relocation is one `UPDATE`: the id never changes, so every run, grant and memory
scope that referenced the workspace still does. A directory that has gone missing
is a reported `Binding::Missing`, never an error — deleting the row because
someone unplugged a drive would take the workspace's runs and grants with it.

**Removing a workspace has to detach memory by hand.** `memory_scope.scope_id` is
polymorphic — a workspace id, a role id, or null — so it carries no foreign key
and no cascade can reach it. The migration-level test only proved the *memory row*
survived; the association was surviving too, leaving memories scoped to a
workspace that no longer existed. `remove` now deletes the scope rows and the
workspace in one transaction. This was a real bug in the load-bearing rule, found
by writing the test at the store rather than at the schema.

**The permission surface compiles into an ordered ruleset, and position is the
policy.** The session-level field on `POST /session` is a `PermissionRuleset` — a
flat `{permission, pattern, action}[]` evaluated last-match-wins — not the nested
`PermissionConfig` object the docs show for config files. Four bands:

1. **Floor** — `read **` ask, `edit **` deny, `bash *` ask.
2. **Shell baseline** — git verbs enumerated one by one (`git status*`, `diff`,
   `log`, `add`, `commit`). Deliberately *not* `git *`, which would silently
   swallow the push rule below it.
3. **Widenings** — the synthetic workspace root (**readable, never writable**,
   because a run works in its worktree), the run's worktree (writable), then
   stored grants oldest-first.
4. **Hard denies, last** — `edit **/.git`, `edit **/.git/**`, `bash git push*`
   ask, `bash rm -rf*` deny. Nothing above can outrank them.

**The workspace root grant is synthetic and never stored**, so revoking every
grant cannot lock a user out of their own project.

**Bash allowances are not storable.** The grant table holds `read` and `edit`
only. A durable "always allow `git push`" would falsify a locked decision, so the
type system refuses the whole category rather than relying on a user not clicking
*always*. Whether a *safe* shell allowance should ever be durable is
[Whether a shell allowance can ever be durable](028-durable-shell-allowance.md).

**An `always` reply is stored here and answered `once` to the engine.** Letting
OpenCode save it too would make its permission store a second authority over a
question workmate has to be able to show, audit and revoke.

**`assetProtocol.scope` stays `[]` in config** and is widened at runtime by
`workspace_open`, from the bound root and granted *directories* only. A grant
naming a single file contributes nothing — widening to its parent would expose
every sibling the user never granted.

**Known limit, ticketed:** a directory whose name contains `*`, `?` or `[`
compiles into a pattern matching more than itself —
[Escape paths that look like globs](027-escape-globs-in-patterns.md).

**Not done here, and deliberately:** nothing carries the ruleset to the engine
yet. That is the sidecar's hop, and it belongs with
[The persistence IPC](026-persistence-ipc.md) and
[Run orchestration](014-run-orchestration.md); the runtime prompt round trip
needs [The engine event stream](023-event-stream.md) to raise the event at all.
