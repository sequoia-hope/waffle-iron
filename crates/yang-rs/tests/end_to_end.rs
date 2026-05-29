//! End-to-end integration test through the real `mesh_booleans` binary.
//!
//! Self-skips when `CHERCHI2022_BIN` env var doesn't resolve to an
//! existing file. Build per `docs/sidecar/cherchi2022_build_guide.md`.

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::Mesh;
use cherchi_sidecar_rs::SidecarBoolean;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// NOTE: the pre-M3 sidecar tests `end_to_end_intersect_via_sidecar`,
// `end_to_end_union_via_sidecar`, `end_to_end_intersect_attribution_has_some_via_sidecar`,
// `end_to_end_union_attribution_has_none_via_sidecar`,
// `end_to_end_intersect_yields_brep_faces_via_sidecar`, and
// `end_to_end_face_count_matches_attribution_components_via_sidecar`
// (with the `unit_cube_at` / `unit_cube_brep_at` / `run_op_via_sidecar`
// helpers) were REMOVED — superseded by the `m3_*` oracle tests below.
// M3's boolean() requires topologized inputs with correct plane offsets;
// those tests used `BRep::from_mesh` (no faces → FaceResolutionFailed) or
// asserted the pre-M3 substitute's partial/None attribution. The new M3
// oracles (volume / watertight / Euler / full coverage on
// `unit_cube_brep_offset_at`) cover the same ground far more strongly.

// =========================================================================
// M3 — functional watertight boolean via LabeledArrangement
//
// Canonical case: diagonal-offset cubes A@[0,0,0], B@[0.5,0.5,0.5] (no
// coplanar faces, no interior face-pierce). The M3 win is at the B-Rep /
// attribution level: FULL attribution (every output tri Some) + a closed,
// 2-manifold output with the analytic signed volume. These FAIL on the
// pre-M3 substitute (leaves Nones / skeleton). Spec:
// specs/yang_m3_functional_boolean.md (I7/I8/I9/I10).
//
// Self-skip on missing binary (BinaryNotFound).
// =========================================================================

