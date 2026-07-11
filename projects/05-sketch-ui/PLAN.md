# 05 — Sketch UI: Plan

## Milestones

### M1: Sketch Mode Activation ✅
- [x] Click face → enter sketch mode (via toolbar Sketch button)
- [x] Solid becomes transparent (CadModel sketch-mode opacity)
- [x] Sketch plane displayed (SketchPlane.svelte grid overlay)
- [x] Camera aligns to sketch plane (dispatches waffle-align-to-plane event)
- [x] 2D coordinate overlay (X/Y axes + origin marker + cursor readout in status bar)
- [x] Exit sketch mode (Finish button / Escape key)

### M2: Line Tool ✅
- [x] Click-click line drawing
- [x] Point creation at endpoints
- [x] Rubberband preview
- [x] Auto-reuse coincident points
- [x] Continuous line chaining (end → start of next)

### M3: Rectangle Tool ✅
- [x] Two-click rectangle
- [x] Auto-generate 4 points, 4 lines
- [x] Auto-apply Horizontal + Vertical constraints
- [x] Preview while drawing

### M4: Circle Tool ✅
- [x] Click center + click radius
- [x] Center point + Circle entity creation
- [x] Radius preview

### M5: Arc Tool ✅
- [x] Click center + start + end
- [x] 3 points + Arc entity creation
- [x] Arc direction from click order
- [x] Preview during each step

### M6: Constraint Application UI ✅
- [x] Select entities → show applicable constraints
- [x] Right-click context menu with constraint options
- [x] Apply constraint → send to engine
- [x] Applicable constraints determined by selection composition
- [x] Constraint type indicators rendered on sketch plane

### M7: Dimension Editing ✅
- [x] Display dimension labels near constrained entities (Distance, Radius, Diameter, Angle)
- [x] Click label → inline edit value
- [x] Update constraint value on Enter/blur
- [x] Leader lines from label to entity
- [x] Uses @threlte/extras HTML component for in-viewport labels

### M8: Auto-Constraining (Snap Detection) ✅
- [x] Horizontal/Vertical snap (3° angle threshold)
- [x] Coincident snap (8px adaptive threshold)
- [x] On-entity snap (lines, circles — 5px threshold)
- [x] Visual snap indicators (green dot for coincident/on-entity, dashed line for H/V)
- [x] Auto-constraint application (H/V constraints auto-added on line creation)
- [x] Tangent snap (line-to-circle tangent point computation)
- [x] Perpendicular snap (point projection onto line foot)
- [x] Configurable threshold settings (snap settings in Property Editor)

### M9: Visual Feedback ✅
- [x] Color coding: blue (default), yellow (selected), light blue (hovered)
- [x] Entity/constraint counts in status bar
- [x] Full DOF counter (libslvs WASM solver integrated — DOF shown in status bar)
- [x] Failed constraint highlighting (over-constrained entities shown in red)
- [x] Construction geometry dashed display (done in M11)

### M10: Profile Selection ✅
- [x] Click inside closed loop → identify profile
- [x] Highlight selected profile (green) and hovered profile (light green)
- [x] Point-in-polygon test via ray casting on sketch geometry
- [x] Client-side profile extraction (half-edge minimal face algorithm)
- [x] Semi-transparent fill for hovered/selected profiles
- [x] Ready for extrusion/revolution (selectedProfileIndex in store)

### M11: Construction Geometry ✅
- [x] Toggle entity as construction (toggleConstruction in store)
- [x] Visual distinction (LineDashedMaterial, dimmer color 0x6677aa)
- [x] Exclude from profile extraction
- [x] Toolbar button ("Constr") + keyboard shortcut (X)
- [x] Works on all entity types (lines, circles, arcs, points)

### M12: Additional Drawing Tools (partially implemented)

The following tools have handler scaffolding in `tools.js` but are not yet covered by milestones:

