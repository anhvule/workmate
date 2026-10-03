import { useEffect, useState } from "react";
import { Banner, Button, Spinner } from "./components/ui.js";
import { api } from "./lib/api.js";
import { messageOf, useLoad } from "./lib/hooks.js";
import { clearFault, startLive, useLive } from "./lib/live.js";
import type { WorkspaceView } from "./lib/types.js";
import { PermissionModal } from "./views/PermissionModal.js";
import { Shell } from "./views/Shell.js";
import { Welcome } from "./views/Welcome.js";

/**
 * Boot, then show either the first-run welcome or the shell.
 *
 * Nothing here is persisted by the webview: which project is open is component
 * state, and everything else is read from Rust.
 */
export function App(): React.JSX.Element {
  const [runtime, setRuntime] = useState<"starting" | "up" | { error: string }>("starting");
  const [activeId, setActiveId] = useState<string | null>(null);
  const workspaces = useLoad(() => api.workspaces.list(), []);
  const fault = useLive((s) => s.fault);

  const boot = (): void => {
    setRuntime("starting");
    api.boot().then(() => setRuntime("up"), (e: unknown) => setRuntime({ error: messageOf(e) }));
  };
  useEffect(() => {
    void startLive();
    boot();
  }, []);

  if (workspaces.loading && !workspaces.data) return <div className="grid h-full place-items-center"><Spinner label="Starting workmate" /></div>;
  const list: WorkspaceView[] = workspaces.data ?? [];

  return (
    <div className="flex h-full flex-col">
      {typeof runtime === "object" && (
        <div className="p-2">
          <Banner tone="danger" action={<Button small onClick={boot}>Try again</Button>}>
            <p className="font-medium">The agent engine didn't start</p>
            <p className="break-words text-muted">{runtime.error}. You can browse everything, but runs can't start until this is fixed.</p>
          </Banner>
        </div>
      )}
      {fault && (
        <div className="p-2">
          <Banner tone="danger" action={<Button small onClick={clearFault}>Dismiss</Button>}>{fault}</Banner>
        </div>
      )}
      <div className="min-h-0 flex-1">
        {list.length === 0 ? (
          <Welcome onReady={(w) => { setActiveId(w.workspace.id); workspaces.reload(); }} />
        ) : (
          <Shell workspaces={list} activeId={activeId ?? list[0]!.workspace.id} onSelect={setActiveId} onChanged={workspaces.reload} />
        )}
      </div>
      {runtime === "starting" && <p className="pointer-events-none fixed bottom-3 right-4 rounded-full bg-raised px-3 py-1 text-xs text-muted shadow">starting the engine…</p>}
      <PermissionModal />
    </div>
  );
}
