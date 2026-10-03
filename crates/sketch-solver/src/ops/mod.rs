//! Sketch operations (`specs/agent_mechanical_design.md` §10.1, S1).
//!
//! Trim, extend, offset, sketch fillet, mirror and projection, as pure
//! functions on a [`Sketch`], each returning a [`SketchEdit`]. Before this
//! they were browser JavaScript with no Rust twin — `handleTrimTool`,
//! `offsetChainSegments`, `executeSketchFillet` — extend and mirror did not
//! exist, and an agent could only rewrite a whole sketch. The UI's pointer
//! handling and rendering stay in JS; everything that decides WHERE geometry
//! goes is here, so the UI and the MCP run one implementation (§10.1: parity
//! by construction, not by porting twice).
//!
//! Three decisions worth stating, because they differ from the JS they
//! replace and they are improvements rather than transcription:
//!
//! 1. **Operations reuse point ids wherever the geometry survives.** The JS
//!    tools deleted an entity and re-created it, which dropped every
//!    constraint that named it (`removeSketchEntities` cascades to
//!    constraints). A trim now keeps the surviving half's far endpoint, and a
//!    fillet rewrites the two lines' corner endpoint rather than re-creating
//!    the lines, so a dimension on the kept geometry survives the operation.
//! 2. **Nothing is a silent no-op.** Every refusal is a typed
//!    [`SketchOpError`]; the JS returned early on a missing corner, a radius
//!    that would not fit, or a branching chain, which looked exactly like
//!    success.
//! 3. **Snapping stays in the UI.** These functions weld only on exact
//!    coincidence ([`POINT_WELD_TOL`]). Which nearby vertex a cursor meant is
//!    an interaction decision; the caller passes the position it decided on.

pub mod chain;
pub mod geom;
pub mod offset;

use std::collections::{HashMap, HashSet};

use crate::types::{Sketch, SketchConstraint, SketchEntity};
use chain::order_chain;
use geom::{
    angle_bisector, arc_line_intersections, entity_radius, line_circle_intersections,
    line_line_intersection, parameter_on_segment, perpendicular_foot, positions_of, Point2,
    Positions,
};
use offset::{offset_chain_segments, resolve_chain_segments, OffsetError, Segment};
use waffle_types::sketch_plane::SketchPlaneBasis;
use waffle_types::{ProjectShape, ProjectedEntity, ProjectedPoint};

pub use waffle_types::{End, ProjectedSource, Side, SketchEdit, SketchOp, SketchOpError};

/// Two minted points closer than this are the same point. Deliberately tight:
/// it exists to stop an operation from minting a duplicate of a vertex it
/// just made, not to snap to the user's intent — that is the UI's job and it
/// passes a decided position.
pub const POINT_WELD_TOL: f64 = 1e-9;

/// Parameter margin for "the cursor is inside this piece of the line", and
/// for discarding an intersection that lands on an endpoint. The JS trim
/// tool's `0.001` in normalized line parameter.
const TRIM_PARAM_EPS: f64 = 0.001;

/// Mints entity ids that collide with nothing in the sketch and nothing the
/// caller has already handed out.
///
/// Seeded from `max(hint, max_existing + 1)`: the UI keeps its own counter
/// (`nextEntityId`) and an agent has none, so the allocator takes the
/// caller's hint when it has one and otherwise derives a safe floor from the
/// sketch itself.
#[derive(Debug, Clone)]
pub struct IdAllocator {
    next: u32,
}

impl IdAllocator {
    pub fn for_sketch(sketch: &Sketch, hint: u32) -> Self {
        let max_existing = sketch.entities.iter().map(|e| e.id()).max().unwrap_or(0);
        IdAllocator {
            next: hint.max(max_existing + 1).max(1),
        }
    }

    pub fn next_id(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }

    /// The id the next call would mint — what a caller stores to keep its own
    /// counter past what this batch used.
    pub fn peek(&self) -> u32 {
        self.next
    }
}

/// A batch of operations, applied.
#[derive(Debug, Clone)]
pub struct AppliedOps {
    /// The sketch after every op. Derived data (`solved_positions`,
    /// `solved_profiles`) is NOT recomputed here — the caller solves once at
    /// the end, which is what makes the batch one undo step (§10.3).
    pub sketch: Sketch,
    /// Everything the batch changed, folded in order.
    pub edit: SketchEdit,
    /// Constraints to hand the FOLLOWING solve and then drop: a `MovePoint`'s
    /// pin. They are not in `sketch.constraints` and must not be persisted.
    pub transient_constraints: Vec<SketchConstraint>,
    /// The next free entity id after the batch.
    pub next_id: u32,
}

// ── Entity lookup helpers ───────────────────────────────────────────────────

fn entity(sketch: &Sketch, id: u32) -> Result<&SketchEntity, SketchOpError> {
    sketch
        .entities
        .iter()
        .find(|e| e.id() == id)
        .ok_or(SketchOpError::NoSuchEntity { id })
}

fn kind_name(e: &SketchEntity) -> &'static str {
    match e {
        SketchEntity::Point { .. } => "Point",
        SketchEntity::Line { .. } => "Line",
        SketchEntity::Circle { .. } => "Circle",
        SketchEntity::Arc { .. } => "Arc",
        SketchEntity::Spline { .. } => "Spline",
        SketchEntity::Gear { .. } => "Gear",
        SketchEntity::Sprocket { .. } => "Sprocket",
    }
}

fn line_ends(sketch: &Sketch, id: u32) -> Result<(u32, u32), SketchOpError> {
    match entity(sketch, id)? {
        SketchEntity::Line {
            start_id, end_id, ..
        } => Ok((*start_id, *end_id)),
        other => Err(SketchOpError::WrongEntityKind {
            id,
            expected: "Line".to_string(),
            found: kind_name(other).to_string(),
        }),
    }
}

fn position(positions: &Positions, id: u32) -> Result<Point2, SketchOpError> {
    positions
        .get(&id)
        .copied()
        .ok_or(SketchOpError::MissingPosition { id })
}

/// Point ids an entity references (a curve's endpoints and an arc's centre).
fn referenced_points(e: &SketchEntity) -> Vec<u32> {
    match e {
        SketchEntity::Line {
            start_id, end_id, ..
        } => vec![*start_id, *end_id],
        SketchEntity::Circle { center_id, .. } => vec![*center_id],
        SketchEntity::Arc {
            center_id,
            start_id,
            end_id,
            ..
        } => vec![*center_id, *start_id, *end_id],
        SketchEntity::Spline { point_ids, .. } => point_ids.clone(),
        _ => Vec::new(),
    }
}

