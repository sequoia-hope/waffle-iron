# §4.5.1 relocation domain — the certificate, and what the corner-transit work left in force

**Status (2026-11-13).** The crease transit and the corner transit that this
spec designed are deleted (ledger N87, RESOLVED by removal). They applied
§4.5.1's truncate-and-continue step to points on a boundary curve and at a
corner where three surfaces meet. The paper excludes both from that strategy
and sends them to §4.5.2 refinement (`refs/text/yang2025_hybrid_boolean.txt:635-657`,
Fig. 13). The kernel now does what the paper says: an out-of-domain junction
is detected, recorded as a failure, and answered by `boolean::refine_loop`.

What this file still specifies, under the section numbers the code and the
tests cite:

| Section | In force |
|---|---|
| §1 | The defect: a relocation that lands on the extended surfaces outside the face that claimed it |
| §3j | R0011's Euler characteristic, χ = 0 (genus 1) |
| §3k | The fan-local torus chart (`SurfaceChart::new_local`) |
| §3s | The §4.5.3 surface-pair tangent arm |
| §3t | The relocation domain certificate (`crease_crossed_by_step`, `boundary_crease_crossed`) |
| §3ah | R0053 adjudicated by exact membership: genus 1 |
| §3aj, §3ak | The divider of a crease seen from the planar face that contains it is the crease's OTHER surface (P0040; every planar conic) |

Sections kept below are as written when their mechanism landed. Where one
mentions a transit, a corridor, an emission or the `YANG_451_TRANSIT*` knobs,
that part is gone; the detection, the chart, the tangent arm and the two
adjudications stand.

Cases whose out-of-domain relocations the certificate detects: R0011, R0044,
R0074, R0085, P0040 (the `RelocationCrossedCarrierVertex` and
`RelocationCrossedCrease` fires).


## 1. The defect (measured; §4-I8/I9/I10 + the 2026-08-29 probes)

One uniform configuration across every fired site
(`YANG_S4_CARRIER_DOMAIN=census`):

- The traveller is a mesh intersection-curve endpoint carrying
  {far, base, facet_k} — it sits where the chord-resolution mesh curve
  crosses the model edge base∩facet_k of a many-facet operand (R0011:
  giant-cylinder base B:1 r≈6277 + planar facets, far = A:2; R0044:
  cone-band facets, far cone; R0074: operands swapped, scale ~1e-1).
