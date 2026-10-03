//! D1b silhouette oracles (`specs/drawings_and_mbd.md` §5.2 increment 2,
//! §5.3).
//!
//! Three layers, in increasing strength:
//!
//! 1. **Per-primitive numeric pins** — a cylinder along `x` seen along `z`
//!    has its rulings at `y = ±R`; a partial cylinder whose angular window
//!    excludes both generators has NONE and one that contains a single
//!    generator has exactly one; a frustum's two rulings extend through its
//!    apex; a sphere's silhouette is a circle of exactly its radius; a
//!    torus's two equator circles and its four edge-on coordinate circles
//!    come out exact, and the oblique branches' bbox is the torus's own
//!    analytic support box.
//! 2. **The brute-force agreement** — the D1a review's 216-configuration
//!    shape, moved from arcs to silhouettes: four closed curved surfaces ×
//!    54 view directions, with the exact zero set of `n·w` located by
//!    bisection on a `(u, v)` grid of the surface and compared BOTH WAYS
//!    against the reported paths. Every grid zero must lie on a reported
//!    path, and every point of a reported path must have `n·w = 0` and lie
//!    in the face's own parameter domain.
//! 3. **The §5.3 oracle** — the projected bbox now EQUALS the solid AABB's
//!    projection for the curved fixtures whose AABB is tight, which is what
//!    D1b buys and D1a could not assert.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use super::*;
use crate::arena::{Plane, UnitVector3};
use crate::cone_fixtures::build_frustum;
use crate::{revolve, BrepArena, FaceId, Profile, ProfileEdge, SolidId, Surface};
use cad_primitives::{Point2 as P2, Vector3};
use waffle_types::kernel::projection::{Aabb2, ViewFrame};

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

fn uv(x: f64, y: f64, z: f64) -> Vector3 {
    Vector3::new(x, y, z)
}

/// A full cylinder of `radius` and `height`, axis along `axis`, built by
/// extruding a circle profile. Returns `(arena, solid, lateral face)`.
fn full_cylinder(
    radius: f64,
    height: f64,
    origin: Point3,
    axis: [f64; 3],
    x_axis: [f64; 3],
) -> (BrepArena, SolidId, FaceId) {
    let y_axis = cross(axis, x_axis);
    let profile = Profile::circle(
        origin,
        uv(x_axis[0], x_axis[1], x_axis[2]),
        uv(y_axis[0], y_axis[1], y_axis[2]),
        P2::new(0.0, 0.0),
        radius,
    )
    .expect("circle profile");
    let mut arena = BrepArena::new();
    let result = crate::extrude(&mut arena, &profile, uv(axis[0], axis[1], axis[2]), height)
        .expect("cylinder extrudes");
    let fid = the_curved_face(&arena, result.solid);
    (arena, result.solid, fid)
}

/// A plate whose wall is a PARTIAL cylinder: the profile is an arc-polygon
/// covering `[lo, hi]` radians of a circle of `radius`, closed by its chord,
/// extruded along `+z`. Returns `(arena, solid, the cylinder face)`.
fn partial_cylinder(radius: f64, lo: f64, hi: f64, height: f64) -> (BrepArena, SolidId, FaceId) {
    let at = |t: f64| P2::new(radius * t.cos(), radius * t.sin());
    let outer = vec![
        ProfileEdge::Arc {
            a: at(lo),
            b: at(hi),
            center: P2::new(0.0, 0.0),
            radius,
            ccw: true,
        },
        ProfileEdge::Line {
            a: at(hi),
            b: at(lo),
        },
    ];
    let profile = Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        uv(1.0, 0.0, 0.0),
        uv(0.0, 1.0, 0.0),
        outer,
        Vec::new(),
    )
    .expect("arc polygon profile");
    let mut arena = BrepArena::new();
    let result = crate::extrude(&mut arena, &profile, uv(0.0, 0.0, 1.0), height)
        .expect("partial cylinder extrudes");
    let fid = the_curved_face(&arena, result.solid);
    (arena, result.solid, fid)
}

/// The closed sphere of `radius` at the origin (full-turn revolve of an
/// on-axis circle). Returns `(arena, solid, the sphere face)`.
fn closed_sphere(radius: f64) -> (BrepArena, SolidId, FaceId) {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        uv(1.0, 0.0, 0.0),
        uv(0.0, 1.0, 0.0),
        P2::new(0.0, 0.0),
        radius,
    )
    .expect("on-axis circle profile");
    let mut arena = BrepArena::new();
    let result = revolve(
        &mut arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        uv(1.0, 0.0, 0.0),
        TAU,
    )
    .expect("sphere revolves");
    let fid = the_curved_face(&arena, result.solid);
    (arena, result.solid, fid)
}

/// The closed ring torus `(major, minor)` about `+z` at the origin.
fn closed_torus(major: f64, minor: f64) -> (BrepArena, SolidId, FaceId) {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        uv(1.0, 0.0, 0.0),
        uv(0.0, 0.0, 1.0),
        P2::new(major, 0.0),
        minor,
    )
    .expect("off-axis circle profile");
    let mut arena = BrepArena::new();
    let result = revolve(
        &mut arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        uv(0.0, 0.0, 1.0),
        TAU,
    )
    .expect("torus revolves");
    let fid = the_curved_face(&arena, result.solid);
    (arena, result.solid, fid)
}

/// The one face of a solid whose surface is `Torus`.
fn the_torus_face(arena: &BrepArena, solid: SolidId) -> FaceId {
    let mut found = None;
    for &sh in &arena.solid(solid).expect("solid").shells {
        for &f in &arena.shell(sh).expect("shell").faces {
            if matches!(
                arena.face(f).expect("face").surface,
                Some(Surface::Torus { .. })
            ) {
                assert!(found.is_none(), "expected exactly one toroidal face");
                found = Some(f);
            }
        }
    }
    found.expect("a toroidal face")
}

