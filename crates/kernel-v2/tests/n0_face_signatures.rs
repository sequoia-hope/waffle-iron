//! N0 defect 1 of `specs/agent_mechanical_design.md` §5.1: **a curved face's
//! `TopoSignature` was EMPTY**.
//!
//! `face_signature` bailed on `Some(Surface::Plane(_))` and returned
//! `TopoSignature::empty()` for every cylinder, cone, sphere and torus face,
//! so `Filter::SurfaceType { "cylindrical" }` could never match a real body
//! and `modeling_ops::boolean::assign_boolean_roles` scored every curved
//! result face 0.0 against both operands — tying all of them to operand A.
//!
//! The contract this pins: every surface the arena holds reports
//! `surface_type`, `area`, `centroid`, `bbox` and — for a cylinder, cone,
//! sphere or torus — the rotation-invariant `axis` descriptor. Areas and
//! centroids of curved faces come from the face's own render tessellation (an
//! inscribed partition), so they sit just inside the analytic value by the
//! chord deficit; the assertions below carry that band explicitly rather than
//! pretending the fingerprint is an exact measurement
//! (`introspect::surface_area` is the exact door).
//!
//! **A face that goes all the way round its axis reports NO point normal.**
//! Its area centroid lies on the axis, where the normal is undefined and the
//! direction to "the centroid" is f64 summation rounding: measured 3.1e-17 m
//! off a cylinder's axis, so a height change from 2 to 2.0001 swung the
//! reported centroid 58° round the axis and flipped the normal from
//! (−0.50, −0.87, 0) to (−1.00, −0.03, 0). A fingerprint exists to be stable,
//! so those faces carry the axis descriptor and an on-axis centroid instead,
//! and `stability` below is the test that keeps it that way. Partial faces
//! keep the projected centroid and the normal there, which are well defined —
//! `a_half_cylinder_keeps_its_normal_and_both_are_stable` pins both halves.

use std::f64::consts::PI;

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{extrude, face_signature, revolve, BrepArena, FaceId, Profile, SolidId, Surface};
use waffle_types::TopoSignature;

/// Every face of `solid`, in shell walk order.
fn faces_of(arena: &BrepArena, solid: SolidId) -> Vec<FaceId> {
    let s = arena.solid(solid).expect("solid");
    s.shells
        .iter()
        .flat_map(|&sh| arena.shell(sh).expect("shell").faces.clone())
        .collect()
}

/// The signature of the one face of `solid` whose surface matches `want`.
fn only_signature(arena: &BrepArena, solid: SolidId, want: &str) -> TopoSignature {
    let hits: Vec<_> = faces_of(arena, solid)
        .into_iter()
        .map(|f| (f, face_signature(arena, f)))
        .filter(|(_, sig)| sig.surface_type.as_deref() == Some(want))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "want exactly one {want} face, got {} (types: {:?})",
        hits.len(),
        faces_of(arena, solid)
            .into_iter()
            .map(|f| face_signature(arena, f).surface_type)
            .collect::<Vec<_>>()
    );
    hits.into_iter().next().unwrap().1
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// A full-turn face's fingerprint: the centroid sits ON the axis (`residual`
/// from the axis line vanishes), there is NO point normal, and the axis
/// descriptor carries the identity.
fn assert_axis_form(sig: &TopoSignature, off_axis: impl Fn([f64; 3]) -> f64) {
    assert!(
        sig.normal.is_none(),
        "a face that goes all the way round its axis has no single outward \
         normal, but reported {:?}",
        sig.normal
    );
    let c = sig.centroid.expect("signature carries a centroid");
    let d = off_axis(c);
    assert!(
        d.abs() < 1e-15,
        "centroid {c:?} is {d:e} off the axis it should be snapped onto"
    );
    assert!(
        sig.axis.is_some(),
        "a rotational face carries an axis descriptor: {sig:?}"
    );
}

