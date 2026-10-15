//! Yang §4.5 failure sites — Phase 1b of `specs/yang_faithful_cleanup.md`
//! (spec `specs/yang_452_refinement_loop.md` §3–§4): every trigger names the
//! input faces at its site, and the region is those faces plus one ring.

use crate::boolean::failures::{
    self, faces_at_vertex, illegal_intersection_sites, record_nonmanifold_mesh, region,
    with_one_ring, FailureSite, FailureTrigger,
};
use crate::brep::{InputId, TriangleAttribution, TriangleAttributionMap};
use crate::tests_unit::rj_cylinder;
use crate::{Mesh, Point3};
use std::collections::BTreeSet;

fn attr(pairs: &[Option<(InputId, u32)>]) -> TriangleAttributionMap {
    TriangleAttributionMap {
        attributions: pairs
            .iter()
            .map(|p| p.map(|(input, face)| TriangleAttribution { input, face }))
            .collect(),
    }
}

/// Two triangles sharing the edge (0,1): tri 0 on A:3, tri 1 on B:7.
fn two_tris() -> (Mesh, TriangleAttributionMap) {
    let mesh = Mesh::new(
        vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(0.0, 0.0, 1.0),
        ],
        vec![[0, 1, 2], [1, 0, 3]],
    );
    (mesh, attr(&[Some((InputId::A, 3)), Some((InputId::B, 7))]))
}

#[test]
fn faces_at_a_vertex_are_the_distinct_incident_attributions() {
    let (mesh, at) = two_tris();
    assert_eq!(
        faces_at_vertex(&mesh, &at, 0),
        vec![(InputId::A, 3), (InputId::B, 7)]
    );
    assert_eq!(faces_at_vertex(&mesh, &at, 2), vec![(InputId::A, 3)]);
    assert!(faces_at_vertex(&mesh, &at, 9).is_empty());
}

#[test]
fn the_record_is_per_run_and_a_sentinel_vertex_names_no_face() {
    failures::reset();
    assert!(failures::recorded().is_empty());
    let (mesh, at) = two_tris();
    failures::record_relocation_divergence(&mesh, &at, 1, "OffCurveBeyondChordBand");
    failures::record_relocation_divergence(&mesh, &at, u32::MAX, "LocalRefinementRequired");
    let rec = failures::recorded();
    assert_eq!(rec.len(), 2);
    assert_eq!(rec[0].trigger, FailureTrigger::RelocationDiverged);
    assert_eq!(rec[0].faces, vec![(InputId::A, 3), (InputId::B, 7)]);
    assert!(rec[1].faces.is_empty(), "a sentinel STOP has no site");
    failures::reset();
    assert!(failures::recorded().is_empty());
}

#[test]
fn an_open_mesh_records_a_nonmanifold_site_naming_its_faces() {
    failures::reset();
    let (mesh, at) = two_tris(); // every edge but (0,1) is unpaired
    record_nonmanifold_mesh(&mesh, &at);
    let rec = failures::recorded();
    assert!(!rec.is_empty());
    assert!(rec
        .iter()
        .all(|s| s.trigger == FailureTrigger::NonManifoldMesh));
    let named: BTreeSet<(InputId, u32)> =
        rec.iter().flat_map(|s| s.faces.iter().copied()).collect();
    assert!(named.contains(&(InputId::A, 3)));
    assert!(named.contains(&(InputId::B, 7)));
    failures::reset();
}

#[test]
fn crossing_triangles_are_an_illegal_intersection_naming_both_faces() {
    // Two triangles whose interiors cross: one in z = 0, one vertical
    // through its middle, sharing no vertex index.
    let mesh = Mesh::new(
        vec![
            Point3::new(-1.0, -1.0, 0.0),
            Point3::new(1.0, -1.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(0.0, -0.2, -1.0),
            Point3::new(0.0, -0.2, 1.0),
            Point3::new(0.0, 0.4, 0.0),
        ],
        vec![[0, 1, 2], [3, 4, 5]],
    );
    let at = attr(&[Some((InputId::A, 1)), Some((InputId::B, 2))]);
    let sites = illegal_intersection_sites(&mesh, &at);
    assert_eq!(sites.len(), 1, "{sites:?}");
    assert_eq!(sites[0].trigger, FailureTrigger::IllegalIntersection);
    assert_eq!(sites[0].faces, vec![(InputId::A, 1), (InputId::B, 2)]);
    // Disjoint triangles are clean.
    let far = Mesh::new(
        vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(5.0, 5.0, 5.0),
            Point3::new(6.0, 5.0, 5.0),
            Point3::new(5.0, 6.0, 5.0),
        ],
        vec![[0, 1, 2], [3, 4, 5]],
    );
    assert!(illegal_intersection_sites(&far, &at).is_empty());
}

#[test]
fn the_region_is_the_named_faces_plus_one_ring_per_operand() {
    // rj_cylinder: face 0 lateral, 1 bottom cap, 2 top cap; the caps meet
    // the lateral at the seam vertices, never each other.
    let a = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 2.0);
    let b = rj_cylinder([5.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 2.0);
    let cap: BTreeSet<u32> = [1u32].into_iter().collect();
    assert_eq!(
        with_one_ring(&a, &cap),
        [0u32, 1].into_iter().collect::<BTreeSet<u32>>(),
        "a cap's ring is the lateral alone"
    );
    let lateral: BTreeSet<u32> = [0u32].into_iter().collect();
    assert_eq!(
        with_one_ring(&a, &lateral),
        [0u32, 1, 2].into_iter().collect::<BTreeSet<u32>>(),
        "the lateral's ring is both caps"
    );
    assert!(with_one_ring(&a, &[9u32].into_iter().collect()).is_empty());
    let sites = vec![
        FailureSite {
            trigger: FailureTrigger::RelocationDiverged,
            faces: vec![(InputId::A, 1)],
            detail: String::new(),
        },
        FailureSite {
            trigger: FailureTrigger::IllegalIntersection,
            faces: vec![(InputId::B, 2)],
            detail: String::new(),
        },
    ];
    let (ra, rb) = region(&sites, &a, &b);
    assert_eq!(ra, [0u32, 1].into_iter().collect::<BTreeSet<u32>>());
    assert_eq!(rb, [0u32, 2].into_iter().collect::<BTreeSet<u32>>());
}