/// Entity ids a constraint names.
fn constraint_refs(c: &SketchConstraint) -> Vec<u32> {
    use SketchConstraint as C;
    match c {
        C::Coincident { point_a, point_b }
        | C::HorizontalPoints { point_a, point_b }
        | C::VerticalPoints { point_a, point_b }
        | C::SymmetricH { point_a, point_b }
        | C::SymmetricV { point_a, point_b } => vec![*point_a, *point_b],
        C::Horizontal { entity } | C::Vertical { entity } => vec![*entity],
        C::Parallel { line_a, line_b } | C::Perpendicular { line_a, line_b } => {
            vec![*line_a, *line_b]
        }
        C::Tangent { line, curve } => vec![*line, *curve],
        C::Equal { entity_a, entity_b }
        | C::Ratio {
            entity_a, entity_b, ..
        }
        | C::SameOrientation { entity_a, entity_b } => vec![*entity_a, *entity_b],
        C::Symmetric {
            entity_a,
            entity_b,
            symmetry_line,
        } => vec![*entity_a, *entity_b, *symmetry_line],
        C::Midpoint { point, line } => vec![*point, *line],
        C::Distance {
            entity_a, entity_b, ..
        } => vec![*entity_a, *entity_b],
        C::PointLineDistance { point, entity, .. } => vec![*point, *entity],
        C::HDistance {
            point_a, point_b, ..
        }
        | C::VDistance {
            point_a, point_b, ..
        } => {
            vec![*point_a, *point_b]
        }
        C::Angle { line_a, line_b, .. } => vec![*line_a, *line_b],
        C::Radius { entity, .. } | C::Diameter { entity, .. } => vec![*entity],
        C::OnEntity { point, entity } => vec![*point, *entity],
        C::Dragged { point } | C::Pinned { point, .. } => vec![*point],
        C::EqualAngle {
            line_a,
            line_b,
            line_c,
            line_d,
        } => vec![*line_a, *line_b, *line_c, *line_d],
        C::EqualPointToLine {
            point_a,
            point_b,
            line,
        } => vec![*point_a, *point_b, *line],
    }
}

// ── Removal, with its cascade ───────────────────────────────────────────────

