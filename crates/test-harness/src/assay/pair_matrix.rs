//! Kernel 1.0 Gate 3.1 — the 21-pair surface-pair matrix generator and report
//! model (`docs/kernel_1_0.md` §2 Gate 3 item 1).
//!
//! Enumerates all 21 unordered pairs of `{plane, cylinder, cone, sphere, torus,
//! B-spline}` (15 analytic-analytic pairs including self-pairs, plus 6 pairs
//! involving B-spline). For the 15 analytic pairs whose surfaces can be
//! authored through the feature engine today, builds deterministic
//! general-position `.waffle` documents for each boolean operation (`union`,
//! `cut`, `intersect`) and seed. The 6 B-spline rows report
//! [`NOT_BUILDABLE_BSPLINE`] honestly because `kernel-v2` and `feature-engine`
//! do not yet expose a B-spline face constructor.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::process::Command;

use feature_engine::types::BooleanOp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use waffle_types::SketchPlaneBasis;

use crate::assay::categorize::{CaseOutcome, Category};
use crate::assay::cpu_runner::RunProvenance;
use crate::assay::exact_membership::{ExactChain, ExactVolumeVerdict};
use crate::assay::gen::AssayMeta;
use crate::assay::prospect::{derive_meta, gen3::Rng};
use crate::workflow::ModelBuilder;

/// Honest status reported for the 6 surface pairs involving `B-spline`:
/// `kernel_v2::arena::Surface` only carries `{Plane, Cylinder, Cone, Torus,
/// Sphere}`, `feature_engine::types::Operation` has no `Loft` variant yet
/// (`crates/nurbs-rs` landed Loft L1a math only), sketch `spline_segments`
/// extrude as planar chord polygons or `NotSupported`, and STEP B-spline faces
/// route to the non-boolean mesh import tier.
pub const NOT_BUILDABLE_BSPLINE: &str = "NOT BUILDABLE (no B-spline face source)";

/// The six Gate 3.1 surface kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SurfaceKind {
    Plane,
    Cylinder,
    Cone,
    Sphere,
    Torus,
    BSpline,
}

impl SurfaceKind {
    /// All six surface kinds in canonical Gate 3.1 order.
    pub const ALL: [SurfaceKind; 6] = [
        SurfaceKind::Plane,
        SurfaceKind::Cylinder,
        SurfaceKind::Cone,
        SurfaceKind::Sphere,
        SurfaceKind::Torus,
        SurfaceKind::BSpline,
    ];

    /// The five analytic surface kinds currently constructible through the
    /// feature engine.
    pub const ANALYTIC: [SurfaceKind; 5] = [
        SurfaceKind::Plane,
        SurfaceKind::Cylinder,
        SurfaceKind::Cone,
        SurfaceKind::Sphere,
        SurfaceKind::Torus,
    ];

    /// Human-readable surface name matching `docs/kernel_1_0.md`.
    pub const fn label(self) -> &'static str {
        match self {
            SurfaceKind::Plane => "plane",
            SurfaceKind::Cylinder => "cylinder",
            SurfaceKind::Cone => "cone",
            SurfaceKind::Sphere => "sphere",
            SurfaceKind::Torus => "torus",
            SurfaceKind::BSpline => "B-spline",
        }
    }

    /// Lowercase ASCII identifier used in case IDs and filenames.
    pub const fn slug(self) -> &'static str {
        match self {
            SurfaceKind::Plane => "plane",
            SurfaceKind::Cylinder => "cylinder",
            SurfaceKind::Cone => "cone",
            SurfaceKind::Sphere => "sphere",
            SurfaceKind::Torus => "torus",
            SurfaceKind::BSpline => "bspline",
        }
    }

    /// Whether a feature in `feature-engine` + `kernel-v2` can build a face of
    /// this surface kind today.
    pub const fn is_buildable(self) -> bool {
        !matches!(self, SurfaceKind::BSpline)
    }

    /// Feature-engine recipe that produces this surface kind, or `None` when no
    /// feature can produce it yet.
    pub const fn feature_recipe(self) -> Option<&'static str> {
        match self {
            SurfaceKind::Plane => Some("extrude_no_merge(polygon_sketch: 5-gon prism face)"),
            SurfaceKind::Cylinder => Some("extrude_no_merge(true_circle_sketch)"),
            SurfaceKind::Cone => Some("revolve(polygon_sketch: on-axis frustum 4-gon, 360 deg)"),
            SurfaceKind::Sphere => Some("revolve(true_circle_sketch: on-axis circle, 360 deg)"),
            SurfaceKind::Torus => Some("revolve(true_circle_sketch: off-axis circle, 360 deg)"),
            SurfaceKind::BSpline => None,
        }
    }

    /// Expected `TopoSignature.surface_type` string emitted by `kernel-v2`
    /// (`kernel_v2::signature::surface_type_str`) for the target face.
    pub const fn introspect_surface_type(self) -> Option<&'static str> {
        match self {
            SurfaceKind::Plane => Some("planar"),
            SurfaceKind::Cylinder => Some("cylindrical"),
            SurfaceKind::Cone => Some("conical"),
            SurfaceKind::Sphere => Some("spherical"),
            SurfaceKind::Torus => Some("toroidal"),
            SurfaceKind::BSpline => None,
        }
    }
}

/// One of the three boolean operations exercised per `(pair, seed)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MatrixOp {
    Union,
    Cut,
    Intersect,
}

impl MatrixOp {
    pub const ALL: [MatrixOp; 3] = [MatrixOp::Union, MatrixOp::Cut, MatrixOp::Intersect];

    pub const fn slug(self) -> &'static str {
        match self {
            MatrixOp::Union => "union",
            MatrixOp::Cut => "cut",
            MatrixOp::Intersect => "intersect",
        }
    }

    pub const fn boolean_op(self) -> BooleanOp {
        match self {
            MatrixOp::Union => BooleanOp::Union,
            MatrixOp::Cut => BooleanOp::Subtract,
            MatrixOp::Intersect => BooleanOp::Intersect,
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "union" => Some(MatrixOp::Union),
            "cut" => Some(MatrixOp::Cut),
            "intersect" => Some(MatrixOp::Intersect),
            _ => None,
        }
    }
}

/// An unordered pair `(a, b)` of surface kinds (`a <= b`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SurfacePair {
    pub index: usize,
    pub a: SurfaceKind,
    pub b: SurfaceKind,
}

impl SurfacePair {
    /// Display label, e.g. `"plane x cylinder"` or `"torus x B-spline"`.
    pub fn label(self) -> String {
        format!("{} × {}", self.a.label(), self.b.label())
    }

    /// ASCII slug, e.g. `"plane_cylinder"`.
    pub fn slug(self) -> String {
        format!("{}_{}", self.a.slug(), self.b.slug())
    }

    /// True when both operand surfaces can be built through the feature engine.
    pub const fn is_buildable(self) -> bool {
        self.a.is_buildable() && self.b.is_buildable()
    }

    /// Human-readable description of which feature builds each surface in this
    /// pair, or [`NOT_BUILDABLE_BSPLINE`] when unbuildable.
    pub fn feature_sources_summary(self) -> String {
        match (self.a.feature_recipe(), self.b.feature_recipe()) {
            (Some(fa), Some(fb)) => format!(
                "A ({}) = {fa}; B ({}) = {fb}",
                self.a.label(),
                self.b.label()
            ),
            _ => NOT_BUILDABLE_BSPLINE.to_string(),
        }
    }

    /// Deterministic case ID for `(self, op, seed)`.
    pub fn case_id(self, op: MatrixOp, seed: u64) -> String {
        format!(
            "pm_{:02}_{}_{}_{}_s{}",
            self.index,
            self.a.slug(),
            self.b.slug(),
            op.slug(),
            seed
        )
    }
}

