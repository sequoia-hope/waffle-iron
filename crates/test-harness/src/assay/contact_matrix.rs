//! Kernel 1.0 Gate 3.7 — the degenerate-contact matrix generator and report
//! model (`docs/kernel_1_0.md` §1 Gate 3 item 7).
//!
//! The gate crosses each surface pair of `{plane, cylinder, cone, sphere,
//! torus, B-spline}` with the six contact classes a mechanical model is made
//! of, and asks that every cell the pair admits categorizes CORRECT or returns
//! a typed refusal that names the contact class:
//!
//! | class | what touches |
//! |---|---|
//! | coincident surfaces | a face of A and a face of B lie on one surface and overlap in area |
//! | tangency along a curve | the two faces share a curve and a tangent plane along it |
//! | tangency at a point | the two faces share one point and a tangent plane there |
//! | an edge lying in a face | an edge of one solid lies in the interior of a face of the other, the faces at the edge crossing that face |
//! | a vertex lying on a face | a vertex of one solid lies in the interior of a face of the other, no edge at the vertex lying in that face |
//! | a shared edge | an edge of A and an edge of B are the same segment or arc, no face of one lying on a face of the other |
//!
//! For the last three classes the pair names the two faces involved: the edge
//! or vertex borders a face of one kind of the pair and the face it lies in
//! (or the face the other solid's edge borders) is of the other kind.
//!
//! [`admission`] decides, per cell, whether the pair admits the class; a cell
//! it does not admit carries a one-line geometric reason and is reported, not
//! skipped. [`all_contact_cases`] authors at least one document per admitted
//! cell. The six B-spline pairs are reported not buildable, as in the pair
//! matrix ([`NOT_BUILDABLE_BSPLINE`]).
//!
//! ## Exactness
//!
//! Every authored number — a sketch origin, a profile coordinate, a radius, a
//! depth, an axis point — is a dyadic rational, exactly representable in
//! `f64`, and the contact holds exactly in those numbers: equal radii, a face
//! offset equal to a radius, slopes from the 3-4-5 and 5-12-13 triangles
//! (`tan = 3/4`, a parallel of the torus at `cos = 3/5`). Sketch planes are
//! axis-aligned (their `SketchPlaneBasis` is then exact) except for the two
//! bodies that must be oblique because a cone's tangent plane is: the oblique
//! cylinder of `cylinder × cone` tangency along a line and the oblique cone of
//! `cone × cone` tangency at a point. Their directions are the unit vectors of
//! a 3-4-5 triangle, exact as rationals and within one ulp as floats.
//!
//! Each case names the two analytic surfaces in contact ([`AnalyticSurface`])
//! and sample points of the contact locus; [`verify_contact_geometry`] checks,
//! with no kernel call, that the points lie on both surfaces to `1e-12`, that
//! the surfaces are tangent there (coincident and tangency classes) or cross
//! (edge, vertex and shared-edge classes), that each point is a boundary point
//! of both authored solids (read from the document through
//! [`ExactChain`]), and that the two solids are arranged as the case says
//! ([`Arrangement`]).
//!
//! ## Expected results
//!
//! The meta of a document is derived from the document
//! (`prospect::derive_meta`), as in the pair matrix: the categorizer's mesh,
//! Euler, on-surface, exact-volume and monotonicity oracles judge it with no
//! authored target. One expectation is authored here, where the result is
//! known by construction to be EMPTY — the intersection of two solids that
//! only kiss, the cut of two identical solids ([`ContactCase::expects_empty`]):
//! the meta then carries `expected_solid_count = 0`, the categorizer's own
//! authored-emptiness expectation. The exact chain cannot tell an empty result
//! from one thinner than its lattice, so without the expectation a correctly
//! empty result is left unjudged (`ERROR: empty_result`); with it, a body the
//! kernel does return is a solid-count failure (WRONG).
//!
//! ## Trimmed operands
//!
//! A sphere or a torus has no edge and a full cylinder has no vertex, so the
//! cells whose edge or vertex must border such a face need a face with a
//! boundary. Where a feature builds one directly the case uses it — the half
//! torus, a tube circle revolved 180°, whose two end circles are edges of its
//! toroidal face. Elsewhere the case uses a TRIMMED operand: a primitive first
//! cut or intersected with a second primitive ([`Operand::trim`]). The trim
//! is a setup step, not the contact under test; it runs alone as the `setupa`
//! / `setupb` pseudo-operation and a case whose setup is not CORRECT is
//! reported BLOCKED rather than as a contact finding. (A trimmed operand the
//! kernel builds but then refuses as a boolean operand — a spherical face
//! with an edge, today — shows as that refusal on the contact boolean
//! itself.)

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use waffle_types::SketchPlaneBasis;

use crate::assay::categorize::{CaseOutcome, Category};
use crate::assay::cpu_runner::RunProvenance;
use crate::assay::exact_membership::{ExactChain, ExactSolid};
use crate::assay::gen::AssayMeta;
use crate::assay::pair_matrix::{
    canonicalize_document, resolve_git_commit, ExactVolumeRecord, SurfaceKind, SurfacePair,
    ALL_SURFACE_PAIRS, NOT_BUILDABLE_BSPLINE,
};
use crate::assay::prospect::derive_meta;
use crate::workflow::ModelBuilder;

// ── Contact classes ─────────────────────────────────────────────────────────

/// The six Gate 3.7 contact classes, in the order the gate lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ContactClass {
    Coincident,
    TangentCurve,
    TangentPoint,
    EdgeInFace,
    VertexOnFace,
    SharedEdge,
}

impl ContactClass {
    pub const ALL: [ContactClass; 6] = [
        ContactClass::Coincident,
        ContactClass::TangentCurve,
        ContactClass::TangentPoint,
        ContactClass::EdgeInFace,
        ContactClass::VertexOnFace,
        ContactClass::SharedEdge,
    ];

    /// The class as `docs/kernel_1_0.md` words it.
    pub const fn label(self) -> &'static str {
        match self {
            ContactClass::Coincident => "coincident surfaces",
            ContactClass::TangentCurve => "tangency along a curve",
            ContactClass::TangentPoint => "tangency at a point",
            ContactClass::EdgeInFace => "an edge lying in a face",
            ContactClass::VertexOnFace => "a vertex lying on a face",
            ContactClass::SharedEdge => "a shared edge",
        }
    }

    /// Lowercase identifier with no underscore, used in case ids.
    pub const fn slug(self) -> &'static str {
        match self {
            ContactClass::Coincident => "coincident",
            ContactClass::TangentCurve => "tangentcurve",
            ContactClass::TangentPoint => "tangentpoint",
            ContactClass::EdgeInFace => "edgeinface",
            ContactClass::VertexOnFace => "vertexonface",
            ContactClass::SharedEdge => "sharededge",
        }
    }

    /// Column header of the printed grid.
    pub const fn column(self) -> &'static str {
        match self {
            ContactClass::Coincident => "coincid",
            ContactClass::TangentCurve => "tan-crv",
            ContactClass::TangentPoint => "tan-pnt",
            ContactClass::EdgeInFace => "edge/fc",
            ContactClass::VertexOnFace => "vert/fc",
            ContactClass::SharedEdge => "sh-edge",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.slug() == s)
    }

    /// Word groups that name this contact class in an error text: the class
    /// is named when every word of at least one group occurs (case ignored).
    ///
    /// This is a reading of TEXT, because text is all the categorizer hands
    /// on; it stands in for the typed variant the gate asks for and errs
    /// toward "not named" (a bare "tangent" names neither tangency class).
    pub const fn refusal_word_groups(self) -> &'static [&'static [&'static str]] {
        match self {
            ContactClass::Coincident => &[
                &["coplanar"],
                &["coincident"],
                &["co-surface"],
                &["cosurface"],
                &["same surface"],
                &["identical", "surface"],
                &["overlapping", "face"],
            ],
            ContactClass::TangentCurve => &[
                &["tangen", "curve"],
                &["tangen", "line"],
                &["tangen", "circle"],
                &["tangen", "along"],
                &["graz", "curve"],
                &["graz", "line"],
                &["pinch", "line"],
            ],
            ContactClass::TangentPoint => &[
                &["tangen", "point"],
                &["graz", "point"],
                &["tangen", "isolated"],
            ],
            ContactClass::EdgeInFace => &[
                &["edge-in-face"],
                &["edge in face"],
                &["edge in a face"],
                &["edge lying in"],
                &["edge on face"],
                &["edge-on-face"],
                &["edge", "lies in", "face"],
            ],
            ContactClass::VertexOnFace => &[
                &["vertex-on-face"],
                &["vertex on face"],
                &["vertex on a face"],
                &["vertex lying on"],
                &["vertex in face"],
                &["vertex", "lies on", "face"],
            ],
            ContactClass::SharedEdge => &[
                &["shared edge"],
                &["shared-edge"],
                &["coincident edge"],
                &["edge-edge"],
                &["edge-on-edge"],
                &["collinear edge"],
            ],
        }
    }
}

/// Whether an error or refusal text names `class` (see
/// [`ContactClass::refusal_word_groups`]).
pub fn refusal_names_class(detail: &str, class: ContactClass) -> bool {
    let d = detail.to_lowercase();
    class
        .refusal_word_groups()
        .iter()
        .any(|group| group.iter().all(|w| d.contains(w)))
}

/// Whether a surface pair admits a contact class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Admitted,
    /// The pair cannot be in this contact; the reason is geometric.
    NotAdmitted(&'static str),
    /// A B-spline pair: no feature builds a B-spline face yet.
    NotBuildable,
}

/// Decide whether `pair` admits `class`.
///
/// Coincidence needs one surface under both faces, so only a pair of equal
/// kinds admits it. Tangency is decided by the tangent planes: a plane is its
/// own tangent plane, a cylinder and a cone carry one tangent plane along each
/// ruling, and a sphere's tangent plane meets it in one point. The edge,
/// vertex and shared-edge classes are admitted by every pair, with a trimmed
/// face where a full primitive has no edge or vertex.
pub fn admission(pair: SurfacePair, class: ContactClass) -> Admission {
    use SurfaceKind::{Cone, Cylinder, Plane, Sphere};
    if !pair.is_buildable() {
        return Admission::NotBuildable;
    }
    match class {
        ContactClass::Coincident => {
            if pair.a == pair.b {
                Admission::Admitted
            } else {
                Admission::NotAdmitted(
                    "two surfaces of different kinds are different irreducible algebraic surfaces: they meet in curves, never in a patch of area",
                )
            }
        }
        ContactClass::TangentCurve => match (pair.a, pair.b) {
            (Plane, Plane) => Admission::NotAdmitted(
                "two planes with a common point and a common normal there are one plane: that is the coincident class",
            ),
            (Plane, Sphere) => Admission::NotAdmitted(
                "a plane tangent to a sphere touches it at exactly one point",
            ),
            (Sphere, Sphere) => Admission::NotAdmitted(
                "two distinct spheres touch at one point only (centres and contact point are collinear); tangent along a curve they are one sphere",
            ),
            _ => Admission::Admitted,
        },
        ContactClass::TangentPoint => match (pair.a, pair.b) {
            (Plane, Plane) => Admission::NotAdmitted(
                "two planes with a common point and a common normal there are one plane: that is the coincident class",
            ),
            (Plane, Cylinder) => Admission::NotAdmitted(
                "a cylinder has one tangent plane along each ruling, so a tangent plane touches along the whole ruling: tangency along a curve",
            ),
            (Plane, Cone) => Admission::NotAdmitted(
                "a cone has one tangent plane along each generator, so a tangent plane touches along the whole generator: tangency along a curve",
            ),
            _ => Admission::Admitted,
        },
        ContactClass::EdgeInFace | ContactClass::VertexOnFace | ContactClass::SharedEdge => {
            Admission::Admitted
        }
    }
}

// ── Authoring vocabulary ────────────────────────────────────────────────────

/// A sketch profile in sketch coordinates `(u, v)`.
#[derive(Debug, Clone, PartialEq)]
pub enum Profile {
    Polygon(Vec<(f64, f64)>),
    Circle { cu: f64, cv: f64, r: f64 },
}

/// One primitive body: a sketch and the feature that sweeps it. The fields are
/// the numbers written into the document.
#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    /// The profile extruded `depth` along `+normal`.
    Extrude {
        origin: [f64; 3],
        normal: [f64; 3],
        profile: Profile,
        depth: f64,
    },
    /// The profile revolved `angle` degrees about `(axis_origin, axis_dir)`
    /// (360 a full turn), counter-clockwise about the axis direction from
    /// the profile's own side.
    Revolve {
        origin: [f64; 3],
        normal: [f64; 3],
        profile: Profile,
        axis_origin: [f64; 3],
        axis_dir: [f64; 3],
        angle: f64,
    },
}

/// How a trimmed operand is cut from its base primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrimOp {
    /// `base ∩ tool`.
    Intersect,
    /// `base − tool`.
    Cut,
}

/// One operand of the contact boolean: a primitive, optionally trimmed by a
/// second primitive in a setup boolean (module doc, "Trimmed operands").
#[derive(Debug, Clone, PartialEq)]
pub struct Operand {
    pub base: Prim,
    pub trim: Option<(TrimOp, Prim)>,
}

impl From<Prim> for Operand {
    fn from(base: Prim) -> Self {
        Operand { base, trim: None }
    }
}

fn trimmed(base: Prim, op: TrimOp, tool: Prim) -> Operand {
    Operand {
        base,
        trim: Some((op, tool)),
    }
}

/// How the two solids sit relative to each other. Checked on the authored
/// document by [`verify_contact_geometry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arrangement {
    /// Interiors disjoint: the solids meet in the contact only. The
    /// regularized intersection is empty and the cut returns A.
    Kiss,
    /// B lies inside A and touches A's boundary in the contact.
    Nested,
    /// A and B are the same point set.
    Identical,
    /// The interiors overlap and neither solid contains the other; the
    /// contact lies on the boundary of the overlap.
    Overlap,
}

impl Arrangement {
    pub const fn label(self) -> &'static str {
        match self {
            Arrangement::Kiss => "kiss",
            Arrangement::Nested => "nested",
            Arrangement::Identical => "identical",
            Arrangement::Overlap => "overlap",
        }
    }
}

/// An analytic surface of one of the five buildable kinds, as an implicit
/// residual and a unit normal — the independent statement of what a case's
/// contact is between.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnalyticSurface {
    Plane {
        point: [f64; 3],
        normal: [f64; 3],
    },
    Cylinder {
        point: [f64; 3],
        axis: [f64; 3],
        radius: f64,
    },
    /// The nappe `radius = slope · height`, height measured from `apex` along
    /// `axis`.
    Cone {
        apex: [f64; 3],
        axis: [f64; 3],
        slope: f64,
    },
    Sphere {
        center: [f64; 3],
        radius: f64,
    },
    Torus {
        center: [f64; 3],
        axis: [f64; 3],
        major: f64,
        minor: f64,
    },
}

impl AnalyticSurface {
    pub fn kind(&self) -> SurfaceKind {
        match self {
            AnalyticSurface::Plane { .. } => SurfaceKind::Plane,
            AnalyticSurface::Cylinder { .. } => SurfaceKind::Cylinder,
            AnalyticSurface::Cone { .. } => SurfaceKind::Cone,
            AnalyticSurface::Sphere { .. } => SurfaceKind::Sphere,
            AnalyticSurface::Torus { .. } => SurfaceKind::Torus,
        }
    }

    /// Signed distance from `p` to the surface (exact for every kind; for the
    /// cone it is the distance to the generator line in `p`'s meridian plane).
    pub fn residual(&self, p: [f64; 3]) -> f64 {
        match *self {
            AnalyticSurface::Plane { point, normal } => dot(sub(p, point), unit(normal)),
            AnalyticSurface::Cylinder {
                point,
                axis,
                radius,
            } => perp_len(sub(p, point), unit(axis)) - radius,
            AnalyticSurface::Cone { apex, axis, slope } => {
                let a = unit(axis);
                let d = sub(p, apex);
                let h = dot(d, a);
                (perp_len(d, a) - slope * h) / (1.0 + slope * slope).sqrt()
            }
            AnalyticSurface::Sphere { center, radius } => len(sub(p, center)) - radius,
            AnalyticSurface::Torus {
                center,
                axis,
                major,
                minor,
            } => {
                let a = unit(axis);
                let d = sub(p, center);
                let h = dot(d, a);
                let rho = perp_len(d, a);
                ((rho - major).powi(2) + h * h).sqrt() - minor
            }
        }
    }

