---
label: wayfinder:research
ticket: 002-cowork-z-architecture
subject: https://github.com/kevinlin/cowork-z
revision-read: shallow clone of `main`, package version 0.8.6 (package.json, src-tauri/Cargo.toml)
---

# Cowork-z architecture up close

All paths are relative to the cowork-z repo root unless stated otherwise. Read from a
`git clone --depth 1` of `https://github.com/kevinlin/cowork-z`; every claim below cites
the file it came from. Where `docs/architecture/architecture.md` disagrees with the code
(it does, in places — it still describes task-scoped `folder_permissions`, dropped in
migration v6), the code wins and the divergence is noted.

---

## 1. Repo and build layout

**Single package, not a monorepo in any meaningful sense.** `pnpm-workspace.yaml` declares
exactly one package: `packages: ["."]`. The Node sidecar at `src-tauri/sidecar-opencode/`
has its **own** `package.json` + `pnpm-lock.yaml` and is deliberately installed with
`pnpm install --ignore-workspace` (`src-tauri/tauri.conf.json` `build.beforeDevCommand`;
`.github/workflows/publish.yml` line 64) — it is a sibling project that happens to live
inside `src-tauri/`.

```
/                           React 19 + TS + Tailwind 3 + Radix/shadcn frontend (src/)
  vite.config.ts            aliases: @ → src, @shared → src/shared, @sidecar → src-tauri/sidecar-opencode/src
  scripts/build-sidecar.mjs OS/arch → `pnpm build:binary:<target>` dispatcher
  src-tauri/
    Cargo.toml              crate `cowork-z`, lib name `cowork_z_lib`, edition 2021
    tauri.conf.json         externalBin: ["binaries/sidecar-opencode"]
    src/                    Rust: commands/, db/, sidecar.rs, path_guard.rs, …
    sidecar-opencode/       separate Node/TS project → pkg'd single-file binary
    binaries/               (gitignored) sidecar-opencode-<rust-target-triple>[.exe]
    resources/              skills/, packs/, pack-docs/ — bundled as Tauri resources
```

Versions that matter (`package.json`, `src-tauri/Cargo.toml`, `src-tauri/sidecar-opencode/package.json`):

| | |
|---|---|
| Tauri | `tauri ~2.10` / `tauri-build ~2.5` / `@tauri-apps/api ~2.10.1` / `@tauri-apps/cli ~2.11.2` |
| Tauri plugins | `opener ~2.5`, `shell ~2.3`, `dialog ~2.7`, `updater ~2.10`, `process ~2.3` |
| Frontend | React 19.2, Vite 7.3, Tailwind 3.4 (**not** v4), react-router 8.3, zustand 5.0, zod 3.25, framer-motion 12, TypeScript ~5.8 |
| Tooling | Biome 2.3.13 via `ultracite` 7.1.3, Vitest 4.1, Husky 9 |
| Rust | `rusqlite 0.31` (`bundled`), `keyring 2`, `notify 7` + `notify-debouncer-mini 0.5`, `cron 0.17`, `reqwest 0.12`, `trash 5`, `sha2`/`hex`, `dirs 5`, `tokio 1` (`sync` only) |
| Sidecar | runtime dep is only `eventsource ^2.0.2`; built with `@yao-pkg/pkg 5.16.1` targeting **node20**; tests on Jest 29 (not Vitest) |

**How the two halves are bundled together.** The sidecar is compiled to a self-contained
executable by `pkg` and dropped into `src-tauri/binaries/` under a Rust target-triple
suffix, which is exactly what Tauri's `externalBin` mechanism expects:

```
src-tauri/sidecar-opencode/package.json
  "build:binary":       pnpm build && pkg dist/index.js --targets node20-macos-arm64  --output ../binaries/sidecar-opencode-aarch64-apple-darwin
  "build:binary:x64":   … node20-macos-x64   → …-x86_64-apple-darwin
  "build:binary:win":   … node20-win-x64     → …-x86_64-pc-windows-msvc.exe
  "build:binary:linux": … node20-linux-x64   → …-x86_64-unknown-linux-gnu
  "build:binary:linux-arm64": … node20-linux-arm64 → …-aarch64-unknown-linux-gnu
```

`scripts/build-sidecar.mjs` maps `${platform()}-${arch()}` onto those five scripts and
`execFileSync`s pnpm. Dev flow (`tauri.conf.json`):

```
beforeDevCommand: cd src-tauri/sidecar-opencode && pnpm install --ignore-workspace
                  && cd ../.. && node scripts/build-sidecar.mjs && pnpm dev
beforeBuildCommand: pnpm build          # tsc && vite build
devUrl: http://localhost:1420           # vite.config.ts: strictPort, watch ignores **/src-tauri/**
frontendDist: ../dist
```

CI (`.github/workflows/publish.yml`) has a five-way matrix (macOS arm64/x64, ubuntu-24.04,
ubuntu-24.04-arm, windows-latest), Node 22.22.0, one `pnpm build:binary:*` step per target,
then `tauri-apps/tauri-action@v1`. **The sidecar cannot be cross-compiled** — each runner
builds its own (`docs/architecture/architecture.md` §"Sidecar Build Constraints").

Notably, **OpenCode itself is not bundled**. `README.md:236-242` requires the user to
`npm install -g opencode-ai` and have `opencode` on `PATH`; the app detects its absence and
shows `src/components/layout/OpenCodeCliMissingDialog.tsx`.

---

## 2. Tauri command surface

118 commands registered in one `tauri::generate_handler![…]` block at
`src-tauri/src/lib.rs:280-436`, grouped by module under `src-tauri/src/commands/`:

