//! PR-Y30 differential diff harness: per-triangle comparison of our Yang
//! pipeline's post-`face_survival_detect` output (Stage B) against the
//! Cherchi 2022 C++ reference (`mesh_booleans union`) on F0020 + cohort
//! (F0044, F0045, R0092).
//!
//! Why this exists:
//! ----------------
//! After four canary ABORTs (Y25/Y26/Y27/Y28) on F0020, internal-stage
//! diffing proved insufficient — every canary classified mechanisms but
//! none predicted whether a fix would close the watertight oracle. Per
//! CLAUDE.md: *"Reference parity is not optional. When the algorithm we're
//! porting has a public reference implementation (Cherchi 2020/2022 C++),
//! build differential testing against that reference as part of the
//! initial port."* This harness asks a bounded question — "which
//! triangles does Cherchi emit that we don't?" — rather than the
//! open-ended structural question "where in our code path do triangles
//! go missing?"
//!
//! PR-Y30 stage choice — Stage B (post-survival), NOT Stage C (post-patch-id):
//! --------------------------------------------------------------------------
//! Cherchi's `mesh_booleans union` output IS the post-survival boolean
//! result — a well-formed simplicial complex per Cherchi 2022 §3 ("when
//! exact methods are used, the arrangement is guaranteed to be a well
//! formed simplicial complex") after applying the boolean operator's
//! in/out selection (Cherchi 2022 §5 inside/outside classification). The
//! apples-to-apples comparison point on the Waffle side is therefore
//! Stage B (Yang §4.4.2 "Mesh and B-Rep Booleans" — the post-
//! `face_survival_detect` output) — NOT Stage C, which is the post-
//! flood-fill patch-id output and includes patch boundary information
//! Cherchi never emits. PR-Y29 used Stage C in error; this revision
//! re-baselines against Stage B.
//!
//! Relationship to `cherchi2022_reference_parity.rs`:
//! --------------------------------------------------
//! The pre-existing parity tests check that Cherchi's output on Waffle's
//! preprocessed A/B inputs is well-formed (a coarse upstream-vs-downstream
//! discriminator). This harness goes further: it reads BOTH Cherchi's
//! output OBJ AND Waffle's Stage-B output OBJ (the boolean result post-
//! `face_survival_detect`, written by `YANG_STAGE_DUMP` at
//! `topology_extract.rs:2569`) and computes the position-quantized
//! triangle-set difference. Future PR-Y31+ canaries consume the captured
//! baselines in `docs/audits/pr_y30_stage_b_baselines.md` as input.
//!
//! Both tests are `#[ignore]`-gated: they require the Cherchi binary
//! (`CHERCHI2022_BIN` env, default
//! `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`).
//!
//! Refs: CLAUDE.md §"Reference parity is not optional"; PR-Y30 plan at
//! `/home/claude/.claude/plans/optimized-wandering-wind.md`;
//! `cherchi2022_reference_parity.rs:71-140` for the OBJ parser pattern;
//! Yang 2025 §4.4.2 (mesh and B-Rep booleans) + Cherchi 2022 §3
//! (well-formed simplicial complex) + §5 (in/out classification) for the
//! reason Stage B is the apples-to-apples comparison point.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use kernel::diagnostics::{check_conformal, ConformalReport};
use test_harness::cherchi_sidecar::{cherchi_bin, run_with_timeout, TimedRun};

