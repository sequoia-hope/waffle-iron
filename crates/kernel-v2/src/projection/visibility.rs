//! Hidden-line classification — **D1c** of `specs/drawings_and_mbd.md`
//! (§5.2 increment 3).
//!
//! D1a projected the edges and D1b added the silhouettes, both tagged
//! `Visible`, which is a wireframe. A drawing is not a wireframe: the lines
//! the solid's own material stands in front of are dashed, or absent. This
//! module decides which.
//!
//! ## Split, then classify, then merge
//!
//! Visibility is piecewise constant along a projected curve, and it can only
//! change where the curve crosses another projected curve in `(u, v)` or where
//! its own projection FOLDS. So:
//!
//! 1. **Split.** Every curve is cut at its crossings with every other curve
//!    ([`super::crossings`], exact for the analytic vocabulary, chord-banded
//!    for polylines) and at its own folds. A piece keeps its parent's KIND:
//!    half a projected rim is still a [`Curve2::Circle`].
//! 2. **Classify.** Each piece is classified at its parameter MIDPOINT, by
//!    casting a ray from there toward the viewer and asking whether any face
//!    of the solid stands in front (see "the ray" below).
//! 3. **Merge.** Adjacent pieces of one parent that agree are rejoined into
//!    one sub-curve, so a box seen from a generic direction reports its twelve
//!    edges as twelve curves — nine visible, three hidden — and not as the
//!    twenty-odd fragments the crossing search cut them into. Then curves that
//!    are COINCIDENT in `(u, v)` and agree on visibility are deduplicated,
//!    which is the other half of §5.2's "segments coincident in (u, v) with
//!    the same visibility are merged".
//!
//! ## The ray, and the one band it carries
//!
//! A piece's midpoint is a 2-D point; what the ray needs is the 3-D point it
//! came from. Each projected curve therefore travels with the **3-D sample
//! polyline of its own source** at the same density the projection used
//! ([`LiftedCurve::lift`]) — an edge's [`crate::introspect::edge_polyline`],
//! a silhouette's own path samples — and a 2-D point is lifted to the NEAREST
//! pre-image on it. Nearest matters: a rim seen edge-on projects its near and
//! far halves onto the SAME segment, and the near one is what a drawing shows.
//!
//! The ray then runs from that 3-D point toward the viewer and the question is
//! whether it meets the solid's render tessellation. Two things make that
//! question answerable rather than a coin flip at the surface it starts on:
//!
//! - **The origin is offset by the MEASURED local gap.** The render mesh is
//!   inscribed, so a point on a curved face's true surface sits OUTSIDE the
//!   mesh by up to the chord sagitta, and the ray would otherwise graze the
//!   face it starts on. The offset is the distance from the lifted point to
//!   the nearest candidate triangle — measured at that point, from the mesh in
//!   hand — plus a float margin. It is NOT the sagitta the caller's chord
//!   tolerance implies: `tessellate` always meshes at the render band, so a
//!   band keyed to the caller's density describes a mesh that was never built.
//!   That is the same lesson D1b's [`super::silhouette::chord_sagitta_rel`]
//!   note records, and the same correction.
//! - **The hit test is EXACT wherever the float test is not decisive.** A
//!   float ray/triangle solve answers the generic case; where the barycentric
//!   coordinates or the hit parameter land inside a float band — the ray
//!   passing through a triangle's edge or vertex, or along its plane — the
//!   verdict comes from `yang_rs::segment_intersects_triangle_3d`, Cherchi
//!   2022 §3's primitive over Shewchuk's adaptive `orient3d`. No orientation
//!   predicate is implemented here; the point of the seam is that there is
//!   exactly one of them in the tree.
//!
//! The one band that survives is the offset: an occluder standing closer to
//! the curve than the local mesh gap cannot be distinguished from the curve's
//! own surface. That is a thin-feature band of the render density, it is
//! measured rather than assumed, and it is the only approximation in the
//! verdict.
//!
//! ## What is counted rather than guessed
//!
//! - A piece whose 3-D depth cannot be recovered keeps its `Visible` tag and
//!   counts [`ProjectionDeclines::depth_unliftable`].
//! - A ray that only GRAZES a candidate triangle — running in its plane, or
//!   meeting it exactly on an edge or at a vertex — counts
//!   [`ProjectionDeclines::ray_grazes_face`], once for the curve. That face
//!   contributes no occlusion, and that is a decision rather than a fallback:
//!   a face hides a curve only by standing BETWEEN it and the viewer, which
//!   means the ray crosses from one side of it to the other, so a face the ray
//!   merely touches separates nothing. The count is there because the
//!   configuration is SYSTEMATIC rather than accidental, and the systematic
//!   cases are the ones where that argument is load-bearing: a bore's far rim
//!   sits at exactly the radius of the inscribed wall it grazes, and an
//!   axis-aligned view of a prismatic solid grazes every face parallel to the
//!   line of sight. Measured 2026-10-03: before this rule the exact
//!   predicate's `Intersects` (which covers an edge or vertex touch as well as
//!   an interior crossing) marked a cylinder's far rim HIDDEN behind its own
//!   bore, because the rim's samples and the end disc's polygon come from the
//!   same angular sampling and the ray went exactly through a shared vertex.
//! - The crossing search's work budget, and its near-tangential contacts, are
//!   [`ProjectionDeclines::split_budget`] and
//!   [`ProjectionDeclines::split_tangency`].
//!
//! Every one of those leans toward VISIBLE, which is the loud direction for a
//! drawing: a line that should have been dashed is drawn solid and is visible
//! on the paper, where a dropped line is not.
//!
//! ## What this does NOT do
//!
//! Visibility is computed per BODY, against that body's own tessellation. In a
//! view of several bodies a curve hidden behind a DIFFERENT body is still
//! reported visible, and [`crate::adapter`] counts
//! [`ProjectionDeclines::cross_body`] for it. That is the one decline here
//! that over-reports.

