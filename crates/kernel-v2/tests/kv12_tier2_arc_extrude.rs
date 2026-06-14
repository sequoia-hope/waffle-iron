//! PR-KV12 Tier 2, increment E1 — exact mixed line/arc extrude.
//!
//! `extrude` of a `ProfileRegion::ArcPolygon` (a closed loop of line + minor
//! circular-arc edges) produces an EXACT B-Rep: planar caps bounded by the
//! line+arc loop, a planar side wall per line edge, and a `Surface::Cylinder`
//! patch per arc edge (an arc swept perpendicular to its plane IS a cylinder
//! lateral). Replaces the KV12 Tier-1 chord-polygon approximation.
//!
//! E1 fixture: a **quarter-disk sector** (apex at the origin, two radial
//! line edges, one 90° arc — a minor arc < π, the arena's arc requirement).
//! Extruded depth `H` → a wedge whose exact volume is the planar sector area
//! `πR²/4` times `H`, carrying exactly one cylinder patch.
//!
//! Oracle groups:
//! 1. Topology + validation census (V=6, E=9, F=5, χ=2; 4 planes + 1 cyl)
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
