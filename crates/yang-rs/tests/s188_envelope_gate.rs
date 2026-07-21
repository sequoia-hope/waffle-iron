//! #188 inc-2 — gate-ON end-to-end fixture for the §3.3 boundary-envelope
//! rebuild (spec `specs/yang_188_f0082_j3_envelope_selection.md` §5 inc-2):
//! the F0082 shape WITHOUT the wall — a slab unioned with a tube whose
//! bottom cap center sits exactly ON the slab's top plane, axis tilted by
//! 5e-3 rad, so the section ellipse (tube ∩ slab-top) and the cap rim
//! osculate (gap amplitude r·tanθ ≈ 1e-3, far below the mesh chord
//! sagitta) and the two planes' intersection line passes through the tube
//! axis (triple points exactly at (0, ±r, 1)).
//!
//! Gate-ON acceptance (§3.3 postconditions at the yang B-Rep level; the
//! render-CDT half of the acceptance runs in kernel-v2 and is covered by
//! the inc-3 gate-ON assay):
//! - the union succeeds;
//! - the tube face's bottom boundary cycle is SIMPLE and azimuth-monotone
//!   (no fold, no dead-side detour);
//! - it switches support exactly at the two free-space triple points
//!   (junction verts within 1e-6 of the analytic pins);
//! - every other bottom-cycle vert lies on the band-live support given by
//!   the inc-1 classifier (ellipse where live, rim where live).
//!
//! Runs in its own process (integration test) so `set_var` cannot race
//! other tests. The gate-OFF control runs FIRST, before the env is set.

use cad_primitives::{BoolOp, Point3, Vector3};
use yang_rs::stage5_envelope::{
    classify_bands, cylinder_two_plane_switch_points, BandLive, EnvPlane, TripleClass,
};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, InputId, Surface};

// ---- array math (integration tests cannot share helpers) ------------------

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    scale(a, 1.0 / norm(a))
}
fn wrap(mut t: f64) -> f64 {
    while t > std::f64::consts::PI {
        t -= 2.0 * std::f64::consts::PI;
    }
    while t <= -std::f64::consts::PI {
        t += 2.0 * std::f64::consts::PI;
    }
    t
}

// ---- fixture solids -------------------------------------------------------

const TILT: f64 = 5e-3;
const R: f64 = 0.2;
const H: f64 = 0.5;
/// Tube bottom-cap center: exactly on the slab's top plane z = 1.
const BOT: [f64; 3] = [0.0, 0.0, 1.0];

fn axis_unit() -> [f64; 3] {
    [TILT.sin(), 0.0, TILT.cos()]
}

fn slab() -> BRep {
    let lo = [-1.0, -1.0, 0.0];
    let hi = [1.0, 1.0, 1.0];
    let v = |x: f64, y: f64, z: f64| BRepVertex {
        point: Point3::new(x, y, z),
    };
    let vertices = vec![
        v(lo[0], lo[1], lo[2]),
        v(hi[0], lo[1], lo[2]),
        v(hi[0], hi[1], lo[2]),
        v(lo[0], hi[1], lo[2]),
        v(hi[0], hi[1], hi[2]),
        v(hi[0], lo[1], hi[2]),
        v(lo[0], lo[1], hi[2]),
        v(lo[0], hi[1], hi[2]),
    ];
    const EDGE_PAIRS: [(u32, u32); 24] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (2, 1),
        (1, 5),
        (5, 4),
        (4, 2),
        (3, 2),
        (2, 4),
        (4, 7),
        (7, 3),
        (0, 3),
        (3, 7),
        (7, 6),
        (6, 0),
        (1, 0),
        (0, 6),
        (6, 5),
        (5, 1),
    ];
    let edges: Vec<BRepEdge> = EDGE_PAIRS
        .iter()
        .map(|&(start, end)| BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        })
        .collect();
    let planes: [([f64; 3], f64); 6] = [
        ([0.0, 0.0, -1.0], lo[2]),
        ([0.0, 0.0, 1.0], -hi[2]),
        ([1.0, 0.0, 0.0], -hi[0]),
        ([0.0, 1.0, 0.0], -hi[1]),
        ([-1.0, 0.0, 0.0], lo[0]),
        ([0.0, -1.0, 0.0], lo[1]),
    ];
    let faces: Vec<BRepFace> = planes
        .iter()
        .enumerate()
        .map(|(i, &(n, d))| BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(n[0], n[1], n[2]),
                d,
            },
            outer_loop: (4 * i as u32..4 * i as u32 + 4).collect(),
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(vertices, edges, faces).expect("slab fixture builds")
}

