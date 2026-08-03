//! Read-only loop-simplicity scan for Stage-6 emitted loops
//! (`YANG_S6_LOOP_SIMPLICITY`) — the census instrument for the
//! planar-and-self-intersecting emitted-loop class.
//!
//! # Why this exists
//!
//! Stage 6 validates each emitted planar loop VERTEX-wise: every vertex must
//! lie on the inherited plane (`s6-planar-loop-nonplanar`) and the cycle must
//! carry enough Newell area (`s6-planar-degenerate-loop`). Both gates pass on a
//! loop that crosses ITSELF — simplicity is a property of the whole cycle, not
//! of any vertex or of its total area. So a self-intersecting loop leaves the
//! producer clean and is first refused one crate away, by kernel-v2's exact
//! CDT, as `TessellationFailed` ("ring rejected by CDT"). That distance between
//! the defect and its wall is the structural hole this scan measures.
//!
//! Anchored instance: F0067 (commit 922a9892). Stage 0 mints flush-overlay
//! crossing vertices on the tessellated cylinder's 13-gon CHORDS; Stage 4 then
//! refines the six carrying an A×B `Circle` key onto the exact circle (up to
//! 3.70e-3) while their loop neighbours — the other operand's own profile
//! corners, not relocation candidates — correctly do not move. The notch-bottom
//! segment is 6.4e-4 long, so a 3.7e-3 per-vertex refinement is **5.8× the
//! segment it belongs to** and cannot stay on its own side of the outline. The
//! emitted 32-point loop has 4 self-intersections.
//!
//! # What it is NOT
//!
//! Not a gate. This module never returns an error and never mutates the mesh:
//! it is a measurement, and the anchor is explicit that a producer-side
//! loop-simplicity STOP would reword ~8 ERRORs and repair nothing (a P10 net
//! only — see `feedback_stop_band_tuning_build_mesh_updating`). The repair is
//! Yang §4.5.2 loop-coherent local refinement under epic #169; this scan exists
//! to scope it, by answering the two questions the anchor left open: which of
//! the tail ERRORs actually share the mechanism, and — the decisive column —
//! whether any SUPPORTED_CORRECT case emits a self-intersecting loop that
//! survives anyway.
//!
//! # Exactness
//!
//! The contact classification is EXACT. Projection to 2D drops the coordinate
//! of the plane normal's dominant axis, which copies the surviving f64
//! coordinates verbatim (no arithmetic, so no rounding), and the orientation
//! and on-segment tests run over `dashu` rationals — the same exact backend
//! Stage 0's overlay engine uses. A near-miss and a true crossing are therefore
//! distinguished by the predicate, not by a band. Segment LENGTHS are reported
//! in f64 (they are context for the reader, not part of any decision).

use crate::coplanar_overlay::{between_box, cross_r, rat, ExactPoint2};

/// Exact contact classification of one pair of loop segments.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Contact {
    /// Closed segments are disjoint.
    None,
    /// Closed segments meet, but not transversally: a shared point (pinch) or
    /// a collinear overlap. Refused by an exact CDT just as a crossing is, but
    /// a distinct mechanism — worth its own column.
    Touch,
    /// Proper transversal crossing: each segment strictly separates the other's
    /// endpoints.
    Cross,
}

/// The measured non-simplicity of one emitted loop.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LoopSimplicity {
    /// Proper transversal crossings between NON-adjacent segment pairs.
    pub(crate) crossings: usize,
    /// Non-transversal contacts (pinch / collinear overlap) between
    /// NON-adjacent segment pairs.
    pub(crate) touches: usize,
    /// ADJACENT segment pairs that meet in more than their shared vertex — a
    /// backtrack or collinear overlap at a corner.
    pub(crate) spikes: usize,
    /// Segments whose two endpoints are bit-identical. Excluded from the
    /// pairwise scan (they would report a contact against everything they
    /// touch), counted here instead.
    pub(crate) degenerate_segments: usize,
    /// Shortest non-degenerate segment (3D, f64). `f64::INFINITY` if none.
    pub(crate) min_seg: f64,
    /// Longest segment (3D, f64). `0.0` if none.
    pub(crate) max_seg: f64,
    /// Segment indices of the first crossing found, in scan order — the
    /// starting point for a per-case investigation.
    pub(crate) first_crossing: Option<(usize, usize)>,
}

