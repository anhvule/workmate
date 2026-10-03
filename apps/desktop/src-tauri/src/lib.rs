//! Workmate's Tauri shell.
//!
//! Rust owns process supervision, persistence and every write to durable state.
//! The webview never persists anything: cowork-z had to move completion handling
//! into Rust because `WKWebView` throttles backgrounded listeners, and workmate
//! takes that as a constraint from the first commit rather than a later fix
//! (ticket 010).

pub mod automation;
pub mod credentials;
pub mod cron;
pub mod db;
pub mod engine;
pub mod events;
pub mod host;
pub mod ids;
pub mod mcp;
pub mod memory;
pub mod migrations;
pub mod ops;
pub mod packs;
pub mod permissions;
pub mod repo;
pub mod runtime;
pub mod settings;
pub mod sidecar;
pub mod skills;
pub mod team;
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
    app: tauri::AppHandle,
    rt: tauri::State<'_, runtime::Runtime>,
    binaries: tauri::State<'_, runtime::Binaries>,
) -> Result<String, String> {
    use tauri::Manager as _;
    // Persistence and host operations run here, in Rust, on Rust's connection:
    // the one writer (tickets 024 and 026). Anything that is not a host
    // operation falls through to the named database operations.
    let db_app = app.clone();
    let handler: sidecar::Handler = std::sync::Arc::new(move |op, args| {
        let root = worktree_root(&db_app)?;
        let engine = db_app
            .state::<runtime::Runtime>()
            .address()
            .map_err(|e| e.to_string())?;
        host::Host {
            db: &db_app.state::<db::Db>(),
            worktree_root: &root,
            engine,
            store: &credentials::Keychain,
        }
        .dispatch(op, args)
    });
    // The sidecar's own events (run progress, handoffs) reach the webview as
    // `workmate:<name>`; Tauri rejects dots in names.
    let ev_app = app.clone();
    let on_event: sidecar::EventOut = std::sync::Arc::new(move |name, payload| {
        use tauri::Emitter as _;
        let _ = ev_app.emit(&format!("workmate:{}", name.replace('.', "_")), payload);
    });
    let hooks = sidecar::Hooks { handler, on_event };
    let address = rt.start(&binaries, hooks).map_err(|e| e.to_string())?;
    // The sink runs here, in Rust, whether or not a window is watching: a
    // backgrounded `WKWebView` is throttled and would miss completion. The
    // webview gets a scrubbed copy to render (ticket 023).
    let emit_app = app.clone();
    rt.attach_events(|addr| {
        let sink: events::Sink = std::sync::Arc::new(move |e| {
            use tauri::Emitter as _;
            let _ = emit_app.emit(&e.name, &e);
        });
        events::Subscriber::start(addr, sink, events::Backoff::default())
    })
    .map_err(|e| e.to_string())?;
    Ok(address.base_url)
}

