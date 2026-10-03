import { Button, ErrorText, Modal, RoleChip } from "../components/ui.js";
import { api } from "../lib/api.js";
import { useAction } from "../lib/hooks.js";
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
 * "always" for shell and MCP tools does not exist by construction (ticket 013);
 * folder access is granted in Settings, where it can be seen and revoked.
 */
export function PermissionModal(): React.JSX.Element | null {
  const prompt = useLive((s) => s.prompts[0]);
  const owner = useLive((s) => (s.prompts[0] ? s.sessions[s.prompts[0].sessionId] : undefined));
  const more = useLive((s) => Math.max(0, s.prompts.length - 1));
  const reply = useAction(async (r: "once" | "reject") => {
    if (prompt) await api.runs.replyPermission(prompt.sessionId, prompt.id, r);
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
        <ErrorText>{reply.error}</ErrorText>
        <div className="flex items-center gap-2">
          {more > 0 && <span className="text-xs text-muted">{more} more waiting</span>}
          <Button className="ml-auto" onClick={() => void reply.run("reject")} disabled={reply.pending}>Deny</Button>
          <Button variant="primary" onClick={() => void reply.run("once")} disabled={reply.pending}>Allow once</Button>
        </div>
      </div>
    </Modal>
  );
}
