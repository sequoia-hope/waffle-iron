//! Yang §4.5.2 per-FACE refinement — Phase 1a of
//! `specs/yang_faithful_cleanup.md` (spec `specs/yang_452_refinement_loop.md`
//! §5). The primitive the one refinement loop will drive: `BRep::refined_at`
//! doubles the named faces' density, a shared rim takes the finer face's
//! ring, the band between unequal rims is the azimuth zipper's, and an empty
//! request is the byte-identical identity.

use crate::stage1_tessellate::FACE_REFINE_MAX;
use crate::stage4_relocate::check_watertight_2manifold;
use crate::tests_unit::rj_cylinder;
use crate::BRep;
use std::collections::BTreeSet;

/// The N2 fixture cylinder: face 0 = lateral, face 1 = bottom cap, face 2 =
/// top cap; edge 0 = bottom rim, edge 1 = top rim, edge 2 = the seam ruling.
fn cyl() -> BRep {
    rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 2.0)
}

fn face_tris(b: &BRep, f: u32) -> usize {
    b.tri_face.iter().filter(|&&x| x == f).count()
}

fn vert_bits(b: &BRep) -> Vec<[u64; 3]> {
    b.mesh
        .verts
        .iter()
        .map(|p| {
            let a = p.as_array();
            [a[0].to_bits(), a[1].to_bits(), a[2].to_bits()]
        })
        .collect()
}

#[test]
fn empty_refinement_is_the_byte_identical_identity() {
    let a = cyl();
    let b = a.refined_at(&BTreeSet::new()).expect("identity rebuild");
    assert!(b.face_refine().is_empty());
    assert_eq!(vert_bits(&a), vert_bits(&b));
    assert_eq!(a.mesh.tris, b.mesh.tris);
}

#[test]
fn refining_the_lateral_doubles_both_rims_and_both_caps_follow() {
    let a = cyl();
    let n = face_tris(&a, 1); // the cap fan has N triangles for a rim of N samples
    assert!(n >= 3);
    assert_eq!(
        face_tris(&a, 0),
        2 * n,
        "a tube of two N-rings is 2N triangles"
    );
    let b = a
        .refined_at(&[0u32].into_iter().collect())
        .expect("refined rebuild");
    assert_eq!(b.face_refine().get(&0), Some(&2));
    assert_eq!(
        face_tris(&b, 0),
        4 * n,
        "both rims at 2N ⇒ a tube of 4N triangles"
    );
    assert_eq!(
        face_tris(&b, 1),
        2 * n,
        "the bottom cap shares the refined rim"
    );
    assert_eq!(
        face_tris(&b, 2),
        2 * n,
        "the top cap shares the refined rim"
    );
    check_watertight_2manifold(&b.mesh).expect("refined mesh is a closed 2-manifold");
}

#[test]
fn refining_one_cap_refines_its_rim_alone_and_the_tube_zips_unequal_rims() {
    let a = cyl();
    let n = face_tris(&a, 1);
    let b = a
        .refined_at(&[1u32].into_iter().collect())
        .expect("refined rebuild");
    assert_eq!(face_tris(&b, 1), 2 * n, "the refined cap's rim is at 2N");
    assert_eq!(face_tris(&b, 2), n, "the other cap's rim is untouched");
    assert_eq!(
        face_tris(&b, 0),
        n + 2 * n,
        "the tube between an N-ring and a 2N-ring is the zipper's m + n triangles"
    );
    check_watertight_2manifold(&b.mesh).expect("refined mesh is a closed 2-manifold");
}

#[test]
fn refinement_composes_by_doubling_and_saturates_at_the_ceiling() {
    let a = cyl();
    let n = face_tris(&a, 1);
    let lateral: BTreeSet<u32> = [0u32].into_iter().collect();
    let b = a.refined_at(&lateral).expect("×2");
    let c = b.refined_at(&lateral).expect("×4");
    assert_eq!(c.face_refine().get(&0), Some(&4));
    assert_eq!(face_tris(&c, 0), 8 * n);
    let mut d = c;
    for _ in 0..10 {
        d = d.refined_at(&lateral).expect("towards the ceiling");
    }
    assert_eq!(d.face_refine().get(&0), Some(&FACE_REFINE_MAX));
    assert_eq!(face_tris(&d, 0), 2 * n * FACE_REFINE_MAX as usize);
    check_watertight_2manifold(&d.mesh).expect("ceiling mesh is a closed 2-manifold");
}

#[test]
fn refinement_is_standing_across_a_from_topology_rebuild() {
    let a = cyl();
    let b = a
        .refined_at(&[2u32].into_iter().collect())
        .expect("refined rebuild");
    let c = b
        .retessellated_at_current_d_eps()
        .expect("rebuild from topology");
    assert_eq!(c.face_refine(), b.face_refine());
    assert_eq!(vert_bits(&b), vert_bits(&c));
    assert_eq!(b.mesh.tris, c.mesh.tris);
}

#[test]
fn refining_an_out_of_range_face_is_loud() {
    let a = cyl();
    assert!(a.refined_at(&[7u32].into_iter().collect()).is_err());
}
