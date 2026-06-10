//! Generator for the indirect `orient2d_{xy,yz,zx}` family.
//!
//! ## Clean-room sources
//!
//! - Attene 2025 §4.3 (`refs/text/attene-predicates.txt:171-230`):
//!   the indirect orient2d rewriting. One implicit point `p1 = λ/d`:
//!   `Λ′ = (d·p2x − λx)(d·p3y − λy) − (d·p2y − λy)(d·p3x − λx)`,
//!   `D′ = d²`. All implicit:
//!   `Λ′ = (d1λ2x − d2λ1x)(d1λ3y − d3λ1y) − (d1λ2y − d2λ1y)(d1λ3x − d3λ1x)`,
//!   `D′ = d1²·d2·d3` — the FIRST argument is the determinant pivot and
//!   its `d` appears squared (no sign contribution).
//! - Attene 2025 §4.5 (orient2d3d): the projections drop one coordinate;
//!   `orient2d_xy` projects on (x, y), `_yz` on (y, z), `_zx` on (z, x).
//! - Cherchi 2020 Appendix A (`refs/text/mesh_arrangement.txt:1009-1060`)
//!   — the worked per-instance ORIENT2D_XY polynomials and published
//!   filter constants ("the YZ and ZX versions can be obtained by simply
//!   replacing all the subscripts"). The appendix instances sort the
//!   types L < T < E (e.g. `LLT(pL1, pL2, pT)`, `LTE(pL, pT, pE)`), so
//!   this family canonicalizes with rank L < T < E — pivoting on the
//!   LOWER-degree implicit point gives the tighter polynomial (LTE via
//!   the L pivot is degree 14; a T pivot would be 17).
//!
//! For the one-implicit instances (LEE, TEE) the appendix factors one
//! `d` out of the §4.3 form, halving the degree:
//! `∆ = d·(p2x·p3y − p2y·p3x) + λx·(p2y − p3y) + λy·(p3x − p2x)`,
//! `D′ = d` (odd — contributes to the sign parity). We emit exactly
//! that form; its published δ∆ factor set (raw |p2|, |p3| coordinates
//! plus the two differences) matches our IR factors.
//!
//! `EEE` delegates to the pure adaptive [`crate`]-external predicate
//! (cherchi-rs `predicates::orient2d`, CR10) over the projected
//! coordinates — Attene §6.

use crate::codegen::{emit_exact_arm_body, emit_instance, InstanceEmit};
use crate::ir::{Beta, Operand, Program};
use crate::orient3d::LambdaSpec;

