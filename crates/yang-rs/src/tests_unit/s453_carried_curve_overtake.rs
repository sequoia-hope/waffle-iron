//! §4.5.3 CARRIED-CURVE OVERTAKE arm (spec `yang_453_line_overtake` §6;
//! `stage4_correct::reversal::carried_curve_overtake_site`). Pinned on the
//! P0036 shape (2026-10-12, `YANG_S6_CYCLE_DUMP` + `YANG_V_PROBE` +
//! `KV2_RING_PROVENANCE`): a revolve's start cap (a disc of radius 4e-4) is
//! cut by a cylinder of radius 1.25e-3 whose Stage-1 12-gon recedes from its
//! circle by a 4.26e-5 sagitta. The cap rim's 13-gon vertex v7 (−6.92° on the
//! rim) sits 3.4e-5 INSIDE the exact cutter and OUTSIDE its chord polygon,
//! so Stage 2 keeps it; the arrangement's rim × cutter corner v9 lands at
//! −5.86°, and Stage 4 relocates it in closed form onto the exact corner A
//! at −12.12° — PAST v7. The cap loop walks v5 (−34.6°) → v7 → A forward
//! and back along ONE circle, and kernel-v2 rejects the ring.
//!
//! The fixture is that run on a unit rim: a closed cylinder B-Rep (two
//! planar caps sharing their rim circles with the lateral) supplies the
//! carried edge, and the mesh carries the three loop vertices at P0036's
//! own azimuths.

#[allow(unused_imports)]
use super::*;
// `carried_curve_overtake_site` / `carried_edge_curve` reach here through
// the crate-root re-export (`use super::*`).
use std::collections::{BTreeMap, HashSet};

const R: f64 = 1.0;
const H: f64 = 2.0;

/// A closed cylinder: two full-circle rims (z = 0, z = H), the lateral face
/// and two planar caps, each cap sharing its rim edge with the lateral.
fn cylinder_with_caps() -> BRep {
    let v0 = BRepVertex {
        point: Point3::new(R, 0.0, 0.0),
    };
    let v1 = BRepVertex {
        point: Point3::new(R, 0.0, H),
    };
    let rim = |z: f64, start: u32| BRepEdge {
        start,
        end: start,
        curve: Curve::Circle {
            center: Point3::new(0.0, 0.0, z),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: R,
        },
    };
    let lateral = BRepFace {
        surface: lateral_surface(),
        outer_loop: vec![0, 1],
        inner_loops: vec![],
        reversed: false,
    };
    let bottom = BRepFace {
        surface: bottom_cap(),
        outer_loop: vec![0],
        inner_loops: vec![],
        reversed: false,
    };
    let top = BRepFace {
        surface: top_cap(),
        outer_loop: vec![1],
        inner_loops: vec![],
        reversed: false,
    };
    BRep::new(
        vec![v0, v1],
        vec![rim(0.0, 0), rim(H, 1)],
        vec![lateral, bottom, top],
    )
    .expect("closed cylinder B-Rep")
}

fn lateral_surface() -> Surface {
    Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: R,
    }
}

fn bottom_cap() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, 0.0, -1.0),
        d: 0.0,
    }
}

fn top_cap() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -H,
    }
}

fn on_rim(deg: f64, rho: f64) -> [f64; 3] {
    let t = deg.to_radians();
    [rho * t.cos(), rho * t.sin(), 0.0]
}

type Fixture = (
    Mesh,
    Vec<[f64; 3]>,
    BTreeMap<(u32, u32), Curve>,
    BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
);

