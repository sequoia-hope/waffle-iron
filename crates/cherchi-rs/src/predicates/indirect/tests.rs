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
                [
                    p3(0.0, 0.0, 0.125),
                    p3(1.0, 0.0, 0.125),
                    p3(0.0, 1.0, 0.125),
                ],
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
            for (axis, coord) in coords.iter().enumerate() {
                let ours = &lam.l[axis] / &lam.d;
                assert_eq!(
                    &ours, coord,
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
        for (axis, coord) in coords.iter().enumerate() {
            let ours = &lam.l[axis] / &lam.d;
            assert_eq!(
                &ours, coord,
                "LPI λ{axis}/d disagrees with AR3c Cramer solve"
            );
        }
    }
}

// =====================================================================
// PR-CR-M7b — hand-truth unit tests for the catalog families
// =====================================================================

/// TPI evaluating to exactly `(1, 2, 3)` (axis planes x=1, y=2, z=3).
fn tpi_123() -> GenericPoint3D {
    GenericPoint3D::tpi(
        [p3(1.0, 0.0, 0.0), p3(1.0, 3.0, 0.0), p3(1.0, 0.0, 2.0)], // x = 1
        [p3(0.0, 2.0, 0.0), p3(4.0, 2.0, 0.0), p3(0.0, 2.0, 5.0)], // y = 2
        [p3(0.0, 0.0, 3.0), p3(2.0, 0.0, 3.0), p3(0.0, 2.0, 3.0)], // z = 3
    )
}

/// LPI with the line parallel to the plane: `d == 0` → undefined point.
fn lpi_undefined() -> GenericPoint3D {
    GenericPoint3D::lpi(
        p3(0.0, 0.0, 1.0),
        p3(1.0, 0.0, 1.0),
        p3(0.0, 0.0, 0.0),
        p3(1.0, 0.0, 0.0),
        p3(0.0, 1.0, 0.0),
    )
}

// ---------------------------------------------------------------------
// Group 6 — orient2d projections
// ---------------------------------------------------------------------

#[test]
fn orient2d_xy_explicit_ccw_cw_collinear() {
    // Γ_xy(a)=(0,0), Γ_xy(b)=(1,0), Γ_xy(c)=(0,1): det[b−a; c−a] =
    // 1·1 − 0·0 = 1 → Positive (CCW). z coords are arbitrary (dropped).
    let a = e(0.0, 0.0, 7.0);
    let b = e(1.0, 0.0, -3.0);
    let c = e(0.0, 1.0, 11.0);
    assert_eq!(orient2d_xy_indirect(&a, &b, &c), Sign::Positive);
    // Swap the last two args → CW → Negative (alternating).
    assert_eq!(orient2d_xy_indirect(&a, &c, &b), Sign::Negative);
    // Collinear along x: (0,0), (1,0), (2,0) → Zero.
    let d = e(2.0, 0.0, 5.0);
    assert_eq!(orient2d_xy_indirect(&a, &b, &d), Sign::Zero);
}

#[test]
fn orient2d_yz_zx_explicit_anchor() {
    // yz: Γ_yz(p) = (p_y, p_z). (0,0), (1,0), (0,1) in (y,z) → Positive.
    let a = e(7.0, 0.0, 0.0);
    let b = e(-3.0, 1.0, 0.0);
    let c = e(11.0, 0.0, 1.0);
    assert_eq!(orient2d_yz_indirect(&a, &b, &c), Sign::Positive);
    assert_eq!(orient2d_yz_indirect(&a, &c, &b), Sign::Negative);
    // zx: Γ_zx(p) = (p_z, p_x). (0,0), (1,0), (0,1) in (z,x) → Positive.
    let a = e(0.0, 7.0, 0.0);
    let b = e(0.0, -3.0, 1.0);
    let c = e(1.0, 11.0, 0.0);
    assert_eq!(orient2d_zx_indirect(&a, &b, &c), Sign::Positive);
    assert_eq!(orient2d_zx_indirect(&a, &c, &b), Sign::Negative);
}

