# M8-mixed — Stage-0 overlay admission for mixed Line+Arc planar faces

**Milestone tag:** `M8-mixed` (grep tag for quarantines/ignores)
**Status:** spec (FIP Phase 1)
**Owner crate:** `crates/yang-rs` (Stage 0), one guard in `collect_edge_splits`
**Assay targets:** R0021 R0026 R0051 R0059 F0075 (probe class `face-unsupported`,
the largest of the five 2026-07-09 M8 residue mechanisms; census in session log)

## 1. Goal

A planar face whose loops mix `Curve::LineSegment` and `Curve::Circle` /
`Curve::Ellipse` edges (a chained boolean's cap face: rectangle-with-arc-notch,
disc-with-polygonal-hole, multi-arc outer loop) currently walls Stage-0 §4.5.5
coplanar preprocessing at `overlay_face_supported` (probe `face-unsupported`),
even though Stage 1 already tessellates such faces
(`tessellate_planar_curved_cdt_face` over shared per-edge sample chains).

After this increment, a near-coplanar pair involving such a **mixed face** is
admitted into the general overlay whenever the overlap boundary does not
subdivide any *curved* sub-chord of the mixed face. The paper requires no shape
distinction at all (Yang 2025 §4.5.5, Fig. 16 — one general 2D Boolean before
discretization); this lifts the disc/annular/all-segment lattice one step
toward that general form while keeping the not-yet-buildable sub-case (arc
subdivision → lateral chain propagation) a **loud typed wall**.

## 2. Parameters

No user-facing parameters. Internal inputs:

- The pair's two faces (any combination of all-segment / disc / annular /
  mixed planar faces).
- `Stage1Tess::chains` (new field): the per-curved-edge sample chains the
  Stage-1 tessellation already builds internally (`rim_rings` map in
  `stage1_tessellate_inner`) — full-circle/full-ellipse closed rings and open
  arc chains, keyed by B-Rep edge index.

**Definition — mixed face:** `Surface::Plane`, NOT a disc
(`disc_circle_edge`), NOT annular (`annular_disc_face`), every loop edge's
curve ∈ {`LineSegment`, `Circle`, `Ellipse`}, and at least one loop edge
curved. (A face with any other curve type stays `face-unsupported`,
unchanged.)

## 3. Branch table

| # | Face classes in pair | Overlap boundary vs mixed face's curved chords | Behavior |
|---|---|---|---|
| 1 | mixed × all-segment | no curved sub-chord subdivided | HANDLED — general overlay (same machinery as annular) |
| 2 | mixed × disc / annular / mixed | no curved sub-chord subdivided | HANDLED — general overlay |
| 3 | mixed × any | a curved sub-chord of the mixed face strictly contains an overlay vertex | LOUD wall `CoplanarFacesUnsupported`, probe `mixed-curved-chord-subdivided` |
| 4 | mixed × disc | (any) | `build_disc_pair` fast path **excluded** — routed to general overlay (see §6 hazard) |
| 5 | disc / annular / all-segment only (no mixed) | — | byte-identical to today (all existing paths untouched) |
| 6 | planar face with any other curve (SurfacePair, …) | — | `face-unsupported` wall, unchanged |

Straight-edge subdivision of a mixed face is NOT a wall — it flows through
`collect_edge_splits` exactly like an all-segment face (branch 1/2 covers it).

## 4. Invariants

- **I1 (conformality):** every curved-chain sample vertex in the mixed face's
  override triangles is bit-identical to the corresponding vertex of the
  face's own Stage-1 tessellation (same `stage1_tessellate_min_segments` call
  shape, same `forced_rim_n`), hence bit-shared with the adjacent curved
  lateral. No T-junctions on arc chains.
- **I2 (loud sub-case):** an overlay vertex strictly interior to a curved
  sub-chord (exact rational collinearity + interior parameter, the
  `rim_subdivided` predicate) ⇒ typed `CoplanarFacesUnsupported`. Never a raw
  chord-position lift into the output (the mint machinery `rim_chord_ctxs`
  stays disc/annular-only and mixed faces are excluded from
  `collect_rim_crossings`).
- **I3 (zero behavior change):** pairs with no mixed face take byte-identical
  paths. `Stage1Tess` gains a field; no existing output changes.
- **I4 (no chord geometry):** `build_disc_pair` never sees a mixed partner
  (its `loop_vertex_ring` chord-approximates arc loops — §6).
- **I5 (2-manifold or loud):** downstream yang stages/kernel-v2 validation
  unchanged — any residual defect stops loud, never silently wrong (P9).

## 5. Oracles

yang-rs integration tests (`tests/m8_mixed_coplanar.rs`), builders mirroring
`m8_holed_disc_coplanar.rs` (box_brep / signed_volume / is_watertight):

1. **Canonical (branch 1):** half-cylinder solid (caps = diameter segment +
   semicircle arc — the minimal mixed loop; lateral = 2-arc partial strip) +
   box stacked flush on the cap strictly inside the straight region. Union
   succeeds; mesh watertight, consistently oriented, outward; volume =
   V(half-cyl mesh) + V(box) within the chord-inscribed tolerance the disc
   suites use.
2. **Branch 2 (disc outer + polygonal hole, R0059 shape):** cylinder with a
   square through-bore (cap = full-circle outer loop + 4-segment inner loop —
   mixed, not annular) + flush box partner overlapping the bore edge
   (exercises straight-edge splits on a mixed face). Union succeeds; volume
   oracle as above.
