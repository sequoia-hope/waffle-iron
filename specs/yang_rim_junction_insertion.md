# Spec: Stage-1 cap-rim junction insertion (§4.3.3 Case IV, N2/F0059 epic increment 2)

> **Status (2026-07-10): SHIPPED — increments 2 AND 3; F0059
> ERROR→SUPPORTED_CORRECT end-to-end.** Slice A (derivation) + slice B
> (wiring, scope-gated to pairs with NO Stage-0 interaction so every
> Stage-0 re-tessellation path stays byte-identical) + increment 3, whose
> FINAL FORM is a **pre-scan exactness certificate**, not the post-scan
> escape sketched below: the corner junctions trip INSERT-TIME detectors
> during the Stage-4 conic scan (measured: the line∩line "out of scope"
> STOP — the corner terminates capA's and capB's trim lines), so the
> certificate must run BEFORE the scan. `exact_junctions` = vertices whose
> inc0 incidence carries ≥3 distinct surfaces with position within
> TAU_WORK of every one; certified vertices are skipped by every map
> insertion (no conic map, no junction map, no `endpoints`), leaving every
> detector/audit unchanged for inexact vertices. F0059's 8 corners certify
> on 4 surfaces each; union completes watertight; the truncated-Steinmetz
> exact-volume green target is un-ignored GREEN
> (`tests/rim_junction_insertion.rs`). The twice-reverted Newton
> triple-junction handler is now UNNECESSARY for this class and its banked
> copy was REMOVED (`yang_stage4_conic_triple_junction.md` remains the
> design record should an INEXACT junction class ever demand relocation).
> Successor to increment 1 (`yang_collapse_membrane_cancellation`,
> SHIPPED) in the N2/F0059 epic.

## Goal

Retire the F0059 cap-face self-intersecting-loop wall (kernel-v2 render CDT
`"ring rejected by CDT"`) and, by the same mechanism, the suspected shared
wall of the TessellationFailed ring-reject family (F0045/R0011 class census
after landing).

**Measured mechanism (F0059, Steinmetz r=0.35, h=0.5 — h/2 < r so the caps
truncate the seam):** each cap disc of operand A intersects operand B's
boundary; the kept cap material is four circular-segment lobes whose corners
are the EXACT rim junction points (rim circle ∩ ∂B, e.g. `(±0.25, ±0.2449)`
in cap frame, `0.2449 = √(r²−(h/2)²)`). The Stage-1 chord-sampled rim
polyline cuts INSIDE the exact circle, so near each corner it crosses the
trim chords: the mesh-level seam chains cannot terminate at the true
junction (it lies outside the sampled cap polygon), the lobes stay
edge-connected through corner-cutting slivers, and the emitted single
boundary loop self-intersects at sub-sagitta scale. S7's §4B split arm
cannot catch this — the junction sits a full sagitta off the crossing chord,
≫ TAU_WORK.

**Paper-faithful fix (Yang §4.3.3 Case IV):** represent the crossing IN the
mesh — force Stage-1 rim samples AT the analytically-derived junction
azimuths, before tessellation. The infrastructure exists end-to-end:

- `stage1_tessellate_with_rim_overrides` (M8 disc-rim crossing) inserts
  exact extra points into a full-circle rim ring, angle-sorted, and routes
  affected laterals through the azimuth-merge strip.
- With the rim passing exactly through the junctions, the seam chains
  terminate there, the four lobes become vertex-pinched patches → the
  KV9-F1 figure-eight wedge walk + pinch split emit them as separate loops.
- Each lobe (chord + arc between the same two vertices) is the SHIPPED
  D-face bigon vocabulary (`dface_bigon_campaign`).
- The inserted vertex lies on the rim circle AND the trim curve(s): the
  line+circle case is the SHIPPED `vert_junction` Stage-4 arm.

## Parameters

- `rim_junction_overrides(a: &BRep, b: &BRep) -> (BTreeMap<u32, Vec<Point3>>, BTreeMap<u32, Vec<Point3>>)`
  — per operand, per full-circle rim edge, the exact junction points of that
  rim circle with the OTHER operand's boundary surfaces.
- New `BRep::rebuilt_with_rim_overrides(map)` mirroring
  `rebuilt_with_min_rim_segments` (both must COMPOSE: the Case-IV phantom
  guard's `min_n_seg` and the override map thread through ONE
  `from_topology` variant).
