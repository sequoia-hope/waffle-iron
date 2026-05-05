//! PR-Y15c-fix-2 — RED reproduction tests for the A15.5 surface-tier
//! preservation defect at `result_topology_to_waffle_solid`.
//!
//! Per the PR-Y15c-fix-2 spec
//! (`specs/yang_pr_y15c_fix_2_a15_5_surface_preservation.md`) and the
//! plan (`/home/claude/.claude/plans/reactive-juggling-sloth.md`):
//!
//! - `crates/kernel/src/boolean/yang_integration.rs:204-264` —
//!   `result_topology_to_waffle_solid` takes `_surface_map`
//!   (underscore-prefixed = unused) and at L235-264 unconditionally
//!   writes `SurfaceGeom::Planar` for every face from the Newell
//!   normal, silently discarding the correctly-propagated
//!   `Cylindrical` tag carried in `surface_map`.
//!
//! - A15.5 (`governance/ARCHITECTURAL_INVARIANTS.md:453-472`) requires
//!   that boolean operations preserve surface tier for unmodified
//!   faces: an analytic face must remain analytic post-boolean. The
//!   F0031–F0040 cohort exercises box ± enclosed-cylinder; per
//!   adversary-6's PR-Y15c-fix Phase 0 v3 validation memo §2 the
//!   `surface_map` reaching the assembly carries
//!   `surface_map_breakdown={"Cylindrical":1,"Planar":8}` per case —
//!   the cylindrical tag IS available; assembly silently drops it.
//!
//! Test access path: the public `KernelIntrospect::compute_signature`
//! API derives `TopoSignature.surface_type` from the kernel's
//! internal `face_geometry` map (see
//! `crates/kernel/src/waffle_kernel.rs:2938` —
//! `SurfaceGeom::Cylindrical(_) => "cylindrical".to_string()`). So a
//! cylindrical hole face surviving the boolean must surface a
//! `surface_type == "cylindrical"` entry in the result solid's face
//! signatures. This avoids needing to expose `WaffleSolid` or
//! `face_geometry` (both `pub(crate)` to the kernel).
//!
//! Per the spec §5 RED-phase requirement: F0031 / F0040 / cohort tests
//! MUST currently fail; F0003 / R0020 / R0021 control tests MUST
//! currently pass. test-author-a does NOT touch any kernel source;
//! the fix is implementer-j's job (FIP §1 + §4.1 separation).
//!
//! All tests `#[ignore]` (gated by manual invocation with
//! `YANG_BOOLEAN=1` set in the harness — the runner sets it
//! per-call). The 60-s WAFFLE_TIMEOUT thread-wrap follows PR-Y15b
//! precedent (`pr_y15b_combined_failures_parity.rs:49`) for hygiene
//! against R0071-class hangs even though F-cases here are fast.

use std::path::Path;
use std::time::Duration;

use test_harness::assay::randomized_runner::run_single_case;
use test_harness::assay::scoring::{AssayResult, AssayStatus};

/// Per-case Waffle timeout. Matches PR-Y15b precedent
/// (`pr_y15b_combined_failures_parity.rs:49`). F-cases in this cohort
/// are fast (<5s); 60 s is generous and protects against any
/// R0071-class hang showing up unexpectedly.
const WAFFLE_TIMEOUT: Duration = Duration::from_secs(60);

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Run a case under `YANG_BOOLEAN=1` on a worker thread with a
/// per-case timeout. Returns the `AssayResult` or `None` on timeout
/// / case-not-found. Mirrors the PR-Y15b pattern at
/// `pr_y15b_combined_failures_parity.rs:94-130`.
fn run_case_with_timeout(dir: &Path, case_id: &str) -> (Option<AssayResult>, bool) {
    std::env::set_var("YANG_BOOLEAN", "1");

    let dir_owned = dir.to_path_buf();
    let id_owned = case_id.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let r = run_single_case(&dir_owned, &id_owned, true);
        let _ = tx.send(r);
    });
    match rx.recv_timeout(WAFFLE_TIMEOUT) {
        Ok(r) => {
            let _ = handle.join();
            (r, false)
        }
        Err(_) => {
            eprintln!(
                "[pr-y15c-fix-2] WAFFLE_TIMEOUT on {} after {}s — leaking thread",
                case_id,
                WAFFLE_TIMEOUT.as_secs()
            );
            (None, true)
        }
    }
}

