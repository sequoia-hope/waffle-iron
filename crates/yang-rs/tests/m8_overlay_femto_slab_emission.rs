//! M8 — fused emission at the overlay rounding gate.
//!
//! Spec: `specs/m8_overlay_fused_emission_collapse.md` (SPEC, 2026-07-12) —
//! the successor to the P10 abort record in
//! `specs/m8_overlay_femto_slab_emission.md` §8. This cycle lands the
//! constrained-edge-collapse (fused-emission) mechanism: vertices of a
//! sub-f64-resolution degenerate complex are FUSED (loser→survivor,
//! recorded in `ClassifiedOverlay::fused`), after which the complex's
//! f64-degenerate triangles vanish by construction and the remaining
//! triangulation is f64-emittable. The refuted approach (local
//! T-subdivision + quad flips over a FIXED rounded vertex set) is exactly
//! what the P10 record proves impossible for the measured corpus
//! structures (chord-collinear mint triples; whole clusters rounding onto
//! one f64 event column) — this spec removes the fixed-vertex-set premise.
//!
//! Corpus targets: F0067 / C0048 (`overlay-failed RoundingCollapse`, the
//! two remaining cases of task #130 mechanism (2)).
//!
//! Oracle stack (spec §5, exercised by [`check_repaired`]):
//!   * I2 — every emitted triangle strictly positive EXACT area.
//!   * I3 — conformality: ≤ 2 triangles per undirected edge.
//!   * I3' — FUSED input-edge tiling: each input edge's on-edge vertex
//!     chain, with every fused vertex substituted by its survivor, is
//!     covered gap-free by emitted triangle edges (edges with no fused
//!     on-edge vertex keep the ORIGINAL exact tiling — that reduction is
//!     automatic here). The refuted spec's exact-tiling-of-original-chains
//!     oracle is provably unsatisfiable jointly with I4 once two input
//!     chains share one rounded image (spec §5 I3' rationale), which is
//!     why this restatement is legitimate, not a weakening.
//!   * I4 — every emitted triangle strictly CCW-positive in ROUNDED f64.
//!   * I5 — per-class exact identity within the absorbed-femto bound.
//!   * I6 — determinism, including a bit-identical `fused` map.
//!
//! Preserved walls (honest `RoundingCollapse`, spec §4 B5/B6) are pinned by
//! [`supra_tau_collinear_stays_loud`]: real-scale rounded-collinear slivers
//! whose every collapse candidate edge is supra-`TAU_MODEL` are NEVER fused.

use cad_primitives::Point2;
use dashu::float::FBig;
use dashu::rational::RBig;
use std::collections::{BTreeMap, BTreeSet};
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

/// Follow the fusion record to a vertex's surviving index. The spec
/// guarantees `fused` is fully resolved (a value is never a key), so this is
/// at most one hop; the loop with a cycle guard is defensive.
fn resolve(ov: &ClassifiedOverlay, mut i: u32) -> u32 {
    let mut guard = 0usize;
    while let Some(&s) = ov.fused.get(&i) {
        i = s;
        guard += 1;
        assert!(
            guard <= ov.exact_verts.len(),
            "fused map is cyclic / not fully resolved at index {i}"
        );
    }
    i
}

/// Set of undirected edges present in the emitted triangulation.
fn emitted_edges(ov: &ClassifiedOverlay) -> BTreeSet<[u32; 2]> {
    let mut set = BTreeSet::new();
    for tri in &ov.tris {
        for k in 0..3 {
            let (i, j) = (tri[k], tri[(k + 1) % 3]);
            set.insert(if i < j { [i, j] } else { [j, i] });
        }
    }
    set
}

