/** Typed wrappers over the Rust commands. No logic: the webview decides nothing durable. */
import { invoke } from "./bridge.js";
import type * as T from "./types.js";

export interface Grant {
  id: string;
  workspaceId: string;
  path: string;
  operation: "read" | "edit";
  source: string;
  createdAt: number;
}

export type Scope = { kind: "global" } | { kind: "workspace"; id: string } | { kind: "role"; id: string };

/** A command routed through Rust to the orchestrating sidecar. */
const cmd = <R = unknown>(name: string, args: Record<string, unknown> = {}): Promise<R> =>
  invoke<R>("sidecar_command", { name, args });

export const api = {
  boot: () => invoke<string>("start_runtime"),
  revealLogs: () => invoke<string>("logs_reveal"),
  engineInfo: () => invoke<{ pinnedVersion: string; sidecarPresent: boolean }>("engine_info"),

  workspaces: {
    list: () => invoke<T.WorkspaceView[]>("workspace_list"),
    create: (name: string, directory: string) => invoke<T.WorkspaceView>("workspace_create", { name, directory }),
    open: (id: string) => invoke<T.WorkspaceView>("workspace_open", { id }),
    relocate: (id: string, directory: string) => invoke<T.WorkspaceView>("workspace_relocate", { id, directory }),
    remove: (id: string) => invoke<void>("workspace_remove", { id }),
    isRepo: (workspaceId: string) => invoke<boolean>("repo_available", { workspaceId }),
  },

  runs: {
    list: (workspaceId: string) => invoke<T.RunSummary[]>("run_list", { workspaceId }),
    get: (runId: string) => invoke<T.PersistedRun>("run_get", { runId }),
    start: (a: {
      workspaceId: string;
      objective: string;
      roles: T.Role[];
      teamId?: string;
      review?: boolean;
    }) => cmd<{ runId: string; branch: string }>("run.start", a),
    say: (runId: string, text: string) => cmd("run.say", { runId, text }),
    pause: (runId: string) => cmd("run.pause", { runId }),
    resume: (runId: string) => cmd("run.resume", { runId }),
    amend: (runId: string, prompt: string) => cmd("run.amend", { runId, prompt }),
    redirect: (runId: string, roleId: string) => cmd("run.redirect", { runId, roleId }),
    veto: (runId: string) => cmd("run.veto", { runId }),
    transcript: (sessionId: string, directory: string) =>
      cmd<T.TranscriptMessage[]>("run.transcript", { sessionId, directory }),
    diff: (workspaceId: string, runId: string) => cmd<T.RunDiff>("run.diff", { workspaceId, runId }),
    status: (workspaceId: string, runId: string) => cmd<T.Change[]>("run.status", { workspaceId, runId }),
    merge: (workspaceId: string, runId: string) =>
      cmd<"upToDate" | "fastForward" | "merged">("run.merge", { workspaceId, runId }),
    archive: (workspaceId: string, runId: string) => cmd("run.archive", { workspaceId, runId }),
    abandon: (workspaceId: string, runId: string, force: boolean) =>
      cmd("run.abandon", { workspaceId, runId, force }),
    reveal: (workspaceId: string, runId: string) => invoke<string>("run_reveal_worktree", { workspaceId, runId }),
    replyPermission: (sessionId: string, permissionId: string, reply: "once" | "reject") =>
      cmd("permission.reply", { sessionId, permissionId, reply }),
  },

  models: {
    list: () => cmd<T.ProviderInfo[]>("models.list"),
    getDefault: () => invoke<{ provider: string | null; model: string | null }>("settings_default_model"),
    setDefault: (provider: string, model: string) => invoke<void>("settings_set_default_model", { provider, model }),
  },

  credentials: {
    status: (provider: string, roleId?: string, workspaceId?: string) =>
      invoke<T.CredentialStatus>("credential_status", { provider, roleId, workspaceId }),
    set: (scope: Scope, provider: string, secret: string) => invoke<void>("credential_set", { scope, provider, secret }),
    remove: (scope: Scope, provider: string) => invoke<void>("credential_delete", { scope, provider }),
  },

  teams: {
    list: () => invoke<T.Team[]>("team_list"),
    updateRole: (role: T.Role) => invoke<void>("team_update_role", { role }),
    remove: (id: string) => invoke<void>("team_remove", { id }),
  },

  packs: {
    list: () => invoke<T.Pack[]>("pack_list"),
    preview: (packId: string, workspaceId: string) => invoke<T.PackPreview>("pack_preview", { packId, workspaceId }),
    apply: (packId: string, workspaceId: string) => invoke<T.Applied>("pack_apply", { packId, workspaceId }),
  },

  skills: {
    catalog: () => invoke<T.SkillEntry[]>("skill_catalog"),
    installed: (workspaceId: string) => invoke<T.InstalledSkill[]>("skill_installed", { workspaceId }),
    install: (workspaceId: string, name: string, origin: string, update = false, force = false) =>
      invoke<void>("skill_install", { workspaceId, name, origin, update, force }),
    uninstall: (workspaceId: string, name: string, force = false) =>
      invoke<void>("skill_uninstall", { workspaceId, name, force }),
    sources: () => invoke<T.SkillSource[]>("skill_sources"),
    addSource: (url: string) => invoke<T.SkillSource>("skill_add_source", { url }),
    removeSource: (id: string) => invoke<void>("skill_remove_source", { id }),
    sync: (id: string) => invoke<number>("skill_sync", { id }),
  },

  memory: {
    list: (includeSuperseded = false) => invoke<T.Memory[]>("memory_list", { includeSuperseded }),
    update: (id: string, patch: { subject?: string; claim?: string; pinned?: boolean }) =>
      invoke<void>("memory_update", { id, ...patch }),
    remove: (id: string) => invoke<void>("memory_delete", { id }),
    export: () => invoke<string>("memory_export"),
  },

  automations: {
    list: () => invoke<T.Automation[]>("automation_list"),
    create: (a: { workspaceId: string; name: string; schedule: string; objective: string; roles: T.Role[] }) =>
      invoke<T.Automation>("automation_create", a),
    setEnabled: (id: string, enabled: boolean) => invoke<void>("automation_set_enabled", { id, enabled }),
    remove: (id: string) => invoke<void>("automation_remove", { id }),
    runNow: (id: string) => invoke<void>("automation_run_now", { id }),
    history: (automationId?: string) => invoke<T.Fire[]>("automation_history", { automationId }),
    markSeen: (fireId: string) => invoke<void>("automation_mark_seen", { fireId }),
    unseen: () => invoke<number>("automation_unseen"),
  },

  permissions: {
    grants: (workspaceId: string) => invoke<Grant[]>("permission_grants", { workspaceId }),
    add: (workspaceId: string, path: string, operation: "read" | "edit") =>
      invoke<Grant>("permission_add", { workspaceId, path, operation, source: "user" }),
    revoke: (grantId: string) => invoke<void>("permission_revoke", { grantId }),
  },

  mcp: {
    list: () => invoke<T.McpServer[]>("mcp_list"),
    add: (workspaceId: string | null, name: string, transport: { kind: "local"; command: string[]; environment?: Record<string, string> } | { kind: "remote"; url: string }) =>
      invoke<T.McpServer>("mcp_add", { workspaceId, name, transport }),
    setEnabled: (id: string, enabled: boolean) => invoke<void>("mcp_set_enabled", { id, enabled }),
    remove: (id: string) => invoke<void>("mcp_remove", { id }),
  },
};
