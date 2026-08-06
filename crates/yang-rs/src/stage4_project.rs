//! #169 Phase B — per-operand parametric charts for the mesh-update splice.
//!
//! The frame-agnostic mesh-update driver
//! ([`crate::stage4_update::two_sided_conformal_update_lifted`]) re-triangulates
//! each operand's trimmed patch in its OWN parametric domain and verifies the
//! seam in 3D. To feed it, Phase B needs, per analytic surface, a chart:
//!
//! * `project`: world `Point3` (on/near the surface) → parametric `Point2`,
//! * `lift`:    parametric `Point2` → world `Point3`,
//!
//! that are mutual inverses for points ON the surface. Then the ONE shared 3D
//! intersection curve projects into each patch's chart, the patch re-CDTs in
//! param space, and the result lifts back conformally (both sides land on the
//! same world curve → 2-manifold seam, Yang 2025 §4.4.1).
//!
//! Charts are provided for **Plane** and **Cylinder** — the pair that dominates
//! the non-2-manifold reassembly bucket and #168's degenerate-cylinder case
//! (`replan_degenerate_cylinder_patches` already uses the same cylinder (θ,z)
//! frame). Sphere / Cone / Torus return `None` for now, so the Phase-B wiring
//! simply does not engage those patches and leaves them byte-identical.
//!
//! WIRED (2026-08-06, N2-3b step 2): this is the projection layer
//! [`crate::stage4_splice`]'s loop consumes, reached from
//! `reconstruct_topology_stage4` behind `YANG_MESHUP_ENABLE`.

use crate::{normalize3, ortho_basis, Surface};
use cad_primitives::{Point2, Point3};

/// A parametric chart for one analytic surface: `project` world→param and `lift`
/// param→world, mutual inverses for points on the surface.
///
/// * `Plane`: param = signed coordinates in an orthonormal in-plane basis
///   `(e1, e2)` rooted at the plane's foot-of-origin. An isometry, so the CDT in
///   param space is faithful and `lift(project(p)) == p` exactly for on-plane p.
/// * `Cylinder`: param = `(θ, z)`, the unrolled surface — `θ = atan2` in the
///   axis's ortho-basis, `z` = axial coordinate. `lift` is `2π`-periodic in `θ`,
///   so it inverts `project` for on-cylinder points regardless of branch. A
///   patch that STRADDLES the `θ = ±π` seam must be unwrapped by the caller
///   before CDT (the projected boundary would otherwise self-cross); `lift`
///   itself is seam-agnostic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SurfaceChart {
    Plane {
        origin: [f64; 3],
        e1: [f64; 3],
        e2: [f64; 3],
    },
    Cylinder {
        axis_point: [f64; 3],
        axis: [f64; 3],
        e1: [f64; 3],
        e2: [f64; 3],
        radius: f64,
    },
}

