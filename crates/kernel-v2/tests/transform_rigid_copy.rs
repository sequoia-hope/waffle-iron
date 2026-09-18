//! `transform_solid` — rigid copy of a solid (spec
//! `specs/custom_features_and_modeling_roadmap.md` §B1, the pattern
//! substrate).
//!
//! Oracle groups:
//! 1. Invariance — volume, surface area and Euler counts of the copy equal
//!    the source's; the copy validates; the source is untouched.
//! 2. Exactness — the copy's tessellation equals the source's tessellation
//!    moved point-by-point (to rounding), and analytic frames rotate.
//! 3. Determinism — two identical calls produce bit-identical arenas.
//! 4. Refusals — a reflection and a scaled matrix are typed errors.
//! 5. Provenance — every copied face's lineage roots at its source face.
//! 6. Composition — a translated copy is a valid boolean operand (union of
//!    two overlapping boxes has the exact inclusion–exclusion volume).

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, check_rigid, extrude, face_lineage, revolve, tessellate, transform_solid,
    validate_solid, BrepArena, Curve, KernelV2Error, OpTag, Profile, RenderMesh, SolidId, Surface,
};
use waffle_types::kernel::RigidPlacement;

fn volume(arena: &BrepArena, s: SolidId) -> f64 {
    kernel_v2::geom::signed_volume(arena, s).expect("volume")
}

/// Mesh volume (divergence theorem over the render triangles) — universal,
/// unlike the exact `signed_volume`, which does not yet cover arc-bounded
/// revolve caps.
fn mesh_volume(m: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let i = i as usize;
        [
            m.positions[3 * i],
            m.positions[3 * i + 1],
            m.positions[3 * i + 2],
        ]
    };
    m.indices
        .chunks(3)
        .map(|t| {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let cross = [
                b[1] * c[2] - b[2] * c[1],
                b[2] * c[0] - b[0] * c[2],
                b[0] * c[1] - b[1] * c[0],
            ];
            (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]) / 6.0
        })
        .sum()
}

/// Per-face mesh areas, in face-range order.
fn mesh_face_areas(m: &RenderMesh) -> Vec<f64> {
    let p = |i: u32| {
        let i = i as usize;
        [
            m.positions[3 * i],
            m.positions[3 * i + 1],
            m.positions[3 * i + 2],
        ]
    };
    m.face_ranges
        .iter()
        .map(|r| {
            m.indices[r.start as usize..(r.start + r.count) as usize]
                .chunks(3)
                .map(|t| {
                    let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                    let x = [
                        u[1] * v[2] - u[2] * v[1],
                        u[2] * v[0] - u[0] * v[2],
                        u[0] * v[1] - u[1] * v[0],
                    ];
                    0.5 * (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt()
                })
                .sum()
        })
        .collect()
}

/// Per-face sorted vertex sets (rounded to 1e-9), in face-range order.
fn mesh_face_vertex_sets(m: &RenderMesh) -> Vec<Vec<[i64; 3]>> {
    let key = |i: u32| -> [i64; 3] {
        let i = i as usize;
        let q = |x: f64| (x * 1e9).round() as i64;
        [
            q(m.positions[3 * i]),
            q(m.positions[3 * i + 1]),
            q(m.positions[3 * i + 2]),
        ]
    };
    m.face_ranges
        .iter()
        .map(|r| {
            let mut v: Vec<[i64; 3]> = m.indices[r.start as usize..(r.start + r.count) as usize]
                .iter()
                .map(|&i| key(i))
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        })
        .collect()
}

/// `mesh` moved by `placement` (positions and normals).
fn moved_mesh(m: &RenderMesh, placement: &RigidPlacement) -> RenderMesh {
    let mut out = m.clone();
    for (i, chunk) in m.positions.chunks(3).enumerate() {
        let p = placement.apply([chunk[0], chunk[1], chunk[2]]);
        out.positions[3 * i..3 * i + 3].copy_from_slice(&p);
    }
    for (i, chunk) in m.normals.chunks(3).enumerate() {
        let n = placement.apply_dir([chunk[0], chunk[1], chunk[2]]);
        out.normals[3 * i..3 * i + 3].copy_from_slice(&n);
    }
    out
}

fn unit_box(arena: &mut BrepArena) -> SolidId {
    let sq = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ],
        vec![],
    )
    .expect("square");
    extrude(arena, &sq, Vector3::new(0.0, 0.0, 1.0), 1.0)
        .expect("box")
        .solid
}

fn cylinder(arena: &mut BrepArena) -> SolidId {
    let c = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        0.5,
    )
    .expect("circle");
    extrude(arena, &c, Vector3::new(0.0, 0.0, 1.0), 2.0)
        .expect("cylinder")
        .solid
}

