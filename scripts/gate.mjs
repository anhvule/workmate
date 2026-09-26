#!/usr/bin/env node
/**
 * The gate: typecheck, lint and test across TypeScript and Rust.
 *
 * One command so the pre-commit hook, CI and a developer at a terminal all run
 * exactly the same checks — a gate that differs between them is a gate that
 * gets argued with.
 */
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const TAURI = join(ROOT, "apps/desktop/src-tauri");
const cargo = join(process.env.HOME ?? "", ".cargo/bin/cargo");
const hasCargo = existsSync(cargo) || spawnSync("cargo", ["--version"]).status === 0;

const ENGINE = join(ROOT, "apps/desktop/src-tauri/binaries/opencode");

const steps = [
  ["typecheck", "pnpm", ["-r", "run", "typecheck"], ROOT],
  ["lint", "pnpm", ["exec", "eslint", "."], ROOT],
  ["test", "pnpm", ["exec", "vitest", "run"], ROOT],
];

// Contract drift: regenerating the client types from the pinned engine must be
// a no-op. Skipped when the engine has not been fetched, since it needs the
// real binary to serve its own OpenAPI document.
if (existsSync(ENGINE)) {
  steps.push([
    "opencode contract",
    "node",
    [join(ROOT, "scripts/generate-opencode-types.mjs"), "--check"],
    ROOT,
  ]);
} else {
  console.warn("! engine binary absent — skipping the contract check. Run `pnpm sidecar:fetch`.");
}

if (hasCargo) {
  steps.push(
    ["clippy", cargo, ["clippy", "--all-targets", "--", "-D", "warnings"], TAURI],
    ["cargo test", cargo, ["test"], TAURI],
  );
} else {
  console.warn("! cargo not found — skipping Rust checks. Install rustup to run the full gate.");
}

let failed = 0;
for (const [name, cmd, args, cwd] of steps) {
  process.stdout.write(`\n── ${name} ──\n`);
  const r = spawnSync(cmd, args, { cwd, stdio: "inherit", env: process.env });
  if (r.status !== 0) {
    failed += 1;
    console.error(`✗ ${name} failed`);
  }
}

if (failed > 0) {
  console.error(`\n${failed} gate step(s) failed.`);
  process.exit(1);
}
console.log("\n✓ gate green");
