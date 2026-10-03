//! The few app-wide settings that are workmate's to store.
//!
//! The webview persists nothing (ticket 010), so even a remembered choice like
//! the default model lives here, in Rust's database.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::{Db, DbError};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultModel {
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// # Errors
/// [`DbError`] on a read failure.
pub fn default_model(db: &Db) -> Result<DefaultModel, DbError> {
    db.with(|c| {
        c.query_row("SELECT default_provider, default_model FROM app_settings WHERE id = 1", [], |r| {
            Ok(DefaultModel { provider: r.get(0)?, model: r.get(1)? })
        })
    })
}

/// Set the default model. Blank means unset. A model without a provider is
/// meaningless, so it is refused rather than half-stored.
///
/// # Errors
/// [`DbError`] on a write failure, or a message if only one half is given.
pub fn set_default_model(db: &Db, provider: &str, model: &str) -> Result<(), String> {
    let (p, m) = (provider.trim(), model.trim());
    if p.is_empty() != m.is_empty() {
        return Err("a default model needs both a provider and a model".into());
    }
    let (p, m) = (Some(p).filter(|s| !s.is_empty()), Some(m).filter(|s| !s.is_empty()));
    db.with(|c| c.execute("UPDATE app_settings SET default_provider=?1, default_model=?2 WHERE id=1", params![p, m]))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_model_round_trips_and_can_be_cleared() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(default_model(&db).unwrap(), DefaultModel::default());
        set_default_model(&db, " anthropic ", "claude-sonnet").unwrap();
        assert_eq!(
            default_model(&db).unwrap(),
            DefaultModel { provider: Some("anthropic".into()), model: Some("claude-sonnet".into()) }
        );
        set_default_model(&db, "", "").unwrap();
        assert_eq!(default_model(&db).unwrap(), DefaultModel::default());
    }

    #[test]
    fn half_a_model_is_refused() {
        let db = Db::open_in_memory().unwrap();
        assert!(set_default_model(&db, "anthropic", "").is_err());
        assert!(set_default_model(&db, "", "m").is_err());
    }
}
