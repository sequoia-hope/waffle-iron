//! PR-CF1 RED — curved boolean fuzz harness (correct-or-loud).
//!
//! Spec of record: `specs/yang_pr_cf1_curved_boolean_fuzz.md`.
//!
//! The planar boolean already has a 900-case randomized fuzz
//! (`tests/fuzz_boxes.rs`) proving **0 silent-wrong** over box pairs. This is
//! the curved analog: a deterministic, randomized harness that runs
//! `boolean({cylinder|sphere|cone}, box, {Union|Subtract}, &sidecar)` over a
//! stream of cases and enforces a **correct-or-loud** contract on every result.
//! The deliverable is twofold:
//!
//!   1. **Anti-requirement (hard invariant):** an `Ok` result must NEVER be
//!      silently wrong. Every `Ok` is structurally + numerically audited
//!      against the correct-or-loud contract (spec §3). Any `Ok` that fails an
//!      audit is bucketed `SILENT_WRONG` and the test FAILS with the recorded
//!      case details (seed + case index + primitive + op + params).
//!   2. **Robustness envelope (report):** the `Err`-taxonomy distribution —
//!      that histogram IS the map of what M5 has left. `Err(variant)` results
//!      are bucketed by variant name (including the `Stage4RegionInvalid` /
//!      `SsiRefinementFailed` sub-reasons, which name the *specific* M5 gaps)
//!      and reported split by (primitive, op).
//!
//! ## The correct-or-loud contract (spec §3 — the bar: ZERO silent-wrong)
//!
//! For every randomized case, `boolean(...)` must be **either** correct (ALL
//! of 1–6 below) **or** a loud, classified `Err`. The test FAILS on a
//! silent-wrong `Ok` (violates any of 1–6), an unclassified `Err`, or a PANIC
//! (a P9 violation — production must return `Result`, never unwind). The
//! `boolean()` call is run under `catch_unwind` ONLY so the harness reports the
//! full histogram + every panicking case rather than aborting on the first
//! one; a caught panic still fails the test loudly (it is never swallowed to
//! green). This is a test-side diagnostic, not a production fallback.
//!
//! 1. **Watertight** closed 2-manifold: `unpaired_half_edges(mesh) == 0`.
//! 2. **Euler χ even AND == sidecar-reference χ** — differential against the
//!    reference mesh's topology (`2 − 2g` computed from topology), NOT a
//!    hardcoded 2. A corner clip can split into several shells; a through-hole
//!    punches genus. χ-even is an additional non-manifold-corruption guard.
//! 3. **Analytic surface survival:** the output carries the input's curved
//!    `Surface` variant with EXACT params; no curved Sphere/Cone/Cylinder
//!    appears in the output that was not an input surface. Survival is only
//!    REQUIRED when the curved surface actually bounds the result (decided from
//!    the independent sidecar reference mesh — see `reference_surface_present`);
//!    a fully-interior/consumed primitive correctly has no surviving analytic
//!    face, so demanding one there would be a false silent-wrong.
//! 4. **On-surface exactness (the STRICT gate):** every output `Curve::Circle`
//!    / `Curve::Ellipse` edge is sampled; each sample's
//!    `signed_distance_to_surface` against BOTH incident faces' surfaces must
//!    be `|sd| <= TAU_MODEL`. This is a *stricter* exactness check than any
//!    volume number, and it is the real correctness gate.
//! 5. **Positive signed volume** (outward-oriented, not inside-out):
//!    `signed_volume(mesh) > 0`.
//! 6. **Chord-band volume sanity:** `|vol_yang − vol_sidecar|` within a
//!    principled chord-error envelope scaled from the curved face's Stage-1
//!    `d_ε` (NOT the strict 1e-6 `fuzz_boxes` uses — see the curved-oracle
//!    decision below and `chord_volume_band`'s derivation comment).
//!
//! ## Empty-result agreement (contract-ambiguity resolution)
//!
//! A boolean result can legitimately be the EMPTY solid — e.g. `sphere − box`
//! where the box fully encloses the sphere removes ALL material (the sidecar
//! reference returns an empty mesh: vol = 0, χ = 0, zero triangles). An empty
//! result is a *correct* outcome, NOT silent-wrong, so the surface-survival
//! (§3.3) and positive-volume (§3.5) gates — which presuppose a non-empty solid
//! — do not apply. The genuine check is that yang AGREES with the reference on
//! emptiness: both empty → `ok_correct`; a disagreement (yang empty where the
//! reference is not, or vice versa) → silent-wrong (a whole solid invented or
//! dropped). This is a deliberate contract interpretation, NOT a relaxation:
//! the empty case is checked MORE strictly (exact emptiness agreement), not
//! waved through.
//!
//! ## Curved-oracle decision (spec §4 — deliberate scoping, NOT widening)
//!
//! This harness REPLACES `fuzz_boxes`' strict `VOL_TOL = 1e-6` differential
//! with a chord-band volume envelope (contract §6). Rationale: yang's
//! exact-curve mesh is **more accurate** than the sidecar's faceted reference,
//! so a strict 1e-6 volume diff would manufacture false silent-wrong on every
//! curved case. The on-surface residual ≤ `TAU_MODEL` (§4) is the real,
//! stricter correctness gate; the chord-band volume is only a gross-loss /
//! dropped-chunk backstop. See `chord_volume_band` for the derivation.
//!
//! ## Direction catch (spec §2)
//!
//! `boolean(prim, box, Subtract)` = `prim − box` = **box-as-subtrahend**, the
//! *opposite* of every demo (`box − prim`). Box-as-subtrahend is explicitly
//! DEFERRED/out-of-scope, so MOST `Subtract` cases SHOULD resolve to a loud
//! classified `Err` — that is correct-or-loud, NOT a failure. Whether they are
//! loud vs silently-wrong is exactly what the fuzz maps.
//!
//! ## Determinism (governance)
//!
//! A hand-rolled splitmix64 PRNG seeded by a fixed constant (`SEED`). No `rand`
//! dep, no system time, no filesystem side effects beyond what the sidecar
//! itself does. The exact same case stream is produced every run.
//!
//! ## N (explicit, not silently truncated)
//!
//! `N_CASES = 300` total cases. Each case independently picks a random
//! primitive ∈ {cylinder, sphere, cone} and a random op ∈ {Union, Subtract}.
//! Every case is run (the loop is `0..N_CASES` with no `break`/early return on
//! the case axis); the only per-case skips are `skipped_bad_input` (a
//! `BRep::new` failure or a sidecar reference-call error/timeout), which are
//! tallied, never silently dropped. Heavy (`#[ignore]`d) — ~600 sidecar
//! reference spawns + ~300 pipeline calls (each of which may itself spawn the
//! sidecar), so ~1200 sidecar subprocess calls; some 30 s timeouts possible.
//!
//! Self-skips LOUDLY when the C++ sidecar binary is absent
//! (`SidecarBoolean::from_env()` → `Err`).
//!
//! ## Findings (PR-CF1 RED run)
//!
//! See the commit message and `docs/yang_functional_roadmap.md` for the
//! captured histogram. If real silent-wrong cases are surfaced, the offending
//! seeds + (primitive, op, params) are documented there; the asserting fuzz
//! below stays `#[ignore]`d so the default `cargo test -p yang-rs` stays green.

