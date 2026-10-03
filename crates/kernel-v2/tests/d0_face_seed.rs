//! D0 item 1 (the F4a reseed) — a face's persistent id is seeded from the
//! CREATING STEP's identity and the face's role in it, not from the arena's
//! allocator.
//!
//! The defect this closes: a monotonic counter's value depends on the arena's
//! HISTORY, so the same step re-executed in an arena that has built other
//! things first stamps different numbers. That is exactly what an incremental
//! rebuild does, and what reopening a document does NOT do — which is how a
//! stored pid can come back pointing at a different face.
//!
//! Every assertion below is about that: same step ⇒ same ids, regardless of
//! what else the arena has done.

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{
    extrude, seeded_face_pid, BrepArena, ExtrudeResult, FaceSeed, Pid, Profile, SolidId,
    PID_CONTENT_BASE,
};

const SEED_A: FaceSeed = FaceSeed {
    origin: [0x1234_5678_9ABC_DEF0, 0x0FED_CBA9_8765_4321],
};
const SEED_B: FaceSeed = FaceSeed {
    origin: [0x1234_5678_9ABC_DEF0, 0x0FED_CBA9_8765_4320], // one bit from A
};

/// An `n`-gon in z = 0, unit circumradius, centred on `(cx, 0)`.
fn polygon(n: usize, cx: f64) -> Profile {
    let pts: Vec<Point2> = (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            Point2::new(cx + t.cos(), t.sin())
        })
        .collect();
    Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        pts,
        vec![],
    )
    .expect("polygon profile")
}

fn rect(w: f64, h: f64) -> Profile {
    Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(w, 0.0),
            Point2::new(w, h),
            Point2::new(0.0, h),
        ],
        vec![],
    )
    .expect("rectangle profile")
}

/// Extrude `profile` by `depth` under `seed`, returning the arena, the
/// result, and the solid's face pids in ascending `FaceId` order.
fn extrude_seeded(
    arena: &mut BrepArena,
    profile: &Profile,
    depth: f64,
    seed: Option<FaceSeed>,
) -> (ExtrudeResult, Vec<Pid>) {
    let prev = arena.set_face_seed(seed);
    let r = extrude(arena, profile, Vector3::new(0.0, 0.0, 1.0), depth).expect("extrude");
    arena.restore_face_seed(prev);
    let pids = face_pids(arena, r.solid);
    (r, pids)
}

fn face_pids(arena: &BrepArena, solid: SolidId) -> Vec<Pid> {
    let (faces, _) = kernel_v2::solid_face_pids(arena, solid).expect("face pids");
    faces.values().copied().collect()
}

// ---------------------------------------------------------------------------
// The defect
// ---------------------------------------------------------------------------

/// The claim, stated as plainly as it can be: the SAME step in an arena that
/// has already built three other solids stamps the SAME face pids as it does
/// in a fresh one. Under the monotonic counter these two lists could not
/// agree — the second arena's counter has advanced past everything the first
/// one handed out.
#[test]
fn the_same_step_in_a_used_arena_stamps_the_same_face_pids_as_in_a_fresh_one() {
    let fresh = {
        let mut arena = BrepArena::new();
        extrude_seeded(&mut arena, &rect(1.0, 1.0), 2.0, Some(SEED_A)).1
    };

    let used = {
        let mut arena = BrepArena::new();
        // Three unrelated steps first, each with its own identity.
        for (i, n) in [3usize, 5, 7].into_iter().enumerate() {
            let other = FaceSeed {
                origin: [0xAAAA_0000_0000_0000, i as u64],
            };
            extrude_seeded(&mut arena, &polygon(n, 10.0 * i as f64), 1.0, Some(other));
        }
        extrude_seeded(&mut arena, &rect(1.0, 1.0), 2.0, Some(SEED_A)).1
    };

    assert_eq!(
        fresh, used,
        "a step's face pids must not depend on what the arena built before it"
    );
    assert_eq!(fresh.len(), 6, "a rectangular box has six faces");
}

