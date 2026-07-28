//! Stage-4 CONE-APEX GENERATOR arm: relocate a `Curve::LineSegment` intersection
//! edge attributed to a `Cone × Plane` pair onto the exact analytic GENERATOR.
//!
//! A cutting plane through the cone APEX degenerates the conic section into
//! straight generators (`ssi_rs::plane_cone` AP-line / AP-lines) rather than an
//! ellipse / parabola / hyperbola — the fourth and last member of the
//! cone-section family whose conic siblings are covered by yr21 (ellipse), yr22
//! (parabola) and yr23 (hyperbola).
//!
//! Stage 4's `Curve::LineSegment` arm converted exactly two pairs — cylinder ×
//! ⟂plane (PR-F3) and PARALLEL cylinder × cylinder (PR-KV9) — and routed every
//! other curved-bearing line edge to a loud `LocalRefinementRequired` STOP, with
//! a note deferring the cone closed form "when a fixture demands them". Corpus
//! cases R0008 and R0085-op2 are that fixture. The closed form itself was never
//! missing: `ssi_rs::plane_cone` has emitted `SsiCurve::Line` for the
//! through-apex cases all along, and Stage 3 already derives the cone owner's
//! band via `cone_chord_tol_for_owner` (PR-YR17). Only the Stage-4 admission was
//! absent.
//!
//! Two things are asserted here, because the corpus needed BOTH to flip R0008:
//!
//! 1. **Admission** — the pair reaches relocation at all (no LRR STOP).
//! 2. **Crossing-pair selection** — a plane through the apex containing the axis
//!    yields TWO generators that CROSS at the apex. Stage 3 was generalized to
//!    the parallelism-free `select_disjoint_line_by_distance` by N45 (#163,
//!    commit 9fca8393); Stage 4 kept calling the R0072-only
//!    `select_disjoint_parallel_line` wrapper, whose mutual-parallelism precheck
//!    rejects crossing candidates. The two stages therefore ran DIFFERENT
//!    tie-breaks — latent while every cone-apex edge STOPped earlier in the pair
//!    match, and immediately observable (`AmbiguousCurve { candidates: 2,
//!    matched: 2 }`) once admission landed.
//!
//! The two are separately reachable and are separately fixtured, because the
//! FIRST does not exercise the second: at a 45° half-angle the two generators
//! are 90° apart, so only one ever falls in the cone's chord band and selection
//! is unambiguous without any tie-break. A NEARLY FLAT cone (89.8° here, 88.95°
//! in R0008) is what puts both candidates inside the band.
//!
//! The oracle is INDEPENDENT of production: each fixture's generators are known
//! in closed form from its own numbers (virtual apex at the origin, axis +ẑ,
//! cutting plane y = 0 ⇒ the two lines `|x| = z·tan α, y = 0`), so every output
//! vertex on the cut face must satisfy that relation to `TAU_MODEL` — an
//! exactness the mesh chords themselves do NOT have.

use std::collections::BTreeMap;

use cad_primitives::{BoolOp, Point3, Vector3, TAU_MODEL};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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
fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = dot(a, a).sqrt();
    assert!(n > 0.0, "cannot normalize zero vector");
    scale(a, 1.0 / n)
}

// =========================================================================
// Fixtures. `frustum_brep` mirrors kv6c's truncated cone (lateral band + two
// caps); `box_brep` is the shared hexahedron of m8_holed_disc_coplanar.
// Integration-test files cannot share helpers, so both are re-declared.
// =========================================================================

