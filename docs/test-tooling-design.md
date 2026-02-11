# Test Tooling Design: GUI Architecture & Testing Infrastructure

## 1. GUI Architecture Overview

### Component Hierarchy

```
App.svelte
  +-- Toolbar.svelte              (keyboard shortcuts, toolbar buttons, mode switching)
  +-- FeatureTree.svelte          (feature list, delete/suppress/reorder)
  +-- Viewport.svelte             (canvas container + overlays)
  |     +-- Canvas (@threlte/core)
  |     |     +-- Scene.svelte
  |     |     |     +-- Lighting.svelte
  |     |     |     +-- CameraControls.svelte
  |     |     |     +-- CadModel.svelte         (3D mesh rendering)
  |     |     |     +-- EdgeOverlay.svelte       (wireframe edges)
  |     |     |     +-- GridFloor.svelte
  |     |     |     +-- SketchPlane.svelte       (visual sketch plane)
  |     |     |     +-- DatumVis.svelte          (datum plane visuals)
  |     |     |     +-- BoxSelect.svelte
  |     |     |     +-- SketchRenderer.svelte    (2D sketch geometry)
  |     |     |     +-- SketchInteraction.svelte (pointer event handling)
  |     |     |     +-- DimensionLabels.svelte   (constraint dimension labels)
  |     |     +-- ViewCubeGizmo.svelte
  |     +-- ViewCubeButtons.svelte
  |     +-- ConstraintMenu.svelte     (right-click constraint menu)
  |     +-- DimensionInput.svelte     (dimension value popup)
  |     +-- ViewportContextMenu.svelte
  +-- StatusBar.svelte
  +-- ExtrudeDialog.svelte
  +-- RevolveDialog.svelte
```

### State Architecture

All reactive state lives in `store.svelte.js` (Svelte 5 runes / `$state`). Key state groups:

| State Group | Variables | Purpose |
|-------------|-----------|---------|
| Engine | `engineReady`, `lastError`, `statusMessage`, `rebuildTime` | WASM engine lifecycle |
| Feature Tree | `featureTree`, `selectedFeatureId` | Parametric feature model |
| Meshes | `meshes` (with `faceRanges`) | 3D geometry for rendering |
| 3D Selection | `hoveredRef`, `selectedRefs`, `boxSelectState`, `selectOtherState` | 3D entity picking |
| Sketch Mode | `sketchMode` (`active`, `origin`, `normal`) | Sketch plane definition |
| Sketch Entities | `sketchEntities`, `sketchConstraints`, `sketchPositions`, `nextEntityId` | Sketch geometry state |
| Sketch UI | `sketchSelection`, `sketchHover`, `sketchCursorPos`, `overConstrainedEntities` | Sketch interaction state |
| Profiles | `extractedProfilesState`, `selectedProfileIndex`, `hoveredProfileIndex` | Profile extraction |
| Tools | `activeTool` | Current tool mode |
| Dialogs | `extrudeDialogState`, `revolveDialogState`, `dimensionPopup` | Modal state |
| Snap | `snapSettings` | Configurable snap thresholds |
| Camera | `cameraObject`, `controlsObject` (non-reactive refs) | Camera/controls refs |

---

## 2. Event Path: User Action to Visual Update

### Sketch Drawing Path (most complex)