impl LoopSimplicity {
    /// A loop an exact CDT can accept: no self-contact of any kind.
    pub(crate) fn is_simple(&self) -> bool {
        self.crossings == 0
            && self.touches == 0
            && self.spikes == 0
            && self.degenerate_segments == 0
    }
}

/// Exact orientation sign of `(a, b, c)`: `+1` CCW, `-1` CW, `0` collinear.
fn orient(a: &ExactPoint2, b: &ExactPoint2, c: &ExactPoint2) -> i8 {
    let v = cross_r(a, b, c);
    match v.cmp(&dashu::rational::RBig::ZERO) {
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
    }
}

/// Exact contact of closed segments `[a, b]` and `[c, d]`.
///
/// The four-orientation test is the textbook one; the point is that every
/// orientation here is EXACT, so `Touch` means the segments genuinely share a
/// point rather than passing within some tolerance of one another.
fn seg_contact(a: &ExactPoint2, b: &ExactPoint2, c: &ExactPoint2, d: &ExactPoint2) -> Contact {
    let d1 = orient(a, b, c);
    let d2 = orient(a, b, d);
    let d3 = orient(c, d, a);
    let d4 = orient(c, d, b);
    if d1 * d2 < 0 && d3 * d4 < 0 {
        return Contact::Cross;
    }
    if (d1 == 0 && between_box(a, b, c))
        || (d2 == 0 && between_box(a, b, d))
        || (d3 == 0 && between_box(c, d, a))
        || (d4 == 0 && between_box(c, d, b))
    {
        return Contact::Touch;
    }
    Contact::None
}

/// Do two ADJACENT segments meet in more than their shared vertex?
///
/// Adjacent segments always share an endpoint, so [`seg_contact`] reports
/// `Touch` for every corner of every loop — useless as a defect signal. What
/// IS a defect is a corner where one segment's FAR endpoint lands on the other
/// segment: the loop doubles back along itself (F0067's minted vertex sat on a
/// neighbouring edge's supporting line at parameter t = −0.606, i.e. off that
/// edge, which is exactly the doubling-back seen from the other side).
fn adjacent_spike(a: &ExactPoint2, b: &ExactPoint2, c: &ExactPoint2, d: &ExactPoint2) -> bool {
    // The far endpoints are the two that are not the shared vertex. Compare by
    // exact value, not by index: a loop may repeat a position at two indices,
    // and in that case both are genuinely coincident.
    let far_of_cd = if a == c || a == d { b } else { a };
    let far_of_ab = if c == a || c == b { d } else { c };
    (orient(a, b, far_of_ab) == 0 && between_box(a, b, far_of_ab))
        || (orient(c, d, far_of_cd) == 0 && between_box(c, d, far_of_cd))
}

