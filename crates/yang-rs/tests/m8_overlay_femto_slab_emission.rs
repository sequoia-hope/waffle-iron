//! M8 — femto-slab collapse at the overlay emission gate: WALL PINS.
//!
//! Spec: `specs/m8_overlay_femto_slab_emission.md` (P10 abort record,
//! 2026-07-10). Corpus targets F0067 / C0048 / R0053 (`overlay-failed
//! RoundingCollapse`, mechanism (ii) of the 2026-07-10 coplanar-wall
//! census).
//!
//! The fixture below is C0048's REAL failing Stage-0 pair (verbatim f64
//! coordinates from the `[poly-probe]` dump): two mirrored 14-gon disc rims
//! whose samples are split by 1–2 ULPs. The exact trapezoidal sweep builds
//! femto event columns from the twins whose cells round f64-degenerate.
//! Local emission-gate surgery (exact T-subdivision + same-class quad
//! flips, strict-progress-gated) was prototyped against this fixture and
//! REFUTED — see the spec for the three measured sub-mechanisms (clean
//! slab needles ARE locally repairable, but chord-collinear mint triples
//! and rounded-order-inverted twin pairs are not). The active test pins
//! the loud wall; the `#[ignore]`d test is the green target for the PR
//! that lands the real fix (per-region re-emission / mint-site collapse),
//! with the full oracle stack (spec §5): I2 exact positivity, I3
//! conformality + input-edge tiling, I4 f64 CCW-positivity, I5 per-class
//! exact identity within the absorbed femto bound, I6 determinism.

use cad_primitives::Point2;
use dashu::float::FBig;
use dashu::rational::RBig;
use std::collections::BTreeMap;
use yang_rs::coplanar_overlay::{
    coplanar_overlay, ClassifiedOverlay, CoplanarOverlayError, ExactPoint2, PolygonWithHoles,
    RegionClass,
};

// ───────────────────────────── helpers ──────────────────────────────────

fn pts(coords: &[(f64, f64)]) -> Vec<Point2> {
    coords.iter().map(|&(x, y)| Point2::new(x, y)).collect()
}

/// Exact f64 → RBig (total for finite input).
fn rb(x: f64) -> RBig {
    let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
    RBig::try_from(fb).expect("FBig → RBig is total")
}

fn cross_e(a: &ExactPoint2, b: &ExactPoint2, c: &ExactPoint2) -> RBig {
    (&b.x - &a.x) * (&c.y - &a.y) - (&b.y - &a.y) * (&c.x - &a.x)
}

/// Exact shoelace area (absolute) of one loop.
fn shoelace_abs(lp: &[Point2]) -> RBig {
    let mut s = RBig::ZERO;
    for i in 0..lp.len() {
        let (p, q) = (&lp[i], &lp[(i + 1) % lp.len()]);
        s += rb(p.x()) * rb(q.y()) - rb(q.x()) * rb(p.y());
    }
    if s < RBig::ZERO {
        s = -s;
    }
    s / RBig::from(2)
}

fn input_area_exact(p: &PolygonWithHoles) -> RBig {
    let mut area = shoelace_abs(&p.outer);
    for h in &p.holes {
        area -= shoelace_abs(h);
    }
    area
}

/// `q` within the closed bbox of `[a, b]` (used with exactly collinear `q`).
fn within(a: &ExactPoint2, b: &ExactPoint2, q: &ExactPoint2) -> bool {
    let (xlo, xhi) = if a.x <= b.x {
        (&a.x, &b.x)
    } else {
        (&b.x, &a.x)
    };
    let (ylo, yhi) = if a.y <= b.y {
        (&a.y, &b.y)
    } else {
        (&b.y, &a.y)
    };
    &q.x >= xlo && &q.x <= xhi && &q.y >= ylo && &q.y <= yhi
}

/// Assert input segment `a→b` is exactly tiled by overlay triangle edges
/// lying on it (yr25 property-4 oracle, condensed: the on-segment edges
/// must sum to the full segment length with no gaps).
fn assert_edge_covered(ov: &ClassifiedOverlay, a: &ExactPoint2, b: &ExactPoint2, label: &str) {
    // Collect all triangle edges exactly on [a,b], as sorted parameters.
    let mut cuts: Vec<(RBig, RBig)> = Vec::new();
    let dx = &b.x - &a.x;
    let dy = &b.y - &a.y;
    let len2 = &dx * &dx + &dy * &dy;
    for tri in &ov.tris {
        for k in 0..3 {
            let p = &ov.exact_verts[tri[k] as usize];
            let q = &ov.exact_verts[tri[(k + 1) % 3] as usize];
            if cross_e(a, b, p) == RBig::ZERO
                && cross_e(a, b, q) == RBig::ZERO
                && within(a, b, p)
                && within(a, b, q)
            {
                let t = |r: &ExactPoint2| (&(&r.x - &a.x) * &dx + &(&r.y - &a.y) * &dy) / &len2;
                let (t0, t1) = (t(p), t(q));
                if t0 < t1 {
                    cuts.push((t0, t1));
                } else if t1 < t0 {
                    cuts.push((t1, t0));
                }
            }
        }
    }
    cuts.sort();
    cuts.dedup();
    let mut reach = RBig::ZERO;
    for (t0, t1) in cuts {
        assert!(t0 <= reach, "{label}: gap in edge tiling at t={t0}");
        if t1 > reach {
            reach = t1;
        }
    }
    assert_eq!(reach, RBig::ONE, "{label}: edge tiling does not reach t=1");
}

