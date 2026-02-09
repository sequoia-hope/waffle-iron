use std::collections::HashMap;

use crate::types::{ClosedProfile, SketchEntity};

/// Extract closed profiles from solved sketch geometry.
///
/// Uses a planar graph minimal face detection algorithm:
/// 1. Build adjacency graph from non-construction line/arc entities
/// 2. Circles are standalone closed profiles
/// 3. For line/arc networks, use angle-sorted adjacency + half-edge traversal
///    to find minimal faces
/// 4. Classify winding direction (CCW = outer, CW = hole)
/// 5. Discard the unbounded outer face
pub fn extract_profiles(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
) -> Vec<ClosedProfile> {
    let mut profiles = Vec::new();

    // Collect standalone circles (each is a closed profile by itself)
    for entity in entities {
        if let SketchEntity::Circle {
            id, construction, ..
        } = entity
        {
            if !construction {
                profiles.push(ClosedProfile {
                    entity_ids: vec![*id],
                    is_outer: true,
                });
            }
        }
    }

    // Build directed edge graph for lines and arcs
    // Each line/arc creates two directed half-edges: (start→end) and (end→start)
    let mut edges: Vec<DirectedEdge> = Vec::new();
    for entity in entities {
        match entity {
            SketchEntity::Line {
                id,
                start_id,
                end_id,
                construction,
            } => {
                if !construction {
                    edges.push(DirectedEdge {
                        from: *start_id,
                        to: *end_id,
                        entity_id: *id,
                    });
                    edges.push(DirectedEdge {
                        from: *end_id,
                        to: *start_id,
                        entity_id: *id,
                    });
                }
            }
            SketchEntity::Arc {
                id,
                start_id,
                end_id,
                construction,
                ..
            } => {
                if !construction {
                    edges.push(DirectedEdge {
                        from: *start_id,
                        to: *end_id,
                        entity_id: *id,
                    });
                    edges.push(DirectedEdge {
                        from: *end_id,
                        to: *start_id,
                        entity_id: *id,
                    });
                }
            }
            _ => {}
        }
    }

    if edges.is_empty() {
        return profiles;
    }

    // Build adjacency: for each vertex, list outgoing edges sorted by angle
    let mut adjacency: HashMap<u32, Vec<DirectedEdge>> = HashMap::new();
    for edge in &edges {
        adjacency.entry(edge.from).or_default().push(edge.clone());
    }

    // Sort each vertex's outgoing edges by departure angle
    for (vertex_id, out_edges) in adjacency.iter_mut() {
        let from_pos = match positions.get(vertex_id) {
            Some(p) => *p,
            None => continue,
        };
        out_edges.sort_by(|a, b| {
            let angle_a = departure_angle(from_pos, positions, a);
            let angle_b = departure_angle(from_pos, positions, b);
            angle_a
                .partial_cmp(&angle_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Track which directed edges have been used
    let mut used: HashMap<(u32, u32, u32), bool> = HashMap::new();
    for edge in &edges {
        used.insert((edge.from, edge.to, edge.entity_id), false);
    }

    // Walk minimal faces using the "next half-edge" rule
    for edge in &edges {
        let key = (edge.from, edge.to, edge.entity_id);
        if used[&key] {
            continue;
        }

        let mut face_edges: Vec<u32> = Vec::new();
        let mut face_vertices: Vec<u32> = Vec::new();
        let mut current = edge.clone();

        loop {
            let key = (current.from, current.to, current.entity_id);
            if let Some(u) = used.get_mut(&key) {
                if *u {
                    // Already used — we've completed the face or hit a dead end
                    break;
                }
                *u = true;
            } else {
                break;
            }

            // Record entity (deduplicate consecutive same-entity)
            if face_edges.is_empty() || *face_edges.last().unwrap() != current.entity_id {
                face_edges.push(current.entity_id);
            }
            face_vertices.push(current.from);

            // Find next edge: at vertex current.to, find the edge that is
            // the "next left turn" after the reverse direction (current.to → current.from)
            let next = next_half_edge(&adjacency, &current, positions);
            match next {
                Some(n) => {
                    if n.from == edge.from && n.to == edge.to && n.entity_id == edge.entity_id {
                        // Completed the face
                        break;
                    }
                    current = n;
                }
                None => break,
            }
        }

        if face_edges.len() >= 2 {
            // Compute winding using shoelace formula on face vertices
            let winding = compute_signed_area(&face_vertices, positions);
            profiles.push(ClosedProfile {
                entity_ids: face_edges,
                is_outer: winding > 0.0,
            });
        }
    }

    // Filter: remove the unbounded outer face (largest absolute area, typically CW)
    // The unbounded face is the one wrapping the entire sketch
    if profiles.len() > 1 {
        // Find which non-circle profile has the largest absolute area
        let mut max_area = 0.0_f64;
        let mut max_idx = None;

        for (i, profile) in profiles.iter().enumerate() {
            // Skip standalone circles
            if profile.entity_ids.len() == 1 {
                // Check if it's a circle
                let is_circle = entities.iter().any(|e| {
                    matches!(e, SketchEntity::Circle { id, .. } if *id == profile.entity_ids[0])
                });
                if is_circle {
                    continue;
                }
            }

            let area = compute_profile_area(profile, entities, positions).abs();
            if area > max_area {
                max_area = area;
                max_idx = Some(i);
            }
        }

        // The unbounded face wraps everything and has negative winding (CW).
        // Only remove it if it's a CW face (hole), which indicates it's the unbounded face.
        if let Some(idx) = max_idx {
            if !profiles[idx].is_outer {
                profiles.remove(idx);
            }
        }
    }

    profiles
}

#[derive(Debug, Clone)]
struct DirectedEdge {
    from: u32,
    to: u32,
    entity_id: u32,
}

/// Compute the departure angle of a directed edge from a vertex.
fn departure_angle(
    from_pos: (f64, f64),
    positions: &HashMap<u32, (f64, f64)>,
    edge: &DirectedEdge,
) -> f64 {
    let to_pos = positions.get(&edge.to).copied().unwrap_or((0.0, 0.0));
    let dx = to_pos.0 - from_pos.0;
    let dy = to_pos.1 - from_pos.1;
    dy.atan2(dx)
}

/// Find the next half-edge in a minimal face traversal.
/// At vertex `current.to`, we look for the outgoing edge that comes
/// immediately after the reverse of `current` (i.e., after the direction
/// from current.to back to current.from) when sorted counter-clockwise.
fn next_half_edge(
    adjacency: &HashMap<u32, Vec<DirectedEdge>>,
    current: &DirectedEdge,
    positions: &HashMap<u32, (f64, f64)>,
) -> Option<DirectedEdge> {
    let out_edges = adjacency.get(&current.to)?;
    if out_edges.is_empty() {
        return None;
    }

    let vertex_pos = positions.get(&current.to)?;

    // Angle of the incoming direction (from current.from to current.to),
    // reversed to get the "arrival" direction at current.to pointing back
    let from_pos = positions.get(&current.from)?;
    let incoming_angle = (from_pos.1 - vertex_pos.1).atan2(from_pos.0 - vertex_pos.0);

    // Find the outgoing edge with the smallest CCW angle after the incoming direction.
    // This implements the "left-turn" rule for minimal face detection.
    let mut best: Option<&DirectedEdge> = None;
    let mut best_delta = f64::MAX;

    for edge in out_edges {
        // Skip the reverse of the current edge (same entity, going back)
        if edge.to == current.from && edge.entity_id == current.entity_id {
            continue;
        }
        let edge_angle = departure_angle(*vertex_pos, positions, edge);
        // Delta: how far CCW we need to rotate from incoming to this edge
        // We want the smallest positive rotation (most clockwise turn = tightest right turn)
        let mut delta = edge_angle - incoming_angle;
        // Normalize to (0, 2π]
        while delta <= 0.0 {
            delta += std::f64::consts::TAU;
        }
        while delta > std::f64::consts::TAU {
            delta -= std::f64::consts::TAU;
        }

        if delta < best_delta {
            best_delta = delta;
            best = Some(edge);
        }
    }

    best.cloned()
}

/// Compute signed area of a polygon from vertex IDs using the shoelace formula.
/// Positive = CCW (outer), Negative = CW (hole).
fn compute_signed_area(vertices: &[u32], positions: &HashMap<u32, (f64, f64)>) -> f64 {
    if vertices.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    let n = vertices.len();
    for i in 0..n {
        let j = (i + 1) % n;
        let (x1, y1) = positions.get(&vertices[i]).copied().unwrap_or((0.0, 0.0));
        let (x2, y2) = positions.get(&vertices[j]).copied().unwrap_or((0.0, 0.0));
        area += x1 * y2 - x2 * y1;
    }
    area / 2.0
}

/// Compute the signed area of a profile for unbounded face detection.
fn compute_profile_area(
    profile: &ClosedProfile,
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
) -> f64 {
    // Collect the ordered vertex IDs by walking the profile's entity chain
    let mut vertices = Vec::new();
    for entity_id in &profile.entity_ids {
        for entity in entities {
            match entity {
                SketchEntity::Line { id, start_id, .. } if *id == *entity_id => {
                    vertices.push(*start_id);
                    break;
                }
                SketchEntity::Arc { id, start_id, .. } if *id == *entity_id => {
                    vertices.push(*start_id);
                    break;
                }
                _ => {}
            }
        }
    }
    compute_signed_area(&vertices, positions)
}
