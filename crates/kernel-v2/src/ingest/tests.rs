//! `ingest_analytic` tests — SI5 C3 (planar) and C4a (full curved bands).
//!
//! The fixtures are built the way an exchange file writes them, not the way a
//! constructor does: index tables, loops in arbitrary order, orientation kept
//! as `same_sense` rather than folded into the geometry.

use super::*;
use cad_primitives::Vector3;
use std::f64::consts::PI;
use waffle_types::kernel::{AnalyticCurve, AnalyticEdge, AnalyticFace, OrientedEdge};

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

/// A closed rim: one `EDGE_CURVE` whose start and end are the same vertex, as
/// every real writer emits a full circular rim.
fn rim_edge(
    anchor: u32,
    center: Point3,
    axis: Vector3,
    radius: f64,
    interior: Point3,
) -> AnalyticEdge {
    AnalyticEdge {
        start: anchor,
        end: anchor,
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

// ---------------------------------------------------------------------------
// C3 — the planar tier
// ---------------------------------------------------------------------------

/// The unit box, as an exchange file would write it: 8 vertices, 12 shared
/// edges, 6 planar faces whose outer loops run CCW about the OUTWARD
/// normal. The top face deliberately declares the *downward* plane normal
/// with `same_sense: false` — the shape a real file takes when the face
/// and the surface disagree — so the ingest path's orientation handling is
/// exercised rather than only its identity case.
fn unit_box() -> AnalyticShellData {
    AnalyticShellData {
        vertices: vec![
            v(0.0, 0.0, 0.0),
            v(1.0, 0.0, 0.0),
            v(1.0, 1.0, 0.0),
            v(0.0, 1.0, 0.0),
            v(0.0, 0.0, 1.0),
            v(1.0, 0.0, 1.0),
            v(1.0, 1.0, 1.0),
            v(0.0, 1.0, 1.0),
        ],
        edges: vec![
            edge(0, 1),
            edge(1, 2),
            edge(2, 3),
            edge(3, 0), // 0..3  bottom ring
            edge(4, 5),
            edge(5, 6),
            edge(6, 7),
            edge(7, 4), // 4..7  top ring
            edge(0, 4),
            edge(1, 5),
            edge(2, 6),
            edge(3, 7), // 8..11 posts
        ],
        faces: vec![
            // z = 0, outward −z: 0 → 3 → 2 → 1
            plane_face(
                v(0.0, 0.0, 0.0),
                Vector3::new(0.0, 0.0, -1.0),
                true,
                vec![oe(3, false), oe(2, false), oe(1, false), oe(0, false)],
            ),
            // z = 1, outward +z, declared as −z with same_sense = false:
            // 4 → 5 → 6 → 7
            plane_face(
                v(0.0, 0.0, 1.0),
                Vector3::new(0.0, 0.0, -1.0),
                false,
                vec![oe(4, true), oe(5, true), oe(6, true), oe(7, true)],
            ),
            // y = 0, outward −y: 0 → 1 → 5 → 4
            plane_face(
                v(0.0, 0.0, 0.0),
                Vector3::new(0.0, -1.0, 0.0),
                true,
                vec![oe(0, true), oe(9, true), oe(4, false), oe(8, false)],
            ),
            // x = 1, outward +x: 1 → 2 → 6 → 5
            plane_face(
                v(1.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
                true,
                vec![oe(1, true), oe(10, true), oe(5, false), oe(9, false)],
            ),
            // y = 1, outward +y: 2 → 3 → 7 → 6
            plane_face(
                v(0.0, 1.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
                true,
                vec![oe(2, true), oe(11, true), oe(6, false), oe(10, false)],
            ),
            // x = 0, outward −x: 3 → 0 → 4 → 7
            plane_face(
                v(0.0, 0.0, 0.0),
                Vector3::new(-1.0, 0.0, 0.0),
                true,
                vec![oe(3, true), oe(8, true), oe(7, false), oe(11, false)],
            ),
        ],
    }
}

/// Reverse every loop AND negate every face sense: a consistently
/// INWARD-oriented shell. Every structural and winding check still passes
/// (it is a perfectly good closed oriented surface) — only the volume
/// sign distinguishes it.
fn inverted(mut shell: AnalyticShellData) -> AnalyticShellData {
    for face in &mut shell.faces {
        face.same_sense = !face.same_sense;
        for lp in &mut face.loops {
            if let AnalyticLoop::Edges(oriented) = lp {
                oriented.reverse();
                for o in oriented.iter_mut() {
                    o.forward = !o.forward;
                }
            }
        }
    }
    shell
}

#[test]
fn a_box_ingests_as_a_validated_solid() {
    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &unit_box()).expect("box ingests");
    let report = crate::validate::validate_solid(&arena, solid).expect("validates");
    assert_eq!(
        (
            report.vertices,
            report.edges,
            report.faces,
            report.rings,
            report.shells,
            report.genus
        ),
        (8, 12, 6, 0, 1, 0)
    );
    // Exact: the ingested geometry is the file's own numbers, so a unit
    // box integrates to exactly 1 with no rounding to allow for.
    assert_eq!(geom::signed_volume(&arena, solid).unwrap(), 1.0);
}

#[test]
fn a_reversed_sense_stores_the_outward_normal_not_the_declared_one() {
    // The top face declares −z with `same_sense: false`. The arena's law
    // is that a face's stored normal points out of the material, so the
    // stored plane must be +z — and its POINT must still be the file's
    // own plane origin (plane fidelity: faces sharing one `PLANE` entity
    // keep identical bits).
    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &unit_box()).expect("box ingests");
    let shell = arena.shell(arena.solid(solid).unwrap().shells[0]).unwrap();
    let top = shell.faces[1];
    let Some(Surface::Plane(plane)) = arena.face(top).unwrap().surface else {
        panic!("planar face expected");
    };
    assert_eq!(
        (plane.normal.x, plane.normal.y, plane.normal.z),
        (0.0, 0.0, 1.0)
    );
    assert_eq!(plane.point, v(0.0, 0.0, 1.0));
}

#[test]
fn an_unsupported_surface_is_a_typed_capability_refusal() {
    let mut shell = unit_box();
    shell.faces[0].surface = AnalyticSurface::Sphere {
        center: v(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::AnalyticIngestUnsupportedSurface {
            face: 0,
            surface: "spherical (C5b)",
        })
    );
}

#[test]
fn an_arc_that_does_not_bound_its_face_is_refused_by_the_winding() {
    // C4b admits an OPEN circle edge, so this box-with-an-arc — an arc of a
    // radius-1 circle about the origin pasted onto a unit box's edge, which
    // bounds none of the faces it is claimed by — no longer stops at the
    // vocabulary. It stops one step later, at the measurement that cannot be
    // argued with: with the circular segment's exact area included, no loop of
    // that face winds as an outer boundary. The refusal moved, it did not
    // weaken (spec §5.4 — never a flip to taste).
    let mut shell = unit_box();
    shell.edges[5].curve = AnalyticCurve::Circle {
        center: v(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
        interior: v(-1.0, 0.0, 0.0),
    };
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::InvalidAnalyticShell(
            "a face has no loop winding as its outer boundary (its declared sense contradicts \
             its own boundary)"
        ))
    );
}

#[test]
fn a_vertex_loop_is_a_typed_capability_refusal() {
    let mut shell = unit_box();
    shell.faces[2].loops.push(AnalyticLoop::Vertex(0));
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::AnalyticIngestUnsupported(
            "a face boundary is a vertex loop"
        ))
    );
}

