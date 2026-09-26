import { describe, expect, it } from "vitest";
import { roleId, runId, sessionId } from "./ids.js";
import {
  HandoffCycleError,
  isSoloRun,
  runBranch,
  runTimeline,
  type Handoff,
  type Run,
  type RunSession,
} from "./run.js";

const s = (id: string, role: string, startedAt: number): RunSession => ({
  id: sessionId(id),
  role: roleId(role),
  directory: `/tmp/wt/${id}`,
  engineVersion: "1.18.32",
  startedAt,
});

const h = (from: string, to: string, at: number): Handoff => ({
  from: sessionId(from),
  to: sessionId(to),
  context: "…",
  at,
});

const run = (sessions: RunSession[], handoffs: Handoff[] = []): Run => ({
  id: runId("run-1"),
  sessions,
  handoffs,
  state: "running",
});

describe("runBranch", () => {
  it("never collides with a user branch", () => {
    expect(runBranch(runId("abc"))).toBe("workmate/run-abc");
  });
});

describe("runTimeline", () => {
  it("follows the handoff graph, not wall-clock, when they disagree", () => {
    // builder started first, but the planner handed off to it.
    const planner = s("p", "planner", 200);
    const builder = s("b", "builder", 100);
    const order = runTimeline(run([builder, planner], [h("p", "b", 300)]));
    expect(order.map((x) => x.id)).toEqual(["p", "b"]);
  });

  it("orders independent sessions by wall-clock", () => {
    const order = runTimeline(run([s("b", "builder", 200), s("a", "researcher", 100)]));
    expect(order.map((x) => x.id)).toEqual(["a", "b"]);
  });

  it("threads a three-role chain in handoff order", () => {
    const sessions = [s("r", "reviewer", 1), s("b", "builder", 1), s("p", "planner", 1)];
    const order = runTimeline(run(sessions, [h("p", "b", 1), h("b", "r", 2)]));
    expect(order.map((x) => x.id)).toEqual(["p", "b", "r"]);
  });

  it("ignores a handoff pointing outside the run rather than dropping a session", () => {
    const order = runTimeline(run([s("p", "planner", 1)], [h("p", "gone", 2)]));
    expect(order.map((x) => x.id)).toEqual(["p"]);
  });

  it("refuses a cyclic graph instead of silently truncating the transcript", () => {
    const cyclic = run([s("a", "x", 1), s("b", "y", 2)], [h("a", "b", 1), h("b", "a", 2)]);
    expect(() => runTimeline(cyclic)).toThrow(HandoffCycleError);
  });
});

describe("isSoloRun", () => {
  it("is true for one role and no handoffs, so the UI can render a plain chat", () => {
    expect(isSoloRun(run([s("a", "builder", 1)]))).toBe(true);
  });

  it("is false as soon as a second role joins", () => {
    expect(isSoloRun(run([s("a", "builder", 1), s("b", "reviewer", 2)]))).toBe(false);
  });
});
