use super::*;

/// Well-known UUIDs for the three built-in datum planes (must match planes.js).
pub(super) const FRONT_PLANE_ID: &str = "00000000-0000-0000-0000-000000000001";
pub(super) const TOP_PLANE_ID: &str = "00000000-0000-0000-0000-000000000002";
pub(super) const RIGHT_PLANE_ID: &str = "00000000-0000-0000-0000-000000000003";

/// Resolve a PlaneDefinition to (origin, normal).
pub(super) fn resolve_plane_definition(
    def: &PlaneDefinition,
    tree: &FeatureTree,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
) -> Result<([f64; 3], [f64; 3]), EngineError> {
    match def {
        PlaneDefinition::PointNormal { origin, normal, .. } => {
            let len =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            if len < TAU_WORK {
                return Err(EngineError::ResolutionFailed {
                    reason: "Datum plane normal is zero-length".into(),
                });
            }
            let n = [normal[0] / len, normal[1] / len, normal[2] / len];
            Ok((*origin, n))
        }
        PlaneDefinition::Offset {
            base_plane_id,
            distance,
            ..
        } => {
            let (base_origin, base_normal) =
                find_datum_plane_data(*base_plane_id, tree, feature_results, introspect)?;
            let origin = [
                base_origin[0] + base_normal[0] * distance,
                base_origin[1] + base_normal[1] * distance,
                base_origin[2] + base_normal[2] * distance,
            ];
            Ok((origin, base_normal))
        }
        PlaneDefinition::OffsetFromFace { base, distance, .. } => {
            let (base_origin, base_normal) = resolve_face_plane(base, feature_results, introspect)?;
            let origin = [
                base_origin[0] + base_normal[0] * distance,
                base_origin[1] + base_normal[1] * distance,
                base_origin[2] + base_normal[2] * distance,
            ];
            Ok((origin, base_normal))
        }
    }
}

/// Resolve a planar face GeomRef to its base plane `(origin, normal)`.
///
/// `origin` is a deterministic point ON the face (the average of the face's
/// boundary vertices, from `compute_signature`); `normal` is the planar face's
/// outward normal. Because the face is resolved from the *current* geometry
/// every rebuild, a face-base datum plane tracks the face as it moves — it is
/// not frozen at creation.
///
/// A non-planar base face is a loud `ResolutionFailed` — never a guessed plane.
///
/// JS agreement (`computeFacePlane` in `store.svelte.js`): both resolvers
/// return the SAME outward `normal` (the planar face normal) and an `origin`
/// that lies ON the same planar face — so the offset `origin + normal*distance`
/// lands on the SAME infinite plane in both. (JS uses the first rendered
/// triangle's centroid; the engine uses the face-centroid from
/// `compute_signature`. The `KernelIntrospect` contract does not expose raw
/// per-face triangles, and feature-engine must not reach into kernel-v2
/// internals, so the two on-face origin points may differ by an in-plane
/// translation — they are guaranteed coplanar, which is the property the
/// offset depends on.)
pub fn resolve_face_plane(
    base: &waffle_types::GeomRef,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
) -> Result<([f64; 3], [f64; 3]), EngineError> {
    let resolved = crate::resolve::resolve_geom_ref_live(base, feature_results, introspect)?;
    crate::connector::planar_face_plane(resolved.kernel_id, introspect, "Datum plane base face")
}

/// How far a re-resolved sketch plane may sit from the cached one and still be
/// "the same face, where it was". The same figure `context::apply_context` uses
/// for the in-context form of this question, for the same reason: below it the
/// difference is the two resolvers' f64 rounding, not the face moving.
pub(super) const SKETCH_PLANE_TOL: f64 = 1e-9;

