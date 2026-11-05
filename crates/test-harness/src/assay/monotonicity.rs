//! Volume monotonicity per operation (remediation plan O5, review
//! 2026-10-28 §2.1; kernel 1.0 gate 1.2): after every solid-bearing
//! operation the model's total live volume must move the way the DOCUMENT
//! says it moves — down, not at all, or up. A cut that removes nothing (a
//! silent no-op) or a union that loses material is exactly what it states.
//!
//! **Where the expected direction comes from.** Not from the meta's authored
//! `volume_monotonicity`: the generator writes "decrease" per cut and
//! "increase" per boss without knowing reach, and the 2026-11-03
//! adjudication (`docs/kernel_1_0.md`, Step 0, "O5 adjudicated") found all
//! 41 corpus violations of that list to be authored-expectation defects —
//! 39 no-change steps (a tool that misses the body returns the operand) and
//! 2 Intersect-combine bosses that legitimately shrink (C0074, C0081). The
//! expectation is instead the EXACT chain's ([`super::exact_membership`])
//! per-step volume change, read for every prefix the chain covers.
//!
//! **The exact side.** For step `k` the document truncated after that
//! feature is parsed as its own [`ExactChain`] (so a later uncovered feature
//! does not take the covered prefix down with it, and an indeterminate
//! decision is charged to the prefix it occurs in), and the change between
//! the prefixes before and after the step is CERTIFIED, not sampled: an
//! adaptive octree over both prefixes' boxes, in the first sketch's frame,
//! asks the chain's own sound ball predicates
//! ([`ExactChain::expr_may_contain_ball`] / `expr_must_contain_ball` — the
//! certificate `certified_empty_at` already relies on) whether each live
//! body of each prefix provably contains or provably misses the node's
//! circumscribed ball. A node decided for every body contributes its exact
//! body-count change times its volume (the kernel sums volume per body, so
//! overlapping bodies count twice on both sides); a node still undecided at
//! the exact-volume verdict's cell size (`EXACT_VOLUME_CELLS` along the
//! longest axis) contributes the whole range its counts allow. The result
//! is an interval `I = [lo, hi]` that holds the true change.
//!
//! Where bodies are undecided, the node does not give up on them: the
//! bodies' undecided OPERANDS are enumerated jointly (an operand both
//! prefixes share is one variable), and when every assignment yields the
//! same count change the node is exact. That is what lets an untouched
//! body's boundary cancel against itself — a cut whose tool misses the body
//! reads `[0, 0]`, not the body's whole boundary layer — and confines the
//! octree's refinement to where the step's tool actually meets material.
//!
//! **Why not the exact-volume verdict's band.** The first cut of this oracle
//! reused that band (`surface_cubes · h³`, applied to the changed region of
//! two sampled lattices, with one cube when nothing changed) and the
//! canonical corpus answered with three false WRONGs (C0032, C0039: 1e-4 m
//! and 2e-5 m rib bosses; C0033: a 1e-4 m cut slit — kernel changes of
//! 4e-7 to 1.6e-5 m³ that no lattice sample saw). A band measured on resolved features
//! cannot bound a feature thinner than the cell; the certificate can — a
//! sub-cell feature leaves its cells undecided and widens `I` by exactly
//! their volume. It has no tolerance of its own: its only constants are the
//! verdict's cell count and a few-ulp pad on the ball radius.
//!
//! The exact chain's direction is *decrease* when `I < 0`, *increase* when
//! `I > 0`, *none* when `0 ∈ I`. A step is a VIOLATION when the kernel's
//! exact change disagrees: any kernel change outside `I` (on a decrease or
//! increase step `I` is only as tight as the first level with a sign), and
//! on a decrease or increase step a kernel change of the
//! wrong sign or none (a dropped cut). The kernel's own "no change" is
//! [`MONOTONIC_REL_EPS`] of its volume.
//!
//! **Not covered** (no verdict, never a pass): a step whose prefix (or the
//! prefix before it) the exact chain does not cover or leaves indeterminate,
//! and a step whose live solids the kernel cannot integrate.
//!
//! **Kernel side.** One replay, no prefix re-runs: feature-engine keeps every
//! feature's result and who consumed whom (`EngineState::consumed_by`), so
//! the live set after step k is "outputs of features ≤ k not consumed by a
//! feature ≤ k". Volumes are the kernel's EXACT B-Rep volumes.

use super::exact_membership::{Basis, Body, ExactChain, Expr, EXACT_VOLUME_CELLS};
use crate::workflow::ModelBuilder;
use feature_engine::types::Operation;
use serde_json::Value;
use std::collections::HashSet;

/// Relative change below which the KERNEL's step counts as "no change" — far
/// above the exact integrator's rounding (bit-level on re-integration of the
/// same B-Rep), far below any authored feature.
pub const MONOTONIC_REL_EPS: f64 = 1e-9;

