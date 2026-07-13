//! Boolean delegation to yang-rs (PR-KV3 planar, PR-KV5b curved).
//!
//! kernel-v2 does NOT implement boolean labeling — that is `cherchi-rs` via
//! `yang-rs` (crate hard rule: this crate's only kernel-rewrite deps are
//! `cad-primitives` and `yang-rs`). This module is the **boundary**: it
//! converts a kernel-v2 solid to yang-rs's own `BRep` input type, invokes
//! `yang_rs::boolean` with the production native backend, and reassembles
//! yang-rs's output `BRep` into a kernel-v2 solid that passes the full
//! [`crate::validate::validate_solid`] invariant set. Per yang-rs's scope
//! rules the conversion lives HERE, on the kernel-v2 side of the boundary —
//! yang-rs never imports kernel-v2's types.
//!
//! ## The three conversion functions
//!
//! - [`to_yang_brep`] — kernel-v2 arena solid → `yang_rs::BRep`. Planar
//!   faces convert per-loop as in PR-KV3. CANONICAL cylinder solids
//!   (PR-KV5b): the KV5a vertex-anchored topology converts to exactly the
//!   yang-rs M5 fixture shape — SHARED full-circle rim edges (each
//!   referenced by its cap loop and the lateral loop, normal = the cap's
//!   outward normal) and a shared seam edge appearing twice in the lateral
//!   loop. Sharing is REQUIRED: yang Stage 1 builds one cached rim ring per
//!   circle edge index and the lateral stitches to the caps' rings — split
//!   edges would tessellate non-watertight. PARTIAL curved faces (arc
//!   edges, non-canonical laterals — the shape a previous curved boolean
//!   produced) cannot re-enter yang Stage 1 (`BRep::new` requires laterals
//!   with exactly 2 full circle rims) and are rejected with the typed
//!   [`KernelV2Error::UnsupportedCurvedBoolean`].
//! - [`from_yang_brep`] — yang-rs *output* `BRep` → kernel-v2 solid.
//!   PR-KV5b vocabulary, established by the survey
//!   (`tests/kv5b_survey.rs`): `Plane` + `Cylinder` surfaces (`reversed`
//!   only on cylinder cavity walls), `LineSegment` + `Circle` edges where
//!   `start != end` circles are MINOR ARCS of the exact intersection
//!   circles ([`crate::arena::Curve::Arc`]), plus the full
//!   (`start == end`) circles of the canonical M5 shape for the mechanical
//!   round-trip. Anything else — `Ellipse`/`Parabola`/`Hyperbola` (oblique
//!   sections), near-half-circle arcs (minor side ambiguous) — is the
//!   typed, NAMED [`KernelV2Error::UnsupportedBooleanOutputCurve`];
//!   `Sphere`/`Cone` surfaces stay
//!   [`KernelV2Error::UnsupportedBooleanOutputSurface`].
//! - [`boolean_op`] — the composition: convert both inputs, run
//!   `yang_rs::boolean(a, b, op, native_backend)`, reassemble the output.
//!
//! ## Reassembly strategy: direct arena assembler (documented decision)
//!
//! `from_yang_brep` rebuilds topology with a **direct arena assembler**
//! (validated afterward by `validate_solid`), not an Euler-operator
//! sequence. Rationale: yang-rs output is a *face soup with arbitrary
//! (already-final) topology* — possibly holed faces (through-cut results),
//! genus > 0, even multiple disjoint closed shells. Deriving an Euler
//! construction schedule (which `mev` chain, where to `kemr`, which face
//! pairs to `kfmrh`) from such a soup is a graph-search problem that adds
//! no safety: the safety comes from the invariant CHECK, not from the
//! mutation path. The assembler validates the yang structure exhaustively
//! BEFORE the first arena mutation (same atomicity contract as the KV2
//! constructors), assembles, then runs the full production
//! `validate_solid` (twin pairing, loop closure, vertex fans, Newell and
//! its curved analogs, ring winding, Euler–Poincaré) as defense in depth —
//! the result meets exactly the same bar as an Euler-built solid.
//!
//! ## Circle-edge sense derivation (PR-KV5b)
//!
//! yang's `Curve::Circle` normal is the circle's plane normal; it does NOT
//! encode traversal direction, while kernel-v2's [`Curve`] normals are
//! directional (the curved orientation source of truth). The conversion
//! derives the direction:
//!
//! - **Arc edges**: the geometry IS the minor arc (each arrangement mesh
//!   edge subtends one facet, far below π). The CCW sweep around the
//!   stored normal from `start` to `end` decides the directional normal
//!   (`< π` → as stored, `> π` → negated); a sweep within
//!   [`ARC_MINOR_AMBIGUITY_BAND`] of π is rejected loudly rather than
//!   guessed.
//! - **Full-circle edges**: the planar (cap) use takes the stored normal
//!   sign-adjusted to the face's plane normal (`+` for an outer loop, `−`
//!   for a ring — the KV5a cap convention); the cylinder-lateral use takes
//!   the exact negation. Underivable configurations (both uses on cylinder
//!   faces) are rejected loudly.
//!
//! ## Error mapping (loud, typed)
//!
//! - Coplanar input pairs in the unsupported Stage-0 residue surface as the
//!   typed [`KernelV2Error::UnsupportedCoplanar`] (see [`map_yang_error`]).
//! - Every other yang-rs failure surfaces as
//!   [`KernelV2Error::BooleanFailed`] carrying the yang error's full
//!   Display text — including the cylinder×cylinder Stage-3 SSI wall
//!   (`SsiRefinementFailed`/AmbiguousCurve: the lateral∩lateral
//!   intersection is a degree-4 space curve with no analytic conic). No
//!   masking, no retry, no tolerance fallback (P9/P10).
//! - An empty result (e.g. intersection of disjoint solids) is the typed
//!   [`KernelV2Error::EmptyBooleanResult`] — kernel-v2 has no empty solid.

use std::collections::BTreeMap;

use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    PairSurface, Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::construct::finalize_solid;
use crate::error::KernelV2Error;
use crate::geom;
use cad_primitives::{BoolOp, Point3, Vector3};

/// Tolerance on `1 − dot(Newell(loop), yang_plane_normal)` for the
/// cross-check that a yang output face's stated plane agrees with its
/// boundary walk. Same bar as `validate::NORMAL_AGREEMENT_TOLERANCE` —
/// both vectors are unit-length; only normalization rounding is absorbed.
/// (= the central [`cad_primitives::TAU_EVAL`] rounding tier, F8.)
const YANG_NORMAL_AGREEMENT_TOLERANCE: f64 = cad_primitives::TAU_EVAL;

/// Sweep band (radians) around π inside which an arc's minor side is
/// declared ambiguous and rejected (`UnsupportedBooleanOutputCurve`)
/// rather than guessed. Arrangement mesh edges subtend ≈ one Stage-1 facet
/// (2π/8 .. 2π/16 on the surveyed corpus), orders of magnitude below π, so
/// the band never fires on in-scope geometry — it exists to make the
/// minor-arc assumption a CHECKED precondition, not a silent one.
pub const ARC_MINOR_AMBIGUITY_BAND: f64 = 1e-6;

mod canonicalize;
use self::canonicalize::{canonicalize_sibling_planes, canonicalize_vertices_to_planes};

// ---------------------------------------------------------------------------
// small vector helpers (component math local to this module)
// ---------------------------------------------------------------------------

