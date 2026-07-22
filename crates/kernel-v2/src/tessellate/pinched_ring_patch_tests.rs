//! #188 inc-4 — pinched-ring developable-patch tessellation (spec
//! `yang_188_f0082_j3_envelope_selection.md` §10.6): a hole loop sharing
//! exactly ONE B-Rep vertex with an outer/rim loop (a vertex-touching notch,
//! the F0082 FaceId(3727) class) must tessellate.
//!
//! Mechanism under test: pass 1.5 shared-vertex canonicalization in
//! `tessellate_developable_patch`. Each loop walk unrolls the shared vertex
//! independently — ulps apart by Δθ-accumulation rounding (F0082: 2.3e-15),
//! or a full span apart across the atan2 branch cut — so the flood-fill
//! CDT's shared-vertex weld (spec `kv2_cdt_triangulation_core` §6b M3b),
//! which needs BITWISE coincidence, engages only by luck; and even when it
//! does, the two copies carry DISTINCT node ids, so the refinement's
//! boundary-kind registry misses the pinch-adjacent hole edges (falls back
//! to Interior) and lifts their split midpoints onto the SURFACE instead of
//! the 3D chord — a silent ~1e-3 conformality crack against the
//! neighboring face's copy of the edge. Without pass 1.5 both barrel tests
//! fail the conformality pin; with it the pinch is one welded node in one
//! window and the splits stay on-chord.

use super::{tessellate_cylinder_patch, RenderMesh};
use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use cad_primitives::Point3;

const R: f64 = 0.73;
const H: f64 = 1.0;

fn cyl_point(theta: f64, z: f64) -> Point3 {
    Point3::new(R * theta.cos(), R * theta.sin(), z)
}

/// Wire one loop of `LineSegment` half-edges over existing vertices.
/// `he_base` is the first half-edge id to mint; returns the loop id.
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
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));
    fid
}

/// Max undirected index-edge incidence over the per-face mesh (a valid
/// partition has boundary edges at 1, interior edges at 2).
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

