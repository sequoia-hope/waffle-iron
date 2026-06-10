//! Generator for the indirect `less_than_on_{x,y,z}` family
//! (Cherchi 2020 Appendix B: POINTCOMPARE_ON_X — "the Y and Z versions
//! can be obtained by replacing the subscripts").
//!
//! `pointCompare_on_c(a, b) = sign(a.c − b.c)`. With `a = λ_a/d_a`,
//! `b = λ_b/d_b`: `a.c − b.c = (λ_ac·d_b − λ_bc·d_a) / (d_a·d_b)`, so
//!
//! - both implicit (Appendix B `LL`/`LT`/`TT` forms):
//!   `∆ = d_b·λ_ac − d_a·λ_bc`, `D′ = d_a·d_b` (both odd → both flip);
//! - second explicit (`LE`/`TE` forms): `∆ = λ_ac − p_bc·d_a`,
//!   `D′ = d_a`;
//! - both explicit: "can be implemented without any explicit
//!   subtraction, and hence without the need for a filter" — the EE arm
//!   delegates to a direct f64 comparison (exact: coordinates are
//!   exact by definition, Attene §4).
//!
//! Canonical rank L < T < E (the Appendix B instance set: LE, LL, LT,
//! TE, TT); the comparator is antisymmetric (`sign(a−b) = −sign(b−a)`),
//! so a canonicalizing swap flips the result.

use crate::codegen::{emit_exact_arm_body, emit_instance, InstanceEmit};
use crate::ir::{Beta, Operand, Program};
use crate::orient3d::LambdaSpec;

/// Point type with the comparator canonical rank: L < T < E.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Ty {
    L,
    T,
    E,
}

impl Ty {
    fn letter(self) -> char {
        match self {
            Ty::L => 'l',
            Ty::T => 't',
            Ty::E => 'e',
        }
    }

    fn variant_pattern(self) -> &'static str {
        match self {
            Ty::L => "GenericPoint3D::Lpi(_)",
            Ty::T => "GenericPoint3D::Tpi(_)",
            Ty::E => "GenericPoint3D::Explicit(_)",
        }
    }
}

/// All 6 sorted type patterns (L ≤ T ≤ E), EE last.
#[allow(clippy::needless_range_loop)] // the i0 ≤ i1 chain is clearer with indices
pub fn patterns() -> Vec<[Ty; 2]> {
    const ALL: [Ty; 3] = [Ty::L, Ty::T, Ty::E];
    let mut out = Vec::new();
    for i0 in 0..3 {
        for i1 in i0..3 {
            out.push([ALL[i0], ALL[i1]]);
        }
    }
    out
}

fn suffix(p: [Ty; 2]) -> String {
    p.iter().map(|t| t.letter()).collect()
}

const AXES: [&str; 3] = ["x", "y", "z"];

pub struct Instance {
    pub pattern: [Ty; 2],
    pub axis: usize,
    pub name: String,
    pub delta: f64,
    pub degree: u32,
    pub code: String,
}

fn lambda_inputs_1(
    prog: &mut Program,
    slot: usize,
    spec: &LambdaSpec,
    axis: usize,
) -> (Operand, Operand) {
    let l = prog.input(
        format!("l{slot}.l[{axis}]"),
        format!("li{slot}.l[{axis}]"),
        format!("le{slot}.l[{axis}].clone()"),
        spec.l_sfe,
        spec.l_deg,
        Beta::Cached(format!("l{slot}.beta")),
    );
    let d = prog.input(
        format!("l{slot}.d"),
        format!("li{slot}.d"),
        format!("le{slot}.d.clone()"),
        spec.d_sfe,
        spec.d_deg,
        Beta::Covered,
    );
    (l, d)
}

pub fn instance(pattern: [Ty; 2], axis: usize, lpi: &LambdaSpec, tpi: &LambdaSpec) -> Instance {
    assert!(
        pattern[0] != Ty::E,
        "EE has no generated instance (direct f64 comparison)"
    );
    let sfx = suffix(pattern);
    let ax = AXES[axis];
    let name = format!("less_than_on_{ax}_{sfx}");
    let mut prog = Program::default();

    let pick = |ty: Ty| -> &LambdaSpec {
        match ty {
            Ty::L => lpi,
            Ty::T => tpi,
            Ty::E => unreachable!("explicit slot has no lambda spec"),
        }
    };

    let (l0, d0) = lambda_inputs_1(&mut prog, 0, pick(pattern[0]), axis);

    let (out, implicit_slots, parity_slots);
    if pattern[1] == Ty::E {
        // ∆ = λ_0c − p1c·d0, D′ = d0 (Appendix B LE/TE).
        let p1c = prog.raw_factor(
            format!("p1.{ax}()"),
            format!("Iv::point(p1.{ax}())"),
            format!("support::rb(p1.{ax}())"),
        );
        let pd = prog.mul(p1c, d0);
        out = prog.sub(l0, pd);
        implicit_slots = vec![0usize];
        parity_slots = vec![0usize];
    } else {
        // ∆ = d1·λ_0c − d0·λ_1c, D′ = d0·d1 (Appendix B LL/LT/TT).
        let (l1, d1) = lambda_inputs_1(&mut prog, 1, pick(pattern[1]), axis);
        let a = prog.mul(d1, l0);
        let b = prog.mul(d0, l1);
        out = prog.sub(a, b);
        implicit_slots = vec![0usize, 1];
        parity_slots = vec![0usize, 1];
    }

    let doc = format!(
        "/// `{name}` — Cherchi 2020 App. B `pointCompare_on_{}` instance\n\
         /// `{}`: the sign of `a.{ax} − b.{ax}` (Negative ⟺ a < b);\n\
         /// semi-statically filtered f64 tier (Attene §5.1 + App. A).\n",
        ax.to_uppercase(),
        sfx.to_uppercase(),
    );

    let emit = InstanceEmit {
        name: name.clone(),
        const_suffix: name.to_uppercase(),
        arity: 2,
        implicit_slots,
        parity_slots,
        prog,
        out,
        doc,
    };
    let (code, delta, degree) = emit_instance(&emit);
    Instance {
        pattern,
        axis,
        name,
        delta,
        degree,
        code,
    }
}

