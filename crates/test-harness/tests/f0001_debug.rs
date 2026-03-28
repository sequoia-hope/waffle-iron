//! Debug test for F0001 — traces LoadProject pipeline to find
//! why F0001 fails in assay despite kernel-level boolean union working.

use std::collections::HashMap;
use std::path::PathBuf;
use test_harness::oracle::check_watertight_mesh;
use test_harness::ModelBuilder;

#[test]
fn debug_f0001_pipeline() {
    let assay_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay");

    let waffle_path = assay_dir.join("F0001.waffle");
    let waffle_json =
        std::fs::read_to_string(&waffle_path).expect("read F0001.waffle");

    let mut builder = ModelBuilder::kernel();
    builder.load(&waffle_json).expect("load F0001");

    // Check engine warnings (clone to avoid borrow conflicts later)
    let warnings: Vec<String> = builder.engine_warnings().to_vec();
    println!("Engine warnings: {:?}", warnings);

    let errors: Vec<_> = builder.engine_errors().to_vec();
    println!("Engine errors: {:?}", errors);

    // Check last feature's result outputs
    {
        let tree = &builder.state.engine.tree;
        let limit = tree.active_index.unwrap_or(tree.features.len());
        for feature in tree.features[..limit].iter().rev() {
            if feature.suppressed {
                continue;
            }
            if let Some(result) = builder.state.engine.get_result(feature.id) {
                println!(
                    "Last active feature: '{}' ({}), outputs: {}",
                    feature.name,
                    feature.id,
                    result.outputs.len()
                );
                for (i, (role, body)) in result.outputs.iter().enumerate() {
                    println!("  output[{}]: role={:?}, handle={:?}", i, role, body.handle);
                }
                break;
            }
        }
    }

    // Tessellate last feature with finer tolerance
    let mesh = builder
        .tessellate_last_with_tol(0.01)
        .expect("tessellate");

    let n_verts = mesh.vertices.len() / 3;
    let n_tris = mesh.indices.len() / 3;

    println!("Mesh: V={}, F(tris)={}", n_verts, n_tris);
    println!("vertices.len() = {}", mesh.vertices.len());
    println!("indices.len() = {}", mesh.indices.len());

    // Check watertight with position-based edge counting
    let max_abs = mesh
        .vertices
        .iter()
        .map(|v| v.abs())
        .fold(0.0_f32, f32::max);
    let grid_size = (max_abs as f64 * 1e-5).max(1e-10);
    let inv_grid = 1.0 / grid_size;

    let quantize = |v: f32| -> i64 { (v as f64 * inv_grid).round() as i64 };

    let vert_key = |idx: u32| -> (i64, i64, i64) {
        let i = idx as usize * 3;
        (
            quantize(mesh.vertices[i]),
            quantize(mesh.vertices[i + 1]),
            quantize(mesh.vertices[i + 2]),
        )
    };

    let make_edge =
        |a: (i64, i64, i64), b: (i64, i64, i64)| -> ((i64, i64, i64), (i64, i64, i64)) {
            if a < b {
                (a, b)
            } else {
                (b, a)
            }
        };

    let mut edge_counts: HashMap<((i64, i64, i64), (i64, i64, i64)), usize> = HashMap::new();
    for tri in mesh.indices.chunks(3) {
        let va = vert_key(tri[0]);
        let vb = vert_key(tri[1]);
        let vc = vert_key(tri[2]);
        *edge_counts.entry(make_edge(va, vb)).or_default() += 1;
        *edge_counts.entry(make_edge(vb, vc)).or_default() += 1;
        *edge_counts.entry(make_edge(vc, va)).or_default() += 1;
    }

    let n_edges = edge_counts.len();
    let unpaired: Vec<_> = edge_counts.iter().filter(|(_, &c)| c != 2).collect();
    let boundary = unpaired.iter().filter(|(_, &c)| c == 1).count();
    let non_manifold = unpaired.iter().filter(|(_, &c)| c >= 3).count();

    println!(
        "Edges: {} total, {} unpaired ({} boundary, {} non-manifold)",
        n_edges,
        unpaired.len(),
        boundary,
        non_manifold
    );
    println!(
        "Euler: V({}) - E({}) + F({}) = {}",
        n_verts,
        n_edges,
        n_tris,
        n_verts as i64 - n_edges as i64 + n_tris as i64
    );

    // Also check via oracle helper
    let wt = check_watertight_mesh(&mesh);
    println!("Oracle watertight check: passed={}, detail={}", wt.passed, wt.detail);

    // Check for auto-union warning
    let auto_union_failed = warnings.iter().any(|w| w.contains("Auto-union failed"));
    println!("Auto-union failed: {}", auto_union_failed);

    assert!(
        !auto_union_failed,
        "F0001: auto-union should not have failed"
    );
    assert_eq!(
        unpaired.len(),
        0,
        "F0001: mesh should be watertight ({} boundary, {} non-manifold edges)",
        boundary,
        non_manifold
    );
}
