//! #169 Phase B — the SPLICE LOOP (Yang 2025 §4.4.1 "Mesh updating").
//!
//! The layers below this one were each built and de-risked separately:
//!
//! * [`SurfaceChart`] — per-operand parametric charts (`stage4_project`),
//! * [`patch_from_cycles_shifted`] — 3D boundary cycles → a 2D
//!   [`Patch`](crate::stage4_update::Patch) plus
//!   the index map back to mesh vertices (`stage4_project`),
//! * [`two_sided_conformal_update_lifted`] — re-triangulate BOTH operands'
//!   patches against ONE shared curve and verify the seam in 3D
//!   (`stage4_update`).
//!
//! This module is the loop that joins them to the real forward mesh: given two
//! adjacent patches and the mesh edges forming their shared seam, it
//!
//! 1. orders each side's seam vertices into a chain ([`ordered_seam_side`]),
//! 2. merges the two chains into ONE shared curve carrying every vertex either
//!    side contributes ([`merge_seam_chains`]) — this is what repairs the
//!    C0044-class defect where one side subdivides the seam and the other does
//!    not: the extra vertex becomes a Fig-11(a) boundary-edge SPLIT on the
//!    coarse side,
//! 3. unwraps the cylinder `θ = ±π` seam ([`unwrap_theta`]) — declared to be
//!    this loop's job by both the chart and the extraction layer, which
//!    explicitly decline it,
//! 4. runs the two-sided driver, and
//! 5. maps the result back into MESH index space, giving each shared-curve
//!    point **one** mesh vertex used by both sides ([`splice_seam_pair`]).
//!
//! Step 5 is the part that makes the output manifold, and it is exactly what
//! the 2026-08-05 gated trial got wrong: a bare `collapse_vertex` rewrote
//! triangle indices without rebuilding the patch, so the surrounding fan was
//! left inconsistent and Stage 6's 2-manifold gate fired. Here the patch is
//! genuinely re-triangulated in the parametric domain, and the seam vertices
//! are *shared*, not merely coincident.
//!
//! # Scope of this increment
//!
//! Pure and deterministic; [`apply_splice`] is the only mutating entry point.
//! **WIRED (2026-08-06, N2-3b step 2)** behind `YANG_MESHUP_ENABLE`, from
//! `stage5_topology::run_meshup_splice_passes` at the end of Stage 4. A
//! gate-OFF run never enters that block, so it stays byte-identical.
//!
//! The SELECTOR is not baked in here: the entry points take the patch pair and
//! seam edges as arguments. The wiring drives them from
//! [`crate::stage4_project::detect_nonmanifold_seams`].
//!
//! # MEASURED 2026-08-06: that selector has ZERO customers on this class
//!
//! Gate-ON on all four intended cases (F0067, R0011, R0074, R0085) reports
//! `no non-manifold seam regions` at this point in the pipeline — the block
//! runs, finds nothing, and changes nothing. Their wall is
//! `TessellationFailed "ring rejected by CDT"`, a Stage-5/6 face-RING defect,
//! not a half-edge imbalance in the Stage-4 mesh. So the detector's own claim
//! to be "consumed by the splice loop" is not, on this evidence, a claim about
//! these cases.
//!
//! **And the full §4.4.1 text (`refs/text/yang2025_hybrid_boolean.txt:552-570`,
//! right column) names what this module still gets wrong.** The paper does not
//! re-triangulate a patch against its EXISTING seam: *"The intersection curves
//! on the parametric surfaces are mapped to the meshes … **Then we set
//! r_A = r_B = r**, so that the two polylines in the meshes coincide with the
//! intersection curve"* — the exact analytic curve is the AUTHORITY and TRIMS
//! both meshes, and CDT is applied to that trimmed result. Flip-freeness is
//! justified by *"the intersection curves are regular"*, i.e. by the curve's
//! own monotone sampling.
//!
//! This module instead derives the seam polyline from the MESH's relocated
//! vertex chain ([`merge_seam_chains`]) and keeps the patch's existing cycles
//! as the CDT boundary. When Stage 4 has moved a vertex past its neighbour
//! (F0067: a 3.7e-3 relocation against a 6.4e-4 segment), that chain is already
//! self-crossing, and re-triangulating from it preserves the crossing rather
//! than repairing it. Sourcing the polyline from `intersection_curves` — the
//! exact per-edge `Curve` map already in scope at the wiring site — is the next
//! increment, and it is a change of AUTHORITY, not a tolerance.
//!
//! # Known limitation, stated loudly rather than papered over
//!
//! [`patch_from_cycles_shifted`] projects a patch's BOUNDARY cycles only, so a
//! spliced patch is re-triangulated from its boundary plus the curve, and any
//! interior vertices it had are dropped. For a **planar** patch those vertices
//! are geometrically redundant — the CDT reproduces the same plane — so this is
//! faithful to §4.4.1 ("the triangulation can be totally operated in the
//! parametric domain"). For a **curved** patch they carry chord fidelity, and
//! dropping them would silently coarsen the surface, so
//! [`splice_seam_pair`] refuses with [`SpliceError::CurvedPatchInteriorVertices`]
//! instead. Carrying interior vertices through needs them threaded into the
//! primitive's `interior` list, and interacts with the N2-2 `d(T)` recompute
//! the paper asks for in the same paragraph — a later increment, not a band.

use crate::brep::{TriangleAttribution, TriangleAttributionMap};
use crate::stage4_project::{patch_from_cycles_shifted, SurfaceChart};
use crate::stage4_update::{
    two_sided_conformal_update_lifted, MeshUpdateOpts, Polyline, TwoSidedError,
};
use crate::Surface;
use cad_primitives::{Point2, Point3};
use cherchi_rs::Mesh;
use std::collections::{BTreeMap, BTreeSet};

const TWO_PI: f64 = std::f64::consts::TAU;

/// Which operand's patch a failure came from (mirrors [`TwoSidedError`]'s
/// `SideA` / `SideB` split, so a splice failure names the same side the driver
/// would).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    A,
    B,
}

/// One side of the splice: a patch of the forward mesh, described by the three
/// things the update needs and nothing else.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SplicePatch {
    /// Boundary cycles as ordered MESH-vertex chains; `cycles[0]` is the outer
    /// boundary and the rest are holes — the order
    /// [`crate::stage4_correct::PatchInfo`] already stores them in. (That type
    /// stores cycles as directed edge pairs `(s, e)`; the caller takes the `s`
    /// of each, which is the same chain.)
    pub cycles: Vec<Vec<u32>>,
    /// Indices into `mesh.tris` of the triangles this patch owns. Replaced
    /// wholesale by the update, so this must be the patch's COMPLETE triangle
    /// set (`flood_fill_patches`'s `tri_indices`).
    pub tris: Vec<u32>,
    /// The analytic surface the patch inherits, which becomes its chart.
    pub surface: Surface,
}

