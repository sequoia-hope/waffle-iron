//! The EXACT-membership oracle on the corpus (`test_harness::assay::
//! exact_membership`): the composed solid of a `.waffle` document as a
//! closed-form point predicate — no tessellation, no kernel — read on a
//! resolution ladder.
//!
//! Two kinds of test live here:
//!
//! - **Pinned adjudications** (always on, seconds): every corpus
//!   `euler_target` that was ever hand-corrected after an adjudication
//!   (R0099, R0091, R0063, R0011, C0075, R0053, R0044 — see
//!   `assay_euler_consistency::historical_authoring_fixes_pinned`) is
//!   re-derived here from the authored numbers alone. The instrument is
//!   validated by the cases that were adjudicated by other means, and each
//!   adjudication is now reproducible by one test instead of a session.
//! - **The corpus sweep** (`--ignored`, minutes): every covered case on a
//!   three-rung ladder, its boundary χ against the authored `euler_target`,
//!   its component count against `expected_shell_count`, and — with
//!   `EXACT_KERNEL=1` — its exact volume against the kernel's tessellated
//!   result (the runner's own scan), printed as a table.
//!
//! ```text
//! cargo test -p test-harness --release --test assay_exact_membership
//! EXACT_CELLS=64,128,256 EXACT_KERNEL=1 cargo test -p test-harness --release \
//!   --test assay_exact_membership -- --ignored --nocapture corpus_sweep
//! ASSAY_CASE=R0053 EXACT_CELLS=128,256,512 EXACT_PHASE=0.5,0.25 cargo test -p test-harness \
//!   --release --test assay_exact_membership -- --ignored --nocapture one_case_ladder
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use test_harness::assay::exact_membership::{readout_exact, ExactChain, ExactReadout};

const CORPUS: &str = "../../app/tests/cases/assay";

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS)
}

fn read_case(id: &str) -> (serde_json::Value, serde_json::Value) {
    let d = corpus_dir();
    let waffle =
        serde_json::from_str(&fs::read_to_string(d.join(format!("{id}.waffle"))).unwrap()).unwrap();
    let meta =
        serde_json::from_str(&fs::read_to_string(d.join(format!("{id}.meta.json"))).unwrap())
            .unwrap();
    (waffle, meta)
}

