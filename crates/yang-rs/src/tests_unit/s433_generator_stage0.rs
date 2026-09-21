#[allow(unused_imports)]
use super::*;

// ====================================================================
// Yang §4.3.3 GENERATOR tangency on the Stage-0 path (spec
// `specs/yang_433_tangent_point_mesh_update.md` §12, C0043).
//
// Two parallel-axis cylinders internally tangent along a generator, with
// COPLANAR caps: the generator mint runs BEFORE Stage 0 and its four rim
// samples are STANDING samples of the rebuilt operands, so Stage 0's ring
// readers see the tangent point on both rims with identical bits and the
// disc∩disc builder classifies the cap pair as a TOUCHING containment —
// one shared fan on both caps plus a pinched crescent on the outer cap —
// instead of a lens the arrangement then meshes twice.
// ====================================================================

use crate::boolean::tangent_generator_rim_overrides;
use crate::stage0::{crescent_tris, mk_v2, touching_containment, Frame, V2};
use crate::tests_unit::n2_junction::rj_cylinder;
use std::collections::BTreeMap;

fn xy_frame() -> Frame {
    Frame {
        n: [0.0, 0.0, 1.0],
        d: 0.0,
        o: [0.0, 0.0, 0.0],
        e1: [1.0, 0.0, 0.0],
        e2: [0.0, 1.0, 0.0],
    }
}

/// A CCW n-gon on the circle (`cx`, `cy`, `r`) with a vertex EXACTLY at
/// the given tangent point `t` (which must lie on the circle).
fn ring_with_vertex(cx: f64, cy: f64, r: f64, n: usize, t: Point3, frame: &Frame) -> Vec<V2> {
    let t0 = (t.y() - cy).atan2(t.x() - cx);
    (0..n)
        .map(|k| {
            if k == 0 {
                t
            } else {
                let th = t0 + k as f64 * std::f64::consts::TAU / n as f64;
                Point3::new(cx + r * th.cos(), cy + r * th.sin(), 0.0)
            }
        })
        .map(|p| mk_v2(p, frame).expect("finite"))
        .collect()
}

fn shoelace2(ring: &[V2]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|k| {
            let (p, q) = (&ring[k], &ring[(k + 1) % n]);
            p.u * q.v - q.u * p.v
        })
        .sum::<f64>()
        .abs()
}

