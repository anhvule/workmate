---
id: 021
title: Packaging, signing and release
type: task
mode: AFK
status: closed
assignee:
blocked-by: [014]
---

## Question

Ship a .dmg a stranger can install.

- Code signing and notarization for macOS arm64 and x64.
- CI matrix that fetches the pinned engine binary per target; the engine cannot
  be cross-compiled, so each target builds on its own runner.
- Installer size: the bundled engine is ~138MB — confirm that is acceptable or
  decide on a download-on-first-run fallback.
- Update channel, or a deliberate decision to ship without one.
- Verify the MIT notice for the bundled engine ships inside the app bundle, not
  only in the repo.

## Resolution

**Built and run for real:** `tauri build` produces `workmate.app` and a `.dmg`;
the built app was **launched**, its webview ran the boot code, and it started the
bundled engine and sidecar from inside the bundle with a private engine home and a
migrated (v6) database. **Not exercised here: signing, notarization and the CI
workflows** — there are no certificates and no runners in this session. They are
written (`.github/workflows`, `scripts/`, `docs/RELEASING.md`) and the pieces that
can run locally do, but the first tag will be their first real run.

**Found by actually packaging and launching, and fixed:**

- Resource globs *flatten* the tree — every `pack.json` landed on top of the
  others. Whole directories are mapped now, and `verify-bundle.sh` checks the
  structure of what shipped rather than trusting the config.
- **The engine outlived the app.** Shutdown was wired to the window being
  destroyed, which a macOS Cmd-Q never does. It is now on the app's
  `ExitRequested`/`Exit`. And because a crash or force-quit runs neither, the
  engine's pid is recorded and the next launch reaps a stale engine (checking the
  pid is still an `opencode` before killing it). Both tested; the quit was
  re-verified on the built app.

**Size: acceptable, no download-on-first-run.** 213 MB installed (engine 138 MB,
sidecar 59 MB); the `.dmg` is **74 MB**. A first-run download would add a network
dependency, a failure state and a trust question to the one moment the app has to
work, to save a download the user is already making.

**Signing.** Tauri signs the app, not nested executables, and notarization rejects
unsigned Mach-O files, so `sign-binaries.sh` signs the engine and sidecar first
with the hardened runtime and a minimal `entitlements.plist` (JIT and unsigned
executable memory, for the two JavaScript runtimes; no sandbox). Without secrets
the workflow builds unsigned and says so.

**CI.** One runner per architecture (`macos-14` arm64, `macos-13` x64), because the
engine cannot be cross-compiled: each fetches its own pinned binary, runs the same
`pnpm gate` as the pre-commit hook, then the engine end-to-end test. The release
job checks the tag against the three version fields, builds, verifies the bundle,
and leaves a *draft* release.

**Notices.** `NOTICE` and `LICENSE` are bundled inside `Resources/` and the
verifier fails if the notice does not mention the engine.

**Update channel: none in v1, deliberately.** It needs a signing key whose loss is
unrecoverable, hosting, and a manifest process; none exist and there are no users
to update yet. Recorded in `docs/RELEASING.md` with the condition for revisiting.

**Out of scope here:** Windows and Linux (the map's cross-platform item); the
binary matrix is decided, what remains is runners and platform-specific keychain
and path handling.
