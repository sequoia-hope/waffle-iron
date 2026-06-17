//! Involute gear profile geometry generation.
//!
//! Port of `app/src/lib/sketch/gearGeometry.js` — pure math, no framework dependencies.
//!
//! Per tooth profile:
//!   root_left → line(rootR→baseR) → left_involute(baseR→addendumR) →
//!   tip_arc(addendumR) → right_involute(addendumR→baseR) → line(baseR→rootR)
//!   → root_right
//! Between teeth:
//!   root_right → root_arc(rootR) → next_root_left

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::bspline::fit_bspline_to_points;
use crate::{ClosedProfile, SketchEntity, SplineSegment};

/// Parameters for generating an involute gear profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GearParams {
    pub tooth_count: u32,
    pub module: f64,
    #[serde(default = "default_pressure_angle")]
    pub pressure_angle_deg: f64,
    #[serde(default)]
    pub backlash: f64,
    #[serde(default)]
    pub center_x: f64,
    #[serde(default)]
    pub center_y: f64,
    #[serde(default)]
    pub rotation_offset: f64,
    /// Internal (ring) gear: teeth point INWARD. The profile is the toothed
    /// inner boundary (a hole), to be combined with a user-drawn outer rim.
    /// Standard offsets: tip (inner) at `pitch − module`, body/root (outer) at
    /// `pitch + 1.25·module`; tooth = the conjugate of a meshing external gear's
    /// space (`half_tooth_angle = angular_pitch/4 − inv α`).
    #[serde(default)]
    pub internal: bool,
}

fn default_pressure_angle() -> f64 {
    20.0
}

impl Default for GearParams {
    fn default() -> Self {
        Self {
            tooth_count: 20,
            module: 0.002,
            pressure_angle_deg: 20.0,
            backlash: 0.0,
            center_x: 0.0,
            center_y: 0.0,
            rotation_offset: 0.0,
            internal: false,
        }
    }
}

/// Result of generating a full gear profile with sketch entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GearProfileResult {
    pub entities: Vec<SketchEntity>,
    #[serde(with = "crate::sketch::u32_key_map")]
    pub positions: HashMap<u32, (f64, f64)>,
    pub profiles: Vec<ClosedProfile>,
    pub pitch_radius: f64,
    pub base_radius: f64,
    pub addendum_radius: f64,
    pub dedendum_radius: f64,
}

/// Involute function: inv(α) = tan(α) - α
fn involute(alpha: f64) -> f64 {
    alpha.tan() - alpha
}

/// Compute a point on the involute curve of a base circle.
fn involute_point(base_radius: f64, roll_angle: f64) -> (f64, f64) {
    let x = base_radius * (roll_angle.cos() + roll_angle * roll_angle.sin());
    let y = base_radius * (roll_angle.sin() - roll_angle * roll_angle.cos());
    (x, y)
}

/// Compute standard gear radii from parameters.
fn gear_radii(params: &GearParams) -> (f64, f64, f64, f64, f64) {
    let alpha = params.pressure_angle_deg.to_radians();
    let pitch_r = (params.tooth_count as f64) * params.module / 2.0;
    let base_r = pitch_r * alpha.cos();
    let addendum_r = pitch_r + params.module;
    let dedendum_r = pitch_r - 1.25 * params.module;
    let root_r = dedendum_r.max(base_r * 0.5);
    (pitch_r, base_r, addendum_r, dedendum_r, root_r)
}