const CHERCHI_SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Position-quantization grid. We pick a coarse-but-bounded scale (1e-6 m =
/// 1 µm) so that intersection-point round-off below the kernel's working
/// tolerance (TAU_WORK = 1e-12) does not split otherwise-matching triangles
/// across the diff. This is the same scale the spec references; coarser
/// would risk merging genuinely-distinct intersection vertices on tight
/// features, finer would surface noise we don't care about.
const QUANTIZE_GRID: f64 = 1e-6;

const TOP_N_REPORT: usize = 10;

/// Minimal Wavefront OBJ parser — handles only `v x y z` and `f i j k`
/// lines, 1-indexed. Skips blank/comment/`vn`/`vt`/`vp`/`g`/`o`/`mtllib`/
/// `usemtl` lines. Duplicates the pattern from
/// `cherchi2022_reference_parity.rs:71-140` (the function there is
/// test-harness-local; not exported).
fn parse_obj(path: &Path) -> Result<(Vec<[f64; 3]>, Vec<[usize; 3]>), String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[usize; 3]> = Vec::new();
    for (line_no, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut tokens = line.split_whitespace();
        let head = match tokens.next() {
            Some(t) => t,
            None => continue,
        };
        match head {
            "v" => {
                let coords: Vec<f64> = tokens
                    .take(3)
                    .map(|t| {
                        t.parse::<f64>().map_err(|e| {
                            format!("OBJ:{}: bad vertex coord `{}`: {}", line_no + 1, t, e)
                        })
                    })
                    .collect::<Result<_, _>>()?;
                if coords.len() != 3 {
                    return Err(format!(
                        "OBJ:{}: vertex line needs 3 coords, got {}",
                        line_no + 1,
                        coords.len()
                    ));
                }
                verts.push([coords[0], coords[1], coords[2]]);
            }
            "f" => {
                let idx: Vec<usize> = tokens
                    .take(3)
                    .map(|t| {
                        let core = t.split('/').next().unwrap_or(t);
                        core.parse::<usize>()
                            .map_err(|e| {
                                format!("OBJ:{}: bad face index `{}`: {}", line_no + 1, t, e)
                            })
                            .map(|i| i.checked_sub(1).unwrap_or(0))
                    })
                    .collect::<Result<_, _>>()?;
                if idx.len() != 3 {
                    return Err(format!(
                        "OBJ:{}: only triangle faces supported (got {} verts)",
                        line_no + 1,
                        idx.len()
                    ));
                }
                tris.push([idx[0], idx[1], idx[2]]);
            }
            "vn" | "vt" | "vp" | "g" | "o" | "mtllib" | "usemtl" | "s" | "l" => continue,
            other => {
                return Err(format!(
                    "OBJ:{}: unrecognized leading token `{}`",
                    line_no + 1,
                    other
                ));
            }
        }
    }
    Ok((verts, tris))
}

fn quantize_pos(p: [f64; 3]) -> (i64, i64, i64) {
    let inv = 1.0 / QUANTIZE_GRID;
    (
        (p[0] * inv).round() as i64,
        (p[1] * inv).round() as i64,
        (p[2] * inv).round() as i64,
    )
}

/// Canonicalize a triangle by sorting its three quantized vertices —
/// winding-insensitive set membership. Two triangles with the same vertex
/// positions but opposite orientation collapse to the same key. This is
/// the standard canonical form for set-difference comparisons on
/// triangle meshes.
fn quantize_tri(verts: &[[f64; 3]], tri: [usize; 3]) -> [(i64, i64, i64); 3] {
    let mut quant = [
        quantize_pos(verts[tri[0]]),
        quantize_pos(verts[tri[1]]),
        quantize_pos(verts[tri[2]]),
    ];
    quant.sort();
    quant
}

fn fmt_report_line(report: &ConformalReport, label: &str) -> String {
    format!(
        "{} : verts={} tris={} unique_edges={} unpaired={} multi_paired={} \
         euler_chi={} well_formed={}",
        label,
        report.vertex_count,
        report.triangle_count,
        report.unique_undirected_edge_count,
        report.unpaired_directed_edges.len(),
        report.multi_paired_edges.len(),
        report.euler_characteristic,
        report.is_well_formed,
    )
}

fn fmt_qtri(t: &[(i64, i64, i64); 3]) -> String {
    let to_m = |q: (i64, i64, i64)| {
        (
            q.0 as f64 * QUANTIZE_GRID,
            q.1 as f64 * QUANTIZE_GRID,
            q.2 as f64 * QUANTIZE_GRID,
        )
    };
    let a = to_m(t[0]);
    let b = to_m(t[1]);
    let c = to_m(t[2]);
    format!(
        "qa=({:+.6e},{:+.6e},{:+.6e}) qb=({:+.6e},{:+.6e},{:+.6e}) qc=({:+.6e},{:+.6e},{:+.6e})",
        a.0, a.1, a.2, b.0, b.1, b.2, c.0, c.1, c.2,
    )
}

