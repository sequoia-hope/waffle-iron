//! B6 increments S2 + S5 (spec `specs/b6_general_sweep.md` §3–§5, §9): the
//! sweep ASSEMBLER — a polygon section along a chain of lines and arcs,
//! open or closed, mitred corners between straight segments, bends as
//! partial revolves, shared rims, no booleans.
//!
//! Oracle groups (spec §9):
//! 1. Topology census + `validate_solid` for every fixture (open: V = e(k+1),
//!    E = e(2k+1), F = ek + 2, χ = 2; ring: V = ek, E = 2ek, F = ek, χ = 0).
//! 2. EXACT volume against the closed form: `A·L` per straight run measured
//!    through the section CENTROID (a mitre shears both neighbours by the
//!    same linear functional, so the centroid carries the exact mean),
//!    `A·θ·R_c` per bend (Pappus), to 1e-12 relative.
//! 3. Render mesh watertight and sane, volume within the chord band.
//! 4. Surface census on a bend: axis-parallel edges become cylinders with
//!    the material sense that side needs, axis-perpendicular edges annular
//!    sectors.
//! 5. Extrude continuity: a one-line path IS the extrude (same census, same
//!    exact volume, same vertex set).
//! 6. Determinism: bit-identical arenas.
//! 7. Refusals typed, arena untouched.
//! 8. Boolean re-entry: an elbow minus a box through one leg, a ring united
//!    with a disjoint box.

use std::collections::HashMap;
use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, sweep, tessellate, validate_solid, BrepArena, KernelV2Error, Profile,
    RenderMesh, SolidId, Surface, SweepPath, SweepResult,
};
use waffle_types::sketch3d::{Chain3d, Edge3d, Edge3dKind};

// ---------------------------------------------------------------------------
// builders
// ---------------------------------------------------------------------------

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(v: V3) -> V3 {
    let l = dot(v, v).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn line(a: V3, b: V3) -> Edge3d {
    Edge3d {
        entity_id: 0,
        kind: Edge3dKind::Line,
        a,
        b,
    }
}

fn arc(a: V3, b: V3, center: V3, normal: V3, radius: f64) -> Edge3d {
    Edge3d {
        entity_id: 0,
        kind: Edge3dKind::Arc {
            center,
            normal,
            radius,
        },
        a,
        b,
    }
}

fn chain(edges: Vec<Edge3d>) -> Chain3d {
    let g1 = vec![false; edges.len().saturating_sub(1)];
    Chain3d {
        edges,
        closed: false,
        g1,
    }
}

/// A `w × h` rectangle centred at `origin` + `(du, dv)`, in the plane
/// perpendicular to unit `t`, with `u` along `u_hint` (projected) and
/// `v = t × u` so that `u × v = t` (the path leaves along `+u × v`).
/// `flip` mirrors `v`, giving the left-handed basis the assembler must
/// carry as drawn.
#[allow(clippy::too_many_arguments)]
fn rect(origin: V3, t: V3, u_hint: V3, w: f64, h: f64, du: f64, dv: f64, flip: bool) -> Profile {
    let u = unit(sub(
        u_hint,
        [
            t[0] * dot(u_hint, t),
            t[1] * dot(u_hint, t),
            t[2] * dot(u_hint, t),
        ],
    ));
    let mut v = cross(t, u);
    if flip {
        v = [-v[0], -v[1], -v[2]];
    }
    Profile::new(
        Point3::new(origin[0], origin[1], origin[2]),
        Vector3::new(u[0], u[1], u[2]),
        Vector3::new(v[0], v[1], v[2]),
        vec![
            Point2::new(du - w / 2.0, dv - h / 2.0),
            Point2::new(du + w / 2.0, dv - h / 2.0),
            Point2::new(du + w / 2.0, dv + h / 2.0),
            Point2::new(du - w / 2.0, dv + h / 2.0),
        ],
        vec![],
    )
    .expect("rectangle section")
}

/// The canonical section: a rectangle centred on the chain's start, `u`
/// along `+y` (in the path plane for the planar fixtures), `v` along `+z`.
fn section_for(c: &Chain3d, w: f64, h: f64) -> Profile {
    let t = c.edges[0].start_tangent();
    let hint = if t[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    rect(c.edges[0].a, t, hint, w, h, 0.0, 0.0, false)
}

fn build(c: &Chain3d, s: &Profile) -> (BrepArena, SweepResult) {
    let path = SweepPath::new(c, s).expect("valid sweep path");
    let mut arena = BrepArena::new();
    let r = sweep(&mut arena, &path).expect("sweep assembles");
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
            dot(a, cross(b, c)) / 6.0
        })
        .sum()
}

