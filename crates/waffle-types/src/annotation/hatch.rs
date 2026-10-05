//! A section cap's hatching, as line SEGMENTS
//! (`specs/drawings_and_mbd.md` §8, D4b/D4c).
//!
//! ## Why segments, and why HERE
//!
//! D4b's argument for segments stands: an SVG `<pattern>` fill, or a
//! `<clipPath>` with a family of long lines through it, would both be
//! shorter — and neither survives the trip to PDF, or to DXF, whose `HATCH`
//! entity is a different thing again. Computing the segments means the hatch
//! is the same geometry in every output, and the line a reader measures on
//! the screen is the line in the file.
//!
//! D4b computed them in the APP, which bought that property for the SVG and
//! the PDF (the PDF is scanned out of the SVG) and could not buy it for the
//! DXF at all: the DXF is written in Rust, so a hatched DXF would have meant
//! a SECOND scanline — two implementations of one fill, which is the thing
//! the segments exist to avoid. So the scanline moved here, below every
//! renderer, and the SVG now draws the segments the engine computed. That is
//! also the §3 division as written: "Rust produces curves and numbers; the
//! app draws them."
//!
//! ## The fill rule
//!
//! Even-odd, by a scanline. The loops are rotated so the hatch direction
//! becomes horizontal, each scanline's crossings with every loop edge are
//! collected and sorted, and consecutive pairs are the inside. A crossing is
//! counted with the half-open test `(y0 <= y) != (y1 <= y)`, which is what
//! makes a scanline passing exactly through a VERTEX count once rather than
//! twice — the classic even-odd bug, and the one a cap with a hole in it
//! meets at the hole's extremes.
//!
//! Even-odd rather than each loop's own winding, even though
//! [`HatchLoop::hole`] records the direction the kernel measured: an outer
//! loop nested inside another outer loop (two bodies, one inside a hollow of
//! the other) comes out right under even-odd without anyone having to work
//! out the nesting.
//!
//! ## Which space
//!
//! The loops' OWN space — a view's `(u, v)` in model meters — and `spacing`
//! and `sagitta` are in those same units, so this module has no opinion about
//! paper. The caller converts: the spacing is a PAPER quantity (3 mm
//! whatever the view's scale), so it arrives divided by the scale.
//!
//! Two consequences of working in the view plane rather than D4b's paper
//! space, both deliberate:
//!
//! - The scanline grid is anchored at the loops' coordinate ORIGIN, which for
//!   a view is its frame origin. The property D4b named — "two caps on one
//!   sheet, a section of two bodies, carry one continuous pattern instead of
//!   two that nearly line up" — is a property of one VIEW's caps, and it is
//!   preserved exactly. What is given up is alignment between two DIFFERENT
//!   section views on the same sheet, which no standard asks for and no
//!   reader can see; anchoring on the paper instead would make a view's
//!   layout depend on where the view was dragged to.
//! - A paper angle and a view angle differ in SIGN, because the renderer's
//!   paper has `y` down. The caller negates
//!   ([`feature_engine::drawing`] does, so the drawn lean is unchanged).

use super::layout::HatchLoop;
use crate::kernel::projection::Curve2;

/// Paper millimetres between hatch lines.
///
/// ISO 128-50 specifies section hatching as continuous NARROW lines at a
/// uniform spacing and leaves the spacing to the drawing. 3 mm is the middle
/// of general practice's 2–4 mm and coarse enough that a 10 mm cap reads as
/// hatched rather than as solid. A PAPER quantity, like the line widths.
pub const HATCH_SPACING_MM: f64 = 3.0;

/// The hatch direction, degrees counter-clockwise, measured on the PAPER.
pub const HATCH_ANGLE_DEG: f64 = 45.0;

/// Chord deviation allowed when flattening a curved hatch BOUNDARY, in paper
/// millimetres: 20 µm, a tenth of a narrow line's width, so the ends of the
/// hatch lines land on the outline a reader sees rather than beside it.
///
/// It bounds only the boundary sampling. The fill itself is exact given the
/// polygon.
pub const HATCH_BOUNDARY_SAGITTA_MM: f64 = 0.02;

/// A cap larger than this many scanlines is a degenerate input — a spacing of
/// nearly nothing, or a cap the size of a building. Reported rather than hung
/// on.
const MAX_LINES: usize = 20_000;

/// How to hatch, in the loops' own units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HatchParams {
    /// Distance between lines, in the loops' units.
    pub spacing: f64,
    /// Direction, radians counter-clockwise from `+u`, in the loops' frame.
    pub angle_rad: f64,
    /// Chord deviation for flattening a curved boundary, in the loops' units.
    pub sagitta: f64,
}

/// The hatch of a set of cap loops: the segments to draw, and what went wrong.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HatchFill {
    /// `[[x0, y0], [x1, y1]]` per line, in the loops' own units.
    pub segments: Vec<[[f64; 2]; 2]>,
    /// Degenerate boundaries and truncation, named rather than swallowed: a
    /// cap that came back unhatched should say why.
    pub warnings: Vec<String>,
}

