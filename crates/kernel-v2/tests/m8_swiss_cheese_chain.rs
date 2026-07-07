//! Task #62 — chained swiss-cheese plates (F0086–F0090 corpus family).
//!
//! A disc plate takes SUCCESSIVE cut-cylinders, every tool sketched on the
//! SAME z=0 plane (same-normal coplanar bottom caps, the production
//! feature-engine pattern). Each cut's OUTPUT re-enters the next boolean, so
//! the chain exercises output curve recovery (`recover.rs`) on rims whose
//! vertex spacing is NON-UNIFORM: the coplanar overlay's sweep events mint
//! dense crossing clusters on the rim rings, and the z=0 rim carries
//! on-chord overlay boundary points (off-circle), so the recovered lateral
//! loses its canonical anchor and the top rim takes the closed-chain
//! 3-piece arc fallback.
//!
//! RED (this file's reason for existing): `closed_fallback_pieces` split at
//! VERTEX-COUNT thirds — with cluster spacing a "third" can subtend > π, the
//! downstream minor-side arc derivation picks the wrong side, and the
//! reassembled outer lateral's top rim walks out-and-back (net winding 0)
//! → `CurvedGeometryMismatch("cylinder patch must have exactly 0 or 2
//! axis-wrapping loops")` at step 2. GREEN: sweep-aware fallback splitting
//! (every piece < MAX_ARC_PIECE_SWEEP by ACCUMULATED sweep).
//!
//! Fixture values are F0086's bit-exact parameters (seed 30001).

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, tessellate, validate_solid, BrepArena, Profile, RenderMesh};

const R: f64 = 1.4518544955342536;
const T: f64 = 0.4517828694874588;
const HR: f64 = 0.06748980564806449;
/// (cx, cy, cut depth) per hole — 3 through (depth > T), 2 blind.
const HOLES: [(f64, f64, f64); 5] = [
    (-0.4844834245158292, -0.3149130149828976, 1.1586804105234212),
    (
        -0.14355049103322348,
        -0.07372970251577235,
        1.0922233379071158,
    ),
    (0.0493293771266266, 0.7410538596365673, 1.046704),
    (0.8472894945087677, -0.7585876572737864, 0.214926),
    (0.5668457676559464, 1.0744567510873022, 0.300221),
];

fn cyl_in_frame(
    a: &mut BrepArena,
    frame: (Vector3, Vector3),
    cx: f64,
    cy: f64,
    r: f64,
    z0: f64,
    z1: f64,
) -> kernel_v2::SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, z0),
        frame.0,
        frame.1,
        Point2::new(cx, cy),
        r,
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z1 - z0)
        .unwrap()
        .solid
}

fn cyl(a: &mut BrepArena, cx: f64, cy: f64, r: f64, z0: f64, z1: f64) -> kernel_v2::SolidId {
    let frame = (Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0));
    cyl_in_frame(a, frame, cx, cy, r, z0, z1)
}

/// The PRODUCTION sketch frame for a z=0 / +z-normal plane:
/// `tangent_x_from_normal([0,0,1])` = X×n̂ = (0,−1,0), y = n̂×x = (1,0,0)
/// (feature-engine `rebuild.rs`, mirroring the JS `buildSketchPlane`). The
/// corpus replay (sketch-extrude + auto-union) builds every profile in THIS
/// frame — the whole scene is the canonical-frame scene rotated −90° about
/// z (bit-exact swap/negate), but the Stage-0 overlay's sweep-event ORDER
/// differs under the rotation, reaching mint/fold configurations the
/// canonical frame never exercises.
fn cyl_engine_frame(
    a: &mut BrepArena,
    cx: f64,
    cy: f64,
    r: f64,
    z0: f64,
    z1: f64,
) -> kernel_v2::SolidId {
    let frame = (Vector3::new(0.0, -1.0, 0.0), Vector3::new(1.0, 0.0, 0.0));
    cyl_in_frame(a, frame, cx, cy, r, z0, z1)
}

fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let mut six_v = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

