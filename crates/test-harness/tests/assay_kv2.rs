//! PR-KV4 — categorized assay replay through `KernelV2Adapter` (kernel-v2).
//!
//! Replays the 190-case assay corpus (`app/tests/cases/assay`) through the
//! real feature-engine dispatch path with kernel-v2 behind the legacy
//! `Kernel` trait, and categorizes EVERY case:
//!
//! - `SUPPORTED_CORRECT` — replay succeeded and validation passed (kernel-v2
//!   `validate_solid` runs inside every constructor/boolean/tessellate call;
//!   here we additionally require a non-empty mesh and the legacy replay's
//!   mesh oracles: watertight, consistent/outward normals, no degenerate
//!   triangles, valid indices/face ranges, positive signed volume, no
//!   self-intersection, Euler characteristic, volume magnitude, minimum
//!   triangle count, bbox extent).
//! - `SUPPORTED_WRONG` — the case replayed (no NotSupported boundary hit)
//!   but validation failed. These are REAL kernel-v2/yang-rs/adapter bugs.
//! - `UNSUPPORTED(reason)` — the replay hit a loud `KernelError::NotSupported`
//!   boundary: revolve / curved profile (circle) / coplanar boolean (Yang
//!   Stage 0, roadmap M8) / fillet-chamfer-shell / other.
//! - `ERROR` — an unexpected failure (anything that is neither clean success
//!   nor a declared NotSupported boundary).
//!
//! This is the NEW kernel's categorized score — there is deliberately no
//! yang_comparison-style legacy scoring here.
//!
//! Tests:
//! - `smoke_subset_supported_correct` (always on) — hand-picked planar cases
//!   that must be SUPPORTED_CORRECT; the regression gate.
//! - `full_corpus_categorized` (`#[ignore]`) — the full 190-case run; prints
//!   the category table and writes `target/assay_kv2_report.json`. Run with:
//!   `cargo test -p test-harness --test assay_kv2 -- --ignored --nocapture`

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use test_harness::assay::gen::AssayMeta;
use test_harness::assay::randomized_runner::{discover_cases, DiscoveredCase};
use test_harness::helpers::mesh_bounding_box;
use test_harness::oracle;
use test_harness::ModelBuilder;

// ── Categories ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum UnsupportedReason {
    Revolve,
    CurvedProfile,
    CoplanarBoolean,
    FilletChamferShell,
    Other,
}