| Module | Commands |
|---|---|
| `app_info` | `get_version`, `get_platform`, `get_arch`, `is_e2e_mode` |
| `tasks` | `start_task`, `cancel_task`, `abort_session`, `get_session_todos`, `get_task`, `list_tasks`, `delete_task`, `clear_task_history`, `save_task_message`, `save_task_status`, `save_task_session`, `save_task_summary`, `complete_task`, `respond_to_permission`, `reply_to_question`, `resume_session` |
| `arena` | `start_arena`, `resume_arena`, `get_arena`, `list_arenas`, `delete_arena`, `abort_arena`, `rename_arena` |
| `workspace_permissions` | `save_workspace_permission`, `get_workspace_permissions`, `remove_workspace_permission`, `get_default_folder_permissions` |
| `settings` | `get/set_debug_mode`, `get/set_user_prompt`, `get/set_mcp_servers_config`, `get_app_settings`, `get/set_theme`, `get/set_onboarding_complete` |
| `api_keys` | `get_api_keys`, `add_api_key`, `remove_api_key`, `has_api_key`, `set_api_key`, `validate_api_key`, `validate_api_key_for_provider`, `clear_api_key`, `get_all_api_keys`, `has_any_api_key` |
| `opencode_cli` | `check_opencode_cli`, `get_opencode_version` |
| `providers` | `get/set_selected_model`, `get_provider_settings`, `set_active_provider`, `get/set/remove_connected_provider`, `update_provider_model`, `get/set_provider_debug_mode`, `fetch_provider_models` |
| `ollama`/`azure_foundry`/`litellm`/`bedrock`/`copilot` | per-provider config/test/fetch-models + `copilot_oauth_authorize`, `copilot_get_models`, `copilot_disconnect` |
| `mcp` | `get_mcp_status`, `get_mcp_tools`, `connect_mcp_server`, `disconnect_mcp_server` |
| `logging` | `log_event`, `export_text_file` |
| `updates` | `check_for_update`, `install_update` |
| `files` | `read_file_content`, `trash_file`, `open_path_in_default_app`, `reveal_path_in_file_manager` |
| `workspaces` | `list_workspaces`, `get_active_workspace`, `add_workspace`, `remove_workspace`, `switch_workspace`, `read_directory`, `initialize_workspace` |
| `packs` | `packs_list`, `packs_install`, `packs_install_default` |
| `skills` | `skills_list_with_status`, `skills_get_skill_file_path` |
| `automations` | `create/update/delete/list/get_automation`, `toggle_automation_enabled`, `list_automation_runs`, `mark_run_read`, `mark_all_runs_read`, `get_automation_unread_count`, `run_automation_now`, `get_automation_next_runs`, `validate_cron` |
| `skill_repos` | `skill_repos_list/add/remove/sync/sync_all/skills`, `skills_install_from_repo`, `skills_list_installed`, `skills_delete_installed` |

Actual signatures (`src-tauri/src/commands/tasks.rs`, `…/workspace_permissions.rs`) — note
every command returns `Result<T, String>` (stringly-typed errors, no error enum):

```rust
#[tauri::command]
pub async fn start_task(config: TaskConfig, app: tauri::AppHandle,
    sidecar_state: State<'_, SidecarState>, db_state: State<'_, DbState>) -> Result<Task, String>

#[tauri::command]
pub async fn resume_session(session_id: String, prompt: String, task_id: Option<String>,
    app: tauri::AppHandle, sidecar_state: State<'_, SidecarState>,
    db_state: State<'_, DbState>) -> Result<Task, String>

#[tauri::command]
pub async fn respond_to_permission(response: PermissionResponse,
    sidecar_state: State<'_, SidecarState>, db_state: State<'_, DbState>) -> Result<(), String>

#[tauri::command]
pub async fn reply_to_question(task_id: String, request_id: String,
    answers: Vec<sidecar::QuestionAnswer>, sidecar_state: State<'_, SidecarState>) -> Result<(), String>

#[tauri::command]
pub async fn save_workspace_permission(workspace_id: String, folder_path: String,
    access_level: String, source: Option<String>, state: State<'_, DbState>,
    app: tauri::AppHandle) -> Result<(), String>

#[tauri::command]
pub async fn get_mcp_status(sidecar_state: State<'_, SidecarState>) -> Result<(), String>
```

That last one is the house pattern for anything the sidecar owns: the command returns
`()` and the *answer* arrives later as an event. Commands are fire-and-forget; state flows
back over the event channel.

`PermissionResponse` (`src-tauri/src/types.rs:102-111`):

```rust
pub struct PermissionResponse {
    pub request_id: String,
    pub task_id: String,
    pub decision: String,               // "allow" | "deny"
    pub message: Option<String>,
    pub patterns: Option<Vec<String>>,  // file or directory paths
}
```

**Events.** Only seven event names originate in Rust
(`grep 'emit' src-tauri/src/`): `show-about`, `show-keyboard-shortcuts`,
`check-for-updates` (native menu, `lib.rs:266-276`), `sidecar:process_error`,
`sidecar:process_terminated` (`sidecar.rs:696,711`), `skills:changed`,
`skills:sync_progress` (`lib.rs`, `commands/skill_repos.rs`), plus
`workspace:added`, `workspace:changed`, `workspace:fs_changed`
(`fs_watcher.rs:65`), `automation:changed`, `automation:run_started`,
`automation:run_completed`.

Everything else is a **verbatim passthrough of the sidecar's own event union**.
`sidecar.rs:393-404`:

```rust
fn tauri_event_name(event_type: &str) -> Option<String> {
    // reject empty / non [A-Za-z0-9_-] types
    Some(format!("sidecar:{}", event_type))
}
```

and the frontend subscribes through one typed helper,
`src/lib/sidecar-bridge.ts`, which imports the union across the `@sidecar` alias so the
Rust layer never needs to know the payload shapes:

```ts
export function onSidecarEvent<K extends SidecarEventType>(
  type: K, handler: (event: SidecarEventOf<K>) => void): Promise<UnlistenFn> {
  return listen<SidecarEventOf<K>>(`sidecar:${type}`, (e) => handler(e.payload));
}
```

The sidecar event union (`src-tauri/sidecar-opencode/src/types.ts:361-399`) —
these become `sidecar:<type>`:
`ready`, `pong`, `server_status`, `task_started`, `task_message`,
`task_message_partial`, `task_message_complete`, `task_progress`,
`permission_request`, `question_request`, `task_complete`, `task_error`,
`todo_updated`, `mcp_status`, `mcp_tools`, `mcp_tools_changed`,
`copilot_oauth_result`, `copilot_oauth_complete`, `copilot_models_result`,
`request_api_keys`, `log`, `error`.

Two of those Rust intercepts rather than forwards
(`SidecarSideEffect`, `sidecar.rs:815-860`):
- `request_api_keys` → answered from the keychain and **never forwarded** (credentials
  never reach the webview).
- `task_complete` → also drives `handle_task_completion_internal` directly in Rust,
  because macOS WKWebView throttles a backgrounded webview's listeners and the automation
  lifecycle must not depend on the UI being awake.

**Rust vs TypeScript split.** Rust owns: SQLite, OS keychain, path canonicalisation and
the asset-protocol scope (`path_guard.rs`), the filesystem watcher, cron scheduling and
the automation dispatch slot, `git` invocation for skill repos, provider HTTP validation
(`reqwest`), Tauri updater, native menu. The sidecar (TypeScript) owns: everything about
the OpenCode server — port, password, config files, SSE stream, session lifecycle, the
system prompt, MCP wiring, Copilot OAuth. The React layer owns: presentation and the
task/message store (`src/stores/taskStore.ts`), and re-persists messages back through
`save_task_message`. `src/lib/tauri-api.ts` is a thin typed `invoke` wrapper behind
`src/lib/tauri-api-interface.ts`.

---

## 3. Sidecar supervision

There are **two** process layers: Rust supervises the pkg'd Node sidecar; the sidecar
supervises `opencode serve`.

### Rust → sidecar (`src-tauri/src/sidecar.rs`)

- **Locating.** `app.shell().sidecar("sidecar-opencode")` (`sidecar.rs:603`) — Tauri's
  externalBin resolution. Before that, `spawn()` builds a *diagnostic* candidate map over
  `<resource_dir>/binaries/<name>` and `<cwd>/src-tauri/binaries/<name>` for the platform's
  triple list, purely for logging (`sidecar.rs:543-591`).
