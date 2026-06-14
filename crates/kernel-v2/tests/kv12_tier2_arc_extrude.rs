//! PR-KV12 Tier 2, increments E1 + E2 — exact mixed line/arc extrude.
//!
//! `extrude` of a `ProfileRegion::ArcPolygon` (a closed loop of line + minor
//! circular-arc edges) produces an EXACT B-Rep: planar caps bounded by the
//! line+arc loop, a planar side wall per line edge, and a `Surface::Cylinder`
//! patch per arc edge (an arc swept perpendicular to its plane IS a cylinder
//! lateral). Replaces the KV12 Tier-1 chord-polygon approximation.
//!
//! **E1** fixture: a **quarter-disk sector** (apex at the origin, two radial
//! line edges, one 90° arc — a minor arc < π, the arena's arc requirement).
//! Extruded depth `H` → a wedge whose exact volume is the planar sector area
//! `πR²/4` times `H`, carrying exactly one cylinder patch.
//!
//! **E2** generalizes to k mixed edges (the E1 assembler is already k-general;
//! these fixtures prove it): a rounded rectangle (4 lines + 4 convex arcs),
//! a vesica lens (two CONSECUTIVE arcs at the minimal k=2 loop), and a square
//! with a CONCAVE arc bite (a cavity-sense `reversed` cylinder among line
//! walls). Each: census V=2k/E=3k/F=k+2/χ=2, exact `signed_volume = area·H`,
//! watertight mesh.
//!
//! Oracle groups:
//! 1. Topology + validation census (V=2k, E=3k, F=k+2, χ=2)
//! 2. Exact analytic volume via `geom::signed_volume` (tessellation-free)
//! 3. Tessellation: watertight, positive volume in the chord band
//! 4. Rejections: malformed arc edges, holes, oblique sweep — typed, loud

use std::f64::consts::PI;

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{
    extrude, geom, tessellate, validate_solid, BrepArena, KernelV2Error, Profile, ProfileEdge,
    RenderMesh, Surface,
};

const R: f64 = 2.0;
const H: f64 = 3.0;

/// The quarter-disk sector profile in the world XY plane (u = x̂, v = ŷ,
/// normal = +ẑ), CCW around +ẑ: apex O=(0,0) → A=(R,0) → arc → B=(0,R) → O.
fn sector_profile() -> Profile {
    let o = Point2::new(0.0, 0.0);
    let a = Point2::new(R, 0.0);
    let b = Point2::new(0.0, R);
    Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            ProfileEdge::Line { a: o, b: a },
            ProfileEdge::Arc {
                a,
                b,
                center: o,
                radius: R,
                ccw: true,
            },
            ProfileEdge::Line { a: b, b: o },
        ],
        vec![],
    )
    .expect("valid quarter-disk sector profile")
}

fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let mut six_v = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

/// Watertightness by position-keyed directed-edge pairing (quantized 1e-9).
fn assert_watertight(mesh: &RenderMesh, what: &str) {
    use std::collections::HashMap;
    let q = |x: f64| (x / 1e-9).round() as i64;
    let key = |i: u32| {
        let k = (i as usize) * 3;
        (
            q(mesh.positions[k]),
            q(mesh.positions[k + 1]),
            q(mesh.positions[k + 2]),
        )
    };
    let mut count: HashMap<_, i64> = HashMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(a), key(b));
            if ka == kb {
                continue;
            }
            *count.entry((ka, kb)).or_insert(0) += 1;
            *count.entry((kb, ka)).or_insert(0) -= 1;
        }
    }
    let unpaired = count.values().filter(|&&c| c != 0).count();
    assert_eq!(unpaired, 0, "{what}: {unpaired} unpaired directed edges");
}

// =========================================================================
// 1. Topology + validation census
// =========================================================================