/// Run the Yang pipeline on `case_id` with `YANG_DUMP_OBJ_BASE` and
/// `YANG_STAGE_DUMP` armed; return paths to the dumped A.obj, B.obj, and
/// Stage-B (post-`face_survival_detect` boolean result) OBJ.
fn run_waffle_and_collect_dumps(case_id: &str) -> WaffleDumpPaths {
    use test_harness::assay::randomized_runner::run_single_case;

    let dir = Path::new(ASSAY_DIR);
    assert!(
        dir.exists(),
        "assay corpus dir not present at {}",
        dir.display()
    );

    let lower = case_id.to_ascii_lowercase();
    let workdir = std::env::temp_dir().join(format!("waffle_cherchi_diff_{}", lower));
    std::fs::create_dir_all(&workdir).expect("create temp work dir");

    // YANG_DUMP_OBJ_BASE writes <base>_a.obj + <base>_b.obj — these are
    // the preprocessed-but-pre-arrangement inputs both pipelines see.
    let base = workdir.join(&lower);
    let base_str = base.to_string_lossy().into_owned();
    let path_a = workdir.join(format!("{}_a.obj", lower));
    let path_b = workdir.join(format!("{}_b.obj", lower));

    // YANG_STAGE_DUMP=<dir> emits stage_*.obj under <dir>/<case_id>/.
    // Stage "B" is the post-`face_survival_detect` boolean result —
    // flattened `survival.groups[*].verts` over the subdivided mesh's
    // shared vertex array (written at topology_extract.rs:2569). This
    // is the apples-to-apples comparison point against Cherchi's
    // `mesh_booleans union` output (Cherchi 2022 §3 well-formed
    // simplicial complex + §5 in/out classification; Yang §4.4.2 mesh
    // and B-Rep booleans). The conformal probe must be armed
    // (YANG_CONFORMAL_PROBE=1) to reach the Stage B dump site.
    let stage_dump_dir = workdir.join("stages");
    std::fs::create_dir_all(&stage_dump_dir).expect("create stage dump dir");
    let path_stage_b = stage_dump_dir.join(case_id).join("stage_B.obj");

    // Clean any stale outputs so a partial run can't be mistaken for fresh.
    for p in [&path_a, &path_b, &path_stage_b] {
        let _ = std::fs::remove_file(p);
    }

    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("YANG_DUMP_OBJ_BASE", &base_str);
    std::env::set_var("YANG_STAGE_DUMP", stage_dump_dir.to_string_lossy().as_ref());
    std::env::set_var("YANG_CONFORMAL_PROBE", "1");
    let case = run_single_case(dir, case_id, true);
    std::env::remove_var("YANG_DUMP_OBJ_BASE");
    std::env::remove_var("YANG_STAGE_DUMP");
    std::env::remove_var("YANG_CONFORMAL_PROBE");
    let case = case.unwrap_or_else(|| panic!("{} must exist in corpus", case_id));
    eprintln!(
        "[diff-harness {}] Waffle case status={:?} detail={}",
        case_id, case.status, case.detail
    );

    WaffleDumpPaths {
        workdir,
        path_a,
        path_b,
        path_stage_b,
    }
}

struct WaffleDumpPaths {
    workdir: PathBuf,
    path_a: PathBuf,
    path_b: PathBuf,
    path_stage_b: PathBuf,
}

/// Local enum mirroring `kernel::boolean::exact_mesh::MeshBooleanOp` (which
/// is `pub(crate)` and not exported). Used to plumb the actual op for each
/// dumped pair into the Cherchi invocation per PR-Y31 spec §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HarnessBoolOp {
    Union,
    Subtract,
    #[allow(dead_code)] // No assay case currently exercises Intersect; scaffolding per spec §11.
    Intersect,
}

fn op_to_cli_str(op: HarnessBoolOp) -> &'static str {
    match op {
        HarnessBoolOp::Union => "union",
        HarnessBoolOp::Subtract => "subtraction",
        HarnessBoolOp::Intersect => "intersection",
    }
}

/// Read `app/tests/cases/assay/<CASE_ID>.waffle` and determine the boolean
/// op for the FIRST dumped pair, which corresponds to the SECOND extrude
/// feature (the first extrude produces solid A; the second's `cut` flag
/// drives the first boolean against A). Per PR-Y31 spec §3:
/// - `"cut": false` (or absent) → Union
/// - `"cut": true` → Subtract
/// - Intersect is not represented in the current corpus; per
///   `feedback_yang_only.md` no fallback paths — panic with a clear message
///   if a value other than bool appears.
fn read_first_boolean_op(case_id: &str) -> HarnessBoolOp {
    let path = Path::new(ASSAY_DIR).join(format!("{}.waffle", case_id));
    let content = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "[diff-harness {}] failed to read .waffle at {}: {}",
            case_id,
            path.display(),
            e
        )
    });
    let json: serde_json::Value = serde_json::from_str(&content).unwrap_or_else(|e| {
        panic!(
            "[diff-harness {}] failed to parse .waffle JSON at {}: {}",
            case_id,
            path.display(),
            e
        )
    });

    let features = json
        .pointer("/tabs/0/kind/features/features")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| {
            panic!(
                "[diff-harness {}] .waffle missing tabs[0].kind.features.features array",
                case_id
            )
        });

    let mut extrude_count = 0usize;
    for feature in features {
        let op_type = feature
            .pointer("/operation/type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if op_type != "Extrude" {
            continue;
        }
        extrude_count += 1;
        if extrude_count == 2 {
            let cut = feature
                .pointer("/operation/params/cut")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            return if cut {
                HarnessBoolOp::Subtract
            } else {
                HarnessBoolOp::Union
            };
        }
    }
    panic!(
        "[diff-harness {}] .waffle has < 2 Extrude features; cannot determine \
         first boolean op (expected at least 2 extrudes for a boolean to exist)",
        case_id
    );
}

