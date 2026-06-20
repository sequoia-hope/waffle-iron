//! M8-cyl Increment 1 GATE (the yang-rs sibling of
//! `crates/cherchi-rs/tests/task28_plug_in_bore.rs`).
//!
//! task28 proved the RAW mesh boolean (native == C++ sidecar) is NON-watertight
//! on an opposite-normal coincident-cylinder wall pair, even with conformal z.
//! THIS test runs the FULL yang `boolean()` — which adds Stage-0 coincident-
//! cylinder re-tessellation + the §4.5.5 membrane (overlap-sheet) drop — and
//! asserts the union becomes WATERTIGHT.
//!
//! Fixture: a TUBE (annular prism, bore wall faces INWARD, an analytic
//! `Surface::Cylinder` cavity) unioned with a coaxial PLUG (solid cylinder, the
//! SAME radius, OUTWARD wall) that fills the bore — exactly the gear's
//! bore/flange pair, minimal form. The plug is shorter than the tube (z⊂z), so
//! the coincident-cylinder overlap is a z-band strictly inside both.

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeling::NativeBoolean;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Count directed half-edges with no opposite (welded by bit-exact coords).
/// Watertight ⇒ 0 unpaired.
fn unpaired_half_edges(mesh: &Mesh) -> usize {
    use std::collections::BTreeMap;
    let mut index: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    let mut remap: Vec<u32> = Vec::with_capacity(mesh.verts.len());
    for v in &mesh.verts {
        let key = [v.x().to_bits(), v.y().to_bits(), v.z().to_bits()];
        let next = index.len() as u32;
        let id = *index.entry(key).or_insert(next);
        remap.push(id);
    }
    let mut edge_dirs: BTreeMap<(u32, u32), i64> = BTreeMap::new();
    for t in &mesh.tris {
        for k in 0..3 {
            let (a, b) = (remap[t[k] as usize], remap[t[(k + 1) % 3] as usize]);
            *edge_dirs.entry((a.min(b), a.max(b))).or_insert(0) += if a < b { 1 } else { -1 };
        }
    }
    edge_dirs.values().filter(|&&c| c != 0).count()
}

fn signed_volume(mesh: &Mesh) -> f64 {
    mesh.tris
        .iter()
        .map(|t| {
            let a = mesh.verts[t[0] as usize];
            let b = mesh.verts[t[1] as usize];
            let c = mesh.verts[t[2] as usize];
            (a.x() * (b.y() * c.z() - c.y() * b.z()) - a.y() * (b.x() * c.z() - c.x() * b.z())
                + a.z() * (b.x() * c.y() - c.x() * b.y()))
                / 6.0
        })
        .sum()
}

/// Build a TUBE as a WASHER: an OUTER analytic `Surface::Cylinder` wall
/// (radius `ro`, outward) and an INNER bore analytic `Surface::Cylinder` wall
/// (radius `ri`, `reversed = true`, inward), z∈[z0,z1], with two ANNULAR planar
/// caps (a full-circle outer rim + a full-circle bore hole). The bore wall is
/// the coincident cylinder with the plug. `n` is unused (analytic walls
/// tessellate from the shared chord bound) — kept for signature symmetry.
fn tube(_n: usize, ro: f64, ri: f64, z0: f64, z1: f64) -> BRep {
    let mut verts: Vec<BRepVertex> = Vec::new();
    // Seam vertices: outer bottom/top, bore bottom/top.
    let ob = verts.len() as u32;
    verts.push(BRepVertex {
        point: p(ro, 0.0, z0),
    });
    let ot = verts.len() as u32;
    verts.push(BRepVertex {
        point: p(ro, 0.0, z1),
    });
    let ib = verts.len() as u32;
    verts.push(BRepVertex {
        point: p(ri, 0.0, z0),
    });
    let it = verts.len() as u32;
    verts.push(BRepVertex {
        point: p(ri, 0.0, z1),
    });

    let mut edges: Vec<BRepEdge> = Vec::new();
    // Outer rims (closed circles), bottom normal -z, top +z.
    let outer_rim_b = edges.len() as u32;
    edges.push(BRepEdge {
        start: ob,
        end: ob,
        curve: Curve::Circle {
            center: p(0.0, 0.0, z0),
            normal: Vector3::new(0.0, 0.0, -1.0),
            radius: ro,
        },
    });
    let outer_rim_t = edges.len() as u32;
    edges.push(BRepEdge {
        start: ot,
        end: ot,
        curve: Curve::Circle {
            center: p(0.0, 0.0, z1),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: ro,
        },
    });
    let outer_seam = edges.len() as u32;
    edges.push(BRepEdge {
        start: ob,
        end: ot,
        curve: Curve::LineSegment,
    });
    // Bore rims (closed circles).
    let bore_rim_b = edges.len() as u32;
    edges.push(BRepEdge {
        start: ib,
        end: ib,
        curve: Curve::Circle {
            center: p(0.0, 0.0, z0),
            normal: Vector3::new(0.0, 0.0, -1.0),
            radius: ri,
        },
    });
    let bore_rim_t = edges.len() as u32;
    edges.push(BRepEdge {
        start: it,
        end: it,
        curve: Curve::Circle {
            center: p(0.0, 0.0, z1),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: ri,
        },
    });
    let bore_seam = edges.len() as u32;
    edges.push(BRepEdge {
        start: ib,
        end: it,
        curve: Curve::LineSegment,
    });

    let faces = vec![
        // Outer cylinder wall (outward).
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: ro,
            },
            outer_loop: vec![outer_rim_b, outer_seam, outer_rim_t, outer_seam],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // Bore cylinder wall (cavity, reversed/inward).
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: ri,
            },
            outer_loop: vec![bore_rim_b, bore_seam, bore_rim_t, bore_seam],
            inner_loops: Vec::new(),
            reversed: true,
        },
        // Bottom annular cap (normal -z): outer = outer rim, hole = bore rim.
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: z0,
            },
            outer_loop: vec![outer_rim_b],
            inner_loops: vec![vec![bore_rim_b]],
            reversed: false,
        },
        // Top annular cap (normal +z): outer = outer rim, hole = bore rim.
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -z1,
            },
            outer_loop: vec![outer_rim_t],
            inner_loops: vec![vec![bore_rim_t]],
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces).expect("tube BRep::new")
}

