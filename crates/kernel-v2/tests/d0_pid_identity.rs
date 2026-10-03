//! D0 identity oracle — `specs/drawings_and_mbd.md` §4 item 5, kernel layer.
//!
//! The three properties a persistent id must have for a drawing annotation
//! or an `UpTo` termination to stay attached to the geometry it names:
//!
//! 1. **Presence + uniqueness.** Every edge and vertex of a finished solid
//!    has a content-seeded `Pid`, and no two share one.
//! 2. **Stable across a rebuild.** Rebuilding the same document reproduces
//!    every id — tested here by building the same solid in two fresh arenas.
//! 3. **Stable across an unrelated edit elsewhere on the body.** Untouched
//!    geometry keeps its ids when a *different* part of the body changes,
//!    even though the boolean rebuilds every face (and therefore every face
//!    `Pid`) in the output. This is the property monotonic allocation cannot
//!    have and the reason edge ids are seeded from face *lineage roots*.
//!
//! Plus the refusal side: an id whose geometry the edit consumed is simply
//! ABSENT from the new map (the loud refusal on top of that absence is
//! `feature_engine::resolve`'s `Selector::Pid`), and a solid with no stamped
//! face pids is a loud `PidMissing` rather than an invented identity.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::pid::solid_pids;
use kernel_v2::{boolean_op, extrude, mvfs, BrepArena, KernelV2Error, Pid, Profile, SolidId};

/// An axis-aligned rectangle in the z = `z0` plane, as an extrudable profile.
fn rect(x0: f64, y0: f64, x1: f64, y1: f64, z0: f64) -> Profile {
    Profile::new(
        Point3::new(x0, y0, z0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(x1 - x0, 0.0),
            Point2::new(x1 - x0, y1 - y0),
            Point2::new(0.0, y1 - y0),
        ],
        vec![],
    )
    .expect("rectangle profile")
}

const EPS: f64 = 1e-9;

fn near(a: Point3, b: [f64; 3]) -> bool {
    let p = a.as_array();
    (p[0] - b[0]).abs() < EPS && (p[1] - b[1]).abs() < EPS && (p[2] - b[2]).abs() < EPS
}

/// The pid of the edge whose two endpoints are `p` and `q` (either order).
fn edge_pid_between(arena: &BrepArena, solid: SolidId, p: [f64; 3], q: [f64; 3]) -> Pid {
    let pids = solid_pids(arena, solid).expect("solid pids");
    let mut found: Option<Pid> = None;
    for (&h, &pid) in &pids.edges {
        let he = arena.half_edge(h).expect("half-edge");
        let a = arena.vertex(he.origin).expect("origin").point;
        let b = arena
            .vertex(arena.half_edge(he.next).expect("next").origin)
            .expect("destination")
            .point;
        if (near(a, p) && near(b, q)) || (near(a, q) && near(b, p)) {
            assert!(found.is_none(), "two edges between {p:?} and {q:?}");
            found = Some(pid);
        }
    }
    found.unwrap_or_else(|| panic!("no edge between {p:?} and {q:?}"))
}

/// The pid of the vertex at `p`.
fn vertex_pid_at(arena: &BrepArena, solid: SolidId, p: [f64; 3]) -> Pid {
    let pids = solid_pids(arena, solid).expect("solid pids");
    let mut found: Option<Pid> = None;
    for (&v, &pid) in &pids.vertices {
        if near(arena.vertex(v).expect("vertex").point, p) {
            assert!(found.is_none(), "two vertices at {p:?}");
            found = Some(pid);
        }
    }
    found.unwrap_or_else(|| panic!("no vertex at {p:?}"))
}

// =========================================================================
// 1 — presence and uniqueness
// =========================================================================

