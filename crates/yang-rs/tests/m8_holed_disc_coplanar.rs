//! M8 — Holed-disc coplanar overlay (Stage-0 generalization).
//! Spec: `specs/m8_holed_disc_coplanar_overlay.md`.
//!
//! An ANNULAR planar cap (outer `Curve::Circle` + one circular-hole inner loop)
//! participates in a §4.5.5 coplanar A×B pair. Today `overlay_face_supported`
//! (`yang-rs/src/stage0.rs`) admits only all-`LineSegment` planar faces or a
//! hole-free single-circle disc, so an annular cap falls through to the loud
//! `CoplanarFacesUnsupported` wall (the dominant `face-unsupported` Stage-0
//! residue — the swiss-cheese family F0086–F0090). These tests assert the
//! ISOLATED (non-chained) holed-disc pair replays to oracle-correct geometry;
//! they are RED until the annular arm lands.
//!
//! Increment 1 covers pure CONTAINMENT (the overlap does not cross the hole
//! rim). A crossing stays the loud residue (pinned below, not silent).

use cad_primitives::{BoolOp, Point3, Vector3};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Axis-aligned box B-Rep [lo, hi] (yr24/yr26/m8_disc_coplanar hexahedron).
fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let v = |x: f64, y: f64, z: f64| BRepVertex { point: p(x, y, z) };
    let vertices = vec![
        v(lo[0], lo[1], lo[2]),
        v(hi[0], lo[1], lo[2]),
        v(hi[0], hi[1], lo[2]),
        v(lo[0], hi[1], lo[2]),
        v(hi[0], hi[1], hi[2]),
        v(hi[0], lo[1], hi[2]),
        v(lo[0], lo[1], hi[2]),
        v(lo[0], hi[1], hi[2]),
    ];
    const EDGE_PAIRS: [(u32, u32); 24] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (2, 1),
        (1, 5),
        (5, 4),
        (4, 2),
        (3, 2),
        (2, 4),
        (4, 7),
        (7, 3),
        (0, 3),
        (3, 7),
        (7, 6),
        (6, 0),
        (1, 0),
        (0, 6),
        (6, 5),
        (5, 1),
    ];
    let edges: Vec<BRepEdge> = EDGE_PAIRS
        .iter()
        .map(|&(start, end)| BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        })
        .collect();
    let planes: [([f64; 3], f64); 6] = [
        ([0.0, 0.0, -1.0], lo[2]),
        ([0.0, 0.0, 1.0], -hi[2]),
        ([1.0, 0.0, 0.0], -hi[0]),
        ([0.0, 1.0, 0.0], -hi[1]),
        ([-1.0, 0.0, 0.0], lo[0]),
        ([0.0, -1.0, 0.0], lo[1]),
    ];
    let faces: Vec<BRepFace> = planes
        .iter()
        .enumerate()
        .map(|(i, &(n, d))| BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(n[0], n[1], n[2]),
                d,
            },
            outer_loop: (4 * i as u32..4 * i as u32 + 4).collect(),
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(vertices, edges, faces).expect("box BRep::new")
}

/// A z-axis solid cylinder (bottom cap on `z = base_z`, extruded +z).
fn z_cylinder(cx: f64, cy: f64, base_z: f64, radius: f64, height: f64) -> BRep {
    let bottom = [cx, cy, base_z];
    let top = [cx, cy, base_z + height];
    let v0 = p(cx + radius, cy, base_z);
    let v1 = p(cx + radius, cy, base_z + height);
    let verts = vec![BRepVertex { point: v0 }, BRepVertex { point: v1 }];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(bottom[0], bottom[1], bottom[2]),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top[0], top[1], top[2]),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius,
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
                axis_point: p(bottom[0], bottom[1], bottom[2]),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: base_z,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -(base_z + height),
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("z_cylinder BRep::new")
}

