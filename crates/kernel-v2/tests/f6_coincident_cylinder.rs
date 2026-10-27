//! Roadmap F6 (ledger N89 item 5 -> N103; matrix rows 4.2-11, 4.5.5-07, 4.5.5-10):
//! Stage-0 §4.5.5 coincident-cylinder conformal re-tessellation and axial-frame
//! projection (`refs/text/yang2025_hybrid_boolean.txt:718-732`,
//! `specs/yang_455_cosurface_cylinder.md` §3.4 / §3.6).
//!
//! End-to-end `kernel-v2` pins asserting on every single-body boolean output:
//! 1. `validate_solid` (`shells == 1`, expected `genus`, `euler_lhs == euler_rhs`),
//! 2. position-welded render-mesh closure (`open == 0 && over == 0` on both
//!    `kernel_v2::tessellate` `f64` meshes and `KernelV2Adapter::tessellate`),
//! 3. exact closed-form `geom::signed_volume` matching analytic solid volume.

use std::collections::HashMap;
use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, geom, revolve, tessellate, validate_solid, BrepArena, KernelV2Adapter,
    Profile, ProfileEdge, RenderMesh as Kv2RenderMesh, SolidId,
};
use waffle_types::kernel::{Kernel, KernelMeasure, Method, RenderMesh};
use waffle_types::{CircleProfile, ClosedProfile};

