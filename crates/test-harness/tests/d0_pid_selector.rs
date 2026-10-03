//! D0 end-to-end — `specs/drawings_and_mbd.md` §4: a `Selector::Pid`
//! recorded against the REAL kernel still names the same edge after an
//! unrelated edit elsewhere on the body, and a pid that is not on the body
//! is refused rather than rebound.
//!
//! The kernel-level identity oracle lives in
//! `crates/kernel-v2/tests/d0_pid_identity.rs`; this is the same claim one
//! layer up, through the feature tree, a rebuild, and
//! `feature_engine::resolve`.

use std::collections::HashMap;

use feature_engine::resolve::resolve_geom_ref_live;
use test_harness::ModelBuilder;
use waffle_types::kernel::{KernelId, KernelIntrospect};
use waffle_types::{Anchor, GeomRef, OutputKey, ResolvePolicy, Selector, TopoKind};

/// A plate with a boss standing on it, the boss's depth being the parameter
/// a later edit changes. The boss's sketch sits INSIDE the plate's top face
/// and below it, so the union has no coplanar operand faces.
fn plate_with_boss(boss_depth: f64) -> ModelBuilder {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("plate_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 40., 40.)
        .expect("plate sketch");
    m.extrude("plate", "plate_sk", 10.0).expect("plate");
    m.rect_sketch("boss_sk", [0., 0., 5.], [0., 0., 1.], 5., 5., 15., 15.)
        .expect("boss sketch");
    m.extrude("boss", "boss_sk", boss_depth).expect("boss");
    m
}

/// Midpoint of an edge, from the kernel's own polyline.
fn edge_mid(introspect: &dyn KernelIntrospect, edge: KernelId) -> [f64; 3] {
    let line = introspect.edge_polyline(edge);
    assert!(!line.is_empty(), "edge {edge:?} has no polyline");
    let n = line.len() as f64;
    let mut m = [0.0; 3];
    for p in &line {
        for k in 0..3 {
            m[k] += p[k] / n;
        }
    }
    m
}

/// The edge whose midpoint is nearest `target`, with its persistent id.
fn edge_nearest(m: &ModelBuilder, body: &str, target: [f64; 3]) -> (KernelId, u64, u64) {
    let handle = m.solid_handle(body).expect("body handle");
    let introspect = m.kernel_ref().as_introspect();
    let pids = introspect.all_entity_pids(&handle, TopoKind::Edge);
    assert!(!pids.is_empty(), "kernel reports edge identities");
    let mut best: Option<(KernelId, u64, u64, f64)> = None;
    for (id, pid) in pids {
        let c = edge_mid(introspect, id);
        let d = (0..3).map(|k| (c[k] - target[k]).powi(2)).sum::<f64>();
        if best.is_none_or(|(_, _, _, bd)| d < bd) {
            best = Some((id, pid.pid, pid.root_pid, d));
        }
    }
    let (id, pid, root, _) = best.expect("some edge");
    (id, pid, root)
}

fn pid_ref(m: &ModelBuilder, body: &str, pid: u64, root_pid: u64) -> GeomRef {
    GeomRef {
        kind: TopoKind::Edge,
        anchor: Anchor::FeatureOutput {
            feature_id: m.feature_id(body).expect("feature id"),
            output_key: OutputKey::Main,
        },
        selector: Selector::Pid { pid, root_pid },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

fn results_for(m: &ModelBuilder, body: &str) -> HashMap<uuid::Uuid, modeling_ops::OpResult> {
    let id = m.feature_id(body).expect("feature id");
    let mut map = HashMap::new();
    map.insert(id, m.op_result(body).expect("op result").clone());
    map
}

/// The far vertical corner edge of the plate — nowhere near the boss.
const FAR_CORNER: [f64; 3] = [40.0, 40.0, 5.0];

#[test]
fn a_recorded_pid_resolves_to_the_edge_it_was_read_from() {
    let m = plate_with_boss(15.0);
    let (id, pid, root) = edge_nearest(&m, "boss", FAR_CORNER);

    let resolved = resolve_geom_ref_live(
        &pid_ref(&m, "boss", pid, root),
        &results_for(&m, "boss"),
        m.kernel_ref().as_introspect(),
    )
    .expect("the pid just read must resolve");
    assert_eq!(
        resolved.kernel_id, id,
        "a pid read from the kernel resolves back to its own edge"
    );
    assert!(
        resolved.warnings.is_empty(),
        "an exact hit warns about nothing, got {:?}",
        resolved.warnings
    );
}

#[test]
fn the_reference_survives_an_unrelated_edit_elsewhere_on_the_body() {
    // Record the far corner edge's pid, then change the boss's depth — an
    // edit at the other end of the body that rebuilds every face of it.
    let mut m = plate_with_boss(15.0);
    let (_, pid, root) = edge_nearest(&m, "boss", FAR_CORNER);
    let before = edge_mid(
        m.kernel_ref().as_introspect(),
        edge_nearest(&m, "boss", FAR_CORNER).0,
    );

    m.edit_extrude_depth("boss", 22.0).expect("edit boss depth");

    let resolved = resolve_geom_ref_live(
        &pid_ref(&m, "boss", pid, root),
        &results_for(&m, "boss"),
        m.kernel_ref().as_introspect(),
    )
    .expect("the stored pid must still resolve after the edit");
    let after = edge_mid(m.kernel_ref().as_introspect(), resolved.kernel_id);
    for k in 0..3 {
        assert!(
            (before[k] - after[k]).abs() < 1e-9,
            "the pid resolved to a DIFFERENT edge after the edit: {before:?} vs {after:?}"
        );
    }
}

/// On the plate's outer boundary (`x ∈ {0, 40}` or `y ∈ {0, −40}` — the
/// harness maps sketch `v` to `−y`). Both adjacent faces of every such edge
/// are the plate's own, i.e. rooted in a feature the edit did not re-execute.
fn on_plate_boundary(mid: [f64; 3]) -> bool {
    let at = |a: f64, b: f64| (a - b).abs() < 1e-9;
    at(mid[0], 0.0) || at(mid[0], 40.0) || at(mid[1], 0.0) || at(mid[1], -40.0)
}

fn edge_pids_where(
    m: &ModelBuilder,
    body: &str,
    keep: impl Fn([f64; 3]) -> bool,
) -> Vec<(u64, [f64; 3])> {
    let handle = m.solid_handle(body).expect("handle");
    let introspect = m.kernel_ref().as_introspect();
    introspect
        .all_entity_pids(&handle, TopoKind::Edge)
        .into_iter()
        .map(|(id, p)| (p.pid, edge_mid(introspect, id)))
        .filter(|(_, mid)| keep(*mid))
        .collect()
}

#[test]
fn every_edge_of_the_untouched_region_keeps_its_id_not_just_the_one_we_picked() {
    // Stronger than the test above: the whole untouched-region id SET is
    // reproduced, not only the edge the test happened to name.
    let mut m = plate_with_boss(15.0);
    let before = edge_pids_where(&m, "boss", on_plate_boundary);
    assert_eq!(
        before.len(),
        12,
        "the plate's boundary contributes 12 edges, got {before:?}"
    );

    m.edit_extrude_depth("boss", 22.0).expect("edit boss depth");

    let after: std::collections::BTreeSet<u64> = edge_pids_where(&m, "boss", |_| true)
        .into_iter()
        .map(|(pid, _)| pid)
        .collect();
    for (pid, mid) in &before {
        assert!(
            after.contains(pid),
            "edge pid {pid} at {mid:?} vanished when the boss grew"
        );
    }
}

/// The other half of D0 item 1 — LIVE since the F4a reseed landed
/// (2026-10-03).
///
/// The four edges where the boss's walls meet the plate's top face do not
/// move when the boss gets taller, and they are no longer RENAMED. Measured
/// cause of the old failure: `edit_extrude_depth` rebuilds incrementally, so
/// the boss's extrude re-ran in an arena whose `next_pid` had already
/// advanced and its faces were stamped with fresh monotonic pids — roots
/// `{6,8,9,10,11}` became `{23,25,26,27,28}` while the plate's `{0..5}` were
/// untouched, and the edge ids seeded from those roots moved with them.
///
/// The fix was item 1 itself: a face's `Pid` is now derived from its
/// creating feature's uuid and its role in that feature
/// (`kernel_v2::seeded_face_pid`), so re-executing a feature whose identity
/// did not change reproduces its face pids whatever the arena has built in
/// between.
#[test]
fn edges_at_the_junction_with_an_edited_feature_keep_their_ids_too() {
    let mut m = plate_with_boss(15.0);
    let junction = |mid: [f64; 3]| (mid[2] - 10.0).abs() < 1e-9 && !on_plate_boundary(mid);
    let before = edge_pids_where(&m, "boss", junction);
    assert_eq!(before.len(), 4, "the boss footprint has 4 edges");

    m.edit_extrude_depth("boss", 22.0).expect("edit boss depth");

    let after: std::collections::BTreeSet<u64> = edge_pids_where(&m, "boss", |_| true)
        .into_iter()
        .map(|(pid, _)| pid)
        .collect();
    for (pid, mid) in &before {
        assert!(
            after.contains(pid),
            "junction edge pid {pid} at {mid:?} was renamed by an edit that did not move it"
        );
    }
}

#[test]
fn a_pid_that_is_not_on_the_body_is_refused_not_rebound() {
    let m = plate_with_boss(15.0);
    let (_, pid, _) = edge_nearest(&m, "boss", FAR_CORNER);
    let bogus = pid ^ 0xFFFF_FFFF;

    let err = resolve_geom_ref_live(
        &pid_ref(&m, "boss", bogus, bogus),
        &results_for(&m, "boss"),
        m.kernel_ref().as_introspect(),
    )
    .expect_err("a pid the body does not carry must be refused");
    let text = format!("{err:?}");
    assert!(
        text.contains("no longer exists"),
        "the refusal must name the finding, got {text}"
    );
}

// ---------------------------------------------------------------------------
// D0 item 1 — FACE pids, the silent-rebind half
// ---------------------------------------------------------------------------

/// Centroid of a face, averaged over its edges' polyline samples. Enough to
/// tell a body's top cap from its bottom cap, which is the distinction the
/// defect below erased.
fn face_centre(introspect: &dyn KernelIntrospect, face: KernelId) -> [f64; 3] {
    let mut sum = [0.0; 3];
    let mut n = 0.0;
    for e in introspect.face_edges(face) {
        for p in introspect.edge_polyline(e) {
            for k in 0..3 {
                sum[k] += p[k];
            }
            n += 1.0;
        }
    }
    assert!(n > 0.0, "face {face:?} has no boundary samples");
    [sum[0] / n, sum[1] / n, sum[2] / n]
}

/// Every FACE identity of the body a FEATURE UUID produced, mapped to where
/// that face is. Keyed by uuid rather than by the harness's own alias so the
/// same body can be found after a save/reopen, which rebuilds the alias map
/// from the loaded tree's auto-generated names. `root` selects the lineage
/// root instead of the face's own pid.
fn face_ids_at(
    m: &ModelBuilder,
    feature: uuid::Uuid,
    root: bool,
) -> std::collections::BTreeMap<u64, [f64; 3]> {
    let result = m
        .state
        .engine
        .get_result(feature)
        .unwrap_or_else(|| panic!("feature {feature} has no result"));
    assert!(!result.outputs.is_empty(), "feature {feature} has no body");
    let handle = result.outputs[0].1.handle.clone();
    let introspect = m.kernel_ref().as_introspect();
    let pids = introspect.all_entity_pids(&handle, TopoKind::Face);
    assert!(!pids.is_empty(), "kernel reports face identities");
    pids.into_iter()
        .map(|(id, p)| {
            (
                if root { p.root_pid } else { p.pid },
                face_centre(introspect, id),
            )
        })
        .collect()
}

/// A single-extrude plate, the shape N1's own pin uses.
fn plain_plate(depth: f64) -> ModelBuilder {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("plate_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 40., 40.)
        .expect("plate sketch");
    m.extrude("plate", "plate_sk", depth).expect("plate");
    m
}

/// The kernel half of the N1 defect, which is why N1 was held until this
/// landed: a document that is REOPENED replays its features from scratch,
/// with none of the editing history the authoring session had. Under the
/// monotonic counter the ids therefore depended on which session you were
/// in — a name authored on a plate's top cap came back resolving, by `pid`,
/// with no warning, to its BOTTOM cap.
///
/// Stated without any naming machinery: author, edit, save; reopen in a
/// FRESH engine and kernel; the pid → face-site map must be the one the
/// authoring session saw. (It is also requirement (1) of the reseed —
/// identical ids from a rebuild that shares no arena with the original — at
/// the document layer rather than the arena layer.)
///
/// The N1 branch's equivalent is the `#[ignore]`d
/// `a_face_name_keeps_its_pid_across_an_edit_to_its_own_feature` in
/// `crates/wasm-bridge/tests/tool_names.rs` — un-ignore it when N1 merges.
#[test]
fn a_face_pid_names_the_same_face_after_a_save_and_a_reopen() {
    let mut authored = plain_plate(10.0);
    authored.edit_extrude_depth("plate", 20.0).expect("edit");
    let plate = authored.feature_id("plate").expect("feature id");
    let before = face_ids_at(&authored, plate, false);
    assert_eq!(before.len(), 6, "a plate has six faces");
    let json = authored.save().expect("save");

    let mut reopened = ModelBuilder::kernel_v2();
    reopened.load(&json).expect("reopen");
    let after = face_ids_at(&reopened, plate, false);

    assert_eq!(
        before, after,
        "a reopened document must mint the same pid for the same face — \
         otherwise a stored pid silently rebinds to a different one"
    );
}

/// Same claim with a boolean in the tree. A boolean's OUTPUT faces stay on
/// the monotonic counter by design (their identity is their journal lineage,
/// not their own number), so the identity under test is each face's ROOT —
/// which is what a stored reference falls back to and what every edge pid is
/// seeded from.
#[test]
fn a_face_root_names_the_same_face_after_a_save_and_a_reopen_through_a_union() {
    let mut authored = plate_with_boss(15.0);
    authored.edit_extrude_depth("boss", 22.0).expect("edit");
    let boss = authored.feature_id("boss").expect("feature id");
    let before = face_ids_at(&authored, boss, true);
    let json = authored.save().expect("save");

    let mut reopened = ModelBuilder::kernel_v2();
    reopened.load(&json).expect("reopen");
    let after = face_ids_at(&reopened, boss, true);

    assert_eq!(
        before.len(),
        after.len(),
        "the two routes to one model must agree on the face count"
    );
    for (root, site) in &before {
        match after.get(root) {
            Some(other) => {
                let d = (0..3).map(|k| (site[k] - other[k]).powi(2)).sum::<f64>();
                assert!(
                    d < 1e-18,
                    "root {root} sits at {site:?} in the authoring session but \
                     {other:?} after a reopen — the same number names two faces"
                );
            }
            None => panic!("root {root} at {site:?} exists only before the reopen"),
        }
    }
}

/// The "leaves every face pid of every OTHER feature unchanged" half: the
/// plate is not re-executed by an edit to the boss, and even the faces the
/// union rebuilt keep their LINEAGE ROOTS, which is the identity a stored
/// reference and every edge id are seeded from.
#[test]
fn an_edit_to_one_feature_leaves_another_features_face_roots_alone() {
    let mut m = plate_with_boss(15.0);
    // The plate's own geometry: every face whose centre lies outside the
    // boss's 5..20 footprint in u and v.
    let plate_roots = |m: &ModelBuilder| -> std::collections::BTreeSet<u64> {
        let handle = m.solid_handle("boss").expect("handle");
        let introspect = m.kernel_ref().as_introspect();
        introspect
            .all_entity_pids(&handle, TopoKind::Face)
            .into_iter()
            .filter(|(id, _)| {
                let c = face_centre(introspect, *id);
                c[0] < 5.0 || c[0] > 20.0 || c[1] < 5.0 || c[1] > 20.0
            })
            .map(|(_, p)| p.root_pid)
            .collect()
    };
    let before = plate_roots(&m);
    assert!(
        before.len() >= 5,
        "the plate contributes at least five faces, got {before:?}"
    );

    m.edit_extrude_depth("boss", 22.0).expect("edit boss depth");

    let after = plate_roots(&m);
    for r in &before {
        assert!(
            after.contains(r),
            "the plate's face root {r} was renamed by an edit to the boss"
        );
    }
}

// ---------------------------------------------------------------------------
// D0 item 1b — a BOOLEAN output face's OWN pid
// ---------------------------------------------------------------------------

/// A 40×40×10 plate with two blind 10×10 pockets cut into its top face,
/// both well inside the footprint and well apart. The body is the SECOND
/// cut's output, so every face of it is a boolean output face — the shape of
/// the measured hazard.
///
/// `first_depth` is pocket 1's depth: 3 leaves it blind, 10 takes it through
/// the plate, which is the edit the hazard was measured under.
fn plate_with_two_pockets(first_depth: f64) -> ModelBuilder {
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("plate_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 40., 40.)
        .expect("plate sketch");
    m.extrude("plate", "plate_sk", 10.0).expect("plate");
    m.rect_sketch("p1_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 10., 10.)
        .expect("pocket 1 sketch");
    m.extrude_cut("p1", "p1_sk", first_depth).expect("pocket 1");
    m.rect_sketch("p2_sk", [0., 0., 10.], [0., 0., 1.], 25., 25., 10., 10.)
        .expect("pocket 2 sketch");
    m.extrude_cut("p2", "p2_sk", 4.0).expect("pocket 2");
    m
}

/// Pocket 2's floor: the face the hazard was measured on. `rect_sketch`
/// takes `(x, y, w, h)` and the sketch basis puts v along −y, so pocket 2
/// spans x 25..35, y −25..−35, and its floor sits 4 below the plate's top.
const P2_FLOOR: [f64; 3] = [30.0, -30.0, 6.0];

/// How far apart two face sites may be and still be the same site.
const SITE_EPS: f64 = 1e-9;

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < SITE_EPS)
}

/// The pid of the face whose centre is `site`, refusing if no face is there
/// — so a fixture change that moves the geometry fails loudly instead of
/// quietly testing a different face.
fn face_pid_at(m: &ModelBuilder, feature: uuid::Uuid, site: [f64; 3]) -> u64 {
    let found: Vec<u64> = face_ids_at(m, feature, false)
        .into_iter()
        .filter(|(_, c)| near(*c, site))
        .map(|(pid, _)| pid)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected exactly one face centred at {site:?}, found {found:?}"
    );
    found[0]
}

/// A face `Selector::Pid` reference, anchored by FEATURE UUID rather than by
/// the harness's alias — the alias map is rebuilt from the loaded tree's
/// auto-generated names, so a reopened document does not answer to it.
fn pid_face_ref(feature: uuid::Uuid, pid: u64, root_pid: u64) -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: feature,
            output_key: OutputKey::Main,
        },
        selector: Selector::Pid { pid, root_pid },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

/// [`results_for`] keyed by uuid, for the same reason.
fn results_for_uuid(
    m: &ModelBuilder,
    feature: uuid::Uuid,
) -> HashMap<uuid::Uuid, modeling_ops::OpResult> {
    let result = m
        .state
        .engine
        .get_result(feature)
        .unwrap_or_else(|| panic!("feature {feature} has no result"));
    let mut map = HashMap::new();
    map.insert(feature, result.clone());
    map
}

/// THE MEASURED HAZARD (D0 item 1b). One plate, two pockets, the body being
/// the second cut's output. Name the second pocket's FLOOR; deepen the FIRST
/// pocket into a through hole; save; reopen in a fresh engine and kernel.
///
/// Under the counter the floor's number was handed out by allocation order,
/// which depends on the editing history — so the reopened document, which
/// replays the features from scratch, re-minted that number onto the second
/// pocket's SIDE WALL, and the stored name resolved there by pid with no
/// warning. Content cannot do that: the floor's id is
/// `H(cut feature, the tool's bottom-cap root, rank)` and a wall's root is a
/// different tool face.
///
/// Measured on this exact fixture by withdrawing the item-1b reseed
/// (2026-10-03): the floor is pid **22** when authored; the edit alone moves
/// it to **47** (the counter has advanced), and after save + reopen pid 22
/// names the side wall at `[35, −30, 8]` while the floor is pid 20. Every
/// step of that is a silent rebind, and all of it is what this test fails
/// on if the reseed is withdrawn — the other eight tests in this file stay
/// green, so the pin is this change's and not item 1's.
#[test]
fn a_boolean_output_face_pid_names_the_same_face_after_an_upstream_edit_and_a_reopen() {
    let mut authored = plate_with_two_pockets(3.0);
    let p2 = authored.feature_id("p2").expect("feature id");
    let floor_pid = face_pid_at(&authored, p2, P2_FLOOR);
    // Item 1b: a boolean output face's pid is H(op seed, lineage root, rank),
    // so its root is the cutter's bottom-cap root, NOT its own pid — and N1's
    // resolver (merged the same day) cross-checks the recorded root before
    // it accepts a number. Record the root the way a real selector would.
    let floor_root = face_ids_at(&authored, p2, true)
        .into_iter()
        .find(|(_, c)| near(*c, P2_FLOOR))
        .map(|(root, _)| root)
        .expect("the floor has a lineage root");

    // The edit the hazard was measured under: pocket 1 becomes a through
    // hole, which rebuilds both cuts and every face of the body.
    authored.edit_extrude_depth("p1", 10.0).expect("deepen p1");
    assert_eq!(
        face_pid_at(&authored, p2, P2_FLOOR),
        floor_pid,
        "an edit to the OTHER pocket renamed pocket 2's floor"
    );
    let json = authored.save().expect("save");

    let mut reopened = ModelBuilder::kernel_v2();
    reopened.load(&json).expect("reopen");
    let after = face_ids_at(&reopened, p2, false);

    // 1. The floor still carries the same number, at the same site.
    let site = after
        .get(&floor_pid)
        .unwrap_or_else(|| panic!("pid {floor_pid} is gone after the reopen"));
    assert!(
        near(*site, P2_FLOOR),
        "pid {floor_pid} named pocket 2's floor at {P2_FLOOR:?} in the \
         authoring session and {site:?} after the reopen"
    );

    // 2. ...which is the same as saying no OTHER face took it. Stated
    //    separately because the side wall is the face it actually landed on.
    let walls: Vec<[f64; 3]> = after
        .iter()
        .filter(|(_, c)| (c[2] - 6.0).abs() > SITE_EPS && c[0] > 20.0 && c[1] < -20.0)
        .map(|(_, c)| *c)
        .collect();
    assert_eq!(
        walls.len(),
        4,
        "pocket 2 has four side walls, got {walls:?}"
    );
    for (pid, c) in &after {
        if !near(*c, P2_FLOOR) {
            assert_ne!(
                *pid, floor_pid,
                "the floor's pid landed on the face at {c:?}"
            );
        }
    }

    // 3. And it resolves, by pid, with no warning.
    let resolved = resolve_geom_ref_live(
        &pid_face_ref(p2, floor_pid, floor_root),
        &results_for_uuid(&reopened, p2),
        reopened.kernel_ref().as_introspect(),
    )
    .expect("the stored pid must resolve after a reopen");
    assert!(
        resolved.warnings.is_empty(),
        "an exact hit warns about nothing, got {:?}",
        resolved.warnings
    );
    assert!(
        near(
            face_centre(reopened.kernel_ref().as_introspect(), resolved.kernel_id),
            P2_FLOOR
        ),
        "the stored pid resolved to a face somewhere else"
    );
}

/// Pin (b): an edit to an UNRELATED earlier feature leaves every output face
/// pid of the later boolean alone — not just the one face a test picked.
///
/// "Unrelated" is geometric: pocket 1 is upstream of pocket 2's boolean, so
/// the edit re-runs it, but nothing it moves touches pocket 2 or the plate's
/// sides. Those sites' pids must be bit-identical across the edit. (Pocket
/// 1's own faces legitimately change: its floor disappears and its walls
/// lengthen.)
#[test]
fn an_upstream_edit_leaves_every_untouched_output_face_pid_of_a_later_boolean_alone() {
    let mut m = plate_with_two_pockets(3.0);
    let p2 = m.feature_id("p2").expect("feature id");

    // Everything outside pocket 1's 5..15 footprint in x: pocket 2's five
    // faces, the plate's top, and three of its four sides.
    let untouched = |m: &ModelBuilder| -> std::collections::BTreeMap<u64, [f64; 3]> {
        face_ids_at(m, p2, false)
            .into_iter()
            .filter(|(_, c)| c[0] > 20.0)
            .collect()
    };
    let before = untouched(&m);
    assert!(
        before.len() >= 6,
        "expected at least six untouched faces, got {before:?}"
    );
    m.edit_extrude_depth("p1", 10.0).expect("deepen p1");

    let after = untouched(&m);
    assert_eq!(
        before, after,
        "an edit to pocket 1 renamed output faces nowhere near it"
    );
}