/// The one non-planar face of a solid.
fn the_curved_face(arena: &BrepArena, solid: SolidId) -> FaceId {
    let mut found = None;
    for &sh in &arena.solid(solid).expect("solid").shells {
        for &f in &arena.shell(sh).expect("shell").faces {
            if !matches!(
                arena.face(f).expect("face").surface,
                Some(Surface::Plane(_)) | None
            ) {
                assert!(found.is_none(), "expected exactly one curved face");
                found = Some(f);
            }
        }
    }
    found.expect("a curved face")
}

fn basis_along(dir: [f64; 3]) -> ViewBasis {
    ViewFrame::looking_along(dir)
        .basis()
        .expect("a non-degenerate direction has a basis")
}

/// The render density — the same `n_seg` a view gets by default.
fn n_seg() -> u32 {
    crate::tessellate::circle_segment_count(crate::tessellate::RENDER_CHORD_TOLERANCE_REL)
}

fn sil(arena: &BrepArena, fid: FaceId, dir: [f64; 3]) -> Vec<Curve2> {
    face_silhouettes(arena, fid, &basis_along(dir), n_seg()).expect("silhouettes")
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

// ---------------------------------------------------------------------------
// 1. per-primitive numeric pins
// ---------------------------------------------------------------------------

/// Spec §5.2: "Cylinder: two lines". The view `looking_along(+z)` has
/// `u = −x̂`, `v = +ŷ`, so an `x`-axis cylinder's two rulings are the lines
/// `v = ±R` running the cylinder's length.
#[test]
fn a_cylinder_along_x_seen_along_z_has_its_two_rulings_at_v_equals_plus_minus_r() {
    let (r, h) = (0.008, 0.030);
    let (arena, _, fid) = full_cylinder(
        r,
        h,
        Point3::new(0.0, 0.0, 0.0),
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
    );
    let curves = sil(&arena, fid, [0.0, 0.0, 1.0]);
    assert_eq!(curves.len(), 2, "two rulings, got {curves:?}");
    let mut vs: Vec<f64> = Vec::new();
    for c in &curves {
        let Curve2::Line { start, end } = c else {
            panic!("a cylinder's ruling projects to a line, got {c:?}");
        };
        assert!(
            close(start.y(), end.y(), 1e-15),
            "the ruling runs along the axis: {start:?} → {end:?}"
        );
        // u = −x̂, so the cylinder from x = 0 to x = h spans u ∈ [−h, 0].
        let (lo, hi) = (start.x().min(end.x()), start.x().max(end.x()));
        assert!(
            close(lo, -h, 1e-15) && close(hi, 0.0, 1e-15),
            "u span {lo}..{hi}"
        );
        vs.push(start.y());
    }
    vs.sort_by(f64::total_cmp);
    assert!(close(vs[0], -r, 1e-18), "lower ruling at v = {}", vs[0]);
    assert!(close(vs[1], r, 1e-18), "upper ruling at v = {}", vs[1]);
}

/// Spec §5.2's named degeneracy: "a cylinder seen along its axis has NO
/// silhouette lines, its rims are the outline".
#[test]
fn a_cylinder_seen_along_its_axis_has_no_silhouette() {
    let (arena, _, fid) = full_cylinder(
        0.008,
        0.030,
        Point3::new(0.0, 0.0, 0.0),
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    );
    assert!(sil(&arena, fid, [0.0, 0.0, 1.0]).is_empty());
    assert!(sil(&arena, fid, [0.0, 0.0, -1.0]).is_empty());
}

/// The clip, exactly: a `z`-axis cylinder's rulings for a view along `+y` sit
/// at `θ = 0` and `θ = π`, and for a view along `+x` at `θ = ±π/2`. A wall
/// covering `θ ∈ [20°, 160°]` therefore has NONE in the first view and
/// exactly ONE in the second — "a silhouette line on a partial cylinder may
/// be absent or a sub-segment" (spec §5.2).
#[test]
fn a_partial_cylinders_rulings_are_clipped_to_its_angular_window() {
    let (r, h) = (0.012, 0.004);
    let (lo, hi) = (20f64.to_radians(), 160f64.to_radians());
    let (arena, _, fid) = partial_cylinder(r, lo, hi, h);

    assert!(
        sil(&arena, fid, [0.0, 1.0, 0.0]).is_empty(),
        "θ = 0 and θ = π are both outside [20°, 160°]"
    );
    assert!(
        sil(&arena, fid, [0.0, -1.0, 0.0]).is_empty(),
        "the opposite view sees the same two generators"
    );

    for dir in [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]] {
        let curves = sil(&arena, fid, dir);
        assert_eq!(
            curves.len(),
            1,
            "only the θ = π/2 generator is on the wall, got {curves:?}"
        );
        let Curve2::Line { start, end } = &curves[0] else {
            panic!("a ruling projects to a line, got {:?}", curves[0]);
        };
        // The ruling is at (x, y) = (0, r) and spans the full height.
        assert!(
            close((end.y() - start.y()).abs(), h, 1e-15),
            "the ruling spans the wall's height: {start:?} → {end:?}"
        );
    }
}

/// Spec §5.2: "Cone: two lines through the apex". A frustum's face does NOT
/// contain the apex, so each ruling is clipped to the band between its rims —
/// but both still EXTEND through the projected apex, which is the property
/// that says the locus is right.
#[test]
fn a_frustums_two_rulings_extend_through_the_projected_apex() {
    let apex = Point3::new(0.0, 0.0, 0.0);
    let axis = UnitVector3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };
    let (tau0, tau1, half) = (0.010, 0.030, 0.4);
    let (arena, _, fid) = build_frustum(apex, axis, tau0, tau1, half, half);
    let basis = basis_along([0.0, 1.0, 0.0]);
    let curves = face_silhouettes(&arena, fid, &basis, n_seg()).expect("silhouettes");
    assert_eq!(curves.len(), 2, "two rulings, got {curves:?}");
    let (apex2, _) = basis.project(apex.as_array());
    let tan = half.tan();
    for c in &curves {
        let Curve2::Line { start, end } = c else {
            panic!("a cone's ruling projects to a line, got {c:?}");
        };
        // Collinear with the apex: the triangle (apex, start, end) is flat.
        let area2 = (start.x() - apex2.x()) * (end.y() - apex2.y())
            - (start.y() - apex2.y()) * (end.x() - apex2.x());
        assert!(
            area2.abs() < 1e-18,
            "the ruling {start:?} → {end:?} misses the apex {apex2:?} (2A = {area2})"
        );
        // Its two ends sit at slant distances τ/cos α from the apex.
        let d = |p: P2| (p.x() - apex2.x()).hypot(p.y() - apex2.y());
        let (lo, hi) = (d(*start).min(d(*end)), d(*start).max(d(*end)));
        let slant = |tau: f64| tau * (1.0 + tan * tan).sqrt();
        assert!(
            close(lo, slant(tau0), 1e-12) && close(hi, slant(tau1), 1e-12),
            "ruling spans {lo}..{hi}, rims are at {}..{}",
            slant(tau0),
            slant(tau1)
        );
    }
}

