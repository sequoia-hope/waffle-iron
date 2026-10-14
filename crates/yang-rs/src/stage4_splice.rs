//! Seam-chain primitives for the §4.4.1 construct pass (`stage5_topology`):
//!
//! * [`ordered_seam_side`] — order one patch's nominated seam edges into a
//!   single chain (open or closed), refusing branches;
//! * [`order_along_curve`] / [`reorder_cycles_to_curve`] /
//!   [`splice_refined_run_into_cycles`] — the curve-authority re-ordering of a
//!   conic seam run inside a patch's boundary cycles (Yang §4.4.1,
//!   "r_A = r_B = r": the exact curve orders the seam, not the mesh's chain);
//! * [`unwrap_theta`] — the cylinder `θ = ±π` branch choice for a patch whose
//!   boundary straddles the chart seam (declared to be this layer's job by
//!   both the chart and the extraction layer, which explicitly decline it);
//! * [`area_vector`] / [`dot3`] — orientation witnesses for a rebuilt patch.
//!
//! [`SplicePatch`] is the patch description every one of these takes: boundary
//! cycles as ordered mesh-vertex chains, the owned triangle indices, and the
//! analytic surface that becomes the chart.

use crate::stage4_project::SurfaceChart;
use crate::{Curve, Surface};
use cad_primitives::Point3;
use std::collections::{BTreeMap, BTreeSet};

const TWO_PI: f64 = std::f64::consts::TAU;

/// Which operand's patch a failure came from. The §4.4.1 construct pass
/// works one patch at a time, so every caller names `Side::A`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    A,
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

/// Why a seam ordering / unwrap step refused. Every variant is a P9/P10 LOUD
/// stop: we never emit a silently re-ordered or re-oriented seam.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SpliceError {
    /// A cylinder patch wraps the axis completely, so no `θ` branch unrolls it
    /// into a simple planar domain.
    PatchEncirclesAxis(Side),
    /// A seam vertex could not be placed on a consistent `θ` branch.
    ThetaBranchUnresolved(Side, u32),
    /// None of the given seam edges lie on this side's boundary cycles.
    SeamNotOnPatch(Side),
    /// This side's seam edges do not form ONE simple path or cycle (a branch or
    /// a T-junction). We do not guess which branch is the curve.
    SeamNotSimple(Side),
    /// The seam's exact curve is not a closed-form conic, so it carries no
    /// parameter to order by. The caller keeps the mesh's chain order.
    SeamCurveNotConic,
    /// Ordering along the curve would move an ENDPOINT of an open seam. Those
    /// are junctions shared with other patches; re-rooting the seam here would
    /// silently corrupt them.
    SeamEndpointsReordered { was: [u32; 2], now: [u32; 2] },
}

// ---------------------------------------------------------------------------
// Step 1 — order one side's seam vertices into a chain.
// ---------------------------------------------------------------------------

/// Order the seam vertices ON `cycles` into a single chain.
///
/// `seam_edges` is the candidate edge set (canonical `(min, max)`) the selector
/// nominated. Only the
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
// Step 2b — THE CURVE AUTHORITY (Yang §4.4.1, "we set r_A = r_B = r").
// ---------------------------------------------------------------------------

