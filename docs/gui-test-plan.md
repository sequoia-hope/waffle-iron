# GUI Test Plan: Core Workflows

## Overview

This test plan covers the 5 core parametric CAD workflows exercised through
real GUI events (mouse clicks, keyboard shortcuts, dialog interaction). Each
workflow is decomposed into a test matrix showing all state combinations.

**Test naming convention**: `W{workflow}-{area}-{case}` (e.g. `W1-entry-default-plane`)

**Verification strategy**: All tests use GUI events for actions and `__waffle` API
for state verification (since sketch entities and 3D meshes render in WebGL, not DOM).

---

## Workflow 1: Start a Sketch

### State Space

Entry into sketch mode depends on:
- **Pre-selection**: nothing | datum XY | datum XZ | datum YZ | solid face
- **Entry method**: toolbar button click | S keyboard shortcut
- **Already in sketch?**: no | yes (toggling exits)

### Test Matrix

| Test ID | Pre-selection | Entry Method | Expected Plane | Expected Tool | Notes |
|---------|--------------|--------------|----------------|---------------|-------|
| W1-entry-btn-nothing | None | Button click | XY (default) | line | Default fallback |
| W1-entry-key-nothing | None | S key | XY (default) | line | Keyboard shortcut |
| W1-entry-btn-xy | Datum XY (via API) | Button click | origin=[0,0,0] normal=[0,0,1] | line | Selected plane used |
| W1-entry-btn-xz | Datum XZ (via API) | Button click | origin=[0,0,0] normal=[0,1,0] | line | |
| W1-entry-btn-yz | Datum YZ (via API) | Button click | origin=[0,0,0] normal=[1,0,0] | line | |
| W1-entry-key-xz | Datum XZ (via API) | S key | origin=[0,0,0] normal=[0,1,0] | line | Key shortcut + plane |
| W1-entry-face | Solid face (requires extrusion) | Button click | Face plane (computed) | line | Sketch-on-face |
| W1-toggle-exit | Already in sketch | Button click (Sketch toggle) | Sketch exits | select | Sketch button toggles mode |
| W1-toggle-key | Already in sketch | S key | Sketch finishes | select | S key also toggles |

### Preconditions & Postconditions

**Precondition**: Engine ready (status dot green).

**Postconditions to verify**:
1. `sketchMode.active === true`
2. `sketchMode.origin` and `sketchMode.normal` match expected plane
3. `activeTool === 'line'` (default tool after entry)
4. Toolbar shows sketch tools (Line, Rect, Circle, Arc, Construction, Finish Sketch)
5. Toolbar hides modeling tools (Extrude, Revolve, Fillet, Chamfer, Shell)
6. Constraint buttons appear (H, V, Co, Perp, Par, Eq, Tan, Mid, Fix)

### GUI Actions

```
Entry via button:     Click [data-testid="toolbar-btn-sketch"]
Entry via key:        page.keyboard.press('s')
Exit via button:      Click [data-testid="toolbar-btn-finish-sketch"]
Exit via Escape:      Press Escape (from select tool)
```

---

## Workflow 2: Draw Basic Shapes

### State Space

Drawing depends on:
- **Tool**: line | rectangle | circle | arc
- **Activation**: toolbar button | keyboard shortcut
- **Snap scenario**: no snap | coincident | horizontal | vertical | on-entity | tangent | perpendicular
- **Chaining**: first shape | continuation from previous endpoint

### Test Matrix — Shape Drawing

| Test ID | Tool | Activation | Action | Expected Entities | Expected Constraints |
|---------|------|-----------|--------|-------------------|---------------------|
| W2-line-btn-draw | line | Line button | 2 clicks | 2 Points + 1 Line | none |
| W2-line-key-draw | line | L key | 2 clicks | 2 Points + 1 Line | none |
| W2-line-chain | line | default | 3 clicks (chained) | 3 Points + 2 Lines | none (shared endpoint) |
| W2-rect-btn-draw | rectangle | Rect button | 2 corner clicks | 4 Points + 4 Lines | 2H + 2V auto |
| W2-rect-key-draw | rectangle | R key | 2 corner clicks | 4 Points + 4 Lines | 2H + 2V auto |
| W2-circle-btn-draw | circle | Circle button | center + edge clicks | 1 Point + 1 Circle | none |
| W2-circle-key-draw | circle | C key | center + edge clicks | 1 Point + 1 Circle | none |
| W2-arc-btn-draw | arc | Arc button | center + start + end clicks | 3 Points + 1 Arc | none |
| W2-arc-key-draw | arc | A key | center + start + end clicks | 3 Points + 1 Arc | none |

