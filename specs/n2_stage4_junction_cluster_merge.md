# N2-3a — Stage-0 exact rim placement for overlay vertices (amended) — Spec

Third N2-track increment (parents: `specs/n2_stage4_mesh_updating.md`,
`specs/n2_stage4_dt_recompute.md`; design history:
`specs/yang_n2_stage4_cdt_mesh_updating.md`). **AMENDED 2026-07-02 after the
test-phase grounding (§0 items 4–5): the root cause is one layer upstream of
the original diagnosis** — the fix is Stage-0 mint-time exactness (Yang §4.5.5
"overlap boundaries become intersection curves", i.e. exact curve geometry on
the shared-region boundary), with the originally-specified Stage-4 Fig-11(b)
junction-cluster merge retained only as a CONTINGENT part 2 (§3b).

## 0. Grounding trail (all measured)

1. **The Stage-4 phase-3 STOPs have no live consumer** (probes: 0 hits across
   yang-rs suites, campaign, 194-case assay). The §5c.5 re-mesh wiring stays
   deferred until a consumer exists; `stage4_mesh_update`/`d_of_t` remain its
   ready machinery.
2. R0072 fails kernel-v2's debug-only `VertexOffSurface` tripwire (release
   builds would ship the geometry silently) — the acceptance target of this
   increment.
3. Original diagnosis: a tangency junction cluster (v7/v8/v11, one geometric
   junction) merged onto an off-curve neighbor instead of the relocated point
   q — a real Fig-11(b) violation, but…
4. **Test-phase finding (spec-scope gap):** enumerating ALL off-band loop
   vertices of R0072 shows **12**, of which 11 are NOT the cluster: they sit
   at chord-sagitta positions (5.4e-6..7.3e-6 off the r=2.13e-4 cylinder;
   worst = r·(1−cos(π/13)), the N=13 chord sagitta exactly).
5. **Root cause (traced):** the coplanar overlay's trapezoidal decomposition
   (`coplanar_overlay.rs:394`, cell corners :447-450) splits every rim chord
   at every event x-coordinate; Stage-0's overlay-vertex resolution closure
   (`stage0.rs:418-442`) resolves those points through face-corner / rim-ring
   exact-key / ULP-snap branches, then **falls through to a raw in-plane
   `frame.lift` (stage0.rs:438)** — minting 3D vertices ON the chord,
   sagitta-deep off the exact rim circle. The SAME sweep already projects the
   *opposite* rim's points at exact radius (`opp_radius`, stage0.rs:1048-1055)
   — the asymmetry is the bug surface. Stage-4 can never repair this: the rim
   edges are same-input (A-lateral × A-cap) and the
   `build_intersection_curves` same-input skip (lib.rs:5428) is *semantically
   correct* per §4.5.5 (a solid's own face boundary is not an A×B intersection
   curve; emitting it also collapses membrane triangles, lib.rs:5529-5547).
   The exact circle is one call away at the mint: `disc_circle_edge(a,
   p.face_a)` (stage0.rs:660) yields the rim `Curve::Circle`.
6. **Implementation-phase finding (measured, P10 STOP honored):**
   UNCONDITIONAL exact minting folds the overlay where the rim tessellation is
   coarse — moving a chord vertex outward by the local sagitta can cross
   other-input mesh edges inside the chord↔arc band (R0013: 9-gon rim,
   sagitta 0.53 at r=8.73, 175 mints → 1 inverted triangle → cherchi
   self-intersection). Measured spec-exact: R0013 SUPPORTED_CORRECT→ERROR,
   R0024 campaign RED, real-R0072 folds at Stage-0. The folding population is
   a REAL mesh-updating demand: repositioned boundary vertices need local
   re-triangulation (Yang Fig 11) — recorded as a live consumer for the
   deferred overlay-level mesh-updating machinery (general Stage-0 §4.5.5
   milestone). Until that lands, exact minting is gated on local validity
   (§3 row 4–5 as amended).

## 1. Goal

Every overlay-derived vertex that lies on a disc-rim chord is minted ON the
exact rim circle at Stage-0 resolution time **wherever that placement keeps
the pre-existing overlay triangulation valid (§3 fold gate)**, for BOTH
kinds:

- **pure subdivision points** (x-event splits of a rim chord; the 11): radial
  projection onto the exact circle in the cap plane;
- **rim × other-input-edge crossings** (the overlap-boundary junctions; the
  tangency corner): the **exact 2D circle∩line intersection** (radial
  projection would slide them off the other input's edge/plane — fixture
  invariant I2 pins the exact junction).

