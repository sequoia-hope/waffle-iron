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

1. ~~**Edge and vertex persistent ids, and content-seeded Pids.**~~ **DONE
   2026-10-03.** Was: Pids covered faces only and were allocation-order
   dependent, so an annotation detached whenever its feature was
   re-executed — and almost every dimension and every geometric tolerance
   anchors to an edge or vertex. Edge and vertex ids landed as D0 items 2–3
   (`kernel_v2::pid`); the content-seeded face Pid (item 1, the F4a reseed)
   and the content-seeded BOOLEAN output face Pid (item 1b) landed the same
   day — see "Implementation notes (D0)", "(D0 item 1)" and "(D0 item 1b)"
   in §4. No face pid is allocation-order dependent any more.
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
its members, which the F4a face reseed below turned out NOT to do: it
stabilizes a face's root, but a boolean that splits one operand face into two
patches still leaves both patches rooted at that face, so their edges still
share a root pair and still need a rank. Item 1b *does* separate those
patches — it is the per-patch discriminator this asked for — but as a FACE
pid, and the edge derivation is seeded from roots rather than from face pids,
so edges do not yet benefit. Pinned as
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
on kind.

**The root is a mandatory CROSS-CHECK, not a second choice** (N1, 2026-10-03).
`resolve_by_pid` requires `pid` AND the recorded `root_pid` before it calls an
entity a match; an entity carrying the number with a different root is not
this reference's entity, and resolution falls through to the recorded root
with a warning saying the number was re-minted onto other geometry. A pid is
unique within one body at one moment, not forever: a boolean's own output
pids are still counter-allocated (see "Still open"), and a counter restarts
in a fresh arena, so a reopened document whose earlier boolean changed its
output face count re-mints the same numbers onto different faces. Measured
2026-10-03 on one plate with two pockets: a reference recorded against the
second pocket's floor came back, after a reopen, on that pocket's SIDE WALL
— matched by number alone, with the floor still present. Pinned at both
doors: `feature_engine::resolve` unit tests
(`a_recycled_pid_is_not_a_match_and_the_recorded_root_answers`,
`a_recycled_pid_with_no_surviving_root_is_refused`) and end to end in
`crates/wasm-bridge/tests/tool_names.rs`
(`a_name_on_a_boolean_output_face_does_not_move_after_a_reload`). The check
is vacuous for edges and vertices, whose `root_pid == pid` — and does not
need to be more, since their ids are content-seeded from face lineage roots
and were never counter-allocated. Both methods default to `None`/empty, so the addition is additive
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

- ~~*Content-seeded FACE pids (item 1, the F4a reseed).*~~ **LANDED
  2026-10-03** — see "Implementation notes (D0 item 1)" below. Face pids
  were monotonic: reproduced exactly by a full rebuild of an unchanged
  document, but an INCREMENTAL rebuild re-runs only the edited feature in an
  arena whose allocator has advanced, so that feature's faces were stamped
  fresh — measured on a plate+boss on 2026-10-03: a boss depth edit moved its
  face roots `{6,8,9,10,11} → {23,25,26,27,28}` while the plate's `{0..5}`
  were untouched, and the four edges where the boss meets the plate were
  renamed by an edit that did not move them.
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

### Implementation notes (D0 item 1)

Landed 2026-10-03, the F4a reseed. Item 1 asked for a face Pid "seeded from
a structural key: the creating feature's id, the role, and for side faces the
sketch entity's id". This is what was built and why it differs where it does.

**Two schemes, in disjoint halves of one number space.** A face pid is now
either content-seeded or counter-allocated, and which one is readable off the
id: content ids have the top bit set (`kernel_v2::PID_CONTENT_BASE`), and
`BrepArena::alloc_pid` refuses (`PidSpaceExhausted`) rather than crossing
into that half. So "a hash id and a counter id are never the same number" is
true by construction rather than by luck, and a dump says which scheme a
face came from. `alloc_pid` returning `Result` is the only signature change
this forced.

**The seed is an opaque 128-bit name, and the seam is one scope setter.**
`Kernel::set_construct_seed(Option<ConstructSeed>) -> Option<ConstructSeed>`
installs the identity of the step about to create geometry and returns what
it replaced; `feature_engine::rebuild` sets it from `feature.id.as_u128()`
around each feature's execution and restores it after. The alternative — a
`seed` argument on `extrude_face`, `revolve_face`, `pipe`, `sweep`,
`make_face_from_region` and every future constructor — repeats one parameter
on every door and makes each new constructor a contract change; and it would
have had to reach ~15 internal `finalize_solid` call sites and every
kernel-v2 test that builds a solid. `ConstructSeed` is deliberately opaque
(`[u64; 2]`, no uuid, no feature type): the kernel does not know what a
feature is and must not learn. The method defaults to ignoring the seed, so
`MockKernel` and any mesh-backed kernel are unchanged.

**The role is the face's LOCAL ordinal in its constructor's output, and that
IS the structural key item 1 asked for.** `BrepArena::assign_face_pids`
derives `H(seed, output ordinal, role)` where `role` is the face's position
in the solid's own face list in ascending `FaceId` order. The `FaceId`s are
arena-global and march on as other steps build, but their relative order
inside one constructor's output is a function of that constructor's creation
sequence alone — which is the face's role. Measured and pinned in
`crates/kernel-v2/tests/d0_face_seed.rs`: a polygon extrude numbers top cap
0, base cap 1, then one lateral per profile edge **in profile order**; a
circle extrude numbers base, top, lateral. The two orders differ, so neither
is stated as a general rule — each is a fixed function of its own
constructor, and the test is where to look before relying on a number. Note
what this buys over the plan's wording: a lateral's index comes from the
profile order the engine already fixed, so no sketch-entity id has to reach
the constructor, and no face's name is re-derived from a coordinate (so
nothing about it can drift with geometry).

**One step, several solids: the output ordinal.** A feature can build more
than one solid in one execution (a sketch with two profiles extrudes twice).
Their faces share every role index, so the seed alone cannot separate them;
each stamping pass consumes the next `output` ordinal of the installed scope.
A pass that stamps nothing does not burn one.

**The seed is withdrawn for a boolean.** Boolean output faces have no ROLE
in the step, so the role-indexing pass is withdrawn for them:
`boolean/from_yang.rs` clears the scope around its `finalize_solid` and
restores it, because a feature that extrudes and then auto-unions would
otherwise hand the union's faces role indices under the same seed as the
extrude's, and the two sets would compete for the same ids. At the time
this left them counter-allocated, with a journal lineage whose root is the
operand face's root; item 1b (below) gives them content ids from that root
instead. `transform_solid`/`mirror_solid` stay on the counter (`Same`
lineage back to the source face).

