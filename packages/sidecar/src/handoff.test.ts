import { describe, expect, it } from "vitest";
import { GLOBAL, memoryId, runId, type Memory } from "@workmate/core";
import { composeHandoff, type Message } from "./handoff.js";

const msg = (role: Message["role"], text: string, at: number): Message => ({ role, text, at });

const base = {
  toRole: "builder",
  objective: "Add rate limiting to the public API.",
  memories: [] as Memory[],
  scopes: [GLOBAL],
};

const mem = (subject: string, claim: string, pinned = true): Memory => ({
  id: memoryId(subject),
  subject,
  claim,
  scopes: [GLOBAL],
  recordedAt: 1,
  recordedByRun: runId("r"),
  pinned,
});

describe("composeHandoff", () => {
  it("addresses the receiving role and restates the objective", () => {
    const h = composeHandoff({
      ...base,
      transcript: [msg("assistant", "Use a token bucket.", 1)],
    });
    expect(h.prompt).toContain("You are the builder on this run.");
    expect(h.prompt).toContain("Add rate limiting to the public API.");
  });

  it("carries the previous role's conclusions, not just its last line", () => {
    const h = composeHandoff({
      ...base,
      transcript: [
        msg("assistant", "Surveyed three approaches.", 1),
        msg("assistant", "Rejected the sliding window.", 2),
        msg("assistant", "Use a token bucket.", 3),
      ],
    });
    expect(h.carried).toHaveLength(3);
    expect(h.prompt).toContain("Rejected the sliding window.");
  });

  it("bounds what it carries rather than dumping the whole transcript", () => {
    const transcript = Array.from({ length: 20 }, (_, i) => msg("assistant", `step ${i}`, i));
    const h = composeHandoff({ ...base, transcript });
    expect(h.carried).toHaveLength(3);
    expect(h.prompt).toContain("step 19");
    expect(h.prompt).not.toContain("step 0");
  });

  it("ignores the user's own turns when summarising what was concluded", () => {
    const h = composeHandoff({
      ...base,
      transcript: [msg("user", "please plan this", 1), msg("assistant", "Planned.", 2)],
    });
    expect(h.carried.map((m) => m.text)).toEqual(["Planned."]);
  });

  it("fails loudly rather than handing a role an empty brief", () => {
    expect(() => composeHandoff({ ...base, transcript: [msg("user", "hi", 1)] })).toThrow(
      /produced no assistant output/,
    );
  });

  it("puts pinned memory in the per-turn system string, where it stays auditable", () => {
    const h = composeHandoff({
      ...base,
      transcript: [msg("assistant", "done", 1)],
      memories: [mem("build", "uses pnpm")],
    });
    expect(h.system).toContain("build: uses pnpm");
  });

  it("sends no system string when nothing is pinned, rather than an empty header", () => {
    const h = composeHandoff({
      ...base,
      transcript: [msg("assistant", "done", 1)],
      memories: [mem("build", "uses pnpm", false)],
    });
    expect(h.system).toBe("");
  });
});
