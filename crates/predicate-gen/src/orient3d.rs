//! Generator for the indirect `orient3d` family.
//!
//! ## Lambda formulations (clean-room sources)
//!
//! **LPI** (line-plane intersection) — Attene 2025 §4.2
//! (`refs/text/attene-predicates.txt:146-170`) and, identically, Cherchi
//! 2020 §4.2.2 (`refs/text/mesh_arrangement.txt:346-369`). The two
//! papers agree (Cherchi cites Attene for this construction):
//!
//! ```text
//! line (p, q), plane (r, s, t)        [paper: q1 = p, q2 = q]
//! d  = det[ p − q ; s − r ; t − r ]
//! n  = det[ p − r ; s − r ; t − r ]
//! λx = d·px + n·qx − n·px  =  d·px − n·(px − qx)
//! (and same for y, z); point undefined iff d == 0
//! ```
//!
//! **TPI** (three-plane intersection) — Cherchi 2020 §4.2.2
//! (`mesh_arrangement.txt:371-394`):
//!
//! ```text
//! triangles v, w, u;  nv = (v2 − v1) × (v3 − v2)   (same for w, u)
//! pv = nv · v1                                      (same for w, u)
//! dT  = det[ nv ; nw ; nu ]
//! λTx = det[[pv, nvy, nvz], [pw, nwy, nwz], [pu, nuy, nuz]]
//! λTy = det[[nvx, pv, nvz], [nwx, pw, nwz], [nux, pu, nuz]]
//! λTz = det[[nvx, nvy, pv], [nwx, nwy, pw], [nux, nuy, pu]]
//! (Cramer's rule for the linear system n·x = p); undefined iff dT == 0
//! ```
//!
//! ## orient3d rewriting (Attene 2025 §4.6)
//!
//! `Λ = det[p1−p4; p2−p4; p3−p4]`, `pi = λi/di` (explicit ⇒ `λ = coords,
//! d = 1`). Multiplying each row by its denominators:
//!
//! - `p4` explicit: implicit row `i` becomes `λi − di⊗p4`, explicit rows
//!   stay as translation differences `pi − p4` (this is exactly the
//!   matrix of Attene §4.6, `attene-predicates.txt:239-252`); the
//!   denominator is `D′ = Π di` over implicit rows.
//! - `p4` implicit: row `i` becomes `d4·λi − di·λ4` (explicit rows would
//!   be `d4·pi − λ4`, but canonicalization makes "p4 implicit" imply
//!   "all implicit"); `D′ = d1·d2·d3·d4³`.
//!
//! The sign of `D′` is resolved by counting negative `d`s with
//! multiplicity (Attene §5.1, lines 281-286): each implicit slot
//! contributes once (`d4`'s multiplicity 3 is odd ⇒ also once).
//!
//! ## Instance reduction (Attene 2025 §6)
//!
//! `orient3d` is alternating in all four arguments, so a stable sort by
//! point type (TPI < LPI < Explicit) with sign parity reduces 3⁴ = 81
//! configurations to the 15 sorted patterns; EEEE delegates to the
//! existing pure adaptive predicate (CR6). The remaining 14 instances
//! are generated below.

use crate::codegen::{
    emit_beta, emit_exact_lets, emit_f64_lets, emit_iv_lets, exact_out, f64_literal, f64_out,
};
use crate::fpg::Sfe;
use crate::ir::{Beta, Operand, Program};

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Ty {
    /// Three-plane intersection (most implicit — sorts first).
    T,
    /// Line-plane intersection.
    L,
    /// Explicit (sorts last so the `p4` slot stays explicit whenever any
    /// argument is — the tightest filter form, per Attene §4.6).
    E,
}

impl Ty {
    fn letter(self) -> char {
        match self {
            Ty::T => 't',
            Ty::L => 'l',
            Ty::E => 'e',
        }
    }

