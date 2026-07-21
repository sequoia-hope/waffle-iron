//! P3b inc-4d-1 unit fixtures: `circle_edge_plane_face_pierce` contract
//! (spec `specs/yang_169_p3b_curved_partner_pierce.md` §7.3, §7.5).
//!
//! The primitive is UNWIRED this sub-increment (wiring into
//! `junction_pierce_points` is inc-4d-3, behind `YANG_P3B_PIERCE_ENABLE`);
//! fixtures drive it directly. The flagship pin uses the LIVE F0082
//! Extrude-11 descriptors (§7.1/§7.2): B's rim-0 cap circle × A's wall
//! plane must yield J2 to 9 decimals.

use super::n2_junction::rj_cylinder;
use crate::boolean::circle_edge_plane_face_pierce;
use crate::*;

/// A rectangular ALL-LINE plate on the plane `normal·x + d = 0`: corners
/// `p0 ± h1·b1 ± h2·b2` (CCW viewed along +normal), one planar face.
fn plate(
    p0: [f64; 3],
    b1: [f64; 3],
    b2: [f64; 3],
    h1: f64,
    h2: f64,
    normal: Vector3,
    d: f64,
) -> BRep {
    let corner = |s1: f64, s2: f64| BRepVertex {
        point: Point3::new(
            p0[0] + s1 * h1 * b1[0] + s2 * h2 * b2[0],
            p0[1] + s1 * h1 * b1[1] + s2 * h2 * b2[1],
            p0[2] + s1 * h1 * b1[2] + s2 * h2 * b2[2],
        ),
    };
    let vertices = vec![
        corner(1.0, 1.0),
        corner(-1.0, 1.0),
        corner(-1.0, -1.0),
        corner(1.0, -1.0),
    ];
    let edges = (0..4u32)
        .map(|i| BRepEdge {
            start: i,
            end: (i + 1) % 4,
            curve: Curve::LineSegment,
        })
        .collect();
    let faces = vec![BRepFace {
        surface: Surface::Plane { normal, d },
        outer_loop: vec![0, 1, 2, 3],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    BRep::new(vertices, edges, faces).expect("plate fixture is a valid B-Rep")
}

/// The synthetic rim: unit-frame circle r=0.25 in the z=0 plane, seam at
/// (0.25, 0, 0), owner surfaces = the z=0 cap plane + the coaxial cylinder
/// (the rim lies ON both by construction — the canonical tube-rim shape).
fn synthetic_rim() -> (Point3, Vector3, f64, Point3, Surface, Surface) {
    (
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        0.25,
        Point3::new(0.25, 0.0, 0.0),
        Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        },
        Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, -0.5),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 0.25,
        },
    )
}

