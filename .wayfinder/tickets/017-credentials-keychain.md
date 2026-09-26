---
id: 017
title: Credentials in the OS Keychain
type: task
mode: AFK
status: open
assignee:
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