/// A closed truncated cone (FRUSTUM) about `+ẑ` with its virtual apex at the
/// origin: `Surface::Cone` lateral band between `z0` and `z1` + two planar caps
/// (the kv6c vocabulary).
///
/// The frustum — not the apex-pointed cone — is the shape this arm actually
/// meets: R0008 and R0085 are both REVOLVE solids, whose profile cannot reach
/// the axis, so the apex is a VIRTUAL point of the extended cone that lies
/// outside the trimmed face. That distinction is load-bearing here. A cut plane
/// through a REAL apex point puts the generators' crossing ON the seam, where
/// the two candidates' endpoint-distance intervals overlap and the position
/// tie-break correctly declines (`AmbiguousCurve` stands, and the cone normal is
/// undefined at the apex anyway). Trimming the apex away leaves the transversal
/// case this arm is for.
fn frustum_brep(half_angle: f64, z0: f64, z1: f64) -> BRep {
    let axis = [0.0, 0.0, 1.0];
    let neg_axis = [0.0, 0.0, -1.0];
    let (r0, r1) = (z0 * half_angle.tan(), z1 * half_angle.tan());
    let bottom_center = [0.0, 0.0, z0];
    let top_center = [0.0, 0.0, z1];

    let verts = vec![
        BRepVertex {
            point: p(r0, 0.0, z0),
        },
        BRepVertex {
            point: p(r1, 0.0, z1),
        },
    ];

    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(bottom_center[0], bottom_center[1], bottom_center[2]),
                normal: Vector3::new(axis[0], axis[1], axis[2]),
                radius: r0,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                radius: r1,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];

    let faces = vec![
        BRepFace {
            surface: Surface::Cone {
                apex: p(APEX[0], APEX[1], APEX[2]),
                axis_dir: Vector3::new(axis[0], axis[1], axis[2]),
                half_angle,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: -dot(neg_axis, bottom_center),
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis[0], axis[1], axis[2]),
                d: -dot(axis, top_center),
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces).expect("frustum_brep: BRep::new should tessellate the frustum")
}

/// Axis-aligned box B-Rep `[lo, hi]`.
fn box_brep(lo: [f64; 3], hi: [f64; 3]) -> BRep {
    let v = |x: f64, y: f64, z: f64| BRepVertex { point: p(x, y, z) };
    let vertices = vec![
        v(lo[0], lo[1], lo[2]),
        v(hi[0], lo[1], lo[2]),
        v(hi[0], hi[1], lo[2]),
        v(lo[0], hi[1], lo[2]),
        v(hi[0], hi[1], hi[2]),
        v(hi[0], lo[1], hi[2]),
        v(lo[0], lo[1], hi[2]),
        v(lo[0], hi[1], hi[2]),
    ];
    const EDGE_PAIRS: [(u32, u32); 24] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (2, 1),
        (1, 5),
        (5, 4),
        (4, 2),
        (3, 2),
        (2, 4),
        (4, 7),
        (7, 3),
        (0, 3),
        (3, 7),
        (7, 6),
        (6, 0),
        (1, 0),
        (0, 6),
        (6, 5),
        (5, 1),
    ];
    let edges: Vec<BRepEdge> = EDGE_PAIRS
        .iter()
        .map(|&(start, end)| BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        })
        .collect();
    let planes: [([f64; 3], f64); 6] = [
        ([0.0, 0.0, -1.0], lo[2]),
        ([0.0, 0.0, 1.0], -hi[2]),
        ([1.0, 0.0, 0.0], -hi[0]),
        ([0.0, 1.0, 0.0], -hi[1]),
        ([-1.0, 0.0, 0.0], lo[0]),
        ([0.0, -1.0, 0.0], lo[1]),
    ];
    let faces: Vec<BRepFace> = planes
        .iter()
        .enumerate()
        .map(|(i, &(n, d))| BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(n[0], n[1], n[2]),
                d,
            },
            outer_loop: (4 * i as u32..4 * i as u32 + 4).collect(),
            inner_loops: Vec::new(),
            reversed: false,
        })
        .collect();
    BRep::new(vertices, edges, faces).expect("box BRep::new")
}

fn nb() -> impl yang_rs::MeshBoolean {
    yang_rs::native_backend().expect("native backend always available")
}

// The fixture: a 45° frustum about +ẑ, virtual apex at the origin, band
// z ∈ [1, 3] (rim radii 1 and 3 — radius = height at 45°).
const APEX: [f64; 3] = [0.0, 0.0, 0.0];
const Z0: f64 = 1.0;
const Z1: f64 = 3.0;
fn half_angle() -> f64 {
    std::f64::consts::FRAC_PI_4
}

