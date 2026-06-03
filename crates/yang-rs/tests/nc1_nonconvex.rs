//! PR-NC1 Part B — non-convex / holed planar Stage-1 tessellation (RED).
//!
//! Exercises `yang_rs::BRep::new` directly (NO sidecar — Stage 1 tessellation
//! runs in pure `BRep::new`). We build input BReps whose planar cap faces are
//! either non-convex (an L-prism cap with a reflex vertex) or holed (a plate
//! whose ±z faces are annuli with one inner loop), then inspect `r.as_mesh()`.
//!
//! Asserts the six Part B oracle points from
//! `specs/yang_pr_nc1_nonconvex_cdt.md`:
//!   1. non-convex face: exact mesh-area coverage + watertight 2-manifold +
//!      no out-of-polygon triangle.
//!   2. holed face: exact coverage (face area − hole area).
//!   3. no boundary edge split: every input B-Rep boundary loop edge
//!      (start,end) appears as a directed mesh edge.
//!   4. bijection: every emitted mesh vertex's `TessellationSource` is a
//!      boundary `BRepVertex` / `BRepEdge` (no `BRepFace` / `Intersection` /
//!      `Unknown`, i.e. no Steiner points).
//!   5. determinism (two `BRep::new` calls → identical mesh tris).
//!   6. convex-box regression: the fan path still yields the expected box mesh.
//!
//! RED state: with the CDT stub returning `Err`, the non-convex and holed
//! faces today flow through the legacy fan path (non-convex faces ⇒
//! out-of-polygon / wrong-area triangles; holed faces ⇒ the hole is ignored
//! and triangulated across). So the non-convex/holed oracles FAIL on coverage
//! / membership / watertightness. The convex-box regression (oracle 6) PASSES
//! already (fan path unchanged) — that's expected.
//!
//! Fixed coordinates only: no rand, no time, no filesystem.

use cad_primitives::{Point3, Vector3};
use cherchi_rs::Mesh;
use std::collections::HashMap;
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface, TessellationSource};

const AREA_TAU: f64 = 1e-9;

// =========================================================================
// Mesh audit helpers (mirror inner_loops.rs / fuzz_boxes conventions).
// =========================================================================

fn unpaired_half_edges(mesh: &Mesh) -> usize {
    let mut counts: HashMap<(u32, u32), i32> = HashMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *counts.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut unpaired = 0;
    for (&(s, e), &fwd) in &counts {
        let rev = counts.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            unpaired += (fwd - rev).unsigned_abs() as usize;
        }
    }
    unpaired
}

/// Total surface area of the mesh (sum of triangle areas).
fn mesh_area(mesh: &Mesh) -> f64 {
    let mut acc = 0.0;
    for t in &mesh.tris {
        let a = mesh.verts[t[0] as usize].as_array();
        let b = mesh.verts[t[1] as usize].as_array();
        let c = mesh.verts[t[2] as usize].as_array();
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cx = ab[1] * ac[2] - ab[2] * ac[1];
        let cy = ab[2] * ac[0] - ab[0] * ac[2];
        let cz = ab[0] * ac[1] - ab[1] * ac[0];
        acc += 0.5 * (cx * cx + cy * cy + cz * cz).sqrt();
    }
    acc
}

/// Set of directed mesh edges (a,b).
fn directed_mesh_edges(mesh: &Mesh) -> std::collections::HashSet<(u32, u32)> {
    let mut set = std::collections::HashSet::new();
    for t in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            set.insert((t[i], t[j]));
        }
    }
    set
}