use cad_primitives::{Point2, Point3};
use waffle_types::kernel::projection::{
    Curve2, CurveDepth, ProjectedCurve, ProjectionDeclines, ViewBasis, Visibility,
};

use crate::arena::{BrepArena, SolidId};
use crate::error::KernelV2Error;
use crate::tessellate::RenderMesh;

use super::crossings::{self, Decomposed};

/// A projected curve together with the 3-D sample polyline of its own source,
/// at the density the projection used.
///
/// The lift is what makes a 2-D point answerable in depth. It is deliberately
/// the SAME sampling the rest of the kernel uses for that entity
/// ([`crate::introspect::edge_polyline`] for an edge, the silhouette path's
/// own refinement for a silhouette), so the drawing, the viewport's edge
/// overlay and this classification cannot disagree about where a curve is.
pub(crate) struct LiftedCurve {
    pub curve: ProjectedCurve,
    pub lift: Vec<Point3>,
}

/// Work budget for one view's crossing search, in the units
/// [`super::crossings`] charges (a segment pair is 1, a conic pair its sample
/// count).
///
/// A budget rather than a timeout, so the boundary is deterministic and
/// reproducible: the same view always declines at the same place. Exhausting
/// it leaves the remaining curves UNSPLIT — each classified at its own
/// midpoint, so still honestly tagged, just not cut — and counts
/// [`ProjectionDeclines::split_budget`] once for the view. No view of the
/// corpus sample the §5.3 visibility oracle sweeps has reached it: that sweep
/// asserts `split_budget == 0`, so the number is pinned rather than assumed.
const SPLIT_BUDGET: u64 = 4_000_000;

/// Relative margin added to the measured local gap before the ray starts, and
/// used as the float noise band throughout. Relative to the solid's own 3-D
/// extent, since the kernel models in metres and a corpus body can be a
/// millimetre or a metre across.
const MARGIN_REL: f64 = 1e-9;

/// Barycentric slack below which a float ray/triangle hit is NOT decisive and
/// the exact predicate is asked instead. Dimensionless, so this is an absolute
/// number: a hit within a part in `1e-9` of a triangle's edge.
const BARY_EPS: f64 = 1e-9;

/// Classify `lifted` against `solid`'s own render tessellation, splitting and
/// merging as the module docs describe.
///
/// `n_seg` is the angular density the projection sampled its polylines at,
/// which is what the crossing search's chord band is derived from.
pub(crate) fn classify(
    arena: &BrepArena,
    solid: SolidId,
    basis: &ViewBasis,
    lifted: Vec<LiftedCurve>,
    n_seg: u32,
    declines: &mut ProjectionDeclines,
) -> Result<Vec<ProjectedCurve>, KernelV2Error> {
    if lifted.is_empty() {
        return Ok(Vec::new());
    }
    let mesh = crate::tessellate::tessellate(arena, solid)?;
    let Some(occ) = Occluders::new(&mesh, basis) else {
        // No triangles at all: nothing can be tested in front of anything, so
        // nothing is decided. A solid that tessellates to nothing is a kernel
        // defect elsewhere; here it is a loud count, not a clean wireframe.
        declines.depth_unliftable = declines
            .depth_unliftable
            .saturating_add(lifted.len() as u32);
        return Ok(lifted.into_iter().map(|l| l.curve).collect());
    };

    // The chord band, in model units: the view's own size times the sagitta of
    // the density the POLYLINES were sampled at. Only the polyline arms need
    // it; an analytic pair's roots land strictly inside their windows.
    let band = occ.view_size * super::silhouette::chord_sagitta_rel(n_seg);

    let splits = split_parameters(&lifted, band, declines);

    let mut out: Vec<ProjectedCurve> = Vec::with_capacity(lifted.len());
    for (idx, l) in lifted.into_iter().enumerate() {
        let LiftedCurve { curve, lift } = l;
        emit_classified(curve, &lift, &splits[idx], basis, &occ, declines, &mut out);
    }
    dedup_coincident(&mut out, band);
    Ok(out)
}

