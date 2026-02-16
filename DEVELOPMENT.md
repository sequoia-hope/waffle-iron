# Waffle Iron — Development Environment

Docker-based development environment for autonomous agent and human development.

## Governance and Workflow

Agent workflow, role separation, and quality gates are defined in:

- `/governance/ENGINEERING_CONSTITUTION.md` — Non-negotiable engineering rules
- `/governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` — Required lifecycle for modeling features
- `/governance/DEFINITION_OF_DONE.md` — Acceptance criteria
- `/governance/ARCHITECTURAL_INVARIANTS.md` — Protected architectural constraints
- `/agents/ORCHESTRATION.md` — How agent teams execute work

See `CLAUDE.md` for session start checklist and `AGENTS.md` for team structure.

## Container Setup

### Base Image

Ubuntu-based container with:

- **Rust toolchain** — latest stable via rustup, plus `wasm32-unknown-unknown` target
- **wasm-pack** — for building Rust to WASM
- **Node.js** (LTS) — for Svelte/SvelteKit/three.js development
- **clang + libclang** — required for the slvs crate's `cc` + `bindgen` build of libslvs
- **cmake** — required for building libslvs from source
- **git** — version control
- **Claude Code CLI** — for autonomous agent sessions

### Dockerfile Outline

```dockerfile
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    curl git build-essential pkg-config \
    clang libclang-dev cmake \
    && rm -rf /var/lib/apt/lists/*

# Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup target add wasm32-unknown-unknown
RUN cargo install wasm-pack

# Node.js
RUN curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - \
    && apt-get install -y nodejs

# Claude Code CLI (installed by user/team)
# RUN npm install -g @anthropic-ai/claude-code

WORKDIR /workspace
```

### Volumes

```yaml
volumes:
  - ./:/workspace                    # Repo from host
  - ~/.gitconfig:/root/.gitconfig:ro # Git credentials (read-only)
  - ~/.ssh:/root/.ssh:ro             # SSH keys (read-only)
```

## Lifecycle

1. **Human starts container** manually (`docker compose up -d`).
2. **Agent sessions** run inside the container, following `/agents/ORCHESTRATION.md`.
3. **Human monitors** progress via git log, PLAN.md updates.
4. **Human stops container** when done (`docker compose down`).
5. **Claude never starts or stops containers.** Agents work within their session only.

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