/// Shared post-repair oracle stack (spec §4/§5): I2/I3/I4 + relaxed I5 +
/// determinism. `check_properties` from yr25 asserts the exact per-class
/// identity, which the repair may shift by absorbed femto sliver areas
/// (I5) — here the identity is asserted within that bound instead.
fn check_repaired(a: &PolygonWithHoles, b: &PolygonWithHoles, ov: &ClassifiedOverlay) {
    // I2: strictly positive EXACT area for every emitted triangle.
    for (i, tri) in ov.tris.iter().enumerate() {
        let area2 = cross_e(
            &ov.exact_verts[tri[0] as usize],
            &ov.exact_verts[tri[1] as usize],
            &ov.exact_verts[tri[2] as usize],
        );
        assert!(
            area2 > RBig::ZERO,
            "tri {i} has non-positive exact area {area2}"
        );
    }

    // I4: every kept triangle strictly CCW-positive in the ROUNDED coords.
    for (i, tri) in ov.tris.iter().enumerate() {
        let a2 = ov.verts[tri[0] as usize];
        let b2 = ov.verts[tri[1] as usize];
        let c2 = ov.verts[tri[2] as usize];
        let area2 = (b2.x() - a2.x()) * (c2.y() - a2.y()) - (b2.y() - a2.y()) * (c2.x() - a2.x());
        assert!(area2 > 0.0, "tri {i} not f64-CCW-positive (area2={area2})");
    }

    // I3: conformality — ≤ 2 triangles per undirected edge.
    let mut edge_count: BTreeMap<[u32; 2], usize> = BTreeMap::new();
    for tri in &ov.tris {
        for k in 0..3 {
            let (i, j) = (tri[k], tri[(k + 1) % 3]);
            let key = if i < j { [i, j] } else { [j, i] };
            *edge_count.entry(key).or_default() += 1;
        }
    }
    for (e, n) in &edge_count {
        assert!(
            *n <= 2,
            "edge {e:?} has {n} adjacent triangles (T-junction)"
        );
    }

    // I3 (input-edge tiling): every input edge of A and B stays exactly
    // tiled by on-segment triangle edges.
    for (side, poly) in [("A", a), ("B", b)] {
        for (li, lp) in std::iter::once(&poly.outer)
            .chain(poly.holes.iter())
            .enumerate()
        {
            for i in 0..lp.len() {
                let p = &lp[i];
                let q = &lp[(i + 1) % lp.len()];
                let pe = ExactPoint2 {
                    x: rb(p.x()),
                    y: rb(p.y()),
                };
                let qe = ExactPoint2 {
                    x: rb(q.x()),
                    y: rb(q.y()),
                };
                assert_edge_covered(ov, &pe, &qe, &format!("{side} loop {li} edge {i}"));
            }
        }
    }

    // I5: per-class exact identity within the absorbed-femto bound. Each
    // absorbed sliver has exact area < 1 ULP × its longest extent; 1e-12
    // is generous at these model scales (≤ ~160) while catching any real
    // misclassification (the smallest genuine region areas are ≫ 1e-9).
    let bound = RBig::from(1) / RBig::from(10u64).pow(12);
    for (side, poly) in [('A', a), ('B', b)] {
        let only = if side == 'A' {
            RegionClass::AOnly
        } else {
            RegionClass::BOnly
        };
        let mut delta =
            ov.area_exact(only) + ov.area_exact(RegionClass::Overlap) - input_area_exact(poly);
        if delta < RBig::ZERO {
            delta = -delta;
        }
        assert!(
            delta < bound,
            "side {side}: per-class identity off by {delta} (> absorbed-femto bound)"
        );
    }

    // I6: determinism — bit-identical second run.
    let ov2 = coplanar_overlay(a, b).expect("second run must succeed");
    assert_eq!(ov.tris, ov2.tris, "tris differ across runs");
    assert_eq!(ov.class, ov2.class, "classes differ across runs");
    assert_eq!(ov.exact_verts, ov2.exact_verts, "exact verts differ");
}

