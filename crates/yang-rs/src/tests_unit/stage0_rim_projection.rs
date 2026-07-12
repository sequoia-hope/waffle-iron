#[allow(unused_imports)]
use super::*;

// ====================================================================
// Task #144 (spec `m8_exact_opposite_rim_projection` — P10 REFUTATION
// RECORD): the opposite-rim placement in `collect_ring_crossings` is the
// f64 radial renormalisation — every projected sample must land ON the
// opposite rim circle (within the stage1 rim band) and ON the opposite
// cap plane, because a projected sample on a rim with no own crossings
// is pure scaffolding that nothing downstream relocates (the
// n2_rim_mint_adversary on-surface contract; this is what refuted the
// exact-translation arm, which mirrored chord-DEEP fused survivors
// off-surface). The known residual: same-ray radial twin pairs collapse
// to one on-circle image (C0048 66v69 / F0067 572v571) — the downstream
// azimuth-merge count wall stays LOUD, never silent.
// ====================================================================

use crate::coplanar_overlay::{rat, ClassifiedOverlay, ExactPoint2};
use crate::stage0::{collect_ring_crossings, RimSplitMap};
use cad_primitives::Point2;
use dashu::rational::RBig;

/// Uniform N-gon ring of the radius-`r` circle at the origin (the cap's
/// 2D rim polygon in the identity cap-plane frame).
fn uniform_ring(n: usize, r: f64) -> Vec<Point2> {
    (0..n)
        .map(|i| {
            let th = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
            let (s, c) = th.sin_cos();
            Point2::new(r * c, r * s)
        })
        .collect()
}

/// Exact interior point of the chord `ring[i] → ring[i+1]` at rational
/// parameter `t` (exactly collinear by construction), plus its rounded
/// f64 image (the bit-exact shared 3D lift the production path uses).
fn chord_point(ring: &[Point2], i: usize, t: RBig) -> (ExactPoint2, Point2) {
    let s = &ring[i];
    let e = &ring[(i + 1) % ring.len()];
    let (sx, sy) = (rat(s.x()).unwrap(), rat(s.y()).unwrap());
    let (ex, ey) = (rat(e.x()).unwrap(), rat(e.y()).unwrap());
    let qx = &sx + &t * (&ex - &sx);
    let qy = &sy + &t * (&ey - &sy);
    let rounded = Point2::new(qx.to_f64().value(), qy.to_f64().value());
    (ExactPoint2 { x: qx, y: qy }, rounded)
}

/// Minimal overlay carrying only `exact_verts` (the sole field
/// `collect_ring_crossings` reads), with the 1:1 rounded `verts`.
fn overlay_of(exact: Vec<ExactPoint2>) -> ClassifiedOverlay {
    let verts = exact
        .iter()
        .map(|q| Point2::new(q.x.to_f64().value(), q.y.to_f64().value()))
        .collect();
    ClassifiedOverlay {
        verts,
        exact_verts: exact,
        tris: Vec::new(),
        class: Vec::new(),
        poly_a: Vec::new(),
        poly_b: Vec::new(),
        fused: Default::default(),
    }
}

const BITS: fn(&Point3) -> [u64; 3] = |p| p.as_array().map(f64::to_bits);