/// Even-odd 2D point-in-polygon (used on a chosen planar projection).
fn point_in_polygon_2d(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let n = poly.len();
    let mut inside = false;
    let (px, py) = (p[0], p[1]);
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// =========================================================================
// Fixture: U-shaped prism.
//
// Cross-section is a U in the XY plane (TWO reflex vertices, and crucially NOT
// star-shaped from vertex 0 — so a fan from the first vertex escapes the
// polygon). Extruded along +z from z=0 to z=1. The z=0 (−z) and z=1 (+z) caps
// are the non-convex faces. Eight rectangular side walls close the solid.
//
// U cross-section (CCW from +z), area 5.0:
//   p0:(0,0) p1:(3,0) p2:(3,2) p3:(2,2) p4:(2,1) p5:(1,1) p6:(1,2) p7:(0,2)
//   reflex vertices = p4 (2,1) and p5 (1,1); the notch is x∈[1,2], y∈[1,2].
//
// Fan-from-p0 covers area 7.0 (≠ 5.0) and puts triangle centroids in the notch
// — that is precisely the RED defect the CDT path must fix.
// =========================================================================

const U_PROFILE: [[f64; 2]; 8] = [
    [0.0, 0.0],
    [3.0, 0.0],
    [3.0, 2.0],
    [2.0, 2.0],
    [2.0, 1.0],
    [1.0, 1.0],
    [1.0, 2.0],
    [0.0, 2.0],
];
const U_N: u32 = 8;

fn u_prism() -> BRep {
    // 16 vertices: bottom ring 0..8 (z=0), top ring 8..16 (z=1).
    let mut verts: Vec<BRepVertex> = Vec::with_capacity(16);
    for &[x, y] in &U_PROFILE {
        verts.push(BRepVertex {
            point: Point3::new(x, y, 0.0),
        });
    }
    for &[x, y] in &U_PROFILE {
        verts.push(BRepVertex {
            point: Point3::new(x, y, 1.0),
        });
    }

    let mut edges: Vec<BRepEdge> = Vec::new();
    let line = |s: u32, e: u32| BRepEdge {
        start: s,
        end: e,
        curve: Curve::LineSegment,
    };

    // --- Bottom cap (z=0), normal (0,0,-1). CW-from-above so that, viewed
    // from outside (below, −z), it reads CCW. Walk the profile in reverse:
    // 0,7,6,5,4,3,2,1.
    let bottom_order: [u32; 8] = [0, 7, 6, 5, 4, 3, 2, 1];
    let bottom_base = edges.len() as u32;
    for i in 0..8 {
        edges.push(line(bottom_order[i], bottom_order[(i + 1) % 8]));
    }
    let bottom_loop: Vec<u32> = (bottom_base..bottom_base + 8).collect();

    // --- Top cap (z=1), normal (0,0,1). CCW-from-above: 8..16.
    let top_order: [u32; 8] = [8, 9, 10, 11, 12, 13, 14, 15];
    let top_base = edges.len() as u32;
    for i in 0..8 {
        edges.push(line(top_order[i], top_order[(i + 1) % 8]));
    }
    let top_loop: Vec<u32> = (top_base..top_base + 8).collect();

    // --- Side walls: one quad per profile edge i → (i,(i+1)%8). The outward
    // normal is the 2D edge normal rotated into XY (z-component 0). Wall loop
    // (CCW from outside): bottom_i → bottom_{i+1} → top_{i+1} → top_i.
    let mut faces: Vec<BRepFace> = Vec::new();
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, -1.0),
            d: 0.0, // n·x + d = 0 at z=0 with n=(0,0,-1) ⇒ d=0
        },
        outer_loop: bottom_loop,
        inner_loops: Vec::new(),
        reversed: false,
    });
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -1.0, // z=1 with n=(0,0,1) ⇒ d=-1
        },
        outer_loop: top_loop,
        inner_loops: Vec::new(),
        reversed: false,
    });

    for i in 0..U_N {
        let bi = i;
        let bj = (i + 1) % U_N;
        let ti = U_N + i;
        let tj = U_N + (i + 1) % U_N;
        let base = edges.len() as u32;
        edges.push(line(bi, bj));
        edges.push(line(bj, tj));
        edges.push(line(tj, ti));
        edges.push(line(ti, bi));
        // Outward 2D normal of profile edge (p_i → p_{i+1}): rotate edge
        // direction (dx,dy) by -90° ⇒ (dy,-dx) for a CCW polygon.
        let a = U_PROFILE[i as usize];
        let b = U_PROFILE[((i + 1) % U_N) as usize];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = (dx * dx + dy * dy).sqrt();
        let nx = dy / len;
        let ny = -dx / len;
        // Plane through point a (z arbitrary): d = -(n·a).
        let d = -(nx * a[0] + ny * a[1]);
        faces.push(BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(nx, ny, 0.0),
                d,
            },
            outer_loop: vec![base, base + 1, base + 2, base + 3],
            inner_loops: Vec::new(),
            reversed: false,
        });
    }

    BRep::new(verts, edges, faces).expect("u_prism BRep::new failed")
}

