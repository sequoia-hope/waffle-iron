//! Exact-tier ground-truth unit tests for `orient3d_indirect`.
//!
//! Every case's expected sign is hand-derived in a comment from the
//! determinant `det[a − d; b − d; c − d]` (Shewchuk convention: Positive
//! = `d` BELOW the CCW plane through `(a, b, c)` — see
//! `specs/cherchi_rs_orient3d_sign.md`).

use super::*;

fn e(x: f64, y: f64, z: f64) -> GenericPoint3D {
    GenericPoint3D::explicit(Point3::new(x, y, z))
}

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// LPI evaluating to `(x, y, 0)`: the vertical line through `(x, y)`
/// intersected with a generic triangle spanning the plane z = 0.
fn lpi_on_z0(x: f64, y: f64) -> GenericPoint3D {
    GenericPoint3D::lpi(
        p3(x, y, -1.0),
        p3(x, y, 1.0),
        // Generic (non-axis-aligned) triangle in z = 0.
        p3(5.0, 0.5, 0.0),
        p3(6.0, 1.5, 0.0),
        p3(4.0, 3.0, 0.0),
    )
}

// ---------------------------------------------------------------------
// Group 1 — known signs with one implicit argument
// ---------------------------------------------------------------------

/// Anchor determinant (all-explicit geometry): a=(0,0,0), b=(1,0,0),
/// c=(0,1,0), d=(0,0,1).
/// rows: a−d=(0,0,−1), b−d=(1,0,−1), c−d=(0,1,−1);
/// det = 0·(0·−1 − (−1)·1) − 0·(1·−1 − (−1)·0) + (−1)·(1·1 − 0·0) = −1.
/// → Negative (d above the CCW plane). The implicit variants below place
/// one or more of these four points as LPI/TPI constructions that
/// evaluate to exactly the same coordinates.
#[test]
fn one_lpi_argument_matches_hand_determinant() {
    let a = lpi_on_z0(0.0, 0.0); // = (0,0,0)
    let b = e(1.0, 0.0, 0.0);
    let c = e(0.0, 1.0, 0.0);
    let d_above = e(0.0, 0.0, 1.0);
    let d_below = e(0.0, 0.0, -1.0);
    assert_eq!(orient3d_indirect(&a, &b, &c, &d_above), Sign::Negative);
    // Mirror case: det flips with d → (0,0,−1) (third row entries −1 → +1).
    assert_eq!(orient3d_indirect(&a, &b, &c, &d_below), Sign::Positive);
}

#[test]
fn lpi_query_point_on_plane_is_zero() {
    // d = (0.25, 0.25, 0) lies exactly in the plane z = 0 of (a, b, c):
    // det[a−d; b−d; c−d] has all-zero third column → 0.
    let a = e(0.0, 0.0, 0.0);
    let b = e(1.0, 0.0, 0.0);
    let c = e(0.0, 1.0, 0.0);
    let d = lpi_on_z0(0.25, 0.25);
    assert_eq!(orient3d_indirect(&a, &b, &c, &d), Sign::Zero);
}

#[test]
fn one_tpi_argument_matches_hand_determinant() {
    // TPI of planes x=1, y=2, z=3 → the point (1, 2, 3).
    let tpi = GenericPoint3D::tpi(
        [p3(1.0, 0.0, 0.0), p3(1.0, 3.0, 0.0), p3(1.0, 0.0, 2.0)], // x = 1
        [p3(0.0, 2.0, 0.0), p3(4.0, 2.0, 0.0), p3(0.0, 2.0, 5.0)], // y = 2
        [p3(0.0, 0.0, 3.0), p3(2.0, 0.0, 3.0), p3(0.0, 2.0, 3.0)], // z = 3
    );
    // (a, b, c) CCW in z = 0 (viewed from +z); d = (1,2,3) is ABOVE → the
    // same column structure as the anchor: det = −1 · (in-plane CCW area
    // term) < 0 → Negative.
    let a = e(0.0, 0.0, 0.0);
    let b = e(1.0, 0.0, 0.0);
    let c = e(0.0, 1.0, 0.0);
    assert_eq!(orient3d_indirect(&a, &b, &c, &tpi), Sign::Negative);

    // d on the plane of (a', b', c') = the z = 3 plane → Zero.
    let a2 = e(0.0, 0.0, 3.0);
    let b2 = e(2.0, 0.0, 3.0);
    let c2 = e(0.0, 2.0, 3.0);
    assert_eq!(orient3d_indirect(&a2, &b2, &c2, &tpi), Sign::Zero);
}

