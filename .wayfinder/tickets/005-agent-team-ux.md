---
id: 005
title: What the agent team looks and feels like
type: prototype
mode: HITL
status: closed
assignee: agent (autonomous mode)
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

## Resolution

> **Agent-made decision.** Taken autonomously at the user's instruction.

No prototype was built — the prototype step needs a human to react to it, and in
autonomous mode there is nobody to react. Decided from first principles instead,
which makes this the ticket most worth revisiting.

**One thread, attributed turns.** The default view is a single readable
conversation where each turn carries its role's chip — *not* three lanes. Lanes
are Arena's answer to competing clones; a collaborating team is sequential, and
lanes would reproduce exactly the noise this differentiator is supposed to avoid.
A secondary "team view" renders the handoff graph for runs complex enough to want
it.

**Handoff is a card in the thread**, not an invisible transition: from-role →
to-role, and the context actually passed, collapsed by default and expandable.
This is the differentiator made visible — it is the thing a user cannot see in any
competing tool — so it gets deliberate weight in the layout.

**Intervention.** A run can be paused. While paused, a pending handoff card is
editable: the user can amend the passed context, redirect to a different role, or
veto and end the run. Pausing is not the same as stopping — sessions stay alive.

**Role identity** is a chip: name plus model badge, with the system prompt, tool
allowlist and permission profile in a popover. All four are surfaced together
because a role is all four, and hiding any of them makes runs unexplainable.

**Status** lives in a compact per-run header strip — one chip per role, showing
working / waiting / blocked. Permission prompts surface modally regardless of
which role raised them, and name the role in the prompt.

**Degenerate case is load-bearing.** A one-role team renders as a plain chat: no
header strip, no handoff cards, no team affordances. If the simple case feels
heavier than cowork-z, the differentiator is a tax rather than a feature.