/// The certificate's WORK bound per document, in ball-predicate body tests
/// summed over every step: one exact-volume reading's worth — the flat
/// lattice `exact_volume_verdict` already samples per case is
/// `(EXACT_VOLUME_CELLS + 2)³` point tests. An oracle must never move a
/// case's category through its own cost (a CORRECT case turned TIMEOUT
/// under the per-case CPU budget), so a document whose certificate would
/// cost more leaves its remaining steps NOT COVERED, reported as such. A
/// work count, not a clock: the verdict is deterministic.
pub const O5_WORK_BUDGET: u64 =
    ((EXACT_VOLUME_CELLS + 2) * (EXACT_VOLUME_CELLS + 2) * (EXACT_VOLUME_CELLS + 2)) as u64;

/// Direction of a volume change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Decrease,
    None,
    Increase,
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Direction::Decrease => "decrease",
            Direction::None => "none",
            Direction::Increase => "increase",
        })
    }
}

/// The exact chain's reading of one step: a CERTIFIED interval holding the
/// true volume change (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExactStep {
    /// Lower bound of the exact change.
    pub lo: f64,
    /// Upper bound of the exact change.
    pub hi: f64,
    /// Finest cell edge length of the certificate.
    pub h: f64,
    /// Finest cells the certificate could not decide (their volume bounds
    /// the interval's width).
    pub undecided: usize,
}

impl ExactStep {
    /// `I = [lo, hi]`, the interval holding the exact change.
    pub fn interval(&self) -> (f64, f64) {
        (self.lo, self.hi)
    }

    pub fn direction(&self) -> Direction {
        if self.hi < 0.0 {
            Direction::Decrease
        } else if self.lo > 0.0 {
            Direction::Increase
        } else {
            Direction::None
        }
    }
}

/// One step's violation, with everything needed to read it.
#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    pub step: usize,
    pub expected: Direction,
    /// The exact interval `I`.
    pub exact_lo: f64,
    pub exact_hi: f64,
    pub kernel_before: f64,
    pub kernel_after: f64,
}

/// Judge one step: `Some` when the kernel's change disagrees with the exact
/// chain's direction (see the module docs).
pub fn judge_step(
    step: usize,
    exact: &ExactStep,
    kernel_before: f64,
    kernel_after: f64,
) -> Option<Violation> {
    let d = kernel_after - kernel_before;
    let eps = MONOTONIC_REL_EPS * kernel_before.abs().max(kernel_after.abs());
    let (lo, hi) = exact.interval();
    let expected = exact.direction();
    // `I` holds the true change whatever the direction, so the kernel's change
    // must lie in it on EVERY step. On a decrease / increase step `I` is the
    // first octree level with a sign (`run` stops there), so this catches a
    // change beyond that coarse bound, not every partly dropped cut; the
    // sign test catches the dropped one (`I` may straddle the kernel's eps).
    let outside = d < lo - eps || d > hi + eps;
    let bad = outside
        || match expected {
            Direction::Decrease => d >= -eps,
            Direction::Increase => d <= eps,
            Direction::None => false,
        };
    bad.then_some(Violation {
        step,
        expected,
        exact_lo: lo,
        exact_hi: hi,
        kernel_before,
        kernel_after,
    })
}

/// The outcome over a whole document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MonotonicityReading {
    /// Steps judged against the exact chain's direction.
    pub checked: usize,
    /// Steps with no verdict (exact prefix uncovered / indeterminate, or a
    /// solid the kernel cannot integrate).
    pub not_covered: usize,
    /// Of `not_covered`, the steps the certificate's [`O5_WORK_BUDGET`]
    /// left unread (the exact chain covered them).
    pub work_bound: usize,
    pub violations: Vec<Violation>,
    /// Wall time of the prefix parses, under `ASSAY_O5_TIME_PROBE=1` only.
    pub probe_parse_secs: Option<f64>,
}

impl MonotonicityReading {
    /// Stable summary: `monotonicity: <checked> checked, <n> not covered,
    /// <n> violation(s)`, then one bracket per violation.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "monotonicity: {} checked, {} not covered, {} violation(s)",
            self.checked,
            self.not_covered,
            self.violations.len()
        );
        if self.work_bound > 0 {
            s.push_str(&format!(" ({} over the work bound)", self.work_bound));
        }
        for v in &self.violations {
            s.push_str(&format!(
                " [op {}: exact {} in [{:.6e}, {:.6e}], kernel {:.6e} -> {:.6e}]",
                v.step, v.expected, v.exact_lo, v.exact_hi, v.kernel_before, v.kernel_after
            ));
        }
        s
    }

    pub fn is_defect(&self) -> bool {
        !self.violations.is_empty()
    }
}

/// The document's feature list (single Part tab), or `None`.
fn doc_features(doc: &Value) -> Option<&Vec<Value>> {
    doc.get("tabs")?
        .as_array()?
        .first()?
        .pointer("/kind/features/features")?
        .as_array()
}

/// The exact chain of the document truncated after feature `upto` (doc
/// index, inclusive); `None` when it is not covered or indeterminate.
fn prefix_chain(doc: &Value, upto: usize) -> Option<ExactChain> {
    let mut d = doc.clone();
    let feats = d
        .get_mut("tabs")?
        .as_array_mut()?
        .first_mut()?
        .pointer_mut("/kind/features/features")?
        .as_array_mut()?;
    feats.truncate(upto + 1);
    let chain = ExactChain::from_waffle(&d).ok()?;
    chain.indeterminate.is_empty().then_some(chain)
}

