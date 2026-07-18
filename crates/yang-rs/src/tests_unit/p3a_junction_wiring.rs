//! P3a #146 increment-2 unit fixtures: the junction wiring layer — the
//! `junction_stage1_overrides` builder (owner edge maps + partner face
//! interior maps), the Stage-1 face-interior override channel, and the
//! operand-rebuild conformality contract (both operands carry every
//! junction point bit-exactly, spec `yang_146_conformal_junction_sampling.md`
//! §3.3/§4).

use super::n2_junction::rj_box;
use super::p3a_edge_overrides::closed_conformal_2_manifold;
use crate::boolean::junction_stage1_overrides;
use crate::*;
use std::collections::BTreeMap;

fn box_parts() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let b = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    (
        b.vertices().to_vec(),
        b.edges().to_vec(),
        b.faces().to_vec(),
    )
}

fn bits(p: Point3) -> [u64; 3] {
    [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
}

/// Interpenetrating boxes (the 1a lead fixture): B's four vertical edges
/// pierce A's top face. The builder must produce OWNER-side edge overrides
/// for all 8 per-loop copies of B's 4 geometric edges, and PARTNER-side
/// interior points on A's top face — the 4 pierce points, deduplicated
/// across the per-copy fan-out, identical exact bits on both sides.
#[test]
fn overrides_builder_maps_owner_and_partner_sides() {
    let a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = rj_box([0.3, 0.3, 0.5], [0.7, 0.7, 1.5]);
    let jo = junction_stage1_overrides(&a, &b);

    assert!(jo.edge_a.is_empty(), "no A edge pierces B: {:?}", jo.edge_a);
    assert!(jo.face_b.is_empty(), "no B face is pierced by A");
    assert_eq!(
        jo.edge_b.len(),
        8,
        "4 geometric B edges × 2 per-loop copies"
    );
    // Partner side: exactly A's top face (rj_box face 1, plane z=1), with
    // the 4 deduped pierce points on it.
    assert_eq!(jo.face_a.len(), 1, "one pierced A face: {:?}", jo.face_a);
    let (f_idx, pts) = jo.face_a.iter().next().unwrap();
    assert_eq!(*f_idx, 1, "rj_box top face index");
    assert_eq!(pts.len(), 4, "4 junction points, per-copy fan-out deduped");
    for p in pts {
        assert!((p.z() - 1.0).abs() < 1e-12, "pierce on A's top plane");
    }
    // Cross-side identity: every partner-side point appears bitwise in some
    // owner-side edge list (one mint, shared by identity).
    for p in pts {
        let found = jo
            .edge_b
            .values()
            .any(|list| list.iter().any(|q| bits(*q) == bits(*p)));
        assert!(found, "partner-side point {p:?} missing from owner side");
    }
}

/// Disjoint solids: the builder returns an all-empty payload.
#[test]
fn overrides_builder_empty_for_disjoint() {
    let a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = rj_box([3.0, 3.0, 3.0], [4.0, 4.0, 4.0]);
    assert!(junction_stage1_overrides(&a, &b).is_empty());
}

/// A face-interior junction point mints exactly one Steiner vertex with a
/// `BRepFace` source, is consumed by that face's keep-interior CDT, and the
/// box stays a closed, consistently wound 2-manifold.
#[test]
fn face_interior_override_mints_and_is_consumed() {
    let (verts, edges, faces) = box_parts();
    let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
    let j = Point3::new(0.5, 0.5, 1.0);
    let mut fov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fov.insert(1, vec![j]); // face 1 = top (z=1)
    let empty: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    let t = stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &empty, &fov, None)
        .expect("face interior override");
    assert_eq!(t.verts.len(), plain.verts.len() + 1, "one interior mint");
    let jv = (0..t.verts.len() as u32)
        .find(|&i| bits(t.verts[i as usize]) == bits(j))
        .expect("interior point minted bit-exactly");
    assert!(
        matches!(
            t.sources[jv as usize],
            TessellationSource::BRepFace { face: 1, .. }
        ),
        "interior Steiner source must be the pierced face"
    );
    let consumed = t.face_tri_ranges[1]
        .clone()
        .any(|ti| t.tris[ti].contains(&jv));
    assert!(consumed, "top face CDT must consume the interior point");
    assert!(closed_conformal_2_manifold(&t.tris));
}

