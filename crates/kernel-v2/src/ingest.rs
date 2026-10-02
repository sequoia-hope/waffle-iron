//! Exact ingestion of an imported analytic shell into the arena — STEP import
//! milestone **SI5 checkpoints C3 (planar) and C4a (full curved bands)**
//! (`specs/step_import_si5_exact_analytic_ingestion.md`).
//!
//! The input is [`AnalyticShellData`]: the index tables of a `CLOSED_SHELL` as
//! the exchange file itself wrote them, with every surface and every curve
//! carrying its full analytic parameters (produced by
//! `step_import::parse_step_analytic`, SI5 C2). The output is a real arena
//! solid — no tessellation anywhere in the path, so the ingested body is a
//! first-class kernel solid rather than the mesh-backed [`crate::imported`]
//! tier.
//!
//! ## Vocabulary
//!
//! - **C3** — planes bounded by `LINE` edges. 15.7 % of the ingestible subset
//!   of ABC chunk 0000 (spec §2.1).
//! - **C4a** — cylinders and cones as a **full band**: two closed `CIRCLE` rims,
//!   arriving either as two single-rim loops (every corpus writer) or as the
//!   arena's own canonical seamed lateral (`kernel_v2::step_export`). Those rims
//!   also bound planar caps, rings and annuli, so the planar tier gains
//!   circle-bounded loops with them. Measured at 37.9 % of cylindrical/conical
//!   faces, and the form in which 20.2 % of cylinder-bearing models arrive
//!   **entirely** (spec §5.1) — with full boolean capability, because a band
//!   ingested this way presents exactly the two full-circle rims
//!   `to_yang_brep` requires.
//!
//! - **C4b** — the arc patch: open circle and ellipse arcs with their side
//!   READ from the file's `interior` point (spec `si5_c4b_arc_patch_tier.md`).
//! - **C5a** — the torus **latitude band**: a `TOROIDAL_SURFACE` between two
//!   closed circles coaxial with its axis — the fillet around a boss or a
//!   hole, 21 of the 31 sphere/torus-bearing models in the sample and a form
//!   no kernel-v2 constructor builds (spec `si5_c5_sphere_torus_tier.md`).
//! - **C5b** — torus and sphere **patches**: a `TOROIDAL_SURFACE` bounded by
//!   open latitude and poloidal arcs (the parameter rectangle, every torus
//!   patch in the sample) and a `SPHERICAL_SURFACE` bounded by great/small
//!   circle arcs (the three-fillet corner blend). Both take the C4b patch path
//!   unchanged — the work was the two closed-form volume terms
//!   (`geom::torus_arc_patch_flux`, `geom::sphere_arc_patch_flux`).
//!
//! A sphere band or windowed sphere, the closed sphere's seam slit, a holed
//! band and a vertex loop are typed, loud refusals here naming their
//! checkpoint (C5c), never a guess. The caller falls back to the mesh tier (C6
//! wires that fallback).
//!
//! ## Why a direct assembler and not an Euler sequence
//!
//! Same reason as [`crate::boolean::from_yang_brep`], whose three-pass shape
//! this module follows deliberately: the file already *is* a half-edge
//! structure in index form (two faces reference the same `edges[i]` with
//! opposite orientation), so there is no stitching, no coordinate dedup and no
//! adjacency derivation to do. An Euler path would have to invent an order in
//! which to build a topology we have been handed whole.
//!
//! - **Pass 1 validates everything before the first arena mutation**:
//!   vocabulary, loop continuity and closure, the file's own edge-sharing as
//!   the manifold pairing, the import-tier on-surface gate, exact loop
//!   winding, connectivity, and the genus back-solve.
//! - **Pass 2 assembles** — the validated input makes it infallible.
//! - **Pass 3 is the production gate**: [`finalize_solid`] (hence
//!   [`crate::validate::validate_solid`]: twin pairing, loop closure, vertex
//!   manifoldness, per-surface orientation, exact Euler–Poincaré), the
//!   self-intersection gate, and outward orientation.
//!
//! ## What this module refuses, and why that is the point
//!
//! An ingested solid is epistemically on the **boolean** path, not the
//! constructor path: its geometry is not guaranteed by construction, it is
//! *asserted by a file somebody else wrote*. So every claim the file makes is
//! checked and a disagreement is a refusal, never a repair (P9/P10):
//!
//! - A boundary vertex off its own face's surface by more than the import band
//!   is [`KernelV2Error::AnalyticVertexOffSurface`] — the measurement of spec
//!   §2.2 (residuals ~1e-13 against a 1e-9 band over 6.18 M incidences)
//!   promoted from a probe to a production gate. Not a snap.
//! - Which loop of a face is its outer boundary is MEASURED, not read: STEP
//!   marks it with a subtype and the reader loses the marker, so the
//!   determination is the exact signed area about the face's outward normal
//!   (`AnalyticFace::loops` carries that finding). A face with no such loop,
//!   or two, is a refusal — not a flip to taste (spec §5.4). The sign test's
//!   one blind spot (an inverted sense flips BOTH signs, so a holed face's
//!   ring and perimeter swap consistently) is closed by the containment fact,
//!   exactly: a face's NET signed area over all its loops is positive.
//! - An inward-oriented shell is a refusal. A `BREP_WITH_VOIDS` void arrives
//!   as its own shell today (the solid grouping is lost upstream of C2, spec
//!   §5.2), and ingesting one as a solid would be a silently inside-out body;
//!   1.0 % of the corpus goes to the mesh tier instead until C4+ captures the
//!   grouping.
//!
//! ## The two things C4a derives rather than reads (spec §5.1)
//!
//! **Which way a rim circle is traversed.** A closed edge runs start → start,
//! so its endpoints say nothing, and `AnalyticCurve::interior` does not separate
//! the two directions either (a full circle passes through all of its own points
//! both ways). The file's circle axis would say — but that is exactly the class
//! of sign `same_sense` taught us not to replay (truck inverts curves while
//! parsing, `Processor` carries its own orientation flag, `EDGE_CURVE.same_sense`
//! is a third). So the sense comes from the band's own material law instead
//! ([`crate::validate::faces::validate_cylinder_face`]: with `reversed == false`
//! each rim's traversal axis points TOWARD the other rim), and the face across
//! the rim takes the negation. The declared circle axis is used only as a
//! geometric consistency check.
//!
//! **Where to cut a circle that has no cut.** kernel-v2's lateral is Stroud
//! 2006 §3.1.4's single-fake-edge loop `[rim, seam, rim, seam]`, and the seam
//! must be a RULING, so the two rims' anchor vertices have to share an azimuth.
//! 82.4 % of corpus bands already do; the rest need a rim RE-ANCHORED. That is
//! not a repair and not a tolerance move: the anchor of a closed edge is pure
//! representation gauge, and sliding it along its own circle changes no point of
//! the boundary. It is admissible exactly when the vertex is load-bearing for
//! nothing else — measured at 4 of 4 794, and those faces are refused instead.
//! Because bands chain (a stepped shaft shares rims), the azimuth is chosen per
//! CONNECTED COMPONENT of rims-joined-by-bands, not pairwise.
//!
//! ## The one thing a torus band cannot derive, and where it comes from (C5a)
//!
//! Two rims bound ONE cylinder band, so "toward the other rim" fixes the
//! sense. Two latitude circles on a torus bound TWO complementary regions
//! (genus 1): the quarter-round between them or the three-quarter round the
//! other way, and the band's own geometry cannot say which. The file's
//! `ORIENTED_EDGE` flag could — and it is now MEASURED to disagree with the
//! C4a law on 60 of 1 544 cylinder/cone rims (3.9 %, spec §2.2), so it seeds
//! nothing. What does: every band's rim component holds a cylinder or cone
//! band whose own law fixes the sense (58 of 64 in the sample), and the sense
//! PROPAGATES across a torus band exactly. For the region running +φ from rim
//! *s* to rim *e* — either region — the start rim is traversed CCW about `+â`
//! and the end rim CCW about `−â` (1d derives it), so one rim fixed by its
//! neighbour fixes the other, and which rim got `+â` says which region the
//! face is. A band whose component has no such seed is a named refusal.

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::{Point3, TAU_EVAL};
use waffle_types::kernel::{AnalyticCurve, AnalyticLoop, AnalyticShellData, AnalyticSurface};

use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::construct::finalize_solid;
use crate::error::KernelV2Error;
use crate::geom;

/// How far a declared surface normal may be off unit length. The extractor
/// normalizes, so this is a corrupted-input wall rather than a rounding
/// allowance — and the normal is stored with the file's own bits, never
/// renormalized here, so that coplanar faces sharing one `PLANE` entity keep
/// bit-identical normals (the plane-fidelity rule that keeps a later boolean
/// from seeing two faces of one plane as near-coplanar strangers).
const INGEST_NORMAL_TOLERANCE: f64 = TAU_EVAL;

/// The import-tier on-surface band: `TAU_EVAL·(1 + ‖p‖∞)`, the same
/// scale-relative form as the boolean-output planarity gate
/// ([`crate::validate::validate_boolean_output_planarity`]). Spec §2.2
/// measured real writers at ~1e-13, so this sits ≥1000× above the input's
/// actual residual and a trip is a finding about the file, not noise.
fn on_surface_band(p: Point3) -> f64 {
    TAU_EVAL * (1.0 + p.x().abs().max(p.y().abs()).max(p.z().abs()))
}

type V3 = [f64; 3];

/// The same direction, reversed — a backwards walk of a curved edge (C4b).
fn neg_unit(n: UnitVector3) -> UnitVector3 {
    UnitVector3 {
        x: -n.x,
        y: -n.y,
        z: -n.z,
    }
}

fn sub(a: Point3, b: Point3) -> V3 {
    [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()]
}