impl UnsupportedReason {
    fn label(self) -> &'static str {
        match self {
            Self::Revolve => "revolve",
            Self::CurvedProfile => "curved-profile",
            Self::CoplanarBoolean => "coplanar-boolean",
            Self::FilletChamferShell => "fillet-chamfer-shell",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Category {
    SupportedCorrect,
    SupportedWrong,
    Unsupported(UnsupportedReason),
    Error,
}

impl Category {
    fn label(&self) -> String {
        match self {
            Self::SupportedCorrect => "SUPPORTED_CORRECT".to_string(),
            Self::SupportedWrong => "SUPPORTED_WRONG".to_string(),
            Self::Unsupported(r) => format!("UNSUPPORTED({})", r.label()),
            Self::Error => "ERROR".to_string(),
        }
    }
}

struct CaseOutcome {
    id: String,
    category: Category,
    detail: String,
}

// ── Replay ─────────────────────────────────────────────────────────────────

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

/// Classify a `NotSupported` message (engine error or auto-union warning
/// text) into the adapter's declared unsupported boundaries.
fn unsupported_reason(msg: &str) -> UnsupportedReason {
    let m = msg.to_lowercase();
    if m.contains("revolve") {
        UnsupportedReason::Revolve
    } else if m.contains("circle") || m.contains("curved") || m.contains("arc") {
        UnsupportedReason::CurvedProfile
    } else if m.contains("coplanar") {
        UnsupportedReason::CoplanarBoolean
    } else if m.contains("fillet") || m.contains("chamfer") || m.contains("shell") {
        UnsupportedReason::FilletChamferShell
    } else {
        UnsupportedReason::Other
    }
}

const NOT_SUPPORTED_MARKER: &str = "operation not supported:";

/// Replay one corpus case through feature-engine + `KernelV2Adapter` and
/// categorize the outcome. Mirrors the legacy randomized runner's replay
/// shape (load → engine errors → tessellate last → mesh oracles) but with
/// the NotSupported-boundary categorization in front.
fn replay_case(case: &DiscoveredCase) -> CaseOutcome {
    let err_outcome = |detail: String| CaseOutcome {
        id: case.id.clone(),
        category: Category::Error,
        detail,
    };

    let waffle_json = match fs::read_to_string(&case.waffle_path) {
        Ok(s) => s,
        Err(e) => return err_outcome(format!("cannot read .waffle: {e}")),
    };
    let meta: AssayMeta = match fs::read_to_string(&case.meta_path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
    {
        Ok(m) => m,
        Err(e) => return err_outcome(format!("cannot read .meta.json: {e}")),
    };

    let mut builder = ModelBuilder::kernel_v2();
    if let Err(e) = builder.load(&waffle_json) {
        return err_outcome(format!("LoadProject failed: {e}"));
    }

    let engine_errors: Vec<String> = builder
        .engine_errors()
        .iter()
        .map(|(id, msg)| format!("{id}: {msg}"))
        .collect();
    let warnings: Vec<String> = builder.engine_warnings().to_vec();

    // 1. NotSupported boundary? Check engine errors first (rebuild failures),
    //    then warnings (the merge=true auto-union path downgrades a boolean
    //    error to an "Auto-union failed: …" warning).
    let not_supported_msgs: Vec<&String> = engine_errors
        .iter()
        .chain(warnings.iter())
        .filter(|m| m.contains(NOT_SUPPORTED_MARKER))
        .collect();
    if let Some(first) = not_supported_msgs.first() {
        return CaseOutcome {
            id: case.id.clone(),
            category: Category::Unsupported(unsupported_reason(first)),
            detail: format!(
                "{} NotSupported boundary(ies); first: {}",
                not_supported_msgs.len(),
                first
            ),
        };
    }

    // 2. Cases whose meta EXPECTS a rebuild error (legacy: disjoint-operand
    //    unions). If kernel-v2 also errors (for a non-NotSupported reason),
    //    that is the expected behavior; if it succeeds, fall through to
    //    normal mesh validation — succeeding with a valid (multi-shell)
    //    result is not wrong for the new kernel.
    if meta.oracles.expect_rebuild_error && !engine_errors.is_empty() {
        return CaseOutcome {
            id: case.id.clone(),
            category: Category::SupportedCorrect,
            detail: format!("expected rebuild error: {}", engine_errors.join("; ")),
        };
    }

    // 3. Any other engine error is an unexpected failure.
    if !engine_errors.is_empty() {
        return err_outcome(format!(
            "{} engine error(s): {}",
            engine_errors.len(),
            engine_errors.join("; ")
        ));
    }

    // 4. Tessellate the last solid (scale-adaptive tolerance like the legacy
    //    runner; the adapter's planar tessellation is exact and ignores it).
    let tess_tol = (meta.scale * 0.01).clamp(1e-9, 0.1);
    let mesh = match builder.tessellate_last_with_tol(tess_tol) {
        Ok(m) => m,
        Err(e) => return err_outcome(format!("no solid / tessellation failed: {e}")),
    };

    // 5. Validation: the legacy replay's mesh oracles + meta expectations.
    let mut failures: Vec<String> = Vec::new();
    for v in oracle::run_all_mesh_checks(&mesh) {
        if !v.passed {
            failures.push(format!("{}: {}", v.oracle_name, v.detail));
        }
    }
    if mesh.indices.is_empty() {
        failures.push("empty mesh: no triangles".to_string());
    }
    {
        let ops: Vec<(String, String)> = meta
            .operations
            .iter()
            .map(|o| (o.kind.clone(), o.profile_type.clone()))
            .collect();
        let v = oracle::check_minimum_triangle_count(&mesh, &ops);
        if !v.passed {
            failures.push(format!("minimum_triangle_count: {}", v.detail));
        }
    }
    if !mesh.vertices.is_empty() {
        let v = oracle::check_volume_magnitude(&mesh, meta.scale);
        if !v.passed {
            failures.push(format!("volume_magnitude: {}", v.detail));
        }
        let v = oracle::check_mesh_euler_characteristic(&mesh, meta.oracles.euler_target);
        if !v.passed {
            failures.push(format!("mesh_euler_characteristic: {}", v.detail));
        }
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        let dx = (bb_max[0] - bb_min[0]) as f64;
        let dy = (bb_max[1] - bb_min[1]) as f64;
        let dz = (bb_max[2] - bb_min[2]) as f64;
        let diagonal = (dx * dx + dy * dy + dz * dz).sqrt();
        if diagonal > meta.oracles.max_bbox_extent {
            failures.push(format!(
                "bbox diagonal {:.3e} exceeds max {:.3e}",
                diagonal, meta.oracles.max_bbox_extent
            ));
        }
    }
    // Multi-op cases must end as a single merged body (legacy runner check).
    if meta.operations.len() > 1 {
        let solid_count = builder.distinct_solid_count();
        if solid_count > 1 {
            failures.push(format!(
                "merge incomplete: {} operations produced {} separate solids",
                meta.operations.len(),
                solid_count
            ));
        }
    }

    if failures.is_empty() {
        CaseOutcome {
            id: case.id.clone(),
            category: Category::SupportedCorrect,
            detail: "all checks passed".to_string(),
        }
    } else {
        CaseOutcome {
            id: case.id.clone(),
            category: Category::SupportedWrong,
            detail: failures.join("; "),
        }
    }
}

/// Replay with a hang guard (booleans go through the yang-rs/cherchi-rs
/// pipeline; a hung case must not wedge the whole run).
fn replay_case_with_timeout(case: &DiscoveredCase, timeout: Duration) -> CaseOutcome {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = case.id.clone();
    let c = DiscoveredCase {
        id: case.id.clone(),
        waffle_path: case.waffle_path.clone(),
        meta_path: case.meta_path.clone(),
    };
    let handle = std::thread::spawn(move || {
        let _ = tx.send(replay_case(&c));
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => {
            let _ = handle.join();
            r
        }
        Err(_) => CaseOutcome {
            id,
            category: Category::Error,
            detail: format!("timeout after {}s", timeout.as_secs()),
        },
    }
}

// ── Smoke subset (always-on regression gate) ───────────────────────────────

/// Hand-picked planar cases that MUST be SUPPORTED_CORRECT: simple boxes,
/// multi-extrude unions, and boolean subtracts that stay inside kernel-v2's
/// Phase-4a boundary (polygon profiles, non-coplanar booleans).
const SMOKE_CASES: &[&str] = &[
    "F0001", "F0002", "F0003", "F0004", "F0005", "F0016", "F0020", "F0021", "F0022", "F0066",
];

#[test]
fn smoke_subset_supported_correct() {
    let dir = assay_dir();
    let cases = discover_cases(&dir);
    assert!(!cases.is_empty(), "assay corpus not found at {dir:?}");

    let mut failures = Vec::new();
    for &id in SMOKE_CASES {
        let case = cases
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("smoke case {id} not in corpus"));
        let outcome = replay_case_with_timeout(case, Duration::from_secs(120));
        eprintln!(
            "  smoke {id}: {} — {}",
            outcome.category.label(),
            outcome.detail
        );
        if outcome.category != Category::SupportedCorrect {
            failures.push(format!(
                "{id}: {} — {}",
                outcome.category.label(),
                outcome.detail
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "smoke subset cases not SUPPORTED_CORRECT:\n{}",
        failures.join("\n")
    );
}

// ── Full corpus run (manual / driver) ──────────────────────────────────────

#[test]
#[ignore] // full 190-case corpus; run with --ignored --nocapture
fn full_corpus_categorized() {
    let dir = assay_dir();
    let cases = discover_cases(&dir);
    assert_eq!(cases.len(), 190, "expected the 190-case assay corpus");

    let mut outcomes = Vec::with_capacity(cases.len());
    for (i, case) in cases.iter().enumerate() {
        eprint!("  [{}/{}] {} ... ", i + 1, cases.len(), case.id);
        let o = replay_case_with_timeout(case, Duration::from_secs(90));
        eprintln!("{}", o.category.label());
        outcomes.push(o);
    }

    // ---- summary table ----------------------------------------------------
    let count = |pred: &dyn Fn(&Category) -> bool| -> usize {
        outcomes.iter().filter(|o| pred(&o.category)).count()
    };
    let mut table = String::new();
    writeln!(table, "\nASSAY KV2 — kernel-v2 categorized corpus score").unwrap();
    writeln!(table, "  total                {:>4}", outcomes.len()).unwrap();
    writeln!(
        table,
        "  SUPPORTED_CORRECT    {:>4}",
        count(&|c| *c == Category::SupportedCorrect)
    )
    .unwrap();
    writeln!(
        table,
        "  SUPPORTED_WRONG      {:>4}",
        count(&|c| *c == Category::SupportedWrong)
    )
    .unwrap();
    for reason in [
        UnsupportedReason::Revolve,
        UnsupportedReason::CurvedProfile,
        UnsupportedReason::CoplanarBoolean,
        UnsupportedReason::FilletChamferShell,
        UnsupportedReason::Other,
    ] {
        writeln!(
            table,
            "  UNSUPPORTED({:<20}) {:>4}",
            reason.label(),
            count(&|c| *c == Category::Unsupported(reason))
        )
        .unwrap();
    }
    writeln!(
        table,
        "  ERROR                {:>4}",
        count(&|c| *c == Category::Error)
    )
    .unwrap();

    for (label, cat) in [
        ("SUPPORTED_WRONG", Category::SupportedWrong),
        ("ERROR", Category::Error),
    ] {
        let ids: Vec<&str> = outcomes
            .iter()
            .filter(|o| o.category == cat)
            .map(|o| o.id.as_str())
            .collect();
        if !ids.is_empty() {
            writeln!(table, "\n{label} cases ({}):", ids.len()).unwrap();
            for o in outcomes.iter().filter(|o| o.category == cat) {
                writeln!(table, "  {} — {}", o.id, o.detail).unwrap();
            }
        }
    }
    eprintln!("{table}");

    // ---- JSON report --------------------------------------------------------
    let report = serde_json::json!({
        "corpus": "app/tests/cases/assay",
        "kernel": "kernel-v2 (KernelV2Adapter)",
        "total": outcomes.len(),
        "supported_correct": count(&|c| *c == Category::SupportedCorrect),
        "supported_wrong": count(&|c| *c == Category::SupportedWrong),
        "unsupported": {
            "revolve": count(&|c| *c == Category::Unsupported(UnsupportedReason::Revolve)),
            "curved_profile": count(&|c| *c == Category::Unsupported(UnsupportedReason::CurvedProfile)),
            "coplanar_boolean": count(&|c| *c == Category::Unsupported(UnsupportedReason::CoplanarBoolean)),
            "fillet_chamfer_shell": count(&|c| *c == Category::Unsupported(UnsupportedReason::FilletChamferShell)),
            "other": count(&|c| *c == Category::Unsupported(UnsupportedReason::Other)),
        },
        "error": count(&|c| *c == Category::Error),
        "cases": outcomes.iter().map(|o| serde_json::json!({
            "id": o.id,
            "category": o.category.label(),
            "detail": o.detail,
        })).collect::<Vec<_>>(),
    });
    let report_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/assay_kv2_report.json");
    fs::write(&report_path, serde_json::to_string_pretty(&report).unwrap())
        .unwrap_or_else(|e| panic!("cannot write {report_path:?}: {e}"));
    eprintln!("report written to {report_path:?}");
}
