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

/// Outcome of one differential-diff invocation. Returned by
/// `run_diff_for_case` so assertion-style tests (PR-Y31+) can verify
/// numerical bounds rather than scrape eprintln output. `None` is
/// returned when the case is skipped (no Cherchi binary; Waffle dumps
/// absent; Cherchi invocation timed out / failed) — the caller decides
/// whether skip is fatal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiffCounts {
    missing: usize,
    extras: usize,
    common: usize,
}

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
    // PR-Y42: Render LOD output emitted by `tessellate_waffle_solid` at
    // `yang_integration.rs:1063-1074` as `stage_E_lod=Render.obj` (the post-
    // F.4 final render mesh; byte-identical to `stage_F.4.obj` since it
    // captures the same return value of `tessellate_solid_bounded`). When
    // multiple boolean ops run in a multi-extrude case, the file is
    // overwritten — last-write IS the final mesh shipped to the assay
    // oracle.
    let path_render_lod = stage_dump_dir
        .join(case_id)
        .join("stage_E_lod=Render.obj");

    // Clean any stale outputs so a partial run can't be mistaken for fresh.
    for p in [&path_a, &path_b, &path_stage_b, &path_render_lod] {
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
        path_render_lod,
    }
}

struct WaffleDumpPaths {
    workdir: PathBuf,
    path_a: PathBuf,
    path_b: PathBuf,
    path_stage_b: PathBuf,
    path_render_lod: PathBuf,
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
fn run_diff_for_case(case_id: &str) -> Option<DiffCounts> {
    let bin = match cherchi_bin() {
        Some(p) => p,
        None => {
            eprintln!(
                "[diff-harness {}] SKIP: CHERCHI2022_BIN unset/missing",
                case_id
            );
            return None;
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
        return None;
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
        return None;
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
        return None;
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

    // Y32_DUMP_POSITIONS: emit full position lists for canary use. Quant
    // keys are integers; we dump them as `[qx,qy,qz];[qx,qy,qz];[qx,qy,qz]`
    // one per line, sorted, with a section delimiter so a downstream test
    // can grep + load them via stable IDs.
    if std::env::var("Y32_DUMP_POSITIONS").as_deref() == Ok("1") {
        eprintln!("[y32-positions {} missing-from-waffle begin n={}]", case_id, missing_sorted.len());
        for t in &missing_sorted {
            eprintln!(
                "[y32-pos {}] M {},{},{};{},{},{};{},{},{}",
                case_id, t[0].0, t[0].1, t[0].2, t[1].0, t[1].1, t[1].2, t[2].0, t[2].1, t[2].2
            );
        }
        eprintln!("[y32-positions {} missing-from-waffle end]", case_id);
        eprintln!("[y32-positions {} extra-in-waffle begin n={}]", case_id, extra_sorted.len());
        for t in &extra_sorted {
            eprintln!(
                "[y32-pos {}] X {},{},{};{},{},{};{},{},{}",
                case_id, t[0].0, t[0].1, t[0].2, t[1].0, t[1].1, t[1].2, t[2].0, t[2].1, t[2].2
            );
        }
        eprintln!("[y32-positions {} extra-in-waffle end]", case_id);
    }

    eprintln!("=== end {} diff ===\n", case_id);

    Some(DiffCounts {
        missing: missing_from_waffle.len(),
        extras: extra_in_waffle.len(),
        common,
    })
}

/// F0020 baseline: compares our Stage-B post-survival boolean result
/// against Cherchi 2022 `mesh_booleans union` on the same preprocessed
/// inputs. Captures the diff for
/// `docs/audits/pr_y30_stage_b_baselines.md`; future PR-Y31+ canaries
/// consume this baseline.
#[test]
#[ignore]
fn f0020_cherchi_diff_baseline() {
    let _ = run_diff_for_case("F0020");
}

/// Cohort baseline: F0044, F0045, R0092 — three additional cases that
/// share the F0020 watertight signature per PR-Y28 cohort analysis.
/// Run as a single test (test-thread serial); each case prints its own
/// `=== <case> diff ===` block.
#[test]
#[ignore]
fn cohort_cherchi_diff_baseline() {
    for case in &["F0044", "F0045", "R0092"] {
        let _ = run_diff_for_case(case);
    }
}

/// PR-Y31 load-bearing assertion: F0044's "48 extras" was a harness
/// mis-configuration (PR-Y29/Y30 hardcoded Cherchi `union`; F0044's
/// first dumped op is `Subtract`). With the op-plumb fix at impl-y31
/// (`e720629`), F0044 Stage B vs Cherchi `subtraction` on the same
/// preprocessed inputs must agree: 0 missing AND 0 extras.
///
/// Pre-fix baseline (PR-Y30 `27a09ed`, harness invoked `union`):
/// 88 common, 0 missing, 48 extras.
/// Post-fix expectation (this commit, harness invokes `subtraction`):
/// 136 common, 0 missing, 0 extras.
///
/// Skip-quietly contract: if `CHERCHI2022_BIN` is unset or the binary
/// is missing, the test no-ops (matches the baseline tests above);
/// this matches the `#[ignore]` posture — CI/local devs without
/// Cherchi installed can still run `cargo test` and not see a hard
/// failure.
#[test]
#[ignore]
fn pr_y31_f0044_extras_zero() {
    let Some(counts) = run_diff_for_case("F0044") else {
        eprintln!("[pr_y31_f0044_extras_zero] SKIP: diff harness returned None");
        return;
    };
    assert_eq!(
        counts.extras, 0,
        "PR-Y31 F0044 Stage B extras must be 0 (was 48 pre-op-plumb-fix); \
         got missing={} extras={} common={}",
        counts.missing, counts.extras, counts.common
    );
    assert_eq!(
        counts.missing, 0,
        "PR-Y31 F0044 Stage B missing must be 0 (no triangles dropped relative to Cherchi); \
         got missing={} extras={} common={}",
        counts.missing, counts.extras, counts.common
    );
}

// ── PR-Y42: Render LOD diff (strategic-pivot B.1 from PR-Y41 §5) ────────
//
// After 10 cycles of Waffle-internal probes on F0020 Render LOD (Y25-Y28
// ABORTs; Y36/Y37/Y38/Y40/Y41 INFRA-only SHIPs), the diagnostic-strategy
// pivot extends the existing PR-Y29/Y30/Y31 Stage-B diff to the FINAL
// render mesh layer. Cherchi C++ has no separate Render LOD pass — its
// `mesh_booleans` output IS its final mesh. Diff target: which Cherchi
// triangles are MISSING from Waffle's Render LOD output, and conversely
// which Waffle Render LOD triangles are EXTRAS.
//
// For F0020 specifically: attribute the 40 unpaired-edge defect by
// cross-referencing Cherchi-only missing triangles against the oracle's
// unpaired-edge positions. The watertight oracle at `oracle.rs:185-274`
// computes a scale-adaptive position grid (max_abs * TAU_TESS_GRID_FACTOR
// = max_abs * 1e-5, with f32 vertex round-trip). To match the oracle, we
// replicate the same quantization on Waffle's Render LOD OBJ (cast f64
// → f32 → quantize), enumerate the unpaired edges, then bucket Cherchi-
// only missing triangles by edge-position match.

/// Outcome of `run_render_lod_diff_for_case`. Mirrors `DiffCounts` plus
/// the F0020-specific attribution fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // baseline tests inspect via Debug print; future asserts may consume fields.
struct RenderLodDiffCounts {
    waffle_tris: usize,
    cherchi_tris: usize,
    missing: usize,
    extras: usize,
    common: usize,
    waffle_unpaired_edges: usize,
    /// Cherchi-only missing triangles whose at-least-one edge matches a
    /// Waffle-Render-LOD unpaired-edge position (oracle quantization).
    missing_tris_explaining_unpaired: usize,
    /// Unique Waffle unpaired-edge positions explained by at-least-one
    /// Cherchi-only missing triangle.
    unpaired_edges_explained: usize,
}

/// Oracle's edge type: positionally-quantized undirected edge.
/// Mirrors `oracle.rs`'s `PosEdge` exactly.
type OraclePosEdge = ((i64, i64, i64), (i64, i64, i64));

/// Replicate `check_watertight_mesh`'s scale-adaptive quantization on
/// f64-loaded OBJ vertices. The oracle quantizes f32 vertices at
/// `max_abs * TAU_TESS_GRID_FACTOR` with a `TAU_TESS_GRID_MIN` floor;
/// we cast f64 → f32 first to match RenderMesh's storage precision.
fn oracle_quantize_waffle_obj(
    verts_f64: &[[f64; 3]],
    tris: &[[usize; 3]],
) -> (
    std::collections::HashMap<OraclePosEdge, usize>,
    Vec<(i64, i64, i64)>, // per-vertex quantized key (index by vert idx)
    f64,                  // grid_size used
) {
    use kernel::units::{TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN};
    let max_abs = verts_f64
        .iter()
        .flat_map(|v| v.iter())
        .map(|x| (*x as f32).abs())
        .fold(0.0_f32, f32::max);
    let grid_size = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid_size;
    let quantize_f32 = |v: f32| -> i64 { (v as f64 * inv_grid).round() as i64 };

    let vert_keys: Vec<(i64, i64, i64)> = verts_f64
        .iter()
        .map(|v| {
            (
                quantize_f32(v[0] as f32),
                quantize_f32(v[1] as f32),
                quantize_f32(v[2] as f32),
            )
        })
        .collect();

    fn make_edge(a: (i64, i64, i64), b: (i64, i64, i64)) -> OraclePosEdge {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    let mut edge_counts: std::collections::HashMap<OraclePosEdge, usize> =
        std::collections::HashMap::new();
    for tri in tris {
        let va = vert_keys[tri[0]];
        let vb = vert_keys[tri[1]];
        let vc = vert_keys[tri[2]];
        *edge_counts.entry(make_edge(va, vb)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(vb, vc)).or_insert(0) += 1;
        *edge_counts.entry(make_edge(vc, va)).or_insert(0) += 1;
    }
    (edge_counts, vert_keys, grid_size)
}

/// Quantize a Cherchi-side f64 vertex against the SAME oracle grid the
/// Waffle side computed. Cherchi's mesh is the diff target — we use
/// Waffle's grid (the one driving the 40 unpaired count) so attribution
/// is in the oracle's own coordinate frame.
fn oracle_quantize_cherchi_vert(v: [f64; 3], grid_size: f64) -> (i64, i64, i64) {
    let inv_grid = 1.0 / grid_size;
    (
        ((v[0] as f32) as f64 * inv_grid).round() as i64,
        ((v[1] as f32) as f64 * inv_grid).round() as i64,
        ((v[2] as f32) as f64 * inv_grid).round() as i64,
    )
}

/// Run F0020 / cohort case through the Render LOD diff (PR-Y42 B.1).
/// Mirrors `run_diff_for_case` shape; reuses A/B dumps + Cherchi invoke.
fn run_render_lod_diff_for_case(case_id: &str) -> Option<RenderLodDiffCounts> {
    let bin = match cherchi_bin() {
        Some(p) => p,
        None => {
            eprintln!(
                "[render-lod-diff {}] SKIP: CHERCHI2022_BIN unset/missing",
                case_id
            );
            return None;
        }
    };

    let dumps = run_waffle_and_collect_dumps(case_id);

    if !dumps.path_a.exists() || !dumps.path_b.exists() {
        eprintln!(
            "[render-lod-diff {}] SKIP: Waffle A/B dumps did not land at {} / {} \
             (case may have short-circuited before the dump site)",
            case_id,
            dumps.path_a.display(),
            dumps.path_b.display()
        );
        return None;
    }

    let op = read_first_boolean_op(case_id);
    let op_str = op_to_cli_str(op);
    eprintln!(
        "[render-lod-diff {}] resolved boolean op for first dumped pair: {:?} → cherchi `{}`",
        case_id, op, op_str
    );

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
            "[render-lod-diff {}] Cherchi invocation failed — cannot diff",
            case_id
        );
        return None;
    }

    if !dumps.path_render_lod.exists() {
        eprintln!(
            "[render-lod-diff {}] SKIP: Waffle Render LOD dump absent at {} \
             (case may have errored/panicked before tessellate_waffle_solid \
             reached the post-Render-LOD probe site)",
            case_id,
            dumps.path_render_lod.display()
        );
        return None;
    }

    let (cv, ct) = parse_obj(&path_cherchi_out).expect("parse Cherchi output");
    let cherchi_report = check_conformal(&cv, &ct);
    let (wv, wt) = parse_obj(&dumps.path_render_lod).expect("parse Waffle Render LOD OBJ");
    let waffle_report = check_conformal(&wv, &wt);

    // Position-quantized triangle-set diff at the 1e-6 spec grid (matches
    // PR-Y29/Y30/Y31 Stage-B diff convention so missing/extras numbers are
    // comparable across stages).
    let cherchi_set: HashSet<[(i64, i64, i64); 3]> =
        ct.iter().map(|t| quantize_tri(&cv, *t)).collect();
    let waffle_set: HashSet<[(i64, i64, i64); 3]> =
        wt.iter().map(|t| quantize_tri(&wv, *t)).collect();

    let missing_from_waffle: Vec<&[(i64, i64, i64); 3]> =
        cherchi_set.difference(&waffle_set).collect();
    let extra_in_waffle: Vec<&[(i64, i64, i64); 3]> = waffle_set.difference(&cherchi_set).collect();
    let common = cherchi_set.intersection(&waffle_set).count();

    eprintln!("=== {} Render LOD diff ===", case_id);
    eprintln!(
        "Cherchi output: {} triangles, {} vertices, well_formed={}, χ={}",
        ct.len(),
        cv.len(),
        cherchi_report.is_well_formed,
        cherchi_report.euler_characteristic,
    );
    eprintln!(
        "Waffle Render LOD: {} triangles, {} vertices, well_formed={}, χ={}",
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
        "  Missing (in Cherchi, not in Waffle Render LOD): {}",
        missing_from_waffle.len()
    );
    eprintln!(
        "  Extras  (in Waffle Render LOD, not in Cherchi): {}",
        extra_in_waffle.len()
    );
    eprintln!("  Common (matching quantized positions): {}", common);

    let mut missing_sorted: Vec<[(i64, i64, i64); 3]> =
        missing_from_waffle.iter().map(|&&t| t).collect();
    missing_sorted.sort();
    let mut extra_sorted: Vec<[(i64, i64, i64); 3]> = extra_in_waffle.iter().map(|&&t| t).collect();
    extra_sorted.sort();

    eprintln!(
        "\nTop {} missing-from-Waffle Render LOD triangles (positions):",
        TOP_N_REPORT.min(missing_sorted.len())
    );
    for (i, t) in missing_sorted.iter().take(TOP_N_REPORT).enumerate() {
        eprintln!("  tri[{}] = {}", i, fmt_qtri(t));
    }
    eprintln!(
        "\nTop {} extra-in-Waffle Render LOD triangles (positions):",
        TOP_N_REPORT.min(extra_sorted.len())
    );
    for (i, t) in extra_sorted.iter().take(TOP_N_REPORT).enumerate() {
        eprintln!("  tri[{}] = {}", i, fmt_qtri(t));
    }

    // ── Attribution: cross-reference missing triangles vs oracle unpaired ──
    //
    // Compute the oracle's unpaired-edge set on Waffle's Render LOD OBJ
    // (using the SAME scale-adaptive grid the production oracle uses).
    // Then quantize each missing-from-Waffle Cherchi triangle's 3 edges
    // against the same grid and bucket by edge-position overlap.
    let (waffle_edge_counts, _wv_keys, grid_size) = oracle_quantize_waffle_obj(&wv, &wt);
    let unpaired_edges: std::collections::HashSet<OraclePosEdge> = waffle_edge_counts
        .iter()
        .filter(|(_, &c)| c != 2)
        .map(|(e, _)| *e)
        .collect();
    let count_1 = waffle_edge_counts.iter().filter(|(_, &c)| c == 1).count();
    let count_3plus = waffle_edge_counts.iter().filter(|(_, &c)| c >= 3).count();

    fn make_oracle_edge(a: (i64, i64, i64), b: (i64, i64, i64)) -> OraclePosEdge {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    let mut missing_tris_explaining_unpaired = 0usize;
    let mut explained_edges: std::collections::HashSet<OraclePosEdge> =
        std::collections::HashSet::new();
    // For top-N report of which missing triangles bound which unpaired edges.
    let mut explanation_records: Vec<(
        [(i64, i64, i64); 3],
        Vec<OraclePosEdge>,
    )> = Vec::new();
    for tri in &missing_sorted {
        // Re-derive the missing triangle's 3 vertex positions back to
        // floating-point (it was Cherchi-side; we need the SAME oracle
        // grid quantization Waffle uses). Convert the 1e-6 quantized
        // positions back to f64 metres, then quantize against the oracle
        // grid. This is a two-step requantization — the 1e-6 → metres
        // path is lossy only at the sub-µm level (well below the oracle
        // grid of `max_abs * 1e-5` which for F0020's max_abs ~0.1m is
        // ~1µm); we accept this as an attribution-time precision floor.
        let to_m = |q: (i64, i64, i64)| -> [f64; 3] {
            [
                q.0 as f64 * QUANTIZE_GRID,
                q.1 as f64 * QUANTIZE_GRID,
                q.2 as f64 * QUANTIZE_GRID,
            ]
        };
        let oa = oracle_quantize_cherchi_vert(to_m(tri[0]), grid_size);
        let ob = oracle_quantize_cherchi_vert(to_m(tri[1]), grid_size);
        let oc = oracle_quantize_cherchi_vert(to_m(tri[2]), grid_size);
        let edges = [
            make_oracle_edge(oa, ob),
            make_oracle_edge(ob, oc),
            make_oracle_edge(oc, oa),
        ];
        let mut matched_edges: Vec<OraclePosEdge> = Vec::new();
        for e in &edges {
            if unpaired_edges.contains(e) {
                matched_edges.push(*e);
                explained_edges.insert(*e);
            }
        }
        if !matched_edges.is_empty() {
            missing_tris_explaining_unpaired += 1;
            explanation_records.push((*tri, matched_edges));
        }
    }

    let coverage_pct = if !unpaired_edges.is_empty() {
        (explained_edges.len() as f64) * 100.0 / (unpaired_edges.len() as f64)
    } else {
        0.0
    };

    eprintln!(
        "\nOracle attribution (grid={:.6e} m; f32 round-trip):",
        grid_size
    );
    eprintln!(
        "  Waffle Render LOD unpaired edges: {} ({} boundary, {} non-manifold)",
        unpaired_edges.len(),
        count_1,
        count_3plus
    );
    eprintln!(
        "  Cherchi-only missing tris with ≥1 edge matching unpaired: {}/{}",
        missing_tris_explaining_unpaired,
        missing_sorted.len()
    );
    eprintln!(
        "  Unpaired edges explained by ≥1 missing tri: {}/{} ({:.1}%)",
        explained_edges.len(),
        unpaired_edges.len(),
        coverage_pct
    );

    eprintln!(
        "\nTop {} attribution records (missing-tri → unpaired-edges):",
        TOP_N_REPORT.min(explanation_records.len())
    );
    for (i, (tri, edges)) in explanation_records.iter().take(TOP_N_REPORT).enumerate() {
        eprintln!(
            "  rec[{}] tri={} matched_edges={}",
            i,
            fmt_qtri(tri),
            edges.len()
        );
    }

    eprintln!("=== end {} Render LOD diff ===\n", case_id);

    Some(RenderLodDiffCounts {
        waffle_tris: wt.len(),
        cherchi_tris: ct.len(),
        missing: missing_from_waffle.len(),
        extras: extra_in_waffle.len(),
        common,
        waffle_unpaired_edges: unpaired_edges.len(),
        missing_tris_explaining_unpaired,
        unpaired_edges_explained: explained_edges.len(),
    })
}

/// PR-Y42 F0020 Render LOD diff baseline. Compares Waffle's final render
/// mesh (post-`tessellate_waffle_solid` E_lod=Render dump) against the
/// Cherchi 2022 `mesh_booleans <op>` output, and attributes the watertight
/// oracle's unpaired-edge defect to specific Cherchi-only missing tris.
///
/// Skip-quietly contract identical to `f0020_cherchi_diff_baseline`: no
/// Cherchi binary → no-op (matches `#[ignore]` posture).
#[test]
#[ignore]
fn f0020_render_lod_diff_baseline() {
    let _ = run_render_lod_diff_for_case("F0020");
}

/// PR-Y42 cohort baseline: F0044/F0045/R0092 at Render LOD.
///
/// F0044 is the highest-signal cohort case per PR-Y30/Y31 — its Stage B
/// is byte-clean against Cherchi (missing=0, extras=0). If F0044 Render
/// LOD ALSO has missing tris, the Render LOD post-processing IS the
/// defect mechanism downstream of byte-clean Stage B (cleanest possible
/// attribution signal in the cohort).
#[test]
#[ignore]
fn cohort_render_lod_diff_baseline() {
    for case in &["F0044", "F0045", "R0092"] {
        let _ = run_render_lod_diff_for_case(case);
    }
}

// ── PR-Y43: A/B/C nearest-triangle attribution ─────────────────────────
//
// 12th investigational PR on F0020 Render LOD. PR-Y42 found 20/40 = 50%
// of unpaired edges explained by 42 Cherchi-only missing tris (borderline-
// sharp). PR-Y43 asks: what does Waffle have NEARBY each of those 42
// missing tris? Per the plan §Phase 2 verdict logic:
//
//   - Case A (sub-grid drift): all 3 verts match at >1× grid but not 1×
//   - Case B (partial match):  exactly 2 of 3 verts match at 1× grid
//   - Case C (no proximity):   ≤1 vert has any near-Waffle match
//   - Case D (residual):       everything else (esp. 3-of-3 at 1×, meaning
//                              positional match but triangle still missing)
//
// Each case implies a different PR-Y44 fix-shape. Grid levels probed:
// 1× / 2× / 5× / 10× of the oracle's scale-adaptive grid
// (max_abs * TAU_TESS_GRID_FACTOR = max_abs * 1e-5). Match at N× = the
// Cherchi vert quantizes to a key present in Waffle's Render-LOD vertex
// set at that grid scale.

/// Per-triangle nearest-Waffle attribution counts. Three of the four
/// counts (match_at_1x / _2x / _5x) drive A/B/C/D classification.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Inspected via Debug print + classification logic; future asserts may consume.
struct NearestVertAttribution {
    match_at_1x: u8,    // 0..=3
    match_at_2x: u8,
    match_at_5x: u8,
    match_at_10x: u8,
    /// For Case B aggregation: index (0/1/2) of the OFF vertex (the one
    /// NOT matched at 1×). Only meaningful when match_at_1x == 2.
    off_vert_idx_when_b: Option<u8>,
}

/// PR-Y44 δ: per-Case-D 4-tuple of grid-match counts at 1×/2×/5×/10×.
/// Separates sub-class (a) `(m1x=3, m5x=3)` ← topology-emission defect
/// from sub-class (b) `(m1x ∈ {0,1}, m5x=2)` ← partial-proximity residual.
/// All other tuples fall under "other" and indicate an unexpected
/// sub-mechanism in the Case D residual catch-all bucket.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
struct CaseDSubclassTuple {
    match_at_1x: u8,
    match_at_2x: u8,
    match_at_5x: u8,
    match_at_10x: u8,
}

/// Per-grid Waffle vertex set. Keys are quantized at `grid_size * factor`.
struct WaffleVertSetAtGrid {
    factor: u32, // 1, 2, 5, 10
    keys: std::collections::HashSet<(i64, i64, i64)>,
}

/// Build the four Waffle-vert key sets (1×, 2×, 5×, 10× of base grid).
/// The base grid is the oracle's scale-adaptive grid; coarser factors
/// yield larger cells (more keys collide). f32 round-trip preserved.
fn build_waffle_vert_sets_at_grids(
    verts_f64: &[[f64; 3]],
    base_grid: f64,
) -> [WaffleVertSetAtGrid; 4] {
    let factors = [1u32, 2, 5, 10];
    factors.map(|factor| {
        let grid = base_grid * factor as f64;
        let inv_grid = 1.0 / grid;
        let quantize_f32 =
            |v: f32| -> i64 { (v as f64 * inv_grid).round() as i64 };
        let keys: std::collections::HashSet<(i64, i64, i64)> = verts_f64
            .iter()
            .map(|v| {
                (
                    quantize_f32(v[0] as f32),
                    quantize_f32(v[1] as f32),
                    quantize_f32(v[2] as f32),
                )
            })
            .collect();
        WaffleVertSetAtGrid { factor, keys }
    })
}

/// Re-quantize one Cherchi-side f64 vertex (already passed through the
/// 1e-6 → metres → f32 lossy path from PR-Y42) at multiple grid scales,
/// and ask: at each scale, does the Cherchi-vert quantize to a key in
/// the Waffle vertex set?
///
/// Returns one bool per grid level (in the same order as
/// `build_waffle_vert_sets_at_grids`: 1×, 2×, 5×, 10×).
fn cherchi_vert_matches_waffle_at_grids(
    cherchi_v: [f64; 3],
    base_grid: f64,
    waffle_sets: &[WaffleVertSetAtGrid; 4],
) -> [bool; 4] {
    let mut out = [false; 4];
    for (i, wset) in waffle_sets.iter().enumerate() {
        let grid = base_grid * wset.factor as f64;
        let inv_grid = 1.0 / grid;
        let q = (
            ((cherchi_v[0] as f32) as f64 * inv_grid).round() as i64,
            ((cherchi_v[1] as f32) as f64 * inv_grid).round() as i64,
            ((cherchi_v[2] as f32) as f64 * inv_grid).round() as i64,
        );
        out[i] = wset.keys.contains(&q);
    }
    out
}

/// Find the nearest Waffle vertex (Chebyshev cell-distance) at the BASE
/// grid for a given Cherchi vertex. Used for Case B off-vertex dumps —
/// gives a concrete (Cherchi position, nearest Waffle position,
/// cell-distance) tuple for the PR-Y44 investigation.
fn nearest_waffle_vert_at_base_grid(
    cherchi_v: [f64; 3],
    base_grid: f64,
    waffle_verts_f64: &[[f64; 3]],
) -> (usize, i64, [f64; 3]) {
    let inv_grid = 1.0 / base_grid;
    let cq = (
        ((cherchi_v[0] as f32) as f64 * inv_grid).round() as i64,
        ((cherchi_v[1] as f32) as f64 * inv_grid).round() as i64,
        ((cherchi_v[2] as f32) as f64 * inv_grid).round() as i64,
    );
    let mut best_idx = 0usize;
    let mut best_dist = i64::MAX;
    let mut best_pos = waffle_verts_f64.first().copied().unwrap_or([0.0; 3]);
    for (i, wv) in waffle_verts_f64.iter().enumerate() {
        let wq = (
            ((wv[0] as f32) as f64 * inv_grid).round() as i64,
            ((wv[1] as f32) as f64 * inv_grid).round() as i64,
            ((wv[2] as f32) as f64 * inv_grid).round() as i64,
        );
        let dx = (wq.0 - cq.0).abs();
        let dy = (wq.1 - cq.1).abs();
        let dz = (wq.2 - cq.2).abs();
        let d = dx.max(dy).max(dz); // Chebyshev / L∞
        if d < best_dist {
            best_dist = d;
            best_idx = i;
            best_pos = *wv;
        }
    }
    (best_idx, best_dist, best_pos)
}

/// Classify a single attribution record (match counts at 1×/2×/5×/10×).
/// Returns one of "A", "B", "C", "D" per the PR-Y43 plan §Phase 2.
fn classify_attribution(attr: &NearestVertAttribution) -> &'static str {
    if attr.match_at_5x == 3 && attr.match_at_1x < 3 {
        "A"
    } else if attr.match_at_1x == 2 {
        "B"
    } else if attr.match_at_5x <= 1 {
        "C"
    } else {
        "D"
    }
}

/// Outcome of `run_nearest_attribution_for_case`. Drives gate-4 histogram
/// + gate-5 Case B dump.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Returned for caller-side asserts; printed via Debug.
struct NearestAttributionResult {
    case_id: String,
    target_tri_count: usize, // F0020: 42; cohort: varies
    case_a: usize,
    case_b: usize,
    case_c: usize,
    case_d: usize,
    /// PR-Y44 δ: per-Case-D 4-tuple capture for sub-class disambiguation.
    /// Length == case_d. Same insertion order as classification loop.
    case_d_tuples: Vec<CaseDSubclassTuple>,
}

/// PR-Y43 core probe. Mirrors `run_render_lod_diff_for_case` but adds
/// A/B/C/D classification of the missing-attributable triangles.
///
/// Steps:
/// 1. Replay PR-Y42's harness: dump Waffle A/B/Render-LOD; invoke Cherchi;
///    diff the 1e-6-quantized triangle sets; compute oracle unpaired
///    edges + attribution.
/// 2. Build Waffle vertex sets at four quantization grids (1×/2×/5×/10×
///    of base = max_abs * TAU_TESS_GRID_FACTOR).
/// 3. For each missing-attributable Cherchi triangle, count vert matches
///    at each grid level → classify A/B/C/D.
/// 4. For Case B: dump (off-vertex Cherchi pos, nearest Waffle pos,
///    cell-distance) for PR-Y44 anchor candidate data.
fn run_nearest_attribution_for_case(case_id: &str) -> Option<NearestAttributionResult> {
    let bin = match cherchi_bin() {
        Some(p) => p,
        None => {
            eprintln!(
                "[nearest-attr {}] SKIP: CHERCHI2022_BIN unset/missing",
                case_id
            );
            return None;
        }
    };

    let dumps = run_waffle_and_collect_dumps(case_id);

    if !dumps.path_a.exists() || !dumps.path_b.exists() {
        eprintln!(
            "[nearest-attr {}] SKIP: Waffle A/B dumps did not land",
            case_id
        );
        return None;
    }

    let op = read_first_boolean_op(case_id);
    let op_str = op_to_cli_str(op);
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
            "[nearest-attr {}] Cherchi invocation failed — cannot run probe",
            case_id
        );
        return None;
    }

