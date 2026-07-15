//! N50 f32-render-twin weld (`weld_f32_render_twins`, spec
//! `specs/yang_n50_f32_render_twin_weld.md`, deviation N50).
//!
//! The producer-side guard that collapses two DISTINCT output vertices which are
//! bitwise-identical after rounding to f32 — the exact G1 render-collapse
//! criterion (kernel-v2 `f32_render_degenerate`, B2 clause). Unlike N47 this
//! reaches NON-relocated arrangement vertices (R0012/R0098) and keys on the
//! render resolution (f32 at the vertex's own magnitude), not the model band.

#[allow(unused_imports)]
use super::*;

fn attribution_for(n: usize) -> Vec<Option<TriangleAttribution>> {
    (0..n)
        .map(|i| {
            Some(TriangleAttribution {
                input: InputId::A,
                face: i as u32,
            })
        })
        .collect()
}

/// f32 bit-key: the render-buffer identity of a vertex (mirrors kernel-v2's
/// `f32_render_degenerate` B2 clause).
fn f32_key(p: Point3) -> [u32; 3] {
    let a = p.as_array();
    [
        (a[0] as f32).to_bits(),
        (a[1] as f32).to_bits(),
        (a[2] as f32).to_bits(),
    ]
}

fn distinct_f32_keys(mesh: &Mesh) -> bool {
    let mut live: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for tri in &mesh.tris {
        for &v in tri {
            live.insert(v);
        }
    }
    let mut keys: std::collections::BTreeMap<[u32; 3], u32> = std::collections::BTreeMap::new();
    for &v in &live {
        if let Some(&other) = keys.get(&f32_key(mesh.verts[v as usize])) {
            if other != v {
                return false;
            }
        }
        keys.insert(f32_key(mesh.verts[v as usize]), v);
    }
    true
}

/// GREEN behaviour: two coincident twins that round to the SAME f32 bits
/// (vert 3 = 0.1 + 1e-9, inside f32 ulp of vert 2's 0.1) weld — the pleat
/// annihilates and the bystander survives byte-identically. This is the
/// render-collapse pair that trips kernel-v2's G1 gate downstream.
#[test]
fn f32_coincident_twin_welds() {
    let mut verts = membrane_fixture_verts();
    verts[3] = Point3::new(0.5, 0.4, 0.1 + 1e-9); // f32-equal to vert 2's 0.1
    assert_eq!(
        f32_key(verts[2]),
        f32_key(verts[3]),
        "fixture precondition: the twin rounds to one f32 point"
    );
    let mut tris = pleat_tetra_tris();
    tris.extend(bystander_tetra_tris());
    let mut mesh = Mesh::new(verts, tris);
    let mut attribution = attribution_for(mesh.tris.len());
    let welded = weld_f32_render_twins(&mut mesh, &mut attribution);
    assert!(welded, "the f32-coincident twin must weld");
    assert_eq!(
        mesh.tris,
        bystander_tetra_tris(),
        "pleat must annihilate; bystander byte-identical"
    );
    assert_eq!(
        mesh.tris.len(),
        attribution.len(),
        "attribution stays lockstep"
    );
    assert!(
        distinct_f32_keys(&mesh),
        "no two live verts share an f32 render cell after the weld"
    );
}

/// GREEN at world magnitude: the ACTUAL R0012 twin (mag ~72, ~1.15e-6 apart,
/// bitwise-f32-equal at that magnitude) welds. Proves the criterion is measured
/// at the vertex's own magnitude (N49 fault 2) — a pair a model `TAU_MODEL` band
/// would treat as distinct (1.15e-6 > 1e-7) is caught because it collapses in the
/// f32 render buffer at magnitude 72.
#[test]
fn r0012_world_magnitude_twin_welds() {
    let u = Point3::new(43.02656630649507, -55.91360142719496, -71.82989113945716);
    let v = Point3::new(43.02656562361036, -55.9136013356693, -71.82989099479798);
    assert_eq!(f32_key(u), f32_key(v), "R0012 pair is bitwise-f32-equal");
    // gap is ABOVE the model coincidence tolerance — a model band would miss it.
    let g = ((u.as_array()[0] - v.as_array()[0]).powi(2)
        + (u.as_array()[1] - v.as_array()[1]).powi(2)
        + (u.as_array()[2] - v.as_array()[2]).powi(2))
    .sqrt();
    assert!(g > cad_primitives::TAU_MODEL, "gap {g:e} exceeds TAU_MODEL");
    // Two tetra: a needle {far, u, v, apex} (u,v are the render twin) + bystander.
    let verts = vec![
        u,                               // 0
        v,                               // 1
        Point3::new(40.0, -50.0, -70.0), // 2 = far base
        Point3::new(42.0, -52.0, -73.0), // 3 = apex
        Point3::new(0.0, 0.0, 0.0),      // 4 bystander
        Point3::new(1.0, 0.0, 0.0),      // 5
        Point3::new(0.5, 1.0, 0.0),      // 6
        Point3::new(0.5, 0.5, 1.0),      // 7
    ];
    // needle tetra on {0,1,2,3}: the (0,1) edge is the render twin.
    let mut tris = vec![[0, 1, 2], [1, 3, 2], [0, 2, 3], [0, 3, 1]];
    tris.extend(vec![[4, 5, 6], [4, 6, 7], [4, 7, 5], [5, 7, 6]]);
    let mut mesh = Mesh::new(verts, tris);
    let mut attribution = attribution_for(mesh.tris.len());
    let welded = weld_f32_render_twins(&mut mesh, &mut attribution);
    assert!(welded, "the world-magnitude render twin must weld");
    assert!(
        distinct_f32_keys(&mesh),
        "no render-coincident live pair survives"
    );
}

