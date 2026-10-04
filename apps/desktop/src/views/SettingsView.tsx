import { useState } from "react";
import { KeyForm } from "../components/KeyForm.js";
import { ModelPicker } from "../components/ModelPicker.js";
import { Banner, Button, Card, Chip, ErrorText, Field, Input, Select, Spinner } from "../components/ui.js";
import { api } from "../lib/api.js";
import { pickFolder } from "../lib/bridge.js";
import { useAction, useLoad } from "../lib/hooks.js";
import { COMMON_PROVIDERS } from "../lib/providers.js";
import type { McpServer } from "../lib/types.js";

function Keys({ workspaceId }: { workspaceId: string }): React.JSX.Element {
  const [adding, setAdding] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const status = useLoad(async () => Object.fromEntries(await Promise.all(COMMON_PROVIDERS.map(async (p) => [p.id, await api.credentials.status(p.id, undefined, workspaceId)] as const))), [workspaceId, tick]);
  const remove = useAction(async (id: string) => {
    await api.credentials.remove({ kind: "global" }, id);
    setTick((t) => t + 1);
  });
  return (
    <section aria-label="Provider keys" className="space-y-2">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">Provider keys</h2>
      <p className="text-sm text-muted">Kept in your system keychain. workmate can use a key but can never show it to you again, so a replaced key is a new key.</p>
      <Card>
        <ul className="divide-y divide-edge">
          {COMMON_PROVIDERS.map((p) => {
            const s = status.data?.[p.id];
            return (
              <li key={p.id} className="px-4 py-2.5">
                <div className="flex items-center gap-3">
                  <span className="flex-1 text-sm font-medium">{p.name}</span>
                  {s?.found ? <Chip tone="good">{s.scope === "global" ? "key saved" : `key saved (${s.scope})`}</Chip> : <Chip>no key</Chip>}
                  <Button small onClick={() => setAdding(adding === p.id ? null : p.id)}>{s?.found ? "Replace" : "Add key"}</Button>
                  {s?.found && s.scope === "global" && <Button small variant="ghost" onClick={() => void remove.run(p.id)}>Remove</Button>}
                </div>
                {adding === p.id && <div className="mt-3"><KeyForm provider={p.id} onSaved={() => { setAdding(null); setTick((t) => t + 1); }} /></div>}
              </li>
            );
          })}
        </ul>
      </Card>
      <ErrorText>{remove.error ?? status.error}</ErrorText>
    </section>
  );
}

function Access({ workspaceId }: { workspaceId: string }): React.JSX.Element {
  const grants = useLoad(() => api.permissions.grants(workspaceId), [workspaceId]);
  const add = useAction(async (op: "read" | "edit") => {
    const dir = await pickFolder();
    if (!dir) return;
    await api.permissions.add(workspaceId, dir, op);
    grants.reload();
  });
  const revoke = useAction(async (id: string) => {
    await api.permissions.revoke(id);
    grants.reload();
  });
  return (
    <section aria-label="Folder access" className="space-y-2">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">Folder access</h2>
      <p className="text-sm text-muted">Runs read this project and write only inside their own branch's folder. Grant access to another folder here; <code className="font-mono">.git</code> is never writable whatever you grant.</p>
      <Card>
        <ul className="divide-y divide-edge">
          {grants.data?.length === 0 && <li className="px-4 py-2.5 text-sm text-muted">No extra folders.</li>}
          {grants.data?.map((g) => (
            <li key={g.id} className="flex items-center gap-3 px-4 py-2.5">
              <Chip tone={g.operation === "edit" ? "warn" : "neutral"}>{g.operation}</Chip>
              <code className="min-w-0 flex-1 truncate font-mono text-xs">{g.path}</code>
              <Button small variant="ghost" onClick={() => void revoke.run(g.id)}>Revoke</Button>
            </li>
          ))}
        </ul>
      </Card>
      <div className="flex gap-2">
        <Button small onClick={() => void add.run("read")}>Allow reading a folder…</Button>
        <Button small onClick={() => void add.run("edit")}>Allow editing a folder…</Button>
      </div>
      <ErrorText>{add.error ?? revoke.error ?? grants.error}</ErrorText>
    </section>
  );
}

