import { useState } from "react";
import { api, type Scope } from "../lib/api.js";
import { useAction } from "../lib/hooks.js";
import { providerName } from "../lib/providers.js";
import { Button, ErrorText, Field, Input } from "./ui.js";

/**
 * Add a provider key. The key goes to the OS keychain and is never shown again:
 * nothing in workmate returns a secret to the webview, so there is no reveal.
 */
export function KeyForm({
  provider,
  scope = { kind: "global" },
  onSaved,
  cta = "Save key",
}: {
  provider: string;
  scope?: Scope;
  onSaved: () => void;
  cta?: string;
}): React.JSX.Element {
  const [secret, setSecret] = useState("");
  const save = useAction(async () => {
    await api.credentials.set(scope, provider, secret);
    setSecret("");
    onSaved();
  });
  return (
    <form
      className="space-y-2"
      onSubmit={(e) => {
        e.preventDefault();
        void save.run();
      }}
    >
      <Field label={`${providerName(provider)} API key`} hint="Stored in your system keychain. It is never displayed again.">
        <Input
          type="password"
          autoComplete="off"
          spellCheck={false}
          value={secret}
          onChange={(e) => setSecret(e.target.value)}
          placeholder="Paste your key"
        />
      </Field>
      <ErrorText>{save.error}</ErrorText>
      <Button type="submit" variant="primary" disabled={save.pending || secret.trim() === ""}>
        {cta}
      </Button>
    </form>
  );
}
