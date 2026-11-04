//! Kernel 1.0 Gate 3.1 — 21-pair surface-pair boolean matrix (`assay_pair_matrix`).
//!
//! Exercises all 21 unordered pairs of `{plane, cylinder, cone, sphere, torus,
//! B-spline}` across `{union, cut, intersect}` in general position through the
//! same `categorize` oracle pipeline and CPU-budgeted subprocess runner as
//! `tests/assay_kv2.rs`.
//!
//! Always-on unit tests verify pair enumeration, deterministic `.waffle`
//! generation, general-position geometry invariants, `ExactChain` coverage, and
//! that each standalone primitive recipe produces its intended analytic surface
//! in `kernel-v2`.
//!
//! Full matrix measurement (`#[ignore]`):
//!   `PAIR_MATRIX_SEEDS=3 ASSAY_JOBS=6 cargo test --release -p test-harness --test assay_pair_matrix surface_pair_matrix -- --ignored --exact --nocapture`
//!
//! Single case diagnosis / subprocess worker (`#[ignore]`):
//!   `ASSAY_CASE=pm_01_plane_cylinder_union_s0 cargo test --release -p test-harness --test assay_pair_matrix single_case -- --ignored --exact --nocapture`

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;
use test_harness::assay::categorize::{categorize_with_exact_volume, CaseOutcome, Category};
use test_harness::assay::cpu_runner::{
    default_assay_jobs, emit_assay_outcome_with_extra, run_batch_with_extra,
    run_with_cpu_timeout_map, RunProvenance,
};
use test_harness::assay::exact_membership::ExactChain;
use test_harness::assay::pair_matrix::{
    build_pair_case, build_report, build_report_at_commit, draw_pair_operands, format_report_table,
    parse_case_id, resolve_git_commit, verify_general_position, ExactVolumeRecord, MatrixOp,
    OperandGeom, PairOperands, SurfaceKind, ALL_SURFACE_PAIRS, NOT_BUILDABLE_BSPLINE,
};
use test_harness::ModelBuilder;

/// Default per-case CPU budget matching the corpus run (`ASSAY_CASE_TIMEOUT_SECS=900`
/// in `GEMINI.md` / `CLAUDE.md`; `torus × torus` union takes ~154s per seed in
/// `--release`, too close to the earlier 180s default).
const DEFAULT_CASE_TIMEOUT_SECS: u64 = 900;

fn parse_timeout_secs(raw: Option<&str>) -> u64 {
    raw.and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CASE_TIMEOUT_SECS)
}

fn case_timeout_secs() -> u64 {
    parse_timeout_secs(std::env::var("ASSAY_CASE_TIMEOUT_SECS").ok().as_deref())
}

fn pair_matrix_out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/pair-matrix")
}

fn evaluate_pair_case(id: &str) -> (CaseOutcome, Option<ExactVolumeRecord>) {
    let err_outcome = |detail: String| {
        (
            CaseOutcome {
                id: id.to_string(),
                category: Category::Error,
                detail,
            },
            None,
        )
    };
    let Some((pair, op, seed)) = parse_case_id(id) else {
        return err_outcome(format!("invalid pair-matrix case id: {id}"));
    };
    let generated = match build_pair_case(pair, op, seed) {
        Ok(g) => g,
        Err(e) => return err_outcome(format!("generator failed: {e}")),
    };
    let (outcome, exact_vol) =
        categorize_with_exact_volume(&generated.id, &generated.waffle_json, &generated.meta);
    (
        outcome,
        exact_vol.as_ref().map(ExactVolumeRecord::from_verdict),
    )
}

fn evaluate_pair_case_with_timeout(
    id: &str,
    budget: Duration,
) -> (CaseOutcome, Option<ExactVolumeRecord>) {
    let owned = id.to_string();
    run_with_cpu_timeout_map(
        id,
        budget,
        move || evaluate_pair_case(&owned),
        |o| (o, None),
    )
}

// ── Always-on generator & surface-construction invariants ───────────────────

