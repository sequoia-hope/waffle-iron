//! SI5 C5b fixture shells — the torus and sphere PATCH forms the corpus writes
//! (spec `si5_c5_sphere_torus_tier.md` §2.4), built the way an exchange file
//! writes them: index tables, `same_sense` kept as a flag, every arc's side
//! pinned by its `interior` point. Public because no kernel-v2 constructor can
//! build a fillet or a corner blend, so these shells are how such solids enter
//! the arena at all — the kernel's own tests pin their closed-form volumes,
//! and the test-harness exports them as the analytic STEP fixtures the
//! export → extract → ingest fixed-point oracle runs on.

use cad_primitives::{Point3, Vector3};
use std::f64::consts::PI;
use waffle_types::kernel::{
    AnalyticCurve, AnalyticEdge, AnalyticFace, AnalyticLoop, AnalyticShellData, AnalyticSurface,
    OrientedEdge,
};

const X: Vector3 = Vector3::new(1.0, 0.0, 0.0);
const Y: Vector3 = Vector3::new(0.0, 1.0, 0.0);
const Z: Vector3 = Vector3::new(0.0, 0.0, 1.0);
const S2: f64 = std::f64::consts::FRAC_1_SQRT_2;

fn v(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn edge(start: u32, end: u32) -> AnalyticEdge {
    AnalyticEdge {
        start,
        end,
        curve: AnalyticCurve::Line,
    }
}

/// An arc edge between two DISTINCT vertices, its side pinned by `interior`.
fn arc_edge(
    start: u32,
    end: u32,
    center: Point3,
    axis: Vector3,
    radius: f64,
    interior: Point3,
) -> AnalyticEdge {
    AnalyticEdge {
        start,
        end,
        curve: AnalyticCurve::Circle {
            center,
            normal: axis,
            radius,
            interior,
        },
    }
}

fn oe(edge: u32, forward: bool) -> OrientedEdge {
    OrientedEdge { edge, forward }
}

fn plane_face(
    origin: Point3,
    normal: Vector3,
    same_sense: bool,
    loop_edges: Vec<OrientedEdge>,
) -> AnalyticFace {
    AnalyticFace {
        surface: AnalyticSurface::Plane { origin, normal },
        loops: vec![AnalyticLoop::Edges(loop_edges)],
        same_sense,
    }
}

/// Move every point of a shell — vertices, curve centres and interior points,
/// surface origins — by `d`. The volume terms carry position-dependent pieces
/// (`C·ŵ`, `C·â`, `C·∫n dA`) that vanish for a solid centred on the origin,
/// so every C5b fixture is also tested displaced.
pub fn translate(mut shell: AnalyticShellData, d: [f64; 3]) -> AnalyticShellData {
    let mv = |p: Point3| v(p.x() + d[0], p.y() + d[1], p.z() + d[2]);
    for p in &mut shell.vertices {
        *p = mv(*p);
    }
    for e in &mut shell.edges {
        match &mut e.curve {
            AnalyticCurve::Line => {}
            AnalyticCurve::Circle {
                center, interior, ..
            }
            | AnalyticCurve::Ellipse {
                center, interior, ..
            } => {
                *center = mv(*center);
                *interior = mv(*interior);
            }
        }
    }
    for f in &mut shell.faces {
        match &mut f.surface {
            AnalyticSurface::Plane { origin, .. } => *origin = mv(*origin),
            AnalyticSurface::Cylinder { axis_point, .. } => *axis_point = mv(*axis_point),
            AnalyticSurface::Cone { apex, .. } => *apex = mv(*apex),
            AnalyticSurface::Sphere { center, .. } | AnalyticSurface::Torus { center, .. } => {
                *center = mv(*center)
            }
        }
    }
    shell
}

/// A QUARTER of the rounded puck (`x ≥ 0, y ≥ 0`): its fillet is the corpus's
/// own torus-patch form — the `CCCC` parameter rectangle of two latitude arcs
/// and two poloidal arcs (121 of 121 in the sample, spec §2.4) — beside a
/// cylinder arc patch, two quarter discs and two planar cut faces each bounded
/// by an arc (the fillet's profile) and lines. Volume: the puck's quarter.
pub fn quarter_puck(rc: f64, h: f64, rho: f64) -> AnalyticShellData {
    let big = rc - rho;
    let zc = h - rho;
    let o = v(0.0, 0.0, 0.0);
    AnalyticShellData {
        vertices: vec![
            o,               // 0  O0
            v(rc, 0.0, 0.0), // 1  A0
            v(0.0, rc, 0.0), // 2  B0
            v(rc, 0.0, zc),  // 3  A1
            v(0.0, rc, zc),  // 4  B1
            v(big, 0.0, h),  // 5  A2
            v(0.0, big, h),  // 6  B2
            v(0.0, 0.0, h),  // 7  O2
        ],
        edges: vec![
            arc_edge(1, 2, o, Z, rc, v(rc * S2, rc * S2, 0.0)), // 0 bottom arc
            edge(2, 0),                                         // 1
            edge(0, 1),                                         // 2
            edge(1, 3),                                         // 3 ruling +x
            edge(2, 4),                                         // 4 ruling +y
            arc_edge(3, 4, v(0.0, 0.0, zc), Z, rc, v(rc * S2, rc * S2, zc)), // 5 mid latitude
            // 6 poloidal arc in the plane y = 0, from the rim up to the top
            arc_edge(
                3,
                5,
                v(big, 0.0, zc),
                Y,
                rho,
                v(big + rho * S2, 0.0, zc + rho * S2),
            ),
            // 7 poloidal arc in the plane x = 0
            arc_edge(
                4,
                6,
                v(0.0, big, zc),
                X,
                rho,
                v(0.0, big + rho * S2, zc + rho * S2),
            ),
            arc_edge(5, 6, v(0.0, 0.0, h), Z, big, v(big * S2, big * S2, h)), // 8 top latitude
            edge(6, 7),                                                       // 9
            edge(7, 5),                                                       // 10
            edge(0, 7),                                                       // 11 axis
        ],
        faces: vec![
            // bottom quarter disc, outward −z
            plane_face(o, Z, false, vec![oe(1, false), oe(0, false), oe(2, false)]),
            AnalyticFace {
                surface: AnalyticSurface::Cylinder {
                    axis_point: o,
                    axis_dir: Z,
                    radius: rc,
                },
                loops: vec![AnalyticLoop::Edges(vec![
                    oe(0, true),
                    oe(4, true),
                    oe(5, false),
                    oe(3, false),
                ])],
                same_sense: true,
            },
            // the fillet: a convex quarter round, material inside the tube
            AnalyticFace {
                surface: AnalyticSurface::Torus {
                    center: v(0.0, 0.0, zc),
                    axis_dir: Z,
                    major_radius: big,
                    minor_radius: rho,
                },
                loops: vec![AnalyticLoop::Edges(vec![
                    oe(5, true),
                    oe(7, true),
                    oe(8, false),
                    oe(6, false),
                ])],
                same_sense: true,
            },
            // top quarter disc, outward +z
            plane_face(
                v(0.0, 0.0, h),
                Z,
                true,
                vec![oe(10, true), oe(8, true), oe(9, true)],
            ),
            // cut y = 0, outward −y
            plane_face(
                o,
                Y,
                false,
                vec![
                    oe(2, true),
                    oe(3, true),
                    oe(6, true),
                    oe(10, false),
                    oe(11, false),
                ],
            ),
            // cut x = 0, outward −x
            plane_face(
                o,
                X,
                false,
                vec![
                    oe(11, true),
                    oe(9, false),
                    oe(7, false),
                    oe(4, false),
                    oe(1, true),
                ],
            ),
        ],
    }
}

/// A QUARTER of the boss on a plate (`x ≥ 0, y ≥ 0`, the plate a square of
/// side `l` here): the concave fillet is a `reversed` torus patch, the
/// plate's top is a square minus a quarter disc, and the two cut faces each
/// carry the fillet's concave profile arc. Volume: `l² t` plus a quarter of
/// the boss and of the Pappus fillet term.
pub fn quarter_boss(l: f64, t: f64, rb: f64, hb: f64, rho: f64) -> AnalyticShellData {
    let big = rb + rho;
    let zt = t + rho;
    let o = v(0.0, 0.0, 0.0);
    AnalyticShellData {
        vertices: vec![
            o,                   // 0
            v(l, 0.0, 0.0),      // 1
            v(l, l, 0.0),        // 2
            v(0.0, l, 0.0),      // 3
            v(l, 0.0, t),        // 4
            v(l, l, t),          // 5
            v(0.0, l, t),        // 6
            v(big, 0.0, t),      // 7  A1 fillet base
            v(0.0, big, t),      // 8  B1
            v(rb, 0.0, zt),      // 9  A2 fillet top
            v(0.0, rb, zt),      // 10 B2
            v(rb, 0.0, t + hb),  // 11 A3
            v(0.0, rb, t + hb),  // 12 B3
            v(0.0, 0.0, t + hb), // 13 O3
        ],
        edges: vec![
            edge(0, 1),                                                       // 0
            edge(1, 2),                                                       // 1
            edge(2, 3),                                                       // 2
            edge(3, 0),                                                       // 3
            edge(1, 4),                                                       // 4
            edge(2, 5),                                                       // 5
            edge(3, 6),                                                       // 6
            edge(4, 5),                                                       // 7
            edge(5, 6),                                                       // 8
            edge(7, 4),                                                       // 9
            edge(6, 8),                                                       // 10
            arc_edge(7, 8, v(0.0, 0.0, t), Z, big, v(big * S2, big * S2, t)), // 11 base latitude
            // 12 concave poloidal arc in y = 0: from the base rim up and inward
            arc_edge(
                7,
                9,
                v(big, 0.0, zt),
                Y,
                rho,
                v(big - rho * S2, 0.0, zt - rho * S2),
            ),
            // 13 concave poloidal arc in x = 0
            arc_edge(
                8,
                10,
                v(0.0, big, zt),
                X,
                rho,
                v(0.0, big - rho * S2, zt - rho * S2),
            ),
            arc_edge(9, 10, v(0.0, 0.0, zt), Z, rb, v(rb * S2, rb * S2, zt)), // 14 top latitude
            edge(9, 11),                                                      // 15
            edge(10, 12),                                                     // 16
            arc_edge(
                11,
                12,
                v(0.0, 0.0, t + hb),
                Z,
                rb,
                v(rb * S2, rb * S2, t + hb),
            ), // 17
            edge(12, 13),                                                     // 18
            edge(13, 11),                                                     // 19
            edge(0, 13),                                                      // 20 axis
        ],
        faces: vec![
            // plate bottom, outward −z
            plane_face(
                o,
                Z,
                false,
                vec![oe(3, false), oe(2, false), oe(1, false), oe(0, false)],
            ),
            // x = l, outward +x
            plane_face(
                v(l, 0.0, 0.0),
                X,
                true,
                vec![oe(1, true), oe(5, true), oe(7, false), oe(4, false)],
            ),
            // y = l, outward +y
            plane_face(
                v(0.0, l, 0.0),
                Y,
                true,
                vec![oe(2, true), oe(6, true), oe(8, false), oe(5, false)],
            ),
            // plate top: the square minus the fillet's quarter disc
            plane_face(
                v(0.0, 0.0, t),
                Z,
                true,
                vec![
                    oe(9, true),
                    oe(7, true),
                    oe(8, true),
                    oe(10, true),
                    oe(11, false),
                ],
            ),
            // the fillet: concave, material OUTSIDE the tube
            AnalyticFace {
                surface: AnalyticSurface::Torus {
                    center: v(0.0, 0.0, zt),
                    axis_dir: Z,
                    major_radius: big,
                    minor_radius: rho,
                },
                loops: vec![AnalyticLoop::Edges(vec![
                    oe(11, true),
                    oe(13, true),
                    oe(14, false),
                    oe(12, false),
                ])],
                same_sense: false,
            },
            // the boss: a cylinder arc patch
            AnalyticFace {
                surface: AnalyticSurface::Cylinder {
                    axis_point: o,
                    axis_dir: Z,
                    radius: rb,
                },
                loops: vec![AnalyticLoop::Edges(vec![
                    oe(14, true),
                    oe(16, true),
                    oe(17, false),
                    oe(15, false),
                ])],
                same_sense: true,
            },
            // boss top, outward +z
            plane_face(
                v(0.0, 0.0, t + hb),
                Z,
                true,
                vec![oe(19, true), oe(17, true), oe(18, true)],
            ),
            // cut y = 0, outward −y
            plane_face(
                o,
                Y,
                false,
                vec![
                    oe(0, true),
                    oe(4, true),
                    oe(9, false),
                    oe(12, true),
                    oe(15, true),
                    oe(19, false),
                    oe(20, false),
                ],
            ),
            // cut x = 0, outward −x
            plane_face(
                o,
                X,
                false,
                vec![
                    oe(20, true),
                    oe(18, false),
                    oe(16, false),
                    oe(13, false),
                    oe(10, false),
                    oe(6, false),
                    oe(3, true),
                ],
            ),
        ],
    }
}

pub fn quarter_boss_volume(l: f64, t: f64, rb: f64, hb: f64, rho: f64) -> f64 {
    l * l * t
        + (PI * rb * rb * hb
            + 2.0
                * PI
                * (rho * rho * (rb + rho / 2.0) - (PI * rho * rho / 4.0) * (rb + rho)
                    + rho * rho * rho / 3.0))
            / 4.0
}

/// An OCTANT of a ball: three quarter discs and a spherical triangle of three
/// great-circle arcs — the corpus's `CCC` sphere patch with `h = 0` on every
/// arc, so the area is pure exterior angle (three right angles). Volume
/// `π r³ / 6`.
pub fn ball_octant(r: f64) -> AnalyticShellData {
    let o = v(0.0, 0.0, 0.0);
    AnalyticShellData {
        vertices: vec![o, v(r, 0.0, 0.0), v(0.0, r, 0.0), v(0.0, 0.0, r)],
        edges: vec![
            edge(0, 1),
            edge(0, 2),
            edge(0, 3),
            arc_edge(1, 2, o, Z, r, v(r * S2, r * S2, 0.0)),
            arc_edge(2, 3, o, X, r, v(0.0, r * S2, r * S2)),
            arc_edge(3, 1, o, Y, r, v(r * S2, 0.0, r * S2)),
        ],
        faces: vec![
            plane_face(o, Z, false, vec![oe(1, true), oe(3, false), oe(0, false)]),
            plane_face(o, X, false, vec![oe(2, true), oe(4, false), oe(1, false)]),
            plane_face(o, Y, false, vec![oe(0, true), oe(5, false), oe(2, false)]),
            AnalyticFace {
                surface: AnalyticSurface::Sphere {
                    center: o,
                    radius: r,
                },
                loops: vec![AnalyticLoop::Edges(vec![
                    oe(3, true),
                    oe(4, true),
                    oe(5, true),
                ])],
                same_sense: true,
            },
        ],
    }
}

/// The ball octant cut at `z = zc`: the sphere patch gains a SMALL-circle arc
/// (`h = zc`, where the geodesic-curvature term is live) beside two great
/// arcs and the quarter equator, and the top is a quarter disc of radius
/// `√(r² − zc²)`. Volume: the octant less a quarter of the cap above `zc`,
/// `π r³ / 6 − π (r − zc)² (2r + zc) / 12`.
pub fn capped_octant(r: f64, zc: f64) -> AnalyticShellData {
    let a = (r * r - zc * zc).sqrt();
    let o = v(0.0, 0.0, 0.0);
    // Half the elevation of the cut, for the great arcs' interior points.
    let (sh, ch) = (zc.atan2(a) / 2.0).sin_cos();
    AnalyticShellData {
        vertices: vec![
            o,               // 0 O0
            v(r, 0.0, 0.0),  // 1 A0
            v(0.0, r, 0.0),  // 2 B0
            v(a, 0.0, zc),   // 3 A1
            v(0.0, a, zc),   // 4 B1
            v(0.0, 0.0, zc), // 5 O1
        ],
        edges: vec![
            arc_edge(1, 2, o, Z, r, v(r * S2, r * S2, 0.0)), // 0 quarter equator
            edge(2, 0),                                      // 1
            edge(0, 1),                                      // 2
            arc_edge(1, 3, o, Y, r, v(r * ch, 0.0, r * sh)), // 3 great arc, y = 0
            arc_edge(2, 4, o, X, r, v(0.0, r * ch, r * sh)), // 4 great arc, x = 0
            arc_edge(3, 4, v(0.0, 0.0, zc), Z, a, v(a * S2, a * S2, zc)), // 5 small circle
            edge(4, 5),                                      // 6
            edge(5, 3),                                      // 7
            edge(0, 5),                                      // 8 axis
        ],
        faces: vec![
            plane_face(o, Z, false, vec![oe(1, false), oe(0, false), oe(2, false)]),
            plane_face(
                o,
                Y,
                false,
                vec![oe(2, true), oe(3, true), oe(7, false), oe(8, false)],
            ),
            plane_face(
                o,
                X,
                false,
                vec![oe(8, true), oe(6, false), oe(4, false), oe(1, true)],
            ),
            plane_face(
                v(0.0, 0.0, zc),
                Z,
                true,
                vec![oe(7, true), oe(5, true), oe(6, true)],
            ),
            AnalyticFace {
                surface: AnalyticSurface::Sphere {
                    center: o,
                    radius: r,
                },
                loops: vec![AnalyticLoop::Edges(vec![
                    oe(0, true),
                    oe(4, true),
                    oe(5, false),
                    oe(3, false),
                ])],
                same_sense: true,
            },
        ],
    }
}

pub fn capped_octant_volume(r: f64, zc: f64) -> f64 {
    PI * r * r * r / 6.0 - PI * (r - zc) * (r - zc) * (2.0 * r + zc) / 12.0
}

/// A cube of side `l` with a ball octant of radius `r` scooped out of one
/// corner: the sphere patch is a `reversed` dimple (material OUTSIDE the
/// ball), and the three cut faces are squares minus a quarter disc. Volume
/// `l³ − π r³ / 6`.
pub fn dimpled_cube(l: f64, r: f64) -> AnalyticShellData {
    let o = v(0.0, 0.0, 0.0);
    AnalyticShellData {
        vertices: vec![
            v(l, 0.0, 0.0), // 0
            v(l, l, 0.0),   // 1
            v(0.0, l, 0.0), // 2
            v(0.0, 0.0, l), // 3
            v(l, 0.0, l),   // 4
            v(l, l, l),     // 5
            v(0.0, l, l),   // 6
            v(r, 0.0, 0.0), // 7 A
            v(0.0, r, 0.0), // 8 B
            v(0.0, 0.0, r), // 9 C
        ],
        edges: vec![
            edge(7, 0),                                      // 0
            edge(0, 1),                                      // 1
            edge(1, 2),                                      // 2
            edge(2, 8),                                      // 3
            arc_edge(7, 8, o, Z, r, v(r * S2, r * S2, 0.0)), // 4
            arc_edge(8, 9, o, X, r, v(0.0, r * S2, r * S2)), // 5
            arc_edge(9, 7, o, Y, r, v(r * S2, 0.0, r * S2)), // 6
            edge(0, 4),                                      // 7
            edge(1, 5),                                      // 8
            edge(2, 6),                                      // 9
            edge(3, 4),                                      // 10
            edge(4, 5),                                      // 11
            edge(5, 6),                                      // 12
            edge(6, 3),                                      // 13
            edge(9, 3),                                      // 14
        ],
        faces: vec![
            // z = 0, outward −z
            plane_face(
                o,
                Z,
                false,
                vec![
                    oe(4, true),
                    oe(3, false),
                    oe(2, false),
                    oe(1, false),
                    oe(0, false),
                ],
            ),
            // x = 0, outward −x
            plane_face(
                o,
                X,
                false,
                vec![
                    oe(5, true),
                    oe(14, true),
                    oe(13, false),
                    oe(9, false),
                    oe(3, true),
                ],
            ),
            // y = 0, outward −y
            plane_face(
                o,
                Y,
                false,
                vec![
                    oe(0, true),
                    oe(7, true),
                    oe(10, false),
                    oe(14, false),
                    oe(6, true),
                ],
            ),
            plane_face(
                v(l, 0.0, 0.0),
                X,
                true,
                vec![oe(1, true), oe(8, true), oe(11, false), oe(7, false)],
            ),
            plane_face(
                v(0.0, l, 0.0),
                Y,
                true,
                vec![oe(2, true), oe(9, true), oe(12, false), oe(8, false)],
            ),
            plane_face(
                v(0.0, 0.0, l),
                Z,
                true,
                vec![oe(10, true), oe(11, true), oe(12, true), oe(13, true)],
            ),
            // the dimple: cavity sense
            AnalyticFace {
                surface: AnalyticSurface::Sphere {
                    center: o,
                    radius: r,
                },
                loops: vec![AnalyticLoop::Edges(vec![
                    oe(6, false),
                    oe(5, false),
                    oe(4, false),
                ])],
                same_sense: false,
            },
        ],
    }
}
