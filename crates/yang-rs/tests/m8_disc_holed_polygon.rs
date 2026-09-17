//! M8 — disc ∩ HOLED line-polygon coplanar pair (Stage-0 routing).
//! Spec: `specs/m8_disc_holed_polygon_overlay.md`.
//!
//! A flat disc cap (a cylinder end) lies on a planar all-`LineSegment` face
//! that carries an inner loop — the face a through-cut leaves behind. The
//! disc-pair fast path (`stage0::disc_pair::build_disc_pair`) is a convex
//! containment builder and cannot express a hole, so it returned
//! `Wall("disc-poly-holed")` and Stage 0 raised the loud
//! `CoplanarFacesUnsupported` residue (corpus R0070, `disc-poly-holed |
//! pair=(133,0)`). The general §4.5.5 overlay already consumes a
//! `PolygonWithHoles` (`face_polygon_2d` projects every inner loop), so the
//! pair now routes there like a non-convex or crossing partner does.
//!
//! Every test is a `boolean()` round trip on an ISOLATED pair with the
//! standard oracles: watertight, outward, and volume against the analytic
//! value within the chord band the sibling M8 suites use.

use cad_primitives::{BoolOp, Point3, Vector3};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Axis-aligned box `[lo, hi]` with a rectangular THROUGH-hole `[hlo, hhi]`
/// (in x/y) along z. Ten planar faces: two holed caps, four outer walls, four
/// hole walls (the cavity, wound so the face normal points INTO the hole —
/// i.e. outward from the material). Every loop is all-`LineSegment`, each
/// face owns its own directed edges (the `box_brep` convention of the sibling
/// M8 suites).
fn holed_box(lo: [f64; 3], hi: [f64; 3], hlo: [f64; 2], hhi: [f64; 2]) -> BRep {
    let mut vertices: Vec<BRepVertex> = Vec::new();
    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut faces: Vec<BRepFace> = Vec::new();
    let mut vert = |x: f64, y: f64, z: f64| -> u32 {
        vertices.push(BRepVertex { point: p(x, y, z) });
        (vertices.len() - 1) as u32
    };
    // Outer corners: bottom 0..4, top 4..8 (CCW seen from +z).
    let ob = [
        vert(lo[0], lo[1], lo[2]),
        vert(hi[0], lo[1], lo[2]),
        vert(hi[0], hi[1], lo[2]),
        vert(lo[0], hi[1], lo[2]),
    ];
    let ot = [
        vert(lo[0], lo[1], hi[2]),
        vert(hi[0], lo[1], hi[2]),
        vert(hi[0], hi[1], hi[2]),
        vert(lo[0], hi[1], hi[2]),
    ];
    // Hole corners (CCW seen from +z).
    let hb = [
        vert(hlo[0], hlo[1], lo[2]),
        vert(hhi[0], hlo[1], lo[2]),
        vert(hhi[0], hhi[1], lo[2]),
        vert(hlo[0], hhi[1], lo[2]),
    ];
    let ht = [
        vert(hlo[0], hlo[1], hi[2]),
        vert(hhi[0], hlo[1], hi[2]),
        vert(hhi[0], hhi[1], hi[2]),
        vert(hlo[0], hhi[1], hi[2]),
    ];
    let mut ring = |vs: &[u32]| -> Vec<u32> {
        let n = vs.len();
        (0..n)
            .map(|i| {
                edges.push(BRepEdge {
                    start: vs[i],
                    end: vs[(i + 1) % n],
                    curve: Curve::LineSegment,
                });
                (edges.len() - 1) as u32
            })
            .collect()
    };
    let rev = |vs: &[u32]| -> Vec<u32> { vs.iter().rev().copied().collect() };
    // Top cap (+z): outer CCW from above, hole CW from above.
    let top_outer = ring(&ot);
    let top_hole = ring(&rev(&ht));
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -hi[2],
        },
        outer_loop: top_outer,
        inner_loops: vec![top_hole],
        reversed: false,
    });
    // Bottom cap (−z): outer CW from above (= CCW from below), hole CCW from above.
    let bot_outer = ring(&rev(&ob));
    let bot_hole = ring(&hb);
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, -1.0),
            d: lo[2],
        },
        outer_loop: bot_outer,
        inner_loops: vec![bot_hole],
        reversed: false,
    });
    // Outer walls: quad (b_i, b_{i+1}, t_{i+1}, t_i) is CCW seen from outside.
    let wall_normals = [
        (Vector3::new(0.0, -1.0, 0.0), lo[1]),
        (Vector3::new(1.0, 0.0, 0.0), -hi[0]),
        (Vector3::new(0.0, 1.0, 0.0), -hi[1]),
        (Vector3::new(-1.0, 0.0, 0.0), lo[0]),
    ];
    for i in 0..4 {
        let j = (i + 1) % 4;
        let lp = ring(&[ob[i], ob[j], ot[j], ot[i]]);
        let (normal, d) = wall_normals[i];
        faces.push(BRepFace {
            surface: Surface::Plane { normal, d },
            outer_loop: lp,
            inner_loops: Vec::new(),
            reversed: false,
        });
    }
    // Hole walls: material is OUTSIDE the hole, so the outward normal points
    // into the hole; the quad (b_i, t_i, t_{i+1}, b_{i+1}) is CCW seen from
    // inside the hole.
    let hole_normals = [
        (Vector3::new(0.0, 1.0, 0.0), -hlo[1]),
        (Vector3::new(-1.0, 0.0, 0.0), hhi[0]),
        (Vector3::new(0.0, -1.0, 0.0), hhi[1]),
        (Vector3::new(1.0, 0.0, 0.0), -hlo[0]),
    ];
    for i in 0..4 {
        let j = (i + 1) % 4;
        let lp = ring(&[hb[i], ht[i], ht[j], hb[j]]);
        let (normal, d) = hole_normals[i];
        faces.push(BRepFace {
            surface: Surface::Plane { normal, d },
            outer_loop: lp,
            inner_loops: Vec::new(),
            reversed: false,
        });
    }
    BRep::new(vertices, edges, faces).expect("holed_box BRep::new")
}