    /// Unit normal of the surface at `p` (the residual's gradient), or `None`
    /// where the surface has none — a cone's apex, a point on an axis.
    pub fn normal(&self, p: [f64; 3]) -> Option<[f64; 3]> {
        let radial = |d: [f64; 3], a: [f64; 3]| -> Option<[f64; 3]> {
            let r = sub(d, scale(a, dot(d, a)));
            let l = len(r);
            (l > 1e-9).then(|| scale(r, 1.0 / l))
        };
        match *self {
            AnalyticSurface::Plane { normal, .. } => Some(unit(normal)),
            AnalyticSurface::Cylinder { point, axis, .. } => radial(sub(p, point), unit(axis)),
            AnalyticSurface::Cone { apex, axis, slope } => {
                let a = unit(axis);
                let e = radial(sub(p, apex), a)?;
                Some(unit(sub(e, scale(a, slope))))
            }
            AnalyticSurface::Sphere { center, .. } => {
                let d = sub(p, center);
                (len(d) > 1e-9).then(|| unit(d))
            }
            AnalyticSurface::Torus {
                center,
                axis,
                major,
                ..
            } => {
                let a = unit(axis);
                let d = sub(p, center);
                let e = radial(d, a)?;
                let tube_center = add(center, scale(e, major));
                let n = sub(p, tube_center);
                (len(n) > 1e-9).then(|| unit(n))
            }
        }
    }
}

// ── Primitive constructors (world coordinates in, sketch numbers out) ───────

fn sketch_uv(origin: [f64; 3], normal: [f64; 3], p: [f64; 3]) -> (f64, f64) {
    let basis = SketchPlaneBasis::from_origin_normal(origin, normal);
    let d = sub(p, origin);
    (dot(d, basis.x_axis), dot(d, basis.y_axis))
}

/// A polygon wound counter-clockwise in `(u, v)`.
fn ccw(mut pts: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let n = pts.len();
    let mut twice_area = 0.0;
    for i in 0..n {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % n];
        twice_area += x0 * y1 - x1 * y0;
    }
    if twice_area < 0.0 {
        pts.reverse();
    }
    pts
}

/// A prism: the world polygon `pts` (all in the plane through `origin` with
/// `normal`) extruded `depth` along `+normal`.
fn prism(origin: [f64; 3], normal: [f64; 3], pts: &[[f64; 3]], depth: f64) -> Prim {
    Prim::Extrude {
        origin,
        normal,
        profile: Profile::Polygon(ccw(pts
            .iter()
            .map(|&p| sketch_uv(origin, normal, p))
            .collect())),
        depth,
    }
}

/// The axis-aligned block `[min, max]`.
fn blk(min: [f64; 3], max: [f64; 3]) -> Prim {
    let z = min[2];
    prism(
        [0.0, 0.0, z],
        [0.0, 0.0, 1.0],
        &[
            [min[0], min[1], z],
            [max[0], min[1], z],
            [max[0], max[1], z],
            [min[0], max[1], z],
        ],
        max[2] - min[2],
    )
}

/// The polygon `xz` in the plane `y = y0`, extruded to `y = y1 > y0`.
fn prism_y(xz: &[(f64, f64)], y0: f64, y1: f64) -> Prim {
    let pts: Vec<[f64; 3]> = xz.iter().map(|&(x, z)| [x, y0, z]).collect();
    prism([0.0, y0, 0.0], [0.0, 1.0, 0.0], &pts, y1 - y0)
}

/// The polygon `yz` in the plane `x = x0`, extruded to `x = x1 > x0`.
fn prism_x(yz: &[(f64, f64)], x0: f64, x1: f64) -> Prim {
    let pts: Vec<[f64; 3]> = yz.iter().map(|&(y, z)| [x0, y, z]).collect();
    prism([x0, 0.0, 0.0], [1.0, 0.0, 0.0], &pts, x1 - x0)
}

/// The polygon `xy` in the plane `z = z0`, extruded to `z = z1 > z0`.
fn prism_z(xy: &[(f64, f64)], z0: f64, z1: f64) -> Prim {
    let pts: Vec<[f64; 3]> = xy.iter().map(|&(x, y)| [x, y, z0]).collect();
    prism([0.0, 0.0, z0], [0.0, 0.0, 1.0], &pts, z1 - z0)
}

/// A cylinder: the circle of radius `r` about `base` in the plane with normal
/// `dir`, extruded `depth` along `dir`. The kernel puts the seam ruling along
/// the sketch's `u` axis from the centre.
fn cyl(base: [f64; 3], dir: [f64; 3], r: f64, depth: f64) -> Prim {
    Prim::Extrude {
        origin: base,
        normal: dir,
        profile: Profile::Circle {
            cu: 0.0,
            cv: 0.0,
            r,
        },
        depth,
    }
}

/// A solid of revolution about the vertical axis through `(cx, cy)`: the
/// polygon `rz` of `(radius, z)` points in the plane `y = cy`, where a
/// NEGATIVE radius draws the profile on the `−x` side of the axis (the kernel
/// puts the seam on the profile's side).
fn lathe_z(cx: f64, cy: f64, rz: &[(f64, f64)]) -> Prim {
    Prim::Revolve {
        origin: [cx, cy, 0.0],
        normal: [0.0, -1.0, 0.0],
        profile: Profile::Polygon(ccw(rz.to_vec())),
        axis_origin: [cx, cy, 0.0],
        axis_dir: [0.0, 0.0, 1.0],
        angle: 360.0,
    }
}

/// A solid of revolution about the axis parallel to `y` through `(cx, ·,
/// cz)`: the polygon `yr` of `(y, radius)` points in the plane `x = cx`, drawn
/// on the `+z` side of the axis.
fn lathe_y(cx: f64, cz: f64, yr: &[(f64, f64)]) -> Prim {
    Prim::Revolve {
        origin: [cx, 0.0, cz],
        normal: [1.0, 0.0, 0.0],
        profile: Profile::Polygon(ccw(yr.to_vec())),
        axis_origin: [cx, 0.0, cz],
        axis_dir: [0.0, 1.0, 0.0],
        angle: 360.0,
    }
}

/// The sphere of radius `r` about `c` (an on-axis circle revolved about the
/// vertical axis). The kernel's poles are `c ± r·ẑ` and its seam the meridian
/// through `c + r·x̂`.
fn sphere(c: [f64; 3], r: f64) -> Prim {
    Prim::Revolve {
        origin: c,
        normal: [0.0, -1.0, 0.0],
        profile: Profile::Circle {
            cu: 0.0,
            cv: 0.0,
            r,
        },
        axis_origin: c,
        axis_dir: [0.0, 0.0, 1.0],
        angle: 360.0,
    }
}

/// The torus about the vertical axis through `c`. `side = 1.0` draws the tube
/// circle on the `+x` side, `-1.0` on the `−x` side; the kernel's seams are
/// that circle and the outer equator.
fn torus_z(c: [f64; 3], major: f64, minor: f64, side: f64) -> Prim {
    Prim::Revolve {
        origin: c,
        normal: [0.0, -1.0, 0.0],
        profile: Profile::Circle {
            cu: side * major,
            cv: 0.0,
            r: minor,
        },
        axis_origin: c,
        axis_dir: [0.0, 0.0, 1.0],
        angle: 360.0,
    }
}

/// The reference torus's half at `y >= 0`: its tube circle on the `+x` side
/// revolved 180 degrees. It ends in two flat discs in the plane `y = 0`, each
/// bounded by a tube circle of radius 0.625 — about `(1.25, 0, 0)` and about
/// `(-1.25, 0, 0)` — which are its edges: a toroidal face with an edge, built
/// with no boolean.
fn half_torus() -> Prim {
    Prim::Revolve {
        origin: [0.0; 3],
        normal: [0.0, -1.0, 0.0],
        profile: Profile::Circle {
            cu: T_MAJOR,
            cv: 0.0,
            r: T_MINOR,
        },
        axis_origin: [0.0; 3],
        axis_dir: [0.0, 0.0, 1.0],
        angle: 180.0,
    }
}

/// Sample points of the half torus's end circle about `(-1.25, 0, 0)`, at the
/// 3-4-5 points of the circle (off both of its axes).
const HALF_TORUS_RIM: [[f64; 3]; 4] = [
    [-0.875, 0.0, 0.5],
    [-1.625, 0.0, 0.5],
    [-0.875, 0.0, -0.5],
    [-1.625, 0.0, -0.5],
];

// The shapes most cells share. Units are metres, as everywhere in the kernel.

/// The reference torus `T`: centre the origin, vertical axis, major radius
/// 1.25, minor radius 0.625. Its dyadic landmarks: equators at radius 0.625
/// and 1.875, crowns at radius 1.25 and `z = ±0.625`, the 3-4-5 parallels at
/// `(1.25 ± 0.375, ±0.5)` and `(1.25 ± 0.5, ±0.375)`.
const T_MAJOR: f64 = 1.25;
const T_MINOR: f64 = 0.625;

fn torus_t() -> Prim {
    torus_z([0.0; 3], T_MAJOR, T_MINOR, 1.0)
}

fn surf_t() -> AnalyticSurface {
    AnalyticSurface::Torus {
        center: [0.0; 3],
        axis: [0.0, 0.0, 1.0],
        major: T_MAJOR,
        minor: T_MINOR,
    }
}

/// The reference cone `K`: apex the origin, vertical axis, `radius = 0.75·z`
/// (the 3-4-5 slope; outward normal `(0.8·ê, −0.6)`), as the frustum between
/// `z0` and `z1`, drawn on the side `side` of the axis.
fn cone_k(z0: f64, z1: f64, side: f64) -> Prim {
    lathe_z(
        0.0,
        0.0,
        &[
            (0.0, z0),
            (side * 0.75 * z0, z0),
            (side * 0.75 * z1, z1),
            (0.0, z1),
        ],
    )
}

fn surf_k() -> AnalyticSurface {
    cone_surf([0.0; 3], [0.0, 0.0, 1.0], 0.75)
}

fn plane_surf(point: [f64; 3], normal: [f64; 3]) -> AnalyticSurface {
    AnalyticSurface::Plane { point, normal }
}

fn cyl_surf(point: [f64; 3], axis: [f64; 3], radius: f64) -> AnalyticSurface {
    AnalyticSurface::Cylinder {
        point,
        axis,
        radius,
    }
}

fn cone_surf(apex: [f64; 3], axis: [f64; 3], slope: f64) -> AnalyticSurface {
    AnalyticSurface::Cone { apex, axis, slope }
}

fn sphere_surf(center: [f64; 3], radius: f64) -> AnalyticSurface {
    AnalyticSurface::Sphere { center, radius }
}

fn torus_surf(center: [f64; 3], major: f64, minor: f64) -> AnalyticSurface {
    AnalyticSurface::Torus {
        center,
        axis: [0.0, 0.0, 1.0],
        major,
        minor,
    }
}

const X: [f64; 3] = [1.0, 0.0, 0.0];
const Y: [f64; 3] = [0.0, 1.0, 0.0];
const Z: [f64; 3] = [0.0, 0.0, 1.0];
const NEG_Y: [f64; 3] = [0.0, -1.0, 0.0];
const NEG_Z: [f64; 3] = [0.0, 0.0, -1.0];

// ── The cases ───────────────────────────────────────────────────────────────

/// One authored configuration of one cell: two operands in an exact contact
/// of the cell's class between a face of kind `pair.a` on operand A and a face
/// of kind `pair.b` on operand B.
#[derive(Debug, Clone)]
pub struct ContactCase {
    pub pair: SurfacePair,
    pub class: ContactClass,
    /// Lowercase name of this configuration within its cell (no underscore).
    pub variant: &'static str,
    /// What the contact is and why it is exact.
    pub what: &'static str,
    pub a: Operand,
    pub b: Operand,
    /// The surface of kind `pair.a`, on operand A, in the contact.
    pub surf_a: AnalyticSurface,
    /// The surface of kind `pair.b`, on operand B, in the contact.
    pub surf_b: AnalyticSurface,
    /// Sample points of the contact locus (one for a point contact).
    pub locus: Vec<[f64; 3]>,
    pub arrangement: Arrangement,
}

/// The operations run on a case: the three booleans `A ∪ B`, `A − B`,
/// `A ∩ B`, and the setup step of a trimmed operand on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContactOp {
    Union,
    Cut,
    Intersect,
    SetupA,
    SetupB,
}

impl ContactOp {
    pub const BOOLEANS: [ContactOp; 3] = [ContactOp::Union, ContactOp::Cut, ContactOp::Intersect];

    pub const fn slug(self) -> &'static str {
        match self {
            ContactOp::Union => "union",
            ContactOp::Cut => "cut",
            ContactOp::Intersect => "intersect",
            ContactOp::SetupA => "setupa",
            ContactOp::SetupB => "setupb",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        [
            ContactOp::Union,
            ContactOp::Cut,
            ContactOp::Intersect,
            ContactOp::SetupA,
            ContactOp::SetupB,
        ]
        .into_iter()
        .find(|o| o.slug() == s)
    }

    pub const fn is_setup(self) -> bool {
        matches!(self, ContactOp::SetupA | ContactOp::SetupB)
    }
}

impl ContactCase {
    /// `cm_<pair index>_<a>_<b>_<class>_<variant>` — the case without its
    /// operation.
    pub fn key(&self) -> String {
        format!(
            "cm_{:02}_{}_{}_{}_{}",
            self.pair.index,
            self.pair.a.slug(),
            self.pair.b.slug(),
            self.class.slug(),
            self.variant
        )
    }

    /// Deterministic case id for one operation.
    pub fn case_id(&self, op: ContactOp) -> String {
        format!("{}_{}", self.key(), op.slug())
    }

    /// The operations this case runs: the three booleans, then the setup of
    /// each trimmed operand.
    pub fn ops(&self) -> Vec<ContactOp> {
        let mut ops = ContactOp::BOOLEANS.to_vec();
        if self.a.trim.is_some() {
            ops.push(ContactOp::SetupA);
        }
        if self.b.trim.is_some() {
            ops.push(ContactOp::SetupB);
        }
        ops
    }

    /// Whether the regularized result of `op` is empty by construction: the
    /// intersection of two solids that only kiss, the cut of two identical
    /// solids. (Module doc, "Expected results".)
    pub fn expects_empty(&self, op: ContactOp) -> bool {
        matches!(
            (self.arrangement, op),
            (Arrangement::Kiss, ContactOp::Intersect) | (Arrangement::Identical, ContactOp::Cut)
        )
    }

