use super::*;

/// How far past the measured material a `ThroughAll` cutter runs, as a
/// FRACTION of that material's own extent along the sweep axis.
///
/// The overshoot exists so the cutter's exit face is never coplanar with the
/// body's far side. It must be relative: the engine's unit is the metre and a
/// document may be authored at any scale, so an absolute margin is a hidden
/// scale assumption. The previous absolute 1 m margin is exactly what assay
/// **P0012** caught — the same model read CORRECT at ×1e-3 (where 1 m dwarfed
/// the whole part) and silently cut NOTHING at ×1.
pub(super) const THROUGH_ALL_OVERSHOOT: f64 = 1e-2;

/// Resolve depth based on the depth mode.
///
/// Every mode is measured along `direction`, which is the direction this
/// extrude ACTUALLY sweeps — the reversal a cut with no authored direction
/// applies has already been folded in by the caller, and `swept` is the
/// material's interval along that same direction, relative to the sketch
/// plane. Measuring a depth along one direction and then sweeping along
/// another is how a `ThroughAll` cut came to miss its body entirely (P0012).
///
/// - `Blind`: returns `blind_depth` directly
/// - `ThroughAll`: the far end of `swept` plus a RELATIVE overshoot
/// - `UpTo`: resolves the reference position and its distance along `direction`
// Nine inputs, past clippy's default threshold. They are the independent axes
// of a depth resolution (mode, magnitude, sketch frame, measured material);
// bundling them into a struct would only move the argument list, not shorten it.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_depth(
    mode: &DepthMode,
    blind_depth: f64,
    sketch_origin: [f64; 3],
    direction: [f64; 3],
    swept: Option<(f64, f64)>,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    kb: &mut dyn KernelBundle,
    context: Option<&EditContext>,
) -> Result<f64, EngineError> {
    match mode {
        DepthMode::Blind => Ok(blind_depth),

        // `swept` is the measured material's interval along the direction this
        // extrude ACTUALLY sweeps, relative to the sketch plane
        // (`swept_primary`, computed AFTER the cut reversal). Through-all is
        // "past the far side of that material", plus a relative overshoot so
        // the exit face is never coplanar with the far side.
        DepthMode::ThroughAll => through_all_depth(swept, blind_depth),

        DepthMode::UpTo { reference } => {
            // Resolve the reference to a 3D position
            let ref_position =
                resolve_reference_position(reference, feature_results, tree, kb, context)?;
            up_to_depth(sketch_origin, direction, ref_position)
        }
    }
}

/// `ThroughAll`: how far to sweep so the cutter clears the measured material.
///
/// `swept` is that material's interval along the direction the extrude
/// ACTUALLY sweeps, relative to the sketch plane (see `resolve_depth`). Split
/// out as a pure function so the arithmetic — which is what assay P0012 got
/// wrong — is pinned directly.
pub(super) fn through_all_depth(
    swept: Option<(f64, f64)>,
    blind_depth: f64,
) -> Result<f64, EngineError> {
    match swept {
        Some((lo, hi)) => {
            if hi <= 0.0 {
                // Nothing lies ahead of the sketch plane along the sweep: no
                // depth whatsoever reaches the body. Loud, never a cutter that
                // silently misses (assay P0012).
                return Err(EngineError::ResolutionFailed {
                    reason: format!(
                        "ThroughAll: the measured body lies entirely behind the sketch plane \
                         along the sweep direction (its interval is [{lo:.6e}, {hi:.6e}] \
                         relative to the plane) — flip the direction or use a blind depth"
                    ),
                });
            }
            // The overshoot is a FRACTION of the material's own extent along
            // the sweep axis — never an absolute length.
            let reach = (hi - lo).max(hi.abs());
            Ok(hi + reach * THROUGH_ALL_OVERSHOOT)
        }
        None => {
            // No measured body — nothing to go "through". Fall back to the
            // authored depth, which is at least at the model's own scale; an
            // absolute default is a hidden scale assumption.
            if blind_depth > 0.0 {
                Ok(blind_depth)
            } else {
                Err(EngineError::ResolutionFailed {
                    reason: "ThroughAll: no body to measure and no positive depth authored".into(),
                })
            }
        }
    }
}

/// `UpTo`: the distance from the sketch plane to `ref_position`, measured
/// along `direction` — the direction the extrude ACTUALLY sweeps, so a
/// reference the reversal put in front resolves instead of being refused.
pub(super) fn up_to_depth(
    sketch_origin: [f64; 3],
    direction: [f64; 3],
    ref_position: [f64; 3],
) -> Result<f64, EngineError> {
    let Some(dir) = unit_or_none(direction) else {
        return Err(EngineError::ResolutionFailed {
            reason: "Extrude direction is zero-length".into(),
        });
    };
    let ref_proj = ref_position[0] * dir[0] + ref_position[1] * dir[1] + ref_position[2] * dir[2];
    let origin_proj =
        sketch_origin[0] * dir[0] + sketch_origin[1] * dir[1] + sketch_origin[2] * dir[2];
    let depth = ref_proj - origin_proj;
    if depth <= 0.0 {
        return Err(EngineError::ResolutionFailed {
            reason: format!("UpTo reference is behind the sweep direction (depth = {depth:.3})"),
        });
    }
    Ok(depth)
}

/// Unit-length `direction`, or `None` when it is degenerate.
pub(super) fn unit_or_none(direction: [f64; 3]) -> Option<[f64; 3]> {
    let len =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt();
    (len >= TAU_WORK).then(|| [direction[0] / len, direction[1] / len, direction[2] / len])
}