/// Order `chain`'s vertices along the EXACT intersection curve, by curve
/// parameter.
///
/// This is §4.4.1's authority rule. The paper does not re-triangulate a patch
/// against whatever seam the mesh currently holds: *"The intersection curves on
/// the parametric surfaces are mapped to the meshes … **Then we set
/// r_A = r_B = r**, so that the two polylines in the meshes coincide with the
/// intersection curve"*, and flip-freeness follows *"since the intersection
/// curves are regular"* — from the CURVE's monotone sampling, not from anything
/// the mesh does.
///
/// It matters because the mesh's chain adjacency is inherited from the
/// PRE-Stage-4 chord polygon. Relocation then moves each vertex onto the exact
/// curve independently, and a vertex whose displacement exceeds its neighbour
/// spacing lands PAST that neighbour (F0067: 3.7e-3 against a 6.4e-4 segment).
/// The chain is then self-crossing, and no re-triangulation *from* that chain
/// can repair it — the crossing is in the boundary being triangulated. Sorting
/// by curve parameter restores the order the curve itself defines.
///
/// The same monotonicity §4.5.3 already reads through
/// [`crate::stage4_correct::conic_param`], which is why this reuses it rather
/// than inventing a second notion of "along the curve".
///
/// Closed-form conics only (`Circle` / `Ellipse`, plus `Hyperbola` under the
/// I13b gate); anything else yields [`SpliceError::SeamCurveNotConic`] and the
/// caller keeps the mesh order. For an OPEN seam on a PERIODIC conic the
/// points lie on one arc, so the single largest circular gap in the
/// parameters is the arc's complement — cutting there linearizes the order
/// without needing a branch convention; an open conic's parameter is already
/// linear, so ascending order needs no cut.
pub(crate) fn order_along_curve(
    curve: &Curve,
    verts: &[Point3],
    chain: &[u32],
    closed: bool,
) -> Result<Vec<u32>, SpliceError> {
    // I13b: an open conic's parameter is unbounded and strictly monotone —
    // ascending order IS the chain order, and no circular-gap cut applies. A
    // CLOSED chain on an open conic is geometrically impossible (a hyperbola
    // branch does not close) — decline it. Under the default env the open
    // params are `None` and the decline below is byte-identical to before.
    let periodic = crate::stage4_correct::conic_param_periodic(curve);
    if !periodic && closed {
        return Err(SpliceError::SeamCurveNotConic);
    }
    let mut keyed: Vec<(f64, u32)> = Vec::with_capacity(chain.len());
    for &v in chain {
        let t = crate::stage4_correct::conic_param(curve, verts[v as usize])
            .ok_or(SpliceError::SeamCurveNotConic)?;
        if !t.is_finite() {
            return Err(SpliceError::SeamCurveNotConic);
        }
        keyed.push((t, v));
    }
    // Ascending parameter; the index tie-break keeps exact ties deterministic.
    keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("finite").then(a.1.cmp(&b.1)));

    let n = keyed.len();
    if closed {
        // Rotation is geometrically irrelevant for a closed loop; start at the
        // lowest vertex index so the result is deterministic.
        let k = (0..n).min_by_key(|&i| keyed[i].1).expect("non-empty");
        keyed.rotate_left(k);
    } else if periodic {
        // Cut at the largest circular gap — the complement of the arc.
        let mut best = (f64::NEG_INFINITY, 0usize);
        for i in 0..n {
            let gap = if i + 1 == n {
                keyed[0].0 + TWO_PI - keyed[n - 1].0
            } else {
                keyed[i + 1].0 - keyed[i].0
            };
            if gap > best.0 {
                best = (gap, (i + 1) % n);
            }
        }
        keyed.rotate_left(best.1);
    }
    let out: Vec<u32> = keyed.into_iter().map(|(_, v)| v).collect();

    // An open seam's endpoints are junctions shared with OTHER patches. The
    // interior may reorder freely; the endpoints may not, or the reorder would
    // silently re-root a seam those patches also carry.
    if !closed {
        let mut was = [chain[0], chain[chain.len() - 1]];
        let mut now = [out[0], out[out.len() - 1]];
        was.sort_unstable();
        now.sort_unstable();
        if was != now {
            return Err(SpliceError::SeamEndpointsReordered { was, now });
        }
    }
    Ok(out)
}