    if !dumps.path_render_lod.exists() {
        eprintln!(
            "[nearest-attr {}] SKIP: Waffle Render LOD dump absent",
            case_id
        );
        return None;
    }

    let (cv, ct) = parse_obj(&path_cherchi_out).expect("parse Cherchi output");
    let (wv, wt) = parse_obj(&dumps.path_render_lod).expect("parse Waffle Render LOD OBJ");

    // ── Replay PR-Y42 1e-6 set diff to identify the 194 missing tris ──
    let cherchi_set: HashSet<[(i64, i64, i64); 3]> =
        ct.iter().map(|t| quantize_tri(&cv, *t)).collect();
    let waffle_set: HashSet<[(i64, i64, i64); 3]> =
        wt.iter().map(|t| quantize_tri(&wv, *t)).collect();
    let missing_from_waffle: Vec<&[(i64, i64, i64); 3]> =
        cherchi_set.difference(&waffle_set).collect();
    let mut missing_sorted: Vec<[(i64, i64, i64); 3]> =
        missing_from_waffle.iter().map(|&&t| t).collect();
    missing_sorted.sort();

    // ── Replay PR-Y42 oracle attribution to identify the 42 attributable tris ──
    let (waffle_edge_counts, _wv_keys, base_grid) = oracle_quantize_waffle_obj(&wv, &wt);
    let unpaired_edges: std::collections::HashSet<OraclePosEdge> = waffle_edge_counts
        .iter()
        .filter(|(_, &c)| c != 2)
        .map(|(e, _)| *e)
        .collect();

