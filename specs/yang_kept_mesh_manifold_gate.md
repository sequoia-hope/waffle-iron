# yang-rs — unconditional kept-mesh watertight/2-manifold gate (§4.4.3)

**Status: ABORTED (P10), 2026-07-03 — see §2b.** The spec's premise ("a
valid boolean's kept mesh is watertight at mesh level") was disproven by
measurement during the cycle's own assay gate; the fix was reverted, not
improvised around. Kept as the P10 record. **Change class:** bug fix
(modeling-affecting; honest-wall relocation). **Crate:** `yang-rs`
(`reconstruct_topology_stage4`).

## 1. Goal

yang-rs crate rule 4: "Output must be 2-manifold or yang-rs returns
`Err(YangError::NonManifoldOutput)`." Today that contract is enforced only
on the Stage-4 path: `check_watertight_2manifold` (§4.4.3) runs inside
`stage4_relocate_and_correct`, which is entered only when `has_conic` is
true. A boolean whose intersection curves are all plane∩plane segments
(`has_conic == false`) skips Stage 4 entirely — a defective kept mesh then
flows through Phase-A/Phase-B emission into kernel-v2, which rejects it
with the unlocalized `InvalidBooleanOutput("an undirected output edge is
not used by exactly two directed edges")`.

After this fix, the kept mesh is gated unconditionally before topology
reconstruction: a non-watertight / non-2-manifold kept set fails LOUD at
the yang boundary with the typed error, in the crate where the defect is
measurable.

## 2. Measured mechanism (2026-07-03, R0046 / R0088 probes)

- R0046 failing subtract: kept mesh 139 tris, `check_watertight_2manifold`
  = FAIL (41 defective undirected edges: `tris=1` open-boundary holes along
  the A-cylinder risers; `tris=3` strips where TWO A/f2 cylinder triangles
  double-cover against one B triangle). All 139 triangles attributed
  (`none_count=0`, 7 patches); the defect is the kept SET, not Stage 6.
- `has_conic == false` for the op (all recognized intersection curves are
  plane∩plane `LineSegment`s) → Stage 4 skipped → the §4.4.3 gate never ran.
- R0088: both failing subtracts identical class (`watertight=false`; one
  kept mesh is a lone 2-triangle open sheet).
- Upstream root (out of scope here, recorded for general-M8 Stage-0 work,
  `specs/cherchi_patch_label_tolerance.md` §6a): the Stage-0 coplanar
  overlay feeds cherchi operand meshes that fail
  `mesh_booleans_inputcheck` (Local Orientation + Intersection); the L2a
  mixed-label flood then fires (8× on R0046, 6008× on R0088) and the
  patch-level in/out verdict keeps a wrong triangle set. Reference parity
  (spec I1) proved the C++ release produces the same output for these
  meshes — the port is faithful; the input contract is what's violated.
- DISPROVEN (P10 record): the roadmap's "stage6-sliver T-subdivision class
  extended" hypothesis for this wall. The unpaired-edge census decomposes
  into one benign T-triangle plus two closed rings that bound kept-mesh
  HOLES — no Stage-6 subdivision can repair a hole in the kept set.

## 2a. AMENDMENT (measured, 2026-07-03 full-assay Phase 4): fold-sliver
## false positives — the gate is UNDIRECTED at the new site

The first implementation reused `check_watertight_2manifold` verbatim
(directed half-edge pairing + per-shell Euler). The full assay measured
exactly two CORRECT→ERROR flips vs baseline: **F0016 and F0024** — the
`yang_stage6_sliver_topology` (N22) class. Their kept meshes legitimately
carry ZERO-AREA fold slivers whose inherited winding is sign-of-zero
(combinatorially arbitrary, N22 §7): a fold duplicates a real triangle's
DIRECTED edge, so the directed multiset is unbalanced even though every
UNDIRECTED edge is used by exactly two triangles (measured on the F0016
§2 structure: chord side + chain side + two shims, all undirected counts
= 2). Excluding degenerate triangles from the directed check does NOT
repair it either — the two sides of the shared collinear chain subdivide
differently, so the sliver-free directed multiset is unbalanced by
construction. The R0046/R0088 defect class, by contrast, is UNDIRECTED
(counts 1 = open holes, 3 = double-cover strips).

First resolution attempt (per the N22 sign-of-zero principle): gate =
undirected 2-cover + per-shell Euler. Second measured amendment, same
day: the Euler test ALSO false-positives — a legitimate N22 sliver
"pillow" (two shims between a chord and its subdivided chain) adds one
undirected edge and two faces on existing vertices, so χ = base + 1
(odd) on VALID output.

## 2b. ABORT (P10 record, 2026-07-03)