fn run_chain_with(
    build: fn(&mut BrepArena, f64, f64, f64, f64, f64) -> kernel_v2::SolidId,
    n_holes: usize,
) -> (BrepArena, kernel_v2::SolidId) {
    let mut a = BrepArena::new();
    let mut body = build(&mut a, 0.0, 0.0, R, 0.0, T);
    for (i, &(hx, hy, d)) in HOLES.iter().take(n_holes).enumerate() {
        let tool = build(&mut a, hx, hy, HR, 0.0, d);
        body = boolean_op(&mut a, body, tool, BoolOp::Subtract)
            .unwrap_or_else(|e| panic!("swiss-cheese cut {} failed: {e:?}", i + 1));
        validate_solid(&a, body).unwrap_or_else(|e| panic!("cut {} output invalid: {e:?}", i + 1));
    }
    (a, body)
}

fn run_chain(n_holes: usize) -> (BrepArena, kernel_v2::SolidId) {
    run_chain_with(cyl, n_holes)
}

/// Analytic volume with each circle discounted to its inscribed chord
/// polygon is scale-dependent; a 1.5% relative band around the ANALYTIC
/// volume rejects a dropped cap / doubled sheet / missing hole while
/// tolerating the Stage-1 chord deficit (N≈52 on the plate ⇒ 0.24%).
fn assert_volume(a: &BrepArena, s: kernel_v2::SolidId, n_holes: usize) {
    let mesh = tessellate(a, s).expect("tessellate");
    let vol = mesh_signed_volume(&mesh);
    let mut analytic = std::f64::consts::PI * R * R * T;
    for &(_, _, d) in HOLES.iter().take(n_holes) {
        analytic -= std::f64::consts::PI * HR * HR * d.min(T);
    }
    assert!(
        (vol - analytic).abs() / analytic < 0.015,
        "{n_holes}-hole plate volume {vol} outside band of analytic {analytic}"
    );
    assert!(vol > 0.0, "volume must be positive");
}

/// The minimal chained case: base disc + TWO through holes. The second cut
/// re-enters the recovered 1-hole plate — the F0086 step-2 wall.
#[test]
fn two_through_holes_chain() {
    let (a, s) = run_chain(2);
    assert_volume(&a, s, 2);
}

/// The full F0086 recipe: 3 through + 2 blind holes, all chained. GREEN
/// since M8 increment 6 (task #62): `rim_chord_ctxs` mints crossing points
/// on an ANNULAR face's rim circles (outer + per-hole), so chained outputs
/// carry pure on-circle z=0 rims that recover can circle-fuse and re-enter.
#[test]
fn full_f0086_five_hole_chain() {
    let (a, s) = run_chain(5);
    assert_volume(&a, s, 5);
}

/// M8 increment 7 (task #62, spec `n2_stage4_junction_cluster_merge` §3
/// amendment 4): the CORPUS-path residual, reproduced directly. In the
/// production sketch frame, cut 2's Stage-0 overlay carries a femto-strip
/// (two sweep-event columns ULPs apart in u) whose diagonal sliver spans
/// from a rim-chord vertex up to the overlap boundary; ANY on-circle mint
/// of that rim vertex inverts the sliver, the fold gate reverted the mint,
/// and the chord-position vertex escaped into the output rims —
/// `VertexOffSurface(FaceId 15)`, residual 3.4e-3 (the Stage-1 sagitta) vs
/// the 2.5e-9 import band. RED before amendment 4's constrained flip
/// repair; GREEN with the mint kept and the sliver locally re-triangulated.
#[test]
fn engine_frame_two_hole_chain() {
    let (a, s) = run_chain_with(cyl_engine_frame, 2);
    assert_volume(&a, s, 2);
}

/// The full F0086 recipe in the production sketch frame — the corpus
/// replay's exact geometry (see `engine_frame_two_hole_chain`).
#[test]
fn engine_frame_five_hole_chain() {
    let (a, s) = run_chain_with(cyl_engine_frame, 5);
    assert_volume(&a, s, 5);
}

/// Regression for the retired cut-3 re-entry wall (was: the pin
/// `third_cut_stays_loud_typed_reentry_wall`, which asserted the TYPED
/// `UnsupportedCurvedBoolean` boundary until M8 increment 6 lifted it):
/// the third chained cut — the first to re-enter a MULTI-hole recovered
/// plate — must succeed with a fully valid output.
#[test]
fn third_cut_reenters_multi_hole_plate() {
    let mut a = BrepArena::new();
    let mut body = cyl(&mut a, 0.0, 0.0, R, 0.0, T);
    for &(hx, hy, d) in HOLES.iter().take(2) {
        let tool = cyl(&mut a, hx, hy, HR, 0.0, d);
        body = boolean_op(&mut a, body, tool, BoolOp::Subtract).expect("first two cuts are green");
    }
    let (hx, hy, d) = HOLES[2];
    let tool = cyl(&mut a, hx, hy, HR, 0.0, d);
    let s = boolean_op(&mut a, body, tool, BoolOp::Subtract)
        .expect("cut 3 re-entry (multi-hole plate) regressed to an error");
    validate_solid(&a, s).expect("cut 3 succeeded but output is invalid");
}

