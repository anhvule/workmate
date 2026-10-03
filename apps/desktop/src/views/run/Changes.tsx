import { useState } from "react";
import { api } from "../../lib/api.js";
import { useAction, useLoad } from "../../lib/hooks.js";
import { Button, Chip, Confirm, ErrorText, Spinner } from "../../components/ui.js";
import type { Phase } from "../../lib/types.js";

const kindTone = { added: "good", modified: "accent", deleted: "danger", renamed: "neutral" } as const;

function Patch({ text }: { text: string }): React.JSX.Element {
  return (
    <pre className="max-h-96 overflow-auto rounded-md bg-panel p-3 font-mono text-xs leading-relaxed">
      {text.split("\n").map((line, i) => (
        <div
          key={i}
          className={
            line.startsWith("+") && !line.startsWith("+++")
              ? "bg-good/10 text-good"
              : line.startsWith("-") && !line.startsWith("---")
                ? "bg-danger-soft text-danger"
                : line.startsWith("@@") || line.startsWith("diff")
                  ? "text-muted"
                  : ""
          }
        >
          {line || " "}
        </div>
      ))}
    </pre>
  );
}

/**
 * What the run did, and what to do about it. Merging is workmate's one
 * deliberate action on your branch: it only ever happens here, on your word,
 * and it refuses rather than guesses (ticket 007).
 */
export function Changes({
  workspaceId,
  runId,
  branch,
  state,
  version,
  onChanged,
}: {
  workspaceId: string;
  runId: string;
  branch: string;
  state: Phase;
  version: number;
  onChanged: () => void;
}): React.JSX.Element {
  const gone = state === "archived";
  const diff = useLoad(() => (gone ? Promise.resolve(undefined) : api.runs.diff(workspaceId, runId)), [workspaceId, runId, gone, version]);
  const [confirm, setConfirm] = useState<"merge" | "abandon" | "archive" | null>(null);
  const [force, setForce] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const merge = useAction(async () => {
    const out = await api.runs.merge(workspaceId, runId);
    setNote(out === "upToDate" ? "Already up to date; nothing to merge." : out === "fastForward" ? "Merged (fast-forward) into your current branch." : "Merged into your current branch with a merge commit.");
    setConfirm(null);
    onChanged();
  });
  const archive = useAction(async () => {
    await api.runs.archive(workspaceId, runId);
    setConfirm(null);
    onChanged();
  });
  const abandon = useAction(async () => {
    await api.runs.abandon(workspaceId, runId, force);
    setConfirm(null);
    onChanged();
  });
  const reveal = useAction(async () => {
    await api.runs.reveal(workspaceId, runId);
  });

  if (gone) {
    return (
      <p className="text-sm text-muted">
        This run is archived. Its work is kept on <code className="font-mono">{branch}</code>; the working folder was removed.
      </p>
    );
  }
  const d = diff.data;
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <code className="rounded bg-panel px-1.5 py-0.5 font-mono text-xs">{branch}</code>
        {d && (
          <span className="text-xs text-muted">
            {d.files.length} file{d.files.length === 1 ? "" : "s"} · <span className="text-good">+{d.additions}</span>{" "}
            <span className="text-danger">−{d.deletions}</span>
          </span>
        )}
        <span className="ml-auto flex gap-2">
          <Button small onClick={() => void reveal.run()}>Reveal folder</Button>
          <Button small onClick={() => { setForce(false); abandon.clear(); setConfirm("abandon"); }}>Abandon</Button>
          <Button small onClick={() => { archive.clear(); setConfirm("archive"); }}>Archive</Button>
          <Button small variant="primary" disabled={!d || d.files.length === 0} onClick={() => { merge.clear(); setConfirm("merge"); }}>
            Merge…
          </Button>
        </span>
      </div>
      <ErrorText>{reveal.error}</ErrorText>
      {note && <p className="text-sm text-good">{note}</p>}
      {diff.loading && !d && <Spinner label="Reading changes" />}
      <ErrorText>{diff.error}</ErrorText>
      {d && d.files.length === 0 && <p className="text-sm text-muted">No changes in this run's folder yet.</p>}
      {d && d.files.length > 0 && (
        <>
          <ul className="space-y-1">
            {d.files.map((f) => (
              <li key={f.path} className="flex items-center gap-2 font-mono text-xs">
                <Chip tone={kindTone[f.kind]}>{f.kind}</Chip>
                {f.path}
              </li>
            ))}
          </ul>
          <Patch text={d.patch} />
        </>
      )}

      <Confirm
        open={confirm === "merge"}
        onOpenChange={(o) => !o && setConfirm(null)}
        title="Merge this run into your branch?"
        confirmLabel="Merge"
        onConfirm={() => void merge.run()}
        pending={merge.pending}
        error={merge.error}
        body={<p>This brings <code className="font-mono">{branch}</code> into the branch you have checked out. It refuses if you have uncommitted changes or if the two conflict, and in both cases changes nothing.</p>}
      />
      <Confirm
        open={confirm === "archive"}
        onOpenChange={(o) => !o && setConfirm(null)}
        title="Archive this run?"
        confirmLabel="Archive"
        onConfirm={() => void archive.run()}
        pending={archive.pending}
        error={archive.error}
        body={<p>The working folder is removed and the branch is kept, so you can still merge it later. It refuses if there are uncommitted changes in the folder.</p>}
      />
      <Confirm
        open={confirm === "abandon"}
        onOpenChange={(o) => !o && setConfirm(null)}
        title="Throw this run's work away?"
        confirmLabel="Abandon"
        danger
        onConfirm={() => void abandon.run()}
        pending={abandon.pending}
        error={abandon.error}
        body={
          <div className="space-y-3">
            <p>This deletes the working folder <strong>and the branch</strong>. It cannot be undone.</p>
            {abandon.error && (
              <label className="flex items-center gap-2">
                <input type="checkbox" checked={force} onChange={(e) => setForce(e.target.checked)} />
                Discard uncommitted changes too
              </label>
            )}
          </div>
        }
      />
    </div>
  );
}
