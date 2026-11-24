//! §4.4.1: triangles a point merge leaves lapping over each other are
//! replaced by the CDT of their own boundary
//! (`stage4_construct::rebuild_patch_folds`,
//! `refs/text/yang2025_hybrid_boolean.txt:557-562`). The fixture makes the
//! fold ORGANICALLY: a manifold two-operand strip whose seam carries two
//! junctions `P`, `G` with a victim cluster between them; running the
//! production `collapse_vertex` on the victims (the weld / trim shape) folds
//! the strip — the edge (P, G) picks up more than two triangles with DISTINCT
//! kept tips, the class no exact-duplicate rule can touch. The rebuild must
//! leave a closed complex without moving a vertex or touching a boundary
//! edge.

use super::super::stage4_correct::collapse_vertex;
use crate::stage4_construct::{plan_keeps_boundary, rebuild_patch_folds, PatchRebuild};
use crate::{InputId, Mesh, Surface, TriangleAttribution, TriangleSideLabel, Vector3};
use cad_primitives::Point3;
use std::collections::{BTreeMap, HashSet};

fn p(x: f64, y: f64) -> Point3 {
    Point3::new(x, y, 0.0)
}

fn z_plane() -> Surface {
    Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    }
}

/// Vertex ids for the strip fixture.
#[allow(clippy::upper_case_acronyms)]
struct Ids {
    s0: u32,
    pp: u32,
    // The victim ids are collapsed away mid-test; kept so the struct mirrors
    // the full seam polyline s0—P—V1—V2—G—s3 the fixture doc describes.
    #[allow(dead_code)]
    v1: u32,
    #[allow(dead_code)]
    v2: u32,
    gg: u32,
    s3: u32,
}

/// The pre-collapse MANIFOLD strip: operand-A triangles above the seam
/// polyline s0—P—V1—V2—G—s3, operand-B below. The A side has a fan cell
/// between the victims with TWO kept interior verts (cu, cv) so that the
/// collapses leave two distinct-tip cross triangles (the fold), plus an upper
/// row through a third kept vert r1 (a third cross). Returns
/// (mesh, attributions, ids).
fn folded_strip_fixture() -> (Mesh, Vec<Option<TriangleAttribution>>, Ids) {
    let verts = vec![
        p(0.0, 0.0),   //  0 s0
        p(1.0, 0.0),   //  1 P   (mint)
        p(1.4, 0.0),   //  2 V1  (victim -> P)
        p(1.6, 0.0),   //  3 V2  (victim -> G)
        p(2.0, 0.0),   //  4 G   (mint)
        p(3.0, 0.0),   //  5 s3
        p(1.0, 0.5),   //  6 q1
        p(1.4, 0.5),   //  7 m1  (victim -> P)
        p(1.6, 0.5),   //  8 m2  (victim -> G)
        p(2.0, 0.5),   //  9 q2
        p(1.45, 0.3),  // 10 cu  (kept cell vert)
        p(1.55, 0.35), // 11 cv  (kept cell vert)
        p(1.5, 1.0),   // 12 r1  (kept rim vert)
        p(2.5, 1.0),   // 13 r2
        p(0.5, -1.0),  // 14 b0
        p(1.5, -1.0),  // 15 b1
        p(2.5, -1.0),  // 16 b2
    ];
    let (s0, pp, v1, v2, gg, s3) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
    let (q1, m1, m2, q2, cu, cv, r1, r2) = (6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32, 13u32);
    let (b0, b1, b2) = (14u32, 15u32, 16u32);
    let a_tris = vec![
        [s0, pp, q1],
        [pp, v1, m1],
        [pp, m1, q1],
        // The victim cell (V1,V2,m2,m1) fanned around the two kept verts.
        [v1, v2, cu],
        [v2, m2, cu],
        [m2, cv, cu],
        [m2, m1, cv],
        [m1, cu, cv],
        [v1, cu, m1],
        [v2, gg, q2],
        [v2, q2, m2],
        // Upper row.
        [q1, m1, r1],
        [m1, m2, r1],
        [m2, q2, r1],
        [q2, r2, r1],
        [gg, s3, q2],
        [s3, r2, q2],
    ];
    let b_tris = vec![
        [pp, s0, b0],
        [pp, b0, b1],
        [v1, pp, b1],
        [v2, v1, b1],
        [v2, b1, b2],
        [gg, v2, b2],
        [s3, gg, b2],
    ];
    let mut tris = Vec::new();
    let mut attr = Vec::new();
    for t in &a_tris {
        tris.push(*t);
        attr.push(Some(TriangleAttribution {
            input: InputId::A,
            face: 0,
        }));
    }
    for t in &b_tris {
        tris.push(*t);
        attr.push(Some(TriangleAttribution {
            input: InputId::B,
            face: 0,
        }));
    }
    (
        Mesh::new(verts, tris),
        attr,
        Ids {
            s0,
            pp,
            v1,
            v2,
            gg,
            s3,
        },
    )
}

