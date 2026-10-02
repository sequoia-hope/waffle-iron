//! Exact analytic extraction: truck's post-parse B-Rep → the neutral
//! [`AnalyticShellData`] contract, with **no tessellation anywhere in the
//! path** (STEP import SI5,
//! `specs/step_import_si5_exact_analytic_ingestion.md`).
//!
//! This is the sibling of [`crate::convert`]. Both read the same
//! `CompressedShell<Point3, Curve3D, Surface>`; `convert` throws the analytic
//! geometry away at `robust_triangulation` and keeps a mesh, this module keeps
//! the geometry and throws nothing away.
//!
//! **The gate is on truck's `Surface`/`Curve3D` variants, not on
//! [`waffle_types::kernel::ImportedSurface`]** — that classification is
//! parameterless, and it mislabels our own `tests/fixtures/cylinder.step`
//! (truck's writer emits a swept surface there, so a "cylinder" fixture
//! classifies as freeform). Gating on the parsed variant is the only honest
//! test of whether a face has an exact representation.
//!
//! Extraction is all-or-nothing **per shell**: one face or edge outside the
//! vocabulary makes that shell ineligible, loudly and by name, and the caller
//! serves it from the mesh tier instead. A file may well have some shells
//! eligible and some not, so the per-shell verdicts are reported individually
//! rather than collapsed.

use truck_meshalgo::prelude::*;
use truck_stepio::r#in::{step_geometry::*, Table};
use waffle_types::kernel::{
    AnalyticCurve, AnalyticEdge, AnalyticFace, AnalyticLoop, AnalyticShellData, AnalyticSurface,
    ImportedShellData, OrientedEdge,
};

use crate::convert::CShell;
use crate::StepImportError;

use std::result::Result;

/// Why a shell cannot be represented exactly. Every variant names the entity
/// and the index that failed, because "this file is not analytic" is not an
/// actionable diagnostic and because these are the rows that become roadmap
/// items.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Ineligible {
    /// A surface with no exact representation in the kernel's vocabulary.
    #[error("face {face}: {entity} has no exact representation")]
    Surface { face: usize, entity: &'static str },
    /// A curve with no exact representation in the kernel's vocabulary.
    #[error("edge {edge}: {entity} has no exact representation")]
    Curve { edge: usize, entity: &'static str },
    /// In vocabulary, but with parameters the kernel's invariants exclude —
    /// e.g. a spindle torus (`minor >= major`), or a degenerate radius.
    #[error("face {face}: {entity} is out of range — {reason}")]
    OutOfRange {
        face: usize,
        entity: &'static str,
        reason: &'static str,
    },
    /// Topology we cannot faithfully carry across.
    #[error("face {face}: {reason}")]
    Topology { face: usize, reason: &'static str },
    /// An index in the file's own tables points nowhere.
    #[error("{table} index {index} is out of range")]
    BadIndex { table: &'static str, index: usize },
    /// The source declares topology the reader cannot represent AND discards
    /// without saying so, which makes the whole file's topology untrustworthy.
    #[error(
        "source declares {entity}, which the reader silently drops — \
             the topology cannot be trusted"
    )]
    SilentlyDroppedTopology { entity: &'static str },
    #[error("shell has no faces")]
    Empty,
    /// The shell is one boundary of a `BREP_WITH_VOIDS` — an outer shell with
    /// `shells_in_solid - 1` voids, or one of those voids. The exact tier
    /// builds one solid per shell, so the outer shell alone would be a
    /// silently filled block and a void alone an inside-out body; the whole
    /// solid stays on the mesh tier until the exact tier carries voids (SI5
    /// spec §5.2).
    #[error(
        "shell is one of {shells_in_solid} boundaries of a solid with voids — \
         exact ingestion of voids is SI5 spec §5.2"
    )]
    Voids { shells_in_solid: usize },
}

/// Every shell of a STEP file, each either extracted exactly or rejected by
/// name. Shell order matches [`crate::parse_step`]'s `shells`, so the two
/// tiers can be compared or mixed per shell.
#[derive(Debug, Clone)]
pub struct AnalyticImport {
    pub source_name: String,
    pub shells: Vec<Result<AnalyticShellData, Ineligible>>,
    pub warnings: Vec<String>,
}

impl AnalyticImport {
    /// Shells that extracted exactly.
    pub fn eligible(&self) -> impl Iterator<Item = &AnalyticShellData> {
        self.shells.iter().filter_map(|s| s.as_ref().ok())
    }

    pub fn eligible_count(&self) -> usize {
        self.shells.iter().filter(|s| s.is_ok()).count()
    }

    /// True when every shell extracted — i.e. the whole file can be served
    /// from the exact tier.
    pub fn fully_eligible(&self) -> bool {
        !self.shells.is_empty() && self.shells.iter().all(|s| s.is_ok())
    }

    /// One line per rejected shell, for a feature warning. Never silent: a
    /// fallback to the mesh tier must be visible to the user.
    pub fn rejections(&self) -> Vec<String> {
        self.shells
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().err().map(|e| format!("shell {i}: {e}")))
            .collect()
    }
}

