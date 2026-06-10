//! Shared deterministic corpus builders for the indirect-predicate
//! oracles (`indirect_filter_soundness.rs`, `indirect_ffi_parity.rs`).
//!
//! Everything here is random-free: coordinates come from a fixed integer
//! mixing function quantized to dyadic fractions, so corpora are
//! byte-stable across runs and platforms.

use cad_primitives::Point3;
use cherchi_rs::predicates::indirect::GenericPoint3D;

/// Deterministic "generic" coordinate: Knuth multiplicative hash of `i`,
/// quantized to multiples of 1/64 in roughly [-8, 8]. Dyadic (exactly
/// representable), scattered, and free of intentional degeneracies.
pub fn coord(i: u64) -> f64 {
    let h = i.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(31);
    ((h % 1024) as f64) / 64.0 - 8.0
}

/// Deterministic generic point with seed `i`.
pub fn point(i: u64) -> Point3 {
    Point3::new(coord(3 * i + 1), coord(3 * i + 2), coord(3 * i + 3))
}

/// Raw generators for one LPI point (line p→q, plane r,s,t), seeded.
/// Generic seeds make degenerate configurations (line parallel to plane)
/// measure-zero; corpus tests additionally tolerate `Undefined` results.
pub fn lpi_generators(seed: u64) -> [Point3; 5] {
    let b = 1000 + 7 * seed;
    [
        point(b),
        point(b + 1),
        point(b + 2),
        point(b + 3),
        point(b + 4),
    ]
}

/// Raw generators for one TPI point (triangles v, w, u), seeded.
pub fn tpi_generators(seed: u64) -> ([Point3; 3], [Point3; 3], [Point3; 3]) {
    let b = 50_000 + 11 * seed;
    (
        [point(b), point(b + 1), point(b + 2)],
        [point(b + 3), point(b + 4), point(b + 5)],
        [point(b + 6), point(b + 7), point(b + 8)],
    )
}

/// Scale a point's coordinates by `s` (exact when `s` is a power of two).
pub fn scale_point(p: Point3, s: f64) -> Point3 {
    Point3::new(p.x() * s, p.y() * s, p.z() * s)
}

/// A mixed pool of generic `GenericPoint3D`s: `n_e` explicit, `n_l` LPI,
/// `n_t` TPI, all coordinates scaled by `s`.
pub fn mixed_pool(n_e: usize, n_l: usize, n_t: usize, s: f64) -> Vec<GenericPoint3D> {
    let mut pool = Vec::new();
    for i in 0..n_e {
        pool.push(GenericPoint3D::explicit(scale_point(
            point(200_000 + i as u64),
            s,
        )));
    }
    for i in 0..n_l {
        let g = lpi_generators(i as u64);
        pool.push(GenericPoint3D::lpi(
            scale_point(g[0], s),
            scale_point(g[1], s),
            scale_point(g[2], s),
            scale_point(g[3], s),
            scale_point(g[4], s),
        ));
    }
    for i in 0..n_t {
        let (v, w, u) = tpi_generators(i as u64);
        let sc = |t: [Point3; 3]| [scale_point(t[0], s), scale_point(t[1], s), scale_point(t[2], s)];
        pool.push(GenericPoint3D::tpi(sc(v), sc(w), sc(u)));
    }
    pool
}

/// Deterministic stream of index 4-tuples over a pool of size `n`,
/// produced by stepping coprime strides — covers many type mixes
/// without RNG.
pub fn tuple_stream(n: usize, count: usize) -> Vec<[usize; 4]> {
    let mut out = Vec::with_capacity(count);
    let mut a = 0usize;
    for k in 0..count {
        a = (a + 7) % n;
        let b = (a + 1 + (k * 13) % (n - 1)) % n;
        let c = (a + 2 + (k * 29) % (n - 1)) % n;
        let d = (a + 3 + (k * 53) % (n - 1)) % n;
        // Skip tuples with repeated indices (degenerate by construction;
        // they are covered by the near-degenerate families instead).
        if a != b && a != c && a != d && b != c && b != d && c != d {
            out.push([a, b, c, d]);
        }
    }
    out
}
