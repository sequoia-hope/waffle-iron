//! `pipe` — a circle swept along a planar G1 chain of lines and arcs, built
//! as ONE solid by direct assembly (spec `specs/b2_pipe_sweep.md` §3–4).
//!
//! Oracle groups:
//! 1. Topology census (solid and hollow) for a line, an arc, line→arc→line,
//!    an S-bend, a U-bend and a 270° bend; `validate_solid` green.
//! 2. EXACT volume: `signed_volume = π(r² − rᵢ²)·L` to 1e-12 relative.
//! 3. Render mesh watertight and sane, volume within the chord band.
//! 4. Seam geometry: every seam vertex is `centre + r·n̂`; rims lie on both
//!    adjacent surfaces.
//! 5. Determinism: bit-identical arenas and meshes.
//! 6. Refusals typed, arena untouched.
//! 7. Boolean re-entry: a pipe minus a box through its straight end.

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, pipe, tessellate, validate_solid, BrepArena, Curve, KernelV2Error,
    PipePath, PipeResult, Profile, ProfileEdge, RenderMesh, SolidId, Surface,
};

const R: f64 = 0.05;
const RI: f64 = 0.03;

fn xy_path(edges: Vec<ProfileEdge>) -> PipePath {
    PipePath::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        edges,
    )
    .expect("valid path")
}

fn line(a: (f64, f64), b: (f64, f64)) -> ProfileEdge {
    ProfileEdge::Line {
        a: Point2::new(a.0, a.1),
        b: Point2::new(b.0, b.1),
    }
}

fn arc(a: (f64, f64), b: (f64, f64), c: (f64, f64), radius: f64, ccw: bool) -> ProfileEdge {
    ProfileEdge::Arc {
        a: Point2::new(a.0, a.1),
        b: Point2::new(b.0, b.1),
        center: Point2::new(c.0, c.1),
        radius,
        ccw,
    }
}

/// Straight tube of length 1.
fn straight() -> PipePath {
    xy_path(vec![line((0.0, 0.0), (1.0, 0.0))])
}

/// Quarter bend of radius 0.3 (CCW).
fn quarter() -> PipePath {
    xy_path(vec![arc((0.0, 0.0), (0.3, 0.3), (0.0, 0.3), 0.3, true)])
}

/// Handlebar: line → quarter arc → line.
fn handlebar() -> PipePath {
    xy_path(vec![
        line((-1.0, 0.0), (0.0, 0.0)),
        arc((0.0, 0.0), (0.3, 0.3), (0.0, 0.3), 0.3, true),
        line((0.3, 0.3), (0.3, 1.3)),
    ])
}

/// S-bend: line → CCW quarter → CW quarter → line (the seam must cross the
/// bend-side flip, which an outer-equator seam cannot).
fn s_bend() -> PipePath {
    xy_path(vec![
        line((-1.0, 0.0), (0.0, 0.0)),
        arc((0.0, 0.0), (0.3, 0.3), (0.0, 0.3), 0.3, true),
        arc((0.3, 0.3), (0.6, 0.6), (0.6, 0.3), 0.3, false),
        line((0.6, 0.6), (1.6, 0.6)),
    ])
}

/// U-bend: line → π arc → line back.
fn u_bend() -> PipePath {
    xy_path(vec![
        line((-1.0, 0.0), (0.0, 0.0)),
        arc((0.0, 0.0), (0.0, 0.6), (0.0, 0.3), 0.3, true),
        line((0.0, 0.6), (-1.0, 0.6)),
    ])
}

/// A 270° bend (sweep beyond π, which `ArcPolygon` forbids but a path allows).
fn three_quarter() -> PipePath {
    xy_path(vec![arc((0.0, 0.0), (-0.3, 0.3), (0.0, 0.3), 0.3, true)])
}

