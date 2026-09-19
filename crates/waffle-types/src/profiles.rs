use std::collections::HashMap;

use crate::sketch::{ArcSegment, CircleProfile, ClosedProfile, SketchEntity};

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
            id,
            center_id,
            radius,
            construction,
        } = entity
        {
            if !construction {
                // Look up center position to populate CircleProfile
                let circle_data = positions.get(center_id).map(|&(cu, cv)| CircleProfile {
                    center_u: cu,
                    center_v: cv,
                    radius: *radius,
                });
                profiles.push(ClosedProfile {
                    entity_ids: vec![*id],
                    is_outer: true,
                    vertex_ids: vec![],
                    circle: circle_data,
                    spline_segments: vec![],
                    arc_segments: vec![],
                });
            }
        }
    }

    // Build directed edge graph for lines, arcs and splines (a spline's edge
    // runs from its first control point to its last — `profiles.js`).
    // Each edge creates two directed half-edges: (start→end) and (end→start)
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
            } if !construction => {
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
            SketchEntity::Spline {
                id,
                point_ids,
                construction,
            } if !construction && point_ids.len() >= 2 => {
                let first = point_ids[0];
                let last = point_ids[point_ids.len() - 1];
                edges.push(DirectedEdge {
                    from: first,
                    to: last,
                    entity_id: *id,
                });
                edges.push(DirectedEdge {
                    from: last,
                    to: first,
                    entity_id: *id,
                });
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
                vertex_ids: face_vertices,
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            });
        }
    }

    // Isolated-ring twin dedup: a closed degree-2 ring yields TWO faces over
    // the SAME entity set (one per traversal direction) — with arcs reduced
    // to chords the walk cannot tell the bounded face from the unbounded
    // one, and the CW twin scrambles downstream profile staging (kernel
    // ProfileRepeatedVertex / NewellMismatch class). Keep the CCW twin.
    // Mirrors app/src/lib/sketch/profiles.js.
    if profiles.len() > 1 {
        let mut by_key: HashMap<Vec<u32>, usize> = HashMap::new();
        let mut deduped: Vec<ClosedProfile> = Vec::with_capacity(profiles.len());
        for p in profiles.drain(..) {
            let mut key = p.entity_ids.clone();
            key.sort_unstable();
            match by_key.get(&key) {
                None => {
                    by_key.insert(key, deduped.len());
                    deduped.push(p);
                }
                Some(&idx) => {
                    if !deduped[idx].is_outer && p.is_outer {
                        deduped[idx] = p;
                    }
                }
            }
        }
        profiles = deduped;
    }

    // Filter: remove the unbounded outer face (largest absolute area, CW winding)
    // The unbounded face is the one wrapping the entire sketch
    if profiles.len() > 1 {
        // Find the largest-area CW (non-outer) face among non-circle profiles.
        // This is the unbounded face that should be discarded.
        let mut max_area = 0.0_f64;
        let mut max_idx = None;

        for (i, profile) in profiles.iter().enumerate() {
            // Skip CCW (outer) profiles — we only want to remove CW (unbounded) faces
            if profile.is_outer {
                continue;
            }

            // Skip standalone circles
            if profile.entity_ids.len() == 1 {
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

        // Remove the largest CW face (the unbounded exterior)
        if let Some(idx) = max_idx {
            profiles.remove(idx);
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
                SketchEntity::Spline { id, point_ids, .. }
                    if *id == *entity_id && point_ids.len() >= 2 =>
                {
                    vertices.push(point_ids[0]);
                    break;
                }
                _ => {}
            }
        }
    }
    compute_signed_area(&vertices, positions)
}

// ── The `FinishSketch` profile payload ───────────────────────────────────

/// Synthetic point ids for arc samples start here, far above any real entity
/// id so they cannot collide with one.
const ARC_SYNTH_ID_BASE: u32 = 900_000;

/// How many segments an arc is sampled into.
const ARC_SAMPLES: usize = 16;

/// What a `FinishSketch` message carries: the profiles, and every point they
/// name — the solved ones plus the synthetic arc samples minted here.
#[derive(Debug, Clone)]
pub struct FinishProfiles {
    pub profiles: Vec<ClosedProfile>,
    pub solved_positions: HashMap<u32, (f64, f64)>,
}