/// The cone's own degeneracies: seen along the axis there is no silhouette,
/// and neither is there when the viewer sits inside the cone's own shadow
/// (`|tan α·w∥/m| > 1`), which for a `tan α = tan 0.4` cone starts at
/// `|w∥| > sin(π/2 − α)`.
#[test]
fn a_cone_seen_along_its_axis_or_from_inside_its_shadow_has_no_silhouette() {
    let axis = UnitVector3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };
    let half = 0.4;
    let (arena, _, fid) = build_frustum(Point3::new(0.0, 0.0, 0.0), axis, 0.010, 0.030, half, half);
    assert!(
        sil(&arena, fid, [0.0, 0.0, 1.0]).is_empty(),
        "along the axis"
    );
    assert!(sil(&arena, fid, [0.0, 0.0, -1.0]).is_empty());
    // `|c| = tan α·w∥/m = 1` at `w∥ = cos α`; just inside it there is none,
    // just outside there are two.
    let (sin_a, cos_a) = half.sin_cos();
    for (eps, want) in [(1e-3, 0usize), (-1e-3, 2)] {
        let wp = cos_a + eps;
        let dir = [(1.0 - wp * wp).max(0.0).sqrt(), 0.0, wp];
        assert_eq!(
            sil(&arena, fid, dir).len(),
            want,
            "w∥ = {wp} against the shadow boundary cos α = {cos_a} (sin α = {sin_a})"
        );
    }
}

/// Spec §5.2: "Sphere: a circle". The great circle lies in the plane through
/// the centre perpendicular to the line of sight — which IS the view plane —
/// so it projects to an exact circle of the sphere's own radius, in every
/// direction.
#[test]
fn a_spheres_silhouette_is_a_full_circle_of_exactly_its_radius() {
    let r = 0.011;
    let (arena, _, fid) = closed_sphere(r);
    for dir in [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        [0.3, -0.5, 0.8],
        [1.0, 1.0, 1.0],
    ] {
        let curves = sil(&arena, fid, dir);
        assert_eq!(
            curves.len(),
            1,
            "one great circle along {dir:?}: {curves:?}"
        );
        let Curve2::Circle {
            center,
            radius,
            start_angle,
            end_angle,
        } = curves[0]
        else {
            panic!(
                "the sphere's silhouette must stay a circle, got {:?}",
                curves[0]
            );
        };
        assert!(close(radius, r, 1e-17), "radius {radius} vs {r}");
        assert!(
            center.x().hypot(center.y()) < 1e-17,
            "centred on the projected centre, got {center:?}"
        );
        assert!(
            close(end_angle - start_angle, TAU, 1e-12),
            "a full turn, got {}",
            end_angle - start_angle
        );
    }
}

/// Spec §5.2: "Torus: two closed curves". Seen ALONG the axis they are the
/// two equator circles `ρ = R ± r`, exactly — and their bbox is then exactly
/// the torus AABB's projection, the §5.3 equality this increment buys.
#[test]
fn a_torus_seen_along_its_axis_has_its_two_equator_circles_exactly() {
    let (major, minor) = (0.020, 0.006);
    let (arena, solid, fid) = closed_torus(major, minor);
    for dir in [[0.0, 0.0, 1.0], [0.0, 0.0, -1.0]] {
        let curves = sil(&arena, fid, dir);
        assert_eq!(curves.len(), 2, "two equators along {dir:?}: {curves:?}");
        let mut radii: Vec<f64> = Vec::new();
        for c in &curves {
            let Curve2::Circle { radius, center, .. } = c else {
                panic!("an equator projects to a circle, got {c:?}");
            };
            assert!(center.x().hypot(center.y()) < 1e-17, "centre {center:?}");
            radii.push(*radius);
        }
        radii.sort_by(f64::total_cmp);
        assert!(close(radii[0], major - minor, 1e-17), "inner {}", radii[0]);
        assert!(close(radii[1], major + minor, 1e-17), "outer {}", radii[1]);
    }
    // And the §5.3 equality, which only silhouettes can reach.
    let (lo, hi) = crate::introspect::conservative_aabb(&arena, solid)
        .expect("aabb")
        .expect("a torus is boundable");
    let basis = basis_along([0.0, 0.0, 1.0]);
    let view = crate::project_solid(
        &arena,
        solid,
        &basis,
        crate::tessellate::RENDER_CHORD_TOLERANCE_REL,
    )
    .expect("projects");
    let got = view.bbox.expect("curves");
    let want = aabb_projection(&basis, lo, hi);
    for (g, w) in [
        (got.min.x(), want.min.x()),
        (got.min.y(), want.min.y()),
        (got.max.x(), want.max.x()),
        (got.max.y(), want.max.y()),
    ] {
        assert!((g - w).abs() < 1e-15, "projected {g} vs AABB {w}");
    }
}