/// A z-axis solid cylinder (bottom cap on `z = base_z`, extruded +z).
fn z_cylinder(cx: f64, cy: f64, base_z: f64, radius: f64, height: f64) -> BRep {
    let bottom = p(cx, cy, base_z);
    let top = p(cx, cy, base_z + height);
    let verts = vec![
        BRepVertex {
            point: p(cx + radius, cy, base_z),
        },
        BRepVertex {
            point: p(cx + radius, cy, base_z + height),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: bottom,
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: top,
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
                axis_point: bottom,
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

/// Run `op` and apply the three oracles; returns the output volume.
fn check(a: &BRep, b: &BRep, op: BoolOp, what: &str, analytic: f64) -> f64 {
    let out = boolean(a, b, op, &nb()).unwrap_or_else(|e| {
        panic!("{what}: disc ∩ holed-polygon pair must be handled by Stage 0: {e}")
    });
    let mesh = out.as_mesh();
    assert!(
        is_watertight(mesh),
        "{what}: output must be a closed 2-manifold"
    );
    assert!(
        is_outward_solid(mesh),
        "{what}: output must be consistently outward-oriented"
    );
    let vol = signed_volume(mesh);
    // The disc rim is a chord polygon (its N-gon under-fills the circle); the
    // sibling M8 suites accept 8 %, which rejects a dropped cap, a doubled
    // sheet, or a paved-over hole (the hole is 1/16 of the plate here, and
    // every cylinder is ≥ 1/3 of the plate volume).
    assert!(
        (vol - analytic).abs() / analytic < 0.08,
        "{what}: volume {vol} not within chord band of analytic {analytic}"
    );
    vol
}

// Plate 4×4×1 (z ∈ [0,1]) with a 1×1 square hole through its centre.
const LO: [f64; 3] = [-2.0, -2.0, 0.0];
const HI: [f64; 3] = [2.0, 2.0, 1.0];
const HLO: [f64; 2] = [-0.5, -0.5];
const HHI: [f64; 2] = [0.5, 0.5];
const PLATE_VOL: f64 = 16.0 - 1.0;

/// Area of the circular segment of a radius-`r` disc cut off by a chord at
/// distance `d` from the centre (the part beyond the chord).
fn segment_area(r: f64, d: f64) -> f64 {
    r * r * (d / r).acos() - d * (r * r - d * d).sqrt()
}

// ───────────────────────────── tests ─────────────────────────────

/// Fixture sanity: the holed plate is a valid solid on its own — a union with
/// a cylinder that touches nothing returns both, with the plate's volume.
#[test]
fn holed_box_fixture_is_a_valid_solid() {
    let plate = holed_box(LO, HI, HLO, HHI);
    let far = z_cylinder(10.0, 0.0, 0.0, 0.5, 1.0);
    let out = boolean(&plate, &far, BoolOp::Union, &nb()).expect("disjoint union");
    let mesh = out.as_mesh();
    assert!(is_watertight(mesh) && is_outward_solid(mesh));
    let vol = signed_volume(mesh);
    let analytic = PLATE_VOL + std::f64::consts::PI * 0.25;
    assert!(
        (vol - analytic).abs() / analytic < 0.08,
        "vol {vol} vs {analytic}"
    );
}

/// CONTAINMENT (R0070's shape, union): a cylinder stands on the holed top
/// cap, its disc inside the face and clear of the hole. Opposite normals.
#[test]
fn disc_in_holed_polygon_union_succeeds() {
    let plate = holed_box(LO, HI, HLO, HHI);
    let boss = z_cylinder(1.2, 0.0, 1.0, 0.6, 1.0);
    let analytic = PLATE_VOL + std::f64::consts::PI * 0.36;
    check(&plate, &boss, BoolOp::Union, "containment union", analytic);
}

/// CONTAINMENT, cut (R0070 op 3 is a cut): the tool's bottom disc is coplanar
/// with the holed top cap (SAME normal side as the cut sees it) and the tool
/// reaches down through the plate.
#[test]
fn disc_in_holed_polygon_cut_succeeds() {
    let plate = holed_box(LO, HI, HLO, HHI);
    // Tool bottom disc on z=1 (the cap), extruded UP: only its base touches.
    let tool_up = z_cylinder(1.2, 0.0, 1.0, 0.6, 1.0);
    check(
        &plate,
        &tool_up,
        BoolOp::Subtract,
        "containment cut (touching)",
        PLATE_VOL,
    );
    // Tool top disc on z=1, extruded DOWN through the plate: a blind→through bore.
    let tool_down = z_cylinder(1.2, 0.0, 0.0, 0.6, 1.0);
    let analytic = PLATE_VOL - std::f64::consts::PI * 0.36;
    check(
        &plate,
        &tool_down,
        BoolOp::Subtract,
        "containment cut (through)",
        analytic,
    );
}

/// CROSSING (union): the disc rim crosses one straight hole edge — the disc
/// spans the hole's +x edge (x = 0.5) as a single chord, far from the hole
/// corners. The overlay must split the hole edge and the disc rim identically.
#[test]
fn disc_crossing_hole_edge_union_succeeds() {
    let plate = holed_box(LO, HI, HLO, HHI);
    // Centre (0.8, 0): r=0.4 reaches x=0.4 < 0.5 … make it cross: r=0.45.
    let boss = z_cylinder(0.8, 0.0, 1.0, 0.45, 1.0);
    let analytic = PLATE_VOL + std::f64::consts::PI * 0.45 * 0.45;
    check(&plate, &boss, BoolOp::Union, "crossing union", analytic);
}

/// CROSSING (cut, through): the tool's TOP disc lies on the holed cap while
/// its rim overhangs the hole across one edge, and the tool reaches below the
/// plate (one coplanar pair only). Removed volume = plate height × (disc area
/// − the circular segment already inside the hole).
#[test]
fn disc_crossing_hole_edge_cut_succeeds() {
    let plate = holed_box(LO, HI, HLO, HHI);
    let (cx, r) = (0.8, 0.45);
    let tool = z_cylinder(cx, 0.0, -0.5, r, 1.5);
    // The hole edge x = 0.5 is a chord at distance cx − 0.5 from the centre.
    let seg = segment_area(r, cx - 0.5);
    let analytic = PLATE_VOL - (std::f64::consts::PI * r * r - seg);
    check(&plate, &tool, BoolOp::Subtract, "crossing cut", analytic);
}

/// PRE-EXISTING BOUNDARY (must stay loud, not silent): a tool whose BOTH caps
/// are flush with the plate's caps (two coplanar pairs sharing one cylinder
/// lateral) while its rim crosses a straight edge. The two pairs' rim splits
/// are merged independently and the lateral's azimuth-merge rejects them.
/// Measured 2026-09-17 on an UNHOLED plate too (outer-edge crossing →
/// `FaceResolutionFailed`), so it is not a hole effect; ledgered in the spec
/// (§7), out of this slice. Pinned so a future silent pass-through is caught.
#[test]
fn doubly_flush_crossing_stays_loud() {
    let plate = holed_box(LO, HI, HLO, HHI);
    let tool = z_cylinder(0.8, 0.0, 0.0, 0.45, 1.0);
    let res = boolean(&plate, &tool, BoolOp::Subtract, &nb());
    assert!(
        res.is_err(),
        "two flush crossing pairs on one lateral are out of scope and must stay loud"
    );
}

/// DISC CONTAINS THE HOLE (union): a wide boss covers the hole opening; the
/// overlap is the annulus disc − hole, the through-hole becomes blind.
#[test]
fn disc_over_hole_union_succeeds() {
    let plate = holed_box(LO, HI, HLO, HHI);
    let boss = z_cylinder(0.0, 0.0, 1.0, 1.0, 1.0);
    let analytic = PLATE_VOL + std::f64::consts::PI;
    check(
        &plate,
        &boss,
        BoolOp::Union,
        "disc-over-hole union",
        analytic,
    );
}

/// DISC CONTAINS THE HOLE (cut): a counterbore centred on the hole — the
/// tool's TOP disc lies on the holed cap (the coplanar pair), its bottom disc
/// is inside the plate.
#[test]
fn disc_over_hole_cut_succeeds() {
    let plate = holed_box(LO, HI, HLO, HHI);
    let tool = z_cylinder(0.0, 0.0, 0.5, 1.0, 0.5);
    let analytic = PLATE_VOL - 0.5 * (std::f64::consts::PI - 1.0);
    check(
        &plate,
        &tool,
        BoolOp::Subtract,
        "disc-over-hole counterbore",
        analytic,
    );
}
