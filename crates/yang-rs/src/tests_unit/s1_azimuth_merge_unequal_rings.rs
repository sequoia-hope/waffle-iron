//! Stage 1 — the azimuth-merge band strip as a ZIPPER of two sorted rings
//! (spec `stage1_azimuth_merge_unequal_rings.md`, 2026-10-13, the wedge-arm
//! follow-up of P0044 / P0054 / P0058 / P0047). `tessellate_band_azimuth_merge`
//! used to pair the two sorted rings positionally and refuse unequal counts
//! or any pairwise azimuth disagreement; a band whose two rims carry
//! different sample sets is ordinary geometry with one honest triangulation,
//! and the zipper produces it: every rim edge used once, every diagonal
//! twice, m + n triangles, all oriented to the target.

use crate::stage1_tessellate::{radial_outward_normal, tessellate_band_azimuth_merge};
use crate::{Point3, Vector3};
use std::collections::{BTreeMap, HashMap};

const TWO_PI: f64 = 2.0 * std::f64::consts::PI;

/// Two rims of a unit cylinder about ẑ: bottom at z = 0 with the given
/// azimuths, top at z = 1 with the given azimuths. Vertex ids: bottom first.
fn rings(bottom_az: &[f64], top_az: &[f64]) -> (Vec<Point3>, BTreeMap<u32, Vec<u32>>) {
    let mut verts = Vec::new();
    let mut rings = BTreeMap::new();
    let mut bottom = Vec::new();
    for &a in bottom_az {
        bottom.push(verts.len() as u32);
        verts.push(Point3::new(a.cos(), a.sin(), 0.0));
    }
    let mut top = Vec::new();
    for &a in top_az {
        top.push(verts.len() as u32);
        verts.push(Point3::new(a.cos(), a.sin(), 1.0));
    }
    rings.insert(0u32, bottom);
    rings.insert(1u32, top);
    (verts, rings)
}

fn uniform(n: usize, phase: f64) -> Vec<f64> {
    (0..n)
        .map(|k| (phase + k as f64 * TWO_PI / n as f64).rem_euclid(TWO_PI))
        .collect()
}

fn zip(verts: &[Point3], rings: &BTreeMap<u32, Vec<u32>>) -> Vec<[u32; 3]> {
    let ap = [0.0, 0.0, 0.0];
    let au = [0.0, 0.0, 1.0];
    let orient =
        |vs: &[Point3], tri: &[u32; 3]| -> [f64; 3] { radial_outward_normal(vs, tri, ap, au) };
    let mut tris = Vec::new();
    tessellate_band_azimuth_merge(
        0,
        rings,
        0,
        1,
        verts,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        &orient,
        &mut tris,
    )
    .expect("the zipper accepts any two rings of ≥ 3 samples");
    tris
}

/// The closed-annulus invariants: m + n triangles; every rim edge
/// (consecutive by azimuth, cyclic) used exactly once; every other edge used
/// exactly twice with opposite directions; every triangle outward and of
/// positive area.
fn assert_closed_band(verts: &[Point3], rings: &BTreeMap<u32, Vec<u32>>, tris: &[[u32; 3]]) {
    let (m, n) = (rings[&0].len(), rings[&1].len());
    assert_eq!(tris.len(), m + n, "a zipper emits m + n triangles");
    let az = |v: u32| {
        verts[v as usize]
            .y()
            .atan2(verts[v as usize].x())
            .rem_euclid(TWO_PI)
    };
    let mut rim_edges: HashMap<(u32, u32), usize> = HashMap::new();
    for ring in [&rings[&0], &rings[&1]] {
        let mut sorted = ring.clone();
        sorted.sort_by(|a, b| az(*a).total_cmp(&az(*b)));
        for k in 0..sorted.len() {
            let (u, w) = (sorted[k], sorted[(k + 1) % sorted.len()]);
            rim_edges.insert((u.min(w), u.max(w)), 0);
        }
    }
    let mut directed: HashMap<(u32, u32), usize> = HashMap::new();
    let mut undirected: HashMap<(u32, u32), usize> = HashMap::new();
    for tri in tris {
        let (a, b, c) = (
            verts[tri[0] as usize].as_array(),
            verts[tri[1] as usize].as_array(),
            verts[tri[2] as usize].as_array(),
        );
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let nrm = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let area2 = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
        assert!(area2 > 1e-9, "degenerate triangle {tri:?}");
        let cx = (a[0] + b[0] + c[0]) / 3.0;
        let cy = (a[1] + b[1] + c[1]) / 3.0;
        assert!(
            nrm[0] * cx + nrm[1] * cy > 0.0,
            "triangle {tri:?} wound inward"
        );
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (u, w) = (tri[i], tri[j]);
            *directed.entry((u, w)).or_default() += 1;
            *undirected.entry((u.min(w), u.max(w))).or_default() += 1;
        }
    }
    for (e, cnt) in &undirected {
        if let Some(r) = rim_edges.get_mut(e) {
            *r += 1;
            assert_eq!(*cnt, 1, "rim edge {e:?} used {cnt} times");
        } else {
            assert_eq!(*cnt, 2, "diagonal {e:?} used {cnt} times");
            assert_eq!(
                directed.get(&(e.0, e.1)),
                Some(&1),
                "diagonal {e:?} not oppositely paired"
            );
            assert_eq!(
                directed.get(&(e.1, e.0)),
                Some(&1),
                "diagonal {e:?} not oppositely paired"
            );
        }
    }
    for (e, used) in &rim_edges {
        assert_eq!(*used, 1, "rim edge {e:?} missing from the strip");
    }
}