The pure undirected 2-cover — the weakest remaining candidate — ALSO
false-positives, and that measurement kills the spec's premise:
F0016's kept mesh (canon wired, current shipped machinery) carries a
**count=1 undirected edge on a REAL positive-area triangle**,
(v9 0.161050238,0.245922473,−0.376470692)–(v37 −0.105658946,
0.168483961,−0.226226293) — the same (9,37) edge named in
`yang_stage6_sliver_topology` §3. A collinear solid-edge chain whose
two sides subdivide DIFFERENTLY is legitimate post-arrangement
structure: at MESH level its coverage only pairs segment-by-segment
after T-junction subdivision, which the pipeline performs at the LOOP
level (N22 §4B, `subdivide_loops_at_shared_vertices`), downstream of
any reconstruct-entry gate. So there is NO mesh-level watertightness
invariant (directed, Euler, or undirected — all three measured) that
separates the R0046/R0088 defect class from valid N22-class output at
the gate's proposed location. A T-junction-aware segment-coverage
census could in principle decide it, but that duplicates the loop-level
machinery that kernel-v2's edge pairing already checks post-subdivision
— new design work, out of this cycle's scope, aborted per P10 rather
than improvised.

**Reverted:** the gate call, `check_kept_mesh_undirected_2cover`, this
cycle's two unit fixtures and the `yang_manifold_gate_campaign` E2E
trackers (they encode the aborted design's contract — wall relocation —
which the real root fix, Stage-0 inputcheck-clean emission, would
falsify by making the cases BUILD).

**Kept (independent test improvements, suite-green without the gate):**
the `arrangement_a_cube_shell` closed mock fixture (replacing the
open bottom-quad mock no real boolean produces) and the `cube_brep`
plane-offset SIGN FIX it unmasked (`offs` was sign-flipped on every
face with a non-zero plane coordinate; latent because only the origin
cube's bottom face, d = 0 either way, was ever attribution-resolved).

**Net outcome:** R0046/R0088 keep their loud kernel-v2
`InvalidBooleanOutput` edge-pairing wall — which this cycle's §2
diagnosis fully localized (defective kept set from non-conformal
Stage-0 overlay input; `m8_shared_boundary_identity` §8c). The named
fix remains Stage-0 inputcheck-clean overlap emission (general-M8).

## 3. Parameters

None. The gate is unconditional and tolerance-free (undirected 2-cover +
per-shell Euler characteristic; §2a).

## 4. Branch table

| # | Configuration | Behavior |
|---|---|---|
| G1 | Kept mesh watertight/2-manifold, `has_conic == false` | Gate passes; output byte-identical to today |
| G2 | Kept mesh watertight/2-manifold, `has_conic == true` | Gate passes; Stage 4 runs as today (its own post-relocation §4.4.3 gate unchanged) |
| G3 | Kept mesh defective, `has_conic == false` (R0046/R0088 class) | Loud `Err(YangError::NonManifoldOutput)` from yang-rs (today: kernel-v2 `InvalidBooleanOutput` edge-pairing) |
| G4 | Kept mesh defective, `has_conic == true` | Loud at the NEW gate (before Stage 4 spends work); today the Stage-4 gate catches it later with the same error |

## 5. Invariants

- I1: no valid output changes — for any kept mesh passing the gate, the
  emitted B-Rep is byte-identical to today (the gate is read-only).
- I2: rule 4 restored — no code path reaches Phase-A/Phase-B emission with
  a non-watertight / non-2-manifold kept mesh.
- I3: error is typed and yang-local — the R0046/R0088 wall string becomes
  `NonManifoldOutput` (surfaced through kernel-v2's boolean delegation as
  a yang error), not kernel-v2 output validation.

## 6. Oracles

- Unit RED (yang-rs): a minimal open-sheet kept mesh (two triangles
  sharing one edge, boundary edges unpaired) driven through
  `reconstruct_topology_stage4` with no conic curves → today emits a
  topology (or fails downstream); after: `Err(NonManifoldOutput)`.
  Plus a 3-tri-edge fixture (double-cover) → same.
- E2E RED (test-harness trackers): R0046 and R0088 boolean-failure sets
  contain kernel-v2 `InvalidBooleanOutput` edge-pairing today; after the
  gate they must instead contain `NonManifoldOutput` and NOT the
  kernel-v2 string (wall relocation, not wall removal).
- Regression: full yang-rs + kernel-v2 + cherchi-rs suites green;
  `./scripts/test.sh rewrite` green.
- Full assay (binding): 0 SUPPORTED_WRONG, no SUPPORTED_CORRECT lost vs
  the pre-cycle baseline (quiet box; assay is load-sensitive).

## 7. Failure modes

- A case that today reaches SUPPORTED_CORRECT despite a non-watertight
  kept mesh would flip CORRECT→ERROR. Per P9 this is only acceptable with
  evidence the old pass was right-for-wrong-reasons; if the assay measures
  such a flip, STOP and re-diagnose that case before shipping (do not
  soften the gate).
- The gate makes multi-op models fail at the FIRST defective op; chained
  cases may report fewer (but earlier, better-localized) errors.

## 8. Research basis

Yang 2025 §4.4.3 (watertight/2-manifold gate after mesh updating); Cherchi
2022 §5 (the boolean's kept set is a closed 2-manifold for conformal
input). The gate function is the existing `check_watertight_2manifold`
(PR-YR10); this cycle only removes its conditional reachability. No new
geometry, no tolerance (7a: not applicable — index-level combinatorics).