// ───────────────────────────── fixtures ─────────────────────────────────

/// C0048's real failing pair (Stage-0 `[poly-probe]` dump, pair=(1,0)):
/// mirrored 14-gon disc rims with 1–2-ULP-split samples. A's three holes are
/// interior to the rim and uninvolved in the left-column femto slab — the
/// outer-loop pair alone reproduces the `RoundingCollapse` (verified RED).
fn c0048_pair() -> (PolygonWithHoles, PolygonWithHoles) {
    let a = PolygonWithHoles {
        outer: pts(&[
            (-1.4623918682727357, -0.3337814009344701),
            (-1.1727472237020449, -0.9352347027881001),
            (-0.6508256086763362, -1.3514533018536292),
            (0.0, -1.5),
            (0.6508256086763373, -1.3514533018536288),
            (1.1727472237020446, -0.9352347027881003),
            (1.4623918682727355, -0.3337814009344716),
            (1.4623918682727355, 0.3337814009344716),
            (1.1727472237020446, 0.9352347027881003),
            (0.6508256086763373, 1.3514533018536288),
            (9.184850993605148e-17, 1.5),
            (-0.6508256086763371, 1.3514533018536288),
            (-1.1727472237020446, 0.9352347027881004),
            (-1.4623918682727355, 0.33378140093447173),
        ]),
        holes: vec![],
    };
    let b = PolygonWithHoles {
        outer: pts(&[
            (-1.4623918682727355, -0.33378140093447173),
            (-1.1727472237020446, -0.9352347027881004),
            (-0.6508256086763371, -1.3514533018536288),
            (0.0, -1.5),
            (0.6508256086763363, -1.351453301853629),
            (1.172747223702045, -0.9352347027881001),
            (1.4623918682727357, -0.33378140093446995),
            (1.4623918682727353, 0.33378140093447195),
            (1.1727472237020446, 0.9352347027881005),
            (0.6508256086763369, 1.351453301853629),
            (-2.755455298081545e-16, 1.5),
            (-0.6508256086763374, 1.3514533018536286),
            (-1.1727472237020449, 0.9352347027881001),
            (-1.4623918682727355, 0.3337814009344714),
        ]),
        holes: vec![],
    };
    (a, b)
}

/// WALL PIN (P10 abort record, spec §"branch table" 2026-07-10): the
/// mirrored-rim femto slab is a LOUD `RoundingCollapse` today. Local
/// emission-gate surgery (T-subdivision + same-class quad flips) was
/// prototyped and REFUTED on this very fixture: the bottom twin-corner
/// cluster carries crossing mints EXACTLY collinear on A's chord (probe:
/// verts 19/22/26 on one line), so every candidate apex yields an exactly
/// degenerate or f64-degenerate piece. If this test starts failing, the
/// gate's behavior changed — either the wall landed (un-quarantine the
/// green target below and delete this pin) or a silent-drop crept in (P9).
#[test]
fn c0048_mirrored_rim_slab_stays_loud() {
    let (a, b) = c0048_pair();
    let err = coplanar_overlay(&a, &b).expect_err("femto-slab collapse must stay LOUD");
    assert!(
        matches!(err, CoplanarOverlayError::RoundingCollapse { .. }),
        "expected RoundingCollapse, got {err:?}"
    );
}

/// GREEN TARGET (quarantined): the C0048 mirrored-rim pair classifies
/// successfully once the femto-slab emission wall lands (per-region
/// re-emission / mint-site collapse — see the spec's P10 abort record for
/// why local surgery cannot do it). Un-quarantine in the PR that lands it.
#[test]
#[ignore = "M8 femto-slab emission (m8_overlay_femto_slab_emission): needs per-region \
            re-emission or mint-site collapse; local T-subdivision/flip refuted 2026-07-10"]
fn c0048_mirrored_rim_slab_repair() {
    let (a, b) = c0048_pair();
    let ov = coplanar_overlay(&a, &b)
        .expect("mirrored-rim femto slab must be repaired at the emission gate");
    check_repaired(&a, &b, &ov);

    // The pair is a near-identical rim pair: virtually everything overlaps.
    // Exact sanity: Overlap dominates, both Only-regions are femto residue.
    let overlap = ov.area_exact(RegionClass::Overlap);
    let a_only = ov.area_exact(RegionClass::AOnly);
    let b_only = ov.area_exact(RegionClass::BOnly);
    assert!(
        overlap > RBig::from(6),
        "expected a dominant overlap region (14-gon area ≈ 6.6)"
    );
    let femto = RBig::from(1) / RBig::from(10u64).pow(9);
    assert!(a_only < femto, "AOnly must be femto residue, got {a_only}");
    assert!(b_only < femto, "BOnly must be femto residue, got {b_only}");
}
