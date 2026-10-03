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
//! The second test is the cross-process half of the stability claim. The
//! in-process oracle (`d0_pid_identity.rs`) rebuilds in two fresh arenas,
//! which cannot see a dependence on anything global; these literals were
//! recorded by a different process than the one running them now, so a
//! derivation that read an address, a clock or a hash seed could not
//! reproduce them.

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::pid::solid_pids;
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
