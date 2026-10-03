/**
 * The commands the webview can send, relayed through Rust.
 *
 * Every command resolves to a JSON value or throws a message; `main` turns that
 * into a `cmd.result`. Nothing here persists: durable effects are named
 * operations, so the webview never writes anything itself (ticket 010).
 */
import type { RoleId, WorkspaceId } from "@workmate/core";
import type { DbClient } from "./db.js";
import type { Orchestrator, RoleSpec } from "./orchestrator.js";
import type { EnginePort } from "./orchestrator.js";

export type Command = (args: Record<string, unknown>) => Promise<unknown>;

const str = (a: Record<string, unknown>, k: string): string => {
  const v = a[k];
  if (typeof v !== "string" || v === "") throw new Error(`missing argument \`${k}\``);
  return v;
};

const parseRoles = (raw: unknown): RoleSpec[] => {
  if (!Array.isArray(raw)) throw new Error("missing argument `roles`");
  return raw.map((r) => {
    const o = r as Record<string, unknown>;
    return {
      id: str(o, "id") as RoleId,
      name: str(o, "name"),
      systemPrompt: typeof o["systemPrompt"] === "string" ? o["systemPrompt"] : "",
      providerId: typeof o["providerId"] === "string" ? o["providerId"] : undefined,
      modelId: typeof o["modelId"] === "string" ? o["modelId"] : undefined,
      toolAllowlist: Array.isArray(o["toolAllowlist"]) ? o["toolAllowlist"].map(String) : [],
    };
  });
};

export const commands = (deps: { orch: Orchestrator; db: DbClient; engine: EnginePort }): Record<string, Command> => {
  const { orch, db, engine } = deps;
  const ws = (a: Record<string, unknown>): { workspaceId: string; runId: string } => ({
    workspaceId: str(a, "workspaceId"),
    runId: str(a, "runId"),
  });
  return {
    "run.start": (a) =>
      orch.start({
        workspaceId: str(a, "workspaceId") as WorkspaceId,
        objective: str(a, "objective"),
        roles: parseRoles(a["roles"]),
        teamId: typeof a["teamId"] === "string" ? a["teamId"] : undefined,
        review: a["review"] === true,
      }),
    "run.pause": async (a) => void orch.get(str(a, "runId")).pause(),
    "run.resume": async (a) => void orch.get(str(a, "runId")).resume(),
    "run.amend": async (a) => void orch.get(str(a, "runId")).amend(str(a, "prompt")),
    "run.redirect": async (a) => void orch.get(str(a, "runId")).redirect(str(a, "roleId") as RoleId),
    "run.veto": async (a) => void orch.get(str(a, "runId")).veto(),
    // The persisted run, ordered by Rust; the UI feeds it to `runTimeline`.
    "run.get": async (a) => {
      const rows = await db.call("run.load", { id: str(a, "runId") });
      if (rows.length === 0) throw new Error("no such run");
      return rows[0];
    },
    "run.transcript": (a) => engine.listMessages(str(a, "sessionId"), str(a, "directory")),
    "run.status": async (a) => (await db.call("repo.status", ws(a)))[0],
    "run.diff": async (a) => (await db.call("repo.diff", { ...ws(a), base: str(a, "base") }))[0],
    // Merging is only ever the user's explicit action.
    "run.merge": async (a) => {
      const out = (await db.call("repo.merge", ws(a)))[0];
      return out;
    },
    // Keep the branch, drop the directory. Refuses uncommitted work.
    "run.archive": async (a) => {
      await db.call("repo.removeWorktree", { ...ws(a), abandon: false, force: false });
      await db.call("run.setState", { id: str(a, "runId"), state: "archived" });
    },
    // Throw the work away. `force` is the only way to discard uncommitted changes.
    "run.abandon": async (a) => {
      await db.call("repo.removeWorktree", { ...ws(a), abandon: true, force: a["force"] === true });
      await db.call("run.setState", { id: str(a, "runId"), state: "archived" });
    },
  };
};
