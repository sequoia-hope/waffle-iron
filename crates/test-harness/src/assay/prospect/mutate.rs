//! Mutator — one knob turned on a document (`specs/assay_prospector.md` §5).
//!
//! Works on the `.waffle` JSON directly, so it applies to the corpus's
//! CORRECT cases, to the prospector's own candidates and to real user
//! documents alike. Exactly one knob per mutant, so every mutant has a
//! known-good sibling one edit away. The mutant's meta is re-derived from
//! the document by the caller (`derive_meta`), never copied from the parent.
//!
//! Sketch geometry lives twice in a document — `entities[].{x,y}` and
//! `solved_positions[id]` (the engine reads the latter for profiles) — and
//! a mutation moves both.

use serde_json::Value;

use super::gen3::Rng;

/// The knobs, each with the delta it applied — the lineage record.
#[derive(Debug, Clone, PartialEq)]
pub enum Knob {
    /// Move one sketch point by `delta` along u or v.
    Point {
        feature: usize,
        point_id: u64,
        axis: char,
        delta: f64,
    },
    /// Move a sketch plane origin by `delta` along its normal (`along_normal`)
    /// or along world x.
    PlaneOrigin {
        feature: usize,
        along_normal: bool,
        delta: f64,
    },
    /// Rotate a sketch plane normal by `degrees` about an in-plane axis.
    PlaneNormal { feature: usize, degrees: f64 },
    /// Scale a Blind extrude depth by `factor` (or add `delta`).
    Depth {
        feature: usize,
        factor: f64,
        delta: f64,
    },
    /// Add `delta` degrees to a revolve angle.
    Angle { feature: usize, delta: f64 },
    /// Uniform scale of every length by `factor`.
    Scale { factor: f64 },
    /// Toggle `symmetric` on an extrude.
    Symmetric { feature: usize },
}

impl Knob {
    pub fn label(&self) -> String {
        match self {
            Knob::Point {
                feature,
                point_id,
                axis,
                delta,
            } => format!("point f{feature} #{point_id} {axis}{delta:+.3e}"),
            Knob::PlaneOrigin {
                feature,
                along_normal,
                delta,
            } => format!(
                "origin f{feature} {}{delta:+.3e}",
                if *along_normal { "n" } else { "x" }
            ),
            Knob::PlaneNormal { feature, degrees } => format!("normal f{feature} {degrees:+.3}°"),
            Knob::Depth {
                feature,
                factor,
                delta,
            } => format!("depth f{feature} ×{factor:.6}{delta:+.3e}"),
            Knob::Angle { feature, delta } => format!("angle f{feature} {delta:+.3e}°"),
            Knob::Scale { factor } => format!("scale ×{factor:.0e}"),
            Knob::Symmetric { feature } => format!("symmetric f{feature}"),
        }
    }
}

fn features_mut(doc: &mut Value) -> Option<&mut Vec<Value>> {
    doc.get_mut("tabs")?
        .as_array_mut()?
        .first_mut()?
        .pointer_mut("/kind/features/features")?
        .as_array_mut()
}

fn features(doc: &Value) -> Option<&Vec<Value>> {
    doc.get("tabs")?
        .as_array()?
        .first()?
        .pointer("/kind/features/features")?
        .as_array()
}

fn vec3(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}

fn set_vec3(v: &mut Value, x: [f64; 3]) {
    *v = serde_json::json!([x[0], x[1], x[2]]);
}

