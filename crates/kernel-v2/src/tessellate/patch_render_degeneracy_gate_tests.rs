//! KV2 render-degeneracy gate (spec `specs/kv2_patch_render_degeneracy_gate.md`).
//!
//! `tessellate_cylinder_patch` is private, so these tests drive it directly
//! on a hand-built cylinder-patch arena (same in-module pattern as
//! `torus_patch_tess_tests`). RED: the gate does not exist yet, so a
//! sub-f32 boundary tessellates silently into a degenerate render mesh
//! instead of failing loudly.
use super::tessellate_cylinder_patch;
use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use crate::tessellate::RenderMesh;
use cad_primitives::Point3;

const N_SEG: u32 = 32;

/// A unit cylinder (axis +z through the origin) PATCH bounded by a single
/// LineSegment loop (a boolean-output patch). Boundary sampled as a
/// rectangle in (θ, z); `with_twin` inserts one extra boundary vertex
/// 1e-12 above its neighbor in z — below f32 resolution at this scale
/// (f32 ulp ≈ 1.2e-7 near magnitude 1), so the pair rounds to bitwise-equal
/// f32 positions while staying f64-valid (passes every existing loud gate).
fn build_cylinder_patch(with_twin: bool) -> (BrepArena, FaceId, usize) {
    let eval = |theta: f64, z: f64| Point3::new(theta.cos(), theta.sin(), z);
    let mut tz: Vec<(f64, f64)> = vec![(0.2, 0.0), (1.2, 0.0), (1.2, 0.5)];
    if with_twin {
        // Consecutive twin on the right edge: M = (1.2, 0.5) above, then
        // M2 = (1.2, 0.5 + 1e-12) — a ~1e-12 boundary edge.
        tz.push((1.2, 0.5 + 1e-12));
    }
    tz.push((1.2, 1.0));
    tz.push((0.2, 1.0));
    let bpts: Vec<Point3> = tz.iter().map(|&(t, z)| eval(t, z)).collect();
    let n = bpts.len();

    let mut arena = BrepArena::new();
    let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
    for p in &bpts {
        arena.vertices.push(Some(Vertex { point: *p }));
    }
    for i in 0..n {
        arena.half_edges.push(Some(HalfEdge {
            twin: HalfEdgeId(i as u32), // self — line segments never read the twin
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
        surface: Some(Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: UnitVector3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            radius: 1.0,
            reversed: false,
        }),
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
    (arena, fid, n)
}

/// Count emitted triangles with two bitwise-identical f32 vertex positions
/// (the B2 degeneracy — the assay `no_degenerate_triangles` witness applied
/// at the render channel's precision).
fn count_f32_degenerate(mesh: &RenderMesh) -> usize {
    let key = |i: usize| -> [u32; 3] {
        [
            (mesh.positions[3 * i] as f32).to_bits(),
            (mesh.positions[3 * i + 1] as f32).to_bits(),
            (mesh.positions[3 * i + 2] as f32).to_bits(),
        ]
    };
    let mut count = 0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (key(t[0] as usize), key(t[1] as usize), key(t[2] as usize));
        if a == b || b == c || a == c {
            count += 1;
        }
    }
    count
}

/// B2 (RED): a sub-f32 patch boundary must fail loudly with the typed
/// render-degeneracy reason — today it tessellates SILENTLY into a mesh
/// carrying degenerate f32 triangles.
#[test]
fn sub_f32_patch_boundary_fails_loudly() {
    let (arena, fid, _n) = build_cylinder_patch(true);
    let mut mesh = RenderMesh::default();
    let result = tessellate_cylinder_patch(&arena, fid, N_SEG, &mut mesh);

    match result {
        Err(KernelV2Error::TessellationFailed { face, reason }) => {
            assert_eq!(face, fid, "the gate must fail THIS patch face");
            assert_eq!(
                reason, "patch triangle collapsed at render precision",
                "the gate must use the spec's typed reason"
            );
        }
        Ok(()) => {
            // RED witness: today the patch tessellates AND emits degenerate
            // f32 triangles — a silently wrecked render mesh.
            let deg = count_f32_degenerate(&mesh);
            assert!(
                deg > 0,
                "fixture defect: expected a sub-f32 degenerate triangle, found none in {} tris",
                mesh.indices.len() / 3
            );
            panic!(
                "B2 RED: sub-f32 patch tessellated OK with {deg} of {} triangle(s) carrying \
                     two bitwise-identical f32 vertices (silent degenerate render mesh); spec \
                     requires TessellationFailed {{ reason: \"patch triangle collapsed at render \
                     precision\" }}",
                mesh.indices.len() / 3
            );
        }
        Err(e) => panic!(
            "expected the render-degeneracy gate (TessellationFailed), got a different \
                 error: {e:?}"
        ),
    }
}

/// B1 / I2 guard: the SAME patch without the sub-f32 twin tessellates and
/// emits NO f32-degenerate triangle — pins that the gate leaves clean
/// patches alone (mutation tripwire). (The full-solid canonical KV5b patch
/// path is covered end-to-end by `tests/kv5b_curved_boolean.rs`; this is
/// the direct-drive gate counterpart, not a duplicate.)
#[test]
fn canonical_patch_tessellates_without_f32_degeneracy() {
    let (arena, fid, n) = build_cylinder_patch(false);
    let mut mesh = RenderMesh::default();
    tessellate_cylinder_patch(&arena, fid, N_SEG, &mut mesh)
        .expect("B1: a clean cylinder patch must tessellate");
    assert!(mesh.num_vertices() >= n, "boundary vertices emitted");
    assert_eq!(
        count_f32_degenerate(&mesh),
        0,
        "B1: a clean patch must emit no f32-degenerate triangle"
    );
}

// Build a cylinder patch of the given radius with an arbitrary (theta,z)
// boundary chain (LineSegment loop). Returns (arena, face, n).
fn build_patch(radius: f64, tz: &[(f64, f64)]) -> (BrepArena, FaceId, usize) {
    let eval = |theta: f64, z: f64| Point3::new(radius * theta.cos(), radius * theta.sin(), z);
    let bpts: Vec<Point3> = tz.iter().map(|&(t, z)| eval(t, z)).collect();
    let n = bpts.len();
    let mut arena = BrepArena::new();
    let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
    for p in &bpts {
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
        surface: Some(Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: UnitVector3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            radius,
            reversed: false,
        }),
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
    (arena, fid, n)
}

// (b2, b3_only): triangles with two bitwise-equal f32 verts; and triangles
// with ALL THREE distinct f32 verts but exactly-zero f32 cross (B3-only).
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

// ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
// Attacks on the f32 render-precision gate. In-module (tessellate_cylinder_patch
// is private). Purely additive; touches no existing test. `build_patch` +
// `scan_degeneracy` above were localized with a throwaway probe.

/// Assert the FIRST returned error is the typed render-degeneracy failure.
fn assert_gate_fires(arena: &BrepArena, fid: FaceId) {
    let mut mesh = RenderMesh::default();
    match tessellate_cylinder_patch(arena, fid, N_SEG, &mut mesh) {
        Err(KernelV2Error::TessellationFailed { face, reason }) => {
            assert_eq!(face, fid);
            assert_eq!(reason, "patch triangle collapsed at render precision");
        }
        other => panic!("expected the render-degeneracy gate, got {other:?}"),
    }
}

/// MUTATION KILLER (b) — the B3 arm. A 3-vertex boundary triangle with a
/// SUB-f32 theta width (1e-9): all three vertices collapse to ONE cylinder
/// ruling in x,y at f32 while keeping DISTINCT z, so the emitted triangle has
/// three DISTINCT f32 positions but an exactly-zero f32 cross product — a
/// B3-ONLY degeneracy (no bitwise-equal pair; verified b2=0 via probe). The
/// B2 arm cannot see it; only B3 fires.
///
/// This is the case the RED suite never witnessed (it only exercised the B2
/// twin), so dropping the B3 arm SURVIVES the RED test but is KILLED here.
#[test]
fn adversary_b3_only_f32_collinear_ruling_fails_loudly() {
    let (arena, fid, _n) = build_patch(1.0, &[(1.2, 0.0), (1.2 + 1e-9, 0.5), (1.2, 1.0)]);
    assert_gate_fires(&arena, fid);
}

/// No over-fire: a single triangle whose theta width is JUST above f32
/// resolution (2.4e-7 ≈ 2× f32 ulp at scale 1) has three DISTINCT f32
/// vertices AND a nonzero f32 cross, so it must tessellate cleanly. A gate
/// widened from bitwise-exact to a tolerance would over-fire here.
#[test]
fn adversary_just_above_f32_resolution_does_not_over_fire() {
    let (arena, fid, _n) = build_patch(1.0, &[(1.2, 0.0), (1.2 + 2.4e-7, 0.5), (1.2, 1.0)]);
    let mut mesh = RenderMesh::default();
    tessellate_cylinder_patch(&arena, fid, N_SEG, &mut mesh)
        .expect("a supra-f32 triangle must tessellate (no over-fire)");
    assert_eq!(
        scan_degeneracy(&mesh),
        (0, 0),
        "a supra-f32 triangle must emit no f32-degenerate triangle"
    );
}

/// Scale-appropriateness: the gate is bitwise-f32, so it fires exactly when
/// the render precision at THAT coordinate magnitude can't resolve the
/// feature. The SAME 5e-5 z pair collapses at z≈1024 (f32 ulp ≈ 1.2e-4 >
/// 5e-5 → fires) but stays resolvable at z≈0.5 (f32 ulp ≈ 6e-8 ≪ 5e-5 →
/// Ok). Pins that firing tracks scale automatically (no absolute tolerance).
#[test]
fn adversary_gate_is_scale_appropriate() {
    // z≈1024: the 5e-5 pair rounds to one f32 → fires.
    let (arena, fid, _n) = build_patch(
        1.0,
        &[
            (0.2, 1024.0),
            (1.2, 1024.0),
            (1.2, 1024.25),
            (1.2, 1024.25 + 5e-5),
            (1.2, 1024.5),
            (0.2, 1024.5),
        ],
    );
    assert_gate_fires(&arena, fid);

    // z≈0.5: the same 5e-5 pair stays distinct at f32 → tessellates clean.
    let (arena, fid, _n) = build_patch(
        1.0,
        &[
            (0.2, 0.0),
            (1.2, 0.0),
            (1.2, 0.25),
            (1.2, 0.25 + 5e-5),
            (1.2, 0.5),
            (0.2, 0.5),
        ],
    );
    let mut mesh = RenderMesh::default();
    tessellate_cylinder_patch(&arena, fid, N_SEG, &mut mesh)
        .expect("the same feature at unit scale is f32-resolvable → Ok");
    assert_eq!(
        scan_degeneracy(&mesh),
        (0, 0),
        "unit-scale feature must emit no f32-degenerate triangle"
    );
}
