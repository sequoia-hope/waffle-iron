//! KV6d increment 5b2: kernel-v2 recovers and RE-TESSELLATES a boolean-output
//! torus patch.
//!
//! to_yang torus-operand conversion is a separate later increment, so we build
//! the torus operand directly as a yang B-Rep, run `yang_rs::boolean`, then feed
//! yang's OUTPUT through the kernel-v2 reconstruction (`from_yang_brep`, with the
//! new `FaceSurf::Torus` arm) and render tessellation (which routes the trimmed
//! torus patch — a polyline boundary, no full-circle edge — to the UV-CDT
//! consumer `yang_rs::tessellate_torus_patch`). The result must validate and
//! tessellate into a watertight, on-surface mesh.

use cad_primitives::{BoolOp, Point3, Vector3};
use kernel_v2::{from_yang_brep, tessellate, validate_solid, BrepArena, RenderMesh};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}
fn v3(x: f64, y: f64, z: f64) -> Vector3 {
    Vector3::new(x, y, z)
}

/// 90° bent tube (partial torus): center origin, axis +z, R=3, r=1. Closed
/// solid (torus band + 2 meridian-plane disk caps).
fn partial_torus_brep() -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(4.0, 0.0, 0.0),
        },
        BRepVertex {
            point: p(0.0, 4.0, 0.0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(3.0, 0.0, 0.0),
                normal: v3(0.0, 1.0, 0.0),
                radius: 1.0,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 3.0, 0.0),
                normal: v3(1.0, 0.0, 0.0),
                radius: 1.0,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 0.0),
                normal: v3(0.0, 0.0, 1.0),
                radius: 4.0,
            },
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Torus {
                center: p(0.0, 0.0, 0.0),
                axis_dir: v3(0.0, 0.0, 1.0),
                major_radius: 3.0,
                minor_radius: 1.0,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: v3(0.0, -1.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: v3(-1.0, 0.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("partial torus B-Rep")
}

fn box_brep(origin: [f64; 3], size: [f64; 3]) -> BRep {
    let [x, y, z] = origin;
    let [sx, sy, sz] = size;
    let pts = [
        [x, y, z],
        [x + sx, y, z],
        [x + sx, y + sy, z],
        [x, y + sy, z],
        [x, y, z + sz],
        [x + sx, y, z + sz],
        [x + sx, y + sy, z + sz],
        [x, y + sy, z + sz],
    ];
    let verts: Vec<BRepVertex> = pts
        .iter()
        .map(|&[a, b, c]| BRepVertex { point: p(a, b, c) })
        .collect();
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
    ];
    let normals = [
        v3(0.0, 0.0, -1.0),
        v3(0.0, 0.0, 1.0),
        v3(0.0, -1.0, 0.0),
        v3(1.0, 0.0, 0.0),
        v3(0.0, 1.0, 0.0),
        v3(-1.0, 0.0, 0.0),
    ];
    let mut edges = Vec::new();
    let mut faces = Vec::new();
    for (i, vs) in face_verts.iter().enumerate() {
        let base = edges.len() as u32;
        for k in 0..4 {
            edges.push(BRepEdge {
                start: vs[k],
                end: vs[(k + 1) % 4],
                curve: Curve::LineSegment,
            });
        }
        let n = normals[i];
        let pv = pts[vs[0] as usize];
        let d = -(n.x() * pv[0] + n.y() * pv[1] + n.z() * pv[2]);
        faces.push(BRepFace {
            surface: Surface::Plane { normal: n, d },
            outer_loop: vec![base, base + 1, base + 2, base + 3],
            inner_loops: vec![],
            reversed: false,
        });
    }
    BRep::new(verts, edges, faces).expect("box brep")
}

/// Positional watertightness: snap each render-mesh vertex to a 1e-9 grid and
/// require every undirected edge to be shared by exactly two triangles.
fn assert_render_watertight(mesh: &RenderMesh, what: &str) {
    use std::collections::BTreeMap;
    let key = |i: u32| -> [i64; 3] {
        let k = i as usize * 3;
        [
            (mesh.positions[k] * 1e9).round() as i64,
            (mesh.positions[k + 1] * 1e9).round() as i64,
            (mesh.positions[k + 2] * 1e9).round() as i64,
        ]
    };
    let mut ec: BTreeMap<([i64; 3], [i64; 3]), u32> = BTreeMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(a), key(b));
            let e = if ka < kb { (ka, kb) } else { (kb, ka) };
            *ec.entry(e).or_insert(0) += 1;
        }
    }
    for (e, n) in &ec {
        assert_eq!(*n, 2, "{what}: edge {e:?} shared by {n} tris (not 2)");
    }
}

