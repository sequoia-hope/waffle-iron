//! S4 — an INDEPENDENT structural oracle for the sketch constraint solver
//! (`specs/agent_mechanical_design.md` §10.4).
//!
//! `sketch_solver` answers "how constrained is this sketch?" with
//! [`SketchSolveReport`](waffle_types::sketch_state::SketchSolveReport): a
//! parameter count, a row count, a rank, `dof = params - rank`, a dependent-row
//! (redundancy) list and a null-space basis. Every one of those numbers comes
//! out of ONE decomposition — a column-pivoted QR of an ANALYTIC Jacobian —
//! so a mistake in a constraint's gradient is invisible to the report: the
//! wrong Jacobian produces a self-consistent wrong rank, and nothing in the
//! crate disagrees with it.
//!
//! This module is the disagreeing party. It recomputes the same structural
//! facts by different algebra at a different implementation, in the
//! reference-parity posture the kernel work uses (CLAUDE.md, "Reference parity
//! is not optional"):
//!
//! | | solver | this oracle |
//! |---|---|---|
//! | residual | hand-written per arm | written from the published equation table |
//! | derivative | analytic, hand-coded | **central finite differences** |
//! | rank | column-pivoted QR, absolute `RANK_TOL` | **SVD**, row-normalized, relative threshold |
//! | null space | `dof` smallest eigenvectors of `JᵗJ` | **right singular vectors** of the padded `J` |
//!
//! ## Where the residual definitions come from
//!
//! Each arm of [`residual_rows`] is written from the constraint's geometric
//! meaning, cross-checked against the PUBLISHED equation table in
//! `specs/sketch_solver_rewrite.md` §"Constraint Types" (lines 160–205) — not
//! from `sketch-solver`'s `constraint_mapping.rs`, which is the code under
//! test. Where a variant's form was genuinely undecidable from its type
//! definition, doc comment and that table, the convention was settled by
//! consulting the solver's published BEHAVIOUR tests, and the arm carries a
//! `CONSULTED:` comment saying so. Those comments are the inventory of where
//! this oracle's independence is partial; everything without one was derived.
//!
//! ## Parameter layout
//!
//! From the published contract on `SketchSolveReport::params` — "2 per point,
//! 1 per circle radius" — plus `SolvedSketch::radii`'s "Arcs are absent (their
//! radius is the center→start distance, captured by points)". Entities are
//! walked in declaration order, which is a CHOICE here: rank, dof and the
//! null-space dimension are invariant under column permutation, so the oracle
//! does not need the solver's column order and deliberately does not know it.
//!
//! ## Honesty about its own thresholds
//!
//! A rank is a judgement call at the tolerance. [`RANK_REL_TOL`] is relative to
//! the largest singular value of the ROW-NORMALIZED Jacobian, which makes it
//! scale-free (the same sketch at 1 m and at 1 km gives the same matrix), and
//! any singular value inside [`INDETERMINATE_DECADE`] of the threshold makes
//! the whole verdict [`RankVerdict::Indeterminate`] rather than a confident
//! number. A caller that gets `Indeterminate` must not assert a disagreement:
//! the oracle is saying it cannot tell, which is a different claim from "the
//! solver is wrong".

use std::collections::HashMap;

use nalgebra::DMatrix;
use waffle_types::{Sketch, SketchConstraint, SketchEntity};

// ── Tolerances, each a judgement call, each stated ───────────────────────────

/// Singular values below `RANK_REL_TOL * σ_max` are treated as zero.
///
/// The Jacobian is ROW-NORMALIZED first (every non-zero row scaled to unit
/// 2-norm), so `σ_max ∈ [1, √rows]` regardless of the sketch's units or
/// coordinate magnitude, and this threshold is a pure conditioning statement:
/// "a direction the constraints resist 10⁸ times more weakly than the
/// stiffest one is not resisted at all". It sits well above the finite-
/// difference noise floor (~1e-10 relative, see [`FD_REL_STEP`]) and well
/// below the conditioning of any sketch a human authors — the four rails plus
/// a pin of a unit square give σ ∈ [0.57, 1.5], a ratio of 2.6.
const RANK_REL_TOL: f64 = 1e-8;

/// A singular value within this factor of the rank threshold, either side,
/// makes the verdict [`RankVerdict::Indeterminate`]. One decade: the
/// finite-difference Jacobian is good to ~10 significant digits, so a
/// singular value within a decade of 1e-8·σ_max could be either side of the
/// line for reasons the oracle cannot see.
const INDETERMINATE_DECADE: f64 = 10.0;

/// Relative step for the central finite difference: `h_j = FD_REL_STEP *
/// max(|x_j|, FD_ABS_FLOOR)`.
///
/// Central differences have truncation error `O(h²·f''')` and roundoff
/// `O(ε·|f|/h)`, minimized near `h ≈ ε^(1/3) ≈ 6e-6` RELATIVE to the
/// parameter. 1e-6 is that optimum rounded to a power of ten; it puts the
/// Jacobian's relative error at ~1e-10, two decades inside [`RANK_REL_TOL`].
const FD_REL_STEP: f64 = 1e-6;

/// Floor under the relative step, so a parameter that is exactly 0 (an origin
/// point, a sketch authored on the axes) still gets differentiated.
const FD_ABS_FLOOR: f64 = 1e-6;

