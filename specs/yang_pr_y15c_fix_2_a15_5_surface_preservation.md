# PR-Y15c-fix-2 — A15.5 surface tier preservation in `result_topology_to_waffle_solid`

**Status:** SPEC (FIP §8 Bug Fix Variant — Phase 1).
**Plan reference:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` sub-phase 0a.
**Wrong-anchor count for PR-Y15c-fix arc: 2 of 3.** If this fix's canary fires AND fix recovers A15.5, the escalation counter effectively resets. If canary silent → wrong-anchor #3 → strategic escalation per `feedback_external_coherence.md` (Cherchi 2022 sidecar reference comparison).

---

## 1. Defect statement

A15.5 (`governance/ARCHITECTURAL_INVARIANTS.md:453-472`) verbatim:

> **A15.5 Surface tier preservation.** Boolean operations must preserve surface tier for unmodified faces. When a face passes through a boolean operation without being split by an intersection curve, it retains its original `SurfaceGeom` variant — an analytic face remains analytic, a procedural face retains its construction recipe.
>
> New intersection faces (created where two surfaces intersect) take the highest tier of the two intersecting surfaces. […]
>
> **Implementation**: The boolean pipeline's face classification step must carry forward the original `SurfaceGeom` when assembling unmodified faces into the result solid. Only faces generated from SSI intersection curves receive new surface geometry.

The fix site `crates/kernel/src/boolean/yang_integration.rs::result_topology_to_waffle_solid` violates A15.5:

- **L207:** function signature takes `_surface_map: &BTreeMap<(MeshId, FaceIdx), SurfaceGeom>` — underscore-prefixed = **intent-to-ignore**.
- **L235-264:** the `face_geometry` construction loop unconditionally writes `SurfaceGeom::Planar(Plane { origin, normal })` from `compute_newell_normal(&verts)` for **every** face, even when the surface_map carries the correct `Cylindrical` (or `Conical`/`Spherical`/`Toroidal`) tag for the source face.

The pre-existing spec `specs/yang_face_geometry_propagation.md` Branch Table already prescribes lookup-first, fallback-only:

| Source in surface_map? | Face has ≥3 vertices? | Newell normal length ≥ TAU_NORMALIZE? | Action |
|---|---|---|---|
| **Yes** | N/A | N/A | **Use source geometry (existing behavior)** |
| No | Yes | Yes | Compute Planar via Newell normal + centroid |
| No | Yes | No | Skip (degenerate zero-area face) |
| No | No | N/A | Skip (degenerate face with <3 vertices) |

The implementation overshot: it never even attempts the lookup. Newell-fallback is implemented as the **default** rather than as the prescribed **fallback when source is missing**.

## 2. Root cause hypothesis

Per adversary-6's PR-Y15c-fix Phase 0 v3 validation memo §2 (`docs/audits/pr_y15c_fix_phase0_v3_validation.md`), independently canary-verified:

> The `surface_map` reaching `result_topology_to_waffle_solid` carries the **Cylindrical tag** for the 1 cylinder side face (correctly propagated from the operand's `cylinder_to_face_polys` at `boolean/mod.rs:583-588`). But `result_topology_to_waffle_solid` takes `_surface_map` (underscore-prefixed = unused) and at L235-264 unconditionally writes `SurfaceGeom::Planar` for every face from the Newell normal […]
>
> Per A15.5 […] the cylinder side face in box-minus-enclosed-cyl is exactly such an unmodified face (or a trimmed unmodified face — the trim does not change the underlying surface). It must stay Cylindrical. It does not. **A15.5 violated.**

Probe `[adv6-result-assembly-entry]` fired 10× per F0031–F0040 batch with identical signature: `surface_map.size=9 face_provenance.size=10 surface_map_breakdown={"Cylindrical":1,"Planar":8}`. The cylindrical tag IS in the map; assembly silently discards it.

This is wrong-anchor #2 of 3 in the PR-Y15c-fix arc (PR-Y15c-fix v1 weld site refuted; v3 L4053 unequal-ring earcut refuted). Direction A from adversary-6's §4 is the recommended next step.

## 3. Fix anchor site

**File:** `crates/kernel/src/boolean/yang_integration.rs`
**Lines:** L235-264 (the `face_geometry` construction loop body inside `result_topology_to_waffle_solid`).
**Function signature:** L204-209 (drop the underscore prefix on `_surface_map` once the parameter is consumed).

**Provenance API:** `result.face_provenance` is typed `BTreeMap<FaceIdx, SourceFace>` (`crates/kernel/src/boolean/topology_extract.rs:145`), with `SourceFace { mesh_id: MeshId, face_idx: FaceIdx }` (`topology_extract.rs:28-31`). The value is **scalar**, not `Vec`. The `surface_map` lookup key is `(SourceFace.mesh_id, SourceFace.face_idx)` matching `surface_map`'s `(MeshId, FaceIdx)` key shape (`yang_integration.rs:115-127`). Implementer-j SHALL re-read `topology_extract.rs:145` + `:28-31` BEFORE writing the lookup — if the type changes between spec time and fix time, escalate.

**Lookup-first policy** (per `yang_face_geometry_propagation.md` Branch Table row 1): for each `face_idx` in `result.face_provenance.keys()`, consult `surface_map.get(&(source.mesh_id, source.face_idx))`. If `Some(geom)`, use `geom.clone()` directly. If `None`, fall through to the existing Newell-fallback path (L243-263).

**Out-of-scope for this fix** (per A15.5 second paragraph): "highest tier of two intersecting surfaces" tier-policy for new intersection faces (faces with no provenance). PR-Y15c-fix-2 scope is unmodified-face preservation only; intersection-face tier policy deferred to PR-Y15c-fix-3 if adversary-7's corpus sweep shows it's needed.

## 4. Expected invariant recovery

**I1 — A15.5 holds for unmodified faces.** For every entry `(face_idx, source)` in `result.face_provenance` such that `surface_map.contains_key(&(source.mesh_id, source.face_idx))`, the post-fix `face_geometry[face_idx]` is the **same `SurfaceGeom` variant** as `surface_map[&(source.mesh_id, source.face_idx)]`. Cylindrical faces stay Cylindrical; Conical stay Conical; Spherical stay Spherical; Toroidal stay Toroidal; Planar stay Planar.

**I2 — `face_geometry` totality (existing invariant from `yang_face_geometry_propagation.md` L33-34).** `face_geometry.len()` equals the count of `result.face_provenance` entries minus the count of degenerate faces (skipped by the existing `verts.len() < 3` and `nl < TAU_NORMALIZE` guards on L244-251). The fix MUST NOT decrease the totality.

**I3 — F0031–F0040 cohort cylindrical recovery.** For each of F0031–F0040, the result solid's `face_geometry` contains ≥1 entry with `SurfaceGeom::Cylindrical(_)` variant. Per adversary-6's `surface_map_breakdown={"Cylindrical":1,"Planar":8}` evidence, every result mesh has exactly 1 cylindrical entry available in `surface_map`.

## 5. Test plan (per FIP §4.2)

test-author-a (NEW role per FIP §1 + §8) writes `crates/test-harness/tests/pr_y15c_fix_2_surface_preservation.rs`. Tests MUST fail RED on commit `6642b09` (current HEAD) BEFORE implementer-j touches the kernel.

**Reproduction tests (RED — must currently fail):**

- **F0031 cylindrical-tag preservation** — assert post-Boolean `face_geometry` contains ≥1 `SurfaceGeom::Cylindrical(_)` variant.
- **F0040 cylindrical-tag preservation** — same assertion (operand-order mirror of F0031 per implementer-i's diagnostic §"Verbatim probe output — F0040").
- **F0031–F0040 cohort homogeneity** — iterate all 10 cases, assert each result solid contains ≥1 cylindrical variant.

**Control tests (must remain GREEN):**

- **F0003** — assert `WaffleSolid` constructs successfully and `face_geometry` is non-empty (no false-positive cylindricals from spurious `surface_map` entries on a pure-planar boss).
- **R0020 + R0021** — assert no regression in their existing pass/fail state in `app/tests/cases/assay/results.json` (PR14's original Render-LOD targets; not in the cylindrical cohort but exercise the same code path).

Test conventions per PR-Y15b precedent (`crates/test-harness/tests/pr_y15b_combined_failures_parity.rs`):

- `#[ignore]` + `YANG_BOOLEAN=1` env-gated (set by harness or by manual invocation).
- `WAFFLE_TIMEOUT = Duration::from_secs(60)` thread-wrap pattern (handles R0071 hang; not directly relevant here but standard hygiene).
- Use `run_single_case` from `test_harness::assay::randomized_runner`.
- Numeric / structural assertions on `face_geometry` contents — NOT just "no panic".

