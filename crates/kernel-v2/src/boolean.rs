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
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use crate::geom;
use crate::validate::validate_solid;
use cad_primitives::{BoolOp, Point3, Vector3};

/// Tolerance on `1 − dot(Newell(loop), yang_plane_normal)` for the
/// cross-check that a yang output face's stated plane agrees with its
/// boundary walk. Same bar as `validate::NORMAL_AGREEMENT_TOLERANCE` —
/// both vectors are unit-length; only normalization rounding is absorbed.
const YANG_NORMAL_AGREEMENT_TOLERANCE: f64 = 1e-9;

/// Sweep band (radians) around π inside which an arc's minor side is
/// declared ambiguous and rejected (`UnsupportedBooleanOutputCurve`)
/// rather than guessed. Arrangement mesh edges subtend ≈ one Stage-1 facet
/// (2π/8 .. 2π/16 on the surveyed corpus), orders of magnitude below π, so
/// the band never fires on in-scope geometry — it exists to make the
/// minor-arc assumption a CHECKED precondition, not a silent one.
pub const ARC_MINOR_AMBIGUITY_BAND: f64 = 1e-6;

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
    let mut vid_map: BTreeMap<VertexId, u32> = BTreeMap::new();
    let mut yverts: Vec<yang_rs::BRepVertex> = Vec::new();
    let mut yedges: Vec<yang_rs::BRepEdge> = Vec::new();
    let mut yfaces: Vec<yang_rs::BRepFace> = Vec::new();
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
                            // PR-KV9: ellipse-arc boundaries (oblique
                            // sections) have no yang Stage-1 INPUT
                            // tessellation yet — boolean outputs carrying
                            // them are terminal for chaining (typed wall).
                            match he.curve {
                                // PR-KV9: ellipse-arc boundaries (oblique
                                // sections) have no yang Stage-1 INPUT
                                // tessellation yet — boolean outputs carrying
                                // them are terminal for chaining (typed wall).
                                Curve::EllipseArc { .. } => {
                                    return Err(KernelV2Error::UnsupportedCurvedBoolean {
                                        face: f,
                                    });
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
                Some(Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                    reversed,
                }) => {
                    // Two convertible shapes (PR-KV6b-2):
                    // - CANONICAL tube: [rim, seam, rim, seam], two closed
                    //   Circle rims, the segs a seam twin PAIR;
                    // - PARTIAL revolve wall: [seg, arc, seg, arc], two
                    //   sweep Arcs + two distinct ruling segments.
                    // `reversed` passes through as yang BRepFace.reversed
                    // (KV6b-1 Stage-1 orients cavity walls inward).
                    // Anything else — boolean-OUTPUT patches whose curved
                    // boundaries are chord polylines, holed laterals —
                    // cannot re-enter yang Stage 1 (the remaining wall).
                    if !face.inner_loops.is_empty() {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                    }
                    let mut hes = arena.loop_half_edges(face.outer_loop)?;
                    if hes.len() != 4 {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                    }
                    if matches!(arena.half_edge(hes[0])?.curve, Curve::LineSegment) {
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
                    if !(canonical || partial) {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                    }
                    // Canonical: the two segments must be the seam twin pair.
                    // Partial: they are two DISTINCT rulings (each twins with
                    // a cap edge instead).
                    if canonical && arena.half_edge(hes[1])?.twin != hes[3] {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                    }

                    let mut loop_indices = Vec::with_capacity(4);
                    for &h in &hes {
                        let he = arena.half_edge(h)?;
                        let key = h.min(he.twin);
                        let idx = match shared_edges.get(&key) {
                            Some(&idx) => idx,
                            None => {
                                let idx = yedges.len() as u32;
                                match he.curve {
                                    // PR-KV9: no yang INPUT vocabulary for
                                    // ellipse arcs — typed re-entry wall.
                                    Curve::EllipseArc { .. } => {
                                        return Err(KernelV2Error::UnsupportedCurvedBoolean {
                                            face: f,
                                        });
                                    }
                                    Curve::Arc {
                                        center,
                                        radius,
                                        normal,
                                    } => {
                                        // Shared directional arc: endpoints +
                                        // normal from THIS half-edge (the
                                        // yang input-arc convention; the twin
                                        // denotes the same point set).
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
                                        // Created from the lateral side: the
                                        // shared edge carries the CAP-outward
                                        // normal = the negation of the
                                        // lateral half-edge's directional
                                        // normal (twins are exact negations).
                                        let nu = neg_unit(normal);
                                        let anchor = map_vertex(
                                            he.origin,
                                            &mut vid_map,
                                            &mut yverts,
                                            arena,
                                        )?;
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
                                            curve: yang_rs::Curve::LineSegment,
                                        });
                                    }
                                }
                                shared_edges.insert(key, idx);
                                idx
                            }
                        };
                        loop_indices.push(idx);
                    }

                    yfaces.push(yang_rs::BRepFace {
                        surface: yang_rs::Surface::Cylinder {
                            axis_point,
                            axis_dir: Vector3::new(axis_dir.x, axis_dir.y, axis_dir.z),
                            radius,
                        },
                        outer_loop: loop_indices,
                        inner_loops: Vec::new(),
                        reversed,
                    });
                }
                None => return Err(KernelV2Error::FaceWithoutSurface { face: f }),
            }
        }
    }

    canonicalize_sibling_planes(&mut yfaces);

    yang_rs::BRep::new(yverts, yedges, yfaces).map_err(|e| {
        KernelV2Error::BooleanFailed(format!("yang-rs rejected the converted input B-Rep: {e}"))
    })
}

