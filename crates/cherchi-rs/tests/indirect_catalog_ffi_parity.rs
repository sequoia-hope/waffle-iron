//! PR-CR-M7b oracle — differential parity of the clean-room predicate
//! catalog (orient2d projections, per-axis comparators, composites,
//! approx_lpi) against the FFI reference sidecar
//! (`indirect-predicates-sidecar-rs`, used strictly as a BLACK BOX: we
//! call its public functions and compare outputs; its C++ internals were
//! not consulted).
//!
//! Documented FFI surface quirks accounted for (both established by the
//! sidecar's own black-box smoke tests):
//!
//! - `less_than_on_*` on an explicit/explicit pair returns the C++ bool
//!   `a.c < b.c` as 0/1 (never −1). EE parity therefore checks the
//!   quirk mapping (native Negative ⟺ FFI Positive), not sign equality.
//! - `point_in_inner_segment` / `point_in_segment` inherit the EE
//!   comparator quirk and become endpoint-order-sensitive for explicit
//!   endpoints; consumers OR both orders (`enforce.rs`). Parity is
//!   asserted against `FFI(fwd) || FFI(rev)` — exactly the symmetric
//!   semantics our native composites implement directly.
#![cfg(feature = "indirect-predicates")]

mod indirect_common;

use cad_primitives::Point3;
use cherchi_rs::predicates::indirect::{
    approx_lpi, inner_segments_cross_indirect, less_than_on_x_indirect, less_than_on_y_indirect,
    less_than_on_z_indirect, orient2d_xy_indirect, orient2d_yz_indirect, orient2d_zx_indirect,
    point_in_inner_segment_indirect, point_in_segment_indirect, point_in_triangle_indirect,
    GenericPoint3D, Sign,
};
use indirect_common::*;
use indirect_predicates_sidecar_rs as ip;

/// Raw geometric description of one generic point, instantiable as both
/// a native `GenericPoint3D` and an FFI handle from the SAME generators.
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

    fn coords(&self) -> Vec<Point3> {
        match self {
            Spec::E(p) => vec![*p],
            Spec::L(g) => g.to_vec(),
            Spec::T(t) => t.iter().flatten().copied().collect(),
        }
    }

    fn is_explicit(&self) -> bool {
        matches!(self, Spec::E(_))
    }
}

