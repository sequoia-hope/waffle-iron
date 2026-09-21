//! Open sketch chains for sweeps (`specs/b2_pipe_sweep.md` §5, checkpoint
//! 2): order a set of sketch lines and arcs into ONE open, oriented,
//! tangent-continuous chain of [`PipePathSegment`]s in sketch `(u, v)`
//! coordinates.
//!
//! `extract_profiles` only yields CLOSED loops; a pipe path (a handlebar, a
//! brake hose) is an open chain, so this is its own extractor. Connectivity
//! is by shared point ids, exactly as the profile graph is built; arcs are
//! CCW from `start_id` to `end_id` about the sketch normal (the entity
//! convention everywhere in the sketch layer — `offset.js`, `profiles.rs`),
//! so an arc walked end→start is a CW segment. Construction entities are
//! allowed: a sweep path is typically drawn as construction geometry.
//!
//! Every failure is a typed [`PathError`] naming the entity or point.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::kernel::PipePathSegment;
use crate::sketch::SketchEntity;

/// Angular agreement (`1 − t_in · t_out`) two unit tangents meeting at a
/// joint must satisfy for the chain to be G1 — the kernel constructor's own
/// `PIPE_TANGENT_TOLERANCE`; checked here too so the refusal can name the
/// sketch point.
pub const PATH_TANGENT_TOLERANCE: f64 = 1e-9;

/// An ordered, oriented open chain.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenChain {
    /// Segments in walk order, head-to-tail.
    pub segments: Vec<PipePathSegment>,
    /// The sketch entity ids in walk order (parallel to `segments`).
    pub entity_ids: Vec<u32>,
    /// Point ids of the joints in walk order (`segments.len() + 1`).
    pub point_ids: Vec<u32>,
    /// Exact arc length (lines + `sweep · radius` per arc).
    pub length: f64,
}

/// Why a set of entities is not an open pipe chain.
#[derive(Debug, Clone, PartialEq)]
pub enum PathError {
    /// No entity ids were given.
    NoEntities,
    /// An id names no entity in the sketch.
    EntityNotFound { entity_id: u32 },
    /// The entity is not a line or an arc (a circle, spline or point cannot
    /// be a path segment).
    NotAPathEntity { entity_id: u32 },
    /// An endpoint point id has no solved position.
    PointNotSolved { point_id: u32 },
    /// A line of zero length or an arc whose endpoints do not lie on its
    /// circle / with a zero radius.
    DegenerateEntity { entity_id: u32 },
    /// More than two entities meet at a point: the path branches.
    Branching { point_id: u32 },
    /// The entities form more than one chain (or a chain plus a loop).
    NotConnected,
    /// The chain closes on itself: a closed pipe loop is a later slice.
    Closed,
    /// The two tangents at an interior joint disagree: the chain is not G1
    /// (a mitred joint is a later slice).
    NotTangent { point_id: u32 },
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::NoEntities => write!(f, "pipe path: no entities selected"),
            PathError::EntityNotFound { entity_id } => {
                write!(f, "pipe path: entity {entity_id} is not in the sketch")
            }
            PathError::NotAPathEntity { entity_id } => {
                write!(f, "pipe path: entity {entity_id} is not a line or an arc")
            }
            PathError::PointNotSolved { point_id } => {
                write!(f, "pipe path: point {point_id} has no solved position")
            }
            PathError::DegenerateEntity { entity_id } => {
                write!(f, "pipe path: entity {entity_id} is degenerate")
            }
            PathError::Branching { point_id } => write!(
                f,
                "pipe path: more than two entities meet at point {point_id}"
            ),
            PathError::NotConnected => {
                write!(f, "pipe path: the entities do not form one connected chain")
            }
            PathError::Closed => write!(
                f,
                "pipe path: the chain is closed (a closed pipe loop is not supported yet)"
            ),
            PathError::NotTangent { point_id } => write!(
                f,
                "pipe path: the segments meeting at point {point_id} are not tangent"
            ),
        }
    }
}

impl std::error::Error for PathError {}

