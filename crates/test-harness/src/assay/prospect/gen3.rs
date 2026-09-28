//! Generator v3 — recipes the prospector draws, builds and minimizes.
//!
//! Spec: `specs/assay_prospector.md` §4. A [`Recipe`] is a serializable
//! list of [`Step`]s drawn from a seeded splitmix64 stream (no `rand`, no
//! time — the `fuzz_boxes.rs` pattern); [`build`] executes it through
//! `ModelBuilder`, so every operation the harness can author is available,
//! and the document `ModelBuilder::save` returns is what gets judged.
//!
//! Geometry placement uses the kernel (the prior body's AABB decides where
//! the next tool goes) but every drawn quantity is RECORDED in absolute
//! terms in the step, so a recipe rebuilds without re-drawing and the
//! minimizer can edit it field by field. Degeneracy snapping (§4, P4) sits
//! in [`Step::snapped`]: when present it overrides the generic geometry,
//! and un-snapping is `snapped = None`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::helpers::{gear_profile, mesh_bounding_box, polygon_profile};
use crate::workflow::ModelBuilder;

// ── Deterministic RNG ────────────────────────────────────────────────────

/// splitmix64 — one `u64` of state, a fixed stream per seed.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// Log-uniform in [lo, hi] (both > 0).
    pub fn log_range(&mut self, lo: f64, hi: f64) -> f64 {
        10f64.powf(self.range(lo.log10(), hi.log10()))
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
}

// ── Recipe types ─────────────────────────────────────────────────────────

/// A sketch profile, in sketch-local coordinates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Profile {
    /// Regular convex n-gon of circumradius `r`, rotated by `rot` (rad).
    Convex { n: u32, r: f64, rot: f64 },
    /// Convex n-gon with each vertex radius scaled by `radii[i] ∈ (0, 1]`,
    /// so most draws are non-convex.
    NonConvex { r: f64, rot: f64, radii: Vec<f64> },
    /// Star polygon — alternating outer/inner radii; `r_in / r_out` small
    /// makes needles.
    Star {
        points: u32,
        r_in: f64,
        r_out: f64,
        rot: f64,
    },
    /// A true circle entity.
    Circle { r: f64 },
    /// Involute spur gear (`helpers::gear_profile`).
    Gear { teeth: u32, module: f64 },
}

impl Profile {
    /// Radius of the origin-centred disc holding the profile.
    pub fn radius(&self) -> f64 {
        match self {
            Profile::Convex { r, .. } | Profile::NonConvex { r, .. } => *r,
            Profile::Star { r_out, .. } => *r_out,
            Profile::Circle { r } => *r,
            // tip radius = pitch + module = module·(teeth/2 + 1)
            Profile::Gear { teeth, module } => module * (*teeth as f64 / 2.0 + 1.0),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Profile::Convex { n, .. } => format!("convex{n}"),
            Profile::NonConvex { radii, .. } => format!("nonconvex{}", radii.len()),
            Profile::Star {
                points,
                r_in,
                r_out,
                ..
            } => {
                format!("star{points}({:.2})", r_in / r_out)
            }
            Profile::Circle { .. } => "circle".to_string(),
            Profile::Gear { teeth, .. } => format!("gear{teeth}"),
        }
    }

    fn vertices(&self) -> Option<Vec<(f64, f64)>> {
        use std::f64::consts::TAU;
        Some(match self {
            Profile::Convex { n, r, rot } => (0..*n)
                .map(|k| {
                    let a = TAU * k as f64 / *n as f64 + rot;
                    (r * a.cos(), r * a.sin())
                })
                .collect(),
            Profile::NonConvex { r, rot, radii } => {
                let n = radii.len();
                (0..n)
                    .map(|k| {
                        let a = TAU * k as f64 / n as f64 + rot;
                        let rk = r * radii[k];
                        (rk * a.cos(), rk * a.sin())
                    })
                    .collect()
            }
            Profile::Star {
                points,
                r_in,
                r_out,
                rot,
            } => {
                let n = points * 2;
                (0..n)
                    .map(|k| {
                        let a = TAU * k as f64 / n as f64 + rot;
                        let rk = if k % 2 == 0 { *r_out } else { *r_in };
                        (rk * a.cos(), rk * a.sin())
                    })
                    .collect()
            }
            Profile::Circle { .. } | Profile::Gear { .. } => return None,
        })
    }
}

