# Spec: Projected sketch geometry with live source coincidence

> STATUS: IMPLEMENTED (vertex projection end-to-end). Increments 1–7 of the
> §Implementation Plan are landed (commits 3219bf5e, fbd8ead6, 89c9a373,
> e0c9b0ab, f144feab, 9fdc6f9a, a512c5f5): SketchPlaneBasis transform,
> resolve_by_position, ProjectedEntity side-table, rebuild reprojection,
> FinishSketch bridge + WASM, the projectVertex UI tool, and the dangling-source
> adversarial case. NOT YET DONE: live projection of **edges** and **faces** in
> the UI (the data model supports EdgeSample; the tool only projects vertices so
> far), pick-time cyclic-ref rejection (the engine degrades gracefully to
> dangling instead), and a full in-browser rebuild oracle for the parametric
> update (covered by the Rust integration test reproject_tracks_moved_source_vertex).

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
