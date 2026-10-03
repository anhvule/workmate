/** Shapes as Rust serialises them (camelCase). The webview owns none of this data. */

export type Phase = "running" | "paused" | "blocked" | "done" | "archived";

export interface Workspace {
  id: string;
  name: string;
  directory: string;
  createdAt: number;
}
export interface WorkspaceView {
  workspace: Workspace;
  binding: "present" | "missing";
}

export interface Role {
  id: string;
  name: string;
  systemPrompt: string;
  providerId?: string | null;
  modelId?: string | null;
  toolAllowlist: string[];
}
export interface Team {
  id: string;
  name: string;
  roles: Role[];
}

export interface RunSummary {
  id: string;
  objective: string;
  branch: string;
  state: Phase;
  createdAt: number;
  teamId: string | null;
}
export interface PersistedSession {
  id: string;
  role: string;
  directory: string;
  engineVersion: string;
  startedAt: number;
}
export interface PersistedHandoff {
  from: string;
  to: string;
  context: string;
  at: number;
}
export interface PersistedRun extends RunSummary {
  workspaceId: string;
  sessions: PersistedSession[];
  handoffs: PersistedHandoff[];
}
export interface TranscriptMessage {
  role: "user" | "assistant";
  text: string;
  at: number;
}

export interface Change {
  path: string;
  kind: "added" | "modified" | "deleted" | "renamed";
}
export interface RunDiff {
  files: Change[];
  additions: number;
  deletions: number;
  patch: string;
}

export interface Memory {
  id: string;
  subject: string;
  claim: string;
  scopes: ({ kind: "global" } | { kind: "workspace"; workspaceId: string } | { kind: "role"; roleId: string })[];
  recordedAt: number;
  recordedByRun: string | null;
  supersededBy: string | null;
  pinned: boolean;
  lastUsedAt: number | null;
}

export interface Automation {
  id: string;
  workspaceId: string;
  name: string;
  schedule: string;
  objective: string;
  roles: Role[];
  enabled: boolean;
  nextFireAt: number | null;
}
export interface Fire {
  id: string;
  automationId: string;
  scheduledFor: number;
  startedAt: number;
  runId: string | null;
  outcome: "started" | "completed" | "quiet" | "blocked" | "failed" | "skipped_overlap" | "missed";
  summary: string;
  seen: boolean;
}

export type McpServer = {
  id: string;
  workspaceId: string | null;
  name: string;
  enabled: boolean;
} & (
  | { kind: "local"; command: string[]; environment: Record<string, string> }
  | { kind: "remote"; url: string }
);

export interface Pack {
  id: string;
  name: string;
  description: string;
  team: { name: string; roles: Role[] };
  files: { path: string; from: string }[];
}
export interface PackPreview {
  willWrite: string[];
  willSkip: string[];
}
export interface Applied {
  team: Team;
  written: string[];
  skipped: string[];
}

export interface SkillEntry {
  name: string;
  description: string;
  dir: string;
  checksum: string;
  origin: string;
}
export interface InstalledSkill {
  name: string;
  origin: string;
  modified: boolean;
}
export interface SkillSource {
  id: string;
  url: string;
  lastSyncedAt: number | null;
}

export interface CredentialStatus {
  found: boolean;
  scope: "role" | "workspace" | "global" | null;
  tried: string[];
}
export interface ProviderInfo {
  id: string;
  name: string;
  models: { id: string; name: string }[];
}

export type BlockReason =
  | { kind: "credential"; role: string; providerId: string; tried: string[] }
  | { kind: "engine"; role: string; message: string };

export interface PermissionPrompt {
  id: string;
  sessionId: string;
  permission: string;
  patterns: string[];
  /** Whether the engine offered a durable "always" for this (we only ever persist read/edit). */
  always: string[];
}