/// Re-resolve the model face a sketch is drawn on, every rebuild, and say what
/// happened (N2 of `specs/agent_mechanical_design.md` §5.3 item 3).
///
/// Three outcomes:
///
/// 1. **The face is there, where it was.** Nothing is reported and nothing
///    moves: the sketch keeps its cached `plane_origin` / `plane_normal`
///    bit-for-bit, which is the frame it was SOLVED in. That is what §5.3
///    means by "the cached numbers remain the solve-time frame for unchanged
///    faces so determinism holds" — re-deriving a frame that agrees to 1e-9
///    would still move every point by the last few ulps.
/// 2. **The face is there and has MOVED** (or the pid answered through its
///    lineage root, or the authored selector had to rebind). A warning says
///    which, and by how far — and the sketch still keeps its cached frame,
///    because moving a solved sketch's plane moves every point of it and every
///    feature downstream. The plane-FOLLOWING half of this is deliberately not
///    in N2; see the spec's implementation notes.
/// 3. **The face is gone.** A loud typed refusal naming the face's last-known
///    signature, so the author can see WHICH face went missing. The sketch
///    produces no result and nothing downstream of it builds. It is never
///    re-bound to whichever face now scores best — that is the silent wrong
///    answer this increment exists to remove.
///
/// A sketch with no `plane_face` (a datum plane, a bare frame, an in-context
/// face, or any sketch authored before N2) is left exactly as it was.
pub(super) fn resolve_sketch_plane_face(
    sketch: &Sketch,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
) -> Result<Vec<String>, EngineError> {
    let Some(face) = &sketch.plane_face else {
        return Ok(Vec::new());
    };

    let describe = |sig: &waffle_types::TopoSignature| -> String {
        let mut parts = Vec::new();
        if let Some(t) = &sig.surface_type {
            parts.push(format!("a {t} face"));
        }
        if let Some(a) = sig.area {
            parts.push(format!("area {a:.6} m²"));
        }
        if let Some(c) = sig.centroid {
            parts.push(format!("centroid [{:.6}, {:.6}, {:.6}]", c[0], c[1], c[2]));
        }
        if let Some(n) = sig.normal {
            parts.push(format!("normal [{:.6}, {:.6}, {:.6}]", n[0], n[1], n[2]));
        }
        if parts.is_empty() {
            "a face the reference recorded no geometry for".to_string()
        } else {
            parts.join(", ")
        }
    };

    let (resolved, lost_identity) = crate::resolve::resolve_pinned(
        &face.target,
        face.fallback.as_ref(),
        feature_results,
        introspect,
    )
    .map_err(|e| {
        // The refusal keeps its classification (so a host can branch) and
        // gains the last-known signature (so a person can tell which face).
        let text = format!(
            "the face this sketch is drawn on is gone: {}. It was {}. The sketch is not drawn on \
             another face — re-attach it to the face you want.",
            e.resolution_text()
                .unwrap_or("the reference did not resolve"),
            describe(&face.signature)
        );
        match e {
            EngineError::ReferenceUnresolved(mut r) => {
                r.reason_text = text;
                EngineError::ReferenceUnresolved(r)
            }
            _ => EngineError::ResolutionFailed { reason: text },
        }
    })?;

    // A face that is not planar cannot carry a sketch. Loud, never a guessed
    // plane — the same rule `resolve_face_plane` applies to a datum base face.
    let (origin, normal) =
        crate::connector::planar_face_plane(resolved.kernel_id, introspect, "Sketch plane face")
            .map_err(|e| EngineError::ResolutionFailed {
                reason: format!(
            "the face this sketch is drawn on is no longer a plane it can be drawn on: {e}. It \
             was {}.",
            describe(&face.signature)
        ),
            })?;

    let mut warnings = Vec::new();
    if lost_identity.is_some() {
        warnings.push(format!(
            "this sketch's face lost its persistent identity and was re-found by geometry, which \
             may be a different face: it was {}",
            describe(&face.signature)
        ));
    }
    // The resolver's own account of which rung answered, verbatim.
    warnings.extend(resolved.warnings.iter().cloned());

    // The PLANE, not the point. The two resolvers deliberately return
    // different on-face origins — the engine's is the face centroid, the UI's
    // is a rendered triangle's centroid (`resolve_face_plane`'s doc comment
    // spells this out) — so comparing origins directly would report every
    // sketch as moved. What has to agree is the plane: the cached origin's
    // signed distance along the current normal, and the normal itself.
    let cached_normal = unit_normal(sketch.plane_normal);
    let off_plane = (0..3)
        .map(|i| (sketch.plane_origin[i] - origin[i]) * normal[i])
        .sum::<f64>()
        .abs();
    let turned = dist3(normal, cached_normal);
    let drift = off_plane.max(turned);
    if drift > SKETCH_PLANE_TOL {
        warnings.push(format!(
            "the face this sketch is drawn on has moved: the sketch's plane is {off_plane:.3e} off \
             it and its normal differs by {turned:.3e}. The sketch keeps the frame it was solved \
             in, so its geometry no longer lies on that face — re-draw it on the face to follow it."
        ));
    }
    Ok(warnings)
}

/// The sketch plane face of `feature`, if it is a sketch that pins one.
pub(super) fn pinned_plane_face(feature: &Feature) -> Option<&waffle_types::SketchFaceRef> {
    match &feature.operation {
        Operation::Sketch { sketch } => sketch.plane_face.as_ref(),
        _ => None,
    }
}

