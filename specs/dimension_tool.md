# Spec: Dimension tool — pick-then-place with orientation heuristic

## Goal

Rework the dimension tool into a pick-then-place flow that mirrors the
constraint-modal UX (see /specs/constraint_modal.md) and supports dimensioning
between two objects:

1. The user activates the Dimension tool and clicks the object(s) to dimension
   (a single line, or a pair: point–point, point–line, line–line).
2. Once a measurable set is picked, a **dimension leader preview follows the
   mouse**. For point/line measurements the leader's position chooses the
   dimension *orientation* (horizontal / vertical / aligned) via a heuristic.
3. Clicking in free space **places** the leader; the value popup opens at that
   spot pre-filled with the measured value. Confirming creates the constraint.

Circles/arcs keep their existing immediate radius popup (orientation heuristic
for curved geometry is out of scope — "we'll deal with circles later").

## Parameters (inputs)

- `targets` — the picked sketch entities, in pick order (points / lines).
- `leader` — current sketch-space cursor position while placing.
- `positions` — solved sketch positions (for measuring).
- `entities` — sketch entities (to resolve line endpoints).

## Branch Table

| targets            | dimKind        | orientation (by leader)        | constraint emitted |
|--------------------|----------------|--------------------------------|--------------------|
| 1 line             | linear         | horizontal / vertical / aligned | HDistance / VDistance / Distance(start,end) |
| point + point      | linear         | horizontal / vertical / aligned | HDistance / VDistance / Distance(a,b) |
| point + line       | perp           | n/a (perpendicular distance)   | PointLineDistance(point,line) |
| line + line ∥      | lineDistance   | n/a                            | PointLineDistance(line2.start, line1) |
| line + line ∦      | angle          | n/a                            | Angle(line1, line2) |
| 1 circle / 1 arc   | radius         | n/a (immediate popup)          | Diameter (= 2·radius) |
| 1 point only       | (incomplete)   | —                              | none — waits for 2nd pick |

### Orientation heuristic (point/line linear case)

Given anchors A, B and leader L, with midpoint M = (A+B)/2 and offset
`o = L − M`, let `deg = atan2(|o.y|, |o.x|)` in degrees (0 = purely horizontal
offset, 90 = purely vertical):

- `deg ≤ 30` (leader pushed to the **side**) → **vertical** dimension, value `|B.y − A.y|`.
- `deg ≥ 60` (leader pushed **above/below**) → **horizontal** dimension, value `|B.x − A.x|`.
- otherwise (diagonal) → **aligned** dimension, value `hypot(B−A)`.

Degenerate `|o| ≈ 0` (leader at the midpoint) → aligned.

This is intentionally simple and covers points + lines only; it is the
documented "basic heuristic" — curved geometry and richer snapping are deferred.

### Completeness

A pick set is *complete* (ready to place) when it is: a single line, or a pair
of {point+point, point+line, line+line}. A lone point is incomplete and waits.
A circle/arc dimensions immediately (no placement step).

## Invariants

- **Measured value matches geometry**: the value handed to the popup equals the
  heuristic's measurement for the chosen orientation (HDistance ⇒ |Δx|, VDistance
  ⇒ |Δy|, aligned ⇒ straight-line distance, perp ⇒ point-line perpendicular
  distance, angle ⇒ interior angle in degrees).
- **Orientation is leader-driven and continuous**: moving the leader from a
  side position to an above/below position flips vertical→horizontal at the
  30°/60° boundaries; nothing else changes the orientation.
- **Constraint correctness**: the emitted constraint type matches the branch
  table; HDistance/VDistance carry the two point ids, aligned a Distance, etc.
- **No constraint until placement**: picking objects never creates a constraint;
  only confirming the value popup does.
- **Determinism**: identical (targets, leader, positions) ⇒ identical classify.

## Oracles

- **Unit (pure, in-browser via `__waffle.classifyDimension`)** over live drawn
  geometry: assert `dimKind`, `orientation`, measured `value`, and emitted
  `constraint.type`/ids for: a side-leader vertical, an above-leader horizontal,
  a diagonal aligned (two points); a single-line length; a point+line perp; a
  parallel line+line distance; a crossing line+line angle. Mutation check: the
  same two points with a side vs. above leader must yield different orientations.
- **GUI end-to-end**: draw two points offset in both x and y; activate the
  dimension tool; click both points; move the leader to the side and click free
  space; in the popup confirm → assert a `VDistance` (or `HDistance` for an
  above/below leader) constraint exists with the expected value, and the points
  satisfy it after solve.

## Failure Modes

- **Incomplete pick** (lone point, or pair containing a circle/arc): no leader,
  no constraint; tool keeps waiting or ignores the curve.
- **Empty-space click before complete**: no-op.
- **Degenerate leader** (at midpoint): falls back to aligned.
- **Switching tools / Escape**: clears the in-progress dimension and any preview.

## Research Basis

No external geometry reference — interaction logic plus a leader-position
heuristic over the existing `sketch-solver` constraints (HDistance, VDistance,
Distance, PointLineDistance, Angle, Diameter). The leader-drives-orientation
behavior mirrors mainstream "smart dimension" tools (Onshape/SolidWorks). Per
the user decision of record, only a basic point/line heuristic is implemented;
curves are deferred.
