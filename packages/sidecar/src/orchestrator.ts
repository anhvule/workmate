/**
 * Run and team orchestration: the differentiator itself (ticket 014).
 *
 * A run is a sequence of roles on one objective. Each role gets its own session
 * in the run's single worktree, with its own system prompt, model and tool
 * allowlist; between roles there is a *handoff* that workmate composes, stores,
 * and — if the user has paused — lets them amend, redirect or veto.
 *
 * Three properties are load-bearing:
 *
 * - **A run owns its sessions.** Nothing here deletes a session because another
 *   run started; that cull is the cowork-z behaviour this design exists to
 *   avoid. Concurrency is a bounded pool of *turns*, keyed on nothing global.
 * - **One path for every run.** A one-role run goes through the same loop and
 *   simply has no boundary to stop at, so it reads as a plain chat (ticket 005).
 * - **Rust writes, this decides.** Every durable fact goes out as a named
 *   operation; every filesystem, git and keychain effect is a host operation.
 */
import {
  type Message,
  composeHandoff,
} from "./handoff.js";
import type { RoleId, RunId, SessionId, WorkspaceId } from "@workmate/core";
import type { DbClient } from "./db.js";
import type { MemoryContext, MemoryService } from "./memory.js";

export interface RoleSpec {
  readonly id: RoleId;
  readonly name: string;
  readonly systemPrompt: string;
  readonly providerId?: string | undefined;
  readonly modelId?: string | undefined;
  /** Tool names this role may use. Empty means the engine's defaults. */
  readonly toolAllowlist: readonly string[];
}

export interface StartRun {
  readonly workspaceId: WorkspaceId;
  readonly objective: string;
  readonly roles: readonly RoleSpec[];
  readonly teamId?: string | undefined;
  /** Stop at every handoff for the user, as if they had paused. */
  readonly review?: boolean | undefined;
}

/** What the orchestrator needs from the engine; the real adapter wraps the HTTP client. */
export interface EnginePort {
  createSession(
    directory: string,
    body: { title: string; permission: readonly unknown[]; model?: { providerID: string; id: string } },
  ): Promise<{ id: string }>;
  /** Blocks until the turn completes; resolves to the assistant's text. */
  sendMessage(
    sessionId: string,
    directory: string,
    body: { prompt: string; system: string; model?: { providerID: string; modelID: string }; tools?: Record<string, boolean> },
  ): Promise<{ text: string }>;
  listMessages(sessionId: string, directory: string): Promise<readonly Message[]>;
  /** Register an MCP server for a directory's engine instance. Replaces one of the same name. */
  registerMcp(directory: string, name: string, config: Record<string, unknown>): Promise<void>;
}

export interface Worktree {
  readonly path: string;
  readonly branch: string;
  readonly base: string;
}

export type Phase = "running" | "paused" | "blocked" | "done" | "archived";

export type RunEvent =
  | { name: "run.started"; payload: { runId: string; roles: string[]; solo: boolean } }
  | { name: "run.changed"; payload: { runId: string; phase: Phase } }
  | { name: "run.turn"; payload: { runId: string; sessionId: string; role: string; text: string } }
  | { name: "run.handoff.proposed"; payload: { runId: string; from: string; to: string; context: string; editable: boolean } }
  | { name: "run.handoff.delivered"; payload: { runId: string; from: string; to: string; context: string } }
  | { name: "run.blocked"; payload: { runId: string; reason: BlockReason } }
  | { name: "run.finished"; payload: { runId: string; reason: "completed" | "vetoed" } };

export type BlockReason =
  | { kind: "credential"; role: string; providerId: string; tried: readonly string[] }
  | { kind: "engine"; role: string; message: string };

export interface Deps {
  readonly db: DbClient;
  readonly engine: EnginePort;
  readonly memory: MemoryService;
  readonly emit: (e: RunEvent) => void;
  /** URL of the memory MCP endpoint for one caller. */
  readonly memoryUrl: (ctx: MemoryContext) => string;
  /** Header the engine must send to the memory endpoint. */
  readonly memoryAuth: string;
  readonly newId: (prefix: string) => string;
  /** Upper bound on turns running at once. */
  readonly maxConcurrentTurns?: number;
}

type Decision =
  | { kind: "continue" }
  | { kind: "amend"; prompt: string }
  | { kind: "redirect"; roleId: RoleId }
  | { kind: "veto" };

interface Pending {
  readonly from: SessionId;
  readonly toIndex: number;
  prompt: string;
}

