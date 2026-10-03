/**
 * Live run state: what the events say right now.
 *
 * This is the one place events land, and it is deliberately not a cache of
 * anything durable. Everything persistent is read from Rust; this holds only the
 * in-flight pieces — the text streaming in, a handoff awaiting a decision, a
 * block — and each is discarded when the authoritative event or reload
 * supersedes it (ticket 010: the webview never persists).
 */
import { useMemo, useSyncExternalStore } from "react";
import { listen } from "./bridge.js";
import type { BlockReason, PermissionPrompt, Phase } from "./types.js";

export interface PendingHandoff {
  from: string;
  to: string;
  context: string;
  editable: boolean;
}

export interface LiveRun {
  phase?: Phase | undefined;
  pending?: PendingHandoff | undefined;
  blocked?: BlockReason | undefined;
  finished?: "completed" | "vetoed" | undefined;
  /** Bumped on anything that makes persisted data stale, so views reload. */
  version: number;
}

interface State {
  runs: Record<string, LiveRun>;
  /** Text streaming into each session, by part, until the transcript replaces it. */
  streams: Record<string, Record<string, string>>;
  prompts: PermissionPrompt[];
  /** Which run and role each session belongs to, so a prompt can name who is asking. */
  sessions: Record<string, { runId: string; workspaceId: string; roleId: string; role: string }>;
  /** Bumped when the event stream reconnected: anything shown may be stale. */
  resync: number;
  findings: number;
  /** Latest fault from the runtime, shown rather than swallowed. */
  fault: string | null;
}

let state: State = { runs: {}, streams: {}, prompts: [], sessions: {}, resync: 0, findings: 0, fault: null };
const subs = new Set<() => void>();
const emit = (): void => subs.forEach((f) => f());
const set = (f: (s: State) => State): void => {
  state = f(state);
  emit();
};

const blank = (): LiveRun => ({ version: 0 });
const patchRun = (id: string, f: (r: LiveRun) => LiveRun): void =>
  set((s) => ({ ...s, runs: { ...s.runs, [id]: f(s.runs[id] ?? blank()) } }));

type Payload = Record<string, any>; // eslint-disable-line @typescript-eslint/no-explicit-any

const handlers: Record<string, (p: Payload) => void> = {
  "workmate:run_started": (p) => patchRun(p["runId"], (r) => ({ ...r, phase: "running", version: r.version + 1 })),
  "workmate:run_changed": (p) =>
    patchRun(p["runId"], (r) => ({
      ...r,
      phase: p["phase"],
      // Leaving a state the user was asked about clears what they were asked.
      ...(p["phase"] === "running" ? { blocked: undefined } : {}),
      version: r.version + 1,
    })),
  "workmate:run_session_opened": (p) =>
    set((s) => ({
      ...s,
      sessions: { ...s.sessions, [p["sessionId"]]: { runId: p["runId"], workspaceId: p["workspaceId"], roleId: p["roleId"], role: p["role"] } },
    })),
  "workmate:run_turn": (p) => {
    // The turn is complete and durable now: drop the streamed copy of it.
    dismissStreaming(p["sessionId"]);
    patchRun(p["runId"], (r) => ({ ...r, version: r.version + 1 }));
  },
  "workmate:run_handoff_proposed": (p) =>
    patchRun(p["runId"], (r) => ({
      ...r,
      pending: { from: p["from"], to: p["to"], context: p["context"], editable: p["editable"] },
      version: r.version + 1,
    })),
  "workmate:run_handoff_delivered": (p) =>
    patchRun(p["runId"], (r) => ({ ...r, pending: undefined, version: r.version + 1 })),
  "workmate:run_blocked": (p) => patchRun(p["runId"], (r) => ({ ...r, blocked: p["reason"], version: r.version + 1 })),
  "workmate:run_finished": (p) =>
    patchRun(p["runId"], (r) => ({ ...r, finished: p["reason"], pending: undefined, version: r.version + 1 })),
  "workmate:automation_finding": () => set((s) => ({ ...s, findings: s.findings + 1 })),
  "sidecar:stream_resumed": () => set((s) => ({ ...s, resync: s.resync + 1 })),
  "sidecar:permission_asked": (p) => {
    const props = p["envelope"]?.payload?.properties;
    if (!props) return;
    const prompt: PermissionPrompt = {
      id: props.id,
      sessionId: props.sessionID,
      permission: props.permission,
      patterns: props.patterns ?? [],
      always: props.always ?? [],
    };
    set((s) => ({ ...s, prompts: [...s.prompts.filter((x) => x.id !== prompt.id), prompt] }));
  },
  "sidecar:permission_replied": (p) => {
    const id = p["envelope"]?.payload?.properties?.requestID;
    set((s) => ({ ...s, prompts: s.prompts.filter((x) => x.id !== id) }));
  },
  "sidecar:message_part_delta": (p) => {
    const props = p["envelope"]?.payload?.properties;
    if (!props || props.field !== "text") return;
    set((s) => {
      const parts = { ...(s.streams[props.sessionID] ?? {}) };
      parts[props.partID] = (parts[props.partID] ?? "") + props.delta;
      return { ...s, streams: { ...s.streams, [props.sessionID]: parts } };
    });
  },
  // Free text from the sidecar's own fault channel is shown, not swallowed.
  "workmate:fault": (p) => set((s) => ({ ...s, fault: String(p["message"] ?? "unknown fault") })),
};

let started = false;

/** Subscribe to every event the shell cares about. Safe to call more than once. */
export const startLive = async (): Promise<void> => {
  if (started) return;
  started = true;
  await Promise.all(
    Object.entries(handlers).map(([name, fn]) =>
      listen(name, (payload) => fn((payload ?? {}) as Payload)),
    ),
  );
};

const subscribe = (f: () => void): (() => void) => {
  subs.add(f);
  return () => subs.delete(f);
};

export const useLive = <T,>(select: (s: State) => T): T => {
  // `getSnapshot` must be referentially stable between changes.
  return useSyncExternalStore(subscribe, () => select(state));
};

export const useRun = (runId: string | null): LiveRun => useLive((s) => (runId ? s.runs[runId] : undefined) ?? EMPTY);
const EMPTY: LiveRun = blank();

const NONE_LIST: string[] = [];

/** Streaming text for a session, one string per part, in arrival order. */
export const useStreaming = (sessionId: string | undefined): string[] => {
  const parts = useLive((s) => (sessionId ? s.streams[sessionId] : undefined));
  return useMemo(() => (parts ? Object.values(parts) : NONE_LIST), [parts]);
};

/** Test and preview hook: reset to a clean slate. */
export const resetLive = (): void => {
  state = { runs: {}, streams: {}, prompts: [], sessions: {}, resync: 0, findings: 0, fault: null };
  started = false;
  emit();
};
export const clearFault = (): void => set((s) => ({ ...s, fault: null }));
export function dismissStreaming(sessionId: string): void {
  set((s) => {
    if (!s.streams[sessionId]) return s;
    const rest = { ...s.streams };
    delete rest[sessionId];
    return { ...s, streams: rest };
  });
}
