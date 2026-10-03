/**
 * The Rust ↔ sidecar wire protocol.
 *
 * Newline-delimited JSON over stdin/stdout, because it is the one framing both
 * ends get right without a dependency. The parsing rules here are the fiddly
 * part — a partial line carried across chunk boundaries, and a bound on how
 * much can be carried before a malformed stream is treated as a fault rather
 * than buffered forever.
 */

/** Rust → sidecar. */
export type Inbound =
  | { readonly type: "hello"; readonly engine: EngineAddress }
  | { readonly type: "db.result"; readonly id: string; readonly rows: readonly unknown[] }
  | { readonly type: "db.error"; readonly id: string; readonly message: string }
  /** A command from the webview, relayed by Rust. Answered with `cmd.result`. */
  | { readonly type: "cmd"; readonly id: string; readonly name: string; readonly args: Record<string, unknown> }
  | { readonly type: "shutdown" };

/** sidecar → Rust. */
export type Outbound =
  /** Readiness. Rust waits for this, never for the spawn itself. */
  | { readonly type: "ready"; readonly pid: number }
  /** A named operation, never SQL: the schema stays Rust's (ticket 026). */
  | { readonly type: "db.call"; readonly id: string; readonly op: string; readonly args: Record<string, unknown> }
  | { readonly type: "event"; readonly name: string; readonly payload: unknown }
  | { readonly type: "cmd.result"; readonly id: string; readonly ok: true; readonly value: unknown }
  | { readonly type: "cmd.result"; readonly id: string; readonly ok: false; readonly message: string }
  | { readonly type: "fault"; readonly message: string };

export interface EngineAddress {
  readonly baseUrl: string;
  readonly password: string;
}

/**
 * Beyond this, an unterminated line is a malformed stream rather than a big
 * message. Cowork-z carries 1MiB for the same reason; the number matters less
 * than having one.
 */
export const MAX_PENDING_BYTES = 1024 * 1024;

export class ProtocolError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ProtocolError";
  }
}

/**
 * Accumulates chunks and yields whole messages.
 *
 * Stateful by necessity: a chunk boundary can fall anywhere, including inside a
 * multi-byte character, so decoding is the caller's job and framing is ours.
 */
export class LineFramer {
  private pending = "";

  /**
   * Feed a chunk; returns the complete lines it completed.
   *
   * @throws {ProtocolError} if an unterminated line exceeds {@link MAX_PENDING_BYTES}.
   */
  push(chunk: string): readonly string[] {
    this.pending += chunk;
    const parts = this.pending.split("\n");
    // The final element is whatever follows the last newline — possibly "".
    this.pending = parts.pop() ?? "";
    if (this.pending.length > MAX_PENDING_BYTES) {
      const size = this.pending.length;
      this.pending = "";
      throw new ProtocolError(
        `unterminated message exceeded ${MAX_PENDING_BYTES} bytes (${size}); treating the stream as malformed`,
      );
    }
    return parts.filter((line) => line.trim().length > 0);
  }

  /** Bytes currently held back awaiting a newline. */
  get pendingBytes(): number {
    return this.pending.length;
  }
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null;

/**
 * Parse one line as an inbound message.
 *
 * Validates the discriminant rather than trusting it: the peer is another
 * process, and a wrong shape here would surface as an unrelated crash later.
 *
 * @throws {ProtocolError} on malformed JSON or an unrecognised message.
 */
export const parseInbound = (line: string): Inbound => {
  let value: unknown;
  try {
    value = JSON.parse(line);
  } catch {
    throw new ProtocolError(`not JSON: ${line.slice(0, 120)}`);
  }
  if (!isRecord(value) || typeof value["type"] !== "string") {
    throw new ProtocolError("message has no type");
  }
  switch (value["type"]) {
    case "hello": {
      const engine = value["engine"];
      if (
        !isRecord(engine) ||
        typeof engine["baseUrl"] !== "string" ||
        typeof engine["password"] !== "string"
      ) {
        throw new ProtocolError("hello is missing a usable engine address");
      }
      return {
        type: "hello",
        engine: { baseUrl: engine["baseUrl"], password: engine["password"] },
      };
    }
    case "db.result": {
      const id = value["id"];
      const rows = value["rows"];
      if (typeof id !== "string" || !Array.isArray(rows)) {
        throw new ProtocolError("db.result is missing id or rows");
      }
      return { type: "db.result", id, rows };
    }
    case "db.error": {
      const id = value["id"];
      const message = value["message"];
      if (typeof id !== "string" || typeof message !== "string") {
        throw new ProtocolError("db.error is missing id or message");
      }
      return { type: "db.error", id, message };
    }
    case "cmd": {
      const { id, name, args } = value;
      if (typeof id !== "string" || typeof name !== "string") {
        throw new ProtocolError("cmd is missing id or name");
      }
      return { type: "cmd", id, name, args: isRecord(args) ? args : {} };
    }
    case "shutdown":
      return { type: "shutdown" };
    default:
      throw new ProtocolError(`unrecognised message type: ${String(value["type"])}`);
  }
};

export const encodeOutbound = (msg: Outbound): string => `${JSON.stringify(msg)}\n`;

/**
 * Strip anything that must never be written to a log or forwarded onward.
 *
 * The engine's launch password reaches the sidecar and must go no further —
 * the same discipline as the api-key fingerprint bridge that keeps real keys
 * out of the webview.
 */
export const redact = (value: unknown): unknown => {
  if (Array.isArray(value)) return value.map(redact);
  if (isRecord(value)) {
    return Object.fromEntries(
      Object.entries(value).map(([k, v]) =>
        /password|token|secret|apikey|api_key/i.test(k) ? [k, "[redacted]"] : [k, redact(v)],
      ),
    );
  }
  return value;
};