/// Parse STEP text and extract every shell analytically.
///
/// Shares `convert`'s parse and assembly walk, so placements are baked in and
/// the shell order is identical to the mesh path's.
pub fn parse_step_analytic(
    step_text: &str,
    source_name: &str,
) -> Result<AnalyticImport, StepImportError> {
    let table = Table::from_step(step_text).ok_or(StepImportError::Parse)?;
    let mut warnings = Vec::new();
    let (unit_scale, unit_warning) = crate::units::scan_length_unit_scale(step_text);
    warnings.extend(unit_warning);

    let shells = crate::convert::collect_placed_shells(&table, &mut warnings)?;
    if shells.is_empty() {
        return Err(StepImportError::NoSolids);
    }

    // A file-level refusal, because the loss is file-level and invisible.
    //
    // truck's reader has no `vertex_loop` table at all — `FaceBound.bound` is
    // typed `EdgeLoop` with the upstream comment "For now, we are going with
    // the policy of accepting nothing but edgeloop", and a bound that fails to
    // resolve is `filter_map`'d away (`truck-stepio/src/in/convert.rs:60,106`).
    // So a face whose ring is a `VERTEX_LOOP` comes back MISSING that ring, with
    // nothing in the compressed shell to say a boundary was dropped.
    //
    // For the mesh tier that is a cosmetic defect. For an exact ingest it is a
    // silent wrong answer — we would build a solid that disagrees with the file
    // about its own boundary. Refuse the file instead and let the mesh tier
    // serve it (P9/P10: turn a silent wrong into a loud stop). Costs the 5.1 %
    // of the corpus that has a degenerate loop; see spec §5.3 for the ways out.
    if step_text.contains("VERTEX_LOOP") {
        return Ok(AnalyticImport {
            source_name: source_name.to_string(),
            shells: shells
                .iter()
                .map(|_| {
                    Err(Ineligible::SilentlyDroppedTopology {
                        entity: "a VERTEX_LOOP",
                    })
                })
                .collect(),
            warnings,
        });
    }

    Ok(AnalyticImport {
        source_name: source_name.to_string(),
        shells: shells
            .iter()
            .map(|s| extract_placed(s, unit_scale))
            .collect(),
        warnings,
    })
}

/// One shell of a tiered import: served exactly, or from the mesh tier with
/// the reason named.
#[derive(Debug, Clone)]
pub enum TieredShell {
    Exact(AnalyticShellData),
    Mesh {
        why: Ineligible,
        data: ImportedShellData,
    },
}

/// Every shell of a STEP file, each served by the tier that can carry it —
/// the SI5 C6 contract. Shells are in the canonical order
/// (`convert::collect_placed_shells`), the same order [`crate::parse_step`]
/// uses, so `shells[i]` here and `parse_step(..).shells[i]` are the same
/// shell of the file.
///
/// Only the shells the exact tier refuses are tessellated: the tessellator is
/// the thing SI5 exists to stop running (spec §1).
#[derive(Debug, Clone)]
pub struct TieredImport {
    pub source_name: String,
    pub shells: Vec<TieredShell>,
    pub warnings: Vec<String>,
}

impl TieredImport {
    pub fn exact_count(&self) -> usize {
        self.shells
            .iter()
            .filter(|s| matches!(s, TieredShell::Exact(_)))
            .count()
    }

    pub fn mesh_count(&self) -> usize {
        self.shells.len() - self.exact_count()
    }

    /// One line per mesh-tier shell naming why, for a feature warning. A
    /// fallback must be visible to the user, never silent.
    pub fn rejections(&self) -> Vec<String> {
        self.shells
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s {
                TieredShell::Mesh { why, .. } => Some(format!("shell {i}: {why}")),
                TieredShell::Exact(_) => None,
            })
            .collect()
    }
}

/// Parse STEP text once and serve every shell from the exact tier when it
/// can be, from the mesh tier (tessellated, named reason) when it cannot.
pub fn parse_step_tiered(
    step_text: &str,
    source_name: &str,
) -> Result<TieredImport, StepImportError> {
    let table = Table::from_step(step_text).ok_or(StepImportError::Parse)?;
    let mut warnings = Vec::new();
    let (unit_scale, unit_warning) = crate::units::scan_length_unit_scale(step_text);
    warnings.extend(unit_warning);

    let shells = crate::convert::collect_placed_shells(&table, &mut warnings)?;
    if shells.is_empty() {
        return Err(StepImportError::NoSolids);
    }
    // The same file-level refusal as `parse_step_analytic`, for the same
    // reason (a dropped `VERTEX_LOOP` is a silent topology loss).
    let dropped_loop = step_text.contains("VERTEX_LOOP");

    let mut out = Vec::with_capacity(shells.len());
    for placed in &shells {
        let exact = if dropped_loop {
            Err(Ineligible::SilentlyDroppedTopology {
                entity: "a VERTEX_LOOP",
            })
        } else {
            extract_placed(placed, unit_scale)
        };
        out.push(match exact {
            Ok(shell) => TieredShell::Exact(shell),
            Err(why) => TieredShell::Mesh {
                why,
                data: crate::convert::convert_shell(&placed.shell, unit_scale, &mut warnings),
            },
        });
    }
    Ok(TieredImport {
        source_name: source_name.to_string(),
        shells: out,
        warnings,
    })
}

/// Extract one placed shell exactly, refusing a boundary of a solid with
/// voids before looking at its geometry.
fn extract_placed(
    placed: &crate::convert::PlacedShell,
    unit_scale: f64,
) -> Result<AnalyticShellData, Ineligible> {
    if placed.shells_in_solid > 1 {
        return Err(Ineligible::Voids {
            shells_in_solid: placed.shells_in_solid,
        });
    }
    extract_analytic(&placed.shell, unit_scale)
}

