# Contributing to Waffle Iron

## Setup

Enable the versioned git hooks once per clone:

```bash
./scripts/setup-hooks.sh
```

The Rust toolchain version is pinned in `rust-toolchain.toml`; `rustup`
installs it on first use.

## Before every commit

Run these for each crate you changed:

```bash
cargo test -p <crate>
cargo clippy --all-targets -p <crate> -- -D warnings
cargo fmt --check -p <crate>
```

`--all-targets` is required: CI lints test code too.

If the change affects what ships to the browser (any crate `wasm-bridge`
depends on), rebuild the WASM bundle and commit it with the Rust change:

```bash
./scripts/build-wasm.sh
```

The pre-commit hook blocks two things:

- a commit whose bundle in `app/static/pkg/` does not match the sources;
- a commit that changes `yang-rs` or the boolean side of `kernel-v2` without
  also recording it in `docs/yang_deviations.md` (or a "faithful, no
  deviation" line in `docs/yang_tail_triage.md`).

## Test tiers

`./scripts/test.sh <tier>` runs a whole tier. Pick the smallest one that covers
your change.

| Tier | Covers |
|---|---|
| `rewrite` | The kernel-stack crates (about 8 minutes) |
| `fast` | `rewrite` plus the engine crates and the fast `test-harness` binaries |
| `full` | Every Rust test, including the complete `test-harness` suite (about 27 minutes) |
| `gui-fast` | GUI smoke tests (Playwright) |
| `gui-full` | Every GUI spec |
| `all-fast` | `fast` + `gui-fast` + the agent-link relay tiers |
| `all` | `full` + `gui-full` + relay — run before merging |

All tiers are green. A red test is a regression. `docs/TESTING.md` describes
each tier, how to add a test to one, and how to run the boolean assay.

## Pushing to main

The pre-push hook refuses a push to `main` when the latest completed CI run on
`main` is not green, or when the pushed commits change a kernel-stack crate
and no passing `./scripts/test.sh rewrite` (or `fast` / `full`) is recorded
against exactly that kernel content. A passing run records itself. Never push
on top of a red or timed-out `main`.

## Rules

- **Determinism.** The same input always produces the same output. No random
  values, no system time in logic, no filesystem side effects in tests.
- **Tests are permanent.** Never delete a passing test. If a test is wrong, fix
  it.
- **Fix it right or don't fix it.** If you cannot explain why a test fails, do
  not change code to make it pass: no tolerance widening, no special-case
  branches, no fallback paths. Write down what you found instead.
- **Stay inside your crate.** Depend on another crate's public contract, never
  its internals. The kernel is reached only through the traits in
  `waffle_types::kernel`.
- **Interface changes** touch every consumer. Propose a change to a
  cross-crate contract in your sub-project's `PLAN.md` under "Interface Change
  Requests"; it needs review, and it must update every consuming crate and
  `INTERFACES.md` together.
- **Other people's branches are theirs.** Do not commit to, merge, rebase or
  edit a branch you did not create.

## Commits

One logical change per commit. The first line is an imperative summary of
about 50 characters; the body explains why, not what.

## Documentation

Documentation describes how the system works now. When you change behaviour,
update the document that describes it in the same commit. Sub-projects with a
dossier under `projects/` track their tasks in its `PLAN.md`; kernel-stack
work is tracked in `docs/yang_faithfulness_roadmap.md`.