#[test]
fn orient2d_xy_one_lpi_argument() {
    // lpi_on_z0(2, 5) = (2, 5, 0). Γ_xy = (2, 5).
    let p = lpi_on_z0(2.0, 5.0);
    // Collinear with (0,5) and (4,5) in xy (all on the line y = 5).
    assert_eq!(
        orient2d_xy_indirect(&p, &e(0.0, 5.0, 0.0), &e(4.0, 5.0, 0.0)),
        Sign::Zero
    );
    // det[b−a; p−a] with a=(0,0), b=(1,0), p=(2,5): 1·5 − 0·2 = 5 → the
    // LPI in the 3rd slot is strictly left of a→b → Positive.
    assert_eq!(
        orient2d_xy_indirect(&e(0.0, 0.0, 0.0), &e(1.0, 0.0, 0.0), &p),
        Sign::Positive
    );
    // Mirror through y = 0: lpi at (2, -5) → Negative.
    let m = lpi_on_z0(2.0, -5.0);
    assert_eq!(
        orient2d_xy_indirect(&e(0.0, 0.0, 0.0), &e(1.0, 0.0, 0.0), &m),
        Sign::Negative
    );
}

#[test]
fn orient2d_xy_tpi_argument_all_slots() {
    // tpi_123 = (1, 2, 3): Γ_xy = (1, 2). det[(1,0); (1,2)] = 2 → Positive.
    let t = tpi_123();
    let a = e(0.0, 0.0, 0.0);
    let b = e(1.0, 0.0, 0.0);
    assert_eq!(orient2d_xy_indirect(&a, &b, &t), Sign::Positive);
    // Cyclic rotations preserve the sign (even permutations).
    assert_eq!(orient2d_xy_indirect(&t, &a, &b), Sign::Positive);
    assert_eq!(orient2d_xy_indirect(&b, &t, &a), Sign::Positive);
    // One transposition flips.
    assert_eq!(orient2d_xy_indirect(&b, &a, &t), Sign::Negative);
}

#[test]
fn orient2d_mixed_implicit_collinear_is_zero() {
    // Three implicit points on the xy-line y = x: lpi(k, k) = (k, k, 0).
    let p1 = lpi_on_z0(1.0, 1.0);
    let p2 = lpi_on_z0(2.0, 2.0);
    let p3v = tpi_123(); // (1, 2, 3) — NOT on the line.
    assert_eq!(
        orient2d_xy_indirect(&p1, &p2, &lpi_on_z0(3.0, 3.0)),
        Sign::Zero
    );
    // (1,1), (2,2), (1,2): det[(1,1); (0,1)] = 1 → Positive.
    assert_eq!(orient2d_xy_indirect(&p1, &p2, &p3v), Sign::Positive);
}

#[test]
fn orient2d_undefined_point_is_undefined() {
    let bad = lpi_undefined();
    assert_eq!(
        orient2d_xy_indirect(&bad, &e(1.0, 0.0, 0.0), &e(0.0, 1.0, 0.0)),
        Sign::Undefined
    );
    assert_eq!(
        orient2d_yz_indirect(&e(1.0, 0.0, 0.0), &bad, &e(0.0, 1.0, 0.0)),
        Sign::Undefined
    );
    assert_eq!(
        orient2d_zx_indirect(&e(1.0, 0.0, 0.0), &e(0.0, 1.0, 0.0), &bad),
        Sign::Undefined
    );
}

// ---------------------------------------------------------------------
// Group 7 — less_than_on_{x,y,z}
// ---------------------------------------------------------------------

#[test]
fn less_than_explicit_pairs() {
    // sign(a.c − b.c): a < b → Negative; equal → Zero; a > b → Positive.
    let a = e(0.0, 5.0, 9.0);
    let b = e(1.0, 5.0, 2.0);
    assert_eq!(less_than_on_x_indirect(&a, &b), Sign::Negative);
    assert_eq!(less_than_on_x_indirect(&b, &a), Sign::Positive);
    assert_eq!(less_than_on_y_indirect(&a, &b), Sign::Zero);
    assert_eq!(less_than_on_z_indirect(&a, &b), Sign::Positive);
}

