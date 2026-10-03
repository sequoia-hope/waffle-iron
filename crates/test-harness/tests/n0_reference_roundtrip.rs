//! The N0 oracle of `specs/agent_mechanical_design.md` §5.1: **every face a
//! body lists resolves back to ITSELF.**
//!
//! Both N0 defects break this round trip, and they break it silently:
//!
//! - a curved face's fingerprint was empty, so a `Selector::Signature`
//!   carrying it scored 0.0 against every candidate;
//! - a roleless face's selector was the face INDEX in `adjacency_hash`, a
//!   field the resolver does not read — same 0.0.
//!
//! Either way `ResolvePolicy::BestEffort` bound the reference to whichever
//! entity was created first and attached a "0.0%" warning nobody reads. This
//! test asserts the round trip over real kernel-v2 geometry: a cylinder (two
//! planar caps and a cylindrical lateral) and a boolean result (a cylinder
//! union a box — curved and planar faces from two operands in one body).

use std::collections::HashMap;

use feature_engine::resolve::resolve_geom_ref_live;
use test_harness::ModelBuilder;
use waffle_types::{ResolvePolicy, Selector, TopoKind};

/// Resolve every `GeomRef` the body's face list hands out, under `Strict`, and
/// assert each comes back to the face it was built from — twice over:
///
/// 1. with the operation's own role assignments, the way the bridge lists a
///    modelled body (`Selector::Role` for every face that has one); and
/// 2. with NO roles, which is what an imported body hands the same builder
///    (`import_body` records none) and the case §5.1 says "bites every
///    imported STEP body". Every selector is then a geometric fingerprint, so
///    this arm is the one both N0 defects break.
fn assert_every_face_resolves_to_itself(m: &mut ModelBuilder, feature: &str) {
    for roleless in [false, true] {
        assert_round_trip(m, feature, roleless);
    }
}

fn assert_round_trip(m: &mut ModelBuilder, feature: &str, roleless: bool) {
    let feature_id = m.feature_id(feature).expect("feature");
    let result = m.op_result(feature).expect("result").clone();
    let mut results = HashMap::new();
    results.insert(feature_id, result.clone());

    // The face list is built from the body's render mesh, as the bridge does.
    let meshes: Vec<_> = result
        .outputs
        .iter()
        .map(|(key, body)| {
            let mesh = m
                .kernel_mut()
                .tessellate(&body.handle, 0.001)
                .expect("the body tessellates");
            (key.clone(), mesh)
        })
        .collect();

    let introspect = m.kernel_ref().as_introspect();
    let mut checked = 0usize;
    let mut fingerprinted = 0usize;
    for (key, mesh) in &meshes {
        let refs = wasm_bridge::face_refs::face_geom_refs(
            feature_id,
            key,
            mesh,
            if roleless {
                &[]
            } else {
                &result.provenance.role_assignments
            },
            introspect,
        );
        for (face, geom_ref) in refs {
            let mut strict = geom_ref.clone();
            strict.policy = ResolvePolicy::Strict;
            if matches!(strict.selector, Selector::Signature { .. }) {
                fingerprinted += 1;
            }
            let resolved =
                resolve_geom_ref_live(&strict, &results, introspect).unwrap_or_else(|e| {
                    panic!("{feature} (roleless={roleless}): face {face:?} does not resolve: {e}")
                });
            assert_eq!(
                resolved.kernel_id,
                face,
                "{feature}: the ref for face {face:?} resolved to {:?} instead \
                 (selector {:?}, signature {:?})",
                resolved.kernel_id,
                strict.selector,
                introspect.compute_signature(face, TopoKind::Face)
            );
            assert!(
                resolved.warnings.is_empty(),
                "{feature}: face {face:?} resolved with warnings: {:?}",
                resolved.warnings
            );
            checked += 1;
        }
    }
    assert!(checked >= 3, "{feature}: only {checked} faces checked");
    if roleless {
        assert_eq!(
            fingerprinted, checked,
            "{feature}: with no roles every selector must be a fingerprint"
        );
    }
    println!("{feature} (roleless={roleless}): {checked} faces, {fingerprinted} by fingerprint");
}

#[test]
fn every_face_of_a_cylinder_resolves_to_itself() {
    let mut m = ModelBuilder::kernel_v2();
    m.true_circle_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("cyl", "sk", 10.0).unwrap();
    m.assert_has_solid("cyl").unwrap();
    assert_every_face_resolves_to_itself(&mut m, "cyl");
}