/// Rewrite each cycle that carries the whole seam so its seam run appears in
/// `ordered` order, leaving every other cycle untouched.
///
/// The seam occupies a contiguous run of the cycle (it is a run of consecutive
/// boundary edges), so this overwrites that span in place. The span's leading
/// vertex picks the direction, which keeps the cycle's own traversal sense; for
/// a cycle that IS the seam, winding is re-normalized downstream by
/// `patch_from_cycles_shifted`'s shoelace and the final triangle orientation is
/// fixed by measurement, so no direction choice is needed here.
pub(crate) fn reorder_cycles_to_curve(
    cycles: &[Vec<u32>],
    ordered: &[u32],
    side: Side,
) -> Result<Vec<Vec<u32>>, SpliceError> {
    let set: BTreeSet<u32> = ordered.iter().copied().collect();
    let mut out = Vec::with_capacity(cycles.len());
    for cyc in cycles {
        // Only the cycle carrying the WHOLE seam is rewritten. A cycle sharing
        // just a junction endpoint passes through untouched.
        if !ordered.iter().all(|v| cyc.contains(v)) {
            out.push(cyc.clone());
            continue;
        }
        let n = cyc.len();
        let in_set: Vec<bool> = cyc.iter().map(|v| set.contains(v)).collect();
        let count = in_set.iter().filter(|b| **b).count();
        if count != ordered.len() {
            return Err(SpliceError::SeamNotSimple(side));
        }
        if count == n {
            out.push(ordered.to_vec());
            continue;
        }
        let start = (0..n)
            .find(|&i| in_set[i] && !in_set[(i + n - 1) % n])
            .ok_or(SpliceError::SeamNotSimple(side))?;
        // The run must be contiguous, or it is not one seam.
        if (0..count).any(|k| !in_set[(start + k) % n]) {
            return Err(SpliceError::SeamNotSimple(side));
        }
        let first = cyc[start];
        let seq: Vec<u32> = if ordered[0] == first {
            ordered.to_vec()
        } else if *ordered.last().expect("non-empty") == first {
            ordered.iter().rev().copied().collect()
        } else {
            let mut was = [first, cyc[(start + count - 1) % n]];
            let mut now = [ordered[0], *ordered.last().expect("non-empty")];
            was.sort_unstable();
            now.sort_unstable();
            return Err(SpliceError::SeamEndpointsReordered { was, now });
        };
        let mut cy = cyc.clone();
        for (k, &v) in seq.iter().enumerate() {
            cy[(start + k) % n] = v;
        }
        out.push(cy);
    }
    Ok(out)
}

