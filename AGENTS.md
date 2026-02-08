# Waffle Iron — Agent Team Structure

## Overview

Waffle Iron is developed by autonomous Claude Code agent teams. Each agent has a defined scope, reads specific documentation, and follows strict boundaries. This document defines the team structure and rules.

## Agent Roles

### Lead Agent

- **Scope:** Orchestration and dispatch. Never writes code directly.
- **Reads:** Top-level ARCHITECTURE.md, INTERFACES.md, all sub-project PLAN.md files.
- **Responsibilities:**
  - Dispatch work to sub-project agents.
  - Review cross-project interface compliance.
  - Prioritize work based on dependency graph.
  - Resolve inter-project conflicts.
  - Manage QUEUE.md for the task scheduler.

### Sub-Project Agents (one per sub-project)

- **Scope:** Single sub-project directory only.
- **Reads:** Their project's ARCHITECTURE.md, PLAN.md, INTERFACES.md, CLAUDE.md, plus top-level INTERFACES.md.
- **Responsibilities:**
  - Implement code within their crate/directory.
  - Run their tests before every commit.
  - Update their PLAN.md to mark completed tasks and add discovered tasks.
  - Document interface change requests (never modify top-level INTERFACES.md directly).

### Integration Agent

- **Scope:** Full workspace. Runs after sub-project milestones.
- **Reads:** All documentation. All code.
- **Responsibilities:**
  - Full workspace `cargo build` and `cargo test`.
  - Cross-crate integration tests.
  - Verify interface compliance across crate boundaries.
  - File issues when interfaces are violated.
  - Update top-level INTERFACES.md when approved changes are needed.

### Review Agent

- **Scope:** Read-only review across all sub-projects.
- **Reads:** All documentation and code.
- **Responsibilities:**
  - Interface compliance review (do crates use the types from INTERFACES.md?).
  - Test coverage review (are public functions tested?).
  - Documentation accuracy review (does PLAN.md reflect actual state?).
  - Determinism review (any non-deterministic code?).
  - Security review (any unsafe code, panics in production paths?).

## Rules

### Boundary Rules

1. **Sub-project agents NEVER modify files outside their sub-project directory.** Exception: integration agent.
2. **Sub-project agents NEVER modify top-level INTERFACES.md.** They document requested changes in their PLAN.md under "Interface Change Requests."
3. **No agent modifies another agent's branch** without explicit coordination.

### Workflow Rules

4. **Every agent reads INTERFACES.md before starting work.** Interface types are the contracts.
5. **Every agent runs tests before committing.** `cargo test -p <crate>` for sub-project agents. Full `cargo test` for integration agent.
6. **Every agent updates PLAN.md** to mark completed tasks and add discovered tasks.
7. **If stuck for more than 15 minutes without a commit,** the task scope is too broad. Break it down, document in PLAN.md, move on.

### Interface Change Process

8. Sub-project agent discovers interface gap → documents in their PLAN.md under "Interface Change Requests" with rationale and proposed change.
9. Lead agent reviews interface change requests across all sub-projects.
10. Integration agent implements approved changes to top-level INTERFACES.md.
11. All consuming sub-project agents are notified and must update their code.

### Quality Rules

12. **Tests are permanent.** Passing tests must never be deleted.
13. **Determinism is mandatory.** Same inputs → same outputs. No random values, no system time, no non-deterministic iteration.
14. **Mock before real.** Test against MockKernel first, TruckKernel second.
15. **Document failures.** If a truck operation fails in unexpected ways, document it in the kernel-fork PLAN.md.