### Test Matrix — Tool Switching

| Test ID | Initial Tool | Action | Expected Tool |
|---------|-------------|--------|---------------|
| W2-switch-line-key | select | Press L | line |
| W2-switch-rect-key | select | Press R | rectangle |
| W2-switch-circle-key | select | Press C | circle |
| W2-switch-arc-key | select | Press A | arc |
| W2-switch-dim-key | select | Press D | dimension |
| W2-switch-escape | line | Press Escape | select |
| W2-switch-escape-twice | line | Escape + Escape | finishes sketch |
| W2-switch-select-btn | line | Click Select button | select |

### Test Matrix — Snap Behavior

**Note**: These require exposing `getSnapIndicator()` through `__waffle` (see tooling improvements).

| Test ID | Setup | Cursor Near | Expected Snap | Expected Constraint |
|---------|-------|-------------|---------------|-------------------|
| W2-snap-coincident | Point exists at (1,0) | Move near (1,0) | Coincident indicator | Reuses existing point ID |
| W2-snap-horizontal | Drawing from (0,0) | Move to (~2, 0.01) | Horizontal indicator | Auto H constraint on line |
| W2-snap-vertical | Drawing from (0,0) | Move to (~0.01, 2) | Vertical indicator | Auto V constraint on line |
| W2-snap-on-line | Line exists | Move near midpoint | On-entity indicator | none (snap position only) |
| W2-snap-on-circle | Circle exists | Move near circumference | On-entity indicator | none (snap position only) |

### Test Matrix — Construction Toggle

| Test ID | Selection | Action | Expected |
|---------|-----------|--------|----------|
| W2-constr-toggle-key | 1 Line selected | Press X | entity.construction = true |
| W2-constr-toggle-btn | 1 Line selected | Click Construction button | entity.construction = true |
| W2-constr-toggle-back | 1 construction Line | Press X | entity.construction = false |
| W2-constr-no-select | Nothing selected | Press X | No change (no-op) |

### Preconditions

- Engine ready, sketch mode active (from W1)
- Canvas visible and events attached

### Postconditions to verify

1. Entity count matches expected (via `getEntityCount()`, `getEntityCountByType()`)
2. Entity types correct (Point, Line, Circle, Arc)
3. For rectangles: 4 auto-constraints created (2 Horizontal, 2 Vertical)
4. For chained lines: endpoint of line N = startpoint of line N+1 (shared ID)
5. Active tool matches expected after switching

---

## Workflow 3: Accept/Cancel a Sketch

### State Space

Finishing a sketch depends on:
- **Sketch content**: empty | points only | open lines | closed profile | multiple profiles
- **Exit method**: Finish Sketch button | Escape (from select tool) | S key toggle
- **Profile presence**: 0 profiles | 1 profile | 2+ profiles

### Test Matrix

| Test ID | Sketch Content | Exit Method | Expected |
|---------|---------------|-------------|----------|
| W3-finish-btn-rect | 4 lines (rectangle) | Finish Sketch button | Sketch feature created, 1 profile |
| W3-finish-esc | 4 lines (rectangle) | Escape→Escape | Sketch feature created |
| W3-finish-skey | 4 lines (rectangle) | S key | Sketch feature created (toggles) |
| W3-finish-empty | No entities | Finish Sketch button | Sketch mode exits, no feature? |
| W3-finish-open | 2 lines (L shape) | Finish Sketch button | Sketch feature created, 0 profiles |
| W3-finish-multi | 2 rectangles | Finish Sketch button | Sketch feature created, 2 profiles |

### Postconditions to verify

1. `sketchMode.active === false`
2. `activeTool === 'select'`
3. Feature tree has Sketch feature (or not, for empty sketch)
4. Toolbar returns to modeling tools
5. Constraint buttons hidden
6. Orbit controls re-enabled (orbit drag works)
7. Profile count matches expected

### GUI State Transitions

```
[Sketch Active, line tool]
  |-- Escape → [Sketch Active, select tool]
  |                |-- Escape → [Not in Sketch] (finishSketch called)
  |                |-- Finish Sketch btn → [Not in Sketch]
  |-- Finish Sketch btn → [Not in Sketch]
  |-- S key → [Not in Sketch] (handleToolClick('sketch') toggles)
```

---

## Workflow 4: Extrude a Sketch Profile

### State Space

Extrude depends on:
- **Prerequisite**: finished sketch with at least 1 profile
- **Dialog interaction**: set depth + Apply | set depth + Enter | Cancel | Escape
- **Depth value**: positive number | zero | negative | non-numeric

