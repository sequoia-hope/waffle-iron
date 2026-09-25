//! Engine side of the 3D sketch (`specs/sketch3d.md` S2).
//!
//! `waffle_types::sketch3d` is pure: it knows how to resolve sketch-relative
//! attachments, expand fillets and walk chains, but nothing about the kernel.
//! This module supplies the missing half — [`EngineAnchors`] resolves an
//! [`Attachment`](waffle_types::sketch3d::Attachment) that names model
//! geometry — and wraps the two into [`evaluate`].
//!
//! Evaluation happens during the rebuild WALK rather than in the engine's
//! mutable pre-pass, because a point attached to a vertex of feature N can
//! only resolve after feature N has rebuilt. The result therefore cannot be
//! written back onto the sketch (the tree is immutable there); it is carried
//! in `RebuildState::sketch3d` and read after the rebuild, exactly as a mate
//! connector's frame is.

use std::collections::HashMap;

use uuid::Uuid;
use waffle_types::kernel::KernelIntrospect;
use waffle_types::sketch3d::{AnchorHit, ExternalAnchors, Sketch3d, Sketch3dEvaluation};
use waffle_types::{GeomRef, SketchPlaneBasis, TopoKind};

use crate::resolve::ResolvedRef;
use crate::types::{EngineError, FeatureTree};
use modeling_ops::OpResult;

/// A plane as `(origin, normal)`, plus whatever the resolver that found it
/// had to say.
type PlaneWithWarnings = (([f64; 3], [f64; 3]), Vec<String>);

/// Resolves the model-geometry attachments of a 3D sketch against the state
/// of the rebuild at the point the sketch is reached.
pub struct EngineAnchors<'a> {
    pub feature_results: &'a HashMap<Uuid, OpResult>,
    pub introspect: &'a dyn KernelIntrospect,
    pub tree: &'a FeatureTree,
}

impl EngineAnchors<'_> {
    /// Resolve the reference to a kernel entity, keeping the resolver's
    /// warnings (a best-effort re-bind onto the nearest entity) and, on
    /// refusal, its reason — both reach the author through the evaluation.
    fn resolve(&self, reference: &GeomRef) -> Result<ResolvedRef, String> {
        // A `Position` selector is how a PICKED entity is named, and picking
        // is how a 3D sketch point attaches to the model — so it has to go
        // through the nearest-entity resolver, which is what lets the
        // attachment re-bind when the geometry moves. `resolve_geom_ref_live`
        // does not route it (it handles `Query`), and the plain resolver
        // refuses it outright. This is the ONE place that decision is made,
        // so a picked face resolves exactly as a picked vertex does.
        let resolved = if let waffle_types::Selector::Position { x, y, z } = reference.selector {
            crate::resolve::resolve_by_position(
                reference,
                self.feature_results,
                self.introspect,
                [x, y, z],
            )
        } else {
            crate::resolve::resolve_geom_ref_live(reference, self.feature_results, self.introspect)
        };
        resolved.map_err(|e| e.to_string())
    }

    /// `(origin, normal)` of a datum plane or a planar face, with the
    /// resolver's warnings.
    fn plane_of(&self, reference: &GeomRef) -> Result<PlaneWithWarnings, String> {
        match &reference.anchor {
            waffle_types::Anchor::Datum { datum_id } => crate::rebuild::resolve_datum_plane(
                *datum_id,
                self.tree,
                self.feature_results,
                self.introspect,
            )
            .map(|plane| (plane, Vec::new()))
            .map_err(|e| e.to_string()),
            waffle_types::Anchor::FeatureOutput { .. } => {
                let resolved = self.resolve(reference)?;
                let plane = crate::connector::planar_face_plane(
                    resolved.kernel_id,
                    self.introspect,
                    "OnPlane attachment face",
                )
                .map_err(|e| e.to_string())?;
                Ok((plane, resolved.warnings))
            }
        }
    }
}