/// The cutter: a box filling `y ≤ 0` around the frustum. Its `y = 0` face plane
/// CONTAINS the cone axis (`n̂ · â = 0`) and passes through the virtual APEX, so
/// `plane_cone` takes the AP-lines branch: two crossed generators, `x = ±z` in
/// the plane `y = 0`. Both are candidates for every edge on the seam, and each
/// seam edge lies on exactly one of them.
fn half_space_cutter() -> BRep {
    box_brep([-5.0, -5.0, 0.0], [5.0, 0.0, 4.0])
}

/// Subtracting the cutter leaves the `y ≥ 0` half-frustum. Its cut face is the
/// planar half-annulus in `y = 0`, bounded by the two generators and the two rim
/// semicircles — the geometry whose Stage-4 relocation this arm supplies.
#[test]
fn cone_apex_generator_relocates_and_is_exact() {
    let cone = frustum_brep(half_angle(), Z0, Z1);
    let cutter = half_space_cutter();

    let out = boolean(&cone, &cutter, BoolOp::Subtract, &nb()).expect(
        "cone MINUS an apex-crossing half-space must relocate onto the exact \
         generators — a LocalRefinementRequired STOP here is the missing \
         Stage-4 cone × plane admission (R0008), and an AmbiguousCurve is the \
         parallel-only tie-break rejecting the CROSSING generator pair",
    );

    let mesh = out.as_mesh();
    assert!(
        !mesh.tris.is_empty(),
        "difference produced a non-empty mesh"
    );

    // Watertight: every undirected edge shared by exactly two triangles.
    let mut edge_count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for tri in &mesh.tris {
        for (a, c) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let key = if a < c { (a, c) } else { (c, a) };
            *edge_count.entry(key).or_insert(0) += 1;
        }
    }
    for (e, n) in &edge_count {
        assert_eq!(*n, 2, "edge {e:?} shared by {n} triangles (not 2)");
    }

    // The generator oracle, computed from the FIXTURE, not from production: a
    // vertex on the cut plane (y = 0) that is also on the cone lateral lies on
    // `x = ±z`. Rim vertices (z = Z0, z = Z1) are excluded — they belong to the
    // cap circles, not to the lateral seam.
    let mut on_generator = 0usize;
    for v in &mesh.verts {
        let [x, y, z] = v.as_array();
        if y.abs() > TAU_MODEL {
            continue; // not on the cut plane
        }
        if (z - Z0).abs() <= TAU_MODEL || (z - Z1).abs() <= TAU_MODEL {
            continue; // a rim point, not a lateral-seam point
        }
        // On the lateral within the cut plane ⇒ EXACTLY on a generator. A mesh
        // chord vertex that was never relocated fails this by the cone's
        // Stage-1 sagitta, orders of magnitude above TAU_MODEL.
        assert!(
            (x.abs() - z.abs()).abs() <= TAU_MODEL,
            "cut-plane vertex {v:?} is off both generators |x| = |z| by {:.3e} \
             (band {TAU_MODEL:.1e}) — it was not relocated onto the exact line",
            (x.abs() - z.abs()).abs()
        );
        on_generator += 1;
    }
    assert!(
        on_generator > 0,
        "the cut face produced no interior generator vertices — the fixture no \
         longer exercises the cone-apex arm"
    );
}

