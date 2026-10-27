//! Piegl & Tiller B-spline / NURBS verification oracles for `nurbs-rs`
//! (`specs/loft.md` §5 L1a).
//!
//! Line citations refer to `refs/text/piegl_tiller1997_nurbs_book.txt`.

use std::f64::consts::FRAC_1_SQRT_2;

use cad_primitives::{Point3, Vector3};
use nurbs_rs::bspline::{
    averaged_knots, basis_funs, chord_length_params, ders_basis_funs, find_span,
    global_curve_interp, global_curve_interp_periodic, periodic_chord_length_params,
    rat_curve_derivs, rat_surface_derivs, BSplineCurve, BSplineError, BSplineSurface,
};

fn dist3(a: Point3, b: Point3) -> f64 {
    let dx = a.x() - b.x();
    let dy = a.y() - b.y();
    let dz = a.z() - b.z();
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn vdist3(a: Vector3, b: Vector3) -> f64 {
    let dx = a.x() - b.x();
    let dy = a.y() - b.y();
    let dz = a.z() - b.z();
    (dx * dx + dy * dy + dz * dz).sqrt()
}

// ============================================================================
// Oracle 1: Piegl & Tiller worked examples (`<= 1e-12`, most `<= 1e-15`)
// ============================================================================

/// P&T Ex2.3 (`refs/text/piegl_tiller1997_nurbs_book.txt:3265-3280, 3784-3798`):
/// `p = 2`, `U = {0,0,0, 1,2,3, 4,4, 5,5,5}`, `u = 5/2` -> `span = 4`,
/// `N = [1/8, 6/8, 1/8]`.
#[test]
fn oracle1_pt_ex2_3_find_span_and_basis_funs() {
    let knots = vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 4.0, 5.0, 5.0, 5.0];
    let p = 2;
    let n = knots.len() - p - 2; // 7
    let u = 2.5;

    let span = find_span(n, p, u, &knots).expect("valid span");
    assert_eq!(span, 4);

    // Right-endpoint special case (`:3245`): `u == U[n+1] == 5.0` returns `n == 7`.
    assert_eq!(find_span(n, p, 5.0, &knots).expect("right endpoint"), 7);
    assert_eq!(find_span(n, p, 0.0, &knots).expect("left endpoint"), 2);

    let basis = basis_funs(span, u, p, &knots).expect("basis_funs");
    assert_eq!(basis.len(), 3);
    assert!((basis[0] - 0.125).abs() <= 1e-15);
    assert!((basis[1] - 0.75).abs() <= 1e-15);
    assert!((basis[2] - 0.125).abs() <= 1e-15);
    assert!((basis.iter().sum::<f64>() - 1.0).abs() <= 1e-15);
}

/// P&T Ex2.4 & §3.3 (`refs/text/piegl_tiller1997_nurbs_book.txt:3366-3419, 4193-4204`):
/// `p = 2`, `U = {0,0,0, 1,2,3, 4,4, 5,5,5}`, `u = 5/2` ->
/// `ders[0] = [1/8, 6/8, 1/8]`, `ders[1] = [-1/2, 0, 1/2]`, `ders[2] = [1, -2, 1]`.
#[test]
fn oracle1_pt_ex2_4_ders_basis_funs() {
    let knots = vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 4.0, 5.0, 5.0, 5.0];
    let ders = ders_basis_funs(4, 2.5, 2, 3, &knots).expect("ders_basis_funs");
    assert_eq!(ders.len(), 4);

    assert!((ders[0][0] - 0.125).abs() <= 1e-15);
    assert!((ders[0][1] - 0.75).abs() <= 1e-15);
    assert!((ders[0][2] - 0.125).abs() <= 1e-15);

    assert!((ders[1][0] + 0.5).abs() <= 1e-15);
    assert!(ders[1][1].abs() <= 1e-15);
    assert!((ders[1][2] - 0.5).abs() <= 1e-15);

    assert!((ders[2][0] - 1.0).abs() <= 1e-15);
    assert!((ders[2][1] + 2.0).abs() <= 1e-15);
    assert!((ders[2][2] - 1.0).abs() <= 1e-15);

    // Order > p vanishes identically.
    assert_eq!(ders[3], vec![0.0, 0.0, 0.0]);
}

/// P&T Ex3.2 (`refs/text/piegl_tiller1997_nurbs_book.txt:4312-4340`):
/// Cubic curve on `U = {0,0,0,0, 2/5, 3/5, 3/5, 1,1,1,1}` has exact quadratic
/// derivative curve `C'(u)` on `U' = {0,0,0, 2/5, 3/5, 3/5, 1,1,1}` with control
/// points `Q_i = p / (u_{i+p+1} - u_{i+1}) * (P_{i+1} - P_i)`.
#[test]
fn oracle1_pt_ex3_2_cubic_curve_derivatives() {
    let u = vec![0.0, 0.0, 0.0, 0.0, 0.4, 0.6, 0.6, 1.0, 1.0, 1.0, 1.0];
    let p_ctrl = vec![
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 2.0, -0.5),
        Point3::new(2.5, 1.5, 1.0),
        Point3::new(3.0, -1.0, 2.0),
        Point3::new(4.5, 0.5, 0.5),
        Point3::new(6.0, 2.5, -1.0),
        Point3::new(7.0, 0.0, 0.0),
    ];
    let curve = BSplineCurve::new(3, u.clone(), p_ctrl.clone(), None).expect("valid curve");

    // Note OCR typo in `:4320` printed fraction `1/3` vs `2/5` from `U`: Eq. (3.4)
    // `Q_i = p / (u_{i+p+1} - u_{i+1}) * (P_{i+1} - P_i)` defines `C'(u)` exactly (`:4277, 4291`).
    let mut q_ctrl = Vec::with_capacity(6);
    for i in 0..6 {
        let scale = 3.0 / (u[i + 4] - u[i + 1]);
        let a = p_ctrl[i].as_array();
        let b = p_ctrl[i + 1].as_array();
        q_ctrl.push(Point3::new(
            scale * (b[0] - a[0]),
            scale * (b[1] - a[1]),
            scale * (b[2] - a[2]),
        ));
    }
    let deriv_curve =
        BSplineCurve::new(2, u[1..10].to_vec(), q_ctrl, None).expect("valid derivative curve");

    for &t in &[0.0, 0.15, 0.4, 0.5, 0.6, 0.85, 1.0] {
        let pt = curve.point_at(t).expect("point_at");
        let ck = curve.derivs(t, 2).expect("derivs");
        assert!(dist3(pt, Point3::from(ck[0].as_array())) <= 1e-14);

        let expected_c1 = deriv_curve.point_at(t).expect("deriv_curve point_at");
        assert!(
            vdist3(ck[1], Vector3::from(expected_c1.as_array())) <= 1e-13,
            "mismatch at t={t}: got {:?}, expected {:?}",
            ck[1],
            expected_c1
        );
    }
}

