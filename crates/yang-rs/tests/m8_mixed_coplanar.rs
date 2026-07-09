//! M8-mixed — Stage-0 overlay admission for mixed Line+Arc planar faces.
//! Spec: `specs/m8_mixed_loop_coplanar_overlay.md`.
//!
//! A planar face whose loops mix `Curve::LineSegment` and `Curve::Circle`
//! edges (a half-cylinder cap: diameter segment + semicircle arc; a cylinder
//! cap with a polygonal through-bore: full-circle outer + 4-segment hole)
//! participates in a §4.5.5 coplanar A×B pair. Today `overlay_face_supported`
//! (`yang-rs/src/stage0.rs`) admits only all-`LineSegment` planar faces,
//! hole-free single-circle discs, or annular discs, so a MIXED face falls
//! through to the loud `CoplanarFacesUnsupported` wall (the `face-unsupported`
//! Stage-0 residue — R0021 R0026 R0051 R0059 F0075). These tests assert the
//! no-arc-crossing pairs replay to oracle-correct geometry; they are RED until
//! the mixed arm lands.
//!
//! Slice 1 covers pairs whose overlap boundary does NOT subdivide any curved
//! sub-chord of the mixed face. A curved-chord crossing stays the loud
//! residue (pinned below, not silent).

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

/// A z-axis solid cylinder (bottom cap on `z = base_z`, extruded +z) —
/// the disc partner for the mixed×disc adversary.
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

