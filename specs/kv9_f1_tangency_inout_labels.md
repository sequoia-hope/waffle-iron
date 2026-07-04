# KV9-F1 — tangency-grade in/out labeling divergence (cherchi-rs)

**Status:** spec (FIP Phase 1) — §2 Measured mechanism: causal chain
COMPLETE through the label layer; the ray-cast-level root is the next
measurement (Increment 0b). **Change class:** bug fix. **Crate:**
`cherchi-rs` (labeling — BL2 ray-cast in/out or BL3 propagation);
`yang-rs`/`kernel-v2` carry only diagnostic probes.

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
- **Label census probes (kept):** `YANG_KEEP_PROBE` (per-surface/inside/
  kept rows + patch census), `YANG_S6_PATCH_PROBE`, `KV2_OUT_TWIN_PROBE`
  (violation + loop dumps, `KV2_OUT_ALL_LOOPS`).
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