- Wire in `boolean()` beside the phantom guard, BEFORE Stage 0.

## v1 scope (closed forms only, per A13.3/P8 — no ad-hoc root-finding)

For a rim circle C (center c, unit normal n, radius r, in plane P) of
operand X against each face of operand Y:

| Y face | section of Y's surface in plane P | junction solve | scope |
|---|---|---|---|
| Cylinder lateral, axis d ∥ P (`|n·d| ≤ 1e-12`, the phantom-guard axis floor) | two parallel lines | circle∩line ×2 (quadratic) | v1 (IMPLEMENTED — sufficient for the F0059 class: its corners are triple junctions, so the lateral arm alone finds them) |
| Plane face (normal m) | line P∩plane | circle∩line in-plane (quadratic) | DEFERRED — would re-derive the same triple-junction points as the lateral arm through a different arithmetic path (ULP-twin risk); add only when a case demands a plane-only junction |
| Cylinder lateral, axis transversal | ellipse in P | circle∩ellipse (quartic) | SKIP v1: no insertion, current loud walls preserved |
| Sphere / cone / torus | conic / quartic in P | — | SKIP v1 |

Each candidate point must pass ALL of:
1. **On-arc**: the rim edge is a full circle (v1: partial arcs skipped).
2. **Within Y's face extents**: for a plane face, inside the face's outer
   loop (2D containment); for a cylinder lateral, within its z-extent
   [z_lo, z_hi] along d.
3. **Transversality**: the rim crosses ∂Y at the point (the trim curve is
   not tangent) — |tangent_C × tangent_section| > derived floor; tangent
   grazes are the §4.3.3 tangency class (existing pinch machinery), skipped
   loudly.
4. **Separation from uniform samples**: `stage1_tessellate_with_rim_overrides`
   already errors loudly on an angular tie with a uniform sample; the caller
   does NOT pre-filter (never silently merge).

## Branch table

| condition | action |
|---|---|
| no junction found on any rim (the common case) | maps empty → operands byte-identical (B1 oracle) |
| junctions found on operand X's rims | X rebuilt with overrides; Y likewise independently |
| Stage-0 path re-tessellates a face of a rebuilt operand | overrides MUST thread through (`forced_rim_n` precedent — the M8 incr-15 trap: Stage-0 re-tessellations silently discard a boolean()-entry boost). v1: if a rim WITH overrides enters a Stage-0 re-tessellation path that cannot honor them → loud `CoplanarFacesUnsupported`-style typed stop, never silent divergence |
| quartic-class junction geometry (transversal-axis cyl, sphere, …) | no insertion (documented residue; current loud walls remain) |
| candidate fails transversality | no insertion (tangency class, existing machinery) |

## Invariants

- **I1 (byte-identity off)**: empty override maps ⇒ both operands and the
  whole pipeline byte-identical (existing
  `rim_override_empty_is_byte_identical` covers Stage 1; extend to
  `boolean()` level).
- **I2 (exactness)**: every inserted point satisfies |‖p−c‖−r| ≤ TAU_WORK,
  |(p−c)·n| ≤ TAU_WORK, and lies on Y's surface to TAU_WORK.
- **I3 (chord validity)**: inserting a rim sample only SHRINKS sagittas —
  never a tolerance relaxation (A14.3).
- **I4 (determinism)**: junction enumeration in (face index, root order)
  order; BTreeMap keyed by edge index.

## Oracles

- **Unit**: derivation returns the four exact corners for the F0059 cap
  configuration (r=0.35, h=0.5: `(±h/2, ±√(r²−h²/4))` in cap frame);
  empty for the kv9f1 configuration (h/2 > r: seam never reaches caps);
  empty for disjoint operands.
- **Red→green (epic target)**: truncated-Steinmetz union end-to-end in
  yang-rs — `boolean(a,b,Union)` Ok, watertight, volume =
  `2πr²h − V_common` with
  `V_common = 2·z0·h² + 8(2r³/3 − r²·z0 + z0³/3)`, `z0 = √(r²−h²/4)`
  (piecewise box-truncated bicylinder). `#[ignore]`-pinned until the
  Stage-4 over-determined-junction exactness escape (increment 3) also
  lands; the un-ignored live pin asserts the CURRENT typed wall so drift
  is loud.