/// Remove entities, cascading to the curves that reference a removed point,
/// the points no surviving entity still uses, and the constraints that name
/// any of them.
///
/// A port of the store's `removeSketchEntities` (`store.svelte.js`), which is
/// the only cascade rule in the product and was JavaScript-only: an agent
/// deleting through `sketch_create` had to work it out for itself.
pub fn remove_entities(sketch: &Sketch, ids: &[u32]) -> SketchEdit {
    let mut to_remove: HashSet<u32> = ids.iter().copied().collect();
    let mut referenced: HashSet<u32> = HashSet::new();
    for e in &sketch.entities {
        if to_remove.contains(&e.id()) {
            referenced.extend(referenced_points(e));
        }
    }

    // A curve whose point is going must go too, and its own points become
    // candidates for orphaning. Repeat to a fixed point: a chain of splines
    // through shared points can cascade more than one level.
    loop {
        let mut grew = false;
        for e in &sketch.entities {
            if to_remove.contains(&e.id()) {
                continue;
            }
            let pts = referenced_points(e);
            if !pts.is_empty() && pts.iter().any(|p| to_remove.contains(p)) {
                to_remove.insert(e.id());
                referenced.extend(pts);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }

    let mut used: HashSet<u32> = HashSet::new();
    for e in &sketch.entities {
        if !to_remove.contains(&e.id()) {
            used.extend(referenced_points(e));
        }
    }
    for pt in &referenced {
        if !used.contains(pt) {
            to_remove.insert(*pt);
        }
    }

    let mut removed: Vec<u32> = sketch
        .entities
        .iter()
        .map(|e| e.id())
        .filter(|id| to_remove.contains(id))
        .collect();
    removed.sort_unstable();

    let constraints_removed: Vec<u32> = sketch
        .constraints
        .iter()
        .enumerate()
        .filter(|(_, c)| constraint_refs(c).iter().any(|id| to_remove.contains(id)))
        .map(|(i, _)| i as u32)
        .collect();

    SketchEdit {
        removed,
        constraints_removed,
        ..Default::default()
    }
}

/// Drop the candidate points that NOTHING references once `edit` is applied,
/// and the constraints that named them.
///
/// An operation that repoints a curve (a trim's surviving half, a fillet's
/// shortened legs) releases the vertices it moved off. Those have to go, or
/// the sketch accumulates invisible free points — two parameters each, which
/// is `dof` the user cannot see and cannot constrain. Only the named
/// candidates are considered: a point drawn on its own is legitimately
/// unreferenced and is nobody's orphan.
fn prune_released_points(sketch: &Sketch, edit: &mut SketchEdit, candidates: &[u32]) {
    if candidates.is_empty() {
        return;
    }
    let mut after = sketch.clone();
    apply_edit(&mut after, edit);
    let mut still_used: HashSet<u32> = HashSet::new();
    for e in &after.entities {
        still_used.extend(referenced_points(e));
    }
    let mut dropped: HashSet<u32> = HashSet::new();
    for c in candidates {
        if !still_used.contains(c) && after.entities.iter().any(|e| e.id() == *c) {
            edit.removed.push(*c);
            dropped.insert(*c);
        }
    }
    if dropped.is_empty() {
        return;
    }
    for (i, c) in sketch.constraints.iter().enumerate() {
        let index = i as u32;
        if !edit.constraints_removed.contains(&index)
            && constraint_refs(c).iter().any(|id| dropped.contains(id))
        {
            edit.constraints_removed.push(index);
        }
    }
    edit.constraints_removed.sort_unstable();
}

// ── Trim ────────────────────────────────────────────────────────────────────

/// Every intersection of `entity` with the sketch's other entities, as points.
///
/// Lines only, like the JS: the piece-wise trim of an arc or a circle needs
/// an angular bracket rather than a parameter one, and `trim` removes a
/// non-line whole (see its docs).
fn entity_intersections(sketch: &Sketch, id: u32, positions: &Positions) -> Vec<Point2> {
    let Ok((start_id, end_id)) = line_ends(sketch, id) else {
        return Vec::new();
    };
    let (Ok(p1), Ok(p2)) = (position(positions, start_id), position(positions, end_id)) else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for other in &sketch.entities {
        if other.id() == id {
            continue;
        }
        match other {
            SketchEntity::Line {
                start_id: os,
                end_id: oe,
                ..
            } => {
                let (Some(p3), Some(p4)) = (positions.get(os), positions.get(oe)) else {
                    continue;
                };
                if let Some(p) = line_line_intersection(p1, p2, *p3, *p4) {
                    let t1 = parameter_on_segment(p, p1, p2);
                    let t2 = parameter_on_segment(p, *p3, *p4);
                    if t1 > TRIM_PARAM_EPS
                        && t1 < 1.0 - TRIM_PARAM_EPS
                        && t2 > -TRIM_PARAM_EPS
                        && t2 < 1.0 + TRIM_PARAM_EPS
                    {
                        results.push(p);
                    }
                }
            }
            SketchEntity::Circle {
                center_id, radius, ..
            } => {
                let Some(c) = positions.get(center_id) else {
                    continue;
                };
                for p in line_circle_intersections(p1, p2, *c, *radius) {
                    let t = parameter_on_segment(p, p1, p2);
                    if t > TRIM_PARAM_EPS && t < 1.0 - TRIM_PARAM_EPS {
                        results.push(p);
                    }
                }
            }
            SketchEntity::Arc {
                center_id,
                start_id: os,
                end_id: oe,
                ..
            } => {
                let (Some(c), Some(s), Some(e)) = (
                    positions.get(center_id),
                    positions.get(os),
                    positions.get(oe),
                ) else {
                    continue;
                };
                let r = c.dist(*s);
                let start_angle = (s.y - c.y).atan2(s.x - c.x);
                let mut end_angle = (e.y - c.y).atan2(e.x - c.x);
                if end_angle <= start_angle {
                    end_angle += geom::TWO_PI;
                }
                for p in arc_line_intersections(*c, r, start_angle, end_angle, p1, p2) {
                    let t = parameter_on_segment(p, p1, p2);
                    if t > TRIM_PARAM_EPS && t < 1.0 - TRIM_PARAM_EPS {
                        results.push(p);
                    }
                }
            }
            _ => {}
        }
    }
    results
}

/// The piece of a line a trim would take, for the hover highlight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrimPreview {
    pub start: Point2,
    pub end: Point2,
    /// Cuts found on the entity. Zero means a trim removes it whole.
    pub cuts: usize,
}

/// Which piece of `entity` the cursor at `at` is inside, and how many cuts
/// the entity has.
///
/// The same bracketing [`trim`] uses, so the highlight and the commit cannot
/// disagree. A non-line entity previews as its whole self with no cuts —
/// which is exactly what trimming it does.
pub fn trim_preview(
    sketch: &Sketch,
    entity_id: u32,
    at: Point2,
) -> Result<TrimPreview, SketchOpError> {
    let e = entity(sketch, entity_id)?;
    let positions = positions_of(&sketch.entities, &sketch.solved_positions);
    let Ok((start_id, end_id)) = line_ends(sketch, entity_id) else {
        // A curve: no piece-wise preview, and nothing claims there is one.
        let anchor = referenced_points(e)
            .first()
            .copied()
            .ok_or(SketchOpError::MissingPosition { id: entity_id })?;
        let p = position(&positions, anchor)?;
        return Ok(TrimPreview {
            start: p,
            end: p,
            cuts: 0,
        });
    };
    let p1 = position(&positions, start_id)?;
    let p2 = position(&positions, end_id)?;
    let intersections = entity_intersections(sketch, entity_id, &positions);
    if intersections.is_empty() {
        return Ok(TrimPreview {
            start: p1,
            end: p2,
            cuts: 0,
        });
    }
    let mut params: Vec<f64> = intersections
        .iter()
        .map(|p| parameter_on_segment(*p, p1, p2))
        .collect();
    params.push(0.0);
    params.push(1.0);
    params.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let cursor_t = parameter_on_segment(at, p1, p2);
    let (mut s, mut t) = (0.0f64, 1.0f64);
    for w in params.windows(2) {
        if w[0] <= cursor_t + 1e-8 && w[1] >= cursor_t - 1e-8 {
            s = w[0];
            t = w[1];
            break;
        }
    }
    let along = |u: f64| p1.plus(p2.minus(p1).scale(u));
    Ok(TrimPreview {
        start: along(s),
        end: along(t),
        cuts: intersections.len(),
    })
}

/// Trim the piece of `entity` that contains `at`.
///
/// A line is cut at every intersection with another entity and the piece
/// under the cursor is dropped; the pieces on either side survive, KEEPING
/// the original line's far endpoints (ids and all), so a dimension or a
/// coincidence on the kept end survives the trim. The JS deleted the line and
/// re-created both halves from scratch, which dropped them.
///
/// A line with no intersection, and any non-line entity, is removed whole —
/// the shipped behaviour. Piece-wise trimming of an arc or circle is not
/// implemented and is refused by nothing: it is the "no intersections" branch,
/// which deletes. That is a real limitation, inherited, and named here.
pub fn trim(
    sketch: &Sketch,
    entity_id: u32,
    at: Point2,
    ids: &mut IdAllocator,
) -> Result<SketchEdit, SketchOpError> {
    let e = entity(sketch, entity_id)?;
    if !matches!(e, SketchEntity::Line { .. }) {
        return Ok(remove_entities(sketch, &[entity_id]));
    }
    let (start_id, end_id) = line_ends(sketch, entity_id)?;
    let positions = positions_of(&sketch.entities, &sketch.solved_positions);
    let p1 = position(&positions, start_id)?;
    let p2 = position(&positions, end_id)?;

    let intersections = entity_intersections(sketch, entity_id, &positions);
    if intersections.is_empty() {
        return Ok(remove_entities(sketch, &[entity_id]));
    }

    // Bracket the cursor between the two cuts around it.
    let mut params: Vec<f64> = intersections
        .iter()
        .map(|p| parameter_on_segment(*p, p1, p2))
        .collect();
    params.push(0.0);
    params.push(1.0);
    params.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let cursor_t = parameter_on_segment(at, p1, p2);
    let (mut seg_start_t, mut seg_end_t) = (0.0f64, 1.0f64);
    for w in params.windows(2) {
        if w[0] <= cursor_t + 1e-8 && w[1] >= cursor_t - 1e-8 {
            seg_start_t = w[0];
            seg_end_t = w[1];
            break;
        }
    }

    let keep_head = seg_start_t > TRIM_PARAM_EPS;
    let keep_tail = seg_end_t < 1.0 - TRIM_PARAM_EPS;
    if !keep_head && !keep_tail {
        return Ok(remove_entities(sketch, &[entity_id]));
    }

    let along = |t: f64| p1.plus(p2.minus(p1).scale(t));
    let mut edit = SketchEdit::default();

    // The head keeps the ORIGINAL line id and start point; the tail, when
    // both survive, is a new line sharing the original end point.
    if keep_head {
        let cut = ids.next_id();
        let p = along(seg_start_t);
        edit.added.push(SketchEntity::Point {
            id: cut,
            x: p.x,
            y: p.y,
            construction: false,
        });
        edit.changed.push(SketchEntity::Line {
            id: entity_id,
            start_id,
            end_id: cut,
            construction: e.is_construction(),
        });
    }
    if keep_tail {
        let cut = ids.next_id();
        let p = along(seg_end_t);
        edit.added.push(SketchEntity::Point {
            id: cut,
            x: p.x,
            y: p.y,
            construction: false,
        });
        if keep_head {
            edit.added.push(SketchEntity::Line {
                id: ids.next_id(),
                start_id: cut,
                end_id,
                construction: e.is_construction(),
            });
        } else {
            edit.changed.push(SketchEntity::Line {
                id: entity_id,
                start_id: cut,
                end_id,
                construction: e.is_construction(),
            });
        }
    }
    // The endpoint on the trimmed-away side is released; it goes unless
    // another entity still stands on it.
    prune_released_points(sketch, &mut edit, &[start_id, end_id]);
    Ok(edit)
}

// ── Extend ──────────────────────────────────────────────────────────────────

/// Extend a line past one end until it meets another entity.
///
/// New in S1: the sketcher had no extend at all (§2.2 item 10), so there is
/// no JS behaviour to match. The rule is the obvious one — the line's own
/// infinite carrier, the nearest hit strictly BEYOND the end being extended,
/// and the hit must lie on the target's own span, not merely on its carrier,
/// so extending to a line does not stop at a point that line does not cover.
///
/// `to` names a single target; without it every other entity is a candidate.
pub fn extend(
    sketch: &Sketch,
    entity_id: u32,
    end: End,
    to: Option<u32>,
    ids: &mut IdAllocator,
) -> Result<SketchEdit, SketchOpError> {
    let e = entity(sketch, entity_id)?.clone();
    let (start_id, end_id) = line_ends(sketch, entity_id)?;
    let positions = positions_of(&sketch.entities, &sketch.solved_positions);
    let p1 = position(&positions, start_id)?;
    let p2 = position(&positions, end_id)?;
    if p1.dist(p2) < POINT_WELD_TOL {
        return Err(SketchOpError::NothingToExtendTo { entity: entity_id });
    }

    // Candidates on this line's carrier, with their parameter.
    let mut best: Option<(f64, Point2)> = None;
    let targets: Vec<&SketchEntity> = match to {
        Some(target) => vec![entity(sketch, target)?],
        None => sketch
            .entities
            .iter()
            .filter(|o| o.id() != entity_id)
            .collect(),
    };
    for other in targets {
        if other.id() == entity_id {
            continue;
        }
        let hits: Vec<Point2> = match other {
            SketchEntity::Line {
                start_id: os,
                end_id: oe,
                ..
            } => {
                let (Some(p3), Some(p4)) = (positions.get(os), positions.get(oe)) else {
                    continue;
                };
                line_line_intersection(p1, p2, *p3, *p4)
                    .into_iter()
                    .filter(|p| {
                        let t = parameter_on_segment(*p, *p3, *p4);
                        (-TRIM_PARAM_EPS..=1.0 + TRIM_PARAM_EPS).contains(&t)
                    })
                    .collect()
            }
            SketchEntity::Circle {
                center_id, radius, ..
            } => match positions.get(center_id) {
                Some(c) => line_circle_intersections(p1, p2, *c, *radius),
                None => continue,
            },
            SketchEntity::Arc {
                center_id,
                start_id: os,
                end_id: oe,
                ..
            } => {
                let (Some(c), Some(s), Some(t)) = (
                    positions.get(center_id),
                    positions.get(os),
                    positions.get(oe),
                ) else {
                    continue;
                };
                let r = c.dist(*s);
                let a0 = (s.y - c.y).atan2(s.x - c.x);
                let mut a1 = (t.y - c.y).atan2(t.x - c.x);
                if a1 <= a0 {
                    a1 += geom::TWO_PI;
                }
                arc_line_intersections(*c, r, a0, a1, p1, p2)
            }
            _ => continue,
        };
        for hit in hits {
            let t = parameter_on_segment(hit, p1, p2);
            // Strictly beyond the end being extended.
            let beyond = match end {
                End::Start => t < -TRIM_PARAM_EPS,
                End::End => t > 1.0 + TRIM_PARAM_EPS,
            };
            if !beyond {
                continue;
            }
            // Nearest first: smallest |t - 1| extending the end, |t| the start.
            let reach = match end {
                End::Start => -t,
                End::End => t - 1.0,
            };
            if best.is_none_or(|(b, _)| reach < b) {
                best = Some((reach, hit));
            }
        }
    }
    let (_, target_point) = best.ok_or(SketchOpError::NothingToExtendTo { entity: entity_id })?;

    let moving = match end {
        End::Start => start_id,
        End::End => end_id,
    };
    // A point another entity also uses cannot be dragged out from under it:
    // the extension gets its own point and the line is repointed.
    let shared = sketch
        .entities
        .iter()
        .any(|o| o.id() != entity_id && referenced_points(o).contains(&moving));
    let mut edit = SketchEdit::default();
    if shared {
        let fresh = ids.next_id();
        edit.added.push(SketchEntity::Point {
            id: fresh,
            x: target_point.x,
            y: target_point.y,
            construction: false,
        });
        edit.changed.push(match end {
            End::Start => SketchEntity::Line {
                id: entity_id,
                start_id: fresh,
                end_id,
                construction: e.is_construction(),
            },
            End::End => SketchEntity::Line {
                id: entity_id,
                start_id,
                end_id: fresh,
                construction: e.is_construction(),
            },
        });
    } else {
        let construction = entity(sketch, moving)?.is_construction();
        edit.changed.push(SketchEntity::Point {
            id: moving,
            x: target_point.x,
            y: target_point.y,
            construction,
        });
    }
    Ok(edit)
}

// ── Offset ──────────────────────────────────────────────────────────────────

/// The resolved geometry of a chain, ready to offset or to preview.
pub struct ResolvedChain {
    pub segments: Vec<Segment>,
    pub closed: bool,
}

/// Resolve a selection into offsettable segments: a single circle becomes one
/// whole-circle segment, anything else is ordered end-to-end.
///
/// Public because the UI's hover preview needs the same resolution the commit
/// will use — a preview computed by different code is a preview that can lie.
pub fn resolve_offset_chain(
    sketch: &Sketch,
    chain: &[u32],
    positions: &Positions,
) -> Result<ResolvedChain, OffsetError> {
    if let [only] = chain {
        if let Ok(SketchEntity::Circle {
            center_id, radius, ..
        }) = entity(sketch, *only)
        {
            let center = *positions
                .get(center_id)
                .ok_or(OffsetError::MissingGeometry)?;
            return Ok(ResolvedChain {
                segments: vec![Segment::Circle { center, r: *radius }],
                closed: true,
            });
        }
    }
    let ordered = order_chain(chain, &sketch.entities, positions).map_err(OffsetError::Chain)?;
    let segments = resolve_chain_segments(&ordered.items, &sketch.entities, positions)?;
    Ok(ResolvedChain {
        segments,
        closed: ordered.closed,
    })
}

/// Offset a chain by `distance` to `side`, as new sketch geometry.
///
/// The output is always new entities — an offset never moves what it was
/// offset from. Consecutive segments share their joint point, and a closed
/// run shares last→first, so the result is one connected chain and not a
/// scattering of unwelded pieces.
pub fn offset(
    sketch: &Sketch,
    chain: &[u32],
    distance: f64,
    side: Side,
    ids: &mut IdAllocator,
) -> Result<SketchEdit, SketchOpError> {
    let positions = positions_of(&sketch.entities, &sketch.solved_positions);
    let refused = |e: OffsetError| SketchOpError::OffsetRefused {
        reason: e.tag().to_string(),
    };
    let resolved = resolve_offset_chain(sketch, chain, &positions).map_err(refused)?;
    let signed = distance.abs() * side.sign();
    let result =
        offset_chain_segments(&resolved.segments, resolved.closed, signed).map_err(refused)?;
    Ok(materialize_segments(&result.segments, result.closed, ids))
}

/// Turn offset segments into Points and Lines/Arcs/Circles.
///
/// A port of the JS `createEntitiesFromSegments`, including its arc
/// convention: sketch arcs are CCW start→end (invariant O2), so a
/// CW-traversal segment swaps its endpoints.
fn materialize_segments(segments: &[Segment], closed: bool, ids: &mut IdAllocator) -> SketchEdit {
    let mut edit = SketchEdit::default();
    let new_point = |p: Point2, edit: &mut SketchEdit, ids: &mut IdAllocator| -> u32 {
        let id = ids.next_id();
        edit.added.push(SketchEntity::Point {
            id,
            x: p.x,
            y: p.y,
            construction: false,
        });
        id
    };

    if let [Segment::Circle { center, r }] = segments {
        let center_id = new_point(*center, &mut edit, ids);
        edit.added.push(SketchEntity::Circle {
            id: ids.next_id(),
            center_id,
            radius: *r,
            construction: false,
        });
        return edit;
    }

    let mut joints: Vec<u32> = Vec::with_capacity(segments.len() + 1);
    joints.push(new_point(segments[0].start(), &mut edit, ids));
    for (i, seg) in segments.iter().enumerate() {
        let is_last = i + 1 == segments.len();
        let end_id = if is_last && closed {
            joints[0]
        } else {
            new_point(seg.end(), &mut edit, ids)
        };
        joints.push(end_id);
    }

    for (i, seg) in segments.iter().enumerate() {
        let (s_id, e_id) = (joints[i], joints[i + 1]);
        match seg {
            Segment::Line { .. } => edit.added.push(SketchEntity::Line {
                id: ids.next_id(),
                start_id: s_id,
                end_id: e_id,
                construction: false,
            }),
            Segment::Arc { center, ccw, .. } => {
                let center_id = new_point(*center, &mut edit, ids);
                edit.added.push(SketchEntity::Arc {
                    id: ids.next_id(),
                    center_id,
                    start_id: if *ccw { s_id } else { e_id },
                    end_id: if *ccw { e_id } else { s_id },
                    construction: false,
                });
            }
            Segment::Circle { center, r } => {
                let center_id = new_point(*center, &mut edit, ids);
                edit.added.push(SketchEntity::Circle {
                    id: ids.next_id(),
                    center_id,
                    radius: *r,
                    construction: false,
                });
            }
        }
    }
    edit
}

// ── Fillet ──────────────────────────────────────────────────────────────────

/// The two lines meeting at a corner point, in sketch order.
fn corner_lines(sketch: &Sketch, point: u32) -> Result<(u32, u32), SketchOpError> {
    let lines: Vec<u32> = sketch
        .entities
        .iter()
        .filter(|e| match e {
            SketchEntity::Line {
                start_id, end_id, ..
            } => *start_id == point || *end_id == point,
            _ => false,
        })
        .map(|e| e.id())
        .collect();
    match lines.as_slice() {
        [a, b] => Ok((*a, *b)),
        other => Err(SketchOpError::NotACorner {
            point,
            lines: other.len() as u32,
        }),
    }
}

/// The geometry of a fillet: where its arc goes and where the two lines end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilletGeometry {
    pub center: Point2,
    pub radius: f64,
    /// Tangent point on the first line.
    pub tangent_a: Point2,
    /// Tangent point on the second line.
    pub tangent_b: Point2,
}

