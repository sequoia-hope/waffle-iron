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
