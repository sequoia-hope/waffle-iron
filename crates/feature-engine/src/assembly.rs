//! Assemblies (`specs/waffle_v4_document_model.md` §9 Phase 3,
//! `projects/10-assemblies/`): an `Assembly` tab places **instances** of parts
//! — a Part tab of the same document, or a tab of a linked `.waffle` source —
//! by rigid transforms, and relates them through **mate connectors** (a
//! coordinate frame on an instance's geometry) joined by **mates**. Phase 3
//! lands the `Fastened` mate: two connector frames are made coincident (up to
//! a flip and a rotation about the connector's z axis), so a chain of
//! fastened parts is placed by rigid-transform composition from the grounded
//! instances — no numeric solver. Other mate kinds are reserved: an unknown
//! kind is preserved opaquely (like `Operation::Unknown`) and reported.
//!
//! Solved placements are **derived hints** (`AssemblyTree::placements`):
//! recomputed on every evaluation, persisted so a reader without the engine
//! (a thumbnail, a script) can position instances, never authoritative.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;
use waffle_types::GeomRef;

// ------------------------------------------------------------------ transform

/// A rigid transform `p' = R p + t` with `R` as a unit quaternion
/// `[x, y, z, w]` (the JS/three.js order). Lengths in meters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Transform {
    #[serde(default)]
    pub translation_m: [f64; 3],
    #[serde(default = "identity_quat")]
    pub rotation_quat: [f64; 4],
}

fn identity_quat() -> [f64; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

impl Default for Transform {
    fn default() -> Self {
        Transform::identity()
    }
}

fn normalize3(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (n > 1e-12).then(|| [v[0] / n, v[1] / n, v[2] / n])
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Hamilton product `a * b` (apply `b` first, then `a`).
pub fn quat_mul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let (ax, ay, az, aw) = (a[0], a[1], a[2], a[3]);
    let (bx, by, bz, bw) = (b[0], b[1], b[2], b[3]);
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

pub fn quat_conj(q: [f64; 4]) -> [f64; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}

pub fn quat_normalize(q: [f64; 4]) -> [f64; 4] {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n < 1e-12 {
        identity_quat()
    } else {
        [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
    }
}

/// Rotate `v` by the unit quaternion `q`.
pub fn quat_rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let u = [q[0], q[1], q[2]];
    let s = q[3];
    let uv = cross(u, v);
    let uuv = cross(u, uv);
    [
        v[0] + 2.0 * (s * uv[0] + uuv[0]),
        v[1] + 2.0 * (s * uv[1] + uuv[1]),
        v[2] + 2.0 * (s * uv[2] + uuv[2]),
    ]
}

/// Quaternion for a rotation of `angle_deg` about the unit `axis`.
pub fn quat_axis_angle(axis: [f64; 3], angle_deg: f64) -> [f64; 4] {
    let a = normalize3(axis).unwrap_or([0.0, 0.0, 1.0]);
    let h = angle_deg.to_radians() / 2.0;
    let s = h.sin();
    quat_normalize([a[0] * s, a[1] * s, a[2] * s, h.cos()])
}

/// Quaternion `exp(θ)` for a rotation vector `θ` (axis × angle, radians).
pub fn quat_exp(theta: [f64; 3]) -> [f64; 4] {
    let angle = (theta[0] * theta[0] + theta[1] * theta[1] + theta[2] * theta[2]).sqrt();
    if angle < 1e-12 {
        return quat_normalize([theta[0] / 2.0, theta[1] / 2.0, theta[2] / 2.0, 1.0]);
    }
    let s = (angle / 2.0).sin() / angle;
    [
        theta[0] * s,
        theta[1] * s,
        theta[2] * s,
        (angle / 2.0).cos(),
    ]
}

/// Rotation vector (axis × angle, the short arc) of a unit quaternion.
pub fn quat_log(q: [f64; 4]) -> [f64; 3] {
    let q = if q[3] < 0.0 {
        [-q[0], -q[1], -q[2], -q[3]]
    } else {
        q
    };
    let vn = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
    if vn < 1e-12 {
        return [2.0 * q[0], 2.0 * q[1], 2.0 * q[2]];
    }
    let angle = 2.0 * vn.atan2(q[3]);
    [q[0] / vn * angle, q[1] / vn * angle, q[2] / vn * angle]
}

/// Quaternion from an orthonormal basis given as columns (x, y, z).
pub fn quat_from_basis(x: [f64; 3], y: [f64; 3], z: [f64; 3]) -> [f64; 4] {
    // Shepperd's method on the column-major rotation matrix.
    let m = [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]];
    let trace = m[0][0] + m[1][1] + m[2][2];
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
            0.25 * s,
        ]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[2][1] - m[1][2]) / s,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
            (m[0][2] - m[2][0]) / s,
        ]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            0.25 * s,
            (m[1][0] - m[0][1]) / s,
        ]
    };
    quat_normalize(q)
}

impl Transform {
    pub fn identity() -> Self {
        Transform {
            translation_m: [0.0; 3],
            rotation_quat: identity_quat(),
        }
    }

    pub fn translation(t: [f64; 3]) -> Self {
        Transform {
            translation_m: t,
            rotation_quat: identity_quat(),
        }
    }

    pub fn from_rotation(q: [f64; 4]) -> Self {
        Transform {
            translation_m: [0.0; 3],
            rotation_quat: quat_normalize(q),
        }
    }

