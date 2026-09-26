import type { Memory } from "@workmate/core";
import { buildDigest } from "@workmate/core";
import type { Scope } from "@workmate/core";

/**
 * Composing what one role hands to the next.
 *
 * This is the agent-team differentiator's mechanism. OpenCode's own delegation
 * passes exactly one string — the child's last text part — and carries none of
 * the originating session's context (ticket 001). So workmate reads the prior
 * session's messages and composes the next role's input itself.
 *
 * The composition is deliberately explicit rather than "send everything":
 * a handoff the user can read, audit and amend is the feature. A blind context
 * dump is both unreadable and unboundedly expensive.
 */

export interface Message {
  readonly role: "user" | "assistant";
  readonly text: string;
  readonly at: number;
}

export interface HandoffRequest {
  /** Messages from the session being handed off *from*, oldest first. */
  readonly transcript: readonly Message[];
  /** The receiving role's name, used to address the brief. */
  readonly toRole: string;
  /** The run's original objective, restated so the receiver is not guessing. */
  readonly objective: string;
  /** Pinned memories in scope; injected per-turn via OpenCode's `system` field. */
  readonly memories: readonly Memory[];
  readonly scopes: readonly Scope[];
}

export interface Handoff {
  /** The prompt body sent as the receiving session's first user message. */
  readonly prompt: string;
  /** Sent as the per-turn `system` string, so what memory said stays auditable. */
  readonly system: string;
  /** Shown on the handoff card, collapsed by default. */
  readonly carried: readonly Message[];
}

/** How many of the prior session's assistant turns are carried forward. */
export const CARRIED_TURNS = 3;

const lastAssistantTurns = (
  transcript: readonly Message[],
  n: number,
): readonly Message[] =>
  transcript.filter((m) => m.role === "assistant").slice(-n);

/**
 * Build the brief for the receiving role.
 *
 * Throws on an empty handoff rather than sending a role an empty brief: a role
 * that receives nothing will invent the missing context, which is worse than
 * failing loudly.
 */
export const composeHandoff = (req: HandoffRequest): Handoff => {
  const carried = lastAssistantTurns(req.transcript, CARRIED_TURNS);
  if (carried.length === 0) {
    throw new Error(
      `cannot hand off to ${req.toRole}: the previous session produced no assistant output`,
    );
  }

  const prompt = [
    `You are the ${req.toRole} on this run.`,
    ``,
    `## Objective`,
    req.objective,
    ``,
    `## What the previous role concluded`,
    ...carried.map((m) => `- ${m.text.trim()}`),
    ``,
    `Continue from there. Do not repeat work already done above.`,
  ].join("\n");

  const digest = buildDigest(req.memories, req.scopes);
  const system =
    digest.length === 0
      ? ""
      : [
          `What workmate remembers about this project:`,
          ...digest.map((m) => `- ${m.subject}: ${m.claim}`),
        ].join("\n");

  return { prompt, system, carried };
};
