//! P0008 (2026-09-30, prospector seed 1 index 19, re-minimized): a curved
//! face's render triangulation must never carry an INTERIOR edge whose two
//! ends both lie on the same planar neighbor's boundary.
//!
//! Shape (the minimal 3-step recipe `convex4:boss convex5:rev gear10:sym`
//! at scale 772): a square boss on the x = 0 plane, a 300° pentagon revolve
//! about a z-parallel axis (its slender cone faces have half-angle 0.085
//! rad), and a 10-tooth module-111 gear extruded symmetrically on a plane
//! tilted 7.6° from the cone axis. The gear's flank planes cut the cone in
//! ELLIPSE arcs; in the cone's unrolled chart those arcs are locally
//! convex, so the boundary-only CDT clips ears over three consecutive arc
//! samples and, one level up, uses chords between two arc samples as
//! interior diagonals. Three points of a planar curve span a triangle IN
//! that curve's plane, so every such ear or chord renders inside the flank
//! face's own sheet: the cone's ear coincides with the plane's ear over the
//! same samples (six render edges with FOUR incident triangles — the
//! silent `wrong[watertight_mesh]` the prospector minimized), or the chord
//! survives as a diagonal of BOTH faces' meshes.
//!
//! The fix (`tessellate/developable.rs`, the P0008 refinement criterion)
//! tags every boundary node with the planar face across its half-edge(s)
//! and splits any interior chart edge whose ends share a tag at its
//! on-surface midpoint. RED without it (mutation-checked 2026-09-30: six
//! exact-bit edges with four uses), GREEN with it: no render edge of the
//! chained union is used by more than two triangles.

use std::collections::HashMap;

use kernel_v2::KernelV2Adapter;
use waffle_types::gear::{generate_gear_profile, GearParams};
use waffle_types::kernel::{Kernel, KernelSolidHandle, RenderMesh};
use waffle_types::ClosedProfile;

fn norm(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The prospector's plane frame (`gen3::plane_basis`, the engine's
/// reference rule): `u = ẑ × n̂` for a normal not along z, `v = n̂ × u`.
fn plane_x_axis(n: [f64; 3]) -> [f64; 3] {
    let n = norm(n);
    let r = if n[2].abs() < 0.99 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    norm(cross(r, n))
}

/// A regular n-gon of circumradius `r`, first vertex at angle `rot`, as an
/// authored polygon profile (`vertex_ids` 1..=n over `positions`).
fn regular_polygon(n: u32, r: f64, rot: f64) -> (ClosedProfile, HashMap<u32, (f64, f64)>) {
    let mut positions = HashMap::new();
    let mut ids = Vec::with_capacity(n as usize);
    for k in 0..n {
        let a = rot + std::f64::consts::TAU * f64::from(k) / f64::from(n);
        positions.insert(k + 1, (r * a.cos(), r * a.sin()));
        ids.push(k + 1);
    }
    let profile = ClosedProfile {
        entity_ids: ids.clone(),
        is_outer: true,
        vertex_ids: ids,
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    };
    (profile, positions)
}

/// Every exact-bit undirected render edge used by MORE than two triangles:
/// a double cover (two faces' sheets meeting along a shared interior chord,
/// or two coincident triangles). One-sided T-junction splits — legitimate in
/// kernel-v2's render — never raise a count above two, so this census is
/// strict without any grid or tolerance.
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

/// The chained union: square boss ∪ pentagon revolve ∪ symmetric gear boss,
/// exactly the P0008 minimal document's geometry (scale 772).
fn build_p0008(k: &mut KernelV2Adapter) -> KernelSolidHandle {
    // Step 0: square boss, r = 200, on the x = 0 plane, depth 80 along +x.
    let n0 = [1.0, 0.0, 0.0];
    let (sq, sq_pos) = regular_polygon(4, 200.0, 0.0);
    let f0 = k
        .make_faces_from_profiles(&[sq], [0.0, 0.0, 0.0], n0, plane_x_axis(n0), &sq_pos)
        .expect("square staging");
    let boss = k.extrude_face(f0[0], n0, 80.0).expect("square boss");

    // Step 1: pentagon r = 260 rot 0.085 on the plane x = 170 through
    // (170, 150, 200), revolved 300° about the z-parallel axis through
    // (170, −250, 200).
    let (pent, pent_pos) = regular_polygon(5, 260.0, 0.085);
    let f1 = k
        .make_faces_from_profiles(
            &[pent],
            [170.0, 150.0, 200.0],
            n0,
            plane_x_axis(n0),
            &pent_pos,
        )
        .expect("pentagon staging");
    let rev = k
        .revolve_face(f1[0], [170.0, -250.0, 200.0], [0.0, 0.0, 1.0], 300.0)
        .expect("pentagon revolve");
    let a = k.boolean_union(&boss, &rev).expect("boss ∪ revolve");

    // Step 2: 10-tooth module-111 gear on the plane through
    // (280, −772, 167) with normal (0.125, 0.0417, 0.991), extruded
    // symmetrically to a total depth of 723 (a 361.5 offset each way).
    let n2 = norm([0.125, 0.0417, 0.991]);
    let g = generate_gear_profile(&GearParams {
        tooth_count: 10,
        module: 111.0,
        pressure_angle_deg: 20.0,
        ..Default::default()
    });
    let half = 723.0 / 2.0;
    let origin2 = [
        280.0 - half * n2[0],
        -772.0 - half * n2[1],
        167.0 - half * n2[2],
    ];
    let f2 = k
        .make_faces_from_profiles(
            &[g.profiles[0].clone()],
            origin2,
            n2,
            plane_x_axis(n2),
            &g.positions,
        )
        .expect("gear staging");
    let gear = k.extrude_face(f2[0], n2, 723.0).expect("gear boss");
    k.boolean_union(&a, &gear).expect("(boss ∪ revolve) ∪ gear")
}

#[test]
fn chained_union_render_has_no_double_covered_edge() {
    let mut k = KernelV2Adapter::new();
    let out = build_p0008(&mut k);
    // The assay's render tolerance (the verdict the prospector judged at).
    let mesh = k.tessellate(&out, 0.001).expect("tessellate");
    let bad = over_used_edges(&mesh);
    assert!(
        bad.is_empty(),
        "{} render edge(s) used by more than two triangles (a double cover — \
         a curved face's interior chord lying in a planar neighbor's sheet): {:?}",
        bad.len(),
        &bad[..bad.len().min(6)]
    );
}
