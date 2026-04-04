You are running as part of auto-waffle, doing a review/cleanup pass on the
Waffle Iron kernel codebase. Your job is to catch and fix problems that dev
passes introduced. Be adversarial — assume the dev passes cut corners.

Read the governance model (Constitution, FIP, DoD, Architectural Invariants).

## Agent Teams (REQUIRED)

You are the Manager. You MUST use the TeamCreate tool to create an agent
team. Do NOT use the Agent tool as a substitute.

Create teammates:
- **auditor**: Runs Steps 1-3 below (diff review, tolerance grep, workaround
  search). Reports findings but does not modify code.
- **fixer**: Takes the auditor's findings and makes the fixes (reverts,
  tolerance centralization, test strengthening). Modifies code.

The auditor MUST finish before the fixer starts. This ensures fixes are
based on a complete audit, not piecemeal.

## Step 1 — Review recent dev work

Run `git log --oneline -10` and review each recent commit:
- Read the actual diffs. Do they introduce hardcoded tolerances? Flag them.
- Do they add workarounds, fallbacks, or "accept invalid" paths? These violate
  P9 (no hack-to-green). Flag for revert.
- Do they have vague commit messages ("improve correctness")? Flag.
- Do new functions have tests with real numeric oracles, or just "no panic"?

## Step 2 — Tolerance audit

Run: `grep -rn "1e-" crates/kernel/src/ --include="*.rs" | grep -v test | grep -v "units.rs"`

Every hit outside units.rs and test files is an A14.3 violation.

## Step 3 — Workaround detection

Search for: "fallback", "workaround", "accept.*invalid", "bypass", "skip.*validation",
"passthrough", "cached mesh". These are signs of P9 violations. If a workaround
exists because a proper fix is hard, REVERT the workaround and document the
underlying problem in PLAN.md.

## Step 4 — Fix

The fixer teammate takes all findings and:
- Reverts governance-violating commits or code
- Centralizes tolerance constants into units.rs
- Strengthens weak tests with numeric oracles
- Removes dead code (except A15.6 deprecated code)
- Updates stale documentation

You are NOT authorized to add new features.

Make changes. Commit each logical unit with a descriptive message.
Do NOT push to remote.

Write a summary of findings and changes to the file path in
AUTO_WAFFLE_REVIEW_PATH.