fn all_paths() -> Vec<(&'static str, PipePath, f64)> {
    vec![
        ("straight", straight(), 1.0),
        ("quarter", quarter(), 0.3 * PI / 2.0),
        ("handlebar", handlebar(), 2.0 + 0.3 * PI / 2.0),
        ("s_bend", s_bend(), 2.0 + 0.3 * PI),
        ("u_bend", u_bend(), 2.0 + 0.3 * PI),
        ("three_quarter", three_quarter(), 0.3 * 1.5 * PI),
    ]
}

fn build(path: &PipePath, inner: Option<f64>) -> (BrepArena, PipeResult) {
    let mut arena = BrepArena::new();
    let r = pipe(&mut arena, path, R, inner).expect("pipe builds");
    (arena, r)
}

fn exact_volume(arena: &BrepArena, s: SolidId) -> f64 {
    kernel_v2::geom::signed_volume(arena, s).expect("exact volume")
}

fn mesh_volume(m: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let i = i as usize;
        [
            m.positions[3 * i],
            m.positions[3 * i + 1],
            m.positions[3 * i + 2],
        ]
    };
    m.indices
        .chunks(3)
        .map(|t| {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let cross = [
                b[1] * c[2] - b[2] * c[1],
                b[2] * c[0] - b[0] * c[2],
                b[0] * c[1] - b[1] * c[0],
            ];
            (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]) / 6.0
        })
        .sum()
}

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

fn assert_mesh_sane(mesh: &RenderMesh, what: &str) {
    assert!(!mesh.indices.is_empty(), "{what}: empty mesh");
    for v in &mesh.positions {
        assert!(v.is_finite(), "{what}: non-finite position");
    }
    for chunk in mesh.normals.chunks_exact(3) {
        let len = (chunk[0] * chunk[0] + chunk[1] * chunk[1] + chunk[2] * chunk[2]).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-9,
            "{what}: non-unit normal {chunk:?}"
        );
    }
}

fn assert_rel(actual: f64, expected: f64, rel: f64, what: &str) {
    let err = (actual - expected).abs();
    assert!(
        err <= rel * expected.abs(),
        "{what}: {actual} vs {expected} (rel err {})",
        err / expected.abs()
    );
}

// =========================================================================
// 1. Topology census + validation
// =========================================================================

#[test]
fn solid_tube_census_every_path() {
    for (name, path, _) in all_paths() {
        let n = path.edges().len();
        let (arena, r) = build(&path, None);
        let report = validate_solid(&arena, r.solid).expect(name);
        assert_eq!(report.vertices, n + 1, "{name}: one seam vertex per joint");
        assert_eq!(report.edges, 2 * n + 1, "{name}: n+1 rims + n seams");
        assert_eq!(
            report.faces,
            n + 2,
            "{name}: caps + one lateral per segment"
        );
        assert_eq!(report.genus, 0, "{name}");
        assert_eq!(r.walls.len(), n);
        assert!(r.inner_walls.is_empty());
        for (j, (&w, e)) in r.walls.iter().zip(path.edges()).enumerate() {
            let s = arena.face(w).unwrap().surface;
            match e {
                ProfileEdge::Line { .. } => assert!(
                    matches!(
                        s,
                        Some(Surface::Cylinder {
                            reversed: false,
                            ..
                        })
                    ),
                    "{name} wall {j}: {s:?}"
                ),
                ProfileEdge::Arc { radius, .. } => match s {
                    Some(Surface::Torus {
                        major_radius,
                        minor_radius,
                        reversed: false,
                        ..
                    }) => {
                        assert_eq!(major_radius, *radius, "{name} wall {j}");
                        assert_eq!(minor_radius, R, "{name} wall {j}");
                    }
                    other => panic!("{name} wall {j}: {other:?}"),
                },
            }
        }
    }
}

