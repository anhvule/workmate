# Wayfinder tracker (local markdown)

No external issue tracker is configured for this repo, so the wayfinder map and its
tickets live here as markdown, versioned alongside the code.

## Layout

- `map.md` — the map. Label: `wayfinder:map`. Loaded once per session.
- `tickets/NNN-slug.md` — child tickets of the map, one file each.

## Ticket conventions

Each ticket carries YAML frontmatter:

```yaml
---
id: NNN
title: <the ticket's name — refer to tickets by this, never by id alone>
type: research | prototype | grilling | task   # the wayfinder:<type> label
mode: AFK | HITL
status: open | closed
assignee:            # empty == unclaimed. Set BEFORE any work, to claim it.
blocked-by: []       # ids of tickets that must close first
---
```

- **Frontier** = tickets with `status: open`, empty `assignee`, and every id in
  `blocked-by` already `status: closed`.
- **Claim** a ticket by filling `assignee` and committing, before doing any work.
- **Resolve** by appending a `## Resolution` section to the ticket body, setting
  `status: closed`, and adding a one-line pointer to the map's *Decisions so far*.
- Assets produced while resolving are **linked** from the ticket, never pasted in.

## Frontier query

```bash
.wayfinder/frontier.sh
```
