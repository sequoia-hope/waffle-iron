//! PR-KV13 F1 — persistent entity tags (`Pid`) on faces.
//!
//! Every constructor stamps a fresh `Pid` on each of its output faces (the
//! `finalize_solid` exit). Pids are distinct from the array-index `FaceId`
//! handles (which churn on rebuild); they are the substrate for the
//! provenance/topological-naming work (F2 journal, F3 `FaceOrigin`, F4
//! rebuild-survival). F1 establishes only: **presence, uniqueness, and
//! determinism**.
//!
//! Oracle groups:
//! 1. Presence + uniqueness — every face of a finished solid carries a unique Pid.
//! 2. Determinism — identical construction sequences ⇒ identical arenas
//!    (Pids + allocator state included in the whole-arena comparison).
//! 3. Coverage — polygon / circle / arc / boolean constructors all stamp.

use std::collections::HashSet;

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, BrepArena, ExtrudeResult, FaceId, Pid, Profile, ProfileEdge};

fn unit_square() -> Profile {
    Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ],
        vec![],
    )
    .expect("unit square")
}

fn all_faces(r: &ExtrudeResult) -> Vec<FaceId> {
    let mut v = vec![r.base, r.top];
    v.extend(r.walls.iter().copied());
    v.extend(r.hole_walls.iter().flatten().copied());
    v
}

/// Every face of an extruded box carries a Pid; all distinct.
fn assert_pids_present_unique(arena: &BrepArena, faces: &[FaceId], what: &str) {
    let mut seen = HashSet::new();
    for &f in faces {
        let pid = arena
            .face_pid(f)
            .unwrap_or_else(|| panic!("{what}: face {f:?} has no Pid"));
        assert!(seen.insert(pid), "{what}: duplicate Pid {pid:?} on {f:?}");
    }
}

#[test]
fn box_faces_carry_unique_pids() {
    let mut arena = BrepArena::new();
    let r =
        extrude(&mut arena, &unit_square(), Vector3::new(0.0, 0.0, 1.0), 2.0).expect("box extrude");
    let faces = all_faces(&r);
    assert_eq!(faces.len(), 6, "box has 6 faces");
    assert_pids_present_unique(&arena, &faces, "box");
}

#[test]
fn circle_and_arc_faces_carry_pids() {
    // Circle profile → cylinder (3 faces).
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
    assert_pids_present_unique(&arena, &all_faces(&r), "cylinder");

    // Arc profile (quarter-disk sector) → cylinder-walled wedge (5 faces).
    let mut arena2 = BrepArena::new();
    let o = Point2::new(0.0, 0.0);
    let a = Point2::new(2.0, 0.0);
    let b = Point2::new(0.0, 2.0);
    let sector = Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            ProfileEdge::Line { a: o, b: a },
            ProfileEdge::Arc {
                a,
                b,
                center: o,
                radius: 2.0,
                ccw: true,
            },
            ProfileEdge::Line { a: b, b: o },
        ],
        vec![],
    )
    .expect("sector");
    let r2 = extrude(&mut arena2, &sector, Vector3::new(0.0, 0.0, 1.0), 3.0).expect("wedge");
    assert_pids_present_unique(&arena2, &all_faces(&r2), "arc wedge");
}

#[test]
fn pids_are_deterministic() {
    // Two identical construction sequences ⇒ bit-identical arenas. Because
    // `next_pid` and `face_pids` are part of `BrepArena`'s derived `PartialEq`,
    // this assertion FAILS if Pid assignment is ever non-deterministic.
    let build = || {
        let mut arena = BrepArena::new();
        let _ = extrude(&mut arena, &unit_square(), Vector3::new(0.0, 0.0, 1.0), 2.0)
            .expect("box extrude");
        arena
    };
    let a1 = build();
    let a2 = build();
    assert_eq!(
        a1, a2,
        "identical extrudes must produce identical arenas (incl. Pids)"
    );
    assert!(
        a1.next_pid >= 6,
        "at least 6 Pids allocated for a box, got {}",
        a1.next_pid
    );
}

#[test]
fn re_extrude_in_fresh_arena_reproduces_pids() {
    // The face Pids of the same box built into two fresh arenas match exactly
    // (monotonic allocation from 0 in FaceId order is reproducible). This is
    // the F1 precursor to F4a's content-keyed reseeding.
    let pids_of = || {
        let mut arena = BrepArena::new();
        let r = extrude(&mut arena, &unit_square(), Vector3::new(0.0, 0.0, 1.0), 2.0)
            .expect("box extrude");
        let mut pids: Vec<(FaceId, Pid)> = all_faces(&r)
            .into_iter()
            .map(|f| (f, arena.face_pid(f).expect("pid")))
            .collect();
        pids.sort_by_key(|(f, _)| f.0);
        pids
    };
    assert_eq!(pids_of(), pids_of(), "same box ⇒ same (FaceId, Pid) map");
}

#[test]
fn boolean_output_faces_carry_pids() {
    // Two overlapping boxes unioned: every face of the result is tagged
    // (F1 presence; per-face lineage attribution is F2).
    let mut arena = BrepArena::new();
    let a = extrude(&mut arena, &unit_square(), Vector3::new(0.0, 0.0, 1.0), 2.0).expect("box A");
    let shifted = Profile::new(
        Point3::new(0.5, 0.5, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ],
        vec![],
    )
    .expect("shifted square");
    let b = extrude(&mut arena, &shifted, Vector3::new(0.0, 0.0, 1.0), 2.0).expect("box B");

    let out =
        boolean_op(&mut arena, a.solid, b.solid, cad_primitives::BoolOp::Union).expect("union");

    // Enumerate the result solid's faces directly from the arena.
    let mut faces: Vec<FaceId> = Vec::new();
    for &sh in &arena.solid(out).expect("solid").shells {
        faces.extend(arena.shell(sh).expect("shell").faces.iter().copied());
    }
    assert!(!faces.is_empty(), "union produced faces");
    assert_pids_present_unique(&arena, &faces, "union");
}
