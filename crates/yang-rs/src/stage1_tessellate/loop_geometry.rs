//! Stage-1 curved-geometry helpers (PR-YR7): vector normalize/ortho-basis,
//! exact rim CCW tiebreak, planar outer-loop fan, 2D loop projection, and the
//! shared per-edge loop-polyline builders. Extracted move-only from
//! stage1_tessellate.rs (#159 F9 decomposition).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// Normalize a `[f64; 3]`; returns the input unchanged if its length is below
/// `TAU_WORK` (defensive — callers pass real surface normals / axes).
pub(crate) fn normalize3(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < cad_primitives::TAU_WORK {
        return v;
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

/// Deterministic orthonormal in-plane basis `(e1, e2)` for the plane with
/// (not-necessarily-unit) normal `n` (PR-YR7, spec §2 "critical coupling").
///
/// USED BY BOTH Stage-1 sampling AND [`BRep::eval_source`] — if these two
/// disagree, the bijection round-trip fails. Construction:
/// 1. `nu = normalize(n)`.
/// 2. Seed = the world axis with the SMALLEST `|nu_i|` (ties broken x<y<z) —
///    the axis least aligned with `nu`, for numerical stability.
/// 3. `e1 = normalize(seed − (seed·nu)·nu)` (Gram–Schmidt).
/// 4. `e2 = nu × e1`.
///
/// `e1` and `e2` are unit and orthogonal to `nu` (and to each other). Note
/// `ortho_basis(-n)` and `ortho_basis(n)` share the SAME `e1` (the projection
/// is invariant to flipping `nu`) but have OPPOSITE `e2` (since `e2 = nu × e1`)
/// — the opposite-rim twist the lateral tessellation must compensate for.
pub(crate) fn ortho_basis(n: Vector3) -> (Vector3, Vector3) {
    let nu = normalize3(n.as_array());
    let abs = [nu[0].abs(), nu[1].abs(), nu[2].abs()];
    // Seed = world axis with smallest |component| (tie-break x < y < z).
    let seed = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let sdotn = seed[0] * nu[0] + seed[1] * nu[1] + seed[2] * nu[2];
    let e1_raw = [
        seed[0] - sdotn * nu[0],
        seed[1] - sdotn * nu[1],
        seed[2] - sdotn * nu[2],
    ];
    let e1 = normalize3(e1_raw);
    // e2 = nu × e1.
    let e2 = [
        nu[1] * e1[2] - nu[2] * e1[1],
        nu[2] * e1[0] - nu[0] * e1[2],
        nu[0] * e1[1] - nu[1] * e1[0],
    ];
    (
        Vector3::new(e1[0], e1[1], e1[2]),
        Vector3::new(e2[0], e2[1], e2[2]),
    )
}

/// EXACT CCW tie-break for two rim points whose f64 frame angles COLLIDE
/// (ULP twins — spec `m8_holed_disc_coplanar_overlay` §8 increment 3): the
/// sign of the exact 2D cross product of their in-frame coordinates, computed
/// in rational arithmetic over the raw f64 inputs (products and sums of f64
/// values are exact in `RBig` — no rounding anywhere). `cross(a,b) > 0` means
/// `b` lies counterclockwise of `a` in the `(e1, e2)` frame, i.e. `a` orders
/// FIRST along increasing frame angle → `Less`. A zero cross (identical exact
/// direction; distinct rim points cannot subtend it) compares `Equal`.
///
/// Only valid for points whose angular separation is far below π (the callers
/// invoke it exclusively on bit-equal f64 angle keys, where the separation is
/// sub-ULP), since a bare cross sign cannot totally order antipodal points.
pub(crate) fn exact_rim_ccw_tiebreak(
    center: [f64; 3],
    e1: [f64; 3],
    e2: [f64; 3],
    pa: Point3,
    pb: Point3,
) -> std::cmp::Ordering {
    use crate::coplanar_overlay::rat;
    use dashu::rational::RBig;
    // Dev-only bisection neuter — gated out of release (F12): the env var is
    // read only under debug_assertions, so the release/WASM build always takes
    // the real tiebreak path.
    if cfg!(debug_assertions) && std::env::var_os("TIEBREAK_NEUTER").is_some() {
        return std::cmp::Ordering::Equal;
    }
    let frame_coords = |p: Point3| -> Option<(RBig, RBig)> {
        let a = p.as_array();
        let w = [
            rat(a[0]).ok()? - rat(center[0]).ok()?,
            rat(a[1]).ok()? - rat(center[1]).ok()?,
            rat(a[2]).ok()? - rat(center[2]).ok()?,
        ];
        let dot = |v: &[f64; 3]| -> Option<RBig> {
            Some(&w[0] * rat(v[0]).ok()? + &w[1] * rat(v[1]).ok()? + &w[2] * rat(v[2]).ok()?)
        };
        Some((dot(&e1)?, dot(&e2)?))
    };
    match (frame_coords(pa), frame_coords(pb)) {
        (Some((xa, ya)), Some((xb, yb))) => {
            let cross = &xa * &yb - &ya * &xb;
            if cross > RBig::ZERO {
                std::cmp::Ordering::Less
            } else if cross < RBig::ZERO {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        }
        // Non-finite input (never produced by the tessellation) — keep stable.
        _ => std::cmp::Ordering::Equal,
    }
}

/// PR-NC1: is the outer loop of a planar, all-LineSegment face **non-convex**
/// (does it have a reflex vertex)?
///
/// Builds `face_verts` from each outer-loop edge's `.start` (the same vertex
/// order the fan path uses), projects them into the plane's intrinsic 2D frame
/// (`ortho_basis(normal)` — the SAME projection the CDT path uses, so the
/// reflex test and the triangulation agree), then walks consecutive 2D cross
/// products. The loop's overall orientation is the sign of its signed area; any
/// turn whose cross product has the OPPOSITE sign is a reflex vertex ⇒
/// non-convex. A near-zero cross (collinear vertices) is not reflex — but it
/// IS fan-unsafe, see below.
///
/// PR-YR27 (unmasked latent, found by the yr5c chained-subtract adversary
/// once Finding 3 let the chain proceed): a CONVEX loop with a COLLINEAR
/// boundary run is also routed to the CDT. A previous boolean's output face
/// legitimately carries collinear boundary subdivisions (arrangement
/// vertices on a straight face edge, e.g. a tunnel wall's rim subdivided by
/// the neighbor cap's mesh); re-fed as input, the fan from vertex 0 emits a
/// ZERO-AREA triangle whenever a collinear chain includes vertex 0's own
/// boundary edge (`fan(v0, c, b)` over collinear `v0—c—b`). That degenerate
/// glue triangle pairs the mesh locally, but the NEXT exact arrangement
/// drops it (zero-area tris cannot be embedded), leaving a T-junction and a
/// NON-watertight kept set. The CDT triangulates the same ring with every
/// boundary sub-segment as a constraint and emits positive-area triangles
/// only. Strictly-convex hole-free loops (every fixture box) keep the
/// byte-for-byte fan path.
pub(crate) fn planar_outer_loop_fan_unsafe(
    f: &BRepFace,
    edges: &[BRepEdge],
    out_verts: &[Point3],
    normal: Vector3,
) -> bool {
    let pts2d = project_loop_2d(&f.outer_loop, edges, out_verts, normal);
    let m = pts2d.len();
    if m < 4 {
        // A triangle is always convex.
        return false;
    }

    // Loop orientation = sign of the 2D signed (shoelace) area.
    let mut area2 = 0.0;
    for i in 0..m {
        let a = pts2d[i];
        let b = pts2d[(i + 1) % m];
        area2 += a[0] * b[1] - b[0] * a[1];
    }
    // Degenerate (zero-area) projection: treat as convex (the fan path's
    // own degeneracy guard will reject it downstream).
    if area2.abs() < cad_primitives::TAU_WORK {
        return false;
    }
    let orient = area2.signum();

    // Tolerance scaled to the loop's area so it is invariant to model scale.
    let eps = area2.abs() * 1e-9;
    for i in 0..m {
        let prev = pts2d[(i + m - 1) % m];
        let cur = pts2d[i];
        let next = pts2d[(i + 1) % m];
        let d1 = [cur[0] - prev[0], cur[1] - prev[1]];
        let d2 = [next[0] - cur[0], next[1] - cur[1]];
        let cross = d1[0] * d2[1] - d1[1] * d2[0];
        // A turn opposite the loop orientation is a reflex vertex.
        if cross * orient < -eps {
            return true;
        }
        // PR-YR27: a (near-)zero turn is a collinear boundary run — convex,
        // but fan-UNSAFE (see the function docs): route to the CDT.
        if cross.abs() <= eps {
            return true;
        }
    }
    false
}

/// PR-NC1: project an edge-index loop's vertices (each loop edge's `.start`)
/// into the plane's intrinsic 2D frame `ortho_basis(normal)`. Returns the 2D
/// coordinates in loop order. The 3D point of vertex `v` projects to
/// `(p·e1, p·e2)` (the origin offset cancels for in-plane analysis).
pub(crate) fn project_loop_2d(
    loop_edges: &[u32],
    edges: &[BRepEdge],
    out_verts: &[Point3],
    normal: Vector3,
) -> Vec<[f64; 2]> {
    let (e1, e2) = ortho_basis(normal);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    loop_edges
        .iter()
        .map(|&e_idx| {
            let p = out_verts[edges[e_idx as usize].start as usize].as_array();
            [
                p[0] * e1a[0] + p[1] * e1a[1] + p[2] * e1a[2],
                p[0] * e2a[0] + p[1] * e2a[1] + p[2] * e2a[2],
            ]
        })
        .collect()
}

/// PR-NC1: tessellate a planar, all-LineSegment face that is **non-convex** or
/// has **inner loops** via a constrained Delaunay triangulation
/// (`cherchi_rs::cdt_polygon_with_holes`).
///
/// Projects the outer loop + every inner loop into the plane's intrinsic 2D
/// frame (`ortho_basis(normal)`, matching the reflex test), builds a *local*
/// `Point2` pool with a `local → global out_verts index` map, triangulates, and
/// maps the local tri indices back to global indices. Each output triangle is
/// wound to agree with the plane normal (reusing `orient_tri`, the same sign
/// rule the fan path uses).
///
/// Pushes **no** new vertices — the output indexes only into existing
/// `out_verts`, so the `TessellationMap` 1:1-on-boundary bijection is preserved
/// (no Steiner points, no boundary subdivision).
/// PR-KV6b-1: expand a B-Rep edge-index loop into its mesh-vertex polyline,
/// splicing each `Curve::Circle` edge's cached sample chain (arc chains are
/// open `[start … end]`, full circles closed seam rings). Edge traversal
/// direction is derived from loop continuity; the returned polyline lists
/// each boundary vertex ONCE (no closing duplicate).
pub(crate) fn loop_polyline(
    f_idx: usize,
    loop_edges: &[u32],
    edges: &[BRepEdge],
    chains: &std::collections::BTreeMap<u32, Vec<u32>>,
) -> Result<Vec<u32>, YangError> {
    Ok(loop_polyline_attributed(f_idx, loop_edges, edges, chains)?
        .into_iter()
        .map(|(v, _)| v)
        .collect())
}

/// [`loop_polyline`] with per-vertex EDGE ATTRIBUTION: each emitted polyline
/// vertex is paired with the index of the loop edge that emitted it (so the
/// polyline segment starting at vertex *i* lies on edge `out[i].1`). The
/// Stage-0 mixed-face overlay arm uses this to mark curved sub-chords (spec
/// `m8_mixed_loop_coplanar_overlay` §8).
pub(crate) fn loop_polyline_attributed(
    f_idx: usize,
    loop_edges: &[u32],
    edges: &[BRepEdge],
    chains: &std::collections::BTreeMap<u32, Vec<u32>>,
) -> Result<Vec<(u32, u32)>, YangError> {
    let malformed = |msg: String| YangError::MalformedTopology(format!("face {f_idx}: {msg}"));

    // Single full-circle / full-ellipse loop: the chain IS the (closed)
    // polyline.
    if loop_edges.len() == 1 {
        let e = &edges[loop_edges[0] as usize];
        if matches!(e.curve, Curve::Circle { .. } | Curve::Ellipse { .. }) && e.start == e.end {
            return chains
                .get(&loop_edges[0])
                .map(|c| c.iter().map(|&v| (v, loop_edges[0])).collect())
                .ok_or_else(|| malformed(format!("chain for edge {} not built", loop_edges[0])));
        }
    }

    // Expansion of one directed edge: the vertex sequence from its
    // traversal origin up to (EXCLUDING) its destination.
    let expand = |e_idx: u32, forward: bool| -> Result<Vec<u32>, YangError> {
        let e = &edges[e_idx as usize];
        match e.curve {
            Curve::LineSegment => Ok(vec![if forward { e.start } else { e.end }]),
            Curve::Circle { .. } | Curve::Ellipse { .. } | Curve::Hyperbola { .. } => {
                let chain = chains
                    .get(&e_idx)
                    .ok_or_else(|| malformed(format!("chain for edge {e_idx} not built")))?;
                if e.start == e.end {
                    return Err(malformed(format!(
                        "full-circle/full-ellipse edge {e_idx} inside a multi-edge loop"
                    )));
                }
                let mut seq: Vec<u32> = if forward {
                    chain[..chain.len() - 1].to_vec()
                } else {
                    chain[1..].iter().rev().copied().collect()
                };
                if seq.is_empty() {
                    seq.push(if forward { e.start } else { e.end });
                }
                Ok(seq)
            }
            _ => Err(malformed(format!(
                "loop edge {e_idx} carries an unsupported curve for Stage-1 ingestion"
            ))),
        }
    };

    // Walk with continuity, trying the first edge forward then backward.
    'attempt: for first_forward in [true, false] {
        let e0 = &edges[loop_edges[0] as usize];
        let mut cur = if first_forward { e0.start } else { e0.end };
        let mut poly: Vec<(u32, u32)> = Vec::new();
        for &e_idx in loop_edges {
            let e = &edges[e_idx as usize];
            let forward = if e.start == cur {
                true
            } else if e.end == cur {
                false
            } else {
                continue 'attempt;
            };
            poly.extend(expand(e_idx, forward)?.into_iter().map(|v| (v, e_idx)));
            cur = if forward { e.end } else { e.start };
        }
        // Closure: the walk must return to its origin.
        if cur == poly[0].0 {
            return Ok(poly);
        }
    }
    Err(malformed("loop is not edge-continuous".to_string()))
}
