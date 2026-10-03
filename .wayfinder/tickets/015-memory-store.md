---
id: 015
title: Memory store, remember and recall
type: task
mode: AFK
status: closed
assignee:
blocked-by: [011, 012]
---

## Question

Wire the memory layer decided in [How workmate remembers](006-persistent-memory.md).

- A `remember` tool the agent calls explicitly. No background extraction pass.
- A `recall` tool for on-demand retrieval.
- The pinned digest injected per turn via the engine's `system` field, bounded by
  `buildDigest`, and persisted on the user message so it stays auditable.
- Supersede rather than overwrite on contradiction; keep provenance and age.
- A memory panel to read, correct and delete; markdown export is read-only.
- Assert in a test that memory survives workspace deletion.

## Resolution

Memory is split along the line ticket 024 drew: Rust stores and enforces,
TypeScript decides which scopes apply and shapes what the agent sees.

- **Store (`memory.rs`, Rust).** `remember`, `list`, `recall`, `update`, `delete`
  and a read-only markdown `export`, reached by the sidecar as named operations
  and by the panel as Tauri commands. Migration v2 adds `last_used_at`;
  `recall` stamps what it returns so staleness is measurable.
- **Supersede, don't overwrite.** A new claim whose subject matches a live claim
  (case- and whitespace-insensitive) *in a shared scope* marks the old one
  `superseded_by` the new, in one transaction. Same subject in a different scope
  is not a contradiction. History stays queryable; deleting a belief deletes the
  versions it replaced, otherwise the panel would resurrect it.
- **Guards live at the writer**, so no caller can forget them: a claim that looks
  like a credential (`sk-`, `ghp_`, `-----BEGIN`, `password=` …) is refused,
  subject and claim are length-bounded ("a claim, not an archive"), and the panel's
  corrections go through the same checks. The refusal comes back to the agent as
  a readable tool error.
- **Digest.** The core domain's `buildDigest` (500-token bound) feeds a new
  `renderDigest`, returning `""` when nothing is pinned so an empty store costs
  zero tokens. `MemoryService.digestFor` is what orchestration puts in the
  per-turn `system` field; the engine persists it on the user message, so what
  memory told the agent stays auditable.
- **Tools.** The agent needs `remember` and `recall` as tools. The sidecar hosts
  a **loopback MCP endpoint** (`mcp.ts`): 127.0.0.1, ephemeral port, per-launch
  bearer token, plain JSON-RPC over POST. The caller's run, workspace and role
  are in the URL path, so a `remember` is attributable without trusting the
  model to say who it is. Orchestration points each session's MCP config at
  `urlFor(ctx)`.
- **Survives its workspace, tested:** removing a workspace detaches the scope
  rows and leaves every memory, including those also scoped elsewhere.
- **Not done here:** the memory *panel UI*. The commands behind it exist
  (`memory_list/update/delete/export`); the screen belongs to the application
  shell, which has no ticket yet and gets one.
