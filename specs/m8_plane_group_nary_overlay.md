# M8 slice: plane-grouped n-ary coplanar overlay (flush bridge across two tower tops)

Status: IMPLEMENTING (task #129)
Driver: user case 2026-07-11 `error_coplanar.waffle` — 30×10 base extruded
2 mm, two 10 mm-square towers (U shape), then a bridge rectangle sketched on a
tower top spanning both towers. The bridge auto-union dies at the typed
Stage-0 wall `stage0/mod.rs` `multi-pair`: the bridge's BOTTOM face is
near-coplanar with BOTH tower tops (`count_b = 2`, 6 cross pairs total), and
`stage0_preprocess` walls any face participating in more than one pair.

## 1. Goal

Lift the "face in >1 coplanar pair" Stage-0 residue for the pure-polygon
planar class by processing coplanar cross pairs in PLANE GROUPS: connected
components of the pair graph (pairs joined by a shared face). A group with
one pair runs today's 1×1 path byte-identically; a group with ≥2 pairs runs
ONE n-ary exact 2D overlay — side A = all its A faces, side B = all its B
faces — so the repeated face is segmented against the union of its partners
in a single consistent triangulation.

User-visible: the flush-bridge union (and its subtract/intersect variants)
builds a correct solid instead of an error toast.

## 2. Parameters

None user-facing. Internal inputs per group:

| input | source | range |
|---|---|---|
| group pairs | `scan_near_coplanar(a, b).cross` connected components over shared `face_a`/`face_b` | ≥ 1 pair |
| canonical frame | `canonical_frame(a, face_a)` of the group's LOWEST pair index (deterministic) | unit plane |
| per-pair `band` | YR24 detection band | sub-model-resolution |

## 3. Branch table

| # | Branch | Behavior |
|---|---|---|
| B1 | group has 1 pair | existing 1×1 path, byte-identical (incl. disc/annular/mixed/rim machinery) |
| B2 | group ≥2 pairs, every face planar with pure all-`LineSegment` loops (holes allowed), per-side outward normals uniform | n-ary overlay path (NEW) |
| B3 | group ≥2 pairs, any face disc / annular / mixed / otherwise non-pure-line | loud `CoplanarFacesUnsupported` (typed residue, unchanged wall text) |
| B4 | group ≥2 pairs, a side's faces have MIXED outward orientation on the plane | loud `CoplanarFacesUnsupported` |
| B5 | group total exact Overlap area = 0 | benign in-plane touch: `continue` (N17), same as 1×1 |
| B6 | op keep-rule on membrane | unchanged (per-pair `PairPlane`, group-uniform `opposite`) — Union/Intersect keep iff `!opposite`, Subtract keep iff `opposite` |
| B7 | overlay engine: 1-polygon sides | `coplanar_overlay(a,b)` ≡ `coplanar_overlay_multi(&[a],&[b])` (delegation, bit-identical) |
| B8 | overlay engine: same-side polygons overlap (contract violation) | exact coverage identity fails → loud `CoverageMismatch` |

## 4. Invariants

- I1 (coverage, exact): per side X of a group overlay,
  `Σ area(XOnly) + Σ area(Overlap) == Σᵢ area(polyᵢ of X)` in rationals.
- I2 (attribution): every output triangle inside side X is attributed to
  EXACTLY one input polygon of X (exact parity per polygon; uniqueness is a
  consequence of I1 + interior-disjoint inputs). Per-face overrides emit only
  the triangles attributed to that face.
- I3 (identical overlap meshes, §4.5.5): Overlap triangles resolve to
  bit-identical 3D vertex triples in both solids' Stage-0 meshes (winding per
  solid).
- I4 (no regression): a 1-pair group takes today's code path; the engine's
  single-polygon behavior is bit-identical through the multi entry point.
- I5 (conformality): overlay vertices subdividing a face's boundary edges
  propagate to adjacent faces via `collect_edge_splits` per group face —
  T-junction-free Stage-1 meshes (existing machinery, class-filtered per side).
