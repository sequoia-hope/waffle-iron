//! Yang §4.3 `:525-530` — T6, the missed-intersection trigger of the one
//! refinement loop (spec `specs/yang_452_refinement_loop.md` §3 "T6"; Phase
//! 1f of `specs/yang_faithful_cleanup.md`): "if the two meshes do not
//! intersect but are within the distance tolerance d_ε, there is a tangent
//! point, or a small loop, or no intersection between the surfaces". A
//! cross-operand contact pair — coarse triangles disjoint, exact surfaces
//! within the band, decided on surface pieces — that the arrangement joined
//! nowhere in the two triangles' one-rings is a missed intersection naming
//! both faces. (A metric capture at d_ε was measured wrong on C0058's true
//! crossings — `failures::missed_intersection_sites` doc.)

use crate::boolean::failures::{missed_intersection_sites, FailureTrigger};
use crate::brep::InputId;
use crate::shell_contact::contact_pairs;
use crate::tests_unit::rj_cylinder;
use crate::BRep;
use std::collections::BTreeSet;

fn contacts_of(a: &BRep, b: &BRep) -> Vec<crate::shell_contact::ContactPair> {
    contact_pairs(
        a.as_mesh(),
        a.tri_face(),
        a.faces(),
        b.as_mesh(),
        b.tri_face(),
        b.faces(),
        &|_, _| false,
    )
    .expect("both operands carry Stage-1 lineage")
}

/// Two unit cylinders, axes parallel (z), centres 2 − `overlap` apart, B
/// shifted up by 0.3 so no caps are coplanar: the two LATERAL surfaces
/// overlap by `overlap` (a graze the chord meshes — N ≈ 12 at this scale,
/// sagitta ≈ 0.034 — miss below the sagitta), while B's caps cut A's lateral
/// along real arcs the meshes do capture.
fn grazing_pair(overlap: f64) -> (BRep, BRep) {
    // B's axis along 15° from A's: half a 30° segment, so both chord
    // polygons present a chord MIDPOINT to the other (at a vertex the
    // polygons would cross and the meshes would see the overlap).
    let (c, s) = (15f64.to_radians().cos(), 15f64.to_radians().sin());
    let d = 2.0 - overlap;
    (
        rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 2.0),
        rj_cylinder([d * c, d * s, 0.3], [0.0, 0.0, 1.0], 1.0, 2.0),
    )
}

/// The site naming the two LATERALS (face 0 of each fixture cylinder).
fn lateral_site(sites: &[crate::boolean::failures::FailureSite]) -> bool {
    sites
        .iter()
        .any(|s| s.faces == vec![(InputId::A, 0), (InputId::B, 0)])
}

#[test]
fn a_sub_sagitta_graze_is_a_contact_pair_on_the_two_laterals() {
    let (a, b) = grazing_pair(0.005);
    let contacts = contacts_of(&a, &b);
    assert!(!contacts.is_empty(), "the exact surfaces overlap by 0.005");
    let pairs: Vec<(u32, u32)> = contacts
        .iter()
        .map(|c| {
            (
                a.tri_face()[c.tri_a as usize],
                b.tri_face()[c.tri_b as usize],
            )
        })
        .collect();
    let lateral_pair = contacts
        .iter()
        .find(|c| a.tri_face()[c.tri_a as usize] == 0 && b.tri_face()[c.tri_b as usize] == 0)
        .unwrap_or_else(|| panic!("a lateral × lateral contact; face pairs seen: {pairs:?}"));
    assert!(
        lateral_pair.reach > 0.0 && lateral_pair.reach < 0.05,
        "leaf reach {}",
        lateral_pair.reach
    );
    assert!(
        (lateral_pair.point[0] - 1.0).abs() < 0.05,
        "near x = 1: {:?}",
        lateral_pair.point
    );
}

#[test]
fn clearly_separated_cylinders_have_no_contact() {
    let (a, b) = (
        rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 2.0),
        rj_cylinder([2.5, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 2.0),
    );
    assert!(contacts_of(&a, &b).is_empty());
}

#[test]
fn a_contact_the_arrangement_joined_nowhere_is_a_missed_intersection() {
    let (a, b) = grazing_pair(0.005);
    let contacts = contacts_of(&a, &b);
    // An arrangement that joined NO triangle pair: every contact is missed,
    // one site per face pair.
    let sites = missed_intersection_sites(
        &contacts,
        a.as_mesh(),
        a.tri_face(),
        b.as_mesh(),
        b.tri_face(),
        &BTreeSet::new(),
    );
    assert!(lateral_site(&sites), "{sites:?}");
    assert!(sites
        .iter()
        .all(|s| s.trigger == FailureTrigger::MissedIntersection));
    // One site per distinct face pair.
    let pairs: BTreeSet<Vec<(InputId, u32)>> = sites.iter().map(|s| s.faces.clone()).collect();
    assert_eq!(pairs.len(), sites.len());
}

