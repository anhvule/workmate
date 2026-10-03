import { useState } from "react";
import { KeyForm } from "../components/KeyForm.js";
import { Banner, Button, Card, ErrorText, RoleChip, Select, Spinner, Textarea } from "../components/ui.js";
import { ModelPicker } from "../components/ModelPicker.js";
import { api } from "../lib/api.js";
import { useAction, useLoad } from "../lib/hooks.js";
import { providerName } from "../lib/providers.js";
import type { Role, Team } from "../lib/types.js";

/** What stands between this team and a first message that works. */
function useReadiness(workspaceId: string, team: Team | undefined, tick: number) {
  return useLoad(async () => {
    const [repo, model] = await Promise.all([api.workspaces.isRepo(workspaceId), api.models.getDefault()]);
    const resolved: Role[] = (team?.roles ?? []).map((r) => ({
      ...r,
      providerId: r.providerId ?? model.provider,
      modelId: r.modelId ?? model.model,
    }));
    const needModel = resolved.some((r) => !r.providerId || !r.modelId);
    const providers = [...new Set(resolved.map((r) => r.providerId).filter((p): p is string => !!p))];
    const statuses = await Promise.all(providers.map(async (p) => [p, (await api.credentials.status(p, undefined, workspaceId)).found] as const));
    return { repo, needModel, missingKeys: statuses.filter(([, ok]) => !ok).map(([p]) => p), resolved };
  }, [workspaceId, team?.id, team?.roles.map((r) => `${r.providerId}/${r.modelId}`).join(), tick]);
}

export function NewRun({ workspaceId, onStarted }: { workspaceId: string; onStarted: (runId: string) => void }): React.JSX.Element {
  const teams = useLoad(() => api.teams.list(), []);
  const [teamId, setTeamId] = useState<string>("");
  const [objective, setObjective] = useState("");
  const [review, setReview] = useState(false);
  const [tick, setTick] = useState(0);
  const team = teams.data?.find((t) => t.id === teamId) ?? teams.data?.[0];
  const ready = useReadiness(workspaceId, team, tick);
  const bump = (): void => setTick((t) => t + 1);

  const start = useAction(async () => {
    if (!team || !ready.data) return;
    const { runId } = await api.runs.start({
      workspaceId,
      objective: objective.trim(),
      roles: ready.data.resolved,
      teamId: team.id,
      review: review && team.roles.length > 1,
    });
    setObjective("");
    onStarted(runId);
  });

  if (teams.loading && !teams.data) return <Spinner label="Loading teams" />;
  if (!team) {
    return (
      <Banner tone="accent">
        <p className="font-medium">Choose how you want to work first</p>
        <p className="text-muted">Pick a starter in the Library tab: a single assistant, or a team that plans, builds and reviews.</p>
      </Banner>
    );
  }
  const r = ready.data;
  const blocker = r && (!r.repo ? "repo" : r.needModel ? "model" : r.missingKeys.length > 0 ? "key" : null);
  const solo = team.roles.length === 1;

  return (
    <Card className="p-4">
      <form
        className="space-y-3"
        onSubmit={(e) => {
          e.preventDefault();
          if (!blocker && objective.trim()) void start.run();
        }}
      >
        <Textarea
          aria-label="What should we work on?"
          rows={3}
          value={objective}
          onChange={(e) => setObjective(e.target.value)}
          placeholder={solo ? "What would you like to do?" : "Describe the goal. The team will plan, build and review it."}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              if (!blocker && objective.trim()) void start.run();
            }
          }}
        />
        <div className="flex flex-wrap items-center gap-3">
          <Select aria-label="Team" value={team.id} onChange={(e) => setTeamId(e.target.value)} className="w-auto">
            {teams.data!.map((t) => (
              <option key={t.id} value={t.id}>{t.roles.length === 1 ? `${t.name} (single assistant)` : `${t.name} (${t.roles.length} roles)`}</option>
            ))}
          </Select>
          <span className="flex flex-wrap gap-1">
            {!solo && team.roles.map((x) => <RoleChip key={x.id} name={x.name} />)}
          </span>
          {!solo && (
            <label className="flex items-center gap-1.5 text-xs text-muted">
              <input type="checkbox" checked={review} onChange={(e) => setReview(e.target.checked)} />
              Pause at each handoff
            </label>
          )}
          <Button type="submit" variant="primary" className="ml-auto" disabled={!!blocker || !r || objective.trim() === "" || start.pending}>
            {start.pending ? "Starting…" : "Start"}
          </Button>
        </div>
        <ErrorText>{start.error}</ErrorText>
      </form>

      {r && blocker === "repo" && (
        <div className="mt-3">
          <Banner tone="warn">
            <p className="font-medium">This folder is not a git repository yet</p>
            <p className="text-muted">Every run works on its own branch, so workmate needs git. In a terminal here, run <code className="font-mono">git init</code> and make a first commit, then come back.</p>
          </Banner>
        </div>
      )}
      {r && blocker === "model" && (
        <div className="mt-3 space-y-2 rounded-lg bg-panel p-3">
          <p className="text-sm font-medium">Choose a model</p>
          <p className="text-xs text-muted">Your team's roles don't name one. Pick a default; you can change a role's model later.</p>
          <ModelPicker onSaved={bump} />
        </div>
      )}
      {r && blocker === "key" && r.missingKeys[0] && (
        <div className="mt-3 space-y-2 rounded-lg bg-panel p-3">
          <p className="text-sm font-medium">Add your {providerName(r.missingKeys[0])} API key</p>
          <p className="text-xs text-muted">That's the only thing needed to start. It's kept in your system keychain.</p>
          <KeyForm provider={r.missingKeys[0]} scope={{ kind: "global" }} onSaved={bump} />
        </div>
      )}
    </Card>
  );
}