/// A HALF-CYLINDER: radius `r`, z ∈ [z0, z1], flat wall in the plane y = 0,
/// material y ≥ 0. Caps are the minimal MIXED planar loop — one diameter
/// `LineSegment` + one semicircle `Curve::Circle` arc. The curved wall is the
/// 2-arc partial-strip cylinder lateral.
fn half_cylinder(r: f64, z0: f64, z1: f64) -> BRep {
    let verts = vec![
        BRepVertex { point: p(r, 0.0, z0) },  // 0: bottom +x
        BRepVertex { point: p(-r, 0.0, z0) }, // 1: bottom −x
        BRepVertex { point: p(r, 0.0, z1) },  // 2: top +x
        BRepVertex { point: p(-r, 0.0, z1) }, // 3: top −x
    ];
    // Arc parameterization is CCW about the circle `normal` starting at
    // `start`: bottom (normal −z) from vertex 1 sweeps (−r,0)→(0,r)→(r,0),
    // the y ≥ 0 half; top (normal +z) from vertex 2 sweeps (r,0)→(0,r)→(−r,0).
    let edges = vec![
        // 0: bottom semicircle arc (v1 → v0 through (0, r, z0)).
        BRepEdge {
            start: 1,
            end: 0,
            curve: Curve::Circle {
                center: p(0.0, 0.0, z0),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius: r,
            },
        },
        // 1: top semicircle arc (v2 → v3 through (0, r, z1)).
        BRepEdge {
            start: 2,
            end: 3,
            curve: Curve::Circle {
                center: p(0.0, 0.0, z1),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
        },
        // 2: bottom diameter segment (v0 → v1), used by the bottom cap loop.
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
        // 3: top diameter segment (v2 → v3), used by the top cap loop.
        BRepEdge {
            start: 2,
            end: 3,
            curve: Curve::LineSegment,
        },
        // 4: vertical seam at +x (v0 → v2), used by the curved wall loop.
        BRepEdge {
            start: 0,
            end: 2,
            curve: Curve::LineSegment,
        },
        // 5: vertical seam at −x (v1 → v3), used by the curved wall loop.
        BRepEdge {
            start: 1,
            end: 3,
            curve: Curve::LineSegment,
        },
        // 6–9: the flat wall's OWN directed rectangle v1→v0→v2→v3→v1
        // (all-`LineSegment` planar loops need head-to-tail directed edges;
        // the box builder duplicates directed edges per face the same way).
        BRepEdge {
            start: 1,
            end: 0,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 0,
            end: 2,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 2,
            end: 3,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 3,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![
        // Bottom cap (normal −z): arc v1→v0, diameter v0→v1. MIXED loop.
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: z0,
            },
            outer_loop: vec![0, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // Top cap (normal +z): arc v2→v3, diameter v3→v2. MIXED loop —
        // the face under test.
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -z1,
            },
            outer_loop: vec![1, 3],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // Flat wall (normal −y): its own directed rectangle v1→v0→v2→v3.
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, -1.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![6, 7, 8, 9],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // Curved wall: 2-arc partial cylinder strip (arc b, +x seam, arc t,
        // −x seam).
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, z0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 4, 1, 5],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("half_cylinder BRep::new")
}

/// A z-axis cylinder with a SQUARE through-bore (the R0059 cap shape): caps
/// are a full-`Curve::Circle` outer loop + a 4-`LineSegment` inner loop —
/// mixed (not annular: the hole is polygonal).
fn square_bore_cylinder(r: f64, half: f64, z0: f64, z1: f64) -> BRep {
    let verts = vec![
        BRepVertex { point: p(r, 0.0, z0) },       // 0: outer seam bottom
        BRepVertex { point: p(r, 0.0, z1) },       // 1: outer seam top
        BRepVertex { point: p(half, half, z0) },   // 2
        BRepVertex { point: p(-half, half, z0) },  // 3
        BRepVertex { point: p(-half, -half, z0) }, // 4
        BRepVertex { point: p(half, -half, z0) },  // 5
        BRepVertex { point: p(half, half, z1) },   // 6
        BRepVertex { point: p(-half, half, z1) },  // 7
        BRepVertex { point: p(-half, -half, z1) }, // 8
        BRepVertex { point: p(half, -half, z1) },  // 9
    ];
    let seg = |start: u32, end: u32| BRepEdge {
        start,
        end,
        curve: Curve::LineSegment,
    };
    let edges = vec![
        // 0: outer rim bottom (full circle), 1: outer rim top.
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
        // 2: outer lateral seam.
        seg(0, 1),
        // 3–6: bore square, bottom (2→3→4→5→2).
        seg(2, 3),
        seg(3, 4),
        seg(4, 5),
        seg(5, 2),
        // 7–10: bore square, top (6→7→8→9→6).
        seg(6, 7),
        seg(7, 8),
        seg(8, 9),
        seg(9, 6),
        // 11–14: +y bore wall's own directed rectangle 2→6→7→3→2.
        seg(2, 6),
        seg(6, 7),
        seg(7, 3),
        seg(3, 2),
        // 15–18: −x bore wall, 3→7→8→4→3.
        seg(3, 7),
        seg(7, 8),
        seg(8, 4),
        seg(4, 3),
        // 19–22: −y bore wall, 4→8→9→5→4.
        seg(4, 8),
        seg(8, 9),
        seg(9, 5),
        seg(5, 4),
        // 23–26: +x bore wall, 5→9→6→2→5.
        seg(5, 9),
        seg(9, 6),
        seg(6, 2),
        seg(2, 5),
    ];
    // Bore walls: the SOLID's outward normal points INTO the hole (toward the
    // bore axis), like the tube bore's reversed cylinder — expressed directly
    // in the plane normal (planar faces carry their sense in the normal).
    let bore_wall = |e0: u32, n: [f64; 3], d: f64| BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(n[0], n[1], n[2]),
            d,
        },
        outer_loop: vec![e0, e0 + 1, e0 + 2, e0 + 3],
        inner_loops: Vec::new(),
        reversed: false,
    };
    let faces = vec![
        // Outer cylinder lateral.
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(0.0, 0.0, z0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        // Bottom cap (normal −z): circle outer + square hole. MIXED face.
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: z0,
            },
            outer_loop: vec![0],
            inner_loops: vec![vec![3, 4, 5, 6]],
            reversed: false,
        },
        // Top cap (normal +z): circle outer + square hole. MIXED face —
        // the face under test.
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -z1,
            },
            outer_loop: vec![1],
            inner_loops: vec![vec![7, 8, 9, 10]],
            reversed: false,
        },
        // +y wall (plane y = +half, outward −y into the hole).
        bore_wall(11, [0.0, -1.0, 0.0], half),
        // −x wall (plane x = −half, outward +x into the hole).
        bore_wall(15, [1.0, 0.0, 0.0], half),
        // −y wall (plane y = −half, outward +y into the hole).
        bore_wall(19, [0.0, 1.0, 0.0], half),
        // +x wall (plane x = +half, outward −x into the hole).
        bore_wall(23, [-1.0, 0.0, 0.0], half),
    ];
    BRep::new(verts, edges, faces).expect("square_bore_cylinder BRep::new")
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