/// Generate a complete involute gear profile with sketch entities.
///
/// Produces Points, Splines, Lines, and Arcs matching the JS implementation exactly.
pub fn generate_gear_profile(params: &GearParams) -> GearProfileResult {
    if params.internal {
        return generate_internal_gear_profile(params);
    }
    let n = params.tooth_count;
    let (pitch_r, base_r, addendum_r, dedendum_r, root_r) = gear_radii(params);

    let angular_pitch = std::f64::consts::TAU / n as f64;
    let alpha = params.pressure_angle_deg.to_radians();
    let inv_alpha = involute(alpha);
    let half_tooth_angle = angular_pitch / 4.0 + inv_alpha;
    let backlash_angle = params.backlash / (2.0 * pitch_r);

    let cx = params.center_x;
    let cy = params.center_y;
    let rot = params.rotation_offset;

    let transform = |x: f64, y: f64| -> (f64, f64) {
        let ca = rot.cos();
        let sa = rot.sin();
        (cx + x * ca - y * sa, cy + x * sa + y * ca)
    };

    let mut points: Vec<(f64, f64)> = Vec::new();
    let mut entities: Vec<SketchEntity> = Vec::new();
    let mut positions: HashMap<u32, (f64, f64)> = HashMap::new();

    // Spline and line data for building ClosedProfile
    let mut profile_entity_ids: Vec<u32> = Vec::new();
    let mut profile_vertex_ids: Vec<u32> = Vec::new();
    let mut spline_segments: Vec<SplineSegment> = Vec::new();

    let mut next_id = 1u32;

    let add_point = |points: &mut Vec<(f64, f64)>,
                     entities: &mut Vec<SketchEntity>,
                     positions: &mut HashMap<u32, (f64, f64)>,
                     next_id: &mut u32,
                     x: f64,
                     y: f64|
     -> u32 {
        let id = *next_id;
        *next_id += 1;
        points.push((x, y));
        entities.push(SketchEntity::Point {
            id,
            x,
            y,
            construction: false,
        });
        positions.insert(id, (x, y));
        id
    };

    // Single center point for all arcs
    let center_pt = transform(0.0, 0.0);
    let center_idx = add_point(
        &mut points,
        &mut entities,
        &mut positions,
        &mut next_id,
        center_pt.0,
        center_pt.1,
    );

    let max_roll_angle = ((addendum_r / base_r).powi(2) - 1.0).sqrt();
    let num_inv_samples: usize = 12;

    // Pre-create root points for each tooth
    let mut tooth_right_root_idx = Vec::with_capacity(n as usize);
    let mut tooth_left_root_idx = Vec::with_capacity(n as usize);

    for tooth in 0..n {
        let tooth_angle = tooth as f64 * angular_pitch;
        let right_angle = tooth_angle + half_tooth_angle - backlash_angle;
        let left_angle = tooth_angle - half_tooth_angle + backlash_angle;

        let rr = transform(root_r * right_angle.cos(), root_r * right_angle.sin());
        let rr_id = add_point(
            &mut points,
            &mut entities,
            &mut positions,
            &mut next_id,
            rr.0,
            rr.1,
        );
        tooth_right_root_idx.push(rr_id);

        let lr = transform(root_r * left_angle.cos(), root_r * left_angle.sin());
        let lr_id = add_point(
            &mut points,
            &mut entities,
            &mut positions,
            &mut next_id,
            lr.0,
            lr.1,
        );
        tooth_left_root_idx.push(lr_id);
    }

    for tooth in 0..n {
        let tooth_angle = tooth as f64 * angular_pitch;
        let right_start_angle = tooth_angle + half_tooth_angle - backlash_angle;
        let left_start_angle = tooth_angle - half_tooth_angle + backlash_angle;

        // === Left involute points ===
        let mut left_involute_pts = Vec::with_capacity(num_inv_samples + 1);
        for i in 0..=num_inv_samples {
            let t = i as f64 / num_inv_samples as f64;
            let roll = t * max_roll_angle;
            let pt = involute_point(base_r, roll);
            let inv_angle = pt.1.atan2(pt.0);
            let r = (pt.0 * pt.0 + pt.1 * pt.1).sqrt();
            let adjusted_angle = left_start_angle + inv_angle;
            left_involute_pts.push(transform(
                r * adjusted_angle.cos(),
                r * adjusted_angle.sin(),
            ));
        }

        // Left base point
        let left_base_id = add_point(
            &mut points,
            &mut entities,
            &mut positions,
            &mut next_id,
            left_involute_pts[0].0,
            left_involute_pts[0].1,
        );

        // Radial line: root → base (left side)
        let line_id = next_id;
        next_id += 1;
        entities.push(SketchEntity::Line {
            id: line_id,
            start_id: tooth_left_root_idx[tooth as usize],
            end_id: left_base_id,
            construction: false,
        });
        profile_entity_ids.push(line_id);
        profile_vertex_ids.push(tooth_left_root_idx[tooth as usize]);

        // Left involute spline (baseR → addendumR)
        let mut left_mid_ids = Vec::with_capacity(num_inv_samples - 1);
        for &(px, py) in &left_involute_pts[1..left_involute_pts.len() - 1] {
            let mid_id = add_point(
                &mut points,
                &mut entities,
                &mut positions,
                &mut next_id,
                px,
                py,
            );
            left_mid_ids.push(mid_id);
        }
        let left_tip_id = add_point(
            &mut points,
            &mut entities,
            &mut positions,
            &mut next_id,
            left_involute_pts[left_involute_pts.len() - 1].0,
            left_involute_pts[left_involute_pts.len() - 1].1,
        );

        let left_spline_point_ids: Vec<u32> = std::iter::once(left_base_id)
            .chain(left_mid_ids.iter().copied())
            .chain(std::iter::once(left_tip_id))
            .collect();

        let left_ctrl = fit_bspline_to_points(&left_involute_pts, 3);

        let spline_id = next_id;
        next_id += 1;
        entities.push(SketchEntity::Spline {
            id: spline_id,
            point_ids: left_spline_point_ids,
            construction: false,
        });
        let spline_start_vertex_index = profile_vertex_ids.len();
        profile_entity_ids.push(spline_id);
        profile_vertex_ids.push(left_base_id);
        for &mid_id in &left_mid_ids {
            profile_vertex_ids.push(mid_id);
        }
        let spline_end_vertex_index = profile_vertex_ids.len();
        profile_vertex_ids.push(left_tip_id);

        spline_segments.push(SplineSegment {
            start_point_index: spline_start_vertex_index,
            end_point_index: spline_end_vertex_index,
            control_points: left_ctrl,
        });

        // === Right involute points (tip to base) ===
        let mut right_involute_pts = Vec::with_capacity(num_inv_samples + 1);
        for i in (0..=num_inv_samples).rev() {
            let t = i as f64 / num_inv_samples as f64;
            let roll = t * max_roll_angle;
            let pt = involute_point(base_r, roll);
            let inv_angle = pt.1.atan2(pt.0);
            let r = (pt.0 * pt.0 + pt.1 * pt.1).sqrt();
            let adjusted_angle = right_start_angle - inv_angle;
            right_involute_pts.push(transform(
                r * adjusted_angle.cos(),
                r * adjusted_angle.sin(),
            ));
        }

        // Tip arc: left tip → right tip
        let right_tip_id = add_point(
            &mut points,
            &mut entities,
            &mut positions,
            &mut next_id,
            right_involute_pts[0].0,
            right_involute_pts[0].1,
        );
        let arc_id = next_id;
        next_id += 1;
        entities.push(SketchEntity::Arc {
            id: arc_id,
            center_id: center_idx,
            start_id: left_tip_id,
            end_id: right_tip_id,
            construction: false,
        });
        profile_entity_ids.push(arc_id);

        // Right involute spline (addendumR → baseR)
        let mut right_mid_ids = Vec::with_capacity(num_inv_samples - 1);
        for &(px, py) in &right_involute_pts[1..right_involute_pts.len() - 1] {
            let mid_id = add_point(
                &mut points,
                &mut entities,
                &mut positions,
                &mut next_id,
                px,
                py,
            );
            right_mid_ids.push(mid_id);
        }
        let right_base_id = add_point(
            &mut points,
            &mut entities,
            &mut positions,
            &mut next_id,
            right_involute_pts[right_involute_pts.len() - 1].0,
            right_involute_pts[right_involute_pts.len() - 1].1,
        );

        let right_spline_point_ids: Vec<u32> = std::iter::once(right_tip_id)
            .chain(right_mid_ids.iter().copied())
            .chain(std::iter::once(right_base_id))
            .collect();

        let right_ctrl = fit_bspline_to_points(&right_involute_pts, 3);

        let spline_id = next_id;
        next_id += 1;
        entities.push(SketchEntity::Spline {
            id: spline_id,
            point_ids: right_spline_point_ids,
            construction: false,
        });
        let spline_start_vertex_index = profile_vertex_ids.len();
        profile_entity_ids.push(spline_id);
        profile_vertex_ids.push(right_tip_id);
        for &mid_id in &right_mid_ids {
            profile_vertex_ids.push(mid_id);
        }
        let spline_end_vertex_index = profile_vertex_ids.len();
        profile_vertex_ids.push(right_base_id);

        spline_segments.push(SplineSegment {
            start_point_index: spline_start_vertex_index,
            end_point_index: spline_end_vertex_index,
            control_points: right_ctrl,
        });

        // Radial line: base → root (right side)
        let line_id = next_id;
        next_id += 1;
        entities.push(SketchEntity::Line {
            id: line_id,
            start_id: right_base_id,
            end_id: tooth_right_root_idx[tooth as usize],
            construction: false,
        });
        profile_entity_ids.push(line_id);

        // Root arc to next tooth
        let next_tooth = ((tooth + 1) % n) as usize;
        let arc_id = next_id;
        next_id += 1;
        entities.push(SketchEntity::Arc {
            id: arc_id,
            center_id: center_idx,
            start_id: tooth_right_root_idx[tooth as usize],
            end_id: tooth_left_root_idx[next_tooth],
            construction: false,
        });
        profile_entity_ids.push(arc_id);
        profile_vertex_ids.push(tooth_right_root_idx[tooth as usize]);
    }

    let profiles = vec![ClosedProfile {
        entity_ids: profile_entity_ids,
        is_outer: true,
        vertex_ids: profile_vertex_ids,
        circle: None,
        spline_segments,
        arc_segments: vec![],
    }];

    GearProfileResult {
        entities,
        positions,
        profiles,
        pitch_radius: pitch_r,
        base_radius: base_r,
        addendum_radius: addendum_r,
        dedendum_radius: dedendum_r,
    }
}

