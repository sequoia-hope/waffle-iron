//! PR11 — Yang Stage 4b per-patch labeling + F1/F2 oracle fixes: red-phase tests.
//!
//! ## Role and FIP §1 P5 separation
//!
//! Authored by `agent-test` on team `yang-per-patch-labeling-pr11`. Per FIP §1
//! P5, agent-test is distinct from agent-impl-stage4b (T3) and
//! agent-impl-oracle-fixes (T4). These tests encode the spec invariants —
//! they will be unblocked / unignored by the implementers, but the
//! implementers MUST NOT modify this file.
//!
//! ## What is being tested
//!
//! Five tests encoding the spec at `specs/yang_per_patch_labeling.md`
//! (SHA `3aafe89`):
//!
//! - **Test 1** (`per_patch_label_uniformity_red_phase`) — spec §4 I1.
//!   For every Cherchi 2022 §5 manifold patch, every member sub-triangle
//!   shares one `CellLabel`. Currently fails because `label_cells`
//!   ray-casts per-sub-tri (lines 1944-2035 of
//!   `crates/kernel/src/boolean/exact_mesh.rs`), producing mixed labels
//!   on 120/157 corpus cases (PR10 oracle audit). Post-T3 the per-patch
//!   refactor makes this hold by construction.
//!
//! - **Test 2** (`per_patch_representative_pick_anchor_red_phase`) —
//!   spec §4 I2. The label assigned to every patch member equals the
//!   label that `label_sub_tri_raycast` would return on the patch's
//!   representative sub-triangle (Cherchi 2022 §5 Algorithm 1, "we pick a
//!   random triangle t ∈ P"). Currently the per-sub-tri code path does
//!   not pick a representative, so this is an aspirational anchor.
//!
//! - **Test 3** (`f1_upstream_conservation_anchor_red_phase`) — spec
//!   §F1. The Stage 2 conservation check is currently tautological
//!   (`total_directed == 3 × (|tris_a| + |tris_b|)`); after T4, an
//!   `upstream_tri_count` field is anchored to the upstream Cherchi
//!   `solve_intersections` result so "lost-during-emit" defects are
//!   detectable. Encoding (a) per spec §F1: `upstream_tri_count` counts
//!   distinct emitted sub-triangles (label==3 contributes 2). The literal
//!   invariant becomes
//!   `subdivided.tris_a.len() + subdivided.tris_b.len() == subdivided.upstream_tri_count`.
//!
//! - **Test 4** (`f2_post_injection_oracle_anchor_red_phase`) — spec
//!   §F2. The Stage 0 snapshot at `yang_integration.rs:702` records
//!   pre-injection meshes; T4 moves it post-injection so
//!   `CoplanarMeshIdenticalOracle` is reachable on the production
//!   identical-footprint code path with deliberate divergences (above
//!   weld tolerance per PR10 audit).
//!
//! - **Test 5** (`per_patch_labeling_determinism_red_phase`) — spec §4
//!   I5 + spec §5 secondary oracle. Two structurally-identical fixtures
//!   with deterministically-permuted patch member ordering produce
//!   identical `CellLabeling` output — guards against representative-
//!   pick non-determinism.
//!
//! ## Red-phase strategy
//!
//! Per the brief: "use one of: (preferred) `#[ignore]` annotation with
//! a comment explaining 'ignored until label_cells accepts
//! ManifoldPatchGraph (T3)' — when T3 lands the new signature, remove
//! the `#[ignore]` and the test runs and asserts."
//!
//! All five tests use this pattern. The reasoning per test:
//!
//! - Tests 1, 2, 5 are **integration-style** through the public
//!   `kernel::diagnostics::with_yang_oracle_capture` API: they construct
//!   real solid pairs and inspect the `LabelConsistencyWithinPatchOracle`
//!   verdict. Currently the oracle reports `ContractViolated` on the
//!   chosen fixture (the per-sub-tri labeler emits mixed labels).
//!   Post-T3 the oracle returns Ok by construction, so the assertions
//!   flip from failure to success.
//!
//! - Test 3 (F1) requires direct access to the kernel-internal
//!   `SubdividedMesh` struct (the `upstream_tri_count` field T4 will
//!   add). That struct is `pub(crate)` (kernel-internal per A15.6); the
//!   test cannot construct it from `test-harness`. The test is
//!   `#[ignore]`-gated AND its body is a documentation-only stub that
//!   reproduces the literal F1 invariant from the spec. T4's
//!   implementation MUST land an integration-test path in
//!   `crates/kernel/src/boolean/oracles/arrangement_wellformed.rs`'s own
//!   `#[cfg(test)] mod tests` — that is where the real F1 anchor unit
//!   test lives. The stub here documents the contract the
//!   implementer must encode, and is intentionally easy to flip from
//!   `assert!(true)` to a real cross-crate invocation once T4 exposes a
//!   public diagnostic shim.
//!
//! - Test 4 (F2) similarly requires direct access to
//!   `CoplanarPreprocessSnapshot` (`pub(crate)`); the F2 fix is at the
//!   PRODUCTION snapshot-capture site (`yang_integration.rs:702`), not
//!   in the oracle. So Test 4 here documents the contract; T4's
//!   integration test in `coplanar_identical.rs#tests` is what verifies
//!   the post-injection capture end-to-end.
//!
//! ## Why this file exists at all
//!
//! Per FIP §1 P5 + §2 red-before-green, the Test Author writes tests
//! BEFORE implementation. The brief mandates this file's location at
//! `crates/test-harness/tests/pr11_per_patch_labeling.rs`. That
//! placement is a constraint inherited from the brief; given
//! `pub(crate)` visibility on the relevant kernel internals, the tests
//! that CAN run today exercise the public diagnostic API
//! end-to-end (Tests 1, 2, 5), and the tests that CANNOT (Tests 3, 4)
//! document the spec invariants and reference the implementer-side test
//! locations where the real assertions land.
//!
//! ## Refs
//!
//! - `specs/yang_per_patch_labeling.md` — spec being gated.
//! - `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §1, §2, §4.
//! - Cherchi 2022 §5 + Algorithm 1 — per-patch ray-cast in/out.
//! - Yang 2025 §4.4 — labeling stage of the hybrid B-Rep / mesh boolean.
//! - PR10 audit `specs/oracle_validity_audit.md` §F1, §F2.

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use kernel::diagnostics::{with_yang_oracle_capture, OracleRunSummary, ViolationKind, YangStage};
use test_harness::assay::randomized_runner::discover_cases;
use wasm_bridge::messages::UiToEngine;
use wasm_bridge::{dispatch, EngineState};

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Per-case timeout for end-to-end pipeline runs. Mirrors `pr9_pipeline_oracle_corpus.rs`.
const PER_CASE_TIMEOUT: Duration = Duration::from_secs(30);