    /// Apply to a point.
    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let r = quat_rotate(self.rotation_quat, p);
        [
            r[0] + self.translation_m[0],
            r[1] + self.translation_m[1],
            r[2] + self.translation_m[2],
        ]
    }

    /// Apply the rotation only (directions).
    pub fn apply_dir(&self, v: [f64; 3]) -> [f64; 3] {
        quat_rotate(self.rotation_quat, v)
    }

    /// `self ∘ inner`: apply `inner` first, then `self`.
    pub fn compose(&self, inner: &Transform) -> Transform {
        Transform {
            translation_m: self.apply(inner.translation_m),
            rotation_quat: quat_normalize(quat_mul(self.rotation_quat, inner.rotation_quat)),
        }
    }

    pub fn inverse(&self) -> Transform {
        let rinv = quat_conj(self.rotation_quat);
        let t = quat_rotate(rinv, self.translation_m);
        Transform {
            translation_m: [-t[0], -t[1], -t[2]],
            rotation_quat: rinv,
        }
    }

    /// Whether two transforms agree within `tol` meters (translation) and
    /// the equivalent angular tolerance (rotation).
    pub fn approx_eq(&self, other: &Transform, tol: f64) -> bool {
        let dt: f64 = (0..3)
            .map(|i| (self.translation_m[i] - other.translation_m[i]).abs())
            .fold(0.0, f64::max);
        // |q·q'| = 1 for equal rotations (q and -q are the same rotation).
        let d = (self.rotation_quat[0] * other.rotation_quat[0]
            + self.rotation_quat[1] * other.rotation_quat[1]
            + self.rotation_quat[2] * other.rotation_quat[2]
            + self.rotation_quat[3] * other.rotation_quat[3])
            .abs();
        dt <= tol && (1.0 - d) <= 1e-9 + tol
    }

    /// Column-major 4×4 matrix (three.js `Matrix4.fromArray` order).
    pub fn to_matrix(&self) -> [f64; 16] {
        let x = self.apply_dir([1.0, 0.0, 0.0]);
        let y = self.apply_dir([0.0, 1.0, 0.0]);
        let z = self.apply_dir([0.0, 0.0, 1.0]);
        let t = self.translation_m;
        [
            x[0], x[1], x[2], 0.0, y[0], y[1], y[2], 0.0, z[0], z[1], z[2], 0.0, t[0], t[1], t[2],
            1.0,
        ]
    }
}

// ---------------------------------------------------------------------- frame

/// A coordinate frame on part geometry: origin, primary (z) axis and
/// secondary (x) axis, in the PART's coordinates. `x_axis` is orthogonalized
/// against `z_axis`; a zero `x_axis` means "any perpendicular" (chosen
/// deterministically).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Frame {
    #[serde(default)]
    pub origin: [f64; 3],
    #[serde(default = "default_z")]
    pub z_axis: [f64; 3],
    #[serde(default)]
    pub x_axis: [f64; 3],
}

fn default_z() -> [f64; 3] {
    [0.0, 0.0, 1.0]
}

/// An orthonormal basis `(x, y, z)`.
pub type Basis = ([f64; 3], [f64; 3], [f64; 3]);

impl Default for Frame {
    fn default() -> Self {
        Frame {
            origin: [0.0; 3],
            z_axis: default_z(),
            x_axis: [0.0; 3],
        }
    }
}

impl Frame {
    /// A frame on a planar face: origin at a point of the face, z along the
    /// outward normal, x chosen deterministically (no explicit direction).
    pub fn on_plane(origin: [f64; 3], normal: [f64; 3]) -> Self {
        Frame {
            origin,
            z_axis: normal,
            x_axis: [0.0; 3],
        }
    }

    /// Orthonormal basis (x, y, z). Errors when the z axis is degenerate.
    pub fn basis(&self) -> Result<Basis, String> {
        let z = normalize3(self.z_axis).ok_or("frame z axis is zero")?;
        let mut x = self.x_axis;
        if normalize3(x).is_none() {
            // The world axis least aligned with z, so the choice is stable.
            let cands = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
            x = cands
                .into_iter()
                .min_by(|a, b| dot(*a, z).abs().partial_cmp(&dot(*b, z).abs()).unwrap())
                .unwrap();
        }
        let d = dot(x, z);
        let x = normalize3([x[0] - d * z[0], x[1] - d * z[1], x[2] - d * z[2]])
            .ok_or("frame x axis is parallel to z")?;
        let y = cross(z, x);
        Ok((x, y, z))
    }

    /// The rigid transform taking frame coordinates to part coordinates.
    pub fn to_transform(&self) -> Result<Transform, String> {
        let (x, y, z) = self.basis()?;
        Ok(Transform {
            translation_m: self.origin,
            rotation_quat: quat_from_basis(x, y, z),
        })
    }

    /// This frame expressed in another coordinate system.
    pub fn transformed(&self, t: &Transform) -> Frame {
        Frame {
            origin: t.apply(self.origin),
            z_axis: t.apply_dir(self.z_axis),
            x_axis: t.apply_dir(self.x_axis),
        }
    }

    /// The frame with its z axis reversed: a turn of 180° about x, so x
    /// stays and y reverses with z (the basis stays right-handed). A zero
    /// `x_axis` stays zero — [`Frame::basis`] picks the same world axis for
    /// z and −z, so the derived x is the same either way.
    pub fn flipped(&self) -> Frame {
        Frame {
            origin: self.origin,
            z_axis: [-self.z_axis[0], -self.z_axis[1], -self.z_axis[2]],
            x_axis: self.x_axis,
        }
    }

    /// The frame turned by `deg` about its own z axis. The secondary axis
    /// becomes explicit (the turned x), so the turn survives `basis`'s
    /// deterministic choice. Errors when z is degenerate; a zero turn is the
    /// frame unchanged.
    pub fn rotated_about_z(&self, deg: f64) -> Result<Frame, String> {
        if deg == 0.0 {
            return Ok(*self);
        }
        let (x, y, _) = self.basis()?;
        let (s, c) = deg.to_radians().sin_cos();
        Ok(Frame {
            origin: self.origin,
            z_axis: self.z_axis,
            x_axis: [
                c * x[0] + s * y[0],
                c * x[1] + s * y[1],
                c * x[2] + s * y[2],
            ],
        })
    }