/// A z-axis annular TUBE: outer radius `ro`, coaxial bore `ri`, z ∈ [z0, z1].
/// Both caps are ANNULAR — outer `Curve::Circle` outer loop + a single bore
/// `Curve::Circle` inner loop (the holed-disc face under test). Mirrors the
/// `m8cyl_plug_in_bore::tube` topology.
fn tube(ro: f64, ri: f64, z0: f64, z1: f64) -> BRep {
    let verts = vec![
        BRepVertex {
            point: p(ro, 0.0, z0),
        }, // 0 outer bottom
        BRepVertex {
            point: p(ro, 0.0, z1),
        }, // 1 outer top
        BRepVertex {
            point: p(ri, 0.0, z0),
        }, // 2 bore bottom
        BRepVertex {
            point: p(ri, 0.0, z1),
        }, // 3 bore top
    ];
    let mut edges: Vec<BRepEdge> = Vec::new();
    let outer_rim_b = edges.len() as u32;
    edges.push(BRepEdge {
        start: 0,
        end: 0,
        curve: Curve::Circle {
            center: p(0.0, 0.0, z0),
            normal: Vector3::new(0.0, 0.0, -1.0),
            radius: ro,
        },
    });
    let outer_rim_t = edges.len() as u32;
    edges.push(BRepEdge {
        start: 1,
        end: 1,
        curve: Curve::Circle {
            center: p(0.0, 0.0, z1),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: ro,
        },
    });
    let outer_seam = edges.len() as u32;
    edges.push(BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::LineSegment,
    });
    let bore_rim_b = edges.len() as u32;
    edges.push(BRepEdge {
        start: 2,
        end: 2,
        curve: Curve::Circle {
            center: p(0.0, 0.0, z0),
            normal: Vector3::new(0.0, 0.0, -1.0),
            radius: ri,
        },
    });
    let bore_rim_t = edges.len() as u32;
    edges.push(BRepEdge {
        start: 3,
        end: 3,
        curve: Curve::Circle {
            center: p(0.0, 0.0, z1),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: ri,
        },
    });
    let bore_seam = edges.len() as u32;
    edges.push(BRepEdge {
        start: 2,
        end: 3,
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
        // Bore cylinder wall (cavity, inward).
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
        // Annular bottom cap (normal −z): outer rim CCW-from-below, bore hole.
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: z0,
            },
            outer_loop: vec![outer_rim_b],
            inner_loops: vec![vec![bore_rim_b]],
            reversed: false,
        },
        // Annular top cap (normal +z): the holed-disc face under test.
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

// ───────────────────────────── oracles ─────────────────────────────

fn signed_volume(mesh: &Mesh) -> f64 {
    mesh.tris
        .iter()
        .map(|t| {
            let a = mesh.verts[t[0] as usize];
            let b = mesh.verts[t[1] as usize];
            let c = mesh.verts[t[2] as usize];
            (a.x() * (b.y() * c.z() - b.z() * c.y()) - a.y() * (b.x() * c.z() - b.z() * c.x())
                + a.z() * (b.x() * c.y() - b.y() * c.x()))
                / 6.0
        })
        .sum()
}

fn is_watertight(mesh: &Mesh) -> bool {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in &mesh.tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    !counts.is_empty() && counts.values().all(|&c| c == 2)
}

fn is_consistently_oriented(mesh: &Mesh) -> bool {
    use std::collections::HashMap;
    let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
    for t in &mesh.tris {
        for k in 0..3 {
            *directed.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
        }
    }
    directed.values().all(|&c| c == 1)
}

fn is_outward_solid(mesh: &Mesh) -> bool {
    is_consistently_oriented(mesh) && signed_volume(mesh) > 0.0
}

fn nb() -> impl yang_rs::MeshBoolean {
    yang_rs::native_backend().expect("native backend always available")
}

// ───────────────────────────── tests ─────────────────────────────