/// `sig` carries a unit normal, and the centroid lies on `surface` (the
/// implicit residual, which for every curved arena surface is an exact signed
/// distance, vanishes to `tol`).
fn assert_on_surface(sig: &TopoSignature, residual: impl Fn([f64; 3]) -> f64, tol: f64) {
    let n = sig.normal.expect("signature carries a normal");
    assert!(
        (norm(n) - 1.0).abs() < 1e-12,
        "normal {n:?} is not a unit vector"
    );
    let c = sig.centroid.expect("signature carries a centroid");
    let r = residual(c);
    assert!(
        r.abs() < tol,
        "centroid {c:?} is {r:e} off its own surface (tol {tol:e})"
    );
}

/// `actual` is within `rel` of `want`, and NOT above it: a tessellated area is
/// an inscribed partition, so it comes in low or exact, never high.
fn assert_inscribed(actual: f64, want: f64, rel: f64, what: &str) {
    assert!(
        actual <= want * (1.0 + 1e-12),
        "{what}: {actual} exceeds the analytic {want}"
    );
    assert!(
        actual >= want * (1.0 - rel),
        "{what}: {actual} is more than {} below the analytic {want}",
        rel
    );
}

const R: f64 = 0.5;
const H: f64 = 2.0;

/// Solid cone of height `H` about +x̂, base radius `R` at the origin plane:
/// the full-turn revolve of an on-axis apex triangle.
fn cone(arena: &mut BrepArena) -> SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(H, 0.0),
            Point2::new(0.0, R),
        ],
        vec![],
    )
    .expect("on-axis apex triangle");
    revolve(
        arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("solid cone")
    .solid
}

/// Unit ball centred at (5, 0, 0): the full-turn revolve of an on-axis circle.
fn ball(arena: &mut BrepArena) -> SolidId {
    let c = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(5.0, 0.0),
        1.0,
    )
    .expect("circle profile");
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

/// Torus segment, major 3, minor 1, a QUARTER turn about +ŷ — the partial
/// case, whose area centroid is off the axis.
fn torus_quarter(arena: &mut BrepArena) -> SolidId {
    let c = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(3.0, 0.0),
        1.0,
    )
    .expect("circle profile");
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

fn cylinder(arena: &mut BrepArena) -> SolidId {
    let c = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        R,
    )
    .expect("circle profile");
    extrude(arena, &c, Vector3::new(0.0, 0.0, 1.0), H)
        .expect("cylinder")
        .solid
}

#[test]
fn cylinder_lateral_signature_carries_its_analytic_content() {
    let mut arena = BrepArena::new();
    let solid = cylinder(&mut arena);
    let sig = only_signature(&arena, solid, "cylindrical");

    // 2πrh, from below by the chord deficit of the inscribed prism.
    assert_inscribed(
        sig.area.expect("area"),
        2.0 * PI * R * H,
        2e-3,
        "cylinder lateral area",
    );

    // A FULL lateral: the centroid is the area centroid, which lies on the
    // axis at mid-height, and there is no single outward normal to report.
    assert_axis_form(&sig, |p| norm([p[0], p[1], 0.0]));
    let c = sig.centroid.unwrap();
    assert!(
        (c[2] - H / 2.0).abs() < 1e-9,
        "centroid at mid-height: {c:?}"
    );
    // The identity is in the descriptor: the axis, the radius, the height.
    let ax = sig.axis.expect("axis descriptor");
    assert_eq!(ax.direction, Some([0.0, 0.0, 1.0]), "{ax:?}");
    assert!((ax.radius.expect("radius") - R).abs() < 1e-15, "{ax:?}");
    assert!((ax.extent.expect("extent") - H).abs() < 1e-12, "{ax:?}");
    assert_eq!(ax.half_angle, None, "a cylinder has no half-angle: {ax:?}");
    assert_eq!(ax.minor_radius, None, "{ax:?}");

    // bbox covers the solid's radial extent, not just one seam point.
    let b = sig.bbox.expect("bbox");
    assert!(
        b[3] - b[0] > 1.9 * R && b[5] - b[2] > 0.99 * H,
        "bbox {b:?}"
    );

    // The planar caps keep their exact planar content (unchanged by N0).
    let caps: Vec<_> = faces_of(&arena, solid)
        .into_iter()
        .map(|f| face_signature(&arena, f))
        .filter(|s| s.surface_type.as_deref() == Some("planar"))
        .collect();
    assert_eq!(caps.len(), 2, "two planar caps");
    for cap in &caps {
        let a = cap.area.expect("cap area");
        assert!(
            (a.abs() - PI * R * R).abs() < 1e-12,
            "cap area {a} is exactly ±πr² = {}",
            PI * R * R
        );
    }
}