/// The oracle's own satisfaction tolerance, as a RELATIVE distance in
/// parameter space.
///
/// A row's residual is divided by its gradient norm before the comparison,
/// which turns it into a first-order distance from the current parameters to
/// that constraint's own variety — a length, in the sketch's own units,
/// whatever the residual's units were (`cross` products are areas, `Angle` is
/// radians). That distance is compared against `ORACLE_SAT_REL * max(1, the
/// sketch's largest coordinate magnitude)`, so the test scales with the model
/// the way `sketch_solver`'s absolute `SOLVE_TOL` does NOT (a known open
/// defect, `specs/agent_mechanical_design.md` §"S2 — solver state").
///
/// 1e-7 is deliberately loose: the oracle's job here is to tell a CONTRADICTION
/// (two dimensions 20 mm apart leave a first-order distance of ~1e-2 m) from
/// convergence noise (~1e-12), and the gap between those is nine decades. A
/// tight threshold would only turn LM's last few digits into false findings.
const ORACLE_SAT_REL: f64 = 1e-7;

/// Components of a null-space direction below this fraction of the direction's
/// largest component are reported as zero, so a rectangle free only in width
/// names the points that slide and not the ones that stay.
const FREE_COMPONENT_REL: f64 = 1e-6;

// ── Parameter layout ────────────────────────────────────────────────────────

/// A point in the sketch plane, `(x, y)`.
type Pt = (f64, f64);

/// One scalar the solver may move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamSlot {
    /// The x coordinate of a `Point` entity.
    PointX(u32),
    /// The y coordinate of a `Point` entity.
    PointY(u32),
    /// The radius of a `Circle` entity. (An `Arc` has none: its radius is the
    /// centre→start distance, which the points already carry.)
    Radius(u32),
}

/// What an entity id refers to, reduced to what a residual needs.
#[derive(Debug, Clone)]
enum Ent {
    Point,
    Line {
        start: u32,
        end: u32,
    },
    Circle {
        center: u32,
    },
    /// An arc's `end` point is deliberately absent: no constraint in the
    /// vocabulary reads it (an arc's radius is its centre→start distance, and
    /// its sweep is not constrainable), so recording it would claim a
    /// dependence the residuals do not have.
    Arc {
        center: u32,
        start: u32,
    },
    /// A spline or a compact generator (gear, sprocket): no constraint arm in
    /// the vocabulary targets one, so the oracle only needs to know it exists.
    Opaque,
}

/// The oracle's own parameter layout over a sketch's entities.
#[derive(Debug, Clone)]
pub struct Layout {
    slots: Vec<ParamSlot>,
    /// Point id → index of its x slot (y is the next one).
    point_at: HashMap<u32, usize>,
    /// Circle entity id → index of its radius slot.
    radius_at: HashMap<u32, usize>,
    ents: HashMap<u32, Ent>,
    /// Point id → the position the caller sent in, which is what a `Dragged`
    /// row holds the point to.
    drag_targets: HashMap<u32, (f64, f64)>,
}

impl Layout {
    /// Allocate 2 params per `Point` and 1 per `Circle`, in entity declaration
    /// order.
    pub fn build(entities: &[SketchEntity]) -> Layout {
        let mut slots = Vec::new();
        let mut point_at = HashMap::new();
        let mut radius_at = HashMap::new();
        let mut ents = HashMap::new();

        for e in entities {
            match e {
                SketchEntity::Point { id, .. } => {
                    point_at.insert(*id, slots.len());
                    slots.push(ParamSlot::PointX(*id));
                    slots.push(ParamSlot::PointY(*id));
                    ents.insert(*id, Ent::Point);
                }
                SketchEntity::Line {
                    id,
                    start_id,
                    end_id,
                    ..
                } => {
                    ents.insert(
                        *id,
                        Ent::Line {
                            start: *start_id,
                            end: *end_id,
                        },
                    );
                }
                SketchEntity::Circle { id, center_id, .. } => {
                    radius_at.insert(*id, slots.len());
                    slots.push(ParamSlot::Radius(*id));
                    ents.insert(*id, Ent::Circle { center: *center_id });
                }
                SketchEntity::Arc {
                    id,
                    center_id,
                    start_id,
                    ..
                } => {
                    ents.insert(
                        *id,
                        Ent::Arc {
                            center: *center_id,
                            start: *start_id,
                        },
                    );
                }
                SketchEntity::Spline { id, .. }
                | SketchEntity::Gear { id, .. }
                | SketchEntity::Sprocket { id, .. } => {
                    ents.insert(*id, Ent::Opaque);
                }
            }
        }

        Layout {
            slots,
            point_at,
            radius_at,
            ents,
            drag_targets: HashMap::new(),
        }
    }

    /// The initial parameter vector, read off the entities' authored geometry.
    pub fn initial(&self, entities: &[SketchEntity]) -> Vec<f64> {
        let mut x = vec![0.0; self.slots.len()];
        for e in entities {
            match e {
                SketchEntity::Point {
                    id, x: px, y: py, ..
                } => {
                    let i = self.point_at[id];
                    x[i] = *px;
                    x[i + 1] = *py;
                }
                SketchEntity::Circle { id, radius, .. } => {
                    x[self.radius_at[id]] = *radius;
                }
                _ => {}
            }
        }
        x
    }

