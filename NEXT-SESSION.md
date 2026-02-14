# Next Session: Fix Sketch Drawing Bug + Remaining Work

## Priority 1: Fix Sketch Drawing Bug (all tools broken)

### Root Cause

The `isDragging` flag in `tools.js` is never reset between click-click interactions. When a user:

1. Clicks (pointerdown) — creates first point, sets `isDragging = false`, stores `pointerDownPos`
2. Moves mouse to second position (pointermove) — if distance > 5px threshold, sets `isDragging = true`
3. Clicks again (pointerdown) — checks `toolState === 'firstPointPlaced' && !isDragging` — **FAILS** because `isDragging` is still `true`

The second click never calls `finalizeLine()` / `finalizeRectangle()` / `finalizeCircle()`.

### Fix

In `app/src/lib/sketch/tools.js`, reset `isDragging` and `pointerDownPos` on each new pointerdown in the `firstPointPlaced` / `firstCornerPlaced` / `centerPlaced` states. All four tools have this bug:

**Line tool** — `handleLineTool()` around line 238:
```javascript
} else if (toolState === 'firstPointPlaced' && !isDragging) {
```
The `!isDragging` guard is correct for preventing finalization mid-drag, but `isDragging` needs to be reset on each new pointerdown. Add at the start of the pointerdown handler:
```javascript
if (eventType === 'pointerdown') {
    // Reset drag state on each new click
    isDragging = false;
    pointerDownPos = { x: snap.x, y: snap.y };
    // ... rest of handler
}
```

**Same pattern in:** `handleRectangleTool()`, `handleCircleTool()`, `handleArcTool()`

### Verification

After fixing, run:
```bash
npx playwright test tests/gui/sketch-drawing-regression.spec.js --reporter=list
```
All 6 tests should pass (click-click and click-drag for line, rectangle, circle).

Then run the diagnostic suite:
```bash
npx playwright test tests/gui/sketch-draw-diagnostic.spec.js --reporter=list
```
All 12 tests should pass.

---

## Priority 2: Other Known Issues

### Remaining Critical Gaps (from REVIEW.md)

| Issue | Status | Details |
|-------|--------|---------|
| **Feature creation dialogs** | Partial | Extrude + revolve dialogs exist; fillet/chamfer/shell/boolean dialogs missing |
| **TruckKernel fillet/chamfer/shell** | NotSupported | `truck_kernel.rs` lines 222-253 return `NotSupported` |
| **STL export in browser** | Missing | Engine has ExportStl message but needs WASM implementation |
| **Units system** | Missing | All dimensions are unitless floats |

### UI Polish

| Issue | Details |
|-------|---------|
| Store.svelte.js size | ~720 lines, could be split into modules |
| Tangent/perpendicular snap | Only coincident + H/V snaps exist |
| WASM binary size | 1.9MB, could add wasm-opt |

### Test Gaps

| Gap | Recommendation |
|-----|---------------|
| TruckKernel integration tests | Run modeling-ops tests against real kernel where supported |
| WASM-in-browser tests | Current tests run native only |
| GeomRef with real topology | Only tested with MockKernel |

---

## File Reference

| File | What to Change |
|------|---------------|
| `app/src/lib/sketch/tools.js` | Fix isDragging reset bug in all 4 tool handlers |
| `app/tests/gui/sketch-drawing-regression.spec.js` | Should pass after fix (canary test) |
| `app/tests/gui/sketch-draw-diagnostic.spec.js` | Should pass after fix (detailed diagnostics) |
