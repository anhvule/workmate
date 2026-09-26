---
id: 005
title: What the agent team looks and feels like
type: prototype
mode: HITL
status: open
assignee:
blocked-by: [003]
---

## Question

The collaborating agent team is workmate's headline differentiator, and it has no
obvious precedent in the UI — cowork-z's Arena shows three parallel columns
because the models are competing. A collaborating team is sequential, branching,
and has handoffs. What does that actually look like on screen?

Build a cheap, throwaway prototype (`prototype`) to react to, then decide:

- How a running team is represented: one conversation with attributed turns, a
  lane per role, a graph of handoffs, or something else.
- What a **handoff** looks like as the user watches it happen, and whether the
  user can intervene mid-handoff — redirect, veto, or amend what's being passed.
- How the user composes a team: picking roles per task, saved team presets, or a
  default team that is simply always there.
- How a role's identity is shown — is a role a persona, a model choice, a tool
  set, a system prompt, or all four surfaced together?
- What the user sees when a role is waiting, working, or blocked on a permission
  prompt, and how three concurrent roles avoid becoming noise.
- The degenerate case: a one-role team should not feel heavier than a plain chat.

Link the prototype from this ticket as an asset; do not paste it in.