- **Spawning is lazy.** `start_task` does `if !manager.is_running() { manager.spawn(&app).await? }`
  (`commands/tasks.rs:154-157`). There is no spawn-at-startup.
- **Readiness is a handshake, not process liveness.**
  ```rust
  pub struct SidecarManager { child: Option<CommandChild>, ready: Arc<AtomicBool>,
                              log_file: Option<Arc<Mutex<File>>>, exited: Arc<AtomicBool> }
  pub fn is_running(&self) -> bool { self.child.is_some() && self.ready.load(SeqCst) && !self.exited.load(SeqCst) }
  ```
  `spawn()` polls every 50 ms up to `READY_TIMEOUT_MS = 15_000` for the sidecar's own
  `ready` IPC event, then kills the child and errors if it never arrives
  (`sidecar.rs:504-530, 725-750`). The comment is explicit that keying off process spawn
  loses commands written before the sidecar wires its stdin reader.
- **No automatic restart.** If the child died, the *next* `spawn()` drops the stale handle
  and respawns (`sidecar.rs:534-541`); `sidecar:process_terminated` is emitted for the UI.
  Nothing supervises it in between.
- **Transport.** JSON lines over stdin/stdout. `send_command` serialises
  `SidecarCommand` and writes `json + "\n"` (`sidecar.rs:800-812`). The stdout reader
  carries partial lines with `MAX_CARRY_BYTES = 1 MiB` before abandoning the carry
  (`sidecar.rs:245`). All sidecar stdout is also tee'd to a log file under the OpenCode log
  dir (`create_log_file`).
- **Shutdown.** `RunEvent::ExitRequested` (`lib.rs:439-449`) — *not* `Exit` — blocks on
  `manager.stop()`, which writes `{"type":"shutdown"}`, waits up to
  `MAX_WAIT_MS = 12_000` in 100 ms polls, then hard-kills. The comment explains why:
  `tauri-plugin-shell` SIGKILLs its tracked children on `Exit`, which would orphan
  `opencode serve`.

`SidecarCommand` (`sidecar.rs:53-122`) is `#[serde(tag = "type", rename_all = "snake_case")]`:
`start_task`, `resume_session`, `cancel_task`, `abort_session`, `send_permission_reply`,
`send_question_reply`, `get_session_todos`, `update_mcp_config`, `get_mcp_status`,
`get_mcp_tools`, `connect_mcp_server`, `disconnect_mcp_server`,
`copilot_oauth_authorize`, `copilot_get_models`, `copilot_disconnect`,
`api_keys_response`, `ping`, `check_server`, plus the out-of-band `shutdown`.

`StartTaskPayload` (camelCase on the wire, `sidecar.rs:138-164`):

```rust
pub struct StartTaskPayload {
    task_id: String, prompt: String,
    api_keys_fingerprint: Option<String>,   // fingerprint only, NEVER key material
    working_directory: Option<String>, model_id: Option<String>,
    folder_permissions: Option<Vec<FolderPermissionPayload>>,  // { path, accessLevel, source }
    custom_prompt: Option<String>, mcp_servers: Option<serde_json::Value>,
    skip_config: Option<bool>,              // Arena sends config once
    arena_id: Option<String>,               // suppresses sibling-session cleanup
}
```

### Sidecar → `opencode serve` (`src-tauri/sidecar-opencode/src/process-manager.ts`)

- **Locating the CLI.** `this.cliPath = options.cliPath ?? 'opencode'` — resolved off
  `PATH` (line 376). Because a Finder/Dock-launched macOS app gets a minimal `PATH`,
  `getAugmentedPath()` (line 95) shells out to `$SHELL -ilc 'echo $PATH'` against an
  allow-list of login shells (`UNIX_ALLOWED_LOGIN_SHELLS`, line 48) and merges the result
  with a curated fallback list. Rust has an independent copy of this trick in
  `commands/opencode_cli.rs:9` for the missing-CLI dialog.
- **Isolation.** `getAvailablePort()` binds `127.0.0.1:0` (with a Windows Hyper-V/WinNAT
  `netsh` excluded-range check and retry, lines 249-341); `generatePassword()` is 32
  `crypto.randomBytes` base64url'd (line 346). The password goes out as
  `OPENCODE_SERVER_PASSWORD` and every HTTP/SSE call carries
  `Authorization: Basic base64("opencode:" + password)`
  (`opencode-client.ts`, `event-stream.ts:52-55`). Rationale in
  `docs/specs/opencode-integration/plan_server-isolation.md`.
- **Spawn.** `spawn(this.cliPath, ['serve','--port',port,'--hostname',hostname], { env, cwd: OPENCODE_DATA_DIR, stdio:['ignore','pipe','pipe'], detached:false, shell: process.platform === 'win32' })`
  (line 571). API keys become provider env vars via `applyApiKeyEnv` (line 29).
- **Health check.** `waitForServer()` polls `GET /global/health` 30× at 500 ms
  (= 15 s ceiling), aborting early if `spawnError` is set or the child already exited
  (line 602).
- **Restart.** Only on credential change, and only while idle: `index.ts:104-126` restarts
  the OpenCode server when the api-keys fingerprint changed, and *defers* if sessions are
  still active. Otherwise there is no watchdog — a dead `opencode serve` surfaces as a
  failing HTTP call.
- **Shutdown.** HTTP dispose → SIGTERM → SIGKILL, with Windows killing the whole process
  tree because the child is a `cmd.exe` shim (`stopServer`, line 634 and the comment at
  682).
- **Serialisation.** `command-queue.ts` is a FIFO promise chain over stdin commands, with
  `ping` and `api_keys_response` deliberately handled *outside* the queue —
  `api_keys_response` resolves a promise a queued command awaits, so queueing it deadlocks.

### Streaming to the UI

`opencode serve` → SSE on **`/global/event`** (not `/event`) — `event-stream.ts:21-70`
documents why: in OpenCode 1.14.x `/event` is bound to per-request Effect/InstanceState
lifecycle and severs after the first event; `/global/event` rides a plain Node
EventEmitter. Envelope is `{ directory?, project?, payload: OpenCodeEvent }`. Reconnect is
exponential with `MAX_RECONNECT_INTERVAL_MS = 60_000`.

`session-manager.ts` translates OpenCode events to sidecar events — e.g.
`message.part.updated` with a `delta` accumulates into `managed.textAccumulator` and emits
`message-partial` `{ taskId, messageId, textSoFar, delta, isStreaming }`
(lines 160-181). So the chain for one token is:

```
opencode serve --port <rand>  --SSE/global/event-->  sidecar (Node)
  --JSON line on stdout-->  Rust stdout reader  --app.emit("sidecar:task_message_partial")-->
  webview listen()  -->  src/lib/sidecar-bridge.ts  -->  src/stores/taskStore.ts
```