#[test]
fn less_than_lpi_vs_explicit() {
    // lpi_on_z0(0.25, 0.5) = (0.25, 0.5, 0).
    let l = lpi_on_z0(0.25, 0.5);
    assert_eq!(
        less_than_on_x_indirect(&l, &e(0.5, 0.0, 0.0)),
        Sign::Negative
    );
    assert_eq!(
        less_than_on_x_indirect(&e(0.5, 0.0, 0.0), &l),
        Sign::Positive
    );
    // Exact tie on x.
    assert_eq!(less_than_on_x_indirect(&l, &e(0.25, 7.0, 3.0)), Sign::Zero);
    // y comparator: 0.5 vs 0.5 → Zero; z comparator: 0 vs −1 → Positive.
    assert_eq!(less_than_on_y_indirect(&l, &e(9.0, 0.5, 0.0)), Sign::Zero);
    assert_eq!(
        less_than_on_z_indirect(&l, &e(0.0, 0.0, -1.0)),
        Sign::Positive
    );
}

#[test]
fn less_than_implicit_pairs() {
    // LPI–LPI: (0.25, 0.5, 0) vs (1, 1, 0).
    let l1 = lpi_on_z0(0.25, 0.5);
    let l2 = lpi_on_z0(1.0, 1.0);
    assert_eq!(less_than_on_x_indirect(&l1, &l2), Sign::Negative);
    assert_eq!(less_than_on_x_indirect(&l2, &l1), Sign::Positive);
    // Exact geometric tie via DIFFERENT generators: lpi(1, 2) = (1, 2, 0)
    // shares x with tpi_123 = (1, 2, 3).
    let t = tpi_123();
    assert_eq!(
        less_than_on_x_indirect(&lpi_on_z0(1.0, 2.0), &t),
        Sign::Zero
    );
    assert_eq!(
        less_than_on_y_indirect(&lpi_on_z0(1.0, 2.0), &t),
        Sign::Zero
    );
    // z: 0 < 3 → Negative.
    assert_eq!(
        less_than_on_z_indirect(&lpi_on_z0(1.0, 2.0), &t),
        Sign::Negative
    );
    // TPI–TPI tie: identical geometric point from the same generators.
    assert_eq!(less_than_on_x_indirect(&t, &tpi_123()), Sign::Zero);
}

#[test]
fn less_than_undefined_point_is_undefined() {
    let bad = lpi_undefined();
    assert_eq!(
        less_than_on_x_indirect(&bad, &e(0.0, 0.0, 0.0)),
        Sign::Undefined
    );
    assert_eq!(
        less_than_on_y_indirect(&e(0.0, 0.0, 0.0), &bad),
        Sign::Undefined
    );
}

// ---------------------------------------------------------------------
// Group 8 — point_in_triangle (closed containment, coplanar query)
// ---------------------------------------------------------------------

#[test]
fn point_in_triangle_explicit_z0() {
    // Triangle (0,0,0), (4,0,0), (0,4,0) in z = 0.
    let a = e(0.0, 0.0, 0.0);
    let b = e(4.0, 0.0, 0.0);
    let c = e(0.0, 4.0, 0.0);
    // Strictly inside.
    assert!(point_in_triangle_indirect(&e(1.0, 1.0, 0.0), &a, &b, &c));
    // On a vertex / on an edge midpoint (boundary-inclusive).
    assert!(point_in_triangle_indirect(&e(0.0, 0.0, 0.0), &a, &b, &c));
    assert!(point_in_triangle_indirect(&e(2.0, 0.0, 0.0), &a, &b, &c));
    // On the hypotenuse: (2, 2, 0) — x + y = 4.
    assert!(point_in_triangle_indirect(&e(2.0, 2.0, 0.0), &a, &b, &c));
    // Strictly outside (beyond the hypotenuse).
    assert!(!point_in_triangle_indirect(&e(3.0, 3.0, 0.0), &a, &b, &c));
    // Outside along an edge extension (collinear with edge AB).
    assert!(!point_in_triangle_indirect(&e(5.0, 0.0, 0.0), &a, &b, &c));
    // Orientation of the triangle must not matter (CW corner order).
    assert!(point_in_triangle_indirect(&e(1.0, 1.0, 0.0), &a, &c, &b));
    assert!(!point_in_triangle_indirect(&e(3.0, 3.0, 0.0), &a, &c, &b));
}