/// Scan one closed loop of 3D points lying on a plane with the given `normal`.
///
/// Returns `None` when the loop cannot be scanned at all: fewer than 3 points,
/// a non-finite coordinate, or a degenerate (zero / non-finite) normal. `None`
/// is "not measured", NEVER "simple" — the caller must report it as such.
pub(crate) fn scan_cycle(pts: &[[f64; 3]], normal: [f64; 3]) -> Option<LoopSimplicity> {
    let m = pts.len();
    if m < 3 {
        return None;
    }
    // Drop the dominant-axis coordinate: the projection is injective on any
    // plane whose normal has that axis as its largest component, and it copies
    // the two surviving f64 coordinates verbatim — an exact operation.
    let mut drop_axis = 0usize;
    for i in 1..3 {
        if normal[i].abs() > normal[drop_axis].abs() {
            drop_axis = i;
        }
    }
    if !normal[drop_axis].is_finite() || normal[drop_axis] == 0.0 {
        return None;
    }
    let (ax, ay) = match drop_axis {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    let mut p2: Vec<ExactPoint2> = Vec::with_capacity(m);
    for p in pts {
        p2.push(ExactPoint2 {
            x: rat(p[ax]).ok()?,
            y: rat(p[ay]).ok()?,
        });
    }

    let mut r = LoopSimplicity {
        min_seg: f64::INFINITY,
        ..LoopSimplicity::default()
    };
    // Segment `i` spans vertices `i` → `(i + 1) % m`.
    let mut alive: Vec<bool> = Vec::with_capacity(m);
    for i in 0..m {
        let j = (i + 1) % m;
        if pts[i] == pts[j] {
            r.degenerate_segments += 1;
            alive.push(false);
            continue;
        }
        alive.push(true);
        let len = ((pts[j][0] - pts[i][0]).powi(2)
            + (pts[j][1] - pts[i][1]).powi(2)
            + (pts[j][2] - pts[i][2]).powi(2))
        .sqrt();
        r.min_seg = r.min_seg.min(len);
        r.max_seg = r.max_seg.max(len);
    }

    for i in 0..m {
        if !alive[i] {
            continue;
        }
        for j in (i + 1)..m {
            if !alive[j] {
                continue;
            }
            let (a, b) = (&p2[i], &p2[(i + 1) % m]);
            let (c, d) = (&p2[j], &p2[(j + 1) % m]);
            // Adjacency is by SEGMENT INDEX in the cycle, including the wrap
            // between the last segment and the first.
            if j == i + 1 || (i == 0 && j == m - 1) {
                if adjacent_spike(a, b, c, d) {
                    r.spikes += 1;
                }
                continue;
            }
            match seg_contact(a, b, c, d) {
                Contact::Cross => {
                    r.crossings += 1;
                    if r.first_crossing.is_none() {
                        r.first_crossing = Some((i, j));
                    }
                }
                Contact::Touch => r.touches += 1,
                Contact::None => {}
            }
        }
    }
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    const Z: [f64; 3] = [0.0, 0.0, 1.0];

    fn xy(v: &[(f64, f64)]) -> Vec<[f64; 3]> {
        v.iter().map(|&(x, y)| [x, y, 0.0]).collect()
    }

    #[test]
    fn convex_square_is_simple() {
        let s = scan_cycle(&xy(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]), Z).unwrap();
        assert!(s.is_simple(), "{s:?}");
        assert_eq!(s.crossings, 0);
        assert!((s.min_seg - 1.0).abs() < 1e-15);
        assert!((s.max_seg - 1.0).abs() < 1e-15);
    }

    #[test]
    fn nonconvex_l_shape_is_simple() {
        // A reflex corner is not a self-intersection.
        let s = scan_cycle(
            &xy(&[
                (0.0, 0.0),
                (2.0, 0.0),
                (2.0, 1.0),
                (1.0, 1.0),
                (1.0, 2.0),
                (0.0, 2.0),
            ]),
            Z,
        )
        .unwrap();
        assert!(s.is_simple(), "{s:?}");
    }

    #[test]
    fn figure_eight_has_one_crossing() {
        let s = scan_cycle(&xy(&[(0.0, 0.0), (1.0, 1.0), (1.0, 0.0), (0.0, 1.0)]), Z).unwrap();
        assert_eq!(s.crossings, 1);
        assert!(!s.is_simple());
        assert_eq!(s.first_crossing, Some((0, 2)));
    }

    #[test]
    fn pinch_at_shared_vertex_is_a_touch_not_a_crossing() {
        // Two triangles meeting at one repeated position: the closed segments
        // share a point without either separating the other's endpoints.
        let s = scan_cycle(
            &xy(&[
                (0.0, 0.0),
                (1.0, 1.0),
                (2.0, 0.0),
                (1.0, 1.0),
                (2.0, 2.0),
                (0.0, 2.0),
            ]),
            Z,
        )
        .unwrap();
        assert_eq!(s.crossings, 0);
        assert!(s.touches > 0, "{s:?}");
        assert!(!s.is_simple());
    }

    #[test]
    fn backtracking_corner_is_a_spike() {
        // The far endpoint of the second segment lands back ON the first.
        let s = scan_cycle(&xy(&[(0.0, 0.0), (2.0, 0.0), (1.0, 0.0), (1.0, 2.0)]), Z).unwrap();
        assert!(s.spikes > 0, "{s:?}");
        assert!(!s.is_simple());
    }

    #[test]
    fn repeated_vertex_is_a_degenerate_segment() {
        let s = scan_cycle(
            &xy(&[(0.0, 0.0), (1.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]),
            Z,
        )
        .unwrap();
        assert_eq!(s.degenerate_segments, 1);
        assert_eq!(s.crossings, 0);
        assert!(!s.is_simple());
        // The zero-length segment must not pollute the length statistics.
        assert!(s.min_seg > 0.0);
    }

    #[test]
    fn near_miss_below_tau_work_is_not_a_crossing() {
        // The exact predicate is what separates these two cases, not a band:
        // the notch tip stops 1e-13 short of the opposite edge.
        let miss = scan_cycle(
            &xy(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.5, 1e-13), (0.0, 1.0)]),
            Z,
        )
        .unwrap();
        assert!(miss.is_simple(), "{miss:?}");
        // ... and 1e-13 past it IS one, with no tolerance to argue about.
        let hit = scan_cycle(
            &xy(&[
                (0.0, 0.0),
                (1.0, 0.0),
                (1.0, 1.0),
                (0.5, -1e-13),
                (0.0, 1.0),
            ]),
            Z,
        )
        .unwrap();
        assert_eq!(hit.crossings, 2, "{hit:?}");
    }

    #[test]
    fn scan_is_projection_axis_invariant() {
        // Same figure-eight in the three axis planes: the drop-axis choice
        // must not change the verdict.
        let f8 = [(0.0, 0.0), (1.0, 1.0), (1.0, 0.0), (0.0, 1.0)];
        let xy_pts: Vec<[f64; 3]> = f8.iter().map(|&(u, v)| [u, v, 3.0]).collect();
        let yz_pts: Vec<[f64; 3]> = f8.iter().map(|&(u, v)| [3.0, u, v]).collect();
        let zx_pts: Vec<[f64; 3]> = f8.iter().map(|&(u, v)| [v, 3.0, u]).collect();
        assert_eq!(scan_cycle(&xy_pts, [0.0, 0.0, 1.0]).unwrap().crossings, 1);
        assert_eq!(scan_cycle(&yz_pts, [1.0, 0.0, 0.0]).unwrap().crossings, 1);
        assert_eq!(scan_cycle(&zx_pts, [0.0, 1.0, 0.0]).unwrap().crossings, 1);
    }

    #[test]
    fn unmeasurable_inputs_return_none_not_simple() {
        assert!(scan_cycle(&xy(&[(0.0, 0.0), (1.0, 0.0)]), Z).is_none());
        assert!(scan_cycle(&xy(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]), [0.0; 3]).is_none());
        let nan = vec![[f64::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        assert!(scan_cycle(&nan, Z).is_none());
    }

    #[test]
    fn f0067_shaped_loop_crosses_when_refinement_exceeds_the_segment() {
        // The anchored mechanism in miniature: a notch whose bottom segment is
        // 6.4e-4 long, one endpoint of which is refined 3.7e-3 sideways (5.8x
        // the segment) while its immovable neighbours stay put. The refined
        // vertex lands PAST the notch's opposite wall, so the wall that used to
        // bound the notch now separates the two ends of the bottom segment.
        let seg = 6.4e-4;
        let push = 3.7e-3;
        let simple = scan_cycle(
            &xy(&[
                (0.0, 0.0),
                (1.0, 0.0),
                (1.0, 1.0),
                (0.4 + seg, 1.0),
                (0.4 + seg, 0.5),
                (0.4, 0.5),
                (0.4, 1.0),
                (0.0, 1.0),
            ]),
            Z,
        )
        .unwrap();
        assert!(simple.is_simple(), "{simple:?}");
        let refined = scan_cycle(
            &xy(&[
                (0.0, 0.0),
                (1.0, 0.0),
                (1.0, 1.0),
                (0.4 + seg, 1.0),
                (0.4 + seg, 0.5),
                (0.4 + push, 0.5),
                (0.4, 1.0),
                (0.0, 1.0),
            ]),
            Z,
        )
        .unwrap();
        assert!(
            !refined.is_simple(),
            "a per-vertex move 5.8x the local segment must leave the outline: {refined:?}"
        );
    }
}