// =========================================================================
// Fixture: plate with a square hole.
//
// Outer 4x4 square footprint, centered 2x2 square hole, thin slab z∈[0,0.5].
// The +z and −z faces are annuli (outer loop + one inner loop). Four outer
// side walls + four inner (hole) walls close the solid.
//
// Outer footprint (XY), CCW from +z:  o0:(0,0) o1:(4,0) o2:(4,4) o3:(0,4)
// Hole footprint (XY):                h0:(1,1) h1:(3,1) h2:(3,3) h3:(1,3)
// =========================================================================

const PLATE_OUTER: [[f64; 2]; 4] = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
const PLATE_HOLE: [[f64; 2]; 4] = [[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0]];
const PLATE_TOP_Z: f64 = 0.5;

fn holed_plate() -> BRep {
    // Vertices: bottom outer 0..4, bottom hole 4..8, top outer 8..12, top hole 12..16.
    let mut verts: Vec<BRepVertex> = Vec::with_capacity(16);
    for &[x, y] in &PLATE_OUTER {
        verts.push(BRepVertex {
            point: Point3::new(x, y, 0.0),
        });
    }
    for &[x, y] in &PLATE_HOLE {
        verts.push(BRepVertex {
            point: Point3::new(x, y, 0.0),
        });
    }
    for &[x, y] in &PLATE_OUTER {
        verts.push(BRepVertex {
            point: Point3::new(x, y, PLATE_TOP_Z),
        });
    }
    for &[x, y] in &PLATE_HOLE {
        verts.push(BRepVertex {
            point: Point3::new(x, y, PLATE_TOP_Z),
        });
    }

    let mut edges: Vec<BRepEdge> = Vec::new();
    let line = |s: u32, e: u32| BRepEdge {
        start: s,
        end: e,
        curve: Curve::LineSegment,
    };

    // Helper: push a 4-edge ring over `order`, return its edge-index loop.
    let push_ring = |edges: &mut Vec<BRepEdge>, order: [u32; 4]| -> Vec<u32> {
        let base = edges.len() as u32;
        for i in 0..4 {
            edges.push(line(order[i], order[(i + 1) % 4]));
        }
        (base..base + 4).collect()
    };

    // --- Bottom face (z=0), normal (0,0,-1). Outer CCW-from-below = reverse of
    // CCW-from-above. Outer order from below: 0,3,2,1. Hole (a hole on this
    // face): opposite orientation to the outer loop ⇒ 4,5,6,7 (CCW-from-above).
    let bottom_outer = push_ring(&mut edges, [0, 3, 2, 1]);
    let bottom_hole = push_ring(&mut edges, [4, 5, 6, 7]);

    // --- Top face (z=PLATE_TOP_Z), normal (0,0,1). Outer CCW-from-above:
    // 8,9,10,11. Hole CW-from-above: 12,15,14,13.
    let top_outer = push_ring(&mut edges, [8, 9, 10, 11]);
    let top_hole = push_ring(&mut edges, [12, 15, 14, 13]);

    let mut faces: Vec<BRepFace> = Vec::new();
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, -1.0),
            d: 0.0,
        },
        outer_loop: bottom_outer,
        inner_loops: vec![bottom_hole],
        reversed: false,
    });
    faces.push(BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -PLATE_TOP_Z,
        },
        outer_loop: top_outer,
        inner_loops: vec![top_hole],
        reversed: false,
    });

    // --- Outer side walls (4): outward = away from center, CCW from outside
    // walls: bottom_i → bottom_{i+1} → top_{i+1} → top_i.
    let wall = |edges: &mut Vec<BRepEdge>,
                faces: &mut Vec<BRepFace>,
                profile: &[[f64; 2]; 4],
                bi: u32,
                bj: u32,
                ti: u32,
                tj: u32,
                i: usize,
                outward_sign: f64| {
        let base = edges.len() as u32;
        edges.push(line(bi, bj));
        edges.push(line(bj, tj));
        edges.push(line(tj, ti));
        edges.push(line(ti, bi));
        let a = profile[i];
        let b = profile[(i + 1) % 4];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = (dx * dx + dy * dy).sqrt();
        // (dy,-dx) is the right-hand normal of a CCW loop; flip for hole walls.
        let nx = outward_sign * dy / len;
        let ny = outward_sign * (-dx) / len;
        let d = -(nx * a[0] + ny * a[1]);
        faces.push(BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(nx, ny, 0.0),
                d,
            },
            outer_loop: vec![base, base + 1, base + 2, base + 3],
            inner_loops: Vec::new(),
            reversed: false,
        });
    };

    for i in 0..4u32 {
        let bi = i;
        let bj = (i + 1) % 4;
        let ti = 8 + i;
        let tj = 8 + (i + 1) % 4;
        wall(
            &mut edges,
            &mut faces,
            &PLATE_OUTER,
            bi,
            bj,
            ti,
            tj,
            i as usize,
            1.0,
        );
    }
    // --- Inner (hole) side walls (4): normals point INTO the hole (toward
    // material), i.e. opposite the CCW right-hand normal of the hole loop.
    // Bottom hole verts 4..8, top hole verts 12..16.
    for i in 0..4u32 {
        let bi = 4 + i;
        let bj = 4 + (i + 1) % 4;
        let ti = 12 + i;
        let tj = 12 + (i + 1) % 4;
        wall(
            &mut edges,
            &mut faces,
            &PLATE_HOLE,
            bi,
            bj,
            ti,
            tj,
            i as usize,
            -1.0,
        );
    }

    BRep::new(verts, edges, faces).expect("holed_plate BRep::new failed")
}

