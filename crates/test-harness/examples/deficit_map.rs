//! Spatial map of a volume-composition deficit (the R0057/R0059 anchor tool).
//!
//! For one assay case: rebuild the isolated operand scans and the in-context
//! output scan (same machinery as the in-line composition check), then
//!
//! 1. localize the deficit per op — composed(ops 0..=k) vs the in-context
//!    prefix build, plus sum-vs-composed (the pairwise-overlap tell);
//! 2. map WHERE the material differs — per grid column, `missing` =
//!    composed \ output and `extra` = output \ composed z-intervals; dump the
//!    offenders as TSV to stdout for geometric classification.
//!
//! ```text
//! cargo run -p test-harness --release --example deficit_map -- R0057 > map.tsv
//! ```
//! Human-readable summary goes to stderr; TSV rows (`x y z_lo z_hi kind`) to
//! stdout.

use std::fs;
use std::path::PathBuf;

use test_harness::assay::volume_oracle::{
    composed_volume, iv_diff, iv_len, iv_union, scan_volume, SolidScan,
};
use test_harness::assay::volume_oracle_doc::{operand_scan, oracle_tol, output_scan};
use test_harness::helpers::mesh_signed_volume;
use test_harness::ModelBuilder;

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

fn main() {
    let id = std::env::args()
        .nth(1)
        .expect("usage: deficit_map <CASE_ID>");
    let grid: usize = std::env::var("MAP_GRID")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(192);
    let waffle: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(assay_dir().join(format!("{id}.waffle"))).expect("read .waffle"),
    )
    .expect("parse .waffle");
    let meta: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(assay_dir().join(format!("{id}.meta.json"))).expect("read meta"),
    )
    .expect("parse meta");
    let scale = meta["scale"].as_f64().expect("scale");
    let n_ops = meta["operations"].as_array().expect("ops").len();
    let tol = oracle_tol(scale);
    eprintln!("case {id}  scale={scale:.3e}  tol={tol:.3e}  grid={grid}  ops={n_ops}");

    // Operand scans (isolated).
    let scans: Vec<SolidScan> = (0..n_ops)
        .map(|k| operand_scan(&waffle, k, tol).expect("operand scan"))
        .collect();
    let op_vols: Vec<f64> = scans.iter().map(|s| scan_volume(s, grid).volume).collect();

    // 1. Per-prefix localization: composed(0..=k) vs the in-context build.
    let feats = waffle["tabs"][0]["kind"]["features"]["features"]
        .as_array()
        .expect("features");
    let op_positions: Vec<usize> = feats
        .iter()
        .enumerate()
        .filter(|(_, f)| f["operation"]["type"] != "Sketch")
        .map(|(i, _)| i)
        .collect();
    eprintln!("\nk | op_vol | sum(0..=k) | composed(0..=k) | in_context(0..=k) | deficit");
    for k in 0..n_ops {
        let refs: Vec<&SolidScan> = scans[..=k].iter().collect();
        let cuts = vec![false; refs.len()];
        let composed = composed_volume(&refs, &cuts, grid).volume;
        let sum: f64 = op_vols[..=k].iter().sum();

        let mut doc = waffle.clone();
        doc["tabs"][0]["kind"]["features"]["features"]
            .as_array_mut()
            .unwrap()
            .truncate(op_positions[k] + 1);
        let mut b = ModelBuilder::kernel_v2();
        b.load(&serde_json::to_string(&doc).unwrap()).unwrap();
        let ctx: f64 = b
            .tessellate_live_with_tol(tol)
            .map(|ms| ms.iter().map(mesh_signed_volume).sum())
            .unwrap_or(f64::NAN);
        eprintln!(
            "{k} | {:.6e} | {sum:.6e} | {composed:.6e} | {ctx:.6e} | {:.3e}",
            op_vols[k],
            composed - ctx
        );
    }

    // 2. Column-level map of composed(all) vs output.
    let out = output_scan(&waffle, tol).expect("output scan");
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for s in scans.iter().chain(std::iter::once(&out)) {
        for a in 0..2 {
            min[a] = min[a].min(s.min[a]);
            max[a] = max[a].max(s.max[a]);
        }
    }
    let hx = (max[0] - min[0]) / grid as f64;
    let hy = (max[1] - min[1]) / grid as f64;
    let cell = hx * hy;
    let (mut miss_vol, mut extra_vol) = (0.0f64, 0.0f64);
    let mut rows = 0usize;
    println!("x\ty\tz_lo\tz_hi\tkind\towner");
    for j in 0..grid {
        let y = min[1] + (j as f64 + 0.5) * hy;
        for i in 0..grid {
            let x = min[0] + (i as f64 + 0.5) * hx;
            let cols: Vec<Vec<(f64, f64)>> = scans.iter().map(|s| s.column(x, y)).collect();
            let mut comp: Vec<(f64, f64)> = Vec::new();
            for c in &cols {
                comp = iv_union(&comp, c);
            }
            let oc = out.column(x, y);
            let missing = iv_diff(&comp, &oc);
            let extra = iv_diff(&oc, &comp);
            miss_vol += iv_len(&missing) * cell;
            extra_vol += iv_len(&extra) * cell;
            for (lo, hi) in missing {
                // Skip chord-scale noise slivers; the deficit is set-level.
                if hi - lo > tol * 10.0 {
                    // Which operand(s) own this lost material?
                    let owner: String = cols
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| iv_len(&iv_diff(&[(lo, hi)], c)) < (hi - lo) * 0.5)
                        .map(|(k, _)| k.to_string())
                        .collect::<Vec<_>>()
                        .join("+");
                    println!("{x:.6e}\t{y:.6e}\t{lo:.6e}\t{hi:.6e}\tmissing\top{owner}");
                    rows += 1;
                }
            }
            for (lo, hi) in extra {
                if hi - lo > tol * 10.0 {
                    println!("{x:.6e}\t{y:.6e}\t{lo:.6e}\t{hi:.6e}\textra\t-");
                    rows += 1;
                }
            }
        }
    }
    eprintln!(
        "\nGROSS missing={miss_vol:.6e}  extra={extra_vol:.6e}  net={:.6e}  rows={rows}",
        miss_vol - extra_vol
    );

    // 3. Convergence probe: analytic geometry converges under finer
    // tessellation; geometry frozen at some pipeline resolution does not.
    eprintln!("\nconvergence (volume, triangles) under tol sweep:");
    for f in [1.0, 0.1, 0.01] {
        let t = tol * f;
        let mut line = format!("  tol={t:.1e}:");
        for k in 0..n_ops {
            if let Some(s) = operand_scan(&waffle, k, t) {
                line += &format!(
                    "  op{k}={:.6e}/{}tris",
                    scan_volume(&s, grid).volume,
                    s.tri_count()
                );
            }
        }
        if let Some(o) = output_scan(&waffle, t) {
            line += &format!(
                "  out={:.6e}/{}tris",
                scan_volume(&o, grid).volume,
                o.tri_count()
            );
        }
        eprintln!("{line}");
    }

    // 4. Logical-face census (mesh-fidelity discriminator): analytic faces are
    // few with many triangles each; mesh-facet emission is ~one face per tri.
    let face_census = |mesh: &waffle_types::kernel::RenderMesh, label: &str| {
        let per_face: Vec<usize> = mesh
            .face_ranges
            .iter()
            .map(|r| ((r.end_index - r.start_index) / 3) as usize)
            .collect();
        let max = per_face.iter().copied().max().unwrap_or(0);
        eprintln!(
            "  {label}: faces={} tris={} max_tris_per_face={max}",
            per_face.len(),
            mesh.indices.len() / 3
        );
    };
    eprintln!("\nlogical faces:");
    {
        let json = test_harness::assay::volume_oracle_doc::isolate_operation(&waffle, n_ops - 1)
            .expect("isolate last op");
        let mut b = ModelBuilder::kernel_v2();
        b.load(&json).unwrap();
        face_census(
            &b.tessellate_last_with_tol(tol).unwrap(),
            "isolated last op",
        );
    }
    {
        let mut b = ModelBuilder::kernel_v2();
        b.load(&serde_json::to_string(&waffle).unwrap()).unwrap();
        for (bi, m) in b.tessellate_live_with_tol(tol).unwrap().iter().enumerate() {
            face_census(m, &format!("output body {bi}"));
        }
    }
}
