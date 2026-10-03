//! Provider credentials in the OS keychain.
//!
//! Rust owns this end to end. Real keys never reach the webview and never reach
//! the sidecar: the sidecar is handed a *fingerprint* of the resolved set, and
//! the keys themselves travel only from here into the engine's environment at
//! spawn time. That narrow bridge is lifted wholesale from cowork-z, which is
//! the one part of its credential design that needs no improvement.

use std::fmt::Write as _;

/// Keychain account-name format version.
///
/// Cowork-z keys entries by provider id alone, which cannot be widened later
/// without walking the user's keychain blind. The prefix exists so a future
/// format change can *find* what it must rewrite (ticket 009).
pub const KEY_VERSION: &str = "v1";

/// The keychain service workmate stores under.
pub const SERVICE: &str = "dev.workmate.app";

/// The axis along which a credential is resolved.
///
/// Ordered narrowest first; a role can override its workspace without the
/// workspace knowing the role exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    Role(String),
    Workspace(String),
    Global,
}

impl Scope {
    fn key(&self) -> String {
        match self {
            Self::Role(id) => format!("role:{id}"),
            Self::Workspace(id) => format!("workspace:{id}"),
            Self::Global => "global".to_owned(),
        }
    }
}

/// `v1:<scope>:<provider>` — the only place this format is constructed.
#[must_use]
pub fn account(scope: &Scope, provider: &str) -> String {
    format!("{KEY_VERSION}:{}:{provider}", scope.key())
}

/// The scopes to consult for a run, narrowest first.
#[must_use]
pub fn resolution_order(role: Option<&str>, workspace: Option<&str>) -> Vec<Scope> {
    let mut out = Vec::with_capacity(3);
    if let Some(r) = role {
        out.push(Scope::Role(r.to_owned()));
    }
    if let Some(w) = workspace {
        out.push(Scope::Workspace(w.to_owned()));
    }
    out.push(Scope::Global);
    out
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("keychain: {0}")]
    Keyring(#[from] keyring::Error),
}

/// What a lookup found, or everything it tried.
///
/// A miss is not an error: a missing credential **pauses** a run so the user can
/// supply a key, and the UI needs to say what was looked for. Failing the run
/// would discard the work every other role has already done (ticket 009).
#[derive(Debug, PartialEq, Eq)]
pub enum Resolved {
    Found { account: String, scope: Scope },
    Missing { tried: Vec<String> },
}

/// Somewhere credentials live. Abstracted so resolution is testable without
/// touching the real keychain, which no test should do.
pub trait CredentialStore {
    /// # Errors
    /// Returns [`CredentialError`] if the store cannot be queried.
    fn contains(&self, account: &str) -> Result<bool, CredentialError>;

    /// The secret itself. Only Rust ever calls this, to hand a key to the
    /// engine; it must never be logged or returned over the sidecar pipe.
    ///
    /// # Errors
    /// Returns [`CredentialError`] if the store cannot be queried.
    fn secret(&self, account: &str) -> Result<Option<String>, CredentialError>;
}

/// The real OS keychain.
pub struct Keychain;

impl CredentialStore for Keychain {
    fn secret(&self, account: &str) -> Result<Option<String>, CredentialError> {
        match keyring::Entry::new(SERVICE, account).and_then(|e| e.get_password()) {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CredentialError::Keyring(e)),
        }
    }

    fn contains(&self, account: &str) -> Result<bool, CredentialError> {
        match keyring::Entry::new(SERVICE, account).and_then(|e| e.get_password()) {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(CredentialError::Keyring(e)),
        }
    }
}

impl Keychain {
    /// Store a secret.
    ///
    /// # Errors
    /// Returns [`CredentialError`] if the keychain rejects the write.
    pub fn set(scope: &Scope, provider: &str, secret: &str) -> Result<(), CredentialError> {
        keyring::Entry::new(SERVICE, &account(scope, provider))?.set_password(secret)?;
        Ok(())
    }

    /// Remove a secret. Absent is success.
    ///
    /// # Errors
    /// Returns [`CredentialError`] if the keychain rejects the delete.
    pub fn delete(scope: &Scope, provider: &str) -> Result<(), CredentialError> {
        match keyring::Entry::new(SERVICE, &account(scope, provider))
            .and_then(|e| e.delete_credential())
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CredentialError::Keyring(e)),
        }
    }
}

/// Resolve which credential a role should use, narrowest scope first.
///
/// # Errors
/// Returns [`CredentialError`] if the store cannot be queried.
pub fn resolve(
    store: &impl CredentialStore,
    provider: &str,
    role: Option<&str>,
    workspace: Option<&str>,
) -> Result<Resolved, CredentialError> {
    let mut tried = Vec::new();
    for scope in resolution_order(role, workspace) {
        let acct = account(&scope, provider);
        tried.push(acct.clone());
        if store.contains(&acct)? {
            return Ok(Resolved::Found {
                account: acct,
                scope,
            });
        }
    }
    Ok(Resolved::Missing { tried })
}