fn sub(a: Point3, b: Point3) -> [f64; 3] {
    [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm3(a: [f64; 3]) -> f64 {
    dot3(a, a).sqrt()
}

fn normalize3_arr(a: [f64; 3]) -> [f64; 3] {
    let n = norm3(a);
    [a[0] / n, a[1] / n, a[2] / n]
}

fn neg_unit(n: UnitVector3) -> UnitVector3 {
    UnitVector3 {
        x: -n.x,
        y: -n.y,
        z: -n.z,
    }
}

// ---------------------------------------------------------------------------
// to_yang_brep
// ---------------------------------------------------------------------------

/// Convert a kernel-v2 arena solid into yang-rs's `BRep` input type.
///
/// - Planar faces with all-`LineSegment` loops convert per loop (per-face
///   directed edges, `Curve::LineSegment`) exactly as in PR-KV3.
/// - Canonical cylinder solids (PR-KV5b) convert to the yang M5 fixture
///   shape with SHARED rim/seam edges — see the module docs for why
///   sharing is load-bearing.
/// - Partial curved faces (arc edges, non-canonical laterals,
///   `reversed` cylinder surfaces) cannot re-enter yang Stage 1 and are
///   the typed [`KernelV2Error::UnsupportedCurvedBoolean`].
pub fn to_yang_brep(arena: &BrepArena, solid: SolidId) -> Result<yang_rs::BRep, KernelV2Error> {
    Ok(to_yang_brep_indexed(arena, solid)?.0)
}

/// [`to_yang_brep`] plus the **yang-face-index → kernel `FaceId`** mapping
/// (one entry per yang `BRepFace`, in push order). KV13 F2 uses it to map
/// `boolean()`'s per-output-face attribution `(InputId, face_idx)` back to the
/// operand's persistent face id for provenance.
pub fn to_yang_brep_indexed(
    arena: &BrepArena,
    solid: SolidId,
) -> Result<(yang_rs::BRep, Vec<FaceId>), KernelV2Error> {
    let mut vid_map: BTreeMap<VertexId, u32> = BTreeMap::new();
    let mut yverts: Vec<yang_rs::BRepVertex> = Vec::new();
    let mut yedges: Vec<yang_rs::BRepEdge> = Vec::new();
    let mut yfaces: Vec<yang_rs::BRepFace> = Vec::new();
    // KV13 F2: kernel FaceId per pushed yang face (parallel to `yfaces`).
    let mut face_ids: Vec<FaceId> = Vec::new();
    // Shared curved edges (rims, seams), keyed by the lower half-edge id of
    // the twin pair.
    let mut shared_edges: BTreeMap<HalfEdgeId, u32> = BTreeMap::new();

    let map_vertex = |v: VertexId,
                      vid_map: &mut BTreeMap<VertexId, u32>,
                      yverts: &mut Vec<yang_rs::BRepVertex>,
                      arena: &BrepArena|
     -> Result<u32, KernelV2Error> {
        if let Some(&id) = vid_map.get(&v) {
            return Ok(id);
        }
        let id = vid_map.len() as u32;
        vid_map.insert(v, id);
        yverts.push(yang_rs::BRepVertex {
            point: arena.vertex(v)?.point,
        });
        Ok(id)
    };

    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            match face.surface {
                Some(Surface::Plane(plane)) => {
                    // Generic per-loop conversion (PR-KV6b-2). Edge classes:
                    // - LineSegment  → one directed yang edge per half-edge
                    //   (the m1 per-loop-copy convention; vertices dedup 1:1)
                    // - Curve::Arc   → one SHARED yang edge per twin pair,
                    //   carrying the FIRST-ENCOUNTERED half-edge's endpoints
                    //   + directional normal (the yang input-arc convention:
                    //   the point set is the CCW sweep around the stored
                    //   normal from start to end — twin traversal denotes
                    //   the same set, so either side is correct; sharing
                    //   keeps the Stage-1 sample chains watertight)
                    // - closed Circle → one SHARED yang edge per twin pair
                    //   (full rims of holed annular caps / disk caps),
                    //   carrying this half-edge's directional normal
                    let mut convert_loop = |lid: LoopId| -> Result<Vec<u32>, KernelV2Error> {
                        let hes = arena.loop_half_edges(lid)?;
                        if hes.is_empty() {
                            return Err(KernelV2Error::NonManifoldTopology(
                                "to_yang_brep: lone-vertex loop has no edge boundary",
                            ));
                        }
                        let mut indices = Vec::with_capacity(hes.len());
                        for &h in &hes {
                            let he = arena.half_edge(h)?;
                            match he.curve {
                                // M5 K11: surface-pair (true degree-4)
                                // boundaries have no yang Stage-1 INPUT
                                // tessellation — boolean outputs carrying
                                // them are terminal for chaining (typed
                                // wall; a later milestone).
                                Curve::SurfacePair { .. } => {
                                    return Err(KernelV2Error::UnsupportedCurvedBoolean {
                                        face: f,
                                        reason: "planar-loop degree-4 boundary (surface-pair edge)",
                                    });
                                }
                                // KV14 ellipse-arc re-entry (spec
                                // `kv14_ellipse_arc_reentry`): an oblique-
                                // section ellipse arc maps field-for-field to
                                // the yang input `Curve::Ellipse` (identical
                                // CCW parameterization around the stored
                                // forward normal; kernel-v2 constructs only
                                // MINOR arcs, sweep < π, so the CCW sweep
                                // from start to end is unambiguous). One
                                // SHARED yang edge per twin pair — the
                                // Stage-1 chain is sampled once, keeping the
                                // cap∩lateral boundary watertight.
                                Curve::EllipseArc {
                                    center,
                                    normal,
                                    major_axis,
                                    major_radius,
                                    minor_radius,
                                } => {
                                    let key = h.min(he.twin);
                                    let idx = match shared_edges.get(&key) {
                                        Some(&idx) => idx,
                                        None => {
                                            let idx = yedges.len() as u32;
                                            let start = map_vertex(
                                                he.origin,
                                                &mut vid_map,
                                                &mut yverts,
                                                arena,
                                            )?;
                                            let dest = arena.half_edge(he.next)?.origin;
                                            let end =
                                                map_vertex(dest, &mut vid_map, &mut yverts, arena)?;
                                            yedges.push(yang_rs::BRepEdge {
                                                start,
                                                end,
                                                curve: yang_rs::Curve::Ellipse {
                                                    center,
                                                    normal: Vector3::new(
                                                        normal.x, normal.y, normal.z,
                                                    ),
                                                    major_axis: Vector3::new(
                                                        major_axis.x,
                                                        major_axis.y,
                                                        major_axis.z,
                                                    ),
                                                    major_radius,
                                                    minor_radius,
                                                },
                                            });
                                            shared_edges.insert(key, idx);
                                            idx
                                        }
                                    };
                                    indices.push(idx);
                                }
                                // KV16 hyperbola-arc re-entry: maps
                                // field-for-field to the yang input
                                // `Curve::Hyperbola` (identical cosh/sinh
                                // branch parameterization; traversal is
                                // endpoint-determined). One SHARED yang edge
                                // per twin pair — the Stage-1 chain is
                                // sampled once, keeping the boundary
                                // watertight.
                                Curve::HyperbolaArc {
                                    center,
                                    normal,
                                    major_axis,
                                    semi_transverse,
                                    semi_conjugate,
                                } => {
                                    let key = h.min(he.twin);
                                    let idx = match shared_edges.get(&key) {
                                        Some(&idx) => idx,
                                        None => {
                                            let idx = yedges.len() as u32;
                                            let start = map_vertex(
                                                he.origin,
                                                &mut vid_map,
                                                &mut yverts,
                                                arena,
                                            )?;
                                            let dest = arena.half_edge(he.next)?.origin;
                                            let end =
                                                map_vertex(dest, &mut vid_map, &mut yverts, arena)?;
                                            yedges.push(yang_rs::BRepEdge {
                                                start,
                                                end,
                                                curve: yang_rs::Curve::Hyperbola {
                                                    center,
                                                    normal: Vector3::new(
                                                        normal.x, normal.y, normal.z,
                                                    ),
                                                    major_axis: Vector3::new(
                                                        major_axis.x,
                                                        major_axis.y,
                                                        major_axis.z,
                                                    ),
                                                    semi_transverse,
                                                    semi_conjugate,
                                                },
                                            });
                                            shared_edges.insert(key, idx);
                                            idx
                                        }
                                    };
                                    indices.push(idx);
                                }
                                Curve::LineSegment => {
                                    let start =
                                        map_vertex(he.origin, &mut vid_map, &mut yverts, arena)?;
                                    let dest = arena.half_edge(he.next)?.origin;
                                    let end = map_vertex(dest, &mut vid_map, &mut yverts, arena)?;
                                    let idx = yedges.len() as u32;
                                    yedges.push(yang_rs::BRepEdge {
                                        start,
                                        end,
                                        curve: yang_rs::Curve::LineSegment,
                                    });
                                    indices.push(idx);
                                }
                                Curve::Circle {
                                    center,
                                    normal,
                                    radius,
                                }
                                | Curve::Arc {
                                    center,
                                    normal,
                                    radius,
                                } => {
                                    let key = h.min(he.twin);
                                    let idx = match shared_edges.get(&key) {
                                        Some(&idx) => idx,
                                        None => {
                                            let idx = yedges.len() as u32;
                                            let start = map_vertex(
                                                he.origin,
                                                &mut vid_map,
                                                &mut yverts,
                                                arena,
                                            )?;
                                            let end = if matches!(he.curve, Curve::Circle { .. }) {
                                                start
                                            } else {
                                                let dest = arena.half_edge(he.next)?.origin;
                                                map_vertex(dest, &mut vid_map, &mut yverts, arena)?
                                            };
                                            yedges.push(yang_rs::BRepEdge {
                                                start,
                                                end,
                                                curve: yang_rs::Curve::Circle {
                                                    center,
                                                    normal: Vector3::new(
                                                        normal.x, normal.y, normal.z,
                                                    ),
                                                    radius,
                                                },
                                            });
                                            shared_edges.insert(key, idx);
                                            idx
                                        }
                                    };
                                    indices.push(idx);
                                }
                            }
                        }
                        Ok(indices)
                    };

                    let outer = convert_loop(face.outer_loop)?;
                    let mut inners = Vec::with_capacity(face.inner_loops.len());
                    for &rid in &face.inner_loops {
                        inners.push(convert_loop(rid)?);
                    }

                    // Anchor d at a loop point so the plane passes exactly
                    // through the boundary geometry (an arc/circle loop's
                    // anchor vertex works the same as a polygon vertex).
                    let first_he = arena.loop_half_edges(face.outer_loop)?[0];
                    let p0 = arena.vertex(arena.half_edge(first_he)?.origin)?.point;
                    let n = plane.normal;
                    // `+ 0.0` normalizes −0.0 → +0.0 so exactly-coplanar
                    // sibling faces (a 180° revolve's snapped caps) emit
                    // BIT-IDENTICAL planes — yang's intra-coplanar gate
                    // excludes the bit-identical class as benign.
                    let d = -(n.x * p0.x() + n.y * p0.y() + n.z * p0.z()) + 0.0;
                    face_ids.push(f);
                    yfaces.push(yang_rs::BRepFace {
                        surface: yang_rs::Surface::Plane {
                            normal: Vector3::new(n.x + 0.0, n.y + 0.0, n.z + 0.0),
                            d,
                        },
                        outer_loop: outer,
                        inner_loops: inners,
                        reversed: false,
                    });
                }
                Some(Surface::Cylinder { reversed, .. })
                | Some(Surface::Cone { reversed, .. })
                | Some(Surface::Torus { reversed, .. }) => {
                    // Cylinder, cone, and torus laterals share this conversion —
                    // the loop vocabulary (rims/profiles + seams/arcs) and edge
                    // handling are identical; only the analytic surface differs,
                    // built at the end (KV6c/KV6d-5a). yang ingests two-rim
                    // frustum cones and a partial torus (two profile circles + a
                    // seam-arc twin pair).
                    // Two convertible shapes (PR-KV6b-2):
                    // - CANONICAL tube: [rim, seam, rim, seam], two closed
                    //   Circle rims, the segs a seam twin PAIR;
                    // - PARTIAL revolve wall: [seg, arc, seg, arc], two
                    //   sweep Arcs + two distinct ruling segments.
                    // `reversed` passes through as yang BRepFace.reversed
                    // (KV6b-1 Stage-1 orients cavity walls inward).
                    // Anything else — boolean-OUTPUT patches whose curved
                    // boundaries are chord polylines — cannot re-enter yang
                    // Stage 1 as structured rim/strip pairs. HOLED cylinder
                    // laterals now route through the KV14 Slice C path below.
                    //
                    // Per-edge converter shared by the holed-patch path (KV14
                    // Slice C) and the structured 4-edge path below: Arc →
                    // directional yang `Circle`, full `Circle` rim → cap-outward
                    // (negated-normal) shared edge, LineSegment → endpoints;
                    // degree-4 (ellipse/surface-pair) edges are the typed wall.
                    // Twin-pair sharing (key = min half-edge id) keeps the
                    // Stage-1 sample chains identical across adjacent faces.
                    let convert_lateral_edge = |h: HalfEdgeId,
                                                arena: &BrepArena,
                                                vid_map: &mut BTreeMap<VertexId, u32>,
                                                yverts: &mut Vec<yang_rs::BRepVertex>,
                                                yedges: &mut Vec<yang_rs::BRepEdge>,
                                                shared_edges: &mut BTreeMap<HalfEdgeId, u32>|
                     -> Result<u32, KernelV2Error> {
                        let he = arena.half_edge(h)?;
                        let key = h.min(he.twin);
                        if let Some(&idx) = shared_edges.get(&key) {
                            return Ok(idx);
                        }
                        let idx = yedges.len() as u32;
                        match he.curve {
                            // M5 K11: no yang INPUT vocabulary for
                            // surface-pair (true degree-4) edges.
                            Curve::SurfacePair { .. } => {
                                return Err(KernelV2Error::UnsupportedCurvedBoolean {
                                    face: f,
                                    reason: "curved lateral degree-4 boundary (surface-pair edge)",
                                });
                            }
                            // KV14 ellipse-arc re-entry: shared directional
                            // ellipse arc — endpoints + frame from the
                            // FIRST-ENCOUNTERED half-edge (the yang input
                            // convention: the point set is the CCW minor-arc
                            // sweep around the stored normal from start to
                            // end; the twin denotes the same set).
                            Curve::EllipseArc {
                                center,
                                normal,
                                major_axis,
                                major_radius,
                                minor_radius,
                            } => {
                                let start = map_vertex(he.origin, vid_map, yverts, arena)?;
                                let dest = arena.half_edge(he.next)?.origin;
                                let end = map_vertex(dest, vid_map, yverts, arena)?;
                                yedges.push(yang_rs::BRepEdge {
                                    start,
                                    end,
                                    curve: yang_rs::Curve::Ellipse {
                                        center,
                                        normal: Vector3::new(normal.x, normal.y, normal.z),
                                        major_axis: Vector3::new(
                                            major_axis.x,
                                            major_axis.y,
                                            major_axis.z,
                                        ),
                                        major_radius,
                                        minor_radius,
                                    },
                                });
                            }
                            // KV16 hyperbola-arc re-entry: shared
                            // endpoint-determined hyperbola piece (twin
                            // carries bit-identical fields — either side's
                            // descriptor denotes the same point set).
                            Curve::HyperbolaArc {
                                center,
                                normal,
                                major_axis,
                                semi_transverse,
                                semi_conjugate,
                            } => {
                                let start = map_vertex(he.origin, vid_map, yverts, arena)?;
                                let dest = arena.half_edge(he.next)?.origin;
                                let end = map_vertex(dest, vid_map, yverts, arena)?;
                                yedges.push(yang_rs::BRepEdge {
                                    start,
                                    end,
                                    curve: yang_rs::Curve::Hyperbola {
                                        center,
                                        normal: Vector3::new(normal.x, normal.y, normal.z),
                                        major_axis: Vector3::new(
                                            major_axis.x,
                                            major_axis.y,
                                            major_axis.z,
                                        ),
                                        semi_transverse,
                                        semi_conjugate,
                                    },
                                });
                            }
                            Curve::Arc {
                                center,
                                radius,
                                normal,
                            } => {
                                // Shared directional arc: endpoints + normal
                                // from THIS half-edge (the yang input-arc
                                // convention; the twin denotes the same set).
                                let start = map_vertex(he.origin, vid_map, yverts, arena)?;
                                let dest = arena.half_edge(he.next)?.origin;
                                let end = map_vertex(dest, vid_map, yverts, arena)?;
                                yedges.push(yang_rs::BRepEdge {
                                    start,
                                    end,
                                    curve: yang_rs::Curve::Circle {
                                        center,
                                        normal: Vector3::new(normal.x, normal.y, normal.z),
                                        radius,
                                    },
                                });
                            }
                            Curve::Circle {
                                center,
                                radius,
                                normal,
                            } => {
                                // Created from the lateral side: the shared
                                // rim edge carries the CAP-outward normal =
                                // the negation of the lateral half-edge's
                                // directional normal (twins are exact
                                // negations).
                                let nu = neg_unit(normal);
                                let anchor = map_vertex(he.origin, vid_map, yverts, arena)?;
                                yedges.push(yang_rs::BRepEdge {
                                    start: anchor,
                                    end: anchor,
                                    curve: yang_rs::Curve::Circle {
                                        center,
                                        normal: Vector3::new(nu.x, nu.y, nu.z),
                                        radius,
                                    },
                                });
                            }
                            Curve::LineSegment => {
                                let start = map_vertex(he.origin, vid_map, yverts, arena)?;
                                let dest = arena.half_edge(he.next)?.origin;
                                let end = map_vertex(dest, vid_map, yverts, arena)?;
                                yedges.push(yang_rs::BRepEdge {
                                    start,
                                    end,
                                    curve: yang_rs::Curve::LineSegment,
                                });
                            }
                        }
                        shared_edges.insert(key, idx);
                        Ok(idx)
                    };

                    // KV14 (spec `yang_stage1_curved_holed_patch`): a curved
                    // lateral re-enters yang Stage 1 through the unroll + CDT
                    // path (yang `tessellate_lateral_holed_cdt`) — which lays the
                    // boundary chains flat in (u = r·θ, v = axial) param space and
                    // triangulates the polygon-with-holes exactly — in two cases:
                    //   * Slice B/C: it carries inner loops (a hole punched by a
                    //     prior boolean).
                    //   * Slice D: its outer loop is a non-canonical boundary
                    //     (not the structured 4-edge rim/strip pattern the
                    //     analytic `tessellate_lateral_face` path handles), e.g. a
                    //     bounded partial patch bitten by a prior boolean. This
                    //     runs the same CDT with an empty hole set.
                    // CYLINDER (Slice C/D) and CONE (Slice E) are wired; the
                    // TORUS unroll is Slice F, so a torus non-4-edge / holed
                    // lateral stays the typed wall. (Probe KV14_SLICED_PROBE:
                    // R0020/R0093/C0063 are CONE partial patches — Slice E; R0053
                    // is the cylinder Slice-D target.) yang develops a cone via
                    // its isometric development (slant ℓ = |v|/cosα, flattened
                    // angle ψ = θ·sinα), the same unroll+CDT path as the cylinder.
                    let outer_hes = arena.loop_half_edges(face.outer_loop)?;
                    if !face.inner_loops.is_empty() || outer_hes.len() != 4 {
                        // A CONE re-enters via the CDT path only when its
                        // boundary is Line/Arc-only (a bounded partial patch or a
                        // holed partial patch — the 0-encircling Slice-E cases).
                        // A boundary carrying a FULL-circle rim (`Curve::Circle`,
                        // start == end) is the apex-fan (1 rim) or frustum-band
                        // (2 rims) vocabulary — the structured yang cone paths,
                        // which need an apex/ring pairing the CDT converter cannot
                        // supply — so it stays the typed wall. (Cylinders route
                        // full rims through: their periodic strip, Slice B/C, is
                        // bounded by encircling rim circles.)
                        let mut curved_full_rim = false;
                        for &h in &outer_hes {
                            if matches!(arena.half_edge(h)?.curve, Curve::Circle { .. }) {
                                curved_full_rim = true;
                            }
                        }
                        for &lid in &face.inner_loops {
                            for &h in &arena.loop_half_edges(lid)? {
                                if matches!(arena.half_edge(h)?.curve, Curve::Circle { .. }) {
                                    curved_full_rim = true;
                                }
                            }
                        }
                        let surface = match face.surface {
                            Some(Surface::Cylinder {
                                axis_point,
                                axis_dir,
                                radius,
                                ..
                            }) => yang_rs::Surface::Cylinder {
                                axis_point,
                                axis_dir: Vector3::new(axis_dir.x, axis_dir.y, axis_dir.z),
                                radius,
                            },
                            Some(Surface::Cone {
                                apex,
                                axis_dir,
                                half_angle,
                                ..
                            }) if !curved_full_rim => yang_rs::Surface::Cone {
                                apex,
                                axis_dir: Vector3::new(axis_dir.x, axis_dir.y, axis_dir.z),
                                half_angle,
                            },
                            // KV14 Slice F/F-2: a boolean-result torus lateral
                            // re-enters via the UV-CDT path (`yang
                            // tessellate_torus_band` → `tessellate_torus_patch`) as
                            // a POLOIDAL PERIODIC BAND — two meridian-wrapping
                            // profile boundaries (outer + ONE inner) bound the tube.
                            // Slice F-2 additionally carves any REMAINING inner
                            // loops as non-wrapping window holes in the tube wall,
                            // so a band with ≥2 inner loops (other profile +
                            // window(s)) now routes too. A full-circle rim
                            // (`Curve::Circle`) is still the canonical structured
                            // torus (no CDT re-entry) → stays the typed wall.
                            Some(Surface::Torus {
                                center,
                                axis_dir,
                                major_radius,
                                minor_radius,
                                ..
                            }) if !curved_full_rim && !face.inner_loops.is_empty() => {
                                yang_rs::Surface::Torus {
                                    center,
                                    axis_dir: Vector3::new(axis_dir.x, axis_dir.y, axis_dir.z),
                                    major_radius,
                                    minor_radius,
                                }
                            }
                            _ => {
                                let reason = if matches!(face.surface, Some(Surface::Cone { .. })) {
                                    "curved lateral is an apex/frustum cone (full-circle rim; \
                                     no CDT re-entry)"
                                } else if matches!(face.surface, Some(Surface::Torus { .. })) {
                                    "curved lateral is a canonical full-rim torus (full-circle \
                                     rim; no CDT re-entry)"
                                } else if face.inner_loops.is_empty() {
                                    "curved lateral outer loop not 4 edges"
                                } else {
                                    "curved lateral has inner loops"
                                };
                                return Err(KernelV2Error::UnsupportedCurvedBoolean {
                                    face: f,
                                    reason,
                                });
                            }
                        };
                        let mut outer = Vec::with_capacity(outer_hes.len());
                        for &h in &outer_hes {
                            outer.push(convert_lateral_edge(
                                h,
                                arena,
                                &mut vid_map,
                                &mut yverts,
                                &mut yedges,
                                &mut shared_edges,
                            )?);
                        }
                        let mut inners = Vec::with_capacity(face.inner_loops.len());
                        for &lid in &face.inner_loops {
                            let hes = arena.loop_half_edges(lid)?;
                            let mut loop_idx = Vec::with_capacity(hes.len());
                            for &h in &hes {
                                loop_idx.push(convert_lateral_edge(
                                    h,
                                    arena,
                                    &mut vid_map,
                                    &mut yverts,
                                    &mut yedges,
                                    &mut shared_edges,
                                )?);
                            }
                            inners.push(loop_idx);
                        }
                        face_ids.push(f);
                        yfaces.push(yang_rs::BRepFace {
                            surface,
                            outer_loop: outer,
                            inner_loops: inners,
                            reversed,
                        });
                        continue;
                    }
                    // Reaching here, the lateral has no inner loops and exactly
                    // four outer edges (non-4-edge outer loops were routed to the
                    // CDT path above). The structured analytic path below matches
                    // the canonical / partial / torus rim-strip patterns.
                    let mut hes = outer_hes;
                    debug_assert_eq!(hes.len(), 4);
                    if matches!(arena.half_edge(hes[0])?.curve, Curve::LineSegment) {
                        hes.rotate_left(1);
                    }
                    // A torus lateral has ARC seams (no line ruling to anchor the
                    // rotation); rotate a profile CIRCLE to the front so its
                    // (Circle, Arc, Circle, Arc) pattern is recognized below.
                    if matches!(face.surface, Some(Surface::Torus { .. }))
                        && matches!(arena.half_edge(hes[0])?.curve, Curve::Arc { .. })
                    {
                        hes.rotate_left(1);
                    }
                    let curve_of = |h: HalfEdgeId| -> Result<Curve, KernelV2Error> {
                        Ok(arena.half_edge(h)?.curve)
                    };
                    let pattern = (
                        curve_of(hes[0])?,
                        curve_of(hes[1])?,
                        curve_of(hes[2])?,
                        curve_of(hes[3])?,
                    );
                    let canonical = matches!(
                        pattern,
                        (
                            Curve::Circle { .. },
                            Curve::LineSegment,
                            Curve::Circle { .. },
                            Curve::LineSegment
                        )
                    );
                    let partial = matches!(
                        pattern,
                        (
                            Curve::Arc { .. },
                            Curve::LineSegment,
                            Curve::Arc { .. },
                            Curve::LineSegment
                        )
                    );
                    // KV6d-5a: a partial torus lateral — two profile CIRCLES at
                    // the meridian planes + two seam ARCS (the φ=0 longitude twin
                    // pair). No line rulings (the meridian is curved).
                    let torus = matches!(
                        pattern,
                        (
                            Curve::Circle { .. },
                            Curve::Arc { .. },
                            Curve::Circle { .. },
                            Curve::Arc { .. }
                        )
                    );
                    // KV6d closed torus (spec `kv6d_closed_torus_revolve.md`):
                    // both seam circles are CLOSED and both twin pairs are
                    // internal to the loop — [prof, eq, prof⁻¹, eq⁻¹], the
                    // aba⁻¹b⁻¹ square of the cut torus.
                    let closed_torus = matches!(face.surface, Some(Surface::Torus { .. }))
                        && matches!(
                            pattern,
                            (
                                Curve::Circle { .. },
                                Curve::Circle { .. },
                                Curve::Circle { .. },
                                Curve::Circle { .. }
                            )
                        );
                    if !(canonical || partial || torus || closed_torus) {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean {
                            face: f,
                            reason: "curved lateral non-{canonical,partial,torus} edge pattern",
                        });
                    }
                    // Canonical: the two segments must be the seam twin pair.
                    // Partial: two DISTINCT rulings (each twins with a cap edge).
                    // Torus: the two seam ARCS (positions 1, 3) are the twin pair.
                    // Closed torus: BOTH pairs (0, 2) and (1, 3) are twins.
                    if (canonical || torus || closed_torus)
                        && arena.half_edge(hes[1])?.twin != hes[3]
                    {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean {
                            face: f,
                            reason: "curved lateral seam edges not a twin pair",
                        });
                    }
                    if closed_torus && arena.half_edge(hes[0])?.twin != hes[2] {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean {
                            face: f,
                            reason: "curved lateral seam edges not a twin pair",
                        });
                    }

                    // Same twin-pair-sharing conversion as the holed path above
                    // (extracted to `convert_lateral_edge`): the structured
                    // 4-edge rim/strip pattern and the holed patch differ only
                    // in loop count, not per-edge semantics.
                    let mut loop_indices = Vec::with_capacity(4);
                    for &h in &hes {
                        loop_indices.push(convert_lateral_edge(
                            h,
                            arena,
                            &mut vid_map,
                            &mut yverts,
                            &mut yedges,
                            &mut shared_edges,
                        )?);
                    }

                    let surface = match face.surface {
                        Some(Surface::Cylinder {
                            axis_point,
                            axis_dir,
                            radius,
                            ..
                        }) => yang_rs::Surface::Cylinder {
                            axis_point,
                            axis_dir: Vector3::new(axis_dir.x, axis_dir.y, axis_dir.z),
                            radius,
                        },
                        Some(Surface::Cone {
                            apex,
                            axis_dir,
                            half_angle,
                            ..
                        }) => yang_rs::Surface::Cone {
                            apex,
                            axis_dir: Vector3::new(axis_dir.x, axis_dir.y, axis_dir.z),
                            half_angle,
                        },
                        Some(Surface::Torus {
                            center,
                            axis_dir,
                            major_radius,
                            minor_radius,
                            ..
                        }) => yang_rs::Surface::Torus {
                            center,
                            axis_dir: Vector3::new(axis_dir.x, axis_dir.y, axis_dir.z),
                            major_radius,
                            minor_radius,
                        },
                        // The arm pattern restricts face.surface to
                        // Cylinder|Cone|Torus.
                        _ => return Err(KernelV2Error::FaceWithoutSurface { face: f }),
                    };
                    face_ids.push(f);
                    yfaces.push(yang_rs::BRepFace {
                        surface,
                        outer_loop: loop_indices,
                        inner_loops: Vec::new(),
                        reversed,
                    });
                }
                Some(Surface::Sphere {
                    center,
                    radius,
                    reversed,
                }) => {
                    // KV6d increment 2 (spec `kv6d_sphere_revolve.md`): only
                    // the PRISTINE closed modeling sphere re-enters yang
                    // Stage 1 — its seam-Arc twin pair is emitted as the
                    // PR-YR12 fixture (2 pole verts + 1 meridian seam Circle,
                    // start = south / end = north, X–Z seam plane). The
                    // constructor authors the canonical z-up seam, so this is
                    // a direct emission. A boolean-OUTPUT sphere patch has no
                    // structured Stage-1 tessellation yet — typed wall.
                    let hes = arena.loop_half_edges(face.outer_loop)?;
                    let closed = face.inner_loops.is_empty()
                        && hes.len() == 2
                        && arena.half_edge(hes[0])?.twin == hes[1]
                        && matches!(arena.half_edge(hes[0])?.curve, Curve::Arc { .. })
                        && matches!(arena.half_edge(hes[1])?.curve, Curve::Arc { .. });
                    if !closed {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean {
                            face: f,
                            reason: "boolean-output sphere patch cannot re-enter yang \
                                     Stage 1 (closed modeling sphere only — later slice)",
                        });
                    }
                    let (va, vb) = (
                        arena.half_edge(hes[0])?.origin,
                        arena.half_edge(hes[1])?.origin,
                    );
                    let (pa, pb) = (arena.vertex(va)?.point, arena.vertex(vb)?.point);
                    let (v_south, v_north) = if pa.z() <= pb.z() { (va, vb) } else { (vb, va) };
                    let south = map_vertex(v_south, &mut vid_map, &mut yverts, arena)?;
                    let north = map_vertex(v_north, &mut vid_map, &mut yverts, arena)?;
                    let seam = yedges.len() as u32;
                    yedges.push(yang_rs::BRepEdge {
                        start: south,
                        end: north,
                        curve: yang_rs::Curve::Circle {
                            center,
                            normal: Vector3::new(0.0, -1.0, 0.0),
                            radius,
                        },
                    });
                    face_ids.push(f);
                    yfaces.push(yang_rs::BRepFace {
                        surface: yang_rs::Surface::Sphere { center, radius },
                        outer_loop: vec![seam],
                        inner_loops: Vec::new(),
                        reversed,
                    });
                }
                None => return Err(KernelV2Error::FaceWithoutSurface { face: f }),
            }
        }
    }

    canonicalize_sibling_planes(&mut yfaces);
    // World-space vertex canonicalization (spec `m8_shared_boundary_identity`
    // §2): re-derive each all-planar-incident vertex from its canonical
    // planes, band-guarded. WIRED 2026-07-03 after two prerequisite cycles
    // removed its blockers (full decision record: m8 spec §8a):
    // `kv2_cdt_triangulation_core` (no silent-WRONG remains — canon failure
    // modes are loud) and `yang_stage6_sliver_topology` (the F0016/F0024
    // fold-sliver Stage-6 class). Re-wire gate measured on the full assay:
    // wired vs unwired = 83↔83 SUPPORTED_CORRECT, 0 WRONG, no CORRECT lost;
    // coplanar walls R0046/R0088/F0063 lift to their next honest wall.
    canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);

    let brep = yang_rs::BRep::new(yverts, yedges, yfaces).map_err(|e| {
        KernelV2Error::BooleanFailed(format!("yang-rs rejected the converted input B-Rep: {e}"))
    })?;
    Ok((brep, face_ids))
}

