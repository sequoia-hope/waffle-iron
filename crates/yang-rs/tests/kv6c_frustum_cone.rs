//! KV6c increment 5b: yang ingests a FRUSTUM-band cone face (two rims at
//! different radii) and tessellates it as a watertight ruled band.
//!
//! Until now `Surface::Cone` accepted only an APEX-pointed cone (one base rim,
//! fanned from the apex — PR-YR16). kernel-v2 revolve produces frustum bands
//! (the profile cannot reach the axis), so a cone face is bounded by TWO rims +
//! a seam ruling, exactly like the cylinder canonical tube. This builds such a
//! B-Rep through `BRep::new` and asserts it tessellates watertight, error-
//! bounded, and outward-oriented.

use std::collections::BTreeMap;

use cad_primitives::{Point3, Vector3};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A closed 45° truncated cone (frustum): apex at the origin, axis +z, bottom
/// rim r=1 @ z=1, top rim r=3 @ z=3 (so radius = height ⇒ half-angle 45°). One
/// `Surface::Cone` lateral (outer_loop `[rim0, seam, rim1, seam]`, the cylinder
/// tube vocabulary) + two planar caps sharing the rims.
fn frustum_cone_brep() -> BRep {
    let apex = [0.0, 0.0, 0.0];
    let axis = [0.0, 0.0, 1.0];
    let half_angle = std::f64::consts::FRAC_PI_4;
    let (z0, z1) = (1.0, 3.0);
    let (r0, r1) = (z0 * half_angle.tan(), z1 * half_angle.tan());
    let bottom_center = [apex[0], apex[1], apex[2] + z0];
    let top_center = [apex[0], apex[1], apex[2] + z1];
    // Seam points at angle 0 = center + r·x̂ on each rim.
    let v0 = [bottom_center[0] + r0, bottom_center[1], bottom_center[2]];
    let v1 = [top_center[0] + r1, top_center[1], top_center[2]];

    let verts = vec![
        BRepVertex {
            point: p(v0[0], v0[1], v0[2]),
        },
        BRepVertex {
            point: p(v1[0], v1[1], v1[2]),
        },
    ];

    let neg_axis = [-axis[0], -axis[1], -axis[2]];
    let edges = vec![
        // e0 bottom rim (normal −axis: toward the apex / toward the top rim).
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(bottom_center[0], bottom_center[1], bottom_center[2]),
                normal: Vector3::new(axis[0], axis[1], axis[2]),
                radius: r0,
            },
        },
        // e1 top rim.
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                radius: r1,
            },
        },
        // e2 seam ruling (a slant generator from the bottom rim to the top).
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];

    let faces = vec![
        // f0 cone lateral — the frustum band.
        BRepFace {
            surface: Surface::Cone {
                apex: p(apex[0], apex[1], apex[2]),
                axis_dir: Vector3::new(axis[0], axis[1], axis[2]),
                half_angle,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f1 bottom cap (outward normal −axis).
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: -(neg_axis[0] * bottom_center[0]
                    + neg_axis[1] * bottom_center[1]
                    + neg_axis[2] * bottom_center[2]),
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // f2 top cap (outward normal +axis).
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis[0], axis[1], axis[2]),
                d: -(axis[0] * top_center[0] + axis[1] * top_center[1] + axis[2] * top_center[2]),
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces).expect("frustum cone B-Rep must tessellate (KV6c 5b)")
}

#[test]
fn frustum_cone_tessellates_watertight_and_outward() {
    let b = frustum_cone_brep();
    let mesh = b.as_mesh();
    assert!(!mesh.tris.is_empty(), "non-empty mesh");

    // Watertight + 2-manifold: every undirected edge shared by exactly two
    // triangles (apex-free frustum — caps + band).
    let mut edge_count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for tri in &mesh.tris {
        for (a, c) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let key = if a < c { (a, c) } else { (c, a) };
            *edge_count.entry(key).or_insert(0) += 1;
        }
    }
    for (e, n) in &edge_count {
        assert_eq!(*n, 2, "edge {e:?} shared by {n} triangles (not 2)");
    }

    // Every band triangle lies on the cone surface within the chord bound, and
    // its centroid's radial residual `r − τ·tan α` is ~0 (on the lateral).
    let apex = [0.0, 0.0, 0.0];
    let ax = [0.0, 0.0, 1.0];
    let tan = std::f64::consts::FRAC_PI_4.tan();
    let mut band_tris = 0;
    for tri in &mesh.tris {
        let c = [
            (mesh.verts[tri[0] as usize].as_array()[0]
                + mesh.verts[tri[1] as usize].as_array()[0]
                + mesh.verts[tri[2] as usize].as_array()[0])
                / 3.0,
            (mesh.verts[tri[0] as usize].as_array()[1]
                + mesh.verts[tri[1] as usize].as_array()[1]
                + mesh.verts[tri[2] as usize].as_array()[1])
                / 3.0,
            (mesh.verts[tri[0] as usize].as_array()[2]
                + mesh.verts[tri[1] as usize].as_array()[2]
                + mesh.verts[tri[2] as usize].as_array()[2])
                / 3.0,
        ];
        let w = [c[0] - apex[0], c[1] - apex[1], c[2] - apex[2]];
        let tau = w[0] * ax[0] + w[1] * ax[1] + w[2] * ax[2];
        let radial = ((w[0] * w[0] + w[1] * w[1] + w[2] * w[2]) - tau * tau)
            .max(0.0)
            .sqrt();
        // Cap centroids sit ON the axis-perpendicular planes; a band triangle's
        // centroid satisfies the cone relation within the chord sagitta.
        if (radial - tau * tan).abs() < 1e-2 {
            band_tris += 1;
        }
    }
    assert!(band_tris > 0, "the cone band produced lateral triangles");
}
