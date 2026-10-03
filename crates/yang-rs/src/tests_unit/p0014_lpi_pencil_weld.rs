//! P0014 — the arrangement LPI-pencil weld (spec
//! `specs/yang_p0014_arrangement_lpi_pencil_weld.md`; `boolean::weld_lpi_pencils`).
//!
//! Pinned on P0014's own measurement (`convex5:boss gear10:rev-cut`,
//! 2026-10-03, release, `CHERCHI_VERT_PROVENANCE=1e-12` +
//! `YANG_PENCIL_WELD_PROBE=1`): a gear tessellation vertex (`B#8153`) lies 2
//! ULP off the boss plane `[A#1,A#0,A#7]`, so the exact arrangement minted one
//! LPI per incident mesh edge — `line[B#8119→B#8153]`,
//! `line[B#8187→B#8153]`, `line[B#8153→B#8188]`, `line[B#8153→B#8154]`,
//! `line[B#8153→B#8120]` — plus `EXPLICIT B#8153` itself. Six output vertices
//! at one geometric point. Left distinct they make every triangle using two of
//! them a needle and the §4.4.1(a) unzip STOPs (`degenerate_no_longedge`,
//! P0014's ERROR).
//!
//! The two positions below are the surviving pair as the Stage-4 twin scan
//! printed them (`[twin-scan] edge (139,141) len=2.730e-13 … moved=(false,false)`):
//! which member carries the EXPLICIT generator is not something the twin scan
//! records, so these tests assign that role themselves — they pin the weld
//! HELPER's band decision and survivor rule at P0014's measured separation and
//! coordinate scale, not the case's index assignment.

use cad_primitives::Point3;

use crate::boolean::weld_lpi_pencils;

/// `[twin-scan] edge (139,141) len=2.730e-13`, verbatim coordinates.
const TWIN_A: [f64; 3] = [-650.0668987467757, 251.10250748699403, 94.65903389988972];
const TWIN_B: [f64; 3] = [-650.0668987467757, 251.1025074869942, 94.65903389988951];

fn dist(p: [f64; 3], q: [f64; 3]) -> f64 {
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

#[test]
fn the_measured_twin_is_two_ulp_and_inside_the_unchanged_band() {
    let d = dist(TWIN_A, TWIN_B);
    assert!(
        (d - 2.729816e-13).abs() < 1e-19,
        "the twin scan printed len=2.730e-13, these coordinates give {d:.6e}"
    );
    let scale = TWIN_A
        .iter()
        .chain(TWIN_B.iter())
        .fold(0.0f64, |m, &c| m.max(c.abs()));
    let band = cad_primitives::TAU_WORK * (1.0 + scale);
    assert!(
        (band - 6.510669e-10).abs() < 1e-16,
        "the KV10 band at scale {scale} is 6.510669e-10, got {band:.6e}"
    );
    // ≈ 2 ULP: the pair has no distinct f64 image at this coordinate scale,
    // which is WHY every triangle using both is a needle.
    assert!(
        d / scale < 1e-15,
        "relative separation {:.4e} must be ULP-order",
        d / scale
    );
    assert!(d < band, "and therefore inside the coincidence band");
}

#[test]
fn pencil_in_band_welds_to_the_explicit_vertex() {
    // v0/v1 = two of the pencil's LPIs (v1 bit-identical to v0 — the
    // bit-exact weld would already have fused such a member; P0014 measured
    // one, `out(143,148) d=0.000e0`), v2 = the EXPLICIT operand vertex. The
    // explicit vertex is deliberately the HIGHEST index: P0014's own cluster
    // is shaped that way (the LPIs are out 143…148, the explicit out 8296),
    // so a min-index survivor rule would keep an LPI's coordinates and lose
    // the operand's own point.
    let verts = vec![
        Point3::new(TWIN_A[0], TWIN_A[1], TWIN_A[2]),
        Point3::new(TWIN_A[0], TWIN_A[1], TWIN_A[2]),
        Point3::new(TWIN_B[0], TWIN_B[1], TWIN_B[2]),
    ];
    // The producer's record: both LPIs' generating lines end at vertex 2.
    let record = [(0u32, 2u32), (1, 2)];
    let mut weld: Vec<u32> = vec![0, 1, 2];
    let welded = weld_lpi_pencils(&verts, &record, &mut weld);
    assert_eq!(welded, 2, "both pencil members are in band");
    assert_eq!(
        weld,
        vec![2, 2, 2],
        "survivor is the EXPLICIT operand vertex, not the minimum index"
    );
}

#[test]
fn a_pencil_member_outside_the_band_is_left_alone() {
    // A real transversal pierce: 1e-6 from its line's endpoint — six orders
    // above the band, i.e. a genuine model feature (MIN_FEATURE_SIZE).
    let verts = vec![
        Point3::new(TWIN_B[0], TWIN_B[1], TWIN_B[2]),
        Point3::new(TWIN_B[0] + 1e-6, TWIN_B[1], TWIN_B[2]),
    ];
    let mut weld: Vec<u32> = vec![0, 1];
    let welded = weld_lpi_pencils(&verts, &[(1, 0)], &mut weld);
    assert_eq!(welded, 0, "an out-of-band pierce must NOT weld");
    assert_eq!(weld, vec![0, 1], "weld array untouched");
}

#[test]
fn an_empty_record_is_a_no_op() {
    let verts = vec![
        Point3::new(TWIN_B[0], TWIN_B[1], TWIN_B[2]),
        Point3::new(TWIN_A[0], TWIN_A[1], TWIN_A[2]),
    ];
    let mut weld: Vec<u32> = vec![0, 1];
    // A producer that does not track generator provenance (the sidecar parity
    // oracle, hand-built fixtures) — byte-identical behaviour.
    assert_eq!(weld_lpi_pencils(&verts, &[], &mut weld), 0);
    assert_eq!(weld, vec![0, 1]);
}

#[test]
fn the_weld_array_stays_flat_through_a_chain() {
    // The one shape that can CHAIN, assembled from two measured layers:
    //   v0, v1 = pencil LPIs; v2, v3 = explicit operand vertices in the band.
    //   The bit-exact weld has already fused explicit v2 into LPI v1 (P0014
    //   measured exactly that: `out(147,8296) d=0.000e0`), so the root `1` is
    //   simultaneously an LPI's root and an explicit's root.
    // Welding v0 onto v2 points root 0 at root 1; welding v1 onto v3 then
    // points root 1 at 3 — leaving entry 0 two hops from its representative.
    // The compaction that follows reads `weld[i]` ONCE, so the pass must
    // re-flatten.
    let verts = vec![
        Point3::new(TWIN_A[0], TWIN_A[1], TWIN_A[2]),
        Point3::new(TWIN_B[0], TWIN_B[1], TWIN_B[2]),
        Point3::new(TWIN_B[0], TWIN_B[1], TWIN_B[2]),
        Point3::new(TWIN_B[0], TWIN_B[1], TWIN_B[2]),
    ];
    let mut weld: Vec<u32> = vec![0, 1, 1, 3]; // 2 already welded into 1
    let welded = weld_lpi_pencils(&verts, &[(0, 2), (1, 3)], &mut weld);
    assert_eq!(welded, 2);
    assert_eq!(weld, vec![3, 3, 3, 3], "flat: no entry needs a second hop");
    for (i, &w) in weld.iter().enumerate() {
        assert_eq!(
            weld[w as usize], w,
            "entry {i} points at {w}, which must itself be a root"
        );
    }
}