// =====================================================================
// Family dispatchers (per axis)
// =====================================================================

fn tuple_pattern(pattern: [Ty; 2]) -> String {
    let pats: Vec<&str> = pattern.iter().map(|t| t.variant_pattern()).collect();
    format!("({})", pats.join(", "))
}

fn ee_delegate(axis: usize) -> String {
    let ax = AXES[axis];
    format!(
        "{{\n            let (x0, x1) = (p0.{ax}(), p1.{ax}());\n\
         \x20           if x0 < x1 {{\n                Sign::Negative\n\
         \x20           }} else if x0 > x1 {{\n                Sign::Positive\n\
         \x20           }} else {{\n                Sign::Zero\n            }}\n        }}"
    )
}

fn emit_dispatchers(axis: usize, insts: &[&Instance]) -> String {
    let mut code = String::new();
    let ee_pat = "(GenericPoint3D::Explicit(p0), GenericPoint3D::Explicit(p1))";
    let unreachable_arm =
        "        _ => unreachable!(\"non-canonical argument order in generated dispatcher\"),\n";
    let ax = AXES[axis];

    code.push_str(&format!(
        "/// Full tier dispatch over the canonical (L < T < E sorted) argument order.\n\
         pub(super) fn dispatch_less_than_on_{ax}_canonical(a: &GenericPoint3D, b: &GenericPoint3D) -> Sign {{\n\
         \x20   match (a, b) {{\n"
    ));
    for inst in insts {
        code.push_str(&format!(
            "        {} => {}(a, b),\n",
            tuple_pattern(inst.pattern),
            inst.name
        ));
    }
    code.push_str(&format!("        {ee_pat} => {},\n", ee_delegate(axis)));
    code.push_str(unreachable_arm);
    code.push_str("    }\n}\n\n");

    code.push_str(&format!(
        "/// Inexact (certified, non-exact-arithmetic) tiers only; `None` = both\n\
         /// uncertain. Exposed for the filter-soundness oracle. (The EE arm is\n\
         /// a direct f64 comparison — exact, never uncertain.)\n\
         pub(super) fn dispatch_less_than_on_{ax}_filtered_canonical(a: &GenericPoint3D, b: &GenericPoint3D) -> Option<Sign> {{\n\
         \x20   match (a, b) {{\n"
    ));
    for inst in insts {
        code.push_str(&format!(
            "        {} => {}_inexact(a, b),\n",
            tuple_pattern(inst.pattern),
            inst.name
        ));
    }
    code.push_str(&format!(
        "        {ee_pat} => Some({}),\n",
        ee_delegate(axis)
    ));
    code.push_str(unreachable_arm);
    code.push_str("    }\n}\n\n");

    code.push_str(&format!(
        "/// Exact tier only — ground truth for the soundness oracle.\n\
         pub(super) fn dispatch_less_than_on_{ax}_exact_canonical(a: &GenericPoint3D, b: &GenericPoint3D) -> Sign {{\n\
         \x20   match (a, b) {{\n"
    ));
    for inst in insts {
        let implicit_slots: Vec<usize> = (0..2).filter(|&i| inst.pattern[i] != Ty::E).collect();
        code.push_str(&format!(
            "        {} => {{\n{}        }}\n",
            tuple_pattern(inst.pattern),
            emit_exact_arm_body(&inst.name, 2, &implicit_slots)
        ));
    }
    code.push_str(&format!("        {ee_pat} => {},\n", ee_delegate(axis)));
    code.push_str(unreachable_arm);
    code.push_str("    }\n}\n\n");
    code
}

/// The whole less_than section of the generated file.
pub fn section(lpi: &LambdaSpec, tpi: &LambdaSpec) -> String {
    let mut out = String::new();
    let insts: Vec<Instance> = (0..3)
        .flat_map(|axis| {
            patterns()
                .into_iter()
                .filter(|p| p[0] != Ty::E)
                .map(move |p| (p, axis))
        })
        .map(|(p, axis)| instance(p, axis, lpi, tpi))
        .collect();
    for inst in &insts {
        out.push_str(&inst.code);
    }
    for axis in 0..3 {
        let of_axis: Vec<&Instance> = insts.iter().filter(|i| i.axis == axis).collect();
        out.push_str(&emit_dispatchers(axis, &of_axis));
    }
    out
}

/// (name, δ, degree) for every generated less_than instance.
pub fn instance_table() -> Vec<(String, f64, u32)> {
    let lpi = crate::orient3d::lpi_lambda_spec();
    let tpi = crate::orient3d::tpi_lambda_spec();
    (0..3)
        .flat_map(|axis| {
            patterns()
                .into_iter()
                .filter(|p| p[0] != Ty::E)
                .map(move |p| (p, axis))
        })
        .map(|(p, axis)| {
            let i = instance(p, axis, &lpi, &tpi);
            (i.name, i.delta, i.degree)
        })
        .collect()
}
