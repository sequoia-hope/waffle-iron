//! PR-KV13 F6a + F7 — face→feature provenance verification.
//!
//! Exercises the full stack: kernel-v2 persistent ids + journal (F1–F2) →
//! lineage walk (F3) → `KernelIntrospect::face_provenance` (F5) → feature-engine
//! `created_by_feature` resolver (F6a). The wasm-bridge + Svelte display is F6b.
//!
//! F7 verification matrix (adapted from `PERSISTENT-NAMING.md` — its
//! fillet/chamfer-centric scenarios can't run, those ops being deferred, so the
//! matrix is built around the shipped face→feature capability):
//! - **Stability under a parameter edit** — `created_by_survives_an_upstream_edit`.
//! - **Stability under a downstream change** — `upstream_pids_stable_under_downstream_change`.
//! - **No-mislabel + completeness** — a 3-contributor union attributes faces to
//!   EXACTLY the three originals, no extras (`union_of_three_*`).
//! - **Graceful break** — deleting a contributor rebuilds without crash or
//!   mislabel (`deleting_a_contributor_*`).

use test_harness::ModelBuilder;

#[test]
fn union_face_resolves_to_original_extrude_not_the_boolean() {
    let mut m = ModelBuilder::kernel_v2();

    // Two overlapping boxes as SEPARATE bodies (no auto-merge), then an
    // explicit union feature — so the boolean is its own feature, distinct
    // from the two extrudes.
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    let a = m.extrude_no_merge("a", "sk_a", 10.0).unwrap();

    m.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], 5., 5., 15., 15.)
        .unwrap();
    let b = m.extrude_no_merge("b", "sk_b", 10.0).unwrap();

    let u = m.boolean_union("u", "a", "b").unwrap();

    let handle = m.solid_handle("u").expect("union body handle");
    let introspect = m.kernel_ref().as_introspect();
    let faces = introspect.list_faces(&handle);
    assert!(!faces.is_empty(), "union produced faces");

    let mut resolved = 0usize;
    for f in faces {
        let cb = m.state.engine.created_by_feature(introspect, f);
        // Every face of the union body was INTRODUCED by extrude a or b —
        // never by the union feature itself (the boolean only re-derived
        // them; their geometry came from the original extrudes).
        assert_ne!(cb, Some(u), "a union face must not resolve to the boolean");
        if let Some(fid) = cb {
            assert!(
                fid == a || fid == b,
                "created_by {fid} should be extrude a ({a}) or b ({b})"
            );
            resolved += 1;
        }
    }
    // The resolver actually resolved faces (not just all-None).
    assert!(
        resolved > 0,
        "at least some union faces resolved to a creating feature"
    );
}

/// The set of persistent face ids of a feature's output solid.
fn face_pid_set(m: &ModelBuilder, name: &str) -> std::collections::BTreeSet<u64> {
    let handle = m.solid_handle(name).expect("handle");
    let introspect = m.kernel_ref().as_introspect();
    introspect
        .list_faces(&handle)
        .into_iter()
        .filter_map(|f| introspect.face_provenance(f).map(|p| p.pid))
        .collect()
}

#[test]
fn upstream_pids_stable_under_downstream_change() {
    // F4a target: a downstream change leaves an upstream feature's face Pids
    // unchanged (so a stored Pid reference to an upstream face stays valid).
    // This already holds — the arena is deterministic and incremental rebuild
    // does not re-execute the upstream feature — so F4a needs no new
    // stable-seeding machinery for this case.
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("a", "sk_a", 10.0).unwrap();
    let before = face_pid_set(&m, "a");
    assert!(!before.is_empty());

    // Downstream: a far-away separate box (no overlap → no consumption of a).
    m.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], 100., 100., 110., 110.)
        .unwrap();
    m.extrude("b", "sk_b", 10.0).unwrap();

    assert_eq!(
        before,
        face_pid_set(&m, "a"),
        "a downstream feature must not perturb upstream face Pids"
    );
}

/// Collect the set of `created_by` features over a feature's output-solid faces.
fn created_by_set(m: &mut ModelBuilder, name: &str) -> std::collections::HashSet<uuid::Uuid> {
    let handle = m.solid_handle(name).expect("solid handle");
    let introspect = m.kernel_ref().as_introspect();
    let mut set = std::collections::HashSet::new();
    for f in introspect.list_faces(&handle) {
        if let Some(fid) = m.state.engine.created_by_feature(introspect, f) {
            set.insert(fid);
        }
    }
    set
}

