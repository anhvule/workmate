import type { ProviderId, RoleId, WorkspaceId } from "./ids.js";
import { resolutionOrder, scopeKey, type Scope } from "./scope.js";

/**
 * Keychain account-name format version.
 *
 * Cowork-z keys its keychain entries by provider id alone, which cannot be
 * widened later without walking the user's OS keychain blind. The `v1:` prefix
 * exists so a future format change can *find* the entries it needs to rewrite
 * (ticket 009).
 */
export const CREDENTIAL_KEY_VERSION = "v1" as const;

/** `v1:<scope>:<provider>` — the only place this format is constructed. */
export const credentialAccount = (scope: Scope, provider: ProviderId): string =>
  `${CREDENTIAL_KEY_VERSION}:${scopeKey(scope)}:${provider}`;

export interface StoredCredential {
  readonly scope: Scope;
  readonly provider: ProviderId;
}

export type CredentialLookup =
  | { readonly found: true; readonly account: string; readonly scope: Scope }
  | { readonly found: false; readonly tried: readonly string[] };

/**
 * Resolve which credential a role should use, narrowest scope first.
 *
 * Returns the accounts tried on failure rather than throwing: a missing
 * credential *pauses* a run so the user can supply a key, and the UI needs to
 * say what it looked for. Failing the run would discard the work every other
 * role in the team has already done (ticket 009).
 */
export const resolveCredential = (
  available: readonly StoredCredential[],
  provider: ProviderId,
  ctx: { role?: RoleId | undefined; workspace?: WorkspaceId | undefined },
): CredentialLookup => {
  const tried: string[] = [];
  for (const scope of resolutionOrder(ctx)) {
    const account = credentialAccount(scope, provider);
    tried.push(account);
    const hit = available.find(
      (c) => c.provider === provider && scopeKey(c.scope) === scopeKey(scope),
    );
    if (hit) return { found: true, account, scope };
  }
  return { found: false, tried };
};