/// Verified render-mesh closure check (matching `f6_ellipse_coplanar.rs` and
/// `crates/test-harness/src/oracle.rs::check_watertight_mesh`):
/// pairs exact-bit opposite-directed edges `(u -> v)` / `(v -> u)` up front,
/// subdivides any remaining residue edges at collinear T-junction vertices
/// within `collinear_rel_tol * (1 + scale)`, and returns
/// `(exact_closed_edges, raw_residue_half_edges, open_edges, over_edges)`.
fn check_position_welded_closure(
    positions: &[f64],
    indices: &[u32],
    collinear_rel_tol: f64,
) -> (usize, usize, usize, usize) {
    assert!(!indices.is_empty() && indices.len().is_multiple_of(3));
    for &c in positions {
        assert!(c.is_finite(), "non-finite render vertex coordinate {c}");
    }
    let max_abs = positions.iter().map(|c| c.abs()).fold(0.0_f64, f64::max);
    let tol = collinear_rel_tol * (1.0 + max_abs);

    type Bits = [u64; 3];
    let pt = |i: u32| -> [f64; 3] {
        let k = (i as usize) * 3;
        [positions[k], positions[k + 1], positions[k + 2]]
    };
    let bits = |p: [f64; 3]| -> Bits { p.map(|x| if x == 0.0 { 0u64 } else { x.to_bits() }) };

    let mut dir_counts: HashMap<(Bits, Bits), usize> = HashMap::new();
    let mut undir_counts: HashMap<(Bits, Bits), usize> = HashMap::new();
    let mut half_edges: Vec<([f64; 3], [f64; 3])> = Vec::with_capacity(indices.len());

    for t in indices.chunks_exact(3) {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (pa, pb) = (pt(t[i]), pt(t[j]));
            let (ka, kb) = (bits(pa), bits(pb));
            assert_ne!(ka, kb, "degenerate zero-length render triangle edge");
            *dir_counts.entry((ka, kb)).or_insert(0) += 1;
            let u = if ka <= kb { (ka, kb) } else { (kb, ka) };
            *undir_counts.entry(u).or_insert(0) += 1;
            half_edges.push((pa, pb));
        }
    }

    let is_exact_closed = |ka: Bits, kb: Bits| -> bool {
        let u = if ka <= kb { (ka, kb) } else { (kb, ka) };
        undir_counts.get(&u) == Some(&2)
            && dir_counts.get(&(ka, kb)) == Some(&1)
            && dir_counts.get(&(kb, ka)) == Some(&1)
    };

    let exact_closed = undir_counts
        .keys()
        .filter(|&&(ka, kb)| is_exact_closed(ka, kb))
        .count();

    let residue: Vec<([f64; 3], [f64; 3])> = half_edges
        .into_iter()
        .filter(|&(pa, pb)| !is_exact_closed(bits(pa), bits(pb)))
        .collect();
    if residue.is_empty() {
        return (exact_closed, 0, 0, 0);
    }

    type QKey = [i64; 3];
    let qkey = |p: [f64; 3]| -> QKey {
        [
            (p[0] / tol).round() as i64,
            (p[1] / tol).round() as i64,
            (p[2] / tol).round() as i64,
        ]
    };
    let mut res_verts: HashMap<QKey, [f64; 3]> = HashMap::new();
    for &(pa, pb) in &residue {
        res_verts.entry(qkey(pa)).or_insert(pa);
        res_verts.entry(qkey(pb)).or_insert(pb);
    }

    let mut sub_dir: HashMap<(QKey, QKey), usize> = HashMap::new();
    let mut sub_undir: HashMap<(QKey, QKey), usize> = HashMap::new();
    for &(pa, pb) in &residue {
        let (qa, qb) = (qkey(pa), qkey(pb));
        if qa == qb {
            continue;
        }
        let ab = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let ab2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
        let mut splits: Vec<(f64, QKey)> = Vec::new();
        for (&qp, &pp) in &res_verts {
            if qp == qa || qp == qb {
                continue;
            }
            let ap = [pp[0] - pa[0], pp[1] - pa[1], pp[2] - pa[2]];
            let t = (ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / ab2;
            if t <= 0.0 || t >= 1.0 {
                continue;
            }
            let proj = [pa[0] + t * ab[0], pa[1] + t * ab[1], pa[2] + t * ab[2]];
            let d2 =
                (pp[0] - proj[0]).powi(2) + (pp[1] - proj[1]).powi(2) + (pp[2] - proj[2]).powi(2);
            if d2 <= tol * tol {
                splits.push((t, qp));
            }
        }
        splits.sort_by(|a, b| a.0.total_cmp(&b.0));
        splits.dedup_by_key(|s| s.1);

        let mut prev = qa;
        for (_, qm) in splits.into_iter().chain(std::iter::once((1.0, qb))) {
            if prev == qm {
                continue;
            }
            *sub_dir.entry((prev, qm)).or_insert(0) += 1;
            let u = if prev <= qm { (prev, qm) } else { (qm, prev) };
            *sub_undir.entry(u).or_insert(0) += 1;
            prev = qm;
        }
    }

    let open = sub_undir
        .iter()
        .filter(|&(&(u, v), &cnt)| {
            cnt != 2 || sub_dir.get(&(u, v)) != Some(&1) || sub_dir.get(&(v, u)) != Some(&1)
        })
        .count();
    let over = sub_undir.values().filter(|&&cnt| cnt > 2).count();
    (exact_closed, residue.len(), open, over)
}

fn assert_closed_2manifold_solid(
    arena: &BrepArena,
    solid: SolidId,
    expected_genus: usize,
    expected_vol: f64,
    label: &str,
) {
    let report = validate_solid(arena, solid)
        .unwrap_or_else(|e| panic!("{label}: validate_solid failed: {e:?}"));
    assert_eq!(
        report.shells, 1,
        "{label}: expected 1 shell, got {report:?}"
    );
    assert_eq!(
        report.genus, expected_genus,
        "{label}: expected genus {expected_genus}, got {report:?}"
    );
    assert_eq!(
        report.euler_lhs, report.euler_rhs,
        "{label}: Euler characteristic unbalanced: {report:?}"
    );

    let mesh: Kv2RenderMesh =
        tessellate(arena, solid).unwrap_or_else(|e| panic!("{label}: tessellate failed: {e:?}"));
    let (exact_closed, raw_residue, open, over) =
        check_position_welded_closure(&mesh.positions, &mesh.indices, 1e-9);
    assert_eq!(
        over, 0,
        "{label}: {over} over-used (>2) render sub-edges \
         (exact_closed={exact_closed}, raw_residue={raw_residue})"
    );
    assert_eq!(
        open, 0,
        "{label}: {open} open or mis-oriented render sub-edges \
         (exact_closed={exact_closed}, raw_residue={raw_residue})"
    );

    let vol = geom::signed_volume(arena, solid)
        .unwrap_or_else(|e| panic!("{label}: signed_volume failed: {e:?}"));
    let tol = 1e-11 * expected_vol.abs().max(1.0);
    assert!(
        (vol - expected_vol).abs() <= tol,
        "{label}: closed-form signed_volume {vol:.15} vs analytic {expected_vol:.15} \
         (|diff|={:.3e} > tol={tol:.3e})",
        (vol - expected_vol).abs()
    );
}