/** A bounded set of turn slots. Fair, and keyed on nothing global. */
export class TurnPool {
  private active = 0;
  private readonly waiting: (() => void)[] = [];
  constructor(private readonly max: number) {}

  async run<T>(fn: () => Promise<T>): Promise<T> {
    if (this.active >= this.max) await new Promise<void>((r) => this.waiting.push(r));
    else this.active += 1;
    try {
      return await fn();
    } finally {
      const next = this.waiting.shift();
      if (next) next(); // hand the slot straight over
      else this.active -= 1;
    }
  }

  get running(): number {
    return this.active;
  }
}

/** A promise that something else resolves: how a paused run waits for the user. */
class Gate<T> {
  private resolve!: (v: T) => void;
  readonly promise: Promise<T>;
  constructor() {
    this.promise = new Promise<T>((r) => (this.resolve = r));
  }
  open(v: T): void {
    this.resolve(v);
  }
}

const roleOf = (roles: readonly RoleSpec[], i: number): RoleSpec => {
  const r = roles[i];
  if (!r) throw new Error(`run has no role at position ${i}`);
  return r;
};

export class Run {
  phase: Phase = "running";
  pending: Pending | undefined;
  private pauseRequested = false;
  private gate: Gate<Decision> | undefined;
  private resumeGate: Gate<void> | undefined;
  private lastSession: SessionId | undefined;
  private done: Promise<void> = Promise.resolve();

  constructor(
    readonly id: RunId,
    private readonly input: StartRun,
    readonly worktree: Worktree,
    private readonly deps: Deps,
    private readonly pool: TurnPool,
  ) {}

  /** Resolves when the run has reached a terminal phase. */
  get finished(): Promise<void> {
    return this.done;
  }

  get solo(): boolean {
    return this.input.roles.length === 1;
  }

  begin(): void {
    this.done = this.drive().catch((err: unknown) => {
      this.deps.emit({ name: "run.blocked", payload: { runId: this.id, reason: { kind: "engine", role: "", message: String(err) } } });
      return this.setPhase("blocked");
    });
  }

  // ----- the user's controls ------------------------------------------------

  /** Takes effect at the next handoff; a turn in flight is not interrupted. */
  pause(): void {
    this.pauseRequested = true;
  }

  resume(): void {
    this.pauseRequested = false;
    if (this.gate && this.pending) this.gate.open({ kind: "continue" });
    this.resumeGate?.open();
  }

  amend(prompt: string): void {
    if (!this.pending || !this.gate) throw new Error("there is no pending handoff to amend");
    this.gate.open({ kind: "amend", prompt });
  }

  redirect(roleId: RoleId): void {
    if (!this.gate) throw new Error("there is no pending handoff to redirect");
    this.gate.open({ kind: "redirect", roleId });
  }

  veto(): void {
    if (!this.gate) throw new Error("there is no pending handoff to veto");
    this.gate.open({ kind: "veto" });
  }

  // ----- the loop -----------------------------------------------------------

  private async setPhase(phase: Phase): Promise<void> {
    this.phase = phase;
    await this.deps.db.call("run.setState", { id: this.id, state: phase });
    this.deps.emit({ name: "run.changed", payload: { runId: this.id, phase } });
  }

  private async block(reason: BlockReason): Promise<void> {
    await this.setPhase("blocked");
    this.deps.emit({ name: "run.blocked", payload: { runId: this.id, reason } });
    this.resumeGate = new Gate<void>();
    await this.resumeGate.promise;
    this.resumeGate = undefined;
    await this.setPhase("running");
  }

  /** Make sure the role's credential is in the engine, blocking until it is. */
  private async ensureCredential(role: RoleSpec): Promise<void> {
    if (role.providerId === undefined) return;
    for (;;) {
      const rows = await this.deps.db.call("credentials.provision", {
        providerId: role.providerId,
        roleId: role.id,
        workspaceId: this.input.workspaceId,
      });
      const answer = rows[0] as { status: string; tried?: string[] } | undefined;
      if (answer?.status === "ok") return;
      await this.block({ kind: "credential", role: role.name, providerId: role.providerId, tried: answer?.tried ?? [] });
    }
  }