fn assert_watertight(mesh: &RenderMesh, what: &str) {
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
    for &v in &mesh.positions {
        assert!(v.is_finite(), "{what}: non-finite position");
    }
}

fn assert_rel(actual: f64, expected: f64, rel: f64, what: &str) {
    let err = (actual - expected).abs() / expected.abs().max(1e-300);
    assert!(
        err <= rel,
        "{what}: got {actual:.15e}, expected {expected:.15e} (rel err {err:.3e} > {rel:.1e})"
    );
}

/// Full oracle pass on one fixture: validate, census, exact volume, mesh.
fn check_open(
    name: &str,
    c: &Chain3d,
    s: &Profile,
    e: usize,
    volume: f64,
) -> (BrepArena, SweepResult) {
    let (arena, r) = build(c, s);
    let k = c.edges.len();
    let report = validate_solid(&arena, r.solid).unwrap_or_else(|err| panic!("{name}: {err:?}"));
    assert_eq!(report.vertices, e * (k + 1), "{name}: vertices");
    assert_eq!(report.edges, e * (2 * k + 1), "{name}: edges");
    assert_eq!(report.faces, e * k + 2, "{name}: faces");
    assert_eq!(report.genus, 0, "{name}: genus");
    assert_eq!(report.rings, 0, "{name}: rings");
    assert!(r.start_cap.is_some() && r.end_cap.is_some(), "{name}: caps");
    assert_eq!(r.walls.len(), k, "{name}: one wall row per segment");
    assert!(
        r.walls.iter().all(|row| row.len() == e),
        "{name}: e walls per row"
    );
    assert_rel(
        exact_volume(&arena, r.solid),
        volume,
        1e-12,
        &format!("{name}: exact volume"),
    );
    let mesh = tessellate(&arena, r.solid).unwrap_or_else(|err| panic!("{name}: {err:?}"));
    assert_mesh_sane(&mesh, name);
    assert_watertight(&mesh, name);
    assert_rel(
        mesh_volume(&mesh),
        volume,
        1e-2,
        &format!("{name}: mesh volume"),
    );
    (arena, r)
}

fn check_ring(
    name: &str,
    c: &Chain3d,
    s: &Profile,
    e: usize,
    volume: f64,
) -> (BrepArena, SweepResult) {
    let (arena, r) = build(c, s);
    let k = c.edges.len();
    let report = validate_solid(&arena, r.solid).unwrap_or_else(|err| panic!("{name}: {err:?}"));
    assert_eq!(report.vertices, e * k, "{name}: vertices");
    assert_eq!(report.edges, 2 * e * k, "{name}: edges");
    assert_eq!(report.faces, e * k, "{name}: faces");
    assert_eq!(report.genus, 1, "{name}: a ring is a solid torus");
    assert_eq!(report.rings, 0, "{name}: no inner loops");
    assert!(
        r.start_cap.is_none() && r.end_cap.is_none(),
        "{name}: a ring has no caps"
    );
    assert_eq!(r.walls.len(), k, "{name}");
    assert_rel(
        exact_volume(&arena, r.solid),
        volume,
        1e-12,
        &format!("{name}: exact volume"),
    );
    let mesh = tessellate(&arena, r.solid).unwrap_or_else(|err| panic!("{name}: {err:?}"));
    assert_mesh_sane(&mesh, name);
    assert_watertight(&mesh, name);
    assert_rel(
        mesh_volume(&mesh),
        volume,
        1e-2,
        &format!("{name}: mesh volume"),
    );
    (arena, r)
}

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

const W: f64 = 0.4;
const H: f64 = 0.3;
const A: f64 = W * H;

fn straight() -> Chain3d {
    chain(vec![line([0.0; 3], [2.0, 0.0, 0.0])])
}

/// Two unit legs meeting at a right angle in the xy plane.
fn elbow() -> Chain3d {
    chain(vec![
        line([-1.0, 0.0, 0.0], [0.0; 3]),
        line([0.0; 3], [0.0, 1.0, 0.0]),
    ])
}