- **Assay**: F0059 ERROR→CORRECT is the class oracle; full corpus 0 WRONG,
  no CORRECT lost; ring-reject family (F0045/R0011/R0012/R0016/R0028/
  R0059/R0072/R0098) re-censused after landing.

## Failure modes / residue

- After insertion the corner vertex lies on ≥3 curves (rim circle + trim
  line + seam ellipse) → today's over-determined Stage-4 audits STOP.
  **Increment 3 = exactness-first escape**: an over-determined junction
  vertex ALREADY within TAU_WORK of all incident surfaces is retagged
  without relocation (never a silent pick — the position is verified
  exact); anything else keeps the loud STOP. This likely supersedes wiring
  the banked Newton handler for the F0059 class.
- Quartic-class junctions: documented residue, loud (existing walls).

## Research Basis

- [#24] Yang et al. 2025 §4.3.3 Case IV (intersection through mesh
  vertices/edges — subdivide so the crossing is represented; also the
  tangency-insertion analog), §4.5.5 (shared-boundary sampling).
- M8 increment 15 (`yang_case_iv_phantom_guard`): pair-derived Stage-1
  density + the Stage-0 pass-through trap this spec must honor.
- M8 increment 6 (`m8_increment6_annular_rim_mint`): the rim-override
  insertion vocabulary this reuses.
- [#1] Patrikalakis Ch.5 (plane-quadric sections; circle∩line closed form).

---

# Increment 4 (2026-07-10): plane-face arm + coaxial azimuth propagation + scale-aware certificate — the cone-hyperbola junction class

## Goal

Retire the PR-YR23 cone-hyperbola over-determined-junction STOP for the
6-case Stage-4 `LocalRefinementRequired` sub-family R0004 / R0017 / R0019 /
R0044 / R0047 / R0049.

**Measured mechanism (probe `YANG_RIM_JUNCTION_PROBE`, enriched
per-surface signed distances, all six cases):** every STOP vertex is the
SAME shape — the rim circle shared by two adjacent **coaxial cone bands**
(a lathe/revolve profile stack; both apexes on one axis line) crossing a
**plane face** of the other operand. The mesh junction vertex sits exactly
on the plane (|f| ≤ 1e-13·scale) and equidistant INSIDE both cones by the
rim-chord sagitta (R0017 v43: −10.70 on both cones at model scale ~4000;
R0044 v244: −2.88; R0019 v25: −3.6e-5; R0047 v16: −1.35e-7; R0049 v46:
−1.6e-5; R0004 v81: −1.7e-2 — identical |f| on both cones is the rim-chord
signature). This is the spec's own deferred v1-scope row — "Plane face …
DEFERRED — add only when a case demands a plane-only junction" — and six
cases now demand it.

Secondary measured defect: the increment-3 exactness certificate band is
the ABSOLUTE `TAU_WORK = 1e-12`, which is ~2 ULP at coordinate magnitude
4000 — R0017's ALREADY-EXACT junctions v4/v5 miss certification at
|f| = 1.36e-12 (pure evaluation rounding). The band must be scale-aware or
large-coordinate models can never certify.

## New behavior

> **Scope correction (measured mid-increment, before implementation of the
> affected arms):** the corpus class operands are PARTIAL revolves — their
> rim circles are ARC edges (`start != end`; R0017 gap/r ≈ 1.88), and the
> crossing plane faces include DISC/ANNULUS caps (R0019/R0044 cylinder
> caps, circle-bounded loops). §4a therefore covers arc rims (in-sweep
> filtering, endpoint-duplication guard) and circle-bounded containment;
> §4b propagates per-target-arc in-sweep. Arc chains already accept
> chord-split overrides (the M8-mixed vocabulary), and partial strips pair
> index-wise — kept conformal by the SAME §4b matched insertion the
> M8-mixed caller used, so no §4c analog is needed for arcs.

### 4a. Plane-face arm of `rim_junctions_against` (v1 table row 2, promoted)

**v1 scope: CONE-flanked rims only** (demonstrated need). The measured
class is cone-band lathes; a cylinder-rim × plane-face junction has no
demanding case, and the corpus PROVES that population healthy without
insertion — ungated, the arm regressed four CORRECT cases
(F0047/R0006/R0075/F0081: the inserted rim vertex ULP-twins the
arrangement's own crossing vertex → render-precision sliver in the
output; diagnosis trail banked with the `YANG_TWIN_SCAN` probe) and
unmasked R0091's banked-§3b unverifiable-χ path (micro-scale cut-tool
rims; the sub-resolution-pair containment hypothesis was REFUTED — the
disc-cap fixture has the same relative spacing and is legitimate). The
LATERAL arm (the F0059 cylinder class) is independent and unchanged. If
a case ever demands cylinder-rim plane junctions, re-derive from the
F0047 trail: the fix shape is §4.4.1(b) merge ELIGIBILITY for the
junction populations with a scale-derived ULP band (max(TAU_WORK,
8·ε·L)), never the absolute feature floor (the KV15b micro lesson,
re-proven by `n2_rim_mint_adversary::extreme_magnitudes_valid_or_loud`).