/// Every parameter each curve must be cut at: its crossings with every other
/// curve, plus its own folds.
fn split_parameters(
    lifted: &[LiftedCurve],
    band: f64,
    declines: &mut ProjectionDeclines,
) -> Vec<Vec<f64>> {
    let n = lifted.len();
    let decs: Vec<Decomposed> = lifted
        .iter()
        .map(|l| Decomposed::of(&l.curve.geometry))
        .collect();
    let mut splits: Vec<Vec<f64>> = lifted
        .iter()
        .map(|l| crossings::folds(&l.curve.geometry))
        .collect();
    let mut budget = SPLIT_BUDGET;
    let mut buf = Vec::new();
    let mut exhausted = false;
    'pairs: for i in 0..n {
        if decs[i].is_empty() {
            continue;
        }
        for j in (i + 1)..n {
            if decs[j].is_empty() || !crossings::boxes_overlap(&decs[i].bbox, &decs[j].bbox, band) {
                continue;
            }
            if budget == 0 {
                exhausted = true;
                break 'pairs;
            }
            buf.clear();
            let tangencies = crossings::crossings(&decs[i], &decs[j], band, &mut buf, &mut budget);
            declines.split_tangency = declines.split_tangency.saturating_add(tangencies);
            for c in &buf {
                splits[i].push(c.a);
                splits[j].push(c.b);
            }
        }
    }
    if exhausted {
        declines.split_budget = declines.split_budget.saturating_add(1);
    }
    splits
}

/// Split one curve at `splits`, classify each piece, merge the runs that
/// agree, and append the result.
fn emit_classified(
    curve: ProjectedCurve,
    lift: &[Point3],
    splits: &[f64],
    basis: &ViewBasis,
    occ: &Occluders,
    declines: &mut ProjectionDeclines,
    out: &mut Vec<ProjectedCurve>,
) {
    let Some((t0, t1)) = curve.geometry.param_range() else {
        // An empty polyline: nothing to classify and nothing to draw.
        out.push(curve);
        return;
    };
    if t1 <= t0 {
        // A DEGENERATE domain — a `Curve2::Point` (a line running along the
        // line of sight), or a one-point polyline. There is no interval to cut
        // and `subcurve` has nothing to answer over one, so the curve is
        // classified once at its only parameter and kept whole. Falling
        // through to the windows loop below drops it instead: `bounds` is
        // `[t0, t0]`, its one window is empty, and nothing is ever flushed —
        // which lost a box's four corner dots from every top view, with no
        // decline to say so (there is no counter for a dropped curve, so
        // neither the declines nor the §5.3 oracle, which only judges the
        // curves that came back, could see it).
        let (vis, occluder) = verdict(&curve.geometry, lift, t0, t1, basis, occ, declines);
        let depth = curve
            .geometry
            .eval(t0)
            .and_then(|q| lift_point(basis, lift, q))
            .map(|(at_midpoint, _)| CurveDepth {
                at_midpoint,
                occluder,
            });
        if depth.is_none() {
            declines.depth_unliftable = declines.depth_unliftable.saturating_add(1);
        }
        out.push(ProjectedCurve {
            geometry: curve.geometry,
            visibility: vis,
            kind: curve.kind,
            source: curve.source,
            depth,
        });
        return;
    }
    // The cut parameters, strictly inside the domain and deduplicated: two
    // curves meeting a third at the same place report the same parameter
    // twice, and a zero-length piece is not a drawing.
    let eps = (t1 - t0).abs() * 1e-9;
    let mut ts: Vec<f64> = splits
        .iter()
        .copied()
        .filter(|t| t.is_finite() && *t > t0 + eps && *t < t1 - eps)
        .collect();
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|b, a| (*b - *a).abs() <= eps);

    let mut bounds: Vec<f64> = Vec::with_capacity(ts.len() + 2);
    bounds.push(t0);
    bounds.extend(ts);
    bounds.push(t1);

    // Classify each piece at its own midpoint, then merge the adjacent runs
    // that agree. The run's reported depth is re-measured at the MERGED
    // midpoint (it is that curve's own depth, not a piece's) while its
    // occluder is the nearest one found anywhere along the run.
    let mut run_start = t0;
    let mut run_vis: Option<Visibility> = None;
    let mut run_occluder: Option<f64> = None;
    let flush = |from: f64,
                 to: f64,
                 vis: Visibility,
                 occluder: Option<f64>,
                 out: &mut Vec<ProjectedCurve>,
                 declines: &mut ProjectionDeclines| {
        let Some(geometry) = curve.geometry.subcurve(from, to) else {
            return;
        };
        let at_midpoint = geometry
            .eval(midparam(&geometry))
            .and_then(|q| lift_point(basis, lift, q))
            .map(|(d, _)| d);
        let depth = at_midpoint.map(|at_midpoint| CurveDepth {
            at_midpoint,
            occluder,
        });
        if depth.is_none() {
            declines.depth_unliftable = declines.depth_unliftable.saturating_add(1);
        }
        out.push(ProjectedCurve {
            geometry,
            visibility: vis,
            kind: curve.kind,
            source: curve.source,
            depth,
        });
    };
    for w in bounds.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b <= a {
            continue;
        }
        let (vis, occluder) = verdict(&curve.geometry, lift, a, b, basis, occ, declines);
        match run_vis {
            Some(prev) if prev == vis => {
                run_occluder = min_opt(run_occluder, occluder);
            }
            Some(prev) => {
                flush(run_start, a, prev, run_occluder, out, declines);
                run_start = a;
                run_vis = Some(vis);
                run_occluder = occluder;
            }
            None => {
                run_start = a;
                run_vis = Some(vis);
                run_occluder = occluder;
            }
        }
    }
    if let Some(vis) = run_vis {
        flush(run_start, t1, vis, run_occluder, out, declines);
    }
}