// ---------------------------------------------------------------------------
// from_yang_brep
// ---------------------------------------------------------------------------

fn edge_kind_tag(e: &EdgeKind) -> &'static str {
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
enum EdgeKind {
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
enum CurveKey {
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
enum PairSurfaceKey {
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

fn pair_surface_key(s: &crate::arena::PairSurface) -> PairSurfaceKey {
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

fn curve_key(ek: &EdgeKind) -> CurveKey {
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

/// One validated loop of the yang output: owning yang face, kind, the
/// vertex cycle in walk order (yang vertex indices), and the per-edge
/// curve classification.
struct LoopSpec {
    face: usize,
    kind: LoopKind,
    cycle: Vec<u32>,
    edges: Vec<EdgeKind>,
}

/// Surface classification of a yang output face.
enum FaceSurf {
    Plane {
        normal: [f64; 3],
    },
    Cylinder {
        axis_point: Point3,
        axis_dir: [f64; 3],
        radius: f64,
        reversed: bool,
    },
    Cone {
        apex: Point3,
        axis_dir: [f64; 3],
        half_angle: f64,
        reversed: bool,
    },
    Torus {
        center: Point3,
        axis_dir: [f64; 3],
        major_radius: f64,
        minor_radius: f64,
        reversed: bool,
    },
    Sphere {
        center: Point3,
        radius: f64,
        reversed: bool,
    },
}

/// Reassemble a yang-rs *output* `BRep` into a kernel-v2 solid.
///
/// PR-KV5b vocabulary (see module docs): planar faces with polygonal /
/// arc-bearing loops or single full-circle caps; cylinder faces — the
/// canonical full lateral or partial patches with arc/segment boundary
/// loops, including `reversed` cavity walls. The output is validated
/// structurally BEFORE the first arena mutation (loop continuity and
/// closure, twin pairing with curve agreement, orientable planar Newell
/// normals, named-curve vocabulary walls), assembled directly into the
/// arena (see module docs for why a direct assembler rather than an Euler
/// sequence), split into connected shells with per-shell genus derived
/// from the Euler–Poincaré formula, and then re-checked by the full
/// [`crate::validate::validate_solid`] — whose curved orientation analysis
/// (unrolled winding, wrap pairing) is the production gate for the curved
/// faces assembled here.
pub fn from_yang_brep(
    arena: &mut BrepArena,
    brep: &yang_rs::BRep,
) -> Result<SolidId, KernelV2Error> {
    Ok(from_yang_brep_indexed(arena, brep)?.0)
}

/// [`from_yang_brep`] plus the **yang-output-face-index → kernel `FaceId`**
/// mapping (`None` where a yang face produced no kernel face). KV13 F2 uses it
/// to attach the boolean's per-output-face attribution to the output faces'
/// persistent ids.
pub fn from_yang_brep_indexed(
    arena: &mut BrepArena,
    brep: &yang_rs::BRep,
) -> Result<(SolidId, Vec<Option<FaceId>>), KernelV2Error> {
    // PR-KV7: recover B-Rep granularity (output curve tagging) before
    // classification — chord runs on recovered exact circles become arcs /
    // full rims, canonical-pairable cylinder faces become the 4-edge
    // [rim, seam, rim, seam] form. Conservative: bails to the original
    // lists on any structural anomaly, so pass-1 below stays the single
    // validation authority.
    let (rverts, redges, rfaces) = crate::recover::recover_output_curves(brep);
    let yverts: &[yang_rs::BRepVertex] = &rverts;
    let yedges: &[yang_rs::BRepEdge] = &redges;
    let yfaces: &[yang_rs::BRepFace] = &rfaces;

    // ---- pass 1 (NO arena mutation): validate the yang structure ---------
    if yfaces.is_empty() {
        return Err(KernelV2Error::EmptyBooleanResult);
    }

    // 1a. Surface vocabulary. Planar output faces never carry `reversed`
    //     (sense belongs in the plane normal); cylinder faces may.
    let mut surfs: Vec<FaceSurf> = Vec::with_capacity(yfaces.len());
    for f in yfaces.iter() {
        match f.surface {
            yang_rs::Surface::Plane { normal, .. } => {
                if f.reversed {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "planar output face with reversed = true (sense belongs in the plane normal)",
                    ));
                }
                let n = normal.as_array();
                if (norm3(n) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face plane normal is not unit-length",
                    ));
                }
                surfs.push(FaceSurf::Plane { normal: n });
            }
            yang_rs::Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                let a = axis_dir.as_array();
                if (norm3(a) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output cylinder axis_dir is not unit-length",
                    ));
                }
                if !(radius.is_finite() && radius > 0.0) {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output cylinder radius is not finite and positive",
                    ));
                }
                surfs.push(FaceSurf::Cylinder {
                    axis_point,
                    axis_dir: a,
                    radius,
                    reversed: f.reversed,
                });
            }
            yang_rs::Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                let a = axis_dir.as_array();
                if (norm3(a) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output cone axis_dir is not unit-length",
                    ));
                }
                if !(half_angle.is_finite()
                    && half_angle > 0.0
                    && half_angle < std::f64::consts::FRAC_PI_2)
                {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output cone half_angle is not in (0, π/2)",
                    ));
                }
                surfs.push(FaceSurf::Cone {
                    apex,
                    axis_dir: a,
                    half_angle,
                    reversed: f.reversed,
                });
            }
            yang_rs::Surface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
            } => {
                let a = axis_dir.as_array();
                if (norm3(a) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output torus axis_dir is not unit-length",
                    ));
                }
                if !(major_radius.is_finite() && major_radius > 0.0) {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output torus major_radius is not finite and positive",
                    ));
                }
                if !(minor_radius.is_finite() && minor_radius > 0.0 && minor_radius < major_radius)
                {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output torus minor_radius is not finite, positive, and below the major radius",
                    ));
                }
                surfs.push(FaceSurf::Torus {
                    center,
                    axis_dir: a,
                    major_radius,
                    minor_radius,
                    reversed: f.reversed,
                });
            }
            yang_rs::Surface::Sphere { center, radius } => {
                if !(radius.is_finite() && radius > 0.0) {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output sphere radius is not finite and positive",
                    ));
                }
                surfs.push(FaceSurf::Sphere {
                    center,
                    radius,
                    reversed: f.reversed,
                });
            }
        }
    }

    // 1b. Loops: vocabulary, continuity, closure, per-edge classification.
    let mut loops: Vec<LoopSpec> = Vec::new();
    for (fi, f) in yfaces.iter().enumerate() {
        for (li, loop_edges) in std::iter::once(&f.outer_loop)
            .chain(f.inner_loops.iter())
            .enumerate()
        {
            if loop_edges.is_empty() {
                return Err(KernelV2Error::InvalidBooleanOutput("empty output loop"));
            }
            // Walk the loop, inferring each edge's traversal direction by
            // chaining: yang OUTPUT loops are directed-continuous
            // (`e.end == next.start`), but the canonical M5 INPUT shape
            // (the round-trip) reuses one shared seam edge in BOTH
            // directions within the lateral loop, so an edge may be
            // traversed against its stored (start, end).
            let mut cycle = Vec::with_capacity(loop_edges.len());
            let mut kinds = Vec::with_capacity(loop_edges.len());
            let mut has_full = false;
            let mut cur: Option<u32> = None;
            for &ei in loop_edges.iter() {
                let Some(e) = yedges.get(ei as usize) else {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output loop references an out-of-range edge",
                    ));
                };
                if (e.start as usize) >= yverts.len() || (e.end as usize) >= yverts.len() {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output edge references an out-of-range vertex",
                    ));
                }
                // The first edge is taken as stored; later edges chain off
                // the current exit vertex.
                let (from, to) = match cur {
                    None => (e.start, e.end),
                    Some(c) if e.start == c => (e.start, e.end),
                    Some(c) if e.end == c => (e.end, e.start),
                    Some(_) => {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "output loop is not edge-continuous",
                        ));
                    }
                };
                let kind = classify_edge(e, yverts, from, to)?;
                if matches!(kind, EdgeKind::Full { .. }) {
                    has_full = true;
                }
                cycle.push(from);
                kinds.push(kind);
                cur = Some(to);
            }
            if cur != Some(cycle[0]) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output loop does not close",
                ));
            }
            if !has_full && cycle.len() < 3 {
                // KV9-F3 (spec `kv9_f3_output_vertex_identity` §2 amendment,
                // E-V5): a TWO-edge loop whose edges are conic arcs on
                // DISTINCT curves is a genuine LENS BIGON — e.g. the
                // parallel cyl×cyl bite's cap, bounded by one arc of each
                // cylinder's section circle meeting at the two ruling
                // points. The femto-twin artifact used to subdivide these
                // loops spuriously; with output identity fixed they arrive
                // as true bigons, which the CurveKey manifold pairing (the
                // M8 disc∩disc lens machinery) supports downstream. Two
                // edges on the SAME curve (or any line segment) remain a
                // degenerate reject.
                let lens_bigon = cycle.len() == 2
                    && kinds.iter().all(|k| !matches!(k, EdgeKind::Seg))
                    && curve_key(&kinds[0]) != curve_key(&kinds[1]);
                // Spec kv9_f3 §4 row E-V6 (ERROR-census campaign 4): a
                // 2-edge loop with exactly ONE `Seg` and one conic arc is a
                // genuine D-FACE — a circular/elliptic SEGMENT bounded by a
                // chord and its arc (R0046's plane∩cylinder cap fragment).
                // `classify_edge` already validated the arc's endpoints on
                // its conic; the chord shares those vertices by loop
                // closure. Two `Seg`s (a zero-area double edge) and
                // same-curve arc pairs remain the loud reject.
                let dface_bigon = cycle.len() == 2
                    && kinds.iter().filter(|k| matches!(k, EdgeKind::Seg)).count() == 1;
                if !(lens_bigon || dface_bigon) {
                    // KV9-F3 diagnosis probe (read-only, env-gated).
                    if std::env::var_os("KV2_OUT_TWIN_PROBE").is_some() {
                        eprintln!(
                            "[out-loop-probe] face {fi} loop {li} degenerate: \
                             edges {loop_edges:?} cycle {cycle:?} kinds {kinds:?}"
                        );
                        for &v in &cycle {
                            let p = yverts[v as usize].point.as_array();
                            eprintln!("    v{v}: ({},{},{})", p[0], p[1], p[2]);
                        }
                    }
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output loop with fewer than 3 edges and no full-circle edge",
                    ));
                }
            }
            if has_full && cycle.len() != 1 && cycle.len() != 4 {
                // Full circles occur only in the canonical vocabulary: a
                // 1-edge cap loop or the 4-edge [rim, seam, rim, seam]
                // lateral.
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "full-circle edge in a non-canonical loop",
                ));
            }
            loops.push(LoopSpec {
                face: fi,
                kind: if li == 0 {
                    LoopKind::Outer
                } else {
                    LoopKind::Inner
                },
                cycle,
                edges: kinds,
            });
        }
    }

    // 1c. Manifold edge pairing: every undirected vertex pair is used by
    //     exactly two directed loop edges with consistent curve geometry —
    //     opposite directions for ordinary edges; full-circle self-pairs
    //     get their per-use directional normal DERIVED here (see module
    //     docs, "Circle-edge sense derivation").
    struct EdgeUse {
        loop_idx: usize,
        pos: usize,
        forward: bool, // a < b for ordinary edges; unused for self-pairs
    }
    // Keyed by (undirected vertex pair, undirected curve identity) so a LENS
    // bigon (two arcs on different circles sharing both endpoints) pairs each
    // arc separately instead of collapsing to a spurious 4-use "non-manifold"
    // edge (M8 disc∩disc crossing).
    let mut pair_uses: BTreeMap<(u32, u32, CurveKey), Vec<EdgeUse>> = BTreeMap::new();
    for (si, spec) in loops.iter().enumerate() {
        let m = spec.cycle.len();
        for k in 0..m {
            let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
            let key = (a.min(b), a.max(b), curve_key(&spec.edges[k]));
            pair_uses.entry(key).or_default().push(EdgeUse {
                loop_idx: si,
                pos: k,
                forward: a < b,
            });
        }
    }
    // Per (loop, pos) directional normal for full-circle uses.
    let mut full_normals: BTreeMap<(usize, usize), UnitVector3> = BTreeMap::new();
    // KV9-F1 diagnosis probe (read-only, env-gated): report EVERY pairing
    // violation with curve identity + owning loops/faces, and every other
    // use touching the offending vertices, before the loud reject.
    if std::env::var_os("KV2_OUT_TWIN_PROBE").is_some() && pair_uses.values().any(|u| u.len() != 2)
    {
        let mut bad_verts: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for (&(a, b, ref ck), uses) in &pair_uses {
            if uses.len() != 2 {
                let pa = yverts[a as usize].point.as_array();
                let pb = yverts[b as usize].point.as_array();
                eprintln!(
                    "[edge-pair-probe] key ({a},{b}) uses={} curve={ck:?}\n  a: ({},{},{})\n  b: ({},{},{})",
                    uses.len(),
                    pa[0],
                    pa[1],
                    pa[2],
                    pb[0],
                    pb[1],
                    pb[2]
                );
                for u in uses {
                    eprintln!(
                        "    use: yang face {} loop_idx {} pos {} forward {}",
                        loops[u.loop_idx].face, u.loop_idx, u.pos, u.forward
                    );
                }
                bad_verts.insert(a);
                bad_verts.insert(b);
            }
        }
        for (&(a, b, ref ck), uses) in &pair_uses {
            if uses.len() == 2 && (bad_verts.contains(&a) || bad_verts.contains(&b)) {
                eprintln!(
                    "[edge-pair-probe]   context edge ({a},{b}) curve={ck:?} faces {:?}",
                    uses.iter()
                        .map(|u| loops[u.loop_idx].face)
                        .collect::<Vec<_>>()
                );
            }
        }
        // Full loop dumps for every face touching a bad vertex.
        let bad_faces: std::collections::BTreeSet<usize> =
            if std::env::var_os("KV2_OUT_ALL_LOOPS").is_some() {
                loops.iter().map(|s| s.face).collect()
            } else {
                loops
                    .iter()
                    .filter(|s| s.cycle.iter().any(|v| bad_verts.contains(v)))
                    .map(|s| s.face)
                    .collect()
            };
        for spec in &loops {
            if !bad_faces.contains(&spec.face) {
                continue;
            }
            eprintln!(
                "[edge-pair-probe] FACE {} loop ({} edges):",
                spec.face,
                spec.cycle.len()
            );
            let m = spec.cycle.len();
            for k in 0..m {
                let (va, vb) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
                let tag = match &spec.edges[k] {
                    EdgeKind::Seg => "Seg",
                    EdgeKind::Full { .. } => "Full",
                    EdgeKind::Arc { .. } => "Arc",
                    EdgeKind::EllipseArc { .. } => "EllArc",
                    EdgeKind::HyperbolaArc { .. } => "HypArc",
                    EdgeKind::SurfacePair { .. } => "SurfPair",
                };
                let p = yverts[va as usize].point.as_array();
                eprintln!(
                    "    [{k}] {va}->{vb} {tag} from ({:.6},{:.6},{:.6})",
                    p[0], p[1], p[2]
                );
            }
        }
    }
    for (&(a, b, ref _ck), uses) in &pair_uses {
        if uses.len() != 2 {
            return Err(KernelV2Error::InvalidBooleanOutput(
                "an undirected output edge is not used by exactly two directed edges",
            ));
        }
        let (u0, u1) = (&uses[0], &uses[1]);
        let k0 = loops[u0.loop_idx].edges[u0.pos];
        let k1 = loops[u1.loop_idx].edges[u1.pos];
        if a != b {
            if u0.forward == u1.forward {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "an undirected output edge is not used by two OPPOSITE directed edges",
                ));
            }
            // Curve agreement between the two uses.
            match (k0, k1) {
                (EdgeKind::Seg, EdgeKind::Seg) => {}
                (
                    EdgeKind::EllipseArc {
                        center: c0,
                        forward_normal: n0,
                        major_axis: m0,
                        major_radius: a0,
                        minor_radius: b0,
                    },
                    EdgeKind::EllipseArc {
                        center: c1,
                        forward_normal: n1,
                        major_axis: m1,
                        major_radius: a1,
                        minor_radius: b1,
                    },
                ) => {
                    // PR-KV9: same frame, exactly negated traversal normals
                    // (each use's normal is derived from its own walk).
                    if c0 != c1
                        || a0 != a1
                        || b0 != b1
                        || m0 != m1
                        || n0[0] != -n1[0]
                        || n0[1] != -n1[1]
                        || n0[2] != -n1[2]
                    {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "twin output edges carry inconsistent ellipse-arc curves",
                        ));
                    }
                }
                (
                    EdgeKind::Arc {
                        center: c0,
                        radius: r0,
                        forward_normal: n0,
                    },
                    EdgeKind::Arc {
                        center: c1,
                        radius: r1,
                        forward_normal: n1,
                    },
                ) => {
                    // The per-use forward normals are derived from each use's
                    // own (start, end), so the twin pair must come out as
                    // exact negations (same stored circle, opposite walks).
                    if c0 != c1 || r0 != r1 || n0[0] != -n1[0] || n0[1] != -n1[1] || n0[2] != -n1[2]
                    {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "twin output edges carry inconsistent arc curves",
                        ));
                    }
                }
                (
                    EdgeKind::SurfacePair { a: a0, b: b0 },
                    EdgeKind::SurfacePair { a: a1, b: b1 },
                ) => {
                    // M5 (K5): surface-pair twins carry BIT-IDENTICAL defining
                    // surfaces (there is no directional normal to negate —
                    // traversal is endpoint-determined). The undirected pairing
                    // already keys by the ordered pair (CurveKey::SurfacePair),
                    // so this only re-affirms exact agreement.
                    if a0 != a1 || b0 != b1 {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "twin output edges carry inconsistent surface-pair curves",
                        ));
                    }
                }
                (
                    EdgeKind::HyperbolaArc {
                        center: c0,
                        normal: n0,
                        major_axis: m0,
                        semi_transverse: a0,
                        semi_conjugate: b0,
                    },
                    EdgeKind::HyperbolaArc {
                        center: c1,
                        normal: n1,
                        major_axis: m1,
                        semi_transverse: a1,
                        semi_conjugate: b1,
                    },
                ) => {
                    // KV16: hyperbola twins carry BIT-IDENTICAL fields (the
                    // SurfacePair convention — endpoint-determined traversal,
                    // both uses copy the same yang edge descriptor).
                    if c0 != c1 || n0 != n1 || m0 != m1 || a0 != a1 || b0 != b1 {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "twin output edges carry inconsistent hyperbola-arc curves",
                        ));
                    }
                }
                _ => {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "twin output edges carry inconsistent curve kinds",
                    ));
                }
            }
        } else {
            // Full-circle self-pair: derive each use's directional normal.
            let (
                EdgeKind::Full {
                    center: c0,
                    radius: r0,
                    normal: n0,
                },
                EdgeKind::Full {
                    center: c1,
                    radius: r1,
                    normal: n1,
                },
            ) = (k0, k1)
            else {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "a self-loop output edge is not a full circle",
                ));
            };
            if c0 != c1 || r0 != r1 || (n0 != n1 && n0 != [-n1[0], -n1[1], -n1[2]]) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "the two uses of a full-circle edge carry inconsistent circles",
                ));
            }
            // Planar use(s) take ±plane normal by loop kind; a cylinder use
            // takes the negation of its partner.
            let derive_planar = |u: &EdgeUse| -> Option<UnitVector3> {
                let spec = &loops[u.loop_idx];
                let FaceSurf::Plane { normal: pn } = &surfs[spec.face] else {
                    return None;
                };
                let stored = match spec.edges[u.pos] {
                    EdgeKind::Full { normal, .. } => normal,
                    _ => unreachable!("checked full above"),
                };
                let want_sign = match spec.kind {
                    LoopKind::Outer => 1.0,
                    LoopKind::Inner => -1.0,
                };
                let d = dot3(stored, *pn);
                if d.abs() < 1.0 - YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return None; // circle axis disagrees with the face plane
                }
                let s = if d * want_sign > 0.0 { 1.0 } else { -1.0 };
                Some(UnitVector3 {
                    x: s * stored[0],
                    y: s * stored[1],
                    z: s * stored[2],
                })
            };
            let n_for = |u: &EdgeUse, partner: &EdgeUse| -> Result<UnitVector3, KernelV2Error> {
                if let Some(nu) = derive_planar(u) {
                    return Ok(nu);
                }
                if matches!(
                    surfs[loops[u.loop_idx].face],
                    FaceSurf::Cylinder { .. }
                        | FaceSurf::Cone { .. }
                        | FaceSurf::Torus { .. }
                        | FaceSurf::Sphere { .. }
                ) {
                    if let Some(nu) = derive_planar(partner) {
                        return Ok(neg_unit(nu));
                    }
                }
                Err(KernelV2Error::InvalidBooleanOutput(
                    "full-circle edge sense is underivable (no planar cap use with an aligned plane)",
                ))
            };
            let nu0 = n_for(u0, u1)?;
            let nu1 = n_for(u1, u0)?;
            if nu0.x != -nu1.x || nu0.y != -nu1.y || nu0.z != -nu1.z {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "the two uses of a full-circle edge do not traverse oppositely",
                ));
            }
            full_normals.insert((u0.loop_idx, u0.pos), nu0);
            full_normals.insert((u1.loop_idx, u1.pos), nu1);
        }
    }

    // 1d. Planar face orientation: outer-loop Newell normal orientable and
    //     in agreement with yang's stated plane normal; rings wind
    //     opposite. Single full-circle loops were checked against the
    //     plane in 1c. Cylinder faces are validated by `validate_solid`'s
    //     curved orientation analysis after assembly.
    let mut face_normals: Vec<Option<UnitVector3>> = vec![None; yfaces.len()];
    for spec in &loops {
        let FaceSurf::Plane { normal } = &surfs[spec.face] else {
            continue;
        };
        if spec.cycle.len() == 1 {
            // Single full-circle loop: plane agreement established in 1c
            // (the derived normal exists only when |dot| ≈ 1).
            if spec.kind == LoopKind::Outer {
                face_normals[spec.face] = Some(UnitVector3 {
                    x: normal[0],
                    y: normal[1],
                    z: normal[2],
                });
            }
            continue;
        }
        // PR-KV9: ARC-MIDPOINT-AUGMENTED loop points (the same mechanism as
        // `validate::winding_points`, KV6a). A chord-only polygon mis-signs
        // the Newell normal when concave arcs dominate the loop — e.g. the
        // CRESCENT cap of a parallel cylinder×cylinder boolean, whose only
        // interior vertex can sit on the concave arc. Each arc contributes
        // its midpoint, which restores the bulge's signed area.
        let m = spec.cycle.len();
        let mut pts: Vec<Point3> = Vec::with_capacity(2 * m);
        for k in 0..m {
            let p0 = yverts[spec.cycle[k] as usize].point;
            pts.push(p0);
            if let EdgeKind::Arc {
                center,
                forward_normal,
                radius: _,
            } = spec.edges[k]
            {
                let p1 = yverts[spec.cycle[(k + 1) % m] as usize].point;
                if let Some(sweep) = geom::ccw_sweep(center, forward_normal, p0, p1) {
                    pts.push(geom::rotate_about_axis(
                        center,
                        forward_normal,
                        p0,
                        sweep / 2.0,
                    ));
                }
            }
            // PR-KV11: the EllipseArc analog (same role as validate.rs
            // `winding_points`) — a planar face whose boundary is dominated
            // by a concave ELLIPSE arc (the box-face bite of an oblique
            // cylinder) mis-signs the chord-only Newell normal exactly like
            // the KV9 crescent did for circle arcs.
            if let EdgeKind::EllipseArc {
                center,
                forward_normal,
                major_axis,
                major_radius,
                minor_radius,
            } = spec.edges[k]
            {
                let p1 = yverts[spec.cycle[(k + 1) % m] as usize].point;
                if let (Some(t0), Some(sweep)) = (
                    geom::ellipse_param(
                        center,
                        forward_normal,
                        major_axis,
                        major_radius,
                        minor_radius,
                        p0,
                    ),
                    geom::ellipse_ccw_sweep(
                        center,
                        forward_normal,
                        major_axis,
                        major_radius,
                        minor_radius,
                        p0,
                        p1,
                    ),
                ) {
                    pts.push(geom::ellipse_point_at(
                        center,
                        forward_normal,
                        major_axis,
                        major_radius,
                        minor_radius,
                        t0 + sweep / 2.0,
                    ));
                }
            }
            // KV16: the HyperbolaArc analog — parametric midpoint (the
            // arc dips toward the hyperbola center relative to its chord;
            // same winding-restoration role as the arc/ellipse midpoints).
            if let EdgeKind::HyperbolaArc {
                center,
                normal,
                major_axis,
                semi_transverse,
                semi_conjugate,
            } = spec.edges[k]
            {
                let p1 = yverts[spec.cycle[(k + 1) % m] as usize].point;
                if let (Some(t0), Some(t1)) = (
                    geom::hyperbola_param(center, normal, major_axis, semi_conjugate, p0),
                    geom::hyperbola_param(center, normal, major_axis, semi_conjugate, p1),
                ) {
                    pts.push(geom::hyperbola_point_at(
                        center,
                        normal,
                        major_axis,
                        semi_transverse,
                        semi_conjugate,
                        0.5 * (t0 + t1),
                    ));
                }
            }
        }
        match spec.kind {
            LoopKind::Outer => {
                let Some(nu) = geom::newell_unit(&pts) else {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face outer loop has a degenerate (zero) Newell normal",
                    ));
                };
                let dotn = nu.x * normal[0] + nu.y * normal[1] + nu.z * normal[2];
                if dotn < 1.0 - YANG_NORMAL_AGREEMENT_TOLERANCE {
                    if std::env::var("KV11_PROBE").is_ok() {
                        eprintln!(
                            "KV11_PROBE newell reject: face={} dotn={dotn:.6} plane_n={normal:?} \
                             newell=({:.6},{:.6},{:.6}) cycle_len={} kinds={:?} pts={:?}",
                            spec.face,
                            nu.x,
                            nu.y,
                            nu.z,
                            spec.cycle.len(),
                            spec.edges.iter().map(edge_kind_tag).collect::<Vec<_>>(),
                            pts
                        );
                    }
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face plane normal disagrees with its outer-loop Newell normal",
                    ));
                }
                face_normals[spec.face] = Some(nu);
            }
            LoopKind::Inner => {
                let nw = geom::newell(&pts);
                if nw[0] * normal[0] + nw[1] * normal[1] + nw[2] * normal[2] >= 0.0 {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face ring does not wind opposite to its outer loop",
                    ));
                }
            }
        }
    }

    // 1e. Connected components over faces via shared undirected edges —
    //     one shell per component.
    let component = face_components(yfaces.len(), &loops);
    let mut shells_faces: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (fi, &c) in component.iter().enumerate() {
        shells_faces.entry(c).or_default().push(fi);
    }

    // 1f. Per-component genus from the Euler–Poincaré formula
    //     (V − E + F − R = 2 − 2g for one closed shell).
    let mut shell_genus: BTreeMap<usize, u32> = BTreeMap::new();
    for (&rep, faces) in &shells_faces {
        let mut vset: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        // Key edges by (vertex pair, curve identity) so a LENS bigon's two arcs
        // count as TWO distinct edges (else E is undercounted and the
        // Euler–Poincaré parity check spuriously fails). Mirrors the manifold
        // pairing key above.
        let mut eset: std::collections::BTreeSet<(u32, u32, CurveKey)> =
            std::collections::BTreeSet::new();
        let mut rings = 0i64;
        for spec in loops.iter().filter(|s| component[s.face] == rep) {
            if spec.kind == LoopKind::Inner {
                rings += 1;
            }
            let m = spec.cycle.len();
            for k in 0..m {
                let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
                vset.insert(a);
                eset.insert((a.min(b), a.max(b), curve_key(&spec.edges[k])));
            }
        }
        let lhs = vset.len() as i64 - eset.len() as i64 + faces.len() as i64 - rings;
        if lhs % 2 != 0 || lhs > 2 {
            return Err(KernelV2Error::InvalidBooleanOutput(
                "output component's Euler characteristic is not genus-representable",
            ));
        }
        shell_genus.insert(rep, ((2 - lhs) / 2) as u32);
    }

    // ---- pass 2: assemble (validated input ⇒ infallible) -----------------
    // Vertices: referenced yang verts only, created in yang index order.
    let mut referenced = vec![false; yverts.len()];
    for spec in &loops {
        for &v in &spec.cycle {
            referenced[v as usize] = true;
        }
    }
    let mut vert_ids: Vec<Option<VertexId>> = vec![None; yverts.len()];
    for (i, yv) in yverts.iter().enumerate() {
        if referenced[i] {
            vert_ids[i] = Some(push_vertex(arena, yv.point));
        }
    }

    // Solid + shells (component order = ascending smallest face index).
    let solid_id = SolidId(arena.solids.len() as u32);
    arena.solids.push(Some(Solid { shells: Vec::new() }));
    let mut shell_of_face: Vec<ShellId> = vec![ShellId(0); yfaces.len()];
    for (&rep, faces) in &shells_faces {
        let shell_id = ShellId(arena.shells.len() as u32);
        arena.shells.push(Some(Shell {
            solid: solid_id,
            faces: Vec::new(),
            genus: shell_genus[&rep],
        }));
        if let Some(Some(solid)) = arena.solids.get_mut(solid_id.index()) {
            solid.shells.push(shell_id);
        }
        for &fi in faces {
            shell_of_face[fi] = shell_id;
        }
    }

    // Faces, loops, half-edges (faces in yang index order; loops outer
    // first then rings in yang order; half-edges in walk order).
    // Keyed by (vertex pair, curve identity) so a LENS bigon's two arcs twin
    // each within its own curve, not cross-twinned by shared endpoints (M8
    // disc∩disc crossing). Mirrors the manifold-pairing + Euler keys above.
    let mut twin_table: BTreeMap<(u32, u32, CurveKey), HalfEdgeId> = BTreeMap::new();
    let mut face_ids: Vec<Option<FaceId>> = vec![None; yfaces.len()];
    for (si, spec) in loops.iter().enumerate() {
        let fi = spec.face;
        let face_id = match face_ids[fi] {
            Some(id) => id,
            None => {
                let id = FaceId(arena.faces.len() as u32);
                let p0 = yverts[spec.cycle[0] as usize].point;
                let surface = match &surfs[fi] {
                    FaceSurf::Plane { .. } => Surface::Plane(Plane {
                        point: p0,
                        normal: face_normals[fi].expect("outer loop set the planar normal"),
                    }),
                    FaceSurf::Cylinder {
                        axis_point,
                        axis_dir,
                        radius,
                        reversed,
                    } => Surface::Cylinder {
                        axis_point: *axis_point,
                        axis_dir: UnitVector3 {
                            x: axis_dir[0],
                            y: axis_dir[1],
                            z: axis_dir[2],
                        },
                        radius: *radius,
                        reversed: *reversed,
                    },
                    FaceSurf::Cone {
                        apex,
                        axis_dir,
                        half_angle,
                        reversed,
                    } => Surface::Cone {
                        apex: *apex,
                        axis_dir: UnitVector3 {
                            x: axis_dir[0],
                            y: axis_dir[1],
                            z: axis_dir[2],
                        },
                        half_angle: *half_angle,
                        reversed: *reversed,
                    },
                    FaceSurf::Torus {
                        center,
                        axis_dir,
                        major_radius,
                        minor_radius,
                        reversed,
                    } => Surface::Torus {
                        center: *center,
                        axis_dir: UnitVector3 {
                            x: axis_dir[0],
                            y: axis_dir[1],
                            z: axis_dir[2],
                        },
                        major_radius: *major_radius,
                        minor_radius: *minor_radius,
                        reversed: *reversed,
                    },
                    FaceSurf::Sphere {
                        center,
                        radius,
                        reversed,
                    } => Surface::Sphere {
                        center: *center,
                        radius: *radius,
                        reversed: *reversed,
                    },
                };
                arena.faces.push(Some(Face {
                    surface: Some(surface),
                    outer_loop: LoopId(0), // patched below
                    inner_loops: Vec::new(),
                    shell: shell_of_face[fi],
                }));
                if let Some(Some(shell)) = arena.shells.get_mut(shell_of_face[fi].index()) {
                    shell.faces.push(id);
                }
                face_ids[fi] = Some(id);
                id
            }
        };

        // The loop slot and its half-edge cycle.
        let loop_id = LoopId(arena.loops.len() as u32);
        let m = spec.cycle.len();
        let he_base = arena.half_edges.len() as u32;
        for k in 0..m {
            let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
            let h = HalfEdgeId(he_base + k as u32);
            let key = (a.min(b), a.max(b), curve_key(&spec.edges[k]));
            // Twin pairing: the second visitor of an undirected (pair, curve)
            // links both directions (pass 1c proved exactly two consistent uses).
            let twin = match twin_table.get(&key) {
                Some(&other) => {
                    if let Some(Some(o)) = arena.half_edges.get_mut(other.index()) {
                        o.twin = h;
                    }
                    other
                }
                None => {
                    twin_table.insert(key, h);
                    h // placeholder; overwritten by the partner's visit
                }
            };
            let curve = match spec.edges[k] {
                EdgeKind::Seg => Curve::LineSegment,
                EdgeKind::Arc {
                    center,
                    forward_normal,
                    radius,
                } => Curve::Arc {
                    center,
                    normal: UnitVector3 {
                        x: forward_normal[0],
                        y: forward_normal[1],
                        z: forward_normal[2],
                    },
                    radius,
                },
                EdgeKind::EllipseArc {
                    center,
                    forward_normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                } => Curve::EllipseArc {
                    center,
                    normal: UnitVector3 {
                        x: forward_normal[0],
                        y: forward_normal[1],
                        z: forward_normal[2],
                    },
                    major_axis: UnitVector3 {
                        x: major_axis[0],
                        y: major_axis[1],
                        z: major_axis[2],
                    },
                    major_radius,
                    minor_radius,
                },
                EdgeKind::HyperbolaArc {
                    center,
                    normal,
                    major_axis,
                    semi_transverse,
                    semi_conjugate,
                } => Curve::HyperbolaArc {
                    center,
                    normal: UnitVector3 {
                        x: normal[0],
                        y: normal[1],
                        z: normal[2],
                    },
                    major_axis: UnitVector3 {
                        x: major_axis[0],
                        y: major_axis[1],
                        z: major_axis[2],
                    },
                    semi_transverse,
                    semi_conjugate,
                },
                EdgeKind::Full { center, radius, .. } => {
                    let nu = full_normals[&(si, k)];
                    Curve::Circle {
                        center,
                        normal: nu,
                        radius,
                    }
                }
                EdgeKind::SurfacePair { a, b } => Curve::SurfacePair { a, b },
            };
            let origin = vert_ids[a as usize].expect("referenced vertex was created");
            arena.half_edges.push(Some(HalfEdge {
                twin,
                next: HalfEdgeId(he_base + ((k + 1) % m) as u32),
                prev: HalfEdgeId(he_base + ((k + m - 1) % m) as u32),
                origin,
                loop_id,
                curve,
            }));
        }
        arena.loops.push(Some(Loop {
            face: face_id,
            boundary: LoopBoundary::Edges(HalfEdgeId(he_base)),
            kind: spec.kind,
        }));
        if let Some(Some(face)) = arena.faces.get_mut(face_id.index()) {
            match spec.kind {
                LoopKind::Outer => face.outer_loop = loop_id,
                LoopKind::Inner => face.inner_loops.push(loop_id),
            }
        }
    }

    // ---- pass 3: full production validation (defense in depth) -----------
    // Validate, then stamp persistent ids on the boolean's output faces
    // (KV13 F1). Per-face lineage attribution (F2) is recorded by `boolean_op`,
    // which has the operand→Pid maps; here we return the output face mapping.
    finalize_solid(arena, solid_id)?;
    Ok((solid_id, face_ids))
}

