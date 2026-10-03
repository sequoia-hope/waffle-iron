//! D0 — the persistent-id hash is FORMAT, and the derivation reads nothing
//! process-dependent.
//!
//! Persistent ids are written into `.waffle` documents, so `pid.rs`'s digest
//! must never drift: a changed hash silently detaches every stored drawing
//! dimension and PMI anchor from the geometry it named. The literals below
//! are the oracle. If a change to `mix`/`digest` turns this test red, that is
//! not a test to update — it is a format break, and it needs a reader-floor
//! bump and a migration, not new constants.
//!
//! Both tests are the cross-process half of the stability claim. The
//! in-process oracles (`d0_pid_identity.rs`, `d0_face_seed.rs`) rebuild in
//! two fresh arenas, which cannot see a dependence on anything global; these
//! literals were recorded by a different process than the one running them
//! now, so a derivation that read an address, a clock or a hash seed could
//! not reproduce them.
//!
//! The EDGE and VERTEX literals are read off an unseeded arena, where face
//! pids still come from the monotonic counter — so they pin the edge/vertex
//! derivation alone and the D0 item 1 face reseed left them untouched. The
//! face literals are their own oracle.

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::pid::{seeded_face_pid, solid_pids};
use kernel_v2::{extrude, BrepArena, Profile};

/// The unit box of `d0_pid_identity.rs`: a 1×1 rectangle in z = 0 extruded
/// 2 along +z.
fn unit_box() -> (BrepArena, kernel_v2::SolidId) {
    let mut arena = BrepArena::new();
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ],
        vec![],
    )
    .expect("rectangle profile");
    let r = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), 2.0).expect("box");
    (arena, r.solid)
}

#[test]
fn a_box_derives_the_same_ids_this_process_as_the_one_that_recorded_them() {
    let (arena, solid) = unit_box();
    let pids = solid_pids(&arena, solid).expect("solid pids");

    let mut edges: Vec<u64> = pids.edges.values().map(|p| p.0).collect();
    edges.sort_unstable();
    let mut vertices: Vec<u64> = pids.vertices.values().map(|p| p.0).collect();
    vertices.sort_unstable();

    assert_eq!(edges, EXPECTED_EDGE_PIDS, "edge pid set drifted");
    assert_eq!(vertices, EXPECTED_VERTEX_PIDS, "vertex pid set drifted");
}

/// Recorded 2026-10-03 from `cargo test --release -p kernel-v2`. Ascending,
/// so the list is a set and does not pin the arena's slot numbering — only
/// the hash and the lineage roots it is seeded from.
const EXPECTED_EDGE_PIDS: [u64; 12] = [
    808_968_233_048_133_449,
    2_356_266_680_657_352_218,
    3_668_948_606_742_487_223,
    6_275_874_052_999_909_787,
    7_220_985_288_818_007_052,
    7_318_230_561_285_814_375,
    9_452_631_361_628_957_050,
    12_106_482_882_365_004_596,
    14_356_826_206_885_826_291,
    14_607_965_729_928_553_243,
    16_851_107_983_592_635_099,
    16_927_261_858_278_994_057,
];

/// The same obligation for the content-seeded FACE pid (D0 item 1): the
/// literals below were recorded by a different process than the one
/// asserting them, so nothing in `seeded_face_pid` can be reading an
/// address, a clock or a hash seed. A document stores these, so a red
/// result here is a format break needing a reader-floor bump and a
/// migration — not new constants.
#[test]
fn a_seeded_face_pid_is_the_same_this_process_as_the_one_that_recorded_it() {
    let seed = kernel_v2::FaceSeed {
        origin: [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210],
    };
    let ids: Vec<u64> = (0..4)
        .flat_map(|output| (0..3).map(move |role| seeded_face_pid(seed, output, role).0))
        .collect();
    assert_eq!(ids, EXPECTED_SEEDED_FACE_PIDS, "face pid digest drifted");
    assert!(
        ids.iter().all(|&p| p >= kernel_v2::PID_CONTENT_BASE),
        "a content-seeded face pid must stay in the top half of the space"
    );
}

/// Recorded 2026-10-03 from `cargo test --release -p kernel-v2`, for seed
/// `[0x0123456789ABCDEF, 0xFEDCBA9876543210]` over `output` 0..4 × `role`
/// 0..3, in that nesting order.
const EXPECTED_SEEDED_FACE_PIDS: [u64; 12] = [
    18_246_737_610_537_624_274,
    10_923_352_044_180_466_657,
    13_569_324_608_624_469_252,
    11_279_279_743_425_471_568,
    13_654_045_101_264_376_481,
    10_582_656_118_332_216_890,
    15_226_835_435_100_552_174,
    12_323_707_673_393_059_229,
    9_595_291_910_213_796_056,
    15_265_343_453_234_432_217,
    13_578_505_666_377_178_222,
    14_654_715_188_200_795_029,
];

const EXPECTED_VERTEX_PIDS: [u64; 8] = [
    1_427_393_341_576_800_899,
    4_943_897_623_188_628_128,
    5_720_032_546_199_829_832,
    8_779_471_315_052_990_283,
    9_948_616_864_904_158_981,
    11_522_018_534_923_925_870,
    15_353_522_090_431_697_688,
    18_212_793_929_646_248_183,
];
