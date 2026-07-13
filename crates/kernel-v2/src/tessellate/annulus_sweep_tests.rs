use super::annulus_sweep_triangles;

// Build the 3D positions for a ring of `n` samples at azimuths `az`,
// radius `r`, height `z`, in the XY plane (axis = +z).
fn ring_positions(az: &[f64], r: f64, z: f64) -> Vec<[f64; 3]> {
    az.iter().map(|&a| [r * a.cos(), r * a.sin(), z]).collect()
}

// Every emitted triangle must wind CCW around +z (its geometric normal's
// z-component is strictly positive) and have non-zero area.
fn assert_all_wind_up(tris: &[[u32; 3]], outer: &[[f64; 3]], inner: &[[f64; 3]]) {
    let pos = |idx: u32| -> [f64; 3] {
        let i = idx as usize;
        if i < outer.len() {
            outer[i]
        } else {
            inner[i - outer.len()]
        }
    };
    let mut reversed = 0usize;
    let mut degenerate = 0usize;
    for t in tris {
        let (o, a, b) = (pos(t[0]), pos(t[1]), pos(t[2]));
        let u = [a[0] - o[0], a[1] - o[1], a[2] - o[2]];
        let v = [b[0] - o[0], b[1] - o[1], b[2] - o[2]];
        let nz = u[0] * v[1] - u[1] * v[0];
        if nz.abs() < 1e-18 {
            degenerate += 1;
        } else if nz < 0.0 {
            reversed += 1;
        }
    }
    assert_eq!(
        reversed,
        0,
        "{reversed} of {} triangles wind DOWN",
        tris.len()
    );
    assert_eq!(degenerate, 0, "{degenerate} zero-area triangles");
}

#[test]
fn aligned_seams_wind_consistently() {
    let n = 16usize;
    let az: Vec<f64> = (0..n)
        .map(|k| std::f64::consts::TAU * (k as f64) / (n as f64))
        .collect();
    let outer = ring_positions(&az, 2.0, 0.0);
    let inner = ring_positions(&az, 1.0, 0.0);
    let tris = annulus_sweep_triangles(&az, &az, 0, n as u32);
    assert_eq!(tris.len(), 2 * n, "n outer + n inner edges → 2n triangles");
    assert_all_wind_up(&tris, &outer, &inner);
}

#[test]
fn offset_seams_wind_consistently() {
    // The gear's counterbore-floor case: the inner ring's seam is ~108°
    // ahead of the outer ring's. A column-k strip would twist and reverse
    // half the triangles; the azimuth sweep must keep them all up.
    let n = 32usize;
    let phase = 108.0_f64.to_radians();
    let outer_az: Vec<f64> = (0..n)
        .map(|k| std::f64::consts::TAU * (k as f64) / (n as f64))
        .collect();
    let inner_az: Vec<f64> = (0..n)
        .map(|k| {
            (phase + std::f64::consts::TAU * (k as f64) / (n as f64))
                .rem_euclid(std::f64::consts::TAU)
        })
        .collect();
    let outer = ring_positions(&outer_az, 5.909, 0.0);
    let inner = ring_positions(&inner_az, 4.903, 0.0);
    let tris = annulus_sweep_triangles(&outer_az, &inner_az, 0, n as u32);
    assert_eq!(tris.len(), 2 * n);
    assert_all_wind_up(&tris, &outer, &inner);
}

#[test]
fn offset_seams_unequal_counts_wind_consistently() {
    // Robustness: differing sample counts (general annulus) still all-up.
    let no = 24usize;
    let ni = 17usize;
    let phase = 1.234_f64;
    let outer_az: Vec<f64> = (0..no)
        .map(|k| std::f64::consts::TAU * (k as f64) / (no as f64))
        .collect();
    let inner_az: Vec<f64> = (0..ni)
        .map(|k| {
            (phase + std::f64::consts::TAU * (k as f64) / (ni as f64))
                .rem_euclid(std::f64::consts::TAU)
        })
        .collect();
    let outer = ring_positions(&outer_az, 3.0, 0.0);
    let inner = ring_positions(&inner_az, 1.5, 0.0);
    let tris = annulus_sweep_triangles(&outer_az, &inner_az, 0, no as u32);
    assert_eq!(tris.len(), no + ni);
    assert_all_wind_up(&tris, &outer, &inner);
}