/// Order `entity_ids` (lines and arcs of `entities`, positions from
/// `positions`) into one open chain. The chain starts at whichever free
/// end holds the FIRST listed entity (so the author's first pick is the
/// path start); a single entity keeps its own direction.
pub fn extract_open_chain(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
    entity_ids: &[u32],
) -> Result<OpenChain, PathError> {
    if entity_ids.is_empty() {
        return Err(PathError::NoEntities);
    }
    // Per entity: (start point, end point, arc centre if any).
    struct Piece {
        id: u32,
        start: u32,
        end: u32,
        center: Option<u32>,
    }
    let mut pieces: Vec<Piece> = Vec::with_capacity(entity_ids.len());
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    for &eid in entity_ids {
        if !seen.insert(eid) {
            continue; // a repeated pick is the same entity
        }
        let ent = entities
            .iter()
            .find(|e| e.id() == eid)
            .ok_or(PathError::EntityNotFound { entity_id: eid })?;
        let piece = match ent {
            SketchEntity::Line {
                start_id, end_id, ..
            } => Piece {
                id: eid,
                start: *start_id,
                end: *end_id,
                center: None,
            },
            SketchEntity::Arc {
                center_id,
                start_id,
                end_id,
                ..
            } => Piece {
                id: eid,
                start: *start_id,
                end: *end_id,
                center: Some(*center_id),
            },
            _ => return Err(PathError::NotAPathEntity { entity_id: eid }),
        };
        for pid in [Some(piece.start), Some(piece.end), piece.center]
            .into_iter()
            .flatten()
        {
            if !positions.contains_key(&pid) {
                return Err(PathError::PointNotSolved { point_id: pid });
            }
        }
        pieces.push(piece);
    }

    // Point → incident piece indices.
    let mut incident: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, p) in pieces.iter().enumerate() {
        if p.start == p.end {
            return Err(PathError::DegenerateEntity { entity_id: p.id });
        }
        incident.entry(p.start).or_default().push(i);
        incident.entry(p.end).or_default().push(i);
    }
    for (&pid, inc) in &incident {
        if inc.len() > 2 {
            return Err(PathError::Branching { point_id: pid });
        }
    }
    let free_ends: Vec<u32> = incident
        .iter()
        .filter(|(_, inc)| inc.len() == 1)
        .map(|(&pid, _)| pid)
        .collect();
    if free_ends.is_empty() {
        return Err(PathError::Closed);
    }
    if free_ends.len() != 2 {
        return Err(PathError::NotConnected);
    }
    // Start at the free end touching the first listed piece, else free_ends[0].
    let first = &pieces[0];
    let start_pid = if free_ends.contains(&first.start) {
        first.start
    } else if free_ends.contains(&first.end) {
        first.end
    } else {
        free_ends[0]
    };

    // Walk.
    let mut used = vec![false; pieces.len()];
    let mut order: Vec<(usize, bool)> = Vec::with_capacity(pieces.len()); // (piece, forward)
    let mut point_ids = vec![start_pid];
    let mut cur = start_pid;
    while let Some(next) = incident[&cur].iter().copied().find(|&i| !used[i]) {
        used[next] = true;
        let p = &pieces[next];
        let forward = p.start == cur;
        cur = if forward { p.end } else { p.start };
        order.push((next, forward));
        point_ids.push(cur);
    }
    if order.len() != pieces.len() {
        return Err(PathError::NotConnected);
    }

    // Geometry + tangency.
    let pos = |pid: u32| positions[&pid];
    let mut segments = Vec::with_capacity(order.len());
    let mut ids = Vec::with_capacity(order.len());
    let mut length = 0.0f64;
    let mut prev_end_tangent: Option<(f64, f64)> = None;
    for (k, &(pi, forward)) in order.iter().enumerate() {
        let p = &pieces[pi];
        let (a_id, b_id) = if forward {
            (p.start, p.end)
        } else {
            (p.end, p.start)
        };
        let (a, b) = (pos(a_id), pos(b_id));
        let (seg, t_start, t_end, len) = match p.center {
            None => {
                let d = (b.0 - a.0, b.1 - a.1);
                let l = (d.0 * d.0 + d.1 * d.1).sqrt();
                if !(l.is_finite() && l > 0.0) {
                    return Err(PathError::DegenerateEntity { entity_id: p.id });
                }
                let t = (d.0 / l, d.1 / l);
                (PipePathSegment::Line { a, b }, t, t, l)
            }
            Some(cid) => {
                let c = pos(cid);
                let ra = (a.0 - c.0, a.1 - c.1);
                let rb = (b.0 - c.0, b.1 - c.1);
                let da = (ra.0 * ra.0 + ra.1 * ra.1).sqrt();
                let db = (rb.0 * rb.0 + rb.1 * rb.1).sqrt();
                if !(da.is_finite() && da > 0.0 && (da - db).abs() <= 1e-9 * da) {
                    return Err(PathError::DegenerateEntity { entity_id: p.id });
                }
                let radius = da;
                // Entity sense is CCW start→end; walking it backwards is CW.
                let ccw = forward;
                let mut sweep = (ra.0 * rb.1 - ra.1 * rb.0).atan2(ra.0 * rb.0 + ra.1 * rb.1);
                if sweep <= 0.0 {
                    sweep += std::f64::consts::TAU;
                }
                if !ccw {
                    sweep = std::f64::consts::TAU - sweep;
                }
                let (sa, ea) = ((ra.0 / da, ra.1 / da), (rb.0 / db, rb.1 / db));
                let (ts, te) = if ccw {
                    ((-sa.1, sa.0), (-ea.1, ea.0))
                } else {
                    ((sa.1, -sa.0), (ea.1, -ea.0))
                };
                (
                    PipePathSegment::Arc {
                        a,
                        b,
                        center: c,
                        radius,
                        ccw,
                    },
                    ts,
                    te,
                    sweep * radius,
                )
            }
        };
        if let Some(pt) = prev_end_tangent {
            if 1.0 - (pt.0 * t_start.0 + pt.1 * t_start.1) > PATH_TANGENT_TOLERANCE {
                return Err(PathError::NotTangent {
                    point_id: point_ids[k],
                });
            }
        }
        prev_end_tangent = Some(t_end);
        segments.push(seg);
        ids.push(p.id);
        length += len;
    }

    Ok(OpenChain {
        segments,
        entity_ids: ids,
        point_ids,
        length,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(id: u32, x: f64, y: f64) -> SketchEntity {
        SketchEntity::Point {
            id,
            x,
            y,
            construction: false,
        }
    }
    fn line(id: u32, s: u32, e: u32) -> SketchEntity {
        SketchEntity::Line {
            id,
            start_id: s,
            end_id: e,
            construction: true,
        }
    }
    fn arc(id: u32, c: u32, s: u32, e: u32) -> SketchEntity {
        SketchEntity::Arc {
            id,
            center_id: c,
            start_id: s,
            end_id: e,
            construction: false,
        }
    }
    fn positions(ents: &[SketchEntity]) -> HashMap<u32, (f64, f64)> {
        ents.iter()
            .filter_map(|e| match e {
                SketchEntity::Point { id, x, y, .. } => Some((*id, (*x, *y))),
                _ => None,
            })
            .collect()
    }

    /// Handlebar: line (−1,0)→(0,0), CCW quarter about (0,0.3) to (0.3,0.3),
    /// line to (0.3,1.3).
    fn handlebar() -> Vec<SketchEntity> {
        vec![
            pt(1, -1.0, 0.0),
            pt(2, 0.0, 0.0),
            pt(3, 0.0, 0.3),
            pt(4, 0.3, 0.3),
            pt(5, 0.3, 1.3),
            line(10, 1, 2),
            arc(11, 3, 2, 4),
            line(12, 4, 5),
        ]
    }

    #[test]
    fn handlebar_orders_orients_and_measures() {
        let ents = handlebar();
        let pos = positions(&ents);
        // Picked out of order, and the last line reversed in the pick list.
        let chain = extract_open_chain(&ents, &pos, &[12, 10, 11]).unwrap();
        // Starts at the free end holding the FIRST pick (line 12 at (0.3,1.3)).
        assert_eq!(chain.entity_ids, vec![12, 11, 10]);
        assert_eq!(chain.point_ids, vec![5, 4, 2, 1]);
        assert_eq!(chain.segments.len(), 3);
        match chain.segments[1] {
            PipePathSegment::Arc { ccw, a, b, .. } => {
                assert!(!ccw, "walked end→start: CW");
                assert_eq!(a, (0.3, 0.3));
                assert_eq!(b, (0.0, 0.0));
            }
            other => panic!("{other:?}"),
        }
        let expect = 2.0 + 0.3 * std::f64::consts::FRAC_PI_2;
        assert!((chain.length - expect).abs() < 1e-12);

        // The natural order walks the other way.
        let fwd = extract_open_chain(&ents, &pos, &[10, 11, 12]).unwrap();
        assert_eq!(fwd.entity_ids, vec![10, 11, 12]);
        assert!(matches!(
            fwd.segments[1],
            PipePathSegment::Arc { ccw: true, .. }
        ));
    }

    #[test]
    fn refusals_are_typed() {
        let ents = handlebar();
        let pos = positions(&ents);
        assert_eq!(
            extract_open_chain(&ents, &pos, &[]).unwrap_err(),
            PathError::NoEntities
        );
        assert_eq!(
            extract_open_chain(&ents, &pos, &[99]).unwrap_err(),
            PathError::EntityNotFound { entity_id: 99 }
        );
        assert_eq!(
            extract_open_chain(&ents, &pos, &[1]).unwrap_err(),
            PathError::NotAPathEntity { entity_id: 1 }
        );
        // Two disjoint lines.
        assert_eq!(
            extract_open_chain(&ents, &pos, &[10, 12]).unwrap_err(),
            PathError::NotConnected
        );
        // A corner: line then a perpendicular line.
        let mut corner = handlebar();
        corner.push(pt(6, 0.0, 1.0));
        corner.push(line(13, 2, 6));
        let cpos = positions(&corner);
        assert_eq!(
            extract_open_chain(&corner, &cpos, &[10, 13]).unwrap_err(),
            PathError::NotTangent { point_id: 2 }
        );
        // Branching at point 2.
        assert_eq!(
            extract_open_chain(&corner, &cpos, &[10, 11, 13]).unwrap_err(),
            PathError::Branching { point_id: 2 }
        );
        // A closed loop of four tangent pieces: two semicircles.
        let stadium = vec![
            pt(1, 0.0, 0.0),
            pt(2, 1.0, 0.0),
            pt(3, 1.0, 0.5),
            pt(4, 1.0, 1.0),
            pt(5, 0.0, 1.0),
            pt(6, 0.0, 0.5),
            line(10, 1, 2),
            arc(11, 3, 2, 4),
            line(12, 4, 5),
            arc(13, 6, 5, 1),
        ];
        let spos = positions(&stadium);
        assert_eq!(
            extract_open_chain(&stadium, &spos, &[10, 11, 12, 13]).unwrap_err(),
            PathError::Closed
        );
    }
}