#[test]
fn cone_lateral_signature_carries_its_analytic_content() {
    let mut arena = BrepArena::new();
    let solid = cone(&mut arena);
    let sig = only_signature(&arena, solid, "conical");
    let slant = (R * R + H * H).sqrt();
    assert_inscribed(
        sig.area.expect("area"),
        PI * R * slant,
        2e-3,
        "cone lateral",
    );

    // A FULL nappe: on-axis centroid, no point normal, and the half-angle in
    // the descriptor instead of a radius (a cone's radius varies).
    assert_axis_form(&sig, |p| norm([0.0, p[1], p[2]]));
    let half_angle = (R / H).atan();
    let ax = sig.axis.expect("axis descriptor");
    assert_eq!(ax.direction, Some([1.0, 0.0, 0.0]), "{ax:?}");
    assert!(
        (ax.half_angle.expect("half-angle") - half_angle).abs() < 1e-12,
        "{ax:?}"
    );
    assert_eq!(ax.radius, None, "a cone's radius varies: {ax:?}");
    assert!((ax.extent.expect("extent") - H).abs() < 1e-12, "{ax:?}");
    let c = sig.centroid.unwrap();
    assert!(
        c[0] > 0.0 && c[0] < H,
        "the centroid is on the axis between base and apex: {c:?}"
    );
}

#[test]
fn sphere_signature_carries_its_analytic_content() {
    let mut arena = BrepArena::new();
    let solid = ball(&mut arena);

    let sig = only_signature(&arena, solid, "spherical");
    let center = [5.0, 0.0, 0.0];
    assert_inscribed(sig.area.expect("area"), 4.0 * PI, 1e-2, "sphere area");
    // A closed ball is isotropic: no distinguished point, so no point normal,
    // and its area centroid IS its centre. The descriptor carries the radius
    // and nothing else — a sphere has no axis to name.
    assert_axis_form(&sig, |p| norm(sub(p, center)));
    let ax = sig.axis.expect("axis descriptor");
    assert_eq!(ax.direction, None, "a sphere has no axis: {ax:?}");
    assert!((ax.radius.expect("radius") - 1.0).abs() < 1e-15, "{ax:?}");
    assert_eq!(ax.extent, None, "no axis to measure along: {ax:?}");
}

#[test]
fn torus_signature_carries_its_analytic_content() {
    let mut arena = BrepArena::new();
    let solid = torus_quarter(&mut arena);
    let sig = only_signature(&arena, solid, "toroidal");
    // A quarter of 4π²Rr.
    assert_inscribed(
        sig.area.expect("area"),
        PI * PI * 3.0,
        2e-2,
        "torus segment area",
    );
    assert_on_surface(
        &sig,
        |p| {
            let h = p[1];
            let rho = norm([p[0], 0.0, p[2]]);
            ((rho - 3.0) * (rho - 3.0) + h * h).sqrt() - 1.0
        },
        1e-9,
    );
    // A QUARTER turn is not rotationally symmetric, so its area centroid is
    // off the axis and the projection onto the surface is well conditioned:
    // this face keeps its point normal. It carries the descriptor too.
    let ax = sig.axis.expect("axis descriptor");
    assert_eq!(ax.direction, Some([0.0, 1.0, 0.0]), "{ax:?}");
    assert!((ax.radius.expect("major") - 3.0).abs() < 1e-15, "{ax:?}");
    assert!(
        (ax.minor_radius.expect("minor") - 1.0).abs() < 1e-15,
        "{ax:?}"
    );
}

