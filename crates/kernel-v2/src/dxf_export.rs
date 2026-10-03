//! DXF export of ONE drawing view — the early deliverable of
//! `specs/drawings_and_mbd.md` §12 ("D1a + a one-view DXF export, which covers
//! laser, waterjet and plasma flat-pattern workflows before any sheet UI
//! exists"), and the first half of §8's `export_dxf`.
//!
//! This is a pure data → text writer over
//! [`waffle_types::kernel::projection::ViewGeometry`]: it holds no geometry
//! knowledge of its own, and the projection ([`crate::projection`]) is what
//! decides what the curves are. The sibling of [`crate::step_export`] in
//! placement and shape, for the same reason — an exporter lives beside the
//! kernel that owns the geometry it writes, and the bridge reaches it through
//! a trait method rather than by linking the writer itself.
//!
//! ## Which DXF
//!
//! **R12 / `AC1009`**, the most widely readable dialect and the one every CAM
//! front end on a cutting table accepts. R12's entity set is the constraint
//! that shapes the output:
//!
//! | view curve | entity |
//! |---|---|
//! | [`Curve2::Point`] | `POINT` |
//! | [`Curve2::Line`] | `LINE` |
//! | [`Curve2::Circle`], full turn | `CIRCLE` |
//! | [`Curve2::Circle`], partial | `ARC` (angles in DEGREES, counter-clockwise) |
//! | [`Curve2::Ellipse`] | `POLYLINE` at [`DEFAULT_POLYLINE_SAGITTA`] |
//! | [`Curve2::Polyline`] | `POLYLINE` |
//!
//! **An ellipse is written as a polyline on purpose.** R12 has no `ELLIPSE`
//! entity (it arrived in R13) and no `LWPOLYLINE` (R14), so the choice is a
//! flattened `POLYLINE` in a file everything reads, or an `ELLIPSE` in a file
//! whose own `$ACADVER` says the entity cannot be there. The analytic ellipse
//! is NOT lost — it stays in `ViewGeometry`, where the SVG renderer (D3) and a
//! later R13+ writer can use it — and the flattening carries its own proved
//! sagitta bound (see [`Curve2::flatten`]). For the flat-pattern workflows
//! §12 names, the curves that matter most — a drilled hole's rim seen square
//! on — are true `CIRCLE`s.
//!
//! ## Units and layers
//!
//! DXF carries no intrinsic unit, so the file is written in **millimetres**
//! (the interchange convention every cutting table assumes, and the same
//! choice [`crate::step_export`] makes); `ViewGeometry` is in the kernel's
//! meters and is scaled here at the boundary.
//!
//! Curves land on the layers §8 names: `VISIBLE` and `HIDDEN`. D1a tags
//! everything visible, so `HIDDEN` is empty today; its layer record is written
//! anyway, so the file's layer table does not change shape when D1c starts
//! filling it. Both are `CONTINUOUS` — a dashed hidden-line type needs an
//! `LTYPE` table and belongs with the increment that produces hidden lines.

use waffle_types::kernel::projection::{Aabb2, Curve2, ViewGeometry, Visibility};

/// Meters → millimetres, as [`crate::step_export`] does.
const SCALE: f64 = 1000.0;

/// Default chord deviation for a curve this writer FLATTENS, in MODEL units
/// (meters): 10 µm, i.e. 0.01 mm in the written file — below the kerf and
/// positioning accuracy of the cutting processes this export serves.
///
/// It bounds the [`Curve2::Ellipse`] arm and nothing else. A
/// [`Curve2::Polyline`] arrives ALREADY sampled, by the projection, at the
/// render chord density (`tessellate::RENDER_CHORD_TOLERANCE_REL`, a RELATIVE
/// 1e-3 ⇒ 71 segments per turn), and this writer cannot tighten what it is
/// handed — the analytic curve is gone by then. That density is 0.0039 mm on
/// a 4 mm radius but 0.049 mm on a 50 mm one and 0.49 mm on a 500 mm one, so
/// on a large part a sampled curve is 50× looser than this constant. A caller
/// who needs a stated bound on those must ask the PROJECTION for it, through
/// `ProjectOpts::rel_chord_tolerance`.
///
/// Which curves those are is the §5.2 increment-1 table: SSI/surface-pair,
/// hyperbola and 3-D ellipse-arc edges. Lines, circles and circular arcs are
/// exact entities and the projected ellipse is flattened here, so a part built
/// from planes and cylinders — the flat-pattern case §12 names — has no
/// sampled curve in it at all.
pub const DEFAULT_POLYLINE_SAGITTA: f64 = 1.0e-5;