Both the partial text *and* a full `textSoFar` are sent each tick; the UI throttles with
`src/hooks/useThrottledValue.ts`.

---

## 4. Persistence

**Driver:** `rusqlite 0.31` with the `bundled` feature (SQLite compiled in; no system
dependency). One `Connection` behind a `std::sync::Mutex` in `DbState`
(`src-tauri/src/db/mod.rs:23-26`) — a single shared connection, not a pool.

**Location** (`db/mod.rs:28-46`): `app.path().app_data_dir()` + `cowork-dev.db` under
`#[cfg(debug_assertions)]`, else `cowork.db`. On macOS that is
`~/Library/Application Support/cowork-z/cowork.db`
(`docs/architecture/architecture.md` §4.1).

**Pragmas** (`db/mod.rs:56-62`): `journal_mode = WAL`, `foreign_keys = ON`.

**Migration approach** (`db/migrations.rs`): a hand-rolled linear ladder. `CURRENT_VERSION
= 8`; the version lives in `schema_meta(key='version')`; `MIGRATIONS: &[(i32, fn(&Connection) -> Result<(), String>)]`
is applied in order for every `stored_version < version`. Each step runs **in its own
transaction** so a mid-step failure rolls back the DDL *and* the version bump
(`run_migrations`, lines 408-441). A stored version *newer* than the app is a hard error
("Please upgrade the app"). Four unit tests cover fresh/idempotent/newer/rollback and an
explicit expected-tables list.

Schema as it stands at v8 (DDL quoted from `db/migrations.rs`):

```sql
-- v1
CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE app_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    debug_mode INTEGER NOT NULL DEFAULT 0,
    onboarding_complete INTEGER NOT NULL DEFAULT 0,
    selected_model TEXT, ollama_config TEXT, litellm_config TEXT,
    azure_foundry_config TEXT,
    user_prompt_enabled INTEGER NOT NULL DEFAULT 0, user_prompt_text TEXT,
    mcp_servers_config TEXT, theme_id TEXT);                 -- + last_workspace_id (v2)

CREATE TABLE provider_meta (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    active_provider_id TEXT, debug_mode INTEGER NOT NULL DEFAULT 0);

CREATE TABLE providers (
    provider_id TEXT PRIMARY KEY,
    connection_status TEXT NOT NULL DEFAULT 'disconnected',
    selected_model_id TEXT,
    credentials_type TEXT NOT NULL, credentials_data TEXT,
    last_connected_at TEXT, available_models TEXT);

CREATE TABLE tasks (
    id TEXT PRIMARY KEY, prompt TEXT NOT NULL, summary TEXT, status TEXT NOT NULL,
    session_id TEXT, created_at TEXT NOT NULL, started_at TEXT, completed_at TEXT);
    -- + workspace_id TEXT REFERENCES workspaces(id)         (v2)
    -- + arena_id TEXT REFERENCES arenas(id), arena_slot INTEGER, model_id TEXT (v5)
    -- + automation_run_id TEXT REFERENCES automation_runs(id) ON DELETE SET NULL (v7)

CREATE TABLE task_messages (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    type TEXT NOT NULL, content TEXT NOT NULL,
    tool_name TEXT, tool_input TEXT,
    timestamp TEXT NOT NULL, sort_order INTEGER NOT NULL);   -- + tool_output TEXT (v4)

CREATE TABLE task_attachments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL REFERENCES task_messages(id) ON DELETE CASCADE,
    type TEXT NOT NULL, data TEXT NOT NULL, label TEXT);

-- v2
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,                    -- "ws_<uuid v4>"
    folder_path TEXT NOT NULL UNIQUE,       -- canonicalised absolute path
    display_name TEXT NOT NULL,
    created_at INTEGER NOT NULL, last_opened_at INTEGER NOT NULL);

-- v3
CREATE TABLE skill_repos (
    id TEXT PRIMARY KEY, url TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
    branch TEXT NOT NULL DEFAULT 'main',
    auth_token_key TEXT,                    -- keychain id, not the token
    last_synced_at TEXT, last_sync_error TEXT, created_at TEXT NOT NULL);

CREATE TABLE repo_skills (
    repo_id TEXT NOT NULL REFERENCES skill_repos(id) ON DELETE CASCADE,
    skill_path TEXT NOT NULL, skill_id TEXT NOT NULL, name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '', category TEXT NOT NULL DEFAULT 'General',
    PRIMARY KEY (repo_id, skill_path));

-- v5
CREATE TABLE arenas (
    id TEXT PRIMARY KEY, prompt TEXT NOT NULL,
    workspace_id TEXT REFERENCES workspaces(id),
    created_at TEXT NOT NULL, completed_at TEXT);

-- v6  (replaces task-scoped folder_permissions, which is DROPped here)
CREATE TABLE workspace_permissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    folder_path TEXT NOT NULL,
    access_level TEXT NOT NULL DEFAULT 'read-write',   -- 'read-write' | 'read-only'
    source TEXT NOT NULL DEFAULT 'adhoc',              -- 'workspace' | 'user' | 'adhoc'
    created_at TEXT NOT NULL,
    UNIQUE(workspace_id, folder_path));

-- v7
CREATE TABLE automations (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL, prompt TEXT NOT NULL,
    schedule_cron TEXT NOT NULL, schedule_display TEXT NOT NULL,
    provider_id TEXT NOT NULL, model_id TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL);

CREATE TABLE automation_runs (
    id TEXT PRIMARY KEY,
    automation_id TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    has_findings INTEGER NOT NULL DEFAULT 0, is_read INTEGER NOT NULL DEFAULT 0,
    started_at TEXT, completed_at TEXT);

-- v8: UPDATE skill_repos SET url = SUBSTR(url,1,LENGTH(url)-4) WHERE url LIKE '%.git'
```

Indexes: `idx_tasks_created_at(created_at DESC)`, `idx_messages_task_id`,
`idx_workspaces_last_opened(last_opened_at DESC)`, `idx_tasks_workspace_id`,
`idx_repo_skills_repo_id`, `idx_arenas_workspace_id`, `idx_arenas_created_at`,
`idx_tasks_arena_id`, `idx_workspace_permissions_workspace_id`,
`idx_automations_workspace_id`, `idx_automation_runs_automation_id`,
`idx_automation_runs_status`.

**What lives where:**

| Store | Contents |
|---|---|
| SQLite | tasks, the app's own copy of the transcript (`task_messages`, `task_attachments`), workspaces, workspace permission grants, provider connection metadata + available model lists (JSON in TEXT), MCP server config (JSON in `app_settings.mcp_servers_config`), automations + runs, skill repo registry and discovered-skill index, all app settings |
| OS Keychain | every secret — API keys, Bedrock credentials, skill-repo auth tokens (only the keychain *id* is in SQLite) |
| Disk, app-owned | `~/.local/share/cowork-z/opencode/{opencode.json,config.json}` (the OpenCode config the sidecar writes — `paths.ts:getAppOpenCodeConfigDir`); sidecar log under `~/.local/share/opencode/log`; `<app_data>/skill-repo-cache/<derived>` git clones; `<app_data>/packs` or `~/Cowork-Z Packs` for installed packs |
| Disk, OpenCode-owned | the actual agent session/message store, keyed by the workspace directory. Cowork-z never reads it directly — it re-queries the server over HTTP |
| Disk, user-owned | the workspace folder itself |
| In memory | sidecar `sessions` map, `sessionToTask`, `textAccumulator`; Rust `SidecarState`, `FsWatcherState`, `AutomationSchedulerRegistry`, `DispatchSlot`, `PendingUpdate`; React zustand stores |