// ── F0087 fixture (seed 30002): the increment-8 wall ──────────────────────
// A LARGER plate (r≈1.98) tessellates at the same global N=14 (Stage-1
// chord bound d_ε = 1e-2·AABB-diag), so its rim-chord mints displace by up
// to ~5.6e-2 — enough to HOP a populated sweep-event column (e.g. the
// current tool's leftmost-x extreme, gap 8.9e-3 at cut 7). The whole strip
// of long CDT triangles between the columns folds together; amendment 4's
// single edge flips cannot repair a multi-column hop (each folded tri's rim
// edge is domain boundary, its side edges neighbor other FOLDED tris), the
// gate reverts, and the chord vertices escape — `VertexOffSurface`.
// Increment 8's scope: boundary-vertex relocation with cavity
// re-triangulation ([#24 Yang §4.4.1 Fig 11] delete-and-reinsert; the fold
// cavity's boundary polygon is non-simple under the moved vertex, so flips
// and fan re-triangulation both provably cannot fix it).

const F87_R: f64 = 1.980614275128782;
const F87_T: f64 = 0.2957032583668985;
const F87_HR: f64 = 0.06795332546654638;
/// All 10 of F0087's holes (3 through, 7 blind; every hole disjoint and
/// strictly interior to the plate — no hole-hole overlap, no rim contact).
/// Cut 7 was the increment-8 wall (rim-mint column hop); cut 10 is the
/// increment-9 wall (partial-patch operand re-entry).
const F87_HOLES: [(f64, f64, f64); 10] = [
    (1.2057456832734317, -0.7407823118143758, 0.8793439139633998),
    (1.3247776006386347, 0.32932385946597764, 0.5419497090202422),
    (0.01070787823164016, 0.47572151833582316, 0.6340016327568604),
    (1.1071210367136741, -1.4684405095030146, 0.8405076970234028),
    (-0.3405596239373243, 0.6167097868169978, 0.5008573650944282),
    (-1.393454312371825, -1.1027644265256487, 0.18704715810097103),
    (1.2093902365320994, 0.6573884082549064, 0.1703244805285629),
    (0.47998012577504184, 0.2760199907593817, 0.14035120129048992),
    (
        0.3512852634623234,
        -0.27379194464433526,
        0.19645927988861972,
    ),
    (
        -0.6561340162470487,
        -0.4773169472712928,
        0.21998259626864872,
    ),
];

fn run_f0087_chain(
    n_holes: usize,
) -> Result<(BrepArena, kernel_v2::SolidId), kernel_v2::KernelV2Error> {
    let mut a = BrepArena::new();
    let mut body = cyl_engine_frame(&mut a, 0.0, 0.0, F87_R, 0.0, F87_T);
    for &(hx, hy, d) in F87_HOLES.iter().take(n_holes) {
        let tool = cyl_engine_frame(&mut a, hx, hy, F87_HR, 0.0, d);
        body = boolean_op(&mut a, body, tool, BoolOp::Subtract)?;
        validate_solid(&a, body)?;
    }
    Ok((a, body))
}

/// Regression for the retired increment-8 wall (was: the pin
/// `f0087_cut7_stays_loud_offsurface_wall`, which asserted the TYPED
/// `VertexOffSurface` boundary until amendment 5's cavity relocation
/// landed): cut 7 — the first cut whose tool x-extreme sweep column lands
/// inside a rim-chord mint's displacement (the COLUMN HOP) — must succeed
/// with a fully valid output.
#[test]
fn f0087_cut7_column_hop_relocates() {
    let (a, s) = run_f0087_chain(7).expect("cut 7 (rim-mint column hop) regressed to an error");
    validate_solid(&a, s).expect("cut 7 succeeded but output is invalid");
}