/// All 21 unordered surface pairs: the 15 analytic-analytic pairs first, then
/// the 6 pairs involving `B-spline`.
pub const ALL_SURFACE_PAIRS: [SurfacePair; 21] = [
    SurfacePair {
        index: 0,
        a: SurfaceKind::Plane,
        b: SurfaceKind::Plane,
    },
    SurfacePair {
        index: 1,
        a: SurfaceKind::Plane,
        b: SurfaceKind::Cylinder,
    },
    SurfacePair {
        index: 2,
        a: SurfaceKind::Plane,
        b: SurfaceKind::Cone,
    },
    SurfacePair {
        index: 3,
        a: SurfaceKind::Plane,
        b: SurfaceKind::Sphere,
    },
    SurfacePair {
        index: 4,
        a: SurfaceKind::Plane,
        b: SurfaceKind::Torus,
    },
    SurfacePair {
        index: 5,
        a: SurfaceKind::Cylinder,
        b: SurfaceKind::Cylinder,
    },
    SurfacePair {
        index: 6,
        a: SurfaceKind::Cylinder,
        b: SurfaceKind::Cone,
    },
    SurfacePair {
        index: 7,
        a: SurfaceKind::Cylinder,
        b: SurfaceKind::Sphere,
    },
    SurfacePair {
        index: 8,
        a: SurfaceKind::Cylinder,
        b: SurfaceKind::Torus,
    },
    SurfacePair {
        index: 9,
        a: SurfaceKind::Cone,
        b: SurfaceKind::Cone,
    },
    SurfacePair {
        index: 10,
        a: SurfaceKind::Cone,
        b: SurfaceKind::Sphere,
    },
    SurfacePair {
        index: 11,
        a: SurfaceKind::Cone,
        b: SurfaceKind::Torus,
    },
    SurfacePair {
        index: 12,
        a: SurfaceKind::Sphere,
        b: SurfaceKind::Sphere,
    },
    SurfacePair {
        index: 13,
        a: SurfaceKind::Sphere,
        b: SurfaceKind::Torus,
    },
    SurfacePair {
        index: 14,
        a: SurfaceKind::Torus,
        b: SurfaceKind::Torus,
    },
    SurfacePair {
        index: 15,
        a: SurfaceKind::Plane,
        b: SurfaceKind::BSpline,
    },
    SurfacePair {
        index: 16,
        a: SurfaceKind::Cylinder,
        b: SurfaceKind::BSpline,
    },
    SurfacePair {
        index: 17,
        a: SurfaceKind::Cone,
        b: SurfaceKind::BSpline,
    },
    SurfacePair {
        index: 18,
        a: SurfaceKind::Sphere,
        b: SurfaceKind::BSpline,
    },
    SurfacePair {
        index: 19,
        a: SurfaceKind::Torus,
        b: SurfaceKind::BSpline,
    },
    SurfacePair {
        index: 20,
        a: SurfaceKind::BSpline,
        b: SurfaceKind::BSpline,
    },
];

/// Parse a case ID emitted by [`SurfacePair::case_id`] back into
/// `(SurfacePair, MatrixOp, seed)`.
pub fn parse_case_id(id: &str) -> Option<(SurfacePair, MatrixOp, u64)> {
    let parts: Vec<&str> = id.split('_').collect();
    if parts.len() != 6 || parts[0] != "pm" {
        return None;
    }
    let idx: usize = parts[1].parse().ok()?;
    let pair = *ALL_SURFACE_PAIRS.get(idx)?;
    if pair.a.slug() != parts[2] || pair.b.slug() != parts[3] {
        return None;
    }
    let op = MatrixOp::from_slug(parts[4])?;
    let seed_str = parts[5].strip_prefix('s')?;
    let seed: u64 = seed_str.parse().ok()?;
    Some((pair, op, seed))
}

// ── General-Position Analytic Operand Geometry ──────────────────────────────

/// Exact analytic description of one standalone operand body, placed so its
/// designated target surface passes cleanly through `crossing_point`.
#[derive(Debug, Clone)]
pub enum OperandGeom {
    /// Asymmetric convex 5-gon extruded standalone along `sketch_normal`.
    /// Target surface is the `t = 0` planar cap through `sketch_origin` with
    /// outward normal `target_normal = -sketch_normal`.
    Prism {
        sketch_origin: [f64; 3],
        sketch_normal: [f64; 3],
        vertices_uv: Vec<(f64, f64)>,
        depth: f64,
        target_normal: [f64; 3],
    },
    /// True circle extruded standalone along `axis_dir`.
    /// Target surface is the lateral `Surface::Cylinder`.
    Cylinder {
        sketch_origin: [f64; 3],
        axis_dir: [f64; 3],
        radius: f64,
        depth: f64,
        target_normal_at_cross: [f64; 3],
    },
    /// On-axis 4-gon frustum revolved 360° about `(axis_origin, axis_dir)`.
    /// Target surface is the lateral `Surface::Cone`.
    ConeFrustum {
        axis_origin: [f64; 3],
        sketch_normal: [f64; 3],
        axis_dir: [f64; 3],
        vertices_uv: [(f64, f64); 4],
        r_bot: f64,
        r_top: f64,
        height: f64,
        half_angle: f64,
        target_normal_at_cross: [f64; 3],
    },
    /// On-axis circle revolved 360° about `(center, axis_dir)`.
    /// Target surface is the single closed `Surface::Sphere`.
    Sphere {
        center: [f64; 3],
        sketch_normal: [f64; 3],
        axis_dir: [f64; 3],
        radius: f64,
        target_normal_at_cross: [f64; 3],
    },
    /// Off-axis circle revolved 360° about `(center, axis_dir)`.
    /// Target surface is the single closed `Surface::Torus`.
    Torus {
        center: [f64; 3],
        sketch_normal: [f64; 3],
        axis_dir: [f64; 3],
        circle_uv: (f64, f64),
        major_radius: f64,
        minor_radius: f64,
        seam_outer_point: [f64; 3],
        target_normal_at_cross: [f64; 3],
    },
}

/// Drawn pair of general-position operands sharing a transverse surface
/// crossing at `crossing_point`.
#[derive(Debug, Clone)]
pub struct PairOperands {
    pub pair: SurfacePair,
    pub seed: u64,
    pub crossing_point: [f64; 3],
    pub a: OperandGeom,
    pub b: OperandGeom,
}

/// One generated `.waffle` document and derived `AssayMeta` ready for
/// [`crate::assay::categorize::categorize`].
#[derive(Debug, Clone)]
pub struct GeneratedPairCase {
    pub id: String,
    pub pair: SurfacePair,
    pub op: MatrixOp,
    pub seed: u64,
    pub waffle_json: String,
    pub meta: AssayMeta,
}

impl OperandGeom {
    /// Surface kind authored by this operand.
    pub fn kind(&self) -> SurfaceKind {
        match self {
            OperandGeom::Prism { .. } => SurfaceKind::Plane,
            OperandGeom::Cylinder { .. } => SurfaceKind::Cylinder,
            OperandGeom::ConeFrustum { .. } => SurfaceKind::Cone,
            OperandGeom::Sphere { .. } => SurfaceKind::Sphere,
            OperandGeom::Torus { .. } => SurfaceKind::Torus,
        }
    }

    /// Outward unit normal of the target surface at `crossing_point`.
    pub fn target_normal_at_cross(&self) -> [f64; 3] {
        match self {
            OperandGeom::Prism { target_normal, .. } => *target_normal,
            OperandGeom::Cylinder {
                target_normal_at_cross,
                ..
            }
            | OperandGeom::ConeFrustum {
                target_normal_at_cross,
                ..
            }
            | OperandGeom::Sphere {
                target_normal_at_cross,
                ..
            }
            | OperandGeom::Torus {
                target_normal_at_cross,
                ..
            } => *target_normal_at_cross,
        }
    }

    /// Primary symmetry / cylinder / revolution axis `(point_on_axis, unit_dir)`
    /// when the target surface carries a 1D axis (`Cylinder`, `ConeFrustum`,
    /// `Torus`). Planar and spherical target surfaces have no unique 1D axis.
    pub fn primary_axis(&self) -> Option<([f64; 3], [f64; 3])> {
        match self {
            OperandGeom::Prism { .. } | OperandGeom::Sphere { .. } => None,
            OperandGeom::Cylinder {
                sketch_origin,
                axis_dir,
                ..
            } => Some((*sketch_origin, *axis_dir)),
            OperandGeom::ConeFrustum {
                axis_origin,
                axis_dir,
                ..
            } => Some((*axis_origin, *axis_dir)),
            OperandGeom::Torus {
                center, axis_dir, ..
            } => Some((*center, *axis_dir)),
        }
    }

