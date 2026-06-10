//! PR-CR-M7a oracle — differential parity of the clean-room pure-Rust
//! indirect `orient3d` against the FFI reference sidecar
//! (`indirect-predicates-sidecar-rs`, used strictly as a BLACK BOX: we
//! call its public functions and compare outputs; its C++ internals were
//! not consulted).
//!
//! Reference parity is load-bearing (CLAUDE.md "Reference parity is not
//! optional"): the soundness oracle proves our two tiers agree with EACH
//! OTHER; only a differential test against the independent reference can
//! catch a mis-derived lambda or determinant that is internally
//! consistent but wrong.
#![cfg(feature = "indirect-predicates")]

mod indirect_common;

use cad_primitives::Point3;
use cherchi_rs::predicates::indirect::{orient3d_indirect, GenericPoint3D, Sign};
use indirect_common::*;
use indirect_predicates_sidecar_rs as ip;

/// Raw geometric description of one generic point, instantiable as both
/// a native `GenericPoint3D` and an FFI handle from the SAME generator
/// coordinates.
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

/// Evaluate the reference (FFI) orient3d on the four specs.
fn ffi_orient3d(specs: &[&Spec; 4]) -> ip::Sign {
    cherchi_rs::arrangements::require_ffi_shim();
    ip::init_fpu();
    // Build all explicit handles first (implicit handles borrow them).
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
    let (h0, h1, h2, h3) = (&handles[0], &handles[1], &handles[2], &handles[3]);
    with_h!(h0, a, with_h!(h1, b, with_h!(h2, c, with_h!(h3, d, ip::orient3d(a, b, c, d)))))
}

/// Map the reference sidecar's sign onto our convention.
///
/// Established EMPIRICALLY (black-box): on explicit non-degenerate input
/// the sidecar's `orient3d` agrees with Shewchuk's convention (Positive
/// = 4th point below the CCW plane), which is also ours — the mapping is
/// the identity. The `calibration_anchor` test pins this down; if it
/// ever fails, the mapping constant here is what must be re-examined.
fn map_ffi(s: ip::Sign) -> Sign {
    match s {
        ip::Sign::Positive => Sign::Positive,
        ip::Sign::Negative => Sign::Negative,
        ip::Sign::Zero => Sign::Zero,
        ip::Sign::Undefined => Sign::Undefined,
    }
}

fn assert_parity(specs: &[&Spec; 4], label: &str) {
    let native: Vec<GenericPoint3D> = specs.iter().map(|s| s.to_native()).collect();
    let ours = orient3d_indirect(&native[0], &native[1], &native[2], &native[3]);
    let theirs = map_ffi(ffi_orient3d(specs));
    assert_eq!(
        ours, theirs,
        "REFERENCE PARITY MISMATCH ({label}): native {ours:?} vs FFI {theirs:?} \
         on specs {specs:?}"
    );
}

