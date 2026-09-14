//! Mate-connector frames (`specs/assembly_connector_frame_resolver.md`).
//!
//! An assembly's mate connector names a coordinate frame on an instance's
//! geometry ([`crate::assembly::MateConnector`]). This module derives that
//! frame from the part's CURRENT geometry, so a connector tracks the face or
//! edge it was placed on as the part rebuilds.
//!
//! Phase 3 derived it with [`crate::rebuild::resolve_face_plane`] — the
//! *datum-plane* resolver, which accepts planar faces only. That left the two
//! mates people actually reach for unauthorable (a Revolute wants a hole's
//! axis, a Slider a shaft's) and, worse, failed soft: a cylindrical pick was
//! refused at evaluation and silently replaced by the connector's default
//! frame, so the mate solved against geometry the user never picked.
//!
//! What a pick derives now:
//!
//! | pick | origin | z |
//! |---|---|---|
//! | planar face | face centroid | outward normal |
//! | cylindrical / conical / toroidal face | the point on the axis at the middle of the FACE's own axial extent — or at either end, by the connector's [`AxialAnchor`] | the surface axis |
//! | spherical face | the centre | the kernel's canonical sphere axis |
//! | circular / elliptical edge | the centre | the rim's outward sense (see [`rim_axis_sense`]) |
//! | straight edge | the midpoint | along the edge |
//! | anything else | — | a typed [`EngineError::ResolutionFailed`] naming the pick |
//!
//! The axial-extent rule is what makes a Revolute mate behave: a pin's
//! connector sits at the pin's mid-height and a hole's at the hole's
//! mid-depth, so "origins coincide, axes parallel" centres the pin in the
//! hole. It is a policy choice, which is why the kernel contract reports the
//! surface's own reference point ([`EntityAxis::origin`]) and leaves the
//! placement on that axis here — and why the connector can choose an end of
//! the extent instead (`specs/assembly_connector_adjustments.md`).
//!
//! There is deliberately no in-plane x: a cylinder has no canonical one.
//! [`crate::assembly::Frame::basis`] derives a stable x from z, and the
//! connector's `rotation_deg` (or the mate's) is the control — never a
//! fabricated direction. The connector's other adjustments (`flip_z`,
//! `rotation_deg`, `offset_m`) are applied by
//! [`crate::assembly::MateConnector::adjusted`] to what this module derives.

use std::collections::HashMap;

use modeling_ops::OpResult;
use uuid::Uuid;
use waffle_types::kernel::units::TAU_WORK;
use waffle_types::kernel::{AxisKind, EntityAxis, KernelId, KernelIntrospect};
use waffle_types::{GeomRef, TopoKind};

use crate::assembly::{adjust_frame, AxialAnchor, Frame};
use crate::resolve::resolve_with_fallback;
use crate::types::{EngineError, FeatureTree, MateConnectorParams, Operation};

/// A part's named mate connector (a `MateConnector` feature) as evaluated:
/// the frame in the PART's coordinates with every adjustment applied.
#[derive(Debug, Clone, PartialEq)]
pub struct PartConnector {
    pub feature_id: Uuid,
    /// The feature's name — the connector's name.
    pub name: String,
    pub frame: Frame,
    /// What the frame was derived from; `None` for an explicit frame.
    pub geometry: Option<ConnectorGeometry>,
}

/// A part mate connector's frame (`specs/part_mate_connectors.md`): derived
/// from `geom_ref` exactly as an assembly connector's is (or `frame` as
/// given), then flipped, turned and offset. Loud on a pick with no frame and
/// on a degenerate z — never a default frame in their place.
pub fn part_connector_frame(
    params: &MateConnectorParams,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
) -> Result<(Frame, Option<ConnectorGeometry>), EngineError> {
    // The anchor's ends are named against the FINAL z (see `AxialAnchor`).
    let anchor = if params.flip_z {
        params.anchor.mirrored()
    } else {
        params.anchor
    };
    let (base, geometry) = match &params.geom_ref {
        Some(geom_ref) => {
            let (mut frame, kind) =
                resolve_connector_frame(geom_ref, feature_results, introspect, anchor)?;
            if params.frame.x_axis != [0.0; 3] {
                frame.x_axis = params.frame.x_axis;
            }
            (frame, Some(kind))
        }
        None => (params.frame, None),
    };
    let frame = adjust_frame(base, params.flip_z, params.rotation_deg, params.offset_m)
        .and_then(|f| f.basis().map(|_| f))
        .map_err(|e| EngineError::ResolutionFailed {
            reason: format!("the mate connector's frame is degenerate: {e}"),
        })?;
    Ok((frame, geometry))
}

