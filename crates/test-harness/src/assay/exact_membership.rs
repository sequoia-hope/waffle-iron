//! EXACT analytic membership for a corpus document — the composed solid as a
//! closed-form point predicate, with no tessellation and no kernel anywhere.
//!
//! Why it exists. Every other topology/volume reference the harness has goes
//! through a mesh: the volume oracle scans kernel-v2 tessellations of the
//! isolated operands, the topology oracle voxelises those scans, and the
//! Cherchi sidecar unions them. On 2026-09-03 the sidecar's union of R0053's
//! tessellated operands read "genus 15" — a closed manifold with 606 faces
//! strictly INSIDE the true solid (spec `yang_451_corner_transit.md` §3ah) —
//! and the lattice ladder over the same tessellations was unstable. The
//! reading with no mesh in it (closed-form predicates for the three
//! operands, cubical `χ` of their set union on a resolution ladder) was
//! stable at genus 1, the kernel's answer. This module makes that reading a
//! document-driven instrument.
//!
//! What it covers — exactly the feature-engine semantics (`rebuild.rs`):
//!
//! - **Sketch planes** from the sketch's own `plane_origin` / `plane_normal`
//!   (every corpus sketch carries them), with the UI basis
//!   (`SketchPlaneBasis` / `tangent_x_from_normal`: `x̂ = normalize(ẑ × n̂)`,
//!   `ŷ = n̂ × x̂`) — or, when the sketch names a `plane_x_axis`, that axis
//!   projected into the plane (`SketchPlaneBasis::from_origin_normal_x`).
//! - **Profiles**: the face the kernel adapter stages for `profile_index`
//!   (`kernel_v2::adapter::make_faces_from_profiles`, one face per input
//!   profile): an `is_outer` loop becomes a REGION with every `is_outer =
//!   false` loop whose centroid it contains attached as a hole (each hole to
//!   the smallest strictly-larger containing outer), an inner loop's own index
//!   stages that loop as a standalone face. A loop is a polygon (its
//!   `vertex_ids` in loop order, positions from `solved_positions`), a
//!   circle, or a `Gear` entity's involute disc in closed form (the layout of
//!   `waffle_types::gear::generate_gear_profile`: tooth `k` centred at
//!   `rotation_offset + k·2π/N`, radial flank root→base at the half-tooth
//!   angle `π/(2N) + inv(α) − backlash/(2 r_p)`, involute base→addendum with
//!   polar offset `inv(t) = t − atan t`, `t = √((r/r_b)² − 1)`, tip arc at
//!   `r_p + m`, root arc at `max(r_p − 1.25 m, r_b/2)`).
//! - **Extrude**: `direction = params.direction ∨ plane_normal`; Blind depth;
//!   a CUT with no explicit direction auto-reverses when the measured
//!   material's mid-extent along the direction lies behind the sketch plane
//!   (bracketing the true extent between extreme occupied lattice samples
//!   `(occ_lo, occ_hi)` and a sound analytic/KD-tree outer bound
//!   `(outer_lo, outer_hi)`, and declining via `indeterminate` when the sketch
//!   plane lies inside the interval of possible midpoints
//!   `[mid(outer_lo, occ_hi), mid(occ_lo, outer_hi)]`), and `ThroughAll` is
//!   then measured along the direction the extrude ACTUALLY sweeps: `outer_proj`
//!   past the plane (a guaranteed outer bound clearing every thin tip of the
//!   body) plus `THROUGH_ALL_OVERSHOOT` (1e-2) of that outer extent along the
//!   axis. `symmetric` / `second_direction: Blind` shift the
//!   face origin back by the second depth and extrude the sum. A sheared prism
//!   (direction ∦ normal) is handled exactly: the solid is
//!   `{face_origin + (u, v) + t·d}`.
//! - **Revolve**: axis origin/direction in the plane, `ŵ` toward the profile,
//!   sweep velocity `m̂ = â × ŵ` at θ = 0 (kernel-v2 `construct/revolve.rs`),
//!   angle in degrees, full turn at 360.
//! - **Bodies and combine modes** (`feature-engine` `normalize_combine` +
//!   the rebuild's combine dispatch): a document is a LIST of bodies. A
//!   legacy feature (`combine: null`) is `Cut` when `cut`, else `Add` when
//!   `merge`, else `NewBody`, targeting every live body of the most recent
//!   solid-bearing feature; a new-style feature carries its `combine` verb
//!   and an explicit `targets` list (`FeatureOutput` anchors with their
//!   `OutputKey` slot — `Main` or `Body { index }`).
//!   `NewBody` pushes a standalone body; `Add` folds the connected component
//!   of targets whose solid meets the tool (interior witness or exact coplanar
//!   planar-face overlap) into ONE body (Add into nothing = standalone) and
//!   re-emits targets provably separated beyond `TAU_MODEL` as this feature's
//!   LEFTOVER `Body { index }` outputs (the engine's disjoint-merge rule).
//!   Contacts below resolution propagate both fold and split hypotheses (keeping
//!   the folded hypothesis in [`ExactChain::bodies_after`] for direct topology
//!   readouts) and decline via `indeterminate` only when a downstream consumer
//!   depends on fold vs split; `Cut` / `Intersect` act on each target
//!   independently, and selective `FeatureOutput` references into a body
//!   split/consumed into `!= 1` components decline only when the outcome depends
//!   on `OutputKey` shell ordering. A cut's auto-reversal and a through-all
//!   depth are measured on the PRIMARY (first) target body, as the engine does.
//!   The readout is per body, summed: overlapping independent bodies count
//!   their volume twice and their components separately — the kernel's own
//!   output semantics.
//!
//! Not covered (typed [`NotCovered`]): region extrudes (`params.regions`),
//! a `BooleanCombine` whose verb is not Union / Subtract / Intersect,
//! share-a-face auto-targeting (`combine` set with no `targets`), `UpTo` /
//! through-all SECOND directions, internal gears, sketches without plane
//! data or a resolvable profile.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::f64::consts::{PI, TAU};

use serde_json::Value;

use super::topology_oracle::{TopologyReadout, VoxelGrid};

/// Why a document could not be turned into an exact chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotCovered {
    /// Feature `index` (name) uses a construct the predicate route has no
    /// closed form for.
    Feature {
        index: usize,
        name: String,
        why: String,
    },
    /// The document is not a single-part tree of features.
    Shape(String),
}

impl std::fmt::Display for NotCovered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotCovered::Feature { index, name, why } => {
                write!(f, "feature {index} ({name}): {why}")
            }
            NotCovered::Shape(s) => write!(f, "document shape: {s}"),
        }
    }
}

fn norm(v: [f64; 3]) -> Option<[f64; 3]> {
    let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if !m.is_finite() || m <= 0.0 {
        return None;
    }
    Some([v[0] / m, v[1] / m, v[2] / m])
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// The sketch-plane frame: origin, in-plane `x`/`y`, unit normal `n` — the
/// UI's `buildSketchPlane` basis, as `SketchPlaneBasis::from_origin_normal`
/// and the feature engine's `tangent_x_from_normal` both derive it.
#[derive(Debug, Clone, Copy)]
pub struct Basis {
    pub origin: [f64; 3],
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub n: [f64; 3],
}

impl Basis {
    pub fn from_origin_normal(origin: [f64; 3], normal: [f64; 3]) -> Option<Self> {
        let n = norm(normal)?;
        let reference = if dot(n, [0.0, 0.0, 1.0]).abs() < 0.99 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let x = norm(cross(reference, n))?;
        let y = norm(cross(n, x))?;
        Some(Basis { origin, x, y, n })
    }
    /// `(u, v)` of a point (its normal component dropped).
    pub fn local(&self, p: [f64; 3]) -> (f64, f64) {
        let r = sub(p, self.origin);
        (dot(r, self.x), dot(r, self.y))
    }
    pub fn embed(&self, u: f64, v: f64) -> [f64; 3] {
        add(self.origin, add(scale(self.x, u), scale(self.y, v)))
    }
}

/// The involute gear disc in closed form (see the module doc).
#[derive(Debug, Clone, Copy)]
pub struct GearDisc {
    pub cx: f64,
    pub cy: f64,
    pub rot: f64,
    pub root_r: f64,
    pub base_r: f64,
    pub add_r: f64,
    /// Half-tooth angle at the base circle, backlash applied.
    pub half: f64,
    pub pitch_ang: f64,
}

impl GearDisc {
    pub fn new(
        teeth: u32,
        module: f64,
        pressure_deg: f64,
        backlash: f64,
        cx: f64,
        cy: f64,
        rot: f64,
    ) -> Self {
        let alpha = pressure_deg.to_radians();
        let pitch_r = teeth as f64 * module / 2.0;
        let base_r = pitch_r * alpha.cos();
        let add_r = pitch_r + module;
        let ded_r = pitch_r - 1.25 * module;
        let root_r = ded_r.max(base_r * 0.5);
        let inv_alpha = alpha.tan() - alpha;
        let pitch_ang = TAU / teeth as f64;
        let backlash_angle = backlash / (2.0 * pitch_r);
        GearDisc {
            cx,
            cy,
            rot,
            root_r,
            base_r,
            add_r,
            half: pitch_ang / 4.0 + inv_alpha - backlash_angle,
            pitch_ang,
        }
    }
    /// Tooth angular half-width at radius `r` (`r ≥ root`).
    pub fn half_width_at(&self, r: f64) -> f64 {
        if r <= self.base_r {
            self.half
        } else {
            let t = ((r / self.base_r).powi(2) - 1.0).max(0.0).sqrt();
            self.half - (t - t.atan())
        }
    }
    pub fn contains(&self, u: f64, v: f64) -> bool {
        let (x, y) = (u - self.cx, v - self.cy);
        let r = x.hypot(y);
        if r <= self.root_r {
            return true;
        }
        if r > self.add_r {
            return false;
        }
        let phi = y.atan2(x) - self.rot;
        let d = (phi + self.pitch_ang / 2.0).rem_euclid(self.pitch_ang) - self.pitch_ang / 2.0;
        d.abs() <= self.half_width_at(r)
    }
}

/// A closed 2D region in sketch coordinates.
#[derive(Debug, Clone)]
pub enum Profile2 {
    /// Simple polygon, vertices in loop order (either orientation).
    Polygon(Vec<(f64, f64)>),
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
    },
    Gear(GearDisc),
    /// An outer loop with holes (KV14 holed region): inside the outer and
    /// outside every hole.
    Region {
        outer: Box<Profile2>,
        holes: Vec<Profile2>,
    },
}

impl Profile2 {
    pub fn contains(&self, u: f64, v: f64) -> bool {
        match self {
            Profile2::Polygon(pts) => point_in_polygon(pts, u, v),
            Profile2::Circle { cx, cy, r } => (u - cx).hypot(v - cy) <= *r,
            Profile2::Gear(g) => g.contains(u, v),
            Profile2::Region { outer, holes } => {
                outer.contains(u, v) && !holes.iter().any(|h| h.contains(u, v))
            }
        }
    }
    /// Unsigned area (the adapter's grouping metric: `polygon_area_abs` for
    /// a loop, `π r²` for a circle).
    pub fn area_abs(&self) -> f64 {
        match self {
            Profile2::Polygon(pts) => {
                let n = pts.len();
                let mut a = 0.0;
                for i in 0..n {
                    let (u0, v0) = pts[i];
                    let (u1, v1) = pts[(i + 1) % n];
                    a += u0 * v1 - u1 * v0;
                }
                0.5 * a.abs()
            }
            Profile2::Circle { r, .. } => std::f64::consts::PI * r * r,
            Profile2::Gear(g) => std::f64::consts::PI * g.add_r * g.add_r,
            Profile2::Region { outer, .. } => outer.area_abs(),
        }
    }
    /// `(umin, umax, vmin, vmax)`.
    pub fn bbox(&self) -> (f64, f64, f64, f64) {
        match self {
            Profile2::Region { outer, .. } => outer.bbox(),
            Profile2::Polygon(pts) => pts.iter().fold(
                (
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                ),
                |b, &(u, v)| (b.0.min(u), b.1.max(u), b.2.min(v), b.3.max(v)),
            ),
            Profile2::Circle { cx, cy, r } => (cx - r, cx + r, cy - r, cy + r),
            Profile2::Gear(g) => (
                g.cx - g.add_r,
                g.cx + g.add_r,
                g.cy - g.add_r,
                g.cy + g.add_r,
            ),
        }
    }

    /// Sound conservative over-approximation of `{ q : dist(q, self) <= margin }`.
    /// If ANY point within `margin` of `(u, v)` belongs to the profile, returns `true`.
    fn may_contain_2d(&self, u: f64, v: f64, margin: f64) -> bool {
        let m = margin.max(0.0);
        match self {
            Profile2::Polygon(pts) => {
                pts.len() >= 3
                    && (point_in_polygon(pts, u, v) || polygon_boundary_dist(pts, u, v) <= m)
            }
            Profile2::Circle { cx, cy, r } => (u - cx).hypot(v - cy) <= *r + m,
            Profile2::Gear(g) => (u - g.cx).hypot(v - g.cy) <= g.add_r + m,
            Profile2::Region { outer, holes } => {
                outer.may_contain_2d(u, v, m) && !holes.iter().any(|h| h.must_contain_2d(u, v, m))
            }
        }
    }

    /// Sound conservative under-approximation of `{ q : closed_ball(q, margin) ⊂ int(self) }`.
    /// Only returns `true` when EVERY point within `margin` of `(u, v)` lies strictly inside.
    fn must_contain_2d(&self, u: f64, v: f64, margin: f64) -> bool {
        let m = margin.max(0.0);
        match self {
            Profile2::Polygon(pts) => {
                pts.len() >= 3
                    && point_in_polygon(pts, u, v)
                    && polygon_boundary_dist(pts, u, v) > m
            }
            Profile2::Circle { cx, cy, r } => *r > m && (u - cx).hypot(v - cy) < *r - m,
            Profile2::Gear(g) => g.root_r > m && (u - g.cx).hypot(v - g.cy) < g.root_r - m,
            Profile2::Region { outer, holes } => {
                outer.must_contain_2d(u, v, m) && !holes.iter().any(|h| h.may_contain_2d(u, v, m))
            }
        }
    }
}

fn polygon_boundary_dist(pts: &[(f64, f64)], u: f64, v: f64) -> f64 {
    let n = pts.len();
    if n == 0 {
        return f64::INFINITY;
    }
    let mut best = f64::INFINITY;
    for i in 0..n {
        let (ax, ay) = pts[i];
        let (bx, by) = pts[(i + 1) % n];
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let d = if len2 <= 0.0 {
            (u - ax).hypot(v - ay)
        } else {
            let t = (((u - ax) * dx + (v - ay) * dy) / len2).clamp(0.0, 1.0);
            (u - (ax + t * dx)).hypot(v - (ay + t * dy))
        };
        best = best.min(d);
    }
    best
}

/// Even-odd point-in-polygon (crossing number); boundary points count as
/// inside on one side only, which is immaterial on a lattice.
fn point_in_polygon(pts: &[(f64, f64)], u: f64, v: f64) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (ui, vi) = pts[i];
        let (uj, vj) = pts[j];
        if (vi > v) != (vj > v) {
            let x = uj + (v - vj) * (ui - uj) / (vi - vj);
            if u < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// One operand of the chain as an exact point predicate.
#[derive(Debug, Clone)]
pub enum ExactSolid {
    /// `{ face_origin + u·x̂ + v·ŷ + t·d̂ : (u, v) ∈ profile, 0 ≤ t ≤ depth }`.
    Extrude {
        basis: Basis,
        profile: Profile2,
        face_origin: [f64; 3],
        /// Unit extrude direction (may be oblique to the plane normal).
        dir: [f64; 3],
        depth: f64,
    },
    Revolve {
        basis: Basis,
        profile: Profile2,
        axis_origin: [f64; 3],
        /// Unit axis direction (in the plane).
        axis: [f64; 3],
        /// Unit radial direction from the axis toward the profile (in the plane).
        w_hat: [f64; 3],
        /// `â × ŵ` — the sweep velocity at θ = 0.
        m_hat: [f64; 3],
        angle: f64,
        full_turn: bool,
    },
}

impl ExactSolid {
    pub fn contains(&self, p: [f64; 3]) -> bool {
        match self {
            ExactSolid::Extrude {
                basis,
                profile,
                face_origin,
                dir,
                depth,
            } => {
                let dn = dot(*dir, basis.n);
                let r = sub(p, *face_origin);
                let t = dot(r, basis.n) / dn;
                if t.is_nan() || t < 0.0 || t > *depth {
                    return false;
                }
                let q = sub(r, scale(*dir, t));
                profile.contains(dot(q, basis.x), dot(q, basis.y))
            }
            ExactSolid::Revolve {
                basis,
                profile,
                axis_origin,
                axis,
                w_hat,
                m_hat,
                angle,
                full_turn,
            } => {
                let r = sub(p, *axis_origin);
                let h = dot(r, *axis);
                let rad = sub(r, scale(*axis, h));
                let rho = dot(rad, rad).sqrt();
                if !*full_turn {
                    let theta = dot(rad, *m_hat).atan2(dot(rad, *w_hat)).rem_euclid(TAU);
                    if theta > *angle {
                        return false;
                    }
                }
                // Rotate back into the profile plane: the point at height h
                // and radius rho on the ŵ side.
                let q = add(add(*axis_origin, scale(*axis, h)), scale(*w_hat, rho));
                let (u, v) = basis.local(q);
                profile.contains(u, v)
            }
        }
    }

    /// Sound over-approximation: `true` if ANY point in `closed_ball(p, rad)`
    /// could belong to `self`.
    fn may_contain_ball(&self, p: [f64; 3], rad: f64) -> bool {
        let m3 = rad.max(0.0) * (1.0 + 1e-12);
        match self {
            ExactSolid::Extrude {
                basis,
                profile,
                face_origin,
                dir,
                depth,
            } => {
                if *depth < 0.0 {
                    return false;
                }
                let dn = dot(*dir, basis.n);
                if !dn.is_finite() || dn.abs() <= 1e-15 {
                    return true;
                }
                // For ||x - p|| <= m3, both |t(x) - t(p)| and ||q(x) - q(p)||
                // in the orthonormal sketch plane are bounded by m3 / |d · n|.
                let m = m3 / dn.abs();
                let r = sub(p, *face_origin);
                let t = dot(r, basis.n) / dn;
                if t.is_nan() || t < -m || t > *depth + m {
                    return false;
                }
                let q = sub(r, scale(*dir, t));
                profile.may_contain_2d(dot(q, basis.x), dot(q, basis.y), m)
            }
            ExactSolid::Revolve {
                basis,
                profile,
                axis_origin,
                axis,
                w_hat,
                m_hat,
                angle,
                full_turn,
            } => {
                if !*full_turn && *angle < 0.0 {
                    return false;
                }
                let r = sub(p, *axis_origin);
                let h = dot(r, *axis);
                let rad_vec = sub(r, scale(*axis, h));
                let rho = dot(rad_vec, rad_vec).sqrt();
                if !*full_turn && rho > m3 {
                    let theta = dot(rad_vec, *m_hat)
                        .atan2(dot(rad_vec, *w_hat))
                        .rem_euclid(TAU);
                    if theta > *angle {
                        let gap = (theta - *angle).min(TAU - theta);
                        let dtheta = (m3 / rho).clamp(0.0, 1.0).asin();
                        if gap > dtheta {
                            return false;
                        }
                    }
                }
                let q = add(add(*axis_origin, scale(*axis, h)), scale(*w_hat, rho));
                let (u, v) = basis.local(q);
                profile.may_contain_2d(u, v, m3)
            }
        }
    }

    /// Sound under-approximation: `true` only when EVERY point in
    /// `closed_ball(p, rad)` lies strictly inside `self`.
    fn must_contain_ball(&self, p: [f64; 3], rad: f64) -> bool {
        let m3 = rad.max(0.0) * (1.0 + 1e-12);
        match self {
            ExactSolid::Extrude {
                basis,
                profile,
                face_origin,
                dir,
                depth,
            } => {
                let dn = dot(*dir, basis.n);
                if !dn.is_finite() || dn.abs() <= 1e-15 {
                    return false;
                }
                let m = m3 / dn.abs();
                if *depth <= 2.0 * m {
                    return false;
                }
                let r = sub(p, *face_origin);
                let t = dot(r, basis.n) / dn;
                if t.is_nan() || t <= m || t >= *depth - m {
                    return false;
                }
                let q = sub(r, scale(*dir, t));
                profile.must_contain_2d(dot(q, basis.x), dot(q, basis.y), m)
            }
            ExactSolid::Revolve {
                basis,
                profile,
                axis_origin,
                axis,
                w_hat,
                m_hat,
                angle,
                full_turn,
            } => {
                let r = sub(p, *axis_origin);
                let h = dot(r, *axis);
                let rad_vec = sub(r, scale(*axis, h));
                let rho = dot(rad_vec, rad_vec).sqrt();
                if !*full_turn {
                    if rho <= m3 {
                        return false;
                    }
                    let dtheta = (m3 / rho).clamp(0.0, 1.0).asin();
                    if *angle <= 2.0 * dtheta {
                        return false;
                    }
                    let theta = dot(rad_vec, *m_hat)
                        .atan2(dot(rad_vec, *w_hat))
                        .rem_euclid(TAU);
                    if theta <= dtheta || theta >= *angle - dtheta {
                        return false;
                    }
                }
                let q = add(add(*axis_origin, scale(*axis, h)), scale(*w_hat, rho));
                let (u, v) = basis.local(q);
                profile.must_contain_2d(u, v, m3)
            }
        }
    }

    /// Conservative world-space bounding box `(min, max)`.
    pub fn bbox(&self) -> ([f64; 3], [f64; 3]) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        let mut take = |p: [f64; 3]| {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        };
        match self {
            ExactSolid::Extrude {
                basis,
                profile,
                face_origin,
                dir,
                depth,
            } => {
                let (u0, u1, v0, v1) = profile.bbox();
                for &(u, v) in &[(u0, v0), (u0, v1), (u1, v0), (u1, v1)] {
                    let base = add(*face_origin, add(scale(basis.x, u), scale(basis.y, v)));
                    take(base);
                    take(add(base, scale(*dir, *depth)));
                }
            }
            ExactSolid::Revolve {
                basis,
                profile,
                axis_origin,
                axis,
                ..
            } => {
                // Axial range and max radius over the profile's bbox corners.
                let (u0, u1, v0, v1) = profile.bbox();
                let mut hmin = f64::INFINITY;
                let mut hmax = f64::NEG_INFINITY;
                let mut rmax: f64 = 0.0;
                for &(u, v) in &[(u0, v0), (u0, v1), (u1, v0), (u1, v1)] {
                    let p = basis.embed(u, v);
                    let r = sub(p, *axis_origin);
                    let h = dot(r, *axis);
                    let rad = sub(r, scale(*axis, h));
                    hmin = hmin.min(h);
                    hmax = hmax.max(h);
                    rmax = rmax.max(dot(rad, rad).sqrt());
                }
                // The full-turn cylinder of radius rmax over [hmin, hmax].
                for &h in &[hmin, hmax] {
                    let c = add(*axis_origin, scale(*axis, h));
                    for k in 0..3 {
                        // Extent of a circle of radius rmax normal to `axis`
                        // along world axis k: rmax · √(1 − axis_k²).
                        let e = rmax * (1.0 - axis[k] * axis[k]).max(0.0).sqrt();
                        let mut a = c;
                        let mut b = c;
                        a[k] -= e;
                        b[k] += e;
                        take(a);
                        take(b);
                    }
                }
            }
        }
        (lo, hi)
    }
}

/// The verb of an explicit `BooleanCombine` feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeKind {
    Union,
    Subtract,
    Intersect,
}

/// A feature output slot (`Anchor::FeatureOutput`): `index == 0` corresponds
/// to `OutputKey::Main`, `index >= 1` to `OutputKey::Body { index }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputAnchor {
    pub feature_id: String,
    pub index: usize,
}

/// An explicit combine of live bodies (`BooleanCombine`): the bodies
/// anchored by `body_a` / `body_b` fold into ONE body whose expression is
/// theirs composed by the verb — a set UNION (P0007, 2026-09-29), or, since
/// 2026-10-13 (prospector seed 5 index 39, an Intersect whose exact result
/// is a sliver the magnitude oracle alone could not judge), `body_a −
/// body_b` / `body_a ∩ body_b`. It adds no operand — the lattice reads the
/// same operands — but it changes the BODY LIST (the per-body readout no
/// longer counts an overlap twice, later targets can anchor to the combine
/// feature, and a Subtract / Intersect re-authors the surviving set).
#[derive(Debug, Clone)]
pub struct BodyMerge {
    /// Applies once the first `after_op` operands have been replayed.
    pub after_op: usize,
    pub feature_id: String,
    pub name: String,
    /// `FeatureOutput` anchors (`body_a` first — the order matters for
    /// Subtract and Intersect).
    pub anchors: Vec<OutputAnchor>,
    pub kind: MergeKind,
}

/// A parsed document: operands in feature order with their combine sense.
#[derive(Debug, Clone)]
pub struct ExactChain {
    pub ops: Vec<ExactOp>,
    /// Union combines, in feature order (see [`BodyMerge`]).
    pub merges: Vec<BodyMerge>,
    /// Notes on judgment calls made while parsing (cut auto-reversal etc.).
    pub notes: Vec<String>,
    /// Decisions the DOCUMENT does not determine — a cut auto-reversal on
    /// a floating-point-noise margin (the sketch plane at the body's
    /// mid-extent), where the engine's choice is arithmetic luck. The
    /// verdict declines such a chain rather than guess.
    pub indeterminate: Vec<String>,
    /// The first sketch's frame — the lattice is laid in it (see
    /// [`readout_exact`]): a lattice OBLIQUE to the model's planes
    /// perforates frame-aligned thin features at every cell size (measured
    /// on R0053: the world-frame lattice flickers χ between −8 and +3 down
    /// to h ≈ 0.37 while the sketch-frame lattice reads 0 from h = 2 to
    /// 0.3 on two phases).
    pub frame: Basis,
}

/// The feature engine's combine verb (`feature_engine::types::CombineMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combine {
    NewBody,
    Add,
    Cut,
    Intersect,
}

/// How a feature's target bodies are chosen (`TargetStrategy`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Targets {
    /// Legacy: every live body of the most recent solid-bearing feature.
    MostRecent,
    /// Explicitly named feature outputs (`Anchor::FeatureOutput`).
    Explicit(Vec<OutputAnchor>),
}

/// Alternate cutter geometry when a direction-less `Extrude` cut's candidate
/// target components disagree on auto-reversal (`reverse = primary_rev` vs
/// `!primary_rev`). Evaluated in [`ExactChain::worlds_after`] so the chain
/// declines only when the two reversals yield different volumes.
#[derive(Debug, Clone)]
pub struct AltCut {
    pub solid: ExactSolid,
    pub primary_rev: bool,
    pub dir_unit: [f64; 3],
    pub sketch_proj: f64,
    pub why: String,
}

#[derive(Debug, Clone)]
pub struct ExactOp {
    pub name: String,
    /// The feature's id (what an explicit target anchors to).
    pub feature_id: String,
    pub solid: ExactSolid,
    /// Second sweep solid (`!primary_rev`) when multi-component cut
    /// auto-reversal disagrees across candidate `Main` components.
    pub alt_cut: Option<AltCut>,
    /// `combine == Cut` — kept for the operand-level instruments.
    pub cut: bool,
    pub combine: Combine,
    pub targets: Targets,
}

#[derive(Debug, Clone)]
struct VoronoiNode {
    lo: [f64; 3],
    hi: [f64; 3],
    pt: [f64; 3],
    lump: u16,
    /// `u16::MAX` when this subtree holds surface voxels from `>= 2` lumps;
    /// otherwise the single lump id shared by every node in the subtree.
    subtree_lump: u16,
    left: u32,
    right: u32,
}

/// Partition of a multi-component boolean expression into discrete connected
/// lumps on the [`EXACT_VOLUME_CELLS`] frame lattice, paired with an exact
/// Euclidean Voronoi classifier over occupied voxel centres so that
/// `{ p ∈ base : lump_of(p) == c }` for `c = 0..num_lumps` partitions `base`
/// disjointly and exhaustively across all of `ℝ³`.
#[derive(Debug)]
pub struct LumpPartition {
    frame: Basis,
    start: [f64; 3],
    h: f64,
    n: [usize; 3],
    labels: Vec<u16>,
    num_lumps: usize,
    nodes: Vec<VoronoiNode>,
    root: u32,
    lump_wbox: Vec<Box3>,
}

/// Pointer-identified handle to a [`LumpPartition`] so [`Expr`] stays `Clone + Eq`.
#[derive(Debug, Clone)]
pub struct LumpPartitionHandle(std::sync::Arc<LumpPartition>);

