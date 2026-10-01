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
//! An **open** circle edge (an arc) is the partial-patch tier, C4b: a typed,
//! loud refusal here naming the checkpoint, never a guess. So is an ellipse, a
//! sphere, a torus, a holed band, and a vertex loop. The caller falls back to
//! the mesh tier (C6 wires that fallback).
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
}

impl FaceSurface {
    fn is_curved(&self) -> bool {
        !matches!(self, FaceSurface::Plane { .. })
    }

    fn axis(&self) -> Option<V3> {
        match *self {
            FaceSurface::Plane { .. } => None,
            FaceSurface::Cylinder { axis_dir, .. } | FaceSurface::Cone { axis_dir, .. } => {
                Some(axis_dir)
            }
        }
    }

    fn reversed(&self) -> bool {
        match *self {
            FaceSurface::Plane { .. } => false,
            FaceSurface::Cylinder { reversed, .. } | FaceSurface::Cone { reversed, .. } => reversed,
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
        }
    }
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
    for (fi, face) in shell.faces.iter().enumerate() {
        let unsupported = |what: &'static str| KernelV2Error::AnalyticIngestUnsupportedSurface {
            face: fi,
            surface: what,
        };
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
            AnalyticSurface::Sphere { .. } => return Err(unsupported("spherical")),
            AnalyticSurface::Torus { .. } => return Err(unsupported("toroidal")),
        });
    }

    // 1b. Curve vocabulary and edge sanity. A `LINE` edge is defined by its
    //     endpoints, so one whose endpoints are the same vertex, or closer
    //     together than the band in which we certify positions at all, carries
    //     no direction and is refused rather than assembled into a zero-length
    //     half-edge pair. A `CIRCLE` edge is admitted only CLOSED — an open one
    //     is an arc, the C4b partial-patch tier.
    let mut rims: BTreeMap<u32, Rim> = BTreeMap::new();
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
                if s != e {
                    // The partial-patch tier (C4b): an arc, whose traversal the
                    // file pins with its axis plus `interior`. Named rather
                    // than guessed.
                    return Err(KernelV2Error::AnalyticIngestUnsupportedCurve {
                        edge: ei,
                        curve: "circular arc (C4b)",
                    });
                }
                let axis = normal.as_array();
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
            AnalyticCurve::Ellipse { .. } => {
                return Err(KernelV2Error::AnalyticIngestUnsupportedCurve {
                    edge: ei,
                    curve: "ellipse (C4b)",
                })
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
            if oriented.len() < 3 {
                // Every straight-edge loop is a chain of chords, so a 1- or
                // 2-edge one bounds no area.
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a loop of line edges has fewer than three edges",
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
        // A curved face must be a FULL BAND: two closed rims, either as two
        // single-rim loops (every corpus writer) or as the canonical seamed
        // lateral (our own exporter). Everything else curved — arc patches,
        // holed bands, unclosed bands — is the C4b/C5 tier, named rather than
        // mangled.
        if surfs[fi].is_curved() && band_of(&shapes).is_none() {
            return Err(KernelV2Error::AnalyticIngestUnsupported(
                "a curved face is not a full band of two closed rims (C4b partial patch)",
            ));
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
    let mut rim_sense: BTreeMap<(u32, usize), V3> = BTreeMap::new();
    let mut uf: BTreeMap<u32, u32> = rims.keys().map(|&k| (k, k)).collect();
    for (fi, shapes) in face_loops.iter().enumerate() {
        if !surfs[fi].is_curved() {
            continue;
        }
        let band = band_of(shapes).expect("1c admitted only full bands on a curved face");
        let (e0, e1) = (&band.rim_a, &band.rim_b);
        let (r0, r1) = (rims[e0], rims[e1]);
        let axis = surfs[fi].axis().expect("curved");
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
    for (fi, shapes) in face_loops.iter().enumerate() {
        if surfs[fi].is_curved() {
            let band = band_of(shapes).expect("1c admitted only full bands on a curved face");
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
            plans.push(LoopPlan {
                face: fi,
                kind: LoopKind::Outer,
                cycle: vec![r0.anchor, r0.anchor, r1.anchor, r1.anchor],
                edges: vec![e0, seam, e1, seam],
                forwards: vec![rim_forward(&r0, n0), fwd, rim_forward(&r1, n1), !fwd],
                curves: vec![
                    circle_at(&r0, n0),
                    Curve::LineSegment,
                    circle_at(&r1, n1),
                    Curve::LineSegment,
                ],
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
                    curves: vec![Curve::LineSegment; cycle.len()],
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
            ) => c1 == c2 && r1 == r2 && n1.x == -n2.x && n1.y == -n2.y && n1.z == -n2.z,
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
            // A full band has exactly one loop, already Outer: there is no
            // second loop to rank it against, and the containment argument
            // below is about a planar face's rings.
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
                let curves = vec![geom::LoopEdgeCurve::Line; pts.len()];
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
    arena.solids.push(Some(Solid { shells: Vec::new() }));
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

#[cfg(test)]
mod tests;
