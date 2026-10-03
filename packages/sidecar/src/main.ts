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
import { DbClient } from "./db.js";
import { OpenCodeClient } from "@workmate/opencode-client";
import {
  encodeOutbound,
  LineFramer,
  parseInbound,
  redact,
  type EngineAddress,
  type Outbound,
} from "./protocol.js";

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
      emit({ type: "ready", pid: process.pid });
      return "continue";
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
      if (handle(line, state, send) === "stop") {
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

