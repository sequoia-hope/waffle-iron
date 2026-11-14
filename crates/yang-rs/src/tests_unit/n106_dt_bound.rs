//! N106 D2 unit pins: per-triangle discretization-error bound `d_ε(T)` carried
//! in 1:1 lockstep with `mesh.tris`, `TriangleAttributionMap`, and
//! `TriangleSideLabelMap` across every Stage-4/5 mesh mutator.
//!
//! Owner rule (D2.3): `d_ε` is stored ONLY on surviving unsplit Stage-1 inner
//! triangles (from Stage 1's structured sampling, §4.1); EVERY triangle created
//! in Stage 4/5 — boundary or inner, by any mutator — carries its own
//! recomputed `d(T)` via `stage4_dt::d_of_t` (`0.0` on exact planes, `f64::NAN`
//! if unattributed or `d_of_t` fails).
//!
//! Paper: Yang et al. 2025 §4.1.2 (`refs/text/yang2025_hybrid_boolean.txt:357-382`)
//! and §4.4.1 (`refs/text/yang2025_hybrid_boolean.txt:566-571`).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use cherchi_rs::Mesh;

use crate::brep::{
    BRep, BRepEdge, BRepFace, BRepVertex, InputId, TriangleAttribution, TriangleAttributionMap,
    TriangleErrorBoundMap, TriangleSideLabel, TriangleSideLabelMap,
};
use crate::geom::{Curve, Surface};
use crate::stage4_construct::{
    apply_rebuild_batch, rebuild_patch_planar_bounded_ex, refill_fan_hole_seeded_with_bounds,
    update_patches_ex_with_bounds, PatchRebuild,
};
use crate::stage4_correct::{
    collapse_subresolution_intersection_segments_with_bounds,
    collapse_subtauwork_mesh_edges_with_bounds, collapse_vertex_scoped_with_bounds,
    compact_unreferenced_verts_with_bounds, retriangulate_collapsed_fan_regions_ex_with_bounds,
    weld_coincident_relocated_ex_with_bounds,
};
use crate::stage4_project::SurfaceChart;
use crate::stage4_relocate::{
    remove_doubled_membranes_scoped_with_bounds, split_pinch_vertices_with_bounds,
};
use crate::stage4_splice::SplicePatch;
use crate::tests_unit::n2_junction::{rj_box, rj_cylinder};

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn attr(input: InputId, face: u32) -> Option<TriangleAttribution> {
    Some(TriangleAttribution { input, face })
}

fn side_lbl(patch: u32, inside: [bool; 2], surface: [bool; 2]) -> TriangleSideLabel {
    TriangleSideLabel {
        cherchi_patch: patch,
        inside,
        surface,
        sheet_opposite: None,
    }
}

