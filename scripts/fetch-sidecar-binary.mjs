#!/usr/bin/env node
/**
 * Fetch the pinned OpenCode binary for this host into the Tauri bundle.
 *
 * Workmate bundles the engine rather than asking the user to `npm i -g
 * opencode-ai` first, so the app works on first launch (ticket 008). OpenCode
 * publishes a prebuilt binary per platform as its own npm package under MIT,
 * which is what makes this cheap.
 *
 * The version is pinned exactly and asserted here: an engine that drifts under
 * a recorded session produces bugs workmate cannot reproduce.
 */
import { createWriteStream } from "node:fs";
import { mkdir, mkdtemp, rm, chmod, rename, access } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";
import { spawnSync } from "node:child_process";
import { Readable } from "node:stream";

export const PINNED_ENGINE_VERSION = "1.18.32";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT_DIR = join(ROOT, "apps/desktop/src-tauri/binaries");

/** Maps a Node platform/arch pair onto OpenCode's published package name. */
export function packageForHost(platform, arch) {
  const table = {
    "darwin:arm64": "opencode-darwin-arm64",
    "darwin:x64": "opencode-darwin-x64",
    "linux:arm64": "opencode-linux-arm64",
    "linux:x64": "opencode-linux-x64",
    "win32:x64": "opencode-windows-x64",
    "win32:arm64": "opencode-windows-arm64",
  };
  const name = table[`${platform}:${arch}`];
  if (!name) {
    throw new Error(
      `no pinned OpenCode binary for ${platform}/${arch}; supported: ${Object.keys(table).join(", ")}`,
    );
  }
  return name;
}

async function main() {
  const pkg = packageForHost(process.platform, process.arch);
  const binName = process.platform === "win32" ? "opencode.exe" : "opencode";
  const dest = join(OUT_DIR, binName);

  if (await access(dest).then(() => true, () => false)) {
    console.log(`sidecar already present: ${dest}`);
    return;
  }

  const metaRes = await fetch(`https://registry.npmjs.org/${pkg}/${PINNED_ENGINE_VERSION}`);
  if (!metaRes.ok) {
    throw new Error(`registry rejected ${pkg}@${PINNED_ENGINE_VERSION}: ${metaRes.status}`);
  }
  const meta = await metaRes.json();
  const tarballUrl = meta?.dist?.tarball;
  if (typeof tarballUrl !== "string") throw new Error(`no tarball for ${pkg}`);

  const work = await mkdtemp(join(tmpdir(), "workmate-sidecar-"));
  try {
    const tarPath = join(work, "pkg.tgz");
    const dl = await fetch(tarballUrl);
    if (!dl.ok || !dl.body) throw new Error(`download failed: ${dl.status}`);
    await pipeline(Readable.fromWeb(dl.body), createWriteStream(tarPath));

    const untar = spawnSync("tar", ["-xzf", tarPath, "-C", work], { stdio: "inherit" });
    if (untar.status !== 0) throw new Error("tar extraction failed");

    const found = spawnSync("find", [join(work, "package"), "-type", "f", "-name", binName], {
      encoding: "utf8",
    });
    const src = found.stdout.trim().split("\n").filter(Boolean)[0];
    if (!src) throw new Error(`no ${binName} inside ${pkg}@${PINNED_ENGINE_VERSION}`);

    await mkdir(OUT_DIR, { recursive: true });
    await rename(src, dest);
    await chmod(dest, 0o755);
    console.log(`sidecar: ${pkg}@${PINNED_ENGINE_VERSION} -> ${dest}`);
  } finally {
    await rm(work, { recursive: true, force: true });
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
}
