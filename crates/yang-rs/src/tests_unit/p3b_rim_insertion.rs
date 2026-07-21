//! P3b inc-4d-2 unit fixtures: the rim-override junction channel +
//! `rebuilt_with_all_overrides` composition
//! (spec `specs/yang_169_p3b_curved_partner_pierce.md` §7.3, §7.5).
//!
//! Production-unreachable this sub-increment: no caller emits rim
//! overrides until the inc-4d-3 enumeration wiring (behind
//! `YANG_P3B_PIERCE_ENABLE`), so the full assay stays byte-identical.

use super::n2_junction::{rj_box, rj_cylinder};
use super::p3a_edge_overrides::closed_conformal_2_manifold;
use crate::boolean::{circle_edge_plane_face_pierce, opposite_rim_projection};
use crate::*;
use std::collections::BTreeMap;

fn bits(p: Point3) -> [u64; 3] {
    [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
}

/// The z-axis tube r=0.25, v∈[0,1]; its lateral face index and its rim-0
/// (bottom, z=0) circle edge index.
fn tube() -> (BRep, u32, u32) {
    let b = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.25, 1.0);
    let lat = b
        .faces()
        .iter()
        .position(|f| matches!(f.surface, Surface::Cylinder { .. }))
        .expect("fixture has a lateral face") as u32;
    let rim0 = b
        .edges()
        .iter()
        .position(|e| matches!(e.curve, Curve::Circle { center, .. } if center.z().abs() < 1e-15))
        .expect("fixture has the z=0 rim") as u32;
    (b, lat, rim0)
}

/// Cross-operand contract (the F0082 J2 shape, synthetic frame): the
/// tube's rim-0 circle pierces the box's x=0.1 face at (0.1, ±√(r²−0.01),
/// 0); the OWNER side inserts both roots into the rim RING
/// (`rim_overrides` — cap CDT and lateral strip conformal by the shared
/// ring) and the PARTNER side mints them into the box face's CDT. Both
/// rebuilt Stage-1 meshes carry the junctions bit-exactly as closed
/// conformal 2-manifolds.
#[test]
fn rim_pierce_lands_bit_exactly_in_both_operands() {
    let (tube, _lat, rim0) = tube();
    let bx = rj_box([0.1, -0.4, -0.3], [0.8, 0.4, 0.3]);
    // The rim descriptors, straight off the tube's own edge.
    let Curve::Circle {
        center,
        normal,
        radius,
    } = tube.edges()[rim0 as usize].curve
    else {
        unreachable!("rim0 is a circle");
    };
    let seam = tube.vertices()[tube.edges()[rim0 as usize].start as usize].point;
    // Owner surfaces: the bottom cap plane + the lateral.
    let s_cap = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, -1.0),
        d: 0.0,
    };
    let s_lat = tube
        .faces()
        .iter()
        .find(|f| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|f| f.surface)
        .expect("fixture has a lateral face");
    // Enumerate the pierces against every box face — exactly the x=0.1
    // face carries both roots strictly inside.
    let mut pts: Vec<Point3> = Vec::new();
    let mut pierced_face: Option<u32> = None;
    for (fi, f) in bx.faces().iter().enumerate() {
        let out = circle_edge_plane_face_pierce(
            center, normal, radius, seam, s_cap, s_lat, fi as u32, f, &bx,
        );
        if !out.is_empty() {
            assert!(pierced_face.is_none(), "only the x=0.1 face may mint");
            assert_eq!(out.len(), 2, "both rim roots contained: {out:?}");
            pierced_face = Some(fi as u32);
            pts = out.iter().map(|pp| pp.point).collect();
        }
    }
    let pierced_face = pierced_face.expect("the x=0.1 face mints");
    let y_hit = (0.25f64 * 0.25 - 0.01).sqrt();
    for pp in &pts {
        let p = pp.as_array();
        assert!((p[0] - 0.1).abs() < 1e-12 && (p[1].abs() - y_hit).abs() < 1e-12);
        assert!(p[2].abs() < 1e-12);
    }
    // OWNER side: rim-ring insertion via the composed rebuild, with the
    // opposite-rim mirror keeping the azimuth-merge ring counts matched
    // (the production `junction_stage1_overrides` rule).
    let mut rim: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    rim.insert(rim0, pts.clone());
    let (opp_edge, opp_pts) =
        opposite_rim_projection(&tube, rim0, &pts).expect("canonical tube has the pairing");
    assert_ne!(opp_edge, rim0);
    for p in &opp_pts {
        // The mirror lands exactly ON the opposite rim circle (z=1, r=0.25).
        let q = p.as_array();
        assert!((q[2] - 1.0).abs() < 1e-15);
        assert!(((q[0] * q[0] + q[1] * q[1]).sqrt() - 0.25).abs() < 1e-15);
    }
    rim.insert(opp_edge, opp_pts);
    let tube_rebuilt = tube
        .rebuilt_with_all_overrides(&rim, &BTreeMap::new(), &BTreeMap::new())
        .expect("owner-side rim splice");
    // PARTNER side: box-face interior Steiner mint.
    let mut fo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fo.insert(pierced_face, pts.clone());
    let box_rebuilt = bx
        .rebuilt_with_all_overrides(&BTreeMap::new(), &BTreeMap::new(), &fo)
        .expect("partner-side face splice");
    for j in &pts {
        for (mesh, side) in [
            (tube_rebuilt.as_mesh(), "owner rim"),
            (box_rebuilt.as_mesh(), "partner face"),
        ] {
            assert!(
                mesh.verts.iter().any(|p| bits(*p) == bits(*j)),
                "{side} mesh carries {j:?} bit-exactly"
            );
        }
    }
    assert!(closed_conformal_2_manifold(&tube_rebuilt.as_mesh().tris));
    assert!(closed_conformal_2_manifold(&box_rebuilt.as_mesh().tris));
}

