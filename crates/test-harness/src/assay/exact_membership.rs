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
//! - **Profiles**: the selected `solved_profiles[profile_index]` — a polygon
//!   (its `vertex_ids` in loop order, positions from `solved_positions`), a
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
//! - **Chain**: bosses union, cuts subtract, in feature order (`merge: false`
//!   bosses still join the SET; the readout's component count tells).
//!
//! Not covered (typed [`NotCovered`]): region extrudes (`params.regions`),
//! `BooleanCombine`, `UpTo` / through-all SECOND directions, internal gears,
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
}

impl Profile2 {
    pub fn contains(&self, u: f64, v: f64) -> bool {
        match self {
            Profile2::Polygon(pts) => point_in_polygon(pts, u, v),
            Profile2::Circle { cx, cy, r } => (u - cx).hypot(v - cy) <= *r,
            Profile2::Gear(g) => g.contains(u, v),
        }
    }
    /// `(umin, umax, vmin, vmax)`.
    pub fn bbox(&self) -> (f64, f64, f64, f64) {
        match self {
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
    /// The first sketch's frame — the lattice is laid in it (see
    /// [`readout_exact`]): a lattice OBLIQUE to the model's planes
    /// perforates frame-aligned thin features at every cell size (measured
    /// on R0053: the world-frame lattice flickers χ between −8 and +3 down
    /// to h ≈ 0.37 while the sketch-frame lattice reads 0 from h = 2 to
    /// 0.3 on two phases).
    pub frame: Basis,
}

#[derive(Debug, Clone)]
pub struct ExactOp {
    pub name: String,
    pub solid: ExactSolid,
    pub cut: bool,
}

impl ExactChain {
    /// Membership of the composed solid: bosses union, cuts subtract, in
    /// order.
    pub fn contains(&self, p: [f64; 3]) -> bool {
        let mut inside = false;
        for op in &self.ops {
            if op.cut {
                if inside && op.solid.contains(p) {
                    inside = false;
                }
            } else if !inside && op.solid.contains(p) {
                inside = true;
            }
        }
        inside
    }

    /// Membership of the first `k` operations only.
    pub fn contains_prefix(&self, k: usize, p: [f64; 3]) -> bool {
        let mut inside = false;
        for op in self.ops.iter().take(k) {
            if op.cut {
                if inside && op.solid.contains(p) {
                    inside = false;
                }
            } else if !inside && op.solid.contains(p) {
                inside = true;
            }
        }
        inside
    }

    /// Bounding box of the bosses (cuts cannot extend the solid).
    pub fn bbox(&self) -> Option<([f64; 3], [f64; 3])> {
        self.bbox_prefix(self.ops.len())
    }

    pub fn bbox_prefix(&self, k: usize) -> Option<([f64; 3], [f64; 3])> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        let mut any = false;
        for op in self.ops.iter().take(k).filter(|o| !o.cut) {
            let (a, b) = op.solid.bbox();
            for k in 0..3 {
                lo[k] = lo[k].min(a[k]);
                hi[k] = hi[k].max(b[k]);
            }
            any = true;
        }
        any.then_some((lo, hi))
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
            frame: Basis::from_origin_normal([0.0; 3], [0.0, 0.0, 1.0]).unwrap(),
        };
        let mut frame_set = false;
        for (index, f) in feats.iter().enumerate() {
            let name = f
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("?")
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
                    let profile = sk.profile(params).map_err(|w| nc(index, &name, &w))?;
                    let is_cut = params.get("cut").and_then(Value::as_bool).unwrap_or(false);
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
                            let extent = chain
                                .bbox()
                                .map(|(lo, hi)| {
                                    extent_past_plane(lo, hi, sk.basis.origin, dir_unit)
                                })
                                .unwrap_or(0.0);
                            let d = if chain.bbox().is_some() {
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
                        match chain.bbox() {
                            Some((lo, hi)) => {
                                let (pmin, pmax) = projection_range(lo, hi, direction);
                                let body_mid = 0.5 * (pmin + pmax);
                                let sketch_proj = dot(sk.basis.origin, direction);
                                let rev = body_mid < sketch_proj;
                                let margin = (body_mid - sketch_proj).abs();
                                let span = (pmax - pmin).max(f64::MIN_POSITIVE);
                                if margin < 1e-3 * span {
                                    chain.notes.push(format!(
                                        "{name}: cut auto-reversal decided on a {:.3e}-relative margin (bbox mid vs plane) — the engine measures B-Rep vertices; verify",
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
                        solid: ExactSolid::Extrude {
                            basis: sk.basis,
                            profile,
                            face_origin,
                            dir: dir_final_unit,
                            depth,
                        },
                        cut: is_cut,
                    });
                }
                "Revolve" => {
                    let params = op
                        .get("params")
                        .ok_or_else(|| nc(index, &name, "no params"))?;
                    let sk = sketch_for(&sketches, params).map_err(|w| nc(index, &name, &w))?;
                    let profile = sk.profile(params).map_err(|w| nc(index, &name, &w))?;
                    let is_cut = params.get("cut").and_then(Value::as_bool).unwrap_or(false);
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
        let prof = self
            .profiles
            .get(index)
            .ok_or_else(|| format!("profile_index {index} of {}", self.profiles.len()))?;
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
    /// Cube counts per face-connected component, largest first.
    pub component_sizes: Vec<usize>,
    /// Centroid of the occupied cubes (world coordinates; NaN when empty).
    pub centroid: [f64; 3],
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
        chain.contains_prefix(prefix, p)
    });
    let readout = grid.readout();
    let volume = readout.cubes as f64 * h * h * h;
    let component_sizes = grid.component_sizes();
    // Centroid of the occupied cubes, in world coordinates.
    let mut acc = [0.0f64; 3];
    let mut count = 0usize;
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
    })
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
    }
}