/// The parameter halfway along a curve's own domain.
fn midparam(curve: &Curve2) -> f64 {
    match curve.param_range() {
        Some((a, b)) => 0.5 * (a + b),
        None => 0.0,
    }
}

fn min_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    }
}

/// Fractions of a piece's own parameter span at which the ray may be cast, in
/// order: the midpoint first, then four other interior points.
///
/// Visibility is constant along a piece — that is what the split established —
/// so ANY interior point of it gives the piece's verdict, and a cast that came
/// back DEGENERATE at one point can simply be redone at another. The
/// alternative is to guess what a degeneracy means, and the guess is wrong
/// either way: measured 2026-10-03 on corpus case C0009, a slot's blind-end
/// edge runs along `y = 0`, which is also the symmetry line the face's own CDT
/// put a triangulation seam on, so the ray from the piece's midpoint passed
/// exactly through that seam and every incident triangle reported a boundary
/// touch — while the face it was crossing stood a fifth of the solid in front
/// of it. One point to the side and the same ray crosses a triangle's interior.
///
/// A degeneracy at ONE point of a piece is a coincidence of the mesh's
/// triangulation; a degeneracy at every point is the configuration (a curve
/// lying IN a face parallel to the line of sight), and only the second is
/// counted as [`ProjectionDeclines::ray_grazes_face`].
///
/// The ORDER is also the order [`verdict`]'s span check probes in, which is
/// why the midpoint is followed by the two points FURTHEST from it rather than
/// by its neighbours: those three bracket the middle three fifths of the
/// piece, so an unsplit visibility change anywhere in that window makes two
/// probes disagree and is counted instead of being drawn whole. A change in
/// the outer fifths still escapes, which is the honest limit of a fixed probe
/// set — only the split can rule it out.
const RECAST_FRACTIONS: [f64; 5] = [0.5, 0.2, 0.8, 0.35, 0.65];

/// How many DECISIVE casts [`verdict`] takes on one piece before it stops.
///
/// One is the verdict and the others are its confirmation. Three rather than
/// two because two adjacent probes bracket only the span between them: with
/// the midpoint and both outer probes of [`RECAST_FRACTIONS`], the window in
/// which an unsplit change is caught is three fifths of the piece rather than
/// three tenths, for one more ray cast per piece against a mesh the view has
/// already built.
const SPAN_PROBES: u32 = 3;