  private async openSession(role: RoleSpec): Promise<SessionId> {
    const rules = (await this.deps.db.call("permission.ruleset", {
      workspaceId: this.input.workspaceId,
      runId: this.id,
    })) as readonly unknown[];
    const body: Parameters<EnginePort["createSession"]>[1] = {
      title: `${role.name}: ${this.input.objective}`.slice(0, 120),
      permission: (rules[0] as unknown[] | undefined) ?? [],
      ...(role.providerId && role.modelId ? { model: { providerID: role.providerId, id: role.modelId } } : {}),
    };
    const { id } = await this.deps.engine.createSession(this.worktree.path, body);
    await this.deps.db.call("session.record", {
      id,
      runId: this.id,
      roleId: role.id,
      directory: this.worktree.path,
      engineVersion: ENGINE_VERSION,
    });
    return id as SessionId;
  }

  /**
   * The `tools` map for a turn. An allowlist means *these on, everything else
   * off*: the engine enables any tool not mentioned, so exclusivity has to be
   * spelled out — every built-in and every configured MCP server's tools are
   * set explicitly. A server is named in the allowlist as `name` or `name_*`.
   * Memory tools are always on: a role that cannot remember is not on the team.
   */
  private tools(role: RoleSpec, servers: readonly string[]): Record<string, boolean> | undefined {
    if (role.toolAllowlist.length === 0) return undefined;
    const allowed = new Set(role.toolAllowlist);
    const out: Record<string, boolean> = {};
    for (const t of BUILTIN_TOOLS) out[t] = allowed.has(t);
    for (const s of servers) out[`${s}_*`] = allowed.has(s) || allowed.has(`${s}_*`);
    for (const t of allowed) out[t] = true;
    for (const t of MEMORY_TOOLS) out[t] = true;
    return out;
  }

  /** The user's enabled MCP servers, delivered to the engine before each turn. */
  private async deliverMcp(ctx: MemoryContext): Promise<string[]> {
    const configs = (await this.deps.db.call("mcp.configs", { workspaceId: this.input.workspaceId })) as {
      name: string;
      config: Record<string, unknown>;
    }[];
    for (const c of configs) await this.deps.engine.registerMcp(this.worktree.path, c.name, c.config);
    await this.deps.engine.registerMcp(this.worktree.path, MEMORY_SERVER, {
      type: "remote",
      url: this.deps.memoryUrl(ctx),
      headers: { authorization: this.deps.memoryAuth },
    });
    return configs.map((c) => c.name);
  }

  /** One role's turn, retrying on the same session if the engine fails. */
  private async takeTurn(role: RoleSpec, session: SessionId, prompt: string): Promise<string> {
    const ctx: MemoryContext = { runId: this.id, workspaceId: this.input.workspaceId, roleId: role.id };
    for (;;) {
      try {
        const servers = await this.deliverMcp(ctx);
        const digest = await this.deps.memory.digestFor(ctx);
        const system = [role.systemPrompt, digest].filter((s) => s.trim() !== "").join("\n\n");
        const tools = this.tools(role, servers);
        const { text } = await this.pool.run(() =>
          this.deps.engine.sendMessage(session, this.worktree.path, {
            prompt,
            system,
            ...(role.providerId && role.modelId ? { model: { providerID: role.providerId, modelID: role.modelId } } : {}),
            ...(tools ? { tools } : {}),
          }),
        );
        this.deps.emit({ name: "run.turn", payload: { runId: this.id, sessionId: session, role: role.name, text } });
        return text;
      } catch (err) {
        await this.block({ kind: "engine", role: role.name, message: err instanceof Error ? err.message : String(err) });
      }
    }
  }

  /** The boundary between two roles. Returns the decision, or auto-continues. */
  private async boundary(): Promise<Decision> {
    const review = this.input.review === true;
    const editable = this.pauseRequested || review;
    const p = this.pending;
    if (!p) throw new Error("boundary without a pending handoff");
    const roles = this.input.roles;
    this.deps.emit({
      name: "run.handoff.proposed",
      payload: { runId: this.id, from: p.from, to: roleOf(roles, p.toIndex).name, context: p.prompt, editable },
    });
    if (!editable) return { kind: "continue" };
    this.gate = new Gate<Decision>();
    await this.setPhase("paused");
    const decision = await this.gate.promise;
    this.gate = undefined;
    this.pauseRequested = false;
    return decision;
  }