/// Quarter bend of radius 0.3, CCW about +z, leaving the origin along +x.
fn quarter() -> Chain3d {
    chain(vec![arc(
        [0.0; 3],
        [0.3, 0.3, 0.0],
        [0.0, 0.3, 0.0],
        [0.0, 0.0, 1.0],
        0.3,
    )])
}

/// line → quarter bend → line, G1 throughout.
fn handlebar() -> Chain3d {
    chain(vec![
        line([-1.0, 0.0, 0.0], [0.0; 3]),
        arc(
            [0.0; 3],
            [0.3, 0.3, 0.0],
            [0.0, 0.3, 0.0],
            [0.0, 0.0, 1.0],
            0.3,
        ),
        line([0.3, 0.3, 0.0], [0.3, 1.3, 0.0]),
    ])
}

/// line → CCW quarter → CW quarter → line: both bend senses.
fn s_bend() -> Chain3d {
    chain(vec![
        line([-1.0, 0.0, 0.0], [0.0; 3]),
        arc(
            [0.0; 3],
            [0.3, 0.3, 0.0],
            [0.0, 0.3, 0.0],
            [0.0, 0.0, 1.0],
            0.3,
        ),
        arc(
            [0.3, 0.3, 0.0],
            [0.6, 0.6, 0.0],
            [0.6, 0.3, 0.0],
            [0.0, 0.0, -1.0],
            0.3,
        ),
        line([0.6, 0.6, 0.0], [1.6, 0.6, 0.0]),
    ])
}

/// A 2 × 2 square ring of four straight rails, four mitred corners.
fn frame() -> Chain3d {
    let mut c = chain(vec![
        line([0.0; 3], [2.0, 0.0, 0.0]),
        line([2.0, 0.0, 0.0], [2.0, 2.0, 0.0]),
        line([2.0, 2.0, 0.0], [0.0, 2.0, 0.0]),
        line([0.0, 2.0, 0.0], [0.0; 3]),
    ]);
    c.closed = true;
    c.g1 = vec![false; 4];
    c
}

/// A rounded 3 × 2 rectangle ring: four rails and four quarter fillets of
/// radius 0.5, tangent throughout (the sketch-fillet archetype, spec §4).
fn rounded_frame() -> Chain3d {
    let r = 0.5;
    let z = [0.0, 0.0, 1.0];
    let mut c = chain(vec![
        line([r, 0.0, 0.0], [3.0 - r, 0.0, 0.0]),
        arc([3.0 - r, 0.0, 0.0], [3.0, r, 0.0], [3.0 - r, r, 0.0], z, r),
        line([3.0, r, 0.0], [3.0, 2.0 - r, 0.0]),
        arc(
            [3.0, 2.0 - r, 0.0],
            [3.0 - r, 2.0, 0.0],
            [3.0 - r, 2.0 - r, 0.0],
            z,
            r,
        ),
        line([3.0 - r, 2.0, 0.0], [r, 2.0, 0.0]),
        arc([r, 2.0, 0.0], [0.0, 2.0 - r, 0.0], [r, 2.0 - r, 0.0], z, r),
        line([0.0, 2.0 - r, 0.0], [0.0, r, 0.0]),
        arc([0.0, r, 0.0], [r, 0.0, 0.0], [r, r, 0.0], z, r),
    ]);
    c.closed = true;
    c.g1 = vec![true; 8];
    c
}

/// A corner that turns OUT of the xy plane.
fn skew_elbow() -> Chain3d {
    chain(vec![
        line([0.0; 3], [1.0, 0.0, 0.0]),
        line([1.0, 0.0, 0.0], [1.0, 0.0, 1.0]),
    ])
}

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

// ---------------------------------------------------------------------------
// 5. extrude continuity
// ---------------------------------------------------------------------------

