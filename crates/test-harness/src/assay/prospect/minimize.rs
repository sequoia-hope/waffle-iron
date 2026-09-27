//! Recipe minimizer — the smallest recipe with the same failure signature.
//!
//! Spec: `specs/assay_prospector.md` §7. Every reduction is tried against a
//! caller-supplied judge (the prospector's verdict subprocess; a closure in
//! tests) and kept only if the signature is unchanged. Reductions, in
//! order: truncate from the end; drop single middle steps; un-snap;
//! simplify profiles; simplify operations; round coordinates. Bounded by a
//! verdict budget.

use super::gen3::{Op, Profile, Recipe, Step, StepGeom, Verb};

/// A judge maps a recipe to its finding signature (`None` = could not be
/// judged, e.g. the driver failed; treated as "signature changed").
pub type Judge<'a> = dyn FnMut(&Recipe) -> Option<String> + 'a;

/// What the minimizer did.
#[derive(Debug, Clone, Default)]
pub struct MinimizeReport {
    pub verdicts_used: usize,
    pub steps_before: usize,
    pub steps_after: usize,
    /// Reductions that were kept, in order.
    pub kept: Vec<String>,
    /// Stopped because the verdict budget ran out.
    pub budget_exhausted: bool,
}

struct Ctx<'a> {
    judge: &'a mut Judge<'a>,
    target: String,
    budget: usize,
    report: MinimizeReport,
}

impl Ctx<'_> {
    /// Try a candidate; `true` (and the caller keeps it) when the signature
    /// is preserved.
    fn holds(&mut self, candidate: &Recipe) -> bool {
        if self.report.verdicts_used >= self.budget {
            self.report.budget_exhausted = true;
            return false;
        }
        self.report.verdicts_used += 1;
        (self.judge)(candidate).as_deref() == Some(self.target.as_str())
    }

    fn exhausted(&self) -> bool {
        self.report.budget_exhausted
    }
}

fn simpler_profile(p: &Profile) -> Option<Profile> {
    Some(match p {
        Profile::Star { points, r_out, .. } => Profile::Convex {
            n: (*points).max(3),
            r: *r_out,
            rot: 0.0,
        },
        Profile::NonConvex { r, radii, .. } => Profile::Convex {
            n: (radii.len() as u32).max(3),
            r: *r,
            rot: 0.0,
        },
        Profile::Gear { teeth, module } => Profile::Circle {
            r: module * (*teeth as f64 / 2.0 + 1.0),
        },
        Profile::Circle { r } => Profile::Convex {
            n: 4,
            r: *r,
            rot: 0.0,
        },
        Profile::Convex { n, r, rot } if *n > 4 || *rot != 0.0 => Profile::Convex {
            n: 4,
            r: *r,
            rot: 0.0,
        },
        Profile::Convex { .. } => return None,
    })
}

fn simpler_op(op: &Op, scale: f64) -> Option<Op> {
    Some(match op {
        Op::Revolve { cut, .. } => Op::Extrude {
            depth: scale,
            cut: *cut,
        },
        Op::Boolean { depth, verb } => Op::Extrude {
            depth: *depth,
            cut: *verb == Verb::Subtract,
        },
        Op::Standalone { depth } => Op::Extrude {
            depth: *depth,
            cut: false,
        },
        Op::ExtrudeSymmetric { total_depth } => Op::Extrude {
            depth: *total_depth,
            cut: false,
        },
        Op::ThroughAllCut => Op::Extrude {
            depth: scale * 4.0,
            cut: true,
        },
        Op::Extrude { .. } => return None,
    })
}

/// Round to `digits` significant digits.
fn round_sig(x: f64, digits: i32) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let mag = x.abs().log10().floor() as i32;
    let f = 10f64.powi(digits - 1 - mag);
    (x * f).round() / f
}

fn round_geom(g: &StepGeom, digits: i32) -> StepGeom {
    let r3 = |v: [f64; 3]| {
        [
            round_sig(v[0], digits),
            round_sig(v[1], digits),
            round_sig(v[2], digits),
        ]
    };
    let mut out = g.clone();
    out.plane.origin = r3(g.plane.origin);
    out.plane.normal = r3(g.plane.normal);
    out.profile = match &g.profile {
        Profile::Convex { n, r, rot } => Profile::Convex {
            n: *n,
            r: round_sig(*r, digits),
            rot: round_sig(*rot, digits),
        },
        Profile::NonConvex { r, rot, radii } => Profile::NonConvex {
            r: round_sig(*r, digits),
            rot: round_sig(*rot, digits),
            radii: radii.iter().map(|x| round_sig(*x, digits)).collect(),
        },
        Profile::Star {
            points,
            r_in,
            r_out,
            rot,
        } => Profile::Star {
            points: *points,
            r_in: round_sig(*r_in, digits),
            r_out: round_sig(*r_out, digits),
            rot: round_sig(*rot, digits),
        },
        Profile::Circle { r } => Profile::Circle {
            r: round_sig(*r, digits),
        },
        Profile::Gear { teeth, module } => Profile::Gear {
            teeth: *teeth,
            module: round_sig(*module, digits),
        },
    };
    out.op = match &g.op {
        Op::Extrude { depth, cut } => Op::Extrude {
            depth: round_sig(*depth, digits),
            cut: *cut,
        },
        Op::ExtrudeSymmetric { total_depth } => Op::ExtrudeSymmetric {
            total_depth: round_sig(*total_depth, digits),
        },
        Op::ThroughAllCut => Op::ThroughAllCut,
        Op::Revolve {
            axis_origin,
            axis_dir,
            angle_deg,
            cut,
        } => Op::Revolve {
            axis_origin: r3(*axis_origin),
            axis_dir: r3(*axis_dir),
            angle_deg: round_sig(*angle_deg, digits),
            cut: *cut,
        },
        Op::Boolean { depth, verb } => Op::Boolean {
            depth: round_sig(*depth, digits),
            verb: *verb,
        },
        Op::Standalone { depth } => Op::Standalone {
            depth: round_sig(*depth, digits),
        },
    };
    out
}