fn assert_closed_adapter_mesh(mesh: &RenderMesh, label: &str) {
    let pos: Vec<f64> = mesh.vertices.iter().map(|&c| f64::from(c)).collect();
    let (exact_closed, raw_residue, open, over) =
        check_position_welded_closure(&pos, &mesh.indices, 2e-6);
    assert_eq!(
        over, 0,
        "{label}: {over} over-used (>2) adapter render sub-edges \
         (exact_closed={exact_closed}, raw_residue={raw_residue})"
    );
    assert_eq!(
        open, 0,
        "{label}: {open} open or mis-oriented adapter render sub-edges \
         (exact_closed={exact_closed}, raw_residue={raw_residue})"
    );
}

/// Full-turn `revolve` washer in `BrepArena`: rectangle `[r_in, r_out] × [z0, z1]`
/// in the XZ plane (`u = +X`, `v = +Z`), revolved `2π` around the +Z axis through
/// `(0, 0, 0)`. Produces `axis_point = (0, 0, 0)` on the bore wall (`reversed = true`)
/// and mirrored inner-rim `Curve::Circle` normals (`top = -Z`, `bottom = +Z`).
fn build_revolved_washer(
    arena: &mut BrepArena,
    r_in: f64,
    r_out: f64,
    z0: f64,
    z1: f64,
) -> SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        vec![
            Point2::new(r_in, z0),
            Point2::new(r_out, z0),
            Point2::new(r_out, z1),
            Point2::new(r_in, z1),
        ],
        vec![],
    )
    .expect("washer XZ profile");
    revolve(
        arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0 * PI,
    )
    .expect("revolve full washer")
    .solid
}

/// Extruded circular shaft spanning `[z0, z1]` (extruded from `z0` along `+Z`,
/// so `axis_point = (0, 0, z0)` and local `extent = [0, z1 - z0]`).
fn build_extruded_shaft(arena: &mut BrepArena, radius: f64, z0: f64, z1: f64) -> SolidId {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, z0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        radius,
    )
    .expect("shaft circle profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), z1 - z0)
        .expect("extrude shaft")
        .solid
}

