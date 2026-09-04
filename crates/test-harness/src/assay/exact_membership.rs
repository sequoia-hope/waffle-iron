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
//!   `ŷ = n̂ × x̂`).
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
//!   `ThroughAll` = the body's extent past the plane + 1 (floored at
//!   `max(depth, 1)`); `symmetric` / `second_direction: Blind` shift the face
//!   origin back by the second depth and extrude the sum; a CUT with no
//!   explicit direction auto-reverses when the target body's mid-extent along
//!   the direction lies behind the sketch plane (the engine measures the
//!   B-Rep's vertices; this module measures the exact chain's bounding box —
//!   the same sign except when the plane sits at the body's mid-extent, which
//!   the readout reports). A sheared prism (direction ∦ normal) is handled
//!   exactly: the solid is `{face_origin + (u, v) + t·d}`.
//! - **Revolve**: axis origin/direction in the plane, `ŵ` toward the profile,
//!   sweep velocity `m̂ = â × ŵ` at θ = 0 (kernel-v2 `construct/revolve.rs`),
//!   angle in degrees, full turn at 360.
//! - **Bodies and combine modes** (`feature-engine` `normalize_combine` +
//!   the rebuild's combine dispatch): a document is a LIST of bodies. A
//!   legacy feature (`combine: null`) is `Cut` when `cut`, else `Add` when
//!   `merge`, else `NewBody`, targeting every live body of the most recent
//!   solid-bearing feature; a new-style feature carries its `combine` verb
//!   and an explicit `targets` list (feature-output anchors — a body is
//!   addressable by every feature that produced or last modified it).
//!   `NewBody` pushes a standalone body; `Add` folds the tool and the
//!   targets it touches into ONE body (Add into nothing = standalone) and
//!   re-emits targets whose bounding box misses the tool's as this
//!   feature's LEFTOVER bodies (the engine's disjoint-merge rule); `Cut` /
//!   `Intersect` act on each target independently. A cut's auto-reversal
//!   and a through-all depth are measured on the FIRST target body, as the
//!   engine does (`combine_targets.first()`). The readout is per body,
//!   summed: overlapping independent bodies count their volume twice and
//!   their components separately — the kernel's own output semantics.
//!
//! Not covered (typed [`NotCovered`]): region extrudes (`params.regions`),
//! `BooleanCombine`, share-a-face auto-targeting (`combine` set with no
//! `targets`), `UpTo` / through-all SECOND directions, internal gears,
//! sketches without plane data or a resolvable profile.

use std::collections::HashMap;
use std::f64::consts::TAU;

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

