//! PR-M8-disc (increment 1) — §4.5.5 coplanar preprocessing for the dominant
//! M8 residue sub-class: a flat circular DISC (a cylinder end-cap, a planar
//! face bounded by a single closed `Curve::Circle`) coplanar with a planar
//! POLYGON face of the other solid.
//!
//! PR-M8-disc-disc (Increment 1) extends this to disc∩disc CONTAINMENT — a
//! cap-on-cap pair where one rim is strictly inside the other (a bearing
//! recess / coaxial cap stack). Stage 0 samples the disc's rim
//! into the SAME ring Stage 1 builds for the cap/lateral (extracted from
//! Stage 1's own output, so it is bit-identical and the §4.5.5 shared-mesh
//! guarantee holds), then routes the disc through the existing exact polygon
//! overlay.
//!
//! Increment 1 covers pure CONTAINMENT (disc ⊆ polygon or polygon ⊆ disc):
//! no circle×edge crossing point, so the rational overlay introduces no
//! irrational arc∩segment intersection and no boundary-split propagation is
//! needed. A crossing pair stays the loud `CoplanarFacesUnsupported` residue
//! (asserted below — the increment boundary is pinned, not silent).

use cad_primitives::{BoolOp, Point3, Vector3};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Mesh, Surface, YangError};

// ════════════════════════════════════════════════════════════════════
// fixtures
// ════════════════════════════════════════════════════════════════════

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Axis-aligned box B-Rep [lo, hi] (yr24/yr26 hexahedron topology).
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

/// A z-axis cylinder whose BOTTOM cap sits on the plane `z = base_z`
/// (normal −z), centred at `(cx, cy, base_z)`, of the given `radius` and
/// `height` (extruded toward +z). Two circle rims + one seam segment; the
/// bottom cap is the disc that goes coplanar with a box top face.
fn z_cylinder(cx: f64, cy: f64, base_z: f64, radius: f64, height: f64) -> BRep {
    let bottom = [cx, cy, base_z];
    let top = [cx, cy, base_z + height];
    // Seam at +x.
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

/// A prism extruded from a CCW 2D `profile` (z0→z1), with per-face directed
/// edges (mirrors nc1_nonconvex::u_prism). Works for NON-CONVEX profiles.
fn polygon_prism(profile: &[[f64; 2]], z0: f64, z1: f64) -> BRep {
    let n = profile.len();
    let mut verts: Vec<BRepVertex> = Vec::with_capacity(2 * n);
    for &[x, y] in profile {
        verts.push(BRepVertex { point: p(x, y, z0) });
    }
    for &[x, y] in profile {
        verts.push(BRepVertex { point: p(x, y, z1) });
    }
    let line = |s: u32, e: u32| BRepEdge {
        start: s,
        end: e,
        curve: Curve::LineSegment,
    };
    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut faces: Vec<BRepFace> = Vec::new();

    // Bottom cap (normal −z): profile reversed so it reads CCW from below.
    let bottom_order: Vec<u32> = std::iter::once(0u32).chain((1..n as u32).rev()).collect();
    let bb = edges.len() as u32;
    for i in 0..n {
        edges.push(line(bottom_order[i], bottom_order[(i + 1) % n]));
    }
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, -1.0),
            d: z0,
        },
        outer_loop: (bb..bb + n as u32).collect(),
        inner_loops: Vec::new(),
        reversed: false,
    });
    // Top cap (normal +z): forward.
    let tb = edges.len() as u32;
    for i in 0..n as u32 {
        edges.push(line(n as u32 + i, n as u32 + (i + 1) % n as u32));
    }
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -z1,
        },
        outer_loop: (tb..tb + n as u32).collect(),
        inner_loops: Vec::new(),
        reversed: false,
    });
    // Side walls.
    for i in 0..n as u32 {
        let (bi, bj) = (i, (i + 1) % n as u32);
        let (ti, tj) = (n as u32 + i, n as u32 + (i + 1) % n as u32);
        let base = edges.len() as u32;
        edges.push(line(bi, bj));
        edges.push(line(bj, tj));
        edges.push(line(tj, ti));
        edges.push(line(ti, bi));
        let a = profile[i as usize];
        let b = profile[((i + 1) % n as u32) as usize];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        let (nx, ny) = (dy / len, -dx / len);
        faces.push(BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(nx, ny, 0.0),
                d: -(nx * a[0] + ny * a[1]),
            },
            outer_loop: vec![base, base + 1, base + 2, base + 3],
            inner_loops: Vec::new(),
            reversed: false,
        });
    }
    BRep::new(verts, edges, faces).expect("polygon_prism BRep::new")
}