/// Square plate `[-half, half]² × [z0, z1]` with a centered circular hole of
/// `hole_radius` authored as 4 quarter-circle `ProfileEdge::Arc`s (`ArcPolygon`
/// Tier 2, producing a 4-face `Surface::Cylinder` bore cluster).
fn build_holed_arc_plate(
    arena: &mut BrepArena,
    half: f64,
    hole_radius: f64,
    z0: f64,
    z1: f64,
) -> SolidId {
    let p = Point2::new;
    let outer = vec![
        ProfileEdge::Line {
            a: p(-half, -half),
            b: p(half, -half),
        },
        ProfileEdge::Line {
            a: p(half, -half),
            b: p(half, half),
        },
        ProfileEdge::Line {
            a: p(half, half),
            b: p(-half, half),
        },
        ProfileEdge::Line {
            a: p(-half, half),
            b: p(-half, -half),
        },
    ];
    let c = p(0.0, 0.0);
    let r = hole_radius;
    let hole = vec![
        ProfileEdge::Arc {
            a: p(r, 0.0),
            b: p(0.0, r),
            center: c,
            radius: r,
            ccw: true,
        },
        ProfileEdge::Arc {
            a: p(0.0, r),
            b: p(-r, 0.0),
            center: c,
            radius: r,
            ccw: true,
        },
        ProfileEdge::Arc {
            a: p(-r, 0.0),
            b: p(0.0, -r),
            center: c,
            radius: r,
            ccw: true,
        },
        ProfileEdge::Arc {
            a: p(0.0, -r),
            b: p(r, 0.0),
            center: c,
            radius: r,
            ccw: true,
        },
    ];
    let profile = Profile::arc_polygon(
        Point3::new(0.0, 0.0, z0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        outer,
        vec![hole],
    )
    .expect("holed ArcPolygon plate profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), z1 - z0)
        .expect("extrude holed ArcPolygon plate")
        .solid
}

#[test]
fn revolved_bore_washer_booleans_with_taller_extruded_shaft() {
    // Isolates BOTH Defect 0 (`axis_point = (0,0,0)` on revolve washer vs
    // `axis_point = (0,0,-5)` on extruded shaft) AND Defect A (`revolve` inner
    // bore's mirrored `Curve::Circle` normals split quads along the opposite
    // diagonal from `band_strip`).
    let (r_in, r_out) = (1.0, 3.0);
    let vol_washer = PI * (r_out * r_out - r_in * r_in) * 4.0; // 32π
    let vol_shaft = PI * r_in * r_in * 10.0; // 10π

    // 1. Union(washer, shaft): opposite-normal bore membrane on z∈[-2, 2] drops,
    //    merging genus-1 washer and genus-0 shaft into 1 genus-0 solid of 42π.
    {
        let mut arena = BrepArena::new();
        let washer = build_revolved_washer(&mut arena, r_in, r_out, -2.0, 2.0);
        let shaft = build_extruded_shaft(&mut arena, r_in, -5.0, 5.0);
        let out = boolean_op(&mut arena, washer, shaft, BoolOp::Union)
            .expect("Union(revolved washer, taller extruded shaft)");
        assert_closed_2manifold_solid(
            &arena,
            out,
            0,
            vol_washer + vol_shaft,
            "Union(washer[-2,2], shaft[-5,5])",
        );
    }

    // 2. Commuted Union(shaft, washer): outer_is_a flips from false to true.
    {
        let mut arena = BrepArena::new();
        let shaft = build_extruded_shaft(&mut arena, r_in, -5.0, 5.0);
        let washer = build_revolved_washer(&mut arena, r_in, r_out, -2.0, 2.0);
        let out = boolean_op(&mut arena, shaft, washer, BoolOp::Union)
            .expect("Union(taller extruded shaft, revolved washer)");
        assert_closed_2manifold_solid(
            &arena,
            out,
            0,
            vol_washer + vol_shaft,
            "Union(shaft[-5,5], washer[-2,2])",
        );
    }

    // 3. Pure Defect-A isolator (shared axis_point = (0,0,0) on BOTH solids):
    //    washer z∈[2, 6], shaft z∈[0, 10] -> local extents [2, 6] ⊂ [0, 10] even
    //    without frame projection, so ONLY the non-identical quad diagonal on
    //    the revolved bore wall causes failure before the fix.
    {
        let mut arena = BrepArena::new();
        let washer = build_revolved_washer(&mut arena, r_in, r_out, 2.0, 6.0);
        let shaft = build_extruded_shaft(&mut arena, r_in, 0.0, 10.0);
        let out = boolean_op(&mut arena, washer, shaft, BoolOp::Union)
            .expect("Union(revolved washer[2,6], shaft[0,10])");
        assert_closed_2manifold_solid(
            &arena,
            out,
            0,
            vol_washer + vol_shaft,
            "Union(washer[2,6], shaft[0,10])",
        );
    }

    // 4. Subtract(washer, shaft) and Subtract(shaft, washer): interiors are
    //    disjoint and meet only along the opposite-normal cylinder r=1, z∈[-2,2],
    //    where §4.5.5 / PR-5 keeps the membrane on Subtract.
    {
        let mut arena = BrepArena::new();
        let washer = build_revolved_washer(&mut arena, r_in, r_out, -2.0, 2.0);
        let shaft = build_extruded_shaft(&mut arena, r_in, -5.0, 5.0);
        let out_ws = boolean_op(&mut arena, washer, shaft, BoolOp::Subtract)
            .expect("Subtract(washer, shaft)");
        assert_closed_2manifold_solid(&arena, out_ws, 1, vol_washer, "Subtract(washer, shaft)");

        let mut arena2 = BrepArena::new();
        let shaft2 = build_extruded_shaft(&mut arena2, r_in, -5.0, 5.0);
        let washer2 = build_revolved_washer(&mut arena2, r_in, r_out, -2.0, 2.0);
        let out_sw = boolean_op(&mut arena2, shaft2, washer2, BoolOp::Subtract)
            .expect("Subtract(shaft, washer)");
        assert_closed_2manifold_solid(&arena2, out_sw, 0, vol_shaft, "Subtract(shaft, washer)");
    }

    // 5. Same fixture through `KernelV2Adapter` (`revolve_face` + `extrude_face`
    //    + `boolean_union` + `tessellate` + `mass_properties`).
    {
        let mut k = KernelV2Adapter::new();
        let rect_pos = HashMap::from([
            (1u32, (r_in, -2.0)),
            (2, (r_out, -2.0)),
            (3, (r_out, 2.0)),
            (4, (r_in, 2.0)),
        ]);
        let rect_prof = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            vertex_ids: vec![1, 2, 3, 4],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let w_faces = k
            .make_faces_from_profiles(
                &[rect_prof],
                [0.0, 0.0, 0.0],
                [0.0, -1.0, 0.0], // x=+X, n=-Y => y_plane = n × x = +Z
                [1.0, 0.0, 0.0],
                &rect_pos,
            )
            .expect("stage revolve washer profile");
        let washer_h = k
            .revolve_face(w_faces[0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 360.0)
            .expect("adapter revolve washer");

        let c_prof = ClosedProfile {
            entity_ids: vec![1],
            is_outer: true,
            vertex_ids: vec![],
            circle: Some(CircleProfile {
                center_u: 0.0,
                center_v: 0.0,
                radius: r_in,
            }),
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let s_faces = k
            .make_faces_from_profiles(
                &[c_prof],
                [0.0, 0.0, -5.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &HashMap::new(),
            )
            .expect("stage shaft circle");
        let shaft_h = k
            .extrude_face(s_faces[0], [0.0, 0.0, 1.0], 10.0)
            .expect("adapter extrude shaft");

        let union_h = k
            .boolean_union(&washer_h, &shaft_h)
            .expect("adapter boolean_union(washer, shaft)");
        let rmesh = k
            .tessellate(&union_h, 0.001)
            .expect("adapter tessellate union");
        assert_closed_adapter_mesh(&rmesh, "adapter Union(washer, shaft)");
        let mp = k
            .mass_properties(&union_h, None)
            .expect("adapter mass_properties");
        let expect = vol_washer + vol_shaft;
        match mp.method {
            Method::Exact => assert!(
                (mp.volume - expect).abs() <= 1e-11 * expect,
                "adapter exact mass_properties.volume {} vs analytic {}",
                mp.volume,
                expect
            ),
            Method::Mesh { chord_bound } => assert!(
                mp.volume <= expect + 1e-6 && expect - mp.volume <= expect * chord_bound * 2.0,
                "adapter mesh mass_properties.volume {} outside chord_bound={} vs {}",
                mp.volume,
                chord_bound,
                expect
            ),
        }
    }
}

#[test]
fn holed_arc_polygon_plate_booleans_with_taller_circle_shaft() {
    // 4-face `ArcPolygon` circular bore cluster (z∈[-2, 2], axis_point=(0,0,-2))
    // vs 1-face `Profile::circle` shaft (z∈[-5, 5], axis_point=(0,0,-5) and -Z
    // sweep from z=+5). Exercises:
    // - shared-frame extent projection (`[0, 4]` at z0=-2 -> `[3, 7]` in `z0=-5`),
    // - multi-arc (`4 * ceil(N/4)`) vs 1-circle (`N`) ring-count convergence,
    // - conformal `band_strip` triangulation on both 4-face and 1-face clusters.
    let half = 3.0;
    let r = 1.0;
    let vol_plate = (4.0 * half * half - PI * r * r) * 4.0; // (36 - π) * 4
    let vol_shaft = PI * r * r * 10.0; // 10π

    // 1. Union(plate, +Z shaft)
    {
        let mut arena = BrepArena::new();
        let plate = build_holed_arc_plate(&mut arena, half, r, -2.0, 2.0);
        let shaft = build_extruded_shaft(&mut arena, r, -5.0, 5.0);
        let out = boolean_op(&mut arena, plate, shaft, BoolOp::Union)
            .expect("Union(holed ArcPolygon plate, taller shaft)");
        assert_closed_2manifold_solid(
            &arena,
            out,
            0,
            vol_plate + vol_shaft,
            "Union(holed plate[-2,2], shaft[-5,5])",
        );
    }

    // 2. Union(plate, -Z extruded shaft from z=+5 by depth 10): opposite axis_dir!
    {
        let mut arena = BrepArena::new();
        let plate = build_holed_arc_plate(&mut arena, half, r, -2.0, 2.0);
        let s_prof = Profile::circle(
            Point3::new(0.0, 0.0, 5.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, -1.0, 0.0), // normal = u × v = -Z
            Point2::new(0.0, 0.0),
            r,
        )
        .expect("top circle profile");
        let shaft_neg = extrude(&mut arena, &s_prof, Vector3::new(0.0, 0.0, -1.0), 10.0)
            .expect("extrude -Z shaft")
            .solid;
        let out = boolean_op(&mut arena, plate, shaft_neg, BoolOp::Union)
            .expect("Union(holed ArcPolygon plate, -Z shaft)");
        assert_closed_2manifold_solid(
            &arena,
            out,
            0,
            vol_plate + vol_shaft,
            "Union(holed plate[-2,2], -Z shaft[5,-5])",
        );
    }

    // 3. Subtract(plate, shaft): keeps the 4-arc cylindrical bore membrane (genus 1).
    {
        let mut arena = BrepArena::new();
        let plate = build_holed_arc_plate(&mut arena, half, r, -2.0, 2.0);
        let shaft = build_extruded_shaft(&mut arena, r, -5.0, 5.0);
        let out = boolean_op(&mut arena, plate, shaft, BoolOp::Subtract)
            .expect("Subtract(holed ArcPolygon plate, taller shaft)");
        assert_closed_2manifold_solid(&arena, out, 1, vol_plate, "Subtract(plate, shaft)");
    }
}

#[test]
fn disjoint_coaxial_extruded_cylinders_subtract_leaves_primary_intact() {
    let mut arena = BrepArena::new();
    let a = build_extruded_shaft(&mut arena, 2.0, 0.0, 3.0);
    let b = build_extruded_shaft(&mut arena, 2.0, 10.0, 13.0);
    let out = boolean_op(&mut arena, a, b, BoolOp::Subtract)
        .expect("Subtract disjoint coaxial extruded cylinders");
    assert_closed_2manifold_solid(
        &arena,
        out,
        0,
        PI * 4.0 * 3.0,
        "Subtract(cyl[0,3], cyl[10,13])",
    );
}