    /// Signed distance-like residual to the infinite target analytic surface at
    /// `p` (`0` on the target surface, `< 0` on the interior side near
    /// `crossing_point`, `> 0` outside).
    pub fn target_surface_residual(&self, p: [f64; 3]) -> f64 {
        match self {
            OperandGeom::Prism {
                sketch_origin,
                target_normal,
                ..
            } => dot(sub(p, *sketch_origin), *target_normal),
            OperandGeom::Cylinder {
                sketch_origin,
                axis_dir,
                radius,
                ..
            } => perp_len(sub(p, *sketch_origin), *axis_dir) - *radius,
            OperandGeom::ConeFrustum {
                axis_origin,
                axis_dir,
                r_bot,
                r_top,
                height,
                ..
            } => {
                let d = sub(p, *axis_origin);
                let t = dot(d, *axis_dir);
                let r_at_t = *r_bot + (*r_top - *r_bot) * (t / *height);
                perp_len(d, *axis_dir) - r_at_t
            }
            OperandGeom::Sphere { center, radius, .. } => len(sub(p, *center)) - *radius,
            OperandGeom::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
                ..
            } => {
                let d = sub(p, *center);
                let tau = dot(d, *axis_dir);
                let rho = perp_len(d, *axis_dir);
                ((rho - *major_radius).powi(2) + tau * tau).sqrt() - *minor_radius
            }
        }
    }

    /// All planar faces `(point_on_plane, outward_unit_normal)` carried by this
    /// solid (7 faces for the 5-gon prism, 2 end-cap planes for cylinder/cone,
    /// 0 for sphere/torus).
    pub fn planar_faces(&self) -> Vec<([f64; 3], [f64; 3])> {
        match self {
            OperandGeom::Prism {
                sketch_origin,
                sketch_normal,
                vertices_uv,
                depth,
                target_normal,
            } => {
                let basis = SketchPlaneBasis::from_origin_normal(*sketch_origin, *sketch_normal);
                let mut planes = Vec::with_capacity(vertices_uv.len() + 2);
                planes.push((*sketch_origin, *target_normal));
                planes.push((
                    add(*sketch_origin, scale(*sketch_normal, *depth)),
                    *sketch_normal,
                ));
                let n = vertices_uv.len();
                for i in 0..n {
                    let (u0, v0) = vertices_uv[i];
                    let (u1, v1) = vertices_uv[(i + 1) % n];
                    let (du, dv) = (u1 - u0, v1 - v0);
                    let side_n = norm(add(scale(basis.x_axis, dv), scale(basis.y_axis, -du)));
                    let pt = add(
                        *sketch_origin,
                        add(scale(basis.x_axis, u0), scale(basis.y_axis, v0)),
                    );
                    planes.push((pt, side_n));
                }
                planes
            }
            OperandGeom::Cylinder {
                sketch_origin,
                axis_dir,
                depth,
                ..
            } => vec![
                (*sketch_origin, scale(*axis_dir, -1.0)),
                (add(*sketch_origin, scale(*axis_dir, *depth)), *axis_dir),
            ],
            OperandGeom::ConeFrustum {
                axis_origin,
                axis_dir,
                height,
                ..
            } => vec![
                (*axis_origin, scale(*axis_dir, -1.0)),
                (add(*axis_origin, scale(*axis_dir, *height)), *axis_dir),
            ],
            OperandGeom::Sphere { .. } | OperandGeom::Torus { .. } => Vec::new(),
        }
    }

    /// Seam/pole vertices of closed curved surfaces so we can keep them well
    /// clear of partner surfaces (avoiding unintended vertex-on-surface
    /// coincidences).
    ///
    /// For `Sphere`, checks the real oblique revolve poles `center ± R·axis_dir`
    /// and in-sketch meridian `center + R·(sketch_normal × axis_dir)` (as well
    /// as `kernel-v2` `assemble_closed_sphere`'s canonical world `±Z` / `+X`
    /// seam frame in `construct/revolve/closed.rs:450-451`).
    fn seam_vertices(&self) -> Vec<[f64; 3]> {
        match self {
            OperandGeom::Sphere {
                center,
                sketch_normal,
                axis_dir,
                radius,
                ..
            } => {
                let w_inplane = norm(cross(*sketch_normal, *axis_dir));
                vec![
                    add(*center, scale(*axis_dir, *radius)),
                    sub(*center, scale(*axis_dir, *radius)),
                    add(*center, scale(w_inplane, *radius)),
                    [center[0], center[1], center[2] + *radius],
                    [center[0], center[1], center[2] - *radius],
                    [center[0] + *radius, center[1], center[2]],
                ]
            }
            OperandGeom::Torus {
                seam_outer_point, ..
            } => vec![*seam_outer_point],
            _ => Vec::new(),
        }
    }

    /// Minimum distance from `p` to any surface sheet (target or planar cap) of
    /// `self` — used only to ensure partner seam/pole vertices do not land on
    /// `self`'s boundary.
    fn min_boundary_clearance(&self, p: [f64; 3]) -> f64 {
        let mut best = self.target_surface_residual(p).abs();
        for (pt, n) in self.planar_faces() {
            best = best.min(dot(sub(p, pt), n).abs());
        }
        best
    }

    /// Author this standalone body into `builder` under feature names `sk_name`
    /// and `body_name` (non-merging so `body_0` and `body_1` remain independent
    /// operands until `BooleanCombine`).
    pub fn add_to_builder(
        &self,
        builder: &mut ModelBuilder,
        sk_name: &str,
        body_name: &str,
    ) -> Result<(), String> {
        let err = |e: crate::helpers::HarnessError| e.to_string();
        match self {
            OperandGeom::Prism {
                sketch_origin,
                sketch_normal,
                vertices_uv,
                depth,
                ..
            } => {
                builder
                    .polygon_sketch(sk_name, *sketch_origin, *sketch_normal, vertices_uv)
                    .map_err(err)?;
                builder
                    .extrude_no_merge(body_name, sk_name, *depth)
                    .map_err(err)?;
            }
            OperandGeom::Cylinder {
                sketch_origin,
                axis_dir,
                radius,
                depth,
                ..
            } => {
                builder
                    .true_circle_sketch(sk_name, *sketch_origin, *axis_dir, 0.0, 0.0, *radius)
                    .map_err(err)?;
                builder
                    .extrude_no_merge(body_name, sk_name, *depth)
                    .map_err(err)?;
            }
            OperandGeom::ConeFrustum {
                axis_origin,
                sketch_normal,
                axis_dir,
                vertices_uv,
                ..
            } => {
                builder
                    .polygon_sketch(sk_name, *axis_origin, *sketch_normal, vertices_uv)
                    .map_err(err)?;
                builder
                    .revolve(body_name, sk_name, *axis_origin, *axis_dir, 360.0)
                    .map_err(err)?;
            }
            OperandGeom::Sphere {
                center,
                sketch_normal,
                axis_dir,
                radius,
                ..
            } => {
                builder
                    .true_circle_sketch(sk_name, *center, *sketch_normal, 0.0, 0.0, *radius)
                    .map_err(err)?;
                builder
                    .revolve(body_name, sk_name, *center, *axis_dir, 360.0)
                    .map_err(err)?;
            }
            OperandGeom::Torus {
                center,
                sketch_normal,
                axis_dir,
                circle_uv: (cx, cy),
                minor_radius,
                ..
            } => {
                builder
                    .true_circle_sketch(sk_name, *center, *sketch_normal, *cx, *cy, *minor_radius)
                    .map_err(err)?;
                builder
                    .revolve(body_name, sk_name, *center, *axis_dir, 360.0)
                    .map_err(err)?;
            }
        }
        Ok(())
    }
}

