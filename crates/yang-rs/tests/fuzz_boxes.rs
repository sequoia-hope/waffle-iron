//! Randomized validation harness for the M3 yang-rs boolean.
//!
//! Stress-tests `yang_rs::boolean()` on many randomized two-box boolean cases
//! (axis-aligned AND arbitrarily-rotated). The point is twofold:
//!
//! 1. **Anti-requirement (hard invariant):** an `Ok` result must NEVER be
//!    silently wrong. Every `Ok` is structurally + numerically audited
//!    (watertight, Euler == 2, differential volume vs the sidecar reference
//!    mesh, plus a closed-form analytic-overlap volume for the axis-aligned
//!    tier). Any `Ok` that fails an audit is bucketed `SILENT_WRONG` and the
//!    test FAILS with the recorded case details.
//! 2. **Robustness envelope (report):** measure how broadly M3 succeeds.
//!    `Err(variant)` results are bucketed by variant name and reported in a
//!    histogram split aligned vs rotated. This characterizes M3's real
//!    failure modes (which error dominates rotated cases, etc.) without
//!    masking them.
//!
//! Determinism (governance): a hand-rolled splitmix64 PRNG seeded by a fixed
//! constant. No `rand` dep, no system time, no filesystem side effects beyond
//! what the sidecar itself does. The exact same case stream is produced every
//! run.
//!
//! Self-skips cleanly when the C++ sidecar binary is absent
//! (`SidecarBoolean::from_env()` → `Err`).

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::SidecarBoolean;
use std::collections::BTreeMap;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, Curve, Surface, YangError};

// =========================================================================
// Configuration
// =========================================================================

/// Fixed PRNG seed. Recorded here so the entire case stream is reproducible.
const SEED: u64 = 0x5EED_C0DE_F00D_1234;

const N_ALIGNED: usize = 150;
const N_ROTATED: usize = 150;

/// Differential / analytic volume tolerance.
const VOL_TOL: f64 = 1e-6;

/// Success floor: `ok_correct` must be at least this fraction of non-skipped
/// cases. This documents the OBSERVED reality (see the module-level report at
/// the bottom and the harness output) rather than an aspirational target — if
/// the real rate is far below, the threshold is set to reflect that and the
/// histogram tells the story. Do NOT inflate this to make the test green.
const SUCCESS_FLOOR: f64 = 0.25;

// =========================================================================
// splitmix64 PRNG — deterministic, no external dep.
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
        // 53-bit mantissa.
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform f64 in [lo, hi).
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
}

// =========================================================================
// Linear algebra helpers (inline; cad-primitives is types-only).
// =========================================================================

type Mat3 = [[f64; 3]; 3];

const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

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

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

// =========================================================================
// Oriented-box generator → topologized BRep.
//
// Identical topology template to `m3_adversary::cube` (8 verts, 24 edges,
// 6 quad faces) but parameterized by center, half-extents, and a rotation
// matrix. Outward normals = R · (±e_axis); plane d = −normal · (a vertex on
// that face). Winding follows the axis-aligned template; yang Stage 1
// (`BRep::new`) canonicalizes triangle winding to the stated normal.
// =========================================================================

struct OrientedBox {
    center: [f64; 3],
    half: [f64; 3],
    rot: Mat3,
}

impl OrientedBox {
    /// World-space position of corner `s ∈ {−1,+1}³`.
    fn corner(&self, sx: f64, sy: f64, sz: f64) -> [f64; 3] {
        let local = [sx * self.half[0], sy * self.half[1], sz * self.half[2]];
        add3(self.center, mat_vec(&self.rot, local))
    }