/// Classify one yang output edge into the KV5b vocabulary, applying the
/// named-curve walls and the minor-arc sense derivation (module docs).
/// `from`/`to` are the loop's TRAVERSAL endpoints (the stored
/// `(start, end)` or its reverse — see the loop walk in
/// [`from_yang_brep`]); the derived arc direction is for that traversal.
fn classify_edge(
    e: &yang_rs::BRepEdge,
    yverts: &[yang_rs::BRepVertex],
    from: u32,
    to: u32,
) -> Result<EdgeKind, KernelV2Error> {
    match e.curve {
        yang_rs::Curve::LineSegment => {
            if e.start == e.end {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "degenerate output edge (start == end)",
                ));
            }
            Ok(EdgeKind::Seg)
        }
        yang_rs::Curve::Circle {
            center,
            normal,
            radius,
        } => {
            let n = normal.as_array();
            if !(radius.is_finite() && radius > 0.0) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output circle edge with a non-positive radius",
                ));
            }
            if (norm3(n) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output circle edge normal is not unit-length",
                ));
            }
            if e.start == e.end {
                return Ok(EdgeKind::Full {
                    center,
                    normal: n,
                    radius,
                });
            }
            let ps = yverts[from as usize].point;
            let pe = yverts[to as usize].point;
            // Endpoints on the circle (f64-construction allowance — the
            // relocated vertices are computed in closed form).
            for p in [ps, pe] {
                let d = sub(p, center);
                let on_plane = dot3(d, n);
                let radial = (dot3(d, d) - on_plane * on_plane).max(0.0).sqrt();
                let band = cad_primitives::TAU_EVAL
                    * (1.0 + radius.max(p.x().abs().max(p.y().abs().max(p.z().abs()))));
                if (radial - radius).abs() > band || on_plane.abs() > band {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output arc endpoint does not lie on its circle",
                    ));
                }
            }
            let Some(sweep) = crate::geom::ccw_sweep(center, n, ps, pe) else {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output arc endpoint has no radial direction",
                ));
            };
            let pi = std::f64::consts::PI;
            if (sweep - pi).abs() <= ARC_MINOR_AMBIGUITY_BAND {
                return Err(KernelV2Error::UnsupportedBooleanOutputCurve {
                    curve: "near-half-circle arc (minor side ambiguous)",
                });
            }
            let forward_normal = if sweep < pi { n } else { [-n[0], -n[1], -n[2]] };
            Ok(EdgeKind::Arc {
                center,
                forward_normal,
                radius,
            })
        }
        yang_rs::Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            // PR-KV9: the exact oblique-section piece. Same minor-side
            // derivation as circles, in the PARAMETRIC frame: each
            // arrangement mesh edge subtends ≈ one Stage-1 facet, far below
            // π; a near-half sweep is rejected loudly rather than guessed.
            let n = normalize3_arr(normal.as_array());
            let m = normalize3_arr(major_axis.as_array());
            if !(major_radius.is_finite()
                && minor_radius.is_finite()
                && major_radius > 0.0
                && minor_radius > 0.0)
            {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output ellipse edge with non-positive radii",
                ));
            }
            if e.start == e.end {
                return Err(KernelV2Error::UnsupportedBooleanOutputCurve {
                    curve: "full Ellipse (no producer constructs closed ellipse edges)",
                });
            }
            let ps = yverts[from as usize].point;
            let pe = yverts[to as usize].point;
            // Endpoints on the ellipse (import band, in-plane residual
            // scaled by the minor radius, out-of-plane direct).
            for p in [ps, pe] {
                let d = sub(p, center);
                let out_of_plane = dot3(d, n);
                let w = [
                    n[1] * m[2] - n[2] * m[1],
                    n[2] * m[0] - n[0] * m[2],
                    n[0] * m[1] - n[1] * m[0],
                ];
                let u = dot3(d, m) / major_radius;
                let v = dot3(d, w) / minor_radius;
                let band = cad_primitives::TAU_EVAL
                    * (1.0 + major_radius.max(p.x().abs().max(p.y().abs().max(p.z().abs()))));
                if out_of_plane.abs() > band || (u.hypot(v) - 1.0).abs() * minor_radius > band {
                    if std::env::var("KV_ELLIPSE_PROBE").is_ok() {
                        eprintln!(
                            "KV_ELLIPSE_PROBE reject: from={from} to={to} start={} end={} \
                             p={p:?} center={center:?} n={n:?} m={m:?} \
                             a={major_radius:.17e} b={minor_radius:.17e} \
                             out_of_plane={out_of_plane:.3e} in_plane_resid={:.3e} band={band:.3e} \
                             u={u:.17} v={v:.17}",
                            e.start,
                            e.end,
                            (u.hypot(v) - 1.0).abs() * minor_radius,
                        );
                    }
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output ellipse-arc endpoint does not lie on its ellipse",
                    ));
                }
            }
            let Some(sweep) =
                crate::geom::ellipse_ccw_sweep(center, n, m, major_radius, minor_radius, ps, pe)
            else {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output ellipse-arc endpoint has no parametric direction",
                ));
            };
            let pi = std::f64::consts::PI;
            if (sweep - pi).abs() <= ARC_MINOR_AMBIGUITY_BAND {
                return Err(KernelV2Error::UnsupportedBooleanOutputCurve {
                    curve: "near-half-ellipse arc (minor side ambiguous)",
                });
            }
            let forward_normal = if sweep < pi { n } else { [-n[0], -n[1], -n[2]] };
            Ok(EdgeKind::EllipseArc {
                center,
                forward_normal,
                major_axis: m,
                major_radius,
                minor_radius,
            })
        }
        yang_rs::Curve::Parabola { .. } => {
            Err(KernelV2Error::UnsupportedBooleanOutputCurve { curve: "Parabola" })
        }
        // KV16 (spec `kv16_hyperbola_arc_vocabulary`): the axis-steep
        // plane∩cone section piece. Endpoint-determined traversal (the open
        // branch is injective — no minor-arc derivation, no directional
        // normal); each use copies the yang edge descriptor verbatim, so
        // twins come out BIT-IDENTICAL. K-checks: positive finite semi-axes,
        // unit frame, open (`start != end`), both endpoints ON the branch
        // (`u > 0`, first-order residual within the import band).
        yang_rs::Curve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => {
            if !(semi_transverse.is_finite()
                && semi_conjugate.is_finite()
                && semi_transverse > 0.0
                && semi_conjugate > 0.0)
            {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output hyperbola edge with non-positive semi-axes",
                ));
            }
            let n = normalize3_arr(normal.as_array());
            let m = normalize3_arr(major_axis.as_array());
            if e.start == e.end {
                return Err(KernelV2Error::UnsupportedBooleanOutputCurve {
                    curve: "closed hyperbola loop edge (the branch is unbounded — impossible)",
                });
            }
            let ps = yverts[from as usize].point;
            let pe = yverts[to as usize].point;
            let scale = semi_transverse.max(semi_conjugate);
            for p in [ps, pe] {
                let (in_plane, out_of_plane, u) = crate::geom::hyperbola_branch_residual(
                    center,
                    n,
                    m,
                    semi_transverse,
                    semi_conjugate,
                    p,
                );
                let mag = p.x().abs().max(p.y().abs()).max(p.z().abs());
                let band = cad_primitives::TAU_EVAL * (1.0 + scale.max(mag));
                if std::env::var("KV_HYPERBOLA_PROBE").is_ok() {
                    eprintln!(
                        "KV_HYPERBOLA_PROBE edge ({},{}) p=({:.6},{:.6},{:.6}) u={u:.3e} \
                         in_plane={in_plane:.3e} oop={out_of_plane:.3e} band={band:.3e} ok={}",
                        e.start,
                        e.end,
                        p.x(),
                        p.y(),
                        p.z(),
                        !(u <= 0.0 || in_plane > band || out_of_plane.abs() > band),
                    );
                }
                if u <= 0.0 || in_plane > band || out_of_plane.abs() > band {
                    if std::env::var("KV_HYPERBOLA_PROBE").is_ok() {
                        eprintln!(
                            "KV_HYPERBOLA_PROBE reject: from={from} to={to} start={} end={} \
                             p={p:?} center={center:?} n={n:?} m={m:?} \
                             a={semi_transverse:.17e} b={semi_conjugate:.17e} \
                             u={u:.3e} in_plane={in_plane:.3e} oop={out_of_plane:.3e} \
                             band={band:.3e}",
                            e.start, e.end,
                        );
                    }
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output hyperbola-arc endpoint does not lie on the +major-axis branch",
                    ));
                }
            }
            Ok(EdgeKind::HyperbolaArc {
                center,
                normal: n,
                major_axis: m,
                semi_transverse,
                semi_conjugate,
            })
        }
        // M5 (K1–K3): the procedural surface-pair curve. Operands are cylinders
        // and/or cones (the cyl×cyl and cone-pair producers); K2 rejects a
        // closed single-edge loop; K3 requires each endpoint on BOTH defining
        // surfaces within the import band (the per-point certification contract,
        // mirroring the circle/ellipse endpoint checks).
        yang_rs::Curve::SurfacePair { a, b } => {
            let pa = yang_surface_to_pair_surface(a)?;
            let pb = yang_surface_to_pair_surface(b)?;
            if e.start == e.end {
                return Err(KernelV2Error::UnsupportedBooleanOutputCurve {
                    curve: "closed surface-pair loop edge (no producer constructs them)",
                });
            }
            let ps = yverts[from as usize].point;
            let pe = yverts[to as usize].point;
            for p in [ps, pe] {
                let xa = [p.x(), p.y(), p.z()];
                let mag = p.x().abs().max(p.y().abs()).max(p.z().abs());
                for s in [&pa, &pb] {
                    let Some((residual, _)) = crate::geom::pair_surface_residual_gradient(s, xa)
                    else {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "surface-pair endpoint lies on a defining surface's axis",
                        ));
                    };
                    let band = cad_primitives::TAU_EVAL
                        * (1.0 + crate::geom::pair_surface_scale(s).max(mag));
                    if residual.abs() > band {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "output surface-pair endpoint does not lie on both surfaces",
                        ));
                    }
                }
            }
            Ok(EdgeKind::SurfacePair { a: pa, b: pb })
        }
    }
}