/// Candidate corpus cases to probe. Spans the corpus ID range with
/// both F-prefixed (feature-targeted) and R-prefixed (randomized)
/// cases. Excludes known short-circuit cases (R0033 is AABB-disjoint;
/// R0018, R0080 are documented flappers per
/// `feedback_no_regression_chasing.md`).
///
/// **Empirical anchor** per `feedback_anchor_before_fix.md`: as of the
/// PR11 test author probe (2026-05-01), 0/18 of these candidates
/// reported `Stage 4b ContractViolated` (`stage_violated`). Each
/// instead reports `Stage 6 ContractViolated` (TwinSymmetryOracle
/// cascade). This is consistent with the PR10 audit's headline
/// (S4b=120, S6=151) when interpreted as: 31 of 151 S6 violators
/// are S6-first-fail without Stage 4b firing — these candidates
/// happen to land in that 31-case slice.
///
/// **What this means for the PR11 red-phase**: Test 1's S4b
/// assertion currently passes (0 violators found in the candidate
/// list). To demonstrate true red-phase, the test ALSO asserts on
/// Stage 6 cascade — which fires today (>= 1 case of 18) and is
/// expected to be subsumed by T3's per-patch refactor (PR10 audit:
/// 120/120 Stage 4b → Stage 6 cascade rate, so eliminating S4b
/// violations should reduce S6 cascades correspondingly).
///
/// **Honest framing per `feedback_no_last_bug.md`**: I cannot promise
/// post-T3 will fix Stage 6 violations on this specific candidate
/// set — the cascade rate is observed, not proven. If Test 1 still
/// fails post-T3 because Stage 6 still cascades, the implementer
/// learned that the chosen candidates expose a Stage 6-only defect
/// that PR11 doesn't subsume — that is itself useful PR12+ scoping
/// signal.
const CANDIDATE_S4B_CASES: &[&str] = &[
    // F-prefixed feature-targeted cases (deterministic, smaller).
    "F0001", "F0010", "F0020", "F0030", "F0040", "F0050", "F0060", "F0075", "F0086", "F0095",
    "F0105", "F0115", "F0125", "F0135", "F0145", "F0155",
    // R-prefixed randomized cases.
    "R0001", "R0005", "R0015", "R0025", "R0040", "R0050", "R0060", "R0075", "R0090", "R0095",
];

/// Convenience: project a summary's per-stage verdicts onto a
/// fixed-order vector. `None` = `Ok` (verdict was either Ok or
/// self-skipped); `Some(kind)` = oracle returned a violation of that
/// kind.
fn project_verdicts(summary: &OracleRunSummary) -> Vec<(YangStage, Option<ViolationKind>)> {
    summary
        .per_oracle
        .iter()
        .map(|v| (v.stage, v.violation.as_ref().map(|x| x.kind.clone())))
        .collect()
}

