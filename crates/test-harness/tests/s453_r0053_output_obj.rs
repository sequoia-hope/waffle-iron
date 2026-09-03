//! Dump the kernel's R0053 output tessellation (all three ops, under the
//! §4.5.3 surface-pair arm) to an OBJ so it can be classified face by face
//! against the EXACT union membership (`s453_r0053_exact_topology.rs`,
//! spec `yang_451_corner_transit.md` §3ah).
//!
//! ```text
//! S453_OBJ_OUT=/path/out.obj cargo test -p test-harness --release \
//!   --test s453_r0053_output_obj -- --ignored --nocapture
//! ```
//! Sets `YANG_453_SPAIR=1` process-wide unless `S453_PROBE_GATES=off`.

use std::fs;
use std::path::{Path, PathBuf};

use test_harness::assay::volume_oracle_doc::oracle_tol;
use test_harness::cherchi_sidecar::{surface_topology, write_obj};
use test_harness::workflow::ModelBuilder;

const CORPUS: &str = "../../app/tests/cases/assay";

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS)
}

#[test]
#[ignore = "sets a gate knob process-wide; run alone"]
fn r0053_output_to_obj() {
    if std::env::var("S453_PROBE_GATES").as_deref() != Ok("off") {
        std::env::set_var("YANG_453_SPAIR", "1");
    }
    let out = std::env::var("S453_OBJ_OUT").unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/topo_sidecar/R0053/kernel_spair.obj")
            .to_string_lossy()
            .into_owned()
    });
    let d = corpus_dir();
    let waffle = fs::read_to_string(d.join("R0053.waffle")).unwrap();
    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.join("R0053.meta.json")).unwrap()).unwrap();
    let scale = meta["scale"].as_f64().unwrap();
    let tol = oracle_tol(scale);
    let mut b = ModelBuilder::kernel_v2();
    b.load(&waffle).expect("load");
    let failures: Vec<String> = b
        .engine_warnings()
        .iter()
        .filter(|w| w.contains("Auto-union failed"))
        .cloned()
        .collect();
    println!(
        "[s453] R0053 errors={:?} union_failures={failures:?}",
        b.engine_errors()
    );
    let mesh = b.tessellate_last_with_tol(tol).expect("tessellate");
    let verts: Vec<[f64; 3]> = mesh
        .vertices
        .chunks(3)
        .map(|v| [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])])
        .collect();
    let tris: Vec<[u32; 3]> = mesh.indices.chunks(3).map(|t| [t[0], t[1], t[2]]).collect();
    let t = surface_topology(&verts, &tris);
    println!("[s453] kernel output topology: {t:?}");
    if let Some(p) = Path::new(&out).parent() {
        fs::create_dir_all(p).unwrap();
    }
    write_obj(Path::new(&out), &mesh).expect("write obj");
    println!(
        "[s453] wrote {out}: {} verts {} tris",
        verts.len(),
        tris.len()
    );
}
