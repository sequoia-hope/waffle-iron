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
| T1 relocation did not converge in-domain | §4.5 `:646-648`; §4.3 `:528-532` ("no solution in one of the two parametric domains … a solving failure") | `YangError::Stage4RegionInvalid { vertex, reason, under_resolution }` (`LocalRefinementRequired`, `OffCurveBeyondChordBand`, `RelocationCrossedCarrierVertex`); **since 2026-10-15 also Stage 3's `SsiRefinementFailed`** — a mesh intersection edge with no SSI curve through it, the paper's Case-IV solving failure (`failures::note_ssi_failure`, pending, read only when the run ends in that error on that edge; measured F0088 with the phantom boost off) | the faces of the vertex's live triangles on both operands (today the error carries the vertex only — it gains `incident: Vec<(InputId, u32)>`, the `DomainFire` form); for Stage 3, the faces of the triangles on the failing edge |
| T2 boundary point left its face across a crease | §4.5 / Fig. 13 (`:683-700`) | `DomainFire` records (`boundary_domain_fire_records`) | `fire.incident` |
| T3 illegal intersection in the output | §4.5.4 `:752-758` | `cherchi_rs::detect_improper_contacts` on the emitted mesh (`output_improper_count`) | the two faces of each improper pair, via the triangle attribution |
| **T3 — RE-SEQUENCED 2026-10-14 (measured).** With the tree broad phase every improper pair on the CORRECT outputs R0019 (217) and F0090 (1–2 per op) is a TRANSVERSAL crossing, never a coplanar touch: cross-face pairs (plane cap × cylinder / cone chord; R0019 ×197, every F0090 pair) are the Stage-4 mesh's own chords crossing beside a relocated intersection curve, and same-face pairs (R0019 ×20: one relocated vertex, tri `[648,134,178]`, pulled across its own fan) are FOLDS the mesh takes because §4.4.1 never retriangulates after relocation. The outputs are correct because the B-Rep emission does not rest on the mesh's local validity. So the paper's §4.5.4 detection presupposes §4.4.1's retriangulation — on a retriangulated mesh a surviving illegal intersection IS a resolution failure; on ours it is mostly the N2 gap. The project measured the same fact once before and then forgot it: `specs/yang_173_selfx_detector.md` §6 (2026-07-17) found the exact Stage-4 mesh test firing on relocation-minted seam chord crossings on 53 cases, 33 of them CORRECT, and built kernel-v2's RENDER-resolution gate (`validate/selfx.rs`) as the output-level layer for the C0116 class instead; this spec is where that two-layer finding becomes the loop's rule. Consequences: (a) the mesh-chord census stays a RECORD (`illegal_intersection_sites`) and is NOT a loop trigger until Phase 2 lands; (b) the class the arms protected (P0057's lens, the wedge trio: two faces' exact boundary curves crossing on ONE surface, seen one crate later as kernel-v2's `SelfIntersectingBooleanOutput`) gets an OUTPUT-level §4.5.4 check, **T3b**: for every output surface carried by ≥ 2 faces, the exact boundary curves of distinct faces are intersected pairwise (conic × conic on the same quadric, line × conic — ssi-rs / `boolean::predicates` vocabulary; no mesh); a crossing strictly inside both edges' parameter spans is an illegal intersection naming exactly the two faces. Cheap (a few faces per surface), exact, and it names the paper's "intersections that may arise during mesh updating" at the level the output actually depends on. T3b is Phase 1c's trigger beside T1/T2/T4/T5; T3 (mesh) joins when Phase 2 has made the mesh valid. **T3b closed form:** every boundary curve of a face on a quadric that is not a `SurfacePair` is a PLANAR section (circle, ellipse, line, hyperbola, parabola — each carries its plane); two such curves of two distinct faces on one quadric cross exactly where their two planes' common line meets the quadric — a line × quadric root pair, the §4.5.1 corner-transit vocabulary already in `stage4_phantom::segment_surface_roots` — and a root is an illegal intersection iff it lies strictly inside both edges' parameter spans (exact parameter tests, the §4.5.3 overtake arm's `conic_param` discipline). Coincident curves (`curves_coincide`) are a shared edge, not a crossing. `SurfacePair` curves are skipped in 1c (named gap; Newton on two implicit curves is Phase 2+). | | | |
| T4 non-manifold output | §4.4.3 `:596-600` (the paper's output is 2-manifold by construction; a non-manifold emission is a mesh-updating failure the paper's loop would have refined) | `NonManifoldOutput`, unpaired-edge census (`refine_452_unpaired`); **since 2026-10-15 also the gate's Euler condition** (`failures::euler_sites`: a shell whose χ no closed orientable surface has — its edges all paired, so the defect is an over-covered edge; over-covered edges are legal at a tangency seam of a VALID shell, so only an impossible shell's count — measured P0029, a rim chord lying in a cap plane) | the faces of the triangles at the unpaired edges; for the Euler condition, at the impossible shell's over-covered edges |
| T5 Stage-6 reassembly STOP (deadend, loop simplicity, face resolution) | same as T4 | typed Stage-5/6 errors | **as built 2026-10-15:** every `NonManifoldOutput` raise (`stage4_relocate::non_manifold_at`) records a PENDING site naming the faces of the enclosing `failures::NonManifoldScope` — `patch_boundary_cycle` and Stage 6's per-patch emission open one on the patch's input face; `failures(result)` reads the pending site only when the run ENDS in `NonManifoldOutput` (several callers recover from a walk error), and a raise outside every scope still names nothing (measured P0017 / P0020 / P0053 / P0055) |

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

**T6 — the trigger the arms approximate (found 2026-10-14 by the first
arm measurement, §7).** With the whole §4.3.3 edge-graze cluster off
(`YANG_433_GUARD=0`: wedge, pierce dip, segment-face graze, edge graze) and
the loop on, the corpus moved exactly THREE rows: P0044 and P0047
CORRECT → ERROR (kernel-v2 refusals one crate later — `bounded cylinder
patch must have exactly one material-CCW loop`, `ring rejected by CDT`),
and R0023 UNSUPPORTED → "CORRECT" — the pre-P0057 FALSE GREEN (its oracles
are watertightness, Euler and volume monotonicity, no exact volume; the
N = 11 mesh swallows the 0.131-deep sliver the dip arm keeps). Every other
case the cluster protected (P0013, P0054, P0057, P0058, P0031 …) is
recovered by the loop. On all three movers the loop never ran: yang
emitted a body with an EMPTY failure set, because the defect of this class
is a mesh that MISSES an intersection — nothing fails in Stages 2–6, the
output is a valid B-Rep of the wrong solid, and only kernel-v2's render
gate (or no one) notices. The paper's mechanism for this is §4.3: *"If the
two meshes do not intersect but are within the distance tolerance d_ε,
there is a tangent point, or a small loop, or no intersection between the
surfaces. We use Newton's method to solve such cases"* (`:525-530`) — a
NEAR-MISS check after the arrangement, not a prediction before it. T6:
for every cross-operand triangle pair within d_ε that the arrangement left
`Disjoint`, ask the two faces' exact surfaces (closed form, N7's
substitution for the paper's Newton — `ssi_rs::intersect`) whether their
intersection curve passes within d_ε of the pair inside both faces'
domains; if it does, the mesh missed a real intersection there — a site
naming both faces, which the loop refines. T6 is Phase 1f; it is the ONE
mechanism the nine Stage-1 arms stand in for, and their retirement (§7)
waits on it.

**T6 as built (2026-10-14, Phase 1f).** Three pieces.

1. `shell_contact::contact_pairs` — the P0030 shell census's machinery
   (`TriSet`, `Bvh`, exact-surface pieces) over every A×B triangle pair whose
   COARSE triangles are disjoint (an intersecting coarse pair is the
   arrangement's own population: Cherchi inserts its tri×tri segment as a
   constraint, an intersection edge), pairs of identical surfaces skipped
   (the §4.5.5 and coincident-cylinder Stage-0 populations). For each,
   `pieces_cross_point` refines the exact pieces — the larger deviation
   first, then, once both deviations are within the band, the larger PIECE
   (planes split too) until the two are within 2× in size — and reports the
   first leaf pair that is a RESOLVABLE CROSSING: the leaf triangles
   intersect, and each leaf's vertices reach more than the band into AND out
   of the other piece's EXACT surface (`geom::signed_distance_to_surface`,
   the cone's radial residual × cos α).
2. `failures::joined_triangle_pairs(la)` — the arrangement's record of
   where the meshes met: for every `LabeledArrangement::intersection_edges`
   entry, the A-parent × B-parent input triangles of the output triangles
   incident to it (`la.source`).
3. `failures::missed_intersection_sites` — a crossing the arrangement joined
   NOWHERE in its two triangles' one-rings is a missed intersection naming
   both faces, one site per face pair. Topological, as the paper's test is
   ("the two meshes do not intersect").

The capture test runs FIRST: `failures::RingCapture` is built once per run
and handed to `contact_pairs` as its skip predicate, so a candidate the
arrangement already joined in its one-rings is never refined. Recorded in
`boolean_once` right after the arrangement (`record_missed_intersections`);
the loop reads it like every trigger.

*The paper's near-miss population is three things* — "a tangent point, or
a small loop, or no intersection" (`:525-530`). Only a crossing deeper than
the model resolution is one refinement can make the meshes meet; the rest
is the §4.3.3 tangent-point mint's business, the A14.2 feature floor's, or
nothing. Every rule above was measured into place on 2026-10-14:

| draft | what it did | measured on | outcome |
|---|---|---|---|
| metric capture | missed = no intersection edge within d_ε of the contact | C0058 (`tests/tangency_pinch_split.rs`) | fired on two TRUE crossings 1.07 d_ε / 1.38 d_ε from their edges (the facet-line offset is (dev_A + dev_B) / sin θ); the loop's ×2 body left the pin's volume band → **one-ring topological capture** |
| circumradius bound | piece deviation = osculating-sphere sagitta over the CIRCUMRADIUS | R0016 (gear revolve, 5 cm) | a sliver's bound read **6.3**; every gear-boss triangle visited all 440 revolve triangles and refined ~90 of them 16 levels — 300 s CPU without finishing one census; four gear cases hit 900 s → **exact `surface_deviation`** per surface (below); R0016's census now 0.064 s |
| all intersecting pairs | the census refined coarse pairs that already intersect | C0058 | 38 s → 5.8 s once skipped (0.1 s with the exact bound) |
| within-band contact | a leaf pair within the band was a contact | R0015 (1e-4 model) | seven planar×planar sites 2e-7 apart (reach = band exactly) took its loop to the ceiling → **leaf pair must intersect** |
| capture after search | the one-ring capture ran AFTER the exact crossing search | F0055 (1e4 model) | 21 pairs BESIDE a real crossing — pieces within reach along the curve, never crossing themselves — refined to the depth limit: 112 s; R0085 went past its 900 s budget → **capture first** (F0055's census 0.000 s, all 21 skipped) |
| plane-extension depth | crossing depth read against the other leaf's infinite PLANE | `n2_rim_mint_adversary` δ = 1e-4·r | a box cap crossing a cylinder over a lens 2.13e-8 deep (band 2e-7) passed through a large cap triangle's far corners; the loop adopted a ×8 body with a vertex 2.13e-8 off its surface where the natural body was valid → **size-matched leaves, depth against the exact surface** |

`surface_deviation` (the census's per-piece bound, also the P0030
passthrough's) reads each surface's own distance field: plane 0; sphere
by the point of the triangle nearest the centre; cylinder by the triangle's
least distance to the axis (closed form per edge, R when the axis pierces
it) — those three exact; cone and outer torus (the whole triangle at ρ ≥ R,
checked by its least distance to the axis since ρ is convex — a vertex
check is not enough) by a TANGENT-PLANE bound on a concave maximum: for any
point p, `max_t g ≤ g(p) + max_j ∇g(p)·(t_j − p)`, least over seven interior
points — exact where ∇g vanishes, within 3× of the maximum on the pin's
fixtures; the inner torus by a Lipschitz-covered sample grid. A nested
ternary search found the cone / torus maximum exactly but at ~5,000
evaluations per piece: R0085's gear × 289° tube census took 45.8 s (88k
refined pairs); the bound takes it to 19.5 s. Pinned against a 400-step
brute oracle on seven inscribed triangles including a cylinder sliver
(circumradius > 100 × deviation) and a cone generator sliver
(`shell_contact::tests::surface_deviation_is_exact_on_inscribed_triangles`).

*A T4 extension tried and refuted (same day).* Under the within-band
draft, `n2_rim_mint_adversary::exact_tangency_and_just_past` had its loop's
×4 run join a cylinder and a box along their shared tangent generator — an
edge with four incident triangles, directions balanced 2 + 2 — and ADOPT a
χ = 3 body. An emitted-body census of overloaded edges and pinched vertices
(plus a `NonManifoldOutput` gate on a standing natural body) caught it, and
regressed F0060 (equal-radius cylinders at 90°, CORRECT) with 446 sites:
yang's internal MESH is legitimately edge-doubled along a tangency seam —
the P10 record in `check_watertight_2manifold` — and the 2-manifold
contract is the B-REP's, which kernel-v2 builds from faces and loops. The
census and gate were removed. Under the final crossing test the tangency
fixture records no T6 site (its contact is depth 0), so nothing reaches
that adoption.

*The arms-off run (same day).* With `YANG_433_GUARD=0` the corpus reads
349C / 0W / 20E: P0044 and P0047 fail exactly as in the morning's
measurement and T6 refines nothing on their failing subtracts (every
candidate coarse-rejected, coarse-intersecting, or captured). The class the
§4.3.3 arm cluster protects is therefore NOT a missed intersection but a
mis-joined one — the meshes met, Stages 4–6 emitted, and only kernel-v2's
patch / ring checks reject the region. T6 is the mechanism for P0021-type
lenses and P0049; the cluster's own retirement waits on a trigger that
sees an invalid output region inside yang-rs.

*R0023 (open).* The 0.131-deep sliver P0057 keeps is a real crossing T6
names on R0023's second op; the loop's ×4 body carries each cap's sliver as
TWO coplanar circular-segment faces on the r = 150.39 rim (chord 12.56,
sagitta 0.131 — one split at the arc's midpoint, one at 3.736 / 8.830)
where the natural body had one, and kernel-v2's planar tessellator samples
one of them to its two chord endpoints and refuses it ("outer loop has
fewer than 3 vertices"). R0023 moves from its honest UNSUPPORTED (the
third op's coplanar wall) to a loud ERROR one op earlier. A first reading —
a straight-edged zero-area two-gon from yang — was refuted by
`[face-loop]` printing the faces; the census written for it was removed.
Open: whether the two segment faces per cap are a correct split (two
slivers) or a duplicated region, and why kernel-v2's arc sampling drops a
4.8° arc's interior points.

The pins (`tests_unit/s452_missed_intersection.rs`, six): a sub-sagitta
graze (B's axis along 15°, half a segment, so the chord polygons present
midpoints to each other) is a lateral × lateral crossing; clearly separated
cylinders have none; an arrangement that joined nothing ⇒ the lateral pair
is missed; joined at the contact pairs, or one vertex-neighbour over on
each side ⇒ nothing missed, joined only far away ⇒ missed; two crossing
cylinders through `boolean()` record no T6; on the crossing pair every
intersecting coarse pair is in `joined_triangle_pairs` and absent from the
census. Beside them, kernel-v2's `p0021_case_iii_finite_extent.rs`
default-path pin asserts the FUSED body: with the N75 extent arm off, T6
names P0021's razor lens and the loop fuses the pair — the first
arm-protected case recovered through the paper's trigger. Probes:
`YANG_T6_PROBE=1` (every candidate pair and every missed site with ring
sizes), `YANG_T6_COST_PROBE=1` (one census summary per run: pairs visited,
box / coarse / intersecting rejections, refined, contacts). The assay
runner nulls a child's stderr — read probes through `WAFFLE_PATH=…
user_case_probe replay_waffle_env`.

**T3c — the output-level §4.5.4 check WITHIN one face (2026-10-14).**
The arms-off measurement above left P0044 and P0047 failing with yang's
failure set EMPTY: their meshes met, Stages 4–6 emitted, and only kernel-v2
rejected the B-Rep. Anchored with the kernel-v2 probes (`KV2_CYLPATCH_PROBE`,
`KV2_RING_PROVENANCE`): P0047's emitted prism side carries the cut cylinder's
ellipse arc, which bulges past the face's own straight edge and crosses
back (the edge pierces the cylinder twice, below the chord sagitta) — a
self-intersecting loop; P0044's emitted cut-cylinder patch has one loop that
winds against its material side. Both are the paper's §4.5.4 case —
"intersections that may arise during mesh updating can lead to topological
inconsistencies between the mesh and the B-Rep model … We detect these
illegal intersections and perform local refinement" (`:752-758`) — inside
ONE face, where T3b only compares two distinct faces on one quadric.

`failures::output_loop_crossings` reads every emitted face's loops on their
EXACT curves (`sample_output_edge`: a straight edge by its endpoints, an
open circle / ellipse edge along its MINOR arc — kernel-v2's own reading,
`from_yang/classify` — a closed conic over its period, a parabola /
hyperbola between its endpoint parameters, sampled to the model band) in the
face's chart — a plane's right-handed frame, a cylinder unrolled to
`(R·θ, h)` (right-handed about the outward radial normal; `reversed` flips
the sense). Two rules: the outer loop encloses positive oriented area and
every hole negative; no two non-adjacent segments cross (each straddles the
other's line by more than the model band). A site names the face's input
lineage and the lineage of the faces across the offending edges. Named gaps:
faces with a `SurfacePair` edge, cylinder faces with a loop that wraps the
axis, cone / sphere / torus charts. Trigger: `IllegalIntersection` (T3b's).
Output loops are CCW viewed from outside, as `BRepFace::outer_loop` states —
measured on three CORRECT outputs; the Stage-1-style `rj_box` fixture walks
CW and is not an output fixture. Pins `tests_unit/s454_output_loop_crossings.rs`
(four). Probe `YANG_T3C_PROBE=1` (each face's samples, oriented area, sense,
and skip reason).

*Surface-pair edges (2026-10-15).* The `SurfacePair` gap closed: a pair
edge is sampled by chord bisection, each midpoint Newton-projected onto
both surfaces (`relocate_onto_implicit_pair`); a projection that moves at
least the chord's length, or a sag that never falls within 16 bisections
(two branches side by side — each midpoint lands on the nearer one, half
the gap away), means the edge's ends lie on different branches: a site.
Kernel-v2 refuses exactly these ("left the chord neighborhood", "depth cap
exceeded"). A failed projection (tangency, an axis, non-convergence) is
undecided — the face is skipped, never a site. Anchor P0034 with the
cylinder-pair arm off. Pins: a one-branch edge samples onto both
surfaces; a chord bridging the two loops of a narrow cylinder through a
wide one is invalid (asymmetric ends: at a mirror-symmetric midpoint both
normals lose their x-component and the Newton cannot leave that plane).

*Every face's edges, and the projection floor (2026-10-15).* A face with no
chart yet (cone / sphere / torus) still has its EDGES read — the edge-
validity half of T3c needs no chart; only the crossing / winding half does.
Anchor R0032 with the rim × plane arm off: kernel-v2 refused a torus × cone
pair edge that T3c had never read. The refusal itself was a floor bug in
BOTH crates' projectors: the converged point oscillated at 4.0e-13 on the
cone residual against `8·ε·|seed|` = 3.0e-13, while the residual is
measured from the apex 8.4e3 away; the floor is now `8·ε·max(|x|,
|anchor|)` per residual (`stage4_relocate::surface_anchor_magnitude`,
kernel-v2 `surface_pair_project_tau`) — the scale kernel-v2's strict
on-surface checks already read. With it every pair edge on R0032 reads
decided, and an undecided read stays a skip.

*The rim × plane retry pre-empted the loop (RETIRED 2026-10-15).* With the arms off P0044 still
failed: `boolean()`'s §4.5.4 rim × plane graze retry (a Stage-1 arm,
cleanup §2a) re-ran the op and RETURNED its body whenever its mesh
self-intersection count did not grow — a body never scanned by the failure
set, and the one kernel-v2 rejected (four T3b / T3c sites). The retry's own
stated purpose is never to adopt illegal geometry, so its adoption now also
requires `failures::output_illegal_sites` (T3b + T3c) to be empty; otherwise
it falls through to the loop, which starts from the natural run's sites.
P0044 then converts at the loop's ×4 run; R0044 and P0029, which convert
THROUGH the retry, keep a clean retry body and are unchanged.

**Faceless STOPs inside the loop (2026-10-15).** A REFINED run that
STOPs without naming a face (a Stage-5/6 text STOP) re-refines the
previous iteration's region: the failure persists where the loop was
refining (`:650-652`). Anchor: kernel-v2's two-op P0034 pin with the
cylinder-pair arm retired — ×2 still bridged the pinch, ×4 reassembled
non-manifold naming no face, ×8 is adopted. The NATURAL run's faceless
STOP still ends the loop (no region exists yet). Corpus-neutral.

**T3 from the §4.4.1 update, and T7 (2026-10-17, matrix F-2 / F-3).** A
patch the update refuses because its cycles CROSS in its chart (CDT
`TriangulationFailed`) records a T3 site; T7 `ReversalUnresolved` records a
§4.5.3 reversal the sweep could not remove (the 2·d_ε gate's survivors at
the fixed point, a removal that dropped nothing). Both name the faces at the
vertices of the curve concerned — every face C_p traverses, on BOTH
operands: naming the refused patch's own face alone left the partner mesh
unrefined and R0011 / R0074 returned the same four sites every iteration
(triage §"2026-10-17 (F-2, F-3)").

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
multiplier `k` (a power of two, ceiling `FACE_REFINE_MAX` — 8, the old d_ε/64 as a DENSITY, corrected from 64 in Phase 1c), and a face
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

**Phase 1c design (2026-10-14), the rule the code implements:**

```
boolean(a, b, op):
    natural = run(a, b)                           # arms still inside `run` until each retires
    F = failures(natural) ∪ T3b(natural)
    if F.is_empty(): return natural
    (ra, rb) = (a, b); best_watertight = None; k = 0
    loop (k < ITER_CAP = 8):
        (Ra, Rb) = region(F, ra, rb)              # faces + one ring, per operand
        if Ra, Rb both empty:  break              # a faceless STOP: nothing to refine
        if every face of Ra ∪ Rb is at FACE_REFINE_MAX: break
        (ra, rb) = (ra.refined_at(Ra), rb.refined_at(Rb))   # density ×2 (first rung: the certificate's k, §6.1)
        out = run(ra, rb)
        F = failures(out) ∪ T3b(out)
        if F.is_empty(): return out               # ADOPTED
        if out is Ok and no T1/T4/T5 in F and best_watertight is None: best_watertight = out
        k += 1
    # ceiling / cap / faceless: the paper's loop is exhausted
    if natural is Ok: return natural              # fires are inventoried, never a wall (yang_45 §7)
    if let Some(b) = best_watertight: return b    # today's STOP-ladder clause (unpaired == 0)
    return natural (the STOP, with F attached to the error's detail)
```

6.0.1 **Convergence monitor, both paths.** The paper's loop terminates
because "the mesh intersections converge to the spline surface
intersections under refinement" (`:650-652`); a failure SET that comes back
IDENTICAL — the same triggers on the same input faces — after a doubling is
the observable negation of that argument, on a STOP as much as on an
emitted body. The set is compared, never the count: a STOP whose site moves
to the next corner (P0039's tooth corners, one T1 site per rung on changing
faces, converging at d_ε/64) is progress and keeps refining; a minted
tangency (P0026) or a gear's inventoried fires (R0003 ×42, R0070 ×90, spec
`yang_45_boundary_point_domain_certificate` §7) return the same set every
rung. First corpus run of the loop without this rule on STOPs (2026-10-14):
P0026 climbed to the ceiling for 483.4 s (was 3.3 s), P0042 / P0043 for
≈ 38 s each (were 1.8 s), P0061 21 s (was 4.8 s) — all still honest ERRORs,
all paying for nothing. The budget the old domain ladder bounded with `[2,
4]` and the lens with `4·N` samples, and the old STOP ladder with its Q3
strict-decrease clause, is now this one convergence test. **The comparison
is against EVERY earlier signature, not only the previous one** (second loop
corpus, 2026-10-14: P0005's second op carries 13 fires on a gear's teeth
whose set OSCILLATES between two signatures — faces 8 / 308 / 310 appear
and vanish alternately — so a previous-only test never fired and the loop
climbed to the ceiling, density ×64 on ~30 faces, 81.9 s on a CORRECT body
that was 13.3 s). A set that RETURNS is a cycle, the same non-convergence;
the first return is tolerated, the second ends the loop. A set that keeps
CHANGING without emptying (P0005's gear fires wander between teeth) is
bounded by the ceiling: three doublings, density 8, the old d_ε/64. **One plateau is tolerated** (measured
2026-10-14 on kernel-v2's P0031 pin: density ×2 leaves both fires exactly
where they were — the fire's post position is the exact corner, independent
of density — and ×4 clears them, the old d_ε/16); the second consecutive
non-improvement ends the loop. That one tolerated plateau is the old `[2,
4]` floor re-expressed, and the one constant this rule keeps.

**T4 is read on the EMITTED body as well as at the Stage-4 gate** (measured
the same day: the `n2_rim_mint_adversary` ×1e-2 fixture's refined body
passed the gate and emitted 18 unpaired half-edges; the old STOP ladder's
`refine_452_unpaired` read the output mesh, and `failures` now does too).

6.1 **Certificate → density.** A T1 site with `under_resolution = f` (a d_ε
divisor) asks for density `k = next power of two ≥ √f` (sagitta ∝ 1/N²), so
R0085's 18.2 → k = 8 at the first rung (d_ε/64 in the old vocabulary),
P0015's 159 → k = 16. Faces named by a site without a certificate double.
A face already carrying a factor doubles from where it stands.

6.2 **What `run` still contains.** Every Stage-1 arm (cleanup §2a) and the
rim×plane retry pass (`refine_rim_plane`) stay inside `boolean_once` /
before the loop in this increment; they retire one per corpus run in step
§7. The loop REPLACES: `refine_452` (the whole-op STOP ladder),
`refine_452_domain` (the domain ladder) and its local lens
(`domain_fire_local_rim_overrides`, `LOCAL_452_STEP_DIVISORS`,
`LOCAL_452_MAX_SAMPLES_PER_N`, `coaxial_circle_closure_with_arcs` if it has
no other reader), `refine_452_rounds*`, `REFINE_452_ROUNDS`,
`REFINE_452_MAX_FACTOR`, `refine_452_unpaired`, and the knobs
`YANG_452_REFINE`, `YANG_452_LOCAL`, `YANG_452_ROUNDS`
(`YANG_452_PROBE` stays as the loop's probe; `with_refined_chord` stays for
the census ladder of the `YANG_CHORD_REFINE` debug knob).

6.3 **T3b, the output-level §4.5.4 check** (`failures::output_curve_crossings`):
group the output's faces by surface (exact `Surface` equality); for every
pair of DISTINCT faces on one quadric and every pair of their planar
boundary curves (`geom::curve_plane`), the two planes' common line meets
the quadric at ≤ 2 roots (`segment_surface_roots` along the line); a root
is an illegal intersection iff `conic_param` places it strictly inside both
edges' parameter spans (circles / ellipses: angular span from the edge's
start to its end CCW; a full circle: everywhere; hyperbola: the open
parameter between the endpoints; lines and `SurfacePair` curves: skipped,
named gap). Each hit is a `FailureSite { IllegalIntersection, faces = the
two faces' INPUT lineage (`BRep::face_attribution`) }`. Coincident curves
(`curves_coincide`) are a shared edge, not a crossing.

6.4 **Pins.** `tests_unit/s452_refinement_loop.rs`: `density_for_demand`
(1.87 → 2, 18.2 → 8, 159 → 16, NaN → 2); the loop adopts the first F-empty
body and stops at the ceiling; a faceless STOP stands; T3b finds the
crossing of two ellipses of two faces on one cylinder and ignores two
disjoint arcs and a coincident pair. The old `s452_under_resolution_ladder.rs`
(pins of `refine_452_rounds_for`) is rewritten onto `density_for_demand`;
`s452_domain_lens.rs` goes with the lens; kernel-v2's
`s452_domain_lens_corner.rs` keeps its geometry pin (P0031 converts through
the loop) and loses its knob note.

6.5 **Corpus expectation.** Monotone: every case the three remedies
convert today (R0044, C0065, R0050, R0085, P0003, P0015, P0031, P0039,
R0072, R0095 …) converts through the loop; zero WRONG; wall-time recorded.
A case that does not is a gap in the loop to fix, never a reason to keep a
ladder. **Measured 2026-10-14 (run 3, release, 8 jobs, 900 s): 350C / 0W /
18E / 5EE / 0T + 1U over 374, wall 967.6 s (baseline 971.5 s), per-case sum
+0.6 %; every old customer holds and P0046 / P0050 / P0052 / P0062 convert
(all boundary-domain-fire cases the fixed `[2, 4]` and the one-closure lens
could not clear); zero regressions.** Triage §"2026-10-14 (Phase 1c)".

#### 6.0 The ladder and adoption (original design text)

- Rungs: factor 2 per iteration on the CURRENT region (regions accumulate
  across iterations: a face refined at rung 1 and named again at rung 2 is
  at factor 4). Ceiling: density 8 per face = the old `REFINE_452_MAX_FACTOR
  = 64` d_ε divisor expressed as a density (√64; R0085 / P0015 / P0039 all
  converge at or before it) — three doublings. The under-resolution
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
3. ~~The loop replaces the three remedies, lens kept as a first rung for one run (1c) — corpus monotone; then lens deleted (1d) — corpus holds P0031.~~ **1c IMPLEMENTED 2026-10-14** as `boolean::refine_loop` (§6 design verbatim): the two §4.5.2 ladders (`refine_452`, `refine_452_domain`), the apex-centred lens (`domain_fire_local_rim_overrides`, `coaxial_circle_closure_with_arcs`, `LOCAL_452_*`, `RimAngleOverrides::retain_closures`), the rung tables (`refine_452_rounds*`, `REFINE_452_ROUNDS`, `REFINE_452_MAX_FACTOR`), the op-wide rung (`with_refined_chord`, `BRep::retessellated_at_current_d_eps`), the unread half of the domain-fire channel (`BOUNDARY_DOMAIN_FIRES` / `_RECORDS` and their getters) and the knobs `YANG_452_REFINE` / `YANG_452_LOCAL` / `YANG_452_ROUNDS` are DELETED in one step (the lens was not kept for a run: its faces are the region's faces, refined uniformly). The rim×plane retry pass stays before the loop (its own arm step). Pins: `s452_refinement_loop.rs` (T3b ×4), `s452_under_resolution_ladder.rs` rewritten onto `density_for_demand` (×3), `s452_face_refine.rs` on `refined_by`; `s452_chord_refine.rs` and `s452_domain_lens.rs` deleted with their mechanisms. Corpus: see the triage ledger §"2026-10-14 (Phase 1c)".
4. BVH-accelerated `detect_improper_contacts` so T3 can be always-on (1e) — corpus wall-time measured. **LANDED 2026-10-14:** `cherchi_rs::aabb_tree::AabbTree` (the labeling octree's deterministic rules — depth 8, leaf split 16, duplication budget 2, fixed octant order, inclusive overlap — as one shared implementation; `TriOctree` is now a wrapper over it), `overlapping_pairs()` as the census's broad phase, the pre-1e sweep kept under `#[cfg(test)]` as the oracle (`tree_and_sweep_broad_phases_agree`: identical improper and unresolved pair sets on three seeded soups), plus the `#[ignore]`d `broad_phase_cost_census` measurement. **Measured, synthetic (seeded LCG soups, release):** n = 20 000 → tree 0.020 s vs sweep 0.022 s; n = 60 000 → tree 0.067 s vs sweep 0.092 s. The broad phase was NOT what made the scan expensive: on a uniformly random soup both are fast, so on a real output the cost sits in the exact `classify_pair` calls on the box-overlapping second-ring neighbours every closed mesh has (adjacent triangles share an index and are skipped for free; their neighbours' neighbours overlap in box without sharing one). **Measured, real outputs (`YANG_SELFX_COST_PROBE=1`, release, tree broad phase):** F0090's 30 subtracts grow 82 → 11 642 triangles and the census grows 0.001 → 1.50 s per op (≈ 18 s over a 184.6 s case); R0019's 14 674-triangle output costs 0.043 s; R0081's 105 752-triangle union costs 0.537 s. **Cost is no longer the obstacle to an always-on §4.5.4 scan.** The obstacle it uncovered is SEMANTIC: these SUPPORTED_CORRECT outputs carry non-zero improper counts — R0019 reports 217 pairs, R0081's first op 7, several F0090 ops 1–2 — so the raw census cannot be T3's predicate; the pairs' kinds (transversal vs coplanar) and faces (same face vs two faces) are being classified next, and T3 will be defined on the paper's "illegal intersection" (a crossing), not on "any exact contact". The real-output measurement (`YANG_SELFX_COST_PROBE=1` on F0090 / R0019 / R0081) decides; if the exact predicate dominates, the remedy is the paper's own scope — §4.5.4 names "the self-intersections that may occur during surface discretization and the NEW intersections that may arise during MESH UPDATING" (`:752-754`): the first class is each operand's own, already the Stage-1 self-contact guard's; the second arises only where Stage 4 moved vertices, so T3 scans the triangles of faces that carry a RELOCATED vertex (plus one ring) against the whole mesh — a tree query per touched triangle, not a scan of every pair. That is a scoping the paper states, not a predictor gate. **Design (2026-10-14):** the census's broad phase (`cherchi-rs/src/inputcheck.rs`) is a sweep on min-x over per-triangle AABBs, then the exact `classify_pair` on every box-overlapping candidate; on a large output (F0090 / R0019 / R0081, the three cases an always-on scan pushed past the budget) the sweep degenerates — every box whose x-extent reaches the current one is a candidate regardless of y and z. The remedy is a tree broad phase: an AABB tree over the output triangles (the `labeling::octree::TriOctree` shape — deterministic build, fixed octant order, sorted deduped query results), each triangle querying its own box, candidate pairs `(s < t)` collected once, the exact predicate unchanged. Acceptance is twofold: the improper-pair SET is identical to the sweep's on every corpus output (an in-crate oracle runs both on the same meshes), and the three large cases stay inside the budget with the scan always on. The arrangement's own candidate search (`arrangements/intersection_detection.rs::detect_intersecting_pairs`, an O(n²) AABB loop parallelized by rayon under the `parallel` feature) is the same problem one stage earlier and is the reason rayon's CPU exceeds wall on the gear-revolve cases (ledger seed 5, index 37); it is a cost follow-up of its own, outside this spec, bound by reference parity (identical sorted pair list).
5. Arm retirements §7, one per run (1f…).
6. N62 → RESOLVED; N5's "local arms" paragraph struck; the nine arms' specs marked SUPERSEDED by this one.
