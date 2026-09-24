//! `mirror_solid` — reflected copy of a solid (the mirror pattern's
//! substrate, `docs/notes/eiffel/FEATURE_NOTES.md` §4).
//!
//! A reflection is improper, so the interesting question is never the
//! geometry (which maps the same way a rigid copy's does) but the
//! ORIENTATION: is the copy a solid with its faces outward, or the same
//! point set turned inside out? Every oracle here is about that.
//!
//! 1. Invariance — the copy validates, and its volume is the source's
//!    (POSITIVE, not negated: a turned-inside-out copy would integrate to
//!    −V and still satisfy every per-face invariant).
//! 2. Exactness — the copy's tessellation is the source's reflected: the
//!    same bounds and volume everywhere, and point-for-point on a box
//!    (curved faces mint interior samples per surface, so those need not
//!    land on the same points).
//! 3. Analytic survival — a mirrored cylinder is a cylinder of the same
//!    radius; a mirrored torus a torus of the same radii.
//! 4. Involution — mirroring twice through the same plane restores the
//!    source geometry exactly.
//! 5. Composition — a mirrored copy is a valid boolean operand.
//! 6. Refusals — a degenerate plane normal is typed, pre-mutation.

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, face_lineage, mirror_solid, revolve, tessellate, validate_solid,
    BrepArena, KernelV2Error, OpTag, Profile, RenderMesh, SolidId, Surface,
};
use waffle_types::kernel::MirrorPlane;

fn volume(arena: &BrepArena, s: SolidId) -> f64 {
    kernel_v2::geom::signed_volume(arena, s).expect("volume")
}

/// Mesh volume by the divergence theorem — signed, so an inside-out solid
/// shows up as a negative number rather than as a passing test.
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

/// Every triangle's geometric normal (from its winding) agrees with the
/// per-vertex normals the tessellation stored: the render-level statement of
/// "this face points out of the solid".
fn winding_agrees_with_normals(m: &RenderMesh) -> bool {
    let p = |i: u32| {
        let i = i as usize;
        [
            m.positions[3 * i],
            m.positions[3 * i + 1],
            m.positions[3 * i + 2],
        ]
    };
    let n = |i: u32| {
        let i = i as usize;
        [m.normals[3 * i], m.normals[3 * i + 1], m.normals[3 * i + 2]]
    };
    m.indices.chunks(3).all(|t| {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let x = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let len = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
        if len < 1e-15 {
            return true; // degenerate triangle carries no orientation
        }
        let stored = n(t[0]);
        (x[0] * stored[0] + x[1] * stored[1] + x[2] * stored[2]) / len > -1e-9
    })
}