/// A face override targeting a NON-PLANAR face is a loud STOP.
#[test]
fn face_override_nonplanar_target_is_loud() {
    let (verts, edges, mut faces) = box_parts();
    faces[1].surface = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let mut fov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fov.insert(1, vec![Point3::new(0.5, 0.5, 1.0)]);
    let empty: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    let Err(YangError::MalformedTopology(msg)) =
        stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &empty, &fov, None)
    else {
        panic!("non-planar face target must be loud");
    };
    assert!(msg.contains("non-planar"), "unexpected: {msg}");
}

/// A face override off the face's plane is a loud STOP.
#[test]
fn face_override_off_plane_is_loud() {
    let (verts, edges, faces) = box_parts();
    let mut fov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fov.insert(1, vec![Point3::new(0.5, 0.5, 0.9)]);
    let empty: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    let Err(YangError::MalformedTopology(msg)) =
        stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &empty, &fov, None)
    else {
        panic!("off-plane point must be loud");
    };
    assert!(msg.contains("off the face plane"), "unexpected: {msg}");
}

/// An on-plane point OUTSIDE the face's bounded region must not silently
/// drop (one-sided mint) — the consumed postcondition is a loud STOP.
#[test]
fn face_override_outside_region_is_loud() {
    let (verts, edges, faces) = box_parts();
    let mut fov: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    fov.insert(1, vec![Point3::new(2.0, 2.0, 1.0)]);
    let empty: BTreeMap<u32, Vec<Point3>> = BTreeMap::new();
    let Err(YangError::MalformedTopology(msg)) =
        stage1_tessellate_with_edge_overrides(&verts, &edges, &faces, &empty, &fov, None)
    else {
        panic!("outside-region point must be loud");
    };
    assert!(
        msg.contains("not consumed") || msg.contains("keep-interior"),
        "unexpected: {msg}"
    );
}

/// End-to-end operand rebuild: both rebuilt operands carry every junction
/// point BIT-EXACTLY (owner as an edge-polyline Steiner, partner as a face
/// interior Steiner), minted exactly once per operand, and both Stage-1
/// meshes stay closed, consistently wound 2-manifolds — the arrangement's
/// exact coincidence merge then shares each junction by identity.
#[test]
fn rebuilt_operands_share_junction_verts_bitwise() {
    let a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = rj_box([0.3, 0.3, 0.5], [0.7, 0.7, 1.5]);
    let jo = junction_stage1_overrides(&a, &b);
    let a2 = a
        .rebuilt_with_junction_overrides(&jo.edge_a, &jo.face_a)
        .expect("A rebuild");
    let b2 = b
        .rebuilt_with_junction_overrides(&jo.edge_b, &jo.face_b)
        .expect("B rebuild");
    let junctions: &Vec<Point3> = &jo.face_a[&1];
    assert_eq!(junctions.len(), 4);
    for j in junctions {
        for (tag, m) in [("A", &a2.mesh), ("B", &b2.mesh)] {
            let count = m.verts.iter().filter(|q| bits(**q) == bits(*j)).count();
            assert_eq!(count, 1, "junction {j:?} minted once in operand {tag}");
        }
    }
    assert!(
        closed_conformal_2_manifold(&a2.mesh.tris),
        "operand A mesh must stay closed and consistently wound"
    );
    assert!(
        closed_conformal_2_manifold(&b2.mesh.tris),
        "operand B mesh must stay closed and consistently wound"
    );
}
