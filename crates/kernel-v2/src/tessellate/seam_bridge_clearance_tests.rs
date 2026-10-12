//! P0035 — a barrel patch's seam bridge must clear the boundary by render
//! resolution (spec `kv2_cdt_triangulation_core.md` §6g; ledger
//! `docs/yang_tail_triage.md` §2026-10-12; prospector seed 3 index 90,
//! `convex4:sym circle:boss`).
//!
//! A barrel segment (two opposite-winding axis-wrapping loops) is cut open
//! along a seam bridge pair anchored at a node of the +wrap loop; the
//! anchor search takes the first node whose two bridges no boundary edge
//! blocks. On P0035 the first outer vertex was the BOTTOM of a boundary
//! GENERATOR (the slab's +X cap meeting the boss wall along a line), so the
//! left bridge ran from the top rim straight down that generator's
//! supporting line to the anchor while the boundary generator ran back up
//! it — one line in 3D, two edges in the chart separated only by the
//! generator top's accumulated-u roundoff (5.08e-20 against a 6.9e-10
//! render floor). The exact crossing predicate saw a weakly simple ring,
//! the CDT tiled the one-ulp corridor with ears whose three corners lie on
//! one 3D generator, and the emit gate STOPped them ("patch triangle
//! collapsed at render precision"). The rule: a bridge that passes any
//! boundary node (other than its own endpoints) closer than
//! `render_height_floor` is rejected like a crossing one, and the anchor
//! moves to the generator's top.
//!
//! The fixture is that configuration with the roundoff under control: the
//! generator top sits `DELTA` radians off the seam azimuth, far below the
//! f32 floor at this scale (ulp ≈ 1.2e-7 near magnitude 1), so the
//! corridor is open in f64 and closed in f32 by construction.

use super::{tessellate_cylinder_patch, RenderMesh};
use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use cad_primitives::Point3;

const R: f64 = 0.73;
const H: f64 = 1.0;
const N_RIM: usize = 24;
const N_SEG: u32 = 32;

fn cyl_point(theta: f64, z: f64) -> Point3 {
    Point3::new(R * theta.cos(), R * theta.sin(), z)
}

fn add_loop(
    arena: &mut BrepArena,
    fid: FaceId,
    he_base: usize,
    origins: &[u32],
    kind: LoopKind,
) -> LoopId {
    let lid = LoopId(arena.loops.len() as u32);
    let n = origins.len();
    for (i, &v) in origins.iter().enumerate() {
        arena.half_edges.push(Some(HalfEdge {
            twin: HalfEdgeId((he_base + i) as u32),
            next: HalfEdgeId((he_base + (i + 1) % n) as u32),
            prev: HalfEdgeId((he_base + (i + n - 1) % n) as u32),
            origin: VertexId(v),
            loop_id: lid,
            curve: Curve::LineSegment,
        }));
    }
    arena.loops.push(Some(Loop {
        face: fid,
        boundary: LoopBoundary::Edges(HalfEdgeId(he_base as u32)),
        kind,
    }));
    lid
}

fn finish_face(arena: &mut BrepArena, outer: LoopId, inner: Vec<LoopId>) -> FaceId {
    let (shell, solid, fid) = (ShellId(0), SolidId(0), FaceId(0));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: UnitVector3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            radius: R,
            reversed: false,
        }),
        outer_loop: outer,
        inner_loops: inner,
        shell,
    }));
    arena.shells.push(Some(Shell {
        solid,
        faces: vec![fid],
        genus: 0,
    }));
    arena.solids.push(Some(Solid::constructed(vec![shell])));
    fid
}

fn max_edge_incidence(mesh: &RenderMesh) -> usize {
    let mut counts: std::collections::BTreeMap<(u32, u32), usize> =
        std::collections::BTreeMap::new();
    for t in mesh.indices.chunks(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *counts.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    counts.values().copied().max().unwrap_or(0)
}

/// Emitted triangles degenerate at f32: two bitwise-equal corners or an
/// exactly zero f32 cross product (the emit gate's own B2/B3 predicate).
fn count_f32_degenerate(mesh: &RenderMesh) -> usize {
    let p = |i: usize| -> [f32; 3] {
        [
            mesh.positions[3 * i] as f32,
            mesh.positions[3 * i + 1] as f32,
            mesh.positions[3 * i + 2] as f32,
        ]
    };
    let mut count = 0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0] as usize), p(t[1] as usize), p(t[2] as usize));
        if a == b || b == c || a == c {
            count += 1;
            continue;
        }
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cx = u[1] * v[2] - u[2] * v[1];
        let cy = u[2] * v[0] - u[0] * v[2];
        let cz = u[0] * v[1] - u[1] * v[0];
        if cx == 0.0 && cy == 0.0 && cz == 0.0 {
            count += 1;
        }
    }
    count
}