Note the deliberate double-storage of the transcript: SQLite holds the app's own render-ready
history, OpenCode holds the real session. Resume goes through OpenCode
(`resume_session` → `POST /session/{id}/message`), not through SQLite.

---

## 5. Workspace on-disk layout

**A workspace is just a folder.** There is no manifest, dotfile or per-workspace config
file — the workspace record lives entirely in SQLite (`workspaces` table, §4). The app
writes *nothing* into the workspace folder.

```
<workspace folder>/            ← any user folder; default ~/Downloads on first launch
├── Input/                     permission edit: deny   — immutable source material
├── Output/                    permission edit: allow  — the agent's scratchpad
│   └── <category>/            every new file must live under a category subfolder
├── Misc/                      permission edit: ask    — assets, curated scripts/prompts
│   └── <topic>/
└── Artefacts/                 permission edit: ask    — governed deliverables promoted from Output/
    └── <category>/            mirrors Output/'s category layout
```

Crucially, **the four convention folders are created by the agent, not by the app.**
`src-tauri/sidecar-opencode/src/config-builder.ts:59-64` instructs the model that its
FIRST action in a workspace is a single idempotent
`mkdir -p "<ws>/Input" "<ws>/Output" "<ws>/Misc" "<ws>/Artefacts"` (PowerShell
`New-Item -Force` on Windows), and explains that this is the *only* way `Input/` can be
created at all, because `edit: deny` blocks the write/edit tools there while bash is not
gated by the `edit` permission. There is no Rust code that creates these folders — a grep
for `Artefacts` outside docs/tests hits only `config-builder.ts`.

Workspace creation (`commands/workspaces.rs`, `workspace_validator.rs`):
`validate_and_canonicalize_workspace_path` requires an absolute path, `canonicalize()`s it
(so a symlink can never alias the validated tree), strips the Windows `\\?\` verbatim
prefix, then checks a platform blocklist (`/`, system prefixes on Unix; drive roots etc. on
Windows). `display_name` is the folder basename. `initialize_workspace` restores
`last_workspace_id`, falls back to creating `~/Downloads` as a workspace, and starts the
`notify` watcher.

**Sessions and history are not in the workspace folder.** Session state lives in
OpenCode's own store keyed by `?directory=<workspace path>`; the app's copy lives in
SQLite scoped by `tasks.workspace_id`. Switching workspaces reconfigures one shared
sidecar rather than starting a second one
(`docs/specs/workspace-as-folder/design_workspace-as-folder.md:76-98`): `switch_workspace`
updates `last_workspace_id`, sends the new working directory + permission rules, and the
sidecar's `EventStream.reconnectWithDirectory()` re-scopes the SSE feed. Every OpenCode
call that touches a session — `PATCH /config`, `POST /permission/{id}/reply`,
`POST /question/{id}/reply` — must carry `?directory=`, or the server bootstraps a fresh
instance in its default directory and the waiting session hangs forever (same doc, lines
96-102; implemented in `opencode-client.ts:91-224`).

---

## 6. Permission model

**Two layers, and neither of them is a sandbox.**

**Layer 1 — grants in SQLite.** A row in `workspace_permissions` is
`(workspace_id, folder_path, access_level, source)` with `UNIQUE(workspace_id, folder_path)`
and upsert-on-conflict (`db/workspace_permissions.rs:20-35`). `source` is the interesting
field: `workspace` (synthesised, never stored — see below), `user` (added from Settings),
`adhoc` (auto-persisted when the user clicks Allow on a runtime prompt). Grant paths are
canonicalised and validated by `path_guard::validate_grant_path` *before* persisting, and
each save calls `path_guard::sync_asset_scope(&app, &conn)` so newly granted folders
become loadable through Tauri's `asset:` protocol. The static scope in
`tauri.conf.json` is deliberately empty (`"assetProtocol": { "enable": true, "scope": [] }`)
and populated only at runtime from the DB (`lib.rs:44-49`).

**Layer 2 — enforcement, which happens inside OpenCode.** On every `start_task`, Rust
loads the workspace's grants and **prepends a synthetic `source: "workspace"` grant for the
workspace folder itself** (`commands/tasks.rs:120-132`) — the workspace root is trusted
implicitly and is not a DB row. That list is shipped to the sidecar, which compiles it into
an OpenCode `permission` config (`config-builder.ts:162-221`):

```ts
const permissionConfig: PermissionConfig = { doom_loop: 'deny' };
const externalDirRules = { '*': 'ask' };      // deny-by-default for anything outside
const editRules        = { '*': 'ask' };
// for source === 'workspace' (last matching pattern wins — general first, overrides last):
externalDirRules[ws] = 'allow';  readRules[ws] = 'allow';
editRules[ws]                = 'allow';
editRules[ws + '/Input']     = 'deny';   editRules[ws + '/Input/*']     = 'deny';
editRules[ws + '/Output']    = 'allow';  editRules[ws + '/Output/*']    = 'allow';
editRules[ws + '/Misc']      = 'ask';    editRules[ws + '/Misc/*']      = 'ask';
editRules[ws + '/Artefacts'] = 'ask';    editRules[ws + '/Artefacts/*'] = 'ask';
// otherwise:
externalDirRules[p] = 'allow'; readRules[p] = 'allow';
editRules[p] = accessLevel === 'read-write' ? (source === 'adhoc' ? 'allow' : 'ask') : 'deny';
```

So a `user`-added read-write folder still prompts on every edit, while an `adhoc` folder
the user already approved does not prompt again on resume. Note the `Input/` deny applies
only to the `write`/`edit` tools — bash is explicitly outside it, by design.

**Runtime prompt flow, tool call → UI → back:**

```
agent tool call outside the rules
  → OpenCode emits SSE permission.asked { id, sessionID, permission, patterns, metadata }
  → session-manager.ts:184-196 maps sessionID→taskId, emits 'permission-request'
  → sidecar writes {"type":"permission_request","taskId","payload":{id,sessionId,permission,patterns,metadata}}
  → Rust forwards verbatim as Tauri event `sidecar:permission_request`
  → src/lib/sidecar-bridge.ts → src/components/chat/PermissionModal.tsx
  → user clicks Allow/Deny
  → invoke("respond_to_permission", { response: PermissionResponse })
  → commands/tasks.rs:532-600 — on "allow", for each pattern: if it is a dir use it, else
    its parent; validate_grant_path; save_workspace_permission(source="adhoc")
  → SidecarCommand::SendPermissionReply { taskId, payload:{ requestId, reply, message } }
    where reply ∈ "once" | "always" | "reject"
  → session-manager.ts:520 → POST /permission/{id}/reply?directory=<ws>
  → agent unblocks