/// Layer for [`Visibility::Visible`] curves (`specs/drawings_and_mbd.md` §8).
pub const LAYER_VISIBLE: &str = "VISIBLE";

/// Layer for [`Visibility::Hidden`] curves. Empty until D1c.
pub const LAYER_HIDDEN: &str = "HIDDEN";

/// Write one view as an R12 DXF drawing.
///
/// `polyline_sagitta` is the chord deviation allowed when flattening a curve
/// R12 cannot carry, in model units; [`DEFAULT_POLYLINE_SAGITTA`] is the
/// usual choice.
pub fn write_dxf(view: &ViewGeometry, polyline_sagitta: f64) -> String {
    let mut out = String::new();
    header(&mut out, view.bbox);
    tables(&mut out);
    out.push_str("  0\nSECTION\n  2\nENTITIES\n");
    for curve in &view.curves {
        let layer = match curve.visibility {
            Visibility::Visible => LAYER_VISIBLE,
            Visibility::Hidden => LAYER_HIDDEN,
        };
        entity(&mut out, &curve.geometry, layer, polyline_sagitta);
    }
    out.push_str("  0\nENDSEC\n  0\nEOF\n");
    out
}

/// `$ACADVER` plus the drawing extents, which is what a reader zooms to.
fn header(out: &mut String, bbox: Option<Aabb2>) {
    out.push_str("  0\nSECTION\n  2\nHEADER\n");
    out.push_str("  9\n$ACADVER\n  1\nAC1009\n");
    out.push_str("  9\n$INSBASE\n");
    point(out, 0.0, 0.0);
    let (min, max) = match bbox {
        // An empty view has no extents; (0,0)–(0,0) is what a reader does
        // least badly with, and the file is honestly empty either way.
        None => ((0.0, 0.0), (0.0, 0.0)),
        Some(bb) => (
            (bb.min.x() * SCALE, bb.min.y() * SCALE),
            (bb.max.x() * SCALE, bb.max.y() * SCALE),
        ),
    };
    out.push_str("  9\n$EXTMIN\n");
    point(out, min.0, min.1);
    out.push_str("  9\n$EXTMAX\n");
    point(out, max.0, max.1);
    out.push_str("  0\nENDSEC\n");
}

/// The LAYER table. R12 wants a `TABLE`/`ENDTAB` pair with a count.
fn tables(out: &mut String) {
    out.push_str("  0\nSECTION\n  2\nTABLES\n");
    out.push_str("  0\nTABLE\n  2\nLAYER\n 70\n     2\n");
    layer(out, LAYER_VISIBLE, 7);
    layer(out, LAYER_HIDDEN, 8);
    out.push_str("  0\nENDTAB\n  0\nENDSEC\n");
}

fn layer(out: &mut String, name: &str, color: i32) {
    out.push_str("  0\nLAYER\n  2\n");
    out.push_str(name);
    out.push_str("\n 70\n     0\n 62\n");
    out.push_str(&format!("{color:6}\n"));
    out.push_str("  6\nCONTINUOUS\n");
}

