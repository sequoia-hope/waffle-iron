//! Kernel 1.0 Gate 3.7 — the degenerate-contact matrix (`assay_contact_matrix`).
//!
//! Crosses the surface pairs of `{plane, cylinder, cone, sphere, torus,
//! B-spline}` with the six contact classes (coincident surfaces, tangency
//! along a curve, tangency at a point, an edge lying in a face, a vertex lying
//! on a face, a shared edge) and runs `union`, `cut` and `intersect` on every
//! authored configuration through the same `categorize` oracle pipeline and
//! CPU-budgeted subprocess runner as `tests/assay_kv2.rs` and
//! `tests/assay_pair_matrix.rs`.
//!
//! Always-on unit tests check the cell table, that every case is the exact
//! contact it claims (no kernel call), document determinism, that every
//! operand primitive builds its intended surface in `kernel-v2`, and the
//! report model.
//!
//! Full matrix measurement (`#[ignore]`):
//!   `ASSAY_JOBS=6 cargo test --release -p test-harness --test assay_contact_matrix contact_matrix -- --ignored --exact --nocapture`
//!
//! Single case diagnosis / subprocess worker (`#[ignore]`):
//!   `ASSAY_CASE=cm_05_cylinder_cylinder_tangentcurve_kiss_union cargo test --release -p test-harness --test assay_contact_matrix single_case -- --ignored --exact --nocapture`

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;
use test_harness::assay::categorize::{
    categorize_with_exact_volume, CaseOutcome, Category, UnsupportedReason,
};
use test_harness::assay::contact_matrix::{
    admission, all_contact_cases, build_contact_case, build_report, build_report_at_commit,
    classify, find_case, finding_signature, format_report_table, refusal_names_class,
    verify_contact_geometry, Admission, Arrangement, CellVerdict, ContactClass, ContactOp, Prim,
};
use test_harness::assay::cpu_runner::{
    default_assay_jobs, emit_assay_outcome_with_extra, run_batch_with_extra,
    run_with_cpu_timeout_map, RunProvenance,
};
use test_harness::assay::pair_matrix::{ExactVolumeRecord, SurfaceKind, ALL_SURFACE_PAIRS};
use test_harness::ModelBuilder;

/// Default per-case CPU budget, the corpus run's (`ASSAY_CASE_TIMEOUT_SECS=900`).
const DEFAULT_CASE_TIMEOUT_SECS: u64 = 900;

fn case_timeout_secs() -> u64 {
    std::env::var("ASSAY_CASE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CASE_TIMEOUT_SECS)
}

fn contact_matrix_out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/contact-matrix")
}

fn evaluate_contact_case(id: &str) -> (CaseOutcome, Option<ExactVolumeRecord>) {
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
    let Some((case, op)) = find_case(id) else {
        return err_outcome(format!("invalid contact-matrix case id: {id}"));
    };
    let generated = match build_contact_case(&case, op) {
        Ok(g) => g,
        Err(e) => return err_outcome(format!("generator failed: {e}")),
    };
    let (mut outcome, exact_vol) =
        categorize_with_exact_volume(&generated.id, &generated.waffle_json, &generated.meta);
    // An engine error names its feature by UUID; name it by role instead
    // (`bool_ab` is the contact boolean, `trim_a` / `trim_b` a setup step).
    outcome.detail = generated.with_roles(&outcome.detail);
    (
        outcome,
        exact_vol.as_ref().map(ExactVolumeRecord::from_verdict),
    )
}

fn evaluate_contact_case_with_timeout(
    id: &str,
    budget: Duration,
) -> (CaseOutcome, Option<ExactVolumeRecord>) {
    let owned = id.to_string();
    run_with_cpu_timeout_map(
        id,
        budget,
        move || evaluate_contact_case(&owned),
        |o| (o, None),
    )
}

// ── Always-on: the cell table ───────────────────────────────────────────────