/// The default radius a fillet offers at this corner: a third of the shorter
/// leg, which is the JS tool's rule and the number its popup pre-fills.
pub fn fillet_default_radius(sketch: &Sketch, corner: u32) -> Result<f64, SketchOpError> {
    let (la, lb) = corner_lines(sketch, corner)?;
    let positions = positions_of(&sketch.entities, &sketch.solved_positions);
    let c = position(&positions, corner)?;
    let leg = |line: u32| -> Result<f64, SketchOpError> {
        let (s, e) = line_ends(sketch, line)?;
        let other = if s == corner { e } else { s };
        Ok(c.dist(position(&positions, other)?))
    };
    Ok(leg(la)?.min(leg(lb)?) / 3.0)
}

/// Where a fillet of this radius lands, or why it cannot.
///
/// Public so the UI's hover preview is the same computation as the commit.
pub fn fillet_geometry(
    sketch: &Sketch,
    corner: u32,
    radius: f64,
) -> Result<FilletGeometry, SketchOpError> {
    let (la, lb) = corner_lines(sketch, corner)?;
    let positions = positions_of(&sketch.entities, &sketch.solved_positions);
    let c = position(&positions, corner)?;
    let far = |line: u32| -> Result<Point2, SketchOpError> {
        let (s, e) = line_ends(sketch, line)?;
        position(&positions, if s == corner { e } else { s })
    };
    let a_far = far(la)?;
    let b_far = far(lb)?;
    let not_fitting = || SketchOpError::FilletDoesNotFit { corner, radius };

    let dir_a = a_far.minus(c).unit().ok_or_else(not_fitting)?;
    let dir_b = b_far.minus(c).unit().ok_or_else(not_fitting)?;
    let len_a = a_far.dist(c);
    let len_b = b_far.dist(c);

    let d = dir_a.dot(dir_b);
    if d.abs() > 0.9999 {
        return Err(not_fitting()); // parallel legs have no corner to round
    }
    // The interior angle of the corner, signed-correct: `acos(dot)`, NOT
    // `acos(|dot|)`. The JS took the absolute value, which folds every obtuse
    // corner onto its acute supplement — a 135° corner was solved as 45°, so
    // the centre landed at `r / sin(22.5°)` instead of `r / sin(67.5°)` and the
    // arc came out 2.41× the radius the user typed (pinned:
    // `a_fillet_on_an_obtuse_corner_has_the_radius_it_was_asked_for`). Right
    // angles and acute corners are unaffected: there `|dot| == dot`.
    let angle = d.clamp(-1.0, 1.0).acos();
    let sin_half = (angle / 2.0).sin();
    if sin_half < 1e-10 {
        return Err(not_fitting());
    }
    if radius <= 0.0 {
        return Err(not_fitting());
    }
    // The tangent point sits `r / tan(θ/2)` from the corner along each leg;
    // it has to fit on the shorter one.
    let tangent_dist = radius / (angle / 2.0).tan();
    if tangent_dist > len_a.min(len_b) - 0.001 {
        return Err(not_fitting());
    }

    let bisector = angle_bisector(dir_a, dir_b);
    let center = c.plus(bisector.scale(radius / sin_half));
    Ok(FilletGeometry {
        center,
        radius,
        tangent_a: perpendicular_foot(center, c, a_far),
        tangent_b: perpendicular_foot(center, c, b_far),
    })
}