/// Replay a case through `ModelBuilder::kernel().load(...)`, find the
/// last non-suppressed feature with a solid output, and return the
/// `(planar_count, cylindrical_count, conical_count, spherical_count,
/// toroidal_count, none_count, total)` from `face_signatures()`.
///
/// This mirrors `tessellate_last_with_tol`'s feature-walk pattern
/// (`crates/test-harness/src/workflow.rs:1302-1322`) but inspects
/// `surface_type` strings via the public `KernelIntrospect` path
/// (`compute_all_signatures` → `TopoSignature.surface_type`)
/// instead of running the tessellator. Returns `Err` on load
/// failure / no solid (so the test can attribute the cause).
fn surface_type_breakdown(dir: &Path, case_id: &str) -> Result<SurfaceBreakdown, String> {
    use test_harness::workflow::ModelBuilder;
    use waffle_types::TopoKind;

    let waffle_path = dir.join(format!("{}.waffle", case_id));
    let waffle_json = std::fs::read_to_string(&waffle_path)
        .map_err(|e| format!("cannot read {}: {}", waffle_path.display(), e))?;

    let mut builder = ModelBuilder::kernel();
    builder
        .load(&waffle_json)
        .map_err(|e| format!("LoadProject failed: {}", e))?;

    // Find the last non-suppressed feature with a solid output —
    // mirrors workflow.rs:1302-1322 (`tessellate_last_with_tol`).
    let tree = &builder.state.engine.tree;
    let limit = tree.active_index.unwrap_or(tree.features.len());
    let mut handle_opt = None;
    for feature in tree.features[..limit].iter().rev() {
        if feature.suppressed {
            continue;
        }
        if let Some(result) = builder.state.engine.get_result(feature.id) {
            if !result.outputs.is_empty() {
                handle_opt = Some(result.outputs[0].1.handle.clone());
                break;
            }
        }
    }
    let handle = handle_opt.ok_or_else(|| {
        format!(
            "no active feature produced a solid (engine_errors={})",
            builder.engine_errors().len()
        )
    })?;

    let introspect = builder.kernel_ref().as_introspect();
    let sigs = introspect.compute_all_signatures(&handle, TopoKind::Face);

    let mut bd = SurfaceBreakdown {
        total: sigs.len(),
        ..Default::default()
    };
    for (_, sig) in &sigs {
        match sig.surface_type.as_deref() {
            Some("planar") => bd.planar += 1,
            Some("cylindrical") => bd.cylindrical += 1,
            Some("conical") => bd.conical += 1,
            Some("spherical") => bd.spherical += 1,
            Some("toroidal") => bd.toroidal += 1,
            Some(other) => bd.other.push(other.to_string()),
            None => bd.untagged += 1,
        }
    }
    Ok(bd)
}

#[derive(Debug, Default, Clone)]
struct SurfaceBreakdown {
    planar: usize,
    cylindrical: usize,
    conical: usize,
    spherical: usize,
    toroidal: usize,
    untagged: usize,
    other: Vec<String>,
    total: usize,
}

impl std::fmt::Display for SurfaceBreakdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "total={} planar={} cylindrical={} conical={} spherical={} toroidal={} untagged={} other={:?}",
            self.total,
            self.planar,
            self.cylindrical,
            self.conical,
            self.spherical,
            self.toroidal,
            self.untagged,
            self.other
        )
    }
}

// ── Test 1: F0031 cylindrical-tag preservation ──────────────────────
//
// RED today: per adversary-6's surface_map_breakdown evidence, the
// surface_map reaching `result_topology_to_waffle_solid` carries
// `Cylindrical:1, Planar:8` — but assembly silently writes Planar for
// every face. Post-fix (PR-Y15c-fix-2 implementer-j) this MUST report
// ≥1 cylindrical face on the result solid.
#[test]
#[ignore]
fn test_f0031_cylindrical_tag_preserved() {
    std::env::set_var("YANG_BOOLEAN", "1");
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[pr-y15c-fix-2] SKIP: assay corpus dir `{}` not present",
            dir.display()
        );
        return;
    }
    let bd = match surface_type_breakdown(dir, "F0031") {
        Ok(b) => b,
        Err(e) => panic!("F0031 surface_type_breakdown failed: {}", e),
    };
    eprintln!("[pr-y15c-fix-2] F0031 surface breakdown: {}", bd);
    assert!(
        bd.cylindrical >= 1,
        "F0031 A15.5 violation: result solid has 0 cylindrical faces \
         (breakdown: {}). Per adversary-6's PR-Y15c-fix Phase 0 v3 \
         validation memo §2, surface_map carries Cylindrical:1 for \
         the enclosed-cylinder hole; assembly must preserve it. \
         Spec: specs/yang_pr_y15c_fix_2_a15_5_surface_preservation.md",
        bd
    );
}