use std::collections::BTreeMap;
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3, TAU_MODEL};
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::SidecarBoolean;
use yang_rs::{
    boolean, signed_distance_to_surface, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface,
    YangError,
};

// =========================================================================
// Configuration
// =========================================================================

/// Fixed PRNG seed. Recorded here so the entire case stream is reproducible.
const SEED: u64 = 0xCF1_CADE_F00D_2026;

/// Total number of randomized cases. Each case picks a random primitive ∈
/// {cylinder, sphere, cone} and a random op ∈ {Union, Subtract}. Every case
/// in `0..N_CASES` is executed (no silent truncation); the only per-case skip
/// is `skipped_bad_input`, which is tallied.
const N_CASES: usize = 300;

// =========================================================================
// splitmix64 PRNG — deterministic, no external dep. (Copied from fuzz_boxes.)
// =========================================================================

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform f64 in [0, 1).
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform f64 in [lo, hi).
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    /// Uniform integer in [0, n).
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

// =========================================================================
// Linear algebra helpers (inline; cad-primitives is types-only).
// (Mat3/quat from fuzz_boxes; array math from yr8/yr16/yr17.)
// =========================================================================

type Mat3 = [[f64; 3]; 3];

/// Quaternion (w, x, y, z) → rotation matrix. Assumes a unit quaternion.
fn quat_to_mat3(w: f64, x: f64, y: f64, z: f64) -> Mat3 {
    let n = (w * w + x * x + y * y + z * z).sqrt();
    let (w, x, y, z) = (w / n, x / n, y / n, z / n);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

fn mat_vec(m: &Mat3, v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
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
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = norm(a);
    assert!(n > 0.0, "cannot normalize zero vector");
    scale(a, 1.0 / n)
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A random rigid rotation matrix from a random unit quaternion.
fn random_rotation(rng: &mut SplitMix64) -> Mat3 {
    loop {
        let w = rng.range(-1.0, 1.0);
        let x = rng.range(-1.0, 1.0);
        let y = rng.range(-1.0, 1.0);
        let z = rng.range(-1.0, 1.0);
        let n2 = w * w + x * x + y * y + z * z;
        if n2 > 1e-6 {
            break quat_to_mat3(w, x, y, z);
        }
    }
}

// =========================================================================
// Primitive kind + the curved-surface tessellation chord bound d_ε.
//
// Each bound is the EXACT literal the yang-rs Stage-1 production sizing uses
// (A14.3 single source of truth), copied test-side from params alone:
//
//   - cylinder: 1e-2 × AABB space diagonal of the two rim circles.
//   - sphere:   1e-2 × 2r√3  (the sphere's AABB cube space diagonal).
//   - cone:     1e-2 × √((2R)² + h²) with R = h·tan(half_angle).
// =========================================================================

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Primitive {
    Cylinder,
    Sphere,
    Cone,
}

impl Primitive {
    fn name(self) -> &'static str {
        match self {
            Primitive::Cylinder => "cylinder",
            Primitive::Sphere => "sphere",
            Primitive::Cone => "cone",
        }
    }
}

/// Concrete primitive parameters generated for a case (post-transform, in world
/// space). Recorded for the SilentWrong panic dump.
#[derive(Clone, Copy, Debug)]
enum PrimParams {
    Cylinder {
        axis_point: [f64; 3],
        axis_dir: [f64; 3],
        radius: f64,
        height: f64,
    },
    Sphere {
        center: [f64; 3],
        radius: f64,
    },
    Cone {
        apex: [f64; 3],
        axis_dir: [f64; 3],
        half_angle: f64,
        height: f64,
    },
}

impl PrimParams {
    /// The Stage-1 chord bound d_ε for this primitive's curved surface.
    fn chord_bound(&self) -> f64 {
        match *self {
            PrimParams::Cylinder {
                axis_point,
                axis_dir,
                radius,
                height,
            } => 1e-2 * cylinder_aabb_diagonal(axis_point, axis_dir, radius, height),
            PrimParams::Sphere { radius, .. } => 1e-2 * 2.0 * radius * 3f64.sqrt(),
            PrimParams::Cone {
                half_angle, height, ..
            } => {
                let r = height * half_angle.tan();
                1e-2 * ((2.0 * r).powi(2) + height.powi(2)).sqrt()
            }
        }
    }

    /// The exact `Surface` variant the curved face carries (for survival §3).
    fn curved_surface(&self) -> Surface {
        match *self {
            PrimParams::Cylinder {
                axis_point,
                axis_dir,
                radius,
                ..
            } => Surface::Cylinder {
                axis_point: p(axis_point[0], axis_point[1], axis_point[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                radius,
            },
            PrimParams::Sphere { center, radius } => Surface::Sphere {
                center: p(center[0], center[1], center[2]),
                radius,
            },
            PrimParams::Cone {
                apex,
                axis_dir,
                half_angle,
                ..
            } => Surface::Cone {
                apex: p(apex[0], apex[1], apex[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                half_angle,
            },
        }
    }
}

/// Analytic AABB space diagonal of a cylinder's two rim circles (copied from
/// yr8's `analytic_aabb_diagonal`).
fn cylinder_aabb_diagonal(
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    height: f64,
) -> f64 {
    let axis_unit = unit(axis_dir);
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, height));
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for center in [bottom_center, top_center] {
        for i in 0..3 {
            let span = radius * (1.0 - axis_unit[i] * axis_unit[i]).max(0.0).sqrt();
            lo[i] = lo[i].min(center[i] - span);
            hi[i] = hi[i].max(center[i] + span);
        }
    }
    norm(sub(hi, lo))
}

// =========================================================================
// Curved B-Rep fixtures — copied from yr8 (cylinder), yr12 (sphere),
// yr16/yr17 (cone). Integration tests cannot share helpers, so these are
// re-declared locally and verbatim in structure. Each returns Err rather than
// panicking so `BRep::new` failures bucket as `skipped_bad_input`.
// =========================================================================

fn cylinder_brep(
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    height: f64,
) -> Result<BRep, YangError> {
    let axis_unit = unit(axis_dir);
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, height));

    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(axis_unit, world));

    let v0 = add(bottom_center, scale(e1, radius));
    let v1 = add(top_center, scale(e1, radius));

    let verts = vec![
        BRepVertex {
            point: p(v0[0], v0[1], v0[2]),
        },
        BRepVertex {
            point: p(v1[0], v1[1], v1[2]),
        },
    ];

    let neg_axis = scale(axis_unit, -1.0);
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(bottom_center[0], bottom_center[1], bottom_center[2]),
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                radius,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];

    let bottom_d = -dot(neg_axis, bottom_center);
    let top_d = -dot(axis_unit, top_center);

    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(axis_point[0], axis_point[1], axis_point[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: bottom_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                d: top_d,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces)
}

