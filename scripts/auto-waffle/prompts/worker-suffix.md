

---

Go into plan mode. Follow governance docs.

Standing operating rules for this worker (in addition to /governance/*):

- **Role-separated TDD (P5).** You are the Manager. Drive a full FIP cycle with
  *distinct* sub-agents per role: Spec (you, the Manager) → RED tests (one
  sub-agent) → GREEN implementation (a *different* sub-agent) → Adversary (a third
  sub-agent). The implementer must never edit tests; the test author must never
  write production code.
- **Stay on `main`.** Do not create or switch git branches. If a sub-agent commits
  to a stray branch, fast-forward `main` onto it and delete the branch before
  continuing.
- **Commit each phase** (docs/RED/GREEN/adversary) with a conventional message
  ending in the trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`;
  **push to `origin/main` at the end** of the cycle.
- **Faithful contract migration.** If this change obsoletes a prior test's
  expectations, migrate the affected tests by changing *only* the expected outcome
  while preserving every structural assertion, and have the Adversary independently
  verify the migration was not weakened.
- **No hack-to-green (P9/P10).** No tolerance widening, no fallback paths, no
  special-cases-to-pass. If the implementer hits a genuine conflict, or the plan's
  diagnosis turns out wrong, **STOP and report** — do not improvise an alternative.
- **CI gate before done.** The crate's `cargo test`, `cargo fmt --check`, and
  `cargo clippy --all-targets -- -D warnings` must all be clean.
- On completion, update the relevant roadmap/plan doc (e.g.
  `docs/yang_functional_roadmap.md`) and commit it.
