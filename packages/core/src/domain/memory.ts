import type { MemoryId, RunId, WorkspaceId } from "./ids.js";
import { scopeKey, specificity, type Scope } from "./scope.js";

/**
 * A memory is a *claim*, with provenance — never an archive.
 *
 * Secrets, verbatim file contents and raw transcript text are deliberately not
 * representable here (ticket 006).
 */
export interface Memory {
  readonly id: MemoryId;
  readonly subject: string;
  readonly claim: string;
  /** Scopes this memory is *associated with*. It is never owned by any of them. */
  readonly scopes: readonly Scope[];
  readonly recordedAt: number;
  readonly recordedByRun: RunId;
  /** Set when a later memory contradicts this one. History is kept, not overwritten. */
  readonly supersededBy?: MemoryId | undefined;
  /** Pinned memories go in the bounded per-turn digest; the rest need `recall`. */
  readonly pinned: boolean;
}

const isLive = (m: Memory): boolean => m.supersededBy === undefined;

const bestScoreFor = (m: Memory, order: readonly Scope[]): number | undefined => {
  let best: number | undefined;
  for (const s of m.scopes) {
    if (order.some((o) => scopeKey(o) === scopeKey(s))) {
      const score = specificity(s);
      if (best === undefined || score > best) best = score;
    }
  }
  return best;
};

/**
 * Select the memories that apply, narrowest scope first, newest breaking ties.
 *
 * Superseded memories are excluded: they are retained so the history of what
 * workmate believed stays inspectable, not so they can be recalled as fact.
 */
export const selectMemories = (
  all: readonly Memory[],
  order: readonly Scope[],
): readonly Memory[] =>
  all
    .filter(isLive)
    .flatMap((m) => {
      const score = bestScoreFor(m, order);
      return score === undefined ? [] : [{ m, score }];
    })
    .sort((a, b) => b.score - a.score || b.m.recordedAt - a.m.recordedAt)
    .map((x) => x.m);

/** Roughly four characters per token — deliberately crude, deliberately bounded. */
const estimateTokens = (s: string): number => Math.ceil(s.length / 4);

export const DIGEST_TOKEN_BUDGET = 500;

/**
 * The pinned digest injected into every turn via OpenCode's per-turn `system`
 * field. Bounded by construction: an unbounded digest is a per-turn tax on
 * every run forever (ticket 006).
 */
export const buildDigest = (
  all: readonly Memory[],
  order: readonly Scope[],
  budget = DIGEST_TOKEN_BUDGET,
): readonly Memory[] => {
  const out: Memory[] = [];
  let spent = 0;
  for (const m of selectMemories(all, order)) {
    if (!m.pinned) continue;
    const cost = estimateTokens(`${m.subject}: ${m.claim}`);
    if (spent + cost > budget) break;
    out.push(m);
    spent += cost;
  }
  return out;
};

/**
 * Removing a workspace detaches it from memories and deletes nothing.
 *
 * This is the single most important rule on the map: cowork-z cascades every
 * table off `workspace_id`, which would delete workmate's differentiator along
 * with a removed folder (ticket 003).
 */
export const detachWorkspace = (
  all: readonly Memory[],
  id: WorkspaceId,
): readonly Memory[] =>
  all.map((m) => ({
    ...m,
    scopes: m.scopes.filter((s) => !(s.kind === "workspace" && s.workspaceId === id)),
  }));