/// The splice's result, entirely in MESH index space and ready for
/// [`apply_splice`]. Nothing here has been written to the mesh yet.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpliceOutput {
    /// The mesh vertex count the plan was built against. [`apply_splice`]
    /// refuses if the mesh has changed since, because every `new_verts` index
    /// below is `base_vert + i`.
    pub base_vert: u32,
    /// World positions to append to `mesh.verts`.
    pub new_verts: Vec<Point3>,
    /// Replacement triangles for each side, in mesh vertex indices.
    pub tris_a: Vec<[u32; 3]>,
    pub tris_b: Vec<[u32; 3]>,
    /// The old triangles each side replaces (indices into `mesh.tris`).
    pub old_tris_a: Vec<u32>,
    pub old_tris_b: Vec<u32>,
    /// The shared seam in mesh indices, in curve order. One vertex per curve
    /// point, referenced by BOTH sides — the identity that makes the
    /// reassembled mesh 2-manifold.
    pub seam: Vec<u32>,
}

/// Why a splice failed. Every variant is a P9/P10 LOUD stop: we never emit a
/// silently non-conformal, silently coarsened, or silently re-oriented patch.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SpliceError {
    /// The patch's surface has no chart yet (Sphere / Cone / Torus). The caller
    /// leaves such a patch untouched, byte-identical.
    UnsupportedSurface(Side),
    /// A cylinder patch wraps the axis completely, so no `θ` branch unrolls it
    /// into a simple planar domain.
    PatchEncirclesAxis(Side),
    /// A seam vertex could not be placed on a consistent `θ` branch.
    ThetaBranchUnresolved(Side, u32),
    /// `patch_from_cycles_shifted` rejected the patch (short cycle, bad index,
    /// or a boundary that projects to zero area).
    MalformedPatch(Side),
    /// None of the given seam edges lie on this side's boundary cycles.
    SeamNotOnPatch(Side),
    /// This side's seam edges do not form ONE simple path or cycle (a branch or
    /// a T-junction). We do not guess which branch is the curve.
    SeamNotSimple(Side),
    /// One side's seam closes into a loop and the other's does not.
    SeamTopologyMismatch,
    /// A seam vertex contributed by one side lies farther than `d_eps` from the
    /// other side's seam polyline, so the two are not the same curve.
    SeamPointOffReference { vertex: u32, dist: f64 },
    /// A seam vertex from one side lands on top of a DIFFERENT vertex of the
    /// other side's chain. Two distinct mesh vertices at one position is the
    /// defect, not something to merge away here.
    SeamPointCoincident { vertex: u32, other: u32 },
    /// A curved patch has interior vertices, which this increment would drop —
    /// see the module docs.
    CurvedPatchInteriorVertices { side: Side, count: usize },
    /// An interior vertex of this patch is also referenced by a triangle
    /// OUTSIDE it. Re-triangulating would orphan that reference into a
    /// T-junction.
    ForeignInteriorVertex { side: Side, vertex: u32 },
    /// The update moved an existing patch vertex. With boundary-only patches
    /// the Fig-11 arms never do this (a boundary merge keeps the vertex fixed
    /// and snaps the curve point to it), so this is a broken assumption, not a
    /// tolerance to widen.
    UnexpectedVertexMove { side: Side, vertex: u32 },
    /// The two sides realized one curve point at two DIFFERENT existing mesh
    /// vertices. Their positions agree (the driver checked), but distinct
    /// indices at one position is a coincident-vertex defect upstream.
    SeamVertexIdentityConflict { point: usize, a: u32, b: u32 },
    /// The re-triangulated patch has no well-defined orientation to match
    /// against the original (a degenerate area vector on one side).
    DegenerateOrientation(Side),
    /// The two-sided driver failed.
    Update(TwoSidedError),
    /// A side's triangles do not all share one attribution, so the replacement
    /// triangles have no unambiguous attribution to inherit.
    MixedAttribution(Side),
    /// [`apply_splice`] was handed a plan built against a different mesh.
    StalePlan { expected: u32, actual: u32 },
}

// ---------------------------------------------------------------------------
// Step 1 — order one side's seam vertices into a chain.
// ---------------------------------------------------------------------------

/// Order the seam vertices ON `cycles` into a single chain.
///
/// `seam_edges` is the candidate edge set (canonical `(min, max)`) the selector
/// nominated — e.g. a [`crate::stage4_project::SeamRegion`]'s `edges`. Only the
/// ones that actually appear as consecutive pairs on this patch's cycles
/// participate, so the same edge set can be handed to both sides.
///
/// Returns the chain and whether it closes into a loop. For an open chain the
/// walk starts at the lower-indexed endpoint; for a closed one at the lowest
/// vertex, stepping to its lower-indexed neighbour — both deterministic.
///
/// A vertex of seam-degree > 2 means the nominated edges branch; we refuse
/// rather than pick a branch ([`SpliceError::SeamNotSimple`]).
pub(crate) fn ordered_seam_side(
    cycles: &[Vec<u32>],
    seam_edges: &BTreeSet<(u32, u32)>,
    side: Side,
) -> Result<(Vec<u32>, bool), SpliceError> {
    let mut adj: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for cyc in cycles {
        let n = cyc.len();
        for i in 0..n {
            let (u, v) = (cyc[i], cyc[(i + 1) % n]);
            if u == v {
                continue;
            }
            let key = if u < v { (u, v) } else { (v, u) };
            if seam_edges.contains(&key) {
                adj.entry(u).or_default().insert(v);
                adj.entry(v).or_default().insert(u);
            }
        }
    }
    if adj.is_empty() {
        return Err(SpliceError::SeamNotOnPatch(side));
    }
    if adj.values().any(|n| n.len() > 2) {
        return Err(SpliceError::SeamNotSimple(side));
    }
    let ends: Vec<u32> = adj
        .iter()
        .filter(|(_, n)| n.len() == 1)
        .map(|(&v, _)| v)
        .collect();
    let (start, next, closed) = match ends.len() {
        // Closed loop: every vertex has exactly two seam neighbours.
        0 => {
            let &s = adj.keys().next().expect("non-empty");
            let n = *adj[&s].iter().next().expect("degree 2");
            (s, n, true)
        }
        // Open path.
        2 => {
            let s = ends[0];
            let n = *adj[&s].iter().next().expect("degree 1");
            (s, n, false)
        }
        _ => return Err(SpliceError::SeamNotSimple(side)),
    };

    let mut chain = vec![start, next];
    let mut prev = start;
    let mut cur = next;
    // Ends when `cur` has no onward neighbour (open end) or we return to
    // `start` (the loop closed).
    while let Some(step) = adj[&cur].iter().copied().find(|&w| w != prev) {
        if step == start {
            break; // closed loop complete
        }
        chain.push(step);
        prev = cur;
        cur = step;
        // A simple chain can never revisit a vertex; guard against a malformed
        // adjacency looping forever rather than trusting it.
        if chain.len() > adj.len() {
            return Err(SpliceError::SeamNotSimple(side));
        }
    }
    if chain.len() != adj.len() {
        // Some seam edges form a second, disconnected run.
        return Err(SpliceError::SeamNotSimple(side));
    }
    Ok((chain, closed))
}

