import * as Tabs from "@radix-ui/react-tabs";
import { useState } from "react";
import { Banner, Button, Chip, Confirm, ErrorText } from "../components/ui.js";
import { api } from "../lib/api.js";
import { pickFolder } from "../lib/bridge.js";
import { baseName } from "../lib/format.js";
import { useAction, useLoad } from "../lib/hooks.js";
import { useLive } from "../lib/live.js";
import type { WorkspaceView } from "../lib/types.js";
import { AutomationsView } from "./AutomationsView.js";
import { LibraryView } from "./LibraryView.js";
import { MemoryView } from "./MemoryView.js";
import { RunPage } from "./run/RunPage.js";
import { RunsView } from "./RunsView.js";
import { SettingsView } from "./SettingsView.js";

const TABS = ["runs", "memory", "automations", "library", "settings"] as const;
type Tab = (typeof TABS)[number];
const LABEL: Record<Tab, string> = { runs: "Runs", memory: "Memory", automations: "Automations", library: "Library", settings: "Settings" };

function Repair({ w, onFixed }: { w: WorkspaceView; onFixed: () => void }): React.JSX.Element {
  const fix = useAction(async () => {
    const dir = await pickFolder();
    if (!dir) return;
    await api.workspaces.relocate(w.workspace.id, dir);
    onFixed();
  });
  return (
    <div className="p-4">
      <Banner tone="warn" action={<Button small onClick={() => void fix.run()}>Locate folder…</Button>}>
        <p className="font-medium">{w.workspace.name}'s folder isn't where it was</p>
        <p className="font-mono text-xs text-muted">{w.workspace.directory}</p>
        <p className="mt-1 text-muted">It may have moved or an external drive may be unplugged. Point workmate at it again; nothing is lost — runs, memory and settings are kept.</p>
        <ErrorText>{fix.error}</ErrorText>
      </Banner>
    </div>
  );
}

function Sidebar({ workspaces, activeId, onSelect, onAdd, onRemove }: { workspaces: WorkspaceView[]; activeId: string; onSelect: (id: string) => void; onAdd: () => void; onRemove: (w: WorkspaceView) => void }): React.JSX.Element {
  return (
    <nav aria-label="Projects" className="flex h-full w-56 shrink-0 flex-col border-r border-edge bg-panel">
      <div className="px-4 py-3 text-sm font-semibold tracking-tight">workmate</div>
      <ul className="flex-1 space-y-0.5 overflow-auto px-2">
        {workspaces.map((w) => (
          <li key={w.workspace.id} className="group flex items-center">
            <button
              type="button"
              onClick={() => onSelect(w.workspace.id)}
              aria-current={w.workspace.id === activeId ? "page" : undefined}
              className={`min-w-0 flex-1 rounded-md px-2 py-1.5 text-left text-sm ${w.workspace.id === activeId ? "bg-raised font-medium shadow-sm" : "hover:bg-raised/60"}`}
              title={w.workspace.directory}
            >
              <span className="block truncate">{w.workspace.name}</span>
              {w.binding === "missing" && <span className="text-xs text-warn">folder missing</span>}
            </button>
            <button type="button" aria-label={`Remove ${w.workspace.name}`} onClick={() => onRemove(w)} className="invisible rounded px-1.5 text-muted hover:text-danger group-hover:visible focus:visible">×</button>
          </li>
        ))}
      </ul>
      <div className="p-2"><Button className="w-full" onClick={onAdd}>Add project…</Button></div>
    </nav>
  );
}

export function Shell({ workspaces, activeId, onSelect, onChanged }: { workspaces: WorkspaceView[]; activeId: string; onSelect: (id: string) => void; onChanged: () => void }): React.JSX.Element {
  const [tab, setTab] = useState<Tab>("runs");
  const [runId, setRunId] = useState<string | null>(null);
  const [removing, setRemoving] = useState<WorkspaceView | null>(null);
  const w = workspaces.find((x) => x.workspace.id === activeId) ?? workspaces[0]!;
  const findings = useLive((s) => s.findings);
  const unseen = useLoad(() => api.automations.unseen(), [findings, tab]);

  const add = useAction(async () => {
    const dir = await pickFolder();
    if (!dir) return;
    const created = await api.workspaces.create(baseName(dir), dir);
    onChanged();
    onSelect(created.workspace.id);
    setRunId(null);
  });
  const remove = useAction(async () => {
    if (!removing) return;
    await api.workspaces.remove(removing.workspace.id);
    setRemoving(null);
    onChanged();
  });

  return (
    <div className="flex h-full">
      <Sidebar workspaces={workspaces} activeId={w.workspace.id} onSelect={(id) => { onSelect(id); setRunId(null); setTab("runs"); }} onAdd={() => void add.run()} onRemove={(x) => { remove.clear(); setRemoving(x); }} />
      <div className="flex min-w-0 flex-1 flex-col">
        <Tabs.Root value={tab} onValueChange={(v) => { setTab(v as Tab); if (v !== "runs") setRunId(null); }} className="flex min-h-0 flex-1 flex-col">
          <div className="flex items-center gap-3 border-b border-edge px-4">
            <h1 className="min-w-0 truncate py-3 text-sm font-semibold" title={w.workspace.directory}>{w.workspace.name}</h1>
            <Tabs.List aria-label="Sections" className="flex gap-1">
              {TABS.map((t) => (
                <Tabs.Trigger key={t} value={t} className="relative px-2.5 py-3 text-sm text-muted data-[state=active]:font-medium data-[state=active]:text-ink data-[state=active]:after:absolute data-[state=active]:after:inset-x-2 data-[state=active]:after:bottom-0 data-[state=active]:after:h-0.5 data-[state=active]:after:bg-accent">
                  {LABEL[t]}
                  {t === "automations" && (unseen.data ?? 0) > 0 && <span className="ml-1.5 align-middle"><Chip tone="accent">{unseen.data}</Chip></span>}
                </Tabs.Trigger>
              ))}
            </Tabs.List>
          </div>
          <ErrorText>{add.error}</ErrorText>
          {w.binding === "missing" ? (
            <Repair w={w} onFixed={onChanged} />
          ) : (
            <div className="min-h-0 flex-1 overflow-auto">
              <Tabs.Content value="runs" className="h-full">
                {runId ? <RunPage workspaceId={w.workspace.id} runId={runId} onBack={() => setRunId(null)} /> : <RunsView workspaceId={w.workspace.id} onOpen={setRunId} />}
              </Tabs.Content>
              <Tabs.Content value="memory"><MemoryView /></Tabs.Content>
              <Tabs.Content value="automations"><AutomationsView workspaceId={w.workspace.id} /></Tabs.Content>
              <Tabs.Content value="library"><LibraryView workspaceId={w.workspace.id} /></Tabs.Content>
              <Tabs.Content value="settings"><SettingsView workspaceId={w.workspace.id} /></Tabs.Content>
            </div>
          )}
        </Tabs.Root>
      </div>
      <Confirm
        open={removing !== null}
        onOpenChange={(o) => !o && setRemoving(null)}
        title={`Remove ${removing?.workspace.name ?? ""}?`}
        confirmLabel="Remove project"
        danger
        pending={remove.pending}
        error={remove.error}
        onConfirm={() => void remove.run()}
        body={<p>workmate forgets this project, its runs, automations and settings. <strong>Your files are not touched, and what workmate remembers is kept</strong> — it is detached from the project, not deleted.</p>}
      />
    </div>
  );
}