    /// The frame moved along its OWN axes: `offset[0]` along x, `[1]` along
    /// y, `[2]` along z (meters). Errors when z is degenerate; a zero offset
    /// is the frame unchanged.
    pub fn offset_along_axes(&self, offset: [f64; 3]) -> Result<Frame, String> {
        if offset == [0.0; 3] {
            return Ok(*self);
        }
        let (x, y, z) = self.basis()?;
        let mut origin = self.origin;
        for k in 0..3 {
            origin[k] += offset[0] * x[k] + offset[1] * y[k] + offset[2] * z[k];
        }
        Ok(Frame { origin, ..*self })
    }
}

/// A connector's adjustments applied to a frame, in this order: `flip_z`,
/// then `rotation_deg` about z, then `offset` along the resulting axes —
/// every step in the frame's own coordinates. Shared by assembly connectors
/// ([`MateConnector::adjusted`]) and part connectors
/// ([`crate::connector::part_connector_frame`]). Errors when z is degenerate.
pub fn adjust_frame(
    frame: Frame,
    flip_z: bool,
    rotation_deg: f64,
    offset: [f64; 3],
) -> Result<Frame, String> {
    let frame = if flip_z { frame.flipped() } else { frame };
    frame
        .rotated_about_z(rotation_deg)?
        .offset_along_axes(offset)
}

// -------------------------------------------------------------------- model

/// Which part an instance is of: a Part tab of this document
/// (`source_id: None`) or of a linked `.waffle` source (v4 §2.3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PartRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<Uuid>,
    pub tab_id: String,
}

/// One placed occurrence of a part.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Instance {
    pub id: Uuid,
    pub name: String,
    pub source: PartRef,
    /// Explicit placement: authoritative for a grounded (`fixed`) instance
    /// and for one no mate reaches; the starting point otherwise.
    #[serde(default)]
    pub transform: Transform,
    /// Grounded: never moved by mates. The first non-suppressed instance is
    /// implicitly grounded when none is marked.
    #[serde(default, skip_serializing_if = "is_false")]
    pub fixed: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub suppressed: bool,
    /// Identity in an external tool (a KiCad footprint UUID, Phase 3b).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_key: Option<String>,
    /// Design-parameter overrides for this instance (reserved; not applied
    /// in Phase 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter_overrides: Option<BTreeMap<String, f64>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

pub(crate) fn is_false(b: &bool) -> bool {
    !*b
}

pub(crate) fn is_zero3(v: &[f64; 3]) -> bool {
    *v == [0.0; 3]
}

/// Where along a rotational face's axis a derived connector sits
/// (`crate::connector`, `specs/assembly_connector_adjustments.md`): the
/// middle of the face's axial extent (the default — what a Revolute or
/// Cylindrical mate wants) or one of its ends. The ends are named by the
/// connector's FINAL z (after `flip_z`), so they always read against the
/// triad the viewport draws: "+z end" is wherever the blue arrow points.
/// Ignored by every other pick (a planar face, a sphere, an edge, an
/// explicit frame).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum AxialAnchor {
    #[default]
    Middle,
    /// The end of the face's extent that z points toward.
    PositiveEnd,
    /// The end of the face's extent that z points away from.
    NegativeEnd,
}

impl AxialAnchor {
    /// The same end seen along the reversed axis.
    pub fn mirrored(self) -> Self {
        match self {
            AxialAnchor::Middle => AxialAnchor::Middle,
            AxialAnchor::PositiveEnd => AxialAnchor::NegativeEnd,
            AxialAnchor::NegativeEnd => AxialAnchor::PositiveEnd,
        }
    }

    pub(crate) fn is_middle(&self) -> bool {
        *self == AxialAnchor::Middle
    }
}

