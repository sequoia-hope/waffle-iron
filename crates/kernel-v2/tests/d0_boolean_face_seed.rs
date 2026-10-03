//! D0 item 1b — a BOOLEAN's own output faces are named from content too, not
//! from the arena's allocator.
//!
//! Item 1 ([`d0_face_seed.rs`](./d0_face_seed.rs)) fixed construct faces and
//! left boolean outputs on the monotonic counter, so only each output face's
//! lineage ROOT was stable. A counter value is a function of the arena's
//! history, and an arena's history is a function of the editing session — so
//! two sessions that reach the same model by different routes stamped
//! different numbers on the same face, and a stored name resolved, by pid and
//! without a warning, to a different one.
//!
//! Every assertion below is about that one claim: the same boolean under the
//! same step seed names its output faces the same way, whatever else the
//! arena has built first.

use std::collections::BTreeMap;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, face_boundary_key, seeded_boolean_face_pid, BrepArena, FaceId, FaceSeed,
    Pid, Profile, SolidId, PID_CONTENT_BASE,
};

const SEED: FaceSeed = FaceSeed {
    origin: [0x1234_5678_9ABC_DEF0, 0x0FED_CBA9_8765_4321],
};
/// The seed of some other feature — one bit from `SEED`.
const OTHER_SEED: FaceSeed = FaceSeed {
    origin: [0x1234_5678_9ABC_DEF0, 0x0FED_CBA9_8765_4320],
};

/// An axis-aligned box: `[x0, x1] × [y0, y1]` extruded from `z0` to `z1`.
fn block(x: [f64; 2], y: [f64; 2], z: [f64; 2]) -> Profile {
    Profile::new(
        Point3::new(0.0, 0.0, z[0]),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x[0], y[0]),
            Point2::new(x[1], y[0]),
            Point2::new(x[1], y[1]),
            Point2::new(x[0], y[1]),
        ],
        vec![],
    )
    .expect("rectangular profile")
}

fn extrude_block(arena: &mut BrepArena, x: [f64; 2], y: [f64; 2], z: [f64; 2]) -> SolidId {
    extrude(
        arena,
        &block(x, y, z),
        Vector3::new(0.0, 0.0, 1.0),
        z[1] - z[0],
    )
    .expect("block")
    .solid
}

/// Every output face's `(pid, lineage root)`, keyed by `FaceId`.
fn pids(arena: &BrepArena, solid: SolidId) -> BTreeMap<FaceId, (Pid, Pid)> {
    let (faces, roots) = kernel_v2::solid_face_pids(arena, solid).expect("face pids");
    faces.iter().map(|(&f, &p)| (f, (p, roots[&f]))).collect()
}

/// A plate with a groove cut across it, built under `SEED`. The groove's
/// tool overhangs the plate in x and in +z, so no operand face pair is
/// coplanar — the cut is a plain two-solid subtract, and the plate's TOP
/// face splits into the two lands either side of the groove.
fn grooved_plate(arena: &mut BrepArena) -> SolidId {
    let plate = extrude_block(arena, [0.0, 10.0], [0.0, 10.0], [0.0, 10.0]);
    let tool = extrude_block(arena, [-1.0, 11.0], [4.0, 6.0], [5.0, 15.0]);
    boolean_op(arena, plate, tool, BoolOp::Subtract).expect("groove cut")
}

/// Requirement (1): with a step seed installed, every attributable output
/// face of a boolean carries a CONTENT id — not a counter number — and it is
/// exactly `H(seed, root, rank)`.
#[test]
fn a_boolean_output_face_pid_is_derived_from_its_root_and_the_op_seed() {
    let mut arena = BrepArena::new();
    let prev = arena.set_face_seed(Some(SEED));
    let out = grooved_plate(&mut arena);
    arena.restore_face_seed(prev);

    let map = pids(&arena, out);
    assert!(map.len() >= 10, "a grooved plate has at least ten faces");
    for (face, (pid, root)) in &map {
        assert!(
            pid.0 >= PID_CONTENT_BASE,
            "output face {face:?} kept a counter pid {pid:?}"
        );
        assert_ne!(*pid, *root, "an output face's pid is not its own root");
        // The rank is 0 or 1 here (only the plate's top cap splits), and
        // whichever it is, the id must be the derivation's own answer.
        assert!(
            (0..2).any(|rank| *pid == seeded_boolean_face_pid(SEED, *root, rank)),
            "output face {face:?} pid {pid:?} is not H(SEED, {root:?}, 0|1)"
        );
    }
}