#[test]
fn enumerates_all_21_unordered_surface_pairs() {
    assert_eq!(ALL_SURFACE_PAIRS.len(), 21);
    let mut seen = BTreeSet::new();
    let mut buildable = 0usize;
    let mut not_buildable = 0usize;
    for (idx, pair) in ALL_SURFACE_PAIRS.iter().enumerate() {
        assert_eq!(pair.index, idx);
        assert!(
            pair.a <= pair.b,
            "pair {} ({}) must be canonical unordered (a <= b)",
            idx,
            pair.label()
        );
        assert!(
            seen.insert((pair.a, pair.b)),
            "duplicate surface pair: {}",
            pair.label()
        );
        if pair.is_buildable() {
            buildable += 1;
            assert!(pair.a.feature_recipe().is_some());
            assert!(pair.b.feature_recipe().is_some());
        } else {
            not_buildable += 1;
            assert_eq!(pair.b, SurfaceKind::BSpline);
            assert_eq!(pair.feature_sources_summary(), NOT_BUILDABLE_BSPLINE);
        }
        for op in MatrixOp::ALL {
            let cid = pair.case_id(op, 7);
            assert_eq!(parse_case_id(&cid), Some((*pair, op, 7)));
        }
    }
    assert_eq!(buildable, 15, "15 analytic-analytic unordered pairs");
    assert_eq!(not_buildable, 6, "6 B-spline pairs honestly not buildable");
}

#[test]
fn generated_documents_are_deterministic_and_in_general_position() {
    for pair in ALL_SURFACE_PAIRS {
        if !pair.is_buildable() {
            assert!(draw_pair_operands(pair, 0).is_none());
            let err = build_pair_case(pair, MatrixOp::Union, 0).unwrap_err();
            assert!(err.contains(NOT_BUILDABLE_BSPLINE), "{err}");
            continue;
        }
        for seed in 0..3u64 {
            let ops = draw_pair_operands(pair, seed).expect("buildable pair operands");
            assert_eq!(ops.a.kind(), pair.a);
            assert_eq!(ops.b.kind(), pair.b);
            verify_general_position(&ops).unwrap_or_else(|e| {
                panic!(
                    "general-position violation on {} seed {seed}: {e}",
                    pair.label()
                )
            });

            for op in MatrixOp::ALL {
                let c1 = build_pair_case(pair, op, seed).expect("build case 1");
                let c2 = build_pair_case(pair, op, seed).expect("build case 2");
                assert_eq!(
                    c1.waffle_json, c2.waffle_json,
                    "non-deterministic .waffle output for {}",
                    c1.id
                );
                assert!(c1.meta.oracles.derived_meta);
                assert!(!c1.meta.oracles.expect_rebuild_error);

                let doc: Value = serde_json::from_str(&c1.waffle_json).unwrap();
                let has_boolean = doc
                    .pointer("/tabs/0/kind/features/features")
                    .and_then(Value::as_array)
                    .is_some_and(|fs| {
                        fs.iter().any(|f| {
                            f.pointer("/operation/type").and_then(Value::as_str)
                                == Some("BooleanCombine")
                        })
                    });
                assert!(has_boolean, "{}: missing BooleanCombine feature", c1.id);

                let chain = ExactChain::from_waffle(&doc).unwrap_or_else(|e| {
                    panic!("{}: ExactChain did not cover generated doc: {e:?}", c1.id)
                });
                assert_eq!(chain.ops.len(), 2);
                assert_eq!(chain.merges.len(), 1);
            }
        }
    }
}

#[test]
fn each_surface_recipe_builds_its_target_analytic_surface_in_kernel_v2() {
    // Pairs 0..5 have `b` equal to Plane, Cylinder, Cone, Sphere, Torus in turn.
    for &pair in ALL_SURFACE_PAIRS.iter().take(5) {
        let kind = pair.b;
        let expected_stype = kind
            .introspect_surface_type()
            .expect("analytic surface type");
        let ops = draw_pair_operands(pair, 0).expect("operands");
        assert_eq!(ops.b.kind(), kind);

        let mut builder = ModelBuilder::kernel_v2();
        ops.b
            .add_to_builder(&mut builder, "sk_test", "body_test")
            .unwrap_or_else(|e| panic!("failed to build {} operand: {e}", kind.label()));
        builder.assert_no_errors().unwrap();

        let sigs = builder.face_signatures("body_test").unwrap();
        let has_target_surface = sigs
            .iter()
            .any(|(_, sig)| sig.surface_type.as_deref() == Some(expected_stype));
        assert!(
            has_target_surface,
            "{} recipe did not produce a {expected_stype:?} face; got {:?}",
            kind.label(),
            sigs.iter()
                .map(|(_, s)| s.surface_type.clone())
                .collect::<Vec<_>>()
        );

        let mesh = builder.tessellate("body_test").unwrap();
        let wt = test_harness::oracle::check_watertight_mesh(&mesh);
        assert!(
            wt.passed,
            "{} standalone operand mesh not watertight: {}",
            kind.label(),
            wt.detail
        );
        let vol = test_harness::helpers::mesh_signed_volume(&mesh);
        assert!(vol > 0.1, "{}: unexpected mesh volume {vol}", kind.label());
    }
}