/// The interval the given solids' vertices occupy along `direction`, as
/// absolute projections (`v · d̂`), or `None` when nothing could be measured.
///
/// ONE measurement feeds BOTH gates that ask "where is the material this
/// feature acts on, along the extrude axis?" — the cut-direction reversal and
/// a `ThroughAll` depth (CLAUDE.md: fix all gates sharing a metric).
/// It reads vertex positions, not face centroids, so it is the true extent
/// along the axis regardless of face counts or centroid weighting. Note the
/// FULL interval: the old `compute_solid_extent` returned the far end alone,
/// clamped at `0.0`, which read "extent 0" for a body lying entirely behind
/// the plane — indistinguishable from a body touching it (assay P0012).
pub(super) fn solids_projection_range(
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
    solids: &[waffle_types::kernel::KernelSolidHandle],
    direction: [f64; 3],
) -> Option<(f64, f64)> {
    let dir = unit_or_none(direction)?;
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for solid in solids {
        for vid in &introspect.list_vertices(solid) {
            let sig = introspect.compute_signature(*vid, TopoKind::Vertex);
            if let Some(c) = sig.centroid {
                let proj = c[0] * dir[0] + c[1] * dir[1] + c[2] * dir[2];
                lo = lo.min(proj);
                hi = hi.max(proj);
            }
        }
    }
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// Resolve a GeomRef to a 3D position for UpTo depth mode.
pub(super) fn resolve_reference_position(
    reference: &waffle_types::GeomRef,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    kb: &dyn KernelBundle,
    context: Option<&EditContext>,
) -> Result<[f64; 3], EngineError> {
    // A reference into another instance of the open assembly (in-context
    // editing) resolves through the context, into this part's frame; without
    // the context it is a loud failure, never a guess.
    if let Some(scope) = &reference.scope {
        return match context {
            Some(ctx) => ctx.centroid(reference, kb.as_introspect()),
            None => Err(EngineError::ResolutionFailed {
                reason: format!(
                    "UpTo reference is on {}; open the part in that assembly's context",
                    crate::context::describe_scope(scope, None)
                ),
            }),
        };
    }
    // Try to resolve the reference via the feature engine's resolve system
    match &reference.anchor {
        waffle_types::Anchor::FeatureOutput { feature_id, .. } => {
            // Check if we can resolve via the feature result
            let resolved = resolve_with_fallback(reference, feature_results).map_err(|e| {
                EngineError::ResolutionFailed {
                    reason: format!("UpTo reference resolution failed: {}", e),
                }
            })?;

            // Get the centroid of the resolved entity
            let result =
                feature_results
                    .get(feature_id)
                    .ok_or_else(|| EngineError::ResolutionFailed {
                        reason: format!("UpTo reference feature {} not found", feature_id),
                    })?;

            // Get solid handle from the result
            let _handle = result
                .outputs
                .first()
                .map(|(_, body)| &body.handle)
                .ok_or_else(|| EngineError::ResolutionFailed {
                    reason: "UpTo reference feature has no solid output".into(),
                })?;

            let introspect = kb.as_introspect();
            let sig = introspect.compute_signature(resolved.kernel_id, reference.kind);
            // An extrude stops AT a point of the referenced entity, so the
            // centroid has to be a point ON it. For a face on a surface of
            // revolution that goes all the way round, the centroid is the
            // area centroid on the AXIS and `normal` is unset (N0 of
            // `specs/agent_mechanical_design.md` §5.1) — a point the face does
            // not contain. Refuse it, which is what this did before N0, when
            // such a face had no centroid at all.
            if reference.kind == TopoKind::Face
                && sig.surface_type.as_deref() != Some("planar")
                && sig.normal.is_none()
            {
                return Err(EngineError::ResolutionFailed {
                    reason: format!(
                        "UpTo references a {} face that goes all the way round its axis, so it \
                         has no single point to stop at; reference a planar face, a partial \
                         face, or a datum plane",
                        sig.surface_type.as_deref().unwrap_or("curved")
                    ),
                });
            }
            sig.centroid.ok_or_else(|| EngineError::ResolutionFailed {
                reason: "UpTo reference has no centroid".into(),
            })
        }
        waffle_types::Anchor::Datum { datum_id } => {
            let (origin, _normal) =
                find_datum_plane_data(*datum_id, tree, feature_results, kb.as_introspect())?;
            Ok(origin)
        }
    }
}

/// Find the most recent solid handle from a feature's references.
///
/// For fillet/chamfer/shell, the edges/faces point to a specific feature's output.
/// We find the solid handle by looking at the first GeomRef's anchor feature_id.
/// If no references are provided, returns an error.
pub(super) fn find_latest_solid_handle(
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
) -> Result<waffle_types::kernel::KernelSolidHandle, EngineError> {
    // Get the target feature from the first edge/face reference
    let first_ref = match &feature.operation {
        Operation::Fillet { params } => params.edges.first(),
        Operation::Chamfer { params } => params.edges.first(),
        Operation::Shell { params } => params.faces_to_remove.first(),
        _ => None,
    };

    let geom_ref = first_ref.ok_or(EngineError::ResolutionFailed {
        reason: "Fillet/chamfer/shell needs at least one edge/face reference".to_string(),
    })?;

    find_solid_handle(geom_ref, feature_results)
}