/// A stable digest of the credential set a run resolved to.
///
/// The sidecar receives this instead of the keys. A scope change alters the
/// resolved set and therefore the fingerprint, which is what makes it a usable
/// cache key as well as a safe one.
#[must_use]
pub fn fingerprint(accounts: &[String]) -> String {
    let mut sorted: Vec<&String> = accounts.iter().collect();
    sorted.sort();
    // FNV-1a: not a security primitive, and not used as one — this identifies a
    // set, it does not protect it.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for account in sorted {
        for byte in account.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        hash ^= u64::from(b'\n');
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    let mut out = String::with_capacity(16);
    let _ = write!(out, "{hash:016x}");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    struct Fake(HashSet<String>);
    impl Fake {
        fn with(accounts: &[&str]) -> Self {
            Self(accounts.iter().map(|s| (*s).to_owned()).collect())
        }
    }
    impl CredentialStore for Fake {
        fn secret(&self, account: &str) -> Result<Option<String>, CredentialError> {
            Ok(self.0.contains(account).then(|| format!("secret-for-{account}")))
        }

        fn contains(&self, account: &str) -> Result<bool, CredentialError> {
            Ok(self.0.contains(account))
        }
    }

    #[test]
    fn the_account_format_is_versioned_and_scoped() {
        assert_eq!(account(&Scope::Global, "anthropic"), "v1:global:anthropic");
        assert_eq!(
            account(&Scope::Role("reviewer".into()), "anthropic"),
            "v1:role:reviewer:anthropic"
        );
        assert_eq!(
            account(&Scope::Workspace("ws-1".into()), "anthropic"),
            "v1:workspace:ws-1:anthropic"
        );
    }

    #[test]
    fn a_role_key_beats_the_workspace_and_the_global_one() {
        let store = Fake::with(&[
            "v1:global:anthropic",
            "v1:workspace:ws-1:anthropic",
            "v1:role:reviewer:anthropic",
        ]);
        let got = resolve(&store, "anthropic", Some("reviewer"), Some("ws-1")).expect("resolve");
        assert_eq!(
            got,
            Resolved::Found {
                account: "v1:role:reviewer:anthropic".to_owned(),
                scope: Scope::Role("reviewer".into()),
            }
        );
    }

    #[test]
    fn resolution_falls_back_through_workspace_to_global() {
        let store = Fake::with(&["v1:global:anthropic"]);
        let got = resolve(&store, "anthropic", Some("reviewer"), Some("ws-1")).expect("resolve");
        assert!(matches!(got, Resolved::Found { scope: Scope::Global, .. }));
    }

    #[test]
    fn another_providers_key_is_never_substituted() {
        let store = Fake::with(&["v1:global:openai"]);
        let got = resolve(&store, "anthropic", None, None).expect("resolve");
        assert!(matches!(got, Resolved::Missing { .. }));
    }

    #[test]
    fn a_miss_reports_what_it_tried_so_the_run_can_pause_and_ask() {
        let store = Fake::with(&[]);
        let got = resolve(&store, "anthropic", Some("reviewer"), Some("ws-1")).expect("resolve");
        assert_eq!(
            got,
            Resolved::Missing {
                tried: vec![
                    "v1:role:reviewer:anthropic".to_owned(),
                    "v1:workspace:ws-1:anthropic".to_owned(),
                    "v1:global:anthropic".to_owned(),
                ]
            }
        );
    }

    #[test]
    fn the_fingerprint_identifies_a_set_regardless_of_order() {
        let a = fingerprint(&["v1:global:a".to_owned(), "v1:global:b".to_owned()]);
        let b = fingerprint(&["v1:global:b".to_owned(), "v1:global:a".to_owned()]);
        assert_eq!(a, b);
    }

    #[test]
    fn changing_a_scope_changes_the_fingerprint() {
        let global = fingerprint(&["v1:global:anthropic".to_owned()]);
        let scoped = fingerprint(&["v1:role:reviewer:anthropic".to_owned()]);
        assert_ne!(global, scoped, "a scope change must invalidate the fingerprint");
    }

    #[test]
    fn the_fingerprint_does_not_collide_on_a_boundary_shift() {
        // "ab" + "c" must not hash as "a" + "bc"; the separator is what prevents it.
        let x = fingerprint(&["ab".to_owned(), "c".to_owned()]);
        let y = fingerprint(&["a".to_owned(), "bc".to_owned()]);
        assert_ne!(x, y);
    }
}
