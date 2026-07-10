#[allow(unused_imports)]
use super::*;

// ── collapse_vertex membrane cancellation ────────────────────────────
// Spec `specs/yang_collapse_membrane_cancellation.md` (task #121, the
// N2/F0059 Stage-6 double-cover origin). A twin collapse can turn the
// two-triangle pleat spanning the twin gap into an EXACT duplicate pair
// with OPPOSITE windings — a zero-volume doubled flap that must cancel
// (drop BOTH), restoring manifold edge counts.

/// The minimal closed pleat: a sliver tetra {a,b,u,v} whose two large
/// walls (a,b,u)/(a,v,b) become the opposite-winding duplicate after the
/// twin collapse v→u. Indices 0..=3; positions are irrelevant to the
/// combinatorial collapse but kept realistic (near-twin apexes).
pub(crate) fn pleat_tetra_tris() -> Vec<[u32; 3]> {
    vec![[0, 1, 2], [1, 3, 2], [0, 2, 3], [0, 3, 1]]
}

pub(crate) fn membrane_fixture_verts() -> Vec<Point3> {
    vec![
        Point3::new(0.0, 0.0, 0.0),       // 0 = a
        Point3::new(1.0, 0.0, 0.0),       // 1 = b
        Point3::new(0.5, 0.4, 0.1),       // 2 = u (survivor twin)
        Point3::new(0.5, 0.4, 0.1000001), // 3 = v (victim twin)
        // Bystander tetra (a separate closed component that must be
        // preserved byte-for-byte through the cancellation).
        Point3::new(3.0, 0.0, 0.0), // 4
        Point3::new(4.0, 0.0, 0.0), // 5
        Point3::new(3.5, 1.0, 0.0), // 6
        Point3::new(3.5, 0.5, 1.0), // 7
    ]
}

pub(crate) fn bystander_tetra_tris() -> Vec<[u32; 3]> {
    vec![[4, 5, 6], [4, 6, 7], [4, 7, 5], [5, 7, 6]]
}

pub(crate) fn undirected_edge_counts(
    tris: &[[u32; 3]],
) -> std::collections::BTreeMap<(u32, u32), u32> {
    let mut counts = std::collections::BTreeMap::new();
    for tri in tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            let key = if a < b { (a, b) } else { (b, a) };
            *counts.entry(key).or_insert(0u32) += 1;
        }
    }
    counts
}

/// Cancellation branch: the pleat annihilates (both duplicate copies
/// dropped), the bystander survives byte-identically, every remaining
/// undirected edge is manifold count-2, and attribution stays lockstep.
#[test]
pub(crate) fn collapse_membrane_pleat_cancels_both_copies() {
    let mut tris = pleat_tetra_tris();
    tris.extend(bystander_tetra_tris());
    let mut mesh = Mesh::new(membrane_fixture_verts(), tris);
    let mut attribution: Vec<Option<TriangleAttribution>> = (0..mesh.tris.len())
        .map(|i| {
            Some(TriangleAttribution {
                input: InputId::A,
                face: i as u32,
            })
        })
        .collect();
    collapse_vertex(&mut mesh, &mut attribution, 3, 2);
    // The pleat's two gap slivers drop as degenerate; its two walls map
    // to the SAME sorted triple {0,1,2} with opposite windings — the
    // zero-volume flap — and must BOTH cancel. Only the bystander stays.
    assert_eq!(
        mesh.tris,
        bystander_tetra_tris(),
        "pleat must annihilate; bystander byte-identical"
    );
    assert_eq!(
        attribution
            .iter()
            .map(|a| a.expect("bystander attribution").face)
            .collect::<Vec<_>>(),
        vec![4, 5, 6, 7],
        "attribution must drop the cancelled pair in lockstep"
    );
    for ((a, b), n) in undirected_edge_counts(&mesh.tris) {
        assert_eq!(n, 2, "edge ({a},{b}) not manifold after cancellation");
    }
}

/// Same-winding branch: a genuine same-winding double cover is NOT a
/// cancellable flap — both copies stay for the downstream loud STOPs.
#[test]
pub(crate) fn collapse_same_winding_duplicate_is_kept() {
    let mut tris = pleat_tetra_tris();
    // Flip the second wall so the post-collapse duplicates share one
    // winding: (0,3,1) → (0,1,3) maps to (0,1,2) — same cycle as wall 1.
    tris[3] = [0, 1, 3];
    tris.extend(bystander_tetra_tris());
    let mut mesh = Mesh::new(membrane_fixture_verts(), tris);
    let mut attribution: Vec<Option<TriangleAttribution>> = vec![None; mesh.tris.len()];
    collapse_vertex(&mut mesh, &mut attribution, 3, 2);
    let dup_count = mesh
        .tris
        .iter()
        .filter(|t| {
            let mut s = **t;
            s.sort_unstable();
            s == [0, 1, 2]
        })
        .count();
    assert_eq!(
        dup_count, 2,
        "same-winding duplicates must be left for downstream loudness"
    );
    assert_eq!(mesh.tris.len(), 6, "2 kept duplicates + 4 bystander tris");
}

