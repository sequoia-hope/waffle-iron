//! Roadmap F1, Increment I1 (`specs/yang_442_classify_after_update.md` §4–§5):
//! per-triangle side labels `(cherchi_patch, inside, surface)` carried in 1:1
//! lockstep with `mesh.tris` and `TriangleAttributionMap` across every Stage-4/5
//! triangle mutator, separate from `TriangleAttribution` (the flood-fill key).
//!
//! Paper: Yang et al. 2025 §4.4.2 (`refs/text/yang2025_hybrid_boolean.txt:572-578`).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::InputId as CherchiInputId;
use cherchi_rs::{LabeledArrangement, Mesh};

use crate::brep::{
    BRep, BRepEdge, BRepFace, BRepVertex, InputId, TriangleAttribution, TriangleAttributionMap,
    TriangleSideLabel, TriangleSideLabelMap,
};
use crate::geom::{Curve, Surface};
use crate::stage4_construct::{apply_rebuild_batch, PatchRebuild};
use crate::stage4_correct::{
    collapse_subresolution_intersection_segments, collapse_subtauwork_mesh_edges, collapse_vertex,
    collapse_vertex_scoped, compute_phase_a, compute_phase_a_with_side_labels,
    merge_same_plane_patches_with_side_labels, resolve_mutual_degenerate_pair,
    resolve_simple_degenerate_split, resolve_synchronized_degenerate_split,
    retriangulate_collapsed_fan_regions, MutualPair,
};
use crate::stage4_relocate::{
    check_watertight_2manifold, check_watertight_2manifold_per_operand, remove_doubled_membranes,
    remove_doubled_membranes_scoped, split_pinch_vertices,
};
use crate::tests_unit::n2_junction::{rj_box, rj_cylinder};
use crate::{flood_fill_patches, flood_fill_patches_with_side_labels, triangle_adjacency};

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

/// Pin 1: `TriangleSideLabel::from_arrangement` + `TriangleSideLabelMap::keep_set`
/// matches `cherchi_rs::LabeledArrangement::keep_set` across ALL 12 valid binary
/// `(surface, inside)` combinations (`{[A], [B], [A,B]}` × `{F,T}²`) and all four
/// `BoolOp`s (`Union`, `Intersect`, `Subtract`, `Xor`), while preserving each
/// triangle's `cherchi_patch` id verbatim.
#[test]
fn side_label_keep_for_op_matches_labeled_arrangement_keep_set() {
    let surface_states: [Vec<CherchiInputId>; 3] = [
        vec![CherchiInputId(0)],
        vec![CherchiInputId(1)],
        vec![CherchiInputId(0), CherchiInputId(1)],
    ];
    let inside_states: [[bool; 2]; 4] =
        [[false, false], [false, true], [true, false], [true, true]];

    let mut verts = Vec::new();
    let mut tris = Vec::new();
    let mut surface = Vec::new();
    let mut inside = Vec::new();
    let mut patch = Vec::new();

    for (si, s) in surface_states.iter().enumerate() {
        for (ii, ins) in inside_states.iter().enumerate() {
            let t = si * 4 + ii;
            let base = (t * 3) as u32;
            verts.push(pt(t as f64, 0.0, 0.0));
            verts.push(pt(t as f64 + 0.5, 0.0, 0.0));
            verts.push(pt(t as f64, 0.5, 0.0));
            tris.push([base, base + 1, base + 2]);
            surface.push(s.clone());
            inside.push(vec![ins[0], ins[1]]);
            patch.push((t as u32) * 10 + 7);
        }
    }

    let la = LabeledArrangement {
        mesh: Mesh::new(verts, tris),
        surface,
        inside,
        patch: patch.clone(),
        source: Vec::new(),
        intersection_edges: BTreeSet::new(),
        lpi_through_vertex: Vec::new(),
        num_inputs: 2,
    };

    let map = TriangleSideLabelMap {
        labels: (0..la.mesh.tris.len())
            .map(|t| TriangleSideLabel::from_arrangement(&la, t))
            .collect(),
    };
    assert_eq!(map.len(), 12);
    for (t, &expected_patch) in patch.iter().enumerate() {
        assert_eq!(map.lookup(t as u32).cherchi_patch, expected_patch);
    }

    for op in [
        BoolOp::Union,
        BoolOp::Intersect,
        BoolOp::Subtract,
        BoolOp::Xor,
    ] {
        assert_eq!(
            map.keep_set(op),
            la.keep_set(op),
            "carried side labels must re-derive LabeledArrangement::keep_set({op:?}) identically"
        );
        for t in 0..la.mesh.tris.len() {
            assert_eq!(
                map.lookup(t as u32).flip_winding_for_op(op),
                crate::boolean::flip_for_op(op, &la, t),
                "flip_winding_for_op({op:?}) must match flip_for_op at tri {t}"
            );
        }
    }
}

/// Pin 2: `TriangleSideLabel` is NOT part of `TriangleAttribution`'s equality
/// (the flood-fill patch key). Two edge-sharing triangles on the same `(InputId, face)`
/// with distinct `cherchi_patch` / `inside` / `surface` side labels still flood
/// into ONE `Patch`.
#[test]
fn side_label_does_not_alter_triangle_attribution_flood_key() {
    let mesh = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0),
            pt(1.0, 0.0, 0.0),
            pt(1.0, 1.0, 0.0),
            pt(0.0, 1.0, 0.0),
        ],
        vec![[0, 1, 2], [0, 2, 3]],
    );
    let attribution = TriangleAttributionMap {
        attributions: vec![attr(InputId::A, 3), attr(InputId::A, 3)],
    };
    let side_labels = TriangleSideLabelMap {
        labels: vec![
            side_lbl(11, [false, false], [true, false]),
            side_lbl(99, [false, true], [true, true]),
        ],
    };
    assert_ne!(side_labels.lookup(0), side_labels.lookup(1));
    assert_eq!(attribution.lookup(0), attribution.lookup(1));

    let adj = triangle_adjacency(&mesh);
    let patches = flood_fill_patches(&mesh, &attribution, &adj);
    assert_eq!(
        patches.len(),
        1,
        "distinct side labels must not split same-(input,face) flood patches"
    );
    assert_eq!(patches[0].tri_indices, vec![0, 1]);
}

