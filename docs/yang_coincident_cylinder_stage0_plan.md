# Plan: yang Stage-0 coincident-cylinder re-tessellation (M8-cyl)

**Status:** scoped, increment 1 starting. Task #29. The gear `err.waffle`'s true
final blocker (localized in task #28). This is genuine **Yang 2025 §4.5.5**
extension work to CURVED surfaces — NOT a Cherchi port (the C++ Cherchi
reference itself cannot resolve coincident opposite-normal walls; proven by
`crates/cherchi-rs/tests/task28_plug_in_bore.rs`, which is also non-watertight
under the C++ sidecar).

## The research (Yang 2025 §4.5.5, lines 718-732, Fig. 16; §5.5 Fig. 24)

> "it is necessary to check coplanar planes and perform 2D Boolean operations
> *before* mesh discretizations. Two coplanar planes will be segmented into
> three parts after a Boolean operation in 2D … The overlapping part is replaced
> by a trimmed common planar surface, and **identical meshes are generated for
> both models in this part**. The boundaries of the common surface are regarded
> as intersection curves."

§5.5 (Fig. 24) confirms the method extends to cylinders (coaxial elliptic
cylinders with shared faces). The gear is the curved analog of this exact
mechanism.

## Why PRs 5-6 weren't enough (and how this completes them)

PR-5 (Stage-6 membrane keep/drop for cylinder pairs) and PR-6 (conformal rim
weld) operate *after* cherchi. But §4.5.5 is explicit: the conformal common mesh
must be produced **before** discretization/arrangement. The gear's bore wall and
flange wall are the same cylinder with **non-conformal z sampling** (bore = one
tall quad band, verts only at z=±0.005; flange caps at z=±0.002), so cherchi (and
the C++ ref) receive non-identical meshes on the overlap → 54 unpaired edges.
This milestone adds the **missing upstream step**: re-tessellate the coincident
cylinder walls so the overlap band is bit-identical, after which the existing
machinery finishes the job:
- cherchi PR-4 pocket-dedups the now-bit-identical overlap → one multi-label sheet,
- yang PR-5 membrane resolution drops that sheet for the union (opposite-normal),
- PR-6 rim weld remains a safety net (may become redundant — verify; do not
  remove blindly).

## The mechanism (cylinder analog of §4.5.5)

For a detected coincident-cylinder pair (PR-5 `detect_coincident_cylinder_pairs`
already finds them: same axis line, equal radius, scale-relative band, `opposite`
flag), unroll each lateral to its parametric domain (θ, z):
1. **2D Boolean in (θ, z).** Each lateral is a rectangle in (θ, z). Segment into
   A-only / B-only / overlap. For the gear (and increment 1): full θ ∈ [0, 2π),
   so this reduces to a **1D z-interval Boolean** — overlap = [max(za0,zb0),
   min(za1,zb1)]; A-only / B-only are the protruding z-bands.
2. **Conformal common mesh on the overlap.** Insert conformal RINGS at the
   overlap-band boundary z-values into BOTH cylinders' laterals (the new
   capability — the existing `stage1_tessellate_with_rim_overrides` inserts
   points into a CAP circle; this needs intermediate *lateral* z-rings at the
   same azimuths/radius on both), so the overlap band is sampled identically on
   both models.
3. **Boundary rings = intersection curves.** The overlap-band boundary rings
   (e.g. the gear's z=±0.002 circles) are where the inner solid's caps stitch to
   the outer wall — they become the shared intersection curves (§4.5.5 "the
   boundaries of the common surface are regarded as intersection curves").

The result is fed to cherchi exactly as the planar Stage-0 overlay output is.

## Increments (each: code + yang-rs tests + the parity-oracle gate + assay delta; commit/push per increment)

### Increment 1 — opposite-normal, full-θ, z-band overlap  ← the gear, do first
The gear's exact case (bore wall ∩ flange wall). The 2D Boolean is a 1D z-interval
(full θ). Insert conformal z-rings at the overlap boundaries on both laterals so
the overlap band is bit-identical; emit the boundary rings as intersection curves.
- **Gate:** `crates/cherchi-rs/tests/task28_plug_in_bore.rs` becomes **watertight**
  after the Stage-0 pass (currently proven non-watertight in both native AND the
  C++ sidecar — so this is the un-portable-from-Cherchi step), THEN `err.waffle`
  builds clean: un-`#[ignore]` `crates/test-harness/tests/gear_flange_union.rs`
  (combined bbox z[-0.005,0.005], `no_self_intersection` + watertight + Euler χ=2 +
  positive volume). Assay 0 new SUPPORTED_WRONG, no CORRECT→WRONG regression.
- **Where:** new coincident-cylinder Stage-0 path in `crates/yang-rs/src/stage0.rs`
  (parallel to the planar `stage0_preprocess`); new intermediate-lateral-z-ring
  insertion in `crates/yang-rs/src/lib.rs` (`stage1_tessellate_*`); wire into
  `boolean()` so a detected coincident-cylinder pair routes through it (returning a
  `Stage0` with conformal meshes, even when there are no planar pairs).

### Increment 2 — same-normal coincident cylinders (flush/pocket)
The cylinder analog of the planar flush case (`opposite=false`): both interiors
on the same side. Keep/drop rule already in PR-5's membrane resolution; this adds
the conformal re-tessellation for the same-normal config + its fixture.

### Increment 3 — partial-θ overlap (general (θ,z) 2D Boolean)
Cylinders sharing only an angular sector (not full θ). Requires the real 2D (θ,z)
overlay (the cylinder analog of `coplanar_overlay.rs`), not just the 1D z-interval.

### Increment 4 — other coincident curved surfaces (sphere, cone)
Deferred — same parametric-domain principle, different unroll. Out of scope until
a user case needs it.

## Discipline (P9/P10 — reinforced by this gear's history)
- **No tolerance snaps.** An F0057-class rounding-weld was reverted in PR-5; a
  broad SSI→LineSegment fallback was reverted earlier. The conformal re-tessellation
  must produce bit-identical sampling by CONSTRUCTION (same axis/radius/azimuth/z
  generators), not by post-hoc fusing within a tolerance.
- **A 0-error build that self-intersects or drops a body is the cardinal failure**
  (`no_self_intersection` + the full-height bbox gate exist to catch it).
- **Reference parity where the reference succeeds.** The mesh boolean of the
  *conformal* output must still match the C++ sidecar on cases the sidecar
  resolves; `task28_plug_in_bore.rs` is the oracle for the case the sidecar cannot
  (it must go watertight post-Stage-0).
- Never claim "last bug" — the gear has been a multi-defect chain; expect more and
  report honestly.

## Out of scope / fold-in
- Task #25 (make the silent auto-union→standalone fallback LOUD) — fold in so a
  future coincident-curved failure surfaces as an error, not a dropped body.

## Critical files
- `crates/yang-rs/src/stage0.rs` (coincident-cylinder detection [PR-5] + new re-tess path)
- `crates/yang-rs/src/lib.rs` (`stage1_tessellate_*` intermediate-lateral-z-rings; `boolean()` wiring; PR-5 membrane resolution; PR-6 rim weld)
- `crates/cherchi-rs/tests/task28_plug_in_bore.rs` (the parity-oracle gate)
- `crates/test-harness/tests/gear_flange_union.rs` (the E2E gate, un-`#[ignore]` on green)
