#!/usr/bin/env node
/**
 * Build the sidecar into a single binary beside the engine.
 *
 * Bun's `--compile` produces a self-contained executable, so the user needs
 * nothing installed — the same promise the bundled engine makes (ticket 025).
 *
 * `@yao-pkg/pkg` was tried first as the recorded fallback and rejected in
 * practice: it has no prebuilt runtime for node20-macos-arm64 and silently
 * falls back to **building Node and V8 from source**, which is 30-90 minutes on
 * every build machine. Bun compiles this in seconds.
 */
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT_DIR = join(ROOT, "apps/desktop/src-tauri/binaries");
const ENTRY = join(ROOT, "packages/sidecar/src/bin.ts");
const BINARY = join(
  OUT_DIR,
  process.platform === "win32" ? "workmate-sidecar.exe" : "workmate-sidecar",
);

function bun() {
  const candidates = [
    join(process.env.HOME ?? "", ".bun/bin/bun"),
    "bun",
  ];
  for (const c of candidates) {
    if (spawnSync(c, ["--version"], { stdio: "ignore" }).status === 0) return c;
  }
  console.error(
    "bun not found. Install it from https://bun.sh — the sidecar is compiled with `bun build --compile`.",
  );
  process.exit(1);
}

mkdirSync(OUT_DIR, { recursive: true });

const r = spawnSync(
  bun(),
  ["build", ENTRY, "--compile", "--target", "bun", "--outfile", BINARY],
  { cwd: ROOT, stdio: "inherit" },
);
if (r.status !== 0) {
  console.error("sidecar compile failed");
  process.exit(1);
}
if (!existsSync(BINARY)) {
  console.error(`compile reported success but ${BINARY} is missing`);
  process.exit(1);
}
chmodSync(BINARY, 0o755);
console.log(`sidecar -> ${BINARY}`);