/// Vertices on the bottom rim: 0 = v5 (−34.6°), 1 = v7 (−6.9°, the
/// overtaken rim chord vertex), 2 = J (entry: the chord corner at −5.9°,
/// a hair inside the disc; now the exact corner at −12.1°), 3 = a rim
/// vertex beyond J's entry (+20°).
fn fixture() -> Fixture {
    let entry = vec![
        on_rim(-34.6, R),
        on_rim(-6.9, R),
        on_rim(-5.9, R * (1.0 - 4e-6)),
        on_rim(20.0, R),
    ];
    let mut verts: Vec<Point3> = entry
        .iter()
        .map(|q| Point3::new(q[0], q[1], q[2]))
        .collect();
    let j_new = on_rim(-12.1, R);
    verts[2] = Point3::new(j_new[0], j_new[1], j_new[2]);
    let mesh = Mesh::new(verts, vec![]);
    let solid_edge = vec![(InputId::A, bottom_cap()), (InputId::A, lateral_surface())];
    let mut inc = BTreeMap::new();
    for &(s, e) in &[(0u32, 1u32), (1, 2), (1, 3)] {
        inc.insert((s.min(e), s.max(e)), solid_edge.clone());
    }
    (mesh, entry, BTreeMap::new(), inc)
}

#[test]
fn p0036_shaped_rim_chord_vertex_collapses_onto_the_relocated_corner() {
    let a = cylinder_with_caps();
    let b = cylinder_with_caps();
    let (mesh, entry, curves, inc) = fixture();
    let moved: HashSet<u32> = [2u32].into_iter().collect();
    // The cap walk v5 → v7 → A: p_r = v7 is overtaken.
    assert_eq!(
        carried_curve_overtake_site(&mesh, &a, &b, &curves, &inc, &moved, &entry, 0, 1, 2),
        Some((1, 2)),
        "the overtaken rim chord vertex collapses onto the exact corner"
    );
    // The lateral face walks the same run the other way.
    assert_eq!(
        carried_curve_overtake_site(&mesh, &a, &b, &curves, &inc, &moved, &entry, 2, 1, 0),
        Some((1, 2))
    );
}

#[test]
fn arm_fails_closed_outside_its_certificate() {
    let a = cylinder_with_caps();
    let b = cylinder_with_caps();
    let (mesh, entry, curves, inc) = fixture();
    let moved: HashSet<u32> = [2u32].into_iter().collect();
    let site = |mesh: &Mesh,
                curves: &BTreeMap<(u32, u32), Curve>,
                inc: &BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
                moved: &HashSet<u32>,
                entry: &[[f64; 3]],
                pb: u32,
                pr: u32,
                pn: u32| {
        carried_curve_overtake_site(mesh, &a, &b, curves, inc, moved, entry, pb, pr, pn)
    };
    // No moved neighbour: an unmoved rim U-turn is not this arm's.
    let none: HashSet<u32> = HashSet::new();
    assert_eq!(site(&mesh, &curves, &inc, &none, &entry, 0, 1, 2), None);
    // Both neighbours moved: ambiguous.
    let both: HashSet<u32> = [0u32, 2].into_iter().collect();
    assert_eq!(site(&mesh, &curves, &inc, &both, &entry, 0, 1, 2), None);
    // The site vertex itself moved: not an overtaken point.
    let self_moved: HashSet<u32> = [1u32, 2].into_iter().collect();
    assert_eq!(
        site(&mesh, &curves, &inc, &self_moved, &entry, 0, 1, 2),
        None
    );
    // A moved neighbour minted during Stage 4 (no entry position).
    assert_eq!(
        site(&mesh, &curves, &inc, &moved, &entry[..2], 0, 1, 2),
        None
    );
    // J moved the OTHER way (entry at −20°, now −12.1°): v7 at −6.9° is not
    // inside the displacement arc although the walk still U-turns there.
    let mut entry_back = entry.clone();
    entry_back[2] = on_rim(-20.0, R);
    assert_eq!(
        site(&mesh, &curves, &inc, &moved, &entry_back, 0, 1, 2),
        None
    );
    // A monotone walk: J moved from −5.9° to −3° (toward +20°, never past
    // v7) — the run v5 → v7 → J progresses in parameter order.
    let mut mesh_mono = mesh.clone();
    let j = on_rim(-3.0, R);
    mesh_mono.verts[2] = Point3::new(j[0], j[1], j[2]);
    assert_eq!(
        site(&mesh_mono, &curves, &inc, &moved, &entry, 0, 1, 2),
        None
    );
    // A TYPED edge at the site (an intersection run) belongs to the conic
    // arms, never to this one.
    let mut curves_typed = curves.clone();
    curves_typed.insert(
        (1, 2),
        Curve::Circle {
            center: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: R,
        },
    );
    assert_eq!(
        site(&mesh, &curves_typed, &inc, &moved, &entry, 0, 1, 2),
        None
    );
    // A mixed-input edge (an intersection edge the pair arm owns).
    let mut inc_mixed = inc.clone();
    inc_mixed.insert(
        (1, 2),
        vec![(InputId::A, bottom_cap()), (InputId::B, lateral_surface())],
    );
    assert_eq!(
        site(&mesh, &curves, &inc_mixed, &moved, &entry, 0, 1, 2),
        None
    );
    // A surface-pair change at p_r (a genuine corner of the operand).
    let mut inc_corner = inc.clone();
    inc_corner.insert(
        (1, 2),
        vec![(InputId::A, top_cap()), (InputId::A, lateral_surface())],
    );
    assert_eq!(
        site(&mesh, &curves, &inc_corner, &moved, &entry, 0, 1, 2),
        None
    );
    // A sub-band displacement.
    let mut entry_still = entry.clone();
    entry_still[2] = on_rim(-12.1, R);
    assert_eq!(
        site(&mesh, &curves, &inc, &moved, &entry_still, 0, 1, 2),
        None
    );
}

