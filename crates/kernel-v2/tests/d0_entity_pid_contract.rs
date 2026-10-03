//! D0 §4 item 4 — persistent identity through the `KernelIntrospect`
//! contract. `entity_pid` / `all_entity_pids` are the door
//! `waffle_types::Selector::Pid` resolves through, so this pins the contract
//! itself: the bulk and per-entity forms agree, faces report their lineage
//! root while edges and vertices are their own root, an unknown entity gets
//! `None`, and a mesh-backed imported body — which has no construction
//! history to seed from — reports nothing rather than a fabricated id.

use std::collections::HashMap;

use kernel_v2::KernelV2Adapter;
use waffle_types::kernel::{
    EntityPid, ImportedBodyData, ImportedEdgeData, ImportedFaceData, ImportedShellData,
    ImportedSurface, Kernel, KernelId, KernelIntrospect, KernelSolidHandle,
};
use waffle_types::{ClosedProfile, TopoKind};

fn box_solid(kernel: &mut KernelV2Adapter, a: f64, b: f64, h: f64) -> KernelSolidHandle {
    let positions: HashMap<u32, (f64, f64)> =
        [(1, (0.0, 0.0)), (2, (a, 0.0)), (3, (a, b)), (4, (0.0, b))]
            .into_iter()
            .collect();
    let faces = kernel
        .make_faces_from_profiles(
            &[ClosedProfile {
                entity_ids: vec![1, 2, 3, 4],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("rectangle stages");
    kernel
        .extrude_face(faces[0], [0.0, 0.0, 1.0], h)
        .expect("box")
}

fn pids_of(
    kernel: &KernelV2Adapter,
    solid: &KernelSolidHandle,
    kind: TopoKind,
) -> Vec<(KernelId, EntityPid)> {
    kernel.all_entity_pids(solid, kind)
}

#[test]
fn every_entity_of_a_box_has_an_identity_and_the_two_forms_agree() {
    let mut kernel = KernelV2Adapter::new();
    let solid = box_solid(&mut kernel, 0.02, 0.01, 0.005);

    for (kind, expected) in [
        (TopoKind::Face, 6usize),
        (TopoKind::Edge, 12),
        (TopoKind::Vertex, 8),
    ] {
        let bulk = pids_of(&kernel, &solid, kind);
        assert_eq!(bulk.len(), expected, "{kind:?}: entity count");
        let distinct: std::collections::BTreeSet<u64> = bulk.iter().map(|(_, p)| p.pid).collect();
        assert_eq!(distinct.len(), expected, "{kind:?}: pids all distinct");
        for (id, pid) in &bulk {
            assert_eq!(
                kernel.entity_pid(*id, kind).as_ref(),
                Some(pid),
                "{kind:?}: per-entity form must agree with the bulk form"
            );
        }
    }
}

#[test]
fn edges_and_vertices_are_their_own_root_faces_report_lineage() {
    let mut kernel = KernelV2Adapter::new();
    let solid = box_solid(&mut kernel, 0.02, 0.01, 0.005);

    for kind in [TopoKind::Edge, TopoKind::Vertex] {
        for (_, pid) in pids_of(&kernel, &solid, kind) {
            assert_eq!(
                pid.root_pid, pid.pid,
                "{kind:?}: a content-seeded id is its own root"
            );
        }
    }
    // A construct-born face has not passed through a boolean, so its journal
    // root is itself — and it must agree with `face_provenance`, which is the
    // older door onto the same pair.
    for (id, pid) in pids_of(&kernel, &solid, TopoKind::Face) {
        let fp = kernel.face_provenance(id).expect("face provenance");
        assert_eq!((pid.pid, pid.root_pid), (fp.pid, fp.root_pid));
    }
}

#[test]
fn the_wrong_kind_or_an_unknown_entity_has_no_identity() {
    let mut kernel = KernelV2Adapter::new();
    let solid = box_solid(&mut kernel, 0.02, 0.01, 0.005);
    let (face, _) = pids_of(&kernel, &solid, TopoKind::Face)[0];

    assert!(
        kernel.entity_pid(face, TopoKind::Edge).is_none(),
        "a face id asked for as an edge is not an identity"
    );
    assert!(
        kernel
            .entity_pid(KernelId(u64::MAX), TopoKind::Face)
            .is_none(),
        "an id from nowhere has no identity"
    );
    assert!(
        kernel.all_entity_pids(&solid, TopoKind::Shell).is_empty(),
        "shells carry no persistent identity"
    );
    assert!(
        kernel
            .all_entity_pids(&KernelSolidHandle::from_raw(9_999_999), TopoKind::Face)
            .is_empty(),
        "an unknown solid handle reports nothing"
    );
}

#[test]
fn a_mesh_backed_imported_body_reports_no_identity() {
    // A single triangle is enough: the point is that the mesh tier has no
    // construction history, so it must answer "no identity available" rather
    // than hand out an index-derived number that re-import would change.
    let mut kernel = KernelV2Adapter::new();
    let data = ImportedBodyData {
        source_name: "tri".to_string(),
        shells: vec![ImportedShellData {
            faces: vec![ImportedFaceData {
                surface: ImportedSurface::Plane {
                    origin: [0.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
                positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
                indices: vec![0, 1, 2],
                edge_indices: vec![0],
            }],
            edges: vec![ImportedEdgeData {
                polyline: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            }],
        }],
        warnings: vec![],
    };
    let solid = kernel.import_body(&data).expect("imported body");
    for kind in [TopoKind::Face, TopoKind::Edge, TopoKind::Vertex] {
        assert!(
            kernel.all_entity_pids(&solid, kind).is_empty(),
            "{kind:?}: a mesh-backed body has no persistent identity"
        );
    }
}
