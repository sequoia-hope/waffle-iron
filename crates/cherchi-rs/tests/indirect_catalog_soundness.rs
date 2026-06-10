//! PR-CR-M7b oracle — filter soundness + non-vacuity for the full
//! clean-room predicate catalog (orient2d projections, per-axis
//! comparators), plus independent-formulation checks for the composite
//! predicates (point_in_triangle, inner_segments_cross,
//! point_in_{inner_,}segment).
//!
//! **Soundness** (load-bearing): whenever the filtered (semi-static +
//! interval) tier certifies a sign, the exact rational tier must agree.
//! A violation means an under-estimated `δ(1)` — the fix is a MORE
//! conservative generator, never a looser test.
//!
//! **Non-vacuity**: the filtered tier must resolve ≥ 90% of generic
//! implicit cases per family (a vacuous filter is "sound" but pays exact
//! rational arithmetic on every call).
//!
//! **Composites**: tested against INDEPENDENT pure-`RBig` formulations
//! written here (parametric line/plane solves + 3D vector algebra), not
//! against the projected-orientation compositions they are built from.
//!
//! Runs with DEFAULT features (pure Rust, no FFI).

mod indirect_common;

use cad_primitives::Point3;
use cherchi_rs::predicates::indirect::{
    inner_segments_cross_indirect, less_than_on_x_indirect, less_than_on_x_indirect_exact,
    less_than_on_x_indirect_filtered, less_than_on_y_indirect, less_than_on_y_indirect_exact,
    less_than_on_y_indirect_filtered, less_than_on_z_indirect, less_than_on_z_indirect_exact,
    less_than_on_z_indirect_filtered, orient2d_xy_indirect, orient2d_xy_indirect_exact,
    orient2d_xy_indirect_filtered, orient2d_yz_indirect, orient2d_yz_indirect_exact,
    orient2d_yz_indirect_filtered, orient2d_zx_indirect, orient2d_zx_indirect_exact,
    orient2d_zx_indirect_filtered, point_in_inner_segment_indirect, point_in_segment_indirect,
    point_in_triangle_indirect, GenericPoint3D, Sign,
};
use dashu::float::FBig;
use dashu::rational::RBig;
use indirect_common::*;

type Tri = (
    fn(&GenericPoint3D, &GenericPoint3D, &GenericPoint3D) -> Option<Sign>,
    fn(&GenericPoint3D, &GenericPoint3D, &GenericPoint3D) -> Sign,
    fn(&GenericPoint3D, &GenericPoint3D, &GenericPoint3D) -> Sign,
);
type Pair = (
    fn(&GenericPoint3D, &GenericPoint3D) -> Option<Sign>,
    fn(&GenericPoint3D, &GenericPoint3D) -> Sign,
    fn(&GenericPoint3D, &GenericPoint3D) -> Sign,
);

const ORIENT2D_FAMILY: [(&str, Tri); 3] = [
    (
        "xy",
        (
            orient2d_xy_indirect_filtered,
            orient2d_xy_indirect_exact,
            orient2d_xy_indirect,
        ),
    ),
    (
        "yz",
        (
            orient2d_yz_indirect_filtered,
            orient2d_yz_indirect_exact,
            orient2d_yz_indirect,
        ),
    ),
    (
        "zx",
        (
            orient2d_zx_indirect_filtered,
            orient2d_zx_indirect_exact,
            orient2d_zx_indirect,
        ),
    ),
];

const LESS_THAN_FAMILY: [(&str, Pair); 3] = [
    (
        "x",
        (
            less_than_on_x_indirect_filtered,
            less_than_on_x_indirect_exact,
            less_than_on_x_indirect,
        ),
    ),
    (
        "y",
        (
            less_than_on_y_indirect_filtered,
            less_than_on_y_indirect_exact,
            less_than_on_y_indirect,
        ),
    ),
    (
        "z",
        (
            less_than_on_z_indirect_filtered,
            less_than_on_z_indirect_exact,
            less_than_on_z_indirect,
        ),
    ),
];