/// Extract one shell exactly, or say precisely why not.
///
/// Every geometric check happens before anything is built, so a rejection
/// costs nothing and the returned shell is complete when it is returned at
/// all.
pub(crate) fn extract_analytic(
    shell: &CShell,
    unit_scale: f64,
) -> Result<AnalyticShellData, Ineligible> {
    if shell.faces.is_empty() {
        return Err(Ineligible::Empty);
    }

    let vertices: Vec<cad_primitives::Point3> = shell
        .vertices
        .iter()
        .map(|p| scale_point(*p, unit_scale))
        .collect();

    let mut edges = Vec::with_capacity(shell.edges.len());
    for (i, edge) in shell.edges.iter().enumerate() {
        let (start, end) = edge.vertices;
        for v in [start, end] {
            if v >= vertices.len() {
                return Err(Ineligible::BadIndex {
                    table: "vertex",
                    index: v,
                });
            }
        }
        let curve =
            curve_params(&edge.curve).map_err(|entity| Ineligible::Curve { edge: i, entity })?;
        edges.push(AnalyticEdge {
            start: start as u32,
            end: end as u32,
            curve: scale_curve(curve, unit_scale),
        });
    }

    let mut faces = Vec::with_capacity(shell.faces.len());
    for (i, face) in shell.faces.iter().enumerate() {
        // Extract in file units, settle the orientation against the file's own
        // geometry, and only then convert to metres.
        let raw = surface_params(&face.surface).map_err(|e| e.at_face(i))?;
        let same_sense = same_sense_of(&face.surface, face.orientation, &raw);
        let surface = scale_surface(raw, unit_scale);

        if face.boundaries.is_empty() {
            // A face with no bounds at all should be unreachable, because the
            // one cause we have VERIFIED for it — truck dropping a `VERTEX_LOOP`
            // bound — is refused file-wide in `parse_step_analytic`.
            //
            // (This branch used to claim such a face was a legitimately
            // seamless sphere or torus. That was a misdiagnosis: the ABC models
            // showing it do declare a bound, `FACE_BOUND -> VERTEX_LOOP`, which
            // the reader discards. Verified on
            // 00000052_666139e3bff64d4e8a6ce183_step_001: `#264 = FACE_BOUND('',
            // #473, .T.)` with `#473 = VERTEX_LOOP('', #613)`.)
            //
            // So if we get here, the reader is in a state we do not understand.
            // Refuse rather than assemble a solid from a boundary we cannot
            // account for.
            return Err(Ineligible::Topology {
                face: i,
                reason: "no boundary loop, and no dropped VERTEX_LOOP to explain it",
            });
        }
        let mut loops = Vec::with_capacity(face.boundaries.len());
        for boundary in &face.boundaries {
            if boundary.is_empty() {
                // Defence in depth. A dropped `VERTEX_LOOP` does NOT arrive as
                // an empty boundary — truck omits the bound entirely, which is
                // why `parse_step_analytic` refuses such files up front. An
                // empty boundary would be a reader invariant we do not
                // understand, so refuse rather than guess.
                return Err(Ineligible::Topology {
                    face: i,
                    reason: "a boundary loop with no edges",
                });
            }
            let mut oriented = Vec::with_capacity(boundary.len());
            for ei in boundary {
                if ei.index >= edges.len() {
                    return Err(Ineligible::BadIndex {
                        table: "edge",
                        index: ei.index,
                    });
                }
                oriented.push(OrientedEdge {
                    edge: ei.index as u32,
                    forward: ei.orientation,
                });
            }
            loops.push(AnalyticLoop::Edges(oriented));
        }

        faces.push(AnalyticFace {
            surface,
            loops,
            same_sense,
        });
    }

    Ok(AnalyticShellData {
        vertices,
        edges,
        faces,
    })
}

/// A surface rejection that does not yet know its face index.
enum SurfaceReject {
    Unsupported(&'static str),
    OutOfRange(&'static str, &'static str),
}

impl SurfaceReject {
    fn at_face(self, face: usize) -> Ineligible {
        match self {
            SurfaceReject::Unsupported(entity) => Ineligible::Surface { face, entity },
            SurfaceReject::OutOfRange(entity, reason) => Ineligible::OutOfRange {
                face,
                entity,
                reason,
            },
        }
    }
}

fn scale_point(p: Point3, s: f64) -> cad_primitives::Point3 {
    cad_primitives::Point3::new(p.x * s, p.y * s, p.z * s)
}

fn vec3(v: Vector3) -> cad_primitives::Vector3 {
    cad_primitives::Vector3::new(v.x, v.y, v.z)
}

/// Relative agreement of two lengths. Used to reject a non-uniform scale,
/// which would turn a circle into an ellipse or a sphere into an ellipsoid —
/// neither of which the exact vocabulary can hold.
///
/// Truck's own inversion code uses `Tolerance::near` (an ABSOLUTE 1e-6), which
/// is both too loose for metre-scale geometry and too tight for large radii.
/// A relative band is scale-free, in the spirit of `TAU_EVAL`.
fn lengths_agree(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// `true` when the solid's outward normal agrees with the surface's natural
/// sense (see [`AnalyticFace::same_sense`]).
///
/// Determined by MEASUREMENT rather than by replaying truck's orientation
/// bookkeeping: the surface may have been `invert()`ed during parsing, the
/// `Processor` carries its own orientation flag, and `CompressedFace` carries
/// a second one that an `ORIENTED_CLOSED_SHELL` flips without touching the
/// surface. Evaluating the normal and comparing it against the natural outward
/// direction at the same point is independent of all three.
fn same_sense_of(surface: &Surface, face_orientation: bool, extracted: &AnalyticSurface) -> bool {
    let p = surface.subs(0.0, 0.0);
    let n = surface.normal(0.0, 0.0);
    // The shell's true outward normal at `p`.
    let outward = if face_orientation { n } else { -n };
    let natural = natural_outward(extracted, p);
    outward.dot(natural) >= 0.0
}

/// The surface's natural outward direction at `p`, both in FILE units — the
/// sense in which [`AnalyticSurface`]'s parameters are stated: away from the
/// plane along `normal`, away from the axis, away from the centre, away from
/// the tube circle. Need not be normalized; only its sign against the true
/// outward normal is consumed.
///
/// `s` must be the file-unit extraction, not the scaled one: the torus arm
/// compares a radius against a distance, so the two must be in one space.
fn natural_outward(s: &AnalyticSurface, p: Point3) -> Vector3 {
    let pt = |q: &cad_primitives::Point3| Point3::new(q.x(), q.y(), q.z());
    let dir = |d: &cad_primitives::Vector3| Vector3::new(d.x(), d.y(), d.z());
    match s {
        AnalyticSurface::Plane { normal, .. } => dir(normal),
        AnalyticSurface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } => {
            let a = dir(axis_dir);
            let d = p - pt(axis_point);
            d - a * d.dot(a)
        }
        AnalyticSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            // Perpendicular to the ruling: the radial direction tilted off by
            // the half-angle, away from the axis.
            let a = dir(axis_dir);
            let d = p - pt(apex);
            let radial = d - a * d.dot(a);
            let m = radial.magnitude();
            if m <= 0.0 {
                return a;
            }
            radial / m * half_angle.cos() - a * half_angle.sin()
        }
        AnalyticSurface::Sphere { center, .. } => p - pt(center),
        AnalyticSurface::Torus {
            center,
            axis_dir,
            major_radius,
            ..
        } => {
            let a = dir(axis_dir);
            let d = p - pt(center);
            let axial = d.dot(a);
            let radial = d - a * axial;
            let m = radial.magnitude();
            if m <= 0.0 {
                return a;
            }
            // From the nearest point of the tube's centre circle to `p`.
            radial / m * (m - *major_radius) + a * axial
        }
    }
}

