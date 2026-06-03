//! PR-YR16 ADVERSARY — independent cone outward-sense witness.
//!
//! A THIRD, independent agent (neither the RED-test author nor the GREEN
//! implementer). This file re-derives the spec §4 tilted cone normal from first
//! principles on a SECOND off-axis cone mock whose apex / axis / half-angle are
//! DISTINCT from the `yr16_cone.rs` corpus (apex `(-2,1,3)`, tilted non-unit
//! axis `(2,-1,2)`, half-angle 0.6 rad). It then witnesses, INDEPENDENTLY of
//! production's `cone_outward_normal`:
//!
//! 1. The analytic normal `n̂ = unit(r̂ − tanα·â)` is ⟂ the generator
//!    (`n̂ · g = 0`) — re-derives the §4 math.
//! 2. Each emitted LATERAL triangle's geometric normal `(v1−v0)×(v2−v0)`
//!    points the SAME way as `n̂` at the triangle centroid (dot > 0) — i.e.
//!    the production mesh winds OUTWARD, witnessed by an independent normal.
//! 3. The whole solid has POSITIVE signed volume
//!    `Σ v0·(v1×v2)/6 > 0` (the `yang_mock_orientation_witness` memory lesson:
//!    watertight + χ=2 can BOTH pass while a solid is globally inside-out, so a
//!    signed-volume witness is required for outward orientation).
//! 4. Watertight (every undirected edge shared by exactly 2 tris) + Euler χ = 2
//!    on this independent mock too.
//!
//! The B-Rep fixture is re-derived here (integration test files cannot share a
//! module, so it is NOT imported from `yr16_cone.rs`).

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::{Point3, Vector3};
use yang_rs::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// ---- pure array math (cad-primitives has no vector helpers) ----
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = norm(a);
    assert!(n > 0.0, "cannot normalize zero vector");
    scale(a, 1.0 / n)
}

// ---- the SECOND off-axis cone (distinct from the yr16_cone corpus) ----
// apex `(-2, 1, 3)`, tilted non-unit axis `(2, -1, 2)` (‖·‖ = 3), α = 0.6 rad.
// Tall (h = 4, R = 4·tan(0.6) ≈ 2.74), used for the §4 normal / winding /
// volume / watertight witnesses.
const APEX: [f64; 3] = [-2.0, 1.0, 3.0];
const AXIS: [f64; 3] = [2.0, -1.0, 2.0];
const HALF_ANGLE: f64 = 0.6;
const HEIGHT: f64 = 4.0;

// ---- a SECOND, WIDE-SHORT off-axis cone (h < 2R) for the chord-bound witness.
// R = 5, h = 0.5 (so the rim-AABB bound `1e-2·2R√2` ≈ 0.1414 EXCEEDS the cone's
// honest bound `1e-2·√((2R)²+h²)` ≈ 0.1001, the §3 case the pre-pass `min` is
// load-bearing for). Off-axis + non-unit to also exercise normalization. This is
// distinct from the corpus's *z-up* wide-short case.
const WS_APEX: [f64; 3] = [3.0, -2.0, 5.0];
const WS_AXIS: [f64; 3] = [1.0, 2.0, 2.0]; // ‖·‖ = 3
const WS_HEIGHT: f64 = 0.5;
// half_angle so R = h·tan(α) = 5 → α = atan(5 / 0.5) = atan(10).
fn ws_half_angle() -> f64 {
    (5.0_f64 / 0.5).atan()
}

