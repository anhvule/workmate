# Releasing workmate

A release is a tag. Everything else is automated in
`.github/workflows/release.yml`, one job per architecture, because the bundled
engine cannot be cross-compiled.

```bash
# 1. Bump the version in all three places (the workflow refuses a mismatch):
#    apps/desktop/package.json, apps/desktop/src-tauri/Cargo.toml,
#    apps/desktop/src-tauri/tauri.conf.json
# 2. Commit, then:
git tag v0.1.0 && git push origin v0.1.0
```

The workflow builds for Apple silicon and Intel, runs the full gate and the
engine end-to-end test, signs, notarizes, and leaves a **draft** GitHub release
with both `.dmg` files. Nothing is published until you publish the draft.

## Secrets (repository settings → Secrets → Actions)

| Secret | What it is |
|---|---|
| `APPLE_CERTIFICATE` | A "Developer ID Application" certificate exported as `.p12`, base64-encoded (`base64 -i cert.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | The password you set on that export |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | The Apple ID used for notarization |
| `APPLE_APP_SPECIFIC_PASSWORD` | An app-specific password for that Apple ID |
| `APPLE_TEAM_ID` | Your 10-character team id |

With none of these set the workflow still runs and produces an **unsigned**
build, which macOS Gatekeeper will refuse to open normally. That is useful for
checking the pipeline; it is not a release.

## Why the executables are signed separately

Tauri signs the app bundle, not the executables inside `Resources/`. Notarization
rejects any unsigned Mach-O file, and the bundled engine (`opencode`) and sidecar
(`workmate-sidecar`) are exactly that. `scripts/sign-binaries.sh` signs them first
with the hardened runtime and `entitlements.plist`. Both are JavaScript runtimes
compiled to a binary, so they need `allow-jit` and `allow-unsigned-executable-memory`;
nothing else is requested, and the App Sandbox is off because the engine runs git
and the user's shell by design.

## What `scripts/verify-bundle.sh` checks

It inspects the built `.app`, not the config: the engine and sidecar are inside,
the shipped packs and skills kept their directory structure, the `NOTICE` and
`LICENSE` ship with the app (the engine is MIT, which requires its notice to
travel with the binary), and, when signing, that the signature verifies and
Gatekeeper accepts it.

## Updates

There is no auto-updater in v1. An updater needs a signing key whose loss or
leak is unrecoverable, a hosting endpoint, and an update-manifest process, and
none of those exist yet. Updates are a new download. Revisit when there are users
to update.

## Moving the engine

The engine version is pinned exactly in `scripts/fetch-sidecar-binary.mjs`. To
move it: change the pin, run `pnpm opencode:types` to regenerate the client types,
read the diff (the gate fails on an undeclared contract change), run
`pnpm test:e2e`, and ship it as a workmate release. The engine moves only when
workmate ships.
