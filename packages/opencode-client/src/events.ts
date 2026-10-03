/**
 * The event union the webview receives from Rust (ticket 023).
 *
 * `payload` is the engine's own generated `Event`, so a contract change in the
 * pinned engine is a type error here rather than a runtime surprise.
 */
import type { components } from "./schema.js";

export type EngineEvent = components["schemas"]["Event"];

/** What Rust decided about an event before the webview saw it. */
export type EventKind = "permission" | "idle" | "other";

export interface SidecarEvent<E extends EngineEvent = EngineEvent> {
  /** Strictly increasing across the subscription, reconnects included. */
  readonly seq: number;
  /** A change means events may have been missed since the last one. */
  readonly epoch: number;
  readonly name: string;
  readonly kind: EventKind;
  readonly envelope: {
    readonly directory?: string;
    readonly project?: string;
    readonly payload: E;
  };
}

/** Synthetic: the stream reconnected and events in between are gone. */
export const STREAM_RESUMED = "sidecar:stream_resumed";

/**
 * The Tauri event name for an engine event type. Mirrors `events::event_name`
 * in Rust — Tauri rejects `.` in event names.
 */
export const sidecarEventName = (type: EngineEvent["type"] | "stream.resumed"): string =>
  `sidecar:${type.replaceAll(".", "_")}`;