```
User clicks canvas
  |
  v
SketchInteraction.svelte:handler(PointerEvent)
  |-- Reads sketchMode from store (origin, normal)
  |-- Gets camera from useThrelte() — camera.current
  |-- Calls screenToSketchCoords(event, canvas, camera, plane)
  |      (sketchCoords.js: raycast from screen → sketch plane intersection)
  |-- Calls getScreenPixelSize() for adaptive thresholds
  |-- Calls handleToolEvent(tool, eventType, x, y, screenPixelSize, shiftKey)
        |
        v
tools.js:handleToolEvent()
  |-- Dispatches to handleLineTool/handleRectangleTool/handleCircleTool/etc.
  |-- Each tool has a state machine: idle → firstPointPlaced → ...
  |-- Calls detectSnaps(x, y, fromPointId, screenPixelSize) from snap.js
  |     |-- Priority: coincident > H/V > on-entity > tangent > perpendicular
  |     |-- Returns: snapped {x, y}, snapPointId, constraints[], indicator
  |-- On placement: findOrCreatePoint(x, y, screenPixelSize, snapPointId)
  |     |-- Reuses existing points within threshold
  |     |-- Creates new Point via allocEntityId() + addLocalEntity()
  |-- Creates geometry entities (Line, Circle, Arc) via addLocalEntity()
  |-- Auto-applies constraints (H/V/Tangent/Perpendicular) via addLocalConstraint()
  |-- Updates currentPreview and currentSnapIndicator for rubberband display
        |
        v
store.svelte.js:addLocalEntity(entity)
  |-- Appends to sketchEntities array (triggers Svelte reactivity)
  |-- Updates sketchPositions map for Point entities
  |-- Sends { type: 'AddSketchEntity', entity } to bridge
  |-- Calls reExtractProfiles() for closed-loop detection
        |
        v
bridge.js:EngineBridge.send(message)
  |-- Posts message to Web Worker via postMessage
  |-- Returns Promise that resolves with worker response
  |-- Worker runs Rust WASM: process_message() → EngineToUi response
        |
        v
Worker responds with SketchSolved / ModelUpdated
  |-- bridge._handleMessage() dispatches to registered callbacks
  |-- store.svelte.js callbacks update reactive state:
  |     - sketchPositions (Map<id, {x,y}>)
  |     - sketchSolveStatus (status, dof, failed, solveTime)
  |     - featureTree, meshes
        |
        v
Svelte 5 reactivity propagates:
  |-- SketchRenderer.svelte: $derived(getSketchEntities()) rebuilds geometry
  |     |-- pointData, lineData, circleData, arcData → THREE.BufferGeometry
  |     |-- entityColor() driven by selection/hover/constraint state
  |     |-- previewGeo from tools.getPreview()
  |     |-- snapGeo from tools.getSnapIndicator()
  |-- DimensionLabels.svelte: $derived(getSketchConstraints()) renders dim labels
  |-- DimensionInput.svelte: $derived(getDimensionPopup()) shows/hides popup
  |-- Toolbar.svelte: $derived(getApplicableConstraints()) enables/disables buttons
```

### Toolbar/Keyboard Shortcut Path

```
User presses key (e.g. 'l' for line)
  |
  v
Toolbar.svelte:onKeyDown(KeyboardEvent)
  |-- Guards: skips if target is input/textarea, if engine not ready
  |-- Ctrl combos: Save(Ctrl+S), Open(Ctrl+O), Undo(Ctrl+Z), Redo(Ctrl+Shift+Z)
  |-- Tool shortcuts: s=sketch, e=extrude, l=line, r=rect, c=circle, a=arc, x=construction, d=dimension
  |-- Escape: tool→select or select→finishSketch (two-stage exit)
  |-- Calls setActiveTool(toolId) / handleToolClick(toolId)
        |
        v
store.svelte.js:setActiveTool(tool)
  |-- Updates activeTool $state
  |-- SketchInteraction.svelte: $effect detects activeTool change → resetTool()
  |-- Toolbar.svelte: $derived(getActiveTool()) updates button highlights
```

### 3D Selection Path

```
User clicks 3D model face/edge
  |
  v
CadModel.svelte (Threlte interactivity):
  |-- Raycaster hit → identifies mesh face → looks up faceRanges → geom_ref
  |-- Calls selectRef(geomRef, shiftKey) from store
        |
        v
store.svelte.js:selectRef(ref, additive)
  |-- Updates selectedRefs array (Svelte reactivity)
  |-- Sends { type: 'SelectEntity', geom_ref } to bridge
  |-- CadModel.svelte re-renders with highlight colors
```

### Constraint Application Path

