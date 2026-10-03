/**
 * The one door to Rust.
 *
 * In the real app this is Tauri's `invoke` and `listen`. In a plain browser (the
 * dev preview and the smoke tests) there is no Tauri, so a small in-memory
 * backend stands in — loaded on demand, so it is never run in a real window.
 */
export type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
export type Listen = (name: string, cb: (payload: unknown) => void) => Promise<() => void>;

interface Bridge {
  invoke: Invoke;
  listen: Listen;
  pickFolder: () => Promise<string | null>;
}

let cached: Promise<Bridge> | undefined;

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const load = async (): Promise<Bridge> => {
  if (hasTauri()) {
    const [{ invoke }, { listen }, { open }] = await Promise.all([
      import("@tauri-apps/api/core"),
      import("@tauri-apps/api/event"),
      import("@tauri-apps/plugin-dialog"),
    ]);
    return {
      invoke: (cmd, args) => invoke(cmd, args),
      listen: async (name, cb) => listen(name, (e) => cb(e.payload)),
      pickFolder: async () => {
        const r = await open({ directory: true, multiple: false });
        return typeof r === "string" ? r : null;
      },
    };
  }
  const { mockBridge } = await import("./mock.js");
  return mockBridge();
};

export const bridge = (): Promise<Bridge> => (cached ??= load());
export const invoke: Invoke = async (cmd, args) => (await bridge()).invoke(cmd, args);
export const listen: Listen = async (name, cb) => (await bridge()).listen(name, cb);
export const pickFolder = async (): Promise<string | null> => (await bridge()).pickFolder();