fn sphere_brep(center: [f64; 3], radius: f64) -> Result<BRep, YangError> {
    let south = add(center, scale([0.0, 0.0, -1.0], radius));
    let north = add(center, scale([0.0, 0.0, 1.0], radius));

    let verts = vec![
        BRepVertex {
            point: p(south[0], south[1], south[2]),
        },
        BRepVertex {
            point: p(north[0], north[1], north[2]),
        },
    ];

    let edges = vec![BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::Circle {
            center: p(center[0], center[1], center[2]),
            normal: Vector3::new(0.0, -1.0, 0.0),
            radius,
        },
    }];

    let faces = vec![BRepFace {
        surface: Surface::Sphere {
            center: p(center[0], center[1], center[2]),
            radius,
        },
        outer_loop: vec![0],
        inner_loops: Vec::new(),
        reversed: false,
    }];

    BRep::new(verts, edges, faces)
}

fn cone_brep(
    apex: [f64; 3],
    axis_dir: [f64; 3],
    half_angle: f64,
    height: f64,
) -> Result<BRep, YangError> {
    let axis_unit = unit(axis_dir);
    let radius = height * half_angle.tan();
    let base_center = add(apex, scale(axis_unit, height));

    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(axis_unit, world));

    let base_seam = add(base_center, scale(e1, radius));

    let verts = vec![
        BRepVertex {
            point: p(apex[0], apex[1], apex[2]),
        },
        BRepVertex {
            point: p(base_seam[0], base_seam[1], base_seam[2]),
        },
    ];

    let edges = vec![BRepEdge {
        start: 1,
        end: 1,
        curve: Curve::Circle {
            center: p(base_center[0], base_center[1], base_center[2]),
            normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
            radius,
        },
    }];

    let cap_d = -dot(axis_unit, base_center);

    let faces = vec![
        BRepFace {
            surface: Surface::Cone {
                apex: p(apex[0], apex[1], apex[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                half_angle,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                d: cap_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces)
}

// =========================================================================
// Oriented box fixture (copied from fuzz_boxes' OrientedBox::to_brep, kept
// self-contained). Center, half-extents, rotation matrix.
// =========================================================================

struct OrientedBox {
    center: [f64; 3],
    half: [f64; 3],
    rot: Mat3,
}

impl OrientedBox {
    fn corner(&self, sx: f64, sy: f64, sz: f64) -> [f64; 3] {
        let local = [sx * self.half[0], sy * self.half[1], sz * self.half[2]];
        add(self.center, mat_vec(&self.rot, local))
    }

    fn to_brep(&self) -> Result<BRep, YangError> {
        let signs: [[f64; 3]; 8] = [
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
        ];
        let verts: Vec<BRepVertex> = signs
            .iter()
            .map(|s| {
                let c = self.corner(s[0], s[1], s[2]);
                BRepVertex {
                    point: p(c[0], c[1], c[2]),
                }
            })
            .collect();

        let face_verts: [[u32; 4]; 6] = [
            [0, 1, 2, 3],
            [4, 7, 6, 5],
            [0, 4, 5, 1],
            [1, 5, 6, 2],
            [2, 6, 7, 3],
            [3, 7, 4, 0],
        ];
        let local_normals: [[f64; 3]; 6] = [
            [0.0, 0.0, -1.0],
            [0.0, 0.0, 1.0],
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
        ];

        let mut edges = Vec::with_capacity(24);
        let mut loops = Vec::with_capacity(6);
        for vs in &face_verts {
            let base = edges.len() as u32;
            for i in 0..4 {
                edges.push(BRepEdge {
                    start: vs[i],
                    end: vs[(i + 1) % 4],
                    curve: Curve::LineSegment,
                });
            }
            loops.push(vec![base, base + 1, base + 2, base + 3]);
        }

        let faces: Vec<BRepFace> = (0..6)
            .map(|i| {
                let wn = mat_vec(&self.rot, local_normals[i]);
                let normal = Vector3::new(wn[0], wn[1], wn[2]);
                let v0 = verts[face_verts[i][0] as usize].point;
                let d = -dot(wn, v0.as_array());
                BRepFace {
                    surface: Surface::Plane { normal, d },
                    outer_loop: loops[i].clone(),
                    inner_loops: Vec::new(),
                    reversed: false,
                }
            })
            .collect();

        BRep::new(verts, edges, faces)
    }
}

// =========================================================================
// Case generation.
//
// The curved primitive is built canonically (axis-aligned about a chosen
// frame), then a random rigid rotation + small translation is applied so the
// box and the primitive interpenetrate (overlap near-certain). The box is
// centered near the primitive's bulk with half-extents on the order of the
// primitive radius, so they overlap.
// =========================================================================

/// Apply rotation `rot` then translation `t` to a point.
fn xform(rot: &Mat3, t: [f64; 3], v: [f64; 3]) -> [f64; 3] {
    add(t, mat_vec(rot, v))
}

struct Case {
    primitive: Primitive,
    op: BoolOp,
    params: PrimParams,
    prim_brep: Result<BRep, YangError>,
    box_brep: Result<BRep, YangError>,
}

fn gen_case(rng: &mut SplitMix64) -> Case {
    let primitive = match rng.below(3) {
        0 => Primitive::Cylinder,
        1 => Primitive::Sphere,
        _ => Primitive::Cone,
    };
    let op = if rng.below(2) == 0 {
        BoolOp::Union
    } else {
        BoolOp::Subtract
    };

    // Random rigid transform (rotation + small translation) applied to the
    // canonical primitive so its world-space surface params vary.
    let rot = random_rotation(rng);
    let t = [
        rng.range(-0.3, 0.3),
        rng.range(-0.3, 0.3),
        rng.range(-0.3, 0.3),
    ];

    // In-scope curved parameter ranges (spec §4).
    let radius = rng.range(0.2, 0.6);

    let (params, prim_brep): (PrimParams, Result<BRep, YangError>) = match primitive {
        Primitive::Cylinder => {
            let height = rng.range(0.8, 2.0);
            // Canonical: axis +Z, base at -height/2 so the cylinder straddles
            // the origin and the rotated/translated box overlaps it.
            let axis_point_c = [0.0, 0.0, -0.5 * height];
            let axis_dir_c = [0.0, 0.0, 1.0];
            let axis_point = xform(&rot, t, axis_point_c);
            let axis_dir = mat_vec(&rot, axis_dir_c);
            let params = PrimParams::Cylinder {
                axis_point,
                axis_dir,
                radius,
                height,
            };
            let brep = cylinder_brep(axis_point, axis_dir, radius, height);
            (params, brep)
        }
        Primitive::Sphere => {
            let center = xform(&rot, t, [0.0, 0.0, 0.0]);
            let params = PrimParams::Sphere { center, radius };
            let brep = sphere_brep(center, radius);
            (params, brep)
        }
        Primitive::Cone => {
            let height = rng.range(0.8, 2.0);
            let half_angle = rng.range(0.2, 0.6);
            // Canonical: apex at -height/2 along +Z so the cone straddles the
            // origin region the box overlaps.
            let apex_c = [0.0, 0.0, -0.5 * height];
            let axis_dir_c = [0.0, 0.0, 1.0];
            let apex = xform(&rot, t, apex_c);
            let axis_dir = mat_vec(&rot, axis_dir_c);
            let params = PrimParams::Cone {
                apex,
                axis_dir,
                half_angle,
                height,
            };
            let brep = cone_brep(apex, axis_dir, half_angle, height);
            (params, brep)
        }
    };

    // Box: centered near the primitive bulk (origin), half-extents on the order
    // of the primitive radius so it interpenetrates; randomly rotated.
    let box_center = [
        rng.range(-0.4, 0.4),
        rng.range(-0.4, 0.4),
        rng.range(-0.4, 0.4),
    ];
    let box_half = [
        rng.range(0.3, 0.8),
        rng.range(0.3, 0.8),
        rng.range(0.3, 0.8),
    ];
    let box_rot = random_rotation(rng);
    let bx = OrientedBox {
        center: box_center,
        half: box_half,
        rot: box_rot,
    };
    let box_brep = bx.to_brep();

    Case {
        primitive,
        op,
        params,
        prim_brep,
        box_brep,
    }
}

// =========================================================================
// Audit helpers (copied from fuzz_boxes / yr17).
// =========================================================================

/// Extract a human-readable message from a `catch_unwind` payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

fn signed_volume(mesh: &Mesh) -> f64 {
    let mut acc = 0.0;
    for tri in &mesh.tris {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        let cx = b[1] * c[2] - b[2] * c[1];
        let cy = b[2] * c[0] - b[0] * c[2];
        let cz = b[0] * c[1] - b[1] * c[0];
        acc += a[0] * cx + a[1] * cy + a[2] * cz;
    }
    acc / 6.0
}

fn unpaired_half_edges(mesh: &Mesh) -> usize {
    use std::collections::HashMap;
    let mut counts: HashMap<(u32, u32), i32> = HashMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *counts.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut unpaired = 0;
    for (&(s, e), &fwd) in &counts {
        let rev = counts.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            unpaired += (fwd - rev).unsigned_abs() as usize;
        }
    }
    unpaired
}

fn euler_characteristic(mesh: &Mesh) -> i64 {
    use std::collections::HashSet;
    let v = mesh.num_verts() as i64;
    let f = mesh.num_tris() as i64;
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    v - edges.len() as i64 + f
}

// =========================================================================
// Contract §3.6 — the chord-band volume envelope.
//
// DERIVATION (documented per spec §4):
//
// yang and the sidecar BOTH approximate the curved surface by chords, but at
// DIFFERENT facet counts, so |vol_yang − vol_sidecar| is NOT zero even when
// both are correct — it is bounded by the worst-case faceting volume error of
// the two meshes. A chord that deviates by up to d_ε from the true surface
// sweeps, per unit of curved-surface area, a volume slab of thickness ≤ d_ε;
// integrated over the curved surface area A_curved this caps the per-mesh
// faceting volume error at ~ A_curved · d_ε. The two meshes can err in opposite
// directions, so the differential is bounded by ~ 2 · A_curved · d_ε.
//
// We use a generous, surface-area-scaled band that strictly DOMINATES that
// bound (so a correct pair never trips it) while still catching a gross loss /
// dropped chunk (a missing wedge is O(volume), not O(area·d_ε)):
//
//     band = CHORD_FACTOR · A_curved · d_ε + ABS_FLOOR
//
// with CHORD_FACTOR = 4.0 (covers the 2·A·d_ε bound with comfortable margin for
// the box-clip's planar faceting and centroid-rounding slack) and ABS_FLOOR =
// 1e-6 (the floor below which sub-µ volume noise is not a dropped chunk). The
// curved surface area is estimated analytically from the primitive params (a
// generous over-estimate is fine — it only loosens the band, and the §3.4
// on-surface residual ≤ TAU_MODEL is the strict gate, not this band).
//
// This is a deliberate oracle scoping, NOT tolerance widening: it replaces a
// false-positive-prone strict 1e-6 differential with an envelope keyed to the
// genuine, physically-motivated faceting error. The strict correctness check
// is the on-surface residual; the band is only a dropped-chunk backstop.
// =========================================================================

const CHORD_FACTOR: f64 = 4.0;
const VOL_ABS_FLOOR: f64 = 1e-6;

/// A generous analytic over-estimate of the curved surface area of the
/// primitive (used only to scale the volume band; over-estimation is safe).
fn curved_surface_area_estimate(params: &PrimParams) -> f64 {
    match *params {
        PrimParams::Cylinder { radius, height, .. } => {
            // Lateral area 2πrh.
            2.0 * std::f64::consts::PI * radius * height
        }
        PrimParams::Sphere { radius, .. } => 4.0 * std::f64::consts::PI * radius * radius,
        PrimParams::Cone {
            half_angle, height, ..
        } => {
            // Lateral area π R L, R = h·tanα, slant L = √(R²+h²).
            let r = height * half_angle.tan();
            let l = (r * r + height * height).sqrt();
            std::f64::consts::PI * r * l
        }
    }
}

fn chord_volume_band(params: &PrimParams) -> f64 {
    let area = curved_surface_area_estimate(params);
    let de = params.chord_bound();
    CHORD_FACTOR * area * de + VOL_ABS_FLOOR
}

// =========================================================================
// Contract §3.3 — analytic surface survival.
//
// Returns (input_surface_present, foreign_curved_present):
//   - input_surface_present: ≥1 output face carries the input's exact curved
//     `Surface` variant (==, full param equality).
//   - foreign_curved_present: a curved (Sphere/Cone/Cylinder) face appears in
//     the output that is NOT the input's curved surface — a survival violation.
// =========================================================================

fn surface_survival(brep: &BRep, want: Surface) -> (bool, bool) {
    let mut present = false;
    let mut foreign = false;
    for f in brep.faces() {
        match f.surface {
            Surface::Plane { .. } => {}
            curved => {
                if curved == want {
                    present = true;
                } else {
                    foreign = true;
                }
            }
        }
    }
    (present, foreign)
}

// =========================================================================
// Reference-derived "does the curved surface bound the boolean result?" gate.
//
// Surface survival (§3.3) must only be REQUIRED when the curved surface is
// actually part of the boolean RESULT boundary. A primitive can be fully
// interior (e.g. a small sphere entirely inside the box: `sphere ∪ box = box`,
// no spherical cap survives — correct), in which case demanding a surviving
// Sphere face would be a FALSE silent-wrong. We decide this from the
// independent sidecar REFERENCE mesh (not from yang's output): if the reference
// has triangles lying ON the analytic curved surface (within the Stage-1 chord
// band), the surface bounds the result and yang MUST carry the exact analytic
// face; if the reference has essentially NO triangles on the surface, the
// primitive is interior/consumed and absence of the analytic face is correct.
//
// `signed_distance_to_surface` is the crate's own infallible signed-distance
// (Plane/Sphere/Cylinder/Cone all wired). A triangle counts as "on the surface"
// when all three of its vertices are within `band` of it.
// =========================================================================

fn reference_surface_present(reference: &Mesh, want: Surface, band: f64) -> bool {
    let mut on_surface = 0usize;
    for tri in &reference.tris {
        let a = reference.verts[tri[0] as usize];
        let b = reference.verts[tri[1] as usize];
        let cc = reference.verts[tri[2] as usize];
        let on = [a, b, cc].iter().all(|&pt| {
            signed_distance_to_surface(want, pt)
                .map(|sd| sd.abs() <= band)
                .unwrap_or(false)
        });
        if on {
            on_surface += 1;
        }
    }
    // A genuine surviving curved boundary contributes MANY facets (the
    // reference facets the whole cap/barrel). Require ≥ 2 on-surface triangles
    // so a single coincidental near-tangent facet does not force a survival
    // demand. (2 is the minimum a real curved patch tessellates to.)
    on_surface >= 2
}

// =========================================================================
// Contract §3.4 — on-surface exactness (the STRICT gate).
//
// For every output `Curve::Circle` / `Curve::Ellipse` edge, find its incident
// faces (the faces whose outer/inner loops reference that edge index), sample
// points ON the analytic curve, and require each sample's
// `signed_distance_to_surface` against BOTH incident faces' surfaces to be
// `|sd| <= TAU_MODEL`. Returns the worst residual seen (f64::NAN if no curved
// edges were present — handled by the caller as "no curved-edge gate fired").
//
// Sampling on the EXACT analytic curve (not the mesh verts) is what makes this
// a true exactness gate: a Circle/Ellipse edge claims the intersection lies
// exactly on both surfaces, so every point of the analytic curve must satisfy
// both surface equations to TAU_MODEL.
// =========================================================================

/// Orthonormal basis (e1, e2) spanning the plane normal to `n`. Deterministic
/// stablest-axis seed (matches the fixtures' convention).
fn ortho_basis(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let nu = unit(n);
    let abs = [nu[0].abs(), nu[1].abs(), nu[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(nu, world));
    let e2 = cross(nu, e1);
    (e1, e2)
}

/// Sample `n_samples` points on a `Curve::Circle` / `Curve::Ellipse`.
fn sample_curve(curve: &Curve, n_samples: usize) -> Vec<[f64; 3]> {
    let mut out = Vec::with_capacity(n_samples);
    match *curve {
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            let c = center.as_array();
            let (e1, e2) = ortho_basis(normal.as_array());
            for k in 0..n_samples {
                let th = 2.0 * std::f64::consts::PI * (k as f64) / (n_samples as f64);
                let pt = add(
                    c,
                    add(scale(e1, radius * th.cos()), scale(e2, radius * th.sin())),
                );
                out.push(pt);
            }
        }
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let c = center.as_array();
            let maj = unit(major_axis.as_array());
            // Minor axis = normal × major.
            let min_ax = unit(cross(unit(normal.as_array()), maj));
            for k in 0..n_samples {
                let th = 2.0 * std::f64::consts::PI * (k as f64) / (n_samples as f64);
                let pt = add(
                    c,
                    add(
                        scale(maj, major_radius * th.cos()),
                        scale(min_ax, minor_radius * th.sin()),
                    ),
                );
                out.push(pt);
            }
        }
        Curve::LineSegment => {}
        // PR-YR22: this fuzz helper samples only the circle/ellipse families;
        // a Parabola yields no samples here (same as LineSegment).
        // Exhaustiveness arm forced by the new enum variant.
        Curve::Parabola { .. } => {}
        // PR-YR23: likewise a Hyperbola yields no samples in this fuzz helper;
        // exhaustiveness arm forced by the new enum variant.
        Curve::Hyperbola { .. } => {}
    }
    out
}

/// Find the faces incident to edge index `e` (loops referencing it).
fn incident_faces(brep: &BRep, e: u32) -> Vec<usize> {
    let mut out = Vec::new();
    for (fi, f) in brep.faces().iter().enumerate() {
        let in_outer = f.outer_loop.contains(&e);
        let in_inner = f.inner_loops.iter().any(|l| l.contains(&e));
        if in_outer || in_inner {
            out.push(fi);
        }
    }
    out
}

/// Returns `Some(worst_residual)` over all curved output edges' samples against
/// both incident faces, or `None` if no curved edges were present.
fn on_surface_residual_max(brep: &BRep) -> Option<f64> {
    const N_SAMPLES: usize = 24;
    let mut worst: Option<f64> = None;
    for (ei, edge) in brep.edges().iter().enumerate() {
        if matches!(edge.curve, Curve::LineSegment) {
            continue;
        }
        let samples = sample_curve(&edge.curve, N_SAMPLES);
        let faces = incident_faces(brep, ei as u32);
        // A Circle/Ellipse edge should bound ≥2 faces (the two surfaces whose
        // intersection it is). If it does not, we still audit against whatever
        // faces reference it (the contract requires checking BOTH incident
        // faces; a curved edge bounding <2 faces is itself suspicious but is
        // surfaced via the residual against whatever it has — and the
        // watertight/euler gates catch a genuinely dangling edge).
        for &fi in &faces {
            let surf = brep.faces()[fi].surface;
            for &s in &samples {
                let sd = signed_distance_to_surface(surf, p(s[0], s[1], s[2]))
                    .expect("signed_distance_to_surface is infallible for Plane/curved surfaces");
                let a = sd.abs();
                worst = Some(worst.map_or(a, |w: f64| w.max(a)));
            }
        }
    }
    worst
}

// =========================================================================
// Error classification — extends fuzz_boxes::err_variant_name with the
// sub-reasons of Stage4RegionInvalid / SsiRefinementFailed (which name the
// SPECIFIC M5 gaps). Returns a stable &'static str so the bucket map keys are
// 'static. The ASSERT below requires no bucket maps to "unknown".
// =========================================================================

fn err_variant_name(e: &YangError) -> &'static str {
    match e {
        YangError::NonManifoldInput => "NonManifoldInput",
        YangError::NonManifoldOutput => "NonManifoldOutput",
        YangError::MeshBooleanFailed(_) => "MeshBooleanFailed",
        YangError::MalformedTopology(_) => "MalformedTopology",
        YangError::DegenerateFace { .. } => "DegenerateFace",
        YangError::FaceResolutionFailed { .. } => "FaceResolutionFailed",
        YangError::UnsupportedOp(_) => "UnsupportedOp",
        YangError::CurvedSurfaceNotYetSupported { .. } => "CurvedSurfaceNotYetSupported",
        YangError::Stage4ReversalUnresolved { .. } => "Stage4ReversalUnresolved",
        // Sub-reason-resolved buckets (the specific M5 gaps).
        YangError::SsiRefinementFailed { reason, .. } => match reason {
            yang_rs::SsiRefinementError::IntersectFailed(_) => {
                "SsiRefinementFailed::IntersectFailed"
            }
            yang_rs::SsiRefinementError::AmbiguousCurve { .. } => {
                "SsiRefinementFailed::AmbiguousCurve"
            }
            yang_rs::SsiRefinementError::UnsupportedCurve => {
                "SsiRefinementFailed::UnsupportedCurve"
            }
            yang_rs::SsiRefinementError::UnsupportedSurfaceForSsi => {
                "SsiRefinementFailed::UnsupportedSurfaceForSsi"
            }
        },
        YangError::Stage4RegionInvalid { reason, .. } => match reason {
            yang_rs::Stage4InvalidReason::OffCurveBeyondChordBand => {
                "Stage4RegionInvalid::OffCurveBeyondChordBand"
            }
            yang_rs::Stage4InvalidReason::OnAxis => "Stage4RegionInvalid::OnAxis",
            yang_rs::Stage4InvalidReason::EllipseProjectionUnsupported => {
                "Stage4RegionInvalid::EllipseProjectionUnsupported"
            }
            yang_rs::Stage4InvalidReason::InvertedTriangle => {
                "Stage4RegionInvalid::InvertedTriangle"
            }
            yang_rs::Stage4InvalidReason::DegenerateTriangle => {
                "Stage4RegionInvalid::DegenerateTriangle"
            }
            yang_rs::Stage4InvalidReason::LoopTooSmall => "Stage4RegionInvalid::LoopTooSmall",
            yang_rs::Stage4InvalidReason::LocalRefinementRequired => {
                "Stage4RegionInvalid::LocalRefinementRequired"
            }
        },
    }
}