/// File units → metres. Points and radii scale; directions and angles do not.
fn scale_surface(s: AnalyticSurface, k: f64) -> AnalyticSurface {
    let sp =
        |p: cad_primitives::Point3| cad_primitives::Point3::new(p.x() * k, p.y() * k, p.z() * k);
    match s {
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
            radius: radius * k,
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
            radius: radius * k,
        },
        AnalyticSurface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => AnalyticSurface::Torus {
            center: sp(center),
            axis_dir,
            major_radius: major_radius * k,
            minor_radius: minor_radius * k,
        },
    }
}

/// File units → metres for a curve.
fn scale_curve(c: AnalyticCurve, k: f64) -> AnalyticCurve {
    let sp =
        |p: cad_primitives::Point3| cad_primitives::Point3::new(p.x() * k, p.y() * k, p.z() * k);
    match c {
        AnalyticCurve::Line => AnalyticCurve::Line,
        AnalyticCurve::Circle {
            center,
            normal,
            radius,
            interior,
        } => AnalyticCurve::Circle {
            center: sp(center),
            normal,
            radius: radius * k,
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
            major_radius: major_radius * k,
            minor_radius: minor_radius * k,
            interior: sp(interior),
        },
    }
}

/// Extract a surface's exact parameters, in FILE units.
///
/// Every value is read from the file's own definition through truck's public
/// accessors — no trigonometry, no sampling, no fitting. The only arithmetic is
/// applying the placement matrix and projecting out an axial component.
fn surface_params(surface: &Surface) -> Result<AnalyticSurface, SurfaceReject> {
    let es = match surface {
        Surface::ElementarySurface(es) => es,
        Surface::SweptCurve(_) => return Err(SurfaceReject::Unsupported("a swept surface")),
        Surface::BSplineSurface(_) => return Err(SurfaceReject::Unsupported("a b-spline surface")),
        Surface::NurbsSurface(_) => return Err(SurfaceReject::Unsupported("a NURBS surface")),
    };

    match es {
        ElementarySurface::Plane(p) => Ok(AnalyticSurface::Plane {
            origin: point(p.origin()),
            normal: vec3(p.normal()),
        }),

        // The cylinder's entity is already in world coordinates and its
        // transform is the identity for a freshly-parsed surface; apply it
        // anyway, because an assembly placement composes into it.
        ElementarySurface::CylindricalSurface(proc) => {
            let m = *proc.transform();
            let revo = proc.entity();
            let axis_point = m.transform_point(revo.origin());
            let axis_dir = m.transform_vector(revo.axis());
            let axis_dir = unit(axis_dir).ok_or(SurfaceReject::OutOfRange(
                "a cylinder",
                "axis is degenerate",
            ))?;
            // Perpendicular distance from the axis to the generating line.
            let d = m.transform_point(revo.entity_curve().0) - axis_point;
            let radius = (d - axis_dir * d.dot(axis_dir)).magnitude();
            if !(radius.is_finite() && radius > 0.0) {
                return Err(SurfaceReject::OutOfRange(
                    "a cylinder",
                    "radius is not positive",
                ));
            }
            Ok(AnalyticSurface::Cylinder {
                axis_point: point(axis_point),
                axis_dir: vec3(axis_dir),
                radius,
            })
        }

        // The cone's entity is in LOCAL coordinates with transform == the
        // placement matrix (unlike the cylinder). Its generating line starts on
        // the reference circle and climbs the axis while its radial distance
        // grows at `tan(half_angle)`, so the apex is where that distance
        // reaches zero.
        ElementarySurface::ConicalSurface(proc) => {
            let m = *proc.transform();
            let revo = proc.entity();
            let a_local = revo.axis();
            let line = *revo.entity_curve();
            let r_vec = line.0 - revo.origin();
            let r_perp = r_vec - a_local * r_vec.dot(a_local);
            let r_local = r_perp.magnitude();
            if !(r_local.is_finite() && r_local > 0.0) {
                return Err(SurfaceReject::OutOfRange(
                    "a cone",
                    "reference radius is not positive",
                ));
            }
            let v = line.1 - line.0;
            let dr = v.dot(r_perp / r_local);
            let dz = v.dot(a_local);
            if dr == 0.0 {
                return Err(SurfaceReject::OutOfRange(
                    "a cone",
                    "zero half-angle — a cylinder written as a cone",
                ));
            }
            let half_angle = dr.atan2(dz).abs();
            if !(half_angle > 0.0 && half_angle < std::f64::consts::FRAC_PI_2) {
                return Err(SurfaceReject::OutOfRange(
                    "a cone",
                    "half-angle outside (0, pi/2)",
                ));
            }
            // Walk the ruling back to zero radius.
            let apex_local = line.0 + v * (-r_local / dr);
            let axis_dir = unit(m.transform_vector(a_local))
                .ok_or(SurfaceReject::OutOfRange("a cone", "axis is degenerate"))?;
            // `axis_dir` must point from the apex INTO the nappe, i.e. the
            // direction in which the radius grows.
            let axis_dir = if dz * dr > 0.0 { axis_dir } else { -axis_dir };
            Ok(AnalyticSurface::Cone {
                apex: point(m.transform_point(apex_local)),
                axis_dir: vec3(axis_dir),
                half_angle,
            })
        }

        ElementarySurface::Sphere(proc) => {
            let m = *proc.transform();
            // `step_geometry::Sphere` is a newtype over truck's; `.0` is pub.
            let s = proc.entity().0;
            let (r0, r1, r2) = column_scales(&m);
            if !lengths_agree(r0, r1) || !lengths_agree(r1, r2) {
                return Err(SurfaceReject::OutOfRange(
                    "a sphere",
                    "non-uniform scale — an ellipsoid",
                ));
            }
            let radius = r0 * s.radius();
            if !(radius.is_finite() && radius > 0.0) {
                return Err(SurfaceReject::OutOfRange(
                    "a sphere",
                    "radius is not positive",
                ));
            }
            Ok(AnalyticSurface::Sphere {
                center: point(m.transform_point(s.center())),
                radius,
            })
        }

        ElementarySurface::ToroidalSurface(proc) => {
            let m = *proc.transform();
            let t = *proc.entity();
            let (r0, r1, r2) = column_scales(&m);
            if !lengths_agree(r0, r1) || !lengths_agree(r1, r2) {
                return Err(SurfaceReject::OutOfRange("a torus", "non-uniform scale"));
            }
            let major_radius = r0 * t.large_radius();
            let minor_radius = r0 * t.small_radius();
            if !(minor_radius > 0.0 && major_radius > minor_radius) {
                // The kernel holds RING tori only; a spindle or horn torus is
                // self-intersecting and its invariants exclude it.
                return Err(SurfaceReject::OutOfRange(
                    "a torus",
                    "not a ring torus (minor >= major)",
                ));
            }
            // `Torus` has no axis accessor: its symmetry axis is local +Z, so
            // the world axis is the transform's third column.
            let axis_dir = unit(m[2].truncate())
                .ok_or(SurfaceReject::OutOfRange("a torus", "axis is degenerate"))?;
            Ok(AnalyticSurface::Torus {
                center: point(m.transform_point(t.center())),
                axis_dir: vec3(axis_dir),
                major_radius,
                minor_radius,
            })
        }
    }
}