/// Pin (a): Every triangle created by Stage-4/5 CDT mutators — both boundary
/// triangles AND strictly inner / I2e-seeded / fan-seeded triangles — carries
/// its own recomputed `stage4_dt::d_of_t` of its chart coordinates (never `d_ε`,
/// which is reserved strictly for surviving unsplit Stage-1 inner triangles).
#[test]
fn n106_d2_pin_a_patch_cdt_all_new_tris_carry_recomputed_d_of_t() {
    // (1) Cylinder patch (r = 10, θ ∈ [0, 0.8], z ∈ [0, 6]) with an 8-vertex
    // boundary cycle and a 4-vertex strictly interior quad so the CDT emits
    // both boundary triangles (`n_boundary = 12`) and strictly inner triangles
    // (`n_inner = 2`).
    let cyl_surface = Surface::Cylinder {
        axis_point: pt(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 10.0,
    };
    let cyl_chart = SurfaceChart::new(cyl_surface).expect("cylinder chart");

    let bnd_uv = [
        (0.0, 0.0), // 0
        (0.4, 0.0), // 1
        (0.8, 0.0), // 2
        (0.8, 3.0), // 3
        (0.8, 6.0), // 4
        (0.4, 6.0), // 5
        (0.0, 6.0), // 6
        (0.0, 3.0), // 7
    ];
    let int_uv = [
        (0.3, 2.9), // 8
        (0.5, 2.9), // 9
        (0.5, 3.1), // 10
        (0.3, 3.1), // 11
    ];
    let verts: Vec<Point3> = bnd_uv
        .iter()
        .chain(int_uv.iter())
        .map(|&(u, v)| cyl_chart.lift(Point2::new(u, v)))
        .collect();

    let old_tris = vec![
        [0, 1, 8],
        [1, 9, 8],
        [1, 2, 9],
        [2, 3, 9],
        [3, 10, 9],
        [3, 4, 10],
        [4, 5, 10],
        [5, 11, 10],
        [5, 6, 11],
        [6, 7, 11],
        [7, 8, 11],
        [7, 0, 8],
        [8, 9, 10],
        [8, 10, 11],
    ];
    let mesh = Mesh::new(verts, old_tris.clone());
    let cycle: Vec<u32> = (0..8).collect();
    let cycle_set: BTreeSet<u32> = cycle.iter().copied().collect();
    let patch = SplicePatch {
        cycles: vec![cycle],
        tris: (0..old_tris.len() as u32).collect(),
        surface: cyl_surface,
    };

    let chord_band = 5.0;
    let rebuild =
        rebuild_patch_planar_bounded_ex(&mesh, 0, &patch, Some(chord_band), &BTreeSet::new())
            .expect("cylinder patch CDT must succeed");

    assert_eq!(rebuild.new_tri_bounds.len(), rebuild.new_tris.len());
    let mut n_boundary = 0usize;
    let mut n_inner = 0usize;
    for (ti, (tri, &bound)) in rebuild
        .new_tris
        .iter()
        .zip(rebuild.new_tri_bounds.iter())
        .enumerate()
    {
        let uv_tri = tri.map(|vi| cyl_chart.project(mesh.verts[vi as usize]));
        let exact_dt = crate::stage4_dt::d_of_t(&cyl_surface, uv_tri)
            .expect("d_of_t on cylinder triangle must succeed");
        let touches_boundary = tri.iter().any(|v| cycle_set.contains(v));
        if touches_boundary {
            n_boundary += 1;
        } else {
            n_inner += 1;
        }
        assert_eq!(
            bound.to_bits(),
            exact_dt.to_bits(),
            "tri {ti} ({tri:?}, boundary={touches_boundary}) bound {bound:.17e} must equal d_of_t {exact_dt:.17e}"
        );
        assert!(bound > 0.0, "curved cylinder tri must have positive d(T)");
        assert_ne!(
            bound.to_bits(),
            chord_band.to_bits(),
            "tri must carry recomputed d(T), never scalar chord_band"
        );
    }
    println!(
        "[n106_d2_pin_a] cylinder patch CDT: boundary_tris={n_boundary}, inner_tris={n_inner}"
    );
    assert_eq!(n_boundary, 12);
    assert_eq!(n_inner, 2);

    // (2) I2e-seeded cylinder patch (`radius = 2.0`, θ ∈ [0, π/2], z ∈ [0, 3.0]
    // notched drum where seedless `(θ, z)` CDT fans mid-height notch vertices
    // across wide θ and triggers I2e interior Steiner seeds `new_verts`):
    // verify `!new_verts.is_empty()` and EVERY emitted triangle (both boundary
    // AND pure-I2e-seed interior) carries its own `d_of_t`, never `gate_d_eps`.
    let drum_surface = Surface::Cylinder {
        axis_point: pt(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let drum_chart = SurfaceChart::new(drum_surface).expect("r=2 cylinder chart");
    let cyl2 = |theta: f64, z: f64| pt(2.0 * theta.cos(), 2.0 * theta.sin(), z);
    let step = std::f64::consts::PI / 8.0;
    let notch = std::f64::consts::PI / 16.0;
    let drum_mesh = Mesh::new(
        vec![
            cyl2(0.0, 0.0),        // 0
            cyl2(step, 0.0),       // 1
            cyl2(2.0 * step, 0.0), // 2
            cyl2(3.0 * step, 0.0), // 3
            cyl2(4.0 * step, 0.0), // 4
            cyl2(4.0 * step, 3.0), // 5
            cyl2(3.0 * step, 3.0), // 6
            cyl2(2.0 * step, 3.0), // 7
            cyl2(step, 3.0),       // 8
            cyl2(0.0, 3.0),        // 9
            cyl2(0.0, 2.0),        // 10
            cyl2(notch, 2.0),      // 11
            cyl2(notch, 1.0),      // 12
            cyl2(0.0, 1.0),        // 13
        ],
        vec![
            [0, 1, 13],
            [13, 1, 12],
            [12, 1, 11],
            [11, 1, 8],
            [11, 8, 10],
            [10, 8, 9],
            [1, 2, 8],
            [2, 7, 8],
            [2, 3, 7],
            [3, 6, 7],
            [3, 4, 6],
            [4, 5, 6],
        ],
    );
    let drum_cyc: Vec<u32> = (0..14).collect();
    let drum_cyc_set: BTreeSet<u32> = drum_cyc.iter().copied().collect();
    let drum_patch = SplicePatch {
        cycles: vec![drum_cyc],
        tris: (0..12).collect(),
        surface: drum_surface,
    };
    let gate_d_eps = 1.0;
    let seeded_rebuild = rebuild_patch_planar_bounded_ex(
        &drum_mesh,
        0,
        &drum_patch,
        Some(gate_d_eps),
        &BTreeSet::new(),
    )
    .expect("I2e-seeded cylinder patch CDT must succeed");
    assert!(
        !seeded_rebuild.new_verts.is_empty(),
        "I2e seed grid must mint interior Steiner vertices"
    );
    let pos_of = |vi: u32| -> Point3 {
        if vi < seeded_rebuild.plan_verts {
            drum_mesh.verts[vi as usize]
        } else {
            seeded_rebuild.new_verts[(vi - seeded_rebuild.plan_verts) as usize]
        }
    };
    let mut n_seed_inner = 0usize;
    for (ti, (tri, &bound)) in seeded_rebuild
        .new_tris
        .iter()
        .zip(seeded_rebuild.new_tri_bounds.iter())
        .enumerate()
    {
        let uv_tri = tri.map(|vi| drum_chart.project(pos_of(vi)));
        let exact_dt =
            crate::stage4_dt::d_of_t(&drum_surface, uv_tri).expect("d_of_t on seeded drum tri");
        if tri.iter().all(|v| !drum_cyc_set.contains(v)) {
            n_seed_inner += 1;
        }
        assert_eq!(
            bound.to_bits(),
            exact_dt.to_bits(),
            "seeded drum tri {ti} ({tri:?}) bound {bound:.17e} must equal d_of_t {exact_dt:.17e}"
        );
        assert_ne!(bound.to_bits(), gate_d_eps.to_bits());
    }
    assert!(
        n_seed_inner > 0,
        "expected pure I2e-seed interior triangles, got {n_seed_inner}"
    );

    // (3) Seeded `refill_fan_hole_seeded_with_bounds` on `cyl_surface`: 6-gon
    // `(0,2), (0.4,0), (0.8,2), (0.8,4), (0.4,6), (0,4)` whose unseeded `(θ,z)`
    // CDT bridges `(0,2) -- (0.8,2)` and `(0,4) -- (0.8,4)` (`Δθ = 0.8` in a
    // vertical secant plane), whereas the reference fan around `(0.4, 3.0)` has
    // max `Δθ = 0.4`, forcing centroid seed insertion (`seed_budget = 6`):
    // verify `!fan_seeds.is_empty()` and EVERY triangle carries `d_of_t`.
    let fan_poly: Vec<u32> = (0..6).collect();
    let fan_verts = vec![
        cyl_chart.lift(Point2::new(0.0, 2.0)), // 0
        cyl_chart.lift(Point2::new(0.4, 0.0)), // 1
        cyl_chart.lift(Point2::new(0.8, 2.0)), // 2
        cyl_chart.lift(Point2::new(0.8, 4.0)), // 3
        cyl_chart.lift(Point2::new(0.4, 6.0)), // 4
        cyl_chart.lift(Point2::new(0.0, 4.0)), // 5
        cyl_chart.lift(Point2::new(0.4, 3.0)), // 6 (condemned center)
    ];
    let c_id = 6u32;
    let fan_ref_tris: Vec<[u32; 3]> = (0..6u32).map(|k| [k, (k + 1) % 6, c_id]).collect();
    let fan_ref_ids: Vec<u32> = (0..6).collect();
    let fan_mesh = Mesh::new(fan_verts, fan_ref_tris);
    let fan_patch = SplicePatch {
        cycles: vec![fan_poly.clone()],
        tris: fan_ref_ids.clone(),
        surface: cyl_surface,
    };
    let seed_base = fan_mesh.verts.len() as u32;
    let (fan_tris, fan_seeds, fan_bounds) = refill_fan_hole_seeded_with_bounds(
        &fan_mesh,
        0,
        &fan_patch,
        &fan_poly,
        &fan_ref_ids,
        6,
        seed_base,
        &BTreeSet::from([c_id]),
    )
    .expect("seeded fan hole refill must succeed");
    assert!(
        !fan_seeds.is_empty(),
        "expected seeded fan refill to mint centroid seed"
    );
    assert_eq!(fan_tris.len(), fan_bounds.len());
    for (ti, (tri, &bound)) in fan_tris.iter().zip(fan_bounds.iter()).enumerate() {
        let uv_tri = tri.map(|vi| {
            let p = if vi < seed_base {
                fan_mesh.verts[vi as usize]
            } else {
                fan_seeds[(vi - seed_base) as usize]
            };
            cyl_chart.project(p)
        });
        let exact_dt =
            crate::stage4_dt::d_of_t(&cyl_surface, uv_tri).expect("d_of_t on fan refill tri");
        assert_eq!(
            bound.to_bits(),
            exact_dt.to_bits(),
            "fan refill tri {ti} bound must equal d_of_t"
        );
    }

    // (4) Cone patch (half_angle = π/6, z ∈ [2.0, 5.0], θ ∈ [0.0, 0.6]): verify
    // every triangle matches `stage4_dt::d_of_t` of its chart coords.
    let cone_surface = Surface::Cone {
        apex: pt(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_6,
    };
    let cone_chart = SurfaceChart::new(cone_surface).expect("cone chart");
    let cone_bnd_uv = [
        (0.0, 2.0),
        (0.3, 2.0),
        (0.6, 2.0),
        (0.6, 5.0),
        (0.3, 5.0),
        (0.0, 5.0),
    ];
    let cone_verts: Vec<Point3> = cone_bnd_uv
        .iter()
        .map(|&(u, v)| cone_chart.lift(Point2::new(u, v)))
        .collect();
    let cone_old_tris = vec![[0, 1, 5], [1, 4, 5], [1, 2, 4], [2, 3, 4]];
    let cone_mesh = Mesh::new(cone_verts, cone_old_tris.clone());
    let cone_patch = SplicePatch {
        cycles: vec![(0..6).collect()],
        tris: (0..cone_old_tris.len() as u32).collect(),
        surface: cone_surface,
    };
    let cone_rebuild =
        rebuild_patch_planar_bounded_ex(&cone_mesh, 0, &cone_patch, Some(5.0), &BTreeSet::new())
            .expect("cone patch CDT must succeed");
    assert_eq!(
        cone_rebuild.new_tri_bounds.len(),
        cone_rebuild.new_tris.len()
    );
    for (tri, &bound) in cone_rebuild
        .new_tris
        .iter()
        .zip(cone_rebuild.new_tri_bounds.iter())
    {
        let uv_tri = tri.map(|vi| cone_chart.project(cone_mesh.verts[vi as usize]));
        let exact_dt =
            crate::stage4_dt::d_of_t(&cone_surface, uv_tri).expect("d_of_t on cone triangle");
        assert_eq!(bound.to_bits(), exact_dt.to_bits());
        assert!(bound > 0.0);
    }
}

/// Pin (b): Distinct per-triangle error bounds survive every Stage-4/5 mesh
/// mutator in 1:1 lockstep with `mesh.tris`, `attribution`, and `side_labels`.
#[test]
fn n106_d2_pin_b_bounds_survive_stage4_mutators_and_boolean_pipeline() {
    let lbl = side_lbl(7, [false, false], [true, false]);

    // 1. `apply_rebuild_batch` with foreign survivors + multi-patch CDT + Steiner seed.
    {
        let mut mesh = Mesh::new(
            vec![
                pt(0.0, 0.0, 0.0), // 0
                pt(1.0, 0.0, 0.0), // 1
                pt(1.0, 1.0, 0.0), // 2
                pt(0.0, 1.0, 0.0), // 3
                pt(2.0, 0.0, 0.0), // 4
                pt(2.0, 1.0, 0.0), // 5
                pt(0.0, 2.0, 0.0), // 6
                pt(2.0, 2.0, 0.0), // 7
            ],
            vec![
                [0, 3, 6], // 0: foreign survivor 0
                [0, 1, 2], // 1: patch A old
                [0, 2, 3], // 2: patch A old
                [2, 5, 7], // 3: foreign survivor 1
                [1, 4, 5], // 4: patch B old
                [1, 5, 2], // 5: patch B old
            ],
        );
        let mut attribution = TriangleAttributionMap {
            attributions: vec![
                attr(InputId::A, 0),
                attr(InputId::A, 1),
                attr(InputId::A, 1),
                attr(InputId::B, 0),
                attr(InputId::B, 2),
                attr(InputId::B, 2),
            ],
        };
        let mut side_labels = TriangleSideLabelMap {
            labels: vec![lbl; 6],
        };
        let mut tri_bounds =
            TriangleErrorBoundMap::from_bounds(vec![1.125, 9.0, 9.5, 2.25, 8.0, 8.5]);
        let plan_verts = mesh.verts.len() as u32;
        let plan_tris = mesh.tris.len() as u32;
        let rebuild_a = PatchRebuild {
            patch: 0,
            old_tris: vec![1, 2],
            new_tris: vec![[0, 1, 3], [1, 2, 3]],
            new_tri_bounds: vec![0.015, 0.025],
            new_verts: Vec::new(),
            dropped: BTreeSet::new(),
            plan_verts,
            plan_tris,
        };
        let rebuild_b = PatchRebuild {
            patch: 1,
            old_tris: vec![4, 5],
            new_tris: vec![[1, 4, 8], [4, 5, 8], [5, 2, 8], [2, 1, 8]],
            new_tri_bounds: vec![0.101, 0.102, 0.103, 0.104],
            new_verts: vec![pt(1.5, 0.5, 0.0)],
            dropped: BTreeSet::new(),
            plan_verts,
            plan_tris,
        };
        apply_rebuild_batch(
            &mut mesh,
            &mut attribution,
            &mut side_labels,
            &mut tri_bounds,
            &[rebuild_a, rebuild_b],
        )
        .expect("batch apply must succeed");
        tri_bounds.assert_lockstep(mesh.tris.len(), "after apply_rebuild_batch");
        assert_eq!(
            tri_bounds.bounds,
            vec![1.125, 2.25, 0.015, 0.025, 0.101, 0.102, 0.103, 0.104],
            "foreign survivors preserve exact bounds; rebuilt patches append their new_tri_bounds"
        );
    }

    // 2. `collapse_vertex_scoped_with_bounds`: drops degenerate triangle AND
    // cancels opposite-winding membrane pleat while preserving surviving bounds.
    {
        let mut mesh = Mesh::new(
            vec![
                pt(0.0, 0.0, 0.0),  // 0 survivor
                pt(1e-8, 0.0, 0.0), // 1 victim
                pt(0.0, 1.0, 0.0),  // 2
                pt(1.0, 0.0, 0.0),  // 3
                pt(1.0, 1.0, 0.0),  // 4
                pt(0.0, 1.0, 1.0),  // 5
                pt(1.0, 0.0, 1.0),  // 6
            ],
            vec![
                [0, 1, 2], // 0: degenerates to [0, 0, 2] -> dropped
                [0, 3, 4], // 1: survives (bound 11.5)
                [0, 2, 5], // 2: pleat +
                [1, 5, 2], // 3: becomes [0, 5, 2], cancels slot 2!
                [1, 4, 6], // 4: becomes [0, 4, 6] -> survives (bound 44.5)
            ],
        );
        let mut attr_vec = vec![attr(InputId::A, 0); 5];
        let mut lbl_vec = vec![lbl; 5];
        let mut bounds_vec = vec![99.1, 11.5, 99.2, 99.3, 44.5];
        let dropped = collapse_vertex_scoped_with_bounds(
            &mut mesh,
            &mut attr_vec,
            &mut lbl_vec,
            &mut bounds_vec,
            1,
            0,
            false,
            None,
        );
        assert_eq!(dropped, 3, "1 degenerate + 2 pleat triangles dropped");
        assert_eq!(mesh.tris, vec![[0, 3, 4], [0, 4, 6]]);
        assert_eq!(bounds_vec, vec![11.5, 44.5]);
        let mut relocs = vec![(0u32, 0.5), (1u32, 0.7)];
        let remap =
            compact_unreferenced_verts_with_bounds(&mut mesh, &mut relocs, &lbl_vec, &bounds_vec);
        assert!(remap.is_some(), "unreferenced vertices 1, 2, 5 compacted");
        assert_eq!(bounds_vec.len(), mesh.tris.len());
    }

    // 3. `remove_doubled_membranes_scoped_with_bounds`: removes opposite-winding
    // coincident pair while keeping distinct bounds on real shell triangles.
    {
        let mut mesh = Mesh::new(
            vec![
                pt(0.0, 0.0, 0.0),
                pt(1.0, 0.0, 0.0),
                pt(0.0, 1.0, 0.0),
                pt(0.0, 0.0, 1.0),
                pt(0.0, 0.0, -1.0),
            ],
            vec![
                [0, 1, 3], // 0: real (bound 3.25)
                [0, 1, 2], // 1: membrane +
                [0, 2, 1], // 2: membrane -
                [1, 0, 4], // 3: real (bound 7.75)
            ],
        );
        let mut attr_vec = vec![attr(InputId::A, 0); 4];
        let mut lbl_vec = vec![lbl; 4];
        let mut bounds_vec = vec![3.25, 88.0, 89.0, 7.75];
        let removed = remove_doubled_membranes_scoped_with_bounds(
            &mut mesh,
            &mut attr_vec,
            &mut lbl_vec,
            &mut bounds_vec,
            false,
        );
        assert_eq!(removed, 2);
        assert_eq!(mesh.tris.len(), 2);
        assert_eq!(bounds_vec, vec![3.25, 7.75]);
    }

    // 4. `split_pinch_vertices_with_bounds`: splits pinch edge in place while
    // preserving 1:1 triangle-bound correspondence.
    {
        let mut pinch_mesh = Mesh::new(
            vec![
                pt(0.0, 0.0, 0.0),
                pt(1.0, 0.0, 0.0),
                pt(0.0, 1.0, 0.0),
                pt(0.0, -1.0, 0.0),
                pt(0.0, 0.0, -1.0),
                pt(0.0, 0.0, 1.0),
            ],
            vec![
                [0, 1, 2],
                [1, 0, 3],
                [0, 1, 4],
                [1, 0, 5],
                [0, 2, 5],
                [0, 4, 3],
            ],
        );
        let pinch_attr = vec![
            attr(InputId::A, 0),
            attr(InputId::A, 0),
            attr(InputId::B, 1),
            attr(InputId::B, 1),
            attr(InputId::A, 0),
            attr(InputId::A, 0),
        ];
        let pinch_lbls: Vec<TriangleSideLabel> = (0..6)
            .map(|i| {
                side_lbl(
                    500 + i,
                    [i % 2 == 0, i % 3 == 0],
                    [!(2..4).contains(&i), (2..4).contains(&i)],
                )
            })
            .collect();
        let pinch_bounds = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
        let mut relocs = Vec::new();
        let splits = split_pinch_vertices_with_bounds(
            &mut pinch_mesh,
            &mut relocs,
            &pinch_attr,
            &pinch_lbls,
            &pinch_bounds,
            true,
        );
        assert_eq!(splits, 1);
        assert_eq!(pinch_mesh.tris.len(), pinch_bounds.len());
    }

    // 5. `collapse_subtauwork_mesh_edges_with_bounds`,
    // `collapse_subresolution_intersection_segments_with_bounds`,
    // `weld_coincident_relocated_ex_with_bounds`, and
    // `retriangulate_collapsed_fan_regions_ex_with_bounds`.
    let z_plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let one_face = |surface: Surface| {
        BRep::new(
            vec![
                BRepVertex {
                    point: pt(0.0, 0.0, 0.0),
                },
                BRepVertex {
                    point: pt(1.0, 0.0, 0.0),
                },
                BRepVertex {
                    point: pt(0.0, 1.0, 0.0),
                },
            ],
            vec![
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
                BRepEdge {
                    start: 2,
                    end: 0,
                    curve: Curve::LineSegment,
                },
            ],
            vec![BRepFace {
                surface,
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            }],
        )
        .expect("one-face brep")
    };
    let brep_a = one_face(z_plane);
    let brep_b = one_face(z_plane);

    {
        // Sub-TAU_WORK collapse on edge (0, 1) at separation 1e-14 (< TAU_WORK = 1e-12).
        let mut mesh = Mesh::new(
            vec![
                pt(0.0, 0.0, 0.0),
                pt(1e-14, 0.0, 0.0),
                pt(0.0, 1.0, 0.0),
                pt(1.0, 1.0, 0.0),
            ],
            vec![[0, 1, 2], [1, 3, 2]],
        );
        let mut attr_vec = vec![attr(InputId::A, 0), attr(InputId::A, 0)];
        let mut lbl_vec = vec![lbl, lbl];
        let mut bounds_vec = vec![55.0, 66.0];
        assert!(collapse_subtauwork_mesh_edges_with_bounds(
            &mut mesh,
            &mut attr_vec,
            &mut lbl_vec,
            &mut bounds_vec,
        ));
        assert_eq!(mesh.tris, vec![[0, 3, 2]]);
        assert_eq!(bounds_vec, vec![66.0]);
    }

    {
        // Sub-TAU_MODEL intersection segment collapse on (0, 1) at separation 1e-10.
        let mut mesh = Mesh::new(
            vec![
                pt(0.0, 0.0, 0.0),
                pt(1e-10, 0.0, 0.0),
                pt(0.0, 1.0, 0.0),
                pt(1.0, 1.0, 0.0),
            ],
            vec![[0, 1, 2], [1, 3, 2]],
        );
        let mut attr_vec = vec![attr(InputId::A, 0), attr(InputId::A, 0)];
        let mut lbl_vec = vec![lbl, lbl];
        let mut bounds_vec = vec![77.0, 88.0];
        let mut curves = BTreeMap::new();
        curves.insert((0u32, 1u32), Curve::LineSegment);
        assert!(collapse_subresolution_intersection_segments_with_bounds(
            &mut mesh,
            &mut attr_vec,
            &mut lbl_vec,
            &mut bounds_vec,
            &curves,
            &brep_a,
            &brep_b,
        ));
        assert_eq!(mesh.tris, vec![[0, 3, 2]]);
        assert_eq!(bounds_vec, vec![88.0]);
    }

    {
        // Coincident relocated-vertex weld: moved vertices 0 and 1 at separation 1e-10.
        let mut mesh = Mesh::new(
            vec![
                pt(0.0, 0.0, 0.0),
                pt(1e-10, 0.0, 0.0),
                pt(0.0, 1.0, 0.0),
                pt(1.0, 1.0, 0.0),
            ],
            vec![[0, 1, 2], [1, 3, 2]],
        );
        let mut attr_vec = vec![attr(InputId::A, 0), attr(InputId::A, 0)];
        let mut lbl_vec = vec![lbl, lbl];
        let mut bounds_vec = vec![12.0, 34.0];
        let moved: HashSet<u32> = [0u32, 1u32].into_iter().collect();
        let minted: HashSet<u32> = HashSet::new();
        assert!(weld_coincident_relocated_ex_with_bounds(
            &mut mesh,
            &mut attr_vec,
            &mut lbl_vec,
            &mut bounds_vec,
            &brep_a,
            &brep_b,
            &moved,
            &minted,
            false,
            None,
        ));
        assert_eq!(mesh.tris, vec![[0, 3, 2]]);
        assert_eq!(bounds_vec, vec![34.0]);
    }

    {
        // Stacked-collapse fan retriangulation (`retriangulate_collapsed_fan_regions_ex_with_bounds`):
        // untouched triangles outside the repaired cluster keep their sentinel bounds,
        // while replacement CDT triangles on planar `z_plane` receive 0.0.
        let verts = vec![
            pt(0.0, 0.0, 0.0),   // 0
            pt(1.0, 0.0, 0.0),   // 1 P (mint)
            pt(1.4, 0.0, 0.0),   // 2 V1
            pt(1.6, 0.0, 0.0),   // 3 V2
            pt(2.0, 0.0, 0.0),   // 4 G (mint)
            pt(3.0, 0.0, 0.0),   // 5
            pt(1.0, 0.5, 0.0),   // 6
            pt(1.4, 0.5, 0.0),   // 7
            pt(1.6, 0.5, 0.0),   // 8
            pt(2.0, 0.5, 0.0),   // 9
            pt(1.45, 0.3, 0.0),  // 10
            pt(1.55, 0.35, 0.0), // 11
            pt(1.5, 1.0, 0.0),   // 12
            pt(2.5, 1.0, 0.0),   // 13
            pt(0.5, -1.0, 0.0),  // 14
            pt(1.5, -1.0, 0.0),  // 15
            pt(2.5, -1.0, 0.0),  // 16
        ];
        let a_tris = [
            [0, 1, 6],
            [1, 2, 7],
            [1, 7, 6],
            [2, 3, 10],
            [3, 8, 10],
            [8, 11, 10],
            [8, 7, 11],
            [7, 10, 11],
            [2, 10, 7],
            [3, 4, 9],
            [3, 9, 8],
            [6, 7, 12],
            [7, 8, 12],
            [8, 9, 12],
            [9, 13, 12],
            [4, 5, 9],
            [5, 13, 9],
        ];
        let b_tris = [
            [1, 0, 14],
            [1, 14, 15],
            [2, 1, 15],
            [3, 2, 15],
            [3, 15, 16],
            [4, 3, 16],
            [5, 4, 16],
        ];
        let mut fan_tris = Vec::new();
        let mut fan_attr = Vec::new();
        let mut fan_lbls = Vec::new();
        let mut fan_bounds = Vec::new();
        for (i, &t) in a_tris.iter().enumerate() {
            fan_tris.push(t);
            fan_attr.push(attr(InputId::A, 0));
            fan_lbls.push(lbl);
            fan_bounds.push(200.0 + i as f64);
        }
        for (i, &t) in b_tris.iter().enumerate() {
            fan_tris.push(t);
            fan_attr.push(attr(InputId::B, 0));
            fan_lbls.push(lbl);
            fan_bounds.push(300.0 + i as f64);
        }
        let mut fan_mesh = Mesh::new(verts, fan_tris);
        for &(victim, survivor) in &[(2, 1), (3, 4), (7, 1), (8, 4)] {
            collapse_vertex_scoped_with_bounds(
                &mut fan_mesh,
                &mut fan_attr,
                &mut fan_lbls,
                &mut fan_bounds,
                victim,
                survivor,
                false,
                None,
            );
        }
        assert_eq!(fan_bounds.len(), fan_mesh.tris.len());
        let minted: HashSet<u32> = [1u32, 4u32].into_iter().collect();
        assert!(retriangulate_collapsed_fan_regions_ex_with_bounds(
            &mut fan_mesh,
            &mut fan_attr,
            &mut fan_lbls,
            &mut fan_bounds,
            &brep_a,
            &brep_b,
            &HashSet::new(),
            &minted,
            false,
        ));
        assert_eq!(fan_bounds.len(), fan_mesh.tris.len());
        // Disjoint triangle `[9, 13, 12]` (originally A-slot 14, bound 214.0) is outside
        // the `{1, 4}` fan cluster and must retain its exact sentinel bound 214.0.
        let untouched_slot = fan_mesh
            .tris
            .iter()
            .position(|&t| t == [9, 13, 12])
            .expect("disjoint triangle [9, 13, 12] must survive");
        assert_eq!(fan_bounds[untouched_slot], 214.0);
    }

    // 6. `update_patches_ex_with_bounds`: rebuilds the moved-degenerate A(0) patch
    // (updating its triangles' bounds to planar `d(T) = 0.0`) while preserving an
    // untouched B(0) triangle's sentinel bound `0.77`.
    {
        let mut mesh = Mesh::new(
            vec![
                pt(0.0, 0.0, 0.0),
                pt(0.5, 0.0, 0.0),
                pt(1.0, 0.0, 0.0),
                pt(0.5, -1.0, 0.0),
                pt(0.0, 1.0, 0.0),
                pt(1.0, 1.0, 0.0),
                pt(5.0, 0.0, 0.0),
                pt(6.0, 0.0, 0.0),
                pt(5.0, 1.0, 0.0),
            ],
            vec![[0, 1, 4], [0, 2, 1], [2, 0, 3], [1, 2, 5], [6, 7, 8]],
        );
        let mut attr_map = TriangleAttributionMap {
            attributions: vec![
                attr(InputId::A, 0),
                attr(InputId::A, 0),
                attr(InputId::A, 0),
                attr(InputId::A, 0),
                attr(InputId::B, 0),
            ],
        };
        let b_lbl = side_lbl(8, [false, false], [false, true]);
        let mut lbl_map = TriangleSideLabelMap {
            labels: vec![lbl, lbl, lbl, lbl, b_lbl],
        };
        let mut tri_bounds = TriangleErrorBoundMap::from_bounds(vec![0.31, 0.32, 0.33, 0.34, 0.77]);
        let applied = update_patches_ex_with_bounds(
            &mut mesh,
            &mut attr_map,
            &mut lbl_map,
            &mut tri_bounds,
            &brep_a,
            &brep_b,
            false,
            &HashSet::from([1u32]),
            None,
        );
        assert_eq!(applied, 1);
        assert_eq!(tri_bounds.len(), mesh.tris.len());
        let b_slot = mesh
            .tris
            .iter()
            .position(|&t| t == [6, 7, 8])
            .expect("untouched B(0) triangle must survive");
        assert_eq!(tri_bounds.lookup(b_slot as u32), 0.77);
        for (ti, &at) in attr_map.attributions.iter().enumerate() {
            if at == attr(InputId::A, 0) {
                assert_eq!(
                    tri_bounds.lookup(ti as u32),
                    0.0,
                    "rebuilt planar A(0) triangle must carry d(T) = 0.0"
                );
            }
        }
    }

    // 7. End-to-end curved `boolean()` runs (cylinder-box and cylinder-cylinder):
    // exercises `TriangleErrorBoundMap::from_stage2_kept`, `stage4_relocate_and_correct`,
    // `run_construct_passes`, `run_fold_merge_passes`, `decimate_patch_chains_441_with_bounds`,
    // and the Stage-5 hand-off `assert_lockstep`.
    {
        let backend = cherchi_rs::NativeBoolean;
        let cyl = rj_cylinder([0.0, 0.0, -2.0], [0.0, 0.0, 1.0], 1.5, 4.0);
        let bx = rj_box([-2.0, 0.0, -1.0], [2.0, 3.0, 1.0]);
        for op in [BoolOp::Union, BoolOp::Intersect, BoolOp::Subtract] {
            let out = crate::boolean(&cyl, &bx, op, &backend)
                .expect("curved cylinder-box boolean must pass");
            assert!(!out.faces().is_empty());
        }
        let cyl2 = rj_cylinder([0.8, 0.0, -1.5], [0.0, 0.0, 1.0], 1.2, 3.0);
        let out_cc = crate::boolean(&cyl, &cyl2, BoolOp::Subtract, &backend)
            .expect("cylinder-cylinder subtract");
        assert!(!out_cc.faces().is_empty());
    }
}

/// Pin (c.1): `TriangleErrorBoundMap::assert_lockstep` panics in both debug and
/// release builds when `tri_bounds.len() != mesh.tris.len()`.
#[test]
#[should_panic(expected = "must be 1:1 with mesh.tris")]
fn n106_d2_pin_c_assert_lockstep_panics_on_length_mismatch() {
    let bounds = TriangleErrorBoundMap::from_bounds(vec![0.01, 0.02]);
    bounds.assert_lockstep(3, "pin_c_test");
}

/// Pin (c.2): `apply_rebuild_batch` panics in both debug and release builds
/// when `tri_bounds` length does not match `mesh.tris.len()`.
#[test]
#[should_panic(expected = "must be 1:1 with mesh.tris")]
fn n106_d2_pin_c_apply_rebuild_batch_panics_on_tri_bounds_mismatch() {
    let mut mesh = Mesh::new(
        vec![pt(0.0, 0.0, 0.0), pt(1.0, 0.0, 0.0), pt(0.0, 1.0, 0.0)],
        vec![[0, 1, 2]],
    );
    let mut attribution = TriangleAttributionMap {
        attributions: vec![attr(InputId::A, 0)],
    };
    let mut side_labels = TriangleSideLabelMap {
        labels: vec![side_lbl(0, [false, false], [true, false])],
    };
    let mut bad_bounds = TriangleErrorBoundMap::from_bounds(vec![0.01, 0.02]);
    let _ = apply_rebuild_batch(
        &mut mesh,
        &mut attribution,
        &mut side_labels,
        &mut bad_bounds,
        &[],
    );
}

/// Pin (c.3): `apply_rebuild_batch` panics in both debug and release builds
/// when a `PatchRebuild`'s `new_tri_bounds.len() != new_tris.len()`, rather
/// than silently truncating triangles via `.zip`.
#[test]
#[should_panic(expected = "new_tri_bounds")]
fn n106_d2_pin_c_apply_rebuild_batch_panics_on_rebuild_bounds_mismatch() {
    let mut mesh = Mesh::new(
        vec![pt(0.0, 0.0, 0.0), pt(1.0, 0.0, 0.0), pt(0.0, 1.0, 0.0)],
        vec![[0, 1, 2]],
    );
    let mut attribution = TriangleAttributionMap {
        attributions: vec![attr(InputId::A, 0)],
    };
    let mut side_labels = TriangleSideLabelMap {
        labels: vec![side_lbl(0, [false, false], [true, false])],
    };
    let mut bounds = TriangleErrorBoundMap::from_bounds(vec![0.01]);
    let bad_rebuild = PatchRebuild {
        patch: 0,
        old_tris: vec![0],
        new_tris: vec![[0, 1, 2]],
        new_tri_bounds: Vec::new(), // mismatch!
        new_verts: Vec::new(),
        dropped: BTreeSet::new(),
        plan_verts: 3,
        plan_tris: 1,
    };
    let _ = apply_rebuild_batch(
        &mut mesh,
        &mut attribution,
        &mut side_labels,
        &mut bounds,
        &[bad_rebuild],
    );
}

/// Pin (c.4): `repair_fan_cluster_ex_with_bounds` panics in both debug and
/// release builds when `tri_bounds.len() < mesh.tris.len()`, rather than
/// silently dropping triangles.
#[test]
#[should_panic(expected = "must be 1:1 with mesh.tris")]
fn n106_d2_pin_c_repair_fan_cluster_panics_on_short_tri_bounds() {
    let bx = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let mut mesh = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0),
            pt(1.0, 0.0, 0.0),
            pt(1.0, 1.0, 0.0),
            pt(0.0, 1.0, 0.0),
        ],
        vec![[0, 1, 2], [0, 2, 3]],
    );
    let mut attr_vec = vec![attr(InputId::A, 0); 2];
    let mut side_labels = vec![side_lbl(0, [false, false], [true, false]); 2];
    let mut short_bounds = vec![0.0]; // short!
    let _ = crate::stage4_correct::repair_fan_cluster_ex_with_bounds(
        &mut mesh,
        &mut attr_vec,
        &mut side_labels,
        &mut short_bounds,
        &bx,
        &bx,
        &BTreeSet::new(),
        &BTreeMap::new(),
        &HashSet::new(),
        &HashSet::new(),
        false,
        false,
    );
}

/// Review D2.2 pin: whenever `d_of_t` fails or a triangle's surface attribution
/// is missing, the stored bound is `f64::NAN` — never `0.0` (which is reserved
/// strictly for exact planar surfaces so an unknown bound can never masquerade
/// as a tight `0.0` bound).
#[test]
fn n106_d2_failed_d_of_t_and_unattributed_tris_store_nan_never_zero() {
    use cherchi_rs::labeled_arrangement::LabeledArrangement;

    let tri_pts = [pt(1.0, 0.0, -2.0), pt(0.0, 1.0, -2.0), pt(1.0, 1.0, 2.0)];

    // 1. `recompute_tri_bound_from_world`: Plane -> exact 0.0; apex-straddling Cone
    // (`DtError::NegativeConeAxialRange`) and invalid Cylinder (`radius <= 0`)
    // -> `NaN` (never `0.0`).
    let plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 2.0,
    };
    assert_eq!(
        crate::brep::recompute_tri_bound_from_world(plane, tri_pts),
        0.0
    );

    let cone_behind_apex = Surface::Cone {
        apex: pt(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_6,
    };
    let bad_cone_bound = crate::brep::recompute_tri_bound_from_world(cone_behind_apex, tri_pts);
    assert!(
        bad_cone_bound.is_nan(),
        "failed d_of_t on behind-apex cone must store NaN, got {bad_cone_bound}"
    );

    let invalid_cyl = Surface::Cylinder {
        axis_point: pt(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: -1.0,
    };
    let bad_cyl_bound = crate::brep::recompute_tri_bound_from_world(invalid_cyl, tri_pts);
    assert!(
        bad_cyl_bound.is_nan(),
        "failed d_of_t on invalid cylinder must store NaN, got {bad_cyl_bound}"
    );

    // 2. `TriangleErrorBoundMap::from_stage2_kept`: missing attribution (`None`)
    // and out-of-range face index -> `NaN`; valid planar face -> `0.0`.
    let bx = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let kept = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0),
            pt(1.0, 0.0, 0.0),
            pt(0.0, 1.0, 0.0),
            pt(1.0, 1.0, 0.0),
        ],
        vec![[0, 1, 2], [1, 3, 2], [0, 2, 3]],
    );
    let attribution = TriangleAttributionMap {
        attributions: vec![None, attr(InputId::A, 999), attr(InputId::A, 0)],
    };
    let empty_la = LabeledArrangement {
        mesh: Mesh::new(Vec::new(), Vec::new()),
        source: Vec::new(),
        inside: Vec::new(),
        surface: Vec::new(),
        patch: Vec::new(),
        intersection_edges: BTreeSet::new(),
        lpi_through_vertex: Vec::new(),
        num_inputs: 2,
    };
    let map = TriangleErrorBoundMap::from_stage2_kept(
        &kept,
        &[0, 1, 2],
        &empty_la,
        &attribution,
        false,
        &bx,
        &bx,
    );
    assert!(
        map.lookup(0).is_nan(),
        "unattributed triangle must store NaN"
    );
    assert!(
        map.lookup(1).is_nan(),
        "missing BRep face triangle must store NaN"
    );
    assert_eq!(map.lookup(2), 0.0, "valid planar face triangle stores 0.0");

    // 3. Fan rebuild (`rebuild_merge_fan`) on a curved surface whose `d_of_t`
    // returns `Err(DtError::InvalidSurface)` (`radius = -1.0`): `new_tri_bounds`
    // stores `NaN` for every triangle, never `0.0`.
    let at_cyl = |th: f64, z: f64| pt(th.cos(), th.sin(), z);
    let fan_mesh = Mesh::new(
        vec![
            at_cyl(0.0, 0.0), // 0
            at_cyl(0.4, 0.0), // 1
            at_cyl(0.4, 1.0), // 2
            at_cyl(0.0, 1.0), // 3
            at_cyl(0.2, 0.5), // 4 (interior victim)
        ],
        vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
    );
    let fan_patch = SplicePatch {
        cycles: vec![vec![0, 1, 2, 3]],
        tris: vec![0, 1, 2, 3],
        surface: invalid_cyl,
    };
    let fan_rebuild =
        crate::stage4_construct::rebuild_merge_fan(&fan_mesh, 0, &fan_patch, 4, 0, false)
            .expect("chart CDT succeeds on square fan");
    assert_eq!(fan_rebuild.new_tris.len(), 2);
    assert_eq!(fan_rebuild.new_tri_bounds.len(), 2);
    for (i, &b) in fan_rebuild.new_tri_bounds.iter().enumerate() {
        assert!(
            b.is_nan(),
            "fan rebuild new_tri_bounds[{i}] must be NaN on failed d_of_t, got {b}"
        );
    }
}