```
User selects entities, clicks constraint button (e.g. "H")
  |
  v
Toolbar.svelte:applyConstraint(id)
  |-- Reads applicable[id] from getApplicableConstraints()
  |-- constraintLogic.js checks selection composition:
  |     1 line → horizontal/vertical/distance
  |     2 points → coincident/distance
  |     2 lines → parallel/perpendicular/equal
  |     1 point + 1 line → midpoint/distance
  |     1 circle/arc → radius
  |     etc.
  |-- Returns builder function, e.g. () => ({ type: 'Horizontal', entity: lineId })
  |-- Calls addLocalConstraint(builder())
        |
        v
store.svelte.js:addLocalConstraint(constraint)
  |-- Appends to sketchConstraints
  |-- Calls recomputeOverConstrained() for visual feedback
  |-- Sends { type: 'AddConstraint', constraint } to bridge
  |-- Calls triggerSolve() → bridge.send({ type: 'SolveSketchLocal', ... })
  |-- Worker returns SketchSolved with updated positions → sketchPositions updates
```

### Dimension Tool Path

```
User selects dimension tool, clicks line
  |
  v
tools.js:handleDimensionTool()
  |-- Hit-test entities at cursor position
  |-- Single-click line → compute length, showDimensionPopup() at midpoint
  |-- Single-click circle/arc → compute radius, showDimensionPopup()
  |-- First click point → wait for second entity
  |-- Second click point/line → showDimensionPopup() with computed distance
        |
        v
DimensionInput.svelte:
  |-- $derived(getDimensionPopup()) shows/hides popup
  |-- sketchToScreen() converts sketch coords → screen position
  |-- User types value, presses Enter
  |-- Calls applyDimensionFromPopup(value)
        |
        v
store.svelte.js:applyDimensionFromPopup(value)
  |-- Creates Distance/Radius/Angle constraint based on dimType
  |-- Calls addLocalConstraint(constraint)
  |-- Hides popup (dimensionPopup = null)
```

---

## 3. Existing Test Infrastructure

### Test File Inventory

