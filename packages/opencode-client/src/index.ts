import type { paths } from "./schema.js";

/**
 * A typed client for the endpoints workmate actually uses.
 *
 * Deliberately narrow: the engine publishes 162 paths, and wrapping all of them
 * would be a second API to maintain. Everything here is checked against the
 * generated schema, so a contract change at upgrade time is a type error rather
 * than a runtime surprise.
 */

type Json<T> = T extends { content: { "application/json": infer B } } ? B : never;

type CreateSessionBody = Json<
  NonNullable<paths["/session"]["post"]["requestBody"]>
>;
type SendMessageBody = Json<
  NonNullable<paths["/session/{sessionID}/message"]["post"]["requestBody"]>
>;

export interface EngineAddress {
  readonly baseUrl: string;
  /** Set when the engine was started with `OPENCODE_SERVER_PASSWORD`. */
  readonly password?: string | undefined;
}

export class EngineError extends Error {
  constructor(
    readonly status: number,
    readonly path: string,
    body: string,
  ) {
    super(`engine ${status} on ${path}: ${body.slice(0, 200)}`);
    this.name = "EngineError";
  }
}

export class OpenCodeClient {
  constructor(private readonly address: EngineAddress) {}

  private headers(): Record<string, string> {
    const h: Record<string, string> = { "content-type": "application/json" };
    if (this.address.password !== undefined) {
      // The engine is unsecured without this; workmate always sets it.
      h["authorization"] = `Basic ${btoa(`opencode:${this.address.password}`)}`;
    }
    return h;
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await fetch(`${this.address.baseUrl}${path}`, {
      ...init,
      headers: { ...this.headers(), ...(init?.headers ?? {}) },
    });
    if (!res.ok) throw new EngineError(res.status, path, await res.text());
    return (await res.json()) as T;
  }

  /** Liveness. Readiness during startup is the announced URL, not this. */
  async health(): Promise<boolean> {
    try {
      const res = await fetch(`${this.address.baseUrl}/global/health`, {
        headers: this.headers(),
      });
      return res.ok;
    } catch {
      return false;
    }
  }

  /**
   * Create a session pinned to one directory.
   *
   * `directory` is the run's own git worktree — workmate creates worktrees
   * itself and addresses them through this stable query parameter, rather than
   * depending on the engine's `experimental_` workspace adapter (ticket 007).
   */
  createSession(directory: string, body: CreateSessionBody): Promise<{ id: string }> {
    const q = new URLSearchParams({ directory });
    return this.request(`/session?${q.toString()}`, {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /**
   * Send a turn.
   *
   * `system` is the per-turn memory digest. The engine appends it to the system
   * prompt for this turn only and persists it on the user message, which is
   * what keeps workmate's memory auditable after the fact (ticket 006).
   */
  sendMessage(sessionId: string, directory: string, body: SendMessageBody): Promise<unknown> {
    const q = new URLSearchParams({ directory });
    return this.request(`/session/${sessionId}/message?${q.toString()}`, {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Providers and their models, as the engine currently knows them. */
  async providers(): Promise<{ id: string; name: string; models: { id: string; name: string }[] }[]> {
    const raw = await this.request<{ providers?: { id: string; name?: string; models?: Record<string, { id?: string; name?: string }> }[] }>("/config/providers");
    return (raw.providers ?? []).map((p) => ({
      id: p.id,
      name: p.name ?? p.id,
      models: Object.entries(p.models ?? {}).map(([key, m]) => ({ id: m.id ?? key, name: m.name ?? key })),
    }));
  }

  /** The prior session's turns — the raw material `composeHandoff` reads. */
  listMessages(sessionId: string, directory: string): Promise<unknown[]> {
    const q = new URLSearchParams({ directory });
    return this.request(`/session/${sessionId}/message?${q.toString()}`);
  }

  /** Answer a permission request. `once` grants this call only. */
  replyPermission(
    sessionId: string,
    permissionId: string,
    response: "once" | "always" | "reject",
  ): Promise<unknown> {
    return this.request(`/session/${sessionId}/permissions/${permissionId}`, {
      method: "POST",
      body: JSON.stringify({ response }),
    });
  }

  /** Working-tree status, used to render what a run changed. */
  vcsStatus(directory: string): Promise<unknown> {
    const q = new URLSearchParams({ directory });
    return this.request(`/vcs/status?${q.toString()}`);
  }

  /** The engine's event stream. Rust consumes this; the webview never does. */
  eventStreamUrl(): string {
    return `${this.address.baseUrl}/global/event`;
  }
}
export * from "./events.js";