/// The P0035 barrel: the outer loop starts at the BOTTOM of a boundary
/// generator on the seam azimuth (θ = 0), climbs it to a top vertex
/// `delta` radians off the seam, runs a shelf to θ = 0.5 and back down to
/// the bottom rim, then follows the bottom rim around the axis back to the
/// start (wrap +1). The top rim at z = H is the second wrapping loop (wrap
/// −1), with a vertex at θ = 0 exactly — the bridge partner.
fn barrel_with_seam_generator(delta: f64) -> (BrepArena, FaceId) {
    use std::f64::consts::PI;
    let mut arena = BrepArena::new();
    let push = |arena: &mut BrepArena, theta: f64, z: f64| -> u32 {
        let id = arena.vertices.len() as u32;
        arena.vertices.push(Some(Vertex {
            point: cyl_point(theta, z),
        }));
        id
    };
    let a = push(&mut arena, 0.0, 0.0); // generator bottom — the first outer vertex
    let b = push(&mut arena, delta, 0.5 * H); // generator top
    let c = push(&mut arena, 0.5, 0.5 * H);
    let d = push(&mut arena, 0.5, 0.0);
    let mut outer = vec![a, b, c, d];
    // Bottom rim chords from θ = 0.5 around to θ = 2π (exclusive — closes at `a`).
    let m = N_RIM;
    for k in 1..m {
        let theta = 0.5 + (2.0 * PI - 0.5) * k as f64 / m as f64;
        outer.push(push(&mut arena, theta, 0.0));
    }
    // Top rim, θ-decreasing walk from θ = 0.
    let top_ids: Vec<u32> = (0..N_RIM)
        .map(|k| push(&mut arena, 2.0 * PI * k as f64 / N_RIM as f64, H))
        .collect();
    let top: Vec<u32> = (0..N_RIM).map(|k| top_ids[(N_RIM - k) % N_RIM]).collect();
    let fid = FaceId(0);
    let l_outer = add_loop(&mut arena, fid, 0, &outer, LoopKind::Outer);
    let l_top = add_loop(&mut arena, fid, outer.len(), &top, LoopKind::Inner);
    let fid = finish_face(&mut arena, l_outer, vec![l_top]);
    (arena, fid)
}

/// RED before the clearance rule: the anchor at the generator's bottom
/// put the left bridge one f64 ulp from the boundary generator; the CDT
/// tiled the corridor and the emit gates STOPped it — P0035's own
/// corridor as the bitwise "patch triangle collapsed at render
/// precision", this fixture's as the sibling "patch triangle below render
/// resolution" (its corridor ear is a hair wider than one f32 ulp; same
/// class, same producer). GREEN: the anchor moves to the generator's top;
/// the barrel renders with no f32-degenerate triangle and an edge-manifold
/// per-face partition.
#[test]
fn sub_floor_generator_corridor_moves_the_seam_anchor() {
    const DELTA: f64 = 1e-17; // 7.3e-18 in u — below f32 resolution at scale 1
    let (arena, fid) = barrel_with_seam_generator(DELTA);
    let mut mesh = RenderMesh::default();
    match tessellate_cylinder_patch(&arena, fid, N_SEG, &mut mesh) {
        Ok(()) => {}
        Err(KernelV2Error::TessellationFailed { reason, .. }) => panic!(
            "P0035 RED: the seam bridge hugged the boundary generator inside render \
             resolution and the barrel STOPped with {reason:?}"
        ),
        Err(e) => panic!("unexpected error {e:?}"),
    }
    assert!(mesh.indices.len() >= 3, "non-empty triangulation");
    assert_eq!(
        count_f32_degenerate(&mesh),
        0,
        "no emitted triangle may collapse at f32"
    );
    assert!(
        max_edge_incidence(&mesh) <= 2,
        "per-face partition stays edge-manifold"
    );
}

/// Companion: with the generator top VISIBLY off the seam (0.3 rad), the
/// first anchor's bridge clears every node and is accepted exactly as
/// before — the rule rejects nothing the render can represent.
#[test]
fn visible_generator_offset_keeps_the_first_anchor() {
    let (arena, fid) = barrel_with_seam_generator(0.3);
    let mut mesh = RenderMesh::default();
    tessellate_cylinder_patch(&arena, fid, N_SEG, &mut mesh)
        .expect("a barrel with a visible corridor tessellates");
    assert_eq!(count_f32_degenerate(&mesh), 0);
    assert!(max_edge_incidence(&mesh) <= 2);
}
