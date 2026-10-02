//! Neutral contract for an **exact analytic B-Rep** extracted from an imported
//! file — STEP import milestone SI5
//! (`specs/step_import_si5_exact_analytic_ingestion.md`).
//!
//! This is the sibling of [`crate::kernel::import`] and the two are deliberately
//! different kinds of thing:
//!
//! - [`ImportedBodyData`](crate::kernel::ImportedBodyData) is **mesh-backed**:
//!   per-face triangles plus a surface *classification*. It is what a file
//!   containing b-spline geometry can honestly become, and it is permanent —
//!   roughly a third of real-world CAD needs it.
//! - [`AnalyticShellData`] is **exact**: every face carries its full surface
//!   parameters, every edge its full curve parameters, and the topology is the
//!   file's own index tables. A shell that survives extraction into this form
//!   can be assembled into the kernel's arena as a real solid, with no
//!   tessellation anywhere in the path.
//!
//! Extraction is **all-or-nothing per shell**: a single face or edge outside the
//! vocabulary makes the whole shell ineligible, loudly, and the caller falls
//! back to the mesh-backed body. This type never holds an approximation — if a
//! value is in here, it came from the file's own analytic definition.
//!
//! Geometry is world-space **meters** (the file's length unit is applied during
//! extraction), matching `ImportedBodyData`.
//!
//! Like the mesh contract these types are RUNTIME-ONLY: the persisted artifact
//! is the compressed source text, re-extracted on rebuild. No serde.

use cad_primitives::{Point3, Vector3};

/// An exact surface, with the full parameters the kernel's own `Surface` enum
/// needs. One variant per member of the kernel's exact vocabulary — there is
/// deliberately **no** `Freeform` arm: a non-elementary surface makes the shell
/// ineligible rather than degrading it.
///
/// `axis_dir` and `normal` are unit vectors. The surface's own sense is kept
/// here; which side is *outside the solid* is
/// [`AnalyticFace::same_sense`]'s job.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnalyticSurface {
    Plane {
        origin: Point3,
        normal: Vector3,
    },
    Cylinder {
        axis_point: Point3,
        axis_dir: Vector3,
        radius: f64,
    },
    /// Single-nappe cone. `half_angle` is in radians, strictly in `(0, π/2)`,
    /// and the nappe is the one on the `+axis_dir` side of `apex`.
    Cone {
        apex: Point3,
        axis_dir: Vector3,
        half_angle: f64,
    },
    Sphere {
        center: Point3,
        radius: f64,
    },
    /// Ring torus: `major_radius > minor_radius`. The spindle and horn cases
    /// are outside the kernel's vocabulary and are rejected at extraction.
    Torus {
        center: Point3,
        axis_dir: Vector3,
        major_radius: f64,
        minor_radius: f64,
    },
}

impl AnalyticSurface {
    /// The classification string shared with the mesh-backed contract, so a
    /// face reports the same `surface_type` whichever tier served it.
    pub fn surface_type_str(&self) -> &'static str {
        match self {
            AnalyticSurface::Plane { .. } => "planar",
            AnalyticSurface::Cylinder { .. } => "cylindrical",
            AnalyticSurface::Cone { .. } => "conical",
            AnalyticSurface::Sphere { .. } => "spherical",
            AnalyticSurface::Torus { .. } => "toroidal",
        }
    }
}

/// An exact edge curve.
///
/// `Line` carries no parameters at all — its endpoints are its definition.
///
/// `normal` is the axis the curve runs counter-clockwise about, as the file
/// defined it; `major_axis` is a unit vector in the ellipse's plane.
///
/// **`interior` is load-bearing, not a convenience.** Two endpoints on a
/// circle define *two* arcs, and the endpoints alone cannot say which one this
/// edge is — the half-turn case is genuinely ambiguous, and a consumer that
/// guesses would silently build the complementary arc. `interior` is a point
/// taken from the middle of the file's own parameter range, so the arc is
/// pinned by construction. For a closed edge it is simply a second point on
/// the curve, which is still what a consumer needs to orient it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnalyticCurve {
    Line,
    Circle {
        center: Point3,
        normal: Vector3,
        radius: f64,
        interior: Point3,
    },
    Ellipse {
        center: Point3,
        normal: Vector3,
        major_axis: Vector3,
        major_radius: f64,
        minor_radius: f64,
        interior: Point3,
    },
}

