import { describe, expect, it, vi } from "vitest";
import { handle, type SidecarState } from "./main.js";
import type { Outbound } from "./protocol.js";

const collect = () => {
  const out: Outbound[] = [];
  return { out, emit: (m: Outbound) => out.push(m) };
};

const hello = JSON.stringify({
  type: "hello",
  engine: { baseUrl: "http://127.0.0.1:1", password: "hunter2" },
});

describe("handle", () => {
  it("announces ready only after hello, so spawn is never mistaken for readiness", () => {
    const { out, emit } = collect();
    const state: SidecarState = {};
    const make = vi.fn(() => ({}) as never);

    expect(state.engine).toBeUndefined();
    handle(hello, state, emit, make);

    expect(out).toEqual([{ type: "ready", pid: process.pid }]);
    expect(state.engine).toBeDefined();
    expect(make).toHaveBeenCalledWith({ baseUrl: "http://127.0.0.1:1", password: "hunter2" });
  });

  it("stops on shutdown", () => {
    const { emit } = collect();
    expect(handle(JSON.stringify({ type: "shutdown" }), {}, emit)).toBe("stop");
  });

  it("reports a malformed message as a fault and keeps serving", () => {
    const { out, emit } = collect();
    expect(handle("{not json", {}, emit)).toBe("continue");
    expect(out[0]).toMatchObject({ type: "fault" });
  });

  it("does not swallow a persistence reply nobody is waiting for", () => {
    const { out, emit } = collect();
    handle(JSON.stringify({ type: "db.result", id: "x", rows: [] }), {}, emit);
    expect(out[0]).toMatchObject({ type: "fault", message: /unmatched/ as unknown as string });
  });
});
