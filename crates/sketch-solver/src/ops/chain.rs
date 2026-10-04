//! Chain connectivity over sketch entities: the connected run through shared
//! or coincident endpoints, and its end-to-end ordering.
//!
//! A port of `app/src/lib/sketch/chain.js` (spec
//! `specs/sketch_chain_offset.md`), welding tolerance and error vocabulary
//! intact. Connectivity is geometry, not interaction, so it belongs on this
//! side of the boundary (§10.1): the offset tool, `sketch_edit` (S3) and the
//! select-first chain expansion all need the same answer, and three copies of
//! a union-find is how they stop agreeing.

use std::collections::{HashMap, HashSet};

use crate::ops::geom::Positions;
use crate::types::SketchEntity;

/// Endpoints closer than this weld into one chain node even when their point
/// ids differ. Projected face boundaries mix bound corner points with static
/// polyline points, so id-only connectivity breaks at every straight↔curved
/// seam.
pub const CHAIN_WELD_TOL: f64 = 1e-6;

/// Why a chain could not be ordered. The spellings are the ones the UI's
/// toasts and `window.__waffle.computeChainOffset` already report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainError {
    /// A node where three or more chain members meet.
    Branching,
    /// The members do not form one run.
    Disconnected,
    /// No members at all.
    Empty,
    /// A member that cannot be an endpoint-connected curve (a Circle, a
    /// Point, a generator).
    Unsupported,
}

impl ChainError {
    pub fn tag(self) -> &'static str {
        match self {
            ChainError::Branching => "branching",
            ChainError::Disconnected => "disconnected",
            ChainError::Empty => "empty",
            ChainError::Unsupported => "unsupported",
        }
    }
}

/// The two connector point ids of a chainable entity, or `None` for one that
/// does not participate in chains. An arc's CENTER is not a connector.
pub fn entity_endpoint_ids(entity: &SketchEntity) -> Option<(u32, u32)> {
    match entity {
        SketchEntity::Line {
            start_id, end_id, ..
        }
        | SketchEntity::Arc {
            start_id, end_id, ..
        } => Some((*start_id, *end_id)),
        SketchEntity::Spline { point_ids, .. } if point_ids.len() >= 2 => {
            Some((point_ids[0], point_ids[point_ids.len() - 1]))
        }
        _ => None,
    }
}

/// Union-find over endpoint ids: same id → same node, and distinct ids within
/// [`CHAIN_WELD_TOL`] → same node.
///
/// The spatial hash is the JS one: projected board outlines carry hundreds of
/// polyline points and an O(n²) sweep over them is felt on every hover.
fn build_weld_nodes(entities: &[&SketchEntity], positions: &Positions) -> HashMap<u32, u32> {
    let mut point_ids: Vec<u32> = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();
    for e in entities {
        if let Some((a, b)) = entity_endpoint_ids(e) {
            for pid in [a, b] {
                if seen.insert(pid) {
                    point_ids.push(pid);
                }
            }
        }
    }

    let mut parent: HashMap<u32, u32> = point_ids.iter().map(|p| (*p, *p)).collect();
    fn find(parent: &mut HashMap<u32, u32>, a: u32) -> u32 {
        let mut r = a;
        while parent[&r] != r {
            r = parent[&r];
        }
        let mut c = a;
        while parent[&c] != c {
            let n = parent[&c];
            parent.insert(c, r);
            c = n;
        }
        r
    }

    let cell = CHAIN_WELD_TOL * 4.0;
    let mut grid: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
    for pid in &point_ids {
        let Some(p) = positions.get(pid) else {
            continue;
        };
        let cx = (p.x / cell).floor() as i64;
        let cy = (p.y / cell).floor() as i64;
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(bucket) = grid.get(&(cx + dx, cy + dy)) {
                    for other in bucket.clone() {
                        let Some(q) = positions.get(&other) else {
                            continue;
                        };
                        if (q.x - p.x).abs() <= CHAIN_WELD_TOL
                            && (q.y - p.y).abs() <= CHAIN_WELD_TOL
                        {
                            let (ra, rb) = (find(&mut parent, *pid), find(&mut parent, other));
                            parent.insert(ra, rb);
                        }
                    }
                }
            }
        }
        grid.entry((cx, cy)).or_default().push(*pid);
    }

    point_ids
        .iter()
        .map(|pid| (*pid, find(&mut parent, *pid)))
        .collect()
}