/// A named frame on an instance's geometry. `geom_ref` (a face or an edge
/// of the part, in the part's own feature space) derives the frame from the
/// current geometry at evaluation time (`crate::connector`) — with
/// `frame.x_axis` as the secondary direction when set; without a `geom_ref`,
/// `frame` is the frame. Either way the connector's adjustments then apply
/// ([`MateConnector::adjusted`]): `anchor` chooses the point on a rotational
/// face's axis, `flip_z` reverses z, `rotation_deg` turns about z, and
/// `offset_m` moves along the resulting axes. All four default to "as
/// derived", so a connector without them is exactly what it was before they
/// existed (additive, no reader-floor bump).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct MateConnector {
    pub id: Uuid,
    pub name: String,
    /// Scope (v4 §2.8): the chain of instance ids from this assembly down to
    /// the owning instance — `[instance]` for a part instance, `[instance,
    /// member, …]` for a member of a sub-assembly instance (3d-2). The mate
    /// solver moves the top-level instance; the member's relative placement
    /// is composed into the frame at evaluation.
    pub instance_path: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geom_ref: Option<GeomRef>,
    /// A named mate connector of the instance's PART (the id of its
    /// `MateConnector` feature, `specs/part_mate_connectors.md`): the frame
    /// is that connector's, as the part evaluates it. Takes precedence over
    /// `geom_ref` and `frame`; this connector's own adjustments still apply
    /// on top. A part connector that is gone or failed is a loud error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_connector: Option<Uuid>,
    #[serde(default)]
    pub frame: Frame,
    /// Where on a rotational face's axis the derived frame sits (default:
    /// the middle of the face's extent). See [`AxialAnchor`].
    #[serde(default, skip_serializing_if = "AxialAnchor::is_middle")]
    pub anchor: AxialAnchor,
    /// Reverse the frame's z axis (a 180° turn about x, so the basis stays
    /// right-handed). The first adjustment applied.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_z: bool,
    /// Turn about the frame's z, in degrees, after `flip_z`. What this moves
    /// is the secondary (x) axis — a Fastened mate's in-plane alignment.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation_deg: f64,
    /// Move along the frame's OWN axes after the turn, meters: `[x, y, z]`.
    #[serde(default, skip_serializing_if = "is_zero3")]
    pub offset_m: [f64; 3],
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl MateConnector {
    /// The anchor the resolver applies along the DERIVED axis. The ends are
    /// named against the connector's final z, so with `flip_z` they swap.
    pub fn derivation_anchor(&self) -> AxialAnchor {
        if self.flip_z {
            self.anchor.mirrored()
        } else {
            self.anchor
        }
    }

    /// This connector's adjustments applied to a frame (the one derived from
    /// its geometry, or its explicit `frame`), in this order: `flip_z`, then
    /// `rotation_deg` about z, then `offset_m` along the resulting axes.
    /// Every step is in the frame's own coordinates, so what a user types
    /// reads against the triad the viewport draws. Errors when the frame's z
    /// is degenerate (the caller reports it; nothing is substituted).
    pub fn adjusted(&self, frame: Frame) -> Result<Frame, String> {
        adjust_frame(frame, self.flip_z, self.rotation_deg, self.offset_m)
    }

    /// The owning instance when the connector is on a direct part instance
    /// (a one-element path).
    pub fn instance_id(&self) -> Option<Uuid> {
        (self.instance_path.len() == 1).then(|| self.instance_path[0])
    }

    /// The TOP-LEVEL instance of this assembly the connector belongs to —
    /// the instance the mate solver moves. For a connector on a sub-assembly
    /// member (`[top, sub, …]`) that is `top`; the evaluator composes the
    /// member's relative placement into the frame.
    pub fn top_instance_id(&self) -> Option<Uuid> {
        self.instance_path.first().copied()
    }
}

/// How two connectors relate (`flip` on every kind: connector b's z axis
/// opposes a's instead of aligning with it — two outward face normals
/// "facing"). `Fastened` removes all six degrees of freedom and is solved
/// exactly by composition; the others are solved numerically
/// (`crate::assembly_solver`) from the instances' current poses, which is
/// what fixes the free degrees of freedom:
///
/// | kind | frees | equations |
/// |---|---|---|
/// | `Fastened` | — | origins coincide, frames aligned (after `rotation_deg` about z) |
/// | `Revolute` | rotation about z | origins coincide, z axes parallel |
/// | `Slider` | translation along z | frames aligned, b's origin on a's z axis |
/// | `Cylindrical` | rotation about + translation along z | z axes parallel, b's origin on a's z axis |
/// | `Planar` | translation in xy + rotation about z | z axes parallel, b's origin in a's xy plane |
/// | `Ball` | all rotation | origins coincide |
///
/// Unknown kinds are preserved verbatim and reported.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum MateKind {
    Fastened {
        #[serde(default, skip_serializing_if = "is_false")]
        flip: bool,
        #[serde(default, skip_serializing_if = "is_zero")]
        rotation_deg: f64,
    },
    Revolute {
        #[serde(default, skip_serializing_if = "is_false")]
        flip: bool,
    },
    Slider {
        #[serde(default, skip_serializing_if = "is_false")]
        flip: bool,
    },
    Cylindrical {
        #[serde(default, skip_serializing_if = "is_false")]
        flip: bool,
    },
    Planar {
        #[serde(default, skip_serializing_if = "is_false")]
        flip: bool,
    },
    Ball,
    #[serde(untagged)]
    Unknown(Value),
}

pub(crate) fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
enum KnownMateKind {
    Fastened {
        #[serde(default)]
        flip: bool,
        #[serde(default)]
        rotation_deg: f64,
    },
    Revolute {
        #[serde(default)]
        flip: bool,
    },
    Slider {
        #[serde(default)]
        flip: bool,
    },
    Cylindrical {
        #[serde(default)]
        flip: bool,
    },
    Planar {
        #[serde(default)]
        flip: bool,
    },
    Ball,
}

/// The mate kinds this build can solve.
pub const MATE_KIND_TAGS: &[&str] = &[
    "Fastened",
    "Revolute",
    "Slider",
    "Cylindrical",
    "Planar",
    "Ball",
];

impl<'de> Deserialize<'de> for MateKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(
            match crate::opaque::known_or_unknown::<D, KnownMateKind>(
                d,
                MATE_KIND_TAGS,
                "mate kind",
            )? {
                Ok(KnownMateKind::Fastened { flip, rotation_deg }) => {
                    MateKind::Fastened { flip, rotation_deg }
                }
                Ok(KnownMateKind::Revolute { flip }) => MateKind::Revolute { flip },
                Ok(KnownMateKind::Slider { flip }) => MateKind::Slider { flip },
                Ok(KnownMateKind::Cylindrical { flip }) => MateKind::Cylindrical { flip },
                Ok(KnownMateKind::Planar { flip }) => MateKind::Planar { flip },
                Ok(KnownMateKind::Ball) => MateKind::Ball,
                Err(v) => MateKind::Unknown(v),
            },
        )
    }
}

impl MateKind {
    pub fn type_tag(&self) -> &str {
        match self {
            MateKind::Fastened { .. } => "Fastened",
            MateKind::Revolute { .. } => "Revolute",
            MateKind::Slider { .. } => "Slider",
            MateKind::Cylindrical { .. } => "Cylindrical",
            MateKind::Planar { .. } => "Planar",
            MateKind::Ball => "Ball",
            MateKind::Unknown(v) => crate::opaque::type_tag(v),
        }
    }