    fn make_oracle_edge(a: (i64, i64, i64), b: (i64, i64, i64)) -> OraclePosEdge {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }
    let to_m = |q: (i64, i64, i64)| -> [f64; 3] {
        [
            q.0 as f64 * QUANTIZE_GRID,
            q.1 as f64 * QUANTIZE_GRID,
            q.2 as f64 * QUANTIZE_GRID,
        ]
    };

    // Filter to the missing-attributable subset (= the "42" set for F0020).
    let mut target_tris: Vec<[(i64, i64, i64); 3]> = Vec::new();
    for tri in &missing_sorted {
        let oa = oracle_quantize_cherchi_vert(to_m(tri[0]), base_grid);
        let ob = oracle_quantize_cherchi_vert(to_m(tri[1]), base_grid);
        let oc = oracle_quantize_cherchi_vert(to_m(tri[2]), base_grid);
        let edges = [
            make_oracle_edge(oa, ob),
            make_oracle_edge(ob, oc),
            make_oracle_edge(oc, oa),
        ];
        if edges.iter().any(|e| unpaired_edges.contains(e)) {
            target_tris.push(*tri);
        }
    }

    eprintln!(
        "[nearest-attr {}] base_grid={:.6e} m; target_tris={} (missing-attributable)",
        case_id,
        base_grid,
        target_tris.len()
    );