/// Undirected edge → (total uses, forward uses, reverse uses).
fn edge_profile(mesh: &Mesh) -> BTreeMap<(u32, u32), (usize, usize, usize)> {
    let mut m: BTreeMap<(u32, u32), (usize, usize, usize)> = BTreeMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (u, w) = (tri[i], tri[j]);
            let e = if u < w { (u, w) } else { (w, u) };
            let ent = m.entry(e).or_default();
            ent.0 += 1;
            if (u, w) == e {
                ent.1 += 1;
            } else {
                ent.2 += 1;
            }
        }
    }
    m
}

/// The pre-collapse fixture must be a valid manifold-with-boundary: every
/// edge has 1 use (fixture rim) or 2 uses in opposite directions.
#[test]
fn fixture_strip_is_manifold_with_boundary() {
    let (mesh, _, _) = folded_strip_fixture();
    for (e, &(n, fwd, rev)) in &edge_profile(&mesh) {
        assert!(
            n == 1 || (n == 2 && fwd == 1 && rev == 1),
            "edge {e:?} has profile n={n} fwd={fwd} rev={rev}"
        );
    }
}

/// Applying the production collapses (weld/trim shape) folds the strip: the
/// mint-pair edge (P,G) ends with >2 incident triangles whose tips are
/// DISTINCT kept vertices — no exact-duplicate rule can fire.
fn collapsed_fixture() -> (Mesh, Vec<Option<TriangleAttribution>>, Ids) {
    let (mut mesh, mut attr, ids) = folded_strip_fixture();
    let mut side_labels = vec![TriangleSideLabel::default(); mesh.tris.len()];
    let (v1, v2, m1, m2) = (2u32, 3u32, 7u32, 8u32);
    collapse_vertex(&mut mesh, &mut attr, &mut side_labels, v1, ids.pp);
    collapse_vertex(&mut mesh, &mut attr, &mut side_labels, v2, ids.gg);
    collapse_vertex(&mut mesh, &mut attr, &mut side_labels, m1, ids.pp);
    collapse_vertex(&mut mesh, &mut attr, &mut side_labels, m2, ids.gg);
    (mesh, attr, ids)
}

#[test]
fn stacked_collapses_manufacture_the_fold() {
    let (mesh, _, ids) = collapsed_fixture();
    let prof = edge_profile(&mesh);
    let pg = prof
        .get(&(ids.pp.min(ids.gg), ids.pp.max(ids.gg)))
        .copied()
        .unwrap_or((0, 0, 0));
    assert!(
        pg.0 > 2,
        "the mint-pair edge must be over-used post-collapse, got {pg:?}"
    );
}