impl AnalyticCurve {
    pub fn curve_type_str(&self) -> &'static str {
        match self {
            AnalyticCurve::Line => "line",
            AnalyticCurve::Circle { .. } => "circle",
            AnalyticCurve::Ellipse { .. } => "ellipse",
        }
    }
}

/// One edge of the shell, shared by exactly two faces in a closed manifold
/// shell. `start`/`end` index [`AnalyticShellData::vertices`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalyticEdge {
    pub start: u32,
    pub end: u32,
    pub curve: AnalyticCurve,
}

impl AnalyticEdge {
    /// A closed edge — a full circle or ellipse anchored at one vertex, which
    /// is how a seamless rim arrives.
    pub fn is_closed(&self) -> bool {
        self.start == self.end
    }
}

/// An edge as traversed by one loop. `forward == false` means this loop walks
/// the edge from its `end` vertex to its `start` vertex.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrientedEdge {
    pub edge: u32,
    pub forward: bool,
}

/// One boundary loop of a face.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalyticLoop {
    /// Edges in boundary order.
    Edges(Vec<OrientedEdge>),
    /// A loop that is a single vertex (STEP `VERTEX_LOOP`) — a cone apex or a
    /// sphere pole. Kept as topology rather than dropped, because the kernel
    /// has a representation for it.
    Vertex(u32),
}

/// One face: an exact surface plus its boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyticFace {
    pub surface: AnalyticSurface,
    /// The face's boundary loops **in the file's own order, which does NOT
    /// put the outer boundary first**.
    ///
    /// This field once promised outer-first and the promise was unkeepable.
    /// STEP marks the outer boundary with a subtype (`FACE_OUTER_BOUND` rather
    /// than `FACE_BOUND`), not with a position, and real writers interleave
    /// them freely — and the reader we extract from collapses both entities
    /// into one table (`truck_stepio::r#in::FaceBound`, "FACE_OUTER_BOUNDS is
    /// also parsed to this struct"), so **the outer marker is gone before the
    /// data reaches us**. It is the sibling of the dropped `VERTEX_LOOP`: a
    /// silent loss, invisible to a mesh tier whose triangulator re-derives
    /// hole nesting anyway, and a silently-wrong solid for an exact one.
    /// Measured on ABC chunk 0000: 7 of 28 polyhedral models have at least
    /// one face whose first loop is a ring (SI5 C3, 2026-10-01).
    ///
    /// So which loop is outer is the CONSUMER's determination, and it is a
    /// measurement, not a convention: on a planar face, the outer boundary is
    /// the loop whose exact signed area about the face's outward normal is
    /// positive, and every ring's is negative (ISO 10303-42 winds a bound so
    /// the material lies to its left). That test needs the surface's own law —
    /// a curved patch needs its parametric domain, not a 3-D area — which is
    /// knowledge the kernel has and this contract does not. A consumer that
    /// cannot make the determination must refuse the face, never guess.
    pub loops: Vec<AnalyticLoop>,
    /// `false` when the solid's outward direction is opposite the surface's own
    /// normal.
    ///
    /// Orientation is kept AS orientation and never folded into the geometry —
    /// unlike the mesh path, which negates a plane's normal in place
    /// (`step-import/src/convert.rs`). The kernel needs the distinction: a
    /// reversed curved face is a bore wall, which its `Surface` records in a
    /// dedicated `reversed` flag rather than by flipping an axis.
    pub same_sense: bool,
}

impl AnalyticFace {
    /// The face has rings (holes) to tell apart from its outer boundary.
    /// Which loop is which is the consumer's measurement — see [`Self::loops`]
    /// for why there is no `outer_loop()` accessor here.
    pub fn has_rings(&self) -> bool {
        self.loops.len() > 1
    }
}

