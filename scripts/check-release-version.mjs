#!/usr/bin/env node
/**
 * A tag must name the version actually inside the app, or the .dmg and the
 * release disagree about what was shipped.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const tag = process.argv[2] ?? "";
const read = (p) => JSON.parse(readFileSync(join(ROOT, p), "utf8")).version;
const cargo = /^version\s*=\s*"([^"]+)"/m.exec(readFileSync(join(ROOT, "apps/desktop/src-tauri/Cargo.toml"), "utf8"))?.[1];
const versions = { tag: tag.replace(/^v/, ""), tauri: read("apps/desktop/src-tauri/tauri.conf.json"), cargo, package: read("apps/desktop/package.json") };

const distinct = new Set(Object.values(versions));
if (distinct.size !== 1 || !/^\d+\.\d+\.\d+/.test(versions.tag)) {
  console.error("version mismatch:", versions);
  process.exit(1);
}
console.log(`releasing ${versions.tag}`);