/// Hatch `loops` under `params`.
pub fn hatch_segments(loops: &[HatchLoop], params: &HatchParams) -> HatchFill {
    let mut out = HatchFill::default();
    if !(params.spacing.is_finite() && params.spacing > 0.0) {
        out.warnings
            .push("the hatch spacing is not a positive length".to_string());
        return out;
    }
    if !params.angle_rad.is_finite() {
        out.warnings
            .push("the hatch angle is not a finite direction".to_string());
        return out;
    }
    let (c, s) = (params.angle_rad.cos(), params.angle_rad.sin());
    // Rotate BY −θ so the hatch lines lie along the x axis.
    let into = |p: [f64; 2]| [p[0] * c + p[1] * s, -p[0] * s + p[1] * c];
    let back = |p: [f64; 2]| [p[0] * c - p[1] * s, p[0] * s + p[1] * c];
    // A curved boundary is flattened in the ORIGINAL frame and rotated after,
    // because the sagitta is a property of the curve and not of the rotation.
    let sagitta = if params.sagitta.is_finite() && params.sagitta > 0.0 {
        params.sagitta
    } else {
        out.warnings
            .push("the hatch boundary sagitta is not a positive length".to_string());
        return out;
    };

    let mut rings: Vec<Vec<[f64; 2]>> = Vec::new();
    for loop_ in loops {
        let mut ring: Vec<[f64; 2]> = Vec::new();
        for curve in &loop_.curves {
            let points = curve.to_curve2().flatten(sagitta);
            if points.is_empty() {
                out.warnings
                    .push("a hatch boundary curve produced no points".to_string());
                continue;
            }
            for p in points {
                let q = into([p.x(), p.y()]);
                // Consecutive curves of a loop SHARE an endpoint, so the join
                // would otherwise be a duplicate vertex — and a duplicate
                // vertex is a zero-length edge, which the crossing test counts
                // as neither in nor out.
                if ring
                    .last()
                    .is_some_and(|l| (l[0] - q[0]).abs() < 1e-12 && (l[1] - q[1]).abs() < 1e-12)
                {
                    continue;
                }
                ring.push(q);
            }
        }
        if ring.len() >= 3 {
            rings.push(ring);
        } else if !ring.is_empty() {
            out.warnings
                .push("a hatch boundary loop had fewer than three distinct points".to_string());
        }
    }
    if rings.is_empty() {
        return out;
    }

    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for ring in &rings {
        for p in ring {
            if !p[1].is_finite() || !p[0].is_finite() {
                out.warnings
                    .push("a hatch boundary point is not finite".to_string());
                return out;
            }
            min_y = min_y.min(p[1]);
            max_y = max_y.max(p[1]);
        }
    }
    // The grid: multiples of the spacing from the ORIGIN, so the caps of one
    // view carry one continuous pattern rather than two that nearly line up.
    let first = (min_y / params.spacing).ceil() * params.spacing;
    let mut lines = 0usize;
    let mut crossings: Vec<f64> = Vec::new();
    let mut k = 0usize;
    loop {
        let y = first + params.spacing * (k as f64);
        if y > max_y {
            break;
        }
        k += 1;
        lines += 1;
        if lines > MAX_LINES {
            out.warnings.push(format!(
                "the hatch stopped at {MAX_LINES} lines; the cap is too large for the spacing"
            ));
            break;
        }
        crossings.clear();
        for ring in &rings {
            for i in 0..ring.len() {
                let [x0, y0] = ring[i];
                let [x1, y1] = ring[(i + 1) % ring.len()];
                if (y0 <= y) != (y1 <= y) {
                    crossings.push(x0 + ((y - y0) / (y1 - y0)) * (x1 - x0));
                }
            }
        }
        crossings.sort_by(f64::total_cmp);
        for pair in crossings.chunks_exact(2) {
            if pair[1] - pair[0] <= 1e-9 {
                continue;
            }
            out.segments.push([back([pair[0], y]), back([pair[1], y])]);
        }
    }
    // The last of this module's "a cap that came back unhatched should say
    // why". Every refusal above names itself, but two paths reached here
    // quietly: a ring of three or more distinct points enclosing NO AREA (a
    // collinear boundary), which yields crossings that never pair into a span,
    // and a grid that stepped over the cap entirely because no multiple of the
    // spacing fell between its extremes. Both answer "no hatch" with nothing
    // said, and a section view whose cap is unhatched for an unnamed reason
    // reads as a cap nobody sectioned.
    //
    // A warning rather than an error: an unhatched cap is a drawing a reader
    // can still use, and the third case this catches — a cap genuinely
    // narrower than the hatch pitch — is a legitimate drawing and not a
    // defect. What it must not be is unexplained.
    if out.segments.is_empty() && out.warnings.is_empty() {
        out.warnings.push(if lines == 0 {
            format!(
                "no hatch line fell inside the cap: its extent across the hatch direction is \
                 {:.3e}, narrower than the {:.3e} spacing",
                max_y - min_y,
                params.spacing
            )
        } else {
            format!(
                "{lines} hatch lines crossed the cap and none found an interior span; the \
                 boundary encloses no area"
            )
        });
    }
    out
}

/// Stored hatch segments as kernel-side curves (D4c), for the DXF writer.
///
/// The segments ride on [`super::layout::ViewLayout`] as plain `[f64; 2]`
/// pairs, because that record has to serialize; a writer that takes
/// [`Curve2`] needs them in that form. Here rather than at the writer's end
/// because `Curve2` is built on `cad_primitives::Point2`, which is two layers
/// below the bridge — and a conversion with no geometry in it has no business
/// pulling a dependency across the stack to get written twice.
pub fn segments_as_curves(segments: &[[[f64; 2]; 2]]) -> Vec<Curve2> {
    segments
        .iter()
        .map(|s| Curve2::Line {
            start: cad_primitives::Point2::new(s[0][0], s[0][1]),
            end: cad_primitives::Point2::new(s[1][0], s[1][1]),
        })
        .collect()
}

#[cfg(test)]
mod tests;