- I6 (snap): all group faces' loop vertices snap onto the ONE group canonical
  plane; cross-weld (B adopts A's coords at equal in-plane keys) runs across
  the whole group.

## 5. Oracles

- yang-rs e2e (`tests/m8_bridge_nary_overlay.rs`, RED first):
  - canonical: U-solid (base ∪ tower1 ∪ tower2, chained) ∪ bridge →
    watertight, χ = 0 (genus-1 frame), signed volume exactly 3.2 (rel 1e-9).
  - edge: NARROW bridge (partner overlap strictly interior in y, partial in
    x) → the pure 2-pair group without the 4 zero-overlap side pairs;
    χ = 0, exact volume.
  - branch Subtract: U − bridge = U exactly (membranes kept, tool removes
    nothing): χ = 2, volume = vol(U).
  - branch Intersect: U ∩ bridge = empty → typed empty-result error.
- overlay engine unit (`tests/yr25_coplanar_overlay.rs` additions):
  two-squares-vs-spanning-rectangle multi overlay — exact per-class areas,
  per-polygon attribution counts, coverage identity; 1×1 delegation equality
  (verts/tris/class byte-equal vs `coplanar_overlay`).
- assay corpus: NEW case **C0101** (family `family_user_reported`, exact
  chain volume 3.2, `euler_target 0`) — RED (UNSUPPORTED(coplanar-boolean))
  before the fix, pinned `SupportedCorrect` in `smoke_corpus_boundary_categories`
  after; full-corpus P9 gate zero-lost.
- user fixture replay (`error_coplanar.waffle`): no engine error, tessellates.

## 6. Failure modes

- B3/B4 walls: typed `YangError::CoplanarFacesUnsupported` naming the first
  offending pair (probe tags `nary-face-unsupported`, `nary-mixed-orientation`
  under `YANG_COPLANAR_PROBE=1`).
- Same-side overlapping inputs (impossible for a valid manifold operand after
  the intra-solid wall; defensive): `CoverageMismatch` → typed pair error.
- Degenerate group frame: existing `frame-degenerate` wall.
- Downstream stages (cherchi arrangement, Stage 3/4/6) are unchanged; any
  configuration they cannot resolve stays a loud typed error (P9-safe).

## 7. Research basis

- [#24] Yang, Jia & Yan 2025 §4.5.5 (Fig. 16;
  `refs/text/yang2025_hybrid_boolean.txt:716-760`): coplanar preprocessing is
  defined per COPLANAR PLANE — "two coplanar planes will be segmented into
  three parts after a Boolean operation in 2D"; the overlap becomes ONE
  trimmed common planar surface meshed identically for both models. The
  n-ary group is the direct reading of that construction when one trimmed
  face overlaps several partner faces on the plane: A-only / B-only /
  overlap are set-level regions of the plane, not per-pair artifacts.
  The roadmap reframe (2026-06-26) names "faces in >1 pair (n-ary overlay)"
  as walled residue of the general §4.5.5 program; this slice closes it for
  the pure-polygon class.
- [#9]/[#38] Cherchi 2020/2022: downstream multi-label welding + keep-rules
  unchanged.
- Engine method unchanged: exact rational arrangement + trapezoidal
  decomposition + parity classification (PR-YR25). Union-of-disjoint-regions
  membership by parity over the combined edge set is exact for
  interior-disjoint polygons; the coverage identity (I1) is the loud guard.

## 7a. Analytical vs. approximate method

Method: exact (rational 2D boolean; planar faces only — plane×plane pairs).
No SSI solver involvement beyond the existing plane∩plane `LineSegment`
short-circuit for overlap-boundary seam curves. No mesh approximation is
introduced; Stage-0 output remains the exact §4.5.5 shared triangulation.

## 8. Increment plan

1. RED: corpus C0101 + smoke pin + yang-rs e2e tests (fail at the multi-pair
   wall).
2. Engine: `coplanar_overlay_multi` (+ per-triangle `poly_a`/`poly_b`
   attribution fields on `ClassifiedOverlay`), `coplanar_overlay` delegates.
3. Stage 0: pair grouping; n-ary group body (snap, cluster, overlay,
   per-face overrides + splits, PairPlane per pair); B3/B4 walls.
4. GREEN: yang-rs suite, rewrite/fast tiers, C0101 correct, full assay
   zero-lost, user fixture replay, WASM rebuild.
