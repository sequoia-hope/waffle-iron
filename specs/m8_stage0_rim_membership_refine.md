# M8 — Stage-0 rim membership refinement (partner-feature-conservative rim sampling)

**Status:** spec (FIP Phase 1) — §2 measured mechanism COMPLETE (2026-08-11,
F0067 overlay dump); increment 1 (gated) IN PROGRESS.
**Change class:** capability increment, M8 coplanar workstream — the
flush-interface residue the §4.4.1 epic attributed here (spec
`yang_441_trim_cdt_construction.md` §4-J1 rim-trim status block; roadmap §0
item 3 → capability-tails item).
**Crates:** `yang-rs` (`stage0/rim_chords.rs`, `stage0/mod.rs`).
**Gate:** `YANG_STAGE0_RIM_REFINE` (env, default OFF — byte-identical off).
**Paper:** Yang §4.5.5 (coplanar 2D Boolean, Fig. 16) + §4.2.1
(error-bounded discretization as the foundation of *conservative*
intersection decisions). The paper's d_eps=1e-7 sampling makes mesh-level
region membership agree with the exact 2D Boolean by brute resolution; our
fixed coarse chord band does not, and this increment restores the agreement
*locally* where it is violated.

## 1. Defect statement

The Stage-0 coplanar overlay performs the §4.5.5 2D Boolean **against the
disc face's chord polygon** (the Stage-1 rim ring, e.g. a 13-gon on F0067's
r=0.208846 flush rim, sagitta 6.06e-3), not against the exact circle. Every
partner-outline feature that lies strictly INSIDE the exact circle but
OUTSIDE the chord polygon — i.e. in a chord's sag crescent — is
misclassified `AOnly` when its exact membership is `Overlap`:

- Overlay-vertex event columns subdivide the chords, but subdivision points
  lie ON the chords: the region boundary stays the chord polygon, so
  density does NOT remove the crescents.
- A partner edge entering the circle through a crescent (one endpoint in
  the crescent) crosses the exact circle once but the chord polygon ZERO
  times → **no junction is minted on that edge** → the covered outline
  content stays on the kept boundary.
- Stage 4 then relocates the rim ring onto the exact circle
  (`resolve_rim_chord_vertex` → OnCircle), sweeping the on-circle chain
  across the misclassified crescent content → the boundary weaves
  (curved-seam × plain crossings) → Stage-6 non-2-manifold.

## 2. Measured mechanism (F0067, 2026-08-11)

Failing boolean: Extrude-10 auto-union, pair `face_a=328` (gear top,
Extrude-9 gear z 1.6402→1.751898) × `face_b=0` (disc bottom, circle
r=0.20884629067185412), opposite normals, `YANG_STAGE0_DUMP_DIR` overlay
dump `overlay_008_pair328_0.txt`:

- Classes: 1540 AOnly / 795 Overlap / 697 BOnly; ring: 13 `rim_b` samples
  ON-circle + 618 mints (event columns) + 11 rimsnap.
- **126 `corner_a` vertices strictly inside the exact circle, dr =
  −3.089e-4 / −1.081e-3 / −1.339e-3 (three gear-profile radii 0.208537422 /
  0.207765007 / 0.207506897), every one classified `AOnly`.** These are the
  gear tooth root-region profile corners; their extruded wall faces are the
  full-height narrow strips (1–4 triangles, top width 2.6e-4..1.1e-3 ×
  height 0.1117) that the §4.4.1 rim-trim measured as un-removable
  "fragment" holders.
- Downstream census (`YANG_441_RIM_CENSUS` on the same run): the kept A-top
  tooth patch boundary walks flank → v787(dr −1.081e-3, plain, INSIDE) →
  16-vertex on-circle chain (sag 3.6e-6) → v820(−1.339e-3) → v813
  (−3.089e-4) → other flank: the design corners poke through the exact
  chain = the A-top rim-weave; NO junction vertex exists on the entering
  flank edges (v785→v787 class). This is J1-0's "exactly one unrelocated
  ≥3-holder corner per seam" (uniform t, 1.339e-3 design gap) measured at
  its mint.