/// SAFETY (I2/I3, the N49 fault-1 guard): a pair that is f64-distinct AND
/// f32-DISTINCT is left untouched — even one INSIDE the model coincidence band.
/// The membrane fixture's verts 2,3 are 1e-7 apart (inside `TAU_MODEL·(1+scale)`,
/// so N47 welds them) but ~13 f32 ulps apart at unit scale, so they render
/// distinctly and must NOT be welded here. This is what makes the criterion
/// render-precision, not model-precision.
#[test]
fn f32_distinct_pair_is_not_welded() {
    let verts = membrane_fixture_verts(); // vert 3 = 0.1000001 (f32-distinct)
    assert_ne!(
        f32_key(verts[2]),
        f32_key(verts[3]),
        "fixture precondition: the pair renders as two distinct f32 points"
    );
    let mut tris = pleat_tetra_tris();
    tris.extend(bystander_tetra_tris());
    let before = tris.clone();
    let mut mesh = Mesh::new(verts, tris);
    let mut attribution = attribution_for(mesh.tris.len());
    let welded = weld_f32_render_twins(&mut mesh, &mut attribution);
    assert!(!welded, "an f32-distinct pair must NOT weld");
    assert_eq!(mesh.tris, before, "mesh byte-identical when nothing welds");
}

/// SAFETY near origin (N49 fault-1, the far-flung-model rim-sample guard): two
/// rim samples 1e-6 apart NEAR the origin (mag ~1e-3) round to DISTINCT f32
/// cells (f32 ulp there ≈ 1e-10 ≪ 1e-6), so they are NOT welded — even though a
/// global `TAU_MODEL·(1+GLOBAL_scale)` band from a far vertex would over-merge
/// them (the refuted N49 approach). Local-magnitude keying is the whole point.
#[test]
fn near_origin_pair_survives_when_far_vertex_present() {
    let verts = vec![
        Point3::new(0.001, 0.0, 0.0),        // 0 near-origin rim sample
        Point3::new(0.001001, 0.0, 0.0),     // 1 = 1e-6 from vert 0
        Point3::new(0.0, 0.001, 0.0),        // 2
        Point3::new(0.0, 0.0, 0.001),        // 3
        Point3::new(1686.0, -376.0, -226.0), // 4 far vertex (R0098-scale)
        Point3::new(1687.0, -377.0, -227.0), // 5
        Point3::new(1688.0, -373.0, -228.0), // 6
        Point3::new(1686.0, -375.0, -229.0), // 7
    ];
    assert_ne!(
        f32_key(verts[0]),
        f32_key(verts[1]),
        "near-origin 1e-6 pair renders as two distinct f32 points"
    );
    let mut tris = vec![[0, 1, 2], [1, 3, 2], [0, 2, 3], [0, 3, 1]];
    tris.extend(vec![[4, 5, 6], [4, 6, 7], [4, 7, 5], [5, 7, 6]]);
    let before = tris.clone();
    let mut mesh = Mesh::new(verts, tris);
    let mut attribution = attribution_for(mesh.tris.len());
    let welded = weld_f32_render_twins(&mut mesh, &mut attribution);
    assert!(!welded, "near-origin distinct rim samples must NOT weld");
    assert_eq!(mesh.tris, before, "mesh byte-identical: no over-merge");
}