/// Invoke `mesh_booleans <op> A.obj B.obj OUT.obj` with a 30 s cap.
/// Returns None on timeout / non-zero exit / no output (each cause logged).
fn invoke_cherchi(
    bin: &Path,
    path_a: &Path,
    path_b: &Path,
    path_out: &Path,
    case_id: &str,
    op: HarnessBoolOp,
) -> Option<()> {
    let _ = std::fs::remove_file(path_out);
    let op_str = op_to_cli_str(op);
    let mut cmd = Command::new(bin);
    cmd.arg(op_str).arg(path_a).arg(path_b).arg(path_out);
    match run_with_timeout(cmd, CHERCHI_SUBPROCESS_TIMEOUT) {
        TimedRun::Completed(out) => {
            if !out.status.success() {
                eprintln!(
                    "[diff-harness {}] Cherchi non-zero exit ({:?}). stderr:\n{}",
                    case_id,
                    out.status,
                    String::from_utf8_lossy(&out.stderr)
                );
                return None;
            }
            if !path_out.exists() {
                eprintln!(
                    "[diff-harness {}] Cherchi exited 0 but produced no `{}`",
                    case_id,
                    path_out.display()
                );
                return None;
            }
            Some(())
        }
        TimedRun::TimedOut => {
            eprintln!(
                "[diff-harness {}] Cherchi timed out after {}s",
                case_id,
                CHERCHI_SUBPROCESS_TIMEOUT.as_secs()
            );
            None
        }
        TimedRun::SpawnFailed(e) => panic!("Cherchi spawn failed: {}", e),
    }
}