// =========================================================================
// Convex box fixture (oracle 6 regression — fan path, expected to pass in RED).
// =========================================================================

fn unit_box() -> BRep {
    let signs: [[f64; 3]; 8] = [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];
    let center = [0.5, 0.5, 0.5];
    let half = [0.5, 0.5, 0.5];
    let verts: Vec<BRepVertex> = signs
        .iter()
        .map(|s| BRepVertex {
            point: Point3::new(
                center[0] + s[0] * half[0],
                center[1] + s[1] * half[1],
                center[2] + s[2] * half[2],
            ),
        })
        .collect();

    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
    ];
    let normals: [[f64; 3]; 6] = [
        [0.0, 0.0, -1.0],
        [0.0, 0.0, 1.0],
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0],
    ];
    let mut edges = Vec::with_capacity(24);
    let mut loops = Vec::with_capacity(6);
    for vs in &face_verts {
        let base = edges.len() as u32;
        for i in 0..4 {
            edges.push(BRepEdge {
                start: vs[i],
                end: vs[(i + 1) % 4],
                curve: Curve::LineSegment,
            });
        }
        loops.push(vec![base, base + 1, base + 2, base + 3]);
    }
    let faces: Vec<BRepFace> = (0..6)
        .map(|i| {
            let n = normals[i];
            let v0 = verts[face_verts[i][0] as usize].point.as_array();
            let d = -(n[0] * v0[0] + n[1] * v0[1] + n[2] * v0[2]);
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(n[0], n[1], n[2]),
                    d,
                },
                outer_loop: loops[i].clone(),
                inner_loops: Vec::new(),
                reversed: false,
            }
        })
        .collect();
    BRep::new(verts, edges, faces).expect("unit_box BRep::new failed")
}