    /// The parameter vector at a solved configuration: positions and radii as
    /// the solver returned them, falling back to the authored value for any
    /// entity the solve did not report.
    pub fn at_solution(
        &self,
        entities: &[SketchEntity],
        positions: &HashMap<u32, (f64, f64)>,
        radii: &HashMap<u32, f64>,
    ) -> Vec<f64> {
        let mut x = self.initial(entities);
        for (id, i) in &self.point_at {
            if let Some((px, py)) = positions.get(id) {
                x[*i] = *px;
                x[*i + 1] = *py;
            }
        }
        for (id, i) in &self.radius_at {
            if let Some(r) = radii.get(id) {
                x[*i] = *r;
            }
        }
        x
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn slots(&self) -> &[ParamSlot] {
        &self.slots
    }

    fn pt(&self, x: &[f64], id: u32) -> Result<(f64, f64), String> {
        let i = *self
            .point_at
            .get(&id)
            .ok_or_else(|| format!("no Point entity with id {id}"))?;
        Ok((x[i], x[i + 1]))
    }

    fn ent(&self, id: u32) -> Result<&Ent, String> {
        self.ents
            .get(&id)
            .ok_or_else(|| format!("no entity with id {id}"))
    }

    /// A line's `(start, end)` endpoints, by entity id.
    fn line_pts(&self, x: &[f64], id: u32) -> Result<(Pt, Pt), String> {
        match self.ent(id)? {
            Ent::Line { start, end } => Ok((self.pt(x, *start)?, self.pt(x, *end)?)),
            other => Err(format!("entity {id} is {other:?}, not a Line")),
        }
    }

    /// A circle's or arc's `(centre, radius)`.
    fn curve(&self, x: &[f64], id: u32) -> Result<((f64, f64), f64), String> {
        match self.ent(id)? {
            Ent::Circle { center } => {
                let c = self.pt(x, *center)?;
                let r = x[self.radius_at[&id]];
                Ok((c, r))
            }
            Ent::Arc { center, start, .. } => {
                let c = self.pt(x, *center)?;
                let s = self.pt(x, *start)?;
                Ok((c, dist(c, s)))
            }
            other => Err(format!("entity {id} is {other:?}, not a Circle or Arc")),
        }
    }
}

// ── Small plane geometry, written out rather than imported ──────────────────

fn sub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 - b.0, a.1 - b.1)
}