Both solids' meshes and the lateral rim overrides consume the SAME `coords`
resolution, so one projection site keeps cap, lateral, opposite rim, and both
meshes identical on the shared region (§4.5.5's identical-mesh requirement).

## 2. Parameters

No new public API, no new tunables. Inputs in hand at the seam (all existing):
the resolution closure's `a`/`p.face_a`/`frame`, `disc_circle_edge` (exact
`Curve::Circle`), the overlay's `exact_verts` (rational 2D positions), and the
existing exact on-rim-chord collinearity + interior-parameter test
(`rim_subdivided` / `collect_rim_crossings`, stage0.rs:1008-1027) that already
identifies exactly these vertices. Classification of "also on another input's
edge" uses the overlay's exact rational data (a crossing point lies on a B
input sub-segment) — exact predicates, no tolerance.

## 3. Branch table (the coords-resolution closure, stage0.rs:418-442)

| Case (checked in order) | Today | After |
|---|---|---|
| Face corner (`corners_a/b` hit) | exact corner | unchanged |
| Rim-ring vertex (exact-key hit) | exact rim sample | unchanged |
| ULP-snap (`rim_pts`, ~1e-13) | snapped | unchanged |
| **NEW: on a rim chord AND on another input's edge (exact rational tests)** | raw `frame.lift` → chord position | exact 2D circle∩line intersection point, lifted to 3D (on circle AND on the other edge) |
| **NEW: on a rim chord only (x-event subdivision)** | raw `frame.lift` → chord position | radial projection onto the exact circle: `center + radius·normalize(lift(q) − center)` in the cap plane |
| Not on any rim chord (straight-edge / interior points) | raw `frame.lift` (exact for straight edges) | unchanged |

**Fold-validity gate (amendment 2, per §0 item 6):** after resolution, any
N2-3a-minted vertex whose incident overlay triangle's 2D signed area becomes
≤ 0 is REVERTED to today's chord lift, iterated to a deterministic fixpoint
(minted indices tracked explicitly — coordinate-comparison inference is
forbidden; it falsely captures ULP-snapped rim vertices). A reverted vertex is
byte-identical to today's behavior and remains observable via kernel-v2's
untouched tripwire (§6). This is a validity check, not a tolerance: exact
placement applies wherever it does not invert the pre-existing overlay
triangulation; the residual population is the recorded mesh-updating demand
(§0 item 6), not silently blessed geometry.