/// A parsed document: operands in feature order with their combine sense.
#[derive(Debug, Clone)]
pub struct ExactChain {
    pub ops: Vec<ExactOp>,
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
    /// Exactly these bodies, by the id of a feature that produced or last
    /// modified them (`Anchor::FeatureOutput`).
    Explicit(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct ExactOp {
    pub name: String,
    /// The feature's id (what an explicit target anchors to).
    pub feature_id: String,
    pub solid: ExactSolid,
    /// `combine == Cut` — kept for the operand-level instruments.
    pub cut: bool,
    pub combine: Combine,
    pub targets: Targets,
}

/// A body's membership as a boolean expression over operand indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Leaf(usize),
    Union(Vec<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Inter(Box<Expr>, Box<Expr>),
}

/// One live body after a prefix of the chain.
#[derive(Debug, Clone)]
pub struct Body {
    /// Every feature id this body is the output of (creator, then each
    /// modifier), in order — the LAST is the feature whose output it is now.
    pub ids: Vec<String>,
    pub expr: Expr,
    /// Index of the operand that last produced or modified it.
    pub last_touched: usize,
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

    /// Membership of one body's expression.
    pub fn expr_contains(&self, e: &Expr, p: [f64; 3]) -> bool {
        match e {
            Expr::Leaf(i) => self.ops[*i].solid.contains(p),
            Expr::Union(es) => es.iter().any(|e| self.expr_contains(e, p)),
            Expr::Sub(a, b) => self.expr_contains(a, p) && !self.expr_contains(b, p),
            Expr::Inter(a, b) => self.expr_contains(a, p) && self.expr_contains(b, p),
        }
    }

    /// Bounding box of one body's expression (a subtraction keeps the
    /// minuend's box; an intersection takes the boxes' overlap, `None` when
    /// they are disjoint).
    pub fn expr_bbox(&self, e: &Expr) -> Option<([f64; 3], [f64; 3])> {
        match e {
            Expr::Leaf(i) => Some(self.ops[*i].solid.bbox()),
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
        }
    }

    /// The live bodies after the first `k` operations, replaying the
    /// engine's combine dispatch (see the module docs).
    pub fn bodies_after(&self, k: usize) -> Vec<Body> {
        let mut bodies: Vec<Body> = Vec::new();
        for (i, op) in self.ops.iter().take(k).enumerate() {
            let target_idx = self.target_indices(&bodies, op.combine, &op.targets);
            match op.combine {
                Combine::NewBody => bodies.push(Body {
                    ids: vec![op.feature_id.clone()],
                    expr: Expr::Leaf(i),
                    last_touched: i,
                }),
                Combine::Add => {
                    if target_idx.is_empty() {
                        // Add into nothing: a standalone body (spec §4.1).
                        bodies.push(Body {
                            ids: vec![op.feature_id.clone()],
                            expr: Expr::Leaf(i),
                            last_touched: i,
                        });
                    } else {
                        let mut sorted = target_idx.clone();
                        sorted.sort_unstable();
                        // Remove from the back so the indices stay valid,
                        // then restore list order.
                        let mut removed: Vec<Body> =
                            sorted.iter().rev().map(|&j| bodies.remove(j)).collect();
                        removed.reverse();
                        // The engine's fold merges what the tool touches and
                        // re-emits the rest as LEFTOVER bodies of this
                        // feature (`disjoint_merge_bodies`), the merged body
                        // first. A target whose box misses the tool's box is
                        // certainly a leftover; one whose box overlaps is
                        // folded here (the engine still keeps it separate if
                        // the solids do not meet — see `first_target_bbox`).
                        let tool_box = self.ops[i].solid.bbox();
                        let (touching, leftovers): (Vec<Body>, Vec<Body>) =
                            removed.into_iter().partition(|b| {
                                self.expr_bbox(&b.expr)
                                    .is_some_and(|bb| boxes_overlap(bb, tool_box))
                            });
                        let mut ids = Vec::new();
                        let mut parts = vec![Expr::Leaf(i)];
                        for b in touching {
                            ids.extend(b.ids);
                            parts.push(b.expr);
                        }
                        ids.push(op.feature_id.clone());
                        let expr = if parts.len() == 1 {
                            Expr::Leaf(i)
                        } else {
                            Expr::Union(parts)
                        };
                        bodies.push(Body {
                            ids,
                            expr,
                            last_touched: i,
                        });
                        for mut b in leftovers {
                            b.ids.push(op.feature_id.clone());
                            b.last_touched = i;
                            bodies.push(b);
                        }
                    }
                }
                Combine::Cut | Combine::Intersect => {
                    // Each target independently (feature scope, spec §4.2);
                    // no target = the engine's ResolutionFailed, nothing here.
                    for &j in &target_idx {
                        let old = std::mem::replace(&mut bodies[j].expr, Expr::Leaf(i));
                        bodies[j].expr = if op.combine == Combine::Cut {
                            Expr::Sub(Box::new(old), Box::new(Expr::Leaf(i)))
                        } else {
                            Expr::Inter(Box::new(old), Box::new(Expr::Leaf(i)))
                        };
                        bodies[j].ids.push(op.feature_id.clone());
                        bodies[j].last_touched = i;
                    }
                }
            }
        }
        bodies
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
            Targets::Explicit(ids) => {
                let mut out = Vec::new();
                for id in ids {
                    if let Some(j) = bodies.iter().position(|b| b.ids.iter().any(|x| x == id)) {
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
        op.cut = false;
        op.combine = Combine::NewBody;
        op.targets = Targets::MostRecent;
        ExactChain {
            ops: vec![op],
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
                    sketches.insert(id.to_string(), parsed);
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
                    // The body this feature measures for auto-reversal and
                    // through-all: its FIRST combine target.
                    let extent = chain.first_target_extent(combine, &targets);
                    let target_box = extent.bbox;
                    if target_box.is_none() && matches!(combine, Combine::Cut | Combine::Intersect)
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
                    let primary = match mode {
                        "Blind" => blind,
                        "ThroughAll" => {
                            let extent = target_box
                                .map(|(lo, hi)| {
                                    extent_past_plane(lo, hi, sk.basis.origin, dir_unit)
                                })
                                .unwrap_or(0.0);
                            let d = if target_box.is_some() {
                                (extent + 1.0).max(blind.max(1.0))
                            } else {
                                blind.max(100.0)
                            };
                            chain
                                .notes
                                .push(format!("{name}: ThroughAll depth resolved to {d:.6}"));
                            d
                        }
                        other => return Err(nc(index, &name, &format!("depth_mode {other}"))),
                    };
                    // Second direction: explicit field, else the symmetric flag.
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
                    // Cut auto-reversal (engine: target body's vertex mid-extent
                    // along `direction` behind the sketch plane ⇒ reverse).
                    let reverse = if is_cut && explicit_dir.is_none() {
                        match target_box {
                            Some((lo, hi)) => {
                                let (pmin, pmax) = projection_range(lo, hi, direction);
                                let body_mid = 0.5 * (pmin + pmax);
                                let sketch_proj = dot(sk.basis.origin, direction);
                                let rev = body_mid < sketch_proj;
                                // The engine keeps a merge's targets as
                                // SEPARATE bodies when the solids never
                                // meet, and then measures the tool body
                                // alone; when that reading would flip the
                                // decision, say so.
                                if let Some((tlo, thi)) = extent.tool_alone {
                                    let (tmin, tmax) = projection_range(tlo, thi, direction);
                                    let tool_rev = 0.5 * (tmin + tmax) < sketch_proj;
                                    if tool_rev != rev {
                                        chain.notes.push(format!(
                                            "{name}: cut auto-reversal decided on the merged body (reverse={rev}); if the previous merge's solids never met, the engine measures the tool body alone and decides reverse={tool_rev} — verify with FE_CUT_TRACE=1"
                                        ));
                                    }
                                }
                                let margin = (body_mid - sketch_proj).abs();
                                let span = (pmax - pmin).max(f64::MIN_POSITIVE);
                                if margin < 1e-3 * span {
                                    chain.notes.push(format!(
                                        "{name}: cut auto-reversal decided on a {:.3e}-relative margin (bbox mid vs plane) — the engine measures B-Rep vertices; verify",
                                        margin / span
                                    ));
                                }
                                if margin < 1e-9 * span {
                                    chain.indeterminate.push(format!(
                                        "{name}: the sketch plane sits at the target's mid-extent ({:.1e} relative) — the cut's auto-reversal is not determined by the document",
                                        margin / span
                                    ));
                                }
                                rev
                            }
                            None => true,
                        }
                    } else {
                        false
                    };
                    let (dir_final, depth, face_origin) = match (is_cut, second) {
                        (true, Some(sd)) => {
                            if reverse {
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
                            if reverse {
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
                    // The engine extrudes `depth` along the (possibly non-unit)
                    // direction's UNIT vector; the prism parameter t is a length.
                    let dir_final_unit = norm(dir_final).unwrap();
                    if reverse {
                        chain.notes.push(format!("{name}: cut auto-reversed"));
                    }
                    chain.ops.push(ExactOp {
                        name,
                        feature_id,
                        solid: ExactSolid::Extrude {
                            basis: sk.basis,
                            profile,
                            face_origin,
                            dir: dir_final_unit,
                            depth,
                        },
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
                    if chain.first_target_extent(combine, &targets).bbox.is_none()
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
                        cut: is_cut,
                        combine,
                        targets,
                    });
                }
                other => return Err(nc(index, &name, &format!("operation {other}"))),
            }
        }
        if chain.ops.is_empty() {
            return Err(NotCovered::Shape("no solid-bearing operation".into()));
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

/// The extent a feature measures for its cut auto-reversal and through-all
/// depth: the engine reads the vertices of its FIRST combine target
/// (`rebuild.rs`: `combine_targets.first()` / `find_most_recent_solid`) —
/// with a legacy target set that is the most recent feature's outputs, the
/// merged body (or the tool body, when the merge left its targets disjoint)
/// comes first.
#[derive(Debug, Clone, Copy)]
struct TargetExtent {
    /// The first target body's box (`None`: no target).
    bbox: Option<([f64; 3], [f64; 3])>,
    /// When the first target is a fold, the box of the fold's TOOL part
    /// alone — what the engine measures if that fold's solids never met.
    tool_alone: Option<([f64; 3], [f64; 3])>,
}

impl ExactChain {
    fn first_target_extent(&self, combine: Combine, targets: &Targets) -> TargetExtent {
        let bodies = self.bodies_after(self.ops.len());
        let idx = self.target_indices(&bodies, combine, targets);
        let Some(&first) = idx.first() else {
            return TargetExtent {
                bbox: None,
                tool_alone: None,
            };
        };
        let body = &bodies[first];
        let bbox = self.expr_bbox(&body.expr);
        let tool_alone = match &body.expr {
            Expr::Union(parts) => parts.first().and_then(|p| self.expr_bbox(p)),
            _ => None,
        };
        TargetExtent { bbox, tool_alone }
    }
}

fn boxes_overlap(a: ([f64; 3], [f64; 3]), b: ([f64; 3], [f64; 3])) -> bool {
    (0..3).all(|k| a.0[k] <= b.1[k] && b.0[k] <= a.1[k])
}

/// The engine's `normalize_combine`: an explicit `combine` verb with its
/// explicit `targets` (feature-output anchors), or the legacy derivation
/// from `cut` / `merge` / `target_body` targeting the most recent body.
fn parse_combine(params: &Value) -> Result<(Combine, Targets), String> {
    fn anchor_id(gr: &Value) -> Result<String, String> {
        let anchor = gr.get("anchor").ok_or("combine target without anchor")?;
        match anchor.get("type").and_then(Value::as_str) {
            Some("FeatureOutput") => anchor
                .get("feature_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| "combine target without feature_id".to_string()),
            other => Err(format!("combine target anchored to {other:?}")),
        }
    }
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
                    let ids = list.iter().map(anchor_id).collect::<Result<Vec<_>, _>>()?;
                    Ok((mode, Targets::Explicit(ids)))
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
                    Targets::Explicit(vec![anchor_id(gr)?])
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

/// Min/max of the 8 bbox corners projected on `dir`.
fn projection_range(lo: [f64; 3], hi: [f64; 3], dir: [f64; 3]) -> (f64, f64) {
    let mut pmin = f64::INFINITY;
    let mut pmax = f64::NEG_INFINITY;
    for i in 0..8 {
        let p = [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ];
        let s = dot(p, dir);
        pmin = pmin.min(s);
        pmax = pmax.max(s);
    }
    (pmin, pmax)
}

/// How far the box reaches past the plane through `origin` along unit `dir`.
fn extent_past_plane(lo: [f64; 3], hi: [f64; 3], origin: [f64; 3], dir: [f64; 3]) -> f64 {
    let (_, pmax) = projection_range(lo, hi, dir);
    (pmax - dot(origin, dir)).max(0.0)
}

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
        let basis = Basis::from_origin_normal(origin, normal).ok_or("degenerate plane normal")?;
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
    if let Some(why) = chain.indeterminate.first() {
        return ExactVolumeVerdict::NotCovered(why.clone());
    }
    let Some(r) = readout_exact(chain, chain.ops.len(), EXACT_VOLUME_CELLS, 0.5) else {
        return ExactVolumeVerdict::NotCovered("no bounding box (no live body)".into());
    };
    let exact = r.volume;
    if exact <= 0.0 {
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
}