/// Where the sketch sits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plane {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
}

/// What the step does with its profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Op {
    /// Blind extrude merged into the main body (boss) or cut from it.
    Extrude { depth: f64, cut: bool },
    /// Symmetric blind boss (`total_depth` across the plane).
    ExtrudeSymmetric { total_depth: f64 },
    /// Through-all cut.
    ThroughAllCut,
    /// Revolve about an in-plane axis (world coordinates), boss or cut.
    Revolve {
        axis_origin: [f64; 3],
        axis_dir: [f64; 3],
        angle_deg: f64,
        cut: bool,
    },
    /// Extrude WITHOUT merging, then combine with the main body explicitly.
    Boolean { depth: f64, verb: Verb },
    /// Extrude without merging and leave it as a separate body (a later
    /// `Boolean` may target it through `target`).
    Standalone { depth: f64 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Verb {
    Union,
    Subtract,
    Intersect,
}

/// The geometry of one step (what the minimizer edits).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepGeom {
    pub plane: Plane,
    pub profile: Profile,
    pub op: Op,
}

/// One step of a recipe: generic geometry plus an optional snapped
/// override (§4 degeneracy snapping; P4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Step {
    pub generic: StepGeom,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapped: Option<StepGeom>,
    /// A description of the snap, for the report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap_note: Option<String>,
}

impl Step {
    pub fn geom(&self) -> &StepGeom {
        self.snapped.as_ref().unwrap_or(&self.generic)
    }
}

/// A complete candidate recipe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Recipe {
    pub seed: u64,
    pub index: u64,
    /// Characteristic length (m) — the base profile's radius.
    pub scale: f64,
    pub steps: Vec<Step>,
}

/// Candidate id for `(seed, index)`: `X<seed hex>-<index>` — a pure
/// function of the pair, so a driver can name a candidate without drawing
/// it (drawing builds geometry).
pub fn candidate_id(seed: u64, index: u64) -> String {
    format!("X{:08x}-{:05}", seed & 0xFFFF_FFFF, index)
}

impl Recipe {
    /// Candidate id (see [`candidate_id`]).
    pub fn id(&self) -> String {
        candidate_id(self.seed, self.index)
    }

