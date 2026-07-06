//! M8 holed-disc increment 3 — ULP-twin CDT boundary conformality (RED).
//!
//! Spec: `specs/m8_holed_disc_coplanar_overlay.md` §8 increment 3.
//!
//! Fixture: the REAL Stage-1 CDT input for the tube's annular bottom cap in
//! the `annular_cap_under_disc` yang-rs fixture, captured bit-exactly
//! (`YANG_CDT_PROBE`). The outer ring (r=1.5) and bore ring (r=0.5) each
//! carry ULP-TWIN vertex pairs — adjacent boundary vertices whose coordinates
//! differ by 1 ULP (minted by the coplanar overlay's femto-close sweep
//! events, propagated to the opposite rims by exact axial projection). The
//! twins create femto-slivers along the boundary whose f64 CENTROID PARITY
//! misclassifies (the F0047 "parity slitting" class): the parity-based
//! `cdt_polygon_with_holes` emits a triangle set that uses constrained
//! boundary edges 0× or 2× — a non-conformal, self-overlapping cap that
//! downstream turns into a non-manifold Stage-0 mesh.
//!
//! Oracle (complete boundary conformality of the kept set):
//!   every constraint (boundary-loop) edge is used by EXACTLY ONE kept
//!   triangle, and every other edge by EXACTLY TWO. This holds iff the kept
//!   set tiles the annulus exactly (no slit, no overlap, no leak into the
//!   hole or the exterior).
//!
//! Target: `cdt_polygon_with_holes_floodfill` (the topological-outer variant
//! kernel-v2 already uses). RED: its HOLE exclusion is still f64 centroid
//! parity, which misclassifies the bore-rim twin slivers. GREEN: exact
//! rational hole parity (dashu `RBig` — f64 inputs are exact rationals).

use cad_primitives::Point2;
use cherchi_rs::triangulation::cdt_polygon_with_holes_floodfill;
use std::collections::HashMap;

const ANNULUS_TWIN_VERT_BITS: [(u64, u64); 86] = [
    (0x3ff8000000000000, 0x0),
    (0x3ff5403de80570af, 0x3fe64e84b1cdd35c),
    (0x3ff3a5dd26c890dc, 0x3feb90afb1ff9ea6),
    (0x3ff1809de46d4778, 0x3ff06be770d10da2),
    (0x3feb44612dfa99a7, 0x3ff3c069b1fb92de),
    (0x3fe64d8e7fb4c4a9, 0x3ff5407e8191a928),
    (0x3fe64d8e7fb4c49e, 0x3ff5407e8191a92b),
    (0x3fe07a46cee3c4be, 0x3ff68aa38576646d),
    (0x3fdd23d167fea057, 0x3ff6de1dd6fed6c9),
    (0x3fdd23d167fea056, 0x3ff6de1dd6fed6c9),
    (0x3fd2748a418e3b62, 0x3ff78d66395f2d4b),
    (0x3fd2748a418e3b58, 0x3ff78d66395f2d4c),
    (0x3fc7249e70bfaf20, 0x3ff7d3340cf0cd8f),
    (0x3fc29e0fb2dc641a, 0x3ff7e30c3860482e),
    (0x3fc29e0fb2dc6411, 0x3ff7e30c3860482e),
    (0x3faf5dcb9dad19d6, 0x3ff7fadfa6eff75a),
    (0x3faf5dcb9dad19c7, 0x3ff7fadfa6eff759),
    (0xbfc75fb7daced2c4, 0x3ff7d24d41fbfcc3),
    (0xbfc75fb7daced2cc, 0x3ff7d24d41fbfcc3),
    (0xbfd8707fdca81942, 0x3ff73591d1d3a563),
    (0xbfd8707fdca81943, 0x3ff73591d1d3a563),
    (0xbfdba37d5893c3b1, 0x3ff6fbdc8cab494e),
    (0xbfdba37d5893c3b9, 0x3ff6fbdc8cab494c),
    (0xbfdf4a0940b45d0e, 0x3ff6b08078397e66),
    (0xbfdf4a0940b45d0f, 0x3ff6b08078397e66),
    (0xbfe1056285a8c636, 0x3ff670bd633580f5),
    (0xbfed8d28bb9a0862, 0x3ff2e992a7e96a75),
    (0xbfed8d28bb9a0863, 0x3ff2e992a7e96a75),
    (0xbff1f6d99c02628a, 0x3fefd47383724746),
    (0xbff2e22603818a90, 0x3feda0206df82854),
    (0xbff74d776e43edc0, 0x3fd6f96bf8d36a4d),
    (0xbff74d776e43edc1, 0xbfd6f96bf8d36a46),
    (0xbff2e22603818a8e, 0xbfeda0206df82855),
    (0xbff1f6d99c02628a, 0xbfefd47383724743),
    (0xbfed8d28bb9a0863, 0xbff2e992a7e96a76),
    (0xbfed8d28bb9a0862, 0xbff2e992a7e96a76),
    (0xbfe1056285a8c63c, 0xbff670bd633580f4),
    (0xbfdf4a0940b45d0f, 0xbff6b08078397e66),
    (0xbfdf4a0940b45d0e, 0xbff6b08078397e66),
    (0xbfdba37d5893c3b9, 0xbff6fbdc8cab494c),
    (0xbfdba37d5893c3b1, 0xbff6fbdc8cab494e),
    (0xbfd8707fdca81943, 0xbff73591d1d3a563),
    (0xbfd8707fdca81942, 0xbff73591d1d3a563),
    (0xbfc75fb7daced2ca, 0xbff7d24d41fbfcc3),
    (0xbfc75fb7daced2c2, 0xbff7d24d41fbfcc3),
    (0x3faf5dcb9dad19c7, 0xbff7fadfa6eff75a),
    (0x3faf5dcb9dad19d6, 0xbff7fadfa6eff75a),
    (0x3fc29e0fb2dc6410, 0xbff7e30c3860482e),
    (0x3fc29e0fb2dc641a, 0xbff7e30c3860482e),
    (0x3fc7249e70bfaf2a, 0xbff7d3340cf0cd8f),
    (0x3fd2748a418e3b58, 0xbff78d66395f2d4c),
    (0x3fd2748a418e3b63, 0xbff78d66395f2d4c),
    (0x3fdd23d167fea056, 0xbff6de1dd6fed6c9),
    (0x3fdd23d167fea057, 0xbff6de1dd6fed6c9),
    (0x3fe07a46cee3c4be, 0xbff68aa38576646b),
    (0x3fe64d8e7fb4c49e, 0xbff5407e8191a92b),
    (0x3fe64d8e7fb4c4a9, 0xbff5407e8191a928),
    (0x3feb44612dfa9998, 0xbff3c069b1fb92e2),
    (0x3ff1809de46d4778, 0xbff06be770d10da4),
    (0x3ff3a5dd26c890dc, 0xbfeb90afb1ff9ea7),
    (0x3ff5403de80570b0, 0xbfe64e84b1cdd35b),
    (0x3fe0000000000000, 0x0),
    (0x3fdc55a7e00740e9, 0x3fcdbe064267c47b),
    (0x3fd22d961ea7111a, 0x3fda55e242a4c3d2),
    (0x3fc7d4d23376f142, 0x3fddb2e28bbd51a3),
    (0x3fc7d4d23376f137, 0x3fddb2e28bbd51a5),
    (0x3fc3089b2619b7bb, 0x3fde8d55ee14a578),
    (0x3fc3089b2619b7b2, 0x3fde8d55ee14a57a),
    (0x3faedb7debaa3ed5, 0x3fdfc44566966769),
    (0xbfc6b1d8b2365d9e, 0x3fddeba72ef20147),
    (0xbfd7f3ccd0032e0d, 0x3fd5384d024c2f84),
    (0xbfdc0b23faa08311, 0x3fced3770bef8554),
    (0xbfdc0b23faa0831c, 0x3fced3770bef852d),
    (0xbfdf11f493053d00, 0x3fbea1e54bc48dbc),
    (0xbfdf11f493053d01, 0xbfbea1e54bc48db3),
    (0xbfdc0b23faa0831c, 0xbfced3770bef852d),
    (0xbfdc0b23faa08311, 0xbfced3770bef8554),
    (0xbfd7f3ccd0032e0e, 0xbfd5384d024c2f82),
    (0xbfc6b1d8b2365da6, 0xbfddeba72ef20146),
    (0x3faedb7debaa3ee3, 0xbfdfc44566966769),
    (0x3fc3089b2619b7b2, 0xbfde8d55ee14a579),
    (0x3fc3089b2619b7bc, 0xbfde8d55ee14a578),
    (0x3fc7d4d23376f139, 0xbfddb2e28bbd51a6),
    (0x3fc7d4d23376f143, 0xbfddb2e28bbd51a4),
    (0x3fd22d961ea71110, 0xbfda55e242a4c3d8),
    (0x3fdc55a7e00740ea, 0xbfcdbe064267c479),
];
const ANNULUS_TWIN_OUTER: [u32; 61] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49,
    50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60,
];
const ANNULUS_TWIN_HOLE: [u32; 25] = [
    61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84,
    85,
];

