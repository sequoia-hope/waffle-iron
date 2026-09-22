//! M1c (spec `kv2_cdt_triangulation_core` §6e, R0085 2026-09-22): the grid-
//! degeneracy flip pass must never MINT a triangle the loud emit gates
//! refuse.
//!
//! The mechanism, replayed from R0085's cone face 1761 (chart dumped with
//! `KV2_SUBRES_DUMP`): a run of boundary vertices collinear up to rounding
//! noise (a plane∩cone generator split 69 times by the arrangement, chart
//! azimuth `u ≈ ±1e-16`) fanned from one far apex. Every fan triangle is a
//! little under the weld grid (height 6e-6 at grid 1.9e-5) with vertices
//! 4e-4 apart — legal for the render channel. The pass's old objective (the
//! below-grid COUNT) accepted swapping two fans for the ear over three
//! consecutive collinear vertices (height 1.3e-8, area 1.5e-13 — render
//! sub-resolution) plus one above-grid triangle: 2 → 1. The lexicographic
//! severity `(gate-refused, below-grid)` refuses that flip.
//!
//! The fixture is the planar analogue (same `grid_degeneracy_flip_pass`,
//! same predicates): a strip whose bottom edge carries three collinear
//! vertices, the middle one bulging OUTWARD by 1e-16 (a convex ear by
//! noise), with the far apices placed so each fan triangle is 0.7× the grid
//! high and their union is 1.4× (above). Certified RED on the shipped count
//! objective: the flip minted the ear, whose f32 cross product is exactly
//! zero, and the planar G1 gate refused the face.
use super::{tessellate_planar_face, RenderMesh};
use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use cad_primitives::Point3;