#[test]
fn a_face_sense_contradicting_its_boundary_is_refused_not_flipped() {
    // Flip ONE face's sense and leave its loop alone: the face's only
    // loop now winds as a ring, so the face has no outer boundary. The
    // arena could be made to accept this by reversing the loop — exactly
    // the repair SI5 refuses to make (spec §5.4), because the identical
    // disagreement on a curved face is unresolvable and a silent flip
    // there inverts a bore wall.
    let mut shell = unit_box();
    shell.faces[0].same_sense = false;
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::InvalidAnalyticShell(
            "a face has no loop winding as its outer boundary (its declared sense \
                     contradicts its own boundary)"
        ))
    );
}

#[test]
fn an_inward_oriented_shell_is_refused_by_the_volume_gate() {
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &inverted(unit_box())),
        Err(KernelV2Error::InvalidAnalyticShell(
            "shell is inward-oriented (a void, or inverted face senses)"
        ))
    );
}

#[test]
fn an_unpaired_edge_is_refused() {
    let mut shell = unit_box();
    shell.faces.remove(1); // the top: its four edges now have one use
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::InvalidAnalyticShell(
            "an edge is not used by exactly two oriented edges"
        ))
    );
}

#[test]
fn a_discontinuous_loop_is_refused() {
    let mut shell = unit_box();
    let AnalyticLoop::Edges(oriented) = &mut shell.faces[0].loops[0] else {
        panic!("edge loop expected");
    };
    oriented.swap(1, 2);
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::InvalidAnalyticShell(
            "a loop is not edge-continuous in the direction the file declares"
        ))
    );
}

#[test]
fn the_on_surface_gate_admits_a_real_writer_residual_and_refuses_a_defect() {
    // Spec §2.2 measured real writers at ~1e-13 m against a 1e-9 band
    // over 6.18 M (face, vertex) incidences, so the gate must pass the
    // former and wall the latter. Vertex 6 is on the top, +x and +y
    // faces; nudging it along z leaves the two vertical planes exact and
    // takes the top face off its own plane.
    for (dz, want) in [(1e-13, true), (1e-6, false)] {
        let mut shell = unit_box();
        shell.vertices[6] = v(1.0, 1.0, 1.0 + dz);
        let mut arena = BrepArena::new();
        let got = ingest_analytic(&mut arena, &shell);
        assert_eq!(
            got.is_ok(),
            want,
            "dz = {dz:e} should {} — got {got:?}",
            if want { "ingest" } else { "be refused" }
        );
        if !want {
            assert_eq!(
                got,
                Err(KernelV2Error::AnalyticVertexOffSurface { face: 1 })
            );
        }
    }
}

#[test]
fn a_disconnected_shell_is_refused() {
    // Two boxes in one `CLOSED_SHELL`: structurally sound, every edge
    // paired, but ISO 10303-42 requires a closed shell to be connected
    // and the two halves are really separate bodies.
    let a = unit_box();
    let mut shell = a.clone();
    let (nv, ne) = (a.vertices.len() as u32, a.edges.len() as u32);
    shell
        .vertices
        .extend(a.vertices.iter().map(|p| v(p.x() + 3.0, p.y(), p.z())));
    shell.edges.extend(a.edges.iter().map(|e| AnalyticEdge {
        start: e.start + nv,
        end: e.end + nv,
        curve: e.curve,
    }));
    shell.faces.extend(a.faces.iter().map(|f| {
        let AnalyticSurface::Plane { origin, normal } = f.surface else {
            unreachable!("box faces are planes");
        };
        AnalyticFace {
            surface: AnalyticSurface::Plane {
                origin: v(origin.x() + 3.0, origin.y(), origin.z()),
                normal,
            },
            loops: f
                .loops
                .iter()
                .map(|lp| match lp {
                    AnalyticLoop::Edges(os) => {
                        AnalyticLoop::Edges(os.iter().map(|o| oe(o.edge + ne, o.forward)).collect())
                    }
                    AnalyticLoop::Vertex(i) => AnalyticLoop::Vertex(i + nv),
                })
                .collect(),
            same_sense: f.same_sense,
        }
    }));
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::InvalidAnalyticShell(
            "shell is not connected"
        ))
    );
}

#[test]
fn an_empty_shell_is_refused() {
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &AnalyticShellData::default()),
        Err(KernelV2Error::InvalidAnalyticShell("shell has no faces"))
    );
}

// ---------------------------------------------------------------------------
// C4a — full curved bands
// ---------------------------------------------------------------------------

const Z: Vector3 = Vector3::new(0.0, 0.0, 1.0);

/// A z-axis cylinder of `radius` from `z = 0` to `z = h`, written the way a
/// real file writes it: TWO single-closed-circle loops on the lateral (no seam
/// anywhere in the file), one on each cap. `azimuth` is where the TOP rim's
/// anchor sits — the misalignment C4a has to resolve by re-anchoring.
fn cylinder(radius: f64, h: f64, azimuth: f64) -> AnalyticShellData {
    AnalyticShellData {
        vertices: vec![
            v(radius, 0.0, 0.0),
            v(radius * azimuth.cos(), radius * azimuth.sin(), h),
        ],
        edges: vec![
            rim_edge(0, v(0.0, 0.0, 0.0), Z, radius, v(-radius, 0.0, 0.0)),
            rim_edge(1, v(0.0, 0.0, h), Z, radius, v(-radius, 0.0, h)),
        ],
        faces: vec![
            // The lateral, two rim loops, surface normal pointing away from
            // the axis — which for a solid IS the outward direction.
            AnalyticFace {
                surface: AnalyticSurface::Cylinder {
                    axis_point: v(0.0, 0.0, 0.0),
                    axis_dir: Z,
                    radius,
                },
                loops: vec![
                    AnalyticLoop::Edges(vec![oe(0, true)]),
                    AnalyticLoop::Edges(vec![oe(1, true)]),
                ],
                same_sense: true,
            },
            // Bottom cap: plane z = 0 declared +z, outward is −z.
            AnalyticFace {
                surface: AnalyticSurface::Plane {
                    origin: v(0.0, 0.0, 0.0),
                    normal: Z,
                },
                loops: vec![AnalyticLoop::Edges(vec![oe(0, false)])],
                same_sense: false,
            },
            // Top cap: plane z = h, outward +z.
            AnalyticFace {
                surface: AnalyticSurface::Plane {
                    origin: v(0.0, 0.0, h),
                    normal: Z,
                },
                loops: vec![AnalyticLoop::Edges(vec![oe(1, false)])],
                same_sense: true,
            },
        ],
    }
}

