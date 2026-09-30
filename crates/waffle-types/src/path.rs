//! Sketch chains for sweeps: order a set of sketch lines and arcs into ONE
//! oriented chain of [`PipePathSegment`]s in sketch `(u, v)` coordinates.
//!
//! Two extractors share one walk:
//!
//! - [`extract_open_chain`] (`specs/b2_pipe_sweep.md` §5, checkpoint 2) —
//!   the pipe's path: OPEN and tangent-continuous, anything else refused.
//! - [`extract_path_chain`] (`specs/b6_general_sweep.md` §5, §8) — the
//!   general sweep's path: open OR closed, and a non-tangent joint is
//!   reported per joint (`PathChain::g1`) rather than refused, because the
//!   kernel mitres a corner between two straight segments and refuses a
//!   non-tangent bend itself. [`chain3d_from_plane`] then embeds the chain
//!   into world space as the [`Chain3d`] `Kernel::sweep` takes, so a planar
//!   path and a `Sketch3d` path reach the kernel through one type.
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
use crate::sketch3d::{Chain3d, Edge3d, Edge3dKind};

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
            PathError::NoEntities => write!(f, "path: no entities selected"),
            PathError::EntityNotFound { entity_id } => {
                write!(f, "path: entity {entity_id} is not in the sketch")
            }
            PathError::NotAPathEntity { entity_id } => {
                write!(f, "path: entity {entity_id} is not a line or an arc")
            }
            PathError::PointNotSolved { point_id } => {
                write!(f, "path: point {point_id} has no solved position")
            }
            PathError::DegenerateEntity { entity_id } => {
                write!(f, "path: entity {entity_id} is degenerate")
            }
            PathError::Branching { point_id } => {
                write!(f, "path: more than two entities meet at point {point_id}")
            }
            PathError::NotConnected => {
                write!(f, "path: the entities do not form one connected chain")
            }
            PathError::Closed => write!(
                f,
                "path: the chain is closed (a closed pipe loop is not supported yet)"
            ),
            PathError::NotTangent { point_id } => write!(
                f,
                "path: the segments meeting at point {point_id} are not tangent"
            ),
        }
    }
}

impl std::error::Error for PathError {}

/// An ordered, oriented chain that may be open or closed, with its joint
/// tangency reported rather than enforced (the general sweep's path,
/// `specs/b6_general_sweep.md` §5).
#[derive(Debug, Clone, PartialEq)]
pub struct PathChain {
    /// Segments in walk order, head-to-tail (a closed chain's last segment
    /// ends where its first begins).
    pub segments: Vec<PipePathSegment>,
    /// The sketch entity ids in walk order (parallel to `segments`).
    pub entity_ids: Vec<u32>,
    /// Point ids of the joints in walk order (`segments.len() + 1`; a
    /// closed chain repeats its start point last).
    pub point_ids: Vec<u32>,
    /// The chain returns to its start.
    pub closed: bool,
    /// Tangent continuity at each interior joint, the `Chain3d::g1`
    /// convention: `g1[i]` is the joint between `segments[i]` and
    /// `segments[i + 1]`; `segments.len() - 1` entries when open and
    /// `segments.len()` when closed (the last is the wrap-around joint).
    pub g1: Vec<bool>,
    /// Exact arc length (lines + `sweep · radius` per arc).
    pub length: f64,
}

/// Order `entity_ids` (lines and arcs of `entities`, positions from
/// `positions`) into one open chain. The chain starts at whichever free
/// end holds the FIRST listed entity (so the author's first pick is the
/// path start); a single entity keeps its own direction.
pub fn extract_open_chain(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
    entity_ids: &[u32],
) -> Result<OpenChain, PathError> {
    let chain = extract_chain(entities, positions, entity_ids, true)?;
    Ok(OpenChain {
        segments: chain.segments,
        entity_ids: chain.entity_ids,
        point_ids: chain.point_ids,
        length: chain.length,
    })
}

/// Order `entity_ids` into one chain, open or closed, corners allowed. An
/// open chain starts at whichever free end holds the FIRST listed entity;
/// a closed chain starts at the first listed entity's own start point and
/// runs in that entity's direction, so the author's first pick fixes both
/// the path start and its sense. Tangency at each joint is reported in
/// `g1`, never refused: the kernel decides what a corner may do (a mitre
/// between two straight segments; a refusal at a curved joint).
pub fn extract_path_chain(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
    entity_ids: &[u32],
) -> Result<PathChain, PathError> {
    extract_chain(entities, positions, entity_ids, false)
}