#[test]
fn hollow_tube_census_every_path() {
    for (name, path, _) in all_paths() {
        let n = path.edges().len();
        let (arena, r) = build(&path, Some(RI));
        let report = validate_solid(&arena, r.solid).expect(name);
        assert_eq!(report.vertices, 2 * (n + 1), "{name}");
        assert_eq!(report.edges, 2 * (2 * n + 1), "{name}");
        assert_eq!(report.faces, 2 * n + 2, "{name}");
        assert_eq!(
            report.genus, 1,
            "{name}: an annulus swept along an open path is a solid torus"
        );
        assert_eq!(r.inner_walls.len(), n);
        for &w in &r.inner_walls {
            let s = arena.face(w).unwrap().surface;
            assert!(
                matches!(
                    s,
                    Some(Surface::Cylinder { reversed: true, .. })
                        | Some(Surface::Torus { reversed: true, .. })
                ),
                "{name}: inner wall is a cavity surface, got {s:?}"
            );
        }
        for cap in [r.start_cap, r.end_cap] {
            assert_eq!(
                arena.face(cap).unwrap().inner_loops.len(),
                1,
                "{name}: annular cap carries one ring"
            );
        }
    }
}

// =========================================================================
// 2. Exact volume
// =========================================================================

#[test]
fn exact_volume_is_pi_r2_times_length() {
    for (name, path, len) in all_paths() {
        assert_rel(path.length(), len, 1e-12, name);
        let (arena, r) = build(&path, None);
        assert_rel(
            exact_volume(&arena, r.solid),
            PI * R * R * len,
            1e-12,
            &format!("{name} solid exact volume"),
        );
        let (arena, r) = build(&path, Some(RI));
        assert_rel(
            exact_volume(&arena, r.solid),
            PI * (R * R - RI * RI) * len,
            1e-12,
            &format!("{name} hollow exact volume"),
        );
    }
}

// =========================================================================
// 3. Render mesh
// =========================================================================

#[test]
fn render_mesh_watertight_with_volume_in_band() {
    for (name, path, len) in all_paths() {
        for inner in [None, Some(RI)] {
            let (arena, r) = build(&path, inner);
            let mesh = tessellate(&arena, r.solid).expect(name);
            let what = format!("{name} inner={inner:?}");
            assert_mesh_sane(&mesh, &what);
            assert_watertight(&mesh, &what);
            let ri2 = inner.map(|x| x * x).unwrap_or(0.0);
            let exact = PI * (R * R - ri2) * len;
            let vol = mesh_volume(&mesh);
            assert!(vol > 0.0, "{what}: outward orientation");
            assert_rel(vol, exact, 1e-2, &format!("{what} mesh volume"));
        }
    }
}

// =========================================================================
// 4. Seam geometry
// =========================================================================

#[test]
fn seam_vertices_sit_on_the_binormal_and_rims_are_shared() {
    for (name, path, _) in all_paths() {
        let (arena, r) = build(&path, Some(RI));
        let report = validate_solid(&arena, r.solid).expect(name);
        // Every seam vertex: z = +r (outer chain) or +rᵢ (inner chain) — the
        // path plane is z = 0 and n̂ = +z.
        let mut outer = 0;
        let mut inner = 0;
        for v in arena.vertices.iter().flatten() {
            if v.point.z() == R {
                outer += 1;
            } else if v.point.z() == RI {
                inner += 1;
            } else {
                panic!("{name}: seam vertex off the binormal: {:?}", v.point);
            }
        }
        assert_eq!(outer + inner, report.vertices);
        assert_eq!(outer, inner);
        // Interior rims: shared by two LATERAL faces (no cap between them).
        let n = path.edges().len();
        let mut lateral_rims = 0;
        for h in arena.half_edges.iter().flatten() {
            if let Curve::Circle { .. } = h.curve {
                let twin = arena.half_edge(h.twin).unwrap();
                let fa = arena.loop_(h.loop_id).unwrap().face;
                let fb = arena.loop_(twin.loop_id).unwrap().face;
                let planar = |f| matches!(arena.face(f).unwrap().surface, Some(Surface::Plane(_)));
                if !planar(fa) && !planar(fb) {
                    lateral_rims += 1;
                }
            }
        }
        // Each interior joint's rim = 2 half-edges, two chains.
        assert_eq!(lateral_rims, 2 * 2 * (n - 1), "{name}");
    }
}

