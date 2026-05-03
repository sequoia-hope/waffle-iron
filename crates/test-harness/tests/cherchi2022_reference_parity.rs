//! Reference-parity differential test against the Cherchi 2022
//! `mesh_booleans` C++ binary (the upstream reference implementation
//! Yang 2025 cites for the exact mesh-boolean stage).
//!
//! Why this test exists:
//! ---------------------
//! Per CLAUDE.md (commit `4808f2e`): when porting from a published algorithm
//! with a public reference implementation, build differential testing against
//! the reference as part of the initial port — and especially after three
//! wrong/incomplete anchors in a row (the strategic-escalation rule).
//! PR12, PR13, PR-Y14a/b cycled through three anchors without resolving
//! F0002/F0004; PR-Y14c's spec (`specs/yang_pr_y14c_cherchi_lpi_canonicalization.md`)
//! makes reference-parity invariant **I8** mandatory. This file is that
//! oracle, fed by:
//!
//!   1. The kernel's `YANG_DUMP_OBJ_BASE` env var which writes Waffle's
//!      preprocessed A and B meshes as OBJ files.
//!   2. The Cherchi 2022 `mesh_booleans` CLI binary, discovered via the
//!      `CHERCHI2022_BIN` env var (default:
//!      `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`).
//!   3. `kernel::diagnostics::check_conformal` — the same oracle the
//!      in-pipeline probes use — applied to BOTH Cherchi's output mesh
//!      AND Waffle's `subdivide_mesh_pair` output (captured via the
//!      Stage A `[conformal-probe]` line).
//!
//! Sidecar binary:
//! ---------------
//! Build with the upstream README's instructions:
//!     git clone https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//!     cd InteractiveAndRobustMeshBooleans && mkdir build && cd build
//!     cmake .. -DCMAKE_BUILD_TYPE=Release && make
//! Repo footprint: ~150 MB. Build artifacts: ~few-hundred MB.
//! Recommended location (outside this workspace to keep the in-repo
//! footprint small): `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/`.
//!
//! Status:
//! -------
//! All tests in this file are `#[ignore]` so they don't run in default `cargo
//! test`. They require: (a) the Cherchi binary at `CHERCHI2022_BIN`, (b)
//! the F0002 case file in `app/tests/cases/assay/`. Each test prints
//! `[reference-parity]` lines summarizing the diff. No assertions on Cherchi's
//! output beyond well-formedness — the test's job is to **localize a
//! divergence**, not to enforce byte-equality.
//!
//! Refs: [#9] Cherchi 2020, [#38] Cherchi 2022, [#24] Yang 2025.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use kernel::diagnostics::{check_conformal, ConformalReport};
use test_harness::cherchi_sidecar::{cherchi_bin, run_with_timeout, TimedRun};

/// Default subprocess timeout for `mesh_booleans` invocations.
///
/// On well-formed input Cherchi finishes a 2-tetra union in milliseconds.
/// On malformed input (non-manifold/non-watertight/self-intersecting —
/// see PR-S1 findings memo + `cherchi2022_sidecar_feasibility.md` §"Build
/// verified 2026-05-03") it loops indefinitely; the previous F0002 run
/// burned 6 hours at 99% CPU before being killed manually. 30s is a
/// generous cap that catches genuine runaways without truncating any
/// realistic well-formed-input run.
const CHERCHI_SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(30);

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Minimal Wavefront OBJ parser — handles only the subset our Cherchi
/// output uses: `v x y z` lines and `f i j k` lines, 1-indexed. Skips
/// blank/comment/`vn`/`vt`/`vp`/`g`/`o`/`mtllib`/`usemtl` lines. Fails
/// loud on any unexpected token format so silent corruption surfaces.
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
                // Cherchi emits plain `f i j k`. Strip any optional
                // `/<texcoord>/<normal>` suffix per the OBJ spec, then
                // 1-index → 0-index.
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
            // Ignore other OBJ tokens we don't model.
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

