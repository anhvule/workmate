import { Button, ErrorText, Modal, RoleChip } from "../components/ui.js";
import { api } from "../lib/api.js";
import { useAction, useLoad } from "../lib/hooks.js";
import { useLive } from "../lib/live.js";

const describe = (permission: string): string => {
  if (permission === "bash") return "run a shell command";
  if (permission === "edit") return "edit files";
  if (permission === "read") return "read files";
  if (permission === "webfetch") return "fetch a web page";
  return `use ${permission.replace(/_\*$/, "")} (an MCP tool)`;
};

/**
 * The engine is asking. It names the role that is asking — in a team, "the
 * agent" is not enough — and it can only be answered once or refused. A durable
 * "always" is a stored, screened allowance for this project (ticket 028): a
 * command prefix or an MCP server, refused if it names something dangerous, and
 * listed in Settings where it can be revoked. Push asks whatever is stored.
 */
export function PermissionModal(): React.JSX.Element | null {
  const prompt = useLive((s) => s.prompts[0]);
  const owner = useLive((s) => (s.prompts[0] ? s.sessions[s.prompts[0].sessionId] : undefined));
  const more = useLive((s) => Math.max(0, s.prompts.length - 1));
  const reply = useAction(async (r: "once" | "reject") => {
    if (prompt) await api.runs.replyPermission(prompt.sessionId, prompt.id, r);
  });
  const servers = useLoad(() => api.mcp.list(), [prompt?.id]);
  // What "always" would store: the command the engine suggests (minus its
  // trailing ` *`), or the MCP server a tool belongs to. Rust screens it.
  const durable: { kind: "bash" | "mcp"; value: string } | undefined = !prompt
    ? undefined
    : prompt.permission === "bash"
      ? (() => {
          const v = (prompt.always[0] ?? prompt.patterns[0] ?? "").replace(/ \*$/, "").trim();
          return v ? { kind: "bash" as const, value: v } : undefined;
        })()
      : (() => {
          const s = servers.data?.find((m) => prompt.permission.startsWith(`${m.name}_`));
          return s ? { kind: "mcp" as const, value: s.name } : undefined;
        })();
  const always = useAction(async () => {
    if (!prompt || !durable || !owner) return;
    await api.allowances.add(owner.workspaceId, durable.kind, durable.value);
    await api.runs.replyPermission(prompt.sessionId, prompt.id, "always");
  });
  if (!prompt) return null;
  return (
    <Modal open onOpenChange={() => undefined} title="Permission needed">
      <div className="space-y-3 text-sm">
        <p className="flex flex-wrap items-center gap-2">
          {owner ? <RoleChip name={owner.role} /> : <strong>An agent</strong>} wants to {describe(prompt.permission)}:
        </p>
        <ul className="max-h-40 space-y-1 overflow-auto rounded-md bg-panel p-2 font-mono text-xs">
          {prompt.patterns.length === 0 ? <li>(no detail given)</li> : prompt.patterns.map((p) => <li key={p} className="break-all">{p}</li>)}
        </ul>
        <ErrorText>{reply.error ?? always.error}</ErrorText>
        <div className="flex flex-wrap items-center gap-2">
          {more > 0 && <span className="text-xs text-muted">{more} more waiting</span>}
          {durable && owner && (
            <Button
              variant="ghost"
              disabled={always.pending}
              onClick={() => void always.run()}
              title="Stored for this project only. Review or revoke it in Settings."
            >
              {durable.kind === "bash" ? <>Always allow <code className="font-mono">{durable.value}</code> here</> : <>Always allow {durable.value} tools here</>}
            </Button>
          )}
          <Button className="ml-auto" onClick={() => void reply.run("reject")} disabled={reply.pending}>Deny</Button>
          <Button variant="primary" onClick={() => void reply.run("once")} disabled={reply.pending}>Allow once</Button>
        </div>
      </div>
    </Modal>
  );
}
