# 12 — Design Review: Agent Instructions

## Purpose

This sub-project is a **read-only assessment** of the Waffle Iron codebase. It produces `REVIEW.md` at the workspace root — a comprehensive, code-verified design review document.

## Rules

- **Do NOT modify any source code.** This is analysis only.
- **Every claim must cite a file path and line number.** No hand-waving.
- **Verify against code, not docs.** PLAN.md files may be optimistic. Check the actual implementation.
- **Be honest about gaps.** The review is useless if it sugarcoats problems.

## Key Files to Inspect

| Area | Files |
|------|-------|
| Rebuild pipeline | `crates/feature-engine/src/rebuild.rs` |
| Kernel capabilities | `crates/kernel-fork/src/truck_kernel.rs`, `mock_kernel.rs` |
| WASM bridge | `crates/wasm-bridge/src/dispatch.rs`, `messages.rs` |
| UI tools/dialogs | `app/src/lib/ui/Toolbar.svelte`, `PropertyEditor.svelte` |
| Sketch interaction | `app/src/lib/sketch/SketchInteraction.svelte`, `tools.js` |
| Face picking | `app/src/lib/viewport/CadModel.svelte` |
| File format | `crates/file-format/src/step_export.rs`, `save.rs`, `load.rs` |
| Store | `app/src/lib/engine/store.svelte.js` |

## Deliverable

`/home/claude/workspace/REVIEW.md` — the design review document with all six sections as specified in the plan.