/// Is the curve visible over `[a, b]`, and at what occluder depth?
///
/// The verdict is the FIRST decisive cast's — the midpoint's, unless that one
/// was degenerate — and a second decisive cast elsewhere on the piece has to
/// CONFIRM it. That second cast is what keeps `RECAST_FRACTIONS` honest: the
/// re-cast rests on "visibility is constant along a piece", which holds only
/// if the piece was split at every crossing and tangency, and the split
/// declines some of both ([`ProjectionDeclines::split_tangency`],
/// [`ProjectionDeclines::silhouette_off_face`]). Where the premise fails, the
/// two points can sit on opposite sides of an unsplit change and answer
/// differently — so a disagreement is DETECTED and counted as
/// [`ProjectionDeclines::piece_spans_change`], never resolved by taking more
/// probes and voting. Voting would convert a curve known to be half wrong into
/// one confidently claimed whole, which is the opposite of what a drawing's
/// reader needs.
fn verdict(
    geometry: &Curve2,
    lift: &[Point3],
    a: f64,
    b: f64,
    basis: &ViewBasis,
    occ: &Occluders,
    declines: &mut ProjectionDeclines,
) -> (Visibility, Option<f64>) {
    let mut unliftable = false;
    let mut decided: Option<(Visibility, Option<f64>)> = None;
    let mut n_decided = 0u32;
    for f in RECAST_FRACTIONS {
        let t = a + (b - a) * f;
        let Some(q) = geometry.eval(t) else {
            unliftable = true;
            continue;
        };
        let Some((_, p3)) = lift_point(basis, lift, q) else {
            unliftable = true;
            continue;
        };
        let this = match occ.occluder_depth(p3, q, basis) {
            // Something is in front: decided, whatever else the cast grazed.
            (Some(d), _) => (Visibility::Hidden, Some(d)),
            // Nothing in front and nothing grazed: decided.
            (None, false) => (Visibility::Visible, None),
            // Nothing in front, but the ray only TOUCHED what it met. Not a
            // verdict; try another point of the same piece.
            (None, true) => continue,
        };
        // The first decisive cast speaks for the piece; the rest confirm or
        // contradict it.
        let first = *decided.get_or_insert(this);
        n_decided += 1;
        if first.0 != this.0 {
            declines.piece_spans_change = declines.piece_spans_change.saturating_add(1);
            return first;
        }
        if n_decided >= SPAN_PROBES {
            return first;
        }
    }
    if let Some(answer) = decided {
        // Fewer than `SPAN_PROBES` points of the piece could be decided at all
        // — the rest grazed or could not be lifted — so there is less to
        // confirm against than asked for, and what was decided stands.
        return answer;
    }
    if unliftable {
        declines.depth_unliftable = declines.depth_unliftable.saturating_add(1);
    } else {
        // Grazing at every probed point of the piece: the configuration, not a
        // coincidence. A face the ray only touches separates nothing, so the
        // piece stays visible, and the count says the verdict rests on that
        // argument.
        declines.ray_grazes_face = declines.ray_grazes_face.saturating_add(1);
    }
    (Visibility::Visible, None)
}

/// The 3-D point of `q` on a curve whose source samples to `lift`, and its
/// depth — the NEAREST pre-image when the projection is not injective (a rim
/// seen edge-on), since that is the one a drawing shows.
fn lift_point(basis: &ViewBasis, lift: &[Point3], q: Point2) -> Option<(f64, [f64; 3])> {
    match lift.len() {
        0 => None,
        1 => {
            let p = lift[0].as_array();
            Some((basis.project(p).1, p))
        }
        _ => {
            let uv: Vec<Point2> = lift.iter().map(|p| basis.project(p.as_array()).0).collect();
            let mut span: f64 = 0.0;
            let mut nearest = f64::INFINITY;
            for w in uv.windows(2) {
                span = span.max((w[1].x() - w[0].x()).abs().max((w[1].y() - w[0].y()).abs()));
                nearest = nearest.min(crossings::point_segment_distance(q, w[0], w[1]));
            }
            if !nearest.is_finite() {
                return None;
            }
            // Every chord that passes within float noise of the nearest one is
            // a pre-image of `q`; the drawing shows the nearest in DEPTH.
            let accept = nearest + MARGIN_REL * span.max(nearest) + f64::MIN_POSITIVE;
            let mut best: Option<(f64, [f64; 3])> = None;
            for (i, w) in uv.windows(2).enumerate() {
                if crossings::point_segment_distance(q, w[0], w[1]) > accept {
                    continue;
                }
                let s = chord_param(q, w[0], w[1]);
                let (a, b) = (lift[i].as_array(), lift[i + 1].as_array());
                let p = [
                    a[0] + s * (b[0] - a[0]),
                    a[1] + s * (b[1] - a[1]),
                    a[2] + s * (b[2] - a[2]),
                ];
                let d = basis.project(p).1;
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, p));
                }
            }
            best
        }
    }
}