/// Increment 8 green target: the 7-cut F0087 chain end-to-end.
#[test]
fn f0087_engine_frame_seven_hole_chain() {
    let (a, s) = run_f0087_chain(7).expect("chain");
    let mesh = tessellate(&a, s).expect("tessellate");
    let vol = mesh_signed_volume(&mesh);
    let mut analytic = std::f64::consts::PI * F87_R * F87_R * F87_T;
    for &(_, _, d) in F87_HOLES.iter() {
        analytic -= std::f64::consts::PI * F87_HR * F87_HR * d.min(F87_T);
    }
    // N=14 on the plate rim ⇒ ~1.1% polygon deficit; 3% band still rejects
    // a dropped cap / missing hole.
    assert!(
        (vol - analytic).abs() / analytic < 0.03,
        "7-hole F0087 plate volume {vol} outside band of analytic {analytic}"
    );
}

/// Regression for the retired increment-9 wall (was: the pin
/// `f0087_cut9_stays_loud_offsurface_wall`, which asserted the TYPED
/// `VertexOffSurface` boundary until amendment 6's JOINT region relocation
/// landed): cut 9 — where the plate-rim mint and a hole-rim mint interact
/// across one multi-column strip (each vertex on the OTHER's cavity
/// polygon, both per-vertex cavities exactly non-simple) — must succeed
/// with a fully valid output. The seeds' star-union region is
/// re-triangulated jointly by the shared constrained exact ear-clip.
#[test]
fn f0087_cut9_column_strip_relocates_jointly() {
    let (a, s) = run_f0087_chain(9).expect("cut 9 (interacting rim mints) regressed to an error");
    validate_solid(&a, s).expect("cut 9 succeeded but output is invalid");
}

/// Increment 9 green target: the full 10-cut F0087 chain end-to-end.
#[test]
fn f0087_engine_frame_full_ten_hole_chain() {
    let (a, s) = run_f0087_chain(10).expect("chain");
    let mesh = tessellate(&a, s).expect("tessellate");
    let vol = mesh_signed_volume(&mesh);
    let mut analytic = std::f64::consts::PI * F87_R * F87_R * F87_T;
    for &(_, _, d) in F87_HOLES.iter() {
        analytic -= std::f64::consts::PI * F87_HR * F87_HR * d.min(F87_T);
    }
    assert!(
        (vol - analytic).abs() / analytic < 0.03,
        "10-hole F0087 plate volume {vol} outside band of analytic {analytic}"
    );
}

// ── F0089 fixture (seed 30004): the increment-10 wall ─────────────────────
// Probe census (2026-07-07): cut 11 — the chain's FIRST blind hole — folds
// a strip whose rim-mint seeds sit exactly ON the intersection curve, so
// the amendment-6 joint region (13 seeds' star union) straddles the class
// boundary and its single-class guard rejects the whole region
// (`[reloc-region-reject] … multi-class region`). The amendment-2 revert
// then leaks chord-position vertices — `VertexOffSurface(FaceId 123)`.
// Increment 10's scope (spec `n2_stage4_junction_cluster_merge` §3
// amendment 7): partition the star-union BY CLASS and relocate each folded
// class sub-region independently; class-boundary edges become sub-region
// boundary by construction, so the intersection curve is preserved.

const F89_R: f64 = 1.7036011398273958;
const F89_T: f64 = 0.2563259121685504;
const F89_HR: f64 = 0.03532679244631051;
/// The first 11 of F0089's 20 holes (bit-exact corpus parameters): holes
/// 1–10 are through (depth > T), hole 11 is the first BLIND hole and the
/// measured multi-class-region wall.
const F89_HOLES: [(f64, f64, f64); 11] = [
    (-0.4415405126836073, 0.28324289523755014, 0.6233098920507019),
    (
        -0.1186870273729144,
        -0.29380724758512716,
        0.3954886846857477,
    ),
    (-0.49316876321318814, -0.28001346410600897, 0.63455954558122),
    (0.10947154595842823, -0.3304457202030231, 0.5183574005461733),
    (0.0581669605049443, -0.4654028183771426, 0.5102860724088583),
    (0.02909947211365144, 0.19418873266955566, 0.591612482380818),
    (0.9173432735859929, -0.18383118684803995, 0.6827686064194173),
    (
        0.16002074019974233,
        -0.23282417642069592,
        0.6170312196510624,
    ),
    (0.4313327877972431, 0.048280993764627425, 0.615042240625649),
    (0.3572766402935755, 0.08889730865519277, 0.6902193404958872),
    (
        -0.3858712880603707,
        -0.22348296867823247,
        0.16489426956369788,
    ),
];