/// The shared walk. With `open_g1`, a closed chain is `PathError::Closed`
/// (decided before any geometry is examined) and the first non-tangent
/// joint is `PathError::NotTangent` (decided in walk order, interleaved
/// with the per-segment geometry checks) — exactly `extract_open_chain`'s
/// historical refusal order, so the pipe's behaviour is unchanged.
fn extract_chain(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
    entity_ids: &[u32],
    open_g1: bool,
) -> Result<PathChain, PathError> {
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
    let closed = free_ends.is_empty();
    if closed && open_g1 {
        return Err(PathError::Closed);
    }
    if !closed && free_ends.len() != 2 {
        return Err(PathError::NotConnected);
    }
    // Open: start at the free end touching the first listed piece, else
    // free_ends[0]. Closed: start at the first listed piece's own start and
    // walk it forward, so the pick order fixes the ring's start and sense.
    let first = &pieces[0];
    let start_pid = if closed || free_ends.contains(&first.start) {
        first.start
    } else if free_ends.contains(&first.end) {
        first.end
    } else {
        free_ends[0]
    };

    // Walk. On a closed ring the first step is forced onto piece 0 (the
    // other piece at `start_pid` is the ring's last); `incident` lists
    // pieces in index order, and piece 0 is the first listed, so the plain
    // scan takes it — an open chain's first step is the only piece there.
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
    let mut g1: Vec<bool> = Vec::with_capacity(order.len());
    let mut first_start_tangent: Option<(f64, f64)> = None;
    let mut prev_end_tangent: Option<(f64, f64)> = None;
    let tangent = |t_in: (f64, f64), t_out: (f64, f64)| {
        1.0 - (t_in.0 * t_out.0 + t_in.1 * t_out.1) <= PATH_TANGENT_TOLERANCE
    };
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
            let smooth = tangent(pt, t_start);
            if open_g1 && !smooth {
                return Err(PathError::NotTangent {
                    point_id: point_ids[k],
                });
            }
            g1.push(smooth);
        } else {
            first_start_tangent = Some(t_start);
        }
        prev_end_tangent = Some(t_end);
        segments.push(seg);
        ids.push(p.id);
        length += len;
    }
    if closed {
        // The wrap-around joint: the last segment's end meets the first's
        // start (`Chain3d::g1`'s last entry on a closed chain).
        if let (Some(t_end), Some(t_start)) = (prev_end_tangent, first_start_tangent) {
            g1.push(tangent(t_end, t_start));
        }
    }

    Ok(PathChain {
        segments,
        entity_ids: ids,
        point_ids,
        closed,
        g1,
        length,
    })
}

