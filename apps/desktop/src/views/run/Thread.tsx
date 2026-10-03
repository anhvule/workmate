import { useState } from "react";
import { Chip, RoleChip } from "../../components/ui.js";
import { bareRole } from "../../lib/format.js";
import type { PersistedHandoff, PersistedSession, Role, TranscriptMessage } from "../../lib/types.js";

export interface ThreadSession {
  session: PersistedSession;
  messages: TranscriptMessage[];
  /** Text arriving right now, not yet in the engine's transcript. */
  streaming: string[];
  handoff?: PersistedHandoff | undefined;
}

const Prose = ({ text }: { text: string }): React.JSX.Element => (
  <div className="prose-sm whitespace-pre-wrap break-words">
    {text.split(/(`[^`\n]+`)/g).map((part, i) =>
      part.startsWith("`") && part.endsWith("`") && part.length > 2 ? <code key={i}>{part.slice(1, -1)}</code> : <span key={i}>{part}</span>,
    )}
  </div>
);

/**
 * The handoff, made visible: from-role to to-role and the context actually
 * passed. Collapsed by default, because it is the part a user cannot see in any
 * other tool and also the part they do not need to read every time (ticket 005).
 */
export function HandoffCard({ from, to, context }: { from: string; to: string; context: string }): React.JSX.Element {
  const [open, setOpen] = useState(false);
  return (
    <div className="my-4 rounded-lg border border-dashed border-edge bg-panel/60">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-muted"
      >
        <span aria-hidden>{open ? "▾" : "▸"}</span>
        <span>Handed off</span>
        <RoleChip name={from} />
        <span aria-hidden>→</span>
        <RoleChip name={to} />
        <span className="ml-auto">{open ? "hide" : "see what was passed"}</span>
      </button>
      {open && (
        <div className="border-t border-edge px-3 py-2">
          <Prose text={context} />
        </div>
      )}
    </div>
  );
}

function Bubble({ message, solo }: { message: TranscriptMessage; solo: boolean }): React.JSX.Element {
  const user = message.role === "user";
  return (
    <div className={user ? "flex justify-end" : ""}>
      <div
        className={
          user
            ? "max-w-[85%] rounded-2xl rounded-br-sm bg-accent-soft px-3.5 py-2"
            : solo
              ? "max-w-[46rem] py-1"
              : "max-w-[46rem] py-1 pl-1"
        }
      >
        <Prose text={message.text} />
      </div>
    </div>
  );
}

export function Thread({
  sessions,
  roles,
  solo,
  working,
}: {
  sessions: ThreadSession[];
  roles: Map<string, Role>;
  solo: boolean;
  working: boolean;
}): React.JSX.Element {
  const nameOf = (id: string): string => roles.get(id)?.name ?? bareRole(id);
  const last = sessions.at(-1)?.session.id;
  return (
    <div className="space-y-3" aria-live="polite">
      {sessions.map(({ session, messages, streaming, handoff }, i) => {
        // On a handed-off session the first user turn *is* the handoff, already
        // shown as a card; repeating it as a chat bubble would say it twice.
        const shown = handoff ? messages.filter((m, j) => !(j === 0 && m.role === "user")) : messages;
        const prev = i > 0 ? sessions[i - 1]?.session : undefined;
        return (
          <section key={session.id} aria-label={solo ? "Conversation" : nameOf(session.role)} className="space-y-3">
            {!solo && handoff && prev && <HandoffCard from={nameOf(prev.role)} to={nameOf(session.role)} context={handoff.context} />}
            {!solo && (
              <div className="pt-1">
                <RoleChip name={nameOf(session.role)} model={roles.get(session.role)?.modelId ?? null} />
              </div>
            )}
            {shown.map((m, j) => (
              <Bubble key={j} message={m} solo={solo} />
            ))}
            {session.id === last && working && streaming.length > 0 && (
              <div className="max-w-[46rem] py-1 pl-1">
                <Prose text={streaming.join("")} />
                <span className="ml-0.5 inline-block h-3.5 w-1.5 animate-pulse bg-muted align-middle" aria-hidden />
              </div>
            )}
          </section>
        );
      })}
      {working && sessions.at(-1) && sessions.at(-1)!.streaming.length === 0 && (
        <div className="pl-1">
          <Chip tone="accent">working…</Chip>
        </div>
      )}
    </div>
  );
}