// =========================================================================
// Buckets keyed by (primitive, op).
// =========================================================================

#[derive(Default)]
struct Buckets {
    ok_correct: usize,
    /// Subset of `ok_correct` whose χ != 2 (multi-shell / holed result) — a
    /// validity-preserving outcome reported separately as a robustness signal.
    ok_multi_shell: usize,
    silent_wrong: usize,
    /// `boolean()` PANICKED rather than returning a classified `Err`. A panic
    /// is a P9 violation (production must return `Result`, never unwind) — it
    /// is the LOUDEST possible failure, NOT a classified `Err`, so it is
    /// counted separately and the test FAILS on any panic. Caught with
    /// `catch_unwind` ONLY so the harness reports the FULL histogram + every
    /// panicking case rather than dying on the first one — this is a test-side
    /// diagnostic, never a production fallback.
    panicked: usize,
    /// `BRep::new` failure or sidecar reference-call error / timeout.
    skipped_bad_input: usize,
    errors: BTreeMap<&'static str, usize>,
}

impl Buckets {
    fn total(&self) -> usize {
        self.ok_correct
            + self.silent_wrong
            + self.panicked
            + self.skipped_bad_input
            + self.errors.values().sum::<usize>()
    }

    fn record_err(&mut self, name: &'static str) {
        *self.errors.entry(name).or_insert(0) += 1;
    }

