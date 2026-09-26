---
id: 001
title: Can OpenCode drive a collaborating agent team?
type: research
mode: AFK
status: open
assignee:
blocked-by: []
---

## Question

Workmate's headline differentiator is a **collaborating** agent team — named,
persistent roles (planner, builder, reviewer) that hand one task between each
other — as opposed to cowork-z's Arena, which races three models on the same
prompt. That team has to be built on top of OpenCode, which is a locked decision.

Does OpenCode expose the primitives that makes this possible, and at what level?

Specifically:

1. **Integration surface.** How is OpenCode driven programmatically from a Node
   sidecar — CLI subprocess, a server with an HTTP/WS API, a published SDK, or
   something else? What is stable and documented versus incidental?
2. **Sessions and concurrency.** Can several distinct agent sessions run at once
   against the same working directory? How is session state identified and
   resumed? What isolation exists between them?
3. **Roles and system prompts.** Can a session be given a bespoke system prompt,
   persona, or restricted tool set — the mechanism a "planner" or "reviewer" role
   would be built from? Does OpenCode already have any agent/subagent concept?
4. **Handoff.** Is there a supported way to pass one session's output (and its
   context) into another as structured input, or must workmate orchestrate that
   itself in the sidecar?
5. **Context injection.** Can workmate inject arbitrary context — the persistent
   memory layer — into a turn, and is there a hook or middleware point to do so
   per-turn rather than only at session start?
6. **Tools, permissions, MCP.** What tool-call and permission events can the host
   observe and gate? How are MCP servers configured, and can that config be set
   per-session by the host?
7. **Distribution.** Is OpenCode a global npm binary the user must install
   themselves (as cowork-z requires), or can it be vendored/bundled into a Tauri
   app? What are the licence terms for bundling?

Answer with a verdict: **which of the three differentiators OpenCode supports
natively, which workmate must build in the sidecar above it, and which are
blocked** — plus the citations to back each.