fn tri_area2(t: &[Point3; 3]) -> f64 {
    let (a, b, c) = (t[0].as_array(), t[1].as_array(), t[2].as_array());
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// The C0043 cap pair in 2D: A = circle r 1 at the origin, B = circle r 0.4
/// at x = 0.6, tangent at (1, 0). The touching containment is recognized
/// with B inside A and the shared vertex located on both rings.
#[test]
fn s433_touching_containment_finds_the_one_shared_vertex() {
    let f = xy_frame();
    let t = Point3::new(1.0, 0.0, 0.0);
    let ring_a = ring_with_vertex(0.0, 0.0, 1.0, 14, t, &f);
    let ring_b = ring_with_vertex(0.6, 0.0, 0.4, 12, t, &f);
    let (inner_is_a, ti, to) =
        touching_containment(&ring_a, &ring_b).expect("B inside A touching at (1, 0)");
    assert!(!inner_is_a, "B is the inner ring");
    assert_eq!(ring_b[ti].p, t);
    assert_eq!(ring_a[to].p, t);
    // Strict containment (no shared vertex) is NOT this class.
    let ring_b_inside = ring_with_vertex(0.5, 0.0, 0.4, 12, Point3::new(0.9, 0.0, 0.0), &f);
    assert!(touching_containment(&ring_a, &ring_b_inside).is_none());
    // A crossing pair is not either.
    let ring_b_cross = ring_with_vertex(0.8, 0.0, 0.4, 12, Point3::new(1.2, 0.0, 0.0), &f);
    assert!(touching_containment(&ring_a, &ring_b_cross).is_none());
}

/// The crescent between the touching rings is covered EXACTLY: every
/// emitted triangle is CCW with positive area, none is degenerate, and the
/// areas sum to area(outer) − area(inner) (the exact certificate inside
/// `crescent_tris` passed, and the f64 shadow agrees).
#[test]
fn s433_crescent_between_touching_rings_is_covered_exactly() {
    let f = xy_frame();
    let t = Point3::new(1.0, 0.0, 0.0);
    let outer = ring_with_vertex(0.0, 0.0, 1.0, 14, t, &f);
    let inner = ring_with_vertex(0.6, 0.0, 0.4, 12, t, &f);
    let (_, ti, to) = touching_containment(&outer, &inner).expect("touching");
    let tris = crescent_tris(&outer, &inner, to, ti).expect("crescent triangulated");
    // P has 14 + 11 = 25 vertices → 23 ears, plus the tip.
    assert_eq!(tris.len(), 24);
    let mut sum2 = 0.0;
    for t in &tris {
        assert!(!crate::stage0::degenerate(t), "degenerate {t:?}");
        let a2 = tri_area2(t);
        assert!(a2 > 0.0, "not CCW / zero area: {t:?}");
        sum2 += a2;
    }
    let expect2 = shoelace2(&outer) - shoelace2(&inner);
    assert!(
        (sum2 - expect2).abs() < 1e-12,
        "crescent area {sum2} vs {expect2}"
    );
}

/// STANDING rim samples: a rim override inserted by `rebuilt_with_rim_
/// overrides` survives every later from-topology rebuild of that B-Rep
/// (the §4.5.2 re-derivation, a phantom-guard boost, an all-overrides
/// rebuild with nothing new), and a re-mint of the same point composes
/// bit-for-bit instead of duplicating the slot.
#[test]
fn s433_standing_rim_samples_survive_from_topology_rebuilds() {
    let a = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0);
    let b = rj_cylinder([0.6, 0.0, 0.0], [0.0, 0.0, 1.0], 0.4, 1.0);
    let (ga, _) = tangent_generator_rim_overrides(&a, &b);
    assert_eq!(ga.len(), 2, "one sample on each of A's two rims: {ga:?}");
    let has = |brep: &BRep| -> bool {
        ga.values()
            .flatten()
            .all(|p| brep.as_mesh().verts.contains(p))
    };
    assert!(
        !has(&a),
        "the natural mesh has no vertex at the tangent azimuth"
    );
    let boosted = a.rebuilt_with_rim_overrides(&ga).unwrap();
    assert!(has(&boosted));
    assert_eq!(boosted.standing_rim(), &ga);
    let n = boosted.as_mesh().verts.len();

    let again = boosted.retessellated_at_current_d_eps().unwrap();
    assert!(has(&again));
    assert_eq!(again.as_mesh().verts.len(), n);
    let empty = BTreeMap::new();
    let all = boosted
        .rebuilt_with_all_overrides(&empty, &empty, &empty)
        .unwrap();
    assert!(has(&all));
    assert_eq!(all.as_mesh().verts.len(), n);
    // Re-minting the standing samples is a no-op (composition dedups).
    let twice = boosted.rebuilt_with_rim_overrides(&ga).unwrap();
    assert_eq!(twice.as_mesh().verts.len(), n);
    assert_eq!(twice.standing_rim(), &ga);
}

/// Stage 0 on the boosted C0043 operands emits the overlap identically:
/// every cap triangle of the inner solid B is bit-identical to a triangle
/// of A's mesh (the shared fan), and A's caps additionally carry the
/// crescent — no cap triangle of A spans across B's rim.
#[test]
fn s433_stage0_emits_the_touching_disc_overlap_identically() {
    let a = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0);
    let b = rj_cylinder([0.6, 0.0, 0.0], [0.0, 0.0, 1.0], 0.4, 1.0);
    let (ga, gb) = tangent_generator_rim_overrides(&a, &b);
    let a2 = a.rebuilt_with_rim_overrides(&ga).unwrap();
    let b2 = b.rebuilt_with_rim_overrides(&gb).unwrap();
    let s0 = crate::stage0::stage0_preprocess(&a2, &b2)
        .expect("stage 0 ok")
        .expect("two coplanar cap pairs");
    assert_eq!(s0.pairs.len(), 2);
    let key = |m: &Mesh, t: &[u32; 3]| -> [Point3; 3] {
        let mut k = [
            m.verts[t[0] as usize],
            m.verts[t[1] as usize],
            m.verts[t[2] as usize],
        ];
        k.sort_by(|p, q| p.as_array().partial_cmp(&q.as_array()).unwrap());
        k
    };
    let a_keys: std::collections::BTreeSet<[[u64; 3]; 3]> = s0
        .mesh_a
        .tris
        .iter()
        .map(|t| key(&s0.mesh_a, t).map(|p| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]))
        .collect();
    let planar_b = |ti: usize| {
        let fi = s0.tri_face_b[ti] as usize;
        matches!(b2.faces()[fi].surface, Surface::Plane { .. })
    };
    let mut cap_b = 0usize;
    for (ti, t) in s0.mesh_b.tris.iter().enumerate() {
        if !planar_b(ti) {
            continue;
        }
        cap_b += 1;
        let k = key(&s0.mesh_b, t).map(|p| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]);
        assert!(
            a_keys.contains(&k),
            "B cap triangle {ti} {t:?} has no bit-identical twin in A's mesh"
        );
    }
    assert_eq!(cap_b, 24, "12-gon fan on each of B's two caps");
}

