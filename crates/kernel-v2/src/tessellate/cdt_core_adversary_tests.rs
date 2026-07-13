//! KV2 CDT triangulation core — ADVERSARY block (FIP Phase 4,
//! `governance/FEATURE_IMPLEMENTATION_PROTOCOL.md` §6). Additive attack
//! coverage for the round-2 mechanisms M1 (grid-degeneracy flip pass) and
//! M3 (pinch/keyhole split). All tests PASS on the shipped implementation;
//! each is a mutation tripwire or a pathological-input guard.
//!
//! Headline finding (see the killer below): the M1 flip pass shipped with
//! ZERO committed coverage. Neutering `grid_degeneracy_flip_pass` to a no-op
//! left the ENTIRE cdt_core + round-2 suite green, because the banked M1
//! witnesses (F0016 FaceId(61), R0040 FaceId(23)) never trigger a flip — the
//! exact-predicate CDT already emits zero grid-degenerate triangles for them
//! (measured: `flips_applied=0`, `residual_degen=0`). The flip pass only
//! fires on cocircular Delaunay TIES, where spade breaks the tie onto the
//! flat chord diagonal and M1 corrects it. `killer_m1_flip_concyclic_tie`
//! banks exactly such a tie; the adversary verified it goes RED
//! (`grid_degen=1`) when the flip pass is neutered.
use super::{tessellate_planar_face, RenderMesh};
use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use cad_primitives::Point3;
use waffle_types::kernel::units::{TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN};

