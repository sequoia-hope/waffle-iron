//! N47 coincident relocated-vertex weld (`weld_coincident_relocated`, spec
//! `specs/yang_n47_coincident_moved_weld.md`, deviation N47).
//!
//! The producer-side guard that collapses two vertices this pipeline RELOCATED
//! (`moved`) that converged to within the scale-relative model coincidence band
//! `TAU_MODEL·(1+scale)`. Reuses the `membrane` twin fixture (verts 2 and 3 are
//! 1e-7 apart — inside the band at unit scale).

#[allow(unused_imports)]
use super::*;
use std::collections::HashSet;

fn attribution_for(n: usize) -> Vec<Option<TriangleAttribution>> {
    (0..n)
        .map(|i| {
            Some(TriangleAttribution {
                input: InputId::A,
                face: i as u32,
            })
        })
        .collect()
}

/// GREEN behaviour: two RELOCATED coincident twins (1e-7 apart, inside the
/// 1.5e-7 unit-scale band) weld — the same collapse the `collapse_vertex`
/// membrane test drives, but reached via the coincidence scan. The pleat
/// annihilates; the bystander survives byte-identically.
#[test]
fn relocated_coincident_twin_welds() {
    let mut tris = pleat_tetra_tris();
    tris.extend(bystander_tetra_tris());
    let mut mesh = Mesh::new(membrane_fixture_verts(), tris);
    let mut attribution = attribution_for(mesh.tris.len());
    // Both twin vertices were relocated onto an analytic curve.
    let moved: HashSet<u32> = [2u32, 3u32].into_iter().collect();
    let welded = weld_coincident_relocated(&mut mesh, &mut attribution, &moved);
    assert!(welded, "the coincident relocated twin must weld");
    assert_eq!(
        mesh.tris,
        bystander_tetra_tris(),
        "pleat must annihilate; bystander byte-identical"
    );
    assert_eq!(
        mesh.tris.len(),
        attribution.len(),
        "attribution stays lockstep"
    );
}

/// SAFETY (I2): a genuinely SEPARATED relocated pair (0.1 apart — far above the
/// feature floor) is left untouched. No weld, mesh byte-identical.
#[test]
fn separated_relocated_pair_is_not_welded() {
    let mut verts = membrane_fixture_verts();
    verts[3] = Point3::new(0.5, 0.4, 0.2); // 0.1 from vert 2 — a real feature
    let mut tris = pleat_tetra_tris();
    tris.extend(bystander_tetra_tris());
    let before = tris.clone();
    let mut mesh = Mesh::new(verts, tris);
    let mut attribution = attribution_for(mesh.tris.len());
    let moved: HashSet<u32> = [2u32, 3u32].into_iter().collect();
    let welded = weld_coincident_relocated(&mut mesh, &mut attribution, &moved);
    assert!(!welded, "a separated pair must NOT weld");
    assert_eq!(mesh.tris, before, "mesh byte-identical when nothing welds");
}

/// RESTRICTION (I2, the R0091 landmine): a coincident twin that was NOT
/// relocated (`moved` empty) is left untouched — the pass never collapses
/// un-relocated arrangement geometry that `boolean()` kept for watertightness.
#[test]
fn coincident_but_unrelocated_twin_is_not_welded() {
    let mut tris = pleat_tetra_tris();
    tris.extend(bystander_tetra_tris());
    let before = tris.clone();
    let mut mesh = Mesh::new(membrane_fixture_verts(), tris);
    let mut attribution = attribution_for(mesh.tris.len());
    let moved: HashSet<u32> = HashSet::new(); // nothing was relocated
    let welded = weld_coincident_relocated(&mut mesh, &mut attribution, &moved);
    assert!(!welded, "un-relocated coincident geometry must NOT weld");
    assert_eq!(
        mesh.tris, before,
        "mesh byte-identical: no un-relocated collapse"
    );
}