impl PartialEq for LumpPartitionHandle {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for LumpPartitionHandle {}

#[derive(Clone)]
enum LumpPartitionOutcome {
    Empty,
    Single,
    Multi(LumpPartitionHandle),
}

/// A body's membership as a boolean expression over operand indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Leaf(usize),
    AltLeaf(usize),
    Union(Vec<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Inter(Box<Expr>, Box<Expr>),
    Lump {
        base: Box<Expr>,
        partition: LumpPartitionHandle,
        lump: u16,
    },
}

/// One live body after a prefix of the chain.
#[derive(Debug, Clone)]
pub struct Body {
    /// Every feature id this body is the output of (creator, then each
    /// modifier), in order — the LAST is the feature whose output it is now.
    pub ids: Vec<String>,
    /// Exact `(feature_id, output_index)` slots this body occupies.
    pub outputs: Vec<OutputAnchor>,
    pub expr: Expr,
    /// Sequence number of the feature that last produced or modified it.
    pub last_touched: usize,
    /// Set when a disjoint `BooleanCombine(Union)` emitted multiple bodies
    /// whose `Main` vs `Body { index }` shell order is arbitrary.
    pub arbitrary_output_order: bool,
}

fn dist_sq3(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    dx * dx + dy * dy + dz * dz
}

fn dist_sq_aabb3(q: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> f64 {
    let mut s = 0.0;
    for a in 0..3 {
        let d = if q[a] < lo[a] {
            lo[a] - q[a]
        } else if q[a] > hi[a] {
            q[a] - hi[a]
        } else {
            0.0
        };
        s += d * d;
    }
    s
}

fn build_voronoi_kd(pts: &mut [([f64; 3], u16)], out: &mut Vec<VoronoiNode>) -> u32 {
    if pts.is_empty() {
        return u32::MAX;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let first_lump = pts[0].1;
    let mut same_lump = true;
    for &(p, l) in pts.iter() {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
        if l != first_lump {
            same_lump = false;
        }
    }
    let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let ax = if ext[0] >= ext[1] && ext[0] >= ext[2] {
        0
    } else if ext[1] >= ext[2] {
        1
    } else {
        2
    };
    let mid = pts.len() / 2;
    pts.select_nth_unstable_by(mid, |a, b| {
        a.0[ax]
            .total_cmp(&b.0[ax])
            .then_with(|| a.0[(ax + 1) % 3].total_cmp(&b.0[(ax + 1) % 3]))
            .then_with(|| a.0[(ax + 2) % 3].total_cmp(&b.0[(ax + 2) % 3]))
            .then_with(|| a.1.cmp(&b.1))
    });
    let (pt, lump) = pts[mid];
    let idx = out.len() as u32;
    out.push(VoronoiNode {
        lo,
        hi,
        pt,
        lump,
        subtree_lump: if same_lump { first_lump } else { u16::MAX },
        left: u32::MAX,
        right: u32::MAX,
    });
    let left = if mid > 0 {
        build_voronoi_kd(&mut pts[..mid], out)
    } else {
        u32::MAX
    };
    let right = if mid + 1 < pts.len() {
        build_voronoi_kd(&mut pts[mid + 1..], out)
    } else {
        u32::MAX
    };
    out[idx as usize].left = left;
    out[idx as usize].right = right;
    idx
}

impl LumpPartition {
    fn to_frame(&self, p: [f64; 3]) -> [f64; 3] {
        let r = sub(p, self.frame.origin);
        [
            dot(r, self.frame.x),
            dot(r, self.frame.y),
            dot(r, self.frame.n),
        ]
    }

    fn cell_coords(&self, q: [f64; 3]) -> Option<(usize, usize, usize)> {
        let fi = ((q[0] - self.start[0]) / self.h).floor();
        let fj = ((q[1] - self.start[1]) / self.h).floor();
        let fk = ((q[2] - self.start[2]) / self.h).floor();
        if fi < 0.0 || fj < 0.0 || fk < 0.0 {
            return None;
        }
        let (i, j, k) = (fi as usize, fj as usize, fk as usize);
        if i < self.n[0] && j < self.n[1] && k < self.n[2] {
            Some((i, j, k))
        } else {
            None
        }
    }

    fn cell_center_q(&self, i: usize, j: usize, k: usize) -> [f64; 3] {
        [
            self.start[0] + (i as f64 + 0.5) * self.h,
            self.start[1] + (j as f64 + 0.5) * self.h,
            self.start[2] + (k as f64 + 0.5) * self.h,
        ]
    }

    /// Exact Voronoi lump classification of world point `p`:
    /// - If `p`'s own grid cell `(i, j, k)` is occupied with label `c`, then
    ///   `|q_a - center(i,j,k)_a| <= 0.5·h <= |q_a - center(i',j',k')_a|` on
    ///   every axis, so `(i, j, k)` is unconditionally a globally nearest
    ///   occupied voxel centre (`O(1)` lookup).
    /// - Otherwise (`(i, j, k)` unoccupied), stepping any 6-interior voxel of
    ///   a lump `c` toward `(i, j, k)` decreases (or ties en route to the
    ///   boundary) Euclidean distance, so the closest occupied voxel of every
    ///   lump lies in its 6-surface set indexed by `nodes`.
    fn lump_of(&self, p: [f64; 3]) -> u16 {
        let q = self.to_frame(p);
        if let Some((i, j, k)) = self.cell_coords(q) {
            let lbl = self.labels[(k * self.n[1] + j) * self.n[0] + i];
            if lbl != u16::MAX {
                return lbl;
            }
        }
        let mut best_d2 = f64::INFINITY;
        let mut best_lump = 0u16;
        self.nearest_kd(self.root, q, &mut best_d2, &mut best_lump);
        best_lump
    }

    fn nearest_kd(&self, idx: u32, q: [f64; 3], best_d2: &mut f64, best_lump: &mut u16) {
        if idx == u32::MAX {
            return;
        }
        let node = &self.nodes[idx as usize];
        if dist_sq_aabb3(q, node.lo, node.hi) > *best_d2 {
            return;
        }
        let d2 = dist_sq3(q, node.pt);
        if d2 < *best_d2 || (d2 == *best_d2 && node.lump < *best_lump) {
            *best_d2 = d2;
            *best_lump = node.lump;
        }
        let dl = if node.left != u32::MAX {
            let l = &self.nodes[node.left as usize];
            dist_sq_aabb3(q, l.lo, l.hi)
        } else {
            f64::INFINITY
        };
        let dr = if node.right != u32::MAX {
            let r = &self.nodes[node.right as usize];
            dist_sq_aabb3(q, r.lo, r.hi)
        } else {
            f64::INFINITY
        };
        if dl <= dr {
            if dl <= *best_d2 {
                self.nearest_kd(node.left, q, best_d2, best_lump);
            }
            if dr <= *best_d2 {
                self.nearest_kd(node.right, q, best_d2, best_lump);
            }
        } else {
            if dr <= *best_d2 {
                self.nearest_kd(node.right, q, best_d2, best_lump);
            }
            if dl <= *best_d2 {
                self.nearest_kd(node.left, q, best_d2, best_lump);
            }
        }
    }

    fn min_dist_sq_lump_filter(
        &self,
        idx: u32,
        q: [f64; 3],
        target: u16,
        want_eq: bool,
        best_d2: &mut f64,
    ) {
        if idx == u32::MAX {
            return;
        }
        let node = &self.nodes[idx as usize];
        if want_eq {
            if node.subtree_lump != u16::MAX && node.subtree_lump != target {
                return;
            }
        } else if node.subtree_lump == target {
            return;
        }
        if dist_sq_aabb3(q, node.lo, node.hi) >= *best_d2 {
            return;
        }
        if (node.lump == target) == want_eq {
            let d2 = dist_sq3(q, node.pt);
            if d2 < *best_d2 {
                *best_d2 = d2;
            }
        }
        let dl = if node.left != u32::MAX {
            let l = &self.nodes[node.left as usize];
            dist_sq_aabb3(q, l.lo, l.hi)
        } else {
            f64::INFINITY
        };
        let dr = if node.right != u32::MAX {
            let r = &self.nodes[node.right as usize];
            dist_sq_aabb3(q, r.lo, r.hi)
        } else {
            f64::INFINITY
        };
        if dl <= dr {
            if dl < *best_d2 {
                self.min_dist_sq_lump_filter(node.left, q, target, want_eq, best_d2);
            }
            if dr < *best_d2 {
                self.min_dist_sq_lump_filter(node.right, q, target, want_eq, best_d2);
            }
        } else {
            if dr < *best_d2 {
                self.min_dist_sq_lump_filter(node.right, q, target, want_eq, best_d2);
            }
            if dl < *best_d2 {
                self.min_dist_sq_lump_filter(node.left, q, target, want_eq, best_d2);
            }
        }
    }

    fn corners_frame_ball(&self, corners: &[[f64; 3]; 8]) -> ([f64; 3], f64) {
        let mut qlo = [f64::INFINITY; 3];
        let mut qhi = [f64::NEG_INFINITY; 3];
        for &c in corners {
            let q = self.to_frame(c);
            for a in 0..3 {
                qlo[a] = qlo[a].min(q[a]);
                qhi[a] = qhi[a].max(q[a]);
            }
        }
        let q_mid = [
            0.5 * (qlo[0] + qhi[0]),
            0.5 * (qlo[1] + qhi[1]),
            0.5 * (qlo[2] + qhi[2]),
        ];
        let (dx, dy, dz) = (qhi[0] - qlo[0], qhi[1] - qlo[1], qhi[2] - qlo[2]);
        let r_box = 0.5 * (dx * dx + dy * dy + dz * dz).sqrt();
        (q_mid, r_box)
    }

    /// True when the triangle inequality proves `lump_of(p) != lump` for every
    /// point `p` inside the convex hull of `corners`.
    fn box_must_miss_voronoi(&self, lump: u16, corners: &[[f64; 3]; 8]) -> bool {
        let (q_mid, r_box) = self.corners_frame_ball(corners);
        let mut d_other_opt = None;
        if let Some((i, j, k)) = self.cell_coords(q_mid) {
            let lbl = self.labels[(k * self.n[1] + j) * self.n[0] + i];
            if lbl == lump {
                return false;
            }
            if lbl != u16::MAX {
                d_other_opt = Some(dist_sq3(q_mid, self.cell_center_q(i, j, k)).sqrt());
            }
        }
        let mut own_d2 = f64::INFINITY;
        self.min_dist_sq_lump_filter(self.root, q_mid, lump, true, &mut own_d2);
        let d_own = own_d2.sqrt();
        let d_other = d_other_opt.unwrap_or_else(|| {
            let mut oth_d2 = f64::INFINITY;
            self.min_dist_sq_lump_filter(self.root, q_mid, lump, false, &mut oth_d2);
            oth_d2.sqrt()
        });
        if !d_other.is_finite() {
            return false;
        }
        let eps = 1e-12 * (d_own + d_other + r_box).max(1.0);
        d_own - d_other > 2.0 * r_box + eps
    }

    /// True when the triangle inequality proves `lump_of(p) == lump` for every
    /// point `p` inside the convex hull of `corners`.
    fn box_must_cover_voronoi(&self, lump: u16, corners: &[[f64; 3]; 8]) -> bool {
        let (q_mid, r_box) = self.corners_frame_ball(corners);
        let mut d_own_opt = None;
        if let Some((i, j, k)) = self.cell_coords(q_mid) {
            let lbl = self.labels[(k * self.n[1] + j) * self.n[0] + i];
            if lbl != u16::MAX && lbl != lump {
                return false;
            }
            if lbl == lump {
                d_own_opt = Some(dist_sq3(q_mid, self.cell_center_q(i, j, k)).sqrt());
            }
        }
        let d_own = d_own_opt.unwrap_or_else(|| {
            let mut own_d2 = f64::INFINITY;
            self.min_dist_sq_lump_filter(self.root, q_mid, lump, true, &mut own_d2);
            own_d2.sqrt()
        });
        if !d_own.is_finite() {
            return false;
        }
        let mut oth_d2 = f64::INFINITY;
        self.min_dist_sq_lump_filter(self.root, q_mid, lump, false, &mut oth_d2);
        let d_other = oth_d2.sqrt();
        let eps = 1e-12 * (d_own + d_other + r_box).max(1.0);
        d_other - d_own > 2.0 * r_box + eps
    }
}

impl ExactChain {
    /// Membership of the SET union of the live bodies after the whole chain
    /// (the reading of a single-body document; overlapping independent
    /// bodies are unioned here and counted separately by [`readout_exact`]).
    pub fn contains(&self, p: [f64; 3]) -> bool {
        self.contains_prefix(self.ops.len(), p)
    }

    /// Set-union membership of the bodies after the first `k` operations.
    pub fn contains_prefix(&self, k: usize, p: [f64; 3]) -> bool {
        self.bodies_after(k)
            .iter()
            .any(|b| self.expr_contains(&b.expr, p))
    }

    fn leaf_solid(&self, i: usize, alt: bool) -> &ExactSolid {
        if alt {
            self.ops[i]
                .alt_cut
                .as_ref()
                .map_or(&self.ops[i].solid, |a| &a.solid)
        } else {
            &self.ops[i].solid
        }
    }

    /// Membership of one body's expression.
    pub fn expr_contains(&self, e: &Expr, p: [f64; 3]) -> bool {
        match e {
            Expr::Leaf(i) => self.ops[*i].solid.contains(p),
            Expr::AltLeaf(i) => self.leaf_solid(*i, true).contains(p),
            Expr::Union(es) => es.iter().any(|e| self.expr_contains(e, p)),
            Expr::Sub(a, b) => self.expr_contains(a, p) && !self.expr_contains(b, p),
            Expr::Inter(a, b) => self.expr_contains(a, p) && self.expr_contains(b, p),
            Expr::Lump {
                base,
                partition,
                lump,
            } => self.expr_contains(base, p) && partition.0.lump_of(p) == *lump,
        }
    }

    /// Sound over-approximation: `true` if ANY point of `closed_ball(p, rad)`
    /// could belong to the body `e` (public for `assay::monotonicity`'s
    /// per-step volume interval).
    pub fn expr_may_contain_ball(&self, e: &Expr, p: [f64; 3], rad: f64) -> bool {
        match e {
            Expr::Leaf(i) => self.ops[*i].solid.may_contain_ball(p, rad),
            Expr::AltLeaf(i) => self.leaf_solid(*i, true).may_contain_ball(p, rad),
            // A lump is a subset of its base body, so the base's "may" bound
            // is a sound over-approximation of the lump's.
            Expr::Lump { base, .. } => self.expr_may_contain_ball(base, p, rad),
            Expr::Union(es) => es.iter().any(|e| self.expr_may_contain_ball(e, p, rad)),
            Expr::Sub(a, b) => {
                self.expr_may_contain_ball(a, p, rad) && !self.expr_must_contain_ball(b, p, rad)
            }
            Expr::Inter(a, b) => {
                self.expr_may_contain_ball(a, p, rad) && self.expr_may_contain_ball(b, p, rad)
            }
        }
    }

    /// Sound under-approximation: `true` only when EVERY point of
    /// `closed_ball(p, rad)` lies inside the body `e`.
    pub fn expr_must_contain_ball(&self, e: &Expr, p: [f64; 3], rad: f64) -> bool {
        match e {
            Expr::Leaf(i) => self.ops[*i].solid.must_contain_ball(p, rad),
            Expr::AltLeaf(i) => self.leaf_solid(*i, true).must_contain_ball(p, rad),
            // The lump partition is a lattice flood-fill with no ball bound, so
            // "must" cannot be certified: `false` is the sound under-approximation.
            Expr::Lump { .. } => false,
            Expr::Union(es) => es.iter().any(|e| self.expr_must_contain_ball(e, p, rad)),
            Expr::Sub(a, b) => {
                self.expr_must_contain_ball(a, p, rad) && !self.expr_may_contain_ball(b, p, rad)
            }
            Expr::Inter(a, b) => {
                self.expr_must_contain_ball(a, p, rad) && self.expr_must_contain_ball(b, p, rad)
            }
        }
    }

    /// Sound emptiness certificate for the live bodies after the first `prefix`
    /// operations: returns `true` ONLY when the conservative bounding box has
    /// zero 3D volume (including `None`, e.g. disjoint `Intersect` boxes or
    /// segment/face-only contact like P0023) OR when every cell ball covering
    /// the conservative bounding box at `cells` resolution is certified
    /// disjoint from every live body. A sub-lattice thin wall (e.g. C0034 /
    /// C0117, 1e-4 m wall at h ≈ 4e-3 m) fails the ball check on its wall
    /// cells and returns `false`.
    pub fn certified_empty_at(&self, prefix: usize, cells: usize) -> bool {
        let bodies = self.bodies_after(prefix);
        if bodies.is_empty() {
            return false;
        }
        let Some((wlo, whi)) = self.bodies_bbox(&bodies) else {
            return true;
        };
        if (whi[0] - wlo[0]) <= 0.0 || (whi[1] - wlo[1]) <= 0.0 || (whi[2] - wlo[2]) <= 0.0 {
            return true;
        }
        let f = &self.frame;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for c in 0..8 {
            let p = [
                if c & 1 == 0 { wlo[0] } else { whi[0] },
                if c & 2 == 0 { wlo[1] } else { whi[1] },
                if c & 4 == 0 { wlo[2] } else { whi[2] },
            ];
            let r = sub(p, f.origin);
            let q = [dot(r, f.x), dot(r, f.y), dot(r, f.n)];
            for k in 0..3 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
        let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        if ext[0] <= 0.0 || ext[1] <= 0.0 || ext[2] <= 0.0 {
            return true;
        }
        let longest = ext[0].max(ext[1]).max(ext[2]);
        if !longest.is_finite() || cells == 0 {
            return false;
        }
        let h = longest / cells as f64;
        let n = [
            ((ext[0] / h).ceil() as usize + 2).max(1),
            ((ext[1] / h).ceil() as usize + 2).max(1),
            ((ext[2] / h).ceil() as usize + 2).max(1),
        ];
        let start = [lo[0] - h, lo[1] - h, lo[2] - h];
        let cell_rad = 0.5 * 3.0_f64.sqrt() * h;
        for k in 0..n[2] {
            for j in 0..n[1] {
                for i in 0..n[0] {
                    let (u, v, w) = (
                        start[0] + (i as f64 + 0.5) * h,
                        start[1] + (j as f64 + 0.5) * h,
                        start[2] + (k as f64 + 0.5) * h,
                    );
                    let p = add(
                        f.origin,
                        add(add(scale(f.x, u), scale(f.y, v)), scale(f.n, w)),
                    );
                    if bodies
                        .iter()
                        .any(|b| self.expr_may_contain_ball(&b.expr, p, cell_rad))
                    {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Bounding box of one body's expression (a subtraction keeps the
    /// minuend's box; an intersection takes the boxes' overlap, `None` when
    /// they are disjoint).
    pub fn expr_bbox(&self, e: &Expr) -> Option<([f64; 3], [f64; 3])> {
        match e {
            Expr::Leaf(i) => Some(self.ops[*i].solid.bbox()),
            Expr::AltLeaf(i) => Some(self.leaf_solid(*i, true).bbox()),
            Expr::Union(es) => {
                let mut lo = [f64::INFINITY; 3];
                let mut hi = [f64::NEG_INFINITY; 3];
                let mut any = false;
                for (a, b) in es.iter().filter_map(|e| self.expr_bbox(e)) {
                    for k in 0..3 {
                        lo[k] = lo[k].min(a[k]);
                        hi[k] = hi[k].max(b[k]);
                    }
                    any = true;
                }
                any.then_some((lo, hi))
            }
            Expr::Sub(a, _) => self.expr_bbox(a),
            Expr::Inter(a, b) => {
                let (alo, ahi) = self.expr_bbox(a)?;
                let (blo, bhi) = self.expr_bbox(b)?;
                let mut lo = [0.0; 3];
                let mut hi = [0.0; 3];
                for k in 0..3 {
                    lo[k] = alo[k].max(blo[k]);
                    hi[k] = ahi[k].min(bhi[k]);
                    if lo[k] > hi[k] {
                        return None;
                    }
                }
                Some((lo, hi))
            }
            Expr::Lump {
                partition, lump, ..
            } => Some(partition.0.lump_wbox[*lump as usize]),
        }
    }

    /// The live bodies after the first `k` operations, replaying the
    /// engine's combine dispatch (see the module docs). When contact between
    /// operands is below resolution, returns the folded hypothesis (`worlds[0]`)
    /// so single-grid topology readouts (`r0081_reads_genus_three`) stay intact.
    pub fn bodies_after(&self, k: usize) -> Vec<Body> {
        self.worlds_after(k)
            .worlds
            .into_iter()
            .next()
            .map(|w| w.bodies)
            .unwrap_or_default()
    }

    fn worlds_after(&self, k: usize) -> WorldsResult {
        const MAX_WORLDS: usize = 32;
        let mut worlds = vec![CustodyWorld {
            bodies: Vec::new(),
            assumed_disjoint: Vec::new(),
            indeterminate: None,
            ambiguity_why: None,
            had_ambiguous: false,
            consumer_after_ambiguous: false,
        }];
        let mut cap_exceeded: Option<String> = None;
        let mut touch_seq: usize = 0;
        let mut lump_cache: Vec<(Expr, LumpPartitionOutcome)> = Vec::new();

        let carry_siblings = |bodies: &mut Vec<Body>,
                              named: &[OutputAnchor],
                              fid_out: &str,
                              seq: usize,
                              next_slot: &mut usize,
                              first_out_pos: Option<usize>| {
            let mut seen_fids: Vec<&str> = Vec::new();
            let mut sibling_indices: Vec<usize> = Vec::new();
            for a in named {
                if seen_fids.contains(&a.feature_id.as_str()) {
                    continue;
                }
                seen_fids.push(&a.feature_id);
                let mut slots: Vec<(usize, usize)> = Vec::new();
                for (bi, b) in bodies.iter().enumerate() {
                    if b.outputs.iter().any(|o| o.feature_id == fid_out) {
                        continue;
                    }
                    for o in &b.outputs {
                        if o.feature_id == a.feature_id && !named.contains(o) {
                            slots.push((o.index, bi));
                        }
                    }
                }
                slots.sort_unstable_by_key(|&(slot, _)| slot);
                for (_, bi) in slots {
                    if !sibling_indices.contains(&bi) {
                        sibling_indices.push(bi);
                    }
                }
            }
            for &bi in &sibling_indices {
                bodies[bi].ids.push(fid_out.to_string());
                bodies[bi].outputs.push(OutputAnchor {
                    feature_id: fid_out.to_string(),
                    index: *next_slot,
                });
                *next_slot += 1;
                bodies[bi].last_touched = seq;
            }
            if let Some(main_pos) = first_out_pos {
                if sibling_indices.iter().any(|&bi| bi < main_pos) {
                    let mut sorted_desc = sibling_indices;
                    sorted_desc.sort_unstable_by(|a, b| b.cmp(a));
                    let mut moved: Vec<(usize, Body)> = Vec::with_capacity(sorted_desc.len());
                    for bi in sorted_desc {
                        let slot = bodies[bi]
                            .outputs
                            .iter()
                            .rev()
                            .find(|o| o.feature_id == fid_out)
                            .map(|o| o.index)
                            .unwrap_or(usize::MAX);
                        moved.push((slot, bodies.remove(bi)));
                    }
                    moved.sort_unstable_by_key(|(slot, _)| *slot);
                    for (_, b) in moved {
                        bodies.push(b);
                    }
                }
            }
        };

        let mark_arb_if_multi = |bodies: &mut Vec<Body>, fid: &str| {
            let count = bodies
                .iter()
                .filter(|b| b.outputs.iter().any(|o| o.feature_id == fid))
                .count();
            for b in bodies.iter_mut() {
                if b.outputs.iter().any(|o| o.feature_id == fid) {
                    b.arbitrary_output_order = count > 1;
                }
            }
        };

        let apply_merges =
            |worlds: &mut Vec<CustodyWorld>,
             after_op: usize,
             seq: &mut usize,
             cap_exc: &mut Option<String>,
             lump_cache: &mut Vec<(Expr, LumpPartitionOutcome)>| {
                for m in self.merges.iter().filter(|m| m.after_op == after_op) {
                    *seq += 1;
                    let cur_seq = *seq;
                    let mut staged_batch: Vec<(CustodyWorld, bool)> =
                        Vec::with_capacity(worlds.len());
                    let total_in = worlds.len();
                    for (wi_in, mut w) in worlds.drain(..).enumerate() {
                        if w.had_ambiguous {
                            w.consumer_after_ambiguous = true;
                        }
                        let rem = total_in - 1 - wi_in;
                        self.stage_explicit_merge_anchors(
                            w,
                            m,
                            rem,
                            MAX_WORLDS,
                            cap_exc,
                            lump_cache,
                            &mut staged_batch,
                        );
                    }
                    let cur_world_count = staged_batch.len();
                    let mut next_worlds = Vec::with_capacity(cur_world_count);
                    for (mut w, propagate_arb) in staged_batch {
                        let find = |bodies: &[Body], anchor: &OutputAnchor| {
                            bodies
                                .iter()
                                .position(|b| b.outputs.iter().any(|o| o == anchor))
                        };
                        let (ja_opt, jb_opt) = (
                            find(&w.bodies, &m.anchors[0]),
                            find(&w.bodies, &m.anchors[1]),
                        );
                        match (ja_opt, jb_opt) {
                            (Some(ja), Some(jb)) if ja != jb => {
                                let (first, second) = (ja.max(jb), ja.min(jb));
                                let hi = w.bodies.remove(first);
                                let lo = w.bodies.remove(second);
                                let (a, b) = if ja > jb { (hi, lo) } else { (lo, hi) };
                                let main_pos = w.bodies.len();
                                if m.kind != MergeKind::Union {
                                    let expr = match m.kind {
                                        MergeKind::Subtract => {
                                            Expr::Sub(Box::new(a.expr), Box::new(b.expr))
                                        }
                                        MergeKind::Intersect => {
                                            Expr::Inter(Box::new(a.expr), Box::new(b.expr))
                                        }
                                        MergeKind::Union => unreachable!(),
                                    };
                                    let keep_primary = !w.consumer_after_ambiguous
                                        || self.expr_has_occupied_sample(&expr, EXACT_VOLUME_CELLS);
                                    let mut next_slot = 0usize;
                                    if keep_primary {
                                        let mut ids = a.ids;
                                        ids.extend(b.ids);
                                        ids.push(m.feature_id.clone());
                                        let mut outputs = a.outputs;
                                        outputs.extend(b.outputs);
                                        outputs.push(OutputAnchor {
                                            feature_id: m.feature_id.clone(),
                                            index: 0,
                                        });
                                        w.bodies.push(Body {
                                            ids,
                                            outputs,
                                            expr,
                                            last_touched: cur_seq,
                                            arbitrary_output_order: false,
                                        });
                                        next_slot = 1;
                                    }
                                    carry_siblings(
                                        &mut w.bodies,
                                        &m.anchors,
                                        &m.feature_id,
                                        cur_seq,
                                        &mut next_slot,
                                        Some(main_pos),
                                    );
                                    if propagate_arb {
                                        mark_arb_if_multi(&mut w.bodies, &m.feature_id);
                                    }
                                    next_worlds.push(w);
                                } else {
                                    match self.exprs_touch(&a.expr, &b.expr) {
                                        TouchVerdict::Touches => {
                                            let mut ids = a.ids;
                                            ids.extend(b.ids);
                                            ids.push(m.feature_id.clone());
                                            let mut outputs = a.outputs;
                                            outputs.extend(b.outputs);
                                            outputs.push(OutputAnchor {
                                                feature_id: m.feature_id.clone(),
                                                index: 0,
                                            });
                                            w.bodies.push(Body {
                                                ids,
                                                outputs,
                                                expr: Expr::Union(vec![a.expr, b.expr]),
                                                last_touched: cur_seq,
                                                arbitrary_output_order: false,
                                            });
                                            let mut next_slot = 1usize;
                                            carry_siblings(
                                                &mut w.bodies,
                                                &m.anchors,
                                                &m.feature_id,
                                                cur_seq,
                                                &mut next_slot,
                                                Some(main_pos),
                                            );
                                            if propagate_arb {
                                                mark_arb_if_multi(&mut w.bodies, &m.feature_id);
                                            }
                                            next_worlds.push(w);
                                        }
                                        TouchVerdict::Disjoint => {
                                            for (slot, mut part) in [a, b].into_iter().enumerate() {
                                                part.ids.push(m.feature_id.clone());
                                                part.outputs.push(OutputAnchor {
                                                    feature_id: m.feature_id.clone(),
                                                    index: slot,
                                                });
                                                part.last_touched = cur_seq;
                                                part.arbitrary_output_order = true;
                                                w.bodies.push(part);
                                            }
                                            let mut next_slot = 2usize;
                                            carry_siblings(
                                                &mut w.bodies,
                                                &m.anchors,
                                                &m.feature_id,
                                                cur_seq,
                                                &mut next_slot,
                                                Some(main_pos),
                                            );
                                            if propagate_arb {
                                                mark_arb_if_multi(&mut w.bodies, &m.feature_id);
                                            }
                                            next_worlds.push(w);
                                        }
                                        TouchVerdict::Ambiguous { h, why } => {
                                            let can_branch = cur_world_count * 2 <= MAX_WORLDS
                                                && next_worlds.len() + 2 <= MAX_WORLDS;
                                            if !can_branch && cap_exc.is_none() {
                                                *cap_exc = Some(format!(
                                                "{}: BooleanCombine(Union) contact is ambiguous at h = {h:.3e} ({why}) and exceeds hypothesis budget",
                                                m.name
                                            ));
                                            }
                                            let mut w_fold = w.clone();
                                            w_fold.had_ambiguous = true;
                                            {
                                                let mut ids = a.ids.clone();
                                                ids.extend(b.ids.clone());
                                                ids.push(m.feature_id.clone());
                                                let mut outputs = a.outputs.clone();
                                                outputs.extend(b.outputs.clone());
                                                outputs.push(OutputAnchor {
                                                    feature_id: m.feature_id.clone(),
                                                    index: 0,
                                                });
                                                w_fold.bodies.push(Body {
                                                    ids,
                                                    outputs,
                                                    expr: Expr::Union(vec![
                                                        a.expr.clone(),
                                                        b.expr.clone(),
                                                    ]),
                                                    last_touched: cur_seq,
                                                    arbitrary_output_order: false,
                                                });
                                                let mut next_slot = 1usize;
                                                carry_siblings(
                                                    &mut w_fold.bodies,
                                                    &m.anchors,
                                                    &m.feature_id,
                                                    cur_seq,
                                                    &mut next_slot,
                                                    Some(main_pos),
                                                );
                                                if propagate_arb {
                                                    mark_arb_if_multi(
                                                        &mut w_fold.bodies,
                                                        &m.feature_id,
                                                    );
                                                }
                                            }
                                            next_worlds.push(w_fold);
                                            if can_branch {
                                                let mut w_split = w;
                                                w_split.had_ambiguous = true;
                                                w_split
                                                    .assumed_disjoint
                                                    .push((a.expr.clone(), b.expr.clone()));
                                                for (slot, mut part) in
                                                    [a, b].into_iter().enumerate()
                                                {
                                                    part.ids.push(m.feature_id.clone());
                                                    part.outputs.push(OutputAnchor {
                                                        feature_id: m.feature_id.clone(),
                                                        index: slot,
                                                    });
                                                    part.last_touched = cur_seq;
                                                    part.arbitrary_output_order = true;
                                                    w_split.bodies.push(part);
                                                }
                                                let mut next_slot = 2usize;
                                                carry_siblings(
                                                    &mut w_split.bodies,
                                                    &m.anchors,
                                                    &m.feature_id,
                                                    cur_seq,
                                                    &mut next_slot,
                                                    Some(main_pos),
                                                );
                                                if propagate_arb {
                                                    mark_arb_if_multi(
                                                        &mut w_split.bodies,
                                                        &m.feature_id,
                                                    );
                                                }
                                                next_worlds.push(w_split);
                                            }
                                        }
                                    }
                                }
                            }
                            (Some(j), None) | (None, Some(j)) if m.kind == MergeKind::Union => {
                                w.bodies[j].ids.push(m.feature_id.clone());
                                w.bodies[j].outputs.push(OutputAnchor {
                                    feature_id: m.feature_id.clone(),
                                    index: 0,
                                });
                                w.bodies[j].last_touched = cur_seq;
                                let mut next_slot = 1usize;
                                carry_siblings(
                                    &mut w.bodies,
                                    &m.anchors,
                                    &m.feature_id,
                                    cur_seq,
                                    &mut next_slot,
                                    Some(j),
                                );
                                if propagate_arb {
                                    mark_arb_if_multi(&mut w.bodies, &m.feature_id);
                                }
                                next_worlds.push(w);
                            }
                            _ => next_worlds.push(w),
                        }
                    }
                    *worlds = next_worlds;
                }
            };

        apply_merges(
            &mut worlds,
            0,
            &mut touch_seq,
            &mut cap_exceeded,
            &mut lump_cache,
        );
        for (i, op) in self.ops.iter().take(k).enumerate() {
            touch_seq += 1;
            let cur_seq = touch_seq;
            let mut staged_batch: Vec<(CustodyWorld, bool)> = Vec::with_capacity(worlds.len());
            if let Targets::Explicit(ref named) = op.targets {
                if op.combine != Combine::NewBody {
                    let kind = match op.combine {
                        Combine::Add => MergeKind::Union,
                        Combine::Cut => MergeKind::Subtract,
                        Combine::Intersect => MergeKind::Intersect,
                        Combine::NewBody => unreachable!(),
                    };
                    let tool_expr = if op.alt_cut.is_some() {
                        Expr::Union(vec![Expr::Leaf(i), Expr::AltLeaf(i)])
                    } else {
                        Expr::Leaf(i)
                    };
                    let total_in = worlds.len();
                    for (wi_in, mut w) in worlds.drain(..).enumerate() {
                        if w.had_ambiguous {
                            w.consumer_after_ambiguous = true;
                        }
                        let rem = total_in - 1 - wi_in;
                        self.stage_explicit_op_anchors(
                            w,
                            named,
                            &op.name,
                            kind,
                            &tool_expr,
                            rem,
                            MAX_WORLDS,
                            &mut cap_exceeded,
                            &mut lump_cache,
                            &mut staged_batch,
                        );
                    }
                } else {
                    for w in worlds.drain(..) {
                        staged_batch.push((w, false));
                    }
                }
            } else {
                for mut w in worlds.drain(..) {
                    if op.combine != Combine::NewBody && w.had_ambiguous {
                        w.consumer_after_ambiguous = true;
                    }
                    staged_batch.push((w, false));
                }
            }
            let cur_world_count = staged_batch.len();
            let mut next_worlds = Vec::with_capacity(cur_world_count);
            for (batch_idx, (mut w, op_propagate_arb)) in staged_batch.into_iter().enumerate() {
                let rem_in_batch = cur_world_count - 1 - batch_idx;
                let target_idx = self.target_indices(&w.bodies, op.combine, &op.targets);
                match op.combine {
                    Combine::NewBody => {
                        w.bodies.push(Body {
                            ids: vec![op.feature_id.clone()],
                            outputs: vec![OutputAnchor {
                                feature_id: op.feature_id.clone(),
                                index: 0,
                            }],
                            expr: Expr::Leaf(i),
                            last_touched: cur_seq,
                            arbitrary_output_order: false,
                        });
                        next_worlds.push(w);
                    }
                    Combine::Add => {
                        if target_idx.is_empty() {
                            w.bodies.push(Body {
                                ids: vec![op.feature_id.clone()],
                                outputs: vec![OutputAnchor {
                                    feature_id: op.feature_id.clone(),
                                    index: 0,
                                }],
                                expr: Expr::Leaf(i),
                                last_touched: cur_seq,
                                arbitrary_output_order: false,
                            });
                            next_worlds.push(w);
                        } else {
                            let mut sorted = target_idx.clone();
                            sorted.sort_unstable();
                            let mut removed: Vec<Body> =
                                sorted.iter().rev().map(|&j| w.bodies.remove(j)).collect();
                            removed.reverse();

                            let n_nodes = 1 + removed.len();
                            let node_expr = |u: usize| -> Expr {
                                if u == 0 {
                                    Expr::Leaf(i)
                                } else {
                                    removed[u - 1].expr.clone()
                                }
                            };
                            let mut reach = vec![false; n_nodes];
                            reach[0] = true;
                            let mut bfs = vec![0usize];
                            let mut edges: Vec<(usize, usize, TouchVerdict)> = Vec::new();
                            while let Some(u) = bfs.pop() {
                                let eu = node_expr(u);
                                for (v, reached_v) in reach.iter_mut().enumerate() {
                                    if v == u {
                                        continue;
                                    }
                                    let (lo_n, hi_n) = (u.min(v), u.max(v));
                                    let tv = if let Some(&(_, _, cached)) =
                                        edges.iter().find(|&&(a, b, _)| a == lo_n && b == hi_n)
                                    {
                                        cached
                                    } else {
                                        let ev = node_expr(v);
                                        let computed = self.exprs_touch(&eu, &ev);
                                        edges.push((lo_n, hi_n, computed));
                                        computed
                                    };
                                    if matches!(
                                        tv,
                                        TouchVerdict::Touches | TouchVerdict::Ambiguous { .. }
                                    ) && !*reached_v
                                    {
                                        *reached_v = true;
                                        bfs.push(v);
                                    }
                                }
                            }

                            // Contract `Touches`-connected components inside `reach`.
                            let mut comp_of = vec![usize::MAX; n_nodes];
                            let mut num_comps = 0usize;
                            for start_n in 0..n_nodes {
                                if !reach[start_n] || comp_of[start_n] != usize::MAX {
                                    continue;
                                }
                                let cid = num_comps;
                                num_comps += 1;
                                comp_of[start_n] = cid;
                                let mut q = vec![start_n];
                                while let Some(u) = q.pop() {
                                    for &(a, b, tv) in &edges {
                                        if tv != TouchVerdict::Touches {
                                            continue;
                                        }
                                        let nb = if a == u {
                                            b
                                        } else if b == u {
                                            a
                                        } else {
                                            continue;
                                        };
                                        if reach[nb] && comp_of[nb] == usize::MAX {
                                            comp_of[nb] = cid;
                                            q.push(nb);
                                        }
                                    }
                                }
                            }
                            let c0 = comp_of[0];
                            let other_comps: Vec<usize> =
                                (0..num_comps).filter(|&c| c != c0).collect();
                            let m_amb = other_comps.len();

                            let emit_branch = |base_w: &CustodyWorld,
                                               in_tool: &[bool],
                                               extra_disjoint: Vec<(Expr, Expr)>,
                                               ambiguous_here: bool|
                             -> CustodyWorld {
                                let mut bw = base_w.clone();
                                if ambiguous_here {
                                    bw.had_ambiguous = true;
                                }
                                bw.assumed_disjoint.extend(extra_disjoint);
                                let main_pos = bw.bodies.len();
                                let mut ids = Vec::new();
                                let mut outputs = Vec::new();
                                let mut parts = vec![Expr::Leaf(i)];
                                let mut leftovers = Vec::new();
                                for (ri, b) in removed.iter().cloned().enumerate() {
                                    if in_tool[ri] {
                                        ids.extend(b.ids);
                                        outputs.extend(b.outputs);
                                        parts.push(b.expr);
                                    } else {
                                        leftovers.push(b);
                                    }
                                }
                                ids.push(op.feature_id.clone());
                                outputs.push(OutputAnchor {
                                    feature_id: op.feature_id.clone(),
                                    index: 0,
                                });
                                let expr = if parts.len() == 1 {
                                    Expr::Leaf(i)
                                } else {
                                    Expr::Union(parts)
                                };
                                bw.bodies.push(Body {
                                    ids,
                                    outputs,
                                    expr,
                                    last_touched: cur_seq,
                                    arbitrary_output_order: false,
                                });
                                let mut next_slot = 1usize;
                                for mut b in leftovers {
                                    b.ids.push(op.feature_id.clone());
                                    b.outputs.push(OutputAnchor {
                                        feature_id: op.feature_id.clone(),
                                        index: next_slot,
                                    });
                                    next_slot += 1;
                                    b.last_touched = cur_seq;
                                    bw.bodies.push(b);
                                }
                                if let Targets::Explicit(ref named) = op.targets {
                                    carry_siblings(
                                        &mut bw.bodies,
                                        named,
                                        &op.feature_id,
                                        cur_seq,
                                        &mut next_slot,
                                        Some(main_pos),
                                    );
                                }
                                if op_propagate_arb {
                                    mark_arb_if_multi(&mut bw.bodies, &op.feature_id);
                                }
                                bw
                            };

                            if m_amb == 0 {
                                let in_tool: Vec<bool> =
                                    (0..removed.len()).map(|ri| reach[1 + ri]).collect();
                                next_worlds.push(emit_branch(&w, &in_tool, Vec::new(), false));
                            } else {
                                let branch_count = 1usize << m_amb.min(5);
                                let can_branch = m_amb <= 4
                                    && cur_world_count * branch_count <= MAX_WORLDS
                                    && next_worlds.len() + branch_count <= MAX_WORLDS;
                                if !can_branch {
                                    if cap_exceeded.is_none() {
                                        cap_exceeded = Some(format!(
                                            "{}: Add has {m_amb} simultaneous ambiguous target contacts exceeding hypothesis budget",
                                            op.name
                                        ));
                                    }
                                    let in_tool: Vec<bool> =
                                        (0..removed.len()).map(|ri| reach[1 + ri]).collect();
                                    next_worlds.push(emit_branch(&w, &in_tool, Vec::new(), true));
                                } else {
                                    for mask in (0..branch_count).rev() {
                                        let mut sel = vec![false; num_comps];
                                        sel[c0] = true;
                                        for (bit, &cid) in other_comps.iter().enumerate() {
                                            if (mask >> bit) & 1 == 1 {
                                                sel[cid] = true;
                                            }
                                        }
                                        // Verify `sel` is connected to `c0` via `Ambiguous` edges.
                                        let mut conn = vec![false; num_comps];
                                        conn[c0] = true;
                                        let mut cq = vec![c0];
                                        while let Some(cu) = cq.pop() {
                                            for &(a, b, tv) in &edges {
                                                if !matches!(tv, TouchVerdict::Ambiguous { .. }) {
                                                    continue;
                                                }
                                                let (ca, cb) = (comp_of[a], comp_of[b]);
                                                if ca == cu && sel[cb] && !conn[cb] {
                                                    conn[cb] = true;
                                                    cq.push(cb);
                                                } else if cb == cu && sel[ca] && !conn[ca] {
                                                    conn[ca] = true;
                                                    cq.push(ca);
                                                }
                                            }
                                        }
                                        if (0..num_comps).any(|c| sel[c] && !conn[c]) {
                                            continue;
                                        }
                                        let mut extra_disjoint = Vec::new();
                                        for &(a, b, tv) in &edges {
                                            if !matches!(tv, TouchVerdict::Ambiguous { .. }) {
                                                continue;
                                            }
                                            let (in_a, in_b) = (sel[comp_of[a]], sel[comp_of[b]]);
                                            if in_a != in_b {
                                                extra_disjoint.push((node_expr(a), node_expr(b)));
                                            }
                                        }
                                        let in_tool: Vec<bool> = (0..removed.len())
                                            .map(|ri| reach[1 + ri] && sel[comp_of[1 + ri]])
                                            .collect();
                                        next_worlds.push(emit_branch(
                                            &w,
                                            &in_tool,
                                            extra_disjoint,
                                            true,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    Combine::Cut | Combine::Intersect => {
                        let apply_cut_or_inter = |mut cw: CustodyWorld, tool_leaf: Expr| {
                            let mut next_slot = 0usize;
                            let first_pos = target_idx.first().copied();
                            let mut empty_positions: Vec<usize> = Vec::new();
                            for &j in &target_idx {
                                let old =
                                    std::mem::replace(&mut cw.bodies[j].expr, tool_leaf.clone());
                                let new_expr = if op.combine == Combine::Cut {
                                    Expr::Sub(Box::new(old), Box::new(tool_leaf.clone()))
                                } else {
                                    Expr::Inter(Box::new(old), Box::new(tool_leaf.clone()))
                                };
                                if cw.consumer_after_ambiguous
                                    && !self.expr_has_occupied_sample(&new_expr, EXACT_VOLUME_CELLS)
                                {
                                    cw.bodies[j].expr = new_expr;
                                    empty_positions.push(j);
                                } else {
                                    cw.bodies[j].expr = new_expr;
                                    cw.bodies[j].ids.push(op.feature_id.clone());
                                    cw.bodies[j].outputs.push(OutputAnchor {
                                        feature_id: op.feature_id.clone(),
                                        index: next_slot,
                                    });
                                    next_slot += 1;
                                    cw.bodies[j].last_touched = cur_seq;
                                }
                            }
                            let adj_first_pos = if !empty_positions.is_empty() {
                                empty_positions.sort_unstable();
                                for &ej in empty_positions.iter().rev() {
                                    cw.bodies.remove(ej);
                                }
                                first_pos.map(|fp| {
                                    let removed_before =
                                        empty_positions.iter().filter(|&&ej| ej < fp).count();
                                    fp - removed_before
                                })
                            } else {
                                first_pos
                            };
                            if let Targets::Explicit(ref named) = op.targets {
                                carry_siblings(
                                    &mut cw.bodies,
                                    named,
                                    &op.feature_id,
                                    cur_seq,
                                    &mut next_slot,
                                    adj_first_pos,
                                );
                            }
                            if op_propagate_arb {
                                mark_arb_if_multi(&mut cw.bodies, &op.feature_id);
                            }
                            cw
                        };

                        if op.combine == Combine::Cut && !target_idx.is_empty() {
                            if let Some(ref alt) = op.alt_cut {
                                let (use_primary, use_alt) = if cur_world_count == 1
                                    && !matches!(op.targets, Targets::Explicit(_))
                                {
                                    (true, true)
                                } else {
                                    let te = self.target_extent_for_bodies(
                                        &w.bodies,
                                        op.combine,
                                        &op.targets,
                                        alt.dir_unit,
                                    );
                                    let (can_true, can_false) = if let Some(comps) =
                                        te.components.as_deref()
                                    {
                                        let all_rev_true = comps.iter().all(|c| {
                                            c.occupied.is_some_and(|(occ_lo_c, _)| {
                                                let c_mid_hi = 0.5 * (occ_lo_c + c.outer_proj.1);
                                                let c_pad = 1e-9
                                                    * (c.outer_proj.1 - c.outer_proj.0)
                                                        .max(f64::MIN_POSITIVE);
                                                alt.sketch_proj > c_mid_hi + c_pad
                                            })
                                        });
                                        let all_rev_false = comps.iter().all(|c| {
                                            c.occupied.is_some_and(|(_, occ_hi_c)| {
                                                let c_mid_lo = 0.5 * (c.outer_proj.0 + occ_hi_c);
                                                let c_pad = 1e-9
                                                    * (c.outer_proj.1 - c.outer_proj.0)
                                                        .max(f64::MIN_POSITIVE);
                                                alt.sketch_proj < c_mid_lo - c_pad
                                            })
                                        });
                                        (!all_rev_false, !all_rev_true)
                                    } else if let Some(ext) = te.primary {
                                        match ext.occupied {
                                            Some((occ_lo, occ_hi)) => {
                                                let mid_lo = 0.5 * (ext.outer_proj.0 + occ_hi);
                                                let mid_hi = 0.5 * (occ_lo + ext.outer_proj.1);
                                                let fp_pad = 1e-9
                                                    * (ext.outer_proj.1 - ext.outer_proj.0)
                                                        .max(f64::MIN_POSITIVE);
                                                (
                                                    mid_lo - fp_pad <= alt.sketch_proj,
                                                    alt.sketch_proj <= mid_hi + fp_pad,
                                                )
                                            }
                                            None => {
                                                if w.indeterminate.is_none() {
                                                    w.indeterminate = Some(alt.why.clone());
                                                }
                                                (alt.primary_rev, !alt.primary_rev)
                                            }
                                        }
                                    } else {
                                        (alt.primary_rev, !alt.primary_rev)
                                    };
                                    let up = if alt.primary_rev { can_true } else { can_false };
                                    let ua = if alt.primary_rev { can_false } else { can_true };
                                    (up, ua)
                                };

                                w.had_ambiguous = true;
                                w.consumer_after_ambiguous = true;
                                if w.ambiguity_why.is_none() {
                                    w.ambiguity_why = Some(alt.why.clone());
                                }
                                match (use_primary, use_alt) {
                                    (true, true) => {
                                        if next_worlds.len() + 2 + rem_in_batch <= MAX_WORLDS {
                                            let w_alt = w.clone();
                                            next_worlds.push(apply_cut_or_inter(w, Expr::Leaf(i)));
                                            next_worlds
                                                .push(apply_cut_or_inter(w_alt, Expr::AltLeaf(i)));
                                        } else {
                                            if cap_exceeded.is_none() {
                                                cap_exceeded = Some(alt.why.clone());
                                            }
                                            next_worlds.push(apply_cut_or_inter(w, Expr::Leaf(i)));
                                        }
                                    }
                                    (false, true) => {
                                        next_worlds.push(apply_cut_or_inter(w, Expr::AltLeaf(i)));
                                    }
                                    _ => {
                                        next_worlds.push(apply_cut_or_inter(w, Expr::Leaf(i)));
                                    }
                                }
                                continue;
                            }
                        }

                        next_worlds.push(apply_cut_or_inter(w, Expr::Leaf(i)));
                    }
                }
            }
            worlds = next_worlds;
            apply_merges(
                &mut worlds,
                i + 1,
                &mut touch_seq,
                &mut cap_exceeded,
                &mut lump_cache,
            );
        }
        WorldsResult {
            worlds,
            cap_exceeded,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stage_explicit_merge_anchors(
        &self,
        w: CustodyWorld,
        m: &BodyMerge,
        rem_outer: usize,
        max_worlds: usize,
        cap_exc: &mut Option<String>,
        lump_cache: &mut Vec<(Expr, LumpPartitionOutcome)>,
        out: &mut Vec<(CustodyWorld, bool)>,
    ) {
        let fid0 = &m.anchors[0].feature_id;
        let fid1 = &m.anchors[1].feature_id;
        let is_sym = m.kind != MergeKind::Subtract;
        let union_for_fid = |bodies: &[Body], fid: &str| -> Option<Expr> {
            let parts: Vec<Expr> = bodies
                .iter()
                .filter(|b| b.outputs.iter().any(|o| o.feature_id == fid))
                .map(|b| b.expr.clone())
                .collect();
            match parts.len() {
                0 => None,
                1 => parts.into_iter().next(),
                _ => Some(Expr::Union(parts)),
            }
        };
        if fid0 == fid1 {
            let mut named_slots = vec![m.anchors[0].index];
            if !named_slots.contains(&m.anchors[1].index) {
                named_slots.push(m.anchors[1].index);
            }
            self.stage_fid_anchor(
                w,
                false,
                fid0,
                &named_slots,
                &m.name,
                m.kind,
                m.kind == MergeKind::Subtract,
                is_sym,
                None,
                rem_outer,
                max_worlds,
                cap_exc,
                lump_cache,
                out,
            );
        } else {
            let other_for_0 = union_for_fid(&w.bodies, fid1);
            let mut mid: Vec<(CustodyWorld, bool)> = Vec::new();
            let cap_for_0 = max_worlds.saturating_sub(out.len());
            self.stage_fid_anchor(
                w,
                false,
                fid0,
                &[m.anchors[0].index],
                &m.name,
                m.kind,
                false,
                is_sym,
                other_for_0.as_ref(),
                rem_outer,
                cap_for_0,
                cap_exc,
                lump_cache,
                &mut mid,
            );
            let mid_len = mid.len();
            for (mi, (mw, prop)) in mid.into_iter().enumerate() {
                let other_for_1 = union_for_fid(&mw.bodies, fid0);
                let rem = rem_outer + (mid_len - 1 - mi);
                self.stage_fid_anchor(
                    mw,
                    prop,
                    fid1,
                    &[m.anchors[1].index],
                    &m.name,
                    m.kind,
                    m.kind == MergeKind::Subtract,
                    is_sym,
                    other_for_1.as_ref(),
                    rem,
                    max_worlds,
                    cap_exc,
                    lump_cache,
                    out,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stage_explicit_op_anchors(
        &self,
        w: CustodyWorld,
        named: &[OutputAnchor],
        op_name: &str,
        kind: MergeKind,
        tool_expr: &Expr,
        rem_outer: usize,
        max_worlds: usize,
        cap_exc: &mut Option<String>,
        lump_cache: &mut Vec<(Expr, LumpPartitionOutcome)>,
        out: &mut Vec<(CustodyWorld, bool)>,
    ) {
        let mut distinct_fids: Vec<&str> = Vec::new();
        for a in named {
            if !distinct_fids.contains(&a.feature_id.as_str()) {
                distinct_fids.push(&a.feature_id);
            }
        }
        let mut cur_stage = vec![(w, false)];
        for fid in distinct_fids {
            let mut named_slots: Vec<usize> = Vec::new();
            for a in named {
                if a.feature_id == fid && !named_slots.contains(&a.index) {
                    named_slots.push(a.index);
                }
            }
            let stage_len = cur_stage.len();
            let mut next_stage = Vec::with_capacity(stage_len);
            for (si, (sw, prop)) in cur_stage.into_iter().enumerate() {
                let rem = rem_outer + (stage_len - 1 - si);
                let cap_here = max_worlds.saturating_sub(out.len());
                self.stage_fid_anchor(
                    sw,
                    prop,
                    fid,
                    &named_slots,
                    op_name,
                    kind,
                    false,
                    true,
                    Some(tool_expr),
                    rem,
                    cap_here,
                    cap_exc,
                    lump_cache,
                    &mut next_stage,
                );
            }
            cur_stage = next_stage;
        }
        out.extend(cur_stage);
    }

    #[allow(clippy::too_many_arguments)]
    fn stage_fid_anchor(
        &self,
        mut w: CustodyWorld,
        prop_in: bool,
        fid: &str,
        named_for_fid: &[usize],
        consumer_name: &str,
        kind: MergeKind,
        is_subtract_tool: bool,
        is_symmetric_all_slots: bool,
        other_expr: Option<&Expr>,
        rem_worlds: usize,
        max_worlds: usize,
        cap_exc: &mut Option<String>,
        lump_cache: &mut Vec<(Expr, LumpPartitionOutcome)>,
        out: &mut Vec<(CustodyWorld, bool)>,
    ) {
        if !w
            .bodies
            .iter()
            .any(|b| b.outputs.iter().any(|o| o.feature_id == fid))
        {
            out.push((w, prop_in));
            return;
        }
        // 1. Partition any unsplit multi-component Sub/Inter body carrying `fid`.
        for bi in (0..w.bodies.len()).rev() {
            if !w.bodies[bi].outputs.iter().any(|o| o.feature_id == fid)
                || !expr_needs_lump_split(&w.bodies[bi].expr)
            {
                continue;
            }
            let outcome = if let Some((_, cached)) =
                lump_cache.iter().find(|(e, _)| *e == w.bodies[bi].expr)
            {
                cached.clone()
            } else {
                let computed = self.partition_lumps(&w.bodies[bi].expr, EXACT_VOLUME_CELLS);
                lump_cache.push((w.bodies[bi].expr.clone(), computed.clone()));
                computed
            };
            match outcome {
                LumpPartitionOutcome::Empty => {
                    if w.indeterminate.is_none() {
                        w.indeterminate = Some(format!(
                            "{consumer_name}: explicit FeatureOutput({fid}, index={}) targets a cut/intersect output with 0 lattice component(s) at {EXACT_VOLUME_CELLS} cells — OutputKey ordering across split/consumed lumps is not determined by the document",
                            named_for_fid.first().copied().unwrap_or(0)
                        ));
                    }
                    out.push((w, prop_in));
                    return;
                }
                LumpPartitionOutcome::Single => {}
                LumpPartitionOutcome::Multi(part) => {
                    let num_lumps = part.0.num_lumps;
                    let mut distinct_old_fids: Vec<String> = Vec::new();
                    for o in &w.bodies[bi].outputs {
                        if !distinct_old_fids.contains(&o.feature_id) {
                            distinct_old_fids.push(o.feature_id.clone());
                        }
                    }
                    let mut max_slots: Vec<usize> = distinct_old_fids
                        .iter()
                        .map(|ofid| {
                            w.bodies
                                .iter()
                                .flat_map(|b| b.outputs.iter())
                                .filter(|o| &o.feature_id == ofid)
                                .map(|o| o.index)
                                .max()
                                .unwrap_or(0)
                        })
                        .collect();
                    let orig = w.bodies.remove(bi);
                    for c in 0..num_lumps {
                        let lump_outputs = if c == 0 {
                            orig.outputs.clone()
                        } else {
                            distinct_old_fids
                                .iter()
                                .enumerate()
                                .map(|(fi, ofid)| {
                                    max_slots[fi] += 1;
                                    OutputAnchor {
                                        feature_id: ofid.clone(),
                                        index: max_slots[fi],
                                    }
                                })
                                .collect()
                        };
                        w.bodies.insert(
                            bi + c,
                            Body {
                                ids: orig.ids.clone(),
                                outputs: lump_outputs,
                                expr: Expr::Lump {
                                    base: Box::new(orig.expr.clone()),
                                    partition: part.clone(),
                                    lump: c as u16,
                                },
                                last_touched: orig.last_touched,
                                arbitrary_output_order: true,
                            },
                        );
                    }
                }
            }
        }

        // 2. Collect candidate bodies for `fid` after expansion.
        let cand_pos: Vec<usize> = w
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.outputs.iter().any(|o| o.feature_id == fid))
            .map(|(bi, _)| bi)
            .collect();
        if cand_pos.len() <= 1
            || !cand_pos
                .iter()
                .any(|&bi| w.bodies[bi].arbitrary_output_order)
        {
            out.push((w, prop_in));
            return;
        }
        let mut cur_slots: Vec<usize> = Vec::with_capacity(cand_pos.len());
        for &bi in &cand_pos {
            let mut it = w.bodies[bi]
                .outputs
                .iter()
                .filter(|o| o.feature_id == fid)
                .map(|o| o.index);
            let Some(s0) = it.next() else {
                out.push((w, prop_in));
                return;
            };
            if it.next().is_some() {
                out.push((w, prop_in));
                return;
            }
            cur_slots.push(s0);
        }
        let all_slots_named = cur_slots.iter().all(|s| named_for_fid.contains(s));
        if all_slots_named && is_symmetric_all_slots {
            out.push((w, true));
            return;
        }
        let full_expr = Expr::Union(
            cand_pos
                .iter()
                .map(|&bi| w.bodies[bi].expr.clone())
                .collect(),
        );
        let order_independent = match kind {
            MergeKind::Union => other_expr
                .is_some_and(|oe| self.exprs_touch(&full_expr, oe) == TouchVerdict::Disjoint),
            MergeKind::Subtract => {
                !is_subtract_tool
                    && other_expr.is_some_and(|oe| {
                        self.exprs_touch(&full_expr, oe) == TouchVerdict::Disjoint
                    })
            }
            MergeKind::Intersect => false,
        };
        if order_independent {
            out.push((w, true));
            return;
        }
        if !named_for_fid.iter().all(|s| cur_slots.contains(s)) {
            out.push((w, prop_in));
            return;
        }

        // 3. Branch across candidate assignments of `named_for_fid` onto `cand_pos`.
        let k_cands = cand_pos.len();
        let r = named_for_fid.len();
        let mut choices: Vec<Vec<usize>> = Vec::new();
        if r == 1 {
            for m_idx in 0..k_cands {
                choices.push(vec![m_idx]);
            }
        } else {
            fn gen_tuples(
                k: usize,
                r: usize,
                ordered: bool,
                cur: &mut Vec<usize>,
                out_c: &mut Vec<Vec<usize>>,
            ) {
                if cur.len() == r {
                    out_c.push(cur.clone());
                    return;
                }
                let start = if ordered {
                    0
                } else {
                    cur.last().map_or(0, |&last| last + 1)
                };
                for idx in start..k {
                    if ordered && cur.contains(&idx) {
                        continue;
                    }
                    cur.push(idx);
                    gen_tuples(k, r, ordered, cur, out_c);
                    cur.pop();
                }
            }
            let mut buf = Vec::with_capacity(r);
            gen_tuples(k_cands, r, !is_symmetric_all_slots, &mut buf, &mut choices);
        }

        let can_branch =
            !choices.is_empty() && out.len() + choices.len() + rem_worlds <= max_worlds;
        if !can_branch {
            if cap_exc.is_none() {
                *cap_exc = Some(format!(
                    "{consumer_name}: explicit FeatureOutput({fid}, index={}) has {} candidate lump assignments exceeding hypothesis budget ({max_worlds})",
                    named_for_fid[0],
                    choices.len()
                ));
            }
            w.had_ambiguous = true;
            w.consumer_after_ambiguous = true;
            out.push((w, true));
            return;
        }

        let unclaimed_slots: Vec<usize> = cur_slots
            .iter()
            .copied()
            .filter(|s| !named_for_fid.contains(s))
            .collect();
        let why = format!(
            "{consumer_name}: explicit FeatureOutput({fid}, index={}) targets a {k_cands}-lump feature output whose outcome depends on OutputKey ordering",
            named_for_fid[0]
        );
        for chosen in choices {
            let mut bw = w.clone();
            bw.had_ambiguous = true;
            bw.consumer_after_ambiguous = true;
            if bw.ambiguity_why.is_none() {
                bw.ambiguity_why = Some(why.clone());
            }
            let mut rem_it = unclaimed_slots.iter().copied();
            for (p, &bi) in cand_pos.iter().enumerate() {
                let (new_slot, still_arb) =
                    if let Some(pos_in_named) = chosen.iter().position(|&c_idx| c_idx == p) {
                        (named_for_fid[pos_in_named], false)
                    } else {
                        (
                            rem_it.next().expect("unclaimed slot count matches"),
                            unclaimed_slots.len() >= 2,
                        )
                    };
                if let Some(oa) = bw.bodies[bi]
                    .outputs
                    .iter_mut()
                    .find(|o| o.feature_id == fid)
                {
                    oa.index = new_slot;
                }
                bw.bodies[bi].arbitrary_output_order = still_arb;
            }
            out.push((bw, prop_in));
        }
    }

    /// Evaluate candidate fold/split worlds on a single joint
    /// [`EXACT_VOLUME_CELLS`] lattice, refuting any world whose
    /// `assumed_disjoint` pair shares an occupied voxel and declining only if
    /// surviving worlds disagree on total occupied cube count or carry an
    /// indeterminate explicit anchor.
    fn check_worlds_volume_agreement(&self, worlds: &[CustodyWorld]) -> Option<String> {
        let mut wlo = [f64::INFINITY; 3];
        let mut whi = [f64::NEG_INFINITY; 3];
        let mut any_box = false;
        let mut unique_exprs: Vec<Expr> = Vec::new();
        let intern_expr = |list: &mut Vec<Expr>, e: &Expr| -> usize {
            if let Some(pos) = list.iter().position(|x| x == e) {
                pos
            } else {
                list.push(e.clone());
                list.len() - 1
            }
        };
        let mut unique_pairs: Vec<(usize, usize)> = Vec::new();
        let mut world_body_ids: Vec<Vec<usize>> = Vec::with_capacity(worlds.len());
        let mut world_pair_ids: Vec<Vec<usize>> = Vec::with_capacity(worlds.len());
        for w in worlds {
            if let Some((blo, bhi)) = self.bodies_bbox(&w.bodies) {
                for a in 0..3 {
                    wlo[a] = wlo[a].min(blo[a]);
                    whi[a] = whi[a].max(bhi[a]);
                }
                any_box = true;
            }
            let bids = w
                .bodies
                .iter()
                .map(|b| intern_expr(&mut unique_exprs, &b.expr))
                .collect();
            world_body_ids.push(bids);
            let mut pids = Vec::with_capacity(w.assumed_disjoint.len());
            for (x, y) in &w.assumed_disjoint {
                for sub_e in [x, y] {
                    if let Some((blo, bhi)) = self.expr_bbox(sub_e) {
                        for a in 0..3 {
                            wlo[a] = wlo[a].min(blo[a]);
                            whi[a] = whi[a].max(bhi[a]);
                        }
                        any_box = true;
                    }
                }
                let ix = intern_expr(&mut unique_exprs, x);
                let iy = intern_expr(&mut unique_exprs, y);
                let pair = (ix.min(iy), ix.max(iy));
                let pid = if let Some(pos) = unique_pairs.iter().position(|&p| p == pair) {
                    pos
                } else {
                    unique_pairs.push(pair);
                    unique_pairs.len() - 1
                };
                pids.push(pid);
            }
            world_pair_ids.push(pids);
        }
        if !any_box {
            return worlds.iter().find_map(|w| w.indeterminate.clone());
        }
        let f = &self.frame;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for c in 0..8 {
            let p = [
                if c & 1 == 0 { wlo[0] } else { whi[0] },
                if c & 2 == 0 { wlo[1] } else { whi[1] },
                if c & 4 == 0 { wlo[2] } else { whi[2] },
            ];
            let r = sub(p, f.origin);
            let q = [dot(r, f.x), dot(r, f.y), dot(r, f.n)];
            for a in 0..3 {
                lo[a] = lo[a].min(q[a]);
                hi[a] = hi[a].max(q[a]);
            }
        }
        let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let longest = ext[0].max(ext[1]).max(ext[2]);
        if !longest.is_finite() || longest <= 0.0 {
            return worlds.iter().find_map(|w| w.indeterminate.clone());
        }
        let h = longest / EXACT_VOLUME_CELLS as f64;
        let n = [
            ((ext[0] / h).ceil() as usize + 2).max(1),
            ((ext[1] / h).ceil() as usize + 2).max(1),
            ((ext[2] / h).ceil() as usize + 2).max(1),
        ];
        let start = [lo[0] - h, lo[1] - h, lo[2] - h];
        let expr_boxes: Vec<Option<Box3>> =
            unique_exprs.iter().map(|e| self.expr_bbox(e)).collect();
        let mut expr_cubes = vec![0usize; unique_exprs.len()];
        let mut pair_refuted = vec![false; unique_pairs.len()];
        let mut hits = vec![false; unique_exprs.len()];

        for k in 0..n[2] {
            let w_coord = start[2] + (k as f64 + 0.5) * h;
            let base_w = add(f.origin, scale(f.n, w_coord));
            for j in 0..n[1] {
                let v_coord = start[1] + (j as f64 + 0.5) * h;
                let base_vw = add(base_w, scale(f.y, v_coord));
                for i in 0..n[0] {
                    let u_coord = start[0] + (i as f64 + 0.5) * h;
                    let p = add(base_vw, scale(f.x, u_coord));
                    for (idx, e) in unique_exprs.iter().enumerate() {
                        let in_box = expr_boxes[idx].is_some_and(|(blo, bhi)| {
                            p[0] >= blo[0]
                                && p[0] <= bhi[0]
                                && p[1] >= blo[1]
                                && p[1] <= bhi[1]
                                && p[2] >= blo[2]
                                && p[2] <= bhi[2]
                        });
                        let inside = in_box && self.expr_contains(e, p);
                        hits[idx] = inside;
                        if inside {
                            expr_cubes[idx] += 1;
                        }
                    }
                    for (pid, &(ix, iy)) in unique_pairs.iter().enumerate() {
                        if !pair_refuted[pid] && hits[ix] && hits[iy] {
                            pair_refuted[pid] = true;
                        }
                    }
                }
            }
        }

        let mut base_cubes: Option<(usize, usize)> = None;
        for (wi, w) in worlds.iter().enumerate() {
            if world_pair_ids[wi].iter().any(|&pid| pair_refuted[pid]) {
                continue;
            }
            if let Some(ref ind) = w.indeterminate {
                return Some(ind.clone());
            }
            let cubes: usize = world_body_ids[wi].iter().map(|&bid| expr_cubes[bid]).sum();
            match base_cubes {
                None => base_cubes = Some((wi, cubes)),
                Some((w0, c0)) if c0 != cubes => {
                    if let Some(why) = w
                        .ambiguity_why
                        .as_deref()
                        .or(worlds[w0].ambiguity_why.as_deref())
                    {
                        return Some(format!(
                            "{why} ({c0} vs {cubes} lattice cubes at {EXACT_VOLUME_CELLS} cells)"
                        ));
                    }
                    return Some(format!(
                        "ambiguous Add/Union contact changes downstream consumer volume ({c0} vs {cubes} lattice cubes at {EXACT_VOLUME_CELLS} cells)"
                    ));
                }
                Some(_) => {}
            }
        }
        None
    }

    /// Indices (into `bodies`) of the bodies a feature with this combine
    /// choice acts on.
    fn target_indices(&self, bodies: &[Body], combine: Combine, targets: &Targets) -> Vec<usize> {
        if combine == Combine::NewBody {
            return Vec::new();
        }
        match targets {
            Targets::MostRecent => {
                let latest = bodies.iter().map(|b| b.last_touched).max();
                bodies
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| Some(b.last_touched) == latest)
                    .map(|(j, _)| j)
                    .collect()
            }
            Targets::Explicit(anchors) => {
                let mut out = Vec::new();
                for a in anchors {
                    if let Some(j) = bodies.iter().position(|b| b.outputs.iter().any(|o| o == a)) {
                        if !out.contains(&j) {
                            out.push(j);
                        }
                    }
                }
                out
            }
        }
    }

    /// Bounding box of the live bodies after the whole chain.
    pub fn bbox(&self) -> Option<([f64; 3], [f64; 3])> {
        self.bbox_prefix(self.ops.len())
    }

    /// Bounding box of the live bodies after the first `k` operations.
    pub fn bbox_prefix(&self, k: usize) -> Option<([f64; 3], [f64; 3])> {
        let bodies = self.bodies_after(k);
        self.bodies_bbox(&bodies)
    }

    fn bodies_bbox(&self, bodies: &[Body]) -> Option<([f64; 3], [f64; 3])> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        let mut any = false;
        for (a, b) in bodies.iter().filter_map(|b| self.expr_bbox(&b.expr)) {
            for k in 0..3 {
                lo[k] = lo[k].min(a[k]);
                hi[k] = hi[k].max(b[k]);
            }
            any = true;
        }
        any.then_some((lo, hi))
    }

    /// Operand `k` on its own, as a standalone boss (a cut's TOOL as a body).
    pub fn operand_alone(&self, k: usize) -> ExactChain {
        let mut op = self.ops[k].clone();
        op.alt_cut = None;
        op.cut = false;
        op.combine = Combine::NewBody;
        op.targets = Targets::MostRecent;
        ExactChain {
            ops: vec![op],
            merges: Vec::new(),
            notes: self.notes.clone(),
            indeterminate: Vec::new(),
            frame: self.frame,
        }
    }

    /// Parse a `.waffle` document (single Part tab).
    pub fn from_waffle(waffle: &Value) -> Result<Self, NotCovered> {
        let tabs = waffle
            .get("tabs")
            .and_then(Value::as_array)
            .ok_or_else(|| NotCovered::Shape("no tabs".into()))?;
        let tab = tabs
            .first()
            .ok_or_else(|| NotCovered::Shape("no tabs".into()))?;
        let feats = tab
            .pointer("/kind/features/features")
            .and_then(Value::as_array)
            .ok_or_else(|| NotCovered::Shape("no feature list".into()))?;
        let mut sketches: HashMap<String, ParsedSketch> = HashMap::new();
        let mut chain = ExactChain {
            ops: Vec::new(),
            merges: Vec::new(),
            notes: Vec::new(),
            indeterminate: Vec::new(),
            frame: Basis::from_origin_normal([0.0; 3], [0.0, 0.0, 1.0]).unwrap(),
        };
        let mut frame_set = false;
        for (index, f) in feats.iter().enumerate() {
            let name = f
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_string();
            let feature_id = f
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if f.get("suppressed").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let op = f
                .get("operation")
                .ok_or_else(|| nc(index, &name, "no operation"))?;
            let ty = op.get("type").and_then(Value::as_str).unwrap_or("");
            match ty {
                "Sketch" => {
                    let sk = op
                        .get("sketch")
                        .ok_or_else(|| nc(index, &name, "no sketch"))?;
                    let id = sk
                        .get("id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| nc(index, &name, "sketch without id"))?;
                    let parsed = ParsedSketch::parse(sk).map_err(|why| nc(index, &name, &why))?;
                    if !frame_set {
                        chain.frame = parsed.basis;
                        frame_set = true;
                    }
                    // Keyed the way the engine resolves `params.sketch_id`:
                    // by the Sketch FEATURE's id (`rebuild.rs`
                    // `find_sketch_in_tree` matches `feature.id`). The
                    // engine-authored corpus mints the inner `sketch.id`
                    // equal to the feature id, so the two keys coincided
                    // there; the prospector's gen3 documents (P-series) mint
                    // them apart, and keying by the inner id read every one
                    // as "sketch not defined before this feature"
                    // (2026-09-28, P0004's meta adjudication).
                    let key = if feature_id.is_empty() {
                        id.to_string()
                    } else {
                        feature_id.clone()
                    };
                    sketches.insert(key, parsed);
                }
                "Extrude" => {
                    let params = op
                        .get("params")
                        .ok_or_else(|| nc(index, &name, "no params"))?;
                    if params.get("regions").is_some_and(|r| !r.is_null()) {
                        return Err(nc(index, &name, "region extrude (params.regions)"));
                    }
                    let sk = sketch_for(&sketches, params).map_err(|w| nc(index, &name, &w))?;
                    // A region extrude (`params.region`) carries its own
                    // footprint — the engine extrudes exactly that polygon
                    // with its holes (`profile_footprint_2d`), in sketch
                    // coordinates.
                    let profile = match params.get("region").filter(|r| r.is_object()) {
                        Some(region) => region_profile(region).map_err(|w| nc(index, &name, &w))?,
                        None => sk.profile(params).map_err(|w| nc(index, &name, &w))?,
                    };
                    let (combine, targets) =
                        parse_combine(params).map_err(|w| nc(index, &name, &w))?;
                    let is_cut = combine == Combine::Cut;
                    // The material this feature measures for its sweep
                    // direction and its through-all depth: its PRIMARY
                    // (first) combine target (`rebuild.rs`:
                    // `combine_targets.first()` / `find_most_recent_solid`).
                    if !chain.has_target_body(combine, &targets)
                        && matches!(combine, Combine::Cut | Combine::Intersect)
                    {
                        chain.notes.push(format!(
                            "{name}: {combine:?} with no target body — the engine reports ResolutionFailed"
                        ));
                    }
                    let explicit_dir = params.get("direction").and_then(vec3);
                    let direction = explicit_dir.unwrap_or(sk.basis.n);
                    let dir_unit = norm(direction)
                        .ok_or_else(|| nc(index, &name, "zero extrude direction"))?;
                    if dot(dir_unit, sk.basis.n).abs() < 1e-9 {
                        return Err(nc(index, &name, "extrude direction in the sketch plane"));
                    }
                    let blind = params.get("depth").and_then(Value::as_f64).unwrap_or(0.0);
                    let mode = params
                        .pointer("/depth_mode/type")
                        .and_then(Value::as_str)
                        .unwrap_or("Blind");
                    let needs_axis_extent =
                        (is_cut && explicit_dir.is_none()) || mode == "ThroughAll";
                    let extent =
                        needs_axis_extent.then(|| chain.target_extent(combine, &targets, dir_unit));
                    if let Some(why) = extent.as_ref().and_then(|e| e.world_indeterminate.as_ref())
                    {
                        chain.indeterminate.push(format!("{name}: {why}"));
                    }
                    // Cut auto-reversal (engine: target material's mid-extent
                    // along `dir_unit` behind the sketch plane ⇒ reverse). It
                    // is decided BEFORE the depth, because a through-all depth
                    // is measured along the direction the extrude actually
                    // sweeps (`rebuild.rs` `swept_primary`).
                    let sketch_proj = dot(sk.basis.origin, dir_unit);
                    let mut multi_comp_disagree_why: Option<String> = None;
                    let reverse = if is_cut && explicit_dir.is_none() {
                        match extent.as_ref().and_then(|e| e.primary) {
                            Some(ext) => {
                                let (outer_lo, outer_hi) = ext.outer_proj;
                                let (mid_lo, mid_hi, body_mid) = match ext.occupied {
                                    Some((occ_lo, occ_hi)) => {
                                        let m_lo = 0.5 * (outer_lo + occ_hi);
                                        let m_hi = 0.5 * (occ_lo + outer_hi);
                                        (m_lo, m_hi, 0.5 * (occ_lo + occ_hi))
                                    }
                                    None => {
                                        chain.indeterminate.push(format!(
                                            "{name}: primary target has no occupied lattice samples at {EXACT_VOLUME_CELLS} cells (h = {:.3e}) — cut auto-reversal is indeterminate",
                                            ext.uncertainty
                                        ));
                                        let m = 0.5 * (outer_lo + outer_hi);
                                        (outer_lo, outer_hi, m)
                                    }
                                };
                                let mut rev = body_mid < sketch_proj;
                                if std::env::var_os("EXACT_CUT_TRACE").is_some() {
                                    eprintln!(
                                        "[exact-cut] {name}: targets={:?} occ={:?} outer=[{outer_lo:.6e}, {outer_hi:.6e}] \
                                         mid_iv=[{mid_lo:.6e}, {mid_hi:.6e}] h={:.6e} sketch_proj={sketch_proj:.6e} \
                                         basis.n={:?} direction={direction:?} reverse={rev}",
                                        targets,
                                        ext.occupied,
                                        ext.uncertainty,
                                        sk.basis.n
                                    );
                                }
                                if let Some(t_ext) = extent.as_ref().and_then(|e| e.tool_alone) {
                                    let (tmin, tmax) = t_ext.occupied.unwrap_or(t_ext.outer_proj);
                                    let tool_rev = 0.5 * (tmin + tmax) < sketch_proj;
                                    if tool_rev != rev {
                                        chain.notes.push(format!(
                                            "{name}: cut auto-reversal decided on the merged body (reverse={rev}); if the previous merge's solids never met, the engine measures the tool body alone and decides reverse={tool_rev} — verify with FE_CUT_TRACE=1"
                                        ));
                                    }
                                }
                                if let Some(comps) =
                                    extent.as_ref().and_then(|e| e.components.as_deref())
                                {
                                    let all_rev_true = comps.iter().all(|c| {
                                        c.occupied.is_some_and(|(occ_lo_c, _)| {
                                            let c_mid_hi = 0.5 * (occ_lo_c + c.outer_proj.1);
                                            let c_pad = 1e-9
                                                * (c.outer_proj.1 - c.outer_proj.0)
                                                    .max(f64::MIN_POSITIVE);
                                            sketch_proj > c_mid_hi + c_pad
                                        })
                                    });
                                    let all_rev_false = comps.iter().all(|c| {
                                        c.occupied.is_some_and(|(_, occ_hi_c)| {
                                            let c_mid_lo = 0.5 * (c.outer_proj.0 + occ_hi_c);
                                            let c_pad = 1e-9
                                                * (c.outer_proj.1 - c.outer_proj.0)
                                                    .max(f64::MIN_POSITIVE);
                                            sketch_proj < c_mid_lo - c_pad
                                        })
                                    });
                                    if all_rev_true {
                                        rev = true;
                                    } else if all_rev_false {
                                        rev = false;
                                    } else if ext.occupied.is_some() {
                                        multi_comp_disagree_why = Some(format!(
                                            "{name}: primary cut target has {} candidate component(s) that do not unanimously agree on cut auto-reversal at sketch_proj = {sketch_proj:.6e}",
                                            comps.len()
                                        ));
                                    }
                                } else {
                                    let span = (outer_hi - outer_lo).max(f64::MIN_POSITIVE);
                                    let fp_pad = 1e-9 * span;
                                    if ext.occupied.is_some()
                                        && mid_lo - fp_pad <= sketch_proj
                                        && sketch_proj <= mid_hi + fp_pad
                                    {
                                        chain.indeterminate.push(format!(
                                            "{name}: sketch plane ({sketch_proj:.6e}) lies inside the target's possible mid-extent interval [{mid_lo:.6e}, {mid_hi:.6e}] — cut auto-reversal is indeterminate"
                                        ));
                                    }
                                }
                                rev
                            }
                            None => true,
                        }
                    } else {
                        false
                    };

                    let build_extrude_for_rev = |rev_cand: bool,
                                                 filter_compat: bool|
                     -> Result<
                        (ExactSolid, Vec<String>, Option<f64>),
                        NotCovered,
                    > {
                        let sweep = if rev_cand {
                            scale(dir_unit, -1.0)
                        } else {
                            dir_unit
                        };
                        let mut inds: Vec<String> = Vec::new();
                        let mut resolved_thru: Option<f64> = None;
                        let primary = match mode {
                            "Blind" => blind,
                            "ThroughAll" => {
                                // Through-all is the far end of the measured
                                // material along the SWEEP direction plus a
                                // RELATIVE overshoot (engine:
                                // `THROUGH_ALL_OVERSHOOT`, 1e-2 of the material's
                                // own extent along the axis — never an absolute
                                // margin; assay P0012). `ext.outer_proj` is a
                                // mathematically sound outer bound on the target
                                // (unlike `far_occupied_sample + h`, which can miss
                                // a sub-`h` star spike or gear tooth by more than
                                // `h`): for a cut, sweeping to that outer bound is
                                // guaranteed to clear every tip of the body.
                                let d = match extent.as_ref().and_then(|e| e.primary) {
                                        Some(ext) => {
                                            let plane = dot(sk.basis.origin, sweep);
                                            let target_exprs = extent
                                                .as_ref()
                                                .map(|e| e.target_exprs.as_slice())
                                                .unwrap_or(&[]);
                                            let is_sym = params
                                                .get("symmetric")
                                                .and_then(Value::as_bool)
                                                == Some(true)
                                                || params
                                                    .pointer("/second_direction/type")
                                                    .and_then(Value::as_str)
                                                    == Some("Symmetric");
                                            let check_thru_slabs =
                                                |d_lo: f64, d_hi: f64, check_fwd: bool| -> bool {
                                                    if d_lo >= d_hi {
                                                        return true;
                                                    }
                                                    let mut ok = if check_fwd {
                                                        let fwd_slab = ExactSolid::Extrude {
                                                            basis: sk.basis,
                                                            profile: profile.clone(),
                                                            face_origin: add(
                                                                sk.basis.origin,
                                                                scale(sweep, d_lo),
                                                            ),
                                                            dir: sweep,
                                                            depth: d_hi - d_lo,
                                                        };
                                                        target_exprs.iter().all(|te| {
                                                            chain.slab_must_miss_expr(&fwd_slab, te)
                                                        })
                                                    } else {
                                                        true
                                                    };
                                                    if ok && is_sym {
                                                        let back_slab = ExactSolid::Extrude {
                                                            basis: sk.basis,
                                                            profile: profile.clone(),
                                                            face_origin: sub(
                                                                sk.basis.origin,
                                                                scale(sweep, d_hi),
                                                            ),
                                                            dir: sweep,
                                                            depth: d_hi - d_lo,
                                                        };
                                                        ok = target_exprs.iter().all(|te| {
                                                            chain.slab_must_miss_expr(
                                                                &back_slab, te,
                                                            )
                                                        });
                                                    }
                                                    ok
                                                };
                                            if let Some(all_comps) = extent
                                                .as_ref()
                                                .and_then(|e| e.components.as_deref())
                                            {
                                                let compat: Vec<&SampledExtent> = all_comps
                                                    .iter()
                                                    .filter(|c| {
                                                        if !filter_compat {
                                                            return true;
                                                        }
                                                        match c.occupied {
                                                            Some((occ_lo_c, occ_hi_c)) => {
                                                                let c_pad = 1e-9
                                                                    * (c.outer_proj.1
                                                                        - c.outer_proj.0)
                                                                        .max(f64::MIN_POSITIVE);
                                                                if rev_cand {
                                                                    let c_mid_lo = 0.5
                                                                        * (c.outer_proj.0
                                                                            + occ_hi_c);
                                                                    c_mid_lo - c_pad <= sketch_proj
                                                                } else {
                                                                    let c_mid_hi = 0.5
                                                                        * (occ_lo_c
                                                                            + c.outer_proj.1);
                                                                    sketch_proj <= c_mid_hi + c_pad
                                                                }
                                                            }
                                                            None => true,
                                                        }
                                                    })
                                                    .collect();
                                                let (s_outer_lo, s_outer_hi) = if compat.is_empty()
                                                {
                                                    if rev_cand {
                                                        (-ext.outer_proj.1, -ext.outer_proj.0)
                                                    } else {
                                                        ext.outer_proj
                                                    }
                                                } else {
                                                    let mut slo = f64::INFINITY;
                                                    let mut shi = f64::NEG_INFINITY;
                                                    for c in &compat {
                                                        let (a, b) = if rev_cand {
                                                            (-c.outer_proj.1, -c.outer_proj.0)
                                                        } else {
                                                            c.outer_proj
                                                        };
                                                        slo = slo.min(a);
                                                        shi = shi.max(b);
                                                    }
                                                    (slo, shi)
                                                };
                                                let (blo, bhi) =
                                                    (s_outer_lo - plane, s_outer_hi - plane);
                                                if bhi <= 0.0 {
                                                    return Err(nc(
                                                        index,
                                                        &name,
                                                        "ThroughAll with the measured material entirely \
                                                         behind the sweep — the engine reports \
                                                         ResolutionFailed",
                                                    ));
                                                }
                                                let d_val = bhi
                                                    + (bhi - blo).max(bhi.abs())
                                                        * THROUGH_ALL_OVERSHOOT;
                                                let mut all_ahead = !compat.is_empty();
                                                let mut min_d_lo = f64::INFINITY;
                                                for c in &compat {
                                                    if let Some((w_min, w_max)) = c.occupied {
                                                        let (s_lo, s_hi) = if rev_cand {
                                                            (-w_max, -w_min)
                                                        } else {
                                                            (w_min, w_max)
                                                        };
                                                        if s_hi <= plane {
                                                            all_ahead = false;
                                                            break;
                                                        }
                                                        let bhi_c = s_hi - plane;
                                                        let span_c =
                                                            (s_hi - s_lo).max(bhi_c.abs());
                                                        let d_lo_c = bhi_c
                                                            + span_c * THROUGH_ALL_OVERSHOOT;
                                                        min_d_lo = min_d_lo.min(d_lo_c);
                                                    } else {
                                                        all_ahead = false;
                                                        break;
                                                    }
                                                }
                                                if !all_ahead {
                                                    inds.push(format!(
                                                        "{name}: ThroughAll measured body has {} candidate component(s), not all proven ahead of the sketch plane ({plane:.6e}) — OutputKey::Main selection is indeterminate",
                                                        compat.len()
                                                    ));
                                                } else if !is_cut {
                                                    if (d_val - min_d_lo) > ext.uncertainty {
                                                        inds.push(format!(
                                                            "{name}: boss ThroughAll measured body has {} candidate component(s) with differing sweep depths [{min_d_lo:.6e}, {d_val:.6e}]",
                                                            compat.len()
                                                        ));
                                                    }
                                                } else if !check_thru_slabs(min_d_lo, d_val, true) {
                                                    inds.push(format!(
                                                        "{name}: ThroughAll cut target has {} candidate component(s) whose sweep depths [{min_d_lo:.6e}, {d_val:.6e}] cross target material — OutputKey::Main selection is indeterminate",
                                                        compat.len()
                                                    ));
                                                }
                                                d_val
                                            } else {
                                                let (s_outer_lo, s_outer_hi) = if rev_cand {
                                                    (-ext.outer_proj.1, -ext.outer_proj.0)
                                                } else {
                                                    ext.outer_proj
                                                };
                                                let (blo, bhi) =
                                                    (s_outer_lo - plane, s_outer_hi - plane);
                                                if bhi <= 0.0 {
                                                    return Err(nc(
                                                        index,
                                                        &name,
                                                        "ThroughAll with the measured material entirely \
                                                         behind the sweep — the engine reports \
                                                         ResolutionFailed",
                                                    ));
                                                }
                                                let d_val = bhi
                                                    + (bhi - blo).max(bhi.abs())
                                                        * THROUGH_ALL_OVERSHOOT;
                                                match ext.occupied {
                                                    Some((w_min, w_max)) => {
                                                        let (s_occ_lo, s_occ_hi) = if rev_cand {
                                                            (-w_max, -w_min)
                                                        } else {
                                                            (w_min, w_max)
                                                        };
                                                        if s_occ_hi - plane <= 0.0
                                                            && s_outer_hi - plane > 0.0
                                                        {
                                                            inds.push(format!(
                                                                "{name}: ThroughAll outer bound ({s_outer_hi:.6e}) crosses the sketch plane ({plane:.6e}) while occupied samples end at {s_occ_hi:.6e} — sweep validity is indeterminate"
                                                            ));
                                                        } else {
                                                            if !is_cut
                                                                && ((s_outer_hi - s_occ_hi)
                                                                    > ext.uncertainty
                                                                    || (s_occ_lo - s_outer_lo)
                                                                        > ext.uncertainty)
                                                            {
                                                                inds.push(format!(
                                                                    "{name}: boss ThroughAll extent gap [{s_occ_hi:.6e}, {s_outer_hi:.6e}] exceeds lattice step h = {:.3e}",
                                                                    ext.uncertainty
                                                                ));
                                                            }
                                                            if (is_cut
                                                                || combine == Combine::Intersect)
                                                                && s_occ_hi - plane > 0.0
                                                                && (target_exprs.len() > 1
                                                                    || is_sym)
                                                            {
                                                                let bhi_occ = s_occ_hi - plane;
                                                                let span_occ = (s_occ_hi
                                                                    - s_occ_lo)
                                                                    .max(bhi_occ.abs());
                                                                let d_occ = bhi_occ
                                                                    + span_occ
                                                                        * THROUGH_ALL_OVERSHOOT;
                                                                if !check_thru_slabs(
                                                                    d_occ,
                                                                    d_val,
                                                                    target_exprs.len() > 1,
                                                                ) {
                                                                    inds.push(format!(
                                                                        "{name}: ThroughAll target extent uncertainty [{s_occ_hi:.6e}, {s_outer_hi:.6e}] induces sweep depth band [{d_occ:.6e}, {d_val:.6e}] that crosses target material"
                                                                    ));
                                                                }
                                                            }
                                                        }
                                                    }
                                                    None if s_outer_hi - plane > 0.0 => {
                                                        inds.push(format!(
                                                            "{name}: primary target has no occupied lattice samples at {EXACT_VOLUME_CELLS} cells (h = {:.3e}) — ThroughAll depth is indeterminate",
                                                            ext.uncertainty
                                                        ));
                                                    }
                                                    None => {}
                                                }
                                                d_val
                                            }
                                        }
                                        None if blind > 0.0 => blind,
                                        None => return Err(nc(
                                            index,
                                            &name,
                                            "ThroughAll with no measured body and no positive authored \
                                                 depth — the engine reports ResolutionFailed",
                                        )),
                                    };
                                resolved_thru = Some(d);
                                d
                            }
                            other => return Err(nc(index, &name, &format!("depth_mode {other}"))),
                        };
                        let second = match params.get("second_direction") {
                            Some(sd) if !sd.is_null() => {
                                match sd.get("type").and_then(Value::as_str).unwrap_or("") {
                                    "Blind" => {
                                        Some(sd.get("depth").and_then(Value::as_f64).unwrap_or(0.0))
                                    }
                                    "Symmetric" => Some(primary),
                                    other => {
                                        return Err(nc(
                                            index,
                                            &name,
                                            &format!("second_direction {other}"),
                                        ))
                                    }
                                }
                            }
                            _ => {
                                if params.get("symmetric").and_then(Value::as_bool) == Some(true) {
                                    Some(primary)
                                } else {
                                    None
                                }
                            }
                        };
                        let (dir_final, depth, face_origin) = match (is_cut, second) {
                            (true, Some(sd)) => {
                                if rev_cand {
                                    (
                                        scale(direction, -1.0),
                                        primary + sd,
                                        add(sk.basis.origin, scale(direction, sd)),
                                    )
                                } else {
                                    (
                                        direction,
                                        primary + sd,
                                        sub(sk.basis.origin, scale(direction, sd)),
                                    )
                                }
                            }
                            (true, None) => {
                                if rev_cand {
                                    (scale(direction, -1.0), primary, sk.basis.origin)
                                } else {
                                    (direction, primary, sk.basis.origin)
                                }
                            }
                            (false, Some(sd)) => (
                                direction,
                                primary + sd,
                                sub(sk.basis.origin, scale(direction, sd)),
                            ),
                            (false, None) => (direction, primary, sk.basis.origin),
                        };
                        let dir_final_unit = norm(dir_final).unwrap();
                        Ok((
                            ExactSolid::Extrude {
                                basis: sk.basis,
                                profile: profile.clone(),
                                face_origin,
                                dir: dir_final_unit,
                                depth,
                            },
                            inds,
                            resolved_thru,
                        ))
                    };

                    let filter_compat = is_cut && explicit_dir.is_none();
                    let (solid_final, alt_cut) = if let Some(why) = multi_comp_disagree_why {
                        let res_p = build_extrude_for_rev(reverse, true);
                        let res_a = build_extrude_for_rev(!reverse, true);
                        match (res_p, res_a) {
                            (Ok((solid_p, inds_p, thru_p)), Ok((solid_a, inds_a, _)))
                                if inds_p.is_empty() && inds_a.is_empty() =>
                            {
                                if let Some(d) = thru_p {
                                    chain.notes.push(format!(
                                        "{name}: ThroughAll depth resolved to {d:.6}"
                                    ));
                                }
                                if reverse {
                                    chain.notes.push(format!("{name}: cut auto-reversed"));
                                }
                                (
                                    solid_p,
                                    Some(AltCut {
                                        solid: solid_a,
                                        primary_rev: reverse,
                                        dir_unit,
                                        sketch_proj,
                                        why,
                                    }),
                                )
                            }
                            (res_p, res_a) => {
                                chain.indeterminate.push(why);
                                let (solid_fallback, inds_fb, thru_fb) = match (res_p, res_a) {
                                    (Ok(t), _) => t,
                                    (Err(_), Ok(t)) => t,
                                    (Err(e), Err(_)) => return Err(e),
                                };
                                if let Some(d) = thru_fb {
                                    chain.notes.push(format!(
                                        "{name}: ThroughAll depth resolved to {d:.6}"
                                    ));
                                }
                                chain.indeterminate.extend(inds_fb);
                                if reverse {
                                    chain.notes.push(format!("{name}: cut auto-reversed"));
                                }
                                (solid_fallback, None)
                            }
                        }
                    } else {
                        let (solid_p, inds_p, thru_p) =
                            build_extrude_for_rev(reverse, filter_compat)?;
                        if let Some(d) = thru_p {
                            chain
                                .notes
                                .push(format!("{name}: ThroughAll depth resolved to {d:.6}"));
                        }
                        chain.indeterminate.extend(inds_p);
                        if reverse {
                            chain.notes.push(format!("{name}: cut auto-reversed"));
                        }
                        (solid_p, None)
                    };
                    chain.ops.push(ExactOp {
                        name,
                        feature_id,
                        solid: solid_final,
                        alt_cut,
                        cut: is_cut,
                        combine,
                        targets,
                    });
                }
                "Revolve" => {
                    let params = op
                        .get("params")
                        .ok_or_else(|| nc(index, &name, "no params"))?;
                    let sk = sketch_for(&sketches, params).map_err(|w| nc(index, &name, &w))?;
                    let profile = sk.profile(params).map_err(|w| nc(index, &name, &w))?;
                    let (combine, targets) =
                        parse_combine(params).map_err(|w| nc(index, &name, &w))?;
                    let is_cut = combine == Combine::Cut;
                    if !chain.has_target_body(combine, &targets)
                        && matches!(combine, Combine::Cut | Combine::Intersect)
                    {
                        chain.notes.push(format!(
                            "{name}: {combine:?} with no target body — the engine reports ResolutionFailed"
                        ));
                    }
                    let axis_origin = params
                        .get("axis_origin")
                        .and_then(vec3)
                        .ok_or_else(|| nc(index, &name, "no axis_origin"))?;
                    let axis = params
                        .get("axis_direction")
                        .and_then(vec3)
                        .and_then(norm)
                        .ok_or_else(|| nc(index, &name, "no axis_direction"))?;
                    let angle_deg = params.get("angle").and_then(Value::as_f64).unwrap_or(0.0);
                    if dot(axis, sk.basis.n).abs() > 1e-6 {
                        return Err(nc(index, &name, "revolve axis not in the sketch plane"));
                    }
                    if dot(sub(axis_origin, sk.basis.origin), sk.basis.n).abs()
                        > 1e-6 * (1.0 + axis_origin.iter().map(|c| c.abs()).fold(0.0, f64::max))
                    {
                        return Err(nc(index, &name, "revolve axis origin off the sketch plane"));
                    }
                    // ŵ = ±(n̂ × â) toward the profile (sign from the profile's
                    // bbox centre — the engine sums the vertices' radial
                    // coordinates; a valid profile lies on one side).
                    let mut w_hat = norm(cross(sk.basis.n, axis))
                        .ok_or_else(|| nc(index, &name, "degenerate axis"))?;
                    let (u0, u1, v0, v1) = profile.bbox();
                    let centre = sk.basis.embed(0.5 * (u0 + u1), 0.5 * (v0 + v1));
                    if dot(sub(centre, axis_origin), w_hat) < 0.0 {
                        w_hat = scale(w_hat, -1.0);
                    }
                    let m_hat = cross(axis, w_hat);
                    let full_turn = (angle_deg - 360.0).abs() <= 1e-7;
                    chain.ops.push(ExactOp {
                        name,
                        feature_id,
                        solid: ExactSolid::Revolve {
                            basis: sk.basis,
                            profile,
                            axis_origin,
                            axis,
                            w_hat,
                            m_hat,
                            angle: angle_deg.to_radians(),
                            full_turn,
                        },
                        alt_cut: None,
                        cut: is_cut,
                        combine,
                        targets,
                    });
                }
                // An explicit combine of two bodies adds no operand to the
                // exact chain — every boss, NewBody or merged, is already an
                // operand — but it re-authors the BODY LIST: a Union (P0007,
                // 2026-09-29: the prospector's `convex4:∪` step unions a
                // NewBody boss into the main body) folds two bodies into one
                // set union; a Subtract / Intersect (2026-10-13, seed 5
                // index 39: `nonconvex5:∩`) composes `a − b` / `a ∩ b` of the
                // two anchored bodies' expressions.
                "BooleanCombine" => {
                    let kind = match op
                        .pointer("/params/operation/type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                    {
                        "Union" => MergeKind::Union,
                        "Subtract" => MergeKind::Subtract,
                        "Intersect" => MergeKind::Intersect,
                        other => {
                            return Err(nc(
                                index,
                                &name,
                                &format!("operation BooleanCombine ({other:?})"),
                            ))
                        }
                    };
                    let anchor_of = |key: &str| -> Result<OutputAnchor, NotCovered> {
                        let gr = op
                            .pointer(&format!("/params/{key}"))
                            .ok_or_else(|| nc(index, &name, &format!("combine without {key}")))?;
                        parse_output_anchor(gr)
                            .map_err(|why| nc(index, &name, &format!("{key} {why}")))
                    };
                    let anchors = vec![anchor_of("body_a")?, anchor_of("body_b")?];
                    chain.merges.push(BodyMerge {
                        after_op: chain.ops.len(),
                        feature_id,
                        name,
                        anchors,
                        kind,
                    });
                }
                other => return Err(nc(index, &name, &format!("operation {other}"))),
            }
        }
        if chain.ops.is_empty() {
            return Err(NotCovered::Shape("no solid-bearing operation".into()));
        }
        let res = chain.worlds_after(chain.ops.len());
        if let Some(ind) = res.worlds.iter().find_map(|w| w.indeterminate.clone()) {
            chain.indeterminate.push(ind);
        } else if res.worlds.iter().any(|w| w.consumer_after_ambiguous) {
            if let Some(why) = res.cap_exceeded {
                chain.indeterminate.push(why);
            } else if res.worlds.len() > 1 {
                if let Some(why) = chain.check_worlds_volume_agreement(&res.worlds) {
                    chain.indeterminate.push(why);
                }
            }
        }
        Ok(chain)
    }
}

fn nc(index: usize, name: &str, why: &str) -> NotCovered {
    NotCovered::Feature {
        index,
        name: name.to_string(),
        why: why.to_string(),
    }
}

#[derive(Clone)]
struct CustodyWorld {
    bodies: Vec<Body>,
    assumed_disjoint: Vec<(Expr, Expr)>,
    indeterminate: Option<String>,
    ambiguity_why: Option<String>,
    had_ambiguous: bool,
    consumer_after_ambiguous: bool,
}

struct WorldsResult {
    worlds: Vec<CustodyWorld>,
    cap_exceeded: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TouchVerdict {
    Touches,
    Disjoint,
    Ambiguous { h: f64, why: &'static str },
}

fn expr_has_sub_or_inter(e: &Expr) -> bool {
    match e {
        Expr::Leaf(_) | Expr::AltLeaf(_) => false,
        Expr::Union(es) => es.iter().any(expr_has_sub_or_inter),
        Expr::Sub(_, _) | Expr::Inter(_, _) | Expr::Lump { .. } => true,
    }
}

fn expr_needs_lump_split(e: &Expr) -> bool {
    match e {
        Expr::Leaf(_) | Expr::AltLeaf(_) | Expr::Lump { .. } => false,
        Expr::Union(es) => es.iter().any(expr_needs_lump_split),
        Expr::Sub(_, _) | Expr::Inter(_, _) => true,
    }
}

/// Sampled axial extent of an expression along a unit direction `dir_unit`,
/// evaluated from [`ExactChain::expr_contains`] on an [`EXACT_VOLUME_CELLS`]
/// lattice laid in an orthonormal frame aligned with `dir_unit`, paired with a
/// mathematically sound outer projection bound `outer_proj`.
#[derive(Debug, Clone, Copy)]
struct SampledExtent {
    /// Guaranteed outer bound `(outer_lo, outer_hi)` on the expression's true
    /// projection `[true_lo, true_hi]` along `dir_unit` (`outer_lo <= true_lo`
    /// and `true_hi <= outer_hi`, even for sub-`h` star spikes or gear teeth).
    outer_proj: (f64, f64),
    /// Extreme occupied sample coordinates `(occ_lo, occ_hi)` along `dir_unit`
    /// (`None` if zero lattice samples fell inside the expression).
    occupied: Option<(f64, f64)>,
    /// Isotropic lattice cell spacing `h`.
    uncertainty: f64,
}

/// The extent a feature measures for its sweep direction and its through-all
/// depth: the engine reads its PRIMARY target — its first combine target,
/// falling back to the most recent solid when it has none (`rebuild.rs`:
/// `combine_targets.first()` / `find_most_recent_solid`). With a legacy target
/// set that is the most recent feature's outputs, the merged body (or the tool
/// body, when the merge left its targets disjoint) comes first.
#[derive(Debug, Clone)]
struct TargetExtent {
    /// Sampled extent of the primary target body (`None`: no target).
    primary: Option<SampledExtent>,
    /// When the first target is a fold, the sampled extent of the fold's TOOL
    /// part alone — what the engine measures if that fold's solids never met.
    tool_alone: Option<SampledExtent>,
    /// Candidate per-lump / per-world extents along `dir_unit` when
    /// `OutputKey::Main` or fold/split hypotheses could pick any of `>= 2`
    /// target extents. `None` when one extent uniquely applies.
    components: Option<Vec<SampledExtent>>,
    /// Expressions of all bodies targeted across candidate hypotheses.
    target_exprs: Vec<Expr>,
    /// Set when candidate fold/split worlds exceed the hypothesis cap or
    /// disagree on whether a target body exists at all.
    world_indeterminate: Option<String>,
}

impl ExactChain {
    fn has_target_body(&self, combine: Combine, targets: &Targets) -> bool {
        let bodies = self.bodies_after(self.ops.len());
        let idx = self.target_indices(&bodies, combine, targets);
        idx.first()
            .and_then(|&first| self.expr_bbox(&bodies[first].expr))
            .is_some()
    }

    fn target_extent_for_bodies(
        &self,
        bodies: &[Body],
        combine: Combine,
        targets: &Targets,
        dir_unit: [f64; 3],
    ) -> TargetExtent {
        let idx = if combine == Combine::NewBody {
            self.target_indices(bodies, Combine::Add, &Targets::MostRecent)
        } else {
            self.target_indices(bodies, combine, targets)
        };
        let Some(&first) = idx.first() else {
            return TargetExtent {
                primary: None,
                tool_alone: None,
                components: None,
                target_exprs: Vec::new(),
                world_indeterminate: None,
            };
        };
        let target_exprs = idx.iter().map(|&j| bodies[j].expr.clone()).collect();
        let expr = &bodies[first].expr;
        let tool_alone = match expr {
            Expr::Union(parts) => parts
                .first()
                .and_then(|p| self.expr_extent_along(p, dir_unit, EXACT_VOLUME_CELLS)),
            _ => None,
        };
        let candidate_indices: Vec<usize> = if bodies[first].arbitrary_output_order {
            match targets {
                Targets::Explicit(anchors) if !anchors.is_empty() => {
                    let fid = &anchors[0].feature_id;
                    bodies
                        .iter()
                        .enumerate()
                        .filter(|(_, b)| b.outputs.iter().any(|o| &o.feature_id == fid))
                        .map(|(j, _)| j)
                        .collect()
                }
                _ => idx
                    .iter()
                    .copied()
                    .filter(|&j| bodies[j].arbitrary_output_order)
                    .collect(),
            }
        } else {
            vec![first]
        };
        if candidate_indices.len() == 1 && !expr_needs_lump_split(expr) {
            return TargetExtent {
                primary: self.expr_extent_along(expr, dir_unit, EXACT_VOLUME_CELLS),
                tool_alone,
                components: None,
                target_exprs,
                world_indeterminate: None,
            };
        }
        let mut primary: Option<SampledExtent> = None;
        let mut all_comps: Vec<SampledExtent> = Vec::new();
        for &ci in &candidate_indices {
            let cexpr = &bodies[ci].expr;
            if let Some(ext) = self.expr_extent_along(cexpr, dir_unit, EXACT_VOLUME_CELLS) {
                if ci == first {
                    primary = Some(ext);
                }
                if expr_needs_lump_split(cexpr) {
                    match self.partition_lumps(cexpr, EXACT_VOLUME_CELLS) {
                        LumpPartitionOutcome::Empty | LumpPartitionOutcome::Single => {
                            all_comps.push(ext);
                        }
                        LumpPartitionOutcome::Multi(part) => {
                            for c in 0..part.0.num_lumps {
                                let lexpr = Expr::Lump {
                                    base: Box::new(cexpr.clone()),
                                    partition: part.clone(),
                                    lump: c as u16,
                                };
                                if let Some(lext) =
                                    self.expr_extent_along(&lexpr, dir_unit, EXACT_VOLUME_CELLS)
                                {
                                    all_comps.push(lext);
                                } else {
                                    all_comps.push(ext);
                                }
                            }
                        }
                    }
                } else {
                    all_comps.push(ext);
                }
            }
        }
        if all_comps.len() > 1 {
            if let Some(ref mut p) = primary {
                for c in &all_comps {
                    p.outer_proj.0 = p.outer_proj.0.min(c.outer_proj.0);
                    p.outer_proj.1 = p.outer_proj.1.max(c.outer_proj.1);
                    if let Some((clo, chi)) = c.occupied {
                        p.occupied = Some(match p.occupied {
                            Some((plo, phi)) => (plo.min(clo), phi.max(chi)),
                            None => (clo, chi),
                        });
                    }
                    p.uncertainty = p.uncertainty.max(c.uncertainty);
                }
            }
        }
        TargetExtent {
            primary,
            tool_alone,
            components: (all_comps.len() > 1).then_some(all_comps),
            target_exprs,
            world_indeterminate: None,
        }
    }

    fn target_extent(
        &self,
        combine: Combine,
        targets: &Targets,
        dir_unit: [f64; 3],
    ) -> TargetExtent {
        let res = self.worlds_after(self.ops.len());
        if res.worlds.len() <= 1 {
            let bodies = res
                .worlds
                .first()
                .map(|w| w.bodies.as_slice())
                .unwrap_or(&[]);
            return self.target_extent_for_bodies(bodies, combine, targets, dir_unit);
        }
        let mut base =
            self.target_extent_for_bodies(&res.worlds[0].bodies, combine, targets, dir_unit);
        if let Some(why) = res.cap_exceeded {
            base.world_indeterminate = Some(why);
            return base;
        }
        let mut any_some = base.primary.is_some();
        let mut any_none = base.primary.is_none();
        let mut all_comps: Vec<SampledExtent> = match base.components.take() {
            Some(cs) => cs,
            None => base.primary.into_iter().collect(),
        };
        for w in &res.worlds[1..] {
            let te = self.target_extent_for_bodies(&w.bodies, combine, targets, dir_unit);
            any_some |= te.primary.is_some();
            any_none |= te.primary.is_none();
            for e in te.target_exprs {
                if !base.target_exprs.contains(&e) {
                    base.target_exprs.push(e);
                }
            }
            let w_comps: Vec<SampledExtent> = match te.components {
                Some(cs) => cs,
                None => te.primary.into_iter().collect(),
            };
            for wc in w_comps {
                let dup = all_comps.iter().any(|ec| {
                    ec.outer_proj == wc.outer_proj
                        && ec.occupied == wc.occupied
                        && ec.uncertainty == wc.uncertainty
                });
                if !dup {
                    all_comps.push(wc);
                }
            }
        }
        if any_some && any_none {
            base.world_indeterminate =
                Some("target existence differs across ambiguous fold/split hypotheses".into());
        }
        if all_comps.len() > 1 {
            if let Some(ref mut p) = base.primary {
                for c in &all_comps {
                    p.outer_proj.0 = p.outer_proj.0.min(c.outer_proj.0);
                    p.outer_proj.1 = p.outer_proj.1.max(c.outer_proj.1);
                    if let Some((clo, chi)) = c.occupied {
                        p.occupied = Some(match p.occupied {
                            Some((plo, phi)) => (plo.min(clo), phi.max(chi)),
                            None => (clo, chi),
                        });
                    }
                    p.uncertainty = p.uncertainty.max(c.uncertainty);
                }
            }
            base.components = Some(all_comps);
        }
        base
    }

    /// True only when `slab` and `expr` are provably disjoint via analytic
    /// projection / sub-box pruning.
    fn slab_must_miss_expr(&self, slab: &ExactSolid, expr: &Expr) -> bool {
        let Some((elo, ehi)) = self.expr_bbox(expr) else {
            return true;
        };
        let (slo, shi) = slab.bbox();
        for a in 0..3 {
            if slo[a] > ehi[a] || elo[a] > shi[a] {
                return true;
            }
        }
        if let ExactSolid::Extrude { dir, .. } = slab {
            let (sd0, sd1) = solid_proj_bound(slab, *dir);
            let Some((ed0, ed1)) = self.expr_proj_bound(expr, *dir) else {
                return true;
            };
            if sd1 < ed0 || ed1 < sd0 {
                return true;
            }
        }
        let frame = match slab {
            ExactSolid::Extrude { basis, .. } | ExactSolid::Revolve { basis, .. } => *basis,
        };
        let bounds_in_frame = |wlo: [f64; 3], whi: [f64; 3]| -> Box3 {
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for c in 0..8 {
                let p = [
                    if c & 1 == 0 { wlo[0] } else { whi[0] },
                    if c & 2 == 0 { wlo[1] } else { whi[1] },
                    if c & 4 == 0 { wlo[2] } else { whi[2] },
                ];
                let r = sub(p, frame.origin);
                let q = [dot(r, frame.x), dot(r, frame.y), dot(r, frame.n)];
                for a in 0..3 {
                    lo[a] = lo[a].min(q[a]);
                    hi[a] = hi[a].max(q[a]);
                }
            }
            (lo, hi)
        };
        let (mut sa, mut sb) = bounds_in_frame(slo, shi);
        let (mut ea, mut eb) = bounds_in_frame(elo, ehi);
        for (a, ax) in [(0, frame.x), (1, frame.y), (2, frame.n)] {
            let orig_a = dot(frame.origin, ax);
            let (sp0, sp1) = solid_proj_bound(slab, ax);
            let Some((ep0, ep1)) = self.expr_proj_bound(expr, ax) else {
                return true;
            };
            sa[a] = sa[a].max(sp0 - orig_a);
            sb[a] = sb[a].min(sp1 - orig_a);
            ea[a] = ea[a].max(ep0 - orig_a);
            eb[a] = eb[a].min(ep1 - orig_a);
        }
        let mut ilo = [0.0; 3];
        let mut ihi = [0.0; 3];
        for a in 0..3 {
            ilo[a] = sa[a].max(ea[a]);
            ihi[a] = sb[a].min(eb[a]);
            if ilo[a] > ihi[a] {
                return true;
            }
        }
        let root_corners = frame_box_corners(&frame, ilo, ihi);
        if solid_must_miss(slab, &root_corners) || self.box_must_miss(expr, &root_corners) {
            return true;
        }
        const SPLIT_BUDGET: usize = 256;
        let mut queue: VecDeque<Box3> = VecDeque::new();
        queue.push_back((ilo, ihi));
        let mut splits = 0usize;
        while let Some((lo, hi)) = queue.pop_front() {
            if splits >= SPLIT_BUDGET {
                return false;
            }
            splits += 1;
            let (c0, c1) = split_longest_axis(lo, hi);
            for (clo, chi) in [c0, c1] {
                let corners = frame_box_corners(&frame, clo, chi);
                if !(solid_must_miss(slab, &corners) || self.box_must_miss(expr, &corners)) {
                    queue.push_back((clo, chi));
                }
            }
        }
        true
    }

    /// True when at least one voxel centre of `expr` on an isotropic `cells`
    /// lattice in `self.frame` is occupied.
    fn expr_has_occupied_sample(&self, expr: &Expr, cells: usize) -> bool {
        let Some((blo, bhi)) = self.expr_bbox(expr) else {
            return false;
        };
        let world_corners = |lo: [f64; 3], hi: [f64; 3]| -> [[f64; 3]; 8] {
            let mut cs = [[0.0; 3]; 8];
            for (c, slot) in cs.iter_mut().enumerate() {
                *slot = [
                    if c & 1 == 0 { lo[0] } else { hi[0] },
                    if c & 2 == 0 { lo[1] } else { hi[1] },
                    if c & 4 == 0 { lo[2] } else { hi[2] },
                ];
            }
            cs
        };
        if self.box_must_miss(expr, &world_corners(blo, bhi)) {
            return false;
        }
        {
            const EMPTY_BUDGET: usize = 64;
            let mut q: VecDeque<Box3> = VecDeque::new();
            q.push_back((blo, bhi));
            let mut splits = 0usize;
            let mut non_empty_candidate = false;
            while let Some((lo, hi)) = q.pop_front() {
                let mid = [
                    0.5 * (lo[0] + hi[0]),
                    0.5 * (lo[1] + hi[1]),
                    0.5 * (lo[2] + hi[2]),
                ];
                if self.expr_contains(expr, mid) {
                    return true;
                }
                if splits >= EMPTY_BUDGET {
                    non_empty_candidate = true;
                    break;
                }
                splits += 1;
                let (c0, c1) = split_longest_axis(lo, hi);
                for (clo, chi) in [c0, c1] {
                    if !self.box_must_miss(expr, &world_corners(clo, chi)) {
                        q.push_back((clo, chi));
                    }
                }
            }
            if !non_empty_candidate && q.is_empty() {
                return false;
            }
        }
        let f = self.frame;
        let mut flo = [f64::INFINITY; 3];
        let mut fhi = [f64::NEG_INFINITY; 3];
        for p in world_corners(blo, bhi) {
            let r = sub(p, f.origin);
            let coords = [dot(r, f.x), dot(r, f.y), dot(r, f.n)];
            for a in 0..3 {
                flo[a] = flo[a].min(coords[a]);
                fhi[a] = fhi[a].max(coords[a]);
            }
        }
        let ext = [fhi[0] - flo[0], fhi[1] - flo[1], fhi[2] - flo[2]];
        let longest = ext[0].max(ext[1]).max(ext[2]);
        if !longest.is_finite() || longest <= 0.0 {
            return false;
        }
        let h = longest / cells as f64;
        let n = [
            ((ext[0] / h).ceil() as usize + 2).max(1),
            ((ext[1] / h).ceil() as usize + 2).max(1),
            ((ext[2] / h).ceil() as usize + 2).max(1),
        ];
        let start = [flo[0] - h, flo[1] - h, flo[2] - h];
        let in_wbox = |p: [f64; 3]| -> bool {
            p[0] >= blo[0]
                && p[0] <= bhi[0]
                && p[1] >= blo[1]
                && p[1] <= bhi[1]
                && p[2] >= blo[2]
                && p[2] <= bhi[2]
        };
        for k in (0..n[2]).step_by(8) {
            let bw = add(f.origin, scale(f.n, start[2] + (k as f64 + 0.5) * h));
            for j in (0..n[1]).step_by(8) {
                let bvw = add(bw, scale(f.y, start[1] + (j as f64 + 0.5) * h));
                for i in (0..n[0]).step_by(8) {
                    let p = add(bvw, scale(f.x, start[0] + (i as f64 + 0.5) * h));
                    if in_wbox(p) && self.expr_contains(expr, p) {
                        return true;
                    }
                }
            }
        }
        for k in 0..n[2] {
            let bw = add(f.origin, scale(f.n, start[2] + (k as f64 + 0.5) * h));
            for j in 0..n[1] {
                let bvw = add(bw, scale(f.y, start[1] + (j as f64 + 0.5) * h));
                for i in 0..n[0] {
                    if i % 8 == 0 && j % 8 == 0 && k % 8 == 0 {
                        continue;
                    }
                    let p = add(bvw, scale(f.x, start[0] + (i as f64 + 0.5) * h));
                    if in_wbox(p) && self.expr_contains(expr, p) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn world_slab_misses_lump(
        &self,
        expr: &Expr,
        part: &LumpPartition,
        lump: u16,
        slo: [f64; 3],
        shi: [f64; 3],
    ) -> bool {
        let world_corners = |lo: [f64; 3], hi: [f64; 3]| -> [[f64; 3]; 8] {
            let mut cs = [[0.0; 3]; 8];
            for (c, slot) in cs.iter_mut().enumerate() {
                *slot = [
                    if c & 1 == 0 { lo[0] } else { hi[0] },
                    if c & 2 == 0 { lo[1] } else { hi[1] },
                    if c & 4 == 0 { lo[2] } else { hi[2] },
                ];
            }
            cs
        };
        let root_c = world_corners(slo, shi);
        if self.box_must_miss(expr, &root_c) || part.box_must_miss_voronoi(lump, &root_c) {
            return true;
        }
        const SPLIT_BUDGET: usize = 128;
        let mut q: VecDeque<Box3> = VecDeque::new();
        q.push_back((slo, shi));
        let mut splits = 0usize;
        while let Some((lo, hi)) = q.pop_front() {
            if splits >= SPLIT_BUDGET {
                return false;
            }
            splits += 1;
            let (c0, c1) = split_longest_axis(lo, hi);
            for (clo, chi) in [c0, c1] {
                let cs = world_corners(clo, chi);
                if !(self.box_must_miss(expr, &cs) || part.box_must_miss_voronoi(lump, &cs)) {
                    q.push_back((clo, chi));
                }
            }
        }
        true
    }

    /// Partition `expr` into connected lumps on an isotropic `cells` lattice in
    /// `self.frame`, building a Euclidean Voronoi KD-tree over 6-surface voxels
    /// and sound world bounding boxes when `>= 2` lumps exist.
    fn partition_lumps(&self, expr: &Expr, cells: usize) -> LumpPartitionOutcome {
        let Some((blo, bhi)) = self.expr_bbox(expr) else {
            return LumpPartitionOutcome::Empty;
        };
        let f = self.frame;
        let mut flo = [f64::INFINITY; 3];
        let mut fhi = [f64::NEG_INFINITY; 3];
        for c in 0..8 {
            let p = [
                if c & 1 == 0 { blo[0] } else { bhi[0] },
                if c & 2 == 0 { blo[1] } else { bhi[1] },
                if c & 4 == 0 { blo[2] } else { bhi[2] },
            ];
            let r = sub(p, f.origin);
            let coords = [dot(r, f.x), dot(r, f.y), dot(r, f.n)];
            for a in 0..3 {
                flo[a] = flo[a].min(coords[a]);
                fhi[a] = fhi[a].max(coords[a]);
            }
        }
        let ext = [fhi[0] - flo[0], fhi[1] - flo[1], fhi[2] - flo[2]];
        let longest = ext[0].max(ext[1]).max(ext[2]);
        if !longest.is_finite() || longest <= 0.0 {
            return LumpPartitionOutcome::Empty;
        }
        let h = longest / cells as f64;
        let n = [
            ((ext[0] / h).ceil() as usize + 2).max(1),
            ((ext[1] / h).ceil() as usize + 2).max(1),
            ((ext[2] / h).ceil() as usize + 2).max(1),
        ];
        let start = [flo[0] - h, flo[1] - h, flo[2] - h];
        let cell_q = |i: usize, j: usize, k: usize| -> [f64; 3] {
            [
                start[0] + (i as f64 + 0.5) * h,
                start[1] + (j as f64 + 0.5) * h,
                start[2] + (k as f64 + 0.5) * h,
            ]
        };
        let point_at = |i: usize, j: usize, k: usize| -> [f64; 3] {
            let [u, v, w] = cell_q(i, j, k);
            add(
                f.origin,
                add(add(scale(f.x, u), scale(f.y, v)), scale(f.n, w)),
            )
        };
        let grid = VoxelGrid::from_fn(n, |i, j, k| self.expr_contains(expr, point_at(i, j, k)));
        let total = n[0] * n[1] * n[2];
        let mut labels = vec![u16::MAX; total];
        let gidx = |i: usize, j: usize, k: usize| (k * n[1] + j) * n[0] + i;
        let mut stack = Vec::new();
        let mut comp_surface = Vec::new();
        let mut surface_pts: Vec<([f64; 3], u16)> = Vec::new();
        let mut occ_wlo: Vec<[f64; 3]> = Vec::new();
        let mut occ_whi: Vec<[f64; 3]> = Vec::new();
        let mut num_lumps = 0usize;

        for k0 in 0..n[2] {
            for j0 in 0..n[1] {
                for i0 in 0..n[0] {
                    let root = gidx(i0, j0, k0);
                    if labels[root] != u16::MAX || !grid.occupied(i0, j0, k0) {
                        continue;
                    }
                    let cid = num_lumps.min((u16::MAX - 1) as usize) as u16;
                    num_lumps += 1;
                    labels[root] = cid;
                    stack.push((i0, j0, k0));
                    comp_surface.clear();
                    let mut scan_pos = 0usize;
                    let mut wlo_c = [f64::INFINITY; 3];
                    let mut whi_c = [f64::NEG_INFINITY; 3];
                    loop {
                        while let Some((i, j, k)) = stack.pop() {
                            let pw = point_at(i, j, k);
                            for a in 0..3 {
                                wlo_c[a] = wlo_c[a].min(pw[a]);
                                whi_c[a] = whi_c[a].max(pw[a]);
                            }
                            let mut is_surf = false;
                            for (ni, nj, nk) in [
                                (i.wrapping_sub(1), j, k),
                                (i + 1, j, k),
                                (i, j.wrapping_sub(1), k),
                                (i, j + 1, k),
                                (i, j, k.wrapping_sub(1)),
                                (i, j, k + 1),
                            ] {
                                if ni < n[0] && nj < n[1] && nk < n[2] {
                                    if grid.occupied(ni, nj, nk) {
                                        let nb = gidx(ni, nj, nk);
                                        if labels[nb] == u16::MAX {
                                            labels[nb] = cid;
                                            stack.push((ni, nj, nk));
                                        }
                                    } else {
                                        is_surf = true;
                                    }
                                } else {
                                    is_surf = true;
                                }
                            }
                            if is_surf {
                                comp_surface.push((i, j, k));
                            }
                        }
                        let mut bridged = false;
                        while scan_pos < comp_surface.len() && !bridged {
                            let (i, j, k) = comp_surface[scan_pos];
                            scan_pos += 1;
                            let p0 = point_at(i, j, k);
                            'diag: for dk in -1isize..=1 {
                                let nk = k.wrapping_add_signed(dk);
                                if nk >= n[2] {
                                    continue;
                                }
                                for dj in -1isize..=1 {
                                    let nj = j.wrapping_add_signed(dj);
                                    if nj >= n[1] {
                                        continue;
                                    }
                                    for di in -1isize..=1 {
                                        if di.abs() + dj.abs() + dk.abs() <= 1 {
                                            continue;
                                        }
                                        let ni = i.wrapping_add_signed(di);
                                        if ni >= n[0] {
                                            continue;
                                        }
                                        let nb = gidx(ni, nj, nk);
                                        if labels[nb] != u16::MAX || !grid.occupied(ni, nj, nk) {
                                            continue;
                                        }
                                        let p1 = point_at(ni, nj, nk);
                                        let d = sub(p1, p0);
                                        if [0.25, 0.5, 0.75]
                                            .into_iter()
                                            .all(|t| self.expr_contains(expr, add(p0, scale(d, t))))
                                        {
                                            labels[nb] = cid;
                                            stack.push((ni, nj, nk));
                                            bridged = true;
                                            scan_pos -= 1;
                                            break 'diag;
                                        }
                                    }
                                }
                            }
                        }
                        if !bridged {
                            break;
                        }
                    }
                    for &(si, sj, sk) in &comp_surface {
                        surface_pts.push((cell_q(si, sj, sk), cid));
                    }
                    occ_wlo.push([
                        wlo_c[0].clamp(blo[0], bhi[0]),
                        wlo_c[1].clamp(blo[1], bhi[1]),
                        wlo_c[2].clamp(blo[2], bhi[2]),
                    ]);
                    occ_whi.push([
                        whi_c[0].clamp(blo[0], bhi[0]),
                        whi_c[1].clamp(blo[1], bhi[1]),
                        whi_c[2].clamp(blo[2], bhi[2]),
                    ]);
                }
            }
        }

        match num_lumps {
            0 => LumpPartitionOutcome::Empty,
            1 => LumpPartitionOutcome::Single,
            _ => {
                let mut nodes = Vec::with_capacity(surface_pts.len());
                let root = build_voronoi_kd(&mut surface_pts, &mut nodes);
                let mut part = LumpPartition {
                    frame: f,
                    start,
                    h,
                    n,
                    labels,
                    num_lumps,
                    nodes,
                    root,
                    lump_wbox: vec![(blo, bhi); num_lumps],
                };
                for c in 0..num_lumps {
                    let mut cur_lo = blo;
                    let mut cur_hi = bhi;
                    for ax in 0..3 {
                        if occ_wlo[c][ax] - cur_lo[ax] > h {
                            let mut lo_b = cur_lo[ax];
                            let mut hi_b = occ_wlo[c][ax];
                            for _ in 0..12 {
                                let mid = 0.5 * (lo_b + hi_b);
                                let mut test_hi = cur_hi;
                                test_hi[ax] = mid;
                                if self
                                    .world_slab_misses_lump(expr, &part, c as u16, cur_lo, test_hi)
                                {
                                    lo_b = mid;
                                    cur_lo[ax] = mid;
                                } else {
                                    hi_b = mid;
                                }
                            }
                        }
                        if cur_hi[ax] - occ_whi[c][ax] > h {
                            let mut lo_b = occ_whi[c][ax];
                            let mut hi_b = cur_hi[ax];
                            for _ in 0..12 {
                                let mid = 0.5 * (lo_b + hi_b);
                                let mut test_lo = cur_lo;
                                test_lo[ax] = mid;
                                if self
                                    .world_slab_misses_lump(expr, &part, c as u16, test_lo, cur_hi)
                                {
                                    hi_b = mid;
                                    cur_hi[ax] = mid;
                                } else {
                                    lo_b = mid;
                                }
                            }
                        }
                    }
                    part.lump_wbox[c] = (cur_lo, cur_hi);
                }
                LumpPartitionOutcome::Multi(LumpPartitionHandle(std::sync::Arc::new(part)))
            }
        }
    }

    /// Decide whether two expressions' solids meet (`Touches`), are provably
    /// separated beyond `TAU_MODEL` (`Disjoint`), or have unresolved sub-step
    /// contact (`Ambiguous`).
    ///
    /// - `Touches` is certified only from an exact interior witness
    ///   (`expr_contains(a, p) && expr_contains(b, p)`) or an exact 2D
    ///   positive-area overlap on a shared coplanar `Extrude` cap/wall plane.
    /// - `Disjoint` is certified only from sound analytic separation beyond
    ///   `TAU_MODEL` (`expr_bbox` or midpoint-padded KD-tree sub-box pruning via
    ///   [`Self::box_must_miss`]) — never from a lattice cell-count heuristic.
    fn exprs_touch(&self, a: &Expr, b: &Expr) -> TouchVerdict {
        let (Some(wa), Some(wb)) = (self.expr_bbox(a), self.expr_bbox(b)) else {
            return TouchVerdict::Disjoint;
        };
        let mut world_scale = 0.0_f64;
        for k in 0..3 {
            world_scale = world_scale
                .max((wa.1[k] - wa.0[k]).abs())
                .max((wb.1[k] - wb.0[k]).abs())
                .max(wa.0[k].abs())
                .max(wa.1[k].abs())
                .max(wb.0[k].abs())
                .max(wb.1[k].abs());
        }
        let sep_tol = cad_primitives::TAU_MODEL + cad_primitives::TAU_WORK * world_scale.max(1.0);
        if (0..3).any(|k| wa.1[k] + sep_tol < wb.0[k] || wb.1[k] + sep_tol < wa.0[k]) {
            return TouchVerdict::Disjoint;
        }
        let f = self.frame;
        let frame_bounds = |w: ([f64; 3], [f64; 3])| -> ([f64; 3], [f64; 3]) {
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for c in 0..8 {
                let p = [
                    if c & 1 == 0 { w.0[0] } else { w.1[0] },
                    if c & 2 == 0 { w.0[1] } else { w.1[1] },
                    if c & 4 == 0 { w.0[2] } else { w.1[2] },
                ];
                let r = sub(p, f.origin);
                let coords = [dot(r, f.x), dot(r, f.y), dot(r, f.n)];
                for m in 0..3 {
                    lo[m] = lo[m].min(coords[m]);
                    hi[m] = hi[m].max(coords[m]);
                }
            }
            (lo, hi)
        };
        let (alo, ahi) = frame_bounds(wa);
        let (blo, bhi) = frame_bounds(wb);
        let mut ilo = [0.0; 3];
        let mut ihi = [0.0; 3];
        let mut jlo = [0.0; 3];
        let mut jhi = [0.0; 3];
        for m in 0..3 {
            ilo[m] = alo[m].max(blo[m]);
            ihi[m] = ahi[m].min(bhi[m]);
            jlo[m] = alo[m].min(blo[m]);
            jhi[m] = ahi[m].max(bhi[m]);
        }
        if (0..3).any(|m| ilo[m] > ihi[m] + sep_tol) {
            return TouchVerdict::Disjoint;
        }
        let ext = [jhi[0] - jlo[0], jhi[1] - jlo[1], jhi[2] - jlo[2]];
        let longest = ext[0].max(ext[1]).max(ext[2]);
        if !longest.is_finite() || longest <= 0.0 {
            return TouchVerdict::Ambiguous {
                h: 0.0,
                why: "zero-extent joint box",
            };
        }
        let h = longest / EXACT_VOLUME_CELLS as f64;
        let n_full = [
            ((ext[0] / h).ceil() as usize + 2).max(1),
            ((ext[1] / h).ceil() as usize + 2).max(1),
            ((ext[2] / h).ceil() as usize + 2).max(1),
        ];
        let start = [jlo[0] - h, jlo[1] - h, jlo[2] - h];
        let mut lo_idx = [0usize; 3];
        let mut hi_idx = [0usize; 3];
        for m in 0..3 {
            let w_lo = ilo[m].min(ihi[m]);
            let w_hi = ilo[m].max(ihi[m]);
            lo_idx[m] =
                (((w_lo - start[m]) / h - 1.5).floor().max(0.0) as usize).min(n_full[m] - 1);
            hi_idx[m] = (((w_hi - start[m]) / h + 1.5).ceil().max(0.0) as usize).min(n_full[m] - 1);
        }
        let point_at = |i: usize, j: usize, k: usize| -> [f64; 3] {
            let (u, v, w) = (
                start[0] + (i as f64 + 0.5) * h,
                start[1] + (j as f64 + 0.5) * h,
                start[2] + (k as f64 + 0.5) * h,
            );
            add(
                f.origin,
                add(add(scale(f.x, u), scale(f.y, v)), scale(f.n, w)),
            )
        };
        // 1a. Joint-lattice interior witness (coarse step-4 pass, then full window).
        for k in (lo_idx[2]..=hi_idx[2]).step_by(4) {
            for j in (lo_idx[1]..=hi_idx[1]).step_by(4) {
                for i in (lo_idx[0]..=hi_idx[0]).step_by(4) {
                    let p = point_at(i, j, k);
                    if self.expr_contains(a, p) && self.expr_contains(b, p) {
                        return TouchVerdict::Touches;
                    }
                }
            }
        }
        for k in lo_idx[2]..=hi_idx[2] {
            for j in lo_idx[1]..=hi_idx[1] {
                for i in lo_idx[0]..=hi_idx[0] {
                    if (i - lo_idx[0]) % 4 == 0
                        && (j - lo_idx[1]) % 4 == 0
                        && (k - lo_idx[2]) % 4 == 0
                    {
                        continue;
                    }
                    let p = point_at(i, j, k);
                    if self.expr_contains(a, p) && self.expr_contains(b, p) {
                        return TouchVerdict::Touches;
                    }
                }
            }
        }
        // 1b. Tight overlap-box interior witness when one operand or their
        //     overlap box is much smaller than the joint box (e.g. C0032, C0039).
        if (0..3).all(|m| ilo[m] < ihi[m]) {
            let ov_ext = [ihi[0] - ilo[0], ihi[1] - ilo[1], ihi[2] - ilo[2]];
            let ov_longest = ov_ext[0].max(ov_ext[1]).max(ov_ext[2]);
            if ov_longest > 0.0 && ov_longest < 0.5 * longest {
                let h_ov = ov_longest / 32.0;
                let n_ov = [
                    ((ov_ext[0] / h_ov).ceil() as usize).clamp(1, 32),
                    ((ov_ext[1] / h_ov).ceil() as usize).clamp(1, 32),
                    ((ov_ext[2] / h_ov).ceil() as usize).clamp(1, 32),
                ];
                let step_ov = [
                    ov_ext[0] / n_ov[0] as f64,
                    ov_ext[1] / n_ov[1] as f64,
                    ov_ext[2] / n_ov[2] as f64,
                ];
                for k in 0..n_ov[2] {
                    let w = ilo[2] + (k as f64 + 0.5) * step_ov[2];
                    let bw = add(f.origin, scale(f.n, w));
                    for j in 0..n_ov[1] {
                        let v = ilo[1] + (j as f64 + 0.5) * step_ov[1];
                        let bvw = add(bw, scale(f.y, v));
                        for i in 0..n_ov[0] {
                            let u = ilo[0] + (i as f64 + 0.5) * step_ov[0];
                            let p = add(bvw, scale(f.x, u));
                            if self.expr_contains(a, p) && self.expr_contains(b, p) {
                                return TouchVerdict::Touches;
                            }
                        }
                    }
                }
            }
        }
        // 2. Exact coplanar planar-face overlap across Extrude caps and polygon walls.
        if self.coplanar_faces_overlap(a, b, world_scale.max(1.0)) {
            return TouchVerdict::Touches;
        }
        // 3. Midpoint-sound KD-tree sub-box pruning: if every sub-box `B` of
        //    `[ilo - 0.5*sep_tol, ihi + 0.5*sep_tol]` expanded by `0.5*sep_tol`
        //    misses either `a` or `b`, then no `p in a` and `q in b` can satisfy
        //    `||p - q||_inf <= sep_tol` (their midpoint `0.5*(p+q)` would lie in
        //    some unpruned leaf `B` with both `p` and `q` inside `B_half`).
        const SPLIT_BUDGET: usize = 512;
        let half_sep = 0.5 * sep_tol;
        let root_lo = [ilo[0] - half_sep, ilo[1] - half_sep, ilo[2] - half_sep];
        let root_hi = [ihi[0] + half_sep, ihi[1] + half_sep, ihi[2] + half_sep];
        let misses_padded = |lo: [f64; 3], hi: [f64; 3]| -> bool {
            let plo = [lo[0] - half_sep, lo[1] - half_sep, lo[2] - half_sep];
            let phi = [hi[0] + half_sep, hi[1] + half_sep, hi[2] + half_sep];
            let corners = frame_box_corners(&f, plo, phi);
            self.box_must_miss(a, &corners) || self.box_must_miss(b, &corners)
        };
        if misses_padded(root_lo, root_hi) {
            return TouchVerdict::Disjoint;
        }
        let mut queue: VecDeque<Box3> = VecDeque::new();
        queue.push_back((root_lo, root_hi));
        let mut splits = 0usize;
        while let Some((lo, hi)) = queue.pop_front() {
            let mid_f = [
                0.5 * (lo[0] + hi[0]),
                0.5 * (lo[1] + hi[1]),
                0.5 * (lo[2] + hi[2]),
            ];
            let mid_p = add(
                f.origin,
                add(
                    add(scale(f.x, mid_f[0]), scale(f.y, mid_f[1])),
                    scale(f.n, mid_f[2]),
                ),
            );
            if self.expr_contains(a, mid_p) && self.expr_contains(b, mid_p) {
                return TouchVerdict::Touches;
            }
            if splits >= SPLIT_BUDGET {
                return TouchVerdict::Ambiguous {
                    h,
                    why: "contact is below lattice and sub-box separation resolution",
                };
            }
            splits += 1;
            let (c0, c1) = split_longest_axis(lo, hi);
            for (clo, chi) in [c0, c1] {
                if !misses_padded(clo, chi) {
                    queue.push_back((clo, chi));
                }
            }
        }
        TouchVerdict::Disjoint
    }

    fn collect_extrude_patches(&self, expr: &Expr, out: &mut Vec<PlanarPatch>) {
        match expr {
            Expr::Leaf(i) | Expr::AltLeaf(i) => {
                if let ExactSolid::Extrude {
                    basis,
                    profile,
                    face_origin,
                    dir,
                    depth,
                } = self.leaf_solid(*i, matches!(expr, Expr::AltLeaf(_)))
                {
                    let dn = dot(*dir, basis.n);
                    if !dn.is_finite() || dn.abs() <= 1e-12 || *depth <= 0.0 {
                        return;
                    }
                    let perp_depth = (*depth * dn.abs()).abs();
                    let (u0, u1, v0, v1) = profile.bbox();
                    out.push(PlanarPatch {
                        origin: *face_origin,
                        u_axis: basis.x,
                        v_axis: basis.y,
                        normal: basis.n,
                        u_lo: u0,
                        u_hi: u1,
                        v_lo: v0,
                        v_hi: v1,
                        step: perp_depth,
                    });
                    out.push(PlanarPatch {
                        origin: add(*face_origin, scale(*dir, *depth)),
                        u_axis: basis.x,
                        v_axis: basis.y,
                        normal: basis.n,
                        u_lo: u0,
                        u_hi: u1,
                        v_lo: v0,
                        v_hi: v1,
                        step: perp_depth,
                    });
                    let mut push_poly_walls = |pts: &[(f64, f64)]| {
                        if pts.len() < 3 || pts.len() > 32 {
                            return;
                        }
                        for k in 0..pts.len() {
                            let (p0, p1) = (pts[k], pts[(k + 1) % pts.len()]);
                            let e3 = add(scale(basis.x, p1.0 - p0.0), scale(basis.y, p1.1 - p0.1));
                            let Some(u_ax) = norm(e3) else { continue };
                            let elen = dot(e3, u_ax);
                            if elen <= 1e-12 {
                                continue;
                            }
                            let Some(n_wall) = norm(cross(u_ax, *dir)) else {
                                continue;
                            };
                            let v_ax = cross(n_wall, u_ax);
                            let w_orig = add(
                                *face_origin,
                                add(scale(basis.x, p0.0), scale(basis.y, p0.1)),
                            );
                            let d_vec = scale(*dir, *depth);
                            let du_d = dot(d_vec, u_ax);
                            let dv_d = dot(d_vec, v_ax);
                            let wu_lo = 0.0_f64.min(du_d).min(elen + du_d);
                            let wu_hi = elen.max(du_d).max(elen + du_d);
                            let wv_lo = 0.0_f64.min(dv_d);
                            let wv_hi = 0.0_f64.max(dv_d);
                            out.push(PlanarPatch {
                                origin: w_orig,
                                u_axis: u_ax,
                                v_axis: v_ax,
                                normal: n_wall,
                                u_lo: wu_lo,
                                u_hi: wu_hi,
                                v_lo: wv_lo,
                                v_hi: wv_hi,
                                step: perp_depth.min(elen),
                            });
                        }
                    };
                    match profile {
                        Profile2::Polygon(pts) => push_poly_walls(pts),
                        Profile2::Region { outer, holes } => {
                            if let Profile2::Polygon(pts) = outer.as_ref() {
                                push_poly_walls(pts);
                            }
                            for h in holes {
                                if let Profile2::Polygon(pts) = h {
                                    push_poly_walls(pts);
                                }
                            }
                        }
                        Profile2::Circle { .. } | Profile2::Gear(_) => {}
                    }
                }
            }
            Expr::Union(es) => {
                for e in es {
                    self.collect_extrude_patches(e, out);
                }
            }
            Expr::Sub(lhs, rhs) | Expr::Inter(lhs, rhs) => {
                self.collect_extrude_patches(lhs, out);
                self.collect_extrude_patches(rhs, out);
            }
            Expr::Lump { base, .. } => {
                self.collect_extrude_patches(base, out);
            }
        }
    }

    /// True when `a` and `b` share a positive-area 2D coplanar face patch across
    /// any pair of coincident `Extrude` cap/wall planes.
    fn coplanar_faces_overlap(&self, a: &Expr, b: &Expr, scale_ref: f64) -> bool {
        let mut patches_a = Vec::new();
        let mut patches_b = Vec::new();
        self.collect_extrude_patches(a, &mut patches_a);
        self.collect_extrude_patches(b, &mut patches_b);
        if patches_a.is_empty() || patches_b.is_empty() {
            return false;
        }
        let ang_tol = 10.0 * cad_primitives::TAU_WORK;
        let dist_tol = 10.0 * cad_primitives::TAU_WORK * scale_ref;
        for pa in &patches_a {
            for pb in &patches_b {
                if dot(pa.normal, pb.normal).abs() < 1.0 - ang_tol {
                    continue;
                }
                if dot(sub(pb.origin, pa.origin), pa.normal).abs() > dist_tol {
                    continue;
                }
                let mut bu_lo = f64::INFINITY;
                let mut bu_hi = f64::NEG_INFINITY;
                let mut bv_lo = f64::INFINITY;
                let mut bv_hi = f64::NEG_INFINITY;
                for &(bu, bv) in &[
                    (pb.u_lo, pb.v_lo),
                    (pb.u_lo, pb.v_hi),
                    (pb.u_hi, pb.v_lo),
                    (pb.u_hi, pb.v_hi),
                ] {
                    let p = add(pb.origin, add(scale(pb.u_axis, bu), scale(pb.v_axis, bv)));
                    let r = sub(p, pa.origin);
                    let u = dot(r, pa.u_axis);
                    let v = dot(r, pa.v_axis);
                    bu_lo = bu_lo.min(u);
                    bu_hi = bu_hi.max(u);
                    bv_lo = bv_lo.min(v);
                    bv_hi = bv_hi.max(v);
                }
                let ou0 = pa.u_lo.max(bu_lo);
                let ou1 = pa.u_hi.min(bu_hi);
                let ov0 = pa.v_lo.max(bv_lo);
                let ov1 = pa.v_hi.min(bv_hi);
                if ou0 >= ou1 || ov0 >= ov1 {
                    continue;
                }
                let du = ou1 - ou0;
                let dv = ov1 - ov0;
                let uv_min = du.min(dv);
                if uv_min <= dist_tol {
                    continue;
                }
                // Lift along `pa.normal` by at most `TAU_MODEL / 2` (`5e-8 m`).
                // Because `expr_contains` tests exact analytical membership on `a`
                // and `b`, finding signs `sa, sb ∈ {+eps_n, -eps_n}` such that
                // `q + sa·n ∈ a` and `q + sb·n ∈ b` at `p0` and all four points of
                // the 2D `±delta_uv` cross proves `||(q + sa·n) - (q + sb·n)|| =
                // |sa - sb| <= 2·eps_n <= TAU_MODEL` across a positive-area 2D
                // neighborhood: either `a ∩ b ≠ ∅` (`sa == sb`) or every normal
                // segment of length `2·eps_n <= TAU_MODEL` crosses both `∂a` and
                // `∂b`, so `dist(∂a, ∂b) <= TAU_MODEL` over that patch.
                let eps_n = (1e-6 * pa.step.min(pb.step))
                    .max(2.0 * dist_tol)
                    .min(0.5 * cad_primitives::TAU_MODEL);
                let delta_uv = 1e-5 * uv_min;
                let n_lift_n = pa.normal;
                for iv in 0..32 {
                    let v = ov0 + (iv as f64 + 0.5) / 32.0 * dv;
                    for iu in 0..32 {
                        let u = ou0 + (iu as f64 + 0.5) / 32.0 * du;
                        let p0 = add(pa.origin, add(scale(pa.u_axis, u), scale(pa.v_axis, v)));
                        let p_plus = add(p0, scale(n_lift_n, eps_n));
                        let p_minus = sub(p0, scale(n_lift_n, eps_n));
                        let a_plus = self.expr_contains(a, p_plus);
                        let a_minus = if a_plus {
                            false
                        } else {
                            self.expr_contains(a, p_minus)
                        };
                        if !a_plus && !a_minus {
                            continue;
                        }
                        let b_plus = self.expr_contains(b, p_plus);
                        let b_minus = if b_plus {
                            false
                        } else {
                            self.expr_contains(b, p_minus)
                        };
                        if !b_plus && !b_minus {
                            continue;
                        }
                        let sa = if a_plus { eps_n } else { -eps_n };
                        let sb = if b_plus { eps_n } else { -eps_n };
                        let mut area_ok = true;
                        for &(du_off, dv_off) in &[
                            (delta_uv, 0.0),
                            (-delta_uv, 0.0),
                            (0.0, delta_uv),
                            (0.0, -delta_uv),
                        ] {
                            let q = add(
                                pa.origin,
                                add(scale(pa.u_axis, u + du_off), scale(pa.v_axis, v + dv_off)),
                            );
                            if !self.expr_contains(a, add(q, scale(n_lift_n, sa)))
                                || !self.expr_contains(b, add(q, scale(n_lift_n, sb)))
                            {
                                area_ok = false;
                                break;
                            }
                        }
                        if area_ok {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn expr_extent_along(
        &self,
        expr: &Expr,
        dir_unit: [f64; 3],
        cells: usize,
    ) -> Option<SampledExtent> {
        let (blo, bhi) = self.expr_bbox(expr)?;
        let frame = Basis::from_origin_normal([0.0; 3], dir_unit)?;
        let raw_proj = self.expr_proj_bound(expr, dir_unit)?;
        let mut flo = [f64::INFINITY; 3];
        let mut fhi = [f64::NEG_INFINITY; 3];
        for c in 0..8 {
            let p = [
                if c & 1 == 0 { blo[0] } else { bhi[0] },
                if c & 2 == 0 { blo[1] } else { bhi[1] },
                if c & 4 == 0 { blo[2] } else { bhi[2] },
            ];
            let coords = [dot(p, frame.x), dot(p, frame.y), dot(p, frame.n)];
            for a in 0..3 {
                flo[a] = flo[a].min(coords[a]);
                fhi[a] = fhi[a].max(coords[a]);
            }
        }
        flo[2] = flo[2].max(raw_proj.0);
        fhi[2] = fhi[2].min(raw_proj.1);
        if flo[2] > fhi[2] {
            return None;
        }
        let ext = [fhi[0] - flo[0], fhi[1] - flo[1], fhi[2] - flo[2]];
        let longest = ext[0].max(ext[1]).max(ext[2]);
        if !longest.is_finite() || longest <= 0.0 {
            return Some(SampledExtent {
                outer_proj: (flo[2], fhi[2]),
                occupied: None,
                uncertainty: 0.0,
            });
        }
        let h = longest / cells as f64;
        let n = [
            ((ext[0] / h).ceil() as usize + 2).max(1),
            ((ext[1] / h).ceil() as usize + 2).max(1),
            ((ext[2] / h).ceil() as usize + 2).max(1),
        ];
        let start = [flo[0] - h, flo[1] - h, flo[2] - h];
        let slice_hits = |k: usize| -> bool {
            let w = start[2] + (k as f64 + 0.5) * h;
            let base_w = scale(frame.n, w);
            for j in (0..n[1]).step_by(8) {
                let vw = add(base_w, scale(frame.y, start[1] + (j as f64 + 0.5) * h));
                for i in (0..n[0]).step_by(8) {
                    let p = add(vw, scale(frame.x, start[0] + (i as f64 + 0.5) * h));
                    if self.expr_contains(expr, p) {
                        return true;
                    }
                }
            }
            for j in 0..n[1] {
                let vw = add(base_w, scale(frame.y, start[1] + (j as f64 + 0.5) * h));
                for i in 0..n[0] {
                    if i % 8 == 0 && j % 8 == 0 {
                        continue;
                    }
                    let p = add(vw, scale(frame.x, start[0] + (i as f64 + 0.5) * h));
                    if self.expr_contains(expr, p) {
                        return true;
                    }
                }
            }
            false
        };
        let k_min = (0..n[2]).find(|&k| slice_hits(k));
        let occupied = k_min.map(|lo_k| {
            let hi_k = ((lo_k + 1)..n[2])
                .rev()
                .find(|&k| slice_hits(k))
                .unwrap_or(lo_k);
            (
                start[2] + (lo_k as f64 + 0.5) * h,
                start[2] + (hi_k as f64 + 0.5) * h,
            )
        });
        let outer_proj = if expr_has_sub_or_inter(expr) {
            self.tighten_outer_extent_along(expr, &frame, (flo, fhi), occupied, h)?
        } else if let Some((occ_lo, occ_hi)) = occupied {
            (flo[2].min(occ_lo), fhi[2].max(occ_hi))
        } else {
            (flo[2], fhi[2])
        };
        Some(SampledExtent {
            outer_proj,
            occupied,
            uncertainty: h,
        })
    }

    /// Closed-form outer bound `(lo, hi)` of `expr` projected onto unit `dir_unit`.
    fn expr_proj_bound(&self, e: &Expr, dir_unit: [f64; 3]) -> Option<(f64, f64)> {
        match e {
            Expr::Leaf(i) | Expr::AltLeaf(i) => Some(solid_proj_bound(
                self.leaf_solid(*i, matches!(e, Expr::AltLeaf(_))),
                dir_unit,
            )),
            Expr::Union(es) => {
                let mut lo = f64::INFINITY;
                let mut hi = f64::NEG_INFINITY;
                let mut any = false;
                for (a, b) in es.iter().filter_map(|x| self.expr_proj_bound(x, dir_unit)) {
                    lo = lo.min(a);
                    hi = hi.max(b);
                    any = true;
                }
                any.then_some((lo, hi))
            }
            Expr::Sub(a, _) => self.expr_proj_bound(a, dir_unit),
            Expr::Inter(a, b) => {
                let (alo, ahi) = self.expr_proj_bound(a, dir_unit)?;
                let (blo, bhi) = self.expr_proj_bound(b, dir_unit)?;
                let (lo, hi) = (alo.max(blo), ahi.min(bhi));
                (lo <= hi).then_some((lo, hi))
            }
            Expr::Lump {
                base,
                partition,
                lump,
            } => {
                let (blo, bhi) = self.expr_proj_bound(base, dir_unit)?;
                let (wlo, whi) = partition.0.lump_wbox[*lump as usize];
                let mut llo = f64::INFINITY;
                let mut lhi = f64::NEG_INFINITY;
                for c in 0..8 {
                    let p = [
                        if c & 1 == 0 { wlo[0] } else { whi[0] },
                        if c & 2 == 0 { wlo[1] } else { whi[1] },
                        if c & 4 == 0 { wlo[2] } else { whi[2] },
                    ];
                    let s = dot(p, dir_unit);
                    llo = llo.min(s);
                    lhi = lhi.max(s);
                }
                let (lo, hi) = (blo.max(llo), bhi.min(lhi));
                (lo <= hi).then_some((lo, hi))
            }
        }
    }

    fn box_must_miss(&self, e: &Expr, corners: &[[f64; 3]; 8]) -> bool {
        match e {
            Expr::Leaf(i) | Expr::AltLeaf(i) => {
                solid_must_miss(self.leaf_solid(*i, matches!(e, Expr::AltLeaf(_))), corners)
            }
            Expr::Union(es) => es.iter().all(|x| self.box_must_miss(x, corners)),
            Expr::Inter(a, b) => self.box_must_miss(a, corners) || self.box_must_miss(b, corners),
            Expr::Sub(a, b) => self.box_must_miss(a, corners) || self.box_must_cover(b, corners),
            Expr::Lump {
                base,
                partition,
                lump,
            } => {
                self.box_must_miss(base, corners)
                    || partition.0.box_must_miss_voronoi(*lump, corners)
            }
        }
    }

    fn box_must_cover(&self, e: &Expr, corners: &[[f64; 3]; 8]) -> bool {
        match e {
            Expr::Leaf(i) | Expr::AltLeaf(i) => {
                solid_must_cover(self.leaf_solid(*i, matches!(e, Expr::AltLeaf(_))), corners)
            }
            Expr::Union(es) => es.iter().any(|x| self.box_must_cover(x, corners)),
            Expr::Inter(a, b) => self.box_must_cover(a, corners) && self.box_must_cover(b, corners),
            Expr::Sub(a, b) => self.box_must_cover(a, corners) && self.box_must_miss(b, corners),
            Expr::Lump {
                base,
                partition,
                lump,
            } => {
                self.box_must_cover(base, corners)
                    && partition.0.box_must_cover_voronoi(*lump, corners)
            }
        }
    }

    /// Tighten `[flo[2], fhi[2]]` along `frame.n` by pruning sub-boxes proven
    /// empty by [`Self::box_must_miss`]. Every discarded box is mathematically
    /// disjoint from `expr`, so the frontier extremum at any stop budget is a
    /// guaranteed outer bound — never missing sub-`h` spikes or gear teeth.
    fn tighten_outer_extent_along(
        &self,
        expr: &Expr,
        frame: &Basis,
        init_fbox: ([f64; 3], [f64; 3]),
        occupied: Option<(f64, f64)>,
        h: f64,
    ) -> Option<(f64, f64)> {
        const POP_BUDGET: usize = 1024;
        let (flo, fhi) = init_fbox;
        let root_corners = frame_box_corners(frame, flo, fhi);
        if occupied.is_none() && self.box_must_miss(expr, &root_corners) {
            return None;
        }
        let min_cell = 0.25 * h;

        let mut outer_lo = flo[2];
        if occupied.is_none_or(|(occ_lo, _)| occ_lo - flo[2] > min_cell) {
            let mut heap: BinaryHeap<KdBoxLo> = BinaryHeap::new();
            heap.push(KdBoxLo { lo: flo, hi: fhi });
            let mut pops = 0usize;
            let mut found = None;
            while let Some(KdBoxLo { lo, hi }) = heap.pop() {
                if occupied.is_some_and(|(occ_lo, _)| lo[2] >= occ_lo)
                    || pops >= POP_BUDGET
                    || (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]) <= min_cell
                {
                    found = Some(lo[2]);
                    break;
                }
                pops += 1;
                let (c0, c1) = split_longest_axis(lo, hi);
                for (clo, chi) in [c0, c1] {
                    let corners = frame_box_corners(frame, clo, chi);
                    if !self.box_must_miss(expr, &corners) {
                        heap.push(KdBoxLo { lo: clo, hi: chi });
                    }
                }
            }
            match found {
                Some(v) => outer_lo = v,
                None if occupied.is_none() => return None,
                None => {}
            }
        }

        let mut outer_hi = fhi[2];
        if occupied.is_none_or(|(_, occ_hi)| fhi[2] - occ_hi > min_cell) {
            let mut heap: BinaryHeap<KdBoxHi> = BinaryHeap::new();
            heap.push(KdBoxHi { lo: flo, hi: fhi });
            let mut pops = 0usize;
            let mut found = None;
            while let Some(KdBoxHi { lo, hi }) = heap.pop() {
                if occupied.is_some_and(|(_, occ_hi)| hi[2] <= occ_hi)
                    || pops >= POP_BUDGET
                    || (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]) <= min_cell
                {
                    found = Some(hi[2]);
                    break;
                }
                pops += 1;
                let (c0, c1) = split_longest_axis(lo, hi);
                for (clo, chi) in [c0, c1] {
                    let corners = frame_box_corners(frame, clo, chi);
                    if !self.box_must_miss(expr, &corners) {
                        heap.push(KdBoxHi { lo: clo, hi: chi });
                    }
                }
            }
            match found {
                Some(v) => outer_hi = v,
                None if occupied.is_none() => return None,
                None => {}
            }
        }

        if let Some((occ_lo, occ_hi)) = occupied {
            outer_lo = outer_lo.min(occ_lo);
            outer_hi = outer_hi.max(occ_hi);
        }
        (outer_lo <= outer_hi).then_some((outer_lo, outer_hi))
    }
}

#[derive(Clone, Copy)]
struct KdBoxLo {
    lo: [f64; 3],
    hi: [f64; 3],
}
impl PartialEq for KdBoxLo {
    fn eq(&self, other: &Self) -> bool {
        self.lo == other.lo && self.hi == other.hi
    }
}
impl Eq for KdBoxLo {}
impl PartialOrd for KdBoxLo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for KdBoxLo {
    fn cmp(&self, other: &Self) -> Ordering {
        other.lo[2]
            .total_cmp(&self.lo[2])
            .then_with(|| (self.hi[2] - self.lo[2]).total_cmp(&(other.hi[2] - other.lo[2])))
    }
}

#[derive(Clone, Copy)]
struct KdBoxHi {
    lo: [f64; 3],
    hi: [f64; 3],
}
impl PartialEq for KdBoxHi {
    fn eq(&self, other: &Self) -> bool {
        self.lo == other.lo && self.hi == other.hi
    }
}
impl Eq for KdBoxHi {}
impl PartialOrd for KdBoxHi {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for KdBoxHi {
    fn cmp(&self, other: &Self) -> Ordering {
        self.hi[2]
            .total_cmp(&other.hi[2])
            .then_with(|| (self.hi[2] - self.lo[2]).total_cmp(&(other.hi[2] - other.lo[2])))
    }
}

type Box3 = ([f64; 3], [f64; 3]);

struct PlanarPatch {
    origin: [f64; 3],
    u_axis: [f64; 3],
    v_axis: [f64; 3],
    normal: [f64; 3],
    u_lo: f64,
    u_hi: f64,
    v_lo: f64,
    v_hi: f64,
    step: f64,
}

fn split_longest_axis(lo: [f64; 3], hi: [f64; 3]) -> (Box3, Box3) {
    let d = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let ax = if d[2] >= d[0] && d[2] >= d[1] {
        2
    } else if d[0] >= d[1] {
        0
    } else {
        1
    };
    let mid = 0.5 * (lo[ax] + hi[ax]);
    let mut hi0 = hi;
    hi0[ax] = mid;
    let mut lo1 = lo;
    lo1[ax] = mid;
    ((lo, hi0), (lo1, hi))
}

fn frame_box_corners(frame: &Basis, lo: [f64; 3], hi: [f64; 3]) -> [[f64; 3]; 8] {
    let mut out = [[0.0; 3]; 8];
    for (c, slot) in out.iter_mut().enumerate() {
        let u = if c & 1 == 0 { lo[0] } else { hi[0] };
        let v = if c & 2 == 0 { lo[1] } else { hi[1] };
        let w = if c & 4 == 0 { lo[2] } else { hi[2] };
        *slot = add(
            frame.origin,
            add(add(scale(frame.x, u), scale(frame.y, v)), scale(frame.n, w)),
        );
    }
    out
}

/// Closed-form outer projection bound `(lo, hi)` of one [`ExactSolid`] along
/// unit direction `d`. Exact for extruded polygons, circles, gears (addendum
/// cylinder), and partial/full revolves over `[0, angle]`.
fn solid_proj_bound(solid: &ExactSolid, d: [f64; 3]) -> (f64, f64) {
    match solid {
        ExactSolid::Extrude {
            basis,
            profile,
            face_origin,
            dir,
            depth,
        } => {
            let f0 = dot(*face_origin, d);
            let dx = dot(basis.x, d);
            let dy = dot(basis.y, d);
            let dt = dot(*dir, d) * *depth;
            let (t_lo, t_hi) = (dt.min(0.0), dt.max(0.0));
            let (p_lo, p_hi) = profile_linear_bound(profile, dx, dy);
            let (lo, hi) = (f0 + p_lo + t_lo, f0 + p_hi + t_hi);
            let orig_max = face_origin[0]
                .abs()
                .max(face_origin[1].abs())
                .max(face_origin[2].abs());
            let scale = (hi - lo)
                .abs()
                .max(orig_max)
                .max(p_lo.abs().max(p_hi.abs()))
                .max(depth.abs())
                .max(f64::MIN_POSITIVE);
            let pad = 1e-12 * scale;
            (lo - pad, hi + pad)
        }
        ExactSolid::Revolve {
            basis,
            profile,
            axis_origin,
            axis,
            w_hat,
            m_hat,
            angle,
            full_turn,
        } => {
            let (u0, u1, v0, v1) = profile.bbox();
            let mut hmin = f64::INFINITY;
            let mut hmax = f64::NEG_INFINITY;
            let mut rmax: f64 = 0.0;
            for &(u, v) in &[(u0, v0), (u0, v1), (u1, v0), (u1, v1)] {
                let p = basis.embed(u, v);
                let r = sub(p, *axis_origin);
                let h = dot(r, *axis);
                let rad = sub(r, scale(*axis, h));
                hmin = hmin.min(h);
                hmax = hmax.max(h);
                rmax = rmax.max(dot(rad, rad).sqrt());
            }
            let a0 = dot(*axis_origin, d);
            let da = dot(*axis, d);
            let dw = dot(*w_hat, d);
            let dm = dot(*m_hat, d);
            let (h_lo, h_hi) = ((hmin * da).min(hmax * da), (hmin * da).max(hmax * da));
            let (r_lo, r_hi) = if *full_turn {
                let amp = dw.hypot(dm);
                (-rmax * amp, rmax * amp)
            } else {
                let eval = |th: f64| dw * th.cos() + dm * th.sin();
                let mut f_min = eval(0.0).min(eval(*angle));
                let mut f_max = eval(0.0).max(eval(*angle));
                let th0 = dm.atan2(dw).rem_euclid(TAU);
                for th in [th0, (th0 + PI).rem_euclid(TAU)] {
                    if th <= *angle {
                        let fv = eval(th);
                        f_min = f_min.min(fv);
                        f_max = f_max.max(fv);
                    }
                }
                (rmax * f_min.min(0.0), rmax * f_max.max(0.0))
            };
            let (lo, hi) = (a0 + h_lo + r_lo, a0 + h_hi + r_hi);
            let pad = 1e-12 * (hi - lo).abs().max(lo.abs()).max(hi.abs()).max(1.0);
            (lo - pad, hi + pad)
        }
    }
}

fn profile_linear_bound(profile: &Profile2, dx: f64, dy: f64) -> (f64, f64) {
    match profile {
        Profile2::Region { outer, .. } => profile_linear_bound(outer, dx, dy),
        Profile2::Polygon(pts) if !pts.is_empty() => {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for &(u, v) in pts {
                let s = u * dx + v * dy;
                lo = lo.min(s);
                hi = hi.max(s);
            }
            (lo, hi)
        }
        Profile2::Circle { cx, cy, r } => {
            let c = cx * dx + cy * dy;
            let amp = r * dx.hypot(dy);
            (c - amp, c + amp)
        }
        Profile2::Gear(g) => {
            let c = g.cx * dx + g.cy * dy;
            let amp = g.add_r * dx.hypot(dy);
            (c - amp, c + amp)
        }
        Profile2::Polygon(_) => (0.0, 0.0),
    }
}

/// If `pts` is a simple, winding-±1 convex polygon, returns `Some((sign, max_edge))`
/// where `sign` is `+1.0` for CCW and `-1.0` for CW.
fn polygon_convex_sign(pts: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = pts.len();
    if n < 3 {
        return None;
    }
    let mut area2 = 0.0;
    let mut max_edge: f64 = 0.0;
    let mut edges = Vec::with_capacity(n);
    for i in 0..n {
        let p0 = pts[i];
        let p1 = pts[(i + 1) % n];
        area2 += p0.0 * p1.1 - p1.0 * p0.1;
        let (ex, ey) = (p1.0 - p0.0, p1.1 - p0.1);
        let elen = ex.hypot(ey);
        max_edge = max_edge.max(elen);
        edges.push((p0, (ex, ey), elen));
    }
    if area2.abs() <= 1e-24 || max_edge <= 0.0 {
        return None;
    }
    let sign = area2.signum();
    let valid_edges: Vec<_> = edges
        .into_iter()
        .filter(|(_, _, elen)| *elen > 1e-14 * max_edge)
        .collect();
    if valid_edges.len() < 3 {
        return None;
    }
    for &(p0, (ex, ey), elen) in &valid_edges {
        let tol = 1e-12 * elen * max_edge;
        for &pk in pts {
            let cross = sign * (ex * (pk.1 - p0.1) - ey * (pk.0 - p0.0));
            if cross < -tol {
                return None;
            }
        }
    }
    let m = valid_edges.len();
    let mut total_turn = 0.0;
    for i in 0..m {
        let (_, (e0x, e0y), _) = valid_edges[i];
        let (_, (e1x, e1y), _) = valid_edges[(i + 1) % m];
        total_turn += (e0x * e1y - e0y * e1x).atan2(e0x * e1x + e0y * e1y);
    }
    if (total_turn - sign * TAU).abs() > 1e-4 {
        return None;
    }
    Some((sign, max_edge))
}

fn segment_meets_rect_2d(
    a: (f64, f64),
    b: (f64, f64),
    rx0: f64,
    rx1: f64,
    ry0: f64,
    ry1: f64,
) -> bool {
    let mut t0 = 0.0_f64;
    let mut t1 = 1.0_f64;
    let clip = |p: f64, q: f64, t0: &mut f64, t1: &mut f64| -> bool {
        if p.abs() <= 1e-30 {
            q >= 0.0
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > *t1 {
                    return false;
                }
                if r > *t0 {
                    *t0 = r;
                }
            } else {
                if r < *t0 {
                    return false;
                }
                if r < *t1 {
                    *t1 = r;
                }
            }
            true
        }
    };
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    clip(-dx, a.0 - rx0, &mut t0, &mut t1)
        && clip(dx, rx1 - a.0, &mut t0, &mut t1)
        && clip(-dy, a.1 - ry0, &mut t0, &mut t1)
        && clip(dy, ry1 - a.1, &mut t0, &mut t1)
}

/// True only when the convex hull of `corners` is provably disjoint from `solid`.
fn solid_must_miss(solid: &ExactSolid, corners: &[[f64; 3]; 8]) -> bool {
    match solid {
        ExactSolid::Extrude {
            basis,
            profile,
            face_origin,
            dir,
            depth,
        } => {
            let dn = dot(*dir, basis.n);
            if !dn.is_finite() || dn.abs() <= 1e-12 {
                return false;
            }
            let t_eps = 1e-12 * depth.abs().max(1.0);
            let mut t_min = f64::INFINITY;
            let mut t_max = f64::NEG_INFINITY;
            let mut pu = [0.0; 8];
            let mut pv = [0.0; 8];
            let mut u_min = f64::INFINITY;
            let mut u_max = f64::NEG_INFINITY;
            let mut v_min = f64::INFINITY;
            let mut v_max = f64::NEG_INFINITY;
            for (c, &p) in corners.iter().enumerate() {
                let r = sub(p, *face_origin);
                let t = dot(r, basis.n) / dn;
                t_min = t_min.min(t);
                t_max = t_max.max(t);
                let q = sub(r, scale(*dir, t));
                let (u, v) = (dot(q, basis.x), dot(q, basis.y));
                pu[c] = u;
                pv[c] = v;
                u_min = u_min.min(u);
                u_max = u_max.max(u);
                v_min = v_min.min(v);
                v_max = v_max.max(v);
            }
            if t_max < -t_eps || t_min > *depth + t_eps {
                return true;
            }
            let (bu0, bu1, bv0, bv1) = profile.bbox();
            let p_scale = (bu1 - bu0).abs().max((bv1 - bv0).abs()).max(1.0);
            let p_eps = 1e-12 * p_scale;
            if u_max < bu0 - p_eps
                || u_min > bu1 + p_eps
                || v_max < bv0 - p_eps
                || v_min > bv1 + p_eps
            {
                return true;
            }
            let profile_misses_uv = |prof: &Profile2| -> bool {
                match prof {
                    Profile2::Circle { cx, cy, r } => {
                        let cu = cx.clamp(u_min, u_max);
                        let cv = cy.clamp(v_min, v_max);
                        (cu - cx).hypot(cv - cy) > *r + p_eps
                    }
                    Profile2::Gear(g) => {
                        let cu = g.cx.clamp(u_min, u_max);
                        let cv = g.cy.clamp(v_min, v_max);
                        (cu - g.cx).hypot(cv - g.cy) > g.add_r + p_eps
                    }
                    Profile2::Polygon(pts) => {
                        if let Some((sign, max_edge)) = polygon_convex_sign(pts) {
                            let n = pts.len();
                            for i in 0..n {
                                let (p0, p1) = (pts[i], pts[(i + 1) % n]);
                                let (ex, ey) = (p1.0 - p0.0, p1.1 - p0.1);
                                let elen = ex.hypot(ey);
                                if elen <= 1e-14 * max_edge {
                                    continue;
                                }
                                let tol = 1e-11 * elen * max_edge;
                                if (0..8).all(|c| {
                                    sign * (ex * (pv[c] - p0.1) - ey * (pu[c] - p0.0)) < -tol
                                }) {
                                    return true;
                                }
                            }
                        }
                        if pts.len() >= 3 {
                            let (rx0, rx1) = (u_min - p_eps, u_max + p_eps);
                            let (ry0, ry1) = (v_min - p_eps, v_max + p_eps);
                            let any_edge = (0..pts.len()).any(|i| {
                                segment_meets_rect_2d(
                                    pts[i],
                                    pts[(i + 1) % pts.len()],
                                    rx0,
                                    rx1,
                                    ry0,
                                    ry1,
                                )
                            });
                            if !any_edge && !prof.contains(0.5 * (rx0 + rx1), 0.5 * (ry0 + ry1)) {
                                return true;
                            }
                        }
                        false
                    }
                    Profile2::Region { .. } => false,
                }
            };
            match profile {
                Profile2::Region { outer, holes } => {
                    if profile_misses_uv(outer) {
                        return true;
                    }
                    let (rx0, rx1) = (u_min - p_eps, u_max + p_eps);
                    let (ry0, ry1) = (v_min - p_eps, v_max + p_eps);
                    let (cu, cv) = (0.5 * (rx0 + rx1), 0.5 * (ry0 + ry1));
                    for hole in holes {
                        if let Profile2::Polygon(hpts) = hole {
                            if hpts.len() >= 3 {
                                let any_edge = (0..hpts.len()).any(|i| {
                                    segment_meets_rect_2d(
                                        hpts[i],
                                        hpts[(i + 1) % hpts.len()],
                                        rx0,
                                        rx1,
                                        ry0,
                                        ry1,
                                    )
                                });
                                if !any_edge && hole.contains(cu, cv) {
                                    return true;
                                }
                            }
                        }
                    }
                    false
                }
                other => profile_misses_uv(other),
            }
        }
        ExactSolid::Revolve {
            basis,
            profile,
            axis_origin,
            axis,
            w_hat,
            m_hat,
            angle,
            full_turn,
        } => {
            let (u0, u1, v0, v1) = profile.bbox();
            let mut hmin = f64::INFINITY;
            let mut hmax = f64::NEG_INFINITY;
            let mut rmax: f64 = 0.0;
            for &(u, v) in &[(u0, v0), (u0, v1), (u1, v0), (u1, v1)] {
                let p = basis.embed(u, v);
                let r = sub(p, *axis_origin);
                let h = dot(r, *axis);
                let rad = sub(r, scale(*axis, h));
                hmin = hmin.min(h);
                hmax = hmax.max(h);
                rmax = rmax.max(dot(rad, rad).sqrt());
            }
            let eps = 1e-12 * (hmax - hmin).abs().max(rmax).max(1.0);
            let mut bh_min = f64::INFINITY;
            let mut bh_max = f64::NEG_INFINITY;
            let mut w_min = f64::INFINITY;
            let mut w_max = f64::NEG_INFINITY;
            let mut m_min = f64::INFINITY;
            let mut m_max = f64::NEG_INFINITY;
            for &p in corners {
                let r = sub(p, *axis_origin);
                let h = dot(r, *axis);
                let wc = dot(r, *w_hat);
                let mc = dot(r, *m_hat);
                bh_min = bh_min.min(h);
                bh_max = bh_max.max(h);
                w_min = w_min.min(wc);
                w_max = w_max.max(wc);
                m_min = m_min.min(mc);
                m_max = m_max.max(mc);
            }
            if bh_max < hmin - eps || bh_min > hmax + eps {
                return true;
            }
            let cw = 0.0_f64.clamp(w_min, w_max);
            let cm = 0.0_f64.clamp(m_min, m_max);
            if cw.hypot(cm) > rmax + eps {
                return true;
            }
            if !*full_turn && *angle <= PI && m_max < -eps {
                return true;
            }
            if !*full_turn && *angle <= 0.5 * PI && w_max < -eps {
                return true;
            }
            false
        }
    }
}

/// True only when the convex hull of `corners` is provably contained inside
/// the regularized closure of `solid` (within `1e-11` relative — 1000× finer
/// than B-Rep coincidence tolerance, so flush coplanar cap cuts leave no
/// phantom zero-volume skin while every physical feature `> 1e-10` survives).
fn solid_must_cover(solid: &ExactSolid, corners: &[[f64; 3]; 8]) -> bool {
    let ExactSolid::Extrude {
        basis,
        profile,
        face_origin,
        dir,
        depth,
    } = solid
    else {
        return false;
    };
    let dn = dot(*dir, basis.n);
    if !dn.is_finite() || dn.abs() <= 1e-12 || *depth <= 0.0 {
        return false;
    }
    let t_eps = 1e-11 * depth.abs().max(1.0);
    let mut pu = [0.0; 8];
    let mut pv = [0.0; 8];
    for (c, &p) in corners.iter().enumerate() {
        let r = sub(p, *face_origin);
        let t = dot(r, basis.n) / dn;
        if !(t >= -t_eps && t <= *depth + t_eps) {
            return false;
        }
        let q = sub(r, scale(*dir, t));
        pu[c] = dot(q, basis.x);
        pv[c] = dot(q, basis.y);
    }
    match profile {
        Profile2::Circle { cx, cy, r } => {
            let r_tol = *r + 1e-11 * r.abs().max(1.0);
            *r > 0.0 && (0..8).all(|c| (pu[c] - cx).hypot(pv[c] - cy) <= r_tol)
        }
        Profile2::Polygon(pts) => {
            let Some((sign, max_edge)) = polygon_convex_sign(pts) else {
                return false;
            };
            let n = pts.len();
            for i in 0..n {
                let (p0, p1) = (pts[i], pts[(i + 1) % n]);
                let (ex, ey) = (p1.0 - p0.0, p1.1 - p0.1);
                let elen = ex.hypot(ey);
                if elen <= 1e-14 * max_edge {
                    continue;
                }
                let tol = 1e-11 * elen * max_edge;
                if !(0..8).all(|c| sign * (ex * (pv[c] - p0.1) - ey * (pu[c] - p0.0)) >= -tol) {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

fn parse_output_anchor(gr: &Value) -> Result<OutputAnchor, String> {
    let anchor = gr.get("anchor").ok_or("without anchor")?;
    match anchor.get("type").and_then(Value::as_str) {
        Some("FeatureOutput") => {
            let feature_id = anchor
                .get("feature_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| "without feature_id".to_string())?;
            let index = match anchor.get("output_key") {
                None => 0,
                Some(k) if k.is_null() => 0,
                Some(k) => match k.get("type").and_then(Value::as_str) {
                    Some("Main") => 0,
                    Some("Body") => k
                        .get("index")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| "OutputKey::Body without index".to_string())?
                        as usize,
                    other => return Err(format!("with output_key {other:?}")),
                },
            };
            Ok(OutputAnchor { feature_id, index })
        }
        other => Err(format!("anchored to {other:?} (only FeatureOutput)")),
    }
}

/// The engine's `normalize_combine`: an explicit `combine` verb with its
/// explicit `targets` (feature-output anchors), or the legacy derivation
/// from `cut` / `merge` / `target_body` targeting the most recent body.
fn parse_combine(params: &Value) -> Result<(Combine, Targets), String> {
    let target_anchor =
        |gr: &Value| parse_output_anchor(gr).map_err(|w| format!("combine target {w}"));
    match params.get("combine") {
        Some(c) if !c.is_null() => {
            let mode = match c.get("type").and_then(Value::as_str) {
                Some("NewBody") => Combine::NewBody,
                Some("Add") => Combine::Add,
                Some("Cut") => Combine::Cut,
                Some("Intersect") => Combine::Intersect,
                other => return Err(format!("combine mode {other:?}")),
            };
            if mode == Combine::NewBody {
                return Ok((mode, Targets::Explicit(Vec::new())));
            }
            match params.get("targets") {
                Some(Value::Array(list)) => {
                    let anchors = list
                        .iter()
                        .map(target_anchor)
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok((mode, Targets::Explicit(anchors)))
                }
                _ => Err("share-a-face auto-targeting (combine set, no targets)".into()),
            }
        }
        _ => {
            let cut = params.get("cut").and_then(Value::as_bool).unwrap_or(false);
            let merge = params.get("merge").and_then(Value::as_bool).unwrap_or(true);
            let mode = if cut {
                Combine::Cut
            } else if merge {
                Combine::Add
            } else {
                Combine::NewBody
            };
            let targets = match params.get("target_body") {
                Some(gr) if !gr.is_null() && mode != Combine::NewBody => {
                    Targets::Explicit(vec![target_anchor(gr)?])
                }
                _ => Targets::MostRecent,
            };
            Ok((mode, targets))
        }
    }
}

/// A `waffle_types::Region` (`outer` + `holes`, sketch coordinates) as a
/// profile.
fn region_profile(region: &Value) -> Result<Profile2, String> {
    fn poly(v: &Value) -> Result<Vec<(f64, f64)>, String> {
        let pts = v
            .as_array()
            .ok_or("region loop is not an array")?
            .iter()
            .map(|p| {
                let a = p.as_array().ok_or("region point")?;
                Ok((
                    a.first().and_then(Value::as_f64).ok_or("region point u")?,
                    a.get(1).and_then(Value::as_f64).ok_or("region point v")?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        if pts.len() < 3 {
            return Err("region loop with fewer than 3 points".into());
        }
        Ok(pts)
    }
    let outer = Profile2::Polygon(poly(region.get("outer").ok_or("region without outer")?)?);
    let holes = match region.get("holes") {
        Some(Value::Array(hs)) => hs
            .iter()
            .map(|h| poly(h).map(Profile2::Polygon))
            .collect::<Result<Vec<_>, _>>()?,
        _ => Vec::new(),
    };
    Ok(if holes.is_empty() {
        outer
    } else {
        Profile2::Region {
            outer: Box::new(outer),
            holes,
        }
    })
}

fn vec3(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    if a.len() != 3 {
        return None;
    }
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}

/// The engine's `THROUGH_ALL_OVERSHOOT` (`feature-engine` `rebuild.rs`): how
/// far past the measured material a through-all cutter runs, as a FRACTION of
/// that material's extent along the sweep axis.
const THROUGH_ALL_OVERSHOOT: f64 = 1e-2;

struct ParsedSketch {
    basis: Basis,
    positions: HashMap<u32, (f64, f64)>,
    profiles: Vec<Value>,
    gear: Option<GearDisc>,
}

impl ParsedSketch {
    fn parse(sk: &Value) -> Result<Self, String> {
        let origin = sk
            .get("plane_origin")
            .and_then(vec3)
            .ok_or("sketch without plane_origin")?;
        let normal = sk
            .get("plane_normal")
            .and_then(vec3)
            .ok_or("sketch without plane_normal")?;
        // An explicit `plane_x_axis` orients the sketch the way the engine
        // orients it (`SketchPlaneBasis::from_origin_normal_x`, the one
        // source); without it, the derived basis. Ignoring it put every
        // point of a rigidly moved or imported sketch in the wrong place
        // (P0001 under the metamorphic motion: the star rotated in its own
        // plane, its overlap with the octagon read empty).
        let basis = match sk.get("plane_x_axis").and_then(vec3) {
            Some(x) => {
                if norm(normal).is_none() {
                    return Err("degenerate plane normal".into());
                }
                let b = waffle_types::sketch_plane::SketchPlaneBasis::from_origin_normal_x(
                    origin,
                    normal,
                    Some(x),
                );
                Basis {
                    origin: b.origin,
                    x: b.x_axis,
                    y: b.y_axis,
                    n: b.normal,
                }
            }
            None => Basis::from_origin_normal(origin, normal).ok_or("degenerate plane normal")?,
        };
        let mut positions = HashMap::new();
        if let Some(obj) = sk.get("solved_positions").and_then(Value::as_object) {
            for (k, v) in obj {
                if let (Ok(id), Some(p)) = (k.parse::<u32>(), v.as_array()) {
                    if let (Some(x), Some(y)) = (
                        p.first().and_then(Value::as_f64),
                        p.get(1).and_then(Value::as_f64),
                    ) {
                        positions.insert(id, (x, y));
                    }
                }
            }
        }
        let mut gear = None;
        if let Some(ents) = sk.get("entities").and_then(Value::as_array) {
            for e in ents {
                match e.get("type").and_then(Value::as_str) {
                    Some("Point") => {
                        if let (Some(id), Some(x), Some(y)) = (
                            e.get("id").and_then(Value::as_u64),
                            e.get("x").and_then(Value::as_f64),
                            e.get("y").and_then(Value::as_f64),
                        ) {
                            positions.entry(id as u32).or_insert((x, y));
                        }
                    }
                    Some("Gear") => {
                        let p = e.get("params").ok_or("gear without params")?;
                        if p.get("internal").and_then(Value::as_bool) == Some(true) {
                            return Err("internal gear".into());
                        }
                        let g = |k: &str| p.get(k).and_then(Value::as_f64);
                        let teeth = p
                            .get("toothCount")
                            .and_then(Value::as_u64)
                            .ok_or("gear toothCount")? as u32;
                        if gear.is_some() {
                            return Err("two gears in one sketch".into());
                        }
                        gear = Some(GearDisc::new(
                            teeth,
                            g("module").ok_or("gear module")?,
                            g("pressureAngleDeg").unwrap_or(20.0),
                            g("backlash").unwrap_or(0.0),
                            g("centerX").unwrap_or(0.0),
                            g("centerY").unwrap_or(0.0),
                            g("rotationOffset").unwrap_or(0.0),
                        ));
                    }
                    _ => {}
                }
            }
        }
        let profiles = sk
            .get("solved_profiles")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(ParsedSketch {
            basis,
            positions,
            profiles,
            gear,
        })
    }

    fn profile(&self, params: &Value) -> Result<Profile2, String> {
        let index = params
            .get("profile_index")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        if let Some(g) = self.gear {
            if index != 0 {
                return Err(format!("gear sketch with profile_index {index}"));
            }
            return Ok(Profile2::Gear(g));
        }
        if index >= self.profiles.len() {
            return Err(format!("profile_index {index} of {}", self.profiles.len()));
        }
        // The adapter's staging (`make_faces_from_profiles`, KV14): one face
        // per input profile. Every loop is parsed; an `is_outer` loop's face
        // is the loop with each `is_outer = false` loop attached whose
        // centroid it contains and whose area it strictly exceeds — each hole
        // going to the SMALLEST such outer; an inner loop's own index stages
        // the loop alone. Circles are always outers.
        let loops = self
            .profiles
            .iter()
            .map(|p| self.parse_loop(p))
            .collect::<Result<Vec<_>, _>>()?;
        let is_outer: Vec<bool> = self
            .profiles
            .iter()
            .zip(&loops)
            .map(|(p, l)| {
                matches!(l, Profile2::Circle { .. })
                    || p.get("is_outer").and_then(Value::as_bool).unwrap_or(true)
            })
            .collect();
        if !is_outer[index] {
            return Ok(loops[index].clone());
        }
        let mut holes = Vec::new();
        for (h, hole) in loops.iter().enumerate() {
            if is_outer[h] {
                continue;
            }
            let Profile2::Polygon(pts) = hole else {
                continue;
            };
            let n = pts.len() as f64;
            let cu = pts.iter().map(|p| p.0).sum::<f64>() / n;
            let cv = pts.iter().map(|p| p.1).sum::<f64>() / n;
            let hole_area = hole.area_abs();
            let container = (0..loops.len())
                .filter(|&j| {
                    j != h
                        && is_outer[j]
                        && loops[j].area_abs() > hole_area * (1.0 + cad_primitives::TAU_EVAL)
                        && loops[j].contains(cu, cv)
                })
                .min_by(|&a, &b| {
                    loops[a]
                        .area_abs()
                        .partial_cmp(&loops[b].area_abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            if container == Some(index) {
                holes.push(hole.clone());
            }
        }
        if holes.is_empty() {
            Ok(loops[index].clone())
        } else {
            Ok(Profile2::Region {
                outer: Box::new(loops[index].clone()),
                holes,
            })
        }
    }

    /// One `solved_profiles` entry as a loop: a circle or a polygon.
    fn parse_loop(&self, prof: &Value) -> Result<Profile2, String> {
        if let Some(c) = prof.get("circle") {
            let f = |k: &str| c.get(k).and_then(Value::as_f64);
            return Ok(Profile2::Circle {
                cx: f("center_u").ok_or("circle center_u")?,
                cy: f("center_v").ok_or("circle center_v")?,
                r: f("radius").ok_or("circle radius")?,
            });
        }
        let ids = prof
            .get("vertex_ids")
            .and_then(Value::as_array)
            .ok_or("profile without vertex_ids or circle")?;
        let mut pts = Vec::with_capacity(ids.len());
        for id in ids {
            let id = id.as_u64().ok_or("vertex id")? as u32;
            let p = self
                .positions
                .get(&id)
                .ok_or_else(|| format!("vertex {id} has no position"))?;
            pts.push(*p);
        }
        if pts.len() < 3 {
            return Err("polygon with fewer than 3 vertices".into());
        }
        Ok(Profile2::Polygon(pts))
    }
}

fn sketch_for<'a>(
    sketches: &'a HashMap<String, ParsedSketch>,
    params: &Value,
) -> Result<&'a ParsedSketch, String> {
    let id = params
        .get("sketch_id")
        .and_then(Value::as_str)
        .ok_or("no sketch_id")?;
    sketches
        .get(id)
        .ok_or_else(|| format!("sketch {id} not defined before this feature"))
}

/// One rung of the exact ladder: the cubical readout plus the cell size and
/// the cube-count volume.
#[derive(Debug, Clone, PartialEq)]
pub struct ExactReadout {
    pub readout: TopologyReadout,
    /// Cell edge length.
    pub h: f64,
    /// Cube counts per axis.
    pub n: [usize; 3],
    /// `cubes · h³`.
    pub volume: f64,
    /// Cube counts per face-connected component, largest first (across all
    /// bodies).
    pub component_sizes: Vec<usize>,
    /// Centroid of the occupied cubes (world coordinates; NaN when empty).
    pub centroid: [f64; 3],
    /// Live bodies after the prefix (the engine's body list), and each
    /// body's own cube volume in that order.
    pub bodies: usize,
    pub body_volumes: Vec<f64>,
    /// Occupied cubes with an empty face neighbour, over all bodies — the
    /// cubes the boundary decided; `surface_cubes · h³` bounds the
    /// reading's volume error.
    pub surface_cubes: usize,
}

impl ExactReadout {
    /// The boundary surface's χ (what the corpus `euler_target` names).
    pub fn boundary_chi(&self) -> i64 {
        2 * self.readout.chi
    }
}

/// Voxelise the composed solid (or its first `prefix` ops) with `cells`
/// cubes along the bounding box's longest axis, sample points at fraction
/// `phase` of each cell, and read the cubical topology. The lattice is laid
/// in the document's first sketch frame ([`ExactChain::frame`]); one cell of
/// padding on every side keeps the solid off the grid boundary.
pub fn readout_exact(
    chain: &ExactChain,
    prefix: usize,
    cells: usize,
    phase: f64,
) -> Option<ExactReadout> {
    let (wlo, whi) = chain.bbox_prefix(prefix)?;
    // The lattice lives in the first sketch's frame: bound the world box's
    // corners in frame coordinates (conservative — a rotated box grows).
    let f = &chain.frame;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for c in 0..8 {
        let p = [
            if c & 1 == 0 { wlo[0] } else { whi[0] },
            if c & 2 == 0 { wlo[1] } else { whi[1] },
            if c & 4 == 0 { wlo[2] } else { whi[2] },
        ];
        let r = sub(p, f.origin);
        let q = [dot(r, f.x), dot(r, f.y), dot(r, f.n)];
        for k in 0..3 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let longest = ext.iter().cloned().fold(0.0, f64::max);
    if !longest.is_finite() || longest <= 0.0 || cells == 0 {
        return None;
    }
    let h = longest / cells as f64;
    let n = [
        ((ext[0] / h).ceil() as usize + 2).max(1),
        ((ext[1] / h).ceil() as usize + 2).max(1),
        ((ext[2] / h).ceil() as usize + 2).max(1),
    ];
    let start = [lo[0] - h, lo[1] - h, lo[2] - h];
    // One grid per live body on the SAME lattice; the readings are summed
    // (the kernel emits one mesh per body, and the runner's oracles read
    // their concatenation).
    let bodies = chain.bodies_after(prefix);
    let mut readout = TopologyReadout {
        n: 0,
        cubes: 0,
        chi: 0,
        components: 0,
    };
    let mut component_sizes = Vec::new();
    let mut body_volumes = Vec::with_capacity(bodies.len());
    let mut surface_cubes = 0usize;
    let mut acc = [0.0f64; 3];
    let mut count = 0usize;
    for body in &bodies {
        let grid = VoxelGrid::from_fn(n, |i, j, k| {
            let (u, v, w) = (
                start[0] + (i as f64 + phase) * h,
                start[1] + (j as f64 + phase) * h,
                start[2] + (k as f64 + phase) * h,
            );
            let p = add(
                f.origin,
                add(add(scale(f.x, u), scale(f.y, v)), scale(f.n, w)),
            );
            chain.expr_contains(&body.expr, p)
        });
        let r = grid.readout();
        readout.n = r.n;
        readout.cubes += r.cubes;
        readout.chi += r.chi;
        readout.components += r.components;
        body_volumes.push(r.cubes as f64 * h * h * h);
        surface_cubes += grid.surface_cubes();
        component_sizes.extend(grid.component_sizes());
        // Centroid of the occupied cubes, in world coordinates.
        for k in 0..n[2] {
            for j in 0..n[1] {
                for i in 0..n[0] {
                    if grid.occupied(i, j, k) {
                        acc[0] += start[0] + (i as f64 + phase) * h;
                        acc[1] += start[1] + (j as f64 + phase) * h;
                        acc[2] += start[2] + (k as f64 + phase) * h;
                        count += 1;
                    }
                }
            }
        }
    }
    component_sizes.sort_unstable_by(|a, b| b.cmp(a));
    let volume = readout.cubes as f64 * h * h * h;
    let centroid = if count > 0 {
        let (u, v, w) = (
            acc[0] / count as f64,
            acc[1] / count as f64,
            acc[2] / count as f64,
        );
        add(
            f.origin,
            add(add(scale(f.x, u), scale(f.y, v)), scale(f.n, w)),
        )
    } else {
        [f64::NAN; 3]
    };
    Some(ExactReadout {
        readout,
        h,
        n,
        volume,
        component_sizes,
        centroid,
        bodies: bodies.len(),
        body_volumes,
        surface_cubes,
    })
}

/// The exact-volume oracle's verdict for one case (the categorized runner's
/// in-line check, `assay_kv2`): the kernel's result volume — the signed
/// volume of its tessellated live bodies, summed — against the exact chain's
/// lattice volume at 256 cells.
#[derive(Debug, Clone, PartialEq)]
pub enum ExactVolumeVerdict {
    /// Within the lattice's own convergence band.
    Agree { rel: f64, band: f64 },
    /// Outside it: the kernel's result is not the document's solid.
    Flag {
        rel: f64,
        band: f64,
        exact: f64,
        kernel: f64,
    },
    /// No honest expectation: the lattice has not converged on this
    /// document (thin features below the cell size) or reads empty.
    NotCovered(String),
}

/// Relative floor under the band: the kernel side is a tessellation at the
/// oracle tolerance (`oracle_tol`, chord sagitta ≈ scale · 1e-4), whose
/// signed volume differs from the curved solid's by a few 1e-3 at most.
pub const EXACT_VOLUME_FLOOR_REL: f64 = 5e-3;
/// A reading whose surface cubes exceed this fraction of its volume is too
/// coarse to author an expectation (a sub-cell wall, a lattice-thin slab).
pub const EXACT_VOLUME_TOO_COARSE_REL: f64 = 0.25;
/// Cells along the longest axis for the verdict's reading.
pub const EXACT_VOLUME_CELLS: usize = 256;

/// Read the chain at [`EXACT_VOLUME_CELLS`]; the band is the reading's own
/// uncertainty — the volume of the cubes the boundary decided
/// (`surface_cubes · h³`, measured 2026-09-04 to cover both the
/// phase-dependent quantisation of thin features and the fixed layer
/// rounding of a thin slab, which a rung-to-rung step cannot see) — plus
/// the tessellation floor. A band converts a silent-wrong into a loud
/// verdict and nothing else: the classes this oracle exists for (a cut
/// that removes nothing, a complementary wedge) sit at 7–90 %. A chain the
/// document leaves indeterminate (a mid-extent cut) is declined.
pub fn exact_volume_verdict(chain: &ExactChain, kernel_volume: f64) -> ExactVolumeVerdict {
    let empty_verdict = |kernel_volume: f64| {
        if kernel_volume == 0.0 {
            ExactVolumeVerdict::Agree {
                rel: 0.0,
                band: 0.0,
            }
        } else {
            let rel = if kernel_volume.is_nan() {
                f64::NAN
            } else {
                f64::INFINITY
            };
            ExactVolumeVerdict::Flag {
                rel,
                band: 0.0,
                exact: 0.0,
                kernel: kernel_volume,
            }
        }
    };
    if let Some(why) = chain.indeterminate.first() {
        return ExactVolumeVerdict::NotCovered(why.clone());
    }
    let Some(r) = readout_exact(chain, chain.ops.len(), EXACT_VOLUME_CELLS, 0.5) else {
        if chain.certified_empty_at(chain.ops.len(), EXACT_VOLUME_CELLS) {
            return empty_verdict(kernel_volume);
        }
        return ExactVolumeVerdict::NotCovered("no bounding box (no live body)".into());
    };
    let exact = r.volume;
    if exact <= 0.0 {
        if chain.certified_empty_at(chain.ops.len(), EXACT_VOLUME_CELLS) {
            return empty_verdict(kernel_volume);
        }
        return ExactVolumeVerdict::NotCovered(format!(
            "exact reads empty at {EXACT_VOLUME_CELLS} cells — sub-lattice or empty (kernel {kernel_volume:.6e})"
        ));
    }
    let surface = r.surface_cubes as f64 * r.h * r.h * r.h / exact;
    if surface > EXACT_VOLUME_TOO_COARSE_REL {
        return ExactVolumeVerdict::NotCovered(format!(
            "lattice too coarse: surface cubes are {:.0} % of the volume at {EXACT_VOLUME_CELLS} cells (h = {:.3e})",
            surface * 100.0,
            r.h
        ));
    }
    let band = surface + EXACT_VOLUME_FLOOR_REL;
    let rel = (kernel_volume - exact) / exact;
    if rel.abs() <= band {
        ExactVolumeVerdict::Agree { rel, band }
    } else {
        ExactVolumeVerdict::Flag {
            rel,
            band,
            exact,
            kernel: kernel_volume,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_box_doc(second: Option<(&str, f64)>, cut_second_box: bool) -> Value {
        // A 1×1 square on z=0 extruded 1 up; optionally a second square cut.
        let mut feats = vec![
            serde_json::json!({
                "id": "s1", "name": "Sketch 1", "suppressed": false,
                "operation": {"type": "Sketch", "sketch": {
                    "id": "s1",
                    "plane_origin": [0.0, 0.0, 0.0], "plane_normal": [0.0, 0.0, 1.0],
                    "entities": [
                        {"type":"Point","id":1,"x":0.0,"y":0.0},{"type":"Point","id":2,"x":1.0,"y":0.0},
                        {"type":"Point","id":3,"x":1.0,"y":1.0},{"type":"Point","id":4,"x":0.0,"y":1.0}
                    ],
                    "solved_profiles": [{"entity_ids":[1,2,3,4],"is_outer":true,"vertex_ids":[1,2,3,4]}]
                }}
            }),
            serde_json::json!({
                "id": "e1", "name": "Extrude 1", "suppressed": false,
                "operation": {"type": "Extrude", "params": {
                    "sketch_id": "s1", "profile_index": 0, "depth": 1.0, "direction": null,
                    "symmetric": false, "cut": false, "merge": true,
                    "depth_mode": {"type": "Blind"},
                    "second_direction": second.map(|(t, d)| serde_json::json!({"type": t, "depth": d}))
                }}
            }),
        ];
        if cut_second_box {
            feats.push(serde_json::json!({
                "id": "s2", "name": "Sketch 2", "suppressed": false,
                "operation": {"type": "Sketch", "sketch": {
                    "id": "s2",
                    "plane_origin": [0.0, 0.0, 1.0], "plane_normal": [0.0, 0.0, 1.0],
                    "entities": [{"type":"Circle","id":9,"center_id":1,"radius":0.25}],
                    "solved_profiles": [{"entity_ids":[9],"is_outer":true,"circle":{"center_u":0.5,"center_v":0.5,"radius":0.25}}]
                }}
            }));
            feats.push(serde_json::json!({
                "id": "e2", "name": "Extrude 2", "suppressed": false,
                "operation": {"type": "Extrude", "params": {
                    "sketch_id": "s2", "profile_index": 0, "depth": 2.0, "direction": null,
                    "symmetric": false, "cut": true, "merge": true,
                    "depth_mode": {"type": "Blind"}, "second_direction": null
                }}
            }));
        }
        serde_json::json!({"tabs": [{"kind": {"type": "Part", "features": {"features": feats}}}]})
    }

    // On a +z-normal plane the UI basis is x̂ = (0, −1, 0), ŷ = (1, 0, 0):
    // sketch (u, v) ↦ world (v, −u, ·). The unit square therefore occupies
    // x ∈ [0, 1], y ∈ [−1, 0].
    // An explicit `plane_x_axis` is the sketch's x̂ (projected into the
    // plane), exactly as the engine reads it — not the derived basis.
    #[test]
    fn an_explicit_plane_x_axis_orients_the_sketch() {
        let sk = serde_json::json!({
            "plane_origin": [0.0, 0.0, 0.0], "plane_normal": [0.0, 0.0, 1.0],
            "plane_x_axis": [1.0, 0.0, 0.5],
        });
        let b = ParsedSketch::parse(&sk).expect("parses").basis;
        assert_eq!(b.x, [1.0, 0.0, 0.0]);
        assert_eq!(b.y, [0.0, 1.0, 0.0]);
        let derived = ParsedSketch::parse(&serde_json::json!({
            "plane_origin": [0.0, 0.0, 0.0], "plane_normal": [0.0, 0.0, 1.0],
        }))
        .expect("parses")
        .basis;
        assert_eq!(derived.x, [0.0, -1.0, 0.0]);
    }

    #[test]
    fn basis_on_a_z_plane_maps_u_to_minus_y_and_v_to_x() {
        let b = Basis::from_origin_normal([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]).unwrap();
        assert_eq!(b.x, [0.0, -1.0, 0.0]);
        assert_eq!(b.y, [1.0, 0.0, 0.0]);
        assert_eq!(b.local([0.3, -0.7, 5.0]), (0.7, 0.3));
    }

    #[test]
    fn unit_box_is_a_ball_of_volume_one() {
        let chain = ExactChain::from_waffle(&unit_box_doc(None, false)).unwrap();
        assert!(chain.contains([0.5, -0.5, 0.5]));
        assert!(!chain.contains([0.5, -0.5, 1.5]));
        assert!(!chain.contains([0.5, 0.5, 0.5]), "the +y side is outside");
        assert!(!chain.contains([1.5, -0.5, 0.5]));
        let r = readout_exact(&chain, 1, 40, 0.5).unwrap();
        assert_eq!((r.readout.chi, r.readout.components), (1, 1));
        assert!((r.volume - 1.0).abs() < 0.02, "{}", r.volume);
    }

    #[test]
    fn symmetric_second_direction_doubles_the_prism() {
        let chain =
            ExactChain::from_waffle(&unit_box_doc(Some(("Symmetric", 0.0)), false)).unwrap();
        assert!(chain.contains([0.5, -0.5, -0.5]));
        assert!(chain.contains([0.5, -0.5, 0.5]));
        assert!(!chain.contains([0.5, -0.5, 1.5]));
        assert!(!chain.contains([0.5, -0.5, -1.5]));
        let r = readout_exact(&chain, 1, 40, 0.5).unwrap();
        assert!((r.volume - 2.0).abs() < 0.04, "{}", r.volume);
    }

    #[test]
    fn through_hole_cut_from_the_top_face_auto_reverses_into_the_body() {
        // The circle sketch sits on the top face (z = 1); the cut has no
        // explicit direction, so the engine reverses it into the body.
        let chain = ExactChain::from_waffle(&unit_box_doc(None, true)).unwrap();
        assert!(
            chain.notes.iter().any(|n| n.contains("auto-reversed")),
            "{:?}",
            chain.notes
        );
        assert!(!chain.contains([0.5, -0.5, 0.5]), "hole through the centre");
        assert!(chain.contains([0.1, -0.1, 0.5]));
        let r = readout_exact(&chain, 2, 64, 0.5).unwrap();
        assert_eq!((r.readout.chi, r.readout.components), (0, 1), "{r:?}");
        let expected = 1.0 - std::f64::consts::PI * 0.25 * 0.25;
        assert!(
            (r.volume - expected).abs() < 0.02,
            "{} vs {expected}",
            r.volume
        );
    }

    #[test]
    fn gear_disc_matches_the_generator_layout() {
        // 16 teeth, module 7.455 (R0053's gear): tooth 0 centred on +x.
        let g = GearDisc::new(16, 7.4553217080021374, 20.0, 0.0, 0.0, 0.0, 0.0);
        assert!((g.root_r - 50.3234).abs() < 1e-3);
        assert!((g.add_r - 67.0979).abs() < 1e-3);
        assert!(g.contains(60.0, 0.0), "on tooth 0's centre line");
        assert!(
            !g.contains(
                60.0 * (11.25f64.to_radians()).cos(),
                60.0 * (11.25f64.to_radians()).sin()
            ),
            "the groove between teeth 0 and 1"
        );
        assert!(g.contains(45.0, 20.0), "inside the root circle");
        assert!(!g.contains(70.0, 0.0), "beyond the addendum");
        // Rotation offset moves the tooth.
        let r = GearDisc::new(
            16,
            7.4553217080021374,
            20.0,
            0.0,
            0.0,
            0.0,
            11.25f64.to_radians(),
        );
        assert!(!r.contains(60.0, 0.0));
    }

    #[test]
    fn revolve_of_a_square_about_an_in_plane_axis_is_a_washer() {
        // Square (u, v) ∈ [0, 1] × [1, 2] on the z-plane — world x ∈ [1, 2],
        // y ∈ [−1, 0] — revolved 360° about the world y axis: a full ring
        // of radial band [1, 2], χ_solid 0, volume π(2² − 1²)·1 = 3π.
        let doc = serde_json::json!({"tabs": [{"kind": {"type": "Part", "features": {"features": [
            {"id":"s1","name":"Sketch 1","suppressed":false,"operation":{"type":"Sketch","sketch":{
                "id":"s1","plane_origin":[0.0,0.0,0.0],"plane_normal":[0.0,0.0,1.0],
                "entities":[{"type":"Point","id":1,"x":0.0,"y":1.0},{"type":"Point","id":2,"x":1.0,"y":1.0},
                            {"type":"Point","id":3,"x":1.0,"y":2.0},{"type":"Point","id":4,"x":0.0,"y":2.0}],
                "solved_profiles":[{"entity_ids":[1,2,3,4],"is_outer":true,"vertex_ids":[1,2,3,4]}]}}},
            {"id":"r1","name":"Revolve 1","suppressed":false,"operation":{"type":"Revolve","params":{
                "sketch_id":"s1","profile_index":0,"axis_origin":[0.0,0.0,0.0],"axis_direction":[0.0,1.0,0.0],
                "angle":360.0,"cut":false,"merge":true}}}
        ]}}}]});
        let chain = ExactChain::from_waffle(&doc).unwrap();
        assert!(chain.contains([1.5, -0.5, 0.0]), "the profile itself");
        assert!(
            chain.contains([-1.5, -0.5, 0.0]),
            "the far side of a full turn"
        );
        assert!(chain.contains([0.0, -0.5, 1.5]));
        assert!(!chain.contains([0.0, -0.5, 0.0]), "the hole");
        assert!(!chain.contains([1.5, 0.5, 0.0]), "beyond the axial extent");
        let r = readout_exact(&chain, 1, 64, 0.5).unwrap();
        assert_eq!((r.readout.chi, r.readout.components), (0, 1), "{r:?}");
        let expected = 3.0 * std::f64::consts::PI;
        assert!(
            (r.volume - expected).abs() < 0.05 * expected,
            "{} vs {expected}",
            r.volume
        );
        // A 180° revolve is a half-washer: a ball, half the volume; the
        // sweep leaves toward m̂ = â × ŵ = ŷ × x̂ = −ẑ first.
        let mut half = doc.clone();
        half["tabs"][0]["kind"]["features"]["features"][1]["operation"]["params"]["angle"] =
            serde_json::json!(180.0);
        let chain = ExactChain::from_waffle(&half).unwrap();
        assert!(chain.contains([0.0, -0.5, -1.5]), "sweep leaves toward −z");
        assert!(!chain.contains([0.0, -0.5, 1.5]));
        assert!(chain.contains([-1.5, -0.5, 0.0]), "the 180° end");
        let r = readout_exact(&chain, 1, 64, 0.5).unwrap();
        assert_eq!((r.readout.chi, r.readout.components), (1, 1), "{r:?}");
        assert!(
            (r.volume - expected / 2.0).abs() < 0.05 * expected,
            "{}",
            r.volume
        );
    }

    #[test]
    fn region_extrude_and_boolean_combine_are_typed_not_covered() {
        let mut doc = unit_box_doc(None, false);
        doc["tabs"][0]["kind"]["features"]["features"][1]["operation"]["params"]["regions"] =
            serde_json::json!([{"x": 0}]);
        let err = ExactChain::from_waffle(&doc).unwrap_err();
        assert!(
            matches!(err, NotCovered::Feature { ref why, .. } if why.contains("region")),
            "{err}"
        );
        let mut doc = unit_box_doc(None, false);
        doc["tabs"][0]["kind"]["features"]["features"][1]["operation"] =
            serde_json::json!({"type": "BooleanCombine", "params": {}});
        let err = ExactChain::from_waffle(&doc).unwrap_err();
        assert!(
            matches!(err, NotCovered::Feature { ref why, .. } if why.contains("BooleanCombine")),
            "{err}"
        );
    }

    /// P0007 (2026-09-29): an explicit Union combine of two live bodies
    /// folds them into ONE set-union body — the overlap is no longer
    /// counted twice and the body count drops to one — while adding no
    /// operand (the two overlapping NewBody boxes of
    /// `new_body_overlapping_counts_twice`: 1 + 0.64 summed, 1 + 0.64 −
    /// 0.04 unioned).
    #[test]
    fn union_combine_folds_two_bodies_into_one() {
        let combine = |id: &str, a: &str, b: &str| {
            let gr = |f: &str| {
                serde_json::json!({"kind": {"type": "Solid"},
                    "anchor": {"type": "FeatureOutput", "feature_id": f, "output_key": {"type": "Main"}},
                    "selector": {"type": "Role", "role": {"type": "EndCapPositive"}, "index": 0},
                    "policy": {"type": "BestEffort"}})
            };
            serde_json::json!({
                "id": id, "name": "Boolean Combine", "suppressed": false,
                "operation": {"type": "BooleanCombine", "params": {
                    "body_a": gr(a), "body_b": gr(b), "operation": {"type": "Union"}}}
            })
        };
        let chain = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0; 3], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.3, 0.2, 0.4], &[(0.0, 0.0, 0.4, 0.4, true)]),
            extrude(
                "e2",
                "s2",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            combine("u1", "e1", "e2"),
        ]))
        .unwrap();
        assert_eq!(chain.ops.len(), 2, "the combine adds no operand");
        assert_eq!(chain.merges.len(), 1);
        let bodies = chain.bodies_after(2);
        assert_eq!(bodies.len(), 1, "{bodies:?}");
        assert!(bodies[0].ids.iter().any(|id| id == "u1"), "{bodies:?}");
        let r = readout_exact(&chain, 2, 128, 0.5).unwrap();
        assert_eq!(r.bodies, 1, "{r:?}");
        // Overlap: the second box spans x ∈ [−0.1, 0.7], y ∈ [−0.2, 0.6],
        // z ∈ [0.4, 1.4]; inside the unit box ([−0.5, 0.5]² × [0, 1]) that is
        // 0.6 · 0.7 · 0.6 = 0.252 — union 1 + 0.64 − 0.252 = 1.388.
        assert!(
            (r.volume - (1.0 + 0.64 - 0.252)).abs() < 0.03,
            "{}",
            r.volume
        );
        // A Subtract / Intersect combine re-authors the set (2026-10-13):
        // the same two boxes compose to `e1 − e2` = 1 − 0.252 and
        // `e1 ∩ e2` = 0.252, one body each, in anchor order.
        for (verb, expect) in [("Subtract", 1.0 - 0.252), ("Intersect", 0.252)] {
            let mut c = combine("c1", "e1", "e2");
            c["operation"]["params"]["operation"] = serde_json::json!({"type": verb});
            let chain = ExactChain::from_waffle(&doc(vec![
                square_sketch("s1", [0.0; 3], &[(0.0, 0.0, 0.5, 0.5, true)]),
                extrude("e1", "s1", 1.0, false, serde_json::json!({})),
                square_sketch("s2", [0.3, 0.2, 0.4], &[(0.0, 0.0, 0.4, 0.4, true)]),
                extrude(
                    "e2",
                    "s2",
                    1.0,
                    false,
                    serde_json::json!({"combine": {"type": "NewBody"}}),
                ),
                c,
            ]))
            .unwrap();
            assert_eq!(chain.ops.len(), 2, "{verb}: the combine adds no operand");
            let bodies = chain.bodies_after(2);
            assert_eq!(bodies.len(), 1, "{verb}: {bodies:?}");
            assert!(
                bodies[0].ids.iter().any(|id| id == "c1"),
                "{verb}: {bodies:?}"
            );
            let expected_expr = if verb == "Subtract" {
                Expr::Sub(Box::new(Expr::Leaf(0)), Box::new(Expr::Leaf(1)))
            } else {
                Expr::Inter(Box::new(Expr::Leaf(0)), Box::new(Expr::Leaf(1)))
            };
            assert_eq!(
                bodies[0].expr, expected_expr,
                "{verb}: anchor order is a then b"
            );
            let r = readout_exact(&chain, 2, 128, 0.5).unwrap();
            assert_eq!(r.bodies, 1, "{verb}: {r:?}");
            assert!((r.volume - expect).abs() < 0.03, "{verb}: {}", r.volume);
        }
        // Anchor order: `e2 − e1` is the second box less the overlap.
        let mut c = combine("c2", "e2", "e1");
        c["operation"]["params"]["operation"] = serde_json::json!({"type": "Subtract"});
        let chain = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0; 3], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.3, 0.2, 0.4], &[(0.0, 0.0, 0.4, 0.4, true)]),
            extrude(
                "e2",
                "s2",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            c,
        ]))
        .unwrap();
        let r = readout_exact(&chain, 2, 128, 0.5).unwrap();
        assert!((r.volume - (0.64 - 0.252)).abs() < 0.03, "{}", r.volume);
        // A new-style combine with no explicit targets is share-a-face
        // auto-targeting: typed out.
        let mut doc = unit_box_doc(None, false);
        doc["tabs"][0]["kind"]["features"]["features"][1]["operation"]["params"]["combine"] =
            serde_json::json!({"type": "Add"});
        let err = ExactChain::from_waffle(&doc).unwrap_err();
        assert!(
            matches!(err, NotCovered::Feature { ref why, .. } if why.contains("share-a-face")),
            "{err}"
        );
        // The app's redundant same-loop pairing (a loop emitted twice) stages
        // profile 0 as that loop — the adapter's strictly-larger rule keeps a
        // twin from becoming a hole equal to its outer.
        let mut doc = unit_box_doc(None, false);
        let profiles = &mut doc["tabs"][0]["kind"]["features"]["features"][0]["operation"]
            ["sketch"]["solved_profiles"];
        let mut twin = profiles[0].clone();
        twin["is_outer"] = serde_json::json!(false);
        profiles.as_array_mut().unwrap().push(twin);
        let chain = ExactChain::from_waffle(&doc).unwrap();
        let r = readout_exact(&chain, 1, 64, 0.5).unwrap();
        assert!((r.volume - 1.0).abs() < 0.02, "{}", r.volume);
    }

    // ---- bodies, holes and combine modes ---------------------------------

    /// A rectangle sketch feature on the plane through `origin` (normal
    /// +z): loops `(cu, cv, half_u, half_v, is_outer)` in sketch coordinates
    /// (`u` → world −y, `v` → world +x on a +z plane) — ids 1.. for the
    /// first loop, 21.. for the second, and so on.
    fn square_sketch(id: &str, origin: [f64; 3], loops: &[(f64, f64, f64, f64, bool)]) -> Value {
        let mut entities = Vec::new();
        let mut profiles = Vec::new();
        for (l, &(cu, cv, hu, hv, is_outer)) in loops.iter().enumerate() {
            let base = 1 + 20 * l as u64;
            let corners = [
                (cu - hu, cv - hv),
                (cu + hu, cv - hv),
                (cu + hu, cv + hv),
                (cu - hu, cv + hv),
            ];
            let mut ids = Vec::new();
            for (k, (x, y)) in corners.iter().enumerate() {
                entities.push(serde_json::json!({"type":"Point","id":base + k as u64,"x":x,"y":y}));
                ids.push(base + k as u64);
            }
            profiles.push(
                serde_json::json!({"entity_ids": ids, "is_outer": is_outer, "vertex_ids": ids}),
            );
        }
        serde_json::json!({
            "id": id, "name": format!("Sketch {id}"), "suppressed": false,
            "operation": {"type": "Sketch", "sketch": {
                "id": id, "plane_origin": origin, "plane_normal": [0.0, 0.0, 1.0],
                "entities": entities, "solved_profiles": profiles
            }}
        })
    }

    /// An extrude feature; `extra` merges further params (`combine`,
    /// `targets`, `profile_index`, …).
    fn extrude(id: &str, sketch: &str, depth: f64, cut: bool, extra: Value) -> Value {
        let mut params = serde_json::json!({
            "sketch_id": sketch, "profile_index": 0, "depth": depth, "direction": null,
            "symmetric": false, "cut": cut, "merge": true,
            "depth_mode": {"type": "Blind"}, "second_direction": null
        });
        if let Some(obj) = extra.as_object() {
            for (k, v) in obj {
                params[k] = v.clone();
            }
        }
        serde_json::json!({
            "id": id, "name": format!("Extrude {id}"), "suppressed": false,
            "operation": {"type": "Extrude", "params": params}
        })
    }

    fn target(feature_id: &str) -> Value {
        serde_json::json!({
            "kind": {"type": "Solid"},
            "anchor": {"type": "FeatureOutput", "feature_id": feature_id, "output_key": {"type": "Main"}},
            "selector": {"type": "Role", "role": {"type": "EndCapPositive"}, "index": 0},
            "policy": {"type": "BestEffort"}
        })
    }

    fn doc(features: Vec<Value>) -> Value {
        serde_json::json!({"tabs": [{"kind": {"type": "Part", "features": {"features": features}}}]})
    }

    /// C0091's shape: a 2 × 2 square with a 1 × 1 hole, 0.5 deep — the
    /// hole is attached to the outer (volume 1.5, genus 1), and the hole's
    /// own index stages the hole loop as a standalone face (volume 0.5).
    #[test]
    fn holed_profile_reads_the_annulus_and_its_hole_index_the_plug() {
        let sk = square_sketch(
            "s1",
            [0.0; 3],
            &[(0.0, 0.0, 1.0, 1.0, true), (0.0, 0.0, 0.5, 0.5, false)],
        );
        let chain = ExactChain::from_waffle(&doc(vec![
            sk.clone(),
            extrude("e1", "s1", 0.5, false, serde_json::json!({})),
        ]))
        .unwrap();
        assert!(chain.contains([0.75, 0.0, 0.25]), "the band");
        assert!(!chain.contains([0.0, 0.0, 0.25]), "the hole");
        let r = readout_exact(&chain, 1, 96, 0.5).unwrap();
        assert_eq!(
            (r.readout.chi, r.readout.components, r.bodies),
            (0, 1, 1),
            "{r:?}"
        );
        assert!((r.volume - 1.5).abs() < 0.03, "{}", r.volume);

        let plug = ExactChain::from_waffle(&doc(vec![
            sk,
            extrude(
                "e1",
                "s1",
                0.5,
                false,
                serde_json::json!({"profile_index": 1}),
            ),
        ]))
        .unwrap();
        assert!(plug.contains([0.0, 0.0, 0.25]));
        assert!(!plug.contains([0.75, 0.0, 0.25]));
        let r = readout_exact(&plug, 1, 64, 0.5).unwrap();
        assert!((r.volume - 0.5).abs() < 0.02, "{}", r.volume);
    }

    /// C0081's shape: a unit cube and a unit cube offset by (0.3, 0.2, 0.4)
    /// with `Intersect` targeting the first — the overlap block, 0.336.
    #[test]
    fn intersect_combine_keeps_the_overlap_block() {
        let chain = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0; 3], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.3, 0.2, 0.4], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude(
                "e2",
                "s2",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "Intersect"}, "targets": [target("e1")]}),
            ),
        ]))
        .unwrap();
        let bodies = chain.bodies_after(2);
        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0].ids, vec!["e1".to_string(), "e2".to_string()]);
        assert!(chain.contains([0.0, 0.0, 0.7]), "inside both");
        assert!(!chain.contains([0.0, 0.0, 0.2]), "below the second cube");
        assert!(!chain.contains([0.6, 0.0, 0.7]), "past the first cube");
        let r = readout_exact(&chain, 2, 128, 0.5).unwrap();
        assert_eq!(
            (r.readout.chi, r.readout.components, r.bodies),
            (1, 1, 1),
            "{r:?}"
        );
        assert!((r.volume - 0.336).abs() < 0.01, "{}", r.volume);
    }

    /// C0083's shape: a unit cube and an overlapping `NewBody` cube — two
    /// independent bodies whose volumes SUM (1 + 0.64) and whose components
    /// count separately, unlike their set union.
    #[test]
    fn new_body_overlapping_counts_twice() {
        let chain = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0; 3], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.3, 0.2, 0.4], &[(0.0, 0.0, 0.4, 0.4, true)]),
            extrude(
                "e2",
                "s2",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
        ]))
        .unwrap();
        assert_eq!(chain.bodies_after(2).len(), 2);
        let r = readout_exact(&chain, 2, 128, 0.5).unwrap();
        assert_eq!(
            (r.readout.chi, r.readout.components, r.bodies),
            (2, 2, 2),
            "{r:?}"
        );
        assert!((r.volume - 1.64).abs() < 0.03, "{}", r.volume);
        assert!(
            (r.body_volumes[0] - 1.0).abs() < 0.02,
            "{:?}",
            r.body_volumes
        );
        assert!(
            (r.body_volumes[1] - 0.64).abs() < 0.02,
            "{:?}",
            r.body_volumes
        );
        // The legacy `merge: false` flag is the same verb.
        let legacy = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0; 3], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.3, 0.2, 0.4], &[(0.0, 0.0, 0.4, 0.4, true)]),
            extrude("e2", "s2", 1.0, false, serde_json::json!({"merge": false})),
        ]))
        .unwrap();
        assert_eq!(legacy.bodies_after(2).len(), 2);
    }

    /// C0079's shape: two `NewBody` cubes bridged by an `Add` with explicit
    /// targets `[A, B]` — one dumbbell body; C0080's: a `Cut` with an
    /// explicit target pockets that body only, and a later legacy cut acts
    /// on the most recent feature's body.
    #[test]
    fn explicit_targets_fold_add_and_scope_cut() {
        let chain = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [-1.5, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [1.5, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e2", "s2", 1.0, false, serde_json::json!({"combine": {"type": "NewBody"}})),
            square_sketch("s3", [0.0, 0.0, 0.25], &[(0.0, 0.0, 0.25, 1.5, true)]),
            extrude(
                "e3",
                "s3",
                0.5,
                false,
                serde_json::json!({"combine": {"type": "Add"}, "targets": [target("e1"), target("e2")]}),
            ),
        ]))
        .unwrap();
        let bodies = chain.bodies_after(3);
        assert_eq!(bodies.len(), 1, "{bodies:?}");
        assert_eq!(bodies[0].ids, ["e1", "e2", "e3"]);
        let r = readout_exact(&chain, 3, 128, 0.5).unwrap();
        assert_eq!((r.readout.chi, r.readout.components), (1, 1), "{r:?}");
        assert!((r.volume - 2.5).abs() < 0.03, "{}", r.volume);

        let chain = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [-2.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude(
                "e2",
                "s2",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            square_sketch("s3", [2.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude(
                "e3",
                "s3",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            // A 0.4-square pocket from z = 2 down 1.5 into body B only.
            square_sketch("s4", [0.0, 0.0, 2.0], &[(0.0, 0.0, 0.2, 0.2, true)]),
            extrude(
                "e4",
                "s4",
                1.5,
                true,
                serde_json::json!({"combine": {"type": "Cut"}, "targets": [target("e2")]}),
            ),
            // A legacy cut: most recent feature's body = the pocketed B.
            square_sketch("s5", [0.0, 0.0, 1.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e5", "s5", 0.3, true, serde_json::json!({})),
        ]))
        .unwrap();
        let bodies = chain.bodies_after(5);
        assert_eq!(bodies.len(), 3, "{bodies:?}");
        assert_eq!(bodies[1].ids, ["e2", "e4", "e5"]);
        assert!(!chain.contains([0.0, 0.0, 0.6]), "the pocket");
        assert!(chain.contains([0.0, 0.0, 0.4]), "under the pocket");
        assert!(
            chain.contains([-2.0, 0.0, 0.9]),
            "A untouched by either cut"
        );
        assert!(
            !chain.contains([0.3, 0.3, 0.8]),
            "the legacy cut's slab off B"
        );
        let r = readout_exact(&chain, 5, 128, 0.5).unwrap();
        assert_eq!((r.readout.components, r.bodies), (3, 3), "{r:?}");
        // A: 1; B: 1 − 0.16·0.5 (pocket to z = 0.5) − slab 0.3 + the pocket's
        // share of the slab already removed (0.16·0.3): 1 − 0.08 − 0.3 + 0.048.
        // A 5-wide lattice at 128 cells reads a unit cube as 26³ h³ ≈ 1.048.
        assert!(
            (r.body_volumes[0] - 1.0).abs() < 0.06,
            "{:?}",
            r.body_volumes
        );
        assert!(
            (r.body_volumes[1] - 0.668).abs() < 0.06,
            "{:?}",
            r.body_volumes
        );
        assert!(
            (r.body_volumes[2] - 1.0).abs() < 0.06,
            "{:?}",
            r.body_volumes
        );
    }

    /// The runner's verdict: a unit cube with a quarter-radius through-hole
    /// agrees with its own volume and with a 0.3 % perturbation, and flags a
    /// kernel that kept the hole's material (+ 20 %); a chain the lattice
    /// cannot resolve is NotCovered, not a verdict.
    #[test]
    fn exact_volume_verdict_agrees_flags_and_declines() {
        let chain = ExactChain::from_waffle(&unit_box_doc(None, true)).unwrap();
        let exact = readout_exact(&chain, 2, 256, 0.5).unwrap().volume;
        let hole = std::f64::consts::PI * 0.25 * 0.25;
        assert!((exact - (1.0 - hole)).abs() < 0.01, "{exact}");
        assert!(matches!(
            exact_volume_verdict(&chain, exact),
            ExactVolumeVerdict::Agree { .. }
        ));
        assert!(matches!(
            exact_volume_verdict(&chain, exact * 1.003),
            ExactVolumeVerdict::Agree { .. }
        ));
        match exact_volume_verdict(&chain, 1.0) {
            ExactVolumeVerdict::Flag { rel, band, .. } => {
                assert!(rel > 0.2 && rel < 0.3, "{rel}");
                // Six faces plus the bore at 256 cells: a few per cent.
                assert!(band > 0.02 && band < 0.06, "{band}");
            }
            other => panic!("{other:?}"),
        }
        // A 1e-4-thick slab across a unit footprint reads as one cell layer
        // — every cube is a surface cube: too coarse, typed out.
        let mut thin = unit_box_doc(None, false);
        thin["tabs"][0]["kind"]["features"]["features"][1]["operation"]["params"]["depth"] =
            serde_json::json!(1e-4);
        let thin = ExactChain::from_waffle(&thin).unwrap();
        assert!(matches!(
            exact_volume_verdict(&thin, 1e-4),
            ExactVolumeVerdict::NotCovered(_)
        ));
    }

    /// Defect 1 (seed-6 row 00122): a cut body `e1 − e2` has surviving material
    /// `z ∈ [0, 1]` (mid `0.5`) while its stale `expr_bbox` still spans `e1`'s
    /// `z ∈ [0, 4]` (mid `2.0`). A direction-less cut sketched at `z = 1.2`
    /// must auto-reverse into `z ∈ [0, 1]` instead of sweeping upward into
    /// `[1.2, 1.8]`; a plane inside the possible mid-extent bracket
    /// `[mid_lo, mid_hi]` must record `indeterminate` (declining via
    /// `NotCovered`); and a `ThroughAll` sweep must resolve against the sound
    /// outer projection bound `SampledExtent::outer_proj`.
    #[test]
    fn cut_direction_and_through_all_measure_membership_not_stale_bbox() {
        let base_cut_features = || {
            vec![
                // e1: 1×1 prism spanning z ∈ [0, 4] (bbox mid z = 2.0).
                square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
                extrude("e1", "s1", 4.0, false, serde_json::json!({})),
                // e2: explicit −z cut from z = 4.0 removing z ∈ [1.0, 4.0], so
                // surviving material is z ∈ [0.0, 1.0] (true mid z = 0.5).
                square_sketch("s2", [0.0, 0.0, 4.0], &[(0.0, 0.0, 1.0, 1.0, true)]),
                extrude(
                    "e2",
                    "s2",
                    3.0,
                    true,
                    serde_json::json!({"direction": [0.0, 0.0, -1.0]}),
                ),
            ]
        };

        // 1. Direction-less pocket cut from z = 1.2 (depth 0.6, 0.5×0.5):
        //    reverses into z ∈ [0.6, 1.0], removing 0.5² · 0.4 = 0.1 → vol 0.9.
        let mut feats = base_cut_features();
        feats.push(square_sketch(
            "s3",
            [0.0, 0.0, 1.2],
            &[(0.0, 0.0, 0.25, 0.25, true)],
        ));
        feats.push(extrude("e3", "s3", 0.6, true, serde_json::json!({})));
        let chain = ExactChain::from_waffle(&doc(feats)).unwrap();
        assert!(
            chain.indeterminate.is_empty(),
            "margin 0.7 is far above lattice resolution: {:?}",
            chain.indeterminate
        );
        assert!(
            !chain.contains([0.0, 0.0, 0.8]),
            "auto-reversed cut removes the pocket inside z ∈ [0.6, 1.0]"
        );
        assert!(
            chain.contains([0.0, 0.0, 0.4]),
            "material below z = 0.6 remains"
        );
        assert!(
            matches!(
                exact_volume_verdict(&chain, 0.9),
                ExactVolumeVerdict::Agree { .. }
            ),
            "expected Agree on 0.9, got {:?}",
            exact_volume_verdict(&chain, 0.9)
        );

        // 2. Sketch plane at z = 0.502 lies inside the possible mid-extent
        //    interval [mid(0.0, occ_hi), mid(occ_lo, 1.0)] ≈ [0.4961, 0.5039]
        //    around the true mid-extent z = 0.5 → indeterminate / NotCovered.
        let mut near_mid = base_cut_features();
        near_mid.push(square_sketch(
            "s3",
            [0.0, 0.0, 0.502],
            &[(0.0, 0.0, 0.25, 0.25, true)],
        ));
        near_mid.push(extrude("e3", "s3", 0.6, true, serde_json::json!({})));
        let near_chain = ExactChain::from_waffle(&doc(near_mid)).unwrap();
        assert!(
            !near_chain.indeterminate.is_empty(),
            "plane inside [mid_lo, mid_hi] must be marked indeterminate"
        );
        assert!(matches!(
            exact_volume_verdict(&near_chain, 0.9),
            ExactVolumeVerdict::NotCovered(_)
        ));

        // 3. ThroughAll cut from z = 1.0 (auto-reversed −z) clears z = 0.0
        //    completely (outer bound clears the body), and a ThroughAll
        //    NewBody boss from z = −0.5 along +z resolves against z_max ≈ 1.0
        //    (depth ≈ 1.52), not the stale bbox z_max = 4.0 (depth ≈ 4.55).
        let mut thru_cut = base_cut_features();
        thru_cut.push(square_sketch(
            "s3",
            [0.0, 0.0, 1.0],
            &[(0.0, 0.0, 0.25, 0.25, true)],
        ));
        thru_cut.push(extrude(
            "e3",
            "s3",
            0.0,
            true,
            serde_json::json!({"depth_mode": {"type": "ThroughAll"}}),
        ));
        let thru_cut_chain = ExactChain::from_waffle(&doc(thru_cut)).unwrap();
        assert!(
            !thru_cut_chain.contains([0.0, 0.0, 0.0]),
            "ThroughAll outer bound must clear the bottom face z = 0"
        );

        let mut thru_boss = base_cut_features();
        thru_boss.push(square_sketch(
            "s3",
            [0.0, 0.0, -0.5],
            &[(0.0, 0.0, 0.25, 0.25, true)],
        ));
        thru_boss.push(extrude(
            "e3",
            "s3",
            0.0,
            false,
            serde_json::json!({
                "direction": [0.0, 0.0, 1.0],
                "combine": {"type": "NewBody"},
                "depth_mode": {"type": "ThroughAll"}
            }),
        ));
        let thru_boss_chain = ExactChain::from_waffle(&doc(thru_boss)).unwrap();
        let ExactSolid::Extrude { depth, .. } = thru_boss_chain.ops[2].solid else {
            panic!("expected Extrude");
        };
        assert!(
            depth > 1.5 && depth < 1.6,
            "ThroughAll depth {depth} should clear true extent 1.5 with outer bound, not stale bbox 4.5"
        );
    }

    /// Review 1 Item 3: a sub-`h` needle spike extending 2 units (> 170 lattice
    /// steps) beyond the extreme occupied lattice sample is missed completely by
    /// `occ_hi + h` and `|occ_mid - sketch_proj| < h`, but bracketed soundly by
    /// `[mid(outer_lo, occ_hi), mid(occ_lo, outer_hi)]` and `outer_proj`.
    #[test]
    fn thin_spike_beyond_occupied_sample_uses_sound_outer_bounds() {
        let spike_sketch = || {
            serde_json::json!({
                "id": "ss",
                "name": "Sketch ss",
                "suppressed": false,
                "operation": {
                    "type": "Sketch",
                    "sketch": {
                        "id": "ss",
                        "plane_origin": [0.0, 0.0, 0.0],
                        "plane_normal": [0.0, 0.0, 1.0],
                        // On plane_normal = [0, 0, 1], basis.x = [0, -1, 0] (u = -y)
                        // and basis.y = [1, 0, 0] (v = +x), so v ∈ [0, 1] with a
                        // needle spike to v = 3.0 at u ≈ 0.00125 spans x ∈ [0, 3].
                        "entities": [
                            {"type": "Point", "id": 1, "x": -0.5, "y": 0.0},
                            {"type": "Point", "id": 2, "x": -0.5, "y": 1.0},
                            {"type": "Point", "id": 3, "x": 0.0012, "y": 1.0},
                            {"type": "Point", "id": 4, "x": 0.00125, "y": 3.0},
                            {"type": "Point", "id": 5, "x": 0.0013, "y": 1.0},
                            {"type": "Point", "id": 6, "x": 0.5, "y": 1.0},
                            {"type": "Point", "id": 7, "x": 0.5, "y": 0.0}
                        ],
                        "solved_profiles": [{
                            "entity_ids": [1, 2, 3, 4, 5, 6, 7],
                            "is_outer": true,
                            "vertex_ids": [1, 2, 3, 4, 5, 6, 7]
                        }]
                    }
                }
            })
        };
        let x_cut_sketch = |id: &str, x0: f64| {
            serde_json::json!({
                "id": id,
                "name": format!("Sketch {id}"),
                "suppressed": false,
                "operation": {
                    "type": "Sketch",
                    "sketch": {
                        "id": id,
                        "plane_origin": [x0, 0.0, 0.5],
                        "plane_normal": [1.0, 0.0, 0.0],
                        "entities": [
                            {"type": "Point", "id": 1, "x": -1.5, "y": -1.5},
                            {"type": "Point", "id": 2, "x": 1.5, "y": -1.5},
                            {"type": "Point", "id": 3, "x": 1.5, "y": 1.5},
                            {"type": "Point", "id": 4, "x": -1.5, "y": 1.5}
                        ],
                        "solved_profiles": [{
                            "entity_ids": [1, 2, 3, 4],
                            "is_outer": true,
                            "vertex_ids": [1, 2, 3, 4]
                        }]
                    }
                }
            })
        };

        // 1. Cut plane at x = 1.1: occupied mid ≈ 0.5 (|0.5 - 1.1| = 0.6 ≫ h),
        //    yet true vertex mid is 0.5 * (0 + 3) = 1.5, so x = 1.1 lies inside
        //    [mid_lo, mid_hi] ≈ [0.497, 1.503] and MUST decline as indeterminate.
        let ambig = ExactChain::from_waffle(&doc(vec![
            spike_sketch(),
            extrude("e1", "ss", 1.0, false, serde_json::json!({})),
            x_cut_sketch("sc", 1.1),
            extrude("c2", "sc", 0.5, true, serde_json::json!({})),
        ]))
        .unwrap();
        assert!(
            !ambig.indeterminate.is_empty(),
            "x = 1.1 lies inside [mid_lo, mid_hi] for a sub-h spike to x = 3.0"
        );

        // 2. Cut plane at x = 2.0 (> mid_hi ≈ 1.503): certified reverse = true.
        let cert = ExactChain::from_waffle(&doc(vec![
            spike_sketch(),
            extrude("e1", "ss", 1.0, false, serde_json::json!({})),
            x_cut_sketch("sc", 2.0),
            extrude("c2", "sc", 0.5, true, serde_json::json!({})),
        ]))
        .unwrap();
        assert!(
            cert.indeterminate.is_empty(),
            "x = 2.0 is strictly past mid_hi ≈ 1.503: {:?}",
            cert.indeterminate
        );

        // 3. ThroughAll cut from x = -0.2 along +x clears the thin spike tip at
        //    x = 2.9 (where occ_hi + h ≈ 1.01 would have stopped short).
        let thru = ExactChain::from_waffle(&doc(vec![
            spike_sketch(),
            extrude("e1", "ss", 1.0, false, serde_json::json!({})),
            x_cut_sketch("sc", -0.2),
            extrude(
                "c2",
                "sc",
                0.0,
                true,
                serde_json::json!({
                    "direction": [1.0, 0.0, 0.0],
                    "depth_mode": {"type": "ThroughAll"}
                }),
            ),
        ]))
        .unwrap();
        assert!(
            !thru.contains([2.9, -0.00125, 0.5])
                && ExactChain::from_waffle(&doc(vec![
                    spike_sketch(),
                    extrude("e1", "ss", 1.0, false, serde_json::json!({})),
                ]))
                .unwrap()
                .contains([2.9, -0.00125, 0.5]),
            "ThroughAll outer bound must sweep past x = 3.0 and remove the sub-h spike tip"
        );
    }

    /// Defect 2 (seed-6 rows 00112, 00134, 00024):
    /// 1. `Combine::Add` decides whether targets merge with the tool from solid
    ///    membership, not world AABB overlap — keeping a near-but-disjoint
    ///    target as a leftover `Body { index: 1 }` that a subsequent
    ///    `BooleanCombine` carries untouched (`carry_untargeted_siblings`).
    /// 2. Sub-lattice gaps and measure-zero face-sharing contacts are marked
    ///    `indeterminate` so `exact_volume_verdict` declines via `NotCovered`,
    ///    while face-sharing bodies remain folded in `bodies_after` for direct
    ///    topology readouts (`r0081_reads_genus_three`).
    /// 3. Selective `FeatureOutput` targeting (`BooleanCombine` / `Explicit`)
    ///    of a body severed into `!= 1` disconnected components by a `Cut`
    ///    marks `indeterminate` because B-Rep shell ordering of `Main` vs
    ///    `Body { index }` is not determined by the document.
    #[test]
    fn add_touching_and_multi_lump_custody_use_membership_not_bbox() {
        let bool_combine = |id: &str, a: &str, b: &str, verb: &str| {
            let gr = |f: &str| {
                serde_json::json!({
                    "kind": {"type": "Solid"},
                    "anchor": {
                        "type": "FeatureOutput",
                        "feature_id": f,
                        "output_key": {"type": "Main"}
                    },
                    "selector": {"type": "Role", "role": {"type": "EndCapPositive"}, "index": 0},
                    "policy": {"type": "BestEffort"}
                })
            };
            serde_json::json!({
                "id": id,
                "name": format!("Combine {id}"),
                "suppressed": false,
                "operation": {
                    "type": "BooleanCombine",
                    "params": {
                        "body_a": gr(a),
                        "body_b": gr(b),
                        "operation": {"type": verb}
                    }
                }
            })
        };

        // 1. Rotated diamond |x| + |y| ≤ 0.6 (z ∈ [0, 1], vol 0.72, world AABB
        //    [-0.6, 0.6]²) + Add square (u, v) ∈ [0.35, 0.55]² (vol 0.04, world
        //    AABB strictly inside the diamond's AABB, Euclidean gap 0.0707 ≫ 2h):
        //    the diamond stays a separate leftover body and survives an
        //    Intersect on `b2`'s `Main`.
        let diamond_sketch = serde_json::json!({
            "id": "sd",
            "name": "Sketch sd",
            "suppressed": false,
            "operation": {
                "type": "Sketch",
                "sketch": {
                    "id": "sd",
                    "plane_origin": [0.0, 0.0, 0.0],
                    "plane_normal": [0.0, 0.0, 1.0],
                    "entities": [
                        {"type": "Point", "id": 1, "x": 0.6, "y": 0.0},
                        {"type": "Point", "id": 2, "x": 0.0, "y": 0.6},
                        {"type": "Point", "id": 3, "x": -0.6, "y": 0.0},
                        {"type": "Point", "id": 4, "x": 0.0, "y": -0.6}
                    ],
                    "solved_profiles": [
                        {"entity_ids": [1, 2, 3, 4], "is_outer": true, "vertex_ids": [1, 2, 3, 4]}
                    ]
                }
            }
        });
        let near_disjoint = ExactChain::from_waffle(&doc(vec![
            diamond_sketch,
            extrude("d1", "sd", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 0.0], &[(0.45, 0.45, 0.1, 0.1, true)]),
            extrude("b2", "s2", 1.0, false, serde_json::json!({})),
            square_sketch("s3", [0.0, 0.0, 0.5], &[(0.45, 0.45, 0.1, 0.1, true)]),
            extrude(
                "c3",
                "s3",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("m4", "b2", "c3", "Intersect"),
        ]))
        .unwrap();
        assert!(
            near_disjoint.indeterminate.is_empty(),
            "gap 0.0707 is well above lattice resolution: {:?}",
            near_disjoint.indeterminate
        );
        assert_eq!(
            near_disjoint.bodies_after(2).len(),
            2,
            "disjoint Add target must stay a separate leftover body"
        );
        assert_eq!(
            near_disjoint.bodies_after(3).len(),
            2,
            "BooleanCombine on b2 Main must carry leftover d1 untouched"
        );
        assert!(
            matches!(
                exact_volume_verdict(&near_disjoint, 0.74),
                ExactVolumeVerdict::Agree { .. }
            ),
            "expected Agree on 0.72 + 0.02 = 0.74, got {:?}",
            exact_volume_verdict(&near_disjoint, 0.74)
        );

        // 2a. Sub-TAU_MODEL non-coplanar gap (5e-8 m) on a pure Add chain with
        //     NO downstream consumer: fold vs split produces the exact same set
        //     union volume (2.0), so the chain stays determinate and Agrees.
        let sub_gap_pure = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 1.0 + 5e-8], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e2", "s2", 1.0, false, serde_json::json!({})),
        ]))
        .unwrap();
        assert!(
            sub_gap_pure.indeterminate.is_empty(),
            "pure Add chain with no selective consumer must stay determinate: {:?}",
            sub_gap_pure.indeterminate
        );
        assert!(matches!(
            exact_volume_verdict(&sub_gap_pure, 2.0),
            ExactVolumeVerdict::Agree { .. }
        ));

        // 2b. The SAME 5e-8 m non-coplanar gap followed by a selective
        //     `BooleanCombine(Intersect)` on `e2`'s `Main` (`00112` mechanism):
        //     folded gives vol(e2 ∩ e3) = 0.5, split carries `e1` untouched for
        //     1.5 → hypotheses diverge, so `from_waffle` declines via `indeterminate`.
        let sub_gap_targeted = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 1.0 + 5e-8], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e2", "s2", 1.0, false, serde_json::json!({})),
            square_sketch("s3", [0.0, 0.0, 1.5], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude(
                "e3",
                "s3",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("m4", "e2", "e3", "Intersect"),
        ]))
        .unwrap();
        assert!(
            !sub_gap_targeted.indeterminate.is_empty(),
            "selective Intersect on ambiguous fold/split must decline"
        );
        assert!(matches!(
            exact_volume_verdict(&sub_gap_targeted, 1.5),
            ExactVolumeVerdict::NotCovered(_)
        ));

        // 2c. Exact coplanar Extrude cap-sharing contact (`z = 1.0`, `F0001` /
        //     `P0025` shape): certified `Touches` via 2D face overlap, so BOTH
        //     pure Add AND selective `BooleanCombine(Intersect)` stay determinate!
        let face_share = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 1.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e2", "s2", 1.0, false, serde_json::json!({})),
        ]))
        .unwrap();
        assert!(
            face_share.indeterminate.is_empty(),
            "coplanar face-sharing Add must be certified Touches: {:?}",
            face_share.indeterminate
        );
        assert_eq!(face_share.bodies_after(2).len(), 1);
        assert!(matches!(
            exact_volume_verdict(&face_share, 2.0),
            ExactVolumeVerdict::Agree { .. }
        ));
        let face_share_targeted = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 1.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e2", "s2", 1.0, false, serde_json::json!({})),
            square_sketch("s3", [0.0, 0.0, 1.5], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude(
                "e3",
                "s3",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("m4", "e2", "e3", "Intersect"),
        ]))
        .unwrap();
        assert!(
            face_share_targeted.indeterminate.is_empty(),
            "coplanar face-sharing Add followed by Intersect must stay determinate: {:?}",
            face_share_targeted.indeterminate
        );
        assert!(matches!(
            exact_volume_verdict(&face_share_targeted, 0.5),
            ExactVolumeVerdict::Agree { .. }
        ));

        // 2b. Review 2 Item 1: at 10 m scale, `1e-6 * step = 1e-5 > TAU_MODEL`.
        //     True z = 10 contact still proves `Touches` under `eps_n <= TAU_MODEL / 2`,
        //     whereas shaving a `2e-6 m` (`20 * TAU_MODEL`) gap off `e1`'s top cap must
        //     NOT falsely certify `Touches` via `e1`'s surviving leaf cap plane (it
        //     proves `Disjoint` instead, keeping `u3`'s two shells separate).
        let ten_m_touch = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 5.0, 5.0, true)]),
            extrude("e1", "s1", 10.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 10.0], &[(0.0, 0.0, 5.0, 5.0, true)]),
            extrude(
                "e2",
                "s2",
                10.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("u3", "e1", "e2", "Union"),
            square_sketch("s3", [0.0, 0.0, 0.0], &[(0.0, 0.0, 5.0, 5.0, true)]),
            extrude(
                "e3",
                "s3",
                15.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("m4", "u3", "e3", "Intersect"),
        ]))
        .unwrap();
        assert_eq!(ten_m_touch.bodies_after(2).len(), 1);
        assert!(
            ten_m_touch.indeterminate.is_empty(),
            "10 m coplanar contact must still prove Touches under TAU_MODEL / 2 lift: {:?}",
            ten_m_touch.indeterminate
        );
        assert!(matches!(
            exact_volume_verdict(&ten_m_touch, 1500.0),
            ExactVolumeVerdict::Agree { .. }
        ));

