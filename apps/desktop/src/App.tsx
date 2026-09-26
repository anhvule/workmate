import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface EngineInfo {
  readonly pinnedVersion: string;
  readonly sidecarPresent: boolean;
}

/**
 * Scaffold shell.
 *
 * Deliberately thin: this proves the Rust boundary and the pinned-engine wiring
 * are real, and nothing more. Workspace, run and team surfaces land in their own
 * tickets, against the domain model in `@workmate/core`.
 */
export function App(): React.JSX.Element {
  const [engine, setEngine] = useState<EngineInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<EngineInfo>("engine_info").then(setEngine).catch((e: unknown) => {
      setError(e instanceof Error ? e.message : String(e));
    });
  }, []);

  return (
    <main className="flex h-full flex-col items-center justify-center gap-3 bg-ground text-ink">
      <h1 className="text-2xl font-semibold tracking-tight">workmate</h1>
      <p className="text-sm text-muted">
        A dev team that remembers your projects and lives in your repo.
      </p>
      <div className="mt-4 rounded-md border border-edge px-4 py-2 font-mono text-xs text-muted">
        {error !== null
          ? `engine: unavailable — ${error}`
          : engine === null
            ? "engine: checking…"
            : `engine: opencode ${engine.pinnedVersion} · sidecar ${engine.sidecarPresent ? "bundled" : "missing"}`}
      </div>
    </main>
  );
}
