# Manual Snap-Click Verification Test

Step-by-step instructions for verifying snap click behavior in the browser.

## Prerequisites

1. Start the dev server:
   ```bash
   cd app && npm run dev
   ```
   This runs on port 8083.

2. Open `http://localhost:8083` in Chrome or Firefox.

3. Open DevTools (F12) and keep the Console tab visible.

---

## Test A -- Origin Click

1. Click the **Sketch** toolbar button.
2. Pick the **XY plane** (or click the quick-pick XY button in the SketchPlaneDialog).
3. Activate the **Line tool** (click it in the toolbar or press `L`).
4. Click at the **origin crosshair center** -- the white dot where the axes cross.
5. Verify in the console:
   ```js
   __waffle.getToolState()
   ```
   - **Expected:** returns `'firstPointPlaced'`
   - If it returns `'idle'`, the click did not register.

---

## Test B -- Endpoint Snap Click

1. Draw a line from left to right (two clicks, well away from the origin).
2. Press `Escape` to end line chaining.
3. Press `L` to re-activate the line tool.
4. Hover near the **right endpoint** of the line you just drew.
5. Observe that a green **"Coincident"** label appears with a green snap dot.
6. Click on the snap indicator (green dot).
7. Verify in the console:
   ```js
   __waffle.getToolState()
   ```
   - **Expected:** returns `'firstPointPlaced'`

---

## Test C -- DOM Layer Debug

1. Hover near any snap point so the snap indicator is showing.
2. Note the mouse coordinates from DevTools (hover over the page and observe the mouse position in the bottom-left of the Elements panel, or use the mousemove listener below):
   ```js
   document.addEventListener('mousemove', e => window._mx = e.clientX, window._my = e.clientY);
   ```
3. Run in the console:
   ```js
   document.elementsFromPoint(_mx, _my).map(el => ({
     tag: el.tagName,
     class: el.className,
     pe: getComputedStyle(el).pointerEvents
   }))
   ```
4. Verify:
   - The `<canvas>` element is present in the returned list.
   - No element above the canvas has `pointer-events: auto` except expected interactive elements like `.dim-label`.

---

## Test D -- Tool Event Log

1. In the console, clear the log:
   ```js
   __waffle.clearToolEventLog()
   ```
2. Click on any snap point (origin or entity endpoint).
3. Read the log:
   ```js
   __waffle.getToolEventLog()
   ```
4. Verify:
   - The log should contain an entry with `event: 'pointerdown'`.
   - If there is **no** `pointerdown` entry, the click was intercepted by a DOM overlay before reaching the sketch event handler.

---

## Test E -- Multiple Rapid Clicks

1. Enter sketch mode and select the **Line tool**.
2. Click rapidly **5 times** on different positions across the canvas.
3. Verify:
   - **4 line segments** are created (the first click places the start point, then each subsequent click creates a segment via chaining).
   - Check the entity count:
     ```js
     __waffle.getEntities().length
     ```
     This should show multiple points and lines.

---

## Expected Results After Fix

- All clicks on snap indicators should register **100% of the time**.
- The snap indicator appearing on hover proves the cursor position is detected correctly.
- If hover works but click does not, the issue is in the pointerdown event path.
- The fix moves pointerdown from canvas-only to `window` (with bounds check), which bypasses any DOM overlays that intercept canvas events.

---

## Troubleshooting

If clicks still fail after the fix, investigate the following:

1. **Check the tool event log:**
   ```js
   __waffle.getToolEventLog()
   ```
   Does a `pointerdown` entry appear? If not, the event never reached the handler.

2. **Check the drawing state:**
   ```js
   __waffle.getDrawingState()
   ```
   Is `toolState` set to `'idle'` when the click fires? If so, the tool was not in the expected state.

3. **Check OrbitControls capture:**
   OrbitControls calls `setPointerCapture()` on pointerdown, which can steal subsequent events. Verify that the sketch handler fires before OrbitControls captures the pointer.

### Common Issues

| Symptom | Likely Cause |
|---------|-------------|
| Hover works, click does not | Threlte interactivity plugin processes pointerdown before the sketch handler |
| Click registers inconsistently | SketchPlane mesh raycast intercepts clicks (should have `raycast={() => {}}` to disable) |
| Click works everywhere except near labels | DimensionLabels HTML wrapper has `pointer-events: auto`, blocking clicks to the canvas underneath |
| pointerdown fires but tool state unchanged | The event coordinates map to an unexpected sketch-plane position, or `isDragging` was not reset |
