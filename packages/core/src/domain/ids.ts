/**
 * Branded identifiers.
 *
 * Every entity in the domain model is addressed by an opaque id, never by a
 * path or a name: a workspace whose folder moves must be repaired, not
 * orphaned (see ticket 003).
 */

declare const brand: unique symbol;
type Brand<T, B extends string> = T & { readonly [brand]: B };

export type WorkspaceId = Brand<string, "WorkspaceId">;
export type RunId = Brand<string, "RunId">;
export type RoleId = Brand<string, "RoleId">;
export type TeamId = Brand<string, "TeamId">;
export type SessionId = Brand<string, "SessionId">;
export type MemoryId = Brand<string, "MemoryId">;
export type ProviderId = Brand<string, "ProviderId">;

export const workspaceId = (v: string): WorkspaceId => v as WorkspaceId;
export const runId = (v: string): RunId => v as RunId;
export const roleId = (v: string): RoleId => v as RoleId;
export const teamId = (v: string): TeamId => v as TeamId;
export const sessionId = (v: string): SessionId => v as SessionId;
export const memoryId = (v: string): MemoryId => v as MemoryId;
export const providerId = (v: string): ProviderId => v as ProviderId;
