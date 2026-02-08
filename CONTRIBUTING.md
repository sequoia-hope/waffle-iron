# Contributing to Waffle Iron

## Branch Rules

- **Never touch other people's branches.** No commits, merges, rebases, or file edits on branches you didn't create.
- Create feature branches for your work. Name them descriptively.
- Keep commits logical and atomic — one conceptual change per commit.

## Code Quality

All code must pass before merging:

```bash
cargo test          # All tests pass
cargo clippy        # No warnings
cargo fmt --check   # Formatted correctly
```

## Determinism Required

All outputs must be deterministic. Same inputs must always produce the same results. This means:

- No random values in tests or production code
- No system time dependencies in logic
- No filesystem side effects in tests
- truck-meshalgo uses deterministic hashing — preserve this

## Interface Changes

Cross-project interfaces are defined in the top-level `INTERFACES.md`. If you need to change an interface:

1. Document the proposed change in your sub-project's `PLAN.md` under "Interface Change Requests"
2. Do NOT modify `INTERFACES.md` directly
3. Interface changes require review and must update all consuming crates

## Tests Are Permanent

The test suite only grows. Passing tests must never be deleted. If a test is wrong, fix the test — don't delete it.

## Sub-Project Boundaries

Each sub-project has its own directory under `projects/`. Agents and contributors work within their assigned sub-project. Cross-project changes go through the integration process.

## Commit Messages

Write clear, descriptive commit messages. First line is a summary (imperative mood, ~50 chars). Body explains why, not what.

## Documentation

Update `PLAN.md` in your sub-project as you complete tasks or discover new ones. Keep architecture docs accurate.