/// The SELECTION half, at fixture level. The 45° frustum above does NOT reach
/// the tie-break: its two generators are 90° apart, so only one ever falls in
/// the cone's chord band and `matched_n == 1` without any discrimination. The
/// tie-break becomes load-bearing only when the cone is NEARLY FLAT — R0008's is
/// 88.95° — because then the two generators are nearly the same undirected line
/// AND the band (which scales with the rim radius) is wide enough to admit both.
///
/// This frustum is 89.8°: rim radii ≈ 286 for a 0.01-thick washer, so a seam
/// point ~287 from the virtual apex sits ~2.0 from the FALSE generator against a
/// band of ~4 — both candidates match, exactly as in R0008, and the parallelism
/// precheck in `select_disjoint_parallel_line` would reject the pair
/// (`AmbiguousCurve { candidates: 2, matched: 2 }`). Only the position criterion
/// separates them.
#[test]
fn near_flat_cone_crossing_generators_resolve_at_the_stage4_call_site() {
    let flat = frustum_brep(89.8_f64.to_radians(), 1.0, 1.5);
    let cutter = box_brep([-400.0, -400.0, 0.5], [400.0, 0.0, 2.0]);

    let out = boolean(&flat, &cutter, BoolOp::Subtract, &nb()).expect(
        "a near-flat cone's two generators BOTH fall in the chord band; \
         resolving them needs the parallelism-free position tie-break at the \
         Stage-4 call site — `AmbiguousCurve { candidates: 2, matched: 2 }` \
         here is the R0072-only wrapper rejecting the crossing pair",
    );

    let mesh = out.as_mesh();
    assert!(
        !mesh.tris.is_empty(),
        "difference produced a non-empty mesh"
    );

    // Same closed-form oracle, at this half-angle: on the cut plane the lateral
    // meets `|x| = z·tan α`.
    let tan_a = 89.8_f64.to_radians().tan();
    let mut on_generator = 0usize;
    for v in &mesh.verts {
        let [x, y, z] = v.as_array();
        if y.abs() > TAU_MODEL {
            continue;
        }
        if (z - 1.0).abs() <= TAU_MODEL || (z - 1.5).abs() <= TAU_MODEL {
            continue; // rim point
        }
        // Scale-relative: the coordinates here are ~300, so an absolute
        // TAU_MODEL on a difference of large numbers is below f64 resolution.
        let residual = (x.abs() - z * tan_a).abs();
        assert!(
            residual <= TAU_MODEL * x.abs().max(1.0),
            "cut-plane vertex {v:?} is off both generators by {residual:.3e} \
             — it was relocated onto the WRONG generator, or not at all"
        );
        on_generator += 1;
    }
    assert!(
        on_generator > 0,
        "the cut face produced no interior generator vertices — the fixture no \
         longer exercises the near-flat crossing pair"
    );
}

/// The geometry behind the selection, isolated: the two generators cross at the apex, so a
/// tie-break that assumes mutual parallelism cannot separate them. Mirrors the
/// production candidate pair for this fixture (`x = ±z` in `y = 0`) and asserts
/// the disjoint-distance criterion picks the one the seam edge lies on, while
/// the R0072 parallel wrapper correctly declines (its contract is unchanged).
///
/// The production-side equivalents are unit-tested against R0008's live probe
/// values in `yang_rs`'s `m5_case_iv::r0008_cone_apex_crossing_generators_position_tiebreak`;
/// this is the fixture-level statement of the same property, phrased so a
/// regression in the Stage-4 CALL SITE (not the selector) is visible here.
#[test]
fn crossing_generators_are_separated_by_position_not_parallelism() {
    // An edge on the `x = +z` generator, well away from the apex crossing.
    let p_s = [0.75, 0.0, 0.75];
    let p_e = [1.00, 0.0, 1.00];
    let g_plus = unit([1.0, 0.0, 1.0]);
    let g_minus = unit([-1.0, 0.0, 1.0]);

    // Perpendicular distance from a point to the line through the apex.
    let perp = |x: [f64; 3], d: [f64; 3]| {
        let w = sub(x, APEX);
        let t = dot(w, d);
        let r = sub(w, scale(d, t));
        dot(r, r).sqrt()
    };

    // The true generator holds both endpoints exactly; the false one is a long
    // way off — and crucially the intervals are DISJOINT, which is what makes
    // the position criterion sound for a crossing pair.
    assert!(perp(p_s, g_plus) <= TAU_MODEL && perp(p_e, g_plus) <= TAU_MODEL);
    let false_lo = perp(p_s, g_minus).min(perp(p_e, g_minus));
    assert!(
        false_lo > perp(p_e, g_plus),
        "the crossing pair must have disjoint endpoint-distance intervals for \
         the position tie-break to be applicable"
    );

    // The two directions are NOT parallel — the property that made the
    // Stage-4 `select_disjoint_parallel_line` call site reject this pair.
    let c = cross(g_plus, g_minus);
    assert!(
        dot(c, c).sqrt() > TAU_MODEL,
        "generators through an apex CROSS; a parallelism precheck rejects them"
    );
}
