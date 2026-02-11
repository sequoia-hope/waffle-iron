# GUI Test Skeptic Report

**Date:** 2026-02-11
**Auditor:** Skeptic Agent (adversarial quality review)
**Scope:** All tests in `app/tests/` (Playwright) and `crates/*/tests/` (Rust)

---

## Executive Summary

**Total Playwright test files:** 16 spec files + 6 helper modules
**Total individual test cases (Playwright):** ~96 tests across all spec files
**Total Rust crate tests:** ~335 (kernel/engine level, zero GUI coverage)

### Test Classification

| Category | Count | % of Playwright Tests |
|---|---|---|
| **True GUI tests** (real DOM clicks/keys -> visible outcome) | ~32 | 33% |
| **API-level tests** (calls `window.__waffle` directly, bypasses GUI) | ~34 | 35% |
| **Infrastructure/smoke tests** (screenshot baselines, perf, trace replay) | ~30 | 31% |
| **Kernel tests** (Rust only, no GUI at all) | ~335 | N/A (separate) |

### Overall Verdict

The project has a **moderate** GUI test foundation for the sketch-draw-extrude happy path, but has **critical blind spots** in constraint workflows, dimension tool, snap feedback, and advanced interactions. A significant fraction of tests that *look* like GUI tests actually bypass the toolbar and dialog layers entirely.

---

## Trust Ratings by Workflow

| Workflow | Rating | Confidence |
|---|---|---|
| Sketch creation (entering sketch mode) | :yellow_circle: | GUI toolbar click tested; datum plane pre-selection uses API bypass |
| Drawing tools (line/rect/circle/arc) | :yellow_circle: | Line, rect, circle tested via real canvas clicks; arc has NO drawing test |
| Constraints (context menu, toolbar buttons) | :red_circle: | ZERO tests click constraint toolbar buttons or verify constraint state |
| Dimension tool | :red_circle: | ZERO tests activate or use the dimension tool |
| Sketch finish/save | :green_circle: | Well-tested via GUI: Finish Sketch button click, toolbar swap, state reset |
| Extrude dialog | :green_circle: | Strong: dialog open/close, depth input, Apply/Cancel, Enter/Escape keys |
| Sketch on face | :red_circle: | Only tested via API (`window.__waffle.enterSketch`), never via actual face click |
| Fully-constrained feedback | :red_circle: | ZERO tests check for fully-constrained visual indicator |
| Snap labels | :red_circle: | ZERO tests verify snap label DOM elements appear during drawing |
| Auto tangent/perpendicular | :red_circle: | ZERO tests verify auto-constraint inference |
| Revolve dialog | :red_circle: | Helper exists (`clickRevolve`) but ZERO tests use it |
| Fillet/Chamfer/Shell dialogs | :red_circle: | Toolbar buttons exist but ZERO tests exercise them |
| Undo/Redo | :red_circle: | Toolbar buttons exist but ZERO tests verify undo/redo behavior |

---

## Detailed Analysis by Test File

### `app/tests/smoke.spec.js` — 4 tests

| Test | Classification | What It Actually Tests |
|---|---|---|
| page loads without console errors | **Smoke** | Page loads, no JS crash. Not a GUI test. |
| toolbar renders with modeling tools | **Weak GUI** | Checks `button` count > 0. Doesn't verify specific buttons exist. |
| canvas element exists | **Smoke** | `<canvas>` is visible. Trivial. |
| `__waffle API is exposed` | **API-level** | Calls `window.__waffle.getState()`. Verifies API, not GUI. |

**Verdict:** Smoke checks only. No user workflows tested.

---

### `app/tests/sketch.spec.js` — 6 tests