/// The document's characteristic length: the largest |coordinate| over
/// sketch points, plane origins and Blind depths.
pub fn characteristic_length(doc: &Value) -> f64 {
    let mut m: f64 = 0.0;
    if let Some(feats) = features(doc) {
        for f in feats {
            let Some(op) = f.get("operation") else {
                continue;
            };
            if let Some(sk) = op.get("sketch") {
                if let Some(o) = sk.get("plane_origin").and_then(vec3) {
                    m = m.max(o.iter().map(|c| c.abs()).fold(0.0, f64::max));
                }
                if let Some(pos) = sk.get("solved_positions").and_then(Value::as_object) {
                    for p in pos.values() {
                        if let Some(a) = p.as_array() {
                            for c in a {
                                m = m.max(c.as_f64().unwrap_or(0.0).abs());
                            }
                        }
                    }
                }
                if let Some(profiles) = sk.get("solved_profiles").and_then(Value::as_array) {
                    for prof in profiles {
                        if let Some(c) = prof.get("circle") {
                            let r = c.get("radius").and_then(Value::as_f64).unwrap_or(0.0);
                            let cu = c.get("center_u").and_then(Value::as_f64).unwrap_or(0.0);
                            let cv = c.get("center_v").and_then(Value::as_f64).unwrap_or(0.0);
                            m = m.max(r + cu.abs().max(cv.abs()));
                        }
                    }
                }
                if let Some(ents) = sk.get("entities").and_then(Value::as_array) {
                    for e in ents {
                        for key in ["x", "y", "radius"] {
                            if let Some(v) = e.get(key).and_then(Value::as_f64) {
                                m = m.max(v.abs());
                            }
                        }
                        // A generator entity (Gear / Sprocket) carries its size
                        // in `params`: tip radius ≈ module·(teeth/2 + 1).
                        if let Some(p) = e.get("params") {
                            let module = p.get("module").and_then(Value::as_f64).unwrap_or(0.0);
                            let teeth = p.get("toothCount").and_then(Value::as_f64).unwrap_or(0.0);
                            m = m.max(module * (teeth / 2.0 + 1.0));
                            for key in ["centerX", "centerY", "pitch", "radius"] {
                                if let Some(v) = p.get(key).and_then(Value::as_f64) {
                                    m = m.max(v.abs());
                                }
                            }
                        }
                    }
                }
            }
            if op.get("type").and_then(Value::as_str) == Some("Extrude") {
                let blind = op
                    .pointer("/params/depth_mode/type")
                    .and_then(Value::as_str)
                    .is_none_or(|m| m == "Blind");
                if blind {
                    if let Some(d) = op.pointer("/params/depth").and_then(Value::as_f64) {
                        m = m.max(d.abs());
                    }
                }
            }
        }
    }
    m.max(1e-6)
}

/// The ε ladder of §5, as a fraction of the characteristic length, signed.
fn epsilon(rng: &mut Rng, scale: f64) -> f64 {
    let e = match rng.below(6) {
        0 => 1e-12,
        1 => 1e-9,
        2 => 1e-7, // TAU_MODEL
        3 => 1e-6, // MIN_FEATURE_SIZE
        4 => 1e-3,
        _ => 1e-1,
    };
    let sign = if rng.chance(0.5) { 1.0 } else { -1.0 };
    sign * e * scale
}

fn rotate_about(v: [f64; 3], axis: [f64; 3], angle: f64) -> [f64; 3] {
    // Rodrigues.
    let (s, c) = angle.sin_cos();
    let k = axis;
    let kv = [
        k[1] * v[2] - k[2] * v[1],
        k[2] * v[0] - k[0] * v[2],
        k[0] * v[1] - k[1] * v[0],
    ];
    let kd = k[0] * v[0] + k[1] * v[1] + k[2] * v[2];
    [
        v[0] * c + kv[0] * s + k[0] * kd * (1.0 - c),
        v[1] * c + kv[1] * s + k[1] * kd * (1.0 - c),
        v[2] * c + kv[2] * s + k[2] * kd * (1.0 - c),
    ]
}

fn any_in_plane_axis(n: [f64; 3]) -> [f64; 3] {
    let r = if n[2].abs() < 0.99 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = [
        r[1] * n[2] - r[2] * n[1],
        r[2] * n[0] - r[0] * n[2],
        r[0] * n[1] - r[1] * n[0],
    ];
    let l = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    [u[0] / l, u[1] / l, u[2] / l]
}

/// Sites a knob can act on, enumerated so a draw is uniform over them.
enum Site {
    Sketch(usize),
    Extrude(usize),
    Revolve(usize),
}

fn sites(doc: &Value) -> Vec<Site> {
    let mut out = Vec::new();
    if let Some(feats) = features(doc) {
        for (i, f) in feats.iter().enumerate() {
            match f.pointer("/operation/type").and_then(Value::as_str) {
                Some("Sketch") => out.push(Site::Sketch(i)),
                Some("Extrude") => out.push(Site::Extrude(i)),
                Some("Revolve") => out.push(Site::Revolve(i)),
                _ => {}
            }
        }
    }
    out
}

/// Turn one knob. Returns the mutant and the knob, or `None` when eight
/// draws in a row found nothing to turn (a document with no sketch point,
/// plane, depth or angle).
pub fn mutate(doc: &Value, rng: &mut Rng) -> Option<(Value, Knob)> {
    (0..8).find_map(|_| mutate_once(doc, rng))
}