  private async drive(): Promise<void> {
    const roles = this.input.roles;
    let index = 0;
    let incoming: { from: SessionId; prompt: string } | undefined;

    while (index < roles.length) {
      const role = roleOf(roles, index);
      await this.ensureCredential(role);
      const session = await this.openSession(role);

      if (incoming) {
        await this.deps.db.call("handoff.append", {
          id: this.deps.newId("hof"),
          runId: this.id,
          from: incoming.from,
          to: session,
          context: incoming.prompt,
        });
        this.deps.emit({ name: "run.handoff.delivered", payload: { runId: this.id, from: incoming.from, to: session, context: incoming.prompt } });
      }

      await this.takeTurn(role, session, incoming?.prompt ?? this.input.objective);
      this.lastSession = session;

      if (index === roles.length - 1) break;

      // Compose the next role's brief from this session's actual transcript.
      const transcript = await this.deps.engine.listMessages(session, this.worktree.path);
      let toIndex = index + 1;
      const next = roleOf(roles, toIndex);
      const composed = composeHandoff({ transcript, toRole: next.name, objective: this.input.objective, memories: [], scopes: [] });
      this.pending = { from: session, toIndex, prompt: composed.prompt };

      const decision = await this.boundary();
      if (decision.kind === "veto") {
        this.pending = undefined;
        await this.setPhase("done");
        this.deps.emit({ name: "run.finished", payload: { runId: this.id, reason: "vetoed" } });
        return;
      }
      if (decision.kind === "amend") this.pending.prompt = decision.prompt;
      if (decision.kind === "redirect") {
        const target = roles.findIndex((r) => r.id === decision.roleId);
        if (target < 0) throw new Error(`cannot redirect: ${decision.roleId} is not on this team`);
        toIndex = target;
      }
      if (this.phase === "paused") await this.setPhase("running");

      incoming = { from: session, prompt: this.pending.prompt };
      this.pending = undefined;
      index = toIndex;
    }

    await this.setPhase("done");
    this.deps.emit({ name: "run.finished", payload: { runId: this.id, reason: "completed" } });
  }

  get last(): SessionId | undefined {
    return this.lastSession;
  }
}

/** The engine's built-in tools, named so an allowlist can switch the rest off. */
export const BUILTIN_TOOLS = ["bash", "edit", "write", "read", "grep", "glob", "list", "patch", "webfetch", "task", "todowrite", "todoread"] as const;

/** The memory server's name, and the tools it provides as the engine names them. */
export const MEMORY_SERVER = "workmate-memory";
export const MEMORY_TOOLS = [`${MEMORY_SERVER}_remember`, `${MEMORY_SERVER}_recall`] as const;

/** The engine version sessions are recorded against; set once from the pin. */
export const ENGINE_VERSION = "1.18.32";

/** All runs in flight, and the shared turn pool. */
export class Orchestrator {
  private readonly runs = new Map<string, Run>();
  private readonly pool: TurnPool;

  constructor(private readonly deps: Deps) {
    this.pool = new TurnPool(deps.maxConcurrentTurns ?? 3);
  }

  get(id: string): Run {
    const r = this.runs.get(id);
    if (!r) throw new Error(`no such run: ${id}`);
    return r;
  }

  async start(input: StartRun): Promise<{ runId: string; branch: string }> {
    if (input.roles.length === 0) throw new Error("a run needs at least one role");
    const id = this.deps.newId("run") as RunId;
    const rows = await this.deps.db.call("repo.createWorktree", { workspaceId: input.workspaceId, runId: id });
    const worktree = rows[0] as Worktree;
    try {
      for (const r of input.roles) {
        await this.deps.db.call("role.upsert", {
          id: r.id,
          name: r.name,
          systemPrompt: r.systemPrompt,
          providerId: r.providerId ?? null,
          modelId: r.modelId ?? null,
          toolAllowlist: r.toolAllowlist,
        });
      }
      await this.deps.db.call("run.create", {
        id,
        workspaceId: input.workspaceId,
        teamId: input.teamId ?? null,
        objective: input.objective,
        branch: worktree.branch,
      });
    } catch (err) {
      // A worktree with no run row would be invisible and never cleaned up.
      await this.deps.db
        .call("repo.removeWorktree", { workspaceId: input.workspaceId, runId: id, abandon: true, force: true })
        .catch(() => undefined);
      throw err;
    }
    const run = new Run(id, input, worktree, this.deps, this.pool);
    this.runs.set(id, run);
    this.deps.emit({ name: "run.started", payload: { runId: id, roles: input.roles.map((r) => r.name), solo: run.solo } });
    run.begin();
    return { runId: id, branch: worktree.branch };
  }
}
