import { afterEach, describe, expect, it } from "vitest";
import type { RoleId, RunId, WorkspaceId } from "@workmate/core";
import { DbClient } from "./db.js";
import { MemoryService, type MemoryContext } from "./memory.js";
import { parseContext, startMcp, type McpHandle } from "./mcp.js";
import type { Outbound } from "./protocol.js";

const ctx: MemoryContext = { runId: "run_1" as RunId, workspaceId: "ws_1" as WorkspaceId, roleId: "coder" as RoleId };

/** A DbClient whose "Rust" answers from a table of canned operations. */
const fakeDb = (ops: Record<string, (args: Record<string, unknown>) => unknown[] | Error>) => {
  const calls: { op: string; args: Record<string, unknown> }[] = [];
  const db: DbClient = new DbClient((m: Outbound) => {
    if (m.type !== "db.call") return;
    calls.push({ op: m.op, args: m.args });
    const out = ops[m.op]?.(m.args) ?? new Error("unknown op");
    queueMicrotask(() => db.settle(m.id, out instanceof Error ? { error: out.message } : { rows: out }));
  });
  return { db, calls };
};

describe("MemoryService", () => {
  it("builds the digest from pinned memories only, narrowest scope first", async () => {
    const { db, calls } = fakeDb({
      "memory.list": () => [
        { id: "a", subject: "build", claim: "uses pnpm", scopes: [{ kind: "workspace", workspaceId: "ws_1" }], recordedAt: 1, recordedByRun: null, pinned: true },
        { id: "b", subject: "minor", claim: "not pinned", scopes: [{ kind: "global" }], recordedAt: 2, recordedByRun: null, pinned: false },
      ],
    });
    const text = await new MemoryService(db).digestFor(ctx);
    expect(text).toContain("- build: uses pnpm");
    expect(text).not.toContain("not pinned");
    expect(calls[0]?.args["scopes"]).toEqual([
      { kind: "role", roleId: "coder" },
      { kind: "workspace", workspaceId: "ws_1" },
      { kind: "global" },
    ]);
  });

  it("injects nothing when nothing is pinned", async () => {
    const { db } = fakeDb({ "memory.list": () => [] });
    expect(await new MemoryService(db).digestFor(ctx)).toBe("");
  });

  it("remembers into the workspace by default, attributed to the run", async () => {
    const { db, calls } = fakeDb({ "memory.remember": () => [{ id: "m", superseded: [] }] });
    const out = await new MemoryService(db).remember(ctx, { subject: "s", claim: "c" });
    expect(out).toEqual({ text: "Remembered.", isError: false });
    expect(calls[0]?.args).toMatchObject({ scopes: [{ kind: "workspace", workspaceId: "ws_1" }], runId: "run_1", pinned: false });
  });

  it("says when a claim replaced an earlier one", async () => {
    const { db } = fakeDb({ "memory.remember": () => [{ id: "m", superseded: ["old"] }] });
    expect((await new MemoryService(db).remember(ctx, { subject: "s", claim: "c" })).text).toMatch(/replaced 1/);
  });

  it("turns the writer's refusal into a tool error the agent can read", async () => {
    const { db } = fakeDb({ "memory.remember": () => new Error("this looks like a credential; secrets are never remembered") });
    const out = await new MemoryService(db).remember(ctx, { subject: "s", claim: "sk-xxxxxxxxxxxxxxxxxxxx" });
    expect(out.isError).toBe(true);
    expect(out.text).toMatch(/credential/);
  });

  it("recalls as plain lines, and says so when nothing matches", async () => {
    const hit = { id: "a", subject: "testing", claim: "vitest", scopes: [], recordedAt: 1, recordedByRun: null, pinned: false };
    const { db } = fakeDb({ "memory.recall": (a) => (a["query"] === "tests" ? [hit] : []) });
    const svc = new MemoryService(db);
    expect((await svc.recall(ctx, { query: "tests" })).text).toBe("- testing: vitest");
    expect((await svc.recall(ctx, { query: "zzz" })).text).toMatch(/Nothing remembered/);
  });
});

describe("the MCP endpoint", () => {
  let handle: McpHandle | undefined;
  afterEach(async () => {
    await handle?.close();
    handle = undefined;
  });

  const start = async () => {
    const f = fakeDb({
      "memory.remember": () => [{ id: "m", superseded: [] }],
      "memory.recall": () => [],
    });
    handle = await startMcp({ memory: new MemoryService(f.db), token: "t0ken" });
    const post = (body: unknown, auth: string | null = "Bearer t0ken", path?: string) =>
      fetch(path ?? handle!.urlFor(ctx), {
        method: "POST",
        headers: { "content-type": "application/json", ...(auth ? { authorization: auth } : {}) },
        body: JSON.stringify(body),
      });
    return { ...f, post };
  };

  it("refuses a caller without the launch token", async () => {
    const { post } = await start();
    expect((await post({ jsonrpc: "2.0", id: 1, method: "ping" }, null)).status).toBe(401);
    expect((await post({ jsonrpc: "2.0", id: 1, method: "ping" }, "Bearer wrong")).status).toBe(401);
  });

  it("initialises and lists remember and recall", async () => {
    const { post } = await start();
    const init = (await (await post({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} })).json()) as { result: { capabilities: unknown } };
    expect(init.result.capabilities).toEqual({ tools: {} });
    const list = (await (await post({ jsonrpc: "2.0", id: 2, method: "tools/list" })).json()) as { result: { tools: { name: string }[] } };
    expect(list.result.tools.map((t) => t.name)).toEqual(["remember", "recall"]);
  });

  it("runs a tool call as the caller named in the path", async () => {
    const { post, calls } = await start();
    const res = await post({ jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "remember", arguments: { subject: "s", claim: "c" } } });
    const body = (await res.json()) as { result: { content: { text: string }[]; isError: boolean } };
    expect(body.result.content[0]?.text).toBe("Remembered.");
    expect(calls.find((c) => c.op === "memory.remember")?.args["runId"]).toBe("run_1");
  });

  it("acknowledges a notification with 202 and rejects unknown methods and tools", async () => {
    const { post } = await start();
    expect((await post({ jsonrpc: "2.0", method: "notifications/initialized" })).status).toBe(202);
    const bad = (await (await post({ jsonrpc: "2.0", id: 4, method: "nope" })).json()) as { error: { code: number } };
    expect(bad.error.code).toBe(-32601);
    const tool = (await (await post({ jsonrpc: "2.0", id: 5, method: "tools/call", params: { name: "rm -rf" } })).json()) as { error: { code: number } };
    expect(tool.error.code).toBe(-32602);
  });

  it("rejects a path that does not name a caller", async () => {
    const { post } = await start();
    expect((await post({ jsonrpc: "2.0", id: 1, method: "ping" }, "Bearer t0ken", `http://127.0.0.1:${handle!.port}/mcp`)).status).toBe(404);
  });
});

describe("parseContext", () => {
  it("round-trips encoded ids", () => {
    expect(parseContext("/mcp/run_1/ws%2F1/coder")).toEqual({ runId: "run_1", workspaceId: "ws/1", roleId: "coder" });
    expect(parseContext("/other")).toBeUndefined();
  });
});