/// The same arena-history independence for the monotonic fallback, which is
/// where the defect lives: with no seed installed the two lists DIFFER. This
/// is the baseline the reseed exists to replace, pinned so the difference
/// between the two schemes is visible rather than asserted in prose.
#[test]
fn without_a_seed_the_same_step_in_a_used_arena_stamps_different_pids() {
    let fresh = {
        let mut arena = BrepArena::new();
        extrude_seeded(&mut arena, &rect(1.0, 1.0), 2.0, None).1
    };
    let used = {
        let mut arena = BrepArena::new();
        extrude_seeded(&mut arena, &polygon(3, 10.0), 1.0, None);
        extrude_seeded(&mut arena, &rect(1.0, 1.0), 2.0, None).1
    };
    assert_ne!(
        fresh, used,
        "the monotonic counter is history-dependent — that IS the defect"
    );
    assert_eq!(fresh, vec![Pid(0), Pid(1), Pid(2), Pid(3), Pid(4), Pid(5)]);
}

/// An edit to the step's PARAMETER (its depth) is not an edit to its
/// identity, so every face keeps its id. This is the "X's own unedited-role
/// faces keep theirs" half of the requirement, at the kernel layer.
#[test]
fn a_depth_edit_leaves_every_face_pid_of_the_step_unchanged() {
    let mut a = BrepArena::new();
    let before = extrude_seeded(&mut a, &rect(1.0, 1.0), 2.0, Some(SEED_A)).1;
    let mut b = BrepArena::new();
    let after = extrude_seeded(&mut b, &rect(1.0, 1.0), 7.5, Some(SEED_A)).1;
    assert_eq!(before, after, "depth is geometry, not identity");
}

// ---------------------------------------------------------------------------
// The role index
// ---------------------------------------------------------------------------

/// What the role index MEANS for a POLYGON extrude, pinned so a reader does
/// not have to infer it: the two caps, then one lateral per profile edge in
/// profile order. This is the spec's "cap top/bottom, lateral by profile-edge
/// index" (§4 item 1).
///
/// The caps come out top-then-base, in that order, because the Euler sequence
/// starts from `mvfs` (which makes the face the sweep rises FROM, leaving its
/// residual loop as the top) — see `construct::extrude`. Measured, not
/// assumed; the circle path below numbers its caps the other way round, which
/// is why neither order is stated as a general rule.
#[test]
fn the_extrude_role_index_is_the_two_caps_then_laterals_in_profile_order() {
    let mut arena = BrepArena::new();
    let (r, _) = extrude_seeded(&mut arena, &polygon(5, 0.0), 1.0, Some(SEED_A));
    let role = |n: u64| seeded_face_pid(SEED_A, 0, n);

    assert_eq!(arena.face_pid(r.top), Some(role(0)), "top cap is role 0");
    assert_eq!(arena.face_pid(r.base), Some(role(1)), "base cap is role 1");
    assert_eq!(r.walls.len(), 5, "a pentagon has five laterals");
    for (i, &w) in r.walls.iter().enumerate() {
        assert_eq!(
            arena.face_pid(w),
            Some(role(2 + i as u64)),
            "lateral {i} must be role {}",
            2 + i
        );
    }
}

/// A cylinder (circle profile — the direct assembler rather than the Euler
/// sequence) numbers base, top, then its single lateral. The order differs
/// from the polygon path's; what the two share, and all that is claimed, is
/// that each is a fixed function of its own constructor.
#[test]
fn a_cylinder_numbers_base_then_top_then_its_one_lateral() {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        1.0,
    )
    .expect("circle profile");
    let mut arena = BrepArena::new();
    let (r, pids) = extrude_seeded(&mut arena, &profile, 2.0, Some(SEED_A));
    assert_eq!(pids.len(), 3, "a cylinder has three faces");
    assert_eq!(arena.face_pid(r.base), Some(seeded_face_pid(SEED_A, 0, 0)));
    assert_eq!(arena.face_pid(r.top), Some(seeded_face_pid(SEED_A, 0, 1)));
    assert_eq!(
        arena.face_pid(r.walls[0]),
        Some(seeded_face_pid(SEED_A, 0, 2))
    );
}

// ---------------------------------------------------------------------------
// Separation: between steps, between a step's several solids, between halves
// ---------------------------------------------------------------------------

/// Two different steps building IDENTICAL geometry get disjoint id sets. The
/// seed is the only thing separating them, which is the point: identity comes
/// from who made the face, not from where it is.
#[test]
fn two_steps_building_identical_geometry_get_disjoint_ids() {
    let mut arena = BrepArena::new();
    let a = extrude_seeded(&mut arena, &rect(1.0, 1.0), 2.0, Some(SEED_A)).1;
    let b = extrude_seeded(&mut arena, &rect(1.0, 1.0), 2.0, Some(SEED_B)).1;
    let shared: Vec<&Pid> = a.iter().filter(|p| b.contains(p)).collect();
    assert!(
        shared.is_empty(),
        "one bit of seed difference must separate every id, shared: {shared:?}"
    );
}