/// P&T Ex3.4 (`refs/text/piegl_tiller1997_nurbs_book.txt:4645-4662`):
/// Biquadratic surface on `U = {0,0,0, 2/5, 3/5, 1,1,1}`,
/// `V = {0,0,0, 1/5, 1/2, 4/5, 1,1,1}` evaluated at `(1/5, 3/5)`.
#[test]
fn oracle1_pt_ex3_4_surface_point() {
    let knots_u = vec![0.0, 0.0, 0.0, 0.4, 0.6, 1.0, 1.0, 1.0]; // n_u = 4 -> 5 rows
    let knots_v = vec![0.0, 0.0, 0.0, 0.2, 0.5, 0.8, 1.0, 1.0, 1.0]; // n_v = 5 -> 6 cols
    let mut grid = vec![vec![Point3::new(0.0, 0.0, 0.0); 6]; 5];
    for (i, row) in grid.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = Point3::new(i as f64, j as f64, ((i + 1) * (j + 2)) as f64);
        }
    }
    let surf = BSplineSurface::new(2, 2, knots_u.clone(), knots_v.clone(), grid.clone(), None)
        .expect("valid surface");

    let u = 0.2;
    let v = 0.6;
    assert_eq!(find_span(4, 2, u, &knots_u).unwrap(), 2);
    assert_eq!(find_span(5, 2, v, &knots_v).unwrap(), 4);

    let nu = basis_funs(2, u, 2, &knots_u).unwrap();
    let nv = basis_funs(4, v, 2, &knots_v).unwrap();
    let mut expected = [0.0; 3];
    for (k, &nuk) in nu.iter().enumerate() {
        for (l, &nvl) in nv.iter().enumerate() {
            let p = grid[k][2 + l].as_array();
            let w = nuk * nvl;
            expected[0] += w * p[0];
            expected[1] += w * p[1];
            expected[2] += w * p[2];
        }
    }
    let got = surf.point_at(u, v).expect("surface point_at");
    assert!(dist3(got, Point3::from(expected)) <= 1e-14);
}

/// P&T Ex4.1 (`refs/text/piegl_tiller1997_nurbs_book.txt:5487-5570`):
/// Quadratic rational curve on `U = {0,0,0, 1,2, 3,3,3}`, `w = {1, 4, 1, 1, 1}`,
/// `P = {(0,0,0), (1,1,0), (3,2,0), (4,1,0), (5,-1,0)}` at `u = 1` gives `C(1) = (7/5, 6/5, 0)`.
#[test]
fn oracle1_pt_ex4_1_rational_curve_point() {
    let knots = vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0];
    let ctrl = vec![
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 1.0, 0.0),
        Point3::new(3.0, 2.0, 0.0),
        Point3::new(4.0, 1.0, 0.0),
        Point3::new(5.0, -1.0, 0.0),
    ];
    let weights = vec![1.0, 4.0, 1.0, 1.0, 1.0];
    let curve = BSplineCurve::new(2, knots, ctrl, Some(weights)).expect("valid NURBS curve");

    let c1 = curve.point_at(1.0).expect("point_at(1.0)");
    assert!(dist3(c1, Point3::new(1.4, 1.2, 0.0)) <= 1e-15);
}

/// P&T Ex4.2 & Exercise 4.2 (`refs/text/piegl_tiller1997_nurbs_book.txt:5650-5675, 6200-6201`):
/// Quadratic rational Bezier circular arc `U = {0,0,0, 1,1,1}`,
/// `P = {(1,0,0), (1,1,0), (0,1,0)}`, `w = {1, 1, 2}` (`C(u) = ((1-u^2)/(1+u^2), 2u/(1+u^2), 0)`):
/// `C'(0) = (0, 2, 0)`, `C''(0) = (-4, 0, 0)`, `C'(1) = (-1, 0, 0)`, `C''(1) = (1, -1, 0)`.
#[test]
fn oracle1_pt_ex4_2_rational_curve_derivs() {
    let curve = BSplineCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        vec![
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ],
        Some(vec![1.0, 1.0, 2.0]),
    )
    .expect("valid rational Bezier arc");

    let d0 = curve.derivs(0.0, 2).expect("derivs at 0");
    assert!(vdist3(d0[0], Vector3::new(1.0, 0.0, 0.0)) <= 1e-15);
    assert!(vdist3(d0[1], Vector3::new(0.0, 2.0, 0.0)) <= 1e-15);
    assert!(vdist3(d0[2], Vector3::new(-4.0, 0.0, 0.0)) <= 1e-15);

    let d1 = curve.derivs(1.0, 2).expect("derivs at 1");
    assert!(vdist3(d1[0], Vector3::new(0.0, 1.0, 0.0)) <= 1e-15);
    assert!(vdist3(d1[1], Vector3::new(-1.0, 0.0, 0.0)) <= 1e-15);
    assert!(vdist3(d1[2], Vector3::new(1.0, -1.0, 0.0)) <= 1e-15);
}