/// PR-KV10 (M8 slice d): collapse rounding-noise plane bits across
/// same-plane sibling faces.
///
/// A boolean output legitimately carries several faces that are disjoint
/// fragments of ONE plane (a side plane split in two by a crossing union).
/// The arena stores each fragment's plane in point-normal form with a
/// per-fragment Newell normal, and `d` above is derived from each face's
/// own first loop vertex — so on oblique geometry the fragments' emitted
/// `(normal, d)` differ at the ~1e-16 rounding level. yang's intra-solid
/// near-coplanar gate treats BIT-identical planes as benign (one plane
/// split into several faces) and walls anything else, so without this pass
/// a fragment-carrying output cannot enter ANY further boolean (the
/// F0016-class corpus residue).
///
/// Rule: planar faces whose unit normals agree component-wise within
/// `TAU_WORK` and whose offsets agree within the scale-relative
/// `TAU_WORK·(1+|d|)` band adopt the FIRST such face's exact bits
/// (deterministic; greedy in face order — the band is ~4 orders below the
/// near-coplanar DETECTION band and ~6 below `MIN_FEATURE_SIZE`, so only
/// rounding noise collapses and cluster drift is impossible). Opposite-
/// orientation coplanar faces never match (component-wise test) — sense is
/// preserved. Vertex coordinates are untouched: the residual between a
/// loop vertex and the adopted plane stays in the same scale-relative
/// rounding class the stored plane already had.
fn canonicalize_sibling_planes(yfaces: &mut [yang_rs::BRepFace]) {
    // Representatives: (normal, d) of the first face seen in each cluster.
    let mut reps: Vec<([f64; 3], f64)> = Vec::new();
    for f in yfaces.iter_mut() {
        let yang_rs::Surface::Plane { normal, d } = &mut f.surface else {
            continue;
        };
        let n = normal.as_array();
        if !(n.iter().all(|c| c.is_finite()) && d.is_finite()) {
            continue;
        }
        let eps_n = cad_primitives::TAU_WORK;
        match reps.iter().find(|(rn, rd)| {
            (0..3).all(|k| (n[k] - rn[k]).abs() <= eps_n)
                && (*d - rd).abs() <= cad_primitives::TAU_WORK * (1.0 + rd.abs())
        }) {
            Some(&(rn, rd)) => {
                *normal = Vector3::new(rn[0], rn[1], rn[2]);
                *d = rd;
            }
            None => reps.push((n, *d)),
        }
    }
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
    }
}

