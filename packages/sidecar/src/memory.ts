/**
 * Memory as the agent and the turn loop see it (ticket 015).
 *
 * Three jobs, all thin because the rules live elsewhere: the pinned digest for
 * the per-turn `system` field is built by the core domain; storage, supersession
 * and the no-secrets guard are Rust's, behind named operations. The sidecar only
 * decides *which scopes* apply to a turn and shapes the tool results.
 */
import {
  buildDigest,
  renderDigest,
  resolutionOrder,
  type Memory,
  type RoleId,
  type RunId,
  type Scope,
  type WorkspaceId,
} from "@workmate/core";
import type { DbClient } from "./db.js";

/** Who is calling: carried in the MCP path so a tool call is attributable to a turn. */
export interface MemoryContext {
  readonly runId: RunId;
  readonly workspaceId: WorkspaceId;
  readonly roleId: RoleId;
}

const scopesOf = (ctx: MemoryContext): readonly Scope[] =>
  resolutionOrder({ role: ctx.roleId, workspace: ctx.workspaceId });

const asMemory = (row: unknown): Memory => {
  const r = row as Record<string, unknown>;
  return { ...(r as unknown as Memory), recordedByRun: (r["recordedByRun"] ?? "") as RunId };
};

export class MemoryService {
  constructor(private readonly db: DbClient) {}

  /**
   * The text to put in this turn's `system` field. The engine persists it on
   * the user message, which is what keeps "what memory told the agent"
   * auditable afterwards.
   */
  async digestFor(ctx: MemoryContext): Promise<string> {
    const rows = await this.db.call("memory.list", { scopes: scopesOf(ctx) });
    return renderDigest(buildDigest(rows.map(asMemory), scopesOf(ctx)));
  }

  /** The `remember` tool. Resolves to text for the agent, never throws. */
  async remember(
    ctx: MemoryContext,
    args: { subject?: unknown; claim?: unknown; scope?: unknown; pinned?: unknown },
  ): Promise<{ text: string; isError: boolean }> {
    // The agent picks how widely a fact applies; the default is the workspace,
    // because most facts are about one project and "global" is a claim about
    // the user, which deserves to be a deliberate choice.
    const scope: Scope =
      args.scope === "global"
        ? { kind: "global" }
        : args.scope === "role"
          ? { kind: "role", roleId: ctx.roleId }
          : { kind: "workspace", workspaceId: ctx.workspaceId };
    try {
      const rows = await this.db.call("memory.remember", {
        subject: args.subject,
        claim: args.claim,
        scopes: [scope],
        pinned: args.pinned === true,
        runId: ctx.runId,
      });
      const replaced = ((rows[0] as { superseded?: string[] } | undefined)?.superseded ?? []).length;
      return {
        text: replaced > 0 ? `Remembered. This replaced ${replaced} earlier claim(s) on the same subject.` : "Remembered.",
        isError: false,
      };
    } catch (err) {
      return { text: err instanceof Error ? err.message : String(err), isError: true };
    }
  }

  /** The `recall` tool. */
  async recall(
    ctx: MemoryContext,
    args: { query?: unknown; limit?: unknown },
  ): Promise<{ text: string; isError: boolean }> {
    try {
      const rows = await this.db.call("memory.recall", {
        query: typeof args.query === "string" ? args.query : "",
        limit: typeof args.limit === "number" ? args.limit : 10,
        scopes: scopesOf(ctx),
      });
      if (rows.length === 0) return { text: "Nothing remembered matches that.", isError: false };
      return {
        text: rows
          .map((r) => asMemory(r))
          .map((m) => `- ${m.subject}: ${m.claim}`)
          .join("\n"),
        isError: false,
      };
    } catch (err) {
      return { text: err instanceof Error ? err.message : String(err), isError: true };
    }
  }
}