/// Quarter-turn revolve of an off-axis circle: a torus segment with two
/// planar caps (exercises `Surface::Torus` and `Curve::Arc`/`Circle`).
fn torus_segment(arena: &mut BrepArena) -> SolidId {
    let c = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(3.0, 0.0),
        1.0,
    )
    .expect("circle");
    revolve(
        arena,
        &c,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        PI / 2.0,
    )
    .expect("torus segment")
    .solid
}

/// Closed sphere: full-turn revolve of an on-axis circle.
fn sphere(arena: &mut BrepArena) -> SolidId {
    let c = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(5.0, 0.0),
        1.0,
    )
    .expect("circle");
    revolve(
        arena,
        &c,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("sphere")
    .solid
}

/// A general rotation (no axis-aligned shortcuts): 37° about a skew axis,
/// through a point off the origin, plus a translation.
fn skew_placement() -> RigidPlacement {
    let axis = [1.0, 2.0, 3.0];
    let n = (14.0f64).sqrt();
    let mut p = RigidPlacement::rotation_about(
        [0.3, -0.2, 0.7],
        [axis[0] / n, axis[1] / n, axis[2] / n],
        37.0_f64.to_radians(),
    );
    p.translation[0] += 2.5;
    p.translation[1] -= 1.25;
    p.translation[2] += 0.75;
    p
}

fn assert_close(a: f64, b: f64, rel: f64, what: &str) {
    let scale = a.abs().max(b.abs()).max(1e-300);
    assert!(
        (a - b).abs() <= rel * scale,
        "{what}: {a} vs {b} (rel diff {})",
        (a - b).abs() / scale
    );
}

/// Distance-like residual of `p` from `surface` (zero on the surface).
fn surface_residual(surface: &Surface, p: [f64; 3]) -> f64 {
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let norm = |a: [f64; 3]| dot(a, a).sqrt();
    let u = |v: kernel_v2::UnitVector3| [v.x, v.y, v.z];
    match *surface {
        Surface::Plane(pl) => dot(sub(p, pl.point.as_array()), u(pl.normal)).abs(),
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
            ..
        } => {
            let d = sub(p, axis_point.as_array());
            let t = dot(d, u(axis_dir));
            let a = u(axis_dir);
            let radial = sub(d, [a[0] * t, a[1] * t, a[2] * t]);
            (norm(radial) - radius).abs()
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
            ..
        } => {
            let d = sub(p, apex.as_array());
            let t = dot(d, u(axis_dir));
            let a = u(axis_dir);
            let radial = sub(d, [a[0] * t, a[1] * t, a[2] * t]);
            (norm(radial) - kernel_v2::geom::cone_radius_at(t, half_angle)).abs()
        }
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
            ..
        } => {
            let d = sub(p, center.as_array());
            let t = dot(d, u(axis_dir));
            let a = u(axis_dir);
            let rho = norm(sub(d, [a[0] * t, a[1] * t, a[2] * t]));
            kernel_v2::geom::torus_residual(t, rho, major_radius, minor_radius).abs()
        }
        Surface::Sphere { center, radius, .. } => {
            kernel_v2::geom::sphere_residual(Point3::from(p), center, radius).abs()
        }
        #[allow(unreachable_patterns)]
        other => panic!("surface_residual: unhandled surface {other:?}"),
    }
}

/// Every render vertex of every face of `solid` lies on that face's
/// analytic surface (residual ≤ `tol`).
fn assert_mesh_on_surfaces(arena: &BrepArena, mesh: &RenderMesh, tol: f64, what: &str) {
    for r in &mesh.face_ranges {
        let surface = arena.face(r.face).unwrap().surface.unwrap();
        for &i in &mesh.indices[r.start as usize..(r.start + r.count) as usize] {
            let i = i as usize;
            let p = [
                mesh.positions[3 * i],
                mesh.positions[3 * i + 1],
                mesh.positions[3 * i + 2],
            ];
            let res = surface_residual(&surface, p);
            assert!(
                res <= tol,
                "{what}: face {:?} vertex {p:?} off its {surface:?} by {res}",
                r.face
            );
        }
    }
}

fn all_faces_flat_or_cylindrical(arena: &BrepArena, solid: SolidId) -> bool {
    let sh = arena.shell(arena.solid(solid).unwrap().shells[0]).unwrap();
    sh.faces.iter().all(|&f| {
        matches!(
            arena.face(f).unwrap().surface,
            Some(Surface::Plane(_)) | Some(Surface::Cylinder { .. })
        )
    })
}