**A seeded pid is unique within one BODY, and a re-execution deliberately
re-mints it.** A rebuild re-executes features into the same arena, leaving
the previous incarnation's solid orphaned but still in `face_pids`; a seeded
id is a function of the step and the role, so the new incarnation takes
exactly the ids the orphan holds. That repetition is the property — "the same
step always names its faces the same way" is what a reopened document needs —
and every consumer already looks a pid up inside one body
(`pid::solid_pids`, `all_entity_pids`, and `resolve_by_pid`, which refuses
rather than searching other bodies; the pre-existing note "a pid is unique
only WITHIN one body" already said so). The collision check is therefore
scoped to the solid: two faces of ONE body may never share an id, and
`PidCollision { kind: "face" }` refuses rather than aliasing. Two faces of
one step with the same role cannot happen at all — a role is a position in a
deduped list.

**The frozen hash gained a third domain, and the existing literals did not
move.** `seeded_face_pid` is the same SplitMix64 chain with a `"FACE_V1"`
domain tag. The edge/vertex literals in
`crates/kernel-v2/tests/d0_pid_hash_frozen.rs` are read off an UNSEEDED
arena, where face pids still come from the counter, so they pin the
edge/vertex derivation alone and were untouched by the reseed — no migration
was needed, and nothing in the repo's 343-file `.waffle` corpus persists a
`Selector::Pid` to migrate (grepped). The face digest has its own literals
in the same file, recorded by a different process than the one asserting
them: the cross-process half of the stability claim.

**What is now green.** `edges_at_the_junction_with_an_edited_feature_keep_their_ids_too`
(the `#[ignore]`d D0 pin) is live. Three new pins sit beside it in
`crates/test-harness/tests/d0_pid_selector.rs`:
`a_face_pid_names_the_same_face_after_a_save_and_a_reopen` (author, edit,
save, reopen in a fresh engine and kernel — the pid → face-site map must be
the one the authoring session saw),
`a_face_root_names_the_same_face_after_a_save_and_a_reopen_through_a_union`,
and `an_edit_to_one_feature_leaves_another_features_face_roots_alone`. The
first is the equivalent of the N1 branch's `#[ignore]`d
`a_face_name_keeps_its_pid_across_an_edit_to_its_own_feature`
(`crates/wasm-bridge/tests/tool_names.rs`) — un-ignore that one when N1
merges. All four were mutation-checked by withdrawing the seed in
`rebuild.rs`: all four go red, the other four tests in the file stay green.

**Still open after this increment:**

- ~~*A boolean's own output pids remain history-dependent.*~~ **LANDED
  2026-10-03** as item 1b — see "Implementation notes (D0 item 1b)" below.
  Was: only their ROOTS were content-seeded, so an incremental edit upstream
  of a boolean re-ran it and its output faces took new counter numbers.
  The N1 pin `a_boolean_output_face_keeps_its_own_pid_across_a_reload`
  (`crates/wasm-bridge/tests/tool_names.rs`) was un-ignored at the merge.
- *Nothing has been measured over the assay corpus* (item 5's oracle is still
  unrun). The reseed changes face pid VALUES everywhere, so the corpus
  verdicts are the thing to confirm it did not disturb — not attempted here,
  one was already running.
- *`ingest` inherits the seed.* A STEP import under an installed seed stamps
  its faces by file order, which is stable across re-imports of the same
  file. Untested; the import path has no identity pin yet.
- *A script's children have no identity of their own, and must not get one
  here.* `script::execute` runs each child through `execute_feature` without
  installing a seed, so a child's faces are named by the SCRIPT feature's
  uuid plus the child's position in the script's construct sequence. That is
  deliberate, and the reason is a finding worth recording: `script/host.rs`
  mints every child's `Uuid` with `Uuid::new_v4()`, so a child feature's id
  is random per run. Seeding from it would make script-generated face pids
  change on every rebuild — strictly worse than the parent's stable seed. A
  script child gets a durable identity only once the host mints its ids
  deterministically (e.g. hashed from the script source position), and that
  is the prerequisite for any annotation anchored inside a script.

### Implementation notes (D0 item 1b)

Landed 2026-10-03. Item 1 content-seeded a CONSTRUCT face's pid and left a
boolean's own output faces on the arena counter, so only each output face's
lineage ROOT was stable. The counter is allocation order, and allocation
order is a function of the editing session — so a face's number depended on
how you got to the model, not on the model.

**The measured hazard.** A 40×40×10 plate with two blind pockets, the body
being the SECOND cut's output. Pocket 2's floor is pid 22 when authored.
Deepen the FIRST pocket into a through hole and the floor becomes pid 47
(the counter has advanced). Save and reopen — a reopen replays the features
from scratch, with none of the editing history — and pid 22 names pocket 2's
SIDE WALL at `[35, −30, 8]` while the floor is pid 20. A stored name
resolves by pid, with no warning, to a different face. Measured on
`crates/test-harness/tests/d0_pid_selector.rs`'s own fixture by withdrawing
the reseed, which is also the mutation check for the two pins there.

**The derivation.** `H("FBOOL_V1", op seed, lineage root, rank)` —
`kernel_v2::pid::seeded_boolean_face_pid`, applied in bulk by
`boolean_output_face_pids`. The `op seed` is the boolean feature's own seed,
already installed by `feature_engine::rebuild` around the whole feature
execution (so an extrude's auto-union is covered too). The `root` is the
output face's lineage root, stable since item 1. Its own hash domain, so a
construct-born and a boolean-born face cannot alias on equal key words; top
bit set, so hash ids stay disjoint from counter ids.

**The disambiguator is a rank over the face's OWN boundary geometry.**
Several output faces share a root exactly when the boolean split one operand
face into patches. They are ordered by `pid::face_boundary_key` — the
sorted, deduped list of the face's boundary-vertex positions under the
f64-bit total order `point_key` already uses. Sorted rather than just the
lowest vertex because comparing two sorted lists compares their minima
first, so the primary discriminator is the same ("the face's lowest-ordered
boundary vertex", the D0 edge-rank idea) and equal minima get free
tie-breaking instead of becoming an ambiguity. A group of one — the
overwhelming majority — ranks 0 whatever its geometry does, and nothing
outside the patch can renumber it.

**Ties refuse, they do not get ordered.** The rank is the count of DISTINCT
keys ordering below a face's own, not its index in the sorted order, so two
faces whose keys compare equal take the same rank and the same id, and the
existing stamp refuses `PidCollision { kind: "face" }`. There is no third
thing to break that tie which is not an arena number, and an arena number is
what item 1b removes. Pinned on a lamina, whose front and back really are
two faces over one boundary
(`pid::tests::two_indistinguishable_patches_of_one_root_are_refused`).

**Where the re-stamp happens, and why not in `from_yang`.** `from_yang.rs`
cannot do it: the root is `face_lineage(operand pid)` and `from_yang` never
sees the operand solids — it has a yang BRep and nothing to attribute it to.
So `boolean_op` re-stamps (`reseed_boolean_output_pids`) after the output is
assembled and validated, overwriting the provisional counter ids
`finalize_solid` handed out. And *before* `record_boolean_evolution`, not
after: the root wanted is the operand's history as it stands before this
boolean appends to it, so re-stamping first makes the journal edge point at
the content id and the walk back from it reach the same root the derivation
used. The attribution lookup both passes need is one shared
`output_face_sources`, so the lineage the journal records and the lineage the
pid is derived from cannot drift apart. (The item-1 note predicted this would
have to "move the stamping pass after `boolean_op` records the journal" —
before, not after, is the correction.)

**A chained boolean records NO lineage edge for a carry-through, and that is
forced.** An output face's id is independent of WHICH boolean of a chain
produced it, so two booleans under one step seed — a cut against several
bodies, a multi-tool combine, the two-pocket plate — hand the face at an
untouched site the same id in the intermediate body and in the final one.
That is correct (same feature, same root, same rank: it is the same
conceptual face), but the second boolean's edge would then be `(P → P)`, and
`journal::face_lineage` walking a self-loop spins to its corruption budget
and reports `P` as its OWN root — which would silently detach every edge pid
seeded from that root (items 2–3). So an edge whose input and output pid are
equal is not recorded: the face already carries the name the earlier
operation gave it, and its ancestry is already in the journal from that
operation. The operand still counts as `sourced` and still consumes its
`Same` claim, so a second output face from the same operand is still a
`Split` with a real edge. Found by measurement while pinning the chain, not
by reading.

**What a boolean output pid is unique WITHIN.** One solid, exactly as for
item 1, and for the same reason — the collision check is per output solid.
Deliberately NOT disambiguated by a per-boolean ordinal within the feature:
an ordinal is positional, so deleting one target body of a multi-target cut
would renumber the others' outputs, which is the class of instability item 1b
exists to remove. The cost is that one feature cutting two bodies with the
same tool gives both outputs the same id at the tool-rooted faces — genuinely
the same tool face in both, and already covered by "a pid is unique only
within one body".

**The frozen hash gained a fourth domain, and the existing literals did not
move.** The edge, vertex and construct-face literals in
`crates/kernel-v2/tests/d0_pid_hash_frozen.rs` are read off geometry with no
boolean in it, so they were untouched; the new domain has its own literals in
the same file, recorded by a different process than the one asserting them.
Re-grepped: the 343-file `.waffle` corpus persists no `Selector::Pid` (only
the v5 schema and the MCP tool manifest name the variant), so no migration
was needed.

**The pins.** `crates/kernel-v2/tests/d0_boolean_face_seed.rs` — the
derivation, the same cut in a busier arena, the split-patch rank, the
chained-boolean roots, the unseeded fallback, and a FRESH-PROCESS check that
re-executes the test binary and compares its child's ids (a literal pin at
that layer would also pin which operand face the kernel happens to split, and
would go red for a kernel improvement that is not a format break).
`crates/test-harness/tests/d0_pid_selector.rs` — the measured two-pocket
case end to end through a save and a reopen, and the untouched-set claim over
an upstream edit. Mutation-checked both ways: withdrawing the reseed reds the
two harness pins and nothing else in that file; ranking by arena id instead
of content reds the three rank-specific pins and nothing else.
`d0_face_seed.rs`'s `a_boolean_under_an_installed_seed_does_not_seed_its_output`
pinned the behaviour this replaces, so it was rewritten rather than removed:
as `…_does_not_role_index_its_output` it pins what still holds — an output
face never takes a role index under a step seed, and the boolean does not
consume one of the step's output ordinals.

**Still open after this increment:**

- *The multi-member rank caveat survives for EDGES.* Item 1b separates the
  split patches of one root as FACES, which is the per-patch discriminator
  the D0 note asked for — but `pid.rs`'s edge derivation is seeded from
  ROOTS, not from face pids, so two edges of two patches of one root still
  share a root pair and still need the positional `rank_groups`. Giving edges
  the benefit means seeding them from the output faces' own pids, which is a
  different (and breaking) change to items 2–3.
- *Nothing has been measured over the assay corpus.* Item 1b changes every
  boolean output face pid VALUE, so the corpus verdicts are the thing that
  would confirm it disturbed nothing — not attempted here (no full tiers or
  assay in this increment's scope). The cross-crate suites that do exercise
  booleans are green: `boolean_chains`, `boolean_determinism`,
  `boolean_combine_custody_kv2`, `face_provenance`, `pattern_kv2`,
  `rebuild_stability`, `incremental_rebuild_kv2`, `multi_body_workflows`.
- *An output face with no attributable operand ancestor keeps a counter id.*
  Yang attributes every patch, so this is normally empty — but it is a real
  branch, and such a face has no content to seed from. The two halves of the
  number space are disjoint, so a mixed solid cannot alias; what a consumer
  gets is an id that is stable only under a full in-order rebuild.
- *`transform_solid` / `mirror_solid` are still counter-allocated.* Their
  output faces are `Same` back to the source, so the same treatment would
  apply, and the pattern features that drive them are exactly where a PMI
  anchor on a patterned body would land.

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
4. **D1d — section. LANDED 2026-10-03.** `section_with_plane` runs the yang
   pipeline with a
   half-space operand built as a box that encloses the solid's AABB with a
   margin, then collects the cap face (the face whose plane equals the cut
   plane, found through `face_provenance` as the only face descended from the
   box operand) and returns its loops. The caller projects the cut solid with
   D1a–c for the section view and hatches the cap loops. The cut plane is
   generic with respect to the solid in the common case; a cut plane coplanar
   with a model face hits the Stage-0 coplanar overlay, which is the correct
   outcome (the section passes through a face) and is handled there. See
   "Implementation notes (D1d)" below: the boolean runs on a SCRATCH arena and
   the result is copied back, the geometry CHECKS the lineage attribution
   rather than merely supplementing it, a cut that keeps nothing has no solid
   to name, and a loop's traversal direction does not survive `Curve2` — so
   the kernel reports the area it measured.

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

**Measured 2026-10-03 (after the review corrections below)**, at the default
stride 8 over the 334-case corpus — 42 cases, **400 s** in `--release`, 264
`(case, body, direction)` views, 3 not built (C0113, P0013, R0007, the assay's
own business), 5 multi-body cases classified per body. **560,960 VISIBLE
samples and 535,455 HIDDEN samples asserted against the depth buffer.** Not
asserted and counted instead: 2,044 samples uncovered, 6,412 with a grazing
occluder, 4,323 on a grazing surface, 395 silhouette curves coverage-checked
only, 0 curves unliftable. (At the increment's landing, before those
corrections: 380 s, 532,786 VISIBLE and 515,907 HIDDEN.)

The kernel's own declines over the same sweep: `split_budget` 0, `cross_body`
0, `depth_unliftable` 0 (all three asserted), `ray_grazes_face` 38,144,
`split_tangency` 1,406, `piece_spans_change` 79, `silhouette_off_face` 12,
`silhouette_non_alternating` 6, `silhouette_grazing_removal` 0,
`silhouette_no_triangles` 0.

Two residues, and they are different in kind. **202 curves over 14 cases SPAN a
visibility change** — the piece covers both states, so the crossing where it
changes was not split; that is the §5.2 split's tail, it is listed per case,
and every mechanism that can lose a crossing is already counted beside it.
(218 over 15 cases before the tangency-split correction below; the kernel now
detects 79 of them from the inside, as `piece_spans_change`, which is the
subset whose change falls in the middle three fifths of its piece — the window
three probe points can bracket.)
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

### D1c review corrections (2026-10-03)

What the code review of the increment changed, in the order the findings were
found rather than by severity.

**A curve whose whole domain is ONE parameter was dropped.** `project_line`
answers `Curve2::Point` for a line running along the line of sight, and a point's
parameter domain is the degenerate `[0, 0]` — which is exactly the shape the
split/classify/merge loop had no window for. `bounds` came out `[t0, t0]`, its
one window was empty, nothing was ever flushed, and the curve vanished. There
is no counter for a DROPPED curve, so neither `ProjectionDeclines` nor the §5.3
oracle (which only judges the curves that came back) could see it; and the two
tests that pinned the four dots of a box's top view had moved to the
unclassified edge pass in the same increment, so it was masked at both doors.
A dot at a box's corner is a drawing — the DXF writer has a `POINT` entity for
it — so a degenerate domain is now classified once at its only parameter and
kept whole. `box_top_view.dxf` regains exactly four `POINT` entities.

**`RECAST_FRACTIONS`' premise is now CHECKED rather than trusted.** The re-cast
rests on "visibility is constant along a piece", which holds only where the
split cut the piece at every crossing and tangency — and the split declines
some of both. Where the premise fails, two points of one piece can sit on
opposite sides of an unsplit change, and nothing detected that: the first
decisive cast returned, so no two points were ever compared. `verdict` now
takes up to three DECISIVE casts and they must agree; a disagreement is
`ProjectionDeclines::piece_spans_change` and the verdict stays the one the
classification decided at. It is never a majority over the probes — voting
would turn a curve known to be half wrong into one confidently claimed whole,
which is the opposite of what a drawing's reader needs. `RECAST_FRACTIONS` is
reordered so the midpoint is followed by the two points FURTHEST from it,
bracketing the middle three fifths of a piece rather than three tenths for the
same number of casts. Measured: the projection test subset costs 8.9–9.9 s
against 6.8–7.5 s at one probe.

**And the new counter found a defect on the plainest fixture there is.**
`segment_conic` counted a tangency and returned WITHOUT splitting when the
quadratic's discriminant came out negative, and pushed the doubled root when it
came out positive. For a true tangency that discriminant is zero up to float
noise, so which happened decided whether the curve was cut. An oblique
cylinder's far rim is tangent to both its silhouette rulings, and its
visibility changes at those two contacts exactly — a point of that rim lies on
the lateral surface, so the ray toward the viewer enters the solid precisely
when the sight direction's radial component there points inward, which is true
on exactly half the circle, whatever the height or the obliquity. The residual
fell positive at one ruling and negative at the other, so only one contact was
split and the rim's hidden arc came back **0.0150 against the exact 0.0228**
along `[2, 3, 5]` — a third of it drawn solid. The fix is the rule these notes
already state: splitting at a true tangency makes two pieces of the same
visibility that the MERGE rejoins, which costs nothing, while not splitting at
a contact that was really a crossing leaves a piece spanning two visibilities.
So a contact within the band is split at the quadratic's own exact minimizer
and the decline is still counted. `conic_conic`'s tangency pass is deliberately
left alone: its contact parameter is known only to the sample spacing, so a
split there would cut in the wrong place. Pinned by
`an_oblique_cylinders_far_rim_is_hidden_over_exactly_half_its_length` over four
generic directions — the existing fixture used the one direction whose two
halves are symmetric, and `a_rims_circle_becomes_an_ellipse_seen_obliquely` had
been asserting through `project` that the far rim is NOT split, which it only
was because of this.

**The ray's measured offset is pinned, not just argued.** It is the one
approximation in the whole verdict and it was prose only. Extracted as
`Occluders::local_gap` and pinned against the closed form for an inscribed
polygon, `R·(1 − cos(π/n))`, at the worst point there is — the exact surface
halfway between two mesh vertices — on two cylinders whose radii differ by
five. That ratio is what says the offset is the SURFACE's number rather than a
constant or the caller's own chord band: a fixed offset would report the same
gap twice.

**The declines travel with the DXF.** They were added to `ViewGeometry` so an
oracle could pin them, and the export door then dropped them:
`export_dxf` returned a bare `String`, so the MCP tool and the app could accept
a drawing with tens of thousands of `ray_grazes_face` with no sign of it.
`KernelProjection::export_dxf_with_declines` is the real door now and
`export_dxf` is the provided method that drops them, so the two cannot drift;
the bridge turns a nonzero count into a named warning on the `warnings` channel
`DxfExportReady` already carries for dropped bodies.

**`KNOWN_DISAGREEMENTS` is a signature, not a case id.** The sweep records
several quite different complaints in one list — a tessellation that failed, a
`project` that refused, a sample in front of the whole solid, and the
classification disagreement itself — and a quarantine keyed to an id excused
all of them, so a pinned case whose classification got FIXED while its
tessellation broke would keep the list satisfied for the wrong reason. Each
entry now names the `ProblemKind`s it covers and why; a pinned case failing a
way its entry does not name fails the sweep like any other, `ProblemKind::Build`
is never excusable, and the stale ratchet runs per KIND as well as per case —
a family a case no longer exhibits must come off the entry. Also:
`VISIBILITY_ORACLE_CASE` naming no corpus case used to sweep nothing, fail
nothing and report green; it now refuses.

**Conic contacts the crossing search had no test for**, each reaching a
different branch of the bracketing: externally tangent circles (the implicit
never changes sign at all, so the contact is only visible as a sample inside
the band — the branch the internal case does not exercise), an osculating
circle and ellipse (tangent AND equal in curvature, so the implicit stays
inside the band over a neighbourhood rather than at one sample, with the
far-apart check that says `COINCIDENT_IMPLICIT` must not swallow them), and
concentric circles of different radii (nothing at all, and specifically not the
coincidence verdict).

**Reported and not fixed.** The `box_oblique_hidden.dxf` golden pins three
coordinates written `-0.000000000`: the `u` of the box's top corner above the
far vertex is exactly zero in exact arithmetic and comes out as a cancellation
residue below 5e-10 mm. `dxf_export::real` normalizes signed ZERO but not a
tiny non-zero, so the golden pins the sign of a ~1e-13 mm residue and will flip
on any reordering of the projection arithmetic, for no geometric reason, and
read as a drawing change.

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

### Implementation notes (D1d)

Landed 2026-10-03, in `kernel_v2::projection::section` with the contract
additions in `waffle_types::kernel::projection`. Where §5.2's increment 4 left
a choice open, this is the choice made and why.

**The cut is the kernel's own Intersect, on a SCRATCH arena, and the result is
copied back.** §5.2's "runs the yang pipeline with a half-space operand" is
taken literally, for Q2's reason (`kernel_v2::interference`): a second
implementation of "which side of this plane is the material on" would be a
second source of truth, free to disagree with the Subtract the user runs
against the same plane a moment later. A section view showing a cap the model
does not have is worse than no section view.

But a boolean in the live arena appends its own entities AND journal entries,
and the box is scaffolding that must not survive the call. So both operands go
into a scratch `BrepArena` the way Q2's does — and then, UNLIKE Q2, the result
is copied back, because a section is not a pure query: the cut body is the
thing the caller projects. The live arena gains one solid and one journal
entry, not a boolean's worth of entities and not the box.

That copy forces one piece of hygiene the plan does not mention and the cap's
attribution depends on. `copy_solid_into` records `(source pid → copy pid)` in
the DESTINATION arena's journal, so a fresh scratch arena's allocator would
hand out numbers equal to the live pids sitting there as sources, and
`journal::face_lineage` — which walks backwards by matching an output pid —
would follow a chain straight through the collision. The scratch arena's
allocator therefore starts at the live arena's `next_pid`, and the live
arena's is advanced past the scratch's BEFORE the copy back: one monotonic
sequence across both arenas, so a lineage walk cannot cross wires. Without it
the cap attribution below would be right by luck. (The order matters as much
as the bump. Copying first would hand out live pids from exactly the range the
scratch arena had been using, and a walk off one copied face would step onto a
scratch pid that also names a live face and keep going.)

**And the cut body's lineage is RE-ROOTED, because the plan's `cut_solid` has
to be anchorable.** The copy writes `(scratch face → live face)`, and that
scratch face is gone when the call returns — so a `face_provenance` on the
section view would answer a root belonging to neither body. Not wrong exactly,
but unusable, and the kind of unusable that looks fine until a D4b annotation
tries to anchor to it. Both journals are in hand and share one pid sequence, so
the composition is available: the scratch journal already carries
`(original face → scratch copy)` from the inbound copy and
`(scratch operand → scratch output)` from the boolean, and walking it lands
either on a face of the SECTIONED body — the real ancestor — or on a face of
the cutting BOX, which has no live ancestor and makes the cap a `generated`
surface. Which is what the cap is. Measured on the `10 × 6 × 4` box cut at mid
height: of the kept half's six faces, five root onto the sectioned body's own
(four walls and the bottom) and one is its own root (the cap), with the cut
body's pids disjoint from the original's and the original's unchanged. The op
tag records the Intersect rather than the copy's `Transform`.

**The geometry CHECKS the lineage; it does not merely supplement it.** §5.2
describes the cap twice over — "the face whose plane equals the cut plane"
and "the only face descended from the box operand" — and both halves are used,
against each other:

- A face whose lineage root is a box face pid must ALSO lie in the cut plane
  with outward normal along the cut normal, or the two disagree and the section
  STOPs (`SectionCapNotOnCutPlane`).
- A face attributed to a box WALL or to the box's far face is the margin
  derivation failing: the box did not enclose the solid. Same loud STOP. This
  is the margin's own test, and it is the reason the margin can be derived
  rather than tuned.

Geometry is also what covers the case lineage CANNOT. A cut plane coplanar with
a model face goes through the §4.5.5 Stage-0 overlay, which replaces the
overlapping region with ONE shared trimmed surface — and that surface may be
attributed to the MODEL operand, not to the box. The cap is then found by its
plane, and `SectionResult::cap_shared_with_model` says so. Measured on a
10-cube with its `x > 4, z > 5` corner removed, cut at `z = 5`: the flag is
set, the cap is the full `10 × 10` square, every curve exact. Reporting it is
the difference between a section known to have gone through Stage 0 and one
that silently came back with no cap at all — which is what a lineage-only
implementation would have produced here.

**The margin is the solid's own AABB and nothing else.** Half-extent
`R + diag` in the plane and depth `−dmin + diag`, with `R = diag/2` the
radius of the AABB about its centre. A half-diagonal bounds the solid's
projection onto the cut plane at ANY plane orientation, so the base rectangle
covers the section whatever the normal, and `−dmin` is the solid's deepest
reach below the plane. No absolute pad: P0012 removed a 1 m pad from a sweep
for exactly this reason — a literal is simultaneously too small for a bridge
and a numerical insult to a bearing.

**Two outcomes are typed answers, not errors, and both are decided on a
CONSERVATIVE bound.** A plane entirely on the kept side returns the input
solid with an empty cap; one entirely on the discarded side returns no solid at
all. `introspect::conservative_aabb` never under-covers, so `dmax ≤ 0` and
`dmin ≥ 0` are proofs rather than tolerance tests — and they also keep a plane
TANGENT to the solid from reaching the boolean as a grazing operand, which
matters because of deviation N69: a box meeting a solid along one edge comes
back from `Intersect` as a bit-for-bit copy of an operand. Whatever the
Intersect does return is checked against the half-space it was asked for
(`SectionCutOutsideHalfSpace`); a copy of the SOLID straddles the plane, so
that net is the one for this class. An `EmptyBooleanResult` — the conservative
box straddled the plane but the solid does not reach across it — is the same
typed "nothing kept" answer. A solid the kernel cannot bound at all (a
surface-pair or hyperbola edge) declines by name rather than being sectioned
with a box that might clip it: a box clipping the solid at its own lateral face
yields a cap that is a SUB-REGION of the true section, which is a silently
wrong drawing.

**Three deviations from §5.1's `SectionResult`, all forced by the types.**

- `cut_solid` is an `Option`. A cut that removes all the material has no solid
  to name and kernel-v2 has no empty solid; a present-but-empty handle would be
  a lie, the argument that already made `ViewGeometry::bbox` an `Option` at
  D1a. In the "nothing was cut" arm the handle that comes back is the INPUT's,
  because nothing was copied.
- The loops are `SectionLoop`s, carrying the signed area the kernel measured.
  A `Curve2` conic is normalized counter-clockwise with `start < end`, so it
  CANNOT represent a clockwise traversal: a hole's loop and an outer loop with
  the same point set are the same curve list. The loop's direction survives
  only in the B-Rep walk, inside the kernel. A consumer asked to tell an outer
  loop from a hole would have to infer it from nesting — at exactly the
  configuration (several outer loops, each holed) where nesting is what it
  wanted to learn. So the area is computed where the direction is still known,
  in closed form by Green's theorem, and reported: positive outer, negative
  hole, with an `exact` flag for whether any curve had to be sampled.
- `plane_basis` is reported. "The cut plane's own `(u, v)`" does not say which
  `(u, v)`, and a consumer free to re-derive a frame is free to rotate the
  hatch against the view. The line of sight is the NEGATED cut normal, so the
  viewer stands on the discarded side and looks at the cap with the kept
  material behind it — the drafting convention, and the frame in which an outer
  loop comes out positive, since `(u, v, n̂)` is then right-handed.

**The cap's curves are exact, and that needed its own conversion.** The cap
lies IN the cut plane, so the map into the plane's frame is an ISOMETRY rather
than a general orthographic projection: a line stays a line, a circle stays a
circle of the same radius, and an ellipse arc stays an ellipse arc with its
semi-axes unchanged. `project_edge` samples `Curve::EllipseArc` into a
polyline, which is the right answer for a general view (D1a sets the analytic
bar at line and circle) and throws away an exactness that is free here — and an
oblique plane cut of a cylinder IS an ellipse, the canonical section a drawing
needs. So the section has a `cap_curve` that handles the in-plane ellipse arm
analytically and delegates the rest.

Measured, on a radius-2 cylinder cut at 45°: four `Curve2::Ellipse` arcs,
semi-axes `2` and `2√2 = r/cos θ` to within 1e-12, parameter spans summing to
exactly one turn, area `π·a·b` to within 1e-12. A `10 × 6 × 4` box cut at mid
height: four exact lines, area exactly `60`. A `10 × 10 × 6` box with a
radius-2 through bore, cut through the bore: two loops, `+100` and `−4π` to
1e-12, net `100 − 4π`. Flipping the normal gives the same cap in the mirrored
frame, same area and sign, with the two halves' volumes summing to the box's.
A `HyperbolaArc` or `SurfacePair` cap edge is still a polyline and its loop
reports `exact = false`, so its area is known to be low by that polyline's
sagitta deficit rather than quietly wrong.

**What §5.3's section oracle can honestly be measured against.** §5.3 asks the
cap area to equal "the area of the stencil-cap polygon the app computes today
for the same plane". That comparison is not available from a Rust test and
should not be the oracle anyway: the stencil cap is a screen-space pass over
the tessellation, in the Svelte/three.js half of the tree, with no numeric area
to read — and the stencil APPROXIMATES the cap, so measuring the kernel against
it is the wrong direction of trust.

`test-harness/tests/section_corpus_oracle.rs` asserts the properties instead,
each independent of what produced the cap: the area is bounded by the AABB's
cross-section (a containment proof, since the solid is inside its own
conservative box — and the check that would have caught the N69 class had the
in-kernel net not caught it first); every loop is closed and crosses no other
curve of its own loop, measured with `section::loop_defects` on D1c's own
crossing search so the oracle and the visibility split cannot disagree about
whether two curves meet; an outer loop is positive, a hole negative, the net
positive; and the cut solid projects, since handing it back is the whole point.
The sharp, closed-form statements live in the per-primitive half above.

Measured 2026-10-03 in `--release` at the default stride 64 (C0001, C0065,
F0011, F0075, R0021, R0085): **6 cases, 305 s / 340 s / 352 s over three
runs** (the spread is the box's other load), 21
`(body, axis)` cuts — 16 capped and asserted, 3 not boundable, 1 pipeline STOP
(C0065 along `z`, the standing torus patch UV-CDT family), 1 empty cap, 1
through the Stage-0 shared cap, 0 sampled loops, 0 cases not built, 0 failures.
The fullest cap fills **1.000000** of its AABB cross-section (C0001 along `x`),
which is what makes the containment bound a check rather than a formality: on a
prismatic body it is TIGHT. The stride default is coarser than the projection
oracle's 8 because the cost differs in kind — that sweep does six projections
per case, this one three real booleans per live body.

**The MCP side.** `section_with_plane` is exposed through `KernelV2Adapter`'s
`KernelProjection` impl, so Q4's `measure_section` and D4b consume it without a
second entry point. `MockKernel` keeps the trait's typed `NotSupported`
defaults: it has no B-Rep to cut, and a trivial cap from a test double would be
indistinguishable from a working section of an empty solid. The adapter samples
a non-analytic cap edge at the RENDER chord band, since the trait method takes
no `ProjectOpts` and a cap is a hatch boundary rather than a dimensioned
outline; a caller needing another density calls the module function.

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

### Implementation notes (D2)

Landed 2026-10-03 (`crates/feature-engine/src/expr/measure.rs`,
`measure.rs`, `params.rs`, `lib.rs`, `drawing.rs`; `crates/wasm-bridge/src/
{dispatch,drawing_view}.rs`). Where the plan above left a choice open, this
is the choice made and why.

**This increment IS P4** of `specs/agent_mechanical_design.md` §6 — "P4 —
measurement functions. D2 as specified" — so that row is closed by this
one, and the MCP spec says so.

**Arguments are N1 entity NAMES, and §6's "`GeomRef`s written in the
existing selector syntax" was not implementable as written.** There is no
textual selector syntax in the tree: `Selector` is a serde-tagged enum
authored as JSON, and nothing anywhere parses one out of a string. The N1
name is what a person or an agent can type into a parameter field, it is
what §6 P4 of the MCP spec asks for in the same words ("arguments accept N1
names"), and a body's display name reaches a solid through the same door.
So `distance(wall_a, wall_b)`, `area(plate.top_face)`, `volume(plate)`.

**Entity names are a separate NAMESPACE, and that is the load-bearing
decision.** A measurement parses to its own AST node (`Expr::Measure`), not
to `Ident`s. `Expr::identifiers()` is the design-parameter dependency list
and `evaluate_parameters`' fixpoint waits on `UnknownIdentifier` coming out
of it — so an entity name collected there would be a name the table can
never resolve and a parameter that never settles. With two node kinds the
namespaces are disjoint by construction: a parameter and a face may share a
spelling, neither shadows the other, and a parameter rename provably cannot
rewrite an entity argument (`collect_reference_spans` does not descend into
a `Measure`). `Expr::entity_references` is the other list, and
`expr::rename_entity_reference` the other rename — a separate function from
P5's `rename_identifier`, for the same reason: renaming the body `plate`
must rewrite `volume(plate)` and must NOT touch a parameter called `plate`.

**D2 reserves no new words.** A measurement name is callable-only, so a
document with a parameter called `radius`, `length`, `area` or `distance`
keeps working: a bare `radius` is that parameter, `radius(rim)` is the
measurement, and the two readings are disjoint so there is no ambiguity to
resolve. Adding the seven to `is_reserved_word` would have invalidated such
a parameter — and every expression reading it — to buy nothing. (The
arithmetic functions ARE reserved; that is pre-D2 behaviour, not a rule
this extends.) Which argument parser runs is decided by the CALLEE, not by
lookahead: every measurement takes only names and every arithmetic function
only numbers.

**One table owns name, arity and DIMENSION** (`expr::measure::
MEASUREMENTS`), and the evaluator attaches the dimension rather than
trusting whoever answered — so no measurer can report an area as a length.
Lengths are mm, areas mm², volumes mm³, angles degrees: the evaluator's own
working space (P1), converted from the kernel's metres once, next to each
kernel call. A measurement COMMITS its dimension, unlike a bare literal,
which is what makes `distance(a, b) / 2` a length a depth accepts,
`sqrt(area(top))` a length, and `area(top)` a length² a depth refuses by
name. Names, arities and the dotted-path grammar (`leaf` or `body.leaf`,
`names::MAX_SEGMENT_LEN`) are all validated at PARSE time, as P1
established.

**`mass` is in the grammar and refuses, naming M1.** `Dim` carries length
and angle exponents only, and the document model has no material table to
read a density from, so there is no number it could return that this
evaluator could carry honestly. It parses — name and arity checked, so the
spelling cannot drift — and evaluation is a typed
`MeasurementUnavailable`. Widening `Dim` now would add a serialized
`Dimension` variant, and with it a reader-floor obligation (the D0/N1
lesson), for a function that still could not answer. `volume(body)` is the
one that works today.

**The ordering rule is what makes a cycle a typed error rather than a
hang.** A measurement may read only geometry EARLIER in the tree than the
expression it drives. `crates/feature-engine/src/measure.rs` enforces it
ordinally, before any number is computed: §6's cycle ("a feature whose own
dimension reads its own output") is the rule violated with
`owner == self`, and reading a LATER feature is the same violation —
both would make the rebuild's answer depend on the order it happened to
compute things in. A name's owning feature is the LATER of its reference's
ANCHOR feature and its pid's lineage root (`Engine::pid_to_feature`):
availability, not provenance, is what a rebuild has to wait for, and for a
boolean-output face those two answers differ. A design parameter has no
index of its own, so the rule is stated at its EARLIEST READER
(`params::earliest_readers`, propagated through parameter→parameter edges):
a parameter read by feature #3 may measure #0–#2 and nothing later.

**A rebuild that measures runs more than one pass, and the rule is why it
terminates.** `Engine::rebuild_once` is the old body, which never measures;
`Engine::rebuild` is the loop — build, measure, and if a measured value
moved a field, build again from that field's feature. Because a
measurement reads only earlier geometry, each pass settles the lowest
unsettled site and never disturbs an earlier one: the sites settle in index
order, each once, in at most (sites + 1) passes. That bound is the budget,
and exhausting it is a typed "did not settle" error rather than a loop —
the backstop for the one case the ordinal rule cannot see, a name whose
owner the kernel cannot attribute (a mesh-backed import), which also
warns. A document with no measurement runs exactly one pass and pays
nothing.

**A pass with no model DEFERS, silently.** The pre-rebuild pass has no
geometry, so every measurement there is skipped: no error, nothing
written, the field keeps the value the last measuring pass gave it, and a
measuring parameter enters the environment at its cached value so its
dependents still resolve. Reporting "no geometry is available here" would
put a permanent error on a perfectly correct document on every rebuild.
The measuring pass is also the ONE reporter of expression errors when it
runs (`rebuild_once(…, report_expressions: false)`): both reporting would
duplicate every non-measurement expression error.

**A body rename carries the expressions that measure it.** A body's display
name IS a measurement argument, so `rename_body` rewrites `volume(plate)`
through the AST's byte spans and the undo record carries both halves —
restoring the name alone would leave every expression reading the other
one. No rebuild: the rewritten expression denotes the same entity, so every
measured value is unchanged by construction. Clearing an override leaves
nothing to spell, so the expressions then refuse loudly, which is the
honest outcome. There is no `entity_rename` tool for a non-body entity name
yet (N1 gives assign and unname); `params::rename_entity` is the mechanism
waiting for one.

**Resolution is `names::resolve`, Strict, not the stored reference.** The
same ladder `names_list` reports — the pid first, the authored fallback
when the pid is gone — so a name the listing calls resolvable is a name an
expression can measure, and one question is not answered two ways. A
vanished identity is `ExprError::MeasurementFailed`, naming the function
AND the name.

**`area` and `radius` are introspection, not `KernelMeasure`.** `area`
reads the face signature's own exact area (N0 fills it for every analytic
surface) and `radius` the axis descriptor's radius (N0, Q6); `angle` is
computed from normals and axis directions here, exactly as §4.1 of the MCP
spec says it must be ("angles are not a kernel method"). `distance`,
`length` and `volume` go to `KernelMeasure` (Q1, Q6, Q3). A face whose
normal is `None` — a full-turn surface of revolution — is refused for
`angle` rather than handed its axis, because those are different
quantities.

**D3's `Measured::Expr` is evaluated through this path.** `check_measured`
no longer refuses it (it is a legal authored value now), and `rebuild_view`
evaluates it through a new `ExprDimensions` trait — a trait rather than the
environment itself, because the environment is the parameter table AND the
live kernel while `rebuild_view` is deliberately pure with respect to the
document. The drawing tab supplies one built from the SOURCE tab's own
engine (`ViewExprs`): the view draws that document, so it must measure that
document. `None` still refuses by name, which is what an assembly source
gets, having no single engine. The value is evaluated BEFORE the anchors
are resolved — anchors-first hides a broken expression behind a missing
anchor whenever both are wrong, and only one error per annotation is
reported either way.

**`expression_evaluate` measures the live model**, through the same
measurer, so a dialog's preview and the rebuilt geometry cannot disagree —
the property P1 and P5 both rest on. It sets no ordering floor: a preview
drives no field, so it has no position and nothing to be circular with
respect to.

**The browser needed no change.** `isPlainMeasurement` requires a leading
number, so `distance(a, b) / 2` already routed to the engine like every
other expression. Nothing client-side evaluates anything.

**Oracles.** `crates/feature-engine/tests/measurement.rs` (MockKernel: the
ordering rule both ways, a vanished name, a kernel that cannot measure,
`sqrt(area(…))` driving a depth to the measured value, the body rename and
its undo/redo) and `crates/wasm-bridge/tests/measurement_expr.rs`
(kernel-v2: a boss whose depth is `distance(wall_a, wall_b) / 2` is half
the measured gap and a wider gap is a deeper boss; the same through a
measuring parameter, whose `parameters_get` row reports the measured mm
with `committed: true` and an EMPTY `depends_on`; 100 mm², a 10 mm rim,
1000 mm³; `mass` refusing; the cycle with real pids). `params.rs`'s
`every_expression_field_can_measure_and_is_enumerated_as_one` is D2's half
of the `expression_sites` drift oracle: all fifteen fields plus the
parameters, and zero errors from a deferred pass.

Still open:

- *`mass` waits for M1*, with the `Dim` widening and the reader-floor
  question it brings.
- *A measurement in an ASSEMBLY tab measures nothing.* `TreeMeasurer` is
  built per part engine, and an assembly's instance-scoped references are
  the same gap Q2 and Q6 have (`RefScope.instance_path` + a world-space
  transform step). A drawing view sourced from an assembly therefore
  refuses an expression dimension by name rather than measuring one leaf.
- *An `along` distance and `thickness` are not exposed.* `DistanceOpts.
  along` exists on the kernel contract and Q5 is unlanded; the grammar has
  no place to put an option, and inventing a keyword argument for one
  function is worse than waiting for the second customer.
- *A measuring expression re-measures on every rebuild, which is a kernel
  call per site per pass.* No caching: a cached measurement that goes stale
  is exactly the silent-wrong-number this increment exists to remove, and
  the cost is one BVH query on a document that has any measurement at all.
- *No `entity_rename` tool*, so a non-body entity name cannot be changed in
  one step; the rewrite mechanism is in place for when one lands.
- *The settle budget's error names the features that were still moving, not
  the cycle.* The ordinal rule names the loop precisely for every case it
  can see; the budget is the backstop for the unattributable ones, where
  there is no owning feature to name.

## 7. D3 — Annotation model (LANDED 2026-10-03)

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

### Implementation notes (D3)

Landed 2026-10-03. Where the plan above left a choice open, this is the choice
made and why.

**The model is split in three, and the split is the mechanism that makes a
dimension measured.** §7 sketches one `Annotation` enum. What landed is that
enum plus two companions, in `crates/waffle-types/src/annotation/`:

| module | holds | written by |
|---|---|---|
| `annotation` | the document model — which entities, how to measure them | the UI / MCP |
| `annotation::measure` | the value, from resolved anchor geometry | the rebuild |
| `annotation::layout` | the record the renderers consume | the rebuild |

`ViewLayout` is §7's "same layout record the engine emits" and §3's
`AnnotationLayout (JSON)` edge made concrete. It carries the projected curves
and the resolved annotations with every `GeomRef` already turned into
geometry and every `Measured` already turned into a number — so a renderer
holding one has **no path back to the model and therefore no way to draw a
value other than the measured one**. That is asserted structurally, on the
schema's `$defs` and `$ref` closure rather than on one instance
(`the_layout_schema_carries_no_geom_ref`).

**`Measured` has a third arm and it is the default.** §7's two —
`Expr(String)` and `Value(f64)` — leave the ordinary case ("this dimension is
however wide the part is") expressible only as a synthesized expression
string naming anchors the annotation already holds, or as `Value`, which is
exactly the typed-in number the spec forbids. `Measured::FromGeometry` names
it directly. `Expr` stays for a *derived* value (D2's measurement functions, a
title-block `mass(part)`); `Value` is documented as a cache or an imported
nominal and is explicitly not offered by a UI. Also, both are struct
variants, not newtypes: serde's internally-tagged representation — which
every persisted enum in this tree uses — cannot serialize a newtype variant
wrapping a primitive, since there is nowhere to put the tag.

**`measure` refuses rather than guessing, in four places.** Two non-parallel
lines have no single distance (`AnchorsNotParallel`, reporting the angle); a
sampled polyline has no witness point, because its midpoint moves with the
chord tolerance that sampled it, so a dimension on one would read a different
number at a different render density; only a conic has a radius; a zero-length
line or zero radius is `Degenerate`, not a zero dimension. Parallelism is
tested on `|sin θ| ≤ 1e-7` — a *dimensionless* tolerance, so it does not
scale with the lines' length or separation the way `TAU_MODEL` would, pinned
by `parallelism_is_judged_on_the_angle_so_it_does_not_depend_on_the_lines_length`
at 1 mm and 1 m.

**An angular dimension reports the ACUTE angle, and cannot do better.** A
projected curve is an undirected point set by `projection::Curve2`'s own
contract ("traversal direction is deliberately not preserved"), so an edge's
direction is known only up to sign and the obtuse supplement is not
distinguishable from the acute one. Inventing one would mean picking a sign
the projection does not carry.

**`LayoutCurve` mirrors `Curve2` rather than reusing it, because
`cad_primitives::Point2` derives no serde.** Adding serde there means editing
a crate two layers down the stack for a consumer two layers up, so instead
there is a serde-able twin over `[f64; 2]` with one conversion
(`LayoutCurve::from_curve2`) and a test pinning it arm-for-arm. `Visibility`
and `CurveKind` DID gain serde — they are C-like enums with nothing inside —
so the tags cross directly. If `Point2` ever gains serde, `LayoutCurve`
collapses to an alias and the conversion becomes the identity.

**`Annotation` is not `PartialEq`.** `GeomRef` is not, and making it so means
deriving `PartialEq` down through `TopoSignature` and `TopoQuery` — reference
types four crates share — for a convenience here. Annotations are compared by
their serialized form, which is also the form that gets persisted and the
only one an equality would have to agree with.

**No `tolerance` field and no `FeatureControlFrame` variant.** Both need M1's
`Tolerance` / `GeometricTolerance`, which this increment does not invent.
Adding the field later is additive (`#[serde(default)]`); adding the variant
is not.

**Nothing persists yet, so nothing bumped.** `Annotation` is unreachable from
a `.waffle` file today — it becomes reachable when D4a adds
`TabKind::Drawing` and M2 adds the `Pmi` feature. Measured against
`docs/FILE_FORMAT.md` §13 rule 3: no new variant reaches a persisted type, no
reader can encounter one, so the reader floor does not move and
`docs/schema/waffle-v5.schema.json` is byte-unchanged (its golden is green).
**That was expected to be the bump D4a owes**, for the §13 reason v7 did: the
annotations it persists are serde-tagged enums whose anchors carry a
`Selector::Pid`. *It turned out not to be* — see the D4a notes below: inside
an unknown tab kind nothing is deserialized, so an old reader never meets the
variant, and bumping would make it reject the whole document instead of
keeping the drawing opaque. The shapes are pinned now anyway —
`docs/schema/annotation.schema.json` and
`docs/schema/annotation-layout.schema.json`, regenerated with
`UPDATE_SCHEMA=1` — because the window between D3 and D4a is exactly when an
accidental change is cheapest to make and hardest to notice.

**The renderer is `app/src/lib/drawings/`: four modules and a thin
component.** `style.js` holds the ISO 128-20 / ISO 129-1 / ISO 3098 defaults
with their citations and the `drawingStyle(overrides)` document-settings seam;
`format.js` turns a measured number into dimension text; `layout.js` is pure
2-D geometry producing drawing primitives; `svg.js` emits the markup.
`DrawingView.svelte` is one `{@html}` of `svg.js`'s string — deliberately not
a declarative `{#each}` renderer, which would be a second source of truth for
the same geometry. D4a's `DrawingSheet.svelte` composes it.

**One SVG user unit is one paper millimetre, and the standards' numbers reach
the output unscaled.** §7 says the defaults are "scaled by the view scale",
which is true of the drawing and false of the pen: a 0.5 mm outline must
print 0.5 mm wide at 1:1 and at 1:10, or a scaled-down view comes out with
hairlines. So `layout.js` converts view-space meters to paper mm exactly once
(`paperTransform`) and works in paper mm thereafter, and the `viewBox` is in
paper mm with `width`/`height` in `mm` so the browser's print path is true to
scale. The v-flip (view `v` up, SVG `y` down) is applied to coordinates, not
as a `scale(1, -1)` transform, which would mirror the text.

**Where the dimension line goes, with no authoring.** It sits
`style.dimensionOffset` clear of the whole VIEW BOX, on the side away from
the view's centre, plus the annotation's `placement`. Clearing the two
witness points is not enough and was the first version's bug: a witness point
is a wall's midpoint, not the part's extreme, so two opposite walls
dimensioned for width put the line 2.5 mm *inside* a 40 × 25 mm plate —
arrowheads, extension lines and all. The view's own bbox is the only thing
that knows where the part ends. `placement` moves the whole dimension rather
than only its label, matching how dragging a sketch dimension behaves and
letting a drafter push one to the other side.

**A radial arrowhead lands on the drawn rim, not at the printed radius.**
For a circle those are the same point. For a hole seen obliquely they are
not: the value is the ellipse's major radius (the hole's true radius) while
the rim along the leader is `1/√((α/a)² + (β/b)²)`, up to `major − minor`
nearer — 1.5 mm on a Ø16 rim at 45°. `layout.js` solves the reach on the
ellipse, so the arrow touches the curve it points at and the text still reads
the true size.

**The renderer never invents a number.** A non-finite value prints an em dash
(`measure` refuses one, so a record carrying one was built by something that
did not — and `NaN` where a machinist reads a size is the worst possible
output); `-0.00` is normalized to `0.00`; an annotation kind this build does
not know draws nothing and reports the omission in `warnings`, because a
placeholder glyph on a manufacturing drawing is worse than a visible absence.

**Colours are four CSS variables declared once.** `--drawing-paper`,
`--drawing-ink`, `--drawing-hidden` and `--drawing-annotation` are defined in
`app/src/app.css`'s bare `:root` *in terms of the theme's own* tokens. A
custom property is substituted where it is used, not where it is declared, so
all nine existing themes and every future one get a drawing palette without a
drawing block of their own.

**What the two test suites each own.** The measurement is Rust's:
`annotation::measure`'s unit tests for the rules, and
`crates/test-harness/tests/d3_annotation_measure.rs` for the whole path —
build a plate, project it top-down through D1, map each projected curve back
to its edge's `EntityPid`, resolve the annotation's `Selector::Pid` anchors
and measure. The plate's sides come back at the authored 40 mm and 25 mm, a
cylinder rim at its radius, two adjacent walls at 90°, and an anchor whose
pid is gone refuses *by name* rather than reading a plausible 25 mm off a
neighbour. `app/tests/gui/drawing-dimension-svg.spec.js` owns the half Rust
cannot reach — that the renderer PRINTS that value at the stated precision —
and asserts on the SVG DOM, never on pixels. The assertion that keeps the two
honest is that the drawn dimension line is as long as the printed number
times the view scale; a 2 % error injected into the paper transform reddens
it and leaves the text assertions green.

A finding worth recording from writing that harness: **which world axis a
named view puts on `u` is the view basis's choice, not something a test may
assume.** `ViewFrame::looking_along([0, 0, -1])` derives its own `up`, and for
the top view it lands the sketch's +y on `u`. The fixture pins its sketch +x
to world +x (`rect_sketch_oriented` — `rect_sketch` takes the derived basis)
and `expected_extent` derives the expected number with `basis.project_dir`,
so the test predicts rather than records.

**Still open after this increment:**

- *Nothing emits a `ViewLayout` yet.* `ViewLayout::from_view` and the
  measurement are built and tested, but the rebuild that resolves anchors
  through `resolve_geom_ref_live`, measures, and caches the record is D4a's.
  Until then the renderer's door is `window.__waffle.renderDrawingSvg(input)`
  — a pure function, exposed for tests and the console, reading no store
  state.
- *`Measured::Expr` is not evaluated anywhere.* It needs D2's measurement
  functions in the expression environment; the arm exists so D4a compiles
  against the finished shape.
- *A small circle's radial dimension stays inside.* ISO 129-1 puts the
  dimension outside the circle with a leader when the text will not fit
  between the arrowheads. The layout always draws the leader form for a
  radius and the through-centre form for a diameter, which is wrong for a
  large circle (the radius should go inside) and cramped for a very small
  one. Deciding needs a text-width measurement, and §3 forbids text metrics
  in Rust — so it belongs in `layout.js`, with the measurement taken from the
  style's text height rather than from the DOM, to keep the function pure.
- *No tolerance, precision or dual-unit document SETTING exists.* The
  renderer takes `documentPrecision` and `unit` as arguments and
  `drawingStyle(overrides)` takes the rest; wiring them to real document
  settings is M1's, which is also where the `units.js` formatter gains
  fractional inches.
- *An ordinate dimension has no ordinate ORIGIN.* `DimensionKind::Ordinate`
  reads one raw view-plane coordinate, so its number is measured from the
  view frame's origin — which is a property of the projection, not of the
  part, and is not the corner the sheet is laid out from (`svg.js` puts paper
  `(0, 0)` at the bbox's top-left). The printed value therefore cannot be
  read off the sheet, and moving the part in space changes it. An
  `origin: GeomRef` (a datum vertex or edge) on the `Ordinate` variant is the
  fix, and it is additive; it belongs with D4a, which is where a view frame
  first becomes a document object.
- *Nothing refuses `Measured::Value`.* It is documented as not authorable and
  nothing in the tree constructs it (only `FromGeometry` is), but there is no
  boundary that rejects one either — no MCP tool, no deserialization guard.
  D4a and M2 own that refusal, at the same seam where they first make an
  `Annotation` reachable from a file.
- *A dual dimension's two units share one precision.* Two places of
  millimetres is 0.01 mm; two places of inches is 0.254 mm, so the bracketed
  value is 25× coarser than the primary it is supposed to restate. ASME
  Y14.5 §1.6.2 wants the conversion to preserve the implied precision. The
  rule is stated and pinned in `format.js`; a separate dual precision is
  M1's, with the rest of the document settings.
- *`Placement2` is in view-space meters.* For a label nudge, paper
  millimetres would be the natural unit, and a label dragged on a 1:10 view
  would then move the same distance on paper at any scale. It is meters here
  to match every other coordinate in the record; revisit when the UI actually
  drags one.
- *The PMI overlay (M2) reuses `layout.js` but not `svg.js`.* The primitives
  are emitter-agnostic by design; the three.js side of that is unwritten.

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

### Implementation notes (D4a)

Landed 2026-10-03. Where §8 left a choice open, this is the choice made and
why. D4b — section and detail views, the title block, the sheet PDF — is
untouched.

**The model is `feature_engine::drawing`, beside `assembly` and for the same
reason.** `file-format`'s `TabKind` holds it, so it has to live below
file-format and above `waffle-types`, which owns the annotations and the
projection contract. Five deviations from §8's sketch:

| §8 | what landed | why |
|---|---|---|
| `annotations` on the tab | on the **view** | a dimension is measured in view-plane `(u, v)`; an annotation with no view has no coordinate system. A tab-level list needs a view id per entry anyway, and then "an entry naming a deleted view" is a state the type permits |
| `sheet: Sheet` | `sheets: Vec<Sheet>` | a part with six views and a detail sheet is the ordinary case; one sheet per tab would split a title block from the views it describes |
| `source: GeomRef` | `ViewSource { tab_id, bodies }` | a `GeomRef` names ONE entity (it has a `TopoKind` and a selector); a view projects a SET of bodies, which is exactly what `project_bodies` takes |
| `cache: ViewGeometry` + `preview: SheetPreview` | `cache: ViewLayout`, no separate preview | `ViewGeometry` has no serde (`cad_primitives::Point2` derives none); `ViewLayout` is its persistable form AND carries the annotations, so one field does both jobs |
| `ViewStyle` with `tangent_edges` | `{ hidden_lines, silhouettes }` | `CurveKind` is `Edge \| Silhouette \| SectionOutline` — there is no tangent-edge classification to switch, and a checkbox wired to nothing is worse than a missing one |

**`TabKind::Drawing` did NOT bump the format version, and that is a
decision.** §13.3 and `MIN_READER_VERSION`'s own doc comment: since v4 a new
tab kind needs no bump, because a reader that does not know the tag keeps the
whole tab as `TabKind::Unknown` and re-emits it verbatim. The D3 notes above
expected D4a to owe a bump anyway, on the v7 precedent — a drawing's
annotation anchors persist a `Selector::Pid`, and a new selector variant IS a
floor bump. **The difference is where the variant sits.** v7's was inside
`FeatureTree.names`, a defaulted field of a kind every reader knows, so an old
reader deserialized it and failed on the unknown variant; inside an unknown
tab kind nothing is deserialized at all.

And bumping anyway would be actively **worse** than not. A reader refuses a
file whose `max(version, min_reader_version)` exceeds its own
`FORMAT_VERSION`, so a bump makes every older build reject the WHOLE document
— losing the part tabs it reads perfectly well — where today it opens the
document and keeps the drawing opaque. That argument lives in
`format_tests.rs::a_drawing_tab_did_not_move_the_format_floor`, which checks
that a document WITH a drawing tab claims exactly what one without claims —
written against the CONSTANTS rather than a literal, because the claim is
"a drawing tab moves nothing", whatever the floor is, and a bump should have
to be deliberate in one place rather than in every test that mentions a
version. (It was 8 — P1's `DesignParameter.unit` — when this landed.) Two
existing tests used
`Drawing` as their stand-in for an unimplemented kind; both moved to
`Schematic`, since the mechanism under test is the opaque branch and not the
name.

**First angle is not "third angle with `dir` negated".** It is the
third-angle frame of the **opposite** placement, which is what the standard
says: the view placed on one side shows the side opposite. Negating only `dir`
and keeping `up` gave a first-angle `Up` view with paper up `+w` — a bottom
view mirrored horizontally against the parent it sits above, the classic wrong
bottom view reached from the other direction. For the same reason
`NamedView::Bottom` has paper up `−y`, not `+y`: every view in a projection
group must share a paper axis with the front view, pinned by
`every_named_view_shares_a_paper_axis_with_the_front_view_it_is_grouped_with`.
`projected_frame` is one four-row table read forwards or backwards.

**A plan view of a box draws four lines, not four plus four hidden.** The
obvious expectation is wrong and D1c is right. The far face's edges are
coincident in `(u, v)` with the near ones, and the ray from them leaves
through the near face's own BOUNDARY — it grazes, and "a face the ray merely
grazes separates nothing" (`ProjectionDeclines::ray_grazes_face`). So they are
visible, and the coincident-and-same-visibility merge leaves one line each,
which is also what a drafter draws. The test asserts the four AND asserts
`ray_grazes_face > 0`, so the merge is the decided outcome rather than a depth
test that quietly found nothing. The count that exercises occlusion is the
isometric view's **9 visible / 3 hidden**.

**An annotation's failure is not its view's.** `rebuild_view` first returned
`Err` for an unresolvable anchor, which blanked the whole view — a sheet of
eight views losing one entirely, curves and all, because one dimension's
entity was gone. It now reports per annotation
(`ViewRebuild::annotation_errors`) and the view still draws. The exception is
`drawing_annotation_add`, which rolls back the annotation IT just added: that
one never worked, so there is nothing to preserve, and leaving it would be a
drawing carrying a dimension that draws nothing.

**`Measured::Value` is refused in two places, and `Measured::Expr` in the
same breath.** §7 left "nothing refuses a literal" open and named D4a as its
owner. `feature_engine::drawing::check_measured` refuses both at the engine
boundary with the index of the annotation that carries them, and the MCP tool
has no `value` argument at all, so a literal is not reachable from the
authoring door. `Expr` is refused rather than measured from geometry instead —
that would print a different number from the one authored. It becomes
evaluable with D2.

**Anchors are offered beside the layout, never inside it.** The layout record
carries no model reference, which is what makes a renderer holding one unable
to draw a value other than the measured one (asserted on the schema's `$ref`
closure, D3). But picking an edge on a sheet to dimension it needs the edge's
id, so the rebuild also produces `Vec<ViewAnchor>` per view — pid, kind,
witness point, radius — which rides on `ModelUpdated.drawing.anchors` and, for
an agent, on `drawing_view_add`/`_edit` with `include_anchors: true`. A pid
naming more than one drawn curve is left out: an annotation must not be
offered an anchor that would then refuse as ambiguous. A count comes back by
default and the list only on request, because a real part's view has thousands
of edges.

**`export_svg` is the app's tool, not wasm-bridge's.** §8 says both exports
are wasm-bridge tools and the DXF one is. The SVG one cannot be, by §3's own
rule — "Rust produces curves and numbers; the app draws them". The sheet's
markup comes from `app/src/lib/drawings/sheet.js` over D3's `svg.js`, and a
second renderer in Rust would be a second source of truth for the same
geometry: exactly what `DrawingView.svelte` refuses for a single view.
Exporting from the page makes the file byte-identical to what the sheet shows,
which a Rust writer could only approximate. It keeps the export door's own
shape (`deliver`, `file_name`, `mime_type`, `bytes`, `warnings`) and delivers
a download the same way, so an agent cannot tell the two apart — and it is in
neither engine routing table, which is why the READ_ONLY pin did not move.

**The sheet is composed by nested `<svg>`, one per view.** Each view is
rendered by `renderViewSvg` exactly as it is on its own — same function, same
bytes — and the nesting only positions it. `renderViewSvg` gained one
additive argument, `paper: false`, so the sheet paints one piece of paper
rather than a rectangle per view (which reads as a stack of cards). The
placement flip happens once, in one expression: `placement_mm` is measured up
from the sheet's bottom-left (the drafting convention) and SVG measures down
from the top-left, and a `scale(1, -1)` transform would mirror every label.

**The sheet DXF composes in paper METERS.** `kernel_v2::dxf_export::write_dxf`
converts meters to millimetres itself, so handing it millimetres would write a
sheet a thousand times too large. `Curve2::transformed` and
`ViewGeometry::transformed` (new in waffle-types) do the placement as a
similarity, which is what keeps a circle a circle and an ellipse's axis
direction — so the analytic DXF entities survive placement instead of being
flattened on the way to the sheet. A non-positive or non-finite ratio is
refused rather than normalized: a negative scale mirrors the view, and a
mirrored manufacturing drawing is a part machined the wrong way round. One
view alone exports at the paper ORIGIN, which is what a cutting table wants
from a sheet it should not read the rest of. `export_dxf` refuses each
shape's arguments on the other — `direction`/`up` name a projection a sheet
does not have, `sheet_id`/`view_id` name views a Part tab does not have.

**A view of no bodies is not asked of the kernel.** Nothing projects to
nothing, so the only thing the call can add is a `NotSupported` from a kernel
that cannot project — which says nothing about this view, and would report a
freshly added view of an unbuilt tab as a projection failure, hiding the real
ones.

**A drawing of an Assembly tab reuses `assembly_view::evaluate`.** The
instance poses are a solve, not a field, so re-deriving them would be the
next thing to disagree with the assembly tab beside it. The whole drawing
evaluation goes through the same part-engine pool as an assembly's, so
switching to a drawing and back does not rebuild a part that did not change.

**Still open after this increment:**

- *No `drawing_get`.* §8 names three tools and they are all mutating; an
  agent reads the drawing from any edit's answer, and the app reads it from
  `ModelUpdated.drawing`. An agent that OPENS a document with a drawing tab
  has no read-only way to list its views — `tab_switch` answers with the
  document, not the drawing. A `drawing_get`, on `assembly_get`'s terms, is
  the fix.
- *No annotation UI.* The engine, the tools, the store door
  (`addDrawingAnnotation`) and the renderer are all wired, and the sheet draws
  what is in the document — but nothing in the panel adds a dimension, and
  nothing on the sheet is clickable. Picking an edge needs a hit test against
  the anchors' witness points, which is the next increment's natural start.
- *`Ordinate` is still not authorable.* §7's open item stands: it reads one
  raw view-plane coordinate measured from the view FRAME's origin, so its
  printed value cannot be read off the sheet. `DIMENSION_TAGS` leaves it out
  deliberately rather than offering a dimension whose number moves when the
  part moves in space. The `origin: GeomRef` fix is additive and now has a
  view frame to be relative to.
- *A sheet has no title block and no second sheet in the UI.* The model holds
  `sheets: Vec<Sheet>` and the renderer takes one; `DrawingPanel` shows the
  first. Choosing between sheets is D4b's, with the title block.
- *The read-only viewer route still shows the 3D viewport on a drawing tab.*
  `app/routes/view/+page.svelte` branches on `Assembly` only, so a shared
  document opened at a Drawing tab shows an empty viewport rather than the
  sheet. One branch, deliberately left to keep this increment's app surface
  to the editing route.
- *Deleting a view deletes the views projected FROM it.* That is what
  deleting a parent means, but it is silent: the panel does not say how many
  go with it.
- *A view's `cache` is persisted, so a `.waffle` with a drawing is larger by
  its curve lists.* Deliberate (a reader with no kernel can draw the sheet)
  and bounded by the drawn curves, but there is no document setting to turn
  it off, and a six-view drawing of a gear would be substantial.

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
| D2 | measurement functions in expressions | D0 | feature-engine — **LANDED 2026-10-03** |
| D3 | `Annotation` types + SVG dimension renderer | D0 | waffle-types, app — **LANDED 2026-10-03** |
| D4a | `Drawing` tab kind, named + projected views, DXF/SVG export | D1c, D3 | file-format, feature-engine, app, wasm-bridge — **LANDED 2026-10-03** |
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
