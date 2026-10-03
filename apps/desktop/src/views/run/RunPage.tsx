import { isSoloRun, runTimeline, type Run as CoreRun } from "@workmate/core";
import { useMemo, useState } from "react";
import { KeyForm } from "../../components/KeyForm.js";
import { Banner, Button, Card, Chip, ErrorText, RoleChip, Select, Spinner, Textarea } from "../../components/ui.js";
import { api } from "../../lib/api.js";
import { bareRole } from "../../lib/format.js";
import { useAction, useLoad } from "../../lib/hooks.js";
import { useRun, useStreaming, useLive } from "../../lib/live.js";
import type { PersistedRun, Phase, Role } from "../../lib/types.js";
import { Changes } from "./Changes.js";
import { Thread, type ThreadSession } from "./Thread.js";

const phaseTone: Record<Phase, "accent" | "warn" | "danger" | "good" | "neutral"> = {
  running: "accent",
  paused: "warn",
  blocked: "danger",
  done: "good",
  archived: "neutral",
};

/** The persisted run, in the order Rust stored it, shaped for the core domain. */
const toCore = (r: PersistedRun): CoreRun =>
  ({
    id: r.id,
    state: r.state,
    sessions: r.sessions.map((s) => ({ id: s.id, role: s.role, directory: s.directory, engineVersion: s.engineVersion, startedAt: s.startedAt })),
    handoffs: r.handoffs.map((h) => ({ from: h.from, to: h.to, context: h.context, at: h.at })),
  }) as unknown as CoreRun;

/** A handoff waiting on the user: the thing a paused run exists to show. */
function PendingHandoff({ runId, roles, to, from, context }: { runId: string; roles: Role[]; to: string; from: string; context: string }): React.JSX.Element {
  const [text, setText] = useState(context);
  const [redirect, setRedirect] = useState("");
  const go = useAction(async () => {
    if (text !== context) await api.runs.amend(runId, text);
    else await api.runs.resume(runId);
  });
  const redir = useAction(async () => api.runs.redirect(runId, redirect));
  const veto = useAction(async () => api.runs.veto(runId));
  return (
    <Card className="border-warn/40 p-4">
      <div className="mb-2 flex flex-wrap items-center gap-2 text-sm">
        <strong>Paused before the handoff</strong>
        <RoleChip name={from} />
        <span aria-hidden>→</span>
        <RoleChip name={to} />
      </div>
      <p className="mb-2 text-xs text-muted">This is exactly what {to} will be told. Edit it, send it somewhere else, or end the run here.</p>
      <Textarea aria-label="Handoff context" rows={8} value={text} onChange={(e) => setText(e.target.value)} className="font-mono text-xs" />
      <ErrorText>{go.error ?? redir.error ?? veto.error}</ErrorText>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        <Button variant="primary" disabled={go.pending} onClick={() => void go.run()}>
          {text !== context ? `Send amended to ${to}` : `Continue to ${to}`}
        </Button>
        <span className="flex items-center gap-1">
          <Select aria-label="Redirect to" value={redirect} onChange={(e) => setRedirect(e.target.value)} className="w-40">
            <option value="">Redirect to…</option>
            {roles.map((r) => (
              <option key={r.id} value={r.id}>{r.name}</option>
            ))}
          </Select>
          <Button disabled={!redirect || redir.pending} onClick={() => void redir.run()}>Go</Button>
        </span>
        <Button variant="danger" className="ml-auto" disabled={veto.pending} onClick={() => void veto.run()}>End run</Button>
      </div>
    </Card>
  );
}

