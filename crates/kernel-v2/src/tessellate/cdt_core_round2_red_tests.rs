//! KV2 CDT triangulation core — RED tests, ROUND 2 (spec
//! `specs/kv2_cdt_triangulation_core.md` §6b: the three Phase-4-assay
//! follow-up mechanisms M1/M2/M3).
//!
//! Round 1 swapped both f64 render cores to the exact-predicate CDT
//! primitive. The full assay then measured three regression classes; this
//! module banks the measured witnesses and asserts the round-2 SPEC TARGET,
//! so each test is RED until the corresponding mechanism lands:
//!
//! * `red_m1_f0016_planar_ring_no_grid_degenerate` — M1 grid-degeneracy
//!   flip pass, planar path. The banked F0016 FaceId(61) 6-vertex ring: the
//!   CDT prefers the on-line chord diagonal, minting a boundary-chord sliver
//!   flatter than the render weld grid (`max_abs·TAU_TESS_GRID_FACTOR`) — a
//!   grid-degenerate the bitwise B2/B3 gates cannot see.
//! * `red_m1_r0040_patch_ring_tessellates_clean` — M1 flip pass, cylinder
//!   patch path. The banked R0040 FaceId(23) 28-vertex barrel-cut ring:
//!   today rejected loudly by the G0 gate (the same chord sliver, bitwise).
//! * `red_m3_pinch_ring_tessellates` — M3 pinch-splitting. A weakly-simple
//!   ring visiting one geometric point through two distinct arena vertices
//!   at bitwise-identical positions → spade `DuplicateVertex` today.
//! * `guard_m3_consecutive_duplicate_stays_loud` — GUARD (M3 boundary):
//!   a zero-length edge (two CONSECUTIVE coincident vertices) must stay
//!   loud both before and after M3.
//!
//! (M2 flood-fill interior classification is exercised at the cherchi-rs
//! primitive level — see `triangulation::floodfill_red_tests` — and E2E by
//! the full-assay F0047 diff; the primitive-level centroid-parity defect
//! could not be reproduced synthetically, see that module's note.)
//!
//! In-module because the target fns are private (same idiom as
//! `cdt_core_red_tests`). Predicate helpers are re-declared (not importable
//! across cfg(test) sibling modules). The banked coordinate builders carry
//! `#[rustfmt::skip]` to preserve the banked one-triple-per-line layout.
use super::{tessellate_cylinder_patch, tessellate_planar_face};
use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use crate::tessellate::RenderMesh;
use cad_primitives::Point3;
use waffle_types::kernel::units::{TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN};

// ── shared predicates (re-declared; private per cfg(test) module) ──────

/// (b2, b3_only): triangles with two bitwise-equal f32 verts (B2); and
/// triangles with three DISTINCT f32 verts but an exactly-zero f32 cross
/// (B3-only). The bitwise render-degeneracy gate's own predicate.
fn scan_degeneracy(mesh: &RenderMesh) -> (usize, usize) {
    let key = |i: usize| -> [u32; 3] {
        [
            (mesh.positions[3 * i] as f32).to_bits(),
            (mesh.positions[3 * i + 1] as f32).to_bits(),
            (mesh.positions[3 * i + 2] as f32).to_bits(),
        ]
    };
    let fpos = |i: usize| -> [f32; 3] {
        [
            mesh.positions[3 * i] as f32,
            mesh.positions[3 * i + 1] as f32,
            mesh.positions[3 * i + 2] as f32,
        ]
    };
    let (mut b2, mut b3) = (0usize, 0usize);
    for t in mesh.indices.chunks_exact(3) {
        let (ka, kb, kc) = (key(t[0] as usize), key(t[1] as usize), key(t[2] as usize));
        if ka == kb || kb == kc || ka == kc {
            b2 += 1;
            continue;
        }
        let (fa, fb, fc) = (
            fpos(t[0] as usize),
            fpos(t[1] as usize),
            fpos(t[2] as usize),
        );
        let uu = [fb[0] - fa[0], fb[1] - fa[1], fb[2] - fa[2]];
        let vv = [fc[0] - fa[0], fc[1] - fa[1], fc[2] - fa[2]];
        let cx = uu[1] * vv[2] - uu[2] * vv[1];
        let cy = uu[2] * vv[0] - uu[0] * vv[2];
        let cz = uu[0] * vv[1] - uu[1] * vv[0];
        if cx == 0.0 && cy == 0.0 && cz == 0.0 {
            b3 += 1;
        }
    }
    (b2, b3)
}