    fn print(&self, label: &str) {
        eprintln!("  [{label}] total={}", self.total());
        eprintln!("    ok_correct        = {}", self.ok_correct);
        eprintln!(
            "      └ of which multi-shell/holed (χ≠2, valid) = {}",
            self.ok_multi_shell
        );
        eprintln!("    SILENT_WRONG      = {}", self.silent_wrong);
        eprintln!("    PANICKED          = {}", self.panicked);
        eprintln!("    skipped_bad_input = {}", self.skipped_bad_input);
        for (name, n) in &self.errors {
            eprintln!("    Err::{name:<42} = {n}");
        }
    }
}

/// A recorded silent-wrong case for the panic message.
struct SilentWrong {
    seed: u64,
    case: usize,
    primitive: &'static str,
    op: BoolOp,
    params: PrimParams,
    vol_y: f64,
    vol_ref: f64,
    unpaired: usize,
    euler: i64,
    euler_ref: i64,
    residual_max: Option<f64>,
    /// Which sub-checks failed (for the human reading the dump).
    failed: Vec<&'static str>,
}

impl std::fmt::Debug for SilentWrong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[seed={:#x} case#{} {} {:?}] FAILED={:?} vol_y={:.9} vol_ref={:.9} \
             unpaired={} euler={} euler_ref={} residual_max={:?} params={:?}",
            self.seed,
            self.case,
            self.primitive,
            self.op,
            self.failed,
            self.vol_y,
            self.vol_ref,
            self.unpaired,
            self.euler,
            self.euler_ref,
            self.residual_max,
            self.params,
        )
    }
}

