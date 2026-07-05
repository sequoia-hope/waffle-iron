# Spec: Snap priority rework + point-alignment inference

> STATUS: IMPLEMENTED (2026-07-05). Full FIP cycle: red tests
> (`snap-inference.spec.js`, 8 oracles red on baseline + the adjudicated
> `snap-preview-candidates.spec.js:143` repair) → implementation →
> adversarial validation (`snap-inference-adversarial.spec.js`, 12 guards).
> Final: 8/8 + 10/10 + 12/12, drawing canary green, gui-fast 342/1 (only the
> known mm/m red — the :143 red is FIXED by the px-derived dedup filter).
>
> VALIDATION FINDINGS (fixed in-cycle):
> - F-AS6c (Adversary): rectangle finalizing corner showed the Align preview
>   but dropped the constraint — `createRectangleEdges` bypassed the shared
>   normalizer. The audit found the full class: slot centers and all arc
>   points also dropped click-time snap constraints. Every cursor-placed
>   point now routes through `applyPointSnapConstraints`; derived corners
>   explicitly excluded (only the corner under the cursor collects the snap).
> - O9 (Implementer stop, adjudicated test bug): the determinism oracle's
>   run 1 placed a point inside run 2's coincident radius — priority-1
>   correctly preempted align per this spec's failure-mode table. Repaired
>   with fresh-sketch reset + identical pointer replay.
> - O5 arming leak (Implementer, in-cycle): placing a point ran detectSnaps
>   on pointerup and self-armed the just-created point — arming is now
>   hover-only (`armOnHover` flag on pointermove).
> - Design note: align over-constraint (failure mode #3) is unreachable by
>   construction — align only constrains a fresh free point, and coincident
>   preempts it near existing points. Defended structurally, not by solver
>   reporting (guard AS4).
> - Pre-existing quadrant-click red cluster (5 tests, see O7 exclusions)
>   signature-verified unchanged; still needs its own cycle. User report: (a) while drawing, hovering over an
> entity (line or point) too easily pops the horizontal/vertical snap when the
> intent is on-entity; (b) wants: after recently hovering a sketch point, when
> the cursor is near the horizontal or vertical axis through that point, show
> an H/V alignment inference — dotted line for visualization — and applying it
> on click as a constraint. Modeling-affecting UI change (constraint emission
> changes) → full FIP. Rust constraint vocabulary is already sufficient
> (`HorizontalPoints`/`VerticalPoints`, `OnEntity` — waffle-types
> `src/sketch.rs:292-301,377-380`); no Rust or WASM changes expected.

## Goal

1. **Priority fix**: on-entity snap outranks segment-direction H/V. H/V still
   applies for free-space direction alignment.
2. **On-entity becomes parametric**: placing a point via on-entity snap emits
   the `OnEntity { point, entity }` constraint (today the template is built in
   `snap.js:207,231` but dropped at every emission site — position-only).
3. **Alignment inference**: hovering a sketch point (any priority-1 coincident
   snap on a real point entity) "arms" it as an inference source. Afterwards,
   while a drawing/point tool is active and the cursor is within a narrow band
   of that point's horizontal or vertical axis, the cursor snaps onto the
   axis, a dashed inference line is drawn from the source point to the cursor,
   and an "Align H"/"Align V" indicator shows. Clicking while shown places the
   point and emits `HorizontalPoints`/`VerticalPoints { point_a: source,
   point_b: new }`.
4. **Candidate-dedup + spec:143 repair**: the preview-candidate dedup filter
   (`tools.js:416-419`, hardcoded `0.001` sketch units) becomes pixel-derived,
   and the defective meter-unit tolerance in
   `snap-preview-candidates.spec.js:143-179` (`0.5` sketch units ≈ 300× the
   whole drawing) is repaired to a pixel-derived tolerance. Adjudication: the
   test encoded unit-blind tolerances, not intended behavior; its intent
   (active snap's own marker is not duplicated in the preview candidates)
   is preserved.

## Parameters (all in `app/src/lib/config.js`, screen-px calibrated)

| Constant | Value | Meaning |
|---|---|---|
| `COINCIDENT_SNAP_PX` | 8 (existing) | point-class snap radius; also the arming radius |
| `ON_ENTITY_SNAP_PX` | 5 (existing) | on-entity radius |
| `HV_ANGLE_DEG` | 3 (existing) | segment-direction H/V wedge |
| `INFERENCE_ALIGN_PX` | 6 (new) | half-band around the source point's axis |
| `INFERENCE_SOURCES_MAX` | 3 (new) | LRU size of armed inference sources |
| `CANDIDATE_DEDUP_PX` | 4 (new) | preview-candidate dedup radius (replaces 0.001 sketch units) |

## New snap cascade order (`detectSnaps`)

| Priority | Type | Change |
|---|---|---|
| 1 | coincident / origin / reference / midpoint / quadrant | unchanged; coincident on a real point entity ALSO arms it as an inference source |
| 2 | **on-entity** | moved up (was 3, below H/V) |
| 3 | **align-h / align-v** (new) | vs armed sources; band `INFERENCE_ALIGN_PX` |
| 4 | horizontal / vertical (segment direction) | was 2 |
| 5 | tangent | relative order to H/V unchanged |
| 6 | perpendicular | unchanged |

## Branch table

| Situation | Winner | Position effect | Constraint emitted on click |
|---|---|---|---|
| Cursor within 8px of a point | coincident | reuse point id | none (shared id); `Coincident` in point tool (existing) |
| Cursor on entity (≤5px), also within 3° H/V wedge | **on-entity** | projected onto entity | **`OnEntity { point, entity }`** (new emission) |
| Cursor in armed source's H band (≤6px of its y), not on point/entity | align-h | y := source.y | `HorizontalPoints { point_a: src, point_b: new }` |
| Cursor in armed source's V band (≤6px of its x), same | align-v | x := source.x | `VerticalPoints { point_a: src, point_b: new }` |
| Cursor in BOTH bands of one or two sources | align-h + align-v combined | x and y both snapped | both point-pair constraints |
| Align band AND segment-H/V wedge both hit | align (more specific: armed by deliberate hover) | axis coords | point-pair constraint(s) only |
| Free space, within 3° wedge from segment start | horizontal/vertical (existing) | direction snapped | `Horizontal`/`Vertical { entity: line }` (existing) |
| Source point = the segment's own start point | align suppressed for that source (segment-H/V already covers it) | — | — |
| No drawing tool active / sketch exited / tool switched | inference sources cleared | — | — |

Arming rules: only real point entities (snap carries `snapPointId`) arm; LRU of
`INFERENCE_SOURCES_MAX`, most-recent first, dedup by point id; re-hover
refreshes recency. Origin/midpoint/quadrant markers do NOT arm (no point
entity to constrain against).

## Invariants

- **I1 — On-entity wins over segment-H/V**: cursor within `ON_ENTITY_SNAP_PX`
  of an entity always yields on-entity, regardless of direction wedge.
- **I2 — Parametric on-entity**: a point placed via on-entity snap carries an
  `OnEntity` constraint; solving after moving the host entity keeps the point
  on it (numeric oracle: perpendicular distance ≤ solver tol).
- **I3 — Alignment is point-pair-constrained**: a point placed via align-h has
  solved `y == source.y` (align-v: `x == source.x`) within solver tolerance,
  and the constraint survives re-solve after dragging the source.
- **I4 — Deliberate arming**: alignment inference never appears for a point
  that was not hovered (no spooky snapping to arbitrary sketch points).
- **I5 — Visual contract**: while align-h/v is the active snap, exactly one
  dashed inference line per armed axis renders from the source point to the
  cursor, and it disappears when the snap deactivates.
- **I6 — Determinism**: no wall-clock time in arming/expiry (LRU by hover
  order only); same pointer sequence ⇒ same snaps.
- **I7 — Scale independence**: all new thresholds are screen-px derived via
  `screenPixelSize` (same discipline as viewport picking.js).
- **I8 — Existing snap behaviors preserved**: priority-1 snaps, tangent,
  perpendicular, and free-space segment-H/V behave exactly as before
  (existing snap spec suite stays green except the adjudicated :143 repair).

## Oracles (GUI, real pointer events; plus store-level assertions)

- **O1 (I1)**: draw a line whose far end passes over an existing line within
  the 3° wedge → indicator is `on-entity`, not `horizontal`. Control: same
  approach in free space → `horizontal`.
- **O2 (I2)**: place via on-entity on a line; assert `OnEntity` constraint in
  `getConstraints()`; drag the host line; assert point still on it (distance
  oracle).
- **O3 (I3/I5)**: hover point P (coincident indicator shows), move away, move
  to same-y-within-band at distance → indicator `align-h`, dashed line
  geometry present from P; click → new point, `HorizontalPoints{P, new}` in
  constraints; solved y equal. Mirror for `align-v`.
- **O4 (both-bands)**: arm two points A (same y) and B (same x); move to the
  intersection → both constraints on click; solved x,y equal respectively.
- **O5 (I4 control)**: WITHOUT hovering, pass through where P's band would be
  → NO align indicator (plain segment-H/V or none).
- **O6 (LRU)**: hover 4 points; the 1st is evicted (no align to it), the
  last 3 align.
- **O7 (I8 regression)**: full existing snap suite
  (snap-detect-new-types, snap-labels, snap-hover-indicator,
  origin-snap-constraint, sketch-snap-click-*, snap-click-*,
  sketch-point-and-drag-snap, snap-preview-*) green — EXCEPT the following
  5 pre-existing reds, stash-verified identical on committed HEAD before this
  cycle (2026-07-05, Test Author): snap-click-quadrant.spec.js (3 tests),
  sketch-snap-click-bug.spec.js:35, sketch-snap-click-regression.spec.js:292
  (quadrant-snap-click / DOM layer-stack class; NOT this cycle's scope).
  This cycle must not change their failure signatures.
- **O8 (:143 repair)**: repaired test asserts, in px-derived units, that the
  active snap's own marker is absent from preview candidates while distinct
  nearby candidates (origin, midpoint) are retained.
- **O9 (I6)**: repeat O3's pointer sequence twice in one session → identical
  indicator/constraint results.

## Failure modes

- Armed source deleted (undo, entity delete) → its inference slot is dropped;
  no dangling-id constraint can be emitted.
- Align click coincides with a priority-1 snap (e.g. band crosses another
  point) → priority-1 wins (cascade); no point-pair constraint.
- Over-constrained result (e.g. align-h onto a point already fixed elsewhere)
  → existing solver conflict reporting handles it; snap layer does not
  pre-filter.
- fromPointId == source point → align suppressed (segment-H/V covers it);
  prevents duplicate H + HorizontalPoints on the same pair.

## Research basis

Inference-line alignment mirrors mainstream parametric sketchers (Onshape
inferencing, SolidWorks sketch relations preview); no novel geometry — the
alignment test is the same axis-delta math the existing H/V block uses
(`snap.js:164-188`) applied to remembered points. Constraint semantics reuse
solver-tested `HorizontalPoints`/`VerticalPoints` (equate-Y/equate-X,
`sketch-solver/src/constraint_mapping.rs:311-323`) and `OnEntity`.

## Implementation notes (for the cycle, not prescriptive)

- Inference-source store + getters live in store.svelte.js; expose read-only
  `__waffle.getInferenceSources()` for tests.
- Emission: normalize early — every placement path that consumes
  `snap.constraints` handles the new `OnEntity` / `HorizontalPoints` /
  `VerticalPoints` templates through ONE shared helper (extend
  `applyPointSnapConstraints`), no per-tool re-branching (Constitution §7).
- Rendering: reuse `snapDashedMaterial` (SketchRenderer.svelte:613) with a new
  `align` snapGeo variant anchored at the SOURCE point (segment-H/V's dashed
  line anchors at fromPos — different anchor, same vocabulary).
- Labels: "Align H" / "Align V" via `snapLabelMap`.
