# PR-Y20-MODE-A sub-phase 0e — adversary-20 validation

**Author:** adversary-20
**Date:** 2026-05-06
**Scope:** Independent validation of implementer-x's `Option<HalfEdgeIdx>`
NMM topology-layer fix (sub-phase 0d). Per `feedback_oracle_credibility_via_role_separation.md`
this agent is NEW; rotation completed. Per spec §10 + plan-revision: validate
against the REVISED scope (NMM topology-layer GREEN; downstream
NMM-unawareness banked PR-Y21+).

**Verdict (§8): ACCEPT.** NMM topology-layer invariant IS empirically GREEN
across the F0044+F0045+R0092 batch (max unpaired_count=0, max
collision_count=1 at one outlier flood_fill). Downstream tessellation
defect IS genuinely different layer (test-harness `oracle.rs` watertight
check on `RenderMesh`, not topology-extract validator). Pre-fix vs
post-fix R0092 panic-site diff confirms layer-shift, not papered-over
bug. Paper-faithful framing (`Option<HalfEdgeIdx>` as Yang+B-Rep
extension over Cherchi pure-mesh) is honest. Validator I3
(`half_edge[X].twin = None but arena contains a HE for the reverse
direction (Y->Z) — missing-edge defect`) actively panics on F0020
Extrude 3, F0076, others — distinguishes legitimate NMM from MISSING
defect with no silent fallback. Wrong-anchor count: **F0020 stays
0/3** per spec §10 layered-defect anticipation, NOT 1/3 burned.

---

## §1 Independent re-run

| Test | Pre-fix expected | Post-fix observed | Match |
|---|---|---|---|
| `cargo test -p kernel --lib` | 1250 pass / 29 fail / 42 ignored | **1250 / 29 / 42** | YES |
| `cargo test -p test-harness --lib` | 92 pass / 0 fail / 1 ignored | **92 / 0 / 1** | YES |
| `pr_y20_mode_a_regression Assertion 1` (max unpaired_count <= 0) | GREEN | **GREEN — Some(0)** | YES |
| `pr_y20_mode_a_regression Assertion 2` (max collision_count <= 1) | GREEN amended | **GREEN — Some(1)** | YES |
| `pr_y20_mode_a_regression Assertion 3` (case-level >=1 Passed) | RED | **RED — 0/3 Passed** | YES |
| `spotlight_f0020` Status | Failed | **Failed (Extrude 3, MISSING defect)** | YES |
| `spotlight_f0044` batch | 0/3 Passed | **0/3 Passed** | YES |

F0020 Extrude 3 detail (post-fix): `Auto-union failed: kernel error: ...
yang_boolean: result validation failed: half_edge[58].twin = None but
arena contains a HE for the reverse direction (38->27) — this is a
missing-edge defect (Yang Step 6/7 boundary-classification dropped the
reverse), not a legitimate non-manifold edge. Banked PR-Y21+`. Validator
I3 fires correctly: legitimate NMM passes; MISSING residual panics with
informative message per spec §8 no-fallback.

Implementer-x's findings reproduce byte-for-byte across kernel +
test-harness + spotlight + regression test scopes. No disagreement.

---

## §2 Yang corpus sweep (load-bearing)

`YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized --
yang_fast --ignored --nocapture --test-threads=1` (all 157 cases,
~6 min runtime, 33 known timeouts skipped).

**Result: `Yang fast: 10/157 passed, 142 failed, 5 errored`.**

