---
id: 021
title: Packaging, signing and release
type: task
mode: AFK
status: open
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
