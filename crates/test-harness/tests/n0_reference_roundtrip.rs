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

/// The three consumers that read a face's point normal, or its centroid as a
/// point ON the face, against a body whose curved face has neither (N0: a
/// full-turn surface of revolution reports `normal: None` and an ON-AXIS
/// centroid). None of them may panic, and none may pick a face arbitrarily.
///
/// A plain cylinder is the fixture: two planar caps and one full lateral.
#[test]
fn a_full_turn_face_without_a_normal_is_skipped_not_guessed() {
    use waffle_types::{Anchor, Filter, GeomRef, Selector, TieBreak, TopoQuery};

    let mut m = ModelBuilder::kernel_v2();
    m.true_circle_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("cyl", "sk", 10.0).unwrap();
    let feature_id = m.feature_id("cyl").expect("feature");
    let result = m.op_result("cyl").expect("result").clone();
    let (output_key, body) = result.outputs.first().expect("body").clone();
    let mut results = HashMap::new();
    results.insert(feature_id, result.clone());
    let introspect = m.kernel_ref().as_introspect();

    // The fixture really is the case under test: one cylindrical face with no
    // normal and a centroid on the axis, plus planar caps that have both.
    let faces: Vec<_> = introspect
        .list_faces(&body.handle)
        .into_iter()
        .map(|f| (f, introspect.compute_signature(f, TopoKind::Face)))
        .collect();
    let lateral: Vec<_> = faces
        .iter()
        .filter(|(_, s)| s.surface_type.as_deref() == Some("cylindrical"))
        .collect();
    assert_eq!(lateral.len(), 1, "one lateral: {faces:?}");
    let (lat_id, lat_sig) = lateral[0];
    assert!(lat_sig.normal.is_none(), "{lat_sig:?}");
    assert!(lat_sig.axis.is_some(), "{lat_sig:?}");
    let on_axis = lat_sig.centroid.expect("on-axis centroid");
    assert!(
        on_axis[0].abs() < 1e-12 && on_axis[1].abs() < 1e-12,
        "{on_axis:?}"
    );
    assert!(
        faces.iter().any(|(_, s)| s.normal.is_some()),
        "the caps still have normals: {faces:?}"
    );

    let anchored = |selector: Selector| GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: output_key.clone(),
        },
        selector,
        policy: ResolvePolicy::BestEffort,
        scope: None,
    };

    // 1. `Filter::NormalDirection` EXCLUDES it. Even at a tolerance of π
    //    radians — which matches every direction there is — "no normal" is
    //    not a match, and the query still answers over the caps.
    for dir in [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]] {
        let filters = vec![Filter::NormalDirection {
            direction: dir,
            tolerance: std::f64::consts::PI,
        }];
        assert!(
            !feature_engine::resolve::passes_all_filters(lat_sig, &filters),
            "a tolerance of π radians still must not match a face with no \
             normal (direction {dir:?}): {lat_sig:?}"
        );
        let matched = faces
            .iter()
            .filter(|(_, s)| feature_engine::resolve::passes_all_filters(s, &filters))
            .count();
        assert_eq!(matched, 2, "but both caps do match (direction {dir:?})");
    }
    // A query carrying that filter therefore answers over the caps, and the
    // tie-break picks among them rather than reaching the lateral.
    let q = TopoQuery {
        filters: vec![Filter::NormalDirection {
            direction: [0.0, 0.0, 1.0],
            tolerance: 0.1,
        }],
        tie_break: Some(TieBreak::LargestArea),
    };
    let hit = resolve_geom_ref_live(
        &anchored(Selector::Query { query: q }),
        &results,
        introspect,
    )
    .expect("the +z cap resolves");
    assert_ne!(hit.kernel_id, *lat_id, "never the lateral");

    // 2. `Selector::Position` AT the lateral's own on-axis centroid resolves
    //    deterministically — the same face every time, never a coin flip.
    let pos = anchored(Selector::Position {
        x: on_axis[0],
        y: on_axis[1],
        z: on_axis[2],
    });
    let first = feature_engine::resolve::resolve_by_position(&pos, &results, introspect, on_axis)
        .map(|r| r.kernel_id);
    for _ in 0..3 {
        let again =
            feature_engine::resolve::resolve_by_position(&pos, &results, introspect, on_axis)
                .map(|r| r.kernel_id);
        assert_eq!(
            first.as_ref().ok(),
            again.as_ref().ok(),
            "a Position selector must be deterministic"
        );
    }
    assert!(
        first.is_ok(),
        "and it must not panic or fail outright: {first:?}"
    );

    // 3. `UpTo` against that face REFUSES, typed, naming why — it did so
    //    before N0 too (the face carried no centroid at all then), and the
    //    alternative is extruding to a plane through the axis.
    let lateral_ref = anchored(Selector::Signature {
        signature: lat_sig.clone(),
    });
    let _ = lat_id;
    m.true_circle_sketch("sk2", [0., 0., 20.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    let added = m.extrude_up_to("upto", "sk2", lateral_ref);
    // The refusal reaches the caller either way: as a dispatch error, or as a
    // typed engine error against the feature with no solid behind it. What it
    // may NOT do is quietly extrude to a plane through the axis.
    let refusal = match &added {
        Err(e) => e.to_string(),
        Ok(id) => {
            let msgs: Vec<&str> = m
                .engine_errors()
                .iter()
                .filter(|(f, _)| f == id)
                .map(|(_, msg)| msg.as_str())
                .collect();
            assert!(
                !msgs.is_empty(),
                "UpTo to a full-turn face must refuse, not pick a plane \
                 (engine errors: {:?})",
                m.engine_errors()
            );
            assert!(
                m.assert_has_solid("upto").is_err(),
                "and it must not have produced a body"
            );
            msgs.join(" | ")
        }
    };
    assert!(
        refusal.contains("all the way round") || refusal.contains("no centroid"),
        "the refusal says why: {refusal}"
    );
}
