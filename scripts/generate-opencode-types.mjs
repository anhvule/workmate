#!/usr/bin/env node
/**
 * Regenerate the OpenCode client types from the pinned engine's own OpenAPI
 * document, and fail if the committed output is stale.
 *
 * This is the mechanism that catches a contract change at upgrade time rather
 * than at runtime. OpenCode's published types are mid-migration in at least one
 * place — the prompt body's `tools` field is deprecated in favour of session
 * permissions — and a silent drift there costs a rewrite (ticket 008).
 *
 *   node scripts/generate-opencode-types.mjs           # regenerate
 *   node scripts/generate-opencode-types.mjs --check    # fail if stale (CI)
 */
import { spawn, spawnSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const BIN = join(ROOT, "apps/desktop/src-tauri/binaries/opencode");
const PKG = join(ROOT, "packages/opencode-client");
const DOC = join(PKG, "openapi.json");
const OUT = join(PKG, "src/schema.d.ts");

const check = process.argv.includes("--check");

/** Wait for the engine to announce its URL; readiness is the line, not the spawn. */
function waitForUrl(child, timeoutMs = 30_000) {
  return new Promise((resolve, reject) => {
    let buf = "";
    const timer = setTimeout(() => reject(new Error("engine did not announce a URL")), timeoutMs);
    const onData = (chunk) => {
      buf += chunk.toString();
      const m = /(http:\/\/127\.0\.0\.1:\d+)/.exec(buf);
      if (m) {
        clearTimeout(timer);
        resolve(m[1]);
      }
    };
    child.stdout.on("data", onData);
    child.stderr.on("data", onData);
    child.once("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`engine exited with ${code} before announcing a URL`));
    });
  });
}

async function fetchDoc() {
  const child = spawn(BIN, ["serve", "--hostname", "127.0.0.1", "--pure"], {
    stdio: ["ignore", "pipe", "pipe"],
  });
  try {
    const url = await waitForUrl(child);
    const res = await fetch(`${url}/doc`);
    if (!res.ok) throw new Error(`/doc returned ${res.status}`);
    return JSON.stringify(await res.json(), null, 2) + "\n";
  } finally {
    child.kill("SIGTERM");
  }
}

const before = check ? await readFile(OUT, "utf8").catch(() => "") : "";

const doc = await fetchDoc();
await writeFile(DOC, doc);

const gen = spawnSync(
  "pnpm",
  ["--filter", "@workmate/opencode-client", "exec", "openapi-typescript", "openapi.json", "-o", "src/schema.d.ts"],
  { cwd: ROOT, stdio: check ? "ignore" : "inherit" },
);
if (gen.status !== 0) {
  console.error("type generation failed");
  process.exit(1);
}

if (check) {
  const after = await readFile(OUT, "utf8");
  if (before !== after) {
    console.error(
      "opencode client types are stale.\n" +
        "The pinned engine's contract no longer matches the committed types.\n" +
        "Run: node scripts/generate-opencode-types.mjs — then read the diff before committing it.",
    );
    process.exit(1);
  }
  console.log("opencode client types are current");
}