/// P&T §4.4 Example & Exercises 4.4 / 4.6 (`refs/text/piegl_tiller1997_nurbs_book.txt:5948-5990, 6207-6219`):
/// 1. Biquadratic NURBS surface at `(5/2, 1)` evaluates to `(2, 98/27, 68/27)`.
/// 2. Rational bilinear patch at `(1/2, 1/2)` matches exact closed-form
///    `S`, `S_u`, `S_v`, `S_{uu}`, `S_{uv}`.
#[test]
fn oracle1_pt_sec4_4_and_ex4_6_rational_surface_point_and_derivs() {
    // Part 1: P&T §4.4 worked example (`:5952-5990`).
    let knots_u = vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 4.0, 5.0, 5.0, 5.0]; // 8 rows (0..=7)
    let knots_v = vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0]; // 5 cols (0..=4)
    let mut ctrl = vec![vec![Point3::new(0.0, 0.0, 0.0); 5]; 8];
    let mut weights = vec![vec![1.0; 5]; 8];

    // Active 3x3 block `i = 2, 3, 4; j = 1, 2, 3` given in homogeneous form `P^w = (wx, wy, wz, w)` (`:5972-5975`).
    let active_pw: [[[f64; 4]; 3]; 3] = [
        [
            [0.0, 2.0, 4.0, 1.0],
            [0.0, 6.0, 4.0, 2.0],
            [0.0, 2.0, 0.0, 1.0],
        ],
        [
            [4.0, 6.0, 8.0, 2.0],
            [12.0, 24.0, 12.0, 6.0],
            [4.0, 6.0, 0.0, 2.0],
        ],
        [
            [4.0, 2.0, 4.0, 1.0],
            [8.0, 6.0, 4.0, 2.0],
            [4.0, 2.0, 0.0, 1.0],
        ],
    ];
    for (di, row) in active_pw.iter().enumerate() {
        for (dj, &pw) in row.iter().enumerate() {
            let w = pw[3];
            ctrl[2 + di][1 + dj] = Point3::new(pw[0] / w, pw[1] / w, pw[2] / w);
            weights[2 + di][1 + dj] = w;
        }
    }
    let surf = BSplineSurface::new(2, 2, knots_u, knots_v, ctrl, Some(weights)).unwrap();
    let pt = surf.point_at(2.5, 1.0).unwrap();
    assert!(dist3(pt, Point3::new(2.0, 98.0 / 27.0, 68.0 / 27.0)) <= 1e-14);

    // Part 2: P&T Exercises 4.4 & 4.6 (`:6207-6219`), `S^w(1/2, 1/2) = (1, 1/2, 9/4, 5/4)`.
    let bil_surf = BSplineSurface::new(
        1,
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![0.0, 0.0, 1.0, 1.0],
        vec![
            vec![Point3::new(0.0, 0.0, 1.0), Point3::new(2.0, 1.0, 1.0)],
            vec![Point3::new(0.0, 1.0, 3.0), Point3::new(2.0, 0.0, 3.0)],
        ],
        Some(vec![vec![2.0, 1.0], vec![1.0, 1.0]]),
    )
    .unwrap();

    let skl = bil_surf.derivs(0.5, 0.5, 2).unwrap();
    assert!(vdist3(skl[0][0], Vector3::new(0.8, 0.4, 1.8)) <= 1e-15);
    assert!(vdist3(skl[1][0], Vector3::new(8.0 / 25.0, 4.0 / 25.0, 48.0 / 25.0)) <= 1e-15);
    assert!(vdist3(skl[0][1], Vector3::new(48.0 / 25.0, 4.0 / 25.0, 8.0 / 25.0)) <= 1e-15);
    assert!(
        vdist3(
            skl[2][0],
            Vector3::new(32.0 / 125.0, 16.0 / 125.0, 192.0 / 125.0)
        ) <= 1e-15
    );
    assert!(
        vdist3(
            skl[0][2],
            Vector3::new(192.0 / 125.0, 16.0 / 125.0, 32.0 / 125.0)
        ) <= 1e-15
    );
    assert!(
        vdist3(
            skl[1][1],
            Vector3::new(32.0 / 125.0, -224.0 / 125.0, 32.0 / 125.0)
        ) <= 1e-15
    );
}