    /// The command that replays one operation of this case.
    pub fn repro(&self, op: ContactOp) -> String {
        format!(
            "ASSAY_CASE={} cargo test --release -p test-harness --test assay_contact_matrix single_case -- --ignored --exact --nocapture",
            self.case_id(op)
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn case(
    pair_index: usize,
    class: ContactClass,
    variant: &'static str,
    what: &'static str,
    a: impl Into<Operand>,
    b: impl Into<Operand>,
    surf_a: AnalyticSurface,
    surf_b: AnalyticSurface,
    locus: &[[f64; 3]],
    arrangement: Arrangement,
) -> ContactCase {
    ContactCase {
        pair: ALL_SURFACE_PAIRS[pair_index],
        class,
        variant,
        what,
        a: a.into(),
        b: b.into(),
        surf_a,
        surf_b,
        locus: locus.to_vec(),
        arrangement,
    }
}

// Pair indices of `ALL_SURFACE_PAIRS`, named for the case table.
const PP: usize = 0; // plane × plane
const PC: usize = 1; // plane × cylinder
const PK: usize = 2; // plane × cone
const PS: usize = 3; // plane × sphere
const PT: usize = 4; // plane × torus
const CC: usize = 5; // cylinder × cylinder
const CK: usize = 6; // cylinder × cone
const CS: usize = 7; // cylinder × sphere
const CT: usize = 8; // cylinder × torus
const KK: usize = 9; // cone × cone
const KS: usize = 10; // cone × sphere
const KT: usize = 11; // cone × torus
const SS: usize = 12; // sphere × sphere
const ST: usize = 13; // sphere × torus
const TT: usize = 14; // torus × torus

/// Sample points of the circle of radius `r` about the vertical axis at
/// height `z`, at azimuths that avoid `+x` and `−y` (the seam sides of the
/// bodies the cases use). `0.6` and `0.8` are the 3-4-5 direction.
fn ring_z(r: f64, z: f64) -> [[f64; 3]; 4] {
    [
        [0.0, r, z],
        [-r, 0.0, z],
        [0.6 * r, 0.8 * r, z],
        [-0.6 * r, 0.8 * r, z],
    ]
}

/// Sample points of the circle of radius `r` about the `y` axis at `y`.
fn ring_y(r: f64, y: f64) -> [[f64; 3]; 4] {
    [
        [0.0, y, r],
        [0.0, y, -r],
        [-r, y, 0.0],
        [0.6 * r, y, 0.8 * r],
    ]
}

/// The cylinder of radius 1.25 about the `z` axis, `z ∈ [0, 2]`, with a flat
/// at `x = 0.75`: the ruling edges are at `(0.75, ±1)` (the 3-4-5 points of
/// the circle) and its corners at `(0.75, ±1, 0)` and `(0.75, ±1, 2)`.
fn flatted_cylinder() -> Operand {
    trimmed(
        cyl([0.0; 3], Z, 1.25, 2.0),
        TrimOp::Intersect,
        blk([-2.0, -2.0, -1.0], [0.75, 2.0, 3.0]),
    )
}

/// The sphere of radius 1.25 about the origin with the cap `y < −1` cut off:
/// a flat disc at `y = −1` whose rim is the circle of radius 0.75.
fn flatted_sphere() -> Operand {
    trimmed(
        sphere([0.0; 3], 1.25),
        TrimOp::Intersect,
        blk([-2.0, -1.0, -2.0], [2.0, 2.0, 2.0]),
    )
}

/// The reference torus cut by the two vertical planes `x = -0.625` and
/// `y = -1.5`, keeping `x >= -0.625`, `y <= -1.5`: the planes' common line
/// pierces the torus at the corners `(-0.625, -1.5, ±0.5)` (the 5-12-13 point
/// of the parallel of radius 1.625). The kept side is `y <= -1.5` so that the
/// face in the plane `x = -0.625` stops short of `(-0.625, 0, 0)`, where that
/// plane touches the inner equator.
fn cornered_torus() -> Operand {
    trimmed(
        torus_t(),
        TrimOp::Intersect,
        blk([-0.625, -3.0, -1.0], [3.0, -1.5, 1.0]),
    )
}

/// The reference torus cut off above `z = h`.
fn topped_torus(h: f64) -> Operand {
    trimmed(
        torus_t(),
        TrimOp::Intersect,
        blk([-2.5, -2.5, -1.0], [2.5, 2.5, h]),
    )
}

/// Every authored case, in pair order then class order.
#[allow(clippy::too_many_lines)]
pub fn all_contact_cases() -> Vec<ContactCase> {
    use Arrangement::{Identical, Kiss, Nested, Overlap};
    use ContactClass::{
        Coincident, EdgeInFace, SharedEdge, TangentCurve, TangentPoint, VertexOnFace,
    };

    let k13 = cone_k(1.0, 3.0, -1.0);
    let mut cases = vec![
        // ── coincident surfaces ────────────────────────────────────────────
        case(
            PP,
            Coincident,
            "overlap",
            "Two blocks overlap and their top faces lie in the one plane z = 2 (both authored at z = 2), overlapping on x in [1,2], y in [0.5,1.5]; no other pair of faces is coplanar.",
            blk([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
            blk([1.0, 0.5, 0.5], [3.0, 1.5, 2.0]),
            plane_surf([0.0, 0.0, 2.0], Z),
            plane_surf([0.0, 0.0, 2.0], Z),
            &[
                [1.25, 0.75, 2.0],
                [1.75, 1.25, 2.0],
                [1.5, 1.0, 2.0],
                [1.25, 1.25, 2.0],
            ],
            Overlap,
        ),
        case(
            PP,
            Coincident,
            "abut",
            "A boss abuts a block: the boss's face x = 2 lies inside the block's face x = 2 (both authored at x = 2), the solids on opposite sides; no other pair of faces is coplanar.",
            blk([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
            blk([2.0, 0.5, 0.5], [4.0, 1.5, 1.5]),
            plane_surf([2.0, 0.0, 0.0], X),
            plane_surf([2.0, 0.0, 0.0], X),
            &[
                [2.0, 1.0, 1.0],
                [2.0, 0.75, 0.75],
                [2.0, 1.25, 0.75],
                [2.0, 0.75, 1.25],
            ],
            Kiss,
        ),
        case(
            CC,
            Coincident,
            "overlap",
            "Two coaxial cylinders of the same radius 1 about the z axis, z in [0,2] and z in [1,3]: the lateral faces are one cylinder on z in [1,2]. The caps are at four different heights.",
            cyl([0.0; 3], Z, 1.0, 2.0),
            cyl([0.0, 0.0, 3.0], NEG_Z, 1.0, 2.0),
            cyl_surf([0.0; 3], Z, 1.0),
            cyl_surf([0.0; 3], Z, 1.0),
            &[
                [1.0, 0.0, 1.5],
                [-1.0, 0.0, 1.25],
                [0.6, 0.8, 1.75],
                [-0.6, -0.8, 1.5],
            ],
            Overlap,
        ),
        case(
            CC,
            Coincident,
            "pinhole",
            "A pin of radius 1 in a hole of radius 1 on the same axis (the block is drilled first, the pin is longer than the hole): hole wall and pin surface are one cylinder on z in [0,2], the solids on opposite sides.",
            trimmed(
                blk([-2.0, -2.0, 0.0], [2.0, 2.0, 2.0]),
                TrimOp::Cut,
                cyl([0.0, 0.0, -1.0], Z, 1.0, 4.0),
            ),
            cyl([0.0, 0.0, 2.5], NEG_Z, 1.0, 3.0),
            cyl_surf([0.0; 3], Z, 1.0),
            cyl_surf([0.0; 3], Z, 1.0),
            &[
                [1.0, 0.0, 1.0],
                [-1.0, 0.0, 0.5],
                [0.6, 0.8, 1.5],
                [-0.6, -0.8, 1.0],
            ],
            Kiss,
        ),
        case(
            KK,
            Coincident,
            "overlap",
            "Two frusta of the one cone r = 0.75 z (same apex, axis and 3-4-5 slope), z in [1,3] and z in [2,4]: the lateral faces are one cone on z in [2,3]. The caps are at four different heights.",
            k13.clone(),
            cone_k(2.0, 4.0, 1.0),
            surf_k(),
            surf_k(),
            &[
                [0.0, 1.875, 2.5],
                [0.0, -1.875, 2.5],
                [0.0, 1.6875, 2.25],
                [0.0, -2.0625, 2.75],
            ],
            Overlap,
        ),
        case(
            SS,
            Coincident,
            "trimmed",
            "A sphere of radius 1.25 and the same sphere cut off below y = -0.5 (a setup boolean): the trimmed body's spherical face lies on the full sphere, B inside A.",
            sphere([0.0; 3], 1.25),
            trimmed(
                sphere([0.0; 3], 1.25),
                TrimOp::Intersect,
                blk([-2.0, -0.5, -2.0], [2.0, 2.0, 2.0]),
            ),
            sphere_surf([0.0; 3], 1.25),
            sphere_surf([0.0; 3], 1.25),
            &[
                [0.0, 1.25, 0.0],
                [-1.25, 0.0, 0.0],
                [0.0, 0.75, 1.0],
                [-0.75, 1.0, 0.0],
            ],
            Nested,
        ),
        case(
            SS,
            Coincident,
            "identical",
            "Two spheres with the same centre and the same radius 1.25: the two solids are the same point set and their one face is the same surface.",
            sphere([0.0; 3], 1.25),
            sphere([0.0; 3], 1.25),
            sphere_surf([0.0; 3], 1.25),
            sphere_surf([0.0; 3], 1.25),
            &[
                [0.0, 1.25, 0.0],
                [-1.25, 0.0, 0.0],
                [0.0, 0.75, 1.0],
                [-0.75, 1.0, 0.0],
            ],
            Identical,
        ),
        case(
            TT,
            Coincident,
            "trimmed",
            "The reference torus and the same torus cut off at x < -0.5 (a setup boolean): the trimmed body's toroidal face lies on the full torus, B inside A.",
            torus_t(),
            trimmed(
                torus_t(),
                TrimOp::Intersect,
                blk([-0.5, -3.0, -1.0], [3.0, 3.0, 1.0]),
            ),
            surf_t(),
            surf_t(),
            &[
                [0.0, 1.25, 0.625],
                [0.0, -1.25, 0.625],
                [0.0, 1.625, 0.5],
                [0.0, -0.875, 0.5],
            ],
            Nested,
        ),
        case(
            TT,
            Coincident,
            "identical",
            "Two tori with the same centre, axis and radii: the two solids are the same point set and their one face is the same surface.",
            torus_t(),
            torus_t(),
            surf_t(),
            surf_t(),
            &[
                [0.0, 1.25, 0.625],
                [0.0, -1.25, 0.625],
                [0.0, 1.625, 0.5],
                [0.0, -0.875, 0.5],
            ],
            Identical,
        ),
        // ── tangency along a curve ─────────────────────────────────────────
        case(
            PC,
            TangentCurve,
            "kiss",
            "A block's face x = 1 touches a cylinder of radius 1 about the z axis along the ruling (1, 0, z), z in [0.5,1.5]: the face offset equals the radius.",
            blk([1.0, -1.5, 0.5], [3.0, 1.5, 1.5]),
            cyl([0.0; 3], Z, 1.0, 2.0),
            plane_surf([1.0, 0.0, 0.0], X),
            cyl_surf([0.0; 3], Z, 1.0),
            &[[1.0, 0.0, 0.75], [1.0, 0.0, 1.0], [1.0, 0.0, 1.25]],
            Kiss,
        ),
        case(
            PC,
            TangentCurve,
            "through",
            "A cylinder of radius 1 runs through a plate of thickness 2 at mid-height (the corpus F0060 contact): it touches the plate's faces z = 0 and z = 2 from inside along the lines (x, 0, 0) and (x, 0, 2), the diameter equal to the thickness.",
            blk([-1.5, -2.0, 0.0], [1.5, 2.0, 2.0]),
            cyl([-3.0, 0.0, 1.0], X, 1.0, 6.0),
            plane_surf([0.0; 3], Z),
            cyl_surf([0.0, 0.0, 1.0], X, 1.0),
            &[[-1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            Overlap,
        ),
        case(
            PK,
            TangentCurve,
            "kiss",
            "A prism's slanted face lies in the tangent plane 4x - 3z = 0 of the cone r = 0.75 z and touches it along the generator x = 0.75 z, y = 0, z in [1.5,2.5]: the face's edge has its two ends on that generator.",
            prism_y(
                &[(1.125, 1.5), (3.0, 1.5), (3.0, 2.5), (1.875, 2.5)],
                -1.0,
                1.0,
            ),
            k13.clone(),
            plane_surf([1.125, 0.0, 1.5], [0.8, 0.0, -0.6]),
            surf_k(),
            &[[1.5, 0.0, 2.0], [1.3125, 0.0, 1.75], [1.6875, 0.0, 2.25]],
            Kiss,
        ),
        case(
            PT,
            TangentCurve,
            "kiss",
            "A block rests on the reference torus: its face z = 0.625 (the minor radius) touches the torus along the whole upper crown circle of radius 1.25.",
            blk([-2.5, -2.5, 0.625], [2.5, 2.5, 1.5]),
            torus_t(),
            plane_surf([0.0, 0.0, 0.625], Z),
            surf_t(),
            &[
                [0.0, 1.25, 0.625],
                [0.0, -1.25, 0.625],
                [-1.25, 0.0, 0.625],
                [0.75, 1.0, 0.625],
            ],
            Kiss,
        ),
        case(
            CC,
            TangentCurve,
            "kiss",
            "Two cylinders with parallel axes 1.5 apart and radii 1 and 0.5 (1 + 0.5 = 1.5) touch from outside along the ruling (1, 0, z), z in [0.5,1.5].",
            cyl([0.0; 3], Z, 1.0, 2.0),
            cyl([1.5, 0.0, 0.5], Z, 0.5, 1.0),
            cyl_surf([0.0; 3], Z, 1.0),
            cyl_surf([1.5, 0.0, 0.0], Z, 0.5),
            &[[1.0, 0.0, 0.75], [1.0, 0.0, 1.0], [1.0, 0.0, 1.25]],
            Kiss,
        ),
        case(
            CC,
            TangentCurve,
            "internal",
            "A cylinder of radius 0.5 runs through a cylinder of radius 1, the parallel axes 0.5 apart (1 - 0.5 = 0.5): it touches the outer wall from inside along the ruling (1, 0, z), z in [0,2].",
            cyl([0.0; 3], Z, 1.0, 2.0),
            cyl([0.5, 0.0, -1.0], Z, 0.5, 4.0),
            cyl_surf([0.0; 3], Z, 1.0),
            cyl_surf([0.5, 0.0, 0.0], Z, 0.5),
            &[[1.0, 0.0, 0.5], [1.0, 0.0, 1.0], [1.0, 0.0, 1.5]],
            Overlap,
        ),
        case(
            CC,
            TangentCurve,
            "pocket",
            "The same internal tangency with the inner cylinder entering from the top and stopping at z = 0.5 (the corpus C0056 contact): it touches the outer wall from inside along the ruling (1, 0, z), z in [0.5,2].",
            cyl([0.0; 3], Z, 1.0, 2.0),
            cyl([0.5, 0.0, 0.5], Z, 0.5, 2.5),
            cyl_surf([0.0; 3], Z, 1.0),
            cyl_surf([0.5, 0.0, 0.0], Z, 0.5),
            &[[1.0, 0.0, 0.75], [1.0, 0.0, 1.25], [1.0, 0.0, 1.75]],
            Overlap,
        ),
        case(
            CK,
            TangentCurve,
            "kiss",
            "A cylinder of radius 1 whose axis is parallel to a generator of the cone r = 0.75 z, at distance 1 along the cone's outward normal (0.8, 0, -0.6): cylinder and cone share the tangent plane 4x - 3z = 0 along the generator from (1.125, 0, 1.5) to (1.875, 0, 2.5). The cylinder's sketch plane has the 3-4-5 normal (0.6, 0, 0.8).",
            Prim::Extrude {
                origin: [1.125, 0.0, 1.5],
                normal: [0.6, 0.0, 0.8],
                profile: Profile::Circle {
                    cu: 0.0,
                    cv: -1.0,
                    r: 1.0,
                },
                depth: 1.25,
            },
            k13.clone(),
            cyl_surf([1.925, 0.0, 0.9], [0.6, 0.0, 0.8], 1.0),
            surf_k(),
            &[[1.5, 0.0, 2.0], [1.3125, 0.0, 1.75], [1.6875, 0.0, 2.25]],
            Kiss,
        ),
        case(
            CS,
            TangentCurve,
            "nested",
            "A sphere of radius 1 centred on the axis of a cylinder of radius 1: the sphere lies inside the cylinder and touches its wall along the equator circle z = 0.",
            cyl([0.0, 0.0, -2.0], Z, 1.0, 4.0),
            sphere([0.0; 3], 1.0),
            cyl_surf([0.0; 3], Z, 1.0),
            sphere_surf([0.0; 3], 1.0),
            &[
                [0.0, 1.0, 0.0],
                [-1.0, 0.0, 0.0],
                [-0.6, 0.8, 0.0],
                [-0.6, -0.8, 0.0],
            ],
            Nested,
        ),
        case(
            CT,
            TangentCurve,
            "kiss",
            "A cylinder of radius 0.625 on the torus's axis fills the hole of the reference torus (major 1.25 - minor 0.625 = 0.625) and touches it along the inner equator circle.",
            cyl([0.0, 0.0, -1.0], Z, 0.625, 2.0),
            torus_t(),
            cyl_surf([0.0; 3], Z, 0.625),
            surf_t(),
            &[
                [0.0, 0.625, 0.0],
                [-0.625, 0.0, 0.0],
                [0.375, 0.5, 0.0],
                [-0.375, -0.5, 0.0],
            ],
            Kiss,
        ),
        case(
            KK,
            TangentCurve,
            "kiss",
            "An upright cone r = 0.75 z about x = 0 and an inverted cone r = 0.75 (4 - z) about x = 3: at every height the two circles touch at x = 0.75 z (0.75 z + 0.75 (4 - z) = 3), so the cones share the generator x = 0.75 z, y = 0 and its tangent plane, z in [1.5,2.5].",
            k13.clone(),
            lathe_z(
                3.0,
                0.0,
                &[(0.0, 1.5), (1.875, 1.5), (1.125, 2.5), (0.0, 2.5)],
            ),
            surf_k(),
            cone_surf([3.0, 0.0, 4.0], NEG_Z, 0.75),
            &[[1.5, 0.0, 2.0], [1.3125, 0.0, 1.75], [1.6875, 0.0, 2.25]],
            Kiss,
        ),
        case(
            KK,
            TangentCurve,
            "internal",
            "Two cones of the same slope 0.75 with parallel axes, the inner one's apex (0.75, 0, 1) on a generator of the outer: the inner cone lies inside the outer and they share the generator x = 0.75 z, y = 0 and its tangent plane, z in [1.5,3.5].",
            cone_k(1.0, 4.0, -1.0),
            lathe_z(
                0.75,
                0.0,
                &[(0.0, 1.5), (-0.375, 1.5), (-1.875, 3.5), (0.0, 3.5)],
            ),
            surf_k(),
            cone_surf([0.75, 0.0, 1.0], Z, 0.75),
            &[[1.5, 0.0, 2.0], [1.875, 0.0, 2.5], [2.25, 0.0, 3.0]],
            Nested,
        ),
        case(
            KS,
            TangentCurve,
            "nested",
            "A sphere inscribed in the cone r = 0.75 z: centre (0, 0, 1.25) on the axis, radius 0.75 = 1.25 x 0.6 (the centre's distance to the cone). It touches the cone from inside along the circle of radius 0.6 at z = 0.8.",
            cone_k(0.25, 2.5, -1.0),
            sphere([0.0, 0.0, 1.25], 0.75),
            surf_k(),
            sphere_surf([0.0, 0.0, 1.25], 0.75),
            &[
                [0.0, 0.6, 0.8],
                [0.0, -0.6, 0.8],
                [-0.6, 0.0, 0.8],
                [0.36, 0.48, 0.8],
            ],
            Nested,
        ),
        case(
            KT,
            TangentCurve,
            "kiss",
            "The reference torus seated on a coaxial cone r = 0.75 (z + 0.625): the tube centre (1.25, 0) is 0.625 = the minor radius from the generator 4r - 3z - 1.875 = 0, so torus and cone touch along the circle of radius 0.75 at z = 0.375.",
            lathe_z(
                0.0,
                0.0,
                &[(0.0, 0.0), (-0.46875, 0.0), (-1.21875, 1.0), (0.0, 1.0)],
            ),
            torus_t(),
            cone_surf([0.0, 0.0, -0.625], Z, 0.75),
            surf_t(),
            &[
                [0.0, 0.75, 0.375],
                [0.0, -0.75, 0.375],
                [-0.75, 0.0, 0.375],
                [0.45, 0.6, 0.375],
            ],
            Kiss,
        ),
        case(
            ST,
            TangentCurve,
            "kiss",
            "A ball of radius 0.9375 resting in the reference torus, centre (0, 0, 0.9375) on the axis: the centre is 1.5625 = 0.9375 + 0.625 from the tube centre circle (the 3-4-5 triangle 0.9375, 1.25, 1.5625), so ball and torus touch along the circle of radius 0.75 at z = 0.375.",
            sphere([0.0, 0.0, 0.9375], 0.9375),
            torus_t(),
            sphere_surf([0.0, 0.0, 0.9375], 0.9375),
            surf_t(),
            &[
                [0.0, 0.75, 0.375],
                [0.0, -0.75, 0.375],
                [-0.75, 0.0, 0.375],
                [0.45, 0.6, 0.375],
            ],
            Kiss,
        ),
        case(
            TT,
            TangentCurve,
            "kiss",
            "Two equal tori stacked on one axis, centres 1.25 = twice the minor radius apart: they touch along the crown circle of radius 1.25 at z = 0.625.",
            torus_t(),
            torus_z([0.0, 0.0, 1.25], T_MAJOR, T_MINOR, -1.0),
            surf_t(),
            torus_surf([0.0, 0.0, 1.25], T_MAJOR, T_MINOR),
            &[
                [0.0, 1.25, 0.625],
                [0.0, -1.25, 0.625],
                [-1.25, 0.0, 0.625],
                [0.75, 1.0, 0.625],
            ],
            Kiss,
        ),
        // ── tangency at a point ────────────────────────────────────────────
        case(
            PS,
            TangentPoint,
            "kiss",
            "A block's face y = -1 touches a sphere of radius 1 about the origin at the one point (0, -1, 0): the face offset equals the radius.",
            blk([-1.5, -3.0, -1.5], [1.5, -1.0, 1.5]),
            sphere([0.0; 3], 1.0),
            plane_surf([0.0, -1.0, 0.0], Y),
            sphere_surf([0.0; 3], 1.0),
            &[[0.0, -1.0, 0.0]],
            Kiss,
        ),
        case(
            PT,
            TangentPoint,
            "kiss",
            "A prism's slanted face lies in the tangent plane of the reference torus at the tube point (0, -1.625, 0.5), whose outward normal is the 3-4-5 direction (0, -0.6, 0.8): the face's edge passes through that point along (0.8, 0.6) in the y-z plane. The plane supports the torus, so the contact is that one point.",
            prism_x(
                &[(-2.625, -0.25), (-0.625, 1.25), (-2.625, 1.25)],
                -1.0,
                1.0,
            ),
            torus_t(),
            plane_surf([0.0, -1.625, 0.5], [0.0, -0.6, 0.8]),
            surf_t(),
            &[[0.0, -1.625, 0.5]],
            Kiss,
        ),
        case(
            PT,
            TangentPoint,
            "saddle",
            "A block's face y = -0.625 touches the reference torus at the inner-equator point (0, -0.625, 0), a saddle point: the plane cuts the torus in a figure-eight whose crossing is the tangent point. The block overlaps the torus.",
            blk([-2.5, -0.625, -1.5], [2.5, 2.5, 1.5]),
            torus_t(),
            plane_surf([0.0, -0.625, 0.0], Y),
            surf_t(),
            &[[0.0, -0.625, 0.0]],
            Overlap,
        ),
        case(
            CC,
            TangentPoint,
            "kiss",
            "Two cylinders of radius 1 with perpendicular axes (the z axis, and the line x = 2, z = 0) 2 = 1 + 1 apart touch from outside at the one point (1, 0, 0).",
            cyl([0.0, 0.0, -2.0], Z, 1.0, 4.0),
            cyl([2.0, 2.0, 0.0], NEG_Y, 1.0, 4.0),
            cyl_surf([0.0; 3], Z, 1.0),
            cyl_surf([2.0, 0.0, 0.0], Y, 1.0),
            &[[1.0, 0.0, 0.0]],
            Kiss,
        ),
        case(
            CC,
            TangentPoint,
            "steinmetz",
            "Two cylinders of the same radius 1 whose axes (the x axis and the y axis) cross at right angles (the corpus F0059 contact): the surfaces are tangent at (0, 0, 1) and (0, 0, -1), where the two intersection ellipses cross.",
            cyl([-2.0, 0.0, 0.0], X, 1.0, 4.0),
            cyl([0.0, 2.0, 0.0], NEG_Y, 1.0, 4.0),
            cyl_surf([0.0; 3], X, 1.0),
            cyl_surf([0.0; 3], Y, 1.0),
            &[[0.0, 0.0, 1.0], [0.0, 0.0, -1.0]],
            Overlap,
        ),
        case(
            CK,
            TangentPoint,
            "kiss",
            "A cylinder of radius 1.25 with axis parallel to y through (2.5, ., 1.25) touches the cone r = 0.75 z at (1.5, 0, 2): the axis is 1.25 along the cone's outward normal (0.8, 0, -0.6) from that point, and the cylinder's ruling there crosses the cone's generator.",
            cyl([2.5, 1.5, 1.25], NEG_Y, 1.25, 3.0),
            k13.clone(),
            cyl_surf([2.5, 0.0, 1.25], Y, 1.25),
            surf_k(),
            &[[1.5, 0.0, 2.0]],
            Kiss,
        ),
        case(
            CS,
            TangentPoint,
            "kiss",
            "A sphere of radius 0.75 centred 1.75 = 1 + 0.75 from the axis of a cylinder of radius 1 touches it from outside at the one point (0, 1, 0).",
            cyl([0.0, 0.0, -2.0], Z, 1.0, 4.0),
            sphere([0.0, 1.75, 0.0], 0.75),
            cyl_surf([0.0; 3], Z, 1.0),
            sphere_surf([0.0, 1.75, 0.0], 0.75),
            &[[0.0, 1.0, 0.0]],
            Kiss,
        ),
        case(
            CT,
            TangentPoint,
            "kiss",
            "A pin of radius 0.25 in the hole of the reference torus, its axis 0.375 = 0.625 - 0.25 from the torus's axis: it touches the hole wall at the one inner-equator point (-0.625, 0, 0).",
            cyl([-0.375, 0.0, -1.0], Z, 0.25, 2.0),
            torus_t(),
            cyl_surf([-0.375, 0.0, 0.0], Z, 0.25),
            surf_t(),
            &[[-0.625, 0.0, 0.0]],
            Kiss,
        ),
        case(
            KK,
            TangentPoint,
            "kiss",
            "A second cone of slope 0.75 lies on the tangent plane 4x - 3z = 0 of the cone r = 0.75 z, on the other side, along its own generator through (1.5, 0, 2) in the y direction: the two generators cross there, so the cones touch at that one point. The second cone's apex is (1.5, -1.25, 2) and its axis the 3-4-5 direction (12, 20, -9)/25; its profile is dyadic in its own sketch plane.",
            k13.clone(),
            Prim::Revolve {
                origin: [1.5, -1.25, 2.0],
                normal: [0.6, 0.0, 0.8],
                profile: Profile::Polygon(ccw(vec![
                    (0.3125, -0.234375),
                    (1.25, -0.9375),
                    (0.546875, -1.875),
                    (0.13671875, -0.46875),
                ])),
                axis_origin: [1.5, -1.25, 2.0],
                axis_dir: [0.48, 0.8, -0.36],
                angle: 360.0,
            },
            surf_k(),
            cone_surf([1.5, -1.25, 2.0], [0.48, 0.8, -0.36], 0.75),
            &[[1.5, 0.0, 2.0]],
            Kiss,
        ),
        case(
            KS,
            TangentPoint,
            "kiss",
            "A sphere of radius 1.25 centred 1.25 along the outward normal (0, -0.8, -0.6) of the cone r = 0.75 z from its point (0, -1.5, 2) touches the cone from outside at that one point.",
            k13.clone(),
            sphere([0.0, -2.5, 1.25], 1.25),
            surf_k(),
            sphere_surf([0.0, -2.5, 1.25], 1.25),
            &[[0.0, -1.5, 2.0]],
            Kiss,
        ),
        case(
            KT,
            TangentPoint,
            "kiss",
            "An upright cone of slope 0.75 about the line x = 0, y = -3.25 (apex at z = -1.625) passes through the tube point (0, -1.75, 0.375) of the reference torus with the opposite normal (0, 0.8, -0.6): the common tangent plane supports the torus at that one point and holds the cone's generator.",
            lathe_z(
                0.0,
                -3.25,
                &[(0.0, -0.625), (0.75, -0.625), (2.25, 1.375), (0.0, 1.375)],
            ),
            torus_t(),
            cone_surf([0.0, -3.25, -1.625], Z, 0.75),
            surf_t(),
            &[[0.0, -1.75, 0.375]],
            Kiss,
        ),
        case(
            SS,
            TangentPoint,
            "kiss",
            "Two spheres of radius 1 and 0.75 with centres 1.75 apart touch from outside at the one point (0, -1, 0).",
            sphere([0.0; 3], 1.0),
            sphere([0.0, -1.75, 0.0], 0.75),
            sphere_surf([0.0; 3], 1.0),
            sphere_surf([0.0, -1.75, 0.0], 0.75),
            &[[0.0, -1.0, 0.0]],
            Kiss,
        ),
        case(
            ST,
            TangentPoint,
            "kiss",
            "A sphere of radius 0.625 centred 0.625 along the outward normal (0, -0.8, 0.6) of the reference torus from its tube point (0, -1.75, 0.375) touches the torus from outside at that one point.",
            sphere([0.0, -2.25, 0.75], 0.625),
            torus_t(),
            sphere_surf([0.0, -2.25, 0.75], 0.625),
            surf_t(),
            &[[0.0, -1.75, 0.375]],
            Kiss,
        ),
        case(
            TT,
            TangentPoint,
            "kiss",
            "Two equal tori with parallel axes, the second centred (0, -3.5, 0.75): its tube point with normal (0, 0.8, -0.6) is the first's tube point (0, -1.75, 0.375) with normal (0, -0.8, 0.6). The common tangent plane supports both tori at that one point.",
            torus_t(),
            torus_z([0.0, -3.5, 0.75], T_MAJOR, T_MINOR, 1.0),
            surf_t(),
            torus_surf([0.0, -3.5, 0.75], T_MAJOR, T_MINOR),
            &[[0.0, -1.75, 0.375]],
            Kiss,
        ),
        // ── an edge lying in a face ────────────────────────────────────────
        case(
            PP,
            EdgeInFace,
            "through",
            "A triangular prism's edge (1, y, 2), y in [0.5,1.5], lies in a block's face z = 2: one of the prism's faces at that edge goes into the block, the other out.",
            blk([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
            prism_y(&[(1.0, 2.0), (1.5, 1.5), (1.5, 2.5)], 0.5, 1.5),
            plane_surf([0.0, 0.0, 2.0], Z),
            plane_surf([1.0, 0.0, 2.0], [-1.0, 0.0, -1.0]),
            &[[1.0, 0.75, 2.0], [1.0, 1.0, 2.0], [1.0, 1.25, 2.0]],
            Overlap,
        ),
        case(
            PC,
            EdgeInFace,
            "through",
            "A triangular prism's edge (1, 0, z), z in [0.5,1.5], is a ruling of a cylinder of radius 1 about the z axis: one of the prism's faces at that edge goes into the cylinder, the other out, neither tangent.",
            prism_z(&[(1.0, 0.0), (2.0, 0.5), (0.0, 0.5)], 0.5, 1.5),
            cyl([0.0; 3], Z, 1.0, 2.0),
            plane_surf([1.0, 0.0, 0.0], [0.5, -1.0, 0.0]),
            cyl_surf([0.0; 3], Z, 1.0),
            &[[1.0, 0.0, 0.75], [1.0, 0.0, 1.0], [1.0, 0.0, 1.25]],
            Overlap,
        ),
        case(
            PK,
            EdgeInFace,
            "through",
            "A spindle (two cones base to base) has its rim circle of radius 0.75 at z = 1 in a block's face z = 1: the lower cone r = 0.75 z goes into the block, the upper cone out.",
            blk([-1.5, -1.5, 0.0], [1.5, 1.5, 1.0]),
            lathe_z(
                0.0,
                0.0,
                &[
                    (0.0, 0.5),
                    (-0.375, 0.5),
                    (-0.75, 1.0),
                    (-0.375, 1.5),
                    (0.0, 1.5),
                ],
            ),
            plane_surf([0.0, 0.0, 1.0], Z),
            surf_k(),
            &ring_z(0.75, 1.0),
            Overlap,
        ),
        case(
            PS,
            EdgeInFace,
            "through",
            "A frustum's cap rim, the circle of radius 0.75 in the plane y = -1, lies in a sphere of radius 1.25 about the origin (0.75, 1, 1.25 is a 3-4-5 triangle): the cap disc is inside the ball, the frustum's cone outside.",
            lathe_y(
                0.0,
                0.0,
                &[(-2.0, 0.0), (-2.0, 0.25), (-1.0, 0.75), (-1.0, 0.0)],
            ),
            sphere([0.0; 3], 1.25),
            plane_surf([0.0, -1.0, 0.0], Y),
            sphere_surf([0.0; 3], 1.25),
            &ring_y(0.75, -1.0),
            Overlap,
        ),
        case(
            PT,
            EdgeInFace,
            "through",
            "A frustum's cap rim, the circle of radius 1.625 at z = 0.5, is a parallel of the reference torus (1.25 + 0.375, 0.5): the cap plane and the frustum's cone both cross the torus there.",
            lathe_z(
                0.0,
                0.0,
                &[(0.0, -0.5), (-1.125, -0.5), (-1.625, 0.5), (0.0, 0.5)],
            ),
            torus_t(),
            plane_surf([0.0, 0.0, 0.5], Z),
            surf_t(),
            &[
                [0.0, 1.625, 0.5],
                [0.0, -1.625, 0.5],
                [-1.625, 0.0, 0.5],
                [0.975, 1.3, 0.5],
            ],
            Overlap,
        ),
        case(
            CC,
            EdgeInFace,
            "through",
            "A cylinder of radius 1.25 with a flat at x = 0.75 (a setup boolean) has its two ruling edges (0.75, +-1, z) on a second cylinder of radius 1.25 about x = 1.5: x = 0.75 is the radical plane of the two circles, which meet at the 3-4-5 points (0.75, +-1).",
            flatted_cylinder(),
            cyl([1.5, 0.0, 0.5], Z, 1.25, 1.0),
            cyl_surf([0.0; 3], Z, 1.25),
            cyl_surf([1.5, 0.0, 0.0], Z, 1.25),
            &[
                [0.75, 1.0, 0.75],
                [0.75, 1.0, 1.0],
                [0.75, -1.0, 1.0],
                [0.75, -1.0, 1.25],
            ],
            Overlap,
        ),
        case(
            CK,
            EdgeInFace,
            "through",
            "A cylinder of radius 0.75, z in [0,1], has its top rim on the coaxial cone r = 0.75 z at z = 1 (0.75 x 1 = 0.75): the cylinder's wall is outside the cone below the rim, its cap disc inside.",
            cyl([0.0; 3], Z, 0.75, 1.0),
            cone_k(0.5, 3.0, 1.0),
            cyl_surf([0.0; 3], Z, 0.75),
            surf_k(),
            &ring_z(0.75, 1.0),
            Overlap,
        ),
        case(
            CS,
            EdgeInFace,
            "through",
            "A cylinder of radius 0.75 about the y axis ends at y = -1 with its cap rim on a sphere of radius 1.25 about the origin (the 3-4-5 triangle 0.75, 1, 1.25): the cap disc is inside the ball, the cylinder's wall outside.",
            cyl([0.0, -1.0, 0.0], NEG_Y, 0.75, 1.5),
            sphere([0.0; 3], 1.25),
            cyl_surf([0.0; 3], Y, 0.75),
            sphere_surf([0.0; 3], 1.25),
            &ring_y(0.75, -1.0),
            Overlap,
        ),
        case(
            CT,
            EdgeInFace,
            "through",
            "A cylinder of radius 1.625 coaxial with the reference torus ends at z = 0.5 with its cap rim on the torus's parallel (1.25 + 0.375, 0.5): wall and cap both cross the torus there.",
            cyl([0.0, 0.0, -1.5], Z, 1.625, 2.0),
            torus_t(),
            cyl_surf([0.0; 3], Z, 1.625),
            surf_t(),
            &[
                [0.0, 1.625, 0.5],
                [-1.625, 0.0, 0.5],
                [0.975, 1.3, 0.5],
                [-0.975, 1.3, 0.5],
            ],
            Overlap,
        ),
        case(
            KK,
            EdgeInFace,
            "through",
            "A frustum r = 0.25 + 0.5 z, z in [0,1], has its top rim of radius 0.75 on the coaxial cone r = 0.75 z at z = 1: its cone is outside the other below the rim, its cap disc inside.",
            lathe_z(
                0.0,
                0.0,
                &[(0.0, 0.0), (-0.25, 0.0), (-0.75, 1.0), (0.0, 1.0)],
            ),
            cone_k(0.5, 3.0, 1.0),
            cone_surf([0.0, 0.0, -0.5], Z, 0.5),
            surf_k(),
            &ring_z(0.75, 1.0),
            Overlap,
        ),
        case(
            KS,
            EdgeInFace,
            "through",
            "A spindle about the y axis has its rim, the circle of radius 0.75 in the plane y = -1, in a sphere of radius 1.25 about the origin: one cone goes into the ball, the other out.",
            lathe_y(
                0.0,
                0.0,
                &[
                    (-2.0, 0.0),
                    (-2.0, 0.25),
                    (-1.0, 0.75),
                    (-0.5, 0.25),
                    (-0.5, 0.0),
                ],
            ),
            sphere([0.0; 3], 1.25),
            cone_surf([0.0, -2.5, 0.0], Y, 0.5),
            sphere_surf([0.0; 3], 1.25),
            &ring_y(0.75, -1.0),
            Overlap,
        ),
        case(
            KT,
            EdgeInFace,
            "through",
            "A spindle coaxial with the reference torus has its rim circle of radius 1.625 at z = 0.5 on the torus's parallel: the lower cone goes into the tube, the upper cone out.",
            lathe_z(
                0.0,
                0.0,
                &[
                    (0.0, -0.5),
                    (-1.125, -0.5),
                    (-1.625, 0.5),
                    (-1.125, 1.5),
                    (0.0, 1.5),
                ],
            ),
            torus_t(),
            cone_surf([0.0, 0.0, -2.75], Z, 0.5),
            surf_t(),
            &[
                [0.0, 1.625, 0.5],
                [0.0, -1.625, 0.5],
                [-1.625, 0.0, 0.5],
                [0.975, 1.3, 0.5],
            ],
            Overlap,
        ),
        case(
            SS,
            EdgeInFace,
            "through",
            "A sphere of radius 1.25 cut off at y = -1 (a setup boolean) has its rim, the circle of radius 0.75, on an equal sphere centred (0, -2, 0): y = -1 is the radical plane of the two spheres.",
            flatted_sphere(),
            sphere([0.0, -2.0, 0.0], 1.25),
            sphere_surf([0.0; 3], 1.25),
            sphere_surf([0.0, -2.0, 0.0], 1.25),
            &ring_y(0.75, -1.0),
            Overlap,
        ),
        case(
            ST,
            EdgeInFace,
            "segment",
            "An end circle of the half torus (the reference torus revolved 180 degrees, no boolean), radius 0.625 about (-1.25, 0, 0) in the plane y = 0, lies in a sphere of radius 1.625 centred (-1.25, 1.5, 0) (the 5-12-13 triangle 0.625, 1.5, 1.625): the torus and its flat end both cross the sphere there.",
            sphere([-1.25, 1.5, 0.0], 1.625),
            half_torus(),
            sphere_surf([-1.25, 1.5, 0.0], 1.625),
            surf_t(),
            &HALF_TORUS_RIM,
            Overlap,
        ),
        case(
            TT,
            EdgeInFace,
            "segment",
            "An end circle of the half torus (the reference torus revolved 180 degrees, no boolean), radius 0.625 about (-1.25, 0, 0) in the plane y = 0, is a parallel of a second torus about the line x = -1.25, z = 0: centre (-1.25, 0.5, 0), major radius 1, minor 0.625, the parallel at radius 1 - 0.375 and height -0.5 (3-4-5). The two tori cross there.",
            half_torus(),
            Prim::Revolve {
                origin: [-1.25, 0.5, 0.0],
                normal: X,
                profile: Profile::Circle {
                    cu: 0.0,
                    cv: 1.0,
                    r: T_MINOR,
                },
                axis_origin: [-1.25, 0.5, 0.0],
                axis_dir: Y,
                angle: 360.0,
            },
            surf_t(),
            AnalyticSurface::Torus {
                center: [-1.25, 0.5, 0.0],
                axis: Y,
                major: 1.0,
                minor: T_MINOR,
            },
            &HALF_TORUS_RIM,
            Overlap,
        ),
        // ── a vertex lying on a face ───────────────────────────────────────
        case(
            PP,
            VertexOnFace,
            "kiss",
            "A diamond prism's corner (0, 0, 0) lies on a wedge's slanted face y + z = 0: the corner's three edges (1,0,1), (-1,0,1), (0,1,0) all leave that plane on the outer side.",
            prism_y(&[(0.0, 0.0), (1.0, 1.0), (0.0, 2.0), (-1.0, 1.0)], 0.0, 2.0),
            prism_x(&[(-2.0, 2.0), (2.0, -2.0), (-2.0, -2.0)], -2.0, 2.0),
            plane_surf([0.0; 3], [1.0, 0.0, -1.0]),
            plane_surf([0.0; 3], [0.0, 1.0, 1.0]),
            &[[0.0, 0.0, 0.0]],
            Kiss,
        ),
        case(
            PC,
            VertexOnFace,
            "kiss",
            "A diamond prism's corner (0, 0, 0) lies on the top ruling of a cylinder of radius 1 about the line y = 0, z = -1: the prism is above z = 0 and meets it in a segment across the ruling, so the contact is the corner.",
            prism_y(&[(0.0, 0.0), (1.0, 1.0), (0.0, 2.0), (-1.0, 1.0)], 0.0, 2.0),
            cyl([-2.0, 0.0, -1.0], X, 1.0, 4.0),
            plane_surf([0.0; 3], [1.0, 0.0, -1.0]),
            cyl_surf([0.0, 0.0, -1.0], X, 1.0),
            &[[0.0, 0.0, 0.0]],
            Kiss,
        ),
        case(
            PK,
            VertexOnFace,
            "kiss",
            "A block's corner (1.5, 0, 2) lies on the cone r = 0.75 z (0.75 x 2 = 1.5): the block is at x >= 1.5, z <= 2, where the cone's radius is at most 1.5, so the contact is the corner.",
            blk([1.5, 0.0, 0.0], [3.5, 2.0, 2.0]),
            k13.clone(),
            plane_surf([1.5, 0.0, 0.0], X),
            surf_k(),
            &[[1.5, 0.0, 2.0]],
            Kiss,
        ),
        case(
            PK,
            VertexOnFace,
            "apex",
            "A cone stands on its apex on a block's face z = 0: the apex (0, 0, 0) is the only point of the cone r = 0.75 z, z in [0,2], in that plane.",
            blk([-1.5, -1.5, -1.0], [1.5, 1.5, 0.0]),
            lathe_z(0.0, 0.0, &[(0.0, 0.0), (-1.5, 2.0), (0.0, 2.0)]),
            plane_surf([0.0; 3], Z),
            surf_k(),
            &[[0.0, 0.0, 0.0]],
            Kiss,
        ),
        case(
            PS,
            VertexOnFace,
            "kiss",
            "A block's corner (1, 1, 0.5) lies on a sphere of radius 1.5 about the origin (1 + 1 + 0.25 = 2.25): the block is in the octant beyond the corner, so the contact is the corner, and no face of the block is tangent.",
            blk([1.0, 1.0, 0.5], [3.0, 3.0, 2.5]),
            sphere([0.0; 3], 1.5),
            plane_surf([1.0, 0.0, 0.0], X),
            sphere_surf([0.0; 3], 1.5),
            &[[1.0, 1.0, 0.5]],
            Kiss,
        ),
        case(
            PT,
            VertexOnFace,
            "kiss",
            "A block's corner (0, -1.625, 0.5) lies on the reference torus's parallel point: the block is at y <= -1.625, z >= 0.5, x >= 0, where the torus reaches only that point.",
            blk([0.0, -3.625, 0.5], [2.0, -1.625, 2.5]),
            torus_t(),
            plane_surf([0.0, -1.625, 0.0], Y),
            surf_t(),
            &[[0.0, -1.625, 0.5]],
            Kiss,
        ),
        case(
            CC,
            VertexOnFace,
            "through",
            "A flatted cylinder's corner (0.75, 1, 2) (a setup boolean) lies on a cylinder of radius 1.25 about the line y = 1.75, z = 1 (0.75, 1, 1.25 again): the corner's three edges cross that cylinder.",
            flatted_cylinder(),
            cyl([0.25, 1.75, 1.0], X, 1.25, 1.25),
            cyl_surf([0.0; 3], Z, 1.25),
            cyl_surf([0.0, 1.75, 1.0], X, 1.25),
            &[[0.75, 1.0, 2.0]],
            Overlap,
        ),
        case(
            CK,
            VertexOnFace,
            "kiss",
            "A cone stands on its apex on the top ruling of a cylinder of radius 1 about the line y = 0, z = -1: the apex (0, 0, 0) is the only point of the cone r = 0.75 z in the plane z = 0.",
            cyl([-2.0, 0.0, -1.0], X, 1.0, 4.0),
            lathe_z(0.0, 0.0, &[(0.0, 0.0), (-1.5, 2.0), (0.0, 2.0)]),
            cyl_surf([0.0, 0.0, -1.0], X, 1.0),
            surf_k(),
            &[[0.0, 0.0, 0.0]],
            Kiss,
        ),
        case(
            CS,
            VertexOnFace,
            "kiss",
            "A flatted cylinder's corner (0.75, 1, 2) (a setup boolean) lies on a sphere of radius 1.25 centred (1.5, 1, 3): the sphere dips below z = 2 only where x >= 0.75, and the trimmed cylinder is at x <= 0.75, z <= 2, so the contact is the corner.",
            flatted_cylinder(),
            sphere([1.5, 1.0, 3.0], 1.25),
            cyl_surf([0.0; 3], Z, 1.25),
            sphere_surf([1.5, 1.0, 3.0], 1.25),
            &[[0.75, 1.0, 2.0]],
            Kiss,
        ),
        case(
            CT,
            VertexOnFace,
            "through",
            "A flatted cylinder's corner (0.75, 1, 2) (a setup boolean) lies on a torus centred (-0.75, 1.625, 1.5): the corner is the parallel point at radius 1.625 (the 5-12-13 offset 1.5, -0.625) and height 0.5, and its three edges cross the torus.",
            flatted_cylinder(),
            torus_z([-0.75, 1.625, 1.5], T_MAJOR, T_MINOR, 1.0),
            cyl_surf([0.0; 3], Z, 1.25),
            torus_surf([-0.75, 1.625, 1.5], T_MAJOR, T_MINOR),
            &[[0.75, 1.0, 2.0]],
            Overlap,
        ),
        case(
            KK,
            VertexOnFace,
            "kiss",
            "A cone's apex (0, -1.5, 2) lies on the cone r = 0.75 z; the second cone opens along -y with slope 0.75, so every generator leaves the first cone's tangent plane there on the outer side.",
            k13.clone(),
            lathe_y(0.0, 2.0, &[(-1.5, 0.0), (-3.5, 1.5), (-3.5, 0.0)]),
            surf_k(),
            cone_surf([0.0, -1.5, 2.0], NEG_Y, 0.75),
            &[[0.0, -1.5, 2.0]],
            Kiss,
        ),
        case(
            KS,
            VertexOnFace,
            "kiss",
            "A cone's apex (0, -1.25, 0) lies on a sphere of radius 1.25 about the origin; the cone opens along -y, away from the sphere.",
            lathe_y(0.0, 0.0, &[(-1.25, 0.0), (-3.25, 1.5), (-3.25, 0.0)]),
            sphere([0.0; 3], 1.25),
            cone_surf([0.0, -1.25, 0.0], NEG_Y, 0.75),
            sphere_surf([0.0; 3], 1.25),
            &[[0.0, -1.25, 0.0]],
            Kiss,
        ),
        case(
            KT,
            VertexOnFace,
            "kiss",
            "A cone's apex lies on the reference torus's tube point (0, -1.75, 0.375), where the outward normal is (0, -0.8, 0.6); the cone opens along -y with slope 0.75, so every generator leaves the torus's supporting plane there on the outer side.",
            lathe_y(0.0, 0.375, &[(-1.75, 0.0), (-3.75, 1.5), (-3.75, 0.0)]),
            torus_t(),
            cone_surf([0.0, -1.75, 0.375], NEG_Y, 0.75),
            surf_t(),
            &[[0.0, -1.75, 0.375]],
            Kiss,
        ),
        case(
            SS,
            VertexOnFace,
            "kiss",
            "A sphere of radius 1.5 cut by the planes x = 1 and z = 0.5 (a setup boolean) has the corner (1, 1, 0.5); a sphere of radius 0.75 centred (1.5, 1.25, 1) passes through it, on the far side of a plane that the trimmed sphere meets only at the corner.",
            trimmed(
                sphere([0.0; 3], 1.5),
                TrimOp::Intersect,
                blk([-2.0, -2.0, -2.0], [1.0, 2.0, 0.5]),
            ),
            sphere([1.5, 1.25, 1.0], 0.75),
            sphere_surf([0.0; 3], 1.5),
            sphere_surf([1.5, 1.25, 1.0], 0.75),
            &[[1.0, 1.0, 0.5]],
            Kiss,
        ),
        case(
            ST,
            VertexOnFace,
            "corner",
            "The reference torus cut by the vertical planes x = -0.625 and y = -1.5 (a setup boolean) has the corner (-0.625, -1.5, 0.5), the 5-12-13 point of its parallel of radius 1.625; a sphere of radius 0.75 centred (-1.125, -1.75, 0) passes through it ((0.5, 0.25, 0.5) has length 0.75), and the corner's three edges cross the sphere.",
            sphere([-1.125, -1.75, 0.0], 0.75),
            cornered_torus(),
            sphere_surf([-1.125, -1.75, 0.0], 0.75),
            surf_t(),
            &[[-0.625, -1.5, 0.5]],
            Overlap,
        ),
        case(
            TT,
            VertexOnFace,
            "kiss",
            "The reference torus cut by the vertical planes x = -0.625 and y = -1.5 (a setup boolean) has the corner (-0.625, -1.5, 0.5), the 5-12-13 point of its parallel of radius 1.625; an equal torus centred (-2.125, -2.125, 1) has it as a parallel point too (offset 1.5, 0.625 at height -0.5), and the corner's three edges all leave that torus on the outer side.",
            cornered_torus(),
            torus_z([-2.125, -2.125, 1.0], T_MAJOR, T_MINOR, 1.0),
            surf_t(),
            torus_surf([-2.125, -2.125, 1.0], T_MAJOR, T_MINOR),
            &[[-0.625, -1.5, 0.5]],
            Kiss,
        ),
        // ── a shared edge ──────────────────────────────────────────────────
        case(
            PP,
            SharedEdge,
            "through",
            "A triangular prism's edge is a block's edge (2, y, 2), y in [0.5,1.5]: one of the prism's faces there goes into the block, the other out, and neither is coplanar with a face of the block.",
            blk([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]),
            prism_y(&[(2.0, 2.0), (1.5, 1.0), (3.0, 2.5)], 0.5, 1.5),
            plane_surf([2.0, 0.0, 0.0], X),
            plane_surf([2.0, 0.0, 2.0], [1.0, 0.0, -0.5]),
            &[[2.0, 0.75, 2.0], [2.0, 1.0, 2.0], [2.0, 1.25, 2.0]],
            Overlap,
        ),
        case(
            PC,
            SharedEdge,
            "through",
            "A frustum's top rim (cap plane z = 1 and a cone) is a pencil's shoulder (a cylinder of radius 0.75 above, a cone below): the one circle of radius 0.75 at z = 1. The four faces at it are four different surfaces.",
            lathe_z(
                0.0,
                0.0,
                &[(0.0, 0.0), (-1.25, 0.0), (-0.75, 1.0), (0.0, 1.0)],
            ),
            lathe_z(
                0.0,
                0.0,
                &[
                    (0.0, 0.5),
                    (0.25, 0.5),
                    (0.75, 1.0),
                    (0.75, 2.0),
                    (0.0, 2.0),
                ],
            ),
            plane_surf([0.0, 0.0, 1.0], Z),
            cyl_surf([0.0; 3], Z, 0.75),
            &ring_z(0.75, 1.0),
            Overlap,
        ),
        case(
            PK,
            SharedEdge,
            "through",
            "A frustum's top rim (cap plane z = 1 and a cone) is a spindle's rim (two cones): the one circle of radius 0.75 at z = 1. The four faces at it are four different surfaces.",
            lathe_z(
                0.0,
                0.0,
                &[(0.0, 0.0), (-1.25, 0.0), (-0.75, 1.0), (0.0, 1.0)],
            ),
            lathe_z(
                0.0,
                0.0,
                &[
                    (0.0, 0.5),
                    (0.25, 0.5),
                    (0.75, 1.0),
                    (0.25, 2.0),
                    (0.0, 2.0),
                ],
            ),
            plane_surf([0.0, 0.0, 1.0], Z),
            cone_surf([0.0, 0.0, 0.25], Z, 1.0),
            &ring_z(0.75, 1.0),
            Overlap,
        ),
        case(
            PS,
            SharedEdge,
            "through",
            "A frustum's cap rim (cap plane y = -1 and a cone) is the rim of a ball of radius 1.25 cut to a coaxial cylinder of radius 0.75 (a setup boolean; sphere and cylinder meet at y = -1 by the 3-4-5 triangle): the one circle of radius 0.75.",
            lathe_y(
                0.0,
                0.0,
                &[(-2.0, 0.0), (-2.0, 0.25), (-1.0, 0.75), (-1.0, 0.0)],
            ),
            trimmed(
                sphere([0.0; 3], 1.25),
                TrimOp::Intersect,
                cyl([0.0, 3.0, 0.0], NEG_Y, 0.75, 6.0),
            ),
            plane_surf([0.0, -1.0, 0.0], Y),
            sphere_surf([0.0; 3], 1.25),
            &ring_y(0.75, -1.0),
            Overlap,
        ),
        case(
            PT,
            SharedEdge,
            "through",
            "An inverted frustum's bottom rim (plane z = 0.5 and a cone) is the rim of the reference torus cut to a coaxial cylinder of radius 1.625 (a setup boolean; they meet on the parallel (1.625, 0.5)): the one circle.",
            lathe_z(
                0.0,
                0.0,
                &[(0.0, 0.5), (-1.625, 0.5), (-2.625, 1.5), (0.0, 1.5)],
            ),
            trimmed(
                torus_t(),
                TrimOp::Intersect,
                cyl([0.0, 0.0, -2.0], Z, 1.625, 4.0),
            ),
            plane_surf([0.0, 0.0, 0.5], Z),
            surf_t(),
            &[
                [0.0, 1.625, 0.5],
                [-1.625, 0.0, 0.5],
                [0.975, 1.3, 0.5],
                [-0.975, 1.3, 0.5],
            ],
            Overlap,
        ),
        case(
            CC,
            SharedEdge,
            "kiss",
            "Two flatted cylinders of radius 1.25 (two setup booleans), about x = 0 cut at x = 0.75 and about x = 1.5 cut at y = 1, have the one ruling edge (0.75, 1, z), z in [0.5,1.5] (the 3-4-5 point of both circles) and no other common point.",
            flatted_cylinder(),
            trimmed(
                cyl([1.5, 0.0, 0.5], Z, 1.25, 1.0),
                TrimOp::Intersect,
                blk([-1.0, 1.0, 0.0], [4.0, 3.0, 2.0]),
            ),
            cyl_surf([0.0; 3], Z, 1.25),
            cyl_surf([1.5, 0.0, 0.0], Z, 1.25),
            &[[0.75, 1.0, 0.75], [0.75, 1.0, 1.0], [0.75, 1.0, 1.25]],
            Kiss,
        ),
        case(
            CK,
            SharedEdge,
            "through",
            "A cylinder's top rim (radius 0.75, cap z = 1) is a spindle's rim (two cones): the one circle of radius 0.75 at z = 1. The four faces at it are four different surfaces.",
            cyl([0.0; 3], Z, 0.75, 1.0),
            lathe_z(
                0.0,
                0.0,
                &[
                    (0.0, 0.5),
                    (0.25, 0.5),
                    (0.75, 1.0),
                    (0.25, 2.0),
                    (0.0, 2.0),
                ],
            ),
            cyl_surf([0.0; 3], Z, 0.75),
            cone_surf([0.0, 0.0, 0.25], Z, 1.0),
            &ring_z(0.75, 1.0),
            Overlap,
        ),
        case(
            CS,
            SharedEdge,
            "through",
            "A bullet's shoulder about the y axis (a cylinder of radius 0.75 and a cone) is the rim of a sphere of radius 1.25 cut off at y = -1 (a setup boolean): the one circle of radius 0.75 in the plane y = -1.",
            lathe_y(
                0.0,
                0.0,
                &[
                    (-2.5, 0.0),
                    (-2.5, 0.75),
                    (-1.0, 0.75),
                    (-0.5, 0.25),
                    (-0.5, 0.0),
                ],
            ),
            flatted_sphere(),
            cyl_surf([0.0; 3], Y, 0.75),
            sphere_surf([0.0; 3], 1.25),
            &ring_y(0.75, -1.0),
            Overlap,
        ),
        case(
            CT,
            SharedEdge,
            "arc",
            "A body of revolution's shoulder (a cylinder of radius 1.625 above, a cone below) and the reference torus cut to x <= -0.25, z <= 0.5 (a setup boolean) share an arc of the circle of radius 1.625 at z = 0.5: the torus's outer rim there, between its two cut planes.",
            lathe_z(
                0.0,
                0.0,
                &[
                    (0.0, -0.5),
                    (1.125, -0.5),
                    (1.625, 0.5),
                    (1.625, 1.5),
                    (0.0, 1.5),
                ],
            ),
            trimmed(
                torus_t(),
                TrimOp::Intersect,
                blk([-2.5, -2.5, -1.0], [-0.25, 2.5, 0.5]),
            ),
            cyl_surf([0.0; 3], Z, 1.625),
            surf_t(),
            &[
                [-1.625, 0.0, 0.5],
                [-0.975, 1.3, 0.5],
                [-0.975, -1.3, 0.5],
                [-1.5, 0.625, 0.5],
            ],
            Overlap,
        ),
        case(
            KK,
            SharedEdge,
            "through",
            "Two coaxial bodies of revolution each have a rim between two cones at the one circle of radius 0.75 at z = 1: the four cones have slopes 0.375, -0.75, 2 and 0.25, four different surfaces.",
            lathe_z(
                0.0,
                0.0,
                &[
                    (0.0, 0.0),
                    (-0.375, 0.0),
                    (-0.75, 1.0),
                    (-0.375, 1.5),
                    (0.0, 1.5),
                ],
            ),
            lathe_z(
                0.0,
                0.0,
                &[
                    (0.0, 0.75),
                    (0.25, 0.75),
                    (0.75, 1.0),
                    (1.0, 2.0),
                    (0.0, 2.0),
                ],
            ),
            cone_surf([0.0, 0.0, -1.0], Z, 0.375),
            cone_surf([0.0, 0.0, 0.625], Z, 2.0),
            &ring_z(0.75, 1.0),
            Overlap,
        ),
        case(
            KS,
            SharedEdge,
            "through",
            "A spindle's rim about the y axis (two cones) is the rim of a sphere of radius 1.25 cut off at y = -1 (a setup boolean): the one circle of radius 0.75 in the plane y = -1.",
            lathe_y(
                0.0,
                0.0,
                &[
                    (-2.0, 0.0),
                    (-2.0, 0.25),
                    (-1.0, 0.75),
                    (-0.5, 0.25),
                    (-0.5, 0.0),
                ],
            ),
            flatted_sphere(),
            cone_surf([0.0, -2.5, 0.0], Y, 0.5),
            sphere_surf([0.0; 3], 1.25),
            &ring_y(0.75, -1.0),
            Overlap,
        ),
        case(
            KT,
            SharedEdge,
            "segment",
            "A spindle about the line x = -1.25, z = 0 has its rim, the circle of radius 0.625 about (-1.25, 0, 0) in the plane y = 0, on an end circle of the half torus (the reference torus revolved 180 degrees, no boolean): the spindle's outer cone leaves the torus, its inner cone enters the tube, and the half torus's flat end is a cross-section of the spindle.",
            lathe_y(
                -1.25,
                0.0,
                &[
                    (-1.0, 0.0),
                    (-1.0, 0.125),
                    (0.0, 0.625),
                    (0.5, 0.25),
                    (0.5, 0.0),
                ],
            ),
            half_torus(),
            cone_surf([-1.25, -1.25, 0.0], Y, 0.5),
            surf_t(),
            &HALF_TORUS_RIM,
            Overlap,
        ),
        case(
            SS,
            SharedEdge,
            "through",
            "A sphere of radius 1.25 cut off at y = -1 and an equal sphere centred (0, -2, 0) cut to a coaxial cylinder of radius 0.75 (two setup booleans) have the one rim: the circle of radius 0.75 in the radical plane y = -1.",
            flatted_sphere(),
            trimmed(
                sphere([0.0, -2.0, 0.0], 1.25),
                TrimOp::Intersect,
                cyl([0.0, 1.0, 0.0], NEG_Y, 0.75, 5.0),
            ),
            sphere_surf([0.0; 3], 1.25),
            sphere_surf([0.0, -2.0, 0.0], 1.25),
            &ring_y(0.75, -1.0),
            Overlap,
        ),
        case(
            ST,
            SharedEdge,
            "through",
            "A sphere of radius 1.25 centred (0, 0, -0.625) cut off above z = 0.375, and the reference torus with a coaxial hole of radius 0.75 drilled through its tube (two setup booleans), have the one rim: the circle of radius 0.75 at z = 0.375.",
            trimmed(
                sphere([0.0, 0.0, -0.625], 1.25),
                TrimOp::Intersect,
                blk([-2.0, -2.0, -2.5], [2.0, 2.0, 0.375]),
            ),
            trimmed(
                torus_t(),
                TrimOp::Cut,
                cyl([0.0, 0.0, -2.0], Z, 0.75, 4.0),
            ),
            sphere_surf([0.0, 0.0, -0.625], 1.25),
            surf_t(),
            &ring_z(0.75, 0.375),
            Overlap,
        ),
        case(
            TT,
            SharedEdge,
            "through",
            "The reference torus cut off above z = 0.375, and a coaxial torus of major radius 1.125 centred at z = 0.875 with a coaxial hole of radius 0.75 drilled through its tube (two setup booleans), have the one rim: the circle of radius 0.75 at z = 0.375.",
            topped_torus(0.375),
            trimmed(
                torus_z([0.0, 0.0, 0.875], 1.125, T_MINOR, -1.0),
                TrimOp::Cut,
                cyl([0.0, 0.0, -1.0], Z, 0.75, 4.0),
            ),
            surf_t(),
            torus_surf([0.0, 0.0, 0.875], 1.125, T_MINOR),
            &ring_z(0.75, 0.375),
            Overlap,
        ),
    ];
    cases.sort_by_key(|c| (c.pair.index, c.class));
    cases
}

/// Parse a case id written by [`ContactCase::case_id`] back into its case
/// and operation.
pub fn find_case(id: &str) -> Option<(ContactCase, ContactOp)> {
    let (key, op_slug) = id.rsplit_once('_')?;
    let op = ContactOp::from_slug(op_slug)?;
    let case = all_contact_cases().into_iter().find(|c| c.key() == key)?;
    case.ops().contains(&op).then_some((case, op))
}

// ── Document generation ─────────────────────────────────────────────────────

/// One generated `.waffle` document and its derived [`AssayMeta`], ready for
/// [`crate::assay::categorize::categorize`].
#[derive(Debug, Clone)]
pub struct GeneratedContactCase {
    pub id: String,
    pub waffle_json: String,
    pub meta: AssayMeta,
    /// `(feature id, role)` for every feature of the document, in order —
    /// `sk_a`, `body_a`, `tool_a`, `trim_a`, …, `bool_ab` — so an engine error
    /// can be read by role instead of by UUID.
    pub feature_roles: Vec<(String, String)>,
}

impl Prim {
    /// Author this primitive into `builder` as the sketch `sk_name` and the
    /// standalone (non-merging) body `body_name`.
    pub fn add_to_builder(
        &self,
        builder: &mut ModelBuilder,
        sk_name: &str,
        body_name: &str,
    ) -> Result<(), String> {
        let err = |e: crate::helpers::HarnessError| e.to_string();
        let (origin, normal, profile) = match self {
            Prim::Extrude {
                origin,
                normal,
                profile,
                ..
            }
            | Prim::Revolve {
                origin,
                normal,
                profile,
                ..
            } => (*origin, *normal, profile),
        };
        match profile {
            Profile::Polygon(pts) => builder
                .polygon_sketch(sk_name, origin, normal, pts)
                .map_err(err)?,
            Profile::Circle { cu, cv, r } => builder
                .true_circle_sketch(sk_name, origin, normal, *cu, *cv, *r)
                .map_err(err)?,
        };
        match self {
            Prim::Extrude { depth, .. } => {
                builder
                    .extrude_no_merge(body_name, sk_name, *depth)
                    .map_err(err)?;
            }
            Prim::Revolve {
                axis_origin,
                axis_dir,
                angle,
                ..
            } => {
                builder
                    .revolve(body_name, sk_name, *axis_origin, *axis_dir, *angle)
                    .map_err(err)?;
            }
        }
        Ok(())
    }
}

/// Author one operand; returns the name of the feature that holds its body.
fn add_operand(
    builder: &mut ModelBuilder,
    operand: &Operand,
    side: &str,
    roles: &mut Vec<String>,
) -> Result<String, String> {
    let err = |e: crate::helpers::HarnessError| e.to_string();
    let sk = format!("sk_{side}");
    let body = format!("body_{side}");
    operand.base.add_to_builder(builder, &sk, &body)?;
    roles.extend([sk, body.clone()]);
    let Some((op, tool)) = &operand.trim else {
        return Ok(body);
    };
    let tool_sk = format!("sk_tool_{side}");
    let tool_body = format!("tool_{side}");
    let trim = format!("trim_{side}");
    tool.add_to_builder(builder, &tool_sk, &tool_body)?;
    match op {
        TrimOp::Intersect => builder.boolean_intersect(&trim, &body, &tool_body),
        TrimOp::Cut => builder.boolean_subtract(&trim, &body, &tool_body),
    }
    .map_err(err)?;
    roles.extend([tool_sk, tool_body, trim.clone()]);
    Ok(trim)
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Build the `.waffle` document and derived [`AssayMeta`] of one operation of
/// a case, through the real `wasm_bridge::dispatch` + `feature-engine` path
/// (`ModelBuilder::mock()`), with its UUIDs canonicalized so the document is
/// byte-for-byte deterministic.
pub fn build_contact_case(
    case: &ContactCase,
    op: ContactOp,
) -> Result<GeneratedContactCase, String> {
    let id = case.case_id(op);
    let err = |e: crate::helpers::HarnessError| e.to_string();
    let mut builder = ModelBuilder::mock();
    let mut roles: Vec<String> = Vec::new();
    match op {
        ContactOp::SetupA | ContactOp::SetupB => {
            let (operand, side) = if op == ContactOp::SetupA {
                (&case.a, "a")
            } else {
                (&case.b, "b")
            };
            if operand.trim.is_none() {
                return Err(format!("{id}: operand {side} has no setup step"));
            }
            add_operand(&mut builder, operand, side, &mut roles)?;
        }
        ContactOp::Union | ContactOp::Cut | ContactOp::Intersect => {
            let ra = add_operand(&mut builder, &case.a, "a", &mut roles)?;
            let rb = add_operand(&mut builder, &case.b, "b", &mut roles)?;
            match op {
                ContactOp::Union => builder.boolean_union("bool_ab", &ra, &rb),
                ContactOp::Cut => builder.boolean_subtract("bool_ab", &ra, &rb),
                _ => builder.boolean_intersect("bool_ab", &ra, &rb),
            }
            .map_err(err)?;
            roles.push("bool_ab".to_string());
        }
    }
    let raw_json = builder.save().map_err(err)?;
    let mut doc: Value = serde_json::from_str(&raw_json).map_err(|e| e.to_string())?;
    canonicalize_document(&mut doc, 0x434D_3337_0000_0000 ^ fnv1a(&id));
    let mut meta = derive_meta(&id, &doc)?;
    if case.expects_empty(op) {
        meta.oracles.expected_solid_count = Some(0);
    }
    meta.description = format!(
        "Gate 3.7 contact matrix: {} — {} ({}, {}): {}",
        case.pair.label(),
        case.class.label(),
        case.variant,
        op.slug(),
        case.what
    );
    let feature_ids: Vec<String> = doc
        .pointer("/tabs/0/kind/features/features")
        .and_then(Value::as_array)
        .map(|fs| {
            fs.iter()
                .filter_map(|f| f.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if feature_ids.len() != roles.len() {
        return Err(format!(
            "{id}: document has {} features, authored {}",
            feature_ids.len(),
            roles.len()
        ));
    }
    let waffle_json = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    Ok(GeneratedContactCase {
        id,
        waffle_json,
        meta,
        feature_roles: feature_ids.into_iter().zip(roles).collect(),
    })
}

/// The case's primitives alone — base and tool of A, then of B, each a
/// standalone body, no boolean — as the document [`verify_contact_geometry`]
/// reads its exact solids from. (The exact chain resolves a document's
/// booleans on a lattice, half a second each; the primitives are all the
/// verification needs.)
fn build_primitives_document(case: &ContactCase) -> Result<String, String> {
    let mut builder = ModelBuilder::mock();
    let mut index = 0usize;
    for operand in [&case.a, &case.b] {
        let prims = std::iter::once(&operand.base).chain(operand.trim.as_ref().map(|(_, t)| t));
        for prim in prims {
            prim.add_to_builder(
                &mut builder,
                &format!("sk_{index}"),
                &format!("body_{index}"),
            )?;
            index += 1;
        }
    }
    builder.save().map_err(|e| e.to_string())
}

impl GeneratedContactCase {
    /// `detail` with every feature UUID replaced by its role (`bool_ab`,
    /// `trim_a`, …).
    pub fn with_roles(&self, detail: &str) -> String {
        let mut out = detail.to_string();
        for (id, role) in &self.feature_roles {
            out = out.replace(id.as_str(), role);
        }
        out
    }
}

// ── Kernel-free verification of the authored contact ────────────────────────

/// Residual tolerance for a locus point on its two surfaces, in metres. The
/// authored numbers are exact; what is left is the rounding of a few float
/// operations on values of order one.
pub const LOCUS_RESIDUAL_TOL: f64 = 1e-12;

/// Operand membership read from the primitives document's exact chain: `ops[first]` is
/// the base primitive and, for a trimmed operand, `ops[first + 1]` the tool.
struct ExactOperand<'a> {
    base: &'a ExactSolid,
    trim: Option<(TrimOp, &'a ExactSolid)>,
}

impl ExactOperand<'_> {
    fn contains(&self, p: [f64; 3]) -> bool {
        if !self.base.contains(p) {
            return false;
        }
        match self.trim {
            None => true,
            Some((TrimOp::Intersect, tool)) => tool.contains(p),
            Some((TrimOp::Cut, tool)) => !tool.contains(p),
        }
    }
}

/// Probe directions: the 26 directions of the unit cube's faces, edges and
/// corners, and the 72 through the lattice points with coordinates `±3` and
/// `0` or `±1` — fine enough to enter a cone of the 3-4-5 half-angle at its
/// apex and the 37° wedge of a cylinder segment at its ruling edge.
fn probe_directions() -> Vec<[f64; 3]> {
    let mut dirs = Vec::with_capacity(98);
    for x in [-3.0_f64, -1.0, 0.0, 1.0, 3.0] {
        for y in [-3.0_f64, -1.0, 0.0, 1.0, 3.0] {
            for z in [-3.0_f64, -1.0, 0.0, 1.0, 3.0] {
                let threes = [x, y, z].iter().filter(|c| c.abs() == 3.0).count();
                let nonzero = x != 0.0 || y != 0.0 || z != 0.0;
                if nonzero && threes <= 1 {
                    dirs.push(unit([x, y, z]));
                }
            }
        }
    }
    dirs
}

/// Check, with no kernel call, that a case is the contact it says it is:
///
/// 1. `surf_a` / `surf_b` are of the pair's two kinds;
/// 2. every locus point lies on both surfaces to [`LOCUS_RESIDUAL_TOL`];
/// 3. the surfaces are tangent there (coincident and tangency classes) or
///    cross (edge, vertex and shared-edge classes) — skipped where a surface
///    has no normal (a cone's apex);
/// 4. every locus point is a boundary point of both authored solids, read
///    from the document through [`ExactChain`];
/// 5. the solids are arranged as [`ContactCase::arrangement`] says, on a
///    lattice over their bounding box and on points near the locus.
pub fn verify_contact_geometry(case: &ContactCase) -> Result<(), String> {
    let key = case.key();
    if case.surf_a.kind() != case.pair.a || case.surf_b.kind() != case.pair.b {
        return Err(format!(
            "{key}: surfaces are {:?} × {:?}, the pair is {}",
            case.surf_a.kind(),
            case.surf_b.kind(),
            case.pair.label()
        ));
    }
    if case.locus.is_empty() {
        return Err(format!("{key}: no locus point"));
    }
    let tangent = matches!(
        case.class,
        ContactClass::Coincident | ContactClass::TangentCurve | ContactClass::TangentPoint
    );
    for &p in &case.locus {
        let (ra, rb) = (case.surf_a.residual(p), case.surf_b.residual(p));
        if ra.abs() > LOCUS_RESIDUAL_TOL || rb.abs() > LOCUS_RESIDUAL_TOL {
            return Err(format!(
                "{key}: locus point {p:?} is off its surfaces: a {ra:.3e}, b {rb:.3e}"
            ));
        }
        if let (Some(na), Some(nb)) = (case.surf_a.normal(p), case.surf_b.normal(p)) {
            let sine = len(cross(na, nb));
            if tangent && sine > 1e-9 {
                return Err(format!(
                    "{key}: surfaces are not tangent at {p:?}: |na × nb| = {sine:.3e}"
                ));
            }
            if !tangent && sine < 0.05 {
                return Err(format!(
                    "{key}: surfaces do not cross at {p:?}: |na × nb| = {sine:.3e}"
                ));
            }
        } else if tangent {
            return Err(format!("{key}: a tangency needs a normal at {p:?}"));
        }
    }

    let primitives = build_primitives_document(case)?;
    let doc: Value = serde_json::from_str(&primitives).map_err(|e| e.to_string())?;
    let chain = ExactChain::from_waffle(&doc)
        .map_err(|e| format!("{key}: the exact chain does not cover the primitives: {e}"))?;
    let n_a = 1 + usize::from(case.a.trim.is_some());
    let n_b = 1 + usize::from(case.b.trim.is_some());
    if chain.ops.len() != n_a + n_b {
        return Err(format!(
            "{key}: expected {} exact operands, got {}",
            n_a + n_b,
            chain.ops.len()
        ));
    }
    let exact = |first: usize, operand: &Operand| ExactOperand {
        base: &chain.ops[first].solid,
        trim: operand
            .trim
            .as_ref()
            .map(|(op, _)| (*op, &chain.ops[first + 1].solid)),
    };
    let a = exact(0, &case.a);
    let b = exact(n_a, &case.b);

    let dirs = probe_directions();
    for &p in &case.locus {
        for (name, solid) in [("A", &a), ("B", &b)] {
            let mut probes: Vec<[f64; 3]> = dirs.clone();
            for n in [case.surf_a.normal(p), case.surf_b.normal(p)]
                .into_iter()
                .flatten()
            {
                probes.push(n);
                probes.push(scale(n, -1.0));
            }
            let inside = probes
                .iter()
                .filter(|d| solid.contains(add(p, scale(**d, 1e-3))))
                .count();
            if inside == 0 || inside == probes.len() {
                return Err(format!(
                    "{key}: locus point {p:?} is not on the boundary of solid {name} ({inside} of {} probes inside)",
                    probes.len()
                ));
            }
        }
    }

    // Arrangement: a lattice over the bounding box, plus points near the
    // locus (a thin overlap can fall between lattice points). Every sample is
    // nudged off the dyadic grid so none sits on a face of either solid.
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for op in &chain.ops {
        let (l, h) = op.solid.bbox();
        for i in 0..3 {
            lo[i] = lo[i].min(l[i]);
            hi[i] = hi[i].max(h[i]);
        }
    }
    const NUDGE: [f64; 3] = [3.1e-7, 5.7e-7, 7.3e-7];
    const N: usize = 44;
    let mut samples: Vec<[f64; 3]> =
        Vec::with_capacity(N * N * N + case.locus.len() * 3 * dirs.len());
    for i in 0..N {
        for j in 0..N {
            for k in 0..N {
                let f = |idx: usize, axis: usize| {
                    lo[axis] + (hi[axis] - lo[axis]) * (idx as f64 + 0.37) / N as f64 + NUDGE[axis]
                };
                samples.push([f(i, 0), f(j, 1), f(k, 2)]);
            }
        }
    }
    for &p in &case.locus {
        for d in &dirs {
            for r in [0.02, 0.05, 0.11] {
                samples.push(add(add(p, scale(*d, r)), NUDGE));
            }
        }
    }
    let (mut both, mut a_only, mut b_only) = (0usize, 0usize, 0usize);
    for &q in &samples {
        match (a.contains(q), b.contains(q)) {
            (true, true) => both += 1,
            (true, false) => a_only += 1,
            (false, true) => b_only += 1,
            (false, false) => {}
        }
    }
    let ok = match case.arrangement {
        Arrangement::Kiss => both == 0 && a_only > 0 && b_only > 0,
        Arrangement::Nested => both > 0 && a_only > 0 && b_only == 0,
        Arrangement::Identical => both > 0 && a_only == 0 && b_only == 0,
        Arrangement::Overlap => both > 0 && a_only > 0 && b_only > 0,
    };
    if !ok {
        return Err(format!(
            "{key}: arrangement is not {}: {both} samples in both, {a_only} in A only, {b_only} in B only",
            case.arrangement.label()
        ));
    }
    Ok(())
}

// ── Verdicts and the report ─────────────────────────────────────────────────

/// What one `(case, operation)` run means for the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CellVerdict {
    /// The categorizer's `SUPPORTED_CORRECT`.
    Correct,
    /// An error or refusal whose text names the contact class
    /// ([`refusal_names_class`]).
    RefusedNamed,
    /// `SUPPORTED_WRONG`. A finding.
    Wrong,
    /// `TIMEOUT`. A finding.
    Timeout,
    /// An `ERROR` whose text does not let a caller tell the contact class —
    /// a kernel STOP, a crash. A finding.
    ErrorUnnamed,
    /// An `UNSUPPORTED(…)` — a typed `NotSupported` wall — for something
    /// other than the contact class (a trimmed operand that cannot re-enter a
    /// boolean, for one). A finding.
    UnsupportedUnnamed,
    /// The case's setup step did not categorize CORRECT, so the contact was
    /// never reached. A finding about the setup, not about the contact.
    Blocked,
}

impl CellVerdict {
    pub const fn label(self) -> &'static str {
        match self {
            CellVerdict::Correct => "CORRECT",
            CellVerdict::RefusedNamed => "REFUSED_NAMED",
            CellVerdict::Wrong => "WRONG",
            CellVerdict::Timeout => "TIMEOUT",
            CellVerdict::ErrorUnnamed => "ERROR_UNNAMED",
            CellVerdict::UnsupportedUnnamed => "UNSUPPORTED_UNNAMED",
            CellVerdict::Blocked => "BLOCKED",
        }
    }

    /// One letter for the grid.
    pub const fn letter(self) -> char {
        match self {
            CellVerdict::Correct => 'C',
            CellVerdict::RefusedNamed => 'R',
            CellVerdict::Wrong => 'W',
            CellVerdict::Timeout => 'T',
            CellVerdict::ErrorUnnamed => 'E',
            CellVerdict::UnsupportedUnnamed => 'U',
            CellVerdict::Blocked => 'B',
        }
    }

    pub const fn is_finding(self) -> bool {
        !matches!(self, CellVerdict::Correct | CellVerdict::RefusedNamed)
    }

    /// Listing order of findings: WRONG first, then TIMEOUT, then errors and
    /// refusals that do not name the class, then blocked setups.
    const fn severity(self) -> u8 {
        match self {
            CellVerdict::Wrong => 0,
            CellVerdict::Timeout => 1,
            CellVerdict::ErrorUnnamed => 2,
            CellVerdict::UnsupportedUnnamed => 3,
            CellVerdict::Blocked => 4,
            CellVerdict::RefusedNamed => 5,
            CellVerdict::Correct => 6,
        }
    }
}

/// Read a categorizer verdict against a contact class.
pub fn classify(category: &Category, detail: &str, class: ContactClass) -> CellVerdict {
    match category {
        Category::SupportedCorrect => CellVerdict::Correct,
        Category::SupportedWrong => CellVerdict::Wrong,
        Category::Timeout | Category::SkippedSlow => CellVerdict::Timeout,
        Category::Error | Category::ExpectedError | Category::Unsupported(_) => {
            if refusal_names_class(detail, class) {
                CellVerdict::RefusedNamed
            } else if matches!(category, Category::Unsupported(_)) {
                CellVerdict::UnsupportedUnnamed
            } else {
                CellVerdict::ErrorUnnamed
            }
        }
    }
}

/// Per-run record in `target/contact-matrix/report.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactCaseRecord {
    pub id: String,
    pub pair: String,
    pub class: String,
    pub variant: String,
    pub arrangement: String,
    pub op: String,
    /// The categorizer's category label.
    pub category: String,
    /// [`CellVerdict::label`]; for a setup step the category read with no
    /// class (CORRECT, or a finding).
    pub verdict: String,
    pub exact_volume: Option<ExactVolumeRecord>,
    pub detail: String,
    pub what: String,
    pub repro: String,
}

/// One cell of the matrix: a surface pair against a contact class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactCell {
    pub pair_index: usize,
    pub pair: String,
    pub class: String,
    /// `ADMITTED`, `NOT_ADMITTED` or `NOT_BUILDABLE`.
    pub admission: String,
    /// Why the pair does not admit the class, or why it cannot be built.
    pub reason: Option<String>,
    /// The authored configurations of this cell.
    pub variants: Vec<String>,
    /// Worst verdict letter per boolean over the cell's variants, or `None`
    /// when nothing ran for it.
    pub union: Option<char>,
    pub cut: Option<char>,
    pub intersect: Option<char>,
    /// `MET` (every run CORRECT or a refusal naming the class), `FINDING`,
    /// `UNTESTED` (admitted, nothing ran), `NOT_ADMITTED`, `NOT_BUILDABLE`.
    pub status: String,
}

/// Counts of [`CellVerdict`] over the boolean runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerdictCounts {
    pub correct: usize,
    pub refused_named: usize,
    pub wrong: usize,
    pub timeout: usize,
    pub error_unnamed: usize,
    pub unsupported_unnamed: usize,
    pub blocked: usize,
    /// Of `correct`: the runs whose exact-volume lattice reading agrees.
    pub correct_volume_agree: usize,
    /// Of `correct`: the runs the exact-volume lattice does not cover.
    pub correct_volume_not_covered: usize,
    /// Of `correct`: the runs judged before the volume check — an empty
    /// result, certified empty.
    pub correct_volume_not_reached: usize,
}

impl VerdictCounts {
    fn record(&mut self, v: CellVerdict) {
        match v {
            CellVerdict::Correct => self.correct += 1,
            CellVerdict::RefusedNamed => self.refused_named += 1,
            CellVerdict::Wrong => self.wrong += 1,
            CellVerdict::Timeout => self.timeout += 1,
            CellVerdict::ErrorUnnamed => self.error_unnamed += 1,
            CellVerdict::UnsupportedUnnamed => self.unsupported_unnamed += 1,
            CellVerdict::Blocked => self.blocked += 1,
        }
    }

    pub fn findings(&self) -> usize {
        self.wrong + self.timeout + self.error_unnamed + self.unsupported_unnamed + self.blocked
    }
}

/// The JSON report written to `target/contact-matrix/report.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactMatrixReport {
    pub gate: String,
    pub kernel: String,
    pub commit: String,
    pub total_cells: usize,
    pub admitted_cells: usize,
    pub not_admitted_cells: usize,
    pub not_buildable_cells: usize,
    pub cells_met: usize,
    pub cells_with_findings: usize,
    pub cells_untested: usize,
    /// Over the boolean runs (three per authored case).
    pub totals: VerdictCounts,
    pub boolean_runs: usize,
    pub setup_runs: usize,
    pub setup_runs_correct: usize,
    pub run: Value,
    pub cells: Vec<ContactCell>,
    pub cases: Vec<ContactCaseRecord>,
    /// Ids of the boolean runs that are findings, WRONG first.
    pub findings: Vec<String>,
}

/// Assemble the report from categorizer outcomes (stamped with the worktree's
/// commit).
pub fn build_report(
    outcomes: &[(CaseOutcome, Option<ExactVolumeRecord>)],
    prov: &RunProvenance,
) -> ContactMatrixReport {
    build_report_at_commit(resolve_git_commit(), outcomes, prov)
}

/// [`build_report`] with an explicit `commit` string.
pub fn build_report_at_commit(
    commit: String,
    outcomes: &[(CaseOutcome, Option<ExactVolumeRecord>)],
    prov: &RunProvenance,
) -> ContactMatrixReport {
    let cases = all_contact_cases();
    let by_key: HashMap<String, &ContactCase> = cases.iter().map(|c| (c.key(), c)).collect();

    // A case is blocked when one of its setup steps ran and is not CORRECT.
    let mut blocked: HashMap<String, String> = HashMap::new();
    for (o, _) in outcomes {
        let Some((key, op_slug)) = o.id.rsplit_once('_') else {
            continue;
        };
        if ContactOp::from_slug(op_slug).is_some_and(ContactOp::is_setup)
            && o.category != Category::SupportedCorrect
        {
            blocked
                .entry(key.to_string())
                .or_insert_with(|| format!("{op_slug} is {}", o.category.label()));
        }
    }

    let mut records = Vec::with_capacity(outcomes.len());
    let mut totals = VerdictCounts::default();
    let (mut boolean_runs, mut setup_runs, mut setup_runs_correct) = (0usize, 0usize, 0usize);
    // (pair index, class) → op → worst verdict.
    let mut cell_ops: HashMap<(usize, ContactClass), HashMap<ContactOp, CellVerdict>> =
        HashMap::new();
    let mut findings: Vec<(u8, String)> = Vec::new();
    for (o, exact_vol) in outcomes {
        let Some((key, op_slug)) = o.id.rsplit_once('_') else {
            continue;
        };
        let (Some(case), Some(op)) = (by_key.get(key), ContactOp::from_slug(op_slug)) else {
            continue;
        };
        let verdict = if op.is_setup() {
            setup_runs += 1;
            if o.category == Category::SupportedCorrect {
                setup_runs_correct += 1;
                CellVerdict::Correct
            } else {
                // A setup failure names no contact class by construction.
                match o.category {
                    Category::SupportedWrong => CellVerdict::Wrong,
                    Category::Timeout | Category::SkippedSlow => CellVerdict::Timeout,
                    Category::Unsupported(_) => CellVerdict::UnsupportedUnnamed,
                    _ => CellVerdict::ErrorUnnamed,
                }
            }
        } else {
            boolean_runs += 1;
            let v = if blocked.contains_key(key) {
                CellVerdict::Blocked
            } else {
                classify(&o.category, &o.detail, case.class)
            };
            totals.record(v);
            if v == CellVerdict::Correct {
                match exact_vol {
                    Some(ExactVolumeRecord::Agree { .. }) => totals.correct_volume_agree += 1,
                    Some(_) => totals.correct_volume_not_covered += 1,
                    None => totals.correct_volume_not_reached += 1,
                }
            }
            let slot = cell_ops
                .entry((case.pair.index, case.class))
                .or_default()
                .entry(op)
                .or_insert(v);
            if v.severity() < slot.severity() {
                *slot = v;
            }
            if v.is_finding() {
                findings.push((v.severity(), o.id.clone()));
            }
            v
        };
        let detail = match blocked.get(key) {
            Some(why) if !op.is_setup() => format!("blocked: {why}; {}", o.detail),
            _ => o.detail.clone(),
        };
        records.push(ContactCaseRecord {
            id: o.id.clone(),
            pair: case.pair.label(),
            class: case.class.label().to_string(),
            variant: case.variant.to_string(),
            arrangement: case.arrangement.label().to_string(),
            op: op.slug().to_string(),
            category: o.category.label(),
            verdict: verdict.label().to_string(),
            exact_volume: exact_vol.clone(),
            detail,
            what: case.what.to_string(),
            repro: case.repro(op),
        });
    }
    findings.sort();

    let mut variants: BTreeMap<(usize, ContactClass), Vec<String>> = BTreeMap::new();
    for c in &cases {
        variants
            .entry((c.pair.index, c.class))
            .or_default()
            .push(c.variant.to_string());
    }

    let mut cells = Vec::with_capacity(ALL_SURFACE_PAIRS.len() * ContactClass::ALL.len());
    let (mut admitted, mut not_admitted, mut not_buildable) = (0usize, 0usize, 0usize);
    let (mut met, mut with_findings, mut untested) = (0usize, 0usize, 0usize);
    for pair in ALL_SURFACE_PAIRS {
        for class in ContactClass::ALL {
            let cell_variants = variants
                .get(&(pair.index, class))
                .cloned()
                .unwrap_or_default();
            let ops = cell_ops.get(&(pair.index, class));
            let letter = |op: ContactOp| ops.and_then(|m| m.get(&op)).map(|v| v.letter());
            let (admission_label, reason, status) = match admission(pair, class) {
                Admission::NotBuildable => {
                    not_buildable += 1;
                    (
                        "NOT_BUILDABLE",
                        Some(NOT_BUILDABLE_BSPLINE.to_string()),
                        "NOT_BUILDABLE",
                    )
                }
                Admission::NotAdmitted(why) => {
                    not_admitted += 1;
                    ("NOT_ADMITTED", Some(why.to_string()), "NOT_ADMITTED")
                }
                Admission::Admitted => {
                    admitted += 1;
                    let ran: Vec<CellVerdict> = ops
                        .map(|m| m.values().copied().collect())
                        .unwrap_or_default();
                    let complete = ContactOp::BOOLEANS
                        .iter()
                        .all(|op| ops.is_some_and(|m| m.contains_key(op)));
                    let status = if ran.iter().any(|v| v.is_finding()) {
                        with_findings += 1;
                        "FINDING"
                    } else if !complete {
                        untested += 1;
                        "UNTESTED"
                    } else {
                        met += 1;
                        "MET"
                    };
                    ("ADMITTED", None, status)
                }
            };
            cells.push(ContactCell {
                pair_index: pair.index,
                pair: pair.label(),
                class: class.label().to_string(),
                admission: admission_label.to_string(),
                reason,
                variants: cell_variants,
                union: letter(ContactOp::Union),
                cut: letter(ContactOp::Cut),
                intersect: letter(ContactOp::Intersect),
                status: status.to_string(),
            });
        }
    }

    ContactMatrixReport {
        gate: "Kernel 1.0 Gate 3.7 — degenerate-contact matrix".to_string(),
        kernel: "kernel-v2 (KernelV2Adapter)".to_string(),
        commit,
        total_cells: cells.len(),
        admitted_cells: admitted,
        not_admitted_cells: not_admitted,
        not_buildable_cells: not_buildable,
        cells_met: met,
        cells_with_findings: with_findings,
        cells_untested: untested,
        totals,
        boolean_runs,
        setup_runs,
        setup_runs_correct,
        run: prov.to_json(None),
        cells,
        cases: records,
        findings: findings.into_iter().map(|(_, id)| id).collect(),
    }
}

/// First `max` characters of `s` on one line.
fn excerpt(s: &str, max: usize) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let cut: String = flat.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// A finding's text reduced to its kind, for counting findings that stop the
/// same way: the text from the kernel's own message on (after the engine's
/// `kernel error:` wrapper when there is one), every run of digits replaced
/// by `N`, cut at 120 characters.
pub fn finding_signature(detail: &str) -> String {
    let core = detail
        .split_once("kernel error: ")
        .map_or(detail, |(_, after)| after);
    let mut sig = String::with_capacity(core.len());
    let mut in_number = false;
    for ch in core.chars() {
        if ch.is_ascii_digit() {
            if !in_number {
                sig.push('N');
            }
            in_number = true;
        } else {
            in_number = false;
            sig.push(ch);
        }
    }
    excerpt(&sig, 120)
}

/// Format the human-readable report: the grid (pair × contact class, one
/// letter per boolean), the cells the pairs do not admit with their reasons,
/// one line per run, the findings with their error text and replay command,
/// and the findings counted by kind.
pub fn format_report_table(report: &ContactMatrixReport) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "\nGATE 3.7 DEGENERATE-CONTACT MATRIX (commit {})",
        report.commit
    )
    .unwrap();
    writeln!(
        out,
        "cell = union/cut/intersect, worst over the cell's configurations: C CORRECT, R refusal naming the contact class, W WRONG, T TIMEOUT, E error not naming the class, U NotSupported wall not naming the class, B blocked by a setup step, · not run; — = the pair does not admit the class"
    )
    .unwrap();
    write!(out, "{:<3} {:<22}", "#", "pair").unwrap();
    for class in ContactClass::ALL {
        write!(out, " {:<8}", class.column()).unwrap();
    }
    writeln!(out).unwrap();
    writeln!(out, "{}", "-".repeat(26 + 9 * ContactClass::ALL.len())).unwrap();
    let per_pair = ContactClass::ALL.len();
    for row in report.cells.chunks(per_pair) {
        let first = &row[0];
        write!(out, "{:02}  {:<22}", first.pair_index, first.pair).unwrap();
        if first.admission == "NOT_BUILDABLE" {
            writeln!(out, " {NOT_BUILDABLE_BSPLINE}").unwrap();
            continue;
        }
        for cell in row {
            let text = if cell.admission == "NOT_ADMITTED" {
                "—".to_string()
            } else {
                [cell.union, cell.cut, cell.intersect]
                    .iter()
                    .map(|l| l.unwrap_or('·'))
                    .collect()
            };
            write!(out, " {text:<8}").unwrap();
        }
        writeln!(out).unwrap();
    }
    writeln!(out, "{}", "-".repeat(26 + 9 * ContactClass::ALL.len())).unwrap();
    writeln!(
        out,
        "Cells: {} total = {} admitted + {} not admitted + {} not buildable (B-spline)",
        report.total_cells,
        report.admitted_cells,
        report.not_admitted_cells,
        report.not_buildable_cells
    )
    .unwrap();
    writeln!(
        out,
        "Admitted cells: {} MET (every run CORRECT or a refusal naming the class) / {} with a FINDING / {} UNTESTED",
        report.cells_met, report.cells_with_findings, report.cells_untested
    )
    .unwrap();
    writeln!(
        out,
        "Boolean runs: {} = {} CORRECT / {} REFUSED_NAMED / {} WRONG / {} TIMEOUT / {} ERROR_UNNAMED / {} UNSUPPORTED_UNNAMED / {} BLOCKED",
        report.boolean_runs,
        report.totals.correct,
        report.totals.refused_named,
        report.totals.wrong,
        report.totals.timeout,
        report.totals.error_unnamed,
        report.totals.unsupported_unnamed,
        report.totals.blocked
    )
    .unwrap();
    writeln!(
        out,
        "Exact-volume reading of the {} CORRECT boolean runs: {} Agree / {} NotCovered / {} not reached (an empty result, judged before the volume check)",
        report.totals.correct,
        report.totals.correct_volume_agree,
        report.totals.correct_volume_not_covered,
        report.totals.correct_volume_not_reached
    )
    .unwrap();
    writeln!(
        out,
        "Setup runs (trimmed operands built alone): {} of {} CORRECT",
        report.setup_runs_correct, report.setup_runs
    )
    .unwrap();

    writeln!(out, "\nCells the pair does not admit:").unwrap();
    for cell in &report.cells {
        if cell.admission == "NOT_ADMITTED" {
            writeln!(
                out,
                "  {:<22} {:<26} {}",
                cell.pair,
                cell.class,
                cell.reason.as_deref().unwrap_or("")
            )
            .unwrap();
        }
    }

    if !report.cases.is_empty() {
        writeln!(out, "\nPer-run verdicts:").unwrap();
        for c in &report.cases {
            let ev = c.exact_volume.as_ref().map_or_else(
                || "— (not reached)".to_string(),
                ExactVolumeRecord::table_label,
            );
            writeln!(
                out,
                "  {:<58} {:<16} {:<18} {}",
                c.id, c.verdict, c.category, ev
            )
            .unwrap();
        }
    }

    let by_id: HashMap<&str, &ContactCaseRecord> =
        report.cases.iter().map(|c| (c.id.as_str(), c)).collect();
    writeln!(
        out,
        "\nFINDINGS ({} boolean runs; WRONG, then TIMEOUT, then errors and NotSupported walls that do not name the contact class, then blocked):",
        report.findings.len()
    )
    .unwrap();
    for id in &report.findings {
        let Some(c) = by_id.get(id.as_str()) else {
            continue;
        };
        writeln!(
            out,
            "  [{}] {} — {} ({}), {}",
            c.verdict, c.id, c.class, c.arrangement, c.op
        )
        .unwrap();
        writeln!(out, "      contact: {}", c.what).unwrap();
        writeln!(out, "      {}: {}", c.category, excerpt(&c.detail, 300)).unwrap();
        writeln!(out, "      repro: {}", c.repro).unwrap();
    }
    let mut by_kind: BTreeMap<(String, String), usize> = BTreeMap::new();
    for id in &report.findings {
        if let Some(c) = by_id.get(id.as_str()) {
            *by_kind
                .entry((c.verdict.clone(), finding_signature(&c.detail)))
                .or_default() += 1;
        }
    }
    if !by_kind.is_empty() {
        writeln!(
            out,
            "\nFINDINGS BY KIND ({} kinds; digits read as N):",
            by_kind.len()
        )
        .unwrap();
        let mut kinds: Vec<_> = by_kind.into_iter().collect();
        kinds.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        for ((verdict, sig), count) in kinds {
            writeln!(out, "  {count:>3} × [{verdict}] {sig}").unwrap();
        }
    }
    let setup_findings: Vec<&ContactCaseRecord> = report
        .cases
        .iter()
        .filter(|c| c.op.starts_with("setup") && c.verdict != "CORRECT")
        .collect();
    if !setup_findings.is_empty() {
        writeln!(
            out,
            "\nSETUP STEPS NOT CORRECT ({}): the trimmed operand, built alone",
            setup_findings.len()
        )
        .unwrap();
        for c in setup_findings {
            writeln!(out, "  [{}] {}", c.category, c.id).unwrap();
            writeln!(out, "      {}", excerpt(&c.detail, 300)).unwrap();
            writeln!(out, "      repro: {}", c.repro).unwrap();
        }
    }
    out
}

// ── Small 3D vector helpers ─────────────────────────────────────────────────

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(v: [f64; 3], s: f64) -> [f64; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn len(v: [f64; 3]) -> f64 {
    dot(v, v).sqrt()
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let l = len(v);
    [v[0] / l, v[1] / l, v[2] / l]
}

fn perp_len(v: [f64; 3], unit_axis: [f64; 3]) -> f64 {
    len(sub(v, scale(unit_axis, dot(v, unit_axis))))
}