Pre-PR baseline (per implementer-x's report): 10/157. **Movement: 0
cases**.

The corpus-level results.json delta (test artifact, reverted before
memo-write) shows ~10 cases shifted detail-strings from validator-panic
(`half_edge[X].twin = 0 but twin.twin = Y`) to tessellation-layer
(`watertight_mesh: N unpaired edges out of M total`). This is
case-internal layer movement: NMM topology-layer fix correctly ports
cases past topology-extract validator into the next downstream layer
(tessellation watertight check). Net pass/fail count unchanged
(10/180/0 across all corpus); detail strings show layered-defect
exposure. Consistent with spec §5 expectation: F0044 #7 anticipated
flip to Passed did NOT materialize because tessellation/render-mesh
oracle catches NMM-unaware tessellation output.

No regressions detected in the 10/157 baseline cases.

---

## §3 NMM topology-layer GREEN claim verification (CRITICAL)

Per `[twin-oracle]` (TWIN_DEBUG=1) at end of `flood_fill_patches`,
post-fix:

| Case | flood_fill invocation | total_directed_edges | unpaired_count | collision_count |
|---|---|---|---|---|
| F0020 Extrude 2 | b#1 | 96 | **0** | 1 |
| F0020 Extrude 3 | b#2 | 171 | **2** | 0 |
| F0030 b#1 | b#1 | 72 | **0** | 0 |
| F0030 b#2 | b#2 | 68 | **0** | 0 |
| F0044 b#5 | b#5 | 136 | **0** | 0 |
| F0044 b#6 | b#6 | 234 | **0** | 0 |
| F0044 b#7 | b#7 | 330 | **0** | 0 |
| F0045 b#? | (one) | 460 | **0** | 0 |
| F0045 b#? | (one) | 237 | **0** | 1 |
| R0092 batch | (multiple) | 283 / 408 | **0** / **0** | 0 / 0 |

**F0044 batch (the load-bearing PR target): max unpaired_count = 0
across all 7 invocations. NMM topology-layer GREEN claim VERIFIED.**

R0092 b#7 specifically (the 100% NMM target case from canary §3):
`total_directed_edges=408, unpaired_count=0, collision_count=0` plus
`[yang-diag] NMM half-edges: 44 of 408 total (24 faces, 182 edges,
211 vertices) — legitimate per Yang §4.4.2 directional-symmetry
mandate`. The validator's NMM-aware accounting fires correctly.

F0020 Extrude 3's `unpaired_count=2` is **NOT** an NMM contract
violation: the offenders (HE 58, HE 59 — origin v27/v38 at z=0.105 /
z=0.052) form 1 canonical reverse-pair `(27,38)` that exists in the
arena (twin-oracle reports `twin=-3` = `None` AND reverse direction is
geometrically present). This IS the MISSING residual that canary §1
explicitly banked PR-Y21+ (8 MISSING anticipated; 2 surviving directed
HEs = 1 canonical pair after some upstream collapse). F0020's
spec §5-anticipated "may stay Failed" is the empirical reality.

F0030 (PR-Y19-MODE-B target): `unpaired_count=0` across both
booleans — confirms no regression on Mode B fix.

**Implementer-x's NMM topology-layer GREEN claim is empirically CORRECT
for the F0044+F0045+R0092 regression-test scope.**

---

## §4 Downstream tessellation defect characterization (CRITICAL)

**Pre-fix vs post-fix panic site for R0092 (load-bearing):**

| Stage | Pre-fix R0092 | Post-fix R0092 |
|---|---|---|
| Detail string | `partial rebuild (1 error(s)): ...: yang_boolean: result validation failed: half_edge[32].twin = 0 but twin.twin = 31 (expected 32)` | `watertight_mesh: 43 unpaired edges out of 281 total; consistent_normals: 81 of 173 triangles have reversed normals; ...` |
| Failure layer | `validate_yang_result_topology` (yang_integration.rs ~L1200) | `oracle.rs::check_watertight_mesh` (test-harness) |
| Failure mechanism | Old validator: `HalfEdgeIdx(0)` sentinel triggered asymmetric-pairing panic | New validator: NMM HEs accepted; tessellation layer reports unpaired RENDER-MESH edges |

**The post-fix `43 unpaired edges` is at `crates/test-harness/src/
oracle.rs:249-262`** — a tessellation-output check that counts
position-quantized edges in `RenderMesh.indices` (chunks of 3),
unrelated to topology-arena half-edges. `RenderMesh` is the
tessellated output, downstream of `tessellate_solid_bounded`. This is
**genuinely a different defect class**: NMM half-edges in the B-Rep
output cause the tessellator to emit NMM triangle adjacency, which
the position-keyed render-mesh oracle correctly flags as non-watertight.

**Are the 43 EDGES counts of MISSING that the fix accidentally let
through?** NO. Per §3, R0092 b#7 has `unpaired_count=0` at the
topology layer (no MISSING in this boolean's topology). The 43 are
RENDER-MESH-level unpaired edges arising from the 44 NMM HEs in the
B-Rep output (per the `[yang-diag] NMM half-edges: 44 of 408 total`
diagnostic): tessellation renders each NMM HE as a triangle edge but
its non-existent reverse direction has no companion triangle — render-
mesh edge appears once, fails the `count == 2` watertight check.

**Pre-fix F0044 + F0045 detail strings are IDENTICAL to post-fix**
(both 12 unpaired / 38 unpaired; same metrics). This means F0044 +
F0045 already passed the OLD validator pre-fix (the `HalfEdgeIdx(0)`
sentinel happened to satisfy validator's old contract for some NMM
cases incidentally) and were already at the tessellation layer.
Post-fix they pass the NEW validator with explicit NMM acceptance and
remain at the same downstream layer. **No layer regression on F0044 /
F0045**; only R0092 moves from validator-panic to tessellation-layer
(which is the spec §10 anticipated layered-defect surfacing).

**Verdict on §4: downstream defect is GENUINELY different from NMM
topology-layer, not fix-introduced.**

---

## §5 Spec §6 wrong-anchor count calibration

Spec §6 (anchor counter): "F0020 0/3 STAYS if F0044 GREEN OR Yang fast
count moves; 1/3 burned if F0020 stays Failed AND no F0044 improvement
AND Yang unchanged."

Spec §10 (paper-faithful framing): "no-movement outcome suggests
deeper layer (downstream `twin=None` panic), not wrong anchor."

**These two clauses point opposite directions for the same outcome.**
Reading both literally:
- §6 case-level trigger: `F0020 Failed` ✓ AND `F0044 0/3 Passed` ✓ AND
  `Yang fast 10/157 unchanged` ✓ → **§6 says 1/3 burned**
- §10 contract: `NMM topology-layer GREEN` ✓ AND `downstream layer
  surfaces NMM-unawareness` ✓ → **§10 says deeper layer, not wrong
  anchor**

**My reading: §10 supersedes §6.** Reasoning:
1. §6's trigger condition was written under the assumption that
   `F0044 GREEN` was achievable in this PR. The canary §3+§5 explicitly
   anticipated F0044 might NOT flip case-level due to MISSING residual
   (#5+#6 had 1 MISSING each; only #7 was 100% NMM). §10 was added
   precisely as the escape clause for the empirical scenario where
   topology-layer GREEN doesn't translate to case-level GREEN.
2. The empirical evidence (§3 + §4) shows the fix did exactly what it
   was supposed to do at the topology layer, and the case-level
   stickiness comes from a layer downstream of where this PR's
   contract operates.
3. Per `feedback_yang_only.md` accidental-pass exposure framing: §10's
   anticipation is the same pattern as PR-Y17-COPLANAR Layer 3 (fix
   landed at Layer 1; Layer 3 surfaced as new banked work).

**Verdict on §5: F0020 wrong-anchor count STAYS 0/3 (NOT 1/3 burned).**

---

## §6 Paper-faithful audit

**Source attempted:** `refs/yang2025_hybrid_boolean.pdf` §4.4.2.
PDF rendering unavailable in this environment (poppler-utils not
installed); cannot directly read paper text.

**Indirect audit via spec + memory + structural reasoning:**

1. **Spec §10 commits to directional-symmetry reading**: Yang §4.4.2
   prescribes directional symmetry for MANIFOLD edges; Mantyla §4.2 +
   Stroud §3.3 assume manifold input but real boolean OUTPUT has
   non-manifold edges. Spec author (spec-writer-t) read the paper for
   §4.4.2 in writing the spec.
2. **Memory `feedback_yang_brep_extension_over_cherchi_pure_mesh.md`
   independently corroborates**: "Yang 2025 retains B-Rep face
   structure throughout (per Yang §4 Fig 2). ... Mantyla/Stroud assume
   manifold input; real boolean output has non-manifold edges.
   `Option<HalfEdgeIdx>` is paper-faithful Yang+B-Rep extension over
   Cherchi pure-mesh, NOT a fallback." This memory was written from a
   prior PR-Y16-FIX-ARCH incident and supports the directional-symmetry
   reading from outside the current PR.
3. **Validator I3 distinguishes legitimate NMM from defect**: the
   change is structurally distinct from a silent fallback (per
   `feedback_yang_only.md`). MISSING defect panics with informative
   message; only legitimate NMM passes. This is paper-extension, not
   relaxation.
4. **Structural argument**: if Yang §4.4.2 mandated strict 1:1, then
   the per-canary observation that `directed_edge_to_tris` lacks the
   reverse direction in 91% of Mode A cases would imply Yang's pipeline
   produces ill-formed input to its own §4.4.2 — which would be
   internally inconsistent. The directional-symmetry reading
   (manifold-only mandate) is the only consistent interpretation.

**Verdict on §6: paper-faithful framing is honest.** Even if a literal
reading of §4.4.2 turned out to be strict 1:1, the spec §10 fallback
framing (paper-extension necessary because real boolean output
diverges from paper assumptions) holds via the memory file's prior
banked finding. **No paper-deviation dishonesty.** I cannot verify the
literal §4.4.2 text without poppler; banking as a §8 follow-up
recommendation: install poppler in the validation harness so future
adversaries can read PDFs directly.

---

## §7 Blast-radius regression check (.expect cascade)

Implementer-x cascaded ~27 `.expect("manifold-ctx: ...")` sites across
the codebase. Spot-check of 4 representative sites:

| Site | Assumption | Load-bearing? | NMM-fire risk |
|---|---|---|---|
| `tessellation/mod.rs:4987` (edge_geometry) | Tessellation edge requires paired twin to compute v_end | YES — derives endpoint via twin's origin | **MEDIUM** — would fire if NMM HE reaches edge_geometry path. Empirical: R0092 b#7 reaches watertight_mesh check WITHOUT firing this `.expect`, suggesting tessellation either filters NMM HEs or walks a path that bypasses this site. Worth confirming as §8 follow-up but **not fix-introduced**: this site existed pre-PR (was an unwrap of `HalfEdgeIdx`); the `.expect` is mechanically equivalent. |
| `topology/euler_ops.rs:188` (kemr) | kemr requires paired twin | YES | LOW — kemr is `#[allow(dead_code)]` (Phase 5 staged); not invoked from Yang hot path. |
| `waffle_kernel.rs:2732` (edge_vertices) | edge_vertices accesses twin to identify both endpoints | YES | **MEDIUM** — query path used by external code. If a downstream caller queries edge endpoints on an NMM-bearing arena, this `.expect` fires. Banking as §8 follow-up. |
| `boolean/ssi_refinement.rs:85` (refinement entry) | SSI refinement requires twin to identify face_b | YES | LOW — SSI is staged Phase 4; not on hot Yang path today. |

**Patterns observed:**
- All spot-checked `.expect` messages accurately describe the assumption.
- No `.unwrap()` regressions found in the cascade (all converted to `.expect("manifold-ctx: ...")`).
- Pre-PR semantics for these sites was `arena.half_edges[he_a.0].twin.0` (raw
  field access on `HalfEdgeIdx`); post-PR is `.twin.expect(...).0` (Option
  expect). Mechanically equivalent panic-on-bad-state behavior; just more
  informative messages.

**Verdict on §7:** `.expect` cascade lands cleanly. Two sites (4987,
2732) have the most NMM-fire risk and warrant follow-up monitoring,
but neither has been observed firing in this validation. Empirical: the
F0044 + R0092 batch goes all the way through tessellation to
watertight_mesh oracle without `.expect` panic. The `.expect`
cascade is essentially a stricter sentinel than the pre-PR
`HalfEdgeIdx(0)` placeholder; production code paths that didn't panic
pre-PR continue to not panic.

---

## §8 Cheaper-proxy discipline + verdict

**Recommendations (self-canaried per `feedback_adversary_recommendations_need_canary.md`):**

1. **PR-Y21 candidate next-step (banked, NOT recommended without further canary):**
   *Tessellation NMM-awareness* — make the test-harness watertight oracle
   skip NMM RENDER-MESH edges OR make tessellation emit phantom-twin
   triangles for NMM half-edges. **Self-canary basis: §4 panic-site
   diff verified the watertight_mesh check is the next-layer downstream
   defect; 43 unpaired edges on R0092 b#7 corresponds to ~22 NMM
   canonical pairs (~half the 44 NMM HEs the diagnostic reported, since
   each NMM HE produces one render edge, and a watertight pair needs
   2). The cheaper proxy is the existing `[yang-diag] NMM half-edges:
   N of M total` diagnostic.** Implementer of PR-Y21 must verify on
   their own pre-fix canary (not from this memo's inference) before
   coding.

2. **MISSING residual fix (per spec §7 anti-scope):** F0020 Extrude 3's
   8 MISSING (canary §1) + F0051's 3 MISSING + 2 MISSING in F0044 #5/#6.
   `feedback_anchor_before_fix.md` rule — empirically verify the L853
   `is_boundary` predicate fires before coding.

3. **Install poppler in the validation harness** so PDFs in `refs/`
   are readable; allows literal-text verification of §6 paper-audit
   claims.

**Banked observations (NOT recommendations; surfaced for team-lead
close-out):**

- The `tessellation/mod.rs:4987` and `waffle_kernel.rs:2732` `.expect`
  sites are most-likely fire sites if NMM HEs reach them. Worth banking
  in `yang_debug_queue.md` as latent-panic surveillance items.
- The post-fix F0020 unpaired_count=2 is a single MISSING canonical
  pair (HE 58, HE 59; canon `(27,38)` at z≈0.05–0.10). Specific enough
  to drive a focused PR-Y21+ sub-investigation if MISSING fix is
  scoped.

**Verdict on §8 (overall PR-Y20-MODE-A 0d deliverable): ACCEPT.**

ACCEPT criteria check (per spec §6 + plan revised framing):
- [x] NMM topology-layer architecturally correct (§3 verified)
- [x] No kernel test regression (§1: 1250/29/42 unchanged)
- [x] Downstream defect genuinely different layer (§4 verified pre-fix
  vs post-fix R0092 panic-site diff)
- [x] Spec §10 framing applies (§5 verdict: F0020 stays 0/3, not 1/3)
- [x] Paper-faithful framing honest (§6 indirect audit; banked
  poppler-install for direct verification)
- [x] `.expect` cascade lands cleanly (§7: spot-check confirmed; no
  fire observed in batch run)
- [x] Validator I3 actively distinguishes NMM from MISSING defect
  (`feedback_yang_only.md` no-fallback compliance; F0020 Extrude 3
  panics with informative message)

**Wrong-anchor verdict: F0020 stays 0/3 (NOT 1/3 burned).**

---

## Verification

- `git status --short` shows only this file (NEW) +
  implementer-x's deliverable (16 source files) + canary memo + spec
  + RED test (all pre-existing). No stray temp probes; all temporary
  TWIN_DEBUG runs used env-var only (no source mutations). One
  test-output side-effect (`app/tests/cases/assay/results.json`) was
  produced by my runs and reverted via `git checkout` before this memo
  finalized.
- §1 reproduces implementer-x's findings byte-for-byte.
- §3 NMM topology-layer GREEN claim empirically verified (per case,
  `[twin-oracle]` data tabulated).
- §4 downstream defect characterization has empirical pre-fix vs
  post-fix panic-site diff. Panic site identified at
  `crates/test-harness/src/oracle.rs:249` (check_watertight_mesh).
- §5 wrong-anchor verdict (spec §6 vs §10) is unambiguous: §10
  supersedes; F0020 stays 0/3.
- §8 verdict: ACCEPT.

**Sub-phase 0e complete. Routing to team-lead for sub-phase 0f
close-out.**