/// A recorded panicking case — a P9 violation: `boolean()` unwound instead of
/// returning a classified `Err`. The most precise possible GREEN anchor.
struct Panicked {
    seed: u64,
    case: usize,
    primitive: &'static str,
    op: BoolOp,
    params: PrimParams,
    message: String,
}

impl std::fmt::Debug for Panicked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[seed={:#x} case#{} {} {:?}] PANIC: {} params={:?}",
            self.seed, self.case, self.primitive, self.op, self.message, self.params,
        )
    }
}

// =========================================================================
// Per-(primitive, op) bucket table.
// =========================================================================

fn key_index(primitive: Primitive, op: BoolOp) -> usize {
    let pi = match primitive {
        Primitive::Cylinder => 0,
        Primitive::Sphere => 1,
        Primitive::Cone => 2,
    };
    let oi = match op {
        BoolOp::Union => 0,
        _ => 1, // Subtract (Intersect/Xor not generated)
    };
    pi * 2 + oi
}

fn key_label(idx: usize) -> &'static str {
    match idx {
        0 => "cylinder/Union",
        1 => "cylinder/Subtract",
        2 => "sphere/Union",
        3 => "sphere/Subtract",
        4 => "cone/Union",
        5 => "cone/Subtract",
        _ => "??",
    }
}

// =========================================================================
// The harness.
// =========================================================================