impl ExternalAnchors for EngineAnchors<'_> {
    fn vertex(&self, reference: &GeomRef) -> Result<AnchorHit, String> {
        if reference.kind != TopoKind::Vertex {
            return Err(format!(
                "a Vertex attachment needs a Vertex reference, not {:?}",
                reference.kind
            ));
        }
        let resolved = self.resolve(reference)?;
        // A vertex signature's centroid IS the vertex point, exactly
        // (`kernel_v2::adapter::vertex_signature`) — no averaging involved.
        let point = self
            .introspect
            .compute_signature(resolved.kernel_id, TopoKind::Vertex)
            .centroid
            .ok_or_else(|| "the vertex reports no position".to_string())?;
        Ok(AnchorHit {
            point,
            warnings: resolved.warnings,
        })
    }

    fn edge_point(&self, reference: &GeomRef, t: f64) -> Result<AnchorHit, String> {
        if reference.kind != TopoKind::Edge {
            return Err(format!(
                "an EdgePoint attachment needs an Edge reference, not {:?}",
                reference.kind
            ));
        }
        if !t.is_finite() || !(0.0..=1.0).contains(&t) {
            return Err(format!("edge parameter t = {t} is not within [0, 1]"));
        }
        let resolved = self.resolve(reference)?;
        let poly = self.introspect.edge_polyline(resolved.kernel_id);
        if poly.len() < 2 {
            return Err("the edge has no polyline to interpolate along".to_string());
        }
        // Interpolate by ARC LENGTH along the polyline, so t is a uniform
        // parameter along the edge as drawn rather than along its sample
        // index — a curved edge's samples are not equally spaced.
        let seg: Vec<f64> = poly.windows(2).map(|w| dist(w[0], w[1])).collect();
        let total: f64 = seg.iter().sum();
        if !total.is_finite() || total <= 0.0 {
            return Err("the edge has zero length".to_string());
        }
        let mut want = t * total;
        let mut point = *poly.last().expect("at least two samples");
        for (i, len) in seg.iter().enumerate() {
            if want <= *len || i + 1 == seg.len() {
                let f = if *len > 0.0 { want / len } else { 0.0 };
                let (a, b) = (poly[i], poly[i + 1]);
                point = [
                    a[0] + (b[0] - a[0]) * f,
                    a[1] + (b[1] - a[1]) * f,
                    a[2] + (b[2] - a[2]) * f,
                ];
                break;
            }
            want -= len;
        }
        Ok(AnchorHit {
            point,
            warnings: resolved.warnings,
        })
    }

    fn plane_point(&self, reference: &GeomRef, uv: [f64; 2]) -> Result<AnchorHit, String> {
        if !uv.iter().all(|c| c.is_finite()) {
            return Err(format!("plane coordinates {uv:?} are not finite"));
        }
        let ((origin, normal), warnings) = self.plane_of(reference)?;
        // The SAME basis derivation the 2D sketch layer and the UI use
        // (`SketchPlaneBasis` mirrors `buildSketchPlane`), so a uv here means
        // what a uv means everywhere else.
        let basis = SketchPlaneBasis::from_origin_normal(origin, normal);
        Ok(AnchorHit {
            point: basis.local_to_world(uv[0], uv[1]),
            warnings,
        })
    }
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// Evaluate a 3D sketch against the rebuild's current state.
///
/// The typed [`Sketch3dError`](waffle_types::sketch3d::Sketch3dError) becomes
/// a per-feature engine error, so a bad fillet or a dangling attachment marks
/// THIS feature failed and names why, rather than surfacing later as a sweep
/// that cannot find its path.
pub fn evaluate(
    sketch: &Sketch3d,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
    tree: &FeatureTree,
) -> Result<Sketch3dEvaluation, EngineError> {
    let anchors = EngineAnchors {
        feature_results,
        introspect,
        tree,
    };
    sketch
        .evaluate(&anchors)
        .map_err(|e| EngineError::ResolutionFailed {
            reason: e.to_string(),
        })
}

/// The empty result a reference-geometry feature produces: no bodies, no
/// provenance. Mirrors the `MateConnector` and `DatumPlane` arms. The
/// evaluation's warnings ride in the diagnostics, which is how every
/// feature's warnings reach `Engine::warnings` (and are carried across a
/// partial rebuild that does not re-execute the feature).
pub fn no_geometry_result(ev: &Sketch3dEvaluation) -> OpResult {
    OpResult {
        outputs: Vec::new(),
        provenance: modeling_ops::Provenance {
            created: Vec::new(),
            deleted: Vec::new(),
            modified: Vec::new(),
            role_assignments: Vec::new(),
        },
        diagnostics: modeling_ops::Diagnostics {
            warnings: ev.warnings.clone(),
            ..Default::default()
        },
    }
}