- The 2026-08-11 rim-trim correctly refuses to remove these vertices
  (their wall-strip holders' cycles would degenerate): they are DESIGN
  vertices, not discretization debris. The removal framing is dead; the
  classification must be fixed where it happens — Stage 0.

## 3. Design — increment 1 (gated)

**Before the overlay is built** (in the general-overlay pair path, after
the §2b/§2c clustering and the amendment-18 rim-table fusion, immediately
before `coplanar_overlay(&poly_a, &poly_b)`): for each pair face that has
rim rings and no curved masks (pure disc / annular), **subdivide rim spans
with exact on-circle samples until no partner chain vertex strictly inside
the exact circle lies strictly outside the chord polygon.**

Per rim ring (disc outer via `disc_circle_edge`; annular outer + each hole
via `annular_disc_face` — the hole predicate is IDENTICAL: a vertex inside
the hole circle but outside its inscribed chord polygon is exactly the
misclassified crescent case):

1. **Feature set:** the partner polygon's chain coordinates (outer + holes)
   EXCLUDING the partner's own rim-sample coordinates (the §2c chain/rim
   domain split — rim samples of a partner disc are on-circle geometry, not
   plane features; congruent-rim identity is amendment-18's job).
   Keep only vertices with `radius − dist > 1e-9·(1+radius)` (the Stage-1
   rim band): a partner vertex within band of the circle is on-circle
   content (junction/tangency machinery owns it), and the floor bounds the
   refinement depth.
2. **Violation predicate (exact):** for span `(s,e)` and feature `q` (all
   `ExactPoint2` rationals): `q` strictly inside the circle
   (`|q−c|² < r²`, center/radius per the `RimChordCtx` convention) AND
   strictly on the non-center side of the chord
   (`sign(cross(e−s, q−s)) = −sign(cross(e−s, c−s))`, both nonzero).
   Consecutive-sample spans subtend < π, so crescents are disjoint and the
   violated span is unique per feature.
3. **Subdivision:** insert the span's azimuth-midpoint sample: 2D at
   `c2 + r·(cos θm, sin θm)` (f64 azimuths, wraparound-correct; a span
   subtending ≥ π or a non-finite coordinate is a LOUD skip → pair_err);
   3D by the SAME radial projection the x-event mints use
   (`center + radius·normalize(lift(u,v) − center)` — on-circle to ULP, in
   the cap plane through the exact 3D center). Iterate rounds (split every
   violated span once per round) to a fixpoint; depth cap 32 → LOUD
   `pair_err` (P10 — never silently accept a residual crescent feature).
4. **Propagation (all four consumers, bit-shared):**
   - `poly` ring: insert the 2D sample in boundary order (the overlay's
     region boundary tightens — membership now agrees with exact for every
     partner feature);
   - `rim` map: `ExactPoint2(u,v) → Point3` (overlay-vertex resolution);
   - `rim_overrides[cap_edge]`: push the 3D point (the Stage-1 ring build
     for the lateral and any other rim consumer inserts it);
   - `rim_overrides[opp_edge]`: push the exact 1:1 opposite-rim image via
     the SAME cylinder-axial / torus-poloidal projection
     `collect_ring_crossings` uses (factored helper), keeping the
     azimuth-merge sample counts matched (the C0048 #143/#144 lesson).
   The refined samples are far from uniform slots (midpoints of existing
   spans), so the #143 uniform-slot merge leaves counts intact.

Out of scope (recorded): MIXED Line+Arc faces (`collect_mixed_crossings`
domain — same defect class, arc-chain analog); the n-ary plane-group path
(`nary.rs` — same classifier, same follow-up shape); partner features whose
clearance is inside the rim band (femto/tangency class — stays with the
junction machinery).

## 3b. Increment-1 measurements (2026-08-11, gate-ON F0067)

- Refinement fires on the flush pair: `[rim-refine] face=0 edge=0
  inserted=57 rounds=3 feats_inside=168 ring_len=70` (13-gon → 70).
- **Membership FIXED at the mint**: overlay dump re-census — all 168
  in-circle `corner_a` vertices are now incident ONLY to
  `BOnly`/`Overlap` triangles (was: 126 `AOnly`); Overlap 795→1179,
  BOnly 697→1049 (the crescents reassigned). The entering flank edges
  now mint their junctions (`OnCircle{crossing:true}` splits observed on
  the (v785→v787)-class edges).
- **First follow-on wall (fixed in-increment): junction femto-trios
  resolve divergently.** The sweep mints ULP-twin columns at each new
  junction (measured trio within 4e-15 in 2D: overlay v576/v577/v583);
  the crossing member resolves to the exact circle∩line junction, the
  column twins take the pure x-event RADIAL projection —
  O(sag·tan(flank tilt)) = 3.3e-5 apart, ABOVE the sub-floor 3D
  collapse key — two 3D points for one semantic arrangement vertex →
  degenerate wall-ring content → loud `build-mesh-triangulate f=390`
  (`UNSUPPORTED(coplanar-boolean)`). Fix (same gate): the shared-mint
  collapse ALSO groups mints whose overlay 2D pre-images are within
  `TAU_WORK·(1+scale)` — 2D-femto-identical mints are ONE vertex, and
  the existing election lets the crossing member's junction win.
- **Named residual (follow-on increment, M-B emission-identification
  class): trio-wedge drop leaves an unpaired resolved edge.** With the
  trio collapsed 3D-identical, the M-B degenerate-wedge drop removes
  FOUR stacked wedges at the corner (measured e8drop census, corner_a
  761: 2 `AOnly` + 2 `Overlap` wedges over verts 1204/1211/1218) — the
  pair-identification contract ("neighbors' resolved edges pair
  directly") holds for twin PAIRS but not for a ≥3-member mixed-class
  cluster: the A operand mesh ships `i6-edge-overuse` /
  `s4-halfedge-pairing fwd=1 rev=0` on the junction↔lift edge
  ((1298,2905)/(2905,3486)-class). F0067 gate-ON therefore lands on the
  canonical ERROR verdict (Stage-6 non-2-manifold), one layer past the
  gate-OFF `s6-planar-loop-nonplanar` (face 388, off-plane 3.435e-5)
  wall, with the defect now scoped to the emission-identification
  logic, not to classification.

## 4. Verification plan

- Unit: refinement function on a synthetic ring+partner (corner in a
  crescent → violation cleared, ring stays on-circle and ordered, override
  counts cap=opposite; on-chord-exact feature untouched; depth cap fires
  loudly on a sub-band-engineered fixture... excluded by the band floor —
  cap test via a contrived deep-crescent instead).
- F0067 gate-ON single case: overlay dump shows the 126 corners
  reclassified `Overlap`; flank junctions minted (crossing overrides on the
  entering flank edges); downstream weave gone or verdict advanced.
- Full corpus assay gate-OFF (byte-identical to canonical 258C/0W/50E/0T)
  and gate-ON (censused; any delta anchored before flip).

## 5. Status log

- 2026-08-11: spec written; mechanism measured (§2). Increment 1
  implementation begun (gated).
- 2026-08-11 (same session): increment 1 LANDED gated —
  `refine_rim_membership` (`stage0/rim_chords.rs`; exact violation
  predicate, band floor, orientation-aware azimuth-midpoint
  subdivision, depth cap 32 loud, cap+opposite override propagation via
  the factored `opposite_rim_image`) + the pair-path call site
  (`stage0/mod.rs`, both faces, chain/rim domain split) + the 2D-femto
  shared-mint grouping extension (same gate). Unit tests
  `tests_unit/m8_rim_refine.rs` (4). Measurements in §3b; the M-B
  trio-wedge residual is the NAMED next increment.