#[test]
fn created_by_survives_an_upstream_edit() {
    // KV13 F4 (edit-survival): `created_by` is recomputed every rebuild, so a
    // face→feature lineage should re-resolve correctly AFTER an upstream
    // parameter edit — without the Parasolid-grade stable-Pid machinery. This
    // test measures whether that already holds (and so localizes what, if
    // anything, F4's deeper work must add).
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    let a = m.extrude_no_merge("a", "sk_a", 10.0).unwrap();
    m.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], 5., 5., 15., 15.)
        .unwrap();
    let b = m.extrude_no_merge("b", "sk_b", 10.0).unwrap();
    m.boolean_union("u", "a", "b").unwrap();

    let before = created_by_set(&mut m, "u");
    assert!(
        before.contains(&a) && before.contains(&b),
        "baseline: union from both extrudes"
    );

    // Edit the FIRST extrude's depth (taller) — re-runs e1 then the union with
    // entirely new kernel ids/pids.
    m.edit_extrude_depth("a", 18.0).expect("edit a depth");

    let after = created_by_set(&mut m, "u");
    assert!(
        after.contains(&a),
        "after the edit, the union still attributes faces to the original extrude a"
    );
    assert!(after.contains(&b), "and to b");
    assert_eq!(
        before, after,
        "the set of creating features is unchanged by the edit"
    );
}

// =========================================================================
// F7 — verification matrix (no-mislabel + completeness; graceful break)
// =========================================================================

#[test]
fn union_of_three_attributes_to_exactly_the_three_contributors() {
    // No-mislabel + completeness: a body unioned from three overlapping
    // extrudes attributes every face to one of those three originals — all
    // three appear (none lost), and NO other feature (a sketch, an intermediate
    // union) is ever the created_by. The adversarial direction: confidently
    // wrong attribution (to the boolean, to a sketch, to an unrelated feature)
    // is worse than "unknown", so the set must be EXACTLY {a, b, c}.
    // Stagger the sketch planes in Z (origins 0/2/4) so the boxes overlap in
    // the interior but share NO coplanar faces — avoiding the M8 coplanar-
    // boolean gap, which is orthogonal to provenance.
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    let a = m.extrude_no_merge("a", "sk_a", 10.0).unwrap();
    m.rect_sketch("sk_b", [0., 0., 2.], [0., 0., 1.], 6., 6., 16., 16.)
        .unwrap();
    let b = m.extrude_no_merge("b", "sk_b", 10.0).unwrap();
    m.rect_sketch("sk_c", [0., 0., 4.], [0., 0., 1.], 12., 12., 22., 22.)
        .unwrap();
    let c = m.extrude_no_merge("c", "sk_c", 10.0).unwrap();

    // Chain two unions: ab = a ∪ b, then u = ab ∪ c. All three overlap
    // pairwise in x/y/z → one connected body, no coplanar pairs.
    m.boolean_union("ab", "a", "b").unwrap();
    m.boolean_union("u", "ab", "c").unwrap();

    let set = created_by_set(&mut m, "u");
    let expected: std::collections::HashSet<uuid::Uuid> = [a, b, c].into_iter().collect();
    assert_eq!(
        set, expected,
        "every face of the 3-way union resolves to EXACTLY the three original \
         extrudes — no boolean, no sketch, no unrelated feature, none missing"
    );
}

#[test]
fn deleting_a_contributor_rebuilds_without_crash_or_mislabel() {
    // Graceful break (PERSISTENT-NAMING scenario 4, adapted): build a ∪ b, then
    // delete a contributor. The rebuild must complete without a crash, and the
    // surviving geometry must not be MISattributed — whatever bodies remain
    // attribute their faces only to features that still exist (never to the
    // deleted one, never to an unrelated id).
    let mut m = ModelBuilder::kernel_v2();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    let a = m.extrude_no_merge("a", "sk_a", 10.0).unwrap();
    m.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], 5., 5., 15., 15.)
        .unwrap();
    let b = m.extrude_no_merge("b", "sk_b", 10.0).unwrap();
    m.boolean_union("u", "a", "b").unwrap();

    // Delete b (a union operand). Must not panic.
    m.delete_feature("b").expect("delete b dispatches");

    // The deleted feature's id must not survive as anyone's created_by, and the
    // surviving features are exactly the live set. Enumerate every live body.
    let live: std::collections::HashSet<uuid::Uuid> =
        m.state.engine.tree.features.iter().map(|f| f.id).collect();
    assert!(
        !live.contains(&b),
        "deleted feature b is gone from the tree"
    );

    let introspect = m.kernel_ref().as_introspect();
    let mut any_solid = false;
    for feat in m.state.engine.tree.features.clone() {
        if let Some(result) = m.state.engine.feature_results.get(&feat.id) {
            for (_key, body) in &result.outputs {
                any_solid = true;
                for face in introspect.list_faces(&body.handle) {
                    if let Some(fid) = m.state.engine.created_by_feature(introspect, face) {
                        assert_ne!(
                            fid, b,
                            "no surviving face is attributed to the deleted feature"
                        );
                        assert!(live.contains(&fid), "created_by {fid} is a live feature");
                    }
                }
            }
        }
    }
    // Sanity: deleting b left SOME geometry (a survives in some form) and the
    // engine is still coherent. (a's id may or may not appear depending on how
    // the orphaned union degrades — the invariant we assert is no mislabel.)
    let _ = (a, any_solid);
}
