# Spec: Projected sketch geometry with live source coincidence

> STATUS: IMPLEMENTED — vertex, edge, and face projection end-to-end. Increments
> 1–7 (commits 3219bf5e, fbd8ead6, 89c9a373, e0c9b0ab, f144feab, 9fdc6f9a,
> a512c5f5): SketchPlaneBasis transform, resolve_by_position, ProjectedEntity
> side-table, rebuild reprojection, FinishSketch bridge + WASM, the projectVertex
> UI tool, and the dangling-source adversarial case. Edge/face projection landed
> in dff20159: a straight edge → two bound endpoint vertices + a line; a face →
> its in-plane straight boundary edges as bound construction lines (shared
> corners deduped). DESIGN NOTE: edges/faces bind by ENDPOINT VERTICES (Position
> refs), not EdgeSample — this reuses the vertex reproject path and needs no
> engine change; EdgeSample remains in the data model for future curved-edge
> interior binding. STILL OPEN: live binding of **curved** edge interiors (kept
> static for now), pick-time cyclic-ref rejection (engine degrades to dangling),
> and a full in-browser parametric-update oracle (covered by the Rust
> integration test reproject_tracks_moved_source_vertex).

## Goal

While editing a sketch, let the user select existing model geometry (vertices,
edges, faces of bodies/datums) and **project** it into the active sketch as new
sketch entities that stay **coincident with their source** — so when an upstream
feature changes (moving the source geometry), the projected sketch geometry
updates on rebuild. Additionally, ordinary (non-projected) sketch geometry can
be constrained coincident to projected geometry (this falls out for free once
projected entities are normal sketch points/lines, via the constraint modal).

Today's `project` tool (app `handleProjectTool`) only bakes a **static**
construction-line snapshot of a hovered edge (no source link, no live update);
face/vertex projection is unimplemented. This feature replaces that with a
live, parametric projection.

## Concepts & Data Model

### New `SketchEntity` provenance (waffle-types `src/sketch.rs`)

Projected entities must remember their source so rebuild can re-derive their 2D
position. Two viable encodings — **decision: option A** (least churn to the
solver and serialization):

