# Spec: Center-point rectangle tool + Rectangle split button

> STATUS: IN PROGRESS. Adds a second rectangle construction mode
> (center → corner) alongside the existing corner → corner mode, selectable via
> a split-button dropdown on the Rect toolbar button. Depends on
> `point_pair_horizontal_vertical.md` (the center-alignment constraints).

## Goal

Let the user draw an axis-aligned rectangle by placing its **center** first and
then a **corner**, in addition to the existing corner-to-corner mode. The
rectangle carries a real center point that stays at the geometric center under
later edits (dimensioning an edge keeps the center centered).

The Rect toolbar button becomes a **split button**: the main face selects the
currently-chosen rectangle mode; a small dropdown arrow in the bottom-right of
the button opens a menu to switch mode (Corner Rectangle / Center Rectangle).

## Parameters

| Input | Source | Notes |
|---|---|---|
| center `(cx, cy)` | first click/press (center mode) | snapped |
| corner `(mx, my)` | second click / drag release | snapped; defines half-extents `hx=|mx-cx|`, `hy=|my-cy|` |
| `rectMode` | toolbar dropdown | `'rectangle'` (corner) \| `'rectangle-center'`; persisted in store, default `'rectangle'` |

Degenerate: `hx==0 || hy==0` (zero-area) → no rectangle created, tool resets
(same guard philosophy as a zero-size corner rectangle).

## Entities & constraints produced (center mode)

For center `C` and corner extents `hx,hy` about `(cx,cy)`, corners:
`p1=(cx-hx,cy-hy) p2=(cx+hx,cy-hy) p3=(cx+hx,cy+hy) p4=(cx-hx,cy+hy)`.

| Entity | Kind | Construction? |
|---|---|---|
| p1..p4 | Point (corners) | no |
| l1..l4 (p1→p2→p3→p4→p1) | Line (edges) | no |
| C (center) | Point | no (real center point) |
| M_top (midpoint of top edge l3: p3→p4) | Point | yes |
| M_left (midpoint of left edge l4: p4→p1) | Point | yes |

| Constraint | Meaning |
|---|---|
| Horizontal(l1), Horizontal(l3) | top & bottom edges horizontal |
| Vertical(l2), Vertical(l4) | left & right edges vertical |
| Midpoint(M_top, l3) | M_top is the midpoint of the top edge |
| Midpoint(M_left, l4) | M_left is the midpoint of the left edge |
| VerticalPoints(C, M_top) | C shares X with the top-edge midpoint ⇒ `cx = (xl+xr)/2` |
| HorizontalPoints(C, M_left) | C shares Y with the left-edge midpoint ⇒ `cy = (yb+yt)/2` |

The last four constraints are exactly the user-specified scheme: "top line
midpoint vertical to the centerpoint, left line midpoint horizontal to the
centerpoint." Together they pin `C` to the centroid with no redundant DOF.

## Branch Table

| Mode (`rectMode` / activeTool) | First input | Second input | Center point | Centering constraints |
|---|---|---|---|---|
| `rectangle` (corner) | first corner | opposite corner | none | none (unchanged behavior) |
| `rectangle-center` | center | corner | yes (real) | Midpoint×2 + VerticalPoints + HorizontalPoints |

Both modes share edge creation + H/V edge constraints (`createRectangleEdges`).
Both support click-click AND click-drag (per GUI test rules).

## Invariants

- **I1 (corner mode unchanged):** corner mode still yields exactly 4 points +
  4 lines + 4 H/V constraints; no center point, no extra constraints.
- **I2 (center geometry):** in center mode the 4 corners are symmetric about the
  first click: `(p1+p3)/2 == (p2+p4)/2 == (cx,cy)` at creation time.
- **I3 (center stays centered):** after the sketch solves, and after any later
  edge dimension change, `C ≈ ((min_x+max_x)/2, (min_y+max_y)/2)` of the 4
  corners (within solver tol).
- **I4 (construction flags):** C is non-construction; M_top, M_left are
  construction (they are scaffolding, not profile geometry).
- **I5 (profile integrity):** the closed profile for extrude is the 4 edges
  only — construction midpoints and the center point do not perturb the profile
  (existing construction-exclusion path).
- **I6 (mode persistence):** selecting a mode from the dropdown sets it active
  and persists `rectMode`; pressing `R` re-activates the persisted mode.

## Oracles

GUI (Playwright, `app/tests/gui/`):
- corner click-click & click-drag: entity counts (4 Point, 4 Line), 4 constraints — regression that corner mode is untouched (I1).
- center click-click & click-drag: counts = 7 Points (4 corner + center + 2 mids) + 4 Lines; constraint set includes 2 Midpoint, 1 VerticalPoints, 1 HorizontalPoints, 4 H/V edges (I2/I4). Read via `__waffle.getEntities()` / `getConstraints()`.
- center centroid oracle: after a real pointer draw, the center point position
  (from `__waffle.getPositions()`) ≈ mean of the 4 corner positions; and it stays
  the centroid after a programmatically-added edge `Distance` re-solves (I3).
- split button: dropdown opens, selecting "Center Rectangle" sets
  `activeTool==='rectangle-center'` and persists; `R` reactivates it (I6).

Per the GUI test rules, drawing is exercised with real pointer events
(`drawCenterRectangle`/`dragCenterRectangle` helpers); structure is read back via
`__waffle.getEntities()` / `getConstraints()` / `getPositions()`.

## Failure Modes

- Zero half-extent → no creation, tool resets to idle (no zero-length lines, I5-safe).
- Snap reuse of an existing point for a corner behaves as in corner mode
  (`findOrCreatePoint`).

## Research Basis

Center-rectangle construction is a standard parametric-CAD primitive
(SolidWorks/Onshape "Center Rectangle"): a center point tied to the rectangle by
midpoint + axis-alignment relations. Constraint solving is the existing
LM solver. The center-alignment relations use the point-pair Horizontal/Vertical
constraints specified in `point_pair_horizontal_vertical.md`.

### Analytical vs. Approximate Method Justification

- **Method:** Exact — corner coordinates and constraints are closed-form; no
  surface-surface intersection. A15 SSI rules do not apply (2D sketch geometry).
