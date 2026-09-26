import type { RoleId, WorkspaceId } from "./ids.js";

/**
 * A scope is the axis along which both memories and credentials are resolved.
 *
 * Specificity is total and ordered: a role is narrower than a workspace, which
 * is narrower than global. Resolution always walks narrowest-first, so a role
 * can override its workspace without the workspace knowing the role exists.
 */
export type Scope =
  | { readonly kind: "global" }
  | { readonly kind: "workspace"; readonly workspaceId: WorkspaceId }
  | { readonly kind: "role"; readonly roleId: RoleId };

export const GLOBAL: Scope = { kind: "global" };
export const forWorkspace = (id: WorkspaceId): Scope => ({ kind: "workspace", workspaceId: id });
export const forRole = (id: RoleId): Scope => ({ kind: "role", roleId: id });

/** Higher is more specific. Used to break ties in every resolver. */
export const specificity = (s: Scope): number =>
  s.kind === "role" ? 2 : s.kind === "workspace" ? 1 : 0;

export const scopeKey = (s: Scope): string => {
  switch (s.kind) {
    case "global":
      return "global";
    case "workspace":
      return `workspace:${s.workspaceId}`;
    case "role":
      return `role:${s.roleId}`;
  }
};

/**
 * The scopes to consult for a run, narrowest first.
 *
 * This is the "role -> workspace -> global" order fixed in ticket 009. Only
 * memory resolves through it here: credentials resolve in Rust, which owns the
 * keychain outright, and a second implementation of that format in TypeScript
 * would be a drift waiting to happen (ticket 017).
 */
export const resolutionOrder = (opts: {
  role?: RoleId | undefined;
  workspace?: WorkspaceId | undefined;
}): Scope[] => {
  const out: Scope[] = [];
  if (opts.role !== undefined) out.push(forRole(opts.role));
  if (opts.workspace !== undefined) out.push(forWorkspace(opts.workspace));
  out.push(GLOBAL);
  return out;
};
