#[allow(unused_imports)]
use super::*;

// ====================================================================
// #178 — sub-resolution coplanar-gap STOP (C0111/C0113-F1).
// Spec `specs/yang_178_subres_coplanar_gap_stop.md`.
//
// Two DISTINCT parallel cross-solid planes separated by a NONZERO gap
// below the Stage-0 detection band are a sub-MIN_FEATURE_SIZE feature
// the §4.5.5 overlay would silently dissolve (χ 0→2, the measured
// C0111/C0113 wall dissolve). Out-of-contract input rejects LOUDLY:
// `YangError::SubResolutionCoplanarGap`. Pairs at or below the
// coincidence-authoring noise line `band/100` keep the overlay path:
// gap == 0 (bit-exact flush/stacked, the mainstream class), chained
// femto twins (corpus max 2.7e-12), and real producer residuals (the
// mm-scale bearing recess, 2.235e-10 — the fixture that REFUTED a
// tighter `TAU_WORK·(1+scale)` line).
// ====================================================================

/// Axis-aligned box BRep spanning `lo..hi` with outward face normals —
/// `boolean_functional::cube_brep` generalized to arbitrary extents.
fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
    let verts = vec![
        BRepVertex {
            point: p(x0, y0, z0),
        },
        BRepVertex {
            point: p(x1, y0, z0),
        },
        BRepVertex {
            point: p(x1, y1, z0),
        },
        BRepVertex {
            point: p(x0, y1, z0),
        },
        BRepVertex {
            point: p(x0, y0, z1),
        },
        BRepVertex {
            point: p(x1, y0, z1),
        },
        BRepVertex {
            point: p(x1, y1, z1),
        },
        BRepVertex {
            point: p(x0, y1, z1),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3], // bottom (z0)
        [4, 7, 6, 5], // top (z1)
        [0, 4, 5, 1], // front (y0)
        [1, 5, 6, 2], // right (x1)
        [2, 6, 7, 3], // back (y1)
        [3, 7, 4, 0], // left (x0)
    ];
    let mut edges = Vec::new();
    let mut loops = Vec::new();
    for vs in &face_verts {
        let base = edges.len() as u32;
        for i in 0..4 {
            edges.push(BRepEdge {
                start: vs[i],
                end: vs[(i + 1) % 4],
                curve: Curve::LineSegment,
            });
        }
        loops.push(vec![base, base + 1, base + 2, base + 3]);
    }
    let normals = [
        Vector3::new(0.0, 0.0, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(-1.0, 0.0, 0.0),
    ];
    // Plane convention n·x + d = 0 with n outward (see cube_brep).
    let offs = [z0, -z1, y0, -x1, -y1, x0];
    let faces: Vec<BRepFace> = (0..6)
        .map(|i| BRepFace {
            surface: Surface::Plane {
                normal: normals[i],
                d: offs[i],
            },
            outer_loop: loops[i].clone(),
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(verts, edges, faces).unwrap()
}

/// Subtract a through-notch tool from the unit box, the tool's +x wall
/// face at `x = wall_x` — the C0113 fixture shape at unit scale (the
/// wall between `wall_x` and the box face x=1 is the feature at stake).
fn notch_subtract(wall_x: f64) -> Result<BRep, YangError> {
    let a = box_brep([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = box_brep([0.3, 0.2, -0.5], [wall_x, 0.8, 1.5]);
    let nb = crate::native_backend().expect("native backend");
    boolean(&a, &b, BoolOp::Subtract, &nb)
}

/// C0113 mirror: wall gap 1e-7 (= TAU_MODEL, the R0091 hazard rung) —
/// inside the detection band, far above rounding noise → typed STOP,
/// never the silent dissolve.
#[test]
pub(crate) fn n178_tau_model_gap_wall_stops_loudly() {
    let r = notch_subtract(1.0 - 1e-7);
    match r {
        Err(YangError::SubResolutionCoplanarGap { gap, band, .. }) => {
            assert!(
                gap > 9e-8 && gap <= band,
                "gap {gap:.3e} must be the ~1e-7 wall within band {band:.3e}"
            );
        }
        other => panic!(
            "sub-resolution wall must STOP with SubResolutionCoplanarGap, got {:?}",
            other.map(|_| "Ok(BRep) — the silent C0113 dissolve")
        ),
    }
}

/// C0111 mirror at mm scale: wall gap 1e-8, below the absolute
/// TAU_MODEL floor — same STOP (the criterion is the pair's own band,
/// not an absolute rung).
#[test]
pub(crate) fn n178_mm_scale_sub_floor_gap_stops_loudly() {
    let s = 1e-3;
    let a = box_brep([0.0, 0.0, 0.0], [s, s, s]);
    let b = box_brep([0.3 * s, 0.2 * s, -0.5 * s], [s - 1e-8, 0.8 * s, 1.5 * s]);
    let nb = crate::native_backend().expect("native backend");
    match boolean(&a, &b, BoolOp::Subtract, &nb) {
        Err(YangError::SubResolutionCoplanarGap { gap, .. }) => {
            assert!(
                (gap - 1e-8).abs() < 1e-9,
                "gap {gap:.3e} must be the 1e-8 wall"
            );
        }
        other => panic!(
            "mm-scale sub-floor wall must STOP, got {:?}",
            other.map(|_| "Ok(BRep) — the silent C0111 dissolve")
        ),
    }
}

/// Bit-exact flush tool face (gap exactly 0): the mainstream §4.5.5
/// coplanar class — the overlay proceeds and the subtract succeeds.
/// Guards I1: the STOP must not eat the exact-coincidence path.
#[test]
pub(crate) fn n178_bit_exact_flush_pair_still_overlays() {
    let out = notch_subtract(1.0).expect("flush notch subtract must stay green");
    assert!(
        !out.faces().is_empty(),
        "flush subtract must produce a real solid"
    );
}

/// Producer-residual class (gap 2e-10, the real bearing-recess magnitude,
/// just under the band/100 = 1e-9 line): intended-coincident geometry from
/// the app chain — the overlay proceeds. Guards the weld side of the line
/// at its measured population.
#[test]
pub(crate) fn n178_producer_residual_gap_still_overlays() {
    let out = notch_subtract(1.0 - 2e-10);
    match out {
        Ok(_) => {}
        Err(YangError::SubResolutionCoplanarGap { gap, band, .. }) => panic!(
            "producer residual (gap {gap:.3e}, band {band:.3e}) must NOT trip \
             the sub-resolution STOP — the real bearing-recess class welds"
        ),
        Err(e) => panic!("producer-residual subtract unexpectedly failed: {e}"),
    }
}

/// Femto rounding twin (gap 1 ulp of 1.0 ≈ 1.1e-16 ≪ band/100):
/// the chained-output rounding class — the overlay proceeds. Guards I1's
/// noise side: the STOP must not reject legitimate rounding twins.
#[test]
pub(crate) fn n178_femto_rounding_twin_still_overlays() {
    // Largest f64 strictly below 1.0 — an offset gap of one ulp.
    let wall_x = f64::from_bits(1.0f64.to_bits() - 1);
    let out = notch_subtract(wall_x);
    match out {
        Ok(_) => {}
        Err(YangError::SubResolutionCoplanarGap { gap, band, .. }) => panic!(
            "femto twin (gap {gap:.3e}, band {band:.3e}) must NOT trip the \
             sub-resolution STOP — it is the legitimate rounding class"
        ),
        // Any OTHER loud error would be a pre-existing overlay limitation,
        // not this spec's concern — but none is expected for a plain
        // femto-flush box pair; keep the assert strict until measured.
        Err(e) => panic!("femto-flush subtract unexpectedly failed: {e}"),
    }
}