// ───────────────────────────── fixture sanity ─────────────────────────────

/// Fixture validity (GREEN today): the half-cylinder itself must build and
/// mesh to a watertight outward solid of ≈ π/2·r²·h. A failure here is a
/// fixture bug, not the feature's RED.
#[test]
fn half_cylinder_fixture_builds() {
    let hc = half_cylinder(1.0, 0.0, 1.0);
    let mesh = hc.as_mesh();
    assert!(is_watertight(mesh), "half-cylinder mesh must be watertight");
    assert!(is_outward_solid(mesh), "half-cylinder must be outward");
    let vol = signed_volume(mesh);
    let analytic = std::f64::consts::FRAC_PI_2;
    assert!(
        (vol - analytic).abs() / analytic < 0.05,
        "half-cylinder volume {vol} not within chord band of {analytic}"
    );
}

/// Fixture validity (GREEN today): the square-bore cylinder meshes to a
/// watertight outward solid of ≈ (π·r² − (2·half)²)·h.
#[test]
fn square_bore_cylinder_fixture_builds() {
    let sb = square_bore_cylinder(1.0, 0.3, 0.0, 1.0);
    let mesh = sb.as_mesh();
    assert!(is_watertight(mesh), "square-bore mesh must be watertight");
    assert!(is_outward_solid(mesh), "square-bore must be outward");
    let vol = signed_volume(mesh);
    let analytic = std::f64::consts::PI - 0.36;
    assert!(
        (vol - analytic).abs() / analytic < 0.05,
        "square-bore volume {vol} not within chord band of {analytic}"
    );
}

// ───────────────────────────── tests (RED) ─────────────────────────────

/// CANONICAL (branch 1, mixed × all-segment): a box stacked flush on the
/// half-cylinder's mixed top cap, strictly inside the straight (diameter-side)
/// region — the overlap boundary touches no curved chord. Union must SUCCEED
/// (today: the `face-unsupported` Stage-0 wall) as one watertight outward
/// solid spanning both bodies.
#[test]
fn mixed_cap_flush_union_succeeds() {
    let hc = half_cylinder(1.0, 0.0, 1.0);
    // Bottom face z=1 coplanar with the mixed top cap; footprint corner
    // radius max √(0.4² + 0.45²) ≈ 0.60 ≪ 1 (arc untouched), y ≥ 0.05 > 0.
    let lid = box_brep([-0.4, 0.05, 1.0], [0.4, 0.45, 2.0]);
    let out = boolean(&hc, &lid, BoolOp::Union, &nb())
        .expect("mixed-cap coplanar overlay (no arc crossing) must be handled by Stage 0");
    let mesh = out.as_mesh();
    assert!(is_watertight(mesh), "union must be a closed 2-manifold");
    assert!(is_outward_solid(mesh), "union must be outward-oriented");
    let (min_z, max_z) = mesh.verts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), v| {
        (lo.min(v.z()), hi.max(v.z()))
    });
    assert!(
        (max_z - 2.0).abs() < 1e-6 && min_z.abs() < 1e-6,
        "union must span z∈[0,2] (min {min_z}, max {max_z})"
    );
    // Volume ≈ π/2 (half-cyl) + 0.8·0.4·1 (box). The discretized semicircle
    // under-fills; the chord band rejects a dropped cap or doubled sheet.
    let vol = signed_volume(mesh).abs();
    let analytic = std::f64::consts::FRAC_PI_2 + 0.32;
    assert!(
        (vol - analytic).abs() / analytic < 0.05,
        "union volume {vol} not within chord band of analytic {analytic}"
    );
}