/// Pin 3 (Split mutators): side labels survive
/// - `resolve_simple_degenerate_split` (§4.4.1(a) simple edge split),
/// - `resolve_mutual_degenerate_pair` (§4.4.1(a) mutual-pair edge split), and
/// - `split_pinch_vertices` (in-place vertex-index split).
#[test]
fn side_labels_survive_splits() {
    // (a) Simple §4.4.1(a) degenerate-triangle edge split:
    // Degenerate tri `d_idx = 1` ([0, 2, 1], with 1 on segment 0..2) sits BEFORE
    // neighbour `n_idx = 2` ([2, 0, 3]) and between two bystanders `0` and `3`.
    let lbl_bystander_0 = side_lbl(100, [false, false], [true, false]);
    let lbl_degen = side_lbl(999, [true, true], [false, true]);
    let lbl_parent = side_lbl(200, [false, true], [false, true]);
    let lbl_bystander_3 = side_lbl(300, [false, false], [true, true]);

    let mut mesh = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0),  // 0: a
            pt(0.5, 0.0, 0.0),  // 1: b (collinear midpoint on a-c)
            pt(1.0, 0.0, 0.0),  // 2: c
            pt(0.5, -1.0, 0.0), // 3: neighbour apex
            pt(0.0, 1.0, 0.0),  // 4: bystander apex
            pt(1.0, 1.0, 0.0),  // 5: bystander apex
        ],
        vec![
            [0, 1, 4], // 0: bystander before d_idx
            [0, 2, 1], // 1: degenerate tri (d_idx)
            [2, 0, 3], // 2: long-edge neighbour (n_idx)
            [1, 2, 5], // 3: bystander after n_idx
        ],
    );
    let mut attr_vec = vec![
        attr(InputId::A, 0),
        attr(InputId::A, 1),
        attr(InputId::B, 2),
        attr(InputId::A, 3),
    ];
    let mut lbls = vec![lbl_bystander_0, lbl_degen, lbl_parent, lbl_bystander_3];

    resolve_simple_degenerate_split(&mut mesh, &mut attr_vec, &mut lbls, 1, 2, (0, 2, 1));
    assert_eq!(mesh.tris.len(), 4);
    assert_eq!(attr_vec.len(), 4);
    assert_eq!(lbls.len(), 4);
    assert_eq!(
        lbls,
        vec![lbl_bystander_0, lbl_bystander_3, lbl_parent, lbl_parent],
        "bystanders keep their labels; both split halves inherit n_idx's label (never d_idx's)"
    );

    // (b) Mutual degenerate pair (§4.4.1(a) §5c.11):
    // Zero-area quad {t1=1, t2=2} over (a=0, c=3) with bl=1, bh=2, outer
    // neighbours nl=3 (across (bl, c) = (1, 3)) and nh=4 (across (a, bh) = (0, 2)),
    // plus bystander 0 at the front.
    let lbl_nl = side_lbl(410, [false, false], [true, false]);
    let lbl_nh = side_lbl(420, [true, false], [false, true]);
    let mut m_mesh = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0),  // 0: a
            pt(0.3, 0.0, 0.0),  // 1: bl
            pt(0.7, 0.0, 0.0),  // 2: bh
            pt(1.0, 0.0, 0.0),  // 3: c
            pt(0.6, 1.0, 0.0),  // 4: outer apex of nl
            pt(0.4, -1.0, 0.0), // 5: outer apex of nh
            pt(-1.0, 0.0, 1.0), // 6: bystander
        ],
        vec![
            [0, 1, 6], // 0: bystander
            [0, 3, 2], // 1: t1 (degen)
            [3, 0, 1], // 2: t2 (degen)
            [1, 3, 4], // 3: nl (shares (bl=1, c=3))
            [2, 0, 5], // 4: nh (shares (a=0, bh=2))
        ],
    );
    let mut m_attr = vec![
        attr(InputId::A, 0),
        attr(InputId::A, 9),
        attr(InputId::B, 9),
        attr(InputId::A, 1),
        attr(InputId::B, 2),
    ];
    let mut m_lbls = vec![lbl_bystander_0, lbl_degen, lbl_degen, lbl_nl, lbl_nh];
    let pair = MutualPair {
        t1: 1,
        t2: 2,
        nl: 3,
        nh: 4,
        a: 0,
        c: 3,
        bl: 1,
        bh: 2,
    };
    resolve_mutual_degenerate_pair(&mut m_mesh, &mut m_attr, &mut m_lbls, &pair);
    assert_eq!(m_mesh.tris.len(), 5);
    assert_eq!(m_attr.len(), 5);
    assert_eq!(
        m_lbls,
        vec![lbl_bystander_0, lbl_nl, lbl_nl, lbl_nh, lbl_nh],
        "nl's two split children inherit lbl_nl; nh's two split children inherit lbl_nh"
    );

    // (c) `split_pinch_vertices`: rewrites vertex indices in place while keeping
    // all triangle slots and their side labels 1:1.
    let mut pinch_mesh = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0),  // 0: pinch low end
            pt(1.0, 0.0, 0.0),  // 1: pinch high end
            pt(0.0, 1.0, 0.0),  // 2
            pt(0.0, -1.0, 0.0), // 3
            pt(0.0, 0.0, -1.0), // 4
            pt(0.0, 0.0, 1.0),  // 5
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
        attr(InputId::B, 2),
        attr(InputId::B, 2),
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
    let mut relocations = Vec::new();
    let splits = split_pinch_vertices(
        &mut pinch_mesh,
        &mut relocations,
        &pinch_attr,
        &pinch_lbls,
        true,
    );
    assert_eq!(splits, 1);
    assert_eq!(pinch_mesh.tris.len(), pinch_lbls.len());
}