| File | Test Count | Description | Uses Real GUI Events? |
|------|-----------|-------------|----------------------|
| **Root-level tests** | | | |
| `smoke.spec.js` | 4 | Page load, toolbar presence, canvas exists, __waffle API | Partial (DOM checks) |
| `sketch.spec.js` | 6 | Sketch mode, tools, drawing, escape key | Mixed (API + canvas clicks) |
| `pipeline.spec.js` | 4 | Sketch→extrude pipeline, save/load, sketch-on-face | Mixed (API + canvas clicks) |
| **gui/ tests** | | | |
| `gui/viewport.spec.js` | 7 | Canvas render, toolbar, status, orbit, zoom, fit-all | YES (mouse drags, wheel, keys) |
| `gui/sketch-entry.spec.js` | 6 | Enter sketch via button/key, toolbar switch, status | YES (button clicks, key presses) |
| `gui/sketch-draw.spec.js` | 8 | Line/rect/circle drawing, tool shortcuts | YES (canvas clicks, key presses) |
| `gui/sketch-finish.spec.js` | 4 | Finish sketch, toolbar reset, orbit re-enable | YES (button clicks) |
| `gui/extrude.spec.js` | 6 | Extrude dialog, apply, cancel, mesh creation, Enter/Escape keys | YES (dialog interaction) |
| `gui/workflow.spec.js` | 4 | Full sketch→extrude tutorials (mouse + keyboard) | YES (end-to-end GUI) |
| `gui/datum-planes.spec.js` | 10 | Datum selection, plane computation, sketch-on-plane | API-only (programmatic) |
| `gui/viewport-advanced.spec.js` | 9 | Zoom behavior, camera state, hover management | Mixed (wheel/orbit + API) |
| **gui/selection/** | | | |
| `gui/selection/edge-pick.spec.js` | 5 | Edge ref selection, hover, additive select | API-only (programmatic) |
| `gui/selection/box-select.spec.js` | 5 | Box select state, programmatic selection | API-only (programmatic) |
| `gui/selection/select-other.spec.js` | 5 | Select-other cycling, additive, clearSelection | API-only (programmatic) |
| **gui/infra/** | | | |
| `gui/infra/trace-replay.spec.js` | 3 | Trace replay infrastructure validation | YES (replay system) |
| `gui/infra/screenshot-baseline.spec.js` | 2 | Screenshot comparison infrastructure | YES (screenshots) |
| `gui/infra/perf-budget.spec.js` | 3 | FPS, pick latency, rebuild time measurement | Mixed |
| **gui/baselines/** | | | |
| `gui/baselines/screenshot-suite.spec.js` | 18 | Visual regression baselines | YES (orbit, zoom, click) |
| **gui/traces/** | | | |
| `gui/traces/trace-suite.spec.js` | 15 | Replay 15 pre-recorded interaction traces | YES (trace replay) |
| **gui/perf/** | | | |
| `gui/perf/budgets.spec.js` | 4 | FPS >= 10, pick latency <= 200ms, rebuild <= 500ms | YES (real interactions) |

**Total test count: ~128 Playwright tests**

### Helper Library

| Helper | File | Key Functions |
|--------|------|---------------|
| **WafflePage fixture** | `helpers/waffle-test.js` | `WafflePage` class with `goto()`, `waitForReady()`, `screenshot()`, `dumpState()`; extended `test` fixture |
| **State queries** | `helpers/state.js` | `isSketchActive()`, `getActiveTool()`, `getEntityCount()`, `getEntities()`, `getEntityCountByType()`, `getFeatureTree()`, `hasMeshWithGeometry()`, `waitForEntityCount()`, `waitForFeatureCount()`, `getExtrudeDialogState()` |
| **Toolbar clicks** | `helpers/toolbar.js` | `clickSketch()`, `clickLine()`, `clickRectangle()`, `clickCircle()`, `clickArc()`, `clickSelect()`, `clickFinishSketch()`, `clickExtrude()`, `clickRevolve()`, `pressKey()`, `isToolbarButtonVisible()` |
| **Canvas interactions** | `helpers/canvas.js` | `getCanvasBounds()`, `clickAt(xOff, yOff)`, `drawLine()`, `drawRectangle()`, `drawCircle()`, `orbitDrag()`, `zoom()` |
| **Trace replay** | `helpers/trace.js` | `replayTrace(page, steps)`, `loadTrace(path)` — supports click, drag, wheel, key, wait, evaluate actions with optional assertions |
| **Screenshots** | `helpers/screenshot.js` | `assertScreenshot(page, name, options)` — wraps `toHaveScreenshot` with default masks |
| **Performance** | `helpers/perf.js` | `measureFPS()`, `measurePickLatency()`, `measureRebuildTime()`, `expectFPS()`, `expectLatency()` |

### `window.__waffle` Test API

Exposed by `initEngine()` in `store.svelte.js`. Available methods:

| Method | Returns | Purpose |
|--------|---------|---------|
| `getState()` | `{ engineReady, sketchMode, activeTool, entityCount }` | Core state snapshot |
| `getEntities()` | `Array<SketchEntity>` | All sketch entities |
| `getPositions()` | `Map<id, {x, y}>` | Solved entity positions |
| `getConstraints()` | `Array<SketchConstraint>` | All sketch constraints |
| `getProfiles()` | `Array<{entityIds, isOuter}>` | Extracted profiles |
| `getFeatureTree()` | `{features, active_index}` | Feature tree |
| `getMeshes()` | `Array<{featureId, vertexCount, triangleCount, ...}>` | Mesh summaries |
| `getCameraState()` | `{position, target, fov, up}` | Camera state |
| `enterSketch(origin, normal)` | `void` | Enter sketch mode programmatically |
| `exitSketch()` | `void` | Exit sketch mode |
| `setTool(tool)` | `void` | Set active tool |
| `finishSketch()` | `Promise` | Finish sketch and create feature |
| `showExtrudeDialog()` | `void` | Open extrude dialog |
| `applyExtrude(depth, profileIndex)` | `Promise` | Apply extrude |
| `saveProject()` | `Promise<string>` | Save project to JSON |
| `loadProject(json)` | `Promise` | Load project from JSON |
| `exportStl()` | `Promise<boolean>` | Export STL |
| `computeFacePlane(geomRef)` | `{origin, normal} | null` | Compute face plane |
| `selectRef(ref, additive)` | `void` | Select a geometry reference |
| `clearSelection()` | `void` | Clear all selections |
| `setHoveredRef(ref)` | `void` | Set hovered entity |
| `getSelectedRefs()` | `Array<GeomRef>` | Get selected refs |
| `getHoveredRef()` | `GeomRef | null` | Get hovered ref |
| `getBoxSelectState()` | `{active, startX, startY, ...}` | Box select state |
| `getSelectOtherState()` | `{intersections, cycleIndex, ...}` | Select-other state |
| `getRebuildTime()` | `number` | Last rebuild time |
| `getDimensionPopup()` | `{entityA, entityB, ...} | null` | Dimension popup state |
| `showDimensionPopup(popup)` | `void` | Show dimension popup |
| `hideDimensionPopup()` | `void` | Hide dimension popup |
| `applyDimensionFromPopup(value)` | `void` | Apply dimension value |
| `getExtrudeDialogState()` | `{sketchId, ...} | null` | Extrude dialog state |
| `getRevolveDialogState()` | `{sketchId, ...} | null` | Revolve dialog state |

### Trace Fixtures (19 traces)

Stored in `tests/fixtures/traces/`. Each is a JSON array of steps:
- **Viewport**: orbit, pan, zoom, fit-all, orbit+zoom combo
- **Sketch**: draw-line, draw-rectangle, draw-circle, snap-origin, constrain-horizontal
- **Selection**: face-pick, edge-pick, box-select-window, box-select-crossing, select-other
- **Workflow**: sketch-extrude, sketch-on-face, multi-feature, orbit-select-sketch, zoom-select-zoom

### Screenshot Baselines (20 baselines)

Stored as PNG snapshots in `*-snapshots/` directories. Cover:
- Scene states: empty, orbited, zoomed, fit-all
- Selection states: hover, selected, none
- Sketch mode: active, grid
- Datum planes: default, orbited
- View angles: front, top, isometric
- UI chrome: toolbar, feature tree
- Combined: rectangle-drawn, zoomed-selected, orbited-edge-area, multi-step-final

### Playwright Configuration

- Browser: Chromium only
- Viewport: 1280x720
- Headless: true (SwiftShader WebGL)
- Timeout: 60s per test, 10s for expects
- Screenshot comparison: 1% max pixel ratio, 0.2 threshold
- WebServer: `npm run dev` on port 5173

---

## 4. Testability Assessment

### What CAN Be Tested Through Real GUI Events Today

| Area | How | Confidence |
|------|-----|-----------|
| Toolbar button clicks | `clickSketch()`, `clickLine()`, etc. via `data-testid` locators | HIGH |
| Keyboard shortcuts | `page.keyboard.press()` | HIGH |
| Canvas pointer events (sketch drawing) | `clickAt(xOffset, yOffset)` relative to canvas center | MEDIUM — coordinate mapping from screen pixels to sketch coords depends on camera position |
| Extrude/Revolve dialogs | Standard DOM interaction (fill inputs, click buttons) | HIGH |
| Dimension value input | DOM input fill + Enter key | HIGH |
| Orbit/Zoom/Pan | Mouse drag + wheel events on canvas | MEDIUM — verified by camera state changes |
| Visual regression | Screenshot baselines | HIGH — but SwiftShader rendering may differ from GPU |
| Trace replay | Pre-recorded mouse/key sequences | MEDIUM — hardcoded absolute coordinates are fragile |

### What Requires `__waffle` API (Can't Be Tested Through GUI Alone)

| Area | Why | Current Approach |
|------|-----|-----------------|
| Entity count / type verification | No DOM representation of sketch entities — they render in WebGL | `getEntities()`, `getEntityCountByType()` |
| Constraint verification | Constraints are stored in JS state, not visible in DOM | `getConstraints()` |
| Feature tree verification | Feature tree panel exists but verifying specific feature types requires API | `getFeatureTree()`, `hasFeatureOfType()` |
| Mesh existence / triangle count | 3D mesh is in WebGL, no DOM queryable | `getMeshes()`, `hasMeshWithGeometry()` |
| Profile extraction | Profiles are computed internally | `getProfiles()` |
| Sketch mode state | Active/inactive, plane origin/normal | `getState().sketchMode` |
| Solver status / DOF | Internal solve state | Not yet exposed for testing |
| Camera position after interactions | Three.js camera state | `getCameraState()` |
| Datum plane selection | Datum planes are visuals without click targets | `selectRef()` programmatically |
| Snap detection | Snap happens in tools.js, no visual indicator queryable | Not directly testable |
| Over-constraint detection | Visual indicator (red color) but requires pixel analysis | Not testable except via screenshots |

### What CANNOT Be Tested At All (Gaps)

| Gap | Why | Impact |
|-----|-----|--------|
| **Snap visual feedback** | Snap indicators (green dot, dashed line) render in WebGL scene — no DOM elements, no queryable state | Cannot verify snap indicator appears/disappears |
| **Constraint label rendering** | H/V labels render as Three.js meshes, not DOM | Cannot verify constraint labels show up |
| **Entity color states** | Selection/hover/over-constraint colors are Three.js materials | Can only test via screenshots (fragile with SwiftShader) |
| **Rubberband preview rendering** | Preview geometry is Three.js lines | Cannot verify preview follows cursor |
| **Profile fill rendering** | Profile highlights are Three.js shape geometries | Cannot verify profile fill appears |
| **Construction line dashing** | Dashed lines are Three.js materials | Cannot verify visual dash pattern |
| **Dimension label click-to-edit** | Labels are `@threlte/extras` HTML overlays — may not be queryable by standard Playwright selectors | Partially testable (HTML is injected into DOM but may have unusual positioning) |
| **Right-click context menu (constraints)** | ConstraintMenu uses custom positioning | Partially testable (if DOM is correct) |
| **Box select visual rectangle** | Renders as DOM overlay — actually testable if `data-testid` is added | Needs data-testid addition |
| **3D face/edge picking via mouse** | Requires raycaster intersection with actual geometry | Cannot test picking without real geometry; tests use `selectRef()` API bypass |

---

## 5. Recommended Tooling Improvements

### Priority 1: Expose Missing State for Verification

**a) Snap indicator state in `__waffle`**

The snap system (`tools.js:getSnapIndicator()`) is module-level state not exposed through `__waffle`. Add:
```js
window.__waffle.getSnapIndicator = () => getSnapIndicator();
```
This enables tests to verify: "after moving cursor near a point, the coincident snap indicator is active."

**b) Preview state in `__waffle`**

The preview geometry (`tools.js:getPreview()`) is module-level state not exposed. Add:
```js
window.__waffle.getPreview = () => getPreview();
```
This enables tests to verify: "while drawing a line, the preview line geometry exists with correct endpoints."

**c) Solver DOF in `__waffle`**

Add:
```js
window.__waffle.getSolveStatus = () => getSketchSolveStatus();
```
This enables tests to verify: "after adding H+V constraints to a rectangle, DOF drops to expected value."

**d) Over-constrained entities in `__waffle`**

Add:
```js
window.__waffle.getOverConstrained = () => [...getOverConstrainedEntities()];
```

### Priority 2: Improve Canvas Interaction Helpers

**a) Sketch-aware click helper**

Current `clickAt(xOffset, yOffset)` uses pixel offsets from canvas center. This is fragile because:
- Camera position affects where sketch origin maps to screen
- Entering sketch mode may change camera (align-to-plane event)

Need a helper that converts sketch coordinates to screen coordinates:
```js
export async function clickAtSketchCoord(page, sketchX, sketchY) {
    const screenPos = await page.evaluate(({sx, sy}) => {
        // Use sketchToScreen from sketchCoords.js
        // ... compute screen position from sketch coords
    }, {sx: sketchX, sy: sketchY});
    await page.mouse.click(screenPos.x, screenPos.y);
}
```

This would require exposing `sketchToScreen()` through `__waffle`, or computing it inside page.evaluate.

**b) Entity-aware click helper**

For the select tool, we need to click on specific entities. Add a helper:
```js
export async function clickOnEntity(page, entityId) {
    const screenPos = await page.evaluate((id) => {
        const pos = window.__waffle.getPositions().get(id);
        if (!pos) return null;
        // Convert sketch coords to screen
    }, entityId);
    if (screenPos) await page.mouse.click(screenPos.x, screenPos.y);
}
```

### Priority 3: Add Missing data-testid Attributes

Currently missing `data-testid` on:
- Constraint buttons in toolbar: **DONE** (have `toolbar-constraint-{id}`)
- Dimension tool button: **DONE** (has `toolbar-btn-dimension`)
- Box select rectangle overlay: MISSING
- Profile highlight regions: N/A (Three.js)
- Feature tree items: LIKELY MISSING (need to verify)

### Priority 4: Constraint Testing Helpers

Add helpers for the constraint workflow:
```js
// helpers/constraint.js
export async function getApplicableConstraints(page) {
    // Returns which constraints are enabled for current selection
}
export async function clickConstraintButton(page, constraintId) {
    await page.locator(`[data-testid="toolbar-constraint-${constraintId}"]`).click();
}
export async function isConstraintEnabled(page, constraintId) {
    const btn = page.locator(`[data-testid="toolbar-constraint-${constraintId}"]`);
    return !(await btn.isDisabled());
}
```

### Priority 5: Dimension Tool Testing Helpers

Add helpers for the dimension workflow:
```js
// helpers/dimension.js
export async function getDimensionPopupState(page) {
    return page.evaluate(() => window.__waffle.getDimensionPopup());
}
export async function applyDimension(page, value) {
    // Click the dimension input, type value, press Enter
    const input = page.locator('.dimension-input');
    await input.fill(String(value));
    await page.keyboard.press('Enter');
}
```

---

## 6. Test Patterns

### Pattern 1: GUI-First with State Verification

The preferred pattern for new tests. Interact through real GUI events, verify through `__waffle` API.

```js
test('draw line creates correct entities', async ({ waffle }) => {
    // GUI action: enter sketch, select tool
    await clickSketch(waffle.page);
    // Tool is 'line' by default after sketch entry

    // GUI action: draw line via canvas clicks
    await drawLine(waffle.page, -100, 0, 100, 0);

    // State verification: check entity creation
    await waitForEntityCount(waffle.page, 3); // 2 points + 1 line
    const entities = await getEntities(waffle.page);
    expect(entities.filter(e => e.type === 'Line')).toHaveLength(1);
    expect(entities.filter(e => e.type === 'Point')).toHaveLength(2);
});
```

### Pattern 2: Programmatic Setup, GUI Verification

When setup is complex, use `__waffle` API for setup, then verify GUI response.

```js
test('constraint button enables when line is selected', async ({ waffle }) => {
    // Programmatic setup: enter sketch, create entities
    await waffle.page.evaluate(() => {
        window.__waffle.enterSketch([0, 0, 0], [0, 0, 1]);
        // ... manually add entities if needed
    });

    // GUI action: select entity, check toolbar state
    // ... click on entity ...

    // GUI verification: H button should be enabled
    const hBtn = waffle.page.locator('[data-testid="toolbar-constraint-horizontal"]');
    await expect(hBtn).not.toBeDisabled();
});
```

### Pattern 3: End-to-End Workflow

Complete user journeys testing the full stack.

```js
test('sketch rectangle → extrude → 3D box', async ({ waffle }) => {
    await clickSketch(waffle.page);
    await clickRectangle(waffle.page);
    await drawRectangle(waffle.page, -80, -60, 80, 60);
    await waitForEntityCount(waffle.page, 8);
    await clickFinishSketch(waffle.page);
    await waitForFeatureCount(waffle.page, 1);
    await clickExtrude(waffle.page);
    await waffle.page.locator('[data-testid="extrude-depth"]').fill('10');
    await waffle.page.locator('[data-testid="extrude-apply"]').click();
    await waitForFeatureCount(waffle.page, 2);
    expect(await hasMeshWithGeometry(waffle.page)).toBe(true);
});
```

### Pattern 4: Screenshot Regression

For visual states that can't be programmatically verified.

```js
test('snap indicator appears near point', async ({ waffle }) => {
    // Setup: enter sketch, draw point at origin
    // Action: move cursor near point
    // Verify: screenshot comparison (fragile but only option for visual state)
    await assertScreenshot(waffle.page, 'snap-indicator-active.png');
});
```

### Anti-Pattern: Pure API Tests

Tests that use ONLY `__waffle` API with zero GUI interaction. These test the store/engine but NOT the GUI:

```js
// BAD: Tests nothing about the GUI
test('selectRef updates selectedRefs', async ({ waffle }) => {
    await waffle.page.evaluate((ref) => window.__waffle.selectRef(ref), someRef);
    const refs = await waffle.page.evaluate(() => window.__waffle.getSelectedRefs());
    expect(refs).toHaveLength(1);
});
```

These should be labeled as "integration tests" and separated from GUI tests.

---

## 7. Classification: Genuine GUI Tests vs API-Bypass Tests

### Tests That Genuinely Test GUI Behavior (48 tests)

- `gui/viewport.spec.js` (7): Canvas rendering, orbit drag, zoom wheel, fit-all key
- `gui/sketch-entry.spec.js` (6): Button clicks, keyboard shortcut S, toolbar state
- `gui/sketch-draw.spec.js` (8): Canvas clicks to draw shapes, keyboard shortcuts
- `gui/sketch-finish.spec.js` (4): Finish Sketch button, toolbar mode switch
- `gui/extrude.spec.js` (6): Dialog open/close, depth input, Apply/Cancel buttons, Enter/Escape keys
- `gui/workflow.spec.js` (4): Full mouse + keyboard workflows
- `gui/baselines/screenshot-suite.spec.js` (18): Visual regression screenshots
- Trace suite has 15 tests but only verifies "no crash" — low value

### Tests That Bypass GUI (API-Only, ~30 tests)

- `gui/datum-planes.spec.js` (10): All use `page.evaluate(() => window.__waffle.selectRef(...))`
- `gui/selection/edge-pick.spec.js` (5): All use `page.evaluate(() => window.__waffle.selectRef(...))`
- `gui/selection/box-select.spec.js` (5): All use `page.evaluate(() => window.__waffle.selectRef(...))`
- `gui/selection/select-other.spec.js` (5): All use `page.evaluate(() => window.__waffle.selectRef(...))`
- `gui/viewport-advanced.spec.js` hover tests (3): Use `page.evaluate(() => window.__waffle.setHoveredRef(...))`

### Tests with Mixed GUI+API (Legacy, ~14 tests)

- `smoke.spec.js` (4): DOM checks + API
- `sketch.spec.js` (6): API setup + canvas clicks
- `pipeline.spec.js` (4): API setup + canvas clicks + API verification

---

## 8. Untested Areas Requiring New Tests

### Critical Gaps (High Priority)

1. **Constraint toolbar buttons**: No test verifies clicking H/V/Co/Perp/Par/Eq/Tan/Mid/Fix buttons
2. **Dimension tool workflow**: No test covers click entity → popup → type value → Enter → constraint created
3. **Snap behavior**: No test verifies snap detection affects entity placement (coincident reuse, H/V alignment)
4. **Construction toggle**: No test for X key or Construction button toggling entity flag
5. **Profile selection**: No test for clicking inside a closed profile region
6. **Line chaining**: No test verifies continuous line drawing (end of line A = start of line B)
7. **Arc drawing**: No test for the 3-click arc tool workflow
8. **Undo/Redo**: No GUI test for Ctrl+Z/Ctrl+Shift+Z
9. **Feature tree interaction**: No test for delete, suppress, reorder, rename features via GUI
10. **Right-click constraint menu**: No test for context menu constraint application

### Medium Priority

11. **Revolve dialog**: Tests exist for toolbar click + dialog wait, but no end-to-end revolve test
12. **Save/Load via GUI**: Pipeline test uses API; no test clicks Save/Open buttons
13. **Export STL via GUI**: No GUI test
14. **Over-constraint visual feedback**: No test verifying red color on over-constrained entities
15. **Sketch plane alignment**: No test verifying camera aligns to sketch plane on entry
16. **Dimension label editing**: No test for clicking a dimension label to edit its value

### Lower Priority

17. **View cube buttons**: Not tested
18. **Context menu on viewport**: Not tested
19. **Multiple sketch + extrude cycles**: Only 1 sketch→extrude tested; no multi-feature workflow
20. **Sketch-on-face via GUI**: Pipeline test uses API; no test that actually clicks a face then clicks Sketch