/// P&T Ex5.1, Ex5.2 & §5.5 (`refs/text/piegl_tiller1997_nurbs_book.txt:6386-6417, 8865-8882`):
/// Knot insertion ratios (`u = 5/2` and `u = 2`) and Bezier degree elevation (`p = 2, t = 4`).
#[test]
fn oracle1_pt_ex5_1_ex5_2_and_sec5_5_knot_refinement_and_elevation() {
    let knots = vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 5.0, 5.0, 5.0];
    let p: Vec<Point3> = (0..8)
        .map(|i| Point3::new(i as f64, ((i * i) % 5) as f64, (i as f64) * 0.5))
        .collect();
    let curve = BSplineCurve::new(3, knots, p.clone(), None).unwrap();

    let lerp = |a: f64, p_hi: Point3, p_lo: Point3| {
        Point3::new(
            a * p_hi.x() + (1.0 - a) * p_lo.x(),
            a * p_hi.y() + (1.0 - a) * p_lo.y(),
            a * p_hi.z() + (1.0 - a) * p_lo.z(),
        )
    };

    // Ex5.1 (`:6386-6402`): insert `u = 2.5` -> `alpha_3 = 5/6, alpha_4 = 1/2, alpha_5 = 1/6`.
    let r1 = curve.refine_knots(&[2.5]).unwrap();
    let q1 = r1.control_points();
    assert_eq!(q1.len(), 9);
    assert!(dist3(q1[2], p[2]) <= 1e-15);
    assert!(dist3(q1[3], lerp(5.0 / 6.0, p[3], p[2])) <= 1e-15);
    assert!(dist3(q1[4], lerp(0.5, p[4], p[3])) <= 1e-15);
    assert!(dist3(q1[5], lerp(1.0 / 6.0, p[5], p[4])) <= 1e-15);
    assert!(dist3(q1[6], p[5]) <= 1e-15);

    // Ex5.2 (`:6408-6417`): insert existing knot `u = 2.0` -> `alpha_3 = 2/3, alpha_4 = 1/3, Q_5 = P_4`.
    let r2 = curve.refine_knots(&[2.0]).unwrap();
    let q2 = r2.control_points();
    assert_eq!(q2.len(), 9);
    assert!(dist3(q2[3], lerp(2.0 / 3.0, p[3], p[2])) <= 1e-15);
    assert!(dist3(q2[4], lerp(1.0 / 3.0, p[4], p[3])) <= 1e-15);
    assert!(dist3(q2[5], p[4]) <= 1e-15);

    // §5.5 Eq. (5.36) worked example (`:8865-8882`): quadratic Bezier elevated by `t = 4`.
    let bez = BSplineCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        vec![
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(0.0, 0.0, 1.0),
        ],
        None,
    )
    .unwrap();
    let elev = bez.degree_elevate(4).unwrap();
    assert_eq!(elev.degree(), 6);
    let qe = elev.control_points();
    assert_eq!(qe.len(), 7);
    assert!(dist3(qe[0], Point3::new(1.0, 0.0, 0.0)) <= 1e-15);
    assert!(dist3(qe[1], Point3::new(4.0 / 6.0, 2.0 / 6.0, 0.0)) <= 1e-15);
    assert!(dist3(qe[2], Point3::new(6.0 / 15.0, 8.0 / 15.0, 1.0 / 15.0)) <= 1e-15);
    assert!(dist3(qe[3], Point3::new(4.0 / 20.0, 12.0 / 20.0, 4.0 / 20.0)) <= 1e-15);
    assert!(dist3(qe[4], Point3::new(1.0 / 15.0, 8.0 / 15.0, 6.0 / 15.0)) <= 1e-15);
    assert!(dist3(qe[5], Point3::new(0.0, 2.0 / 6.0, 4.0 / 6.0)) <= 1e-15);
    assert!(dist3(qe[6], Point3::new(0.0, 0.0, 1.0)) <= 1e-15);
}

/// P&T Ex9.1 (`refs/text/piegl_tiller1997_nurbs_book.txt:15846-15923`):
/// `Q = {(0,0,0), (3,4,0), (-1,4,0), (-4,0,0), (-4,-3,0)}`, `p = 3`:
/// chords `5, 4, 5, 3` (`d = 17`), `uk = {0, 5/17, 9/17, 14/17, 1}`,
/// `U = {0,0,0,0, 28/51, 1,1,1,1}`.
#[test]
fn oracle1_pt_ex9_1_global_curve_interp_parameters_and_knots() {
    let q = vec![
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(3.0, 4.0, 0.0),
        Point3::new(-1.0, 4.0, 0.0),
        Point3::new(-4.0, 0.0, 0.0),
        Point3::new(-4.0, -3.0, 0.0),
    ];
    let uk = chord_length_params(&q).unwrap();
    let expected_uk = [0.0, 5.0 / 17.0, 9.0 / 17.0, 14.0 / 17.0, 1.0];
    for (got, exp) in uk.iter().zip(expected_uk.iter()) {
        assert!((got - exp).abs() <= 1e-15);
    }

    let knots = averaged_knots(&uk, 3).unwrap();
    let expected_knots = [0.0, 0.0, 0.0, 0.0, 28.0 / 51.0, 1.0, 1.0, 1.0, 1.0];
    for (got, exp) in knots.iter().zip(expected_knots.iter()) {
        assert!((got - exp).abs() <= 1e-15);
    }

    let curve = global_curve_interp(&q, 3).unwrap();
    for (&u_val, &target) in uk.iter().zip(q.iter()) {
        assert!(dist3(curve.point_at(u_val).unwrap(), target) <= 1e-12);
    }
}

// ============================================================================
// Oracle 2: Derivatives vs central finite differences (`<= 1e-8`)
// ============================================================================

fn sample_polynomial_curve() -> BSplineCurve {
    BSplineCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.65, 1.0, 1.0, 1.0, 1.0],
        vec![
            Point3::new(0.0, 0.5, -1.0),
            Point3::new(1.2, 2.3, 0.4),
            Point3::new(2.8, -0.7, 1.9),
            Point3::new(4.1, 1.6, -0.8),
            Point3::new(5.5, 0.2, 1.1),
            Point3::new(6.8, -1.4, 0.3),
        ],
        None,
    )
    .unwrap()
}

fn sample_rational_curve() -> BSplineCurve {
    BSplineCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.65, 1.0, 1.0, 1.0, 1.0],
        vec![
            Point3::new(0.0, 0.5, -1.0),
            Point3::new(1.2, 2.3, 0.4),
            Point3::new(2.8, -0.7, 1.9),
            Point3::new(4.1, 1.6, -0.8),
            Point3::new(5.5, 0.2, 1.1),
            Point3::new(6.8, -1.4, 0.3),
        ],
        Some(vec![1.0, 2.4, 0.65, 1.8, 0.9, 1.3]),
    )
    .unwrap()
}

fn sample_surface(rational: bool) -> BSplineSurface {
    let knots_u = vec![0.0, 0.0, 0.0, 0.0, 0.35, 0.72, 1.0, 1.0, 1.0, 1.0]; // p=3, 6 rows
    let knots_v = vec![0.0, 0.0, 0.0, 0.42, 0.78, 1.0, 1.0, 1.0]; // q=2, 5 cols
    let mut ctrl = vec![vec![Point3::new(0.0, 0.0, 0.0); 5]; 6];
    let mut weights = vec![vec![1.0; 5]; 6];
    for (i, (c_row, w_row)) in ctrl.iter_mut().zip(weights.iter_mut()).enumerate() {
        for (j, (c_cell, w_cell)) in c_row.iter_mut().zip(w_row.iter_mut()).enumerate() {
            let fi = i as f64;
            let fj = j as f64;
            *c_cell = Point3::new(
                fi * 1.1 + 0.2 * (fj * 1.3).sin(),
                fj * 0.95 - 0.3 * (fi * 0.8).cos(),
                0.6 * (fi * 0.7 + fj * 1.1).sin() + 0.15 * fi * fj,
            );
            *w_cell = 0.6 + 0.35 * ((i + 2 * j) % 5) as f64;
        }
    }
    BSplineSurface::new(
        3,
        2,
        knots_u,
        knots_v,
        ctrl,
        if rational { Some(weights) } else { None },
    )
    .unwrap()
}