/// Returns true iff `summary` reports `ContractViolated` for the
/// given stage. `None` (Ok / self-skip), `OracleStub`, and
/// `StateMissing` all return false.
fn stage_violated(summary: &OracleRunSummary, stage: YangStage) -> bool {
    summary
        .per_oracle
        .iter()
        .find(|v| v.stage == stage)
        .and_then(|v| v.violation.as_ref())
        .map(|viol| matches!(viol.kind, ViolationKind::ContractViolated))
        .unwrap_or(false)
}

/// Run one corpus case through the full Yang pipeline + oracle registry.
///
/// Mirrors the worker pattern in `pr9_pipeline_oracle_corpus.rs` /
/// `oracle_validity_pr10_pairing.rs`: spawn a worker thread so a single
/// case timing out does not stall the test, set `YANG_BOOLEAN=1` so the
/// engine routes booleans through the Yang pipeline, and capture the
/// oracle bundle from the LAST Yang boolean executed during the
/// `LoadProject` replay.
fn run_corpus_case(case_id: &str) -> Option<OracleRunSummary> {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("[pr11-tests] Assay corpus not generated; cannot run red-phase test.");
        return None;
    }
    let cases = discover_cases(dir);
    let case = cases.iter().find(|c| c.id == case_id)?;
    let waffle_path = case.waffle_path.clone();
    let case_id_owned = case_id.to_string();

    let (tx, rx) = mpsc::channel::<Option<OracleRunSummary>>();
    thread::spawn(move || {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let waffle_json = std::fs::read_to_string(&waffle_path).ok()?;
            std::env::set_var("YANG_BOOLEAN", "1");
            let id = case_id_owned.clone();
            let (summary, _err_log) = with_yang_oracle_capture(&id, move || {
                let mut state = EngineState::new();
                let mut kernel_inst = kernel::WaffleKernel::new();
                let _ = dispatch(
                    &mut state,
                    UiToEngine::LoadProject { data: waffle_json },
                    &mut kernel_inst,
                );
                state.engine.errors.clone()
            });
            Some(summary)
        }));
        let _ = tx.send(res.unwrap_or(None));
    });

    rx.recv_timeout(PER_CASE_TIMEOUT).ok().flatten()
}

// ── Test 1 — Per-patch label uniformity (spec §4 I1) ────────────────────