/// Pin 4 (Collapse mutators): side labels survive
/// - `collapse_vertex` (degenerate drop + opposite-winding membrane-pleat cancellation),
/// - `collapse_subresolution_intersection_segments`,
/// - `collapse_subtauwork_mesh_edges`, and
/// - `remove_doubled_membranes`.
#[test]
fn side_labels_survive_collapses() {
    let lbl_drop = side_lbl(901, [true, true], [true, false]);
    let lbl_pleat_pos = side_lbl(902, [true, false], [false, true]);
    let lbl_pleat_neg = side_lbl(903, [false, true], [true, false]);
    let lbl_surv_0 = side_lbl(10, [false, false], [true, false]);
    let lbl_surv_1 = side_lbl(20, [true, false], [false, true]);

    // (a) `collapse_vertex`: victim=1 -> survivor=0.
    // Slot 0: [0, 1, 2] -> [0, 0, 2] (degenerate, dropped).
    // Slot 1: [0, 3, 4] -> survives (lbl_surv_0).
    // Slot 2: [0, 2, 5] and Slot 3: [1, 5, 2] -> [0, 5, 2] (opposite-winding pleat, both cancelled!).
    // Slot 4: [1, 4, 6] -> [0, 4, 6] -> survives (lbl_surv_1).
    let mut mesh = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0),  // 0 survivor
            pt(1e-8, 0.0, 0.0), // 1 victim
            pt(0.0, 1.0, 0.0),  // 2
            pt(1.0, 0.0, 0.0),  // 3
            pt(1.0, 1.0, 0.0),  // 4
            pt(0.0, 1.0, 1.0),  // 5
            pt(0.0, 0.0, 1.0),  // 6
        ],
        vec![
            [0, 1, 2], // 0: degenerate on 1->0
            [0, 3, 4], // 1: survivor 0
            [0, 2, 5], // 2: pleat +
            [1, 5, 2], // 3: pleat - after 1->0
            [1, 4, 6], // 4: survivor 1 (remapped)
        ],
    );
    let mut attr_vec = vec![
        attr(InputId::A, 9),
        attr(InputId::A, 0),
        attr(InputId::B, 8),
        attr(InputId::B, 8),
        attr(InputId::B, 1),
    ];
    let mut lbls = vec![
        lbl_drop,
        lbl_surv_0,
        lbl_pleat_pos,
        lbl_pleat_neg,
        lbl_surv_1,
    ];
    let removed = collapse_vertex(&mut mesh, &mut attr_vec, &mut lbls, 1, 0);
    assert_eq!(removed, 3, "1 degenerate + 2 cancelled pleat tris removed");
    assert_eq!(mesh.tris, vec![[0, 3, 4], [0, 4, 6]]);
    assert_eq!(attr_vec, vec![attr(InputId::A, 0), attr(InputId::B, 1)]);
    assert_eq!(lbls, vec![lbl_surv_0, lbl_surv_1]);

    // (b) `collapse_subresolution_intersection_segments`:
    // Dropped degenerate tri at slot 0 BEFORE two surviving triangles with distinct labels.
    let empty_brep = BRep::new(Vec::new(), Vec::new(), Vec::new()).expect("empty brep");
    let mut seg_mesh = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0),
            pt(5e-8, 0.0, 0.0),
            pt(1.0, 0.0, 0.0),
            pt(0.0, 1.0, 0.0),
            pt(-1.0, 0.0, 0.0),
        ],
        vec![
            [0, 1, 3], // 0: collapses to degenerate [0,0,3]
            [1, 2, 3], // 1: remaps to [0,2,3], carries lbl_surv_0
            [0, 3, 4], // 2: untouched [0,3,4], carries lbl_surv_1
        ],
    );
    let mut seg_attr = vec![None, attr(InputId::A, 0), attr(InputId::B, 1)];
    let mut seg_lbls = vec![lbl_drop, lbl_surv_0, lbl_surv_1];
    let mut curves = BTreeMap::new();
    curves.insert((0u32, 1u32), Curve::LineSegment);
    assert!(collapse_subresolution_intersection_segments(
        &mut seg_mesh,
        &mut seg_attr,
        &mut seg_lbls,
        &curves,
        &empty_brep,
        &empty_brep,
    ));
    assert_eq!(seg_mesh.tris, vec![[0, 2, 3], [0, 3, 4]]);
    assert_eq!(seg_lbls, vec![lbl_surv_0, lbl_surv_1]);

    // (c) `collapse_subtauwork_mesh_edges`:
    // Flap at slot 0 drops; survivors at slots 1 and 2 keep their distinct labels.
    let mut tau_mesh = Mesh::new(
        vec![
            pt(0.5, 0.5, 0.5),
            pt(0.5 + 5e-14, 0.5, 0.5),
            pt(1.0, 0.0, 0.5),
            pt(0.0, 1.0, 0.5),
            pt(-1.0, 0.0, 0.5),
        ],
        vec![[0, 1, 2], [0, 2, 3], [1, 3, 4]],
    );
    let mut tau_attr = vec![
        attr(InputId::A, 9),
        attr(InputId::A, 0),
        attr(InputId::B, 1),
    ];
    let mut tau_lbls = vec![lbl_drop, lbl_surv_0, lbl_surv_1];
    assert!(collapse_subtauwork_mesh_edges(
        &mut tau_mesh,
        &mut tau_attr,
        &mut tau_lbls,
    ));
    assert_eq!(tau_mesh.tris, vec![[0, 2, 3], [0, 3, 4]]);
    assert_eq!(tau_lbls, vec![lbl_surv_0, lbl_surv_1]);

    // (d) `remove_doubled_membranes`:
    // Cancelling opposite-winding fin at slots 0 & 1 drops; tetra faces keep distinct labels.
    let mut mem_mesh = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0),
            pt(1.0, 0.0, 0.0),
            pt(0.0, 1.0, 0.0),
            pt(0.0, 0.0, 1.0),
            pt(0.4, 0.4, 0.02),
        ],
        vec![
            [1, 2, 4],
            [1, 4, 2],
            [0, 2, 1],
            [0, 1, 3],
            [0, 3, 2],
            [1, 2, 3],
        ],
    );
    let tetra_lbls = vec![
        side_lbl(1, [false, false], [true, false]),
        side_lbl(2, [false, false], [true, false]),
        side_lbl(3, [true, false], [false, true]),
        side_lbl(4, [true, false], [false, true]),
    ];
    let mut mem_attr = vec![None; 6];
    let mut mem_lbls = vec![
        lbl_pleat_pos,
        lbl_pleat_neg,
        tetra_lbls[0],
        tetra_lbls[1],
        tetra_lbls[2],
        tetra_lbls[3],
    ];
    assert_eq!(
        remove_doubled_membranes(&mut mem_mesh, &mut mem_attr, &mut mem_lbls),
        2
    );
    assert_eq!(mem_lbls, tetra_lbls);
}