/// Deterministic PRNG stream for `(pair_index, seed)`.
fn seeded_rng(pair_index: usize, seed: u64) -> Rng {
    let mixed = 0x5041_4952_4D54_5831u64
        ^ ((pair_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        ^ seed.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let mut rng = Rng::new(mixed);
    for _ in 0..4 {
        let _ = rng.next_u64();
    }
    rng
}

/// Draw a unit vector whose components are all strictly oblique to the world
/// axes (`|u_i| >= 0.18` and `|u_z| <= 0.88`).
fn draw_oblique_unit(rng: &mut Rng) -> [f64; 3] {
    loop {
        let z = rng.range(-0.85, 0.85);
        let phi = rng.range(0.0, std::f64::consts::TAU);
        let r = (1.0 - z * z).sqrt();
        let v = [r * phi.cos(), r * phi.sin(), z];
        if v[0].abs() >= 0.18 && v[1].abs() >= 0.18 && v[2].abs() >= 0.18 && v[2].abs() <= 0.88 {
            return norm(v);
        }
    }
}

/// Draw a unit vector perpendicular to `axis` that is also oblique to world Z
/// (`|n_z| <= 0.88`, avoiding the `0.99` `SketchPlaneBasis` reference switch).
fn draw_perp_oblique_unit(rng: &mut Rng, axis: [f64; 3]) -> [f64; 3] {
    loop {
        let g = draw_oblique_unit(rng);
        let proj = sub(g, scale(axis, dot(g, axis)));
        let l = len(proj);
        if l >= 0.25 {
            let u = [proj[0] / l, proj[1] / l, proj[2] / l];
            if u[0].abs() >= 0.12 && u[1].abs() >= 0.12 && u[2].abs() <= 0.88 {
                return u;
            }
        }
    }
}

fn draw_single_operand(rng: &mut Rng, kind: SurfaceKind, p_cross: [f64; 3]) -> OperandGeom {
    match kind {
        SurfaceKind::Plane => {
            let target_normal = draw_oblique_unit(rng);
            let sketch_normal = scale(target_normal, -1.0);
            let basis = SketchPlaneBasis::from_origin_normal([0.0; 3], sketch_normal);
            let r_poly = rng.range(2.2, 2.9);
            let rot = rng.range(0.11, 1.19);
            // Asymmetric strictly-convex cyclic 5-gon so no two side faces are
            // parallel or orthogonal.
            let mut vertices_uv = Vec::with_capacity(5);
            for k in 0..5 {
                let jitter = rng.range(-0.10, 0.10);
                let ang = rot + std::f64::consts::TAU * (k as f64) / 5.0 + jitter;
                vertices_uv.push((r_poly * ang.cos(), r_poly * ang.sin()));
            }
            let du = rng.range(-0.42, 0.42);
            let dv = rng.range(-0.42, 0.42);
            let sketch_origin = sub(
                p_cross,
                add(scale(basis.x_axis, du), scale(basis.y_axis, dv)),
            );
            let depth = rng.range(2.3, 3.2);
            OperandGeom::Prism {
                sketch_origin,
                sketch_normal,
                vertices_uv,
                depth,
                target_normal,
            }
        }
        SurfaceKind::Cylinder => {
            let axis_dir = draw_oblique_unit(rng);
            let e1 = draw_perp_oblique_unit(rng, axis_dir);
            let e2 = cross(axis_dir, e1);
            let phi = rng.range(0.35, 5.85);
            let e_rad = norm(add(scale(e1, phi.cos()), scale(e2, phi.sin())));
            let radius = rng.range(1.35, 2.15);
            let depth = rng.range(3.5, 4.7);
            let t_cross = depth * rng.range(0.36, 0.64);
            let axis_at_cross = sub(p_cross, scale(e_rad, radius));
            let sketch_origin = sub(axis_at_cross, scale(axis_dir, t_cross));
            OperandGeom::Cylinder {
                sketch_origin,
                axis_dir,
                radius,
                depth,
                target_normal_at_cross: e_rad,
            }
        }
        SurfaceKind::Cone => {
            let axis_dir = draw_oblique_unit(rng);
            let sketch_normal = draw_perp_oblique_unit(rng, axis_dir);
            let basis = SketchPlaneBasis::from_origin_normal([0.0; 3], sketch_normal);
            // Right-handed in-plane radial unit vector `w = n_sk × axis_dir`.
            let w_inplane = norm(cross(sketch_normal, axis_dir));
            let m_perp = cross(axis_dir, w_inplane);
            let r_bot = rng.range(1.70, 2.35);
            let r_top = rng.range(0.65, 1.20);
            let height = rng.range(3.3, 4.4);
            let alpha = rng.range(0.36, 0.64);
            let t_cross = alpha * height;
            let r_cross = r_bot + (r_top - r_bot) * alpha;
            let theta = rng.range(0.55, 2.55);
            let e_rad = norm(add(
                scale(w_inplane, theta.cos()),
                scale(m_perp, theta.sin()),
            ));
            let axis_at_cross = sub(p_cross, scale(e_rad, r_cross));
            let axis_origin = sub(axis_at_cross, scale(axis_dir, t_cross));
            let to_uv = |t_ax: f64, r_rad: f64| -> (f64, f64) {
                let v3 = add(scale(axis_dir, t_ax), scale(w_inplane, r_rad));
                (dot(v3, basis.x_axis), dot(v3, basis.y_axis))
            };
            // CCW 4-gon in `(basis.x_axis, basis.y_axis)` with one on-axis edge
            // `(0,0) -> (height,0)`, two perpendicular disc-cap edges, and one
            // oblique slant edge `(height, r_top) -> (0, r_bot)`.
            let vertices_uv = [
                to_uv(0.0, 0.0),
                to_uv(height, 0.0),
                to_uv(height, r_top),
                to_uv(0.0, r_bot),
            ];
            let slope = (r_bot - r_top) / height;
            let half_angle = slope.atan();
            let target_normal_at_cross = norm(add(e_rad, scale(axis_dir, slope)));
            OperandGeom::ConeFrustum {
                axis_origin,
                sketch_normal,
                axis_dir,
                vertices_uv,
                r_bot,
                r_top,
                height,
                half_angle,
                target_normal_at_cross,
            }
        }
        SurfaceKind::Sphere => {
            let axis_dir = draw_oblique_unit(rng);
            let sketch_normal = draw_perp_oblique_unit(rng, axis_dir);
            let w_inplane = norm(cross(sketch_normal, axis_dir));
            let target_normal = loop {
                let n = draw_oblique_unit(rng);
                // Keep crossing point away from both the oblique revolve poles /
                // sketch meridian (`center ± R·axis_dir`, `w_inplane`) and
                // `assemble_closed_sphere`'s canonical z-up +X seam / ±Z poles.
                let near_revolve_pole = dot(n, axis_dir).abs() > 0.85;
                let near_revolve_seam =
                    dot(n, sketch_normal).abs() < 0.22 && dot(n, w_inplane) > 0.0;
                let near_canonical_seam = n[1].abs() < 0.22 && n[0] > 0.0;
                if !near_revolve_pole && !near_revolve_seam && !near_canonical_seam {
                    break n;
                }
            };
            let radius = rng.range(1.45, 2.25);
            let center = sub(p_cross, scale(target_normal, radius));
            OperandGeom::Sphere {
                center,
                sketch_normal,
                axis_dir,
                radius,
                target_normal_at_cross: target_normal,
            }
        }
        SurfaceKind::Torus => {
            let axis_dir = draw_oblique_unit(rng);
            let sketch_normal = draw_perp_oblique_unit(rng, axis_dir);
            let basis = SketchPlaneBasis::from_origin_normal([0.0; 3], sketch_normal);
            let w_inplane = norm(cross(sketch_normal, axis_dir));
            let m_perp = cross(axis_dir, w_inplane);
            let minor_radius = rng.range(0.62, 0.94);
            let major_radius = minor_radius * rng.range(2.15, 2.75);
            // Generic toroidal angle `theta` and poloidal angle `phi` away from
            // both seam circles and equators/crowns.
            let theta = rng.range(0.65, 2.45);
            let phi = rng.range(0.42, 1.15);
            let u_ring = norm(add(
                scale(w_inplane, theta.cos()),
                scale(m_perp, theta.sin()),
            ));
            let target_normal = norm(add(scale(u_ring, phi.cos()), scale(axis_dir, phi.sin())));
            let center = sub(
                sub(p_cross, scale(u_ring, major_radius)),
                scale(target_normal, minor_radius),
            );
            let circle_uv = (
                major_radius * dot(w_inplane, basis.x_axis),
                major_radius * dot(w_inplane, basis.y_axis),
            );
            let seam_outer_point = add(center, scale(w_inplane, major_radius + minor_radius));
            OperandGeom::Torus {
                center,
                sketch_normal,
                axis_dir,
                circle_uv,
                major_radius,
                minor_radius,
                seam_outer_point,
                target_normal_at_cross: target_normal,
            }
        }
        SurfaceKind::BSpline => unreachable!("BSpline is not buildable"),
    }
}

/// Pure analytic check that `ops` satisfies every Gate 3.1 general-position
/// requirement:
/// - `crossing_point` lies on both target surfaces (`|residual| < 1e-11`),
/// - target surface normals at `crossing_point` cross transversely
///   (`0.18 <= |n_a · n_b| <= 0.82`),
/// - operand axes are oblique (`0.15 <= |a_a · a_b| <= 0.85`) and strictly skew
///   in 3D (`skew_distance >= 0.20`, never shared or intersecting),
/// - distinct radii / half-angles / centers on self-pairs and curved pairs,
/// - no parallel or coplanar planar faces across the two solids,
/// - no cap-to-sphere or cap-to-cylinder tangency, and seam/pole vertices clear
///   of the partner's boundary.
pub fn verify_general_position(ops: &PairOperands) -> Result<(), String> {
    let p = ops.crossing_point;
    let res_a = ops.a.target_surface_residual(p).abs();
    let res_b = ops.b.target_surface_residual(p).abs();
    if res_a > 1e-11 || res_b > 1e-11 {
        return Err(format!(
            "crossing_point off target surface: res_a={res_a:.3e}, res_b={res_b:.3e}"
        ));
    }

    let na = ops.a.target_normal_at_cross();
    let nb = ops.b.target_normal_at_cross();
    let ndot = dot(na, nb).abs();
    if !(0.18..=0.82).contains(&ndot) {
        return Err(format!(
            "target normals not in general transverse range: |na·nb|={ndot:.4}"
        ));
    }

    // Skew, non-parallel, non-perpendicular axes when both operands carry axes.
    if let (Some((pa, da)), Some((pb, db))) = (ops.a.primary_axis(), ops.b.primary_axis()) {
        let adot = dot(da, db).abs();
        if !(0.15..=0.85).contains(&adot) {
            return Err(format!(
                "axes nearly parallel or orthogonal: |da·db|={adot:.4}"
            ));
        }
        let axb = norm(cross(da, db));
        let skew = dot(sub(pb, pa), axb).abs();
        if skew < 0.20 {
            return Err(format!("axes nearly intersect / shared: skew={skew:.4}"));
        }
    }

    // Distinct shape parameters on curved pairs.
    match (&ops.a, &ops.b) {
        (OperandGeom::Cylinder { radius: ra, .. }, OperandGeom::Cylinder { radius: rb, .. })
        | (OperandGeom::Sphere { radius: ra, .. }, OperandGeom::Sphere { radius: rb, .. })
        | (OperandGeom::Cylinder { radius: ra, .. }, OperandGeom::Sphere { radius: rb, .. })
        | (OperandGeom::Sphere { radius: ra, .. }, OperandGeom::Cylinder { radius: rb, .. })
            if (ra - rb).abs() < 0.14 =>
        {
            return Err(format!("radii too close: {ra:.4} vs {rb:.4}"));
        }
        (
            OperandGeom::ConeFrustum { half_angle: aa, .. },
            OperandGeom::ConeFrustum { half_angle: ab, .. },
        ) if (aa - ab).abs() < 0.035 => {
            return Err(format!("cone half-angles too close: {aa:.4} vs {ab:.4}"));
        }
        (
            OperandGeom::Torus {
                major_radius: ma,
                minor_radius: na_r,
                ..
            },
            OperandGeom::Torus {
                major_radius: mb,
                minor_radius: nb_r,
                ..
            },
        ) if (ma - mb).abs() < 0.18 || (na_r - nb_r).abs() < 0.06 => {
            return Err("torus radii too close".to_string());
        }
        _ => {}
    }

    // Sphere/torus centers well clear of partner centers and axes.
    let center_of = |g: &OperandGeom| -> Option<[f64; 3]> {
        match g {
            OperandGeom::Sphere { center, .. } | OperandGeom::Torus { center, .. } => Some(*center),
            _ => None,
        }
    };
    if let (Some(ca), Some(cb)) = (center_of(&ops.a), center_of(&ops.b)) {
        if len(sub(ca, cb)) < 0.55 {
            return Err("curved centers too close".to_string());
        }
    }
    if let (Some(ca), Some((pb, db))) = (center_of(&ops.a), ops.b.primary_axis()) {
        if perp_len(sub(ca, pb), db) < 0.25 {
            return Err("center A too close to axis B".to_string());
        }
    }
    if let (Some(cb), Some((pa, da))) = (center_of(&ops.b), ops.a.primary_axis()) {
        if perp_len(sub(cb, pa), da) < 0.25 {
            return Err("center B too close to axis A".to_string());
        }
    }

    // No parallel or coplanar planar faces across the two solids (every pair of
    // planar faces across A and B makes at least a ~9.9° dihedral angle; the two
    // target planes on `plane × plane` additionally satisfy `0.18 <= |na·nb| <= 0.82`).
    let planes_a = ops.a.planar_faces();
    let planes_b = ops.b.planar_faces();
    for (_, pna) in &planes_a {
        for (_, pnb) in &planes_b {
            if dot(*pna, *pnb).abs() > 0.985 {
                return Err("planar faces between A and B are nearly parallel".to_string());
            }
        }
    }

    // Planar faces of one solid are non-tangent to sphere / cylinder of the other.
    let check_planes_against_curved = |planes: &[([f64; 3], [f64; 3])],
                                       other: &OperandGeom,
                                       skip_first: bool|
     -> Result<(), String> {
        for (idx, (pt, n)) in planes.iter().enumerate() {
            if skip_first && idx == 0 {
                // Plane 0 of a Prism IS its crossing target face, which is
                // already checked for transverse crossing at `p_cross`.
                continue;
            }
            match other {
                OperandGeom::Sphere { center, radius, .. }
                    if (dot(sub(*center, *pt), *n).abs() - *radius).abs() < 0.14 =>
                {
                    return Err("auxiliary plane nearly tangent to sphere".to_string());
                }
                OperandGeom::Cylinder { axis_dir, .. } if dot(*n, *axis_dir).abs() < 0.08 => {
                    return Err("plane nearly parallel to cylinder axis".to_string());
                }
                _ => {}
            }
        }
        Ok(())
    };
    check_planes_against_curved(
        &planes_a,
        &ops.b,
        matches!(ops.a, OperandGeom::Prism { .. }),
    )?;
    check_planes_against_curved(
        &planes_b,
        &ops.a,
        matches!(ops.b, OperandGeom::Prism { .. }),
    )?;

    // Seam / pole vertices clear of partner surfaces.
    for v in ops.a.seam_vertices() {
        if ops.b.min_boundary_clearance(v) < 0.12 {
            return Err("seam/pole vertex of A too close to B boundary".to_string());
        }
    }
    for v in ops.b.seam_vertices() {
        if ops.a.min_boundary_clearance(v) < 0.12 {
            return Err("seam/pole vertex of B too close to A boundary".to_string());
        }
    }

    Ok(())
}

/// Draw deterministic general-position operands for `(pair, seed)` (pure
/// analytic geometry — never calls the kernel boolean).
pub fn draw_pair_operands(pair: SurfacePair, seed: u64) -> Option<PairOperands> {
    if !pair.is_buildable() {
        return None;
    }
    let mut rng = seeded_rng(pair.index, seed);
    let mut last_err = String::new();
    for _ in 0..512 {
        let crossing_point = [
            rng.range(-1.4, 1.4),
            rng.range(-1.4, 1.4),
            rng.range(-1.4, 1.4),
        ];
        let a = draw_single_operand(&mut rng, pair.a, crossing_point);
        let b = draw_single_operand(&mut rng, pair.b, crossing_point);
        let cand = PairOperands {
            pair,
            seed,
            crossing_point,
            a,
            b,
        };
        match verify_general_position(&cand) {
            Ok(()) => {
                // Confirm analytically (via closed-form ExactChain point membership
                // near `crossing_point`, still with zero kernel calls) that all
                // three boolean combinations `A ∩ B`, `A \ B`, and `B \ A` have
                // non-empty open neighborhoods around `crossing_point`.
                match check_local_boolean_witnesses(&cand) {
                    Ok(()) => return Some(cand),
                    Err(e) => last_err = e,
                }
            }
            Err(e) => last_err = e,
        }
    }
    panic!(
        "failed to draw general-position operands within 512 draws for {} seed {seed} (last rejection: {last_err})",
        pair.label()
    );
}

/// Closed-form check near `crossing_point` along the four quadrants spanned by
/// `(na, nb)` that `A ∩ B`, `A \ B`, `B \ A`, and `(R^3 \ (A ∪ B))` are all
/// inhabited right around the target surface crossing.
fn check_local_boolean_witnesses(ops: &PairOperands) -> Result<(), String> {
    let union_case =
        build_case_from_operands(ops, MatrixOp::Union).map_err(|e| format!("build_case: {e}"))?;
    let doc =
        serde_json::from_str::<Value>(&union_case.waffle_json).map_err(|e| format!("json: {e}"))?;
    let chain = ExactChain::from_waffle(&doc).map_err(|e| format!("ExactChain: {e:?}"))?;
    if chain.ops.len() != 2 {
        return Err(format!(
            "expected 2 ExactChain ops, got {}",
            chain.ops.len()
        ));
    }
    let p0 = ops.crossing_point;
    let na = ops.a.target_normal_at_cross();
    let nb = ops.b.target_normal_at_cross();
    // Dual basis `(ga, gb)` in `span(na, nb)` satisfying `ga·na = 1, ga·nb = 0`
    // and `gb·na = 0, gb·nb = 1`, so `p0 + sa*ga + sb*gb` has first-order
    // signed distances `(sa, sb)` to the two target surfaces.
    let c = dot(na, nb);
    let denom = 1.0 - c * c;
    if denom <= 0.1 {
        return Err(format!("degenerate normal span denom={denom:.4}"));
    }
    let ga = scale(sub(na, scale(nb, c)), 1.0 / denom);
    let gb = scale(sub(nb, scale(na, c)), 1.0 / denom);
    let eps = 0.01;
    let probe = |sa: f64, sb: f64| -> (bool, bool) {
        let q = add(p0, add(scale(ga, sa), scale(gb, sb)));
        (
            chain.ops[0].solid.contains(q),
            chain.ops[1].solid.contains(q),
        )
    };
    let q_ii = probe(-eps, -eps);
    let q_io = probe(-eps, eps);
    let q_oi = probe(eps, -eps);
    let q_oo = probe(eps, eps);
    if q_ii == (true, true)
        && q_io == (true, false)
        && q_oi == (false, true)
        && q_oo == (false, false)
    {
        Ok(())
    } else {
        Err(format!(
            "local quadrant witnesses mismatch: (-,-)={q_ii:?} (-,+)={q_io:?} (+,-)={q_oi:?} (+,+)={q_oo:?}"
        ))
    }
}

/// Build the `.waffle` JSON document and derived `AssayMeta` for `(pair, op,
/// seed)` through the real `wasm_bridge::dispatch` + `feature-engine` path
/// (`ModelBuilder::mock()`), canonicalizing generated UUIDs so the emitted
/// `.waffle` document is byte-for-byte deterministic.
pub fn build_pair_case(
    pair: SurfacePair,
    op: MatrixOp,
    seed: u64,
) -> Result<GeneratedPairCase, String> {
    let ops = draw_pair_operands(pair, seed)
        .ok_or_else(|| format!("{}: {NOT_BUILDABLE_BSPLINE}", pair.label()))?;
    build_case_from_operands(&ops, op)
}

fn build_case_from_operands(ops: &PairOperands, op: MatrixOp) -> Result<GeneratedPairCase, String> {
    let id = ops.pair.case_id(op, ops.seed);
    let mut builder = ModelBuilder::mock();
    ops.a.add_to_builder(&mut builder, "sk_a", "body_a")?;
    ops.b.add_to_builder(&mut builder, "sk_b", "body_b")?;
    let err = |e: crate::helpers::HarnessError| e.to_string();
    match op.boolean_op() {
        BooleanOp::Union => {
            builder
                .boolean_union("bool_ab", "body_a", "body_b")
                .map_err(err)?;
        }
        BooleanOp::Subtract => {
            builder
                .boolean_subtract("bool_ab", "body_a", "body_b")
                .map_err(err)?;
        }
        BooleanOp::Intersect => {
            builder
                .boolean_intersect("bool_ab", "body_a", "body_b")
                .map_err(err)?;
        }
    }
    let raw_json = builder.save().map_err(err)?;
    let mut doc: Value = serde_json::from_str(&raw_json).map_err(|e| e.to_string())?;
    let uuid_seed =
        0x5555_4944_504D_0000u64 ^ ((ops.pair.index as u64) << 40) ^ ((op as u64) << 32) ^ ops.seed;
    canonicalize_document(&mut doc, uuid_seed);
    let mut meta = derive_meta(&id, &doc)?;
    meta.description = format!(
        "Gate 3.1 surface-pair matrix: {} ({}) seed {} — {}",
        ops.pair.label(),
        op.slug(),
        ops.seed,
        ops.pair.feature_sources_summary()
    );
    let waffle_json = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    Ok(GeneratedPairCase {
        id,
        pair: ops.pair,
        op,
        seed: ops.seed,
        waffle_json,
        meta,
    })
}

fn canonicalize_document(val: &mut Value, seed: u64) {
    if let Some(doc_meta) = val.get_mut("document").and_then(Value::as_object_mut) {
        doc_meta.insert(
            "created".to_string(),
            Value::String("1970-01-01T00:00:00Z".to_string()),
        );
        doc_meta.insert(
            "modified".to_string(),
            Value::String("1970-01-01T00:00:00Z".to_string()),
        );
    }
    let mut rng = Rng::new(seed);
    let mut map: HashMap<String, String> = HashMap::new();
    let mut next_det_uuid = |orig: &str| -> String {
        if let Some(existing) = map.get(orig) {
            return existing.clone();
        }
        let hi = rng.next_u64() as u128;
        let lo = rng.next_u64() as u128;
        // RFC 4122 v4 layout bits for clean schema compatibility, deterministic
        // payload bits from `rng`.
        let raw = (hi << 64) | lo;
        let v4_shaped = (raw & !0x0000_0000_0000_F000_C000_0000_0000_0000u128)
            | 0x0000_0000_0000_4000_8000_0000_0000_0000u128;
        let det = Uuid::from_u128(v4_shaped).to_string();
        map.insert(orig.to_string(), det.clone());
        det
    };
    rewrite_value_uuids(val, &mut next_det_uuid);
}

fn rewrite_value_uuids(val: &mut Value, map_fn: &mut dyn FnMut(&str) -> String) {
    match val {
        Value::String(s) => {
            if Uuid::parse_str(s).is_ok() {
                *s = map_fn(s);
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_value_uuids(item, map_fn);
            }
        }
        Value::Object(obj) => {
            let keys: Vec<String> = obj.keys().cloned().collect();
            for k in keys {
                if let Some(mut v) = obj.remove(&k) {
                    rewrite_value_uuids(&mut v, map_fn);
                    let new_k = if Uuid::parse_str(&k).is_ok() {
                        map_fn(&k)
                    } else {
                        k
                    };
                    obj.insert(new_k, v);
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

// ── Matrix Aggregation, Table Formatting & JSON Report ──────────────────────

/// Structured record of `categorize`'s `ExactChain` 256-cell lattice volume
/// verdict (`categorize.rs:509-549`) for one evaluated case.
///
/// `prospect::derive_meta` sets `expected_volume: None`
/// (`prospect/mod.rs:251`), so this lattice check is the only volume oracle
/// active on pair-matrix documents; when it returns `NotCovered`, `categorize`
/// records no failure and the case can still read `SUPPORTED_CORRECT` without
/// volume confirmation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict")]
pub enum ExactVolumeRecord {
    Agree {
        rel: f64,
        band: f64,
    },
    NotCovered {
        reason: String,
    },
    Flag {
        rel: f64,
        band: f64,
        exact: f64,
        kernel: f64,
    },
}

impl ExactVolumeRecord {
    pub fn from_verdict(v: &ExactVolumeVerdict) -> Self {
        match v {
            ExactVolumeVerdict::Agree { rel, band } => Self::Agree {
                rel: *rel,
                band: *band,
            },
            ExactVolumeVerdict::NotCovered(reason) => Self::NotCovered {
                reason: reason.clone(),
            },
            ExactVolumeVerdict::Flag {
                rel,
                band,
                exact,
                kernel,
            } => Self::Flag {
                rel: *rel,
                band: *band,
                exact: *exact,
                kernel: *kernel,
            },
        }
    }

    pub const fn is_agree(&self) -> bool {
        matches!(self, Self::Agree { .. })
    }

    pub fn table_label(&self) -> String {
        match self {
            Self::Agree { rel, band } => format!("Agree(rel={rel:+.2e}, band={band:.2e})"),
            Self::NotCovered { reason } => format!("NotCovered({reason})"),
            Self::Flag {
                rel,
                band,
                exact,
                kernel,
            } => format!(
                "Flag(rel={rel:+.2e}, band={band:.2e}, kernel={kernel:.4e}, exact={exact:.4e})"
            ),
        }
    }
}

/// Per-operation outcome counts (`CORRECT(Agree/NotCovered) / WRONG / ERROR /
/// TIMEOUT / UNSUPPORTED`) plus `ExactChain` volume-verdict breakdown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpCounts {
    pub correct: usize,
    /// Subset of `correct` where `ExactVolumeRecord::Agree` confirmed volume.
    pub correct_volume_agree: usize,
    /// Subset of `correct` where exact-volume returned `NotCovered` (or `None`).
    pub correct_volume_not_covered: usize,
    pub wrong: usize,
    pub error: usize,
    pub timeout: usize,
    pub unsupported: usize,
    pub exact_volume_agree: usize,
    pub exact_volume_not_covered: usize,
    pub exact_volume_flag: usize,
}

impl OpCounts {
    pub fn record(&mut self, cat: &Category, exact_vol: Option<&ExactVolumeRecord>) {
        match cat {
            Category::SupportedCorrect => {
                self.correct += 1;
                if exact_vol.is_some_and(ExactVolumeRecord::is_agree) {
                    self.correct_volume_agree += 1;
                } else {
                    self.correct_volume_not_covered += 1;
                }
            }
            Category::SupportedWrong => self.wrong += 1,
            Category::Error | Category::ExpectedError => self.error += 1,
            Category::Timeout | Category::SkippedSlow => self.timeout += 1,
            Category::Unsupported(_) => self.unsupported += 1,
        }
        match exact_vol {
            Some(ExactVolumeRecord::Agree { .. }) => self.exact_volume_agree += 1,
            Some(ExactVolumeRecord::NotCovered { .. }) => self.exact_volume_not_covered += 1,
            Some(ExactVolumeRecord::Flag { .. }) => self.exact_volume_flag += 1,
            None => {}
        }
    }

    pub fn compact_cell(&self) -> String {
        format!(
            "{}C({}A/{}NC)/{}W/{}E/{}T/{}U",
            self.correct,
            self.correct_volume_agree,
            self.correct_volume_not_covered,
            self.wrong,
            self.error,
            self.timeout,
            self.unsupported
        )
    }
}

/// Per-case record in `target/pair-matrix/report.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairCaseRecord {
    pub id: String,
    pub pair: String,
    pub op: String,
    pub seed: u64,
    pub category: String,
    pub exact_volume: Option<ExactVolumeRecord>,
    pub detail: String,
}

/// One row of the 21-row Gate 3.1 surface-pair matrix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairMatrixRow {
    pub index: usize,
    pub pair: String,
    pub surface_a: String,
    pub surface_b: String,
    pub buildable: bool,
    pub not_buildable_reason: Option<String>,
    pub feature_a: Option<String>,
    pub feature_b: Option<String>,
    pub union: Option<OpCounts>,
    pub cut: Option<OpCounts>,
    pub intersect: Option<OpCounts>,
    /// Gate 3.1 column: whether at least one `(op, seed)` for this pair
    /// categorized `SUPPORTED_CORRECT`.
    pub has_correct_customer: bool,
    /// Stricter companion metric: whether all 3 ops (`union`, `cut`, AND
    /// `intersect`) each have at least one `SUPPORTED_CORRECT` seed.
    pub all_ops_have_correct_customer: bool,
    /// Whether at least one `(op, seed)` for this pair categorized
    /// `SUPPORTED_CORRECT` AND had `ExactVolumeRecord::Agree`.
    pub has_correct_with_volume_agreement: bool,
    /// Whether all 3 ops each have at least one `SUPPORTED_CORRECT` seed with
    /// `ExactVolumeRecord::Agree`.
    pub all_ops_have_correct_with_volume_agreement: bool,
}

/// Complete JSON report written to `target/pair-matrix/report.json` and read by
/// `scripts/kernel_1_0_score.sh`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairMatrixReport {
    pub gate: String,
    pub kernel: String,
    pub commit: String,
    pub seeds_per_op: usize,
    pub total_pairs: usize,
    pub buildable_pairs: usize,
    pub not_buildable_pairs: usize,
    pub pairs_with_correct_customer: usize,
    pub pairs_with_all_ops_correct: usize,
    pub pairs_with_correct_volume_agreement: usize,
    pub pairs_with_all_ops_correct_volume_agreement: usize,
    pub totals: OpCounts,
    pub total_cases_run: usize,
    pub run: Value,
    pub rows: Vec<PairMatrixRow>,
    pub cases: Vec<PairCaseRecord>,
}

/// Resolve the worktree's current `HEAD` short hash (`<short>` when tracked
/// files match `HEAD`, `<short>-dirty` when tracked files have uncommitted
/// edits, or `"unknown"` if `git` is unavailable).
pub fn resolve_git_commit() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let head = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(manifest_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(short) = head else {
        return "unknown".to_string();
    };
    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .current_dir(manifest_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|s| !s.trim().is_empty());
    if dirty {
        format!("{short}-dirty")
    } else {
        short
    }
}

/// Assemble the 21-row [`PairMatrixReport`] from `(CaseOutcome,
/// Option<ExactVolumeRecord>)` pairs.
pub fn build_report(
    seeds_per_op: usize,
    outcomes: &[(CaseOutcome, Option<ExactVolumeRecord>)],
    prov: &RunProvenance,
) -> PairMatrixReport {
    build_report_at_commit(resolve_git_commit(), seeds_per_op, outcomes, prov)
}

/// Assemble the 21-row [`PairMatrixReport`] stamped with an explicit `commit`
/// string.
pub fn build_report_at_commit(
    commit: String,
    seeds_per_op: usize,
    outcomes: &[(CaseOutcome, Option<ExactVolumeRecord>)],
    prov: &RunProvenance,
) -> PairMatrixReport {
    let mut by_pair_op: HashMap<(usize, MatrixOp), OpCounts> = HashMap::new();
    let mut totals = OpCounts::default();
    for (o, exact_vol) in outcomes {
        let Some((pair, op, _seed)) = parse_case_id(&o.id) else {
            continue;
        };
        by_pair_op
            .entry((pair.index, op))
            .or_default()
            .record(&o.category, exact_vol.as_ref());
        totals.record(&o.category, exact_vol.as_ref());
    }

    let mut rows = Vec::with_capacity(ALL_SURFACE_PAIRS.len());
    let mut buildable_pairs = 0usize;
    let mut not_buildable_pairs = 0usize;
    let mut pairs_with_correct_customer = 0usize;
    let mut pairs_with_all_ops_correct = 0usize;
    let mut pairs_with_correct_volume_agreement = 0usize;
    let mut pairs_with_all_ops_correct_volume_agreement = 0usize;

    for pair in ALL_SURFACE_PAIRS {
        if pair.is_buildable() {
            buildable_pairs += 1;
            let u = by_pair_op
                .remove(&(pair.index, MatrixOp::Union))
                .unwrap_or_default();
            let c = by_pair_op
                .remove(&(pair.index, MatrixOp::Cut))
                .unwrap_or_default();
            let i = by_pair_op
                .remove(&(pair.index, MatrixOp::Intersect))
                .unwrap_or_default();
            let has_correct = (u.correct + c.correct + i.correct) > 0;
            let all_ops_correct = u.correct > 0 && c.correct > 0 && i.correct > 0;
            let has_vol_agree =
                (u.correct_volume_agree + c.correct_volume_agree + i.correct_volume_agree) > 0;
            let all_ops_vol_agree = u.correct_volume_agree > 0
                && c.correct_volume_agree > 0
                && i.correct_volume_agree > 0;
            if has_correct {
                pairs_with_correct_customer += 1;
            }
            if all_ops_correct {
                pairs_with_all_ops_correct += 1;
            }
            if has_vol_agree {
                pairs_with_correct_volume_agreement += 1;
            }
            if all_ops_vol_agree {
                pairs_with_all_ops_correct_volume_agreement += 1;
            }
            rows.push(PairMatrixRow {
                index: pair.index,
                pair: pair.label(),
                surface_a: pair.a.label().to_string(),
                surface_b: pair.b.label().to_string(),
                buildable: true,
                not_buildable_reason: None,
                feature_a: pair.a.feature_recipe().map(str::to_string),
                feature_b: pair.b.feature_recipe().map(str::to_string),
                union: Some(u),
                cut: Some(c),
                intersect: Some(i),
                has_correct_customer: has_correct,
                all_ops_have_correct_customer: all_ops_correct,
                has_correct_with_volume_agreement: has_vol_agree,
                all_ops_have_correct_with_volume_agreement: all_ops_vol_agree,
            });
        } else {
            not_buildable_pairs += 1;
            rows.push(PairMatrixRow {
                index: pair.index,
                pair: pair.label(),
                surface_a: pair.a.label().to_string(),
                surface_b: pair.b.label().to_string(),
                buildable: false,
                not_buildable_reason: Some(NOT_BUILDABLE_BSPLINE.to_string()),
                feature_a: pair.a.feature_recipe().map(str::to_string),
                feature_b: pair.b.feature_recipe().map(str::to_string),
                union: None,
                cut: None,
                intersect: None,
                has_correct_customer: false,
                all_ops_have_correct_customer: false,
                has_correct_with_volume_agreement: false,
                all_ops_have_correct_with_volume_agreement: false,
            });
        }
    }

    let cases = outcomes
        .iter()
        .map(|(o, exact_vol)| {
            let (pair_label, op_slug, seed) = parse_case_id(&o.id)
                .map(|(p, op, s)| (p.label(), op.slug().to_string(), s))
                .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string(), 0));
            PairCaseRecord {
                id: o.id.clone(),
                pair: pair_label,
                op: op_slug,
                seed,
                category: o.category.label(),
                exact_volume: exact_vol.clone(),
                detail: o.detail.clone(),
            }
        })
        .collect();

    PairMatrixReport {
        gate: "Kernel 1.0 Gate 3.1 — surface-pair matrix".to_string(),
        kernel: "kernel-v2 (KernelV2Adapter)".to_string(),
        commit,
        seeds_per_op,
        total_pairs: ALL_SURFACE_PAIRS.len(),
        buildable_pairs,
        not_buildable_pairs,
        pairs_with_correct_customer,
        pairs_with_all_ops_correct,
        pairs_with_correct_volume_agreement,
        pairs_with_all_ops_correct_volume_agreement,
        totals,
        total_cases_run: outcomes.len(),
        run: prov.to_json(None),
        rows,
        cases,
    }
}

/// Format the 21-row human-readable ASCII / Markdown table (including the
/// `"CORRECT with volume agreement"` column and per-case `ExactChain`
/// exact-volume verdicts) printed by `tests/assay_pair_matrix.rs`.
pub fn format_report_table(report: &PairMatrixReport) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "\nGATE 3.1 SURFACE-PAIR MATRIX (commit {}, {} seed(s) per (pair, op)) — C(A/NC) = CORRECT(ExactChain Agree / NotCovered)",
        report.commit, report.seeds_per_op
    )
    .unwrap();
    writeln!(
        out,
        "{:<3} {:<22} {:<24} {:<24} {:<24} {:<22} CORRECT with volume agreement",
        "#",
        "pair",
        "union C(A/NC)/W/E/T/U",
        "cut C(A/NC)/W/E/T/U",
        "intersect C(A/NC)/W/E/T/U",
        "has a CORRECT customer"
    )
    .unwrap();
    writeln!(out, "{}", "-".repeat(156)).unwrap();
    for row in &report.rows {
        if row.buildable {
            let u = row.union.as_ref().unwrap();
            let c = row.cut.as_ref().unwrap();
            let i = row.intersect.as_ref().unwrap();
            let ops_ok = usize::from(u.correct > 0)
                + usize::from(c.correct > 0)
                + usize::from(i.correct > 0);
            let customer_str = if row.has_correct_customer {
                format!("YES ({ops_ok}/3 ops)")
            } else {
                "NO (0/3 ops)".to_string()
            };
            let agree_ops = usize::from(u.correct_volume_agree > 0)
                + usize::from(c.correct_volume_agree > 0)
                + usize::from(i.correct_volume_agree > 0);
            let agree_seeds =
                u.correct_volume_agree + c.correct_volume_agree + i.correct_volume_agree;
            let total_c = u.correct + c.correct + i.correct;
            let vol_agree_str = if row.has_correct_with_volume_agreement {
                format!("YES ({agree_ops}/3 ops, {agree_seeds}/{total_c} C)")
            } else if total_c > 0 {
                format!("NO (0/3 ops, 0/{total_c} C — NotCovered)")
            } else {
                "NO (0/3 ops)".to_string()
            };
            writeln!(
                out,
                "{:02}  {:<22} {:<24} {:<24} {:<24} {:<22} {}",
                row.index,
                row.pair,
                u.compact_cell(),
                c.compact_cell(),
                i.compact_cell(),
                customer_str,
                vol_agree_str
            )
            .unwrap();
        } else {
            let reason = row
                .not_buildable_reason
                .as_deref()
                .unwrap_or(NOT_BUILDABLE_BSPLINE);
            let no_str = format!("NO ({reason})");
            writeln!(
                out,
                "{:02}  {:<22} {:<74} {:<22} {}",
                row.index, row.pair, reason, "NO", no_str
            )
            .unwrap();
        }
    }
    writeln!(out, "{}", "-".repeat(156)).unwrap();
    writeln!(
        out,
        "Score: {}/{} pairs have a CORRECT customer ({}/{} all 3 ops); {}/{} CORRECT with volume agreement ({}/{} all 3 ops); {} buildable, {} not buildable",
        report.pairs_with_correct_customer,
        report.total_pairs,
        report.pairs_with_all_ops_correct,
        report.total_pairs,
        report.pairs_with_correct_volume_agreement,
        report.total_pairs,
        report.pairs_with_all_ops_correct_volume_agreement,
        report.total_pairs,
        report.buildable_pairs,
        report.not_buildable_pairs,
    )
    .unwrap();
    writeln!(
        out,
        "Totals across {} buildable runs: {} CORRECT ({} Agree, {} NotCovered) / {} WRONG / {} ERROR / {} TIMEOUT / {} UNSUPPORTED",
        report.total_cases_run,
        report.totals.correct,
        report.totals.correct_volume_agree,
        report.totals.correct_volume_not_covered,
        report.totals.wrong,
        report.totals.error,
        report.totals.timeout,
        report.totals.unsupported,
    )
    .unwrap();
    let evaluated_vol = report.totals.exact_volume_agree
        + report.totals.exact_volume_not_covered
        + report.totals.exact_volume_flag;
    let not_reached = report.total_cases_run.saturating_sub(evaluated_vol);
    writeln!(
        out,
        "Exact-volume verdicts (ExactChain 256-cell lattice): {} Agree / {} NotCovered / {} Flag / {} not reached",
        report.totals.exact_volume_agree,
        report.totals.exact_volume_not_covered,
        report.totals.exact_volume_flag,
        not_reached,
    )
    .unwrap();
    if !report.cases.is_empty() {
        writeln!(out, "\nPer-case exact-volume verdicts:").unwrap();
        for c in &report.cases {
            let ev = c.exact_volume.as_ref().map_or_else(
                || "— (not reached)".to_string(),
                ExactVolumeRecord::table_label,
            );
            writeln!(out, "  {:<36} {:<18} {}", c.id, c.category, ev).unwrap();
        }
    }
    out
}

// ── Small 3D Vector Helpers ─────────────────────────────────────────────────

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

fn norm(v: [f64; 3]) -> [f64; 3] {
    let l = len(v);
    [v[0] / l, v[1] / l, v[2] / l]
}

fn perp_len(v: [f64; 3], unit_axis: [f64; 3]) -> f64 {
    len(sub(v, scale(unit_axis, dot(v, unit_axis))))
}