// ---------------------------------------------------------------------
// Group 2 — multiple implicit arguments
// ---------------------------------------------------------------------

#[test]
fn two_lpi_arguments_match_hand_determinant() {
    // lpi(0,0) = (0,0,0) and lpi(1,0) = (1,0,0): same geometry as the
    // anchor determinant → Negative.
    let a = lpi_on_z0(0.0, 0.0);
    let b = lpi_on_z0(1.0, 0.0);
    let c = e(0.0, 1.0, 0.0);
    let d = e(0.0, 0.0, 1.0);
    assert_eq!(orient3d_indirect(&a, &b, &c, &d), Sign::Negative);
}

#[test]
fn all_implicit_arguments_match_hand_determinant() {
    // All four anchor points as LPI constructions; the fourth is the
    // vertical-line LPI through (0,0) with the plane z = 1.
    let a = lpi_on_z0(0.0, 0.0);
    let b = lpi_on_z0(1.0, 0.0);
    let c = lpi_on_z0(0.0, 1.0);
    let d = GenericPoint3D::lpi(
        p3(0.0, 0.0, -1.0),
        p3(0.0, 0.0, 2.0),
        p3(5.0, 0.5, 1.0),
        p3(6.0, 1.5, 1.0),
        p3(4.0, 3.0, 1.0),
    ); // = (0, 0, 1)
    assert_eq!(orient3d_indirect(&a, &b, &c, &d), Sign::Negative);
}

// ---------------------------------------------------------------------
// Group 3 — undefined points (d == 0 exactly; Attene §4.2 / §5.3)
// ---------------------------------------------------------------------

#[test]
fn lpi_line_parallel_to_plane_is_undefined() {
    // Line in z = 1, plane z = 0: never intersects → d == 0 → Undefined.
    let bad = GenericPoint3D::lpi(
        p3(0.0, 0.0, 1.0),
        p3(1.0, 0.0, 1.0),
        p3(0.0, 0.0, 0.0),
        p3(1.0, 0.0, 0.0),
        p3(0.0, 1.0, 0.0),
    );
    let b = e(1.0, 0.0, 0.0);
    let c = e(0.0, 1.0, 0.0);
    let d = e(0.0, 0.0, 1.0);
    assert_eq!(orient3d_indirect(&bad, &b, &c, &d), Sign::Undefined);
}

#[test]
fn lpi_degenerate_line_is_undefined() {
    // p == q: the "line" is a point → d == 0 → Undefined.
    let bad = GenericPoint3D::lpi(
        p3(0.5, 0.5, 0.5),
        p3(0.5, 0.5, 0.5),
        p3(0.0, 0.0, 0.0),
        p3(1.0, 0.0, 0.0),
        p3(0.0, 1.0, 0.0),
    );
    assert_eq!(
        orient3d_indirect(
            &bad,
            &e(1.0, 0.0, 0.0),
            &e(0.0, 1.0, 0.0),
            &e(0.0, 0.0, 1.0)
        ),
        Sign::Undefined
    );
}

