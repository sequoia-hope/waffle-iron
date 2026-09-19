//! Roller-chain sprocket profile generation (ISO 606 tooth gap form).
//!
//! Part B3 of `specs/custom_features_and_modeling_roadmap.md`. A sprocket is
//! stored as one compact `SketchEntity::Sprocket` and expanded on demand into
//! the existing entity set — points and arcs only — so extrude, pattern and
//! boolean need nothing new and every wall is an exact cylinder.
//!
//! ISO 606 tooth gap, per gap (roller centred on the pitch circle):
//!
//! - pitch diameter `d = p / sin(π/z)`
//! - roller seating curve: an arc of radius `ri` about the roller centre,
//!   spanning the seating angle `α`, `ri ∈ [0.505·d1, 0.505·d1 + 0.069·∛d1]`
//!   (the cube root is taken with `d1` in **millimetres** — the standard's
//!   formula is dimensional), `α ∈ [120° − 90°/z, 140° − 90°/z]`
//! - tooth flanks: arcs of radius `re` tangent to the seating curve at its
//!   ends, `re ∈ [0.12·d1·(z + 2), 0.008·d1·(z² + 180)]`, convex (their
//!   centres lie beyond the seating curve, on the ray from the roller centre
//!   through the seating end point)
//! - tip diameter `da ∈ [d + (1 − 1.6/z)·p − d1, d + 1.25·p − d1]`; the flank
//!   arcs are cut by the tip circle and a tip arc (about the sprocket centre)
//!   joins neighbouring teeth
//! - root diameter `df = d − d1` nominal; the actual gap bottom is at
//!   `d/2 − ri`
//!
//! The generator picks the mid-range value of each ranged quantity by default
//! and exposes `ri`, `re`, `da` and `α` as overrides. Every profile is closed
//! by construction: the roller seating arc, both flank arcs and the tip arc
//! share their end points, so the profile is one loop of `4·z` arcs.
//!
//! Only the ISO 606 form is implemented. `SprocketStandard` is an enum so an
//! ANSI B29.1 tooth form (a different construction with its own constants)
//! can be added without a wire-format change; it is NOT implemented here
//! because its constants must come from the standard's text, which is not in
//! `refs/`.

use std::collections::HashMap;
use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use crate::profiles::{build_finish_profiles_with_synth_base, extract_profiles};
use crate::{ClosedProfile, SketchEntity};

/// Which tooth-form standard a sprocket is cut to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum SprocketStandard {
    /// ISO 606 tooth gap form (the only form implemented).
    #[default]
    Iso606,
}

/// Parameters for generating a roller-chain sprocket profile.
///
/// Lengths are metres (model units), like `GearParams`. camelCase on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SprocketParams {
    /// Number of teeth `z` (≥ 5).
    pub tooth_count: u32,
    /// Chain pitch `p` (m). ISO 08B is 12.7 mm.
    pub pitch: f64,
    /// Roller diameter `d1` (m). ISO 08B is 8.51 mm.
    pub roller_diameter: f64,
    #[serde(default)]
    pub center_x: f64,
    #[serde(default)]
    pub center_y: f64,
    /// Rotation of the whole profile about its centre (radians). At zero the
    /// first tooth GAP (roller seat) is centred on the +u axis.
    #[serde(default)]
    pub rotation_offset: f64,
    #[serde(default)]
    pub standard: SprocketStandard,
    /// Roller seating radius `ri` override (m). Default: mid-range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seating_radius: Option<f64>,
    /// Tooth flank radius `re` override (m). Default: mid-range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flank_radius: Option<f64>,
    /// Tip diameter `da` override (m). Default: mid-range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip_diameter: Option<f64>,
    /// Roller seating angle `α` override (degrees). Default: mid-range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seating_angle_deg: Option<f64>,
}

impl Default for SprocketParams {
    fn default() -> Self {
        Self {
            tooth_count: 20,
            pitch: 0.0127,
            roller_diameter: 0.00851,
            center_x: 0.0,
            center_y: 0.0,
            rotation_offset: 0.0,
            standard: SprocketStandard::Iso606,
            seating_radius: None,
            flank_radius: None,
            tip_diameter: None,
            seating_angle_deg: None,
        }
    }
}