fn mutate_once(doc: &Value, rng: &mut Rng) -> Option<(Value, Knob)> {
    let scale = characteristic_length(doc);
    // A uniform scale is document-wide; everything else picks a site.
    if rng.chance(0.08) {
        let factor = if rng.chance(0.5) { 1e-3 } else { 1e3 };
        return scale_document(doc, factor).map(|d| (d, Knob::Scale { factor }));
    }
    let sites = sites(doc);
    if sites.is_empty() {
        return None;
    }
    let site = &sites[rng.below(sites.len() as u64) as usize];
    let mut out = doc.clone();
    let feats = features_mut(&mut out)?;
    match *site {
        Site::Sketch(i) => {
            let sk = feats[i].pointer_mut("/operation/sketch")?;
            match rng.below(4) {
                0 => {
                    // Move one point in both representations.
                    let ids: Vec<u64> = match sk.get("solved_positions").and_then(Value::as_object)
                    {
                        Some(pos) => pos.keys().filter_map(|k| k.parse().ok()).collect(),
                        None => sk
                            .get("entities")?
                            .as_array()?
                            .iter()
                            .filter(|e| e.get("type").and_then(Value::as_str) == Some("Point"))
                            .filter_map(|e| e.get("id").and_then(Value::as_u64))
                            .collect(),
                    };
                    if ids.is_empty() {
                        return None;
                    }
                    let point_id = ids[rng.below(ids.len() as u64) as usize];
                    let axis = if rng.chance(0.5) { 'u' } else { 'v' };
                    let delta = epsilon(rng, scale);
                    let idx = if axis == 'u' { 0 } else { 1 };
                    if let Some(p) = sk
                        .get_mut("solved_positions")
                        .and_then(|m| m.get_mut(point_id.to_string()))
                        .and_then(Value::as_array_mut)
                    {
                        let v = p[idx].as_f64()? + delta;
                        p[idx] = Value::from(v);
                    }
                    if let Some(ents) = sk.get_mut("entities").and_then(Value::as_array_mut) {
                        for e in ents {
                            if e.get("type").and_then(Value::as_str) == Some("Point")
                                && e.get("id").and_then(Value::as_u64) == Some(point_id)
                            {
                                let key = if axis == 'u' { "x" } else { "y" };
                                let v = e.get(key).and_then(Value::as_f64)? + delta;
                                e[key] = Value::from(v);
                            }
                        }
                    }
                    Some((
                        out,
                        Knob::Point {
                            feature: i,
                            point_id,
                            axis,
                            delta,
                        },
                    ))
                }
                1 => {
                    let sketch_id = sk.get("id").and_then(Value::as_str).map(str::to_string);
                    let n = sk.get("plane_normal").and_then(vec3)?;
                    let mut o = sk.get("plane_origin").and_then(vec3)?;
                    let along_normal = rng.chance(0.7);
                    let delta = epsilon(rng, scale);
                    let shift = if along_normal {
                        [n[0] * delta, n[1] * delta, n[2] * delta]
                    } else {
                        [delta, 0.0, 0.0]
                    };
                    for k in 0..3 {
                        o[k] += shift[k];
                    }
                    set_vec3(sk.get_mut("plane_origin")?, o);
                    // A revolve of this sketch keeps its axis on the plane:
                    // its origin moves with the plane (else the mutant is an
                    // authored-invalid `RevolveAxisNotInPlane`).
                    if let Some(sid) = sketch_id {
                        for f in feats.iter_mut() {
                            if f.pointer("/operation/type").and_then(Value::as_str)
                                != Some("Revolve")
                                || f.pointer("/operation/params/sketch_id")
                                    .and_then(Value::as_str)
                                    != Some(sid.as_str())
                            {
                                continue;
                            }
                            let params = f.pointer_mut("/operation/params")?;
                            if let Some(ao) = params.get("axis_origin").and_then(vec3) {
                                set_vec3(
                                    params.get_mut("axis_origin")?,
                                    [ao[0] + shift[0], ao[1] + shift[1], ao[2] + shift[2]],
                                );
                            }
                        }
                    }
                    Some((
                        out,
                        Knob::PlaneOrigin {
                            feature: i,
                            along_normal,
                            delta,
                        },
                    ))
                }
                2 => {
                    let sketch_id = sk.get("id").and_then(Value::as_str).map(str::to_string);
                    let origin = sk.get("plane_origin").and_then(vec3)?;
                    let n = sk.get("plane_normal").and_then(vec3)?;
                    let degrees: f64 = match rng.below(5) {
                        0 => 1e-7,
                        1 => 1e-3,
                        2 => 0.5,
                        3 => 2.8,
                        _ => 45.0,
                    } * if rng.chance(0.5) { 1.0 } else { -1.0 };
                    let axis = any_in_plane_axis(n);
                    let n2 = rotate_about(n, axis, degrees.to_radians());
                    set_vec3(sk.get_mut("plane_normal")?, n2);
                    if let Some(x) = sk.get("plane_x_axis").and_then(vec3) {
                        let x2 = rotate_about(x, axis, degrees.to_radians());
                        set_vec3(sk.get_mut("plane_x_axis")?, x2);
                    }
                    // A revolve of this sketch keeps its axis IN the plane:
                    // rotate the axis direction with the normal and its
                    // origin about the plane origin (else the mutant is an
                    // authored-invalid `RevolveAxisNotInPlane`, not a finding).
                    if let Some(sid) = sketch_id {
                        for f in feats.iter_mut() {
                            if f.pointer("/operation/type").and_then(Value::as_str)
                                != Some("Revolve")
                                || f.pointer("/operation/params/sketch_id")
                                    .and_then(Value::as_str)
                                    != Some(sid.as_str())
                            {
                                continue;
                            }
                            let params = f.pointer_mut("/operation/params")?;
                            if let Some(d) = params.get("axis_direction").and_then(vec3) {
                                set_vec3(
                                    params.get_mut("axis_direction")?,
                                    rotate_about(d, axis, degrees.to_radians()),
                                );
                            }
                            if let Some(o) = params.get("axis_origin").and_then(vec3) {
                                let rel = [o[0] - origin[0], o[1] - origin[1], o[2] - origin[2]];
                                let r = rotate_about(rel, axis, degrees.to_radians());
                                set_vec3(
                                    params.get_mut("axis_origin")?,
                                    [origin[0] + r[0], origin[1] + r[1], origin[2] + r[2]],
                                );
                            }
                        }
                    }
                    Some((
                        out,
                        Knob::PlaneNormal {
                            feature: i,
                            degrees,
                        },
                    ))
                }
                _ => {
                    // Move every point of the sketch by the same ε (a rigid
                    // in-plane shift — the profile's relation to earlier
                    // bodies changes, its shape does not).
                    let delta = epsilon(rng, scale);
                    let axis = if rng.chance(0.5) { 'u' } else { 'v' };
                    let idx = if axis == 'u' { 0 } else { 1 };
                    if let Some(pos) = sk
                        .get_mut("solved_positions")
                        .and_then(Value::as_object_mut)
                    {
                        for p in pos.values_mut() {
                            if let Some(a) = p.as_array_mut() {
                                let v = a[idx].as_f64().unwrap_or(0.0) + delta;
                                a[idx] = Value::from(v);
                            }
                        }
                    }
                    if let Some(ents) = sk.get_mut("entities").and_then(Value::as_array_mut) {
                        let key = if axis == 'u' { "x" } else { "y" };
                        for e in ents {
                            if e.get("type").and_then(Value::as_str) == Some("Point") {
                                if let Some(v) = e.get(key).and_then(Value::as_f64) {
                                    e[key] = Value::from(v + delta);
                                }
                            }
                        }
                    }
                    Some((
                        out,
                        Knob::Point {
                            feature: i,
                            point_id: 0,
                            axis,
                            delta,
                        },
                    ))
                }
            }
        }
        Site::Extrude(i) => {
            let params = feats[i].pointer_mut("/operation/params")?;
            if rng.chance(0.2) {
                let cur = params
                    .get("symmetric")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                params["symmetric"] = Value::from(!cur);
                return Some((out, Knob::Symmetric { feature: i }));
            }
            let blind = params
                .pointer("/depth_mode/type")
                .and_then(Value::as_str)
                .is_none_or(|m| m == "Blind");
            if !blind {
                return None;
            }
            let d = params.get("depth").and_then(Value::as_f64)?;
            let (factor, delta) = if rng.chance(0.5) {
                (1.0, epsilon(rng, scale))
            } else {
                (1.0 + epsilon(rng, 1.0), 0.0)
            };
            params["depth"] = Value::from(d * factor + delta);
            Some((
                out,
                Knob::Depth {
                    feature: i,
                    factor,
                    delta,
                },
            ))
        }
        Site::Revolve(i) => {
            let params = feats[i].pointer_mut("/operation/params")?;
            let a = params.get("angle").and_then(Value::as_f64)?;
            let delta = match rng.below(4) {
                0 => 1e-7,
                1 => 1e-3,
                2 => 0.5,
                _ => 360.0 - a, // snap to the full turn
            } * if rng.chance(0.5) { 1.0 } else { -1.0 };
            // Stay inside the revolve's own contract (0, 360]: outside it the
            // mutant is authored-invalid (`RevolveInvalidAngle`), not a finding.
            let new_angle = a + delta;
            if !(new_angle > 0.0 && new_angle <= 360.0) {
                return None;
            }
            params["angle"] = Value::from(new_angle);
            Some((out, Knob::Angle { feature: i, delta }))
        }
    }
}