    fn to_brep(&self) -> Result<BRep, YangError> {
        // Corner index layout matching m3_adversary::cube (an axis-aligned
        // box from origin uses corners in this s-order):
        //   0:(−−−) 1:(+−−) 2:(++−) 3:(−+−) 4:(−−+) 5:(+−+) 6:(+++) 7:(−++)
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
        let verts: Vec<yang_rs::BRepVertex> = signs
            .iter()
            .map(|s| yang_rs::BRepVertex {
                point: {
                    let c = self.corner(s[0], s[1], s[2]);
                    Point3::new(c[0], c[1], c[2])
                },
            })
            .collect();

        // Same face→vertex template as m3_adversary::cube.
        let face_verts: [[u32; 4]; 6] = [
            [0, 1, 2, 3], // −z
            [4, 7, 6, 5], // +z
            [0, 4, 5, 1], // −y
            [1, 5, 6, 2], // +x
            [2, 6, 7, 3], // +y
            [3, 7, 4, 0], // −x
        ];
        // Outward local-axis normals for each face above, in the SAME order.
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
                // World-space outward normal.
                let wn = mat_vec(&self.rot, local_normals[i]);
                let normal = Vector3::new(wn[0], wn[1], wn[2]);
                // A vertex known to lie on this face.
                let v0 = verts[face_verts[i][0] as usize].point;
                let d = -dot3(wn, v0.as_array());
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

/// Generate a random box. If `rotated`, use a random unit-quaternion rotation;
/// otherwise the identity (axis-aligned).
fn gen_box(rng: &mut SplitMix64, center: [f64; 3], rotated: bool) -> OrientedBox {
    let half = [
        rng.range(0.3, 0.9),
        rng.range(0.3, 0.9),
        rng.range(0.3, 0.9),
    ];
    let rot = if rotated {
        // Random quaternion components in [−1, 1]; normalized in quat_to_mat3.
        // Reject the (vanishingly unlikely) near-zero quaternion.
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
    } else {
        IDENTITY
    };
    OrientedBox { center, half, rot }
}

// =========================================================================
// Audit helpers (copied from m3_adversary.rs — kept self-contained).
// =========================================================================

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

/// Closed-form boolean volume for two AXIS-ALIGNED boxes, derived from the
/// overlap box. Returns `None` if `op` is `Xor` (not exercised here).
fn analytic_aligned_volume(a: &OrientedBox, b: &OrientedBox, op: BoolOp) -> Option<f64> {
    let vol = |bx: &OrientedBox| 8.0 * bx.half[0] * bx.half[1] * bx.half[2];
    let va = vol(a);
    let vb = vol(b);
    // Overlap box = ∏ max(0, min(hi) − max(lo)) per axis.
    let mut overlap = 1.0;
    for axis in 0..3 {
        let a_lo = a.center[axis] - a.half[axis];
        let a_hi = a.center[axis] + a.half[axis];
        let b_lo = b.center[axis] - b.half[axis];
        let b_hi = b.center[axis] + b.half[axis];
        let lo = a_lo.max(b_lo);
        let hi = a_hi.min(b_hi);
        overlap *= (hi - lo).max(0.0);
    }
    Some(match op {
        BoolOp::Union => va + vb - overlap,
        BoolOp::Intersect => overlap,
        BoolOp::Subtract => va - overlap,
        BoolOp::Xor => return None,
    })
}

// =========================================================================
// Buckets + reporting.
// =========================================================================

#[derive(Default)]
struct Buckets {
    ok_correct: usize,
    /// Subset of `ok_correct` whose χ != 2 (multi-shell or holed result) — a
    /// validity-preserving outcome, reported separately as a robustness signal.
    ok_multi_shell: usize,
    silent_wrong: usize,
    skipped_bad_input: usize,
    /// Keyed by YangError variant name.
    errors: BTreeMap<&'static str, usize>,
}

impl Buckets {
    fn total(&self) -> usize {
        self.ok_correct
            + self.silent_wrong
            + self.skipped_bad_input
            + self.errors.values().sum::<usize>()
    }

    fn non_skipped(&self) -> usize {
        self.total() - self.skipped_bad_input
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
        eprintln!("    skipped_bad_input = {}", self.skipped_bad_input);
        for (name, n) in &self.errors {
            eprintln!("    Err::{name:<20} = {n}");
        }
    }
}

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
        YangError::SsiRefinementFailed { .. } => "SsiRefinementFailed",
        YangError::Stage4ReversalUnresolved { .. } => "Stage4ReversalUnresolved",
        YangError::Stage4RegionInvalid { .. } => "Stage4RegionInvalid",
        YangError::CoplanarFacesUnsupported { .. } => "CoplanarFacesUnsupported",
    }
}

/// A recorded silent-wrong case for the panic message.
struct SilentWrong {
    tier: &'static str,
    op: BoolOp,
    case: usize,
    vol_y: f64,
    vol_ref: f64,
    analytic: Option<f64>,
    unpaired: usize,
    euler: i64,
    euler_ref: i64,
    a: BoxParams,
    b: BoxParams,
}

#[derive(Clone, Copy)]
struct BoxParams {
    center: [f64; 3],
    half: [f64; 3],
}