// ---------------------------------------------------------------------
// 1. orient2d: generic corpus soundness + hit rate, per projection
// ---------------------------------------------------------------------

#[test]
fn orient2d_generic_corpus_soundness_and_hit_rate() {
    let pool = mixed_pool(8, 8, 8, 1.0);
    let tuples = tuple_stream(pool.len(), 640);
    assert!(tuples.len() > 500, "corpus too small: {}", tuples.len());

    for (name, (filtered, exact, full)) in ORIENT2D_FAMILY {
        let mut implicit_definite = 0usize;
        let mut implicit_total = 0usize;
        for &[a, b, c, _] in &tuples {
            let (pa, pb, pc) = (&pool[a], &pool[b], &pool[c]);
            let f = filtered(pa, pb, pc);
            let x = exact(pa, pb, pc);
            if let Some(s) = f {
                assert_eq!(
                    s, x,
                    "orient2d_{name}: FILTER SOUNDNESS VIOLATION on [{a}, {b}, {c}]: \
                     filtered {s:?} vs exact {x:?}"
                );
            }
            assert_eq!(
                full(pa, pb, pc),
                x,
                "orient2d_{name}: dispatcher disagrees with exact on [{a}, {b}, {c}]"
            );
            if !(pa.is_explicit() && pb.is_explicit() && pc.is_explicit()) {
                implicit_total += 1;
                if f.is_some() {
                    implicit_definite += 1;
                }
            }
        }
        assert!(implicit_total > 300, "implicit subset too small");
        let rate = implicit_definite as f64 / implicit_total as f64;
        assert!(
            rate >= 0.90,
            "orient2d_{name}: filtered tier resolved only \
             {implicit_definite}/{implicit_total} = {rate:.3} of generic implicit cases \
             (gate: >= 0.90)"
        );
        println!("orient2d_{name}: implicit-only hit rate {rate:.3}");
    }
}

// ---------------------------------------------------------------------
// 2. orient2d: magnitude sweeps
// ---------------------------------------------------------------------

