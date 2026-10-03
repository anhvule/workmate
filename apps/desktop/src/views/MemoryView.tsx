import { useState } from "react";
import { Button, Card, Chip, Confirm, Empty, ErrorText, Input, Modal, Spinner, Textarea } from "../components/ui.js";
import { api } from "../lib/api.js";
import { ago } from "../lib/format.js";
import { useAction, useLoad } from "../lib/hooks.js";
import type { Memory } from "../lib/types.js";

function Row({ m, names, onChanged, history }: { m: Memory; names: Map<string, string>; onChanged: () => void; history: boolean }): React.JSX.Element {
  const [editing, setEditing] = useState(false);
  const [subject, setSubject] = useState(m.subject);
  const [claim, setClaim] = useState(m.claim);
  const [del, setDel] = useState(false);
  const save = useAction(async () => {
    await api.memory.update(m.id, { subject, claim });
    setEditing(false);
    onChanged();
  });
  const pin = useAction(async () => {
    await api.memory.update(m.id, { pinned: !m.pinned });
    onChanged();
  });
  const remove = useAction(async () => {
    await api.memory.remove(m.id);
    setDel(false);
    onChanged();
  });
  const scopes = m.scopes.map((s) =>
    s.kind === "global" ? "everywhere" : s.kind === "workspace" ? (names.get(s.workspaceId) ?? "a removed workspace") : `role ${s.roleId}`,
  );
  return (
    <li className={`px-4 py-3 ${history ? "opacity-60" : ""}`}>
      {editing ? (
        <div className="space-y-2">
          <Input aria-label="Subject" value={subject} onChange={(e) => setSubject(e.target.value)} />
          <Textarea aria-label="Claim" rows={2} value={claim} onChange={(e) => setClaim(e.target.value)} />
          <ErrorText>{save.error}</ErrorText>
          <div className="flex gap-2">
            <Button small variant="primary" onClick={() => void save.run()} disabled={save.pending}>Save</Button>
            <Button small onClick={() => { setEditing(false); setSubject(m.subject); setClaim(m.claim); }}>Cancel</Button>
          </div>
        </div>
      ) : (
        <div className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <p className="text-sm"><strong>{m.subject}</strong> <span className="text-muted">—</span> {m.claim}</p>
            <p className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-muted">
              {scopes.length === 0 ? <Chip tone="warn">not attached to anything</Chip> : scopes.map((s) => <Chip key={s}>{s}</Chip>)}
              <span>recorded {ago(m.recordedAt)}</span>
              <span>· {m.lastUsedAt ? `last used ${ago(m.lastUsedAt)}` : "never recalled"}</span>
              {history && <Chip tone="neutral">replaced</Chip>}
            </p>
          </div>
          {!history && (
            <div className="flex shrink-0 gap-1">
              <Button small variant="ghost" aria-pressed={m.pinned} title="Pinned memories go into every turn's digest" onClick={() => void pin.run()}>{m.pinned ? "📌 Pinned" : "Pin"}</Button>
              <Button small variant="ghost" onClick={() => setEditing(true)}>Edit</Button>
              <Button small variant="ghost" onClick={() => setDel(true)}>Forget</Button>
            </div>
          )}
        </div>
      )}
      <ErrorText>{pin.error}</ErrorText>
      <Confirm
        open={del}
        onOpenChange={setDel}
        title="Forget this?"
        confirmLabel="Forget"
        danger
        onConfirm={() => void remove.run()}
        pending={remove.pending}
        error={remove.error}
        body={<p>Workmate will no longer know <strong>{m.subject}</strong>, and the older versions it replaced go with it.</p>}
      />
    </li>
  );
}

/**
 * Everything workmate remembers, across all projects. Memory is explicit — an
 * agent records a fact by calling `remember`, never by silent extraction — so
 * this is the place to read it, correct it and throw it away (ticket 006).
 */
export function MemoryView(): React.JSX.Element {
  const [history, setHistory] = useState(false);
  const mem = useLoad(() => api.memory.list(history), [history]);
  const wss = useLoad(() => api.workspaces.list(), []);
  const names = new Map((wss.data ?? []).map((w) => [w.workspace.id, w.workspace.name]));
  const [exportText, setExportText] = useState<string | null>(null);
  const exp = useAction(async () => setExportText(await api.memory.export()));
  const pinned = (mem.data ?? []).filter((m) => m.pinned && !m.supersededBy).length;

  return (
    <div className="mx-auto max-w-3xl space-y-4 p-5">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex-1">
          <h2 className="text-base font-semibold">What workmate remembers</h2>
          <p className="text-sm text-muted">Facts agents chose to record. Pinned ones ride along on every turn; the rest are looked up when needed.</p>
        </div>
        <label className="flex items-center gap-1.5 text-xs text-muted">
          <input type="checkbox" checked={history} onChange={(e) => setHistory(e.target.checked)} />
          Show replaced
        </label>
        <Button small onClick={() => void exp.run()}>Export markdown</Button>
      </div>
      {mem.loading && !mem.data && <Spinner />}
      <ErrorText>{mem.error ?? exp.error}</ErrorText>
      {mem.data?.length === 0 && (
        <Empty title="Nothing remembered yet">When an agent learns something durable about you or a project, it will show up here, where you can correct it.</Empty>
      )}
      {mem.data && mem.data.length > 0 && (
        <>
          <p className="text-xs text-muted">{pinned} pinned · {mem.data.length} total</p>
          <Card>
            <ul className="divide-y divide-edge">
              {mem.data.map((m) => <Row key={m.id} m={m} names={names} history={!!m.supersededBy} onChanged={mem.reload} />)}
            </ul>
          </Card>
        </>
      )}
      <Modal open={exportText !== null} onOpenChange={(o) => !o && setExportText(null)} title="Memory export" description="A read-only view. Edit memory here in workmate; the database stays the single source of truth." wide>
        <Textarea readOnly rows={14} value={exportText ?? ""} className="font-mono text-xs" />
        <div className="mt-3 flex justify-end">
          <Button onClick={() => void navigator.clipboard?.writeText(exportText ?? "")}>Copy</Button>
        </div>
      </Modal>
    </div>
  );
}