/// I3' — FUSED input-edge tiling of segment `a→b`.
///
/// Build the ORIGINAL on-edge vertex chain (every overlay vertex exactly on
/// `[a,b]`, sorted by parameter — endpoints included). Substitute each
/// vertex by its fused survivor. Every consecutive pair of DISTINCT
/// survivors must be an emitted triangle edge: the pre-fusion mesh tiled the
/// input edge with a triangle edge between consecutive on-edge vertices, and
/// the loser→survivor remap carries that edge to `{survivor_j, survivor_k}`
/// unless the sub-edge collapsed (its two survivors coincide — an absorbed
/// femto sub-edge, legitimately skipped). This is robust to which side wins
/// each collapse (the assertion checks the remapped edge's mere existence,
/// making no assumption that a survivor still lies on the original line).
///
/// When `[a,b]` has NO fused on-edge vertex the chain's survivors are the
/// original indices, so this reduces to the original exact tiling.
fn assert_edge_tiled_fused(ov: &ClassifiedOverlay, a: &ExactPoint2, b: &ExactPoint2, label: &str) {
    let dx = &b.x - &a.x;
    let dy = &b.y - &a.y;
    let len2 = &dx * &dx + &dy * &dy;
    assert!(
        len2 > RBig::ZERO,
        "{label}: zero-length input edge in fixture"
    );

    // Original on-edge vertex chain (indices), sorted by parameter along a→b.
    let mut chain: Vec<(RBig, u32)> = Vec::new();
    for (i, q) in ov.exact_verts.iter().enumerate() {
        if cross_e(a, b, q) == RBig::ZERO && within(a, b, q) {
            let t = (&(&q.x - &a.x) * &dx + &(&q.y - &a.y) * &dy) / &len2;
            chain.push((t, i as u32));
        }
    }
    chain.sort();
    // Endpoints must be on the chain (parameters 0 and 1).
    assert!(
        chain.first().map(|(t, _)| t) == Some(&RBig::ZERO),
        "{label}: input edge start vertex missing from overlay"
    );
    assert!(
        chain.last().map(|(t, _)| t) == Some(&RBig::ONE),
        "{label}: input edge end vertex missing from overlay"
    );

    let edges = emitted_edges(ov);
    let mut prev = resolve(ov, chain[0].1);
    for &(_, idx) in &chain[1..] {
        let s = resolve(ov, idx);
        if s == prev {
            continue; // sub-edge collapsed into a single survivor (absorbed)
        }
        let key = if prev < s { [prev, s] } else { [s, prev] };
        assert!(
            edges.contains(&key),
            "{label}: fused chain sub-edge {prev}->{s} is not an emitted triangle edge"
        );
        prev = s;
    }
}

/// Shared post-repair oracle stack (spec §5): I2, I3, I3', I4, I5, I6.
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
            "I2: tri {i} has non-positive exact area {area2}"
        );
    }

    // I4: every kept triangle strictly CCW-positive in the ROUNDED coords.
    for (i, tri) in ov.tris.iter().enumerate() {
        let a2 = ov.verts[tri[0] as usize];
        let b2 = ov.verts[tri[1] as usize];
        let c2 = ov.verts[tri[2] as usize];
        let area2 = (b2.x() - a2.x()) * (c2.y() - a2.y()) - (b2.y() - a2.y()) * (c2.x() - a2.x());
        assert!(
            area2 > 0.0,
            "I4: tri {i} not f64-CCW-positive (area2={area2})"
        );
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
            "I3: edge {e:?} has {n} adjacent triangles (T-junction)"
        );
    }

    // I3': fused input-edge tiling for every input edge of A and B.
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
                assert_edge_tiled_fused(ov, &pe, &qe, &format!("{side} loop {li} edge {i}"));
            }
        }
    }

    // I5: per-class exact identity within the absorbed-femto bound. Fusion
    // moves each fused vertex by < TAU_MODEL and drops only index-degenerate
    // triangles (exact area sub-TAU_MODEL × domain scale); 1e-12 is generous
    // at the corpus model scales while catching any real misclassification.
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
            "I5: side {side}: per-class identity off by {delta} (> absorbed-femto bound)"
        );
    }

    // I6: determinism — bit-identical second run, INCLUDING the fusion record.
    let ov2 = coplanar_overlay(a, b).expect("I6: second run must succeed");
    assert_eq!(ov.tris, ov2.tris, "I6: tris differ across runs");
    assert_eq!(ov.class, ov2.class, "I6: classes differ across runs");
    assert_eq!(ov.exact_verts, ov2.exact_verts, "I6: exact verts differ");
    assert_eq!(ov.fused, ov2.fused, "I6: fused record differs across runs");
}