/** A run that stopped for something the user can fix. Resuming retries; nothing done is lost. */
function Blocked({ runId, workspaceId, reason }: { runId: string; workspaceId: string; reason: NonNullable<ReturnType<typeof useRun>["blocked"]> | undefined }): React.JSX.Element {
  const resume = useAction(async () => api.runs.resume(runId));
  if (reason?.kind === "credential") {
    return (
      <Card className="border-danger/40 p-4">
        <p className="font-medium">{reason.role} needs an API key for {reason.providerId}</p>
        <p className="mb-3 mt-1 text-sm text-muted">Everything finished so far is kept. Add the key and the run picks up where it stopped.</p>
        <KeyForm
          provider={reason.providerId}
          scope={{ kind: "global" }}
          cta="Save key and continue"
          onSaved={() => void resume.run()}
        />
        <ErrorText>{resume.error}</ErrorText>
        <details className="mt-3 text-xs text-muted">
          <summary>Where workmate looked</summary>
          <ul className="mt-1 font-mono">{reason.tried.map((t) => <li key={t}>{t}</li>)}</ul>
        </details>
        <span className="hidden">{workspaceId}</span>
      </Card>
    );
  }
  return (
    <Banner
      tone="danger"
      action={<Button small onClick={() => void resume.run()} disabled={resume.pending}>Retry</Button>}
    >
      <p className="font-medium">{reason ? `${reason.role || "The run"} hit an error` : "This run is blocked"}</p>
      <p className="mt-0.5 break-words text-muted">{reason?.kind === "engine" ? reason.message : "Try again; nothing done so far is lost."}</p>
      <ErrorText>{resume.error}</ErrorText>
    </Banner>
  );
}

function FollowUp({ runId, onSent, disabled }: { runId: string; onSent: (t: string) => void; disabled: boolean }): React.JSX.Element {
  const [text, setText] = useState("");
  const send = useAction(async () => {
    const t = text.trim();
    if (!t) return;
    onSent(t);
    setText("");
    await api.runs.say(runId, t);
  });
  return (
    <form
      className="sticky bottom-0 -mx-1 bg-ground/95 px-1 pb-3 pt-2 backdrop-blur"
      onSubmit={(e) => {
        e.preventDefault();
        void send.run();
      }}
    >
      <div className="flex items-end gap-2">
        <Textarea
          aria-label="Message"
          rows={2}
          value={text}
          disabled={disabled}
          placeholder={disabled ? "Waiting for the current turn…" : "Send a follow-up"}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              void send.run();
            }
          }}
        />
        <Button type="submit" variant="primary" disabled={disabled || send.pending || text.trim() === ""}>Send</Button>
      </div>
      <ErrorText>{send.error}</ErrorText>
    </form>
  );
}