/// Where along the chord `a → b` the nearest point to `p` is, clamped.
fn chord_param(p: Point2, a: Point2, b: Point2) -> f64 {
    let (vx, vy) = (b.x() - a.x(), b.y() - a.y());
    let len2 = vx * vx + vy * vy;
    if len2 <= 0.0 {
        return 0.0;
    }
    (((p.x() - a.x()) * vx + (p.y() - a.y()) * vy) / len2).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// the coincidence merge
// ---------------------------------------------------------------------------

/// Drop curves that are COINCIDENT in `(u, v)` with an earlier curve of the
/// same visibility — §5.2's "segments coincident in (u, v) with the same
/// visibility are merged".
///
/// Coincidence is tested geometrically (sample one curve, measure against the
/// other's flattening) rather than by comparing the two representations, since
/// the same point set can arrive as an `Ellipse` from one rim and a `Polyline`
/// from another. The sweep is over the `u` order with an active set, so the
/// cost is linear in the number of curves times the number that overlap any
/// one of them, not quadratic in the view.
///
/// Curves of DIFFERENT visibility are never merged, and that is the case that
/// matters: two coincident curves one of which is hidden are a near line and a
/// far one, and a drawing needs both.
///
/// Of two that DO merge, the survivor is the NEARER — the line a drawing
/// actually shows — with the earlier curve winning a tie, so a silhouette that
/// reproduces an edge at the same depth loses to the edge (the edge pass runs
/// first) and keeps the `CurveKind::Edge` tag on the one line that is drawn.
fn dedup_coincident(curves: &mut Vec<ProjectedCurve>, band: f64) {
    const SAMPLES: usize = 7;
    let n = curves.len();
    if n < 2 {
        return;
    }
    let boxes: Vec<[f64; 4]> = curves
        .iter()
        .map(|c| {
            let bb = c.geometry.bbox();
            [bb.min.x(), bb.min.y(), bb.max.x(), bb.max.y()]
        })
        .collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| boxes[*a][0].total_cmp(&boxes[*b][0]));

    let tol = band.max(f64::MIN_POSITIVE);
    let mut drop = vec![false; n];
    let mut active: Vec<usize> = Vec::new();
    for &i in &order {
        active.retain(|&k| boxes[k][2] + tol >= boxes[i][0]);
        for &k in &active {
            if drop[k] || curves[k].visibility != curves[i].visibility {
                continue;
            }
            if !crossings::boxes_overlap(&boxes[k], &boxes[i], tol) {
                continue;
            }
            if same_point_set(&curves[i].geometry, &curves[k].geometry, tol, SAMPLES) {
                let depth_of = |n: usize| curves[n].depth.map_or(f64::INFINITY, |d| d.at_midpoint);
                let (dk, di) = (depth_of(k), depth_of(i));
                // The FARTHER one goes; a tie goes to the later index, which
                // keeps the edge over a silhouette that reproduces it.
                let loser = if di < dk { k } else { i };
                drop[loser] = true;
                if loser == i {
                    break;
                }
            }
        }
        if !drop[i] {
            active.push(i);
        }
    }
    let mut keep = drop.iter();
    curves.retain(|_| !*keep.next().unwrap_or(&false));
}

/// Whether two curves are the same point set within `tol`: each is sampled and
/// measured against the other's flattening, both ways, since one containing
/// the other is not the same as the two being equal.
fn same_point_set(a: &Curve2, b: &Curve2, tol: f64, samples: usize) -> bool {
    let one_way = |x: &Curve2, y: &Curve2| -> bool {
        let Some((t0, t1)) = x.param_range() else {
            return false;
        };
        let pts = y.flatten(tol * 0.25);
        if pts.is_empty() {
            return false;
        }
        for k in 0..=samples {
            let t = t0 + (t1 - t0) * (k as f64) / (samples as f64);
            let Some(p) = x.eval(t) else { return false };
            let mut best = f64::INFINITY;
            if pts.len() == 1 {
                best = (p.x() - pts[0].x()).hypot(p.y() - pts[0].y());
            }
            for w in pts.windows(2) {
                best = best.min(crossings::point_segment_distance(p, w[0], w[1]));
            }
            if y.is_closed() && pts.len() > 2 {
                best = best.min(crossings::point_segment_distance(
                    p,
                    pts[pts.len() - 1],
                    pts[0],
                ));
            }
            if best > tol {
                return false;
            }
        }
        true
    };
    one_way(a, b) && one_way(b, a)
}

// ---------------------------------------------------------------------------
// the occluders
// ---------------------------------------------------------------------------

/// The solid's render triangles, indexed by their footprint in the view plane.
///
/// A view ray projects to a single point, so only the triangles whose
/// projected box covers that point can be hit — which makes a uniform grid
/// over the view plane the whole acceleration structure needed, with one cell
/// lookup per query.
struct Occluders {
    tris: Vec<[[f64; 3]; 3]>,
    cells: Vec<Vec<u32>>,
    nx: usize,
    ny: usize,
    min: [f64; 2],
    cell: f64,
    /// Smallest depth over every mesh vertex: nothing of the solid is in front
    /// of this, so a ray that reaches it has left.
    front_depth: f64,
    /// The solid's 3-D extent, which every relative margin is taken against.
    extent: f64,
    /// The view's own size, which the chord band is taken against.
    view_size: f64,
    /// `KV2_VISIBILITY_PROBE`, read ONCE here rather than per ray: this is the
    /// kernel's hot path and an env lookup per query costs more than the cast.
    probe: bool,
}

