# 2D manufacturing drawings and Model-Based Definition

A drawing is a derived document: a sheet of views of one or more parts or
assemblies, each view a projection of B-Rep geometry onto a plane, with
annotations whose values are measured from the model. Model-Based Definition
(MBD) is the same annotation vocabulary attached directly to 3D topology, with
tolerance semantics, exported so other tools can read it. The two share most of
their machinery, so this spec covers both and sequences them together.

Owner crates: `kernel-v2` (D1, D2), `waffle-types` (D0, D3, M1), `file-format`
(D4), `feature-engine` (D0, D3, M1, M2), `wasm-bridge` (D5), `app` (D3, D4, D5,
M2), `step-export` work in `kernel-v2::step_export` (M3).

Status: **D1a, D1b and D1c landed 2026-10-03, with the one-view DXF export of
§12.** Everything else is still design. Written 2026-10-03 from a survey of
the tree.
The v4 document model (`specs/waffle_v4_document_model.md` §Phase 4, line 503)
reserved the `Drawing` tab kind and named the kernel projection debt (line
489) that this spec carries as D1.

What D1a put in the tree:

- `waffle_types::kernel::projection` — the §5.1 contract: `KernelProjection`,
  `ViewFrame`/`ViewBasis`, `Curve2` (with exact `bbox`, `length`, `flatten`),
  `ProjectedCurve`, `ViewGeometry`, `ProjectOpts`, `ProjectionBody`,
  `SectionResult`. Every trait method defaults to a typed `NotSupported`, so
  D1b–D1d extend this shape rather than renegotiating it, and an
  unimplemented increment is loud. `MockKernel` implements none of them.
- `kernel_v2::projection` — orthographic projection of every B-Rep edge, line
  and circle/arc surviving analytically (ellipse, circular and edge-on
  degenerate branches all exact), everything else as the render-identical
  sample polyline. Implemented on `KernelV2Adapter` as `project` /
  `project_bodies` (assembly placements composed into the view basis).
- `kernel_v2::dxf_export` — R12/`AC1009`, millimetres, layers `VISIBLE` and
  `HIDDEN`, with a golden file. R12 has no `ELLIPSE` entity, so an obliquely
  seen rim is a `POLYLINE` at a proved sagitta bound; the analytic ellipse
  stays in `ViewGeometry` for the SVG renderer and a later R13+ writer.
- `wasm-bridge` — `UiToEngine::ExportDxf` and the MCP tool `export_dxf`
  (named views plus a free direction, `deliver` agent/download), beside
  `export_step` and `export_stl`.
- Oracles — the §5.3 checks per primitive in `kernel_v2::projection::tests`
  and corpus-wide in `test-harness/tests/projection_corpus_oracle.rs`
  (`#[ignore]`, stride-sampled).

Two things §5.3 as written cannot assert at D1a, and both are now recorded in
those tests rather than worked around: the projected bbox can only EQUAL the
AABB's projection for a solid whose extremes lie on edges (a curved solid's
lie on a silhouette, which is D1b), and the kernel's own `solid_aabb` is
conservative on circle edges and declines a solid carrying a surface-pair
curve, so the equality is asserted for prismatic cases and containment for the
rest.

What D1b added, the same day:

- `kernel_v2::projection::silhouette` — the §5.2 increment-2 locus for all
  four curved surfaces, clipped exactly to each face's trimming loops, with
  every degeneracy the plan names (and one it did not: the edge-on torus has
  four exact silhouette curves, not two). Reported as `CurveKind::Silhouette`
  with `source` = the face, tagged `Visible` until D1c.
- `kernel_v2::project_solid` — edges then silhouettes, which is what
  `project` / `project_bodies` / `export_dxf` now answer, so a flat-pattern
  DXF of a curved part carries its outline.
- The §5.3 bbox equality now holds: literally against each fixture's exact
  support function in `silhouette::tests`, and corpus-wide as a sandwich
  between the conservative AABB above and the render tessellation below (see
  "Implementation notes (D1b)" for why the conservative AABB cannot be the
  reference on its own).

What D1c added, also the same day:

- `kernel_v2::projection::crossings` — the §5.2 increment-3 split. Every
  `Curve2` decomposes into segments and conics, so three pair kinds carry the
  whole problem; segment × segment and segment × conic are closed form (the
  latter a quadratic in the conic's own normalized frame, exact for a circle
  and an ellipse alike), and conic × conic is bracketed and bisected on the
  second's exact implicit function along the first.
- `kernel_v2::projection::visibility` — the classification. Each split piece
  is classified at its midpoint by a ray toward the viewer against the solid's
  render tessellation, with `yang_rs::segment_intersects_triangle_3d` (Cherchi
  2022 §3 over Shewchuk's `orient3d`, re-exported for kernel-v2's dep rules)
  deciding wherever the float solve is not; adjacent pieces that agree are
  rejoined, and curves coincident in `(u, v)` with the same visibility are
  deduplicated.
- `waffle_types::kernel::projection` — `Curve2::subcurve` plus `eval` and
  `param_range` over every arm (a polyline is parameterized by chord index, so
  a sub-polyline is exact rather than resampled); `CurveDepth` on each curve;
  and **`ProjectionDeclines`** on `ViewGeometry`, which closes the open finding
  D1b's own notes recorded — the clip's declines were print-only, so no oracle
  could pin them.
- `kernel_v2::dxf_export` — the `HIDDEN` layer carries lines, with
  `box_oblique_hidden.dxf` as its golden, and the `export_dxf` tool
  description no longer tells callers that hidden-line removal is missing.
- Oracles — the §5.3 visibility oracle in
  `test-harness/tests/projection_visibility_oracle.rs` (a software
  orthographic depth buffer, no GPU) and the per-primitive pins in
  `kernel_v2::projection::visibility::tests`.

What D1c added, also the same day:

- `kernel_v2::projection::crossings` — the §5.2 increment-3 split. Every
  `Curve2` decomposes into segments and conics, so three pair kinds carry the
  whole problem; segment × segment and segment × conic are closed form (the
  latter a quadratic in the conic's own normalized frame, exact for a circle
  and an ellipse alike), and conic × conic is bracketed and bisected on the
  second's exact implicit function along the first.
- `kernel_v2::projection::visibility` — the classification. Each split piece
  is classified at its midpoint by a ray toward the viewer against the solid's
  render tessellation, with `yang_rs::segment_intersects_triangle_3d` (Cherchi
  2022 §3 over Shewchuk's `orient3d`, re-exported for kernel-v2's dep rules)
  deciding wherever the float solve is not; adjacent pieces that agree are
  rejoined, and curves coincident in `(u, v)` with the same visibility are
  deduplicated.
- `waffle_types::kernel::projection` — `Curve2::subcurve` plus `eval` and
  `param_range` over every arm (a polyline is parameterized by chord index, so
  a sub-polyline is exact rather than resampled); `CurveDepth` on each curve;
  and **`ProjectionDeclines`** on `ViewGeometry`, which closes the open finding
  D1b's own notes recorded — the clip's declines were print-only, so no oracle
  could pin them.
- `kernel_v2::dxf_export` — the `HIDDEN` layer carries lines, with
  `box_oblique_hidden.dxf` as its golden, and the `export_dxf` tool
  description no longer tells callers that hidden-line removal is missing.
- Oracles — the §5.3 visibility oracle in
  `test-harness/tests/projection_visibility_oracle.rs` (a software
  orthographic depth buffer, no GPU) and the per-primitive pins in
  `kernel_v2::projection::visibility::tests`.

Fillet, chamfer and shell remain deferred and nothing here depends on them.

---

## 1. Scope

In scope:

1. **Drawing views** — orthographic projection of a part or assembly along a
   view direction, with visible and hidden edges distinguished, silhouettes of
   curved faces, and planar section views with hatchable cap regions.
2. **A drawing sheet** — a new tab kind holding views, their placement and
   scale, and annotations.
3. **Drawing annotations** — driven dimensions, notes, centre marks and centre
   lines, with values measured from the model at rebuild.
4. **Flat exports** — DXF and SVG of a view or a sheet; PDF via the browser.
5. **MBD** — a tolerance data model, datum features, geometric tolerances
   anchored to faces and edges, rendered in the 3D viewport, and exported as
   AP242 semantic PMI.

Out of scope for this spec: isometric shading or rendered views, bill of
materials and balloon automation beyond a plain table of assembly instances,
weld symbols, surface-finish symbols beyond a text note, and reading PMI from
imported STEP. Each of those can hang off the structures defined here later.

## 2. What exists today, and what is missing

### 2.1 Already in the tree

- **Tab kinds with forward compatibility.** `TabKind` in
  `crates/file-format/src/metadata.rs:236` has `Part`, `Assembly` and an
  untagged `Unknown(Value)` that round-trips verbatim. Adding `Drawing` is not
  a `MIN_READER_VERSION` bump. The plug-in points are the enum, the
  `KnownTabKind` mirror, `TAB_KIND_TAGS`, the custom `Deserialize`, the
  hand-written `JsonSchema`, and the `Tab::part` / `Tab::assembly` constructor
  siblings.
- **Scoped references.** `GeomRef` (`crates/waffle-types/src/geom_ref.rs:11`)
  with `RefScope` already addresses geometry in another tab or inside an
  assembly instance path. A drawing view's source and an annotation's anchors
  are `GeomRef`s with scope; no new reference type is needed.
- **Dimension vocabulary.** Sketch constraints already include `Distance`,
  `PointLineDistance`, `HDistance`, `VDistance`, `Angle`, `Radius`, `Diameter`
  (`crates/waffle-types/src/sketch.rs:578-657`), each with an optional driving
  `expression` and a `reference: bool` flag for driven dimensions excluded
  from the solver. The driven-dimension concept exists in the data model.
- **Kernel introspection.** `KernelIntrospect`
  (`crates/waffle-types/src/kernel/traits.rs:338-459`) exposes faces, edges,
  vertices, full adjacency, `edge_polyline`, `entity_axis`, volume, surface
  area, AABB, and `face_provenance`. Analytic surface and curve types are
  exposed through `kernel/analytic.rs`.
- **Face-level persistent ids.** `Pid` (`crates/kernel-v2/src/arena.rs:135`)
  and the operation journal (`journal.rs`) give per-face lineage through
  booleans.
- **Units.** A document-level `display_unit` and a JS formatter table
  (`app/src/lib/units.js`).

### 2.2 Missing

Listed in dependency order; each later item needs the earlier ones.

1. **Edge and vertex persistent ids, and content-seeded Pids.** Pids cover
   faces only and are allocation-order dependent (the F4a reseeding note at
   `arena.rs:135` is unimplemented). Almost every dimension and every geometric
   tolerance anchors to an edge or vertex, so without this an annotation
   detaches whenever its feature is re-executed.
2. **Silhouette, hidden-line classification, and planar section.** Edge
   projection LANDED as D1a (`kernel_v2::projection`, 2026-10-03); the rest of
   D1 has not. The viewport's section view is still a three.js stencil cap
   (`app/src/lib/viewport/SectionCap.svelte`), not geometry, and
   `KernelProjection::section_with_plane` is a typed `NotSupported`. Before
   D1a, edge extraction existed only as render polylines
   (`kernel_v2::extract_edges`), which is still what the polyline arm of the
   projection samples.
3. **A 2D paper-space renderer.** The sketch editor is the threlte 3D viewport
   locked to a plane. Dimension rendering is an HTML label and a bare leader
   line (`app/src/lib/sketch/DimensionLabels.svelte`); there are no
   arrowheads, extension lines, tick marks or text metrics.
4. **A measurement-to-expression bridge.** The expression environment
   (`crates/feature-engine/src/params.rs`) is a map of design parameters.
   Measurement lives one layer out, in the MCP `body_measure` tool. No value
   can flow from geometry into a document number.
5. **Tolerance, precision, material.** No dimensional tolerance type, no
   per-dimension precision or dual-unit display (precision is a hard-coded
   default argument in the formatter), no material or density on a body.