    // ── Build Waffle vert key sets at 1×/2×/5×/10× of base grid ──
    let waffle_sets = build_waffle_vert_sets_at_grids(&wv, base_grid);

    // ── Classify each target triangle ──
    let mut case_a = 0usize;
    let mut case_b = 0usize;
    let mut case_c = 0usize;
    let mut case_d = 0usize;

    // For Case B vertex dump
    let mut case_b_dumps: Vec<(
        [(i64, i64, i64); 3],
        u8,        // off-vert idx (0/1/2)
        [f64; 3],  // Cherchi position (lossy from 1e-6 grid)
        [f64; 3],  // Nearest Waffle position (raw OBJ f64)
        i64,       // Chebyshev cell-distance at base grid
    )> = Vec::new();

    // PR-Y44 δ: per-Case-D 4-tuple capture for sub-class disambiguation.
    // Pairs (quantized tri, tuple) so the per-tri table can print the tri id.
    let mut case_d_entries: Vec<([(i64, i64, i64); 3], CaseDSubclassTuple)> = Vec::new();

    for tri in &target_tris {
        // Three vertex positions (Cherchi-side, lossily de-quantized).
        let vs = [to_m(tri[0]), to_m(tri[1]), to_m(tri[2])];

        // Per-vertex match flags across 4 grids.
        let m0 = cherchi_vert_matches_waffle_at_grids(vs[0], base_grid, &waffle_sets);
        let m1 = cherchi_vert_matches_waffle_at_grids(vs[1], base_grid, &waffle_sets);
        let m2 = cherchi_vert_matches_waffle_at_grids(vs[2], base_grid, &waffle_sets);

        // Aggregate per grid.
        let count_at = |g_idx: usize| -> u8 {
            (m0[g_idx] as u8) + (m1[g_idx] as u8) + (m2[g_idx] as u8)
        };
        // off_vert_idx_when_b: which of the 3 verts is NOT matched at 1×
        // when match_at_1x == 2 (i.e., 2 of 3 verts matched, 1 missed).
        let at_1x = [m0[0], m1[0], m2[0]];
        let unmatched_at_1x: Vec<u8> = (0..3u8)
            .filter(|&i| !at_1x[i as usize])
            .collect();
        let off_idx = if unmatched_at_1x.len() == 1 {
            Some(unmatched_at_1x[0])
        } else {
            None
        };

        let attr = NearestVertAttribution {
            match_at_1x: count_at(0),
            match_at_2x: count_at(1),
            match_at_5x: count_at(2),
            match_at_10x: count_at(3),
            off_vert_idx_when_b: off_idx,
        };
        let cls = classify_attribution(&attr);

        match cls {
            "A" => case_a += 1,
            "B" => {
                case_b += 1;
                if let Some(off) = off_idx {
                    let (_idx, dist, wpos) = nearest_waffle_vert_at_base_grid(
                        vs[off as usize],
                        base_grid,
                        &wv,
                    );
                    case_b_dumps.push((*tri, off, vs[off as usize], wpos, dist));
                }
            }
            "C" => case_c += 1,
            _ => {
                case_d += 1;
                // PR-Y44 δ: capture per-Case-D 4-tuple for sub-class disambiguation.
                case_d_entries.push((
                    *tri,
                    CaseDSubclassTuple {
                        match_at_1x: attr.match_at_1x,
                        match_at_2x: attr.match_at_2x,
                        match_at_5x: attr.match_at_5x,
                        match_at_10x: attr.match_at_10x,
                    },
                ));
            }
        }
    }

