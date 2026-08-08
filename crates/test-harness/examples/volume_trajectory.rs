//! Per-operation IN-CONTEXT volume trajectory for one assay case.
//!
//! For each op k of the case, rebuild the document truncated to features
//! `[0..=op_k]` and print every live body's volume plus the total. This is the
//! localization tool for an independent-volume-oracle flag
//! (`tests/assay_volume_oracle.rs`): the composed-vs-output discrepancy is a
//! SUM over the chain; the trajectory says at which op it enters, in the
//! kernel's own context (no operand isolation involved).
//!
//! Volume-monotonicity violations matter here because the categorized assay
//! DOWNGRADES them to advisory passes (`assay/properties_v2.rs` I9–I12), so a
//! union step that loses material is invisible in the SUPPORTED_CORRECT
//! verdict.
//!
//! ```text
//! cargo run -p test-harness --release --example volume_trajectory -- R0090
//! ```

use std::fs;
use std::path::PathBuf;

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
        .expect("usage: volume_trajectory <CASE_ID>");
    let waffle: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(assay_dir().join(format!("{id}.waffle"))).expect("read .waffle"),
    )
    .expect("parse .waffle");
    let meta: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(assay_dir().join(format!("{id}.meta.json"))).expect("read meta"),
    )
    .expect("parse meta");
    let scale = meta["scale"].as_f64().expect("scale");
    // Same tolerance family as the volume oracle: far finer than render tol.
    let tol = (scale * 1e-4).clamp(1e-15, 1e-3);

    let feats = waffle["tabs"][0]["kind"]["features"]["features"]
        .as_array()
        .expect("features");
    let op_positions: Vec<usize> = feats
        .iter()
        .enumerate()
        .filter(|(_, f)| f["operation"]["type"] != "Sketch")
        .map(|(i, _)| i)
        .collect();
    println!(
        "case {id}  scale={scale:.3e}  tol={tol:.3e}  ops={}",
        op_positions.len()
    );

    // Isolated operand volumes (divergence theorem) — cross-checks the winding
    // sweep the oracle uses on the same solids: the two integrators share no
    // code, so agreement certifies the scan, not the composition.
    for (k, &pos) in op_positions.iter().enumerate() {
        let mut doc = waffle.clone();
        let op = feats[pos].clone();
        let sketch_id = op["operation"]["params"]["sketch_id"]
            .as_str()
            .unwrap_or("");
        let sketch = feats
            .iter()
            .find(|f| f["operation"]["sketch"]["id"].as_str() == Some(sketch_id))
            .cloned();
        let Some(sketch) = sketch else { continue };
        let list = doc["tabs"][0]["kind"]["features"]["features"]
            .as_array_mut()
            .unwrap();
        *list = vec![sketch, op];
        let json = serde_json::to_string(&doc).unwrap();
        let mut b = ModelBuilder::kernel_v2();
        if b.load(&json).is_err() || !b.engine_errors().is_empty() {
            println!("operand {k} isolated: build failed");
            continue;
        }
        match b.tessellate_live_with_tol(tol) {
            Ok(meshes) => {
                let total: f64 = meshes.iter().map(mesh_signed_volume).sum();
                println!("operand {k} isolated: divergence vol={total:.6e}");
            }
            Err(e) => println!("operand {k} isolated: TESSELLATE FAILED: {e}"),
        }
    }

    for (k, &pos) in op_positions.iter().enumerate() {
        let mut doc = waffle.clone();
        let list = doc["tabs"][0]["kind"]["features"]["features"]
            .as_array_mut()
            .unwrap();
        list.truncate(pos + 1);
        let json = serde_json::to_string(&doc).unwrap();
        let mut b = ModelBuilder::kernel_v2();
        if let Err(e) = b.load(&json) {
            println!("after op {k}: LOAD FAILED: {e}");
            continue;
        }
        if !b.engine_errors().is_empty() {
            println!("after op {k}: ENGINE ERRORS: {:?}", b.engine_errors());
            continue;
        }
        match b.tessellate_live_with_tol(tol) {
            Ok(meshes) => {
                let vols: Vec<f64> = meshes.iter().map(mesh_signed_volume).collect();
                let total: f64 = vols.iter().sum();
                println!(
                    "after op {k}: bodies={} distinct_solid_count={} total={total:.6e} per-body={:?}",
                    meshes.len(),
                    b.distinct_solid_count(),
                    vols.iter().map(|v| format!("{v:.4e}")).collect::<Vec<_>>()
                );
            }
            Err(e) => println!("after op {k}: TESSELLATE FAILED: {e}"),
        }
    }
}