// =========================================================================
// 5. Determinism
// =========================================================================

#[test]
fn construction_and_tessellation_deterministic() {
    for (name, path, _) in all_paths() {
        let (a1, r1) = build(&path, Some(RI));
        let (a2, r2) = build(&path, Some(RI));
        assert_eq!(a1, a2, "{name}: arenas");
        assert_eq!(r1, r2, "{name}: results");
        let m1 = tessellate(&a1, r1.solid).unwrap();
        let m2 = tessellate(&a2, r2.solid).unwrap();
        assert_eq!(m1.positions, m2.positions, "{name}: mesh positions");
        assert_eq!(m1.indices, m2.indices, "{name}: mesh indices");
    }
}

// =========================================================================
// 6. Refusals
// =========================================================================

#[test]
fn refusals_are_typed_and_leave_the_arena_untouched() {
    let o = Point3::new(0.0, 0.0, 0.0);
    let (u, v) = (Vector3::new(1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0));
    let mk = |edges: Vec<ProfileEdge>| PipePath::new(o, u, v, edges);

    assert_eq!(mk(vec![]).unwrap_err(), KernelV2Error::PipePathEmpty);
    assert_eq!(
        mk(vec![
            line((0.0, 0.0), (1.0, 0.0)),
            line((2.0, 0.0), (3.0, 0.0))
        ])
        .unwrap_err(),
        KernelV2Error::PipePathNotChained { segment: 0 }
    );
    assert_eq!(
        mk(vec![
            line((0.0, 0.0), (1.0, 0.0)),
            arc((1.0, 0.0), (1.0, 2.0), (1.0, 1.0), 1.0, true),
            line((1.0, 2.0), (0.0, 2.0)),
            arc((0.0, 2.0), (0.0, 0.0), (0.0, 1.0), 1.0, true),
        ])
        .unwrap_err(),
        KernelV2Error::PipeClosedPathUnsupported
    );
    assert_eq!(
        mk(vec![line((0.0, 0.0), (0.0, 0.0))]).unwrap_err(),
        KernelV2Error::PipePathEdgeInvalid { segment: 0 }
    );
    // Endpoint off the circle.
    assert_eq!(
        mk(vec![arc((0.0, 0.0), (0.5, 0.3), (0.0, 0.3), 0.3, true)]).unwrap_err(),
        KernelV2Error::PipePathEdgeInvalid { segment: 0 }
    );
    // Right-angle corner between two lines.
    assert_eq!(
        mk(vec![
            line((0.0, 0.0), (1.0, 0.0)),
            line((1.0, 0.0), (1.0, 1.0))
        ])
        .unwrap_err(),
        KernelV2Error::PipeJoinNotTangent { joint: 1 }
    );
    // Arc traversed the wrong way round (tangent reverses at the joint).
    assert_eq!(
        mk(vec![
            line((-1.0, 0.0), (0.0, 0.0)),
            arc((0.0, 0.0), (0.3, 0.3), (0.0, 0.3), 0.3, false),
        ])
        .unwrap_err(),
        KernelV2Error::PipeJoinNotTangent { joint: 1 }
    );
    assert!(matches!(
        PipePath::new(
            o,
            u,
            Vector3::new(0.0, 2.0, 0.0),
            vec![line((0.0, 0.0), (1.0, 0.0))]
        )
        .unwrap_err(),
        KernelV2Error::ProfileCircleFrameNotOrthonormal
    ));

    let mut arena = BrepArena::new();
    let path = handlebar();
    let before = arena.clone();
    assert_eq!(
        pipe(&mut arena, &path, 0.0, None).unwrap_err(),
        KernelV2Error::PipeNonPositiveRadius
    );
    assert_eq!(
        pipe(&mut arena, &path, R, Some(R)).unwrap_err(),
        KernelV2Error::PipeInnerRadiusInvalid
    );
    assert_eq!(
        pipe(&mut arena, &path, R, Some(0.0)).unwrap_err(),
        KernelV2Error::PipeInnerRadiusInvalid
    );
    // Tube radius ≥ the bend radius of segment 1.
    assert_eq!(
        pipe(&mut arena, &path, 0.3, None).unwrap_err(),
        KernelV2Error::PipeBendRadiusTooSmall { segment: 1 }
    );
    assert_eq!(arena, before, "refusals leave the arena untouched");
}