/// Why a sprocket could not be generated. Every variant names the offending
/// number so the caller can show it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SprocketError {
    /// `tooth_count` below the minimum the construction supports.
    TooFewTeeth { tooth_count: u32, min: u32 },
    /// A length or angle is not finite and positive.
    BadValue { field: String, value: f64 },
    /// The roller would not fit its seat (`ri < d1 / 2`).
    SeatSmallerThanRoller {
        seating_radius: f64,
        roller_radius: f64,
    },
    /// The seating angle leaves no room for the flanks (`α ≥ 180°`).
    SeatingAngleTooLarge { seating_angle_deg: f64 },
    /// The flank arc never reaches the tip circle.
    TipUnreachable { tip_diameter: f64, flank_reach: f64 },
    /// Neighbouring flanks cross below the tip circle (pointed teeth); the
    /// tip diameter is too large for this tooth count.
    FlanksCross {
        tip_diameter: f64,
        max_tip_diameter: f64,
    },
}

impl std::fmt::Display for SprocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewTeeth { tooth_count, min } => {
                write!(f, "sprocket: tooth_count {tooth_count} is below the minimum {min}")
            }
            Self::BadValue { field, value } => {
                write!(f, "sprocket: {field} must be finite and positive (got {value})")
            }
            Self::SeatSmallerThanRoller {
                seating_radius,
                roller_radius,
            } => write!(
                f,
                "sprocket: seating radius {seating_radius} is smaller than the roller radius {roller_radius}"
            ),
            Self::SeatingAngleTooLarge { seating_angle_deg } => write!(
                f,
                "sprocket: seating angle {seating_angle_deg}° leaves no room for the flanks (must be < 180°)"
            ),
            Self::TipUnreachable {
                tip_diameter,
                flank_reach,
            } => write!(
                f,
                "sprocket: the flank arc never reaches the tip circle (tip diameter {tip_diameter}, flank reaches {flank_reach})"
            ),
            Self::FlanksCross {
                tip_diameter,
                max_tip_diameter,
            } => write!(
                f,
                "sprocket: neighbouring flanks cross below the tip circle (tip diameter {tip_diameter} exceeds {max_tip_diameter} for this tooth count)"
            ),
        }
    }
}

impl std::error::Error for SprocketError {}

/// The resolved (post-default) quantities of a sprocket, all in metres and
/// degrees, so a host can display what the mid-range defaults chose.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SprocketDimensions {
    pub pitch_diameter: f64,
    /// Nominal root diameter `d − d1`.
    pub root_diameter: f64,
    /// The diameter of the actual gap bottom, `d − 2·ri`.
    pub bottom_diameter: f64,
    pub tip_diameter: f64,
    pub seating_radius: f64,
    pub flank_radius: f64,
    pub seating_angle_deg: f64,
    /// ISO 606 range the defaults were taken from, for reference.
    pub seating_radius_range: (f64, f64),
    pub flank_radius_range: (f64, f64),
    pub tip_diameter_range: (f64, f64),
    pub seating_angle_deg_range: (f64, f64),
}

/// Result of generating a full sprocket profile with sketch entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprocketProfileResult {
    /// Points and arcs; the first entity is the sprocket centre point.
    pub entities: Vec<SketchEntity>,
    /// Every point the profile names: the entity points plus the arc samples
    /// `build_finish_profiles` mints (ids continue after the entities').
    #[serde(with = "crate::sketch::u32_key_map")]
    pub positions: HashMap<u32, (f64, f64)>,
    /// Exactly one closed profile, kernel-ready (`vertex_ids` + `arc_segments`).
    pub profiles: Vec<ClosedProfile>,
    pub pitch_radius: f64,
    pub dimensions: SprocketDimensions,
}

/// The minimum tooth count the construction accepts. ISO 606 sprockets in
/// practice start at 9 teeth; below 5 the tip circle falls inside the pitch
/// circle for every standard chain.
pub const MIN_TOOTH_COUNT: u32 = 5;

fn positive(field: &str, value: f64) -> Result<f64, SprocketError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(SprocketError::BadValue {
            field: field.to_string(),
            value,
        })
    }
}