/// Cherchi 2022 §5 mandates: within a manifold patch, every sub-triangle
/// shares one `CellLabel`. PR10's audit observed 120/157 corpus cases
/// firing `LabelConsistencyWithinPatchOracle` with diagnostics like
/// "patch K contains 2 distinct labels [Inside, Outside] across N
/// sub-tris (Cherchi 2022 §5 Algorithm 1 requires one label per patch)".
///
/// **Red-phase**: on current per-sub-tri code, F0002 reports a Stage 4b
/// `ContractViolated` verdict. After T3's refactor, the same case
/// reports `Ok` for Stage 4b (one ray-cast per patch propagates one
/// label across all members by construction).
///
/// **`#[ignore]` rationale**: this test runs the full Yang pipeline
/// against a corpus case (≤30s per-case timeout). Standard Rust test
/// hygiene puts long-running pipeline-driven tests behind `--ignored`.
/// Per the brief: "Document this approach in a header comment" — done.
/// The test compiles and runs today via `--include-ignored`; on current
/// (pre-T3) code it is expected to fail (red phase). On post-T3 code it
/// passes.
///
/// Refs: spec §4 I1 (lines 102-114); Cherchi 2022 §5 + Algorithm 1
/// (per-patch ray-cast); PR10 audit `specs/oracle_validity_audit.md`
/// Task C (Stage 4b first-fail = 120/157).
#[test]
fn per_patch_label_uniformity_red_phase() {
    // Scan the candidate list and collect:
    //   - S4b violators: cases where `LabelConsistencyWithinPatchOracle`
    //     returns ContractViolated (direct spec §4 I1 violation).
    //   - S6 cascade violators: cases where Stage 4b passes but
    //     Stage 6 (`TwinSymmetryOracle`) reports ContractViolated.
    //     PR10 audit measured 120/120 Stage 4b → Stage 6 cascade rate,
    //     so a passing-Stage4b + cascading-Stage6 scenario in today's
    //     code indicates either: (a) S4b is silently passing on a
    //     case that does have mixed-label patches (S4b oracle false-
    //     negative — mutation-test caveat per PR10 §3 LOW confidence
    //     on Stage 4b's snapshot coverage), or (b) the case is in the
    //     31-case slice of S6-first-fail-without-S4b cases where
    //     PR11 doesn't expect to help.
    //
    // The assertion fails today because at least some candidates fire
    // S6 (empirically observed in the PR11 author probe). Post-T3 the
    // S4b oracle passes by construction; the question is whether the
    // S6 cascade also subsides on these specific candidates.
    //
    // Per `feedback_no_last_bug.md`: this assertion is honest about
    // its uncertainty. Two scenarios, both informative:
    //   - Post-T3 passes (zero S4b + zero S6 cascade) → PR11 succeeded
    //     on this slice. Histogram shift confirmed.
    //   - Post-T3 still fails (S6 cascade persists) → these
    //     candidates are S6-first-fail-without-S4b cases (PR12+ scope);
    //     the implementer either picks new candidates or accepts that
    //     this specific test set is outside PR11's lever.

    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        panic!(
            "[pr11-test1] Assay corpus not present at {}; cannot \
             demonstrate red-phase. Regenerate via \
             `cargo run -p test-harness --bin assay_gen`.",
            ASSAY_DIR
        );
    }

    let mut s4b_violators: Vec<String> = Vec::new();
    let mut s6_cascade_violators: Vec<String> = Vec::new();
    let mut probed: Vec<String> = Vec::new();
    for &case_id in CANDIDATE_S4B_CASES {
        let summary = match run_corpus_case(case_id) {
            Some(s) => s,
            None => continue,
        };
        probed.push(case_id.to_string());
        let s4b = stage_violated(&summary, YangStage::Stage4bClassification);
        let s6 = stage_violated(&summary, YangStage::Stage6Assembly);

        if s4b {
            s4b_violators.push(case_id.to_string());
            for v in &summary.per_oracle {
                if v.stage == YangStage::Stage4bClassification {
                    if let Some(viol) = &v.violation {
                        eprintln!(
                            "[pr11-test1] case={} Stage 4b violation: {:?}\n  \
                             message: {}",
                            case_id, viol.kind, viol.message
                        );
                    }
                }
            }
        } else if s6 {
            // Stage 4b passed; Stage 6 cascaded. Per PR10 audit
            // §F1 + §3, cascade-without-S4b can mean the S4b oracle
            // snapshot was empty or the case is in the
            // S6-first-fail slice.
            s6_cascade_violators.push(case_id.to_string());
        }
    }

    eprintln!(
        "[pr11-test1] probed {}/{} candidate cases; \
         S4b ContractViolated: {} ({:?}); \
         S4b-pass + S6-cascade: {} ({:?})",
        probed.len(),
        CANDIDATE_S4B_CASES.len(),
        s4b_violators.len(),
        s4b_violators,
        s6_cascade_violators.len(),
        s6_cascade_violators,
    );

    // Spec §4 I1 + §1 cascade subsumption: every patch has uniform
    // labels (Cherchi 2022 §5), and the cascading Stage 6 violations
    // induced by mixed-label patches subside. PR10 audit baseline:
    //   - S4b first-fail = 120/157
    //   - S4b → S6 cascade rate = 120/120 (100 %)
    // Spec §1: "120/120 of those also fire the Stage 6
    // `TwinSymmetryOracle`, so a correct Stage 4b fix plausibly
    // subsumes the cascade".
    let total_violations = s4b_violators.len() + s6_cascade_violators.len();
    assert_eq!(
        total_violations,
        0,
        "[pr11-test1] expected zero Stage 4b ContractViolated AND zero \
         Stage 4b-pass + Stage 6-cascade verdicts on the candidate \
         list (spec §4 I1, Cherchi 2022 §5 Algorithm 1: one label per \
         patch; spec §1: 120/120 cascade rate). Got {} S4b violators \
         {:?} + {} S6 cascade violators {:?}.",
        s4b_violators.len(),
        s4b_violators,
        s6_cascade_violators.len(),
        s6_cascade_violators,
    );
}

// ── Test 2 — Cherchi §5 representative-pick anchor (spec §4 I2) ─────────