/// Requirement (2) — the defect itself. The same boolean, under the same
/// step seed, in an arena whose HISTORY differs: the counter would have
/// handed out different numbers, content cannot.
#[test]
fn the_same_boolean_names_its_output_faces_the_same_in_a_busier_arena() {
    let mut clean = BrepArena::new();
    let prev = clean.set_face_seed(Some(SEED));
    let out_clean = grooved_plate(&mut clean);
    clean.restore_face_seed(prev);

    // The busy arena has already built — and unioned — other geometry under
    // another feature's seed, so its allocator and its slot numbering are
    // well past where the clean arena's were.
    let mut busy = BrepArena::new();
    let prev = busy.set_face_seed(Some(OTHER_SEED));
    let decoy_a = extrude_block(&mut busy, [40.0, 50.0], [40.0, 50.0], [0.0, 3.0]);
    let decoy_b = extrude_block(&mut busy, [45.0, 55.0], [42.0, 48.0], [1.0, 2.0]);
    boolean_op(&mut busy, decoy_a, decoy_b, BoolOp::Union).expect("decoy union");
    busy.restore_face_seed(prev);
    assert!(
        busy.next_pid > clean.next_pid,
        "the busy arena must really be further along: {} vs {}",
        busy.next_pid,
        clean.next_pid
    );
    let prev = busy.set_face_seed(Some(SEED));
    let out_busy = grooved_plate(&mut busy);
    busy.restore_face_seed(prev);

    let a: Vec<(Pid, Pid)> = pids(&clean, out_clean).into_values().collect();
    let b: Vec<(Pid, Pid)> = pids(&busy, out_busy).into_values().collect();
    assert_eq!(
        a, b,
        "the same cut in a busier arena renamed its output faces"
    );
}

/// Requirement (4): the split patches of ONE root get two distinct ids, and
/// the rank that separates them follows the patches' own content keys — the
/// lower key takes rank 0 — not their arena order.
#[test]
fn two_patches_of_one_split_face_rank_by_content_not_by_arena_order() {
    let mut arena = BrepArena::new();
    let prev = arena.set_face_seed(Some(SEED));
    let out = grooved_plate(&mut arena);
    arena.restore_face_seed(prev);

    let map = pids(&arena, out);
    let mut by_root: BTreeMap<Pid, Vec<FaceId>> = BTreeMap::new();
    for (&face, &(_, root)) in &map {
        by_root.entry(root).or_default().push(face);
    }
    let split: Vec<(&Pid, &Vec<FaceId>)> = by_root.iter().filter(|(_, fs)| fs.len() > 1).collect();
    assert_eq!(
        split.len(),
        1,
        "exactly one operand face (the plate's top cap) splits, got {:?}",
        split
            .iter()
            .map(|(r, f)| (**r, f.len()))
            .collect::<Vec<_>>()
    );
    let (root, faces) = split[0];
    assert_eq!(faces.len(), 2, "the top cap splits into two lands");

    let key = |f: FaceId| face_boundary_key(&arena, f).expect("content key");
    let (lo, hi) = if key(faces[0]) < key(faces[1]) {
        (faces[0], faces[1])
    } else {
        (faces[1], faces[0])
    };
    assert_ne!(key(lo), key(hi), "the two lands differ in content");
    assert_eq!(
        map[&lo].0,
        seeded_boolean_face_pid(SEED, *root, 0),
        "the land with the lower content key ranks 0"
    );
    assert_eq!(
        map[&hi].0,
        seeded_boolean_face_pid(SEED, *root, 1),
        "the land with the higher content key ranks 1"
    );
}

/// Requirement (3): a from-scratch rebuild in a FRESH PROCESS yields
/// byte-identical output face pids.
///
/// Two in-process arenas cannot see a dependence on anything per-process —
/// an address, a `HashMap` seed, a clock — because they share it. So this
/// re-executes the test binary and compares the child's ids to the
/// parent's. It is the cross-process half of the stability claim without
/// recorded literals, which at this layer would also pin which operand
/// faces the kernel happens to split and go red for reasons that are not a
/// format break. (The literal pin belongs to the hash alone, and lives in
/// `d0_pid_hash_frozen.rs`.)
const DUMP_ENV: &str = "D0_1B_DUMP_PIDS";
const DUMP_TEST: &str = "prints_the_grooved_plate_pids_for_its_own_child";

