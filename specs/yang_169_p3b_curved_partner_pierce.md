# SPEC — #169 P3b (F0082): curved-partner pierce mint — the ellipse×wall corner

Status: **DESIGN + increment 0 (probe) MEASURED**. Grounded in the #181/inc-3c
characterization (`specs/yang_146_conformal_junction_sampling.md` §"Blocker (2)
CHARACTERIZED", memory `session_2026_07_19_146_inc3c_f0082_corner_class`) and
this spec's own increment-0 probe run (§2). Companion to — NOT a replacement
for — the #137 grazing-corner spec (`yang_137_torus_plane_grazing_corner.md`):
this spec covers the TRANSVERSAL corner class reachable by the Stage-1 mint
mechanism; #137 keeps the tangential/grazing class that needs refinement plus
the output-level stitch.

## 0. Scope

- **In scope:** a `LineSegment` boundary edge of one operand (incident to two
  PLANAR faces — the existing P3a owner-edge channel scope) transversally
  piercing a bounded CYLINDER lateral face of the other operand. Corpus
  driver: **F0082 Extrude-11** (`TessellationFailed "ring rejected by CDT"`,
  kernel-v2 FaceId 3716 / yang output face 362).
- **Out of scope (fail-closed skips, later increments):** cone/sphere/torus
  partner faces; curved-incident OWNER edges; holed/partial cylinder laterals
  as pierce targets (canonical full tubes only first); the tangential/grazing
  corner class (#137 part (b) — the transversality floor routes it there).

## 1. Contract grounding

The junction research findings (`docs/yang_junction_research_findings.md`)
bind all P3 specs:

- **Q4 corollary:** prevention lives at the SAMPLING/MINT layer — the
  arrangement is exonerated (it faithfully preserves whatever the Stage-1
  meshes carry, N48 sidecar-certified). A never-minted junction is a Stage-1
  sampling defect.
- **Q1 (corner taxonomy):** a boundary point where an intersection curve
  exits a face boundary invokes the junction path: mint the corner ONCE,
  insert into both operands as one shared arrangement vertex.
- **The junction contract:** mint once exactly; share by identity (same
  bits, both meshes); multiplicity below resolution is a loud STOP, never a
  fuzzy merge; refinement is not a lever here (the pierce is transversal).

Yang 2025 anchor: §4.4.1's `r_A = r_B = r` shared-junction precondition
(`refs/text/yang2025_hybrid_boolean.txt:551-554`) — the same clause P3a
implements for mid-edge pierces; this spec extends it to the corner class
where the pierced partner surface is curved.

## 2. The case, precisely (F0082 Extrude-11 — MEASURED, increment 0)

From inc-3c (#181): the union output face 362's cyl∩plane section
`Curve::Ellipse` arc (r≈0.2124) terminates at output vert 913 — a relocated
cylinder chord-ring crossing vertex, ON the ellipse to 4.4e-16 but at the
canonical parameter t≈π/2 — instead of the true terminus, the ellipse ×
wall-plane corner at t=1.5578, exact point
`(-0.06399183, -0.10911126, 2.10955341)`, 2.76e-3 away along-curve. The arc
overshoots the wall segment (x≈-0.063992) by 1.29e-3 in-face; the ring
self-intersects; the #173 render gate STOPs loudly. `YANG_INPUT_VERT_PROBE`
zero-hits: the defect is minted by this union, not inherited.

**Increment-0 probe (`YANG_P3B_PIERCE_PROBE`, this spec, read-only —
measured 2026-07-19 on the live F0082 chain):** the corner IS an enumerable
edge×face pierce:

```
[p3b-pierce] A edge 2424 (owner_planar=true) × cyl face 2 (r=0.212325):
    t=0.232061 J=(-0.063991829, 0.092341791, 2.113152675) transv=0.474
    t=0.767345 J=(-0.063991829,-0.109111255, 2.109553406) transv=0.474
```

- The t=0.767 root **matches the inc-3c true corner to 9 decimals**.
- The owner edge (operand A's wall edge 2424) is `owner_planar=true` —
  already inside the P3a owner-edge channel scope. Only the PARTNER side
  (operand B's cylinder face 2) is out of scope today, at the two documented
  gates: `junction.rs` "planar partners only" (`line_edge_plane_face_pierce`
  early return) and the ALL-LINE partner-loop restriction.
- Transversality 0.474 — well-conditioned, nowhere near the 1e-9 tangency
  floor. This is NOT a grazing corner; no refinement is needed. The #137
  Urick-stitch machinery is the wrong (heavier) tool for this class.
- The t=0.232 root is the arc's other-side wall crossing (the v915/near-dup
  8.5e-4 region of the same rejected ring) — the same mint fixes both ends.

**Why P3a's proven mechanism transfers:** once J carries identical exact bits
as a vertex in BOTH Stage-1 meshes, the arrangement dedups them into one
shared vertex and the intersection polyline threads it — the identical
mechanism proven at F0082's v588/v601 site (inc-2 measurement). J lies on
face-362's plane AND on the cylinder ⇒ J is ON the section ellipse exactly
(Stage-4 relocation residual ~0), and on the wall plane ⇒ the output ring's
arc/wall chains meet AT J.

## 3. Design

### 3.1 Pierce primitive (line × cylinder)

`line_edge_cylinder_face_pierce(p0, p1, s1, s2, f_idx, f, y) -> Vec<PiercePoint>`
mirroring `line_edge_plane_face_pierce` gate-for-gate:

- Roots of the quadratic `|w(t)|² = r²` (w = radial component of p(t) − axis)
  in `(0,1)` — up to TWO genuine pierces per edge×face (unlike the plane
  case; both are minted, subject to the gates below).
- Transversality `|t̂ · n̂(J)|` with the radial outward normal at J; same
  `TRANSVERSALITY_MIN = 1e-9` floor → tangential contacts route to #137,
  never minted (fail closed).
- Endpoint margin `TAU_MODEL·(1+scale)` — a pierce at an owner-edge endpoint
  is a higher-order corner (vertex-on-surface), P3b-later territory.
- On-surface postcondition `TAU_EVAL·(1+scale)` for the owner's two incident
  planes at J (producer-fault guard, identical to the planar arm).
- The `junction_stage1_overrides` sub-weld cluster filter applies unchanged
  (the two F0082 roots are ~0.2 apart — far above any band).

### 3.2 Containment on the bounded cylinder face (canonical tubes first)

The planar arm's 2D chord-polygon containment does not transfer. For a
**canonical full-tube lateral** (the `tessellate_lateral_face` hole-free
"2 FULL-circle rims" arm — F0082's face-2 shape): azimuth is always
contained; containment is the axial interval `v_J ∈ (v_rim0, v_rim1)` with
the same `TAU_MODEL·(1+scale)` boundary margin (a pierce within the margin
of a rim plane is a rim-corner — P3b-later, fail closed). Exact: the rim
planes are analytic. Partial-arc strips and holed laterals: fail-closed
skip this increment (unroll-space containment is a later widening).

### 3.3 Partner-side insertion into the cylinder Stage-1 mesh

The planar face channel (`cdt_polygon_with_holes_keep_interior` Steiner
mint) does not apply to the structured tube grid. Insertion for canonical
tubes: locate the containing grid triangle in the (θ, v) unroll of the tube
tessellation and split it into a 3-fan around J — J's EXACT bits become the
new mesh vertex (source `BRepFace{face, u, v}`), grid untouched elsewhere.
Fail-closed non-degeneracy gates, mirroring the planar margins:

- J within the weld band `TAU_MODEL·(1+scale)` of an existing mesh vertex →
  skip the mint on BOTH sides (multiplicity guard; status quo, never worse);
- J within the band of a grid EDGE → split the edge's two incident triangles
  (2+2 fan) instead of a degenerate 3-fan — or, first increment, skip
  fail-closed and measure whether F0082 needs it.

The owner side needs NO new machinery: the existing
`rebuilt_with_junction_overrides` edge-polyline splice carries J into both
copies of the owner edge (per-loop fan-out, proven by the P3a fixtures).

### 3.4 Non-goals

- No tolerance merges, no band widening — every gate above is the existing
  derived margin vocabulary (R0091 discipline).
- No output-level ring surgery: if the mint does not resolve the ring, the
  #173 gate keeps STOPping loudly (expected: chained models carry layered
  defects; the inc-2/3a/3b history says expose-the-next-layer is normal).
- The relocated chord-crossing vertex (v913-class, t≈π/2 beyond J) is NOT
  deleted by this spec: with J present it lands on the discarded side of
  the wall; if a residual sliver survives reassembly, that is a loud STOP
  naming the next increment — never a silent trim.

## 4. Oracles & verification

- Unit: pierce primitive pins F0082's two J's (9-decimal fixture from §2);
  containment red/green at the rim margins; insertion fixture proves both
  rebuilt operands carry J bit-exactly as closed 2-manifolds (the
  `p3a_junction_wiring.rs` end-to-end contract, cylinder edition).
- Gate-OFF full assay byte-identical (increments 1–2).
- Gate-ON: 0-WRONG ratchet; F0082 Extrude-11 ring-reject cleared or the
  next defect layer exposed LOUDLY; zero regressions; Stage-0 seam suite +
  Cherchi sidecar parity (arrangement input changes — same ledger as P3a
  inc-3).
- Always-on flip only on the standard ledger (the P3a inc-3 precedent).

## 5. Increments

- **inc-0 — DONE (this session): probe.** `YANG_P3B_PIERCE_PROBE` banked in
  `junction_pierce_points` (read-only): enumerates line×cylinder pierce
  candidates. Measured on F0082: the corner enumerated exactly (§2); ~250
  candidate lines across the whole 11-op chain (scope is modest).
- **inc-1 — pierce primitive + tube containment. DONE (this session).**
  Production-shaped `line_edge_cylinder_face_pierce` (all gates: canonical-
  tube vocabulary, quadratic roots, endpoint/rim margins, transversality
  floor, owner on-surface postcondition, exact axial containment via the
  rim planes), called only by the probe's new MINT arm + 6 unit fixtures
  (`tests_unit/p3b_cylinder_pierce.rs`, `rj_cylinder` canonical tube:
  analytic two-root mint, tangential/endpoint/axial/off-owner/vocabulary
  fail-closed rejections). **Measured on live F0082: 4 MINTs on cyl face
  2** — the lead corner (edge 2424 t=0.767345,
  J=(-0.063991829,-0.109111255,2.109553406)) plus three more ring-corner
  junctions (edges 2422/2425, matching the rejected ring's other
  wall-crossing coordinates at x≈0.3095 / y≈-0.1966). Edge 2424's t=0.232
  root is axially OUT of the bounded tube span (fail-closed reject) —
  consistent with the section curve exiting that end via the tube RIM
  (v915 sits 0.0125 inside the wall), i.e. that terminus is a rim-corner
  junction, not a wall pierce; the rim-corner arm stays a later widening.
- **inc-2 — tube-grid insertion channel. DONE (this session).** The
  face-override pre-pass gains a CYLINDER arm (on-surface radial check
  1e-9·(1+scale), mint with the `eval_source`-matching
  `BRepFace{u: azimuth, v: axial}` source) and the cylinder dispatch arm
  splices each interior point into the freshly-emitted lateral triangles:
  containing-triangle 3-fan in the J-re-centered isometric chart
  `(u=r·Δθ, v)` (branch cut half a turn away; quarter-turn straddle
  guard), exact bits, winding preserved, all other triangles untouched.
  CONSUMED postcondition all-loud: not-contained / ambiguous /
  weld-band-of-boundary placements are `MalformedTopology` errors — never
  a silent drop (one-sided mint) or a sub-band sliver fan (the R0091
  hazard); sub-band routing is the inc-3 wiring pre-filters' job.
  5 fixtures (`tests_unit/p3b_tube_insertion.rs`) incl. the cross-operand
  contract: the inc-1 primitive's own J through the owner EDGE channel
  (box) + partner FACE channel (tube) lands bit-exactly in BOTH rebuilt
  Stage-1 meshes as closed 2-manifolds. The superseded P3a fixture
  `face_override_nonplanar_target_is_loud` now uses a SPHERE as its
  out-of-scope representative. Production-unreachable by construction
  (no production caller emits cylinder face overrides until the inc-3
  wiring, which carries the env gate) — full assay byte-identical.
- **inc-3 — wire + measure. WIRED gated-off (this session), gate-ON
  measured:** cylinder partners join `junction_pierce_points` behind
  `YANG_P3B_PIERCE_ENABLE` (gate-OFF the arm is dead — byte-identical,
  assay-verified).

  **Gate-ON F0082 (the target): the MECHANISM WORKS at the defect site.**
  The failing union mints (3 geometric edges × 2 copies + the pierced
  cylinder face), and the rejected ring — now 8 verts, was 16 — carries
  the EXACT minted corner J=(-0.063991828646, -0.109111255316,
  2.109553406435) as the wall chain's terminus. The case still STOPs
  (same FaceId 3716 ring-reject, loud) on the PREDICTED §3.4 residual:
  the relocated chord-crossing vertex (-0.0652822, -0.1066743) at
  ellipse-param t≈π/2 — 1.29e-3 BEYOND the wall, i.e. outside the arc's
  kept range past the minted corner — survives in the ring between J and
  the arc interior, and the segment J→phantom→arc re-crosses the wall
  chain. Root: the Stage-4 ELLIPSE relocation arm has no bounded-face
  containment gate (the torus arm's KV6d check, `stage4_correct.rs:4119`,
  has no ellipse counterpart), so a sample whose true curve position is
  outside the bounded face relocates out of it silently. Next increment
  (spec-first): beyond-corner sample resolution — a section-curve sample
  relocated past a minted corner junction is on the TRIMMED side of the
  curve; remove it conformally (ALL incident output faces — an output
  edge collapse J↔phantom, a topological trim, NOT a tolerance merge at
  2.76e-3) or STOP loudly via a new ellipse-arm containment gate.

  **Gate-ON full ledger: 250C/0W/56E — 0 WRONG holds; ONE regression:
  R0061 CORRECT→ERROR** (`reassembled output would be non-2-manifold`,
  loud). R0061 is the #145 chained-zigzag model: gate-OFF the failing
  subtract mints NOTHING; gate-ON it mints densely (112 owner-edge
  copies + 1 face — the chained operand's sampled polylines each pierce
  the subtract cylinder). The dense-mint reassembly break is the inc-4
  blocker to characterize (the P3a inc-2 F0016/F0084 pattern: chained
  models expose the next layer under insertion; expected, loud, named).
- **inc-4a — R0061 CHARACTERIZED (measured 2026-07-19) + the
  moved×minted §4.3 weld.** The R0061 gate-ON break is NOT an insertion
  defect and NOT the F0016 in-boolean sub-weld mint class — it is the
  SAME structural class as F0082's beyond-corner phantom, at identity
  distance. Probe chain (`NONMANIFOLD_SITE_PROBE` wedge-dump +
  `YANG_JUNCTION_MINT_PROBE=v` + `CHERCHI_VERT_PROVENANCE=1e-12` +
  `YANG_V_PROBE=173,186` on the failing subtract):
  - Output vert 186 IS a P3b mint (edges 2447/2449,
    J=(0.1451332743051347, 0.0373125820582086, 0.08632893203547327),
    exact bits carried — the mechanism worked).
  - Output vert 173 is an arrangement chord-crossing vertex
    (pre-relocation ~4e-4 from J) that Stage 4's **`ell_junction` arm**
    relocated onto the junction of the two adjacent wall-planes' section
    ellipses. That junction IS geometrically the pierce corner (the
    planes' shared line is the owner polyline edge; its cylinder pierce
    is the section-curves' crossing), computed via plane-pair×cylinder
    arithmetic (`stage4_correct.rs:3279`) instead of the mint's
    line×cylinder quadratic → lands **1e-15 from J** (machine roundoff
    of two exact-intent computations of ONE junction).
  - `CHERCHI_VERT_PROVENANCE=1e-12` reports ZERO pairs at emission —
    the twins are NOT coincident at arrangement level; the coincidence
    is CREATED by Stage-4 relocation. Gate-OFF worked by accident: with
    no mint, the relocated chordal corner WAS the corner's only copy.
  - The pair is ineligible for the N56 §4.3 coincident weld
    (`weld_coincident_relocated` is `moved`×`moved`; the mint is
    unmoved), so the twins survive → sliver tri [173,186,188] →
    `s6-wedge-walk-not-outgoing` at v173 → loud non-2-manifold STOP.

  **Fix (this increment): extend the §4.3 weld to `moved`×`minted`
  pairs.** Thread the minted junction points' bit-keys from `boolean()`
  (the `jo` override set) into `stage4_relocate_and_correct`; map to
  mesh vert ids by bit-exact match; pair eligibility becomes
  (moved×moved) OR (moved×minted) at the SAME `TAU_MODEL·(1+scale)`
  band; **survivor = the minted vert always** (its bits are the shared
  cross-operand junction identity — the mint may never move, N54).
  minted×minted pairs stay INELIGIBLE (sub-band mint multiplicity is a
  contract violation that must stay loud). This is the same Yang §4.3
  "remove a point too close to another on the same loop" op — the
  minted junction is on the section curve by construction, the relocated
  vert by relocation; merging removes a redundant curve point, not a
  tolerance. Note the P3a mints are always-on in production, so this
  extension is live gate-OFF too — measured by full assay, not assumed
  byte-identical. Rim-junction mints (`junction_boosted`) are a possible
  later widening, not in scope.

  **BUILT (this session), three pieces — measured layer by layer:**
  1. The (3b′) weld extension alone did NOT fire: the §4.5.3 sweep
     (`sweep_reversed_intersections` → `compute_phase_a`) walks patch
     boundaries on the post-relocation mesh BEFORE (3b′), and the wedge
     walk died on the twins there. The weld must run pre-sweep.
  2. **Pre-sweep moved×minted weld** (placed after the PR-KV9 bit-dup
     collapse, before the sweep; restricted to moved verts within the
     band of some mint so no moved×moved pair is reordered). Fires on
     R0061: 10 collapses, incl. the diagnosed v173→v186 (survivor = the
     mint). Cleared layer 1; the walk then died at v211 — a mint that is
     a legitimate **4-strand crossing** (the two wall-plane section
     curves crossing at the pierce corner: 4 in + 4 out boundary edges)
     — with an earlier cycle having consumed the wedge continuation.
  3. **Orbit-based boundary-cycle extraction** (stage5_topology): the
     old chain walk began cycles at the lowest start VERTEX (first edge
     picked without wedge pairing when starting at a crossing) and
     closed on first return to the start vertex (no wedge check at
     closure) — both stitch lobes wrongly at a 4-strand crossing, and a
     partially-consumed crossing could also masquerade as unambiguous
     (dynamic out-degree 1). Replaced with the successor-map orbit
     extraction: every directed boundary edge pairs with its
     wedge-consistent continuation (static out-degree; sole-out fast
     path preserved), the map must be a bijection (violation = new loud
     site `s6-wedge-succ-collision`), cycles are its orbits (closure at
     the EDGE level, no start heuristic). Byte-identical cycles for
     simple boundaries; the C0058 pinch fixtures stay green.

     **Measured limitation → legacy fallback (KV9-F1 Steinmetz).** The
     pure orbit walk broke `steinmetz_subtract_passes_stage4_with_
     volume_oracle`: at the Steinmetz TANGENCY GENERATOR, FOUR patch
     sheets share the tangency edge and all four are mutually tangent,
     so `wedge_continuation`'s fan rotation (which assumes 2 tris per
     interior edge and has no radial sort) can emerge in the wrong
     sheet — first-order dihedral sorting DEGENERATES at a tangency, so
     a correct radial sort there must be curvature-aware (a future
     spec-level increment). The legacy chain walk passed these fixtures
     only by consumption ORDER (dynamic out-degree hid the crossing
     from the wedge rule) — validated by the volume oracle, so it is
     kept as the FALLBACK: orbit first; on any resolution failure
     (deadend / not-outgoing / succ-collision) the patch re-runs the
     byte-identical legacy walk; double failure keeps the legacy loud
     error taxonomy. `[wedge-orbit] unresolvable, legacy fallback`
     (probe-gated) marks each fallback for measurement.

  **R0061 after all three: still a loud STOP, one layer deeper** — the
  orbit attempt reports `s6-wedge-succ-collision: edge (145,288)
  claimed by 2 strands` (v145 is another mint, previously a §4.5.3
  reversal-collapse participant; the two strands' wedge continuations
  genuinely collide there), then the legacy fallback re-fails loudly.
  The dense-mint chained model keeps exposing the next layer, per the
  §6 risk note; the succ-collision fingerprint is the next
  characterization target. F0082 unchanged in both gate states (its
  phantom is at trim distance — inc-4b).

  **inc-4a ledger (2026-07-19): SHIPPED always-on.** Gate-OFF full
  assay 251C/0W/55E/2T, `results.json` per-case BYTE-IDENTICAL to the
  committed production baseline; rewrite tier green (incl. the three
  KV9-F1 Steinmetz volume-oracle fixtures via the legacy fallback); 398
  yang-rs lib tests green (5 N47 weld fixtures incl. the new
  moved×minted survivor pin and the minted×minted loudness pin); fmt +
  clippy clean; WASM bundle rebuilt. Gate-ON: R0061 ERROR (loud,
  next-layer), F0082 ERROR (unchanged, inc-4b's target).
- **inc-4b — beyond-corner conformal trim. SHIPPED always-on (2026-07-19);
  "greens F0082" REFUTED by measurement — F0082 is a deeper class (below).**
  Premises measured on live F0082 via `YANG_P3B_TRIM_PROBE`: the phantom
  (mesh v917, moved) IS mesh-edge-adjacent to the mint (v919) at d=2.76e-3,
  beyond the wall by 1.29e-3.
  The phantom class: a RELOCATED section-curve sample beyond a minted
  corner junction on the same curve, in a region the RESULT does not keep
  on its boundary — zero kept content ⇒ remove topologically (collapse
  phantom→J, survivor = the mint, `collapse_vertex`).

  **Predicate (all derived, no new tolerance):** for a mesh edge (m, v),
  m a Stage-1 minted junction vertex, v `moved` (relocated onto its
  section curve) and not itself a mint, with the mint's owner planes
  (n̂₁,d₁),(n̂₂,d₂) threaded from pierce time:
  - (a) **beyond-corner, op-resolved**: signed distance dᵢ(v) = n̂ᵢ·v + dᵢ
    > `TAU_MODEL·(1+scale)` for an owner plane i whose RESOLVED verdict
    (below) says "beyond has zero kept content";
  - (b) **on-curve/on-the-other-plane**: |dⱼ(v)| ≤ `TAU_EVAL·(1+scale)`
    for the other owner plane j — v is a section-curve sample of
    (partner surface × plane j), so the segment m→v leaves the bounded
    face AT the corner m;
  - (c) **corridor cap**: |v−m| ≤ `tangent_plane_corridor(d_ε, sinθ)`
    with sinθ = dᵢ(v)/|v−m| — the chord-crossing displacement bound.
    A beyond-corridor vert may be LEGITIMATE far-side geometry (the
    owner plane is infinite; a non-convex face can re-enter its
    positive half-space away from this corner) → NO fire, status quo
    (the downstream #173/ring gates stay loud). Never a false wall;
  - (d) **patch-subset guard** (the F0082 cap-ring lesson, measured):
    attributed-patch(v) ⊆ attributed-patch(m), else NO fire — a collapse
    reroutes EVERY patch incident to v onto m, so a candidate carrying a
    face the mint does not touch (F0082's phantom also anchors B's
    near-coplanar CAP ring, 1e-4 off the mint) would drag that ring onto
    a foreign point (measured: `s6-planar-loop-nonplanar` at 1.05e-4 —
    the guard converts that silent-wrong-shaped collapse into a no-op).

  **Geometric verdict at the pierce (`material_beyond: Option<bool>` per
  plane):** under this B-Rep's loop convention ("CCW viewed from outside
  ALONG the face normal" = looking in the +n direction; planar
  `Plane.normal` outward, `reversed == false`) face material at a directed
  edge copy lies to the RIGHT of travel: u = t̂ × n̂ — pinned EMPIRICALLY
  against `rj_box` by the `box_pierce_provenance_is_convex_and_on_plane`
  fixture (the first material-left draft read every convex box edge as
  reflex and the pin caught it). For plane i: take the edge copy in the
  face on plane j; s = n̂ᵢ·(t̂ⱼ × n̂ⱼ); s > 1e-9 ⇒ reflex
  (`Some(true)`: material extends beyond), s < −1e-9 ⇒ convex
  (`Some(false)`), else undetermined (`None`, inert).

  **Op resolution (`resolve_trim_beyond`, pinned by the
  `resolve_trim_beyond_pins_the_op_owner_table` fixture):** zero-content-
  beyond depends on the op and the owner side — Union: reflex only
  (beyond = interior of the union; F0082's measured union fires are
  reflex rising-wall corners); Subtract A−B: owner A convex only (beyond
  = outside the result), owner B reflex only (beyond = carved away;
  R0061's measured 19 fires); Intersect: convex only; Xor: never;
  undetermined: never.

  **Wiring:** `PiercePoint` carries `owner_planes: [PierceTrimPlane; 2]`
  (geometric); `junction_stage1_overrides` records per-mint
  `PierceProvenance { owner, planes }`; `boolean()` resolves against the
  op into `MintProvenance` ([MintTrimPlane; 2]) and the Stage-4 mint
  channel widens from a bit-key set to `BTreeMap<[u64;3], MintProvenance>`;
  the trim pass runs at the pre-sweep site immediately AFTER the
  moved×minted weld (weld owns ≤ band; trim owns band→corridor),
  fixed-point iterated. Live gate-OFF too (P3a planar mints thread the
  same channel) — measured by full assay: gate-OFF 250C/0W/55E/3T,
  per-case category-identical to the committed baseline except the known
  F0090 timeout flake. 9 unit fixtures
  (`tests_unit/p3b_beyond_corner_trim.rs`).

  **Measured outcomes (2026-07-19):**
  - **F0082: the trim hypothesis is REFUTED for this case — it is a
    3-junction micro-complex, not a lone phantom.** The phantom fires
    eligibility (reflex wall × union) but the patch guard correctly
    blocks it: B's extrusion CAP is near-coplanar with A's top face
    (~1e-4 at the corner; sub-Stage-0 band), the cap rim and the section
    ellipse osculate at t≈π/2, and the phantom is simultaneously (i) the
    beyond-corner ellipse sample, (ii) a CAP-ring boundary vertex, and
    (iii) the chord stand-in for TWO unminted junctions: J2 = cap-rim ×
    wall-face (a CURVED-owner pierce — the rim is a Circle edge) and
    J3 = tube ∩ cap-plane ∩ top-plane (the ellipse×rim crossing). An
    unconditional collapse dragged the cap ring 1.05e-4 off-plane
    (measured `s6-planar-loop-nonplanar`); with the guard the case
    keeps its honest FaceId-3716 ring-reject STOP in both gate states.
    The fix vehicle is the **curved-owner/rim-corner mint widening**
    (inc-4d) — mint J2 via circle-edge × planar-face pierce (rim
    polyline + wall-face interior insertion), after which the phantom
    becomes a true zero-content sample.
  - **R0061: 19 trims fire correctly (subtract, tool-owner reflex
    corners, all on-curve to ≤1e-16), one layer deeper again** — the
    orbit walk now dead-ends at mint v211 (`s6-boundary-walk-deadend`,
    2 incoming / 0 outgoing). Root measured via the deadend wedge-dump:
    the kept set carries an OVER-USED minted×minted mesh edge (211,186)
    — THREE same-patch triangles, two of them same-winding slivers
    spanning from the mint pair to near-dup tips (v172/v184, 3e-5–1.7e-4
    apart — above the weld band, below sampling) on the cyl×ridge-face
    section arc (B's zigzag ridge cap, face 394; its side-edge pierces
    ARE minted at the arc's ends). This is the Stage-4 non-2-manifold
    bucket one layer deeper: a same-winding fold with UNFUSED near-dup
    tips (the inc-3a collapsed-wedge class generalized) — inc-4c below.
- **inc-4c — R0061 fold resolution: §4.4.1 post-merge fan
  re-triangulation (own spec `yang_169_p3b_inc4c_fan_retriangulation.md`;
  inc-4c-1 SHIPPED always-on 2026-07-20, fail-closed).** Measurement
  REFUTED the fold-dedup sketch above (the extra triangles have DISTINCT
  tips — deletion leaves holes; the collapse stack is the manufacturer:
  pre-weld/trim the mesh is manifold, and every pre-edge crossing the
  victim-partition cut collapses onto the mint-pair edge). The fix is
  connectivity-only local re-CDT of the merged fan regions
  (`retriangulate_collapsed_fan_regions`, seam-pinned defective edges
  constrained, pinch verts paired by combinatorial fan chains, expected-
  multiplicity postcondition; 5 unit fixtures). R0061 gate-ON now bails
  LOUDLY at the CDT on the newly-characterized next layer: the seam
  polylines carry Stage-4 RELOCATION-ORDER zigzag needles (samples
  relocated onto the analytic curves out of order; self-crossing at
  ~1e-8 transverse) — resolved by **inc-4c-2** (same sub-spec, SHIPPED
  2026-07-20): cluster-level seam-run canonicalization (sort connected
  same-key-pair chain components by the pair's analytic curve parameter;
  disorder-triggered bounded anchor growth; §4.3 sub-render redundant-
  sample drop — deviation N58, drop criterion pending ratification).
  **R0061 gate-ON = SUPPORTED_CORRECT — the inc-4c flip blocker is
  CLEARED.** The always-on flip (inc-5) now awaits the F0082 decision
  (inc-4d greens it or its honest STOP is accepted) plus the standard
  flip ledger (gate-ON full assay regression set, sidecar parity).
- **inc-4d — curved-owner / rim-corner pierce widening (spec-first;
  geometry MEASURED 2026-07-21, see §7).** Circle-rim-edge × planar-face
  pierce mint under the identical junction contract: closed-form
  circle∩plane roots, seam-vertex margin, transversality floor, 2D
  all-line containment on the partner face; owner-side insertion into
  the rim RING (both incident faces conformal by construction: the cap
  CDT and the lateral strip both consume the shared ring — the
  `rim_overrides` Stage-1 channel, task-#143 vintage) + partner-side
  planar face-interior insertion (the existing P3a channel). The
  rim/edge/face override kinds compose inside ONE
  `stage1_tessellate_inner_overrides` rebuild (the tessellator already
  accepts all three maps; only the `BRep` wrapper + wiring are new).
  Full design + measured F0082 geometry: **§7**.
- **inc-5 — always-on** per the standard ledger once inc-4c clears
  R0061 gate-ON; then scope widenings (strip/holed laterals, cone
  partners, edge-split arms) as separate measured increments.

## 6. Risks & guardrails (P9/P10)

- **Layered defects:** F0082 is a chained multi-defect model (inc-2
  history). The mint may expose an over-use/next-layer failure rather than
  green the case — that outcome is a CORRECT result of this spec (loud,
  named, next increment), not a refutation of it.
- **The t-ordering nuance (§3.4):** J (t=1.5578) sits INSIDE the arc's
  kept range; the chord-crossing relocation target (t≈1.5708) is beyond
  the wall. If the labeling does not discard the beyond-wall sliver
  cleanly, the §4.3 dedup / I6 wedge machinery may absorb it — measure,
  never special-case.
- **Grid degeneration near rims/seams:** the fail-closed margins skip the
  mint rather than fan a sliver (missed mint = status quo). Any STOP the
  skip leaves standing is the case's pre-existing state.
- **No global re-tessellation:** the 3-fan is local; rings, rims, and seam
  rulings stay byte-identical outside the containing triangle (the N54
  lesson: never move existing coordinates).

## 7. inc-4d — circle-rim owner pierce (design, measured 2026-07-21)

### 7.1 The failing union, definitively attributed

`YANG_RUN_PROBE` + the group/wire probes on the live F0082 chain (15
boolean runs) pin Extrude-11's failing auto-union as the run
`op=Union a: 883v/367f b: 2v/3f`:

- **A = the accumulated body** (owns the top×wall boundary edges — the
  edge-2424 group with top plane `n=(0.05062681, -0.01784078,
  0.99855828), d=-2.10521894` and wall plane `n=(-0.99871764,
  -0.00090438, 0.05061873), d=-0.17079136`).
- **B = the fresh canonical tube** (2 vertices / 3 faces: lateral +
  two DISC caps; rim-0 `Circle` center `(0.1227322098851793,
  -0.008327366889270053, 2.1018871743865217)`, normal = ∓axis
  `(0.06821306, -0.05163710, 0.99633357)`, r `0.2123252664164556`).
  The rims ARE `Curve::Circle` edges with `start == end` — the
  curved-owner class.

(The later runs where a tube appears with POLYLINE rim chords are
post-failure chain ops on the standalone body — not this increment's
target.)

### 7.2 The micro-complex, closed-form

With the probe-pinned planes and rim circle (exact circle∩plane roots):

- **J** (minted since inc-3) `(-0.063991829, -0.109111255, 2.109553406)`
  — ellipse×wall; sits `+1.0529e-4` axially INSIDE the tube span from
  the cap plane (so the ellipse, not the cap section, is the live
  boundary at this corner).
- **J2 = rim-0 × wall plane** `(-0.063997163, -0.109109265,
  2.109448193)` — `1.0537e-4` from J (straight down the wall).
  In-face: `~1.05e-4` below the wall face's top boundary edge — far
  above the `TAU_MODEL·(1+scale) ≈ 3.2e-7` boundary margin, and far
  above the sub-weld cluster band vs J. **This is the inc-4d mint.**
  On the wall face, B's section = (wall∩lateral curve) ⌣ (wall∩cap
  line) joining AT J2; the cap ring crosses the wall exactly there.
- **J3 = rim-0 × top plane** `(-0.065282249, -0.106674345,
  2.109662370)` — matches the inc-3 "relocated chord-crossing phantom"
  `(-0.0652822, -0.1066743)` to every printed digit: the phantom IS the
  true tube∩cap∩top triple point. But it lies `1.291e-3` BEYOND the
  wall — **outside A's bounded top face**, where B's rim is entirely
  outside A: crossing A's top PLANE there is not a boolean event.
  **J3 is NOT an output junction, and the partner-side bounded-face
  containment gate excludes it automatically.** No J3 mint — by
  design, not by limitation. (The phantom's fate: with J2 minted the
  cap ring is exact at the wall; the relocated stand-in is expected to
  land on the trimmed side or weld onto exact geometry — measured, not
  assumed.)

### 7.3 Design

**Pierce primitive** `circle_edge_plane_face_pierce` (junction.rs,
mirroring the line arms gate-for-gate):

- Owner edge: `Curve::Circle` with `start == end` (full rim; arc rims
  are a later widening), incident to exactly TWO distinct surfaces
  (any mix — the canonical tube rim is Plane cap + Cylinder lateral).
- Partner face: `Surface::Plane` with ALL-LINE loops (reuse the
  existing exact 2D containment + boundary margin verbatim).
- Roots: solve `n·p(θ)+d = 0` on `p(θ) = c + r(cosθ·u + sinθ·v)` —
  `A·cosθ + B·sinθ = C` with `R = hypot(A,B)`; `R < |C|` ⇒ miss;
  up to two roots. Near-tangency guard: root pair closer than
  `TAU_MODEL·(1+scale)` in 3D ⇒ treat as tangential, no mint (the
  A14.2 rule the Case-IV `circle_line_roots` also applies).
- Transversality: `|t̂(θ)·n̂|` with the circle tangent at the root;
  same `TRANSVERSALITY_MIN` floor (F0082's J2: ≈0.475 — well clear).
- Seam-vertex margin `TAU_MODEL·(1+scale)`: a root near the rim's own
  B-Rep seam vertex is a higher-order corner — fail closed (mirrors
  the line arms' endpoint margin; also required by the ring builder's
  seam-slot contract).
- On-surface postcondition `TAU_EVAL·(1+scale)` on both owner incident
  surfaces at the root (producer-fault guard, identical).
- `PiercePoint.t` = seam-relative angle normalized to `[0,1)` (sort
  key only). Trim provenance (`owner_planes`): the owner's incident
  surfaces include a non-plane ⇒ `owner_trim_planes` yields the
  fail-closed default — the Stage-4 beyond-corner trim stays inert for
  rim mints this increment.

**Owner-side insertion — the `rim_overrides` Stage-1 channel** (already
production for M8 disc-rim crossings): `JunctionStage1Overrides` gains
`rim_a`/`rim_b: BTreeMap<u32, Vec<Point3>>`; the builder routes
circle-owner pierces there, fanned to EVERY copy of the geometric rim
(grouped by center/normal/radius/seam bits — same conformality-by-
identity rule as the line-edge fan-out; identical override lists keep
per-index cached rings identical under either edge-sharing convention).
The ring builder's loud contracts (on-circle band, seam-bit authority,
distinct-override slot collision) apply unchanged; our closed-form
roots are on-circle to machine precision.

**Opposite-rim mirror (measured 4d-2, the first composition wall):**
the azimuth-merge lateral pairs its two rings 1:1 and REQUIRES matched
sample counts (`azimuth-merge rims have mismatched / too-few samples`,
loud). Every rim mint therefore mirrors onto the opposite rim by the
production `collect_ring_crossings` exact AXIAL projection
(`opposite_rim_projection`: strip the axial component, renormalize the
radial offset to the opposite radius — lands ON the opposite circle to
machine precision). The mirror is a plain exact ring sample, NOT a
junction (no partner-side insert, no trim provenance). A rim with no
canonical cylinder-lateral pairing (`lateral_for_cap` fails / torus
profile) skips the mint on BOTH sides — fail closed: a rim entry
without its mirror hits the loud count wall, and a partner-only insert
would be the one-sided mint the junction contract forbids.

**Partner-side insertion:** the existing planar `face_overrides`
channel (interior Steiner mint into the partner face's CDT) — no new
machinery.

**Composition:** `stage1_tessellate_inner_overrides` ALREADY accepts
(rim, edge, face) maps in one call. New: `BRep::
rebuilt_with_all_overrides(rim, edge, face)` (empty-rim ⇒ byte-
identical to `rebuilt_with_junction_overrides` — the empty-override
identity), and the two `boolean()` call sites pass `jo.rim_*`. The
Case-IV rim-junction rebuild's mutual-exclusion SCOPE GATE with the
P3a channel is UNTOUCHED (P3b rim overrides ride the P3a rebuild;
composition is within the one rebuild, not across rebuilds).

**Cluster filter:** rim mints join the global sub-weld scan unchanged
(J vs J2 at 1.05e-4 ≫ band: no poisoning).

### 7.4 The Case-IV one-sided precedent (watch-list, P9)

`rim_junctions_against` increment-4 v1 MEASURED that blanket
cylinder-rim × plane-face insertion (ONE-SIDED: rim ring only, no
partner-side mint) REGRESSED F0047/R0006/R0075/F0081 and unmasked
R0091's banked-§3b path — it is scoped to cone-flanked rims. inc-4d is
structurally different (two-sided identity mint + cluster filter +
containment margins, behind `YANG_P3B_PIERCE_ENABLE`), but those five
cases are the named regression watch-list for the gate-ON assay. A
regression there is a characterized next layer, not a silent cost —
the flip decision (inc-5) sees the full ledger.

### 7.5 Increments & ledger

- **inc-4d-1:** primitive + unit fixtures (roots pinned to §7.2's J2
  at 9 decimals on the live descriptors; tangential / seam-margin /
  containment / off-owner / two-root red-green). Unwired: assay
  byte-identical by construction.
- **inc-4d-2:** rim channel + `rebuilt_with_all_overrides` composition
  + builder fan-out + cross-operand contract fixture (J2 lands
  bit-exactly in BOTH rebuilt Stage-1 meshes — tube ring/cap/lateral
  AND partner wall CDT — as closed 2-manifolds). Production-unreachable
  until 4d-3 (no caller emits rim overrides); gate-OFF assay
  byte-identical.
- **inc-4d-3:** circle-owner arm joins `junction_pierce_points` behind
  `YANG_P3B_PIERCE_ENABLE`. Measure: gate-OFF full assay byte-identical;
  gate-ON F0082 (green or loud named next layer); gate-ON full ledger
  vs the inc-4c baseline (R0061 stays CORRECT; §7.4 watch-list; 0-WRONG
  ratchet).

### 7.6 inc-4d-3 measurements (2026-07-21)

**Gate-ON F0082: the J/J2 corner is FIXED; the STOP moved one layer down
(loud), FaceId 3716 → 3727.** The rim arm fires across the chain's tube
unions (`rim_b=2` / `rim_a=2` wire entries — own rim + the opposite-rim
mirror); the failing union now mints `edge_a=6 face_a=3 face_b=1
rim_b=2`. `KV2_OUT_VERT_PROBE` at the corner: output v937 = J EXACT and
v943 = J2 EXACT (every digit of §7.2), joined by the 1.05e-4 wall×tube
section arc; the top-face ellipse arc terminates at J; the wall∩cap
section line and the cap-ring chords terminate at J2; the old relocated
phantom is GONE from the corner neighbourhood. The original top-face
ring (old FaceId 3716 / yang 362 class) assembles.

**The next layer (FaceId 3727, the tube-side patch, ring measured via
`KV2_RING_REJECT_PROBE`, 61 verts):** in the lateral unroll chart the
bottom chain runs …→ J (u=0.328739, v≈1e-16) → J2 (u=0.328735,
v=−1.0529e-4) → a vertex at (u=0.325977, v≈−6e-14) — the **J3
position** (2.76e-3 back along u, ON the ellipse) — then forward along
the baseline. The J→J2→J3 notch folds back UNDER the incoming ellipse
chain at the same v≈0: a degenerate self-overlap the render CDT
correctly rejects. This is the §7.2 J3 = tube∩cap∩top triple point
surfacing in its REAL home: not a junction of A's top face (the §7.2
exclusion stands) but of the TUBE-SIDE patch boundary, where the kept
boundary switches between the section ellipse and the cap rim — two
curves that OSCULATE across the ~1e-4 near-coplanar cap gap. That
junction class is a **tangential curve×curve crossing on the curved
surface** (edge×edge, grazing) — outside the pierce vocabulary by
design (the transversality floor routes tangential contact to the #137
family). The remaining work is a J3-class increment: either the #137
grazing-junction machinery generalized to rim×section-ellipse
osculation, or §4.3-class resolution of the sub-scale sliver (the
rim/ellipse crescent's v-content is ≤1.05e-4 and pinches to 0 at J3).
Honest state: ERROR both gate states, one layer deeper, fully named.

**Gate-ON full ledger (vs the byte-identical gate-OFF baseline
251C/0W/55E):** 249C/1W/57E. R0061 STAYS CORRECT; watch-list
F0047/R0006/R0075/F0081 clean; F0085 TIMEOUT→ERROR (load flake class).
Three real deltas, all characterized:
- **C0102/C0103 CORRECT→ERROR (loud, the composition collision class):**
  the rim arm's ring insertions (mints + opposite-rim mirrors) shift the
  azimuth-merged lateral grid, so line-pierce interior mints that
  previously sat strictly inside a grid triangle now land ON a ruling
  (C0102: edge distance 5.55e-17 vs band 2e-7 — "guaranteed post-weld
  sliver" gate) or outside every lateral triangle (C0103: consumed
  postcondition). The §3.3 deferred "within band of a grid EDGE → 2+2
  edge-split fan" arm is now DEMANDED by live cases — next increment.
- **R0091 ERROR→SUPPORTED_WRONG (silent, THE inc-5 flip blocker):**
  χ = V(611)−E(1795)+F(1180) = −4 — the §7.4 Case-IV history repeating
  ("cut-tool rim insertions unmask the banked-§3b unverifiable-χ path"),
  now as a silent wrong. The flip requires either the structural fix or
  a production-side Euler/validity STOP that converts it to a loud
  ERROR (P10: a new silent-wrong must become a loud STOP — this is the
  legitimate use of a safety net, not a band).

Production gate-OFF is per-case byte-identical (empty results.json
diff), so inc-4d ships gated-off; the flip ledger now reads:
R0061 ✓ cleared (inc-4c), F0082 moved to the named J3-osculation layer,
C0102/C0103 = 2+2 edge-split arm, R0091 = loud-χ conversion or fix.

### 7.7 inc-4e: the flip-blocker triple (2026-07-21, task #186)

**inc-4e-1 — the §3.3 deferred "2+2 edge-split fan" arm SHIPPED
(C0102/C0103).** `splice_lateral_interior_points` now surveys, in one
pass, strict containment PLUS proximity to every grid vertex and every
undirected grid edge in the face's range. Routing: within the §4.3 weld
band of an existing VERTEX → loud error (unchanged — the pre-filters'
skip-on-both-sides multiplicity arm); within band of exactly one grid
EDGE (exactly ON a ruling — C0103's consumed-postcondition shape — or a
hair inside one incident triangle — C0102's 5.55e-17 guaranteed-sliver
shape) → split the edge's two incident triangles into a 2+2 fan (winding
preserved; exactly-2-incident enforced loudly; >1 candidate edge
ambiguity loud); otherwise the existing strict 3-fan. Unit fixtures:
on-edge 2+2, near-edge (sub-band, 1e-8 off the seam ruling) 2+2,
sub-band duplicate-of-prior-mint loud, all closed-conformal-2-manifold.
Measured gate-ON: **C0102 and C0103 both SUPPORTED_CORRECT** (were
CORRECT→ERROR loud at inc-4d).

**inc-4e-2 — R0091 RESOLVED: the meta χ was the authoring error.** The
§3b unblock path (spec `yang_453_junction_protected_collapse`) was
executed BOTH ways:

- *Sidecar reference parity:* the exact Stage-1 operand meshes of
  R0091's only boolean (subtract: box 20v/36t, tool tube 50v/96t —
  captured via `YANG_STAGE0_DUMP_DIR`; the revolve sausage is
  bbox-disjoint and never enters a boolean) fed to the Cherchi-2022
  `mesh_booleans` binary give a fully-paired 1-shell output with
  **χ = V(92) − E(288) + F(192) = −4**.
- *Authored-numbers derivation:* independent voxel-CSG (half-space box ∩
  ¬tool, N=140) gives χ_solid = −2 → boundary **χ = −4**, one component.
  Geometric mechanism: the tilted cut tube (r = 3.905e-5, axis ≈ 36° off
  the box normal) spans the box's cross-section mid-bands (half-widths
  2.153e-5 / 3.755e-5 < r) but not its corners (4.33e-5 > r) — the cut
  removes the mid-band and leaves 4 corner pillars bridging the two
  remaining slabs: genus 3. (At N=90 the voxelization disconnects a
  sub-voxel pillar — the resolution sweep to N=140 converges.)

`R0091.meta.json` `euler_target` corrected 2 → −4 (the naive 3-op
default was never derived; same protocol as R0099/R0006, new pin in
`assay_euler_consistency::historical_authoring_fixes_pinned`). Gate-ON
R0091 = **SUPPORTED_CORRECT**; gate-OFF stays its honest merge-budget
LRR ERROR (#171 class). The gate-ON χ=−4 output was never wrong — the
"silent wrong" was a wrong oracle.

**inc-4e-3 — spec-§3b ranked merge survivor WIRED always-on.** The bank
condition (unverifiable R0091 χ) is resolved, so
`sub_feature_merge_direction` (Yang Fig. 11(b): the exact vertex
survives) now picks the §4.4.1(b) merge victim. Campaign trackers
r0009/r0091 un-ignored (measured: their ellipse-endpoint walls had
already drifted to the merge-budget LRR wall — still loud ERRORs, wall
absent, ranked survivor keeps it absent). Corpus ledger: see the
measurement blocks below.

**F0082 J3 (item 3 of the triple) — honest STOP ACCEPTED for the flip.**
The rim×section-ellipse osculation layer (§7.6) is ERROR in BOTH gate
states — flip-neutral. It stays the named #137-family/§4.3-sliver
follow-up; greening it is NOT a flip precondition.

**inc-4e final ledgers (2026-07-21, full stack: 2+2 arm + meta fix +
§3b wired):**

- Gate-ON full corpus: **251C / 0W / 55E / 2T** (+2 UNSUPPORTED-coplanar,
  1 EXPECTED_ERROR, 1 UNSUPPORTED-curved-profile; T = F0072/F0090, the
  known load-flake pair). Per-case diff vs the committed gate-OFF
  baseline: **R0091 ERROR→SUPPORTED_CORRECT and NOTHING else** (modulo
  the two timeout flakes F0085 T↔E / F0090 C↔T). C0102/C0103 hold
  CORRECT, R0061 holds CORRECT, the §7.4 watch-list is clean, F0082
  keeps its honest J3 ERROR.
- Gate-OFF: see the committed `results.json` of this increment (the §3b
  wiring is the only production-reachable change gate-OFF; the 2+2 arm
  is unreachable without pierce mints and the meta only affects
  supported verdicts).

The inc-5 flip precondition (gate-ON ⊇ gate-OFF correctness, 0 WRONG,
zero uncharacterized regressions) is now MET on this evidence.

### 7.8 inc-5: the always-on flip (2026-07-21, task #187)

**Gate flipped to the P3a inc-3 pattern.** The two production arms in
`junction_pierce_points` (cylinder-partner line pierce + full-circle rim
pierce) now run by default; `YANG_P3B_PIERCE_ENABLE=off|0` disables them
purely as a dev A/B knob (compliance-ledger measurement, the
`weld_enabled` precedent). Unset = production default = on.

**Flip-exposed defect (caught by the rewrite tier, FIXED pre-flip):**
`n2_junction_cluster::i4_locality_noncoplanar_tangent_all_on_surface`
(the R0072-scale near-tangent cylinder∪box, a fixture OUTSIDE the assay
corpus) went `NonManifoldInput` gate-ON. Root: when BOTH rims of a
lateral pierce the SAME wall, each rim's OWN circle∩plane mint and the
opposite rim's azimuth-mirror (inc-4d `opposite_rim_projection`) are the
same physical point computed through different arithmetic — ulps apart
(measured 4.5e-20), never bit-equal — and the BITWISE cross-mirror dedup
kept both, manufacturing sub-weld ring near-dups (needle triangles →
i6 wedge-dedup winding REJECT → loud `NonManifoldInput`). The global
sub-weld cluster scan could not see it: it covers pierce points only,
and mirrors are synthesized later.

Fix: mirror placements are DEFERRED to a second pass after ALL own
mints, and dedup'd by BAND (`TAU_MODEL·(1+scale)`, the rim arm's own
vocabulary) instead of bits — an own mint always wins over a mirror of
the same physical point. The projection is azimuth-preserving, so paired
rims drop symmetrically and the azimuth-merge 1:1 ring counts stay
matched; a boundary-band asymmetry hits the loud count wall (fail
closed, never silent). Regression fixtures: the n2 i4 fixture itself +
`p3b_rim_insertion::both_rims_pierce_same_wall_mirror_yields_to_own_mint`
(benign-scale frame, pins own-mint bit-survival and no sub-band ring
pair).

**Flip ledger (2026-07-21, release assay, production default = ON):**

- Full corpus: **252C / 0W / 55E / 1T** (+2 UNSUPPORTED-coplanar,
  1 EXPECTED_ERROR, 1 UNSUPPORTED-curved-profile). Per-case diff vs the
  committed inc-4e gate-OFF baseline:
  - **R0091 ERROR→SUPPORTED_CORRECT** — the expected sole category flip.
  - F0090 T→C, F0085 T→E — the two known load-flake pairs.
  - F0082 detail-only: same honest J3 ring-reject STOP, FaceId
    3716→3727 (the R0044 re-index precedent class).
  - R0016 detail-only WALL DRIFT (ERROR→ERROR): render ring-reject →
    reassembly non-2-manifold. CHARACTERIZED: the deferred-mirror pass
    changed ring-vector ORDER only (probe shows zero mirror drops on
    R0016 and F0082, and zero drops ⇒ the band dedup kept exactly the
    bitwise set), so the same mint set flows in a permuted ring order
    and the case's loud STOP surfaces one layer earlier. Knob-off
    reproduces the baseline detail byte-for-byte. Flip-neutral.
- Sidecar parity: flagship `parity_native_vs_sidecar` 18/18 +
  r0046_patch_label_parity + stage0_operand_inputcheck (parity tier)
  green; yang-rs `backend_parity` 5/5 green (`--include-ignored`).
- Rewrite tier + fast tier green; clippy/fmt clean; WASM rebuilt from
  this stack and bundled in the same commit.
- The committed `results.json` baseline is now the PRODUCTION-DEFAULT
  (arms-on) ledger; the dev knob's `off` state is the A/B measurement
  side going forward (the `weld_enabled` compliance-ledger pattern).

### 7.9 The J3 layer RE-CHARACTERIZED (2026-07-21, task #188 — §7.6's
attribution REFUTED by measurement)

Post-flip re-probe of the F0082 Extrude-11 ring-reject (FaceId 3727,
`KV2_RING_REJECT_PROBE` + `KV2_OUT_VERT_PROBE` r=0.25 around J, chart
decoded: u = unroll azimuth·r, v = axial height above the CAP plane):

**§7.6 was wrong about WHERE.** The reject notch (ring idx 13–15,
chart u≈0.3287/0.3287/0.3260) is NOT the J/J2/J3 wall complex — the
identical u/v numerology (1.0529e-4 drop, 2.76e-3 span) was an exact
antipodal coincidence (the two planes' intersection line passes through
the tube axis to ~1e-9, so the gap function is an odd sinusoid). The
J/J2 wall corner sits at chart u=0.9958 (idx 22/23) and is CLEAN: J2→J
joined by the wall×tube section-ellipse arc (edge 2657), exactly as
inc-4d built it. Δu(notch↔J) = 0.667039 = HALF the unroll circumference
exactly.

**The two triple points.** The section ellipse (tube∩top) and the cap
rim (tube∩cap) cross at exactly two azimuths — the tube's two hits of
the top∩cap plane-intersection line, verified antipodal (π to 5
digits), both on all three surfaces to ≤1e-9:
- **J3 (original, θ≈2.0496)** — masked BEHIND the wall: the kept
  boundary switches ellipse↔rim through the wall arc J→J2 instead, so
  J3 is correctly not an output junction (§7.2 stands).
- **v925 (antipode)** — in FREE SPACE: the boundary MUST switch curves
  exactly there. **v925 IS an output vertex** (bit-exact in the ring,
  idx 15), shared with the cap-disc spoke edges (v925↔v928 = the rim
  CENTER — faces 362/370 triangulate the caps center+spokes). The
  "mint the osculation junction" half is ALREADY DONE.

**The actual defect = the Stage-5/6 bottom-boundary WEAVE around the
switch (output-ring class, NOT a missing mint, NOT #137 grazing
refinement):**
1. **Wrong-side rim segment**: ring idx 10–13 runs the cap rim across
   u∈[0.119, 0.326] where the rim is SUBMERGED inside A (sd_top < 0,
   down to −6.7e-3) — interior points emitted as boundary.
2. **Overshoot fold at v925**: the rim chain overshoots past v925 to a
   sample at sd_top=+1.05e-4 (idx 13), chords to a DEAD ellipse sample
   1.05e-4 BELOW the cap (idx 14 — no lateral surface exists there),
   runs BACKWARD to v925, then forward on the rim — the degenerate
   self-overlap the render CDT rightly rejects.
3. **Raw switch chords**: at u≈0.119 the ring switches ellipse→rim via
   a bare 6.7e-3 radial-plane chord with NO junction vertex and no wall
   (idx 9→10); similar structure near u≈1.327 (idx 39→40). Open probe
   question: what geometric feature (if any) selects u≈0.119.
4. **Vocabulary loss**: parts of the rim-side boundary are B-Rep
   `LineSegment` chords (edges 2611/2612: J2→v954→v955), not
   Circle/curve edges — the #158 (F6) output-rim curve-typing gap.
5. **Micro ellipse stubs at J**: the ellipse chain reaches J through
   near-dup intermediate verts v938 (3.9e-6 from J) and v932 (2.5e-5)
   — sub-scale stub edges 2658/2659 (§4.3-sliver flavored, secondary).

**Correct target boundary** (per azimuth θ, union semantics): the live
bottom curve is the ellipse where it lies ABOVE the cap plane (tube
enters A there; rim is inside A), and the rim where the ellipse falls
BELOW the cap (lateral ends at B's own cap; rim is outside A) — switch
EXACTLY at the two triple points (v925 live; J3's switch subsumed by
the wall arc). One simple monotone ring; no interior segments, no
folds, no bare chords.

**Fix vehicle:** Stage-5/6 output-boundary envelope selection for
osculating curve pairs — split both curves at the triple points, keep
the per-band winner, drop submerged/dead complements; preserve curve
vocabulary (#158 tie-in). The measured ring (61 verts, banked in this
session's probe logs) is the red fixture. **SPEC WRITTEN:**
`specs/yang_188_f0082_j3_envelope_selection.md` (task #188) — the J3
layer's plan of record from here.
