//! F4 / N91, N92 — sub-`f32`-ULP small-loop graded collar render pin (`C0118`).
//!
//! Two perpendicular cylinders (`R_A = 0.5` along `+Z`, `R_B = 0.3` along `+X`)
//! grazing `1e-8` deep at `(0, 0.5, 1.0)` (`C0118`'s authored coordinates) meet
//! in a tiny closed degree-4 intersection loop of diameter `~2.0e-4` (`~32`
//! samples), resolved on pass 0 via §4.2.2 Gauss-map admission + Fig. 10(c)
//! barycenter seeding + Bajaj closed-loop tracing (`stage4_small_loop.rs`).
//!
//! Near `(0, 0.5, 1.0)`, the two cylinder sheets sit `< 2e-8` apart in `y` —
//! strictly below 1 `f32` ULP (`5.96e-8` at `0.5_f32`). `insert_graded_hole_collars`
//! uses reflection-asymmetric ring phases `{0.25, 0.75}` anchored at the
//! canonical 3D hole vertex so the two twin faces' Steiner collar points are
//! staggered by `0.5 * ds_k >= 16 f32 ULPs` and never collide into
//! double-covered (`count > 2`) `f32` render edges.

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

fn over_used_edges(mesh: &RenderMesh) -> Vec<([f32; 3], [f32; 3], usize)> {
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
        .filter(|(_, c)| *c > 2)
        .map(|((a, b), c)| (pos(a), pos(b), c))
        .collect();
    bad.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    bad
}

#[test]
fn c0118_sub_ulp_small_loop_union_renders_without_double_covered_collar_edges() {
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
            [-1.5, 0.79999999, 1.0],
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
        .expect("C0118 sub-ULP grazing cylinder union must succeed on pass 0");
    let mesh = k.tessellate(&union, 0.001).expect("render tessellation");
    let bad = over_used_edges(&mesh);
    assert!(
        bad.is_empty(),
        "{} render edge(s) used by >2 triangles on C0118 collar: {:?}",
        bad.len(),
        &bad[..bad.len().min(6)]
    );
}