/// Build the profile payload of a `FinishSketch` from a solved sketch.
///
/// Ported from `app/src/lib/sketch/finishProfiles.js`, which the app's own
/// Finish Sketch and the agent's `sketch_create` both used, so a sketch
/// committed by any host carries identical `solved_profiles` and
/// `solved_positions` (`specs/waffle_mcp_server.md` I1).
///
/// [`extract_profiles`] yields entity ids; the kernel wants ordered point-id
/// loops. Lines contribute their start point, arcs are sampled into synthetic
/// points with an `arc_segments` record, splines contribute every control
/// point, and a standalone circle becomes a tagged `circle` profile (a true
/// cylinder, not a polygon).
///
/// `entities` must already carry the solver's radii: a `Diameter`/`Radius`
/// constraint solves a circle's radius into `SolvedSketch::radii`, not into
/// the entity, and the circle profile below reads it from the entity.
pub fn build_finish_profiles(
    extracted: &[ClosedProfile],
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
) -> FinishProfiles {
    build_finish_profiles_with_synth_base(extracted, entities, positions, ARC_SYNTH_ID_BASE)
}

/// [`build_finish_profiles`] with the synthetic arc-sample ids starting at
/// `synth_base` instead of the sketch-wide base. A generator that expands
/// into its own id range (a sprocket) uses this so its samples stay inside
/// that range and cannot collide with the sketch's own arc samples.
pub fn build_finish_profiles_with_synth_base(
    extracted: &[ClosedProfile],
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
    synth_base: u32,
) -> FinishProfiles {
    let mut solved_positions = positions.clone();
    // One counter across ALL profiles: two profiles must never mint the same
    // synthetic id, or their arcs would share points.
    let mut next_synth_id = synth_base;
    let mut profiles = Vec::with_capacity(extracted.len());

    for p in extracted {
        let edges: Vec<&SketchEntity> = p
            .entity_ids
            .iter()
            .filter_map(|id| entities.iter().find(|e| e.id() == *id))
            .collect();

        // A standalone circle is passed through as a tagged circle profile.
        if edges.len() == 1 {
            if let SketchEntity::Circle {
                id,
                center_id,
                radius,
                ..
            } = edges[0]
            {
                profiles.push(match positions.get(center_id) {
                    Some(&(center_u, center_v)) => ClosedProfile {
                        entity_ids: vec![*id],
                        is_outer: p.is_outer,
                        vertex_ids: Vec::new(),
                        circle: Some(CircleProfile {
                            center_u,
                            center_v,
                            radius: *radius,
                        }),
                        spline_segments: Vec::new(),
                        arc_segments: Vec::new(),
                    },
                    None => bare_profile(p),
                });
                continue;
            }
        }

        if edges.is_empty() {
            profiles.push(bare_profile(p));
            continue;
        }

        let Some((first_start, first_end)) = entity_endpoints(edges[0]) else {
            profiles.push(bare_profile(p));
            continue;
        };

        let mut point_ids: Vec<u32> = Vec::new();
        let mut arc_segments: Vec<ArcSegment> = Vec::new();

        // The walk can enter the first entity either way: take its direction
        // from how it connects to the second. Always assuming forward pushed
        // the shared vertex twice (kernel `ProfileRepeatedVertex`) and
        // misdirected the whole chain. A 2-entity loop (bigon) connects at
        // both ends, so it keeps forward.
        let mut first_forward = true;
        if edges.len() > 2 {
            if let Some((s2, e2)) = entity_endpoints(edges[1]) {
                let end_connects = first_end == s2 || first_end == e2;
                let start_connects = first_start == s2 || first_start == e2;
                if !end_connects && start_connects {
                    first_forward = false;
                }
            }
        }

        add_entity_points(
            edges[0],
            first_forward,
            positions,
            &mut point_ids,
            &mut arc_segments,
            &mut solved_positions,
            &mut next_synth_id,
        );
        let mut prev_end = if first_forward {
            first_end
        } else {
            first_start
        };

        for entity in edges.iter().skip(1) {
            let Some((next_start, next_end)) = entity_endpoints(entity) else {
                continue;
            };
            let forward = next_start == prev_end;
            let connected = forward || next_end == prev_end;
            let dir = if connected { forward } else { true };

            add_entity_points(
                entity,
                dir,
                positions,
                &mut point_ids,
                &mut arc_segments,
                &mut solved_positions,
                &mut next_synth_id,
            );
            prev_end = if connected {
                if forward {
                    next_end
                } else {
                    next_start
                }
            } else {
                next_end
            };
        }

        // An arc that CLOSES the profile ends on vertex 0 — the kernel reads
        // index runs cyclically.
        for seg in arc_segments.iter_mut() {
            if seg.end_vertex_index >= point_ids.len() {
                seg.end_vertex_index = 0;
            }
        }

        profiles.push(ClosedProfile {
            entity_ids: p.entity_ids.clone(),
            is_outer: p.is_outer,
            vertex_ids: point_ids,
            circle: None,
            spline_segments: Vec::new(),
            arc_segments,
        });
    }

    FinishProfiles {
        profiles,
        solved_positions,
    }
}