/// The curve vocabulary of one directed yang loop edge, KV5b-classified.
#[derive(Clone, Copy, PartialEq)]
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
    for (i, f) in yfaces.iter().enumerate() {
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
            _ => return Err(KernelV2Error::UnsupportedBooleanOutputSurface { face: i }),
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
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output loop with fewer than 3 edges and no full-circle edge",
                ));
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
    let mut pair_uses: BTreeMap<(u32, u32), Vec<EdgeUse>> = BTreeMap::new();
    for (si, spec) in loops.iter().enumerate() {
        let m = spec.cycle.len();
        for k in 0..m {
            let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
            let key = (a.min(b), a.max(b));
            pair_uses.entry(key).or_default().push(EdgeUse {
                loop_idx: si,
                pos: k,
                forward: a < b,
            });
        }
    }
    // Per (loop, pos) directional normal for full-circle uses.
    let mut full_normals: BTreeMap<(usize, usize), UnitVector3> = BTreeMap::new();
    for (&(a, b), uses) in &pair_uses {
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
                if matches!(surfs[loops[u.loop_idx].face], FaceSurf::Cylinder { .. }) {
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
        let mut eset: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
        let mut rings = 0i64;
        for spec in loops.iter().filter(|s| component[s.face] == rep) {
            if spec.kind == LoopKind::Inner {
                rings += 1;
            }
            let m = spec.cycle.len();
            for k in 0..m {
                let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
                vset.insert(a);
                eset.insert((a.min(b), a.max(b)));
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
    let mut twin_table: BTreeMap<(u32, u32), HalfEdgeId> = BTreeMap::new();
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
            let key = (a.min(b), a.max(b));
            // Twin pairing: the second visitor of an undirected pair links
            // both directions (pass 1c proved exactly two consistent uses).
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
                EdgeKind::Full { center, radius, .. } => {
                    let nu = full_normals[&(si, k)];
                    Curve::Circle {
                        center,
                        normal: nu,
                        radius,
                    }
                }
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
    validate_solid(arena, solid_id)?;
    Ok(solid_id)
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
                let band = 1e-9 * (1.0 + radius.max(p.x().abs().max(p.y().abs().max(p.z().abs()))));
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
                let band =
                    1e-9 * (1.0 + major_radius.max(p.x().abs().max(p.y().abs().max(p.z().abs()))));
                if out_of_plane.abs() > band || (u.hypot(v) - 1.0).abs() * minor_radius > band {
                    if std::env::var("KV_ELLIPSE_PROBE").is_ok() {
                        eprintln!(
                            "KV_ELLIPSE_PROBE reject: p={p:?} center={center:?} n={n:?} m={m:?} \
                             a={major_radius:.17e} b={minor_radius:.17e} \
                             out_of_plane={out_of_plane:.3e} in_plane_resid={:.3e} band={band:.3e} \
                             u={u:.17} v={v:.17}",
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
        yang_rs::Curve::Hyperbola { .. } => {
            Err(KernelV2Error::UnsupportedBooleanOutputCurve { curve: "Hyperbola" })
        }
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
    // PR-KV7: yang's input has no shell structure and its reassembly cannot
    // rebuild voids — wall multi-shell operands loudly (typed).
    for s in [a, b] {
        let shells = arena.solid(s)?.shells.len();
        if shells > 1 {
            return Err(KernelV2Error::UnsupportedMultiShellBoolean { shells });
        }
    }
    let ya = to_yang_brep(arena, a)?;
    let yb = to_yang_brep(arena, b)?;
    let Some(backend) = yang_rs::native_backend() else {
        // Unreachable since cherchi-rs M7c (the backend is always available),
        // kept as a loud arm rather than an unwrap (P9, no-panic rule).
        return Err(KernelV2Error::BooleanFailed(
            "yang-rs native backend unavailable".to_string(),
        ));
    };
    let out = yang_rs::boolean(&ya, &yb, op, &backend).map_err(map_yang_error)?;
    from_yang_brep(arena, &out)
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

    // Per-shell AABB over its faces' loop vertices.
    let mut boxes: Vec<([f64; 3], [f64; 3])> = Vec::with_capacity(shells.len());
    for &sh in &shells {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        let faces = arena.shell(sh)?.faces.clone();
        for f in faces {
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
    if groups.len() <= 1 {
        return Ok(vec![solid]);
    }

    // First cluster stays in the original solid; the rest get fresh solids.
    let mut result = Vec::with_capacity(groups.len());
    let mut clusters = groups.into_values();
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