fn dot3(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len3(a: V3) -> f64 {
    dot3(a, a).sqrt()
}

fn offset(p: Point3, d: V3) -> Point3 {
    Point3::new(p.x() + d[0], p.y() + d[1], p.z() + d[2])
}

fn unitize(v: V3) -> Option<V3> {
    let l = len3(v);
    if !(l.is_finite() && l > 0.0) {
        return None;
    }
    Some([v[0] / l, v[1] / l, v[2] / l])
}

/// Component of `v` perpendicular to the unit axis `a`.
fn radial(v: V3, a: V3) -> V3 {
    let t = dot3(v, a);
    [v[0] - t * a[0], v[1] - t * a[1], v[2] - t * a[2]]
}

/// A face's surface with the file's orientation already applied: a plane's
/// normal negated when `same_sense` is false, a curved surface's cavity flag
/// set instead (the arena records a reversed curved face in `reversed`, never
/// by flipping its axis).
#[derive(Clone, Copy)]
enum FaceSurface {
    Plane {
        origin: Point3,
        normal: V3,
    },
    Cylinder {
        axis_point: Point3,
        axis_dir: V3,
        radius: f64,
        reversed: bool,
    },
    Cone {
        apex: Point3,
        axis_dir: V3,
        half_angle: f64,
        reversed: bool,
    },
    /// C5a. A ring torus (`major > minor`, enforced at extraction); the
    /// cavity flag records a concave fillet (the material outside the tube).
    Torus {
        center: Point3,
        axis_dir: V3,
        major: f64,
        minor: f64,
        reversed: bool,
    },
    /// C5b. The cavity flag records a spherical dimple (the material outside
    /// the ball). A sphere has no axis: its only admitted form is the patch.
    Sphere {
        center: Point3,
        radius: f64,
        reversed: bool,
    },
}

impl FaceSurface {
    fn is_curved(&self) -> bool {
        !matches!(self, FaceSurface::Plane { .. })
    }

    fn axis(&self) -> Option<V3> {
        match *self {
            FaceSurface::Plane { .. } | FaceSurface::Sphere { .. } => None,
            FaceSurface::Cylinder { axis_dir, .. }
            | FaceSurface::Cone { axis_dir, .. }
            | FaceSurface::Torus { axis_dir, .. } => Some(axis_dir),
        }
    }

    fn reversed(&self) -> bool {
        match *self {
            FaceSurface::Plane { .. } => false,
            FaceSurface::Cylinder { reversed, .. }
            | FaceSurface::Cone { reversed, .. }
            | FaceSurface::Torus { reversed, .. }
            | FaceSurface::Sphere { reversed, .. } => reversed,
        }
    }

    /// Distance from `p` to this surface, the quantity the import-tier
    /// on-surface gate bands. `None` when `p` is somewhere the surface has no
    /// defined residual (behind a cone's apex).
    fn residual(&self, p: Point3) -> Option<f64> {
        match *self {
            FaceSurface::Plane { origin, normal } => Some(dot3(sub(p, origin), normal).abs()),
            FaceSurface::Cylinder {
                axis_point,
                axis_dir,
                radius,
                ..
            } => Some((len3(radial(sub(p, axis_point), axis_dir)) - radius).abs()),
            FaceSurface::Cone {
                apex,
                axis_dir,
                half_angle,
                ..
            } => {
                let d = sub(p, apex);
                let tau = dot3(d, axis_dir);
                if !(tau.is_finite() && tau > 0.0) {
                    return None;
                }
                Some((len3(radial(d, axis_dir)) - geom::cone_radius_at(tau, half_angle)).abs())
            }
            FaceSurface::Torus {
                center,
                axis_dir,
                major,
                minor,
                ..
            } => {
                // A LENGTH, unlike `geom::torus_residual`'s length²: the
                // distance from the tube's centre circle, less the tube radius.
                let d = sub(p, center);
                let tau = dot3(d, axis_dir);
                let rho = len3(radial(d, axis_dir));
                Some(((rho - major).hypot(tau) - minor).abs())
            }
            FaceSurface::Sphere { center, radius, .. } => {
                Some((len3(sub(p, center)) - radius).abs())
            }
        }
    }

    fn to_arena(self) -> Surface {
        let u = |v: V3| UnitVector3 {
            x: v[0],
            y: v[1],
            z: v[2],
        };
        match self {
            FaceSurface::Plane { origin, normal } => Surface::Plane(Plane {
                point: origin,
                normal: u(normal),
            }),
            FaceSurface::Cylinder {
                axis_point,
                axis_dir,
                radius,
                reversed,
            } => Surface::Cylinder {
                axis_point,
                axis_dir: u(axis_dir),
                radius,
                reversed,
            },
            FaceSurface::Cone {
                apex,
                axis_dir,
                half_angle,
                reversed,
            } => Surface::Cone {
                apex,
                axis_dir: u(axis_dir),
                half_angle,
                reversed,
            },
            FaceSurface::Torus {
                center,
                axis_dir,
                major,
                minor,
                reversed,
            } => Surface::Torus {
                center,
                axis_dir: u(axis_dir),
                major_radius: major,
                minor_radius: minor,
                reversed,
            },
            FaceSurface::Sphere {
                center,
                radius,
                reversed,
            } => Surface::Sphere {
                center,
                radius,
                reversed,
            },
        }
    }

    /// C5a: the poloidal angle of a closed rim on this torus — `φ` with
    /// `ρ = R + r cos φ`, `τ = r sin φ` — after checking the circle really is
    /// a latitude circle of THIS torus: axis parallel, centre on the axis, and
    /// radius-and-height consistent with one `φ` within the import band. Any
    /// disagreement is a refusal naming it; nothing is snapped.
    fn torus_rim_phi(&self, r: &Rim) -> Result<f64, KernelV2Error> {
        let FaceSurface::Torus {
            center,
            axis_dir,
            major,
            minor,
            ..
        } = *self
        else {
            unreachable!("torus_rim_phi on a non-torus");
        };
        if dot3(r.declared_axis, axis_dir).abs() < 1.0 - INGEST_NORMAL_TOLERANCE {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a torus band's rim circle is not coaxial with the torus (not a latitude circle)",
            ));
        }
        let d = sub(r.center, center);
        let tau = dot3(d, axis_dir);
        if len3(radial(d, axis_dir)) > on_surface_band(r.center) {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a torus band's rim circle is not centred on the torus axis",
            ));
        }
        // The rim lies on the torus iff its (ρ, τ) is `minor` from the tube
        // centre circle — the same quantity the on-surface gate bands.
        if ((r.radius - major).hypot(tau) - minor).abs() > on_surface_band(r.center) {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a torus band's rim circle does not lie on the torus",
            ));
        }
        Ok(tau.atan2(r.radius - major))
    }
}