#[test]
fn orient2d_magnitude_sweep_soundness() {
    let scales: [f64; 6] = [2.0f64.powi(-100), 1e-30, 1.0, 2.0f64.powi(50), 1e20, 1e30];
    for s in scales {
        let pool = mixed_pool(4, 4, 4, s);
        let tuples = tuple_stream(pool.len(), 150);
        for (name, (filtered, exact, _)) in ORIENT2D_FAMILY {
            for &[a, b, c, _] in &tuples {
                let (pa, pb, pc) = (&pool[a], &pool[b], &pool[c]);
                if let Some(f) = filtered(pa, pb, pc) {
                    let x = exact(pa, pb, pc);
                    assert_eq!(
                        f, x,
                        "orient2d_{name} scale {s:e}: SOUNDNESS VIOLATION on [{a}, {b}, {c}]"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------
// 3. orient2d: collinear ± 1..4-ulp families
// ---------------------------------------------------------------------

fn ulps(x: f64, n: i32) -> f64 {
    let mut v = x;
    for _ in 0..n.abs() {
        v = if n > 0 { v.next_up() } else { v.next_down() };
    }
    v
}

#[test]
fn orient2d_xy_near_collinear_perturbations() {
    // a = (0, 1), b = (1, 1) in xy; c = (0.3, 1 ± k ulps):
    // det[b−a; c−a] = (1)(c_y − 1) − 0 = c_y − 1 → sign(k).
    let a = GenericPoint3D::explicit(Point3::new(0.0, 1.0, 0.0));
    let b = GenericPoint3D::explicit(Point3::new(1.0, 1.0, 5.0));
    for n in -4i32..=4 {
        let cy = ulps(1.0, n);
        let expected = match n.cmp(&0) {
            core::cmp::Ordering::Greater => Sign::Positive,
            core::cmp::Ordering::Less => Sign::Negative,
            core::cmp::Ordering::Equal => Sign::Zero,
        };
        // Explicit query point.
        let c = GenericPoint3D::explicit(Point3::new(0.3, cy, -2.0));
        assert_eq!(
            orient2d_xy_indirect_exact(&a, &b, &c),
            expected,
            "exact tier wrong at {n} ulps (explicit)"
        );
        if let Some(s) = orient2d_xy_indirect_filtered(&a, &b, &c) {
            assert_eq!(s, expected, "SOUNDNESS VIOLATION at {n} ulps (explicit)");
        }
        // LPI query point at exactly (0.3, cy, 0) — vertical line through
        // (0.3, cy) into a generic triangle in z = 0.
        let l = GenericPoint3D::lpi(
            Point3::new(0.3, cy, -1.0),
            Point3::new(0.3, cy, 1.0),
            Point3::new(5.0, 0.5, 0.0),
            Point3::new(6.0, 1.5, 0.0),
            Point3::new(4.0, 3.0, 0.0),
        );
        assert_eq!(
            orient2d_xy_indirect_exact(&a, &b, &l),
            expected,
            "exact tier wrong at {n} ulps (LPI)"
        );
        if let Some(s) = orient2d_xy_indirect_filtered(&a, &b, &l) {
            assert_eq!(s, expected, "SOUNDNESS VIOLATION at {n} ulps (LPI)");
        }
        assert_eq!(orient2d_xy_indirect(&a, &b, &l), expected);
    }
}

// ---------------------------------------------------------------------
// 4. less_than: generic corpus soundness + hit rate, sweeps, ties
// ---------------------------------------------------------------------

#[test]
fn less_than_generic_corpus_soundness_and_hit_rate() {
    let pool = mixed_pool(8, 8, 8, 1.0);
    let tuples = tuple_stream(pool.len(), 640);

    for (name, (filtered, exact, full)) in LESS_THAN_FAMILY {
        let mut implicit_definite = 0usize;
        let mut implicit_total = 0usize;
        for &[a, b, _, _] in &tuples {
            let (pa, pb) = (&pool[a], &pool[b]);
            let f = filtered(pa, pb);
            let x = exact(pa, pb);
            if let Some(s) = f {
                assert_eq!(
                    s, x,
                    "less_than_on_{name}: FILTER SOUNDNESS VIOLATION on [{a}, {b}]"
                );
            }
            assert_eq!(
                full(pa, pb),
                x,
                "less_than_on_{name}: dispatcher disagrees with exact on [{a}, {b}]"
            );
            // Antisymmetry (exact tier): lt(a, b) == lt(b, a).flipped().
            assert_eq!(
                x,
                exact(pb, pa).flipped(),
                "less_than_on_{name}: antisymmetry violated on [{a}, {b}]"
            );
            if !(pa.is_explicit() && pb.is_explicit()) {
                implicit_total += 1;
                if f.is_some() {
                    implicit_definite += 1;
                }
            }
        }
        assert!(implicit_total > 300, "implicit subset too small");
        let rate = implicit_definite as f64 / implicit_total as f64;
        assert!(
            rate >= 0.90,
            "less_than_on_{name}: filtered tier resolved only \
             {implicit_definite}/{implicit_total} = {rate:.3} (gate: >= 0.90)"
        );
        println!("less_than_on_{name}: implicit-only hit rate {rate:.3}");
    }
}

#[test]
fn less_than_magnitude_sweep_soundness() {
    let scales: [f64; 6] = [2.0f64.powi(-100), 1e-30, 1.0, 2.0f64.powi(50), 1e20, 1e30];
    for s in scales {
        let pool = mixed_pool(4, 4, 4, s);
        let tuples = tuple_stream(pool.len(), 150);
        for (name, (filtered, exact, _)) in LESS_THAN_FAMILY {
            for &[a, b, _, _] in &tuples {
                let (pa, pb) = (&pool[a], &pool[b]);
                if let Some(f) = filtered(pa, pb) {
                    assert_eq!(
                        f,
                        exact(pa, pb),
                        "less_than_on_{name} scale {s:e}: SOUNDNESS VIOLATION on [{a}, {b}]"
                    );
                }
            }
        }
    }
}

/// Equal-coordinate degenerate family: implicit/explicit pairs sharing a
/// coordinate EXACTLY (different generators, same geometric value). The
/// exact tier must say Zero; the filtered tier must agree or pass.
#[test]
fn less_than_exact_tie_family() {
    for k in 0..6u64 {
        let x = coord(40 + k);
        let y = coord(50 + k);
        // LPI at (x, y, 0): vertical line through (x, y) × plane z = 0.
        let l = GenericPoint3D::lpi(
            Point3::new(x, y, -1.0),
            Point3::new(x, y, 1.0),
            Point3::new(5.0, 0.5, 0.0),
            Point3::new(6.0, 1.5, 0.0),
            Point3::new(4.0, 3.0, 0.0),
        );
        // Explicit point sharing x only.
        let ex = GenericPoint3D::explicit(Point3::new(x, y + 1.0, 3.0));
        assert_eq!(
            less_than_on_x_indirect_exact(&l, &ex),
            Sign::Zero,
            "tie {k}: exact x tie expected"
        );
        if let Some(s) = less_than_on_x_indirect_filtered(&l, &ex) {
            assert_eq!(s, Sign::Zero, "tie {k}: SOUNDNESS VIOLATION (x)");
        }
        // Second LPI from different plane generators, same point.
        let l2 = GenericPoint3D::lpi(
            Point3::new(x, y, -2.0),
            Point3::new(x, y, 3.0),
            Point3::new(7.0, -0.5, 0.0),
            Point3::new(9.0, 1.0, 0.0),
            Point3::new(6.0, 4.0, 0.0),
        );
        for (_, (_, exact, full)) in LESS_THAN_FAMILY {
            assert_eq!(exact(&l, &l2), Sign::Zero, "tie {k}: LPI/LPI tie");
            assert_eq!(full(&l, &l2), Sign::Zero, "tie {k}: LPI/LPI tie (full)");
        }
    }
}

// ---------------------------------------------------------------------
// 5. Composites vs independent pure-RBig formulations
// ---------------------------------------------------------------------

fn rb(x: f64) -> RBig {
    let fb: FBig = FBig::try_from(x).expect("finite");
    RBig::try_from(fb).expect("rational")
}

fn rb3(p: Point3) -> [RBig; 3] {
    [rb(p.x()), rb(p.y()), rb(p.z())]
}

fn sub3(a: &[RBig; 3], b: &[RBig; 3]) -> [RBig; 3] {
    [&a[0] - &b[0], &a[1] - &b[1], &a[2] - &b[2]]
}

fn add3(a: &[RBig; 3], b: &[RBig; 3]) -> [RBig; 3] {
    [&a[0] + &b[0], &a[1] + &b[1], &a[2] + &b[2]]
}

fn scale3(a: &[RBig; 3], s: &RBig) -> [RBig; 3] {
    [&a[0] * s, &a[1] * s, &a[2] * s]
}

fn cross3(a: &[RBig; 3], b: &[RBig; 3]) -> [RBig; 3] {
    [
        &a[1] * &b[2] - &a[2] * &b[1],
        &a[2] * &b[0] - &a[0] * &b[2],
        &a[0] * &b[1] - &a[1] * &b[0],
    ]
}

fn dot3(a: &[RBig; 3], b: &[RBig; 3]) -> RBig {
    &a[0] * &b[0] + &a[1] * &b[1] + &a[2] * &b[2]
}

fn is_zero3(a: &[RBig; 3]) -> bool {
    a.iter().all(|c| *c == RBig::ZERO)
}

/// A generic point spec with INDEPENDENTLY computable exact coordinates.
#[derive(Clone, Debug)]
enum Spec {
    E(Point3),
    L([Point3; 5]),
    T([[Point3; 3]; 3]),
}

impl Spec {
    fn to_native(&self) -> GenericPoint3D {
        match self {
            Spec::E(p) => GenericPoint3D::explicit(*p),
            Spec::L(g) => GenericPoint3D::lpi(g[0], g[1], g[2], g[3], g[4]),
            Spec::T(t) => GenericPoint3D::tpi(t[0], t[1], t[2]),
        }
    }

    /// Exact rational coordinates via an INDEPENDENT derivation:
    /// parametric line-plane solve for LPI (`x = p + t·(q − p)` with
    /// `t = N·(r − p) / N·(q − p)`, `N = (s − r) × (t − r)`); 3-plane
    /// Cramer solve over the plane equations for TPI. (The production
    /// lambdas use Attene/Cherchi's determinant rewriting instead.)
    fn exact(&self) -> Option<[RBig; 3]> {
        match self {
            Spec::E(p) => Some(rb3(*p)),
            Spec::L(g) => {
                let (p, q, r, s, t) = (rb3(g[0]), rb3(g[1]), rb3(g[2]), rb3(g[3]), rb3(g[4]));
                let n = cross3(&sub3(&s, &r), &sub3(&t, &r));
                let dir = sub3(&q, &p);
                let denom = dot3(&n, &dir);
                if denom == RBig::ZERO {
                    return None;
                }
                let tnum = dot3(&n, &sub3(&r, &p));
                let tt = tnum / denom;
                Some(add3(&p, &scale3(&dir, &tt)))
            }
            Spec::T(tris) => {
                let mut n = Vec::new();
                let mut d = Vec::new();
                for tri in tris {
                    let (a, b, c) = (rb3(tri[0]), rb3(tri[1]), rb3(tri[2]));
                    let nn = cross3(&sub3(&b, &a), &sub3(&c, &a));
                    let dd = dot3(&nn, &a);
                    n.push(nn);
                    d.push(dd);
                }
                let det3 = |m: [&[RBig; 3]; 3]| -> RBig {
                    &m[0][0] * (&m[1][1] * &m[2][2] - &m[1][2] * &m[2][1])
                        - &m[0][1] * (&m[1][0] * &m[2][2] - &m[1][2] * &m[2][0])
                        + &m[0][2] * (&m[1][0] * &m[2][1] - &m[1][1] * &m[2][0])
                };
                let den = det3([&n[0], &n[1], &n[2]]);
                if den == RBig::ZERO {
                    return None;
                }
                let col = |k: usize| -> RBig {
                    let mut m: Vec<[RBig; 3]> = n.clone();
                    for (row, mat) in m.iter_mut().enumerate() {
                        mat[k] = d[row].clone();
                    }
                    det3([&m[0], &m[1], &m[2]]) / &den
                };
                Some([col(0), col(1), col(2)])
            }
        }
    }
}

/// Independent truth: closed point-in-triangle via 3D vector algebra
/// (coplanar inputs): with `n = (b−a)×(c−a) ≠ 0`, `p` is inside iff each
/// edge cross product `(next−cur)×(p−cur)` has non-negative dot with `n`.
fn rbig_point_in_triangle(p: &[RBig; 3], a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]) -> bool {
    let n = cross3(&sub3(b, a), &sub3(c, a));
    if is_zero3(&n) {
        return false; // degenerate triangle
    }
    for (v0, v1) in [(a, b), (b, c), (c, a)] {
        let e = dot3(&cross3(&sub3(v1, v0), &sub3(p, v0)), &n);
        if e < RBig::ZERO {
            return false;
        }
    }
    true
}

/// Independent truth: proper open-segment crossing via the exact
/// parametric solve `a + s·u = p + t·w` (coplanar inputs): with
/// `D = u×w ≠ 0`, `s = ((p−a)×w)·D / |D|²`, `t = ((p−a)×u)·D / |D|²`;
/// proper cross iff `0 < s < 1`, `0 < t < 1` and the two parametric
/// points coincide (coplanarity witness).
fn rbig_inner_segments_cross(a: &[RBig; 3], b: &[RBig; 3], p: &[RBig; 3], q: &[RBig; 3]) -> bool {
    let u = sub3(b, a);
    let w = sub3(q, p);
    let d = cross3(&u, &w);
    let dd = dot3(&d, &d);
    if dd == RBig::ZERO {
        return false; // parallel or degenerate
    }
    let pa = sub3(p, a);
    let s = dot3(&cross3(&pa, &w), &d) / &dd;
    let t = dot3(&cross3(&pa, &u), &d) / &dd;
    let one = RBig::ONE;
    if !(s > RBig::ZERO && s < one && t > RBig::ZERO && t < one) {
        return false;
    }
    // Coplanarity witness: the two parametric points must coincide.
    let x1 = add3(a, &scale3(&u, &s));
    let x2 = add3(p, &scale3(&w, &t));
    x1 == x2
}

/// Independent truth: p on the open/closed segment via cross == 0 plus a
/// dot-product parameter range.
fn rbig_point_in_segment(p: &[RBig; 3], v1: &[RBig; 3], v2: &[RBig; 3], closed: bool) -> bool {
    let d = sub3(v2, v1);
    let w = sub3(p, v1);
    if !is_zero3(&cross3(&d, &w)) {
        return false;
    }
    let t_num = dot3(&w, &d);
    let len2 = dot3(&d, &d);
    if len2 == RBig::ZERO {
        return closed && is_zero3(&w);
    }
    if closed {
        t_num >= RBig::ZERO && t_num <= len2
    } else {
        t_num > RBig::ZERO && t_num < len2
    }
}

/// Pool of coplanar specs on the plane `o + i·e1 + j·e2`: explicit, LPI
/// and TPI representations of exact lattice points (mixed), spanning
/// inside/boundary/outside configurations for the composites.
fn planar_pool(o: Point3, e1: Point3, e2: Point3, off: Point3) -> Vec<Spec> {
    let at = |i: f64, j: f64| -> Point3 {
        Point3::new(
            o.x() + i * e1.x() + j * e2.x(),
            o.y() + i * e1.y() + j * e2.y(),
            o.z() + i * e1.z() + j * e2.z(),
        )
    };
    let add = |p: Point3, q: Point3| Point3::new(p.x() + q.x(), p.y() + q.y(), p.z() + q.z());
    let sub = |p: Point3, q: Point3| Point3::new(p.x() - q.x(), p.y() - q.y(), p.z() - q.z());
    let mut pool = Vec::new();
    let lattice: [(f64, f64); 12] = [
        (0.0, 0.0),
        (4.0, 0.0),
        (0.0, 4.0),
        (1.0, 1.0),
        (2.0, 0.0),
        (2.0, 2.0),
        (3.0, 3.0),
        (0.5, 0.25),
        (-1.0, 2.0),
        (1.0, -1.0),
        (2.0, 1.0),
        (0.25, 0.5),
    ];
    for (k, &(i, j)) in lattice.iter().enumerate() {
        let t = at(i, j);
        match k % 3 {
            0 => pool.push(Spec::E(t)),
            1 => {
                // LPI: line through t ± off, plane (o, o+e1, o+e2).
                pool.push(Spec::L([
                    add(t, off),
                    sub(t, off),
                    o,
                    add(o, e1),
                    add(o, e2),
                ]));
            }
            _ => {
                // TPI: base plane ∩ plane(t, t+off, t+e1) ∩ plane(t, t+off, t+e2) = t.
                pool.push(Spec::T([
                    [o, add(o, e1), add(o, e2)],
                    [t, add(t, off), add(t, e1)],
                    [t, add(t, off), add(t, e2)],
                ]));
            }
        }
    }
    pool
}

fn planar_pools() -> Vec<Vec<Spec>> {
    vec![
        // z = 0 plane.
        planar_pool(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(0.25, -0.5, 1.0),
        ),
        // Plane parallel to the z axis (xy projection collapses).
        planar_pool(
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 0.0, 1.0),
            Point3::new(0.5, 1.0, -0.25),
        ),
        // Generic tilted plane with dyadic basis.
        planar_pool(
            Point3::new(0.5, -0.25, 1.0),
            Point3::new(1.0, 0.5, 0.25),
            Point3::new(-0.5, 1.0, 0.5),
            Point3::new(0.25, 0.25, 1.0),
        ),
    ]
}

#[test]
fn point_in_triangle_matches_independent_rbig_formulation() {
    let mut checked = 0usize;
    for pool in planar_pools() {
        let n = pool.len();
        for k in 0..160usize {
            let p = (k * 7 + 1) % n;
            let a = (k * 13 + 2) % n;
            let b = (k * 29 + 5) % n;
            let c = (k * 53 + 7) % n;
            if a == b || b == c || a == c {
                continue;
            }
            let (xp, xa, xb, xc) = (
                pool[p].exact().unwrap(),
                pool[a].exact().unwrap(),
                pool[b].exact().unwrap(),
                pool[c].exact().unwrap(),
            );
            let truth = rbig_point_in_triangle(&xp, &xa, &xb, &xc);
            let got = point_in_triangle_indirect(
                &pool[p].to_native(),
                &pool[a].to_native(),
                &pool[b].to_native(),
                &pool[c].to_native(),
            );
            assert_eq!(
                got,
                truth,
                "point_in_triangle mismatch vs independent RBig truth on \
                 (p={p}, a={a}, b={b}, c={c}): {:?}",
                (&pool[p], &pool[a], &pool[b], &pool[c])
            );
            checked += 1;
        }
    }
    assert!(checked > 300, "corpus too small: {checked}");
}

#[test]
fn inner_segments_cross_matches_independent_rbig_formulation() {
    let mut checked = 0usize;
    for pool in planar_pools() {
        let n = pool.len();
        for k in 0..160usize {
            let a = (k * 7 + 1) % n;
            let b = (k * 13 + 3) % n;
            let p = (k * 29 + 5) % n;
            let q = (k * 53 + 8) % n;
            if a == b || p == q {
                continue;
            }
            let (xa, xb, xp, xq) = (
                pool[a].exact().unwrap(),
                pool[b].exact().unwrap(),
                pool[p].exact().unwrap(),
                pool[q].exact().unwrap(),
            );
            let truth = rbig_inner_segments_cross(&xa, &xb, &xp, &xq);
            let got = inner_segments_cross_indirect(
                &pool[a].to_native(),
                &pool[b].to_native(),
                &pool[p].to_native(),
                &pool[q].to_native(),
            );
            assert_eq!(
                got,
                truth,
                "inner_segments_cross mismatch vs independent RBig truth on \
                 (a={a}, b={b}, p={p}, q={q}): {:?}",
                (&pool[a], &pool[b], &pool[p], &pool[q])
            );
            checked += 1;
        }
    }
    assert!(checked > 300, "corpus too small: {checked}");
}

#[test]
fn point_in_segment_variants_match_independent_rbig_formulation() {
    let mut checked = 0usize;
    for pool in planar_pools() {
        let n = pool.len();
        for k in 0..160usize {
            let p = (k * 7 + 1) % n;
            let v1 = (k * 13 + 3) % n;
            let v2 = (k * 29 + 6) % n;
            if v1 == v2 {
                continue;
            }
            let (xp, x1, x2) = (
                pool[p].exact().unwrap(),
                pool[v1].exact().unwrap(),
                pool[v2].exact().unwrap(),
            );
            let (gp, g1, g2) = (
                pool[p].to_native(),
                pool[v1].to_native(),
                pool[v2].to_native(),
            );
            assert_eq!(
                point_in_inner_segment_indirect(&gp, &g1, &g2),
                rbig_point_in_segment(&xp, &x1, &x2, false),
                "point_in_inner_segment mismatch (p={p}, v1={v1}, v2={v2}): {:?}",
                (&pool[p], &pool[v1], &pool[v2])
            );
            assert_eq!(
                point_in_segment_indirect(&gp, &g1, &g2),
                rbig_point_in_segment(&xp, &x1, &x2, true),
                "point_in_segment mismatch (p={p}, v1={v1}, v2={v2}): {:?}",
                (&pool[p], &pool[v1], &pool[v2])
            );
            checked += 1;
        }
    }
    assert!(checked > 300, "corpus too small: {checked}");
}