#[test]
fn sector_extrude_topology_census() {
    let mut arena = BrepArena::new();
    let profile = sector_profile();
    let r = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), H).expect("sector extrude");

    let report = validate_solid(&arena, r.solid).expect("sector extrude validates");
    // k = 3 profile edges ⇒ V=2k=6, E=3k=9 (3 bottom + 3 top + 3 seam),
    // F=k+2=5, χ=2 (genus 0).
    assert_eq!(report.vertices, 6, "V");
    assert_eq!(report.edges, 9, "E");
    assert_eq!(report.faces, 5, "F");
    assert_eq!(report.rings, 0, "R");
    assert_eq!(report.shells, 1);
    assert_eq!(report.genus, 0);
    assert_eq!(report.euler_lhs, 2);
    assert_eq!(report.euler_rhs, 2);

    // Exactly one cylinder patch (the arc edge); the other 4 faces planar
    // (2 caps + 2 radial line walls), none cavity-sense.
    let (mut planes, mut cyls, mut rev) = (0usize, 0usize, 0usize);
    for f in std::iter::once(r.base)
        .chain(std::iter::once(r.top))
        .chain(r.walls.iter().copied())
    {
        match arena.face(f).expect("face").surface {
            Some(Surface::Plane(_)) => planes += 1,
            Some(Surface::Cylinder {
                reversed, radius, ..
            }) => {
                cyls += 1;
                if reversed {
                    rev += 1;
                }
                assert!((radius - R).abs() < 1e-12, "cylinder radius = R");
            }
            other => panic!("untyped surface {other:?}"),
        }
    }
    assert_eq!((planes, cyls, rev), (4, 1, 0), "surface census");
}

// =========================================================================
// 2. Exact analytic volume (tessellation-free)
// =========================================================================

#[test]
fn sector_extrude_exact_volume() {
    let mut arena = BrepArena::new();
    let profile = sector_profile();
    let r = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), H).expect("sector extrude");

    // Quarter-disk area = πR²/4; prism volume = area·H.
    let expected = PI * R * R / 4.0 * H;
    let vol = geom::signed_volume(&arena, r.solid).expect("analytic sector volume");
    assert!(
        (vol - expected).abs() <= 1e-9 * expected,
        "signed_volume {vol} vs analytic {expected}"
    );
}

// =========================================================================
// 3. Tessellation: watertight, positive volume in the chord band
// =========================================================================

#[test]
fn sector_extrude_mesh_watertight() {
    let mut arena = BrepArena::new();
    let profile = sector_profile();
    let r = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), H).expect("sector extrude");

    let mesh = tessellate(&arena, r.solid).expect("tessellate sector wedge");
    assert!(!mesh.indices.is_empty(), "non-empty mesh");
    for v in &mesh.positions {
        assert!(v.is_finite(), "finite positions");
    }
    assert_watertight(&mesh, "sector wedge mesh");

    // Chord tessellation under-fills the convex arc bulge, so the mesh volume
    // is a slight UNDER-estimate of the exact πR²H/4; positive and within a
    // few percent.
    let analytic = PI * R * R / 4.0 * H;
    let v = mesh_signed_volume(&mesh);
    assert!(v > 0.0, "positive mesh volume, got {v}");
    assert!(
        v <= analytic + 1e-9 && v >= analytic * 0.9,
        "mesh volume {v} in chord band below analytic {analytic}"
    );
}

// =========================================================================
// 4. Rejections — typed, loud, pre-mutation
// =========================================================================

#[test]
fn rejects_non_minor_arc() {
    // A 180° "arc" (A → B diametrically opposite about the center) is the
    // ambiguous half-circle the arena forbids: reject at construction.
    let a = Point2::new(R, 0.0);
    let b = Point2::new(-R, 0.0);
    let err = Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            ProfileEdge::Arc {
                a,
                b,
                center: Point2::new(0.0, 0.0),
                radius: R,
                ccw: true,
            },
            ProfileEdge::Line { a: b, b: a },
        ],
        vec![],
    )
    .expect_err("half-circle arc is rejected");
    assert!(
        matches!(err, KernelV2Error::ProfileArcEdgeInvalid),
        "{err:?}"
    );
}