#[test]
fn point_in_triangle_lpi_query() {
    let a = e(0.0, 0.0, 0.0);
    let b = e(4.0, 0.0, 0.0);
    let c = e(0.0, 4.0, 0.0);
    // lpi_on_z0(1, 1) = (1, 1, 0): strictly inside.
    assert!(point_in_triangle_indirect(&lpi_on_z0(1.0, 1.0), &a, &b, &c));
    // lpi_on_z0(2, 0) = on edge AB.
    assert!(point_in_triangle_indirect(&lpi_on_z0(2.0, 0.0), &a, &b, &c));
    // lpi_on_z0(3, 3): strictly outside.
    assert!(!point_in_triangle_indirect(
        &lpi_on_z0(3.0, 3.0),
        &a,
        &b,
        &c
    ));
}

#[test]
fn point_in_triangle_xy_degenerate_projection_falls_through() {
    // Triangle in the plane y = 0: its xy projection is collinear
    // (degenerate), so the composite must fall through to a usable
    // projection (zx works: (z,x) corners (0,0), (0,2), (2,0)).
    let a = e(0.0, 0.0, 0.0);
    let b = e(2.0, 0.0, 0.0);
    let c = e(0.0, 0.0, 2.0);
    assert!(point_in_triangle_indirect(&e(0.5, 0.0, 0.5), &a, &b, &c));
    assert!(point_in_triangle_indirect(&e(1.0, 0.0, 0.0), &a, &b, &c)); // edge
    assert!(!point_in_triangle_indirect(&e(2.0, 0.0, 2.0), &a, &b, &c));
}

#[test]
fn point_in_triangle_tilted_plane() {
    // Triangle (1,0,0), (0,1,0), (0,0,1) in the plane x + y + z = 1.
    let a = e(1.0, 0.0, 0.0);
    let b = e(0.0, 1.0, 0.0);
    let c = e(0.0, 0.0, 1.0);
    // (0.5, 0.25, 0.25): barycentric coords (0.5, 0.25, 0.25) → inside.
    assert!(point_in_triangle_indirect(&e(0.5, 0.25, 0.25), &a, &b, &c));
    // (0.5, 0.5, 0): on edge AB.
    assert!(point_in_triangle_indirect(&e(0.5, 0.5, 0.0), &a, &b, &c));
    // (1.5, −0.25, −0.25): on the plane (sum = 1) but outside.
    assert!(!point_in_triangle_indirect(
        &e(1.5, -0.25, -0.25),
        &a,
        &b,
        &c
    ));
}

#[test]
fn point_in_triangle_degenerate_or_undefined_is_false() {
    // Degenerate (collinear) triangle → false even for an on-segment p.
    let a = e(0.0, 0.0, 0.0);
    let b = e(1.0, 0.0, 0.0);
    let c = e(2.0, 0.0, 0.0);
    assert!(!point_in_triangle_indirect(&e(0.5, 0.0, 0.0), &a, &b, &c));
    // Undefined implicit query point → false.
    let t = e(0.0, 4.0, 0.0);
    assert!(!point_in_triangle_indirect(
        &lpi_undefined(),
        &a,
        &e(4.0, 0.0, 0.0),
        &t
    ));
}

// ---------------------------------------------------------------------
// Group 9 — inner_segments_cross
// ---------------------------------------------------------------------

#[test]
fn inner_segments_cross_proper_x() {
    // (0,0)→(2,2) × (0,2)→(2,0) cross at (1,1), interior to both.
    let a = e(0.0, 0.0, 0.0);
    let b = e(2.0, 2.0, 0.0);
    let p = e(0.0, 2.0, 0.0);
    let q = e(2.0, 0.0, 0.0);
    assert!(inner_segments_cross_indirect(&a, &b, &p, &q));
    // Argument order within each segment must not matter.
    assert!(inner_segments_cross_indirect(&b, &a, &q, &p));
    // Segment pair order must not matter.
    assert!(inner_segments_cross_indirect(&p, &q, &a, &b));
}

#[test]
fn inner_segments_cross_rejects_shared_endpoint_and_t_touch() {
    let a = e(0.0, 0.0, 0.0);
    let b = e(2.0, 0.0, 0.0);
    // Shared endpoint: (0,0)→(2,0) and (0,0)→(0,2) meet AT a, not inside.
    assert!(!inner_segments_cross_indirect(
        &a,
        &b,
        &a,
        &e(0.0, 2.0, 0.0)
    ));
    // T-touch: (1,0) is interior to (a,b) but an ENDPOINT of (p,q).
    assert!(!inner_segments_cross_indirect(
        &a,
        &b,
        &e(1.0, 0.0, 0.0),
        &e(1.0, 2.0, 0.0)
    ));
}

