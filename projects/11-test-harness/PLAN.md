# 11 — Test Harness: Plan

## Overview

Autonomous GUI testing harness enabling Claude to programmatically test every CAD workflow (sketch, extrude, face selection, booleans, etc.) without human interaction.

## Prerequisites

- Sub-project 12 (Design Review) should be completed first to validate architecture before instrumenting tests against it.

## Milestones

### M1: Layer 1 — Engine Protocol Tests ✅
Implemented as native Rust integration tests (not Node.js WASM) in
`crates/wasm-bridge/tests/pipeline_tests.rs` — 21 tests covering:
- [x] Sketch → Extrude → Mesh verification (indices=36, faces=6, bounding box)
- [x] Sketch → Revolve → Mesh verification
- [x] Non-XY plane sketch → Extrude (plane_origin/normal preserved)
- [x] Multi-feature: Extrude → Fillet (face count increases)
- [x] Save/load roundtrip with plane data preservation
- [x] Feature CRUD: delete middle, suppress/unsuppress
- [x] Undo/redo with mesh verification (full cycle)
- [x] Error paths: nonexistent sketch, bad profile index, no BeginSketch
- [x] Explicit extrude direction, multiple independent models
- [x] Rename preserves solid, face ranges cover all indices, reorder

### M2: Layer 2 — Programmatic Test API (in-browser)
- [ ] Create `app/src/lib/test-api.js` with high-level operations
- [ ] Register `window.__test` in dev mode via `+page.svelte`
- [ ] Implement: `drawRectangle()`, `drawCircle()`, `finishSketch()`, `extrude()`
- [ ] Implement: `selectFaceByRole()`, `getState()`, `getMeshStats()`, `getMeshBoundingBox()`
- [ ] Implement: `waitForEngine()` with timeout polling

### M3: Layer 3 — Playwright E2E Tests (headless Chromium)
- [ ] Install Playwright + Chromium in container
- [ ] Create `playwright.config.js` with SwiftShader WebGL
- [ ] Create test fixtures with `waffle` helper
- [ ] Test: Sketch → Extrude → Verify box mesh
- [ ] Test: Two-step extrusion (sketch-on-face)
- [ ] Test: Boolean cut
- [ ] Test: Undo/redo full chain
- [ ] Screenshot capture + Claude vision verification workflow

### M4: Integration with Development Workflow
- [ ] Add npm test scripts to package.json
- [ ] Document Claude's quick/full/ad-hoc verification loops
- [ ] Verify Layer 1 tests run in < 5 seconds
- [ ] Verify Layer 3 tests run in < 60 seconds

## Architecture

Three-layer approach (details in ARCHITECTURE.md):
1. **Engine Protocol Tests** — Node.js WASM, no browser, < 1s
2. **Programmatic Test API** — `window.__test`, wraps store functions
3. **Playwright E2E** — headless Chromium, SwiftShader WebGL, screenshots

## Key Insight

The store already exposes all operations as functions. We drive operations programmatically (not via mouse simulation). Face selection uses GeomRef roles, not pixel positions.

## Blockers

None currently. Depends on Chromium being installable in the Docker container.