/// Resolve the ISO 606 ranges and the values this sprocket uses. Validates the
/// parameters; the geometric checks (tip reachability, flank crossing) happen
/// during generation.
pub fn sprocket_dimensions(params: &SprocketParams) -> Result<SprocketDimensions, SprocketError> {
    let z = params.tooth_count;
    if z < MIN_TOOTH_COUNT {
        return Err(SprocketError::TooFewTeeth {
            tooth_count: z,
            min: MIN_TOOTH_COUNT,
        });
    }
    let p = positive("pitch", params.pitch)?;
    let d1 = positive("roller_diameter", params.roller_diameter)?;
    let zf = z as f64;

    let d = p / (PI / zf).sin();
    // 0.069·∛d1 is written with d1 in mm.
    let d1_mm = d1 * 1000.0;
    let ri_range = (0.505 * d1, 0.505 * d1 + 0.069e-3 * d1_mm.cbrt());
    let re_range = (0.12 * d1 * (zf + 2.0), 0.008 * d1 * (zf * zf + 180.0));
    let da_range = (d + (1.0 - 1.6 / zf) * p - d1, d + 1.25 * p - d1);
    let alpha_range = (120.0 - 90.0 / zf, 140.0 - 90.0 / zf);
    let mid = |r: (f64, f64)| 0.5 * (r.0 + r.1);

    let ri = match params.seating_radius {
        Some(v) => positive("seating_radius", v)?,
        None => mid(ri_range),
    };
    let re = match params.flank_radius {
        Some(v) => positive("flank_radius", v)?,
        None => mid(re_range),
    };
    let da = match params.tip_diameter {
        Some(v) => positive("tip_diameter", v)?,
        None => mid(da_range),
    };
    let alpha = match params.seating_angle_deg {
        Some(v) => positive("seating_angle_deg", v)?,
        None => mid(alpha_range),
    };
    if ri < 0.5 * d1 {
        return Err(SprocketError::SeatSmallerThanRoller {
            seating_radius: ri,
            roller_radius: 0.5 * d1,
        });
    }
    if alpha >= 180.0 {
        return Err(SprocketError::SeatingAngleTooLarge {
            seating_angle_deg: alpha,
        });
    }

    Ok(SprocketDimensions {
        pitch_diameter: d,
        root_diameter: d - d1,
        bottom_diameter: d - 2.0 * ri,
        tip_diameter: da,
        seating_radius: ri,
        flank_radius: re,
        seating_angle_deg: alpha,
        seating_radius_range: ri_range,
        flank_radius_range: re_range,
        tip_diameter_range: da_range,
        seating_angle_deg_range: alpha_range,
    })
}

/// The one gap the whole profile is made of, in the frame where the gap is
/// centred on the +x axis and the sprocket centre is the origin.
struct GapTemplate {
    /// Roller centre (on the pitch circle).
    seat_center: (f64, f64),
    /// Seating-arc end on the +y side / −y side.
    seat_end_plus: (f64, f64),
    seat_end_minus: (f64, f64),
    /// Flank-arc centres (beyond the seating ends, radius `re`).
    flank_center_plus: (f64, f64),
    flank_center_minus: (f64, f64),
    /// Where each flank meets the tip circle.
    tip_plus: (f64, f64),
    tip_minus: (f64, f64),
}

/// The gap template, with the `FlanksCross` bound filled in (the bound is
/// found by bisection over `gap_template_unbounded`, which reports it as 0).
fn gap_template(dims: &SprocketDimensions, z: u32) -> Result<GapTemplate, SprocketError> {
    gap_template_unbounded(dims, z).map_err(|e| match e {
        SprocketError::FlanksCross { tip_diameter, .. } => SprocketError::FlanksCross {
            tip_diameter,
            max_tip_diameter: max_tip_diameter(dims, z),
        },
        other => other,
    })
}

fn gap_template_unbounded(dims: &SprocketDimensions, z: u32) -> Result<GapTemplate, SprocketError> {
    let rp = 0.5 * dims.pitch_diameter;
    let ra = 0.5 * dims.tip_diameter;
    let ri = dims.seating_radius;
    let re = dims.flank_radius;
    let beta = 0.5 * dims.seating_angle_deg.to_radians();

    let seat_center = (rp, 0.0);
    // Unit vector from the roller centre to the +y seating end: the inward
    // direction (−x) rotated by −β.
    let u = (-beta.cos(), beta.sin());
    let seat_end_plus = (rp + ri * u.0, ri * u.1);
    let flank_center_plus = (rp + (ri + re) * u.0, (ri + re) * u.1);

    // Flank circle (centre C', radius re) ∩ tip circle (origin, radius ra).
    let (cx, cy) = flank_center_plus;
    let dist = cx.hypot(cy);
    let reach = dist + re;
    if reach <= ra || dist <= 0.0 {
        return Err(SprocketError::TipUnreachable {
            tip_diameter: dims.tip_diameter,
            flank_reach: 2.0 * reach,
        });
    }
    let a = (ra * ra - re * re + dist * dist) / (2.0 * dist);
    let h2 = ra * ra - a * a;
    if h2 < 0.0 {
        return Err(SprocketError::TipUnreachable {
            tip_diameter: dims.tip_diameter,
            flank_reach: 2.0 * reach,
        });
    }
    let h = h2.sqrt();
    let (ux, uy) = (cx / dist, cy / dist);
    let mid = (a * ux, a * uy);
    let candidates = [
        (mid.0 - h * uy, mid.1 + h * ux),
        (mid.0 + h * uy, mid.1 - h * ux),
    ];

    // The flank leaves the seating end counter-clockwise about its centre
    // (the tooth surface is convex); take the intersection reached first.
    let phi_seat = (seat_end_plus.1 - cy).atan2(seat_end_plus.0 - cx);
    let mut best: Option<((f64, f64), f64)> = None;
    for c in candidates {
        let phi = (c.1 - cy).atan2(c.0 - cx);
        let mut sweep = phi - phi_seat;
        while sweep <= 0.0 {
            sweep += 2.0 * PI;
        }
        while sweep > 2.0 * PI {
            sweep -= 2.0 * PI;
        }
        if sweep < PI && best.is_none_or(|(_, s)| sweep < s) {
            best = Some((c, sweep));
        }
    }
    let Some((tip_plus, _)) = best else {
        return Err(SprocketError::TipUnreachable {
            tip_diameter: dims.tip_diameter,
            flank_reach: 2.0 * reach,
        });
    };

    // The tip arc between this gap's +y flank and the next gap's −y flank
    // must have positive length: the flank tip's polar angle γ < π/z.
    let gamma = tip_plus.1.atan2(tip_plus.0);
    let half_pitch_angle = PI / z as f64;
    if gamma <= 0.0 || gamma >= half_pitch_angle * (1.0 - 1e-9) {
        return Err(SprocketError::FlanksCross {
            tip_diameter: dims.tip_diameter,
            max_tip_diameter: 0.0,
        });
    }

    Ok(GapTemplate {
        seat_center,
        seat_end_plus,
        seat_end_minus: (seat_end_plus.0, -seat_end_plus.1),
        flank_center_plus,
        flank_center_minus: (flank_center_plus.0, -flank_center_plus.1),
        tip_plus,
        tip_minus: (tip_plus.0, -tip_plus.1),
    })
}

