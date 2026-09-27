//! Workmate's Tauri shell.
//!
//! Rust owns process supervision, persistence and every write to durable state.
//! The webview never persists anything: cowork-z had to move completion handling
//! into Rust because `WKWebView` throttles backgrounded listeners, and workmate
//! takes that as a constraint from the first commit rather than a later fix
//! (ticket 010).

pub mod credentials;
pub mod db;
pub mod engine;
pub mod ids;
pub mod migrations;
pub mod permissions;
pub mod runtime;
pub mod sidecar;
pub mod workspace;

use std::path::PathBuf;

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

/// Start the engine and sidecar, returning the engine's base URL.
///
/// The launch password is deliberately **not** returned: it crosses to the
/// sidecar and no further, the same discipline as the api-key bridge.
// Tauri generates the command shim from this signature and requires owned
// `State`, so these cannot be taken by reference here.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn start_runtime(
    rt: tauri::State<'_, runtime::Runtime>,
    binaries: tauri::State<'_, runtime::Binaries>,
) -> Result<String, String> {
    rt.start(&binaries).map(|a| a.base_url).map_err(|e| e.to_string())
}

/// The schema version this build expects, surfaced for diagnostics.
// Tauri generates the command shim from this signature and requires an owned
// `State`, so the value cannot be taken by reference here.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn schema_version(db: tauri::State<'_, db::Db>) -> Result<i64, String> {
    db.schema_version().map_err(|e| e.to_string())
}

/// A workspace plus whether its directory is still there.
///
/// The two travel together because every screen that shows a workspace has to
/// be able to offer the repair.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
    pub workspace: workspace::Workspace,
    pub binding: workspace::Binding,
}

fn view(ws: workspace::Workspace) -> WorkspaceView {
    let binding = workspace::binding(&ws);
    WorkspaceView { workspace: ws, binding }
}

// Tauri generates the command shims from these signatures and requires owned
// `State` and `AppHandle`, so none of these can be taken by reference.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn workspace_create(
    db: tauri::State<'_, db::Db>,
    name: String,
    directory: PathBuf,
) -> Result<WorkspaceView, String> {
    workspace::create(&db, &name, &directory).map(view).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn workspace_list(db: tauri::State<'_, db::Db>) -> Result<Vec<WorkspaceView>, String> {
    workspace::list(&db)
        .map(|all| all.into_iter().map(view).collect())
        .map_err(|e| e.to_string())
}

/// Open a workspace, and widen the asset protocol to what it may serve.
///
/// `assetProtocol.scope` ships empty and is filled in here, from the bound
/// root and the granted directories only — never a wildcard, and never a
/// folder the user did not grant (ticket 013).
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn workspace_open(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    id: String,
) -> Result<WorkspaceView, String> {
    use tauri::Manager as _;
    let ws = workspace::open(&db, &id).map_err(|e| e.to_string())?;
    let scope = app.asset_protocol_scope();
    for dir in permissions::granted_directories(&db, &ws).map_err(|e| e.to_string())? {
        scope.allow_directory(&dir, true).map_err(|e| e.to_string())?;
    }
    Ok(view(ws))
}

/// Re-point a workspace whose folder moved. The id — and every row that
/// references it — survives.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn workspace_relocate(
    db: tauri::State<'_, db::Db>,
    id: String,
    directory: PathBuf,
) -> Result<WorkspaceView, String> {
    workspace::relocate(&db, &id, &directory).map(view).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn workspace_remove(db: tauri::State<'_, db::Db>, id: String) -> Result<(), String> {
    workspace::remove(&db, &id).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn permission_grants(
    db: tauri::State<'_, db::Db>,
    workspace_id: String,
) -> Result<Vec<permissions::Grant>, String> {
    permissions::grants(&db, &workspace_id).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn permission_add(
    db: tauri::State<'_, db::Db>,
    workspace_id: String,
    path: PathBuf,
    operation: permissions::Operation,
    source: permissions::Source,
) -> Result<permissions::Grant, String> {
    permissions::grant(&db, &workspace_id, &path, operation, source).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn permission_revoke(db: tauri::State<'_, db::Db>, grant_id: String) -> Result<(), String> {
    permissions::revoke(&db, &grant_id).map_err(|e| e.to_string())
}

/// Persist the user's answer to a runtime prompt.
///
/// Answering the engine is the caller's next step, and is always `once`: the
/// durable half of an *always* lives here, so workmate can show and revoke it.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn permission_reply(
    db: tauri::State<'_, db::Db>,
    workspace_id: String,
    path: PathBuf,
    operation: permissions::Operation,
    reply: permissions::Reply,
) -> Result<Option<permissions::Grant>, String> {
    permissions::record_reply(&db, &workspace_id, &path, operation, reply)
        .map_err(|e| e.to_string())
}

/// The ruleset a session in `worktree` is created with.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn permission_ruleset(
    db: tauri::State<'_, db::Db>,
    workspace_id: String,
    worktree: PathBuf,
) -> Result<Vec<permissions::Rule>, String> {
    let ws = workspace::open(&db, &workspace_id).map_err(|e| e.to_string())?;
    permissions::ruleset(&db, &ws, &worktree).map_err(|e| e.to_string())
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
            app.manage(runtime::Runtime::default());
            app.manage(runtime::Binaries::in_dir(
                &app.path().resolve("binaries", tauri::path::BaseDirectory::Resource)?,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_info,
            schema_version,
            start_runtime,
            workspace_create,
            workspace_list,
            workspace_open,
            workspace_relocate,
            workspace_remove,
            permission_grants,
            permission_add,
            permission_revoke,
            permission_reply,
            permission_ruleset
        ])
        .on_window_event(|window, event| {
            // `ExitRequested`, never `Exit`: by the time `Exit` fires the
            // runtime is tearing down and the engine is left orphaned.
            if matches!(event, tauri::WindowEvent::Destroyed) {
                use tauri::Manager as _;
                if let Some(rt) = window.app_handle().try_state::<runtime::Runtime>() {
                    let _ = rt.shutdown();
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