#[test]
fn an_unrecorded_error_is_one_faceless_reassembly_stop() {
    failures::reset();
    let err: Result<crate::BRep, crate::YangError> = Err(crate::YangError::NonManifoldOutput);
    let f = failures::failures(&err);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].trigger, FailureTrigger::ReassemblyStop);
    assert!(f[0].faces.is_empty());
}

#[test]
fn a_clean_body_has_an_empty_failure_set() {
    failures::reset();
    let a = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 2.0);
    let ok: Result<crate::BRep, crate::YangError> = Ok(a);
    let f = failures::failures(&ok);
    assert!(f.is_empty(), "{f:?}");
}

/// A tetrahedron `[o, a, b, c]` with outward winding, every triangle on
/// `face`.
fn tet_tris(o: u32, a: u32, b: u32, c: u32) -> Vec<[u32; 3]> {
    vec![[o, b, a], [o, a, c], [o, c, b], [a, b, c]]
}

/// Two closed tetrahedra glued along ONE edge (0,1): every half-edge is
/// paired, the edge is used 2 + 2 times, and the shell's χ = 6 − 11 + 8 = 3
/// — the Stage-4 gate's second condition (measured P0029 with the rim ×
/// plane arm off). The site names the faces on the over-covered edge.
#[test]
fn an_impossible_euler_shell_names_the_faces_on_its_over_covered_edges() {
    failures::reset();
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let mesh = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
            p(0.0, -1.0, 0.0),
            p(0.0, 0.0, -1.0),
        ],
        tet_tris(0, 1, 2, 3)
            .into_iter()
            .chain(tet_tris(0, 1, 4, 5))
            .collect(),
    );
    let at = attr(&[
        Some((InputId::A, 1)),
        Some((InputId::A, 1)),
        Some((InputId::A, 2)),
        Some((InputId::A, 2)),
        Some((InputId::B, 4)),
        Some((InputId::B, 5)),
        Some((InputId::B, 6)),
        Some((InputId::B, 6)),
    ]);
    assert!(
        crate::stage4_relocate::check_watertight_2manifold(&mesh).is_err(),
        "the gate must refuse the χ = 3 shell"
    );
    let sites = failures::euler_sites(&mesh, &at);
    assert_eq!(sites.len(), 1, "{sites:?}");
    assert_eq!(sites[0].trigger, FailureTrigger::NonManifoldMesh);
    // The four triangles on edge (0,1): tet 1's [0,2,1] (A:1) and [0,1,3]
    // (A:1); tet 2's [0,4,1] (B:4) and [0,1,5] (B:5).
    assert_eq!(
        sites[0].faces,
        vec![(InputId::A, 1), (InputId::B, 4), (InputId::B, 5)]
    );
    // The recorder falls back to them when no edge is unpaired.
    record_nonmanifold_mesh(&mesh, &at);
    assert_eq!(failures::recorded(), sites);
    failures::reset();
}

/// A valid closed shell is not an Euler site, even when a seam is used 2 + 2
/// times elsewhere (the tangency-seam doubling is legal on a χ = 2 shell).
#[test]
fn a_valid_shell_is_no_euler_site() {
    let mesh = Mesh::new(
        vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(0.0, 0.0, 1.0),
        ],
        tet_tris(0, 1, 2, 3),
    );
    let at = attr(&[Some((InputId::A, 0)); 4]);
    assert!(failures::euler_sites(&mesh, &at).is_empty());
}

/// T5 with faces: a `NonManifoldOutput` raise inside a scope names the
/// scope's face; the run's failure set reads it only when the run ENDS in
/// that error; a later raise outside any scope replaces it with no face.
#[test]
fn a_nonmanifold_raise_names_its_scope_face_only_when_the_run_ends_there() {
    failures::reset();
    let err: Result<crate::BRep, crate::YangError> = Err(crate::YangError::NonManifoldOutput);
    {
        let _s = failures::nonmanifold_scope(InputId::B, 9);
        let _ = crate::stage4_relocate::non_manifold_at("test-site", format_args!("v1"));
    }
    let f = failures::failures(&err);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].trigger, FailureTrigger::ReassemblyStop);
    assert_eq!(f[0].faces, vec![(InputId::B, 9)]);
    assert!(f[0].detail.contains("test-site"), "{}", f[0].detail);

    // A recovered raise leaves nothing on a body the run emits.
    let ok: Result<crate::BRep, crate::YangError> =
        Ok(rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 2.0));
    assert!(failures::failures(&ok)
        .iter()
        .all(|s| s.trigger != FailureTrigger::ReassemblyStop));

    // Scopes nest and restore; a raise outside every scope names nothing.
    {
        let _outer = failures::nonmanifold_scope(InputId::A, 1);
        {
            let _inner = failures::nonmanifold_scope(InputId::A, 2);
        }
        let _ = crate::stage4_relocate::non_manifold_at("outer", format_args!(""));
    }
    assert_eq!(failures::failures(&err)[0].faces, vec![(InputId::A, 1)]);
    let _ = crate::stage4_relocate::non_manifold_at("bare", format_args!(""));
    assert!(failures::failures(&err)[0].faces.is_empty());
    failures::reset();
}