/// Review D2.4 pin: Direct calls to Stage-4/5 mutators on CURVED cylinder
/// fixtures, asserting the exact recomputed `d_of_t` value (`bound > 0.0` and
/// bit-identical to `d_of_t` / `recompute_tri_bound_from_world`) on EVERY newly
/// created curved triangle across:
///   (1) `TriangleErrorBoundMap::from_stage2_kept` (surviving Stage-1 inner = `d_ε`,
///       surviving Stage-1 boundary = `tri_dt`, Stage-2 split = `recompute_tri_bound_from_world`),
///   (2) `retriangulate_collapsed_fan_regions_ex_with_bounds` (curved cylinder fan),
///   (3) `update_patches_ex_with_bounds` (new curved cylinder triangles),
///   (4) `decimate_patch_chains_441_with_bounds` (curved cylinder × plane chain decimation),
///   (5) `rebuild_merge_fan` + `apply_rebuild_batch` (curved cylinder fan rebuild),
///   (6) `split_boundary_edge` + `apply_rebuild_batch` (curved cylinder edge split).
#[test]
fn n106_d2_pin_b_curved_mutators_recompute_exact_d_of_t() {
    use cherchi_rs::labeled_arrangement::InputId as LaInputId;

    let build_kept_subtract = |a: &BRep, b: &BRep, a_minus_b: bool| {
        let la = cherchi_rs::native_labeled_arrangement(a.as_mesh(), b.as_mesh())
            .expect("native labeled arrangement");
        let mut vert_map: BTreeMap<[u64; 3], u32> = BTreeMap::new();
        let mut compact_verts: Vec<Point3> = Vec::new();
        let mut compact_tris: Vec<[u32; 3]> = Vec::new();
        let mut orig_tri: Vec<usize> = Vec::new();
        let mut attributions: Vec<Option<TriangleAttribution>> = Vec::new();
        let mut side_labels_vec: Vec<TriangleSideLabel> = Vec::new();

        let intern =
            |p: Point3, vmap: &mut BTreeMap<[u64; 3], u32>, cverts: &mut Vec<Point3>| -> u32 {
                let key = [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
                *vmap.entry(key).or_insert_with(|| {
                    let id = cverts.len() as u32;
                    cverts.push(p);
                    id
                })
            };

        for (ot, raw_tri) in la.mesh.tris.iter().enumerate() {
            if la.surface[ot].len() != 1 {
                continue;
            }
            let LaInputId(k) = la.surface[ot][0];
            let (keep, flip) = if a_minus_b {
                // `a \ b`: keep A outside B (unflipped) + B inside A (flipped).
                if k == 0 {
                    (!la.inside[ot][1], false)
                } else {
                    (la.inside[ot][0], true)
                }
            } else {
                // `b \ a`: keep B outside A (unflipped) + A inside B (flipped).
                if k == 1 {
                    (!la.inside[ot][0], false)
                } else {
                    (la.inside[ot][1], true)
                }
            };
            if !keep {
                continue;
            }
            let (input_id, operand) = if k == 0 {
                (InputId::A, a)
            } else {
                (InputId::B, b)
            };
            let (_, local_raw) = la.source[ot]
                .iter()
                .find(|&&(LaInputId(sk), _)| sk == k)
                .copied()
                .expect("arrangement provenance");
            let face = operand.tri_face()[local_raw as usize];
            let mut tri = raw_tri.map(|vi| {
                intern(
                    la.mesh.verts[vi as usize],
                    &mut vert_map,
                    &mut compact_verts,
                )
            });
            if flip {
                tri.swap(1, 2);
            }
            if tri[0] == tri[1] || tri[1] == tri[2] || tri[2] == tri[0] {
                continue;
            }
            compact_tris.push(tri);
            orig_tri.push(ot);
            attributions.push(Some(TriangleAttribution {
                input: input_id,
                face,
            }));
            side_labels_vec.push(TriangleSideLabel::from_arrangement(&la, ot));
        }

        (
            la,
            Mesh::new(compact_verts, compact_tris),
            orig_tri,
            TriangleAttributionMap { attributions },
            TriangleSideLabelMap {
                labels: side_labels_vec,
            },
        )
    };

    // -------------------------------------------------------------------------
    // (1) `TriangleErrorBoundMap::from_stage2_kept` on curved torus × box AND
    // curved cylinder × box: exercises all three curved Stage-2 populations:
    //   - surviving unsplit Stage-1 inner (`is_stage1_inner_tri == true`) -> `face_error_budget` `d_ε`,
    //   - surviving unsplit Stage-1 boundary (`is_stage1_inner_tri == false`) -> `tri_dt[raw_t]`,
    //   - Stage-2 split curved triangles -> `recompute_tri_bound_from_world` (`d_of_t`).
    // -------------------------------------------------------------------------
    {
        let sorted_bits = |pts: [Point3; 3]| -> [[u64; 3]; 3] {
            let mut keys = pts.map(|p| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]);
            keys.sort_unstable();
            keys
        };

        let major = 3.0_f64;
        let minor = 1.0_f64;
        let torus = BRep::new(
            vec![BRepVertex {
                point: pt(major + minor, 0.0, 0.0),
            }],
            vec![
                BRepEdge {
                    start: 0,
                    end: 0,
                    curve: Curve::Circle {
                        center: pt(major, 0.0, 0.0),
                        normal: Vector3::new(0.0, 1.0, 0.0),
                        radius: minor,
                    },
                },
                BRepEdge {
                    start: 0,
                    end: 0,
                    curve: Curve::Circle {
                        center: pt(0.0, 0.0, 0.0),
                        normal: Vector3::new(0.0, 0.0, 1.0),
                        radius: major + minor,
                    },
                },
            ],
            vec![BRepFace {
                surface: Surface::Torus {
                    center: pt(0.0, 0.0, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    major_radius: major,
                    minor_radius: minor,
                },
                outer_loop: vec![0, 1, 0, 1],
                inner_loops: Vec::new(),
                reversed: false,
            }],
        )
        .expect("closed torus BRep");
        let bx_tor = rj_box([1.5, 0.3, -2.0], [5.0, 4.0, 2.0]);
        let cyl = rj_cylinder([0.0, 0.0, -2.0], [0.0, 0.0, 1.0], 1.5, 4.0);
        let bx_cyl = rj_box([-2.0, 0.3, -1.0], [2.0, 3.0, 1.0]);

        let mut total_inner = 0usize;
        let mut total_bnd = 0usize;
        let mut total_split = 0usize;

        for (curved_op, box_op) in [(&torus, &bx_tor), (&cyl, &bx_cyl)] {
            let (la, kept, orig_tri, attribution, _side_labels) =
                build_kept_subtract(curved_op, box_op, true);
            let map = TriangleErrorBoundMap::from_stage2_kept(
                &kept,
                &orig_tri,
                &la,
                &attribution,
                false,
                curved_op,
                box_op,
            );
            assert_eq!(map.len(), kept.tris.len());
            let curved_surf = curved_op.faces()[0].surface;
            let expected_inner_budget =
                crate::brep::face_error_budget(curved_op, 0).expect("curved face_error_budget");
            assert!(expected_inner_budget > 0.0);

            let mut n_bnd = 0usize;
            let mut n_split = 0usize;
            for (ct, &ot) in orig_tri.iter().enumerate() {
                if attribution.lookup(ct as u32) != attr(InputId::A, 0) {
                    continue;
                }
                let tri = kept.tris[ct];
                let pts = tri.map(|vi| kept.verts[vi as usize]);
                let bound = map.lookup(ct as u32);
                assert!(
                    bound.is_finite() && bound > 0.0,
                    "curved triangle {ct} must have finite positive bound, got {bound}"
                );
                let raw_t = la.source[ot]
                    .iter()
                    .find(|&&(LaInputId(sk), _)| sk == 0)
                    .map(|&(_, local)| local as usize)
                    .expect("A source");
                let rpts = curved_op.as_mesh().tris[raw_t]
                    .map(|vi| curved_op.as_mesh().verts[vi as usize]);
                if sorted_bits(pts) == sorted_bits(rpts) {
                    if curved_op.is_stage1_inner_tri(raw_t) {
                        total_inner += 1;
                        assert_eq!(
                            bound.to_bits(),
                            expected_inner_budget.to_bits(),
                            "surviving Stage-1 inner curved tri must carry face_error_budget d_ε"
                        );
                    } else {
                        n_bnd += 1;
                        total_bnd += 1;
                        assert_eq!(
                            bound.to_bits(),
                            curved_op.tri_dt()[raw_t].to_bits(),
                            "surviving Stage-1 boundary curved tri must carry Stage-1 tri_dt"
                        );
                    }
                } else {
                    n_split += 1;
                    total_split += 1;
                    let exact_dt = crate::brep::recompute_tri_bound_from_world(curved_surf, pts);
                    assert_eq!(
                        bound.to_bits(),
                        exact_dt.to_bits(),
                        "Stage-2 split curved tri must carry recomputed d_of_t"
                    );
                }
            }
            assert!(
                n_bnd > 0 && n_split > 0,
                "expected surviving boundary and Stage-2 split curved tris > 0, got bnd={n_bnd}, split={n_split}"
            );
        }
        assert!(
            total_inner > 0 && total_bnd > 0 && total_split > 0,
            "expected all 3 curved Stage-2 populations > 0, got inner={total_inner}, bnd={total_bnd}, split={total_split}"
        );
    }

    let cyl5 = rj_cylinder([0.0, 0.0, -3.0], [0.0, 0.0, 1.0], 5.0, 6.0);
    let cyl5_surf = cyl5.faces()[0].surface;
    let cyl5_chart = SurfaceChart::new(cyl5_surf).expect("cyl5 chart");
    let plane_y0 = BRep::new(
        vec![
            BRepVertex {
                point: pt(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: pt(6.0, 0.0, 0.0),
            },
            BRepVertex {
                point: pt(0.0, 0.0, 6.0),
            },
        ],
        vec![
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
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ],
        vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 1.0, 0.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }],
    )
    .expect("plane_y0 brep");
    let lbl_a = side_lbl(7, [false, false], [true, false]);
    let lbl_b = side_lbl(8, [false, false], [false, true]);

    // -------------------------------------------------------------------------
    // (2) `retriangulate_collapsed_fan_regions_ex_with_bounds` on curved `cyl5`:
    // lift the 17-vertex stacked-collapse fan onto `cyl5_surf` (`r = 5.0`).
    // -------------------------------------------------------------------------
    {
        let uv_coords = [
            (0.0, 0.0),   // 0
            (1.0, 0.0),   // 1 P (mint)
            (1.4, 0.0),   // 2 V1
            (1.6, 0.0),   // 3 V2
            (2.0, 0.0),   // 4 G (mint)
            (3.0, 0.0),   // 5
            (1.0, 0.5),   // 6
            (1.4, 0.5),   // 7
            (1.6, 0.5),   // 8
            (2.0, 0.5),   // 9
            (1.45, 0.3),  // 10
            (1.55, 0.35), // 11
            (1.5, 1.0),   // 12
            (2.5, 1.0),   // 13
            (0.5, -1.0),  // 14
            (1.5, -1.0),  // 15
            (2.5, -1.0),  // 16
        ];
        let verts: Vec<Point3> = uv_coords
            .iter()
            .enumerate()
            .map(|(idx, &(x, y))| {
                let th: f64 = x * 0.15;
                if idx >= 14 {
                    // B-side cap vertices lie on `z = 0` at radius `3.5`.
                    pt(3.5 * th.cos(), 3.5 * th.sin(), 0.0)
                } else {
                    cyl5_chart.lift(Point2::new(th, y))
                }
            })
            .collect();
        let plane_z0 = BRep::new(
            vec![
                BRepVertex {
                    point: pt(0.0, 0.0, 0.0),
                },
                BRepVertex {
                    point: pt(0.0, 6.0, 0.0),
                },
                BRepVertex {
                    point: pt(6.0, 0.0, 0.0),
                },
            ],
            vec![
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
                BRepEdge {
                    start: 2,
                    end: 0,
                    curve: Curve::LineSegment,
                },
            ],
            vec![BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            }],
        )
        .expect("plane_z0 brep");
        let a_tris = [
            [0, 1, 6],
            [1, 2, 7],
            [1, 7, 6],
            [2, 3, 10],
            [3, 8, 10],
            [8, 11, 10],
            [8, 7, 11],
            [7, 10, 11],
            [2, 10, 7],
            [3, 4, 9],
            [3, 9, 8],
            [6, 7, 12],
            [7, 8, 12],
            [8, 9, 12],
            [9, 13, 12],
            [4, 5, 9],
            [5, 13, 9],
        ];
        let b_tris = [
            [1, 0, 14],
            [1, 14, 15],
            [2, 1, 15],
            [3, 2, 15],
            [3, 15, 16],
            [4, 3, 16],
            [5, 4, 16],
        ];
        let mut fan_tris = Vec::new();
        let mut fan_attr = Vec::new();
        let mut fan_lbls = Vec::new();
        let mut fan_bounds = Vec::new();
        for (i, &t) in a_tris.iter().enumerate() {
            fan_tris.push(t);
            fan_attr.push(attr(InputId::A, 0));
            fan_lbls.push(lbl_a);
            fan_bounds.push(200.0 + i as f64);
        }
        for (i, &t) in b_tris.iter().enumerate() {
            fan_tris.push(t);
            fan_attr.push(attr(InputId::B, 0));
            fan_lbls.push(lbl_b);
            fan_bounds.push(300.0 + i as f64);
        }
        let mut fan_mesh = Mesh::new(verts, fan_tris);
        for &(victim, survivor) in &[(2, 1), (3, 4), (7, 1), (8, 4)] {
            collapse_vertex_scoped_with_bounds(
                &mut fan_mesh,
                &mut fan_attr,
                &mut fan_lbls,
                &mut fan_bounds,
                victim,
                survivor,
                false,
                None,
            );
        }
        let minted: HashSet<u32> = [1u32, 4u32].into_iter().collect();
        assert!(retriangulate_collapsed_fan_regions_ex_with_bounds(
            &mut fan_mesh,
            &mut fan_attr,
            &mut fan_lbls,
            &mut fan_bounds,
            &cyl5,
            &plane_z0,
            &HashSet::new(),
            &minted,
            false,
        ));
        assert_eq!(fan_bounds.len(), fan_mesh.tris.len());
        let untouched_slot = fan_mesh
            .tris
            .iter()
            .position(|&t| t == [9, 13, 12])
            .expect("disjoint triangle [9, 13, 12] must survive");
        assert_eq!(fan_bounds[untouched_slot], 214.0);

        let mut n_new_curved = 0usize;
        for (ti, (&tri, &bound)) in fan_mesh.tris.iter().zip(fan_bounds.iter()).enumerate() {
            if fan_attr[ti] == attr(InputId::A, 0) && bound < 100.0 {
                n_new_curved += 1;
                let pts = tri.map(|vi| fan_mesh.verts[vi as usize]);
                let exact_dt = crate::brep::recompute_tri_bound_from_world(cyl5_surf, pts);
                assert!(
                    bound > 0.0,
                    "new curved retriangulate tri {ti} must have positive d(T), got {bound}"
                );
                assert_eq!(
                    bound.to_bits(),
                    exact_dt.to_bits(),
                    "new curved retriangulate tri {ti} bound must match recomputed d_of_t"
                );
            }
        }
        assert!(
            n_new_curved >= 8,
            "expected >= 8 newly created curved cylinder triangles from retriangulate, got {n_new_curved}"
        );
    }

    // -------------------------------------------------------------------------
    // (3) `update_patches_ex_with_bounds` on curved `cyl5`:
    // vertex `1` (`moved = {1}`) lies on generator segment `(0, 2)` (`θ = 0.2`,
    // `z = 0, 1, 2`) so `[0, 2, 1]` triggers patch CDT rebuild on `(A, 0)`.
    // -------------------------------------------------------------------------
    {
        let mut mesh = Mesh::new(
            vec![
                cyl5_chart.lift(Point2::new(0.0, 0.0)),   // 0
                cyl5_chart.lift(Point2::new(0.0, 1.0)),   // 1 (moved onto segment 0..2)
                cyl5_chart.lift(Point2::new(0.0, 2.0)),   // 2
                cyl5_chart.lift(Point2::new(0.03, 1.0)),  // 3
                cyl5_chart.lift(Point2::new(-0.03, 0.0)), // 4
                cyl5_chart.lift(Point2::new(-0.03, 2.0)), // 5
                pt(1.0, 0.0, 0.0),                        // 6 (untouched B(0))
                pt(2.0, 0.0, 0.0),                        // 7
                pt(1.0, 0.0, 1.0),                        // 8
            ],
            vec![
                [4, 0, 1],
                [4, 1, 5],
                [5, 1, 2],
                [0, 2, 1],
                [0, 3, 2],
                [6, 7, 8],
            ],
        );
        let mut attr_map = TriangleAttributionMap {
            attributions: vec![
                attr(InputId::A, 0),
                attr(InputId::A, 0),
                attr(InputId::A, 0),
                attr(InputId::A, 0),
                attr(InputId::A, 0),
                attr(InputId::B, 0),
            ],
        };
        let mut lbl_map = TriangleSideLabelMap {
            labels: vec![lbl_a, lbl_a, lbl_a, lbl_a, lbl_a, lbl_b],
        };
        let mut tri_bounds =
            TriangleErrorBoundMap::from_bounds(vec![50.1, 50.2, 50.3, 50.4, 50.5, 0.77]);
        let applied = update_patches_ex_with_bounds(
            &mut mesh,
            &mut attr_map,
            &mut lbl_map,
            &mut tri_bounds,
            &cyl5,
            &plane_y0,
            false,
            &HashSet::from([1u32]),
            None,
        );
        assert!(
            applied >= 1,
            "curved cylinder patch must be rebuilt, got applied={applied}"
        );
        assert_eq!(tri_bounds.len(), mesh.tris.len());
        let b_slot = mesh
            .tris
            .iter()
            .position(|&t| t == [6, 7, 8])
            .expect("untouched B(0) triangle must survive");
        assert_eq!(tri_bounds.lookup(b_slot as u32), 0.77);

        let mut n_rebuilt_curved = 0usize;
        for (ti, &at) in attr_map.attributions.iter().enumerate() {
            if at == attr(InputId::A, 0) {
                n_rebuilt_curved += 1;
                let bound = tri_bounds.lookup(ti as u32);
                let uv_tri = mesh.tris[ti].map(|vi| cyl5_chart.project(mesh.verts[vi as usize]));
                let exact_dt = crate::stage4_dt::d_of_t(&cyl5_surf, uv_tri)
                    .expect("d_of_t on rebuilt cylinder tri");
                assert!(
                    bound > 0.0 && bound < 1.0,
                    "rebuilt curved tri {ti} must replace 50.x sentinel with positive d(T), got {bound}"
                );
                assert_eq!(
                    bound.to_bits(),
                    exact_dt.to_bits(),
                    "rebuilt curved tri {ti} bound must equal d_of_t"
                );
            }
        }
        assert!(
            n_rebuilt_curved >= 4,
            "expected >= 4 newly created curved cylinder triangles from update_patches, got {n_rebuilt_curved}"
        );
    }

    // -------------------------------------------------------------------------
    // (4) `decimate_patch_chains_441_with_bounds` on curved `cyl5` × `plane_y0`:
    // 20-vertex dense `5e-5` generator chain at `(5.0, 0.0, z)` decimates and
    // refills the curved cylinder patch via `refill_fan_hole_seeded_with_bounds`.
    // -------------------------------------------------------------------------
    {
        let n = 20usize;
        let step = 5e-5_f64;
        let z_end = (n - 1) as f64 * step;
        let mut verts: Vec<Point3> = (0..n).map(|i| pt(5.0, 0.0, i as f64 * step)).collect();
        let c_a0 = n as u32;
        let c_a1 = (n + 1) as u32;
        let c_b0 = (n + 2) as u32;
        let c_b1 = (n + 3) as u32;
        verts.push(cyl5_chart.lift(Point2::new(-0.3, z_end))); // c_a0
        verts.push(cyl5_chart.lift(Point2::new(-0.3, 0.0))); // c_a1
        verts.push(pt(3.0, 0.0, z_end)); // c_b0
        verts.push(pt(3.0, 0.0, 0.0)); // c_b1

        let mut tris = Vec::new();
        let mut attrs = Vec::new();
        let mut labels = Vec::new();
        let mut init_bounds = Vec::new();
        for i in 0..(n - 1) as u32 {
            tris.push([i, i + 1, c_a1]);
            attrs.push(attr(InputId::A, 0));
            labels.push(lbl_a);
            init_bounds.push(800.0 + i as f64);
        }
        tris.push([(n - 1) as u32, c_a0, c_a1]);
        attrs.push(attr(InputId::A, 0));
        labels.push(lbl_a);
        init_bounds.push(899.0);

        for i in 0..(n - 1) as u32 {
            tris.push([i + 1, i, c_b1]);
            attrs.push(attr(InputId::B, 0));
            labels.push(lbl_b);
            init_bounds.push(900.0 + i as f64);
        }
        tris.push([c_b0, (n - 1) as u32, c_b1]);
        attrs.push(attr(InputId::B, 0));
        labels.push(lbl_b);
        init_bounds.push(999.0);

        let mut mesh = Mesh::new(verts, tris);
        let mut attribution = TriangleAttributionMap {
            attributions: attrs,
        };
        let mut side_labels = TriangleSideLabelMap { labels };
        let mut tri_bounds = TriangleErrorBoundMap::from_bounds(init_bounds);
        let (infos, _inc, curves) = crate::stage4_correct::compute_phase_a(
            &mesh,
            &attribution,
            &cyl5,
            &plane_y0,
            &crate::stage3_ssi::NO_EDGE_PROVENANCE,
        )
        .expect("compute_phase_a on curved cylinder × plane");
        let st = crate::stage4_chain_decimate::decimate_patch_chains_441_with_bounds(
            &mut mesh,
            &mut attribution,
            &mut side_labels,
            &mut tri_bounds,
            &infos,
            &curves,
            &cyl5,
            &plane_y0,
            false,
        )
        .expect("decimate_patch_chains_441_with_bounds");
        assert!(
            st.verts_dropped > 0,
            "curved cylinder generator chain must decimate interior vertices: {st:?}"
        );
        assert_eq!(tri_bounds.len(), mesh.tris.len());

        let mut n_new_cyl = 0usize;
        for (ti, &tri) in mesh.tris.iter().enumerate() {
            let bound = tri_bounds.lookup(ti as u32);
            if attribution.lookup(ti as u32) == attr(InputId::A, 0) && bound < 800.0 {
                n_new_cyl += 1;
                let uv_tri = tri.map(|vi| cyl5_chart.project(mesh.verts[vi as usize]));
                let exact_dt = crate::stage4_dt::d_of_t(&cyl5_surf, uv_tri)
                    .expect("d_of_t on decimated cylinder tri");
                assert!(
                    bound > 0.0,
                    "decimated curved cylinder tri {ti} must have positive d(T)"
                );
                assert_eq!(
                    bound.to_bits(),
                    exact_dt.to_bits(),
                    "decimated curved cylinder tri {ti} bound must equal d_of_t"
                );
            }
        }
        assert!(
            n_new_cyl > 0,
            "decimate refill must emit newly bounded curved cylinder triangles"
        );
    }

    // -------------------------------------------------------------------------
    // (5) Fan rebuild (`rebuild_merge_fan` + `apply_rebuild_batch`) on curved `cyl5`.
    // -------------------------------------------------------------------------
    {
        let mut fan_mesh = Mesh::new(
            vec![
                cyl5_chart.lift(Point2::new(0.0, 0.0)), // 0
                cyl5_chart.lift(Point2::new(0.4, 0.0)), // 1
                cyl5_chart.lift(Point2::new(0.4, 2.0)), // 2
                cyl5_chart.lift(Point2::new(0.0, 2.0)), // 3
                cyl5_chart.lift(Point2::new(0.2, 1.0)), // 4 (interior victim)
                cyl5_chart.lift(Point2::new(0.8, 1.0)), // 5 (untouched bystander)
            ],
            vec![
                [0, 1, 4],
                [1, 2, 4],
                [2, 3, 4],
                [3, 0, 4],
                [1, 5, 2], // untouched bystander slot 4
            ],
        );
        let fan_patch = SplicePatch {
            cycles: vec![vec![0, 1, 2, 3]],
            tris: vec![0, 1, 2, 3],
            surface: cyl5_surf,
        };
        let fan_rebuild =
            crate::stage4_construct::rebuild_merge_fan(&fan_mesh, 0, &fan_patch, 4, 0, false)
                .expect("curved rebuild_merge_fan must succeed");
        assert_eq!(fan_rebuild.new_tris.len(), 2);
        assert_eq!(fan_rebuild.new_tri_bounds.len(), 2);

        let mut attr_map = TriangleAttributionMap {
            attributions: vec![attr(InputId::A, 0); 5],
        };
        let mut lbl_map = TriangleSideLabelMap {
            labels: vec![lbl_a; 5],
        };
        let mut tri_bounds = TriangleErrorBoundMap::from_bounds(vec![60.1, 60.2, 60.3, 60.4, 60.9]);
        apply_rebuild_batch(
            &mut fan_mesh,
            &mut attr_map,
            &mut lbl_map,
            &mut tri_bounds,
            &[fan_rebuild],
        )
        .expect("apply_rebuild_batch for curved rebuild_merge_fan");
        assert_eq!(fan_mesh.tris.len(), 3);
        assert_eq!(tri_bounds.lookup(0), 60.9, "untouched bystander keeps 60.9");
        for slot in 1..3 {
            let bound = tri_bounds.lookup(slot as u32);
            let uv_tri =
                fan_mesh.tris[slot].map(|vi| cyl5_chart.project(fan_mesh.verts[vi as usize]));
            let exact_dt =
                crate::stage4_dt::d_of_t(&cyl5_surf, uv_tri).expect("d_of_t on curved fan tri");
            assert!(bound > 0.0);
            assert_eq!(bound.to_bits(), exact_dt.to_bits());
        }
    }

    // -------------------------------------------------------------------------
    // (6) `split_boundary_edge` + `apply_rebuild_batch` on curved `cyl5`.
    // -------------------------------------------------------------------------
    {
        let mint = cyl5_chart.lift(Point2::new(0.2, 0.0));
        let mut split_mesh = Mesh::new(
            vec![
                cyl5_chart.lift(Point2::new(0.0, 0.0)),  // 0
                cyl5_chart.lift(Point2::new(0.4, 0.0)),  // 1
                cyl5_chart.lift(Point2::new(0.2, 2.0)),  // 2
                mint,                                    // 3 (inserted vertex)
                cyl5_chart.lift(Point2::new(-0.2, 1.0)), // 4 (untouched bystander)
            ],
            vec![
                [0, 1, 2], // 0: split target on boundary edge (0, 1)
                [0, 2, 4], // 1: untouched bystander
            ],
        );
        let split_patch = SplicePatch {
            cycles: vec![vec![0, 1, 2]],
            tris: vec![0],
            surface: cyl5_surf,
        };
        let split_rebuild = crate::stage4_construct::split_boundary_edge(
            &split_mesh,
            0,
            &split_patch,
            0,
            1,
            3,
            mint,
        )
        .expect("curved split_boundary_edge must succeed");
        assert_eq!(split_rebuild.new_tris.len(), 2);
        assert_eq!(split_rebuild.new_tri_bounds.len(), 2);

        let mut attr_map = TriangleAttributionMap {
            attributions: vec![attr(InputId::A, 0); 2],
        };
        let mut lbl_map = TriangleSideLabelMap {
            labels: vec![lbl_a; 2],
        };
        let mut tri_bounds = TriangleErrorBoundMap::from_bounds(vec![70.1, 70.9]);
        apply_rebuild_batch(
            &mut split_mesh,
            &mut attr_map,
            &mut lbl_map,
            &mut tri_bounds,
            &[split_rebuild],
        )
        .expect("apply_rebuild_batch for curved split_boundary_edge");
        assert_eq!(split_mesh.tris.len(), 3);
        assert_eq!(tri_bounds.lookup(0), 70.9, "untouched bystander keeps 70.9");
        for slot in 1..3 {
            let bound = tri_bounds.lookup(slot as u32);
            let pts = split_mesh.tris[slot].map(|vi| split_mesh.verts[vi as usize]);
            let exact_dt = crate::brep::recompute_tri_bound_from_world(cyl5_surf, pts);
            assert!(bound > 0.0);
            assert_eq!(bound.to_bits(), exact_dt.to_bits());
        }
    }
}
