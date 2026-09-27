//! Metamorphic identities — two kernel runs that must agree with each
//! other (`specs/assay_prospector.md` §6).
//!
//! No reference is needed: a document and its rigidly moved copy must
//! produce the same category, volume, χ and shell count; a uniformly
//! scaled copy the same category and χ with the volume scaled by s³.
//! Because the engine derives a sketch's in-plane basis from its normal
//! (`tangent_x_from_normal`), a rotated normal alone would change the
//! basis non-rigidly — so the REFERENCE document first gets every sketch's
//! implicit x-axis written out explicitly (`with_explicit_x_axes`), and
//! the moved copy rotates that axis with everything else. That the
//! explicit-axis reference equals the original is itself checked (the
//! `axes` identity): the engine must honour a `plane_x_axis` that equals
//! the one it would have derived.

use serde_json::Value;

use super::gen3::{plane_basis, Rng};

fn vec3(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}

fn set_vec3(v: &mut Value, x: [f64; 3]) {
    *v = serde_json::json!([x[0], x[1], x[2]]);
}

fn features_mut(doc: &mut Value) -> Option<&mut Vec<Value>> {
    doc.get_mut("tabs")?
        .as_array_mut()?
        .first_mut()?
        .pointer_mut("/kind/features/features")?
        .as_array_mut()
}

/// A proper rotation (3×3, row-major) and a translation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RigidMotion {
    pub r: [[f64; 3]; 3],
    pub t: [f64; 3],
}

impl RigidMotion {
    /// Rotation by `angle` about the unit `axis` (Rodrigues), translation `t`.
    pub fn about(axis: [f64; 3], angle: f64, t: [f64; 3]) -> Self {
        let l = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        let k = [axis[0] / l, axis[1] / l, axis[2] / l];
        let (s, c) = angle.sin_cos();
        let v = 1.0 - c;
        let r = [
            [
                c + k[0] * k[0] * v,
                k[0] * k[1] * v - k[2] * s,
                k[0] * k[2] * v + k[1] * s,
            ],
            [
                k[1] * k[0] * v + k[2] * s,
                c + k[1] * k[1] * v,
                k[1] * k[2] * v - k[0] * s,
            ],
            [
                k[2] * k[0] * v - k[1] * s,
                k[2] * k[1] * v + k[0] * s,
                c + k[2] * k[2] * v,
            ],
        ];
        RigidMotion { r, t }
    }

    /// A seeded generic motion: random axis, angle in (20°, 160°), a
    /// translation of about `scale` in each coordinate.
    pub fn draw(rng: &mut Rng, scale: f64) -> Self {
        let theta = rng.range(0.2, std::f64::consts::PI - 0.2);
        let phi = rng.range(0.0, std::f64::consts::TAU);
        let axis = [
            theta.sin() * phi.cos(),
            theta.sin() * phi.sin(),
            theta.cos(),
        ];
        let angle = rng.range(20f64.to_radians(), 160f64.to_radians());
        let t = [
            rng.range(-1.0, 1.0) * scale,
            rng.range(-1.0, 1.0) * scale,
            rng.range(-1.0, 1.0) * scale,
        ];
        Self::about(axis, angle, t)
    }

    pub fn rotate(&self, v: [f64; 3]) -> [f64; 3] {
        let r = &self.r;
        [
            r[0][0] * v[0] + r[0][1] * v[1] + r[0][2] * v[2],
            r[1][0] * v[0] + r[1][1] * v[1] + r[1][2] * v[2],
            r[2][0] * v[0] + r[2][1] * v[1] + r[2][2] * v[2],
        ]
    }

    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let q = self.rotate(p);
        [q[0] + self.t[0], q[1] + self.t[1], q[2] + self.t[2]]
    }
}

/// Write every sketch's implicit x-axis (the engine's `tangent_x_from_normal`
/// basis, `gen3::plane_basis`) into `plane_x_axis` where it is absent. The
/// result is the rigid-motion REFERENCE: geometrically identical to the
/// input by construction.
pub fn with_explicit_x_axes(doc: &Value) -> Option<Value> {
    let mut out = doc.clone();
    for f in features_mut(&mut out)? {
        let Some(sk) = f.pointer_mut("/operation/sketch") else {
            continue;
        };
        if sk.get("plane_x_axis").is_some_and(|x| !x.is_null()) {
            continue;
        }
        let n = sk.get("plane_normal").and_then(vec3)?;
        let (u, _) = plane_basis(n);
        sk["plane_x_axis"] = serde_json::json!([u[0], u[1], u[2]]);
    }
    Some(out)
}