    /// Whether the z axes are made to oppose (`flip`) rather than align.
    pub fn flip(&self) -> bool {
        match self {
            MateKind::Fastened { flip, .. }
            | MateKind::Revolute { flip }
            | MateKind::Slider { flip }
            | MateKind::Cylindrical { flip }
            | MateKind::Planar { flip } => *flip,
            MateKind::Ball | MateKind::Unknown(_) => false,
        }
    }

    /// Solved exactly by rigid-transform composition (`solve_fastened`).
    pub fn is_fastened(&self) -> bool {
        matches!(self, MateKind::Fastened { .. })
    }

    /// Solved numerically (`crate::assembly_solver`).
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            MateKind::Revolute { .. }
                | MateKind::Slider { .. }
                | MateKind::Cylindrical { .. }
                | MateKind::Planar { .. }
                | MateKind::Ball
        )
    }
}

#[cfg(feature = "json-schema")]
impl schemars::JsonSchema for MateKind {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "MateKind".into()
    }
    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = <KnownMateKind as schemars::JsonSchema>::json_schema(g);
        let obj = schema.as_object_mut().expect("object schema");
        obj.insert(
            "description".into(),
            Value::String(
                "How two mate connectors relate: Fastened (exact), Revolute, Slider, Cylindrical, \
                 Planar, Ball (numeric). Any other well-formed object with a string `type` (a mate \
                 kind from a newer build) is preserved verbatim and reported."
                    .into(),
            ),
        );
        obj.get_mut("oneOf")
            .and_then(Value::as_array_mut)
            .expect("tagged enum renders as oneOf")
            .push(serde_json::json!({
                "type": "object",
                "description": "Unknown mate kind (opaque, preserved; not solved).",
                "required": ["type"],
                "properties": { "type": { "type": "string", "not": { "enum": MATE_KIND_TAGS } } }
            }));
        schema
    }
}

/// A relation between two connectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Mate {
    pub id: Uuid,
    pub name: String,
    pub kind: MateKind,
    /// `[a, b]`: `b`'s instance is placed against `a`'s when only one side
    /// is placed; either direction works.
    pub connectors: [Uuid; 2],
    #[serde(default, skip_serializing_if = "is_false")]
    pub suppressed: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The content of an `Assembly` tab.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct AssemblyTree {
    #[serde(default)]
    pub instances: Vec<Instance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connectors: Vec<MateConnector>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mates: Vec<Mate>,
    /// Solved placements by instance id — derived hints (see module doc).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub placements: BTreeMap<Uuid, Transform>,
    /// Unknown keys preserved across load → save (v4 §2.6).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl AssemblyTree {
    pub fn instance(&self, id: Uuid) -> Option<&Instance> {
        self.instances.iter().find(|i| i.id == id)
    }

    pub fn connector(&self, id: Uuid) -> Option<&MateConnector> {
        self.connectors.iter().find(|c| c.id == id)
    }

    /// Distinct parts referenced by non-suppressed instances.
    pub fn parts(&self) -> Vec<PartRef> {
        let mut seen = HashSet::new();
        self.instances
            .iter()
            .filter(|i| !i.suppressed)
            .filter(|i| seen.insert(i.source.clone()))
            .map(|i| i.source.clone())
            .collect()
    }

    /// Structural problems, as loader warnings (never a load failure).
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut ids = HashSet::new();
        for i in &self.instances {
            if !ids.insert(i.id) {
                out.push(format!("instance `{}` ({}): duplicate id", i.name, i.id));
            }
        }
        let mut cids = HashSet::new();
        for c in &self.connectors {
            if !cids.insert(c.id) {
                out.push(format!("connector `{}` ({}): duplicate id", c.name, c.id));
            }
            match c.instance_path.first() {
                Some(top) if self.instance(*top).is_some() => {}
                Some(top) => out.push(format!(
                    "connector `{}` ({}): instance {top} does not exist",
                    c.name, c.id
                )),
                None => out.push(format!("connector `{}` ({}): no instance", c.name, c.id)),
            }
        }
        let mut mids = HashSet::new();
        for m in &self.mates {
            if !mids.insert(m.id) {
                out.push(format!("mate `{}` ({}): duplicate id", m.name, m.id));
            }
            for cid in m.connectors {
                if self.connector(cid).is_none() {
                    out.push(format!(
                        "mate `{}` ({}): connector {cid} does not exist",
                        m.name, m.id
                    ));
                }
            }
            if let MateKind::Unknown(_) = &m.kind {
                out.push(format!(
                    "mate `{}` ({}): unknown mate kind `{}` — preserved, not solved in this version",
                    m.name,
                    m.id,
                    m.kind.type_tag()
                ));
            }
            if let (Some(a), Some(b)) = (
                self.connector(m.connectors[0]),
                self.connector(m.connectors[1]),
            ) {
                if a.top_instance_id().is_some() && a.top_instance_id() == b.top_instance_id() {
                    out.push(format!(
                        "mate `{}` ({}): both connectors are on the same instance",
                        m.name, m.id
                    ));
                }
            }
        }
        out
    }
}

// ------------------------------------------------------------------- solving

/// Result of placing instances from mates.
#[derive(Debug, Clone, Default)]
pub struct SolveResult {
    /// Placement of every non-suppressed instance.
    pub placements: BTreeMap<Uuid, Transform>,
    /// Loud problems (over-constrained mates, unusable frames, missing
    /// connectors); the placements are still the best consistent set.
    pub errors: Vec<String>,
    /// Instances no mate grounds (their explicit transform was used).
    pub warnings: Vec<String>,
}