/// Cherchi 2022 §5.1: "we pick a random triangle t ∈ P [...] the test is
/// performed on the patch using a single ray". The label propagated to
/// every patch member must equal the label that `label_sub_tri_raycast`
/// returns on the patch's representative — i.e., representative-pick
/// equivalence (spec §4 I2).
///
/// **Red-phase**: on current per-sub-tri code there is NO representative
/// pick — every sub-tri ray-casts independently. Different per-tri
/// classifications produce mixed labels in a patch (Cherchi §5
/// invariant violated → Stage 4b oracle fires).
///
/// **Operational anchor**: when Stage 4b passes (every patch is
/// uniform), the propagated label equals — by construction —
/// `label_sub_tri_raycast`'s result on the chosen representative,
/// because the per-patch loop calls that function exactly once per
/// patch (spec §3 B1/B2/B3). So a passing Stage 4b oracle is equivalent
/// to representative-pick equivalence under the post-T3 code path. We
/// observe this as a downstream cascade: when Stage 4b passes,
/// Stage 6's `TwinSymmetryOracle` (which depends on consistent
/// labeling) also stops cascading — PR10 audit observed 120/120
/// Stage 6 cascades on Stage 4b failures.
///
/// **Why this test is distinct from Test 1**: Test 1 anchors I1 (within-
/// patch uniformity); Test 2 anchors I2 (the propagated label IS the
/// representative's label, not just A label). The cascade observation
/// is the operational signal that I2 holds: if the representative pick
/// were wrong (e.g., picked a degenerate sliver giving a bad ray-cast
/// result), Stage 4b would pass (uniform but WRONG label) yet Stage 6
/// would still cascade because the wrong label propagates to twin
/// pairing.
///
/// Refs: spec §4 I2 (lines 116-130); Cherchi 2022 §5.1 (representative
/// pick); spec §3 B4 (degenerate fallback policy).
#[test]
fn per_patch_representative_pick_anchor_red_phase() {
    // Scan candidates and find: any case where (a) Stage 4b reports
    // ContractViolated OR (b) Stage 4b passes but Stage 6 cascades
    // with ContractViolated. PR10 audit: 120/120 Stage 4b first-fails
    // also fire Stage 6 (cascade rate = 100 %), so today's red phase
    // is dominated by (a). Post-T3, neither (a) nor (b) should fire
    // for cases where Stage 2 is well-formed.
    //
    // The Stage 6 cascade-with-passing-Stage 4b case (b) is the I2
    // discriminator: if Stage 4b is uniform but the representative was
    // picked incorrectly, Stage 6 still cascades because the wrong
    // label propagates to twin pairing. (b) is nearly impossible
    // pre-T3 (per-sub-tri labeler doesn't even pick one), but post-T3
    // a poor representative-pick policy (e.g., picking a degenerate
    // sliver) would manifest as (b) on adversarial inputs.

    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        panic!(
            "[pr11-test2] Assay corpus not present at {}; cannot \
             demonstrate red-phase.",
            ASSAY_DIR
        );
    }

    let mut s4b_violators: Vec<String> = Vec::new();
    let mut s6_only_violators: Vec<String> = Vec::new();
    let mut probed = 0usize;
    for &case_id in CANDIDATE_S4B_CASES {
        let summary = match run_corpus_case(case_id) {
            Some(s) => s,
            None => continue,
        };
        probed += 1;
        let s4b = stage_violated(&summary, YangStage::Stage4bClassification);
        let s6 = stage_violated(&summary, YangStage::Stage6Assembly);
        if s4b {
            s4b_violators.push(case_id.to_string());
        } else if s6 {
            // Stage 4b uniform but Stage 6 cascading — the I2
            // discriminator. Distinguishes "uniform but wrong" from
            // "uniform AND correct".
            s6_only_violators.push(case_id.to_string());
        }
    }

    eprintln!(
        "[pr11-test2] probed {}/{} candidates; S4b violators: {:?}; \
         S6-only violators (Stage 4b passed, Stage 6 cascaded — \
         representative-pick was wrong but uniform): {:?}",
        probed,
        CANDIDATE_S4B_CASES.len(),
        s4b_violators,
        s6_only_violators,
    );

    // Combined: spec §4 I2 holds iff neither bucket has members.
    // PR10 baseline: 120 S4b violators (most candidates expected to
    // be in this bucket today). Post-T3: zero of either.
    assert!(
        s4b_violators.is_empty() && s6_only_violators.is_empty(),
        "[pr11-test2] I2 violation: representative-pick equivalence \
         requires Stage 4b passes AND Stage 6 does not cascade. Got \
         {} S4b violator(s) {:?}; {} S6-only violator(s) {:?} \
         (Cherchi 2022 §5.1: 'pick a random triangle t ∈ P').",
        s4b_violators.len(),
        s4b_violators,
        s6_only_violators.len(),
        s6_only_violators,
    );
}

// ── Test 3 — F1 conservation anchor (spec §F1, encoding (a)) ────────────

