//! N0 defect 1 of `specs/agent_mechanical_design.md` §5.1: **a curved face's
//! `TopoSignature` was EMPTY**.
//!
//! `face_signature` bailed on `Some(Surface::Plane(_))` and returned
//! `TopoSignature::empty()` for every cylinder, cone, sphere and torus face,
//! so `Filter::SurfaceType { "cylindrical" }` could never match a real body
//! and `modeling_ops::boolean::assign_boolean_roles` scored every curved
//! result face 0.0 against both operands — tying all of them to operand A.
//!
//! The contract this pins, per the spec: every surface the arena holds
//! reports `surface_type`, `area`, `centroid`, `bbox`, and `normal` as the
//! OUTWARD normal AT the centroid. Areas and centroids of curved faces come
//! from the face's own render tessellation (an inscribed partition), so they
//! sit just inside the analytic value by the chord deficit — the assertions
//! below carry that band explicitly rather than pretending the fingerprint is
//! an exact measurement (`introspect::surface_area` is the exact door).

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

    // The centroid sits ON the cylinder (radius R from the axis), at
    // mid-height — and the normal there is RADIAL, not axial: this is the
    // field `assign_boolean_roles` and `Filter::NormalDirection` read.
    assert_on_surface(
        &sig,
        |p| {
            let d = sub(p, [0.0, 0.0, p[2]]);
            norm(d) - R
        },
        1e-9,
    );
    let n = sig.normal.unwrap();
    assert!(
        dot(n, [0.0, 0.0, 1.0]).abs() < 1e-12,
        "a cylinder lateral's outward normal is radial, got {n:?}"
    );
    let c = sig.centroid.unwrap();
    assert!(
        dot(n, [c[0], c[1], 0.0]) > 0.0,
        "normal {n:?} points away from the axis at {c:?}"
    );
    assert!(
        (c[2] - H / 2.0).abs() < 1e-9,
        "centroid at mid-height: {c:?}"
    );

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
    // Full-turn revolve of an on-axis apex triangle: the solid cone of
    // height H about +x̂, base radius R at the origin plane.
    let mut arena = BrepArena::new();
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
    let solid = revolve(
        &mut arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("solid cone")
    .solid;

    let sig = only_signature(&arena, solid, "conical");
    let slant = (R * R + H * H).sqrt();
    assert_inscribed(
        sig.area.expect("area"),
        PI * R * slant,
        2e-3,
        "cone lateral",
    );

    // On the nappe: radial = τ·tanα with τ the axial distance from the apex
    // at (H, 0, 0) toward −x̂.
    let half_angle = (R / H).atan();
    assert_on_surface(
        &sig,
        |p| {
            let tau = H - p[0];
            norm([0.0, p[1], p[2]]) - tau * half_angle.tan()
        },
        1e-9,
    );
    let n = sig.normal.unwrap();
    let c = sig.centroid.unwrap();
    // The solid narrows toward the apex at +x̂, so leaving the lateral means
    // moving radially out AND toward the apex: `cosα·r̂ − sinα·â` with the
    // stored `â = −x̂` (apex → base). Checked against a membership probe:
    // at x = 1 the allowed radius is 0.25, and (1.1, 0.25, 0) is outside.
    assert!(
        dot(n, [0.0, c[1], c[2]]) > 0.0,
        "the lateral normal points away from the axis: {n:?} at {c:?}"
    );
    let want_axial = (R / H).atan().sin();
    assert!(
        (dot(n, [1.0, 0.0, 0.0]) - want_axial).abs() < 1e-12,
        "its axial part is sinα = {want_axial}, toward the apex: {n:?}"
    );
}

#[test]
fn sphere_signature_carries_its_analytic_content() {
    // Full-turn revolve of an on-axis circle: the closed ball.
    let mut arena = BrepArena::new();
    let c = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(5.0, 0.0),
        1.0,
    )
    .expect("circle profile");
    let solid = revolve(
        &mut arena,
        &c,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("sphere")
    .solid;

    let sig = only_signature(&arena, solid, "spherical");
    let center = [5.0, 0.0, 0.0];
    assert_inscribed(sig.area.expect("area"), 4.0 * PI, 1e-2, "sphere area");
    assert_on_surface(&sig, |p| norm(sub(p, center)) - 1.0, 1e-9);
    let n = sig.normal.unwrap();
    let cc = sig.centroid.unwrap();
    assert!(
        dot(n, sub(cc, center)) > 0.99,
        "a ball's normal is the outward radial: {n:?} at {cc:?}"
    );
}

#[test]
fn torus_signature_carries_its_analytic_content() {
    // Quarter-turn revolve of an off-axis circle: a torus segment, R = 3,
    // r = 1, about +ŷ.
    let mut arena = BrepArena::new();
    let c = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(3.0, 0.0),
        1.0,
    )
    .expect("circle profile");
    let solid = revolve(
        &mut arena,
        &c,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        PI / 2.0,
    )
    .expect("torus segment")
    .solid;

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
}

#[test]
fn every_curved_face_of_every_fixture_has_a_non_empty_signature() {
    // The defect in one assertion: NO face of an arena solid may report an
    // empty fingerprint. A signature with no field set scores 0.0 against
    // everything, which is what bound a reference to an arbitrary face.
    let mut arena = BrepArena::new();
    let solids = vec![cylinder(&mut arena)];
    for solid in solids {
        for f in faces_of(&arena, solid) {
            let sig = face_signature(&arena, f);
            let surface = arena.face(f).unwrap().surface.unwrap();
            assert!(
                sig.surface_type.is_some()
                    && sig.area.is_some()
                    && sig.centroid.is_some()
                    && sig.normal.is_some()
                    && sig.bbox.is_some(),
                "face {f:?} ({surface:?}) has an incomplete signature: {sig:?}"
            );
            if matches!(surface, Surface::Plane(_)) {
                assert_eq!(sig.surface_type.as_deref(), Some("planar"));
            }
        }
    }
}
