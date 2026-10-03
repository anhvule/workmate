export const ago = (ms: number, now = Date.now()): string => {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 45) return "just now";
  if (s < 3600) return `${Math.round(s / 60)}m ago`;
  if (s < 86_400) return `${Math.round(s / 3600)}h ago`;
  return `${Math.round(s / 86_400)}d ago`;
};

export const inFuture = (ms: number, now = Date.now()): string => {
  const s = Math.max(0, Math.round((ms - now) / 1000));
  if (s < 90) return "in a minute";
  if (s < 3600) return `in ${Math.round(s / 60)}m`;
  if (s < 86_400) return `in ${Math.round(s / 3600)}h`;
  return `in ${Math.round(s / 86_400)}d`;
};

export const baseName = (p: string): string => p.replace(/\/+$/, "").split("/").pop() ?? p;

/** A stable hue for a name, so a role looks the same everywhere it appears. */
export const hueOf = (name: string): number => {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 360;
  return h;
};

/** `plan-build-review.planner` -> `planner`. Role ids are namespaced by pack. */
export const bareRole = (id: string): string => id.split(".").pop() ?? id;