/// Assert the fusion record is non-empty and fully resolved (spec: a `fused`
/// value is never itself a key).
fn assert_fused_resolved(ov: &ClassifiedOverlay) {
    assert!(
        !ov.fused.is_empty(),
        "expected a non-empty fusion record for a repaired femto slab"
    );
    for (loser, survivor) in &ov.fused {
        assert_ne!(loser, survivor, "fused vertex maps to itself");
        assert!(
            !ov.fused.contains_key(survivor),
            "fused survivor {survivor} is itself a key (record not fully resolved)"
        );
    }
}

/// FNV-1a over the byte image of the whole overlay result (tris, class,
/// rounded verts) — a stable golden key for the zero-regression witness.
fn overlay_fnv(ov: &ClassifiedOverlay) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut mix = |bytes: &[u8]| {
        for &byte in bytes {
            h ^= byte as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    for t in &ov.tris {
        for v in t {
            mix(&v.to_le_bytes());
        }
    }
    for c in &ov.class {
        mix(&[*c as u8]);
    }
    for v in &ov.verts {
        mix(&v.x().to_bits().to_le_bytes());
        mix(&v.y().to_bits().to_le_bytes());
    }
    h
}

/// Count of distinct exact verts that round onto a SHARED f64 point — the
/// footprint of a benign coincident-needle drop.
fn coincident_f64_pairs(ov: &ClassifiedOverlay) -> usize {
    let mut m: BTreeMap<(u64, u64), usize> = BTreeMap::new();
    for v in &ov.verts {
        *m.entry((v.x().to_bits(), v.y().to_bits())).or_default() += 1;
    }
    m.values().filter(|c| **c > 1).map(|c| *c - 1).sum()
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

/// Uniformly scale a hole-free polygon's coordinates.
fn scale_outer(p: &PolygonWithHoles, s: f64) -> PolygonWithHoles {
    PolygonWithHoles {
        outer: p
            .outer
            .iter()
            .map(|q| Point2::new(q.x() * s, q.y() * s))
            .collect(),
        holes: vec![],
    }
}

/// f64 one-ULP neighbours (nextafter toward ±∞), used to split a shared
/// chain by exactly 1–2 ULP. Deterministic bit arithmetic; inputs here are
/// finite and non-zero so the sign branches are exercised cleanly.
fn up(x: f64) -> f64 {
    debug_assert!(x.is_finite());
    if x > 0.0 {
        f64::from_bits(x.to_bits() + 1)
    } else {
        f64::from_bits(x.to_bits() - 1)
    }
}
fn dn(x: f64) -> f64 {
    debug_assert!(x.is_finite());
    if x > 0.0 {
        f64::from_bits(x.to_bits() - 1)
    } else {
        f64::from_bits(x.to_bits() + 1)
    }
}

/// Synthetic minimal femto slab (spec §6 B4/B8): two near-identical
/// parallelograms whose four shared corner chains are split by 1–2 ULP in a
/// crossing pattern, so the strip between corresponding edges rounds
/// f64-degenerate. Hardcoded f64 coords (fully deterministic); away from
/// zero so no subnormals are involved. Verified to fail `RoundingCollapse`
/// under the pre-fusion code, and to fuse (overlap ≈ 4) after the repair.
fn synthetic_femto_parallelograms() -> (PolygonWithHoles, PolygonWithHoles) {
    let a = PolygonWithHoles {
        outer: pts(&[(1.0, 1.0), (3.0, 1.0), (4.0, 3.0), (2.0, 3.0)]),
        holes: vec![],
    };
    let b = PolygonWithHoles {
        outer: pts(&[
            (up(1.0), dn(1.0)),
            (up(3.0), up(1.0)),
            (dn(4.0), up(3.0)),
            (dn(2.0), dn(3.0)),
        ]),
        holes: vec![],
    };
    (a, b)
}

// ───────────────────────────── tests ────────────────────────────────────

/// GREEN TARGET (un-quarantined this cycle): the C0048 mirrored-rim pair now
/// classifies successfully via the fused-emission repair. Full oracle stack
/// (spec §5) plus: the fusion record is non-empty and fully resolved, and
/// Overlap dominates while both Only-regions are femto residue.
///
/// Until the repair lands this FAILS RED with `RoundingCollapse` (the
/// pre-fusion code, HEAD, has an always-empty `fused` field).
#[test]
fn c0048_mirrored_rim_slab_repair() {
    let (a, b) = c0048_pair();
    let ov = coplanar_overlay(&a, &b)
        .expect("mirrored-rim femto slab must be repaired at the emission gate");

    check_repaired(&a, &b, &ov);
    assert_fused_resolved(&ov);

    // Near-identical rim pair: virtually everything overlaps.
    let overlap = ov.area_exact(RegionClass::Overlap);
    let a_only = ov.area_exact(RegionClass::AOnly);
    let b_only = ov.area_exact(RegionClass::BOnly);
    assert!(
        overlap > RBig::from(6),
        "expected a dominant overlap region (14-gon area ≈ 6.6), got {overlap}"
    );
    let femto = RBig::from(1) / RBig::from(10u64).pow(9);
    assert!(a_only < femto, "AOnly must be femto residue, got {a_only}");
    assert!(b_only < femto, "BOnly must be femto residue, got {b_only}");
}

/// B4/B8 (spec §6): synthetic minimal femto slab — two near-identical
/// parallelograms whose shared chains are 1–2-ULP split. FAILS RED with
/// `RoundingCollapse` today; after the repair it fuses to a dominant Overlap
/// and passes the full oracle stack with a non-empty fusion record.
#[test]
fn synthetic_femto_slab_fuses() {
    let (a, b) = synthetic_femto_parallelograms();
    let ov = coplanar_overlay(&a, &b)
        .expect("synthetic femto parallelogram slab must fuse at the emission gate");

    check_repaired(&a, &b, &ov);
    assert_fused_resolved(&ov);

    // Both parallelograms have exact area 4; the fused pair overlaps almost
    // entirely, leaving only femto Only-residue.
    let overlap = ov.area_exact(RegionClass::Overlap);
    let a_only = ov.area_exact(RegionClass::AOnly);
    let b_only = ov.area_exact(RegionClass::BOnly);
    assert!(
        overlap > RBig::from(3),
        "expected a dominant overlap region (parallelogram area = 4), got {overlap}"
    );
    let femto = RBig::from(1) / RBig::from(10u64).pow(6);
    assert!(a_only < femto, "AOnly must be femto residue, got {a_only}");
    assert!(b_only < femto, "BOnly must be femto residue, got {b_only}");
}

/// B2 (spec §6) — zero-regression witness. C0048 scaled by 1e10 rounds its
/// femto slab to benign COINCIDENT NEEDLES only (no `CollinearSliver`), so it
/// succeeds on the LEGACY path today. After the repair lands it must stay on
/// the legacy path (spec §3 step 2: no sliver ⇒ byte-identical) — the fusion
/// record stays empty and every byte of the output is unchanged.
///
/// The golden below was captured from the pre-repair code (HEAD). 23 distinct
/// exact verts round onto shared f64 points here, i.e. the needle-drop path
/// is genuinely exercised — this is a needle fixture, not a trivial one.
#[test]
fn needle_only_overlay_byte_identical_legacy() {
    let (a, b) = c0048_pair();
    let a = scale_outer(&a, 1e10);
    let b = scale_outer(&b, 1e10);
    let ov = coplanar_overlay(&a, &b).expect("scaled c0048 needle overlay must succeed (legacy)");

    // Golden (pre-repair capture): structural counts + FNV-1a of the whole
    // result. Any drift means the legacy needle-weld path changed.
    assert_eq!(ov.verts.len(), 110, "golden: vertex count");
    assert_eq!(ov.tris.len(), 109, "golden: triangle count");
    assert_eq!(
        ov.class
            .iter()
            .filter(|c| **c == RegionClass::Overlap)
            .count(),
        56,
        "golden: Overlap triangle count"
    );
    assert_eq!(
        ov.class
            .iter()
            .filter(|c| **c == RegionClass::AOnly)
            .count(),
        31,
        "golden: AOnly triangle count"
    );
    assert_eq!(
        ov.class
            .iter()
            .filter(|c| **c == RegionClass::BOnly)
            .count(),
        22,
        "golden: BOnly triangle count"
    );
    assert_eq!(
        overlay_fnv(&ov),
        0x4219f62a20bdfbfe,
        "golden: overlay FNV-1a drifted (legacy path is no longer byte-identical)"
    );
    assert_eq!(
        coincident_f64_pairs(&ov),
        23,
        "golden: needle-drop footprint (coincident f64 vert pairs)"
    );
    assert!(
        ov.fused.is_empty(),
        "legacy (no-sliver) path must not fuse anything"
    );
}

/// B5/B6 (spec §4) — preserved-wall pin. The honest `RoundingCollapse` must
/// survive when every collapse candidate is real-scale (supra-`TAU_MODEL`).
///
/// C0048 scaled by 1e9 keeps a rounded-collinear sliver while lifting every
/// femto feature above the fusion ceiling: the original twin separations
/// (~2e-16 at unit scale) become ~2e-7, which is above `TAU_MODEL` (1e-7),
/// and any three-distinct-f64 collinear sliver at this scale spans ≥ 1 ULP
/// (~3e-7) — so its edges are supra-`TAU_MODEL` and INELIGIBLE for fusion
/// (spec §2 eligibility ceiling `< TAU_MODEL²`). The repair must therefore
/// leave the sliver alone and return the loud wall (KV15b R0091 lesson:
/// widening the ceiling to MIN_FEATURE_SIZE would wrongly fuse real
/// micro-geometry).
///
/// (The task's suggested ×1e10 does NOT reproduce a sliver — it rounds to
/// needles only and succeeds — so the ×1e9 fallback is used per spec §6.)
#[test]
fn supra_tau_collinear_stays_loud() {
    let (a, b) = c0048_pair();
    let a = scale_outer(&a, 1e9);
    let b = scale_outer(&b, 1e9);
    let err = coplanar_overlay(&a, &b)
        .expect_err("supra-TAU rounded-collinear sliver must stay a LOUD RoundingCollapse");
    assert!(
        matches!(err, CoplanarOverlayError::RoundingCollapse { .. }),
        "expected RoundingCollapse (honest supra-TAU wall), got {err:?}"
    );
}

// ══════════════════════ adversary hardening (task #142) ══════════════════
// Added by the Adversary role to close mutation-sanity gaps the cycle tests
// left open (spec §6 mutation matrix). See the report for the mutation→test
// matrix; these three tests catch the survivor-preference inversion and pin
// geometry preservation / health that `check_repaired` alone did not force.

/// The exact set of input-loop vertices of BOTH sides — the survivor-rank
/// authority the repair re-derives from its own inputs (spec §3).
fn input_loop_set(a: &PolygonWithHoles, b: &PolygonWithHoles) -> BTreeSet<ExactPoint2> {
    let mut set = BTreeSet::new();
    for poly in [a, b] {
        for lp in std::iter::once(&poly.outer).chain(poly.holes.iter()) {
            for p in lp {
                set.insert(ExactPoint2 {
                    x: rb(p.x()),
                    y: rb(p.y()),
                });
            }
        }
    }
    set
}

/// Survivor-preference pin (spec §3 / §6 mutation (b) catcher). The rule is:
/// an input-loop vertex OUTRANKS a minted arrangement vertex, so a fusion
/// NEVER demotes an input-loop vertex in favour of a mint. Two independent
/// assertions on the fusion record:
///
///   * INVARIANT — no fused entry has an input-loop *loser* with a mint
///     *survivor*. An input-loop vertex is only ever fused into ANOTHER
///     input-loop vertex (the both-input min-index arm), never into a mint.
///   * WITNESS — at least one fused entry demotes a MINT into an input-loop
///     survivor, proving the mixed (input-vs-mint) arm is actually exercised
///     so the invariant is not vacuous.
///
/// Under an inverted survivor preference (prefer mints), every mixed fusion
/// records input-loser → mint-survivor: the invariant is violated AND the
/// witness count falls to zero. Either assertion fails — the mutation cannot
/// pass both. Checked on the C0048 pair (14 mixed fusions) and the synthetic
/// slab (both arms exercised) so the catch is not fixture-specific.
#[test]
fn fusion_survivors_prefer_input_loop_vertices() {
    for (label, (a, b)) in [
        ("c0048", c0048_pair()),
        ("synthetic", synthetic_femto_parallelograms()),
    ] {
        let ov = coplanar_overlay(&a, &b)
            .unwrap_or_else(|e| panic!("{label}: femto slab must repair, got {e:?}"));
        assert_fused_resolved(&ov);
        let set = input_loop_set(&a, &b);
        let is_input = |i: u32| set.contains(&ov.exact_verts[i as usize]);

        let mut mint_into_input = 0usize;
        for (&loser, &survivor) in &ov.fused {
            let (loser_in, surv_in) = (is_input(loser), is_input(survivor));
            // Implication: a fused input-loop LOSER must have an input-loop SURVIVOR.
            assert!(
                !loser_in || surv_in,
                "{label}: survivor preference violated — input-loop vertex {loser} \
                 was demoted into MINT survivor {survivor} (inverted preference)"
            );
            if !loser_in && surv_in {
                mint_into_input += 1;
            }
        }
        assert!(
            mint_into_input >= 1,
            "{label}: expected ≥1 mint fused INTO an input-loop survivor (the mixed \
             arm must be exercised so the invariant is non-vacuous); got {mint_into_input}"
        );
    }
}

/// A parallelogram-with-hole where B (hole-free, ULP-split outer) covers A's
/// hole. Two co-present structures: (1) a sub-`TAU_MODEL` femto slab on the
/// ULP-split outer boundary that MUST fuse (repair runs, `fused` non-empty),
/// and (2) a supra-`TAU_MODEL` real feature — A's 0.4×0.4 hole (area 0.16,
/// seven orders above `TAU_MODEL`) — that MUST survive as a BOnly region and
/// NOT be swept up by the fusion. Pins that fusion touches only
/// sub-resolution structure: the supra-scale hole area is preserved to well
/// within the femto bound. (If the eligibility ceiling or the worklist ever
/// reached real geometry, BOnly would collapse toward femto and this fails.)
#[test]
fn femto_slab_coexists_with_supra_hole_feature() {
    // A: synthetic parallelogram outer + a real interior hole (CCW, like the
    // yr25 hole fixtures). The hole sits well inside the parallelogram.
    let a = PolygonWithHoles {
        outer: pts(&[(1.0, 1.0), (3.0, 1.0), (4.0, 3.0), (2.0, 3.0)]),
        holes: vec![pts(&[(2.3, 1.8), (2.7, 1.8), (2.7, 2.2), (2.3, 2.2)])],
    };
    // B: the ULP-split outer that triggers the slab repair, no hole.
    let (_, b) = synthetic_femto_parallelograms();

    let ov = coplanar_overlay(&a, &b)
        .expect("femto slab (ULP outer) must repair while the supra hole survives");
    check_repaired(&a, &b, &ov);
    assert_fused_resolved(&ov); // (1) the sub-TAU slab actually fused.

    // (2) The supra-TAU hole survives as BOnly (B material where A is void).
    let hole_area = RBig::from(4) / RBig::from(25); // 0.4 * 0.4 = 0.16
    let b_only = ov.area_exact(RegionClass::BOnly);
    let femto = RBig::from(1) / RBig::from(10u64).pow(6);
    let mut miss = &b_only - &hole_area;
    if miss < RBig::ZERO {
        miss = -miss;
    }
    assert!(
        miss < femto,
        "supra-TAU hole (0.16) must survive as BOnly, got {b_only} (off by {miss})"
    );
    // Overlap is the parallelogram minus the hole; AOnly is femto residue.
    assert!(
        ov.area_exact(RegionClass::Overlap) > RBig::from(3),
        "overlap must dominate (parallelogram 4 − hole 0.16)"
    );
    assert!(
        ov.area_exact(RegionClass::AOnly) < femto,
        "AOnly must be femto residue, got {}",
        ov.area_exact(RegionClass::AOnly)
    );
}

/// Geometry health of fused output (spec §7 — no silent damage). Beyond the
/// exact/rounded positivity `check_repaired` already asserts, pin explicitly
/// that every emitted rounded coordinate is finite (no NaN/∞ leaked through
/// the collapse remap) and every emitted triangle references three DISTINCT
/// vertex indices (no index-degenerate triangle survived cleanup — I2 covers
/// the exact-area consequence, this covers the structural one directly).
#[test]
fn fused_output_is_finite_and_nondegenerate() {
    for (label, (a, b)) in [
        ("c0048", c0048_pair()),
        ("synthetic", synthetic_femto_parallelograms()),
    ] {
        let ov = coplanar_overlay(&a, &b)
            .unwrap_or_else(|e| panic!("{label}: femto slab must repair, got {e:?}"));
        for (i, v) in ov.verts.iter().enumerate() {
            assert!(
                v.x().is_finite() && v.y().is_finite(),
                "{label}: emitted vertex {i} has non-finite coord ({}, {})",
                v.x(),
                v.y()
            );
        }
        for (i, t) in ov.tris.iter().enumerate() {
            assert!(
                t[0] != t[1] && t[1] != t[2] && t[0] != t[2],
                "{label}: emitted tri {i} is index-degenerate {t:?}"
            );
        }
    }
}

/// Input-validation precedence (spec §7): NaN and degenerate loops are
/// rejected BEFORE the emission gate, whether or not a femto slab is present.
/// Mirrors the yr25 rejection pins at the repair-path boundary so a future
/// gate change cannot accidentally swallow malformed input into the repair
/// loop. (Item 2 of the adversary pathological-input checklist.)
#[test]
fn nan_and_degenerate_inputs_rejected_before_gate() {
    let (good, _) = synthetic_femto_parallelograms();

    let nan = PolygonWithHoles {
        outer: pts(&[(1.0, 1.0), (3.0, 1.0), (f64::NAN, 3.0), (2.0, 3.0)]),
        holes: vec![],
    };
    assert!(
        matches!(
            coplanar_overlay(&nan, &good),
            Err(CoplanarOverlayError::NonFiniteInput)
        ),
        "NaN input must be rejected as NonFiniteInput"
    );

    let two_vert = PolygonWithHoles {
        outer: pts(&[(0.0, 0.0), (1.0, 0.0)]),
        holes: vec![],
    };
    assert!(
        matches!(
            coplanar_overlay(&two_vert, &good),
            Err(CoplanarOverlayError::DegenerateLoop(_))
        ),
        "2-vertex loop must be rejected as DegenerateLoop"
    );
}