/// One prefix's live bodies, for the certificate.
struct Prefix<'a> {
    chain: &'a ExactChain,
    bodies: Vec<Body>,
}

/// The exact chain's reading of each step. `step_doc_index[k]` is the doc
/// feature index of step `k`; the state before step 0 is the empty model.
/// Entry `k` is `None` when step `k` is not covered.
pub fn exact_steps(doc: &Value, step_doc_index: &[usize]) -> Vec<Option<ExactStep>> {
    let chains: Vec<Option<ExactChain>> = step_doc_index
        .iter()
        .map(|&i| prefix_chain(doc, i))
        .collect();
    exact_steps_with_budget(&chains, O5_WORK_BUDGET).0
}

/// [`exact_steps`] over parsed prefix chains under a work budget; also
/// returns how many steps the budget (not coverage) left unread.
fn exact_steps_with_budget(
    chains: &[Option<ExactChain>],
    budget: u64,
) -> (Vec<Option<ExactStep>>, usize) {
    let mut work_left = budget;
    let mut over = 0usize;
    let steps = (0..chains.len())
        .map(|k| {
            let cur = chains[k].as_ref()?;
            let prev = if k == 0 {
                None
            } else {
                Some(chains[k - 1].as_ref()?)
            };
            let step = certify_step(prev, cur, &mut work_left);
            if step.is_none() {
                over += 1;
            }
            step
        })
        .collect();
    (steps, over)
}

/// The certified change between two prefixes (`prev = None`: the empty
/// model). An adaptive octree over the union of both prefixes' boxes, laid
/// in the first sketch's frame, refined down to the exact-volume verdict's
/// cell (`EXACT_VOLUME_CELLS` along the longest axis): a node whose
/// circumscribed ball every body of both prefixes provably contains or
/// provably misses contributes its exact count change times its volume; a
/// finest cell still undecided contributes the whole range its counts allow.
fn certify_step(
    prev: Option<&ExactChain>,
    cur: &ExactChain,
    work_left: &mut u64,
) -> Option<ExactStep> {
    let frame = cur.frame;
    let prefixes: Vec<Prefix> = prev
        .into_iter()
        .chain(std::iter::once(cur))
        .map(|c| Prefix {
            chain: c,
            bodies: c.bodies_after(c.ops.len()),
        })
        .collect();
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &prefixes {
        let Some((wlo, whi)) = p.chain.bbox_prefix(p.chain.ops.len()) else {
            continue;
        };
        for corner in 0..8 {
            let w = [
                if corner & 1 == 0 { wlo[0] } else { whi[0] },
                if corner & 2 == 0 { wlo[1] } else { whi[1] },
                if corner & 4 == 0 { wlo[2] } else { whi[2] },
            ];
            let r = [
                w[0] - frame.origin[0],
                w[1] - frame.origin[1],
                w[2] - frame.origin[2],
            ];
            let q = [
                r[0] * frame.x[0] + r[1] * frame.x[1] + r[2] * frame.x[2],
                r[0] * frame.y[0] + r[1] * frame.y[1] + r[2] * frame.y[2],
                r[0] * frame.n[0] + r[1] * frame.n[1] + r[2] * frame.n[2],
            ];
            for k in 0..3 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
    }
    let longest = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max);
    if !longest.is_finite() || longest <= 0.0 {
        // No live body in either prefix (or a zero-volume box): no change.
        return Some(ExactStep {
            lo: 0.0,
            hi: 0.0,
            h: 0.0,
            undecided: 0,
        });
    }
    let h = longest / EXACT_VOLUME_CELLS as f64;
    // Root cube: 2^levels finest cells per side, one cell of padding below.
    let levels = ((EXACT_VOLUME_CELLS + 2) as f64).log2().ceil() as u32;
    let root = h * (1u64 << levels) as f64;
    let corner = [lo[0] - h, lo[1] - h, lo[2] - h];
    let all = |pi: usize| -> Vec<(usize, usize)> {
        (0..prefixes[pi].bodies.len()).map(|b| (pi, b)).collect()
    };
    let (prev_bodies, cur_bodies) = if prefixes.len() == 2 {
        (all(0), all(1))
    } else {
        (Vec::new(), all(0))
    };
    let mut acc = Acc {
        work_left: *work_left,
        ..Acc::default()
    };
    // Leaves the two prefixes share: the same op, parsed from the same
    // document prefix (an op's solid depends only on the features before it).
    let shared = match prev {
        Some(p) => p
            .ops
            .iter()
            .zip(&cur.ops)
            .take_while(|(a, b)| a.feature_id == b.feature_id)
            .count(),
        None => 0,
    };
    let ctx = Ctx {
        prefixes: &prefixes,
        frame,
        h,
        shared,
    };
    ctx.run(corner, root, prev_bodies, cur_bodies, &mut acc);
    *work_left = acc.work_left;
    (!acc.exhausted).then_some(ExactStep {
        lo: acc.lo,
        hi: acc.hi,
        h,
        undecided: acc.undecided,
    })
}