/// Generate a TRUE internal (ring) gear profile — teeth point inward.
///
/// Same involute flanks as an external gear of the same `(N, module, α)`, but:
/// tip (inner) at `pitch − module`, body/root (outer) at `pitch + 1.25·module`
/// (offsets swap across the pitch circle), and the tooth is the conjugate of a
/// meshing external gear's SPACE, so `half_tooth_angle = angular_pitch/4 − inv α`
/// (external uses `+`). Emitted as a sampled closed polyline (the extrude path
/// consumes `vertex_ids`, like the arc/spline gears), flagged `is_outer = false`
/// (a hole) — combine with a user-drawn outer rim and extrude (KV14 assembles
/// the annulus). Sampling chords are the same fidelity the viewport/solver use.
fn generate_internal_gear_profile(params: &GearParams) -> GearProfileResult {
    let n = params.tooth_count;
    let alpha = params.pressure_angle_deg.to_radians();
    let pitch_r = (n as f64) * params.module / 2.0;
    let base_r = pitch_r * alpha.cos();
    let tip_r = pitch_r - params.module; // inner tip of the inward tooth
    let body_r = pitch_r + 1.25 * params.module; // outer (ring inner wall between teeth)

    let angular_pitch = std::f64::consts::TAU / n as f64;
    let inv_alpha = involute(alpha);
    let half = angular_pitch / 4.0 - inv_alpha; // conjugate of the external space
    let backlash_angle = params.backlash / (2.0 * pitch_r);
    let (cx, cy, rot) = (params.center_x, params.center_y, params.rotation_offset);
    let transform = |x: f64, y: f64| -> (f64, f64) {
        let (ca, sa) = (rot.cos(), rot.sin());
        (cx + x * ca - y * sa, cy + x * sa + y * ca)
    };

    let roll_max = ((body_r / base_r).powi(2) - 1.0).max(0.0).sqrt();
    let tip_above = tip_r >= base_r;
    let roll_min = if tip_above {
        ((tip_r / base_r).powi(2) - 1.0).max(0.0).sqrt()
    } else {
        0.0
    };
    // Involute polar angle at the flank's inner/outer ends, so the tip arc and
    // body arc start/end exactly where the flanks do (no backward jump).
    let inv_at = |roll: f64| {
        let p = involute_point(base_r, roll);
        p.1.atan2(p.0)
    };
    let inv_inner = inv_at(roll_min);
    let inv_outer = inv_at(roll_max);
    let nflank = 12usize;
    let narc = 4usize;

    // One involute flank, sampled inner→outer (base→body).
    let flank = |start: f64, sign: f64| -> Vec<(f64, f64)> {
        (0..=nflank)
            .map(|i| {
                let t = i as f64 / nflank as f64;
                let roll = roll_min + t * (roll_max - roll_min);
                let p = involute_point(base_r, roll);
                let r = (p.0 * p.0 + p.1 * p.1).sqrt();
                let a = start + sign * p.1.atan2(p.0);
                transform(r * a.cos(), r * a.sin())
            })
            .collect()
    };
    let arc = |r: f64, a0: f64, a1: f64| -> Vec<(f64, f64)> {
        (0..=narc)
            .map(|i| {
                let t = i as f64 / narc as f64;
                let a = a0 + t * (a1 - a0);
                transform(r * a.cos(), r * a.sin())
            })
            .collect()
    };

    // CCW boundary polyline, deduped at segment joins.
    let mut boundary: Vec<(f64, f64)> = Vec::new();
    fn push(b: &mut Vec<(f64, f64)>, p: (f64, f64)) {
        if b.last()
            .map_or(true, |&l| (l.0 - p.0).hypot(l.1 - p.1) > 1e-12)
        {
            b.push(p);
        }
    }
    for tooth in 0..n {
        let ta = tooth as f64 * angular_pitch;
        let ls = ta - half + backlash_angle;
        let rs = ta + half - backlash_angle;
        // Internal teeth are the CONJUGATE of the external space: flanks DIVERGE
        // outward (tooth narrows toward the inner tip), so the involute sign is
        // opposite the external gear's (left −, right +).
        // left flank body(outer) → base/inner (reverse of inner→outer samples)
        for &p in flank(ls, -1.0).iter().rev() {
            push(&mut boundary, p);
        }
        if !tip_above {
            push(&mut boundary, transform(tip_r * ls.cos(), tip_r * ls.sin()));
        }
        // tip arc (inner): left tip (ls − inv_inner) → right tip (rs + inv_inner)
        for &p in arc(tip_r, ls - inv_inner, rs + inv_inner).iter() {
            push(&mut boundary, p);
        }
        // right flank base/inner → body(outer)
        for &p in flank(rs, 1.0).iter() {
            push(&mut boundary, p);
        }
        // body arc (outer): right flank end (rs + inv_outer) → next left flank
        // start (next_ls − inv_outer)
        let next_ls = (tooth as f64 + 1.0) * angular_pitch - half + backlash_angle;
        for &p in arc(body_r, rs + inv_outer, next_ls - inv_outer).iter() {
            push(&mut boundary, p);
        }
    }
    // close: drop a final point coincident with the first
    if boundary.len() > 1 {
        let f = boundary[0];
        if (boundary.last().unwrap().0 - f.0).hypot(boundary.last().unwrap().1 - f.1) < 1e-12 {
            boundary.pop();
        }
    }

    let mut entities = Vec::new();
    let mut positions = HashMap::new();
    let mut id = 1u32;
    // Center point FIRST, matching the external gear's contract (entity[0] is
    // the gear center). The JS `createGear` anchors the construction pitch
    // circle on `entityIds[0]`; without an explicit center here that landed on
    // the first boundary vertex, drawing the pitch circle offset from the gear.
    // The center is not part of `vertex_ids`, so it never enters the profile.
    let center = transform(0.0, 0.0);
    entities.push(SketchEntity::Point {
        id,
        x: center.0,
        y: center.1,
        construction: false,
    });
    positions.insert(id, center);
    id += 1;
    let mut point_ids = Vec::with_capacity(boundary.len());
    for &(x, y) in &boundary {
        entities.push(SketchEntity::Point {
            id,
            x,
            y,
            construction: false,
        });
        positions.insert(id, (x, y));
        point_ids.push(id);
        id += 1;
    }
    let mut entity_ids = Vec::with_capacity(point_ids.len());
    for i in 0..point_ids.len() {
        let s = point_ids[i];
        let e = point_ids[(i + 1) % point_ids.len()];
        entities.push(SketchEntity::Line {
            id,
            start_id: s,
            end_id: e,
            construction: false,
        });
        entity_ids.push(id);
        id += 1;
    }

    let profiles = vec![ClosedProfile {
        entity_ids,
        is_outer: false,
        vertex_ids: point_ids,
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    }];
    GearProfileResult {
        entities,
        positions,
        profiles,
        pitch_radius: pitch_r,
        base_radius: base_r,
        addendum_radius: tip_r,
        dedendum_radius: body_r,
    }
}