// =========================================================================
// 7. Boolean re-entry
// =========================================================================

/// Box x∈[x0,x1], y∈[y0,y1], z∈[z0,z1].
fn box_solid(arena: &mut BrepArena, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> SolidId {
    let sq = Profile::new(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x.0, y.0),
            Point2::new(x.1, y.0),
            Point2::new(x.1, y.1),
            Point2::new(x.0, y.1),
        ],
        vec![],
    )
    .unwrap();
    extrude(arena, &sq, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .expect("box")
        .solid
}

/// `path` minus a box swallowing the first `cut` of its straight lead-in
/// along −x (the lead-in runs from x = −1 to 0 at y = 0).
fn cut_lead_in(name: &str, path: &PipePath, len: f64, cut: f64) {
    let mut arena = BrepArena::new();
    let p = pipe(&mut arena, path, R, None).unwrap().solid;
    let b = box_solid(&mut arena, (-1.5, -1.0 + cut), (-0.5, 0.5), (-0.5, 0.5));
    let out = boolean_op(&mut arena, p, b, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("{name} − box: {e:?}"));
    let report = validate_solid(&arena, out).expect("result validates");
    assert_eq!(report.genus, 0, "{name}");
    let mesh = tessellate(&arena, out).expect("result tessellates");
    assert_mesh_sane(&mesh, name);
    assert_watertight(&mesh, name);
    assert_rel(
        mesh_volume(&mesh),
        PI * R * R * (len - cut),
        1e-2,
        &format!("{name} − box mesh volume"),
    );
}

#[test]
fn straight_pipe_minus_box_reenters_yang() {
    let path = xy_path(vec![line((-1.0, 0.0), (0.0, 0.0))]);
    cut_lead_in("straight", &path, 1.0, 0.5);
}

#[test]
fn handlebar_minus_box_reenters_yang() {
    cut_lead_in("handlebar", &handlebar(), 2.0 + 0.3 * PI / 2.0, 0.5);
}

#[test]
fn pipe_minus_box_through_its_straight_end_reenters_yang() {
    // The S-bend carries a binormal-seam torus band on BOTH bend senses; the
    // box removes the first 0.5 of the straight lead-in, so the result
    // volume is exactly π r² (L − 0.5) and every torus face survives
    // untouched through yang Stage 1 (the φ₀ = ±π/2 seam).
    let path = s_bend();
    let len = 2.0 + 0.3 * PI;
    let mut arena = BrepArena::new();
    let p = pipe(&mut arena, &path, R, None).unwrap().solid;
    let b = box_solid(&mut arena, (-1.5, -0.5), (-0.5, 0.5), (-0.5, 0.5));
    let out = boolean_op(&mut arena, p, b, BoolOp::Subtract).expect("pipe − box");
    let report = validate_solid(&arena, out).expect("result validates");
    assert_eq!(report.genus, 0);
    let mesh = tessellate(&arena, out).expect("result tessellates");
    assert_mesh_sane(&mesh, "pipe − box");
    assert_watertight(&mesh, "pipe − box");
    assert_rel(
        mesh_volume(&mesh),
        PI * R * R * (len - 0.5),
        1e-2,
        "pipe − box mesh volume",
    );
}