/// M1 grid-degeneracy count (spec §6b): emitted triangles whose f32-rounded
/// height is below the shared render weld grid
/// `(max_abs·TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN)` — the SAME
/// constant the watertight oracle welds at (A3.3 single ownership), ~100×
/// coarser than f32 ulp, so it sees the boundary-chord sliver the bitwise
/// `scan_degeneracy` cannot. Heights on the f32-rounded 3D positions with
/// f32 arithmetic, matching `oracle::check_no_degenerate_triangles`.
fn grid_degenerate(mesh: &RenderMesh) -> usize {
    let fp = |i: u32| -> [f32; 3] {
        let i = i as usize * 3;
        [
            mesh.positions[i] as f32,
            mesh.positions[i + 1] as f32,
            mesh.positions[i + 2] as f32,
        ]
    };
    let max_abs = mesh
        .positions
        .iter()
        .map(|&p| (p as f32).abs())
        .fold(0.0_f32, f32::max) as f64;
    let grid = (max_abs * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let mut count = 0usize;
    for t in mesh.indices.chunks_exact(3) {
        let (pa, pb, pc) = (fp(t[0]), fp(t[1]), fp(t[2]));
        let ax = pb[0] - pa[0];
        let ay = pb[1] - pa[1];
        let az = pb[2] - pa[2];
        let bx = pc[0] - pa[0];
        let by = pc[1] - pa[1];
        let bz = pc[2] - pa[2];
        let cx = ay * bz - az * by;
        let cy = az * bx - ax * bz;
        let cz = ax * by - ay * bx;
        let area = (cx * cx + cy * cy + cz * cz).sqrt() / 2.0;
        let max_side2 = (ax * ax + ay * ay + az * az)
            .max(bx * bx + by * by + bz * bz)
            .max((bx - ax) * (bx - ax) + (by - ay) * (by - ay) + (bz - az) * (bz - az));
        let height = if max_side2 > 0.0 {
            2.0 * area / max_side2.sqrt()
        } else {
            0.0
        };
        if (height as f64) < grid {
            count += 1;
        }
    }
    count
}

/// Highest incidence count over undirected triangle index-edges. A
/// watertight per-face partition has every edge count 1 (boundary) or 2
/// (interior); any edge shared by ≥3 triangles is a non-manifold fan.
fn max_edge_incidence(mesh: &RenderMesh) -> usize {
    use std::collections::HashMap;
    let mut counts: HashMap<(u32, u32), usize> = HashMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for &(a, b) in &[(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    counts.values().copied().max().unwrap_or(0)
}

// ── fixture builders ───────────────────────────────────────────────────

/// Build a single planar face from a projected loop of z-plane points at
/// `normal = +z, plane point = origin`, all LineSegment half-edges. Shared
/// builder for the synthetic M3 pinch + consecutive-duplicate fixtures.
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
            point: Point3::new(0.0, 0.0, 0.0),
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

/// Banked F0016 FaceId(61): a 6-vertex all-LineSegment planar ring (boolean
    /// output, coordinate scale ~0.28) that mints ONE boundary-chord sliver
    /// under the round-1 CDT — grid-degenerate but NOT bitwise-degenerate.
    /// Verbatim measured fixture (§6b, 2026-07-02).
    #[rustfmt::skip]
    fn build_f0016_planar() -> (BrepArena, FaceId) {
        let verts: [[f64; 3]; 6] = [
            [1.43678157469419809e-1, 1.15954355224674524e-1, 1.63568283439396556e-1],
            [1.43678157469419809e-1, 1.15954355224674524e-1, 1.84341198824998137e-1],
            [1.25307302742208193e-1, 1.39843650855904000e-1, 1.69508915250426079e-1],
            [6.58462835491393506e-2, 2.17166229861079974e-1, 1.88736982064941494e-1],
            [1.27043537062290351e-1, 1.37585867232146331e-1, 2.75724685742304187e-1],
            [1.95063117800456154e-1, 4.91338111305104769e-2, 1.46951793108171941e-1],
        ];
        let mut arena = BrepArena::new();
        let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
        let n = verts.len();
        for p in &verts {
            arena.vertices.push(Some(Vertex { point: Point3::new(p[0], p[1], p[2]) }));
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
        arena.loops.push(Some(Loop { face: fid, boundary: LoopBoundary::Edges(HalfEdgeId(0)), kind: LoopKind::Outer }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Plane(Plane {
                point: Point3::new(1.43678157469419809e-1, 1.15954355224674524e-1, 1.63568283439396556e-1),
                normal: UnitVector3 { x: 7.92712605646587187e-1, y: 6.09595542018639081e-1, z: 0.00000000000000000e0 },
            })),
            outer_loop: lid,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.shells.push(Some(Shell { solid, faces: vec![fid], genus: 0 }));
        arena.solids.push(Some(Solid { shells: vec![shell] }));
        (arena, fid)
    }

/// Banked R0040 FaceId(23): a 28-vertex all-LineSegment CYLINDER-PATCH ring
    /// (barrel-cut boundary, n_seg=71, coordinate scale ~44) that mints the
    /// same boundary-chord sliver under the round-1 CDT — today rejected loudly
    /// by the G0 gate. Verbatim measured fixture (§6b, 2026-07-02).
    #[rustfmt::skip]
    fn build_r0040_patch() -> (BrepArena, FaceId) {
        let verts: [[f64; 3]; 28] = [
            [-2.29658777157921712e1, 9.28562110120933148e-1, 2.61019467100763265e1],
            [-2.82085496603433690e1, -1.33598555438530724e1, 2.64480149505118050e1],
            [-3.29625819957986295e1, -2.63165311222440863e1, 2.00224421426300090e1],
            [-3.62461006855921326e1, -3.52654577212390876e1, 8.15233392327518658e0],
            [-3.73809442191168628e1, -3.83583688456449750e1, -6.71071698596150323e0],
            [-3.61327276947762712e1, -3.49564701170955985e1, -2.14969704650776876e1],
            [-3.27592515040658157e1, -2.57623726667096093e1, -3.31525477507866242e1],
            [-2.79572565294419420e1, -1.26749793498302701e1, -3.92701642664410002e1],
            [-2.27185227708299706e1, 1.60270514248613161e0, -3.85863181259494041e1],
            [-1.81250320699813869e1, 1.41218393392356756e1, -3.12422474873586040e1],
            [-1.51255009980336563e1, 2.22967839429324997e1, -1.87547599308271913e1],
            [-1.43394376848438565e1, 2.44391268245991000e1, -3.70295861521555469e0],
            [-1.59291917115374382e1, 2.01063992115175623e1, 1.08044327755573200e1],
            [-1.95664232408577021e1, 1.01934609722397500e1, 2.17711302577531427e1],
            [-3.87002022096730958e0, 4.43417441332113427e0, 2.17711302577531498e1],
            [-2.32788691647042967e-1, 1.43471126525989554e1, 1.08044327755573129e1],
            [1.35696533504653694e0, 1.86798402656804861e1, -3.70295861521556091e0],
            [5.70902021856738884e-1, 1.65374973840138928e1, -1.87547599308271842e1],
            [-2.42862905009098906e0, 8.36255278031708116e0, -3.12422474873585934e1],
            [-7.02211975093957541e0, -4.15658141643247703e0, -3.85863181259494112e1],
            [-1.22608535095515538e1, -1.84342659087488983e1, -3.92701642664410073e1],
            [-1.70628484841754258e1, -3.15216592256282446e1, -3.31525477507866242e1],
            [-2.04363246748858813e1, -4.07157566760142231e1, -2.14969704650776983e1],
            [-2.16845411992264729e1, -4.41176554045635996e1, -6.71071698596152633e0],
            [-2.05496976657017427e1, -4.10247442801577193e1, 8.15233392327518125e0],
            [-1.72661789759082396e1, -3.20758176811627109e1, 2.00224421426300196e1],
            [-1.25121466404529738e1, -1.91191421027716792e1, 2.64480149505118121e1],
            [-7.26947469590178130e0, -4.83072444879768526e0, 2.61019467100763265e1],
        ];
        let mut arena = BrepArena::new();
        let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
        let n = verts.len();
        for p in &verts {
            arena.vertices.push(Some(Vertex { point: Point3::new(p[0], p[1], p[2]) }));
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
        arena.loops.push(Some(Loop { face: fid, boundary: LoopBoundary::Edges(HalfEdgeId(0)), kind: LoopKind::Outer }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Cylinder {
                axis_point: Point3::new(-17.99445556589601, -9.791477331725282, -6.338879609194002),
                axis_dir: UnitVector3 { x: 0.938800151062225, y: -0.3444623003545433, z: 0.0 },
                radius: 33.49858032434566,
                reversed: false,
            }),
            outer_loop: lid,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.shells.push(Some(Shell { solid, faces: vec![fid], genus: 0 }));
        arena.solids.push(Some(Solid { shells: vec![shell] }));
        (arena, fid)
    }

// ── tests ──────────────────────────────────────────────────────────────

/// RED (M1, planar): the banked F0016 6-vertex ring must tessellate into an
/// exact 4-triangle partition of the simple hexagon with ZERO
/// grid-degenerate triangles and a watertight per-face partition. TODAY the
/// CDT emits the on-line boundary-chord sliver (grid-degenerate, invisible
/// to the bitwise gates), so `grid_degenerate` returns ≥1 — RED.
#[test]
fn red_m1_f0016_planar_ring_no_grid_degenerate() {
    let (arena, fid) = build_f0016_planar();
    let mut mesh = RenderMesh::default();
    // n_seg is irrelevant for an all-LineSegment planar loop.
    tessellate_planar_face(&arena, fid, 32, &mut mesh)
        .expect("M1: the banked F0016 planar ring must tessellate");
    assert_eq!(
        grid_degenerate(&mesh),
        0,
        "M1: zero grid-degenerate triangles (RED today: the CDT mints the \
             on-line boundary-chord sliver flatter than max_abs·TAU_TESS_GRID_FACTOR)"
    );
    assert_eq!(
        mesh.indices.len() / 3,
        4,
        "exact partition of the simple hexagon: 6 - 2 = 4 triangles"
    );
    assert!(
        max_edge_incidence(&mesh) <= 2,
        "watertight per-face partition — every undirected index-edge count 1 or 2"
    );
}

/// RED (M1, cylinder patch): the banked R0040 28-vertex barrel-cut ring
/// must tessellate with ZERO f32-degenerate (bitwise B2/B3) AND ZERO
/// grid-degenerate triangles. TODAY it fails loudly at the G0 render gate
/// (the ear-clip/CDT boundary-chord sliver), so `expect` panics — RED via
/// the gate error.
#[test]
fn red_m1_r0040_patch_ring_tessellates_clean() {
    let (arena, fid) = build_r0040_patch();
    let mut mesh = RenderMesh::default();
    tessellate_cylinder_patch(&arena, fid, 71, &mut mesh).expect(
        "M1: the banked R0040 patch ring must tessellate cleanly (RED today: \
             the G0 render-degeneracy gate rejects the boundary-chord sliver)",
    );
    assert_eq!(
        scan_degeneracy(&mesh),
        (0, 0),
        "zero bitwise f32-degenerate triangles (B2 + B3) on the healthy R0040 ring"
    );
    assert_eq!(
        grid_degenerate(&mesh),
        0,
        "M1: zero grid-degenerate triangles on the healthy R0040 ring"
    );
}

/// RED (M3, pinch-splitting): a weakly-simple planar ring visiting the
/// geometric point (0, -2) twice — through two DISTINCT arena vertices at
/// bitwise-identical positions (a tangent pinch) — must tessellate into an
/// exact partition of the two CCW sub-rings. TODAY the coincident pool
/// vertices make spade return `DuplicateVertex`, mapped to a loud
/// `TessellationFailed`, so `expect` panics — RED.
///
/// Geometry: a big CCW square (−2,−2)..(2,2) whose bottom edge is pinched
/// at (0,−2) by a diamond lobe protruding INTO the square. Both sub-rings
/// are CCW by hand-shoelace: square pentagon area 16 (the pinch point sits
/// collinear on the bottom edge), diamond area 0.7 → partition area 16.7.
#[test]
fn red_m3_pinch_ring_tessellates() {
    // Loop order (weakly simple; edges share only the pinch point):
    //   square: (2,-2)(2,2)(-2,2)(-2,-2)  → P1(0,-2)
    //   diamond CCW: (0.5,-1.2)(0,-0.6)(-0.5,-1.2) → P2(0,-2)  → close.
    // P1 (idx 4) and P2 (idx 8) are two vertex ids at identical coords.
    let z = 0.0;
    let pts = [
        Point3::new(2.0, -2.0, z),  // 0
        Point3::new(2.0, 2.0, z),   // 1
        Point3::new(-2.0, 2.0, z),  // 2
        Point3::new(-2.0, -2.0, z), // 3
        Point3::new(0.0, -2.0, z),  // 4  P1 (pinch)
        Point3::new(0.5, -1.2, z),  // 5
        Point3::new(0.0, -0.6, z),  // 6
        Point3::new(-0.5, -1.2, z), // 7
        Point3::new(0.0, -2.0, z),  // 8  P2 (pinch twin of P1)
    ];
    let (arena, fid) = build_planar_loop(&pts);
    let mut mesh = RenderMesh::default();
    tessellate_planar_face(&arena, fid, 32, &mut mesh).expect(
        "M3: the pinch ring must tessellate (RED today: coincident pinch \
             vertices → CDT DuplicateVertex → TessellationFailed)",
    );

    // Exact partition: triangle areas (f64, xy plane, normal +z) sum to the
    // analytic partition area = square 16 + diamond 0.7 = 16.7.
    const ANALYTIC_AREA: f64 = 16.7;
    let pos = |vid: u32| -> [f64; 2] {
        let i = vid as usize * 3;
        [mesh.positions[i], mesh.positions[i + 1]]
    };
    let signed = |t: &[u32]| -> f64 {
        let (a, b, c) = (pos(t[0]), pos(t[1]), pos(t[2]));
        0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
    };
    let mut area_sum = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        area_sum += signed(t).abs();
    }
    assert!(
        (area_sum - ANALYTIC_AREA).abs() < 1e-9,
        "M3: exact partition area {area_sum} != analytic {ANALYTIC_AREA}"
    );

    // Non-inverted: every triangle shares one winding sign in the projected
    // frame (I4 — winding follows the CCW ring).
    let all_pos = mesh.indices.chunks_exact(3).all(|t| signed(t) > 0.0);
    let all_neg = mesh.indices.chunks_exact(3).all(|t| signed(t) < 0.0);
    assert!(
        all_pos || all_neg,
        "M3: all triangles must share one winding sign (no inverted triangle)"
    );

    // Watertight local pairing (the two sub-rings share the pinch VERTEX
    // but no edge — every undirected index-edge count 1 or 2).
    assert!(
        max_edge_incidence(&mesh) <= 2,
        "M3: watertight per-face partition — no edge shared by >2 triangles"
    );
}

/// GUARD (M3 boundary): a planar loop with two CONSECUTIVE coincident
/// vertices (a zero-length edge) must FAIL loudly. This passes TODAY (the
/// CDT rejects the coincident pair) and must keep passing after M3 — pinch
/// splitting handles NON-consecutive duplicates only; a zero-length edge
/// stays loud. Labeled a guard (not RED).
#[test]
fn guard_m3_consecutive_duplicate_stays_loud() {
    let z = 0.0;
    let pts = [
        Point3::new(0.0, 0.0, z), // 0
        Point3::new(2.0, 0.0, z), // 1
        Point3::new(2.0, 0.0, z), // 2  consecutive twin of 1 (zero-length edge)
        Point3::new(2.0, 2.0, z), // 3
        Point3::new(0.0, 2.0, z), // 4
    ];
    let (arena, fid) = build_planar_loop(&pts);
    let mut mesh = RenderMesh::default();
    match tessellate_planar_face(&arena, fid, 32, &mut mesh) {
        Err(KernelV2Error::TessellationFailed { face, .. }) => {
            assert_eq!(face, fid, "the guard must fail THIS planar face");
        }
        other => panic!(
            "a consecutive-duplicate (zero-length-edge) ring must fail loudly \
                 with TessellationFailed, got {other:?}"
        ),
    }
}

// ── ROUND 3 (M3 amendment, spec §6b M3a/M3b/M3c) ───────────────────────

/// Even-odd point-in-polygon in f64 (orientation-independent). Used to
/// assert hole exclusion — no emitted triangle centroid lands inside the
/// keyhole's diamond lobe.
fn point_in_poly_xy(px: f64, py: f64, poly: &[[f64; 2]]) -> bool {
    let n = poly.len();
    let mut inside = false;
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

/// RED (M3b, keyhole): a weakly-simple planar ring whose pinch split yields
/// ONE CCW sub-ring (the square) and ONE CW sub-ring (a tangent diamond
/// lobe → a HOLE touching the outer boundary at the pinch). TODAY round-2's
/// both-CCW rule rejects it loudly with `"pinch sub-ring is not CCW"`, so
/// `expect` panics — RED. TARGET (spec §6b M3b): outer = the CCW sub-ring,
/// hole = the CW sub-ring, triangulated via the flood-fill welding variant;
/// area = square − diamond.
///
/// Hand-shoelace: square pentagon 16 (the pinch sits collinear on the
/// bottom edge), diamond lobe wound CW area 0.7 → full-loop shoelace
/// 16 − 0.7 = 15.3.
#[test]
fn red_m3b_keyhole_ring_tessellates() {
    // Loop: square (2,-2)(2,2)(-2,2)(-2,-2) CCW → P1(0,-2), then the diamond
    // detour wound CW: (-0.5,-1.2)(0,-0.6)(0.5,-1.2) → P2(0,-2) → close.
    // P1 (idx 4) and P2 (idx 8) are two vertex ids at identical coords.
    let z = 0.0;
    let pts = [
        Point3::new(2.0, -2.0, z),  // 0
        Point3::new(2.0, 2.0, z),   // 1
        Point3::new(-2.0, 2.0, z),  // 2
        Point3::new(-2.0, -2.0, z), // 3
        Point3::new(0.0, -2.0, z),  // 4  P1 (pinch)
        Point3::new(-0.5, -1.2, z), // 5  ┐ diamond, CW → a HOLE
        Point3::new(0.0, -0.6, z),  // 6  │
        Point3::new(0.5, -1.2, z),  // 7  ┘
        Point3::new(0.0, -2.0, z),  // 8  P2 (pinch twin of P1)
    ];
    let (arena, fid) = build_planar_loop(&pts);
    let mut mesh = RenderMesh::default();
    tessellate_planar_face(&arena, fid, 32, &mut mesh).expect(
        "M3b: the keyhole ring must tessellate (RED today: the CW diamond \
             sub-ring is rejected by the round-2 both-CCW rule)",
    );

    // Exact partition: triangle areas (f64, xy plane, normal +z) sum to
    // square 16 − diamond 0.7 = 15.3 (the diamond is a hole).
    const DIAMOND_AREA: f64 = 0.7;
    const KEYHOLE_AREA: f64 = 16.0 - DIAMOND_AREA;
    let pos = |vid: u32| -> [f64; 2] {
        let i = vid as usize * 3;
        [mesh.positions[i], mesh.positions[i + 1]]
    };
    let signed = |t: &[u32]| -> f64 {
        let (a, b, c) = (pos(t[0]), pos(t[1]), pos(t[2]));
        0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
    };
    let mut area_sum = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        area_sum += signed(t).abs();
    }
    assert!(
        (area_sum - KEYHOLE_AREA).abs() < 1e-9,
        "M3b: keyhole area {area_sum} != square − diamond {KEYHOLE_AREA}"
    );

    // Hole exclusion: no emitted triangle centroid lies inside the diamond.
    let diamond = [[0.0, -2.0], [-0.5, -1.2], [0.0, -0.6], [0.5, -1.2]];
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (pos(t[0]), pos(t[1]), pos(t[2]));
        let cx = (a[0] + b[0] + c[0]) / 3.0;
        let cy = (a[1] + b[1] + c[1]) / 3.0;
        assert!(
            !point_in_poly_xy(cx, cy, &diamond),
            "M3b: a triangle centroid ({cx}, {cy}) lies inside the excluded diamond hole"
        );
    }

    // Non-inverted + watertight (the hole boundary and outer share only the
    // pinch VERTEX, no edge — every undirected index-edge count 1 or 2).
    let all_pos = mesh.indices.chunks_exact(3).all(|t| signed(t) > 0.0);
    let all_neg = mesh.indices.chunks_exact(3).all(|t| signed(t) < 0.0);
    assert!(
        all_pos || all_neg,
        "M3b: all triangles must share one winding sign (no inverted triangle)"
    );
    assert!(
        max_edge_incidence(&mesh) <= 2,
        "M3b: watertight per-face partition — no edge shared by >2 triangles"
    );
}

/// GUARD (M3c): a pinched ring whose BOTH sub-rings are CW (the round-2
/// M3a fixture with the whole loop reversed) is invalid winding and must
/// FAIL loudly. Passes TODAY (round-2 both-CCW rule) and must keep passing
/// after M3b — a mutation tripwire so the keyhole path (exactly-one-CCW)
/// never admits a fully-inverted (CW + CW) ring. Labeled a guard (not RED).
#[test]
fn guard_m3c_double_cw_stays_loud() {
    // The M3a both-CCW fixture reversed end-to-end: pinch at (0,-2) again,
    // but now the diamond sub-ring AND the square sub-ring are both CW.
    let z = 0.0;
    let pts = [
        Point3::new(0.0, -2.0, z),  // 0  P1 (pinch)
        Point3::new(-0.5, -1.2, z), // 1  ┐ diamond (CW)
        Point3::new(0.0, -0.6, z),  // 2  │
        Point3::new(0.5, -1.2, z),  // 3  ┘
        Point3::new(0.0, -2.0, z),  // 4  P2 (pinch)  → square (CW) follows
        Point3::new(-2.0, -2.0, z), // 5
        Point3::new(-2.0, 2.0, z),  // 6
        Point3::new(2.0, 2.0, z),   // 7
        Point3::new(2.0, -2.0, z),  // 8
    ];
    let (arena, fid) = build_planar_loop(&pts);
    let mut mesh = RenderMesh::default();
    match tessellate_planar_face(&arena, fid, 32, &mut mesh) {
        Err(KernelV2Error::TessellationFailed { face, .. }) => {
            assert_eq!(face, fid, "the guard must fail THIS planar face");
        }
        other => panic!(
            "a double-CW (invalid-winding) pinch ring must fail loudly with \
                 TessellationFailed, got {other:?}"
        ),
    }
}
