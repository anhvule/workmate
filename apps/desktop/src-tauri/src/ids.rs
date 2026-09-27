//! Opaque entity ids.
//!
//! Prefixed so an id read out of a log, an error or a branch name says what it
//! identifies. The random half is 128 bits from the system CSPRNG — the same
//! source as the engine's launch password, because a guessable workspace id
//! would be a guessable `workmate/run-<id>` branch name.

use std::fmt::Write as _;

/// A fresh `<prefix>_<32 hex chars>` id.
///
/// # Panics
/// Panics if the system randomness source is unavailable, which is not a
/// condition workmate can meaningfully continue past.
#[must_use]
pub fn new_id(prefix: &str) -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("system randomness is unavailable");
    bytes.iter().fold(format!("{prefix}_"), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_prefixed_and_does_not_repeat() {
        let a = new_id("ws");
        assert!(a.starts_with("ws_"), "{a} should name what it identifies");
        assert_eq!(a.len(), 3 + 32);
        assert_ne!(a, new_id("ws"));
    }
}