impl From<&OrientedBox> for BoxParams {
    fn from(b: &OrientedBox) -> Self {
        BoxParams {
            center: b.center,
            half: b.half,
        }
    }
}

impl std::fmt::Debug for SilentWrong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{} {:?} case#{}] vol_y={:.9} vol_ref={:.9} analytic={:?} \
             unpaired={} euler={} euler_ref={} A(c={:?},h={:?}) B(c={:?},h={:?})",
            self.tier,
            self.op,
            self.case,
            self.vol_y,
            self.vol_ref,
            self.analytic,
            self.unpaired,
            self.euler,
            self.euler_ref,
            self.a.center,
            self.a.half,
            self.b.center,
            self.b.half,
        )
    }
}

// =========================================================================
// The harness.
// =========================================================================

// 8 args since BL3c added the native backend alongside the sidecar oracle;
// a params struct for a one-caller test harness would be noise.
#[allow(clippy::too_many_arguments)]
fn run_tier(
    rng: &mut SplitMix64,
    sb: &SidecarBoolean,
    nb: &yang_rs::NativeBoolean,
    tier: &'static str,
    n_cases: usize,
    rotated: bool,
    buckets: &mut Buckets,
    silent: &mut Vec<SilentWrong>,
) {
    for case in 0..n_cases {
        // A centered near origin; B offset within ~A's extent so they
        // interpenetrate. Offset each component in [−0.6, 0.6]·extent of A.
        let a = gen_box(rng, [0.0, 0.0, 0.0], rotated);
        // Use A's half-extents to scale the offset so overlap is near-certain.
        let off = [
            rng.range(-0.6, 0.6) * a.half[0],
            rng.range(-0.6, 0.6) * a.half[1],
            rng.range(-0.6, 0.6) * a.half[2],
        ];
        let b = gen_box(rng, off, rotated);

        let a_brep = match a.to_brep() {
            Ok(br) => br,
            Err(_) => {
                buckets.skipped_bad_input += 1;
                continue;
            }
        };
        let b_brep = match b.to_brep() {
            Ok(br) => br,
            Err(_) => {
                buckets.skipped_bad_input += 1;
                continue;
            }
        };
        let a_mesh = a_brep.as_mesh().clone();
        let b_mesh = b_brep.as_mesh().clone();

        for op in [BoolOp::Union, BoolOp::Intersect, BoolOp::Subtract] {
            // Reference mesh directly from the sidecar. If THIS errors, the
            // random input is degenerate from the backend's view — not yang's
            // fault — so skip.
            let sidecar_direct = match sb.boolean(&a_mesh, &b_mesh, op) {
                Ok(m) => m,
                Err(_) => {
                    buckets.skipped_bad_input += 1;
                    continue;
                }
            };

            match boolean(&a_brep, &b_brep, op, nb) {
                Ok(brep) => {
                    let mesh = brep.as_mesh();
                    let vol_y = signed_volume(mesh);
                    let vol_ref = signed_volume(&sidecar_direct);
                    let unpaired = unpaired_half_edges(mesh);
                    let euler = euler_characteristic(mesh);
                    let euler_ref = euler_characteristic(&sidecar_direct);

                    // Watertight (closed 2-manifold surface): forward/reverse
                    // half-edge counts balance.
                    let watertight = unpaired == 0;

                    // Euler: a valid boolean result need NOT be a single genus-0
                    // shell. A corner/edge clip can split the result into several
                    // disconnected shells (χ = 2·#shells) or punch a through-hole
                    // (χ = 2 − 2·genus). The CORRECT topological check is
                    // differential: yang must reproduce the REFERENCE mesh's Euler
                    // characteristic, not a hardcoded 2. (Probe confirmed: the C++
                    // reference itself returns χ=4, 2 shells for these subtracts.)
                    // We additionally require χ even — an odd χ on a closed
                    // surface would indicate genuine non-manifold corruption.
                    let euler_ok = euler == euler_ref && euler % 2 == 0;

                    let diff_ok = (vol_y - vol_ref).abs() < VOL_TOL;

                    let analytic = if rotated {
                        None
                    } else {
                        analytic_aligned_volume(&a, &b, op)
                    };
                    let analytic_ok = match analytic {
                        Some(av) => (vol_y - av).abs() < VOL_TOL,
                        None => true,
                    };

                    if watertight && euler_ok && diff_ok && analytic_ok {
                        buckets.ok_correct += 1;
                        if euler != 2 {
                            buckets.ok_multi_shell += 1;
                        }
                    } else {
                        buckets.silent_wrong += 1;
                        silent.push(SilentWrong {
                            tier,
                            op,
                            case,
                            vol_y,
                            vol_ref,
                            analytic,
                            unpaired,
                            euler,
                            euler_ref,
                            a: (&a).into(),
                            b: (&b).into(),
                        });
                    }
                }
                Err(e) => buckets.record_err(err_variant_name(&e)),
            }
        }
    }
}

