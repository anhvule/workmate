import { useState } from "react";
import { api } from "../lib/api.js";
import { useAction, useLoad } from "../lib/hooks.js";
import { COMMON_PROVIDERS } from "../lib/providers.js";
import { Button, ErrorText, Field, Input, Select } from "./ui.js";

/**
 * Choose the default provider and model. The model list comes from the engine
 * when it is running; otherwise the model is typed, because a stale list baked
 * into the app would be wrong the week after it shipped.
 */
export function ModelPicker({ onSaved }: { onSaved: () => void }): React.JSX.Element {
  const known = useLoad(() => api.models.list().catch(() => []), []);
  const current = useLoad(() => api.models.getDefault(), []);
  const [provider, setProvider] = useState<string | null>(null);
  const [model, setModel] = useState<string | null>(null);
  const p = provider ?? current.data?.provider ?? "";
  const m = model ?? current.data?.model ?? "";
  const engineModels = known.data?.find((x) => x.id === p)?.models ?? [];
  const providers = [...new Map([...COMMON_PROVIDERS, ...(known.data ?? [])].map((x) => [x.id, x.name])).entries()];

  const save = useAction(async () => {
    await api.models.setDefault(p, m);
    onSaved();
  });
  return (
    <form
      className="grid gap-3 sm:grid-cols-[1fr_1fr_auto] sm:items-end"
      onSubmit={(e) => {
        e.preventDefault();
        void save.run();
      }}
    >
      <Field label="Provider">
        <Select value={p} onChange={(e) => { setProvider(e.target.value); setModel(""); }}>
          <option value="">Choose…</option>
          {providers.map(([id, name]) => <option key={id} value={id}>{name}</option>)}
        </Select>
      </Field>
      <Field label="Model">
        {engineModels.length > 0 ? (
          <Select value={m} onChange={(e) => setModel(e.target.value)}>
            <option value="">Choose…</option>
            {engineModels.map((x) => <option key={x.id} value={x.id}>{x.name}</option>)}
          </Select>
        ) : (
          <Input value={m} onChange={(e) => setModel(e.target.value)} placeholder="model id" spellCheck={false} />
        )}
      </Field>
      <Button type="submit" variant="primary" disabled={!p || !m || save.pending}>Use this model</Button>
      <div className="sm:col-span-3"><ErrorText>{save.error}</ErrorText></div>
    </form>
  );
}