/// M5 (K1): map a yang output `Surface` to a kernel-v2 [`PairSurface`]. The
/// producers are `Cylinder` (cyl×cyl) and `Cone` (the cone-pair arms: cyl×cone,
/// cone×cone); a `Plane`/`Sphere`/`Torus` operand is a typed wall (no producer
/// emits them onto a surface-pair curve).
fn yang_surface_to_pair_surface(s: yang_rs::Surface) -> Result<PairSurface, KernelV2Error> {
    match s {
        yang_rs::Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            if !(radius.is_finite() && radius > 0.0) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "surface-pair cylinder operand has a non-positive radius",
                ));
            }
            let ad = normalize3_arr(axis_dir.as_array());
            Ok(PairSurface::Cylinder {
                axis_point,
                axis_dir: UnitVector3 {
                    x: ad[0],
                    y: ad[1],
                    z: ad[2],
                },
                radius,
            })
        }
        yang_rs::Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            // α ∈ (0, π/2): a line at α→0, a plane at α→π/2 — both reject.
            if !(half_angle.is_finite()
                && half_angle > 0.0
                && half_angle < std::f64::consts::FRAC_PI_2)
            {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "surface-pair cone operand has a half-angle outside (0, π/2)",
                ));
            }
            let ad = normalize3_arr(axis_dir.as_array());
            Ok(PairSurface::Cone {
                apex,
                axis_dir: UnitVector3 {
                    x: ad[0],
                    y: ad[1],
                    z: ad[2],
                },
                half_angle,
            })
        }
        yang_rs::Surface::Sphere { center, radius } => {
            // F10: general-position sphere×cyl / sphere×cone degree-4 pairs.
            if !(radius.is_finite() && radius > 0.0) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "surface-pair sphere operand has a non-positive radius",
                ));
            }
            Ok(PairSurface::Sphere { center, radius })
        }
        _ => Err(KernelV2Error::UnsupportedBooleanOutputCurve {
            curve: "surface-pair with a plane/torus operand (only cyl/cone/sphere are produced)",
        }),
    }
}

