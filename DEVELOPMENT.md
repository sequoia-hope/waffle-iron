# Waffle Iron — Development Environment

Development runs directly on the host (no container since 2026-11-12;
`HOST-MOVE-CHECKLIST.md` records the move).

## Governance and Workflow

Agent workflow, role separation, and quality gates are defined in:

- `/governance/ENGINEERING_CONSTITUTION.md` — Non-negotiable engineering rules
- `/governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` — Required lifecycle for modeling features
- `/governance/DEFINITION_OF_DONE.md` — Acceptance criteria
- `/governance/ARCHITECTURAL_INVARIANTS.md` — Protected architectural constraints
- `/agents/ORCHESTRATION.md` — How agent teams execute work

See `CLAUDE.md` for session start checklist and `AGENTS.md` for team structure.

## Host Setup

- **Rust** — `rust-toolchain.toml` pins the version, with clippy, rustfmt and
  `wasm32-unknown-unknown`; rustup installs it on the first `cargo` in the repo
- **wasm-pack 0.13.1** — the version the committed `app/static/pkg/` bundles
  are built with (`cargo install wasm-pack --version 0.13.1 --locked`)
- **Node.js** (20 or 22) — for the SvelteKit app
- **Playwright browsers** — `cd app && npx playwright install chromium firefox webkit`
- **clang + libclang, cmake** — for the Cherchi reference sidecars, built by
  `scripts/build_sidecars.sh` into `$CHERCHI2022_ROOT`, else `~/cherchi2022`

## Lifecycle

1. **Agent sessions** run in host tmux: `./launch.sh` (Claude, or `./launch.sh agy`)
   and `./opencode.sh`, following `/agents/ORCHESTRATION.md`.
2. **The GUI dev server** runs from `./serve.sh` on the port the machine's
   project registry assigns (`proj up waffle-iron`). Playwright starts its own
   dev server for test runs.
3. **Human monitors** progress via git log, PLAN.md updates.

## Session Recovery

Each session starts from:
- Documentation (ARCHITECTURE.md, INTERFACES.md, governance docs, sub-project docs)
- Code (current state of the crate)
- Tests (the ratchet — what passes must keep passing)
- PLAN.md (what's done, what's next, what's blocked)

No implicit knowledge is required. No state is carried between sessions except what's in git and documentation files.

## Usage Windows

Development happens in bursts:
- Start the container, run agent sessions for a few hours.
- Stop the container.
- Review progress: `git log --oneline`, read PLAN.md updates.
- Restart.
