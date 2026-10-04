/**
 * An in-memory stand-in for Rust and the sidecar, for the browser preview and
 * the smoke tests. It speaks the same command names and event names, so the
 * screens cannot tell the difference — which is also why it must stay small and
 * honest: it simulates, it does not implement.
 */
import type { Invoke, Listen } from "./bridge.js";
import type * as T from "./types.js";

type Args = Record<string, any>; // eslint-disable-line @typescript-eslint/no-explicit-any

export interface MockOptions {
  /** Pre-populate a workspace with a finished team run, memories and so on. */
  demo?: boolean;
  /** Milliseconds between simulated run steps. */
  tick?: number;
}

const solo: T.Role = { id: "solo.builder", name: "Builder", systemPrompt: "You build.", toolAllowlist: [] };
const trio: T.Role[] = [
  { id: "plan-build-review.planner", name: "Planner", systemPrompt: "Plan.", toolAllowlist: ["read", "grep"] },
  { id: "plan-build-review.builder", name: "Builder", systemPrompt: "Build.", toolAllowlist: [] },
  { id: "plan-build-review.reviewer", name: "Reviewer", systemPrompt: "Review.", toolAllowlist: ["read", "bash"] },
];

const packs: T.Pack[] = [
  { id: "solo", name: "Solo builder", description: "One agent, plain chat.", team: { name: "Solo", roles: [solo] }, files: [{ path: "AGENTS.md", from: "AGENTS.md" }] },
  { id: "plan-build-review", name: "Plan, build, review", description: "A planner scopes the work, a builder implements it, a reviewer checks it.", team: { name: "Plan, build, review", roles: trio }, files: [{ path: "AGENTS.md", from: "AGENTS.md" }] },
  { id: "docs-pair", name: "Docs writer and editor", description: "A writer drafts; an editor checks it against the source.", team: { name: "Docs pair", roles: [] }, files: [] },
];

