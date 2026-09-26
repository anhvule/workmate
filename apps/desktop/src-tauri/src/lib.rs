//! Workmate's Tauri shell.
//!
//! Rust owns process supervision, persistence and every write to durable state.
//! The webview never persists anything: cowork-z had to move completion handling
//! into Rust because `WKWebView` throttles backgrounded listeners, and workmate
//! takes that as a constraint from the first commit rather than a later fix
//! (ticket 010).

pub mod db;
pub mod engine;
pub mod migrations;

use serde::Serialize;

/// The `OpenCode` version workmate is pinned to.
///
/// The engine moves only when workmate ships. An engine that changes underneath
/// a recorded session produces bugs workmate cannot reproduce (ticket 008).
pub const PINNED_ENGINE_VERSION: &str = "1.18.32";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub pinned_version: &'static str,
    pub sidecar_present: bool,
}

// Tauri generates the command shim from this signature and requires an owned
// `AppHandle`, so the value cannot be taken by reference here.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn engine_info(app: tauri::AppHandle) -> EngineInfo {
    EngineInfo {
        pinned_version: PINNED_ENGINE_VERSION,
        sidecar_present: sidecar_present(&app),
    }
}

/// Whether the bundled `OpenCode` binary resolved.
///
/// Workmate bundles the engine rather than requiring a global install, so a
/// missing sidecar is a packaging fault worth surfacing plainly instead of
/// failing at first prompt.
fn sidecar_present(app: &tauri::AppHandle) -> bool {
    use tauri::Manager as _;
    app.path()
        .resolve("opencode", tauri::path::BaseDirectory::Resource)
        .is_ok_and(|p| p.exists())
}

/// The schema version this build expects, surfaced for diagnostics.
// Tauri generates the command shim from this signature and requires an owned
// `State`, so the value cannot be taken by reference here.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn schema_version(db: tauri::State<'_, db::Db>) -> Result<i64, String> {
    db.schema_version().map_err(|e| e.to_string())
}

/// # Panics
/// Panics if the Tauri runtime cannot start, or if the database cannot be
/// opened or migrated — neither is recoverable, and continuing without
/// persistence would silently lose the user's work.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager as _;
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(db::Db::open(&dir.join("workmate.sqlite3"))?);
            app.manage(engine::EngineState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![engine_info, schema_version])
        .on_window_event(|window, event| {
            // `ExitRequested`, never `Exit`: by the time `Exit` fires the
            // runtime is tearing down and the engine is left orphaned.
            if matches!(event, tauri::WindowEvent::Destroyed) {
                use tauri::Manager as _;
                if let Some(state) = window.app_handle().try_state::<engine::EngineState>() {
                    let _ = state.shutdown();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running workmate");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_version_is_pinned_exactly_with_no_range() {
        // A range here would let the engine drift under a recorded session.
        assert!(PINNED_ENGINE_VERSION.chars().all(|c| c.is_ascii_digit() || c == '.'));
        assert_eq!(PINNED_ENGINE_VERSION.split('.').count(), 3);
    }
}