3. **Edge/degenerate (branch 3, RED-then-stays-red):** box partner whose edge
   crosses the semicircle arc ⇒ `Err(YangError::CoplanarFacesUnsupported)`.
   Asserts the typed error, not a panic and not success.
4. **Adversary (branch 4, §6 hazard):** small disc (cylinder cap) coplanar on
   the half-cylinder cap, strictly inside the straight region ⇒ must either
   succeed with the exact-volume oracle or fail typed — asserted to NOT
   produce a silently wrong volume (assert success + volume; the fast-path
   exclusion makes this deterministic).
5. **kernel-v2 E2E** (`crates/kernel-v2/tests/m8_mixed_coplanar_chain.rs`):
   box − corner cylinder cut (cap becomes segments+arc) → flush box union on
   the cap away from the arc. Previously `KernelError::NotSupported`
   (coplanar); now succeeds with exact volume bookkeeping (mirrors R0021's
   auto-union shape).

Existing suites (m8_disc_coplanar, m8_holed_disc_coplanar, yr25/yr26/yr27,
full `./scripts/test.sh rewrite`) = the I3 regression oracle. Full categorized
assay before/after; gate = zero lost CORRECT, 0 WRONG.

## 6. Failure modes

- **Curved chord subdivided** (partner boundary crosses an arc, or an arc
  crosses the partner's region): loud `CoplanarFacesUnsupported` + probe
  `mixed-curved-chord-subdivided`. Lifting this wall needs arc-chain analogs
  of `rim_overrides` (crossing insertion into an *open* chain + partial-strip
  lateral re-pairing) — a named follow-up increment, not this one.
- **Chain missing / discontinuous loop:** `loop_polyline` error ⇒ the existing
  `polygon2d-a/b` wall (typed, probed).
- **Latent hazard closed (I4):** today a mixed×disc pair reaches
  `build_disc_pair`, which builds the partner ring from `loop_vertex_ring` —
  arc edges silently become their chords. A strictly-convex chord polygon in
  containment would emit sagitta-wrong geometry. Excluding mixed partners
  makes this structurally unreachable.
- **`collect_edge_splits` on arc edges:** today a full circle self-skips
  (`start == end` ⇒ zero-length) but an *arc* edge would be treated as its
  secant segment (exact-collinear vertices on the secant would register a
  bogus "split"). Guard: only `Curve::LineSegment` edges collect splits.
  Behavior change for existing classes: none (disc/annular faces carry only
  full circles; all-segment faces carry only segments).

## 7. Research basis

- [#24] Yang et al. 2025 §4.5.5 (Fig. 16): coplanar overlap resolved by ONE
  general 2D Boolean before discretization, identical meshes on the shared
  region, overlap boundaries become intersection curves. No face-shape
  lattice — this increment removes one shape restriction.
- [#24] §4.1/§4.4.1: shared exact boundary sampling (per-edge chains) is the
  watertightness mechanism; the overlay must consume the SAME chains Stage 1
  emits (I1), exactly as the disc rim path does (spec
  `m8_holed_disc_coplanar_overlay`).
- Cherchi 2020 §4 exact predicates: the curved-chord interior test reuses the
  exact rational collinearity/parameter predicate already in `rim_subdivided`
  / `collect_edge_splits`.

### 7a. Analytical vs approximate

Method: the overlay operates on the exact 2D projections of Stage-1 sample
chains (mesh-as-exact-intermediate, A15 hybrid corollary). Surface types
survive: the mixed face stays `Surface::Plane`; adjacent laterals keep their
analytic surfaces. No new mesh-as-final-representation. Surface pairs:
plane×plane only (the coplanar pair itself); curved laterals are untouched.

## 8. Implementation sketch (Phase 3 contract)

1. `Stage1Tess` gains `pub(crate) chains: BTreeMap<u32, Vec<u32>>` (populated
   from the existing `rim_rings`; construction site lib.rs:1448).
2. `loop_polyline` grows a sibling `loop_polyline_attributed` returning, per
   emitted polyline vertex, the emitting edge index (loop_polyline delegates —
   single walk implementation). `pub(crate)`.
3. stage0 `mixed_planar_face()` classifier per §2.
4. `face_polygon_2d_tessellated` gains a mixed arm (ordered AFTER disc and
   annular arms): tessellate via `stage1_tessellate_min_segments` (passing
   `forced_rim_n`), splice each loop with `loop_polyline_attributed`, project;
   polyline vertices that are B-Rep vertices → `corners`, chain Steiner
   vertices → `rim` map; per-ring segment mask `curved[i]` = emitting edge of
   vertex *i* is curved. Return type gains the mask component (empty ⇔ not
   mixed).
5. Pair loop: (a) disc fast path additionally requires both faces non-mixed;
   (b) after the zero-overlap early-out, if either face is mixed and any
   masked sub-chord (post-clustering coordinates, outer + holes) strictly
   contains an overlay vertex ⇒ probe + `pair_err` (branch 3); (c)
   `rim_cross_a/b` additionally require the face non-mixed (mixed rim maps
   are arc samples — `collect_rim_crossings` is disc/annular machinery).
6. `collect_edge_splits`: skip non-`LineSegment` edges (§6).
