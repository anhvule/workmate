import { describe, expect, it } from "vitest";
import { memoryId, roleId, runId, workspaceId } from "./ids.js";
import { forRole, forWorkspace, GLOBAL, resolutionOrder } from "./scope.js";
import { buildDigest, detachWorkspace, selectMemories, type Memory } from "./memory.js";

const ws = workspaceId("ws-1");
const other = workspaceId("ws-2");
const reviewer = roleId("reviewer");
const run = runId("run-1");

const mem = (id: string, over: Partial<Memory> = {}): Memory => ({
  id: memoryId(id),
  subject: "build",
  claim: "uses pnpm",
  scopes: [GLOBAL],
  recordedAt: 1,
  recordedByRun: run,
  pinned: false,
  ...over,
});

const order = resolutionOrder({ role: reviewer, workspace: ws });

describe("selectMemories", () => {
  it("puts the narrowest scope first", () => {
    const g = mem("g", { scopes: [GLOBAL] });
    const w = mem("w", { scopes: [forWorkspace(ws)] });
    const r = mem("r", { scopes: [forRole(reviewer)] });
    expect(selectMemories([g, w, r], order).map((m) => m.id)).toEqual(["r", "w", "g"]);
  });

  it("breaks ties within a scope by recency", () => {
    const old = mem("old", { scopes: [forWorkspace(ws)], recordedAt: 1 });
    const fresh = mem("fresh", { scopes: [forWorkspace(ws)], recordedAt: 2 });
    expect(selectMemories([old, fresh], order).map((m) => m.id)).toEqual(["fresh", "old"]);
  });

  it("excludes a superseded memory without deleting it", () => {
    const stale = mem("stale", { supersededBy: memoryId("fresh") });
    expect(selectMemories([stale], order)).toEqual([]);
    expect(stale.supersededBy).toBe("fresh");
  });

  it("ignores memories scoped to a workspace we are not in", () => {
    expect(selectMemories([mem("x", { scopes: [forWorkspace(other)] })], order)).toEqual([]);
  });
});

describe("buildDigest", () => {
  it("carries only pinned memories", () => {
    const pinned = mem("p", { pinned: true });
    expect(buildDigest([pinned, mem("u")], order).map((m) => m.id)).toEqual(["p"]);
  });

  it("stops at the token budget rather than growing without bound", () => {
    const big = (id: string) => mem(id, { pinned: true, claim: "x".repeat(1200) });
    const digest = buildDigest([big("a"), big("b"), big("c")], order);
    expect(digest.length).toBeLessThan(3);
  });
});

describe("detachWorkspace", () => {
  it("detaches the workspace and keeps the memory itself", () => {
    const m = mem("m", { scopes: [forWorkspace(ws), GLOBAL] });
    const after = detachWorkspace([m], ws);
    expect(after).toHaveLength(1);
    expect(after[0]?.scopes).toEqual([GLOBAL]);
  });

  it("leaves a memory that survives only globally still recallable", () => {
    const m = mem("m", { scopes: [forWorkspace(ws)] });
    const after = detachWorkspace([m], ws);
    expect(after[0]?.scopes).toEqual([]);
    expect(after).toHaveLength(1);
  });
});
