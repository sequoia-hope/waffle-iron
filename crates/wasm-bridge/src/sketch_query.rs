//! Read-only answers about sketch geometry (S1,
//! `specs/agent_mechanical_design.md` §10.1).
//!
//! The sketch tools' hover feedback — the trim highlight, the fillet arc, the
//! offset ghost — used to be computed in the browser by code that was not the
//! code that committed the operation. Two implementations of the same
//! geometry is two implementations that drift, and a preview that disagrees
//! with its own commit is worse than no preview. These answers come from
//! `sketch_solver::ops`, the same functions `ApplySketchOps` runs.

use sketch_solver::ops::chain::order_chain;
use sketch_solver::ops::geom::{positions_of, Point2};
use sketch_solver::ops::offset::{
    offset_chain_segments, segments_to_polyline, signed_distance_to_chain,
};
use sketch_solver::ops::{
    chain::connected_chain, fillet_default_radius, fillet_geometry, resolve_offset_chain,
    trim_preview,
};
use waffle_types::Side;

use crate::messages::{LiveSketch, SketchQuery, SketchQueryResult};

fn refused(reason: impl Into<String>) -> SketchQueryResult {
    SketchQueryResult::Refused {
        reason: reason.into(),
    }
}

/// Answer one query. Every refusal carries the operation vocabulary's own
/// tag, so the UI maps one set of strings and not two.
pub fn answer(live: &LiveSketch, query: &SketchQuery) -> SketchQueryResult {
    let sketch = live.to_sketch();
    let positions = positions_of(&sketch.entities, &sketch.solved_positions);

    match query {
        SketchQuery::Chain { seed, only_seed } => {
            let ids = if *only_seed {
                vec![*seed]
            } else {
                connected_chain(*seed, &sketch.entities, &positions)
            };
            if ids.is_empty() {
                return refused("no-such-entity");
            }
            let closed = order_chain(&ids, &sketch.entities, &positions)
                .ok()
                .map(|o| o.closed);
            SketchQueryResult::Chain { ids, closed }
        }

        SketchQuery::TrimPreview { entity, at } => {
            match trim_preview(&sketch, *entity, Point2::new(at[0], at[1])) {
                Ok(p) => SketchQueryResult::TrimPreview {
                    start: [p.start.x, p.start.y],
                    end: [p.end.x, p.end.y],
                    cuts: p.cuts as u32,
                },
                Err(e) => refused(e.to_string()),
            }
        }

        SketchQuery::FilletPreview { corner, radius } => {
            let default_radius = match fillet_default_radius(&sketch, *corner) {
                Ok(r) => r,
                Err(e) => return refused(e.to_string()),
            };
            match fillet_geometry(&sketch, *corner, radius.unwrap_or(default_radius)) {
                Ok(g) => SketchQueryResult::FilletPreview {
                    center: [g.center.x, g.center.y],
                    radius: g.radius,
                    tangent_a: [g.tangent_a.x, g.tangent_a.y],
                    tangent_b: [g.tangent_b.x, g.tangent_b.y],
                    default_radius,
                },
                Err(e) => refused(e.to_string()),
            }
        }

        SketchQuery::OffsetPreview {
            chain,
            cursor,
            distance,
            side,
        } => {
            let resolved = match resolve_offset_chain(&sketch, chain, &positions) {
                Ok(r) => r,
                Err(e) => return refused(e.tag()),
            };
            let size = chain.len() as u32;
            let signed = match (cursor, distance) {
                // A typed value wins: the popup's number is exact, and the
                // side comes with it.
                (_, Some(d)) => Some(d.abs() * side.unwrap_or(Side::Left).sign()),
                (Some(c), None) => Some(signed_distance_to_chain(
                    &resolved.segments,
                    Point2::new(c[0], c[1]),
                )),
                (None, None) => None,
            };
            match signed {
                // Unarmed hover: the chain's own outline is the ghost.
                None => SketchQueryResult::OffsetPreview {
                    polyline: segments_to_polyline(&resolved.segments, resolved.closed),
                    closed: resolved.closed,
                    signed_distance: None,
                    size,
                },
                Some(d) => match offset_chain_segments(&resolved.segments, resolved.closed, d) {
                    Ok(out) => SketchQueryResult::OffsetPreview {
                        polyline: segments_to_polyline(&out.segments, out.closed),
                        closed: out.closed,
                        signed_distance: Some(d),
                        size,
                    },
                    // A degenerate or collapsing offset still reports the
                    // cursor distance: the tool needs it to arm, and the
                    // caller decides whether to draw nothing.
                    Err(e) => SketchQueryResult::OffsetPreview {
                        polyline: Vec::new(),
                        closed: resolved.closed,
                        signed_distance: Some(d),
                        size,
                    }
                    .or_refused_when(
                        matches!(e, sketch_solver::ops::offset::OffsetError::Chain(_)),
                        e.tag(),
                    ),
                },
            }
        }
    }
}

impl SketchQueryResult {
    /// Turn this answer into a refusal when a condition says the question
    /// itself was unanswerable — used where a partial answer is still useful
    /// (an offset at a collapsing distance) but a structural failure is not.
    fn or_refused_when(self, condition: bool, reason: &str) -> SketchQueryResult {
        if condition {
            refused(reason)
        } else {
            self
        }
    }
}
