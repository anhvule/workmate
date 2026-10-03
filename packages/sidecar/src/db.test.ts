import { afterEach, describe, expect, it, vi } from "vitest";
import { DbClient, DbError } from "./db.js";
import type { Outbound } from "./protocol.js";

const setup = (timeoutMs?: number) => {
  const sent: Outbound[] = [];
  return { sent, db: new DbClient((m) => sent.push(m), timeoutMs) };
};

afterEach(() => vi.useRealTimers());

describe("DbClient", () => {
  it("sends a named operation and resolves with the rows for its id", async () => {
    const { sent, db } = setup();
    const p = db.call("run.load", { id: "r" });
    expect(sent).toEqual([{ type: "db.call", id: "1", op: "run.load", args: { id: "r" } }]);
    expect(db.settle("1", { rows: [{ ok: true }] })).toBe(true);
    await expect(p).resolves.toEqual([{ ok: true }]);
    expect(db.inFlight).toBe(0);
  });

  it("matches replies by id even when they arrive out of order", async () => {
    const { db } = setup();
    const a = db.call("a");
    const b = db.call("b");
    db.settle("2", { rows: ["B"] });
    db.settle("1", { rows: ["A"] });
    await expect(a).resolves.toEqual(["A"]);
    await expect(b).resolves.toEqual(["B"]);
  });

  it("rejects with Rust's message on a db.error", async () => {
    const { db } = setup();
    const p = db.call("run.setState");
    db.settle("1", { error: "no such run" });
    await expect(p).rejects.toThrow(new DbError("no such run"));
  });

  it("fails a call whose reply never comes instead of hanging", async () => {
    vi.useFakeTimers();
    const { db } = setup(50);
    const p = db.call("run.create");
    const assertion = expect(p).rejects.toThrow(/timed out after 50ms/);
    await vi.advanceTimersByTimeAsync(60);
    await assertion;
    expect(db.inFlight).toBe(0);
  });

  it("reports a reply nobody is waiting for, including one that arrives after the timeout", () => {
    const { db } = setup();
    expect(db.settle("99", { rows: [] })).toBe(false);
  });

  it("emits calls in the order they were made", () => {
    const { sent, db } = setup();
    void db.call("first").catch(() => undefined);
    void db.call("second").catch(() => undefined);
    expect(sent.map((m) => (m.type === "db.call" ? m.op : ""))).toEqual(["first", "second"]);
    db.settle("1", { rows: [] });
    db.settle("2", { rows: [] });
  });
});