/// Equal, azimuth-matched rings: the zipper reproduces the positional
/// strip's triangle list byte for byte (two triangles per slot, split along
/// b0→t1, in slot order).
#[test]
fn equal_matched_rings_are_byte_identical_to_the_positional_strip() {
    let (verts, rings) = rings(&uniform(12, 0.0), &uniform(12, 0.0));
    let tris = zip(&verts, &rings);
    assert_closed_band(&verts, &rings, &tris);
    let n = 12u32;
    for k in 0..n {
        let kn = (k + 1) % n;
        let (b0, b1, t0, t1) = (k, kn, n + k, n + kn);
        assert_eq!(
            tris[2 * k as usize],
            [b0, b1, t1],
            "slot {k} first triangle"
        );
        assert_eq!(
            tris[2 * k as usize + 1],
            [b0, t1, t0],
            "slot {k} second triangle"
        );
    }
}

/// P0047's shape: the shared samples plus ONE extra sample on one rim.
#[test]
fn one_extra_sample_on_one_rim_zips_to_a_closed_band() {
    let mut top = uniform(12, 0.0);
    top.push(0.7); // off-grid, between slots 1 and 2
    let (verts, rings) = rings(&uniform(12, 0.0), &top);
    let tris = zip(&verts, &rings);
    assert_closed_band(&verts, &rings, &tris);
}

/// Arbitrary unequal sample sets on both rims.
#[test]
fn unequal_arbitrary_rings_zip_to_a_closed_band() {
    let bottom: Vec<f64> = [0.1, 0.9, 1.3, 2.2, 2.9, 3.7, 4.4, 4.5, 5.0, 5.6, 6.1]
        .into_iter()
        .collect();
    let top: Vec<f64> = [
        0.0, 0.5, 1.0, 1.1, 1.9, 2.4, 3.3, 3.9, 4.2, 4.9, 5.1, 5.4, 5.9,
    ]
    .into_iter()
    .collect();
    let (verts, rings) = rings(&bottom, &top);
    let tris = zip(&verts, &rings);
    assert_closed_band(&verts, &rings, &tris);
}

/// Equal counts but a phase offset beyond the match tolerance: formerly the
/// loud "rims disagree" refusal; the zipper alternates triangles instead.
#[test]
fn phase_offset_equal_rings_zip_instead_of_refusing() {
    let step = TWO_PI / 10.0;
    let (verts, rings) = rings(&uniform(10, 0.0), &uniform(10, 0.4 * step));
    let tris = zip(&verts, &rings);
    assert_closed_band(&verts, &rings, &tris);
}

/// The start partner sitting just BEFORE bot[0] across the 0/2π cut (the
/// F0086 wrap-split shape) still zips: the signed start offset keeps the
/// unwrapped top azimuths monotone.
#[test]
fn start_partner_across_the_wrap_zips() {
    let mut top = uniform(8, 0.0);
    top[0] = TWO_PI - 1e-9; // the shared seam sample, a hair before 0
    let (verts, rings) = rings(&uniform(8, 0.0), &top);
    let tris = zip(&verts, &rings);
    assert_closed_band(&verts, &rings, &tris);
}

// ── §3b: one rim sample per model-scale point ────────────────────────────

use crate::boolean::rim_junction::RimAngleOverrides;
use crate::tests_unit::n2_junction::rj_cylinder;

/// The uniform cylinder tube: a sample inserted on ONE rim leaves the two
/// rings unequal, which the index-pairing path used to refuse ("cylinder
/// rims have mismatched / too-few samples") and now routes to the zipper.
#[test]
fn uniform_tube_routes_unequal_rings_to_the_zipper() {
    let a = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0);
    let natural = a.as_mesh().verts.len();
    let theta = 1.234_f64;
    let p = Point3::new(theta.cos(), theta.sin(), 0.0);
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![p]);
    let rebuilt = a
        .rebuilt_with_rim_overrides(&ov)
        .expect("a one-rim override set tessellates (unequal rings take the zipper)");
    let verts = &rebuilt.as_mesh().verts;
    assert_eq!(verts.len(), natural + 1, "one inserted sample");
    assert!(verts.contains(&p));
}

