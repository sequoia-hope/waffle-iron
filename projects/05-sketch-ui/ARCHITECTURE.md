# 05 — Sketch UI: Architecture

## Purpose

2D sketch editing interface overlaid on the 3D viewport during sketch mode. This is where users draw geometry (lines, rectangles, circles, arcs) and apply constraints to define parametric 2D profiles for extrusion and revolution.

## Sketch Mode Lifecycle

### Entering Sketch Mode

1. User clicks a face in the 3D viewport (or selects a datum plane).
2. Engine identifies the face's plane (origin, normal, X-axis).
3. UI transitions to 2D mode:
   - Solid mesh becomes transparent (ghosted).
   - Sketch plane displayed as opaque grid.
   - Camera aligns to face the sketch plane (smooth transition).
   - Drawing tools become available in toolbar.
   - 2D coordinate system overlaid on the plane.

### During Sketch Mode

- User draws entities with drawing tools.
- Constraints are applied (automatic + manual).
- Solver runs after each change, positions updated in real-time.
- Visual feedback shows constraint status.
- User selects a closed profile for extrusion.

### Exiting Sketch Mode

1. User clicks "Finish Sketch" or presses Escape.
2. Sketch entities and constraints are committed as a Sketch feature.
3. Solid returns to full opacity.
4. Camera returns to 3D view.

## Drawing Tools

### Line Tool
- Click first point → click second point → line created.
- Two Point entities + one Line entity created.
- Points at endpoints are reused if coincident with existing points (auto-snap).
- Rubberband preview while drawing.

### Rectangle Tool
- Click first corner → click opposite corner → rectangle created.
- Creates: 4 Points, 4 Lines, 4 Coincident constraints (corners), 2 Horizontal constraints, 2 Vertical constraints.
- Auto-constraining: rectangle always produces horizontal/vertical lines.

### Circle Tool
- Click center → drag/click to set radius → circle created.
- Creates: 1 Center Point, 1 Circle entity.
- Radius preview while dragging.

### Arc Tool
- Click center → click start point → click end point → arc created.
- Creates: 3 Points (center, start, end), 1 Arc entity.
- Arc direction determined by click order.

### Construction Geometry Toggle
- Any entity can be toggled to "construction" mode.
- Construction entities participate in constraint solving but are NOT included in profiles for extrusion.
- Visual distinction: dashed lines for construction, solid for real geometry.

## Constraint Application

### Manual Constraints
- Select one or two entities → right-click or toolbar shows applicable constraints.
- Applicable constraints determined by selection type:
  - Two points: Coincident, Horizontal, Vertical, Distance, Symmetric
  - Two lines: Parallel, Perpendicular, Equal, Angle
  - Point + line: OnEntity, Distance, Midpoint
  - Line + arc: Tangent
  - Single line: Horizontal, Vertical
  - Single circle/arc: Radius, Diameter

### Auto-Constraining (Snap Detection)
As the user draws, automatically detect and apply constraints when close to trigger thresholds:

- **Horizontal/Vertical snap:** If a line's angle is within threshold (~3°) of horizontal or vertical, snap and add constraint.
- **Coincident snap:** If a new point is within threshold (~5px screen distance) of an existing point, snap to it and add Coincident constraint.
- **Tangent snap:** If a line endpoint is near a circle/arc and the line direction is approximately tangent, snap and add Tangent constraint.
- **On-entity snap:** If a point is near a line or circle, snap to it and add OnEntity constraint.

Auto-constraining is critical for UX — Onshape does this aggressively and users expect it.

### Dragged Constraint
When the user drags a point in an under-constrained sketch:
1. Set the point's position to the cursor location.
2. Add a `Dragged` constraint on the point.
3. Solve.
4. Read the solved position (solver moves point as close to cursor as constraints allow).
5. Remove the `Dragged` constraint.

This enables interactive "rubber banding" of unconstrained geometry.

## Visual Feedback

### Color Coding
| State | Color |
|-------|-------|
| Fully constrained | Green or Black |
| Under-constrained | Blue |
| Over-constrained (conflicting) | Red |
| Construction geometry | Dashed, lighter color |
| Selected entity | Highlight yellow |
| Hovered entity | Lighter highlight |

### Dimension Display
- Dimensional constraints (Distance, Angle, Radius) displayed as labels near the constrained entities.
- Labels are editable: click label → type new value → re-solve.
- Leader lines connect label to constrained entities.

### DOF Counter
- Displayed in status bar: "DOF: 3" (3 degrees of freedom remaining).
- "Fully Constrained" when DOF = 0.
- "Over-Constrained" with conflict indicator when constraints conflict.

### Failed Constraint Highlighting
- When solver returns OverConstrained, highlight the conflicting constraints in red.
- Conflict IDs come from solver's failed_constraints list.

## Profile Selection

After sketch is fully constrained (or user explicitly selects):
1. User clicks inside a closed loop.
2. System identifies which closed profile contains the click point (point-in-polygon test on solved geometry).
3. Profile highlights.
4. Profile is ready for extrusion/revolution (user picks the operation from toolbar).
