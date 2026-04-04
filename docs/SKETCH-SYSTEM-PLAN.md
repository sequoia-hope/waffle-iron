# Sketch System Development Plan

## Current State

Waffle Iron has a working sketch system built on **libslvs** (SolvSpace's constraint solver compiled to WASM). The solver itself is capable — it supports 10 entity types and 38 constraint types. But the application layer only exposes a fraction of that capability.

### What works today

**Entities**: Point, Line, Circle, Arc
**Drawing tools**: Line (click-click + click-drag + chaining), Rectangle (4 lines + H/V constraints), Circle (center + radius), Arc (center + start + end), Select, Dimension
**Constraints exposed in UI** (right-click menu, 11 of 38 available): Coincident, Horizontal, Vertical, Parallel, Perpendicular, Equal Length, Tangent, Midpoint, Fix Point, Distance, Radius
**Solver bridge** (slvs-solver.js, 21 of 38 mapped): The above 11 plus PointOnLine, PointOnCircle, Symmetric, SymmetricHoriz, SymmetricVert, Angle, Diameter, EqualRadius, PointLineDistance, LengthRatio, WhereDragged
**Snap system**: Coincident, origin, midpoint, quadrant, H/V alignment, on-entity, tangent, perpendicular — with auto-constraint application during drawing
**Profile extraction**: Half-edge minimal face detection, standalone circle detection, point-in-polygon for region selection
**Dimension labels**: Distance, radius, angle display with click-to-edit values

### What's missing

The sketch system is missing the interaction model that makes a parametric sketcher usable. Users cannot delete entities, cannot drag to reposition, cannot see which constraints exist on their sketch, cannot toggle construction geometry, and have no way to apply most of the constraints the solver already supports. The following plan addresses these gaps in priority order.

---

## Phase 1: Core Interaction (Entity manipulation)

These are the most basic interaction gaps — without them, the sketch is frustrating to use.

### 1.1 Entity Deletion

**Files**: `tools.js`, `store.svelte.js`

- Delete/Backspace key handler when entities are selected
- Remove the selected entities from `sketchEntities`
- Remove all constraints that reference deleted entity IDs (cascade delete)
- Remove points that become orphaned (not referenced by any remaining line/circle/arc)
- Re-run solver and profile extraction after deletion
- Support undo via storing pre-deletion state (or integrate with an undo system later)

### 1.2 Drag-to-Reposition Entities

**Files**: `tools.js`, `store.svelte.js`, `slvs-solver.js`

The solver already supports `WhereDragged` (100031). Implement point dragging:

- In select tool: detect mousedown on a point entity, enter drag mode
- On each mousemove during drag: update the point's position in `sketchPositions`, add a temporary `WhereDragged` constraint on the dragged point, run the solver, remove the temporary constraint
- On mouseup: finalize position, re-run solver without the drag constraint
- For line/circle/arc entities: dragging the entity body should move all constituent points (translate all points by the mouse delta, then solve)
- SolvSpace's approach: the dragged point gets 1/20th scaling in the least-squares solve so it moves most while everything else adjusts minimally. This is already built into libslvs.

### 1.3 Construction Geometry Toggle

**Files**: `tools.js`, `constraintLogic.js`, `SketchRenderer.svelte`, `store.svelte.js`

- Add a "Toggle Construction" action (keyboard shortcut `G`, matching SolvSpace convention)
- When entities are selected and `G` is pressed, flip their `construction` flag
- Construction entities already render dimmer (0x6677aa, opacity 0.5) and are excluded from profile extraction
- Also allow drawing in construction mode: a toolbar toggle that sets `construction: true` on newly created entities
- Visual indicator in toolbar showing current construction mode state

### 1.4 Constraint Visualization

**Files**: new `ConstraintIcons.svelte`, `SketchRenderer.svelte`

Currently constraints are invisible unless they have a dimension label. Users need to see geometric constraints.

- Render small icons/symbols near constrained entities:
  - **H** / **V** for horizontal/vertical constraints on lines
  - **=** for equal length between two lines
  - **||** for parallel
  - **+** for perpendicular
  - **T** for tangent
  - **O** for coincident (small dot at coincidence point)
  - **M** for midpoint
  - A pin icon for fix/WhereDragged
- Icons positioned at the midpoint of the constrained entity, offset slightly to avoid overlap
- Click on a constraint icon to select it (for deletion or editing)
- Color-code: red for driving constraints, gray for reference dimensions (future)

### 1.5 Constraint Deletion

**Files**: `store.svelte.js`, `ConstraintIcons.svelte` or `ConstraintMenu.svelte`

- Select a constraint icon or dimension label, press Delete to remove it
- Or: right-click a constraint icon to get a context menu with "Delete Constraint"
- Cascade: removing a constraint re-runs the solver (sketch becomes less constrained, DOF increases)

---

## Phase 2: Missing Constraint UI

The solver bridge already maps 21 constraints, but the right-click menu only exposes 11. Wire up the rest.

### 2.1 Angle Constraint (between two lines)

**Files**: `constraintLogic.js`, `ConstraintMenu.svelte`, `DimensionLabels.svelte`

- When 2 lines are selected, add "Angle" to the right-click menu
- Opens dimension popup with the current measured angle as default
- Renders as an arc label between the two lines with the angle value
- The solver mapping for `Angle` (100024) already exists in slvs-solver.js

### 2.2 Symmetric Constraint

**Files**: `constraintLogic.js`, `ConstraintMenu.svelte`

- When 2 points + 1 line are selected: offer "Symmetric about Line"
- When 2 points are selected and sketch has H or V axis: offer "Symmetric Horizontal" / "Symmetric Vertical"
- All three variants (Symmetric, SymmetricHoriz, SymmetricVert) are already mapped in the solver bridge

### 2.3 PointOnLine / PointOnCircle Constraints

**Files**: `constraintLogic.js`

- When 1 point + 1 line selected: add "Point on Line" (in addition to existing Midpoint and Distance)
- When 1 point + 1 circle selected: add "Point on Circle"
- Both already mapped in solver bridge

### 2.4 EqualRadius Constraint

**Files**: `constraintLogic.js`

- When 2 circles or 2 arcs (or 1 circle + 1 arc) selected: add "Equal Radius"
- Already mapped in solver bridge

### 2.5 LengthRatio Constraint

**Files**: `constraintLogic.js`, dimension popup

- When 2 lines selected: add "Length Ratio" to menu
- Opens popup for ratio value (default: compute current ratio)
- Already mapped in solver bridge

---

## Phase 3: Drawing Tool Expansion

### 3.1 Polyline / Polygon Tool

**Files**: `tools.js`

- Multi-click polyline: each click adds a vertex, double-click or close-to-start-point finishes
- Close the shape: when the last click snaps to the first point, auto-add coincident constraint
- Auto-apply H/V constraints on segments using the existing snap system
- Optional: polygon preset (enter number of sides, click center + radius)

### 3.2 Ellipse Entity

**Files**: `tools.js`, `slvs-solver.js`, `store.svelte.js`, `SketchRenderer.svelte`, `profiles.js`

The slvs solver does NOT have a native ellipse entity. Two options:

**Option A (recommended)**: Approximate with a construction — model an ellipse as a set of constrained arcs or Bezier cubics. This is fragile.

**Option B**: Use PlaneGCS instead of slvs for the solver backend (see Phase 6). PlaneGCS has native ellipse support.

**Option C**: Represent ellipses as visual-only entities that get discretized for the kernel, similar to how circles currently work (but this prevents meaningful constraints on ellipses).

Recommendation: defer ellipse to Phase 6 when/if the solver is upgraded. For now, document the limitation.

### 3.3 Spline / Bezier Curve

**Files**: `tools.js`, `slvs-solver.js`, `store.svelte.js`, `SketchRenderer.svelte`, `profiles.js`

libslvs supports `SLVS_E_CUBIC` (non-rational cubic Bezier) and `SLVS_C_CUBIC_LINE_TANGENT` (tangent to line). Implement:

- Click to place 4 control points (or click-drag for handle-style placement)
- Render as cubic Bezier using de Casteljau subdivision (64 segments for display)
- Map `SLVS_E_CUBIC` in slvs-solver.js
- Map `SLVS_C_CUBIC_LINE_TANGENT` and `SLVS_C_CURVE_CURVE_TANGENT` constraints
- Profile extraction: discretize Bezier into polyline for profile polygon (similar to arc handling)
- Kernel path: discretize to polyline points for `make_faces_from_profiles` (or add a spline-aware kernel path)

### 3.4 Slot Tool

**Files**: `tools.js`

A slot is two semicircular arcs connected by two parallel tangent lines. It's a compound tool:

- Click center-to-center (or click-drag for the long axis), then set width
- Creates: 2 arc entities + 2 line entities + coincident + tangent + equal radius + parallel + distance constraints
- Pure sugar over existing primitives — no new entity types needed

### 3.5 Fillet Tool (Sketch-level)

**Files**: `tools.js`, `store.svelte.js`

Not to be confused with 3D fillet (which is deferred). This creates a tangent arc at the intersection of two lines:

- Select two lines that share an endpoint (or whose extensions intersect)
- Enter fillet radius
- Replace the shared endpoint region with an arc entity tangent to both lines
- Trim the original lines (adjust their endpoints) and add tangent + coincident constraints
- This is a generator: modifies existing entities rather than just adding constraints

### 3.6 Trim Tool

**Files**: `tools.js`, `store.svelte.js`

- Click on a segment of an entity between two intersection points
- The clicked segment is removed; the entity is split into two shorter entities at the intersection points
- Lines split into two shorter lines sharing endpoints with intersecting geometry
- Arcs/circles split into arcs
- Add coincident constraints at the new intersection endpoints

### 3.7 Offset Tool

**Files**: `tools.js`, `store.svelte.js`

- Select a closed profile or set of connected entities
- Enter offset distance
- Create a parallel copy of the entire contour at the specified distance
- For line segments: create parallel line with distance constraint
- For arcs/circles: create concentric arc/circle with radius offset
- Add constraints to maintain the offset relationship

---

## Phase 4: Dimension and Measurement System

### 4.1 Horizontal / Vertical Distance Dimensions

**Files**: `tools.js`, `DimensionLabels.svelte`, `slvs-solver.js`

- Currently only point-to-point unsigned distance exists
- Add horizontal distance (DistanceX): constrains only the X component of the distance between two points
- Add vertical distance (DistanceY): constrains only the Y component
- Map to `SLVS_C_PT_PT_DISTANCE` with appropriate workplane axis projection, or implement as the difference between the X (or Y) coordinates of two points
- Note: slvs doesn't have a native DistanceX/DistanceY — implement by constraining the projection. Use `SLVS_C_PROJ_PT_DISTANCE` (100030) which gives signed projected distance along a line direction. Project onto the sketch X or Y axis.

### 4.2 Reference Dimensions (Non-driving)

**Files**: `store.svelte.js`, `DimensionLabels.svelte`, `slvs-solver.js`

- Any dimensional constraint should be toggleable between "driving" and "reference" mode
- Reference dimensions display the measured value but don't constrain geometry
- Visual distinction: reference dimensions render in a different color (gray or blue) with "REF" suffix
- Implementation: when a dimension is set to reference mode, remove it from the solver constraint list but keep it in the UI; recompute its displayed value from solved positions after each solve

### 4.3 Expression-based Dimensions

**Files**: `DimensionInput.svelte`, `store.svelte.js`

Allow dimension values to be expressions:

- Simple arithmetic: `20 + 5`, `50 / 2`, `sqrt(3) * 10`
- Named parameters: `width`, `height`, `bolt_radius`
- Parameter table UI: a panel where users define named parameters and their values
- When a named parameter changes, all referencing dimensions update and the solver re-runs
- Implementation: store the expression string alongside the evaluated numeric value. Re-evaluate on parameter change.

### 4.4 Dimension Label Improvements

**Files**: `DimensionLabels.svelte`

- Draggable dimension labels (reposition the label without changing the constraint)
- Leader lines from label to measured geometry
- Automatic label placement to avoid overlaps
- Display units (mm, in) with configurable precision
- Angle dimensions with arc display between the two lines

---

## Phase 5: Solver Feedback and Diagnostics

### 5.1 DOF Counter Display

**Files**: `SketchToolbar.svelte` or new `SketchStatusBar.svelte`, `store.svelte.js`

- Display current DOF prominently in the sketch toolbar/status bar
- "6 DOF remaining" → "Fully constrained" progression
- Color: green when DOF=0, yellow when DOF>0, red when over-constrained
- The solver already returns DOF — just need to display it

### 5.2 Under-constrained Point Highlighting

**Files**: `SketchRenderer.svelte`, `slvs-solver.js`

- When DOF > 0, identify which points still have freedom
- Highlight under-constrained points with a distinctive marker (e.g., cyan square, matching SolvSpace's convention)
- Implementation: after solving, for each point, temporarily add a `WhereDragged` constraint and check if the solver reports it as redundant. If redundant, the point is already fully constrained. If not, it has remaining DOF. (This is expensive — only run on explicit "Show DOF" request, not on every solve.)
- Simpler alternative: highlight all points that are not directly or transitively constrained via the constraint graph.

### 5.3 Conflicting Constraint Highlighting

**Files**: `SketchRenderer.svelte`, `store.svelte.js`

- The solver already returns `failedConstraints` (list of constraint IDs that couldn't be satisfied)
- Currently over-constrained entities render red, but specific conflicting constraints aren't identified
- Highlight the specific conflicting constraint icons/dimension labels in red
- Show a diagnostic message: "N constraints conflict — remove one to fix"
- Allow clicking a conflicting constraint to select it for deletion

### 5.4 Redundant Constraint Detection

**Files**: `slvs-solver.js`, `store.svelte.js`

- After each solve, check if any constraint is redundant (satisfied but provides no additional DOF reduction)
- libslvs returns `SLVS_RESULT_REDUNDANT_OKAY` (4) when constraints are redundant but consistent
- Highlight redundant constraints with a distinct color (orange) and tooltip "Redundant — can be removed without changing the sketch"

---

## Phase 6: Solver Upgrade Evaluation

The current libslvs solver is functional but has limitations: no ellipse entity, no B-spline constraints, limited tangent handling, and the Emscripten WASM bridge is manually marshaled at byte offsets.

### Evaluate PlaneGCS as replacement or supplement

PlaneGCS (FreeCAD's solver) offers:
- Native ellipse, hyperbola, parabola entities
- B-spline support (with control point weights)
- 40+ constraint types including all the slvs types plus conic-section constraints
- Multiple solver algorithms (DogLeg, Levenberg-Marquardt, BFGS)
- QR-based DOF analysis and redundancy detection
- An existing WASM port: `@salusoft89/planegcs` on npm

**Trade-offs:**
- PlaneGCS is larger and more complex than libslvs
- libslvs is already working and tested in the codebase
- PlaneGCS would enable ellipse, B-spline, and conic constraints that slvs cannot do
- Migration cost: rewrite slvs-solver.js to use PlaneGCS parameter/constraint model

**Recommendation:** Keep libslvs for now. Phases 1-5 don't require any capability beyond what libslvs provides. Revisit PlaneGCS only if/when ellipse or B-spline support becomes a priority. The abstraction boundary is clean (slvs-solver.js is the only file that touches libslvs), so swapping solvers later is straightforward.

---

## Phase 7: Advanced Features (Lower priority)

### 7.1 External Geometry References

Project edges from existing 3D solid geometry onto the sketch plane as fixed (immovable) construction geometry. Users can then constrain sketch entities relative to existing model edges.

### 7.2 Mirror Tool (Parametric)

Select entities + a mirror line → create mirrored copies with symmetric constraints. Copies update when the source geometry changes.

### 7.3 Linear / Circular Pattern

Repeat selected entities in a rectangular grid (rows, columns, spacing) or around a center point (count, angular span). Each copy is constrained relative to the original.

### 7.4 Sketch-level Undo/Redo

Maintain a history stack of sketch states (entities + constraints + positions). Each user action pushes a snapshot. Ctrl+Z pops and restores. This is separate from the feature-tree-level undo which doesn't currently exist either.

### 7.5 Constraint List Panel

A sidebar panel listing all constraints in the current sketch:
- Type, involved entities, value (for dimensional constraints)
- Click to highlight in viewport
- Right-click to delete, toggle driving/reference, rename
- Filter by type, by entity, by status (satisfied/conflicting/redundant)

---

## Implementation Order

| Priority | Phase | Effort | Impact |
|----------|-------|--------|--------|
| **P0** | 1.1 Entity Deletion | Small | Unblocks basic editing |
| **P0** | 1.2 Drag-to-Reposition | Medium | Fundamental interaction |
| **P0** | 1.3 Construction Toggle | Small | Needed for reference geometry |
| **P1** | 1.4 Constraint Visualization | Medium | Users can't see what constrains what |
| **P1** | 1.5 Constraint Deletion | Small | Can't remove bad constraints |
| **P1** | 2.1 Angle Constraint UI | Small | Already in solver, just needs menu entry |
| **P1** | 2.2 Symmetric Constraint UI | Small | Already in solver |
| **P1** | 2.3-2.5 Remaining constraint UIs | Small each | Already in solver |
| **P1** | 5.1 DOF Counter | Small | Critical user feedback |
| **P2** | 3.1 Polyline/Polygon Tool | Medium | Drawing efficiency |
| **P2** | 3.3 Spline/Bezier | Medium | Needed for organic shapes |
| **P2** | 3.4 Slot Tool | Small | Common mechanical feature |
| **P2** | 3.5 Sketch Fillet | Medium | Common workflow |
| **P2** | 3.6 Trim Tool | Medium | Essential editing |
| **P2** | 4.1 H/V Distance Dimensions | Small | Precision positioning |
| **P2** | 4.2 Reference Dimensions | Small | Inspection workflow |
| **P2** | 5.2-5.4 Solver diagnostics | Medium | Debugging over-constrained sketches |
| **P3** | 4.3 Expression Dimensions | Large | Parametric design |
| **P3** | 4.4 Dimension label improvements | Medium | Polish |
| **P3** | 7.1-7.5 Advanced features | Large each | Power user features |

---

## Architecture Decisions

### Keep libslvs as the solver

libslvs is working, battle-tested, and covers all constraint types needed through Phase 5. The slvs-solver.js abstraction boundary is clean — one file, ~485 lines. If a solver swap is ever needed, only this file changes.

### All sketch logic stays in JavaScript

The sketch UI, tool interactions, constraint logic, snap system, and profile extraction are all JS-side code. The Rust crates (`sketch-solver`, `kernel`) handle the 3D operations downstream. This separation is correct — do not move sketch interaction logic to Rust/WASM.

### The constraint model is the source of truth

Sketch entities define geometry. Constraints define relationships. The solver resolves positions. This flow is one-directional: entities + constraints → solver → positions. Never write solved positions back into constraint values. Never infer constraints from positions.

### Avoid premature abstraction

Each phase should be implementable independently. Don't build a "constraint framework" upfront — add each constraint type directly in `constraintLogic.js` and `slvs-solver.js`. The pattern is already established and simple: one switch case per constraint type.

---

## Reference: Constraint Coverage

Constraints available in libslvs but **not yet wired** in Waffle Iron (17 remaining):

| slvs Constant | Value | Notes |
|---|---|---|
| `PT_PLANE_DISTANCE` | 100002 | 3D — not needed for 2D sketch |
| `PT_FACE_DISTANCE` | 100004 | 3D — not needed for 2D sketch |
| `PT_IN_PLANE` | 100005 | 3D — not needed for 2D sketch |
| `PT_ON_FACE` | 100007 | 3D — not needed for 2D sketch |
| `EQ_LEN_PT_LINE_D` | 100010 | Specialized — wire up if needed |
| `EQ_PT_LN_DISTANCES` | 100011 | Specialized — wire up if needed |
| `EQUAL_ANGLE` | 100012 | Useful: angle equality between two pairs of lines |
| `EQUAL_LINE_ARC_LEN` | 100013 | Useful: match line length to arc length |
| `SYMMETRIC_LINE` | 100017 | Useful: symmetry about arbitrary line (not just H/V) |
| `SAME_ORIENTATION` | 100023 | 3D — not needed for 2D sketch |
| `CUBIC_LINE_TANGENT` | 100028 | Needed when Bezier curves are added (Phase 3.3) |
| `PROJ_PT_DISTANCE` | 100030 | Useful: horizontal/vertical distance (Phase 4.1) |
| `LENGTH_DIFFERENCE` | 100033 | Specialized — wire up if needed |
| `ARC_ARC_LEN_RATIO` | 100034 | Specialized — wire up if needed |
| `ARC_LINE_LEN_RATIO` | 100035 | Specialized — wire up if needed |
| `ARC_ARC_DIFFERENCE` | 100036 | Specialized — wire up if needed |
| `ARC_LINE_DIFFERENCE` | 100037 | Specialized — wire up if needed |

Of these, **EQUAL_ANGLE**, **SYMMETRIC_LINE**, **PROJ_PT_DISTANCE**, and **CUBIC_LINE_TANGENT** are the highest value additions. The 3D constraints (100002, 100004, 100005, 100007, 100023) are irrelevant for 2D sketch workplanes. The remaining specialized length-comparison constraints are niche.