#[test]
fn report_and_table_distinguish_volume_agree_not_covered_and_flag() {
    let outcomes = vec![
        (
            CaseOutcome {
                id: "pm_00_plane_plane_union_s0".to_string(),
                category: Category::SupportedCorrect,
                detail: "all checks passed".to_string(),
            },
            Some(ExactVolumeRecord::Agree {
                rel: 1.2e-4,
                band: 1.4e-2,
            }),
        ),
        (
            CaseOutcome {
                id: "pm_01_plane_cylinder_union_s0".to_string(),
                category: Category::SupportedCorrect,
                detail: "all checks passed".to_string(),
            },
            Some(ExactVolumeRecord::NotCovered {
                reason: "lattice too coarse: surface cubes are 31 % of the volume".to_string(),
            }),
        ),
        (
            CaseOutcome {
                id: "pm_02_plane_cone_union_s0".to_string(),
                category: Category::SupportedWrong,
                detail: "exact_volume: mismatch".to_string(),
            },
            Some(ExactVolumeRecord::Flag {
                rel: 0.16,
                band: 0.02,
                exact: 4.0,
                kernel: 4.64,
            }),
        ),
    ];
    let prov = RunProvenance::capture(1, 900, 1.0);
    let report = build_report_at_commit("abc12345".to_string(), 1, &outcomes, &prov);
    assert_eq!(report.commit, "abc12345");
    assert_eq!(report.seeds_per_op, 1);
    assert!(!resolve_git_commit().is_empty());
    assert_eq!(report.pairs_with_correct_customer, 2);
    assert_eq!(report.pairs_with_correct_volume_agreement, 1);
    assert!(report.rows[0].has_correct_customer);
    assert!(report.rows[0].has_correct_with_volume_agreement);
    assert!(report.rows[1].has_correct_customer);
    assert!(!report.rows[1].has_correct_with_volume_agreement);
    assert_eq!(report.totals.correct, 2);
    assert_eq!(report.totals.correct_volume_agree, 1);
    assert_eq!(report.totals.correct_volume_not_covered, 1);
    assert_eq!(report.totals.exact_volume_agree, 1);
    assert_eq!(report.totals.exact_volume_not_covered, 1);
    assert_eq!(report.totals.exact_volume_flag, 1);

    let table = format_report_table(&report);
    assert!(table.contains("commit abc12345, 1 seed(s) per (pair, op)"));
    assert!(table.contains("CORRECT with volume agreement"));
    assert!(table.contains("YES (1/3 ops, 1/1 C)"));
    assert!(table.contains("NO (0/3 ops, 0/1 C — NotCovered)"));
    assert!(table.contains("NotCovered(lattice too coarse: surface cubes are 31 % of the volume)"));
    assert!(table.contains("Flag(rel=+1.60e-1, band=2.00e-2"));
    assert_eq!(parse_timeout_secs(None), 900);
    assert_eq!(parse_timeout_secs(Some("240")), 240);
}

