//! Cut-consumes-body semantics — RED trackers + engine fixture.
//!
//! Spec: `specs/cut_consumes_body.md` (ERROR-census campaign 3). A Cut whose
//! tool ENGULFS the whole target produces a geometrically-correct empty
//! boolean; kernel-v2 loudly returns its typed `EmptyBooleanResult` (the
//! kernel has no empty solid — right at that layer), but the engine records
//! an operation ERROR instead of consuming the body with a warning
//! (standard CAD semantics: removing all material deletes the body).
//!
//! RED today on the direct fixture and all four corpus cases
//! (R0023 / R0027 / R0058 / R0088 — boss + larger engulfing cut); GREEN when
//! the empty-result policy lands (spec §3 branches 2–3).
//!
//! Run: `cargo test -p test-harness --test cut_consumes_body_campaign`

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use test_harness::ModelBuilder;

// ── Engine fixture ──────────────────────────────────────────────────────

/// Small box boss, then a strictly larger cut on the same plane, deeper than
/// the boss: the cut consumes ALL material. Expect NO engine errors, ONE
/// consumed-body warning, ZERO live bodies; a follow-up boss then rebuilds a
/// single live body (spec §5 engine oracle, invariants I2/I3).
#[test]
fn engulfing_cut_consumes_body_then_boss_rebuilds() {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("s1", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], -0.5, -0.5, 1.0, 1.0)
        .expect("boss sketch");
    b.extrude("boss", "s1", 0.5).expect("boss extrude");
    b.rect_sketch("s2", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], -1.5, -1.5, 3.0, 3.0)
        .expect("cut sketch");
    b.extrude_cut("cut", "s2", 2.0).expect("cut feature added");

    let errors = b.engine_errors();
    assert!(
        errors.is_empty(),
        "an engulfing cut must CONSUME the body, not error: {errors:?}"
    );
    assert!(
        b.engine_warnings()
            .iter()
            .any(|w| w.contains("consumed the entire target body")),
        "the consumed-body warning must reach engine_warnings: {:?}",
        b.engine_warnings()
    );
    assert_eq!(
        b.distinct_solid_count(),
        0,
        "no live body may remain after the cut consumed all material"
    );

    // I3: a subsequent boss creates its own body and the model continues.
    b.rect_sketch("s3", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 5.0, 5.0, 1.0, 1.0)
        .expect("post-consume sketch");
    b.extrude("boss2", "s3", 0.5).expect("post-consume boss");
    assert!(
        b.engine_errors().is_empty(),
        "rebuild after a consumed body must stay error-free: {:?}",
        b.engine_errors()
    );
    assert_eq!(
        b.distinct_solid_count(),
        1,
        "the follow-up boss is the single live body"
    );
    // I1, sharpened (2026-09-03, the exact-membership oracle's class C): the
    // follow-up boss must be its OWN body — 1 × 1 × 0.5 — not a union with
    // the consumed box resurrected through the most-recent-body walk (which
    // would read 1.0 here and still count as one live body).
    let mesh = b.tessellate_last().expect("follow-up boss tessellates");
    let vol = test_harness::helpers::mesh_volume(&mesh);
    assert!(
        (vol - 0.5).abs() < 1e-9,
        "the follow-up boss alone has volume 0.5; got {vol} (0.5 + 0.5 would be the consumed box resurrected)"
    );
}

