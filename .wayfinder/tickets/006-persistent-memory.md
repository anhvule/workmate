---
id: 006
title: How workmate remembers
type: grilling
mode: HITL
status: closed
assignee: agent (autonomous mode)
blocked-by: [003]
---

## Question

Cowork-z isolates each workspace precisely so context does not bleed between
tasks. Workmate's second differentiator deliberately breaks that isolation — so
it has to break it in a way that is inspectable and controlled, or it becomes the
bug cowork-z was avoiding.

Decide:

- **What is worth remembering.** Facts about a project, user preferences,
  decisions made, conventions inferred from a repo, outcomes of past sessions —
  which of these, and what is explicitly *not* remembered.
- **Scope.** Is a memory bound to a workspace, a repo, a role, or the user
  globally? Can one memory be visible in several scopes, and what wins on
  conflict?
- **Capture.** Written by the agent as it works, extracted after a session, or
  only ever written when the user says so. What stops it filling with noise.
- **Retrieval.** How memories reach a turn given OpenCode's injection points (see
  [Can OpenCode drive a collaborating agent team?](001-opencode-agent-team.md)) —
  everything relevant every turn, retrieved on demand as a tool, or selected by
  the user. What the token cost of the chosen approach is per turn.
- **Inspection and editing.** The user must be able to see, correct and delete
  what workmate believes. What that surface is, and whether memory is a file on
  disk the user can edit directly.
- **Staleness and trust.** What happens when a remembered fact stops being true,
  and how a memory carries its provenance and age.
- **Privacy.** Local-first is a product promise — state where memory lives and
  confirm nothing about it leaves the machine except inside a model call the user
  initiated.

## Known before starting

[Can OpenCode drive a collaborating agent team?](001-opencode-agent-team.md)
settled the mechanism: `POST /session/{id}/message` accepts a per-turn
`system?: string` appended to the system prompt for that turn only — no plugin, no
restart, no experimental flag — and the injected text is persisted on the user
message, so what memory said is auditable after the fact. The store, the retrieval
policy and the per-turn selection are entirely workmate's; OpenCode has no memory
concept and binds a session to one project by construction.

Retention of the transcripts memory is extracted *from* is decided in
[Who owns the conversation](010-conversation-source-of-truth.md).

## Resolution

> **Agent-made decision.** Taken autonomously at the user's instruction.

**What is remembered:** project facts, inferred conventions, decisions and their
rationale, and user preferences. **Never remembered:** secrets or credentials,
verbatim file contents, and raw transcript text — memory is a claim, not an
archive.

**Scope by association, never by ownership.** A memory row lives in its own table;
a separate scope table associates it with a workspace, a repo, a role, or global.
Deleting a workspace deletes scope rows and never cascades into memory. On
conflict the most specific scope wins; ties break to the newest.

**Capture is an explicit tool call.** The agent writes memory by calling a
`remember` tool — no silent background extraction pass. This is the noise control:
it costs the agent a visible tool call, it appears in the transcript, and it is
attributable to a turn. An extraction pass that quietly mines every session is how
a memory store fills with garbage nobody can trace.

**Retrieval is two-tier, and the token cost is bounded by design.** A small pinned
digest is injected into every turn via OpenCode's per-turn `system` field, capped
at roughly 500 tokens; everything else is fetched on demand through a `recall`
tool. Injection is the right mechanism because the injected text is persisted on
the user message, so what memory told the agent is auditable after the fact.

**SQLite is canonical; the markdown view is a read-only export.** Tempting as an
editable file is, two writers over one store is precisely the bug cowork-z hit by
persisting transcripts twice. Editing happens in the memory panel.

**Staleness.** Each memory carries provenance — the run that produced it, a
timestamp, a last-used stamp. A contradicting fact **supersedes** rather than
overwrites, so the history of what workmate believed stays intact.

**Privacy.** Memory lives in local SQLite and leaves the machine only inside a
model call the user initiated. A scheduled automation counts as user-initiated,
because the user created the schedule.
