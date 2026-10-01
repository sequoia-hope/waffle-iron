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
    /// Outer loop **first**, then rings (holes).
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
    /// The outer loop, which is always present and always first.
    pub fn outer_loop(&self) -> &AnalyticLoop {
        &self.loops[0]
    }

    /// The ring (hole) loops.
    pub fn rings(&self) -> &[AnalyticLoop] {
        &self.loops[1..]
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
    fn outer_loop_is_first_and_rings_follow() {
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
        assert_eq!(
            face.outer_loop(),
            &AnalyticLoop::Edges(vec![OrientedEdge {
                edge: 0,
                forward: true
            }])
        );
        assert_eq!(face.rings().len(), 1);
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
