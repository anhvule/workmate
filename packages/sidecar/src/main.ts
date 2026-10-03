/**
 * The workmate sidecar process.
 *
 * Owns the engine connection and the team orchestration; Rust owns the window,
 * the keychain and every write to SQLite. Persistence travels back over this
 * pipe precisely so there is exactly one writer
 * (see `.wayfinder/tickets/024-where-orchestration-runs.md`).
 *
 * Readiness is the `ready` message on stdout, never the spawn — the same rule
 * Rust applies to the engine.
 */
import { commands, type Command } from "./commands.js";
import { DbClient } from "./db.js";
import { engineAdapter } from "./engine-port.js";
import { MemoryService } from "./memory.js";
import { startMcp, type McpHandle } from "./mcp.js";
import { Orchestrator } from "./orchestrator.js";
import { OpenCodeClient } from "@workmate/opencode-client";
import {
  encodeOutbound,
  LineFramer,
  parseInbound,
  redact,
  type EngineAddress,
  type Outbound,
} from "./protocol.js";

let lastHello: EngineAddress | undefined;

const send = (msg: Outbound): void => {
  process.stdout.write(encodeOutbound(msg));
};

/** stderr only — stdout is the protocol channel and must carry nothing else. */
const log = (message: string, detail?: unknown): void => {
  const suffix = detail === undefined ? "" : ` ${JSON.stringify(redact(detail))}`;
  process.stderr.write(`sidecar: ${message}${suffix}\n`);
};

export interface SidecarState {
  engine?: OpenCodeClient | undefined;
  /** Present once the handshake has run; persistence goes through it. */
  db?: DbClient | undefined;
  /** The command table, built once the engine address is known. */
  commands?: Record<string, Command> | undefined;
  mcp?: McpHandle | undefined;
}

/** Exported for testing: the message loop, free of process wiring. */
export const handle = (
  line: string,
  state: SidecarState,
  emit: (msg: Outbound) => void,
  makeClient: (addr: EngineAddress) => OpenCodeClient = (a) => new OpenCodeClient(a),
): "continue" | "stop" => {
  let msg;
  try {
    msg = parseInbound(line);
  } catch (err) {
    emit({ type: "fault", message: err instanceof Error ? err.message : String(err) });
    return "continue";
  }

  switch (msg.type) {
    case "hello":
      state.engine = makeClient(msg.engine);
      state.db = new DbClient(emit);
      lastHello = msg.engine;
      emit({ type: "ready", pid: process.pid });
      return "continue";
    case "cmd": {
      const run = state.commands?.[msg.name];
      const { id } = msg;
      if (!run) {
        emit({ type: "cmd.result", id, ok: false, message: `unknown or premature command: ${msg.name}` });
        return "continue";
      }
      run(msg.args).then(
        (value) => emit({ type: "cmd.result", id, ok: true, value: value ?? null }),
        (err: unknown) =>
          emit({ type: "cmd.result", id, ok: false, message: err instanceof Error ? err.message : String(err) }),
      );
      return "continue";
    }
    case "shutdown":
      return "stop";
    case "db.result":
    case "db.error": {
      // Unmatched ids stay a fault, not something to swallow: a late reply to a
      // timed-out call means Rust did the write while the run believes it failed.
      const settled = state.db?.settle(
        msg.id,
        msg.type === "db.result" ? { rows: msg.rows } : { error: msg.message },
      );
      if (!settled) emit({ type: "fault", message: `unmatched persistence reply ${msg.id}` });
      return "continue";
    }
  }
};

/**
 * Build the real orchestration graph once the engine address is known.
 *
 * Separate from `handle` so the message loop stays testable without starting a
 * loopback server; `run` calls this after the handshake.
 */
export const wire = async (
  state: SidecarState,
  emit: (msg: Outbound) => void,
  addr: EngineAddress,
): Promise<void> => {
  const { randomBytes } = await import("node:crypto");
  const db = state.db;
  const client = state.engine;
  if (!db || !client) throw new Error("wire called before the handshake");
  const token = randomBytes(24).toString("hex");
  const memory = new MemoryService(db);
  const mcp = await startMcp({ memory, token });
  const addMcp = async (directory: string, name: string, config: Record<string, unknown>): Promise<void> => {
    const q = new URLSearchParams({ directory });
    const res = await fetch(`${addr.baseUrl}/mcp?${q.toString()}`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Basic ${btoa(`opencode:${addr.password}`)}`,
      },
      body: JSON.stringify({ name, config }),
    });
    if (!res.ok) throw new Error(`could not register ${name} with the engine: ${res.status}`);
  };
  const engine = engineAdapter(client, addMcp);
  const orch = new Orchestrator({
    db,
    engine,
    memory,
    emit: (e) => emit({ type: "event", name: e.name, payload: e.payload }),
    memoryUrl: (ctx) => mcp.urlFor(ctx),
    memoryAuth: `Bearer ${token}`,
    newId: (prefix) => `${prefix}_${randomBytes(16).toString("hex")}`,
  });
  state.mcp = mcp;
  state.commands = commands({ orch, db, engine, models: () => client.providers() });
};

/** Wire the message loop to the real process streams. */
export const run = (): void => {
  const framer = new LineFramer();
  const state: SidecarState = {};

  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk: string) => {
    let lines: readonly string[];
    try {
      lines = framer.push(chunk);
    } catch (err) {
      log("framing failed", { error: String(err) });
      send({ type: "fault", message: String(err) });
      return;
    }
    for (const line of lines) {
      const hadEngine = state.engine !== undefined;
      const verdict = handle(line, state, send);
      if (!hadEngine && state.engine && state.db) {
        // `ready` has been sent; the orchestration graph follows. A failure
        // here is a fault, not a crash: the process stays up to say why.
        const addr = lastHello;
        if (addr) wire(state, send, addr).catch((err: unknown) => send({ type: "fault", message: `wiring failed: ${String(err)}` }));
      }
      if (verdict === "stop") {
        log("shutdown requested");
        process.exit(0);
      }
    }
  });

  // A closed stdin means Rust is gone; there is nothing left to serve.
  process.stdin.on("end", () => {
    log("stdin closed, exiting");
    process.exit(0);
  });

  log("started, awaiting hello");
};