fn entity(out: &mut String, curve: &Curve2, layer: &str, sagitta: f64) {
    match *curve {
        Curve2::Point(p) => {
            start(out, "POINT", layer);
            point(out, p.x() * SCALE, p.y() * SCALE);
        }
        Curve2::Line { start: a, end: b } => {
            start(out, "LINE", layer);
            point(out, a.x() * SCALE, a.y() * SCALE);
            second_point(out, b.x() * SCALE, b.y() * SCALE);
        }
        Curve2::Circle {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            if curve.is_closed() {
                start(out, "CIRCLE", layer);
                point(out, center.x() * SCALE, center.y() * SCALE);
                out.push_str(" 40\n");
                out.push_str(&real(radius * SCALE));
            } else {
                start(out, "ARC", layer);
                point(out, center.x() * SCALE, center.y() * SCALE);
                out.push_str(" 40\n");
                out.push_str(&real(radius * SCALE));
                let (a0, a1) = arc_degrees(start_angle, end_angle);
                out.push_str(" 50\n");
                out.push_str(&real(a0));
                out.push_str(" 51\n");
                out.push_str(&real(a1));
            }
        }
        // R12 has no ELLIPSE; the flattening is the entity (see module docs).
        Curve2::Ellipse { .. } => {
            polyline(out, &curve.flatten(sagitta), curve.is_closed(), layer);
        }
        Curve2::Polyline { ref points, closed } => polyline(out, points, closed, layer),
    }
}

/// An `ARC`'s groups 50 and 51 as DXF angles: degrees counter-clockwise from
/// the entity's `+x`, each in `[0, 360)`, sweeping from the first to the
/// second and wrapping through zero.
///
/// [`Curve2::Circle`] carries the radian interval the projection produced, and
/// that interval is only normalized to be INCREASING — it is anchored wherever
/// the circle's own frame put it. A projection that reverses the circle's
/// sense yields `[angle0 − sweep, angle0]` with `angle0 ∈ (−π, π]`, so a
/// negative start is the ordinary case and a start below `−360°` is reachable
/// for a wide arc. R12 readers are not obliged to normalize an out-of-range
/// angle and a CAM front end that clamps instead would cut the wrong arc, so
/// the wrapping happens here, where the file's own convention applies.
fn arc_degrees(start_angle: f64, end_angle: f64) -> (f64, f64) {
    let sweep = (end_angle - start_angle).to_degrees();
    let a0 = start_angle.to_degrees().rem_euclid(360.0);
    // `rem_euclid` can answer exactly 360.0 for a tiny negative input; the
    // range is half-open, so fold that back to 0.
    let wrap = |a: f64| if a >= 360.0 || !a.is_finite() { 0.0 } else { a };
    (wrap(a0), wrap((a0 + sweep).rem_euclid(360.0)))
}

fn polyline(out: &mut String, points: &[cad_primitives::Point2], closed: bool, layer: &str) {
    if points.is_empty() {
        return;
    }
    if points.len() == 1 {
        start(out, "POINT", layer);
        point(out, points[0].x() * SCALE, points[0].y() * SCALE);
        return;
    }
    start(out, "POLYLINE", layer);
    // 66 = "vertices follow", mandatory in R12.
    out.push_str(" 66\n     1\n");
    point(out, 0.0, 0.0);
    out.push_str(" 70\n");
    out.push_str(if closed { "     1\n" } else { "     0\n" });
    for p in points {
        start(out, "VERTEX", layer);
        point(out, p.x() * SCALE, p.y() * SCALE);
    }
    start(out, "SEQEND", layer);
}

fn start(out: &mut String, kind: &str, layer: &str) {
    out.push_str("  0\n");
    out.push_str(kind);
    out.push_str("\n  8\n");
    out.push_str(layer);
    out.push('\n');
}

/// Groups 10/20/30 — DXF is a 3-D format and a drawing lives at `z = 0`.
fn point(out: &mut String, x: f64, y: f64) {
    out.push_str(" 10\n");
    out.push_str(&real(x));
    out.push_str(" 20\n");
    out.push_str(&real(y));
    out.push_str(" 30\n");
    out.push_str(&real(0.0));
}

/// Groups 11/21/31 — a `LINE`'s second endpoint.
fn second_point(out: &mut String, x: f64, y: f64) {
    out.push_str(" 11\n");
    out.push_str(&real(x));
    out.push_str(" 21\n");
    out.push_str(&real(y));
    out.push_str(" 31\n");
    out.push_str(&real(0.0));
}

/// A DXF real: fixed decimal, never an exponent (R12 readers vary on `1e-7`),
/// at nine decimals — picometre resolution on a millimetre file, so the
/// formatting is never the limiting error. `-0.0` is written as `0.0` so the
/// text is a function of the geometry and not of which way a zero was signed.
fn real(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v:.9}\n")
}

#[cfg(test)]
mod tests;