#[test]
fn a_cylinder_ingests_into_strouds_single_fake_edge_form() {
    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &cylinder(2.0, 3.0, 0.0)).expect("cylinder ingests");
    let report = crate::validate::validate_solid(&arena, solid).expect("validates");
    // `arena.rs` module docs: a closed cylinder is V=2, E=3, F=3, R=0, S=1,
    // G=0 — the seam edge exists in the arena and nowhere in the file.
    assert_eq!(
        (
            report.vertices,
            report.edges,
            report.faces,
            report.rings,
            report.shells,
            report.genus
        ),
        (2, 3, 3, 0, 1, 0)
    );
    let vol = geom::signed_volume(&arena, solid).unwrap();
    assert!(
        (vol - PI * 4.0 * 3.0).abs() <= 1e-12 * vol,
        "volume {vol} should be pi r^2 h"
    );
}

#[test]
fn a_cylinder_whose_rim_anchors_disagree_is_re_anchored_not_refused() {
    // 17.6 % of corpus bands arrive with their two rim anchors at different
    // azimuths (p90 = pi/2, max = pi). The seam must be a ruling, so one rim
    // is re-anchored along its own circle — a gauge move, since the loop is
    // the whole circle either way.
    for azimuth in [0.0, 0.3, PI / 2.0, PI, -2.0] {
        let mut arena = BrepArena::new();
        let solid = ingest_analytic(&mut arena, &cylinder(2.0, 3.0, azimuth))
            .unwrap_or_else(|e| panic!("azimuth {azimuth} should ingest, got {e:?}"));
        crate::validate::validate_solid(&arena, solid).expect("validates");
        let vol = geom::signed_volume(&arena, solid).unwrap();
        assert!((vol - PI * 4.0 * 3.0).abs() <= 1e-12 * vol);
        // The re-anchored vertex is still exactly on its own rim circle.
        let shell = arena.shell(arena.solid(solid).unwrap().shells[0]).unwrap();
        for &f in &shell.faces {
            for p in arena
                .loop_points(arena.face(f).unwrap().outer_loop)
                .unwrap()
            {
                assert!(
                    ((p.x() * p.x() + p.y() * p.y()).sqrt() - 2.0).abs() < 1e-12,
                    "vertex {p:?} left its circle"
                );
            }
        }
    }
}

#[test]
fn a_bore_wall_ingests_with_the_cavity_sense_not_a_flipped_axis() {
    // `same_sense: false` on a cylindrical face is a bore: the arena records
    // it in `Surface::Cylinder::reversed`, never by negating the axis. Build
    // a block with a through hole — the bore's rims are RINGS of the two
    // planar faces, which is the orientation case a cap cannot exercise.
    let shell = drilled_block();
    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &shell).expect("drilled block ingests");
    let report = crate::validate::validate_solid(&arena, solid).expect("validates");
    // 8 box corners + 2 rim anchors; 12 box edges + 2 rims + 1 seam;
    // 6 planes + 1 bore; 2 rings; genus 1.
    assert_eq!(
        (
            report.vertices,
            report.edges,
            report.faces,
            report.rings,
            report.shells,
            report.genus
        ),
        (10, 15, 7, 2, 1, 1)
    );
    let bore = arena
        .shell(arena.solid(solid).unwrap().shells[0])
        .unwrap()
        .faces
        .iter()
        .find_map(|&f| match arena.face(f).unwrap().surface {
            Some(Surface::Cylinder {
                axis_dir,
                reversed,
                radius,
                ..
            }) => Some((axis_dir, reversed, radius)),
            _ => None,
        })
        .expect("a cylindrical face");
    assert!(bore.1, "a bore wall is the cavity sense");
    assert_eq!(
        (bore.0.x, bore.0.y, bore.0.z),
        (0.0, 0.0, 1.0),
        "the axis keeps the file's own direction"
    );
    let vol = geom::signed_volume(&arena, solid).unwrap();
    let want = 1.0 - PI * 0.2 * 0.2;
    assert!((vol - want).abs() <= 1e-12, "volume {vol} vs {want}");
}

/// The unit box with a `r = 0.2` through hole on the z axis: the box's top and
/// bottom faces gain a circular RING, and the bore wall is a cylindrical face
/// with `same_sense: false`.
fn drilled_block() -> AnalyticShellData {
    let mut shell = unit_box();
    let r = 0.2;
    let (c, s) = (0.5, 0.5);
    shell.vertices.push(v(c + r, s, 0.0)); // 8: bottom rim anchor
    shell.vertices.push(v(c + r, s, 1.0)); // 9: top rim anchor
    shell
        .edges
        .push(rim_edge(8, v(c, s, 0.0), Z, r, v(c - r, s, 0.0))); // 12
    shell
        .edges
        .push(rim_edge(9, v(c, s, 1.0), Z, r, v(c - r, s, 1.0))); // 13
                                                                  // Rings on the two caps. Orientation is NOT set here: the ingest path
                                                                  // derives each rim's traversal from the bore's material sense and gives
                                                                  // the planar side the negation, so the file's flags only have to be
                                                                  // consistent (which pass 1f checks).
    shell.faces[0]
        .loops
        .push(AnalyticLoop::Edges(vec![oe(12, true)]));
    shell.faces[1]
        .loops
        .push(AnalyticLoop::Edges(vec![oe(13, true)]));
    shell.faces.push(AnalyticFace {
        surface: AnalyticSurface::Cylinder {
            axis_point: v(c, s, 0.0),
            axis_dir: Z,
            radius: r,
        },
        loops: vec![
            AnalyticLoop::Edges(vec![oe(12, false)]),
            AnalyticLoop::Edges(vec![oe(13, false)]),
        ],
        same_sense: false,
    });
    shell
}