// ════════════════════════════════════════════════════════════════════
// oracles
// ════════════════════════════════════════════════════════════════════

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

/// Every undirected triangle edge is shared by exactly two triangles
/// (closed 2-manifold surface).
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

fn nb() -> impl yang_rs::MeshBoolean {
    yang_rs::native_backend().expect("native backend always available")
}

/// Is the mesh CONSISTENTLY oriented — every interior edge traversed in
/// opposite directions by its two triangles? A flipped patch makes some
/// directed edge appear twice (same direction). This is orientation-correct
/// even for concave/pocketed solids (unlike a centroid-outward heuristic).
fn is_consistently_oriented(mesh: &Mesh) -> bool {
    use std::collections::HashMap;
    let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
    for t in &mesh.tris {
        for k in 0..3 {
            *directed.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
        }
    }
    // Consistent ⇔ no directed edge is used more than once.
    directed.values().all(|&c| c == 1)
}

/// Then orient globally: a consistently-oriented closed mesh has positive
/// signed volume iff outward. Combined with `is_consistently_oriented`, a
/// positive signed volume confirms an outward-oriented solid.
fn is_outward_solid(mesh: &Mesh) -> bool {
    is_consistently_oriented(mesh) && signed_volume(mesh) > 0.0
}

// ════════════════════════════════════════════════════════════════════
// tests
// ════════════════════════════════════════════════════════════════════