/// Every torus-band render vertex lies on the tube: √((ρ−R)² + τ²) ≈ r.
fn assert_band_on_surface(mesh: &RenderMesh, what: &str) {
    let mut band = 0;
    for i in 0..mesh.num_vertices() {
        let k = i * 3;
        let (px, py, pz) = (
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        );
        let tau = pz; // axis +z
        let rho = (px * px + py * py).sqrt();
        let d = rho - 3.0;
        let resid = ((d * d + tau * tau).sqrt() - 1.0).abs();
        if resid < 1e-6 {
            band += 1;
        }
    }
    assert!(band > 0, "{what}: no on-tube torus vertices");
}

/// The full path — yang boolean → kernel-v2 reconstruction → render — once the
/// torus output boundary lands on the analytic surface. BLOCKED on torus Stage-4
/// SSI relocation: yang returns `UnsupportedSurfaceForSsi` for a torus, so the
/// intersection curve stays on the input tessellation's chords (~0.096 off the
/// analytic torus for this fixture — see `torus_output_boundary_is_chord_off_surface`),
/// and `validate_torus_face` (correctly, per P9) rejects the off-surface boundary
/// during `from_yang_brep`. The reconstruction + UV-CDT render wiring it would
/// exercise are already in place and unit-tested
/// (`tessellate::torus_patch_tess_tests`); only the SSI relocation is missing.
#[test]
#[ignore = "KV6d-5b2: needs torus Stage-4 SSI relocation — output boundary is ~0.1 off the analytic torus (chord), so from_yang_brep validation rejects it"]
fn torus_minus_box_reconstructs_and_tessellates() {
    let Some(backend) = yang_rs::native_backend() else {
        eprintln!("native backend unavailable — skipping");
        return;
    };
    // Box biting the outer wall near θ=0 trims the torus lateral into an
    // arbitrary-boundary patch (no full-circle edge → UV-CDT path).
    let a = partial_torus_brep();
    let b = box_brep([3.4, -0.6, -0.6], [1.2, 1.2, 1.2]);

    let out = yang_rs::boolean(&a, &b, BoolOp::Subtract, &backend)
        .unwrap_or_else(|e| panic!("torus − box (yang): {e:?}"));

    let mut arena = BrepArena::new();
    let solid = from_yang_brep(&mut arena, &out)
        .unwrap_or_else(|e| panic!("from_yang_brep(torus output): {e:?}"));
    validate_solid(&arena, solid).expect("reconstructed torus-cut solid validates");

    let mesh = tessellate(&arena, solid).expect("tessellate reconstructed torus cut");
    assert!(!mesh.indices.is_empty(), "non-empty render mesh");
    assert_render_watertight(&mesh, "torus − box render");
    assert_band_on_surface(&mesh, "torus − box render");
}

/// Documents the blocker above with a hard number: the trimmed torus face's
/// boundary vertices sit well off the analytic torus (no SSI relocation), so
/// the analytic-B-Rep reconstruction cannot accept them yet.
#[test]
fn torus_output_boundary_is_chord_off_surface() {
    let Some(backend) = yang_rs::native_backend() else {
        return;
    };
    let a = partial_torus_brep();
    let b = box_brep([3.4, -0.6, -0.6], [1.2, 1.2, 1.2]);
    let out = yang_rs::boolean(&a, &b, BoolOp::Subtract, &backend).expect("yang boolean");
    let mut maxr: f64 = 0.0;
    for f in out.faces() {
        if let Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } = f.surface
        {
            let c = center.as_array();
            let ax = axis_dir.as_array();
            let verts = out.vertices();
            for e in &f.outer_loop {
                let pid = out.edges()[*e as usize].start as usize;
                let p = verts[pid].point.as_array();
                let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
                let tau = d[0] * ax[0] + d[1] * ax[1] + d[2] * ax[2];
                let rad = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
                let rho = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
                let resid = ((rho - major_radius).powi(2) + tau * tau).sqrt() - minor_radius;
                maxr = maxr.max(resid.abs());
            }
        }
    }
    // Far above any analytic-surface tolerance: relocation (SSI) is required.
    assert!(
        maxr > 1e-3,
        "expected a large chord-vs-analytic gap, got {maxr:.3e}"
    );
}