#[test]
fn a_cone_frustum_ingests_with_its_rims_at_their_own_radii() {
    // apex at the origin, half-angle 45°, so the rim at z = t has radius t.
    let half = PI / 4.0;
    let (r0, r1, z0, z1) = (1.0, 2.0, 1.0, 2.0);
    let shell = AnalyticShellData {
        vertices: vec![v(r0, 0.0, z0), v(r1, 0.0, z1)],
        edges: vec![
            rim_edge(0, v(0.0, 0.0, z0), Z, r0, v(-r0, 0.0, z0)),
            rim_edge(1, v(0.0, 0.0, z1), Z, r1, v(-r1, 0.0, z1)),
        ],
        faces: vec![
            AnalyticFace {
                surface: AnalyticSurface::Cone {
                    apex: v(0.0, 0.0, 0.0),
                    axis_dir: Z,
                    half_angle: half,
                },
                loops: vec![
                    AnalyticLoop::Edges(vec![oe(0, true)]),
                    AnalyticLoop::Edges(vec![oe(1, true)]),
                ],
                same_sense: true,
            },
            AnalyticFace {
                surface: AnalyticSurface::Plane {
                    origin: v(0.0, 0.0, z0),
                    normal: Z,
                },
                loops: vec![AnalyticLoop::Edges(vec![oe(0, false)])],
                same_sense: false,
            },
            AnalyticFace {
                surface: AnalyticSurface::Plane {
                    origin: v(0.0, 0.0, z1),
                    normal: Z,
                },
                loops: vec![AnalyticLoop::Edges(vec![oe(1, false)])],
                same_sense: true,
            },
        ],
    };
    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &shell).expect("frustum ingests");
    crate::validate::validate_solid(&arena, solid).expect("validates");
    let vol = geom::signed_volume(&arena, solid).unwrap();
    // A frustum: (pi h / 3)(R^2 + Rr + r^2).
    let want = PI * (z1 - z0) / 3.0 * (r1 * r1 + r1 * r0 + r0 * r0);
    assert!((vol - want).abs() <= 1e-12 * want, "volume {vol} vs {want}");
}

#[test]
fn an_unclosed_band_is_still_a_typed_refusal() {
    // Drop one rim loop from the lateral: ONE closed rim and nothing else. C4b
    // admits patches bounded by OPEN edges, so this stays what it always was —
    // a band that does not close, a shape the arena has no face for and that no
    // choice of seam repairs.
    let mut shell = cylinder(2.0, 3.0, 0.0);
    shell.faces[0].loops.pop();
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::AnalyticIngestUnsupported(
            "a curved face is neither a full band of two closed rims nor a patch of open edges \
             (an unclosed or holed band)"
        ))
    );
}

#[test]
fn a_rim_that_bounds_no_curved_face_is_refused() {
    // Two planar faces sharing a full circle would be two faces of one plane,
    // and nothing would fix the circle's traversal sense. Swap the lateral
    // for a plane and the shell says exactly that.
    let mut shell = cylinder(2.0, 3.0, 0.0);
    shell.faces[0].surface = AnalyticSurface::Plane {
        origin: v(0.0, 0.0, 0.0),
        normal: Z,
    };
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::InvalidAnalyticShell(
            "a full-circle edge bounds no curved face"
        ))
    );
}

#[test]
fn a_band_whose_rims_are_not_coaxial_is_refused() {
    // Shift the top rim's centre off the axis: the file's two rims are not
    // cross-sections of one cylinder, which no amount of seam choice fixes.
    let mut shell = cylinder(2.0, 3.0, 0.0);
    shell.edges[1].curve = AnalyticCurve::Circle {
        center: v(0.5, 0.0, 3.0),
        normal: Z,
        radius: 2.0,
        interior: v(-1.5, 0.0, 3.0),
    };
    let mut arena = BrepArena::new();
    assert!(matches!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::InvalidAnalyticShell(_))
    ));
}

/// Give a rim's anchor vertex a second owner, so it may not be re-anchored.
fn pin_anchor(shell: &mut AnalyticShellData, vertex: u32) {
    let far = shell.vertices.len() as u32;
    shell.vertices.push(v(5.0, 5.0, 5.0 + far as f64));
    shell.edges.push(edge(vertex, far));
}

#[test]
fn one_pinned_rim_becomes_the_components_seam_reference() {
    // A pinned anchor cannot move, so the seam azimuth is chosen to BE its
    // azimuth and the free rim comes to it — the reason the reference is
    // picked pinned-first rather than by index.
    let mut shell = cylinder(2.0, 3.0, PI / 2.0);
    pin_anchor(&mut shell, 1);
    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &shell).expect("the free rim re-anchors instead");
    crate::validate::validate_solid(&arena, solid).expect("validates");
    // The pinned vertex kept the file's own bits; the free one moved to it.
    assert_eq!(arena.vertex(VertexId(1)).unwrap().point, shell.vertices[1]);
    let moved = arena.vertex(VertexId(0)).unwrap().point;
    assert!(
        moved.x().abs() < 1e-15 && (moved.y() - 2.0).abs() < 1e-15,
        "the free anchor should have slid to the pinned rim's azimuth, got {moved:?}"
    );
}

#[test]
fn two_pinned_rims_at_different_azimuths_are_refused_not_forced() {
    // The gauge argument for sliding an anchor along its own circle only holds
    // while nothing else depends on where it is. With both anchors owned
    // elsewhere and no shared azimuth, there is no seam to mint and C4a says so
    // rather than moving a vertex somebody's boundary depends on.
    let mut shell = cylinder(2.0, 3.0, PI / 2.0);
    pin_anchor(&mut shell, 0);
    pin_anchor(&mut shell, 1);
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::AnalyticIngestUnsupported(
            "a rim needing a re-anchored seam shares its anchor vertex with another edge"
        ))
    );
}

#[test]
fn two_pinned_rims_that_already_agree_still_ingest() {
    // The refusal above must be about the disagreement, not about pinning.
    let mut shell = cylinder(2.0, 3.0, 0.0);
    pin_anchor(&mut shell, 0);
    pin_anchor(&mut shell, 1);
    let mut arena = BrepArena::new();
    ingest_analytic(&mut arena, &shell).expect("aligned pinned anchors need no move");
}

#[test]
fn an_ingested_cylinder_is_boolean_eligible() {
    // The point of C4a: a band ingested in the canonical form presents
    // exactly the two full-circle rims `to_yang_brep` requires, so the
    // imported body can take part in booleans — unlike the mesh-backed tier
    // and unlike a partial patch.
    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &cylinder(1.0, 2.0, 0.0)).expect("ingests");
    crate::boolean::to_yang_brep(&arena, solid).expect("converts for the boolean pipeline");
}

// ---------------------------------------------------------------------------
// C4b — the arc-patch tier (spec `si5_c4b_arc_patch_tier.md`)
// ---------------------------------------------------------------------------

/// An arc edge: one `EDGE_CURVE` between two DISTINCT vertices, whose side is
/// pinned by the file's own `interior` point (never derived from the endpoints).
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