#[test]
fn tpi_parallel_planes_is_undefined() {
    // Planes z = 0 and z = 1 are parallel → dT == 0 → Undefined.
    let bad = GenericPoint3D::tpi(
        [p3(0.0, 0.0, 0.0), p3(1.0, 0.0, 0.0), p3(0.0, 1.0, 0.0)], // z = 0
        [p3(0.0, 0.0, 1.0), p3(1.0, 0.0, 1.0), p3(0.0, 1.0, 1.0)], // z = 1
        [p3(0.0, 0.0, 0.0), p3(0.0, 1.0, 0.0), p3(0.0, 0.0, 1.0)], // x = 0
    );
    assert_eq!(
        orient3d_indirect(
            &bad,
            &e(1.0, 0.0, 0.0),
            &e(0.0, 1.0, 0.0),
            &e(0.0, 0.0, 1.0)
        ),
        Sign::Undefined
    );
}

// ---------------------------------------------------------------------
// Group 4 — invariances: generator order, argument permutation
// ---------------------------------------------------------------------

#[test]
fn lpi_generator_swap_same_point_same_sign() {
    // Swapping the line endpoints (p, q) negates BOTH d and (with the
    // lambda rewrite) the λs — the geometric point is identical, and the
    // denominator-sign parity rule (Attene §5.1) must absorb the flip.
    let fwd = lpi_on_z0(0.25, 0.5);
    let rev = GenericPoint3D::lpi(
        p3(0.25, 0.5, 1.0),
        p3(0.25, 0.5, -1.0),
        p3(5.0, 0.5, 0.0),
        p3(6.0, 1.5, 0.0),
        p3(4.0, 3.0, 0.0),
    );
    let b = e(1.0, 0.0, 0.0);
    let c = e(0.0, 1.0, 0.0);
    let d = e(0.0, 0.0, 1.0);
    let s_fwd = orient3d_indirect(&fwd, &b, &c, &d);
    let s_rev = orient3d_indirect(&rev, &b, &c, &d);
    assert_eq!(s_fwd, Sign::Negative);
    assert_eq!(s_rev, Sign::Negative);
}

#[test]
fn argument_permutation_antisymmetry() {
    // orient3d is alternating in all four arguments: a single transposition
    // flips the sign (Attene §6). Exercises canonicalization parity with
    // mixed explicit/implicit arguments.
    let a = lpi_on_z0(0.0, 0.0);
    let b = e(1.0, 0.0, 0.0);
    let c = e(0.0, 1.0, 0.0);
    let d = e(0.0, 0.0, 1.0);
    let base = orient3d_indirect(&a, &b, &c, &d);
    assert_eq!(base, Sign::Negative);
    assert_eq!(orient3d_indirect(&b, &a, &c, &d), base.flipped());
    assert_eq!(orient3d_indirect(&a, &c, &b, &d), base.flipped());
    assert_eq!(orient3d_indirect(&a, &b, &d, &c), base.flipped());
    assert_eq!(orient3d_indirect(&d, &b, &c, &a), base.flipped());
    // Even permutation (two transpositions): same sign.
    assert_eq!(orient3d_indirect(&b, &a, &d, &c), base);
}

#[test]
fn all_explicit_delegates_to_cr6_orient3d() {
    // EEEE must agree with crate::predicates::orient3d on a grid sample.
    for i in 0..3 {
        for j in 0..3 {
            let pa = p3(0.0, 0.0, 0.0);
            let pb = p3(1.0, 0.0, i as f64 - 1.0);
            let pc = p3(0.0, 1.0, j as f64 - 1.0);
            let pd = p3(0.3, 0.4, 0.5);
            let direct: Sign = crate::predicates::orient3d(pa, pb, pc, pd).into();
            let indirect = orient3d_indirect(
                &GenericPoint3D::explicit(pa),
                &GenericPoint3D::explicit(pb),
                &GenericPoint3D::explicit(pc),
                &GenericPoint3D::explicit(pd),
            );
            assert_eq!(direct, indirect, "EEEE mismatch at ({i}, {j})");
        }
    }
}

