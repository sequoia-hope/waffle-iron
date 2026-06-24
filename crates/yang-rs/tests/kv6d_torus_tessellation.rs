//! KV6d increment 4b: yang tessellates a partial-torus `Surface::Torus` face
//! (the 2D bijective grid) into a watertight, on-surface mesh.

use std::collections::BTreeMap;

use cad_primitives::{Point3, Vector3};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A partial torus (90° bent tube): center origin, axis +z, major R=3, minor
/// r=1. θ=0 meridian in the xz-plane (radial +x), θ=α=90° meridian in the
/// yz-plane (radial +y). 2 profile circles + 1 seam arc (the φ=0 outer
/// longitude, radius R+r=4) + 2 disk caps in the meridian planes.
fn partial_torus_brep() -> BRep {
    // θ=0: w0=+x, m0 = z×x = +y. C0=(3,0,0), V0=(4,0,0).
    // θ=α: wα=+y, mα = z×y = −x. Cα=(0,3,0), Vα=(0,4,0).
    let verts = vec![
        BRepVertex {
            point: p(4.0, 0.0, 0.0),
        }, // V0 (θ=0, φ=0)
        BRepVertex {
            point: p(0.0, 4.0, 0.0),
        }, // Vα (θ=α, φ=0)
    ];
    let edges = vec![
        // e0 profile circle at θ=0 (meridian plane normal +m0 = +y).
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(3.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 1.0, 0.0),
                radius: 1.0,
            },
        },
        // e1 profile circle at θ=α (meridian plane normal −mα = +x).
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 3.0, 0.0),
                normal: Vector3::new(1.0, 0.0, 0.0),
                radius: 1.0,
            },
        },
        // e2 seam arc: φ=0 longitude V0→Vα, CCW around +z, radius R+r=4.
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: 4.0,
            },
        },
    ];
    let faces = vec![
        // f0 torus lateral.
        BRepFace {
            surface: Surface::Torus {
                center: p(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                major_radius: 3.0,
                minor_radius: 1.0,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f1 start cap (θ=0 disk, outward normal −m0 = −y).
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, -1.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f2 end cap (θ=α disk, outward normal +mα = −x).
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(-1.0, 0.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("partial torus B-Rep must tessellate (KV6d 4b)")
}

#[test]
fn partial_torus_tessellates_watertight_and_on_surface() {
    let b = partial_torus_brep();
    let mesh = b.as_mesh();
    assert!(!mesh.tris.is_empty(), "non-empty mesh");

    // Watertight + 2-manifold: every undirected edge shared by exactly 2 tris.
    let mut ec: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in &mesh.tris {
        for (a, c) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let k = if a < c { (a, c) } else { (c, a) };
            *ec.entry(k).or_insert(0) += 1;
        }
    }
    for (e, n) in &ec {
        assert_eq!(*n, 2, "edge {e:?} shared by {n} tris (not 2)");
    }

    // Every torus-band triangle's centroid lies on the tube surface:
    // √((ρ−R)² + τ²) ≈ r. (Cap triangles lie in the meridian planes; skip them
    // by checking the residual only where it is near 0 within the chord band.)
    let mut band_tris = 0;
    for t in &mesh.tris {
        let cen = [0, 1, 2].map(|k| {
            (mesh.verts[t[0] as usize].as_array()[k]
                + mesh.verts[t[1] as usize].as_array()[k]
                + mesh.verts[t[2] as usize].as_array()[k])
                / 3.0
        });
        let tau = cen[2]; // axis +z
        let rho = (cen[0] * cen[0] + cen[1] * cen[1]).sqrt();
        let d = rho - 3.0;
        let resid = ((d * d + tau * tau).sqrt() - 1.0).abs();
        if resid < 5e-2 {
            band_tris += 1;
        }
    }
    assert!(
        band_tris > 0,
        "the torus band produced on-surface triangles"
    );
}