#[test]
fn rejects_broken_chain() {
    // edge[0].b != edge[1].a — a gap in the loop.
    let err = Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            ProfileEdge::Line {
                a: Point2::new(0.0, 0.0),
                b: Point2::new(1.0, 0.0),
            },
            ProfileEdge::Line {
                a: Point2::new(2.0, 0.0),
                b: Point2::new(0.0, 0.0),
            },
        ],
        vec![],
    )
    .expect_err("broken chain is rejected");
    assert!(
        matches!(err, KernelV2Error::ProfileArcEdgeInvalid),
        "{err:?}"
    );
}

#[test]
fn rejects_holes_in_e1() {
    // A valid square hole loop, but E1 does not wire holed arc caps yet.
    let mut arena = BrepArena::new();
    let profile = Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            ProfileEdge::Line {
                a: Point2::new(0.0, 0.0),
                b: Point2::new(10.0, 0.0),
            },
            ProfileEdge::Line {
                a: Point2::new(10.0, 0.0),
                b: Point2::new(10.0, 10.0),
            },
            ProfileEdge::Line {
                a: Point2::new(10.0, 10.0),
                b: Point2::new(0.0, 10.0),
            },
            ProfileEdge::Line {
                a: Point2::new(0.0, 10.0),
                b: Point2::new(0.0, 0.0),
            },
        ],
        vec![vec![
            ProfileEdge::Line {
                a: Point2::new(3.0, 3.0),
                b: Point2::new(6.0, 3.0),
            },
            ProfileEdge::Line {
                a: Point2::new(6.0, 3.0),
                b: Point2::new(6.0, 6.0),
            },
            ProfileEdge::Line {
                a: Point2::new(6.0, 6.0),
                b: Point2::new(3.0, 6.0),
            },
            ProfileEdge::Line {
                a: Point2::new(3.0, 6.0),
                b: Point2::new(3.0, 3.0),
            },
        ]],
    )
    .expect("well-formed holed arc-polygon");
    let err = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), H)
        .expect_err("E1 rejects holes");
    assert!(
        matches!(err, KernelV2Error::ExtrudeArcHolesUnsupported),
        "{err:?}"
    );
    assert!(arena.solids.is_empty(), "arena untouched on rejection");
}

#[test]
fn rejects_oblique_sweep() {
    let mut arena = BrepArena::new();
    let profile = sector_profile();
    // Sweep at 45° to the +ẑ normal → elliptic-section cylinder.
    let err = extrude(&mut arena, &profile, Vector3::new(1.0, 0.0, 1.0), H)
        .expect_err("oblique arc sweep rejected");
    assert!(
        matches!(err, KernelV2Error::ExtrudeObliqueArcUnsupported),
        "{err:?}"
    );
}

// =========================================================================
// E2 — general k-edge single loop: multiple arcs + lines
// =========================================================================
//
// The E1 assembler is already k-general; these fixtures exercise the
// multi-edge path: several convex arcs (rounded rectangle), two CONSECUTIVE
// arcs at the minimal k=2 loop (a vesica lens), and a CONCAVE arc producing a
// cavity-sense (`reversed`) cylinder embedded among line walls.

