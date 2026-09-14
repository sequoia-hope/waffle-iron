//! Numeric mate solver (v4 Phase 3d, `projects/10-assemblies/PLAN.md` M3):
//! places the free instances of an assembly so every known mate's equations
//! hold, by damped Gauss-Newton (Levenberg–Marquardt) over the instances'
//! poses. Fastened chains are first placed exactly by composition
//! (`assembly::solve_fastened`); the numeric pass then starts from those
//! poses — and from each remaining instance's own transform — so the free
//! degrees of freedom a Revolute/Slider/… leaves open keep their current
//! value instead of drifting.
//!
//! Residuals are written in connector a's frame: with `D = Wa⁻¹ ∘ Wb` (b's
//! world frame seen from a's), `d` its origin and `z_d`/`R_d` its axes, a
//! Fastened mate asks `d = t_off` and `R_d = R_off`; Revolute asks `d = 0`
//! and `z_d = ±ẑ`; Slider asks `R_d = R_off` and `d.xy = 0`; Cylindrical
//! `z_d = ±ẑ`, `d.xy = 0`; Planar `z_d = ±ẑ`, `d.z = 0`; Ball `d = 0`.
//! The Jacobian is by central differences (six pose increments per free
//! instance: translation, then a rotation vector applied on the left).

use std::collections::HashMap;

use nalgebra::{DMatrix, DVector};
use uuid::Uuid;

use crate::assembly::{
    quat_axis_angle, quat_conj, quat_exp, quat_log, quat_mul, quat_normalize, solve_fastened,
    AssemblyTree, Frame, Mate, MateKind, SolveResult, Transform,
};

/// Everything the numeric pass needs about one mate.
struct MateEq<'a> {
    mate: &'a Mate,
    /// Free-instance index (or None when grounded) and connector transform,
    /// for a and b.
    ia: Option<usize>,
    ib: Option<usize>,
    ta_ground: Transform,
    tb_ground: Transform,
    fa: Transform,
    fb: Transform,
}