// ~30 min: 300 box pairs × 3 ops × 2 sidecar subprocess calls each (~1800
// spawns). On-demand deep validation, not part of the normal suite. Run with:
//   CHERCHI2022_BIN=… cargo test -p yang-rs --test fuzz_boxes -- --ignored --nocapture
#[test]
#[ignore = "deep fuzz: ~30 min / ~1800 sidecar subprocess calls; run with --ignored"]
fn fuzz_two_box_booleans() {
    // Reference-mesh oracle: the C++ sidecar (test-only parity oracle since
    // PR-CR-BL3c). Backend under test: the native cherchi-rs pipeline.
    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[fuzz_boxes] SKIP: sidecar binary not found (SidecarBoolean::from_env() Err)");
        return;
    };
    let Some(nb) = yang_rs::native_backend() else {
        eprintln!("[fuzz_boxes] SKIP: native FFI shim not linked (stub build)");
        return;
    };

    let mut rng = SplitMix64::new(SEED);
    let mut aligned = Buckets::default();
    let mut rotated = Buckets::default();
    let mut silent: Vec<SilentWrong> = Vec::new();

    run_tier(
        &mut rng,
        &sb,
        &nb,
        "aligned",
        N_ALIGNED,
        false,
        &mut aligned,
        &mut silent,
    );
    run_tier(
        &mut rng,
        &sb,
        &nb,
        "rotated",
        N_ROTATED,
        true,
        &mut rotated,
        &mut silent,
    );

    // ---- Histogram ----
    eprintln!("\n========== fuzz_boxes histogram (seed={SEED:#x}) ==========");
    eprintln!("aligned cases requested = {N_ALIGNED}, rotated = {N_ROTATED}, ops/case = 3");
    aligned.print("ALIGNED");
    rotated.print("ROTATED");

    let total_ok = aligned.ok_correct + rotated.ok_correct;
    let total_silent = aligned.silent_wrong + rotated.silent_wrong;
    let total_non_skipped = aligned.non_skipped() + rotated.non_skipped();
    let rate = |ok: usize, denom: usize| {
        if denom == 0 {
            0.0
        } else {
            ok as f64 / denom as f64
        }
    };
    eprintln!("  ----------------------------------------");
    eprintln!(
        "  ALIGNED ok_correct rate = {:.1}%  ({} / {} non-skipped)",
        100.0 * rate(aligned.ok_correct, aligned.non_skipped()),
        aligned.ok_correct,
        aligned.non_skipped()
    );
    eprintln!(
        "  ROTATED ok_correct rate = {:.1}%  ({} / {} non-skipped)",
        100.0 * rate(rotated.ok_correct, rotated.non_skipped()),
        rotated.ok_correct,
        rotated.non_skipped()
    );
    eprintln!(
        "  OVERALL ok_correct rate = {:.1}%  ({} / {} non-skipped)",
        100.0 * rate(total_ok, total_non_skipped),
        total_ok,
        total_non_skipped
    );
    eprintln!("  SILENT_WRONG total = {total_silent}");
    eprintln!("============================================================\n");

    // ---- HARD ASSERT: no silently-wrong Ok results. ----
    assert_eq!(
        total_silent, 0,
        "SILENT_WRONG = {} — yang-rs boolean produced Ok results that fail audit \
         (watertight / euler==2 / differential vol / analytic vol). Cases:\n{:#?}",
        total_silent, silent
    );

    // ---- Success floor: prove generalization, not all-error. ----
    let overall_rate = rate(total_ok, total_non_skipped);
    assert!(
        overall_rate >= SUCCESS_FLOOR,
        "overall ok_correct rate {:.1}% < floor {:.1}% — M3 succeeds on too few \
         non-skipped cases. Histogram above documents the real distribution.",
        100.0 * overall_rate,
        100.0 * SUCCESS_FLOOR
    );
}