fn chain(id: &str) -> ExactChain {
    let (waffle, _) = read_case(id);
    ExactChain::from_waffle(&waffle).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn ladder(chain: &ExactChain, cells: &[usize], phase: f64) -> Vec<ExactReadout> {
    cells
        .iter()
        .map(|&c| readout_exact(chain, chain.ops.len(), c, phase).expect("bbox"))
        .collect()
}

/// Every rung must read the same `(boundary χ, components)`.
fn assert_stable(id: &str, rungs: &[ExactReadout], expect_chi: i64, expect_components: usize) {
    for r in rungs {
        eprintln!(
            "[exact] {id} cells={:?} h={:.4e} chi_solid={} boundary_chi={} components={} volume={:.6e}",
            r.n, r.h, r.readout.chi, r.boundary_chi(), r.readout.components, r.volume
        );
    }
    let bad: Vec<_> = rungs
        .iter()
        .filter(|r| r.boundary_chi() != expect_chi || r.readout.components != expect_components)
        .collect();
    assert!(
        bad.is_empty(),
        "{id}: expected boundary χ {expect_chi} with {expect_components} component(s) at every rung; got {:?}",
        rungs
            .iter()
            .map(|r| (r.boundary_chi(), r.readout.components))
            .collect::<Vec<_>>()
    );
}

fn cells_env(default: &[usize]) -> Vec<usize> {
    std::env::var("EXACT_CELLS")
        .ok()
        .map(|s| s.split(',').filter_map(|t| t.trim().parse().ok()).collect())
        .filter(|v: &Vec<usize>| !v.is_empty())
        .unwrap_or_else(|| default.to_vec())
}

fn phases_env() -> Vec<f64> {
    std::env::var("EXACT_PHASE")
        .ok()
        .map(|s| s.split(',').filter_map(|t| t.trim().parse().ok()).collect())
        .filter(|v: &Vec<f64>| !v.is_empty())
        .unwrap_or_else(|| vec![0.5])
}

// ---- pinned adjudications ------------------------------------------------
//
// SCOPE of the cubical ladder (measured 2026-09-03, spec `yang_451_corner_
// transit.md` §3ah): a reading is the solid's topology once every rung reads
// alike at cell sizes below the solid's thinnest feature and its narrowest
// gap. Features that TAPER to zero thickness — a tangent graze's contact
// boundary is fine (a curve), but a crescent pillar or a knife edge is a
// sheet the lattice perforates at every cell size — cannot be pinned this
// way (R0091's four corner pillars: genus reads 1 / 5 / 1 at 256 / 512 /
// 1024 cells while the volume holds at 4.2624e-12; its authored genus 3 came
// from an analytic patch count and the sidecar, `assay_euler_consistency`).
// The heavy rungs below are `#[ignore]`d instruments, not gates.

/// F0001: two overlapping axis-aligned boxes — one ball.
#[test]
fn f0001_two_boxes_read_one_ball() {
    let c = chain("F0001");
    assert_stable("F0001", &ladder(&c, &[48, 96], 0.5), 2, 1);
}

/// R0053 (spec `yang_451_corner_transit.md` §3ah): rectangle revolve ∪ box
/// ∪ gear revolve about a parallel axis — genus 1 (the box bridges the
/// C-ring; the gear adds no handle). Stable from cell size ≈ 1.0 down
/// (§3ah's ladder to 0.4 on two phases); at ≈ 2 a two-unit feature aliases.
#[test]
fn r0053_reads_genus_one() {
    let c = chain("R0053");
    assert_eq!(c.ops.len(), 3);
    assert_stable("R0053", &ladder(&c, &[320, 448], 0.5), 0, 1);
    assert_stable("R0053 phase ¼", &ladder(&c, &[320], 0.25), 0, 1);
    // The two-op prefix is already genus 1 (§3af's op-1 reading).
    let r = readout_exact(&c, 2, 320, 0.5).unwrap();
    assert_eq!((r.boundary_chi(), r.readout.components), (0, 1), "{r:?}");
}

/// R0011 (spec §3j, corrected 2026-08-31): a 14-tooth gear prism grazing a
/// 295.56° rectangle-revolve band in two adjacent-tooth patches — genus 1.
/// Below ≈ 35 units per cell the neighbouring teeth's clearance resolves and
/// the reading is stable (0 at 512 and 1024 cells); coarser rungs bridge
/// the near-contact teeth and read extra handles.
#[test]
fn r0011_reads_genus_one() {
    let c = chain("R0011");
    assert_stable("R0011", &ladder(&c, &[512], 0.5), 0, 1);
}

/// R0011's finer rung (375 M cubes).
#[test]
#[ignore = "heavy pin (≈ 40 s in release)"]
fn r0011_reads_genus_one_at_1024() {
    let c = chain("R0011");
    assert_stable("R0011", &ladder(&c, &[1024], 0.5), 0, 1);
}

/// R0044 (corrected 2026-09-05): a 195.8° rectangle revolve ∪ a 304.6° gear
/// revolve about another axis, then a 695-deep circle cut (r ≈ 451) —
/// genus 1: the cut bores THROUGH the union. Read 0 / 1 component at 128,
/// 256 and 512 cells on two lattice phases (cell 107 → 27 units on a
/// ≈ 8000-unit solid; volume 1.0201e11 ± 0.2 %). The authored
/// `euler_target: 2` was the generator's guess; the kernel's χ = 0 output
/// (the day the thin-band chart guard let its 70 conical bands tessellate)
/// is the true topology, not a WRONG.
#[test]
fn r0044_reads_genus_one() {
    let c = chain("R0044");
    assert_eq!(c.ops.len(), 3);
    assert_stable("R0044", &ladder(&c, &[128, 256, 512], 0.5), 0, 1);
    assert_stable("R0044 phase ¼", &ladder(&c, &[256], 0.25), 0, 1);
}

/// R0099 (fix 74564242): circle boss + circle through-cut + rectangle
/// revolve cut — genus 1.
#[test]
fn r0099_reads_genus_one() {
    let c = chain("R0099");
    assert_stable("R0099", &ladder(&c, &[96, 160, 256], 0.5), 0, 1);
}

/// R0063 (task #195, corrected 2026-07-22): the concentric prism stack —
/// one cycle through two crescents, genus 1. The gear top sits 5.64e-6
/// below the cut floor: at cell sizes above that gap the lattice speckles
/// the slab into thousands of cavities (χ_solid 295 / 741 / 1076 / −2917 /
/// −3508 at 96…768 cells); at 1024 cells (h = 2.83e-6) it reads 0.
#[test]
#[ignore = "heavy pin (610 M cubes, ≈ 60 s in release)"]
fn r0063_reads_genus_one_below_its_gap() {
    let c = chain("R0063");
    assert_stable("R0063", &ladder(&c, &[1024], 0.5), 0, 1);
}

/// R0091 (task #186): the exact VOLUME is what the ladder can pin here —
/// the sausage (3.966e-12 by Pappus) plus the cut box; its topology is out of
/// the lattice's scope (tapered pillars, see the scope note above). This
/// case also exposed a kernel silent-wrong: the completed result's revolve
/// body comes back as the COMPLEMENTARY 140.6° wedge of its authored 219.4°
/// sweep (2.541e-12 instead of 3.966e-12) after the cut — the boolean-output
/// torus band patch takes the principal-branch longitude interval between
/// its two rims (yang-rs `tessellate_torus_patch`, band case), which is the
/// wrong side whenever the band spans more than 180° (FIXED 2026-09-03).
///
/// The pinned total was 4.2624e-12 until 2026-09-04: that reading decided
/// the cut's auto-reversal on the merged (sausage ∪ box) extent, where the
/// engine decides on its FIRST combine target alone — here the box, since
/// the merge left the two bodies disjoint (`FE_CUT_TRACE=1`: `target
/// verts=8 … reverse=true`). Mirroring that rule reads 4.2232e-12 (box
/// remains 2.586e-13 + sausage 3.9646e-12), 0.1 % from the kernel's
/// 4.2190e-12.
#[test]
fn r0091_exact_volume_is_the_sausage_plus_the_cut_box() {
    let c = chain("R0091");
    let r = readout_exact(&c, 3, 256, 0.5).unwrap();
    assert_eq!(r.bodies, 2, "the box and the sausage stay separate bodies");
    let expected = 4.2232e-12;
    assert!(
        ((r.volume - expected) / expected).abs() < 2e-3,
        "R0091 exact volume {:.6e} vs {expected:.4e}",
        r.volume
    );
    // The revolve alone: Pappus π r² R θ with r = 6.0347e-5, R = 9.0521e-5,
    // θ = 219.43°.
    let sausage = readout_exact(&c, 1, 256, 0.5).unwrap();
    let pappus = std::f64::consts::PI
        * 6.034705044252099e-05_f64.powi(2)
        * 9.0521e-5
        * 219.42928010836604_f64.to_radians();
    assert!(
        ((sausage.volume - pappus) / pappus).abs() < 3e-3,
        "sausage {:.6e} vs Pappus {pappus:.6e}",
        sausage.volume
    );
}

/// C0075 (corrected 2026-08-19): two interleaving 12-tooth gears enclose
/// exactly two pockets — genus 2, boundary χ = −2.
#[test]
fn c0075_reads_genus_two() {
    let c = chain("C0075");
    assert_stable("C0075", &ladder(&c, &[128, 192, 256], 0.5), -2, 1);
}

/// C0065 (corrected 2026-09-04, ERROR case): a full torus (R = 1.2,
/// r = 0.3) with a 0.5-wide through-block centred on the tube — the block
/// spans radius 0.95…1.45 of a 0.9…1.5 tube, so it WINDOWS the tube and
/// leaves an inner and an outer bridge: genus 2, boundary χ = −2, one
/// component (the authoring said "severs the ring", χ = 2). Stable at
/// 128 / 256 / 512 cells on two lattice phases.
#[test]
fn c0065_reads_genus_two() {
    let c = chain("C0065");
    assert_stable("C0065", &ladder(&c, &[128, 256, 512], 0.5), -2, 1);
    assert_stable("C0065 phase ¼", &ladder(&c, &[256], 0.25), -2, 1);
}

/// R0026 (corrected 2026-09-04, ERROR case): circle boss ∪ circle revolve ∪
/// box — genus 1, one component, at 128 / 256 / 512 cells on two phases
/// (the generator's default χ = 2 was never adjudicated).
#[test]
fn r0026_reads_genus_one() {
    let c = chain("R0026");
    assert_stable("R0026", &ladder(&c, &[128, 256, 512], 0.5), 0, 1);
    assert_stable("R0026 phase ¼", &ladder(&c, &[256], 0.25), 0, 1);
}

// ---- instruments -----------------------------------------------------------

/// One case on a ladder (`ASSAY_CASE`, `EXACT_CELLS`, `EXACT_PHASE`,
/// `EXACT_PREFIX=k` for the first `k` ops).
#[test]
#[ignore = "manual instrument"]
fn one_case_ladder() {
    let id = std::env::var("ASSAY_CASE").unwrap_or_else(|_| "R0053".into());
    let (waffle, meta) = read_case(&id);
    let c = match ExactChain::from_waffle(&waffle) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[exact] {id}: NOT COVERED — {e}");
            return;
        }
    };
    for n in &c.notes {
        eprintln!("[exact] {id} note: {n}");
    }
    // `EXACT_ONLY=k`: read operand k on its own, as a boss (its tool shape).
    let c = match std::env::var("EXACT_ONLY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    {
        Some(k) => {
            let only = c.operand_alone(k);
            eprintln!("[exact] {id}: operand {k} alone ({})", only.ops[0].name);
            only
        }
        None => c,
    };
    let prefix = std::env::var("EXACT_PREFIX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(c.ops.len());
    let target = meta
        .pointer("/oracles/euler_target")
        .and_then(|v| v.as_i64());
    if std::env::var_os("EXACT_KERNEL").is_some() {
        match kernel_volume(&waffle, &meta) {
            Some(Ok(v)) => eprintln!(
                "[exact] {id} kernel volume (tessellated at the oracle tol, scan 256) = {v:.6e}"
            ),
            Some(Err(e)) => eprintln!("[exact] {id} kernel volume: {e}"),
            None => eprintln!("[exact] {id} kernel volume: no scale in meta"),
        }
    }
    for phase in phases_env() {
        for cells in cells_env(&[64, 128, 256]) {
            let r = readout_exact(&c, prefix, cells, phase).expect("bbox");
            eprintln!(
                "[exact] {id} ops=0..{prefix} cells={cells} phase={phase} n={:?} h={:.4e} chi_solid={} boundary_chi={} components={} sizes={:?} volume={:.6e} bodies={} body_volumes={:?} (authored euler_target {:?})",
                r.n, r.h, r.readout.chi, r.boundary_chi(), r.readout.components,
                r.component_sizes.iter().take(8).collect::<Vec<_>>(), r.volume,
                r.bodies, r.body_volumes.iter().map(|v| format!("{v:.4e}")).collect::<Vec<_>>(), target
            );
            eprintln!(
                "[exact] {id}   centroid=({:.6e}, {:.6e}, {:.6e})",
                r.centroid[0], r.centroid[1], r.centroid[2]
            );
        }
    }
}

