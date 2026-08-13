// Solver robustness against the user-reported document (2026-07-05,
// sketch.waffle): exact fixture, drag point 15 (upper-left inner corner,
// shared by the two Equal edges), NO pin (stripped by FinishSketch),
// meter-scale coordinates. The app-level explosion turned out to be the
// drag<->auto-fit camera feedback loop (sketch-drag-autofit-feedback.spec.js);
// these hunts pin down that the SOLVER stays bounded on this fixture, drag point 15 (upper-left inner
// corner, shared by the two Equal edges), NO pin (stripped by FinishSketch),
// meter-scale coordinates. Sweep many deterministic pseudo-random mouse paths
// to hunt the intermittent explosion.
use sketch_solver::*;
use std::collections::HashMap;
use uuid::Uuid;

fn dummy_geom_ref() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::nil(),
        },
        selector: Selector::Role {
            role: Role::ProfileFace,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    }
}
fn pt(id: u32, x: f64, y: f64, c: bool) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: c,
    }
}
fn line(id: u32, s: u32, e: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id: s,
        end_id: e,
        construction: false,
    }
}

fn fixture() -> (Vec<SketchEntity>, Vec<SketchConstraint>) {
    let entities = vec![
        pt(1, 0.0, 0.0, false),
        pt(2, -0.0132331, -0.0127945, false),
        pt(3, 0.0132331, -0.0127945, false),
        pt(4, 0.0132331, 0.0127945, false),
        pt(5, -0.0132331, 0.0127945, false),
        line(6, 2, 3),
        line(7, 3, 4),
        line(8, 4, 5),
        line(9, 5, 2),
        pt(10, 0.0, 0.0127945, true),
        pt(11, -0.0132331, 0.0, true),
        pt(12, -0.00850528, -0.00928513, false),
        pt(13, 0.00850528, -0.00928513, false),
        pt(14, 0.00850528, 0.00928513, false),
        pt(15, -0.00850528, 0.00928513, false),
        line(16, 12, 13),
        line(17, 13, 14),
        line(18, 14, 15),
        line(19, 15, 12),
        pt(20, 0.0, 0.00928513, true),
        pt(21, -0.00850528, 0.0, true),
    ];
    let constraints = vec![
        SketchConstraint::Horizontal { entity: 6 },
        SketchConstraint::Horizontal { entity: 8 },
        SketchConstraint::Vertical { entity: 7 },
        SketchConstraint::Vertical { entity: 9 },
        SketchConstraint::Midpoint { point: 10, line: 8 },
        SketchConstraint::Midpoint { point: 11, line: 9 },
        SketchConstraint::VerticalPoints {
            point_a: 1,
            point_b: 10,
        },
        SketchConstraint::HorizontalPoints {
            point_a: 1,
            point_b: 11,
        },
        SketchConstraint::Horizontal { entity: 16 },
        SketchConstraint::Horizontal { entity: 18 },
        SketchConstraint::Vertical { entity: 17 },
        SketchConstraint::Vertical { entity: 19 },
        SketchConstraint::Midpoint {
            point: 20,
            line: 18,
        },
        SketchConstraint::Midpoint {
            point: 21,
            line: 19,
        },
        SketchConstraint::VerticalPoints {
            point_a: 1,
            point_b: 20,
        },
        SketchConstraint::HorizontalPoints {
            point_a: 1,
            point_b: 21,
        },
        SketchConstraint::Equal {
            entity_a: 18,
            entity_b: 19,
        },
    ];
    (entities, constraints)
}

fn lcg(state: &mut u64) -> f64 {
    // deterministic pseudo-random in [0,1)
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*state >> 11) as f64) / ((1u64 << 53) as f64)
}