/// No-duplicate branch: a clean twin collapse (split-pole octahedron —
/// the twins own DISJOINT fan sectors) is byte-identical to the plain
/// index-mapping semantics: seam tents drop as degenerate, fans merge,
/// nothing cancels.
#[test]
pub(crate) fn collapse_without_duplicate_is_byte_identical() {
    // Equator 0..=3, south pole 4, north twins u=5 / v=6.
    let verts: Vec<Point3> = vec![
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
        Point3::new(-1.0, 0.0, 0.0),
        Point3::new(0.0, -1.0, 0.0),
        Point3::new(0.0, 0.0, -1.0),
        Point3::new(0.0, 0.0, 1.0),
        Point3::new(0.0, 0.0, 1.0000001),
    ];
    let tris: Vec<[u32; 3]> = vec![
        // south fans
        [1, 0, 4],
        [2, 1, 4],
        [3, 2, 4],
        [0, 3, 4],
        // north: u covers sectors 01/12, v covers 23/30
        [0, 1, 5],
        [1, 2, 5],
        [2, 3, 6],
        [3, 0, 6],
        // seam tents at equator verts 2 and 0
        [5, 2, 6],
        [6, 0, 5],
    ];
    let mut mesh = Mesh::new(verts.clone(), tris);
    let mut attribution: Vec<Option<TriangleAttribution>> = vec![None; mesh.tris.len()];
    let dropped = collapse_vertex(&mut mesh, &mut attribution, 6, 5);
    assert_eq!(dropped, 2, "exactly the two seam tents drop as degenerate");
    let expected: Vec<[u32; 3]> = vec![
        [1, 0, 4],
        [2, 1, 4],
        [3, 2, 4],
        [0, 3, 4],
        [0, 1, 5],
        [1, 2, 5],
        [2, 3, 5],
        [3, 0, 5],
    ];
    assert_eq!(
        mesh.tris, expected,
        "clean collapse must not cancel anything"
    );
    assert_eq!(mesh.verts, verts, "collapse never touches vertex storage");
    for ((a, b), n) in undirected_edge_counts(&mesh.tris) {
        assert_eq!(n, 2, "edge ({a},{b}) not manifold after clean collapse");
    }
}

// ── rim junction derivation (N2/F0059 increment 2, banked) ──────────
// Spec `specs/yang_rim_junction_insertion.md`. Fixture mirrors the
// integration cylinder fixture (seam-edge encoding).