// ── Test 2: F0040 cylindrical-tag preservation ──────────────────────
//
// F0040 is the operand-order mirror of F0031 (per implementer-i's
// diagnostic §"Verbatim probe output — F0040"). Same defect class;
// pinned separately so a regression on either case fires its own
// failure record.
#[test]
#[ignore]
fn test_f0040_cylindrical_tag_preserved() {
    std::env::set_var("YANG_BOOLEAN", "1");
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[pr-y15c-fix-2] SKIP: assay corpus dir `{}` not present",
            dir.display()
        );
        return;
    }
    let bd = match surface_type_breakdown(dir, "F0040") {
        Ok(b) => b,
        Err(e) => panic!("F0040 surface_type_breakdown failed: {}", e),
    };
    eprintln!("[pr-y15c-fix-2] F0040 surface breakdown: {}", bd);
    assert!(
        bd.cylindrical >= 1,
        "F0040 A15.5 violation: result solid has 0 cylindrical faces \
         (breakdown: {}). Per adversary-6's PR-Y15c-fix Phase 0 v3 \
         validation memo §2 (operand-order mirror of F0031), \
         surface_map carries Cylindrical:1 for the enclosed-cylinder \
         hole; assembly must preserve it.",
        bd
    );
}

// ── Test 3: F0031–F0040 cohort homogeneity ──────────────────────────
//
// All 10 cases are box ± enclosed-cylinder per the cohort
// classification. Each result mesh should carry ≥1 cylindrical face.
// Iterates the cohort and reports per-case results in a single
// combined panic so a partial failure (e.g. only F0031–F0035 pass
// post-fix) is clearly attributed.
#[test]
#[ignore]
fn test_f0031_f0040_cohort_cylindrical_homogeneity() {
    std::env::set_var("YANG_BOOLEAN", "1");
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[pr-y15c-fix-2] SKIP: assay corpus dir `{}` not present",
            dir.display()
        );
        return;
    }

    let cases = [
        "F0031", "F0032", "F0033", "F0034", "F0035", "F0036", "F0037", "F0038", "F0039", "F0040",
    ];
    let mut failures: Vec<String> = Vec::new();

    for case_id in cases.iter() {
        // First confirm the WAFFLE_TIMEOUT-protected runner doesn't
        // hang on this case (avoids stalling the whole cohort).
        let (case_result, timed_out) = run_case_with_timeout(dir, case_id);
        if timed_out {
            failures.push(format!(
                "{}: WAFFLE_TIMEOUT after {}s (unexpected — F-cases are fast)",
                case_id,
                WAFFLE_TIMEOUT.as_secs()
            ));
            continue;
        }
        if case_result.is_none() {
            failures.push(format!("{}: not found in corpus", case_id));
            continue;
        }

        match surface_type_breakdown(dir, case_id) {
            Ok(bd) => {
                eprintln!("[pr-y15c-fix-2] {} surface breakdown: {}", case_id, bd);
                if bd.cylindrical < 1 {
                    failures.push(format!(
                        "{}: 0 cylindrical faces (breakdown: {})",
                        case_id, bd
                    ));
                }
            }
            Err(e) => {
                failures.push(format!("{}: surface_type_breakdown failed: {}", case_id, e));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "F0031–F0040 cohort A15.5 violation: {} of {} cases lack ≥1 \
             cylindrical face on result solid:\n  - {}\n\
             Per adversary-6's PR-Y15c-fix Phase 0 v3 validation memo §2, \
             every case's surface_map carries Cylindrical:1.",
            failures.len(),
            cases.len(),
            failures.join("\n  - ")
        );
    }
}