/// The tip diameter at which neighbouring flanks meet exactly (pointed
/// teeth), found by bisection — reported in `FlanksCross` so the caller knows
/// how far to back off.
fn max_tip_diameter(dims: &SprocketDimensions, z: u32) -> f64 {
    let mut lo = dims.pitch_diameter;
    let mut hi = dims.tip_diameter;
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        let trial = SprocketDimensions {
            tip_diameter: mid,
            ..*dims
        };
        match gap_template_unbounded(&trial, z) {
            Ok(_) => lo = mid,
            Err(_) => hi = mid,
        }
    }
    lo
}

/// Generate a complete sprocket profile with sketch entities.
///
/// Emits the sprocket centre point first (so hosts can hang a construction
/// pitch circle on it, as they do for gears), then per gap its roller centre,
/// seating ends, flank centres and tip points, and per gap four arcs: seating,
/// two flanks, and the tip arc to the next gap. Arcs are CCW `start → end`
/// about their centre (the sketch convention), so concave seating arcs are
/// stored `+y end → −y end` and traversed in reverse by the CCW outer loop.
/// The profile is then finished exactly as a hand-drawn arc sketch is
/// (`build_finish_profiles`): sampled `vertex_ids` plus `arc_segments`, so
/// the kernel builds exact cylindrical walls.
pub fn generate_sprocket_profile(
    params: &SprocketParams,
) -> Result<SprocketProfileResult, SprocketError> {
    let dims = sprocket_dimensions(params)?;
    let z = params.tooth_count;
    let template = gap_template(&dims, z)?;

    let cx = params.center_x;
    let cy = params.center_y;
    let pitch_angle = 2.0 * PI / z as f64;

    let mut entities: Vec<SketchEntity> = Vec::with_capacity(11 * z as usize + 1);
    let mut positions: HashMap<u32, (f64, f64)> = HashMap::new();
    let mut next_id = 1u32;
    let mut add_point = |x: f64, y: f64, entities: &mut Vec<SketchEntity>| -> u32 {
        let id = next_id;
        next_id += 1;
        entities.push(SketchEntity::Point {
            id,
            x,
            y,
            construction: false,
        });
        positions.insert(id, (x, y));
        id
    };

    let center_id = add_point(cx, cy, &mut entities);

    struct GapIds {
        seat_center: u32,
        seat_plus: u32,
        seat_minus: u32,
        flank_center_plus: u32,
        flank_center_minus: u32,
        tip_plus: u32,
        tip_minus: u32,
    }
    let mut gaps: Vec<GapIds> = Vec::with_capacity(z as usize);
    for k in 0..z {
        let theta = params.rotation_offset + k as f64 * pitch_angle;
        let (ca, sa) = (theta.cos(), theta.sin());
        let place = |(x, y): (f64, f64)| (cx + x * ca - y * sa, cy + x * sa + y * ca);
        let sc = place(template.seat_center);
        let sp = place(template.seat_end_plus);
        let sm = place(template.seat_end_minus);
        let fp = place(template.flank_center_plus);
        let fm = place(template.flank_center_minus);
        let tp = place(template.tip_plus);
        let tm = place(template.tip_minus);
        gaps.push(GapIds {
            seat_center: add_point(sc.0, sc.1, &mut entities),
            seat_plus: add_point(sp.0, sp.1, &mut entities),
            seat_minus: add_point(sm.0, sm.1, &mut entities),
            flank_center_plus: add_point(fp.0, fp.1, &mut entities),
            flank_center_minus: add_point(fm.0, fm.1, &mut entities),
            tip_plus: add_point(tp.0, tp.1, &mut entities),
            tip_minus: add_point(tm.0, tm.1, &mut entities),
        });
    }

    let mut arc = |center_id: u32, start_id: u32, end_id: u32, entities: &mut Vec<SketchEntity>| {
        let id = next_id;
        next_id += 1;
        entities.push(SketchEntity::Arc {
            id,
            center_id,
            start_id,
            end_id,
            construction: false,
        });
    };
    for k in 0..z as usize {
        let g = &gaps[k];
        let next = &gaps[(k + 1) % z as usize];
        // Seating arc: concave, so CCW about the roller centre runs +y → −y.
        arc(g.seat_center, g.seat_plus, g.seat_minus, &mut entities);
        // Flanks: convex, CCW about their own centres from the seat outward
        // on the +y side and from the tip inward on the −y side.
        arc(g.flank_center_plus, g.seat_plus, g.tip_plus, &mut entities);
        arc(
            g.flank_center_minus,
            g.tip_minus,
            g.seat_minus,
            &mut entities,
        );
        // Tip arc to the next gap, CCW about the sprocket centre.
        arc(center_id, g.tip_plus, next.tip_minus, &mut entities);
    }

    let extracted = extract_profiles(&entities, &positions);
    let finished =
        build_finish_profiles_with_synth_base(&extracted, &entities, &positions, next_id);

    Ok(SprocketProfileResult {
        entities,
        positions: finished.solved_positions,
        profiles: finished.profiles,
        pitch_radius: 0.5 * dims.pitch_diameter,
        dimensions: dims,
    })
}