/// Operand A's triangles (the folded side) and the plan that replaces them.
fn rebuild_a(
    mesh: &Mesh,
    attr: &[Option<TriangleAttribution>],
    surface: Surface,
) -> (Vec<u32>, Result<PatchRebuild, String>) {
    let a_tris: Vec<u32> = (0..mesh.tris.len() as u32)
        .filter(|&t| attr[t as usize].is_some_and(|x| x.input == InputId::A))
        .collect();
    let bounds = vec![0.0; mesh.tris.len()];
    let plan = rebuild_patch_folds(mesh, 0, surface, &a_tris, &bounds, None);
    (a_tris, plan)
}

/// The mesh with operand A's triangles replaced by the plan's.
fn with_plan(mesh: &Mesh, a_tris: &[u32], plan: &PatchRebuild) -> Mesh {
    let gone: HashSet<u32> = a_tris.iter().copied().collect();
    let mut tris: Vec<[u32; 3]> = (0..mesh.tris.len() as u32)
        .filter(|t| !gone.contains(t))
        .map(|t| mesh.tris[t as usize])
        .collect();
    tris.extend(plan.new_tris.iter().copied());
    Mesh::new(mesh.verts.clone(), tris)
}

#[test]
fn the_fold_is_rebuilt_to_a_closed_complex() {
    let (mesh, attr, ids) = collapsed_fixture();
    let (a_tris, plan) = rebuild_a(&mesh, &attr, z_plane());
    let plan = plan.expect("the folded part rebuilds");
    assert_eq!(plan.old_tris, a_tris, "one plan for the whole patch");
    assert!(plan.new_verts.is_empty(), "no vertex is added");
    let old: Vec<[u32; 3]> = a_tris.iter().map(|&t| mesh.tris[t as usize]).collect();
    assert!(plan_keeps_boundary(&old, &plan.new_tris));
    let after = with_plan(&mesh, &a_tris, &plan);
    for (e, &(n, fwd, rev)) in &edge_profile(&after) {
        assert!(
            n == 1 || (n == 2 && fwd == 1 && rev == 1),
            "edge {e:?} has profile n={n} fwd={fwd} rev={rev} after the rebuild"
        );
    }
    // The seam's vertices are still referenced: nothing moved, nothing on
    // the boundary left.
    let live: HashSet<u32> = after.tris.iter().flatten().copied().collect();
    for v in [ids.s0, ids.pp, ids.gg, ids.s3] {
        assert!(live.contains(&v), "seam vertex {v} lost by the rebuild");
    }
}

#[test]
fn a_patch_without_a_fold_is_not_rebuilt() {
    let (mesh, attr, _) = folded_strip_fixture();
    let (_, plan) = rebuild_a(&mesh, &attr, z_plane());
    assert_eq!(plan.err().as_deref(), Some("no-fold"));
}

/// The folded strip lifted onto a CONE through the chart's own `lift`
/// (chart (θ, z) := (0.3·x, 2 + y) — away from the apex, no branch cut),
/// every triangle wound in the given sense. The rebuilt part must keep the
/// sense of the triangles it replaces whichever that is (P0059: a rebuild
/// that shipped the chart's own winding met its neighbours head-on).
fn cone_strip_fixture(reversed: bool) -> (Mesh, Vec<Option<TriangleAttribution>>, Ids, Surface) {
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let chart = crate::stage4_project::SurfaceChart::new(cone).expect("cone chart");
    let (mut mesh, attr, ids) = folded_strip_fixture();
    let verts: Vec<Point3> = mesh
        .verts
        .iter()
        .map(|p| chart.lift(cad_primitives::Point2::new(0.3 * p.x(), 2.0 + p.y())))
        .collect();
    let mut tris = mesh.tris.clone();
    if reversed {
        for t in &mut tris {
            t.swap(1, 2);
        }
    }
    mesh = Mesh::new(verts, tris);
    (mesh, attr, ids, cone)
}