/// The Stage-0 entry declines EXTERNAL contact (C0042: two equal cylinders
/// touching from outside — a two-lobe union pinched along the line, the
/// pinch-edge family's output), while the idle-Stage-0 arm still mints it
/// (§11's contract, unchanged).
#[test]
fn s433_stage0_entry_declines_external_contact() {
    let a = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5, 1.0);
    let b = rj_cylinder([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5, 1.0);
    let (ga, gb) = tangent_generator_rim_overrides(&a, &b);
    assert!(
        ga.is_empty() && gb.is_empty(),
        "external contact must not boost: {ga:?} {gb:?}"
    );
    let (ta, tb) = crate::boolean::tangent_point_face_overrides(&a, &b);
    assert_eq!(
        ta.rim.len(),
        2,
        "the full arm mints the external ruling on A"
    );
    assert_eq!(tb.rim.len(), 2, "…and on B");
    let (p, _, external) = crate::boolean::cyl_cyl_tangent_generator_contact(
        (Point3::new(0.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 1.0), 0.5),
        (Point3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 1.0), 0.5),
    )
    .expect("tangent");
    assert!(external);
    assert_eq!(p, Point3::new(0.5, 0.0, 0.0));
}

fn mesh_signed_volume(m: &Mesh) -> f64 {
    m.tris
        .iter()
        .map(|t| {
            let (a, b, c) = (
                m.verts[t[0] as usize].as_array(),
                m.verts[t[1] as usize].as_array(),
                m.verts[t[2] as usize].as_array(),
            );
            let cx = b[1] * c[2] - b[2] * c[1];
            let cy = b[2] * c[0] - b[0] * c[2];
            let cz = b[0] * c[1] - b[1] * c[0];
            (a[0] * cx + a[1] * cy + a[2] * cz) / 6.0
        })
        .sum()
}

fn assert_closed_2_manifold(m: &Mesh) {
    let mut dir: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    for t in &m.tris {
        for k in 0..3 {
            let (s, e) = (t[k], t[(k + 1) % 3]);
            *dir.entry((s.min(e), s.max(e))).or_default() += if s < e { 1 } else { -1 };
            let c = dir.get(&(s.min(e), s.max(e))).copied().unwrap();
            assert!(c.abs() <= 1, "edge ({s},{e}) over-used");
        }
    }
    for ((s, e), c) in dir {
        assert_eq!(c, 0, "edge ({s},{e}) unpaired");
    }
}

/// C0043 end to end: the union of the internally tangent pair IS operand A
/// (B lies inside A, touching along the generator). The output is a closed
/// 2-manifold whose volume is A's own prism volume (the boosted 14-gon).
#[test]
fn s433_internally_tangent_cylinders_with_coplanar_caps_union_is_a() {
    let nb = crate::native_backend().expect("native backend");
    let a = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0);
    let b = rj_cylinder([0.6, 0.0, 0.0], [0.0, 0.0, 1.0], 0.4, 1.0);
    let out = crate::boolean(&a, &b, BoolOp::Union, &nb).expect("union completes");
    let m = out.as_mesh();
    assert_closed_2_manifold(m);
    let (ga, _) = tangent_generator_rim_overrides(&a, &b);
    let expect = mesh_signed_volume(a.rebuilt_with_rim_overrides(&ga).unwrap().as_mesh()).abs();
    let got = mesh_signed_volume(m).abs();
    assert!(
        (got - expect).abs() < 1e-9,
        "union volume {got} vs A's prism {expect}"
    );
    // Every output face is one of A's three surfaces.
    for f in out.faces() {
        match f.surface {
            Surface::Plane { .. } => {}
            Surface::Cylinder { radius, .. } => assert_eq!(radius, 1.0, "B's wall survived"),
            other => panic!("unexpected surface {other:?}"),
        }
    }
}