/// Uniformly scale every length in the document by `factor`.
pub fn scale_document(doc: &Value, factor: f64) -> Option<Value> {
    let mut out = doc.clone();
    let feats = features_mut(&mut out)?;
    for f in feats.iter_mut() {
        let Some(op) = f.get_mut("operation") else {
            continue;
        };
        let ty = op
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(sk) = op.get_mut("sketch") {
            if let Some(o) = sk.get("plane_origin").and_then(vec3) {
                set_vec3(
                    sk.get_mut("plane_origin")?,
                    [o[0] * factor, o[1] * factor, o[2] * factor],
                );
            }
            if let Some(pos) = sk
                .get_mut("solved_positions")
                .and_then(Value::as_object_mut)
            {
                for p in pos.values_mut() {
                    if let Some(a) = p.as_array_mut() {
                        for c in a.iter_mut() {
                            let v = c.as_f64().unwrap_or(0.0) * factor;
                            *c = Value::from(v);
                        }
                    }
                }
            }
            if let Some(ents) = sk.get_mut("entities").and_then(Value::as_array_mut) {
                for e in ents {
                    for key in ["x", "y", "radius"] {
                        if let Some(v) = e.get(key).and_then(Value::as_f64) {
                            e[key] = Value::from(v * factor);
                        }
                    }
                    // Generator entities (Gear / Sprocket): every LENGTH in
                    // `params` scales; counts and angles do not. (Unscaled
                    // gears made a ×1e-3 mutant of R0082 explode its bbox
                    // and one of R0018 put a revolve axis through its
                    // profile — mutator bugs, not kernel findings.)
                    if let Some(p) = e.get_mut("params").and_then(Value::as_object_mut) {
                        for key in [
                            "module", "centerX", "centerY", "backlash", "pitch", "radius",
                        ] {
                            if let Some(v) = p.get(key).and_then(Value::as_f64) {
                                p.insert(key.to_string(), Value::from(v * factor));
                            }
                        }
                    }
                }
            }
        }
        // The engine reads profile GEOMETRY from `solved_profiles` (a circle's
        // centre and radius, an arc segment's centre and radius, a spline's
        // control points), not from the entities — the ×1e3 metamorphic run
        // scaled every circle case's volume by 1e3 instead of 1e9 until this
        // was scaled too.
        if let Some(profiles) = op
            .get_mut("sketch")
            .and_then(|sk| sk.get_mut("solved_profiles"))
            .and_then(Value::as_array_mut)
        {
            for prof in profiles.iter_mut() {
                if let Some(c) = prof.get_mut("circle").and_then(Value::as_object_mut) {
                    for key in ["center_u", "center_v", "radius"] {
                        if let Some(v) = c.get(key).and_then(Value::as_f64) {
                            c.insert(key.to_string(), Value::from(v * factor));
                        }
                    }
                }
                if let Some(arcs) = prof.get_mut("arc_segments").and_then(Value::as_array_mut) {
                    for a in arcs.iter_mut() {
                        if let Some(a) = a.as_object_mut() {
                            for key in ["center_u", "center_v", "radius"] {
                                if let Some(v) = a.get(key).and_then(Value::as_f64) {
                                    a.insert(key.to_string(), Value::from(v * factor));
                                }
                            }
                        }
                    }
                }
                if let Some(splines) = prof
                    .get_mut("spline_segments")
                    .and_then(Value::as_array_mut)
                {
                    for sp in splines.iter_mut() {
                        if let Some(cps) =
                            sp.get_mut("control_points").and_then(Value::as_array_mut)
                        {
                            for cp in cps.iter_mut() {
                                if let Some(pair) = cp.as_array_mut() {
                                    for c in pair.iter_mut() {
                                        let v = c.as_f64().unwrap_or(0.0) * factor;
                                        *c = Value::from(v);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(params) = op.get_mut("params") {
            match ty.as_str() {
                "Extrude" => {
                    if let Some(d) = params.get("depth").and_then(Value::as_f64) {
                        params["depth"] = Value::from(d * factor);
                    }
                }
                "Revolve" => {
                    if let Some(o) = params.get("axis_origin").and_then(vec3) {
                        set_vec3(
                            params.get_mut("axis_origin")?,
                            [o[0] * factor, o[1] * factor, o[2] * factor],
                        );
                    }
                }
                _ => {}
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus_doc(id: &str) -> Value {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/tests/cases/assay");
        serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{id}.waffle"))).unwrap())
            .unwrap()
    }

    #[test]
    fn scale_moves_every_length_and_nothing_else() {
        let doc = corpus_doc("C0059");
        let s = scale_document(&doc, 1e-3).unwrap();
        assert!((characteristic_length(&s) - characteristic_length(&doc) * 1e-3).abs() < 1e-15);
        // Angles untouched.
        let a0 = doc.pointer("/tabs/0/kind/features/features/1/operation/params/angle");
        let a1 = s.pointer("/tabs/0/kind/features/features/1/operation/params/angle");
        assert_eq!(a0, a1);
    }

    #[test]
    fn scale_reaches_the_solved_profile_circle() {
        let doc = corpus_doc("C0042");
        let s = scale_document(&doc, 1e3).unwrap();
        let r0 = doc
            .pointer(
                "/tabs/0/kind/features/features/0/operation/sketch/solved_profiles/0/circle/radius",
            )
            .and_then(Value::as_f64)
            .unwrap();
        let r1 = s
            .pointer(
                "/tabs/0/kind/features/features/0/operation/sketch/solved_profiles/0/circle/radius",
            )
            .and_then(Value::as_f64)
            .unwrap();
        assert!((r1 - r0 * 1e3).abs() < 1e-9, "{r0} → {r1}");
    }

    #[test]
    fn every_knob_changes_the_document_once() {
        let doc = corpus_doc("C0059");
        let mut rng = Rng::new(3);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            if let Some((m, knob)) = mutate(&doc, &mut rng) {
                assert_ne!(m, doc, "{knob:?} changed nothing");
                seen.insert(knob.label().split(' ').next().unwrap().to_string());
            }
        }
        assert!(seen.len() >= 5, "knob variety: {}", seen.len());
    }

    #[test]
    fn point_mutation_moves_both_representations() {
        let doc = corpus_doc("C0059");
        let mut rng = Rng::new(11);
        for _ in 0..500 {
            let Some((m, knob)) = mutate(&doc, &mut rng) else {
                continue;
            };
            if let Knob::Point {
                feature,
                point_id,
                axis,
                delta,
            } = knob
            {
                if point_id == 0 {
                    continue;
                }
                let sk = m
                    .pointer(&format!(
                        "/tabs/0/kind/features/features/{feature}/operation/sketch"
                    ))
                    .unwrap();
                let idx = if axis == 'u' { 0 } else { 1 };
                let solved = sk["solved_positions"][point_id.to_string()][idx]
                    .as_f64()
                    .unwrap();
                let ent = sk["entities"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["id"].as_u64() == Some(point_id))
                    .unwrap();
                let ex = ent[if axis == 'u' { "x" } else { "y" }].as_f64().unwrap();
                assert!((solved - ex).abs() < 1e-15, "{knob:?}: {solved} vs {ex}");
                let orig = doc
                    .pointer(&format!(
                        "/tabs/0/kind/features/features/{feature}/operation/sketch/solved_positions/{point_id}/{idx}"
                    ))
                    .unwrap()
                    .as_f64()
                    .unwrap();
                assert!((solved - orig - delta).abs() < 1e-12);
                return;
            }
        }
        panic!("no single-point mutation drawn");
    }
}