/// Flagship pin (spec §7.2, live F0082 Extrude-11 descriptors): B's rim-0
/// cap circle pierces A's WALL plane at J2 = (-0.063997163, -0.109109265,
/// 2.109448193) — 1.0537e-4 from the minted J, transversality ≈ 0.475.
/// The partner plate spans ±0.05 around J2's in-plane neighbourhood, so
/// exactly ONE root is contained (the second wall root at y≈+0.0918 falls
/// outside).
#[test]
fn f0082_rim_wall_pierce_pins_j2() {
    // Live rim-0 (cap) circle of the fresh tube tool.
    let c = Point3::new(
        0.1227322098851793,
        -0.008327366889270053,
        2.1018871743865217,
    );
    let axis = [
        0.06821305565326538,
        -0.05163709792422363,
        0.9963335732355949,
    ];
    let rim_n = Vector3::new(-axis[0], -axis[1], -axis[2]);
    let r = 0.2123252664164556;
    let seam = Point3::new(0.1227322098851793, -0.22036804832395687, 2.0908977169143577);
    // Owner surfaces: the cap plane (normal = −axis through the center) and
    // the tube lateral.
    let ca = c.as_array();
    let cap_d = axis[0] * ca[0] + axis[1] * ca[1] + axis[2] * ca[2];
    let s_cap = Surface::Plane {
        normal: Vector3::new(-axis[0], -axis[1], -axis[2]),
        d: cap_d,
    };
    let s_lat = Surface::Cylinder {
        axis_point: c,
        axis_dir: Vector3::new(axis[0], axis[1], axis[2]),
        radius: r,
    };
    // Live wall plane of the accumulated body (probe-pinned, §7.1).
    let wn = [
        -0.9987176408266406,
        -0.0009043814193418063,
        0.050618731670377184,
    ];
    let wd = -0.17079136415422735;
    // Plate centered at the wall-plane point nearest the expected J2.
    let j2 = [-0.063997163, -0.109109265, 2.109448193];
    let off = wn[0] * j2[0] + wn[1] * j2[1] + wn[2] * j2[2] + wd;
    let p0 = [
        j2[0] - off * wn[0],
        j2[1] - off * wn[1],
        j2[2] - off * wn[2],
    ];
    // In-plane basis.
    let b1 = {
        // wn × ŷ, normalized — an in-plane direction.
        let raw: [f64; 3] = [-wn[2], 0.0, wn[0]];
        let l = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]).sqrt();
        [raw[0] / l, raw[1] / l, raw[2] / l]
    };
    let b2 = [
        wn[1] * b1[2] - wn[2] * b1[1],
        wn[2] * b1[0] - wn[0] * b1[2],
        wn[0] * b1[1] - wn[1] * b1[0],
    ];
    let y = plate(
        p0,
        b1,
        b2,
        0.05,
        0.05,
        Vector3::new(wn[0], wn[1], wn[2]),
        wd,
    );
    let out = circle_edge_plane_face_pierce(c, rim_n, r, seam, s_cap, s_lat, 0, &y.faces()[0], &y);
    assert_eq!(out.len(), 1, "exactly the J2 root is contained: {out:?}");
    let p = out[0].point.as_array();
    assert!(
        (p[0] - j2[0]).abs() < 1e-9 && (p[1] - j2[1]).abs() < 1e-9 && (p[2] - j2[2]).abs() < 1e-9,
        "J2 pinned to 9 decimals, got ({:.9},{:.9},{:.9})",
        p[0],
        p[1],
        p[2]
    );
    // Exactly on the wall plane and on the rim circle (the mint IS the
    // junction — both to machine precision).
    assert!((wn[0] * p[0] + wn[1] * p[1] + wn[2] * p[2] + wd).abs() < 1e-14);
    let w = [p[0] - ca[0], p[1] - ca[1], p[2] - ca[2]];
    let along = w[0] * axis[0] + w[1] * axis[1] + w[2] * axis[2];
    let rad = ((w[0] - along * axis[0]).powi(2)
        + (w[1] - along * axis[1]).powi(2)
        + (w[2] - along * axis[2]).powi(2))
    .sqrt();
    assert!((rad - r).abs() < 1e-14, "on the rim circle, radial {rad}");
    assert!(
        (out[0].transversality - 0.475).abs() < 0.01,
        "well-conditioned crossing, got {}",
        out[0].transversality
    );
    assert!(out[0].t > 0.0 && out[0].t < 1.0);
}