// ---------------------------------------------------------------------------
// Step 2 — merge the two sides' chains into ONE shared curve.
// ---------------------------------------------------------------------------

/// Distance from `p` to segment `ab` in 3D, with the clamped parameter `t`.
fn point_segment3(p: Point3, a: Point3, b: Point3) -> (f64, f64) {
    let ab = [b.x() - a.x(), b.y() - a.y(), b.z() - a.z()];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    if len2 == 0.0 {
        return (dist3(p, a), 0.0);
    }
    let w = [p.x() - a.x(), p.y() - a.y(), p.z() - a.z()];
    let t = ((w[0] * ab[0] + w[1] * ab[1] + w[2] * ab[2]) / len2).clamp(0.0, 1.0);
    let proj = Point3::new(a.x() + t * ab[0], a.y() + t * ab[1], a.z() + t * ab[2]);
    (dist3(p, proj), t)
}

fn dist3(a: Point3, b: Point3) -> f64 {
    let d = [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// Merge the two sides' seam chains into ONE ordered shared curve containing
/// every vertex either side contributes.
///
/// `chain_a` is the reference: its own order is preserved exactly, and each
/// vertex only `chain_b` has is inserted at the position where it projects onto
/// A's 3D polyline. This is what turns the C0044-class mismatch — A subdivides
/// the seam at `m`, B does not — into a single curve `a → m → b` that BOTH
/// sides then re-triangulate against, so `m` becomes a Fig-11(a) boundary-edge
/// split on B's side.
///
/// A B-only vertex farther than `d_eps` from A's polyline is not on the same
/// curve ([`SpliceError::SeamPointOffReference`]), and one landing on top of an
/// A vertex is a coincident-vertex defect
/// ([`SpliceError::SeamPointCoincident`]) — both loud.
pub(crate) fn merge_seam_chains(
    verts: &[Point3],
    chain_a: &[u32],
    closed_a: bool,
    chain_b: &[u32],
    closed_b: bool,
    d_eps: f64,
) -> Result<(Vec<u32>, bool), SpliceError> {
    if closed_a != closed_b {
        return Err(SpliceError::SeamTopologyMismatch);
    }
    let in_a: BTreeSet<u32> = chain_a.iter().copied().collect();
    let extras: Vec<u32> = chain_b
        .iter()
        .copied()
        .filter(|v| !in_a.contains(v))
        .collect();
    if extras.is_empty() {
        return Ok((chain_a.to_vec(), closed_a));
    }

    let seg_count = if closed_a {
        chain_a.len()
    } else {
        chain_a.len() - 1
    };
    // (segment index, t along it, vertex) for each B-only vertex.
    let mut placed: Vec<(usize, f64, u32)> = Vec::with_capacity(extras.len());
    for v in extras {
        let p = verts[v as usize];
        let mut best: Option<(f64, usize, f64)> = None;
        for s in 0..seg_count {
            let a = verts[chain_a[s] as usize];
            let b = verts[chain_a[(s + 1) % chain_a.len()] as usize];
            let (d, t) = point_segment3(p, a, b);
            if best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, s, t));
            }
        }
        let (d, s, t) = best.expect("seg_count >= 1");
        if d > d_eps {
            return Err(SpliceError::SeamPointOffReference { vertex: v, dist: d });
        }
        // Landing ON an endpoint of the reference chain means two distinct mesh
        // vertices share a position — not ours to merge away. `t` is the
        // normalized parameter, so `|t - end| * seg_len` is the distance along
        // the segment from that endpoint.
        let seg_len = dist3(
            verts[chain_a[s] as usize],
            verts[chain_a[(s + 1) % chain_a.len()] as usize],
        );
        for (&cv, end_t) in [
            (&chain_a[s], 0.0f64),
            (&chain_a[(s + 1) % chain_a.len()], 1.0f64),
        ] {
            if (t - end_t).abs() * seg_len <= d_eps {
                return Err(SpliceError::SeamPointCoincident {
                    vertex: v,
                    other: cv,
                });
            }
        }
        placed.push((s, t, v));
    }
    // Deterministic: by segment, then along it, then by index on an exact tie.
    placed.sort_by(|x, y| {
        x.0.cmp(&y.0)
            .then(x.1.partial_cmp(&y.1).expect("finite"))
            .then(x.2.cmp(&y.2))
    });

    let mut out = Vec::with_capacity(chain_a.len() + placed.len());
    for (i, &v) in chain_a.iter().enumerate() {
        out.push(v);
        // `placed` is sorted by (segment, t), so the extras for segment `i`
        // come out already in curve order.
        out.extend(
            placed
                .iter()
                .filter(|&&(s, _, _)| s == i)
                .map(|&(_, _, ev)| ev),
        );
    }
    Ok((out, closed_a))
}

// ---------------------------------------------------------------------------
// Step 3 — cylinder θ = ±π seam unwrapping (this loop's declared job).
// ---------------------------------------------------------------------------