/// A HALF-ROUND: a cylinder of `radius` and height `h` cut by the plane y = 0,
/// keeping y ≥ 0. Four vertices, six edges, four faces — and it is the corpus's
/// own arc-patch shape:
///
/// - the lateral is the `CCLL` form §5.1 measured as 58.4 % of curved faces:
///   two OPEN arcs and two DISTINCT rulings, no seam anywhere;
/// - the two caps are planar faces bounded by an arc and a chord — a 2-edge
///   loop, which bounds area only because one edge is curved;
/// - the cut face is an ordinary rectangle.
///
/// Its volume is π r² h / 2 exactly, which makes it a closed-form oracle, and a
/// half turn is the sweep the boolean path has to refuse as ambiguous — so this
/// fixture also pins that reading `interior` escapes that limit.
fn half_round(radius: f64, h: f64) -> AnalyticShellData {
    let (a, b, c, d) = (0u32, 1u32, 2u32, 3u32); // +r bottom, −r bottom, +r top, −r top
    AnalyticShellData {
        vertices: vec![
            v(radius, 0.0, 0.0),
            v(-radius, 0.0, 0.0),
            v(radius, 0.0, h),
            v(-radius, 0.0, h),
        ],
        edges: vec![
            // 0: bottom arc A → B through +y
            arc_edge(a, b, v(0.0, 0.0, 0.0), Z, radius, v(0.0, radius, 0.0)),
            // 1: bottom chord B → A
            edge(b, a),
            // 2: top arc C → D through +y
            arc_edge(c, d, v(0.0, 0.0, h), Z, radius, v(0.0, radius, h)),
            // 3: top chord D → C
            edge(d, c),
            // 4: ruling at +r, A → C
            edge(a, c),
            // 5: ruling at −r, B → D
            edge(b, d),
        ],
        faces: vec![
            // The arc patch: bottom arc (increasing azimuth, i.e. toward the
            // top), up the −r ruling, top arc back, down the +r ruling.
            AnalyticFace {
                surface: AnalyticSurface::Cylinder {
                    axis_point: v(0.0, 0.0, 0.0),
                    axis_dir: Z,
                    radius,
                },
                loops: vec![AnalyticLoop::Edges(vec![
                    oe(0, true),
                    oe(5, true),
                    oe(2, false),
                    oe(4, false),
                ])],
                same_sense: true,
            },
            // Bottom half-disc: plane z = 0 declared +z, outward is −z, so the
            // loop runs chord then arc.
            plane_face(v(0.0, 0.0, 0.0), Z, false, vec![oe(1, false), oe(0, false)]),
            // Top half-disc: outward +z — arc then chord.
            plane_face(v(0.0, 0.0, h), Z, true, vec![oe(2, true), oe(3, true)]),
            // The cut face: plane y = 0, outward −y.
            plane_face(
                v(0.0, 0.0, 0.0),
                Vector3::new(0.0, 1.0, 0.0),
                false,
                vec![oe(4, true), oe(3, false), oe(5, false), oe(1, true)],
            ),
        ],
    }
}

#[test]
fn a_half_round_ingests_as_an_arc_patch_with_its_exact_volume() {
    let (radius, h) = (2.0, 3.0);
    let mut arena = BrepArena::new();
    let solid =
        ingest_analytic(&mut arena, &half_round(radius, h)).expect("the half-round ingests");
    let report = crate::validate::validate_solid(&arena, solid).expect("validates");
    // Nothing is minted: the file's own four vertices and six edges ARE the
    // arena's, unlike a full band where a seam appears from nowhere.
    assert_eq!(
        (
            report.vertices,
            report.edges,
            report.faces,
            report.rings,
            report.shells,
            report.genus
        ),
        (4, 6, 4, 0, 1, 0)
    );
    let vol = geom::signed_volume(&arena, solid).unwrap();
    let want = PI * radius * radius * h / 2.0;
    assert!(
        (vol - want).abs() <= 1e-12 * want,
        "half-round volume {vol} vs {want}"
    );
}

#[test]
fn an_arcs_side_comes_from_the_files_interior_point_not_its_endpoints() {
    // The half-round's arcs are HALF turns, where the two endpoints alone are
    // genuinely undecidable — which is exactly why `from_yang_brep` refuses a
    // near-half arc rather than guessing. Move each `interior` to the far side
    // and mirror the document with it (a reflection reverses every loop): the
    // result is the OTHER half, on the same four vertices, the same two circles
    // and the same six edges. A reading that derived an arc's side from its
    // endpoints could not tell these two files apart, so it would build one of
    // them inside out. The oracle is the tessellated body's own extent, since
    // both halves have identical volume.
    let (radius, h) = (2.0, 3.0);
    let mut flipped = half_round(radius, h);
    for (ei, far) in [(0usize, v(0.0, -radius, 0.0)), (2usize, v(0.0, -radius, h))] {
        let AnalyticCurve::Circle {
            center,
            normal,
            radius: r,
            ..
        } = flipped.edges[ei].curve
        else {
            unreachable!("arc edge")
        };
        flipped.edges[ei].curve = AnalyticCurve::Circle {
            center,
            normal,
            radius: r,
            interior: far,
        };
    }
    // Every loop reversed, and the cut face's outward normal is now +y.
    flipped.faces[0].loops = vec![AnalyticLoop::Edges(vec![
        oe(4, true),
        oe(2, true),
        oe(5, false),
        oe(0, false),
    ])];
    flipped.faces[1].loops = vec![AnalyticLoop::Edges(vec![oe(0, true), oe(1, true)])];
    flipped.faces[2].loops = vec![AnalyticLoop::Edges(vec![oe(3, false), oe(2, false)])];
    flipped.faces[3].same_sense = true;
    flipped.faces[3].loops = vec![AnalyticLoop::Edges(vec![
        oe(1, false),
        oe(5, true),
        oe(3, true),
        oe(4, false),
    ])];

    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &flipped).expect("the mirrored half-round ingests");
    let vol = geom::signed_volume(&arena, solid).unwrap();
    let want = PI * radius * radius * h / 2.0;
    assert!(
        (vol - want).abs() <= 1e-12 * want,
        "mirrored half-round volume {vol} vs {want}"
    );

    // The bodies really are different halves, and only the render mesh can say
    // so: the file's four vertices all sit at y = 0, so the discriminator is the
    // sampled arc.
    let extent = |s| {
        let mesh = crate::tessellate::tessellate(&arena, s).expect("tessellates");
        mesh.positions
            .iter()
            .skip(1)
            .step_by(3)
            .fold((f64::MAX, f64::MIN), |(lo, hi), y| (lo.min(*y), hi.max(*y)))
    };
    let (lo, hi) = extent(solid);
    assert!(
        hi <= 1e-9 && lo < -radius * 0.9,
        "the mirrored body should occupy y <= 0, got y in [{lo}, {hi}]"
    );

    let mut original = BrepArena::new();
    let o = ingest_analytic(&mut original, &half_round(radius, h)).expect("ingests");
    let omesh = crate::tessellate::tessellate(&original, o).expect("tessellates");
    let (olo, ohi) = omesh
        .positions
        .iter()
        .skip(1)
        .step_by(3)
        .fold((f64::MAX, f64::MIN), |(lo, hi), y| (lo.min(*y), hi.max(*y)));
    assert!(
        olo >= -1e-9 && ohi > radius * 0.9,
        "the original body should occupy y >= 0, got y in [{olo}, {ohi}]"
    );
}

