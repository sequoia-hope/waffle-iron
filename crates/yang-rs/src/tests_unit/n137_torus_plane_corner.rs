//! #137 N-137.1 — exact grazing-CORNER junction primitive
//! (`stage4_relocate::torus_plane_clip_junction`, spec
//! `specs/yang_137_torus_plane_grazing_corner.md`).
//!
//! The de-risking foundation for the torus∩plane grazing-loop fix: PROVE that the
//! existing 3-surface Newton (`relocate_onto_implicit_triple`, until now only
//! exercised on conic junctions) converges onto a torus∩plane∩plane corner, and
//! that the validated wrapper pins the EXACT junction. Fixture = C0065: torus
//! center [0,0,0.5] axis z, R=1.2 r=0.3; cutting plane x=1.45; clip plane y=-0.25.
//! The true loop reaches |y|=0.384 (outside the box), crossing y=-0.25 at
//! z = 0.5 ± sqrt(r^2 - (radial - R)^2) with radial = sqrt(1.45^2 + 0.25^2)
//! = 1.47139..., i.e. z = 0.62771... and z = 0.37228...

#[allow(unused_imports)]
use super::*;
use crate::stage4_relocate::torus_plane_clip_junction;
use crate::surface_value_and_normal;

/// C0065 torus.
fn c0065_torus() -> Surface {
    Surface::Torus {
        center: Point3::new(0.0, 0.0, 0.5),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        major_radius: 1.2,
        minor_radius: 0.3,
    }
}

/// Cutting face x = 1.45 (F = x - 1.45; the box notch's +x wall, as reported by
/// `YANG_TORUS_PROBE`: `Plane { normal: [1,0,0], d: -1.45 }`).
fn plane_x_1_45() -> Surface {
    Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -1.45,
    }
}

/// Clip face y = -0.25 (F = -y - 0.25; probe: `Plane { normal: [0,-1,0], d: -0.25 }`).
fn plane_y_neg_0_25() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, -1.0, 0.0),
        d: -0.25,
    }
}

/// The two closed-form junctions (z = 0.5 ± 0.127713...).
fn expected_upper() -> [f64; 3] {
    let radial = (1.45f64 * 1.45 + 0.25 * 0.25).sqrt();
    let dz = (0.3f64 * 0.3 - (radial - 1.2).powi(2)).sqrt();
    [1.45, -0.25, 0.5 + dz]
}
fn expected_lower() -> [f64; 3] {
    let u = expected_upper();
    [1.45, -0.25, 1.0 - u[2]]
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// GREEN: a seed near the UPPER crossing converges to the exact junction, on all
/// three surfaces to sub-nano precision.
#[test]
fn corner_junction_upper_pins_exact_triple() {
    // A mesh-ish seed: near the crossing but off it (as a coarse loop vertex would be).
    let seed = Point3::new(1.45, -0.23, 0.60);
    let j = torus_plane_clip_junction(seed, c0065_torus(), plane_x_1_45(), plane_y_neg_0_25())
        .expect("torus∩plane∩plane junction converges");
    let e = expected_upper();
    assert!(
        dist(j.as_array(), e) < 1e-9,
        "junction {:?} != expected upper {:?}",
        j.as_array(),
        e
    );
    // On all three surfaces.
    for s in [c0065_torus(), plane_x_1_45(), plane_y_neg_0_25()] {
        let (f, _) = surface_value_and_normal(s, j.as_array()).unwrap();
        assert!(f.abs() < 1e-9, "off-surface residual {f:e}");
    }
}

/// GREEN: a seed near the LOWER crossing pins the other junction (the loop crosses
/// y=-0.25 twice — both must be recoverable).
#[test]
fn corner_junction_lower_pins_exact_triple() {
    let seed = Point3::new(1.45, -0.23, 0.40);
    let j = torus_plane_clip_junction(seed, c0065_torus(), plane_x_1_45(), plane_y_neg_0_25())
        .expect("lower junction converges");
    assert!(dist(j.as_array(), expected_lower()) < 1e-9);
}

/// The seed-selects-the-branch invariant: the two seeds land on DIFFERENT
/// junctions (z separated by 2*dz ≈ 0.255), so a caller can recover both loop
/// crossings by seeding from each mesh crossing vertex.
#[test]
fn corner_junctions_are_distinct_branches() {
    let up = torus_plane_clip_junction(
        Point3::new(1.45, -0.23, 0.60),
        c0065_torus(),
        plane_x_1_45(),
        plane_y_neg_0_25(),
    )
    .unwrap();
    let lo = torus_plane_clip_junction(
        Point3::new(1.45, -0.23, 0.40),
        c0065_torus(),
        plane_x_1_45(),
        plane_y_neg_0_25(),
    )
    .unwrap();
    assert!((up.as_array()[2] - lo.as_array()[2]).abs() > 0.2);
}

/// LOUD STOP: a clip plane that does NOT actually cut the torus∩cutting_plane loop
/// (y = -0.9, well beyond the loop's |y|<=0.384 extent) has no real triple root —
/// the primitive returns None (never a spurious/off-surface junction). This is the
/// guard that keeps the eventual assembly from inventing a corner where the loop
/// stays inside the face.
#[test]
fn no_junction_when_clip_plane_misses_the_loop() {
    let far_clip = Surface::Plane {
        normal: Vector3::new(0.0, -1.0, 0.0),
        d: -0.9,
    };
    let j = torus_plane_clip_junction(
        Point3::new(1.45, -0.5, 0.5),
        c0065_torus(),
        plane_x_1_45(),
        far_clip,
    );
    assert!(
        j.is_none(),
        "clip plane beyond the loop extent must yield no junction, got {j:?}"
    );
}

/// LOUD STOP: a cutting plane BEYOND the outer equator (x = 1.55 > 1.5) does not
/// meet the torus at all — no junction.
#[test]
fn no_junction_when_cutting_plane_misses_the_torus() {
    let far_cut = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -1.55,
    };
    let j = torus_plane_clip_junction(
        Point3::new(1.55, -0.25, 0.5),
        c0065_torus(),
        far_cut,
        plane_y_neg_0_25(),
    );
    assert!(
        j.is_none(),
        "cutting plane past the equator must yield no junction"
    );
}