/// The traversal axis of a rim about `axis` as a sign: `+1` CCW about `+axis`.
fn axis_sign(sense: V3, axis: V3) -> f64 {
    if dot3(sense, axis) > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// The stored sense vector for `r` that is CCW about `sign · axis`: `±r`'s own
/// declared axis, so a twin's negation is bit-exact (1d's rule).
fn sense_about(r: &Rim, axis: V3, sign: f64) -> V3 {
    let s = sign * axis_sign(r.declared_axis, axis);
    [
        s * r.declared_axis[0],
        s * r.declared_axis[1],
        s * r.declared_axis[2],
    ]
}

/// The parameters of a closed `CIRCLE` edge, as the file declares them.
#[derive(Clone, Copy)]
struct Rim {
    anchor: u32,
    center: Point3,
    /// The circle's declared axis. Used ONLY to check that the rim is
    /// perpendicular to its band's axis — never for a traversal sign (module
    /// docs).
    declared_axis: V3,
    radius: f64,
}

/// What a face loop is made of, decided before anything is assembled.
enum LoopShape {
    /// A chain of straight edges — C3's planar polygon.
    Polygon {
        cycle: Vec<u32>,
        edges: Vec<u32>,
        forwards: Vec<bool>,
    },
    /// A single closed-circle edge: a rim.
    Rim { edge: u32 },
    /// A band that already carries its own seam: one loop of
    /// `[rim, seam, rim, seam]` with the SAME line edge traversed once each
    /// way — the arena's canonical lateral (`arena.rs:36-57`), which is what
    /// `kernel_v2::step_export` writes.
    ///
    /// No corpus writer emits this (every `CCLL` face measured is a partial
    /// patch of arcs and two distinct rulings, spec §5.1), but our own
    /// exporter does, and the export → extract → ingest fixed point is SI5's
    /// strongest acceptance oracle (spec §8.7). Accepting it costs a shape
    /// check and nothing else: it needs no minting at all.
    Seamed {
        rim_a: u32,
        rim_b: u32,
        seam: u32,
        /// The file's own flag on the seam use that follows `rim_a`.
        seam_forward: bool,
    },
}

/// One validated loop as it will be assembled: its owning face, kind, and the
/// per-half-edge entry vertex / edge key / traversal sense / arena curve.
struct LoopPlan {
    face: usize,
    kind: LoopKind,
    cycle: Vec<u32>,
    /// Edge keys: `0..shell.edges.len()` are the file's own edges, above that
    /// are minted seams. Twin pairing is keyed by this.
    edges: Vec<u32>,
    forwards: Vec<bool>,
    curves: Vec<Curve>,
}

/// Union-find over rim edge keys, so the seam azimuth is chosen per connected
/// component of rims-joined-by-bands (a stepped shaft chains them).
fn uf_find(parent: &mut BTreeMap<u32, u32>, mut x: u32) -> u32 {
    while parent[&x] != x {
        let up = parent[&parent[&x]];
        parent.insert(x, up);
        x = up;
    }
    x
}

fn uf_union(parent: &mut BTreeMap<u32, u32>, a: u32, b: u32) {
    let (ra, rb) = (uf_find(parent, a), uf_find(parent, b));
    if ra != rb {
        let (lo, hi) = (ra.min(rb), ra.max(rb));
        parent.insert(hi, lo);
    }
}

/// Ingest one exact analytic shell as a solid in `arena`.
///
/// Vocabulary: planes with straight or full-circle boundaries, and cylinders /
/// cones in the full-band form (see module docs). Returns the new [`SolidId`],
/// or a typed refusal — on `Err` the caller must treat `arena` as having gained
/// no usable solid (pass 2 may have mutated it, exactly as the boolean
/// assembler does when its own exit validation rejects).
pub fn ingest_analytic(
    arena: &mut BrepArena,
    shell: &AnalyticShellData,
) -> Result<SolidId, KernelV2Error> {
    let probe = std::env::var_os("KV2_INGEST_PROBE").is_some();

    // ---- pass 1 (NO arena mutation): validate the file's claims -----------
    if shell.faces.is_empty() {
        return Err(KernelV2Error::InvalidAnalyticShell("shell has no faces"));
    }
    for p in &shell.vertices {
        if !(p.x().is_finite() && p.y().is_finite() && p.z().is_finite()) {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "shell vertex is not finite",
            ));
        }
    }

    // 1a. Surface vocabulary, with the file's orientation applied: a plane's
    //     normal negated (exact in f64, so the stored plane is still the
    //     file's own geometry), a curved surface's cavity flag set.
    let mut surfs: Vec<FaceSurface> = Vec::with_capacity(shell.faces.len());
    // Since C5b every member of the analytic contract is in the surface
    // vocabulary; `KernelV2Error::AnalyticIngestUnsupportedSurface` stays for
    // the next surface the contract grows.
    for face in shell.faces.iter() {
        let unit_axis = |v: cad_primitives::Vector3, what: &'static str| -> Result<V3, _> {
            let a = v.as_array();
            let l = len3(a);
            if !(l.is_finite() && (l - 1.0).abs() <= INGEST_NORMAL_TOLERANCE) {
                return Err(KernelV2Error::InvalidAnalyticShell(what));
            }
            Ok(a)
        };
        surfs.push(match face.surface {
            AnalyticSurface::Plane { origin, normal } => {
                let n = unit_axis(normal, "a face's plane normal is not unit length")?;
                FaceSurface::Plane {
                    origin,
                    normal: if face.same_sense {
                        n
                    } else {
                        [-n[0], -n[1], -n[2]]
                    },
                }
            }
            AnalyticSurface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                let a = unit_axis(axis_dir, "a cylinder axis is not unit length")?;
                if !(radius.is_finite() && radius > 0.0) {
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "a cylinder radius is not finite and positive",
                    ));
                }
                FaceSurface::Cylinder {
                    axis_point,
                    axis_dir: a,
                    radius,
                    // The arena records a bore wall in `reversed`, not by
                    // flipping the axis (`arena::Surface::Cylinder`).
                    reversed: !face.same_sense,
                }
            }
            AnalyticSurface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                let a = unit_axis(axis_dir, "a cone axis is not unit length")?;
                if !(half_angle.is_finite()
                    && half_angle > 0.0
                    && half_angle < std::f64::consts::FRAC_PI_2)
                {
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "a cone half-angle is not finite in (0, pi/2)",
                    ));
                }
                FaceSurface::Cone {
                    apex,
                    axis_dir: a,
                    half_angle,
                    reversed: !face.same_sense,
                }
            }
            AnalyticSurface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
            } => {
                let a = unit_axis(axis_dir, "a torus axis is not unit length")?;
                if !(minor_radius.is_finite()
                    && minor_radius > 0.0
                    && major_radius.is_finite()
                    && major_radius > minor_radius)
                {
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "a torus is not a ring torus (needs major > minor > 0)",
                    ));
                }
                FaceSurface::Torus {
                    center,
                    axis_dir: a,
                    major: major_radius,
                    minor: minor_radius,
                    reversed: !face.same_sense,
                }
            }
            AnalyticSurface::Sphere { center, radius } => {
                if !(radius.is_finite() && radius > 0.0) {
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "a sphere radius is not finite and positive",
                    ));
                }
                FaceSurface::Sphere {
                    center,
                    radius,
                    reversed: !face.same_sense,
                }
            }
        });
    }

    // 1b. Curve vocabulary and edge sanity. A `LINE` edge is defined by its
    //     endpoints, so one whose endpoints are the same vertex, or closer
    //     together than the band in which we certify positions at all, carries
    //     no direction and is refused rather than assembled into a zero-length
    //     half-edge pair. A `CIRCLE` edge is admitted only CLOSED — an open one
    //     is an arc, the C4b partial-patch tier.
    let mut rims: BTreeMap<u32, Rim> = BTreeMap::new();
    // C4b: the arena curve of every OPEN curved edge, in the direction the file
    // declares (start → end). A loop walking such an edge backwards takes the
    // negated normal, which is exactly `curves_twin_consistent`'s rule.
    let mut open_curves: BTreeMap<u32, Curve> = BTreeMap::new();
    for (ei, edge) in shell.edges.iter().enumerate() {
        let (s, e) = (edge.start as usize, edge.end as usize);
        if s >= shell.vertices.len() || e >= shell.vertices.len() {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "an edge references an out-of-range vertex",
            ));
        }
        match edge.curve {
            AnalyticCurve::Line => {
                if s == e {
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "a line edge closes on its own start vertex",
                    ));
                }
                let (a, b) = (shell.vertices[s], shell.vertices[e]);
                let d = len3(sub(b, a));
                if d <= on_surface_band(a) {
                    if probe {
                        eprintln!(
                            "[ingest-probe] edge {ei} length {d:.3e} <= band {:.3e} ({s} -> {e})",
                            on_surface_band(a)
                        );
                    }
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "an edge is shorter than the import band, so its direction is not knowable",
                    ));
                }
            }
            AnalyticCurve::Circle {
                center,
                normal,
                radius,
                ..
            } => {
                let axis = normal.as_array();
                if s != e {
                    // C4b, the partial-patch tier: an ARC. Its traversal side
                    // is READ from the file's own `interior` point, never
                    // derived from the two endpoints — which is why this tier
                    // may take the 16.8 % of corpus arcs at or within 1° of a
                    // half turn that `from_yang_brep` must refuse as ambiguous
                    // (spec `si5_c4b_arc_patch_tier.md` §3.1, §5.3).
                    if !(len3(axis).is_finite()
                        && (len3(axis) - 1.0).abs() <= INGEST_NORMAL_TOLERANCE
                        && radius.is_finite()
                        && radius > 0.0)
                    {
                        return Err(KernelV2Error::InvalidAnalyticShell(
                            "an arc edge has a non-unit axis or a non-positive radius",
                        ));
                    }
                    let (p, q, i) = (
                        shell.vertices[s],
                        shell.vertices[e],
                        match edge.curve {
                            AnalyticCurve::Circle { interior, .. } => interior,
                            _ => unreachable!("in the Circle arm"),
                        },
                    );
                    // CCW about the DECLARED axis from p to q: does it pass the
                    // file's interior point? If not, the arc runs the other way
                    // and the arena curve carries the negated axis.
                    let Some(span) = geom::ccw_sweep(center, axis, p, q) else {
                        return Err(KernelV2Error::InvalidAnalyticShell(
                            "an arc endpoint has no radial direction from its centre",
                        ));
                    };
                    let Some(to_i) = geom::ccw_sweep(center, axis, p, i) else {
                        return Err(KernelV2Error::InvalidAnalyticShell(
                            "an arc's interior point has no radial direction from its centre",
                        ));
                    };
                    let forward = to_i < span;
                    let n = if forward {
                        axis
                    } else {
                        [-axis[0], -axis[1], -axis[2]]
                    };
                    open_curves.insert(
                        ei as u32,
                        Curve::Arc {
                            center,
                            normal: UnitVector3 {
                                x: n[0],
                                y: n[1],
                                z: n[2],
                            },
                            radius,
                        },
                    );
                    continue;
                }
                if !(len3(axis).is_finite()
                    && (len3(axis) - 1.0).abs() <= INGEST_NORMAL_TOLERANCE
                    && radius.is_finite()
                    && radius > 0.0)
                {
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "a circle edge has a non-unit axis or a non-positive radius",
                    ));
                }
                rims.insert(
                    ei as u32,
                    Rim {
                        anchor: edge.start,
                        center,
                        declared_axis: axis,
                        radius,
                    },
                );
            }
            AnalyticCurve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
                interior,
            } => {
                // C4b takes OPEN ellipse arcs — the oblique cut, 60 of whose 80
                // corpus uses are on a cylinder and 20 on a plane, both forms
                // the validators already carry (PR-KV9 and the exact planar
                // area). A CLOSED ellipse is a rim-like edge whose sense no
                // interior point can settle — the band machinery for it does
                // not exist — so it is named, not guessed.
                if s == e {
                    return Err(KernelV2Error::AnalyticIngestUnsupportedCurve {
                        edge: ei,
                        curve: "closed ELLIPSE edge (C4b takes open ellipse arcs)",
                    });
                }
                let axis = normal.as_array();
                let maj = major_axis.as_array();
                if !(len3(axis).is_finite()
                    && (len3(axis) - 1.0).abs() <= INGEST_NORMAL_TOLERANCE
                    && (len3(maj) - 1.0).abs() <= INGEST_NORMAL_TOLERANCE
                    && major_radius.is_finite()
                    && minor_radius.is_finite()
                    && minor_radius > 0.0
                    && major_radius >= minor_radius)
                {
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "an ellipse edge has a non-unit frame or non-ordered radii",
                    ));
                }
                // Same rule as the arc, in the ellipse's own parameter.
                let param = |pt: Point3| {
                    geom::ellipse_param(center, axis, maj, major_radius, minor_radius, pt)
                };
                let (Some(t0), Some(t1), Some(ti)) = (
                    param(shell.vertices[s]),
                    param(shell.vertices[e]),
                    param(interior),
                ) else {
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "an ellipse endpoint has no parameter in the declared frame",
                    ));
                };
                let tau = 2.0 * std::f64::consts::PI;
                let forward = (ti - t0).rem_euclid(tau) < (t1 - t0).rem_euclid(tau);
                let n = if forward {
                    axis
                } else {
                    [-axis[0], -axis[1], -axis[2]]
                };
                open_curves.insert(
                    ei as u32,
                    Curve::EllipseArc {
                        center,
                        normal: UnitVector3 {
                            x: n[0],
                            y: n[1],
                            z: n[2],
                        },
                        // The twin carries the negated normal and the SAME
                        // major axis (`arena.rs`): the frame's minor direction
                        // flips with the normal, so the point set is identical.
                        major_axis: UnitVector3 {
                            x: maj[0],
                            y: maj[1],
                            z: maj[2],
                        },
                        major_radius,
                        minor_radius,
                    },
                );
            }
        }
    }

    // 1c. Loop shape: a chain of straight edges (continuity and closure
    //     VERIFIED against the direction the file declares, not inferred), or a
    //     single closed circle. A loop that mixes the two has no arena face.
    let mut face_loops: Vec<Vec<LoopShape>> = Vec::with_capacity(shell.faces.len());
    for (fi, face) in shell.faces.iter().enumerate() {
        if face.loops.is_empty() {
            return Err(KernelV2Error::InvalidAnalyticShell("a face has no loops"));
        }
        let mut shapes = Vec::with_capacity(face.loops.len());
        for (li, lp) in face.loops.iter().enumerate() {
            let AnalyticLoop::Edges(oriented) = lp else {
                // A `VERTEX_LOOP` is a cone apex or a sphere pole: real
                // topology the arena represents (`LoopBoundary::Lone`), but C2
                // refuses the whole file for it anyway (the reader drops it
                // silently, spec §5.3).
                return Err(KernelV2Error::AnalyticIngestUnsupported(
                    "a face boundary is a vertex loop",
                ));
            };
            for oe in oriented {
                if oe.edge as usize >= shell.edges.len() {
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "a loop references an out-of-range edge",
                    ));
                }
            }
            let circles = oriented
                .iter()
                .filter(|oe| rims.contains_key(&oe.edge))
                .count();
            if circles == 1 && oriented.len() == 1 {
                shapes.push(LoopShape::Rim {
                    edge: oriented[0].edge,
                });
                continue;
            }
            if circles > 0 {
                // The only other shape with full circles in it that the arena
                // has a face for is its own canonical lateral.
                let Some(seamed) = classify_seamed(shell, oriented, &rims) else {
                    return Err(KernelV2Error::AnalyticIngestUnsupported(
                        "a loop mixes a full circle with other edges and is not the canonical \
                         [rim, seam, rim, seam] lateral",
                    ));
                };
                shapes.push(seamed);
                continue;
            }
            // Every straight-edge loop is a chain of chords, so a 1- or 2-edge
            // one bounds no area. A C4b chain carrying a curved edge does: a
            // half-disc is one arc and one chord, a lens two arcs.
            let curved_edges = oriented
                .iter()
                .filter(|oe| open_curves.contains_key(&oe.edge))
                .count();
            if oriented.len() < 3 && (curved_edges == 0 || oriented.len() < 2) {
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a loop bounds no area: fewer than three edges and no curved edge",
                ));
            }
            let mut cycle = Vec::with_capacity(oriented.len());
            let mut edges = Vec::with_capacity(oriented.len());
            let mut forwards = Vec::with_capacity(oriented.len());
            let mut cur: Option<u32> = None;
            for oe in oriented {
                let edge = &shell.edges[oe.edge as usize];
                let (from, to) = if oe.forward {
                    (edge.start, edge.end)
                } else {
                    (edge.end, edge.start)
                };
                if cur.is_some_and(|c| c != from) {
                    if probe {
                        eprintln!(
                            "[ingest-probe] face {fi} loop {li} discontinuous at edge {} \
                             (expected entry {:?}, got {from})",
                            oe.edge, cur
                        );
                    }
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "a loop is not edge-continuous in the direction the file declares",
                    ));
                }
                cycle.push(from);
                edges.push(oe.edge);
                forwards.push(oe.forward);
                cur = Some(to);
            }
            if cur != Some(cycle[0]) {
                return Err(KernelV2Error::InvalidAnalyticShell("a loop does not close"));
            }
            shapes.push(LoopShape::Polygon {
                cycle,
                edges,
                forwards,
            });
        }
        // A curved face is either a FULL BAND — two closed rims, as two
        // single-rim loops (every corpus writer) or as the canonical seamed
        // lateral (our own exporter) — or, since C4b, an ARC PATCH: one chain
        // of arcs, ellipse arcs and rulings.
        // A sphere's only admitted form is the PATCH (C5b). Its two closed-
        // circle forms are named refusals (spec §2.3, §2.5 — C5c): the band
        // between two circles is self-seeding but 2 models, non-coaxial by law
        // and without a canonical seam; the windowed sphere needs the
        // unrolled-domain outer-loop ranking C4b refuses on every curved
        // surface. Both must be caught here, BEFORE 1d asks the face for an
        // axis it does not have.
        if matches!(surfs[fi], FaceSurface::Sphere { .. })
            && !shapes
                .iter()
                .all(|sh| matches!(sh, LoopShape::Polygon { .. }))
        {
            return Err(KernelV2Error::AnalyticIngestUnsupported(
                "a spherical face bounded by a closed circle (C5c: sphere band / windowed \
                 sphere)",
            ));
        }
        if surfs[fi].is_curved() && band_of(&shapes).is_none() {
            // Measured (`c4b_arc_patch_census`, spec §5.2): 1 111 of 1 113
            // corpus arc patches have exactly ONE boundary loop and 2 have
            // two. So the single-loop case is stated as what it is — the only
            // loop IS the outer boundary — and the windowed patch is a named
            // refusal rather than general unrolled-domain ranking machinery
            // written for 0.18 % of faces against a law only the validator can
            // check. When a case demands that ranking, this wall names it.
            // A patch is bounded by OPEN edges. A curved face whose loop is a
            // closed circle is an unclosed band (one rim, or a rim plus a ring)
            // — a shape the arena has no face for, and one that no choice of
            // seam fixes, so it stays the refusal it was before C4b.
            if !shapes
                .iter()
                .all(|sh| matches!(sh, LoopShape::Polygon { .. }))
            {
                return Err(KernelV2Error::AnalyticIngestUnsupported(
                    "a curved face is neither a full band of two closed rims nor a patch of \
                     open edges (an unclosed or holed band)",
                ));
            }
            if shapes.len() != 1 {
                return Err(KernelV2Error::AnalyticIngestUnsupported(
                    "a curved patch has more than one boundary loop (C4b: unrolled-domain \
                     outer-loop ranking)",
                ));
            }
            // C5b: no straight line lies on a sphere or a torus, so a LINE edge
            // on such a face is an impossible boundary claim — the on-surface
            // gate (1g) sees only its endpoints, so it is named here.
            if matches!(
                surfs[fi],
                FaceSurface::Sphere { .. } | FaceSurface::Torus { .. }
            ) && shapes.iter().any(|sh| match sh {
                LoopShape::Polygon { edges, .. } => {
                    edges.iter().any(|e| !open_curves.contains_key(e))
                }
                _ => false,
            }) {
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a line edge bounds a spherical or toroidal face (no straight line lies on \
                     either surface)",
                ));
            }
            // A patch loop that walks one edge TWICE is a seam slit, not a
            // region: our own exporter's closed sphere is two uses of one
            // meridian arc between the poles, whose boundary doubles back on
            // itself and bounds the whole sphere with no sign to say so (the
            // sphere flux refuses the ±π exterior angle too, as a P10 net).
            // The closed forms of our own writer — the sphere, the bent tube —
            // are C5c (spec `si5_c5_sphere_torus_tier.md` §3), named here.
            if let [LoopShape::Polygon { edges, .. }] = &shapes[..] {
                let mut seen = edges.clone();
                seen.sort_unstable();
                if seen.windows(2).any(|w| w[0] == w[1]) {
                    return Err(KernelV2Error::AnalyticIngestUnsupported(
                        "a curved patch loop uses one edge twice (a seam slit — C5c: our own \
                         exporter's closed sphere / bent tube)",
                    ));
                }
            }
            // A conical patch bounded by a conic SECTION arc is kernel-v2's own
            // KV16b vocabulary gap, not an ingestion one: `validate_cone_patch`
            // has no rule for an ellipse on a cone (no constant-radius axis-⊥
            // projection, unlike the cylinder-section ellipse). Zero corpus
            // models hit this in the 400-model sample (spec §5.1), so it is a
            // named wall rather than a reach cost.
            if matches!(surfs[fi], FaceSurface::Cone { .. }) {
                let ellipse_here = shapes.iter().any(|sh| match sh {
                    LoopShape::Polygon { edges, .. } => edges
                        .iter()
                        .any(|e| matches!(open_curves.get(e), Some(Curve::EllipseArc { .. }))),
                    _ => false,
                });
                if ellipse_here {
                    return Err(KernelV2Error::AnalyticIngestUnsupported(
                        "a conical patch is bounded by an ellipse arc (KV16b cone-section \
                         conic vocabulary)",
                    ));
                }
            }
        }
        face_loops.push(shapes);
    }

    // 1d. Rim traversal sense, derived from each band's own material law, and
    //     the seam azimuth, chosen per connected component of rims joined by
    //     bands. Both are explained in the module docs; neither is read from
    //     the file, because neither survives the reader with a trustworthy
    //     sign.
    //
    //     `rim_sense[(edge, face)]` is the traversal axis of that rim AS WALKED
    //     by that face.
    //
    //     C5a: a cylinder or cone band SEEDS (its own law fixes the sense); a
    //     torus band's rims are checked here and its sense PROPAGATED below,
    //     because on a torus two latitude circles bound two regions and only
    //     a neighbour can say which (module docs).
    let mut rim_sense: BTreeMap<(u32, usize), V3> = BTreeMap::new();
    let mut uf: BTreeMap<u32, u32> = rims.keys().map(|&k| (k, k)).collect();
    // Torus bands awaiting a sense: (face, [rim_a, rim_b]).
    let mut torus_bands: Vec<(usize, [u32; 2])> = Vec::new();
    for (fi, shapes) in face_loops.iter().enumerate() {
        if !surfs[fi].is_curved() {
            continue;
        }
        // A C4b arc patch has no rim at all, so it has no traversal sense to
        // derive and no seam to place: its boundary arcs carry the side the
        // file's own `interior` points pinned in 1b.
        let Some(band) = band_of(shapes) else {
            continue;
        };
        let (e0, e1) = (&band.rim_a, &band.rim_b);
        let (r0, r1) = (rims[e0], rims[e1]);
        let axis = surfs[fi].axis().expect("curved");
        if matches!(surfs[fi], FaceSurface::Torus { .. }) {
            // Both rims must be latitude circles of THIS torus at two distinct
            // poloidal angles. `torus_rim_phi` checks the geometry; two rims
            // at one angle would be one circle written twice.
            let (p0, p1) = (surfs[fi].torus_rim_phi(&r0)?, surfs[fi].torus_rim_phi(&r1)?);
            let FaceSurface::Torus { minor, .. } = surfs[fi] else {
                unreachable!()
            };
            let dphi = (p1 - p0).rem_euclid(2.0 * std::f64::consts::PI);
            if dphi * minor <= on_surface_band(r0.center)
                || (2.0 * std::f64::consts::PI - dphi) * minor <= on_surface_band(r0.center)
            {
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a torus band's two rims lie at the same poloidal angle",
                ));
            }
            uf_union(&mut uf, *e0, *e1);
            torus_bands.push((fi, [*e0, *e1]));
            continue;
        }
        let d = sub(r1.center, r0.center);
        let t = dot3(d, axis);
        // The rims must be distinct cross-sections of the same axis, and the
        // declared circle axes must agree with the band's.
        if !(t.is_finite() && t.abs() > on_surface_band(r0.center)) {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a curved band's two rims lie at the same axial position",
            ));
        }
        for r in [r0, r1] {
            if dot3(r.declared_axis, axis).abs() < 1.0 - INGEST_NORMAL_TOLERANCE {
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a rim circle's axis disagrees with its band's axis",
                ));
            }
            if len3(radial(sub(r.center, point_on_axis(&surfs[fi])), axis))
                > on_surface_band(r.center)
            {
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a rim circle's centre is off its band's axis",
                ));
            }
        }
        // Material sense: outward (`reversed == false`) ⇒ each rim's traversal
        // axis points TOWARD the other rim.
        //
        // The axis used is each RIM's own declared axis, not the band's. Both
        // are parallel (checked just above), but the rim's is one `DIRECTION`
        // entity shared by the two faces meeting along it, so the sense this
        // face derives and the negation the other face takes are bit-for-bit
        // opposite — which is what `validate_solid`'s exact twin-curve
        // comparison requires. Deriving from the band's own axis would make a
        // cylinder-meets-cone rim depend on two independently written unit
        // vectors agreeing in the last bit.
        let sense = |r: &Rim, toward: V3| -> V3 {
            let mut s = if dot3(r.declared_axis, toward) > 0.0 {
                1.0
            } else {
                -1.0
            };
            if surfs[fi].reversed() {
                s = -s;
            }
            [
                s * r.declared_axis[0],
                s * r.declared_axis[1],
                s * r.declared_axis[2],
            ]
        };
        rim_sense.insert((*e0, fi), sense(&r0, d));
        rim_sense.insert((*e1, fi), sense(&r1, [-d[0], -d[1], -d[2]]));
        uf_union(&mut uf, *e0, *e1);
    }

    // C5a: propagate senses across torus bands to a fixpoint. Why the two
    // rims of ONE torus face are traversed oppositely about its axis,
    // whichever region the face is: with `x = C + (R + r cos φ) ŵ(θ) +
    // r sin φ â` and outward `n = cos φ ŵ + sin φ â`, the frame (∂θ, ∂φ) is
    // right-handed about `n` (ŵ' = â × ŵ), so the boundary of the region
    // `φ_s < φ < φ_e` with material on the left runs +θ at `φ_s` (CCW about
    // `+â`) and −θ at `φ_e` (CCW about `−â`); `reversed` flips both. Which of
    // the two complementary regions the face is follows from which rim got
    // `+â` — the fact 1e and the volume/render paths read back.
    loop {
        let mut changed = false;
        for &(fi, [e0, e1]) in &torus_bands {
            if rim_sense.contains_key(&(e0, fi)) {
                continue;
            }
            let axis = surfs[fi].axis().expect("torus");
            // The sense a neighbour has fixed for rim `e`, as THIS face must
            // walk it (the negation).
            let from_neighbour = |e: u32| {
                rim_sense
                    .iter()
                    .find(|((re, rf), _)| *re == e && *rf != fi)
                    .map(|(_, &n)| [-n[0], -n[1], -n[2]])
            };
            let (n0, n1) = (from_neighbour(e0), from_neighbour(e1));
            let (mine0, mine1) = match (n0, n1) {
                (Some(m0), Some(m1)) => {
                    if axis_sign(m0, axis) == axis_sign(m1, axis) {
                        if probe {
                            eprintln!(
                                "[ingest-probe] torus band face {fi}: rims {e0} and {e1} are \
                                 both fixed CCW about {:+.0}·axis by their neighbours",
                                axis_sign(m0, axis)
                            );
                        }
                        return Err(KernelV2Error::InvalidAnalyticShell(
                            "the faces across a torus band's two rims fix senses that are not \
                             opposite about its axis (the file's faces disagree about which \
                             region the band is)",
                        ));
                    }
                    (m0, m1)
                }
                (Some(m0), None) => (m0, sense_about(&rims[&e1], axis, -axis_sign(m0, axis))),
                (None, Some(m1)) => (sense_about(&rims[&e0], axis, -axis_sign(m1, axis)), m1),
                (None, None) => continue,
            };
            rim_sense.insert((e0, fi), mine0);
            rim_sense.insert((e1, fi), mine1);
            changed = true;
        }
        if !changed {
            break;
        }
    }
    for &(fi, [e0, _]) in &torus_bands {
        if !rim_sense.contains_key(&(e0, fi)) {
            // Measured (spec §2.2): the 6 plane-only components in 400 models
            // sit in a model walled upstream by a non-band cylinder, so the
            // plane-seeded rule (outer CCW, ring CW by containment) has no
            // customer yet and is named rather than built.
            return Err(KernelV2Error::AnalyticIngestUnsupported(
                "a torus band's rim component has no cylinder or cone band to seed its sense \
                 (C5: plane-seeded sense)",
            ));
        }
    }

    // Every rim must be incident to at least one curved face: a full circle
    // bounding two planar faces would mean two faces of one plane, a zero-volume
    // sliver, and nothing would fix its traversal sense.
    for &ei in rims.keys() {
        if !rim_sense.keys().any(|&(e, _)| e == ei) {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a full-circle edge bounds no curved face",
            ));
        }
    }

    // How many times each vertex is named by the file's own edge table. A
    // closed rim names its anchor twice; more than that means the anchor is
    // load-bearing for another edge and may not be re-anchored.
    let mut vertex_refs: BTreeMap<u32, usize> = BTreeMap::new();
    for edge in &shell.edges {
        *vertex_refs.entry(edge.start).or_default() += 1;
        *vertex_refs.entry(edge.end).or_default() += 1;
    }
    let pinned = |r: &Rim| vertex_refs.get(&r.anchor).copied().unwrap_or(0) != 2;

    // Seam azimuth per component. The reference is a PINNED rim when the
    // component has one (its anchor cannot move, so everything else must come
    // to it); otherwise the lowest-keyed rim. `vpos` is the shell's vertex
    // table with re-anchor overrides applied — every later pass reads it.
    let mut vpos: Vec<Point3> = shell.vertices.clone();
    let mut components: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &ei in rims.keys() {
        let root = uf_find(&mut uf, ei);
        components.entry(root).or_default().push(ei);
    }
    for members in components.values() {
        let reference = members
            .iter()
            .copied()
            .find(|e| pinned(&rims[e]))
            .unwrap_or(members[0]);
        let r_ref = rims[&reference];
        let Some(g) = unitize(sub(vpos[r_ref.anchor as usize], r_ref.center)) else {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a rim circle's anchor vertex sits on its own centre",
            ));
        };
        for &ei in members {
            let r = rims[&ei];
            let target = offset(
                r.center,
                [g[0] * r.radius, g[1] * r.radius, g[2] * r.radius],
            );
            // `target` is on the circle by construction EXCEPT for the
            // out-of-plane part, which is exactly `radius · |ĝ·n̂|` — so the
            // coaxiality check is the point error itself, with no invented
            // angular constant.
            if (r.radius * dot3(g, r.declared_axis)).abs() > on_surface_band(target) {
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a seam-aligned rim component is not coaxial",
                ));
            }
            let declared = vpos[r.anchor as usize];
            if len3(sub(target, declared)) <= on_surface_band(declared) {
                continue; // already aligned — keep the FILE's own bits
            }
            if pinned(&r) {
                if probe {
                    eprintln!(
                        "[ingest-probe] rim edge {ei} needs re-anchoring but its anchor vertex \
                         {} is named by {} edge ends",
                        r.anchor,
                        vertex_refs.get(&r.anchor).copied().unwrap_or(0)
                    );
                }
                return Err(KernelV2Error::AnalyticIngestUnsupported(
                    "a rim needing a re-anchored seam shares its anchor vertex with another edge",
                ));
            }
            if probe {
                eprintln!(
                    "[ingest-probe] re-anchored rim edge {ei} vertex {} by {:.3e} m \
                     (gauge move along its own circle)",
                    r.anchor,
                    len3(sub(target, declared))
                );
            }
            vpos[r.anchor as usize] = target;
        }
    }

    // 1e. Build the assembly plans. A curved band's TWO rim loops become ONE
    //     lateral loop `[rim_a, seam_up, rim_b, seam_dn]` around a minted seam
    //     edge traversed once in each direction — Stroud's single fake edge.
    let mut plans: Vec<LoopPlan> = Vec::new();
    let mut next_key = shell.edges.len() as u32;
    let circle_at = |r: &Rim, n: V3| Curve::Circle {
        center: r.center,
        normal: UnitVector3 {
            x: n[0],
            y: n[1],
            z: n[2],
        },
        radius: r.radius,
    };
    // C4b: the arena curve a loop sees when it walks `edge` in the given
    // direction. A backwards walk negates the normal — `curves_twin_consistent`'s
    // rule, so the two uses of one file edge pair up by construction.
    let curve_for = |edge: u32, forward: bool| -> Curve {
        match open_curves.get(&edge) {
            None => Curve::LineSegment,
            Some(&c) if forward => c,
            Some(&c) => match c {
                Curve::Arc {
                    center,
                    normal,
                    radius,
                } => Curve::Arc {
                    center,
                    normal: neg_unit(normal),
                    radius,
                },
                Curve::EllipseArc {
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                } => Curve::EllipseArc {
                    center,
                    normal: neg_unit(normal),
                    major_axis,
                    major_radius,
                    minor_radius,
                },
                other => other,
            },
        }
    };
    for (fi, shapes) in face_loops.iter().enumerate() {
        // A curved face with a band becomes ONE lateral loop; a curved PATCH
        // (C4b) has no band and takes the ordinary chain path below, exactly
        // like a planar face — its single loop is its outer boundary.
        if let (true, Some(band)) = (surfs[fi].is_curved(), band_of(shapes)) {
            let (e0, e1) = (band.rim_a, band.rim_b);
            let (r0, r1) = (rims[&e0], rims[&e1]);
            let (n0, n1) = (rim_sense[&(e0, fi)], rim_sense[&(e1, fi)]);
            // Either the file already wrote the seam (the canonical lateral) or
            // one is minted here under a fresh key. Everything downstream — twin
            // pairing, the Euler count — treats the two identically.
            let (seam, fwd) = match band.seam {
                Some(pair) => pair,
                None => {
                    let k = next_key;
                    next_key += 1;
                    (k, true)
                }
            };
            // The seam is a RULING on a cylinder or cone. On a torus (C5a) it
            // is the POLOIDAL ARC at the rims' shared azimuth `ĝ` — centre
            // `C + R·ĝ`, radius `r` — running +φ from the start rim (the one
            // walked CCW about `+â`, `−â` when reversed; 1d) to the end rim,
            // which is the arc lying ON the face. The arc from anchor a to
            // anchor b therefore carries `+(ĝ × â)` when a is the start rim
            // and the negation otherwise; its twin the negation again.
            let (seam_up, seam_dn) = match surfs[fi] {
                FaceSurface::Torus {
                    center,
                    axis_dir,
                    major,
                    minor,
                    reversed,
                } => {
                    let Some(g) = unitize(radial(sub(vpos[r0.anchor as usize], center), axis_dir))
                    else {
                        return Err(KernelV2Error::InvalidAnalyticShell(
                            "a torus band's rim anchor sits on the torus axis",
                        ));
                    };
                    let cp = offset(center, [g[0] * major, g[1] * major, g[2] * major]);
                    let m = [
                        g[1] * axis_dir[2] - g[2] * axis_dir[1],
                        g[2] * axis_dir[0] - g[0] * axis_dir[2],
                        g[0] * axis_dir[1] - g[1] * axis_dir[0],
                    ];
                    let a_is_start = axis_sign(n0, axis_dir) == if reversed { -1.0 } else { 1.0 };
                    let up = if a_is_start { m } else { [-m[0], -m[1], -m[2]] };
                    // A seam the FILE wrote (our own exporter's form) must be
                    // this arc: same poloidal circle, checked — not adopted.
                    if band.seam.is_some() {
                        match open_curves.get(&seam) {
                            Some(&Curve::Arc {
                                center: fc,
                                radius: fr,
                                ..
                            }) if len3(sub(fc, cp)) <= on_surface_band(cp)
                                && (fr - minor).abs() <= on_surface_band(cp) => {}
                            _ => {
                                return Err(KernelV2Error::InvalidAnalyticShell(
                                    "a torus band's file-written seam is not the poloidal arc \
                                     at its rims' azimuth",
                                ));
                            }
                        }
                    }
                    let arc = |n: V3| Curve::Arc {
                        center: cp,
                        normal: UnitVector3 {
                            x: n[0],
                            y: n[1],
                            z: n[2],
                        },
                        radius: minor,
                    };
                    (arc(up), arc([-up[0], -up[1], -up[2]]))
                }
                _ => (Curve::LineSegment, Curve::LineSegment),
            };
            plans.push(LoopPlan {
                face: fi,
                kind: LoopKind::Outer,
                cycle: vec![r0.anchor, r0.anchor, r1.anchor, r1.anchor],
                edges: vec![e0, seam, e1, seam],
                forwards: vec![rim_forward(&r0, n0), fwd, rim_forward(&r1, n1), !fwd],
                curves: vec![circle_at(&r0, n0), seam_up, circle_at(&r1, n1), seam_dn],
            });
            continue;
        }
        for shape in shapes {
            match shape {
                LoopShape::Polygon {
                    cycle,
                    edges,
                    forwards,
                } => plans.push(LoopPlan {
                    face: fi,
                    // Provisional: which loop is the outer boundary is measured
                    // in 1h, because the file does not say.
                    kind: LoopKind::Inner,
                    cycle: cycle.clone(),
                    edges: edges.clone(),
                    forwards: forwards.clone(),
                    curves: edges
                        .iter()
                        .zip(forwards.iter())
                        .map(|(&e, &fwd)| curve_for(e, fwd))
                        .collect(),
                }),
                LoopShape::Seamed { .. } => {
                    // The canonical lateral is a curved face's shape; a planar
                    // face carrying one would mix a full circle with chords,
                    // which `validate_planar_face` has no rule for.
                    return Err(KernelV2Error::AnalyticIngestUnsupported(
                        "a planar face is bounded by the canonical curved lateral shape",
                    ));
                }
                LoopShape::Rim { edge } => {
                    // The planar side of a rim takes the NEGATION of the sense
                    // the band derived — twins carry negated circle normals.
                    let r = rims[edge];
                    let Some((_, &n)) = rim_sense.iter().find(|((e, f), _)| e == edge && *f != fi)
                    else {
                        return Err(KernelV2Error::InvalidAnalyticShell(
                            "a rim edge has no band to take its traversal sense from",
                        ));
                    };
                    let back = [-n[0], -n[1], -n[2]];
                    plans.push(LoopPlan {
                        face: fi,
                        kind: LoopKind::Inner,
                        cycle: vec![r.anchor],
                        edges: vec![*edge],
                        forwards: vec![rim_forward(&r, back)],
                        curves: vec![circle_at(&r, back)],
                    });
                }
            }
        }
    }
    // A rim shared by TWO curved faces (a cylinder meeting a cone) takes no
    // negation above — both sides derived their own sense, and they must
    // disagree, or the two faces lie on the same side of the circle.
    for &ei in rims.keys() {
        let senses: Vec<V3> = rim_sense
            .iter()
            .filter(|((e, _), _)| *e == ei)
            .map(|(_, &n)| n)
            .collect();
        if senses.len() == 2 && dot3(senses[0], senses[1]) >= 0.0 {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "two curved faces traverse a shared rim the same way",
            ));
        }
        if senses.len() > 2 {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a rim edge bounds more than two curved faces",
            ));
        }
    }

    // 1f. Manifold pairing, keyed by the file's OWN edge index (and, for a
    //     seam, by the key minted for it). The exchange file shares one
    //     `EDGE_CURVE` between the two faces that meet along it, so identity is
    //     given rather than derived from coordinates — stronger than the boolean
    //     assembler's `(v_min, v_max, curve)` key, and it means a writer that
    //     emitted two coincident edge records instead of one is refused here (as
    //     an unpaired edge) rather than welded silently.
    let mut uses: BTreeMap<u32, Vec<(usize, usize, bool)>> = BTreeMap::new();
    for (pi, plan) in plans.iter().enumerate() {
        for k in 0..plan.cycle.len() {
            uses.entry(plan.edges[k])
                .or_default()
                .push((pi, k, plan.forwards[k]));
        }
    }
    for (&ei, u) in &uses {
        if u.len() != 2 {
            if probe {
                eprintln!(
                    "[ingest-probe] edge {ei} has {} uses (faces {:?})",
                    u.len(),
                    u.iter().map(|&(pi, ..)| plans[pi].face).collect::<Vec<_>>()
                );
            }
            return Err(KernelV2Error::InvalidAnalyticShell(
                "an edge is not used by exactly two oriented edges",
            ));
        }
        if u[0].2 == u[1].2 {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "an edge's two uses traverse it in the same direction",
            ));
        }
        // Twins describe the same undirected edge in opposite directions: both
        // straight, or both circles with exactly negated axes.
        let (a, b) = (plans[u[0].0].curves[u[0].1], plans[u[1].0].curves[u[1].1]);
        let twinned = match (a, b) {
            (Curve::LineSegment, Curve::LineSegment) => true,
            (
                Curve::Circle {
                    center: c1,
                    normal: n1,
                    radius: r1,
                },
                Curve::Circle {
                    center: c2,
                    normal: n2,
                    radius: r2,
                },
            )
            | (
                Curve::Arc {
                    center: c1,
                    normal: n1,
                    radius: r1,
                },
                Curve::Arc {
                    center: c2,
                    normal: n2,
                    radius: r2,
                },
            ) => c1 == c2 && r1 == r2 && n1.x == -n2.x && n1.y == -n2.y && n1.z == -n2.z,
            // C4b: the ellipse twin negates the normal and keeps the SAME major
            // axis (`arena.rs`) — the frame's minor direction flips with the
            // normal, so the point set is identical, traversed oppositely.
            (
                Curve::EllipseArc {
                    center: c1,
                    normal: n1,
                    major_axis: m1,
                    major_radius: a1,
                    minor_radius: b1,
                },
                Curve::EllipseArc {
                    center: c2,
                    normal: n2,
                    major_axis: m2,
                    major_radius: a2,
                    minor_radius: b2,
                },
            ) => {
                c1 == c2
                    && a1 == a2
                    && b1 == b2
                    && m1.x == m2.x
                    && m1.y == m2.y
                    && m1.z == m2.z
                    && n1.x == -n2.x
                    && n1.y == -n2.y
                    && n1.z == -n2.z
            }
            _ => false,
        };
        if !twinned {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "an edge's two uses disagree about its curve",
            ));
        }
    }

    // 1g. The import-tier on-surface gate (spec §8 oracle 4): every boundary
    //     vertex on its own face's surface, at the band this tier claims
    //     exactness in. A trip is a measurement about the file — recorded by
    //     the probe, refused loudly, never snapped.
    //
    //     This is one of the three PRODUCTION gates that between them bracket
    //     the file's on-curve claim too, which is what lets the debug-tier
    //     construction tripwire be banded by provenance rather than by curve
    //     form (spec `si5_geometry_provenance_tier.md` §4, where the bracket
    //     is measured as a sweep): the other two are the rim-radius agreement
    //     in `validate_cylinder_face`/`validate_cone_face` (1e-9 · r) and 1e's
    //     seam-anchor reconciliation, which refuses a rim whose anchor cannot
    //     be placed on its own circle.
    for plan in &plans {
        let fi = plan.face;
        for &v in &plan.cycle {
            let p = vpos[v as usize];
            let band = on_surface_band(p);
            let Some(d) = surfs[fi].residual(p) else {
                return Err(KernelV2Error::AnalyticVertexOffSurface { face: fi });
            };
            if d > band {
                if probe {
                    eprintln!(
                        "[ingest-probe] face {fi} vertex {v} off surface: \
                         p=({:.17e},{:.17e},{:.17e}) d={d:.3e} band={band:.3e}",
                        p.x(),
                        p.y(),
                        p.z()
                    );
                }
                return Err(KernelV2Error::AnalyticVertexOffSurface { face: fi });
            }
        }
    }

    // 1h. WHICH LOOP IS THE OUTER BOUNDARY — measured, because the file does
    //     not say. STEP marks the outer bound with a subtype
    //     (`FACE_OUTER_BOUND`) and the reader collapses it into the ordinary
    //     `FACE_BOUND` table, so the marker is gone before extraction
    //     (`AnalyticFace::loops` carries the finding; 7 of 28 polyhedral ABC
    //     models have a face whose first loop is a ring). What survives is
    //     ISO 10303-42's winding law — a bound runs with the material on its
    //     left — so about the face's OUTWARD normal exactly one loop has
    //     positive exact signed area and that one is the outer boundary.
    //
    //     A circle-bounded loop needs no area integral: its signed area is
    //     `±πr²` by the sign of its traversal axis against the face normal,
    //     which is the arena's own rule for such a loop
    //     (`validate::faces::validate_planar_face`) stated as a number.
    //
    //     Zero positive loops means the face's declared sense (`same_sense`)
    //     contradicts its boundary; two or more means the file's loops do not
    //     describe a single region. Both are refusals, not flips to taste
    //     (spec §5.4): the identical disagreement on a curved face is
    //     unresolvable, and a silent flip there inverts a bore wall.
    let mut outer_of_face: Vec<Option<usize>> = vec![None; shell.faces.len()];
    let mut net_area: Vec<f64> = vec![0.0; shell.faces.len()];
    for (pi, plan) in plans.iter().enumerate() {
        if surfs[plan.face].is_curved() {
            // A full band has exactly one loop, and so does every arc patch 1c
            // admits (spec `si5_c4b_arc_patch_tier.md` §5.2 — the windowed
            // patch is refused there by name). Either way there is no second
            // loop to rank it against, and the containment argument below is
            // about a planar face's rings.
            outer_of_face[plan.face] = Some(pi);
            continue;
        }
        let FaceSurface::Plane { normal, .. } = surfs[plan.face] else {
            unreachable!("curved handled above");
        };
        let area = match plan.curves[0] {
            Curve::Circle {
                normal: cn, radius, ..
            } if plan.cycle.len() == 1 => {
                let d = dot3([cn.x, cn.y, cn.z], normal);
                if d.abs() < 1.0 - INGEST_NORMAL_TOLERANCE {
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "a circle-bounded planar loop's axis is not along the face normal",
                    ));
                }
                Some(d.signum() * std::f64::consts::PI * radius * radius)
            }
            _ => {
                let pts: Vec<Point3> = plan.cycle.iter().map(|&v| vpos[v as usize]).collect();
                // C4b: a chord contributes its chord only, a curved edge its
                // exact circular / elliptic segment — so a half-disc's area is
                // the half-disc's, not the triangle's, and the outer-loop
                // determination of §5.4 stays exact on arc boundaries.
                let curves: Vec<geom::LoopEdgeCurve> = plan
                    .curves
                    .iter()
                    .map(|c| match *c {
                        Curve::Arc {
                            center,
                            normal: n,
                            radius,
                        } => geom::LoopEdgeCurve::Circle {
                            center,
                            normal: [n.x, n.y, n.z],
                            radius,
                        },
                        Curve::EllipseArc {
                            center,
                            normal: n,
                            major_axis: m,
                            major_radius,
                            minor_radius,
                        } => geom::LoopEdgeCurve::Ellipse {
                            center,
                            normal: [n.x, n.y, n.z],
                            major_axis: [m.x, m.y, m.z],
                            major_radius,
                            minor_radius,
                        },
                        _ => geom::LoopEdgeCurve::Line,
                    })
                    .collect();
                geom::planar_loop_signed_area(normal, &pts, &curves)
            }
        };
        let Some(area) = area.filter(|a| a.is_finite() && *a != 0.0) else {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a loop encloses no area in its face's plane",
            ));
        };
        if probe {
            eprintln!(
                "[ingest-probe] face {} loop {pi} ({} edges) area={area:.6e}",
                plan.face,
                plan.cycle.len()
            );
        }
        net_area[plan.face] += area;
        if area > 0.0 {
            if outer_of_face[plan.face].is_some() {
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a face has two loops that both wind as an outer boundary",
                ));
            }
            outer_of_face[plan.face] = Some(pi);
        }
    }
    for (fi, outer) in outer_of_face.iter().enumerate() {
        match outer {
            Some(pi) => plans[*pi].kind = LoopKind::Outer,
            None => {
                if probe {
                    eprintln!(
                        "[ingest-probe] face {fi} has no outer boundary: every loop winds as a \
                         ring about the outward normal"
                    );
                }
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a face has no loop winding as its outer boundary (its declared sense \
                     contradicts its own boundary)",
                ));
            }
        }
    }

    // The sign test alone has one blind spot, and it is a SILENT WRONG rather
    // than a refusal: if a face's declared sense is inverted, both signs flip,
    // so on a face with a hole the ring comes out "positive" and the perimeter
    // "negative" — a consistent-looking swap that `validate_solid` would also
    // accept, leaving a face whose perimeter is treated as a hole. The
    // containment fact rules it out exactly and with no threshold: rings lie
    // strictly inside the outer boundary, so the face's NET signed area (every
    // loop summed) is positive. A non-positive net area means the loops cannot
    // be a perimeter with holes in it, whichever one we called outer.
    for (fi, &net) in net_area.iter().enumerate() {
        if surfs[fi].is_curved() {
            continue;
        }
        if !(net.is_finite() && net > 0.0) {
            if probe {
                eprintln!("[ingest-probe] face {fi} net signed area {net:.6e} is not positive");
            }
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a face's loops do not enclose positive area (its rings do not lie inside its \
                 outer boundary, or its declared sense is inverted)",
            ));
        }
    }

    // 1i. Connectivity. ISO 10303-42 requires a closed shell to be connected,
    //     and the contract hands us one shell; a disconnected one would
    //     silently become a multi-shell solid whose shells are really
    //     separate bodies.
    if components_of(shell.faces.len(), &plans) > 1 {
        return Err(KernelV2Error::InvalidAnalyticShell(
            "shell is not connected",
        ));
    }

    // 1j. Genus from the Euler–Poincaré formula, back-solved the way the
    //     boolean assembler does it: `Shell::genus` is stored state with no
    //     derivation helper, and STEP states no genus at all.
    let mut vset: BTreeSet<u32> = BTreeSet::new();
    let mut rings = 0i64;
    for plan in &plans {
        if plan.kind == LoopKind::Inner {
            rings += 1;
        }
        vset.extend(plan.cycle.iter().copied());
    }
    let lhs = vset.len() as i64 - uses.len() as i64 + shell.faces.len() as i64 - rings;
    if lhs % 2 != 0 || lhs > 2 {
        if probe {
            eprintln!(
                "[ingest-probe] V={} E={} F={} R={rings} lhs={lhs}",
                vset.len(),
                uses.len(),
                shell.faces.len()
            );
        }
        return Err(KernelV2Error::InvalidAnalyticShell(
            "shell's Euler characteristic is not genus-representable",
        ));
    }
    let genus = ((2 - lhs) / 2) as u32;

    // ---- pass 2: assemble (validated input ⇒ infallible) ------------------
    // Vertices: referenced ones only, in the file's index order, so the
    // assembly is a deterministic function of the file.
    let mut vert_ids: Vec<Option<VertexId>> = vec![None; shell.vertices.len()];
    for &v in &vset {
        let id = VertexId(arena.vertices.len() as u32);
        arena.vertices.push(Some(Vertex {
            point: vpos[v as usize],
        }));
        vert_ids[v as usize] = Some(id);
    }

    let solid_id = SolidId(arena.solids.len() as u32);
    // ASSERTED, not constructed (spec `si5_geometry_provenance_tier.md`): every
    // coordinate below is the file's own rounding, so the debug-tier tripwires
    // must band this solid at the import tier rather than at the construction
    // tier whose premise ("the assembler placed it from closed form") is false
    // here. The on-curve claim is not dropped: the three production gates of
    // 1g / 1e / `validate_*_face` bracket it (spec §4).
    arena.solids.push(Some(Solid::asserted(Vec::new())));
    let shell_id = ShellId(arena.shells.len() as u32);
    arena.shells.push(Some(Shell {
        solid: solid_id,
        faces: Vec::new(),
        genus,
    }));
    if let Some(Some(solid)) = arena.solids.get_mut(solid_id.index()) {
        solid.shells.push(shell_id);
    }

    let mut twin_table: BTreeMap<u32, HalfEdgeId> = BTreeMap::new();
    let mut face_ids: Vec<Option<FaceId>> = vec![None; shell.faces.len()];
    for plan in &plans {
        let fi = plan.face;
        let face_id = match face_ids[fi] {
            Some(id) => id,
            None => {
                let id = FaceId(arena.faces.len() as u32);
                arena.faces.push(Some(Face {
                    // The file's own surface parameters: the ingested face is
                    // NOT re-fitted to its loop.
                    surface: Some(surfs[fi].to_arena()),
                    outer_loop: LoopId(0), // patched below
                    inner_loops: Vec::new(),
                    shell: shell_id,
                }));
                if let Some(Some(sh)) = arena.shells.get_mut(shell_id.index()) {
                    sh.faces.push(id);
                }
                face_ids[fi] = Some(id);
                id
            }
        };

        let loop_id = LoopId(arena.loops.len() as u32);
        let m = plan.cycle.len();
        let he_base = arena.half_edges.len() as u32;
        for k in 0..m {
            let h = HalfEdgeId(he_base + k as u32);
            let twin = match twin_table.get(&plan.edges[k]) {
                Some(&other) => {
                    if let Some(Some(o)) = arena.half_edges.get_mut(other.index()) {
                        o.twin = h;
                    }
                    other
                }
                None => {
                    twin_table.insert(plan.edges[k], h);
                    h // placeholder; overwritten by the partner's visit
                }
            };
            arena.half_edges.push(Some(HalfEdge {
                twin,
                next: HalfEdgeId(he_base + ((k + 1) % m) as u32),
                prev: HalfEdgeId(he_base + ((k + m - 1) % m) as u32),
                origin: vert_ids[plan.cycle[k] as usize].expect("referenced vertex was created"),
                loop_id,
                curve: plan.curves[k],
            }));
        }
        arena.loops.push(Some(Loop {
            face: face_id,
            boundary: LoopBoundary::Edges(HalfEdgeId(he_base)),
            kind: plan.kind,
        }));
        if let Some(Some(face)) = arena.faces.get_mut(face_id.index()) {
            match plan.kind {
                LoopKind::Outer => face.outer_loop = loop_id,
                LoopKind::Inner => face.inner_loops.push(loop_id),
            }
        }
    }

    // ---- pass 3: production gates (spec §8) -------------------------------
    // `validate_solid` (twin pairing, loop closure, vertex manifoldness,
    // per-surface orientation, exact Euler–Poincaré) plus the face-`Pid`
    // stamp every kernel solid carries.
    finalize_solid(arena, solid_id)?;
    // Oracle 3: no two faces of the ingested body may penetrate each other.
    // An imported file can assert a self-intersecting boundary and nothing
    // upstream would notice.
    crate::validate::validate_boolean_output_self_intersection(arena, solid_id)?;
    // Outward orientation. A consistently INWARD shell passes every check
    // above — it is a perfectly good closed oriented surface — but it is
    // either a `BREP_WITH_VOIDS` void (whose solid grouping is lost upstream
    // of C2, spec §5.2) or a file whose face senses are inverted. Either way
    // ingesting it as a solid would be a silently inside-out body.
    let volume = geom::signed_volume(arena, solid_id)?;
    if !(volume.is_finite() && volume > 0.0) {
        if probe {
            eprintln!("[ingest-probe] signed volume {volume:.6e} is not positive");
        }
        return Err(KernelV2Error::InvalidAnalyticShell(
            "shell is inward-oriented (a void, or inverted face senses)",
        ));
    }

    Ok(solid_id)
}