/// Which rung answered this feature's pinned plane face, for the record a
/// kernel-less read reports (N2 §5.3 item 4). `None` for a feature with no
/// pinned face.
///
/// Called only after the feature SUCCEEDED, so the resolution cannot fail —
/// `execute_feature` would have refused the feature. If it somehow does, say so
/// rather than claiming an answer.
pub(super) fn resolved_plane_face_state(
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
) -> Option<crate::types::ReferenceState> {
    let face = pinned_plane_face(feature)?;
    let outcome = crate::resolve::resolve_pinned(
        &face.target,
        face.fallback.as_ref(),
        feature_results,
        introspect,
    );
    Some(match outcome {
        Ok((resolved, primary)) => crate::types::ReferenceState {
            role: SKETCH_PLANE_FACE_ROLE.to_string(),
            resolves: true,
            via: Some(resolved.via),
            rebound: primary.is_some() || resolved.via.rebound(),
            lost_identity: primary
                .as_ref()
                .and_then(|e| e.resolution_reason().cloned()),
            refusal: None,
        },
        Err(e) => crate::types::ReferenceState {
            role: SKETCH_PLANE_FACE_ROLE.to_string(),
            resolves: false,
            via: None,
            rebound: false,
            lost_identity: None,
            refusal: e.resolution_reason().cloned(),
        },
    })
}

/// The same record for a feature that FAILED — but only when the failure was
/// this reference's own.
///
/// A sketch has other ways to fail (an x-axis that cannot orient its plane, a
/// face that resolved but is no longer planar), and reporting those as the
/// plane face having died is the misattribution this record exists to stop. The
/// refusal is recognised by the reference it names: `resolve_pinned` returns the
/// primary failure of `face.target`, whose digest is that reference's.
pub(super) fn refused_plane_face_state(
    feature: &Feature,
    e: &EngineError,
) -> Option<crate::types::ReferenceState> {
    let face = pinned_plane_face(feature)?;
    let EngineError::ReferenceUnresolved(refusal) = e else {
        return None;
    };
    if refusal.reference.as_ref()? != &crate::types::RefDigest::of(&face.target) {
        return None;
    }
    Some(crate::types::ReferenceState {
        role: SKETCH_PLANE_FACE_ROLE.to_string(),
        resolves: false,
        via: None,
        rebound: false,
        lost_identity: None,
        refusal: Some(refusal.reason.clone()),
    })
}

/// The `role` every sketch-plane-face record carries, so the engine and the
/// tool layer name it the same thing.
pub const SKETCH_PLANE_FACE_ROLE: &str = "sketch_plane_face";

pub(super) fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Resolve a datum plane, named by its UUID, to `(origin, normal)`.
///
/// The public face of [`find_datum_plane_data`], for a host that must resolve
/// a plane an agent named rather than one a rebuild is walking
/// (`specs/waffle_server_mode.md` §2.3 S3 C5 — the engine's `sketch_create`).
/// Purely additive: the resolution is the same one every rebuild already uses,
/// so no existing behaviour moves.
pub fn resolve_datum_plane(
    datum_id: Uuid,
    tree: &FeatureTree,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
) -> Result<([f64; 3], [f64; 3]), EngineError> {
    find_datum_plane_data(datum_id, tree, feature_results, introspect)
}

