# GUI Test Patterns

## Test Files

| File | Purpose |
|------|---------|
| `sketch-drawing-regression.spec.js` | **Canary** — simplest possible drawing tests. Run before any sketch commit. |
| `sketch-draw-diagnostic.spec.js` | Full pipeline instrumentation — diagnoses WHERE drawing breaks. |
| `sketch-draw.spec.js` | Original drawing tests (entity counts + keyboard shortcuts). |

## Drawing Test Helpers

### Click-click (in `helpers/canvas.js`)
- `drawLine(page, x1, y1, x2, y2)` — two clicks
- `drawRectangle(page, x1, y1, x2, y2)` — two corner clicks
- `drawCircle(page, cx, cy, rx, ry)` — center + edge click

### Click-drag (in `helpers/canvas.js`)
- `dragLine(page, x1, y1, x2, y2)` — mousedown → move → mouseup
- `dragRectangle(page, x1, y1, x2, y2)` — drag from corner to corner
- `dragCircle(page, cx, cy, rx, ry)` — drag from center to edge

### State Inspection (in `helpers/state.js`)
- `getToolState(page)` — internal tool state machine ('idle', 'firstPointPlaced', etc.)
- `getDrawingState(page)` — full snapshot: toolState, isDragging, positions
- `getToolEventLog(page)` — ring buffer of last 50 events reaching the handler
- `clearToolEventLog(page)` — reset event log before a test sequence
- `waitForToolState(page, expected)` — wait for state machine transition

## Anti-patterns

### Do NOT swallow assertion errors
```javascript
// BAD — hides the real failure
try {
    await waitForEntityCount(page, 3, 3000);
} catch {
    await waffle.dumpState('failed');
}
// subsequent assertions fail with "expected 3, got 0" — but WHY?

// GOOD — let the timeout throw directly
await waitForEntityCount(page, 3, 5000);
```

### Do NOT use API bypasses for drawing tests
```javascript
// BAD — tests the API, not the drawing pipeline
await page.evaluate(() => {
    window.__waffle.addSketchEntity({ type: 'Point', id: 1, x: 0, y: 0 });
});

// GOOD — tests real pointer events
await clickAt(page, 0, 0);
```

### Do NOT test only entity counts
```javascript
// WEAK — knows drawing failed but not why
const count = await getEntityCount(page);
expect(count).toBe(3);

// STRONG — reveals exactly where the pipeline broke
const state = await getToolState(page);
expect(state).toBe('firstPointPlaced'); // did first click register?
const log = await getToolEventLog(page);
expect(log.filter(e => e.event === 'pointerdown').length).toBe(2); // did events arrive?
```