    eprintln!(
        "\n=== {} A/B/C/D classification (PR-Y43) ===",
        case_id
    );
    let n = target_tris.len().max(1); // avoid div-by-zero on cohort common=0 + 0 target
    let pct = |c: usize| -> f64 { (c as f64) * 100.0 / (n as f64) };
    eprintln!(
        "  Case A (sub-grid drift; all-3 verts ≤5× grid + not all-3 at 1×): {} / {} = {:.1}%",
        case_a, target_tris.len(), pct(case_a)
    );
    eprintln!(
        "  Case B (partial match; exactly 2 verts at 1× + 1 off):            {} / {} = {:.1}%",
        case_b, target_tris.len(), pct(case_b)
    );
    eprintln!(
        "  Case C (no proximity; ≤1 vert anywhere at 5× grid):               {} / {} = {:.1}%",
        case_c, target_tris.len(), pct(case_c)
    );
    eprintln!(
        "  Case D (residual; e.g., all-3 at 1× but triangle missing):        {} / {} = {:.1}%",
        case_d, target_tris.len(), pct(case_d)
    );

    // Case B vertex dump — PR-Y44 candidate-anchor data.
    if !case_b_dumps.is_empty() {
        eprintln!(
            "\n=== {} Case B off-vertex dump ({} entries) ===",
            case_id,
            case_b_dumps.len()
        );
        eprintln!(
            "  format: tri=qa,qb,qc off_vert_idx C_pos=(x,y,z) W_pos=(x,y,z) cell_dist=N"
        );
        // Stable order: sort by Cherchi off-vert quantized position.
        let mut sorted = case_b_dumps.clone();
        sorted.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        for (i, (tri, off, cpos, wpos, dist)) in sorted.iter().enumerate() {
            eprintln!(
                "  b[{}] tri={} off={} C_pos=({:+.6e},{:+.6e},{:+.6e}) \
                 W_pos=({:+.6e},{:+.6e},{:+.6e}) cell_dist={}",
                i,
                fmt_qtri(tri),
                off,
                cpos[0], cpos[1], cpos[2],
                wpos[0], wpos[1], wpos[2],
                dist,
            );
        }
    } else {
        eprintln!("\n=== {} Case B off-vertex dump: EMPTY ===", case_id);
    }