### Test Matrix

| Test ID | Entry | Depth | Confirm | Expected |
|---------|-------|-------|---------|----------|
| W4-extrude-btn-apply | Extrude button | 10 | Apply button | Extrude feature + 3D mesh |
| W4-extrude-key-apply | E key | 10 | Apply button | Extrude feature + 3D mesh |
| W4-extrude-enter | Extrude button | 5 | Enter key | Extrude feature + 3D mesh |
| W4-extrude-cancel-btn | Extrude button | 10 | Cancel button | No new feature |
| W4-extrude-cancel-esc | Extrude button | 10 | Escape key | No new feature |
| W4-extrude-default | Extrude button | (default value) | Apply button | Extrude feature |
| W4-extrude-no-sketch | No sketch in tree | Extrude button | n/a | No dialog shown |

### Prerequisites

1. Completed sketch with closed profile (rectangle, circle, etc.)
2. Sketch feature exists in feature tree

### Postconditions to verify

1. Feature tree: Sketch + Extrude features (2 total)
2. 3D mesh generated with `triangleCount > 0`
3. Mesh has `faceRanges` with valid `geom_ref` entries
4. Extruded box should have >= 6 face ranges (6 faces for rectangular prism)
5. Each face range has `kind.type === 'Face'`, valid anchor and selector
6. Dialog dismissed after Apply/Cancel
7. `extrudeDialogState === null` after completion

### GUI Actions

```
Open dialog:    Click [data-testid="toolbar-btn-extrude"] OR press E key
Set depth:      Fill [data-testid="extrude-depth"] with value
Apply:          Click [data-testid="extrude-apply"] OR press Enter
Cancel:         Click [data-testid="extrude-cancel"] OR press Escape
```

---

## Workflow 5: Create Second Sketch on Face of Extrusion

### State Space

Sketch-on-face depends on:
- **Face selection**: top face | bottom face | side face
- **Selection method**: click face on model | programmatic via API
- **Entry**: toolbar button | S key

### Test Matrix

| Test ID | Face | Selection | Entry | Expected Plane |
|---------|------|-----------|-------|---------------|
| W5-face-top-btn | Top face (EndCapPositive) | Click model (or API) | Button | normal ≈ [0,0,1], origin on top surface |
| W5-face-side-btn | Side face | Click model (or API) | Button | normal perpendicular to side |
| W5-face-key | Top face | API (click too flaky) | S key | normal ≈ [0,0,1] |
| W5-face-draw | Top face sketch | Button | Draw rectangle | 4 Points + 4 Lines on face plane |
| W5-face-extrude | Two sketches + one extrude | Full workflow | Second extrude | 3 features total |

### Prerequisites

1. Completed W4 (Sketch + Extrude = solid model visible)
2. Model has pickable faces with `faceRanges` and `geom_ref`

### Postconditions to verify

1. `sketchMode.active === true`
2. `sketchMode.origin` matches face centroid (approximately)
3. `sketchMode.normal` matches face normal (unit vector, may be negated)
4. Normal is NOT [0,0,1] for side faces (validates non-XY plane)
5. Drawing on the new plane creates entities in the correct coordinate space

### Implementation Notes

Face clicking is the hardest test to make reliable because:
- It requires actual 3D geometry to exist and be rendered
- Raycaster must intersect with the correct face
- Camera angle affects which face is visible
- SwiftShader rendering may differ from GPU

**Recommended approach**: Use programmatic `selectRef()` for face selection, then test that clicking Sketch button uses the selected face plane. This is a compromise between API-bypass and full GUI testing. Alternatively, use camera positioning to guarantee a specific face is visible before clicking.

---

## Workflow 6: Constraint Application (Bonus — supports Task #4)

### Test Matrix — Toolbar Constraints

| Test ID | Selection | Constraint | Button | Expected |
|---------|-----------|-----------|--------|----------|
| W6-h-one-line | 1 Line | Horizontal | toolbar-constraint-horizontal | constraint.type === 'Horizontal' |
| W6-v-one-line | 1 Line | Vertical | toolbar-constraint-vertical | constraint.type === 'Vertical' |
| W6-co-two-points | 2 Points | Coincident | toolbar-constraint-coincident | constraint.type === 'Coincident' |
| W6-perp-two-lines | 2 Lines | Perpendicular | toolbar-constraint-perpendicular | constraint.type === 'Perpendicular' |
| W6-par-two-lines | 2 Lines | Parallel | toolbar-constraint-parallel | constraint.type === 'Parallel' |
| W6-eq-two-lines | 2 Lines | Equal | toolbar-constraint-equal | constraint.type === 'Equal' |
| W6-tan-line-arc | 1 Line + 1 Arc | Tangent | toolbar-constraint-tangent | constraint.type === 'Tangent' |
| W6-mid-point-line | 1 Point + 1 Line | Midpoint | toolbar-constraint-midpoint | constraint.type === 'Midpoint' |
| W6-fix-one-point | 1 Point | Fix | toolbar-constraint-fix | constraint.type === 'WhereDragged' |