fn grooved_plate_pid_line() -> String {
    let mut arena = BrepArena::new();
    let prev = arena.set_face_seed(Some(SEED));
    let out = grooved_plate(&mut arena);
    arena.restore_face_seed(prev);
    let mut ids: Vec<u64> = pids(&arena, out).into_values().map(|(p, _)| p.0).collect();
    ids.sort_unstable();
    ids.iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// The child half of [`a_fresh_process_mints_the_same_output_face_pids`].
/// Inert unless the parent asks for it, so a plain run of this suite
/// asserts the same thing twice rather than nothing.
#[test]
fn prints_the_grooved_plate_pids_for_its_own_child() {
    let line = grooved_plate_pid_line();
    if std::env::var_os(DUMP_ENV).is_some() {
        println!("PIDS {line}");
    } else {
        assert!(!line.is_empty(), "the plate must have faces");
    }
}

#[test]
fn a_fresh_process_mints_the_same_output_face_pids() {
    let mine = grooved_plate_pid_line();
    let exe = std::env::current_exe().expect("this test binary's path");
    let out = std::process::Command::new(exe)
        .args(["--exact", DUMP_TEST, "--nocapture"])
        .env(DUMP_ENV, "1")
        .output()
        .expect("re-run this test binary as a child process");
    assert!(
        out.status.success(),
        "the child run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let theirs = stdout
        .lines()
        .find_map(|l| l.strip_prefix("PIDS "))
        .unwrap_or_else(|| panic!("the child printed no pid line, got:\n{stdout}"));
    assert_eq!(
        mine, theirs,
        "a fresh process minted different output face pids — the derivation \
         is reading something per-process"
    );
}

/// With no step seed installed there is no identity to seed from, and the
/// pre-item-1b counter behaviour stands — unchanged, so every raw-arena test
/// and every caller that never sets a seed is unaffected.
#[test]
fn without_a_seed_a_boolean_output_face_keeps_a_counter_pid() {
    let mut arena = BrepArena::new();
    let out = grooved_plate(&mut arena);
    for (face, (pid, _)) in pids(&arena, out) {
        assert!(
            pid.0 < PID_CONTENT_BASE,
            "unseeded output face {face:?} got a content pid {pid:?}"
        );
    }
}

/// Chained booleans under ONE step seed — a cut against several bodies, a
/// multi-tool combine, and the two-pocket plate of the measured hazard.
///
/// This is also the pin for the journal consequence item 1b forced. An
/// output face's id is independent of WHICH boolean of the chain produced
/// it, so the face at an unchanged site takes the same id in the
/// intermediate body and in the final one — correct, but it means the second
/// boolean's lineage edge would be `(P → P)`, and `face_lineage` walking a
/// self-loop reports `P` as its own root. Every root below must be a
/// CONSTRUCT-seeded id, which is what fails if that edge is recorded.
#[test]
fn a_second_boolean_in_the_same_feature_seeds_through_the_first() {
    let mut arena = BrepArena::new();
    let prev = arena.set_face_seed(Some(SEED));
    let plate = extrude_block(&mut arena, [0.0, 10.0], [0.0, 10.0], [0.0, 10.0]);
    let tool1 = extrude_block(&mut arena, [1.0, 3.0], [1.0, 3.0], [7.0, 15.0]);
    let cut1 = boolean_op(&mut arena, plate, tool1, BoolOp::Subtract).expect("pocket 1");
    let after_cut1 = pids(&arena, cut1);
    let tool2 = extrude_block(&mut arena, [6.0, 8.0], [6.0, 8.0], [7.0, 15.0]);
    let cut2 = boolean_op(&mut arena, cut1, tool2, BoolOp::Subtract).expect("pocket 2");
    arena.restore_face_seed(prev);

    // The carry-through: every id the first cut minted is still a live name
    // on the second cut's body. (With the self-edge recorded these pass too
    // — it is the roots below that catch it.)
    let final_pids: std::collections::BTreeSet<Pid> =
        pids(&arena, cut2).into_values().map(|(p, _)| p).collect();
    let carried = after_cut1
        .values()
        .filter(|(p, _)| final_pids.contains(p))
        .count();
    assert!(
        carried >= 5,
        "the sites the second cut did not touch should keep their names, \
         only {carried} of {} did",
        after_cut1.len()
    );

    for (face, (pid, root)) in pids(&arena, cut2) {
        assert!(
            pid.0 >= PID_CONTENT_BASE,
            "face {face:?} of the second cut kept a counter pid {pid:?}"
        );
        assert!(
            root.0 >= PID_CONTENT_BASE,
            "face {face:?} root {root:?} should be a construct-seeded id"
        );
        assert_eq!(
            pid,
            seeded_boolean_face_pid(SEED, root, 0),
            "face {face:?}: no face of a two-pocket plate shares a root, so \
             every rank is 0"
        );
    }
}