/// Build a single +z planar face from a projected loop of z-plane points,
/// all LineSegment half-edges (same idiom as `cdt_core_round2_red_tests`).
fn build_planar_loop(pts: &[Point3]) -> (BrepArena, FaceId) {
    let n = pts.len();
    let mut arena = BrepArena::new();
    let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
    for p in pts {
        arena.vertices.push(Some(Vertex { point: *p }));
    }
    for i in 0..n {
        arena.half_edges.push(Some(HalfEdge {
            twin: HalfEdgeId(i as u32),
            next: HalfEdgeId(((i + 1) % n) as u32),
            prev: HalfEdgeId(((i + n - 1) % n) as u32),
            origin: VertexId(i as u32),
            loop_id: lid,
            curve: Curve::LineSegment,
        }));
    }
    arena.loops.push(Some(Loop {
        face: fid,
        boundary: LoopBoundary::Edges(HalfEdgeId(0)),
        kind: LoopKind::Outer,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: UnitVector3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
        })),
        outer_loop: lid,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.shells.push(Some(Shell {
        solid,
        faces: vec![fid],
        genus: 0,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));
    (arena, fid)
}

fn projected(mesh: &RenderMesh, vid: u32) -> [f64; 2] {
    let i = vid as usize * 3;
    [mesh.positions[i], mesh.positions[i + 1]]
}
fn signed_xy(mesh: &RenderMesh, t: &[u32]) -> f64 {
    let (a, b, c) = (
        projected(mesh, t[0]),
        projected(mesh, t[1]),
        projected(mesh, t[2]),
    );
    0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
}
fn winding_uniform(mesh: &RenderMesh) -> bool {
    let all_pos = mesh
        .indices
        .chunks_exact(3)
        .all(|t| signed_xy(mesh, t) > 0.0);
    let all_neg = mesh
        .indices
        .chunks_exact(3)
        .all(|t| signed_xy(mesh, t) < 0.0);
    all_pos || all_neg
}
fn total_area(mesh: &RenderMesh) -> f64 {
    mesh.indices
        .chunks_exact(3)
        .map(|t| signed_xy(mesh, t).abs())
        .sum()
}
fn max_edge_incidence(mesh: &RenderMesh) -> usize {
    use std::collections::HashMap;
    let mut counts: HashMap<(u32, u32), usize> = HashMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for &(a, b) in &[(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    counts.values().copied().max().unwrap_or(0)
}
/// Count grid-degenerate triangles the same shape as the round-2
/// `grid_degenerate` helper / `oracle::check_no_degenerate_triangles`
/// (f32-rounded height < `(max_abs·FACTOR).max(MIN)`).
fn grid_degenerate(mesh: &RenderMesh) -> usize {
    let fp = |i: u32| -> [f32; 3] {
        let i = i as usize * 3;
        [
            mesh.positions[i] as f32,
            mesh.positions[i + 1] as f32,
            mesh.positions[i + 2] as f32,
        ]
    };
    let max_abs = mesh
        .positions
        .iter()
        .map(|&p| (p as f32).abs())
        .fold(0.0_f32, f32::max) as f64;
    let grid = (max_abs * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let mut count = 0;
    for t in mesh.indices.chunks_exact(3) {
        let (pa, pb, pc) = (fp(t[0]), fp(t[1]), fp(t[2]));
        let ax = pb[0] - pa[0];
        let ay = pb[1] - pa[1];
        let az = pb[2] - pa[2];
        let bx = pc[0] - pa[0];
        let by = pc[1] - pa[1];
        let bz = pc[2] - pa[2];
        let cx = ay * bz - az * by;
        let cy = az * bx - ax * bz;
        let cz = ax * by - ay * bx;
        let area = (cx * cx + cy * cy + cz * cz).sqrt() / 2.0;
        let ms = (ax * ax + ay * ay + az * az)
            .max(bx * bx + by * by + bz * bz)
            .max((bx - ax) * (bx - ax) + (by - ay) * (by - ay) + (bz - az) * (bz - az));
        let h = if ms > 0.0 {
            2.0 * area / ms.sqrt()
        } else {
            0.0
        };
        if (h as f64) < grid {
            count += 1;
        }
    }
    count
}

/// The concyclic flat quad: three points clustered within 0.5° on the unit
    /// circle plus one at 180°. Four cocircular vertices ⇒ a Delaunay TIE on the
    /// quad diagonal; spade breaks it onto the flat chord (the near-collinear
    /// cluster), so the raw CDT emits ONE grid-degenerate triangle that the M1
    /// flip pass then corrects to the fat diagonal.
    #[rustfmt::skip]
    fn concyclic_flat_quad() -> [Point3; 4] {
        [
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.9999957692054863, 0.0029088779843619342, 0.0),
            Point3::new(0.9999830768577442, 0.005817731354993834, 0.0),
            Point3::new(-1.0, 1.2246467991473532e-16, 0.0),
        ]
    }

/// KILLER (M1 flip pass, spec §6b M1). The mutation tripwire the cycle was
/// missing: on this cocircular tie the raw CDT emits one grid-degenerate
/// triangle; the M1 flip pass flips it to the fat diagonal. Assert the
/// emitted mesh has ZERO grid-degenerate triangles, uniform winding (I4),
/// and an area exactly equal to the input quad's shoelace area (a flip is an
/// area-preserving diagonal swap, I3). Adversary-verified: neutering
/// `grid_degeneracy_flip_pass` flips this to `grid_degenerate == 1` (RED);
/// inverting its improvement predicate either leaves the sliver or spins the
/// budget to the loud "did not converge". No previously-committed test
/// covered the flip pass at all (F0016/R0040 report `flips_applied = 0`).
#[test]
fn killer_m1_flip_concyclic_tie() {
    let pts = concyclic_flat_quad();
    let (arena, fid) = build_planar_loop(&pts);
    let mut mesh = RenderMesh::default();
    tessellate_planar_face(&arena, fid, 32, &mut mesh)
        .expect("the concyclic flat quad must tessellate");
    assert_eq!(mesh.indices.len() / 3, 2, "a quad is exactly 2 triangles");
    assert_eq!(
        grid_degenerate(&mesh),
        0,
        "M1: the flip pass must clear the cocircular-tie flat triangle \
             (RED if grid_degeneracy_flip_pass is neutered)"
    );
    assert!(winding_uniform(&mesh), "I4: flip must preserve winding");
    // Exact shoelace area of the input quad — a flip preserves it.
    let mut sh = 0.0;
    for i in 0..4 {
        let a = [pts[i].x(), pts[i].y()];
        let b = [pts[(i + 1) % 4].x(), pts[(i + 1) % 4].y()];
        sh += a[0] * b[1] - b[0] * a[1];
    }
    let quad_area = (sh / 2.0).abs();
    assert!(
        (total_area(&mesh) - quad_area).abs() < 1e-12,
        "I3: flip preserves area ({} vs {quad_area})",
        total_area(&mesh)
    );
}

/// DETERMINISM (I5) on a flip-EXERCISING fixture: the concyclic tie above
/// (the strongest determinism case in the cycle — its output depends on both
/// spade's tie-break and the M1 flip pass). Two independent builds must
/// produce byte-identical positions AND indices.
#[test]
fn determinism_m1_flip_fixture_byte_identical() {
    let run = || {
        let (arena, fid) = build_planar_loop(&concyclic_flat_quad());
        let mut mesh = RenderMesh::default();
        tessellate_planar_face(&arena, fid, 32, &mut mesh).expect("tessellates");
        mesh
    };
    let (m1, m2) = (run(), run());
    assert_eq!(
        m1.indices, m2.indices,
        "I5: byte-identical indices (flip + tie)"
    );
    assert_eq!(m1.positions, m2.positions, "I5: byte-identical positions");
}

/// M3a (spec §6b M3a) — a true FIGURE-EIGHT: two CCW lobes on OPPOSITE sides
/// of a shared pinch point. The split must triangulate each lobe separately
/// with NO overlap: total area = sum of both lobe areas (3 + 3), uniform
/// winding, watertight local pairing.
#[test]
fn adversary_m3_figure_eight_two_lobes() {
    let z = 0.0;
    let pts = [
        Point3::new(0.0, 0.0, z),   // 0 pinch
        Point3::new(2.0, -1.0, z),  // 1 ┐ east lobe (CCW)
        Point3::new(3.0, 0.0, z),   // 2 │
        Point3::new(2.0, 1.0, z),   // 3 ┘
        Point3::new(0.0, 0.0, z),   // 4 pinch twin
        Point3::new(-2.0, 1.0, z),  // 5 ┐ west lobe (CCW)
        Point3::new(-3.0, 0.0, z),  // 6 │
        Point3::new(-2.0, -1.0, z), // 7 ┘
    ];
    let (arena, fid) = build_planar_loop(&pts);
    let mut mesh = RenderMesh::default();
    tessellate_planar_face(&arena, fid, 32, &mut mesh)
        .expect("M3a: the figure-eight ring must tessellate");
    assert!(
        (total_area(&mesh) - 6.0).abs() < 1e-9,
        "M3a: two lobes area 3 + 3 = 6"
    );
    assert!(
        winding_uniform(&mesh),
        "M3a: uniform winding, no overlap-inversion"
    );
    assert!(
        max_edge_incidence(&mesh) <= 2,
        "M3a: watertight per-face partition"
    );
}

/// M3a (spec §6b M3a) — TWO distinct pinch positions in one ring (nested
/// keyholes / recursive splits): a square with two separate diamond lobes
/// protruding in at two different bottom-edge pinch points. Both pinches must
/// be split and each lobe triangulated: area = 36 − 0.75 − 0.75 = 34.5.
#[test]
fn adversary_m3_two_distinct_pinches() {
    let z = 0.0;
    let pts = [
        Point3::new(3.0, -3.0, z),  // 0
        Point3::new(3.0, 3.0, z),   // 1
        Point3::new(-3.0, 3.0, z),  // 2
        Point3::new(-3.0, -3.0, z), // 3
        Point3::new(-1.0, -3.0, z), // 4 pinch 1
        Point3::new(-1.5, -2.0, z), // 5 ┐ diamond 1 (CCW, into square)
        Point3::new(-1.0, -1.5, z), // 6 │
        Point3::new(-0.5, -2.0, z), // 7 ┘
        Point3::new(-1.0, -3.0, z), // 8 pinch 1 twin
        Point3::new(1.0, -3.0, z),  // 9 pinch 2
        Point3::new(0.5, -2.0, z),  // 10 ┐ diamond 2 (CCW)
        Point3::new(1.0, -1.5, z),  // 11 │
        Point3::new(1.5, -2.0, z),  // 12 ┘
        Point3::new(1.0, -3.0, z),  // 13 pinch 2 twin
    ];
    let (arena, fid) = build_planar_loop(&pts);
    let mut mesh = RenderMesh::default();
    tessellate_planar_face(&arena, fid, 32, &mut mesh)
        .expect("M3a: the two-pinch ring must tessellate");
    assert!(
        (total_area(&mesh) - 34.5).abs() < 1e-9,
        "M3a: 36 − 0.75 − 0.75 = 34.5"
    );
    assert!(winding_uniform(&mesh), "M3a: uniform winding");
    assert!(max_edge_incidence(&mesh) <= 2, "M3a: watertight");
}

/// GUARD (M3, spec §6b M3): a genuine TRIPLE point — one geometric position
/// visited three times in one ring (a three-petal clover). The split yields a
/// zero-area (both-lobe) sub-ring; it must fail LOUDLY, never emit an
/// overlapping silent triangulation.
#[test]
fn guard_m3_triple_pinch_stays_loud() {
    let z = 0.0;
    let pts = [
        Point3::new(0.0, 0.0, z),   // 0 P
        Point3::new(2.0, -0.6, z),  // 1 east petal
        Point3::new(2.0, 0.6, z),   // 2
        Point3::new(0.0, 0.0, z),   // 3 P
        Point3::new(-0.6, 2.0, z),  // 4 north petal
        Point3::new(0.6, 2.0, z),   // 5
        Point3::new(0.0, 0.0, z),   // 6 P
        Point3::new(-2.0, 0.6, z),  // 7 west petal
        Point3::new(-2.0, -0.6, z), // 8
    ];
    let (arena, fid) = build_planar_loop(&pts);
    let mut mesh = RenderMesh::default();
    match tessellate_planar_face(&arena, fid, 32, &mut mesh) {
        Err(KernelV2Error::TessellationFailed { face, .. }) => {
            assert_eq!(face, fid, "the guard must fail THIS face");
        }
        other => panic!("a triple-point ring must fail loudly, got {other:?}"),
    }
}

/// GUARD (M3, spec §6b M3): a pinch whose split would make a 2-vertex
/// sub-ring (a degenerate spike — pinch at ring positions i and i+2) must
/// fail LOUDLY ("fewer than 3 vertices"), never emit a degenerate spike.
#[test]
fn guard_m3_two_vertex_subring_stays_loud() {
    let z = 0.0;
    let pts = [
        Point3::new(2.0, -2.0, z),  // 0
        Point3::new(2.0, 2.0, z),   // 1
        Point3::new(-2.0, 2.0, z),  // 2
        Point3::new(-2.0, -2.0, z), // 3
        Point3::new(0.0, -2.0, z),  // 4 P
        Point3::new(0.5, -1.5, z),  // 5
        Point3::new(0.0, -2.0, z),  // 6 P twin (i=4, j=6 → [4,5] = 2 verts)
    ];
    let (arena, fid) = build_planar_loop(&pts);
    let mut mesh = RenderMesh::default();
    match tessellate_planar_face(&arena, fid, 32, &mut mesh) {
        Err(KernelV2Error::TessellationFailed { face, .. }) => {
            assert_eq!(face, fid, "the guard must fail THIS face");
        }
        other => panic!("a 2-vertex-sub-ring pinch must fail loudly, got {other:?}"),
    }
}

// Deterministic LCG — no external rng dependency.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

/// SWEEP (M1/CDT invariants): 2000 star-shaped near-collinear polygons — the
/// regime where interior grid-degenerate triangles are most likely. Whatever
/// the CDT + flip pass do, EVERY emitted mesh must stay a valid partition:
/// uniform winding (I4, no inverted triangle from a bad flip) and watertight
/// local pairing (no edge shared by >2 triangles, i.e. the flip never breaks
/// manifoldness). Deterministic seed ⇒ reproducible.
#[test]
fn sweep_flip_pass_preserves_winding_and_manifoldness() {
    let mut rng = Lcg(0x1234_5678_9abc_def0);
    let mut tessellated = 0usize;
    for _ in 0..2000 {
        let n = 5 + (rng.next() * 14.0) as usize; // 5..=18 vertices
        let mut pts = Vec::with_capacity(n);
        for k in 0..n {
            let a = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
            let base = if k % 2 == 0 {
                1.0
            } else {
                0.98 + 0.04 * rng.next()
            };
            let r = base + 1e-6 * (rng.next() - 0.5);
            pts.push(Point3::new(r * a.cos(), r * a.sin(), 0.0));
        }
        let (arena, fid) = build_planar_loop(&pts);
        let mut mesh = RenderMesh::default();
        if tessellate_planar_face(&arena, fid, 32, &mut mesh).is_ok() {
            tessellated += 1;
            assert!(
                winding_uniform(&mesh),
                "sweep: a flip inverted a triangle (I4)"
            );
            assert!(
                max_edge_incidence(&mesh) <= 2,
                "sweep: a flip broke manifoldness"
            );
        }
    }
    assert!(tessellated > 1900, "most sweep polygons should tessellate");
}