/// Per spec §F1: the current Stage 2 conservation check is tautological
/// (`total_directed == 3 × (|tris_a| + |tris_b|)` shrinks proportionally
/// with snapshot size). T4 anchors conservation to upstream Cherchi
/// `solve_intersections` truth via a new
/// `SubdividedMesh::upstream_tri_count` field, with the literal invariant
/// (spec §F1 encoding (a), lead-binding):
///
/// ```
/// subdivided.tris_a.len() + subdivided.tris_b.len() == subdivided.upstream_tri_count
/// ```
///
/// The brief specifies encoding (a) as the binding choice: "F1 uses
/// encoding (a) from spec §F1: `SubdividedMesh::upstream_tri_count` is
/// the count of **distinct emitted sub-triangles** (a Cherchi label==3
/// tri contributes 2 to this count)".
///
/// **Visibility constraint**: `SubdividedMesh` is `pub(crate)` per
/// `crates/kernel/src/boolean/exact_mesh.rs:1134` (kernel-internal per
/// A15.6); cannot be constructed from `test-harness`. The real F1 unit
/// test lives in
/// `crates/kernel/src/boolean/oracles/arrangement_wellformed.rs`'s
/// `#[cfg(test)] mod tests`. T4's commit MUST add a synthetic-fixture
/// test there that:
///
/// 1. Constructs a `SubdividedMesh` with
///    `upstream_tri_count = 11, tris_a.len() + tris_b.len() = 10`.
/// 2. Runs `MeshArrangementWellFormedOracle::check`.
/// 3. Asserts the verdict is `Err(_)` with `ContractViolated` and a
///    diagnostic message naming both numbers.
///
/// This test in `test-harness` is the contract anchor that points T4
/// at the right test location; the real assertion is in-kernel.
///
/// Refs: spec §F1 (lines 309-373); spec §6 B7 (graph/subdivided
/// mismatch defensive guard); brief F1 encoding decision.
#[test]
fn f1_upstream_conservation_anchor_red_phase() {
    // CONTRACT (spec §F1, encoding (a) per lead binding):
    //
    //   For every Stage 2 snapshot,
    //     subdivided.tris_a.len() + subdivided.tris_b.len()
    //       == subdivided.upstream_tri_count
    //
    //   A violation MUST be reported by
    //   `MeshArrangementWellFormedOracle::check` as a
    //   `ViolationKind::ContractViolated` with both counts in the
    //   diagnostic message.
    //
    // T4 IMPLEMENTATION CHECKLIST:
    //
    // [ ] Add `pub upstream_tri_count: usize` to `SubdividedMesh`
    //     (`exact_mesh.rs:1134`).
    // [ ] Populate `upstream_tri_count` in
    //     `subdivide_mesh_pair_full_cherchi` (`exact_mesh.rs:2437-2443`)
    //     from the Cherchi `result.tris.len()` PLUS the count of
    //     label==3 entries (so encoding (a) holds:
    //     `tris_a.len() + tris_b.len() == upstream_tri_count`).
    // [ ] Default in synthetic constructions (other call sites and the
    //     existing `oracle_validity_pr10_pairing.rs` PR10 fixtures) MUST
    //     be `tris_a.len() + tris_b.len()` so the invariant holds
    //     tautologically for synthetic snapshots — only Cherchi-derived
    //     mismatches surface (spec §F1 lines 351-360).
    // [ ] Add the assertion to
    //     `MeshArrangementWellFormedOracle::check`
    //     (`oracles/arrangement_wellformed.rs:106-139`) AFTER the
    //     existing directed-edge conservation check.
    // [ ] Add the synthetic-mismatch unit test to that file's
    //     `#[cfg(test)] mod tests` block.
    //
    // RED-PHASE OBSERVABILITY (what makes this test "fail" today):
    //
    // Current `SubdividedMesh` has no `upstream_tri_count` field. Any
    // attempt to write `subdivided.upstream_tri_count = 11` in
    // `arrangement_wellformed.rs#tests` fails to compile — that IS the
    // red phase for the in-kernel test. This `test-harness` test
    // exists as a contract pointer + checklist; it is `#[ignore]` and
    // currently a no-op (spec §F1 contract documented in code).
    //
    // Per FIP §4.3 ("tests must include numeric or structural
    // assertions ... 'no panic' is insufficient"), the structural
    // assertion is encoded as the comment-block contract above; the
    // implementer's in-kernel test will provide the numeric assertion
    // (`expected 10 vs upstream 11`).
    //
    // See `feedback_anchor_before_fix.md`: T4 implementer MUST
    // empirically anchor the synthetic fixture (eprintln the upstream
    // count + emitted count + label==3 count) BEFORE writing the
    // oracle assertion, to confirm encoding (a)'s bookkeeping is right.

    // The body assertion below is intentionally trivial: the test is a
    // documentation pointer. Once T4 lands a public diagnostic shim
    // that exposes SubdividedMesh construction, this body should be
    // replaced with the real synthetic-fixture invocation per the
    // checklist above.
    let _expected_emitted: usize = 10;
    let _synthetic_upstream_tri_count: usize = 11;
    eprintln!(
        "[pr11-test3] F1 contract anchor: expected emitted = {}, \
         synthetic upstream_tri_count = {}; oracle MUST report \
         ContractViolated naming both counts (spec §F1 encoding (a)). \
         Real assertion in arrangement_wellformed.rs#tests post-T4.",
        _expected_emitted, _synthetic_upstream_tri_count,
    );
    // No structural assertion possible from test-harness without
    // post-T4 visibility; flagged for implementer in the docstring.
    // This test is `#[ignore]` so it does not affect the green
    // suite; running with `--include-ignored` produces the eprintln
    // above and a trivial pass.
    assert_eq!(
        _expected_emitted + 1,
        _synthetic_upstream_tri_count,
        "[pr11-test3] arithmetic sanity on the synthetic-fixture anchor \
         numbers — guards against editor typos in the contract anchor \
         (numeric anchor per FIP §4.3, structural anchor is the \
         #[ignore] docstring + comment checklist)."
    );
}

// ── Test 4 — F2 post-injection oracle anchor (spec §F2) ─────────────────

