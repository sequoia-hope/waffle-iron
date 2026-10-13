//! Body decomposition: `split_solid_into_bodies` splits a disjoint union into
//! one body per OUTWARD shell (a lump), each void filed with the lump that
//! encloses it; overlapping unions stay one.
use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::*;

fn make_box(arena: &mut BrepArena, lo: [f64; 3], hi: [f64; 3]) -> SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, lo[2]),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(lo[0], lo[1]),
            Point2::new(hi[0], lo[1]),
            Point2::new(hi[0], hi[1]),
            Point2::new(lo[0], hi[1]),
        ],
        vec![],
    )
    .unwrap();
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), hi[2] - lo[2])
        .unwrap()
        .solid
}

#[test]
fn split_disjoint_union_into_two_bodies() {
    let mut arena = BrepArena::new();
    let a = make_box(&mut arena, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = make_box(&mut arena, [5.0, 5.0, 0.0], [6.0, 6.0, 1.0]);
    let s = boolean_op(&mut arena, a, b, BoolOp::Union).unwrap();
    let bodies = split_solid_into_bodies(&mut arena, s).unwrap();
    eprintln!("PROBE bodies={}", bodies.len());
    assert_eq!(bodies.len(), 2, "disjoint union should split into 2 bodies");
    // Each body is a single-shell solid with volume 1.0.
    for bid in &bodies {
        assert_eq!(arena.solid(*bid).unwrap().shells.len(), 1);
        let v = geom::signed_volume(&arena, *bid).unwrap();
        assert!((v - 1.0).abs() < 1e-9, "each body volume ~1.0, got {v}");
    }
}

#[test]
fn overlapping_union_stays_one_body() {
    let mut arena = BrepArena::new();
    let a = make_box(&mut arena, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = make_box(&mut arena, [1.0, 1.0, 1.0], [3.0, 3.0, 3.0]); // overlaps
    let s = boolean_op(&mut arena, a, b, BoolOp::Union).unwrap();
    let bodies = split_solid_into_bodies(&mut arena, s).unwrap();
    assert_eq!(bodies.len(), 1, "overlapping union is a single body");
}

/// A cylinder of radius 1 about z, z ∈ [0, 2]: its conservative box is
/// [−1, 1]² × [0, 2]; a block tucked into that box's corner, outside the
/// disc (its nearest corner (0.8, 0.8) is 1.13 from the axis), is a
/// disjoint lump whose box OVERLAPS the cylinder's.
fn cylinder_z(arena: &mut BrepArena) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        1.0,
    )
    .unwrap();
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), 2.0)
        .unwrap()
        .solid
}

/// Move every shell of `donor` into `host` (one multi-shell solid), the way
/// a disjoint union's output holds its lumps.
fn adopt_shells(arena: &mut BrepArena, host: SolidId, donor: SolidId) {
    let shells = arena.solid(donor).unwrap().shells.clone();
    for sh in &shells {
        arena.shell_mut(*sh).unwrap().solid = host;
    }
    arena.solid_mut(host).unwrap().shells.extend(shells);
    arena.solid_mut(donor).unwrap().shells.clear();
}

/// Two lumps whose conservative boxes overlap are still two bodies: a body
/// is an OUTWARD shell, and box overlap says nothing about containment. The
/// render-view parity golden pinned this on R0052 (a gear revolve beside an
/// oblique gear extrude, boxes overlapping, never meeting) when the
/// splitter briefly clustered by conservative-box overlap (2026-10-12,
/// Amendment 2 of `kv2_multishell_boolean_operands`): one body where there
/// are two. RED under box-overlap clustering.
#[test]
fn disjoint_lumps_with_overlapping_boxes_are_two_bodies() {
    let mut arena = BrepArena::new();
    let cyl = cylinder_z(&mut arena);
    let block = make_box(&mut arena, [0.8, 0.8, 0.5], [0.95, 0.95, 1.5]);
    adopt_shells(&mut arena, cyl, block);
    assert_eq!(arena.solid(cyl).unwrap().shells.len(), 2);
    let bodies = split_solid_into_bodies(&mut arena, cyl).unwrap();
    assert_eq!(bodies.len(), 2, "two outward shells are two bodies");
    for bid in &bodies {
        assert_eq!(arena.solid(*bid).unwrap().shells.len(), 1);
    }
}

/// A void files with the lump that ENCLOSES it, not with a neighbour whose
/// box it merely overlaps: the cylinder's cavity [−0.3, 0.9] × [−0.3, 0.3] ×
/// [0.6, 1.4] (far corner (0.9, 0.3) is 0.95 from the axis, inside r = 1)
/// reaches toward the box corner the block sits in, so its box overlaps the
/// block's box in x and z; it stays with the cylinder, whose box contains
/// it, and the block is a body of its own.
#[test]
fn a_void_files_with_its_enclosing_lump_not_an_overlapping_neighbour() {
    let mut arena = BrepArena::new();
    let cyl = cylinder_z(&mut arena);
    let tool = make_box(&mut arena, [-0.3, -0.3, 0.6], [0.9, 0.3, 1.4]);
    let voided = boolean_op(&mut arena, cyl, tool, BoolOp::Subtract).unwrap();
    assert_eq!(
        arena.solid(voided).unwrap().shells.len(),
        2,
        "fixture: boss + cavity"
    );
    let block = make_box(&mut arena, [0.8, 0.8, 0.5], [0.95, 0.95, 1.5]);
    adopt_shells(&mut arena, voided, block);
    let bodies = split_solid_into_bodies(&mut arena, voided).unwrap();
    assert_eq!(bodies.len(), 2, "cylinder-with-cavity and the block");
    let shell_counts: Vec<usize> = bodies
        .iter()
        .map(|b| arena.solid(*b).unwrap().shells.len())
        .collect();
    assert_eq!(
        shell_counts,
        vec![2, 1],
        "the cavity went with the cylinder"
    );
    let v = geom::signed_volume(&arena, bodies[0]).unwrap();
    let expect = std::f64::consts::PI * 2.0 - 1.2 * 0.6 * 0.8;
    assert!(
        (v - expect).abs() < 1e-9,
        "voided cylinder volume {v} vs {expect}"
    );
}