/// The on-surface scaffolding contract (the invariant that REFUTED the
/// exact-translation arm): an OFF-CIRCLE cap crossing (a #142 fused
/// survivor at chord depth) projects onto the opposite rim ON the circle
/// — radial residual within the stage1 rim band — and ON the opposite
/// cap plane, at the source's azimuth. A chord-deep mirror here would be
/// unrelocated off-surface geometry on rims with no own crossings.
#[test]
pub(crate) fn opposite_rim_projection_lands_on_circle_within_band() {
    let r = 0.5;
    let (verts, edges, faces) = rt_cylinder(0.0, 1.0, r);
    let brep = BRep::new(verts, edges, faces).expect("cylinder brep");
    let ring = uniform_ring(8, r);
    let t = RBig::from(3u8) / RBig::from(8u8);
    let (q_exact, q_round) = chord_point(&ring, 0, t);
    let cap_pt = Point3::new(q_round.x(), q_round.y(), 0.0);
    // Fixture sanity: the source is genuinely chord-deep (off-circle).
    let src_r = (cap_pt.as_array()[0].powi(2) + cap_pt.as_array()[1].powi(2)).sqrt();
    assert!(r - src_r > 1e-3, "fixture: source must sit at chord depth");
    let overlay = overlay_of(vec![q_exact]);

    let mut ov: RimSplitMap = Default::default();
    collect_ring_crossings(&brep, 0, &ring, &overlay, &[cap_pt], &mut ov)
        .expect("ring crossings must collect");

    let cap_entry = ov.get(&0).expect("cap rim entry");
    assert_eq!(cap_entry.len(), 1);
    assert_eq!(BITS(&cap_entry[0]), BITS(&cap_pt));

    let opp_entry = ov.get(&1).expect("opposite rim entry");
    assert_eq!(opp_entry.len(), 1, "one cap crossing → one opposite sample");
    let o = opp_entry[0].as_array();
    let band = 1e-9 * (1.0 + r);
    let opp_r = (o[0] * o[0] + o[1] * o[1]).sqrt();
    assert!(
        (opp_r - r).abs() <= band,
        "opposite sample must lie ON the rim circle within the stage1 band \
         (got radial {opp_r} vs radius {r})"
    );
    assert_eq!(
        o[2], 1.0,
        "opposite sample must lie exactly on the opposite plane"
    );
    // Same azimuth as the source (the projection is radial).
    let az_src = cap_pt.as_array()[1].atan2(cap_pt.as_array()[0]);
    let az_opp = o[1].atan2(o[0]);
    assert!((az_src - az_opp).abs() < 1e-12, "azimuth must be preserved");
}

/// Characterisation of the KNOWN residual (task #144 refutation record):
/// same-ray radial twins — two bit-distinct cap points on an exactly
/// radial chord — project onto the opposite rim at the SAME exact
/// azimuth. Both images land on-circle within band; for this fixture the
/// f64 renormalisation happens to keep their last-bit images distinct
/// (C0048's real pairs collide bit-exactly — the documented open count
/// deficit, kept LOUD downstream by the azimuth-merge multiset wall).
#[test]
pub(crate) fn opposite_rim_projection_same_ray_twins_land_on_circle() {
    let r = 0.5;
    let (verts, edges, faces) = rt_cylinder(0.0, 1.0, r);
    let brep = BRep::new(verts, edges, faces).expect("cylinder brep");
    // Explicit ring where chord 1 is the RADIAL segment 0.9·u1 → u1 — two
    // interior points on it are exact same-azimuth radial twins.
    let n = 8;
    let base = uniform_ring(n, r);
    let mut ring: Vec<Point2> = Vec::with_capacity(n + 1);
    ring.push(base[0]);
    let u1 = base[1];
    ring.push(Point2::new(0.9 * u1.x(), 0.9 * u1.y()));
    ring.push(u1);
    ring.extend_from_slice(&base[2..]);
    let (qa_exact, qa_round) = chord_point(&ring, 1, RBig::from(1u8) / RBig::from(4u8));
    let (qb_exact, qb_round) = chord_point(&ring, 1, RBig::from(3u8) / RBig::from(4u8));
    let pa = Point3::new(qa_round.x(), qa_round.y(), 0.0);
    let pb = Point3::new(qb_round.x(), qb_round.y(), 0.0);
    assert_ne!(BITS(&pa), BITS(&pb), "fixture: twins must be bit-distinct");
    let overlay = overlay_of(vec![qa_exact, qb_exact]);

    let mut ov: RimSplitMap = Default::default();
    collect_ring_crossings(&brep, 0, &ring, &overlay, &[pa, pb], &mut ov)
        .expect("ring crossings must collect");

    assert_eq!(
        ov.get(&0).map(Vec::len),
        Some(2),
        "both twins on the cap rim"
    );
    let opp = ov.get(&1).expect("opposite rim entry");
    let band = 1e-9 * (1.0 + r);
    for o in opp.iter().map(|p| p.as_array()) {
        let opp_r = (o[0] * o[0] + o[1] * o[1]).sqrt();
        assert!(
            (opp_r - r).abs() <= band,
            "every projected sample must lie ON the circle within band"
        );
        assert_eq!(o[2], 1.0);
    }
    // Current behaviour for THIS fixture (deterministic): both images
    // survive with distinct last bits. When a pair DOES collide (C0048),
    // the count deficit surfaces as the loud azimuth-merge wall.
    assert_eq!(opp.len(), 2);
    assert_ne!(BITS(&opp[0]), BITS(&opp[1]));
}