/// Build a single +z planar face from a CCW loop of z-plane points, all
/// LineSegment half-edges (the `cdt_core_adversary_tests` idiom).
fn build_planar_loop(pts: &[Point3]) -> (BrepArena, FaceId) {
    let n = pts.len();
    let mut arena = BrepArena::new();
    let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
    for p in pts {
        arena.vertices.push(Some(Vertex { point: *p }));
    }
    for i in 0..n {
        arena.half_edges.push(Some(HalfEdge {
            twin: HalfEdgeId(i as u32),
            next: HalfEdgeId(((i + 1) % n) as u32),
            prev: HalfEdgeId(((i + n - 1) % n) as u32),
            origin: VertexId(i as u32),
            loop_id: lid,
            curve: Curve::LineSegment,
        }));
    }
    arena.loops.push(Some(Loop {
        face: fid,
        boundary: LoopBoundary::Edges(HalfEdgeId(0)),
        kind: LoopKind::Outer,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: pts[0],
            normal: UnitVector3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
        })),
        outer_loop: lid,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.shells.push(Some(Shell {
        solid,
        faces: vec![fid],
        genus: 0,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));
    (arena, fid)
}

/// The corpus oracle's degenerate rule on an emitted mesh (f32 positions):
/// area < 1e-12 AND height < 4 f32 ulps of the coordinate scale.
fn subresolution_count(mesh: &RenderMesh) -> usize {
    let f = |i: u32| -> [f32; 3] {
        let k = i as usize * 3;
        [
            mesh.positions[k] as f32,
            mesh.positions[k + 1] as f32,
            mesh.positions[k + 2] as f32,
        ]
    };
    let max_abs = mesh
        .positions
        .iter()
        .map(|&c| (c as f32).abs())
        .fold(0.0_f32, f32::max);
    let floor = 4.0 * max_abs * f32::EPSILON;
    mesh.indices
        .chunks(3)
        .filter(|t| {
            let (a, b, c) = (f(t[0]), f(t[1]), f(t[2]));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cx = u[1] * v[2] - u[2] * v[1];
            let cy = u[2] * v[0] - u[0] * v[2];
            let cz = u[0] * v[1] - u[1] * v[0];
            let area = (cx * cx + cy * cy + cz * cz).sqrt() / 2.0;
            let w = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
            let l2 = |x: [f32; 3]| x[0] * x[0] + x[1] * x[1] + x[2] * x[2];
            let side = l2(u).max(l2(v)).max(l2(w)).sqrt();
            let h = if side > 0.0 { 2.0 * area / side } else { 0.0 };
            area < 1e-12 && h < floor
        })
        .count()
}

/// The R0085 strip, in the planar chart's own metric: a run of six vertices
/// on the line `x = 1.9` (every other one bulging OUTWARD by two ulps — a
/// convex ear by rounding noise, exactly collinear at f32), fanned from the
/// apex `A` that sits beside the run's end with lateral offset 2e-3 over a
/// 0.069 reach (slope 0.03, the R0085 ratio), and its mirror `B` below —
/// a thin strip, so every apex the CDT can fan from is a flat one (an
/// isotropic chart cannot otherwise reproduce R0085's fans, which were fat
/// in the anisotropic cone chart and flat only in 3D). Weld grid =
/// 1e-5 · max|coord| ≈ 1.9e-5; each fan triangle over one run segment is
/// 4.4e-4 · 0.03 ≈ 1.3e-5 high (below), the merged triangle over two
/// segments 2.6e-5 (above). CCW ring: R0, B, A, R5 … R1 (interior on the
/// `x > 1.9` side).
#[rustfmt::skip]
fn r0085_strip() -> Vec<Point3> {
    let x0 = 1.9;
    let seg = 4.4e-4;
    let bulge = 4.4e-16;
    let run: Vec<Point3> = (0..6)
        .map(|k| {
            let x = if k % 2 == 1 { x0 - bulge } else { x0 };
            Point3::new(x, seg * k as f64, 0.0)
        })
        .collect();
    let a = Point3::new(x0 + 2.0e-3, 0.069, 0.0);
    let b = Point3::new(x0 + 2.0e-3, -0.069, 0.0);
    let mut ring = vec![run[0], b, a];
    ring.extend(run[1..].iter().rev().copied());
    ring
}

/// M1c killer: the strip tessellates with ZERO render-sub-resolution
/// triangles and no ear over three consecutive run vertices. RED on the
/// count objective (the flip minted the ear; the planar G1 gate refused the
/// face with "planar triangle collapsed at render precision").
#[test]
fn m1c_flip_never_mints_a_subresolution_ear() {
    let pts = r0085_strip();
    let (arena, fid) = build_planar_loop(&pts);
    let mut mesh = RenderMesh::default();
    tessellate_planar_face(&arena, fid, 32, &mut mesh)
        .expect("M1c: the collinear-run strip must tessellate");
    assert_eq!(mesh.indices.len() / 3, pts.len() - 2, "n − 2 triangles");
    assert_eq!(
        subresolution_count(&mesh),
        0,
        "M1c: no emitted triangle may be render sub-resolution"
    );
    // Run vertices are ring positions 0 and 3..8 (R0, then R5 … R1): an ear
    // over three consecutive run vertices is any triangle whose corners all
    // lie on the run line.
    let on_run = |i: u32| i == 0 || i >= 3;
    let has_ear = mesh
        .indices
        .chunks(3)
        .any(|t| on_run(t[0]) && on_run(t[1]) && on_run(t[2]));
    assert!(!has_ear, "M1c: an ear over the collinear run was minted");
}

/// The shipped M1 killer's premise still holds under the severity pair: on
/// the concyclic flat quad (one below-grid, NOT sub-resolution triangle) the
/// pass still flips to the fat diagonal — severity (0, 1) → (0, 0).
#[test]
fn m1c_keeps_the_concyclic_tie_flip() {
    #[rustfmt::skip]
    let pts = [
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(0.9999957692054863, 0.0029088779843619342, 0.0),
        Point3::new(0.9999830768577442, 0.005817731354993834, 0.0),
        Point3::new(-1.0, 1.2246467991473532e-16, 0.0),
    ];
    let (arena, fid) = build_planar_loop(&pts);
    let mut mesh = RenderMesh::default();
    tessellate_planar_face(&arena, fid, 32, &mut mesh).expect("tessellates");
    assert_eq!(mesh.indices.len() / 3, 2);
    // The flat chord triangle (0, 1, 2) must have been flipped away.
    let has_flat = mesh.indices.chunks(3).any(|t| {
        let mut s = [t[0], t[1], t[2]];
        s.sort_unstable();
        s == [0, 1, 2]
    });
    assert!(
        !has_flat,
        "the concyclic tie must still flip to the fat diagonal"
    );
}
