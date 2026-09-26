---
ticket: 001
title: Can OpenCode drive a collaborating agent team?
type: research-findings
date: 2026-09-26
---

# OpenCode as the engine for a collaborating agent team

Primary sources only: the OpenCode docs site (`opencode.ai/docs`), the repo source
on the `dev` branch, and the published npm registry metadata. Source files were
fetched from `https://raw.githubusercontent.com/sst/opencode/dev/<path>`; line
numbers below refer to those files as of 2026-09-26.

> Note on repo identity: `sst/opencode` still serves content, but the README's own
> badges point at `github.com/anomalyco/opencode` — the canonical repo has moved to
> the `anomalyco` org. Raw fetches against `sst/opencode` currently follow through.
> Source: [README.md badges](https://raw.githubusercontent.com/sst/opencode/dev/README.md).

---

## 1. Integration surface

**There is a first-class, documented HTTP server with a generated TypeScript SDK.
This is the stable surface. The CLI is not the integration point.**

`opencode serve` starts an HTTP server:

- Flags: `--port` (default `4096`), `--hostname` (default `127.0.0.1`), `--mdns`,
  `--cors <origin>`.
- HTTP basic auth via `OPENCODE_SERVER_PASSWORD` / `OPENCODE_SERVER_USERNAME`.
- The server publishes its own **OpenAPI 3.1 spec at `GET /doc`**.

Source: <https://opencode.ai/docs/server/>

The SDK is published as **`@opencode-ai/sdk`** and its types are *generated from
that OpenAPI spec* — so the type file is the authoritative description of the wire
protocol, not a hand-written wrapper.

```js
// spawn server + client together
import { createOpencode } from "@opencode-ai/sdk"
const { client } = await createOpencode()

// or attach to an already-running server
import { createOpencodeClient } from "@opencode-ai/sdk"
const client = createOpencodeClient({ baseUrl: "http://localhost:4096" })
```

`createOpencode()` accepts `hostname`, `port`, `signal`, `timeout`, and `config`.
Namespaces: `global.*`, `app.*`, `project.*`, `session.*`, `file.*`, `find.*`,
`tui.*`, `auth.*`, `event.*`.
Source: <https://opencode.ai/docs/sdk/>

### Full route list

Extracted from the generated SDK types
(`packages/sdk/js/src/gen/types.gen.ts`, every `url:` literal):

```
/agent                          /project
/auth/{id}                      /project/current
/command                        /provider
/config                         /provider/auth
/config/providers               /provider/{id}/oauth/authorize
/event                          /provider/{id}/oauth/callback
/experimental/tool              /pty
/experimental/tool/ids          /pty/{id}
/file  /file/content            /pty/{id}/connect
/file/status                    /session
/find  /find/file               /session/status
/find/symbol                    /session/{id}
/formatter                      /session/{id}/abort
/global/event                   /session/{id}/children
/instance/dispose               /session/{id}/command
/log                            /session/{id}/diff
/lsp                            /session/{id}/fork
/mcp                            /session/{id}/init
/mcp/{name}/auth                /session/{id}/message
/mcp/{name}/auth/authenticate   /session/{id}/message/{messageID}
/mcp/{name}/auth/callback       /session/{id}/permissions/{permissionID}
/mcp/{name}/connect             /session/{id}/prompt_async
/mcp/{name}/disconnect          /session/{id}/revert
/path                           /session/{id}/share
                                /session/{id}/shell
                                /session/{id}/summarize
                                /session/{id}/todo
                                /session/{id}/unrevert
/tui/* (10 routes)              /vcs
```

**Stable vs incidental.** Everything above is in the generated SDK and the OpenAPI
spec, so it is a published contract. Two caveats visible in the source:

- Routes under `/experimental/*` and plugin hooks prefixed `experimental.` are
  explicitly marked as such (see §5).
- The `tools` field on the prompt body is annotated
  `"@deprecated tools and permissions have been merged, you can set permissions
  on the session itself now"` —
  `packages/opencode/src/session/prompt.ts:1506-1509`. The session-level permission
  field is present in the runtime source (`session.permission`, used at
  `prompt.ts:1261` and `prompt.ts:1276`) but **not yet in the published
  `types.gen.ts`** — `SessionUpdateData.body` there only accepts `{ title? }`
  (`types.gen.ts:2207`). This part of the surface is actively moving.
- The `/tui/*` routes drive OpenCode's own terminal UI and are irrelevant to a
  Tauri host.

**Verdict on §1:** drive OpenCode by spawning `opencode serve` as a sidecar child
process and speaking to it with `@opencode-ai/sdk`. Do not shell out to the CLI
per turn.

---

## 2. Sessions and concurrency

**Yes — multiple sessions run concurrently in one server process against the same
working directory. Busy-ness is tracked per session, not per server.**

The run-state service keeps a map keyed by session:

```ts
const runners = new Map<SessionID, Runner.Runner<SessionV1.WithParts>>()
...
const assertNotBusy = Effect.fn("SessionRunState.assertNotBusy")(function* (sessionID: SessionID) {
  const existing = data.runners.get(sessionID)
  if (existing?.busy) yield* busyError(sessionID)
})
```
`packages/opencode/src/session/run-state.ts:38,71-75`

So the only serialisation is *within* one session: a second prompt to a busy
session fails with `BusyError`. Different session IDs run in parallel. This is
corroborated on the wire by `GET /session/status`, whose 200 response is a **map**
of many session IDs to statuses:

```ts
export type SessionStatusResponses = {
  200: { [key: string]: SessionStatus }
}
export type SessionStatus =
  | { type: "idle" }
  | { type: "retry"; attempt: number; message: string; next: number }
  | { type: "busy" }
```
`types.gen.ts:453-465, 2130-2139`

and by `POST /session/{id}/prompt_async`, which returns 204 immediately so the host
can fan several prompts out and watch `/event`
(<https://opencode.ai/docs/server/>).

### Identity and resumption

```ts
export type Session = {
  id: string
  projectID: string
  directory: string
  parentID?: string
  summary?: { additions: number; deletions: number; files: number; diffs?: Array<FileDiff> }
  share?: { url: string }
  title: string
  version: string
  time: { created: number; updated: number; compacting?: number }
  revert?: { messageID: string; partID?: string; snapshot?: string; diff?: string }
}
```
`types.gen.ts:533-560`

Sessions are server-persisted and addressable by `id`. Resume = `GET /session/{id}`
plus `GET /session/{id}/message`; there is no separate "resume" call, because the
session never went away. `POST /session` takes `{ parentID?, title? }` with a
`?directory=` query param (`types.gen.ts:2082-2092`), so a session is *created
against* a directory and carries it.

### Isolation

- **Conversation isolation: real.** Separate message histories, separate
  busy/abort state, separate `parentID` trees (`/session/{id}/children`).
- **Filesystem isolation: none by default.** Two sessions with the same
  `directory` write to the same files. Nothing in the session model prevents a
  planner and a builder stepping on each other.
- **There is an experimental escape hatch.** The plugin API exposes a workspace
  adapter with a `branch` and its own `directory`:

  ```ts
  export type WorkspaceInfo = {
    id: string; type: string; name: string
    branch: string | null
    directory: string | null
    extra: unknown | null
    projectID: string
  }
  export type PluginInput = {
    ...
    experimental_workspace: { register(type: string, adapter: WorkspaceAdapter): void }
  }
  ```
  `packages/plugin/src/index.ts` (`WorkspaceInfo`, `WorkspaceAdapter`, `PluginInput`)

  This is the git-worktree-per-agent primitive, and it is flagged
  `experimental_`. Building on it is a bet.

**Verdict on §2:** concurrency is native. Worktree isolation for a parallel team is
not — workmate either creates worktrees itself and points each session at a
different `directory`, or takes the experimental workspace-adapter bet.

---

## 3. Roles and system prompts

**OpenCode already has a full agent/subagent concept, and it is exactly the shape a
planner/builder/reviewer team needs.**

Agents are defined as markdown files in `~/.config/opencode/agents/` (global) or
`.opencode/agents/` (per project), or as JSON under `agent` in `opencode.json`.
Frontmatter/JSON fields: `description` (required), `mode`, `model`, `temperature`,
`permission`, and the markdown body becomes the prompt.
Source: <https://opencode.ai/docs/agents/>

The runtime type:

```ts
export type Agent = {
  name: string
  description?: string
  mode: "subagent" | "primary" | "all"
  builtIn: boolean
  topP?: number
  temperature?: number
  color?: string
  permission: {
    edit: "ask" | "allow" | "deny"
    bash: { [key: string]: "ask" | "allow" | "deny" }
    webfetch?: "ask" | "allow" | "deny"
    doom_loop?: "ask" | "allow" | "deny"
    external_directory?: "ask" | "allow" | "deny"
  }
  model?: { modelID: string; providerID: string }
  prompt?: string
  tools: { [key: string]: boolean }
  options: { [key: string]: unknown }
  maxSteps?: number
}
```
`types.gen.ts:1585-1613`

and the config-side `AgentConfig` mirrors it with `model`, `temperature`, `top_p`,
`prompt`, `tools`, `disable`, `description`, `mode`, `color`, `maxSteps`,
`permission` (`types.gen.ts:975-1007`). Agents are readable over the wire at
`GET /agent`.

**A custom `prompt` replaces the built-in system prompt outright** — it does not
merely prepend. From the request builder:

```ts
const system = [
  [
    ...(input.agent.prompt ? [input.agent.prompt] : SystemPrompt.provider(input.model)),
    ...input.system,
    ...(input.user.system ? [input.user.system] : []),
  ]
    .filter((x) => x)
    .join("\n"),
]
```
`packages/opencode/src/session/llm/request.ts:56-66`

This single expression answers §3 and §5 together: the agent's prompt is the base,
the per-turn `system` string lands after it, and both are appended to the
environment/instruction blocks assembled in
`packages/opencode/src/session/prompt.ts:1257-1271`.

Built-ins: primary agents `Build` (full tools) and `Plan` (no edits/bash);
subagents `General`, `Explore` (read-only codebase), `Scout` (read-only dependency
research). Source: <https://opencode.ai/docs/agents/>

Tool restriction per role is `tools: { [name]: boolean }` on the agent, plus the
`permission` block. Restricting *which subagents a role may call* is
`permission.task` with glob patterns (<https://opencode.ai/docs/agents/>).

**Verdict on §3:** native. A workmate "planner"/"reviewer" is an OpenCode agent
definition (markdown or JSON), not something to invent.

---

## 4. Handoff

**There is a supported, structured handoff — and it is parent→child, not
peer→peer.**

Two mechanisms, both first-class:

### a) The `task` tool (model-initiated)

`packages/opencode/src/tool/task.ts` — parameters:

```ts
description: "A short (3-5 words) description of the task"
prompt: "The task for the agent to perform"
subagent_type: "The type of specialized agent to use for this task"
task_id: "... you can pass a prior task_id and the task will continue the same
          subagent session as before instead of creating a fresh one"
background: "Run the agent in the background. You will be notified when it completes."
```
`task.ts:44-60`

It creates a **child session** with `parentID: ctx.sessionID` and a title
`params.description + " (@" + next.name + " subagent)"` (`task.ts:159-161`), runs
the named agent in it, and returns the child's last text part as the tool result:

```ts
return result.parts.findLast((item) => item.type === "text")?.text ?? ""
```
`task.ts:224`

Failures propagate as `Subagent failed (task_id: <sessionID>): <message>`
(`task.ts:218,222`). Background mode is gated behind
`OPENCODE_EXPERIMENTAL_BACKGROUND_SUBAGENTS=true` (`task.ts:98-101`), and nesting
depth is capped by config `subagent_depth`, default `1` (`task.ts:111-114`).

### b) `SubtaskPartInput` (host-initiated)

The host can put a subtask directly into a prompt body — no model decision
involved:

```ts
export type SubtaskPartInput = {
  id?: string
  type: "subtask"
  prompt: string
  description: string
  agent: string
}
```
`types.gen.ts:1439-1445`, accepted in `SessionPromptData.body.parts`
(`types.gen.ts:2588-2601`) and handled by
`SessionPrompt.handleSubtask` (`prompt.ts:255,1144-1145`).

### What this gives, and what it does not

Native: **parent delegates to a named child agent, gets its final text back, and
can resume that same child later via `task_id`.** The child's full transcript stays
addressable (`GET /session/{id}/children`, `GET /session/{id}/message`).

Not native: a **peer handoff with context** — "planner finishes, builder starts
with the planner's reasoning in its window". What crosses the boundary is *one
string* (`result.parts.findLast(type === "text").text`), not the child's context.
The child's own context is built fresh from its agent prompt plus the prompt text
it was given.

**Verdict on §4:** workmate orchestrates the team graph in the sidecar. It reads
session A's messages via `GET /session/{id}/message` and composes them into session
B's prompt parts itself. OpenCode gives the plumbing (sessions, parentage, named
agents, structured subtask parts) but not the choreography.

---

## 5. Context injection

**Per-turn injection is native and does not require a plugin.**

Three layers, in increasing order of invasiveness:

### a) Per-turn `system` on the prompt body — the cheapest hook

```ts
export type SessionPromptData = {
  body?: {
    messageID?: string
    model?: { providerID: string; modelID: string }
    agent?: string
    noReply?: boolean
    system?: string
    tools?: { [key: string]: boolean }   // deprecated
    parts: Array<TextPartInput | FilePartInput | AgentPartInput | SubtaskPartInput>
  }
  path: { id: string }
  query?: { directory?: string }
  url: "/session/{id}/message"
}
```
`types.gen.ts:2588-2611`

That `system` string is appended to the system prompt **for that turn only** —
see the `request.ts:56-66` expression quoted in §3 (`...input.user.system`). It is
also persisted on the user message (`prompt.ts:668`), so the injected memory is
part of the auditable transcript.

This is the memory hook. Every turn, workmate can compute what the agent should
remember and pass it as `system` — no plugin, no config file, no restart.

### b) Synthetic parts

`TextPartInput` carries `synthetic?: boolean`, `ignored?: boolean` and a free-form
`metadata?: { [key: string]: unknown }` (`types.gen.ts:1404-1417`), plus
`noReply?: boolean` on the body — i.e. workmate can push a message into a session
without triggering a model turn.

### c) Plugin hooks (in-process, some experimental)

`packages/plugin/src/index.ts`, `interface Hooks`:

```ts
"chat.message"?: (input: { sessionID; agent?; model?; messageID?; variant? },
                  output: { message: UserMessage; parts: Part[] }) => Promise<void>
"chat.params"?:  (input: { sessionID; agent; model; provider; message },
                  output: { temperature; topP; topK; maxOutputTokens; options }) => Promise<void>
"chat.headers"?: (...) => Promise<void>
"experimental.chat.messages.transform"?: (input: {},
                  output: { messages: { info: Message; parts: Part[] }[] }) => Promise<void>
"experimental.chat.system.transform"?: (input: { sessionID?; model },
                  output: { system: string[] }) => Promise<void>
"experimental.session.compacting"?: (input: { sessionID },
                  output: { context: string[]; prompt?: string }) => Promise<void>
```

`experimental.chat.system.transform` is fired inside the prepare step with the
assembled array mutable (`request.ts:69-73`), and
`experimental.chat.messages.transform` fires on the full message list each step
(`prompt.ts:1255`). Both are `experimental.`-prefixed. `chat.message` and
`chat.params` are not.

Note `experimental.session.compacting` — it lets a host inject context into the
compaction prompt, which matters for a memory layer that wants to survive long
sessions.

**Verdict on §5:** native, per-turn, via `body.system`. The plugin hooks are a
deeper fallback if memory needs to rewrite history rather than append to the system
prompt — but the stable path does not need them.

---

## 6. Tools, permissions, MCP

### Permission gating from the host

Fully supported over HTTP. A permission request is published as an event and
answered by a route:

```ts
export type Permission = {
  id: string
  type: string
  pattern?: string | Array<string>
  sessionID: string
  messageID: string
  callID?: string
  title: string
  metadata: { [key: string]: unknown }
  time: { created: number }
}

export type EventPermissionUpdated = { type: "permission.updated"; properties: Permission }
export type EventPermissionReplied = {
  type: "permission.replied"
  properties: { sessionID: string; permissionID: string; response: string }
}
```
`types.gen.ts:423-452`

```ts
export type PostSessionIdPermissionsPermissionIdData = {
  body?: { response: "once" | "always" | "reject" }
  path: { id: string; permissionID: string }
  query?: { directory?: string }
  url: "/session/{id}/permissions/{permissionID}"
}
```
`types.gen.ts:2888-2901`

So the Tauri UI subscribes to `GET /event` (SSE, opens with `server.connected`,
then bus events — <https://opencode.ai/docs/server/>), renders the
`permission.updated` payload, and POSTs `once` / `always` / `reject`. This is
exactly the folder-level-permission UX the map calls for.

Permission keys: `read`, `edit`, `glob`, `grep`, `bash`, `task`, `skill`, `lsp`,
`question`, `webfetch`, `websearch`, `external_directory`, `doom_loop`; values
`allow` / `ask` / `deny`; bash supports pattern objects with last-match-wins:

```json
{ "permission": { "bash": { "*": "ask", "git *": "allow", "rm *": "deny" } } }
```
<https://opencode.ai/docs/permissions/>

Agent-level permissions override global. Same source.

### Tool-call observation

`tool.execute.before(input: { tool, sessionID, callID }, output: { args })`,
`tool.execute.after(input: { tool, sessionID, callID, args }, output: { title, output, metadata })`,
`tool.definition(input: { toolID }, output: { description, parameters })`,
and `permission.ask(input: Permission, output: { status: "ask"|"deny"|"allow" })`
— all non-experimental, all in `packages/plugin/src/index.ts` `interface Hooks`.
`permission.ask` lets a plugin answer a permission *programmatically* rather than
round-tripping to the UI.

Note these are **plugin** hooks: they run in-process inside OpenCode, not over
HTTP. Gating from the Tauri side uses the event + POST pair above; gating with
policy logic uses a plugin workmate ships alongside the sidecar.

### MCP

Configured under `mcp` in the OpenCode config:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "my-local-mcp-server": {
      "type": "local",
      "command": ["npx", "-y", "my-mcp-command"],
      "enabled": true,
      "environment": { "MY_ENV_VAR": "value" }
    }
  }
}
```
<https://opencode.ai/docs/mcp-servers/>; types at `types.gen.ts:1101-1124`
(`McpLocalConfig`) and `1141-1173` (`McpRemoteConfig`, with `url`, `headers`,
`oauth`, `timeout`).

**Per-session MCP config: no. Per-agent MCP tool scoping: yes.** The documented
pattern disables an MCP globally and re-enables it for one agent:

```json
{
  "mcp": { "my-mcp": { "type": "local", "command": ["bun", "x", "my-mcp-command"], "enabled": true } },
  "tools": { "my-mcp*": false },
  "agent": { "my-agent": { "tools": { "my-mcp*": true } } }
}
```
<https://opencode.ai/docs/mcp-servers/>

Config in `Config.mcp` is a flat record scoped to the server/directory
(`types.gen.ts:1287-1289`), and `Config.agent` is a record of `AgentConfig`
(`types.gen.ts:1271-1277`). Runtime control exists at the *server* level —
`POST /mcp/{name}/connect`, `POST /mcp/{name}/disconnect`, `GET /mcp` — and the
whole config can be replaced at runtime:

```ts
export type ConfigUpdateData = {
  body?: Config
  path?: never
  query?: { directory?: string }
  url: "/config"
}
```
`types.gen.ts:1927-1934`

Plus `createOpencode({ config })` seeds config at server start
(<https://opencode.ai/docs/sdk/>).

**Verdict on §6:** permission observation and gating are native and well-shaped for
a desktop host. MCP is configured per *server/directory* and scoped per *agent* —
so "per-session MCP" is achievable by mapping each workmate role to an OpenCode
agent with its own `tools` allowlist, **not** by varying config per session ID.

---

## 7. Distribution

**MIT-licensed, distributed as platform-native binaries. Bundling into a Tauri app
is permitted and mechanically straightforward.**

License — `LICENSE` on the `dev` branch:

```
MIT License

Copyright (c) 2025 opencode

Permission is hereby granted, free of charge, to any person obtaining a copy
```
<https://raw.githubusercontent.com/sst/opencode/dev/LICENSE>

npm registry metadata for `opencode-ai@1.18.32`
(<https://registry.npmjs.org/opencode-ai/latest>):

- `"license": "MIT"`
- `"bin": { "opencode": "bin/opencode.exe" }`
- `"os": ["darwin", "linux", "win32"]`, `"cpu": ["arm64", "x64"]`
- `"dependencies": null` — **no runtime npm dependencies**
- `optionalDependencies`: twelve per-platform binary packages —
  `opencode-darwin-arm64`, `opencode-darwin-x64`, `opencode-linux-x64`,
  `opencode-linux-arm64`, `-musl` and `-baseline` variants,
  `opencode-windows-x64`, `opencode-windows-arm64`
- `"scripts": { "postinstall": "node ./postinstall.mjs" }`

The wrapper package selects a prebuilt native binary at install time. That is the
ideal shape for Tauri: workmate can download or vendor
`opencode-darwin-arm64` / `opencode-darwin-x64` and ship them as Tauri
**sidecar binaries** (`externalBin`), so **the user does not need a global npm
install** — unlike cowork-z's requirement.

Install methods (README, "Installation"):

```bash
curl -fsSL https://opencode.ai/install | bash
npm i -g opencode-ai@latest        # or bun/pnpm/yarn
scoop install opencode             # Windows
choco install opencode             # Windows
brew install anomalyco/tap/opencode # macOS and Linux (recommended, always up to date)
brew install opencode              # official brew formula, updated less
sudo pacman -S opencode            # Arch Linux (Stable)
paru -S opencode-bin               # Arch Linux (AUR)
mise use -g opencode               # Any OS
nix run nixpkgs#opencode
```
<https://raw.githubusercontent.com/sst/opencode/dev/README.md>

MIT requires only that the copyright notice and permission notice be included in
distributions — i.e. workmate must ship OpenCode's LICENSE text in its
about/licences surface. There is no copyleft, no attribution-in-UI clause, no
commercial restriction.

**Two open items:**

- **Trademark.** The MIT licence covers code, not the "OpenCode" name/logo. There
  is no `TRADEMARK.md` in the repo root as fetched. *Unknown* — would be settled by
  checking for a trademark policy page or asking the maintainers before using the
  name in workmate's UI. Using the binary is unambiguous; calling it "OpenCode" in
  marketing copy is the unclear part.
- **Code signing / notarization.** A vendored third-party binary inside a macOS
  `.app` must be signed and hardened-runtime-notarized with workmate's certificate.
  Not an OpenCode fact; flagged because the map lists packaging as unspecified.

---

## Cross-cutting: what git support OpenCode has

Relevant to the third differentiator, so worth stating explicitly. The entire VCS
surface is:

```ts
export type VcsInfo = { branch: string }

export type VcsGetData = {
  body?: never; path?: never
  query?: { directory?: string }
  url: "/vcs"
}
```
`types.gen.ts:1400-1402, 2046-2053`

and on the project:

```ts
export type Project = {
  id: string
  worktree: string
  vcsDir?: string
  vcs?: "git"
  time: { created: number; initialized?: number }
}
```
`types.gen.ts:743-752`

There is also per-session diff tracking — `GET /session/{id}/diff`,
`Session.summary.{additions,deletions,files,diffs}`, the `session.diff` event
(`types.gen.ts:583-589`), and snapshot-based revert/unrevert
(`Session.revert.snapshot`, `POST /session/{id}/revert`,
`POST /session/{id}/unrevert`).

So OpenCode **knows** the current branch and **tracks** what a session changed, and
can roll a session back. It does not branch, stage, commit, or merge — the agent
does that through the `bash` tool like any other command.

---

## Verdict

### Collaborating agent team — **build it in the sidecar above OpenCode**

The primitives are all native and unusually well-matched: named agents with bespoke
system prompts, models and tool allowlists
(`Agent`/`AgentConfig`, `types.gen.ts:1585-1613`, `975-1007`); many concurrent
sessions in one process, serialised only per session
(`run-state.ts:38,71-75`); parent/child session trees with a structured
delegation part (`SubtaskPartInput`, `types.gen.ts:1439-1445`) and a `task` tool
that returns the child's result and can resume it by `task_id`
(`task.ts:44-60, 159-161, 224`).

What is *not* native is the collaboration itself. Handoff carries exactly one
string — `result.parts.findLast((item) => item.type === "text")?.text ?? ""`
(`task.ts:224`) — never the originating session's context. There is no peer-to-peer
handoff; the only relationship is parent→child, capped at `subagent_depth`
default 1 (`task.ts:111-114`). A planner→builder→reviewer round trip, where each
role sees what the last one decided, is workmate reading
`GET /session/{id}/message` and composing session B's prompt parts itself.

That is a sidecar orchestration layer, not an OpenCode change. Nothing about
OpenCode's design fights it — which is the point of the differentiator being
"a layer above the engine".

One caveat for a *parallel* team: sessions sharing a `directory` share the
filesystem, and the only isolation primitive is the `experimental_workspace`
adapter with its `branch`/`directory` fields (`packages/plugin/src/index.ts`,
`WorkspaceInfo`/`WorkspaceAdapter`). If workmate's roles run concurrently rather
than in sequence, workmate should create the git worktrees itself and pass each
session a different `?directory=` (`types.gen.ts:2082-2092`) rather than depend on
an `experimental_`-prefixed API.

### Persistent cross-workspace memory — **build it in the sidecar above OpenCode**

The injection point is native and stable. `POST /session/{id}/message` accepts a
per-turn `system?: string` (`types.gen.ts:2588-2601`), and the request builder
appends it to the system prompt for that turn only:

```ts
const system = [[
  ...(input.agent.prompt ? [input.agent.prompt] : SystemPrompt.provider(input.model)),
  ...input.system,
  ...(input.user.system ? [input.user.system] : []),
].filter((x) => x).join("\n")]
```
`packages/opencode/src/session/llm/request.ts:56-66`

That is a per-turn hook with no plugin, no restart, and no experimental flag — and
the injected text is persisted on the user message (`prompt.ts:668`), so memory
stays auditable. Deeper rewriting is available via
`experimental.chat.system.transform` / `experimental.chat.messages.transform`
(`packages/plugin/src/index.ts`) if ever needed.

What OpenCode does not have is the memory itself. `Config.instructions` is a list
of file globs (`types.gen.ts:1324`) — static, session-start, per-directory. There
is no store, no retrieval, no cross-workspace scope; `Session.projectID` and
`Project.worktree` (`types.gen.ts:533-535, 743-746`) bind a session to one project
by construction. The store, the retrieval policy and the per-turn selection are
all workmate's.

### Repo-native git workflow — **build it in the sidecar above OpenCode**

OpenCode gives read-level git awareness and change tracking, not workflow:
`GET /vcs` returns `{ branch: string }` and nothing else
(`types.gen.ts:1400-1402, 2046-2053`); `Project` carries `vcs?: "git"`,
`vcsDir?`, `worktree` (`types.gen.ts:743-752`); sessions expose
`summary.{additions,deletions,files,diffs}`, `GET /session/{id}/diff`, a
`session.diff` event (`types.gen.ts:583-589`), and snapshot revert/unrevert
(`Session.revert.snapshot`, `/session/{id}/revert`, `/session/{id}/unrevert`).

Branching, staging, conventional commits and merges happen through the `bash`
tool, which means workmate gets them for free at the agent level but gets no
structured control. The good news is that the control it needs is right there:
`permission.bash` takes last-match-wins glob rules
(`{"bash": {"*": "ask", "git *": "allow", "rm *": "deny"}}`,
<https://opencode.ai/docs/permissions/>), and every permission request surfaces as
`permission.updated` on `GET /event` with a `POST /session/{id}/permissions/{permissionID}`
reply of `once`/`always`/`reject` (`types.gen.ts:423-452, 2888-2901`). So workmate
can gate, observe and drive the git workflow precisely — it just has to implement
it.

### Nothing is blocked

No differentiator is blocked by OpenCode's design. Each is a sidecar layer over
primitives OpenCode already exposes on a versioned, OpenAPI-generated contract
(<https://opencode.ai/docs/server/>, `/doc`), under MIT
(<https://raw.githubusercontent.com/sst/opencode/dev/LICENSE>), with prebuilt
per-platform binaries suitable for Tauri sidecar bundling
(<https://registry.npmjs.org/opencode-ai/latest>).

The two risks worth carrying forward into implementation tickets are both
churn risks, not capability risks:

1. **Session-level permissions are mid-migration.** The prompt body's `tools` field
   is annotated `"@deprecated tools and permissions have been merged, you can set
   permissions on the session itself now"` (`prompt.ts:1506-1509`), the runtime
   already reads `session.permission` (`prompt.ts:1261,1276`), but the published
   `SessionUpdateData.body` still only accepts `{ title? }`
   (`types.gen.ts:2207-2218`). Pin an OpenCode version and re-check this surface
   before building on either side of it.
2. **Worktree isolation is `experimental_`-prefixed** (`packages/plugin/src/index.ts`).
   Prefer workmate-managed worktrees plus the stable `?directory=` query param.