- **Option A (chosen): optional `source` on existing point variant.**
  Add `source: Option<ProjectedSource>` (serde default `None`) to `Point` (and
  the line's identity is implied by its projected endpoints). A projected edge
  becomes N projected points + connecting lines, exactly like the static path
  but with each generated point carrying its `source`.
- Option B: dedicated `ProjectedPoint { id, source, .. }` variant. Cleaner
  typing but touches every `match` on `SketchEntity` across the codebase.

```rust
/// Where a projected sketch point came from, so rebuild can re-derive it.
pub struct ProjectedSource {
    pub geom_ref: GeomRef,          // the source vertex/edge/face
    pub kind: ProjectedKind,        // Vertex | EdgeSample{ t } | FaceVertex{ i }
}
```

A projected point is **driven** (not freely draggable): the solver treats it
like a `WhereDragged`/fixed anchor at its reprojected position, so user
constraints reference it but cannot move it. (Implementation: emit an internal
fixed constraint for projected points, or mark them fixed in `ParamLayout`.)

### Position-selector resolution (feature-engine `src/resolve.rs`)

The viewport already builds `Selector::Position { x, y, z }` GeomRefs for picked
vertices/edges (see `VertexOverlay.makeVertexRef`). `resolve_geom_ref` currently
**errors** on `Position` (resolve.rs ~lines 53–59). Implement
`resolve_by_position`: query the kernel (`KernelIntrospect`) for the
vertex/edge/face nearest the quantized 3D point within `TAU_MODEL`, returning the
`KernelId`. This is what makes a projected source survive rebuilds (the picked
position re-resolves to the moved geometry through persistent naming).

### Rebuild-time reprojection (feature-engine `src/rebuild.rs`)

After a sketch feature's upstream features execute, for each projected entity in
the sketch:
1. Resolve `source.geom_ref` → `KernelId` (via the Position/Role resolver).
2. Read its 3D coordinate(s) from the kernel.
3. Transform world→sketch-plane 2D using the sketch's `plane_origin` /
   `plane_normal` (+ the same basis `buildSketchPlane` uses in JS — share the
   math or mirror it exactly; this is the MEDIUM-HIGH risk step, test it hard).
4. Overwrite the projected point's `(x, y)` before solving.

Reprojection runs only when a sketch actually has projected entities (guard on a
per-sketch flag) to bound rebuild cost.

### Bridge + UI

- `wasm-bridge`: new `UiToEngine::ProjectGeometry { source: GeomRef }` (or extend
  `AddSketchEntity` with optional `source`). Dispatch adds the projected
  point(s)/line(s) to the active sketch.
- App: the `project` tool (and a viewport pick path) sends the picked ref. The
  static `handleProjectTool` edge path is replaced by the live path; vertices →
  projected point; edge → projected polyline points; face → its boundary edges.
- Rebuild the WASM bundle (per CLAUDE.md WASM workflow) in the same increment as
  the Rust changes.

## Branch Table

| Source pick | Sketch result | Live link |
|-------------|---------------|-----------|
| Vertex      | 1 projected Point | reprojects from source vertex |
| Edge (line) | 2 projected endpoints + 1 line | endpoints reproject from edge ends |
| Edge (curve)| sampled projected points + polyline (construction) | each sample reprojects via `EdgeSample{t}` |
| Face        | projected boundary loop (its edges) | each boundary vertex reprojects |
| (source coplanar with sketch plane) | true projection | identity in-plane |
| (source resolve fails on rebuild)   | keep last position; mark dangling (BestEffort policy) | — |

## Invariants

- **Coincidence**: a projected point's solved 2D position equals the world→plane
  projection of its resolved source within `TAU_MODEL`.
- **Parametric update**: changing an upstream feature so the source moves by Δ
  moves the projected point's world position to match on rebuild (oracle below).
- **Driven, not free**: a projected point cannot be moved by dragging or by a
  conflicting user constraint — the solver keeps it at the reprojected spot
  (over-constraint with a user dimension on a projected point is reported, not
  silently satisfied by moving it).
- **Non-projected coincidence**: a normal sketch point made `Coincident` with a
  projected point follows it across rebuilds.
- **Determinism**: reprojection is a pure function of (source position, plane).

## Oracles

- **feature-engine unit (`resolve.rs`)**: build a body, pick a vertex by
  `Position`, assert `resolve_by_position` returns the correct `KernelId`;
  perturb an upstream parameter and assert it re-resolves to the moved vertex.
- **feature-engine integration (`test-harness`)**: box → sketch on a face →
  project the box's top-edge endpoints → edit the box height → rebuild → assert
  the projected points' world Z tracks the new height (numeric Δ oracle).
- **Transform unit**: world→plane→world round-trips a known point within
  `TAU_MODEL`; an in-plane source projects to itself.
- **GUI**: project a vertex into a sketch; assert a projected point appears at
  the expected 2D location; constrain a drawn point coincident to it; edit the
  upstream feature; assert both move together after rebuild.

## Failure Modes

- **Position unresolvable on rebuild** (source deleted/merged): `BestEffort`
  policy keeps the last good 2D position and flags the entity dangling (toast +
  feature-tree marker); `Strict` raises a rebuild error. Default: BestEffort.
- **Source not on/near a single plane** (non-planar face): project its boundary
  edges only; do not attempt a planar fill.
- **Self-reference / cyclic** (projecting geometry created downstream of the
  sketch): rejected at pick time (the source must precede the sketch in the
  feature order).
- **Performance**: large numbers of projected entities re-resolved every rebuild
  — guard with the per-sketch flag and cache resolution within a rebuild pass.

## Research Basis

Persistent naming / GeomRef resolution follows the existing `feature-engine`
design ([#16] Mantyla half-edge topology, the repo's GeomRef system). No new
geometric algorithm — projection is an affine world→plane transform; the
parametric behavior reuses the rebuild + persistent-naming machinery already in
the engine. The "driven external reference" model mirrors mainstream parametric
CAD projected/converted geometry (Onshape "Use", SolidWorks "Convert Entities").

## Implementation Plan (each its own red/green increment)

1. **Transform util** (waffle-types or feature-engine): world↔plane 2D, with the
   round-trip unit test. Mirrors `buildSketchPlane`.
2. **Position resolver** (`resolve.rs`): `resolve_by_position` + unit tests.
3. **Data model** (`sketch.rs`): `ProjectedSource` + `Option<source>` on Point
   (serde default); serialization round-trip test; solver treats projected
   points as fixed.
4. **Rebuild reprojection** (`rebuild.rs`): reproject projected points each
   rebuild; integration oracle (box-height edit).
5. **Bridge** message + dispatch; **WASM rebuild**.
6. **UI**: live `project` tool for vertex/edge/face; GUI oracle.
7. **Validation/adversarial**: dangling source, non-planar face, cyclic ref.

---

# Cycle 2 (2026-07-05): Picking & flow fixes — select-first projection

> STATUS: IMPLEMENTED (2026-07-05). Full FIP cycle: red tests
> (`projection-select-first.spec.js`, 5/8 red on baseline) → implementation →
> adversarial validation (`projection-select-first-adversarial.spec.js`, 11
> guards). User report: while sketching with a body in view, no hover or
> selection of body geometry occurs, and the Project entry point is
> undiscoverable. Real-pointer diagnosis (Playwright, real mouse events)
> confirmed the projection *engine* path works (vertex + face project and bind
> correctly under the `project` tool) but found the flow/picking layer has one
> design gap and two genuine bugs. UI-only change: no modeling crates are
> touched; the projection bindings machinery from Cycle 1 is reused unchanged.
>
> VALIDATION FINDINGS (fixed in-cycle, each with a regression guard):
> - Scale regression caught by the Test Author: the first occlusion fix used a
>   distance-relative margin while the edge pick threshold was 0.06 absolute
>   world units — faces of default-drawn bodies (~0.01–0.02 world) were
>   unpickable everywhere (0/540 pixel grid scan). Root fix: screen-space
>   calibration in `app/src/lib/viewport/picking.js` (6px edge threshold,
>   1.5px occlusion margin, per-frame `worldPerPixel`). Also exposed that
>   `face-select.spec.js`'s `kind==='Face'` assertion is satisfied by datum
>   planes — SR1/SR2 assert the picked face's `FeatureOutput` anchor instead.
> - Adversary: additive face selection double-toggled (two click paths);
>   stale selection under a drawing tool projected on `J`; undo left stale
>   `projectedBindings`. All fixed.
> - Implementer: bare click with no prior pointermove read a stale hover
>   (12/12 wrong-selection repro) — user-reachable, fixed with a pixel-
>   freshness guard (`getFreshHoveredRef`, 4px) rather than trusting the last
>   arbitrated hover.
> - Out-of-scope finding (filed in `projects/04-3d-viewport/PLAN.md`
>   Blockers): auto-fit frames high-aspect "needle" bodies with the camera
>   inside the solid (sketch-XY vs extrude-Z unit-scale mismatch, ~14×);
>   NOT a picking defect.
> - Known pre-existing reds, baseline-verified untouched by this cycle:
>   dimension-tool.spec.js:241 (mm/m), snap-preview-candidates.spec.js:143,
>   extrude-regions.spec.js:41/:167, sketch-on-face-workflow.spec.js:96.

## Goal

While a sketch is active with the **Select** tool, body vertices / edges /
faces are hoverable and click-selectable; pressing the **Proj** toolbar button
(or `J`) projects the selected body entities into the sketch (live-bound, as in
Cycle 1). The existing tool-first flow (activate Proj, then hover+click) keeps
working unchanged. Additionally fix the two picking bugs found in diagnosis.

## Findings being fixed

- **F1 (design gap / discoverability)**: body picking is gated OFF in sketch
  mode unless `activeTool === 'project'` (`CadModel.svelte:532` no-op raycast;
  `EdgeOverlay.svelte:191,213` and `VertexOverlay.svelte:178,187` early
  returns). With the default tool nothing highlights, which reads as "feature
  missing".
- **F2 (bug)**: edge hover is unreachable in the straight-on sketch view.
  `EdgeOverlay.handleEdgePointerMove` / `handleEdgeClick` early-return whenever
  `isFaceHitAtPosition()` is true (`EdgeOverlay.svelte:199,220`); looking down
  the sketch normal, a face is *always* hit behind every boundary edge, so no
  edge ever highlights. Root cause: the face test checks *existence* of a face
  hit, not *occlusion* (is the face strictly in front of the edge?).
- **F3 (bug, minor)**: hover priority at corners is last-writer-wins between
  `CadModel.handlePointerMove` (Face) and `VertexOverlay.handlePointerMove`
  (Vertex) — the highlight flickers by mouse-approach path.

## Branch Table

| Sketch-mode tool | Body hover/click | Proj button click | J shortcut |
|---|---|---|---|
| Select, body entities selected | enabled | project ALL selected body refs now; clear that selection; stay in Select | same as button |
| Select, nothing selected | enabled (hover highlight + click selects) | activate `project` tool (existing flow) | same |
| `project` tool | enabled (existing) | no-op (already active) | no-op |
| Any drawing tool (line/rect/circle/arc/…) | **disabled** (unchanged gate) | activate `project` tool | same |
| Not in sketch mode | unchanged (normal modeling picking) | n/a (button hidden) | n/a |

Selection kinds accepted for projection: Vertex, Edge, Face (mixed multi-select
allowed; each dispatches to the Cycle 1 `projectVertex` / `projectEdge` /
`projectFace` store path — curved edges keep Cycle 1's static-snapshot
behavior).

## Invariants

- **I1 — Sketch entities win**: with Select active in sketch mode, if a sketch
  entity is under the pointer (`sketchHover` non-null), body hover is
  suppressed and a click selects the sketch entity, never the body ref behind
  it. Body picking must not regress sketch selection, dragging, or
  `sketch-drawing-regression.spec.js`.
- **I2 — Occlusion, not existence**: an edge (or vertex) is hoverable iff no
  face hit is *strictly closer to the camera* than the edge/vertex hit point
  (beyond a small depth epsilon). A face coplanar-behind or containing the
  edge never suppresses it. This fixes F2 in all camera orientations, not just
  the sketch view.
- **I3 — Deterministic hover priority**: at the same pointer position,
  Vertex ≻ Edge ≻ Face, independent of mouse-approach path (fixes F3).
- **I4 — Same bindings**: entities projected via select-first are byte-identical
  in `projectedBindings` to the same entities projected tool-first (both funnel
  through the same store functions).
- **I5 — Drawing tools unaffected**: with any drawing tool active, body
  hover/click remains fully gated off (no raycast hits, no highlight).
- **I6 — Selection intercepts unaffected**: `selectRef` intercepts
  (extrude profile pick, revolve axis pick, sketch-plane selection) do not
  trigger from sketch-mode Select-tool body clicks.

## Oracles (GUI, real pointer events only — no `__waffle.projectX` shortcuts)

- **O1**: box body → sketch on its front face → Select tool → real-hover body
  vertex/edge/face → `getHoveredRef()` returns the matching kind for each
  (I2, I3 checked at a corner and at an edge midpoint in the straight-on view).
- **O2**: real-click a body edge → ref selected; click Proj →
  `getProjectedBindings()` grows by the edge's endpoint bindings; entity count
  grows by 2 points + 1 line; selection cleared; tool still `select`.
- **O3**: same for a face (4 corner bindings + 4 construction lines) and a
  vertex (1 binding, 1 point).
- **O4**: with Line tool active, real-hover over the body → `getHoveredRef()`
  stays null (I5).
- **O5**: draw a sketch line crossing in front of a body face; hover the line →
  `sketchHover` set and `getHoveredRef()` null; click selects the line (I1).
- **O6**: tool-first flow regression: activate Proj, hover+click a body edge in
  the straight-on view → edge projects (F2 fix, previously impossible).
- **O7**: `sketch-drawing-regression.spec.js` and `projection.spec.js` stay
  green.

## Failure modes

- Selected ref kind not projectable (e.g. whole-body selection): Proj click
  ignores it with a toast, projects the projectable remainder.
- Mixed selection with duplicates (edge + its endpoint): project both; the
  binding side-table dedup (Cycle 1 corner dedup) applies within a face only —
  duplicate points across picks are acceptable (same as tool-first behavior).

## Research Basis

No new geometry. Select-then-command mirrors mainstream parametric CAD
(Onshape "Use"/"Project", SolidWorks "Convert Entities" — both accept
pre-selection). Occlusion-aware picking (I2) is standard raycast hit-depth
comparison; no published-algorithm citation applicable.