    pub fn summary(&self) -> String {
        self.steps
            .iter()
            .map(|s| {
                let g = s.geom();
                let op = match &g.op {
                    Op::Extrude { cut: true, .. } => "cut",
                    Op::Extrude { .. } => "boss",
                    Op::ExtrudeSymmetric { .. } => "sym",
                    Op::ThroughAllCut => "thru",
                    Op::Revolve { cut: true, .. } => "rev-cut",
                    Op::Revolve { .. } => "rev",
                    Op::Boolean {
                        verb: Verb::Union, ..
                    } => "∪",
                    Op::Boolean {
                        verb: Verb::Subtract,
                        ..
                    } => "−",
                    Op::Boolean {
                        verb: Verb::Intersect,
                        ..
                    } => "∩",
                    Op::Standalone { .. } => "solo",
                };
                let snap = if s.snapped.is_some() { "*" } else { "" };
                format!("{}:{}{}", g.profile.label(), op, snap)
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

// ── Frames ───────────────────────────────────────────────────────────────

fn norm(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The engine's in-plane basis for a sketch normal (feature-engine
/// `tangent_x_from_normal` + `v = n × u`) — the frame the sketch-local
/// coordinates are embedded with, so a revolve axis or an offset written
/// here lands where the engine puts it.
pub fn plane_basis(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let n = norm(n);
    let r = if n[2].abs() < 0.99 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = norm(cross(r, n));
    let v = cross(n, u);
    (u, v)
}

// ── Drawing ──────────────────────────────────────────────────────────────

fn draw_profile(rng: &mut Rng, r: f64) -> Profile {
    use std::f64::consts::TAU;
    match rng.below(10) {
        0..=3 => Profile::Convex {
            n: 3 + rng.below(6) as u32,
            r,
            rot: rng.range(0.0, TAU),
        },
        4..=5 => {
            let n = 5 + rng.below(6) as usize;
            Profile::NonConvex {
                r,
                rot: rng.range(0.0, TAU),
                radii: (0..n).map(|_| rng.range(0.35, 1.0)).collect(),
            }
        }
        6 => Profile::Star {
            points: 3 + rng.below(6) as u32,
            r_in: r * rng.range(0.08, 0.9),
            r_out: r,
            rot: rng.range(0.0, TAU),
        },
        7..=8 => Profile::Circle { r },
        _ => {
            let teeth = 8 + rng.below(20) as u32;
            // tip radius = module·(teeth/2 + 1) = r
            let module = r / (teeth as f64 / 2.0 + 1.0);
            // INPUT feature-floor contract (A14.2): a gear's involute
            // polyline has segments ≈ 1 % of its module, and a candidate
            // drawn at a 1e-4 m scale authors segments below
            // `MIN_FEATURE_SIZE` — the kernel then rightly refuses the
            // INPUT (`face # is degenerate`), which is a wall by contract,
            // not a finding (seed 1 index 5, adjudicated 2026-09-28). Keep
            // the vocabulary generic: below the floor the draw is a circle.
            if gear_min_segment(teeth, module) < GEAR_SEGMENT_FLOOR {
                Profile::Circle { r }
            } else {
                Profile::Gear { teeth, module }
            }
        }
    }
}

/// The smallest profile segment a drawn gear may author: ten times the
/// kernel's `MIN_FEATURE_SIZE`, so a candidate never sits ON the feature
/// floor's rounding edge either.
const GEAR_SEGMENT_FLOOR: f64 = 10.0 * cad_primitives::MIN_FEATURE_SIZE;

/// The shortest segment of the gear profile the builder will author for
/// `(teeth, module)` — measured on the same helper `write_profile` uses.
fn gear_min_segment(teeth: u32, module: f64) -> f64 {
    let (_, positions, profiles) = gear_profile(teeth, module, 20.0);
    let mut min = f64::INFINITY;
    for p in &profiles {
        let n = p.vertex_ids.len();
        for i in 0..n {
            let (Some(a), Some(b)) = (
                positions.get(&p.vertex_ids[i]),
                positions.get(&p.vertex_ids[(i + 1) % n]),
            ) else {
                continue;
            };
            min = min.min(((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt());
        }
    }
    min
}

fn draw_unit_normal(rng: &mut Rng) -> [f64; 3] {
    let theta = rng.range(0.1, std::f64::consts::PI - 0.1);
    let phi = rng.range(0.0, std::f64::consts::TAU);
    [
        theta.sin() * phi.cos(),
        theta.sin() * phi.sin(),
        theta.cos(),
    ]
}

fn draw_plane(rng: &mut Rng, aabb: &([f64; 3], [f64; 3])) -> Plane {
    let (lo, hi) = aabb;
    // Origin inside the prior envelope (so the tool meets the body more
    // often than not), extended by a little so misses also occur.
    let origin = [
        rng.range(
            lo[0] - 0.15 * (hi[0] - lo[0]),
            hi[0] + 0.15 * (hi[0] - lo[0]),
        ),
        rng.range(
            lo[1] - 0.15 * (hi[1] - lo[1]),
            hi[1] + 0.15 * (hi[1] - lo[1]),
        ),
        rng.range(
            lo[2] - 0.15 * (hi[2] - lo[2]),
            hi[2] + 0.15 * (hi[2] - lo[2]),
        ),
    ];
    let normal = match rng.below(5) {
        0 => [1.0, 0.0, 0.0],
        1 => [0.0, 1.0, 0.0],
        2 => [0.0, 0.0, 1.0],
        _ => draw_unit_normal(rng),
    };
    Plane { origin, normal }
}

/// Draw one recipe from `(seed, index)`.
///
/// Placement of step k needs the model after steps 0..k, so drawing builds
/// as it goes (the kernel decides the envelope); the returned recipe is
/// self-contained for [`build`].
pub fn generate(seed: u64, index: u64) -> Recipe {
    let mut rng = Rng::new(seed ^ index.wrapping_mul(0xD6E8_FEB8_6659_FD93));
    let scale = rng.log_range(1e-4, 1e3);
    let n_steps = 2 + rng.below(7) as usize; // 2..=8

    let mut recipe = Recipe {
        seed,
        index,
        scale,
        steps: Vec::with_capacity(n_steps),
    };

    // Step 0: a boss on an axis-aligned plane at the origin.
    let base = StepGeom {
        plane: Plane {
            origin: [0.0; 3],
            normal: match rng.below(3) {
                0 => [1.0, 0.0, 0.0],
                1 => [0.0, 1.0, 0.0],
                _ => [0.0, 0.0, 1.0],
            },
        },
        profile: draw_profile(&mut rng, scale),
        op: if rng.chance(0.15) {
            Op::ExtrudeSymmetric {
                total_depth: scale * rng.range(0.3, 2.0),
            }
        } else {
            Op::Extrude {
                depth: scale * rng.range(0.3, 2.0),
                cut: false,
            }
        },
    };
    recipe.steps.push(Step {
        generic: base,
        snapped: None,
        snap_note: None,
    });

    let mut builder = ModelBuilder::kernel_v2();
    let mut names = Names::default();
    let mut has_standalone = false;
    for k in 1..n_steps {
        // Build what we have so far to learn the envelope.
        if apply_step(&mut builder, &mut names, &recipe.steps[k - 1], k - 1).is_err() {
            break;
        }
        let Some(aabb) = live_aabb(&mut builder, scale) else {
            break;
        };
        let ext = envelope_extent(&aabb);
        let r = ext * rng.range(0.15, 0.8);
        let plane = draw_plane(&mut rng, &aabb);
        let profile = draw_profile(&mut rng, r);
        let depth = ext * rng.range(0.2, 1.5);
        let op = match rng.below(20) {
            0..=6 => Op::Extrude { depth, cut: false },
            7..=11 => Op::Extrude { depth, cut: true },
            12 => Op::ThroughAllCut,
            13 => Op::ExtrudeSymmetric { total_depth: depth },
            14..=15 => {
                // Revolve about an in-plane axis clear of the profile.
                let (u, v) = plane_basis(plane.normal);
                let off = r * rng.range(1.05, 2.5);
                let axis_origin = [
                    plane.origin[0] - u[0] * off,
                    plane.origin[1] - u[1] * off,
                    plane.origin[2] - u[2] * off,
                ];
                let angle_deg = if rng.chance(0.25) {
                    360.0
                } else {
                    rng.range(15.0, 345.0)
                };
                Op::Revolve {
                    axis_origin,
                    axis_dir: v,
                    angle_deg,
                    cut: rng.chance(0.4),
                }
            }
            16..=17 => Op::Boolean {
                depth,
                verb: match rng.below(3) {
                    0 => Verb::Union,
                    1 => Verb::Subtract,
                    _ => Verb::Intersect,
                },
            },
            _ => {
                if has_standalone {
                    Op::Boolean {
                        depth,
                        verb: Verb::Union,
                    }
                } else {
                    has_standalone = true;
                    Op::Standalone { depth }
                }
            }
        };
        recipe.steps.push(Step {
            generic: StepGeom { plane, profile, op },
            snapped: None,
            snap_note: None,
        });
    }
    recipe
}

fn envelope_extent(aabb: &([f64; 3], [f64; 3])) -> f64 {
    let (lo, hi) = aabb;
    (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2])
}

fn live_aabb(builder: &mut ModelBuilder, scale: f64) -> Option<([f64; 3], [f64; 3])> {
    let tol = (scale * 0.01).clamp(1e-9, 0.1);
    let meshes = builder.tessellate_live_with_tol(tol).ok()?;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut any = false;
    for m in &meshes {
        if m.vertices.len() < 3 {
            continue;
        }
        any = true;
        let (a, b) = mesh_bounding_box(m);
        for i in 0..3 {
            lo[i] = lo[i].min(a[i] as f64);
            hi[i] = hi[i].max(b[i] as f64);
        }
    }
    any.then_some((lo, hi))
}

// ── Building ─────────────────────────────────────────────────────────────

/// Feature names as the build proceeds.
#[derive(Default)]
struct Names {
    /// The feature every merge / cut / boolean acts on.
    main: Option<String>,
    /// Standalone bodies (step index → feature name).
    standalone: HashMap<usize, String>,
}

/// Outcome of building a recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildReport {
    /// Steps applied without a harness error.
    pub steps_applied: usize,
    /// The harness error that stopped the build, if any (the engine error is
    /// ALSO in the document — the failing feature stays in the tree).
    pub stopped_by: Option<String>,
}

fn add_sketch(
    builder: &mut ModelBuilder,
    name: &str,
    plane: &Plane,
    profile: &Profile,
) -> Result<(), String> {
    match profile {
        Profile::Circle { r } => builder
            .true_circle_sketch(name, plane.origin, plane.normal, 0.0, 0.0, *r)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Profile::Gear { teeth, module } => {
            let (entities, positions, profiles) = gear_profile(*teeth, *module, 20.0);
            builder.begin_sketch(plane.origin, plane.normal);
            for e in entities {
                builder.add_sketch_entity(e);
            }
            builder
                .finish_sketch_manual(name, positions, profiles, plane.origin, plane.normal)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        _ => {
            let verts = profile.vertices().expect("polygonal profile");
            let (entities, positions, profiles) = polygon_profile(&verts);
            builder.begin_sketch(plane.origin, plane.normal);
            for e in entities {
                builder.add_sketch_entity(e);
            }
            builder
                .finish_sketch_manual(name, positions, profiles, plane.origin, plane.normal)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
    }
}

fn apply_step(
    builder: &mut ModelBuilder,
    names: &mut Names,
    step: &Step,
    k: usize,
) -> Result<(), String> {
    let g = step.geom();
    let sk = format!("sk_{k}");
    let feat = format!("op_{k}");
    add_sketch(builder, &sk, &g.plane, &g.profile)?;
    let e = |e: crate::helpers::HarnessError| e.to_string();
    match &g.op {
        // Every merging op CONSUMES the body it acts on and becomes the
        // feature that owns it (the engine's "a cut consumes its target"
        // rule) — so the main name follows the latest such feature, or a
        // later explicit boolean names a consumed body (`GeomRef resolution
        // failed … already consumed`: 9 of the first 40 candidates).
        Op::Extrude { depth, cut: false } => {
            builder.extrude(&feat, &sk, *depth).map_err(e)?;
            names.main = Some(feat);
        }
        Op::Extrude { depth, cut: true } => {
            builder.extrude_cut(&feat, &sk, *depth).map_err(e)?;
            names.main = Some(feat);
        }
        Op::ExtrudeSymmetric { total_depth } => {
            builder
                .extrude_symmetric(&feat, &sk, *total_depth)
                .map_err(e)?;
            names.main = Some(feat);
        }
        Op::ThroughAllCut => {
            builder.extrude_through_all(&feat, &sk, true).map_err(e)?;
            names.main = Some(feat);
        }
        Op::Revolve {
            axis_origin,
            axis_dir,
            angle_deg,
            cut,
        } => {
            if *cut {
                builder
                    .revolve_cut(&feat, &sk, *axis_origin, *axis_dir, *angle_deg)
                    .map_err(e)?;
            } else {
                builder
                    .revolve(&feat, &sk, *axis_origin, *axis_dir, *angle_deg)
                    .map_err(e)?;
            }
            names.main = Some(feat);
        }
        Op::Standalone { depth } => {
            builder.extrude_no_merge(&feat, &sk, *depth).map_err(e)?;
            names.standalone.insert(k, feat);
        }
        Op::Boolean { depth, verb } => {
            let tool = format!("tool_{k}");
            builder.extrude_no_merge(&tool, &sk, *depth).map_err(e)?;
            // A merging boss after a standalone body may have auto-unioned
            // it away (the engine merges into EVERY body it overlaps); a
            // consumed standalone is no longer a target.
            let consumed: Vec<usize> = names
                .standalone
                .iter()
                .filter(|(_, name)| {
                    builder
                        .feature_id(name)
                        .map(|id| builder.consumed_features().contains(&id))
                        .unwrap_or(true)
                })
                .map(|(&idx, _)| idx)
                .collect();
            for idx in consumed {
                names.standalone.remove(&idx);
            }
            // Target: the main body; a pending standalone body is folded in
            // first when the verb is Union (so multi-body chains occur).
            let target =
                if let (Verb::Union, Some((&idx, _))) = (verb, names.standalone.iter().next()) {
                    names.standalone.remove(&idx).expect("present")
                } else {
                    names
                        .main
                        .clone()
                        .ok_or_else(|| "boolean before any main body".to_string())?
                };
            let r = match verb {
                Verb::Union => builder.boolean_union(&feat, &target, &tool),
                Verb::Subtract => builder.boolean_subtract(&feat, &target, &tool),
                Verb::Intersect => builder.boolean_intersect(&feat, &target, &tool),
            };
            r.map_err(e)?;
            names.main = Some(feat);
        }
    }
    Ok(())
}

/// Execute a recipe. The returned builder holds the document whatever
/// happened: a harness error at step k leaves the failing feature (and its
/// engine error) in the tree, which is exactly what the categorizer reads.
pub fn build(recipe: &Recipe) -> (ModelBuilder, BuildReport) {
    let mut builder = ModelBuilder::kernel_v2();
    let mut names = Names::default();
    let mut report = BuildReport {
        steps_applied: 0,
        stopped_by: None,
    };
    for (k, step) in recipe.steps.iter().enumerate() {
        match apply_step(&mut builder, &mut names, step, k) {
            Ok(()) => report.steps_applied += 1,
            Err(e) => {
                report.stopped_by = Some(format!("step {k}: {e}"));
                break;
            }
        }
    }
    (builder, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_a_fixed_stream() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..16 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let u = Rng::new(7).unit();
        assert!((0.0..1.0).contains(&u));
    }

    /// Seed 1 index 5 (2026-09-28): a 17-tooth gear at module 2.26e-5 m
    /// authored 2.6e-7 m segments — below `MIN_FEATURE_SIZE` — and the
    /// kernel's INPUT feature-floor wall graded it ERROR. The draw must not
    /// author below the floor; above it the gear vocabulary is unchanged.
    #[test]
    fn gear_draw_respects_the_input_feature_floor() {
        assert!(gear_min_segment(17, 2.2621130904020902e-05) < GEAR_SEGMENT_FLOOR);
        assert!(gear_min_segment(17, 2.2621130904020902e-03) > GEAR_SEGMENT_FLOOR);
        // Every gear the vocabulary can draw at a 1e-4 m tip radius is
        // replaced by a circle; at 1e-1 m the gears stay gears.
        for seed in 0..64u64 {
            let mut rng = Rng::new(seed);
            if let Profile::Gear { teeth, module } = draw_profile(&mut rng, 1e-4) {
                panic!("sub-floor gear drawn: teeth {teeth} module {module}");
            }
        }
        let mut gears = 0;
        for seed in 0..64u64 {
            let mut rng = Rng::new(seed);
            if let Profile::Gear { teeth, module } = draw_profile(&mut rng, 1e-1) {
                assert!(gear_min_segment(teeth, module) >= GEAR_SEGMENT_FLOOR);
                gears += 1;
            }
        }
        assert!(
            gears > 0,
            "the gear vocabulary must survive above the floor"
        );
    }

    #[test]
    fn generate_is_deterministic_and_rebuilds() {
        let r1 = generate(1, 3);
        let r2 = generate(1, 3);
        assert_eq!(r1, r2);
        assert!(r1.steps.len() >= 2);
        let (mut b, rep) = build(&r1);
        assert!(rep.steps_applied >= 1, "{rep:?}");
        let doc = b.save().unwrap();
        assert!(doc.contains("\"Extrude\"") || doc.contains("\"Revolve\""));
    }

    #[test]
    fn plane_basis_is_orthonormal_and_matches_the_engine_reference_choice() {
        let (u, v) = plane_basis([0.0, 0.0, 1.0]);
        // Z normal ⇒ reference X ⇒ u = X × Z = −Y... the engine's u = ref × n.
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        assert!(dot(u, v).abs() < 1e-12);
        assert!((dot(u, u) - 1.0).abs() < 1e-12);
        assert!(dot(u, [0.0, 0.0, 1.0]).abs() < 1e-12);
        assert!(dot(v, [0.0, 0.0, 1.0]).abs() < 1e-12);
    }
}