/// Round the corner at a point shared by exactly two lines.
///
/// The two lines are REPOINTED to their tangent points rather than deleted
/// and re-created (the JS did the latter, dropping every constraint on them),
/// the corner point goes if nothing else uses it, and the arc arrives with a
/// `Tangent` constraint to each line so the rounding survives the next solve.
pub fn fillet(
    sketch: &Sketch,
    corner: u32,
    radius: f64,
    ids: &mut IdAllocator,
) -> Result<SketchEdit, SketchOpError> {
    let (la, lb) = corner_lines(sketch, corner)?;
    let geometry = fillet_geometry(sketch, corner, radius)?;
    let (sa, ea) = line_ends(sketch, la)?;
    let (sb, eb) = line_ends(sketch, lb)?;

    let mut edit = SketchEdit::default();
    let tp_a = ids.next_id();
    edit.added.push(SketchEntity::Point {
        id: tp_a,
        x: geometry.tangent_a.x,
        y: geometry.tangent_a.y,
        construction: false,
    });
    let tp_b = ids.next_id();
    edit.added.push(SketchEntity::Point {
        id: tp_b,
        x: geometry.tangent_b.x,
        y: geometry.tangent_b.y,
        construction: false,
    });
    let arc_center = ids.next_id();
    edit.added.push(SketchEntity::Point {
        id: arc_center,
        x: geometry.center.x,
        y: geometry.center.y,
        construction: false,
    });

    // The arc runs CCW from whichever tangent point comes first in that
    // sense, so the entity obeys the CCW start→end convention (invariant O2)
    // without relying on which line the caller happened to pick first.
    let angle_of = |p: Point2| (p.y - geometry.center.y).atan2(p.x - geometry.center.x);
    let sweep_a_to_b = geom::norm_2pi(angle_of(geometry.tangent_b) - angle_of(geometry.tangent_a));
    let (arc_start, arc_end) = if sweep_a_to_b <= std::f64::consts::PI {
        (tp_a, tp_b)
    } else {
        (tp_b, tp_a)
    };
    let arc_id = ids.next_id();
    edit.added.push(SketchEntity::Arc {
        id: arc_id,
        center_id: arc_center,
        start_id: arc_start,
        end_id: arc_end,
        construction: false,
    });

    let repoint = |line: u32, s: u32, e: u32, tp: u32, construction: bool| SketchEntity::Line {
        id: line,
        start_id: if s == corner { tp } else { s },
        end_id: if e == corner { tp } else { e },
        construction,
    };
    edit.changed.push(repoint(
        la,
        sa,
        ea,
        tp_a,
        entity(sketch, la)?.is_construction(),
    ));
    edit.changed.push(repoint(
        lb,
        sb,
        eb,
        tp_b,
        entity(sketch, lb)?.is_construction(),
    ));

    // The corner point is now referenced by nothing — unless a third entity
    // (a circle centred there, a construction line) uses it, in which case it
    // stays and only the two lines moved off it.
    prune_released_points(sketch, &mut edit, &[corner]);

    edit.constraints_added.push(SketchConstraint::Tangent {
        line: la,
        curve: arc_id,
    });
    edit.constraints_added.push(SketchConstraint::Tangent {
        line: lb,
        curve: arc_id,
    });
    Ok(edit)
}