/// Unit cube BRep at `origin` with correct OUTWARD normals AND correct
/// plane offsets (`n·x + d = 0`). The existing `unit_cube_brep_at` uses
/// `d = 0.0` for every face (only valid for a cube through the origin),
/// which the M3 centroid-in-plane face resolution cannot use; this fixture
/// carries the true per-face offset so geometric resolution succeeds.
fn unit_cube_brep_offset_at(origin: [f64; 3]) -> BRep {
    let [x, y, z] = origin;
    let verts = vec![
        BRepVertex { point: p(x, y, z) },
        BRepVertex {
            point: p(x + 1.0, y, z),
        },
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z),
        },
        BRepVertex {
            point: p(x, y + 1.0, z),
        },
        BRepVertex {
            point: p(x, y, z + 1.0),
        },
        BRepVertex {
            point: p(x + 1.0, y, z + 1.0),
        },
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z + 1.0),
        },
        BRepVertex {
            point: p(x, y + 1.0, z + 1.0),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // F0 bottom (z)
        [4, 7, 6, 5], // F1 top (z+1)
        [0, 4, 5, 1], // F2 front (y)
        [1, 5, 6, 2], // F3 right (x+1)
        [2, 6, 7, 3], // F4 back (y+1)
        [3, 7, 4, 0], // F5 left (x)
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
    let normals: [Vector3; 6] = [
        Vector3::new(0.0, 0.0, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(-1.0, 0.0, 0.0),
    ];
    // Plane n·x + d = 0. For face on plane n·X = c (c = the coordinate of
    // that face), d = -c. With outward normals:
    //   bottom z=z   : n=-z  ⇒ -z·z = -z, need n·X+d=0 at X.z=z ⇒ d=z
    //   top    z=z+1 : n=+z  ⇒ d=-(z+1)
    //   front  y=y   : n=-y  ⇒ d=y
    //   right  x=x+1 : n=+x  ⇒ d=-(x+1)
    //   back   y=y+1 : n=+y  ⇒ d=-(y+1)
    //   left   x=x   : n=-x  ⇒ d=x
    let offs = [z, -(z + 1.0), y, -(x + 1.0), -(y + 1.0), x];
    let faces: Vec<BRepFace> = (0..6)
        .map(|i| BRepFace {
            surface: Surface::Plane {
                normal: normals[i],
                d: offs[i],
            },
            outer_loop: loops[i].clone(),
        })
        .collect();
    BRep::new(verts, edges, faces).expect("offset cube BRep::new failed")
}

/// Signed volume V = (1/6) Σ v0 · (v1 × v2) over all triangles.
fn signed_volume(mesh: &Mesh) -> f64 {
    let mut acc = 0.0;
    for tri in &mesh.tris {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        // cross = b × c
        let cx = b[1] * c[2] - b[2] * c[1];
        let cy = b[2] * c[0] - b[0] * c[2];
        let cz = b[0] * c[1] - b[1] * c[0];
        acc += a[0] * cx + a[1] * cy + a[2] * cz;
    }
    acc / 6.0
}

/// Count directed half-edges with no opposite. Watertight ⇒ 0 unpaired.
fn unpaired_half_edges(mesh: &Mesh) -> usize {
    use std::collections::HashMap;
    let mut counts: HashMap<(u32, u32), i32> = HashMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *counts.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut unpaired = 0;
    for (&(s, e), &fwd) in &counts {
        let rev = counts.get(&(e, s)).copied().unwrap_or(0);
        // Each forward directed edge must have exactly one opposite.
        if fwd != rev {
            unpaired += (fwd - rev).unsigned_abs() as usize;
        }
    }
    unpaired
}

/// Euler V − E + F over the mesh (E = unique undirected edges).
fn euler_characteristic(mesh: &Mesh) -> i64 {
    use std::collections::HashSet;
    let v = mesh.num_verts() as i64;
    let f = mesh.num_tris() as i64;
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    let e = edges.len() as i64;
    v - e + f
}

/// Run M3 oracles for one op over the diagonal cubes.
fn m3_oracles(op: BoolOp, expected_volume: f64) {
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[yang-rs end_to_end] SKIP: sidecar binary not found");
        return;
    };
    let a = unit_cube_brep_offset_at([0.0, 0.0, 0.0]);
    let b = unit_cube_brep_offset_at([0.5, 0.5, 0.5]);
    let r = boolean(&a, &b, op, &sb).expect("yang-rs M3 boolean failed");

    // B-Rep reconstructed.
    assert!(
        !r.faces().is_empty(),
        "{op:?}: output BRep should have ≥1 face"
    );

    // I7 + full coverage: every output triangle is Some-attributed.
    let attr = r.triangle_attribution();
    assert_eq!(
        attr.len(),
        r.num_tris(),
        "{op:?}: attribution length must equal output tri count"
    );
    assert!(r.num_tris() > 0, "{op:?}: output mesh is empty");
    for t in 0..attr.len() as u32 {
        assert!(
            attr.lookup(t).is_some(),
            "{op:?}: tri {t} is None — M3 requires FULL attribution (closed B-Rep)"
        );
    }

    // I9 signed volume (with sign).
    let vol = signed_volume(r.as_mesh());
    assert!(
        (vol - expected_volume).abs() < 1e-6,
        "{op:?}: signed volume {vol} != expected {expected_volume}"
    );

    // I8 watertight.
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "{op:?}: output mesh has unpaired half-edges (not watertight)"
    );

    // I10 Euler V−E+F = 2 (genus 0).
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "{op:?}: Euler characteristic != 2"
    );
}

#[test]
fn m3_union_diagonal_cubes_oracles_via_sidecar() {
    // Union = 1 + 1 − 0.125 = 1.875.
    m3_oracles(BoolOp::Union, 1.875);
}

#[test]
fn m3_intersect_diagonal_cubes_oracles_via_sidecar() {
    // Intersect = overlap = 0.5³ = 0.125.
    m3_oracles(BoolOp::Intersect, 0.125);
}

#[test]
fn m3_subtract_diagonal_cubes_oracles_via_sidecar() {
    // A − B = 1 − 0.125 = 0.875.
    m3_oracles(BoolOp::Subtract, 0.875);
}