/// Extract a curve's exact parameters, in FILE units.
fn curve_params(curve: &Curve3D) -> Result<AnalyticCurve, &'static str> {
    let conic = match curve {
        Curve3D::Line(_) => return Ok(AnalyticCurve::Line),
        Curve3D::Conic(c) => c,
        Curve3D::Polyline(_) => return Err("a polyline"),
        Curve3D::BSplineCurve(_) => return Err("a b-spline curve"),
        Curve3D::NurbsCurve(_) => return Err("a NURBS curve"),
        Curve3D::PCurve(_) => return Err("a p-curve"),
    };

    let proc = match conic {
        Conic3D::Ellipse(p) => p,
        // truck parses a STEP PARABOLA into UnitHyperbola and reports it as
        // Conic3D::Hyperbola, so this arm covers both. Neither has an exact
        // representation (there is no parabola in the vocabulary) and the
        // corpus contains zero of either, so one rejection serves both.
        Conic3D::Hyperbola(_) => return Err("a hyperbola or parabola"),
        Conic3D::Parabola(_) => return Err("a parabola"),
    };

    let m = conic.posture();
    let center = m[3].to_point();
    let u = m[0].truncate();
    let v = m[1].truncate();
    let r0 = u.magnitude();
    let r1 = v.magnitude();
    let normal = unit(m[2].truncate()).ok_or("a conic with a degenerate axis")?;
    if !(r0.is_finite() && r0 > 0.0 && r1.is_finite() && r1 > 0.0) {
        return Err("a conic with a degenerate radius");
    }

    // A point from the middle of the file's own parameter range: the only
    // thing that distinguishes this arc from its complement.
    let (t0, t1) = BoundedCurve::range_tuple(proc);
    let interior = point(proc.subs(0.5 * (t0 + t1)));

    // A circle and an ellipse are the same truck type; the axis scales say
    // which. Uniform ⇒ circle.
    if lengths_agree(r0, r1) {
        return Ok(AnalyticCurve::Circle {
            center: point(center),
            normal: vec3(normal),
            radius: r0,
            interior,
        });
    }
    let (major_radius, minor_radius, major_axis) = if r0 >= r1 {
        (r0, r1, u / r0)
    } else {
        (r1, r0, v / r1)
    };
    Ok(AnalyticCurve::Ellipse {
        center: point(center),
        normal: vec3(normal),
        major_axis: vec3(major_axis),
        major_radius,
        minor_radius,
        interior,
    })
}

fn point(p: Point3) -> cad_primitives::Point3 {
    cad_primitives::Point3::new(p.x, p.y, p.z)
}

fn unit(v: Vector3) -> Option<Vector3> {
    let m = v.magnitude();
    if !m.is_finite() || m <= 0.0 {
        return None;
    }
    Some(v / m)
}