#[test]
fn box_edges_and_vertices_carry_unique_pids() {
    let mut arena = BrepArena::new();
    let r = extrude(
        &mut arena,
        &rect(0.0, 0.0, 1.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0,
    )
    .expect("box");
    let pids = solid_pids(&arena, r.solid).expect("solid pids");

    assert_eq!(pids.faces.len(), 6, "a box has 6 faces");
    assert_eq!(pids.edges.len(), 12, "a box has 12 edges");
    assert_eq!(pids.vertices.len(), 8, "a box has 8 vertices");

    let distinct_edges: std::collections::BTreeSet<Pid> = pids.edges.values().copied().collect();
    assert_eq!(distinct_edges.len(), 12, "every edge pid distinct");
    let distinct_verts: std::collections::BTreeSet<Pid> = pids.vertices.values().copied().collect();
    assert_eq!(distinct_verts.len(), 8, "every vertex pid distinct");
    assert!(
        pids.edges.values().all(|p| p.0 != 0) && pids.vertices.values().all(|p| p.0 != 0),
        "content-seeded pids are never 0"
    );
}

#[test]
fn cylinder_rims_and_seam_carry_pids() {
    // A circle-profile extrude: two rim circles + one seam edge, and the two
    // rims share NO face pair with each other (side × bottom cap vs
    // side × top cap), so all three ids come out of distinct groups.
    let mut arena = BrepArena::new();
    let circle = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        1.0,
    )
    .expect("circle");
    let r = extrude(&mut arena, &circle, Vector3::new(0.0, 0.0, 1.0), 2.0).expect("cylinder");
    let pids = solid_pids(&arena, r.solid).expect("solid pids");
    assert_eq!(pids.faces.len(), 3, "cylinder: side + 2 caps");
    let distinct: std::collections::BTreeSet<Pid> = pids.edges.values().copied().collect();
    assert_eq!(
        distinct.len(),
        pids.edges.len(),
        "cylinder edge pids all distinct ({} edges)",
        pids.edges.len()
    );
    assert!(!pids.vertices.is_empty(), "cylinder has seam vertices");
}

// =========================================================================
// 2 — stable across a rebuild
// =========================================================================