#[test]
fn hunt_explosion_waffle_fixture() {
    let (base_entities, constraints) = fixture();
    let drag_pt = 15u32;
    let mut worst_overall = 0.0f64;
    let mut exploded_seeds = Vec::new();

    for seed in 0..40u64 {
        let mut rng = seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1);
        let mut positions: HashMap<u32, (f64, f64)> = base_entities
            .iter()
            .filter_map(|e| match e {
                SketchEntity::Point { id, x, y, .. } => Some((*id, (*x, *y))),
                _ => None,
            })
            .collect();
        let start = positions[&drag_pt];
        let mut max_coord = 0.0f64;
        let mut first_explosion_step = None;

        for step in 0..60usize {
            // random-walk mouse: per-frame delta up to ±4mm, occasionally big
            // jumps (fast drag while zoomed out) up to ±20mm
            let big = lcg(&mut rng) < 0.15;
            let amp = if big { 0.020 } else { 0.004 };
            let cur = positions[&drag_pt];
            let mouse = (
                cur.0 + amp * (lcg(&mut rng) * 2.0 - 1.0),
                cur.1 + amp * (lcg(&mut rng) * 2.0 - 1.0),
            );
            positions.insert(drag_pt, mouse);

            let entities: Vec<SketchEntity> = base_entities
                .iter()
                .map(|e| match e {
                    SketchEntity::Point {
                        id, construction, ..
                    } => {
                        let (x, y) = positions[id];
                        pt(*id, x, y, *construction)
                    }
                    other => other.clone(),
                })
                .collect();
            let mut cons = constraints.clone();
            cons.push(SketchConstraint::Dragged { point: drag_pt });
            let sketch = Sketch {
                id: Uuid::nil(),
                plane: dummy_geom_ref(),
                plane_origin: [0.0, 0.0, 0.0],
                plane_normal: [0.0, 0.0, 1.0],
                entities,
                constraints: cons,
                solve_status: SolveStatus::UnderConstrained { dof: 99 },
                solved_positions: HashMap::new(),
                solved_profiles: Vec::new(),
                projected: Vec::new(),
            };
            let solved = solve_sketch(&sketch);
            for (id, p) in &solved.positions {
                positions.insert(*id, *p);
            }
            let step_max = solved
                .positions
                .values()
                .flat_map(|(x, y)| [x.abs(), y.abs()])
                .fold(0.0f64, f64::max);
            max_coord = max_coord.max(step_max);
            if step_max > 0.5 && first_explosion_step.is_none() {
                first_explosion_step = Some((step, step_max, format!("{:?}", solved.status)));
            }
        }
        worst_overall = worst_overall.max(max_coord);
        if let Some((step, mag, status)) = first_explosion_step {
            exploded_seeds.push(seed);
            if exploded_seeds.len() <= 5 {
                println!(
                    "seed {seed:3}: EXPLODED at step {step} to {mag:.3} m (start {:.4}) status={status}",
                    start.0
                );
            }
        }
    }
    println!(
        "\n{}/40 seeds exploded (>0.5 m). worst overall = {worst_overall:.3} m (sketch is 0.026 m wide)",
        exploded_seeds.len()
    );
    println!(
        "exploded seeds: {:?}",
        &exploded_seeds[..exploded_seeds.len().min(20)]
    );
    assert!(exploded_seeds.is_empty(), "explosion reproduced");
}

/// Accidental-pin scenario: a previous drag released near a snap target and
/// silently added Pinned(p15, release_spot) — weight 1.0. The NEXT drag of
/// p15 (or a coupled corner) fights that pin: weight-1 conflict → large cost
/// budget → does LM run away?
#[test]
fn hunt_explosion_with_accidental_pin() {
    let (base_entities, constraints_base) = fixture();
    let mut worst = 0.0f64;
    let mut exploded = Vec::new();

    // pin p15 where a drag might have released it (a few plausible spots)
    let pin_spots = [
        (-0.0132331, 0.0127945), // outer corner p5 (Coincident-ish)
        (0.0, 0.0127945),        // outer top midpoint p10
        (0.0, 0.0),              // origin
        (-0.02, 0.02),           // free space release
    ];
    // then drag various points afterwards
    let drag_targets = [15u32, 14, 12, 4, 5];

    for (si, &(px, py)) in pin_spots.iter().enumerate() {
        for &drag_pt in &drag_targets {
            let mut positions: HashMap<u32, (f64, f64)> = base_entities
                .iter()
                .filter_map(|e| match e {
                    SketchEntity::Point { id, x, y, .. } => Some((*id, (*x, *y))),
                    _ => None,
                })
                .collect();
            // the accidental pin has already "won" a solve: put p15 there
            positions.insert(15, (px, py));

            let mut rng = (si as u64 + 1).wrapping_mul(0xD1B54A32D192ED03);
            let start = positions[&drag_pt];
            let mut max_coord = 0.0f64;
            let mut blew = None;

            for step in 0..40usize {
                let cur = positions[&drag_pt];
                let big = lcg(&mut rng) < 0.2;
                let amp = if big { 0.02 } else { 0.004 };
                let mouse = (
                    cur.0 + amp * (lcg(&mut rng) * 2.0 - 1.0),
                    cur.1 + amp * (lcg(&mut rng) * 2.0 - 1.0),
                );
                positions.insert(drag_pt, mouse);
                let entities: Vec<SketchEntity> = base_entities
                    .iter()
                    .map(|e| match e {
                        SketchEntity::Point {
                            id, construction, ..
                        } => {
                            let (x, y) = positions[id];
                            pt(*id, x, y, *construction)
                        }
                        other => other.clone(),
                    })
                    .collect();
                let mut cons = constraints_base.clone();
                cons.push(SketchConstraint::Pinned {
                    point: 15,
                    x: px,
                    y: py,
                });
                cons.push(SketchConstraint::Dragged { point: drag_pt });
                let sketch = Sketch {
                    id: Uuid::nil(),
                    plane: dummy_geom_ref(),
                    plane_origin: [0.0, 0.0, 0.0],
                    plane_normal: [0.0, 0.0, 1.0],
                    entities,
                    constraints: cons,
                    solve_status: SolveStatus::UnderConstrained { dof: 99 },
                    solved_positions: HashMap::new(),
                    solved_profiles: Vec::new(),
                    projected: Vec::new(),
                };
                let solved = solve_sketch(&sketch);
                for (id, p) in &solved.positions {
                    positions.insert(*id, *p);
                }
                let step_max = solved
                    .positions
                    .values()
                    .flat_map(|(x, y)| [x.abs(), y.abs()])
                    .fold(0.0f64, f64::max);
                max_coord = max_coord.max(step_max);
                if step_max > 0.5 && blew.is_none() {
                    blew = Some((step, step_max, format!("{:?}", solved.status)));
                }
            }
            worst = worst.max(max_coord);
            if let Some((step, mag, status)) = blew {
                exploded.push((si, drag_pt));
                println!(
                    "pin_spot {si} drag p{drag_pt}: EXPLODED step {step} to {mag:.3e} m (start {:.4}) {status}",
                    start.0
                );
            } else {
                println!("pin_spot {si} drag p{drag_pt}: ok, max {max_coord:.4}");
            }
        }
    }
    println!("\nworst = {worst:.3e} m; exploded combos: {exploded:?}");
    assert!(
        exploded.is_empty(),
        "explosion reproduced with accidental pin"
    );
}