fn run_f0089_chain(
    n_holes: usize,
) -> Result<(BrepArena, kernel_v2::SolidId), kernel_v2::KernelV2Error> {
    let mut a = BrepArena::new();
    let mut body = cyl_engine_frame(&mut a, 0.0, 0.0, F89_R, 0.0, F89_T);
    for &(hx, hy, d) in F89_HOLES.iter().take(n_holes) {
        let tool = cyl_engine_frame(&mut a, hx, hy, F89_HR, 0.0, d);
        body = boolean_op(&mut a, body, tool, BoolOp::Subtract)?;
        validate_solid(&a, body)?;
    }
    Ok((a, body))
}

/// Regression for the retired increment-10 wall (was: the pin
/// `f0089_cut11_stays_loud_offsurface_wall`, which asserted the TYPED
/// `VertexOffSurface` boundary until amendment 7's class-partitioned joint
/// region relocation landed): cut 11 — the chain's first BLIND hole, whose
/// rim-mint strip straddles the intersection-curve class boundary — must
/// succeed with a fully valid output. Each folded class sub-region is
/// re-triangulated independently; the class-boundary edges survive as
/// sub-region boundary.
#[test]
fn f0089_cut11_multiclass_strip_relocates_partitioned() {
    let (a, s) =
        run_f0089_chain(11).expect("cut 11 (multi-class rim-mint strip) regressed to an error");
    validate_solid(&a, s).expect("cut 11 succeeded but output is invalid");
}

/// Increment 10 green target: the 11-cut F0089 chain end-to-end.
#[test]
fn f0089_engine_frame_eleven_hole_chain() {
    let (a, s) = run_f0089_chain(11).expect("chain");
    let mesh = tessellate(&a, s).expect("tessellate");
    let vol = mesh_signed_volume(&mesh);
    let mut analytic = std::f64::consts::PI * F89_R * F89_R * F89_T;
    for &(_, _, d) in F89_HOLES.iter() {
        analytic -= std::f64::consts::PI * F89_HR * F89_HR * d.min(F89_T);
    }
    // N≈16 on the plate rim ⇒ ~0.8% polygon deficit; 3% band still rejects
    // a dropped cap / missing hole.
    assert!(
        (vol - analytic).abs() / analytic < 0.03,
        "11-hole F0089 plate volume {vol} outside band of analytic {analytic}"
    );
}

// ── F0090 fixture (seed 30005): the increment-11 wall ─────────────────────
// Probe census post-amendment-7 (2026-07-07): cut 7's second femto-strip
// (seeds [183,189,190,195,196]) passes the class partition, but the folded
// AOnly sub-region's boundary is a BOW-TIE under the minted positions —
// the strip's two long sides cross exactly ([reloc-ring] edges 0 × 4) —
// so the shared ear-clip's simplicity guard rejects
// (`class AOnly region polygon not simple`) and the amendment-2 revert
// leaks chord vertices as VertexOffSurface. Increment 11's scope (spec
// `n2_stage4_junction_cluster_merge` §3 amendment 8): grow the sub-region
// across a crossing edge's single external same-class neighbor until the
// boundary ring is exactly simple (the region form of amendment 5's
// constrained visibility growth).

const F90_R: f64 = 1.949952676445734;
const F90_T: f64 = 0.4640981207724759;
const F90_HR: f64 = 0.024226758979155424;
/// The first 7 of F0090's 30 holes (bit-exact corpus parameters, all
/// through: depth > T). Cut 7 is the measured bow-tie sub-region wall.
const F90_HOLES: [(f64, f64, f64); 7] = [
    (0.27316509614628137, 0.27144718566612946, 1.0152352850121833),
    (-0.53673080264824, -1.4341425308476503, 1.1387439874959013),
    (-1.3291095975787688, 0.2274569468070904, 1.2789422323321724),
    (-0.3971281771685192, 1.8292424007626322, 1.0460475747493216),
    (0.09378079689674315, 0.6628791953622455, 1.3057819201236136),
    (
        -0.005707830499044367,
        0.061132734513484076,
        1.3203995259758554,
    ),
    (1.5425033160829418, 0.2390890943567454, 1.292075306693227),
];