/// Every mate connector of a part that rebuilt: active, not suppressed, and
/// with a result (a failed connector is already in the rebuild's errors).
pub fn part_connectors(
    tree: &FeatureTree,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
) -> Vec<PartConnector> {
    tree.active_features()
        .iter()
        .filter(|f| !f.suppressed && feature_results.contains_key(&f.id))
        .filter_map(|f| {
            let Operation::MateConnector { params } = &f.operation else {
                return None;
            };
            let (frame, geometry) =
                part_connector_frame(params, feature_results, introspect).ok()?;
            Some(PartConnector {
                feature_id: f.id,
                name: f.name.clone(),
                frame,
                geometry,
            })
        })
        .collect()
}

/// What a connector's frame was derived from — reported so the UI can label
/// a connector by its source and a diagnostic can name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorGeometry {
    /// A planar face: centroid + outward normal.
    PlanarFace,
    /// A rotational face: the surface's axis (cylindrical, conical, toroidal).
    AxialFace(AxisKind),
    /// A spherical face: the centre (what a `Ball` mate wants).
    SphericalFace,
    /// A circular or elliptical edge: the centre + the rim's outward sense.
    CircularEdge(AxisKind),
    /// A straight edge: its midpoint, z along the edge.
    StraightEdge,
}

impl ConnectorGeometry {
    /// Short label for the UI (`"cylindrical face"`, `"circular edge"`, …).
    pub fn label(self) -> &'static str {
        match self {
            ConnectorGeometry::PlanarFace => "planar face",
            ConnectorGeometry::AxialFace(AxisKind::Cylindrical) => "cylindrical face",
            ConnectorGeometry::AxialFace(AxisKind::Conical) => "conical face",
            ConnectorGeometry::AxialFace(AxisKind::Toroidal) => "toroidal face",
            ConnectorGeometry::AxialFace(_) => "axial face",
            ConnectorGeometry::SphericalFace => "spherical face",
            ConnectorGeometry::CircularEdge(AxisKind::Elliptical) => "elliptical edge",
            ConnectorGeometry::CircularEdge(_) => "circular edge",
            ConnectorGeometry::StraightEdge => "straight edge",
        }
    }
}

/// Derive a connector's frame from the geometry its `GeomRef` names, in the
/// part's own coordinates. `anchor` is where on a rotational face's axis the
/// frame sits, measured along the DERIVED axis (a connector with `flip_z`
/// passes [`crate::assembly::MateConnector::derivation_anchor`]); it is
/// ignored by every other pick.
///
/// Loud (typed [`EngineError::ResolutionFailed`]) when the reference does not
/// resolve, or resolves to geometry with no derivable frame — an imported
/// body's mesh-backed face, a freeform surface, a curved edge with no
/// analytic axis. A caller must NOT substitute a default frame for such a
/// pick: that is the silent-wrong this resolver exists to remove.
pub fn resolve_connector_frame(
    geom_ref: &GeomRef,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
    anchor: AxialAnchor,
) -> Result<(Frame, ConnectorGeometry), EngineError> {
    let resolved = resolve_with_fallback(geom_ref, feature_results)?;
    let id = resolved.kernel_id;

    match geom_ref.kind {
        TopoKind::Face => face_frame(id, introspect, anchor),
        TopoKind::Edge => edge_frame(id, introspect),
        TopoKind::Vertex => Err(EngineError::ResolutionFailed {
            reason: "a vertex carries no direction, so it cannot define a connector frame; \
                     pick a face or an edge"
                .into(),
        }),
        other => Err(EngineError::ResolutionFailed {
            reason: format!("a connector is placed on a face or an edge, not a {other:?}"),
        }),
    }
}