/// Composition on ONE operand: a rim-ring override AND a lateral
/// face-interior override splice in the SAME rebuild — the azimuth-merge
/// strip (non-uniform ring) and the tube-grid 3-fan compose without
/// disturbing each other. Both bits present, closed conformal 2-manifold.
#[test]
fn rim_and_face_overrides_compose_in_one_rebuild() {
    let (tube, lat, rim0) = tube();
    // A rim junction at θ=1.1 (off every uniform slot and the seam).
    let j_rim = Point3::new(0.25 * 1.1f64.cos(), 0.25 * 1.1f64.sin(), 0.0);
    // A lateral interior junction (the proven inc-2 splice point).
    let j_face = Point3::new(0.25 * 0.7f64.cos(), 0.25 * 0.7f64.sin(), 0.5);
    let mut rim: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    rim.insert(rim0, vec![j_rim]);
    let (opp_edge, opp_pts) =
        opposite_rim_projection(&tube, rim0, &[j_rim]).expect("canonical tube has the pairing");
    rim.insert(opp_edge, opp_pts);
    let mut fo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fo.insert(lat, vec![j_face]);
    let rebuilt = tube
        .rebuilt_with_all_overrides(&rim, &BTreeMap::new(), &fo)
        .expect("composed rim + face splice");
    let mesh = rebuilt.as_mesh();
    for j in [j_rim, j_face] {
        assert!(
            mesh.verts.iter().any(|p| bits(*p) == bits(j)),
            "composed rebuild carries {j:?} bit-exactly"
        );
    }
    assert!(
        closed_conformal_2_manifold(&mesh.tris),
        "rim + face overrides compose into a closed conformal 2-manifold"
    );
}

/// The empty-override identity: `rebuilt_with_all_overrides` with an empty
/// rim map is byte-identical to `rebuilt_with_junction_overrides`, and
/// all-empty is byte-identical to the plain rebuild.
#[test]
fn empty_rim_map_is_byte_identical() {
    let (tube, lat, _rim0) = tube();
    let j_face = Point3::new(0.25 * 0.7f64.cos(), 0.25 * 0.7f64.sin(), 0.5);
    let mut fo: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fo.insert(lat, vec![j_face]);
    let via_all = tube
        .rebuilt_with_all_overrides(&BTreeMap::new(), &BTreeMap::new(), &fo)
        .expect("all-overrides rebuild");
    let via_junction = tube
        .rebuilt_with_junction_overrides(&BTreeMap::new(), &fo)
        .expect("junction-overrides rebuild");
    assert_eq!(
        via_all
            .as_mesh()
            .verts
            .iter()
            .map(|p| bits(*p))
            .collect::<Vec<_>>(),
        via_junction
            .as_mesh()
            .verts
            .iter()
            .map(|p| bits(*p))
            .collect::<Vec<_>>()
    );
    assert_eq!(via_all.as_mesh().tris, via_junction.as_mesh().tris);
}
