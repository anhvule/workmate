import { useState } from "react";
import { Banner, Button, Card, Chip, Confirm, Empty, ErrorText, Field, Input, Select, Spinner, Textarea } from "../components/ui.js";
import { api } from "../lib/api.js";
import { ago, inFuture } from "../lib/format.js";
import { useAction, useLoad } from "../lib/hooks.js";
import { useLive } from "../lib/live.js";
import type { Automation, Fire, Team } from "../lib/types.js";

const PRESETS: { label: string; cron: string }[] = [
  { label: "Every day at 9:00", cron: "0 9 * * *" },
  { label: "Every weekday at 9:00", cron: "0 9 * * 1-5" },
  { label: "Every Monday at 9:00", cron: "0 9 * * 1" },
  { label: "Every night at 3:00", cron: "0 3 * * *" },
  { label: "Every hour", cron: "0 * * * *" },
];

const outcomeTone: Record<Fire["outcome"], "good" | "neutral" | "warn" | "danger" | "accent"> = {
  started: "accent", completed: "good", quiet: "neutral", blocked: "warn", failed: "danger", skipped_overlap: "neutral", missed: "warn",
};
const outcomeLabel: Record<Fire["outcome"], string> = {
  started: "running", completed: "found something", quiet: "nothing to report", blocked: "blocked", failed: "failed", skipped_overlap: "skipped (still running)", missed: "missed",
};

function Create({ workspaceId, teams, onCreated }: { workspaceId: string; teams: Team[]; onCreated: () => void }): React.JSX.Element {
  const [name, setName] = useState("");
  const [objective, setObjective] = useState("");
  const [preset, setPreset] = useState(PRESETS[3]!.cron);
  const [custom, setCustom] = useState("");
  const [teamId, setTeamId] = useState(teams[0]?.id ?? "");
  const schedule = preset === "custom" ? custom : preset;
  const create = useAction(async () => {
    const team = teams.find((t) => t.id === teamId);
    if (!team) throw new Error("Pick a team first (see the Library tab).");
    await api.automations.create({ workspaceId, name, schedule, objective, roles: team.roles });
    setName("");
    setObjective("");
    onCreated();
  });
  return (
    <Card className="p-4">
      <form className="space-y-3" onSubmit={(e) => { e.preventDefault(); void create.run(); }}>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label="Name"><Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Nightly dependency check" /></Field>
          <Field label="When">
            <Select value={preset} onChange={(e) => setPreset(e.target.value)}>
              {PRESETS.map((p) => <option key={p.cron} value={p.cron}>{p.label}</option>)}
              <option value="custom">Custom (cron)…</option>
            </Select>
          </Field>
        </div>
        {preset === "custom" && (
          <Field label="Cron expression" hint="minute hour day-of-month month day-of-week, in your local time. For example 30 8 * * 1-5."><Input className="font-mono" value={custom} onChange={(e) => setCustom(e.target.value)} placeholder="0 9 * * *" /></Field>
        )}
        <Field label="What should it do?"><Textarea rows={3} value={objective} onChange={(e) => setObjective(e.target.value)} placeholder="Check for outdated dependencies with known security advisories." /></Field>
        <Field label="Team"><Select value={teamId} onChange={(e) => setTeamId(e.target.value)}>{teams.map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}</Select></Field>
        <Banner tone="accent">
          <p className="font-medium">It runs without you.</p>
          <p className="text-muted">Each run works on its own branch and never touches your checkout. Nobody is there to approve things, so anything that would ask is refused. If it finds nothing it stays silent; if it finds something, it lands in Findings.</p>
        </Banner>
        <ErrorText>{create.error}</ErrorText>
        <Button type="submit" variant="primary" disabled={create.pending || !name.trim() || !objective.trim() || !schedule.trim() || teams.length === 0}>Create automation</Button>
      </form>
    </Card>
  );
}

