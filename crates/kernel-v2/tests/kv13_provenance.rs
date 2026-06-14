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

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, BrepArena, EvoKind, ExtrudeResult, FaceId, OpTag, Pid, Profile,
    ProfileEdge, SolidId,
};

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

// =========================================================================
// F2 — operation journal + boolean attribution
// =========================================================================

/// Persistent face ids of a solid (from the arena), as a set.
fn solid_face_pids(arena: &BrepArena, solid: SolidId) -> Vec<Pid> {
    let mut pids = Vec::new();
    for &sh in &arena.solid(solid).expect("solid").shells {
        for &f in &arena.shell(sh).expect("shell").faces {
            pids.push(arena.face_pid(f).expect("output face has a Pid"));
        }
    }
    pids
}

fn shifted_square(dx: f64, dy: f64, side: f64) -> Profile {
    Profile::new(
        Point3::new(dx, dy, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(side, 0.0),
            Point2::new(side, side),
            Point2::new(0.0, side),
        ],
        vec![],
    )
    .expect("shifted square")
}

#[test]
fn union_journal_lineage_is_total_and_from_both_operands() {
    let mut arena = BrepArena::new();
    let a = extrude(
        &mut arena,
        &shifted_square(0.0, 0.0, 2.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0,
    )
    .expect("box A");
    let b = extrude(
        &mut arena,
        &shifted_square(1.0, 1.0, 2.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0,
    )
    .expect("box B");
    let a_pids: Vec<Pid> = all_faces(&a)
        .iter()
        .map(|&f| arena.face_pid(f).unwrap())
        .collect();
    let b_pids: Vec<Pid> = all_faces(&b)
        .iter()
        .map(|&f| arena.face_pid(f).unwrap())
        .collect();

    let out = boolean_op(&mut arena, a.solid, b.solid, BoolOp::Union).expect("union");

    // Exactly one boolean evolution was recorded.
    assert_eq!(arena.journal.len(), 1, "one boolean ⇒ one journal entry");
    let ev = &arena.journal[0];
    assert_eq!(ev.op, OpTag::Boolean(BoolOp::Union));

    // Every output face's Pid is accounted for — the OUTPUT of a `modified`
    // edge, or `generated`. No orphaned output face (lineage is total).
    let out_pids = solid_face_pids(&arena, out);
    let modified_outputs: std::collections::HashSet<Pid> =
        ev.modified.iter().map(|&(_, o, _)| o).collect();
    let generated: std::collections::HashSet<Pid> = ev.generated.iter().copied().collect();
    for p in &out_pids {
        assert!(
            modified_outputs.contains(p) || generated.contains(p),
            "output face Pid {p:?} has no lineage edge"
        );
    }

    // Every `modified` edge's INPUT is an operand face Pid (no invented source).
    let operand: std::collections::HashSet<Pid> =
        a_pids.iter().chain(b_pids.iter()).copied().collect();
    for &(input, _, _) in &ev.modified {
        assert!(
            operand.contains(&input),
            "modified-edge input {input:?} is not an operand face"
        );
    }

    // The union draws faces from BOTH operands (cross-operand attribution works).
    let from_a = ev.modified.iter().any(|&(i, _, _)| a_pids.contains(&i));
    let from_b = ev.modified.iter().any(|&(i, _, _)| b_pids.contains(&i));
    assert!(
        from_a && from_b,
        "union output should inherit from BOTH boxes"
    );
}

#[test]
fn subtract_journal_attributes_to_operands() {
    // Corner-cut: a big box minus a smaller box overlapping one corner. All
    // planar, so the result is reliable; the cut walls descend from the tool
    // (operand B) — the cross-operand attribution F2 establishes.
    let mut arena = BrepArena::new();
    let big = extrude(
        &mut arena,
        &shifted_square(0.0, 0.0, 4.0),
        Vector3::new(0.0, 0.0, 1.0),
        4.0,
    )
    .expect("big box");
    let tool = extrude(
        &mut arena,
        &shifted_square(3.0, 3.0, 2.0),
        Vector3::new(0.0, 0.0, 1.0),
        5.0,
    )
    .expect("tool box");
    let b_pids: Vec<Pid> = all_faces(&tool)
        .iter()
        .map(|&f| arena.face_pid(f).unwrap())
        .collect();

    let out = boolean_op(&mut arena, big.solid, tool.solid, BoolOp::Subtract).expect("subtract");
    assert_eq!(arena.journal.len(), 1);
    let ev = &arena.journal[0];
    assert_eq!(ev.op, OpTag::Boolean(BoolOp::Subtract));

    // At least one output face (a cut wall) descends from the tool operand.
    let from_tool = ev.modified.iter().any(|&(i, _, _)| b_pids.contains(&i));
    assert!(
        from_tool,
        "a cut wall must descend from the tool operand (B)"
    );

    // Lineage total: every output face has an edge.
    let out_pids = solid_face_pids(&arena, out);
    let outs: std::collections::HashSet<Pid> = ev
        .modified
        .iter()
        .map(|&(_, o, _)| o)
        .chain(ev.generated.iter().copied())
        .collect();
    for p in &out_pids {
        assert!(outs.contains(p), "output face Pid {p:?} unaccounted");
    }
}

#[test]
fn boolean_journal_is_deterministic() {
    // Same union twice ⇒ identical arenas, including the journal (the journal
    // is part of BrepArena's derived PartialEq, so non-deterministic lineage
    // would fail this).
    let build = || {
        let mut arena = BrepArena::new();
        let a = extrude(
            &mut arena,
            &shifted_square(0.0, 0.0, 2.0),
            Vector3::new(0.0, 0.0, 1.0),
            2.0,
        )
        .expect("A");
        let b = extrude(
            &mut arena,
            &shifted_square(1.0, 1.0, 2.0),
            Vector3::new(0.0, 0.0, 1.0),
            2.0,
        )
        .expect("B");
        let _ = boolean_op(&mut arena, a.solid, b.solid, BoolOp::Union).expect("union");
        arena
    };
    let a1 = build();
    let a2 = build();
    assert_eq!(
        a1.journal, a2.journal,
        "boolean journal must be deterministic"
    );
    assert!(!a1.journal.is_empty());
    assert!(matches!(a1.journal[0].op, OpTag::Boolean(BoolOp::Union)));
    // The lineage records at least the Same kind (Split appears only when an
    // operand face fragments into multiple output faces).
    assert!(a1.journal[0]
        .modified
        .iter()
        .any(|&(_, _, k)| k == EvoKind::Same));
}

// =========================================================================
// F3 — face lineage resolution (Pid level; feature-id binding is F5)
// =========================================================================

use kernel_v2::{descendants, face_lineage};

#[test]
fn union_face_lineage_resolves_to_an_operand_root() {
    // Every union output face descends ONE step (the union) from an operand
    // box face — and that operand face is a ROOT (a constructor face, no
    // incoming edge). I.e. the union-body face resolves to the original
    // extrude, NOT the boolean — proven here at the Pid level.
    let mut arena = BrepArena::new();
    let a = extrude(
        &mut arena,
        &shifted_square(0.0, 0.0, 2.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0,
    )
    .expect("A");
    let b = extrude(
        &mut arena,
        &shifted_square(1.0, 1.0, 2.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0,
    )
    .expect("B");
    let operands: HashSet<Pid> = all_faces(&a)
        .iter()
        .chain(all_faces(&b).iter())
        .map(|&f| arena.face_pid(f).unwrap())
        .collect();

    let out = boolean_op(&mut arena, a.solid, b.solid, BoolOp::Union).expect("union");
    for p in solid_face_pids(&arena, out) {
        let lin = face_lineage(&arena.journal, p);
        assert_eq!(
            lin.through,
            vec![OpTag::Boolean(BoolOp::Union)],
            "one union step"
        );
        assert!(
            operands.contains(&lin.root),
            "root {:?} is an original box face",
            lin.root
        );
        assert!(
            !operands.contains(&p),
            "the output face's OWN pid is fresh, not an operand's"
        );
    }
}

#[test]
fn chained_subtract_resolves_through_both_booleans() {
    // extrude→union→subtract. Each final face's root is a CONSTRUCTOR face
    // (∈ A∪B∪C); the most recent op is always the subtract; faces that came
    // from an original box (A/B) chain through BOTH booleans, tool (C) cut
    // walls through only the subtract.
    let mut arena = BrepArena::new();
    let a = extrude(
        &mut arena,
        &shifted_square(0.0, 0.0, 3.0),
        Vector3::new(0.0, 0.0, 1.0),
        3.0,
    )
    .expect("A");
    let b = extrude(
        &mut arena,
        &shifted_square(2.0, 2.0, 3.0),
        Vector3::new(0.0, 0.0, 1.0),
        3.0,
    )
    .expect("B");
    let c = extrude(
        &mut arena,
        &shifted_square(4.0, 4.0, 2.0),
        Vector3::new(0.0, 0.0, 1.0),
        4.0,
    )
    .expect("C tool");
    let ab: HashSet<Pid> = all_faces(&a)
        .iter()
        .chain(all_faces(&b).iter())
        .map(|&f| arena.face_pid(f).unwrap())
        .collect();
    let c_pids: HashSet<Pid> = all_faces(&c)
        .iter()
        .map(|&f| arena.face_pid(f).unwrap())
        .collect();

    let u = boolean_op(&mut arena, a.solid, b.solid, BoolOp::Union).expect("union");
    let s = boolean_op(&mut arena, u, c.solid, BoolOp::Subtract).expect("subtract");

    let (mut saw_two_deep, mut saw_one_deep) = (false, false);
    for p in solid_face_pids(&arena, s) {
        let lin = face_lineage(&arena.journal, p);
        assert_eq!(
            lin.through.first(),
            Some(&OpTag::Boolean(BoolOp::Subtract)),
            "most recent op is the subtract"
        );
        assert!(
            ab.contains(&lin.root) || c_pids.contains(&lin.root),
            "root {:?} is a constructor face",
            lin.root
        );
        if ab.contains(&lin.root) {
            assert_eq!(
                lin.through,
                vec![
                    OpTag::Boolean(BoolOp::Subtract),
                    OpTag::Boolean(BoolOp::Union)
                ],
                "an original-box face chains through union THEN subtract"
            );
            saw_two_deep = true;
        }
        if c_pids.contains(&lin.root) {
            assert_eq!(
                lin.through,
                vec![OpTag::Boolean(BoolOp::Subtract)],
                "tool wall: one step"
            );
            saw_one_deep = true;
        }
    }
    assert!(
        saw_two_deep,
        "some original-box face survives both booleans"
    );
    assert!(saw_one_deep, "some tool cut wall is present");
}

#[test]
fn inverse_descendants_finds_surviving_faces() {
    // The inverse: a given origin face's geometry can be traced FORWARD to the
    // current faces it produced.
    let mut arena = BrepArena::new();
    let a = extrude(
        &mut arena,
        &shifted_square(0.0, 0.0, 2.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0,
    )
    .expect("A");
    let b = extrude(
        &mut arena,
        &shifted_square(1.0, 1.0, 2.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0,
    )
    .expect("B");
    let a_pids: Vec<Pid> = all_faces(&a)
        .iter()
        .map(|&f| arena.face_pid(f).unwrap())
        .collect();

    let out = boolean_op(&mut arena, a.solid, b.solid, BoolOp::Union).expect("union");
    let current: HashSet<Pid> = solid_face_pids(&arena, out).into_iter().collect();

    // At least one of A's faces produced a live face in the union result.
    let any_survives = a_pids.iter().any(|&root| {
        descendants(&arena.journal, root)
            .iter()
            .any(|d| current.contains(d))
    });
    assert!(
        any_survives,
        "some original-A face survives into the union body"
    );
}