enum Handle<'a> {
    E(&'a ip::ExplicitPoint3D),
    L(ip::ImplicitPoint3DLpi<'a>),
    T(ip::ImplicitPoint3DTpi<'a>),
}

macro_rules! with_h {
    ($h:expr, $b:ident, $body:expr) => {
        match $h {
            Handle::E(p) => {
                let $b = *p;
                $body
            }
            Handle::L(p) => {
                let $b = p;
                $body
            }
            Handle::T(p) => {
                let $b = p;
                $body
            }
        }
    };
}

/// Build the explicit-point arena + typed handles for a slice of specs,
/// then run `f` over the handles.
fn with_handles<R>(specs: &[&Spec], f: impl FnOnce(&[Handle<'_>]) -> R) -> R {
    cherchi_rs::arrangements::require_ffi_shim();
    ip::init_fpu();
    let mut pts: Vec<ip::ExplicitPoint3D> = Vec::new();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for s in specs {
        let start = pts.len();
        for p in s.coords() {
            pts.push(ip::ExplicitPoint3D::new(p.x(), p.y(), p.z()));
        }
        ranges.push((start, pts.len()));
    }
    let handles: Vec<Handle<'_>> = specs
        .iter()
        .zip(&ranges)
        .map(|(s, &(b, _))| match s {
            Spec::E(_) => Handle::E(&pts[b]),
            Spec::L(_) => Handle::L(ip::ImplicitPoint3DLpi::new(
                &pts[b],
                &pts[b + 1],
                &pts[b + 2],
                &pts[b + 3],
                &pts[b + 4],
            )),
            Spec::T(_) => Handle::T(ip::ImplicitPoint3DTpi::new(
                &pts[b],
                &pts[b + 1],
                &pts[b + 2],
                &pts[b + 3],
                &pts[b + 4],
                &pts[b + 5],
                &pts[b + 6],
                &pts[b + 7],
                &pts[b + 8],
            )),
        })
        .collect();
    f(&handles)
}

/// The FFI orient2d / less_than signs map onto ours IDENTICALLY (CCW →
/// Positive — pinned by the calibration anchors below; unlike orient3d,
/// which is mirrored).
fn map_ffi(s: ip::Sign) -> Sign {
    match s {
        ip::Sign::Positive => Sign::Positive,
        ip::Sign::Negative => Sign::Negative,
        ip::Sign::Zero => Sign::Zero,
        ip::Sign::Undefined => Sign::Undefined,
    }
}

// ---------------------------------------------------------------------
// orient2d projections
// ---------------------------------------------------------------------

fn ffi_orient2d(proj: usize, specs: &[&Spec; 3]) -> ip::Sign {
    with_handles(specs, |h| {
        with_h!(&h[0], a, {
            with_h!(&h[1], b, {
                with_h!(&h[2], c, {
                    match proj {
                        0 => ip::orient2d_xy(a, b, c),
                        1 => ip::orient2d_yz(a, b, c),
                        _ => ip::orient2d_zx(a, b, c),
                    }
                })
            })
        })
    })
}

fn native_orient2d(proj: usize, specs: &[&Spec; 3]) -> Sign {
    let n: Vec<GenericPoint3D> = specs.iter().map(|s| s.to_native()).collect();
    match proj {
        0 => orient2d_xy_indirect(&n[0], &n[1], &n[2]),
        1 => orient2d_yz_indirect(&n[0], &n[1], &n[2]),
        _ => orient2d_zx_indirect(&n[0], &n[1], &n[2]),
    }
}

fn assert_orient2d_parity(proj: usize, specs: &[&Spec; 3], label: &str) {
    let ours = native_orient2d(proj, specs);
    let theirs = map_ffi(ffi_orient2d(proj, specs));
    assert_eq!(
        ours, theirs,
        "REFERENCE PARITY MISMATCH (orient2d proj {proj}, {label}): native {ours:?} \
         vs FFI {theirs:?} on {specs:?}"
    );
}

/// Pin the sign convention: CCW explicit triple in each projection must
/// be Positive on BOTH sides (identity mapping).
#[test]
fn orient2d_calibration_anchor() {
    // xy: (0,0), (1,0), (0,1) CCW.
    let a = Spec::E(Point3::new(0.0, 0.0, 7.0));
    let b = Spec::E(Point3::new(1.0, 0.0, -3.0));
    let c = Spec::E(Point3::new(0.0, 1.0, 11.0));
    assert_eq!(native_orient2d(0, &[&a, &b, &c]), Sign::Positive);
    assert_eq!(map_ffi(ffi_orient2d(0, &[&a, &b, &c])), Sign::Positive);
    // yz CCW.
    let a = Spec::E(Point3::new(7.0, 0.0, 0.0));
    let b = Spec::E(Point3::new(-3.0, 1.0, 0.0));
    let c = Spec::E(Point3::new(11.0, 0.0, 1.0));
    assert_eq!(native_orient2d(1, &[&a, &b, &c]), Sign::Positive);
    assert_eq!(map_ffi(ffi_orient2d(1, &[&a, &b, &c])), Sign::Positive);
    // zx CCW.
    let a = Spec::E(Point3::new(0.0, 7.0, 0.0));
    let b = Spec::E(Point3::new(0.0, -3.0, 1.0));
    let c = Spec::E(Point3::new(1.0, 11.0, 0.0));
    assert_eq!(native_orient2d(2, &[&a, &b, &c]), Sign::Positive);
    assert_eq!(map_ffi(ffi_orient2d(2, &[&a, &b, &c])), Sign::Positive);
}

fn spec_pool() -> Vec<Spec> {
    let mut pool = Vec::new();
    for i in 0..6u64 {
        pool.push(Spec::E(point(200_000 + i)));
    }
    for i in 0..6u64 {
        pool.push(Spec::L(lpi_generators(i)));
    }
    for i in 0..6u64 {
        let (v, w, u) = tpi_generators(i);
        pool.push(Spec::T([v, w, u]));
    }
    pool
}

#[test]
fn orient2d_generic_mixed_parity() {
    let pool = spec_pool();
    let tuples = tuple_stream(pool.len(), 480);
    assert!(tuples.len() > 300, "corpus too small: {}", tuples.len());
    for (k, &[a, b, c, _]) in tuples.iter().enumerate() {
        let proj = k % 3;
        assert_orient2d_parity(
            proj,
            &[&pool[a], &pool[b], &pool[c]],
            &format!("generic [{a}, {b}, {c}]"),
        );
    }
}

/// Exact-degenerate family: implicit point exactly collinear (in the
/// projection) with the two explicit anchors → Zero on both sides.
#[test]
fn orient2d_exact_collinear_parity() {
    for k in 0..6u64 {
        let y = k as f64 - 2.0;
        // LPI at (2, y, 0).
        let l = Spec::L([
            Point3::new(2.0, y, 3.0),
            Point3::new(2.0, y, -1.0),
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ]);
        let a = Spec::E(Point3::new(0.0, y, 0.0));
        let b = Spec::E(Point3::new(4.0, y, 5.0));
        let native: Vec<GenericPoint3D> = [&l, &a, &b].iter().map(|s| s.to_native()).collect();
        assert_eq!(
            orient2d_xy_indirect(&native[0], &native[1], &native[2]),
            Sign::Zero,
            "collinear case {k} must be Zero"
        );
        assert_orient2d_parity(0, &[&l, &a, &b], &format!("collinear {k}"));
    }
}

#[test]
fn orient2d_undefined_d_parity() {
    let bad = Spec::L([
        Point3::new(0.0, 0.0, 1.0),
        Point3::new(1.0, 0.0, 1.0),
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    ]);
    let b = Spec::E(point(700));
    let c = Spec::E(point(710));
    for proj in 0..3 {
        let native: Vec<GenericPoint3D> = [&bad, &b, &c].iter().map(|s| s.to_native()).collect();
        let ours = match proj {
            0 => orient2d_xy_indirect(&native[0], &native[1], &native[2]),
            1 => orient2d_yz_indirect(&native[0], &native[1], &native[2]),
            _ => orient2d_zx_indirect(&native[0], &native[1], &native[2]),
        };
        assert_eq!(ours, Sign::Undefined, "proj {proj}: ours must be Undefined");
        assert_orient2d_parity(proj, &[&bad, &b, &c], "undefined-d");
    }
}

// ---------------------------------------------------------------------
// less_than_on_{x,y,z}
// ---------------------------------------------------------------------

fn ffi_less_than(axis: usize, specs: &[&Spec; 2]) -> ip::Sign {
    with_handles(specs, |h| {
        with_h!(&h[0], a, {
            with_h!(&h[1], b, {
                match axis {
                    0 => ip::less_than_on_x(a, b),
                    1 => ip::less_than_on_y(a, b),
                    _ => ip::less_than_on_z(a, b),
                }
            })
        })
    })
}

fn native_less_than(axis: usize, specs: &[&Spec; 2]) -> Sign {
    let n: Vec<GenericPoint3D> = specs.iter().map(|s| s.to_native()).collect();
    match axis {
        0 => less_than_on_x_indirect(&n[0], &n[1]),
        1 => less_than_on_y_indirect(&n[0], &n[1]),
        _ => less_than_on_z_indirect(&n[0], &n[1]),
    }
}

fn assert_less_than_parity(axis: usize, specs: &[&Spec; 2], label: &str) {
    let ours = native_less_than(axis, specs);
    let theirs = map_ffi(ffi_less_than(axis, specs));
    if specs[0].is_explicit() && specs[1].is_explicit() {
        // Documented EE quirk: FFI returns bool `a < b` (1 = Positive,
        // 0 = Zero, never Negative). Map: ours Negative ⟺ theirs
        // Positive; ours Zero/Positive ⟺ theirs Zero.
        let expected_theirs = if ours == Sign::Negative {
            Sign::Positive
        } else {
            Sign::Zero
        };
        assert_eq!(
            theirs, expected_theirs,
            "REFERENCE PARITY MISMATCH (less_than axis {axis}, {label}, EE-quirk \
             mapping): native {ours:?} vs FFI {theirs:?} on {specs:?}"
        );
    } else {
        assert_eq!(
            ours, theirs,
            "REFERENCE PARITY MISMATCH (less_than axis {axis}, {label}): native \
             {ours:?} vs FFI {theirs:?} on {specs:?}"
        );
    }
}

/// Pin the implicit-pair convention: an LPI at x = 0.25 vs an LPI at
/// x = 1.0 must be Negative ("first is less") on BOTH sides.
#[test]
fn less_than_calibration_anchor() {
    let mk = |x: f64, y: f64| {
        Spec::L([
            Point3::new(x, y, -1.0),
            Point3::new(x, y, 1.0),
            Point3::new(5.0, 0.5, 0.0),
            Point3::new(6.0, 1.5, 0.0),
            Point3::new(4.0, 3.0, 0.0),
        ])
    };
    let lo = mk(0.25, 0.5);
    let hi = mk(1.0, 0.25);
    assert_eq!(native_less_than(0, &[&lo, &hi]), Sign::Negative);
    assert_eq!(map_ffi(ffi_less_than(0, &[&lo, &hi])), Sign::Negative);
    assert_eq!(map_ffi(ffi_less_than(0, &[&hi, &lo])), Sign::Positive);
    // y: 0.5 vs 0.25 → Positive both sides.
    assert_eq!(native_less_than(1, &[&lo, &hi]), Sign::Positive);
    assert_eq!(map_ffi(ffi_less_than(1, &[&lo, &hi])), Sign::Positive);
    // z: exactly equal (both 0) → Zero both sides.
    assert_eq!(native_less_than(2, &[&lo, &hi]), Sign::Zero);
    assert_eq!(map_ffi(ffi_less_than(2, &[&lo, &hi])), Sign::Zero);
}

#[test]
fn less_than_generic_mixed_parity() {
    let pool = spec_pool();
    let tuples = tuple_stream(pool.len(), 480);
    for (k, &[a, b, _, _]) in tuples.iter().enumerate() {
        let axis = k % 3;
        assert_less_than_parity(axis, &[&pool[a], &pool[b]], &format!("generic [{a}, {b}]"));
    }
}

/// Exact ties via different generators: same geometric coordinate from
/// two distinct LPI constructions, and LPI-vs-explicit.
#[test]
fn less_than_exact_tie_parity() {
    for k in 0..6u64 {
        let x = coord(40 + k);
        let y = coord(50 + k);
        let l1 = Spec::L([
            Point3::new(x, y, -1.0),
            Point3::new(x, y, 1.0),
            Point3::new(5.0, 0.5, 0.0),
            Point3::new(6.0, 1.5, 0.0),
            Point3::new(4.0, 3.0, 0.0),
        ]);
        let l2 = Spec::L([
            Point3::new(x, y, -2.0),
            Point3::new(x, y, 3.0),
            Point3::new(7.0, -0.5, 0.0),
            Point3::new(9.0, 1.0, 0.0),
            Point3::new(6.0, 4.0, 0.0),
        ]);
        let e = Spec::E(Point3::new(x, y + 1.0, 4.0));
        for axis in 0..3 {
            assert_less_than_parity(axis, &[&l1, &l2], &format!("LPI/LPI tie {k}"));
        }
        assert_less_than_parity(0, &[&l1, &e], &format!("LPI/E x-tie {k}"));
        let native = native_less_than(0, &[&l1, &l2]);
        assert_eq!(native, Sign::Zero, "tie {k}: native must be Zero on x");
    }
}

#[test]
fn less_than_undefined_d_parity() {
    let bad = Spec::L([
        Point3::new(0.0, 0.0, 1.0),
        Point3::new(1.0, 0.0, 1.0),
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    ]);
    let e = Spec::E(point(720));
    for axis in 0..3 {
        let ours = native_less_than(axis, &[&bad, &e]);
        assert_eq!(ours, Sign::Undefined, "axis {axis}: ours must be Undefined");
        assert_less_than_parity(axis, &[&bad, &e], "undefined-d");
    }
}

// ---------------------------------------------------------------------
// Composites: point_in_triangle
// ---------------------------------------------------------------------

fn ffi_point_in_triangle(specs: &[&Spec; 4]) -> bool {
    with_handles(specs, |h| {
        with_h!(&h[0], p, {
            with_h!(&h[1], a, {
                with_h!(&h[2], b, {
                    with_h!(&h[3], c, ip::point_in_triangle(p, a, b, c))
                })
            })
        })
    })
}

/// Coplanar pool on the plane spanned by (o, e1, e2): explicit + LPI +
/// TPI representations of exact lattice points (same construction as the
/// soundness oracle's planar pools).
fn coplanar_specs(o: Point3, e1: Point3, e2: Point3, off: Point3) -> Vec<Spec> {
    let at = |i: f64, j: f64| -> Point3 {
        Point3::new(
            o.x() + i * e1.x() + j * e2.x(),
            o.y() + i * e1.y() + j * e2.y(),
            o.z() + i * e1.z() + j * e2.z(),
        )
    };
    let add = |p: Point3, q: Point3| Point3::new(p.x() + q.x(), p.y() + q.y(), p.z() + q.z());
    let sub = |p: Point3, q: Point3| Point3::new(p.x() - q.x(), p.y() - q.y(), p.z() - q.z());
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
    let mut pool = Vec::new();
    for (k, &(i, j)) in lattice.iter().enumerate() {
        let t = at(i, j);
        match k % 3 {
            0 => pool.push(Spec::E(t)),
            1 => pool.push(Spec::L([
                add(t, off),
                sub(t, off),
                o,
                add(o, e1),
                add(o, e2),
            ])),
            _ => pool.push(Spec::T([
                [o, add(o, e1), add(o, e2)],
                [t, add(t, off), add(t, e1)],
                [t, add(t, off), add(t, e2)],
            ])),
        }
    }
    pool
}

fn coplanar_pools() -> Vec<Vec<Spec>> {
    vec![
        coplanar_specs(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(0.25, -0.5, 1.0),
        ),
        coplanar_specs(
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 0.0, 1.0),
            Point3::new(0.5, 1.0, -0.25),
        ),
        coplanar_specs(
            Point3::new(0.5, -0.25, 1.0),
            Point3::new(1.0, 0.5, 0.25),
            Point3::new(-0.5, 1.0, 0.5),
            Point3::new(0.25, 0.25, 1.0),
        ),
    ]
}

#[test]
fn point_in_triangle_coplanar_parity() {
    let mut checked = 0usize;
    for pool in coplanar_pools() {
        let n = pool.len();
        for k in 0..120usize {
            let p = (k * 7 + 1) % n;
            let a = (k * 13 + 2) % n;
            let b = (k * 29 + 5) % n;
            let c = (k * 53 + 7) % n;
            if a == b || b == c || a == c {
                continue;
            }
            // Skip degenerate (collinear) triangles: the FFI's behavior
            // there is unspecified; consumers never pass them.
            let nat: Vec<GenericPoint3D> = [&pool[a], &pool[b], &pool[c]]
                .iter()
                .map(|s| s.to_native())
                .collect();
            let degenerate = [
                orient2d_xy_indirect(&nat[0], &nat[1], &nat[2]),
                orient2d_yz_indirect(&nat[0], &nat[1], &nat[2]),
                orient2d_zx_indirect(&nat[0], &nat[1], &nat[2]),
            ]
            .iter()
            .all(|s| *s == Sign::Zero);
            if degenerate {
                continue;
            }
            let pp = pool[p].to_native();
            let ours = point_in_triangle_indirect(&pp, &nat[0], &nat[1], &nat[2]);
            let theirs = ffi_point_in_triangle(&[&pool[p], &pool[a], &pool[b], &pool[c]]);
            assert_eq!(
                ours,
                theirs,
                "REFERENCE PARITY MISMATCH (point_in_triangle, p={p} a={a} b={b} c={c}): \
                 native {ours} vs FFI {theirs} on {:?}",
                (&pool[p], &pool[a], &pool[b], &pool[c])
            );
            checked += 1;
        }
    }
    assert!(checked > 250, "corpus too small: {checked}");
}

// ---------------------------------------------------------------------
// Composites: inner_segments_cross + point_in_{inner_,}segment
// ---------------------------------------------------------------------

fn ffi_inner_segments_cross(specs: &[&Spec; 4]) -> bool {
    with_handles(specs, |h| {
        with_h!(&h[0], a, {
            with_h!(&h[1], b, {
                with_h!(&h[2], p, {
                    with_h!(&h[3], q, ip::inner_segments_cross(a, b, p, q))
                })
            })
        })
    })
}

/// FFI point_in_inner_segment / point_in_segment, OR-ed over both
/// endpoint orders (the consumer-level workaround for the documented EE
/// order-sensitivity — symmetric semantics on the FFI side too).
fn ffi_point_in_inner_segment_sym(specs: &[&Spec; 3]) -> bool {
    with_handles(specs, |h| {
        with_h!(&h[0], p, {
            with_h!(&h[1], v1, {
                with_h!(&h[2], v2, {
                    ip::point_in_inner_segment(p, v1, v2) || ip::point_in_inner_segment(p, v2, v1)
                })
            })
        })
    })
}

fn ffi_point_in_segment_sym(specs: &[&Spec; 3]) -> bool {
    with_handles(specs, |h| {
        with_h!(&h[0], p, {
            with_h!(&h[1], v1, {
                with_h!(&h[2], v2, {
                    ip::point_in_segment(p, v1, v2) || ip::point_in_segment(p, v2, v1)
                })
            })
        })
    })
}

#[test]
fn inner_segments_cross_coplanar_parity() {
    let mut checked = 0usize;
    for pool in coplanar_pools() {
        let n = pool.len();
        for k in 0..120usize {
            let a = (k * 7 + 1) % n;
            let b = (k * 13 + 3) % n;
            let p = (k * 29 + 5) % n;
            let q = (k * 53 + 8) % n;
            if a == b || p == q {
                continue;
            }
            let nat: Vec<GenericPoint3D> = [&pool[a], &pool[b], &pool[p], &pool[q]]
                .iter()
                .map(|s| s.to_native())
                .collect();
            let ours = inner_segments_cross_indirect(&nat[0], &nat[1], &nat[2], &nat[3]);
            let theirs = ffi_inner_segments_cross(&[&pool[a], &pool[b], &pool[p], &pool[q]]);
            assert_eq!(
                ours,
                theirs,
                "REFERENCE PARITY MISMATCH (inner_segments_cross, a={a} b={b} p={p} q={q}): \
                 native {ours} vs FFI {theirs} on {:?}",
                (&pool[a], &pool[b], &pool[p], &pool[q])
            );
            checked += 1;
        }
    }
    assert!(checked > 250, "corpus too small: {checked}");
}

#[test]
fn point_in_segment_variants_coplanar_parity() {
    let mut checked = 0usize;
    for pool in coplanar_pools() {
        let n = pool.len();
        for k in 0..120usize {
            let p = (k * 7 + 1) % n;
            let v1 = (k * 13 + 3) % n;
            let v2 = (k * 29 + 6) % n;
            if v1 == v2 {
                continue;
            }
            let nat: Vec<GenericPoint3D> = [&pool[p], &pool[v1], &pool[v2]]
                .iter()
                .map(|s| s.to_native())
                .collect();
            let specs = [&pool[p], &pool[v1], &pool[v2]];
            let ours_open = point_in_inner_segment_indirect(&nat[0], &nat[1], &nat[2]);
            let theirs_open = ffi_point_in_inner_segment_sym(&specs);
            assert_eq!(
                ours_open,
                theirs_open,
                "REFERENCE PARITY MISMATCH (point_in_inner_segment, p={p} v1={v1} v2={v2}): \
                 {:?}",
                (&pool[p], &pool[v1], &pool[v2])
            );
            let ours_closed = point_in_segment_indirect(&nat[0], &nat[1], &nat[2]);
            let theirs_closed = ffi_point_in_segment_sym(&specs);
            assert_eq!(
                ours_closed,
                theirs_closed,
                "REFERENCE PARITY MISMATCH (point_in_segment, p={p} v1={v1} v2={v2}): {:?}",
                (&pool[p], &pool[v1], &pool[v2])
            );
            checked += 1;
        }
    }
    assert!(checked > 250, "corpus too small: {checked}");
}

/// On-segment / endpoint exact-degenerate family across representations:
/// midpoints and endpoints of lattice segments, as E / L / T specs.
#[test]
fn point_in_segment_exact_degenerate_parity() {
    let pools = coplanar_pools();
    for pool in &pools {
        // pool[0] is E at (0,0), pool[1] is L at (4,0), pool[4] is L at
        // (2,0) — the exact midpoint of the segment (pool[0], pool[1]).
        let mid = [&pool[4], &pool[0], &pool[1]];
        let nat: Vec<GenericPoint3D> = mid.iter().map(|s| s.to_native()).collect();
        assert!(
            point_in_inner_segment_indirect(&nat[0], &nat[1], &nat[2]),
            "midpoint must be strictly inside"
        );
        assert_eq!(
            point_in_inner_segment_indirect(&nat[0], &nat[1], &nat[2]),
            ffi_point_in_inner_segment_sym(&mid),
            "midpoint parity"
        );
        // Endpoint: excluded from the open segment, included in closed.
        let endp = [&pool[0], &pool[0], &pool[1]];
        let nat: Vec<GenericPoint3D> = endp.iter().map(|s| s.to_native()).collect();
        assert!(!point_in_inner_segment_indirect(&nat[0], &nat[1], &nat[2]));
        assert!(point_in_segment_indirect(&nat[0], &nat[1], &nat[2]));
        assert_eq!(
            point_in_inner_segment_indirect(&nat[0], &nat[1], &nat[2]),
            ffi_point_in_inner_segment_sym(&endp),
            "endpoint open parity"
        );
        assert_eq!(
            point_in_segment_indirect(&nat[0], &nat[1], &nat[2]),
            ffi_point_in_segment_sym(&endp),
            "endpoint closed parity"
        );
    }
}

// ---------------------------------------------------------------------
// approx_lpi vs the FFI lambda3d_lpi_interval midpoints
// ---------------------------------------------------------------------

#[test]
fn approx_lpi_matches_ffi_interval_midpoints() {
    cherchi_rs::arrangements::require_ffi_shim();
    ip::init_fpu();
    let iv = |pt: Point3| -> [ip::IntervalNumber; 3] {
        [
            ip::IntervalNumber::point(pt.x()),
            ip::IntervalNumber::point(pt.y()),
            ip::IntervalNumber::point(pt.z()),
        ]
    };
    let mid = |n: ip::IntervalNumber| -> f64 { (n.inf + n.sup) / 2.0 };
    let mut checked = 0usize;
    for k in 0..32u64 {
        let g = lpi_generators(k);
        let res = ip::lambda3d_lpi_interval(iv(g[0]), iv(g[1]), iv(g[2]), iv(g[3]), iv(g[4]));
        let d = mid(res.lambda_d);
        let ours = approx_lpi(g[0], g[1], g[2], g[3], g[4]);
        if d == 0.0 {
            assert_eq!(ours, None, "case {k}: FFI degenerate, ours must be None");
            continue;
        }
        let theirs = Point3::new(
            mid(res.lambda_x) / d,
            mid(res.lambda_y) / d,
            mid(res.lambda_z) / d,
        );
        let ours = ours.unwrap_or_else(|| panic!("case {k}: ours must be Some"));
        let scale = theirs
            .x()
            .abs()
            .max(theirs.y().abs())
            .max(theirs.z().abs())
            .max(1.0);
        for (o, t) in [
            (ours.x(), theirs.x()),
            (ours.y(), theirs.y()),
            (ours.z(), theirs.z()),
        ] {
            assert!(
                (o - t).abs() <= 1e-12 * scale,
                "case {k}: approx_lpi component {o} vs FFI {t} (scale {scale})"
            );
        }
        checked += 1;
    }
    assert!(checked >= 20, "too many degenerate cases: {checked}");
}