#[test]
fn oracle2_curve_derivatives_vs_central_finite_differences() {
    let h1 = 1e-6;
    let h2 = 5e-7;
    for curve in [sample_polynomial_curve(), sample_rational_curve()] {
        for &u in &[0.08, 0.19, 0.47, 0.55, 0.81, 0.93] {
            let ck = curve.derivs(u, 2).unwrap();
            let p_minus = curve.point_at(u - h1).unwrap().as_array();
            let p_plus = curve.point_at(u + h1).unwrap().as_array();
            let fd1 = Vector3::new(
                (p_plus[0] - p_minus[0]) / (2.0 * h1),
                (p_plus[1] - p_minus[1]) / (2.0 * h1),
                (p_plus[2] - p_minus[2]) / (2.0 * h1),
            );
            assert!(
                vdist3(ck[1], fd1) <= 1e-8,
                "curve C'(u) FD mismatch at u={u}: analytic={:?}, fd={:?}",
                ck[1],
                fd1
            );

            let d_minus = curve.derivs(u - h2, 1).unwrap()[1].as_array();
            let d_plus = curve.derivs(u + h2, 1).unwrap()[1].as_array();
            let fd2 = Vector3::new(
                (d_plus[0] - d_minus[0]) / (2.0 * h2),
                (d_plus[1] - d_minus[1]) / (2.0 * h2),
                (d_plus[2] - d_minus[2]) / (2.0 * h2),
            );
            assert!(
                vdist3(ck[2], fd2) <= 1e-8,
                "curve C''(u) FD mismatch at u={u}: analytic={:?}, fd={:?}",
                ck[2],
                fd2
            );
        }
    }
}

#[test]
fn oracle2_surface_derivatives_vs_central_finite_differences() {
    let h = 1e-6;
    for rational in [false, true] {
        let surf = sample_surface(rational);
        for &(u, v) in &[
            (0.14, 0.21),
            (0.51, 0.19),
            (0.53, 0.61),
            (0.86, 0.33),
            (0.89, 0.91),
        ] {
            let skl = surf.derivs(u, v, 2).unwrap();
            let p_here = surf.point_at(u, v).unwrap();
            assert!(dist3(p_here, Point3::from(skl[0][0].as_array())) <= 1e-14);

            let pu_m = surf.point_at(u - h, v).unwrap().as_array();
            let pu_p = surf.point_at(u + h, v).unwrap().as_array();
            let fd_su = Vector3::new(
                (pu_p[0] - pu_m[0]) / (2.0 * h),
                (pu_p[1] - pu_m[1]) / (2.0 * h),
                (pu_p[2] - pu_m[2]) / (2.0 * h),
            );
            assert!(vdist3(skl[1][0], fd_su) <= 1e-8);

            let pv_m = surf.point_at(u, v - h).unwrap().as_array();
            let pv_p = surf.point_at(u, v + h).unwrap().as_array();
            let fd_sv = Vector3::new(
                (pv_p[0] - pv_m[0]) / (2.0 * h),
                (pv_p[1] - pv_m[1]) / (2.0 * h),
                (pv_p[2] - pv_m[2]) / (2.0 * h),
            );
            assert!(vdist3(skl[0][1], fd_sv) <= 1e-8);

            let du_m = surf.derivs(u - h, v, 1).unwrap()[1][0].as_array();
            let du_p = surf.derivs(u + h, v, 1).unwrap()[1][0].as_array();
            let fd_suu = Vector3::new(
                (du_p[0] - du_m[0]) / (2.0 * h),
                (du_p[1] - du_m[1]) / (2.0 * h),
                (du_p[2] - du_m[2]) / (2.0 * h),
            );
            assert!(vdist3(skl[2][0], fd_suu) <= 1e-8);

            let dv_m = surf.derivs(u, v - h, 1).unwrap()[0][1].as_array();
            let dv_p = surf.derivs(u, v + h, 1).unwrap()[0][1].as_array();
            let fd_svv = Vector3::new(
                (dv_p[0] - dv_m[0]) / (2.0 * h),
                (dv_p[1] - dv_m[1]) / (2.0 * h),
                (dv_p[2] - dv_m[2]) / (2.0 * h),
            );
            assert!(vdist3(skl[0][2], fd_svv) <= 1e-8);

            let duv_m = surf.derivs(u, v - h, 1).unwrap()[1][0].as_array();
            let duv_p = surf.derivs(u, v + h, 1).unwrap()[1][0].as_array();
            let fd_suv = Vector3::new(
                (duv_p[0] - duv_m[0]) / (2.0 * h),
                (duv_p[1] - duv_m[1]) / (2.0 * h),
                (duv_p[2] - duv_m[2]) / (2.0 * h),
            );
            assert!(vdist3(skl[1][1], fd_suv) <= 1e-8);
        }
    }
}

// ============================================================================
// Oracle 3: Knot refinement / Bezier decomposition / degree elevation leave
// geometry unchanged across 200 sampled parameters (`<= 1e-12` relative)
// ============================================================================