/// Pin 5 (Re-CDT batch mutators): side labels survive
/// - `apply_rebuild_batch` (multi-patch batch with seeded vertices + interleaved
///   unreplaced foreign triangles), and
/// - `retriangulate_collapsed_fan_regions` (post-collapse two-operand fan repair).
#[test]
fn side_labels_survive_re_cdt_batch_and_fan_retriangulation() {
    // (a) Multi-patch `apply_rebuild_batch` with interleaved foreign triangles and
    // an appended Steiner seed vertex on patch B:
    // Slots:
    //   0: foreign_0
    //   1, 2: patch A old tris -> replaced by 2 new tris
    //   3: foreign_1
    //   4, 5: patch B old tris -> replaced by 4 new tris around a newly seeded vertex (id = 8)
    let lbl_foreign_0 = side_lbl(70, [false, false], [true, true]);
    let lbl_patch_a = side_lbl(81, [false, false], [true, false]);
    let lbl_foreign_1 = side_lbl(71, [false, true], [false, true]);
    let lbl_patch_b = side_lbl(82, [true, false], [false, true]);

    let mut mesh = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0),  // 0
            pt(1.0, 0.0, 0.0),  // 1
            pt(1.0, 1.0, 0.0),  // 2
            pt(0.0, 1.0, 0.0),  // 3
            pt(2.0, 0.0, 0.0),  // 4
            pt(2.0, 1.0, 0.0),  // 5
            pt(-1.0, 0.5, 0.0), // 6 foreign_0 apex
            pt(3.0, 0.5, 0.0),  // 7 foreign_1 apex
        ],
        vec![
            [0, 3, 6], // 0: foreign_0
            [0, 1, 2], // 1: patch A old
            [0, 2, 3], // 2: patch A old
            [4, 7, 5], // 3: foreign_1
            [1, 4, 5], // 4: patch B old
            [1, 5, 2], // 5: patch B old
        ],
    );
    let mut attribution = TriangleAttributionMap {
        attributions: vec![
            attr(InputId::A, 10),
            attr(InputId::A, 1),
            attr(InputId::A, 1),
            attr(InputId::B, 11),
            attr(InputId::B, 2),
            attr(InputId::B, 2),
        ],
    };
    let mut side_labels = TriangleSideLabelMap {
        labels: vec![
            lbl_foreign_0,
            lbl_patch_a,
            lbl_patch_a,
            lbl_foreign_1,
            lbl_patch_b,
            lbl_patch_b,
        ],
    };

    let plan_verts = mesh.verts.len() as u32; // 8
    let plan_tris = mesh.tris.len() as u32; // 6
    let rebuild_a = PatchRebuild {
        patch: 0,
        old_tris: vec![1, 2],
        new_tris: vec![[0, 1, 3], [1, 2, 3]],
        new_verts: Vec::new(),
        dropped: BTreeSet::new(),
        plan_verts,
        plan_tris,
    };
    // Patch B seeds one interior Steiner point referenced as `plan_verts + 0` (= 8).
    let rebuild_b = PatchRebuild {
        patch: 1,
        old_tris: vec![4, 5],
        new_tris: vec![[1, 4, 8], [4, 5, 8], [5, 2, 8], [2, 1, 8]],
        new_verts: vec![pt(1.5, 0.5, 0.0)],
        dropped: BTreeSet::new(),
        plan_verts,
        plan_tris,
    };

    apply_rebuild_batch(
        &mut mesh,
        &mut attribution,
        &mut side_labels,
        &[rebuild_a, rebuild_b],
    )
    .expect("multi-patch batch write-back must succeed");

    assert_eq!(mesh.verts.len(), 9, "one seeded vertex appended");
    assert_eq!(mesh.tris.len(), 8, "2 foreign + 2 patch A + 4 patch B");
    assert_eq!(attribution.attributions.len(), 8);
    assert_eq!(side_labels.len(), 8);
    assert_eq!(
        side_labels.labels,
        vec![
            lbl_foreign_0,
            lbl_foreign_1,
            lbl_patch_a,
            lbl_patch_a,
            lbl_patch_b,
            lbl_patch_b,
            lbl_patch_b,
            lbl_patch_b,
        ],
        "foreign survivors keep their labels; each rebuilt patch's replacement CDT tris inherit that patch's exact side label"
    );

    // (b) `retriangulate_collapsed_fan_regions`:
    // Two-operand strip with distinct side labels per operand; after stacked
    // collapses create a fold on the minted seam edge (1, 4), the fan repair
    // re-CDTs both clusters while preserving each operand's side label.
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
    let verts = vec![
        pt(0.0, 0.0, 0.0),   // 0 s0
        pt(1.0, 0.0, 0.0),   // 1 P (mint)
        pt(1.4, 0.0, 0.0),   // 2 V1
        pt(1.6, 0.0, 0.0),   // 3 V2
        pt(2.0, 0.0, 0.0),   // 4 G (mint)
        pt(3.0, 0.0, 0.0),   // 5 s3
        pt(1.0, 0.5, 0.0),   // 6 q1
        pt(1.4, 0.5, 0.0),   // 7 m1
        pt(1.6, 0.5, 0.0),   // 8 m2
        pt(2.0, 0.5, 0.0),   // 9 q2
        pt(1.45, 0.3, 0.0),  // 10 cu
        pt(1.55, 0.35, 0.0), // 11 cv
        pt(1.5, 1.0, 0.0),   // 12 r1
        pt(2.5, 1.0, 0.0),   // 13 r2
        pt(0.5, -1.0, 0.0),  // 14 b0
        pt(1.5, -1.0, 0.0),  // 15 b1
        pt(2.5, -1.0, 0.0),  // 16 b2
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
    let lbl_op_a = side_lbl(101, [false, false], [true, false]);
    let lbl_op_b = side_lbl(202, [true, false], [false, true]);
    let mut fan_tris = Vec::new();
    let mut fan_attr = Vec::new();
    let mut fan_lbls = Vec::new();
    for &t in &a_tris {
        fan_tris.push(t);
        fan_attr.push(attr(InputId::A, 0));
        fan_lbls.push(lbl_op_a);
    }
    for &t in &b_tris {
        fan_tris.push(t);
        fan_attr.push(attr(InputId::B, 0));
        fan_lbls.push(lbl_op_b);
    }
    let mut fan_mesh = Mesh::new(verts, fan_tris);
    collapse_vertex(&mut fan_mesh, &mut fan_attr, &mut fan_lbls, 2, 1);
    collapse_vertex(&mut fan_mesh, &mut fan_attr, &mut fan_lbls, 3, 4);
    collapse_vertex(&mut fan_mesh, &mut fan_attr, &mut fan_lbls, 7, 1);
    collapse_vertex(&mut fan_mesh, &mut fan_attr, &mut fan_lbls, 8, 4);

    let brep_a = one_face(z_plane);
    let brep_b = one_face(z_plane);
    let minted: HashSet<u32> = [1u32, 4u32].into_iter().collect();
    assert!(retriangulate_collapsed_fan_regions(
        &mut fan_mesh,
        &mut fan_attr,
        &mut fan_lbls,
        &brep_a,
        &brep_b,
        &HashSet::new(),
        &minted,
    ));
    assert_eq!(fan_mesh.tris.len(), fan_attr.len());
    assert_eq!(fan_mesh.tris.len(), fan_lbls.len());
    for (ti, (at, &lbl)) in fan_attr.iter().zip(fan_lbls.iter()).enumerate() {
        let expected = match at.expect("attributed").input {
            InputId::A => lbl_op_a,
            InputId::B => lbl_op_b,
        };
        assert_eq!(
            lbl, expected,
            "post-fan-repair triangle {ti} must carry its operand's exact side label"
        );
    }
}