/// One connected shell, as index tables — the same shape the source file uses,
/// so no geometric stitching or coordinate deduplication is involved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnalyticShellData {
    pub vertices: Vec<Point3>,
    pub edges: Vec<AnalyticEdge>,
    pub faces: Vec<AnalyticFace>,
}

impl AnalyticShellData {
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Euler characteristic ingredients as the file presents them. This is NOT
    /// a validity check — it is the input a consumer needs to back-solve a
    /// shell's genus, and a consumer must still verify the topology itself.
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.vertices.len(), self.edges.len(), self.faces.len())
    }

    /// Uniform scale about the model origin, EXACT on every member of the
    /// vocabulary: points and radii scale, directions and the cone's
    /// half-angle do not. The user's per-import scale factor; must be applied
    /// BEFORE [`Self::apply_placement`], mirroring the mesh contract.
    ///
    /// `scale` must be finite and positive — a mirror or a collapse is not a
    /// scale, and applying one would silently invert every face sense. The
    /// caller refuses such a factor for the exact tier (the feature-engine
    /// serves it from the mesh tier with a warning).
    pub fn apply_scale(&mut self, scale: f64) {
        debug_assert!(scale.is_finite() && scale > 0.0, "apply_scale({scale})");
        if scale == 1.0 {
            return;
        }
        let sp = |p: Point3| Point3::new(p.x() * scale, p.y() * scale, p.z() * scale);
        for p in &mut self.vertices {
            *p = sp(*p);
        }
        for e in &mut self.edges {
            e.curve = match e.curve {
                AnalyticCurve::Line => AnalyticCurve::Line,
                AnalyticCurve::Circle {
                    center,
                    normal,
                    radius,
                    interior,
                } => AnalyticCurve::Circle {
                    center: sp(center),
                    normal,
                    radius: radius * scale,
                    interior: sp(interior),
                },
                AnalyticCurve::Ellipse {
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                    interior,
                } => AnalyticCurve::Ellipse {
                    center: sp(center),
                    normal,
                    major_axis,
                    major_radius: major_radius * scale,
                    minor_radius: minor_radius * scale,
                    interior: sp(interior),
                },
            };
        }
        for f in &mut self.faces {
            f.surface = match f.surface {
                AnalyticSurface::Plane { origin, normal } => AnalyticSurface::Plane {
                    origin: sp(origin),
                    normal,
                },
                AnalyticSurface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => AnalyticSurface::Cylinder {
                    axis_point: sp(axis_point),
                    axis_dir,
                    radius: radius * scale,
                },
                AnalyticSurface::Cone {
                    apex,
                    axis_dir,
                    half_angle,
                } => AnalyticSurface::Cone {
                    apex: sp(apex),
                    axis_dir,
                    half_angle,
                },
                AnalyticSurface::Sphere { center, radius } => AnalyticSurface::Sphere {
                    center: sp(center),
                    radius: radius * scale,
                },
                AnalyticSurface::Torus {
                    center,
                    axis_dir,
                    major_radius,
                    minor_radius,
                } => AnalyticSurface::Torus {
                    center: sp(center),
                    axis_dir,
                    major_radius: major_radius * scale,
                    minor_radius: minor_radius * scale,
                },
            };
        }
    }

    /// Rigid placement: rotate by intrinsic X→Y→Z Euler angles (degrees)
    /// about the model origin, then translate (meters) — the same convention
    /// as [`crate::kernel::ImportedBodyData::apply_placement`]. Every point
    /// moves by `R·p + t`, every direction by `R·v`; radii and angles are
    /// untouched, and a proper rotation preserves every face's sense.
    pub fn apply_placement(&mut self, rotation_deg: [f64; 3], translation_m: [f64; 3]) {
        let r = crate::kernel::rotation_matrix_xyz_deg(rotation_deg);
        let t = translation_m;
        let mul = |v: [f64; 3]| {
            [
                r[0][0] * v[0] + r[0][1] * v[1] + r[0][2] * v[2],
                r[1][0] * v[0] + r[1][1] * v[1] + r[1][2] * v[2],
                r[2][0] * v[0] + r[2][1] * v[1] + r[2][2] * v[2],
            ]
        };
        let pp = |p: Point3| {
            let q = mul([p.x(), p.y(), p.z()]);
            Point3::new(q[0] + t[0], q[1] + t[1], q[2] + t[2])
        };
        let pv = |v: Vector3| {
            let q = mul([v.x(), v.y(), v.z()]);
            Vector3::new(q[0], q[1], q[2])
        };
        for p in &mut self.vertices {
            *p = pp(*p);
        }
        for e in &mut self.edges {
            e.curve = match e.curve {
                AnalyticCurve::Line => AnalyticCurve::Line,
                AnalyticCurve::Circle {
                    center,
                    normal,
                    radius,
                    interior,
                } => AnalyticCurve::Circle {
                    center: pp(center),
                    normal: pv(normal),
                    radius,
                    interior: pp(interior),
                },
                AnalyticCurve::Ellipse {
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                    interior,
                } => AnalyticCurve::Ellipse {
                    center: pp(center),
                    normal: pv(normal),
                    major_axis: pv(major_axis),
                    major_radius,
                    minor_radius,
                    interior: pp(interior),
                },
            };
        }
        for f in &mut self.faces {
            f.surface = match f.surface {
                AnalyticSurface::Plane { origin, normal } => AnalyticSurface::Plane {
                    origin: pp(origin),
                    normal: pv(normal),
                },
                AnalyticSurface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => AnalyticSurface::Cylinder {
                    axis_point: pp(axis_point),
                    axis_dir: pv(axis_dir),
                    radius,
                },
                AnalyticSurface::Cone {
                    apex,
                    axis_dir,
                    half_angle,
                } => AnalyticSurface::Cone {
                    apex: pp(apex),
                    axis_dir: pv(axis_dir),
                    half_angle,
                },
                AnalyticSurface::Sphere { center, radius } => AnalyticSurface::Sphere {
                    center: pp(center),
                    radius,
                },
                AnalyticSurface::Torus {
                    center,
                    axis_dir,
                    major_radius,
                    minor_radius,
                } => AnalyticSurface::Torus {
                    center: pp(center),
                    axis_dir: pv(axis_dir),
                    major_radius,
                    minor_radius,
                },
            };
        }
    }

    /// Every distinct surface classification present, for diagnostics.
    pub fn surface_kinds(&self) -> Vec<&'static str> {
        let mut kinds: Vec<&'static str> = self
            .faces
            .iter()
            .map(|f| f.surface.surface_type_str())
            .collect();
        kinds.sort_unstable();
        kinds.dedup();
        kinds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> AnalyticSurface {
        AnalyticSurface::Plane {
            origin: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        }
    }

    #[test]
    fn a_closed_edge_is_the_one_whose_endpoints_coincide() {
        let open = AnalyticEdge {
            start: 0,
            end: 1,
            curve: AnalyticCurve::Line,
        };
        let rim = AnalyticEdge {
            start: 2,
            end: 2,
            curve: AnalyticCurve::Circle {
                center: Point3::new(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
                interior: Point3::new(-1.0, 0.0, 0.0),
            },
        };
        assert!(!open.is_closed());
        assert!(rim.is_closed());
    }

    #[test]
    fn a_face_with_two_loops_has_rings() {
        let face = AnalyticFace {
            surface: plane(),
            loops: vec![
                AnalyticLoop::Edges(vec![OrientedEdge {
                    edge: 0,
                    forward: true,
                }]),
                AnalyticLoop::Edges(vec![OrientedEdge {
                    edge: 1,
                    forward: false,
                }]),
            ],
            same_sense: true,
        };
        // Which of the two is the outer boundary is NOT recorded here — the
        // file does not put it first and the reader loses the marker, so the
        // consumer measures it (see `AnalyticFace::loops`).
        assert!(face.has_rings());
        assert_eq!(face.loops.len(), 2);
    }

    #[test]
    fn surface_and_curve_classification_strings_match_the_mesh_contract() {
        // The mesh tier and the exact tier must report the same vocabulary, or
        // a face's `surface_type` would change depending on which tier served
        // it — and `surface_type == "planar"` is what gates sketch-on-face.
        assert_eq!(plane().surface_type_str(), "planar");
        assert_eq!(
            AnalyticSurface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            }
            .surface_type_str(),
            "cylindrical"
        );
        assert_eq!(
            crate::kernel::ImportedSurface::Plane {
                origin: [0.0; 3],
                normal: [0.0, 0.0, 1.0],
            }
            .surface_type_str(),
            plane().surface_type_str()
        );
        assert_eq!(AnalyticCurve::Line.curve_type_str(), "line");
    }

    /// A shell with one of every surface and curve kind, so the two
    /// transforms are exercised on every arm.
    fn one_of_everything() -> AnalyticShellData {
        let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
        let v = |x: f64, y: f64, z: f64| Vector3::new(x, y, z);
        AnalyticShellData {
            vertices: vec![p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
            edges: vec![
                AnalyticEdge {
                    start: 0,
                    end: 1,
                    curve: AnalyticCurve::Line,
                },
                AnalyticEdge {
                    start: 0,
                    end: 0,
                    curve: AnalyticCurve::Circle {
                        center: p(0.0, 0.0, 0.0),
                        normal: v(0.0, 0.0, 1.0),
                        radius: 1.0,
                        interior: p(-1.0, 0.0, 0.0),
                    },
                },
                AnalyticEdge {
                    start: 0,
                    end: 1,
                    curve: AnalyticCurve::Ellipse {
                        center: p(0.0, 0.0, 0.0),
                        normal: v(0.0, 0.0, 1.0),
                        major_axis: v(1.0, 0.0, 0.0),
                        major_radius: 1.0,
                        minor_radius: 0.5,
                        interior: p(0.5, 0.0, 0.0),
                    },
                },
            ],
            faces: vec![
                AnalyticFace {
                    surface: plane(),
                    loops: vec![AnalyticLoop::Vertex(0)],
                    same_sense: true,
                },
                AnalyticFace {
                    surface: AnalyticSurface::Cylinder {
                        axis_point: p(0.0, 0.0, 0.0),
                        axis_dir: v(0.0, 0.0, 1.0),
                        radius: 2.0,
                    },
                    loops: vec![AnalyticLoop::Vertex(0)],
                    same_sense: false,
                },
                AnalyticFace {
                    surface: AnalyticSurface::Cone {
                        apex: p(0.0, 0.0, 3.0),
                        axis_dir: v(0.0, 0.0, -1.0),
                        half_angle: 0.4,
                    },
                    loops: vec![AnalyticLoop::Vertex(0)],
                    same_sense: true,
                },
                AnalyticFace {
                    surface: AnalyticSurface::Sphere {
                        center: p(1.0, 1.0, 1.0),
                        radius: 0.25,
                    },
                    loops: vec![AnalyticLoop::Vertex(0)],
                    same_sense: true,
                },
                AnalyticFace {
                    surface: AnalyticSurface::Torus {
                        center: p(0.0, 0.0, 0.0),
                        axis_dir: v(1.0, 0.0, 0.0),
                        major_radius: 3.0,
                        minor_radius: 1.0,
                    },
                    loops: vec![AnalyticLoop::Vertex(0)],
                    same_sense: true,
                },
            ],
        }
    }

    fn near(a: Point3, b: Point3) -> bool {
        (a.x() - b.x()).abs() < 1e-12
            && (a.y() - b.y()).abs() < 1e-12
            && (a.z() - b.z()).abs() < 1e-12
    }

    #[test]
    fn scale_moves_points_and_radii_but_not_directions_or_angles() {
        let mut s = one_of_everything();
        s.apply_scale(2.0);
        assert!(near(s.vertices[0], Point3::new(2.0, 0.0, 0.0)));
        let AnalyticCurve::Circle {
            radius, interior, ..
        } = s.edges[1].curve
        else {
            panic!()
        };
        assert_eq!(radius, 2.0);
        assert!(near(interior, Point3::new(-2.0, 0.0, 0.0)));
        let AnalyticCurve::Ellipse {
            major_radius,
            minor_radius,
            major_axis,
            ..
        } = s.edges[2].curve
        else {
            panic!()
        };
        assert_eq!((major_radius, minor_radius), (2.0, 1.0));
        assert_eq!(
            major_axis,
            Vector3::new(1.0, 0.0, 0.0),
            "directions are not scaled"
        );
        let AnalyticSurface::Cone {
            apex, half_angle, ..
        } = s.faces[2].surface
        else {
            panic!()
        };
        assert!(near(apex, Point3::new(0.0, 0.0, 6.0)));
        assert_eq!(half_angle, 0.4, "the half-angle is scale-free");
        let AnalyticSurface::Torus {
            major_radius,
            minor_radius,
            ..
        } = s.faces[4].surface
        else {
            panic!()
        };
        assert_eq!((major_radius, minor_radius), (6.0, 2.0));
        assert!(!s.faces[1].same_sense, "a positive scale keeps every sense");
    }

    #[test]
    fn placement_rotates_then_translates_points_and_only_rotates_directions() {
        let mut s = one_of_everything();
        // 90° about Z: x̂ → ŷ; then +10 in x.
        s.apply_placement([0.0, 0.0, 90.0], [10.0, 0.0, 0.0]);
        assert!(near(s.vertices[0], Point3::new(10.0, 1.0, 0.0)));
        let AnalyticCurve::Ellipse {
            center, major_axis, ..
        } = s.edges[2].curve
        else {
            panic!()
        };
        assert!(near(center, Point3::new(10.0, 0.0, 0.0)));
        assert!((major_axis.x()).abs() < 1e-12 && (major_axis.y() - 1.0).abs() < 1e-12);
        let AnalyticSurface::Torus {
            axis_dir, center, ..
        } = s.faces[4].surface
        else {
            panic!()
        };
        assert!(
            (axis_dir.y() - 1.0).abs() < 1e-12,
            "the torus axis rotated with it"
        );
        assert!(near(center, Point3::new(10.0, 0.0, 0.0)));
        let AnalyticSurface::Cone { apex, axis_dir, .. } = s.faces[2].surface else {
            panic!()
        };
        assert!(near(apex, Point3::new(10.0, 0.0, 3.0)));
        assert!(
            (axis_dir.z() + 1.0).abs() < 1e-12,
            "an axis along Z is fixed by a Z rotation"
        );
    }

    #[test]
    fn identity_placement_and_unit_scale_are_noops() {
        let before = one_of_everything();
        let mut s = before.clone();
        s.apply_scale(1.0);
        s.apply_placement([0.0; 3], [0.0; 3]);
        assert_eq!(s, before);
    }

    #[test]
    fn surface_kinds_is_sorted_and_deduplicated() {
        let shell = AnalyticShellData {
            vertices: vec![Point3::new(0.0, 0.0, 0.0)],
            edges: vec![],
            faces: vec![
                AnalyticFace {
                    surface: AnalyticSurface::Sphere {
                        center: Point3::new(0.0, 0.0, 0.0),
                        radius: 1.0,
                    },
                    loops: vec![AnalyticLoop::Vertex(0)],
                    same_sense: true,
                },
                AnalyticFace {
                    surface: plane(),
                    loops: vec![AnalyticLoop::Vertex(0)],
                    same_sense: true,
                },
                AnalyticFace {
                    surface: plane(),
                    loops: vec![AnalyticLoop::Vertex(0)],
                    same_sense: false,
                },
            ],
        };
        assert_eq!(shell.surface_kinds(), vec!["planar", "spherical"]);
        assert_eq!(shell.counts(), (1, 0, 3));
    }
}