#[test]
fn sphere_revolve_poles_along_oblique_axis_are_checked_for_clearance() {
    // Pair 03 is `plane × sphere`. Construct a fixture where the plane passes
    // through both `crossing_point` AND the sphere's oblique north revolve pole
    // `center + R·axis_dir`, while staying well clear (> 0.5) of world ±Z / +X.
    let pair = ALL_SURFACE_PAIRS[3];
    let center = [0.0, 0.0, 0.0];
    let radius = 2.0;
    let axis_dir = [0.6, 0.8, 0.0]; // unit oblique axis in XY
    let sketch_normal = [0.0, 0.0, 1.0];
    // Choose crossing_point on the sphere (`|p| = 2`) with normal `[0.0, 0.6, 0.8]`.
    let sphere_n_at_cross = [0.0, 0.6, 0.8];
    let p_cross = [0.0, 1.2, 1.6];
    // Oblique pole `center + R·axis_dir = [1.2, 1.6, 0.0]`.
    // Vector `pole - p_cross = [1.2, 0.4, -1.6]`; choose plane normal
    // perpendicular to `[1.2, 0.4, -1.6]`, e.g. `normalize([0.0, 4.0, 1.0])`:
    //   `dot(plane_n, pole - p_cross) = 1.6 - 1.6 = 0` -> pole lies ON plane!
    //   `|dot(plane_n, sphere_n_at_cross)| = (2.4 + 0.8) / sqrt(17) ≈ 0.776` ∈ [0.18, 0.82].
    //   World ±Z / +X distances to plane through `p_cross`:
    //     `+Z` `[0,0,2]`: `|(0 - 1.2)*4 + (2 - 1.6)*1| / sqrt(17) = 4.4 / 4.123 = 1.067`
    //     `-Z` `[0,0,-2]`: `|(-1.2)*4 + (-3.6)*1| / sqrt(17) = 8.4 / 4.123 = 2.037`
    //     `+X` `[2,0,0]`: `|(-1.2)*4 + (-1.6)*1| / sqrt(17) = 6.4 / 4.123 = 1.552`
    let inv = 17.0_f64.sqrt().recip();
    let plane_n = [0.0, 4.0 * inv, inv];
    let ops = PairOperands {
        pair,
        seed: 99,
        crossing_point: p_cross,
        a: OperandGeom::Prism {
            sketch_origin: p_cross,
            sketch_normal: [0.0, -4.0 * inv, -inv],
            vertices_uv: vec![
                (2.0, 0.0),
                (0.6, 1.9),
                (-1.6, 1.2),
                (-1.6, -1.2),
                (0.6, -1.9),
            ],
            depth: 2.8,
            target_normal: plane_n,
        },
        b: OperandGeom::Sphere {
            center,
            sketch_normal,
            axis_dir,
            radius,
            target_normal_at_cross: sphere_n_at_cross,
        },
    };
    let err = verify_general_position(&ops)
        .expect_err("oblique sphere pole on partner plane must fail general-position check");
    assert!(
        err.contains("seam/pole"),
        "expected seam/pole clearance rejection, got: {err}"
    );
}

// ── Single-case runner (manual diagnosis + subprocess child entry) ──────────

#[test]
#[ignore] // manual / driver child: ASSAY_CASE=pm_01_plane_cylinder_union_s0 ...
fn single_case() {
    let Ok(id) = std::env::var("ASSAY_CASE") else {
        println!("single_case: ASSAY_CASE not set — nothing to do (manual/driver runner)");
        return;
    };
    let timeout_secs = case_timeout_secs();
    let t0 = Instant::now();
    let (outcome, exact_vol) =
        evaluate_pair_case_with_timeout(&id, Duration::from_secs(timeout_secs));
    let ev_label = exact_vol
        .as_ref()
        .map_or_else(|| "not reached".to_string(), ExactVolumeRecord::table_label);
    println!(
        "{id}: {} [exact_volume: {ev_label}] ({:.1}s) — {}",
        outcome.category.label(),
        t0.elapsed().as_secs_f64(),
        outcome.detail
    );
    let extra = exact_vol
        .as_ref()
        .map(|ev| serde_json::to_value(ev).expect("serialize ExactVolumeRecord"));
    emit_assay_outcome_with_extra(&outcome, extra.as_ref());
}

// ── Full 21-pair matrix measurement ─────────────────────────────────────────

