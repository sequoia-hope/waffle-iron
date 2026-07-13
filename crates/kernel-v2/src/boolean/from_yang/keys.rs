//! yang-output curve/surface **key vocabulary** for the from_yang assembler
//! (move-only F9 split from `from_yang.rs`; byte-identical). The KV5b `EdgeKind`
//! classification, the undirected `CurveKey`/`PairSurfaceKey` bit-exact
//! manifold-pairing keys, and their constructors. See `super`'s module docs.

use super::*;

pub(crate) fn edge_kind_tag(e: &EdgeKind) -> &'static str {
    match e {
        EdgeKind::Seg => "Seg",
        EdgeKind::Full { .. } => "Full",
        EdgeKind::Arc { .. } => "Arc",
        EdgeKind::EllipseArc { .. } => "EllipseArc",
        EdgeKind::HyperbolaArc { .. } => "HyperbolaArc",
        EdgeKind::SurfacePair { .. } => "SurfacePair",
    }
}

/// The curve vocabulary of one directed yang loop edge, KV5b-classified.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum EdgeKind {
    Seg,
    /// Full circle (`start == end`), canonical-cylinder vocabulary.
    Full {
        center: Point3,
        normal: [f64; 3],
        radius: f64,
    },
    /// Minor arc (`start != end`); `forward_normal` is the kernel-v2
    /// directional normal for THIS directed use (sweep < π).
    Arc {
        center: Point3,
        forward_normal: [f64; 3],
        radius: f64,
    },
    /// Minor ELLIPSE arc (PR-KV9, `start != end`): the exact oblique
    /// `plane ∩ cylinder` section piece. `forward_normal` is the
    /// directional plane normal for THIS directed use (parametric sweep
    /// < π in its frame).
    EllipseArc {
        center: Point3,
        forward_normal: [f64; 3],
        major_axis: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
    },
    /// Hyperbola arc (KV16, `start != end`): the axis-steep plane∩cone
    /// section piece between the endpoints, on the `+major_axis` branch.
    /// No directional normal (the open branch is injective — traversal is
    /// endpoint-determined, like `SurfacePair`); twins carry BIT-IDENTICAL
    /// fields.
    HyperbolaArc {
        center: Point3,
        normal: [f64; 3],
        major_axis: [f64; 3],
        semi_transverse: f64,
        semi_conjugate: f64,
    },
    /// Procedural surface-pair curve piece (M5, `start != end`): the general
    /// degree-4 cyl×cyl intersection between the endpoints, defined implicitly
    /// by its two `PairSurface`s. No directional normal (traversal is
    /// endpoint-determined); twins carry identical `a`/`b`.
    SurfacePair {
        a: crate::arena::PairSurface,
        b: crate::arena::PairSurface,
    },
}

/// UNDIRECTED curve identity for manifold edge-pairing, so two DISTINCT curved
/// edges sharing the same endpoint pair (a "bigon" — e.g. the LENS of two
/// crossing coplanar disc rims, bounded by one arc per circle) are paired
/// SEPARATELY. Keying the pairing by vertex pair alone would lump the lens's
/// two arcs into "4 uses" and reject a perfectly manifold output. Ignores the
/// per-use `forward_normal` (the two uses of one edge negate it); two real
/// twins always share exact `(center, radius)` (the curve-agreement check below
/// requires it), so this never splits a genuine twin — it only distinguishes
/// arcs on DIFFERENT circles.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub(crate) enum CurveKey {
    Seg,
    Circle {
        center: [u64; 3],
        radius: u64,
    },
    Ellipse {
        center: [u64; 3],
        major: [u64; 3],
        major_r: u64,
        minor_r: u64,
    },
    /// KV16: bit-exact hyperbola frame identity — distinct hyperbolas on
    /// the same vertex pair key separately; genuine twins share the
    /// descriptor exactly (bit-identical fields).
    Hyperbola {
        center: [u64; 3],
        major: [u64; 3],
        semi_t: u64,
        semi_c: u64,
    },
    /// M5: the ordered pair of defining-surface bit patterns. Distinct
    /// quartics on the same vertex pair (different cylinder pairs) key
    /// separately; genuine twins share the descriptor exactly.
    SurfacePair {
        a: PairSurfaceKey,
        b: PairSurfaceKey,
    },
}

/// Bit-exact key for a [`crate::arena::PairSurface`] (M5, K4).
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub(crate) enum PairSurfaceKey {
    Cylinder {
        axis_point: [u64; 3],
        axis_dir: [u64; 3],
        radius: u64,
    },
    Cone {
        apex: [u64; 3],
        axis_dir: [u64; 3],
        half_angle: u64,
    },
    /// F10: sphere operand of a general-position sphere×cyl / sphere×cone
    /// degree-4 pair.
    Sphere { center: [u64; 3], radius: u64 },
}

pub(crate) fn pair_surface_key(s: &crate::arena::PairSurface) -> PairSurfaceKey {
    match *s {
        crate::arena::PairSurface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => PairSurfaceKey::Cylinder {
            axis_point: [
                axis_point.x().to_bits(),
                axis_point.y().to_bits(),
                axis_point.z().to_bits(),
            ],
            axis_dir: [
                axis_dir.x.to_bits(),
                axis_dir.y.to_bits(),
                axis_dir.z.to_bits(),
            ],
            radius: radius.to_bits(),
        },
        crate::arena::PairSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => PairSurfaceKey::Cone {
            apex: [apex.x().to_bits(), apex.y().to_bits(), apex.z().to_bits()],
            axis_dir: [
                axis_dir.x.to_bits(),
                axis_dir.y.to_bits(),
                axis_dir.z.to_bits(),
            ],
            half_angle: half_angle.to_bits(),
        },
        crate::arena::PairSurface::Sphere { center, radius } => PairSurfaceKey::Sphere {
            center: [
                center.x().to_bits(),
                center.y().to_bits(),
                center.z().to_bits(),
            ],
            radius: radius.to_bits(),
        },
    }
}

pub(crate) fn curve_key(ek: &EdgeKind) -> CurveKey {
    let pb = |p: Point3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
    let vb = |v: [f64; 3]| [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()];
    match ek {
        EdgeKind::Seg => CurveKey::Seg,
        EdgeKind::Full { center, radius, .. } | EdgeKind::Arc { center, radius, .. } => {
            CurveKey::Circle {
                center: pb(*center),
                radius: radius.to_bits(),
            }
        }
        EdgeKind::EllipseArc {
            center,
            major_axis,
            major_radius,
            minor_radius,
            ..
        } => CurveKey::Ellipse {
            center: pb(*center),
            major: vb(*major_axis),
            major_r: major_radius.to_bits(),
            minor_r: minor_radius.to_bits(),
        },
        EdgeKind::HyperbolaArc {
            center,
            major_axis,
            semi_transverse,
            semi_conjugate,
            ..
        } => CurveKey::Hyperbola {
            center: pb(*center),
            major: vb(*major_axis),
            semi_t: semi_transverse.to_bits(),
            semi_c: semi_conjugate.to_bits(),
        },
        EdgeKind::SurfacePair { a, b } => CurveKey::SurfacePair {
            a: pair_surface_key(a),
            b: pair_surface_key(b),
        },
    }
}