/// Spec §3 branch 3 (adversary): an INTERSECT whose operands' bboxes overlap
/// but whose volumes are disjoint (diagonal boxes) produces no material — the
/// target is consumed with a warning, not an error. (True bbox-disjoint pairs
/// never reach the kernel — the engine pre-checks them.)
#[test]
fn disjoint_volume_intersect_consumes_target() {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("s1", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .expect("box A sketch");
    b.extrude_no_merge("a", "s1", 1.0).expect("box A");
    // Box B: diagonal offset in x/y so AABBs overlap at a corner region but
    // the solids share no volume (B spans [1.5,2.5]² in-plane, A [0,1]²; AABB
    // overlap comes from B's z-range enclosing A's).
    b.rect_sketch("s2", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.5, 1.5, 1.0, 1.0)
        .expect("box B sketch");
    b.extrude_no_merge("bb", "s2", 1.0).expect("box B");
    match b.boolean_intersect("ix", "a", "bb") {
        Ok(_) => {
            assert!(
                b.engine_errors().is_empty(),
                "empty-volume intersect must consume, not error: {:?}",
                b.engine_errors()
            );
            assert!(
                b.engine_warnings()
                    .iter()
                    .any(|w| w.contains("no material")),
                "intersect-consumed warning missing: {:?}",
                b.engine_warnings()
            );
        }
        Err(e) => {
            // The engine may pre-check disjoint bounds and refuse before the
            // kernel — that path is NOT this spec's branch; accept it only if
            // it is the explicit disjoint pre-check, never EmptyBooleanResult.
            let msg = format!("{e}");
            assert!(
                msg.contains("disjoint"),
                "unexpected intersect failure (not the disjoint pre-check, \
                 not consumption): {msg}"
            );
        }
    }
}

// ── Corpus trackers ─────────────────────────────────────────────────────

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

fn replay_failures(case_id: &str) -> Vec<String> {
    let dir = assay_dir();
    let waffle_json = match fs::read_to_string(dir.join(format!("{case_id}.waffle"))) {
        Ok(s) => s,
        Err(e) => return vec![format!("cannot read {case_id}.waffle: {e}")],
    };
    let mut builder = ModelBuilder::kernel_v2();
    if let Err(e) = builder.load(&waffle_json) {
        return vec![format!("LoadProject failed: {e}")];
    }
    builder
        .engine_errors()
        .iter()
        .map(|(id, msg)| format!("error {id}: {msg}"))
        .collect()
}

fn replay_failures_with_timeout(case_id: &str, timeout: Duration) -> Vec<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = case_id.to_string();
    let worker = id.clone();
    let handle = std::thread::spawn(move || {
        let _ = tx.send(replay_failures(&worker));
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => {
            let _ = handle.join();
            r
        }
        Err(_) => vec![format!("{id}: timeout after {}s", timeout.as_secs())],
    }
}

fn assert_no_empty_result_error(case_id: &str) {
    let failures = replay_failures_with_timeout(case_id, Duration::from_secs(200));
    let joined = failures.join("\n  ");
    assert!(
        !failures.iter().any(|f| f.contains("EmptyBooleanResult")),
        "cut-consumes-body RED — {case_id} still records the engulfing cut as \
         an engine ERROR:\n  {joined}"
    );
}

#[test]
fn r0023_engulfing_cut_not_an_error() {
    assert_no_empty_result_error("R0023");
}

#[test]
fn r0027_engulfing_cut_not_an_error() {
    assert_no_empty_result_error("R0027");
}

#[test]
fn r0058_engulfing_cut_not_an_error() {
    assert_no_empty_result_error("R0058");
}

#[test]
fn r0088_engulfing_cut_not_an_error() {
    assert_no_empty_result_error("R0088");
}

/// Corpus adjudication 2026-09-03 (spec §7): in R0007, R0027 and R0088 the
/// second operation's cut CONSUMES the only body (the exact-membership chain
/// reads EMPTY after it — `assay_exact_membership`), so the third operation,
/// a cut, has no material to cut and the engine's typed "requires an
/// existing body" error is the truthful outcome. Their authored
/// `expect_rebuild_error` was the generator's default `false`; corrected to
/// `true` and pinned here so the corrected expectation cannot regress.
#[test]
fn consumed_then_cut_chains_expect_a_rebuild_error() {
    for id in ["R0007", "R0027", "R0088"] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../app/tests/cases/assay/{id}.meta.json"));
        let meta: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("meta readable")).unwrap();
        assert_eq!(
            meta["oracles"]["expect_rebuild_error"],
            serde_json::json!(true),
            "{id}: a cut after the body was consumed must be expected to error"
        );
    }
}
