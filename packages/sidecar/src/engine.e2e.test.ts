/**
 * The real engine, end to end, against a fake model.
 *
 * Opt-in (`pnpm test:e2e`): it spawns the bundled engine and, on a cold cache,
 * lets it fetch an AI SDK provider package, so it needs the binary and a
 * network and is not part of the default gate. What it proves cannot be proved
 * against fakes: that the engine accepts workmate's compiled permission ruleset,
 * runs a turn, keeps the transcript, and — through the sidecar's own loopback
 * MCP endpoint — lets an agent write memory (tickets 014 and 015).
 */
import { execSync, spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { RoleId, RunId, WorkspaceId } from "@workmate/core";
import { DbClient } from "./db.js";
import { startMcp, type McpHandle } from "./mcp.js";
import { MemoryService } from "./memory.js";
import { MEMORY_SERVER } from "./orchestrator.js";

const ENGINE = join(__dirname, "../../../apps/desktop/src-tauri/binaries/opencode");
const enabled = process.env["WORKMATE_E2E"] === "1" && existsSync(ENGINE);

const sse = (delta: object, finish?: string): string =>
  `data: ${JSON.stringify({ id: "c", object: "chat.completion.chunk", created: 1, model: "m", choices: [{ index: 0, delta, finish_reason: finish ?? null }] })}\n\n`;
const usage = `data: ${JSON.stringify({ id: "c", object: "chat.completion.chunk", created: 1, model: "m", choices: [], usage: { prompt_tokens: 5, completion_tokens: 5, total_tokens: 10 } })}\n\n`;

describe.skipIf(!enabled)("the real engine", () => {
  const calls: { op: string; args: Record<string, unknown> }[] = [];
  const modelSaw: { tools: string[]; system: string[] } = { tools: [], system: [] };
  let llm: http.Server;
  let engine: ChildProcess;
  let mcp: McpHandle;
  let base = "";
  let repo = "";
  const auth = `Basic ${Buffer.from("opencode:pw").toString("base64")}`;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- the engine's JSON, asserted on below
  const call = async (path: string, init: RequestInit = {}): Promise<any> => {
    const r = await fetch(base + path, { ...init, headers: { authorization: auth, "content-type": "application/json" } });
    const text = await r.text();
    if (!r.ok) throw new Error(`${r.status} ${path}: ${text.slice(0, 300)}`);
    return text ? JSON.parse(text) : null;
  };

  beforeAll(async () => {
    const db: DbClient = new DbClient((m) => {
      if (m.type !== "db.call") return;
      calls.push({ op: m.op, args: m.args });
      queueMicrotask(() => db.settle(m.id, { rows: m.op === "memory.remember" ? [{ id: "mem_1", superseded: [] }] : [] }));
    });
    mcp = await startMcp({ memory: new MemoryService(db), token: "tok" });

    llm = http.createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const j = body ? JSON.parse(body) : {};
        res.writeHead(200, { "content-type": "text/event-stream" });
        const messages: { role: string; content?: unknown }[] = j.messages ?? [];
        modelSaw.tools = (j.tools ?? []).map((t: { function?: { name?: string } }) => t.function?.name ?? "");
        modelSaw.system = messages.filter((m) => m.role === "system").map((m) => String(m.content));
        const userText = JSON.stringify(messages.find((m) => m.role === "user")?.content ?? "");
        if (messages.at(-1)?.role === "tool") {
          res.write(sse({ role: "assistant", content: "" }) + sse({ content: "Noted." }) + sse({}, "stop"));
        } else if (userText.includes("run:")) {
          const command = /run:([^"\\]+)/.exec(userText)?.[1]?.trim() ?? "true";
          res.write(sse({ role: "assistant", content: null, tool_calls: [{ index: 0, id: "call_b", type: "function", function: { name: "bash", arguments: JSON.stringify({ command, description: "test" }) } }] }) + sse({}, "tool_calls"));
        } else if (userText.includes("remember")) {
          res.write(sse({ role: "assistant", content: null, tool_calls: [{ index: 0, id: "call_1", type: "function", function: { name: `${MEMORY_SERVER}_remember`, arguments: JSON.stringify({ subject: "package manager", claim: "uses pnpm", pinned: true }) } }] }) + sse({}, "tool_calls"));
        } else {
          res.write(sse({ role: "assistant", content: "" }) + sse({ content: "Hello from the fake model." }) + sse({}, "stop"));
        }
        res.end(usage + "data: [DONE]\n\n");
      });
    });
    await new Promise<void>((r) => llm.listen(0, "127.0.0.1", r));

    // A stable cache, so the provider package is fetched once, not every run.
    const cache = join(tmpdir(), "workmate-e2e-cache");
    mkdirSync(cache, { recursive: true });
    const home = mkdtempSync(join(tmpdir(), "wm-engine-"));
    repo = mkdtempSync(join(tmpdir(), "wm-repo-"));
    execSync("git init -q && git config user.email t@t && git config user.name t && echo hi > a.txt && git add -A && git commit -qm init", { cwd: repo });
    const config = { provider: { fake: { npm: "@ai-sdk/openai-compatible", name: "Fake", options: { baseURL: `http://127.0.0.1:${(llm.address() as { port: number }).port}/v1`, apiKey: "x" }, models: { m: { name: "m", tool_call: true, limit: { context: 8000, output: 1000 } } } } } };
    engine = spawn(ENGINE, ["serve", "--hostname", "127.0.0.1", "--port", "0"], {
      env: { ...process.env, OPENCODE_SERVER_PASSWORD: "pw", XDG_DATA_HOME: join(home, "d"), XDG_CONFIG_HOME: join(home, "c"), XDG_CACHE_HOME: cache, XDG_STATE_HOME: join(home, "s"), OPENCODE_CONFIG_CONTENT: JSON.stringify(config) },
    });
    base = await new Promise<string>((resolve, reject) => {
      const t = setTimeout(() => reject(new Error("the engine never announced an address")), 90_000);
      const on = (d: unknown): void => {
        const m = /(http:\/\/127\.0\.0\.1:\d+)/.exec(String(d));
        if (m?.[1]) {
          clearTimeout(t);
          resolve(m[1]);
        }
      };
      engine.stdout?.on("data", on);
      engine.stderr?.on("data", on);
    });
  }, 120_000);

  afterAll(async () => {
    engine?.kill();
    llm?.close();
    await mcp?.close();
  });

  const session = async () => {
    const dir = encodeURIComponent(repo);
    // The shape workmate sends: the compiled ruleset, as an array.
    const permission = [
      { permission: "read", pattern: "**", action: "ask" },
      { permission: "edit", pattern: "**", action: "deny" },
      { permission: "bash", pattern: "*", action: "ask" },
      { permission: "read", pattern: `${repo}/**`, action: "allow" },
      { permission: "edit", pattern: `${repo}/**`, action: "allow" },
      { permission: "edit", pattern: "**/.git/**", action: "deny" },
    ];
    const s = await call(`/session?directory=${dir}`, { method: "POST", body: JSON.stringify({ title: "t", permission }) });
    return { id: s.id as string, dir };
  };

  it("accepts the compiled permission ruleset, runs a turn, and keeps the transcript", async () => {
    const { id, dir } = await session();
    const msg = await call(`/session/${id}/message?directory=${dir}`, {
      method: "POST",
      body: JSON.stringify({ parts: [{ type: "text", text: "say hello" }], system: "You are a test role.", model: { providerID: "fake", modelID: "m" } }),
    });
    expect(msg.parts.filter((p: { type: string }) => p.type === "text").map((p: { text: string }) => p.text)).toEqual(["Hello from the fake model."]);
    // The per-turn `system` field really reaches the model: that is how the
    // memory digest is injected.
    expect(modelSaw.system.join("\n")).toContain("You are a test role.");
    const transcript = await call(`/session/${id}/message?directory=${dir}`);
    expect(transcript.map((m: { info: { role: string } }) => m.info.role)).toEqual(["user", "assistant"]);
  }, 60_000);

  it("lets an agent write memory through the sidecar's loopback endpoint, attributed to its run", async () => {
    const { id, dir } = await session();
    const ctx = { runId: "run_1" as RunId, workspaceId: "ws_1" as WorkspaceId, roleId: "coder" as RoleId };
    const reg = await call(`/mcp?directory=${dir}`, {
      method: "POST",
      body: JSON.stringify({ name: MEMORY_SERVER, config: { type: "remote", url: mcp.urlFor(ctx), headers: { authorization: "Bearer tok" } } }),
    });
    expect(reg[MEMORY_SERVER].status).toBe("connected");

    await call(`/session/${id}/message?directory=${dir}`, {
      method: "POST",
      body: JSON.stringify({ parts: [{ type: "text", text: "remember that we use pnpm" }], model: { providerID: "fake", modelID: "m" } }),
    });
    // The tool names the engine gives the model are what the orchestrator's
    // allowlist assumes.
    expect(modelSaw.tools).toEqual(expect.arrayContaining([`${MEMORY_SERVER}_remember`, `${MEMORY_SERVER}_recall`]));
    const remembered = calls.find((c) => c.op === "memory.remember");
    expect(remembered?.args).toMatchObject({
      subject: "package manager",
      claim: "uses pnpm",
      pinned: true,
      runId: "run_1",
      scopes: [{ kind: "workspace", workspaceId: "ws_1" }],
    });
  }, 60_000);

  /**
   * The allowlist (ticket 028) and the shell baseline both allow a command by
   * prefix, so they are only safe if the engine judges each command in a chain
   * on its own, rather than matching the whole string against `git status *`.
   */
  const bashUnder = async (command: string): Promise<string> => {
    const dir = encodeURIComponent(repo);
    const permission = [
      { permission: "bash", pattern: "*", action: "deny" },
      { permission: "bash", pattern: "git status *", action: "allow" },
    ];
    const s = await call(`/session?directory=${dir}`, { method: "POST", body: JSON.stringify({ title: "t", permission }) });
    await call(`/session/${s.id}/message?directory=${dir}`, {
      method: "POST",
      body: JSON.stringify({ parts: [{ type: "text", text: `run:${command}` }], model: { providerID: "fake", modelID: "m" } }),
    });
    // The tool's own record — status and output — is in the transcript.
    const transcript: { parts: { type: string; tool?: string; state?: unknown }[] }[] = await call(`/session/${s.id}/message?directory=${dir}`);
    return JSON.stringify(transcript.flatMap((m) => m.parts).filter((p) => p.type === "tool").map((p) => p.state));
  };

  it("runs a command its prefix allows", async () => {
    const parts = await bashUnder("git status");
    expect(parts).toMatch(/On branch|No commits yet|nothing to commit/);
  }, 60_000);

  it("judges each command in a chain on its own, so an allowed prefix cannot carry another command", async () => {
    const { existsSync: exists } = await import("node:fs");
    for (const chain of ["git status; touch pwned1.txt", "git status && touch pwned2.txt", "git status | touch pwned3.txt", "git status $(touch pwned4.txt)"]) {
      const record = await bashUnder(chain);
      // Refused, not merely failed: the engine says why.
      expect(record, chain).toMatch(/denied|rejected|not allowed|permission/i);
    }
    for (const n of [1, 2, 3, 4]) expect(exists(join(repo, `pwned${n}.txt`)), `pwned${n}.txt`).toBe(false);
  }, 120_000);

  it("replaces an MCP server registered again under the same name", async () => {
    const { dir } = await session();
    const reg = (url: string) =>
      call(`/mcp?directory=${dir}`, { method: "POST", body: JSON.stringify({ name: "swap", config: { type: "remote", url } }) });
    const first = await reg("http://127.0.0.1:9/a");
    const second = await reg("http://127.0.0.1:9/b");
    expect(Object.keys(first)).toContain("swap");
    expect(Object.keys(second).filter((k) => k === "swap")).toHaveLength(1);
  }, 60_000);
});