#[test]
fn a_closed_ellipse_edge_is_a_typed_refusal_naming_the_tier() {
    // An open ellipse arc is C4b's; a CLOSED one is rim-like, and no interior
    // point can settle the sense of a curve that passes through all its own
    // points both ways.
    let mut shell = half_round(2.0, 3.0);
    shell.edges.push(AnalyticEdge {
        start: 0,
        end: 0,
        curve: AnalyticCurve::Ellipse {
            center: v(0.0, 0.0, 0.0),
            normal: Z,
            major_axis: Vector3::new(1.0, 0.0, 0.0),
            major_radius: 3.0,
            minor_radius: 2.0,
            interior: v(-3.0, 0.0, 0.0),
        },
    });
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::AnalyticIngestUnsupportedCurve {
            edge: 6,
            curve: "closed ELLIPSE edge (C4b takes open ellipse arcs)",
        })
    );
}

#[test]
fn a_windowed_curved_patch_is_refused_by_name() {
    // Measured at 2 of 1 113 corpus arc patches (spec §5.2): a second boundary
    // loop needs outer-loop ranking in the unrolled (θ, h) domain, which is NOT
    // built — so the wall names it instead of a guess.
    let mut shell = half_round(2.0, 3.0);
    let extra = AnalyticLoop::Edges(vec![oe(0, true), oe(5, true), oe(2, false), oe(4, false)]);
    shell.faces[0].loops.push(extra);
    let mut arena = BrepArena::new();
    assert_eq!(
        ingest_analytic(&mut arena, &shell),
        Err(KernelV2Error::AnalyticIngestUnsupported(
            "a curved patch has more than one boundary loop (C4b: unrolled-domain outer-loop \
             ranking)"
        ))
    );
}

// ---------------------------------------------------------------------------
// C5a — the torus latitude band (spec `si5_c5_sphere_torus_tier.md`)
// ---------------------------------------------------------------------------

/// A ROUNDED PUCK: a cylinder of radius `rc` and height `h` whose top edge is
/// filleted at radius `rho` — bottom disc, cylinder band, a torus LATITUDE
/// band (the quarter round, major `rc − rho`, minor `rho`, centre at
/// `z = h − rho`), and a top disc of radius `rc − rho`. The corpus's own
/// fillet form (62 of 183 torus faces, 21 of 31 models), which no kernel-v2
/// constructor builds. Its volume is closed-form by Pappus:
/// `π rc² (h − ρ) + π R² ρ + π² ρ² R / 2 + 2π ρ³ / 3` with `R = rc − ρ`, and
/// the torus centre is off the origin so the flux's `C·â` term is exercised.
fn rounded_puck(rc: f64, h: f64, rho: f64) -> AnalyticShellData {
    let big = rc - rho;
    AnalyticShellData {
        vertices: vec![v(rc, 0.0, 0.0), v(rc, 0.0, h - rho), v(big, 0.0, h)],
        edges: vec![
            rim_edge(0, v(0.0, 0.0, 0.0), Z, rc, v(-rc, 0.0, 0.0)),
            rim_edge(1, v(0.0, 0.0, h - rho), Z, rc, v(-rc, 0.0, h - rho)),
            rim_edge(2, v(0.0, 0.0, h), Z, big, v(-big, 0.0, h)),
        ],
        faces: vec![
            plane_face(v(0.0, 0.0, 0.0), Z, false, vec![oe(0, false)]),
            AnalyticFace {
                surface: AnalyticSurface::Cylinder {
                    axis_point: v(0.0, 0.0, 0.0),
                    axis_dir: Z,
                    radius: rc,
                },
                loops: vec![
                    AnalyticLoop::Edges(vec![oe(0, true)]),
                    AnalyticLoop::Edges(vec![oe(1, true)]),
                ],
                same_sense: true,
            },
            // The fillet: material INSIDE the tube (a convex edge), so the
            // torus's own outward normal is the solid's.
            AnalyticFace {
                surface: AnalyticSurface::Torus {
                    center: v(0.0, 0.0, h - rho),
                    axis_dir: Z,
                    major_radius: big,
                    minor_radius: rho,
                },
                loops: vec![
                    AnalyticLoop::Edges(vec![oe(1, false)]),
                    AnalyticLoop::Edges(vec![oe(2, true)]),
                ],
                same_sense: true,
            },
            plane_face(v(0.0, 0.0, h), Z, true, vec![oe(2, false)]),
        ],
    }
}

fn rounded_puck_volume(rc: f64, h: f64, rho: f64) -> f64 {
    let big = rc - rho;
    PI * rc * rc * (h - rho)
        + PI * big * big * rho
        + PI * PI * rho * rho * big / 2.0
        + 2.0 * PI * rho * rho * rho / 3.0
}

/// Divergence-theorem volume of a render mesh, for the free differential
/// oracle between the closed-form `signed_volume` and the tessellator.
fn mesh_volume(mesh: &crate::tessellate::RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = i as usize * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let mut six = 0.0;
    for t in mesh.indices.chunks(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six / 6.0
}

#[test]
fn a_rounded_puck_ingests_with_its_fillet_as_a_torus_latitude_band() {
    let (rc, h, rho) = (3.0, 5.0, 0.75);
    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &rounded_puck(rc, h, rho)).expect("the puck ingests");
    let report = crate::validate::validate_solid(&arena, solid).expect("validates");
    // Three anchors, three rims plus TWO minted seams (one per band — the
    // torus's is a poloidal arc), four faces.
    assert_eq!(
        (
            report.vertices,
            report.edges,
            report.faces,
            report.rings,
            report.shells,
            report.genus
        ),
        (3, 5, 4, 0, 1, 0)
    );
    let vol = geom::signed_volume(&arena, solid).unwrap();
    let want = rounded_puck_volume(rc, h, rho);
    assert!(
        (vol - want).abs() <= 1e-12 * want,
        "puck volume {vol:.17e} vs Pappus {want:.17e}"
    );
    // The minted seam lies ON the face: a poloidal arc of the tube radius,
    // centred on the tube's centre circle at the rims' azimuth.
    let seam = arena
        .half_edges
        .iter()
        .flatten()
        .find_map(|he| match he.curve {
            Curve::Arc { center, radius, .. } => Some((center, radius)),
            _ => None,
        })
        .expect("a seam arc");
    assert_eq!(seam, (v(rc - rho, 0.0, h - rho), rho));
    // The tessellator sees the same region the volume term does.
    let mesh = crate::tessellate::tessellate(&arena, solid).expect("tessellates");
    let mv = mesh_volume(&mesh);
    assert!(
        (mv - want).abs() <= 3e-3 * want,
        "mesh volume {mv} vs {want} — beyond the chord-error bound"
    );
}