    fn variant_pattern(self) -> &'static str {
        match self {
            Ty::T => "GenericPoint3D::Tpi(_)",
            Ty::L => "GenericPoint3D::Lpi(_)",
            Ty::E => "GenericPoint3D::Explicit(_)",
        }
    }
}

/// All 15 sorted type patterns (T ≤ L ≤ E), EEEE last.
#[allow(clippy::needless_range_loop)] // the i0 ≤ i1 ≤ i2 ≤ i3 chain is clearer with indices
pub fn patterns() -> Vec<[Ty; 4]> {
    const ALL: [Ty; 3] = [Ty::T, Ty::L, Ty::E];
    let mut out = Vec::new();
    for i0 in 0..3 {
        for i1 in i0..3 {
            for i2 in i1..3 {
                for i3 in i2..3 {
                    out.push([ALL[i0], ALL[i1], ALL[i2], ALL[i3]]);
                }
            }
        }
    }
    out
}

fn suffix(p: [Ty; 4]) -> String {
    p.iter().map(|t| t.letter()).collect()
}

// =====================================================================
// Shared program-building helpers
// =====================================================================

const AXES: [&str; 3] = ["x", "y", "z"];

fn pt_raw(prog: &mut Program, name: &str) -> [Operand; 3] {
    AXES.map(|ax| {
        prog.raw_factor(
            format!("{name}.{ax}()"),
            format!("Iv::point({name}.{ax}())"),
            format!("support::rb({name}.{ax}())"),
        )
    })
}

fn pt_diff(prog: &mut Program, a: &str, b: &str) -> [Operand; 3] {
    AXES.map(|ax| {
        prog.diff_factor(
            format!("{a}.{ax}() - {b}.{ax}()"),
            format!("Iv::point({a}.{ax}()) - Iv::point({b}.{ax}())"),
            format!("support::rb({a}.{ax}()) - support::rb({b}.{ax}())"),
        )
    })
}

// =====================================================================
// Lambda specs
// =====================================================================

pub struct LambdaSpec {
    pub prog: Program,
    pub l_out: [Operand; 3],
    pub d_out: Operand,
    pub l_sfe: Sfe,
    pub l_deg: u32,
    pub d_sfe: Sfe,
    pub d_deg: u32,
    /// Ready-to-emit `δ(1)` for the d-sign semi-static filter.
    pub d_delta: f64,
}

fn analyze_lambda(prog: Program, l_out: [Operand; 3], d_out: Operand) -> LambdaSpec {
    let (l_sfe_x, l_deg) = prog.analyze(l_out[0]);
    // λy / λz have the same structure over different atoms — identical
    // Sfe by symmetry. Assert rather than assume.
    for o in [l_out[1], l_out[2]] {
        let (s, d) = prog.analyze(o);
        assert_eq!((s, d), (l_sfe_x, l_deg), "lambda components asymmetric");
    }
    let (d_sfe, d_deg) = prog.analyze(d_out);
    let d_delta = d_sfe.delta(d_deg);
    LambdaSpec {
        prog,
        l_out,
        d_out,
        l_sfe: l_sfe_x,
        l_deg,
        d_sfe,
        d_deg,
        d_delta,
    }
}

/// LPI lambdas (Attene §4.2 = Cherchi §4.2.2). The β factor set is
/// exactly Cherchi's `δL` (`mesh_arrangement.txt:352-356`): the raw
/// coordinates of `p` (= q1) plus the differences `p−q`, `s−r`, `t−r`,
/// `p−r` — 15 values.
pub fn lpi_lambda_spec() -> LambdaSpec {
    let mut prog = Program::default();
    let p = pt_raw(&mut prog, "p");
    let a = pt_diff(&mut prog, "p", "q"); // q1 − q2
    let b = pt_diff(&mut prog, "s", "r");
    let c = pt_diff(&mut prog, "t", "r");
    let e = pt_diff(&mut prog, "p", "r"); // q1 − r
    let d = prog.det3([a, b, c]);
    let n = prog.det3([e, b, c]);
    let l = [0, 1, 2].map(|ax| {
        let dp = prog.mul(d, p[ax]);
        let na = prog.mul(n, a[ax]);
        prog.sub(dp, na) // λ = d·p − n·(p − q)
    });
    analyze_lambda(prog, l, d)
}

