import { useState } from "react";
import { Banner, Button, Card, Chip, Empty, ErrorText, Field, Input, Modal, RoleChip, Spinner, Textarea } from "../components/ui.js";
import { api } from "../lib/api.js";
import { useAction, useLoad } from "../lib/hooks.js";
import type { Pack, Role, SkillEntry, Team } from "../lib/types.js";

function PackCard({ pack, workspaceId, onApplied }: { pack: Pack; workspaceId: string; onApplied: () => void }): React.JSX.Element {
  const [open, setOpen] = useState(false);
  const preview = useLoad(() => (open ? api.packs.preview(pack.id, workspaceId) : Promise.resolve(undefined)), [open, pack.id, workspaceId]);
  const [done, setDone] = useState<string | null>(null);
  const apply = useAction(async () => {
    const out = await api.packs.apply(pack.id, workspaceId);
    setDone(`Created the "${out.team.name}" team${out.written.length ? ` and wrote ${out.written.join(", ")}` : ""}${out.skipped.length ? `; left ${out.skipped.join(", ")} alone because it already exists` : ""}.`);
    setOpen(false);
    onApplied();
  });
  const solo = pack.team.roles.length === 1;
  return (
    <Card className="p-4">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <p className="font-medium">{pack.name}</p>
          <p className="mt-0.5 text-sm text-muted">{pack.description}</p>
          <p className="mt-2 flex flex-wrap gap-1.5">{solo ? <Chip>single assistant</Chip> : pack.team.roles.map((r) => <RoleChip key={r.id} name={r.name} />)}</p>
        </div>
        <Button onClick={() => setOpen(true)}>Use…</Button>
      </div>
      {done && <p className="mt-2 text-sm text-good">{done}</p>}
      <Modal open={open} onOpenChange={setOpen} title={`Use "${pack.name}"`} description="This creates the team below in workmate and may add files to your project.">
        <div className="space-y-3 text-sm">
          <div>
            <p className="mb-1 text-xs font-medium text-muted">Team</p>
            <ul className="space-y-1">{pack.team.roles.map((r) => <li key={r.id}><strong>{r.name}</strong> <span className="text-muted">— {r.toolAllowlist.length ? `can use ${r.toolAllowlist.join(", ")}` : "all tools"}</span></li>)}</ul>
          </div>
          <div>
            <p className="mb-1 text-xs font-medium text-muted">Files</p>
            {preview.loading && <Spinner />}
            {preview.data && preview.data.willWrite.length === 0 && preview.data.willSkip.length === 0 && <p className="text-muted">None.</p>}
            <ul className="font-mono text-xs">
              {preview.data?.willWrite.map((f) => <li key={f}>+ {f} <span className="text-muted">(new)</span></li>)}
              {preview.data?.willSkip.map((f) => <li key={f} className="text-muted">· {f} (already there; left untouched)</li>)}
            </ul>
          </div>
          <ErrorText>{apply.error ?? preview.error}</ErrorText>
          <div className="flex justify-end gap-2">
            <Button onClick={() => setOpen(false)}>Cancel</Button>
            <Button variant="primary" disabled={apply.pending} onClick={() => void apply.run()}>Create team</Button>
          </div>
        </div>
      </Modal>
    </Card>
  );
}

function RoleEditor({ role, onClose, onSaved }: { role: Role; onClose: () => void; onSaved: () => void }): React.JSX.Element {
  const [prompt, setPrompt] = useState(role.systemPrompt);
  const [provider, setProvider] = useState(role.providerId ?? "");
  const [model, setModel] = useState(role.modelId ?? "");
  const [tools, setTools] = useState(role.toolAllowlist.join(", "));
  const save = useAction(async () => {
    if (!!provider !== !!model) throw new Error("Set both a provider and a model, or neither to use your default.");
    await api.teams.updateRole({
      ...role,
      systemPrompt: prompt,
      providerId: provider.trim() || null,
      modelId: model.trim() || null,
      toolAllowlist: tools.split(",").map((t) => t.trim()).filter(Boolean),
    });
    onSaved();
    onClose();
  });
  return (
    <Modal open onOpenChange={(o) => !o && onClose()} title={`Edit ${role.name}`} description="A role is its prompt, its model and the tools it may use." wide>
      <form className="space-y-3" onSubmit={(e) => { e.preventDefault(); void save.run(); }}>
        <Field label="System prompt"><Textarea rows={7} value={prompt} onChange={(e) => setPrompt(e.target.value)} /></Field>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label="Provider" hint="Leave both blank to use your default model."><Input value={provider} onChange={(e) => setProvider(e.target.value)} placeholder="anthropic" /></Field>
          <Field label="Model"><Input value={model} onChange={(e) => setModel(e.target.value)} placeholder="model id" /></Field>
        </div>
        <Field label="Allowed tools" hint="Comma-separated. Blank means all of them. With a list, everything else is off: for example read, grep, glob, list, or an MCP server's name."><Input className="font-mono" value={tools} onChange={(e) => setTools(e.target.value)} placeholder="read, grep, glob" /></Field>
        <ErrorText>{save.error}</ErrorText>
        <div className="flex justify-end gap-2"><Button type="button" onClick={onClose}>Cancel</Button><Button type="submit" variant="primary" disabled={save.pending}>Save</Button></div>
      </form>
    </Modal>
  );
}