#[test]
fn a_torus_bands_sense_is_propagated_from_its_neighbour_not_read_from_the_file() {
    // Flip EVERY rim flag. The census measured the file's flag wrong on 3.9 %
    // of rims where the truth is known (spec §2.2); a reading that consulted it
    // would build the three-quarter round here and get the wrong volume.
    let (rc, h, rho) = (3.0, 5.0, 0.75);
    let mut shell = rounded_puck(rc, h, rho);
    for f in &mut shell.faces {
        for l in &mut f.loops {
            if let AnalyticLoop::Edges(os) = l {
                for o in os {
                    o.forward = !o.forward;
                }
            }
        }
    }
    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &shell).expect("flags are not read");
    let vol = geom::signed_volume(&arena, solid).unwrap();
    let want = rounded_puck_volume(rc, h, rho);
    assert!((vol - want).abs() <= 1e-12 * want, "{vol} vs {want}");
}

/// A BOSS ON A PLATE with the concave fillet at its base: the material is
/// OUTSIDE the tube (`same_sense: false` ⇒ `reversed`), the plate's top face
/// is an annulus whose ring is the fillet's base rim, and the fillet's sense
/// is seeded by the boss band. Volume by Pappus: the plate, the boss, and the
/// fillet's square-minus-quarter-disc section revolved —
/// `2π [ρ²(rb + ρ/2) − (πρ²/4)(rb + ρ) + ρ³/3]`.
fn boss_on_plate(rp: f64, t: f64, rb: f64, hb: f64, rho: f64) -> AnalyticShellData {
    let big = rb + rho;
    AnalyticShellData {
        vertices: vec![
            v(rp, 0.0, 0.0),
            v(rp, 0.0, t),
            v(big, 0.0, t),
            v(rb, 0.0, t + rho),
            v(rb, 0.0, t + hb),
        ],
        edges: vec![
            rim_edge(0, v(0.0, 0.0, 0.0), Z, rp, v(-rp, 0.0, 0.0)),
            rim_edge(1, v(0.0, 0.0, t), Z, rp, v(-rp, 0.0, t)),
            rim_edge(2, v(0.0, 0.0, t), Z, big, v(-big, 0.0, t)),
            rim_edge(3, v(0.0, 0.0, t + rho), Z, rb, v(-rb, 0.0, t + rho)),
            rim_edge(4, v(0.0, 0.0, t + hb), Z, rb, v(-rb, 0.0, t + hb)),
        ],
        faces: vec![
            plane_face(v(0.0, 0.0, 0.0), Z, false, vec![oe(0, false)]),
            AnalyticFace {
                surface: AnalyticSurface::Cylinder {
                    axis_point: v(0.0, 0.0, 0.0),
                    axis_dir: Z,
                    radius: rp,
                },
                loops: vec![
                    AnalyticLoop::Edges(vec![oe(0, true)]),
                    AnalyticLoop::Edges(vec![oe(1, true)]),
                ],
                same_sense: true,
            },
            // Plate top: an annulus, ring first (the file order the reader
            // loses the outer marker of — spec §5.4).
            AnalyticFace {
                surface: AnalyticSurface::Plane {
                    origin: v(0.0, 0.0, t),
                    normal: Z,
                },
                loops: vec![
                    AnalyticLoop::Edges(vec![oe(2, true)]),
                    AnalyticLoop::Edges(vec![oe(1, true)]),
                ],
                same_sense: true,
            },
            AnalyticFace {
                surface: AnalyticSurface::Torus {
                    center: v(0.0, 0.0, t + rho),
                    axis_dir: Z,
                    major_radius: big,
                    minor_radius: rho,
                },
                loops: vec![
                    AnalyticLoop::Edges(vec![oe(2, true)]),
                    AnalyticLoop::Edges(vec![oe(3, true)]),
                ],
                same_sense: false,
            },
            AnalyticFace {
                surface: AnalyticSurface::Cylinder {
                    axis_point: v(0.0, 0.0, 0.0),
                    axis_dir: Z,
                    radius: rb,
                },
                loops: vec![
                    AnalyticLoop::Edges(vec![oe(3, true)]),
                    AnalyticLoop::Edges(vec![oe(4, true)]),
                ],
                same_sense: true,
            },
            plane_face(v(0.0, 0.0, t + hb), Z, true, vec![oe(4, false)]),
        ],
    }
}

#[test]
fn a_concave_fillet_ingests_as_a_reversed_band_with_its_pappus_volume() {
    let (rp, t, rb, hb, rho) = (4.0, 1.0, 1.5, 2.0, 0.5);
    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &boss_on_plate(rp, t, rb, hb, rho))
        .expect("the boss on a plate ingests");
    let report = crate::validate::validate_solid(&arena, solid).expect("validates");
    assert_eq!(
        (
            report.vertices,
            report.edges,
            report.faces,
            report.rings,
            report.shells,
            report.genus
        ),
        (5, 8, 6, 1, 1, 0)
    );
    let torus = arena
        .faces
        .iter()
        .flatten()
        .find_map(|f| match f.surface {
            Some(Surface::Torus { reversed, .. }) => Some(reversed),
            _ => None,
        })
        .expect("a torus face");
    assert!(torus, "the concave fillet is a cavity-sense band");
    let vol = geom::signed_volume(&arena, solid).unwrap();
    let want = PI * rp * rp * t
        + PI * rb * rb * hb
        + 2.0
            * PI
            * (rho * rho * (rb + rho / 2.0) - (PI * rho * rho / 4.0) * (rb + rho)
                + rho * rho * rho / 3.0);
    assert!(
        (vol - want).abs() <= 1e-12 * want,
        "boss-on-plate volume {vol:.17e} vs Pappus {want:.17e}"
    );
    let mesh = crate::tessellate::tessellate(&arena, solid).expect("tessellates");
    let mv = mesh_volume(&mesh);
    assert!(
        (mv - want).abs() <= 3e-3 * want,
        "mesh volume {mv} vs {want} — beyond the chord-error bound"
    );
}