/// (planes, cylinders, reversed-cylinders) over a result's faces, and a full
/// topology + exact-volume + watertight assertion. `expected_area` is the
/// exact planar region area; the extruded volume is `area · H`.
fn assert_arc_extrude(
    profile: &Profile,
    edges: usize,
    expected_census: (usize, usize, usize),
    expected_area: f64,
    what: &str,
) {
    let mut arena = BrepArena::new();
    let r = extrude(&mut arena, profile, Vector3::new(0.0, 0.0, 1.0), H)
        .unwrap_or_else(|e| panic!("{what} extrude: {e:?}"));

    let report =
        validate_solid(&arena, r.solid).unwrap_or_else(|e| panic!("{what} validates: {e:?}"));
    // k edges ⇒ V=2k, E=3k, F=k+2, χ=2 (genus 0).
    assert_eq!(report.vertices, 2 * edges, "{what}: V");
    assert_eq!(report.edges, 3 * edges, "{what}: E");
    assert_eq!(report.faces, edges + 2, "{what}: F");
    assert_eq!(report.genus, 0, "{what}: genus");
    assert_eq!(report.euler_lhs, 2, "{what}: χ");
    assert_eq!(report.euler_rhs, 2, "{what}: χ rhs");

    let (mut planes, mut cyls, mut rev) = (0usize, 0usize, 0usize);
    for f in std::iter::once(r.base)
        .chain(std::iter::once(r.top))
        .chain(r.walls.iter().copied())
    {
        match arena.face(f).expect("face").surface {
            Some(Surface::Plane(_)) => planes += 1,
            Some(Surface::Cylinder { reversed, .. }) => {
                cyls += 1;
                if reversed {
                    rev += 1;
                }
            }
            other => panic!("{what}: untyped surface {other:?}"),
        }
    }
    assert_eq!((planes, cyls, rev), expected_census, "{what}: census");

    let expected_vol = expected_area * H;
    let vol = geom::signed_volume(&arena, r.solid).expect("analytic volume");
    assert!(
        (vol - expected_vol).abs() <= 1e-9 * expected_vol.abs(),
        "{what}: signed_volume {vol} vs analytic {expected_vol}"
    );

    let mesh = tessellate(&arena, r.solid).expect("tessellate");
    assert_watertight(&mesh, what);
    assert!(
        mesh_signed_volume(&mesh) > 0.0,
        "{what}: positive mesh volume"
    );
}

/// Axis-aligned rectangle of half-extents `(a, b)` with quarter-circle
/// rounded corners of radius `r`, CCW around +ẑ. 8 edges (4 lines + 4 convex
/// arcs). Area = `4ab − r²(4 − π)`.
fn rounded_rect(a: f64, b: f64, r: f64) -> Profile {
    let p = |x, y| Point2::new(x, y);
    let edges = vec![
        ProfileEdge::Line {
            a: p(-a + r, -b),
            b: p(a - r, -b),
        },
        ProfileEdge::Arc {
            a: p(a - r, -b),
            b: p(a, -b + r),
            center: p(a - r, -b + r),
            radius: r,
            ccw: true,
        },
        ProfileEdge::Line {
            a: p(a, -b + r),
            b: p(a, b - r),
        },
        ProfileEdge::Arc {
            a: p(a, b - r),
            b: p(a - r, b),
            center: p(a - r, b - r),
            radius: r,
            ccw: true,
        },
        ProfileEdge::Line {
            a: p(a - r, b),
            b: p(-a + r, b),
        },
        ProfileEdge::Arc {
            a: p(-a + r, b),
            b: p(-a, b - r),
            center: p(-a + r, b - r),
            radius: r,
            ccw: true,
        },
        ProfileEdge::Line {
            a: p(-a, b - r),
            b: p(-a, -b + r),
        },
        ProfileEdge::Arc {
            a: p(-a, -b + r),
            b: p(-a + r, -b),
            center: p(-a + r, -b + r),
            radius: r,
            ccw: true,
        },
    ];
    Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        edges,
        vec![],
    )
    .expect("valid rounded rectangle")
}

#[test]
fn rounded_rectangle_multiple_convex_arcs() {
    let (a, b, r) = (3.0, 2.0, 0.5);
    let area = 4.0 * a * b - r * r * (4.0 - PI);
    // 8 edges → 8 walls (4 line planes + 4 convex cylinders) + 2 caps.
    assert_arc_extrude(&rounded_rect(a, b, r), 8, (6, 4, 0), area, "rounded rect");
}

#[test]
fn vesica_lens_consecutive_arcs_k2() {
    // Two arcs, no line edges (k = 2). Endpoints (∓1, 0); each arc bulges
    // away from the lens → both convex. With chord 2 and centers (0, ∓1)
    // the radius is √2, sweep π/2, lens area = π − 2.
    let r2 = 2.0_f64.sqrt();
    let a = Point2::new(-1.0, 0.0);
    let b = Point2::new(1.0, 0.0);
    let profile = Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            // Lower arc A → B (CCW around +ẑ dips below), center above.
            ProfileEdge::Arc {
                a,
                b,
                center: Point2::new(0.0, 1.0),
                radius: r2,
                ccw: true,
            },
            // Upper arc B → A, center below.
            ProfileEdge::Arc {
                a: b,
                b: a,
                center: Point2::new(0.0, -1.0),
                radius: r2,
                ccw: true,
            },
        ],
        vec![],
    )
    .expect("valid vesica lens");
    // k=2 → 2 walls (both convex cylinders) + 2 caps.
    assert_arc_extrude(&profile, 2, (2, 2, 0), PI - 2.0, "vesica lens");
}