/// Every covered corpus case on a three-rung ladder against its authored
/// oracles; `EXACT_KERNEL=1` adds the kernel's tessellated volume.
#[test]
#[ignore = "manual instrument: the whole corpus (minutes; with EXACT_KERNEL=1 the kernel runs too)"]
fn corpus_sweep() {
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(corpus_dir().join("manifest.json")).expect("manifest"),
    )
    .unwrap();
    let ids: Vec<String> = manifest
        .pointer("/cases")
        .and_then(|v| v.as_array())
        .expect("manifest cases")
        .iter()
        .filter_map(|c| c.get("id").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    let cells = cells_env(&[64, 128, 256]);
    let with_kernel = std::env::var_os("EXACT_KERNEL").is_some();
    let mut covered = 0usize;
    let mut not_covered = Vec::new();
    let mut unstable = Vec::new();
    let mut chi_disagree = Vec::new();
    let mut shells_disagree = Vec::new();
    let mut kernel_rows = Vec::new();
    for id in &ids {
        let (waffle, meta) = read_case(id);
        let c = match ExactChain::from_waffle(&waffle) {
            Ok(c) => c,
            Err(e) => {
                not_covered.push(format!("{id}: {e}"));
                continue;
            }
        };
        covered += 1;
        let rungs: Vec<ExactReadout> = cells
            .iter()
            .filter_map(|&n| readout_exact(&c, c.ops.len(), n, 0.5))
            .collect();
        let target = meta
            .pointer("/oracles/euler_target")
            .and_then(|v| v.as_i64());
        let shells = meta
            .pointer("/oracles/expected_shell_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;
        let stable = rungs.windows(2).all(|w| {
            w[0].readout.chi == w[1].readout.chi
                && w[0].readout.components == w[1].readout.components
        });
        let last = rungs.last().expect("a rung");
        let kernel = if with_kernel {
            kernel_volume(&waffle, &meta)
        } else {
            None
        };
        let kcol = match kernel {
            Some(Ok(v)) => format!(
                " kernel_vol={:.6e} rel={:+.3e} verdict={:?}",
                v,
                (v - last.volume) / last.volume.abs().max(f64::MIN_POSITIVE),
                test_harness::assay::exact_membership::exact_volume_verdict(&c, v)
            ),
            Some(Err(ref e)) => format!(" kernel: {e}"),
            None => String::new(),
        };
        eprintln!(
            "[exact] {id} ops={} bodies={} chi_boundary={:?}{} components={:?} volume={:.6e} target={:?} shells={shells}{kcol}{}",
            c.ops.len(),
            last.bodies,
            rungs.iter().map(|r| r.boundary_chi()).collect::<Vec<_>>(),
            if stable { "" } else { " UNSTABLE" },
            rungs.iter().map(|r| r.readout.components).collect::<Vec<_>>(),
            last.volume,
            target,
            if c.notes.is_empty() { String::new() } else { format!(" notes={:?}", c.notes) }
        );
        if !stable {
            unstable.push(id.clone());
            continue;
        }
        if let Some(t) = target {
            if last.boundary_chi() != t {
                chi_disagree.push(format!(
                    "{id}: exact {} vs authored {t}",
                    last.boundary_chi()
                ));
            }
        }
        if last.readout.components != shells {
            shells_disagree.push(format!(
                "{id}: exact {} vs authored {shells}",
                last.readout.components
            ));
        }
        if let Some(Ok(v)) = kernel {
            kernel_rows.push((
                id.clone(),
                (v - last.volume) / last.volume.abs().max(f64::MIN_POSITIVE),
            ));
        }
    }
    eprintln!(
        "[exact] covered {covered}/{}; not covered {}:",
        ids.len(),
        not_covered.len()
    );
    for n in &not_covered {
        eprintln!("[exact]   {n}");
    }
    eprintln!(
        "[exact] UNSTABLE ladders ({}): {unstable:?}",
        unstable.len()
    );
    eprintln!(
        "[exact] boundary χ disagreements with the authored euler_target ({}):",
        chi_disagree.len()
    );
    for d in &chi_disagree {
        eprintln!("[exact]   {d}");
    }
    eprintln!(
        "[exact] component-count disagreements ({}):",
        shells_disagree.len()
    );
    for d in &shells_disagree {
        eprintln!("[exact]   {d}");
    }
    if with_kernel {
        kernel_rows.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap());
        eprintln!(
            "[exact] kernel volume vs exact, worst 25 of {}:",
            kernel_rows.len()
        );
        for (id, rel) in kernel_rows.iter().take(25) {
            eprintln!("[exact]   {id} rel={rel:+.3e}");
        }
    }
}