/// Apply a rigid motion to every world-frame quantity of the document:
/// sketch plane origins (points), plane normals and x-axes (vectors), an
/// extrude's explicit `direction` (vector), a revolve's `axis_origin`
/// (point) and `axis_direction` (vector). Sketch-local coordinates are
/// untouched — they ride on the plane.
///
/// Call on a document that already has explicit x-axes
/// ([`with_explicit_x_axes`]); otherwise the engine re-derives a basis from
/// the rotated normal and the motion is not rigid.
pub fn rigid_motion(doc: &Value, m: &RigidMotion) -> Option<Value> {
    let mut out = doc.clone();
    for f in features_mut(&mut out)? {
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
                set_vec3(sk.get_mut("plane_origin")?, m.apply(o));
            }
            if let Some(n) = sk.get("plane_normal").and_then(vec3) {
                set_vec3(sk.get_mut("plane_normal")?, m.rotate(n));
            }
            if let Some(x) = sk.get("plane_x_axis").and_then(vec3) {
                set_vec3(sk.get_mut("plane_x_axis")?, m.rotate(x));
            }
        }
        if let Some(params) = op.get_mut("params") {
            match ty.as_str() {
                "Extrude" => {
                    if let Some(d) = params.get("direction").and_then(vec3) {
                        set_vec3(params.get_mut("direction")?, m.rotate(d));
                    }
                }
                "Revolve" => {
                    if let Some(o) = params.get("axis_origin").and_then(vec3) {
                        set_vec3(params.get_mut("axis_origin")?, m.apply(o));
                    }
                    if let Some(a) = params.get("axis_direction").and_then(vec3) {
                        set_vec3(params.get_mut("axis_direction")?, m.rotate(a));
                    }
                }
                _ => {}
            }
        }
    }
    Some(out)
}

/// What one run of a document measured, for comparing pairs.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Measurement {
    pub category: String,
    pub detail: String,
    /// Sum of the live bodies' signed volumes (`None` when nothing tessellated).
    pub volume: Option<f64>,
    /// Sum of the live bodies' exact-bit Euler characteristics.
    pub chi: Option<i64>,
    pub bodies: usize,
}