fn column_scales(m: &Matrix4) -> (f64, f64, f64) {
    (
        m[0].truncate().magnitude(),
        m[1].truncate().magnitude(),
        m[2].truncate().magnitude(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_primitives::Point3 as P3;

    fn analytic(fixture: &str) -> AnalyticImport {
        let path = format!("{}/tests/fixtures/{fixture}", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path}: {e} — regenerate with UPDATE_SI5_FIXTURES=1"));
        parse_step_analytic(&text, fixture).expect("parses")
    }

    fn only_shell(fixture: &str) -> AnalyticShellData {
        let import = analytic(fixture);
        assert_eq!(import.shells.len(), 1, "{fixture}: one shell");
        import
            .shells
            .into_iter()
            .next()
            .unwrap()
            .unwrap_or_else(|e| {
                panic!("{fixture} must be eligible, got: {e}");
            })
    }

    /// Signed distance from `p` to the surface — zero on the surface.
    ///
    /// This is the extraction's own oracle: it re-derives the surface from the
    /// parameters we extracted and asks whether the file's own vertices lie on
    /// it. A transcription error in any parameter shows up here, which is why
    /// every fixture runs it rather than asserting hand-copied numbers.
    fn residual(s: &AnalyticSurface, p: P3) -> f64 {
        let v = |a: P3, b: P3| [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()];
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let norm = |a: [f64; 3]| dot(a, a).sqrt();
        let axial = |d: [f64; 3], a: [f64; 3]| {
            let t = dot(d, a);
            (t, [d[0] - a[0] * t, d[1] - a[1] * t, d[2] - a[2] * t])
        };
        match s {
            AnalyticSurface::Plane { origin, normal } => {
                dot(v(p, *origin), [normal.x(), normal.y(), normal.z()])
            }
            AnalyticSurface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                let (_, r) = axial(
                    v(p, *axis_point),
                    [axis_dir.x(), axis_dir.y(), axis_dir.z()],
                );
                norm(r) - radius
            }
            AnalyticSurface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                let (t, r) = axial(v(p, *apex), [axis_dir.x(), axis_dir.y(), axis_dir.z()]);
                // Distance measured perpendicular to the ruling.
                (norm(r) - t * half_angle.tan()) * half_angle.cos()
            }
            AnalyticSurface::Sphere { center, radius } => norm(v(p, *center)) - radius,
            AnalyticSurface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
            } => {
                let (t, r) = axial(v(p, *center), [axis_dir.x(), axis_dir.y(), axis_dir.z()]);
                ((norm(r) - major_radius).powi(2) + t * t).sqrt() - minor_radius
            }
        }
    }

    /// Every boundary vertex of every face must lie on that face's extracted
    /// surface. `import_band`-tier: 1e-9 relative, the band kernel-v2 itself
    /// uses for imported geometry.
    fn assert_vertices_on_surfaces(shell: &AnalyticShellData, what: &str) {
        let mut worst = 0.0f64;
        let mut worst_face = usize::MAX;
        for (fi, face) in shell.faces.iter().enumerate() {
            for l in &face.loops {
                let AnalyticLoop::Edges(oriented) = l else {
                    continue;
                };
                for oe in oriented {
                    let e = shell.edges[oe.edge as usize];
                    for vi in [e.start, e.end] {
                        let p = shell.vertices[vi as usize];
                        let scale = p.x().abs().max(p.y().abs()).max(p.z().abs()).max(1.0);
                        let rel = residual(&face.surface, p).abs() / scale;
                        if rel > worst {
                            worst = rel;
                            worst_face = fi;
                        }
                    }
                }
            }
        }
        assert!(
            worst <= 1e-9,
            "{what}: face {worst_face} has a vertex {worst:.3e} off its extracted surface \
             (relative) — the extraction is wrong, not the file"
        );
    }

    #[test]
    fn a_planar_cube_extracts_exactly() {
        // The truck-written cube is already fully in vocabulary: 6 PLANE, 12
        // LINE, no b-splines.
        let shell = only_shell("cube.step");
        assert_eq!(shell.counts(), (8, 12, 6), "cube V/E/F");
        assert_eq!(shell.surface_kinds(), vec!["planar"]);
        assert!(shell
            .edges
            .iter()
            .all(|e| matches!(e.curve, AnalyticCurve::Line)));
        assert_vertices_on_surfaces(&shell, "cube");
    }

    #[test]
    fn an_out_of_vocabulary_shell_is_ineligible_by_name_not_silently_coerced() {
        // tests/fixtures/cylinder.step was written by truck's own exporter,
        // which emits SURFACE_OF_REVOLUTION for the lateral and RATIONAL_B_-
        // SPLINE_CURVE for the rims — so a "cylinder" fixture contains no
        // CYLINDRICAL_SURFACE and no CIRCLE at all (spec §4.3). It is in fact
        // refused at the first rim, before the swept surface is reached, which
        // is why the assertion is on the vocabulary rather than on which of the
        // two out-of-vocabulary entities happens to be visited first.
        let import = analytic("cylinder.step");
        let err = import.shells[0]
            .as_ref()
            .expect_err("a swept/b-spline shell cannot be exact");
        let msg = err.to_string();
        assert!(
            ["b-spline", "NURBS", "swept", "polyline", "p-curve"]
                .iter()
                .any(|e| msg.contains(e)),
            "the rejection must name the offending entity, got: {msg}"
        );
        assert!(
            matches!(err, Ineligible::Curve { .. } | Ineligible::Surface { .. }),
            "and say whether it was an edge or a face: {err:?}"
        );
        assert_eq!(import.eligible_count(), 0);
        assert!(!import.fully_eligible());
        assert_eq!(import.rejections().len(), 1);
    }

    #[test]
    fn a_cylinder_extracts_axis_radius_and_the_ccll_lateral() {
        let shell = only_shell("analytic/cylinder.step");
        assert_eq!(shell.counts(), (2, 3, 3), "cylinder V/E/F");
        assert_eq!(shell.surface_kinds(), vec!["cylindrical", "planar"]);
        assert_vertices_on_surfaces(&shell, "cylinder");

        let lateral = shell
            .faces
            .iter()
            .find(|f| matches!(f.surface, AnalyticSurface::Cylinder { .. }))
            .expect("a cylindrical face");
        let AnalyticSurface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = lateral.surface
        else {
            unreachable!()
        };
        assert!((radius - 0.005).abs() < 1e-12, "5 mm radius: {radius}");
        assert!(axis_dir.z().abs() > 1.0 - 1e-12, "axis is z: {axis_dir:?}");
        assert!(axis_point.x().abs() < 1e-12 && axis_point.y().abs() < 1e-12);
        assert!(
            lateral.same_sense,
            "a solid cylinder's wall points away from its axis"
        );

        // The lateral is the 4-edge [rim, seam, rim, seam] loop, with the seam
        // traversed twice in opposite directions — the form 51 % of real-world
        // cylindrical faces arrive in.
        assert_eq!(lateral.loops.len(), 1, "the lateral has one boundary");
        let AnalyticLoop::Edges(oriented) = &lateral.loops[0] else {
            panic!("edges")
        };
        assert_eq!(oriented.len(), 4, "CCLL lateral loop");
        let seam: Vec<_> = oriented
            .iter()
            .filter(|oe| matches!(shell.edges[oe.edge as usize].curve, AnalyticCurve::Line))
            .collect();
        assert_eq!(seam.len(), 2, "the seam appears twice");
        assert_eq!(seam[0].edge, seam[1].edge, "and it is ONE edge");
        assert_ne!(seam[0].forward, seam[1].forward, "in opposite directions");

        // Both rims are closed circles anchored at one vertex each.
        for e in shell.edges.iter() {
            if let AnalyticCurve::Circle { radius: r, .. } = e.curve {
                assert!(e.is_closed(), "a rim is a closed edge");
                assert!((r - 0.005).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn a_cone_extracts_its_apex_and_half_angle() {
        let shell = only_shell("analytic/cone.step");
        assert_vertices_on_surfaces(&shell, "cone");
        let face = shell
            .faces
            .iter()
            .find(|f| matches!(f.surface, AnalyticSurface::Cone { .. }))
            .expect("a conical face");
        let AnalyticSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        } = face.surface
        else {
            unreachable!()
        };
        // Base radius 5 mm at z = 0, apex at z = 12 mm.
        assert!((apex.z() - 0.012).abs() < 1e-12, "apex z: {}", apex.z());
        assert!(apex.x().abs() < 1e-12 && apex.y().abs() < 1e-12);
        assert!(
            (half_angle - (0.005f64 / 0.012).atan()).abs() < 1e-12,
            "half-angle atan(5/12): {half_angle}"
        );
        // The nappe lies on the +axis_dir side of the apex, i.e. downward.
        assert!(axis_dir.z() < -1.0 + 1e-12, "axis points into the nappe");
        assert!(face.same_sense, "a solid cone's wall points outward");
    }

    #[test]
    fn a_sphere_and_a_torus_extract_their_radii() {
        let sphere = only_shell("analytic/sphere.step");
        assert_vertices_on_surfaces(&sphere, "sphere");
        let AnalyticSurface::Sphere { center, radius } = sphere.faces[0].surface else {
            panic!("a spherical face, got {:?}", sphere.faces[0].surface)
        };
        assert!((radius - 0.005).abs() < 1e-12, "radius: {radius}");
        assert!(center.x().abs().max(center.y().abs()).max(center.z().abs()) < 1e-12);

        let torus = only_shell("analytic/torus.step");
        assert_vertices_on_surfaces(&torus, "torus");
        let AnalyticSurface::Torus {
            major_radius,
            minor_radius,
            axis_dir,
            ..
        } = torus.faces[0].surface
        else {
            panic!("a toroidal face, got {:?}", torus.faces[0].surface)
        };
        assert!(
            (major_radius - 0.015).abs() < 1e-12,
            "major: {major_radius}"
        );
        assert!(
            (minor_radius - 0.005).abs() < 1e-12,
            "minor: {minor_radius}"
        );
        assert!(axis_dir.x().abs() > 1.0 - 1e-12, "axis is x: {axis_dir:?}");
    }

    #[test]
    fn a_bore_wall_is_marked_as_a_cavity_and_caps_carry_rings() {
        let shell = only_shell("analytic/drilled_block.step");
        assert_vertices_on_surfaces(&shell, "drilled_block");

        let bore = shell
            .faces
            .iter()
            .find(|f| matches!(f.surface, AnalyticSurface::Cylinder { .. }))
            .expect("a bore wall");
        assert!(
            !bore.same_sense,
            "a bore wall's outward normal points TOWARD its axis — the \
             distinction the kernel records as `reversed`"
        );

        let ringed = shell.faces.iter().filter(|f| f.has_rings()).count();
        assert_eq!(ringed, 2, "both caps carry the bore's ring");
    }

    /// Orientation must be preserved AS orientation: a cavity wall and a solid
    /// wall differ only in `same_sense`, never in the stored axis.
    #[test]
    fn a_cavity_and_a_solid_wall_share_their_surface_parameters() {
        let solid = only_shell("analytic/cylinder.step");
        let bore = only_shell("analytic/drilled_block.step");
        let axis_of = |s: &AnalyticShellData| {
            s.faces.iter().find_map(|f| match f.surface {
                AnalyticSurface::Cylinder { axis_dir, .. } => Some(axis_dir),
                _ => None,
            })
        };
        assert_eq!(
            axis_of(&solid),
            axis_of(&bore),
            "both axes are +z; only same_sense differs"
        );
    }

    /// Real-world KiCad fixtures live in `refs/step/` (gitignored — license).
    /// R_0603 is 18 PLANE + 8 CYLINDRICAL_SURFACE with LINE and CIRCLE edges:
    /// entirely in vocabulary, and the canonical SI5 target.
    #[test]
    #[ignore = "refs-fixture: needs local refs/step/R_0603.step (gitignored)"]
    fn a_real_occ_chip_resistor_is_fully_eligible() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../refs/step/R_0603.step");
        let text = std::fs::read_to_string(path).expect("download per roadmap §0");
        let import = parse_step_analytic(&text, "R_0603").expect("parses");
        assert!(
            import.fully_eligible(),
            "R_0603 is analytic-only: {:?}",
            import.rejections()
        );
        let shell = import.eligible().next().expect("one shell");
        assert_eq!(shell.faces.len(), 26, "26 faces");
        assert_eq!(shell.surface_kinds(), vec!["cylindrical", "planar"]);
        assert_vertices_on_surfaces(shell, "R_0603");
    }

    /// USB_C has b-spline-free geometry but 34 solids; every shell should be
    /// eligible, and this is the scale check (515 faces).
    #[test]
    #[ignore = "refs-fixture: needs local refs/step/USB_C.step (gitignored)"]
    fn a_real_occ_connector_extracts_every_shell() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../refs/step/USB_C.step");
        let text = std::fs::read_to_string(path).expect("download per roadmap §0");
        let import = parse_step_analytic(&text, "USB_C").expect("parses");
        assert_eq!(import.shells.len(), 34);
        for shell in import.eligible() {
            assert_vertices_on_surfaces(shell, "USB_C");
        }
        eprintln!(
            "USB_C: {}/{} shells eligible; rejections: {:?}",
            import.eligible_count(),
            import.shells.len(),
            import.rejections()
        );
    }
}