impl Occluders {
    fn new(mesh: &RenderMesh, basis: &ViewBasis) -> Option<Occluders> {
        let n_tri = mesh.indices.len() / 3;
        if n_tri == 0 {
            return None;
        }
        let mut tris: Vec<[[f64; 3]; 3]> = Vec::with_capacity(n_tri);
        let mut boxes: Vec<[f64; 4]> = Vec::with_capacity(n_tri);
        let mut uv_min = [f64::INFINITY; 2];
        let mut uv_max = [f64::NEG_INFINITY; 2];
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        let mut front_depth = f64::INFINITY;
        for t in mesh.indices.chunks_exact(3) {
            let (Some(a), Some(b), Some(c)) = (
                super::silhouette::mesh_vertex(mesh, t[0]),
                super::silhouette::mesh_vertex(mesh, t[1]),
                super::silhouette::mesh_vertex(mesh, t[2]),
            ) else {
                continue;
            };
            let mut bb = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for p in [a, b, c] {
                let (q, d) = basis.project(p);
                bb[0] = bb[0].min(q.x());
                bb[1] = bb[1].min(q.y());
                bb[2] = bb[2].max(q.x());
                bb[3] = bb[3].max(q.y());
                front_depth = front_depth.min(d);
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            uv_min[0] = uv_min[0].min(bb[0]);
            uv_min[1] = uv_min[1].min(bb[1]);
            uv_max[0] = uv_max[0].max(bb[2]);
            uv_max[1] = uv_max[1].max(bb[3]);
            tris.push([a, b, c]);
            boxes.push(bb);
        }
        if tris.is_empty() || !front_depth.is_finite() {
            return None;
        }
        let view_size = (uv_max[0] - uv_min[0]).max(uv_max[1] - uv_min[1]).max(0.0);
        let extent = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2))
            .sqrt()
            .max(f64::MIN_POSITIVE);

        // One cell per triangle on average, capped so a dense body does not
        // pay for a huge sparse grid.
        let side = ((tris.len() as f64).sqrt().ceil() as usize).clamp(1, 512);
        let cell = if view_size > 0.0 {
            view_size / side as f64
        } else {
            f64::MAX
        };
        let (nx, ny) = (side, side);
        let mut occ = Occluders {
            tris,
            cells: vec![Vec::new(); nx * ny],
            nx,
            ny,
            min: uv_min,
            cell,
            front_depth,
            extent,
            view_size,
            probe: std::env::var_os("KV2_VISIBILITY_PROBE").is_some(),
        };
        for (i, bb) in boxes.iter().enumerate() {
            let (x0, y0) = occ.cell_of(bb[0], bb[1]);
            let (x1, y1) = occ.cell_of(bb[2], bb[3]);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    occ.cells[y * nx + x].push(i as u32);
                }
            }
        }
        Some(occ)
    }

    fn cell_of(&self, u: f64, v: f64) -> (usize, usize) {
        let ix = if self.cell.is_finite() && self.cell > 0.0 {
            ((u - self.min[0]) / self.cell).floor()
        } else {
            0.0
        };
        let iy = if self.cell.is_finite() && self.cell > 0.0 {
            ((v - self.min[1]) / self.cell).floor()
        } else {
            0.0
        };
        (
            (ix.max(0.0) as usize).min(self.nx - 1),
            (iy.max(0.0) as usize).min(self.ny - 1),
        )
    }

    fn candidates(&self, q: Point2) -> &[u32] {
        let (x, y) = self.cell_of(q.x(), q.y());
        &self.cells[y * self.nx + x]
    }

    /// Depth of the nearest face standing in front of `p3`, and whether the
    /// cast GRAZED anything — touched a triangle without crossing its
    /// interior. The caller re-casts a grazing miss elsewhere on the same
    /// piece (see [`verdict`]).
    fn occluder_depth(&self, p3: [f64; 3], q: Point2, basis: &ViewBasis) -> (Option<f64>, bool) {
        let cands = self.candidates(q);
        if cands.is_empty() {
            return (None, false);
        }
        let margin = MARGIN_REL * self.extent;
        // The MEASURED local gap between the curve's own 3-D point and the
        // mesh here: the mesh is inscribed, so a point on a curved face's true
        // surface sits outside it by up to the chord sagitta, and a ray that
        // started at `p3` would graze the face it belongs to.
        let mut gap = f64::INFINITY;
        for &i in cands {
            let [a, b, c] = self.tris[i as usize];
            gap = gap.min(super::silhouette::distance_to_triangle(p3, a, b, c));
        }
        let offset = if gap.is_finite() { gap } else { 0.0 } + margin;

        let w = basis.w;
        let origin = [
            p3[0] - offset * w[0],
            p3[1] - offset * w[1],
            p3[2] - offset * w[2],
        ];
        let depth_origin = basis.project(origin).1;
        // Far enough along the ray that nothing of the solid is beyond it. If
        // the offset origin is ALREADY in front of the whole mesh, there is
        // nothing to hit and no segment to build.
        let far = depth_origin - self.front_depth + margin;
        if !(far.is_finite() && far > 0.0) {
            return (None, false);
        }
        let dir = [-w[0], -w[1], -w[2]];
        let p_far = [
            origin[0] + far * dir[0],
            origin[1] + far * dir[1],
            origin[2] + far * dir[2],
        ];

        if self.probe {
            println!(
                "[vis] q=({:.6},{:.6}) p3={p3:?} cands={} gap={gap:.6e} offset={offset:.6e} \
                 far={far:.6e} front_depth={:.6e} depth_origin={depth_origin:.6e}",
                q.x(),
                q.y(),
                cands.len(),
                self.front_depth
            );
        }
        let mut best: Option<f64> = None;
        let mut grazed = false;
        for &i in cands {
            let [a, b, c] = self.tris[i as usize];
            let t = match ray_triangle(origin, dir, a, b, c, margin) {
                RayHit::Miss => continue,
                RayHit::At(t) if t <= far => Some(t),
                RayHit::At(_) => continue,
                RayHit::Grazing => {
                    match yang_rs::segment_intersects_triangle_3d(
                        pt(origin),
                        pt(p_far),
                        pt(a),
                        pt(b),
                        pt(c),
                    ) {
                        // The float test already placed this contact on the
                        // triangle's boundary or in its plane, and the exact
                        // predicate confirms there IS one — but a contact
                        // that does not cross the triangle's interior
                        // separates nothing, so it does not occlude. Counted
                        // ONCE for the curve, below: a ray grazing a face
                        // grazes every triangle of it, and a per-triangle
                        // tally would report the mesh's density rather than
                        // the configuration.
                        yang_rs::SegmentTriangleIntersection::Intersects
                        | yang_rs::SegmentTriangleIntersection::Coplanar => {
                            grazed = true;
                            continue;
                        }
                        // And the exact predicate's own answer where it is
                        // decisive: no contact at all, so nothing to count.
                        yang_rs::SegmentTriangleIntersection::Disjoint => continue,
                    }
                }
            };
            if let Some(t) = t {
                best = Some(best.map_or(t, |b: f64| b.min(t)));
            }
        }
        if self.probe {
            println!("[vis]   -> best={best:?} grazed={grazed}");
        }
        (best.map(|t| depth_origin - t), grazed)
    }
}