**Sub-floor shared-mint collapse (amendment 3, 2026-07-06 — M8 holed-disc
increment 4, task #61, spec `m8_holed_disc_coplanar_overlay` §8):** the exact
trapezoidal overlay legitimately mints femto-twin split pairs (two sweep-event
columns ULPs apart in `u` crossing the same rim chord). Resolved
independently, the twins become two distinct on-circle points closer than
`MIN_FEATURE_SIZE` (A14.2) — below the kernel's supported feature floor, so
they cannot be two real features; left distinct, the wedge between them folds
under amendment 2's gate, reverting BOTH mints to chord positions that
Stage 4 cannot relocate (no conic assignment — the R0072 micro class,
increment-3 quarantine). After resolution and BEFORE the fold gate, minted
vertices are grouped per rim-ctx slot (a shared target cannot lie on two
circles) by 3D distance < `MIN_FEATURE_SIZE` (greedy first-seen; real
crossings are ≥ the floor apart, so groups are isolated and cannot
chain-drift), and every multi-member group is collapsed to ONE shared
on-circle target: a crossing-branch member if the group has one (I2 — the
junction stays on the other input's edge), else the first member — never an
average. The resulting 2D-distinct/3D-identical boundary pair is the
established M-B emission-identification class (degenerate wedge dropped at
emission, neighbors' resolved edges pair directly; the weld measured working
at the same fixture's box↔circle junctions v19/v23).

**Gate degeneracy skip (part of amendment 3):** the fold gate ignores
triangles whose RESOLVED 3D image is degenerate (bit-duplicate vertices, the
M-B drop class) — they are never emitted, so their 2D fold state (the
collapsed twin wedge projects to area exactly 0) must not revert mints.
Scoped strictly to never-emitted triangles: the gate's judgment on every
emitted triangle is unchanged.

**Constrained flip repair (amendment 4, 2026-07-07 — M8 holed-disc
increment 7, task #62, spec `m8_holed_disc_coplanar_overlay` §8):** the
measured F0086–F0090 corpus-path residual. The production sketch frame
(`tangent_x_from_normal([0,0,1])` → x=(0,−1,0)) rotates the overlay's 2D
coordinates relative to the direct-constructor chain, and under that
alignment a femto-strip (two sweep-event columns ULPs apart in `u`) can
intersect BOTH a rim chord and the overlap boundary. The strip-diagonal
sliver triangle (rim-chord vertex at the bottom, two intersection-curve
vertices at the top) is inverted by ANY on-circle mint of its rim vertex —
the radial-projection displacement (~sagitta, 1e-5–1e-3) dwarfs the strip
width (~1e-17) — so amendment 2's gate reverted the mint and the chord-
position vertex escaped into the output rims (kernel-v2 `VertexOffSurface`,
F0086 FaceId 15 class; measured fold area −2.4e-6 at cut 2 of the
engine-frame chain). Reverting is the WRONG remedy when the fold is
repairable: the recorded demand is overlay-level mesh updating ([#24 Yang
§4.4.1 Fig 11] — a repositioned boundary vertex needs local
re-triangulation). Amendment 4 wires the minimal deterministic form,
Lawson edge flips constrained to region interiors:

- Before reverting a folded emitted triangle that has ≥1 minted vertex, try
  flipping each of its three edges in fixed order. A flip is **legal** iff
  the edge is shared with exactly ONE other triangle, of the SAME
  `RegionClass` (a class-boundary edge IS the intersection curve and a
  single-incidence edge is the domain boundary — both immovable), the
  neighbor's resolved 3D image is non-degenerate, and the replacement
  diagonal does not already exist in the mesh. A flip is **accepted** iff
  both replacement triangles are valid under the CURRENT resolved
  coordinates: signed 2D area > 0, or 3D-bit-degenerate (the M-B
  emission-drop class). The first legal+accepted flip is applied (the two
  triangles are rewritten in place, classes unchanged).
- Only when NO edge admits a legal+accepted flip does the amendment-2
  revert run — the R0013-class folds that cross another input's edges keep
  their loud/revert behavior; nothing is silently blessed.
- Termination: with coordinates fixed, every accepted flip strictly reduces
  the folded-triangle count (both replacements are valid by acceptance and
  only the two rewritten triangles change); reverts are one-way (a vertex
  reverts at most once). The combined gate loop is a deterministic fixpoint
  (triangle-index order, fixed edge order — I6).

| Gate case (amendment 4) | Behavior |
|---|---|
| folded emitted tri, ≥1 minted vert, legal+accepted flip exists | mint KEPT; local re-triangulation (first accepted flip in fixed order) |
| folded emitted tri, ≥1 minted vert, no legal/accepted flip | amendment-2 revert to chord lift (unchanged) |
| folded tri, no minted vert / 3D-degenerate tri | ignored (unchanged) |

Research basis: [#24] Yang 2025 §4.4.1 Fig 11 (mesh updating for repositioned
boundary vertices); Lawson 1977 local edge flips (the classical constrained
flip repair). Oracles: the engine-frame chained fixture
(`kernel-v2/tests/m8_swiss_cheese_chain.rs::engine_frame_*` — RED before this
amendment at cut 2, exactly the corpus replay's failure), the F0086–F0090
corpus family, full yang-rs/kernel-v2 suites, corpus P9 gate (0 WRONG, zero
CORRECT lost).

**Cavity relocation (amendment 5, 2026-07-07 — M8 holed-disc increment 8,
task #62, spec `m8_holed_disc_coplanar_overlay` §8):** the measured
F0087/F0089/F0090 residual — the rim-mint COLUMN HOP. Stage 1's global chord
bound (`d_ε = 1e-2·AABB-diag`) gives a large plate a coarse uniform rim
(F0087: N=14, sagitta 4.9e-2), so a rim mint's in-plane u-displacement can
exceed the gap to a POPULATED sweep-event column (F0087 cut 7: displacement
1.3e-2 vs gap 8.9e-3 to the tool's leftmost-x column). Every long CDT
triangle in the strip between the two columns folds together; amendment 4's
single flips provably cannot repair it (each folded tri's rim edge is domain
boundary, its side edges neighbor other FOLDED tris), and the folded set's
boundary polygon is NON-SIMPLE under the moved vertex (it pokes past the
hopped column), so fan/ear re-triangulation of the folded set alone cannot
either. Sampling floors measured and rejected (`YANG_NSEG_FLOOR`, 9f37aa5c):
the hop survives at every finite N; N=56 clears this chain at 5× runtime.
Amendment 5 wires the full Fig-11 form — delete-and-reinsert vertex
relocation, a constrained star-cavity re-triangulation around the minted
position:

- When a folded emitted triangle with ≥1 minted vertex admits NO
  legal+accepted flip, each of its minted vertices `v` is tried in fixed
  vertex-slot order for **cavity relocation**:
  1. **Star collection:** all triangles incident to `v`. The link chain is
     assembled from the star triangles' oriented opposite edges (consistent-
     CCW mesh ⇒ head-to-tail chaining); open chain for a domain-boundary
     vertex, closed for an interior one. A non-manifold or non-chainable
     star rejects (revert fallback).
  2. **Cavity carve (constrained visibility growth, deferring at
     constraints):** the cavity starts as the star; each link edge carries
     the class of the cavity triangle that exposed it. The candidate fan
     triangle over a link edge is `(v, wᵢ, wᵢ₊₁)` under the CURRENT
     resolved coordinates. While some fan triangle is invalid (2D signed
     area ≤ 0 and not 3D-bit-degenerate), the cavity grows across that link
     edge into its one external neighbor — growable iff the edge has
     exactly one external incident triangle (a single-incidence edge is the
     domain boundary), the neighbor's class equals the link edge's class (a
     class-boundary edge IS the intersection curve — never crossed), and
     the neighbor's apex is not already a link vertex nor `v` (a repeat
     pinches the cavity non-simple). An UNGROWABLE invalid edge is
     DEFERRED, not fatal. Growth strictly enlarges the cavity (termination)
     and blocked edges can never become growable (fan validity is
     coordinate-determined; externals only shrink), so one forward scan
     with in-place re-checks is a fixpoint.
  3. **Re-triangulation:** if no edge was deferred, the fan
     `(v, wᵢ, wᵢ₊₁)` IS the re-triangulation (each member valid by
     construction, carrying its link edge's class). Otherwise the cavity is
     not star-shaped from `v`'s minted position — measured F0087 cut 7: the
     mint crosses the LINE of a tool chord whose constraint segment lies
     elsewhere, so the fan over that chord inverts and growth may not cross
     it — and the cavity polygon `[v, w₀, …, w_k]` is re-triangulated by
     **constrained exact ear-clipping** instead: the constraint edge stays
     a cavity BOUNDARY, connected to other link vertices rather than `v`.
     Guards, each a reject: single-class cavity and open chain only (no
     constraint spokes to preserve); the polygon must be exactly simple and
     CCW on its deduplicated position ring (rational predicates over the
     raw f64 frame projections — P9, no tolerance); an ear needs exact-CCW
     orientation, gate validity, no other polygon vertex strictly inside or
     on it, and a diagonal that does not already exist outside the cavity.
     Ears whose 3D image is bit-degenerate (collapsed sub-floor twins) clip
     freely — they are dropped at emission (M-B class).
  4. **Commit:** cavity triangle count equals replacement count (a
     boundary star of k triangles has a k-edge open chain; each growth step
     adds one triangle and one link edge; a (k+2)-gon ear-clips to k
     triangles), so the replacement overwrites the cavity triangles in
     place; the edge map is updated incrementally. Constraint edges are
     never removed, so region-interface and domain-boundary polylines are
     preserved.
- The amendment-2 revert remains the fallback when every minted vertex of
  the folded triangle rejects (build-then-commit: a rejected relocation
  leaves NO mutation). Nothing is silently blessed: the loud tripwire path
  stays intact.
- Termination of the combined gate: coordinates are fixed during a
  relocation (pure combinatorial rewrite, same contract as amendment 4's
  flips); every committed relocation replaces ≥1 folded triangle with
  all-valid triangles and folds cannot be created (no coordinates move), so
  the folded count strictly decreases; flips strictly decrease it; reverts
  are one-way. The fixpoint loop is deterministic (triangle-index order,
  vertex-slot order, first-invalid-link-edge growth order — I6).

| Gate case (amendment 5) | Behavior |
|---|---|
| folded emitted tri, ≥1 minted vert, no flip, all fan triangles valid after growth | mint KEPT; cavity re-fanned from the minted position |
| …deferred constraint edge remains, single-class open-chain cavity, simple CCW polygon, ear-clip completes | mint KEPT; cavity ear-clipped (constraint edges stay cavity boundaries) |
| …all relocations reject (multi-class / interior vertex / non-simple polygon / no clippable ear / non-manifold star) | amendment-2 revert to chord lift (unchanged) |
| all other gate cases | unchanged (amendments 2–4) |

Research basis: [#24] Yang 2025 §4.4.1 Fig 11 (delete-and-reinsert mesh
updating for repositioned boundary vertices; `refs/text/
yang2025_hybrid_boolean.txt:556-560`); Bowyer–Watson cavity insertion
(visibility-carved star-shaped cavity, fanned from the inserted point) as
the classical constrained realization. Oracles: the F0087 engine-frame
chain (`kernel-v2/tests/m8_swiss_cheese_chain.rs::
f0087_engine_frame_seven_hole_chain`, `#[ignore]`d green target committed
RED at f5e49bc8; `f0087_cut7_stays_loud_offsurface_wall` is the pinned
boundary whose retire signal converts it to a positive regression), the
F0086–F0090 corpus family, full yang-rs/kernel-v2 suites, corpus P9 gate
(0 WRONG, zero CORRECT lost).

**Joint region relocation (amendment 6, 2026-07-07 — M8 increment 9,
task #64):** the measured F0087 cut-9 residual — TWO (or more) interacting
rim mints in one multi-column strip. The plate-rim mint (sagitta ~4.2e-2)
and a hole-rim mint sit at the two ends of the strip of long CDT triangles
joining the outer rim to hole-8's rim; each vertex appears on the OTHER's
cavity polygon, whose long collapsed-spoke edges cross (probe:
`[reloc-ring] edges 0 x 6` for vert 186, `edges 1 x 7` for vert 189 — the
polygons are GENUINELY non-simple, so per-vertex Fig-11 relocation is
exhausted; the amendment-2 revert then leaves chord-position vertices that
escape as `VertexOffSurface`). The 1-ULP twin columns are NOT the cause
(increment 4's shared-mint collapse already handles them — `[mint-collapse]`
fires); the hopped columns are genuine ~5e-3 geometry, so input-level
welding does not apply (A14.2 protects real features).

Amendment 6 relocates the interacting set JOINTLY — the Fig-11
delete-and-reinsert generalized from one vertex's star to the UNION of the
set's stars:

- **Trigger:** a folded emitted triangle whose per-vertex relocations
  (amendment 5) all reject, at least one with an exactly NON-SIMPLE cavity
  polygon. The seed set `S` = the folded triangle's minted vertices ∪ the
  minted vertices found on each non-simple cavity ring (ascending order,
  deduplicated).
- **Region:** the union of `S`'s vertex stars. Oriented boundary = the
  region triangles' edges whose reverse is carried by no region triangle
  (domain-boundary edges qualify by construction); chained head-to-tail
  into exactly ONE closed cycle, else reject.
- **Guards (each rejects → amendment-2 revert, loud):** single class
  across the region (class-boundary edges are then automatically on the
  region boundary — the intersection curve is never re-triangulated
  across); every region-triangle vertex lies ON the boundary cycle (an
  interior vertex — seed or not — would be orphaned by a polygon
  triangulation; measured F0087 cut 9 has none); one cycle only; the
  deduplicated position ring exactly simple and CCW (same rational
  predicates as amendment 5).
- **Re-triangulation:** the boundary cycle is re-triangulated by the SAME
  constrained exact ear-clip as amendment 5 (ears exact-CCW, gate-valid,
  empty, NEW diagonal; bit-degenerate ears clip freely). A triangulated
  simple polygon with no interior vertices has exactly `m − 2` triangles
  (`m` = deduplicated cycle length), so the replacement count equals the
  region size and the region's triangle slots are overwritten in place;
  `edge_map` is maintained incrementally (build-then-commit; any reject
  leaves NO mutation).
- Purely combinatorial (coords fixed — same termination contract as
  amendments 4/5: every committed joint relocation replaces ≥1 folded
  triangle with all-valid triangles, folds cannot be created, the folded
  count strictly decreases). Deterministic: ascending seed collection,
  smallest-tail cycle start, first-clippable-ear order (I6).

| Gate case (amendment 6) | Behavior |
|---|---|
| per-vertex relocations reject, ≥1 non-simple ring; joint region passes guards + ear-clip | mints KEPT; region re-triangulated jointly |
| joint region rejects (multi-class / interior vertex / multiple cycles / non-simple / no ear) | amendment-2 revert (unchanged, loud) |
| per-vertex relocation succeeds | unchanged (amendment 5) |

Research basis: [#24] Yang 2025 §4.4.1 Fig 11 (delete-and-reinsert mesh
updating — the region form; `refs/text/yang2025_hybrid_boolean.txt:556-560`).
Oracles: `f0087_cut9_stays_loud_offsurface_wall` (pinned boundary; its
retire signal converts it to a positive regression) + the `#[ignore]`d
`f0087_engine_frame_full_ten_hole_chain` green target; stage0 unit tests
on the joint fn (interlocking-pair fixture, reject-no-mutation, mutation
checks); F0086–F0090 family; full yang-rs/kernel-v2 suites; corpus P9
gate (0 WRONG, zero CORRECT lost).

**Class-partitioned joint region relocation (amendment 7, 2026-07-07 — M8
increment 10, task #67):** the measured F0089/F0090 residual. Probe census
(2026-07-07, `YANG_SPLIT_PROBE` single-case runs): F0089's ONE remaining
error and the bulk of F0090's 18 all die at `[reloc-region-reject] …
multi-class region` — a rim mint is minted exactly ON the intersection
curve (that is what a rim crossing IS), so the seeds' star union straddles
the class boundary and amendment 6's single-class guard rejects the whole
region (F0089 cut 11, seeds `[334…391]`, 13 seeds; F0090 repeats the same
strip once per chained cut). The amendment-2 revert then leaks
chord-position vertices → `VertexOffSurface` (F0089: FaceId(123); F0090:
18×).

Amendment 7 partitions the star-union region BY CLASS and relocates each
class sub-region independently — the intersection curve is never
re-triangulated across, by construction:

- **Partition:** region triangles grouped by `RegionClass` (deterministic:
  ascending class order). Each sub-region is attempted separately.
- **Folded-triangle gate (termination):** only sub-regions containing at
  least one FOLDED triangle (2D signed area ≤ 0, not 3D-bit-degenerate)
  are attempted; a committed sub-region therefore strictly decreases the
  gate's folded count (its replacement ears are gate-valid by
  construction), preserving the amendment-4/5/6 termination contract. A
  valid-only sub-region is SKIPPED (re-triangulating it could churn
  without progress).
- **Boundary:** each sub-region's oriented boundary is built exactly as
  amendment 6 — edges whose reverse no sub-region triangle carries. A
  class-boundary edge qualifies automatically (its reverse lives in the
  OTHER class's triangle, outside this sub-region), so the intersection
  curve becomes sub-region boundary and survives the re-triangulation
  verbatim.
- **Guards per sub-region (each rejects that sub-region only, loud):**
  single closed cycle; every sub-region-triangle vertex on the cycle (no
  interior vertex); deduplicated position ring exactly simple + CCW (same
  rational predicates); shared constrained exact ear-clip
  (`earclip_cavity_polygon`, unchanged). Build-then-commit per sub-region:
  a rejecting sub-region leaves NO mutation of its own; other sub-regions'
  commits stand (each is independently valid and fold-reducing).
- **Result:** the joint relocation reports success iff ≥1 folded
  sub-region committed. If the triggering triangle's own sub-region
  rejected, the gate loop re-scans (folded count decreased elsewhere) and
  the surviving fold falls through per amendments 5/6 → amendment-2
  revert, still loud — nothing is silently blessed.

| Gate case (amendment 7) | Behavior |
|---|---|
| joint region multi-class; ≥1 folded class sub-region passes guards + ear-clip | mints KEPT in committed sub-regions; intersection-curve edges preserved as sub-region boundary |
| every folded class sub-region rejects (interior vertex / multiple cycles / non-simple / no ear) | amendment-2 revert (unchanged, loud) |
| single-class region | unchanged (amendment 6 — the partition is the identity) |

**Region growth to simplicity (amendment 8, 2026-07-07 — M8 increment 11,
task #68):** the measured F0090 residual after amendment 7. Probe census
(`probe2_F0090`, new binary): 22 joint relocations now COMMIT, but the
dominant remaining reject is `class AOnly region polygon not simple` — the
folded sub-region is a long femto-strip whose boundary is a BOW-TIE under
the minted positions (e.g. seeds `[183,189,190,195,196]`: the two long
sides of the strip cross, `[reloc-ring] edges 0 × 4`). The region form
froze the region at the seeds' star union; the per-vertex form (amendment
5) has constrained visibility GROWTH for exactly this situation, the
region form had none.

Amendment 8 grows the sub-region across a crossing edge until its boundary
is exactly simple:

- **Trigger:** the sub-region's boundary cycle exists (single closed
  cycle, no interior vertex) but the position ring has an exact proper
  crossing / interior endpoint touch (the `EarclipErr::NotSimple` class;
  pinches — repeated non-adjacent positions — stay terminal rejects,
  unchanged).
- **Growth step (deterministic):** take the FIRST crossing pair in
  boundary order; try its two mesh edges in order. An edge is growable iff
  it has exactly ONE incident triangle outside the region (single-incidence
  = domain boundary — uncrossable), that triangle's class equals the
  sub-region class (a class-boundary edge IS the intersection curve —
  never crossed), and its apex does not already lie on the boundary cycle
  (a repeat would pinch the ring). The external triangle joins the region;
  the boundary cycle and no-interior-vertex guard are recomputed; repeat.
- **Reject (loud, no mutation):** neither edge of the crossing growable,
  or a recomputed guard fails. The amendment-2 revert stays the fallback.
- **Termination:** the region strictly grows and is bounded by the class
  component; every committed relocation still replaces ≥1 folded triangle
  with gate-valid ears (the amendment-4/5/6/7 contract — absorbing VALID
  triangles into a fold-carrying region is exactly what amendment 5's
  visibility growth already does per-vertex).

| Gate case (amendment 8) | Behavior |
|---|---|
| sub-region polygon non-simple; growth across crossing edges reaches a simple ring | mints KEPT; grown region re-triangulated by the shared ear-clip |
| crossing edge ungrowable both sides (domain/class boundary, apex pinch) or guard fails after growth | that sub-region rejects (loud); amendment-2 revert unchanged |
| sub-region polygon already simple | unchanged (amendment 7) |

**Connected-component split (amendment 9, 2026-07-07 — M8 increment 12,
task #69):** the last measured F0090 fold-gate revert (timestamped probe,
t≈131s, ~cut 22): a 33-seed joint trigger whose class sub-region rejects
`region boundary is not a single closed cycle`. The seeds accumulate from
MANY non-simple per-vertex rings across one folded triangle's attempt, and
their star union's class sub-region is DISCONNECTED — several separate
strips. One boundary walk cannot cover two components; the whole
sub-region rejected wholesale.

Amendment 9 splits each class sub-region into edge-connected components
(triangles connected through shared edges; deterministic ascending-index
BFS) before the boundary build. Each component is attempted independently
under the amendment-7 folded-triangle gate (a fold-free component is
skipped — termination unchanged) and the amendment-8 growth loop (growth
stays inside the component's class; an absorbed triangle joins that
component). A genuinely ANNULAR component — one component, multiple
boundary cycles — still rejects loud (`region boundary is not a single
closed cycle`).

**Post-ship measurement (same day):** the F0090 33-seed corpus site
survived the split — its sub-region is CONNECTED and ANNULAR (the probe
re-run still shows the same single revert; vert 151's per-vertex ring
alone has 40+ edges, and the ~30 ring mints inflate the joint region
into a band that encircles a hole). The component split is retained as
required coverage for multi-strip joint triggers (unit-proven; the
disconnected shape is reachable whenever seeds accumulate from separate
strips), but the F0090 tail is the ANNULAR class — next lever, measure
the region's cycle structure before designing (candidates: narrow the
amendment-6 seed set to crossing-edge endpoints so the region stays a
strip, vs. a bridge-edge annular ear-clip).

| Gate case (amendment 9) | Behavior |
|---|---|
| class sub-region disconnected; ≥1 folded component passes guards + ear-clip | mints KEPT per committed component |
| a folded component rejects | that component only (loud); others' commits stand |
| connected sub-region | unchanged (amendments 7–8 — the split is the identity) |

**Crossing-endpoint seed narrowing (amendment 10, 2026-07-07 — M8
increment 13, task #70):** the measured F0090 annular tail. The
`[reloc-region-cycles]` probe confirmed the 33-seed site: ONE connected
sub-region with TWO boundary cycles (lengths [32, 20]) — a band
encircling a hole rim (the inner cycle is a tool-rim class boundary:
ungrowable, and filling it would orphan its vertices). Root: the
amendment-6 trigger surfaced EVERY minted vertex on the non-simple
cavity ring as a joint seed; vert 151's 40+-edge ring lists ~30 mints,
and their star union is the annulus.

Amendment 10 narrows the surfaced seeds to the mints ON the crossing
edges — the interacting set. Fig-11 mesh updating is local to the
repositioned vertices' neighborhood; two mints interact exactly when one
appears on the edges that make the other's cavity polygon non-simple
(that is what the crossing IS — increment 9's measured F0087 cut-9
signature). `EarclipErr::NotSimple` now carries the first crossing
pair's endpoint positions (bit-identical frame projections); the
per-vertex caller filters the ring mints by exact position match. The
narrowed seed set keeps the joint region a strip: single boundary cycle,
repairable by the amendment-7/8/9 machinery.

| Gate case (amendment 10) | Behavior |
|---|---|
| non-simple cavity ring, minted verts on the crossing edges | surfaced as joint seeds (unchanged semantics, narrower set) |
| minted verts elsewhere on the ring | NOT seeded — their own folds get their own gate iteration |
| no minted vert on the crossing | `ring_mints` empty → singleton seed set → joint path skipped (amendment-6 trigger unchanged) |

Research basis: [#24] Yang 2025 §4.4.1 Fig 11 (mesh updating is local to
the repositioned vertex's neighborhood). Oracles:
`nonsimple_ring_mints_narrow_to_crossing_endpoints` (RED→GREEN: a ring
mint off the crossing is excluded); the F0087 cut-9 / F0089 cut-11 /
F0090 cut-7 positive regressions (the interacting-pair semantics are
preserved — all 13 chain pins green); the F0090 probe re-run (annular
reject gone / revert count 0 in the container window); full
yang-rs/kernel-v2 suites.

Research basis: [#24] Yang 2025 §4.4.1 Fig 11 — mesh updating is local to
each repositioned vertex's neighborhood; disconnected neighborhoods are
independent Fig-11 instances. Oracles: stage0 unit tests (two disjoint
folded stars under one seed set: both commit; one-component identity
regression), the F0090 probe re-run (fold-revert count 1 → 0 inside the
container's 300s window), F0086–F0090 family chain suite, full
yang-rs/kernel-v2 suites.

Research basis: [#24] Yang 2025 §4.4.1 Fig 11 (`refs/text/
yang2025_hybrid_boolean.txt:556-560`); the growth rule is the region-form
of amendment 5's constrained visibility growth (Bowyer–Watson cavity
carving, deferring at constraints). Oracles:
`f0090_cut7_stays_loud_offsurface_wall` (pinned boundary; retire signal →
positive regression) + the `#[ignore]`d `f0090_engine_frame_seven_hole_
chain` green target; stage0 unit tests (bow-tie growth commit,
ungrowable reject-no-mutation); F0086–F0090 family; full yang-rs/kernel-v2
suites; corpus P9 gate (0 WRONG, zero CORRECT lost).

Research basis: [#24] Yang 2025 §4.4.1 Fig 11 (delete-and-reinsert mesh
updating; the constraint that intersection-curve segments are preserved
during updating is §4.4.1's own requirement — partitioning at the class
boundary is the region form of "never cross a constraint edge" that
amendments 4/5 already enforce edge-wise). Oracles:
`f0089_cut11_stays_loud_offsurface_wall` (pinned boundary; retire signal
converts it to a positive regression) + the `#[ignore]`d
`f0089_engine_frame_eleven_hole_chain` green target; stage0 unit tests on
the partitioned fn; F0086–F0090 family; full yang-rs/kernel-v2 suites;
corpus P9 gate (0 WRONG, zero CORRECT lost).

### 3b. CONTINGENT part 2 — Stage-4 Fig-11(b) junction-cluster merge

Implement ONLY if, after part 1 is green at Stage-0, the acceptance oracle I1
still fails on a residual cluster (measure; do not build speculatively — P10).
Definition retained from the original spec: for each relocated
LineSegment-curve endpoint q, mesh-adjacent non-endpoint vertices within the
derived band (`LineReloc.band_budget + d_eps`) merge onto q via
`collapse_vertex` (survivor = q), ascending-(q,p) deterministic order;
ambiguous double-ownership → loud `LocalRefinementRequired`.

## 4. Invariants (measurable — unchanged; the committed red tests assert these)

- **I1 (on-surface output):** every output loop vertex attributed to a face
  lies on that face's `Surface` within the kernel import band
  `1e-9·(1+max(r,‖p‖∞))`.
- **I2 (exact junction survives):** the rim∩plane junction vertex at
  `(R−δ, +y*, 0)` remains exact (distance 0.0) — the circle∩line branch, not
  radial projection, must handle crossings.
- **I3 (watertight preserved):** watertight + Euler χ=2 + plausible volume.
- **I4 (locality / no-op):** non-Stage-0 pipelines byte-identical (fixture
  I4 does not traverse Stage-0); straight-edge and interior overlay points
  unchanged.
- **I6 (determinism):** repeat runs byte-identical.

## 5. Oracles (committed at ff3763e7, `crates/yang-rs/tests/n2_junction_cluster.rs` + `crates/test-harness/tests/n2_junction_cluster_campaign.rs`)

- `i1_cylinder_face_loop_vertices_on_surface` — RED today (11 off-band, worst
  6.200744e-6 vs band 1.000213e-9).
- `i2_exact_junction_vertex_survives`, `pins_watertight_euler_volume`,
  `i4_locality_noncoplanar_tangent_all_on_surface`, `i6_determinism` — GREEN
  pins.
- `red_r0072_vertex_off_surface` (`#[ignore]`, campaign replay) — RED today.
- R0096 verdict: DIFFERENT mode (torus×torus v1-scope STOP) — documented, no
  test.
- **Blast-radius regression (from the trace):** R0013/R0024 (now-green
  same-normal), disc∩polygon crossing suites (PR-M8 classes), non-convex
  containment, gear-flange chain, full assay: **0 SUPPORTED_WRONG, no
  SUPPORTED_CORRECT lost.**

## 6. Failure modes

- Circle∩line with no real intersection for a claimed crossing point (exact
  discriminant < 0) → loud `CoplanarOverlayError`/Stage-0 error naming the
  vertex (never fall back to the chord position silently).
- Anything downstream still off-band → the kernel tripwire stays (untouched);
  the campaign test keeps it observable.

## 7. Research basis

- **#24 Yang et al. 2025 §4.5.5** — overlap boundaries become intersection
  curves; the shared trimmed surface carries exact curve geometry and BOTH
  models receive identical meshes (`refs/text/yang2025_hybrid_boolean.txt`,
  §4.5.5). Minting shared-boundary points on the exact circle is that
  requirement; the chord-position mint was the deviation.
- **Precedent in-file:** the opposite-rim exact-radius projection
  (stage0.rs:1048-1055) — the same operation this spec extends to the own-cap
  rim.
- **Fig 11(b)** (part 2, contingent) — merge p with q, the on-curve point
  (`refs/text/yang2025_hybrid_boolean.txt:556-560`).

### 7a. Analytical vs approximate

Exact: radial projection and circle∩line are closed-form on the analytic
`Curve::Circle`; classification uses exact rational predicates. No SSI; A15
N/A.

## 8. Scope / non-goals

- No re-mesh wiring (deferred with cause, §0 item 1).
- No change to `build_intersection_curves` same-input semantics (correct per
  paper), to kernel-v2's tripwire, or to non-Stage-0 paths.
- R0021's re-entry wall and render-triangle blemish: different milestones.
- Part 2 (§3b) only on measured need.
