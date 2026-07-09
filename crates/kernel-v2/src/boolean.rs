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
                            // PR-KV9: ellipse-arc boundaries (oblique
                            // sections) have no yang Stage-1 INPUT
                            // tessellation yet — boolean outputs carrying
                            // them are terminal for chaining (typed wall).
                            match he.curve {
                                // PR-KV9: ellipse-arc boundaries (oblique
                                // sections) have no yang Stage-1 INPUT
                                // tessellation yet — boolean outputs carrying
                                // them are terminal for chaining (typed wall).
                                // M5 K11: surface-pair (degree-4) boundaries
                                // are the same wall — chained booleans on
                                // quartic-bounded bodies are a later
                                // milestone.
                                Curve::EllipseArc { .. } | Curve::SurfacePair { .. } => {
                                    return Err(KernelV2Error::UnsupportedCurvedBoolean {
                                        face: f,
                                        reason: "planar-loop degree-4 boundary (ellipse/surface-pair edge)",
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
                            // PR-KV9 / M5 K11: no yang INPUT vocabulary for
                            // ellipse arcs or surface-pair (degree-4) edges.
                            Curve::EllipseArc { .. } | Curve::SurfacePair { .. } => {
                                return Err(KernelV2Error::UnsupportedCurvedBoolean {
                                        face: f,
                                        reason: "curved lateral degree-4 boundary (ellipse/surface-pair edge)",
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
                            // KV14 Slice F: a boolean-result torus lateral re-enters
                            // via the UV-CDT path (`yang tessellate_torus_band` →
                            // `tessellate_torus_patch`) as a POLOIDAL PERIODIC BAND —
                            // two full profile boundaries (outer + ONE inner) bounding
                            // the tube. A full-circle rim (`Curve::Circle`) is the
                            // canonical structured torus (no CDT re-entry), and a HOLED
                            // band (a window in the tube → ≥2 inner loops) is out of the
                            // patch tessellator's scope — both stay the typed wall.
                            Some(Surface::Torus {
                                center,
                                axis_dir,
                                major_radius,
                                minor_radius,
                                ..
                            }) if !curved_full_rim && face.inner_loops.len() == 1 => {
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
                                    "curved lateral is a canonical or holed torus (full-circle \
                                     rim / windowed band — Slice F sub-slice)"
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
                    if !(canonical || partial || torus) {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean {
                            face: f,
                            reason: "curved lateral non-{canonical,partial,torus} edge pattern",
                        });
                    }
                    // Canonical: the two segments must be the seam twin pair.
                    // Partial: two DISTINCT rulings (each twins with a cap edge).
                    // Torus: the two seam ARCS (positions 1, 3) are the twin pair.
                    if (canonical || torus) && arena.half_edge(hes[1])?.twin != hes[3] {
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
/// `TAU_WORK·(1+|d|)` band — under EITHER sign `s ∈ {+1, −1}` applied to the
/// representative (spec `m8_intra_opposite_plane_canonicalization` B1/B2) —
/// adopt the FIRST such face's exact bits times `s` (deterministic; greedy
/// in face order — the band is ~4 orders below the near-coplanar DETECTION
/// band and ~6 below `MIN_FEATURE_SIZE`, so only rounding noise collapses
/// and cluster drift is impossible). The sign keeps each face's outward
/// sense (I1: `dot(n_before, n_after) > 0`) while an opposite-orientation
/// step pair (a chained output whose lower-step top and overhang bottom
/// share one geometric plane) ends up with EXACTLY negated plane bits (I2)
/// — the form yang-rs's intra-solid gate treats as benign. Vertex
/// coordinates are untouched: the residual between a loop vertex and the
/// adopted plane stays in the same scale-relative rounding class the
/// stored plane already had.
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
        let dv = *d;
        let matched = reps.iter().find_map(|&(rn, rd)| {
            [1.0f64, -1.0f64]
                .into_iter()
                .find(|s| {
                    (0..3).all(|k| (n[k] - s * rn[k]).abs() <= eps_n)
                        && (dv - s * rd).abs() <= cad_primitives::TAU_WORK * (1.0 + rd.abs())
                })
                .map(|s| (rn, rd, s))
        });
        match matched {
            Some((rn, rd, s)) => {
                *normal = Vector3::new(s * rn[0], s * rn[1], s * rn[2]);
                *d = s * rd;
            }
            None => reps.push((n, dv)),
        }
    }
}

/// Spec `m8_shared_boundary_identity` — chained-output VERTEX canonicalization
/// (the KV10 completion: planes above, vertices here).
///
/// Each boolean-output vertex carries independent ~1e-16 rounding, so a
/// re-imported face loop is femto-crooked relative to its (canonicalized)
/// planes: intended-straight edges are not exactly straight, intended-
/// plane-constant coordinates are not bit-constant. The Stage-0 exact
/// overlay faithfully arranges that crookedness into femto-wide sweep
/// slabs, needle cells (`RoundingCollapse`), femto-twin split vertices
/// (ear-clip stalls), and near-coincident cross-input vertices inside
/// cherchi (`LabelMismatch`). Re-deriving each vertex from its incident
/// canonical planes eliminates the disease at the producer boundary.
///
/// Rules (spec §3): a vertex whose incident faces are ALL planar is
/// re-derived from its distinct incident planes ((n,d) and exactly
/// (−n,−d) are ONE plane): ≥3 independent → exact rational 3-plane solve
/// (B1); exactly 2 (or no independent triple, B6) → exact projection onto
/// the 2-plane intersection line (B2); <2 → unchanged (B3). The result is
/// rounded to f64 ONCE and adopted only when it moves the vertex by at
/// most the KV10-scale band `TAU_WORK·(1+|coord|)` per component (B4 —
/// a vertex genuinely off its planes' intersection is never forced).
/// Any curved incident face vetoes the vertex (B5 — rim/arc endpoints
/// must stay exactly on their curves). Deterministic: faces and planes in
/// push order; first independent triple / first non-degenerate pair wins.
fn canonicalize_vertices_to_planes(
    yverts: &mut [yang_rs::BRepVertex],
    yedges: &[yang_rs::BRepEdge],
    yfaces: &[yang_rs::BRepFace],
) {
    use dashu::rational::RBig;

    // Exact f64 → RBig (f64 is exactly representable; non-finite handled
    // by the incidence filter below).
    fn rat(x: f64) -> RBig {
        let fb: dashu::float::FBig = dashu::float::FBig::try_from(x).expect("finite");
        RBig::try_from(fb).expect("finite")
    }

    // ── incidence: vertex → distinct incident canonical planes ──────────
    // Plane identity key: the raw (n, d) 4-tuple sign-normalized so (n, d)
    // and exactly (−n, −d) collapse to one geometric plane. Sign flip of an
    // f64 is exact, so the key is exact.
    let mut planes: Vec<Vec<([f64; 3], f64)>> = vec![Vec::new(); yverts.len()];
    let mut plane_keys: Vec<Vec<[u64; 4]>> = vec![Vec::new(); yverts.len()];
    let mut curved: Vec<bool> = vec![false; yverts.len()];
    for f in yfaces {
        let planar = match f.surface {
            yang_rs::Surface::Plane { normal, d } => {
                let n = normal.as_array();
                (n.iter().all(|c| c.is_finite()) && d.is_finite()).then_some((n, d))
            }
            _ => None,
        };
        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            for &e in lp {
                let Some(edge) = yedges.get(e as usize) else {
                    continue;
                };
                for vi in [edge.start as usize, edge.end as usize] {
                    if vi >= yverts.len() {
                        continue;
                    }
                    match planar {
                        None => curved[vi] = true,
                        Some((n, d)) => {
                            let raw = [n[0], n[1], n[2], d];
                            // Sign-normalize: flip so the first nonzero
                            // component is positive (0.0/−0.0 both count
                            // as zero — skip them for the sign choice).
                            let s = raw.iter().find(|c| **c != 0.0).map_or(1.0, |c| {
                                if *c < 0.0 {
                                    -1.0
                                } else {
                                    1.0
                                }
                            });
                            let key = [
                                (s * raw[0]).to_bits(),
                                (s * raw[1]).to_bits(),
                                (s * raw[2]).to_bits(),
                                (s * raw[3]).to_bits(),
                            ];
                            if !plane_keys[vi].contains(&key) {
                                plane_keys[vi].push(key);
                                planes[vi].push((n, d));
                            }
                        }
                    }
                }
            }
        }
    }

    // ── per-vertex re-derivation ─────────────────────────────────────────
    // B6 conditioning floor: skip a plane triple whose exact determinant
    // satisfies det² ≤ FLOOR²·(|n1|²·|n2|²·|n3|²) — sub-floor dihedrals
    // amplify femto residuals into large motion the band guard would
    // reject anyway; the floor keeps the search deterministic and cheap.
    const DET_FLOOR: f64 = 1.0e-9;

    for (vi, v) in yverts.iter_mut().enumerate() {
        if curved[vi] || planes[vi].len() < 2 {
            continue; // B5 / B3
        }
        let pls: Vec<([RBig; 3], RBig)> = planes[vi]
            .iter()
            .map(|&(n, d)| ([rat(n[0]), rat(n[1]), rat(n[2])], rat(d)))
            .collect();
        let norm2 = |n: &[RBig; 3]| &n[0] * &n[0] + &n[1] * &n[1] + &n[2] * &n[2];
        let det3 = |a: &[RBig; 3], b: &[RBig; 3], c: &[RBig; 3]| -> RBig {
            &a[0] * (&b[1] * &c[2] - &b[2] * &c[1]) - &a[1] * (&b[0] * &c[2] - &b[2] * &c[0])
                + &a[2] * (&b[0] * &c[1] - &b[1] * &c[0])
        };
        let floor2 = rat(DET_FLOOR) * rat(DET_FLOOR);

        // B1: first independent triple in plane order.
        let mut exact: Option<[RBig; 3]> = None;
        'triple: for i in 0..pls.len() {
            for j in (i + 1)..pls.len() {
                for k in (j + 1)..pls.len() {
                    let (na, nb, nc) = (&pls[i].0, &pls[j].0, &pls[k].0);
                    let det = det3(na, nb, nc);
                    if &det * &det <= &floor2 * &(norm2(na) * norm2(nb) * norm2(nc)) {
                        continue;
                    }
                    // Cramer: solve n·X = −d for the three planes (replace
                    // column m of [na; nb; nc] with the rhs).
                    let rhs = [-pls[i].1.clone(), -pls[j].1.clone(), -pls[k].1.clone()];
                    let col = |m: usize| -> RBig {
                        let rep = |r: &[RBig; 3], rv: &RBig| -> [RBig; 3] {
                            let mut o = r.clone();
                            o[m] = rv.clone();
                            o
                        };
                        det3(&rep(na, &rhs[0]), &rep(nb, &rhs[1]), &rep(nc, &rhs[2])) / &det
                    };
                    exact = Some([col(0), col(1), col(2)]);
                    break 'triple;
                }
            }
        }

        // B2 (or B6 degrade): exact projection onto the first
        // non-degenerate pair's intersection line. Solve
        // [n1; n2; dir] · X = [−d1; −d2; dir·P] with dir = n1×n2.
        if exact.is_none() {
            let p = v.point.as_array();
            let pr = [rat(p[0]), rat(p[1]), rat(p[2])];
            'pair: for i in 0..pls.len() {
                for j in (i + 1)..pls.len() {
                    let (na, nb) = (&pls[i].0, &pls[j].0);
                    let dir = [
                        &na[1] * &nb[2] - &na[2] * &nb[1],
                        &na[2] * &nb[0] - &na[0] * &nb[2],
                        &na[0] * &nb[1] - &na[1] * &nb[0],
                    ];
                    // |dir|² ≤ floor²·|na|²·|nb|² guards near-parallel
                    // distinct planes (sub-floor sin of the dihedral).
                    let d2 = norm2(&dir);
                    if d2 <= &floor2 * &(norm2(na) * norm2(nb)) {
                        continue;
                    }
                    let det = det3(na, nb, &dir);
                    if det == RBig::ZERO {
                        continue;
                    }
                    let rhs = [
                        -pls[i].1.clone(),
                        -pls[j].1.clone(),
                        &dir[0] * &pr[0] + &dir[1] * &pr[1] + &dir[2] * &pr[2],
                    ];
                    let rep = |r: &[RBig; 3], m: usize, rv: &RBig| -> [RBig; 3] {
                        let mut o = r.clone();
                        o[m] = rv.clone();
                        o
                    };
                    let col = |m: usize| -> RBig {
                        det3(
                            &rep(na, m, &rhs[0]),
                            &rep(nb, m, &rhs[1]),
                            &rep(&dir, m, &rhs[2]),
                        ) / &det
                    };
                    exact = Some([col(0), col(1), col(2)]);
                    break 'pair;
                }
            }
        }

        let Some(exact) = exact else { continue };
        let p = v.point.as_array();
        let mut newp = [0.0f64; 3];
        let mut ok = true;
        for k in 0..3 {
            let nf = exact[k].to_f64().value();
            if !nf.is_finite() {
                ok = false;
                break;
            }
            // B4 band guard, per component (KV10-scale, A14.3 reuse).
            if (nf - p[k]).abs() > cad_primitives::TAU_WORK * (1.0 + p[k].abs()) {
                ok = false;
                break;
            }
            newp[k] = nf;
        }
        if ok {
            v.point = Point3::new(newp[0], newp[1], newp[2]);
        } else if std::env::var_os("KV2_VERTEX_CANON_PROBE").is_some() {
            eprintln!(
                "[vertex-canon-over-band] v{vi} p={p:?} planes={}",
                planes[vi].len()
            );
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
                    FaceSurf::Cylinder { .. } | FaceSurf::Cone { .. } | FaceSurf::Torus { .. }
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
        yang_rs::Curve::Hyperbola { .. } => {
            Err(KernelV2Error::UnsupportedBooleanOutputCurve { curve: "Hyperbola" })
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
                    let band = 1e-9 * (1.0 + crate::geom::pair_surface_scale(s).max(mag));
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
        _ => Err(KernelV2Error::UnsupportedBooleanOutputCurve {
            curve: "surface-pair with a plane/sphere/torus operand (only cyl/cone are produced)",
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
    // PR-KV7: yang's input has no shell structure and its reassembly cannot
    // rebuild voids — wall multi-shell operands loudly (typed).
    for s in [a, b] {
        let shells = arena.solid(s)?.shells.len();
        if shells > 1 {
            return Err(KernelV2Error::UnsupportedMultiShellBoolean { shells });
        }
    }
    let (ya, a_faces) = to_yang_brep_indexed(arena, a)?;
    let (yb, b_faces) = to_yang_brep_indexed(arena, b)?;
    let Some(backend) = yang_rs::native_backend() else {
        // Unreachable since cherchi-rs M7c (the backend is always available),
        // kept as a loud arm rather than an unwrap (P9, no-panic rule).
        return Err(KernelV2Error::BooleanFailed(
            "yang-rs native backend unavailable".to_string(),
        ));
    };
    let out = yang_rs::boolean(&ya, &yb, op, &backend).map_err(map_yang_error)?;
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

// ---------------------------------------------------------------------------
// M8-intra: sign-aware `canonicalize_sibling_planes` (spec
// `specs/m8_intra_opposite_plane_canonicalization.md`).
//
// RED (FIP Phase 2). `canonicalize_sibling_planes` is private to this module,
// so these unit tests exercise it directly (the seam KV10 pins E2E through the
// public `to_yang_brep`; the femto-EXACT-negation assertions below need
// hand-crafted plane bits that a real solid cannot be coaxed into producing).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod m8_intra_canonicalization_tests {
    use super::*;

    fn normalize(v: [f64; 3]) -> [f64; 3] {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l]
    }

    /// A planar `yang_rs::BRepFace` carrying exactly `(normal, d)` — the loops
    /// are irrelevant to `canonicalize_sibling_planes` (it only rewrites
    /// `surface`), so they stay empty.
    fn plane_face(n: [f64; 3], d: f64) -> yang_rs::BRepFace {
        yang_rs::BRepFace {
            surface: yang_rs::Surface::Plane {
                normal: Vector3::new(n[0], n[1], n[2]),
                d,
            },
            outer_loop: Vec::new(),
            inner_loops: Vec::new(),
            reversed: false,
        }
    }

    fn plane_of(f: &yang_rs::BRepFace) -> ([f64; 3], f64) {
        match f.surface {
            yang_rs::Surface::Plane { normal, d } => (normal.as_array(), d),
            _ => panic!("expected a planar face"),
        }
    }

    /// Nudge an f64 by `n` ULPs (femto-scale drift ~1e-16 at unit magnitude) —
    /// the same rounding-noise class PR-KV10 collapses, here on the NEGATED
    /// sibling.
    fn bump(x: f64, n: u64) -> f64 {
        f64::from_bits(x.to_bits().wrapping_add(n))
    }

    /// Spec B2 + I1 + I2 (RED): a femto-near-negated sibling pair must
    /// canonicalize so the second face's plane bits are the EXACT negation of
    /// the first (representative) face's bits, with sense preserved.
    ///
    /// RED today: `canonicalize_sibling_planes` matches only same-sign
    /// component-wise, so the near-negated face never joins the cluster and
    /// keeps its perturbed bits — the exact-negation assertions fail.
    #[test]
    fn femto_negated_sibling_canonicalizes_to_exact_negation() {
        let n = normalize([0.6026151226794615, -0.3228572568748562, 0.7298069646154802]);
        let d0 = 23.84180252162639_f64;

        // Face B: the near-EXACT negation of A, drifted a few ULPs per
        // component (a chained-output rounding artifact).
        let nb_before = [bump(-n[0], 3), bump(-n[1], 2), bump(-n[2], 5)];
        let db_before = bump(-d0, 4);

        let mut faces = vec![plane_face(n, d0), plane_face(nb_before, db_before)];
        canonicalize_sibling_planes(&mut faces);

        let (na_after, da_after) = plane_of(&faces[0]);
        let (nb_after, db_after) = plane_of(&faces[1]);

        // The representative (first face) is untouched.
        assert_eq!(na_after, n, "representative normal moved");
        assert_eq!(da_after, d0, "representative offset moved");

        // I2 (bit-exact negation): B adopts exactly (-n_rep, -d_rep).
        for k in 0..3 {
            assert_eq!(
                nb_after[k], -na_after[k],
                "component {k}: sibling plane not the exact negation of the representative"
            );
        }
        assert_eq!(db_after, -da_after, "sibling offset not the exact negation");

        // I1 (sense preservation): the adopted normal keeps B's outward sense.
        let dot: f64 = (0..3).map(|k| nb_before[k] * nb_after[k]).sum();
        assert!(
            dot > 0.0,
            "canonicalization flipped face B's sense (dot = {dot})"
        );
    }

    /// Spec B3 (guard): two GENUINELY distinct parallel planes (1e-3 apart —
    /// six orders above the `TAU_WORK·(1+|d|)` band) must never cluster. Passes
    /// today and must keep passing (no over-merge).
    #[test]
    fn distinct_parallel_planes_stay_unclustered_guard() {
        let n = normalize([0.6026151226794615, -0.3228572568748562, 0.7298069646154802]);
        let d0 = 23.84180252162639_f64;
        let d1 = d0 + 1.0e-3;

        let mut faces = vec![plane_face(n, d0), plane_face(n, d1)];
        canonicalize_sibling_planes(&mut faces);

        let (_, da) = plane_of(&faces[0]);
        let (_, db) = plane_of(&faces[1]);
        assert_eq!(da, d0, "first distinct plane offset must be untouched");
        assert_eq!(
            db, d1,
            "distinct parallel plane wrongly collapsed onto sibling"
        );
    }

    // Spec I3 (same-orientation path byte-identical): the same-orientation
    // sibling-collapse behavior is pinned END-TO-END through the public
    // `to_yang_brep` path by `tests/kv10_plane_canonicalization.rs`
    // (`sibling_fragments_emit_bit_identical_planes`,
    // `chained_boolean_over_split_fragments_succeeds`). Those tests must remain
    // green after the sign-aware extension; this guard pins the same rule at
    // the unit boundary so a regression is localized here too.
    #[test]
    fn same_orientation_femto_siblings_still_collapse_guard() {
        let n = normalize([0.6026151226794615, -0.3228572568748562, 0.7298069646154802]);
        let d0 = 23.84180252162639_f64;

        // A femto-drifted SAME-sign sibling (the KV10 class).
        let nb = [bump(n[0], 3), bump(n[1], 2), bump(n[2], 5)];
        let db = bump(d0, 4);

        let mut faces = vec![plane_face(n, d0), plane_face(nb, db)];
        canonicalize_sibling_planes(&mut faces);

        let (na, da) = plane_of(&faces[0]);
        let (nb2, db2) = plane_of(&faces[1]);
        assert_eq!(na, n, "representative normal moved");
        assert_eq!(
            nb2, n,
            "same-orientation sibling did not adopt representative bits"
        );
        assert_eq!(da, d0, "representative offset moved");
        assert_eq!(
            db2, d0,
            "same-orientation sibling offset did not adopt representative"
        );
    }

    // ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
    // Attacks on the sign-aware `canonicalize_sibling_planes` at the SAME unit
    // boundary the RED tests above use. These live in-module (not in a new
    // `tests/` integration file) because `canonicalize_sibling_planes` is
    // module-private: the ULP-level band/greedy/zero-component attacks require
    // hand-crafted plane bits that only a direct call can inject, and the RED
    // note above already established that "a real solid cannot be coaxed into
    // producing" them through the public `to_yang_brep` seam. The E2E
    // over-merge guards that ARE reachable through the public API live in
    // `tests/m8_intra_adversary.rs`. Purely additive; touches no existing test.

    /// Attack 1 (offset-band boundary, negated arm): a negated sibling whose
    /// offset drift is 0.5× the `TAU_WORK·(1+|d|)` band clusters; 2× the band
    /// does not. Pins that the sign-aware match reuses the KV10 offset band
    /// unchanged (spec §2 — no new tolerance).
    #[test]
    fn adversary_negated_offset_band_just_below_and_just_above() {
        let n = [1.0, 0.0, 0.0];
        let d0 = 5.0_f64;
        let band = cad_primitives::TAU_WORK * (1.0 + d0.abs()); // = 6e-12

        // EXACT negation of the normal; offset drifted from -d0.
        let below = -d0 + 0.5 * band; // |below + d0| = 0.5·band ≤ band → cluster
        let above = -d0 + 2.0 * band; // 2·band > band → no cluster

        {
            let mut faces = vec![plane_face(n, d0), plane_face([-1.0, 0.0, 0.0], below)];
            canonicalize_sibling_planes(&mut faces);
            let (nb, db) = plane_of(&faces[1]);
            assert_eq!(
                nb,
                [-1.0, 0.0, 0.0],
                "just-below sibling normal not negated"
            );
            assert_eq!(db, -d0, "just-below sibling offset did not adopt −d_rep");
        }
        {
            let mut faces = vec![plane_face(n, d0), plane_face([-1.0, 0.0, 0.0], above)];
            canonicalize_sibling_planes(&mut faces);
            let (_, db) = plane_of(&faces[1]);
            assert_eq!(
                db, above,
                "just-above sibling wrongly clustered (over-merge)"
            );
        }
    }

    /// Attack 1 (normal-component band, negated arm): a per-component normal
    /// drift of 0.5·`TAU_WORK` off exact negation clusters; 2·`TAU_WORK` does
    /// not. Uses a zero representative component so the drift is injected
    /// exactly (no cancellation).
    #[test]
    fn adversary_negated_normal_component_band_boundary() {
        let n = [1.0, 0.0, 0.0];
        let d0 = 5.0_f64;
        let eps = cad_primitives::TAU_WORK;

        // y-component drift off exact negation (rep y = 0, so |n_y − s·0| = n_y).
        {
            let mut faces = vec![plane_face(n, d0), plane_face([-1.0, 0.5 * eps, 0.0], -d0)];
            canonicalize_sibling_planes(&mut faces);
            let (nb, db) = plane_of(&faces[1]);
            assert_eq!(
                nb,
                [-1.0, 0.0, 0.0],
                "0.5·eps drift should cluster to −n_rep"
            );
            assert_eq!(db, -d0);
        }
        {
            let drift = [-1.0, 2.0 * eps, 0.0];
            let mut faces = vec![plane_face(n, d0), plane_face(drift, -d0)];
            canonicalize_sibling_planes(&mut faces);
            let (nb, _) = plane_of(&faces[1]);
            assert_eq!(
                nb, drift,
                "2·eps normal drift wrongly clustered (over-merge)"
            );
        }
    }

    /// Attack 3 (greedy / order determinism): three faces — a representative, a
    /// same-sign femto sibling, and a negated femto sibling — collapse to ONE
    /// cluster under ALL 6 orderings, every face keeps its outward sense
    /// (I1: dot(before, after) > 0), and the result is sense-preserving and
    /// deterministic (each face's plane is exactly ± the first-seen rep's).
    #[test]
    fn adversary_three_face_cluster_is_order_invariant_and_sense_preserving() {
        let n = normalize([0.6026151226794615, -0.3228572568748562, 0.7298069646154802]);
        let d0 = 23.84180252162639_f64;

        // Same-sign femto sibling and negated femto sibling.
        let same = ([bump(n[0], 3), bump(n[1], 1), bump(n[2], 2)], bump(d0, 4));
        let neg = (
            [bump(-n[0], 2), bump(-n[1], 5), bump(-n[2], 1)],
            bump(-d0, 3),
        );
        let specs = [(n, d0), same, neg];

        for perm in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let before: Vec<([f64; 3], f64)> = perm.iter().map(|&i| specs[i]).collect();
            let mut faces: Vec<_> = before.iter().map(|&(nn, dd)| plane_face(nn, dd)).collect();
            canonicalize_sibling_planes(&mut faces);

            let after: Vec<([f64; 3], f64)> = faces.iter().map(plane_of).collect();
            let (rn, rd) = after[0];

            for (i, (&(nb, _), &(na, da))) in before.iter().zip(after.iter()).enumerate() {
                // I1: outward sense preserved.
                let dot: f64 = (0..3).map(|k| nb[k] * na[k]).sum();
                assert!(
                    dot > 0.0,
                    "perm {perm:?} face {i}: sense flipped (dot={dot})"
                );
                // Collapsed to exactly ± the first-seen representative.
                let pos = na == rn && da == rd;
                let negd = (0..3).all(|k| na[k] == -rn[k]) && da == -rd;
                assert!(
                    pos || negd,
                    "perm {perm:?} face {i}: plane {na:?},{da} is neither +rep nor −rep {rn:?},{rd}"
                );
            }
        }
    }

    /// Attack 4 (zero normal components): a rep with 0.0 components and a
    /// femto-near-negated sibling canonicalizes to the value-exact negation,
    /// with the zero components carried as −0.0 (= s·0.0) — the form the
    /// yang-rs exclusion's `0.0 == -0.0` value compare treats as benign.
    #[test]
    fn adversary_zero_component_negation_is_value_exact() {
        let n = [0.0, 0.0, 1.0];
        let d0 = 3.0_f64;
        let nb_before = [bump(-0.0, 0), bump(-0.0, 0), bump(-1.0, 4)]; // −0.0,−0.0,~−1
        let mut faces = vec![plane_face(n, d0), plane_face(nb_before, bump(-d0, 2))];
        canonicalize_sibling_planes(&mut faces);

        let (nb, db) = plane_of(&faces[1]);
        // Value-exact negation (0.0 == −0.0 holds by value).
        for k in 0..3 {
            assert_eq!(
                nb[k], -n[k],
                "component {k} not the value-negation of the rep"
            );
        }
        assert_eq!(db, -d0);
        // s·0.0 = −0.0: the adopted zero components carry the negative sign bit.
        assert_eq!(
            nb[0].to_bits(),
            (-0.0f64).to_bits(),
            "zero comp lost its −0.0 bit"
        );
        assert_eq!(nb[1].to_bits(), (-0.0f64).to_bits());
    }

    /// Attack 5 (non-unit normals): two faces with normals of different
    /// magnitude on ONE geometric plane (n vs −2n) must NOT cluster — the
    /// component band assumes unit normals, so the |−2 − (−1)·1| = 1 gap keeps
    /// them apart. Documented conservative residue; nothing crashes.
    #[test]
    fn adversary_nonunit_opposite_normals_do_not_cluster() {
        let mut faces = vec![
            plane_face([0.0, 0.0, 1.0], 3.0),
            plane_face([0.0, 0.0, -2.0], -6.0),
        ];
        canonicalize_sibling_planes(&mut faces);
        let (nb, db) = plane_of(&faces[1]);
        assert_eq!(nb, [0.0, 0.0, -2.0], "non-unit sibling wrongly rewritten");
        assert_eq!(db, -6.0);
    }

    /// Attack 6 (offset near 0, F0084-class): exactly-negated normals with tiny
    /// femto-scale offsets (the real probed signature d ≈ −6.9e-18 vs
    /// ≈ 1.2e-17) cluster — the offset band is ≈ `TAU_WORK`, orders above the
    /// drift — and the sibling adopts the exact negation −d_rep.
    #[test]
    fn adversary_offset_near_zero_negation_clusters() {
        let n = normalize([0.6026151226794615, -0.3228572568748562, 0.7298069646154802]);
        let rd = -6.9e-18_f64;
        let neg_n = [-n[0], -n[1], -n[2]]; // exact negation of the normal
        let db_before = 1.2e-17_f64; // ≈ −rd, drifted at the femto scale

        let mut faces = vec![plane_face(n, rd), plane_face(neg_n, db_before)];
        canonicalize_sibling_planes(&mut faces);

        let (nb, db) = plane_of(&faces[1]);
        for k in 0..3 {
            assert_eq!(
                nb[k], -n[k],
                "near-zero-offset sibling normal not exactly negated"
            );
        }
        assert_eq!(
            db, -rd,
            "near-zero-offset sibling did not adopt −d_rep exactly"
        );
        let dot: f64 = (0..3).map(|k| neg_n[k] * nb[k]).sum();
        assert!(dot > 0.0, "sense flipped on the near-zero-offset sibling");
    }
}

// ---------------------------------------------------------------------------
// M8-vertex-canon: chained-output VERTEX canonicalization
// (spec `specs/m8_shared_boundary_identity.md`, FIP Phase 2, RED).
//
// Seam: a direct unit on the new pass `canonicalize_vertices_to_planes`, which
// `to_yang_brep` will call immediately after `canonicalize_sibling_planes` on
// the assembled yang arrays. A hand-CROOKED arena cannot be built through the
// public arena/Euler constructors — `to_yang_brep` anchors each yang plane's
// `d` at a loop vertex, so a vertex is never inconsistent with its own derived
// plane; the femto-crooked-vs-canonical divergence only appears mid-`to_yang`,
// after plane canonicalization. So the invariants are exercised on the yang
// (verts, edges, faces) shape directly.
//
// SETTLED SIGNATURE (the implementer matches this — it is exactly the data
// `to_yang_brep` holds at that point):
//
//   fn canonicalize_vertices_to_planes(
//       yverts: &mut [yang_rs::BRepVertex],
//       yedges: &[yang_rs::BRepEdge],
//       yfaces: &[yang_rs::BRepFace],
//   )
//
// Vertex→incident-plane incidence is recovered from `yedges` (loop edge →
// vertex pair) over each face's loops. These tests do NOT compile until that
// function exists — that IS the RED state for the unit oracles.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod m8_vertex_canon_tests {
    use super::*;
    use dashu::float::FBig;
    use dashu::rational::RBig;

    fn vtx(x: f64, y: f64, z: f64) -> yang_rs::BRepVertex {
        yang_rs::BRepVertex {
            point: Point3::new(x, y, z),
        }
    }

    fn seg(s: u32, e: u32) -> yang_rs::BRepEdge {
        yang_rs::BRepEdge {
            start: s,
            end: e,
            curve: yang_rs::Curve::LineSegment,
        }
    }

    fn plane_face(n: [f64; 3], d: f64, loop_edges: Vec<u32>) -> yang_rs::BRepFace {
        yang_rs::BRepFace {
            surface: yang_rs::Surface::Plane {
                normal: Vector3::new(n[0], n[1], n[2]),
                d,
            },
            outer_loop: loop_edges,
            inner_loops: Vec::new(),
            reversed: false,
        }
    }

    fn cyl_face(loop_edges: Vec<u32>) -> yang_rs::BRepFace {
        yang_rs::BRepFace {
            surface: yang_rs::Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            },
            outer_loop: loop_edges,
            inner_loops: Vec::new(),
            reversed: false,
        }
    }

    /// Nudge `x` by `k` ULPs (femto drift ~1e-16 at unit magnitude).
    fn bump(x: f64, k: i64) -> f64 {
        if k >= 0 {
            f64::from_bits(x.to_bits().wrapping_add(k as u64))
        } else {
            f64::from_bits(x.to_bits().wrapping_sub((-k) as u64))
        }
    }

    fn vbits(v: &yang_rs::BRepVertex) -> [u64; 3] {
        let a = v.point.as_array();
        [a[0].to_bits(), a[1].to_bits(), a[2].to_bits()]
    }

    fn rat(x: f64) -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite");
        RBig::try_from(fb).expect("finite")
    }

    fn round_f64(r: &RBig) -> f64 {
        r.to_f64().value()
    }

    /// Box [1,3]³ topology: 6 quad faces, each carrying its own 4 directed
    /// edges over its corner loop; every corner is incident to exactly its 3
    /// axis planes. Returns (edges, faces); vertices supplied separately.
    fn box_topology() -> (Vec<yang_rs::BRepEdge>, Vec<yang_rs::BRepFace>) {
        // (corner loop, plane normal, plane d) — n·x + d = 0.
        let faces: [([u32; 4], [f64; 3], f64); 6] = [
            ([0, 1, 2, 3], [0.0, 0.0, -1.0], 1.0), // z = 1
            ([4, 5, 6, 7], [0.0, 0.0, 1.0], -3.0), // z = 3
            ([0, 1, 5, 4], [0.0, -1.0, 0.0], 1.0), // y = 1
            ([1, 2, 6, 5], [1.0, 0.0, 0.0], -3.0), // x = 3
            ([2, 3, 7, 6], [0.0, 1.0, 0.0], -3.0), // y = 3
            ([3, 0, 4, 7], [-1.0, 0.0, 0.0], 1.0), // x = 1
        ];
        let mut yedges = Vec::new();
        let mut yfaces = Vec::new();
        for (corners, n, d) in faces {
            let base = yedges.len() as u32;
            for k in 0..4 {
                yedges.push(seg(corners[k], corners[(k + 1) % 4]));
            }
            yfaces.push(plane_face(n, d, (base..base + 4).collect()));
        }
        (yedges, yfaces)
    }

    const BOX_CORNERS: [[f64; 3]; 8] = [
        [1.0, 1.0, 1.0],
        [3.0, 1.0, 1.0],
        [3.0, 3.0, 1.0],
        [1.0, 3.0, 1.0],
        [1.0, 1.0, 3.0],
        [3.0, 1.0, 3.0],
        [3.0, 3.0, 3.0],
        [1.0, 3.0, 3.0],
    ];

    /// B1 / I1 (RED): a femto-crooked axis-aligned box — planes exact, each
    /// corner perturbed a few ULPs off its exact tri-plane intersection —
    /// snaps every corner BIT-equal to the exact integer intersection. I3: a
    /// second pass is a byte-identical no-op.
    #[test]
    fn femto_crooked_box_snaps_corners_to_exact_intersections() {
        // Distinct per-vertex/per-axis ULP perturbations (all ≪ the band
        // TAU_WORK·(1+3) = 4e-12, so all adopted).
        let dk: [[i64; 3]; 8] = [
            [1, -2, 3],
            [-1, 2, -3],
            [2, -1, 1],
            [-3, 1, -2],
            [1, 3, -1],
            [-2, -1, 2],
            [3, -3, 1],
            [-1, 2, -2],
        ];
        let mut yverts: Vec<_> = (0..8)
            .map(|i| {
                vtx(
                    bump(BOX_CORNERS[i][0], dk[i][0]),
                    bump(BOX_CORNERS[i][1], dk[i][1]),
                    bump(BOX_CORNERS[i][2], dk[i][2]),
                )
            })
            .collect();
        let (yedges, yfaces) = box_topology();

        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);

        for i in 0..8 {
            let got = yverts[i].point.as_array();
            for k in 0..3 {
                assert_eq!(
                    got[k].to_bits(),
                    BOX_CORNERS[i][k].to_bits(),
                    "B1/I1: corner {i} coord {k} not bit-equal to the exact intersection"
                );
            }
        }

        // I3 idempotence: rerun is byte-identical.
        let snap: Vec<_> = yverts.iter().map(vbits).collect();
        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);
        for i in 0..8 {
            assert_eq!(
                vbits(&yverts[i]),
                snap[i],
                "I3: second pass is not byte-identical (vertex {i})"
            );
        }
    }

    /// B2 (RED): a subdivided-edge vertex on exactly 2 (orthogonal) planes,
    /// perturbed femto off their intersection line, lands BIT-equal to the
    /// exact rational line projection and both plane residuals collapse to a
    /// rounding ULP.
    #[test]
    fn subdivided_edge_vertex_projects_onto_intersection_line() {
        // Planes y=1 (n=(0,1,0), d=−1) and z=1 (n=(0,0,1), d=−1); line {(t,1,1)}.
        let n_a = [0.0, 1.0, 0.0];
        let d_a = -1.0;
        let n_b = [0.0, 0.0, 1.0];
        let d_b = -1.0;
        // V intended at x=2 on the line, perturbed femto in y and z (x exact).
        let vx = 2.0;
        let v = [vx, bump(1.0, 3), bump(1.0, 2)];

        let mut yverts = vec![
            vtx(v[0], v[1], v[2]), // 0: V — incident to both planes
            vtx(0.0, 1.0, 0.0),    // 1: on y=1
            vtx(0.0, 1.0, 3.0),    // 2: on y=1
            vtx(0.0, 0.0, 1.0),    // 3: on z=1
            vtx(0.0, 3.0, 1.0),    // 4: on z=1
        ];
        let yedges = vec![
            seg(0, 1),
            seg(1, 2),
            seg(2, 0), // face A (y=1): edges 0,1,2
            seg(0, 3),
            seg(3, 4),
            seg(4, 0), // face B (z=1): edges 3,4,5
        ];
        let yfaces = vec![
            plane_face(n_a, d_a, vec![0, 1, 2]),
            plane_face(n_b, d_b, vec![3, 4, 5]),
        ];

        // Exact expected: P' = V − (n_a·V+d_a)·n_a − (n_b·V+d_b)·n_b, valid as
        // the line projection because n_a·n_b = 0 (orthogonal). Computed in
        // RBig, rounded once — exactly what the pass must produce.
        let dot = n_a[0] * n_b[0] + n_a[1] * n_b[1] + n_a[2] * n_b[2];
        assert_eq!(
            dot, 0.0,
            "fixture: planes must be orthogonal for this closed form"
        );
        let vr = [rat(v[0]), rat(v[1]), rat(v[2])];
        let nar = [rat(n_a[0]), rat(n_a[1]), rat(n_a[2])];
        let nbr = [rat(n_b[0]), rat(n_b[1]), rat(n_b[2])];
        let dot3 = |a: &[RBig; 3], b: &[RBig; 3]| {
            a[0].clone() * b[0].clone() + a[1].clone() * b[1].clone() + a[2].clone() * b[2].clone()
        };
        let ra = dot3(&nar, &vr) + rat(d_a);
        let rb = dot3(&nbr, &vr) + rat(d_b);
        let mut expected = [0.0; 3];
        for k in 0..3 {
            let pk = vr[k].clone() - ra.clone() * nar[k].clone() - rb.clone() * nbr[k].clone();
            expected[k] = round_f64(&pk);
        }

        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);

        let got = yverts[0].point.as_array();
        for k in 0..3 {
            assert_eq!(
                got[k].to_bits(),
                expected[k].to_bits(),
                "B2: vertex coord {k} not bit-equal to the exact line projection"
            );
        }
        let res_a = n_a[0] * got[0] + n_a[1] * got[1] + n_a[2] * got[2] + d_a;
        let res_b = n_b[0] * got[0] + n_b[1] * got[1] + n_b[2] * got[2] + d_b;
        let ulp = 4.0 * f64::EPSILON * (1.0 + vx.abs());
        assert!(
            res_a.abs() <= ulp && res_b.abs() <= ulp,
            "B2: plane residuals must collapse to a rounding ULP (a={res_a}, b={res_b})"
        );
    }

    /// B4 guard: a vertex 1e-6 off its 3 planes (≫ band) is left UNCHANGED
    /// (never forced onto an intersection it doesn't belong to).
    #[test]
    fn vertex_beyond_band_is_unchanged() {
        // Planes x=2, y=2, z=2; V is 1e-6 off each (band = 3e-12).
        let v = [2.0 + 1e-6, 2.0 + 1e-6, 2.0 + 1e-6];
        let mut yverts = vec![
            vtx(v[0], v[1], v[2]),
            vtx(2.0, 0.0, 0.0),
            vtx(2.0, 0.0, 5.0), // x=2
            vtx(0.0, 2.0, 0.0),
            vtx(0.0, 2.0, 5.0), // y=2
            vtx(0.0, 0.0, 2.0),
            vtx(5.0, 0.0, 2.0), // z=2
        ];
        let yedges = vec![
            seg(0, 1),
            seg(1, 2),
            seg(2, 0),
            seg(0, 3),
            seg(3, 4),
            seg(4, 0),
            seg(0, 5),
            seg(5, 6),
            seg(6, 0),
        ];
        let yfaces = vec![
            plane_face([1.0, 0.0, 0.0], -2.0, vec![0, 1, 2]),
            plane_face([0.0, 1.0, 0.0], -2.0, vec![3, 4, 5]),
            plane_face([0.0, 0.0, 1.0], -2.0, vec![6, 7, 8]),
        ];
        let before = vbits(&yverts[0]);
        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);
        assert_eq!(
            vbits(&yverts[0]),
            before,
            "B4: a vertex 1e-6 off its planes (≫ band) must be left UNCHANGED"
        );
    }

    /// B7 / I4 guard: an already-exact box is byte-identical through the pass.
    #[test]
    fn exact_box_is_byte_identical() {
        let mut yverts: Vec<_> = (0..8)
            .map(|i| vtx(BOX_CORNERS[i][0], BOX_CORNERS[i][1], BOX_CORNERS[i][2]))
            .collect();
        let (yedges, yfaces) = box_topology();
        let before: Vec<_> = yverts.iter().map(vbits).collect();
        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);
        for i in 0..8 {
            assert_eq!(
                vbits(&yverts[i]),
                before[i],
                "B7/I4: exact box vertex {i} must be byte-identical"
            );
        }
    }

    /// B5 guard: a vertex that WOULD snap under B2 (femto off two planes) but
    /// also touches a curved (cylinder) face is left UNCHANGED — curve
    /// exactness owns the vertex.
    #[test]
    fn vertex_with_curved_incident_face_is_unchanged() {
        let v = [2.0, bump(1.0, 3), bump(1.0, 2)];
        let mut yverts = vec![
            vtx(v[0], v[1], v[2]), // 0: V
            vtx(0.0, 1.0, 0.0),
            vtx(0.0, 1.0, 3.0), // y=1
            vtx(0.0, 0.0, 1.0),
            vtx(0.0, 3.0, 1.0), // z=1
            vtx(5.0, 0.0, 0.0),
            vtx(5.0, 5.0, 0.0), // cylinder-face loop mates
        ];
        let yedges = vec![
            seg(0, 1),
            seg(1, 2),
            seg(2, 0), // plane A (y=1)
            seg(0, 3),
            seg(3, 4),
            seg(4, 0), // plane B (z=1)
            seg(0, 5),
            seg(5, 6),
            seg(6, 0), // cylinder face touches V
        ];
        let yfaces = vec![
            plane_face([0.0, 1.0, 0.0], -1.0, vec![0, 1, 2]),
            plane_face([0.0, 0.0, 1.0], -1.0, vec![3, 4, 5]),
            cyl_face(vec![6, 7, 8]),
        ];
        let before = vbits(&yverts[0]);
        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);
        assert_eq!(
            vbits(&yverts[0]),
            before,
            "B5: a vertex with ANY curved incident face must be left UNCHANGED"
        );
    }

    /// I2 (oblique bounded motion): a rotated-frame crooked box — planes exact
    /// in an oblique orthonormal frame, corners carrying the sub-band residuals
    /// an oblique fresh extrude has by construction (§4 I4 amendment) — is
    /// canonicalized with EVERY adopted per-component displacement ≤ the KV10
    /// band `TAU_WORK·(1+|coord|)`, and at least one vertex actually moves
    /// (non-vacuous). This pins the oblique blast radius the amended I4 carved
    /// out of byte-identity.
    #[test]
    fn oblique_crooked_box_moves_within_band() {
        // Oblique orthonormal frame (u, v, t) — irrational direction cosines.
        fn norm(a: [f64; 3]) -> [f64; 3] {
            let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            [a[0] / l, a[1] / l, a[2] / l]
        }
        let u = norm([1.0, 2.0, 3.0]);
        let wref = [0.3, -0.4, 0.5];
        let du = wref[0] * u[0] + wref[1] * u[1] + wref[2] * u[2];
        let v = norm([
            wref[0] - du * u[0],
            wref[1] - du * u[1],
            wref[2] - du * u[2],
        ]);
        let t = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let o = [0.1, 0.2, 0.3];
        let l = 2.0;
        // Corner (i,j,k) = O + i·L·u + j·L·v + k·L·t (i,j,k ∈ {0,1}); its f64
        // evaluation carries the ~1e-16 frame residual off the exact 3-plane
        // intersection. A couple ULPs of extra perturbation guarantees motion.
        let corner = |i: f64, j: f64, k: f64| {
            [
                o[0] + i * l * u[0] + j * l * v[0] + k * l * t[0],
                o[1] + i * l * u[1] + j * l * v[1] + k * l * t[1],
                o[2] + i * l * u[2] + j * l * v[2] + k * l * t[2],
            ]
        };
        let ijk = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        let mut yverts: Vec<_> = ijk
            .iter()
            .map(|c| {
                let p = corner(c[0], c[1], c[2]);
                // +2 ULPs on x to force a detectable move on adoption.
                vtx(bump(p[0], 2), p[1], p[2])
            })
            .collect();

        // 6 faces: normals u,v,t; offset d = −(axis·O + s·L), s the face's slab.
        let faces: [([u32; 4], [f64; 3], f64); 6] = [
            ([0, 1, 2, 3], t, -dot(t, o)),       // t = 0
            ([4, 5, 6, 7], t, -(dot(t, o) + l)), // t = L
            ([0, 1, 5, 4], v, -dot(v, o)),       // v = 0
            ([1, 2, 6, 5], u, -(dot(u, o) + l)), // u = L
            ([2, 3, 7, 6], v, -(dot(v, o) + l)), // v = L
            ([3, 0, 4, 7], u, -dot(u, o)),       // u = 0
        ];
        let mut yedges = Vec::new();
        let mut yfaces = Vec::new();
        for (corners, n, d) in faces {
            let base = yedges.len() as u32;
            for k in 0..4 {
                yedges.push(seg(corners[k], corners[(k + 1) % 4]));
            }
            yfaces.push(plane_face(n, d, (base..base + 4).collect()));
        }

        let before: Vec<[f64; 3]> = yverts.iter().map(|v| v.point.as_array()).collect();
        canonicalize_vertices_to_planes(&mut yverts, &yedges, &yfaces);

        let mut moved = 0usize;
        for i in 0..8 {
            let a = before[i];
            let b = yverts[i].point.as_array();
            for k in 0..3 {
                let disp = (b[k] - a[k]).abs();
                let band = cad_primitives::TAU_WORK * (1.0 + a[k].abs());
                assert!(
                    disp <= band,
                    "I2: oblique vertex {i} coord {k} moved {disp:e} > band {band:e}"
                );
            }
            if b != a {
                moved += 1;
            }
        }
        assert!(
            moved > 0,
            "non-vacuous: the oblique pass must actually move at least one vertex"
        );
    }

    // ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
    // Attacks on canonicalize_vertices_to_planes (band edge, DET_FLOOR wedge,
    // negated/duplicate plane dedup, >3-plane determinism). In-module (the
    // function is private). Purely additive; touches no existing test.

    /// Build a vertex-0 incident to a list of planes, each face a triangle loop
    /// (V, a, b) so V is topologically on every plane. `extra_verts` supplies the
    /// two loop-mate coordinates per face (their positions are irrelevant to the
    /// pass — only V's planes matter).
    fn single_vertex_on_planes(
        v0: [f64; 3],
        planes: &[([f64; 3], f64)],
    ) -> (
        Vec<yang_rs::BRepVertex>,
        Vec<yang_rs::BRepEdge>,
        Vec<yang_rs::BRepFace>,
    ) {
        let mut yverts = vec![vtx(v0[0], v0[1], v0[2])];
        let mut yedges = Vec::new();
        let mut yfaces = Vec::new();
        for (i, &(n, d)) in planes.iter().enumerate() {
            // Two throwaway loop mates per face (distinct indices).
            let a = yverts.len() as u32;
            yverts.push(vtx(10.0 + i as f64, 0.0, 0.0));
            let b = yverts.len() as u32;
            yverts.push(vtx(0.0, 10.0 + i as f64, 0.0));
            let base = yedges.len() as u32;
            yedges.push(seg(0, a));
            yedges.push(seg(a, b));
            yedges.push(seg(b, 0));
            yfaces.push(plane_face(n, d, vec![base, base + 1, base + 2]));
        }
        (yverts, yedges, yfaces)
    }

    /// B4 band edge (per component): a vertex off its 3 axis planes by 0.5·band
    /// on one axis ADOPTS the exact intersection; by 2·band it is left
    /// UNCHANGED. Tighter than the 1e-6 guard; pins the `<=` boundary and is the
    /// dedicated killer for dropping the band guard.
    #[test]
    fn adversary_band_edge_adopt_below_reject_above() {
        let planes = [
            ([1.0, 0.0, 0.0], -2.0),
            ([0.0, 1.0, 0.0], -2.0),
            ([0.0, 0.0, 1.0], -2.0),
        ];
        let band = cad_primitives::TAU_WORK * (1.0 + 2.0); // 3e-12

        // 0.5·band off on x only → adopts (x snaps to 2.0 exactly).
        let (mut yv, ye, yf) = single_vertex_on_planes([2.0 + 0.5 * band, 2.0, 2.0], &planes);
        canonicalize_vertices_to_planes(&mut yv, &ye, &yf);
        assert_eq!(
            yv[0].point.x().to_bits(),
            2.0f64.to_bits(),
            "0.5·band off must adopt the exact plane intersection"
        );

        // 2·band off on x → whole vertex unchanged (band guard rejects).
        let off = 2.0 + 2.0 * band;
        let (mut yv, ye, yf) = single_vertex_on_planes([off, 2.0, 2.0], &planes);
        canonicalize_vertices_to_planes(&mut yv, &ye, &yf);
        assert_eq!(
            yv[0].point.x().to_bits(),
            off.to_bits(),
            "2·band off must leave the vertex UNCHANGED (band guard)"
        );
    }

    /// MUTATION KILLER (c) — DET_FLOOR wedge. A vertex on two well-conditioned
    /// planes (x=2, y=2) plus a THIN-WEDGE plane whose normal (1,0,ε), ε=1e-11,
    /// is near-parallel to x=2 AND whose offset places the exact 3-plane
    /// intersection FAR away (z≈1e6). With the DET_FLOOR (production) the
    /// near-dependent triple is skipped and the vertex degrades to B2 — projected
    /// exactly onto the x=2,y=2 line, so its femto-off x/y SNAP to 2.0. WITHOUT
    /// the floor (DET_FLOOR=0) the ill-conditioned 3-plane solve returns the far
    /// (2,2,≈1e6) point, which the band guard REJECTS → the vertex is left
    /// crooked (x stays 2+δ). So the floor is load-bearing: it turns a rejected
    /// wild solve into an adopted B2 straighten.
    ///
    /// Verified: production → x bit-equal 2.0; DET_FLOOR=0 mutant → x unchanged.
    #[test]
    fn adversary_thin_wedge_floor_degrades_to_b2_straighten() {
        let eps = 1.0e-11_f64; // < DET_FLOOR=1e-9 → triple skipped in production
        let d3 = -2.0 - 1.0e-5_f64; // 3-plane intersection z = 1e-5/eps = 1e6
        let planes = [
            ([1.0, 0.0, 0.0], -2.0), // x = 2
            ([0.0, 1.0, 0.0], -2.0), // y = 2
            ([1.0, 0.0, eps], d3),   // thin wedge, near-parallel to x=2
        ];
        let delta = 1.0e-13_f64; // femto off the x=2,y=2 line, ≪ band
        let (mut yv, ye, yf) = single_vertex_on_planes([2.0 + delta, 2.0 + delta, 5.0], &planes);
        canonicalize_vertices_to_planes(&mut yv, &ye, &yf);
        // B2 degrade projects onto the exact x=2,y=2 line.
        assert_eq!(
            yv[0].point.x().to_bits(),
            2.0f64.to_bits(),
            "B6→B2: x must snap to the exact 2-plane line (DET_FLOOR skipped the wild triple)"
        );
        assert_eq!(
            yv[0].point.y().to_bits(),
            2.0f64.to_bits(),
            "B6→B2: y snaps to 2.0"
        );
        assert_eq!(yv[0].point.z(), 5.0, "z stays on the free line coordinate");
    }

    /// Negated + exact-duplicate plane dedup. A vertex incident to x=2 via BOTH
    /// orientations (n,d)=((1,0,0),−2) AND ((−1,0,0),2) — plus y=2 and z=2 — must
    /// solve to the exact apex (2,2,2): the negated pair is ONE plane, so three
    /// DISTINCT planes remain. Pins the dedup's intended semantics.
    ///
    /// FINDING (documented, not a killer): dropping the dedup does NOT change the
    /// result — the exact det floor skips every triple containing a
    /// negated/duplicate pair (det ≡ 0), so the first INDEPENDENT triple found is
    /// identical with or without the dedup. The dedup is a
    /// performance/legibility optimization, structurally redundant with the det
    /// floor for correctness (analogous to the ear-clip coverage-cert finding).
    #[test]
    fn adversary_negated_duplicate_planes_solve_to_apex() {
        let planes = [
            ([1.0, 0.0, 0.0], -2.0), // x = 2
            ([-1.0, 0.0, 0.0], 2.0), // x = 2, opposite orientation (dedup target)
            ([0.0, 1.0, 0.0], -2.0), // y = 2
            ([0.0, 0.0, 1.0], -2.0), // z = 2
        ];
        let bump = |x: f64, k: i64| bump(x, k);
        let (mut yv, ye, yf) =
            single_vertex_on_planes([bump(2.0, 2), bump(2.0, -1), bump(2.0, 3)], &planes);
        canonicalize_vertices_to_planes(&mut yv, &ye, &yf);
        for (k, want) in [(0usize, 2.0f64), (1, 2.0), (2, 2.0)] {
            assert_eq!(
                yv[0].point.as_array()[k].to_bits(),
                want.to_bits(),
                "negated/duplicate planes must solve to the exact apex (coord {k})"
            );
        }
    }

    /// I5 determinism — a vertex where FOUR planes concur at the apex (2,2,2):
    /// the three axis planes plus a diagonal x+y+z=6. Every face-order
    /// permutation selects a valid independent triple through the SAME apex, so
    /// the adopted point is permutation-invariant and bit-exact.
    #[test]
    fn adversary_four_concurrent_planes_permutation_invariant() {
        let base = [
            ([1.0, 0.0, 0.0], -2.0),
            ([0.0, 1.0, 0.0], -2.0),
            ([0.0, 0.0, 1.0], -2.0),
            ([1.0, 1.0, 1.0], -6.0), // x+y+z=6, through (2,2,2)
        ];
        let start = [bump(2.0, 1), bump(2.0, -2), bump(2.0, 2)];
        // A few representative orderings of the four planes.
        for perm in [[0, 1, 2, 3], [3, 2, 1, 0], [2, 0, 3, 1], [1, 3, 0, 2]] {
            let planes: Vec<_> = perm.iter().map(|&i| base[i]).collect();
            let (mut yv, ye, yf) = single_vertex_on_planes(start, &planes);
            canonicalize_vertices_to_planes(&mut yv, &ye, &yf);
            for k in 0..3 {
                assert_eq!(
                    yv[0].point.as_array()[k].to_bits(),
                    2.0f64.to_bits(),
                    "I5: permutation {perm:?} coord {k} must be the exact apex"
                );
            }
        }
    }
}