fn push_vertex(arena: &mut BrepArena, point: Point3) -> VertexId {
    let id = VertexId(arena.vertices.len() as u32);
    arena.vertices.push(Some(Vertex { point }));
    id
}

/// Union-find over yang face indices, joined by shared undirected edges.
/// Returns each face's component representative (the smallest face index
/// in its component).
fn face_components(num_faces: usize, loops: &[LoopSpec]) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..num_faces).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut edge_face: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for spec in loops {
        let m = spec.cycle.len();
        for k in 0..m {
            let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
            let key = (a.min(b), a.max(b));
            match edge_face.get(&key) {
                Some(&other) => {
                    let (ra, rb) = (find(&mut parent, spec.face), find(&mut parent, other));
                    let (lo, hi) = (ra.min(rb), ra.max(rb));
                    parent[hi] = lo;
                }
                None => {
                    edge_face.insert(key, spec.face);
                }
            }
        }
    }
    (0..num_faces).map(|f| find(&mut parent, f)).collect()
}

// ---------------------------------------------------------------------------
// boolean_op
// ---------------------------------------------------------------------------

/// Boolean operation on two kernel-v2 solids via the yang-rs pipeline with
/// the production native (cherchi-rs) backend.
///
/// The input solids stay live in the arena; the result is a NEW solid.
/// Error contract: [`KernelV2Error::UnsupportedCoplanar`] for coplanar
/// input face pairs (M8 boundary), [`KernelV2Error::EmptyBooleanResult`]
/// for an empty result, [`KernelV2Error::UnsupportedCurvedBoolean`] for
/// curved input shapes that cannot re-enter yang Stage 1 (partial patches
/// from a previous curved boolean), [`KernelV2Error::BooleanFailed`] for
/// any other yang-rs failure (loud Display text — including the
/// cylinder×cylinder Stage-3 SSI wall), plus the [`from_yang_brep`]
/// reassembly errors (named curve/surface walls).
pub fn boolean_op(
    arena: &mut BrepArena,
    a: SolidId,
    b: SolidId,
    op: BoolOp,
) -> Result<SolidId, KernelV2Error> {
    // Spec `kv2_multishell_boolean_operands` (KV7-F2): multi-shell operands
    // — disjoint lumps, interlocking lumps, AND internal voids — are
    // admitted. `to_yang_brep_indexed` emits every shell into one
    // multi-component BRep, and the pipeline is component- and
    // cavity-agnostic: the Cherchi 2022 §2.4/§5 in/out labeling is
    // ray-cast parity against each whole input mesh (a cavity-interior
    // point crosses two boundaries and classifies OUTSIDE), the Stage-4
    // Euler gate is per connected shell, and `from_yang_brep` assembles
    // multi-component outputs into multi-shell solids (production since
    // the disjoint-union path). The former PR-KV7 wall
    // (`UnsupportedMultiShellBoolean`) pinned a stale "reassembly cannot
    // rebuild voids" claim and is deleted.
    let (ya, a_faces) = to_yang_brep_indexed(arena, a)?;
    let (yb, b_faces) = to_yang_brep_indexed(arena, b)?;
    // Task #134 (spec `yang_disjoint_union_passthrough`): a UNION of
    // strictly AABB-disjoint operands (yang's own predicate — beyond the
    // YR24 weld band, conservative curved bounds) is the DISJOINT SUM.
    // Merge the shells at the ARENA level: every face/edge/curve of both
    // operands is preserved bit-for-bit (yang's passthrough output is
    // INPUT-convention topology, which `from_yang_brep` does not ingest,
    // so the merge happens here instead). Lineage: every operand face is
    // `Same` (identity — the faces ARE the output faces).
    if op == BoolOp::Union && yang_rs::union_operands_strictly_disjoint(&ya, &yb) {
        let mut shells = arena.solid(a)?.shells.clone();
        shells.extend(arena.solid(b)?.shells.iter().copied());
        let new_id = SolidId(arena.solids.len() as u32);
        arena.solids.push(Some(Solid {
            shells: shells.clone(),
        }));
        for sh in shells {
            arena.shell_mut(sh)?.solid = new_id;
        }
        {
            use crate::journal::{EvoKind, Evolution, OpTag};
            let modified: Vec<_> = a_faces
                .iter()
                .chain(b_faces.iter())
                .filter_map(|&fid| arena.face_pid(fid))
                .map(|pid| (pid, pid, EvoKind::Same))
                .collect();
            arena.journal.push(Evolution {
                op: OpTag::Boolean(op),
                generated: Vec::new(),
                modified,
                deleted: Vec::new(),
            });
        }
        return Ok(new_id);
    }
    if std::env::var_os("KV2_PLANE_TRACE").is_some() {
        for (tag, y) in [("A", &ya), ("B", &yb)] {
            for (i, f) in y.faces().iter().enumerate() {
                if let yang_rs::Surface::Plane { normal, d } = f.surface {
                    let n = normal.as_array();
                    eprintln!(
                        "[plane-trace] input {tag} face {i} n=({:.17e},{:.17e},{:.17e}) d={d:.17e}",
                        n[0], n[1], n[2]
                    );
                }
            }
        }
    }
    let Some(backend) = yang_rs::native_backend() else {
        // Unreachable since cherchi-rs M7c (the backend is always available),
        // kept as a loud arm rather than an unwrap (P9, no-panic rule).
        return Err(KernelV2Error::BooleanFailed(
            "yang-rs native backend unavailable".to_string(),
        ));
    };
    let out = yang_rs::boolean(&ya, &yb, op, &backend).map_err(map_yang_error)?;
    if std::env::var_os("KV2_PLANE_TRACE").is_some() {
        for (i, f) in out.faces().iter().enumerate() {
            if let yang_rs::Surface::Plane { normal, d } = f.surface {
                let n = normal.as_array();
                eprintln!(
                    "[plane-trace] OUTPUT face {i} n=({:.17e},{:.17e},{:.17e}) d={d:.17e}",
                    n[0], n[1], n[2]
                );
            }
        }
    }
    // KV9-F3 diagnosis probe (read-only, env-gated): census near-twin
    // vertex pairs in yang's OUTPUT B-Rep, with incident-edge context —
    // localizes output-identity defects to the yang side vs the assembler.
    if std::env::var_os("KV2_OUT_TWIN_PROBE").is_some() {
        let vs: Vec<_> = out.vertices().iter().map(|v| v.point).collect();
        let scale = vs
            .iter()
            .flat_map(|p| p.as_array())
            .fold(1.0_f64, |m, c| m.max(c.abs()));
        let band = 1.0e-9 * scale;
        for i in 0..vs.len() {
            for j in (i + 1)..vs.len() {
                let (p, q) = (vs[i].as_array(), vs[j].as_array());
                let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
                if d2 <= band * band {
                    eprintln!(
                        "[out-twin-probe] verts {i}/{j} dist={:e}\n  {i}: ({},{},{})\n  {j}: ({},{},{})",
                        d2.sqrt(),
                        p[0],
                        p[1],
                        p[2],
                        q[0],
                        q[1],
                        q[2]
                    );
                    for (ei, e) in out.edges().iter().enumerate() {
                        if e.start == i as u32
                            || e.end == i as u32
                            || e.start == j as u32
                            || e.end == j as u32
                        {
                            eprintln!(
                                "    edge {ei}: {} -> {} curve {:?}",
                                e.start,
                                e.end,
                                std::mem::discriminant(&e.curve)
                            );
                        }
                    }
                }
            }
        }
    }
    let (out_solid, out_face_ids) = from_yang_brep_indexed(arena, &out)?;
    // F1 (design review 2026-07-12): PRODUCTION planarity gate for the
    // assembled boolean output. The debug-only tripwire in `validate_solid`
    // rests on "planar by construction", which is false for yang re-entry —
    // the F0064/R0051 class shipped planar faces with off-plane loop
    // vertices that evaded the (averaged) Newell orientation checks. Loud
    // typed reject at the boundary; never a snap or repair (P9).
    crate::validate::validate_boolean_output_planarity(arena, out_solid)?;
    // KV13 F2: record the boolean's per-face lineage in the journal.
    record_boolean_evolution(arena, op, &out, &out_face_ids, &a_faces, &b_faces);
    Ok(out_solid)
}