```

The persisted adhoc grant is what makes the *next* session not ask again: it is compiled
into `editRules[path] = 'allow'` on the following `start_task`.

`question_request` / `reply_to_question` is the identical shape for the agent's
multiple-choice questions (`QuestionAnswer { labels: Vec<String>, custom_text: Option<String> }`,
flattened to `string[][]` before hitting OpenCode — `session-manager.ts:527-537`).

**What is *not* enforced:** nothing constrains the agent at the OS level. There is no
sandbox, no seccomp, no separate uid; `opencode serve` runs as the user with the user's
`PATH`. The defences are the OpenCode permission ruleset, the random port + basic auth,
`path_guard` on what the *webview* may read, and the Tauri capability files
(`src-tauri/capabilities/default.json`, `desktop.json`, `skills.json`) plus a tight CSP in
`tauri.conf.json` (`connect-src ipc: http://ipc.localhost` only).

---

## 7. Credentials

**Integration:** the `keyring` crate v2 (`src-tauri/Cargo.toml`), wrapped in
`src-tauri/src/secure_storage.rs` — macOS Keychain, Windows Credential Manager, Linux
Secret Service.

**Service name is profile-scoped** so dev and release never collide
(`secure_storage.rs:11-17`):

```rust
fn service_name() -> &'static str {
    if cfg!(debug_assertions) { "com.kevinlin.cowork-z-dev" } else { "com.kevinlin.cowork-z" }
}
```

**Account = provider id.** `Entry::new(service_name(), provider)`. The provider list
(`secure_storage.rs:20-33`) is: `anthropic`, `openai`, `google`, `xai`, `ollama`,
`deepseek`, `zai`, `azure-foundry`, `bedrock`, `litellm`, `openrouter`, `custom`.
`migrate_legacy_azure_foundry_key()` runs before anything reads keys, moving an old
`azureFoundry` entry to the canonical id and deleting the legacy one — otherwise a UI
delete would leave a resurrectable entry behind (`lib.rs:38-40`).

**What is stored:** the raw API key string per provider; for Bedrock a serialised
`BedrockCredentials { access_key_id, secret_access_key, region }` (`sidecar.rs:42-47`);
for skill repos, a PAT under the id recorded in `skill_repos.auth_token_key`. Nothing
secret is ever in SQLite — `providers.credentials_data` holds non-secret config only. The
frontend only ever receives a masked prefix (`get_key_prefix`: first 8 chars + `…`).

**Scoping is global, not per-workspace.** There is exactly one key per provider for the
whole app. Per-workspace or per-agent-role credentials do not exist. Selection is global
too: `provider_meta.active_provider_id` + `providers.selected_model_id`, resolved in
`start_task` (`commands/tasks.rs:16-46`) with a fallback scan for any connected provider.

**The credential bridge is deliberately narrow** (annotated "2026-06-12 review #5"):
- `start_task`/`resume_session` carry only `apiKeysFingerprint` — a hash, no key material
  (`sidecar::current_api_keys_fingerprint`, `sidecar.rs:950`).
- When the sidecar is about to (re)spawn `opencode serve` and the fingerprint differs from
  what it applied, it emits `request_api_keys { requestId }`.
- Rust intercepts that event, reads the keychain, and replies over stdin with
  `api_keys_response { requestId, apiKeys, fingerprint }`. **It is never forwarded to the
  webview** (`sidecar.rs:815-822`).
- The sidecar turns them into env vars on the `opencode serve` child
  (`process-manager.ts:29 applyApiKeyEnv`) and never logs them; `redact.ts` and
  `logger.ts` gate payload logging.
- The OpenCode server's own password is never logged either — only its length
  (`process-manager.ts:520`).

Key *changes* only take effect on server restart, and the restart is deferred while
sessions are active (`index.ts:104-126`).

---

## 8. MCP, automations, starter packs, skills catalog

### MCP — inherited from OpenCode, with an app-built config surface

| | |
|---|---|
| Inherited | everything at runtime: connection, tool discovery, OAuth, transport. OpenCode reads `mcp` from its config |
| Built in app | the config UI, storage, and a passthrough command surface |
| Where | UI `src/components/settings/Mcp*.tsx`, `src/hooks/useMcpRuntime.ts`; types `src/shared/types/mcpSettings.ts`; storage `app_settings.mcp_servers_config` (JSON TEXT); commands `src-tauri/src/commands/mcp.rs` (61 lines, pure forwarding); delivery `config-builder.ts` + `process-manager.ts:writePreStartConfig` |

Config shape (`src/shared/types/mcpSettings.ts`):
`{ type: 'local'|'remote', command?: string[], url?: string, enabled?: boolean, environment?, headers?, oauth?: McpOAuthConfig|false, timeout? }`.

Delivery is doubled: written into the app-private `opencode.json`/`config.json`
*before* spawn, **and** passed as `OPENCODE_CONFIG_CONTENT` (OpenCode's
highest-priority config source) so a post-disposal config re-read does not lose it
(`process-manager.ts:552-568`). The honest caveat is in the code:
"OpenCode does NOT dynamically reload MCP servers from PATCH /config, so changes only take
effect on next server restart" (`process-manager.ts:475-479`). `get_mcp_status` /
`get_mcp_tools` return `()` and the answer arrives as `sidecar:mcp_status` /
`sidecar:mcp_tools` / `sidecar:mcp_tools_changed`.

### Automations — entirely app-built

Nothing here comes from OpenCode. `src-tauri/src/`:

- `cron_schedule.rs` — wraps the `cron 0.17` crate; `validate_cron` is exposed as a command.
- `automation_scheduler.rs` (471 lines) — an `AutomationSchedulerRegistry` giving **one
  OS thread per enabled automation**, each sleeping on a `Condvar` until its next fire time,
  then re-sleeping. `reload_all` runs 5 s after startup (`lib.rs:69-75`).
- `dispatch_slot.rs` — a single global `AtomicBool` CAS slot with an RAII `SlotGuard`: "the
  single 'one automation run at a time' execution slot (v1 sequential model)". Concurrent
  automation runs are structurally impossible.
- `automation_dispatch.rs` — builds the dispatch context and calls `start_task` internally.
- `db/automations.rs` (403 lines) + tables `automations`, `automation_runs`,
  `tasks.automation_run_id`.
- UI: `src/components/landing/Automation*.tsx`, `src/components/sidebar/AutomationRuns*.tsx`,
  `src/stores/automationStore.ts`, `src/lib/cron-utils.ts`. Findings surface as an unread
  count (`automation_runs.is_read`, `has_findings`).

Completion is driven from **Rust**, not the frontend, precisely so a backgrounded window
cannot stall the slot release (`sidecar.rs:823-845`).