pub(crate) fn rj_cylinder(
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    radius: f64,
    height: f64,
) -> BRep {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let crs = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let d = normalize3(axis_dir);
    let bot = axis_point;
    let top = [
        bot[0] + d[0] * height,
        bot[1] + d[1] * height,
        bot[2] + d[2] * height,
    ];
    let abs = [d[0].abs(), d[1].abs(), d[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = normalize3(crs(d, world));
    let verts = vec![
        BRepVertex {
            point: Point3::new(
                bot[0] + e1[0] * radius,
                bot[1] + e1[1] * radius,
                bot[2] + e1[2] * radius,
            ),
        },
        BRepVertex {
            point: Point3::new(
                top[0] + e1[0] * radius,
                top[1] + e1[1] * radius,
                top[2] + e1[2] * radius,
            ),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(bot[0], bot[1], bot[2]),
                normal: Vector3::new(-d[0], -d[1], -d[2]),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(top[0], top[1], top[2]),
                normal: Vector3::new(d[0], d[1], d[2]),
                radius,
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
                axis_point: Point3::new(axis_point[0], axis_point[1], axis_point[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                radius,
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
    BRep::new(verts, edges, faces).expect("rj cylinder fixture builds")
}

/// The truncated-Steinmetz pair (h/2 < r): axes x and y crossing at
/// each other's midpoints — the F0059 shape.
pub(crate) fn rj_truncated_pair(r: f64, h: f64) -> (BRep, BRep) {
    (
        rj_cylinder([0.0, -h / 2.0, 0.0], [0.0, 1.0, 0.0], r, h),
        rj_cylinder([-h / 2.0, 0.0, 0.0], [1.0, 0.0, 0.0], r, h),
    )
}

/// F0059 class: each cap rim of each operand carries exactly the four
/// lobe corners `(±h/2, ±√(r²−h²/4))`, exact on the rim circle AND on
/// the other operand's lateral (spec oracle 1 + I2).
#[test]
pub(crate) fn rim_junctions_truncated_steinmetz_four_corners_per_cap() {
    let (r, h) = (0.35f64, 0.5f64);
    let (a, b) = rj_truncated_pair(r, h);
    let (map_a, map_b) = rim_junction_overrides(&a, &b);
    let w = (r * r - h * h / 4.0).sqrt();
    for (brep, map, other_axis_is_x) in [(&a, &map_a, true), (&b, &map_b, false)] {
        assert_eq!(
            map.keys().copied().collect::<Vec<_>>(),
            vec![0, 1],
            "both cap rims carry junctions"
        );
        for (&ei, pts) in map.iter() {
            assert_eq!(pts.len(), 4, "four lobe corners per cap rim");
            let Curve::Circle { center, radius, .. } = brep.edges()[ei as usize].curve else {
                panic!("rim edge is a circle");
            };
            for p in pts {
                let pa = p.as_array();
                let ca = center.as_array();
                let dd = [pa[0] - ca[0], pa[1] - ca[1], pa[2] - ca[2]];
                let dist = (dd[0] * dd[0] + dd[1] * dd[1] + dd[2] * dd[2]).sqrt();
                assert!(
                    (dist - radius).abs() <= 1e-12,
                    "I2: junction exactly on the rim circle"
                );
                // Exactly on the OTHER operand's lateral: distance to
                // its axis (x or y axis through the origin) equals r.
                let lat = if other_axis_is_x {
                    (pa[1] * pa[1] + pa[2] * pa[2]).sqrt()
                } else {
                    (pa[0] * pa[0] + pa[2] * pa[2]).sqrt()
                };
                assert!(
                    (lat - r).abs() <= 1e-12,
                    "I2: junction exactly on the crossing lateral"
                );
                // The corner coordinates are the analytic lobe corners.
                let along = if other_axis_is_x { pa[0] } else { pa[1] };
                assert!(
                    (along.abs() - h / 2.0).abs() <= 1e-12,
                    "corner sits at ±h/2 along the crossing axis"
                );
                assert!(
                    (pa[2].abs() - w).abs() <= 1e-12,
                    "corner sits at ±√(r²−h²/4) in z"
                );
            }
        }
    }
}

/// Rebuild plumbing (spec I1/I3): an empty override map rebuild is
/// byte-identical; a real map plants every junction as a bit-exact
/// Stage-1 mesh vertex.
#[test]
pub(crate) fn rebuilt_with_rim_overrides_identity_and_insertion() {
    let (a, b) = rj_truncated_pair(0.35, 0.5);
    let same = a
        .rebuilt_with_rim_overrides(&std::collections::BTreeMap::new())
        .expect("empty rebuild");
    assert_eq!(
        same.as_mesh(),
        a.as_mesh(),
        "I1: empty override map is byte-identical"
    );
    let (map_a, _) = rim_junction_overrides(&a, &b);
    let boosted = a
        .rebuilt_with_rim_overrides(&map_a)
        .expect("boosted rebuild");
    for pts in map_a.values() {
        for p in pts {
            assert!(
                boosted.as_mesh().verts.iter().any(|q| q == p),
                "junction {p:?} must be a bit-exact Stage-1 mesh vertex"
            );
        }
    }
}

/// kv9f1 class (h/2 > r): the seam never reaches the caps — no rim
/// junctions, both maps empty (spec oracle 2 / branch row 1).
#[test]
pub(crate) fn rim_junctions_empty_when_seam_clears_caps() {
    let (a, b) = (
        rj_cylinder([0.0, -0.45, 0.0], [0.0, 1.0, 0.0], 0.2, 0.9),
        rj_cylinder([-0.45, 0.0, 0.0], [1.0, 0.0, 0.0], 0.2, 0.9),
    );
    let (map_a, map_b) = rim_junction_overrides(&a, &b);
    assert!(map_a.is_empty() && map_b.is_empty());
}

/// h/2 == r: each cap plane is exactly TANGENT to the other lateral —
/// the tangency class is skipped (|δ| ≥ r_b), never inserted.
#[test]
pub(crate) fn rim_junctions_tangent_cap_plane_skipped() {
    let (a, b) = rj_truncated_pair_tangent();
    let (map_a, map_b) = rim_junction_overrides(&a, &b);
    assert!(map_a.is_empty() && map_b.is_empty());
}

pub(crate) fn rj_truncated_pair_tangent() -> (BRep, BRep) {
    let (r, h) = (0.35f64, 0.7f64);
    (
        rj_cylinder([0.0, -h / 2.0, 0.0], [0.0, 1.0, 0.0], r, h),
        rj_cylinder([-h / 2.0, 0.0, 0.0], [1.0, 0.0, 0.0], r, h),
    )
}

/// Candidates beyond the crossing lateral's axial extent are excluded
/// (spec candidate filter 2): shifting B along its axis puts every
/// infinite-LATERAL junction outside both operands' extents
/// (a-rim × b-lateral would sit at x = ±0.245, outside b's
/// [0.3, 0.65]; b-rim × a-lateral at y = ±0.302, outside a's
/// [−0.25, 0.25]). The PLANE arm never fires here: cylinder rims are
/// outside its cone-flanked v1 scope (the demonstrated-need gate —
/// this population is proven healthy without insertion).
#[test]
pub(crate) fn rim_junctions_respect_lateral_extent() {
    let a = rj_cylinder([0.0, -0.25, 0.0], [0.0, 1.0, 0.0], 0.35, 0.5);
    let b = rj_cylinder([0.3, 0.0, 0.0], [1.0, 0.0, 0.0], 0.35, 0.5);
    let (map_a, map_b) = rim_junction_overrides(&a, &b);
    assert!(
        map_a.is_empty() && map_b.is_empty(),
        "lateral out-of-extent candidates excluded; cylinder rims outside \
             the plane arm's cone-flanked scope"
    );
}

// ── Increment 4: plane-face arm + coaxial azimuth propagation ────────
// Spec `specs/yang_rim_junction_insertion.md` §4a/§4b — the
// cone-hyperbola junction class (R0004/R0017/R0019/R0044/R0047/R0049):
// coaxial cone-band rim circles crossing a PLANE face of the other
// operand.

/// Coaxial double-frustum lathe on the z-axis: rims (z=0, r0),
/// (z=1, r1), (z=2, r2), two cone bands sharing the middle rim, planar
/// caps at both ends. Adjacent radii must differ (genuine cones).
pub(crate) fn rj_lathe(r0: f64, r1: f64, r2: f64) -> BRep {
    assert!(r0 != r1 && r1 != r2, "bands must be genuine cones");
    let verts = vec![
        BRepVertex {
            point: Point3::new(r0, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(r1, 0.0, 1.0),
        },
        BRepVertex {
            point: Point3::new(r2, 0.0, 2.0),
        },
    ];
    let circle = |cz: f64, nz: f64, radius: f64| Curve::Circle {
        center: Point3::new(0.0, 0.0, cz),
        normal: Vector3::new(0.0, 0.0, nz),
        radius,
    };
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: circle(0.0, -1.0, r0),
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: circle(1.0, 1.0, r1),
        },
        BRepEdge {
            start: 2,
            end: 2,
            curve: circle(2.0, 1.0, r2),
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 1,
            end: 2,
            curve: Curve::LineSegment,
        },
    ];
    // Cone through profile points (ra, za)-(rb, zb): apex on the axis
    // where the linear radius profile reaches 0; axis_dir points from
    // the apex toward the band.
    let cone = |ra: f64, za: f64, rb: f64, zb: f64| -> Surface {
        let slope = (rb - ra) / (zb - za);
        let z_apex = za - ra / slope;
        let dir = if slope > 0.0 { 1.0 } else { -1.0 };
        Surface::Cone {
            apex: Point3::new(0.0, 0.0, z_apex),
            axis_dir: Vector3::new(0.0, 0.0, dir),
            half_angle: slope.abs().atan(),
        }
    };
    let faces = vec![
        BRepFace {
            surface: cone(r0, 0.0, r1, 1.0),
            outer_loop: vec![0, 3, 1, 3],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: cone(r1, 1.0, r2, 2.0),
            outer_loop: vec![1, 4, 2, 4],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -2.0,
            },
            outer_loop: vec![2],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("rj lathe fixture builds")
}

/// Axis-aligned box (the slab operand): 6 polygonal plane faces.
pub(crate) fn rj_box(lo: [f64; 3], hi: [f64; 3]) -> BRep {
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
    BRep::new(vertices, edges, faces).expect("rj box fixture builds")
}

/// §4a+§4b class oracle: every lathe rim crosses the slab's x = c face
/// plane transversally → per rim, TWO direct junctions
/// `(c, ±√(r²−c²), z)` PLUS the other rims' azimuths propagated
/// exactly onto its own circle. All three rims present the SAME
/// azimuth multiset (the Stage-1 band-strip alignment invariant I5).
#[test]
pub(crate) fn rim_junctions_plane_arm_lathe_slab_all_rims() {
    let (r0, r1, r2) = (1.0f64, 2.0, 0.8);
    let c = 0.75f64;
    let lathe = rj_lathe(r0, r1, r2);
    let slab = rj_box([c, -4.0, -0.5], [4.0, 4.0, 2.5]);
    let (map_l, map_s) = rim_junction_overrides(&lathe, &slab);
    assert!(map_s.is_empty(), "the slab has no circle rims");
    assert_eq!(
        map_l.keys().copied().collect::<Vec<_>>(),
        vec![0, 1, 2],
        "all three rims carry insertions"
    );
    let mut az_sets: Vec<Vec<f64>> = Vec::new();
    for (&ei, pts) in map_l.iter() {
        let Curve::Circle { center, radius, .. } = lathe.edges()[ei as usize].curve else {
            panic!("rim edge is a circle");
        };
        let cz = center.as_array()[2];
        // 2 direct junctions per rim + 2 propagated from each other rim.
        assert_eq!(pts.len(), 6, "rim {ei}: 2 direct + 4 propagated");
        let mut on_plane = 0usize;
        let mut azimuths: Vec<f64> = Vec::new();
        for pt in pts {
            let pa = pt.as_array();
            let rad = (pa[0] * pa[0] + pa[1] * pa[1]).sqrt();
            assert!(
                (rad - radius).abs() <= 1e-12,
                "I2/I5: point exactly on rim {ei}'s circle"
            );
            assert!((pa[2] - cz).abs() <= 1e-12, "point in rim {ei}'s plane");
            if (pa[0] - c).abs() <= 1e-12 {
                on_plane += 1;
                let w = (radius * radius - c * c).sqrt();
                assert!(
                    (pa[1].abs() - w).abs() <= 1e-12,
                    "direct junction at (c, ±√(r²−c²), z)"
                );
            }
            azimuths.push(pa[1].atan2(pa[0]).rem_euclid(2.0 * std::f64::consts::PI));
        }
        assert_eq!(on_plane, 2, "rim {ei}: exactly two direct junctions");
        azimuths.sort_by(f64::total_cmp);
        az_sets.push(azimuths);
    }
    for k in 1..az_sets.len() {
        assert_eq!(az_sets[k].len(), az_sets[0].len());
        for (a, b) in az_sets[k].iter().zip(az_sets[0].iter()) {
            assert!(
                (a - b).abs() <= 1e-12,
                "azimuth multisets align across coaxial rims"
            );
        }
    }
}

/// §4a containment: the slab shifted so its x-face plane still crosses
/// the rim circles but OUTSIDE the face polygon → no insertion.
#[test]
pub(crate) fn rim_junctions_plane_arm_containment_outside_face() {
    let lathe = rj_lathe(1.0, 2.0, 0.8);
    let slab = rj_box([0.75, 2.5, -0.5], [4.0, 5.0, 2.5]);
    let (map_l, map_s) = rim_junction_overrides(&lathe, &slab);
    assert!(
        map_l.is_empty() && map_s.is_empty(),
        "crossings outside the face polygon must not insert"
    );
}

/// §4a parallel skip: a box whose only near face is PARALLEL to the rim
/// planes (top face containing the middle rim's plane) → no section
/// line, no insertion; its transversal side faces miss the circles.
#[test]
pub(crate) fn rim_junctions_plane_arm_parallel_plane_skipped() {
    let lathe = rj_lathe(1.0, 2.0, 0.8);
    let slab = rj_box([-4.0, -4.0, -1.0], [4.0, 4.0, 1.0]);
    let (map_l, map_s) = rim_junction_overrides(&lathe, &slab);
    assert!(
        map_l.is_empty() && map_s.is_empty(),
        "parallel planes have no transversal section line"
    );
}

/// §4b vocabulary gate: a full-circle rim owned by a TORUS face (the
/// kv6d bent-tube profile rim) must never receive insertions — the
/// band-strip propagation vocabulary covers Cone/Cylinder/Plane only.
#[test]
pub(crate) fn rim_junctions_group_gate_drops_torus_rims() {
    // 90° bent tube: torus center origin, axis +z, R=3, r=1 (the kv6d
    // fixture), profile rim e0 at center (3,0,0), normal +y, radius 1.
    let verts = vec![
        BRepVertex {
            point: Point3::new(4.0, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(0.0, 4.0, 0.0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(3.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 1.0, 0.0),
                radius: 1.0,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(0.0, 3.0, 0.0),
                normal: Vector3::new(1.0, 0.0, 0.0),
                radius: 1.0,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: 4.0,
            },
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Torus {
                center: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                major_radius: 3.0,
                minor_radius: 1.0,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, -1.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(-1.0, 0.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    let tube = BRep::new(verts, edges, faces).expect("kv6d bent tube builds");
    // The slab's x = 3 face plane crosses profile rim e0 (center
    // (3,0,0), r=1, plane y=0) at (3, 0, ±1) — transversal, contained.
    let slab = rj_box([3.0, -0.5, -2.0], [5.0, 0.5, 2.0]);
    let (map_t, map_s) = rim_junction_overrides(&tube, &slab);
    assert!(
        map_t.is_empty() && map_s.is_empty(),
        "torus-owned rim groups must be dropped by the vocabulary gate"
    );
}

/// §4a arc extension (the measured corpus shape — partial revolves):
/// a half-turn washer sector's OUTER arcs cross the slab plane at ONE
/// in-sweep azimuth (the mirror root lies in the missing half); the
/// junction is inserted there and NEVER at the out-of-sweep root, and
/// §4b propagates the azimuth onto the INNER arcs exactly on-circle.
#[test]
pub(crate) fn rim_junctions_plane_arm_partial_arc_rims() {
    // Half-turn CONE-walled washer sector about +x (the plane arm's
    // v1 scope demands cone-flanked rims): trapezoid profile
    // (0,1.0)-(1,1.3)-(1,2.3)-(0,2.0), swept z ≥ 0 (angle π). Arcs:
    // e8 (r=1.0 @ x=0), e9 (r=1.3 @ x=1), e10 (r=2.3 @ x=1),
    // e11 (r=2.0 @ x=0), all centered on the x-axis with normal +x̂.
    let angle = std::f64::consts::PI;
    let prof = [(0.0, 1.0), (1.0, 1.3), (1.0, 2.3), (0.0, 2.0)];
    let mut verts: Vec<BRepVertex> = prof
        .iter()
        .map(|&(x, y)| BRepVertex {
            point: Point3::new(x, y, 0.0),
        })
        .collect();
    for &(x, y) in &prof {
        // Rotation by π about +x̂: (y, z) → (−y, z sign-flipped ≈ 0).
        let (c, s) = (angle.cos(), angle.sin());
        verts.push(BRepVertex {
            point: Point3::new(x, y * c, y * s),
        });
    }
    let seg = |a: u32, b: u32| BRepEdge {
        start: a,
        end: b,
        curve: Curve::LineSegment,
    };
    let mut edges = vec![
        seg(0, 1),
        seg(1, 2),
        seg(2, 3),
        seg(3, 0),
        seg(4, 5),
        seg(5, 6),
        seg(6, 7),
        seg(7, 4),
    ];
    for i in 0..4u32 {
        let (x, y) = prof[i as usize];
        edges.push(BRepEdge {
            start: i,
            end: i + 4,
            curve: Curve::Circle {
                center: Point3::new(x, 0.0, 0.0),
                normal: Vector3::new(1.0, 0.0, 0.0),
                radius: y,
            },
        });
    }
    let (a0, a1, a2, a3) = (8u32, 9u32, 10u32, 11u32);
    let faces = vec![
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: vec![],
            reversed: false,
        },
        // End cap after a π sweep: the z = 0 plane again, outward −ẑ
        // rotated → +ẑ... outward normal is R_x(π)·ẑ = −ẑ → (0,0,-1)?
        // The kv6b fixture computes (0, −sin α, cos α) = (0, 0, −1).
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: 0.0,
            },
            outer_loop: vec![4, 5, 6, 7],
            inner_loops: vec![],
            reversed: false,
        },
        BRepFace {
            // Inner CONE wall (cavity sense): r = 1.0 @ x=0 → 1.3 @
            // x=1, slope 0.3, apex on the axis at x = −1.0/0.3.
            surface: Surface::Cone {
                apex: Point3::new(-1.0 / 0.3, 0.0, 0.0),
                axis_dir: Vector3::new(1.0, 0.0, 0.0),
                half_angle: 0.3f64.atan(),
            },
            outer_loop: vec![0, a1, 4, a0],
            inner_loops: vec![],
            reversed: true,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(1.0, 0.0, 0.0),
                d: -1.0,
            },
            outer_loop: vec![1, a2, 5, a1],
            inner_loops: vec![],
            reversed: false,
        },
        BRepFace {
            // Outer CONE wall: r = 2.0 @ x=0 → 2.3 @ x=1, slope 0.3,
            // apex at x = −2.0/0.3.
            surface: Surface::Cone {
                apex: Point3::new(-2.0 / 0.3, 0.0, 0.0),
                axis_dir: Vector3::new(1.0, 0.0, 0.0),
                half_angle: 0.3f64.atan(),
            },
            outer_loop: vec![2, a3, 6, a2],
            inner_loops: vec![],
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(-1.0, 0.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![3, a0, 7, a3],
            inner_loops: vec![],
            reversed: false,
        },
    ];
    let sector = BRep::new(verts, edges, faces).expect("washer sector builds");
    // Slab beyond y = −1.5: its y = −1.5 face plane crosses the OUTER
    // arcs (r = 2.3, 2.0) at z = +√(r² − 2.25) — only z > 0 is in the
    // sweep (the mirror root lies in the missing half). The inner arcs
    // (r = 1.0, 1.3) never reach y = −1.5 and receive only the
    // propagated cluster azimuths.
    let slab = rj_box([-1.0, -4.0, -4.0], [2.0, -1.5, 4.0]);
    let (map_x, map_s) = rim_junction_overrides(&sector, &slab);
    assert!(map_s.is_empty(), "the slab has no circle rims");
    assert_eq!(
        map_x.keys().copied().collect::<Vec<_>>(),
        vec![8, 9, 10, 11],
        "outer arcs carry direct junctions; inner arcs the propagated azimuths"
    );
    for (&ei, pts) in map_x.iter() {
        let Curve::Circle { center, radius, .. } = sector.edges()[ei as usize].curve else {
            panic!("arc edge is a circle");
        };
        // TWO clusters (one per outer arc's distinct junction azimuth),
        // both inside every arc's sweep window.
        assert_eq!(pts.len(), 2, "arc {ei}: both cluster azimuths inserted");
        let ca = center.as_array();
        for pt in pts {
            let pa = pt.as_array();
            assert!(pa[2] > 0.0, "arc {ei}: insertion inside the sweep window");
            let rad = ((pa[1] - ca[1]).powi(2) + (pa[2] - ca[2]).powi(2)).sqrt();
            assert!(
                (rad - radius).abs() <= 1e-12,
                "I2/I5: insertion exactly on arc {ei}'s circle"
            );
            assert!(
                (pa[0] - ca[0]).abs() <= 1e-12,
                "insertion in arc {ei}'s plane"
            );
        }
        if ei >= 10 {
            // Outer arcs contain their own DIRECT junction at
            // (x, −1.5, √(r²−2.25)) bit-near exactly.
            let w = (radius * radius - 2.25).sqrt();
            assert!(
                pts.iter().any(|pt| {
                    let pa = pt.as_array();
                    (pa[1] + 1.5).abs() <= 1e-12 && (pa[2] - w).abs() <= 1e-12
                }),
                "outer arc {ei}: direct junction at (x, −1.5, √(r²−2.25)) missing"
            );
        }
    }
}

/// §4a disc containment: a cylinder's cap DISC (circle-bounded loop)
/// admits only junctions within its radius — the R0019/R0044 shape.
#[test]
pub(crate) fn rim_junctions_plane_arm_disc_cap_containment() {
    let lathe = rj_lathe(1.0, 2.0, 0.8);
    // Cylinder along +x from x = 0.75, radius 1.3, centered at z = 1:
    // its x = 0.75 cap disc admits rim0's junction (distance 1.20 from
    // the cap center) and rim2's (1.04) but NOT rim1's (1.854 > 1.3).
    let cyl = rj_cylinder([0.75, 0.0, 1.0], [1.0, 0.0, 0.0], 1.3, 3.25);
    let (map_l, _map_c) = rim_junction_overrides(&lathe, &cyl);
    let c = 0.75f64;
    let cap_center = [0.75f64, 0.0, 1.0];
    // Every on-cap-plane insertion respects the disc radius.
    for pts in map_l.values() {
        for pt in pts {
            let pa = pt.as_array();
            if (pa[0] - c).abs() <= 1e-9 {
                let dd = [
                    pa[0] - cap_center[0],
                    pa[1] - cap_center[1],
                    pa[2] - cap_center[2],
                ];
                let dist = (dd[0] * dd[0] + dd[1] * dd[1] + dd[2] * dd[2]).sqrt();
                assert!(
                    dist <= 1.3 + 1e-9,
                    "on-cap junction outside the disc: {pa:?} (dist {dist})"
                );
            }
        }
    }
    // The in-disc junctions on rim0 ARE inserted (red oracle).
    let w0 = (1.0f64 - c * c).sqrt();
    let rim0 = map_l.get(&0).expect("rim0 carries junctions");
    for sy in [-1.0f64, 1.0] {
        assert!(
            rim0.iter().any(|p| {
                let pa = p.as_array();
                (pa[0] - c).abs() <= 1e-9 && (pa[1] - sy * w0).abs() <= 1e-9 && pa[2].abs() <= 1e-9
            }),
            "rim0 in-disc junction (c, {sy}·√(1−c²), 0) missing"
        );
    }
    // And rim1's on-cap-plane candidates (outside the disc) are NOT.
    if let Some(rim1) = map_l.get(&1) {
        assert!(
            rim1.iter().all(|p| (p.as_array()[0] - c).abs() > 1e-9),
            "rim1 candidates on the cap plane must be rejected by the disc"
        );
    }
}

/// §4d: the certificate band is the TAU_WORK floor at unit scale,
/// covers the measured ~1.2·ε·L ULP noise at the R0017 magnitude, and
/// stays orders below every measured junction sagitta at its own
/// scale (band monotonicity, spec I7).
#[test]
pub(crate) fn junction_certificate_band_is_scale_aware() {
    // Unit scale: the floor.
    let plane_unit = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -0.5,
    };
    assert_eq!(
        junction_certificate_band([0.1, 0.2, 0.5], plane_unit),
        cad_primitives::TAU_WORK
    );
    // R0017 magnitude (~4e3 coords, cone apex ~3e3): the measured
    // already-exact junction residual 1.36e-12 must certify, while
    // the measured chord sagitta 10.7 must stay ≥ 1e6× above.
    let cone_large = Surface::Cone {
        apex: Point3::new(-3216.2, -1481.6, 1664.5),
        axis_dir: Vector3::new(0.7596, 0.0, -0.6504),
        half_angle: 1.0477,
    };
    let band = junction_certificate_band([-3901.5, -2954.8, -2747.5], cone_large);
    assert!(
        band >= 1.36e-12,
        "covers evaluation-precision noise: {band}"
    );
    assert!(band <= 1e-10, "stays sub-sagitta by ≥6 orders: {band}");
    // R0047 micro magnitude (~3e-4): the floor rules, and the measured
    // 1.35e-7 sagitta can never certify.
    let cone_micro = Surface::Cone {
        apex: Point3::new(2.68e-4, -2.09e-4, 2.76e-4),
        axis_dir: Vector3::new(-0.4092, 0.0, -0.9124),
        half_angle: 0.5959,
    };
    let band_micro = junction_certificate_band([1.02e-4, -1.53e-4, 1.59e-4], cone_micro);
    assert_eq!(band_micro, cad_primitives::TAU_WORK);
    assert!(band_micro < 1.35e-7 / 1e4, "micro sagitta stays loud");
}

/// §4c: a group-consistent insertion (one azimuth on all three coaxial
/// rims) tessellates the double-frustum watertight, with every inserted
/// point a bit-exact Stage-1 mesh vertex.
#[test]
pub(crate) fn cone_bands_with_inserted_shared_rim_tessellate_watertight() {
    let lathe = rj_lathe(1.0, 2.0, 0.8);
    let th = 0.6f64;
    let mut map: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
    for (ei, r, z) in [(0u32, 1.0f64, 0.0f64), (1, 2.0, 1.0), (2, 0.8, 2.0)] {
        map.insert(ei, vec![Point3::new(r * th.cos(), r * th.sin(), z)]);
    }
    let boosted = lathe
        .rebuilt_with_rim_overrides(&map)
        .expect("group-consistent insertion tessellates");
    let mesh = boosted.as_mesh();
    for pts in map.values() {
        for pt in pts {
            assert!(
                mesh.verts.iter().any(|q| q == pt),
                "inserted point {pt:?} must be a bit-exact mesh vertex"
            );
        }
    }
    // Watertight: every directed edge pairs with its reverse.
    let mut counts: std::collections::HashMap<(u32, u32), i64> = std::collections::HashMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *counts.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    for (&(s, e), &fwd) in &counts {
        let rev = counts.get(&(e, s)).copied().unwrap_or(0);
        assert_eq!(
            fwd, rev,
            "unpaired half-edge ({s},{e}) after shared-rim insertion"
        );
    }
}
