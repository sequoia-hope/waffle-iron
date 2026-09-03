//! The independent TOPOLOGY oracle on a corpus case — the genus of the SET
//! UNION of the case's isolated operand solids, read on a resolution ladder
//! (`test_harness::assay::topology_oracle`).
//!
//! This is the adjudication instrument for an authored `euler_target` that
//! the kernel's output contradicts: when the ladder is stable and agrees
//! with the kernel, the authoring is what was wrong (the R0011 precedent,
//! genus 1, `euler_target = 0`); when the ladder is stable and disagrees,
//! the kernel's output is a silent WRONG that the mesh oracle caught.
//!
//! Run on one case:
//! ```text
//! ASSAY_CASE=R0053 TOPO_GRIDS=64,128,256 \
//!   cargo test -p test-harness --test assay_topology_oracle --release \
//!     -- --ignored --nocapture adjudicate_case
//! ```
//! `TOPO_OUTPUT=1` also reads the kernel's own output through the SAME
//! voxel walk (gate env vars apply — the kernel is live), so the two
//! readouts are comparable like for like. `TOPO_KEEP_OPS=k` truncates the
//! document to its first `k` operations (a chain prefix, e.g. R0044's union
//! without its cut). `TOPO_PHASE=0.25,0.75` shifts the sampling lattice.
//! `TOPO_SIDECAR=1` additionally unions the operand tessellations through the
//! Cherchi 2022 sidecar (the reference mesh boolean) and reads `V − E + F`
//! and the shell count off ITS result — the reference topology, independent
//! of both the kernel's boolean and the lattice.

use std::fs;
use std::path::{Path, PathBuf};

use test_harness::assay::topology_oracle::{readout_at, TopologyReadout};
use test_harness::assay::volume_oracle::SolidScan;
use test_harness::assay::volume_oracle_doc::{
    isolate_operation, operand_scan, oracle_tol, output_scan, truncate_ops,
};
use test_harness::cherchi_sidecar::{read_obj, sidecar_boolean, surface_topology, write_obj};
use test_harness::oracle;
use test_harness::workflow::ModelBuilder;

const CORPUS: &str = "../../app/tests/cases/assay";

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS)
}

fn read_case(id: &str) -> Option<(serde_json::Value, Vec<bool>, f64)> {
    let d = corpus_dir();
    let waffle: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.join(format!("{id}.waffle"))).ok()?).ok()?;
    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.join(format!("{id}.meta.json"))).ok()?).ok()?;
    let cuts = meta
        .get("operations")?
        .as_array()?
        .iter()
        .map(|o| o.get("is_cut").and_then(serde_json::Value::as_bool) == Some(true))
        .collect();
    let scale = meta.get("scale").and_then(serde_json::Value::as_f64)?;
    Some((waffle, cuts, scale))
}

fn grids() -> Vec<usize> {
    std::env::var("TOPO_GRIDS")
        .ok()
        .map(|s| s.split(',').filter_map(|t| t.trim().parse().ok()).collect())
        .filter(|v: &Vec<usize>| !v.is_empty())
        .unwrap_or_else(|| vec![64, 128, 256])
}

fn phases() -> Vec<f64> {
    std::env::var("TOPO_PHASE")
        .ok()
        .map(|s| s.split(',').filter_map(|t| t.trim().parse().ok()).collect())
        .filter(|v: &Vec<f64>| !v.is_empty())
        .unwrap_or_else(|| vec![0.5])
}

fn print_ladder(label: &str, scans: &[&SolidScan]) -> Vec<TopologyReadout> {
    let mut out = Vec::new();
    for n in grids() {
        for phase in phases() {
            match readout_at(scans, n, phase) {
                Some(r) => {
                    eprintln!(
                        "[topo] {label} n={} phase={phase} cubes={} chi={} components={} boundary_chi={}",
                        r.n,
                        r.cubes,
                        r.chi,
                        r.components,
                        r.boundary_chi()
                    );
                    out.push(r);
                }
                None => eprintln!("[topo] {label} n={n} phase={phase}: nothing to scan"),
            }
        }
    }
    out
}

/// Adjudicate `ASSAY_CASE` (default R0053): the composed operands' genus on
/// the ladder, and optionally the kernel output's.
#[test]
#[ignore = "manual instrument: builds the case's operand solids through kernel-v2"]
fn adjudicate_case() {
    let id = std::env::var("ASSAY_CASE").unwrap_or_else(|_| "R0053".into());
    let (mut waffle, mut cuts, scale) = read_case(&id).expect("readable corpus case");
    if let Some(k) = std::env::var("TOPO_KEEP_OPS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    {
        waffle = truncate_ops(&waffle, k).expect("corpus document shape");
        cuts.truncate(k);
        eprintln!("[topo] {id}: truncated to the first {k} op(s)");
    }
    assert!(
        !cuts.iter().any(|&c| c),
        "{id}: has a cut op — the tool is not re-authored (not covered)"
    );
    let tol = oracle_tol(scale);
    let mut scans = Vec::new();
    for k in 0..cuts.len() {
        let s = operand_scan(&waffle, k, tol)
            .unwrap_or_else(|| panic!("{id}: operand {k} failed to build"));
        eprintln!("[topo] {id} operand {k}: tris={}", s.tri_count());
        scans.push(s);
    }
    let refs: Vec<&SolidScan> = scans.iter().collect();
    let composed = print_ladder(&format!("{id} composed"), &refs);
    let stable = composed
        .windows(2)
        .all(|w| w[0].chi == w[1].chi && w[0].components == w[1].components);
    eprintln!(
        "[topo] {id} composed ladder {}: chi={:?} components={:?}",
        if stable { "STABLE" } else { "UNSTABLE" },
        composed.iter().map(|r| r.chi).collect::<Vec<_>>(),
        composed.iter().map(|r| r.components).collect::<Vec<_>>()
    );
    if std::env::var_os("TOPO_SIDECAR").is_some() {
        reference_topology(&id, &waffle, cuts.len(), tol);
    }
    if std::env::var_os("TOPO_OUTPUT").is_some() {
        match output_scan(&waffle, tol) {
            Some(out) => {
                print_ladder(&format!("{id} output"), &[&out]);
            }
            None => eprintln!("[topo] {id} output: build failed (engine error)"),
        }
    }
}

