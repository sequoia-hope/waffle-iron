You are running as part of auto-waffle. The previous work session was
interrupted because it exceeded the time limit.

Re-read the governance model. Review all uncommitted changes against it.
Be honest — does the work follow the Engineering Constitution and
Definition of Done?

For each logical unit of uncommitted work, decide:
- Commit it if it's governance-compliant, tests pass, and it's a clean
  intermediate step or complete feature
- Revert it if it's incomplete, breaks tests, or violates governance

Run cargo test and cargo clippy for any affected crates before deciding.

Leave the repo clean — all tests passing, no uncommitted changes.
Do NOT push to remote. Do NOT start new work or finish incomplete work.

Write a summary of your decisions to the file path in
AUTO_WAFFLE_CLEANUP_PATH.
