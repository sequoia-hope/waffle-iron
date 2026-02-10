//! TruckKernel scenario tests.
//!
//! These test against real truck geometry. Some operations are known
//! to fail or be unsupported — those tests are #[ignore]d.

use test_harness::ModelBuilder;

#[test]
fn test_truck_box_extrude() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    let (v, e, f) = m.topology_counts("box").unwrap();
    assert!(v > 0, "Truck box should have vertices");
    assert!(e > 0, "Truck box should have edges");
    assert!(f > 0, "Truck box should have faces");
}

#[test]
fn test_truck_revolve() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [5., 0., 0.], [0., 0., 1.], 5., 0., 5., 5.)
        .unwrap();
    m.revolve("rev", "sk", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("rev").unwrap();
}

#[test]
fn test_truck_tessellate_stl() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let mesh = m.tessellate("box").unwrap();
    assert!(!mesh.indices.is_empty(), "Truck mesh should have triangles");

    let stl = m.export_stl("box").unwrap();
    assert!(stl.len() > 84, "STL should have content");
}

#[test]
fn test_truck_boolean_offset() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    m.rect_sketch("sk2", [5., 5., 5.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    // Offset boxes (not coplanar) should work in truck
    m.boolean_union("merged", "box1", "box2").unwrap();
    m.assert_has_solid("merged").unwrap();
}

#[test]
#[ignore = "truck 0.4: coplanar boolean faces fail"]
fn test_truck_boolean_coplanar() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk1", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box1", "sk1", 10.0).unwrap();

    // Same Z plane = coplanar faces
    m.rect_sketch("sk2", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box2", "sk2", 10.0).unwrap();

    m.boolean_union("merged", "box1", "box2").unwrap();
    m.assert_has_solid("merged").unwrap();
}

#[test]
#[ignore = "TruckKernel fillet returns NotSupported"]
fn test_truck_fillet() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.fillet("fillet", "box", 1.0).unwrap();
    m.assert_has_solid("fillet").unwrap();
}

#[test]
#[ignore = "TruckKernel chamfer returns NotSupported"]
fn test_truck_chamfer() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    m.chamfer("cham", "box", 1.0).unwrap();
    m.assert_has_solid("cham").unwrap();
}