#[test]
fn hollow_pipe_union_with_disjoint_box_keeps_both_volumes() {
    let path = handlebar();
    let len = 2.0 + 0.3 * PI / 2.0;
    let mut arena = BrepArena::new();
    let p = pipe(&mut arena, &path, R, Some(RI)).unwrap().solid;
    let b = box_solid(&mut arena, (2.0, 3.0), (2.0, 3.0), (2.0, 3.0));
    let out = boolean_op(&mut arena, p, b, BoolOp::Union).expect("pipe ∪ box");
    let mesh = tessellate(&arena, out).expect("result tessellates");
    assert_watertight(&mesh, "pipe ∪ box");
    assert_rel(
        mesh_volume(&mesh),
        PI * (R * R - RI * RI) * len + 1.0,
        1e-2,
        "pipe ∪ box mesh volume",
    );
}
/// Every torus band of a boolean output keeps the constructor's own
/// `[rim, seam, rim, seam]` form (the recovery pass re-mints the seam yang
/// drops as patch-interior), so the output re-enters `to_yang_brep` and
/// the lateral tessellator unchanged — pinned on the S-bend, whose two
/// bends have opposite senses and share a torus↔torus rim.
#[test]
fn boolean_output_torus_bands_keep_the_seam_form() {
    let path = s_bend();
    let mut arena = BrepArena::new();
    let p = pipe(&mut arena, &path, R, None).unwrap().solid;
    let b = box_solid(&mut arena, (-1.5, -0.5), (-0.5, 0.5), (-0.5, 0.5));
    let out = boolean_op(&mut arena, p, b, BoolOp::Subtract).expect("pipe − box");
    let mut bands = 0;
    for &sh in &arena.solid(out).unwrap().shells {
        for &f in &arena.shell(sh).unwrap().faces {
            let face = arena.face(f).unwrap();
            if !matches!(face.surface, Some(Surface::Torus { .. })) {
                continue;
            }
            bands += 1;
            assert!(face.inner_loops.is_empty(), "band {f:?} carries a ring");
            let hes = arena.loop_half_edges(face.outer_loop).unwrap();
            let kinds: Vec<&str> = hes
                .iter()
                .map(|&h| match arena.half_edge(h).unwrap().curve {
                    Curve::Circle { .. } => "rim",
                    Curve::Arc { .. } => "seam",
                    _ => "other",
                })
                .collect();
            assert_eq!(kinds, ["rim", "seam", "rim", "seam"], "band {f:?}");
            // The seam twin pair is internal to the loop.
            assert_eq!(arena.half_edge(hes[1]).unwrap().twin, hes[3], "band {f:?}");
        }
    }
    assert_eq!(bands, 2, "both bends survive as torus bands");
}

#[test]
fn hollow_s_bend_minus_box_reenters_yang() {
    let path = s_bend();
    let len = 2.0 + 0.3 * PI;
    let mut arena = BrepArena::new();
    let p = pipe(&mut arena, &path, R, Some(RI)).unwrap().solid;
    let b = box_solid(&mut arena, (-1.5, -0.5), (-0.5, 0.5), (-0.5, 0.5));
    let out = boolean_op(&mut arena, p, b, BoolOp::Subtract).expect("hollow pipe − box");
    let report = validate_solid(&arena, out).expect("result validates");
    assert_eq!(report.genus, 1, "the bore survives");
    let mesh = tessellate(&arena, out).expect("result tessellates");
    assert_mesh_sane(&mesh, "hollow − box");
    assert_watertight(&mesh, "hollow − box");
    assert_rel(
        mesh_volume(&mesh),
        PI * (R * R - RI * RI) * (len - 0.5),
        1e-2,
        "hollow pipe − box mesh volume",
    );
}