/// Mutation check on the determinism pins above: perturbing ONE bit of the
/// seed changes every face pid. Without this, a pin that passes because the
/// seed is ignored would look exactly like a pin that passes because the
/// seed works.
#[test]
fn perturbing_one_bit_of_the_seed_changes_every_face_pid() {
    for role in 0..8u64 {
        for output in 0..3u64 {
            assert_ne!(
                seeded_face_pid(SEED_A, output, role),
                seeded_face_pid(SEED_B, output, role),
                "seed bit flip left role {role} / output {output} unchanged"
            );
        }
    }
}

/// One step can build several solids in ONE execution (a sketch with two
/// profiles extrudes twice under one feature id). Their faces share every
/// role index, so the seed alone cannot tell them apart — the stamping
/// pass's output ordinal does.
#[test]
fn two_solids_under_one_seed_are_separated_by_the_output_ordinal() {
    let mut arena = BrepArena::new();
    let prev = arena.set_face_seed(Some(SEED_A));
    let first = extrude(
        &mut arena,
        &rect(1.0, 1.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0,
    )
    .expect("first extrude");
    let second = extrude(
        &mut arena,
        &rect(3.0, 3.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0,
    )
    .expect("second extrude");
    arena.restore_face_seed(prev);

    let a = face_pids(&arena, first.solid);
    let b = face_pids(&arena, second.solid);
    assert_eq!(a.len(), 6);
    assert_eq!(b.len(), 6);
    for p in &a {
        assert!(!b.contains(p), "pid {p:?} is on both solids of one step");
    }
    // And the ordinals are the ones documented: pass 0 then pass 1.
    assert_eq!(a[0], seeded_face_pid(SEED_A, 0, 0));
    assert_eq!(b[0], seeded_face_pid(SEED_A, 1, 0));
}

/// The content half and the counter half of the number space never meet, by
/// construction rather than by luck: a content pid has the top bit set and
/// the counter is refused before it could reach there.
#[test]
fn content_ids_and_counter_ids_live_in_disjoint_halves() {
    let mut arena = BrepArena::new();
    let seeded = extrude_seeded(&mut arena, &rect(1.0, 1.0), 2.0, Some(SEED_A)).1;
    let counted = extrude_seeded(&mut arena, &rect(2.0, 2.0), 2.0, None).1;
    for p in &seeded {
        assert!(
            p.0 >= PID_CONTENT_BASE,
            "content pid {p:?} is below the base"
        );
    }
    for p in &counted {
        assert!(
            p.0 < PID_CONTENT_BASE,
            "counter pid {p:?} is at or above the base"
        );
    }
}

/// The counter refuses to cross into the content half rather than minting a
/// number a hash could also produce. Reaching the wall honestly needs 2^63
/// allocations, so the arena is placed at it directly — a branch no test can
/// enter is a branch nobody knows works.
#[test]
fn the_counter_refuses_to_cross_into_the_content_half() {
    let mut arena = BrepArena::new();
    arena.next_pid = PID_CONTENT_BASE;
    assert_eq!(
        arena.alloc_pid(),
        Err(kernel_v2::KernelV2Error::PidSpaceExhausted)
    );
    arena.next_pid = PID_CONTENT_BASE - 1;
    assert_eq!(arena.alloc_pid(), Ok(Pid(PID_CONTENT_BASE - 1)));
    assert_eq!(
        arena.alloc_pid(),
        Err(kernel_v2::KernelV2Error::PidSpaceExhausted),
        "the last legal id must not be followed by an illegal one"
    );
}

/// Two faces of ONE BODY may never share a pid — a stored reference would
/// resolve to whichever came first. Within a stamping pass that is already
/// impossible (a role is a position in a deduped list, and the digest is an
/// avalanche hash), so the refusal is staged: one face of the solid is made
/// to carry, in advance, the id another face's role would mint.
#[test]
fn two_faces_of_one_body_may_not_share_a_pid() {
    let mut arena = BrepArena::new();
    let r = extrude_seeded(&mut arena, &rect(1.0, 1.0), 2.0, None).0;
    // Leave the role-0 face (lowest FaceId) unstamped, and plant the pid it
    // would mint on a different face of the same solid.
    let role0 = r.top.min(r.base);
    let other = r.walls[0];
    arena.face_pids.remove(&role0);
    arena.face_pids.insert(other, seeded_face_pid(SEED_A, 0, 0));

    let prev = arena.set_face_seed(Some(SEED_A));
    let err = arena
        .assign_face_pids(r.solid)
        .expect_err("two faces of one body sharing a pid must be refused");
    arena.restore_face_seed(prev);
    assert_eq!(err, kernel_v2::KernelV2Error::PidCollision { kind: "face" });
}

/// A re-executed step takes the SAME ids its previous incarnation holds —
/// the orphaned solid left behind by a rebuild does not block them. This is
/// the property, not a leak: "the same step always names its faces the same
/// way" is exactly what a reopened document needs, and every consumer looks
/// a pid up inside one body.
#[test]
fn re_executing_a_step_in_the_same_arena_re_mints_its_ids() {
    let mut arena = BrepArena::new();
    let first = extrude_seeded(&mut arena, &rect(1.0, 1.0), 2.0, Some(SEED_A)).1;
    let second = extrude_seeded(&mut arena, &rect(1.0, 1.0), 9.0, Some(SEED_A)).1;
    assert_eq!(
        first, second,
        "a rebuild re-executes into the same arena; the step's names must not move"
    );
}

// ---------------------------------------------------------------------------
// The seed does not leak into a boolean
// ---------------------------------------------------------------------------

/// A boolean's output faces have no ROLE in the step — their name is their
/// lineage root plus a rank (D0 item 1b, `d0_boolean_face_seed.rs`) — so the
/// role-indexing pass is withdrawn for them. If it were not, a feature that
/// extrudes and then auto-unions would hand the union's faces role indices
/// under the same seed as the extrude's, and the two sets would compete for
/// the same ids.
///
/// Updated for item 1b: an output face is still CONTENT-seeded, just from
/// the other derivation. What this pins is that it is never
/// `seeded_face_pid(step seed, output, role)`, and that the boolean does not
/// consume one of the step's output ordinals.
#[test]
fn a_boolean_under_an_installed_seed_does_not_role_index_its_output() {
    use cad_primitives::BoolOp;

    let mut arena = BrepArena::new();
    let a = extrude_seeded(&mut arena, &rect(4.0, 4.0), 1.0, Some(SEED_A)).0;
    // A block straddling the plate's top face, so the union is a real one.
    let block = Profile::new(
        Point3::new(1.0, 1.0, 0.5),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
        ],
        vec![],
    )
    .expect("block profile");
    let b = extrude_seeded(&mut arena, &block, 2.0, Some(SEED_B)).0;

    // The union runs while A's seed is still installed — the arrangement a
    // combine-on-create feature produces.
    let prev = arena.set_face_seed(Some(SEED_A));
    let out = kernel_v2::boolean_op(&mut arena, a.solid, b.solid, BoolOp::Union)
        .expect("union of a plate and an overlapping block");
    let scope = arena.face_seed;
    arena.restore_face_seed(prev);

    assert_eq!(
        scope,
        Some(kernel_v2::FaceSeedScope {
            seed: SEED_A,
            next_output: 0
        }),
        "the boolean must not consume one of the step's output ordinals"
    );
    let outputs = face_pids(&arena, out);
    // No output face may carry a ROLE-indexed id under either operand's
    // seed. The role space is small and enumerable, so this is exhaustive
    // for any plausible output/role of a four-face-plus plate and block.
    let role_ids: std::collections::BTreeSet<Pid> = [SEED_A, SEED_B]
        .into_iter()
        .flat_map(|s| {
            (0..4).flat_map(move |output| {
                (0..32).map(move |role| kernel_v2::seeded_face_pid(s, output, role))
            })
        })
        .collect();
    for p in &outputs {
        assert!(
            !role_ids.contains(p),
            "boolean output face {p:?} took a role index under a step seed"
        );
    }
    // And every output still roots in one of the two seeded operands.
    let roots: std::collections::BTreeSet<Pid> = kernel_v2::solid_face_pids(&arena, out)
        .expect("pids")
        .1
        .values()
        .copied()
        .collect();
    assert!(
        roots.iter().all(|r| r.0 >= PID_CONTENT_BASE),
        "every output face must root in a content-seeded operand face: {roots:?}"
    );
}
