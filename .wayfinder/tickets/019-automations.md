---
id: 019
title: Cron automations
type: task
mode: AFK
status: open
assignee:
blocked-by: [014, 016]
---

## Question

Scheduled, unattended runs — and the reason worktree isolation had to land first.

- Scheduling surface and persistence; one run per fire, bounded concurrency.
- Unattended runs work in their own worktree and never touch the user's checkout.
- What a permission prompt means with nobody watching: deny by default, or a
  pre-granted profile attached to the automation. Decide and justify.
- Where findings surface, and how an automation that finds nothing stays quiet.
- Run history, failure handling, and what happens when the machine was asleep.