/// Seen EDGE-ON (`w·a = 0`) the torus's silhouette is four exact curves: the
/// two latitude circles of radius `R` at `τ = ±r`, seen edge-on and so
/// projecting to segments of length `2R`, and the two profile circles of
/// radius `r`, whose plane is perpendicular to the line of sight and so
/// project to exact circles.
#[test]
fn a_torus_seen_edge_on_has_its_two_latitude_and_two_profile_circles() {
    let (major, minor) = (0.020, 0.006);
    let (arena, _, fid) = closed_torus(major, minor);
    let curves = sil(&arena, fid, [0.0, 1.0, 0.0]);
    assert_eq!(curves.len(), 4, "four curves, got {curves:?}");
    let mut segments = Vec::new();
    let mut circles = Vec::new();
    for c in &curves {
        match c {
            Curve2::Line { start, end } => segments.push((*start, *end)),
            Curve2::Circle { center, radius, .. } => circles.push((*center, *radius)),
            other => panic!("unexpected silhouette arm {other:?}"),
        }
    }
    assert_eq!(segments.len(), 2, "two edge-on latitude circles");
    assert_eq!(circles.len(), 2, "two profile circles");
    // Looking along +y: u = +x̂, v = +ẑ. The latitude circles sit at z = ±r
    // and span x ∈ [−R, R].
    let mut vs: Vec<f64> = Vec::new();
    for (a, b) in &segments {
        assert!(close(a.y(), b.y(), 1e-15), "edge-on, so v is constant");
        assert!(
            close((b.x() - a.x()).abs(), 2.0 * major, 1e-15),
            "length {} vs 2R = {}",
            (b.x() - a.x()).abs(),
            2.0 * major
        );
        vs.push(a.y());
    }
    vs.sort_by(f64::total_cmp);
    assert!(
        close(vs[0], -minor, 1e-17) && close(vs[1], minor, 1e-17),
        "at v = {vs:?}"
    );
    let mut us: Vec<f64> = Vec::new();
    for (c, r) in &circles {
        assert!(close(*r, minor, 1e-17), "profile radius {r} vs {minor}");
        assert!(c.y().abs() < 1e-17, "profile centre on v = 0, got {c:?}");
        us.push(c.x());
    }
    us.sort_by(f64::total_cmp);
    assert!(
        close(us[0], -major, 1e-17) && close(us[1], major, 1e-17),
        "profile centres at u = {us:?}"
    );
}