impl SurfaceChart {
    /// Build a chart for `surface`, or `None` for a surface type Phase B does not
    /// yet re-triangulate (Sphere / Cone / Torus). The caller keeps its existing
    /// behaviour for those (byte-identical).
    pub(crate) fn new(surface: Surface) -> Option<Self> {
        match surface {
            Surface::Plane { normal, d } => {
                let n = normalize3(normal.as_array());
                // A point on the plane: the foot of the world origin. With the
                // stored (unit) normal, `n·x + d = 0` ⇒ `x = -d·n`.
                let origin = [-d * n[0], -d * n[1], -d * n[2]];
                let (e1v, e2v) = ortho_basis(normal);
                Some(SurfaceChart::Plane {
                    origin,
                    e1: e1v.as_array(),
                    e2: e2v.as_array(),
                })
            }
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                let (e1v, e2v) = ortho_basis(axis_dir);
                Some(SurfaceChart::Cylinder {
                    axis_point: axis_point.as_array(),
                    axis: normalize3(axis_dir.as_array()),
                    e1: e1v.as_array(),
                    e2: e2v.as_array(),
                    radius,
                })
            }
            Surface::Sphere { .. } | Surface::Cone { .. } | Surface::Torus { .. } => None,
        }
    }

    /// Project a world point (assumed on/near the surface) to parametric space.
    pub(crate) fn project(&self, p: Point3) -> Point2 {
        let x = p.as_array();
        match *self {
            SurfaceChart::Plane { origin, e1, e2 } => {
                let w = [x[0] - origin[0], x[1] - origin[1], x[2] - origin[2]];
                Point2::new(dot(w, e1), dot(w, e2))
            }
            SurfaceChart::Cylinder {
                axis_point,
                axis,
                e1,
                e2,
                ..
            } => {
                let w = [
                    x[0] - axis_point[0],
                    x[1] - axis_point[1],
                    x[2] - axis_point[2],
                ];
                let z = dot(w, axis);
                let radial = [w[0] - z * axis[0], w[1] - z * axis[1], w[2] - z * axis[2]];
                let theta = dot(radial, e2).atan2(dot(radial, e1));
                Point2::new(theta, z)
            }
        }
    }

    /// Lift a parametric point back to world space (the exact inverse of
    /// [`project`](Self::project) for on-surface points).
    pub(crate) fn lift(&self, uv: Point2) -> Point3 {
        match *self {
            SurfaceChart::Plane { origin, e1, e2 } => {
                let (u, v) = (uv.x(), uv.y());
                Point3::new(
                    origin[0] + u * e1[0] + v * e2[0],
                    origin[1] + u * e1[1] + v * e2[1],
                    origin[2] + u * e1[2] + v * e2[2],
                )
            }
            SurfaceChart::Cylinder {
                axis_point,
                axis,
                e1,
                e2,
                radius,
            } => {
                let (theta, z) = (uv.x(), uv.y());
                let (ct, st) = (theta.cos(), theta.sin());
                Point3::new(
                    axis_point[0] + radius * (ct * e1[0] + st * e2[0]) + z * axis[0],
                    axis_point[1] + radius * (ct * e1[1] + st * e2[1]) + z * axis[1],
                    axis_point[2] + radius * (ct * e1[2] + st * e2[2]) + z * axis[2],
                )
            }
        }
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// ===========================================================================
// #169 Phase B / Phase 0 — the failure-region detector.
//
// Yang §4.5: after relocation, find the regions where the mesh is NOT a valid
// 2-manifold — the input to the §4.4.1 mesh-update. `check_watertight_2manifold`
// already reports the FIRST unpaired half-edge and stops; the mesh-update loop
// needs the WHOLE set, grouped into the patch pairs whose shared seam is
// mismatched, so it can re-triangulate each pair conformally.
//
// Confirmed on C0044 (2026-07-16): its non-manifold edge (14,15) is an unpaired
// half-edge (fwd=1 rev=0) between two adjacent PLANAR patches whose shared seam
// is subdivided differently — exactly a two-sided conformality failure. The
// detector groups such edges by the patch pair + connected seam run.
// ===========================================================================

/// One non-manifold seam region: the patch(es) whose shared boundary carries a
/// directional half-edge imbalance, and the offending undirected edges.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SeamRegion {
    /// Distinct triangle-attribution keys `(is_a, face)` incident to the region's
    /// unpaired edges — normally the two adjacent patches whose seam mismatches.
    pub keys: Vec<(bool, u32)>,
    /// The unpaired undirected edges `(s, e)` with `s < e`, in ascending order.
    pub edges: Vec<(u32, u32)>,
}

