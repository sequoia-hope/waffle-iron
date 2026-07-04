# KV9-F1 — tangency-grade in/out labeling divergence (cherchi-rs)

**Status:** spec (FIP Phase 1) — layer 1 (labels, N24) SHIPPED §2b;
Increment 0c (Stage-4 tangency ellipse-junction band) MEASURED §2c,
fix E-L2 in flight. **Change class:** bug fix. **Crate:** layer 1
`cherchi-rs` (N24 predicates); Increment 0c `yang-rs` (Stage-4
`vert_ell_junction` gate); named next walls in yang-rs Stage 6 and
kernel-v2 import (§2c.5).

## 1. Goal

The Steinmetz pair (equal-radius perpendicular intersecting-axes
cylinders; kv9 fixtures, corpus F0056/F0058 class) produces correct
boolean output. Today both steinmetz kv9 tests are quarantined `KV9-F1`
and fail at kernel-v2's edge-pairing wall.

## 2. Measured mechanism (2026-07-04, probe chain)

The original quarantine framing ("emit-topology seam at tangency grade")
is SUPERSEDED by measurement — the emit machinery is fine; the LABELS are
wrong:

1. `KV2_OUT_TWIN_PROBE` full census: ALL 36 intersection-ring edges of
   the union output are once-used; the output contains ONLY input A's
   faces (4 faces / 6 loops — A's two lateral halves + two caps).
   (An earlier 10-violation reading was `head` truncation — the M-C
   lesson again.)
2. `YANG_S6_PATCH_PROBE`: the kept set entering Stage-6 is A-only
   (116/116 tris attributed A, zero B).
3. `YANG_KEEP_PROBE` over the labeled arrangement (384 tris): A's labels
   are correct (116 outside-B kept, 76 band inside-B dropped); **all 192
   B triangles carry `inside=[true,false]`** ("inside A") — including
   B's caps at x=±0.6, far outside A. The union keep rule is faithful to
   these labels; the labels are wrong.
4. Patch structure is CORRECT (B: one 120-tri outside patch — caps +
   lateral outside A — plus two 36-tri inside band pieces pinched at the
   tangency points (0, ±r, 0)). Only the 120-patch's in/out verdict is
   wrong (the two 36-patches genuinely are inside A).
5. **Reference parity (binding, the gear-flange discipline):** the C++
   `mesh_booleans` on the SAME dumped operand meshes produces a correct
   union (236 tris, signed volume 0.5047 vs analytic 0.5346 — inside the
   chord-under-fill band; both solids present). The divergence is OURS:
   cherchi-rs's BL2 ray-cast in/out (or BL3 patch propagation) at
   tangency grade.
6. Downstream amplifier (why it reaches kernel-v2 instead of a yang
   gate): with B absent the kept mesh is A's complete CLOSED solid
   tessellation minus nothing it can't close... and no intersection
   curves survive ⇒ `has_conic=false` ⇒ Stage 4 + its §4.4.3
   watertightness gate are SKIPPED (the R0046 §8c bypass, again) ⇒ the
   A-only mesh with open rings walls at kernel-v2 edge pairing.

### 2a. Increment 0b — ray-cast root MEASURED (CHERCHI_INOUT_PROBE +
### CHERCHI_PERTURB_PROBE)

1. Patch 3 (B's 120-tri outside patch) rays from B's cap centre
   (−0.6, 0, 0) along +X — straight down B's own axis, THROUGH A. It
   should cross A twice (entry ≈ x=−0.3, exit ≈ x=+0.3); only the exit
   registered (`hits 1`) → odd parity → "inside A".
2. Both crossings are grazes on A's azimuth-0/π edge chains. The exit
   graze resolves correctly (perturbation winner tri 21 at offset 0). The
   ENTRY graze hits edge (6,17) — A's azimuth-π lateral edge whose
   endpoints carry y = ∓3.6739e-17 (the `r·sin(π)` rounding, opposite
   signs at the two rings): a femto-SKEWED edge whose midpoint
   (−0.3, 0, 0) lies EXACTLY on the ray line (codimension-2).
3. `perturb_ray_and_find_inters_tri` tried all 8 v1 ULP-perturbations;
   the edge-side orient3d **stayed Zero for every offset** (tri 28
   P/Zero/P, tri 31 P/P/Zero) → no strict winner → the C++
   `winner_tri == -1` semantics classified the entry as tangential →
   crossing dropped.
4. The Zero is FALSE: the plane through (v6, v17, v0) has exactly zero
   x-normal-component, so a +y-subnormal bump of v1 gives a true plane
   value ≈ −0.36·5e-324 ≠ 0 — but that magnitude UNDERFLOWS f64.
   `orient3d` wraps `geometry_predicates` (Shewchuk adaptive), whose
   exactness guarantee excludes underflow: the expansion collapses to
   exactly 0.0 and the wrapper certifies `Sign::Zero`. **A filtered/
   adaptive tier may certify NONZERO signs; only exact arithmetic may
   certify Zero** (the M7 cascade philosophy) — the wrapper violates
   this, and the graze resolver turns the unsound Zero into a dropped
   crossing.

Fix (E-L1): on a 0.0 result from the adaptive predicate, re-certify with
an exact rational determinant (dashu; same formula orientation as
Shewchuk's `orient3d` = det[a−d, b−d, c−d], preserving the sign
convention — the "absolute-sign sites are the hazard" M7 mirror lesson).
Truly-coplanar inputs stay Zero (the rational confirms); underflow zeros
get their true sign. Nonzero adaptive results are untouched (they are
already sound), so the entire currently-passing population is
byte-identical (I2). `orient2d` gets the same zero-certification (same
underflow hole, same wrapper). Recorded as a port deviation (exactness
STRENGTHENING) in `docs/yang_deviations.md`.

### 2b. Layer-1 GREEN outcome + next increment (2026-07-04, measured)

The N24 fix (exact-rational zero-certification in `orient3d`/`orient2d`)
lands exactly on the measured root: the steinmetz patch-3 ray now
registers BOTH crossings (`hits 2 → inner []`), every patch verdict is
correct, and B's patches are kept. RED unit trio GREEN; full cherchi-rs
suite (556) + 18/18 sidecar arrangement parity + yang-rs + kernel-v2
suites green.

**Assay (I2 gate):** 86 CORRECT / 0 WRONG vs `baseline-kv9f3` (88):
**+1 genuine gain — F0056 ERROR→CORRECT** (the corpus Steinmetz union,
N24's direct payoff); −3 flips to the documented load-sensitive TIMEOUT
class (R0001 — the known 29s-borderline noise case — R0013, R0056; run
8% slower overall, timeout class 35→38). Discrimination (recorded, not
assumed): all three pass individually on the quiet box
(`n24_single_case_timing` example: 1.3s / 5.2s / 1.5s release) — load
wobble, not N24 cost. Report banked as `baseline-n24.json`.

**Next increment (measured):** with B kept the pipeline reaches Stage 4
(previously bypassed via `has_conic=false`) and stops LOUDLY at
`Stage4RegionInvalid { vertex: 41, OffCurveBeyondChordBand }` — a
tangency-adjacent intersection vertex sits off its assigned Steinmetz
ellipse beyond the chord band (the two ellipses CROSS at the tangency
points; assignment/junction handling at tangency grade is the remaining
layer — the `vert_ell_junction` machinery's tangency case). The kv9
quarantine tags are updated to this wall; measuring that vertex's
curve-assignment state is Increment 0c.

### 2c. Increment 0c — tangency ellipse-junction MEASURED
### (KV9_JUNCTION_PROBE + NONMANIFOLD_SITE_PROBE, 2026-07-04)

1. **Junction census:** the steinmetz union carries FOUR
   `vert_ell_junction` vertices in two twin pairs — v41/v44 at
   (±0.00698, 0.28532, ∓0.04519) and v79/v82 (mirror at y<0). For every
   one, `e_a` and `e_b` reference the SAME unordered cylinder pair
   (bit-identical `axis_point`/`axis_dir`/`second_cyl`, deterministic
   from the InputId-sorted cyl×cyl insert) with combined budget
   B = ε_A + ε_B = 2.9394e-2 (ε = d_ε = 1.4697e-2 each), and the
   closed-form junction `(plane₁ ∩ plane₂) ∩ cylinder` is EXACTLY the
   surface-tangency point (0, ±r, 0), with line-metric grad
   |d̂·r̂| = 1.0.
2. **Why the first-order gate is the wrong metric here:** a junction of
   two sections of the same cyl×cyl pair is ALWAYS the pair's
   surface-tangency point (the decomposition planes z = ±x intersect in
   the line through both tangency points; that line meets the cylinder
   where the radial gradients align). The mesh vertex there is the
   PINCH of the two faceted-surface intersection polylines, and its
   standoff from the exact crossing is SECOND-order-controlled: in
   tangent-plane coordinates at the junction the cylinders are the
   graphs y = r − x²/2r and y = r − z²/2r; facet displacements
   a ∈ [0, ε_A], b ∈ [0, ε_B] perturb the intersection to the hyperbola
   x² − z² = 2r(b−a), whose standoff from the exact crossing is
   √(2r·|b−a|) ≤ √(2r·B), plus ≤ B normal-direction offset. Measured:
   ρ = 4.8026e-2 vs derived band √(2·0.3·2.9394e-2) + 2.9394e-2
   = 1.622e-1 (and vs the inapplicable first-order gate
   2·d_ε/1.0 = 2.9394e-2 — the RED wall). The first-order
   2·d_ε/|d̂·r̂| metric presumes a vertex ON the junction line off the
   cylinder (the KV11 box-edge class) and remains correct there.
3. **Fix (E-L2):** in the `vert_ell_junction` relocation loop, when
   BOTH ellipses carry `second_cyl` naming the same unordered cylinder
   pair, gate ρ against the derived tangency band
   `√(2·r·B) + B` (B = max of the two carried combined budgets —
   identical by construction). Everything else about the arm is
   unchanged: the relocation target stays the EXACT junction point,
   and every non-cyl×cyl junction keeps the first-order gate
   byte-identical. This is a derived metric conversion (the
   single-ellipse arm's 1/sin α analog at tangency grade), NOT
   tolerance widening (A14.3 / P9).
4. **Twin collapse is already handled:** after relocation both twins of
   a pair land on the identical exact junction; the §4.4.1(b)
   sub-feature merge collapses them (post-merge census: v44/v82
   unreferenced, v41/v79 degree 9 — the KV9-F3 machinery, no new code).
5. **Named NEXT walls (measured, NOT this increment):** with Stage 4
   passed, (a) the UNION stops loudly at Stage-6
   `s6-curved-degenerate-loop` — `extract_boundary_cycles` at the now
   4-valent tangency junction interleaves the top-lens and bottom-lens
   boundary cycles into one 76-edge figure-eight whose Newell vector
   cancels (~2.3e-16); the walk needs junction-aware continuation
   pairing. (b) The SUBTRACT clears yang-rs and walls at kernel-v2
   import `NonManifoldVertex(43)` — four elliptical arcs sharing BOTH
   endpoints (two per ellipse) defeat the vertex-pair(+curve) edge
   keying, the same class as the M8 disc∩disc lens BIGON keys. The kv9
   quarantine tags move to these walls.

## 3. Parameters

None new. No tolerances (A14.3): whatever the root, the fix must be an
exact/combinatorial correction of the port's divergence from the
reference, verified by label-level parity.

## 4. Branch table

Deferred to the Increment-0b amendment (the branch space is the ray-cast
decision path; enumerating it before the measurement would be guessing).
Fixed rows already known:

| # | Path | Contract row |
|---|------|--------------|
| L1 | Non-tangent configurations (the entire passing corpus + fuzz populations) | Byte-identical labels (I2) |
| L2 | Steinmetz tangency (kv9 fixtures) | B's outside patch labels `inside=[false,false]` → kept for union/kept-complement rules per op |
| J1 | Ellipse junction, both `second_cyl` naming the same unordered cylinder pair (Steinmetz tangency pinch) | Gate ρ ≤ √(2·r·B) + B; relocate to the exact `(plane₁∩plane₂)∩cyl` junction (nearest root); twins collapse via §4.4.1(b) |
| J2 | Every other ellipse junction (mixed pair, any `second_cyl` = None — the KV11 box-edge class) | First-order gate 2·d_ε/&#124;d̂·r̂&#124; byte-identical |

## 5. Invariants

- **I1 (label parity):** on the steinmetz operands, native per-patch
  in/out verdicts match the C++ reference's effective keep set (oracle:
  output tri multiset / volume parity, plus the label census).
- **I2 (non-regression):** parity_native_vs_sidecar suite + fuzz_boxes +
  fuzz_curved differentials stay green; full assay 0 WRONG, zero CORRECT
  lost vs `baseline-kv9f3`.
- **I3 (E2E):** both KV9-F1 kv9 tests pass (exact-volume oracles) and are
  un-quarantined in the GREEN PR.

## 6. Oracles

- **E2E RED (already red):** the 2 `#[ignore = "KV9-F1 …"]` steinmetz
  tests (union + subtract).
- **Increment-0c RED unit pair** (`yang-rs/tests/kv9f1_tangency_junction.rs`):
  (a) steinmetz SUBTRACT through yang-rs `boolean()` — RED at
  `Stage4RegionInvalid { vertex: 41, OffCurveBeyondChordBand }`, GREEN =
  Ok + watertight + signed volume ≈ πr²h − 16r³/3 (the full yang-level
  numeric oracle; the remaining subtract wall is kernel-v2 import, not
  yang); (b) steinmetz UNION — RED at the same Stage-4 stop, GREEN =
  progression past Stage 4 (the op must NOT fail `Stage4RegionInvalid`;
  its own next wall is the named Stage-6 boundary-walk item, §2c.5a).
- **Label census probes (kept):** `YANG_KEEP_PROBE` (per-surface/inside/
  kept rows + patch census), `YANG_S6_PATCH_PROBE`, `KV2_OUT_TWIN_PROBE`
  (violation + loop dumps, `KV2_OUT_ALL_LOOPS`), `KV9_JUNCTION_PROBE`
  (junction census + post-merge twin state), `NONMANIFOLD_SITE_PROBE`
  (self-localizing NonManifoldOutput gates).
- **Reference parity:** C++ union/subtract on the banked steinmetz
  operand meshes (this cycle banks them as cherchi-rs parity fixtures).
- **Full assay** vs `baseline-kv9f3` (88 CORRECT / 0 WRONG).

## 7. Failure modes / P10 stop criteria

- **Measurement stop:** if Increment 0b shows the C++ reference reaches
  the right answer through machinery the port deliberately deviates from
  (a recorded deviation), the fix re-opens that deviation record — do not
  patch labels downstream of the divergence point.
- **Fix-shape gate:** no tolerance-based verdict overrides; no yang-side
  label "correction" (the fix belongs in cherchi-rs at the divergence
  site, per the crate routing rules).
- **Expected residue:** the irreducible degree-4 quartic stays walled
  (`unequal_perpendicular_stays_walled` keeps passing).

## 8. Research basis

- Cherchi 2022 §5 [#38] ray-cast in/out + patch propagation (reference
  implementation `InteractiveAndRobustMeshBooleans`, the binding oracle).
- Prior records: `cherchi_rs_pr_cr_bl2_cycle_a` (ray-cast port; N20
  t=0-touch mislabel note — tangency-adjacent), KV4-F1 rational-ray
  fallback (needle patches), the gear-flange reference-parity
  re-localization discipline (#28).

### 8a. Analytical vs approximate

Not applicable — labeling logic only.
