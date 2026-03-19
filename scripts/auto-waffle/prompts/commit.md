You are running as part of auto-waffle. The previous work session completed
normally. Your job is to verify the work, commit it, and prepare for the
next iteration.

## Step 1 — Verify

Run:
- cargo test -p kernel
- cargo clippy -p kernel
- cargo fmt -p kernel -- --check

## Step 2 — Review

Quickly verify:
- All new code has corresponding tests
- PLAN.md has been updated
- No uncommitted files that should be tracked
- No files outside crates/kernel/ were modified (except specs/ and PLAN.md)

## Step 3 — Commit

If everything passes:
- Stage all relevant changes
- Commit with a descriptive message following the repo's commit style
- Push to remote

If something fails:
- Diagnose which changes are broken
- Commit the passing portions separately
- Revert the broken portions
- Update PLAN.md with what went wrong

## Step 4 — Write Summary

Write a brief summary to the file path in AUTO_WAFFLE_COMMIT_PATH:
- What was committed
- Commit hash(es)
- What task to pick up next (if obvious from PLAN.md)
