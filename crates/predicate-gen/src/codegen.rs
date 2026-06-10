//! Rust source emission for [`crate::ir::Program`]s.
//!
//! Emission is deliberately dumb: straight-line `let` bindings, one per
//! input and per step, in program order. Filters, dispatch and struct
//! packing are assembled by the predicate-specific generator
//! ([`crate::orient3d`]).

use crate::ir::{Beta, Operand, Program};

fn f64_ref(o: Operand) -> String {
    match o {
        Operand::Input(i) => format!("i{i}"),
        Operand::Step(s) => format!("t{s}"),
    }
}

fn exact_ref(o: Operand) -> String {
    // Exact steps operate on references: `&a op &b` keeps every RBig
    // binding live for later reuse (no moves).
    match o {
        Operand::Input(i) => format!("&i{i}"),
        Operand::Step(s) => format!("&t{s}"),
    }
}

/// `let` bindings evaluating the whole program in f64, indented by
/// 4 spaces.
pub fn emit_f64_lets(prog: &Program) -> String {
    let mut out = String::new();
    for (i, inp) in prog.inputs.iter().enumerate() {
        out.push_str(&format!("    let i{i} = {};\n", inp.f64_init));
    }
    for (s, st) in prog.steps.iter().enumerate() {
        out.push_str(&format!(
            "    let t{s} = {} {} {};\n",
            f64_ref(st.lhs),
            st.op.symbol(),
            f64_ref(st.rhs)
        ));
    }
    out
}

/// `let` bindings evaluating the whole program over `Iv` (interval
/// arithmetic with operator overloading — `Iv` is `Copy`, so the step
/// emission is identical to the f64 form).
pub fn emit_iv_lets(prog: &Program) -> String {
    let mut out = String::new();
    for (i, inp) in prog.inputs.iter().enumerate() {
        out.push_str(&format!("    let i{i} = {};\n", inp.iv_init));
    }
    for (s, st) in prog.steps.iter().enumerate() {
        out.push_str(&format!(
            "    let t{s} = {} {} {};\n",
            f64_ref(st.lhs),
            st.op.symbol(),
            f64_ref(st.rhs)
        ));
    }
    out
}

/// `let` bindings evaluating the whole program over `RBig`.
pub fn emit_exact_lets(prog: &Program) -> String {
    let mut out = String::new();
    for (i, inp) in prog.inputs.iter().enumerate() {
        out.push_str(&format!("    let i{i} = {};\n", inp.exact_init));
    }
    for (s, st) in prog.steps.iter().enumerate() {
        out.push_str(&format!(
            "    let t{s} = {} {} {};\n",
            exact_ref(st.lhs),
            st.op.symbol(),
            exact_ref(st.rhs)
        ));
    }
    out
}

/// `let beta = ...;` accumulating the runtime filter factor
/// (Attene 2025 Appendix A): max of `|v|` over `Beta::Factor` inputs
/// (referencing their `i{k}` bindings — must come after
/// [`emit_f64_lets`]' input section) and each distinct `Beta::Cached`
/// expression.
pub fn emit_beta(prog: &Program) -> String {
    let mut out = String::from("    let mut beta = 0.0f64;\n");
    let mut cached_seen: Vec<&str> = Vec::new();
    for (i, inp) in prog.inputs.iter().enumerate() {
        match &inp.beta {
            Beta::Factor => out.push_str(&format!("    beta = beta.max(i{i}.abs());\n")),
            Beta::Cached(expr) => {
                if !cached_seen.contains(&expr.as_str()) {
                    cached_seen.push(expr);
                    out.push_str(&format!("    beta = beta.max({expr});\n"));
                }
            }
            Beta::Covered => {}
        }
    }
    out
}

/// Reference text for the program's output value.
pub fn f64_out(o: Operand) -> String {
    f64_ref(o)
}

pub fn exact_out(o: Operand) -> String {
    match o {
        Operand::Input(i) => format!("i{i}"),
        Operand::Step(s) => format!("t{s}"),
    }
}

/// Shortest-roundtrip scientific literal for an `f64` constant.
pub fn f64_literal(v: f64) -> String {
    format!("{v:e}")
}

// =====================================================================
// Generic predicate-instance emission (PR-CR-M7b)
// =====================================================================
//
// The orient3d generator predates this and keeps its own emission; the
// orient2d and pointCompare families share this scaffold. The structure
// is Attene §5's cascade: semi-statically filtered f64 (§5.1 + App. A)
// → intervals (§5.2) → exact rationals (§5.3), with the denominator
// sign-parity rule applied per instance (§5.1: "possible multiplicities
// must be considered" — `parity_slots` lists the slots whose `d` has
// ODD multiplicity in `D′`; even-multiplicity `d`s only feed the
// undefinedness checks).