#[cfg(test)]
mod vertex_loop_guard {
    use super::*;

    /// A file declaring a `VERTEX_LOOP` must be refused outright, because
    /// truck's reader drops that bound without reporting it — see the comment
    /// in `parse_step_analytic`. Built by injecting the entity into a fixture
    /// that otherwise extracts cleanly, so the guard is what changes the
    /// verdict and nothing else.
    #[test]
    fn a_vertex_loop_in_the_source_refuses_the_whole_file() {
        let clean = include_str!("../tests/fixtures/analytic/cylinder.step");
        let import = parse_step_analytic(clean, "clean").expect("parses");
        assert!(import.fully_eligible(), "the fixture is the control");

        // Inject a VERTEX_LOOP that nothing references: truck ignores it, so
        // only our guard can react to it.
        let injected = clean.replace(
            "ENDSEC;\nEND-ISO-10303-21;",
            "#99001 = VERTEX_LOOP('',#25);\nENDSEC;\nEND-ISO-10303-21;",
        );
        assert_ne!(injected, clean, "injection landed");
        let import = parse_step_analytic(&injected, "injected").expect("still parses");
        assert_eq!(import.eligible_count(), 0, "every shell is refused");
        let err = import.shells[0].as_ref().expect_err("refused");
        assert!(
            matches!(err, Ineligible::SilentlyDroppedTopology { .. }),
            "and for the right reason: {err:?}"
        );
        assert!(err.to_string().contains("silently drops"));
    }
}

