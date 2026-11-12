# Waffle Iron — Development Environment

## Machine setup

- **Rust**: `rust-toolchain.toml` pins the version, with clippy, rustfmt and
  `wasm32-unknown-unknown`. rustup installs it on the first `cargo` in the repo.
- **wasm-pack 0.13.1**: the version the committed `app/static/pkg/` bundle is
  built with (`cargo install wasm-pack --version 0.13.1 --locked`).
- **Node.js** 20 or 22, for the SvelteKit app (`cd app && npm install`).
- **Playwright browsers**: `cd app && npx playwright install chromium firefox webkit`.
- **uv**, for the agent-link relay (`relay/`, a Python project).
- **clang + libclang, cmake**, for the Cherchi reference sidecars. They are
  dev-only parity oracles, built by `scripts/build_sidecars.sh` into
  `$CHERCHI2022_ROOT`, else `~/cherchi2022`.
- **gh**, authenticated. The pre-push hook reads CI status with it.
- **Git hooks**: run `./scripts/setup-hooks.sh` once per clone. It points
  `core.hooksPath` at `.githooks/` (the pre-commit checks and the pre-push
  gate). Without it nothing is checked.

## Running

- **The app**: `./serve.sh` runs the Vite dev server on the port the machine's
  project registry assigns (`proj up waffle-iron`). Playwright starts its own
  dev server for test runs.
- **Agent sessions**: `./launch.sh` (Claude, or `./launch.sh agy`) and
  `./opencode.sh` start agents in tmux in this directory.
- **The MCP relay**: `./scripts/add-waffle-mcp.sh` registers the relay with
  Claude Code, taking its ports from `tailscale serve`. See `relay/README.md`.
- **Rebuilding the WASM bundle** after a Rust change: `./scripts/build-wasm.sh`.

## Testing

`./scripts/test.sh <tier>`; `docs/TESTING.md` defines the tiers. Heavy runs
are booked through `~/.claude/bin/reserve` on shared machines.

## Rules and workflow

- `governance/`: the engineering constitution, the feature implementation
  protocol, the definition of done and the architectural invariants.
- `agents/ORCHESTRATION.md` and `AGENTS.md`: how agent teams work.
- `CLAUDE.md`: the session guide, with current priorities and coding rules.

A session carries no state except what is in git and these documents.