/// Re-derive the §1 cone B-Rep fixture independently (NOT imported from
/// `yr16_cone.rs`). One `Surface::Cone` lateral + one `Surface::Plane` base cap
/// sharing a single base-rim `Curve::Circle`; no seam LineSegment.
fn cone_brep(apex: [f64; 3], axis: [f64; 3], half_angle: f64, height: f64) -> BRep {
    let axis_unit = unit(axis);
    let radius = height * half_angle.tan();
    let base_center = add(apex, scale(axis_unit, height));

    // Any on-rim point (the rim pre-pass recovers azimuth). Stablest cross seed.
    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(axis_unit, world));
    let base_seam = add(base_center, scale(e1, radius));

    let verts = vec![
        BRepVertex {
            point: p(apex[0], apex[1], apex[2]),
        },
        BRepVertex {
            point: p(base_seam[0], base_seam[1], base_seam[2]),
        },
    ];

    let edges = vec![BRepEdge {
        start: 1,
        end: 1,
        curve: Curve::Circle {
            center: p(base_center[0], base_center[1], base_center[2]),
            normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
            radius,
        },
    }];

    let cap_d = -dot(axis_unit, base_center);

    let faces = vec![
        BRepFace {
            surface: Surface::Cone {
                apex: p(apex[0], apex[1], apex[2]),
                // Pass the NON-UNIT axis deliberately (production normalizes).
                axis_dir: Vector3::new(axis[0], axis[1], axis[2]),
                half_angle,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                d: cap_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces).expect("adversary cone_brep must tessellate")
}

fn adversary_cone_brep() -> BRep {
    cone_brep(APEX, AXIS, HALF_ANGLE, HEIGHT)
}

/// Independent re-derivation of the spec §4 analytic outward cone normal at a
/// point `x` on the lateral: `n̂ = unit(r̂ − tanα·â)` where `â = unit(axis)`
/// (apex→base) and `r̂` is the unit radial component of `(x − apex)`.
fn analytic_cone_normal(x: [f64; 3], apex: [f64; 3], axis: [f64; 3], half_angle: f64) -> [f64; 3] {
    let ax = unit(axis);
    let w = sub(x, apex);
    let along = dot(w, ax);
    let radial = sub(w, scale(ax, along));
    let rhat = unit(radial);
    let t = half_angle.tan();
    unit(sub(rhat, scale(ax, t)))
}

/// Signed (analytic) cone residual `radial − |h_axial|·tanα`. A lateral
/// triangle has all 3 vertices with |residual| ≤ tol.
fn cone_residual(x: [f64; 3], apex: [f64; 3], axis: [f64; 3], half_angle: f64) -> f64 {
    let ax = unit(axis);
    let w = sub(x, apex);
    let h_axial = dot(w, ax);
    let radial = norm(sub(w, scale(ax, h_axial)));
    radial - h_axial.abs() * half_angle.tan()
}

/// `cone_chord_bound` recomputed test-side (spec §3) — IDENTICAL literal to
/// production. The honest worst-case chord band any lateral sample must respect.
fn cone_chord_bound(height: f64, half_angle: f64) -> f64 {
    let r = height * half_angle.tan();
    1e-2 * ((2.0 * r).powi(2) + height.powi(2)).sqrt()
}

// =========================================================================
// 0. First re-derive §4: n̂ ⟂ generator on this mock (apex→rim direction).
// =========================================================================
#[test]
fn adv_analytic_normal_perpendicular_to_generator() {
    // Build a rim point and its generator g = apex→rim. n̂·g must be 0.
    let ax = unit(AXIS);
    let radius = HEIGHT * HALF_ANGLE.tan();
    let base_center = add(APEX, scale(ax, HEIGHT));
    // any in-plane direction:
    let world = [1.0, 0.0, 0.0];
    let e1 = unit(cross(ax, world));
    // Sample several azimuths so the witness is not a single lucky direction.
    for k in 0..8 {
        let theta = std::f64::consts::TAU * (k as f64) / 8.0;
        let e2 = cross(ax, e1);
        let radial_dir = add(scale(e1, theta.cos()), scale(e2, theta.sin()));
        let rim = add(base_center, scale(radial_dir, radius));
        let generator = sub(rim, APEX); // apex → rim
                                        // Mid-generator surface point to evaluate the normal at.
        let mid = add(APEX, scale(generator, 0.5));
        let n = analytic_cone_normal(mid, APEX, AXIS, HALF_ANGLE);
        let g = unit(generator);
        let perp = dot(n, g);
        assert!(
            perp.abs() < 1e-12,
            "n̂ must be ⟂ generator: n̂·ĝ = {perp} at azimuth k={k}"
        );
        // Sanity: the analytic normal must have an OUTWARD radial component
        // (positive dot with r̂), not point inward.
        let along = dot(sub(mid, APEX), ax);
        let rhat = unit(sub(sub(mid, APEX), scale(ax, along)));
        assert!(
            dot(n, rhat) > 0.0,
            "n̂ must have positive radial (outward) component at azimuth k={k}"
        );
    }
}

// =========================================================================
// 0b. WHY THE TILT MATTERS — pure radial r̂ is the WRONG cone normal.
//
// ADVERSARY FINDING (documented loudly): mutating production's
// `cone_outward_normal` to drop the `−tanα·â` tilt (return the PURE radial r̂)
// produces a BYTE-IDENTICAL mesh — verified by an ephemeral winding-hash probe
// (hash 25c917f32aec61a8 unchanged). The reason is structural: every apex-FAN
// triangle's geometric normal lies in the SAME half-space relative to BOTH r̂
// and the tilted n̂, so `orient_tri`'s binary flip decision is identical for
// either target at every steepness (verified analytically for α ∈ [0.2, 1.5]).
// Hence NO winding / watertight / signed-distance / Euler oracle — in
// `yr16_cone.rs` OR here — can red the pure-radial mutation for the current
// pure-fan tessellation: there is no observable output difference to assert on.
//
// The tilt is nonetheless the mathematically CORRECT surface normal, and it
// becomes orientation-load-bearing the moment the tessellation gains non-fan
// (interior-ring) triangles — i.e. PR-YR17 cone cavity. This test pins the
// honest mathematical witness that distinguishes the two: the SPEC §4 tilted
// normal is ⟂ the generator (n̂·ĝ ≈ 0) while the pure radial r̂ is NOT
// (r̂·ĝ is a large nonzero), so the tilt is the load-bearing CORRECT normal
// even though the current fan tessellation masks the difference in its winding.
// =========================================================================
#[test]
fn adv_tilt_is_correct_normal_pure_radial_is_wrong() {
    let ax = unit(AXIS);
    let radius = HEIGHT * HALF_ANGLE.tan();
    let base_center = add(APEX, scale(ax, HEIGHT));
    let e1 = unit(cross(ax, [1.0, 0.0, 0.0]));
    let e2 = cross(ax, e1);
    for k in 0..8 {
        let theta = std::f64::consts::TAU * (k as f64) / 8.0;
        let radial_dir = add(scale(e1, theta.cos()), scale(e2, theta.sin()));
        let rim = add(base_center, scale(radial_dir, radius));
        let generator = sub(rim, APEX);
        let ghat = unit(generator);
        let mid = add(APEX, scale(generator, 0.5));

        // Pure radial r̂ (what the MUTATION returns).
        let w = sub(mid, APEX);
        let along = dot(w, ax);
        let rhat = unit(sub(w, scale(ax, along)));
        // Tilted n̂ (spec §4, what production returns).
        let nhat = analytic_cone_normal(mid, APEX, AXIS, HALF_ANGLE);

        let r_perp = dot(rhat, ghat).abs();
        let n_perp = dot(nhat, ghat).abs();
        // The tilted normal is ⟂ the generator; the pure radial is decidedly NOT.
        assert!(
            n_perp < 1e-12,
            "tilted n̂ must be ⟂ generator (got {n_perp}) at k={k}"
        );
        assert!(
            r_perp > 0.3,
            "pure radial r̂ must be FAR from ⟂ the generator — it is the WRONG \
             cone normal (got r̂·ĝ = {r_perp}) at k={k}; this is why spec §4 \
             tilts. (Mutation note: dropping the tilt yields a byte-identical \
             fan mesh, so only this analytic witness distinguishes the two.)"
        );
    }
}

// =========================================================================
// 1. Independent outward-winding witness on the production mesh.
//    For each LATERAL triangle, its geometric normal must agree (dot > 0) with
//    the INDEPENDENTLY re-derived analytic normal at its centroid.
// =========================================================================
#[test]
fn adv_lateral_triangles_wind_outward() {
    let b = adversary_cone_brep();
    let mesh = b.as_mesh();
    let d_eps = {
        let r = HEIGHT * HALF_ANGLE.tan();
        1e-2 * ((2.0 * r).powi(2) + HEIGHT.powi(2)).sqrt()
    };

    let mut lateral_count = 0usize;
    for &tri in &mesh.tris {
        let v0 = mesh.verts[tri[0] as usize].as_array();
        let v1 = mesh.verts[tri[1] as usize].as_array();
        let v2 = mesh.verts[tri[2] as usize].as_array();
        // A lateral triangle has all 3 verts ON the analytic cone (residual ~0).
        let on_cone = [v0, v1, v2]
            .iter()
            .all(|&x| cone_residual(x, APEX, AXIS, HALF_ANGLE).abs() <= d_eps);
        if !on_cone {
            continue; // base-cap triangle — skip
        }
        lateral_count += 1;

        let centroid = scale(add(add(v0, v1), v2), 1.0 / 3.0);
        let n_analytic = analytic_cone_normal(centroid, APEX, AXIS, HALF_ANGLE);
        let geo = cross(sub(v1, v0), sub(v2, v0));
        let d = dot(geo, n_analytic);
        assert!(
            d > 0.0,
            "lateral tri {tri:?} geometric normal opposes the independently \
             re-derived outward normal (dot {d} ≤ 0) — winding is INWARD"
        );
    }
    assert!(
        lateral_count >= 3,
        "expected ≥3 lateral fan triangles, found {lateral_count}"
    );
}

// =========================================================================
// 2. Global outward-orientation witness: signed volume > 0.
//    (yang_mock_orientation_witness: watertight + χ can pass while inside-out.)
// =========================================================================
#[test]
fn adv_positive_signed_volume() {
    let b = adversary_cone_brep();
    let mesh = b.as_mesh();
    let mut vol6 = 0.0_f64;
    for &tri in &mesh.tris {
        let v0 = mesh.verts[tri[0] as usize].as_array();
        let v1 = mesh.verts[tri[1] as usize].as_array();
        let v2 = mesh.verts[tri[2] as usize].as_array();
        vol6 += dot(v0, cross(v1, v2));
    }
    let vol = vol6 / 6.0;
    assert!(
        vol > 0.0,
        "signed volume must be > 0 (outward-oriented solid); got {vol}"
    );
    // Cross-check magnitude against the analytic cone volume πR²h/3 — loose,
    // just to confirm we measured a sane solid (not a sliver / inside-out near 0).
    let r = HEIGHT * HALF_ANGLE.tan();
    let analytic = std::f64::consts::PI * r * r * HEIGHT / 3.0;
    assert!(
        vol > 0.5 * analytic && vol < 1.05 * analytic,
        "signed volume {vol} should be a chord-under-approximation of the \
         analytic cone volume {analytic} (within [0.5, 1.05]×)"
    );
}

// =========================================================================
// 3. Watertight + Euler χ = 2 on the independent mock.
// =========================================================================
#[test]
fn adv_watertight_and_euler() {
    let b = adversary_cone_brep();
    let mesh = b.as_mesh();

    let mut edge_count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut undirected: BTreeSet<(u32, u32)> = BTreeSet::new();
    for tri in &mesh.tris {
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            let (a, c) = (tri[i], tri[j]);
            let key = if a < c { (a, c) } else { (c, a) };
            *edge_count.entry(key).or_insert(0) += 1;
            undirected.insert(key);
        }
    }
    for (edge, count) in &edge_count {
        assert_eq!(
            *count, 2,
            "undirected edge {edge:?} shared by {count} tris (must be 2)"
        );
    }
    let v = mesh.num_verts() as i64;
    let f = mesh.num_tris() as i64;
    let e = undirected.len() as i64;
    assert_eq!(v - e + f, 2, "Euler V-E+F: V={v} E={e} F={f}");
}