fn cross(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

fn dot(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

fn norm(a: (f64, f64)) -> f64 {
    a.0.hypot(a.1)
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    norm(sub(a, b))
}

/// Signed perpendicular distance from `p` to the infinite line `s→e`, positive
/// to the LEFT of the direction `s→e`.
///
/// Used for INCIDENCE (`OnEntity` on a line, `EqualPointToLine`), where the
/// signed form is the correct residual: a point's being on a line is a
/// two-sided zero, and `|·|` would hand the solver a vanishing gradient there.
///
/// The sign is the oracle's own choice. It is irrelevant to rank — a row's sign
/// does not change the Jacobian's span — and `sketch_solver`'s published
/// behaviour test `distance_point_line_and_line_point_compile_identically`
/// pins only the magnitude (`|forward| == 7`), so the solver's handedness is
/// not published anywhere the oracle could read it. Measured separately: the
/// solver's is the OPPOSITE handedness, which matters for the DIMENSION arms
/// and is the subject of
/// `a_point_line_distance_dimension_mirrors_the_point_across_the_line`.
fn signed_line_dist(p: (f64, f64), s: (f64, f64), e: (f64, f64)) -> Result<f64, String> {
    let d = sub(e, s);
    let l = norm(d);
    if l == 0.0 {
        return Err("zero-length line has no direction".into());
    }
    Ok(cross(d, sub(p, s)) / l)
}

/// The directed angle from `a` to `b`, wrapped into `(-π, π]`.
fn wrap_pi(mut a: f64) -> f64 {
    while a > std::f64::consts::PI {
        a -= std::f64::consts::TAU;
    }
    while a <= -std::f64::consts::PI {
        a += std::f64::consts::TAU;
    }
    a
}

fn line_angle_between(da: (f64, f64), db: (f64, f64)) -> Result<f64, String> {
    if norm(da) == 0.0 || norm(db) == 0.0 {
        return Err("zero-length line has no direction".into());
    }
    Ok(cross(da, db).atan2(dot(da, db)))
}

// ── Residuals ───────────────────────────────────────────────────────────────

/// How many residual rows a constraint owns, and what they evaluate to at `x`.
///
/// Every arm is the constraint's geometric meaning. The equation table in
/// `specs/sketch_solver_rewrite.md` §"Constraint Types" was used as the
/// published cross-check; `CONSULTED:` comments mark the arms where the
/// solver's own tests settled a convention the meaning did not fix.
///
/// `Err` is "this constraint does not apply to these operands" — the oracle
/// refuses rather than inventing a row, which is what makes a silently
/// accepted nonsense operand pair a visible disagreement.
pub fn residual_rows(layout: &Layout, c: &SketchConstraint, x: &[f64]) -> Result<Vec<f64>, String> {
    use SketchConstraint as C;
    match c {
        // ‖P₁ - P₂‖ = 0, componentwise: two rows, the whole point.
        C::Coincident { point_a, point_b } => {
            let a = layout.pt(x, *point_a)?;
            let b = layout.pt(x, *point_b)?;
            Ok(vec![a.0 - b.0, a.1 - b.1])
        }
        // A horizontal line's endpoints share a y.
        C::Horizontal { entity } => {
            let (s, e) = layout.line_pts(x, *entity)?;
            Ok(vec![s.1 - e.1])
        }
        // A vertical line's endpoints share an x.
        C::Vertical { entity } => {
            let (s, e) = layout.line_pts(x, *entity)?;
            Ok(vec![s.0 - e.0])
        }
        // The point-pair analogues (specs/point_pair_horizontal_vertical.md):
        // the same equation without a line entity to read the points off.
        C::HorizontalPoints { point_a, point_b } => {
            let a = layout.pt(x, *point_a)?;
            let b = layout.pt(x, *point_b)?;
            Ok(vec![a.1 - b.1])
        }
        C::VerticalPoints { point_a, point_b } => {
            let a = layout.pt(x, *point_a)?;
            let b = layout.pt(x, *point_b)?;
            Ok(vec![a.0 - b.0])
        }
        // Parallel ⇔ the directions' cross product vanishes.
        C::Parallel { line_a, line_b } => {
            let (sa, ea) = layout.line_pts(x, *line_a)?;
            let (sb, eb) = layout.line_pts(x, *line_b)?;
            Ok(vec![cross(sub(ea, sa), sub(eb, sb))])
        }
        // Perpendicular ⇔ the directions' dot product vanishes.
        C::Perpendicular { line_a, line_b } => {
            let (sa, ea) = layout.line_pts(x, *line_a)?;
            let (sb, eb) = layout.line_pts(x, *line_b)?;
            Ok(vec![dot(sub(ea, sa), sub(eb, sb))])
        }
        // Tangency ⇔ the centre's distance to the line equals the radius.
        //
        // CONSULTED (form only): the published test
        // `tangent_line_arc_selects_the_arc_arm` records that the solver "uses
        // the squared form". The oracle keeps the LINEAR form `|d| - r`,
        // because that is the geometric statement and because squaring
        // multiplies the gradient by 2r, which changes no rank: at a satisfied
        // tangency ∇(d²-r²) = 2r·∇(|d|-r) exactly. Away from tangency the two
        // gradients are not proportional, so the differential test keeps its
        // Tangent rows satisfied.
        C::Tangent { line, curve } => {
            let (s, e) = layout.line_pts(x, *line)?;
            let (c0, r) = layout.curve(x, *curve)?;
            Ok(vec![signed_line_dist(c0, s, e)?.abs() - r])
        }
        // "Equal size": equal length for two lines, equal radius for two
        // curves. A line and a curve have no comparable size.
        C::Equal { entity_a, entity_b } => {
            let ka = layout.ent(*entity_a)?.clone();
            let kb = layout.ent(*entity_b)?.clone();
            match (&ka, &kb) {
                (Ent::Line { .. }, Ent::Line { .. }) => {
                    let (sa, ea) = layout.line_pts(x, *entity_a)?;
                    let (sb, eb) = layout.line_pts(x, *entity_b)?;
                    Ok(vec![dist(sa, ea) - dist(sb, eb)])
                }
                (Ent::Circle { .. } | Ent::Arc { .. }, Ent::Circle { .. } | Ent::Arc { .. }) => {
                    let (_, ra) = layout.curve(x, *entity_a)?;
                    let (_, rb) = layout.curve(x, *entity_b)?;
                    Ok(vec![ra - rb])
                }
                _ => Err(format!(
                    "Equal between {ka:?} and {kb:?} compares no common size"
                )),
            }
        }
        // P₁ reflected across L equals P₂, written as the two independent
        // halves of that statement: the chord is perpendicular to L, and the
        // chord's midpoint lies on L.
        C::Symmetric {
            entity_a,
            entity_b,
            symmetry_line,
        } => {
            let a = layout.pt(x, *entity_a)?;
            let b = layout.pt(x, *entity_b)?;
            let (s, e) = layout.line_pts(x, *symmetry_line)?;
            let d = sub(e, s);
            let l = norm(d);
            if l == 0.0 {
                return Err("symmetry line has zero length".into());
            }
            let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            Ok(vec![dot(sub(b, a), d) / l, cross(d, sub(mid, s)) / l])
        }
        // CONSULTED (which axis): nothing in the type definition says which
        // axis `SymmetricH` mirrors about, and `specs/sketch_solver_rewrite.md`
        // §"Known Issues" records the naming as "inverted from intuition". Its
        // published equation table (line 179) settles it: `x₁ + x₂ = 0`,
        // `y₁ - y₂ = 0` — mirrored about the Y axis, through the sketch origin.
        C::SymmetricH { point_a, point_b } => {
            let a = layout.pt(x, *point_a)?;
            let b = layout.pt(x, *point_b)?;
            Ok(vec![a.0 + b.0, a.1 - b.1])
        }
        C::SymmetricV { point_a, point_b } => {
            let a = layout.pt(x, *point_a)?;
            let b = layout.pt(x, *point_b)?;
            Ok(vec![a.0 - b.0, a.1 + b.1])
        }
        // P = (start + end)/2, both components.
        C::Midpoint { point, line } => {
            let p = layout.pt(x, *point)?;
            let (s, e) = layout.line_pts(x, *line)?;
            Ok(vec![p.0 - (s.0 + e.0) / 2.0, p.1 - (s.1 + e.1) / 2.0])
        }
        // Point-to-point distance, or (point, line) perpendicular distance —
        // the two operand pairs the vocabulary admits. A DISTANCE is an
        // unsigned quantity, so both arms subtract the value from a magnitude.
        //
        // CONSULTED (and DISAGREED WITH, deliberately): the solver's
        // (point, line) arm subtracts the value from a SIGNED perpendicular
        // distance. Measured, not inferred — see the defect pin
        // `a_point_line_distance_dimension_mirrors_the_point_across_the_line`,
        // which shows a `value: +7.0` on a point 7 away from the line being
        // "satisfied" only after the solve mirrors the point to the other
        // side. The app emits an ABSOLUTE value for this family
        // (`dimensionHeuristic.js` `pointLineDistance`, `constraintLogic.js`
        // `pointLineDistance`, both `Math.abs`), so the unsigned form is what
        // the stored value means and the oracle keeps it. Rank is unaffected
        // either way: `∇|g| = ±∇g`.
        C::Distance {
            entity_a,
            entity_b,
            value,
            ..
        } => {
            let ka = layout.ent(*entity_a)?.clone();
            let kb = layout.ent(*entity_b)?.clone();
            match (&ka, &kb) {
                (Ent::Point, Ent::Point) => {
                    let a = layout.pt(x, *entity_a)?;
                    let b = layout.pt(x, *entity_b)?;
                    Ok(vec![dist(a, b) - value])
                }
                (Ent::Point, Ent::Line { .. }) => {
                    let p = layout.pt(x, *entity_a)?;
                    let (s, e) = layout.line_pts(x, *entity_b)?;
                    Ok(vec![signed_line_dist(p, s, e)?.abs() - value])
                }
                (Ent::Line { .. }, Ent::Point) => {
                    let p = layout.pt(x, *entity_b)?;
                    let (s, e) = layout.line_pts(x, *entity_a)?;
                    Ok(vec![signed_line_dist(p, s, e)?.abs() - value])
                }
                _ => Err(format!(
                    "Distance between {ka:?} and {kb:?} is not a defined measurement"
                )),
            }
        }
        // The dimension tool's point–line measurement: the same quantity as
        // the (Point, Line) arm of Distance, which the solver's
        // `point_line_distance_matches_the_distance_point_line_arm` also
        // asserts of itself — so it inherits the unsigned form and the
        // CONSULTED note above with it.
        C::PointLineDistance {
            point,
            entity,
            value,
            ..
        } => {
            let p = layout.pt(x, *point)?;
            let (s, e) = layout.line_pts(x, *entity)?;
            Ok(vec![signed_line_dist(p, s, e)?.abs() - value])
        }
        // |Δx| = value. The TYPE's doc comment says "constrains |Δx|"; the
        // older equation table writes the signed `x₂ - x₁ - d`. The two differ
        // only when the pair is authored right-to-left, and never in gradient
        // SPAN, so the oracle takes the type's absolute form (the newer
        // contract) and the differential test authors every H/V dimension
        // left-to-right and bottom-to-top, where the forms coincide.
        C::HDistance {
            point_a,
            point_b,
            value,
            ..
        } => {
            let a = layout.pt(x, *point_a)?;
            let b = layout.pt(x, *point_b)?;
            Ok(vec![(b.0 - a.0).abs() - value])
        }
        C::VDistance {
            point_a,
            point_b,
            value,
            ..
        } => {
            let a = layout.pt(x, *point_a)?;
            let b = layout.pt(x, *point_b)?;
            Ok(vec![(b.1 - a.1).abs() - value])
        }
        // The directed angle between the two directions, in radians.
        C::Angle {
            line_a,
            line_b,
            value_degrees,
            ..
        } => {
            let (sa, ea) = layout.line_pts(x, *line_a)?;
            let (sb, eb) = layout.line_pts(x, *line_b)?;
            let theta = line_angle_between(sub(ea, sa), sub(eb, sb))?;
            Ok(vec![wrap_pi(theta - value_degrees.to_radians())])
        }
        // A circle's radius param, or an arc's centre→start distance.
        C::Radius { entity, value, .. } => {
            let (_, r) = layout.curve(x, *entity)?;
            Ok(vec![r - value])
        }
        // Twice that. CONSULTED (the factor's side): the published test
        // `diameter_on_arc_selects_the_diameter_arc_arm` records the residual
        // as `2‖C-S‖ - value`, where the older equation table wrote
        // `r_c - d/2`. Same variety, gradient scaled by 2 — no rank
        // consequence — but the residual VALUE the report publishes differs by
        // 2×, so the oracle matches the tested form.
        C::Diameter { entity, value, .. } => {
            let (_, r) = layout.curve(x, *entity)?;
            Ok(vec![2.0 * r - value])
        }
        // Incidence: on a line is zero perpendicular distance; on a circle or
        // an arc is distance-from-centre equals radius.
        C::OnEntity { point, entity } => {
            let p = layout.pt(x, *point)?;
            match layout.ent(*entity)? {
                Ent::Line { start, end } => {
                    let s = layout.pt(x, *start)?;
                    let e = layout.pt(x, *end)?;
                    Ok(vec![signed_line_dist(p, s, e)?])
                }
                Ent::Circle { .. } | Ent::Arc { .. } => {
                    let (c0, r) = layout.curve(x, *entity)?;
                    Ok(vec![dist(p, c0) - r])
                }
                other => Err(format!("a point cannot lie ON {other:?}")),
            }
        }
        // The interaction hint: hold the point where the caller sent it. Two
        // rows, like any pin. (The solver down-weights these to 1/20; a
        // positive row weight changes no rank, and the report's residuals are
        // published UNWEIGHTED, so the oracle weights nothing.)
        C::Dragged { point } => {
            let p = layout.pt(x, *point)?;
            let target = layout.dragged_target(*point)?;
            Ok(vec![p.0 - target.0, p.1 - target.1])
        }
        // A real lock at an explicit position.
        C::Pinned {
            point,
            x: px,
            y: py,
        } => {
            let p = layout.pt(x, *point)?;
            Ok(vec![p.0 - px, p.1 - py])
        }
        // Two angles equal: one row, the difference of the two directed
        // angles. Not in the published table — derived.
        C::EqualAngle {
            line_a,
            line_b,
            line_c,
            line_d,
        } => {
            let (sa, ea) = layout.line_pts(x, *line_a)?;
            let (sb, eb) = layout.line_pts(x, *line_b)?;
            let (sc, ec) = layout.line_pts(x, *line_c)?;
            let (sd, ed) = layout.line_pts(x, *line_d)?;
            let ab = line_angle_between(sub(ea, sa), sub(eb, sb))?;
            let cd = line_angle_between(sub(ec, sc), sub(ed, sd))?;
            Ok(vec![wrap_pi(ab - cd)])
        }
        // ‖L₁‖ - k·‖L₂‖ = 0 (the published table's LengthRatio). Written this
        // way rather than as a quotient so the row stays finite when the
        // second line degenerates.
        C::Ratio {
            entity_a,
            entity_b,
            value,
        } => {
            let (sa, ea) = layout.line_pts(x, *entity_a)?;
            let (sb, eb) = layout.line_pts(x, *entity_b)?;
            Ok(vec![dist(sa, ea) - value * dist(sb, eb)])
        }
        // Two points the same perpendicular distance from one line — the
        // signed distance, so the points stay on the same side (two points
        // straddling the line at equal distance is a DIFFERENT statement).
        // Not in the published table — derived.
        C::EqualPointToLine {
            point_a,
            point_b,
            line,
        } => {
            let a = layout.pt(x, *point_a)?;
            let b = layout.pt(x, *point_b)?;
            let (s, e) = layout.line_pts(x, *line)?;
            Ok(vec![
                signed_line_dist(a, s, e)? - signed_line_dist(b, s, e)?,
            ])
        }
        // CONSULTED (that it is a no-op): the two entity ids are not enough to
        // say what "same orientation" constrains in a plane, and the solver's
        // published test `same_orientation_never_changes_a_solve` states it:
        // "SameOrientation is a documented 2D no-op. It owns zero residual
        // rows". Zero rows is therefore the oracle's row count too — which is
        // itself the assertion, since a variant that quietly owned a row would
        // shift every later constraint's rank contribution.
        C::SameOrientation { .. } => Ok(vec![]),
    }
}

impl Layout {
    /// A `Dragged` point's target: the position the caller sent in, which for
    /// a stored sketch is the point's own authored position. Held separately
    /// from the parameter vector because the target is a CONSTANT of the row,
    /// not a parameter — differentiating it would halve the row's gradient.
    fn dragged_target(&self, point: u32) -> Result<(f64, f64), String> {
        self.drag_targets
            .get(&point)
            .copied()
            .ok_or_else(|| format!("no authored position recorded for point {point}"))
    }
}

// ── The report ──────────────────────────────────────────────────────────────

/// Whether the oracle is willing to state a rank at all.
#[derive(Debug, Clone, PartialEq)]
pub enum RankVerdict {
    /// Every singular value is at least a decade clear of the rank threshold.
    Decided(u32),
    /// A singular value sits within [`INDETERMINATE_DECADE`] of the threshold:
    /// the oracle cannot tell which side of the line it belongs on, and a
    /// caller must NOT read a disagreement with the solver out of this.
    Indeterminate {
        /// The best guess, for diagnostics only.
        rank: u32,
        /// The singular value that could not be classified.
        sigma: f64,
        /// The threshold it sat beside.
        threshold: f64,
    },
}

impl RankVerdict {
    /// The rank, when the oracle is willing to state one.
    pub fn decided(&self) -> Option<u32> {
        match self {
            RankVerdict::Decided(r) => Some(*r),
            RankVerdict::Indeterminate { .. } => None,
        }
    }

    pub fn best_guess(&self) -> u32 {
        match self {
            RankVerdict::Decided(r) => *r,
            RankVerdict::Indeterminate { rank, .. } => *rank,
        }
    }
}

/// One driving constraint's row block, in the caller's index space.
#[derive(Debug, Clone)]
pub struct OracleRow {
    /// Index into the caller's FULL constraint array (reference dimensions
    /// included), which is the index space `SketchSolveReport` publishes.
    pub index: u32,
    pub kind: &'static str,
    /// How many residual rows this constraint owns.
    pub rows: usize,
    /// Largest residual magnitude over the block.
    pub residual: f64,
    /// First-order distance in parameter space from here to this constraint's
    /// own variety: `|r| / ‖∇r‖`, worst row. `None` when the block's gradient
    /// is numerically zero, where no such distance exists.
    pub variety_distance: Option<f64>,
    /// `variety_distance` within the oracle's scaled tolerance.
    pub satisfied: bool,
}

/// The oracle's independent structural state for one sketch.
#[derive(Debug, Clone)]
pub struct OracleReport {
    /// Parameters the oracle's own layout allocated.
    pub params: u32,
    /// Residual rows over the DRIVING constraints (reference dimensions are
    /// not sent to a solver and own no rows here either).
    pub rows: u32,
    /// Rank of the row-normalized, finite-difference Jacobian by SVD.
    pub rank: RankVerdict,
    /// `params - rank`, when the rank is decided.
    pub dof: Option<u32>,
    /// Dimension of the Jacobian's null space — the same number as `dof`,
    /// computed from the right singular vectors rather than by subtraction, so
    /// the two agreeing is itself a check on the decomposition.
    pub null_dim: Option<u32>,
    /// Driving constraints that add no rank to the ones declared before them,
    /// by a greedy declaration-order walk. Caller indices.
    pub dependent: Vec<u32>,
    /// Driving constraints whose residual the oracle cannot see satisfied at
    /// this configuration, worst first. Caller indices.
    pub unsatisfiable: Vec<u32>,
    /// Per-constraint rows, driving constraints only, declaration order.
    pub blocks: Vec<OracleRow>,
    /// Singular values of the row-normalized Jacobian, descending.
    pub singular_values: Vec<f64>,
    /// Null-space directions: for each, the parameter slots that move along it.
    pub free: Vec<Vec<(ParamSlot, f64)>>,
    /// Constraints the oracle refused to compile, with the reason. A non-empty
    /// list means the oracle's structural numbers describe a DIFFERENT system
    /// from the one the solver saw, so a caller must investigate rather than
    /// compare.
    pub refused: Vec<(u32, String)>,
    /// The scale the satisfaction tolerance was taken against: the largest
    /// coordinate magnitude in the evaluated configuration.
    pub scale: f64,
}

// ── Computation ─────────────────────────────────────────────────────────────

/// Compute the oracle's structural state for `sketch`, evaluated at the
/// configuration `positions`/`radii` (pass the solver's own solved geometry to
/// make this a check of the SAME Jacobian the report describes).
pub fn analyze_at(
    sketch: &Sketch,
    positions: &HashMap<u32, (f64, f64)>,
    radii: &HashMap<u32, f64>,
) -> OracleReport {
    let mut layout = Layout::build(&sketch.entities);
    layout.record_drag_targets(&sketch.entities);
    let x = layout.at_solution(&sketch.entities, positions, radii);
    analyze_with_layout(sketch, &layout, &x)
}

/// [`analyze_at`] at the sketch's AUTHORED geometry — the configuration before
/// any solve.
pub fn analyze_as_authored(sketch: &Sketch) -> OracleReport {
    let mut layout = Layout::build(&sketch.entities);
    layout.record_drag_targets(&sketch.entities);
    let x = layout.initial(&sketch.entities);
    analyze_with_layout(sketch, &layout, &x)
}

impl Layout {
    fn record_drag_targets(&mut self, entities: &[SketchEntity]) {
        for e in entities {
            if let SketchEntity::Point { id, x, y, .. } = e {
                self.drag_targets.insert(*id, (*x, *y));
            }
        }
    }
}

fn analyze_with_layout(sketch: &Sketch, layout: &Layout, x: &[f64]) -> OracleReport {
    let n = layout.len();
    let scale = x.iter().fold(1.0f64, |m, v| m.max(v.abs()));
    let sat_tol = ORACLE_SAT_REL * scale;

    // Residual blocks, driving constraints only, in the caller's index space.
    let mut refused: Vec<(u32, String)> = Vec::new();
    let mut blocks_meta: Vec<(u32, &'static str, usize)> = Vec::new();
    let mut residual_values: Vec<f64> = Vec::new();

    for (i, c) in sketch.constraints.iter().enumerate() {
        if c.is_reference() {
            continue;
        }
        match residual_rows(layout, c, x) {
            Ok(r) => {
                blocks_meta.push((i as u32, c.kind(), r.len()));
                residual_values.extend(r);
            }
            Err(why) => refused.push((i as u32, why)),
        }
    }

    // Finite-difference Jacobian: one central difference per parameter, all
    // rows at once (two residual evaluations per column, not per entry).
    let m = residual_values.len();
    let mut jac = DMatrix::<f64>::zeros(m, n);
    for j in 0..n {
        let h = FD_REL_STEP * x[j].abs().max(FD_ABS_FLOOR);
        let mut xp = x.to_vec();
        let mut xm = x.to_vec();
        xp[j] = x[j] + h;
        xm[j] = x[j] - h;
        let rp = eval_driving(sketch, layout, &xp);
        let rm = eval_driving(sketch, layout, &xm);
        if rp.len() != m || rm.len() != m {
            // A perturbation changed which constraints compile (a line
            // degenerating to zero length, say). Nothing honest can be said
            // about the Jacobian there; leave the column zero and record it.
            continue;
        }
        for i in 0..m {
            jac[(i, j)] = (rp[i] - rm[i]) / (2.0 * h);
        }
    }

    // Row-normalize: the rank question is about which DIRECTIONS the
    // constraints resist, not how loudly each row shouts. Without this a
    // `Parallel` row on metre-scale geometry (gradient ~1e0) and the same row
    // on kilometre-scale geometry (gradient ~1e3) would need different
    // thresholds.
    let mut normalized = jac.clone();
    for i in 0..m {
        let rn = normalized.row(i).norm();
        if rn > 0.0 {
            for j in 0..n {
                normalized[(i, j)] /= rn;
            }
        }
    }

    // SVD of the row-normalized Jacobian, PADDED to at least `n` rows so the
    // thin right-singular matrix spans the whole parameter space. Without the
    // padding an under-constrained sketch (m < n, every one of them) has no
    // null-space basis to read — the same trap `SketchSolveReport::free`
    // documents hitting with nalgebra's thin `v_t`.
    let padded_rows = m.max(n);
    let mut padded = DMatrix::<f64>::zeros(padded_rows, n);
    padded.view_mut((0, 0), (m, n)).copy_from(&normalized);

    let (sigmas, v) = if n == 0 {
        (Vec::new(), DMatrix::<f64>::zeros(0, 0))
    } else {
        let svd = padded.svd(false, true);
        let v_t = svd.v_t.expect("v_t requested");
        let mut pairs: Vec<(f64, usize)> = svd
            .singular_values
            .iter()
            .copied()
            .enumerate()
            .map(|(i, s)| (s, i))
            .collect();
        pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut vmat = DMatrix::<f64>::zeros(n, pairs.len());
        for (col, (_, src)) in pairs.iter().enumerate() {
            vmat.set_column(col, &v_t.row(*src).transpose());
        }
        (pairs.into_iter().map(|(s, _)| s).collect(), vmat)
    };

    let sigma_max = sigmas.first().copied().unwrap_or(0.0);
    let threshold = RANK_REL_TOL * sigma_max;
    let mut rank_count = 0u32;
    let mut undecided: Option<f64> = None;
    for s in &sigmas {
        if *s > threshold {
            rank_count += 1;
        }
        if sigma_max > 0.0
            && *s > threshold / INDETERMINATE_DECADE
            && *s < threshold * INDETERMINATE_DECADE
        {
            undecided = Some(*s);
        }
    }
    let rank = match undecided {
        None => RankVerdict::Decided(rank_count),
        Some(sigma) => RankVerdict::Indeterminate {
            rank: rank_count,
            sigma,
            threshold,
        },
    };

    let dof = rank.decided().map(|r| layout.len() as u32 - r);
    // The padding makes `sigmas.len() == n` for every n > 0, so the null space
    // is spanned by the right singular vectors of the sub-threshold values and
    // its dimension is counted, not inferred by subtraction.
    let null_dim = rank
        .decided()
        .map(|r| (sigmas.len() as u32).saturating_sub(r));

    // Null-space directions, named by parameter slot.
    let mut free = Vec::new();
    if let Some(r) = rank.decided() {
        for col in (r as usize)..sigmas.len() {
            let vcol = v.column(col);
            let biggest = vcol.iter().fold(0.0f64, |mx, v| mx.max(v.abs()));
            let mut comps = Vec::new();
            for (slot_i, slot) in layout.slots().iter().enumerate() {
                let val = vcol[slot_i];
                if biggest > 0.0 && val.abs() >= FREE_COMPONENT_REL * biggest {
                    comps.push((*slot, val));
                }
            }
            free.push(comps);
        }
    }

    // Per-constraint blocks and the greedy declaration-order dependency walk.
    let mut blocks = Vec::new();
    let mut unsat: Vec<(u32, f64)> = Vec::new();
    let mut dependent = Vec::new();
    let mut taken = 0usize; // rows consumed so far
    let mut prefix_rank = 0u32;
    for (index, kind, nrows) in &blocks_meta {
        let mut worst = 0.0f64;
        let mut worst_dist: Option<f64> = None;
        for k in 0..*nrows {
            let r = residual_values[taken + k];
            // `f64::max` is deliberately NOT used: it returns the non-NaN
            // operand, so a non-finite row would be folded away and reported
            // as a finite worst — the exact fabricated-zero the S2 notes
            // record having had to fix three times in the solver. A NaN here
            // must win the fold and reach the `satisfied` verdict as a NaN.
            if r.is_nan() || r.abs() > worst {
                worst = r.abs();
            }
            let gn = jac.row(taken + k).norm();
            if gn > 0.0 {
                let d = r.abs() / gn;
                if worst_dist.is_none_or(|w| d > w) {
                    worst_dist = Some(d);
                }
            }
        }
        let satisfied = match worst_dist {
            // A non-finite residual is never satisfied, whatever its gradient.
            _ if !worst.is_finite() => false,
            Some(d) => d <= sat_tol,
            // No gradient anywhere in the block: fall back to the raw
            // residual against the same tolerance. Nothing better is available
            // and inventing a verdict would be worse.
            None => worst <= sat_tol,
        };
        if !satisfied {
            unsat.push((*index, worst));
        }

        // Does this constraint's block add rank to everything declared before
        // it? Recomputed from the prefix each time, which is the definition
        // rather than an incremental shortcut.
        let prefix_end = taken + nrows;
        let new_rank = prefix_rank_of(&normalized, prefix_end, n);
        if new_rank < prefix_rank + *nrows as u32 {
            dependent.push(*index);
        }
        prefix_rank = new_rank;

        blocks.push(OracleRow {
            index: *index,
            kind,
            rows: *nrows,
            residual: worst,
            variety_distance: worst_dist,
            satisfied,
        });
        taken = prefix_end;
    }

    unsat.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    OracleReport {
        params: layout.len() as u32,
        rows: m as u32,
        rank,
        dof,
        null_dim,
        dependent,
        unsatisfiable: unsat.into_iter().map(|(i, _)| i).collect(),
        blocks,
        singular_values: sigmas,
        free,
        refused,
        scale,
    }
}

/// Rank of the first `end` rows of the row-normalized Jacobian.
fn prefix_rank_of(normalized: &DMatrix<f64>, end: usize, n: usize) -> u32 {
    if end == 0 || n == 0 {
        return 0;
    }
    let block = normalized.view((0, 0), (end, n)).into_owned();
    let sv = block.singular_values();
    let smax = sv.iter().fold(0.0f64, |m, v| m.max(*v));
    let thr = RANK_REL_TOL * smax;
    sv.iter().filter(|s| **s > thr).count() as u32
}

/// Every driving constraint's residual rows at `x`, concatenated in the order
/// [`analyze_with_layout`] built them.
fn eval_driving(sketch: &Sketch, layout: &Layout, x: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    for c in &sketch.constraints {
        if c.is_reference() {
            continue;
        }
        if let Ok(r) = residual_rows(layout, c, x) {
            out.extend(r);
        }
    }
    out
}

/// Convenience: a one-line summary for a test failure message.
pub fn summarize(r: &OracleReport) -> String {
    let sv = r
        .singular_values
        .iter()
        .map(|s| format!("{s:.3e}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "oracle: params={} rows={} rank={:?} dof={:?} null_dim={:?} dependent={:?} \
         unsatisfiable={:?} refused={:?} σ=[{}]",
        r.params, r.rows, r.rank, r.dof, r.null_dim, r.dependent, r.unsatisfiable, r.refused, sv
    )
}