// ── Mirror ──────────────────────────────────────────────────────────────────

/// Mirror entities across a line, adding the images as new geometry.
///
/// New in S1 on both sides (§10.1). The image is independent geometry, not a
/// pattern: a `Symmetric` constraint relating the two would need a solver
/// relation per entity pair and is a separate increment — stated here so the
/// absence is a decision and not an oversight.
pub fn mirror(
    sketch: &Sketch,
    entity_ids: &[u32],
    axis: u32,
    ids: &mut IdAllocator,
) -> Result<SketchEdit, SketchOpError> {
    let (axis_s, axis_e) = line_ends(sketch, axis)?;
    let positions = positions_of(&sketch.entities, &sketch.solved_positions);
    let a = position(&positions, axis_s)?;
    let b = position(&positions, axis_e)?;
    let dir = b
        .minus(a)
        .unit()
        .ok_or_else(|| SketchOpError::MirrorRefused {
            reason: format!("axis line {axis} has zero length"),
        })?;
    if entity_ids.is_empty() {
        return Err(SketchOpError::MirrorRefused {
            reason: "nothing selected to mirror".to_string(),
        });
    }
    if entity_ids.contains(&axis) {
        return Err(SketchOpError::MirrorRefused {
            reason: format!("the axis {axis} cannot also be mirrored"),
        });
    }

    let reflect = |p: Point2| -> Point2 {
        let rel = p.minus(a);
        let along = dir.scale(rel.dot(dir));
        let perp = rel.minus(along);
        a.plus(along).minus(perp)
    };

    // Every point the selected entities stand on, mirrored once each.
    let mut image: HashMap<u32, u32> = HashMap::new();
    let mut edit = SketchEdit::default();
    let mut ordered_points: Vec<u32> = Vec::new();
    for id in entity_ids {
        let e = entity(sketch, *id)?;
        let pts = if matches!(e, SketchEntity::Point { .. }) {
            vec![e.id()]
        } else {
            referenced_points(e)
        };
        for p in pts {
            if let std::collections::hash_map::Entry::Vacant(slot) = image.entry(p) {
                slot.insert(0);
                ordered_points.push(p);
            }
        }
    }
    for p in ordered_points {
        let src = position(&positions, p)?;
        let q = reflect(src);
        let new_id = ids.next_id();
        image.insert(p, new_id);
        edit.added.push(SketchEntity::Point {
            id: new_id,
            x: q.x,
            y: q.y,
            construction: entity(sketch, p)
                .map(|e| e.is_construction())
                .unwrap_or(false),
        });
    }

    for id in entity_ids {
        let e = entity(sketch, *id)?.clone();
        let c = e.is_construction();
        let m = |p: u32| image[&p];
        match &e {
            SketchEntity::Point { .. } => {} // already minted above
            SketchEntity::Line {
                start_id, end_id, ..
            } => edit.added.push(SketchEntity::Line {
                id: ids.next_id(),
                start_id: m(*start_id),
                end_id: m(*end_id),
                construction: c,
            }),
            SketchEntity::Circle {
                center_id, radius, ..
            } => edit.added.push(SketchEntity::Circle {
                id: ids.next_id(),
                center_id: m(*center_id),
                radius: *radius,
                construction: c,
            }),
            // A reflection reverses orientation, so the mirrored arc's CCW
            // start→end run is the SWAP of the original's. Keeping the order
            // would mint the complementary arc — the long way round.
            SketchEntity::Arc {
                center_id,
                start_id,
                end_id,
                ..
            } => edit.added.push(SketchEntity::Arc {
                id: ids.next_id(),
                center_id: m(*center_id),
                start_id: m(*end_id),
                end_id: m(*start_id),
                construction: c,
            }),
            SketchEntity::Spline { point_ids, .. } => edit.added.push(SketchEntity::Spline {
                id: ids.next_id(),
                point_ids: point_ids.iter().map(|p| m(*p)).collect(),
                construction: c,
            }),
            SketchEntity::Gear { .. } | SketchEntity::Sprocket { .. } => {
                return Err(SketchOpError::MirrorRefused {
                    reason: format!(
                        "entity {} is a generator; expand it before mirroring",
                        e.id()
                    ),
                })
            }
        }
    }
    Ok(edit)
}

// ── Project ─────────────────────────────────────────────────────────────────