/// Run F0020 / cohort case through the full diff: emit Cherchi + Waffle
/// summaries, set difference counts, and TOP_N missing/extra triangles.
fn run_diff_for_case(case_id: &str) {
    let bin = match cherchi_bin() {
        Some(p) => p,
        None => {
            eprintln!(
                "[diff-harness {}] SKIP: CHERCHI2022_BIN unset/missing",
                case_id
            );
            return;
        }
    };

    let dumps = run_waffle_and_collect_dumps(case_id);

    if !dumps.path_a.exists() || !dumps.path_b.exists() {
        eprintln!(
            "[diff-harness {}] SKIP: Waffle A/B dumps did not land at {} / {} \
             (case may have short-circuited before the dump site)",
            case_id,
            dumps.path_a.display(),
            dumps.path_b.display()
        );
        return;
    }

    // Per PR-Y31 spec §3: read the boolean op for the FIRST dumped pair
    // from the .waffle JSON's second Extrude feature so Cherchi runs the
    // SAME op as Waffle (Cherchi 2022 §3 op-parameterized output).
    let op = read_first_boolean_op(case_id);
    let op_str = op_to_cli_str(op);
    eprintln!(
        "[diff-harness {}] resolved boolean op for first dumped pair: {:?} → cherchi `{}`",
        case_id, op, op_str
    );

    // Invoke Cherchi on Waffle's preprocessed A/B inputs.
    let path_cherchi_out = dumps.workdir.join(format!(
        "{}_cherchi_{}.obj",
        case_id.to_ascii_lowercase(),
        op_str
    ));
    if invoke_cherchi(
        &bin,
        &dumps.path_a,
        &dumps.path_b,
        &path_cherchi_out,
        case_id,
        op,
    )
    .is_none()
    {
        eprintln!(
            "[diff-harness {}] Cherchi invocation failed — cannot diff (Waffle \
             Stage B output, if present, is reported below)",
            case_id
        );
        // Still emit Waffle side if it landed.
        if dumps.path_stage_b.exists() {
            let (vw, tw) = parse_obj(&dumps.path_stage_b).expect("parse Waffle stage_B.obj");
            let rw = check_conformal(&vw, &tw);
            eprintln!(
                "=== {} diff (Cherchi unavailable) ===\nWaffle output:  {}",
                case_id,
                fmt_report_line(&rw, "Waffle Stage B")
            );
        }
        return;
    }

    let (cv, ct) = parse_obj(&path_cherchi_out).expect("parse Cherchi output");
    let cherchi_report = check_conformal(&cv, &ct);

    // Waffle Stage B may not land if the pipeline short-circuits/panics
    // upstream of Stage B (the post-survival conformal probe site).
    // Report what we have.
    if !dumps.path_stage_b.exists() {
        eprintln!(
            "=== {} diff (Waffle Stage B unavailable) ===\nCherchi output: {}\n\
             Waffle output:  ABSENT — Stage B dump not produced. Likely the \
             Yang pipeline panicked or returned an error before reaching the \
             Stage-B conformal probe site (`topology_extract.rs:2569`). The \
             case-status line above indicates the failure mode.",
            case_id,
            fmt_report_line(&cherchi_report, "Cherchi 2022")
        );
        return;
    }

    let (wv, wt) = parse_obj(&dumps.path_stage_b).expect("parse Waffle stage_B.obj");
    let waffle_report = check_conformal(&wv, &wt);

    // Build canonicalized quantized triangle sets for set diff.
    let cherchi_set: HashSet<[(i64, i64, i64); 3]> =
        ct.iter().map(|t| quantize_tri(&cv, *t)).collect();
    let waffle_set: HashSet<[(i64, i64, i64); 3]> =
        wt.iter().map(|t| quantize_tri(&wv, *t)).collect();

    let missing_from_waffle: Vec<&[(i64, i64, i64); 3]> =
        cherchi_set.difference(&waffle_set).collect();
    let extra_in_waffle: Vec<&[(i64, i64, i64); 3]> = waffle_set.difference(&cherchi_set).collect();
    let common = cherchi_set.intersection(&waffle_set).count();

    eprintln!("=== {} diff ===", case_id);
    eprintln!(
        "Cherchi output: {} triangles, {} vertices, well_formed={}, χ={}",
        ct.len(),
        cv.len(),
        cherchi_report.is_well_formed,
        cherchi_report.euler_characteristic,
    );
    eprintln!(
        "Waffle output:  {} triangles, {} vertices, well_formed={}, χ={}",
        wt.len(),
        wv.len(),
        waffle_report.is_well_formed,
        waffle_report.euler_characteristic,
    );
    eprintln!(
        "Triangle count delta: N_c - N_w = {}",
        ct.len() as i64 - wt.len() as i64
    );
    eprintln!(
        "\nPosition-quantized triangle set comparison (grid={:.0e} m, winding-insensitive):",
        QUANTIZE_GRID
    );
    eprintln!(
        "  In Cherchi, not in Waffle: {} triangles",
        missing_from_waffle.len()
    );
    eprintln!(
        "  In Waffle, not in Cherchi: {} triangles",
        extra_in_waffle.len()
    );
    eprintln!("  Common (matching quantized positions): {}", common);

    // Deterministic top-N: sort canonical keys for stable output. Without
    // sort, HashSet iteration order varies → spurious diffs on rerun.
    let mut missing_sorted: Vec<[(i64, i64, i64); 3]> =
        missing_from_waffle.iter().map(|&&t| t).collect();
    missing_sorted.sort();
    let mut extra_sorted: Vec<[(i64, i64, i64); 3]> = extra_in_waffle.iter().map(|&&t| t).collect();
    extra_sorted.sort();

    eprintln!(
        "\nTop {} missing-from-Waffle triangles (positions):",
        TOP_N_REPORT.min(missing_sorted.len())
    );
    for (i, t) in missing_sorted.iter().take(TOP_N_REPORT).enumerate() {
        eprintln!("  tri[{}] = {}", i, fmt_qtri(t));
    }
    eprintln!(
        "\nTop {} extra-in-Waffle triangles (positions):",
        TOP_N_REPORT.min(extra_sorted.len())
    );
    for (i, t) in extra_sorted.iter().take(TOP_N_REPORT).enumerate() {
        eprintln!("  tri[{}] = {}", i, fmt_qtri(t));
    }
    eprintln!("=== end {} diff ===\n", case_id);
}

/// F0020 baseline: compares our Stage-B post-survival boolean result
/// against Cherchi 2022 `mesh_booleans union` on the same preprocessed
/// inputs. Captures the diff for
/// `docs/audits/pr_y30_stage_b_baselines.md`; future PR-Y31+ canaries
/// consume this baseline.
#[test]
#[ignore]
fn f0020_cherchi_diff_baseline() {
    run_diff_for_case("F0020");
}

/// Cohort baseline: F0044, F0045, R0092 — three additional cases that
/// share the F0020 watertight signature per PR-Y28 cohort analysis.
/// Run as a single test (test-thread serial); each case prints its own
/// `=== <case> diff ===` block.
#[test]
#[ignore]
fn cohort_cherchi_diff_baseline() {
    for case in &["F0044", "F0045", "R0092"] {
        run_diff_for_case(case);
    }
}