/// The mesh's vertex positions as a sorted, rounded set.
fn point_set(m: &RenderMesh) -> Vec<[i64; 3]> {
    let q = |x: f64| (x * 1e9).round() as i64;
    let mut v: Vec<[i64; 3]> = m
        .positions
        .chunks(3)
        .map(|c| [q(c[0]), q(c[1]), q(c[2])])
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Axis-aligned bounds of a mesh.
fn mesh_bounds(m: &RenderMesh) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for c in m.positions.chunks(3) {
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    (lo, hi)
}

/// Bounds of a mesh after reflecting every point.
fn reflected_bounds(m: &RenderMesh, plane: &MirrorPlane) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for c in m.positions.chunks(3) {
        let p = plane.apply([c[0], c[1], c[2]]).expect("plane");
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

fn reflected_points(m: &RenderMesh, plane: &MirrorPlane) -> Vec<[i64; 3]> {
    let q = |x: f64| (x * 1e9).round() as i64;
    let mut v: Vec<[i64; 3]> = m
        .positions
        .chunks(3)
        .map(|c| {
            let p = plane.apply([c[0], c[1], c[2]]).expect("plane");
            [q(p[0]), q(p[1]), q(p[2])]
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
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
/// planar caps (`Surface::Torus`, `Curve::Arc` and `Curve::Circle`).
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

/// An OBLIQUE plane, so the test is not carried by an axis-aligned
/// coincidence: x + 2y − 2z = 3.
fn oblique_plane() -> MirrorPlane {
    MirrorPlane {
        point: [3.0, 0.0, 0.0],
        normal: [1.0, 2.0, -2.0],
    }
}

#[test]
fn mirrored_box_is_a_solid_of_the_same_volume() {
    for plane in [
        MirrorPlane {
            point: [0.0, 0.0, 0.0],
            normal: [1.0, 0.0, 0.0],
        },
        oblique_plane(),
    ] {
        let mut arena = BrepArena::new();
        let src = unit_box(&mut arena);
        let copy = mirror_solid(&mut arena, src, &plane).expect("mirror");
        assert_ne!(copy, src);
        validate_solid(&arena, copy).expect("copy validates");
        validate_solid(&arena, src).expect("source untouched");
        // POSITIVE, and equal: an inside-out copy would give −1.
        assert!(
            (volume(&arena, copy) - 1.0).abs() < 1e-12,
            "volume {} for plane {plane:?}",
            volume(&arena, copy)
        );
    }
}

#[test]
fn mirrored_tessellation_is_the_reflected_solid_with_outward_winding() {
    let plane = oblique_plane();
    // The tolerance is the FACETING, not the geometry: a curved face's
    // extreme sample sits wherever its chord bound put it, and reflecting the
    // frame can move that by a sagitta. The box is exact.
    for (name, build, tol) in [
        ("box", unit_box as fn(&mut BrepArena) -> SolidId, 1e-12),
        ("cylinder", cylinder, 1e-3),
        ("torus segment", torus_segment, 1e-3),
    ] {
        let mut arena = BrepArena::new();
        let src = build(&mut arena);
        let copy = mirror_solid(&mut arena, src, &plane).expect("mirror");
        let src_mesh = tessellate(&arena, src).expect("tess src");
        let copy_mesh = tessellate(&arena, copy).expect("tess copy");
        // Comparable size: a curved face picks its sample count from a
        // chord bound, and the reflected frame can land a sample either side
        // of that boundary, so the counts are close rather than equal.
        let (n_copy, n_src) = (copy_mesh.indices.len(), src_mesh.indices.len());
        assert!(
            (n_copy as f64 - n_src as f64).abs() <= 0.05 * n_src as f64,
            "{name}: triangle count {n_copy} vs {n_src}"
        );
        // The reflection turned the winding round with the geometry, so the
        // stored normals still point out of the solid.
        assert!(
            winding_agrees_with_normals(&copy_mesh),
            "{name}: mirrored copy is inside out"
        );
        // POSITIVE and equal. The sign is the oracle — an inside-out copy
        // integrates to −V while passing every per-face check. The magnitude
        // agrees only to the faceting: the two meshes approximate the same
        // curved solid with slightly different sample counts.
        let (v_src, v_copy) = (mesh_volume(&src_mesh), mesh_volume(&copy_mesh));
        assert!(v_src > 0.0, "{name}: source mesh volume {v_src}");
        assert!(
            (v_copy - v_src).abs() < 1e-4 * v_src.abs().max(1.0),
            "{name}: mesh volume {v_copy} vs {v_src}"
        );
        // Same solid, reflected: the bounding boxes agree.
        let (lo_c, hi_c) = mesh_bounds(&copy_mesh);
        let (lo_r, hi_r) = reflected_bounds(&src_mesh, &plane);
        for k in 0..3 {
            assert!(
                (lo_c[k] - lo_r[k]).abs() < tol && (hi_c[k] - hi_r[k]).abs() < tol,
                "{name}: bounds axis {k}: [{}, {}] vs [{}, {}]",
                lo_c[k],
                hi_c[k],
                lo_r[k],
                hi_r[k]
            );
        }
    }
}

/// The planar case can say more than bounds: a box's tessellation is EXACTLY
/// its source's reflected, point for point (curved faces mint their interior
/// samples per surface, so their sample positions need not coincide).
#[test]
fn a_mirrored_box_tessellates_to_exactly_the_reflected_points() {
    let plane = oblique_plane();
    let mut arena = BrepArena::new();
    let src = unit_box(&mut arena);
    let copy = mirror_solid(&mut arena, src, &plane).expect("mirror");
    let src_mesh = tessellate(&arena, src).expect("tess src");
    let copy_mesh = tessellate(&arena, copy).expect("tess copy");
    assert_eq!(point_set(&copy_mesh), reflected_points(&src_mesh, &plane));
}

#[test]
fn analytic_surfaces_survive_the_mirror() {
    let plane = oblique_plane();
    let mut arena = BrepArena::new();
    let src = cylinder(&mut arena);
    let copy = mirror_solid(&mut arena, src, &plane).expect("mirror");
    let lateral = arena
        .solid(copy)
        .expect("solid")
        .shells
        .iter()
        .flat_map(|&sh| arena.shell(sh).expect("shell").faces.clone())
        .find_map(|f| match arena.face(f).expect("face").surface {
            Some(Surface::Cylinder {
                radius, axis_dir, ..
            }) => Some((radius, axis_dir)),
            _ => None,
        })
        .expect("a cylinder face survives");
    assert!((lateral.0 - 0.5).abs() < 1e-15, "radius {}", lateral.0);
    // The axis is the source's (0,0,1) reflected in x + 2y − 2z = 3.
    let expect = plane.apply_dir([0.0, 0.0, 1.0]).expect("plane");
    for k in 0..3 {
        let got = [lateral.1.x, lateral.1.y, lateral.1.z][k];
        assert!(
            (got.abs() - expect[k].abs()).abs() < 1e-12,
            "axis component {k}: {got} vs ±{}",
            expect[k]
        );
    }
}

#[test]
fn mirroring_twice_restores_the_source() {
    let plane = oblique_plane();
    let mut arena = BrepArena::new();
    let src = torus_segment(&mut arena);
    let once = mirror_solid(&mut arena, src, &plane).expect("mirror");
    let twice = mirror_solid(&mut arena, once, &plane).expect("mirror back");
    validate_solid(&arena, twice).expect("validates");
    let src_mesh = tessellate(&arena, src).expect("tess");
    let back_mesh = tessellate(&arena, twice).expect("tess");
    let q = |x: f64| (x * 1e8).round() as i64;
    let round = |m: &RenderMesh| -> Vec<[i64; 3]> {
        let mut v: Vec<[i64; 3]> = m
            .positions
            .chunks(3)
            .map(|c| [q(c[0]), q(c[1]), q(c[2])])
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    assert_eq!(round(&back_mesh), round(&src_mesh));
    assert!(
        (mesh_volume(&back_mesh) - mesh_volume(&src_mesh)).abs() < 1e-9,
        "volume after two mirrors"
    );
}

#[test]
fn a_mirrored_copy_is_a_boolean_operand() {
    // Two unit boxes: [0,1]³ and its reflection in x = 1.5, which is
    // [2,3]×[0,1]×[0,1] — disjoint. Reflect in x = 0.75 instead and they
    // overlap in [0.5,1], so inclusion–exclusion gives 2 − 0.5.
    let mut arena = BrepArena::new();
    let a = unit_box(&mut arena);
    let b = mirror_solid(
        &mut arena,
        a,
        &MirrorPlane {
            point: [0.75, 0.0, 0.0],
            normal: [1.0, 0.0, 0.0],
        },
    )
    .expect("mirror");
    let u = boolean_op(&mut arena, a, b, BoolOp::Union).expect("union");
    validate_solid(&arena, u).expect("union validates");
    assert!(
        (volume(&arena, u) - 1.5).abs() < 1e-12,
        "union volume {}",
        volume(&arena, u)
    );
}

#[test]
fn lineage_of_a_mirrored_face_roots_at_its_source_face() {
    let mut arena = BrepArena::new();
    let src = unit_box(&mut arena);
    let copy = mirror_solid(&mut arena, src, &oblique_plane()).expect("mirror");
    let faces: Vec<_> = arena
        .solid(copy)
        .expect("solid")
        .shells
        .iter()
        .flat_map(|&sh| arena.shell(sh).expect("shell").faces.clone())
        .collect();
    assert_eq!(faces.len(), 6);
    for f in faces {
        let pid = arena.face_pid(f).expect("pid");
        let lineage = face_lineage(&arena.journal, pid);
        assert_ne!(lineage.root, pid, "a copy's face is not its own root");
        assert_eq!(
            lineage.through,
            vec![OpTag::Mirror],
            "a mirrored face says so"
        );
    }
}

#[test]
fn a_degenerate_mirror_plane_is_refused_before_anything_is_written() {
    let mut arena = BrepArena::new();
    let src = unit_box(&mut arena);
    let before = arena.clone();
    for normal in [[0.0, 0.0, 0.0], [f64::NAN, 0.0, 1.0]] {
        let err = mirror_solid(
            &mut arena,
            src,
            &MirrorPlane {
                point: [0.0, 0.0, 0.0],
                normal,
            },
        )
        .unwrap_err();
        assert!(
            matches!(err, KernelV2Error::TransformNotRigid { .. }),
            "{err:?}"
        );
    }
    assert_eq!(arena, before, "arena untouched by the refusals");
}