// =========================================================================
// Oracle helpers shared across tests.
// =========================================================================

/// Every directed boundary edge of every loop (outer + inner) of `brep`.
fn all_directed_loop_edges(brep: &BRep) -> Vec<(u32, u32)> {
    let edges = brep.edges();
    let mut out = Vec::new();
    for f in brep.faces() {
        for &ei in &f.outer_loop {
            let e = &edges[ei as usize];
            out.push((e.start, e.end));
        }
        for inner in &f.inner_loops {
            for &ei in inner {
                let e = &edges[ei as usize];
                out.push((e.start, e.end));
            }
        }
    }
    out
}

/// Oracle 4: every mesh vertex that is actually referenced by a triangle maps
/// to a boundary feature (BRepVertex / BRepEdge), never a face-interior /
/// intersection / unknown source. Also asserts no Steiner points were added
/// (mesh vertex count == B-Rep vertex count).
fn assert_boundary_bijection(brep: &BRep) {
    let map = brep.tessellation_map();
    let mesh = brep.as_mesh();
    assert_eq!(
        mesh.num_verts(),
        brep.vertices().len(),
        "Steiner points added: mesh has {} verts but B-Rep has {}",
        mesh.num_verts(),
        brep.vertices().len()
    );
    let mut referenced = std::collections::HashSet::new();
    for t in &mesh.tris {
        referenced.extend(t.iter().copied());
    }
    for &v in &referenced {
        let src = map.lookup(v);
        match src {
            TessellationSource::BRepVertex(_) | TessellationSource::BRepEdge { .. } => {}
            other => panic!(
                "mesh vertex {v} maps to non-boundary source {other:?} (expected BRepVertex/BRepEdge)"
            ),
        }
    }
}

// =========================================================================
// Oracle 1 — non-convex U-prism cap.
// =========================================================================

#[test]
fn nonconvex_u_prism_exact_coverage_watertight_membership() {
    let brep = u_prism();
    let mesh = brep.as_mesh();

    // Surface area: 2 U-caps (5.0 each) + side walls. Wall perimeter of the U
    // = 3+2+1+1+1+1+1+2 = 12.0, height 1.0 ⇒ wall area 12.0.
    // Total = 2*5.0 + 12.0 = 22.0.
    let expected = 22.0;
    let got = mesh_area(mesh);
    assert!(
        (got - expected).abs() < AREA_TAU,
        "U-prism mesh area {got} != {expected} (TAU {AREA_TAU}) — \
         fan path over the reflex vertices produces out-of-polygon / overlapping tris"
    );

    // Watertight 2-manifold: every directed edge has its reverse.
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "U-prism mesh is not watertight (unpaired half-edges)"
    );

    // No cap triangle centroid lies outside the U profile. Project to XY.
    let poly: Vec<[f64; 2]> = U_PROFILE.to_vec();
    for t in &mesh.tris {
        let a = mesh.verts[t[0] as usize].as_array();
        let b = mesh.verts[t[1] as usize].as_array();
        let c = mesh.verts[t[2] as usize].as_array();
        // Only check cap triangles (all three z equal to 0 or 1).
        let on_cap = (a[2] == b[2]) && (b[2] == c[2]) && (a[2] == 0.0 || a[2] == 1.0);
        if !on_cap {
            continue;
        }
        let cx = (a[0] + b[0] + c[0]) / 3.0;
        let cy = (a[1] + b[1] + c[1]) / 3.0;
        assert!(
            point_in_polygon_2d([cx, cy], &poly),
            "U-prism cap triangle {t:?} centroid ({cx},{cy}) is outside the U profile"
        );
    }
}