/// Every entity id connected to `start_id` through shared or coincident
/// endpoints, including itself. A non-chainable entity is a singleton chain;
/// an id that is not in the sketch yields nothing.
///
/// The order is deterministic (ascending id) — the JS returned insertion
/// order out of a `Set`, which was stable in practice but not promised.
pub fn connected_chain(
    start_id: u32,
    entities: &[SketchEntity],
    positions: &Positions,
) -> Vec<u32> {
    let Some(start) = entities.iter().find(|e| e.id() == start_id) else {
        return Vec::new();
    };
    if entity_endpoint_ids(start).is_none() {
        return vec![start_id];
    }

    let refs: Vec<&SketchEntity> = entities.iter().collect();
    let nodes = build_weld_nodes(&refs, positions);
    let mut by_node: HashMap<u32, Vec<u32>> = HashMap::new();
    for e in entities {
        if let Some((a, b)) = entity_endpoint_ids(e) {
            for pid in [a, b] {
                if let Some(node) = nodes.get(&pid) {
                    by_node.entry(*node).or_default().push(e.id());
                }
            }
        }
    }

    let by_id: HashMap<u32, &SketchEntity> = entities.iter().map(|e| (e.id(), e)).collect();
    let mut visited: HashSet<u32> = HashSet::from([start_id]);
    let mut queue = vec![start_id];
    while let Some(id) = queue.pop() {
        let Some(e) = by_id.get(&id) else { continue };
        let Some((a, b)) = entity_endpoint_ids(e) else {
            continue;
        };
        for pid in [a, b] {
            let Some(node) = nodes.get(&pid) else {
                continue;
            };
            for other in by_node.get(node).into_iter().flatten() {
                if visited.insert(*other) {
                    queue.push(*other);
                }
            }
        }
    }
    let mut out: Vec<u32> = visited.into_iter().collect();
    out.sort_unstable();
    out
}

/// One member of an ordered chain. `reversed` means traversal runs end→start
/// relative to the entity's own `start_id`/`end_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainItem {
    pub id: u32,
    pub reversed: bool,
}

/// An ordered chain and whether it closes on itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedChain {
    pub items: Vec<ChainItem>,
    pub closed: bool,
}