/// The reference topology: each operand rebuilt in isolation, tessellated at
/// the oracle tolerance, unioned through the Cherchi sidecar; `V − E + F`
/// and shells of the result. Also the kernel's own output, read by the
/// runner's Euler oracle, for the like-for-like comparison.
fn reference_topology(id: &str, waffle: &serde_json::Value, ops: usize, tol: f64) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/topo_sidecar")
        .join(id);
    fs::create_dir_all(&dir).expect("scratch dir");
    let mut inputs = Vec::new();
    for k in 0..ops {
        let json = isolate_operation(waffle, k).expect("isolated operand document");
        let mut b = ModelBuilder::kernel_v2();
        b.load(&json).expect("operand loads");
        assert!(
            b.engine_errors().is_empty(),
            "operand {k}: {:?}",
            b.engine_errors()
        );
        let mesh = b
            .tessellate_last_with_tol(tol)
            .expect("operand tessellates");
        let t = surface_topology(
            &mesh
                .vertices
                .chunks(3)
                .map(|v| [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])])
                .collect::<Vec<_>>(),
            &mesh
                .indices
                .chunks(3)
                .map(|t| [t[0], t[1], t[2]])
                .collect::<Vec<_>>(),
        );
        eprintln!("[topo] {id} operand {k} tessellation: {t:?}");
        let p = dir.join(format!("op{k}.obj"));
        write_obj(&p, &mesh).expect("write operand obj");
        inputs.push(p);
    }
    // Chain PAIRWISE in operation order — the kernel's own evaluation order
    // (each op auto-unions with the accumulated body), and the sidecar's
    // N-way form refuses a fully implicit (coplanar-graze) patch on R0053.
    let mut acc = inputs[0].clone();
    for (k, next) in inputs.iter().enumerate().skip(1) {
        let out = dir.join(format!("union_0_{k}.obj"));
        let _ = fs::remove_file(&out);
        match sidecar_boolean(
            "union",
            &[acc.clone(), next.clone()],
            &out,
            std::time::Duration::from_secs(600),
        ) {
            Ok(()) => {
                let (v, t) = read_obj(&out).expect("read sidecar union");
                let s = surface_topology(&v, &t);
                eprintln!(
                    "[topo] {id} REFERENCE union ops 0..={k} (sidecar, chained): {s:?} \
                     genus_if_one_shell={:?}",
                    s.genus_if_one_shell()
                );
                acc = out;
            }
            Err(e) => {
                eprintln!("[topo] {id} REFERENCE union ops 0..={k}: sidecar failed — {e}");
                return;
            }
        }
    }
    // The kernel's own output through the runner's Euler oracle.
    let json = serde_json::to_string(waffle).unwrap();
    let mut b = ModelBuilder::kernel_v2();
    b.load(&json).expect("load");
    let failures: Vec<String> = b
        .engine_warnings()
        .iter()
        .filter(|w| w.contains("Auto-union failed"))
        .cloned()
        .collect();
    if !b.engine_errors().is_empty() || !failures.is_empty() {
        eprintln!(
            "[topo] {id} KERNEL output: no result (errors={:?} union_failures={failures:?})",
            b.engine_errors()
        );
        return;
    }
    match b.tessellate_last_with_tol(tol) {
        Ok(mesh) => {
            let v = oracle::check_mesh_euler_characteristic_with_shells(&mesh, 2, None);
            eprintln!(
                "[topo] {id} KERNEL output (runner's Euler oracle vs target 2): {}",
                v.detail
            );
        }
        Err(e) => eprintln!("[topo] {id} KERNEL output: tessellation failed — {e}"),
    }
}

/// The instrument on a shape whose topology is closed-form: the corpus's
/// all-box F cases are single genus-0 shells (two overlapping axis-aligned
/// boxes), so the ladder must read `χ = 1`, one component, at every rung.
#[test]
#[ignore = "builds corpus operand solids through kernel-v2 (seconds)"]
fn box_union_reads_one_ball() {
    let id = "F0001";
    let (waffle, cuts, scale) = read_case(id).expect("readable corpus case");
    let tol = oracle_tol(scale);
    let scans: Vec<SolidScan> = (0..cuts.len())
        .map(|k| operand_scan(&waffle, k, tol).expect("operand builds"))
        .collect();
    let refs: Vec<&SolidScan> = scans.iter().collect();
    for n in [16, 32, 64] {
        let r = readout_at(&refs, n, 0.5).expect("scan");
        assert_eq!((r.chi, r.components), (1, 1), "{id} at n={n}: {r:?}");
    }
}