#[test]
fn inner_segments_cross_rejects_collinear_overlap_and_disjoint() {
    let a = e(0.0, 0.0, 0.0);
    let b = e(2.0, 0.0, 0.0);
    // Collinear overlap: (1,0)→(3,0) overlaps (0,0)→(2,0) — no PROPER cross.
    assert!(!inner_segments_cross_indirect(
        &a,
        &b,
        &e(1.0, 0.0, 0.0),
        &e(3.0, 0.0, 0.0)
    ));
    // Parallel disjoint.
    assert!(!inner_segments_cross_indirect(
        &a,
        &b,
        &e(0.0, 1.0, 0.0),
        &e(2.0, 1.0, 0.0)
    ));
    // Crossing LINES but the intersection is outside one segment.
    assert!(!inner_segments_cross_indirect(
        &a,
        &b,
        &e(3.0, -1.0, 0.0),
        &e(3.0, 1.0, 0.0)
    ));
}

#[test]
fn inner_segments_cross_in_xy_collapsed_plane() {
    // Both segments in the plane y = 0 (xy projection collapses it to a
    // line): cross at (1, 0, 1).
    let a = e(0.0, 0.0, 0.0);
    let b = e(2.0, 0.0, 2.0);
    let p = e(0.0, 0.0, 2.0);
    let q = e(2.0, 0.0, 0.0);
    assert!(inner_segments_cross_indirect(&a, &b, &p, &q));
    assert!(!inner_segments_cross_indirect(
        &a,
        &b,
        &p,
        &e(0.0, 0.0, 4.0)
    ));
}

#[test]
fn inner_segments_cross_with_implicit_endpoints() {
    // Same proper-X as above with three endpoints implicit:
    // lpi_on_z0(k1,k2) = (k1, k2, 0).
    let a = lpi_on_z0(0.0, 0.0);
    let b = lpi_on_z0(2.0, 2.0);
    let p = lpi_on_z0(0.0, 2.0);
    let q = e(2.0, 0.0, 0.0);
    assert!(inner_segments_cross_indirect(&a, &b, &p, &q));
    // Undefined endpoint → false.
    assert!(!inner_segments_cross_indirect(&lpi_undefined(), &b, &p, &q));
}

// ---------------------------------------------------------------------
// Group 10 — point_in_inner_segment / point_in_segment
// ---------------------------------------------------------------------

#[test]
fn point_in_inner_segment_explicit() {
    let v1 = e(0.0, 0.0, 0.0);
    let v2 = e(4.0, 2.0, 6.0);
    let mid = e(2.0, 1.0, 3.0);
    // Strictly inside, symmetric in endpoint order.
    assert!(point_in_inner_segment_indirect(&mid, &v1, &v2));
    assert!(point_in_inner_segment_indirect(&mid, &v2, &v1));
    // Endpoints are EXCLUDED.
    assert!(!point_in_inner_segment_indirect(&v1, &v1, &v2));
    assert!(!point_in_inner_segment_indirect(&v2, &v1, &v2));
    // Collinear but beyond.
    assert!(!point_in_inner_segment_indirect(
        &e(6.0, 3.0, 9.0),
        &v1,
        &v2
    ));
    assert!(!point_in_inner_segment_indirect(
        &e(-2.0, -1.0, -3.0),
        &v1,
        &v2
    ));
    // Off the line.
    assert!(!point_in_inner_segment_indirect(
        &e(2.0, 1.0, 4.0),
        &v1,
        &v2
    ));
}