/// `R_off` for a mate: the frame b must coincide with, expressed in a's frame.
fn offset_rotation(kind: &MateKind) -> [f64; 4] {
    let flip = if kind.flip() {
        quat_axis_angle([1.0, 0.0, 0.0], 180.0)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    match kind {
        MateKind::Fastened { rotation_deg, .. } => {
            quat_mul(quat_axis_angle([0.0, 0.0, 1.0], *rotation_deg), flip)
        }
        _ => flip,
    }
}

/// The residual rows of one mate for b's frame `d` seen in a's frame.
fn mate_residuals(kind: &MateKind, d: &Transform, out: &mut Vec<f64>) {
    let r_off = offset_rotation(kind);
    let sign = if kind.flip() { -1.0 } else { 1.0 };
    let t = d.translation_m;
    match kind {
        MateKind::Fastened { .. } => {
            out.extend_from_slice(&t);
            out.extend_from_slice(&quat_log(quat_mul(quat_conj(r_off), d.rotation_quat)));
        }
        MateKind::Revolute { .. } => {
            out.extend_from_slice(&t);
            let z = d.apply_dir([0.0, 0.0, 1.0]);
            out.extend_from_slice(&[z[0], z[1], z[2] - sign]);
        }
        MateKind::Slider { .. } => {
            out.extend_from_slice(&[t[0], t[1]]);
            out.extend_from_slice(&quat_log(quat_mul(quat_conj(r_off), d.rotation_quat)));
        }
        MateKind::Cylindrical { .. } => {
            out.extend_from_slice(&[t[0], t[1]]);
            let z = d.apply_dir([0.0, 0.0, 1.0]);
            out.extend_from_slice(&[z[0], z[1], z[2] - sign]);
        }
        MateKind::Planar { .. } => {
            out.push(t[2]);
            let z = d.apply_dir([0.0, 0.0, 1.0]);
            out.extend_from_slice(&[z[0], z[1], z[2] - sign]);
        }
        MateKind::Ball => out.extend_from_slice(&t),
        MateKind::Unknown(_) => {}
    }
}

/// A free instance's pose from its base and the current increment.
fn pose(base: &Transform, p: &[f64]) -> Transform {
    Transform {
        translation_m: [
            base.translation_m[0] + p[0],
            base.translation_m[1] + p[1],
            base.translation_m[2] + p[2],
        ],
        rotation_quat: quat_normalize(quat_mul(quat_exp([p[3], p[4], p[5]]), base.rotation_quat)),
    }
}

fn residual_vector(eqs: &[MateEq], bases: &[Transform], p: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    for e in eqs {
        let ta = match e.ia {
            Some(i) => pose(&bases[i], &p[6 * i..6 * i + 6]),
            None => e.ta_ground,
        };
        let tb = match e.ib {
            Some(i) => pose(&bases[i], &p[6 * i..6 * i + 6]),
            None => e.tb_ground,
        };
        let wa = ta.compose(&e.fa);
        let wb = tb.compose(&e.fb);
        let d = wa.inverse().compose(&wb);
        mate_residuals(&e.mate.kind, &d, &mut out);
    }
    out
}

/// Place the instances of `tree` by all its known mates: Fastened chains
/// exactly by composition, then a numeric pass for the rest (see the module
/// doc). `frames` are the connector frames in part coordinates.
pub fn solve_mates(tree: &AssemblyTree, frames: &HashMap<Uuid, Frame>, tol_m: f64) -> SolveResult {
    // Exact pass. Its "not grounded" warnings are re-derived below.
    let fastened_only = !tree
        .mates
        .iter()
        .any(|m| !m.suppressed && m.kind.is_numeric());
    let seed = solve_fastened(tree, frames, tol_m);
    if fastened_only {
        return seed;
    }
    let mut result = SolveResult {
        placements: seed.placements.clone(),
        errors: seed.errors.clone(),
        warnings: Vec::new(),
    };

    let live: Vec<&crate::assembly::Instance> =
        tree.instances.iter().filter(|i| !i.suppressed).collect();
    let explicit_grounds: Vec<Uuid> = live.iter().filter(|i| i.fixed).map(|i| i.id).collect();
    let grounds: Vec<Uuid> = if explicit_grounds.is_empty() {
        live.first().map(|i| vec![i.id]).unwrap_or_default()
    } else {
        explicit_grounds
    };

    // Connector transforms.
    let mut frame_xf: HashMap<Uuid, Transform> = HashMap::new();
    for c in &tree.connectors {
        if let Ok(t) = frames.get(&c.id).copied().unwrap_or(c.frame).to_transform() {
            frame_xf.insert(c.id, t);
        }
    }

    // Instances touched by any known mate; free = touched and not grounded.
    let mut touched: Vec<Uuid> = Vec::new();
    let mut usable: Vec<&Mate> = Vec::new();
    for m in tree
        .mates
        .iter()
        .filter(|m| !m.suppressed && (m.kind.is_fastened() || m.kind.is_numeric()))
    {
        let (Some(ca), Some(cb)) = (
            tree.connector(m.connectors[0]),
            tree.connector(m.connectors[1]),
        ) else {
            continue; // reported by validate()/solve_fastened
        };
        let (Some(ia), Some(ib)) = (ca.top_instance_id(), cb.top_instance_id()) else {
            continue;
        };
        if !frame_xf.contains_key(&ca.id) || !frame_xf.contains_key(&cb.id) {
            continue;
        }
        if tree.instance(ia).map(|i| i.suppressed).unwrap_or(true)
            || tree.instance(ib).map(|i| i.suppressed).unwrap_or(true)
        {
            continue;
        }
        for id in [ia, ib] {
            if !touched.contains(&id) {
                touched.push(id);
            }
        }
        usable.push(m);
    }
    let free: Vec<Uuid> = touched
        .iter()
        .copied()
        .filter(|id| !grounds.contains(id))
        .collect();
    let index_of = |id: Uuid| free.iter().position(|f| *f == id);

    let bases: Vec<Transform> = free.iter().map(|id| result.placements[id]).collect();
    let eqs: Vec<MateEq> = usable
        .iter()
        .map(|m| {
            let ca = tree.connector(m.connectors[0]).unwrap();
            let cb = tree.connector(m.connectors[1]).unwrap();
            let ia_id = ca.top_instance_id().unwrap();
            let ib_id = cb.top_instance_id().unwrap();
            MateEq {
                mate: m,
                ia: index_of(ia_id),
                ib: index_of(ib_id),
                ta_ground: result.placements[&ia_id],
                tb_ground: result.placements[&ib_id],
                fa: frame_xf[&ca.id],
                fb: frame_xf[&cb.id],
            }
        })
        .collect();

    if !free.is_empty() && !eqs.is_empty() {
        let n = 6 * free.len();
        let mut bases = bases;
        let mut lambda = 1e-3;
        let mut p = vec![0.0; n];
        let mut r = residual_vector(&eqs, &bases, &p);
        let norm = |r: &[f64]| r.iter().map(|x| x * x).sum::<f64>().sqrt();
        let mut cost = norm(&r);
        for _iter in 0..200 {
            if cost < 1e-12 {
                break;
            }
            // Central-difference Jacobian at p = 0 (bases carry the pose).
            let m = r.len();
            let mut jac = DMatrix::<f64>::zeros(m, n);
            for j in 0..n {
                let h = if j % 6 < 3 { 1e-7 } else { 1e-6 };
                let mut pp = p.clone();
                pp[j] += h;
                let rp = residual_vector(&eqs, &bases, &pp);
                let mut pm = p.clone();
                pm[j] -= h;
                let rm = residual_vector(&eqs, &bases, &pm);
                for i in 0..m {
                    jac[(i, j)] = (rp[i] - rm[i]) / (2.0 * h);
                }
            }
            let rv = DVector::from_vec(r.clone());
            let jt = jac.transpose();
            let jtj = &jt * &jac;
            let g = &jt * &rv;
            let mut improved = false;
            for _try in 0..12 {
                let mut a = jtj.clone();
                for i in 0..n {
                    // Pure Levenberg damping (λI), deliberately NOT
                    // Marquardt's λ·diag(JᵀJ): along a free degree of freedom
                    // (a Revolute's hinge angle, a Slider's travel) JᵀJ is
                    // near-singular, and diagonal scaling regularizes that
                    // direction by almost nothing, so the solve blows the
                    // component up (measured: a 3 mm hinge-origin error moved
                    // the hinge 15°). With λI the step is the MINIMUM-NORM
                    // correction for any λ, and the free coordinate keeps its
                    // current value.
                    a[(i, i)] += lambda;
                }
                let Some(delta) = a.lu().solve(&(-&g)) else {
                    lambda *= 4.0;
                    continue;
                };
                let step: Vec<f64> = delta.iter().copied().collect();
                let r_new = residual_vector(&eqs, &bases, &step);
                let cost_new = norm(&r_new);
                if cost_new < cost {
                    if std::env::var("WAFFLE_MATE_TRACE").is_ok() {
                        eprintln!("step cost {cost:.3e} -> {cost_new:.3e} lambda {lambda:.1e} step {step:?}");
                    }
                    // Fold the step into the bases; increments restart at 0.
                    for (i, b) in bases.iter_mut().enumerate() {
                        *b = pose(b, &step[6 * i..6 * i + 6]);
                    }
                    p = vec![0.0; n];
                    let step_norm = step.iter().map(|x| x * x).sum::<f64>().sqrt();
                    r = r_new;
                    cost = cost_new;
                    lambda = (lambda / 3.0).max(1e-9);
                    improved = true;
                    if step_norm < 1e-14 {
                        break;
                    }
                    break;
                }
                lambda *= 4.0;
            }
            if !improved {
                break;
            }
        }
        for (i, id) in free.iter().enumerate() {
            result.placements.insert(*id, bases[i]);
        }
        // Report mates that are not satisfied.
        let final_r = residual_vector(&eqs, &bases, &vec![0.0; n]);
        let mut cursor = 0;
        for e in &eqs {
            let mut rows = Vec::new();
            mate_residuals(&e.mate.kind, &Transform::identity(), &mut rows);
            let k = rows.len();
            let worst = final_r[cursor..cursor + k]
                .iter()
                .fold(0.0_f64, |a, x| a.max(x.abs()));
            cursor += k;
            if worst > tol_m {
                result.errors.push(format!(
                    "mate `{}` ({}, {}): not satisfied (residual {worst:.3e}) — over-constrained or conflicting",
                    e.mate.name,
                    e.mate.id,
                    e.mate.kind.type_tag()
                ));
            }
        }
    }

    // Ungrounded warnings: instances no mate touches at all.
    for i in &live {
        if !grounds.contains(&i.id) && !touched.contains(&i.id) {
            result.warnings.push(format!(
                "instance `{}` ({}) is not grounded through mates; its own transform is used",
                i.name, i.id
            ));
            result.placements.entry(i.id).or_insert(i.transform);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::{AxialAnchor, Instance, MateConnector, PartRef};
    use serde_json::Map;
    use std::collections::BTreeMap;

    fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < tol)
    }

    fn inst(name: &str, t: Transform, fixed: bool) -> Instance {
        Instance {
            id: Uuid::new_v4(),
            name: name.into(),
            source: PartRef {
                source_id: None,
                tab_id: "t".into(),
            },
            transform: t,
            fixed,
            suppressed: false,
            external_key: None,
            parameter_overrides: None,
            extra: Map::new(),
        }
    }

    fn conn(i: Uuid, frame: Frame) -> MateConnector {
        MateConnector {
            id: Uuid::new_v4(),
            name: "c".into(),
            instance_path: vec![i],
            geom_ref: None,
            part_connector: None,
            frame,
            anchor: AxialAnchor::Middle,
            flip_z: false,
            rotation_deg: 0.0,
            offset_m: [0.0; 3],
            extra: Map::new(),
        }
    }

    fn mate(kind: MateKind, a: Uuid, b: Uuid) -> Mate {
        Mate {
            id: Uuid::new_v4(),
            name: "m".into(),
            kind,
            connectors: [a, b],
            suppressed: false,
            extra: Map::new(),
        }
    }

    /// A grounded at the origin; B starting displaced and rotated 30° about y.
    fn two(
        kind: MateKind,
        fa: Frame,
        fb: Frame,
        b_start: Transform,
    ) -> (AssemblyTree, Uuid, HashMap<Uuid, Frame>) {
        let a = inst("A", Transform::identity(), true);
        let b = inst("B", b_start, false);
        let (ida, idb) = (a.id, b.id);
        let ca = conn(ida, fa);
        let cb = conn(idb, fb);
        let mut frames = HashMap::new();
        frames.insert(ca.id, fa);
        frames.insert(cb.id, fb);
        let tree = AssemblyTree {
            instances: vec![a, b],
            connectors: vec![ca.clone(), cb.clone()],
            mates: vec![mate(kind, ca.id, cb.id)],
            placements: BTreeMap::new(),
            extra: Map::new(),
        };
        (tree, idb, frames)
    }

    fn b_in_a(
        tree: &AssemblyTree,
        r: &SolveResult,
        idb: Uuid,
        frames: &HashMap<Uuid, Frame>,
    ) -> Transform {
        let ca = &tree.connectors[0];
        let cb = &tree.connectors[1];
        let wa =
            r.placements[&tree.instances[0].id].compose(&frames[&ca.id].to_transform().unwrap());
        let wb = r.placements[&idb].compose(&frames[&cb.id].to_transform().unwrap());
        wa.inverse().compose(&wb)
    }

    #[test]
    fn revolute_hinge_keeps_the_current_angle_and_aligns_the_axis() {
        // Hinge axis along y at the top-right edge of A; B starts 30° open,
        // displaced off the axis.
        let fa = Frame {
            origin: [0.01, 0.0, 0.01],
            z_axis: [0.0, 1.0, 0.0],
            x_axis: [0.0; 3],
        };
        let fb = Frame {
            origin: [0.0, 0.0, 0.01],
            z_axis: [0.0, 1.0, 0.0],
            x_axis: [0.0; 3],
        };
        let rot30 = Transform::from_rotation(quat_axis_angle([0.0, 1.0, 0.0], 30.0));
        let start = Transform::translation([0.012, 0.003, 0.002]).compose(&rot30);
        let (tree, idb, frames) = two(MateKind::Revolute { flip: false }, fa, fb, start);
        let r = solve_mates(&tree, &frames, 1e-7);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let d = b_in_a(&tree, &r, idb, &frames);
        assert!(close(d.translation_m, [0.0; 3], 1e-7), "{d:?}");
        assert!(close(d.apply_dir([0.0, 0.0, 1.0]), [0.0, 0.0, 1.0], 1e-7));
        // The free rotation kept its 30° up to the minimum-norm coupling of
        // the correction (lever 10 mm × 3 mm origin error ≈ 6e-5 rad): the
        // solver moved the hinge by 0.003°, not by the 15° a diagonal-scaled
        // damping produced.
        let angle = quat_log(r.placements[&idb].rotation_quat);
        let deg = (angle[0] * angle[0] + angle[1] * angle[1] + angle[2] * angle[2])
            .sqrt()
            .to_degrees();
        assert!((deg - 30.0).abs() < 1e-2, "{deg}");
    }

    #[test]
    fn slider_removes_lateral_offset_and_keeps_the_travel() {
        let fa = Frame::on_plane([0.0; 3], [0.0, 0.0, 1.0]);
        let fb = Frame::on_plane([0.0; 3], [0.0, 0.0, 1.0]);
        let start = Transform::translation([0.001, 0.002, 0.03]);
        let (tree, idb, frames) = two(MateKind::Slider { flip: false }, fa, fb, start);
        let r = solve_mates(&tree, &frames, 1e-7);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let t = r.placements[&idb];
        assert!(close(t.translation_m, [0.0, 0.0, 0.03], 1e-7), "{t:?}");
        assert!((t.rotation_quat[3].abs() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cylindrical_planar_and_ball_satisfy_their_equations_only() {
        let fa = Frame::on_plane([0.0; 3], [0.0, 0.0, 1.0]);
        let fb = Frame::on_plane([0.0; 3], [0.0, 0.0, 1.0]);
        let tilt = Transform::from_rotation(quat_axis_angle([1.0, 0.0, 0.0], 10.0));
        let start = Transform::translation([0.004, 0.003, 0.02]).compose(&tilt);

        let (tree, idb, frames) = two(MateKind::Cylindrical { flip: true }, fa, fb, start);
        let r = solve_mates(&tree, &frames, 1e-7);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let d = b_in_a(&tree, &r, idb, &frames);
        assert!(
            d.translation_m[0].abs() < 1e-7 && d.translation_m[1].abs() < 1e-7,
            "{d:?}"
        );
        assert!(
            close(d.apply_dir([0.0, 0.0, 1.0]), [0.0, 0.0, -1.0], 1e-7),
            "{d:?}"
        );

        let (tree, idb, frames) = two(MateKind::Planar { flip: false }, fa, fb, start);
        let r = solve_mates(&tree, &frames, 1e-7);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let d = b_in_a(&tree, &r, idb, &frames);
        assert!(d.translation_m[2].abs() < 1e-7, "{d:?}");
        assert!(
            close(d.apply_dir([0.0, 0.0, 1.0]), [0.0, 0.0, 1.0], 1e-7),
            "{d:?}"
        );
        // Lateral position is free and kept near the start.
        assert!((d.translation_m[0] - 0.004).abs() < 1e-3, "{d:?}");

        let (tree, idb, frames) = two(MateKind::Ball, fa, fb, start);
        let r = solve_mates(&tree, &frames, 1e-7);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let d = b_in_a(&tree, &r, idb, &frames);
        assert!(close(d.translation_m, [0.0; 3], 1e-7), "{d:?}");
        // Orientation free: the tilt is kept.
        let z = d.apply_dir([0.0, 0.0, 1.0]);
        assert!((z[2] - 10f64.to_radians().cos()).abs() < 1e-6, "{z:?}");
    }

    #[test]
    fn fastened_only_trees_use_the_exact_path_and_conflicts_are_loud() {
        let fa = Frame::on_plane([0.005, 0.005, 0.01], [0.0, 0.0, 1.0]);
        let fb = Frame::on_plane([0.005, 0.005, 0.0], [0.0, 0.0, -1.0]);
        let (tree, idb, frames) = two(
            MateKind::Fastened {
                flip: true,
                rotation_deg: 0.0,
            },
            fa,
            fb,
            Transform::identity(),
        );
        let r = solve_mates(&tree, &frames, 1e-9);
        assert!(close(
            r.placements[&idb].translation_m,
            [0.0, 0.0, 0.01],
            1e-12
        ));

        // A revolute about z PLUS a fastened at a different rotation cannot
        // both hold when the fastened demands rotation the revolute allows —
        // they agree; make them disagree in translation instead: a Ball mate
        // pulling B's origin to a point 5 mm away from the fastened stack.
        let mut tree = tree;
        let cc = conn(
            tree.instances[0].id,
            Frame::on_plane([0.005, 0.005, 0.015], [0.0, 0.0, 1.0]),
        );
        let cd = conn(idb, Frame::on_plane([0.0; 3], [0.0, 0.0, 1.0]));
        let mut frames = frames;
        frames.insert(cc.id, cc.frame);
        frames.insert(cd.id, cd.frame);
        tree.mates.push(mate(MateKind::Ball, cc.id, cd.id));
        tree.connectors.push(cc);
        tree.connectors.push(cd);
        let r = solve_mates(&tree, &frames, 1e-7);
        assert!(
            r.errors.iter().any(|e| e.contains("not satisfied")),
            "{:?}",
            r.errors
        );
    }

    #[test]
    fn mate_kinds_round_trip() {
        for (v, expect) in [
            (
                serde_json::json!({ "type": "Revolute" }),
                MateKind::Revolute { flip: false },
            ),
            (
                serde_json::json!({ "type": "Slider", "flip": true }),
                MateKind::Slider { flip: true },
            ),
            (
                serde_json::json!({ "type": "Cylindrical" }),
                MateKind::Cylindrical { flip: false },
            ),
            (
                serde_json::json!({ "type": "Planar", "flip": true }),
                MateKind::Planar { flip: true },
            ),
            (serde_json::json!({ "type": "Ball" }), MateKind::Ball),
        ] {
            let k: MateKind = serde_json::from_value(v.clone()).unwrap();
            assert_eq!(k, expect);
            assert_eq!(serde_json::to_value(&k).unwrap(), v);
            assert!(k.is_numeric());
        }
        assert!(!MateKind::Fastened {
            flip: false,
            rotation_deg: 0.0
        }
        .is_numeric());
    }
}