/// Pin the FFI↔native sign-convention mapping on a hand-derived case:
/// a=(0,0,0), b=(1,0,0), c=(0,1,0), d=(0,0,1): det[a−d;b−d;c−d] = −1 →
/// our Negative (d above the CCW plane).
#[test]
fn calibration_anchor() {
    let a = Spec::E(Point3::new(0.0, 0.0, 0.0));
    let b = Spec::E(Point3::new(1.0, 0.0, 0.0));
    let c = Spec::E(Point3::new(0.0, 1.0, 0.0));
    let d = Spec::E(Point3::new(0.0, 0.0, 1.0));
    assert_eq!(
        map_ffi(ffi_orient3d(&[&a, &b, &c, &d])),
        Sign::Negative,
        "FFI sign-convention mapping in map_ffi() is mis-calibrated"
    );
    assert_parity(&[&a, &b, &c, &d], "calibration");
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

// ---------------------------------------------------------------------
// Generic mixed corpus: explicit / LPI / TPI in all arities
// ---------------------------------------------------------------------

#[test]
fn generic_mixed_parity() {
    let pool = spec_pool();
    let tuples = tuple_stream(pool.len(), 480);
    assert!(tuples.len() > 400, "corpus too small: {}", tuples.len());
    for &[a, b, c, d] in &tuples {
        assert_parity(
            &[&pool[a], &pool[b], &pool[c], &pool[d]],
            &format!("generic [{a}, {b}, {c}, {d}]"),
        );
    }
}

// ---------------------------------------------------------------------
// Exact-degenerate family: implicit point exactly ON the query plane
// ---------------------------------------------------------------------

#[test]
fn exact_on_plane_parity() {
    // LPI of a vertical line with a generic triangle in the plane z = k:
    // the point lies EXACTLY on the plane z = k queried below → Zero on
    // both sides (forces both implementations into their exact tiers).
    for k in 0..6u64 {
        let z = k as f64 - 2.0;
        let lpi = Spec::L([
            Point3::new(0.25 + 0.5 * k as f64, 0.375, z - 1.0),
            Point3::new(0.25 + 0.5 * k as f64, 0.375, z + 1.0),
            Point3::new(5.0, 0.5, z),
            Point3::new(6.0, 1.5, z),
            Point3::new(4.0, 3.0, z),
        ]);
        let a = Spec::E(Point3::new(0.0, 0.0, z));
        let b = Spec::E(Point3::new(1.0, 0.0, z));
        let c = Spec::E(Point3::new(0.0, 1.0, z));
        let native = [
            lpi.to_native(),
            a.to_native(),
            b.to_native(),
            c.to_native(),
        ];
        // Hand truth: coplanar → Zero.
        assert_eq!(
            orient3d_indirect(&native[1], &native[2], &native[3], &native[0]),
            Sign::Zero,
            "exact-degenerate case {k} must be Zero"
        );
        assert_parity(&[&a, &b, &c, &lpi], &format!("on-plane {k}"));
    }
}

// ---------------------------------------------------------------------
// Off-by-ulps family around the exact-degenerate configuration
// ---------------------------------------------------------------------

#[test]
fn near_plane_ulp_parity() {
    for n in -3i32..=3 {
        let mut z = 1.0f64;
        for _ in 0..n.abs() {
            z = if n > 0 { z.next_up() } else { z.next_down() };
        }
        let lpi = Spec::L([
            Point3::new(0.3, 0.4, 0.0),
            Point3::new(0.3, 0.4, 2.0),
            Point3::new(5.0, 0.5, z),
            Point3::new(6.0, 1.5, z),
            Point3::new(4.0, 3.0, z),
        ]);
        let a = Spec::E(Point3::new(0.0, 0.0, 1.0));
        let b = Spec::E(Point3::new(1.0, 0.0, 1.0));
        let c = Spec::E(Point3::new(0.0, 1.0, 1.0));
        assert_parity(&[&a, &b, &c, &lpi], &format!("ulp {n}"));
    }
}

// ---------------------------------------------------------------------
// Undefined-d family
// ---------------------------------------------------------------------

#[test]
fn undefined_d_parity() {
    // Line exactly parallel to the plane → d == 0 → the implicit point
    // does not exist. Ours returns Undefined (Attene §5.3); the
    // reference must agree.
    for k in 0..4u64 {
        let dz = 1.5 + k as f64;
        let bad = Spec::L([
            Point3::new(coord(80 + k), coord(81 + k), dz),
            Point3::new(coord(82 + k), coord(83 + k), dz),
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ]);
        let b = Spec::E(point(700 + k));
        let c = Spec::E(point(710 + k));
        let d = Spec::E(point(720 + k));
        let native = [bad.to_native(), b.to_native(), c.to_native(), d.to_native()];
        assert_eq!(
            orient3d_indirect(&native[0], &native[1], &native[2], &native[3]),
            Sign::Undefined,
            "undefined-d case {k}: ours must be Undefined"
        );
        assert_parity(&[&bad, &b, &c, &d], &format!("undefined-d {k}"));
    }
}