#[test]
fn a_contact_joined_within_its_one_rings_is_not_missed() {
    let (a, b) = grazing_pair(0.005);
    let contacts = contacts_of(&a, &b);
    // Joined exactly at the contact pairs: nothing missed.
    let exact: BTreeSet<(u32, u32)> = contacts.iter().map(|c| (c.tri_a, c.tri_b)).collect();
    assert!(missed_intersection_sites(
        &contacts,
        a.as_mesh(),
        a.tri_face(),
        b.as_mesh(),
        b.tri_face(),
        &exact
    )
    .is_empty());
    // Joined one triangle over on each side — a neighbour sharing a vertex
    // with the contact triangle, the chord offset's reach: still captured.
    let neighbour = |mesh: &crate::Mesh, t: u32| -> u32 {
        let tri = mesh.tris[t as usize];
        mesh.tris
            .iter()
            .position(|o| *o != tri && o.iter().any(|v| tri.contains(v)))
            .expect("a neighbour") as u32
    };
    let shifted: BTreeSet<(u32, u32)> = contacts
        .iter()
        .map(|c| {
            (
                neighbour(a.as_mesh(), c.tri_a),
                neighbour(b.as_mesh(), c.tri_b),
            )
        })
        .collect();
    assert!(missed_intersection_sites(
        &contacts,
        a.as_mesh(),
        a.tri_face(),
        b.as_mesh(),
        b.tri_face(),
        &shifted
    )
    .is_empty());
    // Joined only far away: the laterals are missed.
    let far: BTreeSet<(u32, u32)> = [(u32::MAX - 1, u32::MAX - 1)].into_iter().collect();
    assert!(lateral_site(&missed_intersection_sites(
        &contacts,
        a.as_mesh(),
        a.tri_face(),
        b.as_mesh(),
        b.tri_face(),
        &far
    )));
}

#[test]
fn crossing_cylinders_mesh_intersections_are_not_near_misses_of_each_other() {
    // Two unit cylinders crossing at right angles: the meshes intersect, so
    // the end-to-end run (below, through `boolean`) records no T6 site — the
    // record is empty after the natural run.
    use crate::boolean::failures;
    let a = rj_cylinder([0.0, 0.0, -2.0], [0.0, 0.0, 1.0], 1.0, 4.0);
    let b = rj_cylinder([-2.0, 0.0, 0.0], [1.0, 0.0, 0.0], 1.0, 4.0);
    let backend = crate::native_backend().expect("native backend");
    let out = crate::boolean(&a, &b, cad_primitives::BoolOp::Union, &backend);
    assert!(out.is_ok(), "{out:?}");
    assert!(
        !failures::recorded()
            .iter()
            .any(|s| s.trigger == FailureTrigger::MissedIntersection),
        "{:?}",
        failures::recorded()
    );
}

#[test]
fn joined_pairs_are_the_intersecting_coarse_triangles_and_the_census_skips_them() {
    // Two unit cylinders crossing at right angles: every A×B triangle pair
    // whose coarse triangles intersect is joined by the arrangement, and the
    // census lists none of them (they are the arrangement's own population).
    use crate::boolean::failures::joined_triangle_pairs;
    use crate::MeshBoolean;
    let a = rj_cylinder([0.0, 0.0, -2.0], [0.0, 0.0, 1.0], 1.0, 4.0);
    let b = rj_cylinder([-2.0, 0.0, 0.0], [1.0, 0.0, 0.0], 1.0, 4.0);
    let backend = crate::native_backend().expect("native backend");
    let la = backend
        .labeled_arrangement(a.as_mesh(), b.as_mesh())
        .expect("arrangement");
    let joined = joined_triangle_pairs(&la);
    assert!(!joined.is_empty());
    let ma = a.as_mesh();
    let mb = b.as_mesh();
    let mut intersecting = 0usize;
    for (ta, ia) in ma.tris.iter().enumerate() {
        for (tb, ib) in mb.tris.iter().enumerate() {
            let d = crate::shell_contact::triangle_distance(
                ia.map(|v| ma.verts[v as usize]),
                ib.map(|v| mb.verts[v as usize]),
            );
            if d == 0.0 {
                intersecting += 1;
                assert!(
                    joined.contains(&(ta as u32, tb as u32)),
                    "intersecting pair {ta}×{tb} not joined"
                );
            }
        }
    }
    assert!(intersecting > 0);
    let contacts = contacts_of(&a, &b);
    for c in &contacts {
        let d = crate::shell_contact::triangle_distance(
            ma.tris[c.tri_a as usize].map(|v| ma.verts[v as usize]),
            mb.tris[c.tri_b as usize].map(|v| mb.verts[v as usize]),
        );
        assert!(d > 0.0, "the census listed an intersecting pair {c:?}");
    }
}

#[test]
fn a_pair_the_caller_skips_is_never_refined_or_listed() {
    // The capture verdict runs BEFORE the exact crossing search (F0055: 21
    // pairs beside a real crossing refined for 112 s): a skipped pair is
    // never a contact.
    let (a, b) = grazing_pair(0.005);
    assert!(!contacts_of(&a, &b).is_empty());
    let skipped = contact_pairs(
        a.as_mesh(),
        a.tri_face(),
        a.faces(),
        b.as_mesh(),
        b.tri_face(),
        b.faces(),
        &|_, _| true,
    )
    .expect("lineage");
    assert!(skipped.is_empty());
}