6. **A writable annotation store on topology.** `entity_meta` is a read-only
   query. There is no per-face or per-edge property anywhere.
7. **AP242 with PMI.** `kernel_v2::step_export` writes AP214 geometry only: no
   presentation, colour, property or PMI entities.

## 3. Architecture

```
                 ┌────────────────────────────────────────────┐
                 │ app (Svelte)                               │
                 │  DrawingSheet.svelte  (SVG paper space)    │
                 │  PmiOverlay.svelte    (3D viewport PMI)    │
                 └───────────────▲────────────────────────────┘
                                 │ ViewGeometry, AnnotationLayout (JSON)
                 ┌───────────────┴────────────────────────────┐
                 │ wasm-bridge                                │
                 └───────────────▲────────────────────────────┘
                                 │
┌────────────────────────────────┴───────────────────────────────────┐
│ feature-engine                                                     │
│  Drawing tab rebuild: resolve view sources (RefScope) → project    │
│  → measure annotations → cache ViewGeometry                        │
│  Datum / Pmi features in the part tree                             │
│  expr env gains measure(...) functions                             │
└────────────────────────────────▲───────────────────────────────────┘
                                 │ KernelProjection trait (waffle-types)
┌────────────────────────────────┴───────────────────────────────────┐
│ kernel-v2                                                          │
│  projection::{project_edges, silhouettes, classify_visibility}     │
│  section::section_with_plane  (half-space boolean via yang-rs)     │
│  arena: Pid for edges + vertices, content-seeded                   │
└────────────────────────────────────────────────────────────────────┘
```

Three rules hold the layering:

- **Rust produces curves and numbers; the app draws them.** The kernel returns
  2D analytic curves tagged visible or hidden; the sheet component lays out
  arrowheads and text. No text metrics in Rust.
- **Drawings are derived, never driving.** A drawing dimension's value is
  measured. Editing a drawing never edits a part. Driving dimensions stay in
  sketches; MBD nominal values read from the model.
- **Annotations are features.** PMI items and datums live in the feature tree
  so they get rebuild ordering, undo, suppression and persistent-naming
  resolution for free. The drawing tab's annotations live in the tab, since
  they reference other tabs' trees.

## 4. D0 — Persistent ids for edges and vertices (prerequisite)

Owner: `kernel-v2` arena and journal; `waffle-types` types.

1. **Content-seeded face Pids** (the F4a note). A face created by a construct
   op is seeded from a structural key: the creating feature's id, the role, and
   for side faces the sketch entity's id. Re-executing an unchanged feature
   reproduces the same Pids. Faces created by a boolean take the operand face's
   Pid when they are a trimmed descendant of exactly one input face, and a
   derived key hashed from both parents when they are not.