/// A curved face's band: its two rim edge keys, plus the seam the file supplied
/// — the seam's edge key and the declared direction of the use that follows the
/// first rim — when there is one. `None` means the seam must be minted.
struct Band {
    rim_a: u32,
    rim_b: u32,
    seam: Option<(u32, bool)>,
}

/// Read a curved face's loops as a band. `None` when the face is not a full
/// band at all (an arc patch, a holed band, an unclosed one) — the C4b tier.
fn band_of(shapes: &[LoopShape]) -> Option<Band> {
    match shapes {
        [LoopShape::Rim { edge: a }, LoopShape::Rim { edge: b }] if a != b => Some(Band {
            rim_a: *a,
            rim_b: *b,
            seam: None,
        }),
        [LoopShape::Seamed {
            rim_a,
            rim_b,
            seam,
            seam_forward,
        }] => Some(Band {
            rim_a: *rim_a,
            rim_b: *rim_b,
            seam: Some((*seam, *seam_forward)),
        }),
        _ => None,
    }
}

/// Recognize the arena's canonical lateral in a face loop: four oriented edges
/// that alternate rim / seam / rim / seam, where both seam uses are the SAME
/// line edge traversed opposite ways and its endpoints are the two rims'
/// anchors. `None` for anything else — a shape check, never a repair.
fn classify_seamed(
    shell: &AnalyticShellData,
    oriented: &[waffle_types::kernel::OrientedEdge],
    rims: &BTreeMap<u32, Rim>,
) -> Option<LoopShape> {
    if oriented.len() != 4 {
        return None;
    }
    // Rotate so a rim comes first; the loop is a cycle, so this loses nothing.
    let start = (0..4).find(|&i| rims.contains_key(&oriented[i].edge))?;
    let at = |k: usize| oriented[(start + k) % 4];
    let (rim_a, rim_b) = (at(0).edge, at(2).edge);
    if rim_a == rim_b || !rims.contains_key(&rim_b) {
        return None;
    }
    if rims.contains_key(&at(1).edge) || at(1).edge != at(3).edge {
        return None;
    }
    if at(1).forward == at(3).forward {
        return None;
    }
    let seam = &shell.edges[at(1).edge as usize];
    let (anchor_a, anchor_b) = (rims[&rim_a].anchor, rims[&rim_b].anchor);
    // The seam runs from rim_a's anchor to rim_b's, in the direction the file
    // declares for the use that follows rim_a.
    let (from, to) = if at(1).forward {
        (seam.start, seam.end)
    } else {
        (seam.end, seam.start)
    };
    if (from, to) != (anchor_a, anchor_b) {
        return None;
    }
    Some(LoopShape::Seamed {
        rim_a,
        rim_b,
        seam: at(1).edge,
        seam_forward: at(1).forward,
    })
}