/// Find every non-manifold seam region in `tris`: undirected edges whose two
/// directions are imbalanced (`fwd != rev`), grouped into connected runs that
/// share a vertex. Each region also lists the attribution keys of the triangles
/// touching those edges (the patches the §4.4.1 mesh-update must reconcile).
///
/// Pure and deterministic (BTree-ordered). A conformal 2-manifold mesh yields an
/// empty vector — the same condition `check_watertight_2manifold` gates on.
pub(crate) fn detect_nonmanifold_seams(
    tris: &[[u32; 3]],
    attr_of: &dyn Fn(usize) -> Option<(bool, u32)>,
) -> Vec<SeamRegion> {
    use std::collections::{BTreeMap, BTreeSet};

    // Directed half-edge counts + undirected edge → incident triangles.
    let mut dir: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    let mut inc: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (ti, tri) in tris.iter().enumerate() {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            *dir.entry((a, b)).or_default() += 1;
            let key = if a < b { (a, b) } else { (b, a) };
            inc.entry(key).or_default().push(ti);
        }
    }

    // Unpaired undirected edges (deterministic ascending order).
    let mut unpaired: Vec<(u32, u32)> = Vec::new();
    for &(s, e) in inc.keys() {
        let fwd = dir.get(&(s, e)).copied().unwrap_or(0);
        let rev = dir.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            unpaired.push((s, e));
        }
    }
    if unpaired.is_empty() {
        return Vec::new();
    }

    // Group unpaired edges into connected runs via shared vertices (union-find
    // over edge indices, keyed by vertex).
    let n = unpaired.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r {
            r = parent[r];
        }
        let mut cur = x;
        while parent[cur] != r {
            let nx = parent[cur];
            parent[cur] = r;
            cur = nx;
        }
        r
    }
    let mut vert_first: BTreeMap<u32, usize> = BTreeMap::new();
    for (idx, &(s, e)) in unpaired.iter().enumerate() {
        for v in [s, e] {
            if let Some(&j) = vert_first.get(&v) {
                let (ra, rb) = (find(&mut parent, idx), find(&mut parent, j));
                if ra != rb {
                    parent[ra] = rb;
                }
            } else {
                vert_first.insert(v, idx);
            }
        }
    }

    // Collect per-root region: edges + attribution keys of incident triangles.
    type RegionAcc = (Vec<(u32, u32)>, BTreeSet<(bool, u32)>);
    let mut by_root: BTreeMap<usize, RegionAcc> = BTreeMap::new();
    for (idx, &(s, e)) in unpaired.iter().enumerate() {
        let root = find(&mut parent, idx);
        let entry = by_root.entry(root).or_default();
        entry.0.push((s, e));
        for &ti in &inc[&(s, e)] {
            if let Some(k) = attr_of(ti) {
                entry.1.insert(k);
            }
        }
    }

    by_root
        .into_values()
        .map(|(mut edges, keys)| {
            edges.sort_unstable();
            SeamRegion {
                keys: keys.into_iter().collect(),
                edges,
            }
        })
        .collect()
}

/// Build the parametric [`Patch`](crate::stage4_update::Patch) the mesh-update
/// driver consumes, from a 3D patch's boundary cycles.
///
/// `cycles[0]` is the outer boundary and the rest are holes — the order
/// [`crate::stage4_correct::PatchInfo`] already stores them in. Each cycle is
/// mesh-vertex indices; every vertex is projected through `chart`.
///
/// Returns the patch **and** the index map `patch vertex -> mesh vertex`, which
/// the splice needs to write the re-triangulated result back into the 3D mesh.
/// Without it the `PatchUpdate` is a set of 2D points with no way home.
///
/// The outer boundary is normalized to **CCW in the chart frame**, which
/// `Patch` requires; a cycle whose projection winds CW is reversed (and its
/// index map with it). Hole cycles are passed through as given — `Patch` states
/// no winding requirement for them, and imposing one here would be inventing a
/// contract.
///
/// `None` when any cycle is shorter than 3 vertices, when a vertex index is out
/// of range, or when the outer boundary projects to zero area (a chart
/// degeneracy, e.g. a cylinder patch collapsed onto its seam) — all cases where
/// a `Patch` would be malformed rather than merely awkward.
///
/// **Seam caveat, inherited from the chart:** a cylinder patch straddling
/// `θ = ±π` projects to a self-crossing boundary and must be unwrapped by the
/// caller first. This function does not unwrap, and does not detect it — the
/// splice loop must, before it trusts the result.
// Superseded in production by `patch_from_cycles_shifted` (the splice loop
// always has a shift map, empty for planes). Kept as the no-shift form its own
// tests exercise; `allow(dead_code)` is scoped to this one wrapper so it cannot
// hide an unreachable arm anywhere else.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn patch_from_cycles(
    chart: &SurfaceChart,
    verts: &[Point3],
    cycles: &[Vec<u32>],
) -> Option<(crate::stage4_update::Patch, Vec<u32>)> {
    patch_from_cycles_shifted(chart, verts, cycles, &std::collections::BTreeMap::new())
}

