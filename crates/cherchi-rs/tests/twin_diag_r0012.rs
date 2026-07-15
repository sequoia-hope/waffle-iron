//! #166 REFERENCE-PARITY DIAGNOSTIC (env-gated, ignored): does a downstream
//! render-collapse twin exist in the NATIVE arrangement only, or in the
//! Cherchi 2022 C++ SIDECAR reference too?
//!
//! Feeds two Stage-2 operand meshes (dumped from a yang boolean via
//! `YANG_STAGE0_DUMP_DIR`) through both `native_labeled_arrangement` and the
//! sidecar, then reports coincident-vertex pairs (and any near a target point)
//! for each. If the two backends agree bit-for-bit, the twin is GENUINE — not
//! a native arrangement bug — and the fix belongs upstream/downstream of the
//! arrangement, never in cherchi-rs.
//!
//! This is the tool that resolved task #166 (yang_deviations.md N48): for both
//! R0012 and R0098 the native arrangement is bit-identical to the C++
//! reference, so their ~1e-6 render-collapse twins are genuine consequences of
//! near-coincident Stage-0 overlay sweep-event columns, not a native defect.
//!
//! Regenerate the operand dumps and run:
//! ```text
//! DUMP=/tmp/r0012 ; mkdir -p $DUMP
//! ASSAY_CASE=R0012 YANG_STAGE0_DUMP_DIR=$DUMP \
//!   ./target/release/deps/assay_kv2-* --exact single_case --ignored --nocapture
//! ARR_A_OBJ=$DUMP/000_subtract_a.obj ARR_B_OBJ=$DUMP/000_subtract_b.obj \
//!   TWIN_TARGET="43.0265,-55.9136,-71.8299" \
//!   cargo test -p cherchi-rs --release --test twin_diag_r0012 -- --ignored --nocapture
//! ```

use std::time::Duration;

use cad_primitives::Point3;
use cherchi_rs::{native_labeled_arrangement, Mesh};

fn read_obj(path: &str) -> Mesh {
    let text = std::fs::read_to_string(path).expect("read obj");
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let x: f64 = it.next().unwrap().parse().unwrap();
                let y: f64 = it.next().unwrap().parse().unwrap();
                let z: f64 = it.next().unwrap().parse().unwrap();
                verts.push(Point3::new(x, y, z));
            }
            Some("f") => {
                let a: u32 = it.next().unwrap().parse::<u32>().unwrap() - 1;
                let b: u32 = it.next().unwrap().parse::<u32>().unwrap() - 1;
                let c: u32 = it.next().unwrap().parse::<u32>().unwrap() - 1;
                tris.push([a, b, c]);
            }
            _ => {}
        }
    }
    Mesh::new(verts, tris)
}

/// Report sub-`tol` coincident pairs (over triangle-referenced vertices only)
/// and any vertex within `near_r` of an optional `target`.
fn report(tag: &str, mesh: &Mesh, tol: f64, target: Option<[f64; 3]>, near_r: f64) {
    let mut referenced = vec![false; mesh.verts.len()];
    for t in &mesh.tris {
        for &v in t {
            referenced[v as usize] = true;
        }
    }
    let ref_ids: Vec<usize> = (0..mesh.verts.len()).filter(|&i| referenced[i]).collect();
    println!(
        "[{tag}] {} verts ({} referenced), {} tris",
        mesh.verts.len(),
        ref_ids.len(),
        mesh.tris.len()
    );

    let mut pair_count = 0usize;
    let mut min_nonzero = f64::INFINITY;
    for (ii, &i) in ref_ids.iter().enumerate() {
        let vi = mesh.verts[i].as_array();
        for &j in &ref_ids[ii + 1..] {
            let vj = mesh.verts[j].as_array();
            let d = (0..3).map(|k| (vi[k] - vj[k]).powi(2)).sum::<f64>().sqrt();
            if d > 0.0 && d < tol {
                pair_count += 1;
                if pair_count <= 12 {
                    println!("  coincident pair v{i} v{j} d={d:.3e}  {vi:?} / {vj:?}");
                }
            }
            if d > 0.0 && d < min_nonzero {
                min_nonzero = d;
            }
        }
    }
    println!("  total sub-{tol:.0e} coincident referenced pairs: {pair_count}; min nonzero gap {min_nonzero:.3e}");

    if let Some(t) = target {
        println!("  verts within {near_r} of target {t:?}:");
        for &i in &ref_ids {
            let v = mesh.verts[i].as_array();
            let d = ((0..3).map(|k| (v[k] - t[k]).powi(2)).sum::<f64>()).sqrt();
            if d <= near_r {
                println!("    v{i} d={d:.3e}  {v:?}");
            }
        }
    }
}

#[test]
#[ignore]
fn twin_diag_r0012() {
    let (Ok(a_path), Ok(b_path)) = (std::env::var("ARR_A_OBJ"), std::env::var("ARR_B_OBJ")) else {
        println!("twin_diag_r0012: set ARR_A_OBJ / ARR_B_OBJ (Stage-0 operand dumps) — skipping");
        return;
    };
    let a = read_obj(&a_path);
    let b = read_obj(&b_path);
    println!(
        "loaded a={} verts/{} tris, b={} verts/{} tris",
        a.verts.len(),
        a.tris.len(),
        b.verts.len(),
        b.tris.len()
    );

    let target = std::env::var("TWIN_TARGET").ok().and_then(|s| {
        let n: Vec<f64> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
        <[f64; 3]>::try_from(n).ok()
    });
    let tol = 1e-4;
    let near_r = 1e-2;

    let native = native_labeled_arrangement(&a, &b).expect("native arrangement");
    report("native", &native.mesh, tol, target, near_r);

    match cherchi_sidecar_rs::labeled_arrangement(&a, &b, Duration::from_secs(150)) {
        Ok(sc) => report("sidecar", &sc.mesh, tol, target, near_r),
        Err(e) => println!("[sidecar] FAILED: {e}"),
    }
}