export const mockBridge = (opts: MockOptions = {}): { invoke: Invoke; listen: Listen; pickFolder: () => Promise<string | null> } => {
  const demo = opts.demo ?? (typeof location !== "undefined" && location.search.includes("demo"));
  const urlTick = typeof location !== "undefined" ? /[?&]tick=(\d+)/.exec(location.search)?.[1] : undefined;
  const tick = opts.tick ?? (urlTick ? Number(urlTick) : 450);
  const listeners = new Map<string, Set<(p: unknown) => void>>();
  const fire = (name: string, payload: unknown): void => listeners.get(name)?.forEach((f) => f(payload));
  let n = 0;
  const id = (p: string): string => `${p}_${(++n).toString(16).padStart(6, "0")}`;
  const now = (): number => Date.now();

  const workspaces: T.WorkspaceView[] = [];
  const teams: T.Team[] = [];
  const runs = new Map<string, T.PersistedRun>();
  const transcripts = new Map<string, T.TranscriptMessage[]>();
  const memories: T.Memory[] = [];
  const automations: T.Automation[] = [];
  const fires: T.Fire[] = [];
  const mcp: T.McpServer[] = [];
  const installed: T.InstalledSkill[] = [];
  const keys = new Set<string>();
  let defaultModel: { provider: string | null; model: string | null } = { provider: null, model: null };
  const pending = new Map<string, { resolve: (d: { kind: string; prompt?: string; roleId?: string }) => void }>();
  const roleOf = new Map<string, T.Role[]>();

  const addWorkspace = (name: string, directory: string): T.WorkspaceView => {
    const w = { workspace: { id: id("ws"), name, directory, createdAt: now() }, binding: "present" as const };
    workspaces.push(w);
    return w;
  };

  if (demo) {
    const w = addWorkspace("acme-api", "/Users/you/code/acme-api");
    teams.push({ id: "team_1", name: "Plan, build, review", roles: trio });
    keys.add("anthropic");
    defaultModel = { provider: "anthropic", model: "claude-sonnet" };
    const rid = "run_demo";
    const ss = trio.map((r, i) => ({ id: `s_${i}`, role: r.id, directory: "/wt/run_demo", engineVersion: "1.18.32", startedAt: 1000 + i }));
    runs.set(rid, {
      id: rid, workspaceId: w.workspace.id, objective: "Add rate limiting to the public API", branch: "workmate/run-demo",
      state: "done", createdAt: now() - 3_600_000, teamId: "team_1", sessions: ss,
      handoffs: [
        { from: "s_0", to: "s_1", context: "## Objective\nAdd rate limiting to the public API\n\n## What the previous role concluded\n- Use a token bucket in middleware/rate_limit.ts, 60 req/min per key.\n- Tests live in test/middleware.\n\nContinue from there.", at: 1100 },
        { from: "s_1", to: "s_2", context: "## Objective\nAdd rate limiting to the public API\n\n## What the previous role concluded\n- Implemented the bucket and wired it into the router; 4 tests added.\n\nContinue from there.", at: 1200 },
      ],
    });
    transcripts.set("s_0", [{ role: "user", text: "Add rate limiting to the public API", at: 1 }, { role: "assistant", text: "Plan: add a token-bucket middleware (`middleware/rate_limit.ts`), 60 requests/minute per API key, return 429 with `Retry-After`. Touch the router and add tests under `test/middleware`.", at: 2 }]);
    transcripts.set("s_1", [{ role: "assistant", text: "Implemented the middleware and wired it into the router. Added 4 tests; all pass. Committed as `feat(api): rate limit public routes`.", at: 3 }]);
    transcripts.set("s_2", [{ role: "assistant", text: "Looks correct. One must-fix: `Retry-After` is computed in ms, the header takes seconds (`rate_limit.ts:41`). Everything else is fine.", at: 4 }]);
    memories.push(
      { id: "mem_1", subject: "Package manager", claim: "This project uses pnpm, not npm.", scopes: [{ kind: "workspace", workspaceId: w.workspace.id }], recordedAt: now() - 86_400_000, recordedByRun: rid, supersededBy: null, pinned: true, lastUsedAt: now() - 3_000_000 },
      { id: "mem_2", subject: "Commit style", claim: "Conventional commits, imperative summary.", scopes: [{ kind: "global" }], recordedAt: now() - 172_800_000, recordedByRun: null, supersededBy: null, pinned: false, lastUsedAt: null },
    );
    automations.push({ id: "auto_1", workspaceId: w.workspace.id, name: "Nightly dependency check", schedule: "0 3 * * *", objective: "Check for outdated dependencies with known advisories.", roles: [solo], enabled: true, nextFireAt: now() + 8 * 3_600_000 });
    fires.push({ id: "fire_1", automationId: "auto_1", scheduledFor: now() - 86_400_000, startedAt: now() - 86_400_000, runId: null, outcome: "quiet", summary: "", seen: true }, { id: "fire_2", automationId: "auto_1", scheduledFor: now() - 172_800_000, startedAt: now() - 172_800_000, runId: "run_x", outcome: "completed", summary: "2 dependencies have advisories: `semver` 7.5.1 and `tar` 6.1.0.", seen: false });
    mcp.push({ id: "mcp_1", workspaceId: null, name: "github", enabled: true, kind: "remote", url: "https://example.com/mcp" });
  }

  const sessionDir = "/wt";
  const simulate = async (runId: string, wsId: string, objective: string, team: T.Role[], review: boolean): Promise<void> => {
    const wait = (): Promise<void> => new Promise((r) => setTimeout(r, tick));
    const run = runs.get(runId)!;
    roleOf.set(runId, team);
    let prev: string | undefined;
    let prompt = objective;
    for (let i = 0; i < team.length; i++) {
      const role = team[i]!;
      const sid = id("s");
      run.sessions.push({ id: sid, role: role.id, directory: sessionDir, engineVersion: "1.18.32", startedAt: now() });
      if (prev) {
        run.handoffs.push({ from: prev, to: sid, context: prompt, at: now() });
        fire("workmate:run_handoff_delivered", { runId, from: prev, to: sid, context: prompt });
      }
      const reply = i === 0 && team.length > 1 ? `Plan for "${objective}": scope it to the smallest change, add a test first.` : `${role.name} here — done. ${team.length > 1 ? "Handing off with a summary of what I changed." : "Anything else you want me to look at?"}`;
      transcripts.set(sid, [{ role: "user", text: prompt, at: now() }]);
      await wait();
      const part = "p1";
      for (const piece of reply.match(/.{1,18}/g) ?? []) {
        fire("sidecar:message_part_delta", { envelope: { payload: { properties: { sessionID: sid, messageID: "m", partID: part, field: "text", delta: piece } } } });
        await new Promise((r) => setTimeout(r, 40));
      }
      transcripts.get(sid)!.push({ role: "assistant", text: reply, at: now() });
      fire("workmate:run_turn", { runId, sessionId: sid, role: role.name, text: reply });
      if (i === team.length - 1) break;
      prompt = `## Objective\n${objective}\n\n## What the previous role concluded\n- ${reply}\n\nContinue from there.`;
      fire("workmate:run_handoff_proposed", { runId, from: sid, to: team[i + 1]!.name, context: prompt, editable: review });
      if (review) {
        run.state = "paused";
        fire("workmate:run_changed", { runId, phase: "paused" });
        const d = await new Promise<{ kind: string; prompt?: string; roleId?: string }>((resolve) => pending.set(runId, { resolve }));
        pending.delete(runId);
        if (d.kind === "veto") {
          run.state = "done";
          fire("workmate:run_changed", { runId, phase: "done" });
          fire("workmate:run_finished", { runId, reason: "vetoed" });
          return;
        }
        if (d.kind === "amend" && d.prompt) prompt = d.prompt;
        run.state = "running";
        fire("workmate:run_changed", { runId, phase: "running" });
      }
      prev = sid;
      await wait();
    }
    run.state = "done";
    fire("workmate:run_changed", { runId, phase: "done" });
    fire("workmate:run_finished", { runId, reason: "completed" });
    void wsId;
  };

  const sidecar = async (name: string, a: Args): Promise<unknown> => {
    switch (name) {
      case "run.start": {
        const runId = id("run");
        const roles: T.Role[] = a["roles"];
        runs.set(runId, { id: runId, workspaceId: a["workspaceId"], objective: a["objective"], branch: `workmate/run-${runId.slice(4)}`, state: "running", createdAt: now(), teamId: a["teamId"] ?? null, sessions: [], handoffs: [] });
        fire("workmate:run_started", { runId, roles: roles.map((r) => r.name), solo: roles.length === 1 });
        void simulate(runId, a["workspaceId"], a["objective"], roles, a["review"] === true);
        return { runId, branch: runs.get(runId)!.branch };
      }
      case "run.say": {
        const run = runs.get(a["runId"]);
        const last = run?.sessions.at(-1);
        if (!run || !last) throw new Error("no such run");
        transcripts.get(last.id)!.push({ role: "user", text: a["text"], at: now() });
        run.state = "running";
        fire("workmate:run_changed", { runId: run.id, phase: "running" });
        setTimeout(() => {
          transcripts.get(last.id)!.push({ role: "assistant", text: `Got it: "${a["text"]}".`, at: now() });
          run.state = "done";
          fire("workmate:run_turn", { runId: run.id, sessionId: last.id, role: "Builder", text: "" });
          fire("workmate:run_changed", { runId: run.id, phase: "done" });
        }, tick);
        return null;
      }
      case "run.pause":
      case "run.resume":
        pending.get(a["runId"])?.resolve({ kind: "continue" });
        return null;
      case "run.amend":
        pending.get(a["runId"])?.resolve({ kind: "amend", prompt: a["prompt"] });
        return null;
      case "run.redirect":
        pending.get(a["runId"])?.resolve({ kind: "continue" });
        return null;
      case "run.veto":
        pending.get(a["runId"])?.resolve({ kind: "veto" });
        return null;
      case "run.transcript":
        return transcripts.get(a["sessionId"]) ?? [];
      case "run.diff":
        return { files: [{ path: "middleware/rate_limit.ts", kind: "added" }, { path: "router.ts", kind: "modified" }], additions: 64, deletions: 3, patch: "diff --git a/router.ts b/router.ts\n@@ -10,3 +10,4 @@\n import { cors } from './cors';\n+import { rateLimit } from './middleware/rate_limit';\n app.use(cors());\n-app.use(logger);\n+app.use(rateLimit({ perMinute: 60 }));\n" } satisfies T.RunDiff;
      case "run.status":
        return [];
      case "run.merge":
        return "fastForward";
      case "run.archive":
      case "run.abandon":
        runs.get(a["runId"])!.state = "archived";
        return null;
      case "permission.reply":
        return null;
      case "models.list":
        return [{ id: "anthropic", name: "Anthropic", models: [{ id: "claude-sonnet", name: "Claude Sonnet" }, { id: "claude-haiku", name: "Claude Haiku" }] }, { id: "openai", name: "OpenAI", models: [{ id: "gpt-5", name: "GPT-5" }] }] satisfies T.ProviderInfo[];
      default:
        throw new Error(`mock: unknown sidecar command ${name}`);
    }
  };

  const inv: Invoke = async <R,>(cmd: string, args: Args = {}): Promise<R> => {
    const ok = (v: unknown): R => v as R;
    switch (cmd) {
      case "start_runtime": return ok("http://127.0.0.1:4096");
      case "logs_reveal": return ok("/Users/you/Library/Logs/dev.workmate.app");
      case "engine_info": return ok({ pinnedVersion: "1.18.32", sidecarPresent: true });
      case "sidecar_command": return ok(await sidecar(args["name"], args["args"] ?? {}));
      case "workspace_list": return ok(workspaces);
      case "workspace_create": return ok(addWorkspace(args["name"], args["directory"]));
      case "workspace_open": return ok(workspaces.find((w) => w.workspace.id === args["id"]));
      case "workspace_relocate": { const w = workspaces.find((x) => x.workspace.id === args["id"])!; w.workspace.directory = args["directory"]; w.binding = "present"; return ok(w); }
      case "workspace_remove": workspaces.splice(workspaces.findIndex((w) => w.workspace.id === args["id"]), 1); return ok(null);
      case "repo_available": return ok(true);
      case "run_list": return ok([...runs.values()].filter((r) => r.workspaceId === args["workspaceId"]).sort((a, b) => b.createdAt - a.createdAt).map(({ sessions: _s, handoffs: _h, workspaceId: _w, ...r }) => r));
      case "run_get": return ok(structuredClone(runs.get(args["runId"])));
      case "run_reveal_worktree": return ok("/wt");
      case "settings_default_model": return ok(defaultModel);
      case "settings_set_default_model": defaultModel = { provider: args["provider"] || null, model: args["model"] || null }; return ok(null);
      case "credential_status": return ok(keys.has(args["provider"]) ? { found: true, scope: "global", tried: [] } : { found: false, scope: null, tried: [`v1:global:${args["provider"]}`] });
      case "credential_set": keys.add(args["provider"]); return ok(null);
      case "credential_delete": keys.delete(args["provider"]); return ok(null);
      case "team_list": return ok(teams);
      case "team_update_role": teams.forEach((t) => t.roles.forEach((r, i) => { if (r.id === args["role"].id) t.roles[i] = args["role"]; })); return ok(null);
      case "team_remove": teams.splice(teams.findIndex((t) => t.id === args["id"]), 1); return ok(null);
      case "pack_list": return ok(packs);
      case "pack_preview": return ok({ willWrite: packs.find((p) => p.id === args["packId"])!.files.map((f) => f.path), willSkip: [] });
      case "pack_apply": {
        const p = packs.find((x) => x.id === args["packId"])!;
        const team = { id: id("team"), name: p.team.name, roles: p.team.roles.map((r) => ({ ...r, id: `${p.id}.${r.id.split(".").pop()}` })) };
        teams.push(team);
        return ok({ team, written: p.files.map((f) => f.path), skipped: [] });
      }
      case "skill_catalog": return ok([{ name: "conventional-commits", description: "Write commit messages in the Conventional Commits style.", dir: "/s/cc", checksum: "ab12", origin: "workmate" }, { name: "review-checklist", description: "A checklist for reviewing a code change.", dir: "/s/rc", checksum: "cd34", origin: "workmate" }]);
      case "skill_installed": return ok(installed);
      case "skill_install": installed.push({ name: args["name"], origin: args["origin"], modified: false }); return ok(null);
      case "skill_uninstall": installed.splice(installed.findIndex((s) => s.name === args["name"]), 1); return ok(null);
      case "skill_sources": return ok([]);
      case "skill_add_source": return ok({ id: id("src"), url: args["url"], lastSyncedAt: null });
      case "skill_remove_source": return ok(null);
      case "skill_sync": return ok(0);
      case "memory_list": return ok(memories.filter((m) => args["includeSuperseded"] || !m.supersededBy));
      case "memory_update": { const m = memories.find((x) => x.id === args["id"])!; Object.assign(m, Object.fromEntries(Object.entries(args).filter(([k, v]) => ["subject", "claim", "pinned"].includes(k) && v != null))); return ok(null); }
      case "memory_delete": memories.splice(memories.findIndex((m) => m.id === args["id"]), 1); return ok(null);
      case "memory_export": return ok(`# workmate memory\n\n${memories.map((m) => `- **${m.subject}** — ${m.claim}`).join("\n")}\n`);
      case "automation_list": return ok(automations);
      case "automation_create": { const a = { id: id("auto"), ...args, enabled: true, nextFireAt: now() + 3_600_000 } as T.Automation; automations.push(a); return ok(a); }
      case "automation_set_enabled": automations.find((a) => a.id === args["id"])!.enabled = args["enabled"]; return ok(null);
      case "automation_remove": automations.splice(automations.findIndex((a) => a.id === args["id"]), 1); return ok(null);
      case "automation_run_now": return ok(null);
      case "automation_history": return ok(fires.filter((f) => !args["automationId"] || f.automationId === args["automationId"]));
      case "automation_mark_seen": fires.find((f) => f.id === args["fireId"])!.seen = true; return ok(null);
      case "automation_unseen": return ok(fires.filter((f) => !f.seen && ["completed", "blocked", "failed"].includes(f.outcome)).length);
      case "mcp_list": return ok(mcp);
      case "mcp_add": { const s = { id: id("mcp"), workspaceId: args["workspaceId"], name: args["name"], enabled: true, ...args["transport"] } as T.McpServer; mcp.push(s); return ok(s); }
      case "mcp_set_enabled": mcp.find((s) => s.id === args["id"])!.enabled = args["enabled"]; return ok(null);
      case "mcp_remove": mcp.splice(mcp.findIndex((s) => s.id === args["id"]), 1); return ok(null);
      default: throw new Error(`mock: unknown command ${cmd}`);
    }
  };

  const lis: Listen = async (name, cb) => {
    const set = listeners.get(name) ?? new Set();
    set.add(cb);
    listeners.set(name, set);
    return () => set.delete(cb);
  };

  return { invoke: inv, listen: lis, pickFolder: async () => "/Users/you/code/my-project" };
};
