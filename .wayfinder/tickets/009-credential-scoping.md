---
id: 009
title: How credentials are scoped
type: grilling
mode: HITL
status: closed
assignee: agent (autonomous mode)
blocked-by: [003]
---

## Question

This carries a deadline the other tickets do not. Cowork-z keys OS Keychain
entries by **provider id alone**, globally, with one active provider and one
selected model for the whole app. Changing that key format after shipping means a
migration across the user's OS keychain — materially nastier than a SQLite
migration, and not something to discover later.

It also breaks immediately under the agent-team differentiator: a team wants a
cheap model for the researcher and an expensive one for the reviewer, which a
single global `active_provider_id` cannot express.

Decide, before any credential code is written:

- **The key identity.** Stay `provider`, or widen to `(provider, scope)` — and if
  scoped, what is a scope: workspace, agent role, team, or a named credential
  profile the user selects? This depends on the vocabulary settled in
  [Workmate's domain model](003-domain-model.md).
- **Model selection versus credentials.** Are these one decision or two? A role
  might need a different *model* from the same provider and credential — which
  argues for separating "which key" from "which model" rather than cowork-z's
  coupled `active_provider_id` + `selected_model_id`.
- **Resolution order.** Given a role inside a team inside a workspace, which
  credential wins, and what does the user see when a role has no credential for
  the model it wants.
- **Migration stance.** Even with a scoped key from day one, state the versioning
  scheme for keychain account names so a future change is survivable.
- **Keep the fingerprint bridge?** Cowork-z passes only an `apiKeysFingerprint` in
  task payloads; real keys move solely through a dedicated request/response at
  server-spawn time and never reach the webview. Confirm workmate adopts this, and
  what a scoped key does to the fingerprint's identity.

Surfaced by [Cowork-z architecture up close](002-cowork-z-architecture.md) §7.

## Resolution

> **Agent-made decision.** Taken autonomously at the user's instruction.

**Keychain account format: `v1:<scope>:<provider>`.** Scope is one of `global`,
`workspace:<id>` or `role:<id>`. The `v1:` prefix is not decoration — it is the
thing that makes a future change survivable, since rewriting keychain entries
without a way to find them is the trap this ticket exists to avoid. A
`credential_key_version` row in app settings records the current format.

**Credential and model are separate fields.** A role selects a model; credential
resolution is independent. Cowork-z's coupled `active_provider_id` +
`selected_model_id` cannot express "cheap model for the researcher, expensive one
for the reviewer" on the same key — which is the agent-team differentiator's most
basic requirement.

**Resolution order: role → workspace → global**, most specific wins.

**A missing credential pauses, it does not fail.** The role shows blocked with an
inline add-key action and the run waits. Failing a multi-role run because the
third role has no key would discard the first two roles' work.

**The fingerprint bridge is adopted wholesale.** Real keys never reach the webview;
payloads carry only a fingerprint, and keys move solely through a dedicated
request/response at engine-spawn time. With scoped keys the fingerprint is
computed over the *resolved set for the run*, so a scope change correctly
invalidates it.
