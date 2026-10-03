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

Status: **D1a and D1b landed 2026-10-03, with the one-view DXF export of
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
R12 DXF in millimetres. Since D1b (also 2026-10-03) it carries the curved
faces' silhouettes too, so a flat pattern of a curved part has its outline;
it is still a view with no HIDDEN-line removal until D1c, and the tool's own
description says so, because a caller who is not told would ship a drawing
with the far edges in it and never know.

## 13. What this is not

- Not a sketch replacement. Drawings never drive geometry; the sketch solver
  stays the only place a dimension changes a part.
- Not a renderer in Rust. The kernel emits curves; text, arrows and line
  weights are the app's.
- Not a PMI importer. Reading PMI from STEP is a separate spec once M3's
  writer and oracle exist.
- Not a fillet, chamfer or shell dependency. Tangent-edge display (`ViewStyle`)
  handles the case where those arrive later.