#[test]
fn a_one_line_path_is_the_extrude() {
    let c = straight();
    let s = section_for(&c, W, H);
    let (arena, r) = check_open("straight", &c, &s, 4, A * 2.0);

    let mut ex = BrepArena::new();
    let e = extrude(&mut ex, &s, Vector3::new(1.0, 0.0, 0.0), 2.0).expect("extrude");
    let rep_e = validate_solid(&ex, e.solid).unwrap();
    let rep_s = validate_solid(&arena, r.solid).unwrap();
    assert_eq!(
        (rep_e.vertices, rep_e.edges, rep_e.faces),
        (rep_s.vertices, rep_s.edges, rep_s.faces)
    );
    assert_eq!(exact_volume(&ex, e.solid), exact_volume(&arena, r.solid));
    // The same eight corners, exactly.
    let corners = |a: &BrepArena| {
        let mut pts: Vec<[i64; 3]> = a
            .vertices
            .iter()
            .flatten()
            .map(|v| {
                [
                    v.point.x().to_bits() as i64,
                    v.point.y().to_bits() as i64,
                    v.point.z().to_bits() as i64,
                ]
            })
            .collect();
        pts.sort();
        pts
    };
    assert_eq!(corners(&ex), corners(&arena));
    // Every wall is planar; the caps face ∓x.
    for f in &r.walls[0] {
        assert!(matches!(
            arena.face(*f).unwrap().surface,
            Some(Surface::Plane(_))
        ));
    }
    let cap = |f: Option<kernel_v2::FaceId>| match arena.face(f.unwrap()).unwrap().surface {
        Some(Surface::Plane(p)) => [p.normal.x, p.normal.y, p.normal.z],
        _ => panic!("cap is planar"),
    };
    assert_eq!(cap(r.start_cap), [-1.0, 0.0, 0.0]);
    assert_eq!(cap(r.end_cap), [1.0, 0.0, 0.0]);
}

#[test]
fn a_left_handed_section_sweeps_the_same_solid() {
    let c = straight();
    let s = rect(
        [0.0; 3],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        W,
        H,
        0.0,
        0.0,
        true,
    );
    check_open("straight (left-handed basis)", &c, &s, 4, A * 2.0);
}

// ---------------------------------------------------------------------------
// 1+2. mitred corners
// ---------------------------------------------------------------------------

#[test]
fn a_centred_section_round_a_mitred_elbow_keeps_a_times_length() {
    let c = elbow();
    let s = section_for(&c, W, H);
    let (arena, r) = check_open("elbow", &c, &s, 4, A * 2.0);
    // Both legs are planar-walled; the shared rim vertices are the SAME ids.
    for row in &r.walls {
        for f in row {
            assert!(matches!(
                arena.face(*f).unwrap().surface,
                Some(Surface::Plane(_))
            ));
        }
    }
}

/// A mitre shears every section point along the tangent by a linear
/// functional of its position, so the exact volume is `A` times the path
/// length measured through the CENTROID: here both legs lose the centroid's
/// inside-of-turn coordinate.
#[test]
fn an_off_centre_section_volume_follows_its_centroid() {
    let c = elbow();
    // Shift the rectangle 0.1 toward the inside of the turn (+y).
    let s = rect(
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        W,
        H,
        0.1,
        0.0,
        false,
    );
    check_open("offset elbow", &c, &s, 4, A * (2.0 - 2.0 * 0.1));
    // ...and away from it.
    let s = rect(
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        W,
        H,
        -0.1,
        0.05,
        false,
    );
    check_open("offset elbow (outside)", &c, &s, 4, A * (2.0 + 2.0 * 0.1));
}

#[test]
fn a_corner_out_of_the_path_plane_mitres_too() {
    let c = skew_elbow();
    let s = section_for(&c, W, H);
    check_open("skew elbow", &c, &s, 4, A * 2.0);
}