#[test]
fn every_cell_is_authored_or_carries_a_reason() {
    let cases = all_contact_cases();
    let mut by_cell: BTreeMap<(usize, ContactClass), Vec<&str>> = BTreeMap::new();
    let mut keys = BTreeSet::new();
    for c in &cases {
        assert!(
            keys.insert(c.key()),
            "duplicate contact case key {}",
            c.key()
        );
        assert!(
            !c.variant.is_empty()
                && c.variant
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit()),
            "{}: a variant name is lowercase letters and digits",
            c.key()
        );
        assert!(c.what.len() > 40, "{}: say what the contact is", c.key());
        by_cell
            .entry((c.pair.index, c.class))
            .or_default()
            .push(c.variant);
        for op in c.ops() {
            let id = c.case_id(op);
            let (found, found_op) =
                find_case(&id).unwrap_or_else(|| panic!("{id} does not parse back"));
            assert_eq!(found.key(), c.key());
            assert_eq!(found_op, op);
        }
        assert_eq!(
            c.ops().contains(&ContactOp::SetupA),
            c.a.trim.is_some(),
            "{}",
            c.key()
        );
        assert_eq!(
            c.ops().contains(&ContactOp::SetupB),
            c.b.trim.is_some(),
            "{}",
            c.key()
        );
    }
    assert!(find_case("cm_00_plane_plane_coincident_overlap_setupa").is_none());
    assert!(find_case("cm_00_plane_plane_tangentcurve_kiss_union").is_none());

    let (mut admitted, mut not_admitted, mut not_buildable) = (0, 0, 0);
    for pair in ALL_SURFACE_PAIRS {
        for class in ContactClass::ALL {
            let authored = by_cell.get(&(pair.index, class));
            match admission(pair, class) {
                Admission::Admitted => {
                    admitted += 1;
                    assert!(
                        authored.is_some_and(|v| !v.is_empty()),
                        "{} admits {} but no case is authored",
                        pair.label(),
                        class.label()
                    );
                }
                Admission::NotAdmitted(reason) => {
                    not_admitted += 1;
                    assert!(reason.len() > 20, "a not-admitted cell states why");
                    assert!(
                        authored.is_none(),
                        "{} does not admit {} yet a case is authored",
                        pair.label(),
                        class.label()
                    );
                }
                Admission::NotBuildable => {
                    not_buildable += 1;
                    assert_eq!(pair.b, SurfaceKind::BSpline);
                    assert!(authored.is_none());
                }
            }
        }
    }
    // 15 analytic pairs × 6 classes = 90 cells: coincidence only for the 5
    // pairs of equal kinds (10 not admitted), tangency along a curve not for
    // plane×plane, plane×sphere, sphere×sphere (3), tangency at a point not
    // for plane×plane, plane×cylinder, plane×cone (3).
    assert_eq!((admitted, not_admitted, not_buildable), (74, 16, 36));
    for class in ContactClass::ALL {
        assert_eq!(ContactClass::from_slug(class.slug()), Some(class));
        assert!(!class.slug().contains('_'));
    }
}

// ── Always-on: every case is the exact contact it claims ────────────────────