#[cfg(test)]
mod tiered {
    use super::*;

    fn fixture(rel: &str) -> String {
        let path = format!("{}/tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The C6 contract on in-tree fixtures: truck's cube is planes bounded by
    /// lines, so it is served exactly and nothing is tessellated; truck's
    /// cylinder writes its rims as B-spline curves (12 `B_SPLINE_CURVE`
    /// entities), which is out of vocabulary by name, so that shell is served
    /// from the mesh tier with the reason attached.
    #[test]
    fn the_tier_is_decided_per_shell_and_the_mesh_fallback_is_named() {
        let cube = parse_step_tiered(&fixture("cube.step"), "cube").expect("parses");
        assert_eq!(cube.shells.len(), 1);
        assert_eq!(cube.exact_count(), 1);
        assert!(cube.rejections().is_empty());
        let TieredShell::Exact(shell) = &cube.shells[0] else {
            panic!("the cube is exact")
        };
        assert_eq!(shell.faces.len(), 6);

        let cyl = parse_step_tiered(&fixture("cylinder.step"), "cylinder").expect("parses");
        assert_eq!(cyl.exact_count(), 0);
        assert_eq!(cyl.mesh_count(), 1);
        let TieredShell::Mesh { why, data } = &cyl.shells[0] else {
            panic!("truck's cylinder is mesh-tier")
        };
        assert!(matches!(why, Ineligible::Curve { .. }), "{why:?}");
        assert!(
            !data.faces.is_empty(),
            "the mesh tier actually tessellated it"
        );
        assert_eq!(cyl.rejections().len(), 1);
        assert!(cyl.rejections()[0].starts_with("shell 0: "));
    }

    /// The tiered parse and the mesh parse index the same shells: both walk
    /// `collect_placed_shells`, whose order is canonical, so the mesh tier's
    /// `shells[i]` is the exact tier's `shells[i]` — what lets a consumer
    /// fall back per shell by index.
    #[test]
    fn tiered_and_mesh_parses_agree_on_the_shell_list() {
        for name in ["cube.step", "cylinder.step", "analytic/drilled_block.step"] {
            let text = fixture(name);
            let mesh = crate::parse_step(&text, name).expect("mesh parse");
            let tiered = parse_step_tiered(&text, name).expect("tiered parse");
            assert_eq!(mesh.shells.len(), tiered.shells.len(), "{name}");
            for (m, t) in mesh.shells.iter().zip(&tiered.shells) {
                let faces = match t {
                    TieredShell::Exact(s) => s.faces.len(),
                    TieredShell::Mesh { data, .. } => data.faces.len(),
                };
                assert_eq!(m.faces.len(), faces, "{name}: face count per shell");
            }
        }
    }

    /// A boundary of a `BREP_WITH_VOIDS` is refused before its geometry is
    /// looked at, however clean that geometry is — the exact tier builds
    /// one solid per shell and cannot carry the grouping yet (spec §5.2).
    #[test]
    fn a_shell_of_a_solid_with_voids_is_refused_by_name() {
        let text = fixture("analytic/block.step");
        let table = Table::from_step(&text).expect("parses");
        let mut warnings = Vec::new();
        let placed = crate::convert::collect_placed_shells(&table, &mut warnings).expect("shells");
        assert_eq!(placed.len(), 1);
        let (unit_scale, _) = crate::units::scan_length_unit_scale(&text);
        assert!(
            extract_placed(&placed[0], unit_scale).is_ok(),
            "the control is eligible"
        );

        let as_outer_of_a_voided_solid = crate::convert::PlacedShell {
            shell: placed[0].shell.clone(),
            solid: 0,
            shells_in_solid: 2,
        };
        let err = extract_placed(&as_outer_of_a_voided_solid, unit_scale).expect_err("refused");
        assert!(
            matches!(err, Ineligible::Voids { shells_in_solid: 2 }),
            "{err:?}"
        );
        assert!(err.to_string().contains("voids"));
    }
}
