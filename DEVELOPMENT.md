# Waffle Iron — Development Environment

Docker-based autonomous development environment for agent-driven and human development.

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
2. **Scheduler runs inside** container, processing tasks from QUEUE.md.
3. **Human monitors** progress via git log, PLAN.md updates, and scheduler logs.
4. **Human stops container** when done (`docker compose down`).
5. **Claude never starts or stops containers.** Agents work within their session only.

## Task Scheduler

### QUEUE.md Format

```markdown
# Task Queue

## Pending
- [ ] 01-kernel-fork: Fork truck, set up workspace dependency
- [ ] 01-kernel-fork: Implement higher-level primitive API
- [ ] 02-sketch-solver: Add slvs crate dependency, verify build

## In Progress
- [~] 01-kernel-fork: Kernel trait adapter (agent session active)

## Completed
- [x] Documentation: Top-level ARCHITECTURE.md
```

### Scheduler Operation

Shell script or Rust binary that:

1. Reads QUEUE.md, finds the first `[ ]` (pending) task.
2. Marks it `[~]` (in progress).
3. Identifies the sub-project from the task prefix.
4. Invokes Claude Code:
   ```bash
   claude --dangerously-skip-permissions \
     --system-prompt "$(cat projects/<sub-project>/CLAUDE.md)" \
     --prompt "Complete this task: <task description>"
   ```
5. Monitors for completion (commit detected) or failure (timeout/error).
6. Marks task `[x]` (completed) or `[!]` (failed) with notes.
7. Advances to next task.

### Stuck Detection

- **No git commit in 15 minutes** → kill the agent session.
- Log the session context (last prompt, last output).
- Narrow the task scope (break into sub-tasks in QUEUE.md).
- Restart with the narrower task.

### Session Recovery

Each session starts from:
- Documentation (ARCHITECTURE.md, INTERFACES.md, sub-project docs)
- Code (current state of the crate)
- Tests (the ratchet — what passes must keep passing)
- PLAN.md (what's done, what's next, what's blocked)

No implicit knowledge is required. No state is carried between sessions except what's in git and documentation files.

## Usage Windows

Development happens in bursts:
- Start the container, let the scheduler run for a few hours.
- Stop the container.
- Review progress: `git log --oneline`, read PLAN.md updates.
- Adjust QUEUE.md priorities if needed.
- Restart.

The scheduler picks up exactly where it left off via QUEUE.md + git status.