function ServerForm({ workspaceId, onAdded }: { workspaceId: string; onAdded: () => void }): React.JSX.Element {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"remote" | "local">("remote");
  const [target, setTarget] = useState("");
  const [everywhere, setEverywhere] = useState(false);
  const add = useAction(async () => {
    const transport = kind === "remote" ? { kind: "remote" as const, url: target.trim() } : { kind: "local" as const, command: target.trim().split(/\s+/) };
    await api.mcp.add(everywhere ? null : workspaceId, name.trim(), transport);
    setName("");
    setTarget("");
    onAdded();
  });
  return (
    <form className="space-y-2 rounded-lg border border-edge p-3" onSubmit={(e) => { e.preventDefault(); void add.run(); }}>
      <div className="grid gap-2 sm:grid-cols-[1fr_8rem]">
        <Field label="Name" hint="Becomes the prefix of its tools: github_create_issue."><Input value={name} onChange={(e) => setName(e.target.value.toLowerCase())} placeholder="github" spellCheck={false} /></Field>
        <Field label="Type"><Select value={kind} onChange={(e) => setKind(e.target.value as "remote" | "local")}><option value="remote">Remote (URL)</option><option value="local">Local (command)</option></Select></Field>
      </div>
      <Field label={kind === "remote" ? "URL" : "Command"} hint={kind === "local" ? "This command runs on your computer, with your permissions." : undefined}>
        <Input className="font-mono" value={target} onChange={(e) => setTarget(e.target.value)} placeholder={kind === "remote" ? "https://example.com/mcp" : "npx -y @scope/some-mcp-server"} spellCheck={false} />
      </Field>
      <label className="flex items-center gap-1.5 text-xs text-muted"><input type="checkbox" checked={everywhere} onChange={(e) => setEverywhere(e.target.checked)} />Available in every project</label>
      <ErrorText>{add.error}</ErrorText>
      <Button type="submit" variant="primary" disabled={add.pending || !name.trim() || !target.trim()}>Add server</Button>
    </form>
  );
}

function Mcp({ workspaceId }: { workspaceId: string }): React.JSX.Element {
  const servers = useLoad(() => api.mcp.list(), []);
  const act = useAction(async (fn: () => Promise<unknown>) => {
    await fn();
    servers.reload();
  });
  const mine = (servers.data ?? []).filter((s: McpServer) => s.workspaceId === null || s.workspaceId === workspaceId);
  return (
    <section aria-label="MCP servers" className="space-y-2">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">MCP servers</h2>
      <p className="text-sm text-muted">Extra tools for agents. Every tool a server provides asks before it runs, and a role only sees the servers its tool list names.</p>
      {servers.loading && !servers.data && <Spinner />}
      {mine.length > 0 && (
        <Card>
          <ul className="divide-y divide-edge">
            {mine.map((s) => (
              <li key={s.id} className="flex items-center gap-3 px-4 py-2.5">
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium">{s.name} {s.workspaceId === null && <Chip>every project</Chip>}</p>
                  <p className="truncate font-mono text-xs text-muted">{s.kind === "remote" ? s.url : s.command.join(" ")}</p>
                </div>
                <label className="flex items-center gap-1.5 text-xs text-muted"><input type="checkbox" checked={s.enabled} onChange={(e) => void act.run(() => api.mcp.setEnabled(s.id, e.target.checked))} />on</label>
                <Button small variant="ghost" onClick={() => void act.run(() => api.mcp.remove(s.id))}>Remove</Button>
              </li>
            ))}
          </ul>
        </Card>
      )}
      <ErrorText>{act.error ?? servers.error}</ErrorText>
      <details className="text-sm"><summary className="cursor-pointer font-medium">Add a server</summary><div className="mt-2"><ServerForm workspaceId={workspaceId} onAdded={servers.reload} /></div></details>
    </section>
  );
}

export function SettingsView({ workspaceId }: { workspaceId: string }): React.JSX.Element {
  const [tick, setTick] = useState(0);
  const model = useLoad(() => api.models.getDefault(), [tick]);
  const info = useLoad(() => api.engineInfo(), []);
  const reveal = useAction(async () => api.revealLogs());
  return (
    <div className="mx-auto max-w-3xl space-y-8 p-5">
      <section aria-label="Default model" className="space-y-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">Default model</h2>
        <p className="text-sm text-muted">Used by any role that doesn't name its own. {model.data?.provider ? <>Currently <strong>{model.data.provider}</strong> / <strong>{model.data.model}</strong>.</> : "None set yet."}</p>
        <ModelPicker onSaved={() => setTick((t) => t + 1)} />
      </section>
      <Keys workspaceId={workspaceId} />
      <Access workspaceId={workspaceId} />
      <Mcp workspaceId={workspaceId} />
      <section aria-label="Diagnostics" className="space-y-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">Diagnostics</h2>
        <p className="text-sm text-muted">workmate sends nothing about you or your work anywhere. When something goes wrong, its logs — and the engine's — are kept on this computer.</p>
        <Button small onClick={() => void reveal.run()}>Show logs</Button>
        <ErrorText>{reveal.error}</ErrorText>
      </section>
      <section aria-label="About" className="space-y-1 text-xs text-muted">
        {info.data && <p>workmate runs OpenCode {info.data.pinnedVersion}, bundled with the app{info.data.sidecarPresent ? "" : " (engine missing from this build)"}.</p>}
        {info.data && !info.data.sidecarPresent && <Banner tone="danger">The bundled engine is missing, so nothing can run. Reinstall workmate.</Banner>}
      </section>
    </div>
  );
}