#[test]
fn every_curved_face_of_every_fixture_has_a_non_empty_signature() {
    // The defect in one assertion: NO face of an arena solid may report an
    // empty fingerprint. A signature with no field set scores 0.0 against
    // everything, which is what bound a reference to an arbitrary face.
    //
    // Every curved fixture, not just the cylinder: the cone, the ball and the
    // torus segment exercise the other three surface arms, two of which are
    // full-turn (no point normal) and one partial (normal kept).
    let mut arena = BrepArena::new();
    let solids = vec![
        cylinder(&mut arena),
        cone(&mut arena),
        ball(&mut arena),
        torus_quarter(&mut arena),
    ];
    let mut saw_axis_form = 0usize;
    let mut saw_normal_form = 0usize;
    for solid in solids {
        for f in faces_of(&arena, solid) {
            let sig = face_signature(&arena, f);
            let surface = arena.face(f).unwrap().surface.unwrap();
            assert!(
                sig.surface_type.is_some()
                    && sig.area.is_some()
                    && sig.centroid.is_some()
                    && sig.bbox.is_some(),
                "face {f:?} ({surface:?}) has an incomplete signature: {sig:?}"
            );
            if matches!(surface, Surface::Plane(_)) {
                assert_eq!(sig.surface_type.as_deref(), Some("planar"));
                assert!(sig.normal.is_some(), "a plane has one normal: {sig:?}");
                assert!(sig.axis.is_none(), "a plane has no axis: {sig:?}");
                continue;
            }
            // A curved face carries the rotation-invariant descriptor, and
            // EITHER a point normal (partial: the centroid is on the face) or
            // none (full turn: the centroid is on the axis). Never neither —
            // that would be a face with nothing to identify it by.
            assert!(
                sig.axis.is_some(),
                "curved face {f:?} ({surface:?}) has no axis descriptor: {sig:?}"
            );
            if sig.normal.is_some() {
                saw_normal_form += 1;
            } else {
                saw_axis_form += 1;
            }
        }
    }
    assert!(
        saw_axis_form > 0 && saw_normal_form > 0,
        "the fixtures must cover both forms: {saw_axis_form} full-turn, \
         {saw_normal_form} partial"
    );
}

/// **The reason the axis form exists.** The fingerprint of a full-turn face
/// must not move when the geometry barely moves. Before this, a cylinder of
/// height 2 and one of height 2.0001 reported centroids 58° apart round the
/// axis and normals (−0.50, −0.87, 0) vs (−1.00, −0.03, 0), because the
/// area-weighted mean landed 3.1e-17 m off the axis and f64 rounding chose the
/// direction. The two fingerprints now score a full match.
#[test]
fn a_hair_of_height_does_not_move_a_full_turn_fingerprint() {
    let sig_at = |h: f64| {
        let mut arena = BrepArena::new();
        let c = Profile::circle(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Point2::new(0.0, 0.0),
            R,
        )
        .expect("circle profile");
        let solid = extrude(&mut arena, &c, Vector3::new(0.0, 0.0, 1.0), h)
            .expect("cylinder")
            .solid;
        only_signature(&arena, solid, "cylindrical")
    };
    let a = sig_at(2.0);
    let b = sig_at(2.0001);

    // Neither carries a direction that rounding could have chosen.
    assert_eq!(a.normal, None, "{a:?}");
    assert_eq!(b.normal, None, "{b:?}");
    // The axis and the radius are identical; the centroid and the extent move
    // by exactly the half-height and the height the geometry moved by.
    let (ax, bx) = (a.axis.expect("a"), b.axis.expect("b"));
    assert_eq!(ax.direction, bx.direction, "{ax:?} vs {bx:?}");
    assert_eq!(ax.radius, bx.radius, "{ax:?} vs {bx:?}");
    let (ca, cb) = (a.centroid.unwrap(), b.centroid.unwrap());
    assert!(
        (ca[0]).abs() < 1e-15 && (ca[1]).abs() < 1e-15,
        "both centroids are on the axis, so x and y are 0: {ca:?}"
    );
    assert!((cb[0]).abs() < 1e-15 && (cb[1]).abs() < 1e-15, "{cb:?}");
    assert!(
        (cb[2] - ca[2] - 0.00005).abs() < 1e-12,
        "the centroid moved by half the height change: {ca:?} -> {cb:?}"
    );
    assert!(
        (bx.extent.unwrap() - ax.extent.unwrap() - 0.0001).abs() < 1e-12,
        "{ax:?} vs {bx:?}"
    );
}

