//! P0021 anchor + conversion pin — deviation **N75** (spec
//! `specs/yang_p0021_case_iii_finite_extent_depth.md`).
//!
//! The two bosses P0021's minimized recipe unions are a pair of cylinders
//! whose INFINITE surfaces interpenetrate deeply (`r_a + r_b − d_lines =
//! 1.480362e-3`) but whose FINITE solids overlap in a razor lens only
//! `6.162267e-6` deep, pinched between their two nearly-coincident base
//! cap planes. The Case-III graze guard
//! (`specs/yang_172_case_iii_graze_guard.md`) measures its penetration
//! depth at the common perpendicular of the two axis LINES, whose foot
//! sits at `s = −3.302106e-3` on a cylinder whose own span is
//! `[0, 1.68e-3]` — 1.97 lengths off the far end. It therefore derives
//! `N = 5`, the natural-N gate (10 / 12) absorbs the demand, Stage 1
//! samples at natural density, and the Stage-2 arrangement finds no
//! intersection at all: the guard's own phase filter prints
//! `meshes_touch=false`, which is Yang Fig. 8 Case III by definition
//! (`refs/text/yang2025_hybrid_boolean.txt:436-447`).
//!
//! The N75 ladder witnesses the overlap on the two faces' OWN axial
//! extents and then refines until the exact tri-tri predicate flips. It is
//! GATED OFF by default (`YANG_172_EXTENT=1|on`) because the always-on
//! flip is a corpus-cost decision owing the full release categorized assay
//! (P10) — so this file pins BOTH sides: the honest wall the default path
//! still hits, and the fused body the ladder recovers.
//!
//! The gate is process-global; both tests serialise on one mutex and
//! set/clear the env var themselves (the `s434_typed_rim_seam_mint.rs`
//! pattern).

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, BrepArena, Curve, SolidId, Surface};
use std::sync::{Mutex, MutexGuard};

static GATE: Mutex<()> = Mutex::new(());

fn ladder(on: bool) -> MutexGuard<'static, ()> {
    let g = GATE.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("YANG_172_EXTENT", if on { "1" } else { "0" });
    g
}

/// The two measured P0021 boss poses, authored exactly as the recipe does:
/// a circle sketched on the plane through `axis_point` normal to
/// `axis_dir`, extruded `len` along that normal.
fn boss(a: &mut BrepArena, axis_point: [f64; 3], axis_dir: [f64; 3], r: f64, len: f64) -> SolidId {
    let unit = |v: [f64; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let cross = |x: [f64; 3], y: [f64; 3]| {
        [
            x[1] * y[2] - x[2] * y[1],
            x[2] * y[0] - x[0] * y[2],
            x[0] * y[1] - x[1] * y[0],
        ]
    };
    let n = unit(axis_dir);
    // Deterministic in-plane frame.
    let seed = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = unit(cross(n, seed));
    let v = cross(n, u);
    let p = kernel_v2::Profile::circle(
        Point3::new(axis_point[0], axis_point[1], axis_point[2]),
        Vector3::new(u[0], u[1], u[2]),
        Vector3::new(v[0], v[1], v[2]),
        Point2::new(0.0, 0.0),
        r,
    )
    .expect("circle profile");
    extrude(a, &p, Vector3::new(n[0], n[1], n[2]), len)
        .expect("extrude boss")
        .solid
}

const A_P: [f64; 3] = [0.000477, 0.000265, 0.00102];
const A_D: [f64; 3] = [
    0.440_078_120_800_278_5,
    0.539_095_697_980_341_2,
    -0.718_127_478_942_272_6,
];
const B_P: [f64; 3] = [0.0015, -0.0013, 0.0017];
const B_D: [f64; 3] = [
    -0.520_223_483_989_871_5,
    -0.079_033_952_375_384_33,
    0.850_365_310_368_059_2,
];

fn union_the_pair(arena: &mut BrepArena) -> Result<SolidId, kernel_v2::KernelV2Error> {
    let sa = boss(arena, A_P, A_D, 0.00071, 0.00168);
    let sb = boss(arena, B_P, B_D, 0.002, 0.0028);
    boolean_op(arena, sa, sb, BoolOp::Union)
}

/// ANCHOR (gate off, today's default path). The BARE PAIR reproduces
/// P0021 with no prism at all — a 2-operand reduction of the 3-op corpus
/// case: the natural meshes miss the lens, both laterals come through
/// un-trimmed against each other, and kernel-v2's render-resolution selfx
/// gate catches the penetration loudly. That STOP is the honest default
/// answer and is pinned here so the conversion's flip is visible; the
/// alternative — a silently unfused pair — would be a P9 silent-wrong.
#[test]
fn p0021_pair_default_path_stops_loudly_on_the_untrimmed_laterals() {
    let _g = ladder(false);
    let mut arena = BrepArena::new();
    match union_the_pair(&mut arena) {
        Err(kernel_v2::KernelV2Error::SelfIntersectingBooleanOutput { penetrations, .. }) => {
            assert!(
                penetrations > 0,
                "a STOP must name at least one penetration"
            );
        }
        Err(other) => panic!("expected the selfx STOP, got {other:?}"),
        Ok(solid) => panic!(
            "the default path must not emit this pair silently; it derived \
             {} SurfacePair half-edges",
            surface_pair_half_edges(&arena, solid)
        ),
    }
}

/// CONVERSION (gate on): the ladder refines until the meshes meet, so the
/// union derives the cylinder×cylinder intersection and the two laterals
/// are actually trimmed against each other. The surviving `SurfacePair`
/// curve is the P0007-shaped assertion: the restored curve TYPE must
/// survive into the output B-Rep, not just the volume be plausible.
#[test]
fn p0021_pair_with_the_n75_ladder_derives_the_cyl_cyl_curve() {
    let _g = ladder(true);
    let mut arena = BrepArena::new();
    let solid = union_the_pair(&mut arena).expect("the N75 ladder must fuse the pair");
    let n = surface_pair_half_edges(&arena, solid);
    assert!(
        n > 0,
        "the ladder must derive the cyl x cyl SurfacePair curve; got {n} half-edges"
    );
}

/// Half-edges of `solid` whose curve is a `SurfacePair` — the degree-4
/// cylinder×cylinder intersection vocabulary (`specs/m5_surface_pair_curve.md`).
fn surface_pair_half_edges(arena: &BrepArena, solid: SolidId) -> usize {
    let mut n = 0;
    let Ok(s) = arena.solid(solid) else { return 0 };
    for &shell in &s.shells {
        let Ok(sh) = arena.shell(shell) else { continue };
        for &face in &sh.faces {
            let Ok(f) = arena.face(face) else { continue };
            // Only count curves bounding a CURVED face — a plane x plane
            // seam is never a SurfacePair.
            if matches!(f.surface, Some(Surface::Plane(_))) {
                continue;
            }
            for lp in std::iter::once(f.outer_loop).chain(f.inner_loops.iter().copied()) {
                for h in arena.loop_half_edges(lp).unwrap_or_default() {
                    if let Ok(he) = arena.half_edge(h) {
                        if matches!(he.curve, Curve::SurfacePair { .. }) {
                            n += 1;
                        }
                    }
                }
            }
        }
    }
    n
}