/// The kernel's completed result, tessellated at the oracle tolerance and
/// scanned by the volume oracle's own route; `Err` names why there is none
/// (an engine error, or an auto-union failure that left a standalone body —
/// `output_scan` alone would merge that body into the "result").
fn kernel_volume(
    waffle: &serde_json::Value,
    meta: &serde_json::Value,
) -> Option<Result<f64, String>> {
    use test_harness::assay::volume_oracle::{scan_volume, SolidScan};
    use test_harness::assay::volume_oracle_doc::oracle_tol;
    use test_harness::workflow::ModelBuilder;
    let scale = meta.get("scale").and_then(|v| v.as_f64())?;
    let tol = oracle_tol(scale);
    let json = serde_json::to_string(waffle).ok()?;
    let mut b = ModelBuilder::kernel_v2();
    if b.load(&json).is_err() || !b.engine_errors().is_empty() {
        return Some(Err("engine error".into()));
    }
    if b.engine_warnings()
        .iter()
        .any(|w| w.contains("Auto-union failed"))
    {
        return Some(Err("auto-union failure (standalone body)".into()));
    }
    let meshes = match b.tessellate_live_with_tol(tol) {
        Ok(m) => m,
        Err(e) => return Some(Err(format!("tessellation: {e}"))),
    };
    // One scan per live body, summed — the exact readout is per body too
    // (overlapping independent bodies count twice; a soup scan would read
    // their set union).
    if meshes.is_empty() {
        return Some(Err("no mesh".into()));
    }
    let mut total = 0.0;
    for m in &meshes {
        match SolidScan::from_render_mesh(m) {
            Some(scan) => total += scan_volume(&scan, 256).volume,
            None => return Some(Err("empty scan".into())),
        }
    }
    Some(Ok(total))
}
