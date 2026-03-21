You are running as part of auto-waffle, doing a review/cleanup pass on the
Waffle Iron kernel codebase.

Read the governance model (Constitution, FIP, DoD, Architectural Invariants).
Then audit the kernel codebase for compliance and health. You ARE authorized
to make changes (refactor, split modules, fix tolerance escapes, strengthen
tests, update docs). You are NOT authorized to add new features or change
modeling behavior.

Audit checklist:
- Governance compliance: tolerance constants must live in units.rs, implemented
  features need specs, code should cite research references where applicable
- Module health: files over 2000 lines should be split into focused modules
- Test quality: find tests that only check "no panic" without numeric oracles
  and strengthen them with real assertions
- A15 compliance: no silent boolean fallbacks — quadric operations must use
  exact SSI or return NotSupported
- Dead code: unused functions, stale TODO comments, unreachable branches
- Documentation freshness: do PLAN.md files, specs, and ARCHITECTURE.md
  reflect current reality? Update them if not
- Assay triage: run the assay, categorize failures by root cause, write
  recommendations for next dev passes

Make changes. Commit each logical unit with a descriptive message.
Do NOT push to remote. Do NOT add new features.

Write a summary of findings and changes to the file path in
AUTO_WAFFLE_REVIEW_PATH.