// ── Test 4: F0003 control — pure-planar boss preserved ──────────────
//
// F0003 is a pass-boss-only case (no cylinder operand). Its result
// solid MUST construct successfully (`face_signatures()` returns
// non-empty), AND `face_geometry` must contain ONLY planar entries
// (no false-positive cylindricals from spurious `surface_map`
// entries). This is the kill switch that catches a fix overshoot:
// if PR-Y15c-fix-2 accidentally writes Cylindrical for a face that
// has no Cylindrical source in `surface_map`, F0003 will surface
// it.
//
// PASSES today (F0003 status=pass per `app/tests/cases/assay/results.json`).
// MUST STAY PASSING post-fix.
#[test]
#[ignore]
fn test_f0003_planar_only_control() {
    std::env::set_var("YANG_BOOLEAN", "1");
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[pr-y15c-fix-2] SKIP: assay corpus dir `{}` not present",
            dir.display()
        );
        return;
    }

    // First confirm the case still passes the assay (status=Passed).
    let (case_result, timed_out) = run_case_with_timeout(dir, "F0003");
    if timed_out {
        panic!(
            "F0003: unexpected WAFFLE_TIMEOUT after {}s. F0003 is \
             pass-boss-only and historically completes in <5s.",
            WAFFLE_TIMEOUT.as_secs()
        );
    }
    let case = case_result.expect("F0003 must exist in corpus");
    assert_eq!(
        case.status,
        AssayStatus::Passed,
        "F0003 control regression: assay status={:?} (expected Passed). \
         PR-Y15c-fix-2 MUST NOT break the pass-boss-only path. \
         Detail: {}",
        case.status,
        case.detail
    );

    // Now inspect the result solid's surface tags directly.
    let bd = match surface_type_breakdown(dir, "F0003") {
        Ok(b) => b,
        Err(e) => panic!("F0003 surface_type_breakdown failed: {}", e),
    };
    eprintln!("[pr-y15c-fix-2] F0003 surface breakdown: {}", bd);
    assert!(
        bd.total > 0,
        "F0003 control regression: result solid has 0 faces (breakdown: {})",
        bd
    );
    assert_eq!(
        bd.cylindrical, 0,
        "F0003 control regression: pure-planar boss has {} cylindrical \
         face(s) (expected 0). PR-Y15c-fix-2 may be over-writing \
         Cylindrical from a spurious surface_map entry. Breakdown: {}",
        bd.cylindrical, bd
    );
    assert_eq!(
        bd.conical, 0,
        "F0003 control regression: pure-planar boss has {} conical \
         face(s) (expected 0). Breakdown: {}",
        bd.conical, bd
    );
    assert_eq!(
        bd.spherical, 0,
        "F0003 control regression: pure-planar boss has {} spherical \
         face(s) (expected 0). Breakdown: {}",
        bd.spherical, bd
    );
    assert_eq!(
        bd.toroidal, 0,
        "F0003 control regression: pure-planar boss has {} toroidal \
         face(s) (expected 0). Breakdown: {}",
        bd.toroidal, bd
    );
    // Planar count should equal total (all faces tagged, no untagged
    // and no curved variants).
    assert_eq!(
        bd.planar, bd.total,
        "F0003 control regression: planar={} but total={} (untagged={}, \
         other={:?}). Pure-planar boss should have every face tagged Planar. \
         Breakdown: {}",
        bd.planar, bd.total, bd.untagged, bd.other, bd
    );
}

// ── Test 5: R0020 + R0021 controls — no regression in pass/fail state ──
//
// R0020 and R0021 are PR14's original Render-LOD targets. They live
// in the same `result_topology_to_waffle_solid` code path as the
// F0031–F0040 cohort but exercise different operand shapes
// (revolves, gear extrudes). Per current `results.json`:
//
//   - R0020: status=fail, detail starts with "partial rebuild (1 error(s))"
//   - R0021: status=fail, detail starts with "auto-union-failed"
//
// The test asserts the FAILURE MODE (status + detail prefix) does
// not change post-fix — the fix MUST NOT cause new failures or mask
// existing ones on these cases. Per spec §6 risk #3: if either
// regresses, the fix has wider impact than expected and adversary-7
// catches it on the corpus sweep.
#[test]
#[ignore]
fn test_r0020_r0021_no_regression() {
    std::env::set_var("YANG_BOOLEAN", "1");
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[pr-y15c-fix-2] SKIP: assay corpus dir `{}` not present",
            dir.display()
        );
        return;
    }

    // Current baseline per app/tests/cases/assay/results.json
    // (refreshed 2026-04-02 per memory). Status MUST stay Failed;
    // detail MUST contain the diagnostic prefix that classifies the
    // failure mode. PR-Y15c-fix-2 may legitimately CHANGE other
    // diagnostics in the detail string (it's about surface tags),
    // but it MUST NOT change the primary failure category.
    let baselines = [
        ("R0020", AssayStatus::Failed, "partial rebuild"),
        ("R0021", AssayStatus::Failed, "auto-union-failed"),
    ];
    let mut failures: Vec<String> = Vec::new();

    for (case_id, expected_status, expected_detail_substr) in baselines.iter() {
        let (case_result, timed_out) = run_case_with_timeout(dir, case_id);
        if timed_out {
            failures.push(format!(
                "{}: unexpected WAFFLE_TIMEOUT after {}s",
                case_id,
                WAFFLE_TIMEOUT.as_secs()
            ));
            continue;
        }
        let case = match case_result {
            Some(c) => c,
            None => {
                failures.push(format!("{}: not found in corpus", case_id));
                continue;
            }
        };
        eprintln!(
            "[pr-y15c-fix-2] {} status={:?} detail={}",
            case_id, case.status, case.detail
        );
        if case.status != *expected_status {
            failures.push(format!(
                "{}: status changed from {:?} to {:?} — detail={}",
                case_id, expected_status, case.status, case.detail
            ));
            continue;
        }
        if !case.detail.contains(expected_detail_substr) {
            failures.push(format!(
                "{}: detail no longer contains `{}` — detail={}",
                case_id, expected_detail_substr, case.detail
            ));
        }
    }

    if !failures.is_empty() {
        panic!(
            "R0020/R0021 regression: {} sub-failures:\n  - {}",
            failures.len(),
            failures.join("\n  - ")
        );
    }
}