/// TPI lambdas (Cherchi §4.2.2). The β factor set is exactly Cherchi's
/// `δT` (`mesh_arrangement.txt:371-394` filter description): raw `v1`,
/// `w1`, `u1` plus the edge differences of each triangle — 27 values.
pub fn tpi_lambda_spec() -> LambdaSpec {
    let mut prog = Program::default();
    let v1 = pt_raw(&mut prog, "v[0]");
    let w1 = pt_raw(&mut prog, "w[0]");
    let u1 = pt_raw(&mut prog, "u[0]");
    let va = pt_diff(&mut prog, "v[1]", "v[0]");
    let vb = pt_diff(&mut prog, "v[2]", "v[1]");
    let wa = pt_diff(&mut prog, "w[1]", "w[0]");
    let wb = pt_diff(&mut prog, "w[2]", "w[1]");
    let ua = pt_diff(&mut prog, "u[1]", "u[0]");
    let ub = pt_diff(&mut prog, "u[2]", "u[1]");
    let nv = prog.cross(va, vb);
    let nw = prog.cross(wa, wb);
    let nu = prog.cross(ua, ub);
    let pv = prog.dot(nv, v1);
    let pw = prog.dot(nw, w1);
    let pu = prog.dot(nu, u1);
    let d = prog.det3([nv, nw, nu]);
    let lx = prog.det3([[pv, nv[1], nv[2]], [pw, nw[1], nw[2]], [pu, nu[1], nu[2]]]);
    let ly = prog.det3([[nv[0], pv, nv[2]], [nw[0], pw, nw[2]], [nu[0], pu, nu[2]]]);
    let lz = prog.det3([[nv[0], nv[1], pv], [nw[0], nw[1], pw], [nu[0], nu[1], pu]]);
    analyze_lambda(prog, [lx, ly, lz], d)
}

// =====================================================================
// Instance generation
// =====================================================================

pub struct Instance {
    pub pattern: [Ty; 4],
    pub suffix: String,
    pub delta: f64,
    pub degree: u32,
    pub code: String,
}

struct SlotVals {
    /// Lambda component operands for implicit slots; coordinate operands
    /// (raw or diff) for explicit non-p4 slots; raw coords for explicit
    /// p4.
    l: [Operand; 3],
    /// Denominator operand (implicit slots only).
    d: Option<Operand>,
}

fn lambda_inputs(
    prog: &mut Program,
    slot: usize,
    spec_l: (Sfe, u32),
    spec_d: (Sfe, u32),
) -> SlotVals {
    let l = [0usize, 1, 2].map(|ax| {
        let beta = if ax == 0 {
            Beta::Cached(format!("l{slot}.beta"))
        } else {
            Beta::Covered
        };
        prog.input(
            format!("l{slot}.l[{ax}]"),
            format!("li{slot}.l[{ax}]"),
            format!("le{slot}.l[{ax}].clone()"),
            spec_l.0,
            spec_l.1,
            beta,
        )
    });
    let d = prog.input(
        format!("l{slot}.d"),
        format!("li{slot}.d"),
        format!("le{slot}.d.clone()"),
        spec_d.0,
        spec_d.1,
        Beta::Covered,
    );
    SlotVals { l, d: Some(d) }
}

fn slot_arg(i: usize) -> &'static str {
    ["a", "b", "c", "d"][i]
}