fn run_f0090_chain(
    n_holes: usize,
) -> Result<(BrepArena, kernel_v2::SolidId), kernel_v2::KernelV2Error> {
    let mut a = BrepArena::new();
    let mut body = cyl_engine_frame(&mut a, 0.0, 0.0, F90_R, 0.0, F90_T);
    for &(hx, hy, d) in F90_HOLES.iter().take(n_holes) {
        let tool = cyl_engine_frame(&mut a, hx, hy, F90_HR, 0.0, d);
        body = boolean_op(&mut a, body, tool, BoolOp::Subtract)?;
        validate_solid(&a, body)?;
    }
    Ok((a, body))
}

/// Regression for the retired increment-11 wall (was: the pin
/// `f0090_cut7_stays_loud_offsurface_wall`, which asserted the TYPED
/// `VertexOffSurface` boundary until amendment 8's region growth to
/// simplicity landed): cut 7 — whose folded sub-region boundary is a
/// bow-tie under the minted positions — must succeed with a fully valid
/// output. The sub-region grows across the crossing edge's same-class
/// neighbor until the ring is exactly simple, then ear-clips.
#[test]
fn f0090_cut7_bowtie_region_grows_to_simplicity() {
    let (a, s) = run_f0090_chain(7).expect("cut 7 (bow-tie sub-region) regressed to an error");
    validate_solid(&a, s).expect("cut 7 succeeded but output is invalid");
}

/// Increment 11 green target: the 7-cut F0090 chain end-to-end.
#[test]
fn f0090_engine_frame_seven_hole_chain() {
    let (a, s) = run_f0090_chain(7).expect("chain");
    let mesh = tessellate(&a, s).expect("tessellate");
    let vol = mesh_signed_volume(&mesh);
    let mut analytic = std::f64::consts::PI * F90_R * F90_R * F90_T;
    for &(_, _, d) in F90_HOLES.iter() {
        analytic -= std::f64::consts::PI * F90_HR * F90_HR * d.min(F90_T);
    }
    // N=14 on the plate rim ⇒ ~1.1% polygon deficit; 3% band still rejects
    // a dropped cap / missing hole.
    assert!(
        (vol - analytic).abs() / analytic < 0.03,
        "7-hole F0090 plate volume {vol} outside band of analytic {analytic}"
    );
}

// ── F0088 fixture (seed 30003): the increment-14 wall ─────────────────────
// Probe census (2026-07-07, post-amendment-10 binary): ops 14/15's
// VertexOffSurface both revert at vert 674, whose cavity polygon is a
// hair-thin full-height NET-CW BOW-TIE (the strip's long return edge
// crosses the up-chain; net 2A = −4.2e-3). The ear-clip checked
// orientation BEFORE simplicity, so the ring died as a terminal
// `cavity polygon not CCW` and never reached the joint trigger.
// Increment 14's scope (spec `n2_stage4_junction_cluster_merge` §3
// amendment 11): simplicity before orientation + the joint trigger
// accepts singleton seed sets.

const F88_R: f64 = 1.2787008340600021;
const F88_T: f64 = 0.23050816593474505;
const F88_HR: f64 = 0.042871795720997065;
/// All 15 of F0088's holes (bit-exact corpus parameters). The corpus
/// runner SKIPS a failed cut and continues on the previous body — op 4
/// dies at a Stage-3 `AmbiguousCurve` wall (a different subsystem,
/// tracked separately), so this chain mirrors that skip semantics.
const F88_HOLES: [(f64, f64, f64); 15] = [
    (0.35984134497173503, 0.6749467831423721, 0.4901131898241702),
    (-0.548323773177785, 0.3554838533489517, 0.4353355653710682),
    (
        0.17464227358983425,
        -0.24105075339392248,
        0.6561329732945973,
    ),
    (-0.4154488532002217, -1.1516293867047513, 0.5924691092311544),
    (0.31920170622438815, 0.0935413256306473, 0.5374632722273324),
    (
        -0.09556733294481233,
        0.03710031540398218,
        0.5095865515769602,
    ),
    (1.194241514087986, 0.11670234703039814, 0.6882093144428375),
    (
        -0.49847312830244883,
        -0.10087950048927558,
        0.3993679024339291,
    ),
    (
        -0.3366088617002614,
        -0.043944800192486075,
        0.15232104308175012,
    ),
    (0.06224055198227289, 0.7729276663879899, 0.11667821141075961),
    (0.6540402265145715, -0.39587922592983693, 0.0840440240772048),
    (
        -0.10971753284914543,
        -0.6315325036761213,
        0.15175473053216826,
    ),
    (
        0.41299838691941887,
        -1.0454508662036808,
        0.14413098426090762,
    ),
    (-0.6984094063469715, 0.8016496195445787, 0.12740098855159063),
    (0.4263661298949291, 1.115532507075303, 0.15186646332483356),
];