/// Look up origin and normal for a datum plane by its UUID.
///
/// Checks the three built-in planes first, then searches the feature tree
/// for user-created DatumPlane features.
pub(crate) fn find_datum_plane_data(
    datum_id: Uuid,
    tree: &FeatureTree,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
) -> Result<([f64; 3], [f64; 3]), EngineError> {
    let id_str = datum_id.to_string();
    // Built-in planes
    if id_str == FRONT_PLANE_ID {
        return Ok(([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
    }
    if id_str == TOP_PLANE_ID {
        return Ok(([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
    }
    if id_str == RIGHT_PLANE_ID {
        return Ok(([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
    }

    // Search user-created DatumPlane features
    for feature in &tree.features {
        if feature.id == datum_id {
            if let Operation::DatumPlane { params } = &feature.operation {
                return resolve_plane_definition(
                    &params.definition,
                    tree,
                    feature_results,
                    introspect,
                );
            }
        }
    }

    Err(EngineError::ResolutionFailed {
        reason: format!("Datum plane {} not found", datum_id),
    })
}

/// A sketch's plane normal at unit length. Stored normals are kept verbatim (an
/// agent or an old file may carry a 6-decimal vector); every kernel frame and
/// default extrude direction is built from the unit vector. An un-normalized
/// normal scaled circle frames by |n|: whole-circle extrudes were rejected and
/// region extrudes built off-circle edges (docs/notes/agent_bicycle_session_failures_2026_09_14.md F1/F2).
///
/// A normal that is already unit to f64 rounding is returned BIT-IDENTICAL.
/// Dividing it by a length of 1 ± 1 ulp only re-rounds it: the result is no
/// more unit, but every frame on a tilted plane moves by one ulp. That was
/// enough to flip the chained revolve union of assay R0081 from CORRECT to a
/// Stage-4 `LocalRefinementRequired` stop. The corpus normals are within
/// 1 EPSILON of unit (67 of 312 cases carry one that renormalizing would
/// change); the F1 defect class is a 6-decimal vector, 3×10⁹ EPSILON off.
pub(crate) fn unit_normal(n: [f64; 3]) -> [f64; 3] {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < TAU_WORK || (len - 1.0).abs() <= UNIT_TO_ROUNDING {
        return n;
    }
    [n[0] / len, n[1] / len, n[2] / len]
}

/// `unit_normal` leaves a normal whose length is this close to 1 untouched: a
/// few ulps, the rounding of a vector that was normalized once in f64.
pub(super) const UNIT_TO_ROUNDING: f64 = 4.0 * f64::EPSILON;

/// The sketch plane's basis: the sketch's OWN x axis when it carries one
/// (`Sketch.plane_x_axis`), else the one derived from the normal. Every
/// consumer of a sketch's in-plane frame goes through here or through
/// [`sketch_x_axis`], so "which way is up in this sketch" has one answer.
pub(crate) fn sketch_basis(sketch: &Sketch) -> SketchPlaneBasis {
    let normal = unit_normal(sketch.plane_normal);
    match sketch.plane_x_axis {
        Some(x) if SketchPlaneBasis::x_axis_is_usable(normal, x) => {
            SketchPlaneBasis::from_origin_normal_x(sketch.plane_origin, normal, Some(x))
        }
        // Bit-for-bit the pre-2026-09-24 path for every sketch without one.
        _ => SketchPlaneBasis::from_origin_normal(sketch.plane_origin, normal),
    }
}

/// The in-plane +u direction of a sketch — [`sketch_basis`]'s x axis, except
/// that a sketch without its own axis keeps `tangent_x_from_normal`'s exact
/// bits (the two agree, and this is the value every existing document was
/// built with).
pub(crate) fn sketch_x_axis(sketch: &Sketch) -> [f64; 3] {
    let normal = unit_normal(sketch.plane_normal);
    match sketch.plane_x_axis {
        Some(x) if SketchPlaneBasis::x_axis_is_usable(normal, x) => {
            SketchPlaneBasis::from_origin_normal_x(sketch.plane_origin, normal, Some(x)).x_axis
        }
        _ => tangent_x_from_normal(normal),
    }
}

/// Compute a tangent X axis from a plane normal.
/// Must match the JS formula in `sketchCoords.js:buildSketchPlane()`:
///   ref = |n·Z| < 0.99 ? Z : X   (Z=[0,0,1], X=[1,0,0])
///   xAxis = ref × n
pub(super) fn tangent_x_from_normal(n: [f64; 3]) -> [f64; 3] {
    // Dot product with Z axis: n[0]*0 + n[1]*0 + n[2]*1 = n[2]
    let ref_vec = if n[2].abs() < 0.99 {
        [0.0, 0.0, 1.0] // Z
    } else {
        [1.0, 0.0, 0.0] // X
    };
    // Cross product: ref × n
    let cx = [
        ref_vec[1] * n[2] - ref_vec[2] * n[1],
        ref_vec[2] * n[0] - ref_vec[0] * n[2],
        ref_vec[0] * n[1] - ref_vec[1] * n[0],
    ];
    let len = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    if len < TAU_WORK {
        return [1.0, 0.0, 0.0];
    }
    [cx[0] / len, cx[1] / len, cx[2] / len]
}

/// Resolve all GeomRef references for a feature, collecting warnings.
///
/// Currently `feature.references` is always empty, so this is
/// forward-compatible plumbing for when features carry explicit refs.
pub(super) fn resolve_feature_refs(
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    warnings: &mut Vec<String>,
) {
    for geom_ref in &feature.references {
        match resolve_with_fallback(geom_ref, feature_results) {
            Ok(resolved) => {
                warnings.extend(resolved.warnings);
            }
            Err(e) => {
                warnings.push(format!(
                    "Feature '{}': reference resolution warning: {}",
                    feature.name, e
                ));
            }
        }
    }
}