pub fn instance(pattern: [Ty; 4], lpi: &LambdaSpec, tpi: &LambdaSpec) -> Instance {
    assert!(
        pattern.iter().any(|&t| t != Ty::E),
        "EEEE has no generated instance (delegates to CR6)"
    );
    let sfx = suffix(pattern);
    let mut prog = Program::default();

    // p4 raw coordinates (only when p4 is explicit; used by every
    // implicit row's `λ − d⊗p4` entries).
    let p4_explicit = pattern[3] == Ty::E;
    let p4_raw = if p4_explicit {
        Some(pt_raw(&mut prog, "p3"))
    } else {
        None
    };

    // Per-slot value operands.
    let mut slots: Vec<SlotVals> = Vec::new();
    for (i, &ty) in pattern.iter().enumerate() {
        let sv = match ty {
            Ty::L => lambda_inputs(&mut prog, i, (lpi.l_sfe, lpi.l_deg), (lpi.d_sfe, lpi.d_deg)),
            Ty::T => lambda_inputs(&mut prog, i, (tpi.l_sfe, tpi.l_deg), (tpi.d_sfe, tpi.d_deg)),
            Ty::E => {
                if i == 3 {
                    SlotVals {
                        l: p4_raw.expect("p4 raw coords exist when p4 explicit"),
                        d: None,
                    }
                } else {
                    // Translation differences pi − p4 (p4 is explicit
                    // whenever any slot is, thanks to canonical sorting).
                    SlotVals {
                        l: pt_diff(&mut prog, &format!("p{i}"), "p3"),
                        d: None,
                    }
                }
            }
        };
        slots.push(sv);
    }

    // Rows of Λ′.
    let mut rows: Vec<[Operand; 3]> = Vec::new();
    if p4_explicit {
        let p4 = p4_raw.unwrap();
        for slot in slots.iter().take(3) {
            let row = match slot.d {
                Some(di) => [0usize, 1, 2].map(|ax| {
                    let dp = prog.mul(di, p4[ax]);
                    prog.sub(slot.l[ax], dp) // λi − di·p4
                }),
                None => slot.l, // pi − p4 differences
            };
            rows.push(row);
        }
    } else {
        let d4 = slots[3].d.expect("p4 implicit");
        let l4 = slots[3].l;
        for slot in slots.iter().take(3) {
            let di = slot.d.expect("all slots implicit when p4 is");
            let row = [0usize, 1, 2].map(|ax| {
                let a = prog.mul(d4, slot.l[ax]);
                let b = prog.mul(di, l4[ax]);
                prog.sub(a, b) // d4·λi − di·λ4
            });
            rows.push(row);
        }
    }
    let lam = prog.det3([rows[0], rows[1], rows[2]]);
    let (sfe, degree) = prog.analyze(lam);
    let delta = sfe.delta(degree);

    // ---- emit ----
    let up_sfx = sfx.to_uppercase();
    let mut code = String::new();

    // Constants.
    code.push_str(&format!(
        "/// `δ(1)` and degree `k` for `orient3d_{sfx}`: Λ′ bound {:.3e},\n\
         /// propagated error {:.3e} (FPG analysis), degree {} in the β\n\
         /// factors. Runtime threshold: `ε = δ·β^k` (Attene App. A).\n\
         pub(super) const DELTA_{up_sfx}: f64 = {};\n\
         pub(super) const DEGREE_{up_sfx}: i32 = {};\n\n",
        sfe.bound,
        sfe.error,
        degree,
        f64_literal(delta),
        degree
    ));

    // Parameter lists.
    let mut fparams: Vec<String> = Vec::new();
    let mut iparams: Vec<String> = Vec::new();
    let mut eparams: Vec<String> = Vec::new();
    for (i, &ty) in pattern.iter().enumerate() {
        match ty {
            Ty::E => {
                fparams.push(format!("p{i}: &Point3"));
                iparams.push(format!("p{i}: &Point3"));
                eparams.push(format!("p{i}: &Point3"));
            }
            _ => {
                fparams.push(format!("l{i}: &LambdaF64"));
                iparams.push(format!("li{i}: &LambdaIv"));
                eparams.push(format!("le{i}: &LambdaExact"));
            }
        }
    }
    let implicit_slots: Vec<usize> = (0..4).filter(|&i| pattern[i] != Ty::E).collect();

    // Filtered tier.
    code.push_str(&format!(
        "/// Semi-statically filtered f64 tier (Attene §5.1 + App. A).\n\
         pub(super) fn orient3d_{sfx}_filtered({}) -> Option<Sign> {{\n",
        fparams.join(", ")
    ));
    for &i in &implicit_slots {
        code.push_str(&format!(
            "    if !l{i}.d_reliable {{\n        return None;\n    }}\n"
        ));
    }
    code.push_str(&emit_f64_lets(&prog));
    code.push_str(&emit_beta(&prog));
    code.push_str(&format!(
        "    let eps = DELTA_{up_sfx} * beta.powi(DEGREE_{up_sfx}) + support::SUBNORMAL_GUARD;\n\
         \x20   if !eps.is_finite() {{\n        return None;\n    }}\n\
         \x20   let lam = {};\n\
         \x20   let mut s = if lam > eps {{\n\
         \x20       Sign::Positive\n\
         \x20   }} else if lam < -eps {{\n\
         \x20       Sign::Negative\n\
         \x20   }} else {{\n\
         \x20       return None;\n\
         \x20   }};\n",
        f64_out(lam)
    ));
    for &i in &implicit_slots {
        code.push_str(&format!(
            "    if l{i}.d < 0.0 {{\n        s = s.flipped();\n    }}\n"
        ));
    }
    code.push_str("    Some(s)\n}\n\n");

    // Interval tier (Attene §5.2): dynamic filter when the semi-static
    // worst-case threshold is too pessimistic (deep TPI instances).
    code.push_str(&format!(
        "/// Interval (dynamic-filter) tier (Attene §5.2). `None` = sign\n\
         /// ambiguous, fall through to exact rationals.\n\
         pub(super) fn orient3d_{sfx}_interval({}) -> Option<Sign> {{\n",
        iparams.join(", ")
    ));
    for &i in &implicit_slots {
        code.push_str(&format!(
            "    let d{i} = li{i}.d.sign()?;\n\
             \x20   if d{i} == Sign::Zero {{\n        return Some(Sign::Undefined);\n    }}\n"
        ));
    }
    code.push_str(&emit_iv_lets(&prog));
    code.push_str(&format!(
        "    let mut s = {}.sign()?;\n\
         \x20   if s == Sign::Zero {{\n        return Some(Sign::Zero);\n    }}\n",
        f64_out(lam)
    ));
    for &i in &implicit_slots {
        code.push_str(&format!(
            "    if d{i} == Sign::Negative {{\n        s = s.flipped();\n    }}\n"
        ));
    }
    code.push_str("    Some(s)\n}\n\n");

    // Exact tier.
    code.push_str(&format!(
        "/// Exact rational tier (Attene §5.3). Caller guarantees every\n\
         /// implicit `d != 0` (else the predicate is Undefined).\n\
         pub(super) fn orient3d_{sfx}_exact({}) -> Sign {{\n",
        eparams.join(", ")
    ));
    code.push_str(&emit_exact_lets(&prog));
    code.push_str(&format!(
        "    let mut s = Sign::of_rbig(&{});\n\
         \x20   if s == Sign::Zero {{\n        return Sign::Zero;\n    }}\n",
        exact_out(lam)
    ));
    for &i in &implicit_slots {
        code.push_str(&format!(
            "    if Sign::of_rbig(&le{i}.d) == Sign::Negative {{\n        s = s.flipped();\n    }}\n"
        ));
    }
    code.push_str("    s\n}\n\n");

    // Per-instance inexact dispatcher: semi-static filter, then interval
    // (Attene §5: try the cheapest certifying model first).
    code.push_str(&format!(
        "/// Inexact tiers: semi-static filter (§5.1), then intervals (§5.2).\n\
         pub(super) fn orient3d_{sfx}_inexact(a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D, d: &GenericPoint3D) -> Option<Sign> {{\n"
    ));
    let mut fargs: Vec<String> = Vec::new();
    let mut iargs: Vec<String> = Vec::new();
    let mut eargs: Vec<String> = Vec::new();
    for (i, &ty) in pattern.iter().enumerate() {
        match ty {
            Ty::E => {
                code.push_str(&format!(
                    "    let p{i} = {}.expect_explicit();\n",
                    slot_arg(i)
                ));
                fargs.push(format!("&p{i}"));
                iargs.push(format!("&p{i}"));
                eargs.push(format!("&p{i}"));
            }
            _ => {
                fargs.push(format!("{}.lambda_f64()", slot_arg(i)));
                iargs.push(format!("{}.lambda_iv()", slot_arg(i)));
                eargs.push(format!("&le{i}"));
            }
        }
    }
    code.push_str(&format!(
        "    if let Some(s) = orient3d_{sfx}_filtered({}) {{\n        return Some(s);\n    }}\n\
         \x20   orient3d_{sfx}_interval({})\n}}\n\n",
        fargs.join(", "),
        iargs.join(", ")
    ));

    // Per-instance full dispatcher: inexact tiers, then exact (with
    // undefined-d detection per Attene §5.3).
    code.push_str(&format!(
        "/// Full tier dispatcher: inexact tiers, then exact rationals.\n\
         pub(super) fn orient3d_{sfx}(a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D, d: &GenericPoint3D) -> Sign {{\n\
         \x20   if let Some(s) = orient3d_{sfx}_inexact(a, b, c, d) {{\n        return s;\n    }}\n"
    ));
    for (i, &ty) in pattern.iter().enumerate() {
        match ty {
            Ty::E => code.push_str(&format!(
                "    let p{i} = {}.expect_explicit();\n",
                slot_arg(i)
            )),
            _ => code.push_str(&format!(
                "    let le{i} = {}.lambda_exact();\n\
                 \x20   if le{i}.is_undefined() {{\n        return Sign::Undefined;\n    }}\n",
                slot_arg(i)
            )),
        }
    }
    code.push_str(&format!(
        "    orient3d_{sfx}_exact({})\n}}\n\n",
        eargs.join(", ")
    ));

    Instance {
        pattern,
        suffix: sfx,
        delta,
        degree,
        code,
    }
}