fn face_frame(
    id: KernelId,
    introspect: &dyn KernelIntrospect,
    anchor: AxialAnchor,
) -> Result<(Frame, ConnectorGeometry), EngineError> {
    let sig = introspect.compute_signature(id, TopoKind::Face);

    // Planar first: the signature path is also how an IMPORTED planar face
    // (which has no arena surface) resolves.
    if sig.surface_type.as_deref() == Some("planar") {
        let (origin, normal) = planar_face_plane(id, introspect, "Connector face")?;
        return Ok((
            Frame::on_plane(origin, normal),
            ConnectorGeometry::PlanarFace,
        ));
    }

    let Some(axis) = introspect.entity_axis(id, TopoKind::Face) else {
        return Err(EngineError::ResolutionFailed {
            reason: format!(
                "a connector needs a planar face or one with an axis (cylindrical, conical, \
                 toroidal, spherical); this face is {} and carries no analytic axis",
                sig.surface_type.as_deref().unwrap_or("of an unknown type")
            ),
        });
    };
    let direction = unit(axis.direction).ok_or_else(|| EngineError::ResolutionFailed {
        reason: format!("the {} face's axis is zero-length", axis.kind.label()),
    })?;

    match axis.kind {
        // Isotropic: the centre IS the frame origin; there is no extent to
        // take the middle of.
        AxisKind::Spherical => Ok((
            Frame {
                origin: axis.origin,
                z_axis: direction,
                x_axis: [0.0; 3],
            },
            ConnectorGeometry::SphericalFace,
        )),
        kind => Ok((
            Frame {
                origin: axial_extent_point(id, &axis, direction, introspect, anchor),
                z_axis: direction,
                x_axis: [0.0; 3],
            },
            ConnectorGeometry::AxialFace(kind),
        )),
    }
}

fn edge_frame(
    id: KernelId,
    introspect: &dyn KernelIntrospect,
) -> Result<(Frame, ConnectorGeometry), EngineError> {
    if let Some(axis) = introspect.entity_axis(id, TopoKind::Edge) {
        let direction = unit(axis.direction).ok_or_else(|| EngineError::ResolutionFailed {
            reason: format!("the {} edge's axis is zero-length", axis.kind.label()),
        })?;
        return Ok((
            Frame {
                origin: axis.origin,
                z_axis: rim_axis_sense(id, direction, introspect),
                x_axis: [0.0; 3],
            },
            ConnectorGeometry::CircularEdge(axis.kind),
        ));
    }

    // No analytic axis. A STRAIGHT edge still defines a frame (midpoint, z
    // along the edge); the kernel contract makes a two-point polyline the
    // discriminator — every curved edge is sampled at render density.
    let polyline = introspect.edge_polyline(id);
    if polyline.len() != 2 {
        return Err(EngineError::ResolutionFailed {
            reason: if polyline.is_empty() {
                "the edge has no geometry to derive a connector frame from".into()
            } else {
                "this edge is curved but carries no analytic axis (it is not a circle, an arc \
                 or an ellipse), so it cannot define a connector frame"
                    .into()
            },
        });
    }
    let (a, b) = (polyline[0], polyline[1]);
    let direction = unit([b[0] - a[0], b[1] - a[1], b[2] - a[2]]).ok_or_else(|| {
        EngineError::ResolutionFailed {
            reason: "the edge is degenerate (its endpoints coincide)".into(),
        }
    })?;
    Ok((
        Frame {
            origin: [
                (a[0] + b[0]) / 2.0,
                (a[1] + b[1]) / 2.0,
                (a[2] + b[2]) / 2.0,
            ],
            z_axis: direction,
            x_axis: [0.0; 3],
        },
        ConnectorGeometry::StraightEdge,
    ))
}