| Test | Classification | Reality |
|---|---|---|
| enter sketch mode via `__waffle` API | **API bypass** | Calls `window.__waffle.enterSketch()` directly. Does NOT click the Sketch toolbar button. |
| set tool via `__waffle` API | **API bypass** | Calls `window.__waffle.setTool('line')`. Does NOT click the Line button. |
| click canvas with line tool creates entities | **Hybrid** | Enters sketch via API bypass, BUT drawing clicks ARE real canvas clicks. Verifies via API. |
| draw rectangle creates 4 lines | **Hybrid** | Same hybrid pattern — API setup, real canvas clicks, API verification. |
| escape key switches to select tool | **Hybrid** | API setup, real keyboard Escape, API verification. |
| exit sketch mode via `__waffle` API | **API bypass** | `window.__waffle.exitSketch()` directly. |

**Verdict:** The "click canvas" tests are the closest to real GUI tests in this file, but they skip the toolbar entry path. The `enterSketchAndWait` helper calls `window.__waffle.enterSketch()` — NOT the Sketch button.

---

### `app/tests/pipeline.spec.js` — 4 tests

| Test | Classification | Reality |
|---|---|---|
| sketch -> finish -> extrude -> verify 3D mesh | **API bypass** | Enters sketch via API, draws via canvas clicks, finishes via `window.__waffle.finishSketch()`, extrudes via `window.__waffle.showExtrudeDialog()` + `applyExtrude()`. **NEVER clicks a toolbar button.** |
| extruded solid has pickable faces with GeomRefs | **API bypass** | Same API bypass pattern. Verifies data structures, not visual rendering. |
| save/load roundtrip preserves feature tree | **API bypass** | `saveProject()`/`loadProject()` are API calls. No File menu, no dialog. |
| sketch-on-face enters sketch mode with correct plane | **API bypass** | Gets face ref via API, computes plane via API, enters sketch via API. **No face click in viewport.** |

**Verdict:** PURE TESTING THEATER. These tests look like end-to-end workflow tests from their names, but every single one bypasses the GUI completely for sketch entry, sketch finish, and extrude. The only real mouse interaction is drawing rectangles on canvas.

---

### `app/tests/gui/viewport.spec.js` — 8 tests

| Test | Classification | Reality |
|---|---|---|
| canvas renders and is visible | **True GUI** | Checks canvas size. |
| viewport container has data-testid | **True GUI** | Checks DOM structure. |
| toolbar renders with modeling tools | **True GUI** | Checks specific `data-testid` buttons. |
| status bar shows engine ready | **True GUI** | Reads status bar text content. |
| status dot is green when ready | **True GUI** | Checks CSS class. |
| orbit drag changes camera | **True GUI** | Real mouse drag on canvas. |
| mouse wheel zoom does not crash | **True GUI** | Real mouse wheel events. |
| F key triggers fit-all | **True GUI** | Real keyboard event. |

**Verdict:** GOOD. These are genuine GUI tests that interact with real DOM/canvas and verify visible outcomes.

---

### `app/tests/gui/sketch-entry.spec.js` — 6 tests

| Test | Classification | Reality |
|---|---|---|
| clicking Sketch button enters sketch mode | **True GUI** | `clickSketch()` uses `[data-testid="toolbar-btn-sketch"]`. Genuine button click. |
| entering sketch mode switches toolbar to sketch tools | **True GUI** | Verifies toolbar buttons appear/hide. |
| entering sketch mode shows Finish Sketch button | **True GUI** | Checks Finish button visibility. |
| S key enters sketch mode | **True GUI** | Real keyboard shortcut. |
| entering sketch mode sets line tool as default | **True GUI** | Verifies default tool state. |
| status bar reflects sketch mode | **True GUI** | Reads status bar text. |

**Verdict:** EXCELLENT. These are the gold standard — real clicks, real keyboard, real DOM verification.

---

### `app/tests/gui/sketch-draw.spec.js` — 8 tests