// ---------------------------------------------------------------------
// Group 5 — exact lambda cross-checks vs our AR3c Cramer machinery
// (gated: `exact_point_coords` lives in the feature-gated aux_structure)
// ---------------------------------------------------------------------

#[cfg(feature = "indirect-predicates")]
mod ar3c_cross_check {
    use super::*;
    use crate::arrangements::aux_structure::exact_point_coords;
    use crate::arrangements::fast_trimesh::VertexCoords;

    /// λ/d from the generated TPI lambdas must equal the coordinates the
    /// AR3c exact Cramer solve produces for the same generators.
    #[test]
    fn tpi_lambda_matches_ar3c_exact_point_coords() {
        let configs: [([Point3; 3], [Point3; 3], [Point3; 3]); 3] = [
            (
                // Axis planes offset: meet at (1, 2, 3).
                [p3(1.0, 0.0, 0.0), p3(1.0, 3.0, 0.0), p3(1.0, 0.0, 2.0)],
                [p3(0.0, 2.0, 0.0), p3(4.0, 2.0, 0.0), p3(0.0, 2.0, 5.0)],
                [p3(0.0, 0.0, 3.0), p3(2.0, 0.0, 3.0), p3(0.0, 2.0, 3.0)],
            ),
            (
                // Tilted planes, generic intersection.
                [p3(0.0, 0.0, 0.0), p3(2.0, 0.5, 0.0), p3(0.0, 1.0, 1.5)],
                [p3(1.0, 0.0, 0.0), p3(1.0, 2.0, 0.25), p3(0.0, 0.0, 3.0)],
                [p3(0.0, 1.0, 0.0), p3(2.0, 1.0, 0.5), p3(0.5, 3.0, 1.0)],
            ),
            (
                // Dyadic-fraction coordinates.
                [p3(0.25, 0.0, 0.0), p3(0.25, 1.0, 0.0), p3(0.25, 0.0, 1.0)],
                [p3(0.0, 0.5, 0.0), p3(1.0, 0.5, 0.0), p3(0.0, 0.5, 1.0)],
                [p3(0.0, 0.0, 0.125), p3(1.0, 0.0, 0.125), p3(0.0, 1.0, 0.125)],
            ),
        ];
        for (idx, (v, w, u)) in configs.iter().enumerate() {
            let gp = GenericPoint3D::tpi(*v, *w, *u);
            let lam = gp.lambda_exact();
            assert!(!lam.is_undefined(), "config {idx}: unexpected dT == 0");
            let coords = exact_point_coords(&VertexCoords::Tpi {
                v: *v,
                w: *w,
                u: *u,
            })
            .expect("config {idx}: AR3c solve must succeed");
            for axis in 0..3 {
                let ours = &lam.l[axis] / &lam.d;
                assert_eq!(
                    ours, coords[axis],
                    "config {idx}: TPI λ{axis}/d disagrees with AR3c Cramer solve"
                );
            }
        }
    }

    /// Same cross-check for LPI generators.
    #[test]
    fn lpi_lambda_matches_ar3c_exact_point_coords() {
        let line = [p3(0.1, 0.2, -1.0), p3(0.4, 0.3, 2.0)];
        let plane = [p3(5.0, 0.5, 0.5), p3(6.0, 1.5, 0.25), p3(4.0, 3.0, 0.75)];
        let gp = GenericPoint3D::lpi(line[0], line[1], plane[0], plane[1], plane[2]);
        let lam = gp.lambda_exact();
        assert!(!lam.is_undefined());
        let coords = exact_point_coords(&VertexCoords::Lpi { line, plane })
            .expect("AR3c LPI solve must succeed");
        for axis in 0..3 {
            let ours = &lam.l[axis] / &lam.d;
            assert_eq!(
                ours, coords[axis],
                "LPI λ{axis}/d disagrees with AR3c Cramer solve"
            );
        }
    }
}