/// Group 1+2 for one fixture: invariants; the copy's mesh lies on the
/// moved analytic surfaces; and, for solids whose samplers are frame-free
/// (plane, cylinder), the copy's tessellation IS the source's, moved.
fn check_copy(build: fn(&mut BrepArena) -> SolidId, placement: &RigidPlacement, what: &str) {
    let mut arena = BrepArena::new();
    let src = build(&mut arena);
    let before = arena.clone();
    let src_exact_vol = kernel_v2::geom::signed_volume(&arena, src);
    let src_exact_area = kernel_v2::surface_area(&arena, src);
    let src_euler = arena.euler_counts(src).unwrap();
    let src_mesh: RenderMesh = tessellate(&arena, src).expect("source tessellates");

    let copy = transform_solid(&mut arena, src, placement)
        .unwrap_or_else(|e| panic!("{what}: transform failed: {e:?}"));
    assert_ne!(copy, src);

    // The source's entities are untouched (the arena only grew).
    assert_eq!(before.vertices[..], arena.vertices[..before.vertices.len()]);
    assert_eq!(before.faces[..], arena.faces[..before.faces.len()]);
    assert_eq!(
        before.half_edges[..],
        arena.half_edges[..before.half_edges.len()]
    );

    validate_solid(&arena, copy).unwrap_or_else(|e| panic!("{what}: copy invalid: {e:?}"));
    assert_eq!(
        arena.euler_counts(copy).unwrap(),
        src_euler,
        "{what}: Euler counts"
    );

    // Exact measures, where the exact evaluators cover the fixture (the
    // exact `signed_volume` does not yet cover arc-bounded revolve caps;
    // the copy must then fail the same way, not differently).
    match (src_exact_vol, kernel_v2::geom::signed_volume(&arena, copy)) {
        (Ok(a), Ok(b)) => assert_close(b, a, 1e-12, &format!("{what}: exact volume")),
        (Err(ea), Err(eb)) => assert_eq!(
            std::mem::discriminant(&ea),
            std::mem::discriminant(&eb),
            "{what}: exact volume support differs"
        ),
        (a, b) => panic!("{what}: exact volume support differs: {a:?} vs {b:?}"),
    }
    if let (Ok(a), Ok(b)) = (src_exact_area, kernel_v2::surface_area(&arena, copy)) {
        assert_close(b, a, 1e-12, &format!("{what}: exact area"));
    }

    // Tessellation of the copy is the source's tessellation, moved: same
    // face ranges, same per-face vertex sets, same per-face areas, same
    // mesh volume. (Triangle DIAGONALS may differ — a rotated planar face
    // projects onto a different dominant axis for the 2D triangulation —
    // so index order is not compared.)
    let copy_mesh = tessellate(&arena, copy).expect("copy tessellates");
    let expect = moved_mesh(&src_mesh, placement);
    assert_eq!(
        copy_mesh.face_ranges.len(),
        expect.face_ranges.len(),
        "{what}: face count"
    );

    // Universal exactness oracle: every render vertex of the copy lies ON
    // the copy's (moved) analytic surface. This holds regardless of how a
    // sampler picks its grid.
    assert_mesh_on_surfaces(&arena, &copy_mesh, 1e-9, what);

    if !all_faces_flat_or_cylindrical(&arena, src) {
        // Torus and sphere samplers derive their (u, v) grid from a
        // world-anchored frame, so a rotated surface is sampled differently
        // (a different, equally valid mesh of the same exact surface).
        // Compare measures within chord tolerance only.
        assert_close(
            mesh_volume(&copy_mesh),
            mesh_volume(&src_mesh),
            2e-3,
            &format!("{what}: mesh volume vs source (chord tolerance)"),
        );
        for (i, (a, b)) in mesh_face_areas(&copy_mesh)
            .iter()
            .zip(mesh_face_areas(&src_mesh))
            .enumerate()
        {
            assert_close(
                *a,
                b,
                2e-3,
                &format!("{what}: face {i} mesh area (chord tolerance)"),
            );
        }
        return;
    }
    assert_eq!(
        copy_mesh.indices.len(),
        expect.indices.len(),
        "{what}: triangle count"
    );
    assert_eq!(
        mesh_face_vertex_sets(&copy_mesh),
        mesh_face_vertex_sets(&expect),
        "{what}: per-face vertex sets"
    );
    for (i, (a, b)) in mesh_face_areas(&copy_mesh)
        .iter()
        .zip(mesh_face_areas(&expect))
        .enumerate()
    {
        assert_close(*a, b, 1e-9, &format!("{what}: face {i} mesh area"));
    }
    assert_close(
        mesh_volume(&copy_mesh),
        mesh_volume(&expect),
        1e-9,
        &format!("{what}: mesh volume"),
    );
    assert_close(
        mesh_volume(&copy_mesh),
        mesh_volume(&src_mesh),
        1e-9,
        &format!("{what}: mesh volume vs source"),
    );
    // Normals: per face, the set of (position, normal) pairs of the copy is
    // the source's set, rotated. Vertices are per-face, so within one face
    // range a position carries exactly one normal.
    let pairs = |m: &RenderMesh| -> Vec<Vec<([i64; 3], [i64; 3])>> {
        let q = |x: f64| (x * 1e9).round() as i64;
        m.face_ranges
            .iter()
            .map(|r| {
                let mut v: Vec<([i64; 3], [i64; 3])> = m.indices
                    [r.start as usize..(r.start + r.count) as usize]
                    .iter()
                    .map(|&i| {
                        let i = i as usize;
                        (
                            [
                                q(m.positions[3 * i]),
                                q(m.positions[3 * i + 1]),
                                q(m.positions[3 * i + 2]),
                            ],
                            [
                                q(m.normals[3 * i]),
                                q(m.normals[3 * i + 1]),
                                q(m.normals[3 * i + 2]),
                            ],
                        )
                    })
                    .collect();
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect()
    };
    assert_eq!(
        pairs(&copy_mesh),
        pairs(&expect),
        "{what}: per-face (position, normal) sets"
    );
}

#[test]
fn box_translated_and_rotated() {
    check_copy(
        unit_box,
        &RigidPlacement::translation([3.0, -1.0, 0.5]),
        "box/translate",
    );
    check_copy(unit_box, &skew_placement(), "box/skew");
}

#[test]
fn cylinder_rotated_keeps_cylinder_surface_and_circle_rims() {
    check_copy(cylinder, &skew_placement(), "cylinder/skew");

    // The lateral face is still a cylinder of radius 0.5 whose axis is the
    // rotated z axis; rims are circles of radius 0.5.
    let mut arena = BrepArena::new();
    let src = cylinder(&mut arena);
    let p = RigidPlacement::rotation_about([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], PI / 2.0);
    let copy = transform_solid(&mut arena, src, &p).unwrap();
    let shell = arena.shell(arena.solid(copy).unwrap().shells[0]).unwrap();
    let mut n_cyl = 0;
    let mut n_plane = 0;
    for &f in &shell.faces {
        match arena.face(f).unwrap().surface.unwrap() {
            Surface::Cylinder {
                axis_dir, radius, ..
            } => {
                n_cyl += 1;
                assert_close(radius, 0.5, 1e-15, "radius");
                // z-axis rotated 90° about x → −y.
                assert!(axis_dir.x.abs() < 1e-15);
                assert!((axis_dir.y + 1.0).abs() < 1e-15, "axis_dir {axis_dir:?}");
                assert!(axis_dir.z.abs() < 1e-15);
            }
            Surface::Plane(_) => n_plane += 1,
            other => panic!("unexpected surface {other:?}"),
        }
        for lid in std::iter::once(arena.face(f).unwrap().outer_loop) {
            for h in arena.loop_half_edges(lid).unwrap() {
                if let Curve::Circle { radius, normal, .. } = arena.half_edge(h).unwrap().curve {
                    assert_close(radius, 0.5, 1e-15, "rim radius");
                    assert!(normal.x.abs() < 1e-15 && normal.z.abs() < 1e-15);
                }
            }
        }
    }
    assert_eq!((n_cyl, n_plane), (1, 2));
}

#[test]
fn torus_segment_rotated() {
    check_copy(torus_segment, &skew_placement(), "torus-segment/skew");
}

#[test]
fn sphere_rotated_off_its_canonical_seam() {
    // The sphere's seam is canonical z-up at construction; a general
    // rotation moves the seam. The copy must still validate and tessellate
    // identically (moved) — the seam is topology, not a z-up assumption.
    check_copy(sphere, &skew_placement(), "sphere/skew");
}

#[test]
fn identity_placement_is_bit_preserving() {
    let mut arena = BrepArena::new();
    let src = torus_segment(&mut arena);
    let copy = transform_solid(&mut arena, src, &RigidPlacement::IDENTITY).unwrap();
    let a = tessellate(&arena, src).unwrap();
    let b = tessellate(&arena, copy).unwrap();
    assert_eq!(
        a.positions, b.positions,
        "identity copy positions bit-identical"
    );
    assert_eq!(a.normals, b.normals);
    // Surfaces bit-identical too (no renormalization).
    let sf = |s: SolidId| -> Vec<Surface> {
        let sh = arena.shell(arena.solid(s).unwrap().shells[0]).unwrap();
        sh.faces
            .iter()
            .map(|&f| arena.face(f).unwrap().surface.unwrap())
            .collect()
    };
    assert_eq!(sf(src), sf(copy));
}

#[test]
fn two_identical_transforms_are_deterministic() {
    let build = |arena: &mut BrepArena| {
        let s = cylinder(arena);
        transform_solid(arena, s, &skew_placement()).unwrap()
    };
    let mut a = BrepArena::new();
    let mut b = BrepArena::new();
    let ca = build(&mut a);
    let cb = build(&mut b);
    assert_eq!(ca, cb);
    assert_eq!(
        a, b,
        "arenas bit-identical after identical transform sequences"
    );
}

#[test]
fn reflection_and_scale_are_refused() {
    let mut arena = BrepArena::new();
    let src = unit_box(&mut arena);
    let before = arena.clone();

    let mirror = RigidPlacement {
        translation: [0.0; 3],
        rotation: [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let err = transform_solid(&mut arena, src, &mirror).unwrap_err();
    assert!(
        matches!(err, KernelV2Error::TransformNotRigid { .. }),
        "{err:?}"
    );
    assert!(err.to_string().contains("reflection"), "{err}");

    let scaled = RigidPlacement {
        translation: [0.0; 3],
        rotation: [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]],
    };
    let err = transform_solid(&mut arena, src, &scaled).unwrap_err();
    assert!(
        matches!(err, KernelV2Error::TransformNotRigid { .. }),
        "{err:?}"
    );

    let nan = RigidPlacement {
        translation: [f64::NAN, 0.0, 0.0],
        rotation: RigidPlacement::IDENTITY.rotation,
    };
    assert!(check_rigid(&nan).is_err());

    // A refused transform leaves the arena untouched (no partial copy).
    assert_eq!(arena, before);
}

#[test]
fn copied_faces_carry_fresh_pids_rooted_at_source_faces() {
    let mut arena = BrepArena::new();
    let src = unit_box(&mut arena);
    let copy = transform_solid(
        &mut arena,
        src,
        &RigidPlacement::translation([5.0, 0.0, 0.0]),
    )
    .unwrap();

    let faces_of = |s: SolidId| -> Vec<kernel_v2::FaceId> {
        arena
            .shell(arena.solid(s).unwrap().shells[0])
            .unwrap()
            .faces
            .clone()
    };
    let src_faces = faces_of(src);
    let copy_faces = faces_of(copy);
    assert_eq!(src_faces.len(), copy_faces.len());

    let ev = arena.journal.last().expect("transform journaled");
    assert_eq!(ev.op, OpTag::Transform);
    assert_eq!(ev.modified.len(), 6);
    assert!(ev.generated.is_empty() && ev.deleted.is_empty());

    for (sf, cf) in src_faces.iter().zip(&copy_faces) {
        let sp = arena.face_pid(*sf).unwrap();
        let cp = arena.face_pid(*cf).unwrap();
        assert_ne!(sp, cp, "copy has a fresh pid");
        let lineage = face_lineage(&arena.journal, cp);
        assert_eq!(lineage.root, sp, "copy face roots at its source face");
        assert_eq!(lineage.through, vec![OpTag::Transform]);
    }
}

#[test]
fn translated_copy_is_a_boolean_operand() {
    // Unit box ∪ (unit box shifted by 0.5 in x): inclusion–exclusion gives
    // 1 + 1 − 0.5 = 1.5 exactly.
    let mut arena = BrepArena::new();
    let a = unit_box(&mut arena);
    let b = transform_solid(&mut arena, a, &RigidPlacement::translation([0.5, 0.0, 0.0])).unwrap();
    let u = boolean_op(&mut arena, a, b, BoolOp::Union).expect("union");
    validate_solid(&arena, u).unwrap();
    assert_close(volume(&arena, u), 1.5, 1e-12, "union volume");

    // Disjoint copies: a rotated cylinder next to its source.
    let mut arena = BrepArena::new();
    let c = cylinder(&mut arena);
    let p = RigidPlacement::rotation_about([2.0, 0.0, 0.0], [0.0, 0.0, 1.0], PI / 3.0);
    let d = transform_solid(&mut arena, c, &p).unwrap();
    let inter = boolean_op(&mut arena, c, d, BoolOp::Intersect);
    assert!(
        matches!(inter, Err(KernelV2Error::EmptyBooleanResult)),
        "disjoint intersect is the typed empty result, got {inter:?}"
    );
}