export function RunPage({ workspaceId, runId, onBack }: { workspaceId: string; runId: string; onBack: () => void }): React.JSX.Element {
  const live = useRun(runId);
  const resync = useLive((s) => s.resync);
  const run = useLoad(() => api.runs.get(runId), [runId, live.version, resync]);
  const teams = useLoad(() => api.teams.list(), []);
  const [showChanges, setShowChanges] = useState(false);
  const [sent, setSent] = useState<{ text: string; at: number }[]>([]);
  const pause = useAction(async () => api.runs.pause(runId));
  const resume = useAction(async () => api.runs.resume(runId));

  const r = run.data;
  const sessionKey = r?.sessions.map((s) => s.id).join(",") ?? "";
  const transcripts = useLoad(
    async () => (r ? Promise.all(r.sessions.map((s) => api.runs.transcript(s.id, s.directory).catch(() => []))) : []),
    [sessionKey, live.version, resync],
  );
  const roles = useMemo(() => new Map((teams.data ?? []).flatMap((t) => t.roles).map((x) => [x.id, x])), [teams.data]);
  const lastSession = r?.sessions.at(-1)?.id;
  const streaming = useStreaming(lastSession);

  if (run.loading && !r) return <div className="p-6"><Spinner label="Opening run" /></div>;
  if (!r) return <div className="p-6"><ErrorText>{run.error ?? "Run not found."}</ErrorText></div>;

  const phase: Phase = live.phase ?? r.state;
  const core = toCore(r);
  // The team says what kind of run this is. Counting sessions would call a team
  // run "solo" until its second role starts, and hide the strip it should show.
  const team = r.teamId ? teams.data?.find((t) => t.id === r.teamId) : undefined;
  const solo = team ? team.roles.length === 1 : isSoloRun(core);
  const order = runTimeline(core);
  const byId = new Map(r.sessions.map((s, i) => [s.id, { s, i }]));
  const sessions: ThreadSession[] = order.flatMap((o) => {
    const hit = byId.get(o.id);
    if (!hit) return [];
    return [{
      session: hit.s,
      messages: transcripts.data?.[hit.i] ?? [],
      streaming: hit.s.id === lastSession ? streaming : [],
      handoff: r.handoffs.find((h) => h.to === hit.s.id),
    }];
  });
  // A follow-up shows at once, then the transcript takes over: local, discarded
  // when the authoritative copy arrives.
  const lastMessages = sessions.at(-1)?.messages ?? [];
  const optimistic = sent.filter((m) => !lastMessages.some((x) => x.role === "user" && x.text === m.text));

  const pending = live.pending && phase === "paused" ? live.pending : undefined;
  const working = phase === "running";
  // The orchestrator names the sending *session*; show its role.
  const fromName = (sessionId: string): string => {
    const role = r.sessions.find((x) => x.id === sessionId)?.role;
    return role ? (roles.get(role)?.name ?? bareRole(role)) : sessionId;
  };

  return (
    <div className="mx-auto flex h-full max-w-3xl flex-col px-5">
      <header className="space-y-2 border-b border-edge py-3">
        <div className="flex items-start gap-3">
          <Button variant="ghost" small onClick={onBack} aria-label="Back to runs">←</Button>
          <h2 className="min-w-0 flex-1 text-base font-semibold leading-snug">{r.objective}</h2>
          <Chip tone={phaseTone[phase]}>{phase}</Chip>
        </div>
        <div className="flex flex-wrap items-center gap-2 pl-9">
          {!solo && (
            // The header strip exists only for a team: a solo run reads as a chat.
            <div className="flex flex-wrap gap-1.5" aria-label="Roles">
              {(team?.roles ?? r.sessions.map((s) => ({ id: s.role, name: bareRole(s.role) } as Role))).map((role) => {
                const spoke = r.sessions.some((s) => s.role === role.id);
                const active = working && r.sessions.at(-1)?.role === role.id;
                return <RoleChip key={role.id} name={role.name} state={active ? "working" : phase === "blocked" && r.sessions.at(-1)?.role === role.id ? "blocked" : spoke ? "done" : "waiting"} />;
              })}
            </div>
          )}
          <span className="ml-auto flex items-center gap-2">
            {!solo && phase === "running" && <Button small onClick={() => void pause.run()} disabled={pause.pending}>Pause at next handoff</Button>}
            {!solo && phase === "paused" && !pending && <Button small onClick={() => void resume.run()}>Resume</Button>}
            <Button small variant={showChanges ? "primary" : "secondary"} onClick={() => setShowChanges((v) => !v)} aria-pressed={showChanges}>Changes</Button>
          </span>
        </div>
        <ErrorText>{pause.error ?? resume.error}</ErrorText>
      </header>

      <div className="flex-1 space-y-4 overflow-y-auto py-4">
        {showChanges && (
          <Card className="p-4">
            <Changes workspaceId={workspaceId} runId={runId} branch={r.branch} state={phase} version={live.version} onChanged={() => { run.reload(); setShowChanges(true); }} />
          </Card>
        )}
        <Thread sessions={sessions} roles={roles} solo={solo} working={working} />
        {optimistic.map((m) => (
          <div key={m.at} className="flex justify-end">
            <div className="max-w-[85%] rounded-2xl rounded-br-sm bg-accent-soft px-3.5 py-2 opacity-70">{m.text}</div>
          </div>
        ))}
        {transcripts.error && <ErrorText>{transcripts.error}</ErrorText>}
        {pending && <PendingHandoff runId={runId} roles={team?.roles ?? []} from={fromName(pending.from)} to={pending.to} context={pending.context} />}
        {(phase === "blocked" || live.blocked) && phase !== "done" && <Blocked runId={runId} workspaceId={workspaceId} reason={live.blocked} />}
        {live.finished === "vetoed" && <Banner tone="accent">You ended this run at a handoff. Everything before it is kept.</Banner>}
      </div>

      {(phase === "done" || phase === "running") && (
        <FollowUp runId={runId} disabled={phase !== "done"} onSent={(text) => setSent((s) => [...s, { text, at: Date.now() }])} />
      )}
    </div>
  );
}
