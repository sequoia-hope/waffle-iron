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
use waffle_types::sketch3d::{ExternalAnchors, Sketch3d, Sketch3dEvaluation};
use waffle_types::{GeomRef, SketchPlaneBasis, TopoKind};

use crate::types::{EngineError, FeatureTree};
use modeling_ops::OpResult;

/// Resolves the model-geometry attachments of a 3D sketch against the state
/// of the rebuild at the point the sketch is reached.
pub struct EngineAnchors<'a> {
    pub feature_results: &'a HashMap<Uuid, OpResult>,
    pub introspect: &'a dyn KernelIntrospect,
    pub tree: &'a FeatureTree,
}

impl EngineAnchors<'_> {
    fn kernel_id(&self, reference: &GeomRef) -> Option<waffle_types::kernel::KernelId> {
        // A `Position` selector is how a PICKED entity is named, and picking
        // is how a 3D sketch point attaches to the model — so it has to go
        // through the nearest-entity resolver, which is what lets the
        // attachment re-bind when the geometry moves. `resolve_geom_ref_live`
        // does not route it (it handles `Query`), and the plain resolver
        // refuses it outright.
        if let waffle_types::Selector::Position { x, y, z } = reference.selector {
            return crate::resolve::resolve_by_position(
                reference,
                self.feature_results,
                self.introspect,
                [x, y, z],
            )
            .ok()
            .map(|r| r.kernel_id);
        }
        crate::resolve::resolve_geom_ref_live(reference, self.feature_results, self.introspect)
            .ok()
            .map(|r| r.kernel_id)
    }

    /// `(origin, normal)` of a datum plane or a planar face.
    fn plane_of(&self, reference: &GeomRef) -> Option<([f64; 3], [f64; 3])> {
        match &reference.anchor {
            waffle_types::Anchor::Datum { datum_id } => crate::rebuild::resolve_datum_plane(
                *datum_id,
                self.tree,
                self.feature_results,
                self.introspect,
            )
            .ok(),
            waffle_types::Anchor::FeatureOutput { .. } => {
                crate::rebuild::resolve_face_plane(reference, self.feature_results, self.introspect)
                    .ok()
            }
        }
    }
}

impl ExternalAnchors for EngineAnchors<'_> {
    fn vertex(&self, reference: &GeomRef) -> Option<[f64; 3]> {
        if reference.kind != TopoKind::Vertex {
            return None;
        }
        let id = self.kernel_id(reference)?;
        // A vertex signature's centroid IS the vertex point, exactly
        // (`kernel_v2::adapter::vertex_signature`) — no averaging involved.
        self.introspect
            .compute_signature(id, TopoKind::Vertex)
            .centroid
    }

    fn edge_point(&self, reference: &GeomRef, t: f64) -> Option<[f64; 3]> {
        if reference.kind != TopoKind::Edge || !t.is_finite() || !(0.0..=1.0).contains(&t) {
            return None;
        }
        let id = self.kernel_id(reference)?;
        let poly = self.introspect.edge_polyline(id);
        if poly.len() < 2 {
            return None;
        }
        // Interpolate by ARC LENGTH along the polyline, so t is a uniform
        // parameter along the edge as drawn rather than along its sample
        // index — a curved edge's samples are not equally spaced.
        let seg: Vec<f64> = poly.windows(2).map(|w| dist(w[0], w[1])).collect();
        let total: f64 = seg.iter().sum();
        if !total.is_finite() || total <= 0.0 {
            return None;
        }
        let mut want = t * total;
        for (i, len) in seg.iter().enumerate() {
            if want <= *len || i + 1 == seg.len() {
                let f = if *len > 0.0 { want / len } else { 0.0 };
                let (a, b) = (poly[i], poly[i + 1]);
                return Some([
                    a[0] + (b[0] - a[0]) * f,
                    a[1] + (b[1] - a[1]) * f,
                    a[2] + (b[2] - a[2]) * f,
                ]);
            }
            want -= len;
        }
        poly.last().copied()
    }

    fn plane_point(&self, reference: &GeomRef, uv: [f64; 2]) -> Option<[f64; 3]> {
        if !uv.iter().all(|c| c.is_finite()) {
            return None;
        }
        let (origin, normal) = self.plane_of(reference)?;
        // The SAME basis derivation the 2D sketch layer and the UI use
        // (`SketchPlaneBasis` mirrors `buildSketchPlane`), so a uv here means
        // what a uv means everywhere else.
        let basis = SketchPlaneBasis::from_origin_normal(origin, normal);
        Some(basis.local_to_world(uv[0], uv[1]))
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
/// provenance. Mirrors the `MateConnector` and `DatumPlane` arms.
pub fn no_geometry_result() -> OpResult {
    OpResult {
        outputs: Vec::new(),
        provenance: modeling_ops::Provenance {
            created: Vec::new(),
            deleted: Vec::new(),
            modified: Vec::new(),
            role_assignments: Vec::new(),
        },
        diagnostics: modeling_ops::Diagnostics::default(),
    }
}