/// Obliquely, the two branches are sampled — and their bbox must equal the
/// torus's own ANALYTIC support box, `R·√(1 − (û·â)²) + r` per view axis.
/// (The kernel's `conservative_aabb` cannot be the reference here: it bounds
/// a torus by the cube `centre ± (R + r)`, which is loose along the axis.)
#[test]
fn an_oblique_torus_branchs_bbox_is_the_analytic_support_box() {
    let (major, minor) = (0.020, 0.006);
    let (arena, _, fid) = closed_torus(major, minor);
    let a = [0.0, 0.0, 1.0];
    for dir in [
        [0.0, 1.0, 0.0],
        [0.3, -0.5, 0.8],
        [1.0, 1.0, 1.0],
        [0.1, 0.0, 0.995],
        [2.0, -1.0, 0.5],
    ] {
        let basis = basis_along(dir);
        let curves = sil(&arena, fid, dir);
        assert!(
            !curves.is_empty(),
            "a torus always has a silhouette: {dir:?}"
        );
        let mut bb: Option<Aabb2> = None;
        for c in &curves {
            bb = Some(match bb {
                None => c.bbox(),
                Some(b) => b.united(c.bbox()),
            });
        }
        let bb = bb.expect("curves");
        let support = |axis: [f64; 3]| {
            let s = (1.0 - dot(axis, a) * dot(axis, a)).max(0.0).sqrt();
            major * s + minor
        };
        // The polyline sampling is inscribed, so it may fall SHORT of the
        // support by at most the render sagitta on the tube.
        let sag = minor * (1.0 - (PI / f64::from(n_seg())).cos());
        for (got, want, which) in [
            (bb.max.x(), support(basis.u), "max u"),
            (-bb.min.x(), support(basis.u), "min u"),
            (bb.max.y(), support(basis.v), "max v"),
            (-bb.min.y(), support(basis.v), "min v"),
        ] {
            assert!(
                got <= want + 1e-15 && got >= want - 4.0 * sag,
                "{dir:?} {which}: {got} vs analytic support {want} (sagitta {sag})"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 2. the brute-force agreement, both ways
// ---------------------------------------------------------------------------

/// A closed curved fixture with a KNOWN parameter domain, so a brute-force
/// sweep of the surface can be restricted to the face without a membership
/// test: `(name, arena, face, the surface, u/v domain, a point at (u, v))`.
struct Sweep {
    name: &'static str,
    arena: BrepArena,
    face: FaceId,
    surface: Surface,
    /// `(u0, u1, v0, v1)` — the face IS the whole image of this rectangle.
    domain: (f64, f64, f64, f64),
    /// `(u, v) ↦ 3-D point`.
    point: fn(&Surface, f64, f64) -> Point3,
    /// Characteristic length for the tolerances.
    scale: f64,
}

fn cylinder_point(s: &Surface, u: f64, v: f64) -> Point3 {
    let Surface::Cylinder {
        axis_point,
        axis_dir,
        radius,
        ..
    } = *s
    else {
        unreachable!()
    };
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let e1 = unit(any_perpendicular(a)).expect("frame");
    let e2 = cross(a, e1);
    let (sn, cs) = u.sin_cos();
    pt(add(
        axis_point.as_array(),
        add(
            scaled(a, v),
            add(scaled(e1, radius * cs), scaled(e2, radius * sn)),
        ),
    ))
}

fn cone_point(s: &Surface, u: f64, v: f64) -> Point3 {
    let Surface::Cone {
        apex,
        axis_dir,
        half_angle,
        ..
    } = *s
    else {
        unreachable!()
    };
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let e1 = unit(any_perpendicular(a)).expect("frame");
    let e2 = cross(a, e1);
    let rho = v * half_angle.tan();
    let (sn, cs) = u.sin_cos();
    pt(add(
        apex.as_array(),
        add(
            scaled(a, v),
            add(scaled(e1, rho * cs), scaled(e2, rho * sn)),
        ),
    ))
}

fn sphere_point(s: &Surface, u: f64, v: f64) -> Point3 {
    let Surface::Sphere { center, radius, .. } = *s else {
        unreachable!()
    };
    let (su, cu) = u.sin_cos();
    let (sv, cv) = v.sin_cos();
    Point3::new(
        center.x() + radius * cv * cu,
        center.y() + radius * cv * su,
        center.z() + radius * sv,
    )
}

fn torus_point(s: &Surface, u: f64, v: f64) -> Point3 {
    let Surface::Torus {
        center,
        axis_dir,
        major_radius,
        minor_radius,
        ..
    } = *s
    else {
        unreachable!()
    };
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let e1 = unit(any_perpendicular(a)).expect("frame");
    let e2 = cross(a, e1);
    let rho = major_radius + minor_radius * v.cos();
    let (su, cu) = u.sin_cos();
    pt(add(
        center.as_array(),
        add(
            add(scaled(e1, rho * cu), scaled(e2, rho * su)),
            scaled(a, minor_radius * v.sin()),
        ),
    ))
}

fn sweeps() -> Vec<Sweep> {
    let mut out = Vec::new();

    let (r, h) = (0.008, 0.030);
    let (arena, _, face) = full_cylinder(
        r,
        h,
        Point3::new(0.0, 0.0, 0.0),
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    );
    let surface = arena.face(face).expect("face").surface.expect("surface");
    out.push(Sweep {
        name: "cylinder",
        arena,
        face,
        surface,
        domain: (0.0, TAU, 0.0, h),
        point: cylinder_point,
        scale: r,
    });

    let (tau0, tau1, half) = (0.010, 0.030, 0.4);
    let (arena, _, face) = build_frustum(
        Point3::new(0.0, 0.0, 0.0),
        UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
        tau0,
        tau1,
        half,
        half,
    );
    let surface = arena.face(face).expect("face").surface.expect("surface");
    out.push(Sweep {
        name: "frustum",
        arena,
        face,
        surface,
        domain: (0.0, TAU, tau0, tau1),
        point: cone_point,
        scale: tau1 * half.tan(),
    });

    let rs = 0.011;
    let (arena, _, face) = closed_sphere(rs);
    let surface = arena.face(face).expect("face").surface.expect("surface");
    out.push(Sweep {
        name: "sphere",
        arena,
        face,
        surface,
        // Poles excluded: the normal is defined there but the lat/long chart
        // degenerates, and the pole is a single point on every meridian.
        domain: (0.0, TAU, -FRAC_PI_2 + 1e-6, FRAC_PI_2 - 1e-6),
        point: sphere_point,
        scale: rs,
    });

    let (major, minor) = (0.020, 0.006);
    let (arena, _, face) = closed_torus(major, minor);
    let surface = arena.face(face).expect("face").surface.expect("surface");
    out.push(Sweep {
        name: "torus",
        arena,
        face,
        surface,
        domain: (0.0, TAU, 0.0, TAU),
        point: torus_point,
        scale: minor,
    });

    out
}

/// 54 deterministic view directions: a 6 × 9 grid in (azimuth, elevation),
/// spanning face-on, oblique and axis-aligned for every fixture.
fn directions() -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    for i in 0..6 {
        let az = TAU * f64::from(i) / 6.0;
        for j in 0..9 {
            let el = -FRAC_PI_2 + PI * f64::from(j) / 8.0;
            let (sa, ca) = az.sin_cos();
            let (se, ce) = el.sin_cos();
            out.push([ce * ca, ce * sa, se]);
        }
    }
    out
}

/// `n·w` at a surface point, or `None` where the normal is undefined.
fn normal_dot(surface: &Surface, p: Point3, w: [f64; 3]) -> Option<f64> {
    crate::signature::outward_normal_at(surface, p).map(|n| dot(n, w))
}

/// Four closed curved surfaces × 54 view directions, with the exact
/// silhouette located by bisection on the surface's own `(u, v)` grid and
/// compared BOTH WAYS against the reported paths.
///
/// Degenerate configurations — where `n·w ≡ 0` over a whole region (a
/// cylinder or cone seen along its axis) or where the two rulings graze into
/// one — are excluded by an ANALYTIC predicate and counted, never by
/// inspecting the answer.
#[test]
fn the_reported_silhouettes_agree_with_a_brute_force_normal_sign_sweep() {
    const GRID: usize = 72;
    let dirs = directions();
    let mut configurations = 0usize;
    let mut generic = 0usize;
    let mut zeros_checked = 0usize;
    let mut samples_checked = 0usize;

    for sweep in sweeps() {
        for &dir in &dirs {
            configurations += 1;
            let w = unit(dir).expect("a unit direction");
            if !is_generic(&sweep.surface, w) {
                continue;
            }
            generic += 1;
            let paths = clipped_paths(&sweep.arena, sweep.face, w, n_seg())
                .unwrap_or_else(|e| panic!("{} along {dir:?}: {e}", sweep.name));
            let tol = 1e-6 * sweep.scale;
            let (u0, u1, v0, v1) = sweep.domain;

            // ---- (a) every exact grid zero lies on a reported path -------
            // BOTH grid directions. Only the `u` sweep finds a cylinder's or
            // a cone's rulings at all: there `n·w` does not vary with `v`, so
            // a `v`-only sweep would make this half of the oracle vacuous on
            // exactly the two surfaces whose silhouette is a straight line.
            for along_u in [false, true] {
                let (f0, f1, g0, g1) = if along_u {
                    (v0, v1, u0, u1)
                } else {
                    (u0, u1, v0, v1)
                };
                for i in 0..=GRID {
                    let fixed = f0 + (f1 - f0) * (i as f64) / (GRID as f64);
                    let at = |g: f64| {
                        if along_u {
                            (sweep.point)(&sweep.surface, g, fixed)
                        } else {
                            (sweep.point)(&sweep.surface, fixed, g)
                        }
                    };
                    let mut prev_g = g0;
                    let mut prev = normal_dot(&sweep.surface, at(g0), w);
                    for j in 1..=GRID {
                        let g = g0 + (g1 - g0) * (j as f64) / (GRID as f64);
                        let cur = normal_dot(&sweep.surface, at(g), w);
                        if let (Some(a), Some(b)) = (prev, cur) {
                            if (a < 0.0) != (b < 0.0) {
                                let p = bisect(&sweep, fixed, prev_g, g, a, w, along_u);
                                zeros_checked += 1;
                                let off = on_some_path(&paths, p);
                                assert!(
                                    off <= tol,
                                    "{} along {dir:?}: the silhouette point {p:?} \
                                     (fixed {fixed}, between {prev_g} and {g}, along_u \
                                     {along_u}) is {off} off every reported path",
                                    sweep.name
                                );
                            }
                        }
                        prev_g = g;
                        prev = cur;
                    }
                }
            }

            // ---- (b) every point of a reported path is a silhouette ------
            for (path, intervals) in &paths {
                for &(s0, s1) in intervals {
                    for k in 0..=32 {
                        let s = s0 + (s1 - s0) * f64::from(k) / 32.0;
                        let p = path.eval(s);
                        samples_checked += 1;
                        let Some(d) = normal_dot(&sweep.surface, p, w) else {
                            continue;
                        };
                        assert!(
                            d.abs() <= 1e-9,
                            "{} along {dir:?}: a reported path point {p:?} has n·w = {d}",
                            sweep.name
                        );
                        assert!(
                            in_domain(&sweep, p),
                            "{} along {dir:?}: a reported path point {p:?} is off the face",
                            sweep.name
                        );
                    }
                }
            }
        }
    }

    assert_eq!(configurations, 4 * 54, "4 surfaces × 54 directions");
    // Measured 2026-10-03: 180 of the 216 are generic. The 36 excluded are
    // the cylinder's and the frustum's near-axial views plus the frustum's
    // own shadow cone — all of them named by `is_generic`, never by the
    // answer.
    assert_eq!(generic, 180, "generic configurations");
    // Both counts are deterministic functions of the fixtures, the direction
    // list and `GRID`, so they are PINNED rather than floored: a change that
    // quietly stopped finding silhouette points would otherwise pass.
    // Measured 2026-10-03.
    assert_eq!(zeros_checked, 28_924, "exact grid zeros compared");
    assert_eq!(samples_checked, 10_494, "reported path samples compared");
}

/// Whether `(surface, w)` is a GENERIC configuration — one with a silhouette
/// CURVE rather than a region or a grazing tangency. Analytic, so the
/// brute-force test never decides what to check by looking at the answer.
fn is_generic(surface: &Surface, w: [f64; 3]) -> bool {
    match *surface {
        Surface::Plane(_) => false,
        Surface::Cylinder { axis_dir, .. } => {
            let a = [axis_dir.x, axis_dir.y, axis_dir.z];
            // Away from the axis: at `|w·a| → 1` every normal is already
            // perpendicular to `w` and the whole face is the locus.
            dot(a, w).abs() < 0.99
        }
        Surface::Cone {
            axis_dir,
            half_angle,
            ..
        } => {
            let a = [axis_dir.x, axis_dir.y, axis_dir.z];
            let wp = dot(a, w);
            let m = (1.0 - wp * wp).max(0.0).sqrt();
            if m < 1e-6 {
                return false;
            }
            // Strictly inside the two-ruling regime, with margin so a
            // grazing double root is not compared against a bisection.
            (half_angle.tan() * wp / m).abs() < 0.98
        }
        Surface::Sphere { .. } | Surface::Torus { .. } => true,
    }
}

/// The exact silhouette point between two stations of one grid line.
fn bisect(
    sweep: &Sweep,
    fixed: f64,
    mut lo: f64,
    mut hi: f64,
    mut flo: f64,
    w: [f64; 3],
    along_u: bool,
) -> Point3 {
    let at = |g: f64| {
        if along_u {
            (sweep.point)(&sweep.surface, g, fixed)
        } else {
            (sweep.point)(&sweep.surface, fixed, g)
        }
    };
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        let Some(fm) = normal_dot(&sweep.surface, at(mid), w) else {
            break;
        };
        if (flo < 0.0) != (fm < 0.0) {
            hi = mid;
        } else {
            lo = mid;
            flo = fm;
        }
    }
    at(0.5 * (lo + hi))
}

/// Distance from `p` to the nearest reported path interval.
fn on_some_path(paths: &[ClippedPath], p: Point3) -> f64 {
    let mut best = f64::INFINITY;
    for (path, intervals) in paths {
        let s = path.param_of(p);
        for &(s0, s1) in intervals {
            // The parameter must be INSIDE the clipped interval (allowing for
            // the closed paths' `+2π` wrap), not merely on the unclipped path.
            let mut t = s;
            if path.closed() {
                while t < s0 - 1e-12 {
                    t += TAU;
                }
            }
            let t = t.clamp(s0, s1);
            best = best.min(norm(sub(p.as_array(), path.eval(t).as_array())));
        }
    }
    best
}

/// Whether `p` lies in the fixture's own parameter domain — the axial window
/// of a cylinder or cone band; a sphere and a closed torus have none.
fn in_domain(sweep: &Sweep, p: Point3) -> bool {
    let (_, _, v0, v1) = sweep.domain;
    let slack = 1e-9 * sweep.scale;
    match sweep.surface {
        Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } => {
            let t = dot(
                sub(p.as_array(), axis_point.as_array()),
                [axis_dir.x, axis_dir.y, axis_dir.z],
            );
            t >= v0 - slack && t <= v1 + slack
        }
        Surface::Cone { apex, axis_dir, .. } => {
            let t = dot(
                sub(p.as_array(), apex.as_array()),
                [axis_dir.x, axis_dir.y, axis_dir.z],
            );
            t >= v0 - slack && t <= v1 + slack
        }
        _ => true,
    }
}

// ---------------------------------------------------------------------------
// 3. the §5.3 oracle, now an EQUALITY for the curved fixtures
// ---------------------------------------------------------------------------

fn aabb_projection(basis: &ViewBasis, lo: [f64; 3], hi: [f64; 3]) -> Aabb2 {
    let mut bb: Option<Aabb2> = None;
    for i in 0..8 {
        let p = [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ];
        let (uv2, _) = basis.project(p);
        bb = Some(match bb {
            None => Aabb2::point(uv2),
            Some(b) => b.united_point(uv2),
        });
    }
    bb.expect("eight corners")
}

/// §5.3's equality, now reachable: **the projected bbox equals the solid's
/// own EXACT support box**, for every curved fixture and every axis view.
///
/// §5.3 writes the reference as "the solid AABB's projection", and at D1a the
/// equality was asserted for the prismatic cases only, for two reasons. One
/// was silhouettes, which this increment supplies. The other is that
/// `introspect::conservative_aabb` is documented CONSERVATIVE and bounds a
/// circular EDGE by the box of its whole circle, so a `z`-axis cylinder's rim
/// at `z = 0` inflates the reported box to `z ∈ [−R, R]` — an equality against
/// THAT would measure the AABB's slack, not the projection. So the reference
/// here is the exact support function of each primitive, in closed form:
///
/// | solid | `max_p p·d`, with `s = |d − (d·a)a|` |
/// |---|---|
/// | cylinder, axial `[t₀, t₁]` | `max(t₀, t₁)(d·a) + R·s` |
/// | cone frustum, `τ ∈ [τ₀, τ₁]` | `max over the two τ of τ(d·a + tan α·s)` |
/// | sphere | `c·d + R` |
/// | torus | `R·s + r` (since `s² + (d·a)² = 1`) |
///
/// Every one of these is attained on a silhouette or a rim, so the equality
/// IS the statement that the silhouette locus and its clip are right. It is
/// exact for the cylinder, the frustum and the sphere (whose silhouettes and
/// rims all stay analytic) and holds within the render sagitta on the torus,
/// whose oblique branches are sampled and therefore inscribed.
#[test]
fn the_projected_bbox_equals_the_exact_support_box_for_every_curved_fixture() {
    let views: Vec<[f64; 3]> = vec![
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    let z = [0.0, 0.0, 1.0];
    let (r_cyl, h_cyl) = (0.008, 0.030);
    let (c_arena, c_solid, _) =
        full_cylinder(r_cyl, h_cyl, Point3::new(0.0, 0.0, 0.0), z, [1.0, 0.0, 0.0]);
    let (tau0, tau1, half) = (0.010, 0.030, 0.4);
    let (f_arena, f_solid, _) = build_frustum(
        Point3::new(0.0, 0.0, 0.0),
        UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
        tau0,
        tau1,
        half,
        half,
    );
    let r_sph = 0.011;
    let (s_arena, s_solid, _) = closed_sphere(r_sph);
    let (major, minor) = (0.020, 0.006);
    let (t_arena, t_solid, _) = closed_torus(major, minor);

    // `(name, arena, solid, support(d), slack)`.
    #[allow(clippy::type_complexity)]
    let cases: Vec<(&str, &BrepArena, SolidId, Box<dyn Fn([f64; 3]) -> f64>, f64)> = vec![
        (
            "cylinder",
            &c_arena,
            c_solid,
            Box::new(move |d: [f64; 3]| {
                let (ax, s) = split(d, z);
                (0.0f64).max(h_cyl * ax) + r_cyl * s
            }),
            1e-15,
        ),
        (
            "frustum",
            &f_arena,
            f_solid,
            Box::new(move |d: [f64; 3]| {
                let (ax, s) = split(d, z);
                let k = ax + half.tan() * s;
                (tau0 * k).max(tau1 * k)
            }),
            1e-15,
        ),
        (
            "sphere",
            &s_arena,
            s_solid,
            Box::new(move |_d: [f64; 3]| r_sph),
            1e-15,
        ),
        (
            "torus",
            &t_arena,
            t_solid,
            Box::new(move |d: [f64; 3]| {
                let (_, s) = split(d, z);
                major * s + minor
            }),
            // The oblique branches are a chord-bounded polyline, inscribed,
            // so the box can fall SHORT by the tube's render sagitta.
            4.0 * minor * (1.0 - (PI / f64::from(n_seg())).cos()),
        ),
    ];

    let mut checked = 0usize;
    for (name, arena, solid, support, slack) in &cases {
        for dir in &views {
            let basis = basis_along(*dir);
            let view = crate::project_solid(
                arena,
                *solid,
                &basis,
                crate::tessellate::RENDER_CHORD_TOLERANCE_REL,
            )
            .expect("projects");
            let got = view.bbox.expect("curves");
            checked += 1;
            for (g, want, which) in [
                (got.max.x(), support(basis.u), "max u"),
                (-got.min.x(), support(neg(basis.u)), "min u"),
                (got.max.y(), support(basis.v), "max v"),
                (-got.min.y(), support(neg(basis.v)), "min v"),
            ] {
                assert!(
                    g <= want + 1e-15 && g >= want - slack,
                    "{name} along {dir:?}: {which} is {g}, the exact support is {want} \
                     (slack {slack})"
                );
            }
        }
    }
    assert_eq!(checked, 24, "4 curved fixtures × 6 axis views");
}

/// `(d·a, |d − (d·a)a|)`.
fn split(d: [f64; 3], a: [f64; 3]) -> (f64, f64) {
    let ax = dot(d, a);
    (ax, (1.0 - ax * ax).max(0.0).sqrt())
}

fn neg(d: [f64; 3]) -> [f64; 3] {
    [-d[0], -d[1], -d[2]]
}

/// A plane's face contributes no silhouette, and neither does a face with no
/// surface — the two trivial arms, pinned so a future surface arm cannot
/// silently start reporting one.
#[test]
fn a_planar_face_has_no_silhouette() {
    let (arena, _, cyl) = full_cylinder(
        0.008,
        0.030,
        Point3::new(0.0, 0.0, 0.0),
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    );
    let mut planar = 0usize;
    for &sh in &arena.solid(SolidId(0)).expect("solid").shells {
        for &f in &arena.shell(sh).expect("shell").faces {
            if f == cyl {
                continue;
            }
            assert!(matches!(
                arena.face(f).expect("face").surface,
                Some(Surface::Plane(_))
            ));
            assert!(sil(&arena, f, [0.3, -0.5, 0.8]).is_empty());
            planar += 1;
        }
    }
    assert_eq!(planar, 2, "a cylinder has two caps");
    // And a `Plane` surface reports no PATH at all, independent of the face.
    assert!(silhouette_paths(
        &Surface::Plane(Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: UnitVector3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
        }),
        [0.0, 1.0, 0.0]
    )
    .is_empty());
}

// ---------------------------------------------------------------------------
// 4. the two things the assay corpus found
// ---------------------------------------------------------------------------

/// A torus with a hole bored through its tube keeps the silhouette branches
/// the hole does not touch — the C0065 class, and the reason the "is this
/// crossing on this path?" tolerance has to be the CHORD BAND on a
/// chord-polyline boundary and not float precision.
///
/// The boolean leaves the torus face bounded by line segments approximating
/// the bore's intersection curve. Their interior points are off the exact
/// tube by a chord sagitta, so a `1e-6`-relative on-path test rejected every
/// crossing of that boundary and the torus reported NO silhouette at all —
/// measured on corpus case C0065 2026-10-03, where the whole outline of a
/// 3 m torus went missing from its drawing.
#[test]
fn a_bored_torus_keeps_the_silhouette_branches_the_bore_misses() {
    let (major, minor) = (0.020, 0.006);
    let (mut arena, torus, _) = closed_torus(major, minor);

    // A 3 mm bore straight down through the tube at θ = 0, well clear of
    // both equator circles (which sit at ρ = R ∓ r in the plane z = 0).
    let bore = Profile::circle(
        Point3::new(0.0, 0.0, -0.030),
        uv(1.0, 0.0, 0.0),
        uv(0.0, 1.0, 0.0),
        P2::new(major, 0.0),
        0.0015,
    )
    .expect("bore profile");
    let drill = crate::extrude(&mut arena, &bore, uv(0.0, 0.0, 1.0), 0.060)
        .expect("bore extrudes")
        .solid;
    let holed = crate::boolean_op(&mut arena, torus, drill, cad_primitives::BoolOp::Subtract)
        .expect("torus minus a bore");
    let fid = the_torus_face(&arena, holed);
    let face = arena.face(fid).expect("face");
    // The bore leaves the torus face bounded by a CHORD-APPROXIMATE boundary
    // — line segments, or the surface-pair curve whose only sampled form is
    // its render polyline. Either way the crossings of it land off the exact
    // tube by a chord sagitta, which is the tolerance this test is about.
    let mut approximate = 0usize;
    for lid in std::iter::once(face.outer_loop).chain(face.inner_loops.iter().copied()) {
        for h in arena.loop_half_edges(lid).expect("loop") {
            if matches!(
                arena.half_edge(h).expect("he").curve,
                crate::arena::Curve::LineSegment | crate::arena::Curve::SurfacePair { .. }
            ) {
                approximate += 1;
            }
        }
    }
    assert!(
        approximate > 2,
        "the bore's boundary should be chord-approximate, got {approximate} such edges"
    );

    // Seen along the axis the two equator circles are untouched by the bore,
    // so both survive — exactly, as circles.
    let curves = sil(&arena, fid, [0.0, 0.0, 1.0]);
    let mut radii: Vec<f64> = curves
        .iter()
        .filter_map(|c| match c {
            Curve2::Circle { radius, .. } => Some(*radius),
            _ => None,
        })
        .collect();
    radii.sort_by(f64::total_cmp);
    assert_eq!(
        radii.len(),
        2,
        "both equators survive a bore that misses them, got {curves:?}"
    );
    assert!(close(radii[0], major - minor, 1e-12), "inner {}", radii[0]);
    assert!(close(radii[1], major + minor, 1e-12), "outer {}", radii[1]);
}

/// `distance_to_triangle` against a brute-force sampling of the triangle, over
/// the vertex, edge and interior regions — the one piece of the membership
/// test that is pure arithmetic, and the one that decides whether a closed
/// path with no crossings is on its face.
#[test]
fn the_triangle_distance_matches_a_brute_force_sampling() {
    let (a, b, c) = ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 2.0, 0.5]);
    const N: usize = 160;
    let brute = |p: [f64; 3]| {
        let mut best = f64::INFINITY;
        for i in 0..=N {
            for j in 0..=(N - i) {
                let (u, v) = (i as f64 / N as f64, j as f64 / N as f64);
                let q = add(a, add(scaled(sub(b, a), u), scaled(sub(c, a), v)));
                best = best.min(norm(sub(p, q)));
            }
        }
        best
    };
    let mut checked = 0usize;
    for k in 0..6 {
        for l in 0..6 {
            for m in 0..3 {
                let p = [
                    -1.0 + 0.6 * k as f64,
                    -1.0 + 0.8 * l as f64,
                    -0.5 + 0.7 * m as f64,
                ];
                let got = distance_to_triangle(p, a, b, c);
                let want = brute(p);
                checked += 1;
                // The brute force samples the triangle, so it can only
                // OVERestimate; the exact answer must not exceed it and must
                // be within one sample step of it.
                assert!(
                    got <= want + 1e-12 && got >= want - 0.05,
                    "{p:?}: exact {got} vs sampled {want}"
                );
            }
        }
    }
    assert_eq!(checked, 108);
    // On the triangle: zero.
    let mid = add(a, add(scaled(sub(b, a), 0.25), scaled(sub(c, a), 0.25)));
    assert!(distance_to_triangle(mid, a, b, c) < 1e-15);
}