/// KV13 F2: append the boolean's [`Evolution`] to the arena journal. Each
/// output face descends (via yang's per-face attribution) from an operand
/// face → a `modified` edge `(operand_pid → output_pid)`; the first output
/// face from a given operand face is `Same`, additional ones are `Split`.
/// Operand faces that produced no output are `deleted`. An output face with no
/// resolvable operand lineage is `generated` (defensive — yang attributes
/// every patch, so this is normally empty). Infallible: a missing Pid simply
/// drops that edge (no false lineage).
fn record_boolean_evolution(
    arena: &mut BrepArena,
    op: BoolOp,
    out: &yang_rs::BRep,
    out_face_ids: &[Option<FaceId>],
    a_faces: &[FaceId],
    b_faces: &[FaceId],
) {
    use crate::arena::Pid;
    use crate::journal::{EvoKind, Evolution, OpTag};
    use std::collections::BTreeSet;

    let attr = out.face_attribution();
    let mut generated: Vec<Pid> = Vec::new();
    let mut modified: Vec<(Pid, Pid, EvoKind)> = Vec::new();
    let mut claimed: BTreeSet<Pid> = BTreeSet::new();
    let mut sourced: BTreeSet<Pid> = BTreeSet::new();
    for (yidx, out_fid) in out_face_ids.iter().enumerate() {
        let Some(out_fid) = out_fid else { continue };
        let Some(out_pid) = arena.face_pid(*out_fid) else {
            continue;
        };
        let operand_pid = attr
            .get(yidx)
            .and_then(|a| {
                let faces = match a.input {
                    yang_rs::InputId::A => a_faces,
                    yang_rs::InputId::B => b_faces,
                };
                faces.get(a.face as usize).copied()
            })
            .and_then(|fid| arena.face_pid(fid));
        match operand_pid {
            Some(opid) => {
                sourced.insert(opid);
                let kind = if claimed.insert(opid) {
                    EvoKind::Same
                } else {
                    EvoKind::Split
                };
                modified.push((opid, out_pid, kind));
            }
            None => generated.push(out_pid),
        }
    }
    let mut deleted: Vec<Pid> = a_faces
        .iter()
        .chain(b_faces.iter())
        .filter_map(|&fid| arena.face_pid(fid))
        .filter(|pid| !sourced.contains(pid))
        .collect();
    deleted.sort_unstable();
    deleted.dedup();
    arena.journal.push(Evolution {
        op: OpTag::Boolean(op),
        generated,
        modified,
        deleted,
    });
}