**RED-phase demonstration (FIP §4.4):** test-author-a runs `YANG_BOOLEAN=1 cargo test -p test-harness --test pr_y15c_fix_2_surface_preservation -- --ignored --nocapture` on commit `6642b09` and DOCUMENTS in the commit message body that all 5 tests fail. Saves verbatim failure output for adversary-7's mutation-test reference.

**Hard constraint:** test-author-a does NOT touch any kernel source. Tests are evidence; the fix is implementer-j's job (FIP §1 + §4.1 separation).

## 6. Adversarial scope (per FIP §4)

adversary-7 (NEW agent — full role rotation per `feedback_oracle_credibility_via_role_separation.md` — NOT adversary-6) validates with corpus subset = F0031–F0040 + control cases (F0001, F0003, F0005). Specifically:

(a) F0031–F0040 cylindrical-tag transitions per the test plan;
(b) no regression on currently-passing cases (corpus pass count `≥ 11` from current baseline);
(c) no regression on R0020/R0021 (currently-failing cases must not change failure mode);
(d) re-run the Stage F probe family from PR-Y15c Phase 0 v2 (`docs/audits/pr_y15c_fix_phase0_diagnostic.md`) to verify whether tracks A and B dissolve post-fix (if they do, A15.5 was upstream of all 3 tracks; if not, tracks A/B are independent and route to PR-Y15c-fix-1 + PR-Y15c-fix-3);
(e) re-run the Stage E probe from PR-Y15c to verify F0031–F0040 transition `well_formed=false → true`.

