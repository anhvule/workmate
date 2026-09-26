---
id: 015
title: Memory store, remember and recall
type: task
mode: AFK
status: open
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