fn tube() -> BRep {
    let d = axis_unit();
    let bot = BOT;
    let top = add(bot, scale(d, H));
    // Seam frame: same construction as the rj/yr fixtures.
    let abs = [d[0].abs(), d[1].abs(), d[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(d, world));
    let verts = vec![
        BRepVertex {
            point: Point3::new(bot[0] + e1[0] * R, bot[1] + e1[1] * R, bot[2] + e1[2] * R),
        },
        BRepVertex {
            point: Point3::new(top[0] + e1[0] * R, top[1] + e1[1] * R, top[2] + e1[2] * R),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(bot[0], bot[1], bot[2]),
                normal: Vector3::new(-d[0], -d[1], -d[2]),
                radius: R,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(top[0], top[1], top[2]),
                normal: Vector3::new(d[0], d[1], d[2]),
                radius: R,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(bot[0], bot[1], bot[2]),
                axis_dir: Vector3::new(d[0], d[1], d[2]),
                radius: R,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(-d[0], -d[1], -d[2]),
                d: dot(d, bot),
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(d[0], d[1], d[2]),
                d: -dot(d, top),
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("tube fixture builds")
}

/// The pair planes with the operands' outward normals.
fn p_int() -> EnvPlane {
    EnvPlane {
        n: [0.0, 0.0, 1.0],
        d: -1.0,
    }
}
fn p_orig() -> EnvPlane {
    let d = axis_unit();
    EnvPlane {
        n: [-d[0], -d[1], -d[2]],
        d: dot(d, BOT),
    }
}

/// Cylinder chart of the output tube face (the fixture's exact surface).
fn chart(p: [f64; 3]) -> (f64, f64) {
    let a = axis_unit();
    // Deterministic seed frame, same rule as stage5_envelope::CylFrame.
    let abs = [a[0].abs(), a[1].abs(), a[2].abs()];
    let k = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        0
    } else if abs[1] <= abs[2] {
        1
    } else {
        2
    };
    let mut seed = [0.0f64; 3];
    seed[k] = 1.0;
    let pa = dot(seed, a);
    let x_hat = unit(sub(seed, scale(a, pa)));
    let y_hat = cross(a, x_hat);
    let q = sub(p, BOT);
    let v = dot(q, a);
    let w = sub(q, scale(a, v));
    (dot(w, y_hat).atan2(dot(w, x_hat)), v)
}

#[test]
fn gate_on_synthetic_free_space_envelope() {
    let Some(backend) = yang_rs::native_backend() else {
        eprintln!("[s188] SKIP: native backend not linked (stub build)");
        return;
    };

    // Sanity: the inc-1 primitives on the fixture's analytic data — two
    // FREE-SPACE triples exactly at (0, ±R, 1).
    let tube_surface = Surface::Cylinder {
        axis_point: Point3::new(BOT[0], BOT[1], BOT[2]),
        axis_dir: Vector3::new(axis_unit()[0], axis_unit()[1], axis_unit()[2]),
        radius: R,
    };
    let triples = cylinder_two_plane_switch_points(&tube_surface, &p_int(), &p_orig())
        .expect("fixture pair is transversal at the triples");
    for t in &triples {
        assert!(
            (t.p[0].abs()) < 1e-9 && (t.p[1].abs() - R).abs() < 1e-9 && (t.p[2] - 1.0).abs() < 1e-9,
            "analytic triple off its pin: {:?}",
            t.p
        );
    }
    let bands = classify_bands(
        &tube_surface,
        &p_int(),
        &p_orig(),
        &[],
        BoolOp::Union,
        InputId::B,
    )
    .expect("fixture bands classify");
    assert!(bands
        .triples
        .iter()
        .all(|t| t.class == TripleClass::FreeSpace));

    // Gate-OFF control FIRST (env not yet set): record the outcome; the
    // corpus-level byte-identical check is the assay diff, not this test.
    let off = boolean(&slab(), &tube(), BoolOp::Union, &backend);
    eprintln!(
        "[s188] gate-off result: {}",
        match &off {
            Ok(_) => "Ok".to_string(),
            Err(e) => format!("Err({e:?})"),
        }
    );

    std::env::set_var("YANG_S5_ENVELOPE_ENABLE", "1");
    let out = boolean(&slab(), &tube(), BoolOp::Union, &backend)
        .expect("gate-ON union must succeed (§5 inc-2 acceptance)");

    // The output tube face, with the fixture's EXACT surface params.
    let face = out
        .faces()
        .iter()
        .find(|f| matches!(f.surface, Surface::Cylinder { .. }))
        .expect("output must carry the tube face");

    // Collect the face's cycles as vert sequences.
    let loop_verts = |loop_edges: &[u32]| -> Vec<u32> {
        loop_edges
            .iter()
            .map(|&ei| out.edges()[ei as usize].start)
            .collect()
    };
    let mut cycles: Vec<Vec<u32>> = vec![loop_verts(&face.outer_loop)];
    for il in &face.inner_loops {
        cycles.push(loop_verts(il));
    }
    assert_eq!(cycles.len(), 2, "tube face must have exactly 2 cycles");

    // The bottom cycle = the one whose mean axial height is lower.
    let mean_v = |c: &[u32]| -> f64 {
        c.iter()
            .map(|&v| chart(out.vertices()[v as usize].point.as_array()).1)
            .sum::<f64>()
            / c.len() as f64
    };
    cycles.sort_by(|a, b| mean_v(a).total_cmp(&mean_v(b)));
    let bottom = &cycles[0];
    assert!(
        bottom.len() >= 6,
        "bottom cycle implausibly small: {bottom:?}"
    );

    // (1) SIMPLE: no repeated vertex.
    let set: std::collections::BTreeSet<u32> = bottom.iter().copied().collect();
    assert_eq!(set.len(), bottom.len(), "bottom cycle repeats a vertex");

    // (2) Azimuth-monotone circularly: all wrapped steps share one sign
    // and sum to ±2π (no fold, no dead-side detour).
    let thetas: Vec<f64> = bottom
        .iter()
        .map(|&v| chart(out.vertices()[v as usize].point.as_array()).0)
        .collect();
    let m = thetas.len();
    let steps: Vec<f64> = (0..m)
        .map(|i| wrap(thetas[(i + 1) % m] - thetas[i]))
        .collect();
    let total: f64 = steps.iter().sum();
    assert!(
        (total.abs() - 2.0 * std::f64::consts::PI).abs() < 1e-9,
        "bottom cycle must wind exactly once: total={total}"
    );
    for (i, s) in steps.iter().enumerate() {
        assert!(
            s * total >= 0.0,
            "azimuth fold at step {i} (vert {} -> {}): dθ={s:.6e} against winding {total:.3}",
            bottom[i],
            bottom[(i + 1) % m]
        );
    }

    // (3) Junctions exactly at the two free-space triples; (4) every other
    // vert on the band-live support.
    let sd = |pl: &EnvPlane, p: [f64; 3]| pl.n[0] * p[0] + pl.n[1] * p[1] + pl.n[2] * p[2] + pl.d;
    let mut junction_count = 0usize;
    for &v in bottom {
        let p = out.vertices()[v as usize].point.as_array();
        let near_triple = triples.iter().any(|t| norm(sub(p, t.p)) < 1e-6);
        if near_triple {
            junction_count += 1;
            continue;
        }
        let (theta, _) = chart(p);
        let on_int = sd(&p_int(), p).abs() < 1e-7 * (1.0 + norm(p));
        let on_orig = sd(&p_orig(), p).abs() < 1e-7 * (1.0 + norm(p));
        match bands.live_at(theta) {
            Some(BandLive::IntCurve) => assert!(
                on_int,
                "vert {v} at θ={theta:.4} must lie on the ellipse (sd_int={:.3e}, sd_orig={:.3e})",
                sd(&p_int(), p),
                sd(&p_orig(), p)
            ),
            Some(BandLive::OrigCurve) => assert!(
                on_orig,
                "vert {v} at θ={theta:.4} must lie on the rim (sd_int={:.3e}, sd_orig={:.3e})",
                sd(&p_int(), p),
                sd(&p_orig(), p)
            ),
            other => panic!("unexpected band at θ={theta:.4}: {other:?}"),
        }
    }
    assert_eq!(
        junction_count, 2,
        "bottom cycle must carry exactly the two triple-point junctions"
    );
}
