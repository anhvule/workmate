/**
 * The real {@link EnginePort}: the pinned engine's HTTP API, narrowed to what
 * orchestration needs. Transcripts stay the engine's (ticket 010) — this only
 * reads them to compose a handoff, and never copies them anywhere.
 */
import type { OpenCodeClient } from "@workmate/opencode-client";
import type { Message } from "./handoff.js";
import type { EnginePort } from "./orchestrator.js";

interface Part {
  readonly type: string;
  readonly text?: string;
}

const textOf = (parts: readonly Part[] | undefined): string =>
  (parts ?? [])
    .filter((p) => p.type === "text" && typeof p.text === "string")
    .map((p) => p.text as string)
    .join("\n")
    .trim();

export const messagesFrom = (raw: readonly unknown[]): readonly Message[] =>
  raw.flatMap((m) => {
    const { info, parts } = m as { info?: { role?: string; time?: { created?: number } }; parts?: Part[] };
    const role = info?.role;
    const text = textOf(parts);
    if ((role !== "user" && role !== "assistant") || text === "") return [];
    return [{ role, text, at: info?.time?.created ?? 0 }];
  });

export const engineAdapter = (
  client: OpenCodeClient,
  /** How to register an MCP server with the engine for a directory. */
  addMcp: (directory: string, name: string, url: string, token: string) => Promise<void>,
  mcpToken: string,
): EnginePort => ({
  createSession: (directory, body) => client.createSession(directory, body as never),
  sendMessage: async (sessionId, directory, body) => {
    const res = (await client.sendMessage(sessionId, directory, {
      parts: [{ type: "text", text: body.prompt }],
      ...(body.system ? { system: body.system } : {}),
      ...(body.model ? { model: body.model } : {}),
      ...(body.tools ? { tools: body.tools } : {}),
    } as never)) as { parts?: Part[] };
    return { text: textOf(res.parts) };
  },
  listMessages: async (sessionId, directory) => messagesFrom(await client.listMessages(sessionId, directory)),
  registerMemoryTools: (directory, url) => addMcp(directory, "workmate-memory", url, mcpToken),
});