/// Choose a `θ` branch per mesh vertex so a cylinder patch that straddles
/// `θ = ±π` unrolls into a simple (non-self-crossing) planar domain.
///
/// Returns `v → shift`, where every shift is a multiple of `2π`. Because
/// [`SurfaceChart::lift`] is `2π`-periodic in `θ`, applying these shifts is a
/// **no-op in world space** — it changes only which branch the 2D CDT sees.
/// That is the whole safety argument for doing it here.
///
/// A `Plane` chart needs no branch and yields an empty map. A cylinder patch
/// that encircles the axis has no branch that unrolls it
/// ([`SpliceError::PatchEncirclesAxis`]).
///
/// Hole cycles are placed on the branch whose mean `θ` is nearest the outer
/// loop's — forced, not chosen: a hole lies inside the outer loop in the chart,
/// so its `θ` must fall within the outer loop's span.
pub(crate) fn unwrap_theta(
    chart: &SurfaceChart,
    verts: &[Point3],
    cycles: &[Vec<u32>],
    side: Side,
) -> Result<BTreeMap<u32, f64>, SpliceError> {
    if matches!(chart, SurfaceChart::Plane { .. }) {
        return Ok(BTreeMap::new());
    }
    let theta = |v: u32| chart.project(verts[v as usize]).x();

    // Unwrap one cycle in isolation; returns per-position unwrapped θ.
    let unwrap_cycle = |cyc: &[u32]| -> Result<Vec<f64>, SpliceError> {
        let mut u = Vec::with_capacity(cyc.len());
        u.push(theta(cyc[0]));
        for i in 1..cyc.len() {
            let t = theta(cyc[i]);
            let k = ((u[i - 1] - t) / TWO_PI).round();
            u.push(t + k * TWO_PI);
        }
        // Closing back onto the first vertex must land on the SAME branch;
        // any other multiple of 2π means the loop encircles the axis.
        let t0 = theta(cyc[0]);
        let k_close = ((u[cyc.len() - 1] - t0) / TWO_PI).round();
        if (t0 + k_close * TWO_PI - u[0]).abs() > TWO_PI * 0.5 {
            return Err(SpliceError::PatchEncirclesAxis(side));
        }
        Ok(u)
    };

    let outer_u = unwrap_cycle(&cycles[0])?;
    let outer_mean = outer_u.iter().sum::<f64>() / outer_u.len() as f64;

    let mut shift: BTreeMap<u32, f64> = BTreeMap::new();
    let mut record = |v: u32, s: f64| -> Result<(), SpliceError> {
        match shift.get(&v) {
            // The same vertex reached on two branches has no consistent choice.
            Some(&prev) if (prev - s).abs() > TWO_PI * 0.5 => {
                Err(SpliceError::ThetaBranchUnresolved(side, v))
            }
            _ => {
                shift.insert(v, s);
                Ok(())
            }
        }
    };
    for (i, &v) in cycles[0].iter().enumerate() {
        record(v, outer_u[i] - theta(v))?;
    }
    for hole in &cycles[1..] {
        let hu = unwrap_cycle(hole)?;
        let hmean = hu.iter().sum::<f64>() / hu.len() as f64;
        let k = ((outer_mean - hmean) / TWO_PI).round();
        for (i, &v) in hole.iter().enumerate() {
            record(v, hu[i] + k * TWO_PI - theta(v))?;
        }
    }
    Ok(shift)
}

