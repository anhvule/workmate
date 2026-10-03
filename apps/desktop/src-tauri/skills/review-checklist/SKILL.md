---
name: review-checklist
description: A checklist for reviewing a code change. Use when asked to review a diff, a branch or a pull request.
---

# Reviewing a change

Work through these in order and report only what you actually found.

1. **Does it do what was asked?** Compare the diff to the stated goal. Name anything missing and anything extra.
2. **Is it correct?** Look for off-by-one errors, unhandled empty or error cases, and changed behaviour at the edges.
3. **Is it tested?** A bug fix needs a test that fails without the fix. New behaviour needs a test of the behaviour, not of the implementation.
4. **Is it safe?** Check input handling, secrets, paths built from user input, and anything that runs a command.
5. **Is it readable?** Names that say what things are, no dead code, comments that explain why.

Separate *must fix* from *would prefer*, cite files and lines, and say plainly when the change is good.