/// Cluster shells by AABB overlap (closed intervals — touching boxes also
/// cluster; conservative). Returned in deterministic (union-find root)
/// order. Shared by [`split_solid_into_bodies`] (body decomposition) and
/// `boolean_op`'s operand admission (spec `kv2_multishell_boolean_operands`):
/// a cluster of ≥2 shells is either a lump with an internal void (nested
/// AABBs) or interlocking lumps — both stay behind the typed multi-shell
/// wall, while singleton clusters are freely re-enterable disjoint lumps.
fn shell_aabb_clusters(
    arena: &BrepArena,
    shells: &[ShellId],
) -> Result<Vec<Vec<ShellId>>, KernelV2Error> {
    // Per-shell AABB over its faces' loop vertices.
    let mut boxes: Vec<([f64; 3], [f64; 3])> = Vec::with_capacity(shells.len());
    for &sh in shells {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            let mut loops = vec![face.outer_loop];
            loops.extend(face.inner_loops.iter().copied());
            for lid in loops {
                for h in arena.loop_half_edges(lid)? {
                    let v = arena.half_edge(h)?.origin;
                    let p = arena.vertex(v)?.point.as_array();
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
            }
        }
        boxes.push((lo, hi));
    }

    // Union-find: cluster shells whose AABBs overlap (closed intervals, so
    // touching boxes also cluster — conservative).
    let n = shells.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let overlap = |a: &([f64; 3], [f64; 3]), b: &([f64; 3], [f64; 3])| {
        (0..3).all(|k| a.1[k] >= b.0[k] && b.1[k] >= a.0[k])
    };
    for i in 0..n {
        for j in (i + 1)..n {
            if overlap(&boxes[i], &boxes[j]) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    // Group shells by cluster root (BTreeMap → deterministic order).
    let mut groups: BTreeMap<usize, Vec<ShellId>> = BTreeMap::new();
    for (i, &sh) in shells.iter().enumerate() {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(sh);
    }
    Ok(groups.into_values().collect())
}

/// Decompose a solid into separate body solids by grouping its shells into
/// spatially-disjoint clusters. Two shells whose axis-aligned bounding boxes
/// overlap go in the same cluster — so a void shell (its AABB nested inside the
/// lump's) stays with its lump, while genuinely disjoint lumps (e.g. a union of
/// far-apart bosses) separate. Returns the original `solid` first (rewritten to
/// hold the first cluster's shells), then one fresh solid per additional
/// cluster.
///
/// Returns `vec![solid]` unchanged when there is ≤1 shell or every shell
/// clusters together (one body, possibly with internal voids). AABB-overlap
/// clustering is deliberately conservative: it never over-splits (two truly
/// disjoint lumps with overlapping boxes stay one body) — under-splitting is a
/// benign display artifact, over-splitting would invent bodies.
pub fn split_solid_into_bodies(
    arena: &mut BrepArena,
    solid: SolidId,
) -> Result<Vec<SolidId>, KernelV2Error> {
    let shells: Vec<ShellId> = arena.solid(solid)?.shells.clone();
    if shells.len() <= 1 {
        return Ok(vec![solid]);
    }

    let groups = shell_aabb_clusters(arena, &shells)?;
    if groups.len() <= 1 {
        return Ok(vec![solid]);
    }

    // First cluster stays in the original solid; the rest get fresh solids.
    let mut result = Vec::with_capacity(groups.len());
    let mut clusters = groups.into_iter();
    let first = clusters.next().expect("groups.len() > 1");
    arena.solid_mut(solid)?.shells = first;
    result.push(solid);
    for cluster in clusters {
        let new_id = SolidId(arena.solids.len() as u32);
        arena.solids.push(Some(Solid {
            shells: cluster.clone(),
        }));
        for sh in cluster {
            arena.shell_mut(sh)?.solid = new_id;
        }
        result.push(new_id);
    }
    Ok(result)
}

/// Map a yang-rs pipeline error to the kernel-v2 typed error contract.
///
/// The structurally-recognized cases are the M8 boundary:
/// - yang-rs's own Stage-1 NEAR-coplanar input gate (PR-YR24),
///   `YangError::CoplanarFacesUnsupported { .. }` — coplanar within the
///   sub-model-resolution band, per Yang 2025 §4.5.5 / roadmap M8;
/// - the cherchi-rs arrangement's bit-EXACT coplanar-pair deferral, nested
///   as `YangError::MeshBooleanFailed(NativeBooleanError::Arrangement(
///   ArrangementError::CoplanarPairDeferred { .. }))`.
///
/// Both map to the typed [`KernelV2Error::UnsupportedCoplanar`]. Everything
/// else is a loud [`KernelV2Error::BooleanFailed`] carrying the full
/// Display text.
fn map_yang_error(e: yang_rs::YangError) -> KernelV2Error {
    if let yang_rs::YangError::CoplanarFacesUnsupported { .. } = &e {
        return KernelV2Error::UnsupportedCoplanar;
    }
    if let yang_rs::YangError::MeshBooleanFailed(src) = &e {
        if let Some(yang_rs::NativeBooleanError::Arrangement(
            yang_rs::ArrangementError::CoplanarPairDeferred { .. },
        )) = src.downcast_ref::<yang_rs::NativeBooleanError>()
        {
            return KernelV2Error::UnsupportedCoplanar;
        }
    }
    KernelV2Error::BooleanFailed(e.to_string())
}