/// Point type with the orient2d canonical rank: L < T < E (the order of
/// Cherchi 2020 Appendix A's instance set).
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Ty {
    /// Line-plane intersection (lowest-degree implicit — the pivot).
    L,
    /// Three-plane intersection.
    T,
    /// Explicit (sorts last).
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

/// All 10 sorted type patterns (L ≤ T ≤ E), EEE last.
#[allow(clippy::needless_range_loop)] // the i0 ≤ i1 ≤ i2 chain is clearer with indices
pub fn patterns() -> Vec<[Ty; 3]> {
    const ALL: [Ty; 3] = [Ty::L, Ty::T, Ty::E];
    let mut out = Vec::new();
    for i0 in 0..3 {
        for i1 in i0..3 {
            for i2 in i1..3 {
                out.push([ALL[i0], ALL[i1], ALL[i2]]);
            }
        }
    }
    out
}

fn suffix(p: [Ty; 3]) -> String {
    p.iter().map(|t| t.letter()).collect()
}

/// An axis-aligned projection: drop one coordinate, keep `(u, v)`.
#[derive(Clone, Copy, Debug)]
pub struct Proj {
    pub name: &'static str,
    /// Axis indices of the kept coordinate pair.
    pub u: usize,
    pub v: usize,
}

pub const PROJECTIONS: [Proj; 3] = [
    Proj {
        name: "xy",
        u: 0,
        v: 1,
    },
    Proj {
        name: "yz",
        u: 1,
        v: 2,
    },
    Proj {
        name: "zx",
        u: 2,
        v: 0,
    },
];

const AXES: [&str; 3] = ["x", "y", "z"];

/// Lambda-value inputs for one implicit slot, restricted to the two
/// projected components plus `d`. Returns `([λu, λv], d)`.
fn lambda_inputs_2(
    prog: &mut Program,
    slot: usize,
    spec: &LambdaSpec,
    proj: Proj,
) -> ([Operand; 2], Operand) {
    let l = [proj.u, proj.v].map(|ax| {
        let beta = if ax == proj.u {
            Beta::Cached(format!("l{slot}.beta"))
        } else {
            Beta::Covered
        };
        prog.input(
            format!("l{slot}.l[{ax}]"),
            format!("li{slot}.l[{ax}]"),
            format!("le{slot}.l[{ax}].clone()"),
            spec.l_sfe,
            spec.l_deg,
            beta,
        )
    });
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

/// Raw explicit coordinate factor `p{slot}.{axis}()`.
fn coord_raw(prog: &mut Program, slot: usize, ax: usize) -> Operand {
    let a = AXES[ax];
    prog.raw_factor(
        format!("p{slot}.{a}()"),
        format!("Iv::point(p{slot}.{a}())"),
        format!("support::rb(p{slot}.{a}())"),
    )
}

/// Translation difference `p{sa}.{axis}() - p{sb}.{axis}()`.
fn coord_diff(prog: &mut Program, sa: usize, sb: usize, ax: usize) -> Operand {
    let a = AXES[ax];
    prog.diff_factor(
        format!("p{sa}.{a}() - p{sb}.{a}()"),
        format!("Iv::point(p{sa}.{a}()) - Iv::point(p{sb}.{a}())"),
        format!("support::rb(p{sa}.{a}()) - support::rb(p{sb}.{a}())"),
    )
}

pub struct Instance {
    pub pattern: [Ty; 3],
    pub proj: Proj,
    pub name: String,
    pub delta: f64,
    pub degree: u32,
    pub code: String,
}

fn spec_for(ty: Ty, lpi: &LambdaSpec, tpi: &LambdaSpec) -> &'static str {
    // Only used for documentation labels.
    let _ = (lpi, tpi);
    match ty {
        Ty::L => "LPI",
        Ty::T => "TPI",
        Ty::E => "explicit",
    }
}

#[allow(clippy::needless_range_loop)] // slot indices feed both names and arrays
pub fn instance(pattern: [Ty; 3], proj: Proj, lpi: &LambdaSpec, tpi: &LambdaSpec) -> Instance {
    assert!(
        pattern[0] != Ty::E,
        "EEE has no generated instance (delegates to CR10 orient2d)"
    );
    let sfx = suffix(pattern);
    let name = format!("orient2d_{}_{sfx}", proj.name);
    let mut prog = Program::default();

    let pick = |ty: Ty| -> &LambdaSpec {
        match ty {
            Ty::L => lpi,
            Ty::T => tpi,
            Ty::E => unreachable!("explicit slot has no lambda spec"),
        }
    };

    // Pivot (slot 0) lambdas — always implicit in generated instances.
    let (l0, d0) = lambda_inputs_2(&mut prog, 0, pick(pattern[0]), proj);

    let one_implicit = pattern[1] == Ty::E; // sorted ⇒ pattern[2] == E too

    let (out, implicit_slots, parity_slots);
    if one_implicit {
        // Factored one-implicit form (Cherchi 2020 App. A, LEE/TEE):
        // ∆ = d·(p1u·p2v − p1v·p2u) + λu·(p1v − p2v) + λv·(p2u − p1u),
        // D′ = d (odd — slot 0 contributes to the parity).
        let p1u = coord_raw(&mut prog, 1, proj.u);
        let p1v = coord_raw(&mut prog, 1, proj.v);
        let p2u = coord_raw(&mut prog, 2, proj.u);
        let p2v = coord_raw(&mut prog, 2, proj.v);
        let dv = coord_diff(&mut prog, 1, 2, proj.v); // p1v − p2v
        let du = coord_diff(&mut prog, 2, 1, proj.u); // p2u − p1u
        let m0 = prog.mul(p1u, p2v);
        let m1 = prog.mul(p1v, p2u);
        let det = prog.sub(m0, m1);
        let t0 = prog.mul(d0, det);
        let t1 = prog.mul(l0[0], dv);
        let t2 = prog.mul(l0[1], du);
        let s = prog.add(t0, t1);
        out = prog.add(s, t2);
        implicit_slots = vec![0usize];
        parity_slots = vec![0usize];
    } else {
        // General pivot form (Attene §4.3 / Cherchi App. A multi-implicit):
        // row_i = (d0·λiu − di·λ0u, d0·λiv − di·λ0v) for implicit slot i,
        //         (d0·piu − λ0u,    d0·piv − λ0v)    for explicit slot i;
        // ∆ = row1u·row2v − row1v·row2u, D′ = d0²·d1·d2 (parity: the
        // implicit non-pivot slots only).
        let mut rows: Vec<[Operand; 2]> = Vec::new();
        let mut imps = vec![0usize];
        let mut pars = Vec::new();
        for i in 1..3 {
            let row = match pattern[i] {
                Ty::E => [proj.u, proj.v].map(|ax| {
                    let p = coord_raw(&mut prog, i, ax);
                    let dp = prog.mul(d0, p);
                    let k = if ax == proj.u { 0 } else { 1 };
                    prog.sub(dp, l0[k])
                }),
                _ => {
                    let (li, di) = lambda_inputs_2(&mut prog, i, pick(pattern[i]), proj);
                    imps.push(i);
                    pars.push(i);
                    [0usize, 1].map(|k| {
                        let a = prog.mul(d0, li[k]);
                        let b = prog.mul(di, l0[k]);
                        prog.sub(a, b)
                    })
                }
            };
            rows.push(row);
        }
        let m0 = prog.mul(rows[0][0], rows[1][1]);
        let m1 = prog.mul(rows[0][1], rows[1][0]);
        out = prog.sub(m0, m1);
        implicit_slots = imps;
        parity_slots = pars;
    }

    let doc = format!(
        "/// `{name}({}, {}, {})` — Cherchi 2020 App. A\n\
         /// instance `{}` over the ({}, {}) projection; semi-statically\n\
         /// filtered f64 tier (Attene §5.1 + App. A).\n",
        spec_for(pattern[0], lpi, tpi),
        spec_for(pattern[1], lpi, tpi),
        spec_for(pattern[2], lpi, tpi),
        sfx.to_uppercase(),
        AXES[proj.u],
        AXES[proj.v],
    );

    let emit = InstanceEmit {
        name: name.clone(),
        const_suffix: name.to_uppercase(),
        arity: 3,
        implicit_slots,
        parity_slots,
        prog,
        out,
        doc,
    };
    let (code, delta, degree) = emit_instance(&emit);
    Instance {
        pattern,
        proj,
        name,
        delta,
        degree,
        code,
    }
}

// =====================================================================
// Family dispatchers (per projection)
// =====================================================================

fn tuple_pattern(pattern: [Ty; 3]) -> String {
    let pats: Vec<&str> = pattern.iter().map(|t| t.variant_pattern()).collect();
    format!("({})", pats.join(", "))
}

fn eee_delegate(proj: Proj) -> String {
    let (u, v) = (AXES[proj.u], AXES[proj.v]);
    format!(
        "Sign::from(crate::predicates::orient2d(\
         Point2::new(p0.{u}(), p0.{v}()), \
         Point2::new(p1.{u}(), p1.{v}()), \
         Point2::new(p2.{u}(), p2.{v}())))"
    )
}

fn emit_dispatchers(proj: Proj, insts: &[&Instance]) -> String {
    let mut code = String::new();
    let eee_pat = "(GenericPoint3D::Explicit(p0), GenericPoint3D::Explicit(p1), \
                   GenericPoint3D::Explicit(p2))";
    let unreachable_arm =
        "        _ => unreachable!(\"non-canonical argument order in generated dispatcher\"),\n";
    let pn = proj.name;

    code.push_str(&format!(
        "/// Full tier dispatch over the canonical (L < T < E sorted) argument order.\n\
         pub(super) fn dispatch_orient2d_{pn}_canonical(a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D) -> Sign {{\n\
         \x20   match (a, b, c) {{\n"
    ));
    for inst in insts {
        code.push_str(&format!(
            "        {} => {}(a, b, c),\n",
            tuple_pattern(inst.pattern),
            inst.name
        ));
    }
    code.push_str(&format!("        {eee_pat} => {},\n", eee_delegate(proj)));
    code.push_str(unreachable_arm);
    code.push_str("    }\n}\n\n");

    code.push_str(&format!(
        "/// Inexact (certified, non-exact-arithmetic) tiers only; `None` = both\n\
         /// uncertain. Exposed for the filter-soundness oracle.\n\
         pub(super) fn dispatch_orient2d_{pn}_filtered_canonical(a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D) -> Option<Sign> {{\n\
         \x20   match (a, b, c) {{\n"
    ));
    for inst in insts {
        code.push_str(&format!(
            "        {} => {}_inexact(a, b, c),\n",
            tuple_pattern(inst.pattern),
            inst.name
        ));
    }
    code.push_str(&format!(
        "        {eee_pat} => Some({}),\n",
        eee_delegate(proj)
    ));
    code.push_str(unreachable_arm);
    code.push_str("    }\n}\n\n");

    code.push_str(&format!(
        "/// Exact tier only — ground truth for the soundness oracle. The\n\
         /// EEE arm delegates to the adaptive CR10 predicate (also exact).\n\
         pub(super) fn dispatch_orient2d_{pn}_exact_canonical(a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D) -> Sign {{\n\
         \x20   match (a, b, c) {{\n"
    ));
    for inst in insts {
        let implicit_slots: Vec<usize> = (0..3).filter(|&i| inst.pattern[i] != Ty::E).collect();
        code.push_str(&format!(
            "        {} => {{\n{}        }}\n",
            tuple_pattern(inst.pattern),
            emit_exact_arm_body(&inst.name, 3, &implicit_slots)
        ));
    }
    code.push_str(&format!("        {eee_pat} => {},\n", eee_delegate(proj)));
    code.push_str(unreachable_arm);
    code.push_str("    }\n}\n\n");
    code
}

/// The whole orient2d section of the generated file.
pub fn section(lpi: &LambdaSpec, tpi: &LambdaSpec) -> String {
    let mut out = String::new();
    let insts: Vec<Instance> = PROJECTIONS
        .iter()
        .flat_map(|&proj| {
            patterns()
                .into_iter()
                .filter(|p| p[0] != Ty::E)
                .map(move |p| (p, proj))
        })
        .map(|(p, proj)| instance(p, proj, lpi, tpi))
        .collect();
    for inst in &insts {
        out.push_str(&inst.code);
    }
    for proj in PROJECTIONS {
        let of_proj: Vec<&Instance> = insts.iter().filter(|i| i.proj.name == proj.name).collect();
        out.push_str(&emit_dispatchers(proj, &of_proj));
    }
    out
}

/// (name, δ, degree) for every generated orient2d instance.
pub fn instance_table() -> Vec<(String, f64, u32)> {
    let lpi = crate::orient3d::lpi_lambda_spec();
    let tpi = crate::orient3d::tpi_lambda_spec();
    PROJECTIONS
        .iter()
        .flat_map(|&proj| {
            patterns()
                .into_iter()
                .filter(|p| p[0] != Ty::E)
                .map(move |p| (p, proj))
        })
        .map(|(p, proj)| {
            let i = instance(p, proj, &lpi, &tpi);
            (i.name, i.delta, i.degree)
        })
        .collect()
}
