//! KV6d increment 2 — unit oracles for `tessellate_sphere_patch`
//! (spec `specs/kv6d_sphere_revolve.md`): the UV-CDT render consumer for
//! boolean-output sphere patches.
//!
//! 1. POLE CAP (wrapping boundary): an equator boundary loop on an outward
//!    face triangulates the NORTHERN hemisphere — watertight against the
//!    boundary (every interior undirected edge used exactly twice, boundary
//!    edges once), boundary vertices bit-exact, all vertices on the sphere,
//!    and the summed solid angle ≈ 2π (the correct hemisphere, refined).
//! 2. DISK (non-wrapping boundary): a small-cap boundary triangulates the
//!    cap; a boundary bounding the COMPLEMENT (wrong orientation) returns
//!    `None` instead of silently rendering the cap.

use std::f64::consts::PI;

use cad_primitives::Point3;
use yang_rs::tessellate_sphere_patch;

const R: f64 = 1.0;
const C: [f64; 3] = [5.0, 0.0, 0.0];

fn ring(lat: f64, n: usize, ccw: bool) -> Vec<Point3> {
    (0..n)
        .map(|k| {
            let u = 2.0 * PI * (k as f64) / (n as f64) * if ccw { 1.0 } else { -1.0 };
            Point3::new(
                C[0] + R * lat.cos() * u.cos(),
                C[1] + R * lat.cos() * u.sin(),
                C[2] + R * lat.sin(),
            )
        })
        .collect()
}

/// Signed volume of the cone fan from the sphere center over the triangles
/// — equals (solid angle)/3·r³, so a northern hemisphere patch sums to
/// 2π/3·r³ and quantifies both coverage AND chord sag.
fn center_fan_volume(verts: &[Point3], tris: &[[u32; 3]]) -> f64 {
    let p = |i: u32| {
        let v = verts[i as usize];
        [v.x() - C[0], v.y() - C[1], v.z() - C[2]]
    };
    let mut six_v = 0.0;
    for t in tris {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v.abs() / 6.0
}

fn assert_boundary_watertight(n_boundary: usize, verts: &[Point3], tris: &[[u32; 3]]) {
    use std::collections::HashMap;
    let q = |x: f64| (x / 1e-12).round() as i64;
    let key = |i: u32| {
        let v = verts[i as usize];
        (q(v.x()), q(v.y()), q(v.z()))
    };
    let mut count: HashMap<_, i64> = HashMap::new();
    for t in tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(a), key(b));
            assert_ne!(ka, kb, "degenerate triangle edge survived the weld");
            let e = if ka < kb { (ka, kb) } else { (kb, ka) };
            *count.entry(e).or_insert(0) += 1;
        }
    }
    let (mut once, mut twice, mut more) = (0usize, 0usize, 0usize);
    for &c in count.values() {
        match c {
            1 => once += 1,
            2 => twice += 1,
            _ => more += 1,
        }
    }
    assert_eq!(more, 0, "an undirected edge used more than twice");
    assert_eq!(
        once, n_boundary,
        "open (once-used) edges must be exactly the boundary chain \
         ({twice} interior edges)"
    );
    assert!(twice > 0, "no interior edges — the patch never refined");
}

#[test]
fn pole_cap_equator_boundary_covers_northern_hemisphere() {
    let n = 64;
    let boundary = ring(0.0, n, true); // equator, CCW eastward (region north)
    let seg = 2.0 * PI * R / 48.0;
    let (verts, tris) = tessellate_sphere_patch(
        Point3::new(C[0], C[1], C[2]),
        R,
        false,
        &boundary,
        &[],
        seg * seg,
    )
    .expect("pole-cap patch tessellates");

    // Boundary vertices pass through bit-exact (prefix of the pool).
    for (k, b) in boundary.iter().enumerate() {
        let v = verts[k];
        assert_eq!(
            (v.x(), v.y(), v.z()),
            (b.x(), b.y(), b.z()),
            "boundary vertex {k} not bit-exact"
        );
    }
    // Every referenced vertex is on the sphere.
    for t in &tris {
        for &i in t {
            let v = verts[i as usize];
            let d = ((v.x() - C[0]).powi(2) + v.y().powi(2) + (v.z() - C[2]).powi(2)).sqrt();
            assert!((d - R).abs() < 1e-9, "vertex off the sphere: {v:?}");
        }
    }
    // The region is the NORTHERN hemisphere (contains the pole), refined:
    // center-fan volume ⇒ solid angle ≈ 2π within the facet band.
    for t in &tris {
        for &i in t {
            assert!(
                verts[i as usize].z() >= C[2] - 1e-9,
                "triangle dips below the equator — wrong pole chosen"
            );
        }
    }
    let exact = 2.0 * PI / 3.0 * R * R * R;
    let vol = center_fan_volume(&verts, &tris);
    assert!(
        (vol - exact).abs() <= 0.05 * exact,
        "hemisphere coverage {vol} vs analytic {exact} (5% band)"
    );
    assert_boundary_watertight(n, &verts, &tris);
}

#[test]
fn disk_small_cap_boundary_triangulates_the_cap() {
    let n = 48;
    // Cap above latitude 60°: boundary CCW eastward bounds the region
    // NORTH of it… as a NON-wrapping loop it must be given region-inside:
    // in UV the cap-interior is ABOVE the ring — a wrapping loop. So use
    // the honest disk case instead: a small circle around the +x̂ point of
    // the sphere (u = 0, v = 0), non-wrapping in longitude.
    let cap_half_angle: f64 = 0.4;
    let boundary: Vec<Point3> = (0..n)
        .map(|k| {
            let t = 2.0 * PI * (k as f64) / (n as f64);
            // Circle of angular radius 0.4 around the (u=0, v=0) point,
            // traversed CCW as seen from outside (+x̂).
            let (st, ct) = t.sin_cos();
            let (sa, ca) = cap_half_angle.sin_cos();
            // Frame at (u=0,v=0): outward x̂, east ŷ, north ẑ.
            Point3::new(C[0] + R * ca, C[1] + R * sa * ct, C[2] + R * sa * st)
        })
        .collect();
    let seg = 2.0 * PI * R / 48.0;
    let (verts, tris) = tessellate_sphere_patch(
        Point3::new(C[0], C[1], C[2]),
        R,
        false,
        &boundary,
        &[],
        seg * seg,
    )
    .expect("disk patch tessellates");
    // Solid angle of the cap: 2π(1−cos a).
    let exact = 2.0 * PI * (1.0 - cap_half_angle.cos()) / 3.0 * R * R * R;
    let vol = center_fan_volume(&verts, &tris);
    assert!(
        (vol - exact).abs() <= 0.05 * exact,
        "cap coverage {vol} vs analytic {exact} (5% band)"
    );
    assert_boundary_watertight(n, &verts, &tris);

    // The SAME boundary traversed the other way bounds the complement
    // (both poles inside the region) — out of scope, must be None.
    let reversed_boundary: Vec<Point3> = boundary.iter().rev().copied().collect();
    assert!(
        tessellate_sphere_patch(
            Point3::new(C[0], C[1], C[2]),
            R,
            false,
            &reversed_boundary,
            &[],
            seg * seg,
        )
        .is_none(),
        "complement-bounding disk loop must be rejected, not silently rendered"
    );
}
