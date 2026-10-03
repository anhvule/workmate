import { describe, expect, it } from "vitest";
import type { RoleId, RunId, WorkspaceId } from "@workmate/core";
import { DbClient } from "./db.js";
import { MemoryService } from "./memory.js";
import { Orchestrator, TurnPool, type Deps, type EnginePort, type RoleSpec, type RunEvent } from "./orchestrator.js";
import type { Outbound } from "./protocol.js";

const ws = "ws_1" as WorkspaceId;
const role = (id: string, over: Partial<RoleSpec> = {}): RoleSpec => ({
  id: id as RoleId,
  name: id,
  systemPrompt: `you are ${id}`,
  toolAllowlist: [],
  ...over,
});

interface Harness {
  orch: Orchestrator;
  events: RunEvent[];
  ops: { op: string; args: Record<string, unknown> }[];
  engine: { sessions: string[]; sent: { session: string; prompt: string; system: string; tools?: unknown }[]; fail: number; gate?: Promise<void> };
  provision: { status: "ok" | "missing" };
}

const harness = (maxConcurrentTurns = 3): Harness => {
  const events: RunEvent[] = [];
  const ops: Harness["ops"] = [];
  const provision = { status: "ok" as "ok" | "missing" };
  const db: DbClient = new DbClient((m: Outbound) => {
    if (m.type !== "db.call") return;
    ops.push({ op: m.op, args: m.args });
    let rows: unknown[] = [];
    if (m.op === "repo.createWorktree") rows = [{ path: `/wt/${String(m.args["runId"])}`, branch: "workmate/run-x", base: "abc" }];
    if (m.op === "permission.ruleset") rows = [[{ permission: "edit", pattern: "/wt/**", action: "allow" }]];
    if (m.op === "credentials.provision") rows = [provision.status === "ok" ? { status: "ok" } : { status: "missing", tried: ["v1:global:anthropic"] }];
    queueMicrotask(() => db.settle(m.id, { rows }));
  });
  const engine: Harness["engine"] = { sessions: [], sent: [], fail: 0 };
  let n = 0;
  const port: EnginePort = {
    createSession: async (_dir, body) => {
      const id = `s${++n}`;
      engine.sessions.push(id);
      expect(body.permission).toHaveLength(1);
      return { id };
    },
    sendMessage: async (session, _dir, body) => {
      if (engine.gate) await engine.gate;
      if (engine.fail > 0) {
        engine.fail -= 1;
        throw new Error("engine down");
      }
      engine.sent.push({ session, prompt: body.prompt, system: body.system, tools: body.tools });
      return { text: `done by ${session}` };
    },
    listMessages: async (session) => [{ role: "assistant", text: `concluded in ${session}`, at: 1 }],
    registerMemoryTools: async () => undefined,
  };
  const memoryDb: DbClient = new DbClient((m: Outbound) => {
    if (m.type === "db.call") queueMicrotask(() => memoryDb.settle(m.id, { rows: [] }));
  });
  let ids = 0;
  const deps: Deps = {
    db,
    engine: port,
    memory: new MemoryService(memoryDb),
    emit: (e) => events.push(e),
    memoryUrl: () => "http://127.0.0.1:1/mcp/x",
    newId: (p) => `${p}_${++ids}`,
    maxConcurrentTurns,
  };
  return { orch: new Orchestrator(deps), events, ops, engine, provision };
};

const until = async (cond: () => boolean): Promise<void> => {
  for (let i = 0; i < 200 && !cond(); i++) await new Promise((r) => setTimeout(r, 2));
  if (!cond()) throw new Error("condition never became true");
};
const names = (h: Harness) => h.events.map((e) => e.name);
const opNames = (h: Harness) => h.ops.map((o) => o.op);