For rim circle C (center c, unit normal n, radius r, plane P) against a
`Surface::Plane { normal m, d }` face of Y (convention `m·p + d = 0`):

- **Parallel skip**: `|1 − (n·m̂)²| ≤ 1e-12`-class floor (same axis floor as
  the lateral arm) → planes parallel or coincident → no transversal line —
  skip (coplanar/tangency classes keep their walls).
- **Section line**: direction `u = normalize(n × m̂)`; point
  `q0 = c + α·(m̂ − (n·m̂)n)` with `α = −(m̂·c + d̂)/(1 − (n·m̂)²)`
  (hat = unit-normalized plane). Then the SAME circle∩line quadratic,
  tangency gate (`2√disc < TAU_MODEL`), and cross-arm TAU_MODEL dedup as
  the lateral arm.
- **Containment (within Y's face extents)**: 2D point-in-polygon in the
  plane face's own frame. v1: faces whose loops are ALL `LineSegment`
  edges (polygonal); any other loop edge type → skip THIS face (no
  insertion, loud walls preserved). Boundary-inclusive band = TAU_WORK
  (keeps triple corners at face edges, mirroring the lateral arm's
  z-extent slack); a point inside an inner loop (hole) is OUTSIDE.

### 4b. Coaxial azimuth propagation

The junction rim flanks two cone bands whose OTHER rims do not carry the
crossing azimuth — Stage-1 band strips (`tessellate_cone_frustum_band`,
cylinder tube) hard-require equal ring counts, so insertion on one rim of
a band stack must propagate:

- **Coaxial group**: full-circle Circle rims of the operand whose axes
  (center + normal line) are the same line (direction cross ≤ 1e-12 floor,
  center-to-line distance ≤ TAU_MODEL).
- For every junction azimuth θ (about the shared axis, one shared
  `ortho_basis` frame) and every group rim R without a point there:
  insert `p' = center_R + r_R·(cosθ·e1 + sinθ·e2)` (exactly on R's
  circle). **Dedup**: skip if R already carries a junction point within
  TAU_MODEL chord distance of `p'` (the arm's existing dedup constant) —
  this makes propagation a no-op for the F0059 class, whose caps BOTH
  derive the same azimuths independently (never insert ULP-twins).
- **Group vocabulary gate**: if any face of the operand touching a group
  rim is not `Cone`/`Cylinder`/`Plane`-surfaced, drop ALL insertions for
  that group (torus/sphere band stacks keep today's loud walls; never a
  half-inserted stack).
- A propagated azimuth colliding with a uniform sample keeps the existing
  LOUD `MalformedTopology` stop in the insert path (never silent merge).

### 4c. Cone-band azimuth merge

`tessellate_cone_face` learns the `inserted_rims` routing the cylinder
tube already has: a frustum band with either rim inserted goes through
the azimuth-merge strip (shared implementation with
`tessellate_lateral_azimuth_merge`, orientation generalized to
`cone_outward_normal`; the cylinder path stays byte-identical). Apex fans
and cap/annulus CDT consume rings of any length already.

### 4d. Scale-aware exactness certificate band

`exact_junctions` certifies `|f| ≤ max(TAU_WORK, K·ε_f64·L)` per surface,
where `L = |p| + |surface reference point|` (apex / axis point / center;
plus radius for cylinder/sphere/torus) is the magnitude the evaluation
arithmetic actually operates on, `ε_f64 = 2⁻⁵²`, and `K = 8` (small
documented safety factor over the ~1.2·ε·L measured on R0017 v4/v5).
This is NOT tolerance-widening (P9): an absolute 1e-12 at scale 4000 is
~2 ULP — unreachable by ANY correct f64 evaluation — while chord-sagitta
inexactness sits ≥10 orders above the band (Stage-1 d_ε = 1e-2·diag).

## Branch table (increment 4)

| condition | action |
|---|---|
| plane face parallel to rim plane | skip (no insertion) |
| plane section line misses / tangent to rim circle | skip (tangency gate) |
| junction outside the face polygon (or inside a hole) | skip |
| face loop carries an edge that is neither LineSegment nor closed Circle | skip THIS face (v1) |
| face loop carries closed Circle edges (disc/annulus cap) | containment by circle parity (inside-count), boundary-inclusive |
| rim is an ARC (`start != end`) | candidates filtered to the CCW sweep window (stage-1 arc convention); candidates within TAU_MODEL of either endpoint vertex are skipped (the junction IS the vertex) |
| propagated azimuth outside a target arc's sweep | skip that target (partial strips stay conformal: band-partner arcs share the sweep window) |
| coaxial group touches a torus/sphere face | drop the whole group's insertions |
| sibling rim lacks a junction azimuth | propagate exact on-circle point |
| sibling rim already has the azimuth (≤ TAU_MODEL chord) | no duplicate |
| propagated azimuth ties a uniform sample | loud `MalformedTopology` (existing) |
| frustum band rim inserted | azimuth-merge strip (multiset-verified) |
| junction vertex within scale-aware band of ≥3 surfaces | certified — skipped by all Stage-4 maps |
| junction vertex beyond band on any incident surface | today's loud STOP unchanged |

## Invariants (increment 4)

- **I5 (propagation exactness)**: every propagated point lies on its rim
  circle within f64 rounding of the closed-form construction.
- **I6 (cylinder byte-identity)**: the azimuth-merge refactor leaves the
  cylinder path's output byte-identical (dispatch-only change).
- **I7 (band monotonicity)**: the certificate band is `max(TAU_WORK, …)`
  — never narrower than the shipped increment-3 band, and always ≥10
  orders below the Stage-1 chord bound at the same scale.

## Oracles (increment 4)

- **Unit**: plane-arm derivation returns the two exact rim∩plane points
  for a synthetic coaxial double-frustum × cuboid configuration; empty for
  a parallel plane; empty for a crossing outside the face polygon;
  propagation fills sibling rims exactly on-circle and dedups the
  F0059-style already-present azimuth.
- **Unit**: double-frustum BRep rebuilt with a shared-rim override
  tessellates watertight through the cone azimuth merge (RED today:
  `MalformedTopology` count mismatch).
- **Unit**: certificate band = TAU_WORK at |p| ≤ 1; covers 1.36e-12 at
  L ≈ 9500; stays ≥10⁶× below the sagitta magnitudes measured in the six
  cases.
- **Green pin (end-to-end)**: coaxial double-frustum lathe ∖ slab whose
  side plane crosses both bands transversally — `boolean()` Ok,
  watertight, 2-manifold, volume equals the analytic reference (slab
  overlap via the documented circular-segment slice integral).
- **Assay**: R0017 (fast canary) ERROR→CORRECT; the 6-case family
  re-censused; full corpus 0 WRONG, no CORRECT lost (the plane arm fires
  broadly — the full run is the regression oracle, not optional).

## Failure modes / residue (increment 4)

- R0004 retains its SEPARATE `RevolveAxisIntersectsProfile` engine error —
  the boolean STOP is fixed by this increment but the case may stay ERROR
  until the revolve wall is addressed (different epic).
- Junction azimuth colliding with a uniform rim sample: loud stop
  (pre-existing insert-path behavior, unchanged).
- Non-polygonal plane faces (arc-bounded caps): no insertion (documented
  v1 residue; loud walls preserved).
- R0020/R0035/R0070 (surface-pair × conic junction mix) and R0096
  (torus×torus) are OUT of this increment's scope (census sub-family b/c).

## Research Basis (increment 4)

- [#24] Yang et al. 2025 §4.3.3 Case IV — unchanged basis; the plane arm
  is the same subdivide-at-the-crossing rule for a different section
  curve (line instead of parallel line pair).
- [#1] Patrikalakis Ch.5 §5.2 (plane∩plane line; circle∩line quadratic).
- Shewchuk-style scale analysis for the certificate band: f64 evaluation
  of a signed surface distance at magnitude L carries O(ε·L) rounding —
  the band certifies "exact to evaluation precision", the strongest
  property float arithmetic can witness.