/// I5-1 (§4.3.4 density refinement, spec `yang_441` §4-I5): rewrite each
/// cycle that carries the whole seam so its seam run is replaced by the
/// LONGER `refined` sequence (the parameter-ordered chain with the paper's
/// midpoint samples interleaved), leaving every other cycle untouched.
///
/// The seam run is located exactly as [`reorder_cycles_to_curve`] locates it
/// (membership set of `ordered`, contiguity required); the direction is
/// picked by matching the run's leading vertex against `refined`'s
/// endpoints, which `refine` preserves (`refined[0] == ordered[0]` and
/// `refined.last() == ordered.last()`). A cycle that IS the whole seam is
/// refused (`SeamNotSimple`) — closed seams are skipped upstream.
pub(crate) fn splice_refined_run_into_cycles(
    cycles: &[Vec<u32>],
    ordered: &[u32],
    refined: &[u32],
    side: Side,
) -> Result<Vec<Vec<u32>>, SpliceError> {
    let set: BTreeSet<u32> = ordered.iter().copied().collect();
    let mut out = Vec::with_capacity(cycles.len());
    for cyc in cycles {
        if !ordered.iter().all(|v| cyc.contains(v)) {
            out.push(cyc.clone());
            continue;
        }
        let n = cyc.len();
        let in_set: Vec<bool> = cyc.iter().map(|v| set.contains(v)).collect();
        let count = in_set.iter().filter(|b| **b).count();
        if count != ordered.len() || count == n {
            return Err(SpliceError::SeamNotSimple(side));
        }
        let start = (0..n)
            .find(|&i| in_set[i] && !in_set[(i + n - 1) % n])
            .ok_or(SpliceError::SeamNotSimple(side))?;
        if (0..count).any(|k| !in_set[(start + k) % n]) {
            return Err(SpliceError::SeamNotSimple(side));
        }
        let first = cyc[start];
        let seq: Vec<u32> = if refined[0] == first {
            refined.to_vec()
        } else if *refined.last().expect("non-empty") == first {
            refined.iter().rev().copied().collect()
        } else {
            let mut was = [first, cyc[(start + count - 1) % n]];
            let mut now = [refined[0], *refined.last().expect("non-empty")];
            was.sort_unstable();
            now.sort_unstable();
            return Err(SpliceError::SeamEndpointsReordered { was, now });
        };
        let rot: Vec<u32> = (0..n).map(|k| cyc[(start + k) % n]).collect();
        let mut cy = Vec::with_capacity(n - count + seq.len());
        cy.extend_from_slice(&seq);
        cy.extend_from_slice(&rot[count..]);
        out.push(cy);
    }
    Ok(out)
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

// ---------------------------------------------------------------------------
// Steps 4-5 — the splice itself.
// ---------------------------------------------------------------------------

/// Area vector (twice the signed area, summed) of a triangle list.
pub(crate) fn area_vector(tris: &[[u32; 3]], pos: &dyn Fn(u32) -> Point3) -> [f64; 3] {
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

pub(crate) fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vector3;
    use cad_primitives::Point2;

    fn dist3(a: Point3, b: Point3) -> f64 {
        let d = [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
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
        let got = ordered_seam_side(&cycles, &edges(&[(7, 8)]), Side::A);
        assert_eq!(got, Err(SpliceError::SeamNotOnPatch(Side::A)));
    }

    #[test]
    fn ordered_seam_side_refuses_two_disconnected_runs() {
        // Opposite sides of a quad: two separate runs, no single chain.
        let cycles = vec![vec![0, 1, 2, 3]];
        let got = ordered_seam_side(&cycles, &edges(&[(0, 1), (2, 3)]), Side::A);
        assert_eq!(got, Err(SpliceError::SeamNotSimple(Side::A)));
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
}

#[cfg(test)]
mod curve_authority_tests {
    use super::*;
    use crate::Vector3;

    fn unit_circle() -> Curve {
        Curve::Circle {
            center: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        }
    }

    /// Points on the unit circle at the given degrees, in index order.
    fn ring(degs: &[f64]) -> Vec<Point3> {
        degs.iter()
            .map(|d| {
                let r = d.to_radians();
                Point3::new(r.cos(), r.sin(), 0.0)
            })
            .collect()
    }

    /// THE F0067 SHAPE. The mesh chain visits 0deg, 60deg, 30deg, 90deg — v1 and
    /// v2 out of order, which is what a relocation overshooting its neighbour
    /// leaves behind. That chain is self-crossing; the curve's parameter order
    /// is not.
    #[test]
    fn order_along_curve_repairs_a_chain_whose_vertex_overshot_its_neighbour() {
        let verts = ring(&[0.0, 30.0, 60.0, 90.0]);
        let mesh_order = vec![0u32, 2, 1, 3];
        let got = order_along_curve(&unit_circle(), &verts, &mesh_order, false).unwrap();
        assert_eq!(got, vec![0, 1, 2, 3], "the curve's parameter order governs");
        assert_ne!(
            got, mesh_order,
            "and it genuinely differs from the mesh order"
        );
    }

    #[test]
    fn order_along_curve_leaves_an_already_monotone_chain_alone() {
        let verts = ring(&[0.0, 30.0, 60.0, 90.0]);
        let chain = vec![0u32, 1, 2, 3];
        let got = order_along_curve(&unit_circle(), &verts, &chain, false).unwrap();
        assert_eq!(got, chain);
    }

    /// An open arc that straddles the parameter's own branch cut must still
    /// linearize correctly — the largest circular gap is the arc's complement.
    #[test]
    fn order_along_curve_linearizes_an_arc_across_the_branch_cut() {
        // 150, 170, -170 (=190), -150 (=210): a 60deg arc across theta = +-pi.
        let verts = ring(&[150.0, 170.0, 190.0, 210.0]);
        let got = order_along_curve(&unit_circle(), &verts, &[0, 1, 2, 3], false).unwrap();
        // Either direction is a valid linearization; what matters is that the
        // arc is contiguous, i.e. the cut did NOT land inside it.
        assert!(
            got == vec![0, 1, 2, 3] || got == vec![3, 2, 1, 0],
            "arc must stay contiguous across the cut, got {got:?}"
        );
    }

    /// Endpoints of an open seam are junctions shared with OTHER patches.
    /// Reordering may rearrange the interior but must never re-root the seam.
    #[test]
    fn order_along_curve_refuses_to_move_an_open_seams_endpoint() {
        let verts = ring(&[0.0, 30.0, 60.0]);
        // Chain endpoints are v1 (30deg) and v2 (60deg), but by parameter the
        // extremes are v0 and v2 — so ordering would re-root the seam.
        let got = order_along_curve(&unit_circle(), &verts, &[1, 0, 2], false);
        assert!(
            matches!(got, Err(SpliceError::SeamEndpointsReordered { .. })),
            "expected an endpoint stop, got {got:?}"
        );
    }

    #[test]
    fn order_along_curve_declines_a_curve_with_no_closed_form_parameter() {
        let verts = ring(&[0.0, 30.0, 60.0]);
        let got = order_along_curve(&Curve::LineSegment, &verts, &[0, 1, 2], false);
        assert_eq!(got, Err(SpliceError::SeamCurveNotConic));
    }

    // ---- Rewriting the patch boundary to match. --------------------------

    #[test]
    fn reorder_cycles_rewrites_only_the_seam_span_and_keeps_the_rest() {
        // Cycle: seam run [0,2,1,3] then two non-seam vertices.
        let cycles = vec![vec![0u32, 2, 1, 3, 7, 8]];
        let got = reorder_cycles_to_curve(&cycles, &[0, 1, 2, 3], Side::A).unwrap();
        assert_eq!(got, vec![vec![0, 1, 2, 3, 7, 8]]);
    }

    #[test]
    fn reorder_cycles_flips_the_run_to_match_the_cycles_own_direction() {
        // The cycle traverses the seam from 3 to 0, so the curve order is
        // applied reversed — the cycle's traversal sense is preserved.
        let cycles = vec![vec![3u32, 1, 2, 0, 7, 8]];
        let got = reorder_cycles_to_curve(&cycles, &[0, 1, 2, 3], Side::A).unwrap();
        assert_eq!(got, vec![vec![3, 2, 1, 0, 7, 8]]);
    }

    #[test]
    fn reorder_cycles_leaves_a_cycle_that_does_not_carry_the_whole_seam() {
        // A hole sharing only a junction endpoint passes through untouched.
        let cycles = vec![vec![0u32, 2, 1, 3, 7, 8], vec![0, 20, 21]];
        let got = reorder_cycles_to_curve(&cycles, &[0, 1, 2, 3], Side::A).unwrap();
        assert_eq!(got[1], vec![0, 20, 21]);
    }

    #[test]
    fn reorder_cycles_refuses_a_non_contiguous_seam_run() {
        // TWO gaps, so the run is split however you rotate it — not one seam.
        // (One gap is NOT enough: `[0,1,7,2,3]` has its seam vertices
        // contiguous *cyclically* as 2,3,0,1, which is a legitimate run.)
        let cycles = vec![vec![0u32, 1, 7, 2, 3, 8]];
        let got = reorder_cycles_to_curve(&cycles, &[0, 1, 2, 3], Side::A);
        assert_eq!(got, Err(SpliceError::SeamNotSimple(Side::A)));
    }

    #[test]
    fn reorder_cycles_accepts_a_run_that_wraps_the_cycle_start() {
        // Seam occupies positions 4,5,0,1 — one run, contiguous across the
        // wrap, with the two non-seam vertices adjacent at 2,3.
        let cycles = vec![vec![2u32, 3, 7, 9, 0, 1]];
        let got = reorder_cycles_to_curve(&cycles, &[0, 1, 2, 3], Side::A).unwrap();
        assert_eq!(got, vec![vec![2, 3, 7, 9, 0, 1]], "already in curve order");
        // And a scrambled wrap-run is repaired in place.
        let scrambled = vec![vec![3u32, 2, 7, 9, 0, 1]];
        let fixed = reorder_cycles_to_curve(&scrambled, &[0, 1, 2, 3], Side::A).unwrap();
        assert_eq!(fixed, vec![vec![2, 3, 7, 9, 0, 1]]);
    }
}
