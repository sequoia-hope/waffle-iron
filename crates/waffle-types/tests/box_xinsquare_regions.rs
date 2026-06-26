//! Regression: X-in-square region extraction (origin: box.waffle, BOXPATCH).
//!
//! Two nested squares plus two interior diagonals crossing at the origin. A real
//! (non-construction) interior line MUST split the face, so clicking a quadrant
//! selects a triangle — independent of any construction flag. This locks in that
//! `compute_regions`, fed the SOLVED sketch coordinates, produces a correct
//! planar arrangement: every region loop is a simple polygon (no repeated
//! vertices, no spurious holes), the four inner quadrants are triangles, and the
//! areas tile the outer square exactly.
//!
//! Context: the user-visible defect was upstream in the UI (the sketch sent RAW
//! drawn coordinates instead of `solved_positions`); the arrangement itself was
//! always correct. This test guards the arrangement against future regression.

use std::collections::HashMap;

use waffle_types::regions::{compute_regions, Region, DEFAULT_CHORD_TOLERANCE};
use waffle_types::sketch::SketchEntity;

fn line(id: u32, a: u32, b: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id: a,
        end_id: b,
        construction: false,
    }
}

/// box.waffle first sketch, using the stored `solved_positions`.
fn box_sketch() -> (Vec<SketchEntity>, HashMap<u32, (f64, f64)>) {
    let positions: HashMap<u32, (f64, f64)> = [
        // outer square (lines 5,6,7,8)
        (1u32, (-0.025, -0.025)),
        (2, (0.025, -0.025)),
        (3, (0.025, 0.025)),
        (4, (-0.025, 0.025)),
        // inner square (lines 15,16,17,18)
        (11, (0.02, 0.02)),
        (12, (-0.02, 0.02)),
        (13, (-0.02, -0.02)),
        (14, (0.02, -0.02)),
    ]
    .into_iter()
    .collect();

    let entities = vec![
        // outer square
        line(5, 1, 2),
        line(6, 2, 3),
        line(7, 3, 4),
        line(8, 4, 1),
        // diagonal 9: outer-TR(3) -> outer-BL(1), through inner corners 11 & 13
        line(9, 3, 1),
        // inner square
        line(15, 11, 12),
        line(16, 12, 13),
        line(17, 13, 14),
        line(18, 14, 11),
        // diagonal 19: inner-BR(14) -> inner-TL(12), through origin
        line(19, 14, 12),
    ];
    (entities, positions)
}

/// A loop must never have two equal consecutive vertices (including the implicit
/// closing edge) — that is exactly what the kernel rejects as
/// `ProfileRepeatedVertex`.
fn loop_is_simple(loop_pts: &[(f64, f64)]) -> bool {
    let n = loop_pts.len();
    n >= 3 && (0..n).all(|i| loop_pts[i] != loop_pts[(i + 1) % n])
}

fn point_in_poly(p: (f64, f64), poly: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > p.1) != (yj > p.1) && p.0 < (xj - xi) * (p.1 - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn region_contains(r: &Region, p: (f64, f64)) -> bool {
    point_in_poly(p, &r.outer) && r.holes.iter().all(|h| !point_in_poly(p, h))
}

#[test]
fn x_in_square_subdivides_into_simple_regions() {
    let (entities, positions) = box_sketch();
    let regions = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE);

    // Four inner triangles + two frame pieces (the outer band split by diagonal 9
    // entering at the TR/BL outer corners).
    assert_eq!(regions.len(), 6, "X-in-square arrangement = 4 triangles + 2 frame pieces");

    // Every region loop is a simple polygon with no spurious holes — no
    // `ProfileRepeatedVertex`, nothing the kernel would reject.
    for (i, r) in regions.iter().enumerate() {
        assert!(loop_is_simple(&r.outer), "region {i} outer loop must be simple: {:?}", r.outer);
        assert!(r.holes.is_empty(), "region {i} must have no holes (X-in-square is hole-free)");
        for h in &r.holes {
            assert!(loop_is_simple(h), "region {i} hole loop must be simple");
        }
    }

    // Areas tile the outer square exactly (0.05 * 0.05).
    let total: f64 = regions.iter().map(|r| r.area).sum();
    assert!((total - 0.0025).abs() < 1e-9, "region areas must tile the outer square, got {total}");

    // Exactly four triangular inner quadrants, each an eighth of the inner square
    // (inner square 0.04² = 0.0016; each diagonal quadrant = 0.0004).
    let triangles: Vec<&Region> = regions.iter().filter(|r| r.outer.len() == 3).collect();
    assert_eq!(triangles.len(), 4, "the inner square's two diagonals yield 4 triangles");
    for t in &triangles {
        assert!((t.area - 0.0004).abs() < 1e-9, "inner triangle area = 0.0004, got {}", t.area);
    }

    // Clicking inside one inner quadrant (between origin and the inner-right edge)
    // selects exactly one region and it is a triangle — the diagonals subdivide
    // the face as a real CAD sketch must.
    let probe = (0.005, 0.0);
    let containing: Vec<&Region> = regions.iter().filter(|r| region_contains(r, probe)).collect();
    assert_eq!(containing.len(), 1, "the quadrant click is unambiguous");
    assert_eq!(containing[0].outer.len(), 3, "the selected quadrant is a triangle");
}