#[test]
#[ignore = "deep curved fuzz: N=300 cases, ~1200 sidecar subprocess calls; run with --ignored"]
fn fuzz_curved_booleans() {
    // Reference-mesh oracle: the C++ sidecar (test-only parity oracle since
    // PR-CR-BL3c). Backend under test: the native cherchi-rs pipeline.
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!(
            "[fuzz_curved] SKIP: sidecar binary not found (SidecarBoolean::from_env() Err). \
             Set CHERCHI2022_BIN to run the curved fuzz."
        );
        return;
    };
    let Some(nb) = yang_rs::native_backend() else {
        eprintln!("[fuzz_curved] SKIP: native FFI shim not linked (stub build)");
        return;
    };

    // Silence the default panic hook for the duration of the fuzz so a caught
    // production panic does not flood the histogram with backtraces. We capture
    // the panic *message* ourselves (via catch_unwind's payload) and report it
    // in the Panicked record. Restored at the end.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut rng = SplitMix64::new(SEED);
    let mut buckets: Vec<Buckets> = (0..6).map(|_| Buckets::default()).collect();
    let mut silent: Vec<SilentWrong> = Vec::new();
    let mut panics: Vec<Panicked> = Vec::new();

    for case in 0..N_CASES {
        let c = gen_case(&mut rng);
        let bk = &mut buckets[key_index(c.primitive, c.op)];

        let prim = match c.prim_brep {
            Ok(b) => b,
            Err(_) => {
                bk.skipped_bad_input += 1;
                continue;
            }
        };
        let bx = match c.box_brep {
            Ok(b) => b,
            Err(_) => {
                bk.skipped_bad_input += 1;
                continue;
            }
        };

        // Reference mesh directly from the sidecar. If THIS errors/times out,
        // the random input is degenerate from the backend's view (not yang's
        // fault) → skip. boolean(prim, box, op) ⇒ sidecar(prim, box, op).
        let sidecar_direct = match MeshBoolean::boolean(&sb, prim.as_mesh(), bx.as_mesh(), c.op) {
            Ok(m) => m,
            Err(_) => {
                bk.skipped_bad_input += 1;
                continue;
            }
        };

        // Run `boolean()` under catch_unwind: a production panic is a P9
        // violation (must return `Err`, never unwind), but catching it lets the
        // harness report the FULL histogram + EVERY panicking case instead of
        // aborting on the first one. The test still FAILS loudly on any panic
        // (asserted below). This is a test-diagnostic, not a production path.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            boolean(&prim, &bx, c.op, &nb)
        }));

        let yang = match result {
            Ok(r) => r,
            Err(payload) => {
                let message = panic_message(&payload);
                bk.panicked += 1;
                panics.push(Panicked {
                    seed: SEED,
                    case,
                    primitive: c.primitive.name(),
                    op: c.op,
                    params: c.params,
                    message,
                });
                continue;
            }
        };

        match yang {
            Ok(brep) => {
                let mesh = brep.as_mesh();
                let vol_y = signed_volume(mesh);
                let vol_ref = signed_volume(&sidecar_direct);
                let unpaired = unpaired_half_edges(mesh);
                let euler = euler_characteristic(mesh);
                let euler_ref = euler_characteristic(&sidecar_direct);

                // EMPTY-RESULT agreement. The reference can legitimately be the
                // EMPTY solid — e.g. `sphere − box` where the box fully encloses
                // the sphere removes all material (vol = 0, χ = 0, no triangles).
                // An empty result is a *correct* boolean outcome, NOT silent-wrong:
                // the §3.3 surface-survival and §3.5 vol>0 gates presuppose a
                // non-empty solid and do NOT apply. The genuine correctness check
                // here is that yang AGREES with the reference on emptiness:
                //   - both empty  → ok_correct (engines agree the result is ∅).
                //   - they DISAGREE (one empty, the other not) → silent-wrong
                //     (yang invented or dropped a whole solid).
                let ref_empty = sidecar_direct.num_tris() == 0;
                let yang_empty = mesh.num_tris() == 0;
                if ref_empty || yang_empty {
                    if ref_empty == yang_empty {
                        bk.ok_correct += 1;
                    } else {
                        bk.silent_wrong += 1;
                        silent.push(SilentWrong {
                            seed: SEED,
                            case,
                            primitive: c.primitive.name(),
                            op: c.op,
                            params: c.params,
                            vol_y,
                            vol_ref,
                            unpaired,
                            euler,
                            euler_ref,
                            residual_max: None,
                            failed: vec![if yang_empty {
                                "yang_empty_ref_nonempty"
                            } else {
                                "yang_nonempty_ref_empty"
                            }],
                        });
                    }
                    continue;
                }

                let want = c.params.curved_surface();
                let (survives, foreign_curved) = surface_survival(&brep, want);
                let residual_max = on_surface_residual_max(&brep);
                // Survival is only REQUIRED when the curved surface actually
                // bounds the result (decided from the independent reference) —
                // an interior/consumed primitive correctly has no surviving
                // analytic face.
                let survival_required =
                    reference_surface_present(&sidecar_direct, want, c.params.chord_bound());

                // ---- correct-or-loud contract §3 ----
                let mut failed: Vec<&'static str> = Vec::new();

                // §3.1 watertight
                if unpaired != 0 {
                    failed.push("watertight");
                }
                // §3.2 Euler even AND == reference χ
                if euler % 2 != 0 {
                    failed.push("euler_odd");
                }
                if euler != euler_ref {
                    failed.push("euler_ne_ref");
                }
                // §3.3 analytic surface survival (only when the surface bounds
                // the result per the reference).
                if survival_required && !survives {
                    failed.push("surface_not_survived");
                }
                if foreign_curved {
                    failed.push("foreign_curved_surface");
                }
                // §3.4 on-surface exactness (strict gate). If no curved edge is
                // present at all on a curved-primitive boolean, that is itself a
                // survival failure already flagged by §3.3 (no curved surface
                // ⇒ no curved bounding edge); the residual gate only fires when
                // curved edges exist.
                if let Some(rmax) = residual_max {
                    if rmax > TAU_MODEL {
                        failed.push("on_surface_residual");
                    }
                }
                // §3.5 positive signed volume (NaN counts as a failure, not a
                // silent pass — an inside-out or corrupt mesh must be loud).
                if vol_y.is_nan() || vol_y <= 0.0 {
                    failed.push("nonpositive_volume");
                }
                // §3.6 chord-band volume sanity
                let band = chord_volume_band(&c.params);
                if (vol_y - vol_ref).abs() > band {
                    failed.push("volume_band");
                }

                if failed.is_empty() {
                    bk.ok_correct += 1;
                    if euler != 2 {
                        bk.ok_multi_shell += 1;
                    }
                } else {
                    bk.silent_wrong += 1;
                    silent.push(SilentWrong {
                        seed: SEED,
                        case,
                        primitive: c.primitive.name(),
                        op: c.op,
                        params: c.params,
                        vol_y,
                        vol_ref,
                        unpaired,
                        euler,
                        euler_ref,
                        residual_max,
                        failed,
                    });
                }
            }
            Err(e) => bk.record_err(err_variant_name(&e)),
        }
    }

    // Restore the default panic hook now that the fuzz loop is done.
    std::panic::set_hook(prev_hook);

    // ---- Histogram ----
    eprintln!("\n========== fuzz_curved histogram (seed={SEED:#x}) ==========");
    eprintln!("N_CASES = {N_CASES} (each: random primitive × random op; none truncated)");
    for (idx, bk) in buckets.iter().enumerate() {
        bk.print(key_label(idx));
    }

    let total_ok: usize = buckets.iter().map(|b| b.ok_correct).sum();
    let total_silent: usize = buckets.iter().map(|b| b.silent_wrong).sum();
    let total_panicked: usize = buckets.iter().map(|b| b.panicked).sum();
    let total_skipped: usize = buckets.iter().map(|b| b.skipped_bad_input).sum();
    let total_err: usize = buckets
        .iter()
        .map(|b| b.errors.values().sum::<usize>())
        .sum();
    let total_seen: usize = buckets.iter().map(|b| b.total()).sum();

    eprintln!("  ----------------------------------------");
    eprintln!(
        "  TOTALS: seen={total_seen} ok_correct={total_ok} SILENT_WRONG={total_silent} \
               PANICKED={total_panicked} classified_err={total_err} \
               skipped_bad_input={total_skipped}"
    );
    eprintln!("  (seen MUST equal N_CASES={N_CASES} — every case accounted for)");
    eprintln!("============================================================\n");

    // Sanity: every case is accounted for (no silent loss on the case axis).
    assert_eq!(
        total_seen, N_CASES,
        "case-accounting leak: {total_seen} outcomes recorded for {N_CASES} cases"
    );

    // ---- No unclassified Err bucket. ----
    // err_variant_name returns a 'static name for EVERY YangError variant +
    // sub-reason, so an "unknown" key can never appear; assert defensively that
    // none did (a future YangError variant added without updating the match
    // would surface here as a compile error in err_variant_name, not a runtime
    // "unknown" — but we keep the invariant explicit).
    for bk in &buckets {
        assert!(
            !bk.errors.contains_key("unknown"),
            "an unclassified Err bucket appeared — err_variant_name must map every variant"
        );
    }

    // ---- Always dump the full PANICKED + SILENT_WRONG case lists BEFORE any
    // assert, so a `--ignored --nocapture` run records every finding even
    // though the first failing assert below aborts the test. (The asserts that
    // follow are the CI gate; the dumps are the diagnostic record.)
    if !panics.is_empty() {
        eprintln!("---- PANICKED cases ({}) ----", panics.len());
        for pc in &panics {
            eprintln!("  {pc:?}");
        }
    }
    if !silent.is_empty() {
        eprintln!("---- SILENT_WRONG cases ({}) ----", silent.len());
        for sw in &silent {
            eprintln!("  {sw:?}");
        }
    }

    // ---- HARD ASSERT: production must NEVER panic (P9). ----
    // A panic in `boolean()` is the loudest failure but is NOT a classified
    // `Err` — production owes the caller a `Result`. Any panicking case fails
    // the test with the full case dump (the GREEN anchor).
    assert_eq!(
        total_panicked, 0,
        "PANICKED = {} — yang-rs curved boolean PANICKED instead of returning a classified \
         Err (a P9 violation: production must return Result, never unwind). Cases:\n{:#?}",
        total_panicked, panics
    );

    // ---- HARD ASSERT: no silently-wrong Ok results. ----
    assert_eq!(
        total_silent, 0,
        "SILENT_WRONG = {} — yang-rs curved boolean produced Ok results that fail the \
         correct-or-loud contract (watertight / euler==ref-χ & even / surface-survival / \
         on-surface≤TAU_MODEL / vol>0 / chord-band). Cases:\n{:#?}",
        total_silent, silent
    );
}