/// Byte-pin of the renormalisation formula (unequal rim radius bits —
/// the placement renormalises the radial component to the OPPOSITE rim's
/// radius): computed here inline with the same expression sequence. Any
/// future projection change must keep this arm byte-identical or amend
/// the #144 spec.
#[test]
pub(crate) fn opposite_rim_projection_unequal_radius_keeps_renormalisation() {
    let (verts, edges, mut faces) = rt_cylinder(0.0, 1.0, 0.5);
    // Legal but radius-bit-mismatched top rim: nudge the top circle's
    // radius by 1 ULP. The Surface::Cylinder stays at 0.5 (the gate keys
    // on the two rim circles' bits).
    let opp_r = f64::from_bits(0.5f64.to_bits() + 1);
    let mut edges = edges;
    if let Curve::Circle { center, normal, .. } = edges[1].curve {
        edges[1].curve = Curve::Circle {
            center,
            normal,
            radius: opp_r,
        };
    } else {
        panic!("fixture: edge 1 must be the top rim circle");
    }
    faces.truncate(faces.len()); // no-op; keep faces as built
    let brep = BRep::new(verts, edges, faces).expect("cylinder brep");

    let ring = uniform_ring(8, 0.5);
    let (q_exact, q_round) = chord_point(&ring, 0, RBig::from(3u8) / RBig::from(8u8));
    let cap_pt = Point3::new(q_round.x(), q_round.y(), 0.0);
    let overlay = overlay_of(vec![q_exact]);

    let mut ov: RimSplitMap = Default::default();
    collect_ring_crossings(&brep, 0, &ring, &overlay, &[cap_pt], &mut ov)
        .expect("ring crossings must collect");

    // Legacy renormalisation, same expression sequence as the fallback arm.
    let axis_point = [0.0, 0.0, 0.0];
    let axis_dir = [0.0, 0.0, 1.0];
    let oc = [0.0, 0.0, 1.0];
    let p = cap_pt.as_array();
    let w = [
        p[0] - axis_point[0],
        p[1] - axis_point[1],
        p[2] - axis_point[2],
    ];
    let axial = w[0] * axis_dir[0] + w[1] * axis_dir[1] + w[2] * axis_dir[2];
    let radial = [
        w[0] - axial * axis_dir[0],
        w[1] - axial * axis_dir[1],
        w[2] - axial * axis_dir[2],
    ];
    let rlen = (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
    let scale = opp_r / rlen;
    let expected = Point3::new(
        oc[0] + radial[0] * scale,
        oc[1] + radial[1] * scale,
        oc[2] + radial[2] * scale,
    );

    let opp = ov.get(&1).expect("opposite rim entry");
    assert_eq!(opp.len(), 1);
    assert_eq!(
        BITS(&opp[0]),
        BITS(&expected),
        "unequal radius bits must keep the legacy renormalisation byte-identically"
    );
}