/// Build a solid PLUG: a cylinder of radius `r`, z∈[z0,z1], OUTWARD wall (an
/// analytic `Surface::Cylinder`), with circular disc caps.
fn plug(r: f64, z0: f64, z1: f64) -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(r, 0.0, z0),
        },
        BRepVertex {
            point: p(r, 0.0, z1),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(0.0, 0.0, z0),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius: r,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(0.0, 0.0, z1),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: z0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -z1,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("plug BRep::new")
}

#[test]
fn plug_in_bore_union_is_watertight() {
    let n = 12;
    let (ro, ri) = (3.0, 1.0);
    let tube = tube(n, ro, ri, -5.0, 5.0);
    let plug = plug(ri, -2.0, 2.0);

    eprintln!(
        "tube: unpaired={} vol={:.4}; plug: unpaired={} vol={:.4}",
        unpaired_half_edges(tube.as_mesh()),
        signed_volume(tube.as_mesh()),
        unpaired_half_edges(plug.as_mesh()),
        signed_volume(plug.as_mesh()),
    );

    let out = boolean(&tube, &plug, BoolOp::Union, &NativeBoolean).expect("union should build");
    let mesh = out.as_mesh();
    let u = unpaired_half_edges(mesh);
    let v = signed_volume(mesh);
    eprintln!(
        "UNION: unpaired={u} watertight={} vol={v:.4} tris={} verts={}",
        u == 0,
        mesh.tris.len(),
        mesh.verts.len()
    );
    assert_eq!(
        u, 0,
        "coincident-cylinder union must be watertight (Stage-0 re-tess + membrane drop)"
    );
    assert!(v > 0.0, "positive volume");

    // Winding consistency: every directed edge's two incident triangles must
    // traverse it in OPPOSITE directions (an orientable, consistently-wound
    // 2-manifold). This catches a flipped patch the watertight (undirected)
    // count would miss.
    use std::collections::BTreeMap;
    let mut index: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    let mut remap: Vec<u32> = Vec::with_capacity(mesh.verts.len());
    for vtx in &mesh.verts {
        let key = [vtx.x().to_bits(), vtx.y().to_bits(), vtx.z().to_bits()];
        let next = index.len() as u32;
        remap.push(*index.entry(key).or_insert(next));
    }
    let mut dir: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    for t in &mesh.tris {
        for k in 0..3 {
            let (a, b) = (remap[t[k] as usize], remap[t[(k + 1) % 3] as usize]);
            *dir.entry((a, b)).or_insert(0) += 1;
        }
    }
    let non_orientable = dir
        .iter()
        .filter(|((a, b), &c)| c > 1 || (a < b && !dir.contains_key(&(*b, *a))))
        .count();
    assert_eq!(
        non_orientable, 0,
        "coincident-cylinder union must be consistently wound (orientable manifold)"
    );
}