/// A recovered pipe output re-enters a SECOND boolean (chained): the
/// handlebar loses 0.5 from each straight end in turn.
#[test]
fn chained_cuts_on_both_ends_reenter_yang() {
    let path = handlebar();
    let len = 2.0 + 0.3 * PI / 2.0;
    let mut arena = BrepArena::new();
    let p = pipe(&mut arena, &path, R, None).unwrap().solid;
    let b1 = box_solid(&mut arena, (-1.5, -0.5), (-0.5, 0.5), (-0.5, 0.5));
    let cut1 = boolean_op(&mut arena, p, b1, BoolOp::Subtract).expect("first cut");
    // Second line runs from (0.3, 0.3) to (0.3, 1.3): remove y ∈ [0.8, 1.3].
    let b2 = box_solid(&mut arena, (-0.2, 0.8), (0.8, 1.8), (-0.5, 0.5));
    let cut2 = boolean_op(&mut arena, cut1, b2, BoolOp::Subtract).expect("second cut");
    let report = validate_solid(&arena, cut2).expect("result validates");
    assert_eq!(report.genus, 0);
    let mesh = tessellate(&arena, cut2).expect("result tessellates");
    assert_watertight(&mesh, "chained cuts");
    assert_rel(
        mesh_volume(&mesh),
        PI * R * R * (len - 1.0),
        1e-2,
        "chained cuts mesh volume",
    );
}

/// Hollow variants of the lead-in cut: the bore's cavity bands and annular
/// caps go through yang and the recovery pass like the outer chain.
fn cut_lead_in_hollow(name: &str, path: &PipePath, len: f64) -> Result<(), String> {
    let mut arena = BrepArena::new();
    let p = pipe(&mut arena, path, R, Some(RI)).unwrap().solid;
    let b = box_solid(&mut arena, (-1.5, -0.5), (-0.5, 0.5), (-0.5, 0.5));
    let out =
        boolean_op(&mut arena, p, b, BoolOp::Subtract).map_err(|e| format!("{name}: {e:?}"))?;
    validate_solid(&arena, out).map_err(|e| format!("{name} validate: {e:?}"))?;
    let mesh = tessellate(&arena, out).map_err(|e| format!("{name} tess: {e:?}"))?;
    let v = mesh_volume(&mesh);
    let exact = PI * (R * R - RI * RI) * (len - 0.5);
    if (v - exact).abs() > 1e-2 * exact {
        return Err(format!("{name}: volume {v} vs {exact}"));
    }
    Ok(())
}

#[test]
fn hollow_straight_minus_box_reenters_yang() {
    let path = xy_path(vec![line((-1.0, 0.0), (0.0, 0.0))]);
    cut_lead_in_hollow("hollow straight", &path, 1.0).unwrap();
}

#[test]
fn hollow_handlebar_minus_box_reenters_yang() {
    cut_lead_in_hollow("hollow handlebar", &handlebar(), 2.0 + 0.3 * PI / 2.0).unwrap();
}

#[test]
fn hollow_handlebar_far_end_cut_reenters_yang() {
    // Cut the SECOND straight instead (y ∈ [0.8, 1.3] of the x = 0.3 line).
    let path = handlebar();
    let len = 2.0 + 0.3 * PI / 2.0;
    let mut arena = BrepArena::new();
    let p = pipe(&mut arena, &path, R, Some(RI)).unwrap().solid;
    let b = box_solid(&mut arena, (-0.2, 0.8), (0.8, 1.8), (-0.5, 0.5));
    let out = boolean_op(&mut arena, p, b, BoolOp::Subtract).expect("far-end cut");
    let mesh = tessellate(&arena, out).expect("tess");
    assert_rel(
        mesh_volume(&mesh),
        PI * (R * R - RI * RI) * (len - 0.5),
        1e-2,
        "far-end",
    );
}