function AutomationRow({ a, reload }: { a: Automation; reload: () => void }): React.JSX.Element {
  const [open, setOpen] = useState(false);
  const [del, setDel] = useState(false);
  const hist = useLoad(() => (open ? api.automations.history(a.id) : Promise.resolve([] as Fire[])), [open, a.id]);
  const toggle = useAction(async () => { await api.automations.setEnabled(a.id, !a.enabled); reload(); });
  const now = useAction(async () => { await api.automations.runNow(a.id); });
  const remove = useAction(async () => { await api.automations.remove(a.id); setDel(false); reload(); });
  return (
    <li className="px-4 py-3">
      <div className="flex flex-wrap items-center gap-3">
        <button type="button" className="min-w-0 flex-1 text-left" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
          <span className="block truncate text-sm font-medium">{a.name}</span>
          <span className="block truncate text-xs text-muted"><code className="font-mono">{a.schedule}</code> · {a.enabled ? (a.nextFireAt ? `next ${inFuture(a.nextFireAt)}` : "never fires") : "paused"}</span>
        </button>
        <Chip tone={a.enabled ? "good" : "neutral"}>{a.enabled ? "on" : "paused"}</Chip>
        <Button small onClick={() => void now.run()} disabled={!a.enabled}>Run now</Button>
        <Button small variant="ghost" onClick={() => void toggle.run()}>{a.enabled ? "Pause" : "Resume"}</Button>
        <Button small variant="ghost" onClick={() => setDel(true)}>Delete</Button>
      </div>
      <ErrorText>{toggle.error ?? now.error}</ErrorText>
      {open && (
        <div className="mt-2 space-y-1 border-t border-edge pt-2">
          <p className="text-xs text-muted">{a.objective}</p>
          {hist.loading && <Spinner />}
          {hist.data?.length === 0 && <p className="text-xs text-muted">Hasn't fired yet.</p>}
          {hist.data?.map((f) => (
            <p key={f.id} className="flex items-center gap-2 text-xs">
              <Chip tone={outcomeTone[f.outcome]}>{outcomeLabel[f.outcome]}</Chip>
              <span className="text-muted">{ago(f.startedAt)}</span>
            </p>
          ))}
        </div>
      )}
      <Confirm open={del} onOpenChange={setDel} title="Delete this automation?" confirmLabel="Delete" danger pending={remove.pending} error={remove.error} onConfirm={() => void remove.run()} body={<p>It stops running and its history is removed.</p>} />
    </li>
  );
}

function Findings({ onChanged }: { onChanged: () => void }): React.JSX.Element | null {
  const bump = useLive((s) => s.findings);
  const all = useLoad(() => api.automations.history(), [bump]);
  const findings = (all.data ?? []).filter((f) => ["completed", "blocked", "failed"].includes(f.outcome));
  const unseen = findings.filter((f) => !f.seen);
  if (findings.length === 0) return null;
  return (
    <section aria-label="Findings">
      <h2 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted">Findings {unseen.length > 0 && <Chip tone="accent">{unseen.length} new</Chip>}</h2>
      <Card>
        <ul className="divide-y divide-edge">
          {findings.slice(0, 8).map((f) => (
            <li key={f.id} className={`flex items-start gap-3 px-4 py-3 ${f.seen ? "opacity-70" : ""}`}>
              <Chip tone={outcomeTone[f.outcome]}>{outcomeLabel[f.outcome]}</Chip>
              <p className="min-w-0 flex-1 whitespace-pre-wrap break-words text-sm">{f.summary || "No details."}</p>
              <span className="text-xs text-muted">{ago(f.startedAt)}</span>
              {!f.seen && <Button small variant="ghost" onClick={() => void api.automations.markSeen(f.id).then(onChanged)}>Mark read</Button>}
            </li>
          ))}
        </ul>
      </Card>
    </section>
  );
}

export function AutomationsView({ workspaceId }: { workspaceId: string }): React.JSX.Element {
  const list = useLoad(() => api.automations.list(), []);
  const teams = useLoad(() => api.teams.list(), []);
  const [creating, setCreating] = useState(false);
  const mine = (list.data ?? []).filter((a) => a.workspaceId === workspaceId);
  return (
    <div className="mx-auto max-w-3xl space-y-6 p-5">
      <div className="flex items-center gap-3">
        <div className="flex-1">
          <h2 className="text-base font-semibold">Automations</h2>
          <p className="text-sm text-muted">Scheduled runs for this project. workmate has to be running for them to fire.</p>
        </div>
        <Button variant={creating ? "secondary" : "primary"} onClick={() => setCreating((c) => !c)}>{creating ? "Close" : "New automation"}</Button>
      </div>
      {creating && <Create workspaceId={workspaceId} teams={teams.data ?? []} onCreated={() => { list.reload(); setCreating(false); }} />}
      <Findings onChanged={list.reload} />
      <ErrorText>{list.error}</ErrorText>
      {list.loading && !list.data && <Spinner />}
      {list.data && mine.length === 0 && !creating && <Empty title="No automations">For example: check for vulnerable dependencies every night and only tell you if something turns up.</Empty>}
      {mine.length > 0 && <Card><ul className="divide-y divide-edge">{mine.map((a) => <AutomationRow key={a.id} a={a} reload={list.reload} />)}</ul></Card>}
    </div>
  );
}