#[derive(Default)]
struct Acc {
    lo: f64,
    hi: f64,
    undecided: usize,
    /// Predicate tests left in the document's [`O5_WORK_BUDGET`].
    work_left: u64,
    exhausted: bool,
}

impl Acc {
    /// Charge `cost` predicate tests; `false` (and exhausted) when the
    /// budget cannot pay.
    fn charge(&mut self, cost: u64) -> bool {
        if self.exhausted || self.work_left < cost {
            self.exhausted = true;
            return false;
        }
        self.work_left -= cost;
        true
    }
}

/// At most this many undecided operands are enumerated jointly at one node
/// (`2^n` assignments). A COST cap, not a tolerance: past it the node falls
/// back to the sound per-body count range, which is wider, never wrong.
const MAX_ENUMERATED_OPERANDS: usize = 10;

/// An operand as a boolean variable of the step's two prefixes. A leaf the
/// two prefixes share (same index, same feature) is ONE variable — that is
/// what lets an unchanged body's boundary cancel against itself — and any
/// other leaf or lump is private to its prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Var {
    Leaf {
        prefix: usize,
        i: usize,
        alt: bool,
    },
    /// A lump node (by address), private to its prefix.
    Lump {
        prefix: usize,
        addr: usize,
    },
}

struct Ctx<'a> {
    prefixes: &'a [Prefix<'a>],
    frame: Basis,
    h: f64,
    /// Ops whose leaves both prefixes share (`prefix` 0 in [`Var::Leaf`]).
    shared: usize,
}

/// A node's ball: centre (world) and radius.
#[derive(Clone, Copy)]
struct Ball {
    p: [f64; 3],
    rad: f64,
}

