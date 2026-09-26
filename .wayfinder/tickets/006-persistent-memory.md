---
id: 006
title: How workmate remembers
type: grilling
mode: HITL
status: open
assignee:
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
