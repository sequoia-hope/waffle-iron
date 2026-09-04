//! kernel-v2 arena solid → `yang_rs::BRep` input conversion (PR-KV3 planar,
//! PR-KV5b curved). Move-only split from the boolean god-module (design review
//! 2026-07-12 F9); byte-identical. See `super`'s module docs for the sharing
//! rationale and the curved-face vocabulary.

use super::*;

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
        // The id is the yang vertex-pool index (NOT the map's length): the
        // apex-cone operand arm below mints an edge-less apex vertex that has
        // no arena counterpart, so the two counts can differ.
        let id = yverts.len() as u32;
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
                    // 2026-08-19 (R0047 op-3 anchor): a FOUR-edge outer loop
                    // is structured only when its curve pattern is one of the
                    // analytic rim/strip vocabularies below (canonical tube,
                    // partial revolve wall, partial torus, closed torus). A
                    // 4-edge loop with any OTHER pattern — e.g. the cone patch
                    // `[HyperbolaArc, Line, EllipseArc, Line]` left by a prior
                    // boolean's two box planes — is a bounded partial patch
                    // bitten by a prior boolean and belongs to the Slice-D/E
                    // CDT re-entry exactly like its 5-edge siblings; the edge
                    // COUNT was never the criterion, the pattern is. Routed
                    // here so the structured path below only ever sees a
                    // structured loop (byte-identical for every structured
                    // loop: the same pattern test, evaluated earlier).
                    let four_edge_structured = outer_hes.len() == 4 && {
                        let mut hes = outer_hes.clone();
                        if matches!(arena.half_edge(hes[0])?.curve, Curve::LineSegment) {
                            hes.rotate_left(1);
                        }
                        if matches!(face.surface, Some(Surface::Torus { .. }))
                            && matches!(arena.half_edge(hes[0])?.curve, Curve::Arc { .. })
                        {
                            hes.rotate_left(1);
                        }
                        let pattern = (
                            arena.half_edge(hes[0])?.curve,
                            arena.half_edge(hes[1])?.curve,
                            arena.half_edge(hes[2])?.curve,
                            arena.half_edge(hes[3])?.curve,
                        );
                        matches!(
                            pattern,
                            (
                                Curve::Circle { .. },
                                Curve::LineSegment,
                                Curve::Circle { .. },
                                Curve::LineSegment
                            ) | (
                                Curve::Arc { .. },
                                Curve::LineSegment,
                                Curve::Arc { .. },
                                Curve::LineSegment
                            ) | (
                                Curve::Circle { .. },
                                Curve::Arc { .. },
                                Curve::Circle { .. },
                                Curve::Arc { .. }
                            )
                        ) || (matches!(face.surface, Some(Surface::Torus { .. }))
                            && matches!(
                                pattern,
                                (
                                    Curve::Circle { .. },
                                    Curve::Circle { .. },
                                    Curve::Circle { .. },
                                    Curve::Circle { .. }
                                )
                            ))
                    };
                    // KV14 apex-cone OPERAND (C0063): a solid cone from an
                    // on-axis apex-triangle revolve has ONE lateral loop — the
                    // closed base rim, twinned to the disc cap — and its apex is
                    // a singular SURFACE point, not an arena vertex
                    // (`kv6a_revolve::on_axis_triangle_full_turn_builds_solid_cone`:
                    // 1 vertex, 1 edge, 2 faces). yang's structured cone arm
                    // (PR-YR16, the `[rim_e]` apex FAN) wants exactly this
                    // shape — one shared rim edge — plus the apex as a
                    // PRE-SEEDED B-Rep vertex it locates by position (Stage 1
                    // seeds every vertex 1:1 into the mesh; the fan reuses it,
                    // so the cone stays watertight with no duplicate apex).
                    // Mint that vertex here (edge-less, deduplicated by
                    // position) and share the rim through the same converter
                    // the structured 4-edge path uses (cap-outward normal).
                    if face.inner_loops.is_empty() && outer_hes.len() == 1 {
                        if let (
                            Some(Surface::Cone {
                                apex,
                                axis_dir,
                                half_angle,
                                ..
                            }),
                            Curve::Circle { .. },
                        ) = (face.surface, arena.half_edge(outer_hes[0])?.curve)
                        {
                            let rim = convert_lateral_edge(
                                outer_hes[0],
                                arena,
                                &mut vid_map,
                                &mut yverts,
                                &mut yedges,
                                &mut shared_edges,
                            )?;
                            let ap = apex.as_array();
                            let seeded = yverts.iter().any(|v| {
                                let q = v.point.as_array();
                                let d = [q[0] - ap[0], q[1] - ap[1], q[2] - ap[2]];
                                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                                    <= cad_primitives::TAU_MODEL
                            });
                            if !seeded {
                                yverts.push(yang_rs::BRepVertex { point: apex });
                            }
                            face_ids.push(f);
                            yfaces.push(yang_rs::BRepFace {
                                surface: yang_rs::Surface::Cone {
                                    apex,
                                    axis_dir: Vector3::new(axis_dir.x, axis_dir.y, axis_dir.z),
                                    half_angle,
                                },
                                outer_loop: vec![rim],
                                inner_loops: Vec::new(),
                                reversed,
                            });
                            continue;
                        }
                    }
                    if !face.inner_loops.is_empty() || outer_hes.len() != 4 || !four_edge_structured
                    {
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
                            // window(s)) now routes too. Slice F-3: a HOLE-FREE
                            // lateral with a non-structured outer loop (R0032's
                            // lone 57-chord torus∩cone polyline) routes as a DISK
                            // patch — the same consumer's 0-wrapping branch. A
                            // full-circle rim (`Curve::Circle`) is still the
                            // canonical structured torus (no CDT re-entry) →
                            // stays the typed wall.
                            Some(Surface::Torus {
                                center,
                                axis_dir,
                                major_radius,
                                minor_radius,
                                ..
                            }) if !curved_full_rim => yang_rs::Surface::Torus {
                                center,
                                axis_dir: Vector3::new(axis_dir.x, axis_dir.y, axis_dir.z),
                                major_radius,
                                minor_radius,
                            },
                            _ => {
                                let reason = if matches!(face.surface, Some(Surface::Cone { .. })) {
                                    "curved lateral is an apex/frustum cone (full-circle rim; \
                                     no CDT re-entry)"
                                } else if matches!(face.surface, Some(Surface::Torus { .. }))
                                    && curved_full_rim
                                {
                                    "curved lateral is a canonical full-rim torus (full-circle \
                                     rim; no CDT re-entry)"
                                } else if face.inner_loops.is_empty() && outer_hes.len() == 4 {
                                    "curved lateral 4-edge non-structured outer loop (no CDT \
                                     re-entry for this surface)"
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
                        if std::env::var_os("KV14_SLICED_PROBE").is_some() {
                            let kind = |c: Curve| match c {
                                Curve::LineSegment => "Line",
                                Curve::Arc { .. } => "Arc",
                                Curve::Circle { .. } => "Circle",
                                Curve::EllipseArc { .. } => "EllipseArc",
                                Curve::HyperbolaArc { .. } => "HyperbolaArc",
                                Curve::SurfacePair { .. } => "SurfacePair",
                            };
                            eprintln!(
                                "[kv14-sliced-probe] face {f:?} surface {:?} 4-edge NON-structured pattern [{}, {}, {}, {}]",
                                face.surface,
                                kind(pattern.0),
                                kind(pattern.1),
                                kind(pattern.2),
                                kind(pattern.3)
                            );
                        }
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