/// Order a set of chainable entities end-to-end.
pub fn order_chain(
    entity_ids: &[u32],
    entities: &[SketchEntity],
    positions: &Positions,
) -> Result<OrderedChain, ChainError> {
    let by_id: HashMap<u32, &SketchEntity> = entities.iter().map(|e| (e.id(), e)).collect();
    let members: Vec<&SketchEntity> = entity_ids
        .iter()
        .filter_map(|id| by_id.get(id).copied())
        .collect();
    if members.is_empty() {
        return Err(ChainError::Empty);
    }
    if members.iter().any(|e| entity_endpoint_ids(e).is_none()) {
        return Err(ChainError::Unsupported);
    }

    let nodes = build_weld_nodes(&members, positions);
    // node → the (entity id, which endpoint) pairs touching it, in member
    // order so the walk is deterministic.
    let mut by_node: HashMap<u32, Vec<(u32, usize)>> = HashMap::new();
    for e in &members {
        let (a, b) = entity_endpoint_ids(e).expect("checked above");
        for (end_idx, pid) in [a, b].into_iter().enumerate() {
            if let Some(node) = nodes.get(&pid) {
                by_node.entry(*node).or_default().push((e.id(), end_idx));
            }
        }
    }

    // Each node's members in ascending (entity id, endpoint) order, so the
    // walk's choice at a fork-free node is a property of the SET of members
    // and not of the order the caller listed them in. The JS walked in
    // selection order, which rotated a closed chain's output by wherever the
    // user happened to click.
    for touching in by_node.values_mut() {
        touching.sort_unstable();
    }

    // A node with three or more members is a branch; the lowest-numbered node
    // with exactly one is the open chain's start, and a closed chain starts
    // at its lowest node. `min` rather than "first seen" because `HashMap`
    // iteration order is not an order.
    let mut start_node: Option<u32> = None;
    let mut lowest_node: Option<u32> = None;
    for (node, touching) in &by_node {
        if touching.len() > 2 {
            return Err(ChainError::Branching);
        }
        if touching.len() == 1 && start_node.is_none_or(|s| *node < s) {
            start_node = Some(*node);
        }
        if lowest_node.is_none_or(|s| *node < s) {
            lowest_node = Some(*node);
        }
    }
    let closed = start_node.is_none();
    let mut node = start_node.or(lowest_node).ok_or(ChainError::Disconnected)?;

    let mut used: HashSet<u32> = HashSet::new();
    let mut items: Vec<ChainItem> = Vec::new();
    while items.len() < members.len() {
        let next = by_node
            .get(&node)
            .and_then(|touching| touching.iter().find(|(id, _)| !used.contains(id)))
            .copied();
        let Some((id, end_idx)) = next else {
            return Err(ChainError::Disconnected);
        };
        used.insert(id);
        // Departing `node` through endpoint `end_idx`: traversal is forward
        // when we leave through the START endpoint.
        let reversed = end_idx != 0;
        items.push(ChainItem { id, reversed });
        let (a, b) = entity_endpoint_ids(by_id[&id]).expect("checked above");
        let onward = if reversed { a } else { b };
        node = *nodes.get(&onward).ok_or(ChainError::Disconnected)?;
    }
    if used.len() != members.len() {
        return Err(ChainError::Disconnected);
    }
    Ok(OrderedChain { items, closed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::geom::positions_of;

    fn point(id: u32, x: f64, y: f64) -> SketchEntity {
        SketchEntity::Point {
            id,
            x,
            y,
            construction: false,
        }
    }
    fn line(id: u32, start_id: u32, end_id: u32) -> SketchEntity {
        SketchEntity::Line {
            id,
            start_id,
            end_id,
            construction: false,
        }
    }

    /// A closed square: 4 points, 4 lines sharing endpoints.
    fn square() -> Vec<SketchEntity> {
        vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            point(3, 1.0, 1.0),
            point(4, 0.0, 1.0),
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 3, 4),
            line(13, 4, 1),
        ]
    }

    fn pos(entities: &[SketchEntity]) -> Positions {
        positions_of(entities, &HashMap::new())
    }

    #[test]
    fn a_square_is_one_closed_chain() {
        let e = square();
        let p = pos(&e);
        assert_eq!(connected_chain(10, &e, &p), vec![10, 11, 12, 13]);
        let ordered = order_chain(&[10, 11, 12, 13], &e, &p).expect("a square orders");
        assert!(ordered.closed);
        assert_eq!(ordered.items.len(), 4);
    }

    #[test]
    fn an_open_run_starts_at_its_free_end() {
        let e = vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            point(3, 2.0, 0.0),
            line(10, 1, 2),
            line(11, 2, 3),
        ];
        let ordered = order_chain(&[11, 10], &e, &pos(&e)).expect("two lines order");
        assert!(!ordered.closed);
        assert_eq!(ordered.items.len(), 2);
        // Whichever end it starts from, the traversal is continuous: the
        // second item departs where the first arrived.
        let ids: Vec<u32> = ordered.items.iter().map(|i| i.id).collect();
        assert!(ids == vec![10, 11] || ids == vec![11, 10], "{ids:?}");
    }

    #[test]
    fn a_t_junction_is_a_branch() {
        let e = vec![
            point(1, -1.0, 0.0),
            point(2, 0.0, 0.0),
            point(3, 1.0, 0.0),
            point(4, 0.0, -1.0),
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 2, 4),
        ];
        assert_eq!(
            order_chain(&[10, 11, 12], &e, &pos(&e)),
            Err(ChainError::Branching)
        );
    }

    #[test]
    fn a_circle_is_not_chainable() {
        let e = vec![
            point(1, 0.0, 0.0),
            SketchEntity::Circle {
                id: 20,
                center_id: 1,
                radius: 1.0,
                construction: false,
            },
        ];
        assert_eq!(
            order_chain(&[20], &e, &pos(&e)),
            Err(ChainError::Unsupported)
        );
        assert_eq!(connected_chain(20, &e, &pos(&e)), vec![20], "a singleton");
    }

    #[test]
    fn two_separate_runs_are_disconnected() {
        let e = vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            point(3, 5.0, 0.0),
            point(4, 6.0, 0.0),
            line(10, 1, 2),
            line(11, 3, 4),
        ];
        assert_eq!(
            order_chain(&[10, 11], &e, &pos(&e)),
            Err(ChainError::Disconnected)
        );
        assert_eq!(connected_chain(10, &e, &pos(&e)), vec![10]);
    }

    #[test]
    fn distinct_points_within_the_weld_tolerance_connect() {
        // Two lines meeting at coordinates 1e-9 apart under DIFFERENT ids —
        // the projected-boundary case the tolerance exists for.
        let e = vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            point(3, 1.0 + 1e-9, 0.0),
            point(4, 2.0, 0.0),
            line(10, 1, 2),
            line(11, 3, 4),
        ];
        assert_eq!(connected_chain(10, &e, &pos(&e)), vec![10, 11]);
        assert!(order_chain(&[10, 11], &e, &pos(&e)).is_ok());
    }

    #[test]
    fn ordering_is_the_same_whatever_order_the_ids_arrive_in() {
        let e = square();
        let p = pos(&e);
        let a = order_chain(&[10, 11, 12, 13], &e, &p).unwrap();
        let b = order_chain(&[12, 10, 13, 11], &e, &p).unwrap();
        assert_eq!(a, b, "the walk must not depend on selection order");
    }
}
