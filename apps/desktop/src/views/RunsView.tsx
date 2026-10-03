import { Chip, Empty, ErrorText, Spinner } from "../components/ui.js";
import { ago } from "../lib/format.js";
import { useLoad } from "../lib/hooks.js";
import { api } from "../lib/api.js";
import { useLive } from "../lib/live.js";
import type { Phase } from "../lib/types.js";
import { NewRun } from "./NewRun.js";

const tone: Record<Phase, "accent" | "warn" | "danger" | "good" | "neutral"> = {
  running: "accent",
  paused: "warn",
  blocked: "danger",
  done: "good",
  archived: "neutral",
};

export function RunsView({ workspaceId, onOpen }: { workspaceId: string; onOpen: (runId: string) => void }): React.JSX.Element {
  // Any run event makes the list stale; Rust is asked again rather than patched.
  const stamp = useLive((s) => Object.values(s.runs).reduce((n, r) => n + r.version, 0));
  const runs = useLoad(() => api.runs.list(workspaceId), [workspaceId, stamp]);
  return (
    <div className="mx-auto max-w-3xl space-y-6 p-5">
      <NewRun workspaceId={workspaceId} onStarted={onOpen} />
      <section aria-label="Runs">
        <h2 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted">Runs</h2>
        {runs.loading && !runs.data && <Spinner />}
        <ErrorText>{runs.error}</ErrorText>
        {runs.data?.length === 0 && <Empty title="No runs yet">Describe something above. Each run works on its own branch, so your checkout stays untouched until you merge.</Empty>}
        <ul className="divide-y divide-edge rounded-lg border border-edge bg-raised">
          {runs.data?.map((r) => (
            <li key={r.id}>
              <button type="button" onClick={() => onOpen(r.id)} className="flex w-full items-center gap-3 px-3 py-2.5 text-left hover:bg-panel">
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">{r.objective}</span>
                  <span className="block truncate font-mono text-xs text-muted">{r.branch}</span>
                </span>
                <span className="text-xs text-muted">{ago(r.createdAt)}</span>
                <Chip tone={tone[r.state]}>{r.state}</Chip>
              </button>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
