//! The twelve synthetic `smoke_*` scenarios of `tests/assay_kv2.rs`, as
//! builders — one source for the smoke gate (which asserts each is
//! SUPPORTED_CORRECT with its exact volume) and the metamorphic suite
//! (`tests/assay_metamorphic.rs`, which checks rigid-motion equivariance,
//! inclusion–exclusion and commutativity on the same documents).
//!
//! Every builder drives the full dispatch path (wasm-bridge →
//! feature-engine → `KernelV2Adapter`) and panics if a step is refused:
//! these are the always-supported scenarios.

use crate::workflow::ModelBuilder;

/// The scenario names, in `assay_kv2.rs` order.
pub const SMOKE_SCENARIOS: [&str; 12] = [
    "single_box",
    "thin_slab",
    "oblique_plane_box",
    "l_shaped_extrude",
    "union_offset_boss",
    "union_face_to_face_stack",
    "union_flush_same_radius_cylinder_stack",
    "subtract_offset_boxes",
    "intersect_offset_boxes",
    "blind_pocket_cut",
    "through_hole_cut",
    "two_standalone_bodies",
];

/// Build the named smoke scenario in a fresh kernel-v2 model.
///
/// # Panics
/// On an unknown name, or when a modeling step is refused.
pub fn build_smoke_scenario(name: &str) -> ModelBuilder {
    let z = [0.0, 0.0, 1.0];
    let mut b = ModelBuilder::kernel_v2();
    match name {
        "single_box" => {
            b.rect_sketch("s", [0.0; 3], z, 0.0, 0.0, 1.0, 1.0).unwrap();
            b.extrude("e", "s", 2.0).unwrap();
        }
        "thin_slab" => {
            b.rect_sketch("s", [0.0; 3], z, -2.0, -1.5, 4.0, 3.0)
                .unwrap();
            b.extrude("e", "s", 0.2).unwrap();
        }
        "oblique_plane_box" => {
            // Sketch plane with a non-axis-aligned unit normal (1, 2, 2)/3.
            let n = [1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0];
            b.rect_sketch("s", [0.5, -0.25, 0.75], n, 0.0, 0.0, 1.0, 0.8)
                .unwrap();
            b.extrude("e", "s", 0.6).unwrap();
        }
        "l_shaped_extrude" => {
            // Non-convex profile: L-shape of area 3.
            b.polygon_sketch(
                "s",
                [0.0; 3],
                z,
                &[
                    (0.0, 0.0),
                    (2.0, 0.0),
                    (2.0, 1.0),
                    (1.0, 1.0),
                    (1.0, 2.0),
                    (0.0, 2.0),
                ],
            )
            .unwrap();
            b.extrude("e", "s", 0.5).unwrap();
        }
        "union_offset_boss" => {
            // Box A: (0..1)² × z∈[0,1]. Boss B: (0.3..0.7)² sketched at
            // z=0.25, extruded 1.5 → z∈[0.25,1.75]. Overlapping, NO coplanar
            // face pairs. merge=true auto-unions.
            b.rect_sketch("sa", [0.0; 3], z, 0.0, 0.0, 1.0, 1.0)
                .unwrap();
            b.extrude("a", "sa", 1.0).unwrap();
            b.rect_sketch("sb", [0.0, 0.0, 0.25], z, 0.3, 0.3, 0.4, 0.4)
                .unwrap();
            b.extrude("u", "sb", 1.5).unwrap();
        }
        "union_face_to_face_stack" => {
            // 2-unit cube, 1-unit cube sketched ON its top plane: a coplanar
            // face pair where neither operand swallows the other.
            b.rect_sketch("sa", [0.0; 3], z, 0.0, 0.0, 2.0, 2.0)
                .unwrap();
            b.extrude("a", "sa", 2.0).unwrap();
            b.rect_sketch("sb", [0.0, 0.0, 2.0], z, 0.5, 0.5, 1.0, 1.0)
                .unwrap();
            b.extrude("boss", "sb", 1.0).unwrap();
        }
        "union_flush_same_radius_cylinder_stack" => {
            // C0044 ops 1–2: two identical r=1 discs stacked flush.
            b.true_circle_sketch("sa", [0.0; 3], z, 0.0, 0.0, 1.0)
                .unwrap();
            b.extrude("a", "sa", 1.0).unwrap();
            b.true_circle_sketch("sb", [0.0, 0.0, 1.0], z, 0.0, 0.0, 1.0)
                .unwrap();
            b.extrude("boss", "sb", 1.0).unwrap();
        }
        "subtract_offset_boxes" | "intersect_offset_boxes" => {
            // Blank (0..1)³, tool (0.4..1.4)² × z∈[-0.3,0.6] — offset on all
            // axes, no coplanar pairs.
            b.rect_sketch("sa", [0.0; 3], z, 0.0, 0.0, 1.0, 1.0)
                .unwrap();
            b.extrude_no_merge("a", "sa", 1.0).unwrap();
            b.rect_sketch("sb", [0.0, 0.0, -0.3], z, 0.4, 0.4, 1.0, 1.0)
                .unwrap();
            b.extrude_no_merge("t", "sb", 0.9).unwrap();
            if name == "subtract_offset_boxes" {
                b.boolean_subtract("cut", "a", "t").unwrap();
            } else {
                b.boolean_intersect("common", "a", "t").unwrap();
            }
        }
        "blind_pocket_cut" => {
            // Box (0..1)³; cut tool (0.3..0.6)² sketched at z=1.5, depth 1.2
            // → tool z∈[0.3,1.5] (the cut path auto-reverses).
            b.rect_sketch("sa", [0.0; 3], z, 0.0, 0.0, 1.0, 1.0)
                .unwrap();
            b.extrude("a", "sa", 1.0).unwrap();
            b.rect_sketch("sb", [0.0, 0.0, 1.5], z, 0.3, 0.3, 0.3, 0.3)
                .unwrap();
            b.extrude_cut("pocket", "sb", 1.2).unwrap();
        }
        "through_hole_cut" => {
            // Same box; the tool pierces both caps → genus-1 through-hole.
            b.rect_sketch("sa", [0.0; 3], z, 0.0, 0.0, 1.0, 1.0)
                .unwrap();
            b.extrude("a", "sa", 1.0).unwrap();
            b.rect_sketch("sb", [0.0, 0.0, 1.5], z, 0.3, 0.3, 0.3, 0.3)
                .unwrap();
            b.extrude_cut("hole", "sb", 1.75).unwrap();
        }
        "two_standalone_bodies" => {
            b.rect_sketch("sa", [0.0; 3], z, 0.0, 0.0, 1.0, 1.0)
                .unwrap();
            b.extrude_no_merge("a", "sa", 1.0).unwrap();
            b.rect_sketch("sb", [3.0, 0.0, 0.0], z, 0.0, 0.0, 0.5, 0.5)
                .unwrap();
            b.extrude_no_merge("b", "sb", 0.5).unwrap();
        }
        other => panic!("unknown smoke scenario {other:?}"),
    }
    b
}