// =====================================================================
// Lambda function emission
// =====================================================================

fn emit_lambda_fns(kind: &str, params: &str, spec: &LambdaSpec, doc: &str) -> String {
    let up_kind = kind.to_uppercase();
    let mut code = String::new();
    code.push_str(&format!(
        "/// `δ(1)` and degree for the {up_kind} denominator's d-sign filter\n\
         /// (Attene §5.1: the d filters run before anything else).\n\
         pub(super) const DELTA_{up_kind}_D: f64 = {};\n\
         pub(super) const DEGREE_{up_kind}_D: i32 = {};\n\n",
        f64_literal(spec.d_delta),
        spec.d_deg
    ));
    code.push_str(doc);
    code.push_str(&format!(
        "pub(super) fn {kind}_lambda_f64({params}) -> LambdaF64 {{\n"
    ));
    code.push_str(&emit_f64_lets(&spec.prog));
    code.push_str(&emit_beta(&spec.prog));
    code.push_str(&format!(
        "    let d = {};\n\
         \x20   let eps = DELTA_{up_kind}_D * beta.powi(DEGREE_{up_kind}_D) + support::SUBNORMAL_GUARD;\n\
         \x20   let d_reliable = eps.is_finite() && (d > eps || d < -eps);\n\
         \x20   LambdaF64 {{\n\
         \x20       l: [{}, {}, {}],\n\
         \x20       d,\n\
         \x20       beta,\n\
         \x20       d_reliable,\n\
         \x20   }}\n}}\n\n",
        f64_out(spec.d_out),
        f64_out(spec.l_out[0]),
        f64_out(spec.l_out[1]),
        f64_out(spec.l_out[2]),
    ));
    code.push_str(&format!(
        "/// Interval lambdas — same polynomials over `Iv` (Attene §5.2,\n\
         /// cached per point per §5.4).\n\
         pub(super) fn {kind}_lambda_iv({params}) -> LambdaIv {{\n"
    ));
    code.push_str(&emit_iv_lets(&spec.prog));
    code.push_str(&format!(
        "    LambdaIv {{\n\
         \x20       l: [{}, {}, {}],\n\
         \x20       d: {},\n\
         \x20   }}\n}}\n\n",
        f64_out(spec.l_out[0]),
        f64_out(spec.l_out[1]),
        f64_out(spec.l_out[2]),
        f64_out(spec.d_out),
    ));
    code.push_str(&format!(
        "/// Exact rational lambdas — same polynomials over `RBig`.\n\
         pub(super) fn {kind}_lambda_exact({params}) -> LambdaExact {{\n"
    ));
    code.push_str(&emit_exact_lets(&spec.prog));
    code.push_str(&format!(
        "    LambdaExact {{\n\
         \x20       l: [{}, {}, {}],\n\
         \x20       d: {},\n\
         \x20   }}\n}}\n\n",
        exact_out(spec.l_out[0]),
        exact_out(spec.l_out[1]),
        exact_out(spec.l_out[2]),
        exact_out(spec.d_out),
    ));
    code
}

