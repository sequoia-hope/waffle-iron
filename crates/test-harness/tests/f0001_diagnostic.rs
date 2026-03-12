//! Diagnostic test for F0001 — traces the full LoadProject path
//! to find where watertight mesh failure originates.

use std::path::PathBuf;
use test_harness::oracle::check_watertight_mesh;
use test_harness::ModelBuilder;

#[test]
fn f0001_load_path_diagnostic() {
    let assay_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay");

    let waffle_path = assay_dir.join("F0001.waffle");
    if !waffle_path.exists() {
        eprintln!("F0001.waffle not found at {:?}, skipping", waffle_path);
        return;
    }

    let waffle_json = std::fs::read_to_string(&waffle_path).unwrap();

    // Load through full feature engine path
    let mut builder = ModelBuilder::kernel();
    builder.load(&waffle_json).unwrap();

    // Check feature count
    let feature_count = builder.feature_count();
    eprintln!("Feature count: {}", feature_count);

    // Check distinct solid count
    let solid_count = builder.distinct_solid_count();
    eprintln!("Distinct solid count: {}", solid_count);

    // Check engine errors/warnings
    let errors = builder.engine_errors();
    eprintln!("Engine errors: {:?}", errors);
    let warnings = builder.engine_warnings();
    eprintln!("Engine warnings: {:?}", warnings);

    // Tessellate via the same path as assay runner
    let mesh = builder.tessellate_last().unwrap();

    let n_verts = mesh.vertices.len() / 3;
    let n_tris = mesh.indices.len() / 3;
    let n_normals = mesh.normals.len() / 3;
    eprintln!(
        "Mesh: {} vertices, {} triangles, {} normals",
        n_verts, n_tris, n_normals
    );

    // Check watertight
    let wt = check_watertight_mesh(&mesh);
    eprintln!("Watertight: {} — {}", wt.passed, wt.detail);

    // Print all triangles with their vertex positions
    for i in 0..n_tris {
        let i0 = mesh.indices[i * 3] as usize;
        let i1 = mesh.indices[i * 3 + 1] as usize;
        let i2 = mesh.indices[i * 3 + 2] as usize;
        let v0 = &mesh.vertices[i0 * 3..i0 * 3 + 3];
        let v1 = &mesh.vertices[i1 * 3..i1 * 3 + 3];
        let v2 = &mesh.vertices[i2 * 3..i2 * 3 + 3];
        let n0 = &mesh.normals[i0 * 3..i0 * 3 + 3];
        eprintln!(
            "  tri {:2}: v[{},{},{}] [{:.4},{:.4},{:.4}] [{:.4},{:.4},{:.4}] [{:.4},{:.4},{:.4}]  n=[{:.2},{:.2},{:.2}]",
            i, i0, i1, i2, v0[0], v0[1], v0[2], v1[0], v1[1], v1[2], v2[0], v2[1], v2[2],
            n0[0], n0[1], n0[2]
        );
    }

    // Also tessellate each feature individually to see what's going on
    eprintln!("\n--- Per-feature tessellation ---");
    // Collect feature info first to avoid borrow issues
    let feature_info: Vec<_> = builder
        .state
        .engine
        .tree
        .features
        .iter()
        .map(|f| {
            let handle = builder
                .state
                .engine
                .get_result(f.id)
                .and_then(|r| r.outputs.first().map(|(_, b)| b.handle.clone()));
            (f.name.clone(), f.id, handle)
        })
        .collect();

    for (name, id, handle_opt) in &feature_info {
        if let Some(handle) = handle_opt {
            match builder.kernel_mut().tessellate(handle, 0.1) {
                Ok(fmesh) => {
                    let ft = fmesh.indices.len() / 3;
                    let wt2 = check_watertight_mesh(&fmesh);
                    eprintln!(
                        "  {} ({}): {} tris, watertight={} {}",
                        name, id, ft, wt2.passed, wt2.detail
                    );
                }
                Err(e) => {
                    eprintln!("  {} ({}): tessellation error: {:?}", name, id, e);
                }
            }
        } else {
            eprintln!("  {} ({}): no outputs", name, id);
        }
    }

    assert!(wt.passed, "F0001 mesh should be watertight: {}", wt.detail);
}

/// Test the explicit boolean path (no LoadProject) to compare
#[test]
fn f0001_explicit_boolean_path() {
    let mut b = ModelBuilder::kernel();

    // Box A: 0.5×0.5 centered at origin, depth 0.3
    b.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], -0.25, -0.25, 0.5, 0.5)
        .unwrap();
    b.extrude_no_merge("body_a", "sk_a", 0.3).unwrap();

    // Box B: identical
    b.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], -0.25, -0.25, 0.5, 0.5)
        .unwrap();
    b.extrude_no_merge("body_b", "sk_b", 0.3).unwrap();

    // Union
    b.boolean_union("result", "body_a", "body_b").unwrap();

    let mesh = b.tessellate("result").unwrap();
    let n_tris = mesh.indices.len() / 3;
    let wt = check_watertight_mesh(&mesh);
    eprintln!(
        "Explicit path: {} tris, watertight={} {}",
        n_tris, wt.passed, wt.detail
    );

    // Print triangles
    for i in 0..n_tris {
        let i0 = mesh.indices[i * 3] as usize;
        let i1 = mesh.indices[i * 3 + 1] as usize;
        let i2 = mesh.indices[i * 3 + 2] as usize;
        let v0 = &mesh.vertices[i0 * 3..i0 * 3 + 3];
        let v1 = &mesh.vertices[i1 * 3..i1 * 3 + 3];
        let v2 = &mesh.vertices[i2 * 3..i2 * 3 + 3];
        eprintln!(
            "  tri {:2}: [{:.4},{:.4},{:.4}] [{:.4},{:.4},{:.4}] [{:.4},{:.4},{:.4}]",
            i, v0[0], v0[1], v0[2], v1[0], v1[1], v1[2], v2[0], v2[1], v2[2]
        );
    }

    assert!(
        wt.passed,
        "Explicit boolean path should be watertight: {}",
        wt.detail
    );
}
