/**
 * The sidecar's view of persistence: named operations answered by Rust.
 *
 * Replies are correlated by id, so they may arrive in any order; Rust serves
 * calls in the order received, so writes sent one after another land in that
 * order. A reply that never comes fails the call after `timeoutMs` rather than
 * hanging the run (ticket 026).
 */
import type { Outbound } from "./protocol.js";

export class DbError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "DbError";
  }
}

interface Pending {
  readonly resolve: (rows: readonly unknown[]) => void;
  readonly reject: (err: Error) => void;
  readonly timer: ReturnType<typeof setTimeout>;
}

export const DEFAULT_TIMEOUT_MS = 10_000;

export class DbClient {
  private readonly pending = new Map<string, Pending>();
  private next = 0;

  constructor(
    private readonly emit: (msg: Outbound) => void,
    private readonly timeoutMs: number = DEFAULT_TIMEOUT_MS,
  ) {}

  /** Number of calls still awaiting a reply. */
  get inFlight(): number {
    return this.pending.size;
  }

  call(op: string, args: Record<string, unknown> = {}): Promise<readonly unknown[]> {
    const id = String(++this.next);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new DbError(`${op} timed out after ${this.timeoutMs}ms`));
      }, this.timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      this.emit({ type: "db.call", id, op, args });
    });
  }

  /** Settle a call. Returns false when nothing was waiting: the caller reports that as a fault. */
  settle(id: string, outcome: { rows: readonly unknown[] } | { error: string }): boolean {
    const p = this.pending.get(id);
    if (!p) return false;
    this.pending.delete(id);
    clearTimeout(p.timer);
    if ("error" in outcome) p.reject(new DbError(outcome.error));
    else p.resolve(outcome.rows);
    return true;
  }
}