/// Bring external geometry into the sketch, mapped onto its plane.
///
/// The caller resolves each source to a world position (that needs the model;
/// this crate has no access to it and must not). What is here is the plane
/// mapping and the entity construction, plus the `projected` bindings that
/// let a rebuild re-derive the points from their sources
/// (`specs/projected_sketch_geometry.md`).
pub fn project(
    points: &[ProjectedPoint],
    shape: &ProjectShape,
    plane: &SketchPlaneBasis,
    ids: &mut IdAllocator,
) -> Result<SketchEdit, SketchOpError> {
    if points.is_empty() {
        return Err(SketchOpError::ProjectRefused {
            reason: "nothing to project".to_string(),
        });
    }
    if let ProjectShape::Polyline { .. } = shape {
        if points.len() < 2 {
            return Err(SketchOpError::ProjectRefused {
                reason: "a polyline needs at least two points".to_string(),
            });
        }
    }

    let mut edit = SketchEdit::default();
    let mut minted: Vec<u32> = Vec::with_capacity(points.len());
    for p in points {
        let (u, v) = plane.world_to_local(p.world);
        if !u.is_finite() || !v.is_finite() {
            return Err(SketchOpError::ProjectRefused {
                reason: format!("{:?} does not map onto the sketch plane", p.world),
            });
        }
        let id = ids.next_id();
        minted.push(id);
        edit.added.push(SketchEntity::Point {
            id,
            x: u,
            y: v,
            construction: false,
        });
        if let Some(source) = &p.source {
            edit.projected_added.push(ProjectedEntity {
                point_id: id,
                source: source.clone(),
            });
        }
    }

    if let ProjectShape::Polyline { closed } = shape {
        for w in minted.windows(2) {
            edit.added.push(SketchEntity::Line {
                id: ids.next_id(),
                start_id: w[0],
                end_id: w[1],
                construction: false,
            });
        }
        if *closed && minted.len() > 2 {
            edit.added.push(SketchEntity::Line {
                id: ids.next_id(),
                start_id: *minted.last().expect("non-empty"),
                end_id: minted[0],
                construction: false,
            });
        }
    }
    Ok(edit)
}

// ── Applying an edit, and a batch of ops ────────────────────────────────────

/// Apply an edit to a sketch in place.
///
/// Removals are applied by id and by constraint INDEX, so the edit must be
/// applied to the sketch it was computed against — which is what
/// [`apply_ops`] guarantees by recomputing each op on the running state.
pub fn apply_edit(sketch: &mut Sketch, edit: &SketchEdit) {
    if !edit.constraints_removed.is_empty() {
        let drop: HashSet<u32> = edit.constraints_removed.iter().copied().collect();
        let mut kept = Vec::with_capacity(sketch.constraints.len());
        for (i, c) in sketch.constraints.iter().enumerate() {
            if !drop.contains(&(i as u32)) {
                kept.push(c.clone());
            }
        }
        sketch.constraints = kept;
    }
    if !edit.removed.is_empty() {
        let drop: HashSet<u32> = edit.removed.iter().copied().collect();
        sketch.entities.retain(|e| !drop.contains(&e.id()));
        for id in &edit.removed {
            sketch.solved_positions.remove(id);
        }
        sketch.projected.retain(|p| !drop.contains(&p.point_id));
    }
    for changed in &edit.changed {
        if let Some(slot) = sketch.entities.iter_mut().find(|e| e.id() == changed.id()) {
            *slot = changed.clone();
        }
        if let SketchEntity::Point { id, x, y, .. } = changed {
            sketch.solved_positions.insert(*id, (*x, *y));
        }
    }
    for added in &edit.added {
        sketch.entities.push(added.clone());
        if let SketchEntity::Point { id, x, y, .. } = added {
            sketch.solved_positions.insert(*id, (*x, *y));
        }
    }
    sketch.constraints.extend(edit.constraints_added.clone());
    sketch.projected.extend(edit.projected_added.clone());
}

/// Apply a batch of operations in order.
///
/// Each op is computed against the state the ops before it produced, so a
/// trim followed by a fillet at the new vertex works. Nothing is solved here:
/// the caller solves ONCE at the end, which is what makes the batch a single
/// undo step and a single re-solve (§10.3).
///
/// `next_id_hint` is the caller's own id counter, when it keeps one (the UI
/// does; an agent does not). Pass `0` to let the sketch's own ids set the
/// floor.
pub fn apply_ops(
    sketch: &Sketch,
    ops: &[SketchOp],
    next_id_hint: u32,
) -> Result<AppliedOps, SketchOpError> {
    let mut current = sketch.clone();
    let mut ids = IdAllocator::for_sketch(sketch, next_id_hint);
    let mut total = SketchEdit::default();
    let mut transient: Vec<SketchConstraint> = Vec::new();

    for op in ops {
        let edit = match op {
            SketchOp::AddEntity { entity: e } => {
                let mut e = e.clone();
                if e.id() == 0 {
                    e = e.with_ids_offset(0);
                    e = assign_id(e, ids.next_id());
                } else {
                    // A caller-chosen id still advances the allocator, so a
                    // later minted id cannot collide with it.
                    ids = IdAllocator::for_sketch(&current, ids.peek().max(e.id() + 1));
                }
                SketchEdit {
                    added: vec![e],
                    ..Default::default()
                }
            }
            SketchOp::RemoveEntity { ids: remove } => remove_entities(&current, remove),
            SketchOp::AddConstraint { constraint } => {
                // Every id the constraint names has to exist. The removal
                // cascade (`remove_entities` + `prune_released_points`) goes to
                // real trouble to keep a dangling constraint out of the
                // sketch, and this arm used to be the hole in that floor: a
                // `Vertical { entity: 999 }` was accepted, persisted, and the
                // NEXT solve failed WHOLESALE on it (`solve_sketch` refuses the
                // first constraint that will not compile), so one bad id from
                // an agent took the whole sketch down with no indication of
                // which op did it. Refusing here names the id instead.
                for id in constraint_refs(constraint) {
                    entity(&current, id)?;
                }
                SketchEdit {
                    constraints_added: vec![constraint.clone()],
                    ..Default::default()
                }
            }
            SketchOp::RemoveConstraint { index } => {
                if *index as usize >= current.constraints.len() {
                    return Err(SketchOpError::NoSuchConstraint { index: *index });
                }
                SketchEdit {
                    constraints_removed: vec![*index],
                    ..Default::default()
                }
            }
            SketchOp::SetDimension {
                index,
                value,
                expression,
            } => {
                let c = current
                    .constraints
                    .get(*index as usize)
                    .ok_or(SketchOpError::NoSuchConstraint { index: *index })?;
                if c.dimension_value().is_none() {
                    return Err(SketchOpError::NotADimension { index: *index });
                }
                let mut updated = c.clone();
                if let Some(v) = value {
                    updated.set_dimension_value(*v);
                }
                if let Some(expr) = expression {
                    // `expression_mut` refuses to invent an expression where
                    // there was none, so set it by rebuilding the variant's
                    // field through serde-free assignment below.
                    set_expression(&mut updated, expr.clone());
                }
                // A dimension edit is a constraint replacement: drop the old
                // index and append the new one, so the sketch's constraint
                // array stays a plain Vec with no in-place mutation path.
                current.constraints[*index as usize] = updated;
                SketchEdit::default()
            }
            SketchOp::SetConstruction {
                entity: id,
                construction,
            } => {
                let e = entity(&current, *id)?.clone();
                SketchEdit {
                    changed: vec![with_construction(e, *construction)],
                    ..Default::default()
                }
            }
            SketchOp::MovePoint { id, to } => {
                let e = entity(&current, *id)?;
                if !matches!(e, SketchEntity::Point { .. }) {
                    return Err(SketchOpError::WrongEntityKind {
                        id: *id,
                        expected: "Point".to_string(),
                        found: kind_name(e).to_string(),
                    });
                }
                transient.push(SketchConstraint::Pinned {
                    point: *id,
                    x: to[0],
                    y: to[1],
                });
                SketchEdit {
                    changed: vec![SketchEntity::Point {
                        id: *id,
                        x: to[0],
                        y: to[1],
                        construction: e.is_construction(),
                    }],
                    ..Default::default()
                }
            }
            SketchOp::Trim { entity: id, at } => {
                trim(&current, *id, Point2::new(at[0], at[1]), &mut ids)?
            }
            SketchOp::Extend {
                entity: id,
                end,
                to,
            } => extend(&current, *id, *end, *to, &mut ids)?,
            SketchOp::Offset {
                chain,
                distance,
                side,
            } => offset(&current, chain, *distance, *side, &mut ids)?,
            SketchOp::Fillet { corner, radius } => fillet(&current, *corner, *radius, &mut ids)?,
            SketchOp::Mirror { entities, axis } => mirror(&current, entities, *axis, &mut ids)?,
            SketchOp::Project { points, shape } => {
                let plane = SketchPlaneBasis::from_origin_normal_x(
                    current.plane_origin,
                    current.plane_normal,
                    current.plane_x_axis,
                );
                project(points, shape, &plane, &mut ids)?
            }
        };
        reject_non_finite(&edit)?;
        apply_edit(&mut current, &edit);
        total.extend(edit);
    }

    Ok(AppliedOps {
        next_id: ids.peek(),
        sketch: current,
        edit: total,
        transient_constraints: transient,
    })
}