#[test]
fn concave_arc_makes_reversed_cylinder() {
    // A unit-ish square [0,4]² whose TOP edge is a concave arc dipping into
    // the interior: center (2,7) ABOVE the chord, so the minor arc bulges
    // DOWN (a bite). The arc is CW around +ẑ for the boundary traversal
    // (4,4)→(0,4) ⇒ a cavity-sense (`reversed`) cylinder wall.
    let c = Point2::new(2.0, 7.0);
    let r = (4.0_f64 + 9.0).sqrt(); // |(4,4) − (2,7)| = √13
    let p = |x, y| Point2::new(x, y);
    let profile = Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            ProfileEdge::Line {
                a: p(0.0, 0.0),
                b: p(4.0, 0.0),
            },
            ProfileEdge::Line {
                a: p(4.0, 0.0),
                b: p(4.0, 4.0),
            },
            ProfileEdge::Arc {
                a: p(4.0, 4.0),
                b: p(0.0, 4.0),
                center: c,
                radius: r,
                ccw: false,
            },
            ProfileEdge::Line {
                a: p(0.0, 4.0),
                b: p(0.0, 0.0),
            },
        ],
        vec![],
    )
    .expect("valid concave-bite square");
    // Region = square 16 minus the circular segment bitten out.
    let theta = 2.0 * (2.0 / r).asin();
    let segment = r * r / 2.0 * (theta - theta.sin());
    let area = 16.0 - segment;
    // 4 edges → 3 line planes + 1 reversed cylinder + 2 caps.
    assert_arc_extrude(&profile, 4, (5, 1, 1), area, "concave bite");
}

// =========================================================================
// E3 — exact arc-loop simplicity validation (self-intersection ⇒ NotSimple)
// =========================================================================

fn arc_polygon_outer(outer: Vec<ProfileEdge>) -> Result<Profile, KernelV2Error> {
    Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        outer,
        vec![],
    )
}

#[test]
fn rejects_line_bowtie() {
    // Figure-eight: the two diagonals (e0, e2) cross at (2,2). All-line, via
    // the ArcPolygon path — exercises the exact proper-crossing predicate.
    let p = |x, y| Point2::new(x, y);
    let err = arc_polygon_outer(vec![
        ProfileEdge::Line {
            a: p(0.0, 0.0),
            b: p(4.0, 4.0),
        },
        ProfileEdge::Line {
            a: p(4.0, 4.0),
            b: p(4.0, 0.0),
        },
        ProfileEdge::Line {
            a: p(4.0, 0.0),
            b: p(0.0, 4.0),
        },
        ProfileEdge::Line {
            a: p(0.0, 4.0),
            b: p(0.0, 0.0),
        },
    ])
    .expect_err("bowtie is self-intersecting");
    assert!(
        matches!(err, KernelV2Error::ProfileNotSimple { loop_index: 0 }),
        "{err:?}"
    );
}

#[test]
fn rejects_arc_crossed_by_diagonal() {
    // Quarter-circle sector arc (2,0)→(0,2) about the origin, then a closing
    // diagonal (2,2)→(0,0) that pierces the arc at (√2, √2). The arc (e1)
    // and the diagonal (e3) are non-adjacent ⇒ illegal crossing.
    let p = |x, y| Point2::new(x, y);
    let err = arc_polygon_outer(vec![
        ProfileEdge::Line {
            a: p(0.0, 0.0),
            b: p(2.0, 0.0),
        },
        ProfileEdge::Arc {
            a: p(2.0, 0.0),
            b: p(0.0, 2.0),
            center: p(0.0, 0.0),
            radius: 2.0,
            ccw: true,
        },
        ProfileEdge::Line {
            a: p(0.0, 2.0),
            b: p(2.0, 2.0),
        },
        ProfileEdge::Line {
            a: p(2.0, 2.0),
            b: p(0.0, 0.0),
        },
    ])
    .expect_err("diagonal pierces the arc");
    assert!(
        matches!(err, KernelV2Error::ProfileNotSimple { loop_index: 0 }),
        "{err:?}"
    );
}

