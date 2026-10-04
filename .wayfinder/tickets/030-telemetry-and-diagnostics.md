---
id: 030
title: Telemetry and error reporting
type: task
mode: AFK
status: closed
assignee: claude
blocked-by: []
---

## Question

Graduated from the map's *Not yet specified*: ship telemetry and error
reporting, or decide deliberately to ship without.

## Resolution

> **Agent-made decision.** Cheap to overturn; nothing here prevents adding an
> opt-in reporter later.

**No telemetry, no remote error reporting.** workmate is local-first and holds a
user's code, prompts and memory. A crash report from a coding agent tends to
carry exactly the content a user would not want to leave the machine, and
scrubbing it reliably is a project of its own. Shipping nothing remote is the
only version of this that needs no trust. The Settings screen says so in one
sentence.

**But real local logs, because a user with a problem needs something to open.**
`logs.rs` writes rotating files (2 MiB, one previous kept) to the OS log
directory (`~/Library/Logs/dev.workmate.app` on macOS): workmate's own warnings,
and everything the engine and sidecar print. Settings → Diagnostics → *Show
logs* reveals the folder.

**This fixed a real bug.** After startup the engine's stdout reader was dropped
and its stderr pipe was never read. A pipe nobody reads fills at ~64 KiB and the
child blocks on its next write, so a long session with enough engine logging
would have hung mid-run. Both pipes are now drained for the engine's whole life;
a test runs a child that writes far past a pipe buffer and asserts it finishes.
The sidecar's stderr, previously inherited (and so lost in a bundled app), goes
to its own log.

**Limit:** logs are local plain text and may contain whatever the engine prints,
which can include paths and prompt fragments. They never leave the machine
unless the user sends them.