/// CANONICAL (containment, annular ⊆ polygon): a tube (ro=1, bore ri=0.4),
/// annular top cap at z=2, capped by a wide lid box whose bottom face (z=2)
/// fully contains the annulus. The annular cap is coplanar with the lid bottom
/// (opposite normals). Union must SUCCEED (currently the `face-unsupported`
/// Stage-0 wall) — one watertight, outward solid. The bore becomes a blind
/// pocket open at the bottom; the union must still reach the lid top (z=3),
/// proving the annular cap fused rather than being dropped.
#[test]
fn annular_cap_in_polygon_union_succeeds() {
    let t = tube(1.0, 0.4, 0.0, 2.0);
    let lid = box_brep([-2.0, -2.0, 2.0], [2.0, 2.0, 3.0]);
    let out = boolean(&t, &lid, BoolOp::Union, &nb())
        .expect("annular-cap coplanar overlay (containment) must be handled by Stage 0");
    let mesh = out.as_mesh();
    assert!(
        is_watertight(mesh),
        "union output must be a closed 2-manifold"
    );
    assert!(
        is_outward_solid(mesh),
        "union must be consistently outward-oriented (no flipped patch)"
    );
    let (min_z, max_z) = mesh.verts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), v| {
        (lo.min(v.z()), hi.max(v.z()))
    });
    assert!(
        (max_z - 3.0).abs() < 1e-6 && (min_z - 0.0).abs() < 1e-6,
        "union must span the tube+lid stack z∈[0,3] (min {min_z}, max {max_z})"
    );
    // Volume ≈ tube annulus π(ro²−ri²)·h + lid (4·4·1). Discretized annulus
    // under-fills; a generous chord band rejects a dropped-cap or doubled sheet.
    let vol = signed_volume(mesh).abs();
    let analytic = std::f64::consts::PI * (1.0 - 0.16) * 2.0 + 16.0;
    assert!(
        (vol - analytic).abs() / analytic < 0.08,
        "union volume {vol} not within chord band of analytic {analytic}"
    );
}

/// CONTAINMENT, disc partner (annular ∩ disc): a solid cap (r=1.2) sits on the
/// tube's annular top cap (ro=1.5, bore 0.5) at z=2; the disc strictly contains
/// the bore hole and lies inside the outer rim (annular overlap, no crossing).
/// Exercises the disc-partner overlay arm symmetric to the polygon partner.
/// GREEN since increment 3 (2026-07-06): the wall was never the sub-annulus
/// arrangement itself — Stage 0 emitted a self-intersecting mesh at ULP-twin
/// rim splits (f64 angle-tie ordering) and the Stage-1 cap CDT misclassified
/// twin femto-slivers (f64 centroid parity). Exact ring ordering + flood-fill
/// CDT with exact hole parity fixed it; see spec §8 increment 3.
#[test]
fn annular_cap_under_disc_union_succeeds() {
    let t = tube(1.5, 0.5, 0.0, 2.0);
    let cap = z_cylinder(0.0, 0.0, 2.0, 1.2, 1.0); // bottom disc at z=2
    let out = boolean(&t, &cap, BoolOp::Union, &nb())
        .expect("annular-cap ∩ disc coplanar overlay (containment) must be handled by Stage 0");
    let mesh = out.as_mesh();
    assert!(
        is_watertight(mesh),
        "union output must be a closed 2-manifold"
    );
    assert!(is_outward_solid(mesh), "union must be outward-oriented");
}

/// INCREMENT BOUNDARY (must stay loud): a partner disc whose rim CROSSES the
/// bore hole rim (r between the two annulus radii, off-centre so the rims
/// intersect) needs arc∩arc crossing + bore-rim split propagation — deferred.
/// Stage 0 must keep the loud `CoplanarFacesUnsupported` residue (P9), not a
/// silent-wrong result. Pins the increment-1 boundary.
#[test]
fn annular_cap_hole_crossing_stays_loud() {
    let t = tube(1.5, 0.5, 0.0, 2.0);
    // Disc r=0.6 centred at (0.4,0): its rim crosses the bore rim (r=0.5).
    let cap = z_cylinder(0.4, 0.0, 2.0, 0.6, 1.0);
    let res = boolean(&t, &cap, BoolOp::Union, &nb());
    assert!(
        res.is_err(),
        "hole-rim crossing is out of increment-1 scope and must stay a loud \
         residue, not silently produce geometry"
    );
}
