---
id: 022
title: First run
type: task
mode: AFK
status: closed
assignee:
blocked-by: [013, 017]
---

## Question

What a brand-new user sees before any workspace exists.

- The first-launch path: no workspace, no credential, a bundled engine that works.
- Where the first credential is asked for, and how little can be asked before the
  app is useful.
- Whether the first run is a solo chat or a team, given a team is the thesis but
  a solo chat is the gentler introduction.
- What proves to the user, in the first minute, that this is not another chat
  window.

## Resolution

Built as `views/Welcome.tsx` and the readiness checks in `views/NewRun.tsx`, and
verified by a smoke test that walks it.

- **The first launch asks for one thing: a folder.** No account, no key, no model
  before the app is useful. The bundled engine means there is nothing to install.
- **Then a choice, with the gentle option the default.** "One assistant" is
  pre-selected and labelled as the good first step; "A team" is shown right beside
  it with its three role chips, one click away. Choosing applies the matching
  starter pack, so the project opens with a team already configured and a starter
  `AGENTS.md` if it had none.
- **Solo or team first? Solo.** A team is the thesis, but a solo chat is the gentler
  introduction, and it is also the proof that the simple case is not a tax. The
  team is offered, visibly, at the same moment.
- **The key is asked for at the first message, not before.** The composer checks
  what stands between this team and a working message and says so in place, one
  thing at a time: not a git repository (with the two commands to fix it), no
  model chosen (a picker), no key for that provider (a password field, "kept in
  your system keychain"). The reason for asking is obvious when it is asked.
- **What proves it is not another chat window, in the first minute:** three plain
  statements on the welcome screen (it works on a branch; a team, not a chatbot;
  it remembers), then the first run itself — its own branch, a Changes panel with
  a real diff and an explicit Merge — and, the moment a team is chosen, the
  handoff cards and the pause-and-amend control. The first *team* run is where
  the differentiator is visible without being explained.
- **Not done:** a guided demo run that needs no key. It would be a fake model
  turn dressed as the product, and the honest version of the first minute needs a
  real key.