/// A closed polyline through the profile's sampled boundary, for live
/// preview rendering (mirrors `generate_gear_preview_polyline`).
pub fn generate_sprocket_preview_polyline(
    params: &SprocketParams,
) -> Result<Vec<(f64, f64)>, SprocketError> {
    let result = generate_sprocket_profile(params)?;
    let Some(profile) = result.profiles.first() else {
        return Ok(Vec::new());
    };
    let mut polyline: Vec<(f64, f64)> = profile
        .vertex_ids
        .iter()
        .filter_map(|id| result.positions.get(id).copied())
        .collect();
    if let Some(&first) = polyline.first() {
        polyline.push(first);
    }
    Ok(polyline)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ISO 08B: p = 12.7 mm, d1 = 8.51 mm.
    fn iso_08b(z: u32) -> SprocketParams {
        SprocketParams {
            tooth_count: z,
            pitch: 0.0127,
            roller_diameter: 0.00851,
            ..Default::default()
        }
    }

    /// The standard chains a bicycle or small machine would use:
    /// (pitch mm, roller mm) — ISO 06B, 08B, 10B, 12B, 16B, ANSI 25, 35, 40, 60.
    const CHAINS: &[(f64, f64)] = &[
        (9.525, 6.35),
        (12.7, 8.51),
        (15.875, 10.16),
        (19.05, 12.07),
        (25.4, 15.88),
        (6.35, 3.30),
        (9.525, 5.08),
        (12.7, 7.92),
        (19.05, 11.91),
    ];

    /// Distance from `c` to the polyline `pts` (closed).
    fn polyline_distance(pts: &[(f64, f64)], c: (f64, f64)) -> f64 {
        let mut best = f64::INFINITY;
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            let (abx, aby) = (b.0 - a.0, b.1 - a.1);
            let len2 = abx * abx + aby * aby;
            let t = if len2 > 0.0 {
                (((c.0 - a.0) * abx + (c.1 - a.1) * aby) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (px, py) = (a.0 + t * abx, a.1 + t * aby);
            best = best.min((c.0 - px).hypot(c.1 - py));
        }
        best
    }

    fn boundary(result: &SprocketProfileResult) -> Vec<(f64, f64)> {
        result.profiles[0]
            .vertex_ids
            .iter()
            .map(|id| result.positions[id])
            .collect()
    }

    fn signed_area(pts: &[(f64, f64)]) -> f64 {
        let mut a = 0.0;
        for i in 0..pts.len() {
            let p = pts[i];
            let q = pts[(i + 1) % pts.len()];
            a += p.0 * q.1 - q.0 * p.1;
        }
        0.5 * a
    }

    #[test]
    fn defaults_sit_mid_range_of_iso_606() {
        let d = sprocket_dimensions(&iso_08b(20)).unwrap();
        let mid = |r: (f64, f64)| 0.5 * (r.0 + r.1);
        assert!((d.pitch_diameter - 0.0127 / (PI / 20.0).sin()).abs() < 1e-15);
        assert_eq!(d.seating_radius, mid(d.seating_radius_range));
        assert_eq!(d.flank_radius, mid(d.flank_radius_range));
        assert_eq!(d.tip_diameter, mid(d.tip_diameter_range));
        assert_eq!(d.seating_angle_deg, mid(d.seating_angle_deg_range));
        // The ranges are the standard's, in metres: ri ∈ [4.298, 4.439] mm
        // for d1 = 8.51 mm (0.069·∛8.51 = 0.1408 mm).
        assert!((d.seating_radius_range.0 - 4.29755e-3).abs() < 1e-8);
        assert!((d.seating_radius_range.1 - 4.43842e-3).abs() < 1e-8);
        // re ∈ [0.12·d1·22, 0.008·d1·580] = [22.466, 39.486] mm.
        assert!((d.flank_radius_range.0 - 22.4664e-3).abs() < 1e-7);
        assert!((d.flank_radius_range.1 - 39.4864e-3).abs() < 1e-7);
        // α ∈ [115.5°, 135.5°].
        assert!((d.seating_angle_deg_range.0 - 115.5).abs() < 1e-12);
        assert!((d.seating_angle_deg_range.1 - 135.5).abs() < 1e-12);
        assert!(d.root_diameter < d.pitch_diameter && d.pitch_diameter < d.tip_diameter);
        assert!(d.bottom_diameter < d.root_diameter);
    }

    #[test]
    fn profile_is_one_loop_of_four_arcs_per_tooth() {
        let r = generate_sprocket_profile(&iso_08b(20)).unwrap();
        let arcs = r
            .entities
            .iter()
            .filter(|e| matches!(e, SketchEntity::Arc { .. }))
            .count();
        let points = r
            .entities
            .iter()
            .filter(|e| matches!(e, SketchEntity::Point { .. }))
            .count();
        assert_eq!(arcs, 80);
        assert_eq!(points, 1 + 7 * 20);
        assert_eq!(r.profiles.len(), 1, "exactly one closed profile");
        let p = &r.profiles[0];
        assert!(p.is_outer);
        assert_eq!(p.entity_ids.len(), 80);
        assert_eq!(p.arc_segments.len(), 80, "every edge is an exact arc");
        assert!(p.circle.is_none() && p.spline_segments.is_empty());
        // The first entity is the centre point.
        assert!(matches!(r.entities[0], SketchEntity::Point { id: 1, .. }));
    }

    /// Spec B3 oracle: a roller of diameter d1 centred on the pitch circle at
    /// every tooth clears the profile by ≥ 0 and ≤ the seating clearance;
    /// tooth count and pitch round-trip from the expanded geometry.
    #[test]
    fn roller_clearance_and_pitch_round_trip() {
        for &(p_mm, d1_mm) in CHAINS {
            for z in [MIN_TOOTH_COUNT, 7, 9, 11, 16, 20, 34, 52, 120] {
                let params = SprocketParams {
                    tooth_count: z,
                    pitch: p_mm * 1e-3,
                    roller_diameter: d1_mm * 1e-3,
                    center_x: 0.003,
                    center_y: -0.002,
                    rotation_offset: 0.4,
                    ..Default::default()
                };
                let r = match generate_sprocket_profile(&params) {
                    Ok(r) => r,
                    Err(e) => panic!("z={z} p={p_mm} d1={d1_mm}: {e}"),
                };
                let dims = r.dimensions;
                let pts = boundary(&r);
                assert!(pts.len() > 4 * z as usize);

                // Seating centres are the arc centres of the seating arcs:
                // the second emitted point of each gap.
                let seats: Vec<(f64, f64)> = (0..z).map(|k| r.positions[&(2 + 7 * k)]).collect();
                assert_eq!(seats.len(), z as usize);
                for (k, &c) in seats.iter().enumerate() {
                    // On the pitch circle.
                    let rc = (c.0 - params.center_x).hypot(c.1 - params.center_y);
                    assert!(
                        (rc - 0.5 * dims.pitch_diameter).abs() < 1e-12,
                        "z={z}: seat {k} off the pitch circle"
                    );
                    // The roller clears the profile: nearest boundary point is
                    // at least the roller radius away, and at most the seating
                    // radius (it sits IN the seat). The polyline is a chord
                    // approximation of the arc, so allow the sagitta.
                    let dist = polyline_distance(&pts, c);
                    let sagitta = dims.seating_radius
                        * (1.0 - (0.5 * dims.seating_angle_deg.to_radians() / 15.0).cos());
                    assert!(
                        dist >= 0.5 * d1_mm * 1e-3 - sagitta - 1e-12,
                        "z={z} p={p_mm}: roller {k} intersects the profile ({dist} < {})",
                        0.5 * d1_mm * 1e-3
                    );
                    assert!(
                        dist <= dims.seating_radius + 1e-12,
                        "z={z} p={p_mm}: roller {k} is not seated ({dist} > {})",
                        dims.seating_radius
                    );
                    // Pitch round-trips as the chord between neighbouring seats.
                    let n = seats[(k + 1) % z as usize];
                    let chord = (n.0 - c.0).hypot(n.1 - c.1);
                    assert!(
                        (chord - p_mm * 1e-3).abs() < 1e-12,
                        "z={z}: chord {chord} ≠ pitch {}",
                        p_mm * 1e-3
                    );
                }

                // Tooth count round-trips as the number of tip arcs, which are
                // the arcs centred on the sprocket centre.
                let tips = r
                    .entities
                    .iter()
                    .filter(|e| matches!(e, SketchEntity::Arc { center_id: 1, .. }))
                    .count();
                assert_eq!(tips, z as usize);

                // The boundary is CCW and its area lies between the bottom
                // and tip discs.
                let area = signed_area(&pts);
                let bottom = PI * (0.5 * dims.bottom_diameter).powi(2);
                let tip = PI * (0.5 * dims.tip_diameter).powi(2);
                assert!(
                    area > bottom && area < tip,
                    "z={z}: area {area} ∉ ({bottom}, {tip})"
                );

                // Every boundary point lies within [bottom, tip] radius.
                for &(x, y) in &pts {
                    let rr = (x - params.center_x).hypot(y - params.center_y);
                    assert!(rr >= 0.5 * dims.bottom_diameter - 1e-12);
                    assert!(rr <= 0.5 * dims.tip_diameter + 1e-12);
                }
            }
        }
    }

    #[test]
    fn arcs_are_tangent_where_they_meet() {
        let r = generate_sprocket_profile(&iso_08b(16)).unwrap();
        let dims = r.dimensions;
        // Gap 0: seat centre id 2, seat_plus 3, flank_center_plus 5.
        let c = r.positions[&2];
        let p = r.positions[&3];
        let f = r.positions[&5];
        // C, P and C' are collinear with |CP| = ri and |PC'| = re.
        assert!(((p.0 - c.0).hypot(p.1 - c.1) - dims.seating_radius).abs() < 1e-15);
        assert!(((f.0 - p.0).hypot(f.1 - p.1) - dims.flank_radius).abs() < 1e-15);
        let cross = (p.0 - c.0) * (f.1 - c.1) - (p.1 - c.1) * (f.0 - c.0);
        assert!(
            cross.abs() < 1e-15,
            "flank centre off the seat ray: {cross}"
        );
        // The tip point is on both the flank circle and the tip circle.
        let t = r.positions[&7];
        assert!(((t.0 - f.0).hypot(t.1 - f.1) - dims.flank_radius).abs() < 1e-15);
        assert!((t.0.hypot(t.1) - 0.5 * dims.tip_diameter).abs() < 1e-15);
    }

    #[test]
    fn rotation_and_center_move_the_profile_rigidly() {
        let base = generate_sprocket_profile(&iso_08b(12)).unwrap();
        let moved = generate_sprocket_profile(&SprocketParams {
            center_x: 0.05,
            center_y: -0.01,
            rotation_offset: 0.3,
            ..iso_08b(12)
        })
        .unwrap();
        assert_eq!(base.entities.len(), moved.entities.len());
        let (ca, sa) = (0.3f64.cos(), 0.3f64.sin());
        for (id, &(x, y)) in &base.positions {
            let (mx, my) = moved.positions[id];
            let ex = 0.05 + x * ca - y * sa;
            let ey = -0.01 + x * sa + y * ca;
            assert!((mx - ex).abs() < 1e-15 && (my - ey).abs() < 1e-15);
        }
    }

    #[test]
    fn preview_polyline_is_the_closed_boundary() {
        let r = generate_sprocket_profile(&iso_08b(9)).unwrap();
        let poly = generate_sprocket_preview_polyline(&iso_08b(9)).unwrap();
        assert_eq!(poly.len(), r.profiles[0].vertex_ids.len() + 1);
        assert_eq!(poly.first(), poly.last());
    }

    #[test]
    fn deterministic() {
        let a = generate_sprocket_profile(&iso_08b(31)).unwrap();
        let b = generate_sprocket_profile(&iso_08b(31)).unwrap();
        assert_eq!(
            serde_json::to_string(&a.entities).unwrap(),
            serde_json::to_string(&b.entities).unwrap()
        );
        assert_eq!(
            serde_json::to_string(&a.profiles).unwrap(),
            serde_json::to_string(&b.profiles).unwrap()
        );
        let sorted = |m: &HashMap<u32, (f64, f64)>| {
            let mut v: Vec<_> = m.iter().map(|(k, v)| (*k, *v)).collect();
            v.sort_by_key(|(k, _)| *k);
            v
        };
        assert_eq!(sorted(&a.positions), sorted(&b.positions));
    }

    #[test]
    fn overrides_are_honoured() {
        let r = generate_sprocket_profile(&SprocketParams {
            seating_radius: Some(0.0044),
            flank_radius: Some(0.03),
            tip_diameter: Some(0.085),
            seating_angle_deg: Some(120.0),
            ..iso_08b(20)
        })
        .unwrap();
        assert_eq!(r.dimensions.seating_radius, 0.0044);
        assert_eq!(r.dimensions.flank_radius, 0.03);
        assert_eq!(r.dimensions.tip_diameter, 0.085);
        assert_eq!(r.dimensions.seating_angle_deg, 120.0);
    }

    #[test]
    fn every_failure_is_typed() {
        assert_eq!(
            generate_sprocket_profile(&iso_08b(4)).unwrap_err(),
            SprocketError::TooFewTeeth {
                tooth_count: 4,
                min: MIN_TOOTH_COUNT
            }
        );
        assert!(matches!(
            generate_sprocket_profile(&SprocketParams {
                pitch: 0.0,
                ..iso_08b(20)
            })
            .unwrap_err(),
            SprocketError::BadValue { ref field, .. } if field == "pitch"
        ));
        assert!(matches!(
            generate_sprocket_profile(&SprocketParams {
                roller_diameter: f64::NAN,
                ..iso_08b(20)
            })
            .unwrap_err(),
            SprocketError::BadValue { ref field, .. } if field == "roller_diameter"
        ));
        assert!(matches!(
            generate_sprocket_profile(&SprocketParams {
                seating_radius: Some(0.004),
                ..iso_08b(20)
            })
            .unwrap_err(),
            SprocketError::SeatSmallerThanRoller { .. }
        ));
        assert!(matches!(
            generate_sprocket_profile(&SprocketParams {
                seating_angle_deg: Some(180.0),
                ..iso_08b(20)
            })
            .unwrap_err(),
            SprocketError::SeatingAngleTooLarge { .. }
        ));
        // A tip circle the flanks cannot reach.
        assert!(matches!(
            generate_sprocket_profile(&SprocketParams {
                tip_diameter: Some(1.0),
                ..iso_08b(20)
            })
            .unwrap_err(),
            SprocketError::TipUnreachable { .. }
        ));
        // A tip circle so large the flanks cross first: the error names the
        // largest tip diameter that still leaves a tip arc, and that bound
        // generates.
        let err = generate_sprocket_profile(&SprocketParams {
            tip_diameter: Some(0.095),
            ..iso_08b(20)
        })
        .unwrap_err();
        let SprocketError::FlanksCross {
            max_tip_diameter, ..
        } = err
        else {
            panic!("expected FlanksCross, got {err:?}");
        };
        assert!(max_tip_diameter > 0.08 && max_tip_diameter < 0.095);
        assert!(generate_sprocket_profile(&SprocketParams {
            tip_diameter: Some(max_tip_diameter * (1.0 - 1e-9)),
            ..iso_08b(20)
        })
        .is_ok());
    }

    #[test]
    fn params_serde_is_camel_case_with_defaults() {
        let json = serde_json::to_string(&iso_08b(11)).unwrap();
        assert!(json.contains(r#""toothCount":11"#));
        assert!(json.contains(r#""rollerDiameter":0.00851"#));
        assert!(json.contains(r#""standard":"Iso606""#));
        assert!(!json.contains("seatingRadius"));
        let back: SprocketParams =
            serde_json::from_str(r#"{"toothCount":9,"pitch":0.0127,"rollerDiameter":0.00851}"#)
                .unwrap();
        assert_eq!(back.standard, SprocketStandard::Iso606);
        assert_eq!(back.seating_radius, None);
        assert_eq!(back.rotation_offset, 0.0);
    }
}
