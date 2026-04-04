You are running as part of auto-waffle, doing a review/cleanup pass on the
Waffle Iron kernel codebase. Your job is to catch and fix problems that dev
passes introduced. Be adversarial — assume the dev passes cut corners.

Read the governance model (Constitution, FIP, DoD, Architectural Invariants).
Then audit RECENT COMMITS first, then the broader codebase.

## Step 1 — Review recent dev work

Run `git log --oneline -10` and review each recent commit:
- Read the actual diffs. Do they introduce hardcoded tolerances? Revert them
  or replace with units.rs constants.
- Do they add workarounds, fallbacks, or "accept invalid" paths? These violate
  P9 (no hack-to-green). Revert them.
- Do they have vague commit messages ("improve correctness")? Document what
  actually changed.
- Do new functions have tests with real numeric oracles, or just "no panic"?

## Step 2 — Tolerance audit

Run: `grep -rn "1e-" crates/kernel/src/ --include="*.rs" | grep -v test | grep -v "units.rs"`

Every hit outside units.rs and test files is an A14.3 violation. Fix each one:
either move it to units.rs with a documented name, or replace with an existing
TAU_* constant.

## Step 3 — Workaround detection

Search for: "fallback", "workaround", "accept.*invalid", "bypass", "skip.*validation",
"passthrough", "cached mesh". These are signs of P9 violations. If a workaround
exists because a proper fix is hard, REVERT the workaround and document the
underlying problem in PLAN.md. Do not keep hacks that hide failures.

## Step 4 — Standard audit

- A15 compliance: no silent boolean fallbacks
- Test quality: strengthen "no panic" tests with numeric oracles
- Dead code removal (except A15.6 deprecated code)
- Documentation freshness

You ARE authorized to revert commits that violate governance. You ARE authorized
to refactor. You are NOT authorized to add new features.

Make changes. Commit each logical unit with a descriptive message.
Do NOT push to remote.

Write a summary of findings and changes to the file path in
AUTO_WAFFLE_REVIEW_PATH.