/// F0082 FaceId(3727) class, parameterized: a FULL barrel (two
/// opposite-winding rim loops of `n_rim` chords) with a notch hole PINNED
/// to bottom-rim vertex `pinch`. The notch's copy of the shared vertex is
/// unrolled by a different walk than the rim's copy — ulps apart in the
/// same window, or a full span apart when the notch's atan2 anchor picks
/// the other principal branch (θ_pinch > π) — so without pass 1.5 the CDT
/// weld cannot engage. After canonicalization the barrel tessellates with
/// the notch excluded and ONE welded pinch vertex.
fn barrel_with_pinned_notch(n_rim: usize, pinch: u32) {
    use std::f64::consts::PI;
    let mut arena = BrepArena::new();
    // Bottom rim verts (θ increasing, wrap +1), top rim (θ-decreasing
    // walk below, wrap −1).
    for k in 0..n_rim {
        let theta = 2.0 * PI * k as f64 / n_rim as f64;
        arena.vertices.push(Some(Vertex {
            point: cyl_point(theta, 0.0),
        }));
    }
    for k in 0..n_rim {
        let theta = 2.0 * PI * k as f64 / n_rim as f64;
        arena.vertices.push(Some(Vertex {
            point: cyl_point(theta, H),
        }));
    }
    // Notch: pinned at the rim vertex + three verts above at ASYMMETRIC
    // angles, walked starting at v_a so the pinch is reached by Δθ
    // ACCUMULATION from v_a's atan2 anchor (as in the F0082 face).
    let theta_p = 2.0 * PI * f64::from(pinch) / n_rim as f64;
    let v_a = arena.vertices.len() as u32;
    arena.vertices.push(Some(Vertex {
        point: cyl_point(theta_p - 0.13, 0.21),
    }));
    let v_b = arena.vertices.len() as u32;
    arena.vertices.push(Some(Vertex {
        point: cyl_point(theta_p - 0.02, 0.30),
    }));
    let v_c = arena.vertices.len() as u32;
    arena.vertices.push(Some(Vertex {
        point: cyl_point(theta_p + 0.17, 0.19),
    }));

    let fid = FaceId(0);
    let bottom: Vec<u32> = (0..n_rim as u32).collect();
    let top: Vec<u32> = (0..n_rim as u32)
        .map(|k| n_rim as u32 + (n_rim as u32 - k) % n_rim as u32)
        .collect(); // n_rim, 2n−1, 2n−2, … — θ decreasing
    let notch = vec![v_a, v_b, v_c, pinch]; // CW in the development (hole)
    let outer = add_loop(&mut arena, fid, 0, &bottom, LoopKind::Outer);
    let l_top = add_loop(&mut arena, fid, n_rim, &top, LoopKind::Inner);
    let l_notch = add_loop(&mut arena, fid, 2 * n_rim, &notch, LoopKind::Inner);
    let fid = finish_face(&mut arena, outer, vec![l_top, l_notch]);

    let mut mesh = RenderMesh::default();
    // n_seg = 64 puts the facet width BELOW the notch edges' Δu, so the
    // refinement must split them — exercising the boundary-kind lookup for
    // the pinch-adjacent notch chords (the conformality pin below).
    tessellate_cylinder_patch(&arena, fid, 64, &mut mesh).expect(
        "§10.6: a notch hole pinned to the rim at one shared vertex must \
         tessellate (the pinch copies must weld)",
    );
    assert!(
        mesh.indices.len() >= 3,
        "the barrel must emit a non-empty triangulation"
    );
    assert!(
        max_edge_incidence(&mesh) <= 2,
        "per-face partition stays edge-manifold (boundary 1, interior 2)"
    );
    // MECHANISM PIN: the shared vertex must be WELDED — exactly one
    // referenced render vertex at the pinch position. Without pass 1.5 the
    // two copies both enter the triangulation as distinct points (or the
    // whole ring is rejected), never one welded vertex.
    let pinch_pos = cyl_point(theta_p, 0.0);
    let referenced: std::collections::BTreeSet<u32> = mesh.indices.iter().copied().collect();
    let at_pinch = referenced
        .iter()
        .filter(|&&i| {
            let j = i as usize * 3;
            let d2 = (mesh.positions[j] - pinch_pos.x()).powi(2)
                + (mesh.positions[j + 1] - pinch_pos.y()).powi(2)
                + (mesh.positions[j + 2] - pinch_pos.z()).powi(2);
            d2 < 1e-12
        })
        .count();
    assert_eq!(
        at_pinch, 1,
        "the pinch vertex must be welded to ONE referenced render vertex"
    );
    // CONFORMALITY PIN (boundary-kind fidelity through the weld): every
    // refinement split of the pinch-adjacent notch chord must stay ON the
    // 3D chord (closure safety with the neighboring face's copy of the
    // edge). With MISMATCHED node ids — welded pool position but the
    // boundary registry still keyed under the hole walk's own node — the
    // edge kind falls back to Interior and the midpoint is lifted to the
    // cylinder SURFACE: a silent ~1e-3 sagitta crack. A vertex is "a split
    // of this edge" iff its development (wrapped-Δθ·R, z) lies on the 2D
    // edge segment; surface-lifted midpoints sit exactly there, while
    // honest 3D-chord splits deviate in development by the chord's angular
    // nonlinearity (≫ the band) and are simply not selected.
    let a_pos = cyl_point(theta_p - 0.13, 0.21);
    let wrap = |d: f64| {
        let mut r = d % (2.0 * PI);
        if r > PI {
            r -= 2.0 * PI;
        }
        if r < -PI {
            r += 2.0 * PI;
        }
        r
    };
    let seg2 = (wrap(-0.13) * R, 0.21f64); // v_a in the pinch-anchored chart
    for &i in &referenced {
        let j = i as usize * 3;
        let (x, y, z) = (
            mesh.positions[j],
            mesh.positions[j + 1],
            mesh.positions[j + 2],
        );
        let uu = wrap(y.atan2(x) - theta_p) * R;
        // Distance from (uu, z) to the 2D segment (0,0)→seg2, interior only.
        let (dx, dz) = (seg2.0, seg2.1);
        let t = (uu * dx + z * dz) / (dx * dx + dz * dz);
        if !(0.01..=0.99).contains(&t) {
            continue;
        }
        let d2d = ((uu - t * dx).powi(2) + (z - t * dz).powi(2)).sqrt();
        if d2d > 1e-9 {
            continue;
        }
        // On the 2D edge ⇒ must be on the 3D chord pinch→v_a.
        let s = ((a_pos.x() - pinch_pos.x()) * (x - pinch_pos.x())
            + (a_pos.y() - pinch_pos.y()) * (y - pinch_pos.y())
            + (a_pos.z() - pinch_pos.z()) * (z - pinch_pos.z()))
            / ((a_pos.x() - pinch_pos.x()).powi(2)
                + (a_pos.y() - pinch_pos.y()).powi(2)
                + (a_pos.z() - pinch_pos.z()).powi(2));
        let (px, py, pz) = (
            pinch_pos.x() + s * (a_pos.x() - pinch_pos.x()),
            pinch_pos.y() + s * (a_pos.y() - pinch_pos.y()),
            pinch_pos.z() + s * (a_pos.z() - pinch_pos.z()),
        );
        let d3 = ((x - px).powi(2) + (y - py).powi(2) + (z - pz).powi(2)).sqrt();
        assert!(
            d3 < 1e-9,
            "a split of the pinch-adjacent notch chord left the 3D chord \
             (d={d3:.3e}) — boundary-kind lookup lost through the weld"
        );
    }
    // The notch interior is EXCLUDED: no triangle centroid may land at the
    // notch midpoint (strictly inside the hole).
    let hole_mid = cyl_point(theta_p, 0.12);
    for t in mesh.indices.chunks(3) {
        let p = |i: u32| {
            let j = i as usize * 3;
            [
                mesh.positions[j],
                mesh.positions[j + 1],
                mesh.positions[j + 2],
            ]
        };
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        let cen = [
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        ];
        let d2 = (cen[0] - hole_mid.x()).powi(2)
            + (cen[1] - hole_mid.y()).powi(2)
            + (cen[2] - hole_mid.z()).powi(2);
        assert!(
            d2 > 1e-4,
            "no triangle centroid may sit at the notch-hole midpoint (hole \
             must be excluded from the cover)"
        );
    }
}

