//! PR-KV13 F6a — feature-engine resolves a face's CREATING feature through
//! booleans (the persistent-naming payoff: click a face → the original
//! extrude, not the boolean).
//!
//! Exercises the full stack: kernel-v2 persistent ids + journal (F1–F2) →
//! lineage walk (F3) → `KernelIntrospect::face_provenance` (F5) → feature-engine
//! `created_by_feature` resolver (F6a). The wasm-bridge + Svelte display is F6b.

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