// =========================================================================
// Oracle 2 — holed plate face coverage.
// =========================================================================

#[test]
fn holed_plate_exact_coverage() {
    let brep = holed_plate();
    let mesh = brep.as_mesh();

    // Each annulus face area = outer 16 − hole 4 = 12; two of them ⇒ 24.
    // Outer side walls: perimeter 16 × height 0.5 = 8.
    // Inner (hole) side walls: perimeter 8 × height 0.5 = 4.
    // Total surface area = 24 + 8 + 4 = 36.
    let expected = 36.0;
    let got = mesh_area(mesh);
    assert!(
        (got - expected).abs() < AREA_TAU,
        "holed-plate mesh area {got} != {expected} (TAU {AREA_TAU}) — \
         fan path ignores the inner loop and triangulates across the hole"
    );

    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "holed-plate mesh is not watertight"
    );
}

// =========================================================================
// Oracle 3 — no boundary edge split (both fixtures).
// =========================================================================

#[test]
fn no_boundary_edge_split_u_prism() {
    let brep = u_prism();
    let mesh_edges = directed_mesh_edges(brep.as_mesh());
    for (a, b) in all_directed_loop_edges(&brep) {
        assert!(
            mesh_edges.contains(&(a, b)) || mesh_edges.contains(&(b, a)),
            "U-prism boundary edge ({a},{b}) does not appear unsplit in the mesh"
        );
    }
}

#[test]
fn no_boundary_edge_split_holed_plate() {
    let brep = holed_plate();
    let mesh_edges = directed_mesh_edges(brep.as_mesh());
    for (a, b) in all_directed_loop_edges(&brep) {
        assert!(
            mesh_edges.contains(&(a, b)) || mesh_edges.contains(&(b, a)),
            "holed-plate boundary edge ({a},{b}) does not appear unsplit in the mesh"
        );
    }
}

// =========================================================================
// Oracle 4 — bijection / no Steiner points (both fixtures).
// =========================================================================

#[test]
fn bijection_u_prism() {
    assert_boundary_bijection(&u_prism());
}

#[test]
fn bijection_holed_plate() {
    assert_boundary_bijection(&holed_plate());
}

// =========================================================================
// Oracle 5 — determinism (two BRep::new calls → identical mesh tris).
// =========================================================================

#[test]
fn determinism_u_prism() {
    let a = u_prism();
    let b = u_prism();
    assert_eq!(
        a.as_mesh().tris,
        b.as_mesh().tris,
        "U-prism tessellation is not deterministic across two BRep::new calls"
    );
}

#[test]
fn determinism_holed_plate() {
    let a = holed_plate();
    let b = holed_plate();
    assert_eq!(
        a.as_mesh().tris,
        b.as_mesh().tris,
        "holed-plate tessellation is not deterministic across two BRep::new calls"
    );
}

// =========================================================================
// Oracle 6 — convex box regression (fan path unchanged; should PASS in RED).
// =========================================================================

#[test]
fn convex_box_fan_regression() {
    let brep = unit_box();
    let mesh = brep.as_mesh();

    // Unit cube: 6 faces × 2 tris = 12 triangles, 8 vertices, surface area 6.0.
    assert_eq!(mesh.num_verts(), 8, "unit box should have 8 mesh verts");
    assert_eq!(mesh.num_tris(), 12, "unit box should have 12 triangles");
    let area = mesh_area(mesh);
    assert!(
        (area - 6.0).abs() < AREA_TAU,
        "unit box surface area {area} != 6.0"
    );
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "unit box mesh should be watertight"
    );
    // Fan path emits no Steiner points and every vert is a BRepVertex.
    assert_boundary_bijection(&brep);
}