fn curve_scale(curve: &BSplineCurve) -> f64 {
    curve
        .control_points()
        .iter()
        .map(|p| {
            let a = p.as_array();
            (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
        })
        .fold(1.0f64, f64::max)
}

fn surface_scale(surf: &BSplineSurface) -> f64 {
    surf.control_points()
        .iter()
        .flat_map(|r| r.iter())
        .map(|p| {
            let a = p.as_array();
            (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
        })
        .fold(1.0f64, f64::max)
}

#[test]
fn oracle3_refine_decompose_and_elevate_curve_invariance_200_samples() {
    // Include curves with both simple and repeated interior knots, polynomial & rational.
    let poly_repeated = BSplineCurve::new(
        4,
        vec![
            0.0, 0.0, 0.0, 0.0, 0.0, 0.25, 0.6, 0.6, 1.0, 1.0, 1.0, 1.0, 1.0,
        ],
        vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 2.0, -1.0),
            Point3::new(2.0, -1.5, 1.5),
            Point3::new(3.2, 0.8, 2.2),
            Point3::new(4.5, 2.1, -0.4),
            Point3::new(5.7, -0.6, 0.9),
            Point3::new(7.0, 1.0, -1.2),
            Point3::new(8.0, 0.0, 0.5),
        ],
        None,
    )
    .unwrap();

    for orig in [
        sample_polynomial_curve(),
        sample_rational_curve(),
        poly_repeated,
    ] {
        let scale = curve_scale(&orig);
        let (u_min, u_max) = orig.domain();

        // 1. A5.4 RefineKnotVectCurve (including duplicate insertions).
        let refined = orig
            .refine_knots(&[0.12, 0.12, 0.44, 0.51, 0.77, 0.91])
            .unwrap();

        // 2. A5.6 DecomposeCurve.
        let segments = orig.decompose().unwrap();
        assert!(segments.len() >= 3);

        // 3. A5.9 DegreeElevateCurve (`t = 1, 2, 3`).
        let elev1 = orig.degree_elevate(1).unwrap();
        let elev2 = orig.degree_elevate(2).unwrap();
        let elev3 = orig.degree_elevate(3).unwrap();
        assert_eq!(elev1.degree(), orig.degree() + 1);
        assert_eq!(elev2.degree(), orig.degree() + 2);
        assert_eq!(elev3.degree(), orig.degree() + 3);
        assert_eq!(orig.degree_elevate(0).unwrap(), orig);

        let mut max_rel_refine = 0.0f64;
        let mut max_rel_decomp = 0.0f64;
        let mut max_rel_elev = 0.0f64;

        for idx in 0..200 {
            let u = u_min + (u_max - u_min) * (idx as f64) / 199.0;
            let p_orig = orig.point_at(u).unwrap();

            let p_ref = refined.point_at(u).unwrap();
            max_rel_refine = max_rel_refine.max(dist3(p_orig, p_ref) / scale);

            let seg = segments
                .iter()
                .find(|s| {
                    let (a, b) = s.domain();
                    u >= a && u <= b
                })
                .expect("segment covering u");
            let p_seg = seg.point_at(u).unwrap();
            max_rel_decomp = max_rel_decomp.max(dist3(p_orig, p_seg) / scale);

            for elev in [&elev1, &elev2, &elev3] {
                let p_el = elev.point_at(u).unwrap();
                max_rel_elev = max_rel_elev.max(dist3(p_orig, p_el) / scale);
            }
        }

        assert!(
            max_rel_refine <= 1e-12,
            "A5.4 max_rel_refine = {max_rel_refine}"
        );
        assert!(
            max_rel_decomp <= 1e-12,
            "A5.6 max_rel_decomp = {max_rel_decomp}"
        );
        assert!(max_rel_elev <= 1e-12, "A5.9 max_rel_elev = {max_rel_elev}");
    }
}

#[test]
fn oracle3_decompose_surface_invariance_200_plus_samples() {
    for rational in [false, true] {
        let surf = sample_surface(rational);
        let scale = surface_scale(&surf);
        let patches = surf.decompose().unwrap();
        assert_eq!(patches.len(), 3); // 3 u-spans: [0, 0.35], [0.35, 0.72], [0.72, 1]
        assert_eq!(patches[0].len(), 3); // 3 v-spans: [0, 0.42], [0.42, 0.78], [0.78, 1]

        let (u_min, u_max) = surf.domain_u();
        let (v_min, v_max) = surf.domain_v();
        let mut max_rel = 0.0f64;

        // 20 x 10 = 200 parameter samples spanning all 9 Bezier sub-patches and boundaries.
        for iu in 0..20 {
            let u = u_min + (u_max - u_min) * (iu as f64) / 19.0;
            let row = patches
                .iter()
                .find(|r| {
                    let (a, b) = r[0].domain_u();
                    u >= a && u <= b
                })
                .expect("u patch strip");
            for iv in 0..10 {
                let v = v_min + (v_max - v_min) * (iv as f64) / 9.0;
                let patch = row
                    .iter()
                    .find(|p| {
                        let (c, d) = p.domain_v();
                        v >= c && v <= d
                    })
                    .expect("v patch");

                let p_orig = surf.point_at(u, v).unwrap();
                let p_bez = patch.point_at(u, v).unwrap();
                max_rel = max_rel.max(dist3(p_orig, p_bez) / scale);
            }
        }
        assert!(
            max_rel <= 1e-12,
            "A5.7 surface decomposition max_rel = {max_rel} (rational={rational})"
        );
    }
}

// ============================================================================
// Oracle 4: Rational quarter circle & 9-point full circle (`<= 1e-15` relative)
// ============================================================================

