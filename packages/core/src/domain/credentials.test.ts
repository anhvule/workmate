import { describe, expect, it } from "vitest";
import { providerId, roleId, workspaceId } from "./ids.js";
import { forRole, forWorkspace, GLOBAL } from "./scope.js";
import {
  credentialAccount,
  resolveCredential,
  type StoredCredential,
} from "./credentials.js";

const anthropic = providerId("anthropic");
const reviewer = roleId("reviewer");
const ws = workspaceId("ws-1");

describe("credentialAccount", () => {
  it("is versioned, so a future format change can find entries to rewrite", () => {
    expect(credentialAccount(GLOBAL, anthropic)).toBe("v1:global:anthropic");
  });

  it("encodes the scope, so one provider can hold several keys", () => {
    expect(credentialAccount(forRole(reviewer), anthropic)).toBe("v1:role:reviewer:anthropic");
    expect(credentialAccount(forWorkspace(ws), anthropic)).toBe("v1:workspace:ws-1:anthropic");
  });
});

describe("resolveCredential", () => {
  const global: StoredCredential = { scope: GLOBAL, provider: anthropic };
  const roleScoped: StoredCredential = { scope: forRole(reviewer), provider: anthropic };
  const wsScoped: StoredCredential = { scope: forWorkspace(ws), provider: anthropic };

  it("prefers the role's own key over the workspace's and the global one", () => {
    const r = resolveCredential([global, wsScoped, roleScoped], anthropic, {
      role: reviewer,
      workspace: ws,
    });
    expect(r).toMatchObject({ found: true, account: "v1:role:reviewer:anthropic" });
  });

  it("falls back to the workspace when the role has no key of its own", () => {
    const r = resolveCredential([global, wsScoped], anthropic, { role: reviewer, workspace: ws });
    expect(r).toMatchObject({ found: true, account: "v1:workspace:ws-1:anthropic" });
  });

  it("falls back to global last", () => {
    const r = resolveCredential([global], anthropic, { role: reviewer, workspace: ws });
    expect(r).toMatchObject({ found: true, scope: GLOBAL });
  });

  it("does not leak another provider's key", () => {
    const openai: StoredCredential = { scope: GLOBAL, provider: providerId("openai") };
    expect(resolveCredential([openai], anthropic, {}).found).toBe(false);
  });

  it("reports what it tried instead of throwing, so the run can pause and ask", () => {
    const r = resolveCredential([], anthropic, { role: reviewer, workspace: ws });
    expect(r.found).toBe(false);
    if (!r.found) {
      expect(r.tried).toEqual([
        "v1:role:reviewer:anthropic",
        "v1:workspace:ws-1:anthropic",
        "v1:global:anthropic",
      ]);
    }
  });
});