// =====================================================================
// Canonical dispatchers
// =====================================================================

fn tuple_pattern(pattern: [Ty; 4]) -> String {
    let pats: Vec<&str> = pattern.iter().map(|t| t.variant_pattern()).collect();
    format!("({})", pats.join(", "))
}

fn emit_dispatchers(insts: &[Instance]) -> String {
    let mut code = String::new();
    let eeee_pat = "(GenericPoint3D::Explicit(p0), GenericPoint3D::Explicit(p1), \
                    GenericPoint3D::Explicit(p2), GenericPoint3D::Explicit(p3))";
    let unreachable_arm =
        "        _ => unreachable!(\"non-canonical argument order in generated dispatcher\"),\n";

    // Combined dispatcher.
    code.push_str(
        "/// Full tier dispatch over the canonical (type-sorted) argument order.\n\
         pub(super) fn dispatch_canonical(a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D, d: &GenericPoint3D) -> Sign {\n\
         \x20   match (a, b, c, d) {\n",
    );
    for inst in insts {
        code.push_str(&format!(
            "        {} => orient3d_{}(a, b, c, d),\n",
            tuple_pattern(inst.pattern),
            inst.suffix
        ));
    }
    code.push_str(&format!(
        "        {eeee_pat} => Sign::from(crate::predicates::orient3d(*p0, *p1, *p2, *p3)),\n"
    ));
    code.push_str(unreachable_arm);
    code.push_str("    }\n}\n\n");

    // Inexact-tiers dispatcher (semi-static + interval).
    code.push_str(
        "/// Inexact (certified, non-exact-arithmetic) tiers only: semi-static\n\
         /// filter then intervals; `None` = both uncertain. Exposed for the\n\
         /// filter-soundness oracle.\n\
         pub(super) fn dispatch_filtered_canonical(a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D, d: &GenericPoint3D) -> Option<Sign> {\n\
         \x20   match (a, b, c, d) {\n",
    );
    for inst in insts {
        code.push_str(&format!(
            "        {} => orient3d_{}_inexact(a, b, c, d),\n",
            tuple_pattern(inst.pattern),
            inst.suffix
        ));
    }
    code.push_str(&format!(
        "        {eeee_pat} => Some(Sign::from(crate::predicates::orient3d(*p0, *p1, *p2, *p3))),\n"
    ));
    code.push_str(unreachable_arm);
    code.push_str("    }\n}\n\n");

    // Exact-only dispatcher.
    code.push_str(
        "/// Exact tier only — ground truth for the soundness oracle. The\n\
         /// EEEE arm delegates to the adaptive CR6 predicate (also exact).\n\
         pub(super) fn dispatch_exact_canonical(a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D, d: &GenericPoint3D) -> Sign {\n\
         \x20   match (a, b, c, d) {\n",
    );
    for inst in insts {
        let mut body = String::new();
        let mut eargs: Vec<String> = Vec::new();
        for (i, &ty) in inst.pattern.iter().enumerate() {
            match ty {
                Ty::E => eargs.push(format!("&{}.expect_explicit()", slot_arg(i))),
                _ => {
                    body.push_str(&format!(
                        "            let le{i} = {}.lambda_exact();\n\
                         \x20           if le{i}.is_undefined() {{\n                return Sign::Undefined;\n            }}\n",
                        slot_arg(i)
                    ));
                    eargs.push(format!("&le{i}"));
                }
            }
        }
        code.push_str(&format!(
            "        {} => {{\n{body}            orient3d_{}_exact({})\n        }}\n",
            tuple_pattern(inst.pattern),
            inst.suffix,
            eargs.join(", ")
        ));
    }
    code.push_str(&format!(
        "        {eeee_pat} => Sign::from(crate::predicates::orient3d(*p0, *p1, *p2, *p3)),\n"
    ));
    code.push_str(unreachable_arm);
    code.push_str("    }\n}\n");
    code
}