/// P&T §7.3 / Algorithm A7.1 (`refs/text/piegl_tiller1997_nurbs_book.txt:13351-13401`):
/// Quarter circle (`w = [1, sqrt(2)/2, 1]`) and 9-point full circle evaluate on
/// `x^2 + y^2 = R^2` to `<= 1e-15` relative.
#[test]
fn oracle4_rational_quarter_and_full_circle_machine_precision() {
    for radius in [1.0, 3.7] {
        // 1. Quarter circle (`p = 2, U = [0,0,0, 1,1,1], w = [1, sqrt(2)/2, 1]`).
        let quarter = BSplineCurve::new(
            2,
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            vec![
                Point3::new(radius, 0.0, 0.0),
                Point3::new(radius, radius, 0.0),
                Point3::new(0.0, radius, 0.0),
            ],
            Some(vec![1.0, FRAC_1_SQRT_2, 1.0]),
        )
        .unwrap();

        for idx in 0..200 {
            let u = (idx as f64) / 199.0;
            let pt = quarter.point_at(u).unwrap();
            assert!(pt.x() >= -1e-15 && pt.y() >= -1e-15);
            assert!(pt.z().abs() <= 1e-15);
            let r = pt.x().hypot(pt.y());
            let rel_err = (r - radius).abs() / radius;
            assert!(
                rel_err <= 1e-15,
                "quarter circle radius rel_err = {rel_err:e} at u={u}"
            );
        }

        // 2. 9-point full circle (`p = 2, U = [0,0,0, 1/4,1/4, 1/2,1/2, 3/4,3/4, 1,1,1]`).
        let full_circle = BSplineCurve::new(
            2,
            vec![
                0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
            ],
            vec![
                Point3::new(radius, 0.0, 0.0),
                Point3::new(radius, radius, 0.0),
                Point3::new(0.0, radius, 0.0),
                Point3::new(-radius, radius, 0.0),
                Point3::new(-radius, 0.0, 0.0),
                Point3::new(-radius, -radius, 0.0),
                Point3::new(0.0, -radius, 0.0),
                Point3::new(radius, -radius, 0.0),
                Point3::new(radius, 0.0, 0.0),
            ],
            Some(vec![
                1.0,
                FRAC_1_SQRT_2,
                1.0,
                FRAC_1_SQRT_2,
                1.0,
                FRAC_1_SQRT_2,
                1.0,
                FRAC_1_SQRT_2,
                1.0,
            ]),
        )
        .unwrap();

        for idx in 0..=200 {
            let u = (idx as f64) / 200.0;
            let pt = full_circle.point_at(u).unwrap();
            assert!(pt.z().abs() <= 1e-15);
            let r = pt.x().hypot(pt.y());
            let rel_err = (r - radius).abs() / radius;
            assert!(
                rel_err <= 1e-15,
                "full circle radius rel_err = {rel_err:e} at u={u}"
            );
        }
    }
}

// ============================================================================
// Oracle 5: Global interpolation (open A9.1 + periodic L3) passes through data
// points to `<= 1e-12`
// ============================================================================

#[test]
fn oracle5_open_and_periodic_global_curve_interpolation() {
    let open_pts = vec![
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(0.4, 1.2, 0.3),
        Point3::new(1.9, 2.5, -0.4),
        Point3::new(3.1, 0.8, 1.1),
        Point3::new(4.8, -0.9, 0.7),
        Point3::new(6.0, 0.3, -0.2),
        Point3::new(7.5, 1.7, 0.9),
    ];

    for degree in [1usize, 2, 3, 4] {
        let params = chord_length_params(&open_pts).unwrap();
        let curve = global_curve_interp(&open_pts, degree).unwrap();
        assert_eq!(curve.degree(), degree);
        for (&u, &target) in params.iter().zip(open_pts.iter()) {
            let got = curve.point_at(u).unwrap();
            assert!(
                dist3(got, target) <= 1e-12,
                "open interp degree={degree} missed point at u={u}: dist={:e}",
                dist3(got, target)
            );
        }
    }

    // Periodic closed loops with both odd (`m = 3, 5`) and even (`m = 4, 6`) point counts
    // across degrees `p = 1, 2, 3` (`specs/loft.md` §2.2 `:130`).
    let ring3 = vec![
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(-0.5, 0.9, 0.4),
        Point3::new(-0.5, -0.8, -0.3),
    ];
    let ring4 = vec![
        Point3::new(2.0, 0.0, 0.0),
        Point3::new(0.2, 1.5, 0.5),
        Point3::new(-1.8, 0.1, 0.0),
        Point3::new(0.0, -1.3, -0.4),
    ];
    let ring5 = vec![
        Point3::new(2.0, 0.0, 0.0),
        Point3::new(0.7, 1.8, 0.3),
        Point3::new(-1.5, 1.1, -0.2),
        Point3::new(-1.7, -1.0, 0.6),
        Point3::new(0.6, -1.6, -0.5),
    ];
    let ring6 = vec![
        Point3::new(2.5, 0.0, 0.0),
        Point3::new(1.3, 2.0, 0.4),
        Point3::new(-1.1, 2.1, -0.1),
        Point3::new(-2.4, 0.2, 0.5),
        Point3::new(-1.2, -1.9, -0.3),
        Point3::new(1.1, -1.8, 0.2),
    ];

    for ring in [&ring3, &ring4, &ring5, &ring6] {
        let max_deg = 3.min(ring.len() - 1);
        let params = periodic_chord_length_params(ring).unwrap();
        for degree in 1..=max_deg {
            let curve = global_curve_interp_periodic(ring, degree).unwrap();
            assert_eq!(curve.domain(), (0.0, 1.0));
            assert!(dist3(curve.point_at(0.0).unwrap(), ring[0]) <= 1e-12);
            assert!(dist3(curve.point_at(1.0).unwrap(), ring[0]) <= 1e-12);

            for (k, &target) in ring.iter().enumerate() {
                let got = curve.point_at(params[k]).unwrap();
                assert!(
                    dist3(got, target) <= 1e-12,
                    "periodic interp m={} deg={degree} missed Q_{k}: dist={:e}",
                    ring.len(),
                    dist3(got, target)
                );
            }

            // Verify C^{degree - 1} periodic continuity across the `u = 0 / u = 1` seam.
            let d0 = curve.derivs(0.0, degree - 1).unwrap();
            let d1 = curve.derivs(1.0, degree - 1).unwrap();
            for r in 0..degree {
                assert!(
                    vdist3(d0[r], d1[r]) <= 1e-11,
                    "periodic seam C^{r} mismatch for m={} deg={degree}: {:?} vs {:?}",
                    ring.len(),
                    d0[r],
                    d1[r]
                );
            }
        }
    }
}