fn fixture_verts() -> Vec<Point2> {
    ANNULUS_TWIN_VERT_BITS
        .iter()
        .map(|&(x, y)| Point2::new(f64::from_bits(x), f64::from_bits(y)))
        .collect()
}

/// Undirected edge use-counts over a triangle list.
fn edge_counts(tris: &[[u32; 3]]) -> HashMap<(u32, u32), u32> {
    let mut m = HashMap::new();
    for t in tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *m.entry((a.min(b), a.max(b))).or_insert(0u32) += 1;
        }
    }
    m
}

#[test]
fn ulp_twin_annulus_floodfill_is_boundary_conformal() {
    let verts = fixture_verts();
    let holes: Vec<Vec<u32>> = vec![ANNULUS_TWIN_HOLE.to_vec()];
    let tris = cdt_polygon_with_holes_floodfill(&verts, &ANNULUS_TWIN_OUTER, &holes)
        .expect("ULP-twin annulus must triangulate");

    // Constraint edge set (outer + hole loops).
    let mut constrained: Vec<(u32, u32)> = Vec::new();
    let mut add_loop = |l: &[u32]| {
        for i in 0..l.len() {
            let (a, b) = (l[i], l[(i + 1) % l.len()]);
            constrained.push((a.min(b), a.max(b)));
        }
    };
    add_loop(&ANNULUS_TWIN_OUTER);
    add_loop(&ANNULUS_TWIN_HOLE);

    let counts = edge_counts(&tris);
    let mut violations: Vec<String> = Vec::new();
    for &e in &constrained {
        match counts.get(&e).copied().unwrap_or(0) {
            1 => {}
            c => violations.push(format!("constraint edge {e:?} used {c}x (want 1)")),
        }
    }
    let cset: std::collections::HashSet<(u32, u32)> = constrained.iter().copied().collect();
    for (&e, &c) in &counts {
        if !cset.contains(&e) && c != 2 {
            violations.push(format!("interior edge {e:?} used {c}x (want 2)"));
        }
    }
    assert!(
        violations.is_empty(),
        "ULP-twin annulus CDT kept set is not boundary-conformal:\n{}",
        violations.join("\n")
    );
}
