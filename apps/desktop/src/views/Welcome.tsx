import { useState } from "react";
import { Button, Card, ErrorText, RoleChip } from "../components/ui.js";
import { api } from "../lib/api.js";
import { pickFolder } from "../lib/bridge.js";
import { baseName } from "../lib/format.js";
import { useAction, useLoad } from "../lib/hooks.js";
import type { Pack, WorkspaceView } from "../lib/types.js";

const POINTS: { title: string; body: string }[] = [
  { title: "It works on a branch", body: "Every run gets its own git branch and folder. Nothing touches your checkout until you read the diff and merge." },
  { title: "A team, not a chatbot", body: "A planner, a builder and a reviewer can hand work to each other. You see each handoff and can change it." },
  { title: "It remembers", body: "Agents record what they learn about you and your projects, and you can read, correct or delete every bit of it." },
];

function Choice({ pack, chosen, onChoose, recommended }: { pack: Pack; chosen: boolean; onChoose: () => void; recommended?: boolean }): React.JSX.Element {
  const solo = pack.team.roles.length === 1;
  return (
    <button
      type="button"
      onClick={onChoose}
      aria-pressed={chosen}
      className={`rounded-xl border p-4 text-left transition-colors ${chosen ? "border-accent bg-accent-soft" : "border-edge bg-raised hover:bg-panel"}`}
    >
      <p className="font-medium">{solo ? "One assistant" : "A team"}{recommended && <span className="ml-2 text-xs font-normal text-accent">good first step</span>}</p>
      <p className="mt-0.5 text-sm text-muted">{solo ? "A plain chat that works in your project. Start here." : pack.description}</p>
      {!solo && <p className="mt-2 flex flex-wrap gap-1.5">{pack.team.roles.map((r) => <RoleChip key={r.id} name={r.name} />)}</p>}
    </button>
  );
}

/**
 * What a brand-new user sees: no workspace, no key. The only thing asked for
 * before the app is useful is a folder; the key is asked for at the first
 * message, when the reason for asking is obvious (ticket 022).
 */
export function Welcome({ onReady }: { onReady: (w: WorkspaceView) => void }): React.JSX.Element {
  const packs = useLoad(() => api.packs.list(), []);
  const [folder, setFolder] = useState<string | null>(null);
  const [choice, setChoice] = useState("solo");
  const choose = packs.data?.filter((p) => p.id === "solo" || p.id === "plan-build-review") ?? [];

  const pick = useAction(async () => {
    const dir = await pickFolder();
    if (dir) setFolder(dir);
  });
  const go = useAction(async () => {
    if (!folder) return;
    const w = await api.workspaces.create(baseName(folder), folder);
    await api.packs.apply(choice, w.workspace.id);
    onReady(w);
  });

  return (
    <main className="mx-auto flex min-h-full max-w-2xl flex-col justify-center gap-8 px-6 py-10">
      <header>
        <h1 className="text-3xl font-semibold tracking-tight">workmate</h1>
        <p className="mt-2 text-lg text-muted">A dev team that remembers your projects and lives in your repo.</p>
      </header>

      <ul className="grid gap-3 sm:grid-cols-3">
        {POINTS.map((p) => (
          <li key={p.title}>
            <Card className="h-full p-3">
              <p className="text-sm font-medium">{p.title}</p>
              <p className="mt-1 text-xs text-muted">{p.body}</p>
            </Card>
          </li>
        ))}
      </ul>

      <section aria-label="Get started" className="space-y-4">
        <div>
          <p className="mb-2 text-sm font-medium">1. Choose a project</p>
          <div className="flex items-center gap-3">
            <Button variant={folder ? "secondary" : "primary"} onClick={() => void pick.run()}>{folder ? "Change folder…" : "Choose a folder…"}</Button>
            {folder && <code className="min-w-0 truncate font-mono text-xs text-muted">{folder}</code>}
          </div>
          <ErrorText>{pick.error}</ErrorText>
        </div>

        {folder && (
          <div>
            <p className="mb-2 text-sm font-medium">2. How do you want to work?</p>
            <div className="grid gap-3 sm:grid-cols-2">
              {choose.map((p) => <Choice key={p.id} pack={p} chosen={choice === p.id} recommended={p.id === "solo"} onChoose={() => setChoice(p.id)} />)}
            </div>
            <p className="mt-2 text-xs text-muted">You can add the other later from the Library. Either way, a starter AGENTS.md is added to your project if it doesn't have one.</p>
            <ErrorText>{go.error}</ErrorText>
            <Button variant="primary" className="mt-4" disabled={go.pending} onClick={() => void go.run()}>{go.pending ? "Setting up…" : "Open project"}</Button>
          </div>
        )}
      </section>
    </main>
  );
}
