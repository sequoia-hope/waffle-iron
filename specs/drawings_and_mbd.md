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

Status: **D1a landed 2026-10-03, with the one-view DXF export of §12.**
Everything else is still design. Written 2026-10-03 from a survey of the tree.
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
2. **D1b — silhouettes.** For each curved face, the locus where the surface
   normal is perpendicular to the view direction, clipped to the face's
   trimming loops. Cylinder: two lines. Cone: two lines through the apex.
   Sphere: a circle. Torus: two closed curves, computed analytically on the
   (θ,φ) chart and sampled. Silhouette curves carry `source` = the face.
3. **D1c — visibility.** Split every projected curve at the (u,v) crossings
   with every other projected curve and at silhouette tangencies, then
   classify each segment by casting a ray from the segment midpoint along the
   view direction toward the viewer and testing for a face hit in front of the
   segment's 3D depth. The face hit test uses the solid's tessellation at
   render chord tolerance with the exact in/out predicates in cherchi-rs as
   the tie-breaker when the ray grazes a face. Segments that are coincident in
   (u,v) after projection and have the same visibility are merged.
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
| D1b | analytic silhouettes | D1a | kernel-v2 |
| D1c | visibility classification + oracle | D1b | kernel-v2 |
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
R12 DXF in millimetres. It is a WIREFRAME until D1b/D1c, and the tool's own
description says so, because a caller who is not told would ship a drawing with
the far edges in it and never know.

## 13. What this is not

- Not a sketch replacement. Drawings never drive geometry; the sketch solver
  stays the only place a dimension changes a part.
- Not a renderer in Rust. The kernel emits curves; text, arrows and line
  weights are the app's.
- Not a PMI importer. Reading PMI from STEP is a separate spec once M3's
  writer and oracle exist.
- Not a fillet, chamfer or shell dependency. Tangent-edge display (`ViewStyle`)
  handles the case where those arrive later.
