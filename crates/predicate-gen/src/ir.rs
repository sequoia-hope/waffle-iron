//! Expression IR for polynomial predicates.
//!
//! Variables, `+`, `−`, `×` — no division: indirect predicates are
//! polynomial *fractions* whose denominators are handled by the
//! sign-parity rule, never evaluated (Attene 2025 §4 + §5.1,
//! `refs/text/attene-predicates.txt:149-166, 281-286`).
//!
//! A [`Program`] is straight-line SSA: a list of typed inputs followed
//! by binary steps in topological order. The same program is consumed
//! three ways:
//!
//! 1. **analysis** — FPG forward error propagation ([`crate::fpg`])
//!    plus homogeneous-degree bookkeeping, producing `δ(1)` and `k`;
//! 2. **f64 codegen** — straight-line `let` bindings over `f64`;
//! 3. **exact codegen** — the same bindings over `dashu::rational::RBig`.

use crate::fpg::Sfe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
}

impl Op {
    pub fn symbol(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    Input(usize),
    Step(usize),
}

/// How an input contributes to the runtime filter factor `β`
/// (Attene 2025 Appendix A: `β = max{b_1, ..., b_k}` over the factors
/// deriving from explicit variables and the cached per-implicit-point
/// maxima).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Beta {
    /// The input IS a factor: its `|value|` enters the runtime max.
    Factor,
    /// The β contribution is a cached per-point value (e.g. `l0.beta`),
    /// emitted once per distinct expression string.
    Cached(String),
    /// No own contribution (already covered by a `Cached` entry —
    /// lambda values are polynomials over their point's cached factors).
    Covered,
}

#[derive(Clone, Debug)]
pub struct Input {
    /// Runtime f64 initializer (e.g. `p2.x() - p3.x()` or `l0.l[0]`).
    pub f64_init: String,
    /// Runtime interval initializer (e.g. `Iv::point(p2.x())` or
    /// `li0.l[0]`) for the dynamic-filter tier (Attene §5.2).
    pub iv_init: String,
    /// Runtime exact initializer (e.g. `support::rb(p2.x())`).
    pub exact_init: String,
    /// Starting `(bound, error)` under the `|factors| ≤ 1` scaling.
    /// For lambda-value inputs this is the analyzed `Sfe` of the lambda
    /// polynomial itself — Sfe propagation is compositional, so seeding
    /// with the sub-program's result is equivalent to full inlining.
    pub sfe: Sfe,
    /// Homogeneous degree in the β factors (1 for a factor itself,
    /// the lambda's degree for lambda values).
    pub degree: u32,
    pub beta: Beta,
}

#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub op: Op,
    pub lhs: Operand,
    pub rhs: Operand,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub inputs: Vec<Input>,
    pub steps: Vec<Step>,
}

impl Program {
    pub fn input(
        &mut self,
        f64_init: impl Into<String>,
        iv_init: impl Into<String>,
        exact_init: impl Into<String>,
        sfe: Sfe,
        degree: u32,
        beta: Beta,
    ) -> Operand {
        self.inputs.push(Input {
            f64_init: f64_init.into(),
            iv_init: iv_init.into(),
            exact_init: exact_init.into(),
            sfe,
            degree,
            beta,
        });
        Operand::Input(self.inputs.len() - 1)
    }

    /// An explicit input coordinate (`|v| ≤ 1` exact, β factor).
    pub fn raw_factor(
        &mut self,
        f64_init: impl Into<String>,
        iv_init: impl Into<String>,
        exact_init: impl Into<String>,
    ) -> Operand {
        self.input(
            f64_init,
            iv_init,
            exact_init,
            Sfe::EXACT_INPUT,
            1,
            Beta::Factor,
        )
    }

    /// A translation difference of two fresh inputs (FPG translation
    /// filter; β factor).
    pub fn diff_factor(
        &mut self,
        f64_init: impl Into<String>,
        iv_init: impl Into<String>,
        exact_init: impl Into<String>,
    ) -> Operand {
        self.input(
            f64_init,
            iv_init,
            exact_init,
            Sfe::translation_input(),
            1,
            Beta::Factor,
        )
    }