/// Send a command to the sidecar and wait for its answer.
///
/// This is how the webview drives a run (start, pause, amend, veto …): through
/// Rust, to the sidecar, with the result coming back here. The webview persists
/// nothing; whatever a command changes is written by Rust and announced as an
/// event (ticket 014).
#[tauri::command]
async fn sidecar_command(
    rt: tauri::State<'_, runtime::Runtime>,
    name: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let commander = rt
        .commander()
        .map_err(|e| e.to_string())?
        .ok_or("the runtime is not started")?;
    // Blocking I/O: off the async executor.
    tauri::async_runtime::spawn_blocking(move || {
        commander.command(&name, &args, std::time::Duration::from_secs(120))
    })
    .await
    .map_err(|e| e.to_string())?
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

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn automation_create(
    db: tauri::State<'_, db::Db>,
    workspace_id: String,
    name: String,
    schedule: String,
    objective: String,
    roles: serde_json::Value,
) -> Result<automation::Automation, String> {
    automation::create(&db, &workspace_id, &name, &schedule, &objective, &roles).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn automation_list(db: tauri::State<'_, db::Db>) -> Result<Vec<automation::Automation>, String> {
    automation::list(&db).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn automation_set_enabled(db: tauri::State<'_, db::Db>, id: String, enabled: bool) -> Result<(), String> {
    automation::set_enabled(&db, &id, enabled).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn automation_remove(db: tauri::State<'_, db::Db>, id: String) -> Result<(), String> {
    automation::remove(&db, &id).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn automation_run_now(db: tauri::State<'_, db::Db>, id: String) -> Result<(), String> {
    automation::run_now(&db, &id).map_err(|e| e.to_string())
}

/// Fire history, newest first; `None` is the inbox across every automation.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn automation_history(
    db: tauri::State<'_, db::Db>,
    automation_id: Option<String>,
) -> Result<Vec<automation::Fire>, String> {
    automation::history(&db, automation_id.as_deref(), 200).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn automation_mark_seen(db: tauri::State<'_, db::Db>, fire_id: String) -> Result<(), String> {
    automation::mark_seen(&db, &fire_id).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn automation_unseen(db: tauri::State<'_, db::Db>) -> Result<i64, String> {
    automation::unseen_findings(&db).map_err(|e| e.to_string())
}

/// How often the scheduler looks. A minute-resolution schedule only needs the
/// tick to be shorter than a minute.
const SCHEDULER_TICK: std::time::Duration = std::time::Duration::from_secs(20);

/// Start the scheduler thread.
///
/// It lives in Rust, not the webview, for the usual reason: a backgrounded
/// `WKWebView` is throttled and an automation must still fire. When the runtime
/// is not up there is nobody to run the work, so the tick is skipped and the slot
/// stays due — it fires, or is recorded as missed, once there is.
fn spawn_scheduler(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        use tauri::Manager as _;
        if let Err(e) = automation::recover(&app.state::<db::Db>()) {
            eprintln!("automations: could not recover interrupted fires: {e}");
        }
        loop {
            std::thread::sleep(SCHEDULER_TICK);
            let Ok(Some(commander)) = app.state::<runtime::Runtime>().commander() else { continue };
            let start = |a: &automation::Automation, fire_id: &str| -> Result<String, String> {
                let args = serde_json::json!({
                    "workspaceId": a.workspace_id,
                    "objective": automation::unattended_objective(&a.objective),
                    "roles": a.roles,
                    "unattended": true,
                    "automation": { "id": a.id, "fireId": fire_id },
                });
                let out = commander.command("run.start", &args, std::time::Duration::from_secs(60))?;
                out["runId"].as_str().map(str::to_owned).ok_or_else(|| "the sidecar did not return a run id".to_owned())
            };
            if let Err(e) = automation::tick(&app.state::<db::Db>(), db::now_ms(), &start) {
                eprintln!("automations: tick failed: {e}");
            }
        }
    });
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn settings_default_model(db: tauri::State<'_, db::Db>) -> Result<settings::DefaultModel, String> {
    settings::default_model(&db).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn settings_set_default_model(db: tauri::State<'_, db::Db>, provider: String, model: String) -> Result<(), String> {
    settings::set_default_model(&db, &provider, &model)
}

/// A workspace's runs, newest first. Read straight from Rust's own store, so the
/// list is there even before the runtime has started.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn run_list(db: tauri::State<'_, db::Db>, workspace_id: String) -> Result<Vec<serde_json::Value>, String> {
    ops::dispatch(&db, "run.list", &serde_json::json!({"workspaceId": workspace_id}))
}

/// One persisted run with its sessions and handoffs, in order.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn run_get(db: tauri::State<'_, db::Db>, run_id: String) -> Result<serde_json::Value, String> {
    ops::dispatch(&db, "run.load", &serde_json::json!({"id": run_id}))?
        .into_iter()
        .next()
        .ok_or_else(|| "no such run".to_owned())
}

/// Show a run's worktree in the file manager. Derived from ids, so the webview
/// can never ask for an arbitrary path to be opened.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn run_reveal_worktree(app: tauri::AppHandle, workspace_id: String, run_id: String) -> Result<String, String> {
    let path = repo::worktree_path(&worktree_root(&app)?, &workspace_id, &run_id);
    if !path.is_dir() {
        return Err("that worktree no longer exists".into());
    }
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(all(unix, not(target_os = "macos")))]
    let opener = "xdg-open";
    std::process::Command::new(opener).arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Where a credential applies, as the webview names it.
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ScopeArg {
    Global,
    Workspace { id: String },
    Role { id: String },
}

impl From<ScopeArg> for credentials::Scope {
    fn from(s: ScopeArg) -> Self {
        match s {
            ScopeArg::Global => Self::Global,
            ScopeArg::Workspace { id } => Self::Workspace(id),
            ScopeArg::Role { id } => Self::Role(id),
        }
    }
}

/// Store a key in the OS keychain. The key goes in and never comes back out:
/// nothing in workmate returns a secret to the webview.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn credential_set(scope: ScopeArg, provider: String, secret: String) -> Result<(), String> {
    if provider.trim().is_empty() || secret.trim().is_empty() {
        return Err("a provider and a key are both required".into());
    }
    credentials::Keychain::set(&scope.into(), provider.trim(), secret.trim()).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn credential_delete(scope: ScopeArg, provider: String) -> Result<(), String> {
    credentials::Keychain::delete(&scope.into(), &provider).map_err(|e| e.to_string())
}

/// What the webview may know about a credential: whether one resolves and from
/// where, or everything that was tried. Never the key.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CredentialStatus {
    found: bool,
    /// `"role"`, `"workspace"` or `"global"` when found.
    scope: Option<&'static str>,
    tried: Vec<String>,
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn credential_status(
    provider: String,
    role_id: Option<String>,
    workspace_id: Option<String>,
) -> Result<CredentialStatus, String> {
    match credentials::resolve(&credentials::Keychain, &provider, role_id.as_deref(), workspace_id.as_deref())
        .map_err(|e| e.to_string())?
    {
        credentials::Resolved::Found { scope, .. } => Ok(CredentialStatus {
            found: true,
            scope: Some(match scope {
                credentials::Scope::Role(_) => "role",
                credentials::Scope::Workspace(_) => "workspace",
                credentials::Scope::Global => "global",
            }),
            tried: vec![],
        }),
        credentials::Resolved::Missing { tried } => Ok(CredentialStatus { found: false, scope: None, tried }),
    }
}

/// A directory shipped with the app, or the source tree's copy when running
/// from `tauri dev`, where resources are not staged.
fn shipped_dir(app: &tauri::AppHandle, name: &str) -> PathBuf {
    use tauri::Manager as _;
    app.path()
        .resolve(name, tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name))
}

fn skill_cache(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager as _;
    app.path().app_data_dir().map(|d| d.join("skill-sources")).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn team_list(db: tauri::State<'_, db::Db>) -> Result<Vec<team::Team>, String> {
    team::list(&db).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn team_create(db: tauri::State<'_, db::Db>, name: String, roles: Vec<team::Role>) -> Result<team::Team, String> {
    team::create(&db, &name, &roles).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn team_update_role(db: tauri::State<'_, db::Db>, role: team::Role) -> Result<(), String> {
    team::update_role(&db, &role).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn team_remove(db: tauri::State<'_, db::Db>, id: String) -> Result<(), String> {
    team::remove(&db, &id).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn pack_list(app: tauri::AppHandle) -> Vec<packs::Pack> {
    packs::load_all(&shipped_dir(&app, "packs")).into_iter().map(|(p, _)| p).collect()
}

fn find_pack(app: &tauri::AppHandle, id: &str) -> Result<(packs::Pack, PathBuf), String> {
    packs::load_all(&shipped_dir(app, "packs"))
        .into_iter()
        .find(|(p, _)| p.id == id)
        .ok_or_else(|| packs::PackError::Unknown(id.to_owned()).to_string())
}

/// What applying a pack would write, so the user can see before agreeing.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn pack_preview(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    pack_id: String,
    workspace_id: String,
) -> Result<packs::Preview, String> {
    let (pack, _) = find_pack(&app, &pack_id)?;
    let ws = workspace::open(&db, &workspace_id).map_err(|e| e.to_string())?;
    packs::preview(&pack, &ws.directory).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn pack_apply(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    pack_id: String,
    workspace_id: String,
) -> Result<packs::Applied, String> {
    let (pack, dir) = find_pack(&app, &pack_id)?;
    let ws = workspace::open(&db, &workspace_id).map_err(|e| e.to_string())?;
    packs::apply(&db, &dir, &pack, &ws).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn skill_catalog(app: tauri::AppHandle, db: tauri::State<'_, db::Db>) -> Result<Vec<skills::Entry>, String> {
    skills::catalog(&db, &shipped_dir(&app, "skills"), &skill_cache(&app)?).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn skill_installed(db: tauri::State<'_, db::Db>, workspace_id: String) -> Result<Vec<skills::Installed>, String> {
    let ws = workspace::open(&db, &workspace_id).map_err(|e| e.to_string())?;
    skills::installed(&db, &ws).map_err(|e| e.to_string())
}

/// Install by `name` and `origin` from the catalog, so the webview never names
/// a path to copy from.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn skill_install(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    workspace_id: String,
    name: String,
    origin: String,
    update: bool,
    force: bool,
) -> Result<(), String> {
    let ws = workspace::open(&db, &workspace_id).map_err(|e| e.to_string())?;
    let entry = skills::catalog(&db, &shipped_dir(&app, "skills"), &skill_cache(&app)?)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|e| e.name == name && e.origin == origin)
        .ok_or("that skill is not in the catalog")?;
    if update {
        skills::update(&db, &ws, &entry, force)
    } else {
        skills::install(&db, &ws, &entry)
    }
    .map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn skill_uninstall(
    db: tauri::State<'_, db::Db>,
    workspace_id: String,
    name: String,
    force: bool,
) -> Result<(), String> {
    let ws = workspace::open(&db, &workspace_id).map_err(|e| e.to_string())?;
    skills::uninstall(&db, &ws, &name, force).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn skill_sources(db: tauri::State<'_, db::Db>) -> Result<Vec<skills::Source>, String> {
    skills::sources(&db).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn skill_add_source(db: tauri::State<'_, db::Db>, url: String) -> Result<skills::Source, String> {
    skills::add_source(&db, &url).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn skill_remove_source(app: tauri::AppHandle, db: tauri::State<'_, db::Db>, id: String) -> Result<(), String> {
    skills::remove_source(&db, &skill_cache(&app)?, &id).map_err(|e| e.to_string())
}

/// Sync a source. Network and a subprocess, so off the async executor.
#[tauri::command]
async fn skill_sync(app: tauri::AppHandle, id: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager as _;
        let cache = skill_cache(&app)?;
        skills::sync(&app.state::<db::Db>(), &cache, &id).map(|v| v.len()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Every configured MCP server, for the settings screen.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn mcp_list(db: tauri::State<'_, db::Db>) -> Result<Vec<mcp::Server>, String> {
    mcp::list(&db).map_err(|e| e.to_string())
}

/// Register a server. User-initiated only: a local server is a command this
/// machine will run, so there is no sidecar route to it.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn mcp_add(
    db: tauri::State<'_, db::Db>,
    workspace_id: Option<String>,
    name: String,
    transport: mcp::Transport,
) -> Result<mcp::Server, String> {
    mcp::add(&db, workspace_id.as_deref(), &name, &transport).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn mcp_set_enabled(db: tauri::State<'_, db::Db>, id: String, enabled: bool) -> Result<(), String> {
    mcp::set_enabled(&db, &id, enabled).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn mcp_remove(db: tauri::State<'_, db::Db>, id: String) -> Result<(), String> {
    mcp::remove(&db, &id).map_err(|e| e.to_string())
}

/// Memories for the panel, with their history when asked. Narrowest scope first.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn memory_list(
    db: tauri::State<'_, db::Db>,
    include_superseded: bool,
) -> Result<Vec<serde_json::Value>, String> {
    memory::list(&db, &serde_json::json!({"includeSuperseded": include_superseded}))
}

/// Correct a memory in place. The same guards as `remember` apply.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn memory_update(
    db: tauri::State<'_, db::Db>,
    id: String,
    subject: Option<String>,
    claim: Option<String>,
    pinned: Option<bool>,
) -> Result<(), String> {
    memory::update(&db, &serde_json::json!({"id": id, "subject": subject, "claim": claim, "pinned": pinned}))
        .map(|_| ())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn memory_delete(db: tauri::State<'_, db::Db>, id: String) -> Result<(), String> {
    memory::delete(&db, &serde_json::json!({"id": id})).map(|_| ())
}

/// The read-only markdown view of everything workmate remembers.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn memory_export(db: tauri::State<'_, db::Db>) -> Result<String, String> {
    let rows = memory::export_markdown(&db)?;
    Ok(rows
        .first()
        .and_then(|r| r["markdown"].as_str())
        .unwrap_or_default()
        .to_owned())
}

/// Where run worktrees live: workmate's data directory, never the user's tree.
fn worktree_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager as _;
    app.path().app_data_dir().map(|d| d.join("worktrees")).map_err(|e| e.to_string())
}

/// The checkout a workspace is bound to, if it is a git repository.
fn checkout_of(db: &db::Db, workspace_id: &str) -> Result<workspace::Workspace, String> {
    let ws = workspace::open(db, workspace_id).map_err(|e| e.to_string())?;
    if repo::is_repo(&ws.directory) {
        Ok(ws)
    } else {
        Err(repo::RepoError::NotARepo(ws.directory).to_string())
    }
}

/// Whether the workspace's folder is a git repository, so the UI can offer runs.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn repo_available(db: tauri::State<'_, db::Db>, workspace_id: String) -> Result<bool, String> {
    let ws = workspace::open(&db, &workspace_id).map_err(|e| e.to_string())?;
    Ok(repo::is_repo(&ws.directory))
}

/// Make the branch and worktree for a run. Workmate creates these; an agent
/// never does (ticket 007).
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn run_worktree_create(
    app: tauri::AppHandle,
    db: tauri::State<'_, db::Db>,
    workspace_id: String,
    run_id: String,
) -> Result<repo::Worktree, String> {
    let ws = checkout_of(&db, &workspace_id)?;
    let path = repo::worktree_path(&worktree_root(&app)?, &workspace_id, &run_id);
    repo::create_worktree(&ws.directory, &path, &run_id).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn run_status(
    app: tauri::AppHandle,
    workspace_id: String,
    run_id: String,
) -> Result<Vec<repo::Change>, String> {
    let path = repo::worktree_path(&worktree_root(&app)?, &workspace_id, &run_id);
    repo::status(&path).map_err(|e| e.to_string())
}

#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn run_diff(
    app: tauri::AppHandle,
    workspace_id: String,
    run_id: String,
    base: String,
) -> Result<repo::RunDiff, String> {
    let path = repo::worktree_path(&worktree_root(&app)?, &workspace_id, &run_id);
    repo::diff(&path, &base).map_err(|e| e.to_string())
}

/// Merge a run into the user's checkout. Only ever invoked by the user's
/// explicit action; it refuses on a dirty checkout or any conflict.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn run_merge(
    db: tauri::State<'_, db::Db>,
    workspace_id: String,
    run_id: String,
) -> Result<repo::MergeOutcome, String> {
    let ws = checkout_of(&db, &workspace_id)?;
    repo::merge(&ws.directory, &run_id).map_err(|e| e.to_string())
}

/// Remove a run's worktree. `abandon` also deletes the branch; `force` is the
/// only way to discard uncommitted work.
#[expect(clippy::needless_pass_by_value)]
#[tauri::command]
fn run_worktree_remove(
    db: tauri::State<'_, db::Db>,
    workspace_id: String,
    run_id: String,
    abandon: bool,
    force: bool,
) -> Result<(), String> {
    let ws = checkout_of(&db, &workspace_id)?;
    repo::remove_worktree(&ws.directory, &run_id, abandon, force).map_err(|e| e.to_string())
}

/// # Panics
/// Panics if the Tauri runtime cannot start, or if the database cannot be
/// opened or migrated — neither is recoverable, and continuing without
/// persistence would silently lose the user's work.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            use tauri::Manager as _;
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(db::Db::open(&dir.join("workmate.sqlite3"))?);
            app.manage(engine::EngineState::default());
            app.manage(runtime::Runtime::default());
            spawn_scheduler(app.handle().clone());
            app.manage(runtime::Binaries::in_dir(
                &app.path().resolve("binaries", tauri::path::BaseDirectory::Resource)?,
                &dir.join("engine"),
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_info,
            schema_version,
            start_runtime,
            sidecar_command,
            workspace_create,
            workspace_list,
            workspace_open,
            workspace_relocate,
            workspace_remove,
            permission_grants,
            permission_add,
            permission_revoke,
            permission_reply,
            permission_ruleset,
            repo_available,
            run_worktree_create,
            run_status,
            run_diff,
            run_merge,
            run_worktree_remove,
            automation_create,
            automation_list,
            automation_set_enabled,
            automation_remove,
            automation_run_now,
            automation_history,
            automation_mark_seen,
            automation_unseen,
            settings_default_model,
            settings_set_default_model,
            run_list,
            run_get,
            run_reveal_worktree,
            credential_set,
            credential_delete,
            credential_status,
            team_list,
            team_create,
            team_update_role,
            team_remove,
            pack_list,
            pack_preview,
            pack_apply,
            skill_catalog,
            skill_installed,
            skill_install,
            skill_uninstall,
            skill_sources,
            skill_add_source,
            skill_remove_source,
            skill_sync,
            mcp_list,
            mcp_add,
            mcp_set_enabled,
            mcp_remove,
            memory_list,
            memory_update,
            memory_delete,
            memory_export
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
