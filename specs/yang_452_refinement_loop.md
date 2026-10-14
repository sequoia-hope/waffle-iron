# §4.5.2 — ONE refinement loop: the paper's response to mesh inadequacy

**Status:** DESIGN 2026-10-14 (Phase 1 of `specs/yang_faithful_cleanup.md`).
Owner `yang-rs` `boolean.rs` (`boolean`, `refine_452`, `refine_452_domain`),
`boolean/rim_junction.rs` (the lens sampler), `brep.rs` (per-face rebuild).
Resolves ledger **N62** when landed; retires the nine Stage-1 pre-emptive
arms (cleanup §2a) one per corpus run. Paper citations are line numbers in
`refs/text/yang2025_hybrid_boolean.txt`.

## 1. The paper's loop, as written

> "The failed areas will undergo local intersection handling or local mesh
> refinement until all the failures are resolved." (`:309-312`)
>
> "After optimization, we collect the point pairs that cannot converge to a
> distance of 0 within their domains. … If such bound cannot be found or
> after crossing boundaries the optimization still fails, we increase the
> mesh resolution locally and re-optimize as the second strategy shown in
> Fig. 14. The above procedures are repeated if optimization failure
> persists. The algorithm is guaranteed to terminate since the mesh
> intersections converge to the spline surface intersections under
> refinement." (`:646-652`)
>
> "The surfaces requiring refinement include those traversed by C_p (red
> regions) and the neighbors of a ring of them (orange regions). We then
> compute the intersections between the meshes only in the refined regions."
> (`:663-668`)
>
> "The self-intersections that may occur during surface discretization and
> the new intersections that may arise during mesh updating can lead to
> topological inconsistencies between the mesh and the B-Rep model … We
> detect these illegal intersections and perform local refinement."
> (`:752-758`, §4.5.4)

Four facts the paper fixes and the current code does not:

1. **Refinement is a RESPONSE to a detected failure**, never a prediction
   made before the arrangement runs. The paper discretizes at d_ε, runs,
   and refines where it failed. Our nine Stage-1 arms (graze, pinch, lens,
   edge graze, plane graze, segment-face graze, pierce dip, wedge,
   self-contact) compute at Stage 1 where the mesh WILL fail and inject
   samples pre-emptively — nine predictors, each wrong in its localization
   at least once (P0013, P0029, P0044, P0047, P0057).
2. **There is ONE trigger set**: optimization failures within their domains
   (§4.5), plus illegal intersections of the output (§4.5.4). Today the
   triggers are scattered: `Stage4RegionInvalid` → `refine_452`;
   boundary-domain fires → `refine_452_domain`; a self-intersecting output →
   the rim×plane retry, but only when a graze predictor says so; a Stage-5/6
   non-manifold STOP → nothing.
3. **Locality is by FACE** — the surfaces the failing curve segment
   traverses plus one ring — not by body (N62's whole-op ladder) and not by
   a rim lens whose shape is one arm's guess.
4. **The loop runs until it terminates**, by convergence under refinement;
   the budget is the ceiling, not the plan.

## 2. Contract

```
boolean(a, b, op):
    out   = run(a, b)                          # natural d_ε
    k     = 0
    loop:
        F = failures(out)                      # §3: the trigger set
        if F.is_empty():            return out
        if k == CEILING:            return out.into_stop()   # loud, with F
        R = region(F)                          # §4: faces + one ring, per operand
        (a, b) = refine(a, b, R, factor = 2)   # §5: those faces at d_ε/2^(k+1)
        out = run(a, b)
        k += 1
```

`run` is today's `boolean_once` with the pre-emptive arms REMOVED (§7
retires them one at a time). `out` is either an emitted B-Rep or a typed
STOP; both carry their failure set.

### 3. The trigger set `failures(out)`

Each trigger names the faces (input, face id) at its site. The set is the
union of:

| trigger | paper | today's producer | names |
|---|---|---|---|
| T1 relocation did not converge in-domain | §4.5 `:646-648` | `YangError::Stage4RegionInvalid { vertex, reason, under_resolution }` (`LocalRefinementRequired`, `OffCurveBeyondChordBand`, `RelocationCrossedCarrierVertex`) | the faces of the vertex's live triangles on both operands (today the error carries the vertex only — it gains `incident: Vec<(InputId, u32)>`, the `DomainFire` form) |
| T2 boundary point left its face across a crease | §4.5 / Fig. 13 (`:683-700`) | `DomainFire` records (`boundary_domain_fire_records`) | `fire.incident` |
| T3 illegal intersection in the output | §4.5.4 `:752-758` | `cherchi_rs::detect_improper_contacts` on the emitted mesh (`output_improper_count`) | the two faces of each improper pair, via the triangle attribution |
| **T3 — RE-SEQUENCED 2026-10-14 (measured).** With the tree broad phase every improper pair on the CORRECT outputs R0019 (217) and F0090 (1–2 per op) is a TRANSVERSAL crossing, never a coplanar touch: cross-face pairs (plane cap × cylinder / cone chord; R0019 ×197, every F0090 pair) are the Stage-4 mesh's own chords crossing beside a relocated intersection curve, and same-face pairs (R0019 ×20: one relocated vertex, tri `[648,134,178]`, pulled across its own fan) are FOLDS the mesh takes because §4.4.1 never retriangulates after relocation. The outputs are correct because the B-Rep emission does not rest on the mesh's local validity. So the paper's §4.5.4 detection presupposes §4.4.1's retriangulation — on a retriangulated mesh a surviving illegal intersection IS a resolution failure; on ours it is mostly the N2 gap. The project measured the same fact once before and then forgot it: `specs/yang_173_selfx_detector.md` §6 (2026-07-17) found the exact Stage-4 mesh test firing on relocation-minted seam chord crossings on 53 cases, 33 of them CORRECT, and built kernel-v2's RENDER-resolution gate (`validate/selfx.rs`) as the output-level layer for the C0116 class instead; this spec is where that two-layer finding becomes the loop's rule. Consequences: (a) the mesh-chord census stays a RECORD (`illegal_intersection_sites`) and is NOT a loop trigger until Phase 2 lands; (b) the class the arms protected (P0057's lens, the wedge trio: two faces' exact boundary curves crossing on ONE surface, seen one crate later as kernel-v2's `SelfIntersectingBooleanOutput`) gets an OUTPUT-level §4.5.4 check, **T3b**: for every output surface carried by ≥ 2 faces, the exact boundary curves of distinct faces are intersected pairwise (conic × conic on the same quadric, line × conic — ssi-rs / `boolean::predicates` vocabulary; no mesh); a crossing strictly inside both edges' parameter spans is an illegal intersection naming exactly the two faces. Cheap (a few faces per surface), exact, and it names the paper's "intersections that may arise during mesh updating" at the level the output actually depends on. T3b is Phase 1c's trigger beside T1/T2/T4/T5; T3 (mesh) joins when Phase 2 has made the mesh valid. **T3b closed form:** every boundary curve of a face on a quadric that is not a `SurfacePair` is a PLANAR section (circle, ellipse, line, hyperbola, parabola — each carries its plane); two such curves of two distinct faces on one quadric cross exactly where their two planes' common line meets the quadric — a line × quadric root pair, the §4.5.1 corner-transit vocabulary already in `stage4_phantom::segment_surface_roots` — and a root is an illegal intersection iff it lies strictly inside both edges' parameter spans (exact parameter tests, the §4.5.3 overtake arm's `conic_param` discipline). Coincident curves (`curves_coincide`) are a shared edge, not a crossing. `SurfacePair` curves are skipped in 1c (named gap; Newton on two implicit curves is Phase 2+). | | | |
| T4 non-manifold output | §4.4.3 `:596-600` (the paper's output is 2-manifold by construction; a non-manifold emission is a mesh-updating failure the paper's loop would have refined) | `NonManifoldOutput`, unpaired-edge census (`refine_452_unpaired`) | the faces of the triangles at the unpaired edges |
| T5 Stage-6 reassembly STOP (deadend, loop simplicity, face resolution) | same as T4 | typed Stage-5/6 errors | the faces the error names (they do today, by output face; map to input faces through the attribution) |

Not triggers: `NonManifoldInput` (topological, the paper's precondition
"watertight manifold B-Rep models" `:793`; refining it is futile, measured
R0019), the loud-by-design walls (coplanar residue, feature floor, the
minted tangency P0026), `AmbiguousCurve` (a Stage-3 producer fault).

**Cost of T3.** `detect_improper_contacts` is O(n²) today and was measured
to push F0090 / R0019 / R0081 over the corpus budget when always-on, which
is why it rides a predictor gate. The paper runs it on every result. The
remedy is an acceleration structure (a BVH over the output triangles,
cherchi-rs already has one for the arrangement), not a gate; it is the one
piece of new non-paper engineering this spec needs, and it is Phase 1
step 2.

### 4. The region `region(F)`

For each failure site, the faces it names on each operand, plus every face
sharing an edge with one of them (the paper's "ring of neighbors", Fig.
14's orange). Expressed per operand as `BTreeSet<u32>` of face ids. A site
with no curved face in its region contributes planar faces only, whose
refinement is a no-op at Stage 1 (planar faces carry no chord error, N9);
such a site refines nothing and, if it is the only failure, the loop ends
with the STOP standing — honestly, as today.

### 5. The refinement `refine(a, b, R, factor)`

**Per-face chord factor.** Stage 1 sizes every curved face from
`chord_rel() · scale` (`normals_chord_bounds.rs`; one thread-local divisor
for the whole op today). Measured 2026-10-14 (`stage1_tessellate.rs:466-527`):
the rim ring count is ONE `n_seg` for every circle edge of the operand —
`max_r` over all circles against one `d_eps` — so "per-face" refinement is,
concretely, **per-RIM N**: a circle edge's ring is sized from the finer of
its two incident faces' factors, and a band whose two rims differ in N is
tessellated by the azimuth ZIPPER (`stage1_azimuth_merge_unequal_rings.md`,
P0044) — the primitive that makes unequal rims legal geometry is already
landed. `natural_rim_n(y)` and every reader of the operand-wide N become
per-edge readers; Stage-3/4 band readers (`chord_tol_for_curved_owner` and
the `*_chord_bound` family) read the face's factor, so mesh and bands move
together per face (`fix_all_gates_sharing_a_metric`). It gains a per-operand map `face_factor: BTreeMap<u32, f64>`
consulted by every `*_chord_bound` reader for the face in hand: the bound
for face f is `CHORD_BASE · scale / (op_divisor · face_factor[f])`. Rims
shared by two faces take the MAX of the two faces' demands (a rim ring is
one sample set, the finer face wins — this is what keeps the shared-ring
watertightness N5 relies on, and it is exactly how the paper's CDT
re-samples a boundary curve once for both patches, `:402-407`).

Reader map (2026-10-14): the operand-wide bound is read by
`stage3_ssi::chord_tol_for_curved_owner` (9 call sites across Stage 3/4),
`stage4_correct::input_curved_chord_bound` (5), `boolean.rs:2744/:3209`,
`rim_junction::natural_rim_n` (`:235`), and Stage 1's ring builder
(`stage1_tessellate.rs:473-609`, one `n_seg`, the ellipse/sphere/torus
siblings at `:1095…`). A FACE-level reader already exists —
`stage4_correct::face_chord_bound(f, edges)` (`:6872`) — and is the shape
every reader takes under this spec; the owner-level readers become
"max over the owner's faces" where they genuinely need an operand bound.

`BRep::refined_at(faces: &BTreeSet<u32>)` doubles `face_refine[f]` for
every f in `faces` and rebuilds from topology
(`from_topology_with_rim_overrides`, standing samples and `forced_rim_n`
preserved). The body-wide `with_refined_chord` becomes the degenerate case
`faces = all`, and stays available for the census ladder only.

**Phase 1a LANDED 2026-10-14 (byte-identical).** The unit of refinement is
the paper's: *"Each iteration of our method increases the mesh density by
2×"* (`:365-366`) — `face_refine: BTreeMap<u32, u32>` holds a DENSITY
multiplier `k` (a power of two, ceiling `FACE_REFINE_MAX = 64`), and a face
at factor `k` has chord bound `d_ε / k²`. Where it lands in Stage 1
(`stage1_tessellate_once`): every circle rim's ring is `n_seg · k_e` with
`k_e` the max over the edge's incident faces (`edge_refine_factors`) — arcs
and ellipse chains alike; the cylinder chart reads `demand = min(guard
demand, budget / k²)` and a rim step of `2π / (n_seg · k)`; the torus patch
and the sphere read their own bound `/ k²`. Stage 0's ring readers
(`stage0::frame`, `mesh_build`, `coincident_cylinder_stage0`) honor the map
through `stage1_tessellate_with_rim_overrides_refined` /
`_with_standing_overrides`, so a refined rim stays conformal across a
coplanar overlay. NOT yet covered (named, not hidden): the holed-cone chart
(`ChartBudget::default()` at `surface_face_tessellators.rs:233/392`, which
today reads no budget at all) and the hyperbola chain (sampled from `d_ε`
directly). Pins `tests_unit/s452_face_refine.rs`: empty map byte-identical;
refining the lateral doubles both rims and both caps follow; refining one
cap refines its rim alone and the tube is the zipper's `m + n`; doubling
composes and saturates at the ceiling; the map stands across a
from-topology rebuild; out-of-range is loud. The map's production reader is
Phase 1c; until then `refined_at` is a pinned primitive.

**What the lens was.** `domain_fire_local_rim_overrides` (P0031) is a
hand-shaped special case of this: it refines ONE rim closure around ONE
azimuth at `natural_step / divisor`. Under the per-face factor the same
faces refine uniformly at d_ε/2 per rung — fewer bespoke rules (no apex,
no half-span, no arc clipping, no `LOCAL_452_MAX_SAMPLES_PER_N` budget,
no "all-planar sites derive nothing" fallback), and the sample count is
bounded by the face's own extent. The lens code is retired when the first
corpus run under per-face refinement holds P0031 (expected: it converges
at rung 2 or 3 instead of lens rung 1, at a cost of one more op).

### 6. The ladder and adoption

- Rungs: factor 2 per iteration on the CURRENT region (regions accumulate
  across iterations: a face refined at rung 1 and named again at rung 2 is
  at factor 4). Ceiling: `REFINE_452_MAX_FACTOR = 64` per face (R0085 /
  P0015 / P0039 all converge at or before 64). The under-resolution
  certificate, when present, sets the FIRST factor for the faces it names
  (the existing economy, `refine_452_rounds_for`).
- Adoption: `failures(out)` is EMPTY. There is no separate "unpaired == 0"
  / "fires == 0" / "improper == 0 unless not required" trio; those are T4,
  T2, T3. (`YANG_452_REQUIRE_CLEAN` is deleted in Phase 0; the clause it
  guarded is now simply T3.)
- Strict-decrease monitor (Q3 clause 2): kept as a per-trigger count
  `|F|`; a rung that does not reduce `|F|` continues (the paper does not
  abort on a plateau — P0039's rung 2 plateaued and rung 6 converged) but
  the ceiling bounds the loop. The monitor becomes a probe line.
- Ordering: the loop replaces the three-remedy sequence in `boolean`
  (rim-plane retry → STOP ladder → domain ladder). One loop, one trigger
  set, one region rule.

### 7. Retiring the arms (the proof obligation)

One arm per corpus run, cheapest first. For arm X:

1. Run the corpus with X off (`YANG_<X>=0`) and the loop on. Record every
   case X protected (the moves CORRECT → anything).
2. Each such case must come back CORRECT through the loop (by the trigger
   it raises — T3 for the wedge/dip class, whose failure is a silent
   topology the output self-intersection reveals; T1/T2 for the graze
   class). A case that does not: the loop is incomplete, find which trigger
   it needed (`YANG_452_PROBE`), fix the loop, re-run. The arm is never the
   answer.
3. Delete the arm, its knob, its demand functions, and the off-arms of its
   pins (the on-arm stays, now asserting the loop converts the fixture).
4. Record wall-time before/after; the paper's cost is paid honestly.

Order: wedge (`segment_risk_intervals` + demand), pierce dip, segment-face
graze, edge graze (`edge_graze_sites` …), rim×plane graze (both forms,
`rim_plane_graze_min_segments` / `_local_rim_overrides`, and the
`refine_rim_plane` retry pass), cylinder pinch, cylinder graze
(`graze_min_rim_segments`), Case-IV phantom floor
(`phantom_min_rim_segments`). The self-contact guard is Phase 3's.

### 8. What this does NOT change

- Stage-1 mints that are the paper's own "set r_A = r_B = r" (tangent
  point/generator, §4.3.3; rim junctions; P3a conformal junction sampling;
  Stage-0 overlay). They are not predictions of failure, they are shared
  exact points, and they stay.
- Stage 4's relocation and §4.5.1 transit. The loop wraps them.
- The §4.5.2 census instruments (`YANG_452_PROBE`, `YANG_452_ROUNDS` for
  the census ladder).

## 9. Oracles

- Unit: `refined_at` doubles the named faces' density and leaves the others
  byte-identical; a shared rim takes the finer face's ring; `failures()`
  names the faces of each trigger on fixtures (one per trigger type);
  the loop converges on the P0031 fixture without the lens; the loop
  STOPs loudly at the ceiling on P0026's minted tangency.
- Corpus: step 1 (loop + per-face refinement, arms still on) must be
  byte-identical or monotone; each arm retirement is its own run (§7).
- Prospector: the per-seed ERROR rate after Phase 1 is the headline.

## 10. Landing sequence

1. ~~`face_factor` + `refined_at` (Phase 1a, gated: nothing reads the map until 1c) — byte-identical.~~ **LANDED 2026-10-14** (`face_refine`, see §5).
2. ~~`failures(out)` with T1–T5 and face attribution; `Stage4RegionInvalid` gains `incident` (1b) — byte-identical (read-only).~~ **LANDED 2026-10-14** as `boolean::failures` — a thread-local failure-site RECORD (the `DomainFire` channel's pattern), not a widened error type: `reset()` at every `boolean_once`; producers push where each failure is detected with mesh and attribution in hand — T1 in the Stage-4 wrapper (`faces_at_vertex` of the STOP'd vertex; a sentinel records no face), T2 in `boundary_domain_postcondition` (the fires' `incident`), T4 at the watertightness gate (`detect_nonmanifold_seams` region keys); T3 is computed on an emitted body by `illegal_intersection_sites` (`detect_improper_contacts` pairs → the two triangles' attributions); `failures(result)` = recorded ∪ T3, or one faceless `ReassemblyStop` for an error nobody recorded (a STOP is never silently an empty set); `region(sites, a, b)` = named faces + one ring per operand, by shared B-Rep VERTEX (yang's `LineSegment` edges are one copy per loop, so edge adjacency would miss straight-edge neighbours). Pins `tests_unit/s452_failure_sites.rs` (7). Record-only until 1c reads it.
3. The loop replaces the three remedies, lens kept as a first rung for one run (1c) — corpus monotone; then lens deleted (1d) — corpus holds P0031.
4. BVH-accelerated `detect_improper_contacts` so T3 can be always-on (1e) — corpus wall-time measured. **LANDED 2026-10-14:** `cherchi_rs::aabb_tree::AabbTree` (the labeling octree's deterministic rules — depth 8, leaf split 16, duplication budget 2, fixed octant order, inclusive overlap — as one shared implementation; `TriOctree` is now a wrapper over it), `overlapping_pairs()` as the census's broad phase, the pre-1e sweep kept under `#[cfg(test)]` as the oracle (`tree_and_sweep_broad_phases_agree`: identical improper and unresolved pair sets on three seeded soups), plus the `#[ignore]`d `broad_phase_cost_census` measurement. **Measured, synthetic (seeded LCG soups, release):** n = 20 000 → tree 0.020 s vs sweep 0.022 s; n = 60 000 → tree 0.067 s vs sweep 0.092 s. The broad phase was NOT what made the scan expensive: on a uniformly random soup both are fast, so on a real output the cost sits in the exact `classify_pair` calls on the box-overlapping second-ring neighbours every closed mesh has (adjacent triangles share an index and are skipped for free; their neighbours' neighbours overlap in box without sharing one). **Measured, real outputs (`YANG_SELFX_COST_PROBE=1`, release, tree broad phase):** F0090's 30 subtracts grow 82 → 11 642 triangles and the census grows 0.001 → 1.50 s per op (≈ 18 s over a 184.6 s case); R0019's 14 674-triangle output costs 0.043 s; R0081's 105 752-triangle union costs 0.537 s. **Cost is no longer the obstacle to an always-on §4.5.4 scan.** The obstacle it uncovered is SEMANTIC: these SUPPORTED_CORRECT outputs carry non-zero improper counts — R0019 reports 217 pairs, R0081's first op 7, several F0090 ops 1–2 — so the raw census cannot be T3's predicate; the pairs' kinds (transversal vs coplanar) and faces (same face vs two faces) are being classified next, and T3 will be defined on the paper's "illegal intersection" (a crossing), not on "any exact contact". The real-output measurement (`YANG_SELFX_COST_PROBE=1` on F0090 / R0019 / R0081) decides; if the exact predicate dominates, the remedy is the paper's own scope — §4.5.4 names "the self-intersections that may occur during surface discretization and the NEW intersections that may arise during MESH UPDATING" (`:752-754`): the first class is each operand's own, already the Stage-1 self-contact guard's; the second arises only where Stage 4 moved vertices, so T3 scans the triangles of faces that carry a RELOCATED vertex (plus one ring) against the whole mesh — a tree query per touched triangle, not a scan of every pair. That is a scoping the paper states, not a predictor gate. **Design (2026-10-14):** the census's broad phase (`cherchi-rs/src/inputcheck.rs`) is a sweep on min-x over per-triangle AABBs, then the exact `classify_pair` on every box-overlapping candidate; on a large output (F0090 / R0019 / R0081, the three cases an always-on scan pushed past the budget) the sweep degenerates — every box whose x-extent reaches the current one is a candidate regardless of y and z. The remedy is a tree broad phase: an AABB tree over the output triangles (the `labeling::octree::TriOctree` shape — deterministic build, fixed octant order, sorted deduped query results), each triangle querying its own box, candidate pairs `(s < t)` collected once, the exact predicate unchanged. Acceptance is twofold: the improper-pair SET is identical to the sweep's on every corpus output (an in-crate oracle runs both on the same meshes), and the three large cases stay inside the budget with the scan always on. The arrangement's own candidate search (`arrangements/intersection_detection.rs::detect_intersecting_pairs`, an O(n²) AABB loop parallelized by rayon under the `parallel` feature) is the same problem one stage earlier and is the reason rayon's CPU exceeds wall on the gear-revolve cases (ledger seed 5, index 37); it is a cost follow-up of its own, outside this spec, bound by reference parity (identical sorted pair list).
5. Arm retirements §7, one per run (1f…).
6. N62 → RESOLVED; N5's "local arms" paragraph struck; the nine arms' specs marked SUPERSEDED by this one.