/// Same-window pinch (θ_pinch = π/2 < π): both walks unroll the pinch into
/// the atan2 principal branch — the copies differ only by accumulation
/// rounding; pass 1.5 merges at k = 0.
#[test]
fn pinned_notch_on_full_barrel_tessellates() {
    barrel_with_pinned_notch(16, 4);
}

/// Seam-window pinch (θ_pinch = 5π/4 > π): the notch walk's atan2 anchor
/// lands in the (−π, π] branch while the rim walk accumulated past the
/// cut — the copies sit a FULL SPAN apart; pass 1.5 rigidly translates the
/// notch chain into the rim's window (k = −1) before merging.
#[test]
fn pinned_notch_across_seam_branch_tessellates() {
    barrel_with_pinned_notch(96, 60);
}

/// Bounded-patch (0-wrap) analogue: a partial cylinder wall whose outer loop
/// carries a 3-vertex notch hole pinned at one shared bottom vertex. In the
/// bounded branch both loops read `nodes[n].p2` directly, so the pass-1.5
/// node merge alone makes the copies bitwise equal.
#[test]
fn pinned_notch_on_bounded_patch_tessellates() {
    let mut arena = BrepArena::new();
    // Outer: bottom θ = 0.3, 0.8, 1.3, 1.8 (z=0), then top θ = 1.8 … 0.3
    // (z=H) — CCW in the development.
    let thetas = [0.3f64, 0.8, 1.3, 1.8];
    for &t in &thetas {
        arena.vertices.push(Some(Vertex {
            point: cyl_point(t, 0.0),
        }));
    }
    for &t in thetas.iter().rev() {
        arena.vertices.push(Some(Vertex {
            point: cyl_point(t, H),
        }));
    }
    // Notch pinned at bottom vertex θ=0.8 (id 1).
    let pinch = 1u32;
    let v_a = arena.vertices.len() as u32;
    arena.vertices.push(Some(Vertex {
        point: cyl_point(0.7, 0.2),
    }));
    let v_b = arena.vertices.len() as u32;
    arena.vertices.push(Some(Vertex {
        point: cyl_point(0.9, 0.2),
    }));

    let fid = FaceId(0);
    let outer_ids: Vec<u32> = (0..8).collect();
    let notch = vec![pinch, v_a, v_b]; // CW in the development (hole)
    let outer = add_loop(&mut arena, fid, 0, &outer_ids, LoopKind::Outer);
    let l_notch = add_loop(&mut arena, fid, 8, &notch, LoopKind::Inner);
    let fid = finish_face(&mut arena, outer, vec![l_notch]);

    let mut mesh = RenderMesh::default();
    tessellate_cylinder_patch(&arena, fid, 16, &mut mesh).expect(
        "§10.6 bounded branch: a notch pinned to the outer loop at one \
         shared vertex must tessellate",
    );
    assert!(mesh.indices.len() >= 3);
    assert!(max_edge_incidence(&mesh) <= 2);
}