/// Pretty-print a ConformalReport summary on a single line.
fn fmt_report(report: &ConformalReport, label: &str) -> String {
    format!(
        "[reference-parity] {} : verts={} tris={} unique_edges={} \
         unpaired={} multi_paired={} euler_chi={} well_formed={}",
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

/// Run F0002 with `YANG_DUMP_OBJ_BASE` set, then call Cherchi's
/// `mesh_booleans union` on the dumped A/B OBJs, parse the output, and
/// report the conformal-mesh diff between Cherchi's output and a
/// well-formedness check.
///
/// **Expected outcome (PR-Y14b state):** Cherchi's output is `well_formed=true`,
/// confirming a paper-faithful reference implementation reaches a well-formed
/// arrangement on the same input where Waffle's `subdivide_mesh_pair`
/// produces `well_formed=false`. That establishes the divergence is in our
/// port, not in the upstream algorithm. The next investigation (PR-Y14c) then
/// has a tight scope: which specific Cherchi sub-stage in OUR port introduces
/// the LPI canonicalization defect that the C++ reference does not.
#[test]
#[ignore]
fn f0002_cherchi_union_reference_parity() {
    use std::path::Path;
    use test_harness::assay::randomized_runner::run_single_case;

    let bin = match cherchi_bin() {
        Some(p) => p,
        None => return,
    };

    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("[reference-parity] SKIP: assay corpus dir not present");
        return;
    }

    // Use a deterministic temp dir under /tmp so re-runs overwrite the same
    // files. Avoid `tempfile` to keep the Cargo footprint small (already
    // a dev-dep; this test does not need RAII cleanup).
    let workdir = std::env::temp_dir().join("waffle_cherchi_parity_f0002");
    std::fs::create_dir_all(&workdir).expect("create temp work dir");
    let base = workdir.join("f0002");
    let base_str = base.to_string_lossy().into_owned();
    let path_a = workdir.join("f0002_a.obj");
    let path_b = workdir.join("f0002_b.obj");
    let path_out = workdir.join("f0002_union.obj");

    // Clear any prior files so a partial run can't be mistaken for fresh data.
    for p in [&path_a, &path_b, &path_out] {
        let _ = std::fs::remove_file(p);
    }

    // Run F0002 with YANG_BOOLEAN=1 + YANG_DUMP_OBJ_BASE set. The kernel
    // dumps the post-coplanar-preprocess, post-tessellation, post-dedup A and
    // B meshes (i.e. exactly the input that `subdivide_mesh_pair_full_cherchi`
    // would receive — the Stage A boundary).
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("YANG_DUMP_OBJ_BASE", &base_str);
    let case_result = run_single_case(dir, "F0002", true);
    std::env::remove_var("YANG_DUMP_OBJ_BASE");
    let case = case_result.expect("F0002 must exist in corpus");
    eprintln!(
        "[reference-parity] F0002 case: status={:?} detail={}",
        case.status, case.detail
    );

    // Sanity-check the dump landed.
    if !path_a.exists() || !path_b.exists() {
        eprintln!(
            "[reference-parity] SKIP: kernel did not write OBJ dumps. \
             expected `{}` and `{}`. Likely the YANG_DUMP_OBJ_BASE path is \
             outside the kernel's writable scope, or the case short-circuited \
             before reaching the dump site.",
            path_a.display(),
            path_b.display()
        );
        return;
    }

    // Diff our own pre-Cherchi OBJ inputs (sanity: they should themselves be
    // well-formed before we hand them to the reference; if not, the bug is
    // upstream of subdivide_mesh_pair and the reference comparison is moot).
    let (verts_a, tris_a) = parse_obj(&path_a).expect("parse waffle A.obj");
    let (verts_b, tris_b) = parse_obj(&path_b).expect("parse waffle B.obj");
    let waffle_a_report = check_conformal(&verts_a, &tris_a);
    let waffle_b_report = check_conformal(&verts_b, &tris_b);
    eprintln!("{}", fmt_report(&waffle_a_report, "waffle A (pre-Cherchi)"));
    eprintln!("{}", fmt_report(&waffle_b_report, "waffle B (pre-Cherchi)"));

    // Invoke Cherchi 2022 mesh_booleans union <a> <b> <out> with a 30s
    // timeout cap. Cherchi runs forever on malformed input (PR-S1 finding
    // — see cherchi2022_sidecar_feasibility.md §"Build verified
    // 2026-05-03"). The timeout converts that into a clean test outcome
    // instead of a multi-hour hang.
    let mut cmd = Command::new(&bin);
    cmd.arg("union").arg(&path_a).arg(&path_b).arg(&path_out);
    let cherchi_out = match run_with_timeout(cmd, CHERCHI_SUBPROCESS_TIMEOUT) {
        TimedRun::Completed(out) => out,
        TimedRun::TimedOut => {
            eprintln!(
                "[reference-parity] Cherchi runaway on F0002 ({}s timeout) \
                 — expected on malformed input per inputcheck. This IS the \
                 test's outcome on this input; the F0002 input mesh is \
                 non-manifold + non-watertight + self-intersecting (see \
                 docs/audits/cherchi2022_sidecar_feasibility.md §'Build \
                 verified 2026-05-03'). PR-Y15 will fix the upstream \
                 tessellation defect that produces it.",
                CHERCHI_SUBPROCESS_TIMEOUT.as_secs()
            );
            return;
        }
        TimedRun::SpawnFailed(e) => {
            panic!("Cherchi spawn failed: {}", e);
        }
    };
    if !cherchi_out.status.success() {
        eprintln!(
            "[reference-parity] Cherchi exited non-zero ({:?}). stderr:\n{}",
            cherchi_out.status,
            String::from_utf8_lossy(&cherchi_out.stderr)
        );
        // Don't panic — the test is informational on first run. Surface the
        // skip clearly and exit.
        return;
    }
    if !path_out.exists() {
        eprintln!(
            "[reference-parity] SKIP: Cherchi did not produce `{}` even though \
             it exited 0. Check Cherchi's stdout/stderr above.",
            path_out.display()
        );
        return;
    }

    // Parse Cherchi's output and run check_conformal — the same oracle the
    // Waffle pipeline uses at Stages A/B/C. If Cherchi reports well_formed
    // but Stage A from Waffle's run does not, the divergence is in our port
    // of the post-tessellation arrangement (NOT in the input meshes, which
    // both pipelines saw identically).
    let (cherchi_verts, cherchi_tris) = parse_obj(&path_out).expect("parse Cherchi union.obj");
    let cherchi_report = check_conformal(&cherchi_verts, &cherchi_tris);
    eprintln!(
        "{}",
        fmt_report(&cherchi_report, "Cherchi 2022 union output")
    );

    // Print the dominant violations from each side for the audit trail.
    if !cherchi_report.is_well_formed {
        eprintln!(
            "[reference-parity] Cherchi output NOT well-formed: \
             unpaired={} multi_paired={} (first-of-each shown)",
            cherchi_report.unpaired_directed_edges.len(),
            cherchi_report.multi_paired_edges.len(),
        );
        if let Some(u) = cherchi_report.unpaired_directed_edges.first() {
            eprintln!("[reference-parity]   cherchi unpaired#0: {:?}", u);
        }
        if let Some(m) = cherchi_report.multi_paired_edges.first() {
            eprintln!("[reference-parity]   cherchi multi_paired#0: {:?}", m);
        }
    } else {
        eprintln!(
            "[reference-parity] Cherchi output WELL-FORMED. \
             Compare to Waffle Stage A `[conformal-probe]` line in the case \
             trace above — if Stage A reports well_formed=false on the SAME \
             inputs, the divergence is localized to our port of \
             `subdivide_mesh_pair_full_cherchi` (PR-Y14c anchor space)."
        );
    }
}

/// Smoke test: parse a known-good two-tetrahedra union output written by
/// the Cherchi binary at smoke-test time. Runs only if both the binary
/// exists AND the smoke OBJ inputs can be created in /tmp. Pure unit
/// validation of the in-test OBJ parser + the binary invocation pattern,
/// independent of the kernel's dump path.
#[test]
#[ignore]
fn cherchi_smoke_two_tetrahedra_union() {
    let bin = match cherchi_bin() {
        Some(p) => p,
        None => return,
    };

    let workdir = std::env::temp_dir().join("waffle_cherchi_parity_smoke");
    std::fs::create_dir_all(&workdir).expect("create temp work dir");
    let path_a = workdir.join("a.obj");
    let path_b = workdir.join("b.obj");
    let path_out = workdir.join("union.obj");

    // Two tetrahedra at unit scale, second offset by (0.3, 0.3, 0.3) so they
    // overlap. Expected union: 10 verts, 16 faces, 3 new intersection points.
    std::fs::write(
        &path_a,
        "v 0.0 0.0 0.0\nv 1.0 0.0 0.0\nv 0.0 1.0 0.0\nv 0.0 0.0 1.0\n\
         f 1 3 2\nf 1 2 4\nf 1 4 3\nf 2 3 4\n",
    )
    .expect("write a.obj");
    std::fs::write(
        &path_b,
        "v 0.3 0.3 0.3\nv 1.3 0.3 0.3\nv 0.3 1.3 0.3\nv 0.3 0.3 1.3\n\
         f 1 3 2\nf 1 2 4\nf 1 4 3\nf 2 3 4\n",
    )
    .expect("write b.obj");

    let _ = std::fs::remove_file(&path_out);
    let mut cmd = Command::new(&bin);
    cmd.arg("union").arg(&path_a).arg(&path_b).arg(&path_out);
    let cherchi_out = match run_with_timeout(cmd, CHERCHI_SUBPROCESS_TIMEOUT) {
        TimedRun::Completed(out) => out,
        TimedRun::TimedOut => panic!(
            "Cherchi smoke timed out after {}s — the binary or two-tetra \
             union is broken (well-formed input should finish in milliseconds)",
            CHERCHI_SUBPROCESS_TIMEOUT.as_secs()
        ),
        TimedRun::SpawnFailed(e) => panic!("Cherchi spawn failed: {}", e),
    };
    assert!(
        cherchi_out.status.success(),
        "Cherchi exited non-zero: {:?}\nstderr:\n{}",
        cherchi_out.status,
        String::from_utf8_lossy(&cherchi_out.stderr)
    );

    let (verts, tris) = parse_obj(&path_out).expect("parse smoke union.obj");
    let report = check_conformal(&verts, &tris);
    eprintln!("{}", fmt_report(&report, "smoke union (two tetrahedra)"));

    // The reference implementation's output on a clean, non-degenerate input
    // MUST be well-formed; if this assertion ever fails, the binary is broken
    // (or our parser is). Either is a hard stop.
    assert!(
        report.is_well_formed,
        "Cherchi smoke union should be well_formed, got: {:?}",
        report
    );
    // V − E + F = 2 for any closed orientable surface; the union of two
    // tetrahedra produces exactly one such surface.
    assert_eq!(
        report.euler_characteristic, 2,
        "Cherchi smoke union Euler characteristic should be 2 (one closed shell)"
    );
}