/// Branch 2 (R0059 shape, straight-edge splits on a mixed face): a wide lid
/// box whose bottom face fully contains the square-bore cylinder's mixed top
/// cap (full-circle outer + polygonal hole). The bore's straight hole edges
/// lie interior to the lid region (legitimate `collect_edge_splits` traffic);
/// the outer circle is strictly inside the lid footprint (no curved chord
/// subdivided). Union must SUCCEED; the bore becomes a blind pocket.
#[test]
fn square_bore_cap_under_lid_union_succeeds() {
    let sb = square_bore_cylinder(1.0, 0.3, 0.0, 1.0);
    let lid = box_brep([-2.0, -2.0, 1.0], [2.0, 2.0, 1.5]);
    let out = boolean(&sb, &lid, BoolOp::Union, &nb())
        .expect("mixed cap (circle outer + polygonal hole) must be handled by Stage 0");
    let mesh = out.as_mesh();
    assert!(is_watertight(mesh), "union must be a closed 2-manifold");
    assert!(is_outward_solid(mesh), "union must be outward-oriented");
    let (min_z, max_z) = mesh.verts.iter().fold((f64::MAX, f64::MIN), |(lo, hi), v| {
        (lo.min(v.z()), hi.max(v.z()))
    });
    assert!(
        (max_z - 1.5).abs() < 1e-6 && min_z.abs() < 1e-6,
        "union must span z∈[0,1.5] (min {min_z}, max {max_z})"
    );
    // Volume ≈ (π − 0.36) (bored cylinder) + 16·0.5 (lid).
    let vol = signed_volume(mesh).abs();
    let analytic = (std::f64::consts::PI - 0.36) + 8.0;
    assert!(
        (vol - analytic).abs() / analytic < 0.05,
        "union volume {vol} not within chord band of analytic {analytic}"
    );
}

/// ADVERSARY (branch 4, the §6 chord-geometry hazard): a small DISC partner
/// (cylinder bottom cap) coplanar on the mixed top cap, strictly inside the
/// material and away from both the arc and the diameter. The disc fast path
/// must NOT chord-approximate the mixed face; the pair routes through the
/// general overlay and must produce the exact stacked volume — or fail typed,
/// never silently wrong.
#[test]
fn mixed_cap_under_disc_union_succeeds() {
    let hc = half_cylinder(1.0, 0.0, 1.0);
    // Disc r=0.2 at (0, 0.45): max radius from axis 0.65 < 1, y ≥ 0.25 > 0.
    let peg = z_cylinder(0.0, 0.45, 1.0, 0.2, 1.0);
    let out = boolean(&hc, &peg, BoolOp::Union, &nb())
        .expect("mixed-cap ∩ disc coplanar overlay (containment) must be handled by Stage 0");
    let mesh = out.as_mesh();
    assert!(is_watertight(mesh), "union must be a closed 2-manifold");
    assert!(is_outward_solid(mesh), "union must be outward-oriented");
    let vol = signed_volume(mesh).abs();
    let analytic = std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * 0.04;
    assert!(
        (vol - analytic).abs() / analytic < 0.05,
        "union volume {vol} not within chord band of analytic {analytic}"
    );
}

/// SLICE BOUNDARY (branch 3, must stay loud): a box whose footprint edge
/// CROSSES the semicircle arc. The crossing subdivides a curved sub-chord of
/// the mixed face — out of slice-1 scope; Stage 0 must keep the loud
/// `CoplanarFacesUnsupported` residue (P9), not silently emit chord geometry.
#[test]
fn mixed_cap_arc_crossing_stays_loud() {
    let hc = half_cylinder(1.0, 0.0, 1.0);
    // Footprint x∈[0.5,1.5], y∈[0.1,0.6]: corner (0.5,0.1) is inside the
    // half-disc (r≈0.51), the +x side is outside — the boundary crosses the arc.
    let lid = box_brep([0.5, 0.1, 1.0], [1.5, 0.6, 2.0]);
    let res = boolean(&hc, &lid, BoolOp::Union, &nb());
    assert!(
        res.is_err(),
        "curved-chord crossing is out of slice-1 scope and must stay a loud \
         residue, not silently produce geometry"
    );
}
