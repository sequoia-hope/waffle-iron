//! P0034 (2026-10-11, prospector seed 3 index 71, minimized): a cylinder
//! boss cut by a skew cylinder whose axes sit 1.336e-4 apart against a
//! radius difference of 1.389e-4 — the boss passes WHOLLY through the
//! cutter with its far generator 5.3e-6 inside the cutter's surface, so the
//! exact cylinder × cylinder curve is TWO loops that nearly touch at that
//! generator (a near-tangent pinch; the surfaces meet at 8.6° there).
//!
//! Two defects met at the pinch, one per crate:
//!
//! 1. yang-rs (spec `yang_172_case_iii_graze_guard.md` §3, the Case-V
//!    pinch row): the natural Stage-1 meshes (rim N 12 / 13, sagitta above
//!    the 5.3e-6 clearance) resolved ONE loop bridging the two exact ones,
//!    Stage 4 converged every vertex exactly, and the output carried pair
//!    edges whose endpoints lie on different components — kernel-v2's
//!    render sampler STOPped on the bridge ("surface-pair projection left
//!    the chord neighborhood"). The guard's skew-axis internal-graze term
//!    now derives N 37 for this pair before Stage 1 samples it.
//!
//! 2. kernel-v2 (spec `kv2_cdt_triangulation_core.md` §6f, generalized):
//!    with the curve right, the two faces — the boss lateral and the
//!    cutter's hole wall — both ear-clipped the same concave dip of their
//!    shared curve with the same two diagonals over the same twin-canonical
//!    samples, and a triangle with all three corners on the shared curve is
//!    the same flat triangle whichever surface claims it (the wedge between
//!    the surfaces is 1.7e-7 thick there, below the render sagitta): two
//!    coincident opposite triangles, two render edges with four incident
//!    triangles. The P0008 neighbor-chord criterion now tags boundary nodes
//!    with ANY twin face, not only a planar one, so each face lifts the
//!    shared diagonal onto its OWN surface.
//!
//! Pinned through the `Kernel` trait exactly as the prospector judged it:
//! the cut succeeds and no exact-bit render edge is used by more than two
//! triangles.

use std::collections::HashMap;

use kernel_v2::KernelV2Adapter;
use waffle_types::kernel::{Kernel, KernelSolidHandle, RenderMesh};
use waffle_types::{CircleProfile, ClosedProfile};

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

/// The prospector's plane frame (`gen3::plane_basis`): `u = ẑ × n̂` for a
/// normal not along z, `v = n̂ × u`.
fn plane_x_axis(n: [f64; 3]) -> [f64; 3] {
    let n = norm(n);
    let r = if n[2].abs() < 0.99 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    norm(cross(r, n))
}

/// A standalone circle profile centred on the sketch origin.
fn circle(radius: f64) -> (ClosedProfile, HashMap<u32, (f64, f64)>) {
    let mut positions = HashMap::new();
    positions.insert(2, (0.0, 0.0));
    let profile = ClosedProfile {
        entity_ids: vec![1],
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

/// Every exact-bit undirected render edge used by MORE than two triangles —
/// a double cover. One-sided T-junction splits never raise a count above
/// two, so this census is strict without any grid or tolerance.
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

/// P0034's two pinching cylinders, at the document's own coordinates.
fn build_p0034(k: &mut KernelV2Adapter) -> Result<KernelSolidHandle, String> {
    // The circle boss: r 2.88e-4 on the +y plane through
    // (7.12e-5, −5.03e-4, 4.92e-5), extruded 6.92e-4 along +y.
    let n_boss = [0.0, 1.0, 0.0];
    let o_boss = [7.12e-5, -0.000503, 4.92e-5];
    let (cb, pb) = circle(0.000288);
    let fb = k
        .make_faces_from_profiles(&[cb], o_boss, n_boss, plane_x_axis(n_boss), &pb)
        .map_err(|e| format!("boss staging: {e:?}"))?;
    let boss = k
        .extrude_face(fb[0], n_boss, 0.000692)
        .map_err(|e| format!("boss extrude: {e:?}"))?;
    // The thru cut: r 4.2689595303603865e-4 on the plane through
    // (−6.660512631808052e-5, −4.3237484391500815e-4, 1.1367060098271782e-4)
    // with normal (0.058238265717590144, 0.28428618102029196,
    // −0.9569690024693094), extruded 1.0 along that normal (the cap lies
    // inside the boss, as in the document).
    let n_cut = [
        0.058_238_265_717_590_144,
        0.284_286_181_020_291_96,
        -0.956_969_002_469_309_4,
    ];
    let o_cut = [
        -6.660_512_631_808_052e-5,
        -0.000_432_374_843_915_008_15,
        0.000_113_670_600_982_717_82,
    ];
    let (cc, pc) = circle(0.000_426_895_953_036_038_65);
    let fc = k
        .make_faces_from_profiles(&[cc], o_cut, n_cut, plane_x_axis(n_cut), &pc)
        .map_err(|e| format!("cutter staging: {e:?}"))?;
    let cutter = k
        .extrude_face(fc[0], n_cut, 1.0)
        .map_err(|e| format!("cutter extrude: {e:?}"))?;
    k.boolean_subtract(&boss, &cutter)
        .map_err(|e| format!("boss − cutter: {e:?}"))
}

#[test]
fn pinched_cut_renders_without_a_double_covered_edge() {
    let mut k = KernelV2Adapter::new();
    let out = build_p0034(&mut k).expect("the pinched cut must complete");
    // The assay's render tolerance (the verdict the prospector judged at).
    let mesh = k.tessellate(&out, 0.001).expect("tessellate");
    let bad = over_used_edges(&mesh);
    assert!(
        bad.is_empty(),
        "{} render edge(s) used by more than two triangles (two faces' coincident \
         ears over a shared pinching curve): {:?}",
        bad.len(),
        &bad[..bad.len().min(6)]
    );
}
