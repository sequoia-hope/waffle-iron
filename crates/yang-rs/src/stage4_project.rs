//! Per-operand parametric charts for the §4.4.1 patch rebuilds.
//!
//! The construct pass (`stage4_construct`) re-triangulates each patch in its
//! OWN parametric domain. To feed it, it needs, per analytic surface, a chart:
//!
//! * `project`: world `Point3` (on/near the surface) → parametric `Point2`,
//! * `lift`:    parametric `Point2` → world `Point3`,
//!
//! that are mutual inverses for points ON the surface. Then the ONE shared 3D
//! intersection curve projects into each patch's chart, the patch re-CDTs in
//! param space, and the result lifts back conformally (both sides land on the
//! same world curve → 2-manifold seam, Yang 2025 §4.4.1).
//!
//! Charts are provided for **Plane**, **Cylinder**, and (I13a, always-on
//! since 2026-08-25) **Cone**. Sphere / Torus return `None` for now, so the
//! construct pass simply does not engage those patches and leaves them
//! byte-identical.

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
/// * `Cone` (I13a, always-on): param =
///   `(θ, z)` with `z` the
///   axial station FROM THE APEX and radius `z·tan(α)` — the same single-nappe
///   `v ≥ 0` convention `stage4_dt::d_of_t` certifies, in the same
///   `ortho_basis(axis)` frame. Injective for `z > 0` given the caller's
///   θ-unwrap; the apex (`z = 0`) has no azimuth and `z ≤ 0` is off the nappe,
///   so [`rebuild ...`](crate::stage4_construct::rebuild_patch_planar) refuses
///   any patch touching either — loudly, never a fallback.
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
    Cone {
        apex: [f64; 3],
        axis: [f64; 3],
        e1: [f64; 3],
        e2: [f64; 3],
        tan_half: f64,
    },
    /// §4.5.1 inc-2c-3b-3 (spec `specs/yang_451_corner_transit.md`): param =
    /// `(θ, φ)` — azimuth about the axis and tube angle — the pinned
    /// `stage4_dt::eval_uv` §2 embedding
    /// `center + (R + r·cos φ)(cos θ·e1 + sin θ·e2) + r·sin φ·â`, in the same
    /// `ortho_basis(axis)` frame, RING torus only (`R > r > 0`, the
    /// `validate_surface` rule — every on-surface point has radial distance
    /// ≥ R − r > 0, so both angles are defined). DOUBLY periodic: `lift`
    /// inverts `project` modulo `2π` in BOTH coordinates, so a caller must
    /// chain-unwrap θ AND φ before CDT. Only [`new_local`](Self::new_local)
    /// builds this variant — a torus patch may wrap a full period in either
    /// direction, so wholesale-patch holders (which chart entire boundary
    /// cycles) keep refusing tori until seam machinery exists.
    Torus {
        center: [f64; 3],
        axis: [f64; 3],
        e1: [f64; 3],
        e2: [f64; 3],
        major: f64,
        minor: f64,
    },
}

// I13a — the Cone chart (and with it, cone-owner rebuilds in the
// construct/fold-merge/corner-merge/rim-trim passes) is ALWAYS-ON since
// 2026-08-25 (corpus category-identical, 271C/0W/36E/1EE, two explained
// detail rows: R0003 advances 437→467, R0004's ring-CDT wall clears).
// §4.5.1 inc-2c-3b-3 — the Torus chart for FAN-LOCAL windows
// ([`SurfaceChart::new_local`] holders only) is ALWAYS-ON since inc-3a
// (2026-09-03; the R0074 far-fan refill is the naming case). Their
// `YANG_441_CONE_CHART` / `YANG_441_TORUS_CHART` off-knobs were deleted
// 2026-10-16 (cleanup spec §4).