/// Compare a reference measurement with a transformed one under the
/// identity `name`; `volume_factor` is what the transform does to volume
/// (1 for rigid, s³ for scale). Returns the disagreement, if any.
pub fn compare(
    name: &str,
    reference: &Measurement,
    other: &Measurement,
    volume_factor: f64,
    rel_band: f64,
) -> Option<String> {
    if reference.category != other.category {
        // The KERNEL's own measurements come first: when the two runs agree
        // on volume, χ and body count but an in-line ORACLE flagged one of
        // them (`exact_volume`, `volume_composition`, the bbox bound), the
        // inconsistency is the oracle's, not the kernel's — the 2026-09-27
        // run found the exact-membership lattice 16 % off on rotated revolve
        // cases (C0059–C0069) while the kernel volumes matched to 2e-5.
        let kernel_agrees = reference.bodies == other.bodies
            && reference.chi == other.chi
            && match (reference.volume, other.volume) {
                (Some(a), Some(b)) => {
                    let expected = a * volume_factor;
                    (b - expected).abs() <= rel_band * expected.abs().max(f64::MIN_POSITIVE)
                }
                _ => false,
            };
        let oracle_only = kernel_agrees
            && ["SUPPORTED_CORRECT", "SUPPORTED_WRONG"].contains(&reference.category.as_str())
            && ["SUPPORTED_CORRECT", "SUPPORTED_WRONG"].contains(&other.category.as_str());
        if oracle_only {
            return Some(format!(
                "oracle[{name}]: kernel measurements agree (V {:?} vs {:?}, χ {:?}, bodies {}) but the categorizer says {} vs {} ({})",
                reference.volume,
                other.volume,
                reference.chi,
                reference.bodies,
                reference.category,
                other.category,
                if reference.category == "SUPPORTED_CORRECT" { &other.detail } else { &reference.detail }
            ));
        }
        return Some(format!(
            "metamorphic[{name}]: category {} vs {} ({})",
            reference.category, other.category, other.detail
        ));
    }
    if reference.category != "SUPPORTED_CORRECT" {
        // Both loud in the same way: consistent. (A different loud text is
        // a weaker signal; the category is the contract.)
        return None;
    }
    if reference.bodies != other.bodies {
        return Some(format!(
            "metamorphic[{name}]: body count {} vs {}",
            reference.bodies, other.bodies
        ));
    }
    match (reference.chi, other.chi) {
        (Some(a), Some(b)) if a != b => {
            return Some(format!("metamorphic[{name}]: χ {a} vs {b}"));
        }
        _ => {}
    }
    if let (Some(a), Some(b)) = (reference.volume, other.volume) {
        let expected = a * volume_factor;
        let rel = (b - expected).abs() / expected.abs().max(f64::MIN_POSITIVE);
        if rel > rel_band {
            return Some(format!(
                "metamorphic[{name}]: volume {b:.9e} vs expected {expected:.9e} (rel {rel:.3e} > {rel_band:.1e})"
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_is_orthonormal_and_translation_applies() {
        let m = RigidMotion::draw(&mut Rng::new(5), 2.0);
        let e = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        for i in 0..3 {
            for j in 0..3 {
                let a = m.rotate(e[i]);
                let b = m.rotate(e[j]);
                let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                assert!((dot - if i == j { 1.0 } else { 0.0 }).abs() < 1e-12);
            }
        }
        let p = m.apply([0.0; 3]);
        assert_eq!(p, m.t);
    }

    #[test]
    fn explicit_axes_match_the_engine_basis_and_rotate_with_the_normal() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/tests/cases/assay");
        let doc: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("C0059.waffle")).unwrap())
                .unwrap();
        let reference = with_explicit_x_axes(&doc).unwrap();
        let sk = reference
            .pointer("/tabs/0/kind/features/features/0/operation/sketch")
            .unwrap();
        let n = vec3(&sk["plane_normal"]).unwrap();
        let x = vec3(&sk["plane_x_axis"]).unwrap();
        assert_eq!(x, plane_basis(n).0);
        let m = RigidMotion::about(
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_2,
            [1.0, 2.0, 3.0],
        );
        let moved = rigid_motion(&reference, &m).unwrap();
        let sk2 = moved
            .pointer("/tabs/0/kind/features/features/0/operation/sketch")
            .unwrap();
        let n2 = vec3(&sk2["plane_normal"]).unwrap();
        let x2 = vec3(&sk2["plane_x_axis"]).unwrap();
        assert_eq!(n2, m.rotate(n));
        assert_eq!(x2, m.rotate(x));
        let o2 = vec3(&sk2["plane_origin"]).unwrap();
        assert_eq!(o2, m.apply(vec3(&sk["plane_origin"]).unwrap()));
        // Sketch-local coordinates untouched.
        assert_eq!(sk["solved_positions"], sk2["solved_positions"]);
    }

    #[test]
    fn compare_flags_only_real_disagreements() {
        let a = Measurement {
            category: "SUPPORTED_CORRECT".into(),
            detail: String::new(),
            volume: Some(8.0),
            chi: Some(2),
            bodies: 1,
        };
        let mut b = a.clone();
        assert!(compare("rigid", &a, &b, 1.0, 1e-6).is_none());
        b.volume = Some(8.0 * 1e-9);
        assert!(compare("scale", &a, &b, 1e-9, 1e-6).is_none());
        b.volume = Some(7.9);
        assert!(compare("rigid", &a, &b, 1.0, 1e-3).is_some());
        b = a.clone();
        b.chi = Some(0);
        assert!(compare("rigid", &a, &b, 1.0, 1e-3).is_some());
        b = a.clone();
        b.category = "ERROR".into();
        assert!(compare("rigid", &a, &b, 1.0, 1e-3).is_some());
        // Kernel agrees, an oracle disagrees ⇒ the oracle's problem.
        b = a.clone();
        b.category = "SUPPORTED_WRONG".into();
        b.detail = "exact_volume: …".into();
        let msg = compare("rigid", &a, &b, 1.0, 1e-3).unwrap();
        assert!(msg.starts_with("oracle[rigid]"), "{msg}");
    }
}