#[test]
fn carried_edge_curve_resolves_the_shared_rim_and_declines_elsewhere() {
    let brep = cylinder_with_caps();
    let band = cad_primitives::TAU_WORK * (1.0 + R);
    let rim_pt = Point3::new(R * 0.6f64.cos(), R * 0.6f64.sin(), 0.0);
    let expected = Curve::Circle {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: R,
    };
    assert_eq!(
        carried_edge_curve(&brep, bottom_cap(), lateral_surface(), rim_pt, band),
        Some(expected),
        "the cap and the lateral share the bottom rim through a rim point"
    );
    assert_eq!(
        carried_edge_curve(&brep, lateral_surface(), bottom_cap(), rim_pt, band),
        Some(expected),
        "surface order is immaterial"
    );
    // A point of the lateral off the rim lies on no shared edge.
    let mid = Point3::new(R, 0.0, 0.5 * H);
    assert_eq!(
        carried_edge_curve(&brep, bottom_cap(), lateral_surface(), mid, band),
        None
    );
    // The two caps share no edge at all.
    assert_eq!(
        carried_edge_curve(&brep, bottom_cap(), top_cap(), rim_pt, band),
        None
    );
}

/// The straight arm's geometry, reached by a carried `LineSegment` edge:
/// the R0059 ratios (J relocated along the x-axis over an unmoved run
/// vertex).
#[test]
fn segment_overtake_keeps_the_straight_arm_geometry() {
    let band = cad_primitives::TAU_WORK * (1.0 + 8.0);
    let old = [0.30, 0.0, 0.0];
    let new = [-1.53, 0.0, 0.0];
    let pr = [-0.15, 0.0, 0.0];
    assert!(segment_overtake(old, new, pr, new, [-1.37, 0.0, 0.0], band));
    // Beyond J's new position: not inside the displacement.
    assert!(!segment_overtake(
        old,
        new,
        [-8.0, 0.0, 0.0],
        new,
        [-3.0, 0.0, 0.0],
        band
    ));
    // A genuine corner (the next point off the line).
    assert!(!segment_overtake(old, new, pr, new, [0.9, 0.4, 0.4], band));
}