/// Extend a `θ` shift map to the shared seam vertices that are NOT on this
/// side's cycles (the ones the other side contributed — they lie on this side's
/// boundary EDGES, so their branch is the branch of their chain neighbours).
///
/// Forward pass, then a backward pass for a chain that starts with an unknown.
fn extend_shift_along_chain(
    chart: &SurfaceChart,
    verts: &[Point3],
    chain: &[u32],
    shift: &mut BTreeMap<u32, f64>,
    side: Side,
) -> Result<(), SpliceError> {
    if matches!(chart, SurfaceChart::Plane { .. }) {
        return Ok(());
    }
    let theta = |v: u32| chart.project(verts[v as usize]).x();
    let unwrapped = |v: u32, s: &BTreeMap<u32, f64>| s.get(&v).map(|k| theta(v) + k);

    for pass in 0..2 {
        let order: Vec<usize> = if pass == 0 {
            (0..chain.len()).collect()
        } else {
            (0..chain.len()).rev().collect()
        };
        let mut anchor: Option<f64> = None;
        for i in order {
            let v = chain[i];
            match unwrapped(v, shift) {
                Some(u) => anchor = Some(u),
                None => {
                    if let Some(a) = anchor {
                        let t = theta(v);
                        let k = ((a - t) / TWO_PI).round();
                        shift.insert(v, k * TWO_PI);
                        anchor = Some(t + k * TWO_PI);
                    }
                }
            }
        }
    }
    if let Some(&v) = chain.iter().find(|v| !shift.contains_key(v)) {
        return Err(SpliceError::ThetaBranchUnresolved(side, v));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Steps 4-5 — the splice itself.
// ---------------------------------------------------------------------------

/// Area vector (twice the signed area, summed) of a triangle list.
fn area_vector(tris: &[[u32; 3]], pos: &dyn Fn(u32) -> Point3) -> [f64; 3] {
    let mut acc = [0.0f64; 3];
    for t in tris {
        let (p0, p1, p2) = (pos(t[0]), pos(t[1]), pos(t[2]));
        let u = [p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z()];
        let v = [p2.x() - p0.x(), p2.y() - p0.y(), p2.z() - p0.z()];
        acc[0] += u[1] * v[2] - u[2] * v[1];
        acc[1] += u[2] * v[0] - u[0] * v[2];
        acc[2] += u[0] * v[1] - u[1] * v[0];
    }
    acc
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Vertices a patch's triangles reference that are not on its boundary cycles.
fn interior_vertices(mesh: &Mesh, tris: &[u32], cycles: &[Vec<u32>]) -> BTreeSet<u32> {
    let on_boundary: BTreeSet<u32> = cycles.iter().flatten().copied().collect();
    let mut out = BTreeSet::new();
    for &t in tris {
        for &v in &mesh.tris[t as usize] {
            if !on_boundary.contains(&v) {
                out.insert(v);
            }
        }
    }
    out
}

/// Indices of the patches whose boundary cycles carry at least one of `edges`.
///
/// This resolves a detected seam region to the patch PAIR that must be
/// re-triangulated together. It keys on the region's actual edges rather than
/// on [`crate::stage4_project::SeamRegion`]'s attribution keys, because an
/// attribution `(input, face)` can name SEVERAL disconnected patches (one input
/// face fragmented by the arrangement) — the edges name exactly one pair.
///
/// A result other than exactly two patches is the caller's signal to leave the
/// region alone: one means the seam's partner is not a patch we hold, and more
/// than two means the region spans a junction rather than a single seam.
pub(crate) fn patches_on_seam(patches: &[SplicePatch], edges: &BTreeSet<(u32, u32)>) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, p) in patches.iter().enumerate() {
        let hit = p.cycles.iter().any(|cyc| {
            let n = cyc.len();
            (0..n).any(|k| {
                let (u, v) = (cyc[k], cyc[(k + 1) % n]);
                edges.contains(&if u < v { (u, v) } else { (v, u) })
            })
        });
        if hit {
            out.push(i);
        }
    }
    out
}

/// Splice one patch pair across their shared seam: the §4.4.1 mesh update,
/// applied to the real forward mesh.
///
/// `seam_edges` nominates the shared boundary (canonical `(min, max)` mesh
/// edges) — typically a [`crate::stage4_project::SeamRegion`]'s `edges`.
///
/// Pure: it reads `mesh` and returns a [`SpliceOutput`] in mesh index space.
/// [`apply_splice`] performs the mutation.
pub(crate) fn splice_seam_pair(
    mesh: &Mesh,
    patch_a: &SplicePatch,
    patch_b: &SplicePatch,
    seam_edges: &BTreeSet<(u32, u32)>,
    opts: MeshUpdateOpts,
    conformal_tol: f64,
) -> Result<SpliceOutput, SpliceError> {
    // ---- Charts. --------------------------------------------------------
    let chart_a =
        SurfaceChart::new(patch_a.surface).ok_or(SpliceError::UnsupportedSurface(Side::A))?;
    let chart_b =
        SurfaceChart::new(patch_b.surface).ok_or(SpliceError::UnsupportedSurface(Side::B))?;

    // ---- Interior-vertex guards (module docs). --------------------------
    for (side, patch) in [(Side::A, patch_a), (Side::B, patch_b)] {
        let interior = interior_vertices(mesh, &patch.tris, &patch.cycles);
        if !interior.is_empty() && !matches!(patch.surface, Surface::Plane { .. }) {
            return Err(SpliceError::CurvedPatchInteriorVertices {
                side,
                count: interior.len(),
            });
        }
        // An interior vertex referenced from OUTSIDE the patch would be
        // orphaned into a T-junction by re-triangulating without it.
        if !interior.is_empty() {
            let own: BTreeSet<u32> = patch.tris.iter().copied().collect();
            for (t, tri) in mesh.tris.iter().enumerate() {
                if own.contains(&(t as u32)) {
                    continue;
                }
                if let Some(&v) = tri.iter().find(|v| interior.contains(v)) {
                    return Err(SpliceError::ForeignInteriorVertex { side, vertex: v });
                }
            }
        }
    }

    // ---- Steps 1-2: the ONE shared seam curve. --------------------------
    let (chain_a, closed_a) = ordered_seam_side(&patch_a.cycles, seam_edges, Side::A)?;
    let (chain_b, closed_b) = ordered_seam_side(&patch_b.cycles, seam_edges, Side::B)?;
    let (shared, closed) = merge_seam_chains(
        &mesh.verts,
        &chain_a,
        closed_a,
        &chain_b,
        closed_b,
        opts.d_eps,
    )?;

    // ---- Step 3: θ branches, patch cycles first then the seam. ----------
    let mut shift_a = unwrap_theta(&chart_a, &mesh.verts, &patch_a.cycles, Side::A)?;
    let mut shift_b = unwrap_theta(&chart_b, &mesh.verts, &patch_b.cycles, Side::B)?;
    extend_shift_along_chain(&chart_a, &mesh.verts, &shared, &mut shift_a, Side::A)?;
    extend_shift_along_chain(&chart_b, &mesh.verts, &shared, &mut shift_b, Side::B)?;

    // ---- Step 4: parametric patches + the shared curve in both charts. --
    let (p2a, back_a) = patch_from_cycles_shifted(&chart_a, &mesh.verts, &patch_a.cycles, &shift_a)
        .ok_or(SpliceError::MalformedPatch(Side::A))?;
    let (p2b, back_b) = patch_from_cycles_shifted(&chart_b, &mesh.verts, &patch_b.cycles, &shift_b)
        .ok_or(SpliceError::MalformedPatch(Side::B))?;

    let project_chain = |chart: &SurfaceChart, shift: &BTreeMap<u32, f64>| -> Polyline {
        Polyline {
            points: shared
                .iter()
                .map(|&v| {
                    let uv = chart.project(mesh.verts[v as usize]);
                    Point2::new(uv.x() + shift.get(&v).copied().unwrap_or(0.0), uv.y())
                })
                .collect(),
            closed,
        }
    };
    let curve_a = project_chain(&chart_a, &shift_a);
    let curve_b = project_chain(&chart_b, &shift_b);

    // Both sides sample the SAME ordered vertex list, so the driver's
    // `SeamLengthMismatch` is unreachable by construction.
    let update = two_sided_conformal_update_lifted(
        &p2a,
        |uv| chart_a.lift(uv),
        &curve_a,
        &p2b,
        |uv| chart_b.lift(uv),
        &curve_b,
        opts,
        conformal_tol,
    )
    .map_err(SpliceError::Update)?;

    // ---- Step 5: back into MESH index space. ----------------------------
    // No existing patch vertex may have moved: with boundary-only patches the
    // Fig-11 arms keep them fixed.
    for (side, p2, upd) in [(Side::A, &p2a, &update.a), (Side::B, &p2b, &update.b)] {
        let back = if side == Side::A { &back_a } else { &back_b };
        for (i, &v) in back.iter().enumerate() {
            if upd.verts[i] != p2.verts[i] {
                return Err(SpliceError::UnexpectedVertexMove { side, vertex: v });
            }
        }
    }

    let base_vert = mesh.verts.len() as u32;
    let mut new_verts: Vec<Point3> = Vec::new();
    // patch-vertex index -> mesh vertex, per side.
    let mut map_a: BTreeMap<u32, u32> = BTreeMap::new();
    let mut map_b: BTreeMap<u32, u32> = BTreeMap::new();
    for (i, &v) in back_a.iter().enumerate() {
        map_a.insert(i as u32, v);
    }
    for (i, &v) in back_b.iter().enumerate() {
        map_b.insert(i as u32, v);
    }

    // The seam FIRST: each curve point gets ONE mesh vertex used by both sides.
    // This is the identity that makes the reassembly manifold; without it the
    // two sides merely coincide, which is what Stage 6 rejects.
    let mut seam_mesh: Vec<u32> = Vec::with_capacity(update.seam.len());
    for (i, &(ia, ib)) in update.seam.iter().enumerate() {
        let ea = (ia as usize) < back_a.len();
        let eb = (ib as usize) < back_b.len();
        let m = match (ea, eb) {
            (true, true) => {
                let (va, vb) = (back_a[ia as usize], back_b[ib as usize]);
                if va != vb {
                    return Err(SpliceError::SeamVertexIdentityConflict {
                        point: i,
                        a: va,
                        b: vb,
                    });
                }
                va
            }
            // One side already has the vertex; the other split an edge to reach
            // it. The driver verified they land at the same world point, so the
            // split foot IS that vertex.
            (true, false) => back_a[ia as usize],
            (false, true) => back_b[ib as usize],
            // Neither side had it: allocate one shared new vertex.
            (false, false) => {
                let p = chart_a.lift(update.a.verts[ia as usize]);
                new_verts.push(p);
                base_vert + new_verts.len() as u32 - 1
            }
        };
        map_a.insert(ia, m);
        map_b.insert(ib, m);
        seam_mesh.push(m);
    }

    // Remaining new vertices (Fig-11 interior inserts) belong to one side only.
    for (map, upd, back, chart) in [
        (&mut map_a, &update.a, &back_a, &chart_a),
        (&mut map_b, &update.b, &back_b, &chart_b),
    ] {
        for i in back.len()..upd.verts.len() {
            if map.contains_key(&(i as u32)) {
                continue;
            }
            new_verts.push(chart.lift(upd.verts[i]));
            map.insert(i as u32, base_vert + new_verts.len() as u32 - 1);
        }
    }

    // ---- Orientation: match each side's ORIGINAL outward sense. ----------
    let pos_of = |v: u32| -> Point3 {
        if v < base_vert {
            mesh.verts[v as usize]
        } else {
            new_verts[(v - base_vert) as usize]
        }
    };
    let mut out_tris: Vec<Vec<[u32; 3]>> = Vec::with_capacity(2);
    for (side, upd, map, patch) in [
        (Side::A, &update.a, &map_a, patch_a),
        (Side::B, &update.b, &map_b, patch_b),
    ] {
        // The map is TOTAL over `upd.verts` by construction: `0..back.len()`
        // is filled from the index map, and `back.len()..` by the seam pass
        // plus the leftover-inserts pass above. `upd.tris` cannot index outside
        // that pool, so a miss here would be a broken invariant, not input.
        let at = |i: u32| -> u32 {
            *map.get(&i)
                .expect("patch-vertex map is total over the update's vertex pool")
        };
        let mut tris: Vec<[u32; 3]> = upd
            .tris
            .iter()
            .map(|t| [at(t[0]), at(t[1]), at(t[2])])
            .collect();
        let old: Vec<[u32; 3]> = patch.tris.iter().map(|&t| mesh.tris[t as usize]).collect();
        let want = area_vector(&old, &|v| mesh.verts[v as usize]);
        let got = area_vector(&tris, &pos_of);
        let d = dot3(want, got);
        // A chart's in-plane basis has arbitrary handedness relative to the
        // surface normal, so the CDT's CCW is not necessarily the patch's
        // outward sense. Decide it by MEASUREMENT against the original patch
        // rather than by assuming the basis convention.
        if d == 0.0 || !d.is_finite() {
            return Err(SpliceError::DegenerateOrientation(side));
        }
        if d < 0.0 {
            for t in &mut tris {
                t.swap(1, 2);
            }
        }
        out_tris.push(tris);
    }
    let tris_b = out_tris.pop().expect("two sides");
    let tris_a = out_tris.pop().expect("two sides");

    Ok(SpliceOutput {
        base_vert,
        new_verts,
        tris_a,
        tris_b,
        old_tris_a: patch_a.tris.clone(),
        old_tris_b: patch_b.tris.clone(),
        seam: seam_mesh,
    })
}

/// Write a [`SpliceOutput`] into the mesh: append the new vertices, drop both
/// patches' old triangles, and append the replacements carrying each patch's
/// attribution.
///
/// `attribution.attributions` is kept in lockstep with `mesh.tris` throughout,
/// which is the invariant every downstream consumer (`compute_phase_a`,
/// `flood_fill_patches`) depends on. Vertices orphaned by the replacement are
/// left for the caller's usual `compact_unreferenced_verts` pass, exactly like
/// the §4.5.3 collapse path.
pub(crate) fn apply_splice(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    out: &SpliceOutput,
) -> Result<(), SpliceError> {
    if mesh.verts.len() as u32 != out.base_vert {
        return Err(SpliceError::StalePlan {
            expected: out.base_vert,
            actual: mesh.verts.len() as u32,
        });
    }
    // Each side's replacement triangles inherit that patch's attribution, which
    // is only unambiguous if the patch had exactly one.
    let attr_of_side =
        |side: Side, tris: &[u32]| -> Result<Option<TriangleAttribution>, SpliceError> {
            let mut it = tris.iter().map(|&t| attribution.attributions[t as usize]);
            let first = it.next().flatten();
            if it.any(|a| a != first) {
                return Err(SpliceError::MixedAttribution(side));
            }
            Ok(first)
        };
    let attr_a = attr_of_side(Side::A, &out.old_tris_a)?;
    let attr_b = attr_of_side(Side::B, &out.old_tris_b)?;

    mesh.verts.extend_from_slice(&out.new_verts);

    let removed: BTreeSet<u32> = out
        .old_tris_a
        .iter()
        .chain(out.old_tris_b.iter())
        .copied()
        .collect();
    let mut tris = Vec::with_capacity(mesh.tris.len() + out.tris_a.len() + out.tris_b.len());
    let mut attrs = Vec::with_capacity(tris.capacity());
    for (t, tri) in mesh.tris.iter().enumerate() {
        if removed.contains(&(t as u32)) {
            continue;
        }
        tris.push(*tri);
        attrs.push(attribution.attributions[t]);
    }
    for (side_tris, attr) in [(&out.tris_a, attr_a), (&out.tris_b, attr_b)] {
        for tri in side_tris {
            tris.push(*tri);
            attrs.push(attr);
        }
    }
    mesh.tris = tris;
    attribution.attributions = attrs;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brep::InputId;
    use crate::Vector3;

    fn opts() -> MeshUpdateOpts {
        MeshUpdateOpts {
            merge_tol: 1e-9,
            d_eps: 1e-6,
        }
    }

    fn edges(list: &[(u32, u32)]) -> BTreeSet<(u32, u32)> {
        list.iter()
            .map(|&(a, b)| if a < b { (a, b) } else { (b, a) })
            .collect()
    }

    // ---- Step 1: per-side chain ordering. --------------------------------

    #[test]
    fn ordered_seam_side_walks_an_open_run_from_the_lower_endpoint() {
        // Pentagon cycle; the seam is the two edges along the bottom.
        let cycles = vec![vec![0, 4, 1, 2, 3]];
        let (chain, closed) =
            ordered_seam_side(&cycles, &edges(&[(0, 4), (4, 1)]), Side::A).unwrap();
        assert_eq!(chain, vec![0, 4, 1]);
        assert!(!closed);
    }

    #[test]
    fn ordered_seam_side_detects_a_closed_loop() {
        let cycles = vec![vec![0, 1, 2, 3]];
        let (chain, closed) =
            ordered_seam_side(&cycles, &edges(&[(0, 1), (1, 2), (2, 3), (3, 0)]), Side::A).unwrap();
        assert_eq!(chain.len(), 4);
        assert!(closed);
        assert_eq!(
            chain[0], 0,
            "starts at the lowest vertex, deterministically"
        );
    }

    #[test]
    fn ordered_seam_side_rejects_edges_absent_from_the_patch() {
        let cycles = vec![vec![0, 1, 2, 3]];
        let got = ordered_seam_side(&cycles, &edges(&[(7, 8)]), Side::B);
        assert_eq!(got, Err(SpliceError::SeamNotOnPatch(Side::B)));
    }

    #[test]
    fn ordered_seam_side_refuses_two_disconnected_runs() {
        // Opposite sides of a quad: two separate runs, no single chain.
        let cycles = vec![vec![0, 1, 2, 3]];
        let got = ordered_seam_side(&cycles, &edges(&[(0, 1), (2, 3)]), Side::A);
        assert_eq!(got, Err(SpliceError::SeamNotSimple(Side::A)));
    }

    // ---- Step 2: merging the two chains. ---------------------------------

    #[test]
    fn merge_seam_chains_inserts_the_finer_sides_vertex_into_the_coarse_chain() {
        // THE C0044 SHAPE, from the coarse side's point of view: B's chain is
        // [0,1]; A subdivides the same seam at 4. The merged curve carries 4.
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.5, 0.0, 0.0),
        ];
        let (chain, closed) =
            merge_seam_chains(&verts, &[0, 1], false, &[0, 2, 1], false, 1e-6).unwrap();
        assert_eq!(
            chain,
            vec![0, 2, 1],
            "the extra vertex lands between 0 and 1"
        );
        assert!(!closed);
    }

    #[test]
    fn merge_seam_chains_rejects_a_vertex_off_the_reference_curve() {
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.5, 0.9, 0.0), // nowhere near the seam
        ];
        let got = merge_seam_chains(&verts, &[0, 1], false, &[0, 2, 1], false, 1e-6);
        assert!(matches!(
            got,
            Err(SpliceError::SeamPointOffReference { vertex: 2, .. })
        ));
    }

    #[test]
    fn merge_seam_chains_rejects_an_open_against_a_closed_seam() {
        let verts = vec![Point3::new(0.0, 0.0, 0.0); 3];
        let got = merge_seam_chains(&verts, &[0, 1], false, &[0, 1, 2], true, 1e-6);
        assert_eq!(got, Err(SpliceError::SeamTopologyMismatch));
    }

    // ---- Step 3: the cylinder theta seam. --------------------------------

    fn unit_cylinder() -> SurfaceChart {
        SurfaceChart::new(Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        })
        .expect("cylinder charts exist")
    }

    /// A point on the unit cylinder at angle `deg` (in the chart's own basis,
    /// recovered by lifting so the test does not assume `ortho_basis`).
    fn cyl_pt(chart: &SurfaceChart, deg: f64, z: f64) -> Point3 {
        chart.lift(Point2::new(deg.to_radians(), z))
    }

    #[test]
    fn unwrap_theta_is_empty_for_a_plane() {
        let chart = SurfaceChart::new(Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        })
        .unwrap();
        let verts = vec![Point3::new(0.0, 0.0, 0.0); 3];
        let got = unwrap_theta(&chart, &verts, &[vec![0, 1, 2]], Side::A).unwrap();
        assert!(got.is_empty(), "a plane has no branch to choose");
    }

    #[test]
    fn unwrap_theta_makes_a_seam_straddling_patch_contiguous() {
        let chart = unit_cylinder();
        // A patch spanning 150 deg .. 210 deg — i.e. across theta = +-pi.
        let verts = vec![
            cyl_pt(&chart, 150.0, 0.0),
            cyl_pt(&chart, 210.0, 0.0),
            cyl_pt(&chart, 210.0, 1.0),
            cyl_pt(&chart, 150.0, 1.0),
        ];
        let cycles = vec![vec![0, 1, 2, 3]];
        let shift = unwrap_theta(&chart, &verts, &cycles, Side::A).unwrap();

        // Every shift is a multiple of 2pi, which is what makes the unwrap a
        // no-op in world space.
        for s in shift.values() {
            assert!(
                (s / TWO_PI - (s / TWO_PI).round()).abs() < 1e-12,
                "shift {s} must be a multiple of 2pi"
            );
        }
        // Unwrapped, the four thetas span 60 deg, not ~300.
        let ts: Vec<f64> = (0..4)
            .map(|v| chart.project(verts[v]).x() + shift[&(v as u32)])
            .collect();
        let span = ts.iter().cloned().fold(f64::MIN, f64::max)
            - ts.iter().cloned().fold(f64::MAX, f64::min);
        assert!(
            (span - 60.0f64.to_radians()).abs() < 1e-9,
            "expected a 60 deg span after unwrapping, got {} deg",
            span.to_degrees()
        );
        // And the shift genuinely does not move the surface.
        for v in 0..4u32 {
            let uv = chart.project(verts[v as usize]);
            let lifted = chart.lift(Point2::new(uv.x() + shift[&v], uv.y()));
            assert!(dist3(lifted, verts[v as usize]) < 1e-12);
        }
    }

    #[test]
    fn unwrap_theta_refuses_a_patch_that_encircles_the_axis() {
        let chart = unit_cylinder();
        let verts = vec![
            cyl_pt(&chart, 0.0, 0.0),
            cyl_pt(&chart, 120.0, 0.0),
            cyl_pt(&chart, 240.0, 0.0),
        ];
        let got = unwrap_theta(&chart, &verts, &[vec![0, 1, 2]], Side::A);
        assert_eq!(got, Err(SpliceError::PatchEncirclesAxis(Side::A)));
    }

    // ---- Region -> patch pair selection. ---------------------------------

    #[test]
    fn patches_on_seam_picks_exactly_the_pair_carrying_the_edges() {
        let (_mesh, _attr, patch_a, patch_b) = mismatched_seam_fixture();
        // A third patch sharing no seam edge with the region.
        let far = SplicePatch {
            cycles: vec![vec![2, 3, 5]],
            tris: vec![],
            surface: patch_a.surface,
        };
        let patches = vec![patch_a, far, patch_b];
        assert_eq!(
            patches_on_seam(&patches, &edges(&[(0, 1), (0, 4), (1, 4)])),
            vec![0, 2]
        );
    }

    #[test]
    fn patches_on_seam_reports_no_pair_when_only_one_patch_carries_the_seam() {
        let (_mesh, _attr, patch_a, _patch_b) = mismatched_seam_fixture();
        // (2,3) is on A's outer boundary only — the caller must leave it alone.
        assert_eq!(patches_on_seam(&[patch_a], &edges(&[(2, 3)])), vec![0]);
    }

    // ---- The C0044-class end-to-end splice. ------------------------------

    /// Two planar patches meeting along the edge (0,1): A (z = 0) subdivides
    /// that edge at the midpoint 4, and B (y = 0) does not.
    ///
    /// A carries 0->4->1 and B carries 1->0, so no direction pairs: (0,1),
    /// (0,4) and (4,1) are all imbalanced — the C0044 shape the whole
    /// mesh-update epic exists to repair.
    fn mismatched_seam_fixture() -> (Mesh, TriangleAttributionMap, SplicePatch, SplicePatch) {
        let mesh = Mesh {
            verts: vec![
                Point3::new(0.0, 0.0, 0.0), // 0
                Point3::new(1.0, 0.0, 0.0), // 1
                Point3::new(1.0, 1.0, 0.0), // 2
                Point3::new(0.0, 1.0, 0.0), // 3
                Point3::new(0.5, 0.0, 0.0), // 4 — A-only seam midpoint
                Point3::new(1.0, 0.0, 1.0), // 5
                Point3::new(0.0, 0.0, 1.0), // 6
            ],
            tris: vec![
                // Patch A on z = 0 (pentagon 0,4,1,2,3), outward +z.
                [0, 4, 3],
                [4, 1, 2],
                [4, 2, 3],
                // Patch B on y = 0 (quad 0,6,5,1), outward +y. Wound so it
                // traverses the shared edge as 1->0, opposite A — the way two
                // adjacent patches of a real mesh meet.
                [0, 6, 5],
                [0, 5, 1],
            ],
        };
        let mut attribution = TriangleAttributionMap::empty();
        attribution.attributions = vec![
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0,
            }),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0,
            }),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0,
            }),
            Some(TriangleAttribution {
                input: InputId::B,
                face: 1,
            }),
            Some(TriangleAttribution {
                input: InputId::B,
                face: 1,
            }),
        ];
        let patch_a = SplicePatch {
            cycles: vec![vec![0, 4, 1, 2, 3]],
            tris: vec![0, 1, 2],
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
        };
        let patch_b = SplicePatch {
            cycles: vec![vec![0, 6, 5, 1]],
            tris: vec![3, 4],
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 1.0, 0.0),
                d: 0.0,
            },
        };
        (mesh, attribution, patch_a, patch_b)
    }

    /// Directed half-edge counts, the same imbalance test
    /// `detect_nonmanifold_seams` and `check_watertight_2manifold` apply.
    fn directed(tris: &[[u32; 3]]) -> BTreeMap<(u32, u32), i32> {
        let mut d: BTreeMap<(u32, u32), i32> = BTreeMap::new();
        for t in tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                *d.entry((t[i], t[j])).or_default() += 1;
            }
        }
        d
    }

    #[test]
    fn the_fixture_really_is_non_manifold_before_the_splice() {
        let (mesh, ..) = mismatched_seam_fixture();
        let d = directed(&mesh.tris);
        // B carries the whole edge 1->0 while A carries it in two halves.
        assert_eq!(d.get(&(1, 0)).copied().unwrap_or(0), 1);
        assert_eq!(d.get(&(0, 1)).copied().unwrap_or(0), 0);
    }

    #[test]
    fn splice_repairs_the_mismatched_seam_into_a_paired_one() {
        let (mut mesh, mut attribution, patch_a, patch_b) = mismatched_seam_fixture();
        let seam = edges(&[(0, 1), (0, 4), (1, 4)]);

        let out = splice_seam_pair(&mesh, &patch_a, &patch_b, &seam, opts(), 1e-9)
            .expect("the splice must succeed on the fixture it exists for");

        // The shared curve carries A's midpoint, and every seam point resolved
        // to an EXISTING mesh vertex (B split its edge onto vertex 4).
        assert_eq!(out.seam, vec![0, 4, 1]);
        assert!(
            out.new_verts.is_empty(),
            "no new vertex is needed: B's split foot IS vertex 4"
        );

        apply_splice(&mut mesh, &mut attribution, &out).unwrap();
        assert_eq!(
            mesh.tris.len(),
            attribution.attributions.len(),
            "attribution must stay in lockstep with tris"
        );

        let d = directed(&mesh.tris);
        // The whole-edge (0,1) is gone; both halves are now paired anti-parallel.
        assert_eq!(d.get(&(0, 1)).copied().unwrap_or(0), 0);
        assert_eq!(d.get(&(1, 0)).copied().unwrap_or(0), 0);
        for (s, e) in [(0u32, 4u32), (4, 1)] {
            assert_eq!(
                d.get(&(s, e)).copied().unwrap_or(0),
                1,
                "seam half ({s},{e}) must be carried once in each direction"
            );
            assert_eq!(d.get(&(e, s)).copied().unwrap_or(0), 1);
        }
    }

    #[test]
    fn splice_preserves_each_patchs_outward_orientation() {
        let (mut mesh, mut attribution, patch_a, patch_b) = mismatched_seam_fixture();
        let seam = edges(&[(0, 1), (0, 4), (1, 4)]);
        let before_a = area_vector(
            &patch_a
                .tris
                .iter()
                .map(|&t| mesh.tris[t as usize])
                .collect::<Vec<_>>(),
            &|v| mesh.verts[v as usize],
        );
        let before_b = area_vector(
            &patch_b
                .tris
                .iter()
                .map(|&t| mesh.tris[t as usize])
                .collect::<Vec<_>>(),
            &|v| mesh.verts[v as usize],
        );

        let out = splice_seam_pair(&mesh, &patch_a, &patch_b, &seam, opts(), 1e-9).unwrap();
        let pos = |v: u32| -> Point3 {
            if v < out.base_vert {
                mesh.verts[v as usize]
            } else {
                out.new_verts[(v - out.base_vert) as usize]
            }
        };
        let after_a = area_vector(&out.tris_a, &pos);
        let after_b = area_vector(&out.tris_b, &pos);

        // Same outward sense, and (both patches being planar and unchanged in
        // outline) the same total area.
        assert!(
            dot3(before_a, after_a) > 0.0,
            "A flipped: {before_a:?} vs {after_a:?}"
        );
        assert!(
            dot3(before_b, after_b) > 0.0,
            "B flipped: {before_b:?} vs {after_b:?}"
        );
        for (b, a) in [(before_a, after_a), (before_b, after_b)] {
            for k in 0..3 {
                assert!(
                    (b[k] - a[k]).abs() < 1e-12,
                    "area vector changed: {b:?} -> {a:?}"
                );
            }
        }
        apply_splice(&mut mesh, &mut attribution, &out).unwrap();
    }

    #[test]
    fn apply_splice_refuses_a_plan_built_against_a_different_mesh() {
        let (mut mesh, mut attribution, patch_a, patch_b) = mismatched_seam_fixture();
        let seam = edges(&[(0, 1), (0, 4), (1, 4)]);
        let out = splice_seam_pair(&mesh, &patch_a, &patch_b, &seam, opts(), 1e-9).unwrap();
        mesh.verts.push(Point3::new(9.0, 9.0, 9.0)); // someone else moved first
        let got = apply_splice(&mut mesh, &mut attribution, &out);
        assert!(matches!(got, Err(SpliceError::StalePlan { .. })));
    }

    #[test]
    fn splice_declines_an_unsupported_surface_rather_than_guessing() {
        let (mesh, _attr, patch_a, mut patch_b) = mismatched_seam_fixture();
        patch_b.surface = Surface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        };
        let got = splice_seam_pair(
            &mesh,
            &patch_a,
            &patch_b,
            &edges(&[(0, 1), (0, 4), (1, 4)]),
            opts(),
            1e-9,
        );
        assert_eq!(got, Err(SpliceError::UnsupportedSurface(Side::B)));
    }

    #[test]
    fn splice_refuses_to_silently_coarsen_a_curved_patch() {
        // Give patch A an interior vertex and a curved surface: dropping the
        // vertex would coarsen the cylinder, so the splice must stop loudly.
        let (mut mesh, _attr, mut patch_a, patch_b) = mismatched_seam_fixture();
        mesh.verts.push(Point3::new(0.5, 0.5, 0.0)); // 7, interior to A
        mesh.tris[0] = [0, 4, 7];
        patch_a.surface = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let got = splice_seam_pair(
            &mesh,
            &patch_a,
            &patch_b,
            &edges(&[(0, 1), (0, 4), (1, 4)]),
            opts(),
            1e-9,
        );
        assert!(matches!(
            got,
            Err(SpliceError::CurvedPatchInteriorVertices { side: Side::A, .. })
        ));
    }
}