// =========================================================================
// 4. WIDE-SHORT chord-bound witness — the §3 pre-pass `min` is load-bearing.
//
// ADVERSARY FINDING (documented loudly): the RED `yr16_cone.rs` oracle 1 only
// samples each lateral triangle's 3 VERTICES (all exactly on the cone → residual
// ≈ 0) plus its CENTROID. The fan-triangle centroid sits at axial fraction
// f ≈ 1/3, where the chord dip is scaled by f and the sample is off the arc
// midpoint — so for the wide-short case (R=5, h=0.5) its residual is ≈ 0.0836,
// BELOW the cone bound ≈ 0.1001, EVEN with the rim-only N=14 mesh. That means
// dropping the production `min(cone_chord_bound)` (which would size N=14 from
// the looser rim bound 0.1414 instead of N=16) does NOT red oracle 1 — oracle 1
// is too sparsely sampled to detect the difference.
//
// The sample that DOES distinguish N=14 from N=16 is the BASE-EDGE MIDPOINT of
// a lateral triangle (axial fraction f=1, the maximum chord dip): at N=14 its
// |residual| ≈ 0.1254 > cone bound 0.1001; at N=16 ≈ 0.0961 < bound. So this
// adversary oracle samples that worst-case point and bounds it by the cone
// chord bound — making the pre-pass `min` wiring load-bearing here even though
// the RED oracle 1 cannot witness it.
// =========================================================================
#[test]
fn adv_wide_short_base_edge_midpoint_within_cone_bound() {
    let half_angle = ws_half_angle();
    let b = cone_brep(WS_APEX, WS_AXIS, half_angle, WS_HEIGHT);
    let mesh = b.as_mesh();
    let bound = cone_chord_bound(WS_HEIGHT, half_angle);
    // Classification tol: a lateral vertex is on the cone (residual ≈ 0); use a
    // generous band (the rim AABB bound) so we never MISS a lateral triangle.
    let r = WS_HEIGHT * half_angle.tan();
    let class_tol = 1e-2 * ((2.0 * r).powi(2) + (2.0 * r).powi(2)).sqrt();

    let mut checked = 0usize;
    for &tri in &mesh.tris {
        let v0 = mesh.verts[tri[0] as usize].as_array();
        let v1 = mesh.verts[tri[1] as usize].as_array();
        let v2 = mesh.verts[tri[2] as usize].as_array();
        let on_cone = [v0, v1, v2]
            .iter()
            .all(|&x| cone_residual(x, WS_APEX, WS_AXIS, half_angle).abs() <= class_tol);
        if !on_cone {
            continue;
        }
        // The base edge is the one NOT touching the apex. The apex vertex has
        // residual ≈ 0 AND axial height ≈ 0; the two rim verts have axial height
        // ≈ height. Identify the apex vertex by minimal axial height.
        let ax = unit(WS_AXIS);
        let h_of = |x: [f64; 3]| dot(sub(x, WS_APEX), ax);
        let hs = [h_of(v0), h_of(v1), h_of(v2)];
        // apex index = argmin |axial|.
        let apex_i = (0..3)
            .min_by(|&i, &j| hs[i].abs().partial_cmp(&hs[j].abs()).unwrap())
            .unwrap();
        let rim: Vec<[f64; 3]> = (0..3)
            .filter(|&i| i != apex_i)
            .map(|i| [v0, v1, v2][i])
            .collect();
        // Worst-case chord sample: midpoint of the base (rim-to-rim) edge.
        let mid = scale(add(rim[0], rim[1]), 0.5);
        let res = cone_residual(mid, WS_APEX, WS_AXIS, half_angle).abs();
        assert!(
            res <= bound,
            "wide-short lateral tri {tri:?}: base-edge midpoint residual {res} \
             exceeds cone chord bound {bound} — the §3 pre-pass min() wiring is \
             NOT folding in the cone bound (mesh undersampled from the looser \
             rim-AABB bound)"
        );
        checked += 1;
    }
    assert!(
        checked >= 3,
        "expected ≥3 wide-short lateral triangles, checked {checked}"
    );
}