| Test | Classification | Reality |
|---|---|---|
| clicking Line button sets line tool | **True GUI** | Real toolbar button click via `clickLine()`. |
| draw line with two clicks creates entities | **True GUI** | Real canvas clicks via `drawLine()`, entities verified. |
| draw rectangle creates entities | **True GUI** | Real toolbar switch + canvas clicks. |
| draw circle creates entities | **True GUI** | Real toolbar switch + canvas clicks. |
| L key switches to line tool | **True GUI** | Real keyboard shortcut. |
| R key switches to rectangle tool | **True GUI** | Real keyboard shortcut. |
| C key switches to circle tool | **True GUI** | Real keyboard shortcut. |
| Escape switches from tool to select | **True GUI** | Real keyboard. |

**Verdict:** EXCELLENT. Genuine GUI tests through toolbar buttons and keyboard shortcuts. NOTE: **Arc drawing is not tested** — only line, rectangle, circle.

---

### `app/tests/gui/sketch-finish.spec.js` — 4 tests

| Test | Classification | Reality |
|---|---|---|
| draw rectangle then Finish Sketch creates a Sketch feature | **True GUI** | Real button clicks throughout. |
| after finishing sketch, toolbar returns to modeling tools | **True GUI** | Verifies toolbar button visibility. |
| after finishing sketch, tool resets to select | **True GUI** | Verifies state. |
| orbit controls re-enable after exiting sketch | **True GUI** | Real orbit drag after finish. |

**Verdict:** EXCELLENT. Fully GUI-driven.

---

### `app/tests/gui/extrude.spec.js` — 6 tests

| Test | Classification | Reality |
|---|---|---|
| after finishing sketch, clicking Extrude shows dialog | **True GUI** | Real button click, dialog DOM verification. |
| extrude dialog Cancel closes without creating feature | **True GUI** | Real Cancel button click. |
| extrude dialog Apply creates Extrude feature | **True GUI** | Real input fill + Apply click. |
| extrude creates 3D mesh with triangles | **True GUI** | Full workflow with mesh verification. |
| Enter key in extrude dialog applies | **True GUI** | Real Enter key. |
| Escape key in extrude dialog cancels | **True GUI** | Real Escape key. |

**Verdict:** EXCELLENT. Comprehensive dialog testing through actual DOM interaction.

---

### `app/tests/gui/workflow.spec.js` — 4 tests

| Test | Classification | Reality |
|---|---|---|
| Tutorial 1: Sketch rectangle -> Extrude box (mouse) | **True GUI** | Full mouse-driven workflow. The gold standard test. |
| Tutorial 1 (keyboard): S -> R -> draw -> Esc -> E | **True GUI** | Full keyboard-driven workflow. |
| draw multiple lines in sketch | **True GUI** | Multiple line draws. |
| sketch-extrude preserves orbit after completion | **True GUI** | Workflow + orbit verification. |

**Verdict:** EXCELLENT. These are genuine end-to-end GUI tests.

---

### `app/tests/gui/datum-planes.spec.js` — 11 tests

| Test | Classification | Reality |
|---|---|---|
| programmatically selecting XY/XZ/YZ datum plane | **API bypass** | `window.__waffle.selectRef()` — NOT a click on the datum plane in the 3D viewport. |
| clearSelection removes datum plane selection | **API bypass** | `window.__waffle.clearSelection()` directly. |
| computeFacePlane returns correct plane for XY/XZ/YZ datum | **API bypass** | Pure API computation check. |
| select XY/XZ/YZ plane then click Sketch enters sketch on that plane | **Hybrid** | Datum selection is API, but Sketch button click is real. |
| no selection defaults to XY plane | **True GUI** | Click Sketch with no prior selection. |
| S key with selected datum plane enters sketch on that plane | **Hybrid** | API datum selection + real keyboard shortcut. |

**Verdict:** MOSTLY API BYPASS. 7 of 11 tests never interact with the GUI. The "sketch entry from selected datum plane" tests are hybrids — the datum selection is programmatic (which is the hard part to test), only the Sketch button click is real.

---

### `app/tests/gui/viewport-advanced.spec.js` — 10 tests