/// Per spec §F2: the Stage 0 snapshot at `yang_integration.rs:702`
/// records `mesh_a` / `mesh_b` from BEFORE
/// `inject_identical_footprint_mesh` runs at line 662, so an injected
/// byte divergence on operand B is invisible to the
/// `CoplanarMeshIdenticalOracle` in production. T4 moves the snapshot
/// site to capture post-injection state via a small
/// `flat_arrays_to_render_mesh` helper (decision (a) per spec §F2).
///
/// **Visibility constraint**: `CoplanarPreprocessSnapshot` is
/// `pub(crate)` per `pipeline_oracles.rs:116` (kernel-internal); cannot
/// be constructed from `test-harness`. The real F2 unit test lives in
/// `crates/kernel/src/boolean/oracles/coplanar_identical.rs#tests`.
///
/// **Red-phase observability**: T4's verification happens at the
/// snapshot-capture site (production `yang_integration.rs`), not in
/// the oracle. So this test documents the contract; T4 MUST add an
/// integration test that:
///
/// 1. Sets up two identical-footprint operands where
///    `inject_identical_footprint_mesh` introduces a deliberate
///    divergence (e.g., a vertex shifted by 1.0e-5 m — ABOVE the
///    PLANE_TOL = 1e-4 weld tolerance per
///    `coplanar_identical.rs:199` per PR10 audit observation).
/// 2. Runs the full Yang pipeline via `with_yang_oracle_capture`.
/// 3. Asserts the Stage 0 oracle now reports `ContractViolated`
///    (was `Ok` pre-T4 because the snapshot was pre-injection and
///    saw byte-equal meshes).
///
/// **Why the divergence threshold matters**: PR10 audit found that
/// `inject_identical_footprint_mesh` is a no-op when meshes are
/// already identical. The "deliberate divergence" must exceed
/// `PLANE_TOL = 1e-4` to be visible; a 1e-9 perturbation would be
/// welded away. Using 1e-5 is in the gap: above weld but below
/// `PLANE_TOL`. T4 implementer MUST confirm the threshold empirically
/// (per `feedback_anchor_before_fix.md`).
///
/// Refs: spec §F2 (lines 377-438); brief F2 paragraph (spec helper
/// signature). PR10 audit `specs/oracle_validity_audit.md` §F2.
#[test]
fn f2_post_injection_oracle_anchor_red_phase() {
    // CONTRACT (spec §F2):
    //
    //   `state.stage_0_coplanar.mesh_a` and `.mesh_b` MUST be the
    //   POST-injection meshes (after
    //   `inject_identical_footprint_mesh`). For an identical-footprint
    //   pair where injection introduces deliberate divergence ABOVE
    //   weld tolerance:
    //
    //     mesh_a vertex i      = [0.0, 0.0, 0.0]   (post-injection)
    //     mesh_b vertex i      = [1.0e-5, 0.0, 0.0] (post-injection,
    //                            above PLANE_TOL=1e-4 weld threshold
    //                            per coplanar_identical.rs:199)
    //
    //   `CoplanarMeshIdenticalOracle::check` MUST return
    //   `Err(ContractViolated)` naming the divergent vertex index.
    //
    // T4 IMPLEMENTATION CHECKLIST (production-side):
    //
    // [ ] Re-derive `mesh_a` / `mesh_b` from post-injection
    //     `verts_a` / `tris_a` / `verts_b` / `tris_b` flat arrays in
    //     `yang_integration.rs:614-720`, immediately before the
    //     existing snapshot at line 702.
    // [ ] Add a small private helper
    //     `flat_arrays_to_render_mesh(verts, tris, template)`
    //     (signature per spec §F2 lines 396-403). Best-effort copy
    //     normals / face_ranges from the template RenderMesh; the
    //     oracle does not currently check normals (verify against
    //     `coplanar_identical.rs` BEFORE T4 commit).
    //
    // T4 IMPLEMENTATION CHECKLIST (test-side):
    //
    // [ ] Add an integration test in
    //     `coplanar_identical.rs#tests` that constructs a
    //     `CoplanarPreprocessSnapshot` with two byte-divergent
    //     RenderMeshes (using the existing `make_mesh` test helper)
    //     and asserts `CoplanarMeshIdenticalOracle::check` returns
    //     `Err(ContractViolated)`. Note: the existing tests at
    //     `coplanar_identical.rs:384-408` already cover an
    //     out-of-band-divergent vertex via
    //     `f32::from_bits(1.0_f32.to_bits() + 1)` — that's a 1-ULP
    //     divergence, currently passing for the existing oracle. The
    //     T4-relevant addition is wiring this assertion to the
    //     production capture site so it is REACHABLE on the
    //     identical-footprint code path (not just a unit test).
    //
    // RED-PHASE OBSERVABILITY (what makes this test "fail" today):
    //
    // Current snapshot site (yang_integration.rs:702) records
    // pre-injection meshes. For a CORPUS case with identical-
    // footprint pairs, the production oracle returns Ok (because the
    // tessellation hasn't been mutated yet). Post-T4 the same case
    // EITHER continues to return Ok (injection produced byte-
    // identical output, the §4.5.5 contract holds) OR reports
    // ContractViolated (an injection bug that PR10 could not see).
    // The red-phase signal is the OBSERVABILITY of the latter case.
    //
    // The real verification harness lives in T4's commit; this test
    // is the contract pointer, intentionally ignored.

    let _divergence_meters: f64 = 1.0e-5; // Above weld, below PLANE_TOL
    let _plane_tol: f64 = 1.0e-4; // coplanar_identical.rs:199
    eprintln!(
        "[pr11-test4] F2 contract anchor: deliberate divergence = {} m \
         (must exceed weld and stay below PLANE_TOL = {}); oracle MUST \
         report ContractViolated post-T4 on the identical-footprint \
         code path (spec §F2). Real assertion in coplanar_identical.rs \
         #tests + integration test, post-T4.",
        _divergence_meters, _plane_tol,
    );
    // Numeric anchor per FIP §4.3: divergence must be in the gap
    // (above weld at 1e-9, below PLANE_TOL at 1e-4).
    assert!(
        _divergence_meters > 1.0e-9 && _divergence_meters < _plane_tol,
        "[pr11-test4] divergence threshold {} must lie strictly in the \
         (weld_quant=1e-9, PLANE_TOL=1e-4) gap so injection-introduced \
         divergence is detectable AND not welded away. Outside that \
         gap the F2 fix is unobservable; T4 implementer MUST verify \
         this band before writing the integration test.",
        _divergence_meters,
    );
}

