use super::tessellate_torus_patch;
use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::tessellate::RenderMesh;
use cad_primitives::Point3;
use std::collections::BTreeMap;

/// A boolean-output torus PATCH (arbitrary polyline boundary, no full-circle
/// edge) tessellates — via the UV-CDT consumer — into a watertight, on-tube
/// mesh with the boundary preserved. This exercises the kernel-v2 render
/// wiring in isolation; the full boolean → reconstruction path is gated on
/// torus Stage-4 SSI relocation (its output boundary is chord-approximate,
/// see `kv6d_torus_boolean_recovery`).
#[test]
fn boolean_output_torus_patch_tessellates_watertight_and_on_surface() {
    let (r_maj, r_min) = (3.0_f64, 1.0_f64);
    // Torus center origin, axis +z, e1=+x, e2=+y.
    let eval = |u: f64, v: f64| -> Point3 {
        let rad = r_maj + r_min * u.cos();
        Point3::new(rad * v.cos(), rad * v.sin(), r_min * u.sin())
    };
    // A UV-rectangle patch boundary, 8 samples/side, all exactly on the tube.
    let (u0, u1, v0, v1) = (0.2_f64, 1.2, 0.5, 1.8);
    let ns = 8;
    let mut bpts: Vec<Point3> = Vec::new();
    let mut push = |u: f64, v: f64| bpts.push(eval(u, v));
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        push(u0 + (u1 - u0) * t, v0);
    }
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        push(u1, v0 + (v1 - v0) * t);
    }
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        push(u1 - (u1 - u0) * t, v1);
    }
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        push(u0, v1 - (v1 - v0) * t);
    }
    // A B-Rep loop puts the material on its LEFT about the face's outward
    // normal, which is CW in the consumer's (u, v) chart; the rectangle above
    // is walked CCW — reverse it (KV14 Slice F-3's region check).
    bpts.reverse();
    let n = bpts.len();

    // Minimal arena: one torus face bounded by a single LineSegment loop.
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
        surface: Some(Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: UnitVector3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            major_radius: r_maj,
            minor_radius: r_min,
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

    let mut mesh = RenderMesh::default();
    tessellate_torus_patch(&arena, fid, 24, &mut mesh).expect("torus patch tessellates");
    assert!(!mesh.indices.is_empty(), "non-empty patch mesh");

    // Boundary vertices preserved exactly (conformal): the first n emitted
    // positions are the input boundary.
    for (i, p) in bpts.iter().enumerate() {
        let k = i * 3;
        assert_eq!(mesh.positions[k], p.x(), "boundary x {i}");
        assert_eq!(mesh.positions[k + 1], p.y(), "boundary y {i}");
        assert_eq!(mesh.positions[k + 2], p.z(), "boundary z {i}");
    }

    // Steiner interior points added (refinement fired).
    assert!(mesh.num_vertices() > n, "interior Steiner points added");

    // Every render vertex lies on the tube within a tight band.
    for i in 0..mesh.num_vertices() {
        let k = i * 3;
        let (px, py, pz) = (
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        );
        let rho = (px * px + py * py).sqrt();
        let resid = (((rho - r_maj).powi(2) + pz * pz).sqrt() - r_min).abs();
        assert!(resid < 1e-9, "vertex {i} off tube: {resid}");
        // Outward normal agrees with (p − tubeCentre).
        let nrm = [mesh.normals[k], mesh.normals[k + 1], mesh.normals[k + 2]];
        let rhat = [px / rho, py / rho, 0.0];
        let out = [px - r_maj * rhat[0], py - r_maj * rhat[1], pz];
        assert!(
            nrm[0] * out[0] + nrm[1] * out[1] + nrm[2] * out[2] > 0.0,
            "vertex {i} normal not outward"
        );
    }

    // Watertight: every undirected (index) edge shared by 1 (boundary) or 2.
    let mut ec: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let e = if a < b { (a, b) } else { (b, a) };
            *ec.entry(e).or_insert(0) += 1;
        }
    }
    assert!(ec.values().all(|&c| c == 1 || c == 2), "non-manifold edge");
    assert_eq!(
        ec.values().filter(|&&c| c == 1).count(),
        n,
        "boundary loop is exactly the original n edges (no slits)"
    );
}