        let ten_m_shaved_gap = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 5.0, 5.0, true)]),
            extrude("e1", "s1", 10.0, false, serde_json::json!({})),
            square_sketch("sc", [0.0, 0.0, 10.0 - 2e-6], &[(0.0, 0.0, 6.0, 6.0, true)]),
            extrude(
                "c1",
                "sc",
                1.0,
                true,
                serde_json::json!({"direction": [0.0, 0.0, 1.0]}),
            ),
            square_sketch("s2", [0.0, 0.0, 10.0], &[(0.0, 0.0, 5.0, 5.0, true)]),
            extrude(
                "e2",
                "s2",
                10.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("u3", "e1", "e2", "Union"),
            square_sketch("s3", [0.0, 0.0, 0.0], &[(0.0, 0.0, 5.0, 5.0, true)]),
            extrude(
                "e3",
                "s3",
                15.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("m4", "u3", "e3", "Intersect"),
        ]))
        .unwrap();
        assert_eq!(
            ten_m_shaved_gap.bodies_after(3).len(),
            2,
            "2e-6 m gap (> TAU_MODEL) at 10 m scale must prove Disjoint, not Touches"
        );
        assert!(
            !ten_m_shaved_gap.indeterminate.is_empty(),
            "selective Intersect on disjoint Union(u3) must decline"
        );
        assert!(matches!(
            exact_volume_verdict(&ten_m_shaved_gap, 1500.0),
            ExactVolumeVerdict::NotCovered(_)
        ));

        // 3. Multi-component Cut split + selective FeatureOutput combine:
        //    bar severed into two 1.6-volume lumps, then Intersect on `e2` Main.
        let split_prefix = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 2.0, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 1.5], &[(0.0, 0.0, 1.0, 0.4, true)]),
            extrude(
                "e2",
                "s2",
                2.0,
                true,
                serde_json::json!({"direction": [0.0, 0.0, -1.0]}),
            ),
        ]))
        .unwrap();
        assert!(
            matches!(
                exact_volume_verdict(&split_prefix, 3.2),
                ExactVolumeVerdict::Agree { .. }
            ),
            "untargeted two-lump cut total 3.2 is determinate: {:?}",
            exact_volume_verdict(&split_prefix, 3.2)
        );
        let split_targeted = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 2.0, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 1.5], &[(0.0, 0.0, 1.0, 0.4, true)]),
            extrude(
                "e2",
                "s2",
                2.0,
                true,
                serde_json::json!({"direction": [0.0, 0.0, -1.0]}),
            ),
            square_sketch("s3", [0.0, 0.0, 0.0], &[(0.0, 1.2, 0.5, 1.0, true)]),
            extrude(
                "e3",
                "s3",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("m4", "e2", "e3", "Intersect"),
        ]))
        .unwrap();
        assert!(
            !split_targeted.indeterminate.is_empty(),
            "selective FeatureOutput(e2, Main) after multi-lump cut must be indeterminate"
        );
        assert!(matches!(
            exact_volume_verdict(&split_targeted, 1.6),
            ExactVolumeVerdict::NotCovered(_)
        ));

        // 3b. Review 2 Item 2: explicit FeatureOutput(e2, Main) after a multi-lump
        //     cut evaluates EVERY candidate lump for output 0 and declines ONLY if
        //     the resulting volumes differ:
        //     - Enclosing Intersect keeps whichever 1.6 lump is Main and carries the
        //       1.6 sibling → 3.2 in both worlds → Agree.
        //     - Touching Union (z = 1.0 cap contact on +v lump, 0 interior overlap)
        //       yields 4.2 whether e3 merges with +v or stays separate alongside it → Agree.
        //     - Interior-overlapping Union (z ∈ [0.5, 1.5] on +v lump, 0.5 overlap)
        //       yields 3.7 if Main = +v vs 4.2 if Main = -v → declines.
        let split_intersect_enclosing = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 2.0, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 1.5], &[(0.0, 0.0, 1.0, 0.4, true)]),
            extrude(
                "e2",
                "s2",
                2.0,
                true,
                serde_json::json!({"direction": [0.0, 0.0, -1.0]}),
            ),
            square_sketch("s3", [0.0, 0.0, -0.5], &[(0.0, 0.0, 1.0, 3.0, true)]),
            extrude(
                "e3",
                "s3",
                2.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("m4", "e2", "e3", "Intersect"),
        ]))
        .unwrap();
        assert!(
            split_intersect_enclosing.indeterminate.is_empty(),
            "enclosing Intersect on multi-lump cut has equal volume across lumps: {:?}",
            split_intersect_enclosing.indeterminate
        );
        assert!(matches!(
            exact_volume_verdict(&split_intersect_enclosing, 3.2),
            ExactVolumeVerdict::Agree { .. }
        ));

        let split_union_touching_one_lump = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 2.0, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 1.5], &[(0.0, 0.0, 1.0, 0.4, true)]),
            extrude(
                "e2",
                "s2",
                2.0,
                true,
                serde_json::json!({"direction": [0.0, 0.0, -1.0]}),
            ),
            square_sketch("s3", [0.0, 0.0, 1.0], &[(0.0, 1.2, 0.5, 0.5, true)]),
            extrude(
                "e3",
                "s3",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("m4", "e2", "e3", "Union"),
        ]))
        .unwrap();
        assert!(
            split_union_touching_one_lump.indeterminate.is_empty(),
            "cap-touching Union on one lump has equal volume (4.2) across lumps: {:?}",
            split_union_touching_one_lump.indeterminate
        );
        assert!(matches!(
            exact_volume_verdict(&split_union_touching_one_lump, 4.2),
            ExactVolumeVerdict::Agree { .. }
        ));

        let split_union_overlapping_one_lump = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 2.0, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, 1.5], &[(0.0, 0.0, 1.0, 0.4, true)]),
            extrude(
                "e2",
                "s2",
                2.0,
                true,
                serde_json::json!({"direction": [0.0, 0.0, -1.0]}),
            ),
            square_sketch("s3", [0.0, 0.0, 0.5], &[(0.0, 1.2, 0.5, 0.5, true)]),
            extrude(
                "e3",
                "s3",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("m4", "e2", "e3", "Union"),
        ]))
        .unwrap();
        assert!(
            !split_union_overlapping_one_lump.indeterminate.is_empty(),
            "interior-overlapping Union on one lump changes volume (3.7 vs 4.2) and must decline"
        );
        assert!(matches!(
            exact_volume_verdict(&split_union_overlapping_one_lump, 3.7),
            ExactVolumeVerdict::NotCovered(_)
        ));
    }

    /// Review 1 Item 2:
    /// 1. A direction-less blind or `ThroughAll` cut on a multi-component target
    ///    stays covered when every lattice component votes the same reversal and
    ///    sweep (e.g. sketch plane on the outer face, P0016 / P0027 / P0029),
    ///    and declines ONLY when components vote opposite directions (e.g.
    ///    sketch plane between severed halves, seed-6 00024).
    /// 2. Chained disjoint `BooleanCombine(Union)` (`P0011`) stays covered when
    ///    the incoming operand is disjoint from all candidate slots.
    #[test]
    fn multi_component_cut_only_declines_when_components_disagree() {
        let bool_combine = |id: &str, a: &str, b: &str, verb: &str| {
            let gr = |f: &str| {
                serde_json::json!({
                    "kind": {"type": "Solid"},
                    "anchor": {
                        "type": "FeatureOutput",
                        "feature_id": f,
                        "output_key": {"type": "Main"}
                    },
                    "selector": {"type": "Role", "role": {"type": "EndCapPositive"}, "index": 0},
                    "policy": {"type": "BestEffort"}
                })
            };
            serde_json::json!({
                "id": id,
                "name": format!("Combine {id}"),
                "suppressed": false,
                "operation": {
                    "type": "BooleanCombine",
                    "params": {
                        "body_a": gr(a),
                        "body_b": gr(b),
                        "operation": {"type": verb}
                    }
                }
            })
        };

        // Base: bar spanning z ∈ [0, 4], v ∈ [-2, 2] (x ∈ [-2, 2], y ∈ [-0.5, 0.5]),
        // split down the middle v ∈ [-0.4, 0.4] into two side-by-side pillars,
        // each spanning z ∈ [0, 4] of volume 1.6 × 4 = 6.4 (total 12.8).
        let split_pillars = || {
            vec![
                square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 2.0, true)]),
                extrude("e1", "s1", 4.0, false, serde_json::json!({})),
                square_sketch("s2", [0.0, 0.0, 5.0], &[(0.0, 0.0, 1.0, 0.4, true)]),
                extrude(
                    "e2",
                    "s2",
                    6.0,
                    true,
                    serde_json::json!({"direction": [0.0, 0.0, -1.0]}),
                ),
            ]
        };

        // 1a. Blind direction-less cut from top face z = 4.0 (intersects bbox!):
        //     both pillars span z ∈ [0, 4], so both vote `reverse = true` → covered.
        let mut blind_agree = split_pillars();
        blind_agree.push(square_sketch(
            "s3",
            [0.0, 0.0, 4.0],
            &[(0.0, 1.2, 0.25, 0.25, true)],
        ));
        blind_agree.push(extrude("c3", "s3", 1.0, true, serde_json::json!({})));
        let blind_chain = ExactChain::from_waffle(&doc(blind_agree)).unwrap();
        assert!(
            blind_chain.indeterminate.is_empty(),
            "agreeing multi-component blind cut must stay determinate: {:?}",
            blind_chain.indeterminate
        );
        assert!(matches!(
            exact_volume_verdict(&blind_chain, 12.8 - 0.25),
            ExactVolumeVerdict::Agree { .. }
        ));

        // 1b. ThroughAll direction-less cut from top face z = 4.0: both pillars
        //     end at z = 0, so both sweep past z = 0 → covered.
        let mut thru_agree = split_pillars();
        thru_agree.push(square_sketch(
            "s3",
            [0.0, 0.0, 4.0],
            &[(0.0, 1.2, 0.25, 0.25, true)],
        ));
        thru_agree.push(extrude(
            "c3",
            "s3",
            0.0,
            true,
            serde_json::json!({"depth_mode": {"type": "ThroughAll"}}),
        ));
        let thru_chain = ExactChain::from_waffle(&doc(thru_agree)).unwrap();
        assert!(
            thru_chain.indeterminate.is_empty(),
            "agreeing multi-component ThroughAll cut must stay determinate: {:?}",
            thru_chain.indeterminate
        );
        assert!(matches!(
            exact_volume_verdict(&thru_chain, 12.8 - 1.0),
            ExactVolumeVerdict::Agree { .. }
        ));

        // 1c. Split along z into [0, 1.5] and [2.5, 4.0] (total volume 3.0), then
        //     direction-less cut of depth 1.0 at z = 2.0: lower component votes
        //     `reverse = true` (removes z ∈ [1.0, 1.5], volume 0.125) and upper
        //     votes `reverse = false` (removes z ∈ [2.5, 3.0], volume 0.125).
        //     Because both reversals yield identical volume 2.875, the oracle
        //     stays determinate.
        let split_z_ops = |cut_z: f64| {
            vec![
                square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
                extrude("e1", "s1", 4.0, false, serde_json::json!({})),
                serde_json::json!({
                    "id": "s2", "name": "Sketch s2", "suppressed": false,
                    "operation": {"type": "Sketch", "sketch": {
                        "id": "s2",
                        "plane_origin": [-1.0, 0.0, 2.0],
                        "plane_normal": [1.0, 0.0, 0.0],
                        "entities": [
                            {"type": "Point", "id": 1, "x": -1.0, "y": -0.5},
                            {"type": "Point", "id": 2, "x": 1.0, "y": -0.5},
                            {"type": "Point", "id": 3, "x": 1.0, "y": 0.5},
                            {"type": "Point", "id": 4, "x": -1.0, "y": 0.5}
                        ],
                        "solved_profiles": [{"entity_ids": [1,2,3,4], "is_outer": true, "vertex_ids": [1,2,3,4]}]
                    }}
                }),
                extrude(
                    "e2",
                    "s2",
                    3.0,
                    true,
                    serde_json::json!({"direction": [1.0, 0.0, 0.0]}),
                ),
                square_sketch("s3", [0.0, 0.0, cut_z], &[(0.0, 0.0, 0.25, 0.25, true)]),
                extrude("c3", "s3", 1.0, true, serde_json::json!({})),
            ]
        };
        let sym_disagree = ExactChain::from_waffle(&doc(split_z_ops(2.0))).unwrap();
        assert!(
            sym_disagree.indeterminate.is_empty(),
            "opposing component reversals with equal volume 2.875 must stay determinate: {:?}",
            sym_disagree.indeterminate
        );
        assert!(matches!(
            exact_volume_verdict(&sym_disagree, 2.875),
            ExactVolumeVerdict::Agree { .. }
        ));

        // 1d. Same z-split [0, 1.5] and [2.5, 4.0], but direction-less cut of
        //     depth 1.0 at z = 1.8: `reverse = true` removes z ∈ [0.8, 1.5]
        //     (0.175 → 2.825) while `reverse = false` removes z ∈ [2.5, 2.8]
        //     (0.075 → 2.925). Because the resulting volumes differ, the oracle
        //     declines.
        let asym_disagree = ExactChain::from_waffle(&doc(split_z_ops(1.8))).unwrap();
        assert!(
            !asym_disagree.indeterminate.is_empty(),
            "opposing component reversals with differing volumes (2.825 vs 2.925) must decline"
        );
        assert!(matches!(
            exact_volume_verdict(&asym_disagree, 2.825),
            ExactVolumeVerdict::NotCovered(_)
        ));

        // 2. P0011 shape: two disjoint NewBody bosses joined by BooleanCombine(Union),
        //    then a third disjoint NewBody boss joined to `Main` of the first combine.
        let p0011_like = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0, 0.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude(
                "b1",
                "s1",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            square_sketch("s2", [0.0, 0.0, 0.0], &[(0.0, 3.0, 0.5, 0.5, true)]),
            extrude(
                "b2",
                "s2",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("m1", "b1", "b2", "Union"),
            square_sketch("s3", [0.0, 0.0, 0.0], &[(0.0, 6.0, 0.5, 0.5, true)]),
            extrude(
                "b3",
                "s3",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "NewBody"}}),
            ),
            bool_combine("m2", "m1", "b3", "Union"),
        ]))
        .unwrap();
        assert!(
            p0011_like.indeterminate.is_empty(),
            "disjoint chained BooleanCombine(Union) is order-independent: {:?}",
            p0011_like.indeterminate
        );
        assert!(matches!(
            exact_volume_verdict(&p0011_like, 3.0),
            ExactVolumeVerdict::Agree { .. }
        ));
    }

    #[test]
    fn through_all_with_multiple_targets_checks_secondary_target_slab() {
        let doc = |features: Vec<Value>| -> Value {
            serde_json::json!({
                "format": "waffle-iron", "version": 3,
                "document": {"name": "t", "created": "2026-01-01T00:00:00Z", "modified": "2026-01-01T00:00:00Z"},
                "tabs": [{"id": "t1", "name": "Part 1", "kind": {"type": "Part", "features": {"features": features, "active_index": null}}}],
                "active_tab": "t1"
            })
        };
        // `b1` is created first, then `b2` is a disjoint `Add` (`merge: true`),
        // leaving `b2` as the primary target (`target_exprs[0]`, `components = None`)
        // and `b1` as a secondary leftover target (`target_exprs[1]`).
        // `b2` has a base block z ∈ [0, 1] plus an un-sampled 1e-4-thick fin
        // y ∈ [0.9999, 1.0], z ∈ [0, 3], so `s_occ_hi < 1.0` while `s_outer_hi = 3.0`.
        let make_case = |b1_z0: f64| -> ExactChain {
            ExactChain::from_waffle(&doc(vec![
                serde_json::json!({
                    "id": "s1", "name": "Sketch s1", "suppressed": false,
                    "operation": {"type": "Sketch", "sketch": {
                        "id": "s1",
                        "plane_origin": [0.0, 0.0, b1_z0],
                        "plane_normal": [0.0, 0.0, 1.0],
                        "entities": [
                            {"type": "Point", "id": 1, "x": -0.4, "y": -0.4},
                            {"type": "Point", "id": 2, "x": 0.4, "y": -0.4},
                            {"type": "Point", "id": 3, "x": 0.4, "y": 0.4},
                            {"type": "Point", "id": 4, "x": -0.4, "y": 0.4}
                        ],
                        "solved_profiles": [{"entity_ids": [1,2,3,4], "is_outer": true, "vertex_ids": [1,2,3,4]}]
                    }}
                }),
                serde_json::json!({
                    "id": "b1", "name": "Extrude b1", "suppressed": false,
                    "operation": {"type": "Extrude", "params": {
                        "sketch_id": "s1", "profile_index": 0, "depth": 0.6, "cut": false, "merge": true
                    }}
                }),
                serde_json::json!({
                    "id": "s2", "name": "Sketch s2", "suppressed": false,
                    "operation": {"type": "Sketch", "sketch": {
                        "id": "s2",
                        "plane_origin": [-0.5, 0.0, 0.0],
                        "plane_normal": [1.0, 0.0, 0.0],
                        "entities": [
                            {"type": "Point", "id": 1, "x": -1.0, "y": 0.0},
                            {"type": "Point", "id": 2, "x": 1.0, "y": 0.0},
                            {"type": "Point", "id": 3, "x": 1.0, "y": 3.0},
                            {"type": "Point", "id": 4, "x": 0.9999, "y": 3.0},
                            {"type": "Point", "id": 5, "x": 0.9999, "y": 1.0},
                            {"type": "Point", "id": 6, "x": -1.0, "y": 1.0}
                        ],
                        "solved_profiles": [{"entity_ids": [1,2,3,4,5,6], "is_outer": true, "vertex_ids": [1,2,3,4,5,6]}]
                    }}
                }),
                serde_json::json!({
                    "id": "b2", "name": "Extrude b2", "suppressed": false,
                    "operation": {"type": "Extrude", "params": {
                        "sketch_id": "s2", "profile_index": 0, "depth": 1.0, "cut": false, "merge": true
                    }}
                }),
                serde_json::json!({
                    "id": "s3", "name": "Sketch s3", "suppressed": false,
                    "operation": {"type": "Sketch", "sketch": {
                        "id": "s3",
                        "plane_origin": [0.0, 0.0, -0.5],
                        "plane_normal": [0.0, 0.0, 1.0],
                        "entities": [
                            {"type": "Point", "id": 1, "x": -0.25, "y": -0.25},
                            {"type": "Point", "id": 2, "x": 0.25, "y": -0.25},
                            {"type": "Point", "id": 3, "x": 0.25, "y": 0.25},
                            {"type": "Point", "id": 4, "x": -0.25, "y": 0.25}
                        ],
                        "solved_profiles": [{"entity_ids": [1,2,3,4], "is_outer": true, "vertex_ids": [1,2,3,4]}]
                    }}
                }),
                serde_json::json!({
                    "id": "c3", "name": "Extrude c3", "suppressed": false,
                    "operation": {"type": "Extrude", "params": {
                        "sketch_id": "s3", "profile_index": 0, "depth": 0.0, "cut": true, "merge": true,
                        "direction": [0.0, 0.0, 1.0],
                        "depth_mode": {"type": "ThroughAll"}
                    }}
                }),
            ]))
            .unwrap()
        };

        // 1. Secondary target `b1` at z ∈ [1.8, 2.4] sits inside the primary's
        //    [occ_hi, outer_hi] ThroughAll depth uncertainty band → must decline.
        let hit_slab = make_case(1.8);
        assert!(
            !hit_slab.indeterminate.is_empty(),
            "secondary target inside [d_occ, d_val] must decline: {:?}",
            hit_slab.notes
        );
        assert!(matches!(
            exact_volume_verdict(&hit_slab, 2.0),
            ExactVolumeVerdict::NotCovered(_)
        ));

        // 2. Secondary target `b1` at z ∈ [4.0, 4.6] sits strictly past `d_val`
        //    (and `b2` inside the cutter profile ends at z = 1.0 < d_occ) → covered.
        let clear_slab = make_case(4.0);
        assert!(
            clear_slab.indeterminate.is_empty(),
            "secondary target clear of [d_occ, d_val] must stay determinate: {:?}",
            clear_slab.indeterminate
        );
        let expected_vol = (2.0002 - 0.25) + (0.8 * 0.8 * 0.6);
        assert!(matches!(
            exact_volume_verdict(&clear_slab, expected_vol),
            ExactVolumeVerdict::Agree { .. }
        ));
    }

    #[test]
    fn exact_volume_verdict_judges_certified_empty_and_declines_sub_lattice_wall() {
        // 1. Disjoint Intersect (disjoint world AABBs): certified empty.
        let disjoint = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0; 3], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [5.0, 5.0, 0.0], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude(
                "e2",
                "s2",
                1.0,
                false,
                serde_json::json!({"combine": {"type": "Intersect"}, "targets": [target("e1")]}),
            ),
        ]))
        .unwrap();
        assert!(matches!(
            exact_volume_verdict(&disjoint, 0.0),
            ExactVolumeVerdict::Agree { .. }
        ));
        assert!(matches!(
            exact_volume_verdict(&disjoint, 1.0),
            ExactVolumeVerdict::Flag { exact: 0.0, .. }
        ));

        // 2. Engulfing Cut (positive-volume AABB, certified empty on cell balls).
        let engulfed = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0; 3], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, -0.5], &[(0.0, 0.0, 1.5, 1.5, true)]),
            extrude("e2", "s2", 2.0, true, serde_json::json!({})),
        ]))
        .unwrap();
        assert!(matches!(
            exact_volume_verdict(&engulfed, 0.0),
            ExactVolumeVerdict::Agree { .. }
        ));

        // 3. C0034-shape thin-wall square tube (1e-4 wall < h = 1/256):
        //    reads 0 cubes at 256 cells, MUST stay NotCovered even at kernel 0.0.
        let thin_tube = ExactChain::from_waffle(&doc(vec![
            square_sketch("s1", [0.0; 3], &[(0.0, 0.0, 0.5, 0.5, true)]),
            extrude("e1", "s1", 1.0, false, serde_json::json!({})),
            square_sketch("s2", [0.0, 0.0, -0.5], &[(0.0, 0.0, 0.4999, 0.4999, true)]),
            extrude("e2", "s2", 2.0, true, serde_json::json!({})),
        ]))
        .unwrap();
        assert!(matches!(
            exact_volume_verdict(&thin_tube, 0.0),
            ExactVolumeVerdict::NotCovered(_)
        ));
        assert!(matches!(
            exact_volume_verdict(&thin_tube, 4e-4),
            ExactVolumeVerdict::NotCovered(_)
        ));
    }
}