#[test]
fn every_case_is_the_contact_it_claims() {
    let mut failures = Vec::new();
    for c in all_contact_cases() {
        if let Err(e) = verify_contact_geometry(&c) {
            failures.push(e);
        }
    }
    assert!(
        failures.is_empty(),
        "{} case(s) are not the contact they claim:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn generated_documents_are_deterministic() {
    for c in all_contact_cases() {
        for op in c.ops() {
            let g1 = build_contact_case(&c, op).expect("build case 1");
            let g2 = build_contact_case(&c, op).expect("build case 2");
            assert_eq!(
                g1.waffle_json, g2.waffle_json,
                "non-deterministic .waffle output for {}",
                g1.id
            );
            assert!(g1.meta.oracles.derived_meta);
            assert!(!g1.meta.oracles.expect_rebuild_error);
            // The one authored expectation: a result empty by construction.
            let empty = matches!(
                (c.arrangement, op),
                (Arrangement::Kiss, ContactOp::Intersect)
                    | (Arrangement::Identical, ContactOp::Cut)
            );
            assert_eq!(c.expects_empty(op), empty, "{}", g1.id);
            assert_eq!(
                g1.meta.oracles.expected_solid_count,
                empty.then_some(0),
                "{}",
                g1.id
            );
            let doc: Value = serde_json::from_str(&g1.waffle_json).unwrap();
            let booleans = doc
                .pointer("/tabs/0/kind/features/features")
                .and_then(Value::as_array)
                .map_or(0, |fs| {
                    fs.iter()
                        .filter(|f| {
                            f.pointer("/operation/type").and_then(Value::as_str)
                                == Some("BooleanCombine")
                        })
                        .count()
                });
            let trims = usize::from(c.a.trim.is_some()) + usize::from(c.b.trim.is_some());
            let expected = if op.is_setup() { 1 } else { trims + 1 };
            assert_eq!(booleans, expected, "{}: boolean feature count", g1.id);
            // The last feature of a boolean document is the contact boolean.
            let last_role = &g1.feature_roles.last().unwrap().1;
            if op.is_setup() {
                assert!(last_role.starts_with("trim_"), "{}: {last_role}", g1.id);
            } else {
                assert_eq!(last_role, "bool_ab", "{}", g1.id);
            }
            let named = g1.with_roles(&format!("{}: boom", g1.feature_roles.last().unwrap().0));
            assert_eq!(named, format!("{last_role}: boom"));
        }
    }
}

/// Every primitive a case is authored from builds in `kernel-v2`, is
/// watertight, and each operand carries a face of its pair's surface kind (on
/// its base primitive or, for a trimmed operand, on its tool — the wall of a
/// drilled hole is the drill's cylinder).
#[test]
fn every_operand_primitive_builds_its_surface_in_kernel_v2() {
    let mut built: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
    let mut surface_types = |prim: &Prim| -> Vec<Option<String>> {
        let key = format!("{prim:?}");
        if let Some(found) = built.get(&key) {
            return found.clone();
        }
        let mut builder = ModelBuilder::kernel_v2();
        prim.add_to_builder(&mut builder, "sk", "body")
            .unwrap_or_else(|e| panic!("primitive does not build: {e}\n{key}"));
        builder
            .assert_no_errors()
            .unwrap_or_else(|e| panic!("primitive raised an engine error: {e}\n{key}"));
        let mesh = builder.tessellate("body").unwrap();
        let wt = test_harness::oracle::check_watertight_mesh(&mesh);
        assert!(wt.passed, "primitive not watertight: {}\n{key}", wt.detail);
        assert!(
            test_harness::helpers::mesh_signed_volume(&mesh) > 1e-3,
            "primitive has no volume\n{key}"
        );
        let types: Vec<Option<String>> = builder
            .face_signatures("body")
            .unwrap()
            .into_iter()
            .map(|(_, sig)| sig.surface_type)
            .collect();
        built.insert(key, types.clone());
        types
    };
    for c in all_contact_cases() {
        for (operand, kind) in [(&c.a, c.pair.a), (&c.b, c.pair.b)] {
            let expected = kind.introspect_surface_type().expect("analytic kind");
            let mut types = surface_types(&operand.base);
            if let Some((_, tool)) = &operand.trim {
                types.extend(surface_types(tool));
            }
            assert!(
                types.iter().any(|t| t.as_deref() == Some(expected)),
                "{}: no {expected} face on the operand of kind {}; got {types:?}",
                c.key(),
                kind.label()
            );
        }
    }
}

// ── Always-on: the report model ─────────────────────────────────────────────

#[test]
fn a_refusal_is_read_against_its_contact_class() {
    let coplanar = "bool_ab: operation error: kernel error: operation not supported: \
                    boolean_union: coplanar input face pair (Yang Stage 0 coplanar preprocessing)";
    assert!(refusal_names_class(coplanar, ContactClass::Coincident));
    assert!(!refusal_names_class(coplanar, ContactClass::TangentCurve));
    assert!(!refusal_names_class(coplanar, ContactClass::EdgeInFace));
    // A bare "tangent" names neither tangency class.
    assert!(!refusal_names_class(
        "tangent contact",
        ContactClass::TangentCurve
    ));
    assert!(refusal_names_class(
        "surfaces are tangent along a line",
        ContactClass::TangentCurve
    ));
    assert!(refusal_names_class(
        "isolated Tangent Point between faces 3 and 7",
        ContactClass::TangentPoint
    ));
    assert!(refusal_names_class(
        "an edge of A lies in a face of B",
        ContactClass::EdgeInFace
    ));
    assert!(refusal_names_class(
        "vertex-on-face contact",
        ContactClass::VertexOnFace
    ));
    assert!(refusal_names_class(
        "shared edge between operands",
        ContactClass::SharedEdge
    ));
    let stop = "bool_ab: kernel error: boolean failed: RefinementExhausted { sites: 4 }";
    for class in ContactClass::ALL {
        assert!(!refusal_names_class(stop, class));
    }

    assert_eq!(
        classify(&Category::SupportedCorrect, "", ContactClass::SharedEdge),
        CellVerdict::Correct
    );
    assert_eq!(
        classify(
            &Category::SupportedWrong,
            coplanar,
            ContactClass::Coincident
        ),
        CellVerdict::Wrong
    );
    assert_eq!(
        classify(&Category::Timeout, "", ContactClass::Coincident),
        CellVerdict::Timeout
    );
    assert_eq!(
        classify(
            &Category::Unsupported(UnsupportedReason::CoplanarBoolean),
            coplanar,
            ContactClass::Coincident
        ),
        CellVerdict::RefusedNamed
    );
    assert_eq!(
        classify(
            &Category::Unsupported(UnsupportedReason::CoplanarBoolean),
            coplanar,
            ContactClass::TangentPoint
        ),
        CellVerdict::UnsupportedUnnamed
    );
    assert_eq!(
        classify(&Category::Error, stop, ContactClass::TangentCurve),
        CellVerdict::ErrorUnnamed
    );
    assert!(!CellVerdict::Correct.is_finding());
    assert!(!CellVerdict::RefusedNamed.is_finding());
    for v in [
        CellVerdict::Wrong,
        CellVerdict::Timeout,
        CellVerdict::ErrorUnnamed,
        CellVerdict::UnsupportedUnnamed,
        CellVerdict::Blocked,
    ] {
        assert!(v.is_finding());
    }
}

#[test]
fn the_report_separates_met_cells_from_findings() {
    let outcome = |id: &str, category: Category, detail: &str| {
        let exact_volume = (category == Category::SupportedCorrect && id.ends_with("_cut"))
            .then_some(ExactVolumeRecord::Agree {
                rel: 1e-3,
                band: 3e-2,
            });
        (
            CaseOutcome {
                id: id.to_string(),
                category,
                detail: detail.to_string(),
            },
            exact_volume,
        )
    };
    let coplanar = "bool_ab: operation not supported: boolean_union: coplanar input face pair";
    let outcomes = vec![
        // plane × plane coincident (overlap): CORRECT, named refusal, CORRECT → MET.
        outcome(
            "cm_00_plane_plane_coincident_overlap_union",
            Category::Unsupported(UnsupportedReason::CoplanarBoolean),
            coplanar,
        ),
        outcome(
            "cm_00_plane_plane_coincident_overlap_cut",
            Category::SupportedCorrect,
            "all checks passed",
        ),
        outcome(
            "cm_00_plane_plane_coincident_overlap_intersect",
            Category::SupportedCorrect,
            "all checks passed",
        ),
        // plane × cylinder tangency along a curve (kiss): WRONG, unnamed, TIMEOUT.
        outcome(
            "cm_01_plane_cylinder_tangentcurve_kiss_union",
            Category::SupportedWrong,
            "watertight_mesh: 4 boundary edges",
        ),
        outcome(
            "cm_01_plane_cylinder_tangentcurve_kiss_cut",
            Category::Error,
            "1 engine error(s): bool_ab: kernel error: boolean failed: LocalRefinementRequired",
        ),
        outcome(
            "cm_01_plane_cylinder_tangentcurve_kiss_intersect",
            Category::Timeout,
            "timeout after 900s CPU (subprocess killed)",
        ),
        // sphere × sphere edge-in-face: the setup is not CORRECT → BLOCKED.
        outcome(
            "cm_12_sphere_sphere_edgeinface_through_setupa",
            Category::Error,
            "1 engine error(s): trim_a: kernel error: boolean failed",
        ),
        outcome(
            "cm_12_sphere_sphere_edgeinface_through_union",
            Category::Error,
            "1 engine error(s): trim_a: kernel error: boolean failed",
        ),
    ];
    let prov = RunProvenance::capture(1, 900, 1.0);
    let report = build_report_at_commit("abc12345".to_string(), &outcomes, &prov);
    assert_eq!(report.commit, "abc12345");
    assert_eq!(report.total_cells, 126);
    assert_eq!(report.admitted_cells, 74);
    assert_eq!(report.not_admitted_cells, 16);
    assert_eq!(report.not_buildable_cells, 36);
    assert_eq!(report.boolean_runs, 7);
    assert_eq!(report.setup_runs, 1);
    assert_eq!(report.setup_runs_correct, 0);
    assert_eq!(report.totals.correct, 2);
    assert_eq!(report.totals.refused_named, 1);
    assert_eq!(report.totals.wrong, 1);
    assert_eq!(report.totals.error_unnamed, 1);
    assert_eq!(report.totals.unsupported_unnamed, 0);
    assert_eq!(report.totals.timeout, 1);
    assert_eq!(report.totals.blocked, 1);
    assert_eq!(report.totals.findings(), 4);
    assert_eq!(report.totals.correct_volume_agree, 1);
    assert_eq!(report.totals.correct_volume_not_covered, 0);
    assert_eq!(report.totals.correct_volume_not_reached, 1);
    assert_eq!(report.cells_met, 1);
    assert_eq!(report.cells_with_findings, 2);
    assert_eq!(report.cells_untested, 71);
    // Findings are listed WRONG first, then TIMEOUT, then unnamed, then blocked.
    assert_eq!(
        report.findings,
        vec![
            "cm_01_plane_cylinder_tangentcurve_kiss_union",
            "cm_01_plane_cylinder_tangentcurve_kiss_intersect",
            "cm_01_plane_cylinder_tangentcurve_kiss_cut",
            "cm_12_sphere_sphere_edgeinface_through_union",
        ]
    );
    let cell = |pair_index: usize, class: ContactClass| {
        report
            .cells
            .iter()
            .find(|c| c.pair_index == pair_index && c.class == class.label())
            .unwrap()
    };
    let met = cell(0, ContactClass::Coincident);
    assert_eq!(met.status, "MET");
    assert_eq!(
        (met.union, met.cut, met.intersect),
        (Some('R'), Some('C'), Some('C'))
    );
    assert_eq!(met.variants, vec!["overlap", "abut"]);
    let finding = cell(1, ContactClass::TangentCurve);
    assert_eq!(finding.status, "FINDING");
    assert_eq!(
        (finding.union, finding.cut, finding.intersect),
        (Some('W'), Some('E'), Some('T'))
    );
    let blocked = cell(12, ContactClass::EdgeInFace);
    assert_eq!(blocked.status, "FINDING");
    assert_eq!(blocked.union, Some('B'));
    assert_eq!(cell(0, ContactClass::TangentCurve).status, "NOT_ADMITTED");
    assert_eq!(cell(20, ContactClass::Coincident).status, "NOT_BUILDABLE");
    assert_eq!(cell(14, ContactClass::SharedEdge).status, "UNTESTED");

    let table = format_report_table(&report);
    assert!(table.contains("GATE 3.7 DEGENERATE-CONTACT MATRIX (commit abc12345)"));
    assert!(table.contains("Cells: 126 total = 74 admitted + 16 not admitted + 36 not buildable"));
    assert!(table.contains("Admitted cells: 1 MET"));
    assert!(table.contains("2 with a FINDING / 71 UNTESTED"));
    assert!(table.contains(
        "Boolean runs: 7 = 2 CORRECT / 1 REFUSED_NAMED / 1 WRONG / 1 TIMEOUT / 1 ERROR_UNNAMED / 0 UNSUPPORTED_UNNAMED / 1 BLOCKED"
    ));
    assert!(table.contains("a plane tangent to a sphere touches it at exactly one point"));
    assert!(table.contains("[WRONG] cm_01_plane_cylinder_tangentcurve_kiss_union"));
    assert!(table.contains(
        "repro: ASSAY_CASE=cm_01_plane_cylinder_tangentcurve_kiss_union cargo test --release"
    ));
    assert!(table.contains(
        "Exact-volume reading of the 2 CORRECT boolean runs: 1 Agree / 0 NotCovered / 1 not reached"
    ));
    assert!(table.contains("FINDINGS BY KIND (4 kinds; digits read as N):"));
    assert_eq!(
        finding_signature(
            "1 engine error(s): bool_ab: operation error: kernel error: boolean operation \
             failed: relocation region around vertex 126 is invalid"
        ),
        "boolean operation failed: relocation region around vertex N is invalid"
    );
    assert!(table.contains("SETUP STEPS NOT CORRECT (1)"));
    assert!(table.contains("blocked: setupa is ERROR"));
}

// ── Single-case runner (manual diagnosis + subprocess child entry) ──────────

#[test]
#[ignore] // manual / driver child: ASSAY_CASE=cm_05_cylinder_cylinder_tangentcurve_kiss_union ...
fn single_case() {
    let Ok(id) = std::env::var("ASSAY_CASE") else {
        println!("single_case: ASSAY_CASE not set — nothing to do (manual/driver runner)");
        return;
    };
    let timeout_secs = case_timeout_secs();
    let t0 = Instant::now();
    let (outcome, exact_vol) =
        evaluate_contact_case_with_timeout(&id, Duration::from_secs(timeout_secs));
    let ev_label = exact_vol
        .as_ref()
        .map_or_else(|| "not reached".to_string(), ExactVolumeRecord::table_label);
    if let Some((case, _)) = find_case(&id) {
        println!(
            "{id}: {} — {} ({})",
            case.class.label(),
            case.what,
            case.arrangement.label()
        );
        println!(
            "{id}: verdict {}",
            classify(&outcome.category, &outcome.detail, case.class).label()
        );
    }
    println!(
        "{id}: {} [exact_volume: {ev_label}] ({:.1}s) — {}",
        outcome.category.label(),
        t0.elapsed().as_secs_f64(),
        outcome.detail
    );
    if let Ok(dir) = std::env::var("CONTACT_MATRIX_DUMP") {
        if let Some((case, op)) = find_case(&id) {
            let generated = build_contact_case(&case, op).expect("rebuildable case");
            let path = PathBuf::from(dir).join(format!("{id}.waffle"));
            fs::write(&path, &generated.waffle_json)
                .unwrap_or_else(|e| panic!("cannot write {path:?}: {e}"));
            println!("{id}: document written to {path:?}");
        }
    }
    let extra = exact_vol
        .as_ref()
        .map(|ev| serde_json::to_value(ev).expect("serialize ExactVolumeRecord"));
    emit_assay_outcome_with_extra(&outcome, extra.as_ref());
}

// ── Full matrix measurement ─────────────────────────────────────────────────

/// The measurement. It prints the matrix and writes
/// `target/contact-matrix/report.json`; like `surface_pair_matrix` it does not
/// fail on a cell that is not yet met — a finding is a result to read, and the
/// gate is open (`docs/kernel_1_0.md` Gate 3.7). `CONTACT_MATRIX_STRICT=1`
/// makes it fail on any finding, for the day the gate is held.
/// `CONTACT_MATRIX_FILTER=<text>[,<text>…]` runs only the cases whose id
/// contains one of the texts and writes `report.filtered.json` instead.
#[test]
#[ignore] // full contact matrix; run with --release -- --ignored --nocapture
fn contact_matrix() {
    let timeout_secs = case_timeout_secs();
    let jobs = default_assay_jobs();
    let filter: Vec<String> = std::env::var("CONTACT_MATRIX_FILTER")
        .ok()
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let cases = all_contact_cases();
    let mut case_ids = Vec::new();
    for c in &cases {
        for op in c.ops() {
            let id = c.case_id(op);
            if filter.is_empty() || filter.iter().any(|f| id.contains(f.as_str())) {
                case_ids.push(id);
            }
        }
    }

    eprintln!("=== Gate 3.7 degenerate-contact matrix ===");
    eprintln!(
        "Running {} documents ({} authored configurations{}), per-case budget {}s, {}",
        case_ids.len(),
        cases.len(),
        if filter.is_empty() {
            String::new()
        } else {
            format!(", filter {filter:?}")
        },
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
            let (o, ev) = evaluate_contact_case_with_timeout(id, budget);
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

    let report = build_report(&outcomes, &prov);
    let table = format_report_table(&report);
    eprintln!("{table}");
    println!("{table}");

    let out_dir = contact_matrix_out_dir();
    fs::create_dir_all(&out_dir).unwrap_or_else(|e| panic!("cannot create {out_dir:?}: {e}"));
    let stem = if filter.is_empty() {
        "report"
    } else {
        "report.filtered"
    };
    let report_path = out_dir.join(format!("{stem}.json"));
    fs::write(&report_path, serde_json::to_string_pretty(&report).unwrap())
        .unwrap_or_else(|e| panic!("cannot write {report_path:?}: {e}"));
    let table_path = out_dir.join(format!("{stem}.txt"));
    fs::write(&table_path, &table).unwrap_or_else(|e| panic!("cannot write {table_path:?}: {e}"));
    eprintln!("report written to {report_path:?} and {table_path:?}");

    // Persist every finding (`<id>.waffle`, `<id>.meta.json`,
    // `<id>.verdict.json`) under `target/contact-matrix/findings/`, the setup
    // steps that are not CORRECT included. A full run resets the directory so
    // stale files from an older run cannot linger.
    let findings_dir = out_dir.join("findings");
    if filter.is_empty() && findings_dir.exists() {
        let _ = fs::remove_dir_all(&findings_dir);
    }
    let to_save: Vec<_> = report
        .cases
        .iter()
        .filter(|c| c.verdict != "CORRECT" && c.verdict != "REFUSED_NAMED")
        .collect();
    if !to_save.is_empty() {
        fs::create_dir_all(&findings_dir)
            .unwrap_or_else(|e| panic!("cannot create {findings_dir:?}: {e}"));
        for record in &to_save {
            let (case, op) = find_case(&record.id).expect("valid contact-matrix case id");
            let generated = build_contact_case(&case, op).expect("rebuildable finding");
            fs::write(
                findings_dir.join(format!("{}.waffle", record.id)),
                &generated.waffle_json,
            )
            .unwrap_or_else(|e| panic!("cannot write finding .waffle: {e}"));
            fs::write(
                findings_dir.join(format!("{}.meta.json", record.id)),
                serde_json::to_string_pretty(&generated.meta).unwrap(),
            )
            .unwrap_or_else(|e| panic!("cannot write finding .meta.json: {e}"));
            let verdict = serde_json::json!({
                "commit": report.commit,
                "record": record,
                "run": prov.to_json(None),
            });
            fs::write(
                findings_dir.join(format!("{}.verdict.json", record.id)),
                serde_json::to_string_pretty(&verdict).unwrap(),
            )
            .unwrap_or_else(|e| panic!("cannot write finding .verdict.json: {e}"));
        }
        eprintln!(
            "{} finding document(s) saved under {findings_dir:?}",
            to_save.len()
        );
    }

    if std::env::var("CONTACT_MATRIX_STRICT").is_ok_and(|v| v == "1") {
        assert!(
            report.findings.is_empty() && report.cells_untested == 0,
            "CONTACT_MATRIX_STRICT: {} finding(s), {} untested cell(s)",
            report.findings.len(),
            report.cells_untested
        );
    }
}