2. **Edge Pids.** An edge's Pid is derived from its two adjacent face Pids plus
   a disambiguator when the same face pair shares more than one edge (a
   cylinder's two end rims against the same side face). Intersection edges
   born in a boolean take the pair of operand face Pids they lie on.
3. **Vertex Pids.** Derived from the set of incident edge Pids, with the same
   disambiguator rule.
4. **Introspection.** `face_provenance` gains edge and vertex siblings;
   `GeomRef` gains `Selector::Pid { pid, root_pid }` as the preferred selector
   for annotation anchors, with `resolve_with_fallback` falling to `Signature`
   when the Pid is gone.
5. **Oracle.** A test-harness check over the assay corpus: rebuild every case
   twice from the same document and assert the full Pid map is identical;
   then apply a no-op edit (re-save the same parameter) and assert again.

Done when the corpus passes the identity oracle and `sketch-on-face`,
`UpTo` terminations and 3D-sketch attachments resolve through `Selector::Pid`
in the GUI suite.

### Implementation notes (D0)

Landed 2026-10-03. Where the plan above left a choice open, this is the
choice made and why.

**Edge and vertex ids are seeded from face lineage ROOTS, not face pids.**
Item 2 says an edge is derived from "its two adjacent face Pids". Taken
literally that is unusable: `boolean_op` constructs new faces, so every face
of a body receives a fresh `Pid` the moment anything on the body is unioned
or cut, and an edge id built on those would churn with them. The journal
(KV13 F2) already recovers the pid where each face's geometry was
*introduced*, so:

```
edge_pid   = H("edge",   root(face₁), root(face₂), rank)    // roots sorted
vertex_pid = H("vertex", sorted incident edge pids…, rank)
```

Item 2's separate rule for intersection edges ("take the pair of operand
face Pids they lie on") then needs no special case: such an edge's two
output faces descend from exactly those operands, so their roots *are* that
pair. `crates/kernel-v2/src/pid.rs` holds the derivation;
`crates/kernel-v2/tests/d0_pid_identity.rs` is the kernel-level oracle.

**The disambiguator is a rank inside the content group.** For edges: order
the group's members by the edge's *unordered* endpoint pair under an exact
total order on coordinate bits (no quantization, so the order is a function
of the bits and cannot flip between runs); the rank is the position. For
vertices: the same, ordered by position. A group whose members compare equal
is `KernelV2Error::PidAmbiguous` — a loud refusal, because choosing one
would silently rebind an annotation on the next rebuild. `PidMissing` (a
solid whose faces were never stamped) and `PidCollision` (two distinct keys
hashing to one id) are the other two refusals; none of the three is ever
repaired.

A rank is a *position*, which is the scheme's one stability caveat and is
worth knowing before relying on it: a group of one — the overwhelming
majority — ranks 0 whatever its geometry does, but inside a group of two or
more, moving one member past another renumbers both and their two ids swap,
even though neither changed its content key. Sign-of-zero counts as a move
(`-0.0` orders below `+0.0`, as under `f64::total_cmp`). Making a
multi-member group order-independent needs the content key itself to separate
its members, which is the F4a face reseed below. Pinned as
`rank_groups_renumbers_a_group_when_a_member_moves_past_another`.

**The hash is frozen.** `H` is a chain of SplitMix64 finalizer steps over
`u64` words, domain-separated per entity kind, with `Pid(0)` avoided. These
ids are persisted inside documents, so the function must never drift — treat
`pid.rs`'s `mix`/`digest` as format, not as an implementation detail.
`crates/kernel-v2/tests/d0_pid_hash_frozen.rs` is the oracle: it holds the
literal ids of the unit box. A red result there is a format break needing a
reader-floor bump and a migration, never new constants. It is also the
cross-process half of the stability claim — its literals were recorded by a
different process than the one asserting them, and nothing in the derivation
reads a `HashMap`, an address or a clock.

**A face's identity does not ride on the edge pass.** `solid_pids` refuses as
a whole, so `all_entity_pids(solid, Face)` originally lost every FACE id of a
body whose edge groups were ambiguous — while `entity_pid` kept answering
those faces through `face_provenance`, so the two doors disagreed. The face
pass is `pid::solid_face_pids`, and the `Face` arm takes it.

**A pid is unique only WITHIN one body.** Collision detection is per solid,
and two bodies split out of one operation can carry edges with the same
adjacent-face roots and so the same id. `resolve_by_pid` therefore requires
the anchor's `output_key` to still exist and refuses rather than falling back
to the feature's first body the way `Selector::Position` does: that fallback
is a rebinding step, and a pid looked up in the wrong body can find a
different edge under the stored number.

**Ids are derived, never stored.** `solid_pids(arena, solid)` recomputes
from the arena and the journal. Nothing was added to `BrepArena` (whose
`Debug` string the determinism oracle compares), so there is no second
source of truth to invalidate and no way for a stale id to outlive its
geometry. The cost is one `O(E log E)` pass; `all_entity_pids` is the bulk
door so a resolver never pays it per entity.

**One contract door for all three kinds.** Item 4 asked for "edge and vertex
siblings" of `face_provenance`. Instead there is one pair of
`KernelIntrospect` methods over every kind — `entity_pid(entity, kind)` and
`all_entity_pids(solid, kind)` — returning `EntityPid { pid, root_pid }`.
`face_provenance` stays as it is for its KV13 F5/F6 callers. For an edge or
vertex `root_pid == pid` (the id is already content-seeded through the
roots), so a consumer matches `pid` then `root_pid` without ever branching
on kind. Both methods default to `None`/empty, so the addition is additive
for every implementor; mesh-backed imported bodies report nothing, since a
face-index-derived number would change silently on re-import.

**`Selector::Pid` never falls back to `Signature`.** This is a deliberate
deviation from item 4's "`resolve_with_fallback` falling to `Signature` when
the Pid is gone". A Signature fallback is a nearest-match, and a drawing
dimension that quietly moves to a geometrically similar edge reports a wrong
number while looking entirely healthy — the P9/P10 case. So a Pid whose id
and root are both absent fails loudly under `BestEffort` exactly as under
`Strict`, with separate messages for "gone", "the root now names several
entities" (split geometry) and "this kernel reports no identity map for this
body". A caller that genuinely wants best-effort rebinding stores a
`Signature` selector, which already does that and says so in its warnings.

**Still open after this increment:**

- *Content-seeded FACE pids (item 1, the F4a reseed).* Face pids remain
  monotonic. They are reproduced exactly by a full rebuild of an unchanged
  document, but an INCREMENTAL rebuild re-runs only the edited feature in an
  arena whose allocator has advanced, so that feature's faces are stamped
  fresh — measured on a plate+boss on 2026-10-03: a boss depth edit moved its
  face roots `{6,8,9,10,11} → {23,25,26,27,28}` while the plate's `{0..5}`
  were untouched. Consequence: the four edges where the boss meets the plate
  are renamed by an edit that does not move them. Pinned as the `#[ignore]`d
  `edges_at_the_junction_with_an_edited_feature_keep_their_ids_too` in
  `crates/test-harness/tests/d0_pid_selector.rs` — un-ignore it in the PR
  that lands the reseed. The reseed is cross-crate (the kernel does not know
  feature ids today; it needs the creating feature's id, the role, and for
  side faces the sketch entity's id to reach the constructor).
- *The corpus-wide oracle (item 5).* Not run: the identity oracle here is
  focused (a box, a cylinder, a plate+boss union through the engine), not the
  assay corpus rebuilt twice.
- *Every consumer must reach the live resolver first.* A `Selector::Pid` is
  answerable only by `resolve_geom_ref_live`. Several production paths still
  call `resolve_with_fallback`, which has no kernel — including the `UpTo`
  and edge-reference resolutions in `feature-engine`'s `rebuild` and the
  assembly-context path in `context`. They refuse loudly today (fallback only
  ever applies to `Selector::Role`, so there is no silent rebinding), but the
  first feature that stores a Pid in one of those fields must move its call
  site to the live form in the same PR.
- *The "done when" GUI clause.* `sketch-on-face`, `UpTo` terminations and
  3D-sketch attachments still store their existing selectors; nothing writes
  a `Selector::Pid` yet. Note for whoever does: `Selector` is a
  serde-tagged enum, so the first document that persists a `Pid` selector
  cannot be read by an older reader — that lands with a format reader-floor
  bump, which this increment did not need.

## 5. D1 — Kernel projection and section

Owner: `kernel-v2`, new module `projection`; trait extension in `waffle-types`.

### 5.1 Trait

```rust
pub trait KernelProjection {
    fn project(&self, solid: SolidId, view: &ViewFrame, opts: &ProjectOpts)
        -> Result<ViewGeometry, KernelError>;
    fn section_with_plane(&self, solid: SolidId, plane: &Plane)
        -> Result<SectionResult, KernelError>;
}

pub struct ViewFrame { origin: Point3, dir: Vector3, up: Vector3 }

pub struct ViewGeometry {
    curves: Vec<ProjectedCurve>,          // in view-plane (u,v), model units
    bbox: Aabb2,
}

pub struct ProjectedCurve {
    geometry: Curve2,                      // Line | Circle | Ellipse | Polyline
    visibility: Visibility,                // Visible | Hidden
    kind: CurveKind,                       // Edge | Silhouette | SectionOutline
    source: Option<GeomRef>,               // scoped ref to the 3D edge or face
}

pub struct SectionResult {
    cap_loops: Vec<Vec<Curve2>>,           // outer + inner loops, hatchable
    cut_solid: SolidId,                    // the half-space result, for projection
}
```

### 5.2 Increments

1. **D1a — edge projection. LANDED 2026-10-03.** Orthographic projection of
   every B-Rep edge. Analytic curve types survive where they can: a line
   projects to a line or a point; a circle projects to an ellipse, a circle, or
   a line segment; other curves (ellipse, hyperbola, SSI curves) project as
   polylines sampled at the chord tolerance. All edges are tagged `Visible` at
   this increment, which gives a wireframe view. Three deviations from §5.1's
   sketch, argued in `waffle_types::kernel::projection`'s module docs:
   `ProjectedCurve::source` is a `KernelId`, not a `GeomRef` (the kernel cannot
   mint one — a `GeomRef` needs the feature anchor); `Curve2` has a `Point` arm,
   since this increment says a line may project to one; and
   `ViewGeometry::bbox` is an `Option`, because a view with no curves has no
   box. Handles are `KernelSolidHandle` and `section_with_plane` takes an
   origin/normal pair, the kernel contract's own vocabulary.
2. **D1b — silhouettes. LANDED 2026-10-03.** For each curved face, the locus
   where the surface
   normal is perpendicular to the view direction, clipped to the face's
   trimming loops. Cylinder: two lines. Cone: two lines through the apex.
   Sphere: a circle. Torus: two closed curves, computed analytically on the
   (θ,φ) chart and sampled. Silhouette curves carry `source` = the face and
   are tagged `Visible` (visibility is D1c). See "Implementation notes (D1b)"
   below: every degeneracy this plan names is handled, the EDGE-ON torus turns
   out to have four exact silhouette curves rather than two, and the clip is a
   local enter/exit classification of exact boundary crossings rather than a
   parity walk over a sampled chart polygon.
3. **D1c — visibility. LANDED 2026-10-03.** Split every projected curve at the
   (u,v) crossings with every other projected curve and at silhouette
   tangencies, then classify each segment by casting a ray from the segment
   midpoint along the view direction toward the viewer and testing for a face
   hit in front of the segment's 3D depth. The face hit test uses the solid's
   tessellation at render chord tolerance with the exact in/out predicates in
   cherchi-rs as the tie-breaker when the ray grazes a face. Segments that are
   coincident in (u,v) after projection and have the same visibility are
   merged. See "Implementation notes (D1c)" below: the tie-breaker's
   `Intersects` covers a boundary TOUCH as well as an interior crossing and
   taking a touch for occlusion is wrong, the one tangency this plan names is
   on a polyline fold rather than on the analytic arms, and the declines are
   now typed and counted on the result.
4. **D1d — section.** `section_with_plane` runs the yang pipeline with a
   half-space operand built as a box that encloses the solid's AABB with a
   margin, then collects the cap face (the face whose plane equals the cut
   plane, found through `face_provenance` as the only face descended from the
   box operand) and returns its loops. The caller projects the cut solid with
   D1a–c for the section view and hatches the cap loops. The cut plane is
   generic with respect to the solid in the common case; a cut plane coplanar
   with a model face hits the Stage-0 coplanar overlay, which is the correct
   outcome (the section passes through a face) and is handled there.

### 5.3 Oracles

- **Projection oracle.** For every assay case, project along ±x, ±y, ±z and
  check that the projected bbox equals the solid AABB's projection, and that
  the union of visible-curve lengths is invariant to a 180° rotation about the
  view axis.
- **Visibility oracle.** Render the solid's tessellation with an orthographic
  depth buffer in the test harness (software rasterizer, no GPU) and sample
  each visible segment at 16 points: every sample must lie within one chord
  tolerance of the depth buffer's front surface. Hidden segments must not.
- **Section oracle.** The cap area from `section_with_plane` must equal the
  area of the stencil-cap polygon the app computes today for the same plane,
  within chord tolerance, for every corpus case cut at its AABB centre.

### 5.4 Non-goals

Perspective projection, curved section lines, broken-out and detail views
beyond cropping a parent view's `ViewGeometry` to a rectangle.

### Implementation notes (D1b)

Landed 2026-10-03, in `kernel_v2::projection::silhouette`. Where §5.2's
increment 2 left a choice open, this is the choice made and why.

**The locus is analytic for all four surfaces, and so are its degeneracies.**
With `w` the unit line of sight, `a` a surface's axis, `w∥ = w·a`,
`m = |w − w∥·a|`, `u₁ = (w − w∥·a)/m` and `u₂ = a × u₁`:

| surface | silhouette | the degeneracies |
|---|---|---|
| cylinder | two rulings at `axis ± R·u₂`, parallel to the axis | `m = 0` (seen along the axis): NONE, the rims are the outline |
| cone | two rulings through the apex at `r̂ = c·u₁ ± √(1−c²)·u₂`, `c = tan α·w∥/m` | `|c| = 1`: one grazing ruling. `|c| > 1`: NONE — the viewer is inside the cone's own shadow. `m = 0`: NONE |
| sphere | the great circle in the plane through the centre ⊥ `w` | none; and since that plane IS the view plane, it projects to an exact `Curve2::Circle` of the sphere's radius |
| torus | two closed branches `φ(θ) = atan2(−m·cos θ, w∥)` and `+π`, on the `(θ, φ)` chart | `m = 0` (along the axis): the two equator circles `ρ = R ± r`, exact. `w∥ = 0` (edge-on): FOUR exact circles — the two latitude circles of radius `R` at `τ = ±r` and the two profile circles of radius `r` at `θ = ±π/2` |

The edge-on torus is the one case §5.2's "two closed curves" undercounts: at
`w∥ = 0` the branch equation factors, and the silhouette really is four
curves (the classic side view of a doughnut — two circles joined by their
common tangents, where the tangents are the latitude circles seen edge-on).
The threshold between the branch form and the four-circle form is `|w∥| ≤
1e-12`; above it the branch form is used for every `w∥`, because its total
arc length is bounded by `2π(R + 2r)` no matter how small `w∥` gets (the
branch's `φ` variation is at most `2π`), so there is no sampling blow-up to
avoid — only an exact answer to prefer where one exists.

**Clipping is local, not a parity walk.** §5.2 asks for the locus "clipped to
the face's trimming loops", exactly in the face's own chart. Rather than
develop the face's boundary into a chart polygon and walk parity — which would
inherit the chart's seam handling and the boundary's sampling — each path
carries a scalar **functional** whose zero set contains it, and the clip is
the set of crossings of the boundary with that functional:

- For every path but a torus branch the functional is a PLANE distance: the
  cylinder's and cone's rulings lie in a plane through the axis, and the
  sphere's great circle and the torus's coordinate circles are plane sections.
  Crossings with a line, circle, arc, ellipse arc or hyperbola arc are then
  closed form — `C + A·cos t + B·sin t = 0`, or a quadratic in `eᵗ` for the
  hyperbola.
- A torus branch's functional is `n·w` itself, and a `SurfacePair` boundary
  edge has no closed form against either; those crossings are bracketed on a
  dense parameter sample and bisected to float precision. The surface-pair
  case rides that curve's render polyline, so it carries that polyline's chord
  error — the same band every other kernel-v2 consumer of a surface-pair curve
  carries, and recorded here rather than hidden.
- The zero set of a plane functional holds BOTH rulings of an axial plane (and
  both profile circles of a meridian plane), so a crossing that is not on this
  path is dropped by a distance test. That is what makes "a silhouette line on
  a partial cylinder may be absent or a sub-segment" come out right. The
  tolerance of that test is the crossing's own representation error, and it is
  NOT one number: a root on an analytic parameterization is exact to float,
  while a root on a chord — a surface-pair curve's render polyline, or a
  `LineSegment` bounding a curved face, which is what a boolean output's
  boundary actually is — sits up to a chord sagitta off the true curve, four
  orders looser. C0065 is the measurement: a torus patch bounded by 110 line
  segments lost BOTH silhouette branches to a single `1e-6` tolerance. The
  paths the test has to tell apart are `2R` apart, so the chord band
  discriminates them with four orders to spare.
- Each surviving crossing is ENTER or EXIT by the sign of `S·(N×T)` — path
  tangent against the inward direction `N × T`, since a loop walk puts the
  face's material on its left. A vanishing sign is a tangency and does not
  toggle. An open path (a ruling) that runs off the end of its crossing list
  is clamped to the boundary's own parameter extent, which is how a cone face
  containing the apex keeps the segment from the apex to its single rim
  crossing.

**A seam is not a boundary.** A half-edge whose twin lies in the SAME face is
a slit the face continues across — the closed sphere's meridian, the closed
torus's profile circle. Those are dropped before the clip, which is what lets
a closed surface report its whole silhouette instead of a piece interrupted at
the seam.

**One global question, asked once.** A closed path with NO crossings on a face
that does have a boundary is wholly inside or wholly outside, and nothing
local decides which. A face whose only boundary is its seams IS the whole
closed surface, so that case is settled by construction; otherwise the
face's own render triangles settle it — the mesh is inscribed in the face, so
a point on the face is within a chord sagitta of it and a point elsewhere on
the same surface is at the distance separating it from the face's region. Only
this branch pays for it, once per path. It is a decision rather than an
estimate except where the path runs within a sagitta OUTSIDE the boundary,
i.e. tangent to it — the same grazing configuration the enter/exit sign test
declines.

That sagitta is MEASURED from the mesh in hand, never from the chord
tolerance the projection was asked for, and the distinction is not academic.
`tessellate_face` always meshes at the render band, so a verdict keyed to the
caller's own density compares against a mesh that was never built: at any
chord tolerance finer than the render default the acceptance band falls below
the fixed mesh's real gap, every sample reads as off the face, and every
closed path is dropped. Measured 2026-10-03, during the review of this
increment: a torus with a bore that misses both equator circles reported them
at the render density and lost BOTH at `n_seg = 1024` — asking for a finer
drawing silently deleted the outline. The band is now the sagitta of the
mesh's own longest chord against the surface's tightest curvature,
`ρ − √(ρ² − h²/4)`, with `h` read off the triangles and `ρ` the sphere's
radius or the torus's MINOR radius; it has no density in it, and at the render
band it is tighter than the formula it replaces (1.7e-5 against 3.2e-5 on that
torus). The oracle is the property rather than the default: the verdict is
pinned equal at five densities spanning two orders.

**Two configurations still decline rather than guess**, both censused under
`KV2_SILHOUETTE_CENSUS`:

- *A grazing removal.* The corpus has one: C0065 punches a 0.5 × 0.5 square
  hole through a torus's tube at `x = 1.2`, which removes an arc of each
  latitude circle — and the hole's boundary is TANGENT to the latitude
  circle's own plane at both ends of that arc, so the removal has no
  transversal crossing to find. Those two paths are dropped whole rather than
  drawn through the hole. The view's bbox is unaffected (the profile circles
  reach the same extremes), so §5.3 still holds; what is lost is two arcs of a
  drawing, and the census says so.
- *A non-alternating crossing sequence* — a tangency the sign test did not
  catch, or a boundary running along the silhouette. Three paths of R0087's
  422-face gear body hit this.

Both are under-reports, never over-reports, and both are loud.

**What a silhouette projects to.** A cylinder's or cone's ruling is a straight
segment, so it stays a `Curve2::Line`; a ruling that projects to a single
point is dropped, since the rims already carry it. A sphere's great circle and
a torus's coordinate circles go through the same `project_circle` the rim edges
use, so they come out as an exact `Circle`, `Ellipse` or edge-on `Line`. Only
a torus's oblique branches are sampled, and they are refined ADAPTIVELY —
bisecting any chord whose midpoint deviates by more than the render sagitta —
rather than uniformly in `θ`, because a branch's speed in `θ` is wildly uneven
as `w∥ → 0` and a uniform sample would miss the steep stretch while
over-sampling the flat one.

**Curve order.** Silhouettes are appended AFTER the edges, so D1a's contract
that the nth curve is the nth `extract_edges` edge survives as a statement
about the `CurveKind::Edge` prefix. The DXF writer needs no change: a
silhouette is `Visible`, so it lands on the `VISIBLE` layer, and a
flat-pattern export of a curved part is now correct where before it was
missing the outline.

**§5.3's bbox equality, and what it can honestly be measured against.** §5.3
writes "the projected bbox equals the solid AABB's projection". D1a could only
assert that for prismatic solids, and silhouettes are only half the reason:
the kernel's own `introspect::conservative_aabb` is documented CONSERVATIVE
and bounds a circular EDGE by the box of its whole circle, so a `z`-axis
cylinder's rim at `z = 0` inflates the reported box to `z ∈ [−R, R]`, and a
torus to the cube `centre ± (R + r)`. An equality against that would measure
the AABB's slack. So the oracle now reads:

- in `kernel_v2::projection::silhouette::tests`, the equality is asserted
  against each fixture's **exact support function** in closed form
  (`max(t₀,t₁)(d·a) + R·s` for a cylinder, `max τ(d·a + tan α·s)` for a cone
  frustum, `c·d + R` for a sphere, `R·s + r` for a torus, with
  `s = |d − (d·a)a|`), for all four curved fixtures in all six axis views. It
  is exact for the cylinder, frustum and sphere and within the render sagitta
  on the torus, whose oblique branches are inscribed.
- in `test-harness/tests/projection_corpus_oracle.rs`, the corpus-wide
  equality is SANDWICHED: the projected bbox must lie inside the AABB's
  projection (D1a's half) and must CONTAIN the render tessellation's
  projection (the new half — the mesh is inscribed, so its box is a sound
  lower bound on the solid's, and at D1a a curved solid failed it by its whole
  radial bulge). Where the two agree the sandwich IS §5.3's literal equality;
  where they do not, the AABB is itself conservative there and the pair is a
  typed decline (`aabb_conservative`), counted and reported rather than
  asserted away.

Measured 2026-10-03 at the default stride 8 over the 334-case corpus — 42
cases, 263 s in `--release`, 39 projected in all six directions, 3 not built
(C0113, P0013, R0007, the assay's own business), 216 of 234 `(case,
direction)` pairs boundable, all 234 pinned from below by the tessellation, 0
failures. The AABB is TIGHT on 136 of the bounded pairs and conservative on
80; on all 136 tight ones §5.3's literal bbox equality HOLDS, against **134
with the edge curves alone**.

Two things are worth reading off that. The equality moves only from 134 to
136 pairs, because a tight AABB and a curved extreme rarely coincide: most
corpus cases are prismatic outlines with interior curved features, whose
global bbox the edges already reached, and the cases whose outline IS curved
are exactly the ones whose AABB is conservative. The check D1b actually turns
green is the TESSELLATION containment — projecting the same sample with
`project_edges` instead of `project_solid`, 2 of the 42 cases fail it (the
ones whose outline is a silhouette: a torus, a bored revolve) and at D1b all
42 pass. The sharp statement lives in the per-primitive half instead, where
the projected bbox equals each fixture's exact support box in all six views
and the brute-force sweep matches 28,924 exact silhouette points both ways.

The corpus also found two real clip defects, both needing a boolean output's
boundary and so invisible to any hand-written fixture: a single on-path
tolerance four orders too tight for a crossing found on a chord (C0065's torus
patch, bounded by 110 line segments, lost its whole outline), and a closed
path with no crossings declined where the face's own triangles can decide it.
Both are fixed; the commits carry the reasoning.

### Implementation notes (D1c)

Landed 2026-10-03, in `kernel_v2::projection::{crossings, visibility}` with the
contract additions in `waffle_types::kernel::projection`. Where §5.2's
increment 3 left a choice open, this is the choice made and why.

**The pipeline is split, classify, merge, and the merge is not cosmetic.**
§5.2 names the three steps and it is worth being explicit that the third is
load-bearing rather than tidying. A box seen from a generic direction has its
far-vertex edges crossing its near-vertex ones in `(u, v)`, so the split cuts
several of its twelve edges — and every piece of each comes back with the same
verdict, because a convex solid's edge is wholly visible or wholly hidden.
Without the merge the drawing would carry a curve per piece and the DXF a
`LINE` per piece, for twelve edges. The pin is
`a_box_from_a_generic_direction_has_nine_visible_and_three_hidden_edges`: the
answer is twelve curves, nine visible and three hidden, and the three are
exactly the ones reaching the far vertex — one assertion over all three steps.

**A split piece keeps its parent's analytic KIND.** The splitting primitive is
`Curve2::subcurve` in `waffle-types`, and it answers the same arm over a
sub-interval: half a projected rim is a `Curve2::Ellipse` of the same centre
and axes over half the parameter window, not a polyline. That is what keeps a
hidden-line-removed drawing writing true `ARC` entities, and it is why
`Curve2::eval`/`param_range` now answer for EVERY arm — including the polyline,
which is parameterized by chord index plus fraction so its integer parameters
are its own vertices and a sub-polyline is exact rather than resampled.

**Crossings are closed form for two pair kinds of three.** Every `Curve2`
decomposes into segments and conics and no third thing, so there are three
pair kinds. Segment × segment is one 2×2 solve. Segment × conic is a QUADRATIC
in the segment's parameter, taken in the conic's own normalized frame where the
conic is the unit circle and the segment is still a segment — exact for a
circle and an ellipse alike, which is the payoff for carrying the projected
ellipse analytically through D1a. Conic × conic is a quartic, and is solved
instead by bracketing the strict sign changes of one conic's implicit function
along the other and bisecting; the implicit is exact, so a transversal root
converges to it, and what a sample can miss is a pair of roots closer together
than its spacing, which is the near-tangential case and is declined.

**Two degeneracies are deliberately NOT declines, and one of them was a bug.**
A parallel or coincident piece pair has no transversal crossing and needs no
split — what it needs is the coincidence merge — so it is passed over silently.
Counting it would bury the real declines under the ordinary degeneracy of an
axis-aligned view, where a box's front and back faces project exactly onto each
other. The conic form of the same statement had to be found the hard way: a
through hole's two rim circles come back with radii differing in the last bit,
so the implicit of one along the other is ~3e-16, and the sign noise around
zero minted THIRTEEN spurious roots and cut one rim into alternating visible and
hidden arcs. Two distinct conics meet in at most four points and cannot agree
on an arc, so "the implicit is zero everywhere along it" is a sound test for
"the same conic", and it is `COINCIDENT_IMPLICIT`. Pinned as
`two_coincident_circles_report_no_crossings`.

**A tangency puts an exact zero in the sample array, and a zero must not
bracket.** The companion defect: two internally tangent circles have `f = 0`
exactly at the sample on their contact, and a sign test that counts a zero as
one side reports two spurious transversal roots around it. So the bracket is
STRICTLY opposite signs, and a sample that lands inside the band is a separate
CONTACT pass — the signs on either side of it say which kind it is: opposite
is a transversal root that happened to land on a sample, equal is a tangency,
which is counted and not split.

**The classification is a ray cast, and what makes it answerable is a MEASURED
offset.** A piece is classified at its parameter midpoint by lifting that 2-D
point to 3-D and asking whether any face stands in front. The ray cannot start
on the surface it belongs to: the render mesh is inscribed, so a point on a
curved face's true surface sits OUTSIDE the mesh by up to the chord sagitta and
the ray would graze its own face. The offset is the distance from the lifted
point to the nearest candidate triangle — measured at that point, from the mesh
in hand — plus a float margin. It is NOT the sagitta the caller's chord
tolerance implies, for exactly the reason D1b records one layer down:
`tessellate` always meshes at the render band, so a band keyed to the caller's
density describes a mesh that was never built. That offset is the ONLY
approximation in the verdict: an occluder standing closer to the curve than the
local mesh gap cannot be distinguished from the curve's own surface, which is a
thin-feature band of the render density.

**A 2-D point lifts to its NEAREST pre-image, and that is a drawing decision.**
Each projected curve travels with the 3-D sample polyline of its own source —
an edge's `introspect::edge_polyline`, a silhouette's own path samples, in both
cases the sampling the rest of the kernel already uses for that entity — and a
2-D point is lifted by taking the minimum DEPTH among the chords that pass
within float noise of it. The projection is not injective: a rim seen edge-on
collapses its near and far halves onto one segment, and the drawing shows the
near one. Taking the nearest pre-image is what makes a cylinder's end rim come
out whole and visible in a side view instead of half-dashed. Pinned as
`an_edge_on_circle_lifts_to_its_near_half`.

**The hit test is exact where the float test is not decisive, and a GRAZE does
not occlude.** A float Möller–Trumbore solve answers the generic case; where
the barycentric coordinates or the hit parameter land inside a float band — the
ray through a triangle's edge or vertex, or along its plane — the verdict comes
from `yang_rs::segment_intersects_triangle_3d`, Cherchi 2022 §3's primitive over
Shewchuk's adaptive `orient3d` (re-exported through yang-rs for the same reason
the CDT is: kernel-v2 may not depend on cherchi-rs directly). No orientation
predicate is implemented here.

But the exact predicate's `Intersects` covers an edge or vertex touch as well as
an interior crossing, and taking a touch for occlusion is wrong: **a face hides
a curve only by standing BETWEEN it and the viewer, which means the ray crosses
from one side of it to the other.** A face the ray merely grazes separates
nothing. That is not a convenient reading — it was measured. Before the rule, a
cylinder's far rim came back HIDDEN behind its own bore, because the rim's
samples and the end disc's polygon come from the same angular sampling and the
ray went exactly through a shared vertex. The configuration is systematic
rather than accidental, so it is counted, as `ray_grazes_face`: a bore's far rim
sits at exactly the radius of the inscribed wall it grazes, and an axis-aligned
view of a prismatic solid grazes every face parallel to its line of sight.
A nonzero `ray_grazes_face` is how a caller tells a degenerate view from a
decided one.

**A degeneracy at ONE point of a piece is a coincidence; at every point it is
the configuration.** The graze rule above is right about what a grazing contact
means and wrong about when to apply it, and the corpus said so: a slot's
blind-end edge in C0009 runs along `y = 0`, which is also the symmetry line the
occluding face's own CDT put a triangulation seam on — so the ray from the
piece's midpoint passed exactly through that seam, every incident triangle
reported a boundary touch, and a face standing a fifth of the solid in front of
the edge was taken for a graze. NINETEEN of the forty-two cases the visibility
oracle sweeps failed before this was fixed, and the stride-48 sample of seven
had shown none of it: the defect needed a face whose triangulation seam happens
to lie under a curve, which is a coincidence only a corpus produces.

The fix is not a different rule but a different POINT. Visibility is constant
along a piece — that is what the split established — so any interior point of
it gives the piece's verdict, and a cast that comes back degenerate can simply
be redone elsewhere on the same piece: `RECAST_FRACTIONS` tries the midpoint
first and then four other interior fractions, stopping at the first cast that
either finds an occluder or grazes nothing. One point to the side of a
triangulation seam, the same ray crosses a triangle's interior and the occluder
is found. Only a piece that grazes at EVERY probed point counts
`ray_grazes_face` — and that is the real configuration, a curve lying IN a face
parallel to the line of sight, which no choice of point can escape.

**Of two coincident curves the survivor is the NEARER.** Coincidence is tested
geometrically — sample one, measure against the other's flattening, both ways —
rather than by comparing representations, since the same point set can arrive as
an `Ellipse` from one rim and a `Polyline` from another; the sweep is over the
`u` order with an active set, so it costs the number of curves times the number
that overlap any one of them. Curves of DIFFERENT visibility are never merged,
which is the case that matters: those are a near line and a far one and a
drawing needs both. Of two that do merge the farther goes, with a tie going to
the later index — so a silhouette that reproduces an edge at the same depth
loses to the edge, and the one line that is drawn keeps the `CurveKind::Edge`
tag. That is not hypothetical: a frustum's seam edge runs exactly along one of
its two silhouette rulings, so an edge-on view of it reports ONE silhouette, not
two (`a_frustum_seen_edge_on_shows_four_curves_and_merges_the_seam_ruling`).

**The declines are typed, counted and on the result.** This is the open finding
D1b's own module docs recorded — "the declines are counted nowhere, so a count
cannot be pinned" — and the fix is `ProjectionDeclines` on `ViewGeometry`, with
D1b's four silhouette declines and D1c's five. `counts()` names every field from
the struct so a report cannot drift from it, and `ViewGeometry::extend` merges
them. Every counter is an under-report of lines — a missing dashed arc — except
`cross_body`, which is the one over-report and is named apart for it:
visibility is computed per BODY against that body's own tessellation, so in a
multi-body view a curve hidden behind a DIFFERENT body is still reported
visible, and the adapter counts one per body rather than leaving it to be
discovered from a wrong drawing.

**The crossing search has a work BUDGET, not a timeout.** The search is
all-pairs over the view's curves, pruned by curve box, then by the overlap
region, then per piece pair; each pair is charged what it costs (a 2×2 solve is
the unit, a bracketed conic pair its sample count) against `SPLIT_BUDGET`.
Exhausting it leaves the remaining curves UNSPLIT — still honestly classified at
their own midpoints, just not cut — and counts `split_budget` once for the view.
A budget rather than a clock, so the boundary is deterministic: the same view
always declines in the same place. No view of the corpus sample the visibility
oracle sweeps reaches it, and that sweep ASSERTS `split_budget == 0`, so the
headroom is pinned rather than assumed.

**D1a's curve-order contract moves to `project_edges`.** "The nth curve is the
nth `extract_edges` edge" cannot survive an increment that splits curves and
drops duplicates. It is now a statement about `projection::project_edges`, the
unclassified door, which is what the D1a tests reach; the corpus projection
oracle keeps projecting through the CLASSIFIED path, since its checks are about
the point set and a merge does not change one. What survives in a classified
view is the grouping (every edge piece
before every silhouette piece, a parent's pieces consecutive and in parameter
order) and the `source` on each piece. The per-edge-sample containment check and
the DXF writer's entity-coverage tests moved to that door for the same reason:
an axis-aligned view of a prismatic solid merges its front and back outlines,
which is the right drawing and the wrong fixture for checking that the writer
emits one `LINE` per `Curve2::Line`.

**DXF.** `HIDDEN` is populated, with `box_oblique_hidden.dxf` as its golden —
nine `LINE`s on `VISIBLE` and three on `HIDDEN` for a box from a generic
direction. Both layers stay `CONTINUOUS` on purpose: R12 expresses a dashed
line type through an `LTYPE` table whose dash lengths are in drawing units, so
the right pitch depends on the sheet scale, which is the drawing sheet's
business (§8) and not a one-view flat-pattern export's. The `box_top_view.dxf`
golden shrank, because the top view's coincident top and bottom outlines now
merge: the drawing is the same drawing, the file no longer carries each line
twice.

**§5.3's visibility oracle.** `test-harness/tests/projection_visibility_oracle.rs`,
`#[ignore]`d and stride-sampled like its projection sibling. It rasterizes each
body's tessellation into a software orthographic depth buffer (512², no GPU),
samples each classified curve at sixteen interior points, and requires that
nothing stand in front of a `Visible` sample and that something stand in front
of a `Hidden` one. Nothing is shared with the kernel's answer: not the ray, not
the predicate, not the acceleration structure, and not the lift — the sample's
own depth is re-derived in the harness from `edge_polyline` — so a
classification that agreed with itself but not with the surface fails here.

§5.3 words it as "a visible sample must lie within one chord tolerance of the
depth buffer's front surface", and stating it as "nothing in front" is not a
weakening: a projected edge's sample lies ON the solid's boundary (the oracle
asserts that separately, as a projection check), so the two are the same
statement — and the "nothing in front" form is the one that can be tested at a
curve that lies on the boundary of its own faces, which every edge does.

Four things about that comparison had to be got right, and every one of them
was found by a corpus case rather than reasoned in advance:

- *The front depth is evaluated AT the sample, not at a cell centre.* A cell
  centre half a cell away can land on material that is not there along the
  sample's own line of sight — which is what a hole's rim edge, sitting inside
  the hole in the body's front face, does. The raster stays for the coverage
  census; the comparison walks the triangles covering the point.
- *Containment is a MODEL-unit test with two different margins.* A surface
  contains the sample at all if the sample is no further outside it than the
  mesh's own inscription deficit — a boss's top rim sits ~1e-4 outside the
  inscribed top disc on a 0.5-unit part — and strictly only if it is inside by
  more than the f32 quantization of `tessellate`'s vertices. A dimensionless
  barycentric margin expresses neither: a rim lying exactly in a plate's top
  plane reads as a part in 1e8 INSIDE the plate's front face, because the f32
  mesh puts that face's top edge 1e-8 above the rim.
- *The depth band carries the front surface's own SLOPE.* The mesh's facet sits
  a chord sagitta off the exact surface measured perpendicular to it, and
  converting that to the line of sight costs `√(1 + slope²)` — 1 on a face seen
  square on, divergent as the surface turns parallel to the view. R0055's rim
  near the silhouette of a 21.8-unit cylinder read 0.069 off a mesh whose
  radial sagitta is 0.021, because there the surface stands at ~68° to the view
  plane.
- *The samples are INTERIOR.* A classified piece's two endpoints are the
  crossings it was cut at, which is exactly where its visibility changes, so a
  sample sitting on one is ambiguous by construction. R0055's hidden arcs
  reported "nothing in front" at their own ends and nowhere else.

The judgement is about the CURVE, not about each sample on its own, and that is
not a convenience either. A classified piece claims one visibility for its whole
length, so a piece whose samples DISAGREE is saying something quite different
from a piece every sample contradicts: the first means the curve really does go
behind something partway along, so a CROSSING WAS NOT SPLIT, and the second
means the classification is wrong. Only the second fails; the first is counted
by curve and listed per case as `spans_a_change`, because the fix is upstream in
the split — a declined silhouette arc or a `split_tangency` is the usual reason
the outline element the curve should have been cut against is not there.

The two directions are deliberately NOT symmetric. "Not definitely in front"
already leaves a `Visible` tag standing, since a surface within the band of the
sample's own depth does not contradict it; it is not enough to call a `Hidden`
tag wrong, for which something has to be definitely NOT in front. A surface
nearer than the sample by less than its own band is the undecidable middle and
is counted. Without that distinction the sweep reported hidden arcs as having
nothing in front of them, on near-silhouette occluders whose slopes run from
0.77 to 44 over the corpus sample.

Four categories are counted rather than asserted, and each is a configuration
rather than a tolerance. **Grazing occluders**: a surface nearer than a hidden
sample that either reaches it only on a triangle's boundary — a curve lying
exactly in a face parallel to the line of sight, which the kernel counts from
its own side as `ray_grazes_face` — or is nearer by less than its own band.
**Grazing surfaces**: the front surface stands so near parallel to the line of
sight that its slope band exceeds a quarter of the solid's depth extent, so the
inscribed mesh cannot place it in depth at all. **Curves spanning a change**, as
above. **Silhouette curves**: coverage-checked only, because at a silhouette the
front and back depths coincide and a band wide enough to be sound would assert
nothing. And the sweep sums the kernel's own `ProjectionDeclines` and asserts
the three that must stay empty — `split_budget`, `cross_body`,
`depth_unliftable` — which is what the typed channel was added for.

**Measured 2026-10-03**, at the default stride 8 over the 334-case corpus — 42
cases, **380 s** in `--release`, 264 `(case, body, direction)` views, 3 not
built (C0113, P0013, R0007, the assay's own business), 5 multi-body cases
classified per body. **532,786 VISIBLE samples and 515,907 HIDDEN samples
asserted against the depth buffer.** Not asserted and counted instead: 2,045
samples uncovered, 6,384 with a grazing occluder, 4,289 on a grazing surface,
394 silhouette curves coverage-checked only, 0 curves unliftable.

The kernel's own declines over the same sweep: `split_budget` 0, `cross_body`
0, `depth_unliftable` 0 (all three asserted), `ray_grazes_face` 36,548,
`split_tangency` 1,406, `silhouette_off_face` 12,
`silhouette_non_alternating` 6, `silhouette_grazing_removal` 0,
`silhouette_no_triangles` 0.

Two residues, and they are different in kind. **218 curves over 15 cases SPAN a
visibility change** — the piece covers both states, so the crossing where it
changes was not split; that is the §5.2 split's tail, it is listed per case,
and every mechanism that can lose a crossing is already counted beside it.
**Five cases are contradicted along a WHOLE curve** (F0059, F0083, P0005,
R0047, R0087) and are pinned in `KNOWN_DISAGREEMENTS` with their two families
named in that constant's docs: a hidden curve whose occluder sits inside the
band this mesh can resolve (F0083, P0005, R0087), and a visible curve with a
face plainly in front of it on a surface that is NOT near-parallel (F0059,
P0005, R0047 — slopes 0.77 to 5.0, discrepancies up to 600 on a 1000-unit
body), which is the one family with no band argument available and so the one
to work next. The pin is a LIST and not a tolerance: nothing was widened,
every other case is asserted, the set cannot grow silently, and a case that
stops disagreeing must come off the list in the commit that fixes it.

The sweep found one of its own: nineteen cases failed before the RE-CAST rule
above, which is the defect it was built to be able to find. The seven-case
stride-48 sample had shown none of them.

**Still open after this increment:**

- *Cross-body occlusion.* Counted (`cross_body`), not computed. A multi-body
  part or an assembly view reports a curve hidden behind another body as
  visible. The fix is a view-level pass: project every body into the view, then
  classify every curve against the union of the bodies' meshes in world space,
  which means `project_bodies` doing the classification instead of delegating
  per body.
- *A fold inside an edge-on conic.* `project_circle` reports a circle whose
  plane contains the line of sight as a `Curve2::Line` over the exact `cos`
  range of its parameter window, which is the right point set but discards the
  fold where the circle turns back on itself. The nearest-pre-image lift makes
  the VERDICT right there (the near half is what is drawn), and nothing is
  currently wrong — but a consumer that wanted the two halves as separate
  curves could not get them, and `crossings::folds` finds a fold only on a
  polyline for this reason.
- *The crossing search is all-pairs.* Pruned three ways and budgeted, and
  nothing in the sampled corpus approaches the budget, but a drawing of a
  thousand-curve assembly would want a sweep or a grid over the curve boxes
  rather than the quadratic pre-filter.
- *A tangency is declined, never resolved.* `split_tangency` counts the
  contacts where two curves touch within the band without crossing. The honest
  resolution is the derivative test at the contact (a transversal crossing has
  a sign change in the cross product of the two tangents, a tangency does not),
  which needs `Curve2` to answer its own tangent — a contract addition D3's
  renderer will want anyway for arrowhead placement.
- *Pieces that SPAN a visibility change.* The oracle's own category, and the
  largest remaining tail: a classified piece whose samples disagree covers both
  states, which means the crossing where it changes was not split. Every
  mechanism that can lose a crossing is already counted — a declined silhouette
  arc (`silhouette_grazing_removal`, `silhouette_non_alternating`,
  `silhouette_off_face`), a near-tangential contact (`split_tangency`), and the
  fold a projected edge-on conic discards — so the next step is to take the
  spanning curves the sweep names, attribute each to the mechanism that lost
  its crossing, and work the mechanisms in order of how many curves they cost.
  That attribution is a session of its own and it wants the oracle's own list as
  its input, which is why the list is printed per case.
- *A SILHOUETTE's visibility is classified but not independently checked.* The
  kernel classifies silhouette curves exactly as it does edges, and the
  per-primitive pins cover the cases with a closed-form answer (a cylinder's two
  rulings, a frustum's). The corpus oracle only coverage-checks them, because at
  a silhouette the front and back depths coincide and a depth comparison there
  has no band that is both sound and non-vacuous. Checking them wants a
  different oracle — a 2-D one, over the region the body's footprint occupies,
  rather than a depth comparison.
- *Samples the inscribed mesh does not reach.* The oracle counts `uncovered`
  samples: a thin feature or a silhouette whose exact curve lies further outside
  the mesh's footprint than the chord band. Reported, not fixed; a finer
  tessellation for the oracle alone would shrink it, at the cost of no longer
  measuring the mesh the kernel actually classifies against.
- *The five pinned disagreements.* `KNOWN_DISAGREEMENTS` in the oracle, with
  the two families in that constant's own docs. The one with no band argument
  available — a visible curve with a face plainly in front of it on a surface
  that is not near-parallel (F0059, P0005, R0047) — is the one to work first,
  and R0047's "a sample sits in front of the whole solid" row is probably where
  to start, since that is a projection-level complaint and not a visibility
  one.

### Implementation notes (D1c)

Landed 2026-10-03, in `kernel_v2::projection::{crossings, visibility}` with the
contract additions in `waffle_types::kernel::projection`. Where §5.2's
increment 3 left a choice open, this is the choice made and why.

**The pipeline is split, classify, merge, and the merge is not cosmetic.**
§5.2 names the three steps and it is worth being explicit that the third is
load-bearing rather than tidying. A box seen from a generic direction has its
far-vertex edges crossing its near-vertex ones in `(u, v)`, so the split cuts
several of its twelve edges — and every piece of each comes back with the same
verdict, because a convex solid's edge is wholly visible or wholly hidden.
Without the merge the drawing would carry a curve per piece and the DXF a
`LINE` per piece, for twelve edges. The pin is
`a_box_from_a_generic_direction_has_nine_visible_and_three_hidden_edges`: the
answer is twelve curves, nine visible and three hidden, and the three are
exactly the ones reaching the far vertex — one assertion over all three steps.

**A split piece keeps its parent's analytic KIND.** The splitting primitive is
`Curve2::subcurve` in `waffle-types`, and it answers the same arm over a
sub-interval: half a projected rim is a `Curve2::Ellipse` of the same centre
and axes over half the parameter window, not a polyline. That is what keeps a
hidden-line-removed drawing writing true `ARC` entities, and it is why
`Curve2::eval`/`param_range` now answer for EVERY arm — including the polyline,
which is parameterized by chord index plus fraction so its integer parameters
are its own vertices and a sub-polyline is exact rather than resampled.

**Crossings are closed form for two pair kinds of three.** Every `Curve2`
decomposes into segments and conics and no third thing, so there are three
pair kinds. Segment × segment is one 2×2 solve. Segment × conic is a QUADRATIC
in the segment's parameter, taken in the conic's own normalized frame where the
conic is the unit circle and the segment is still a segment — exact for a
circle and an ellipse alike, which is the payoff for carrying the projected
ellipse analytically through D1a. Conic × conic is a quartic, and is solved
instead by bracketing the strict sign changes of one conic's implicit function
along the other and bisecting; the implicit is exact, so a transversal root
converges to it, and what a sample can miss is a pair of roots closer together
than its spacing, which is the near-tangential case and is declined.

**Two degeneracies are deliberately NOT declines, and one of them was a bug.**
A parallel or coincident piece pair has no transversal crossing and needs no
split — what it needs is the coincidence merge — so it is passed over silently.
Counting it would bury the real declines under the ordinary degeneracy of an
axis-aligned view, where a box's front and back faces project exactly onto each
other. The conic form of the same statement had to be found the hard way: a
through hole's two rim circles come back with radii differing in the last bit,
so the implicit of one along the other is ~3e-16, and the sign noise around
zero minted THIRTEEN spurious roots and cut one rim into alternating visible and
hidden arcs. Two distinct conics meet in at most four points and cannot agree
on an arc, so "the implicit is zero everywhere along it" is a sound test for
"the same conic", and it is `COINCIDENT_IMPLICIT`. Pinned as
`two_coincident_circles_report_no_crossings`.

**A tangency puts an exact zero in the sample array, and a zero must not
bracket.** The companion defect: two internally tangent circles have `f = 0`
exactly at the sample on their contact, and a sign test that counts a zero as
one side reports two spurious transversal roots around it. So the bracket is
STRICTLY opposite signs, and a sample that lands inside the band is a separate
CONTACT pass — the signs on either side of it say which kind it is: opposite
is a transversal root that happened to land on a sample, equal is a tangency,
which is counted and not split.

**The classification is a ray cast, and what makes it answerable is a MEASURED
offset.** A piece is classified at its parameter midpoint by lifting that 2-D
point to 3-D and asking whether any face stands in front. The ray cannot start
on the surface it belongs to: the render mesh is inscribed, so a point on a
curved face's true surface sits OUTSIDE the mesh by up to the chord sagitta and
the ray would graze its own face. The offset is the distance from the lifted
point to the nearest candidate triangle — measured at that point, from the mesh
in hand — plus a float margin. It is NOT the sagitta the caller's chord
tolerance implies, for exactly the reason D1b records one layer down:
`tessellate` always meshes at the render band, so a band keyed to the caller's
density describes a mesh that was never built. That offset is the ONLY
approximation in the verdict: an occluder standing closer to the curve than the
local mesh gap cannot be distinguished from the curve's own surface, which is a
thin-feature band of the render density.

**A 2-D point lifts to its NEAREST pre-image, and that is a drawing decision.**
Each projected curve travels with the 3-D sample polyline of its own source —
an edge's `introspect::edge_polyline`, a silhouette's own path samples, in both
cases the sampling the rest of the kernel already uses for that entity — and a
2-D point is lifted by taking the minimum DEPTH among the chords that pass
within float noise of it. The projection is not injective: a rim seen edge-on
collapses its near and far halves onto one segment, and the drawing shows the
near one. Taking the nearest pre-image is what makes a cylinder's end rim come
out whole and visible in a side view instead of half-dashed. Pinned as
`an_edge_on_circle_lifts_to_its_near_half`.

**The hit test is exact where the float test is not decisive, and a GRAZE does
not occlude.** A float Möller–Trumbore solve answers the generic case; where
the barycentric coordinates or the hit parameter land inside a float band — the
ray through a triangle's edge or vertex, or along its plane — the verdict comes
from `yang_rs::segment_intersects_triangle_3d`, Cherchi 2022 §3's primitive over
Shewchuk's adaptive `orient3d` (re-exported through yang-rs for the same reason
the CDT is: kernel-v2 may not depend on cherchi-rs directly). No orientation
predicate is implemented here.

But the exact predicate's `Intersects` covers an edge or vertex touch as well as
an interior crossing, and taking a touch for occlusion is wrong: **a face hides
a curve only by standing BETWEEN it and the viewer, which means the ray crosses
from one side of it to the other.** A face the ray merely grazes separates
nothing. That is not a convenient reading — it was measured. Before the rule, a
cylinder's far rim came back HIDDEN behind its own bore, because the rim's
samples and the end disc's polygon come from the same angular sampling and the
ray went exactly through a shared vertex. The configuration is systematic
rather than accidental, so it is counted, as `ray_grazes_face`: a bore's far rim
sits at exactly the radius of the inscribed wall it grazes, and an axis-aligned
view of a prismatic solid grazes every face parallel to its line of sight.
A nonzero `ray_grazes_face` is how a caller tells a degenerate view from a
decided one.

**A degeneracy at ONE point of a piece is a coincidence; at every point it is
the configuration.** The graze rule above is right about what a grazing contact
means and wrong about when to apply it, and the corpus said so: a slot's
blind-end edge in C0009 runs along `y = 0`, which is also the symmetry line the
occluding face's own CDT put a triangulation seam on — so the ray from the
piece's midpoint passed exactly through that seam, every incident triangle
reported a boundary touch, and a face standing a fifth of the solid in front of
the edge was taken for a graze. NINETEEN of the forty-two cases the visibility
oracle sweeps failed before this was fixed, and the stride-48 sample of seven
had shown none of it: the defect needed a face whose triangulation seam happens
to lie under a curve, which is a coincidence only a corpus produces.

The fix is not a different rule but a different POINT. Visibility is constant
along a piece — that is what the split established — so any interior point of
it gives the piece's verdict, and a cast that comes back degenerate can simply
be redone elsewhere on the same piece: `RECAST_FRACTIONS` tries the midpoint
first and then four other interior fractions, stopping at the first cast that
either finds an occluder or grazes nothing. One point to the side of a
triangulation seam, the same ray crosses a triangle's interior and the occluder
is found. Only a piece that grazes at EVERY probed point counts
`ray_grazes_face` — and that is the real configuration, a curve lying IN a face
parallel to the line of sight, which no choice of point can escape.

**Of two coincident curves the survivor is the NEARER.** Coincidence is tested
geometrically — sample one, measure against the other's flattening, both ways —
rather than by comparing representations, since the same point set can arrive as
an `Ellipse` from one rim and a `Polyline` from another; the sweep is over the
`u` order with an active set, so it costs the number of curves times the number
that overlap any one of them. Curves of DIFFERENT visibility are never merged,
which is the case that matters: those are a near line and a far one and a
drawing needs both. Of two that do merge the farther goes, with a tie going to
the later index — so a silhouette that reproduces an edge at the same depth
loses to the edge, and the one line that is drawn keeps the `CurveKind::Edge`
tag. That is not hypothetical: a frustum's seam edge runs exactly along one of
its two silhouette rulings, so an edge-on view of it reports ONE silhouette, not
two (`a_frustum_seen_edge_on_shows_four_curves_and_merges_the_seam_ruling`).

**The declines are typed, counted and on the result.** This is the open finding
D1b's own module docs recorded — "the declines are counted nowhere, so a count
cannot be pinned" — and the fix is `ProjectionDeclines` on `ViewGeometry`, with
D1b's four silhouette declines and D1c's five. `counts()` names every field from
the struct so a report cannot drift from it, and `ViewGeometry::extend` merges
them. Every counter is an under-report of lines — a missing dashed arc — except
`cross_body`, which is the one over-report and is named apart for it:
visibility is computed per BODY against that body's own tessellation, so in a
multi-body view a curve hidden behind a DIFFERENT body is still reported
visible, and the adapter counts one per body rather than leaving it to be
discovered from a wrong drawing.

**The crossing search has a work BUDGET, not a timeout.** The search is
all-pairs over the view's curves, pruned by curve box, then by the overlap
region, then per piece pair; each pair is charged what it costs (a 2×2 solve is
the unit, a bracketed conic pair its sample count) against `SPLIT_BUDGET`.
Exhausting it leaves the remaining curves UNSPLIT — still honestly classified at
their own midpoints, just not cut — and counts `split_budget` once for the view.
A budget rather than a clock, so the boundary is deterministic: the same view
always declines in the same place. No view of the corpus sample the visibility
oracle sweeps reaches it, and that sweep ASSERTS `split_budget == 0`, so the
headroom is pinned rather than assumed.

**D1a's curve-order contract moves to `project_edges`.** "The nth curve is the
nth `extract_edges` edge" cannot survive an increment that splits curves and
drops duplicates. It is now a statement about `projection::project_edges`, the
unclassified door, which is what the D1a tests reach; the corpus projection
oracle keeps projecting through the CLASSIFIED path, since its checks are about
the point set and a merge does not change one. What survives in a classified
view is the grouping (every edge piece
before every silhouette piece, a parent's pieces consecutive and in parameter
order) and the `source` on each piece. The per-edge-sample containment check and
the DXF writer's entity-coverage tests moved to that door for the same reason:
an axis-aligned view of a prismatic solid merges its front and back outlines,
which is the right drawing and the wrong fixture for checking that the writer
emits one `LINE` per `Curve2::Line`.

**DXF.** `HIDDEN` is populated, with `box_oblique_hidden.dxf` as its golden —
nine `LINE`s on `VISIBLE` and three on `HIDDEN` for a box from a generic
direction. Both layers stay `CONTINUOUS` on purpose: R12 expresses a dashed
line type through an `LTYPE` table whose dash lengths are in drawing units, so
the right pitch depends on the sheet scale, which is the drawing sheet's
business (§8) and not a one-view flat-pattern export's. The `box_top_view.dxf`
golden shrank, because the top view's coincident top and bottom outlines now
merge: the drawing is the same drawing, the file no longer carries each line
twice.

**D1b's projection oracle needed one honest correction, not a widening.** Its
check 3 — "total visible length is invariant under a 180° rotation about the
view axis" — went red on ten of the forty-two sampled cases the moment D1c
landed, and the reason is that the check never meant what its wording said. The
quantity it was built for is the projected LENGTH: a half turn about the view
axis is an isometry of the view plane, so every analytic reconstruction the
projection performs must come out the same, and at D1a and D1b every curve was
`Visible`, so summing the visible ones WAS the whole point set.

It is not any more, and D1c breaks it twice over without either break being a
defect. The half turn maps `(u, v)` to `(−u, −v)`, so the crossing roots and
the coincidence sweep's `u` ordering are computed on negated coordinates, and at
a marginal configuration a piece changes side — the VISIBLE subset swings by up
to 1.311e-1 over the sample (F0043 −z), with one direction of C0065 reporting no
visible curve at all. Summing BOTH visibilities does not rescue it either,
because the coincidence MERGE drops a curve that reproduces another and the
near-coincidence decision flips the same way: the whole point set's length
swings by up to 1.517e-2. Asserting on either would be asserting that the
split and the merge are exactly symmetric under a coordinate negation, which is
a claim about float arithmetic and not about the projection.

The BBOX has neither problem, and it is what the check was always reaching for:
a dropped duplicate's points are also in the curve that kept them and a split
tiles its parent, so the extremes are exactly what they were — while a mistake
in any analytic reconstruction (an ellipse's principal axes, its parameter
range, a degenerate branch) moves one. So check 3 now asserts that the turned
view's bounding box is the NEGATION of the original's, and both length swings
are measured and printed beside it. What judges the split itself is the
visibility oracle, against the SURFACE rather than against a rotation of
itself.

**§5.3's visibility oracle.** `test-harness/tests/projection_visibility_oracle.rs`,
`#[ignore]`d and stride-sampled like its projection sibling. It rasterizes each
body's tessellation into a software orthographic depth buffer (512², no GPU),
samples each classified curve at sixteen interior points, and requires that
nothing stand in front of a `Visible` sample and that something stand in front
of a `Hidden` one. Nothing is shared with the kernel's answer: not the ray, not
the predicate, not the acceleration structure, and not the lift — the sample's
own depth is re-derived in the harness from `edge_polyline` — so a
classification that agreed with itself but not with the surface fails here.

§5.3 words it as "a visible sample must lie within one chord tolerance of the
depth buffer's front surface", and stating it as "nothing in front" is not a
weakening: a projected edge's sample lies ON the solid's boundary (the oracle
asserts that separately, as a projection check), so the two are the same
statement — and the "nothing in front" form is the one that can be tested at a
curve that lies on the boundary of its own faces, which every edge does.

Four things about that comparison had to be got right, and every one of them
was found by a corpus case rather than reasoned in advance:

- *The front depth is evaluated AT the sample, not at a cell centre.* A cell
  centre half a cell away can land on material that is not there along the
  sample's own line of sight — which is what a hole's rim edge, sitting inside
  the hole in the body's front face, does. The raster stays for the coverage
  census; the comparison walks the triangles covering the point.
- *Containment is a MODEL-unit test with two different margins.* A surface
  contains the sample at all if the sample is no further outside it than the
  mesh's own inscription deficit — a boss's top rim sits ~1e-4 outside the
  inscribed top disc on a 0.5-unit part — and strictly only if it is inside by
  more than the f32 quantization of `tessellate`'s vertices. A dimensionless
  barycentric margin expresses neither: a rim lying exactly in a plate's top
  plane reads as a part in 1e8 INSIDE the plate's front face, because the f32
  mesh puts that face's top edge 1e-8 above the rim.
- *The depth band carries the front surface's own SLOPE.* The mesh's facet sits
  a chord sagitta off the exact surface measured perpendicular to it, and
  converting that to the line of sight costs `√(1 + slope²)` — 1 on a face seen
  square on, divergent as the surface turns parallel to the view. R0055's rim
  near the silhouette of a 21.8-unit cylinder read 0.069 off a mesh whose
  radial sagitta is 0.021, because there the surface stands at ~68° to the view
  plane.
- *The samples are INTERIOR.* A classified piece's two endpoints are the
  crossings it was cut at, which is exactly where its visibility changes, so a
  sample sitting on one is ambiguous by construction. R0055's hidden arcs
  reported "nothing in front" at their own ends and nowhere else.

The judgement is about the CURVE, not about each sample on its own, and that is
not a convenience either. A classified piece claims one visibility for its whole
length, so a piece whose samples DISAGREE is saying something quite different
from a piece every sample contradicts: the first means the curve really does go
behind something partway along, so a CROSSING WAS NOT SPLIT, and the second
means the classification is wrong. Only the second fails; the first is counted
by curve and listed per case as `spans_a_change`, because the fix is upstream in
the split — a declined silhouette arc or a `split_tangency` is the usual reason
the outline element the curve should have been cut against is not there.

The two directions are deliberately NOT symmetric. "Not definitely in front"
already leaves a `Visible` tag standing, since a surface within the band of the
sample's own depth does not contradict it; it is not enough to call a `Hidden`
tag wrong, for which something has to be definitely NOT in front. A surface
nearer than the sample by less than its own band is the undecidable middle and
is counted. Without that distinction the sweep reported hidden arcs as having
nothing in front of them, on near-silhouette occluders whose slopes run from
0.77 to 44 over the corpus sample.

Four categories are counted rather than asserted, and each is a configuration
rather than a tolerance. **Grazing occluders**: a surface nearer than a hidden
sample that either reaches it only on a triangle's boundary — a curve lying
exactly in a face parallel to the line of sight, which the kernel counts from
its own side as `ray_grazes_face` — or is nearer by less than its own band.
**Grazing surfaces**: the front surface stands so near parallel to the line of
sight that its slope band exceeds a quarter of the solid's depth extent, so the
inscribed mesh cannot place it in depth at all. **Curves spanning a change**, as
above. **Silhouette curves**: coverage-checked only, because at a silhouette the
front and back depths coincide and a band wide enough to be sound would assert
nothing. And the sweep sums the kernel's own `ProjectionDeclines` and asserts
the three that must stay empty — `split_budget`, `cross_body`,
`depth_unliftable` — which is what the typed channel was added for.

**Measured 2026-10-03**, at the default stride 8 over the 334-case corpus — 42
cases, **380 s** in `--release`, 264 `(case, body, direction)` views, 3 not
built (C0113, P0013, R0007, the assay's own business), 5 multi-body cases
classified per body. **532,786 VISIBLE samples and 515,907 HIDDEN samples
asserted against the depth buffer.** Not asserted and counted instead: 2,045
samples uncovered, 6,384 with a grazing occluder, 4,289 on a grazing surface,
394 silhouette curves coverage-checked only, 0 curves unliftable.

The kernel's own declines over the same sweep: `split_budget` 0, `cross_body`
0, `depth_unliftable` 0 (all three asserted), `ray_grazes_face` 36,548,
`split_tangency` 1,406, `silhouette_off_face` 12,
`silhouette_non_alternating` 6, `silhouette_grazing_removal` 0,
`silhouette_no_triangles` 0.

Two residues, and they are different in kind. **218 curves over 15 cases SPAN a
visibility change** — the piece covers both states, so the crossing where it
changes was not split; that is the §5.2 split's tail, it is listed per case,
and every mechanism that can lose a crossing is already counted beside it.
**Five cases are contradicted along a WHOLE curve** (F0059, F0083, P0005,
R0047, R0087) and are pinned in `KNOWN_DISAGREEMENTS` with their two families
named in that constant's docs: a hidden curve whose occluder sits inside the
band this mesh can resolve (F0083, P0005, R0087), and a visible curve with a
face plainly in front of it on a surface that is NOT near-parallel (F0059,
P0005, R0047 — slopes 0.77 to 5.0, discrepancies up to 600 on a 1000-unit
body), which is the one family with no band argument available and so the one
to work next. The pin is a LIST and not a tolerance: nothing was widened,
every other case is asserted, the set cannot grow silently, and a case that
stops disagreeing must come off the list in the commit that fixes it.

The sweep found one of its own: nineteen cases failed before the RE-CAST rule
above, which is the defect it was built to be able to find. The seven-case
stride-48 sample had shown none of them.

**Still open after this increment:**

- *Cross-body occlusion.* Counted (`cross_body`), not computed. A multi-body
  part or an assembly view reports a curve hidden behind another body as
  visible. The fix is a view-level pass: project every body into the view, then
  classify every curve against the union of the bodies' meshes in world space,
  which means `project_bodies` doing the classification instead of delegating
  per body.
- *A fold inside an edge-on conic.* `project_circle` reports a circle whose
  plane contains the line of sight as a `Curve2::Line` over the exact `cos`
  range of its parameter window, which is the right point set but discards the
  fold where the circle turns back on itself. The nearest-pre-image lift makes
  the VERDICT right there (the near half is what is drawn), and nothing is
  currently wrong — but a consumer that wanted the two halves as separate
  curves could not get them, and `crossings::folds` finds a fold only on a
  polyline for this reason.
- *The crossing search is all-pairs.* Pruned three ways and budgeted, and
  nothing in the sampled corpus approaches the budget, but a drawing of a
  thousand-curve assembly would want a sweep or a grid over the curve boxes
  rather than the quadratic pre-filter.
- *A tangency is declined, never resolved.* `split_tangency` counts the
  contacts where two curves touch within the band without crossing. The honest
  resolution is the derivative test at the contact (a transversal crossing has
  a sign change in the cross product of the two tangents, a tangency does not),
  which needs `Curve2` to answer its own tangent — a contract addition D3's
  renderer will want anyway for arrowhead placement.
- *Pieces that SPAN a visibility change.* The oracle's own category, and the
  largest remaining tail: a classified piece whose samples disagree covers both
  states, which means the crossing where it changes was not split. Every
  mechanism that can lose a crossing is already counted — a declined silhouette
  arc (`silhouette_grazing_removal`, `silhouette_non_alternating`,
  `silhouette_off_face`), a near-tangential contact (`split_tangency`), and the
  fold a projected edge-on conic discards — so the next step is to take the
  spanning curves the sweep names, attribute each to the mechanism that lost
  its crossing, and work the mechanisms in order of how many curves they cost.
  That attribution is a session of its own and it wants the oracle's own list as
  its input, which is why the list is printed per case.
- *A SILHOUETTE's visibility is classified but not independently checked.* The
  kernel classifies silhouette curves exactly as it does edges, and the
  per-primitive pins cover the cases with a closed-form answer (a cylinder's two
  rulings, a frustum's). The corpus oracle only coverage-checks them, because at
  a silhouette the front and back depths coincide and a depth comparison there
  has no band that is both sound and non-vacuous. Checking them wants a
  different oracle — a 2-D one, over the region the body's footprint occupies,
  rather than a depth comparison.
- *Samples the inscribed mesh does not reach.* The oracle counts `uncovered`
  samples: a thin feature or a silhouette whose exact curve lies further outside
  the mesh's footprint than the chord band. Reported, not fixed; a finer
  tessellation for the oracle alone would shrink it, at the cost of no longer
  measuring the mesh the kernel actually classifies against.
- *The five pinned disagreements.* `KNOWN_DISAGREEMENTS` in the oracle, with
  the two families in that constant's own docs. The one with no band argument
  available — a visible curve with a face plainly in front of it on a surface
  that is not near-parallel (F0059, P0005, R0047) — is the one to work first,
  and R0047's "a sample sits in front of the whole solid" row is probably where
  to start, since that is a projection-level complaint and not a visibility
  one.

## 6. D2 — Measurement bridge

Owner: `feature-engine` (`expr.rs`, `params.rs`).

The expression environment gains functions that read the kernel:

| function | returns |
|---|---|
| `volume(body)` | solid volume |
| `area(face)` | face area |
| `length(edge)` | edge length |
| `distance(a, b)` | min distance between two entities |
| `angle(a, b)` | angle between two planar faces or two lines |
| `radius(entity)` | radius of a cylinder, sphere, circle edge |
| `mass(body)` | volume × the body's density (see M1) |

Arguments are `GeomRef`s written in the existing selector syntax. A parameter
whose expression calls one of these acquires a rebuild dependency on the
referenced feature's output; `cached_env` is recomputed after that feature
rather than before the tree. Circular dependencies (a feature whose own
dimension reads its own output) are a typed rebuild error, surfaced as an error
toast, not a silent stale value.

This is what lets a drawing dimension's text, an MBD nominal, and a mass line
in a title block all be ordinary expressions.

## 7. D3 — Annotation model

Owner: `waffle-types` (types), `feature-engine` (evaluation), `app`
(rendering).

```rust
pub enum Annotation {
    Dimension { kind: DimensionKind, anchors: Vec<GeomRef>, value: Measured,
                tolerance: Option<Tolerance>, precision: Option<u8>,
                dual_unit: Option<String>, placement: Placement2 },
    Note      { text: String, leader: Option<GeomRef>, placement: Placement2 },
    CentreMark { anchor: GeomRef },
    CentreLine { anchors: [GeomRef; 2] },
    Datum     { label: String, anchor: GeomRef, placement: Placement2 },
    FeatureControlFrame { tolerance: GeometricTolerance, anchor: GeomRef,
                          placement: Placement2 },
}

pub enum Measured { Expr(String), Value(f64) }   // Expr re-measured per rebuild
```

`DimensionKind` reuses the seven sketch dimension kinds, with `Ordinate` added.
`Placement2` is the cosmetic offset the sketch dimension labels already have.
The same `Annotation` enum is used in the drawing tab (anchors scoped to a view
source) and in the part tree as a `Pmi` feature (anchors in the part's own
scope). One renderer in SVG for sheets, one in the 3D viewport for PMI, both
consuming the same layout record the engine emits.

Rendering, in the app, as SVG: dimension line, two extension lines with the
standard gap and overshoot, filled arrowheads with an architectural-tick
option, text with a halo gap, and leader lines with a dot or arrow terminator.
Line weights and text heights follow ISO 128 / ASME Y14.2 defaults scaled by
the view scale, and are document settings.

## 8. D4 — Drawing tab

Owner: `file-format`, `feature-engine`, `app`.

```rust
TabKind::Drawing {
    sheet:  Sheet { size: SheetSize, orientation, title_block: TitleBlock },
    views:  Vec<DrawingView>,
    annotations: Vec<Annotation>,
    preview: Option<SheetPreview>,
}

pub struct DrawingView {
    id: Uuid,
    source: GeomRef,                      // RefScope → part tab or assembly instance
    projection: Projection,               // Named(Front|Top|Right|…) | Custom(ViewFrame)
                                          // | ProjectedFrom { parent: Uuid, dir } 
                                          // | Section { parent: Uuid, plane: Plane }
                                          // | Detail { parent: Uuid, rect: Aabb2, scale }
    scale: f64,
    placement: Point2,                    // sheet mm
    style: ViewStyle,                     // hidden lines on/off, tangent edges, hatch
    cache: Option<ViewGeometry>,
}
```

Rebuild of a drawing tab: resolve each view's source through `RefScope`, run
`KernelProjection::project` (D1), measure each annotation (D2), write the
caches. A `ProjectedFrom` view derives its frame from its parent's frame and
the projection standard (third-angle default, first-angle as a document
setting). A `Section` view runs D1d on the parent's source, then projects.

Title block fields are expressions over document metadata and the measurement
functions, so `mass(part)` and a parameter table work with no special casing.

Export: `export_dxf(tab)` writes the sheet's curves on layers VISIBLE, HIDDEN,
SECTION, HATCH, DIMENSION, TEXT; `export_svg(tab)` writes the same SVG the
sheet component renders. Both are new wasm-bridge tools alongside
`export_step` and `export_stl`. PDF is the browser's print path over the SVG.

The MCP surface gains `drawing_view_add`, `drawing_view_edit`,
`drawing_annotation_add`, and `export_dxf` / `export_svg`, mirroring the
feature tools.

## 9. M1 — Tolerance, precision, material

Owner: `waffle-types`, `feature-engine`, `app`.

```rust
pub enum Tolerance {
    Symmetric { plus_minus: f64 },
    Bilateral { plus: f64, minus: f64 },
    Limits { upper: f64, lower: f64 },
    Fit { hole: FitClass, shaft: FitClass },     // ISO 286 table lookup
    Basic,                                       // boxed basic dimension
}

pub struct GeometricTolerance {
    characteristic: Characteristic,   // Flatness | Straightness | Circularity |
                                      // Cylindricity | Perpendicularity | Parallelism |
                                      // Angularity | Position | Concentricity |
                                      // Symmetry | Profile | Runout
    value: f64,
    modifier: Option<MaterialCondition>,   // MMC | LMC | RFS
    datums: Vec<DatumRef>,                 // ordered, each with optional modifier
    zone: ZoneShape,                       // Diametral | Width | Spherical
}
```

Sketch dimensions gain `tolerance: Option<Tolerance>` so a tolerance authored
in the defining sketch flows through to the drawing and to PMI without
re-authoring.

Precision and dual units become document settings with per-annotation
override. The formatter in `units.js` reads them instead of its default
argument, and gains fractional-inch output.

`Body` gains `material: Option<MaterialRef>`; a document-level material table
holds `{ name, density, appearance }`. `mass(body)` in D2 reads it; the
properties panel shows mass and centre of mass from `solid_volume` and a new
`solid_centroid` introspection call.

## 10. M2 — Datum and PMI features

Owner: `feature-engine`, `app`.

- **`Datum` feature.** Outputs a plane, axis or point under
  `OutputKey::Datum { name }`, which exists in the enum but has no producer.
  Inputs: a face (plane), a cylindrical face or an edge (axis), or a vertex
  (point), each a `GeomRef`. This also fills the unimplemented
  `Anchor::Datum` arm in `resolve.rs`.
- **`Pmi` feature.** Holds one `Annotation` (§7) anchored in the part's scope.
  Rendered by `PmiOverlay.svelte` as a billboarded frame in the 3D viewport
  with a leader to the anchored entity; selecting the frame highlights the
  anchor through the existing `face_ranges` path.
- A PMI panel lists the part's datums and tolerances and flags any whose
  anchor resolved by fallback rather than by Pid, so a detached annotation is
  visible, not silent.

## 11. M3 — AP242 export with PMI

Owner: `kernel-v2::step_export`, `wasm-bridge`.

The writer gains a schema parameter: `Ap214` (today's output, unchanged) or
`Ap242`. Under `Ap242`:

1. `FILE_SCHEMA` becomes `AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF`;
   the geometry entities are unchanged.
2. Each `Datum` feature writes `DATUM`, `DATUM_FEATURE`, and
   `SHAPE_ASPECT_RELATIONSHIP` tying it to its face.
3. Each `Pmi` dimension writes `DIMENSIONAL_SIZE` or `DIMENSIONAL_LOCATION`
   with `PLUS_MINUS_TOLERANCE` or `TOLERANCE_VALUE` for limits.
4. Each geometric tolerance writes the matching `*_TOLERANCE` entity (e.g.
   `POSITION_TOLERANCE`, `FLATNESS_TOLERANCE`) with `DATUM_SYSTEM` references
   and `MODIFIED_GEOMETRIC_TOLERANCE` for MMC/LMC.
5. Graphical presentation is written as `DRAUGHTING_MODEL` with
   `ANNOTATION_PLANE` and `TESSELLATED_ANNOTATION_OCCURRENCE` polylines from
   the same layout record the viewport draws.
6. `STYLED_ITEM` / `COLOUR_RGB` per body from the material's appearance.

Oracle: round-trip through an external AP242 reader (the test-harness already
drives truck for geometry parity; for PMI the candidate is the `steputils`
Python reader or STEPcode's `stepcode` validator, chosen when M3 starts and
documented in `docs/TESTING.md`) asserting entity counts and nominal values
per PMI item. The existing analytic geometry round-trip oracle must stay green
under both schema settings.

## 12. Increments and order

| id | increment | depends on | lands |
|---|---|---|---|
| D0 | content-seeded Pids; edge + vertex Pids; `Selector::Pid`; identity oracle | — | kernel-v2, waffle-types, feature-engine |
| D1a | edge projection, wireframe views + the §12 one-view DXF export | — | kernel-v2, waffle-types, wasm-bridge — **LANDED 2026-10-03** |
| D1b | analytic silhouettes | D1a | kernel-v2 — **LANDED 2026-10-03** |
| D1c | visibility classification + oracle | D1b | kernel-v2 — **LANDED 2026-10-03** |
| D1d | `section_with_plane` | D1a | kernel-v2 |
| D2 | measurement functions in expressions | D0 | feature-engine |
| D3 | `Annotation` types + SVG dimension renderer | D0 | waffle-types, app |
| D4a | `Drawing` tab kind, named + projected views, DXF/SVG export | D1c, D3 | file-format, feature-engine, app, wasm-bridge |
| D4b | section + detail views, title block, sheet PDF | D1d, D2, D4a | same |
| M1 | tolerance types, precision, material + mass | D2 | waffle-types, feature-engine, app |
| M2 | `Datum` + `Pmi` features, 3D PMI overlay | D0, D3, M1 | feature-engine, app |
| M3 | AP242 writer with PMI + round-trip oracle | M2 | kernel-v2, wasm-bridge |

D0 and D1 are independent and can run in parallel. D1 is the only piece that
is hard kernel work and it sits in the Yang stack's area (half-space booleans,
cherchi-rs in/out predicates, SSI silhouettes), so it belongs on the kernel
priority list rather than competing with it. Everything from D3 outward is
app and document-model work that can proceed on wireframe views while D1b–c
land.

An early deliverable with real value is **D1a + a one-view DXF export**, which
covers laser, waterjet and plasma flat-pattern workflows before any sheet UI
exists. **Both landed 2026-10-03** (see the status note at the top): the MCP
tool `export_dxf` writes one named or free-direction view of the whole model as
R12 DXF in millimetres. Since D1b (also 2026-10-03) it carries the curved
faces' silhouettes too, so a flat pattern of a curved part has its outline,
and since D1c (the same day) the far edges land on the `HIDDEN` layer instead
of being drawn as if they were near ones — the tool's description was updated
in the same commit, because a caller who is not told would either distrust a
correct drawing or redo the removal itself.

## 13. What this is not

- Not a sketch replacement. Drawings never drive geometry; the sketch solver
  stays the only place a dimension changes a part.
- Not a renderer in Rust. The kernel emits curves; text, arrows and line
  weights are the app's.
- Not a PMI importer. Reading PMI from STEP is a separate spec once M3's
  writer and oracle exist.
- Not a fillet, chamfer or shell dependency. Tangent-edge display (`ViewStyle`)
  handles the case where those arrive later.
