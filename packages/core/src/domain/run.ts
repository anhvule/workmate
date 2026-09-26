import type { RoleId, RunId, SessionId } from "./ids.js";

/**
 * A Run is one unit of work handed to a Team, and it **owns** its sessions.
 *
 * This is the aggregate that fixes cowork-z's worst constraint: there, starting
 * a task deletes every other live session as "stale", so only one agent can
 * exist at a time. Here a session ends when its run ends, and nothing else may
 * cull it (ticket 003).
 */
export interface Run {
  readonly id: RunId;
  readonly sessions: readonly RunSession[];
  readonly handoffs: readonly Handoff[];
  readonly state: RunState;
}

export type RunState = "running" | "paused" | "blocked" | "done" | "archived";

export interface RunSession {
  readonly id: SessionId;
  readonly role: RoleId;
  /** The run's own worktree. A run never works in the user's checkout. */
  readonly directory: string;
  /** Recorded so a resume across an engine upgrade can warn rather than misbehave. */
  readonly engineVersion: string;
  readonly startedAt: number;
}

/**
 * An explicit edge carrying selected context from one session to another.
 *
 * OpenCode's native delegation passes only the child's last text part, so the
 * context a role actually received is workmate's to compose, store and render
 * (ticket 001).
 */
export interface Handoff {
  readonly from: SessionId;
  readonly to: SessionId;
  readonly context: string;
  readonly at: number;
}

/** `workmate/run-<id>` — the only place a run's branch name is constructed. */
export const runBranch = (id: RunId): string => `workmate/run-${id}`;

export class HandoffCycleError extends Error {
  constructor(readonly sessions: readonly SessionId[]) {
    super(`handoff graph contains a cycle: ${sessions.join(" -> ")}`);
    this.name = "HandoffCycleError";
  }
}

/**
 * The canonical order a run reads in: handoff graph first, wall-clock second.
 *
 * The stored order and the order the UI renders are deliberately the same
 * computation, so the two cannot drift (tickets 005 and 010).
 */
export const runTimeline = (run: Run): readonly RunSession[] => {
  const byId = new Map(run.sessions.map((s) => [s.id, s]));
  const incoming = new Map<SessionId, number>(run.sessions.map((s) => [s.id, 0]));
  const next = new Map<SessionId, SessionId[]>();

  for (const h of run.handoffs) {
    if (!byId.has(h.from) || !byId.has(h.to)) continue;
    incoming.set(h.to, (incoming.get(h.to) ?? 0) + 1);
    next.set(h.from, [...(next.get(h.from) ?? []), h.to]);
  }

  const byStart = (a: SessionId, b: SessionId): number =>
    (byId.get(a)?.startedAt ?? 0) - (byId.get(b)?.startedAt ?? 0);

  const ready = run.sessions
    .filter((s) => (incoming.get(s.id) ?? 0) === 0)
    .map((s) => s.id)
    .sort(byStart);

  const out: RunSession[] = [];
  while (ready.length > 0) {
    const id = ready.shift() as SessionId;
    const session = byId.get(id);
    if (session) out.push(session);
    for (const to of next.get(id) ?? []) {
      const remaining = (incoming.get(to) ?? 0) - 1;
      incoming.set(to, remaining);
      if (remaining === 0) {
        ready.push(to);
        ready.sort(byStart);
      }
    }
  }

  if (out.length !== run.sessions.length) {
    const stuck = run.sessions.filter((s) => !out.some((o) => o.id === s.id)).map((s) => s.id);
    throw new HandoffCycleError(stuck);
  }
  return out;
};

/** A one-role run must stay indistinguishable from a plain chat (ticket 005). */
export const isSoloRun = (run: Run): boolean =>
  new Set(run.sessions.map((s) => s.role)).size <= 1 && run.handoffs.length === 0;