#[test]
fn a_hexagon_section_has_six_walls_per_segment() {
    let c = elbow();
    let t = [1.0, 0.0, 0.0];
    let r = 0.2;
    let pts: Vec<Point2> = (0..6)
        .map(|i| {
            let a = i as f64 * PI / 3.0;
            Point2::new(r * a.cos(), r * a.sin())
        })
        .collect();
    let s = Profile::new(
        Point3::new(-1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        pts,
        vec![],
    )
    .unwrap();
    let _ = t;
    let area = 3.0 * 3f64.sqrt() / 2.0 * r * r;
    check_open("hexagon elbow", &c, &s, 6, area * 2.0);
}

// ---------------------------------------------------------------------------
// 1+2+4. bends
// ---------------------------------------------------------------------------

#[test]
fn a_rectangle_round_a_quarter_bend_is_the_partial_revolve() {
    let c = quarter();
    // 0.2 wide across the bend plane (u along +y, toward the axis at y = 0.3),
    // 0.1 tall along the axis: clears the axis by 0.2.
    let s = section_for(&c, 0.2, 0.1);
    let volume = 0.2 * 0.1 * (PI / 2.0) * 0.3;
    let (arena, r) = check_open("quarter", &c, &s, 4, volume);
    // Working ring is CCW about +x from (−0.1,−0.05): edges are
    // 0: v=−0.05 across (perpendicular), 1: u=+0.1 along the axis (inner,
    // ρ = 0.2), 2: v=+0.05 across, 3: u=−0.1 along (outer, ρ = 0.4).
    let mut cylinders = Vec::new();
    let mut planes = 0;
    for f in &r.walls[0] {
        match arena.face(*f).unwrap().surface {
            Some(Surface::Cylinder {
                radius, reversed, ..
            }) => cylinders.push((radius, reversed)),
            Some(Surface::Plane(_)) => planes += 1,
            other => panic!("unexpected surface {other:?}"),
        }
    }
    assert_eq!(planes, 2, "two annular sectors");
    cylinders.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert_eq!(cylinders.len(), 2);
    assert_rel(cylinders[0].0, 0.2, 1e-12, "inner radius");
    assert!(
        cylinders[0].1,
        "the inner wall has its material outside: a bore"
    );
    assert_rel(cylinders[1].0, 0.4, 1e-12, "outer radius");
    assert!(!cylinders[1].1, "the outer wall is a solid lateral");
}

#[test]
fn a_handlebar_sums_its_runs_and_its_bend() {
    let c = handlebar();
    let s = section_for(&c, 0.2, 0.1);
    let volume = 0.2 * 0.1 * (1.0 + (PI / 2.0) * 0.3 + 1.0);
    check_open("handlebar", &c, &s, 4, volume);
}

#[test]
fn an_s_bend_carries_both_bend_senses() {
    let c = s_bend();
    let s = section_for(&c, 0.2, 0.1);
    let volume = 0.2 * 0.1 * (1.0 + (PI / 2.0) * 0.3 * 2.0 + 1.0);
    let (arena, r) = check_open("s-bend", &c, &s, 4, volume);
    // Each bend has one bore-sense and one solid-sense cylinder; the inner
    // side flips between the two bends.
    for (i, row) in r
        .walls
        .iter()
        .enumerate()
        .filter(|(i, _)| *i == 1 || *i == 2)
    {
        let mut bore = 0;
        for f in row {
            if let Some(Surface::Cylinder { reversed: true, .. }) = arena.face(*f).unwrap().surface
            {
                bore += 1;
            }
        }
        assert_eq!(bore, 1, "bend {i}: exactly one bore-sense wall");
    }
}

// ---------------------------------------------------------------------------
// S5. rings
// ---------------------------------------------------------------------------

#[test]
fn a_square_ring_of_rails_is_a_picture_frame() {
    let c = frame();
    let s = section_for(&c, W, H);
    check_ring("frame", &c, &s, 4, A * 8.0);
}

#[test]
fn a_rounded_ring_is_pappus_on_every_fillet() {
    let c = rounded_frame();
    let s = section_for(&c, 0.2, 0.1);
    let straight = 2.0 * (3.0 - 1.0) + 2.0 * (2.0 - 1.0);
    let bends = 4.0 * (PI / 2.0) * 0.5;
    check_ring("rounded frame", &c, &s, 4, 0.2 * 0.1 * (straight + bends));
}

#[test]
fn a_ring_is_a_ring_by_its_coordinates() {
    let mut c = frame();
    c.closed = false;
    let s = section_for(&c, W, H);
    let path = SweepPath::new(&c, &s).unwrap();
    assert!(path.closed());
    check_ring("frame (unflagged)", &c, &s, 4, A * 8.0);
}

#[test]
fn a_skew_ring_with_holonomy_is_refused() {
    let mut c = chain(vec![
        line([0.0; 3], [2.0, 0.0, 0.0]),
        line([2.0, 0.0, 0.0], [2.0, 1.0, 0.7]),
        line([2.0, 1.0, 0.7], [0.5, 1.5, 0.2]),
        line([0.5, 1.5, 0.2], [0.0; 3]),
    ]);
    c.closed = true;
    let s = section_for(&c, 0.05, 0.05);
    assert_eq!(
        SweepPath::new(&c, &s).unwrap_err(),
        KernelV2Error::SweepClosedPathTwisted
    );
}

// ---------------------------------------------------------------------------
// 7. refusals, arena untouched
// ---------------------------------------------------------------------------

fn arena_counts(a: &BrepArena) -> (usize, usize, usize, usize, usize, usize) {
    (
        a.vertices.len(),
        a.half_edges.len(),
        a.loops.len(),
        a.faces.len(),
        a.shells.len(),
        a.solids.len(),
    )
}

#[test]
fn a_circle_section_is_the_pipes_business() {
    let c = elbow();
    let s = Profile::circle(
        Point3::new(-1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        Point2::new(0.0, 0.0),
        0.1,
    )
    .unwrap();
    let path = SweepPath::new(&c, &s).unwrap();
    let mut arena = BrepArena::new();
    let before = arena_counts(&arena);
    assert!(matches!(
        sweep(&mut arena, &path).unwrap_err(),
        KernelV2Error::SweepSectionUnsupported { .. }
    ));
    assert_eq!(arena_counts(&arena), before);
}

#[test]
fn a_holed_section_is_increment_s4() {
    let c = elbow();
    let s = Profile::new(
        Point3::new(-1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        vec![
            Point2::new(-0.2, -0.2),
            Point2::new(0.2, -0.2),
            Point2::new(0.2, 0.2),
            Point2::new(-0.2, 0.2),
        ],
        vec![vec![
            Point2::new(-0.1, -0.1),
            Point2::new(0.1, -0.1),
            Point2::new(0.1, 0.1),
            Point2::new(-0.1, 0.1),
        ]],
    )
    .unwrap();
    let path = SweepPath::new(&c, &s).unwrap();
    let mut arena = BrepArena::new();
    assert!(matches!(
        sweep(&mut arena, &path).unwrap_err(),
        KernelV2Error::SweepSectionUnsupported { .. }
    ));
    assert_eq!(arena_counts(&arena), arena_counts(&BrepArena::new()));
}

#[test]
fn an_oblique_section_edge_round_a_bend_is_increment_s3() {
    let c = quarter();
    // A diamond: every edge at 45° to the bend axis.
    let s = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        vec![
            Point2::new(0.0, -0.1),
            Point2::new(0.1, 0.0),
            Point2::new(0.0, 0.1),
            Point2::new(-0.1, 0.0),
        ],
        vec![],
    )
    .unwrap();
    let path = SweepPath::new(&c, &s).unwrap();
    let mut arena = BrepArena::new();
    assert_eq!(
        sweep(&mut arena, &path).unwrap_err(),
        KernelV2Error::SweepObliqueEdgeOnBend {
            segment: 0,
            edge: 0
        }
    );
    assert_eq!(arena_counts(&arena), arena_counts(&BrepArena::new()));
    // The same diamond down a STRAIGHT run is fine: obliqueness is only a
    // question on a bend.
    let c = straight();
    let s = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        vec![
            Point2::new(0.0, -0.1),
            Point2::new(0.1, 0.0),
            Point2::new(0.0, 0.1),
            Point2::new(-0.1, 0.0),
        ],
        vec![],
    )
    .unwrap();
    check_open("diamond straight", &c, &s, 4, 0.02 * 2.0);
}

// ---------------------------------------------------------------------------
// 6. determinism
// ---------------------------------------------------------------------------

#[test]
fn construction_is_deterministic() {
    for (name, c) in [
        ("handlebar", handlebar()),
        ("rounded frame", rounded_frame()),
    ] {
        let s = section_for(&c, 0.2, 0.1);
        let (a, _) = build(&c, &s);
        let (b, _) = build(&c, &s);
        assert_eq!(format!("{a:?}"), format!("{b:?}"), "{name}");
        let ma = tessellate(&a, SolidId(0)).unwrap();
        let mb = tessellate(&b, SolidId(0)).unwrap();
        assert_eq!(ma.positions, mb.positions, "{name}");
        assert_eq!(ma.indices, mb.indices, "{name}");
    }
}

// ---------------------------------------------------------------------------
// 8. boolean re-entry
// ---------------------------------------------------------------------------

#[test]
fn an_elbow_minus_a_box_through_one_leg_reenters_yang() {
    let c = elbow();
    let s = section_for(&c, W, H);
    let mut arena = BrepArena::new();
    let path = SweepPath::new(&c, &s).unwrap();
    let e = sweep(&mut arena, &path).unwrap().solid;
    // Swallow the first 0.5 of the −x leg.
    let b = box_solid(&mut arena, (-1.5, -0.5), (-0.5, 0.5), (-0.5, 0.5));
    let out = boolean_op(&mut arena, e, b, BoolOp::Subtract).expect("elbow − box");
    let report = validate_solid(&arena, out).expect("result validates");
    assert_eq!(report.genus, 0);
    let mesh = tessellate(&arena, out).expect("result tessellates");
    assert_mesh_sane(&mesh, "elbow − box");
    assert_watertight(&mesh, "elbow − box");
    assert_rel(mesh_volume(&mesh), A * 1.5, 1e-9, "elbow − box mesh volume");
}

#[test]
fn a_handlebar_minus_a_box_through_its_bend_reenters_yang() {
    let c = handlebar();
    let s = section_for(&c, 0.2, 0.1);
    let mut arena = BrepArena::new();
    let path = SweepPath::new(&c, &s).unwrap();
    let h = sweep(&mut arena, &path).unwrap().solid;
    // A box that removes everything at y > 0.15: half the bend and the whole
    // second leg — cylinders and annular sectors all cut.
    let b = box_solid(&mut arena, (-2.0, 2.0), (0.15, 2.0), (-0.5, 0.5));
    let out = boolean_op(&mut arena, h, b, BoolOp::Subtract).expect("handlebar − box");
    validate_solid(&arena, out).expect("result validates");
    let mesh = tessellate(&arena, out).expect("result tessellates");
    assert_mesh_sane(&mesh, "handlebar − box");
    assert_watertight(&mesh, "handlebar − box");
    // Remaining: the −x leg (A·1) plus the bend below y = 0.15. The bend's
    // material at height y is a strip; integrate the exact section-swept
    // volume numerically against the mesh at the chord band.
    let expected = exact_volume(&arena, out);
    assert_rel(
        mesh_volume(&mesh),
        expected,
        1e-2,
        "handlebar − box mesh vs exact",
    );
    assert!(expected > 0.2 * 0.1 * 1.0 && expected < 0.2 * 0.1 * (1.0 + (PI / 2.0) * 0.3));
}

#[test]
fn a_ring_united_with_a_disjoint_box_keeps_both_volumes() {
    let c = rounded_frame();
    let s = section_for(&c, 0.2, 0.1);
    let mut arena = BrepArena::new();
    let path = SweepPath::new(&c, &s).unwrap();
    let ring = sweep(&mut arena, &path).unwrap().solid;
    let b = box_solid(&mut arena, (5.0, 6.0), (5.0, 6.0), (5.0, 6.0));
    let out = boolean_op(&mut arena, ring, b, BoolOp::Union).expect("ring ∪ box");
    let mesh = tessellate(&arena, out).expect("result tessellates");
    assert_watertight(&mesh, "ring ∪ box");
    let straight = 2.0 * (3.0 - 1.0) + 2.0 * (2.0 - 1.0);
    let bends = 4.0 * (PI / 2.0) * 0.5;
    assert_rel(
        mesh_volume(&mesh),
        0.2 * 0.1 * (straight + bends) + 1.0,
        1e-2,
        "ring ∪ box mesh volume",
    );
}

#[test]
fn a_frame_minus_a_box_through_one_rail_opens_the_ring() {
    let c = frame();
    let s = section_for(&c, W, H);
    let mut arena = BrepArena::new();
    let path = SweepPath::new(&c, &s).unwrap();
    let ring = sweep(&mut arena, &path).unwrap().solid;
    // Cut the middle 0.5 out of the bottom rail (y = 0, x ∈ [0.75, 1.25]).
    let b = box_solid(&mut arena, (0.75, 1.25), (-0.5, 0.5), (-0.5, 0.5));
    let out = boolean_op(&mut arena, ring, b, BoolOp::Subtract).expect("frame − box");
    let report = validate_solid(&arena, out).expect("result validates");
    assert_eq!(report.genus, 0, "an opened ring is a ball");
    let mesh = tessellate(&arena, out).expect("result tessellates");
    assert_watertight(&mesh, "frame − box");
    assert_rel(
        mesh_volume(&mesh),
        A * (8.0 - 0.5),
        1e-9,
        "frame − box mesh volume",
    );
}