/// Regression for the retired increment-14 wall: the full 15-cut F0088
/// chain with the corpus runner's skip-on-error semantics must produce NO
/// `VertexOffSurface` (the vert-674 net-CW bow-tie is repaired by the
/// joint path), and every succeeded cut's output stays valid with the
/// volume oracle over the holes that actually cut.
#[test]
fn f0088_engine_frame_chain_no_offsurface_residue() {
    let mut a = BrepArena::new();
    let mut body = cyl_engine_frame(&mut a, 0.0, 0.0, F88_R, 0.0, F88_T);
    let mut errors: Vec<String> = Vec::new();
    let mut cut_holes: Vec<f64> = Vec::new();
    for &(hx, hy, d) in F88_HOLES.iter() {
        let tool = cyl_engine_frame(&mut a, hx, hy, F88_HR, 0.0, d);
        match boolean_op(&mut a, body, tool, BoolOp::Subtract) {
            Ok(next) => {
                validate_solid(&a, next).expect("succeeded cut output must be valid");
                body = next;
                cut_holes.push(d);
            }
            Err(e) => errors.push(format!("{e:?}")),
        }
    }
    assert!(
        !errors.iter().any(|e| e.contains("VertexOffSurface")),
        "no cut may leak chord-position vertices (the increment-14 wall): {errors:?}"
    );
    assert!(
        !errors.iter().any(|e| e.contains("AmbiguousCurve")),
        "no cut may die at a Case-IV phantom intersection (the increment-15 \
         wall): {errors:?}"
    );
    let mesh = tessellate(&a, body).expect("tessellate");
    let vol = mesh_signed_volume(&mesh);
    let mut analytic = std::f64::consts::PI * F88_R * F88_R * F88_T;
    for &d in &cut_holes {
        analytic -= std::f64::consts::PI * F88_HR * F88_HR * d.min(F88_T);
    }
    assert!(
        (vol - analytic).abs() / analytic < 0.03,
        "F0088 plate volume {vol} outside band of analytic {analytic} \
         ({} of 15 cuts succeeded)",
        cut_holes.len()
    );
}

/// Regression for the retired increment-15 wall (was: the pin
/// `f0088_cut4_stays_loud_phantom_intersection_wall`, which asserted the
/// loud Stage-3 `AmbiguousCurve { candidates: 0 }` until the Case-IV
/// phantom guard landed): hole 4's tool cylinder is ANALYTICALLY disjoint
/// from the plate's outer cylinder (internal gap = R − d_axes − r =
/// 0.0115) but at the natural N=14 the plate's chord facets dipped inward
/// past the gap, so the MESHES intersected where the surfaces do not —
/// Yang Fig-8 Case IV. The guard (spec `yang_case_iv_phantom_guard`) now
/// rebuilds both operands at the pair-derived rim density, the phantom
/// never reaches the arrangement, and cut 4 succeeds with a valid output
/// — the thin wall between hole 4 and the plate rim SURVIVES.
#[test]
fn f0088_cut4_phantom_intersection_filtered() {
    let mut a = BrepArena::new();
    let mut body = cyl_engine_frame(&mut a, 0.0, 0.0, F88_R, 0.0, F88_T);
    for &(hx, hy, d) in F88_HOLES.iter().take(4) {
        let tool = cyl_engine_frame(&mut a, hx, hy, F88_HR, 0.0, d);
        body = boolean_op(&mut a, body, tool, BoolOp::Subtract)
            .expect("cut 4 (Case-IV phantom pair) regressed to an error");
    }
    validate_solid(&a, body).expect("cut 4 succeeded but output is invalid");
    // The thin wall survives: volume equals the 4-hole analytic value (a
    // phantom notch through the rim wall would siphon volume).
    let mesh = tessellate(&a, body).expect("tessellate");
    let vol = mesh_signed_volume(&mesh);
    let mut analytic = std::f64::consts::PI * F88_R * F88_R * F88_T;
    for &(_, _, d) in F88_HOLES.iter().take(4) {
        analytic -= std::f64::consts::PI * F88_HR * F88_HR * d.min(F88_T);
    }
    assert!(
        (vol - analytic).abs() / analytic < 0.03,
        "4-hole F0088 plate volume {vol} outside band of analytic {analytic}"
    );
}