/// The transform applied to connector `a`'s frame that connector `b`'s frame
/// must coincide with: a rotation about z, then the optional flip.
fn mate_offset(kind: &MateKind) -> Option<Transform> {
    match kind {
        MateKind::Fastened { flip, rotation_deg } => {
            let rz = Transform::from_rotation(quat_axis_angle([0.0, 0.0, 1.0], *rotation_deg));
            let fx = if *flip {
                Transform::from_rotation(quat_axis_angle([1.0, 0.0, 0.0], 180.0))
            } else {
                Transform::identity()
            };
            Some(rz.compose(&fx))
        }
        _ => None,
    }
}

/// Place the instances of `tree` by its `Fastened` mates. `frames` gives
/// each connector's frame in its PART's coordinates (geometry-derived by the
/// caller when the connector has a `geom_ref`, else the connector's own
/// `frame`). Grounded instances keep their explicit transform; every other
/// instance reached through a chain of mates is placed by rigid-transform
/// composition; an instance no mate reaches keeps its own transform (warning).
/// A mate whose both instances are already placed must agree within
/// `tol_m`, else it is an error (over-constrained) and ignored.
pub fn solve_fastened(
    tree: &AssemblyTree,
    frames: &HashMap<Uuid, Frame>,
    tol_m: f64,
) -> SolveResult {
    let mut result = SolveResult::default();
    let live: Vec<&Instance> = tree.instances.iter().filter(|i| !i.suppressed).collect();
    if live.is_empty() {
        return result;
    }
    let explicit_grounds: Vec<Uuid> = live.iter().filter(|i| i.fixed).map(|i| i.id).collect();
    let grounds = if explicit_grounds.is_empty() {
        vec![live[0].id]
    } else {
        explicit_grounds
    };
    let mut placed: BTreeMap<Uuid, Transform> = grounds
        .iter()
        .filter_map(|id| tree.instance(*id).map(|i| (*id, i.transform)))
        .collect();

    // Connector frame → transform (part coords), computed once.
    let mut frame_xf: HashMap<Uuid, Transform> = HashMap::new();
    for c in &tree.connectors {
        let f = frames.get(&c.id).copied().unwrap_or(c.frame);
        match f.to_transform() {
            Ok(t) => {
                frame_xf.insert(c.id, t);
            }
            Err(e) => result
                .errors
                .push(format!("connector `{}` ({}): {e}", c.name, c.id)),
        }
    }

    let mut consumed: HashSet<Uuid> = HashSet::new();
    loop {
        let mut progressed = false;
        let pending: Vec<&Mate> = tree
            .mates
            .iter()
            .filter(|m| !m.suppressed && !consumed.contains(&m.id))
            .collect();
        for m in pending {
            let Some(offset) = mate_offset(&m.kind) else {
                consumed.insert(m.id); // reported by validate()
                continue;
            };
            let (Some(ca), Some(cb)) = (
                tree.connector(m.connectors[0]),
                tree.connector(m.connectors[1]),
            ) else {
                result
                    .errors
                    .push(format!("mate `{}` ({}): missing connector", m.name, m.id));
                consumed.insert(m.id);
                continue;
            };
            let (Some(ia), Some(ib)) = (ca.top_instance_id(), cb.top_instance_id()) else {
                result.errors.push(format!(
                    "mate `{}` ({}): connector without a usable instance",
                    m.name, m.id
                ));
                consumed.insert(m.id);
                continue;
            };
            let (Some(fa), Some(fb)) =
                (frame_xf.get(&ca.id).copied(), frame_xf.get(&cb.id).copied())
            else {
                consumed.insert(m.id); // frame error already reported
                continue;
            };
            let pa = placed.get(&ia).copied();
            let pb = placed.get(&ib).copied();
            match (pa, pb) {
                (Some(ta), None) => {
                    // T_b * F_b = T_a * F_a * offset
                    let tb = ta.compose(&fa).compose(&offset).compose(&fb.inverse());
                    placed.insert(ib, tb);
                    consumed.insert(m.id);
                    progressed = true;
                }
                (None, Some(tb)) => {
                    let ta = tb
                        .compose(&fb)
                        .compose(&offset.inverse())
                        .compose(&fa.inverse());
                    placed.insert(ia, ta);
                    consumed.insert(m.id);
                    progressed = true;
                }
                (Some(ta), Some(tb)) => {
                    let expected = ta.compose(&fa).compose(&offset).compose(&fb.inverse());
                    if !expected.approx_eq(&tb, tol_m) {
                        result.errors.push(format!(
                            "mate `{}` ({}): over-constrained — `{}` is already placed elsewhere",
                            m.name,
                            m.id,
                            tree.instance(ib).map(|i| i.name.as_str()).unwrap_or("?")
                        ));
                    }
                    consumed.insert(m.id);
                    progressed = true;
                }
                (None, None) => {} // neither side reachable yet; retry next pass
            }
        }
        if !progressed {
            break;
        }
    }

    for i in &live {
        if let std::collections::btree_map::Entry::Vacant(slot) = placed.entry(i.id) {
            result.warnings.push(format!(
                "instance `{}` ({}) is not grounded through mates; its own transform is used",
                i.name, i.id
            ));
            slot.insert(i.transform);
        }
    }
    result.placements = placed;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn transform_compose_inverse_apply() {
        let r = Transform::from_rotation(quat_axis_angle([0.0, 0.0, 1.0], 90.0));
        let t = Transform::translation([1.0, 2.0, 3.0]);
        let tr = t.compose(&r); // rotate then translate
        assert!(close(tr.apply([1.0, 0.0, 0.0]), [1.0, 3.0, 3.0]));
        let back = tr.inverse().compose(&tr);
        assert!(back.approx_eq(&Transform::identity(), 1e-9));
        assert!(close(tr.inverse().apply([1.0, 3.0, 3.0]), [1.0, 0.0, 0.0]));
        let m = tr.to_matrix();
        assert!(close([m[12], m[13], m[14]], [1.0, 2.0, 3.0]));
        assert!(close([m[0], m[1], m[2]], [0.0, 1.0, 0.0]), "{:?}", &m[0..3]);
    }

    #[test]
    fn frame_basis_is_orthonormal_and_deterministic() {
        let f = Frame::on_plane([0.0, 0.0, 0.01], [0.0, 0.0, 1.0]);
        let (x, y, z) = f.basis().unwrap();
        assert!(close(z, [0.0, 0.0, 1.0]));
        assert!(close(x, [1.0, 0.0, 0.0]));
        assert!(close(y, [0.0, 1.0, 0.0]));
        let t = f.to_transform().unwrap();
        assert!(close(t.apply([0.0, 0.0, 0.0]), [0.0, 0.0, 0.01]));
        // Explicit x is orthogonalized against z.
        let g = Frame {
            origin: [0.0; 3],
            z_axis: [0.0, 0.0, 2.0],
            x_axis: [1.0, 0.0, 0.5],
        };
        let (x, _, _) = g.basis().unwrap();
        assert!(close(x, [1.0, 0.0, 0.0]));
        assert!(Frame {
            origin: [0.0; 3],
            z_axis: [0.0; 3],
            x_axis: [0.0; 3]
        }
        .basis()
        .is_err());
    }

    fn tree_two_cubes() -> (AssemblyTree, HashMap<Uuid, Frame>) {
        // Two 10 mm cubes at the origin; A grounded. Connector A on A's TOP
        // face, connector B on B's BOTTOM face (outward normals).
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let ca = Uuid::new_v4();
        let cb = Uuid::new_v4();
        let inst = |id, name: &str| Instance {
            id,
            name: name.into(),
            source: PartRef {
                source_id: None,
                tab_id: "t".into(),
            },
            transform: Transform::identity(),
            fixed: false,
            suppressed: false,
            external_key: None,
            parameter_overrides: None,
            extra: Map::new(),
        };
        let conn = |id, inst_id, name: &str| MateConnector {
            id,
            name: name.into(),
            instance_path: vec![inst_id],
            geom_ref: None,
            part_connector: None,
            frame: Frame::default(),
            anchor: AxialAnchor::Middle,
            flip_z: false,
            rotation_deg: 0.0,
            offset_m: [0.0; 3],
            extra: Map::new(),
        };
        let tree = AssemblyTree {
            instances: vec![inst(a, "A"), inst(b, "B")],
            connectors: vec![conn(ca, a, "A top"), conn(cb, b, "B bottom")],
            mates: vec![Mate {
                id: Uuid::new_v4(),
                name: "stack".into(),
                kind: MateKind::Fastened {
                    flip: true,
                    rotation_deg: 0.0,
                },
                connectors: [ca, cb],
                suppressed: false,
                extra: Map::new(),
            }],
            placements: BTreeMap::new(),
            extra: Map::new(),
        };
        let mut frames = HashMap::new();
        frames.insert(ca, Frame::on_plane([0.005, 0.005, 0.01], [0.0, 0.0, 1.0]));
        frames.insert(cb, Frame::on_plane([0.005, 0.005, 0.0], [0.0, 0.0, -1.0]));
        (tree, frames)
    }

    #[test]
    fn fastened_with_flip_stacks_b_on_a_upright() {
        let (tree, frames) = tree_two_cubes();
        let r = solve_fastened(&tree, &frames, 1e-9);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let b = tree.instances[1].id;
        let tb = r.placements[&b];
        // B's bottom (z=0) lands on A's top (z=10 mm), B stays upright.
        assert!(
            close(tb.apply([0.0, 0.0, 0.0]), [0.0, 0.0, 0.01]),
            "{:?}",
            tb
        );
        assert!(
            close(tb.apply_dir([0.0, 0.0, 1.0]), [0.0, 0.0, 1.0]),
            "{:?}",
            tb
        );
        assert!(r.placements[&tree.instances[0].id].approx_eq(&Transform::identity(), 1e-12));
    }

    #[test]
    fn fastened_without_flip_aligns_z_axes_so_b_is_upside_down() {
        let (mut tree, frames) = tree_two_cubes();
        tree.mates[0].kind = MateKind::Fastened {
            flip: false,
            rotation_deg: 0.0,
        };
        let r = solve_fastened(&tree, &frames, 1e-9);
        let tb = r.placements[&tree.instances[1].id];
        assert!(close(tb.apply_dir([0.0, 0.0, -1.0]), [0.0, 0.0, 1.0]));
        assert!(close(tb.apply([0.005, 0.005, 0.0]), [0.005, 0.005, 0.01]));
    }

    #[test]
    fn rotation_about_z_and_direction_independence() {
        let (mut tree, frames) = tree_two_cubes();
        tree.mates[0].kind = MateKind::Fastened {
            flip: true,
            rotation_deg: 90.0,
        };
        let r = solve_fastened(&tree, &frames, 1e-9);
        let tb = r.placements[&tree.instances[1].id];
        // B's x axis is turned 90° about the connector z (world z).
        let x = tb.apply_dir([1.0, 0.0, 0.0]);
        assert!(
            (x[2]).abs() < 1e-9 && (x[0].abs() - 0.0).abs() < 1e-9 && x[1].abs() > 0.999,
            "{x:?}"
        );
        // Grounding B instead places A below it: same relative placement.
        tree.instances[1].fixed = true;
        let r2 = solve_fastened(&tree, &frames, 1e-9);
        let ta = r2.placements[&tree.instances[0].id];
        let rel = tb.inverse(); // A relative to B in the first solve
        assert!(ta.approx_eq(&rel, 1e-9), "{ta:?} vs {rel:?}");
    }

    #[test]
    fn chains_ungrounded_and_over_constrained() {
        let (mut tree, mut frames) = tree_two_cubes();
        let a = tree.instances[0].id;
        let b = tree.instances[1].id;
        // C stacked on B through a second mate; D unmated with its own transform.
        let c = Uuid::new_v4();
        let d = Uuid::new_v4();
        let base = &tree.instances[0];
        let mut ic = base.clone();
        ic.id = c;
        ic.name = "C".into();
        let mut id_ = base.clone();
        id_.id = d;
        id_.name = "D".into();
        id_.transform = Transform::translation([0.05, 0.0, 0.0]);
        tree.instances.push(ic);
        tree.instances.push(id_);
        let cb_top = Uuid::new_v4();
        let cc_bot = Uuid::new_v4();
        let mut k = tree.connectors[0].clone();
        k.id = cb_top;
        k.instance_path = vec![b];
        tree.connectors.push(k);
        let mut k2 = tree.connectors[1].clone();
        k2.id = cc_bot;
        k2.instance_path = vec![c];
        tree.connectors.push(k2);
        frames.insert(
            cb_top,
            Frame::on_plane([0.005, 0.005, 0.01], [0.0, 0.0, 1.0]),
        );
        frames.insert(
            cc_bot,
            Frame::on_plane([0.005, 0.005, 0.0], [0.0, 0.0, -1.0]),
        );
        let mut m2 = tree.mates[0].clone();
        m2.id = Uuid::new_v4();
        m2.connectors = [cb_top, cc_bot];
        // Listed BEFORE the mate that grounds B: the solver must iterate.
        tree.mates.insert(0, m2);

        let r = solve_fastened(&tree, &frames, 1e-9);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(close(
            r.placements[&c].apply([0.0, 0.0, 0.0]),
            [0.0, 0.0, 0.02]
        ));
        assert_eq!(r.warnings.len(), 1);
        assert!(r.warnings[0].contains("`D`"));
        assert!(close(r.placements[&d].translation_m, [0.05, 0.0, 0.0]));

        // A second, contradictory mate between A and B is over-constrained.
        let mut m3 = tree.mates[1].clone();
        m3.id = Uuid::new_v4();
        m3.kind = MateKind::Fastened {
            flip: true,
            rotation_deg: 45.0,
        };
        tree.mates.push(m3);
        let r = solve_fastened(&tree, &frames, 1e-9);
        assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
        assert!(r.errors[0].contains("over-constrained"));
        let _ = a;
    }

    #[test]
    fn mate_kind_round_trips_and_unknown_is_opaque() {
        let k: MateKind =
            serde_json::from_value(serde_json::json!({ "type": "Fastened", "flip": true }))
                .unwrap();
        assert_eq!(
            k,
            MateKind::Fastened {
                flip: true,
                rotation_deg: 0.0
            }
        );
        let v = serde_json::json!({ "type": "Gear", "ratio": 2.5, "limits": [0, 90] });
        let u: MateKind = serde_json::from_value(v.clone()).unwrap();
        assert!(matches!(u, MateKind::Unknown(_)));
        assert_eq!(u.type_tag(), "Gear");
        assert_eq!(serde_json::to_value(&u).unwrap(), v);
        assert!(serde_json::from_value::<MateKind>(serde_json::json!({ "flip": true })).is_err());
        assert!(serde_json::from_value::<MateKind>(
            serde_json::json!({ "type": "Fastened", "flip": "yes" })
        )
        .is_err());
    }

    #[test]
    fn validate_reports_dangling_and_unsupported() {
        let (mut tree, _) = tree_two_cubes();
        assert!(tree.validate().is_empty());
        tree.connectors[0].instance_path = vec![Uuid::new_v4()];
        tree.connectors[1].instance_path = vec![tree.instances[1].id, Uuid::new_v4()];
        tree.mates[0].connectors[1] = Uuid::new_v4();
        tree.mates.push(Mate {
            id: Uuid::new_v4(),
            name: "hinge".into(),
            kind: MateKind::Unknown(serde_json::json!({ "type": "Gear" })),
            connectors: [tree.connectors[0].id, tree.connectors[0].id],
            suppressed: false,
            extra: Map::new(),
        });
        let w = tree.validate();
        assert!(w.iter().any(|m| m.contains("does not exist")), "{w:?}");
        assert!(
            w.iter()
                .any(|m| m.contains("connector") && m.contains("does not exist")),
            "{w:?}"
        );
        assert!(
            w.iter().any(|m| m.contains("unknown mate kind `Gear`")),
            "{w:?}"
        );
        assert!(w.iter().any(|m| m.contains("same instance")), "{w:?}");
    }

    #[test]
    fn tree_round_trips_with_derived_placements_and_unknown_keys() {
        let (mut tree, frames) = tree_two_cubes();
        let r = solve_fastened(&tree, &frames, 1e-9);
        tree.placements = r.placements;
        tree.extra
            .insert("x-note".into(), Value::String("kept".into()));
        let json = serde_json::to_value(&tree).unwrap();
        assert_eq!(json["x-note"], "kept");
        assert_eq!(
            json["instances"][0]["transform"]["rotation_quat"],
            serde_json::json!([0.0, 0.0, 0.0, 1.0])
        );
        assert!(
            json["instances"][0].get("fixed").is_none(),
            "false flags are omitted"
        );
        let back: AssemblyTree = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(&back).unwrap(), json);
        assert_eq!(back.parts().len(), 1);
        // A minimal hand-written instance parses with defaults.
        let min: AssemblyTree = serde_json::from_value(serde_json::json!({
            "instances": [{ "id": Uuid::nil(), "name": "P", "source": { "tab_id": "t" } }]
        }))
        .unwrap();
        assert!(min.instances[0]
            .transform
            .approx_eq(&Transform::identity(), 0.0));
    }
}