| Test | Classification | Reality |
|---|---|---|
| zoom-in moves camera closer | **Hybrid** | Real mouse wheel, but verified via `getCameraState()` API. |
| zoom-out moves camera farther | **Hybrid** | Same. |
| zoom with no object under cursor | **True GUI** | Real mouse events, DOM stability check. |
| zoom near model zooms toward that area | **Hybrid** | Real mouse, API verification. |
| hover over edge area triggers store update | **API bypass** | `window.__waffle.setHoveredRef()` — NOT real mouse hover. |
| clearing hover resets hoveredRef | **API bypass** | Same. |
| getCameraState returns valid position after orbit | **Hybrid** | Real orbit, API verification. |
| getCameraState returns valid position after fit-all | **Hybrid** | Real keyboard, API verification. |
| getCameraState returns valid target after pan | **Hybrid** | Real drag, API verification. |
| edge hover color differs from default | **API bypass** | `setHoveredRef()` programmatically. **Title says "edge hover color" but doesn't check any visual color.** |

**Verdict:** MIXED. The zoom/orbit tests use real mouse events but the "hover" tests are pure API. The "edge hover color" test is especially misleading — it never checks any visual color, just stores/clears a ref programmatically.

---

### `app/tests/gui/selection/edge-pick.spec.js` — 5 tests

| Test | Classification | Reality |
|---|---|---|
| All 5 tests | **API bypass** | Every single test uses `window.__waffle.selectRef()` and `setHoveredRef()`. ZERO real mouse clicks for edge picking. |

**Verdict:** TESTING THEATER. These tests verify the selection store API, not edge picking in the viewport. A user picks edges by clicking geometry in the 3D view — none of that is tested.

---

### `app/tests/gui/selection/box-select.spec.js` — 5 tests

| Test | Classification | Reality |
|---|---|---|
| All 5 tests | **API bypass** | All use `window.__waffle.selectRef()`, `clearSelection()`, `getBoxSelectState()`. ZERO real mouse drag to perform box selection. |

**Verdict:** TESTING THEATER. "Box select" implies dragging a rectangle in the viewport. Not a single test does this.

---

### `app/tests/gui/selection/select-other.spec.js` — 5 tests

| Test | Classification | Reality |
|---|---|---|
| select other state starts empty | **API check** | Reads state shape. |
| selecting a face populates select-other state | **Hybrid** | One test uses `clickAt()` (real mouse), checks state. |
| shift-additive select keeps previous selection | **API bypass** | `selectRef()` calls. |
| clearSelection resets select-other state | **API bypass** | `selectRef()` + `clearSelection()`. |
| select other state has correct shape | **API check** | State shape verification. |

**Verdict:** 1 of 5 tests uses a real click. The rest are API-level. The actual "Select Other" cycling UI (clicking to cycle through overlapping geometry) is NOT tested.

---

### Infrastructure Tests (trace, screenshot, perf) — ~30 tests total

These test the *testing infrastructure*, not the application itself:

- **`trace-suite.spec.js`** (20 tests): Replays JSON trace files, only checks "no crash" (canvas visible after replay). No state verification beyond "still alive."
- **`screenshot-suite.spec.js`** (16 tests): Visual regression baselines. Uses real mouse interactions but only compares pixel screenshots — no semantic verification.
- **`perf-budget.spec.js`** (5 tests): Measures FPS, latency. Infrastructure validation, not workflow testing.
- **`trace-replay.spec.js`** (3 tests): Tests the trace replay helper itself.
- **`screenshot-baseline.spec.js`** (2 tests): Tests the screenshot helper.
- **`perf-budget.spec.js`** infra (3 tests): Tests the perf measurement helpers.

**Verdict:** These are valuable infrastructure but provide zero assurance that any specific workflow works correctly.

---

## Testing Theater Hall of Shame

Tests that LOOK like GUI tests from their names but actually bypass the GUI:

### 1. `pipeline.spec.js` — "sketch -> finish -> extrude -> verify 3D mesh"
**The name suggests:** A full end-to-end user workflow.
**What it actually does:** `window.__waffle.enterSketch()`, `window.__waffle.finishSketch()`, `window.__waffle.showExtrudeDialog()`, `window.__waffle.applyExtrude(10, 0)`. Not a single toolbar button is clicked.

### 2. `pipeline.spec.js` — "sketch-on-face enters sketch mode with correct plane"
**The name suggests:** User clicks a face in the 3D viewport, then enters sketch on it.
**What it actually does:** Gets a face ref via API, computes its plane via API, calls `enterSketch()` via API. The actual face-clicking raycast workflow is completely untested.

### 3. `edge-pick.spec.js` — "programmatic edge ref select via API"
**The name is at least honest**, but the test file is called "edge-pick" which implies clicking edges in the viewport. None of the 5 tests click anything in the viewport.

### 4. `box-select.spec.js` — "box select"
**The name suggests:** Dragging a selection rectangle in the viewport.
**What it actually does:** Calls `window.__waffle.selectRef()` programmatically. Zero mouse drags.

### 5. `viewport-advanced.spec.js` — "edge hover color differs from default"
**The name suggests:** Visual verification that edges change color on hover.
**What it actually does:** Calls `setHoveredRef()` programmatically, then reads it back. Never checks any visual color. Never hovers the mouse over anything.

### 6. `datum-planes.spec.js` — "programmatically selecting XY datum plane"
**The reality:** 7 of 11 tests use `window.__waffle.selectRef()` to programmatically select datum planes. A real user would click the datum plane representation in the 3D viewport.

---

## Critical Gaps — What Is NOT Tested At All

### 1. Constraint Toolbar Buttons (:red_circle: ZERO TESTS)
The toolbar has 9 constraint buttons (Horizontal, Vertical, Coincident, Perpendicular, Parallel, Equal, Tangent, Midpoint, Fix). **NOT A SINGLE TEST** clicks any of these buttons. We have:
- No test that selects two entities and clicks "Horizontal"
- No test that verifies the `disabled` state toggling based on selection
- No test that verifies constraints appear in the sketch after applying
- No test that verifies the constraint is sent to the solver

### 2. Dimension Tool (:red_circle: ZERO TESTS)
The Smart Dimension tool (`D` shortcut, `data-testid="toolbar-btn-dimension"`) has zero tests. No test:
- Activates the dimension tool
- Clicks an entity to dimension
- Enters a value
- Verifies the dimension constraint appears

### 3. Snap Labels (:red_circle: ZERO TESTS)
When drawing near grid points, the origin, or existing points, snap labels should appear. Zero tests verify:
- Snap labels render in the DOM
- Snap indicators appear at the correct position
- Snapping actually changes the coordinate that gets created

### 4. Fully-Constrained Feedback (:red_circle: ZERO TESTS)
When all entities in a sketch are fully constrained, the sketch should show visual feedback (e.g., green entities, status message). Zero tests verify this.

### 5. Construction Mode (:red_circle: ZERO TESTS)
The toolbar has a "Construction" toggle (`X` shortcut). Zero tests:
- Toggle construction mode
- Draw a construction line
- Verify construction lines are visually different
- Verify construction lines are excluded from profiles

### 6. Arc Drawing (:red_circle: ZERO TESTS)
The arc tool has a toolbar button and keyboard shortcut (`A`), but zero tests actually draw an arc. The keyboard shortcut test would need to exist alongside actual arc drawing.

### 7. Revolve Dialog (:red_circle: ZERO TESTS)
A `clickRevolve()` helper exists but is never used. Zero tests exercise the revolve workflow.

### 8. Fillet/Chamfer/Shell (:red_circle: ZERO TESTS)
Toolbar buttons exist for all three. Zero tests click them or interact with any dialogs.