#[test]
fn rejects_two_line_digon() {
    // A two-edge loop of straight segments is a zero-area degenerate digon.
    let p = |x, y| Point2::new(x, y);
    let err = arc_polygon_outer(vec![
        ProfileEdge::Line {
            a: p(0.0, 0.0),
            b: p(4.0, 0.0),
        },
        ProfileEdge::Line {
            a: p(4.0, 0.0),
            b: p(0.0, 0.0),
        },
    ])
    .expect_err("two-line digon is degenerate");
    assert!(
        matches!(err, KernelV2Error::ProfileNotSimple { loop_index: 0 }),
        "{err:?}"
    );
}

#[test]
fn rejects_vertex_pinch_on_edge() {
    // A non-junction vertex landing on a non-adjacent edge's interior (the
    // loop pinches against itself). Square (0,0)→(4,0)→(4,4)→(0,4), but the
    // last vertex is pulled onto the bottom edge at (2,0).
    let p = |x, y| Point2::new(x, y);
    let err = arc_polygon_outer(vec![
        ProfileEdge::Line {
            a: p(0.0, 0.0),
            b: p(4.0, 0.0),
        },
        ProfileEdge::Line {
            a: p(4.0, 0.0),
            b: p(4.0, 4.0),
        },
        ProfileEdge::Line {
            a: p(4.0, 4.0),
            b: p(2.0, 0.0),
        },
        ProfileEdge::Line {
            a: p(2.0, 0.0),
            b: p(0.0, 0.0),
        },
    ])
    .expect_err("vertex pinches the bottom edge");
    assert!(
        matches!(err, KernelV2Error::ProfileNotSimple { loop_index: 0 }),
        "{err:?}"
    );
}

#[test]
fn rejects_hole_crossing_outer() {
    // A hole loop whose edge crosses the outer boundary ⇒ loops intersect.
    let p = |x, y| Point2::new(x, y);
    let outer = vec![
        ProfileEdge::Line {
            a: p(0.0, 0.0),
            b: p(10.0, 0.0),
        },
        ProfileEdge::Line {
            a: p(10.0, 0.0),
            b: p(10.0, 10.0),
        },
        ProfileEdge::Line {
            a: p(10.0, 10.0),
            b: p(0.0, 10.0),
        },
        ProfileEdge::Line {
            a: p(0.0, 10.0),
            b: p(0.0, 0.0),
        },
    ];
    // Hole straddles the right wall (x from 8 to 12 crosses x=10).
    let hole = vec![
        ProfileEdge::Line {
            a: p(8.0, 4.0),
            b: p(12.0, 4.0),
        },
        ProfileEdge::Line {
            a: p(12.0, 4.0),
            b: p(12.0, 6.0),
        },
        ProfileEdge::Line {
            a: p(12.0, 6.0),
            b: p(8.0, 6.0),
        },
        ProfileEdge::Line {
            a: p(8.0, 6.0),
            b: p(8.0, 4.0),
        },
    ];
    let err = Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        outer,
        vec![hole],
    )
    .expect_err("hole crosses the outer boundary");
    assert!(
        matches!(
            err,
            KernelV2Error::ProfileLoopsIntersect {
                loop_a: 0,
                loop_b: 1
            }
        ),
        "{err:?}"
    );
}

#[test]
fn accepts_valid_arc_loops() {
    // Sanity (GREEN): the valid E1/E2 fixtures pass the exact simplicity gate
    // — a self-intersection check must not reject a simple boundary.
    sector_profile();
    rounded_rect(3.0, 2.0, 0.5);
}