fn run_cone_fold(reversed: bool) {
    let (mut mesh, mut attr, ids, cone) = cone_strip_fixture(reversed);
    let mut side_labels = vec![TriangleSideLabel::default(); mesh.tris.len()];
    let (v1, v2, m1, m2) = (2u32, 3u32, 7u32, 8u32);
    collapse_vertex(&mut mesh, &mut attr, &mut side_labels, v1, ids.pp);
    collapse_vertex(&mut mesh, &mut attr, &mut side_labels, v2, ids.gg);
    collapse_vertex(&mut mesh, &mut attr, &mut side_labels, m1, ids.pp);
    collapse_vertex(&mut mesh, &mut attr, &mut side_labels, m2, ids.gg);
    let pg = edge_profile(&mesh)
        .get(&(ids.pp.min(ids.gg), ids.pp.max(ids.gg)))
        .copied()
        .unwrap_or((0, 0, 0));
    assert!(
        pg.0 > 2,
        "the cone strip must fold like the planar one, got {pg:?}"
    );
    let (a_tris, plan) = rebuild_a(&mesh, &attr, cone);
    let plan =
        plan.unwrap_or_else(|e| panic!("the folded cone part rebuilds (reversed={reversed}): {e}"));
    // Operand B's triangles stand as they were, so an edge of the seam run
    // one way by A and the other by B says the rebuilt part kept its sense.
    let after = with_plan(&mesh, &a_tris, &plan);
    for (e, &(n, fwd, rev)) in &edge_profile(&after) {
        assert!(
            n == 1 || (n == 2 && fwd == 1 && rev == 1),
            "cone edge {e:?} has profile n={n} fwd={fwd} rev={rev} after the rebuild \
             (reversed={reversed})"
        );
    }
}

#[test]
fn a_cone_fold_is_rebuilt_in_the_sense_of_its_triangles() {
    run_cone_fold(false);
}

#[test]
fn a_cone_fold_is_rebuilt_in_the_sense_of_its_triangles_reversed() {
    run_cone_fold(true);
}

// ---- paper_chain_sample_redundant: the Yang §4.3.4 h/l/α acceptance test
// (d_p = 1e-7 = TAU_MODEL, `refs/text/yang2025_hybrid_boolean.txt:586-592,744`).

use super::super::stage4_correct::paper_chain_sample_redundant;

#[test]
fn paper_test_drops_a_needle_inside_all_three_bounds() {
    // Neighbours 1e-5 apart, sample 1e-8 off-chord: h ≪ d_p·10²,
    // l ≪ d_p·10³, α ≈ 0 — the paper's refinement would never insert it.
    let a = [0.0, 0.0, 0.0];
    let m = [5.0e-6, 1.0e-8, 0.0];
    let b = [1.0e-5, 0.0, 0.0];
    assert!(paper_chain_sample_redundant(a, m, b));
}

#[test]
fn paper_test_keeps_a_sample_beyond_the_chord_length_bound() {
    // Spacing 1.0 ≫ d_p·10³ (~1e-4): a genuinely load-bearing sample.
    let a = [0.0, 0.0, 0.0];
    let m = [0.5, 1.0e-8, 0.0];
    let b = [1.0, 0.0, 0.0];
    assert!(!paper_chain_sample_redundant(a, m, b));
}

#[test]
fn paper_test_keeps_a_sample_beyond_the_arc_height_bound() {
    // l ≈ 7.8e-5 < d_p·10³ but h = 6e-5 > d_p·10² (~1e-5): real curvature.
    let a = [0.0, 0.0, 0.0];
    let m = [5.0e-5, 6.0e-5, 0.0];
    let b = [1.0e-4, 0.0, 0.0];
    assert!(!paper_chain_sample_redundant(a, m, b));
}

#[test]
fn paper_test_keeps_a_sample_beyond_the_turning_angle_bound() {
    // Tiny h and l but a 20° turn at m (> π/18): a genuine corner sample.
    let a = [0.0, 0.0, 0.0];
    let m = [2.0e-6, 0.0, 0.0];
    let turn = 20.0_f64.to_radians();
    let b = [2.0e-6 + 2.0e-6 * turn.cos(), 2.0e-6 * turn.sin(), 0.0];
    assert!(!paper_chain_sample_redundant(a, m, b));
}