/// Zoomed-out fast drags: per-frame mouse deltas of 0.05–2.0 m (what a 100px
/// move means when the view is zoomed way out). Sweep delta amplitude.
#[test]
fn hunt_explosion_large_mouse_deltas() {
    let (base_entities, constraints_base) = fixture();
    let drag_pt = 15u32;
    let mut exploded = Vec::new();

    for (ai, &amp) in [0.02, 0.05, 0.1, 0.3, 1.0, 2.0].iter().enumerate() {
        for seed in 0..15u64 {
            let mut rng = (seed + 1).wrapping_mul(0xA0761D6478BD642F) ^ (ai as u64);
            let mut positions: HashMap<u32, (f64, f64)> = base_entities
                .iter()
                .filter_map(|e| match e {
                    SketchEntity::Point { id, x, y, .. } => Some((*id, (*x, *y))),
                    _ => None,
                })
                .collect();
            let mut max_coord = 0.0f64;
            let mut blew = None;
            for step in 0..25usize {
                let cur = positions[&drag_pt];
                let mouse = (
                    cur.0 + amp * (lcg(&mut rng) * 2.0 - 1.0),
                    cur.1 + amp * (lcg(&mut rng) * 2.0 - 1.0),
                );
                positions.insert(drag_pt, mouse);
                let entities: Vec<SketchEntity> = base_entities
                    .iter()
                    .map(|e| match e {
                        SketchEntity::Point {
                            id, construction, ..
                        } => {
                            let (x, y) = positions[id];
                            pt(*id, x, y, *construction)
                        }
                        other => other.clone(),
                    })
                    .collect();
                let mut cons = constraints_base.clone();
                cons.push(SketchConstraint::Dragged { point: drag_pt });
                let sketch = Sketch {
                    id: Uuid::nil(),
                    plane: dummy_geom_ref(),
                    plane_origin: [0.0, 0.0, 0.0],
                    plane_normal: [0.0, 0.0, 1.0],
                    entities,
                    constraints: cons,
                    solve_status: SolveStatus::UnderConstrained { dof: 99 },
                    solved_positions: HashMap::new(),
                    solved_profiles: Vec::new(),
                    projected: Vec::new(),
                };
                let solved = solve_sketch(&sketch);
                for (id, p) in &solved.positions {
                    positions.insert(*id, *p);
                }
                let step_max = solved
                    .positions
                    .values()
                    .flat_map(|(x, y)| [x.abs(), y.abs()])
                    .fold(0.0f64, f64::max);
                max_coord = max_coord.max(step_max);
                // "explosion" = far beyond both sketch and mouse envelope
                let mouse_env = amp * 25.0 + 0.05;
                if (step_max > 20.0 * mouse_env || !step_max.is_finite()) && blew.is_none() {
                    blew = Some((step, step_max));
                }
            }
            if let Some((step, mag)) = blew {
                exploded.push((amp, seed, step, mag));
            }
            let _ = max_coord;
        }
    }
    for (amp, seed, step, mag) in &exploded {
        println!("amp={amp} seed={seed}: EXPLODED step {step} to {mag:.3e}");
    }
    println!("total exploded: {}/90", exploded.len());
    assert!(exploded.is_empty(), "explosion reproduced at large deltas");
}