/// The pairing sense of a rim use. A closed edge gives no direction from its
/// endpoints (it runs anchor → anchor), so "forward" is the traversal axis
/// measured against the circle's own declared axis — a sign that is shared by
/// the two faces meeting along the rim and opposite between them, which is
/// exactly what the manifold-pairing check needs.
fn rim_forward(r: &Rim, traversal: V3) -> bool {
    dot3(traversal, r.declared_axis) > 0.0
}

/// A point known to be on a curved surface's axis.
fn point_on_axis(s: &FaceSurface) -> Point3 {
    match *s {
        FaceSurface::Plane { origin, .. } => origin,
        FaceSurface::Cylinder { axis_point, .. } => axis_point,
        FaceSurface::Cone { apex, .. } => apex,
        FaceSurface::Torus { center, .. } | FaceSurface::Sphere { center, .. } => center,
    }
}

/// Number of connected components over face indices, joined by a shared
/// edge key. Union-find, like the boolean assembler's `face_components`,
/// but keyed by the file's edge index.
fn components_of(num_faces: usize, plans: &[LoopPlan]) -> usize {
    let mut parent: Vec<usize> = (0..num_faces).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut edge_face: BTreeMap<u32, usize> = BTreeMap::new();
    for plan in plans {
        for &ei in &plan.edges {
            match edge_face.get(&ei) {
                Some(&other) => {
                    let (ra, rb) = (find(&mut parent, plan.face), find(&mut parent, other));
                    let (lo, hi) = (ra.min(rb), ra.max(rb));
                    parent[hi] = lo;
                }
                None => {
                    edge_face.insert(ei, plan.face);
                }
            }
        }
    }
    (0..num_faces)
        .map(|f| find(&mut parent, f))
        .collect::<BTreeSet<_>>()
        .len()
}

pub mod fixtures;

#[cfg(test)]
mod tests;