/// The far side of the same rule: a PARTIAL curved face has a well-defined
/// point normal, it keeps it, and it is stable under the same perturbation
/// that used to swing a full-turn one by 58°.
///
/// A HALF-turn revolve of a rectangle about +ŷ: a half-annular solid whose
/// outer and inner laterals are 180° cylindrical bands, so their area
/// centroids are genuinely off the axis.
#[test]
fn a_partial_cylindrical_band_keeps_its_normal_and_both_are_stable() {
    let outer_lateral = |height: f64| {
        let mut arena = BrepArena::new();
        let profile = Profile::new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            vec![
                Point2::new(2.0, 0.0),
                Point2::new(3.0, 0.0),
                Point2::new(3.0, height),
                Point2::new(2.0, height),
            ],
            vec![],
        )
        .expect("rectangle profile");
        let solid = revolve(
            &mut arena,
            &profile,
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            PI,
        )
        .expect("half annulus")
        .solid;
        // The outer band: cylindrical, radius 3.
        faces_of(&arena, solid)
            .into_iter()
            .map(|f| face_signature(&arena, f))
            .filter(|s| s.surface_type.as_deref() == Some("cylindrical"))
            .filter(|s| s.axis.and_then(|a| a.radius).is_some_and(|r| r > 2.5))
            .max_by(|p, q| {
                let key = |s: &TopoSignature| s.area.unwrap_or(0.0);
                key(p)
                    .partial_cmp(&key(q))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("the outer cylindrical band")
    };
    let a = outer_lateral(1.0);
    let b = outer_lateral(1.0001);

    // A 180° band HAS one outward normal at its centroid, and it is radial.
    let (na, nb) = (a.normal.expect("a has a normal"), b.normal.expect("b"));
    assert!(
        dot(na, [0.0, 1.0, 0.0]).abs() < 1e-9,
        "a cylinder band's outward normal is radial, got {na:?}"
    );
    let ca = a.centroid.expect("centroid");
    assert!(
        (norm([ca[0], 0.0, ca[2]]) - 3.0).abs() < 1e-9,
        "and its centroid is ON the band, radius 3: {ca:?}"
    );
    // Stable under the same hair of height that moved the full-turn one.
    assert!(
        dot(na, nb) > 1.0 - 1e-9,
        "the normal moved: {na:?} vs {nb:?}"
    );
    let cb = b.centroid.expect("centroid");
    assert!(
        norm(sub([ca[0], 0.0, ca[2]], [cb[0], 0.0, cb[2]])) < 1e-9,
        "nor did its position round the axis: {ca:?} vs {cb:?}"
    );
    // And it still carries the descriptor, radius and all.
    let ax = a.axis.expect("axis descriptor");
    assert_eq!(ax.direction, Some([0.0, 1.0, 0.0]), "{ax:?}");
    assert!((ax.radius.expect("radius") - 3.0).abs() < 1e-15, "{ax:?}");
}