/// A secant plane through the synthetic rim mints BOTH roots with the
/// analytic values x = 0.1, y = ±√(r²−0.01), sorted by seam-relative angle.
#[test]
fn secant_plane_mints_both_roots_exactly() {
    let (c, n, r, seam, s1, s2) = synthetic_rim();
    let y = plate(
        [0.1, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        0.4,
        0.4,
        Vector3::new(1.0, 0.0, 0.0),
        -0.1,
    );
    let out = circle_edge_plane_face_pierce(c, n, r, seam, s1, s2, 7, &y.faces()[0], &y);
    assert_eq!(out.len(), 2, "both crossings mint: {out:?}");
    let y_hit = (0.25f64 * 0.25 - 0.01).sqrt();
    let transv = y_hit / 0.25;
    let mut ys: Vec<f64> = out.iter().map(|pp| pp.point.y()).collect();
    ys.sort_by(f64::total_cmp);
    for (got, want) in ys.iter().zip([-y_hit, y_hit]) {
        assert!(
            (got - want).abs() < 1e-12,
            "analytic root y, got {got} want {want}"
        );
    }
    for pp in &out {
        let p = pp.point.as_array();
        assert!((p[0] - 0.1).abs() < 1e-12 && p[2].abs() < 1e-12);
        assert!((pp.transversality - transv).abs() < 1e-12);
        assert!(pp.t > 0.0 && pp.t < 1.0);
        assert_eq!(pp.partner_face, 7);
    }
    assert!(out[0].t < out[1].t, "sorted by seam-relative angle");
}

/// Tangent plane (x = r exactly) and a sub-band secant (root pair closer
/// than `TAU_MODEL·(1+scale)`): the near-tangency guard rejects both — a
/// tangential contact is ONE point, never two transversal mints (A14.2).
#[test]
fn tangential_and_sub_band_secant_must_not_mint() {
    let (c, n, r, seam, s1, s2) = synthetic_rim();
    for x_plane in [0.25, 0.25 - 1e-15, 0.3] {
        let y = plate(
            [x_plane, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            0.4,
            0.4,
            Vector3::new(1.0, 0.0, 0.0),
            -x_plane,
        );
        let out = circle_edge_plane_face_pierce(c, n, r, seam, s1, s2, 0, &y.faces()[0], &y);
        assert!(
            out.is_empty(),
            "x={x_plane}: tangential/miss must not mint: {out:?}"
        );
    }
}

/// A root landing on the rim's own B-Rep seam vertex is a higher-order
/// corner: the seam margin rejects it; the opposite root still mints.
#[test]
fn seam_margin_rejects_root_at_seam() {
    let (c, n, r, seam, s1, s2) = synthetic_rim();
    // Plane y=0 crosses the rim at (±0.25, 0, 0); (+0.25,0,0) IS the seam.
    let y = plate(
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        0.4,
        0.4,
        Vector3::new(0.0, 1.0, 0.0),
        0.0,
    );
    let out = circle_edge_plane_face_pierce(c, n, r, seam, s1, s2, 0, &y.faces()[0], &y);
    assert_eq!(out.len(), 1, "only the non-seam root mints: {out:?}");
    let p = out[0].point.as_array();
    assert!((p[0] + 0.25).abs() < 1e-12 && p[1].abs() < 1e-12);
}

/// Containment is exact and margin-guarded: a plate that does not contain
/// the roots (or puts them exactly on its boundary) mints nothing.
#[test]
fn containment_and_boundary_margin_fail_closed() {
    let (c, n, r, seam, s1, s2) = synthetic_rim();
    // Plate shifted to y ∈ [0.3, 1.1]: neither root (y=±0.229) contained.
    let outside = plate(
        [0.1, 0.7, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        0.4,
        0.4,
        Vector3::new(1.0, 0.0, 0.0),
        -0.1,
    );
    let out =
        circle_edge_plane_face_pierce(c, n, r, seam, s1, s2, 0, &outside.faces()[0], &outside);
    assert!(out.is_empty(), "uncontained roots must not mint: {out:?}");
    // Plate whose boundary edge z=0 passes exactly through both roots.
    let grazing = plate(
        [0.1, 0.0, 0.4],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        0.4,
        0.4,
        Vector3::new(1.0, 0.0, 0.0),
        -0.1,
    );
    let out =
        circle_edge_plane_face_pierce(c, n, r, seam, s1, s2, 0, &grazing.faces()[0], &grazing);
    assert!(
        out.is_empty(),
        "boundary-margin roots must not mint: {out:?}"
    );
}

/// Owner on-surface postcondition: descriptors whose surfaces do not carry
/// the rim (a producer fault) must not mint.
#[test]
fn off_owner_surface_fails_closed() {
    let (c, n, r, seam, _s1, s2) = synthetic_rim();
    let bad_cap = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -0.5, // rim plane is z=0 — the rim is NOT on this surface
    };
    let y = plate(
        [0.1, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        0.4,
        0.4,
        Vector3::new(1.0, 0.0, 0.0),
        -0.1,
    );
    let out = circle_edge_plane_face_pierce(c, n, r, seam, bad_cap, s2, 0, &y.faces()[0], &y);
    assert!(out.is_empty(), "off-owner root must not mint: {out:?}");
}

/// Partner vocabulary gates: a non-planar partner face and a planar face
/// with a CURVED loop edge (a disc cap) both fail closed.
#[test]
fn partner_vocabulary_fails_closed() {
    let (c, n, r, seam, s1, s2) = synthetic_rim();
    let tube = rj_cylinder([0.05, 0.0, -0.5], [0.0, 0.0, 1.0], 0.3, 1.0);
    for (fi, f) in tube.faces().iter().enumerate() {
        // The lateral is non-planar; the caps are planar but circle-bounded
        // (ALL-LINE gate) — nothing in this B-Rep is a legal partner.
        let out = circle_edge_plane_face_pierce(c, n, r, seam, s1, s2, fi as u32, f, &tube);
        assert!(out.is_empty(), "face {fi} must fail closed: {out:?}");
    }
}