- The relocation solves the exact triple {far, base, facet_k} — and that
  junction EXISTS but is a PHANTOM: it lies past the still model corner
  q = base∩facet_k∩facet_j (post is ON all three surfaces to ≤3e-14 at
  scale 5e3, and outside facet_k's actual extent). The §4-I9 postcondition
  catches exactly this (q strictly interior to the pre→post segment,
  off-travel ≤6e-13; overrun 0.3–3.6 % of the exact-zero extrapolation,
  §4-I10 (f)).
- The corner q ALREADY EXISTS as a mesh vertex (`TruncateAtVertex`
  t = 0.14–0.32 names it). Nothing needs minting at the corner itself.
- The TRUE junction is {far, base, facet_j} on the adjacent facet — the
  exact crossing of the curve far∩base with the next facet's surface.

Why neither existing vehicle claims it today (measured 2026-08-29):
- `YANG_441_REHOME=census` prints NOTHING on R0011 — the rehome census hooks
  the I13d selector's `not_richer` branch, and this flow never enters the
  I13d selector; it dies at the §4-I9 stage-end postcondition.
- §4.5.1 proper (Fig-12) is selector-excluded: the traveller is a BOUNDARY
  glider (on two surfaces of one operand at both step ends), Fig-13's
  exclusion, §4-I10 (f) 24/24.
- §4.5.2 refinement: adjudicated out by direct measurement (the crossing is
  binary in the overrun sign — any nonzero overrun at any density fires).

## 3j. R0011 — χ = 0, genus 1

`euler_target` 0 is authored in `R0011.meta.json` and pinned in
`assay_euler_consistency.rs`. Three independent proofs (the R0091/R0063 protocol):
(a) exact-involute voxel-CSG from the authored numbers — the prism
touches the band ONLY near its start cap (spine t < 150; the root
circle r=2055 misses the band r∈[4708, 6277] entirely), in exactly
TWO disjoint adjacent-tooth patches ⇒ genus k−1 = 1; (b) the density
ladder — χ = 0 at 1×/2×/4× through three DIFFERENT fire anatomies
(3 corridors / 1 corridor / refine-retry apply after a NoRealCandidate
natural decline: the apply generalizes across densities); (c) volume +
watertight + single-shell + improper=0 all pass. `euler_target` 2 → 0
authored in `R0011.meta.json`, pinned in `assay_euler_consistency.rs`.

## 3k. inc-2c-3b-3 — the fan-local TORUS chart (LANDED 2026-08-31,
third session; R0074 CONVERTS — the epic's SECOND conversion)

The R0074 wall typed by 3b-2's measurement: the refine-retry
invocation fires (129, 127), the corridor plans cleanly (4 plans,
2 mints, zero declines), and the apply refuses at
`far fan refill v129: NonPlanarPatch{592}` — R0074's far is a TORUS
(revolve(circle)) and `refill_fan_hole`'s `SurfaceChart` had no torus
chart.

Landed shape (`stage4_project.rs` + `stage4_construct.rs`):

- **`SurfaceChart::Torus`** with the PINNED `stage4_dt::eval_uv` §2
  embedding — param `(θ, φ)` = azimuth about the axis + tube angle,
  `center + (R + r·cos φ)(cos θ·e1 + sin θ·e2) + r·sin φ·â`, the same
  `ortho_basis(axis)` frame — so the like-for-like d(T) budget
  (which projects through the chart and certifies through `d_of_t`)
  speaks one convention. Ring torus only (`R > r > 0`, the
  `validate_surface` rule). A unit test pins chart.lift ≡ eval_uv and
  the on-surface round-trip.
- **`SurfaceChart::new_local`**, the FAN-LOCAL constructor: everything
  `new` charts, plus the Torus under `YANG_441_TORUS_CHART=1|on`
  (default OFF — byte-identical everywhere by construction).
  `new`/`supports` are UNCHANGED: a torus is doubly periodic and a
  whole patch may wrap a full period in either direction, so
  wholesale-patch holders (construct / splice / stage-5 holder gates,
  which chart entire boundary cycles) keep today's typed refusals
  until seam machinery exists. Only `refill_fan_hole` consumes
  `new_local` — a fan hole's link polygon is corner-local, where
  chain-unwrap is sound.
- **Double-periodic handling in `refill_fan_hole`**: the predecessor-
  relative chain-unwrap and the `< 2π` span guard now apply to φ
  exactly as to θ (torus only; cylinder/cone byte-identical), and the
  budget's old-fan unwrap re-centres both coordinates toward the
  window mid. Unit fixture: a fan window straddling BOTH seams
  (θ = π AND φ = π) fills with the far-arm polygon shape (link +
  corridor mint) and refuses `NonPlanarPatch` with the knob off. The
  first fixture attempt dropped a corner from the polygon and the
  like-for-like budget honestly refused it (ChordDegradation 1.13 vs
  0.61) — the budget arm works on tori; the far-arm polygon (same
  footprint, corridor mints closing the hole) is the satisfiable
  shape, exactly why §3j's far arm passes the whole link + path.

**Measured (R0074, `YANG_451_TRANSIT=1 YANG_441_TORUS_CHART=1`):
`APPLIED corridors=1 plans=4 mints=2 removed=2`, the §4.5.4
refine-retry invocation completes (the natural pass still stops at
OffCurve v91 and the retry repairs at 2733 verts), and the case is
`SUPPORTED_CORRECT — all checks passed` (10.4s), with the STANDING
authored expectations — no oracle adjudication needed.** Proofs:
gates off → byte-identical honest ERROR (OffCurve v91); transit-only
(torus knob off) → the fire + `REFUSE far fan refill v129:
NonPlanarPatch{592}` reproduced verbatim, ERROR unchanged (the torus
knob IS the delta); R0011 converts identically under transit-only and
transit+torus (the knob is inert for its cylinder far).

## 3s. inc-2c-3b-10 — the §4.5.3 SURFACE-PAIR tangent arm (LANDED
2026-08-31, fourth session; the FaceId(459) ring wall falls — the case
moves to FaceId(626) "patch triangulation folded")