function Teams(): React.JSX.Element {
  const teams = useLoad(() => api.teams.list(), []);
  const [editing, setEditing] = useState<Role | null>(null);
  return (
    <section aria-label="Your teams" className="space-y-2">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">Your teams</h2>
      {teams.loading && !teams.data && <Spinner />}
      {teams.data?.length === 0 && <p className="text-sm text-muted">None yet. Use a starter above.</p>}
      {teams.data?.map((t: Team) => (
        <Card key={t.id} className="p-3">
          <p className="text-sm font-medium">{t.name}</p>
          <div className="mt-1.5 flex flex-wrap gap-2">
            {t.roles.map((r) => (
              <button key={r.id} type="button" onClick={() => setEditing(r)} className="rounded-full hover:ring-2 hover:ring-accent/40" aria-label={`Edit ${r.name}`}>
                <RoleChip name={r.name} model={r.modelId} />
              </button>
            ))}
          </div>
        </Card>
      ))}
      {editing && <RoleEditor role={editing} onClose={() => setEditing(null)} onSaved={teams.reload} />}
    </section>
  );
}

function Skills({ workspaceId }: { workspaceId: string }): React.JSX.Element {
  const catalog = useLoad(() => api.skills.catalog(), []);
  const installed = useLoad(() => api.skills.installed(workspaceId), [workspaceId]);
  const sources = useLoad(() => api.skills.sources(), []);
  const [url, setUrl] = useState("");
  const act = useAction(async (fn: () => Promise<unknown>) => {
    await fn();
    installed.reload();
    catalog.reload();
    sources.reload();
  });
  const add = useAction(async () => {
    const s = await api.skills.addSource(url);
    setUrl("");
    await api.skills.sync(s.id);
    sources.reload();
    catalog.reload();
  });
  const isIn = (e: SkillEntry) => installed.data?.find((i) => i.name === e.name);
  return (
    <section aria-label="Skills" className="space-y-2">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">Skills</h2>
      <p className="text-sm text-muted">Instructions an agent loads when they apply. Installing one adds a folder under <code className="font-mono">.opencode/skills</code> in your project.</p>
      {catalog.data?.length === 0 && <Empty title="No skills available" />}
      <Card>
        <ul className="divide-y divide-edge">
          {catalog.data?.map((e) => {
            const have = isIn(e);
            return (
              <li key={`${e.origin}/${e.name}`} className="flex items-center gap-3 px-4 py-2.5">
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium">{e.name} {have?.modified && <Chip tone="warn">edited here</Chip>}</p>
                  <p className="truncate text-xs text-muted">{e.description} · <span title={e.origin}>{e.origin === "workmate" ? "from workmate" : e.origin}</span></p>
                </div>
                {have ? (
                  <Button small onClick={() => void act.run(() => api.skills.uninstall(workspaceId, e.name, false))}>Remove</Button>
                ) : (
                  <Button small variant="primary" onClick={() => void act.run(() => api.skills.install(workspaceId, e.name, e.origin))}>Install</Button>
                )}
              </li>
            );
          })}
        </ul>
      </Card>
      <ErrorText>{act.error}</ErrorText>
      <details className="rounded-lg border border-edge p-3 text-sm">
        <summary className="cursor-pointer font-medium">Add a skill source</summary>
        <div className="mt-2 space-y-2">
          <Banner tone="warn">A skill is instructions an agent will follow. Only add sources you trust.</Banner>
          <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); void add.run(); }}>
            <Input aria-label="Git URL" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://github.com/you/skills.git" />
            <Button type="submit" disabled={add.pending || !url.trim()}>{add.pending ? "Syncing…" : "Add and sync"}</Button>
          </form>
          <ErrorText>{add.error}</ErrorText>
          {sources.data?.map((s) => (
            <p key={s.id} className="flex items-center gap-2 text-xs"><span className="min-w-0 flex-1 truncate font-mono">{s.url}</span><Button small variant="ghost" onClick={() => void act.run(() => api.skills.sync(s.id))}>Sync</Button><Button small variant="ghost" onClick={() => void act.run(() => api.skills.removeSource(s.id))}>Remove</Button></p>
          ))}
        </div>
      </details>
    </section>
  );
}

export function LibraryView({ workspaceId }: { workspaceId: string }): React.JSX.Element {
  const packs = useLoad(() => api.packs.list(), []);
  const [bump, setBump] = useState(0);
  return (
    <div className="mx-auto max-w-3xl space-y-8 p-5">
      <section aria-label="Starters" className="space-y-2">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">Starters</h2>
        {packs.loading && !packs.data && <Spinner />}
        <ErrorText>{packs.error}</ErrorText>
        {packs.data?.map((p) => <PackCard key={p.id} pack={p} workspaceId={workspaceId} onApplied={() => setBump((b) => b + 1)} />)}
      </section>
      <Teams key={bump} />
      <Skills workspaceId={workspaceId} />
    </div>
  );
}