### 9. Undo/Redo (:red_circle: ZERO TESTS)
Undo and Redo buttons exist in the toolbar. Zero tests verify:
- Drawing a line, pressing Ctrl+Z, and seeing it disappear
- Pressing Ctrl+Shift+Z and seeing it reappear
- Undoing an extrude operation

### 10. Context Menu Constraints (:red_circle: ZERO TESTS)
If there's a right-click context menu for constraints (Onshape-style), it's completely untested.

### 11. Face Click -> Sketch on Face (GUI path) (:red_circle: ZERO TESTS)
The pipeline test does sketch-on-face via API. No test actually:
1. Extrudes a box
2. Clicks a face in the 3D viewport (raycast picking)
3. Clicks the Sketch button
4. Verifies sketch enters on the correct plane

### 12. Feature Tree Interaction (:red_circle: ZERO TESTS)
No test clicks on features in the feature tree panel to:
- Select/highlight a feature
- Suppress/unsuppress a feature
- Edit a feature's parameters
- Reorder features

---

## What Works Well

The following areas have genuine, trustworthy GUI coverage:

1. **Sketch entry via toolbar** — `sketch-entry.spec.js` is solid
2. **Drawing with line/rect/circle** — `sketch-draw.spec.js` uses real clicks
3. **Finish Sketch button** — `sketch-finish.spec.js` is thorough
4. **Extrude dialog lifecycle** — `extrude.spec.js` covers open/close/apply/cancel
5. **End-to-end sketch->extrude workflow** — `workflow.spec.js` is the gold standard
6. **Viewport orbit/zoom** — `viewport.spec.js` uses real mouse events
7. **Keyboard shortcuts for tools** — S, L, R, C, E, Escape all tested

---

## Recommendations

### Immediate Priority (Before any "ship" claim)
1. **Constraint toolbar**: Test each of the 9 buttons with appropriate selection states
2. **Dimension tool**: Activate, apply to an entity, verify constraint created
3. **Arc drawing**: The only untested drawing tool
4. **Undo/Redo**: Critical for any CAD workflow

### High Priority
5. **Snap verification**: Draw near origin/grid, verify snap label DOM and resulting coordinates
6. **Construction toggle**: Activate, draw, verify visual distinction
7. **Sketch-on-face via GUI**: Full raycast path, not API bypass
8. **Revolve dialog**: Like extrude but for revolve

### Medium Priority
9. **Feature tree clicks**: Select, suppress, edit features
10. **Fully-constrained feedback**: Visual state changes when fully constrained
11. **Auto tangent/perpendicular inference**: Draw arc tangent to line, verify auto-constraint
12. **Box select via mouse drag**: Real drag selection, not API

### Architecture Concerns
- The `window.__waffle` API is a **testing backdoor** that makes it too easy to write tests that feel productive but bypass the GUI entirely. Tests using it for *verification* (checking state after a real click) are fine. Tests using it for *actions* (entering sketch mode, finishing sketch) are API tests, not GUI tests.
- The `enterSketchAndWait()` helper in `sketch.spec.js` and `pipeline.spec.js` calls `window.__waffle.enterSketch()` — this should be replaced with `clickSketch()` from the toolbar helper for true GUI coverage.

---

## Summary Stats

| Metric | Value |
|---|---|
| GUI workflows with :green_circle: trust | 3 (sketch finish, extrude dialog, viewport basics) |
| GUI workflows with :yellow_circle: partial trust | 2 (sketch creation, drawing tools) |
| GUI workflows with :red_circle: no trust | 10 (constraints, dimensions, snap, construction, arc, revolve, fillet/chamfer/shell, undo/redo, sketch-on-face, feature tree) |
| Tests that bypass GUI but have GUI-sounding names | 6+ (pipeline.spec.js, edge-pick, box-select, datum-planes) |
| Critical untested toolbar buttons | 14 (9 constraints + dimension + construction + revolve + fillet + chamfer + shell) |
