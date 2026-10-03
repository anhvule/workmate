/**
 * A loopback MCP endpoint that gives the engine's agents `remember` and `recall`.
 *
 * OpenCode takes tools from MCP servers, and a remote (HTTP) server is the one
 * shape the sidecar can host without owning its own stdio — stdout is the
 * protocol channel to Rust. Streamable-HTTP lets a server answer each POST with
 * plain JSON, so this is a small JSON-RPC handler rather than an SDK.
 *
 * Bound to 127.0.0.1 on an ephemeral port and guarded by a per-launch bearer
 * token: other local processes can connect to a loopback port, and this one can
 * write memory.
 *
 * The caller's identity travels in the path — `/mcp/<run>/<workspace>/<role>` —
 * so every `remember` is attributable to a run and a role without trusting the
 * model to say who it is.
 */
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import type { RoleId, RunId, WorkspaceId } from "@workmate/core";
import type { MemoryContext, MemoryService } from "./memory.js";

const TOOLS = [
  {
    name: "remember",
    description:
      "Record one durable fact about this project or the user so future runs know it. A short subject and a one-sentence claim. Never store secrets, file contents or transcripts. A new claim on the same subject replaces the old one.",
    inputSchema: {
      type: "object",
      properties: {
        subject: { type: "string", description: "What the fact is about, e.g. 'package manager'." },
        claim: { type: "string", description: "The fact, in one sentence." },
        scope: {
          type: "string",
          enum: ["workspace", "global", "role"],
          description: "How widely it applies. Defaults to this workspace.",
        },
        pinned: { type: "boolean", description: "Include in every turn's digest. Use sparingly." },
      },
      required: ["subject", "claim"],
    },
  },
  {
    name: "recall",
    description: "Look up what workmate has remembered. Use before asking the user something they may have already told you.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Words describing what you want to know." },
        limit: { type: "number" },
      },
      required: ["query"],
    },
  },
] as const;

const PROTOCOL_VERSION = "2025-03-26";

type Json = Record<string, unknown>;

const readBody = (req: IncomingMessage): Promise<string> =>
  new Promise((resolve, reject) => {
    let body = "";
    req.setEncoding("utf8");
    req.on("data", (c: string) => {
      body += c;
      if (body.length > 256 * 1024) reject(new Error("body too large"));
    });
    req.on("end", () => resolve(body));
    req.on("error", reject);
  });

const reply = (res: ServerResponse, status: number, body?: unknown): void => {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(body === undefined ? undefined : JSON.stringify(body));
};

/** `/mcp/<run>/<workspace>/<role>` -> the caller, or undefined. */
export const parseContext = (path: string): MemoryContext | undefined => {
  const m = /^\/mcp\/([^/]+)\/([^/]+)\/([^/]+)$/.exec(path);
  if (!m) return undefined;
  const [, run, ws, role] = m.map((s) => (s === undefined ? s : decodeURIComponent(s)));
  return { runId: run as RunId, workspaceId: ws as WorkspaceId, roleId: role as RoleId };
};

const rpcError = (id: unknown, code: number, message: string): Json => ({
  jsonrpc: "2.0",
  id: id ?? null,
  error: { code, message },
});

const handleRpc = async (msg: Json, ctx: MemoryContext, memory: MemoryService): Promise<Json | undefined> => {
  const id = msg["id"];
  const params = (msg["params"] ?? {}) as Json;
  switch (msg["method"]) {
    case "initialize":
      return {
        jsonrpc: "2.0",
        id,
        result: {
          protocolVersion: PROTOCOL_VERSION,
          capabilities: { tools: {} },
          serverInfo: { name: "workmate-memory", version: "0.0.0" },
        },
      };
    case "ping":
      return { jsonrpc: "2.0", id, result: {} };
    case "tools/list":
      return { jsonrpc: "2.0", id, result: { tools: TOOLS } };
    case "tools/call": {
      const args = (params["arguments"] ?? {}) as Json;
      const name = params["name"];
      if (name !== "remember" && name !== "recall") return rpcError(id, -32602, `unknown tool ${String(name)}`);
      const out = name === "remember" ? await memory.remember(ctx, args) : await memory.recall(ctx, args);
      return { jsonrpc: "2.0", id, result: { content: [{ type: "text", text: out.text }], isError: out.isError } };
    }
    default:
      // A notification (no id) is acknowledged by the 202 and needs no body.
      return id === undefined ? undefined : rpcError(id, -32601, `method not found: ${String(msg["method"])}`);
  }
};

export interface McpHandle {
  readonly server: Server;
  readonly port: number;
  /** The URL a session's MCP config points at for one caller. */
  urlFor(ctx: MemoryContext): string;
  close(): Promise<void>;
}

export const startMcp = async (opts: { memory: MemoryService; token: string }): Promise<McpHandle> => {
  const server = createServer((req, res) => {
    void (async () => {
      if (req.headers["authorization"] !== `Bearer ${opts.token}`) return reply(res, 401, { error: "unauthorized" });
      const ctx = parseContext((req.url ?? "").split("?")[0] ?? "");
      if (!ctx) return reply(res, 404, { error: "unknown path" });
      if (req.method !== "POST") return reply(res, 405, { error: "POST only" });
      let msg: unknown;
      try {
        msg = JSON.parse(await readBody(req));
      } catch {
        return reply(res, 400, rpcError(null, -32700, "parse error"));
      }
      if (typeof msg !== "object" || msg === null || Array.isArray(msg)) {
        return reply(res, 400, rpcError(null, -32600, "invalid request"));
      }
      const out = await handleRpc(msg as Json, ctx, opts.memory);
      return out === undefined ? reply(res, 202) : reply(res, 200, out);
    })().catch(() => reply(res, 500, { error: "internal error" }));
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = (server.address() as AddressInfo).port;
  return {
    server,
    port,
    urlFor: (c) =>
      `http://127.0.0.1:${port}/mcp/${encodeURIComponent(c.runId)}/${encodeURIComponent(c.workspaceId)}/${encodeURIComponent(c.roleId)}`,
    close: () => new Promise((resolve) => server.close(() => resolve())),
  };
};