describe("a one-role run", () => {
  it("is a plain chat: one session, the objective as the prompt, no handoff anywhere", async () => {
    const h = harness();
    const { runId } = await h.orch.start({ workspaceId: ws, objective: "fix the bug", roles: [role("coder")] });
    await h.orch.get(runId).finished;

    expect(h.engine.sessions).toEqual(["s1"]);
    expect(h.engine.sent[0]?.prompt).toBe("fix the bug");
    expect(h.engine.sent[0]?.system).toContain("you are coder");
    expect(opNames(h)).not.toContain("handoff.append");
    expect(names(h).some((n) => n.startsWith("run.handoff"))).toBe(false);
    expect(h.events.find((e) => e.name === "run.started")?.payload).toMatchObject({ solo: true });
    expect(h.orch.get(runId).phase).toBe("done");
    expect(h.ops.filter((o) => o.op === "run.setState").at(-1)?.args["state"]).toBe("done");
  });

  it("creates the worktree before the run row, and the session inside that worktree", async () => {
    const h = harness();
    const { runId } = await h.orch.start({ workspaceId: ws, objective: "x", roles: [role("coder")] });
    await h.orch.get(runId).finished;
    const order = opNames(h);
    expect(order.indexOf("repo.createWorktree")).toBeLessThan(order.indexOf("run.create"));
    expect(h.ops.find((o) => o.op === "session.record")?.args["directory"]).toMatch(/^\/wt\/run_/);
  });
});

describe("a team run", () => {
  it("hands the previous role's conclusion to the next, recording the handoff after both sessions exist", async () => {
    const h = harness();
    const { runId } = await h.orch.start({ workspaceId: ws, objective: "ship it", roles: [role("planner"), role("coder")] });
    await h.orch.get(runId).finished;

    expect(h.engine.sessions).toEqual(["s1", "s2"]);
    expect(h.engine.sent[1]?.prompt).toContain("concluded in s1");
    expect(h.engine.sent[1]?.prompt).toContain("ship it");
    const rec = h.ops.filter((o) => o.op === "session.record").length;
    expect(rec).toBe(2);
    const handoff = h.ops.find((o) => o.op === "handoff.append");
    expect(handoff?.args).toMatchObject({ from: "s1", to: "s2" });
    expect(opNames(h).lastIndexOf("session.record")).toBeLessThan(opNames(h).indexOf("handoff.append"));
    expect(h.engine.sent[1]?.system).toContain("you are coder");
  });

  it("starting a second run never touches the first run's sessions", async () => {
    const h = harness();
    const a = await h.orch.start({ workspaceId: ws, objective: "a", roles: [role("r1")] });
    const b = await h.orch.start({ workspaceId: ws, objective: "b", roles: [role("r1")] });
    await Promise.all([h.orch.get(a.runId).finished, h.orch.get(b.runId).finished]);
    expect(h.engine.sessions).toHaveLength(2);
    expect(opNames(h).filter((o) => /delete|remove|cull/i.test(o) && o !== "repo.removeWorktree")).toEqual([]);
    expect(h.ops.filter((o) => o.op === "repo.removeWorktree")).toHaveLength(0);
  });

  it("an allowlist turns those tools on, keeps memory on, and leaves others to the engine", async () => {
    const h = harness();
    const { runId } = await h.orch.start({ workspaceId: ws, objective: "x", roles: [role("a", { toolAllowlist: ["read"] })] });
    await h.orch.get(runId).finished;
    expect(h.engine.sent[0]?.tools).toEqual({ read: true, remember: true, recall: true });
  });
});