The `YANG_453_PAIR=census` + `YANG_T145_SWEEP_PROBE` run adjudicated
the 3b-10 anchor's suspect one layer deeper: the fold sites ARE
detected by the task-#145 probe — `[t145-sweep] mixed-cycle conic
U-turn skip … cn=SurfacePair { a: Cylinder(r=2327.8), b: Cone(…) }`,
890 lines — and the #145 shared-conic parameter arm IS landed, but
`mixed_cycle_shared_conic` branch-12-skips `Curve::SurfacePair` and
`conic_param_deltas` has no parameterization for it (branch 11): the
M5 Option-B procedural curves (the far cylinder × the CONE bands)
were vocabulary-invisible to the sweep. The bands are cones — one
half-angle per band — and the fold chain's edges all carry
SurfacePair curves.

Landed (always-on, the #145 branch table unchanged):

- `mixed_cycle_shared_conic` admits `SurfacePair` — identity = exact
  surface equality, unordered (storage order is a frame choice).
- `conic_param_deltas` gains the TANGENT arm: at a SurfacePair site
  the curve's OWN tangent at `p_r` is exact — T = n_a × n_b from the
  two surface gradients — and the deltas are the signed tangent
  projections of the neighbours (lengths, no wrap). This is the
  paper's "progress along the intersection curve" evaluated
  analytically AT the site: a coarse-but-monotone chain has its
  neighbours on OPPOSITE sides of T and reads healthy whatever its
  turn angle, so the P10-disproven angle-band false-positive class
  cannot arise. Degenerate/unreadable tangent = branch 11 (cannot
  diagnose). Victim selection and the 2·d_ε resolution gate are the
  existing shared path, verbatim.

**Measured (R0044, transit+torus+SPAIR gates): the fires, the APPLIED
line (6/20/15/20), and the retry's v105 wall are all byte-identical;
the FaceId(459) ring wall FALLS (the fold chain is §4.5.3-collapsed
before topology), and the case stops at the next output-tessellation
frontier — `FaceId(626): patch triangulation folded (inverted
triangle)`, measured by `KV2_CHORD_DEPTH_CENSUS`: `kind=dev
w_facet=325.9 r_unroll=3683.1 n_split=1 max_split_dev=3.60
max_chord_sag=2.4e-12 min_h2d=1.18 fold=inverted` — the KV9-F2
inverted arm with dev ≫ sag despite the F2b lift-faithful refinement
criterion, on a near-sliver 2D height (min_h2d 1.18).** R0011
(3/13/11/13) + R0074 (1/4/2/2): byte-identical, both
SUPPORTED_CORRECT under the arm.

**RESOLVED (inc-2c-3b-11, 2026-08-31): FaceId(626) = the one-sided
conforming-insert fold — kernel-v2 inc-8b closes it.** The full
anatomy, measured (`[chain-probe]`+`[chain-node]` final chart table,
`KV2_PATCH_FOLD_PROBE`, `[conform-pt]`): face 626 is a LEGITIMATE
carried B-band — a ~304° near-sliver cone strip (tan α=1.596)
between coaxial rims r=3681.154/3683.060, chart width 1.19 vs facet
chord sag 3.60. The R0054 grid alignment rung-pairs the two rails
bitwise for the whole strip EXCEPT one node: a face-627 boundary
vertex at d3=1.656 from rail 1's circle (azimuth 58.495Δ) mints a
mechanism-2 conforming insert on rail 1 — and rail 2's pool (its
incident faces are 625/626, not 627) can never see that vertex,
though it sits well inside rail 2's window too. The pool is
EDGE-local; the fold constraint is FACE-local. The unpaired insert
lands mid-chord of rail 2's rung (sag 3.6 ≫ strip 1.19, and the
inversion needs the cone's tilted normal — a cylinder merely
degrades); F2b then fires on the inverted all-on-surface triangle
(dev≈0 < sag ✓), LEPP-splits the rung, and its ArcSample split rule
lerps the mint ON the 3D chord (dev=3.605 — the census's split; the
T-junction closure contract, correct as designed), after which
`dev ≥ sag` rightly refuses to chase the arm's own off-development
node. The fix is NOT the split rule and NOT a band: **kernel-v2
inc-8b** (spec `yang_434_output_chord_refinement.md`) completes the
gated inc-8a curve pool (`KV2_ARC_CONFORM_CURVES`) to depth 1 — a
pool arc contributes its grid samples PLUS its own vertex-pool
inserts (static B-Rep data, no recursion into curves), making the
pool view of an arc EQUAL to its chain view's insert set; the
azimuth-set closure is exact at depth 1. R0044 is inc-8a's FIRST
corpus customer. Unit-pinned both ways
(`pool_curves_carry_their_vertex_inserts_across_the_strip`); gate
off, the fold stays the pinned loud wall. Under
transit+torus+SPAIR+`KV2_ARC_CONFORM_CURVES=1` (the gate set is now
FOUR knobs) face 626 tessellates clean (`n_split=0 fold=0`,
min_h2d=1.19 = the strip width) and the case advances to
**`FaceId(627): ring rejected by CDT`** — an UNMASKED latent (626's
fold was the loud stop hiding it; the R0053 lesson shape, §3o).
ANCHORED same session (`KV2_RING_REJECT_PROBE` +
`[chain-probe]` 627): face 627 is the next band down (rims
3681.154/3548.944) carrying the corner-transit notch itself —
SurfacePair → 2×HyperbolaArc (per-mesh-piece, n_interior=0) → arc
chains. The junction vertex X=(−1813.598774,−2388.465072,
−6104.296761) between the SurfacePair edge and the first
HyperbolaArc is **ON face 627's own cone** (7.7e-6 — the uniform
rim-derived fit residual EVERY ring point shares, not a per-point
defect) but sits **0.827 PAST the station of the r=3681.154 rim that
BOUNDS the face** (h=2124.192 vs the rim's 2123.365). X is on the
EXTENDED surface, in the parametric band belonging to the NEIGHBOUR
face 626: in 626's own frame X lands at h=2307.550, INSIDE 626's
[2306.722, 2307.916] band (0.827 past the shared rail of a 1.194-wide
strip), 0.060 off 626's cone — the crease departure between two cones
(tan α 1.7336 vs 1.5958). **This is the 3b-8
identification-vs-domain shape at the EMISSION layer**: a point valid
on the extended surface, OUT OF DOMAIN for the face's actual B-Rep
boundary. In the chart the ring therefore pokes above its own top
edge and self-intersects the rim run TWICE — the incoming SurfacePair
edge at u=63.446 (against the negative-going run) and the outgoing
hyperbola edge at u=65.738 (against the positive-going run) — so the
CDT rejects CORRECTLY, over a yang-side emission defect. The rim node
at X's exact azimuth (Δu=7e-13) is X's OWN always-on mechanism-2
vertex insert on 627's chain, so the crossing exists with or without
inc-8b. Face 626 carries NO notch (a clean 8-edge loop: 6 arcs + 2
line segments), so the crossing's transit into the 626 band was never
built — now the MEASURED leading reading of the three this anchor
opened.**

**ADJUDICATED same session — the transit reading WINS, exactly
(`[chain-probe]` SurfacePair/HyperbolaArc parameter dump + closed-form
root solve).** The three surfaces meeting at X are named: the
SurfacePair is **far Cylinder** (r=2327.818) **× cone 627**, and the
HyperbolaArc's normal is exactly −(cylinder axis), so it is the
cylinder's **CAP PLANE × cone 627**. X is therefore the
cylinder-end-circle ∩ cone triple point — and it is EXACT: plane
residual −2.0e-13, cylinder 4.5e-13, cone 4.5e-13, and the
closed-form solve for that circle × cone 627 returns X itself to
**8.2e-13**. **X is not a stray or mis-relocated vertex; it is the
correctly-computed triple point of the EXTENDED cone 627, placed
0.827 outside 627's own domain.** The identification is right and the
domain check is missing — the 3b-8 lesson, verbatim, at the emission
layer.

The true corner topology, all of it measured:
- **The real junction J EXISTS on face 626**: the same cylinder end
  circle × **cone 626** has a root at station h=2307.654 — INSIDE
  626's [2306.722, 2307.916] band (0.932 past the shared rail, 0.262
  short of the far rail), 0.138 from X. J is where the junction
  belongs.
- **Both crease crossings EXIST on the shared rim** (r=3681.154):
  the rim meets the cylinder LATERAL at
  P_lat=(−1814.091366,−2390.548529,−6103.426403) (2.311 from X) and
  the CAP PLANE at
  P_cap=(−1813.637849,−2388.505205,−6102.509823) (1.788 from X).
- So the correct emission is: 627's SurfacePair chain terminates at
  P_lat and its hyperbola chain at P_cap (both ON the crease); the
  shared rim splits at both; and **face 626 gains the notch** —
  lateral∩cone626 from P_lat to J, cap∩cone626 from J to P_cap. Today
  626 carries NO notch (a clean 8-edge loop: 6 arcs + 2 line
  segments) and 627 carries the whole excursion on its extended
  surface. The excursion is small but real: chart width 2.29, height
  0.827.

**inc-2c-3b-12 is therefore a DETERMINED build, not an
investigation**: split the crease at P_lat/P_cap, re-terminate 627's
two chains there, and construct the 626-side notch through J. The
domain postcondition it needs is the §4-I9 shape already in the
stack — a chain terminal must be certified against its OWN face's
bounding-rim domain, not merely against its surface (the
identification-vs-domain rule, now with a second measured customer).**

*Correction, same session:* an earlier draft of this block recorded X
as "0.15 off face 627's own cone", an UNMEASURED figure. The measured
values are above: X is on 627's cone and out of 627's DOMAIN. The
error inverted the diagnosis (an off-surface emission would be a
geometry defect; an out-of-domain terminal is a topology/transit one)
— the 3b-8 lesson, incurred again by asserting a distance without
computing it.

**GATED `YANG_453_SPAIR` (default OFF) by the always-on corpus run's
verdict: ONE E→W flip — R0053, the M8 coplanar-graze case.** Its
fold's ring rejection (`FaceId(474)`) was the loud stop masking the
coplanar capability gap; with the fold collapsed the case COMPLETES
at χ=0 against the authored 2 (`mesh_euler_characteristic` — the
composition oracle keeps it loud as W, but the canonical bar is
0 WRONG, enforced). The arm itself behaved exactly as designed there
— the adjudication owed is R0053's: either its χ expectation is
oracle authoring (the R0011 `euler_target` precedent — true topology
changed by the graze) or the completion is a genuine M8 silent-wrong
(then the flip waits on M8 Stage-0). **The flip condition is that
adjudication — never a narrower band.** Default-off corpus:
byte-identical canonical 273C/0W/34E/1EE/0T (re-proven).

## 3t. inc-2c-3b-12 — the relocation DOMAIN certificate (LANDED, GATED)

**The defect, anchored empirically** (`YANG_TRIPLE_WATCH` backtrace on
the exact solution): R0044's out-of-domain junction X is minted by
`stage4_relocate_and_correct_inner`'s **triple-junction relocation
arm** — `relocate_onto_implicit_triple(seed, Cylinder, Plane, Cone627)`
on mesh vertex **v47**, travel 18.07. The arm's only acceptance gate is
the displacement corridor `tangent_plane_corridor(d_eps, sin θ)`, which
passes it easily: across the case's 306 triple relocations the
displacement distribution is p25 3.4 / p50 6.1 / p75 13.9 / max 182.8,
so 18.07 sits near p80 and is in no way anomalous. **The arm has no
domain postcondition at all** — it accepts any exact solution of the
three EXTENDED implicits.

**Why the check must be ANALYTIC, not mesh-derived.** v47's seed lies
17.98 from the cylinder, 10.53 from the crease circle, and 5.09 from
its own cone; all four exact candidate features (X, the true junction
J, and the two crease crossings P_lat/P_cap) sit 16.6–18.4 away, i.e.
inside ONE mesh chord. A mesh-based containment test cannot separate a
0.827 overrun from a legitimate landing at that resolution. The crease
is analytic, so the certificate is too.

**The paper is explicit here, and this is its stated trigger** —
§4.5.1, `refs/text/yang2025_hybrid_boolean.txt:672-690`: *"Instead of
taking a full step length that takes the point to a position `p1`
**outside the surface `S2`** where the point is initially located, we
truncate the step so that the point moves to `p` on the boundary curve
`C_b` between `S2` and the neighboring surface `S1`. In the next
iteration, the optimization step of `p` is computed using the
parameterization of `S1` … After obtaining the correct position of
`p`, we first solve the intersection points `q1` and `q2` on `C_b`."*
That is, in order: the domain test, the truncation, the transit, and
the q-points — P_lat and P_cap being exactly the q-points here.
`stage4_truncate`'s own header predicted this join: it records that its
mechanism and §4.5.1's stated trigger "can be joined without borrowing"
for the class that converges exactly *as equations* but not *within its
domain*. v47 is precisely that class.

**Built (detection half only), gated `YANG_451_TRIPLE_DOMAIN`**
(unset/`0`/`off` = OFF and byte-identical, `census` = report and
continue, `1`/`on` = typed STOP `RelocationCrossedCrease`):

1. **`crease_circle_from_pair`** (`stage4_boundary_curve.rs`) — the
   analytic boundary curve `C_b`, generalizing `rim_circle_from_pair`
   past Cylinder×Plane to **Cone×Cone** (coaxial, distinct openings:
   `h·tanα₀ = (h+δ)·tanα₁`), **Cone×Plane** (⊥ axis) and
   **Cylinder×Cone** (coaxial). Only configurations whose intersection
   is exactly a CIRCLE are answered; everything else declines rather
   than approximating (a near-coaxial pair meets in a quartic).
2. **`creases_by_surface` / `creases_for_surfaces`** — the crease index
   keyed BY SURFACE, built once per stage. Sourcing this from edges at
   the moving vertex was measured WRONG and fixed: v47 sits 10.5 from
   the crease it overruns, so a vertex-incident sourcing sees nothing.
   The domain a relocation must not leave belongs to the FACE.
3. **`crease_crossed_by_step`** — the certificate. A step violates its
   domain when the pre- and post-positions lie on strictly OPPOSITE
   sides of a crease plane bounding one of the vertex's own surfaces.
   Two exemptions, both membership statements rather than thresholds:
   a vertex that lies ON the crease (`on_crease`: satisfies BOTH
   forming surfaces within their own `junction_certificate_band`) may
   glide along it; and the residual band is **PROPAGATED** — the crease
   plane is DERIVED, so its band is its own plus both parents'. The
   plane's band alone understates the construction badly (its reference
   magnitude omits the crease radius entirely, and for a
   near-cylindrical cone omits an apex magnitude four orders larger
   than the geometry it describes).

**Measured on R0044** (`census`): **8 fires, 5 noise fires eliminated by
the propagated band.** The two populations separate by TEN orders —
material overruns 0.309 … 40.08 against bands of order 1e-11:

| v | ρ | d_pre | d_post |
|---|---|---|---|
| 47 | 18.07 | −0.194 | +0.827 |
| 75 | 84.95 | +6.054 | −30.653 |
| 76 | 60.43 | +16.200 | −31.727 |
| 89 | 58.60 | +40.078 | −12.133 |
| 38 | 7.60 | +4.989 | −0.309 |
| 39 | 7.56 | +0.984 | −4.197 |
| 59 | 9.76 | +1.129 | −4.983 |
| 105 | 5.62 | +2.452 | −2.584 |

The five exempted rode a crease with residuals of 1.6e-11 … 1.4e-10 at
both ends, sign meaningless. **v105 is a cross-confirmation**: the
independent §4-I9 carrier-domain check (`YANG_S4_CARRIER_DOMAIN`) fires
on that same vertex by a different mechanism, and v105 is also the
§4.5.4 retry's own `ChordDegradation` wall — three unrelated
instruments naming one site.

7 unit tests (`tests_unit/s451_crease_domain.rs`) pin the crease
geometry (both directions, including the declines), the material-overrun
fire, the noise exemption at R0044's own magnitudes, the on-crease
exemption, and the by-surface sourcing.

**FULL-CORPUS CENSUS (312 cases, `census` mode).** Categorized score is
the canonical **273C/0W/34E/1EE/0T** — census mode is behaviour-neutral
corpus-wide, as built. The certificate fires in **exactly TWO cases**:

| case | verdict | fires | overrun range |
|---|---|---|---|
| R0044 | ERROR | 8 | 0.309 … 40.08 |
| R0003 | **SUPPORTED_CORRECT** | 6 | 0.00078 … 0.265 |

R0003's six (v1983, v7611, v8658, v8809, v9336, v11356; ρ 0.0043 …
0.459) are the increment's most consequential measurement: **a
SUPPORTED_CORRECT case carries genuine out-of-domain triple relocations
and still produces correct output.** So the condition is real but not
always fatal — it is fatal in R0044 (where the overrun self-intersects
face 627's output ring) and silent in R0003. The same shape as the F2a
chord census, where "folding is the sliver lottery on a UBIQUITOUS
depth defect": the defect is the overrun, and whether it kills the case
is downstream luck.

**Consequence, and it is binding: the STOP must never be armed
always-on as it stands** — it would convert R0003 from CORRECT to
ERROR, breaking the 0W/273C bar for a defect that case survives. The
certificate's role is diagnosis and the repair's precondition, not a
wall.

**A discriminator deliberately NOT taken.** R0003's largest overrun
(0.265) and R0044's smallest (0.309) happen not to overlap. That gap is
1.2×, sits on six and eight samples, and separates nothing structural —
using it would be exactly the band-tuning P10 forbids, and it would
"squeak a case through" while leaving the defect in place. The two
populations are the same defect at different magnitudes; only the
repair distinguishes them, by fixing both.

**Not built here — the repair (3b-12b).** Detection only. The paper's
remaining three steps are: truncate the step to `C_b`, transit onto the
neighbouring surface (which for v47 yields J, the true junction, on
cone 626 at station 2307.654 INSIDE 626's band), and solve the
q-points P_lat/P_cap on `C_b`, splitting the crease there so face 627's
two chains terminate on it and face 626 receives the notch it currently
lacks. The gate stays OFF until that arm exists: today the certificate
would only convert one confusing downstream stop (a rejected output
ring on a different face) into a precise local one, which is worth
having but is not a conversion.

## 3ah. inc-3b — R0053 ADJUDICATED by exact membership: genus 1, the
authored χ was the default, the sidecar reference was a mesh inside the
solid; `YANG_453_SPAIR` FLIPPED always-on (LANDED 2026-09-03)

§3af closed on two readings of R0053's union that both went through a
mesh: the lattice ladder over the operand TESSELLATIONS (unstable) and the
Cherchi sidecar's union of those same tessellations (χ = −28, "genus 15",
attributed to "fourteen gear-groove tunnels at the graze" — an inference,
never located). Its addendum then found the exact arrangement's kept
submesh already at genus 1 and named "the overlay/classification at Stage
0–2" as where the fourteen handles were lost. This increment removes the
mesh from the question and finds there were never fourteen handles to
lose.

**The configuration, measured from the authored numbers.** All three
sketches share one plane; the sketch x axis is the revolve axis direction
(`SketchPlaneBasis` puts `x̂ = −â` exactly). In the sketch frame `(u, v, w)`
(`w` = the plane normal, the extrude's direction): the ring is the
rectangle `|u| ≤ 20.814, |v| ≤ 44.556` revolved 287.61° about the line
`v = −62.443` (radial band 17.89…107.00), the box is `|u| ≤ 41.494,
|v| ≤ 52.075, w ∈ [0, 100.27]`, and the gear (16 teeth, module 7.455:
root 50.323, base 56.046, addendum 67.098; half-tooth 6.479° at the base,
2.119° at the tip) is revolved 301.92° about the PARALLEL line
`v = −89.464`, 27.021 further out. Both revolves sweep toward `−w` first
(`m̂ = â × ŵ = −n̂`), so their gaps lie on the box's side and the box
bridges the C-ring (§3af's op-1 reading, confirmed). The ring's
cross-section lives entirely inside the gear's axial band `|u| ≤ 20.814`,
so it can meet only the teeth whose flanks reach that band: a census of
the ring solid against the gear's meridian disc
(`s453_r0053_exact_topology::r0053_ring_vs_teeth`) finds ring material in
exactly THREE teeth — 247.5°, 270°, 292.5° — and in their two grooves from
the root radius up to the addendum, i.e. the ring FILLS those grooves
from the root side and embeds the teeth; it never covers a groove with an
open floor. Thirteen teeth never touch the ring at all.

**The exact-membership ladder** (`s453_r0053_exact_topology::
r0053_exact_ladder`): each operand is a closed-form point predicate — the
revolves through `(ρ, θ, v_unrotated)` with kernel-v2's sign conventions,
the gear disc through the involute half-width `π/(2N) + inv(α) − inv(t)`,
`t = √((r/r_b)² − 1)` — and the cubical `V − E + F − C` of the SET UNION is
read on the in-memory grid (`VoxelGrid::from_fn`) over the joint bounding
box at cell size `h`:

| set | h = 2 | 1 | 0.7 | 0.5 | 0.4 | phase ¼ at 0.7 / 0.5 / 0.4 |
|---|---|---|---|---|---|---|
| ring / box / gear alone | 1 / 1 / 1 | 1 / 1 / 1 | | | | |
| ring ∪ box | 0 | 0 | | | | |
| ring ∪ gear | | | 1 | 1 | 1 | 1 / 1 / 1 |
| **ring ∪ box ∪ gear** | **0** | **0** | **0** | **0** | **0** | **0 / 0 / 0** |

One face-connected component throughout; the finest rung is 94.7 M cubes
(`n = [346, 793, 793]`, 7.9 s). A predicate cannot alias a coplanar
overlap (the union is solid across the shared plane) and cannot mint a
sliver, so the ladder has nothing to be unstable about — and it is not.
**R0053's union is genus 1** (χ_solid 0, boundary χ 0, one shell): the box
bridges the C-ring, and the gear adds no handle. Volume 6.062e6 ± 0.1 %
across rungs.

**The kernel is right; the reference was never the union's boundary.**
Face-by-face classification against the exact predicate (probe each
centroid ± δ along its normal, δ = 0.05…2.0; a boundary face has inside
on one side and outside on the other at the smallest δ that clears the
chord band):

| mesh | faces | strictly INSIDE the exact solid | strictly outside | where |
|---|---|---|---|---|
| kernel-v2 operand tessellations (controls) | 468 / 12 / 55548 | 0 | 0 | — (every face within 0.5 of the exact surface: the predicates match the kernel's gear) |
| sidecar ring ∪ box | 552 | 9 (area ≈ 4011) | 0 | the ring's θ = 0 cap region, solid on both sides |
| **sidecar ring ∪ box ∪ gear (χ = −28)** | 52118 | **606 in 65 clusters** (largest 85 faces, area 1552, the gear-cap ∩ box-base overlap at `w = 0`; then 59 / 59 / 45 / 28 / 14 / 13 / 11 more coplanar membranes) | 5 | membranes at the shared plane, and sliver strips of area 0.06–0.5 at `u = ±20.814` along the ring-end / tooth-flank crossings (φ ≈ 251° and 289°, r ≈ 64.4–65.4; eleven such strips among the twenty largest clusters, at θ₂ = 43°…286°) |
| **kernel-v2 R0053 under `SPAIR`** | 56026 | **38** (4 × 9 micro-faces of area 0.011 at the box-edge / root-arc crossings, 2 faces of area 0.35 on the root arc) | 1 (area 0) | all at `w = 0` inside the root arc's chord band — tessellation-band residue, not geometry |

The sidecar meshes are closed 2-manifolds (0 boundary, 0 non-manifold
edges); dropping their inside faces leaves 13 / 670 boundary edges, so
the false walls are WOVEN INTO the surface — the closed surface runs
through the solid. Its `V − E + F` is therefore not the union's; "genus
15" was the pairwise sidecar's coplanar handling on this input (the same
limitation whose N-way form refuses the case outright), and the fourteen
"tunnels" were its sliver strips. The kernel's output reads χ = 0 with one
shell by the runner's Euler oracle (`V(28014) − E(84040) + F(56026)`),
signed volume 6.048e6 (the tessellated operands' inscribed deficit; the
sidecar's 6.0479e6 sits at the same value), and carries no face inside the
exact solid beyond the root arc's chord band.

**What this decides.**

1. **§3s's flip condition is met on its FIRST branch**: R0053's authored
   `euler_target = 2` was the generator's genus-0 default, refuted by an
   exact derivation from the authored numbers (the R0011 protocol,
   `historical_authoring_fixes_pinned`). The meta is corrected to 0 and
   pinned; the completion under the surface-pair arm is CORRECT.
2. **§3af's second-branch adjudication is RETRACTED**, and with it the
   addendum's "handles lost at Stage 0–2" and the M8 silent-wrong at
   R0053: there is no coplanar-graze defect here for Stage 0 to make loud.
   Stage 0 handles the flush caps correctly. The lesson is the reference
   oracle's own (`feedback_reference_oracle_invalidates_in_both_
   directions`): a reference whose output was not checked against the
   input's own membership adjudicated a kernel result WRONG that was
   right. A reference mesh must be certified — closed, manifold, AND no
   face inside the exact solid — before its χ is a verdict.
3. **`YANG_453_SPAIR` flips always-on** (household pattern, `0|off` = dev
   A/B). Two-proof corpus (release, 8 jobs, 360 s CPU budget), with the
   corrected meta in place for every run:

| run | score (+4 UNSUPPORTED) | `results.json` | wall |
|---|---|---|---|
| pre-flip, `YANG_453_SPAIR=1` | 276C / 0W / 31E / 1EE / 0T | vs canonical 275C: exactly TWO moves — R0053 ERROR → SUPPORTED_CORRECT ("all checks passed"); R0044's ERROR detail `FaceId(459)` ring rejected → `FaceId(626)` patch triangulation folded (§3s's reading, the §4.5.3 fold collapsed); nothing else | 557.6 s (F0085 298.8 s) |
| **post-flip default** | **276C / 0W / 31E / 1EE / 0T** | **BYTE-IDENTICAL to the pre-flip proof** | 571.2 s (F0085 311.6 s, F0065 171.7 s honest) |
| post-flip `YANG_453_SPAIR=0` | 275C / 0W / 32E / 1EE / 0T | **BYTE-IDENTICAL to the committed canonical** | 532.5 s (F0085 301.6 s) |

The committed `results.json` moves with the conversion (the UI snapshot);
no smoke-table pin names R0053. **New canonical: 276C / 0W / 31E / 1EE /
0T.** R0044 now walls canonically at `FaceId(626)` — the KV9-F2 fold on
the cone face the emission arm (§3ac–§3ae, still gated) repairs — and its
minimal converting set is now the emission chain alone on top of the
defaults.

**Instruments landed.** `tests/s453_r0053_exact_topology.rs` (the exact
predicates, the ladder, the ring-vs-teeth census — R0053's authored numbers
inline, so the probe reads the same document the corpus does without a
kernel in the loop) and `tests/s453_r0053_output_obj.rs` (the kernel's
R0053 output to OBJ for the face classifier). The classifier itself ran
as a scratch script this session. The generalisation landed the same day:
`assay::exact_membership` parses any corpus document into closed-form
predicates (polygon / circle / involute-gear profiles, extrude and revolve
with the feature engine's own semantics, cuts included) and lays its
lattice in the document's sketch frame — a lattice OBLIQUE to the model's
planes perforates frame-aligned thin features at every cell size (R0053
in the world frame: χ_solid +3 / −3 / −6 / −8 at h = 1.04 / 0.75 / 0.52 /
0.37 (phase ¼); in the sketch frame 0 at every rung from 2 to 0.3 on two
phases). The pinned adjudications (F0001, R0053, R0011, R0099, C0075,
R0091's volume, R0063 as a heavy pin) are its tests; the corpus sweep
found three kernel silent-wrong classes the mesh-borne oracles cannot see
(`docs/audits/exact_membership_sweep_2026_09_03.md`).

## 3aj. P0040 — a step within a cap plane that leaves the cap's disc

A step WITHIN a cap plane that leaves the cap's rim has a zero residual
against the rim circle's plane (the cap's own plane) at both ends, so a
plane divider cannot see it. The domain boundary of a planar face is RADIAL:
its divider is the rim's cylinder. §3ak states the rule for every planar
conic. Fixture: `crates/kernel-v2/tests/s451_transit_neighbour_side_chain.rs`
(P0040's two bosses; the §4.5 postcondition records the fires and the
§4.5.2 loop's first refinement emits the exact topology).

## 3ak. The divider of a crease seen from the planar face that contains it is the crease's OTHER surface

A crease is the intersection of two surfaces, and within either one the OTHER
surface's residual changes sign exactly across the crease. A planar curve
lying in a planar face has a zero residual against its own plane at both ends
of any in-plane step, so its plane cannot be the divider there. The rule
holds for every planar conic: a circular rim, and an oblique cap's ellipse,
parabola or hyperbola alike.

**Code (`stage4_boundary_curve.rs`).**

1. `crease_divider_for(curve, s_own, s_other)` — for a planar curve lying IN
   the planar face `s_own`: a circle's divider is its own coaxial cylinder
   (every surface of revolution carrying that circle shares it, so it needs
   no `s_other`); an ellipse / parabola / hyperbola's divider is `s_other`
   itself (the cylinder or cone it is a section of — a conic alone names TWO
   such surfaces, so with no `s_other` the divider is DECLINED, never the
   face's own plane that cannot fire). On a face the crease's plane cuts
   transversally the divider is that plane, for all four planar curves
   (`crease_plane`). Both certificates — §3t's `crease_crossed_by_step` and
   the §4.5 `CreaseIndex` — pass `s_other`.
2. `crease_curve_from_pair` — the triple arm's incidence creases:
   `crease_circle_from_pair`, else the ssi-rs plane-section solver (Stage 3's
   own) for `{cylinder, cone} × plane`, admitting exactly one planar conic;
   two generator lines, spirics and surface pairs stay out (the B-Rep index
   carries line creases).
3. `CreaseIndex::build` pairs a face's neighbour across a vertex pair by
   `curves_coincide` (exact, else sense-free `same_planar_curve`). The m1
   convention emits one directed copy per half-edge with the curve's normal
   in its face's sense (a tilted cylinder ∩ box: the cap's ellipse copy
   `normal (0, 0, 1)`, the lateral's `(−0, −0, −1)`), so exact equality would
   leave every planar crease of a real boolean output without its `s_other`.
   A seam of ONE surface split into two faces is recognised and skipped.

**Pins.** `tests_unit/s451_crease_domain.rs`:
`an_ellipse_crease_in_the_oblique_cap_that_contains_it_is_divided_by_its_cylinder`,
`crease_curve_from_pair_knows_the_sections_the_circle_constructor_did_not`,
`the_brep_crease_index_gives_a_cap_its_radial_divider`,
`the_brep_crease_index_gives_an_oblique_cap_its_cylinder_divider` (on a REAL
output: tilted unit cylinder ∩ box — the cap's crease divider IS the
cylinder, and a step within the cap past the ellipse fires);
`s4_conic_crease_in_plane_chain.rs::the_oblique_cap_boss_corner_builds_and_validates`.

**Open neighbour.** P0040's numbers with the big cap tilted by 0.15 / 0.3
(`crates/kernel-v2/tests/s4_conic_crease_in_plane_chain.rs`, the `probe_*`
sweeps) hold one failing window, y ∈ [−167.4, −159.9]: the small cylinder
GRAZING the big lateral from inside (external tangency at y = −160.807),
present with a flat cap too. That is the Case-III / Case-V graze family, with
an `#[ignore]`d reproducer
(`an_internal_skew_cylinder_graze_window_stops_loudly`), not this rule.