### Test Matrix — Constraint Button Enable/Disable

| Test ID | Selection | Button | Expected State |
|---------|-----------|--------|---------------|
| W6-dis-h-no-sel | Nothing | horizontal | disabled |
| W6-dis-h-point | 1 Point | horizontal | disabled |
| W6-en-h-line | 1 Line | horizontal | enabled |
| W6-dis-co-1pt | 1 Point | coincident | disabled |
| W6-en-co-2pt | 2 Points | coincident | enabled |
| W6-dis-perp-1line | 1 Line | perpendicular | disabled |
| W6-en-perp-2line | 2 Lines | perpendicular | enabled |

### Test Matrix — Dimension Tool

| Test ID | Click Target | Expected Popup | Type Value | Expected Constraint |
|---------|-------------|----------------|-----------|-------------------|
| W6-dim-line | 1 Line | distance popup with line length | 5.0 | Distance(start, end, 5.0) |
| W6-dim-circle | 1 Circle | radius popup | 3.0 | Radius(circle, 3.0) |
| W6-dim-pt-pt | Point A then Point B | distance popup | 2.5 | Distance(A, B, 2.5) |
| W6-dim-pt-line | Point then Line | distance popup | 1.0 | Distance(point, line, 1.0) |
| W6-dim-cancel | 1 Line | distance popup | (Escape) | No constraint added |
| W6-dim-blur | 1 Line | distance popup | (click away) | Popup dismissed |

### Preconditions

- Sketch mode active with entities already drawn
- Selection set appropriately for each test case

### Postconditions to verify

1. Constraint count increases by 1 after application
2. Constraint type matches expected
3. Constraint references correct entity IDs
4. For dimension constraints: value matches typed value
5. Button state (enabled/disabled) matches selection composition
6. DOF decreases after constraint (if solver status exposed)

---

## Workflow 7: Undo/Redo (Bonus)

| Test ID | Action | Undo Key | Expected |
|---------|--------|----------|----------|
| W7-undo-add-entity | Draw a line | Ctrl+Z | Entity removed |
| W7-undo-add-feature | Extrude | Ctrl+Z | Extrude feature removed |
| W7-redo | Undo line draw | Ctrl+Shift+Z | Line restored |
| W7-undo-constraint | Apply H constraint | Ctrl+Z | Constraint removed |

---

## Test Execution Dependencies

```
W1 (Start Sketch) ← independent, always runs first
  |
  v
W2 (Draw Shapes) ← requires W1 to enter sketch mode
  |
  v
W3 (Finish Sketch) ← requires W2 to have drawn something
  |
  v
W4 (Extrude) ← requires W3 to have a Sketch feature
  |
  v
W5 (Sketch-on-Face) ← requires W4 to have a 3D solid
```

W6 (Constraints) can run independently after W1+W2.
W7 (Undo/Redo) can run independently after W1.

---

## Test Infrastructure Requirements

### Before implementing these tests, need:

1. **Expose snap state in `__waffle`** — for W2 snap tests
2. **Expose solve status in `__waffle`** — for W6 DOF verification
3. **Add sketch-coordinate click helper** — for reliable entity picking in select tool
4. **Add constraint helper functions** — `clickConstraintButton()`, `isConstraintEnabled()`
5. **Add dimension helper functions** — `getDimensionPopupState()`, `applyDimension()`

### Implementation priority for Task #4:

1. W6 constraint toolbar tests (highest value — completely untested)
2. W6 dimension tool tests (completely untested)
3. W2 snap tests (requires tooling improvement)
4. W2 arc drawing test (simple gap fill)
5. W2 construction toggle tests (simple gap fill)
6. W3 finish variants (empty sketch, open profile)

---

## Total Test Count

| Workflow | Test Count |
|----------|-----------|
| W1 Start Sketch | 9 |
| W2 Draw Shapes | 22 |
| W3 Finish Sketch | 6 |
| W4 Extrude | 7 |
| W5 Sketch-on-Face | 5 |
| W6 Constraints | 21 |
| W7 Undo/Redo | 4 |
| **Total** | **74** |