describe("intervention", () => {
  const paused = async (h: Harness, roles: RoleSpec[]) => {
    const { runId } = await h.orch.start({ workspaceId: ws, objective: "obj", roles, review: true });
    const run = h.orch.get(runId);
    await until(() => run.phase === "paused" && run.pending !== undefined);
    return run;
  };

  it("holds at the handoff with sessions alive until the user continues", async () => {
    const h = harness();
    const run = await paused(h, [role("a"), role("b")]);
    expect(h.engine.sessions).toEqual(["s1"]);
    expect(h.events.find((e) => e.name === "run.handoff.proposed")?.payload).toMatchObject({ editable: true });
    run.resume();
    await run.finished;
    expect(h.engine.sessions).toEqual(["s1", "s2"]);
  });

  it("delivers the amended context, and records what was actually passed", async () => {
    const h = harness();
    const run = await paused(h, [role("a"), role("b")]);
    run.amend("only touch the parser");
    await run.finished;
    expect(h.engine.sent[1]?.prompt).toBe("only touch the parser");
    expect(h.ops.find((o) => o.op === "handoff.append")?.args["context"]).toBe("only touch the parser");
  });

  it("redirects to another role on the team", async () => {
    const h = harness();
    const run = await paused(h, [role("a"), role("b"), role("c")]);
    run.redirect("c" as RoleId);
    await run.finished;
    const rec = h.ops.filter((o) => o.op === "session.record").map((o) => o.args["roleId"]);
    expect(rec).toEqual(["a", "c"]);
  });

  it("a veto ends the run without starting the next role", async () => {
    const h = harness();
    const run = await paused(h, [role("a"), role("b")]);
    run.veto();
    await run.finished;
    expect(h.engine.sessions).toEqual(["s1"]);
    expect(h.events.at(-1)).toMatchObject({ name: "run.finished", payload: { reason: "vetoed" } });
    expect(opNames(h)).not.toContain("handoff.append");
  });

  it("a pause requested mid-run takes effect at the next handoff", async () => {
    const h = harness();
    let release!: () => void;
    h.engine.gate = new Promise<void>((r) => (release = r));
    const { runId } = await h.orch.start({ workspaceId: ws, objective: "o", roles: [role("a"), role("b")] });
    const run = h.orch.get(runId);
    run.pause();
    release();
    await until(() => run.phase === "paused");
    expect(h.engine.sessions).toEqual(["s1"]);
    run.resume();
    await run.finished;
  });
});

describe("blocking", () => {
  it("a missing credential blocks the run, says what was tried, and continues once supplied", async () => {
    const h = harness();
    h.provision.status = "missing";
    const { runId } = await h.orch.start({ workspaceId: ws, objective: "o", roles: [role("a", { providerId: "anthropic", modelId: "m" })] });
    const run = h.orch.get(runId);
    await until(() => run.phase === "blocked");
    const blocked = h.events.find((e) => e.name === "run.blocked");
    expect(blocked?.payload).toMatchObject({ reason: { kind: "credential", tried: ["v1:global:anthropic"] } });
    expect(h.engine.sessions).toEqual([]);
    h.provision.status = "ok";
    run.resume();
    await run.finished;
    expect(run.phase).toBe("done");
  });

  it("an engine failure blocks, and resuming retries on the same session", async () => {
    const h = harness();
    h.engine.fail = 1;
    const { runId } = await h.orch.start({ workspaceId: ws, objective: "o", roles: [role("a")] });
    const run = h.orch.get(runId);
    await until(() => run.phase === "blocked");
    expect(h.events.find((e) => e.name === "run.blocked")?.payload).toMatchObject({ reason: { kind: "engine", message: "engine down" } });
    run.resume();
    await run.finished;
    expect(h.engine.sessions).toEqual(["s1"]);
    expect(h.engine.sent).toHaveLength(1);
  });

  it("refuses an empty team before touching anything", async () => {
    const h = harness();
    await expect(h.orch.start({ workspaceId: ws, objective: "o", roles: [] })).rejects.toThrow(/at least one role/);
    expect(h.ops).toHaveLength(0);
  });
});

describe("TurnPool", () => {
  it("never runs more turns than its bound, and drains the queue", async () => {
    const pool = new TurnPool(2);
    let live = 0;
    let peak = 0;
    const job = () =>
      pool.run(async () => {
        live += 1;
        peak = Math.max(peak, live);
        await new Promise((r) => setTimeout(r, 5));
        live -= 1;
      });
    await Promise.all(Array.from({ length: 6 }, job));
    expect(peak).toBe(2);
    expect(pool.running).toBe(0);
  });
});

export type { RunId };
