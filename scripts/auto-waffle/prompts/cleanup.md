You are running as part of auto-waffle. The previous work session was
interrupted because it exceeded the time limit. Your job is to review all
uncommitted changes, decide what to keep and what to revert, and leave the
repository in a clean, governance-compliant state.

## Step 1 — Re-read Governance (MANDATORY)

Read these files first. This is critical — the previous session may have lost
context on the governance model:

- /governance/ENGINEERING_CONSTITUTION.md
- /governance/DEFINITION_OF_DONE.md
- /governance/ARCHITECTURAL_INVARIANTS.md

## Step 2 — Assess the Damage

Run:
- git status
- git diff
- git diff --cached
- cargo test -p kernel
- cargo clippy -p kernel

## Step 3 — Review Each Change

For each logical unit of uncommitted work, answer honestly:

1. **Does it have a spec?** If it adds modeling behavior, /specs/<feature>.md
   must exist. If not, the work is incomplete.
2. **Do tests exist and pass?** If tests were written, do they pass? Are they
   meaningful (numeric/structural oracles, not just "no panic")? Do they
   cover all branches?
3. **Does the implementation follow governance?**
   - No hack-to-green (P9)
   - No undocumented branches
   - Research references cited (P8)
   - Analytical primacy maintained (A15)
   - Architecture boundaries intact (A1-A6)
   - Tolerances from units.rs (A14)
4. **Is it complete?** A half-implemented feature is worse than no feature.
   Partial work that compiles and passes tests MAY be kept if it's a clean
   intermediate step (e.g., type definitions without logic). Partial work
   that breaks tests MUST be reverted.

## Step 4 — Commit or Revert

For changes that pass review:
- Stage and commit with a clear message explaining what was accomplished
- Include "auto-waffle: timeout recovery" in the commit message

For changes that fail review:
- Revert with: git checkout -- <file>
- Explain in your response WHY each reverted file was reverted

## Step 5 — Update PLAN.md

Update projects/01-kernel-fork/PLAN.md:
- Mark any completed tasks
- Note partial progress on interrupted tasks
- Add discovered blockers or subtasks

## Step 6 — Write Summary

Write a summary of your decisions to the file path in
AUTO_WAFFLE_CLEANUP_PATH. Include:
- What was kept and why
- What was reverted and why
- Current state of the task that was interrupted
- Recommendations for the next iteration

## Rules

- Be honest. If the work is sloppy, say so and revert it.
- Do not finish incomplete work in this session. Only assess and clean up.
- Do not start new work.
- Leave the repo with all tests passing and no uncommitted changes.