/// As [`patch_from_cycles`], but adds `theta_shift[v]` to the FIRST parametric
/// coordinate of mesh vertex `v` before any winding or area decision.
///
/// This is the hook the splice loop's cylinder seam unwrapping needs. A patch
/// straddling `θ = ±π` projects to a self-crossing boundary, so its shoelace
/// area is meaningless — the branch choice has to be applied *before* the CCW
/// normalization below, not patched onto the result afterwards.
///
/// Every shift MUST be a multiple of `2π`, which makes the transformation a
/// **no-op in world space**: [`SurfaceChart::lift`] is `2π`-periodic in `θ`, so
/// a shifted patch lifts back to exactly the same 3D points. The caller
/// ([`crate::stage4_splice::unwrap_theta`]) is what guarantees that; this
/// function does not re-check it.
///
/// For a `Plane` chart the shift is meaningless and callers pass an empty map;
/// `patch_from_cycles` delegates here with one, so its behaviour is unchanged.
pub(crate) fn patch_from_cycles_shifted(
    chart: &SurfaceChart,
    verts: &[Point3],
    cycles: &[Vec<u32>],
    theta_shift: &std::collections::BTreeMap<u32, f64>,
) -> Option<(crate::stage4_update::Patch, Vec<u32>)> {
    if cycles.is_empty() || cycles.iter().any(|c| c.len() < 3) {
        return None;
    }
    let mut p2: Vec<Point2> = Vec::new();
    let mut back: Vec<u32> = Vec::new();
    let mut loops: Vec<Vec<u32>> = Vec::with_capacity(cycles.len());
    for cyc in cycles {
        let mut idx = Vec::with_capacity(cyc.len());
        for &v in cyc {
            let w = *verts.get(v as usize)?;
            let uv = chart.project(w);
            let shift = theta_shift.get(&v).copied().unwrap_or(0.0);
            idx.push(p2.len() as u32);
            p2.push(Point2::new(uv.x() + shift, uv.y()));
            back.push(v);
        }
        loops.push(idx);
    }
    // Shoelace on the OUTER loop, in chart coordinates.
    let outer = &mut loops[0];
    let area2: f64 = outer
        .iter()
        .enumerate()
        .map(|(i, &a)| {
            let b = outer[(i + 1) % outer.len()];
            let (pa, pb) = (p2[a as usize], p2[b as usize]);
            pa.x() * pb.y() - pb.x() * pa.y()
        })
        .sum();
    if area2 == 0.0 || !area2.is_finite() {
        return None;
    }
    if area2 < 0.0 {
        outer.reverse();
    }
    let mut it = loops.into_iter();
    let boundary = it.next()?;
    Some((
        crate::stage4_update::Patch {
            verts: p2,
            boundary,
            holes: it.collect(),
        },
        back,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage4_update::{
        two_sided_conformal_update_lifted, MeshUpdateOpts, Patch, Polyline, TwoSidedUpdate,
    };
    use crate::Vector3;

    fn dist3(a: Point3, b: Point3) -> f64 {
        let d = [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }

    // ---- Failure-region detector. ---------------------------------------

    /// A CLOSED tetrahedron (verts `base..base+4`) — a valid 2-manifold, so the
    /// detector sees no imbalanced edge. Faces oriented outward.
    fn tetra(base: u32) -> Vec<[u32; 3]> {
        let (a, b, c, d) = (base, base + 1, base + 2, base + 3);
        vec![[a, b, c], [a, d, b], [a, c, d], [b, d, c]]
    }

    /// The same tetrahedron but with face `[a,b,c]` split at a midpoint `base+4`
    /// on edge `a-b` — so edge (a,b) is subdivided on ONE side only, exactly the
    /// C0044-class seam mismatch. Returns (tris, the unpaired undirected edges).
    fn tetra_with_split_seam(base: u32) -> Vec<[u32; 3]> {
        let (a, b, c, d, m) = (base, base + 1, base + 2, base + 3, base + 4);
        // Split face [a,b,c] into [a,m,c] + [m,b,c] (seam a→m→b); the opposite
        // face [b,d,c] still carries the un-split edge b→...; face [a,d,b] carries
        // edge a→...→b — the imbalance lands on (a,b),(a,m),(b,m).
        vec![[a, m, c], [m, b, c], [a, d, b], [a, c, d], [b, d, c]]
    }

    #[test]
    fn detector_empty_on_closed_manifold() {
        let tris = tetra(0);
        let attr = |_: usize| Some((true, 0u32));
        assert!(detect_nonmanifold_seams(&tris, &attr).is_empty());
    }

    #[test]
    fn detector_finds_mismatched_seam_and_both_patches() {
        // The seam edge a-b (0-1) is shared by the split face (halves = tris 0,1,
        // patch A) and its neighbour [a,d,b] (tri 2, patch B (false,7)). The
        // imbalance is exactly the A/B seam mismatch.
        let tris = tetra_with_split_seam(0);
        let attr = |ti: usize| Some((ti != 2, if ti == 2 { 7 } else { 0 }));
        let regions = detect_nonmanifold_seams(&tris, &attr);
        assert_eq!(regions.len(), 1, "one connected mismatched seam run");
        let r = &regions[0];
        // Edges: a-b (0-1), a-m (0-4), b-m (1-4).
        assert_eq!(r.edges, vec![(0, 1), (0, 4), (1, 4)]);
        // Both patches named — the pair the mesh-update must reconcile.
        assert_eq!(r.keys, vec![(false, 7), (true, 0)]);
    }

    #[test]
    fn detector_separates_disjoint_regions() {
        // Two independent split tetrahedra (disjoint vertex sets) → two regions.
        let mut tris = tetra_with_split_seam(0);
        tris.extend(tetra_with_split_seam(10));
        let attr = |ti: usize| {
            let local = ti % 5;
            Some((local != 2, ti as u32))
        };
        let regions = detect_nonmanifold_seams(&tris, &attr);
        assert_eq!(regions.len(), 2, "two vertex-disjoint runs");
        assert!(regions.iter().all(|r| r.edges.len() == 3));
    }

    // ---- Round-trip: lift ∘ project = identity for on-surface points. -------

    #[test]
    fn plane_chart_round_trips_on_surface_points() {
        // A tilted plane through (1,0,0) with a non-axis normal.
        let n = Vector3::new(1.0, 2.0, 2.0); // |n| = 3
        let surf = Surface::Plane {
            normal: Vector3::new(1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0),
            d: -(1.0 / 3.0), // plane passes through (1,0,0): n·x + d = 1/3 - 1/3 = 0
        };
        let _ = n;
        let chart = SurfaceChart::new(surf).unwrap();
        // Points ON the plane (built by lifting arbitrary params).
        for &(u, v) in &[(0.0, 0.0), (1.5, -2.0), (-3.0, 4.0), (10.0, 10.0)] {
            let w = chart.lift(Point2::new(u, v));
            let uv2 = chart.project(w);
            assert!(
                (uv2.x() - u).abs() < 1e-12 && (uv2.y() - v).abs() < 1e-12,
                "plane project∘lift must be identity: ({u},{v}) -> {uv2:?}"
            );
            // And the lifted point lies on the plane.
            let sd = crate::signed_distance_to_surface(surf, w).unwrap();
            assert!(sd.abs() < 1e-12, "lifted point off plane by {sd}");
        }
    }

    #[test]
    fn cylinder_chart_round_trips_on_surface_points() {
        let surf = Surface::Cylinder {
            axis_point: Point3::new(0.5, -0.5, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 2.0),
            radius: 2.0,
        };
        let chart = SurfaceChart::new(surf).unwrap();
        for &(theta, z) in &[(0.0, 0.0), (1.0, 3.0), (-2.5, -1.0), (3.0, 5.0)] {
            let w = chart.lift(Point2::new(theta, z));
            // On the cylinder.
            let sd = crate::signed_distance_to_surface(surf, w).unwrap();
            assert!(sd.abs() < 1e-12, "lifted point off cylinder by {sd}");
            // project returns the same (θ mod 2π, z); lift(project(w)) == w.
            let w2 = chart.lift(chart.project(w));
            assert!(
                dist3(w, w2) < 1e-12,
                "cyl lift∘project∘lift drift {w:?} {w2:?}"
            );
        }
    }

    #[test]
    fn unsupported_surfaces_have_no_chart() {
        assert!(SurfaceChart::new(Surface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 1.0
        })
        .is_none());
        assert!(SurfaceChart::new(Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: 0.5
        })
        .is_none());
        assert!(SurfaceChart::new(Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: 2.0,
            minor_radius: 0.5
        })
        .is_none());
    }

    // ---- Integration: chart + frame-agnostic driver on a REAL surface pair. --

    /// Build a rectangular patch in `chart` param space that straddles the chord
    /// `p0 → p2` (given already in param space): P0 and P2 sit on the two short
    /// boundary edges (an edge-split each), the chord runs through the interior.
    /// Works for any (possibly diagonal) chord.
    fn rect_around_chord(p0: Point2, p2: Point2, halfwidth: f64) -> Patch {
        let dir = [p2.x() - p0.x(), p2.y() - p0.y()];
        let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
        let (dx, dy) = (dir[0] / len, dir[1] / len);
        // Perpendicular.
        let (px, py) = (-dy * halfwidth, dx * halfwidth);
        Patch {
            verts: vec![
                Point2::new(p0.x() - px, p0.y() - py), // 0
                Point2::new(p0.x() + px, p0.y() + py), // 1  (edge 0-1 hosts P0 at t=.5)
                Point2::new(p2.x() + px, p2.y() + py), // 2
                Point2::new(p2.x() - px, p2.y() - py), // 3  (edge 2-3 hosts P2 at t=.5)
            ],
            boundary: vec![0, 1, 2, 3],
            holes: vec![],
        }
    }

    /// A plane tangent to a cylinder shares ONE generator line (the #168 R0038
    /// geometry). Re-triangulating the plane patch and the cylinder patch against
    /// that shared generator, each in its OWN chart, produces a seam that
    /// coincides in 3D — the Phase-B two-sided update on a genuine surface pair.
    #[test]
    fn plane_tangent_cylinder_generator_is_conformal() {
        // Cylinder: axis = z, radius 2 → tangent generator at θ=0 is the line
        // x=2, y=0, z free.
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.0,
        };
        // Tangent plane at that generator: x = 2, i.e. normal (1,0,0), d = -2.
        let plane = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: -2.0,
        };
        let chart_p = SurfaceChart::new(plane).unwrap();
        let chart_c = SurfaceChart::new(cyl).unwrap();

        // The ONE shared 3D curve: the generator (2,0,z), z = 0, 0.5, 1.
        let world: Vec<Point3> = vec![
            Point3::new(2.0, 0.0, 0.0),
            Point3::new(2.0, 0.0, 0.5),
            Point3::new(2.0, 0.0, 1.0),
        ];
        // Sanity: every world point lies on BOTH surfaces.
        for &w in &world {
            assert!(crate::signed_distance_to_surface(plane, w).unwrap().abs() < 1e-12);
            assert!(crate::signed_distance_to_surface(cyl, w).unwrap().abs() < 1e-12);
        }

        // Project into each chart.
        let pa: Vec<Point2> = world.iter().map(|&w| chart_p.project(w)).collect();
        let pc: Vec<Point2> = world.iter().map(|&w| chart_c.project(w)).collect();
        let curve_a = Polyline {
            points: pa.clone(),
            closed: false,
        };
        let curve_c = Polyline {
            points: pc.clone(),
            closed: false,
        };

        // A patch around the chord in each chart.
        let patch_a = rect_around_chord(pa[0], pa[2], 1.0);
        let patch_c = rect_around_chord(pc[0], pc[2], 0.5);

        let opts = MeshUpdateOpts {
            merge_tol: 1e-6,
            d_eps: 1e-2,
        };
        let ts: TwoSidedUpdate = two_sided_conformal_update_lifted(
            &patch_a,
            |q| chart_p.lift(q),
            &curve_a,
            &patch_c,
            |q| chart_c.lift(q),
            &curve_c,
            opts,
            1e-9,
        )
        .expect("plane-tangent-cylinder seam must be conformal");

        assert_eq!(ts.seam.len(), 3);
        // Every paired seam vertex lifts to the SAME world point — and it is the
        // original shared curve point.
        for (i, &(ia, ib)) in ts.seam.iter().enumerate() {
            let wa = chart_p.lift(ts.a.verts[ia as usize]);
            let wc = chart_c.lift(ts.b.verts[ib as usize]);
            assert!(dist3(wa, wc) < 1e-9, "seam pair {i} diverges in world");
            assert!(
                dist3(wa, world[i]) < 1e-9,
                "seam pair {i} off the shared curve"
            );
        }
    }
}

