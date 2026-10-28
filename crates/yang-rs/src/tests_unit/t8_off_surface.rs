// ====================================================================
// T8 (`boolean::failures::off_surface_vertex_sites`, ledger N100 item 9):
// every loop vertex of an EMITTED body lies on its face's surface within
// kernel-v2's import band, else the body is a failure site the §4.5.2 loop
// cannot adopt.
// ====================================================================

use super::plane_weld::box_brep;
use super::*;
use crate::boolean::failures::{off_surface_vertex_sites, FailureTrigger};

#[test]
fn t8_exact_box_has_no_off_surface_site() {
    let b = box_brep([0.0, 0.0, 0.0], [1.0, 2.0, 3.0]);
    assert!(off_surface_vertex_sites(&b).is_empty());
}

#[test]
fn t8_vertex_within_the_import_band_is_on_its_surface() {
    // 1e-10 off the z = 0 and x = 0 planes: inside 1e-9·(1 + 1).
    let mut b = box_brep([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    b.vertices[0].point = Point3::new(-1e-10, 0.0, -1e-10);
    assert!(off_surface_vertex_sites(&b).is_empty());
}

#[test]
fn t8_vertex_off_its_planes_is_a_site_on_every_face_it_bounds() {
    // Vertex 0 is a corner of three faces; pull it 1e-6 out of all three.
    let mut b = box_brep([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    b.vertices[0].point = Point3::new(-1e-6, -1e-6, -1e-6);
    let sites = off_surface_vertex_sites(&b);
    assert_eq!(sites.len(), 3, "{sites:?}");
    assert!(sites
        .iter()
        .all(|s| s.trigger == FailureTrigger::OffSurfaceVertex));
}