#[test]
fn every_face_of_a_cylinder_union_box_resolves_to_itself() {
    let mut m = ModelBuilder::kernel_v2();
    m.true_circle_sketch("sk_c", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude_no_merge("cyl", "sk_c", 10.0).unwrap();
    m.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], 0., 0., 8., 8.)
        .unwrap();
    m.extrude_no_merge("box", "sk_b", 4.0).unwrap();
    m.boolean_union("u", "cyl", "box").unwrap();
    m.assert_has_solid("u").unwrap();
    assert_every_face_resolves_to_itself(&mut m, "u");
}

/// A fingerprint is only worth saving if the same body computes the same one,
/// and if the drift a chord-tolerance change would cause still scores the same
/// face. Both are properties of the CURVED arm, which N0 introduced: a curved
/// face's area and centroid come from its render tessellation.
///
/// 1. **Reproducible.** Building the body again gives a bit-identical
///    fingerprint for every face. (The tessellator is deterministic and
///    `face_signature` sums its triangles in mesh order, so anything else
///    would be a nondeterminism bug, not rounding.)
/// 2. **The scoring covers a tolerance change.** `tessellate_face` is pinned
///    to `RENDER_CHORD_TOLERANCE_REL`, so a stored fingerprint cannot drift
///    today — but if that constant moved, a curved face's inscribed area would
///    move by the chord deficit, ≈3.3e-4 relative at the canonical band. This
///    arm perturbs every stored area by ten times that and asserts the
///    reference still binds the same face, with no low-confidence warning: the
///    scorer's area term is relative and the margin to the next candidate is
///    orders of magnitude wider than the deficit.
#[test]
fn a_curved_fingerprint_is_reproducible_and_survives_a_chord_tolerance_change() {
    // `TopoSignature` is not `PartialEq`; `{:?}` on an f64 is the shortest
    // round-tripping decimal, so the Debug rendering is injective and a
    // string comparison IS a bit comparison.
    let fingerprints = |()| -> Vec<String> {
        let mut m = ModelBuilder::kernel_v2();
        m.true_circle_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
            .unwrap();
        m.extrude("cyl", "sk", 10.0).unwrap();
        let result = m.op_result("cyl").expect("result").clone();
        let handle = &result.outputs.first().expect("body").1.handle;
        let introspect = m.kernel_ref().as_introspect();
        introspect
            .list_faces(handle)
            .into_iter()
            .map(|f| format!("{:?}", introspect.compute_signature(f, TopoKind::Face)))
            .collect()
    };
    let first = fingerprints(());
    let again = fingerprints(());
    assert_eq!(
        first.len(),
        again.len(),
        "the same construction lists the same faces"
    );
    for (i, (a, b)) in first.iter().zip(again.iter()).enumerate() {
        assert_eq!(a, b, "face {i}'s fingerprint is not reproducible");
    }
    assert!(
        first.iter().any(|s| s.contains("cylindrical")),
        "the fixture must carry a curved face: {first:?}"
    );

    // Arm 2: the same round trip as above, with every stored area moved by
    // ten chord deficits before it is resolved.
    let mut m = ModelBuilder::kernel_v2();
    m.true_circle_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("cyl", "sk", 10.0).unwrap();
    let feature_id = m.feature_id("cyl").expect("feature");
    let result = m.op_result("cyl").expect("result").clone();
    let mut results = HashMap::new();
    results.insert(feature_id, result.clone());
    let (key, body) = result.outputs.first().expect("body").clone();
    let mesh = m
        .kernel_mut()
        .tessellate(&body.handle, 0.001)
        .expect("tessellates");
    let introspect = m.kernel_ref().as_introspect();
    let refs = wasm_bridge::face_refs::face_geom_refs(feature_id, &key, &mesh, &[], introspect);
    assert!(!refs.is_empty(), "no faces listed");
    for (face, geom_ref) in refs {
        let mut drifted = geom_ref.clone();
        drifted.policy = ResolvePolicy::Strict;
        if let Selector::Signature { signature } = &mut drifted.selector {
            if let Some(area) = signature.area.as_mut() {
                *area *= 1.0 - 3.3e-3;
            }
        } else {
            panic!("with no roles every selector is a fingerprint: {drifted:?}");
        }
        let resolved = resolve_geom_ref_live(&drifted, &results, introspect)
            .unwrap_or_else(|e| panic!("face {face:?} lost to a chord-band area drift: {e}"));
        assert_eq!(
            resolved.kernel_id, face,
            "face {face:?} bound elsewhere after an area drift"
        );
        assert!(
            resolved.warnings.is_empty(),
            "face {face:?}: {:?}",
            resolved.warnings
        );
    }
}