impl Ctx<'_> {
    /// Split `bodies` (still undecided at the parent) at the node's ball:
    /// `(count provably containing it, still undecided)`.
    fn split(&self, b: Ball, bodies: &[(usize, usize)]) -> (i64, Vec<(usize, usize)>) {
        let mut must = 0i64;
        let mut open = Vec::new();
        for &(pi, bi) in bodies {
            let pre = &self.prefixes[pi];
            let e = &pre.bodies[bi].expr;
            if !pre.chain.expr_may_contain_ball(e, b.p, b.rad) {
                continue;
            }
            if pre.chain.expr_must_contain_ball(e, b.p, b.rad) {
                must += 1;
            } else {
                open.push((pi, bi));
            }
        }
        (must, open)
    }

    fn var_of_leaf(&self, pi: usize, i: usize, alt: bool) -> Var {
        let prefix = if i < self.shared { 0 } else { pi + 1 };
        Var::Leaf { prefix, i, alt }
    }

    /// Collect the variables of `e` (body of prefix `pi`), deduplicated.
    fn collect(&self, pi: usize, e: &Expr, out: &mut Vec<Var>) {
        let v = match e {
            Expr::Leaf(i) => self.var_of_leaf(pi, *i, false),
            Expr::AltLeaf(i) => self.var_of_leaf(pi, *i, true),
            Expr::Lump { .. } => Var::Lump {
                prefix: pi,
                addr: e as *const Expr as usize,
            },
            Expr::Union(es) => {
                for x in es {
                    self.collect(pi, x, out);
                }
                return;
            }
            Expr::Sub(a, b) | Expr::Inter(a, b) => {
                self.collect(pi, a, out);
                self.collect(pi, b, out);
                return;
            }
        };
        if !out.contains(&v) {
            out.push(v);
        }
    }

    /// The variable's state on the ball: `Some(true)` provably containing,
    /// `Some(false)` provably missing, `None` undecided. `rep` is an
    /// expression of prefix `pi` that the variable stands for.
    fn state(&self, pi: usize, rep: &Expr, b: Ball) -> Option<bool> {
        let chain = self.prefixes[pi].chain;
        if !chain.expr_may_contain_ball(rep, b.p, b.rad) {
            Some(false)
        } else if chain.expr_must_contain_ball(rep, b.p, b.rad) {
            Some(true)
        } else {
            None
        }
    }

    /// Membership of `e` under an assignment of every variable.
    fn eval(&self, pi: usize, e: &Expr, vars: &[Var], vals: &[bool]) -> bool {
        let look = |v: Var| vals[vars.iter().position(|w| *w == v).expect("collected")];
        match e {
            Expr::Leaf(i) => look(self.var_of_leaf(pi, *i, false)),
            Expr::AltLeaf(i) => look(self.var_of_leaf(pi, *i, true)),
            Expr::Lump { .. } => look(Var::Lump {
                prefix: pi,
                addr: e as *const Expr as usize,
            }),
            Expr::Union(es) => es.iter().any(|x| self.eval(pi, x, vars, vals)),
            Expr::Sub(a, b) => self.eval(pi, a, vars, vals) && !self.eval(pi, b, vars, vals),
            Expr::Inter(a, b) => self.eval(pi, a, vars, vals) && self.eval(pi, b, vars, vals),
        }
    }

    /// The range of the count change `Σ cur − Σ prev` over the node, from
    /// the open bodies' operands (`None`: too many operands to enumerate,
    /// or the budget ran out).
    fn enumerate(
        &self,
        b: Ball,
        prev: &[(usize, usize)],
        cur: &[(usize, usize)],
        acc: &mut Acc,
    ) -> Option<(i64, i64)> {
        let mut vars = Vec::new();
        // One representative expression per variable, to test its state.
        let mut reps: Vec<(usize, Expr)> = Vec::new();
        for &(pi, bi) in prev.iter().chain(cur) {
            let e = &self.prefixes[pi].bodies[bi].expr;
            let before = vars.len();
            self.collect(pi, e, &mut vars);
            for v in &vars[before..] {
                reps.push(match *v {
                    Var::Leaf { i, alt: false, .. } => (pi, Expr::Leaf(i)),
                    Var::Leaf { i, alt: true, .. } => (pi, Expr::AltLeaf(i)),
                    Var::Lump { addr, .. } => (pi, find_lump(e, addr).clone()),
                });
            }
        }
        if !acc.charge(vars.len() as u64) {
            return None;
        }
        let states: Vec<Option<bool>> = reps.iter().map(|(pi, e)| self.state(*pi, e, b)).collect();
        let unknown: Vec<usize> = (0..vars.len()).filter(|&k| states[k].is_none()).collect();
        if unknown.len() > MAX_ENUMERATED_OPERANDS {
            return None;
        }
        let assignments = 1u64 << unknown.len();
        if !acc.charge(assignments * (prev.len() + cur.len()) as u64) {
            return None;
        }
        let mut vals: Vec<bool> = states.iter().map(|s| s.unwrap_or(false)).collect();
        let (mut dmin, mut dmax) = (i64::MAX, i64::MIN);
        for a in 0..assignments {
            for (bit, &k) in unknown.iter().enumerate() {
                vals[k] = (a >> bit) & 1 == 1;
            }
            let count = |set: &[(usize, usize)]| {
                set.iter()
                    .filter(|&&(pi, bi)| {
                        self.eval(pi, &self.prefixes[pi].bodies[bi].expr, &vars, &vals)
                    })
                    .count() as i64
            };
            let d = count(cur) - count(prev);
            dmin = dmin.min(d);
            dmax = dmax.max(d);
        }
        Some((dmin, dmax))
    }

    /// Refine level by level from the root node. Each level's interval is
    /// the decided contributions plus, for every node subdivided at that
    /// level, its own enumerated range — and the levels' intervals are
    /// NESTED (a child's operand states only get more decided than its
    /// parent's), so the run stops at the first level whose interval has a
    /// sign: the direction is then certified and a finer level cannot
    /// change it. A step whose change straddles 0 refines to the finest
    /// cell. Returns the final `(lo, hi)` in `acc`.
    fn run(
        &self,
        corner: [f64; 3],
        root: f64,
        prev: Vec<(usize, usize)>,
        cur: Vec<(usize, usize)>,
        acc: &mut Acc,
    ) {
        struct Node {
            c: [f64; 3],
            s: f64,
            base: (i64, i64),
            prev: Vec<(usize, usize)>,
            cur: Vec<(usize, usize)>,
        }
        let mut frontier = vec![Node {
            c: corner,
            s: root,
            base: (0, 0),
            prev,
            cur,
        }];
        // Contributions no finer level can change.
        let (mut fixed_lo, mut fixed_hi) = (0.0f64, 0.0f64);
        while !frontier.is_empty() {
            let (mut pend_lo, mut pend_hi) = (0.0f64, 0.0f64);
            let mut next = Vec::new();
            for n in frontier {
                if !acc.charge((n.prev.len() + n.cur.len()) as u64) {
                    return;
                }
                let f = &self.frame;
                let (u, v, w) = (n.c[0] + 0.5 * n.s, n.c[1] + 0.5 * n.s, n.c[2] + 0.5 * n.s);
                let p = [
                    f.origin[0] + u * f.x[0] + v * f.y[0] + w * f.n[0],
                    f.origin[1] + u * f.x[1] + v * f.y[1] + w * f.n[1],
                    f.origin[2] + u * f.x[2] + v * f.y[2] + w * f.n[2],
                ];
                // The cube's circumscribed ball; the relative pad covers the
                // rounding of the corner distance (a few ulps), nothing more.
                let rad = 0.5 * n.s * 3.0f64.sqrt() * (1.0 + 16.0 * f64::EPSILON);
                let ball = Ball { p, rad };
                let (mp, op) = self.split(ball, &n.prev);
                let (mc, oc) = self.split(ball, &n.cur);
                let (mp, mc) = (n.base.0 + mp, n.base.1 + mc);
                let vol = n.s * n.s * n.s;
                if op.is_empty() && oc.is_empty() {
                    let d = (mc - mp) as f64 * vol;
                    fixed_lo += d;
                    fixed_hi += d;
                    continue;
                }
                // The open bodies' count change over the node, enumerated
                // over their undecided operands. Every point of the node
                // assigns each operand a definite value consistent with its
                // state, so the true change at every point is one of the
                // enumerated values.
                let range = self.enumerate(ball, &op, &oc, acc);
                if acc.exhausted {
                    return;
                }
                let (dmin, dmax) = range.unwrap_or((-(op.len() as i64), oc.len() as i64));
                let (lo, hi) = ((mc - mp + dmin) as f64 * vol, (mc - mp + dmax) as f64 * vol);
                // Exact, or the finest level (`s` halves exactly from
                // `root = h · 2^levels`): final.
                if dmin == dmax || n.s <= self.h * 1.5 {
                    fixed_lo += lo;
                    fixed_hi += hi;
                    if dmin != dmax {
                        acc.undecided += 1;
                    }
                    continue;
                }
                pend_lo += lo;
                pend_hi += hi;
                let half = 0.5 * n.s;
                for k in 0..8 {
                    next.push(Node {
                        c: [
                            n.c[0] + if k & 1 == 0 { 0.0 } else { half },
                            n.c[1] + if k & 2 == 0 { 0.0 } else { half },
                            n.c[2] + if k & 4 == 0 { 0.0 } else { half },
                        ],
                        s: half,
                        base: (mp, mc),
                        prev: op.clone(),
                        cur: oc.clone(),
                    });
                }
            }
            acc.lo = fixed_lo + pend_lo;
            acc.hi = fixed_hi + pend_hi;
            if acc.lo > 0.0 || acc.hi < 0.0 {
                return;
            }
            frontier = next;
        }
        acc.lo = fixed_lo;
        acc.hi = fixed_hi;
    }
}

