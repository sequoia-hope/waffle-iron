# Agent Workflow

Docker-based autonomous development workflow for Waffle Iron.

## Container Setup

### Requirements

| Package | Purpose |
|---------|---------|
| Rust toolchain (stable) | Compile Rust crates |
| `wasm32-unknown-unknown` target | Compile to WASM |
| wasm-pack | Build WASM packages |
| Node.js (LTS) | Svelte/SvelteKit/three.js development |
| clang | C compiler for libslvs |
| libclang-dev | bindgen dependency for slvs crate |
| cmake | Build system for libslvs |
| git | Version control |
| Claude Code CLI | Agent sessions |

### Container Build

```bash
docker build -t waffle-iron-dev .
```

### Container Run

```bash
docker run -it \
  -v $(pwd):/workspace \
  -v ~/.gitconfig:/root/.gitconfig:ro \
  -v ~/.ssh:/root/.ssh:ro \
  -e ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY \
  waffle-iron-dev
```

## Task Queue (QUEUE.md)

### Format

```markdown
# Task Queue

## Pending
- [ ] <sub-project>: <task description>

## In Progress
- [~] <sub-project>: <task description> (session started: <timestamp>)

## Completed
- [x] <sub-project>: <task description> (completed: <timestamp>)

## Failed
- [!] <sub-project>: <task description> (failed: <timestamp>, reason: <reason>)
```

### Priority Order

Tasks are ordered by dependency graph:
1. Phase 1: kernel-fork + sketch-solver (parallel)
2. Phase 2: wasm-bridge + 3d-viewport (parallel, after kernel-fork)
3. Phase 3: sketch-ui (after sketch-solver + wasm-bridge + 3d-viewport)
4. Phase 4: feature-engine + modeling-ops (parallel, after kernel-fork)
5. Phase 5: ui-chrome (after sketch-ui + feature-engine + modeling-ops)
6. Phase 6: file-format (after feature-engine)

Within a sub-project, tasks are ordered by milestone number.

## Scheduler Operation

### Script Behavior

```bash
#!/bin/bash
# scheduler.sh — runs inside the container

while true; do
    # 1. Find next pending task
    TASK=$(grep -m1 '^\- \[ \]' QUEUE.md)
    if [ -z "$TASK" ]; then
        echo "No pending tasks. Exiting."
        exit 0
    fi

    # 2. Extract sub-project and description
    SUBPROJECT=$(echo "$TASK" | sed 's/.*\] \(.*\):.*/\1/')
    DESCRIPTION=$(echo "$TASK" | sed 's/.*: //')

    # 3. Mark in-progress
    sed -i "s/$TASK/- [~] ${SUBPROJECT}: ${DESCRIPTION} (started: $(date -Iseconds))/" QUEUE.md

    # 4. Run agent session
    timeout 30m claude --dangerously-skip-permissions \
        --system-prompt "$(cat projects/${SUBPROJECT}/CLAUDE.md)" \
        --prompt "Complete this task: ${DESCRIPTION}. Read your ARCHITECTURE.md and PLAN.md first."

    # 5. Check result
    if git log -1 --since='15 minutes ago' --oneline | grep -q .; then
        # Commit found — task likely completed
        sed -i "s/\[~\] ${SUBPROJECT}: ${DESCRIPTION}.*/[x] ${SUBPROJECT}: ${DESCRIPTION} (completed: $(date -Iseconds))/" QUEUE.md
    else
        # No commit — task failed
        sed -i "s/\[~\] ${SUBPROJECT}: ${DESCRIPTION}.*/[!] ${SUBPROJECT}: ${DESCRIPTION} (failed: $(date -Iseconds), reason: no commit in 15 min)/" QUEUE.md
    fi
done
```

### Stuck Detection

- **No git commit in 15 minutes** → session is stuck.
- Kill the session.
- Log context: last task, branch state, any error output.
- Narrow the task scope: break the failed task into 2-3 smaller tasks in QUEUE.md.
- Restart scheduler.

### Session Isolation

Each agent session starts fresh:
- No memory of previous sessions (except what's in git + docs).
- Agent reads docs → picks task → implements → tests → commits.
- Session state is entirely captured in the repository.

## Session Recovery

When a session ends (success, failure, or timeout):

1. Check git status — any uncommitted changes?
2. If uncommitted changes exist: stash or commit with "[WIP]" prefix.
3. Update QUEUE.md with task status.
4. Next session starts by reading QUEUE.md + git log + PLAN.md.

## Progress Monitoring

### For Humans

```bash
# See recent work
git log --oneline -20

# See task queue status
cat QUEUE.md

# See sub-project progress
cat projects/01-kernel-fork/PLAN.md | grep '\[x\]\|M[0-9]'

# Run all tests
cargo test
```

### Automated Metrics

Track over time:
- Tasks completed per session.
- Average session duration.
- Test count growth.
- Crate compilation success rate.

## Usage Window Strategy

Development happens in bursts (limited API budget, human oversight):

1. **Plan session** (human): Review QUEUE.md, adjust priorities, add new tasks.
2. **Development burst** (agent): Start container, scheduler runs 2-6 hours.
3. **Review session** (human): Review git log, read PLAN.md updates, check test results.
4. **Adjust** (human): Re-prioritize, narrow scope of failed tasks, add new requirements.
5. **Repeat.**

The system is designed for intermittent use — no continuous deployment, no always-on infrastructure. State is captured in git and documentation files.
