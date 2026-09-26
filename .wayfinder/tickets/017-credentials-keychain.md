---
id: 017
title: Credentials in the OS Keychain
type: task
mode: AFK
status: closed
assignee: agent (autonomous mode)
blocked-by: [011]
---

## Question

Implement the scheme decided in [How credentials are scoped](009-credential-scoping.md).

- Keychain accounts `v1:<scope>:<provider>`, constructed only via
  `credentialAccount` in `@workmate/core`.
- Resolution role → workspace → global, via `resolveCredential`.
- Credential and model are separate fields on a role.
- A missing credential pauses the run with an inline add-key action; it must not
  fail the run.
- Adopt the fingerprint bridge: real keys never reach the webview, and the
  fingerprint is computed over the run's resolved set.

## Resolution

Implemented in Rust, in `apps/desktop/src-tauri/src/credentials.rs`, with 8 tests.

**A correction to the plan.** The ticket assumed `credentialAccount` in
`@workmate/core` would construct the account name. It does not, because real keys
never reach the sidecar — only a fingerprint does — so the keychain is Rust's
alone. Keeping a TypeScript copy of the format would have been a second
implementation of a string that must never drift, so the TypeScript credential
module was **deleted**. `scope.ts` keeps `resolutionOrder`, which memory still
needs, with a note saying why credentials left.

**Landed.** Account format `v1:<scope>:<provider>`, constructed in exactly one
function. Scope is role, workspace or global, resolved narrowest-first. `Keychain`
wraps the `keyring` crate with native backends per platform; resolution is
written against a `CredentialStore` trait so it is tested against a fake — no
test touches the user's real keychain.

**A miss is not an error.** `Resolved::Missing` carries every account tried, in
order, so the UI can say what it looked for and the run can pause rather than
fail. Failing would discard the work other roles in the team had already done.

**The fingerprint bridge is in.** `fingerprint` hashes the sorted resolved
account set with FNV-1a — explicitly not a security primitive, and not used as
one: it identifies a set, it does not protect it. Tests cover order-independence,
that a scope change invalidates it, and that a boundary shift (`"ab"+"c"` versus
`"a"+"bc"`) does not collide, which is the failure a naive concatenation would
have.

Still to come: the UI for adding a key, and wiring resolution into run startup,
both in [Run and team orchestration](014-run-orchestration.md).