Spec: `docs/specs/automations/design_automations.md`, `plan_dispatch-slot-raii-pure-con-module.md`.

### Starter packs — entirely app-built, shipped as Tauri resources

Eight packs live as literal file trees in `src-tauri/resources/packs/<pack-id>/` with
docs split into `src-tauri/resources/pack-docs/<pack-id>/{START_HERE,PACK_INFO,PROMPTS,EXPECTED_OUTPUTS[,CONTRIBUTING]}.md`,
bundled via `tauri.conf.json` `bundle.resources`. `src-tauri/src/commands/packs.rs`
(258 lines) hardcodes the `PackMeta` list in `list_packs()` (id, title, description,
complexity, time_estimate, tags) and `install_pack` copies
`<resource_dir>/resources/packs/<id>` into `~/Cowork-Z Packs` (or `<app_data>/packs`),
de-duplicating with a `-1`, `-2` suffix, then copies the doc files alongside. No OpenCode
involvement whatsoever. UI: `src/components/landing/StarterPacks.tsx`.

### Skills catalog — OpenCode's discovery, the app's distribution

Skills are plain `SKILL.md` directories, which is an OpenCode/Claude-style convention the
app *conforms to* rather than implements.

- **Discovery at runtime** is OpenCode's: anything under `~/.config/opencode/skills/` is
  picked up automatically. The app just installs into that directory
  (`commands/skills.rs:106-109 opencode_skills_dir()`), and also supports
  `~/.claude/skills` and `~/.agents/skills`, plus workspace-local
  `.opencode/skills`, `.claude/skills`, `.agents/skills`
  (`commands/skills.rs:193-226`).
- **Everything else is app-built.** `skill_discovery.rs` (352 lines) walks a repo and
  parses YAML frontmatter out of `SKILL.md`; `git_ops.rs` (305 lines) shells out to the
  system `git` for clone/pull with optional token auth; `db/skill_repos.rs` +
  `commands/skill_repos.rs` (608 lines) manage the repo registry and install/delete;
  `sha2`/`hex` checksums drive the `needs_update` flag. Repos are cloned into
  `<app_data>/skill-repo-cache/<derive_cache_dir_name(url)>` and re-synced on a background
  thread 3 s after launch (`lib.rs:110-219`), emitting `skills:sync_progress` and
  `skills:changed`.
- **Curated list** is a hardcoded TS array: `src/components/landing/curatedSkillRepos.ts`
  (anthropics/skills, anthropics/knowledge-work-plugins, openai/skills,
  vercel-labs/skills, …). UI in `src/components/landing/SkillsCatalog.tsx` and a separate
  `skills` window (`src/pages/SkillsManager.tsx`, capability
  `src-tauri/capabilities/skills.json`).
- One skill is bundled and force-installed at every startup:
  `src-tauri/resources/skills/opencode-server-api/SKILL.md` is copied to
  `~/.config/opencode/skills/opencode-server-api/SKILL.md` on setup (`lib.rs:77-108`) so
  the agent can introspect its own server.

---

## 9. Licence and reuse

**Licence: MIT.** `LICENSE` is the verbatim 21-line MIT text, headed:

> MIT License
>
> Copyright (c) 2025-present Kevin Lin and contributors

`README.md:362-364` repeats it. There is no CLA, no `NOTICE` file, no additional
patent or trademark grant, and no per-file licence headers.

**What that permits workmate to do.** Everything, essentially: use, copy, modify, merge,
publish, distribute, sublicense and sell, including in a closed-source or differently
licensed product. MIT is not copyleft — workmate may adopt cowork-z code verbatim, adapt
it, or reimplement it, and workmate's own licence may be anything.

**The concrete obligation — one clause, and it is not optional.** The MIT terms require
that "the above copyright notice and this permission notice shall be included in all copies
or substantial portions of the Software." In practice, for workmate:

1. If any cowork-z source is copied or adapted into workmate — even a single non-trivial
   file such as `db/migrations.rs`, `config-builder.ts` or `process-manager.ts` — workmate
   must ship the full MIT text together with `Copyright (c) 2025-present Kevin Lin and contributors`.
   The normal shape is a `THIRD-PARTY-LICENSES.md` (or `licenses/cowork-z-MIT.txt`) in the
   repo **and** inside the distributed app bundle, since "all copies" includes the binary.
2. Attribution must survive into the installer/DMG, not just the repo. An "About" or
   "Open Source Licenses" panel is the usual vehicle; cowork-z itself has
   `src/components/layout/AboutDialog.tsx` for this.
3. Keep the notice even where code was adapted rather than copied verbatim — "substantial
   portions" covers derivative files.
4. Ideas, architecture, API shapes and schema *designs* are not copyrightable; a clean
   reimplementation from this research note carries no obligation. Given workmate is "a
   similar app, not a fork", the cheapest posture is: **reimplement by default, and add the
   attribution file the moment any file is actually derived.** Decide this per file and
   record it, rather than deciding it once at the end.

**Flag — things we must not copy, MIT notwithstanding.** The MIT grant covers "the
Software"; it does not launder third-party or non-code material that happens to sit in the
tree:

- **`public/assets/ai-logos/*.svg`** — Anthropic, OpenAI, Google, AWS Bedrock, Azure, xAI,
  GitHub Copilot, DeepSeek, Ollama, OpenRouter, LiteLLM, Z.ai, Vertex marks. These are
  **third-party trademarks**. Kevin Lin cannot MIT-license someone else's logo. Reuse is a
  trademark question governed by each vendor's brand guidelines, not by the MIT file.
  Re-source them from each vendor's own brand kit, or use neutral iconography.
- **`public/fonts/DMSans-*.ttf`** — DM Sans is SIL OFL 1.1, a separate licence with its own
  notice requirement and a reserved-font-name rule. Ship it under OFL with its own licence
  file, or pull it from Google Fonts at build time.
- **`src-tauri/icons/*`, `public/assets/logo*.png`, `assets/Screenshot_*.png`** — the
  Cowork-Z product identity. Copying the app icon or screenshots would make workmate look
  like cowork-z and is a trademark/passing-off problem even though the bits are MIT.
  Also **do not reuse the bundle identifier** `com.kevinlin.cowork-z` or the keychain
  service name derived from it (`src-tauri/src/secure_storage.rs:11-17`), and **never** the
  updater `pubkey` or endpoint in `src-tauri/tauri.conf.json` — that key signs *their*
  releases.
- **`src-tauri/resources/packs/**` and `pack-docs/**`** — the starter-pack corpora
  (legal templates, research papers, financial samples, the micro-drama scripts). Provenance
  is unstated in the repo; some read like curated or generated third-party content. Treat as
  unclear provenance and author workmate's own packs rather than copying these.
- **`docs/specs/opencode-integration/opencode-api.json`** — an OpenCode API schema dump.
  That is OpenCode's artefact, not cowork-z's; take it from OpenCode upstream under
  OpenCode's own licence.