    // PR-Y44 δ: Case D sub-class distribution + per-tri 4-tuple table.
    // Sub-class (a) = (m1x=3, m5x=3)         ← topology-emission defect
    //                                          (paper anchors: Cherchi 2022 §5
    //                                          manifold-flood; Yang 2025 §4.4.1
    //                                          mesh-updating dup-retention)
    // Sub-class (b) = (m1x ∈ {0,1}, m5x=2)   ← partial-proximity residual
    //                                          (vertex-production mechanism;
    //                                          Case-B-adjacent)
    // Sub-class other = everything else inside the Case D residual catch-all.
    //
    // Per audit-y43 §3.2 + §4.1: the canary memo's "3-of-3 at 1× / triangle
    // missing" framing was an inference, not a measurement; the probe did
    // not distinguish sub-classes. δ measures the proportion before α/γ
    // anchor selection (`feedback_phase1_diagnosis_ranking_is_inference`).
    eprintln!(
        "\n=== {} Case D sub-class distribution ({} entries) ===",
        case_id,
        case_d_entries.len()
    );
    eprintln!(
        "  legend: (a) [m1x=3, m5x=3] — topology-emission (α/γ anchor)"
    );
    eprintln!(
        "          (b) [m1x ∈ {{0,1}}, m5x=2] — partial-proximity residual (Case-B-adjacent)"
    );
    eprintln!("          other — unexpected residual sub-class");
    let mut subclass_a = 0usize;
    let mut subclass_b = 0usize;
    let mut subclass_other = 0usize;
    for (_tri, tup) in &case_d_entries {
        let is_a = tup.match_at_1x == 3 && tup.match_at_5x == 3;
        let is_b = (tup.match_at_1x == 0 || tup.match_at_1x == 1)
            && tup.match_at_5x == 2;
        if is_a {
            subclass_a += 1;
        } else if is_b {
            subclass_b += 1;
        } else {
            subclass_other += 1;
        }
    }
    let dn = case_d_entries.len().max(1);
    let dpct = |c: usize| -> f64 { (c as f64) * 100.0 / (dn as f64) };
    eprintln!(
        "  subclass_a (m1x=3, m5x=3):              {} / {} = {:.1}%",
        subclass_a, case_d_entries.len(), dpct(subclass_a)
    );
    eprintln!(
        "  subclass_b (m1x ∈ {{0,1}}, m5x=2):       {} / {} = {:.1}%",
        subclass_b, case_d_entries.len(), dpct(subclass_b)
    );
    eprintln!(
        "  subclass_other:                          {} / {} = {:.1}%",
        subclass_other, case_d_entries.len(), dpct(subclass_other)
    );
    // Sum-equals-total check (audit invariant from PR-Y44 plan Gate 5).
    let subclass_sum = subclass_a + subclass_b + subclass_other;
    eprintln!(
        "  bucket-sum check: {} + {} + {} = {} (Case D total = {}) — {}",
        subclass_a,
        subclass_b,
        subclass_other,
        subclass_sum,
        case_d_entries.len(),
        if subclass_sum == case_d_entries.len() { "OK" } else { "MISMATCH" }
    );