/// CONTAINMENT (disc ⊆ polygon): a r=0.5 cylinder stands on the centre of a
/// [0,2]³ box's top face (z = 2). The bottom cap (disc) is coplanar with and
/// strictly inside the 2×2 top face. Union must now SUCCEED (was the
/// `CoplanarFacesUnsupported` M8 wall) and yield a single watertight solid of
/// volume ≈ box (8) + cylinder (π r² h = π·0.25·1 ≈ 0.7854), within the
/// Stage-1 chord band.
#[test]
fn disc_in_polygon_union_succeeds() {
    let a = box_brep([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = z_cylinder(1.0, 1.0, 2.0, 0.5, 1.0);
    let out = boolean(&a, &b, BoolOp::Union, &nb())
        .expect("disc-in-polygon union must be handled by Stage 0");
    let mesh = out.as_mesh();
    assert!(
        is_watertight(mesh),
        "union output must be a closed 2-manifold"
    );
    let vol = signed_volume(mesh);
    let analytic = 8.0 + std::f64::consts::PI * 0.25 * 1.0;
    // Discretized cylinder under-fills the disc; allow a generous chord band
    // but reject a missing-cylinder (vol ≈ 8) or doubled-sheet result.
    assert!(
        (vol - analytic).abs() < 0.05,
        "union volume {vol} not within chord band of analytic {analytic}"
    );
}

/// CONTAINMENT (polygon ⊆ disc): a small [0.9,1.1]²×[1,2] box stands on the
/// flat top cap of a wide r=0.5 cylinder, the box's bottom face coplanar with
/// and strictly inside the cap disc. The orientation with the disc on the
/// OTHER solid exercises the symmetric `ring_b` resolution path.
#[test]
fn polygon_in_disc_union_succeeds() {
    let cyl = z_cylinder(1.0, 1.0, 0.0, 0.5, 1.0); // top cap at z = 1
    let small = box_brep([0.9, 0.9, 1.0], [1.1, 1.1, 2.0]);
    let out = boolean(&cyl, &small, BoolOp::Union, &nb())
        .expect("polygon-in-disc union must be handled by Stage 0");
    assert!(
        is_watertight(out.as_mesh()),
        "union output must be a closed 2-manifold"
    );
}

/// disc∩disc CONTAINMENT, UNION: a narrow r=0.5 cylinder stands coaxially on
/// the flat top cap (r=2) of a wide cylinder — both caps coplanar at z=2, the
/// small rim strictly inside the large rim. Union must succeed (was the M8
/// `disc-disc` wall), watertight, volume ≈ wide cylinder (π·4·2) + the small
/// protrusion (π·0.25·1) within the chord band.
#[test]
fn disc_in_disc_union_succeeds() {
    let wide = z_cylinder(0.0, 0.0, 0.0, 2.0, 2.0); // top cap at z=2
    let pin = z_cylinder(0.0, 0.0, 2.0, 0.5, 1.0); // bottom cap at z=2
    let out = boolean(&wide, &pin, BoolOp::Union, &nb())
        .expect("disc∩disc containment union must be handled by Stage 0");
    let mesh = out.as_mesh();
    assert!(
        is_watertight(mesh),
        "union output must be a closed 2-manifold"
    );
    // The pin protrudes above the body top: the union must reach the pin's top
    // (z=3), proving the small disc was fused (not dropped). (Volume is a poor
    // discriminator here — the pin is only ~3% of the coarsely-tessellated wide
    // cylinder, well inside the N-gon under-fill band.)
    let max_z = mesh.verts.iter().map(|p| p.z()).fold(f64::MIN, f64::max);
    assert!(
        (max_z - 3.0).abs() < 1e-6,
        "union must include the protruding pin (max z {max_z}, expected 3)"
    );
    assert!(
        is_outward_solid(mesh),
        "union must be consistently outward-oriented (no flipped patch)"
    );
    let vol = signed_volume(mesh).abs();
    let analytic = std::f64::consts::PI * 4.0 * 2.0 + std::f64::consts::PI * 0.25 * 1.0;
    assert!(
        (vol - analytic).abs() / analytic < 0.06,
        "union volume {vol} not within chord band of analytic {analytic}"
    );
}

/// disc∩disc CONTAINMENT, SUBTRACT — the BEARING RECESS the user reported: a
/// small cylinder is cut partial-depth into the flat top of a larger cylinder,
/// the two caps coplanar at z=2. Must succeed (was the M8 wall), watertight,
/// and remove material (volume strictly less than the solid body).
#[test]
fn bearing_recess_subtract_succeeds() {
    let body = z_cylinder(0.0, 0.0, 0.0, 2.0, 2.0); // body top cap at z=2
                                                    // Recess tool: small cylinder spanning the cap, cut downward into the body.
    let tool = z_cylinder(0.0, 0.0, 1.0, 0.5, 1.0); // top cap at z=2, into body
    let out = boolean(&body, &tool, BoolOp::Subtract, &nb())
        .expect("bearing-recess subtract (disc∩disc) must be handled by Stage 0");
    let mesh = out.as_mesh();
    assert!(
        is_watertight(mesh),
        "bearing-recess output must be a closed 2-manifold"
    );
    assert!(
        is_outward_solid(mesh),
        "bearing-recess normals must point outward"
    );
    let vol = signed_volume(mesh).abs();
    let body_vol = std::f64::consts::PI * 4.0 * 2.0;
    let recess = std::f64::consts::PI * 0.25 * 1.0;
    assert!(
        vol < body_vol - recess * 0.5,
        "recess must remove material: vol {vol} vs body {body_vol}"
    );
}

/// Two coplanar caps whose rims are disjoint in-plane (AABBs overlap via the
/// scan band, the discs do not): benign — Stage 0 emits no override and the
/// exact arrangement passes the coplanar non-overlap through.
#[test]
fn disc_disc_disjoint_is_benign() {
    // Both caps at z=2; centres 3 apart, radii 0.5 each → rims disjoint.
    let left = z_cylinder(-1.5, 0.0, 2.0, 0.5, 1.0);
    let right = z_cylinder(1.5, 0.0, 0.0, 0.5, 2.0); // top cap at z=2
    let out =
        boolean(&left, &right, BoolOp::Union, &nb()).expect("disjoint coplanar caps are benign");
    assert!(is_watertight(out.as_mesh()));
}

/// INCREMENT 2 BOUNDARY: two coplanar caps whose rims CROSS (neither contained)
/// need arc∩arc crossing + rim-split propagation — deferred. Must stay loud.
#[test]
fn disc_disc_crossing_stays_unsupported() {
    // Both caps at z=2, radius 1, centres 1 apart → rims cross.
    let a = z_cylinder(-0.5, 0.0, 2.0, 1.0, 1.0);
    let b = z_cylinder(0.5, 0.0, 0.0, 1.0, 2.0); // top cap at z=2
    let err = boolean(&a, &b, BoolOp::Union, &nb())
        .expect_err("crossing disc∩disc must stay the loud M8 residue");
    assert!(
        matches!(err, YangError::CoplanarFacesUnsupported { .. }),
        "expected CoplanarFacesUnsupported, got {err:?}"
    );
}

/// NON-CONVEX containment: a small cylinder cap sits coplanar inside the
/// non-convex top face of an L-prism (in the lower arm, away from the reflex
/// corner). The convex fast path does not apply; the general overlay handles
/// it. Union succeeds, watertight, consistently oriented.
#[test]
fn disc_in_nonconvex_polygon_union_succeeds() {
    let l = polygon_prism(
        &[
            [0.0, 0.0],
            [3.0, 0.0],
            [3.0, 1.0],
            [1.0, 1.0],
            [1.0, 3.0],
            [0.0, 3.0],
        ],
        0.0,
        1.0,
    );
    let pin = z_cylinder(0.5, 0.5, 1.0, 0.3, 1.0);
    let out = boolean(&l, &pin, BoolOp::Union, &nb())
        .expect("disc in non-convex polygon (containment) must be handled by Stage 0");
    let mesh = out.as_mesh();
    assert!(is_watertight(mesh), "union must be a closed 2-manifold");
    assert!(
        is_outward_solid(mesh),
        "union must be consistently outward-oriented"
    );
    let max_z = mesh.verts.iter().map(|p| p.z()).fold(f64::MIN, f64::max);
    assert!(
        (max_z - 2.0).abs() < 1e-6,
        "pin must protrude (max z {max_z})"
    );
}

/// PR-M8 disc-rim crossing: a disc that CROSSES the polygon boundary (the cap
/// pokes past the box edge) must now SUCCEED — the rim crossing points
/// propagate into the cylinder lateral and the opposite cap so the mesh stays
/// conformal. The cylinder's bottom cap is −z (OPPOSITE the box top's +z), so
/// it routes through the opposite-normal crossing path.
#[test]
fn disc_crossing_polygon_succeeds() {
    // Box top is [0,2]², cap radius 0.5 centred at the CORNER (0,0): the disc
    // crosses the x = 0 and y = 0 box edges.
    let a = box_brep([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = z_cylinder(0.0, 0.0, 2.0, 0.5, 1.0);
    let out = boolean(&a, &b, BoolOp::Union, &nb())
        .expect("disc-rim crossing (opposite-normal) must be handled by Stage 0");
    let mesh = out.as_mesh();
    assert!(
        is_watertight(mesh),
        "crossing union output must be a closed 2-manifold"
    );
    assert!(
        is_outward_solid(mesh),
        "crossing union must be consistently outward-oriented"
    );
}

/// PR-M8 disc-rim crossing — the user's reported boss/recess: a cylinder whose
/// bottom cap (−z) is coplanar with the box top (+z, OPPOSITE normals) and
/// whose rim crosses the box corner edges. Both UNION (box ∪ boss) and SUBTRACT
/// (boss − box, operand-swapped) must succeed, watertight + outward.
#[test]
fn cylinder_cap_crossing_box_corner_union_and_subtract() {
    let bx = box_brep([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let boss = z_cylinder(0.0, 0.0, 2.0, 0.5, 1.0);

    let u = boolean(&bx, &boss, BoolOp::Union, &nb())
        .expect("boss-at-corner UNION must be handled by Stage 0");
    let um = u.as_mesh();
    assert!(is_watertight(um), "union must be a closed 2-manifold");
    assert!(is_outward_solid(um), "union must be outward-oriented");

    let s = boolean(&boss, &bx, BoolOp::Subtract, &nb())
        .expect("boss−box SUBTRACT must be handled by Stage 0");
    let sm = s.as_mesh();
    assert!(is_watertight(sm), "subtract must be a closed 2-manifold");
    assert!(is_outward_solid(sm), "subtract must be outward-oriented");
}

/// SAME-normal coplanar crossing: a cylinder whose TOP cap (+z) is coplanar
/// with the box top (+z, SAME normal), the cap centred at the box CORNER so its
/// rim crosses the x=0 / y=0 box-top edges. cherchi N13 (commit 6280237d) now
/// classifies the resulting single-coplanar-edge crossings, so this case is
/// handled end-to-end (it reaches cherchi — the stage-0 same-normal residue
/// gate does not fire for it). Union must succeed and be a watertight,
/// consistently outward solid (NOT silently mis-handled). NB: oblique / scaled
/// same-normal corpus variants still hit the stage-0 same-normal gate because
/// the rim-crossing opposite-rim projection is not yet correct for them
/// (task #20) — this asserts only the axis-aligned config cherchi N13 covers.
#[test]
fn same_normal_crossing_union_succeeds() {
    let bx = box_brep([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    // base_z=1, height 1 → TOP cap (+z) at z=2, same normal as the box top.
    let cyl = z_cylinder(0.0, 0.0, 1.0, 0.5, 1.0);
    let out = boolean(&bx, &cyl, BoolOp::Union, &nb())
        .expect("same-normal corner crossing union must be handled (cherchi N13)");
    let m = out.as_mesh();
    assert!(
        is_watertight(m),
        "same-normal crossing union must be watertight"
    );
    assert!(
        is_outward_solid(m),
        "same-normal crossing union must be consistently outward"
    );
}

/// USER REPRO: cylinder dia 10 (r=5) length 10, a dia-5 (r=2.5) circle on the
/// top face, extrude-CUT 2 deep (concentric). The cut tool's top cap (r=2.5,
/// z=10) is coplanar+contained in the body top cap (r=5, z=10) — disc∩disc
/// CONTAINMENT (bearing recess). Reported failing in-app with a Stage-5/6
/// "geometric face resolution failed for kept triangle …".
#[test]
fn user_recess_dia10_len10_dia5_cut2() {
    let body = z_cylinder(0.0, 0.0, 0.0, 5.0, 10.0); // top cap at z=10
    let tool = z_cylinder(0.0, 0.0, 8.0, 2.5, 2.0); // top cap z=10, floor z=8
    match boolean(&body, &tool, BoolOp::Subtract, &nb()) {
        Ok(out) => {
            let m = out.as_mesh();
            assert!(is_watertight(m), "recess subtract must be watertight");
            assert!(is_outward_solid(m), "recess subtract must be outward");
        }
        Err(e) => panic!("user recess failed: {e:?}"),
    }
}

/// CORE M8 planar partial-overlap (the cross-box, F0002/F0004/F0006 class): two
/// box prisms whose coplanar caps PARTIALLY overlap (neither contains the
/// other) — a cross shape. Stage 0's exact 2D overlay segments the coplanar
/// faces, and the neighbor side walls are re-tessellated with the propagated
/// boundary splits. The side walls are convex rectangles subdivided on TWO
/// opposite edges, which the apex-fan cannot triangulate — the interior-centroid
/// fan fallback in `triangulate_ring` covers them. Union must be a watertight,
/// outward solid with the exact cross volume (4 + 4 − 1 = 7).
#[test]
fn cross_box_partial_overlap_union_succeeds() {
    let a = box_brep([-2.0, -0.5, 0.0], [2.0, 0.5, 1.0]); // bar along x, vol 4
    let b = box_brep([-0.5, -2.0, 0.0], [0.5, 2.0, 1.0]); // bar along y, vol 4
    let out = boolean(&a, &b, BoolOp::Union, &nb())
        .expect("cross-box partial-overlap union must be handled by Stage 0");
    let m = out.as_mesh();
    assert!(is_watertight(m), "cross-box union must be watertight");
    assert!(
        is_outward_solid(m),
        "cross-box union must be outward-oriented"
    );
    let v = signed_volume(m);
    assert!(
        (v - 7.0).abs() < 1e-9,
        "cross-box union volume must be 4+4-1=7, got {v}"
    );
}