// ============================================================================
// Validation & loud-refusal tests (no panics, no silent clamps, no fallbacks)
// ============================================================================

#[test]
fn validation_refuses_invalid_curves_surfaces_weights_params_and_interpolations() {
    let p0 = Point3::new(0.0, 0.0, 0.0);
    let p1 = Point3::new(1.0, 0.0, 0.0);
    let p2 = Point3::new(2.0, 1.0, 0.0);

    // Degree 0 refused.
    assert_eq!(
        BSplineCurve::new(0, vec![0.0, 1.0], vec![p0], None),
        Err(BSplineError::ZeroDegree)
    );

    // Too few control points refused.
    assert_eq!(
        BSplineCurve::new(2, vec![0.0, 0.0, 0.0, 1.0, 1.0], vec![p0, p1], None),
        Err(BSplineError::TooFewControlPoints {
            degree: 2,
            count: 2
        })
    );

    // Wrong knot length refused.
    assert_eq!(
        BSplineCurve::new(2, vec![0.0, 0.0, 0.0, 1.0, 1.0], vec![p0, p1, p2], None),
        Err(BSplineError::KnotCountMismatch {
            expected: 6,
            actual: 5
        })
    );

    // Decreasing knots refused.
    assert_eq!(
        BSplineCurve::new(
            1,
            vec![0.0, 0.0, 0.7, 0.4, 1.0, 1.0],
            vec![p0, p1, p2, p0],
            None
        ),
        Err(BSplineError::DecreasingKnots {
            index: 2,
            prev: 0.7,
            next: 0.4
        })
    );

    // Unclamped start / end refused.
    assert!(matches!(
        BSplineCurve::new(
            2,
            vec![0.0, 0.1, 0.2, 1.0, 1.0, 1.0],
            vec![p0, p1, p2],
            None
        ),
        Err(BSplineError::UnclampedStart { .. })
    ));
    assert!(matches!(
        BSplineCurve::new(
            2,
            vec![0.0, 0.0, 0.0, 0.8, 0.9, 1.0],
            vec![p0, p1, p2],
            None
        ),
        Err(BSplineError::UnclampedEnd { .. })
    ));

    // Interior multiplicity > degree refused.
    assert_eq!(
        BSplineCurve::new(
            2,
            vec![0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0],
            vec![p0, p1, p2, p0, p1, p2],
            None
        ),
        Err(BSplineError::InteriorKnotMultiplicityExceedsDegree {
            knot: 0.5,
            multiplicity: 3,
            degree: 2
        })
    );

    // Non-positive and non-finite weights refused loudly.
    for bad_w in [0.0, -0.25, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            BSplineCurve::new(
                2,
                vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                vec![p0, p1, p2],
                Some(vec![1.0, bad_w, 1.0])
            ),
            Err(BSplineError::NonPositiveWeight { index: 1, .. })
        ));
    }

    // Out-of-domain parameter refused without silent clamping.
    let valid = BSplineCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        vec![p0, p1, p2],
        None,
    )
    .unwrap();
    assert!(matches!(
        valid.point_at(-1e-12),
        Err(BSplineError::ParameterOutOfRange { .. })
    ));
    assert!(matches!(
        valid.point_at(1.0 + 1e-12),
        Err(BSplineError::ParameterOutOfRange { .. })
    ));
    assert!(matches!(
        valid.derivs(f64::NAN, 1),
        Err(BSplineError::ParameterOutOfRange { .. })
    ));

    // Refinement errors: empty, out-of-range, unsorted, excessive multiplicity.
    assert_eq!(
        valid.refine_knots(&[]),
        Err(BSplineError::EmptyRefinementKnots)
    );
    assert!(matches!(
        valid.refine_knots(&[0.0]),
        Err(BSplineError::RefinementKnotOutOfRange { .. })
    ));
    assert!(matches!(
        valid.refine_knots(&[0.6, 0.4]),
        Err(BSplineError::UnsortedRefinementKnots { .. })
    ));
    assert!(matches!(
        valid.refine_knots(&[0.5, 0.5, 0.5]),
        Err(BSplineError::RefinementMultiplicityExceedsDegree { .. })
    ));

    // Surface ragged grid & non-positive weight refused.
    assert!(matches!(
        BSplineSurface::new(
            1,
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![0.0, 0.0, 1.0, 1.0],
            vec![vec![p0, p1], vec![p2]],
            None
        ),
        Err(BSplineError::RaggedControlGrid { row: 1, .. })
    ));
    assert!(matches!(
        BSplineSurface::new(
            1,
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![0.0, 0.0, 1.0, 1.0],
            vec![vec![p0, p1], vec![p2, p0]],
            Some(vec![vec![1.0, 1.0], vec![0.0, 1.0]])
        ),
        Err(BSplineError::NonPositiveWeight { index: 2, .. })
    ));

    // Standalone rational derivative quotient guards.
    assert!(matches!(
        rat_curve_derivs(&[Vector3::new(0.0, 0.0, 0.0)], &[0.0], 0),
        Err(BSplineError::NonPositiveWeight { index: 0, .. })
    ));
    assert!(matches!(
        rat_surface_derivs(&[vec![Vector3::new(0.0, 0.0, 0.0)]], &[vec![-1.0]], 0),
        Err(BSplineError::NonPositiveWeight { index: 0, .. })
    ));

    // Global interpolation refuses duplicate consecutive points & degenerate counts.
    assert_eq!(
        global_curve_interp(&[p0, p1, p1, p2], 2),
        Err(BSplineError::ZeroChordLength { index: 1 })
    );
    assert_eq!(
        global_curve_interp(&[p0, p1], 2),
        Err(BSplineError::InterpolationTooFewPoints {
            count: 2,
            min_required: 3
        })
    );
    assert_eq!(
        global_curve_interp_periodic(&[p0, p1, p2, p0], 2),
        Err(BSplineError::ZeroChordLength { index: 3 })
    );
}