- **`.claude/`, `.impeccable/design.json`, `docs/review/*`** — the author's private tooling
  and review artefacts. Legally copyable, but they encode cowork-z's process, not ours.

Nothing in the repo is GPL/AGPL-contaminated, and OpenCode is *not* vendored — the user
installs `opencode-ai` from npm themselves (`README.md:236`), so workmate inherits no
redistribution obligation for the engine either. That is a genuinely good property worth
keeping.

---

## Divergence notes

These are the places where copying cowork-z's design would actively obstruct workmate's
three differentiators. Each is a concrete decision workmate has to take differently, with
the cowork-z artefact that forces the issue.

**1. One task per sidecar, one sidecar per app — the agent team has nowhere to live.**
`session-manager.ts:319-338` deletes every other session the moment a new task starts
("clean up stale sessions"), and the only escape hatch is `arenaId`, which exists purely so
Arena's three columns can coexist. `dispatch_slot.rs` then hard-caps *automation* concurrency
at exactly one via a global `AtomicBool`. The whole stack assumes **one live agent at a
time**. Workmate's collaborating team needs N concurrent sessions that outlive each other,
so the "stale session cleanup" heuristic has to be replaced from the start with explicit
session ownership — a team/run aggregate that owns its member sessions — and `DispatchSlot`
has to become a bounded pool keyed by run, not a single boolean.

**2. Arena is parallelism, not collaboration — do not mistake it for a head start.**
`commands/arena.rs:141-267` hardcodes `config.models.len() != 3`, gives each slot an
independent OpenCode session over the same prompt and workspace, and provides **no channel
between them**: no shared scratch, no message passing, no supervisor. Slots 1 and 2 even
set `skip_config: true` so they inherit slot 0's config rather than negotiating their own.
Reusing Arena as the agent-team substrate would bake in "three isolated clones racing"
when workmate needs "several specialists exchanging work". The UI (`ArenaColumns.tsx`) is
worth studying; the execution model is not.

**3. No agent role abstraction at all — and OpenCode fought them on it.**
`config-builder.ts:224-228` is explicit: "agent/default_agent are NOT sent here. OpenCode
1.1.48 ignores custom agent names and falls back to the built-in 'build' agent. Instead,
the system prompt is injected directly via the `system` field on each sendMessage call."
So cowork-z has exactly one agent persona, expressed as a ~100-line prompt string baked
into a TypeScript function. Workmate needs per-role prompts, tools and models as *data*.
That means a first-class role/agent entity in the domain model and the DB (cowork-z has
none — no `agents` table anywhere in `db/migrations.rs`), and a decision about whether to
keep fighting OpenCode's agent handling or keep the per-message `system` injection trick
and layer roles above it. This is also where the `providers` table's single global
`active_provider_id` + one `selected_model_id` breaks down: a team wants a cheap model for
the researcher and an expensive one for the reviewer.

**4. Everything is workspace-scoped by construction — memory has no home.**
A grep for "memory" across the repo returns nothing but incidental matches. More
structurally: `tasks.workspace_id`, `workspace_permissions.workspace_id`,
`automations.workspace_id` and `arenas.workspace_id` all carry
`ON DELETE CASCADE` or a workspace FK, the session list is filtered by active workspace,
and `session-manager` re-scopes the SSE stream by `?directory=` on every switch. There is
no table, index or code path where knowledge outlives a workspace. Workmate must add a
**workspace-independent** store from migration v1 — memory rows that are *referenced by*
workspaces rather than owned by them — otherwise the cascade semantics that cowork-z
relies on will delete our differentiator along with a removed folder. Related: cowork-z's
`repo_skills` table is the closest thing to a cross-workspace knowledge index, and it is
one-way (synced from git, never written by the agent); it is not a model to copy for memory.

**5. The four-folder convention is prompt-enforced and repo-hostile.**
`config-builder.ts:59-99` makes `Input/ Output/ Misc/ Artefacts/` the agent's first act in
every workspace, and `editRules` hardwires them. In a *git repository* — workmate's target
workspace — this is wrong twice over: it litters the repo root with four folders that do not
belong in the tree, and it maps the agent's write surface onto a folder taxonomy instead of
onto tracked/untracked/staged state. Worse, the design's own escape hatch is that
"bash is not gated by the `edit` permission" (`config-builder.ts:64`), so the folder
governance is advisory for any agent that can run a shell — unacceptable when the thing
being protected is the user's source tree. Workmate should drop the four-folder convention
entirely and derive the permission surface from git: the working tree is writable, `.git/`
is not, and destructive operations are mediated by commands rather than by `edit` rules.

**6. Git is present only as a skill-repo fetcher — the repo-native workflow is greenfield.**
`git_ops.rs` (305 lines) is `Command::new("git")` for `clone`/`pull` of skill repositories
into `<app_data>/skill-repo-cache/`, with token auth. It never touches the workspace.
Nothing in the schema models a branch, commit, diff or PR; `tasks` has no git columns;
`fs_watcher.rs` debounces raw filesystem events with no notion of tracked state. Workmate's
git integration is therefore new construction, and the shelling-out approach in `git_ops.rs`
is the wrong starting point for it — status/diff/stage on every keystroke wants `gitoxide`
or `git2` in-process, not a `Command` per query. `git_ops.rs` is still a fine model for the
*credentialled clone* case (keychain id in the DB, token passed at call time).

**7. Persisting the transcript twice will not survive a team.**
Cowork-z keeps its own render-ready copy in `task_messages`/`task_attachments` while
OpenCode independently owns the real session, and the frontend re-persists messages through
`save_task_message`. With one agent this is merely redundant. With N collaborating agents
it becomes a consistency problem — whose ordering wins, what `sort_order` means across
sessions, and what happens when the webview is throttled (cowork-z already had to move
`task_complete` handling into Rust for exactly this reason, `sidecar.rs:823-845`). Workmate
should decide up front on a single source of truth for conversation state, and make Rust —
not the webview — the only writer.

**8. Credentials are global; a team needs them scoped.**
`secure_storage.rs` keys keychain entries by provider id alone, and model selection resolves
from one global `active_provider_id` (`commands/tasks.rs:16-46`). There is no per-workspace
or per-role credential. Keep the narrow fingerprint bridge (§7) — it is genuinely good, and
worth copying wholesale in design if not in code — but widen the key identity to
`(provider, scope)` before any of it ships, because retrofitting the keychain account format
later means a migration across the OS keychain, which is far nastier than a SQLite migration.

**Worth adopting as-is.** For balance: the api-keys fingerprint bridge, the `ready`-event
handshake instead of spawn-as-readiness, the `ExitRequested`-not-`Exit` shutdown ordering,
per-migration transactions with a rollback test, random port + per-launch password + HTTP
basic auth on the engine, the empty static `assetProtocol.scope` populated at runtime from
granted folders, and the verbatim `sidecar:{type}` event passthrough with the typed
`@sidecar` union shared across the Rust boundary. Each of those is a scar from a real
review finding (the comments cite dates), and re-deriving them would cost workmate weeks.