/// Generate a flat polyline approximation of a gear profile for live preview.
pub fn generate_gear_preview_polyline(params: &GearParams) -> Vec<(f64, f64)> {
    if params.internal {
        // The internal profile's vertex_ids are already the boundary polyline.
        let g = generate_internal_gear_profile(params);
        return g.profiles[0]
            .vertex_ids
            .iter()
            .map(|id| g.positions[id])
            .collect();
    }
    let n = params.tooth_count;
    let (_, base_r, addendum_r, _, root_r) = gear_radii(params);

    let angular_pitch = std::f64::consts::TAU / n as f64;
    let alpha = params.pressure_angle_deg.to_radians();
    let inv_alpha = involute(alpha);
    let half_tooth_angle = angular_pitch / 4.0 + inv_alpha;
    let backlash_angle = params.backlash / (2.0 * (n as f64 * params.module / 2.0));

    let cx = params.center_x;
    let cy = params.center_y;
    let rot = params.rotation_offset;

    let transform = |x: f64, y: f64| -> (f64, f64) {
        let ca = rot.cos();
        let sa = rot.sin();
        (cx + x * ca - y * sa, cy + x * sa + y * ca)
    };

    let mut polyline = Vec::new();
    let samples_per_involute = 8;
    let max_roll_angle = ((addendum_r / base_r).powi(2) - 1.0).sqrt();

    for tooth in 0..n {
        let tooth_angle = tooth as f64 * angular_pitch;
        let right_start_angle = tooth_angle + half_tooth_angle - backlash_angle;
        let left_start_angle = tooth_angle - half_tooth_angle + backlash_angle;

        // Left root point
        polyline.push(transform(
            root_r * left_start_angle.cos(),
            root_r * left_start_angle.sin(),
        ));

        // Left involute (base to tip)
        for i in 0..=samples_per_involute {
            let t = i as f64 / samples_per_involute as f64;
            let roll = t * max_roll_angle;
            let pt = involute_point(base_r, roll);
            let inv_angle = pt.1.atan2(pt.0);
            let r = (pt.0 * pt.0 + pt.1 * pt.1).sqrt();
            let adjusted_angle = left_start_angle + inv_angle;
            polyline.push(transform(
                r * adjusted_angle.cos(),
                r * adjusted_angle.sin(),
            ));
        }

        // Right involute (tip to base)
        for i in (0..=samples_per_involute).rev() {
            let t = i as f64 / samples_per_involute as f64;
            let roll = t * max_roll_angle;
            let pt = involute_point(base_r, roll);
            let inv_angle = pt.1.atan2(pt.0);
            let r = (pt.0 * pt.0 + pt.1 * pt.1).sqrt();
            let adjusted_angle = right_start_angle - inv_angle;
            polyline.push(transform(
                r * adjusted_angle.cos(),
                r * adjusted_angle.sin(),
            ));
        }

        // Right root point
        polyline.push(transform(
            root_r * right_start_angle.cos(),
            root_r * right_start_angle.sin(),
        ));
    }

    // Close the polyline
    if let Some(&first) = polyline.first() {
        polyline.push(first);
    }

    polyline
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn involute_zero_is_zero() {
        assert!((involute(0.0) - 0.0).abs() < 1e-15);
    }

    #[test]
    fn radii_for_20_tooth_module_2() {
        let params = GearParams {
            tooth_count: 20,
            module: 2.0,
            pressure_angle_deg: 20.0,
            ..Default::default()
        };
        let (pitch_r, base_r, addendum_r, dedendum_r, _root_r) = gear_radii(&params);
        assert!((pitch_r - 20.0).abs() < 1e-10);
        assert!((base_r - 20.0 * 20.0_f64.to_radians().cos()).abs() < 1e-10);
        assert!((addendum_r - 22.0).abs() < 1e-10);
        assert!((dedendum_r - 17.5).abs() < 1e-10);
    }

    #[test]
    fn profile_entity_counts() {
        let params = GearParams {
            tooth_count: 20,
            module: 2.0,
            pressure_angle_deg: 20.0,
            ..Default::default()
        };
        let result = generate_gear_profile(&params);

        let point_count = result
            .entities
            .iter()
            .filter(|e| matches!(e, SketchEntity::Point { .. }))
            .count();
        let spline_count = result
            .entities
            .iter()
            .filter(|e| matches!(e, SketchEntity::Spline { .. }))
            .count();
        let line_count = result
            .entities
            .iter()
            .filter(|e| matches!(e, SketchEntity::Line { .. }))
            .count();
        let arc_count = result
            .entities
            .iter()
            .filter(|e| matches!(e, SketchEntity::Arc { .. }))
            .count();

        // Per tooth: 1 center (shared) + 2 root points + 1 left base + 11 left mid + 1 left tip
        //          + 1 right tip + 11 right mid + 1 right base = 28 non-shared points per tooth
        // Total points: 1 (center) + 20*2 (root) + 20*26 (involute) = 1 + 40 + 520 = 561
        assert_eq!(
            point_count,
            1 + 20 * 2 + 20 * 26,
            "point count: 1 center + 20*2 root + 20*26 involute"
        );
        assert_eq!(spline_count, 40, "2 splines per tooth × 20 teeth");
        assert_eq!(line_count, 40, "2 lines per tooth × 20 teeth");
        assert_eq!(arc_count, 40, "2 arcs per tooth × 20 teeth");
    }

    #[test]
    fn profile_has_spline_segments() {
        let params = GearParams {
            tooth_count: 8,
            module: 1.0,
            pressure_angle_deg: 20.0,
            ..Default::default()
        };
        let result = generate_gear_profile(&params);
        assert_eq!(result.profiles.len(), 1);
        let profile = &result.profiles[0];
        assert!(profile.is_outer);
        // 2 spline segments per tooth
        assert_eq!(profile.spline_segments.len(), 16);
        // Each spline segment should have control points
        for seg in &profile.spline_segments {
            assert!(
                seg.control_points.len() >= 2,
                "spline segment should have control points"
            );
        }
    }

    #[test]
    fn preview_polyline_closes() {
        let params = GearParams {
            tooth_count: 10,
            module: 1.0,
            pressure_angle_deg: 20.0,
            ..Default::default()
        };
        let polyline = generate_gear_preview_polyline(&params);
        assert!(polyline.len() > 10);
        let first = polyline.first().unwrap();
        let last = polyline.last().unwrap();
        assert!(
            (first.0 - last.0).abs() < 1e-10 && (first.1 - last.1).abs() < 1e-10,
            "polyline should close"
        );
    }

    #[test]
    fn rotation_offset_rotates_profile() {
        let base = GearParams {
            tooth_count: 8,
            module: 1.0,
            ..Default::default()
        };
        let rotated = GearParams {
            rotation_offset: std::f64::consts::FRAC_PI_4,
            internal: false,
            ..base.clone()
        };
        let poly_base = generate_gear_preview_polyline(&base);
        let poly_rot = generate_gear_preview_polyline(&rotated);
        // Same number of points
        assert_eq!(poly_base.len(), poly_rot.len());
        // But different positions
        let (bx, by) = poly_base[1];
        let (rx, ry) = poly_rot[1];
        assert!(
            (bx - rx).abs() > 1e-6 || (by - ry).abs() > 1e-6,
            "rotation should change point positions"
        );
    }

    #[test]
    fn internal_gear_first_entity_is_center() {
        // Regression: internal gears must emit the gear center as entity[0],
        // matching the external gear contract. The JS layer anchors the
        // construction pitch circle on `entities[0]`; when that was a boundary
        // vertex the pitch circle drew offset from the gear (bug report).
        let params = GearParams {
            tooth_count: 12,
            module: 1.5,
            pressure_angle_deg: 20.0,
            center_x: 3.0,
            center_y: -2.0,
            internal: true,
            ..Default::default()
        };
        let result = generate_gear_profile(&params);
        let first = &result.entities[0];
        match first {
            SketchEntity::Point { id, x, y, .. } => {
                assert!(
                    (*x - 3.0).abs() < 1e-9 && (*y - (-2.0)).abs() < 1e-9,
                    "entity[0] must be the gear center at (center_x, center_y)"
                );
                // The center is an anchor only — never part of the profile loop.
                assert!(
                    !result.profiles[0].vertex_ids.contains(id),
                    "center point must not be in the boundary vertex_ids"
                );
            }
            _ => panic!("entity[0] must be the center Point"),
        }
    }

    #[test]
    fn spline_control_point_count() {
        let params = GearParams {
            tooth_count: 6,
            module: 1.0,
            ..Default::default()
        };
        let result = generate_gear_profile(&params);
        for seg in &result.profiles[0].spline_segments {
            // 13 sample points → 13 control points from fit
            assert_eq!(
                seg.control_points.len(),
                13,
                "each involute spline should have 13 control points"
            );
        }
    }
}