#[test]
fn a_bead_between_two_planar_rims_is_refused_by_name_not_guessed() {
    // A half torus sitting on a plane: both rims lie on the one planar face,
    // and nothing in the component can say whether the band is the upper or
    // the lower half. Spec §2.2: the plane-seeded rule has no corpus customer.
    let (big, r) = (3.0, 1.0);
    let shell = AnalyticShellData {
        vertices: vec![v(big + r, 0.0, 0.0), v(big - r, 0.0, 0.0)],
        edges: vec![
            rim_edge(0, v(0.0, 0.0, 0.0), Z, big + r, v(-(big + r), 0.0, 0.0)),
            rim_edge(1, v(0.0, 0.0, 0.0), Z, big - r, v(-(big - r), 0.0, 0.0)),
        ],
        faces: vec![
            AnalyticFace {
                surface: AnalyticSurface::Torus {
                    center: v(0.0, 0.0, 0.0),
                    axis_dir: Z,
                    major_radius: big,
                    minor_radius: r,
                },
                loops: vec![
                    AnalyticLoop::Edges(vec![oe(0, true)]),
                    AnalyticLoop::Edges(vec![oe(1, true)]),
                ],
                same_sense: true,
            },
            AnalyticFace {
                surface: AnalyticSurface::Plane {
                    origin: v(0.0, 0.0, 0.0),
                    normal: Z,
                },
                loops: vec![
                    AnalyticLoop::Edges(vec![oe(0, false)]),
                    AnalyticLoop::Edges(vec![oe(1, true)]),
                ],
                same_sense: false,
            },
        ],
    };
    let mut arena = BrepArena::new();
    match ingest_analytic(&mut arena, &shell) {
        Err(KernelV2Error::AnalyticIngestUnsupported(r)) => {
            assert!(r.contains("plane-seeded sense"), "{r}")
        }
        other => panic!("expected the named C5 refusal, got {other:?}"),
    }
}

#[test]
fn neighbours_that_fix_the_same_sign_on_both_rims_are_a_refusal() {
    // The puck's top disc replaced by a cylinder of the top rim's radius
    // running DOWN into the solid, capped below: both faces across the torus's
    // rims now derive "+axis" for the torus, which no region of the band can
    // satisfy — the file's faces disagree about the band, and 1d says so.
    let (rc, h, rho) = (3.0, 5.0, 0.75);
    let big = rc - rho;
    let mut shell = rounded_puck(rc, h, rho);
    shell.faces.pop();
    shell.vertices.push(v(big, 0.0, h - rho));
    shell.edges.push(rim_edge(
        3,
        v(0.0, 0.0, h - rho),
        Z,
        big,
        v(-big, 0.0, h - rho),
    ));
    shell.faces.push(AnalyticFace {
        surface: AnalyticSurface::Cylinder {
            axis_point: v(0.0, 0.0, 0.0),
            axis_dir: Z,
            radius: big,
        },
        loops: vec![
            AnalyticLoop::Edges(vec![oe(2, true)]),
            AnalyticLoop::Edges(vec![oe(3, true)]),
        ],
        same_sense: true,
    });
    shell.faces.push(plane_face(
        v(0.0, 0.0, h - rho),
        Z,
        false,
        vec![oe(3, false)],
    ));
    let mut arena = BrepArena::new();
    match ingest_analytic(&mut arena, &shell) {
        Err(KernelV2Error::InvalidAnalyticShell(r)) => {
            assert!(r.contains("not opposite about its axis"), "{r}")
        }
        other => panic!("expected the sense-disagreement refusal, got {other:?}"),
    }
}

#[test]
fn a_torus_latitude_band_is_a_typed_boolean_wall_not_a_bent_tube() {
    // `to_yang_brep`'s structured torus arm matches (Circle, Arc, Circle, Arc)
    // — the bent tube's pattern — and the fillet band has the same pattern
    // with LATITUDE circles. Emitting it would re-enter Stage 1 as a tube:
    // the spec §2.1 silent-wrong, now a named refusal.
    let mut arena = BrepArena::new();
    let solid = ingest_analytic(&mut arena, &rounded_puck(3.0, 5.0, 0.75)).expect("ingests");
    match crate::boolean::to_yang_brep(&arena, solid) {
        Err(KernelV2Error::UnsupportedCurvedBoolean { reason, .. }) => {
            assert!(reason.contains("latitude circles"), "{reason}")
        }
        other => panic!("expected the typed C5a boolean wall, got {other:?}"),
    }
}

#[test]
fn a_torus_patch_is_a_typed_refusal_naming_c5b() {
    // A quarter of the puck's fillet band as a parameter rectangle — the
    // corpus's CCCC form — reaches the UV-CDT tessellator but has no volume
    // term yet; it is named, not guessed. (The shell is only the face.)
    let (rc, h, rho) = (3.0, 5.0, 0.75);
    let big = rc - rho;
    let shell = AnalyticShellData {
        vertices: vec![
            v(rc, 0.0, h - rho),
            v(0.0, rc, h - rho),
            v(0.0, big, h),
            v(big, 0.0, h),
        ],
        edges: vec![
            arc_edge(
                0,
                1,
                v(0.0, 0.0, h - rho),
                Z,
                rc,
                v(rc * 0.5f64.sqrt(), rc * 0.5f64.sqrt(), h - rho),
            ),
            arc_edge(
                1,
                2,
                v(0.0, big, h - rho),
                Vector3::new(1.0, 0.0, 0.0),
                rho,
                v(
                    0.0,
                    big + rho * 0.5f64.sqrt(),
                    h - rho + rho * 0.5f64.sqrt(),
                ),
            ),
            arc_edge(
                2,
                3,
                v(0.0, 0.0, h),
                Z,
                big,
                v(big * 0.5f64.sqrt(), big * 0.5f64.sqrt(), h),
            ),
            arc_edge(
                3,
                0,
                v(big, 0.0, h - rho),
                Vector3::new(0.0, 1.0, 0.0),
                rho,
                v(
                    big + rho * 0.5f64.sqrt(),
                    0.0,
                    h - rho + rho * 0.5f64.sqrt(),
                ),
            ),
        ],
        faces: vec![AnalyticFace {
            surface: AnalyticSurface::Torus {
                center: v(0.0, 0.0, h - rho),
                axis_dir: Z,
                major_radius: big,
                minor_radius: rho,
            },
            loops: vec![AnalyticLoop::Edges(vec![
                oe(0, true),
                oe(1, true),
                oe(2, true),
                oe(3, true),
            ])],
            same_sense: true,
        }],
    };
    let mut arena = BrepArena::new();
    match ingest_analytic(&mut arena, &shell) {
        Err(KernelV2Error::AnalyticIngestUnsupported(r)) => assert!(r.contains("C5b"), "{r}"),
        other => panic!("expected the named C5b refusal, got {other:?}"),
    }
}