- **Polyline tool** — multi-segment connected line drawing (handler: `handlePolylineTool`)
- **Dimension tool** — interactive dimension placement on entities (handler: `handleDimensionTool`)
- **Project tool** — ✅ COMPLETE (see M13): live-bound vertex/edge/face projection (handler: `handleProjectTool`, shared dispatch `projectRef`, logic in `projectGeometry.js`)
- **Slot tool** — two-center slot shape (handler: `handleSlotTool`, state vars: `slotFirstCenter*`, `slotSecondCenter*`)
- **Trim tool** — trim/extend entities at intersections (handler: `handleTrimTool`, state: `trimHighlight`)
- **Sketch fillet tool** — fillet corners between sketch entities (handler: `handleSketchFilletTool`, state: `filletCorner`). Note: this is a 2D sketch-level fillet, distinct from the deferred 3D fillet feature operation.

### M13: Select-First Projection ✅ (2026-07-05)

Spec: `specs/projected_sketch_geometry.md` Cycle 2. FIP cycle with role-separated
Test Author / Implementer / Adversary; real-pointer GUI oracles only.

- [x] Body vertex/edge/face hover + click-select in sketch mode under the
      **Select** tool (was gated to the `project` tool only — the "feature
      looks missing" report). Drawing tools stay fully gated.
- [x] Proj button / `J`: with body entities selected → project them all
      immediately (same live bindings as tool-first, shared `projectRef`),
      clear selection, stay in Select; empty selection → activate project tool.
      Stale selection under a drawing tool activates the tool, never projects.
- [x] Sketch entities always win hover/click priority over body geometry.
- [x] Additive (shift) face multi-select double-toggle fixed (ownership by
      kind: pointerdown path owns Vertex/Edge, CadModel click owns Face).
- [x] Undo/redo now prune/restore `projectedBindings` for removed points.
- [x] Stale-hover click race fixed (`getFreshHoveredRef` pixel-freshness guard).
- Tests: `projection-select-first.spec.js` (8),
  `projection-select-first-adversarial.spec.js` (11), helper
  `helpers/worldToScreen.js`. Viewport-side picking fixes tracked in
  `projects/04-3d-viewport/PLAN.md` M11.
- Still open (unchanged from Cycle 1): live binding of curved-edge interiors
  (static construction snapshot for now).

### M14: Snap Priority Rework + Point-Alignment Inference ✅ (2026-07-05)

Spec: `specs/snap_inference_and_priority.md` (IMPLEMENTED). FIP cycle with
role-separated Test Author / Implementer / Adversary.

- [x] Cascade reordered: point-class ≻ **on-entity** ≻ **align** ≻
      segment-H/V ≻ tangent ≻ perpendicular (on-entity no longer loses to the
      3° H/V wedge when the cursor is on an entity).
- [x] On-entity snap is parametric: emits `OnEntity{point,entity}` (template
      existed but was dropped at every emission site). Verified on lines AND
      circles with drag-tracking oracles.
- [x] Alignment inference: hovering a sketch point arms it (LRU of 3,
      hover-only arming, cleared on tool switch/sketch exit, dead-id safe);
      within a 6px screen band of an armed point's axis the cursor snaps,
      a dashed line renders from the source, and click emits
      `HorizontalPoints`/`VerticalPoints{source, new}`. Both-band emits both.
      Drawing FROM the armed point self-suppresses (segment-H/V covers it).
- [x] ALL cursor-placed points route snap constraints through ONE normalizer
      (`applyPointSnapConstraints`) — rect/center-rect finalizing corner,
      slot centers, arc center/start/end were silently dropping them.
- [x] Preview-candidate dedup filter px-derived (`CANDIDATE_DEDUP_PX=4`, was
      0.001 sketch units ≈ 10.5px over-filter at default zoom);
      `snap-preview-candidates.spec.js:143` repaired + FIXED (off the
      known-red list).
- Tests: `snap-inference.spec.js` (8), `snap-inference-adversarial.spec.js`
  (12). New hooks: `__waffle.getSketchPixelSize`, `getInferenceSources`.
- Known follow-up (pre-existing, NOT this cycle): quadrant-click red cluster —
  `snap-click-quadrant.spec.js:46/:169/:200`, `sketch-snap-click-bug.spec.js:35`,
  `sketch-snap-click-regression.spec.js:292` (DOM layer-stack class,
  "Canvas not found in elements at point"). Needs its own cycle.

### M15: Chain Select + Offset Tool + Project-with-Offset ✅ (2026-07-11)

Spec: `specs/sketch_chain_offset.md` (task #139; SI3 slice of
`docs/step_import_roadmap.md`). User driver: offset an imported board
outline by 0.5 mm to build a printed housing.

- [x] Chain select: double-click selects the connected run (shared or
      coincident endpoints, `chain.js` union-find weld @1e-6 m); shift
      unions. Gear double-click precedence preserved.
- [x] Offset tool (`O`): click chain (or seed from the selection via
      select-first `O`) → cursor picks side/magnitude → click → exact-value
      popup (dimensionPopup `customApply`) → real Line/Arc/Circle entities.
      Pure math in `offset.js`: tangent-weld joints (|d|-scaled tolerance),
      outside corners = true offset arcs (miter under 30° line-line turns),
      inside corners = carrier-intersection trim, arcs r∓d with typed
      radius-collapse error. d>0 = left-of-traversal; the cursor side picks
      the sign so users never see the convention.
- [x] Project-with-offset: with Offset active, clicking hovered body
      Edge/Face runs `projectRef` and arms the projected chain (branch 6).
- [x] `projectFace` curved in-plane edges → static construction polylines
      sharing the bound corner points (O4: one connected loop; was
      straight-edges-only, so rounded outlines projected with gaps).
- [x] Fillet-tool latent bug FIXED en route: `executeSketchFillet` removed
      the old lines before creating replacements, so the far endpoints were
      orphan-cascade-deleted and the new lines dangled on dead point ids.
      Create the new lines first. (Surfaced by chain resolve
      `missing-point`; fillet chains were never offsettable/chainable.)
- Tests: `sketch-chain-select.spec.js` (3), `sketch-offset-tool.spec.js`
  (8), `project-face-offset.spec.js` (imported-STEP-cylinder e2e). New
  hooks: `__waffle.findConnectedChain` / `computeChainOffset` /
  `getOffsetToolState` / `sketchPointToScreen`; `getMeshes().edgeRanges`.
- Known limits (documented in the spec): no global self-intersection
  removal (offset larger than local feature size); splines chain-select but
  don't offset; native kernel-v2 bodies export rim circles as degenerate
  2-point edge polylines, so face projection of curved rims is currently
  mesh-import (SI1) territory until the kernel emits rim polylines.
- Follow-up candidates: fillet tool's `findOrCreatePoint(…, 0.001)`
  hard-coded pixel size = ~8 mm merge radius (small fillets degenerate to
  start==end arcs); offset constraint linkage (parametric offset distance).

### M15b: Ring-profile extrude fix (2026-07-11, user case step_extrude.waffle)

Offset rings exposed a pre-existing failure class: extruding a sketch
containing a closed line/arc ring stored in CW walk order failed with
`kernel-v2 profile rejected: ProfileRepeatedVertex` (and, once past that,
`NewellMismatch`) — even when the extruded profile itself was clean, because
`make_faces_from_profiles` stages ALL profiles. Three root causes, all fixed:

- [x] **Twin profiles**: an isolated degree-2 ring yields TWO minimal faces
      over the same entity set (one per traversal direction); with arcs
      reduced to chords the extractor cannot tell bounded from unbounded, so
      both survived. Dedup by entity-id set keeping the CCW twin — in BOTH
      `app/src/lib/sketch/profiles.js` and
      `crates/waffle-types/src/profiles.rs` (rebuild/solve path), + red→green
      `crates/waffle-types/tests/profile_ring_twin_dedup.rs`.
- [x] **Densifier first-entity direction** (`finishSketch`,
      store.svelte.js): the first entity was always sampled forward; a
      reversed-first walk pushed the shared vertex twice → the kernel's
      exact-equality ProfileRepeatedVertex. Direction now derived from
      connectivity with the second entity (bigons keep forward).
- [x] **Densifier reversed-arc sampling**: arcs were always sampled CCW from
      the traversal start, so reversed arcs sampled the COMPLEMENT arc.
      Reversed traversal now samples with decreasing angles.
- [x] Offset tool now emits closed rings with deterministic CCW winding
      (offset.js normalization) so new geometry never enters the CW-ring
      regime in the first place.
- Repair for existing broken saves: open the sketch (edit) and Finish — the
  profiles are re-authored by the fixed densifier. Persisted profiles are
  otherwise preserved on load (`recompute_derived` fills only when empty).
- Tests: `sketch-ring-profile-extrude.spec.js` (2, verified red pre-fix),
  Rust `profile_ring_twin_dedup.rs` (2, red pre-fix); user file verified
  load→edit→finish→extrude green end-to-end.

## Implementation Summary

### New files created
| File | Purpose |
|------|---------|
| `app/src/lib/sketch/sketchCoords.js` | Screen→3D→2D coordinate projection |
| `app/src/lib/sketch/tools.js` | Tool state machines (line, rect, circle, arc, select) + profile hit-test |
| `app/src/lib/sketch/snap.js` | Auto-constraining: coincident, H/V, on-entity snap |
| `app/src/lib/sketch/profiles.js` | Client-side closed-loop extraction (half-edge algorithm) |
| `app/src/lib/sketch/SketchRenderer.svelte` | Renders sketch entities + preview + snap + profiles |
| `app/src/lib/sketch/SketchInteraction.svelte` | Invisible plane capturing pointer events |
| `app/src/lib/sketch/ConstraintMenu.svelte` | Right-click popup for manual constraints |
| `app/src/lib/sketch/DimensionLabels.svelte` | Editable dimension labels via HTML overlay |
| `app/src/lib/sketch/constraintLogic.js` | Constraint applicability logic for selection compositions |
| `app/src/lib/sketch/DimensionInput.svelte` | Inline dimension value editing input |
| `app/src/lib/sketch/geometry-utils.js` | Geometric helper functions (intersection, projection, distance) |
| `app/src/lib/sketch/InactiveSketchRenderer.svelte` | Renders sketch entities when not in active sketch mode |
| `app/src/lib/sketch/projectGeometry.js` | Project 3D edges onto sketch plane + polyline simplification |
| `app/src/lib/sketch/sketchToolState.svelte.js` | Svelte 5 reactive tool state management |

### Modified files
| File | Changes |
|------|---------|
| `app/src/lib/engine/store.svelte.js` | Sketch entity/constraint state, ID allocator, hit-test helpers, construction toggle, profile state |
| `app/src/lib/viewport/Scene.svelte` | Import SketchRenderer + SketchInteraction + DimensionLabels |
| `app/src/lib/viewport/Viewport.svelte` | Import ConstraintMenu HTML overlay |
| `app/src/lib/ui/StatusBar.svelte` | Show entity/constraint counts in sketch mode |

## Blockers

(None — libslvs WASM solver now integrated via Emscripten build)

## Interface Change Requests

(None)

## Notes

- Auto-constraining is critical for UX. Without it, sketching is painful.
- The Dragged constraint workflow enables interactive geometry manipulation.
- Dimension editing should feel like Onshape: click label, type value, Enter.
- All sketch state resets when entering/exiting sketch mode.
- Tool state machines support continuous chaining (line tool) and multi-click flows (arc tool).