/// The lump node of `e` at address `addr`.
fn find_lump(e: &Expr, addr: usize) -> &Expr {
    fn go(e: &Expr, addr: usize) -> Option<&Expr> {
        if e as *const Expr as usize == addr {
            return Some(e);
        }
        match e {
            Expr::Union(es) => es.iter().find_map(|x| go(x, addr)),
            Expr::Sub(a, b) | Expr::Inter(a, b) => go(a, addr).or_else(|| go(b, addr)),
            _ => None,
        }
    }
    go(e, addr).expect("collected from this expression")
}

/// Read every solid-bearing operation of the loaded document: the kernel's
/// exact live volume after each, judged against the exact chain of `doc`
/// (`None` — the exact oracle is off or the JSON did not parse — leaves
/// every step not covered).
pub fn measure(builder: &ModelBuilder, doc: Option<&Value>) -> MonotonicityReading {
    let engine = &builder.state.engine;
    let tree = &engine.tree;
    let limit = tree.active_index.unwrap_or(tree.features.len());
    let ops: Vec<_> = tree.features[..limit]
        .iter()
        .filter(|f| {
            !f.suppressed
                && !matches!(
                    f.operation,
                    Operation::Sketch { .. } | Operation::DatumPlane { .. }
                )
        })
        .collect();
    let mut r = MonotonicityReading::default();
    if ops.is_empty() {
        return r;
    }
    // Exact side: each step's doc feature index, matched by feature id.
    let exact: Vec<Option<ExactStep>> = doc
        .and_then(|doc| {
            let feats = doc_features(doc)?;
            let idx: Option<Vec<usize>> = ops
                .iter()
                .map(|f| {
                    let id = f.id.to_string();
                    feats
                        .iter()
                        .position(|g| g.get("id").and_then(Value::as_str) == Some(id.as_str()))
                })
                .collect();
            // `ASSAY_O5_TIME_PROBE=1`: time the prefix parses (dev probe).
            let probe = std::env::var("ASSAY_O5_TIME_PROBE").is_ok_and(|v| v == "1");
            let t0 = probe.then(std::time::Instant::now);
            let chains: Vec<Option<ExactChain>> =
                idx?.iter().map(|&i| prefix_chain(doc, i)).collect();
            r.probe_parse_secs = t0.map(|t| t.elapsed().as_secs_f64());
            let (steps, over) = exact_steps_with_budget(&chains, O5_WORK_BUDGET);
            r.work_bound = over;
            Some(steps)
        })
        .unwrap_or_else(|| vec![None; ops.len()]);

    let kernel = builder.kernel_ref().as_introspect();
    let mut before = 0.0f64;
    let mut before_known = true;
    for (k, _) in ops.iter().enumerate() {
        // Live after step k: outputs of ops[..=k] not consumed by ops[..=k].
        let upto: HashSet<_> = ops[..=k].iter().map(|g| g.id).collect();
        let consumed: HashSet<_> = engine
            .consumed_by
            .iter()
            .filter(|(consumer, _)| upto.contains(consumer))
            .flat_map(|(_, v)| v.iter().copied())
            .collect();
        let mut after = 0.0f64;
        let mut known = true;
        for g in &ops[..=k] {
            if consumed.contains(&g.id) {
                continue;
            }
            if let Some(res) = engine.get_result(g.id) {
                for (_, body) in &res.outputs {
                    match kernel.solid_volume(&body.handle) {
                        Ok(v) => after += v,
                        Err(_) => known = false,
                    }
                }
            }
        }
        match (&exact[k], known && before_known) {
            (Some(step), true) => {
                r.checked += 1;
                if let Some(v) = judge_step(k, step, before, after) {
                    r.violations.push(v);
                }
            }
            _ => r.not_covered += 1,
        }
        before = after;
        before_known = known;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn square_sketch(id: &str, origin: [f64; 3], cu: f64, cv: f64, hu: f64, hv: f64) -> Value {
        let corners = [
            (cu - hu, cv - hv),
            (cu + hu, cv - hv),
            (cu + hu, cv + hv),
            (cu - hu, cv + hv),
        ];
        let entities: Vec<Value> = corners
            .iter()
            .enumerate()
            .map(|(k, (x, y))| json!({"type": "Point", "id": k + 1, "x": x, "y": y}))
            .collect();
        json!({
            "id": id, "name": format!("Sketch {id}"), "suppressed": false,
            "operation": {"type": "Sketch", "sketch": {
                "id": id, "plane_origin": origin, "plane_normal": [0.0, 0.0, 1.0],
                "entities": entities,
                "solved_profiles": [{"entity_ids": [1, 2, 3, 4], "is_outer": true, "vertex_ids": [1, 2, 3, 4]}]
            }}
        })
    }

    fn extrude(id: &str, sketch: &str, depth: f64, cut: bool, extra: Value) -> Value {
        let mut params = json!({
            "sketch_id": sketch, "profile_index": 0, "depth": depth, "direction": [0.0, 0.0, 1.0],
            "symmetric": false, "cut": cut, "merge": true,
            "depth_mode": {"type": "Blind"}, "second_direction": null
        });
        if let Some(obj) = extra.as_object() {
            for (k, v) in obj {
                params[k] = v.clone();
            }
        }
        json!({
            "id": id, "name": format!("Extrude {id}"), "suppressed": false,
            "operation": {"type": "Extrude", "params": params}
        })
    }

    fn target(feature_id: &str) -> Value {
        json!({
            "kind": {"type": "Solid"},
            "anchor": {"type": "FeatureOutput", "feature_id": feature_id, "output_key": {"type": "Main"}},
            "selector": {"type": "Role", "role": {"type": "EndCapPositive"}, "index": 0},
            "policy": {"type": "BestEffort"}
        })
    }

    fn doc(features: Vec<Value>) -> Value {
        json!({"tabs": [{"kind": {"type": "Part", "features": {"features": features}}}]})
    }

    /// A unit cube (s1/e1) and a second extrude e2 on sketch s2.
    fn cube_then(s2: Value, e2: Value) -> Value {
        doc(vec![
            square_sketch("s1", [0.0; 3], 0.0, 0.0, 0.5, 0.5),
            extrude("e1", "s1", 1.0, false, json!({})),
            s2,
            e2,
        ])
    }

    /// A 0.5 × 0.5 cut through the cube removes 0.25: the exact chain reads a
    /// decrease, and a kernel that left the volume at 1 (a dropped cut) is a
    /// violation; a kernel that removed 0.25 is not.
    #[test]
    fn a_dropped_cut_is_flagged() {
        let d = cube_then(
            square_sketch("s2", [0.0, 0.0, -0.5], 0.0, 0.0, 0.25, 0.25),
            extrude("e2", "s2", 2.0, true, json!({})),
        );
        let steps = exact_steps(&d, &[1, 3]);
        let s0 = steps[0].expect("first step covered");
        assert_eq!(s0.direction(), Direction::Increase, "{s0:?}");
        assert!(judge_step(0, &s0, 0.0, 1.0).is_none());
        let s1 = steps[1].expect("cut step covered");
        assert_eq!(s1.direction(), Direction::Decrease, "{s1:?}");
        let v = judge_step(1, &s1, 1.0, 1.0).expect("dropped cut flagged");
        assert_eq!(v.expected, Direction::Decrease);
        // A cut that ADDED material is also flagged.
        assert!(judge_step(1, &s1, 1.0, 1.1).is_some());
        assert!(judge_step(1, &s1, 1.0, 0.75).is_none());
        // A decrease larger than `I` allows (the cut took more than its tool
        // can reach) is flagged too.
        let v = judge_step(1, &s1, 1.0, 1.0 + s1.lo - 0.01).expect("over-removal flagged");
        assert_eq!(v.expected, Direction::Decrease);
    }

    /// A cut whose tool misses the body (a free-space cut — the adjudicated
    /// no-change class) reads none: the kernel's unchanged volume is not a
    /// violation, a kernel that removed material anyway is.
    #[test]
    fn a_legit_no_change_step_is_not_flagged() {
        let d = cube_then(
            square_sketch("s2", [5.0, 5.0, 0.0], 0.0, 0.0, 0.25, 0.25),
            extrude("e2", "s2", 1.0, true, json!({})),
        );
        let steps = exact_steps(&d, &[1, 3]);
        let s1 = steps[1].expect("free-space cut covered");
        assert_eq!(s1.interval(), (0.0, 0.0), "{s1:?}");
        assert_eq!(s1.direction(), Direction::None);
        assert!(judge_step(1, &s1, 1.0, 1.0).is_none());
        assert!(judge_step(1, &s1, 1.0, 1.0 + 1e-12).is_none());
        assert!(
            judge_step(1, &s1, 1.0, 0.75).is_some(),
            "a kernel change the exact chain does not make is flagged"
        );
    }

    /// C0081's shape: an `Intersect`-combine BOSS keeps only the overlap
    /// (0.336 of a unit cube) — a legitimate shrink the authored "increase"
    /// called a violation. The exact chain reads a decrease.
    #[test]
    fn an_intersect_combine_shrink_is_not_flagged() {
        let d = cube_then(
            square_sketch("s2", [0.3, 0.2, 0.4], 0.0, 0.0, 0.5, 0.5),
            extrude(
                "e2",
                "s2",
                1.0,
                false,
                json!({"combine": {"type": "Intersect"}, "targets": [target("e1")]}),
            ),
        );
        let steps = exact_steps(&d, &[1, 3]);
        let s1 = steps[1].expect("intersect step covered");
        assert_eq!(s1.direction(), Direction::Decrease, "{s1:?}");
        assert!(judge_step(1, &s1, 1.0, 0.336).is_none());
        assert!(judge_step(1, &s1, 1.0, 1.0).is_some());
    }

    /// C0032's shape: a 1e-4 m needle rib boss on a slab — thinner than the
    /// cell, so no lattice sample sees it. The certificate leaves the rib's
    /// cells undecided: `I` straddles 0 and holds the rib's volume, so the
    /// kernel adding it is not a violation; a kernel change far outside `I`
    /// still is.
    #[test]
    fn a_sub_cell_rib_is_bounded_not_flagged() {
        let d = doc(vec![
            square_sketch("s1", [0.0; 3], 0.0, 0.0, 0.5, 0.5),
            extrude("e1", "s1", 0.2, false, json!({})),
            square_sketch("s2", [0.0, 0.0, 0.2], 0.0, 0.0, 0.5, 0.5e-4),
            extrude("e2", "s2", 0.3, false, json!({})),
        ]);
        let steps = exact_steps(&d, &[1, 3]);
        let s1 = steps[1].expect("rib step covered");
        let rib = 1.0 * 1e-4 * 0.3;
        let (lo, hi) = s1.interval();
        assert!(lo <= rib && rib <= hi, "{s1:?}");
        assert!(s1.undecided > 0, "{s1:?}");
        assert!(judge_step(1, &s1, 0.2, 0.2 + rib).is_none());
        assert!(judge_step(1, &s1, 0.2, 0.1).is_some());
        // The slab itself is a resolved increase.
        let s0 = steps[0].expect("slab step covered");
        assert_eq!(s0.direction(), Direction::Increase, "{s0:?}");
        let (lo0, hi0) = s0.interval();
        assert!(lo0 <= 0.2 && 0.2 <= hi0, "{s0:?}");
    }

    /// The work bound: a document whose certificate would cost more than
    /// the budget leaves the remaining steps NOT COVERED (counted), never a
    /// verdict and never an unbounded oracle cost.
    #[test]
    fn the_work_bound_leaves_steps_not_covered() {
        let d = cube_then(
            square_sketch("s2", [0.0, 0.0, -0.5], 0.0, 0.0, 0.25, 0.25),
            extrude("e2", "s2", 2.0, true, json!({})),
        );
        let chains: Vec<Option<ExactChain>> = [1, 3].iter().map(|&i| prefix_chain(&d, i)).collect();
        let (steps, over) = exact_steps_with_budget(&chains, 0);
        assert_eq!((steps[0], steps[1], over), (None, None, 2));
        let (steps, over) = exact_steps_with_budget(&chains, O5_WORK_BUDGET);
        assert!(steps.iter().all(Option::is_some) && over == 0);
        let r = MonotonicityReading {
            checked: 0,
            not_covered: 2,
            work_bound: 2,
            ..Default::default()
        };
        assert_eq!(
            r.summary(),
            "monotonicity: 0 checked, 2 not covered, 0 violation(s) (2 over the work bound)"
        );
    }

    /// A feature the exact chain has no closed form for (a region extrude)
    /// leaves its step — and every later one — not covered, while the
    /// covered prefix before it still reads.
    #[test]
    fn an_uncovered_prefix_is_not_covered() {
        let d = doc(vec![
            square_sketch("s1", [0.0; 3], 0.0, 0.0, 0.5, 0.5),
            extrude("e1", "s1", 1.0, false, json!({})),
            square_sketch("s2", [0.0, 0.0, 1.0], 0.0, 0.0, 0.25, 0.25),
            extrude("e2", "s2", 1.0, false, json!({"regions": [0]})),
            extrude("e3", "s1", 0.5, true, json!({})),
        ]);
        let steps = exact_steps(&d, &[1, 3, 4]);
        assert!(steps[0].is_some(), "covered prefix reads");
        assert!(steps[1].is_none(), "region extrude is not covered");
        assert!(steps[2].is_none(), "nor is anything after it");
        let r = MonotonicityReading {
            checked: 1,
            not_covered: 2,
            ..Default::default()
        };
        assert_eq!(
            r.summary(),
            "monotonicity: 1 checked, 2 not covered, 0 violation(s)"
        );
    }
}