/// Embed a sketch-plane chain into world space as the [`Chain3d`]
/// `Kernel::sweep` takes: `(u, v) ↦ origin + u·x̂ + v·ŷ` with `ŷ = n̂ × x̂`,
/// the frame convention `Kernel::make_faces_from_profiles` and the pipe use.
/// An arc's traversal sense becomes its `Edge3dKind::Arc` normal: `+n̂` for
/// a CCW segment, `−n̂` for a CW one (the `Edge3d` convention is "CCW about
/// `normal`"). `closed`, `g1` and the entity ids carry over unchanged, so a
/// planar path is indistinguishable from a `Sketch3d` path downstream.
pub fn chain3d_from_plane(
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    plane_x_axis: [f64; 3],
    chain: &PathChain,
) -> Chain3d {
    let n = plane_normal;
    let x = plane_x_axis;
    let y = [
        n[1] * x[2] - n[2] * x[1],
        n[2] * x[0] - n[0] * x[2],
        n[0] * x[1] - n[1] * x[0],
    ];
    let embed = |p: (f64, f64)| -> [f64; 3] {
        [
            plane_origin[0] + p.0 * x[0] + p.1 * y[0],
            plane_origin[1] + p.0 * x[1] + p.1 * y[1],
            plane_origin[2] + p.0 * x[2] + p.1 * y[2],
        ]
    };
    let edges = chain
        .segments
        .iter()
        .zip(&chain.entity_ids)
        .map(|(seg, &entity_id)| match *seg {
            PipePathSegment::Line { a, b } => Edge3d {
                entity_id,
                kind: Edge3dKind::Line,
                a: embed(a),
                b: embed(b),
            },
            PipePathSegment::Arc {
                a,
                b,
                center,
                radius,
                ccw,
            } => Edge3d {
                entity_id,
                kind: Edge3dKind::Arc {
                    center: embed(center),
                    normal: if ccw { n } else { [-n[0], -n[1], -n[2]] },
                    radius,
                },
                a: embed(a),
                b: embed(b),
            },
        })
        .collect();
    Chain3d {
        edges,
        closed: chain.closed,
        g1: chain.g1.clone(),
    }
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

    /// A unit square of four lines, ids 10..13, CCW from the origin.
    fn square() -> Vec<SketchEntity> {
        vec![
            pt(1, 0.0, 0.0),
            pt(2, 1.0, 0.0),
            pt(3, 1.0, 1.0),
            pt(4, 0.0, 1.0),
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 3, 4),
            line(13, 4, 1),
        ]
    }

    #[test]
    fn path_chain_accepts_a_closed_square_with_corners() {
        let ents = square();
        let pos = positions(&ents);
        let chain = extract_path_chain(&ents, &pos, &[10, 11, 12, 13]).unwrap();
        assert!(chain.closed);
        assert_eq!(chain.entity_ids, vec![10, 11, 12, 13]);
        // Starts at the first pick's own start and repeats it last.
        assert_eq!(chain.point_ids, vec![1, 2, 3, 4, 1]);
        // Four corners, none tangent — including the wrap-around joint.
        assert_eq!(chain.g1, vec![false, false, false, false]);
        assert!((chain.length - 4.0).abs() < 1e-12);
        assert_eq!(
            chain.segments[0],
            PipePathSegment::Line {
                a: (0.0, 0.0),
                b: (1.0, 0.0)
            }
        );

        // The first pick fixes the ring's start and sense: picking the
        // top edge first starts there and walks it start→end.
        let from_top = extract_path_chain(&ents, &pos, &[12, 10, 13, 11]).unwrap();
        assert_eq!(from_top.point_ids, vec![3, 4, 1, 2, 3]);
        assert_eq!(from_top.entity_ids, vec![12, 13, 10, 11]);

        // An open L is open with one corner; the pipe extractor refuses it.
        let l = extract_path_chain(&ents, &pos, &[10, 11]).unwrap();
        assert!(!l.closed);
        assert_eq!(l.g1, vec![false]);
        assert_eq!(l.point_ids, vec![1, 2, 3]);
        assert_eq!(
            extract_open_chain(&ents, &pos, &[10, 11]).unwrap_err(),
            PathError::NotTangent { point_id: 2 }
        );
        // Branching and disconnection are still refused.
        assert_eq!(
            extract_path_chain(&ents, &pos, &[10, 12]).unwrap_err(),
            PathError::NotConnected
        );
    }

    #[test]
    fn path_chain_matches_open_chain_on_a_g1_path() {
        let ents = handlebar();
        let pos = positions(&ents);
        let open = extract_open_chain(&ents, &pos, &[12, 10, 11]).unwrap();
        let chain = extract_path_chain(&ents, &pos, &[12, 10, 11]).unwrap();
        assert!(!chain.closed);
        assert_eq!(chain.segments, open.segments);
        assert_eq!(chain.entity_ids, open.entity_ids);
        assert_eq!(chain.point_ids, open.point_ids);
        assert_eq!(chain.length, open.length);
        assert_eq!(chain.g1, vec![true, true]);
    }

    #[test]
    fn chain3d_embeds_the_plane_frame_and_arc_sense() {
        let ents = handlebar();
        let pos = positions(&ents);
        // Walked from the far end, so the bend is CW in the sketch.
        let chain = extract_path_chain(&ents, &pos, &[12, 10, 11]).unwrap();
        // Plane: origin (0,0,5), normal +x, x_axis +y ⇒ y_axis = x × y = +z.
        let c3 = chain3d_from_plane([0.0, 0.0, 5.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], &chain);
        assert_eq!(c3.edges.len(), 3);
        assert!(!c3.closed);
        assert_eq!(c3.g1, vec![true, true]);
        assert_eq!(
            c3.edges.iter().map(|e| e.entity_id).collect::<Vec<_>>(),
            vec![12, 11, 10]
        );
        // (u, v) = (0.3, 1.3) ↦ (0, 0.3, 5 + 1.3).
        let close = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-12);
        assert!(close(c3.edges[0].a, [0.0, 0.3, 6.3]), "{:?}", c3.edges[0].a);
        assert!(close(c3.edges[0].b, [0.0, 0.3, 5.3]));
        assert_eq!(c3.edges[0].kind, Edge3dKind::Line);
        match c3.edges[1].kind {
            Edge3dKind::Arc {
                center,
                normal,
                radius,
            } => {
                assert!(close(center, [0.0, 0.0, 5.3]), "{center:?}");
                // CW in the sketch ⇒ CCW about −n̂.
                assert!(close(normal, [-1.0, 0.0, 0.0]), "{normal:?}");
                assert!((radius - 0.3).abs() < 1e-12);
            }
            other => panic!("{other:?}"),
        }
        // The embedded edges chain head-to-tail and keep their tangents:
        // the sketch tangent at the arc's start ((0.3,0.3) walked CW) is
        // −v, which embeds to −z.
        assert!(close(c3.edges[0].b, c3.edges[1].a));
        assert!(close(c3.edges[1].b, c3.edges[2].a));
        assert!(close(c3.edges[1].start_tangent(), [0.0, 0.0, -1.0]));
        assert!((c3.length() - chain.length).abs() < 1e-12);

        // The forward walk's CCW bend keeps +n̂.
        let fwd = extract_path_chain(&ents, &pos, &[10, 11, 12]).unwrap();
        let f3 = chain3d_from_plane([0.0, 0.0, 5.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], &fwd);
        assert!(
            matches!(f3.edges[1].kind, Edge3dKind::Arc { normal, .. } if close(normal, [1.0, 0.0, 0.0]))
        );

        // A closed ring embeds closed with its wrap-around joint.
        let sq = square();
        let spos = positions(&sq);
        let ring = extract_path_chain(&sq, &spos, &[10, 11, 12, 13]).unwrap();
        let r3 = chain3d_from_plane([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], &ring);
        assert!(r3.closed);
        assert_eq!(r3.g1.len(), 4);
        assert!(close(r3.edges[3].b, r3.edges[0].a));
    }
}