/// Refuse an edit that would write a point at a non-finite coordinate.
///
/// The ops are double arithmetic on caller-supplied geometry, and at extreme
/// magnitudes ordinary steps overflow: a difference reaching `inf`, a
/// normalization dividing by it, `inf * 0.0` landing on NaN. Nothing
/// downstream can cope — `serde_json` writes `f64::NAN` as `null`, and the
/// next message carrying that entity fails to deserialize, so the sketch is
/// wedged around a point the user cannot see, select or delete. Every batch
/// is checked here so the op refuses by name instead.
pub fn reject_non_finite(edit: &SketchEdit) -> Result<(), SketchOpError> {
    for e in edit.added.iter().chain(edit.changed.iter()) {
        match e {
            SketchEntity::Point { id, x, y, .. } => {
                if !x.is_finite() || !y.is_finite() {
                    return Err(SketchOpError::NonFiniteResult { at: *id });
                }
            }
            SketchEntity::Circle { id, radius, .. } if !radius.is_finite() => {
                return Err(SketchOpError::NonFiniteResult { at: *id });
            }
            _ => {}
        }
    }
    Ok(())
}

/// The entity with a new id (every referenced id unchanged).
fn assign_id(e: SketchEntity, id: u32) -> SketchEntity {
    match e {
        SketchEntity::Point {
            x, y, construction, ..
        } => SketchEntity::Point {
            id,
            x,
            y,
            construction,
        },
        SketchEntity::Line {
            start_id,
            end_id,
            construction,
            ..
        } => SketchEntity::Line {
            id,
            start_id,
            end_id,
            construction,
        },
        SketchEntity::Circle {
            center_id,
            radius,
            construction,
            ..
        } => SketchEntity::Circle {
            id,
            center_id,
            radius,
            construction,
        },
        SketchEntity::Arc {
            center_id,
            start_id,
            end_id,
            construction,
            ..
        } => SketchEntity::Arc {
            id,
            center_id,
            start_id,
            end_id,
            construction,
        },
        SketchEntity::Spline {
            point_ids,
            construction,
            ..
        } => SketchEntity::Spline {
            id,
            point_ids,
            construction,
        },
        SketchEntity::Gear {
            params,
            construction,
            ..
        } => SketchEntity::Gear {
            id,
            params,
            construction,
        },
        SketchEntity::Sprocket {
            params,
            construction,
            ..
        } => SketchEntity::Sprocket {
            id,
            params,
            construction,
        },
    }
}

/// The entity with its construction flag set.
fn with_construction(e: SketchEntity, construction: bool) -> SketchEntity {
    match e {
        SketchEntity::Point { id, x, y, .. } => SketchEntity::Point {
            id,
            x,
            y,
            construction,
        },
        SketchEntity::Line {
            id,
            start_id,
            end_id,
            ..
        } => SketchEntity::Line {
            id,
            start_id,
            end_id,
            construction,
        },
        SketchEntity::Circle {
            id,
            center_id,
            radius,
            ..
        } => SketchEntity::Circle {
            id,
            center_id,
            radius,
            construction,
        },
        SketchEntity::Arc {
            id,
            center_id,
            start_id,
            end_id,
            ..
        } => SketchEntity::Arc {
            id,
            center_id,
            start_id,
            end_id,
            construction,
        },
        SketchEntity::Spline { id, point_ids, .. } => SketchEntity::Spline {
            id,
            point_ids,
            construction,
        },
        SketchEntity::Gear { id, params, .. } => SketchEntity::Gear {
            id,
            params,
            construction,
        },
        SketchEntity::Sprocket { id, params, .. } => SketchEntity::Sprocket {
            id,
            params,
            construction,
        },
    }
}

/// Set a dimension's driving expression, including on a dimension that had
/// none (`SketchConstraint::expression_mut` deliberately refuses that, so the
/// variants are matched here).
fn set_expression(c: &mut SketchConstraint, expr: String) {
    use SketchConstraint as C;
    match c {
        C::Distance { expression, .. }
        | C::PointLineDistance { expression, .. }
        | C::HDistance { expression, .. }
        | C::VDistance { expression, .. }
        | C::Angle { expression, .. }
        | C::Radius { expression, .. }
        | C::Diameter { expression, .. } => *expression = Some(expr),
        _ => {}
    }
}

/// Radius of a curve as the sketch shows it — re-exported for the UI's
/// readouts, which need the same number the ops use.
pub fn curve_radius(sketch: &Sketch, id: u32) -> Option<f64> {
    let positions = positions_of(&sketch.entities, &sketch.solved_positions);
    entity_radius(sketch.entities.iter().find(|e| e.id() == id)?, &positions)
}