// =====================================================================
// Section generation (assembled into the file by `crate::generate_file`)
// =====================================================================

/// The orient3d section of the generated file: lambda evaluators,
/// 14 canonical instances, and the canonical dispatchers.
pub fn section(lpi: &LambdaSpec, tpi: &LambdaSpec) -> String {
    let insts: Vec<Instance> = patterns()
        .into_iter()
        .filter(|p| p.iter().any(|&t| t != Ty::E))
        .map(|p| instance(p, lpi, tpi))
        .collect();

    let mut out = String::new();
    out.push_str(&emit_lambda_fns(
        "lpi",
        "p: &Point3, q: &Point3, r: &Point3, s: &Point3, t: &Point3",
        lpi,
        "/// LPI lambdas — Attene 2025 §4.2 / Cherchi 2020 §4.2.2:\n\
         /// `d = det[p−q; s−r; t−r]`, `n = det[p−r; s−r; t−r]`,\n\
         /// `λ = d·p − n·(p−q)`. `beta` is the max |factor| (Cherchi's δL).\n",
    ));
    out.push_str(&emit_lambda_fns(
        "tpi",
        "v: &[Point3; 3], w: &[Point3; 3], u: &[Point3; 3]",
        tpi,
        "/// TPI lambdas — Cherchi 2020 §4.2.2: plane normals\n\
         /// `nv = (v2−v1)×(v3−v2)` (same for w, u), `pv = nv·v1`,\n\
         /// `dT = det[nv; nw; nu]`, `λT` by Cramer column replacement.\n\
         /// `beta` is the max |factor| (Cherchi's δT).\n",
    ));
    for inst in &insts {
        out.push_str(&inst.code);
    }
    out.push_str(&emit_dispatchers(&insts));
    out
}

/// (suffix, δ, degree) for every generated instance — for generator tests
/// and the close-out report.
pub fn instance_table() -> Vec<(String, f64, u32)> {
    let lpi = lpi_lambda_spec();
    let tpi = tpi_lambda_spec();
    patterns()
        .into_iter()
        .filter(|p| p.iter().any(|&t| t != Ty::E))
        .map(|p| {
            let i = instance(p, &lpi, &tpi);
            (i.suffix, i.delta, i.degree)
        })
        .collect()
}

/// Backwards-compatible whole-file generation (delegates to
/// [`crate::generate_file`]).
pub fn generate_file() -> String {
    crate::generate_file()
}