#[test]
fn rebuild_reproduces_every_edge_and_vertex_pid() {
    let build = || {
        let mut arena = BrepArena::new();
        let r = extrude(
            &mut arena,
            &rect(0.0, 0.0, 1.0, 1.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            2.0,
        )
        .expect("box");
        let pids = solid_pids(&arena, r.solid).expect("solid pids");
        (arena, r.solid, pids)
    };
    let (_a1, _s1, p1) = build();
    let (_a2, _s2, p2) = build();
    assert_eq!(p1, p2, "a rebuild of the same solid reproduces every pid");
}

#[test]
fn rebuild_after_a_boolean_reproduces_every_pid() {
    let build = || {
        let mut arena = BrepArena::new();
        let plate = extrude(
            &mut arena,
            &rect(0.0, 0.0, 4.0, 4.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            1.0,
        )
        .expect("plate");
        let boss = extrude(
            &mut arena,
            &rect(0.5, 0.5, 1.5, 1.5, 0.5),
            Vector3::new(0.0, 0.0, 1.0),
            1.5,
        )
        .expect("boss");
        let out = boolean_op(&mut arena, plate.solid, boss.solid, BoolOp::Union).expect("union");
        let pids = solid_pids(&arena, out).expect("solid pids");
        (arena, out, pids)
    };
    let (_a1, _s1, p1) = build();
    let (_a2, _s2, p2) = build();
    assert_eq!(
        p1.edges, p2.edges,
        "a rebuild of the same union reproduces every edge pid"
    );
    assert_eq!(
        p1.vertices, p2.vertices,
        "a rebuild of the same union reproduces every vertex pid"
    );
}

// =========================================================================
// 3 — stable across an unrelated edit elsewhere on the body
// =========================================================================

/// Plate + boss, the boss's height being the parameter the "edit" changes.
/// The boss straddles the plate's top face (z 0.5 → 0.5 + `boss_height`) so
/// no operand faces are coplanar.
fn plate_with_boss(boss_height: f64) -> (BrepArena, SolidId) {
    let mut arena = BrepArena::new();
    let plate = extrude(
        &mut arena,
        &rect(0.0, 0.0, 4.0, 4.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        1.0,
    )
    .expect("plate");
    let boss = extrude(
        &mut arena,
        &rect(0.5, 0.5, 1.5, 1.5, 0.5),
        Vector3::new(0.0, 0.0, 1.0),
        boss_height,
    )
    .expect("boss");
    let out = boolean_op(&mut arena, plate.solid, boss.solid, BoolOp::Union).expect("union");
    (arena, out)
}

#[test]
fn far_corner_edge_and_vertex_survive_an_edit_at_the_other_end() {
    let (a1, s1) = plate_with_boss(1.5);
    let (a2, s2) = plate_with_boss(2.25);

    // The plate's far vertical corner edge (x = 4, y = 4) and the vertex at
    // its top. Nothing the edit touched is anywhere near them.
    let e1 = edge_pid_between(&a1, s1, [4.0, 4.0, 0.0], [4.0, 4.0, 1.0]);
    let e2 = edge_pid_between(&a2, s2, [4.0, 4.0, 0.0], [4.0, 4.0, 1.0]);
    assert_eq!(
        e1, e2,
        "the far corner edge keeps its pid when the boss height changes"
    );

    let v1 = vertex_pid_at(&a1, s1, [4.0, 4.0, 1.0]);
    let v2 = vertex_pid_at(&a2, s2, [4.0, 4.0, 1.0]);
    assert_eq!(
        v1, v2,
        "the far corner vertex keeps its pid when the boss height changes"
    );
}

#[test]
fn the_boolean_rebuilt_the_faces_whose_pids_the_edge_ids_outlived() {
    // The premise of the test above: the union DOES give the output faces
    // fresh face pids, so an edge id seeded from current face pids would
    // have churned. Seeding from lineage roots is what makes it survive.
    let mut arena = BrepArena::new();
    let plate = extrude(
        &mut arena,
        &rect(0.0, 0.0, 4.0, 4.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        1.0,
    )
    .expect("plate");
    let before = solid_pids(&arena, plate.solid).expect("plate pids");
    let boss = extrude(
        &mut arena,
        &rect(0.5, 0.5, 1.5, 1.5, 0.5),
        Vector3::new(0.0, 0.0, 1.0),
        1.5,
    )
    .expect("boss");
    let out = boolean_op(&mut arena, plate.solid, boss.solid, BoolOp::Union).expect("union");
    let after = solid_pids(&arena, out).expect("union pids");

    let before_faces: std::collections::BTreeSet<Pid> = before.faces.values().copied().collect();
    let after_faces: std::collections::BTreeSet<Pid> = after.faces.values().copied().collect();
    assert!(
        after_faces.is_disjoint(&before_faces),
        "the union's output faces carry fresh face pids"
    );
    assert_eq!(
        edge_pid_between(&arena, plate.solid, [4.0, 4.0, 0.0], [4.0, 4.0, 1.0]),
        edge_pid_between(&arena, out, [4.0, 4.0, 0.0], [4.0, 4.0, 1.0]),
        "…yet the far corner edge's pid is unchanged by the union"
    );
}

// =========================================================================
// Refusal side — absence, and a solid with nothing to seed from
// =========================================================================

#[test]
fn an_edge_the_edit_consumed_is_absent_from_the_new_map() {
    let mut arena = BrepArena::new();
    let plate = extrude(
        &mut arena,
        &rect(0.0, 0.0, 4.0, 4.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        1.0,
    )
    .expect("plate");
    let boss = extrude(
        &mut arena,
        &rect(0.5, 0.5, 1.5, 1.5, 0.5),
        Vector3::new(0.0, 0.0, 1.0),
        1.5,
    )
    .expect("boss");
    // The boss's bottom-face corner edge ends up inside the material.
    let swallowed = edge_pid_between(&arena, boss.solid, [0.5, 0.5, 0.5], [1.5, 0.5, 0.5]);
    let out = boolean_op(&mut arena, plate.solid, boss.solid, BoolOp::Union).expect("union");
    let after = solid_pids(&arena, out).expect("union pids");
    assert!(
        !after.edges.values().any(|&p| p == swallowed),
        "an edge the union consumed must not reappear under its old pid"
    );
}

#[test]
fn a_solid_with_no_stamped_face_pids_is_a_loud_refusal() {
    // `mvfs` is a raw Euler operator: it never passes through a
    // constructor's `finalize_solid`, so its face has no `Pid` and there is
    // nothing to seed from. A refusal, not an invented id.
    let mut arena = BrepArena::new();
    let r = mvfs(&mut arena, Point3::new(0.0, 0.0, 0.0)).expect("mvfs");
    match solid_pids(&arena, r.solid) {
        Err(KernelV2Error::PidMissing { .. }) => {}
        other => panic!("expected PidMissing, got {other:?}"),
    }
}
