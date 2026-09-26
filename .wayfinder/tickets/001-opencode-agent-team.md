---
id: 001
title: Can OpenCode drive a collaborating agent team?
type: research
mode: AFK
status: closed
assignee: jack.le@zuhlke.com
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

## Resolution

**Nothing is blocked.** All three differentiators are sidecar layers built on
primitives OpenCode already exposes on a versioned, OpenAPI-generated contract
under MIT. Full findings, with citations:
[.wayfinder/research/opencode-agent-team.md](../research/opencode-agent-team.md).

**What is native.** Named agents carrying their own system prompt, model and tool
allowlist (`Agent`/`AgentConfig`). Many concurrent sessions in one process,
serialised only per session. Parent/child session trees with a structured
delegation part and a `task` tool that returns a child's result and can resume it
by id.

**What workmate must build.** The collaboration itself. Handoff carries exactly
one string — the child's last text part — never the originating session's context;
there is no peer-to-peer handoff, only parent→child, capped at `subagent_depth`
default 1. A planner→builder→reviewer round trip where each role sees what the
last one decided is workmate reading `GET /session/{id}/message` and composing the
next session's prompt parts itself.

**The memory hook is better than hoped.** `POST /session/{id}/message` takes a
per-turn `system?: string` that the request builder appends to the system prompt
for that turn only — no plugin, no restart, no experimental flag — and the
injected text is persisted on the user message, so memory stays auditable.
OpenCode has no memory *store*: `Config.instructions` is static file globs, and
`Session.projectID`/`Project.worktree` bind a session to one project by
construction. Store, retrieval policy and per-turn selection are all workmate's.

**Git is read-level only.** `GET /vcs` returns `{ branch }` and nothing more.
Sessions expose diff summaries, `GET /session/{id}/diff`, a `session.diff` event
and snapshot revert/unrevert. Branching, staging and commits happen through the
`bash` tool — so workmate gets them free at the agent level and gets no structured
control. The control it needs is adjacent: `permission.bash` takes last-match-wins
glob rules, and every request surfaces as `permission.updated` with an explicit
`once`/`always`/`reject` reply.

**Distribution is an opportunity.** OpenCode ships prebuilt per-platform binaries
under MIT, suitable for Tauri sidecar bundling. Cowork-z requires the user to
`npm i -g opencode-ai` first; workmate need not. See
[Pin the OpenCode contract and how it ships](008-opencode-contract-and-distribution.md).

**Two churn risks, not capability risks.**

1. *Session-level permissions are mid-migration.* The prompt body's `tools` field
   is deprecated in favour of permissions on the session, the runtime already
   reads `session.permission`, but the published `SessionUpdateData.body` still
   accepts only `{ title? }`. Pin a version and re-check before building on either
   side of it.
2. *Worktree isolation is `experimental_`-prefixed.* Prefer workmate-managed git
   worktrees passed as the stable `?directory=` query param over the experimental
   workspace adapter. This is the same conclusion the concurrency question reaches
   from the other direction — see
   [What "repo-native" actually means](007-repo-native-surface.md).

**Caveat on sourcing.** `sst/opencode` still serves content, but its own README
badges and Homebrew tap point at `anomalyco/opencode`. The canonical repo appears
to have moved orgs; citations use `sst` URLs, which currently resolve. Pinning
should target `anomalyco`.