#[test]
#[ignore] // full 21-pair matrix; run with --release -- --ignored --nocapture
fn surface_pair_matrix() {
    let seeds_per_op: usize = std::env::var("PAIR_MATRIX_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3)
        .max(1);
    let timeout_secs = case_timeout_secs();
    let jobs = default_assay_jobs();

    let mut case_ids = Vec::new();
    for pair in ALL_SURFACE_PAIRS {
        if !pair.is_buildable() {
            continue;
        }
        for op in MatrixOp::ALL {
            for seed in 0..(seeds_per_op as u64) {
                case_ids.push(pair.case_id(op, seed));
            }
        }
    }

    eprintln!("=== Gate 3.1 surface-pair matrix ===");
    eprintln!("Surface feature recipes:");
    for kind in SurfaceKind::ALL {
        match kind.feature_recipe() {
            Some(recipe) => eprintln!("  {:<10} <- {recipe}", kind.label()),
            None => eprintln!("  {:<10} <- {NOT_BUILDABLE_BSPLINE}", kind.label()),
        }
    }
    eprintln!(
        "Running {} buildable boolean documents ({} seeds/op), per-case budget {}s, {}",
        case_ids.len(),
        seeds_per_op,
        timeout_secs,
        if jobs <= 1 {
            "serial (in-process)".to_string()
        } else {
            format!("{jobs} parallel subprocess jobs")
        }
    );

    let (raw_outcomes, wall_secs) = run_batch_with_extra(
        &case_ids,
        jobs,
        Duration::from_secs(timeout_secs),
        |id, budget| {
            let (o, ev) = evaluate_pair_case_with_timeout(id, budget);
            let extra = ev
                .as_ref()
                .map(|v| serde_json::to_value(v).expect("serialize ExactVolumeRecord"));
            (o, extra)
        },
    );
    assert_eq!(
        raw_outcomes.len(),
        case_ids.len(),
        "driver lost a case outcome"
    );
    let outcomes: Vec<(CaseOutcome, Option<ExactVolumeRecord>)> = raw_outcomes
        .into_iter()
        .map(|(o, extra)| {
            let ev = extra.and_then(|v| serde_json::from_value::<ExactVolumeRecord>(v).ok());
            (o, ev)
        })
        .collect();

    let prov = RunProvenance::capture(jobs, timeout_secs, wall_secs);
    eprintln!("{}", prov.summary_line());

    let report = build_report(seeds_per_op, &outcomes, &prov);
    let table = format_report_table(&report);
    eprintln!("{table}");
    println!("{table}");

    for (label, cat) in [
        ("SUPPORTED_WRONG", Category::SupportedWrong),
        ("ERROR", Category::Error),
        ("TIMEOUT", Category::Timeout),
    ] {
        let matching: Vec<&(CaseOutcome, Option<ExactVolumeRecord>)> =
            outcomes.iter().filter(|(o, _)| o.category == cat).collect();
        if !matching.is_empty() {
            eprintln!("{label} cases ({}):", matching.len());
            for (o, _) in matching {
                eprintln!("  {} — {}", o.id, o.detail);
            }
        }
    }

    let out_dir = pair_matrix_out_dir();
    fs::create_dir_all(&out_dir).unwrap_or_else(|e| panic!("cannot create {out_dir:?}: {e}"));
    let report_path = out_dir.join("report.json");
    fs::write(&report_path, serde_json::to_string_pretty(&report).unwrap())
        .unwrap_or_else(|e| panic!("cannot write {report_path:?}: {e}"));
    eprintln!("report written to {report_path:?}");

    // Persist any SUPPORTED_WRONG cases (`<id>.waffle`, `<id>.meta.json`,
    // `<id>.verdict.json`) under `target/pair-matrix/findings/` for inspection
    // and `prospect_promote`. Reset the directory first so stale files from an
    // older run cannot linger.
    let findings_dir = out_dir.join("findings");
    if findings_dir.exists() {
        let _ = fs::remove_dir_all(&findings_dir);
    }
    let wrong_outcomes: Vec<&(CaseOutcome, Option<ExactVolumeRecord>)> = outcomes
        .iter()
        .filter(|(o, _)| o.category == Category::SupportedWrong)
        .collect();
    if !wrong_outcomes.is_empty() {
        fs::create_dir_all(&findings_dir)
            .unwrap_or_else(|e| panic!("cannot create {findings_dir:?}: {e}"));
        for (o, exact_vol) in &wrong_outcomes {
            let (pair, op, seed) = parse_case_id(&o.id).expect("valid pair-matrix case id");
            let gen = build_pair_case(pair, op, seed).expect("rebuildable wrong case");
            fs::write(
                findings_dir.join(format!("{}.waffle", o.id)),
                &gen.waffle_json,
            )
            .unwrap_or_else(|e| panic!("cannot write finding .waffle: {e}"));
            fs::write(
                findings_dir.join(format!("{}.meta.json", o.id)),
                serde_json::to_string_pretty(&gen.meta).unwrap(),
            )
            .unwrap_or_else(|e| panic!("cannot write finding .meta.json: {e}"));
            let verdict = serde_json::json!({
                "id": o.id,
                "commit": report.commit,
                "seeds_per_op": report.seeds_per_op,
                "pair": pair.label(),
                "op": op.slug(),
                "seed": seed,
                "features": pair.feature_sources_summary(),
                "category": o.category.label(),
                "exact_volume": exact_vol,
                "detail": o.detail,
                "run": prov.to_json(None),
            });
            fs::write(
                findings_dir.join(format!("{}.verdict.json", o.id)),
                serde_json::to_string_pretty(&verdict).unwrap(),
            )
            .unwrap_or_else(|e| panic!("cannot write finding .verdict.json: {e}"));
        }
        eprintln!(
            "WARNING: {} SUPPORTED_WRONG finding(s) saved under {findings_dir:?}",
            wrong_outcomes.len()
        );
    } else {
        eprintln!("SUPPORTED_WRONG findings: 0");
    }
}