#[test]
fn point_in_inner_segment_axis_aligned_and_implicit() {
    // Segment along z only (x and y comparators are ties — the separating
    // axis search must reach z).
    let v1 = e(1.0, 2.0, 0.0);
    let v2 = e(1.0, 2.0, 8.0);
    assert!(point_in_inner_segment_indirect(&e(1.0, 2.0, 3.0), &v1, &v2));
    assert!(!point_in_inner_segment_indirect(
        &e(1.0, 2.0, 9.0),
        &v1,
        &v2
    ));
    // Implicit p strictly inside an explicit segment: lpi(1,1) = (1,1,0)
    // on the segment (0,0,0)→(2,2,0).
    assert!(point_in_inner_segment_indirect(
        &lpi_on_z0(1.0, 1.0),
        &e(0.0, 0.0, 0.0),
        &e(2.0, 2.0, 0.0)
    ));
    // Implicit ENDPOINT tie: p geometrically equal to v1 (different
    // generators) is excluded.
    assert!(!point_in_inner_segment_indirect(
        &lpi_on_z0(0.0, 0.0),
        &e(0.0, 0.0, 0.0),
        &e(2.0, 2.0, 0.0)
    ));
    // Undefined p → false.
    assert!(!point_in_inner_segment_indirect(
        &lpi_undefined(),
        &e(0.0, 0.0, 0.0),
        &e(2.0, 2.0, 0.0)
    ));
}

#[test]
fn point_in_segment_closed_semantics() {
    let v1 = e(0.0, 0.0, 0.0);
    let v2 = e(4.0, 2.0, 6.0);
    // Midpoint AND endpoints included.
    assert!(point_in_segment_indirect(&e(2.0, 1.0, 3.0), &v1, &v2));
    assert!(point_in_segment_indirect(&v1, &v1, &v2));
    assert!(point_in_segment_indirect(&v2, &v1, &v2));
    assert!(point_in_segment_indirect(&v2, &v2, &v1));
    // Beyond / off-line excluded.
    assert!(!point_in_segment_indirect(&e(6.0, 3.0, 9.0), &v1, &v2));
    assert!(!point_in_segment_indirect(&e(2.0, 1.0, 4.0), &v1, &v2));
    // Degenerate segment [v, v] contains exactly v.
    assert!(point_in_segment_indirect(&v1, &v1, &v1));
    assert!(!point_in_segment_indirect(&e(1.0, 0.0, 0.0), &v1, &v1));
    // Implicit endpoint-tie IS included (closed).
    assert!(point_in_segment_indirect(
        &lpi_on_z0(0.0, 0.0),
        &e(0.0, 0.0, 0.0),
        &e(2.0, 2.0, 0.0)
    ));
}

// ---------------------------------------------------------------------
// Group 11 — approx_lpi
// ---------------------------------------------------------------------

#[test]
fn approx_lpi_exact_vertical_case() {
    // Vertical line through (1, 1) × plane z = 0 → exactly (1, 1, 0); all
    // intermediate quantities are small integers, so the interval
    // midpoints are exact and the ratio is exactly representable.
    let got = approx_lpi(
        p3(1.0, 1.0, 5.0),
        p3(1.0, 1.0, -5.0),
        p3(0.0, 0.0, 0.0),
        p3(1.0, 0.0, 0.0),
        p3(0.0, 1.0, 0.0),
    )
    .expect("non-degenerate LPI must produce an approximation");
    assert_eq!((got.x(), got.y(), got.z()), (1.0, 1.0, 0.0));
}

#[test]
fn approx_lpi_generic_case_near_true_point() {
    // Line (0,0,−1)→(1,1,1) crosses z = 0 at t = 1/2 → (0.5, 0.5, 0).
    let got = approx_lpi(
        p3(0.0, 0.0, -1.0),
        p3(1.0, 1.0, 1.0),
        p3(5.0, 0.5, 0.0),
        p3(6.0, 1.5, 0.0),
        p3(4.0, 3.0, 0.0),
    )
    .expect("non-degenerate LPI must produce an approximation");
    assert!((got.x() - 0.5).abs() < 1e-12);
    assert!((got.y() - 0.5).abs() < 1e-12);
    assert!(got.z().abs() < 1e-12);
}

#[test]
fn approx_lpi_degenerate_returns_none() {
    // Line parallel to the plane: d == 0 → None (caller's fallback).
    assert_eq!(
        approx_lpi(
            p3(0.0, 0.0, 1.0),
            p3(1.0, 0.0, 1.0),
            p3(0.0, 0.0, 0.0),
            p3(1.0, 0.0, 0.0),
            p3(0.0, 1.0, 0.0)
        ),
        None
    );
}
