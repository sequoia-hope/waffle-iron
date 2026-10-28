//! F4 / N91, N92 — `C0116` pass-0 small-loop resolution and `C0118`
//! sub-`MIN_FEATURE_SIZE` twin-sheet suppression render pin.
//!
//! 1. `C0116` (`R_A = 0.5` along `+Z`, `R_B = 0.3` along `+X` at `y = 0.79`,
//!    `1e-2` penetration `>> MIN_FEATURE_SIZE`) meets in a closed degree-4
//!    small loop resolved on pass 0 via §4.2.2 Gauss-map admission + Fig. 10(c)
//!    barycenter seeding + Bajaj closed-loop tracing, and renders cleanly with
//!    zero non-manifold (`count != 2`) `f32` edges.
//! 2. `C0118` (`y = 0.79999999`, `1e-8` penetration `< MIN_FEATURE_SIZE`) has
//!    twin surface sheets separated by `|t_A - t_B| = 1e-8 < MIN_FEATURE_SIZE`:
//!    `solve_common_axis_interior_caps` suppresses the sub-`MIN_FEATURE_SIZE`
//!    twin-sheet splice rather than emitting sheets `1e-8` apart, preserving a
//!    clean 2-manifold render mesh without renderer collar workarounds.

use std::collections::HashMap;

use kernel_v2::KernelV2Adapter;
use waffle_types::kernel::{Kernel, RenderMesh};
use waffle_types::{CircleProfile, ClosedProfile};

fn circle(radius: f64) -> (ClosedProfile, HashMap<u32, (f64, f64)>) {
    let mut positions = HashMap::new();
    positions.insert(1, (0.0, 0.0));
    let profile = ClosedProfile {
        entity_ids: vec![2],
        is_outer: true,
        vertex_ids: vec![],
        circle: Some(CircleProfile {
            center_u: 0.0,
            center_v: 0.0,
            radius,
        }),
        spline_segments: vec![],
        arc_segments: vec![],
    };
    (profile, positions)
}

fn non_manifold_edges(mesh: &RenderMesh) -> Vec<([f32; 3], [f32; 3], usize)> {
    let key = |i: u32| -> [u32; 3] {
        let k = i as usize * 3;
        [
            mesh.vertices[k].to_bits(),
            mesh.vertices[k + 1].to_bits(),
            mesh.vertices[k + 2].to_bits(),
        ]
    };
    let mut counts: HashMap<([u32; 3], [u32; 3]), usize> = HashMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (key(t[i]), key(t[j]));
            let e = if a <= b { (a, b) } else { (b, a) };
            *counts.entry(e).or_insert(0) += 1;
        }
    }
    let pos = |k: [u32; 3]| {
        [
            f32::from_bits(k[0]),
            f32::from_bits(k[1]),
            f32::from_bits(k[2]),
        ]
    };
    let mut bad: Vec<_> = counts
        .into_iter()
        .filter(|(_, c)| *c != 2)
        .map(|((a, b), c)| (pos(a), pos(b), c))
        .collect();
    bad.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    bad
}

fn run_perpendicular_grazing_union(y_offset: f64) -> RenderMesh {
    let mut k = KernelV2Adapter::new();
    let (ca, pa) = circle(0.5);
    let fa = k
        .make_faces_from_profiles(
            &[ca],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &pa,
        )
        .expect("cyl A profile");
    let cyl_a = k
        .extrude_face(fa[0], [0.0, 0.0, 1.0], 2.0)
        .expect("cyl A extrude");

    let (cb, pb) = circle(0.3);
    let fb = k
        .make_faces_from_profiles(
            &[cb],
            [-1.5, y_offset, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            &pb,
        )
        .expect("cyl B profile");
    let cyl_b = k
        .extrude_face(fb[0], [1.0, 0.0, 0.0], 3.5)
        .expect("cyl B extrude");

    let union = k
        .boolean_union(&cyl_a, &cyl_b)
        .expect("perpendicular grazing cylinder union must succeed");
    k.tessellate(&union, 0.001).expect("render tessellation")
}

#[test]
fn c0116_small_loop_and_c0118_sub_min_feature_graze_render_manifold() {
    for (label, y_offset) in [("C0116", 0.79), ("C0118", 0.79999999)] {
        let mesh = run_perpendicular_grazing_union(y_offset);
        let bad = non_manifold_edges(&mesh);
        assert!(
            bad.is_empty(),
            "{label}: {} non-manifold render edge(s) (count != 2): {:?}",
            bad.len(),
            &bad[..bad.len().min(6)]
        );
    }
}