    fn push(&mut self, op: Op, lhs: Operand, rhs: Operand) -> Operand {
        self.steps.push(Step { op, lhs, rhs });
        Operand::Step(self.steps.len() - 1)
    }

    pub fn add(&mut self, a: Operand, b: Operand) -> Operand {
        self.push(Op::Add, a, b)
    }

    pub fn sub(&mut self, a: Operand, b: Operand) -> Operand {
        self.push(Op::Sub, a, b)
    }

    pub fn mul(&mut self, a: Operand, b: Operand) -> Operand {
        self.push(Op::Mul, a, b)
    }

    /// 3×3 determinant by cofactor expansion along the first row.
    pub fn det3(&mut self, m: [[Operand; 3]; 3]) -> Operand {
        let c0 = self.minor(m, 1, 2);
        let c1 = self.minor(m, 0, 2);
        let c2 = self.minor(m, 0, 1);
        let p0 = self.mul(m[0][0], c0);
        let p1 = self.mul(m[0][1], c1);
        let p2 = self.mul(m[0][2], c2);
        let d = self.sub(p0, p1);
        self.add(d, p2)
    }

    /// 2×2 minor of rows 1..2, columns `ca` < `cb`.
    fn minor(&mut self, m: [[Operand; 3]; 3], ca: usize, cb: usize) -> Operand {
        let p0 = self.mul(m[1][ca], m[2][cb]);
        let p1 = self.mul(m[1][cb], m[2][ca]);
        self.sub(p0, p1)
    }

    /// Cross product of two 3-vectors of operands.
    pub fn cross(&mut self, a: [Operand; 3], b: [Operand; 3]) -> [Operand; 3] {
        let x0 = self.mul(a[1], b[2]);
        let x1 = self.mul(a[2], b[1]);
        let x = self.sub(x0, x1);
        let y0 = self.mul(a[2], b[0]);
        let y1 = self.mul(a[0], b[2]);
        let y = self.sub(y0, y1);
        let z0 = self.mul(a[0], b[1]);
        let z1 = self.mul(a[1], b[0]);
        let z = self.sub(z0, z1);
        [x, y, z]
    }

    /// Dot product of two 3-vectors of operands.
    pub fn dot(&mut self, a: [Operand; 3], b: [Operand; 3]) -> Operand {
        let p0 = self.mul(a[0], b[0]);
        let p1 = self.mul(a[1], b[1]);
        let p2 = self.mul(a[2], b[2]);
        let s = self.add(p0, p1);
        self.add(s, p2)
    }

    /// Forward error analysis of `out`: the `(Sfe, degree)` of the
    /// expression under the `|factors| ≤ 1` scaling.
    ///
    /// Panics if an `Add`/`Sub` combines operands of different degree —
    /// the predicate polynomials are homogeneous (FPG §2, lines
    /// 132-135), and an inhomogeneous sum means the program is wrong.
    pub fn analyze(&self, out: Operand) -> (Sfe, u32) {
        let mut acc: Vec<(Sfe, u32)> = Vec::with_capacity(self.steps.len());
        for (i, s) in self.steps.iter().enumerate() {
            let (la, ld) = self.resolve(&acc, s.lhs);
            let (ra, rd) = self.resolve(&acc, s.rhs);
            let v = match s.op {
                Op::Add | Op::Sub => {
                    assert_eq!(
                        ld, rd,
                        "inhomogeneous {:?} at step {i}: degree {ld} vs {rd}",
                        s.op
                    );
                    (la.add(ra), ld)
                }
                Op::Mul => (la.mul(ra), ld + rd),
            };
            acc.push(v);
        }
        self.resolve(&acc, out)
    }

    fn resolve(&self, acc: &[(Sfe, u32)], o: Operand) -> (Sfe, u32) {
        match o {
            Operand::Input(i) => (self.inputs[i].sfe, self.inputs[i].degree),
            Operand::Step(s) => acc[s],
        }
    }
}