impl SurfaceChart {
    /// Build a chart for `surface`, or `None` for a surface type Phase B does
    /// not yet re-triangulate (Sphere / Torus). The caller keeps its existing
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
            Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                let (e1v, e2v) = ortho_basis(axis_dir);
                Some(SurfaceChart::Cone {
                    apex: apex.as_array(),
                    axis: normalize3(axis_dir.as_array()),
                    e1: e1v.as_array(),
                    e2: e2v.as_array(),
                    tan_half: half_angle.tan(),
                })
            }
            Surface::Sphere { .. } | Surface::Torus { .. } => None,
        }
    }

    /// §4.5.1 inc-2c-3b-3 — the FAN-LOCAL chart: everything [`new`](Self::new)
    /// charts, plus a ring Torus. Valid ONLY
    /// for a simply-connected local window that wraps neither period — a fan
    /// hole's link polygon, never a whole patch boundary cycle — and the
    /// caller owns the chain-unwrap of BOTH coordinates (θ and φ are each
    /// `2π`-periodic). Non-ring tori (horn/spindle, `r ≥ R`) stay `None`:
    /// they self-intersect and `stage4_dt::validate_surface` refuses them
    /// anyway, so the holder's existing typed decline is the honest verdict.
    pub(crate) fn new_local(surface: Surface) -> Option<Self> {
        if let Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } = surface
        {
            let finite = center.as_array().iter().all(|c| c.is_finite())
                && axis_dir.as_array().iter().all(|c| c.is_finite())
                && major_radius.is_finite()
                && minor_radius.is_finite();
            let ring = minor_radius > 0.0 && major_radius > minor_radius;
            let len2 = {
                let a = axis_dir.as_array();
                a[0] * a[0] + a[1] * a[1] + a[2] * a[2]
            };
            // `len2` is finite whenever `finite` holds (a sum of squares of
            // finite components), so `<= 0.0` is exactly "degenerate axis".
            if !finite || !ring || len2 <= 0.0 {
                return None;
            }
            let (e1v, e2v) = ortho_basis(axis_dir);
            return Some(SurfaceChart::Torus {
                center: center.as_array(),
                axis: normalize3(axis_dir.as_array()),
                e1: e1v.as_array(),
                e2: e2v.as_array(),
                major: major_radius,
                minor: minor_radius,
            });
        }
        Self::new(surface)
    }

    /// The chartability pre-filter — the ONE capability test every construct /
    /// fold-merge / corner-merge / rim-trim holder gate consults (previously
    /// five hand-copied `matches!(Plane | Cylinder)` sites). `true` exactly
    /// when [`new`](Self::new) would return `Some`.
    pub(crate) fn supports(s: &Surface) -> bool {
        match s {
            Surface::Plane { .. } | Surface::Cylinder { .. } | Surface::Cone { .. } => true,
            Surface::Sphere { .. } | Surface::Torus { .. } => false,
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
            SurfaceChart::Cone {
                apex, axis, e1, e2, ..
            } => {
                let w = [x[0] - apex[0], x[1] - apex[1], x[2] - apex[2]];
                let z = dot(w, axis);
                let radial = [w[0] - z * axis[0], w[1] - z * axis[1], w[2] - z * axis[2]];
                let theta = dot(radial, e2).atan2(dot(radial, e1));
                Point2::new(theta, z)
            }
            SurfaceChart::Torus {
                center,
                axis,
                e1,
                e2,
                major,
                ..
            } => {
                let w = [x[0] - center[0], x[1] - center[1], x[2] - center[2]];
                let z = dot(w, axis);
                let radial = [w[0] - z * axis[0], w[1] - z * axis[1], w[2] - z * axis[2]];
                let theta = dot(radial, e2).atan2(dot(radial, e1));
                // On-surface: |radial| = R + r·cos φ and z = r·sin φ, so the
                // tube angle is atan2(z, |radial| − R) — well-defined for the
                // ring torus (|radial| ≥ R − r > 0).
                let rho =
                    (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
                let phi = z.atan2(rho - major);
                Point2::new(theta, phi)
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
            SurfaceChart::Cone {
                apex,
                axis,
                e1,
                e2,
                tan_half,
            } => {
                let (theta, z) = (uv.x(), uv.y());
                let (ct, st) = (theta.cos(), theta.sin());
                let r = z * tan_half;
                Point3::new(
                    apex[0] + r * (ct * e1[0] + st * e2[0]) + z * axis[0],
                    apex[1] + r * (ct * e1[1] + st * e2[1]) + z * axis[1],
                    apex[2] + r * (ct * e1[2] + st * e2[2]) + z * axis[2],
                )
            }
            SurfaceChart::Torus {
                center,
                axis,
                e1,
                e2,
                major,
                minor,
            } => {
                let (theta, phi) = (uv.x(), uv.y());
                let (ct, st) = (theta.cos(), theta.sin());
                let rho = major + minor * phi.cos();
                let z = minor * phi.sin();
                Point3::new(
                    center[0] + rho * (ct * e1[0] + st * e2[0]) + z * axis[0],
                    center[1] + rho * (ct * e1[1] + st * e2[1]) + z * axis[1],
                    center[2] + rho * (ct * e1[2] + st * e2[2]) + z * axis[2],
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

/// Build the parametric [`Patch`](crate::stage4_update::Patch) the construct
/// pass re-triangulates, from a 3D patch's boundary cycles.
///
/// `cycles[0]` is the outer boundary and the rest are holes — the order
/// [`crate::stage4_correct::PatchInfo`] already stores them in. Each cycle is
/// mesh-vertex indices; every vertex is projected through `chart`.
///
/// Returns the patch **and** the index map `patch vertex -> mesh vertex`, which
/// the rebuild needs to write the re-triangulated result back into the 3D mesh.
/// Without it the result is a set of 2D points with no way home.
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
/// Adds `theta_shift[v]` to the FIRST parametric coordinate of mesh vertex `v`
/// before any winding or area decision.
///
/// This is the hook the splice loop's cylinder seam unwrapping needs. A patch
/// straddling `θ = ±π` projects to a self-crossing boundary, so its shoelace
/// area is meaningless — the branch choice has to be applied *before* the CCW
/// normalization below, not patched onto the result afterwards.
///
/// Every shift MUST be a multiple of `2π`, which makes the transformation a
/// **no-op in world space**: [`SurfaceChart::lift`] is `2π`-periodic in `θ`, so
/// a shifted patch lifts back to exactly the same 3D points. The caller
/// (`stage4_splice::unwrap_theta`) is what guarantees that; this
/// function does not re-check it.
///
/// For a `Plane` chart the shift is meaningless and callers pass an empty map.
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
    // The OUTER loop is the one enclosing the others — the largest |area| in
    // chart coordinates — whatever order the patch's cycles arrived in. (A
    // hole first made the CDT carve the whole region away as "inside the
    // hole" and return nothing.) Ties keep the earlier loop.
    let shoelace = |l: &[u32]| -> f64 {
        l.iter()
            .enumerate()
            .map(|(i, &a)| {
                let b = l[(i + 1) % l.len()];
                let (pa, pb) = (p2[a as usize], p2[b as usize]);
                pa.x() * pb.y() - pb.x() * pa.y()
            })
            .sum()
    };
    let mut outer_at = 0usize;
    for (k, l) in loops.iter().enumerate().skip(1) {
        if shoelace(l).abs() > shoelace(&loops[outer_at]).abs() {
            outer_at = k;
        }
    }
    loops.swap(0, outer_at);
    let outer = &mut loops[0];
    let area2: f64 = shoelace(outer);
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

/// Phase 2b (spec `yang_441_phase2b_patch_update.md` §6 item 1): the chart
/// polygon of a BAND — a cylinder or cone patch two of whose cycles encircle
/// the axis (the parametric domain of a closed surface is periodic in θ; a
/// band has no single branch that unrolls it, `unwrap_theta`'s
/// `PatchEncirclesAxis`). The band is cut open along a SEAM: a chart segment
/// joining a vertex `a` of one encircling cycle to a vertex `b` of the other
/// that crosses no cycle edge on any 2π copy. The polygon walks cycle A in its
/// own direction from `a` once around (to `a` + 2π·w_A), the seam to `b`'s
/// copy there, cycle B in its own direction once around, and the seam back to
/// `a`. Both seam copies are one 3D edge (`lift` is 2π-periodic), so the
/// triangles on its two sides pair; `back` maps both chart copies of `a` and
/// `b` to the same mesh vertex. Non-encircling cycles are holes, each placed on
/// the 2π branch whose first vertex lies inside the cut polygon. `None` when
/// the cycles are not exactly two encircling loops of opposite winding plus
/// holes, or no seam is clear.
pub(crate) fn band_patch_from_cycles(
    chart: &SurfaceChart,
    verts: &[Point3],
    cycles: &[Vec<u32>],
) -> Option<(crate::stage4_update::Patch, Vec<u32>)> {
    use std::f64::consts::TAU;
    if !matches!(
        chart,
        SurfaceChart::Cylinder { .. } | SurfaceChart::Cone { .. }
    ) {
        return None;
    }
    if cycles.iter().any(|c| c.len() < 3) {
        return None;
    }
    let uv = |v: u32| -> Option<Point2> { verts.get(v as usize).map(|&p| chart.project(p)) };
    // Unwrap a cycle starting from `start` (position index) at θ = `theta0`;
    // returns the unwrapped θ of each position in walk order and the winding.
    let unwrap_from = |cyc: &[u32], start: usize, theta0: f64| -> Option<(Vec<Point2>, i64)> {
        let n = cyc.len();
        let mut out = Vec::with_capacity(n + 1);
        let p0 = uv(cyc[start])?;
        out.push(Point2::new(theta0, p0.y()));
        for k in 1..=n {
            let q = uv(cyc[(start + k) % n])?;
            let prev = out[k - 1].x();
            let t = q.x() + ((prev - q.x()) / TAU).round() * TAU;
            out.push(Point2::new(t, q.y()));
        }
        let w = ((out[n].x() - theta0) / TAU).round() as i64;
        out.pop();
        Some((out, w))
    };
    let mut enc: Vec<(usize, i64)> = Vec::new();
    let mut holes_idx: Vec<usize> = Vec::new();
    for (ci, c) in cycles.iter().enumerate() {
        let (_, w) = unwrap_from(c, 0, uv(c[0])?.x())?;
        match w {
            0 => holes_idx.push(ci),
            1 | -1 => enc.push((ci, w)),
            _ => return None,
        }
    }
    if enc.len() != 2 || enc[0].1 != -enc[1].1 {
        return None;
    }
    let (ca, wa) = enc[0];
    let (cb, _) = enc[1];
    let (cyc_a, cyc_b) = (&cycles[ca], &cycles[cb]);

    let cross = |a0: Point2, a1: Point2, b0: Point2, b1: Point2| -> bool {
        let o = |p: Point2, q: Point2, r: Point2| {
            (q.x() - p.x()) * (r.y() - p.y()) - (q.y() - p.y()) * (r.x() - p.x())
        };
        let (d1, d2) = (o(a0, a1, b0), o(a0, a1, b1));
        let (d3, d4) = (o(b0, b1, a0), o(b0, b1, a1));
        // Proper crossings and touching both count; a shared endpoint is
        // excluded by the caller (it skips edges incident to the seam's ends).
        (d1 * d2 <= 0.0) && (d3 * d4 <= 0.0)
    };
    let clear = |sa: Point2, sb: Point2, va: u32, vb: u32| -> bool {
        for c in cycles {
            let n = c.len();
            let (u, _) = match unwrap_from(c, 0, uv(c[0]).map_or(0.0, |p| p.x())) {
                Some(x) => x,
                None => return false,
            };
            for i in 0..n {
                let (x, y) = (c[i], c[(i + 1) % n]);
                if x == va || x == vb || y == va || y == vb {
                    continue;
                }
                let p = u[i];
                let q0 = u[(i + 1) % n];
                let q = Point2::new(q0.x() + ((p.x() - q0.x()) / TAU).round() * TAU, q0.y());
                for k in -2..=2 {
                    let s = k as f64 * TAU;
                    let (ps, qs) = (Point2::new(p.x() + s, p.y()), Point2::new(q.x() + s, q.y()));
                    if cross(sa, sb, ps, qs) {
                        return false;
                    }
                }
            }
        }
        true
    };
    // Seam search: for each vertex of A, the three vertices of B nearest in θ
    // (on the branch nearest a); the shortest clear chart segment wins.
    let mut best: Option<(f64, usize, usize, f64)> = None;
    for (ia, &va) in cyc_a.iter().enumerate() {
        let pa = uv(va)?;
        let mut cand: Vec<(f64, usize, f64)> = cyc_b
            .iter()
            .enumerate()
            .filter_map(|(ib, &vb)| {
                let pb = uv(vb)?;
                let t = pb.x() + ((pa.x() - pb.x()) / TAU).round() * TAU;
                Some(((t - pa.x()).abs(), ib, t))
            })
            .collect();
        cand.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        for &(_, ib, tb) in cand.iter().take(3) {
            let pb = Point2::new(tb, uv(cyc_b[ib])?.y());
            let len = ((pb.x() - pa.x()).powi(2) + (pb.y() - pa.y()).powi(2)).sqrt();
            if best.is_some_and(|b| b.0 <= len) {
                continue;
            }
            if clear(pa, pb, va, cyc_b[ib]) {
                best = Some((len, ia, ib, tb));
            }
        }
    }
    let (_, ia, ib, tb) = best?;
    let pa0 = uv(cyc_a[ia])?;
    let (ua, _) = unwrap_from(cyc_a, ia, pa0.x())?;
    // B walks from `b` at the copy nearest a' = a + 2π·w_A.
    let tb_end = tb + wa as f64 * TAU;
    let (ub, _) = unwrap_from(cyc_b, ib, tb_end)?;
    let mut p2: Vec<Point2> = Vec::new();
    let mut back: Vec<u32> = Vec::new();
    let mut outer: Vec<u32> = Vec::new();
    let na = cyc_a.len();
    for k in 0..=na {
        // k = na is a' (a's copy one turn on), the seam's far end on A.
        let t = if k < na {
            ua[k]
        } else {
            Point2::new(ua[0].x() + wa as f64 * TAU, ua[0].y())
        };
        outer.push(p2.len() as u32);
        p2.push(t);
        back.push(cyc_a[(ia + k) % na]);
    }
    let nb = cyc_b.len();
    for k in 0..=nb {
        let t = if k < nb {
            ub[k]
        } else {
            Point2::new(ub[0].x() - wa as f64 * TAU, ub[0].y())
        };
        outer.push(p2.len() as u32);
        p2.push(t);
        back.push(cyc_b[(ib + k) % nb]);
    }
    let area2 = |lp: &[u32], p2: &[Point2]| -> f64 {
        (0..lp.len())
            .map(|i| {
                let (pa, pb) = (p2[lp[i] as usize], p2[lp[(i + 1) % lp.len()] as usize]);
                pa.x() * pb.y() - pb.x() * pa.y()
            })
            .sum()
    };
    let a2 = area2(&outer, &p2);
    if a2 == 0.0 || !a2.is_finite() {
        return None;
    }
    if a2 < 0.0 {
        outer.reverse();
    }
    let mut holes: Vec<Vec<u32>> = Vec::new();
    for &hi in &holes_idx {
        let h = &cycles[hi];
        let ph = uv(h[0])?;
        let mut placed = None;
        for k in -2..=2 {
            let t0 = ph.x() + k as f64 * TAU;
            if point_in_polygon(Point2::new(t0, ph.y()), &outer, &p2) {
                placed = Some(t0);
                break;
            }
        }
        let (uh, _) = unwrap_from(h, 0, placed?)?;
        let mut idx = Vec::with_capacity(h.len());
        for (k, &v) in h.iter().enumerate() {
            idx.push(p2.len() as u32);
            p2.push(uh[k]);
            back.push(v);
        }
        holes.push(idx);
    }
    Some((
        crate::stage4_update::Patch {
            verts: p2,
            boundary: outer,
            holes,
        },
        back,
    ))
}

/// Even-odd point-in-polygon on a chart loop (`lp` indexes `pool`).
pub(crate) fn point_in_polygon(pt: Point2, lp: &[u32], pool: &[Point2]) -> bool {
    let mut inside = false;
    let n = lp.len();
    for i in 0..n {
        let (a, b) = (pool[lp[i] as usize], pool[lp[(i + 1) % n] as usize]);
        if (a.y() > pt.y()) != (b.y() > pt.y()) {
            let x = a.x() + (pt.y() - a.y()) * (b.x() - a.x()) / (b.y() - a.y());
            if pt.x() < x {
                inside = !inside;
            }
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
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

    // Phase 2b: a cylinder BAND (two rims encircling the axis, opposite
    // winding — the patch's material between them) cuts open along a clear
    // seam into one simple chart polygon whose area is the band's (2π·Δz on
    // the unit cylinder), with both seam copies mapped to the same vertices.
    #[test]
    fn a_cylinder_band_cuts_open_into_one_simple_polygon() {
        let surf = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let chart = SurfaceChart::new(surf).expect("cylinder chart");
        let n = 6u32;
        let mut verts = Vec::new();
        for z in [0.0, 1.0] {
            for i in 0..n {
                let t = std::f64::consts::TAU * (i as f64 + 0.3 * z) / n as f64;
                verts.push(Point3::new(t.cos(), t.sin(), z));
            }
        }
        let bottom: Vec<u32> = (0..n).collect();
        let top: Vec<u32> = (n..2 * n).rev().collect();
        let (patch, back) =
            band_patch_from_cycles(&chart, &verts, &[bottom.clone(), top.clone()]).expect("band");
        assert!(patch.holes.is_empty());
        assert_eq!(
            patch.boundary.len(),
            2 * n as usize + 2,
            "both rims plus two seam copies"
        );
        let area2: f64 = (0..patch.boundary.len())
            .map(|i| {
                let a = patch.verts[patch.boundary[i] as usize];
                let b = patch.verts[patch.boundary[(i + 1) % patch.boundary.len()] as usize];
                a.x() * b.y() - b.x() * a.y()
            })
            .sum();
        // The rims are chords, so the chart polygon's area is the band's
        // 2π·Δz exactly (the chart is (θ, z) and the rims sit at constant z).
        assert!(
            (area2 * 0.5 - std::f64::consts::TAU).abs() < 1e-12,
            "area {}",
            area2 * 0.5
        );
        // Every chart vertex lifts back to its mesh vertex.
        for (k, &v) in back.iter().enumerate() {
            let lifted = chart.lift(patch.verts[k]);
            assert!(dist3(lifted, verts[v as usize]) < 1e-12);
        }
        // One rim alone (or two rims of the SAME winding) is not a band.
        assert!(band_patch_from_cycles(&chart, &verts, std::slice::from_ref(&bottom)).is_none());
        let top_same: Vec<u32> = (n..2 * n).collect();
        assert!(band_patch_from_cycles(&chart, &verts, &[bottom, top_same]).is_none());
    }

    #[test]
    fn point_in_polygon_even_odd() {
        let pool = vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
        ];
        let lp = [0u32, 1, 2, 3];
        assert!(point_in_polygon(Point2::new(1.0, 1.0), &lp, &pool));
        assert!(!point_in_polygon(Point2::new(3.0, 1.0), &lp, &pool));
        assert!(!point_in_polygon(Point2::new(1.0, -0.5), &lp, &pool));
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
        // I13a (always-on since 2026-08-25): the cone IS charted.
        assert!(SurfaceChart::new(Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: 0.5
        })
        .is_some());
        assert!(SurfaceChart::new(Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: 2.0,
            minor_radius: 0.5
        })
        .is_none());
    }

    /// inc-2c-3b-3 — the fan-local Torus chart agrees with the PINNED
    /// `stage4_dt::eval_uv` §2 embedding (the like-for-like d(T) budget
    /// projects through the chart and certifies through `d_of_t`, so the two
    /// conventions must be one), and round-trips on-surface points. Built
    /// directly (the fan-local [`SurfaceChart::new_local`] torus path is
    /// covered by the construct-side fixture).
    #[test]
    fn torus_chart_matches_the_pinned_dt_embedding_and_round_trips() {
        let axis = Vector3::new(0.3, -0.2, 0.9);
        let surf = Surface::Torus {
            center: Point3::new(0.5, -0.25, 1.0),
            axis_dir: axis,
            major_radius: 3.0,
            minor_radius: 0.7,
        };
        let (e1v, e2v) = ortho_basis(axis);
        let chart = SurfaceChart::Torus {
            center: [0.5, -0.25, 1.0],
            axis: normalize3(axis.as_array()),
            e1: e1v.as_array(),
            e2: e2v.as_array(),
            major: 3.0,
            minor: 0.7,
        };
        for &(u, v) in &[
            (0.0, 0.0),
            (1.0, -2.0),
            (-2.5, 3.0),
            (3.1, 3.1),
            (-3.1, -3.1),
        ] {
            let w = chart.lift(Point2::new(u, v));
            let e = crate::stage4_dt::eval_uv(&surf, Point2::new(u, v)).unwrap();
            assert!(
                dist3(w, e) < 1e-12,
                "chart.lift diverges from eval_uv at ({u},{v}): {w:?} vs {e:?}"
            );
            let w2 = chart.lift(chart.project(w));
            assert!(
                dist3(w, w2) < 1e-12,
                "torus lift∘project∘lift drift {w:?} {w2:?}"
            );
            let sd = crate::signed_distance_to_surface(surf, w).unwrap();
            assert!(sd.abs() < 1e-11, "lifted point off torus by {sd}");
        }
    }
}

#[cfg(test)]
mod patch_extraction_tests {
    use super::*;
    use cad_primitives::Vector3;

    /// The no-shift form of [`patch_from_cycles_shifted`].
    fn patch_from_cycles(
        chart: &SurfaceChart,
        verts: &[Point3],
        cycles: &[Vec<u32>],
    ) -> Option<(crate::stage4_update::Patch, Vec<u32>)> {
        patch_from_cycles_shifted(chart, verts, cycles, &std::collections::BTreeMap::new())
    }

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
    fn the_enclosing_loop_is_the_outer_whatever_its_cycle_order() {
        // A 4×4 square with a unit-square hole; the HOLE arrives first (the
        // relocated yr10 cap's order). The enclosing loop must become the
        // boundary, the hole a hole, and the CDT then tiles the annulus.
        let verts = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(4.0, 0.0, 0.0),
            Point3::new(4.0, 4.0, 0.0),
            Point3::new(0.0, 4.0, 0.0),
            Point3::new(1.5, 1.5, 0.0),
            Point3::new(1.5, 2.5, 0.0),
            Point3::new(2.5, 2.5, 0.0),
            Point3::new(2.5, 1.5, 0.0),
        ];
        let cycles = [vec![4u32, 5, 6, 7], vec![0u32, 1, 2, 3]];
        let (patch, back) = patch_from_cycles(&xy_plane(), &verts, &cycles).unwrap();
        let outer: Vec<u32> = patch.boundary.iter().map(|&i| back[i as usize]).collect();
        let mut sorted = outer.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1, 2, 3], "the 4×4 square is the outer loop");
        assert_eq!(patch.holes.len(), 1);
        let tris = cherchi_rs::cdt_with_interior_constraints(
            &patch.verts,
            &patch.boundary,
            &patch.holes,
            &[],
            &[],
        )
        .expect("the annulus triangulates");
        let area: f64 = tris
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| patch.verts[i as usize]);
                0.5 * ((b.x() - a.x()) * (c.y() - a.y()) - (c.x() - a.x()) * (b.y() - a.y())).abs()
            })
            .sum();
        assert!(
            (area - 15.0).abs() < 1e-12,
            "annulus area 16 − 1, got {area}"
        );
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