/// A profile with no usable point loop: the ids and the winding, nothing else.
fn bare_profile(p: &ClosedProfile) -> ClosedProfile {
    ClosedProfile {
        entity_ids: p.entity_ids.clone(),
        is_outer: p.is_outer,
        vertex_ids: Vec::new(),
        circle: None,
        spline_segments: Vec::new(),
        arc_segments: Vec::new(),
    }
}

/// The two connection points of an edge entity, start then end.
fn entity_endpoints(entity: &SketchEntity) -> Option<(u32, u32)> {
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

/// The point an entity contributes when the chain enters it: each entity adds
/// exactly one, and the next entity's start supplies the end.
fn entity_start_point(entity: &SketchEntity, forward: bool) -> Option<u32> {
    match entity {
        SketchEntity::Line {
            start_id, end_id, ..
        }
        | SketchEntity::Arc {
            start_id, end_id, ..
        } => Some(if forward { *start_id } else { *end_id }),
        SketchEntity::Spline { point_ids, .. } if point_ids.len() >= 2 => Some(if forward {
            point_ids[0]
        } else {
            point_ids[point_ids.len() - 1]
        }),
        _ => None,
    }
}

/// Add one entity's points to the chain, densely: all of a spline's samples,
/// an arc's sampled sweep, or a line's single start point. The LAST point is
/// never added — the next entity's start is that point.
fn add_entity_points(
    entity: &SketchEntity,
    forward: bool,
    positions: &HashMap<u32, (f64, f64)>,
    point_ids: &mut Vec<u32>,
    arc_segments: &mut Vec<ArcSegment>,
    solved_positions: &mut HashMap<u32, (f64, f64)>,
    next_synth_id: &mut u32,
) {
    match entity {
        // A spline carries its curve as every sample point (involute gear
        // teeth are 12+ each), so all but the last go in.
        SketchEntity::Spline { point_ids: pts, .. } if pts.len() >= 2 => {
            let ordered: Vec<u32> = if forward {
                pts.clone()
            } else {
                pts.iter().rev().copied().collect()
            };
            point_ids.extend(&ordered[..ordered.len() - 1]);
        }
        SketchEntity::Arc {
            center_id,
            start_id,
            end_id,
            ..
        } => {
            let (s_id, e_id) = if forward {
                (*start_id, *end_id)
            } else {
                (*end_id, *start_id)
            };
            let (Some(&(cx, cy)), Some(&(sx, sy)), Some(&(ex, ey))) = (
                positions.get(center_id),
                positions.get(&s_id),
                positions.get(&e_id),
            ) else {
                // No geometry to sample: the start point alone.
                point_ids.push(s_id);
                return;
            };

            let radius = (sx - cx).hypot(sy - cy);
            let start_angle = (sy - cy).atan2(sx - cx);
            let mut end_angle = (ey - cy).atan2(ex - cx);
            // Arcs are CCW start→end. Traversed in REVERSE the same physical
            // arc is walked clockwise, so the angle must DECREASE — forcing
            // CCW here sampled the complement arc, the wrong side of the
            // circle.
            if forward {
                if end_angle <= start_angle {
                    end_angle += std::f64::consts::TAU;
                }
            } else if end_angle >= start_angle {
                end_angle -= std::f64::consts::TAU;
            }

            let arc_start_index = point_ids.len();
            point_ids.push(s_id);
            for s in 1..ARC_SAMPLES {
                let t = s as f64 / ARC_SAMPLES as f64;
                let angle = start_angle + t * (end_angle - start_angle);
                let synth_id = *next_synth_id;
                *next_synth_id += 1;
                solved_positions.insert(
                    synth_id,
                    (cx + angle.cos() * radius, cy + angle.sin() * radius),
                );
                point_ids.push(synth_id);
            }
            // The arc's true end vertex is the NEXT entity's start, which will
            // occupy the index `point_ids.len()` names (wrapped to 0 above
            // when this arc closes the profile). Pointing at the last interior
            // sample instead cut every arc to (N-1)/N of its sweep and left a
            // chord sliver: an extruded 90° fillet was really 84.4°.
            arc_segments.push(ArcSegment {
                start_vertex_index: arc_start_index,
                end_vertex_index: point_ids.len(),
                center_u: cx,
                center_v: cy,
                radius,
            });
        }
        // A line, or anything with no curve of its own.
        other => {
            if let Some(pt) = entity_start_point(other, forward) {
                point_ids.push(pt);
            }
        }
    }
}