fn pt(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

/// What a float ray/triangle solve could establish.
enum RayHit {
    /// Decisively no hit in front.
    Miss,
    /// Decisively a hit, at this ray parameter.
    At(f64),
    /// Not decisive — the ray passes through a triangle edge or vertex, lies
    /// near its plane, or meets it at a parameter inside the noise band. No
    /// parameter is carried: the exact predicate settles whether there is a
    /// contact, and a contact that does not cross the triangle's interior
    /// does not occlude, so there is no depth to report from one.
    Grazing,
}

/// Möller–Trumbore, with every inconclusive configuration reported as
/// [`RayHit::Grazing`] instead of guessed. `dir` is unit.
fn ray_triangle(
    origin: [f64; 3],
    dir: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    margin: f64,
) -> RayHit {
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let h = cross(dir, e2);
    let det = dot(e1, h);
    let (l1, l2) = (norm(e1), norm(e2));
    if !det.is_finite() || l1 <= 0.0 || l2 <= 0.0 {
        return RayHit::Miss; // a degenerate triangle occludes nothing
    }
    // `det` is `(dir × e2) · e1`, an area; normalized it is the sine of the
    // angle between the ray and the triangle's plane.
    if det.abs() <= BARY_EPS * l1 * l2 {
        return RayHit::Grazing;
    }
    let inv = 1.0 / det;
    let s = [origin[0] - a[0], origin[1] - a[1], origin[2] - a[2]];
    let u = inv * dot(s, h);
    let qv = cross(s, e1);
    let v = inv * dot(dir, qv);
    let t = inv * dot(e2, qv);
    if !(u.is_finite() && v.is_finite() && t.is_finite()) {
        return RayHit::Grazing;
    }
    let bary_min = u.min(v).min(1.0 - u - v);
    if bary_min < -BARY_EPS {
        return RayHit::Miss;
    }
    if bary_min <= BARY_EPS || t.abs() <= margin {
        return RayHit::Grazing;
    }
    if t > 0.0 {
        RayHit::At(t)
    } else {
        RayHit::Miss
    }
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

#[cfg(test)]
mod tests;