/// `RimAngleOverrides::finish`: two sample requests closer than the model's
/// coincidence band `TAU_MODEL·(1 + scale)` of arc are one sample; two
/// requests farther apart stay two.
#[test]
fn finish_merges_sample_requests_within_the_model_band() {
    let a = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0);
    let b = rj_cylinder([5.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0);
    let theta = 0.9_f64;
    // scale = |center| + r = 1 ⇒ band 2e-7 of arc on r = 1.
    for (delta, want) in [(1.0e-7, 1usize), (1.0e-5, 2usize)] {
        let mut acc = RimAngleOverrides::default();
        acc.commit(
            true,
            vec![0, 1],
            [0.0, 0.0, 1.0],
            vec![theta, theta + delta],
            0.1,
        );
        let (out_a, out_b) = acc.finish(&a, &b);
        assert!(out_b.is_empty());
        assert_eq!(out_a[&0].len(), want, "rim 0 at Δθ = {delta}");
        assert_eq!(out_a[&1].len(), want, "rim 1 at Δθ = {delta}");
    }
}

/// `RimAngleOverrides::finish`: a free sample within the model band of a point
/// the rim ALREADY carries (standing on the operand) is dropped — the
/// standing point keeps its bits.
#[test]
fn finish_drops_a_free_sample_the_rim_already_carries() {
    let a = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0);
    let b = rj_cylinder([5.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0);
    let theta = 2.1_f64;
    let standing_pt = Point3::new(theta.cos(), theta.sin(), 0.0);
    let mut ov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    ov.insert(0, vec![standing_pt]);
    let a = a.rebuilt_with_rim_overrides(&ov).expect("standing sample");
    let mut acc = RimAngleOverrides::default();
    acc.commit(true, vec![0, 1], [0.0, 0.0, 1.0], vec![theta], 0.1);
    let (out_a, _) = acc.finish(&a, &b);
    assert!(
        out_a.get(&0).is_none_or(|v| v.is_empty()),
        "rim 0 already carries the point: {out_a:?}"
    );
    assert_eq!(out_a[&1].len(), 1, "rim 1 does not: the sample stands");
}

/// `merge_rim_points` (the composition of a B-Rep's STANDING rim samples with
/// an incoming override map): an incoming point within `TAU_WORK` of a
/// standing point takes its slot with the incoming bits; a band-close pair
/// arriving TOGETHER both enter (each standing point is replaced at most
/// once, incoming points never absorb each other).
#[test]
fn merge_rim_points_replaces_a_standing_twin_once() {
    use crate::brep::merge_rim_points;
    let p = Point3::new(0.000121, 0.0004, 5.688387772537982e-5);
    let twin = Point3::new(0.00012099999999999999, 0.0004, 5.6883877725379815e-5);
    let far = Point3::new(0.000121, -0.0001, 5.688387772537982e-5);
    let mut base: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    base.insert(1, vec![p, far]);
    let mut extra: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    extra.insert(1, vec![twin]);
    let out = merge_rim_points(&base, &extra);
    assert_eq!(
        out[&1],
        vec![twin, far],
        "the twin takes p's slot, bits of the arrival"
    );
    // Two incoming twins of one standing point: the first takes the slot,
    // the second enters beside it (E-C1's band-close pair stays a pair).
    let twin2 = Point3::new(
        0.000121,
        f64::from_bits(0.0004_f64.to_bits() + 1),
        5.688387772537982e-5,
    );
    extra.insert(1, vec![twin, twin2]);
    let out = merge_rim_points(&base, &extra);
    assert_eq!(out[&1], vec![twin, far, twin2]);
    // Bit-identical repeats still compose to nothing.
    extra.insert(1, vec![p]);
    assert_eq!(merge_rim_points(&base, &extra)[&1], vec![p, far]);
    // A point 6e-8 away is DISTINCT geometry (R0072's micro-scale rim
    // points, measured 2026-10-13): below `TAU_MODEL`, far above
    // `TAU_WORK` — it enters beside the standing point, never replaces it.
    let near_real = Point3::new(0.000121 + 6.0e-8, 0.0004, 5.688387772537982e-5);
    extra.insert(1, vec![near_real]);
    assert_eq!(merge_rim_points(&base, &extra)[&1], vec![p, far, near_real]);
}