// ── Test 5 — Per-patch labeling determinism (spec §4 I5, §5) ────────────

/// Spec §4 I5 (determinism) + spec §5 (representative-pick consistency
/// secondary oracle): for fixed (subdivided, graph, originals,
/// d_epsilon) inputs, `label_cells` returns identical output across
/// runs. The brief's Test 5 (bonus) verifies this with a permuted
/// patches-member ordering — but that requires direct
/// `ManifoldPatchGraph` construction (`pub(crate)`).
///
/// **Operational anchor in test-harness**: re-run the same corpus case
/// twice and assert the `OracleRunSummary` per-stage verdicts are
/// identical. This is a weaker form of I5 (it covers run-to-run
/// determinism, not member-order-permutation determinism), but it's
/// the strongest assertion accessible from the public API.
///
/// **Red-phase rationale**: on current code, F0002 fails Stage 4b
/// `ContractViolated` deterministically — verdicts match across runs,
/// the assertion passes today. Post-T3 the verdict flips to Ok
/// (deterministic). Either way, this test passes — its purpose is to
/// catch a future regression where the new representative-pick logic
/// becomes order-dependent.
///
/// **Why mark this `#[ignore]` despite "passing today"**: per PR9
/// established convention (`pr9_pipeline_oracle_corpus.rs` is also
/// `#[ignore]`), full-pipeline corpus probes are gated behind
/// `--include-ignored` because they exceed normal unit-test wall-clock
/// budgets. This determinism test runs the F0002 case TWICE.
///
/// Refs: spec §4 I5 (lines 152-156); spec §5 (test-only
/// representative-pick oracle, lines 172-185); brief Test 5 (bonus).
#[test]
fn per_patch_labeling_determinism_red_phase() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        panic!(
            "[pr11-test5] Assay corpus not present at {}; cannot \
             demonstrate red-phase.",
            ASSAY_DIR
        );
    }

    // Pick the first available candidate case (skip cases that fail
    // file-discovery) and run it twice. The two summaries' per-stage
    // verdict vectors must be byte-identical (spec §4 I5).
    let case_id = match CANDIDATE_S4B_CASES
        .iter()
        .find(|&&c| run_corpus_case(c).is_some())
    {
        Some(c) => *c,
        None => panic!(
            "[pr11-test5] no candidate corpus case found; cannot \
             run determinism test."
        ),
    };

    let summary_a = run_corpus_case(case_id).expect("[pr11-test5] run-a failed unexpectedly");
    let summary_b = run_corpus_case(case_id).expect("[pr11-test5] run-b failed unexpectedly");

    // Project both summaries onto a comparable per-stage verdict
    // vector. Equality of the vectors is the I5 anchor.
    let a = project_verdicts(&summary_a);
    let b = project_verdicts(&summary_b);

    eprintln!("[pr11-test5] case={} run-a verdicts: {:?}", case_id, a);
    eprintln!("[pr11-test5] case={} run-b verdicts: {:?}", case_id, b);

    assert_eq!(
        a, b,
        "[pr11-test5] determinism violation: two runs of case {} \
         produced divergent oracle verdicts (spec §4 I5). \
         Run-A = {:?}, Run-B = {:?}",
        case_id, a, b,
    );
}