// =========================================================================
// DEMONSTRATOR (PR-CF1 finding) — reproduces the ONE genuine production bug the
// fuzz surfaced: `sphere − box` at SEED case #23 PANICS inside `boolean()`
// (`emit_topology` curved branch indexes `cycles[outer_idx]` with an EMPTY
// `cycles`, src/lib.rs ~4132 — a P9 violation: production must return `Err`,
// never unwind). The empty cycle set arises for this kept-Sphere subtract patch
// (the box fully encloses the sphere region → the curved patch has no boundary
// loop), which the curved branch's E2-guard does not cover (it iterates an
// empty `cycles`, then unconditionally indexes `cycles[0]`).
//
// This is a DEMONSTRATOR, not a correctness assertion: it deterministically
// replays the case stream to case #23, runs `boolean()` under `catch_unwind`,
// and REPORTS the outcome (panic today; a classified `Err` once GREEN adds the
// `cycles.is_empty()` guard mirroring the planar E3 check). It asserts NOTHING
// about correctness, so it neither fails the default suite (it is `#[ignore]`d
// and sidecar-gated) nor becomes a liability once the bug is fixed. The
// asserting fuzz above (`fuzz_curved_booleans`) is the real gate; this just
// pins the seed for the follow-up GREEN increment.
//
// Run: `CHERCHI2022_BIN=… cargo test -p yang-rs --test fuzz_curved -- \
//        --ignored demonstrator_case23 --nocapture`
// =========================================================================

const DEMONSTRATOR_CASE: usize = 23;

#[test]
#[ignore = "demonstrator: reproduces the sphere−box PANIC at SEED case #23 (src/lib.rs ~4132 \
            empty-cycles index in emit_topology curved branch); reports outcome, asserts nothing"]
fn demonstrator_case23_sphere_subtract_box_panics() {
    let Some(nb) = yang_rs::native_backend() else {
        eprintln!("[fuzz_curved demonstrator] SKIP: native FFI shim not linked (stub build)");
        return;
    };

    // Replay the EXACT deterministic case stream to case #23 (same SEED + same
    // gen_case draw order as the main fuzz, so this is the same case).
    let mut rng = SplitMix64::new(SEED);
    let mut target: Option<Case> = None;
    for case in 0..=DEMONSTRATOR_CASE {
        let c = gen_case(&mut rng);
        if case == DEMONSTRATOR_CASE {
            target = Some(c);
        }
    }
    let c = target.expect("case #23 generated");
    eprintln!(
        "[demonstrator] case#{DEMONSTRATOR_CASE} primitive={} op={:?} params={:?}",
        c.primitive.name(),
        c.op,
        c.params
    );

    let prim = c.prim_brep.expect("case#23 prim BRep::new should succeed");
    let bx = c.box_brep.expect("case#23 box BRep::new should succeed");

    // Suppress the panic hook so the demonstrator output is clean.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        boolean(&prim, &bx, c.op, &nb)
    }));
    std::panic::set_hook(prev_hook);

    match outcome {
        Err(payload) => {
            eprintln!(
                "[demonstrator] REPRODUCED: boolean() PANICKED — {} \
                 (the documented P9 bug; GREEN should convert this to a classified Err)",
                panic_message(&payload)
            );
        }
        Ok(Ok(brep)) => {
            eprintln!(
                "[demonstrator] boolean() returned Ok ({} faces, {} tris) — \
                 the panic appears FIXED; flip/retire this demonstrator.",
                brep.faces().len(),
                brep.as_mesh().num_tris()
            );
        }
        Ok(Err(e)) => {
            eprintln!(
                "[demonstrator] boolean() returned a classified Err ({}) — \
                 the panic appears FIXED (now loud-and-classified); flip/retire this demonstrator.",
                err_variant_name(&e)
            );
        }
    }
}

// =========================================================================
// Compile-time anchor: keep the imported `Error` trait referenced so an
// unused-import lint never fires if the harness is edited to drop the explicit
// `MeshBoolean::boolean` UFCS call. (cheap, documents intent.)
// =========================================================================
#[allow(dead_code)]
fn _assert_error_trait_in_scope() {
    fn _takes<E: Error>(_: E) {}
}