    if !case_d_entries.is_empty() {
        eprintln!(
            "\n--- {} Case D per-tri 4-tuple table ---",
            case_id
        );
        eprintln!("  format: d[i] tri=qa,qb,qc (m1x=_, m2x=_, m5x=_, m10x=_) [subclass]");
        // Stable order: sort by quantized triangle key (same canonical key as
        // missing_sorted; inherits from line 1319 missing_sorted.sort()).
        let mut sorted = case_d_entries.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        for (i, (tri, tup)) in sorted.iter().enumerate() {
            let is_a = tup.match_at_1x == 3 && tup.match_at_5x == 3;
            let is_b = (tup.match_at_1x == 0 || tup.match_at_1x == 1)
                && tup.match_at_5x == 2;
            let tag = if is_a { "(a)" } else if is_b { "(b)" } else { "other" };
            eprintln!(
                "  d[{}] tri={} (m1x={}, m2x={}, m5x={}, m10x={}) {}",
                i,
                fmt_qtri(tri),
                tup.match_at_1x,
                tup.match_at_2x,
                tup.match_at_5x,
                tup.match_at_10x,
                tag,
            );
        }
    }

    eprintln!("=== end {} A/B/C/D classification ===\n", case_id);

    let case_d_tuples: Vec<CaseDSubclassTuple> =
        case_d_entries.iter().map(|(_, t)| *t).collect();

    Some(NearestAttributionResult {
        case_id: case_id.to_string(),
        target_tri_count: target_tris.len(),
        case_a,
        case_b,
        case_c,
        case_d,
        case_d_tuples,
    })
}

/// PR-Y43 LOAD-BEARING test: F0020 A/B/C/D histogram + Case B dump.
/// 12th investigational PR on F0020 Render LOD; drives PR-Y44 decision
/// per the plan §Phase 2 verdict logic.
#[test]
#[ignore]
fn f0020_render_lod_nearest_attribution() {
    let _ = run_nearest_attribution_for_case("F0020");
}

/// PR-Y43 cohort sanity: F0044/F0045/R0092 expected ≥95% Case C
/// (common=0 for all → target_tris is the entire 136/236/192 missing
/// list, which contains no positionally-matched Waffle verts → Case C
/// dominates by methodology). Confirms the probe is not a bug.
#[test]
#[ignore]
fn cohort_render_lod_nearest_attribution() {
    for case in &["F0044", "F0045", "R0092"] {
        let _ = run_nearest_attribution_for_case(case);
    }
}
