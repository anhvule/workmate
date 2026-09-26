import { describe, expect, it } from "vitest";
import {
  encodeOutbound,
  LineFramer,
  MAX_PENDING_BYTES,
  parseInbound,
  ProtocolError,
  redact,
} from "./protocol.js";

describe("LineFramer", () => {
  it("yields whole lines and holds back the partial one", () => {
    const f = new LineFramer();
    expect(f.push('{"a":1}\n{"b":2}\n{"c"')).toEqual(['{"a":1}', '{"b":2}']);
    expect(f.pendingBytes).toBe(4);
  });

  it("completes a message split across chunk boundaries", () => {
    const f = new LineFramer();
    expect(f.push('{"type":"shut')).toEqual([]);
    expect(f.push('down"}\n')).toEqual(['{"type":"shutdown"}']);
  });

  it("ignores blank lines rather than emitting empty messages", () => {
    const f = new LineFramer();
    expect(f.push("\n\n  \n")).toEqual([]);
  });

  it("treats an unbounded unterminated line as a malformed stream", () => {
    const f = new LineFramer();
    expect(() => f.push("x".repeat(MAX_PENDING_BYTES + 1))).toThrow(ProtocolError);
  });

  it("recovers after a malformed stream instead of staying wedged", () => {
    const f = new LineFramer();
    expect(() => f.push("x".repeat(MAX_PENDING_BYTES + 1))).toThrow();
    expect(f.pendingBytes).toBe(0);
    expect(f.push('{"type":"shutdown"}\n')).toEqual(['{"type":"shutdown"}']);
  });
});

describe("parseInbound", () => {
  it("accepts a hello carrying the engine address", () => {
    const msg = parseInbound(
      JSON.stringify({ type: "hello", engine: { baseUrl: "http://127.0.0.1:1", password: "p" } }),
    );
    expect(msg).toEqual({
      type: "hello",
      engine: { baseUrl: "http://127.0.0.1:1", password: "p" },
    });
  });

  it("rejects a hello without a usable address rather than starting half-configured", () => {
    expect(() => parseInbound(JSON.stringify({ type: "hello", engine: {} }))).toThrow(
      /usable engine address/,
    );
  });

  it("rejects an unknown type instead of silently ignoring it", () => {
    expect(() => parseInbound(JSON.stringify({ type: "launch_missiles" }))).toThrow(
      /unrecognised message type/,
    );
  });

  it("rejects malformed JSON with the offending text, truncated", () => {
    expect(() => parseInbound("{not json")).toThrow(/not JSON/);
  });

  it("rejects a bare value that is not a message", () => {
    expect(() => parseInbound("42")).toThrow(/no type/);
  });
});

describe("encodeOutbound", () => {
  it("terminates every message with a newline so the peer can frame it", () => {
    expect(encodeOutbound({ type: "ready", pid: 7 })).toBe('{"type":"ready","pid":7}\n');
  });
});

describe("redact", () => {
  it("hides the engine password so it cannot reach a log", () => {
    expect(redact({ engine: { baseUrl: "u", password: "hunter2" } })).toEqual({
      engine: { baseUrl: "u", password: "[redacted]" },
    });
  });

  it("hides secrets nested in arrays", () => {
    expect(redact([{ apiKey: "sk-1" }])).toEqual([{ apiKey: "[redacted]" }]);
  });

  it("leaves ordinary values alone", () => {
    expect(redact({ n: 1, s: "x", b: true, nul: null })).toEqual({
      n: 1,
      s: "x",
      b: true,
      nul: null,
    });
  });
});