#[cfg(test)]
mod patch_extraction_tests {
    use super::*;
    use cad_primitives::Vector3;

    fn xy_plane() -> SurfaceChart {
        SurfaceChart::new(Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        })
        .unwrap()
    }

    #[test]
    fn extracts_boundary_and_index_map() {
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ];
        let cycles = [vec![0u32, 1, 2, 3]];
        let (patch, back) = patch_from_cycles(&xy_plane(), &verts, &cycles).unwrap();
        assert_eq!(patch.boundary.len(), 4);
        assert!(patch.holes.is_empty());
        assert_eq!(back, vec![0, 1, 2, 3]);
        // Every patch vertex lifts back to the mesh vertex it came from.
        let ch = xy_plane();
        for (pi, &mv) in back.iter().enumerate() {
            let round = ch.lift(patch.verts[pi]);
            let want = verts[mv as usize].as_array();
            let got = round.as_array();
            for k in 0..3 {
                assert!((got[k] - want[k]).abs() < 1e-12, "{got:?} vs {want:?}");
            }
        }
    }

    #[test]
    fn outer_boundary_is_ccw_normalized() {
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ];
        let ccw = patch_from_cycles(&xy_plane(), &verts, &[vec![0u32, 1, 2, 3]]).unwrap();
        let cw = patch_from_cycles(&xy_plane(), &verts, &[vec![3u32, 2, 1, 0]]).unwrap();
        let area = |p: &crate::stage4_update::Patch| -> f64 {
            p.boundary
                .iter()
                .enumerate()
                .map(|(i, &a)| {
                    let b = p.boundary[(i + 1) % p.boundary.len()];
                    let (pa, pb) = (p.verts[a as usize], p.verts[b as usize]);
                    pa.x() * pb.y() - pb.x() * pa.y()
                })
                .sum()
        };
        assert!(area(&ccw.0) > 0.0, "already CCW must stay CCW");
        assert!(area(&cw.0) > 0.0, "CW input must be reversed to CCW");
    }

    #[test]
    fn holes_are_carried_and_not_rewound() {
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(4.0, 0.0, 0.0),
            Point3::new(4.0, 4.0, 0.0),
            Point3::new(0.0, 4.0, 0.0),
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(1.0, 2.0, 0.0),
            Point3::new(2.0, 2.0, 0.0),
        ];
        let cycles = [vec![0u32, 1, 2, 3], vec![4u32, 5, 6]];
        let (patch, back) = patch_from_cycles(&xy_plane(), &verts, &cycles).unwrap();
        assert_eq!(patch.holes.len(), 1);
        assert_eq!(patch.holes[0].len(), 3);
        assert_eq!(back.len(), 7);
        // The hole's mesh identities survive the trip.
        let hole_mesh: Vec<u32> = patch.holes[0].iter().map(|&i| back[i as usize]).collect();
        assert_eq!(hole_mesh, vec![4, 5, 6]);
    }

    #[test]
    fn malformed_inputs_are_rejected_not_patched() {
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(2.0, 0.0, 0.0),
        ];
        let ch = xy_plane();
        // Fewer than 3 vertices.
        assert!(patch_from_cycles(&ch, &verts, &[vec![0u32, 1]]).is_none());
        // Index out of range.
        assert!(patch_from_cycles(&ch, &verts, &[vec![0u32, 1, 9]]).is_none());
        // Zero projected area (collinear boundary) — a malformed Patch, not an
        // awkward one.
        assert!(patch_from_cycles(&ch, &verts, &[vec![0u32, 1, 2]]).is_none());
        // No cycles at all.
        assert!(patch_from_cycles(&ch, &verts, &[]).is_none());
    }

    #[test]
    fn cylinder_chart_round_trips_through_extraction() {
        let ch = SurfaceChart::new(Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.0,
        })
        .unwrap();
        let ang = [0.2f64, 0.5, 0.9, 1.3];
        let verts: Vec<Point3> = ang
            .iter()
            .enumerate()
            .map(|(i, &t)| Point3::new(2.0 * t.cos(), 2.0 * t.sin(), i as f64 * 0.25))
            .collect();
        let (patch, back) = patch_from_cycles(&ch, &verts, &[vec![0u32, 1, 2, 3]]).unwrap();
        for (pi, &mv) in back.iter().enumerate() {
            let got = ch.lift(patch.verts[pi]).as_array();
            let want = verts[mv as usize].as_array();
            for k in 0..3 {
                assert!((got[k] - want[k]).abs() < 1e-9, "{got:?} vs {want:?}");
            }
        }
    }
}