Per `feedback_validate_against_corpus.md`: adversary-7 SHALL verify on the FULL corpus subset (not just the spot reproducers F0031/F0040) before issuing ACCEPT. Unit-test green is not GREEN.

Per `feedback_adversary_recommendations_need_canary.md`: if adversary-7 recommends a follow-up investigation site, adversary-7 SHALL canary-verify the proposed site is reachable on the relevant cohort BEFORE writing the recommendation. Inference-without-canary recommendations are forbidden.

## 7. Anchor pre-verification canary requirement

Per `feedback_anchor_before_fix.md` (canary discipline + 3-wrong-anchor escalation rule) AND `feedback_adversary_recommendations_need_canary.md` (treat adversary recommendations as candidate anchors that need their own canary), implementer-j SHALL add the following canary at `yang_integration.rs:210` (just inside the `result_topology_to_waffle_solid` body) BEFORE writing any fix code:

```rust
eprintln!("[fix2-canary] result_topology_to_waffle_solid invoked face_count={}", result.face_provenance.len());
```

**Verification:** run `YANG_BOOLEAN=1 cargo test -p test-harness --test pr_y15c_fix_2_surface_preservation -- --ignored --nocapture` (the RED tests from §5). Verify `[fix2-canary]` fires once per result-mesh case (F0031–F0040 = 10 case-fires; cohort + spot tests sum higher).

**ABORT condition:** if `[fix2-canary]` fires **0 times** for any case in the cohort, the function is not reached on F0031–F0040, adversary-6's analysis is incorrect at the function-reachability level, and P10 (`governance/ENGINEERING_CONSTITUTION.md`) fires. This becomes wrong-anchor #3 of 3 → strategic escalation per `feedback_external_coherence.md` (route to Cherchi 2022 reference comparison; do NOT improvise an alternative fix per the "Don't Chase Regressions" rule).

The canary requirement is non-optional even though adversary-6 already provided strong canary evidence — independent re-verification on the actual fix anchor is what the discipline requires (the anchor for the FIX is L210/L235-264; adversary-6's canary was at L204 on a different probe family).

**Canary removal:** implementer-j MUST remove the canary BEFORE the fix code lands (per the byte-clean diff requirement in DoD §6 + the canary removal precedent in implementer-i's diagnostic §"Production safety verification" #5).

## 8. FIP role table

| Sub-phase | Agent | Reads | Writes |
|---|---|---|---|
| 0a Spec | spec-writer-h | This task brief; `feedback_anchor_before_fix.md`; `feedback_adversary_recommendations_need_canary.md`; A15.5 verbatim; `yang_face_geometry_propagation.md`; adversary-6 v3 §2; FIP §8 Bug Fix Variant | `specs/yang_pr_y15c_fix_2_a15_5_surface_preservation.md` (THIS) |
| 0b Test | test-author-a (NEW role per FIP §1 + §8) | This spec; PR-Y15b test file precedent; `assay/randomized_runner::run_single_case`; `SurfaceGeom` enum; Yang 2025 §4.5; Cherchi 2022 §5 (context) | `crates/test-harness/tests/pr_y15c_fix_2_surface_preservation.rs` (RED tests + RED-phase demonstration in commit msg body) |
| 0c Fix | implementer-j (NEW; NOT spec-writer-h, NOT test-author-a per FIP §1 + §3.2 + §4.1) | This spec; the tests from 0b; DoD §2 (Bug Fix); P9 + P10; `result_topology_to_waffle_solid` L204-290; `build_surface_map` L115-127; `face_provenance` type at `topology_extract.rs:145`; `yang_face_geometry_propagation.md` Branch Table | Canary at L210 + fix at L235-264 (drop `_surface_map` underscore on L207); verify GREEN; remove canary before fix lands |
| 0d Adversary | adversary-7 (NEW agent; NOT adversary-6 per `feedback_oracle_credibility_via_role_separation.md`) | All 0a-0c deliverables; PR-Y15b adversary memo (template); FIP §4; `feedback_oracle_credibility_via_role_separation.md`; `feedback_adversary_recommendations_need_canary.md`; `feedback_validate_against_corpus.md` | `docs/audits/pr_y15c_fix_2_validation.md`; mutation test verified; corpus sweep run |
| 0e Commit | team-lead | All 0a-0d | WASM rebuild + memory updates + git commit + push |