/// The point on `axis` at the middle of the face's own axial extent — or at
/// the end `direction` points toward / away from, by `anchor`: every
/// boundary point of the face (its edges at render density) projected onto
/// the axis, then the extremes or halfway between them.
///
/// So a drilled hole's connector sits at mid-depth and a shaft's at
/// mid-height — the axis point a Revolute or Cylindrical mate should bring
/// together — unless the connector asks for a rim. Falls back to the
/// surface's own reference point when the face reports no boundary geometry
/// (never a wrong answer, just an unrefined one).
fn axial_extent_point(
    face: KernelId,
    axis: &EntityAxis,
    direction: [f64; 3],
    introspect: &dyn KernelIntrospect,
    anchor: AxialAnchor,
) -> [f64; 3] {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for edge in introspect.face_edges(face) {
        for p in introspect.edge_polyline(edge) {
            let d = [
                p[0] - axis.origin[0],
                p[1] - axis.origin[1],
                p[2] - axis.origin[2],
            ];
            let t = d[0] * direction[0] + d[1] * direction[1] + d[2] * direction[2];
            lo = lo.min(t);
            hi = hi.max(t);
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        return axis.origin;
    }
    let t = match anchor {
        AxialAnchor::Middle => (lo + hi) / 2.0,
        AxialAnchor::PositiveEnd => hi,
        AxialAnchor::NegativeEnd => lo,
    };
    [
        axis.origin[0] + direction[0] * t,
        axis.origin[1] + direction[1] * t,
        axis.origin[2] + direction[2] * t,
    ]
}

/// Which way a rim's frame should point.
///
/// A circle's stored normal is the direction the half-edge traverses CCW
/// around — deterministic, but which of the two half-edges a reference
/// resolves to is an implementation detail. When the rim bounds exactly one
/// PLANAR face, that face's outward normal is the meaningful sense: the rim
/// of a hole in the top face points out of the top face, and a shaft's end
/// rim points out of its cap. With no such face (a rim between two curved
/// faces) or more than one, the curve's own normal stands.
fn rim_axis_sense(
    edge: KernelId,
    direction: [f64; 3],
    introspect: &dyn KernelIntrospect,
) -> [f64; 3] {
    let planar: Vec<[f64; 3]> = introspect
        .edge_faces(edge)
        .into_iter()
        .filter_map(|f| {
            let sig = introspect.compute_signature(f, TopoKind::Face);
            if sig.surface_type.as_deref() != Some("planar") {
                return None;
            }
            sig.normal.and_then(unit)
        })
        .collect();
    match planar.as_slice() {
        [n] => *n,
        _ => direction,
    }
}

/// The `(origin, normal)` of a PLANAR face from its signature: a point on the
/// face (its centroid) and the outward normal.
///
/// Shared by the connector resolver and [`crate::rebuild::resolve_face_plane`]
/// (the datum-plane resolver) so both read planarity the same way; `what`
/// names the caller's subject in the error messages.
pub(crate) fn planar_face_plane(
    id: KernelId,
    introspect: &dyn KernelIntrospect,
    what: &str,
) -> Result<([f64; 3], [f64; 3]), EngineError> {
    let sig = introspect.compute_signature(id, TopoKind::Face);
    if sig.surface_type.as_deref() != Some("planar") {
        return Err(EngineError::ResolutionFailed {
            reason: format!(
                "{what} is not planar (surface_type: {})",
                sig.surface_type.as_deref().unwrap_or("unknown")
            ),
        });
    }
    let normal = sig.normal.ok_or_else(|| EngineError::ResolutionFailed {
        reason: format!("{what} has no normal"),
    })?;
    let origin = sig.centroid.ok_or_else(|| EngineError::ResolutionFailed {
        reason: format!("{what} has no centroid"),
    })?;
    let normal = unit(normal).ok_or_else(|| EngineError::ResolutionFailed {
        reason: format!("{what} normal is zero-length"),
    })?;
    Ok((origin, normal))
}

/// Normalize, or `None` when the vector is shorter than the working
/// tolerance ([`TAU_WORK`]).
fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (n >= TAU_WORK).then(|| [v[0] / n, v[1] / n, v[2] / n])
}