/// One predicate instance ready for emission.
pub struct InstanceEmit {
    /// Generated function base name, e.g. `orient2d_xy_lle`.
    pub name: String,
    /// Constant suffix, e.g. `ORIENT2D_XY_LLE`.
    pub const_suffix: String,
    /// Number of generic-point arguments (2 or 3).
    pub arity: usize,
    /// Slots holding implicit points (lambda parameters `l{i}`).
    pub implicit_slots: Vec<usize>,
    /// Slots whose `d` sign flips the result (odd multiplicity in `D′`).
    pub parity_slots: Vec<usize>,
    /// The polynomial program for `Λ′`.
    pub prog: Program,
    /// The `Λ′` output operand.
    pub out: Operand,
    /// Doc line for the instance functions (paper citation).
    pub doc: String,
}

const ARG_NAMES: [&str; 4] = ["a", "b", "c", "d"];

/// Emit the 2 constants + 5 tier functions for one instance; returns
/// `(code, delta, degree)`.
#[allow(clippy::needless_range_loop)] // slot indices feed both names and arrays
pub fn emit_instance(e: &InstanceEmit) -> (String, f64, u32) {
    let (sfe, degree) = e.prog.analyze(e.out);
    let delta = sfe.delta(degree);
    let up = &e.const_suffix;
    let name = &e.name;
    let explicit_slots: Vec<usize> = (0..e.arity)
        .filter(|i| !e.implicit_slots.contains(i))
        .collect();

    let mut code = String::new();
    code.push_str(&format!(
        "/// `δ(1)` and degree `k` for `{name}`: Λ′ bound {:.3e},\n\
         /// propagated error {:.3e} (FPG analysis), degree {} in the β\n\
         /// factors. Runtime threshold: `ε = δ·β^k` (Attene App. A).\n\
         pub(super) const DELTA_{up}: f64 = {};\n\
         pub(super) const DEGREE_{up}: i32 = {};\n\n",
        sfe.bound,
        sfe.error,
        degree,
        f64_literal(delta),
        degree
    ));

    // Parameter lists per tier.
    let mut fparams: Vec<String> = Vec::new();
    let mut iparams: Vec<String> = Vec::new();
    let mut eparams: Vec<String> = Vec::new();
    for i in 0..e.arity {
        if e.implicit_slots.contains(&i) {
            fparams.push(format!("l{i}: &LambdaF64"));
            iparams.push(format!("li{i}: &LambdaIv"));
            eparams.push(format!("le{i}: &LambdaExact"));
        } else {
            fparams.push(format!("p{i}: &Point3"));
            iparams.push(format!("p{i}: &Point3"));
            eparams.push(format!("p{i}: &Point3"));
        }
    }

    // Filtered tier (Attene §5.1 + App. A).
    code.push_str(&e.doc);
    code.push_str(&format!(
        "pub(super) fn {name}_filtered({}) -> Option<Sign> {{\n",
        fparams.join(", ")
    ));
    for &i in &e.implicit_slots {
        code.push_str(&format!(
            "    if !l{i}.d_reliable {{\n        return None;\n    }}\n"
        ));
    }
    code.push_str(&emit_f64_lets(&e.prog));
    code.push_str(&emit_beta(&e.prog));
    code.push_str(&format!(
        "    let eps = DELTA_{up} * beta.powi(DEGREE_{up}) + support::SUBNORMAL_GUARD;\n\
         \x20   if !eps.is_finite() {{\n        return None;\n    }}\n\
         \x20   let lam = {};\n\
         \x20   let mut s = if lam > eps {{\n\
         \x20       Sign::Positive\n\
         \x20   }} else if lam < -eps {{\n\
         \x20       Sign::Negative\n\
         \x20   }} else {{\n\
         \x20       return None;\n\
         \x20   }};\n",
        f64_out(e.out)
    ));
    for &i in &e.parity_slots {
        code.push_str(&format!(
            "    if l{i}.d < 0.0 {{\n        s = s.flipped();\n    }}\n"
        ));
    }
    code.push_str("    Some(s)\n}\n\n");

    // Interval tier (Attene §5.2).
    code.push_str(&format!(
        "/// Interval (dynamic-filter) tier (Attene §5.2). `None` = sign\n\
         /// ambiguous, fall through to exact rationals.\n\
         pub(super) fn {name}_interval({}) -> Option<Sign> {{\n",
        iparams.join(", ")
    ));
    for &i in &e.implicit_slots {
        code.push_str(&format!(
            "    let d{i} = li{i}.d.sign()?;\n\
             \x20   if d{i} == Sign::Zero {{\n        return Some(Sign::Undefined);\n    }}\n"
        ));
    }
    code.push_str(&emit_iv_lets(&e.prog));
    code.push_str(&format!(
        "    let mut s = {}.sign()?;\n\
         \x20   if s == Sign::Zero {{\n        return Some(Sign::Zero);\n    }}\n",
        f64_out(e.out)
    ));
    for &i in &e.parity_slots {
        code.push_str(&format!(
            "    if d{i} == Sign::Negative {{\n        s = s.flipped();\n    }}\n"
        ));
    }
    code.push_str("    Some(s)\n}\n\n");

    // Exact tier (Attene §5.3).
    code.push_str(&format!(
        "/// Exact rational tier (Attene §5.3). Caller guarantees every\n\
         /// implicit `d != 0` (else the predicate is Undefined).\n\
         pub(super) fn {name}_exact({}) -> Sign {{\n",
        eparams.join(", ")
    ));
    code.push_str(&emit_exact_lets(&e.prog));
    code.push_str(&format!(
        "    let mut s = Sign::of_rbig(&{});\n\
         \x20   if s == Sign::Zero {{\n        return Sign::Zero;\n    }}\n",
        exact_out(e.out)
    ));
    for &i in &e.parity_slots {
        code.push_str(&format!(
            "    if Sign::of_rbig(&le{i}.d) == Sign::Negative {{\n        s = s.flipped();\n    }}\n"
        ));
    }
    code.push_str("    s\n}\n\n");

    // Per-instance inexact dispatcher.
    let gp_params: Vec<String> = (0..e.arity)
        .map(|i| format!("{}: &GenericPoint3D", ARG_NAMES[i]))
        .collect();
    let mut fargs: Vec<String> = Vec::new();
    let mut iargs: Vec<String> = Vec::new();
    let mut eargs: Vec<String> = Vec::new();
    for i in 0..e.arity {
        if e.implicit_slots.contains(&i) {
            fargs.push(format!("{}.lambda_f64()", ARG_NAMES[i]));
            iargs.push(format!("{}.lambda_iv()", ARG_NAMES[i]));
            eargs.push(format!("&le{i}"));
        } else {
            fargs.push(format!("&p{i}"));
            iargs.push(format!("&p{i}"));
            eargs.push(format!("&p{i}"));
        }
    }
    code.push_str(&format!(
        "/// Inexact tiers: semi-static filter (§5.1), then intervals (§5.2).\n\
         pub(super) fn {name}_inexact({}) -> Option<Sign> {{\n",
        gp_params.join(", ")
    ));
    for &i in &explicit_slots {
        code.push_str(&format!(
            "    let p{i} = {}.expect_explicit();\n",
            ARG_NAMES[i]
        ));
    }
    code.push_str(&format!(
        "    if let Some(s) = {name}_filtered({}) {{\n        return Some(s);\n    }}\n\
         \x20   {name}_interval({})\n}}\n\n",
        fargs.join(", "),
        iargs.join(", ")
    ));

    // Per-instance full dispatcher.
    code.push_str(&format!(
        "/// Full tier dispatcher: inexact tiers, then exact rationals.\n\
         pub(super) fn {name}({}) -> Sign {{\n\
         \x20   if let Some(s) = {name}_inexact({}) {{\n        return s;\n    }}\n",
        gp_params.join(", "),
        (0..e.arity)
            .map(|i| ARG_NAMES[i].to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    for i in 0..e.arity {
        if e.implicit_slots.contains(&i) {
            code.push_str(&format!(
                "    let le{i} = {}.lambda_exact();\n\
                 \x20   if le{i}.is_undefined() {{\n        return Sign::Undefined;\n    }}\n",
                ARG_NAMES[i]
            ));
        } else {
            code.push_str(&format!(
                "    let p{i} = {}.expect_explicit();\n",
                ARG_NAMES[i]
            ));
        }
    }
    code.push_str(&format!("    {name}_exact({})\n}}\n\n", eargs.join(", ")));

    (code, delta, degree)
}

/// Exact-tier-only body for the family dispatchers' match arms: bind
/// exact lambdas / explicit points for `pattern`, with undefined checks,
/// then call `{name}_exact`.
#[allow(clippy::needless_range_loop)] // slot indices feed both names and arrays
pub fn emit_exact_arm_body(name: &str, arity: usize, implicit_slots: &[usize]) -> String {
    let mut body = String::new();
    let mut eargs: Vec<String> = Vec::new();
    for i in 0..arity {
        if implicit_slots.contains(&i) {
            body.push_str(&format!(
                "            let le{i} = {}.lambda_exact();\n\
                 \x20           if le{i}.is_undefined() {{\n                return Sign::Undefined;\n            }}\n",
                ARG_NAMES[i]
            ));
            eargs.push(format!("&le{i}"));
        } else {
            body.push_str(&format!(
                "            let p{i} = {}.expect_explicit();\n",
                ARG_NAMES[i]
            ));
            eargs.push(format!("&p{i}"));
        }
    }
    body.push_str(&format!("            {name}_exact({})\n", eargs.join(", ")));
    body
}