/// Pin 6: End-to-end `boolean()` runs through the Stage-4 hand-off `debug_assert_eq!`
/// across `Union`, `Intersect`, and `Subtract` on both transversal and coplanar-overlap
/// box pairs, confirming the carried side labels re-derive today's kept set `(0..tris.len())`.
#[test]
fn boolean_end_to_end_handoff_assertion_holds_across_ops_and_coplanar_sheets() {
    let backend = crate::native_backend().expect("native backend");
    // Transversal overlap.
    let a = rj_box([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = rj_box([1.0, 1.0, 1.0], [3.0, 3.0, 3.0]);
    for op in [BoolOp::Union, BoolOp::Intersect, BoolOp::Subtract] {
        let out = crate::boolean(&a, &b, op, &backend)
            .unwrap_or_else(|e| panic!("transversal {op:?} failed: {e:?}"));
        assert!(!out.faces().is_empty());
    }

    // Coplanar-face overlap (exercises step (3b) overlap-sheet filtering).
    let c = rj_box([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let d = rj_box([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);
    for op in [BoolOp::Union, BoolOp::Intersect, BoolOp::Subtract] {
        let out = crate::boolean(&c, &d, op, &backend)
            .unwrap_or_else(|e| panic!("coplanar {op:?} failed: {e:?}"));
        assert!(!out.faces().is_empty());
    }
}

/// Pin 7 (I3a): `TriangleSideLabel::keep_for_op_with_membrane`,
/// `flip_winding_for_op`, and `operand_winding_reversed` encode post-Stage-4
/// §4.4.2 + §4.5.5 classification on `[true, true]` overlap sheets.
#[test]
fn sheet_opposite_keep_and_winding_flip_rules() {
    let same_norm = TriangleSideLabel {
        cherchi_patch: 1,
        inside: [false, false],
        surface: [true, true],
        sheet_opposite: Some(false),
    };
    let opp_norm = TriangleSideLabel {
        cherchi_patch: 2,
        inside: [false, false],
        surface: [true, true],
        sheet_opposite: Some(true),
    };
    let unresolved = TriangleSideLabel {
        cherchi_patch: 3,
        inside: [false, false],
        surface: [true, true],
        sheet_opposite: None,
    };
    assert_eq!(
        same_norm.keep_for_op_with_membrane(BoolOp::Union),
        Some(true)
    );
    assert_eq!(
        same_norm.keep_for_op_with_membrane(BoolOp::Intersect),
        Some(true)
    );
    assert_eq!(
        same_norm.keep_for_op_with_membrane(BoolOp::Subtract),
        Some(false)
    );
    assert_eq!(
        opp_norm.keep_for_op_with_membrane(BoolOp::Union),
        Some(false)
    );
    assert_eq!(
        opp_norm.keep_for_op_with_membrane(BoolOp::Intersect),
        Some(false)
    );
    assert_eq!(
        opp_norm.keep_for_op_with_membrane(BoolOp::Subtract),
        Some(true)
    );
    assert_eq!(unresolved.keep_for_op_with_membrane(BoolOp::Union), None);

    assert!(!same_norm.flip_winding_for_op(BoolOp::Subtract));
    assert!(!opp_norm.flip_winding_for_op(BoolOp::Subtract));
    let pure_b_in_a = side_lbl(4, [true, false], [false, true]);
    assert!(pure_b_in_a.flip_winding_for_op(BoolOp::Subtract));
    assert!(!pure_b_in_a.flip_winding_for_op(BoolOp::Union));

    assert!(!same_norm.operand_winding_reversed(InputId::A));
    assert!(!same_norm.operand_winding_reversed(InputId::B));
    assert!(!opp_norm.operand_winding_reversed(InputId::A));
    assert!(opp_norm.operand_winding_reversed(InputId::B));
}

/// Pin 8 (I3a): `flood_fill_patches_with_side_labels` and
/// `merge_same_plane_patches_with_side_labels` keep `A_out` and `A_in` separate
/// across the intersection curve while merging adjacent coplanar faces that
/// share the same `TriangleSideLabel`.
#[test]
fn full_arrangement_flood_fill_and_same_plane_merge_respect_side_labels() {
    let z_plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let two_plane_brep = BRep::new(
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
        vec![
            BRepFace {
                surface: z_plane,
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: z_plane,
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ],
    )
    .expect("two-plane brep");

    // Strip of 3 triangles:
    // tri 0 (A, face 0, patch 10 out) --edge (1,2)-- tri 1 (A, face 1, patch 10 out)
    //                                 --edge (2,3)-- tri 2 (A, face 1, patch 20 in)
    let mesh = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0), // 0
            pt(1.0, 0.0, 0.0), // 1
            pt(0.5, 1.0, 0.0), // 2
            pt(1.5, 1.0, 0.0), // 3
            pt(2.0, 0.0, 0.0), // 4
        ],
        vec![
            [0, 1, 2], // 0: face 0, patch 10 (out)
            [1, 3, 2], // 1: face 1, patch 10 (out)
            [1, 4, 3], // 2: face 1, patch 20 (in)
        ],
    );
    let attribution = TriangleAttributionMap {
        attributions: vec![
            attr(InputId::A, 0),
            attr(InputId::A, 1),
            attr(InputId::A, 1),
        ],
    };
    let lbl_out = side_lbl(10, [false, false], [true, false]);
    let lbl_in = side_lbl(20, [false, true], [true, false]);
    let side_labels = TriangleSideLabelMap {
        labels: vec![lbl_out, lbl_out, lbl_in],
    };
    let adj = triangle_adjacency(&mesh);
    let raw_patches =
        flood_fill_patches_with_side_labels(&mesh, &attribution, Some(&side_labels), &adj);
    assert_eq!(
        raw_patches.len(),
        3,
        "tri 1 and tri 2 share (A, face 1) but differ in cherchi_patch/inside, so flood keeps them separate"
    );

    let merged = merge_same_plane_patches_with_side_labels(
        raw_patches,
        &adj,
        Some(&side_labels),
        &two_plane_brep,
        &two_plane_brep,
    );
    assert_eq!(
        merged.len(),
        2,
        "same-side-label coplanar patches [0,1] merge, while A_in [2] stays split across the curve"
    );
    assert_eq!(merged[0].tri_indices, vec![0, 1]);
    assert_eq!(merged[1].tri_indices, vec![2]);
}

/// Pin 9 (I3a): `compute_phase_a_with_side_labels` deduplicates a 4-patch
/// transverse curve edge `(A_out, A_in, B_out, B_in)` to 2 `(InputId, Surface)`
/// entries so `build_intersection_curves` recovers the exact `Curve::Line`, and
/// resolves a 3-patch `[true, true]` overlap-sheet boundary to the 2 transverse
/// surfaces.
#[test]
fn full_arrangement_phase_a_dedupes_four_patch_curve_and_overlap_boundary() {
    // `brep_a`: radius-1 cylinder z ∈ [-2, 0] (`face 0` = cylinder wall, `face 2` = z=0 cap).
    // `brep_b`: box [-2..2, 0..2, 0..2] (`face 0` = z=0 bottom with -z normal, `face 5` = y=0 wall).
    let brep_a = rj_cylinder([0.0, 0.0, -2.0], [0.0, 0.0, 1.0], 1.0, 2.0);
    let brep_b = rj_box([-2.0, 0.0, 0.0], [2.0, 2.0, 2.0]);

    // (a) 4-patch transverse edge (0, 1) along x-axis (y=0, z=0):
    // A_out (z=0, y>0), A_in (z=0, y<0), B_out (y=0, z>0), B_in (y=0, z<0).
    let mesh4 = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0),  // 0
            pt(1.0, 0.0, 0.0),  // 1
            pt(0.5, 1.0, 0.0),  // 2: A_out
            pt(0.5, -1.0, 0.0), // 3: A_in
            pt(0.5, 0.0, 1.0),  // 4: B_out
            pt(0.5, 0.0, -1.0), // 5: B_in
        ],
        vec![
            [0, 1, 2], // A_out
            [1, 0, 3], // A_in
            [1, 0, 4], // B_out
            [0, 1, 5], // B_in
        ],
    );
    let attr4 = TriangleAttributionMap {
        attributions: vec![
            attr(InputId::A, 2),
            attr(InputId::A, 2),
            attr(InputId::B, 5),
            attr(InputId::B, 5),
        ],
    };
    let lbls4 = TriangleSideLabelMap {
        labels: vec![
            side_lbl(1, [false, false], [true, false]),
            side_lbl(2, [false, true], [true, false]),
            side_lbl(3, [false, false], [false, true]),
            side_lbl(4, [true, false], [false, true]),
        ],
    };
    let (_infos_unscoped, _inc_unscoped, curves_unscoped) = compute_phase_a(
        &mesh4,
        &attr4,
        &brep_a,
        &brep_b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("unscoped phase a");
    assert!(
        !curves_unscoped.contains_key(&(0, 1)),
        "without side labels, A_out+A_in and B_out+B_in merge across (0,1) and lose the curve"
    );

    let (_infos_scoped, inc_scoped, curves_scoped) = compute_phase_a_with_side_labels(
        &mesh4,
        &attr4,
        Some(&lbls4),
        &brep_a,
        &brep_b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("scoped phase a");
    assert_eq!(inc_scoped.get(&(0, 1)).map(Vec::len), Some(2));
    assert!(
        matches!(curves_scoped.get(&(0, 1)), Some(Curve::LineSegment)),
        "4-patch transverse seam must dedupe to 2 (InputId, Surface) entries and recover Curve::LineSegment"
    );

    // (b) 3-patch `[true, true]` overlap boundary along rim chord (0, 1) on unit circle in z=0:
    // tri 0 = `[true, true]` cap overlap attributed to `(A, 2 = z=0 cap)`,
    // tri 1 = `[true, false]` cylinder wall `(A, 0 = cyl_a)`,
    // tri 2 = `[false, true]` plane continuation `(B, 0 = z=0 bottom, opposite normal)`.
    let mesh3 = Mesh::new(
        vec![
            pt(1.0, 0.0, 0.0),  // 0 on unit circle z=0
            pt(0.0, 1.0, 0.0),  // 1 on unit circle z=0
            pt(0.0, 0.0, 0.0),  // 2 inside cap overlap
            pt(0.7, 0.7, -1.0), // 3 on cylinder wall side
            pt(1.2, 1.2, 0.0),  // 4 outside cap on B's z=0 plane
        ],
        vec![
            [0, 1, 2], // [true, true] overlap (attributed to A face 2 = z=0 cap)
            [1, 0, 3], // [true, false] A face 0 = cyl_a
            [1, 0, 4], // [false, true] B face 0 = z=0 bottom
        ],
    );
    let attr3 = TriangleAttributionMap {
        attributions: vec![
            attr(InputId::A, 2),
            attr(InputId::A, 0),
            attr(InputId::B, 0),
        ],
    };
    let lbls3 = TriangleSideLabelMap {
        labels: vec![
            TriangleSideLabel {
                cherchi_patch: 10,
                inside: [false, false],
                surface: [true, true],
                sheet_opposite: Some(false),
            },
            side_lbl(11, [false, false], [true, false]),
            side_lbl(12, [false, false], [false, true]),
        ],
    };
    let (_infos3, inc3, curves3) = compute_phase_a_with_side_labels(
        &mesh3,
        &attr3,
        Some(&lbls3),
        &brep_a,
        &brep_b,
        &crate::stage3_ssi::NO_EDGE_PROVENANCE,
    )
    .expect("overlap-boundary phase a");
    assert_eq!(inc3.get(&(0, 1)).map(Vec::len), Some(2));
    assert!(
        matches!(curves3.get(&(0, 1)), Some(Curve::Circle { .. })),
        "overlap boundary must drop the redundant (A, plane_a) overlap entry and recover cyl_a ∩ plane_a = Circle"
    );
}

/// Pin 10 (I3a): `collapse_vertex_scoped` and `remove_doubled_membranes_scoped`
/// preserve cross-operand opposite-winding coplanar triangles on the full
/// arrangement while still cancelling genuine intra-patch pleats.
#[test]
fn scoped_collapse_and_membrane_removal_preserve_cross_operand_coplanar_sheets() {
    let lbl_a = side_lbl(1, [false, false], [true, false]);
    let lbl_b = side_lbl(2, [false, false], [false, true]);

    // Opposite-winding identical triangles `[0, 1, 2]` (operand A) and `[0, 2, 1]`
    // (operand B), plus an intra-operand-A pleat `[0, 2, 3]` + `[0, 3, 2]`.
    let verts = vec![
        pt(0.0, 0.0, 0.0),
        pt(1.0, 0.0, 0.0),
        pt(0.0, 1.0, 0.0),
        pt(1.0, 1.0, 0.0),
    ];
    let tris = vec![
        [0, 1, 2], // A legitimate sheet
        [0, 2, 1], // B legitimate opposite-winding sheet
        [0, 2, 3], // A pleat half 1
        [0, 3, 2], // A pleat half 2
    ];
    let attrs = vec![
        attr(InputId::A, 0),
        attr(InputId::B, 0),
        attr(InputId::A, 0),
        attr(InputId::A, 0),
    ];
    let lbls = vec![lbl_a, lbl_b, lbl_a, lbl_a];

    // Unscoped `remove_doubled_membranes` drops ALL 4 triangles.
    let mut m_unscoped = Mesh::new(verts.clone(), tris.clone());
    let mut a_unscoped = attrs.clone();
    let mut l_unscoped = lbls.clone();
    assert_eq!(
        remove_doubled_membranes(&mut m_unscoped, &mut a_unscoped, &mut l_unscoped),
        4
    );
    assert!(m_unscoped.tris.is_empty());

    // Scoped `remove_doubled_membranes_scoped(..., true)` drops ONLY the 2
    // intra-patch pleat triangles, preserving A's and B's coplanar sheets.
    let mut m_scoped = Mesh::new(verts.clone(), tris);
    let mut a_scoped = attrs;
    let mut l_scoped = lbls;
    assert_eq!(
        remove_doubled_membranes_scoped(&mut m_scoped, &mut a_scoped, &mut l_scoped, true),
        2
    );
    assert_eq!(m_scoped.tris, vec![[0, 1, 2], [0, 2, 1]]);
    assert_eq!(l_scoped, vec![lbl_a, lbl_b]);

    // Likewise `collapse_vertex_scoped(..., true)` preserves `[0, 1, 2]` (A) vs
    // `[0, 2, 1]` (B) when vertex 4 collapses onto vertex 2.
    let mut m_col = Mesh::new(
        vec![
            pt(0.0, 0.0, 0.0), // 0
            pt(1.0, 0.0, 0.0), // 1
            pt(0.0, 1.0, 0.0), // 2 survivor
            pt(0.0, 1.0, 0.0), // 3 victim -> 2
        ],
        vec![
            [0, 1, 2], // A
            [0, 3, 1], // B -> becomes [0, 2, 1] after 3 -> 2
            [0, 2, 3], // A degenerate -> dropped
        ],
    );
    let mut a_col = vec![
        attr(InputId::A, 0),
        attr(InputId::B, 0),
        attr(InputId::A, 0),
    ];
    let mut l_col = vec![lbl_a, lbl_b, lbl_a];
    let mut tv: BTreeSet<u32> = [3u32].into_iter().collect();
    let dropped = collapse_vertex_scoped(
        &mut m_col,
        &mut a_col,
        &mut l_col,
        3,
        2,
        true,
        Some(&mut tv),
    );
    assert_eq!(
        dropped, 1,
        "only the degenerate [0,2,2] triangle drops; cross-operand [0,1,2]+[0,2,1] survive"
    );
    assert_eq!(m_col.tris, vec![[0, 1, 2], [0, 2, 1]]);
    assert!(
        tv.contains(&2) && !tv.contains(&3),
        "tangential vertex remaps victim -> survivor"
    );
}

/// Pin 11 (I3a): `resolve_synchronized_degenerate_split` unzips a 4-triangle
/// full-arrangement A×B seam edge `(a, c)` across BOTH operands atomically,
/// keeping both operand shells closed 2-manifolds AND keeping every post-keep
/// Boolean result (`Union`, `Intersect`, `Subtract`) watertight.
#[test]
fn synchronized_four_triangle_degenerate_split_keeps_both_operands_conformal() {
    // Operand A: closed surface over shared edge (0, 2) with collinear midpoint 1:
    //   d_idx = [0, 2, 1] (lbl_a_out, degenerate),
    //   a_ab  = [0, 1, 3] (lbl_a_out),
    //   a_bc  = [1, 2, 3] (lbl_a_out),
    //   a_top1= [3, 1, 4] ... wait: build exact closed shells!
    // Closed shell A (5 verts {0,1,2,3,4}):
    //   top (lbl_a_out): [0, 1, 3], [1, 2, 3], [0, 2, 1] (boundary 0->2, 2->3, 3->0)
    //   bottom (lbl_a_in): [2, 0, 4], [3, 2, 4], [0, 3, 4] (boundary 2->0, 0->3, 3->2)
    // Check A half-edges:
    //   (0,1)/(1,0) paired between [0,1,3] and [0,2,1];
    //   (1,2)/(2,1) paired between [1,2,3] and [0,2,1];
    //   (0,2)/(2,0) paired between [0,2,1] and [2,0,4];
    //   (1,3)/(3,1) paired between [0,1,3] and [1,2,3];
    //   (2,3)/(3,2) paired between [1,2,3] and [3,2,4];
    //   (3,0)/(0,3) paired between [0,1,3] and [0,3,4];
    //   (0,4)/(4,0), (2,4)/(4,2), (3,4)/(4,3) paired in bottom!
    // Closed shell B (4 verts {0,2,5,6}, tetrahedron sharing edge (0, 2)):
    //   o1 (lbl_b_in_a):  [0, 2, 5] (inside A -> boundary 0->2)
    //   o2 (lbl_b_out_a): [2, 0, 6] (outside A -> boundary 2->0)
    //   b3 (lbl_b_out_a): [2, 5, 6] ... wait: close o1 and o2 cleanly!
    // Let B be a bipyramid over triangle (0, 2, 7) with apexes 5 (inside A) and 6 (outside A):
    //   inside A (lbl_b_in):   [0, 2, 5], [2, 7, 5], [7, 0, 5]  (boundary 0->2->7->0)
    //   outside A (lbl_b_out): [2, 0, 6], [0, 7, 6], [7, 2, 6]  (boundary 0<-2<-7<-0)
    // Wait: then A's boundary between A_out and A_in should ALSO be 0->2->7->0 so A and B
    // intersect along the closed triangle loop (0, 2, 7)!
    // Let's check! Closed bipyramid A over loop (0, 2, 7) with apex 3 (outside B) and 4 (inside B),
    // with vertex 1 subdivided on edge (0, 2) of A_out:
    //   A_out (lbl_a_out = [F, F]): [0, 2, 1] (degen!), [0, 1, 3], [1, 2, 3], [2, 7, 3], [7, 0, 3]
    //   A_in  (lbl_a_in  = [F, T]): [2, 0, 4], [0, 7, 4], [7, 2, 4]
    let verts = vec![
        pt(0.0, 0.0, 0.0),  // 0: a
        pt(0.5, 0.0, 0.0),  // 1: b (collinear midpoint on 0..2)
        pt(1.0, 0.0, 0.0),  // 2: c
        pt(0.5, 0.3, 1.0),  // 3: A_out apex
        pt(0.5, 0.3, -1.0), // 4: A_in apex
        pt(0.5, 0.3, 0.5),  // 5: B_in apex
        pt(0.5, 0.3, -0.5), // 6: B_out apex
        pt(0.5, 1.0, 0.0),  // 7: third vertex of intersection loop (0, 2, 7)
    ];
    let lbl_a_out = side_lbl(10, [false, false], [true, false]);
    let lbl_a_in = side_lbl(11, [false, true], [true, false]);
    let lbl_b_in = side_lbl(20, [true, false], [false, true]);
    let lbl_b_out = side_lbl(21, [false, false], [false, true]);

    let tris = vec![
        [0, 2, 1], // 0: d_idx (A_out, degenerate)
        [0, 1, 3], // 1: A_out
        [1, 2, 3], // 2: A_out
        [2, 7, 3], // 3: A_out
        [7, 0, 3], // 4: A_out
        [2, 0, 4], // 5: n_same (A_in)
        [0, 7, 4], // 6: A_in
        [7, 2, 4], // 7: A_in
        [0, 2, 5], // 8: o1 (B_in)
        [2, 7, 5], // 9: B_in
        [7, 0, 5], // 10: B_in
        [2, 0, 6], // 11: o2 (B_out)
        [0, 7, 6], // 12: B_out
        [7, 2, 6], // 13: B_out
    ];
    let mut attr_vec: Vec<Option<TriangleAttribution>> = (0..14)
        .map(|i| {
            if i < 8 {
                attr(InputId::A, 0)
            } else {
                attr(InputId::B, 0)
            }
        })
        .collect();
    let mut lbl_vec = vec![
        lbl_a_out, lbl_a_out, lbl_a_out, lbl_a_out, lbl_a_out, lbl_a_in, lbl_a_in, lbl_a_in,
        lbl_b_in, lbl_b_in, lbl_b_in, lbl_b_out, lbl_b_out, lbl_b_out,
    ];
    let mut mesh = Mesh::new(verts, tris);
    let mut tv: BTreeSet<u32> = [0u32, 2u32].into_iter().collect();

    assert!(resolve_synchronized_degenerate_split(
        &mut mesh,
        &mut attr_vec,
        &mut lbl_vec,
        0,
        5,
        [8, 11],
        (0, 2, 1),
        Some(&mut tv),
    ));
    assert!(
        tv.contains(&1),
        "midpoint of tangential edge inherits tangential tag"
    );

    let attr_map = TriangleAttributionMap {
        attributions: attr_vec,
    };
    let lbl_map = TriangleSideLabelMap {
        labels: lbl_vec.clone(),
    };
    check_watertight_2manifold_per_operand(&mesh, &attr_map, &lbl_map, false)
        .expect("both operand shells must remain closed 2-manifolds after synchronized split");

    for op in [BoolOp::Union, BoolOp::Intersect, BoolOp::Subtract] {
        let kept_tris: Vec<[u32; 3]> = mesh
            .tris
            .iter()
            .zip(lbl_vec.iter())
            .filter_map(|(&t, lbl)| {
                if !lbl.keep_for_op_with_membrane(op).unwrap_or(false) {
                    return None;
                }
                let mut kt = t;
                if lbl.flip_winding_for_op(op) {
                    kt.swap(1, 2);
                }
                Some(kt)
            })
            .collect();
        let kept_mesh = Mesh::new(mesh.verts.clone(), kept_tris);
        check_watertight_2manifold(&kept_mesh).unwrap_or_else(|e| {
            panic!("post-keep {op:?} mesh must be watertight after synchronized split: {e:?}")
        });
    }
}

/// Pin 12 (I3a): `check_watertight_2manifold_per_operand` rejects paired A+B
/// holes whose unpaired half-edges cancel each other in the global half-edge
/// multiset (`specs/yang_442_classify_after_update.md` §4 table row 8).
#[test]
fn per_operand_watertight_gate_catches_cancelling_cross_operand_holes() {
    let verts = vec![
        pt(0.0, 0.0, 0.0),  // 0
        pt(1.0, 0.0, 0.0),  // 1
        pt(0.0, 1.0, 0.0),  // 2
        pt(0.2, 0.2, 1.0),  // 3: A apex
        pt(0.2, 0.2, -1.0), // 4: B apex
    ];
    // Tetrahedron A missing base `[0, 2, 1]` (unpaired: 0->1, 1->2, 2->0);
    // Tetrahedron B missing base `[0, 1, 2]` (unpaired: 1->0, 2->1, 0->2).
    // Together, every directed edge has fwd == rev == 1 and V=5, E=9, F=6 -> chi=2!
    let mesh = Mesh::new(
        verts,
        vec![
            [0, 1, 3],
            [1, 2, 3],
            [2, 0, 3],
            [1, 0, 4],
            [2, 1, 4],
            [0, 2, 4],
        ],
    );
    let attribution = TriangleAttributionMap {
        attributions: vec![
            attr(InputId::A, 0),
            attr(InputId::A, 0),
            attr(InputId::A, 0),
            attr(InputId::B, 0),
            attr(InputId::B, 0),
            attr(InputId::B, 0),
        ],
    };
    let side_labels = TriangleSideLabelMap {
        labels: vec![
            side_lbl(1, [false, false], [true, false]),
            side_lbl(1, [false, false], [true, false]),
            side_lbl(1, [false, false], [true, false]),
            side_lbl(2, [false, false], [false, true]),
            side_lbl(2, [false, false], [false, true]),
            side_lbl(2, [false, false], [false, true]),
        ],
    };
    assert!(
        check_watertight_2manifold(&mesh).is_ok(),
        "global half-edge balance passes vacuously on paired A+B cancelling holes"
    );
    assert!(
        check_watertight_2manifold_per_operand(&mesh, &attribution, &side_labels, false).is_err(),
        "per-operand closed-manifold gate must reject open operand shells"
    );
}

/// Pin 13 (I3a): Within the SAME operand (`surface = [true, false]`),
/// `collapse_vertex_scoped(..., true)` and `remove_doubled_membranes_scoped(..., true)`
/// must NEVER cancel an opposite-winding triangle in `A-out` (`inside = [false, false]`,
/// kept for Union/Subtract) against one in `A-in` (`inside = [false, true]`, dropped
/// for Union/Subtract), because cancelling kept-against-dropped leaves M_A globally
/// balanced while tearing `A-out`'s boundary cycle after post-Stage-4 keep
/// (`yr10_adversary::adv2` and `tangency_pinch_split::c0058` regression class).
#[test]
fn scoped_collapse_and_membrane_removal_preserve_kept_vs_dropped_within_same_operand() {
    let lbl_a_out = side_lbl(10, [false, false], [true, false]);
    let lbl_a_in = side_lbl(11, [false, true], [true, false]);

    let verts = vec![
        pt(0.0, 0.0, 0.0),
        pt(1.0, 0.0, 0.0),
        pt(0.0, 1.0, 0.0),
        pt(0.0, 1.0, 0.0), // 3 coincides with 2 (collapse victim)
    ];

    // 1. `remove_doubled_membranes_scoped(..., true)` preserves `[0, 1, 2]` (`A-out`)
    //    vs `[0, 2, 1]` (`A-in`).
    let mut m_mem = Mesh::new(verts.clone(), vec![[0, 1, 2], [0, 2, 1]]);
    let mut a_mem = vec![attr(InputId::A, 0), attr(InputId::A, 0)];
    let mut l_mem = vec![lbl_a_out, lbl_a_in];
    assert_eq!(
        remove_doubled_membranes_scoped(&mut m_mem, &mut a_mem, &mut l_mem, true),
        0,
        "kept (`A-out`) vs dropped (`A-in`) opposite-winding sheets must not cancel before keep"
    );
    assert_eq!(m_mem.tris.len(), 2);

    // 2. `collapse_vertex_scoped(..., true)` collapsing 3 -> 2 preserves
    //    `[0, 1, 2]` (`A-out`) vs `[0, 3, 1] -> [0, 2, 1]` (`A-in`).
    let mut m_col = Mesh::new(verts, vec![[0, 1, 2], [0, 3, 1], [0, 2, 3]]);
    let mut a_col = vec![
        attr(InputId::A, 0),
        attr(InputId::A, 0),
        attr(InputId::A, 0),
    ];
    let mut l_col = vec![lbl_a_out, lbl_a_in, lbl_a_in];
    let dropped = collapse_vertex_scoped(&mut m_col, &mut a_col, &mut l_col, 3, 2, true, None);
    assert_eq!(
        dropped, 1,
        "only the degenerate `[0,2,2]` triangle drops; `A-out` vs `A-in` survive"
    );
    assert_eq!(m_col.tris, vec![[0, 1, 2], [0, 2, 1]]);
    assert_eq!(l_col, vec![lbl_a_out, lbl_a_in]);
}

/// Pin 14 (I3a): `check_watertight_2manifold_per_operand` counts independent
/// `(1 fwd, 1 rev)` interior diagonals of distinct `TriangleSideLabel` patches
/// as distinct 1-cells in CW-complex Euler characteristic, while still
/// rejecting intra-patch double-cover folds (`chi = 3`) loudly.
#[test]
fn per_operand_manifold_gate_distinguishes_cross_patch_interior_chord_from_fold() {
    // Vertices 0..4: operand B's two patches (`B-out` and `B-in`) glued along
    // 4-cycle equator 0-1-2-3, BOTH triangulated with interior chord `(0, 2)`.
    // Vertices 4..8: operand A's valid closed tetrahedron.
    let verts = vec![
        pt(0.0, 0.0, 0.0),
        pt(1.0, 0.0, 0.0),
        pt(1.0, 1.0, 0.0),
        pt(0.0, 1.0, 0.0),
        pt(2.0, 0.0, 0.0),
        pt(3.0, 0.0, 0.0),
        pt(2.0, 1.0, 0.0),
        pt(2.0, 0.0, 1.0),
    ];
    let tris = vec![
        // B-out: boundary 2->1->0->3->2, interior diagonal (0, 2) = 1 fwd + 1 rev.
        [0, 2, 1],
        [0, 3, 2],
        // B-in: boundary 0->1->2->3->0, interior diagonal (0, 2) = 1 fwd + 1 rev.
        [0, 1, 2],
        [0, 2, 3],
        // A-out: closed tetrahedron on 4..8.
        [4, 6, 5],
        [4, 5, 7],
        [5, 6, 7],
        [6, 4, 7],
    ];
    let mesh = Mesh::new(verts, tris);
    let attrs = TriangleAttributionMap {
        attributions: vec![
            attr(InputId::B, 0),
            attr(InputId::B, 0),
            attr(InputId::B, 0),
            attr(InputId::B, 0),
            attr(InputId::A, 0),
            attr(InputId::A, 1),
            attr(InputId::A, 2),
            attr(InputId::A, 3),
        ],
    };
    let b_out = side_lbl(10, [false, false], [false, true]);
    let b_in = side_lbl(11, [true, false], [false, true]);
    let a_out = side_lbl(20, [false, false], [true, false]);

    // 1. Distinct patches `B-out` and `B-in` each carrying `(0, 2)` as a
    //    balanced interior diagonal form a genus-0 sphere (V=4, E_cw=6, F=4 => chi=2).
    let split_labels = TriangleSideLabelMap {
        labels: vec![b_out, b_out, b_in, b_in, a_out, a_out, a_out, a_out],
    };
    assert!(
        crate::stage4_relocate::check_watertight_2manifold_per_operand(
            &mesh,
            &attrs,
            &split_labels,
            false,
        )
        .is_ok(),
        "two distinct patches glued along equator 0-1-2-3 with independent interior chord (0,2) must pass with chi=2"
    );

    // 2. Identical geometry inside ONE patch (`b_out` on all four B triangles)
    //    is an intra-patch double-cover fold (`fwd=2, rev=2` in one patch =>
    //    E=5, chi=3) and must still reject with `NonManifoldOutput`.
    let folded_labels = TriangleSideLabelMap {
        labels: vec![b_out, b_out, b_out, b_out, a_out, a_out, a_out, a_out],
    };
    assert!(
        matches!(
            crate::stage4_relocate::check_watertight_2manifold_per_operand(
                &mesh,
                &attrs,
                &folded_labels,
                false,
            ),
            Err(crate::YangError::NonManifoldOutput)
        ),
        "intra-patch double-cover fold (chi=3) must still fail loudly"
    );
}