/// Minimize `recipe` while `judge` keeps returning `target_signature`.
///
/// The returned recipe always has the target signature (it starts as the
/// input, which the caller has judged already) — a reduction whose judge
/// call fails or disagrees is discarded.
pub fn minimize(
    recipe: &Recipe,
    target_signature: &str,
    judge: &mut Judge<'_>,
    budget: usize,
) -> (Recipe, MinimizeReport) {
    let mut ctx = Ctx {
        judge,
        target: target_signature.to_string(),
        budget,
        report: MinimizeReport {
            steps_before: recipe.steps.len(),
            ..Default::default()
        },
    };
    let mut cur = recipe.clone();

    // 1. Truncate from the end (binary-ish: halve first, then one by one).
    let mut n = cur.steps.len();
    while n > 1 && !ctx.exhausted() {
        let try_n = if n > 3 { n / 2 } else { n - 1 };
        let mut c = cur.clone();
        c.steps.truncate(try_n.max(1));
        if ctx.holds(&c) {
            ctx.report
                .kept
                .push(format!("truncate to {}", c.steps.len()));
            cur = c;
            n = cur.steps.len();
        } else if try_n == n - 1 {
            break;
        } else {
            // Halving was too much; fall back to one-by-one from here.
            let mut c = cur.clone();
            c.steps.truncate(n - 1);
            if ctx.holds(&c) {
                ctx.report
                    .kept
                    .push(format!("truncate to {}", c.steps.len()));
                cur = c;
                n = cur.steps.len();
            } else {
                break;
            }
        }
    }

    // 2. Drop single middle steps (never step 0).
    let mut i = cur.steps.len().saturating_sub(1);
    while i >= 1 && !ctx.exhausted() {
        if cur.steps.len() > 1 {
            let mut c = cur.clone();
            c.steps.remove(i);
            if ctx.holds(&c) {
                ctx.report.kept.push(format!("drop step {i}"));
                cur = c;
            }
        }
        i -= 1;
    }

    // 3. Un-snap (a snapped step's generic geometry).
    for i in 0..cur.steps.len() {
        if ctx.exhausted() {
            break;
        }
        if cur.steps[i].snapped.is_some() {
            let mut c = cur.clone();
            c.steps[i].snapped = None;
            c.steps[i].snap_note = None;
            if ctx.holds(&c) {
                ctx.report.kept.push(format!("un-snap step {i}"));
                cur = c;
            }
        }
    }

    // 4. Simplify profiles, 5. simplify operations — per step, repeat while
    //    a simpler form exists.
    for i in 0..cur.steps.len() {
        loop {
            if ctx.exhausted() {
                break;
            }
            let g = cur.steps[i].geom().clone();
            let Some(p) = simpler_profile(&g.profile) else {
                break;
            };
            let mut c = cur.clone();
            let ng = StepGeom {
                profile: p.clone(),
                ..g
            };
            c.steps[i] = Step {
                generic: ng,
                snapped: None,
                snap_note: None,
            };
            if ctx.holds(&c) {
                ctx.report
                    .kept
                    .push(format!("step {i} profile → {}", p.label()));
                cur = c;
            } else {
                break;
            }
        }
        if ctx.exhausted() {
            break;
        }
        let g = cur.steps[i].geom().clone();
        if let Some(op) = simpler_op(&g.op, cur.scale) {
            let mut c = cur.clone();
            c.steps[i] = Step {
                generic: StepGeom { op, ..g },
                snapped: None,
                snap_note: None,
            };
            if ctx.holds(&c) {
                ctx.report.kept.push(format!("step {i} op simplified"));
                cur = c;
            }
        }
    }

    // 6. Round coordinates: 3, then 2, then 1 significant digits, per step.
    for digits in [3, 2, 1] {
        for i in 0..cur.steps.len() {
            if ctx.exhausted() {
                break;
            }
            let g = round_geom(cur.steps[i].geom(), digits);
            if &g == cur.steps[i].geom() {
                continue;
            }
            let mut c = cur.clone();
            c.steps[i] = Step {
                generic: g,
                snapped: None,
                snap_note: None,
            };
            if ctx.holds(&c) {
                ctx.report
                    .kept
                    .push(format!("step {i} rounded to {digits} sig. digits"));
                cur = c;
            }
        }
    }

    ctx.report.steps_after = cur.steps.len();
    let report = ctx.report;
    (cur, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assay::prospect::gen3::Plane;

    fn step(profile: Profile, op: Op) -> Step {
        Step {
            generic: StepGeom {
                plane: Plane {
                    origin: [0.123456, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
                profile,
                op,
            },
            snapped: None,
            snap_note: None,
        }
    }

    /// A fake defect: the signature holds iff some step still has a Star
    /// profile AND a cut follows it.
    fn fake_judge(r: &Recipe) -> Option<String> {
        let star_at = r
            .steps
            .iter()
            .position(|s| matches!(s.geom().profile, Profile::Star { .. }));
        let cut_after = star_at.map(|i| {
            r.steps[i..].iter().any(|s| {
                matches!(
                    s.geom().op,
                    Op::Extrude { cut: true, .. }
                        | Op::Boolean {
                            verb: Verb::Subtract,
                            ..
                        }
                )
            })
        });
        Some(if cut_after == Some(true) {
            "error[star-cut]".into()
        } else {
            "correct".into()
        })
    }

    #[test]
    fn minimizer_keeps_only_what_the_signature_needs() {
        let recipe = Recipe {
            seed: 9,
            index: 9,
            scale: 1.0,
            steps: vec![
                step(
                    Profile::Gear {
                        teeth: 12,
                        module: 0.1,
                    },
                    Op::Extrude {
                        depth: 1.234567,
                        cut: false,
                    },
                ),
                step(
                    Profile::Circle { r: 0.5 },
                    Op::Extrude {
                        depth: 0.3,
                        cut: false,
                    },
                ),
                step(
                    Profile::Star {
                        points: 5,
                        r_in: 0.1,
                        r_out: 0.9,
                        rot: 0.7,
                    },
                    Op::Boolean {
                        depth: 0.4,
                        verb: Verb::Subtract,
                    },
                ),
                step(
                    Profile::Convex {
                        n: 7,
                        r: 0.2,
                        rot: 0.1,
                    },
                    Op::Revolve {
                        axis_origin: [0.0; 3],
                        axis_dir: [0.0, 1.0, 0.0],
                        angle_deg: 90.0,
                        cut: false,
                    },
                ),
            ],
        };
        let mut judge = fake_judge;
        let (min, rep) = minimize(&recipe, "error[star-cut]", &mut judge, 200);
        assert_eq!(fake_judge(&min).as_deref(), Some("error[star-cut]"));
        assert!(!rep.budget_exhausted, "{rep:?}");
        // The revolve step was dropped; the star stays a star (simplifying
        // it to a convex breaks the signature); the boolean became a plain
        // cut; the gear base was simplified to a circle then a 4-gon.
        assert_eq!(min.steps.len(), 2, "{rep:?}");
        assert!(matches!(min.steps[1].geom().profile, Profile::Star { .. }));
        assert!(matches!(
            min.steps[1].geom().op,
            Op::Extrude { cut: true, .. }
        ));
        assert!(matches!(
            min.steps[0].geom().profile,
            Profile::Convex { n: 4, .. }
        ));
        // Rounded coordinates.
        assert_eq!(min.steps[0].geom().plane.origin[0], 0.1);
        assert!(rep.verdicts_used < 40, "{rep:?}");
    }

    #[test]
    fn budget_stops_the_search_and_returns_a_valid_recipe() {
        let recipe = Recipe {
            seed: 1,
            index: 1,
            scale: 1.0,
            steps: vec![
                step(
                    Profile::Circle { r: 1.0 },
                    Op::Extrude {
                        depth: 1.0,
                        cut: false,
                    },
                ),
                step(
                    Profile::Star {
                        points: 3,
                        r_in: 0.2,
                        r_out: 0.8,
                        rot: 0.0,
                    },
                    Op::Extrude {
                        depth: 0.5,
                        cut: true,
                    },
                ),
            ],
        };
        let mut judge = fake_judge;
        let (min, rep) = minimize(&recipe, "error[star-cut]", &mut judge, 1);
        assert!(rep.budget_exhausted);
        assert_eq!(fake_judge(&min).as_deref(), Some("error[star-cut]"));
    }

    #[test]
    fn round_sig_rounds_significant_digits() {
        assert_eq!(round_sig(0.123456, 3), 0.123);
        assert_eq!(round_sig(1234.5, 2), 1200.0);
        assert_eq!(round_sig(-0.00098765, 1), -0.001);
        assert_eq!(round_sig(0.0, 3), 0.0);
    }
}
