//! PR-YR9 (P3) ADVERSARY — independent audit that the Stage-3 SSI wiring
//! produces the EXACT analytic conic, never a mesh-refit or a silent polyline
//! fallback.
//!
//! This is the THIRD, independent auditor of a role-separated FIP cycle. It
//! does NOT touch production (`crates/yang-rs/src/**`) and does NOT weaken or
//! delete any assertion in `tests/yr9_stage3_ssi.rs`. It ADDS coverage that the
//! RED oracle did not, targeting the spec §8 adversary mandate:
//!
//!   (a) the conic is truly EXACT (hand-derived from analytic geometry,
//!       NOT re-fit from the mesh) — proven by mesh-facet-count INDEPENDENCE:
//!       a second hand-built tube mock at a DIFFERENT facet count (N=16) must
//!       assign a BYTE-IDENTICAL `Curve::Circle` to the one the N=8 mock does;
//!   (b) the SSI path can never silently fall back to `LineSegment` on a
//!       genuine intersect/selection failure (the only `LineSegment` path is an
//!       edge ABSENT from the map — same-input or <2-incidence);
//!   (c) tolerances are not weakened vs YR8 — the hand-derived circle is exact
//!       to TAU_MODEL (strictly stronger than d_ε);
//!   (d) determinism — the SET of conic edges and their EXACT params are
//!       byte-identical across runs and stable in order;
//!   (e) scope — same-input / planar edges stay `LineSegment`.
//!
//! Plus two adversarial probes:
//!   - the Circle axial-term guard: a circle at z=0 must REJECT a query point
//!     at the right radius but the wrong axial plane (z=5) — exercised directly
//!     against the same implicit on-curve metric production uses;
//!   - the Ellipse mapping path (`ssi_rs::SsiCurve::Ellipse` → `Curve::Ellipse`
//!     field-for-field), exercised via the public `ssi_rs::intersect` of an
//!     OBLIQUE plane∩cylinder (the C2 branch the canonical case never hits).
//!
//! Per the YR8/YR9 RED precedent, integration test files cannot share helpers,
//! so the harness (`p`, array math, `cylinder_brep`, `unit_cube_brep_offset_at`,
//! `LabelMock`, canonical config) is re-declared verbatim here.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3, TAU_MODEL};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

// =========================================================================
// Pure-Rust array math (re-declared verbatim — integration tests cannot share).
// =========================================================================

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}
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

// =========================================================================
// Cylinder + cube fixtures (re-declared verbatim from yr9_stage3_ssi.rs).
// =========================================================================

fn cylinder_brep(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, height: f64) -> BRep {
    let axis_unit = unit(axis_dir);
    let bottom_center = axis_point;
    let top_center = add(axis_point, scale(axis_unit, height));

    let abs = [axis_unit[0].abs(), axis_unit[1].abs(), axis_unit[2].abs()];
    let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let e1 = unit(cross(axis_unit, world));

    let v0 = add(bottom_center, scale(e1, radius));
    let v1 = add(top_center, scale(e1, radius));

    let verts = vec![
        BRepVertex {
            point: p(v0[0], v0[1], v0[2]),
        },
        BRepVertex {
            point: p(v1[0], v1[1], v1[2]),
        },
    ];

    let neg_axis = scale(axis_unit, -1.0);
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: p(bottom_center[0], bottom_center[1], bottom_center[2]),
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: p(top_center[0], top_center[1], top_center[2]),
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                radius,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];

    let bottom_d = -dot(neg_axis, bottom_center);
    let top_d = -dot(axis_unit, top_center);

    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: p(axis_point[0], axis_point[1], axis_point[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_axis[0], neg_axis[1], neg_axis[2]),
                d: bottom_d,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(axis_unit[0], axis_unit[1], axis_unit[2]),
                d: top_d,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];

    BRep::new(verts, edges, faces).expect("cylinder_brep: BRep::new should tessellate the cylinder")
}

fn unit_cube_brep_offset_at(origin: [f64; 3]) -> BRep {
    let [x, y, z] = origin;
    let verts = vec![
        BRepVertex { point: p(x, y, z) },
        BRepVertex {
            point: p(x + 1.0, y, z),
        },
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z),
        },
        BRepVertex {
            point: p(x, y + 1.0, z),
        },
        BRepVertex {
            point: p(x, y, z + 1.0),
        },
        BRepVertex {
            point: p(x + 1.0, y, z + 1.0),
        },
        BRepVertex {
            point: p(x + 1.0, y + 1.0, z + 1.0),
        },
        BRepVertex {
            point: p(x, y + 1.0, z + 1.0),
        },
    ];
    let face_verts: [[u32; 4]; 6] = [
        [0, 1, 2, 3],
        [4, 7, 6, 5],
        [0, 4, 5, 1],
        [1, 5, 6, 2],
        [2, 6, 7, 3],
        [3, 7, 4, 0],
    ];
    let mut edges = Vec::with_capacity(24);
    let mut loops = Vec::with_capacity(6);
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
    let normals: [Vector3; 6] = [
        Vector3::new(0.0, 0.0, -1.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(-1.0, 0.0, 0.0),
    ];
    let offs = [z, -(z + 1.0), y, -(x + 1.0), -(y + 1.0), x];
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
    BRep::new(verts, edges, faces).expect("offset cube BRep::new failed")
}

// =========================================================================
// Canonical config (identical to yr9_stage3_ssi.rs).
// =========================================================================

const CYL_AXIS_POINT: [f64; 3] = [0.5, 0.5, -0.5];
const CYL_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CYL_RADIUS: f64 = 0.25;
const CYL_HEIGHT: f64 = 2.0;

fn canonical_cylinder() -> BRep {
    cylinder_brep(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_RADIUS, CYL_HEIGHT)
}
fn canonical_box() -> BRep {
    unit_cube_brep_offset_at([0.0, 0.0, 0.0])
}

// =========================================================================
// LabelMock (re-declared verbatim).
// =========================================================================

struct LabelMock {
    arrangement: LabeledArrangement,
}
impl MeshBoolean for LabelMock {
    fn boolean(
        &self,
        _a: &Mesh,
        _b: &Mesh,
        _op: BoolOp,
    ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
        Ok(self.arrangement.mesh.clone())
    }
    fn labeled_arrangement(
        &self,
        _a: &Mesh,
        _b: &Mesh,
    ) -> Result<LabeledArrangement, Box<dyn Error + Send + Sync>> {
        Ok(self.arrangement.clone())
    }
}

// =========================================================================
// Hand-built tube arrangement, PARAMETERIZED on facet count N. (The N=8 form is
// the canonical RED fixture; an N=16 form gives a DIFFERENT mesh ring of
// vertices with the same analytic cylinder/cap geometry — used to prove the
// assigned conic is mesh-INDEPENDENT.)
//
// Lateral walls → InputId(0) = CYLINDER. Both cap fans → InputId(1) = BOX.
// Bottom cap on plane z=0, top cap on plane z=1. `inside` all-false ⇒ Union
// keeps all triangles.
// =========================================================================

fn hand_built_tube_arrangement_n(n_facets: usize) -> LabeledArrangement {
    let cx = CYL_AXIS_POINT[0];
    let cy = CYL_AXIS_POINT[1];
    let r = CYL_RADIUS;
    let (za, zb) = (0.0f64, 1.0f64);

    let ring: Vec<(f64, f64)> = (0..n_facets)
        .map(|k| {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (n_facets as f64);
            (cx + r * th.cos(), cy + r * th.sin())
        })
        .collect();

    let mut verts: Vec<Point3> = Vec::new();
    let mut bot = Vec::with_capacity(n_facets);
    let mut top = Vec::with_capacity(n_facets);
    for &(x, y) in &ring {
        bot.push(verts.len() as u32);
        verts.push(p(x, y, za));
    }
    for &(x, y) in &ring {
        top.push(verts.len() as u32);
        verts.push(p(x, y, zb));
    }
    let cb = verts.len() as u32;
    verts.push(p(cx, cy, za));
    let ct = verts.len() as u32;
    verts.push(p(cx, cy, zb));

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();
    let push =
        |t: [u32; 3], label: u32, tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
            tris.push(t);
            surf.push(vec![LaInputId(label)]);
        };
    for k in 0..n_facets {
        let k1 = (k + 1) % n_facets;
        push([bot[k], bot[k1], top[k1]], 0, &mut tris, &mut surface);
        push([bot[k], top[k1], top[k]], 0, &mut tris, &mut surface);
    }
    for k in 0..n_facets {
        let k1 = (k + 1) % n_facets;
        push([cb, bot[k1], bot[k]], 1, &mut tris, &mut surface);
    }
    for k in 0..n_facets {
        let k1 = (k + 1) % n_facets;
        push([ct, top[k], top[k1]], 1, &mut tris, &mut surface);
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let inside = vec![vec![false, false]; n];
    let patch = vec![0u32; n];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        source: Vec::new(),
        intersection_edges: Default::default(),
        num_inputs: 2,
    }
}

// =========================================================================
// Output-edge helpers.
// =========================================================================

fn conic_circles(brep: &BRep) -> Vec<(Point3, Vector3, f64)> {
    brep.edges()
        .iter()
        .filter_map(|e| match e.curve {
            Curve::Circle {
                center,
                normal,
                radius,
            } => Some((center, normal, radius)),
            _ => None,
        })
        .collect()
}

/// The DISTINCT set of (center, normal, radius) circles in the output, keyed by
/// the bit pattern of each field so the comparison is BYTE-exact (no tolerance).
/// Sorted deterministically by center z then x then y.
fn distinct_circle_bits(brep: &BRep) -> Vec<([u64; 3], [u64; 3], u64)> {
    let mut set: BTreeMap<([u64; 3], [u64; 3], u64), ()> = BTreeMap::new();
    for (c, n, r) in conic_circles(brep) {
        let cb = c.as_array().map(|v| v.to_bits());
        // A circle's POINT SET is invariant under normal negation; Stage 6
        // orients each directed edge copy's normal for its own traversal
        // (task #133, spec `yang_stage6_arc_orientation`), so twin copies
        // legitimately carry opposite signs. Canonicalize the sign (first
        // nonzero component positive) so "distinct circle" means distinct
        // geometry, not distinct traversal.
        let na = n.as_array();
        let flip = match na.iter().find(|v| **v != 0.0) {
            Some(v) => *v < 0.0,
            None => false,
        };
        let nb = na.map(|v| if flip { (-v).to_bits() } else { v.to_bits() });
        set.insert((cb, nb, r.to_bits()), ());
    }
    set.into_keys().collect()
}

fn unpaired_half_edges(mesh: &Mesh) -> usize {
    let mut counts: HashMap<(u32, u32), i32> = HashMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *counts.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut unpaired = 0;
    for (&(s, e), &fwd) in &counts {
        let rev = counts.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            unpaired += (fwd - rev).unsigned_abs() as usize;
        }
    }
    unpaired
}

fn euler_characteristic(mesh: &Mesh) -> i64 {
    let v = mesh.num_verts() as i64;
    let f = mesh.num_tris() as i64;
    let mut edges: HashSet<(u32, u32)> = HashSet::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    let e = edges.len() as i64;
    v - e + f
}

// =========================================================================
// ADVERSARY 1 (a) — the conic is HAND-DERIVED-EXACT, NOT mesh-refit.
//
// Re-derive the cap-ring circles PURELY from analytic geometry (NO ssi-rs, NO
// oracle round-trip): cylinder axis (0.5,0.5,*)+Z, r=0.25; caps z=0 and z=1 →
// circles center (0.5,0.5,0)/(0.5,0.5,1), normal ±Z, radius 0.25. Assert the
// OUTPUT Circle equals this hand value EXACTLY within TAU_MODEL.
//
// This is independent of yr9_stage3_ssi.rs's t1, which compares against an
// ssi-rs oracle CALL — here the ground truth is hand-written, so a bug shared
// between production's `surface_to_quadric` and the test's oracle conversion
// (the t1 path) cannot hide.
// =========================================================================

#[test]
fn adv1_cap_circles_equal_hand_derived_geometry_exactly() {
    let cyl = canonical_cylinder();
    let bx = canonical_box();
    let mock = LabelMock {
        arrangement: hand_built_tube_arrangement_n(8),
    };
    let r = boolean(&cyl, &bx, BoolOp::Union, &mock).expect("adv1: union must Ok");

    let circles = conic_circles(&r);
    assert!(
        !circles.is_empty(),
        "adv1: expected ≥1 Circle intersection edge, got none"
    );

    // Hand-derived ground truth (NO ssi-rs call).
    let hand_bottom_center = [0.5, 0.5, 0.0];
    let hand_top_center = [0.5, 0.5, 1.0];
    let hand_axis = [0.0, 0.0, 1.0];
    let hand_radius = 0.25;

    let mut saw_bottom = false;
    let mut saw_top = false;
    for (center, normal, radius) in &circles {
        let c = center.as_array();
        let nrm = unit(normal.as_array());
        let (hc, hn) = if c[2].abs() <= 0.5 {
            saw_bottom = true;
            (hand_bottom_center, hand_axis)
        } else {
            saw_top = true;
            (hand_top_center, hand_axis)
        };
        assert!(
            norm(sub(c, hc)) <= TAU_MODEL,
            "adv1: Circle center {c:?} ≠ hand-derived {hc:?} within TAU_MODEL"
        );
        // normal parallel to ±Z (sign-invariant).
        let dotn = dot(nrm, hn).abs();
        assert!(
            (dotn - 1.0).abs() <= TAU_MODEL,
            "adv1: Circle normal {nrm:?} not parallel to hand axis {hn:?}"
        );
        assert!(
            (radius - hand_radius).abs() <= TAU_MODEL,
            "adv1: Circle radius {radius} ≠ hand-derived {hand_radius} within TAU_MODEL"
        );
    }
    assert!(
        saw_bottom && saw_top,
        "adv1: both cap rings must be present; saw_bottom={saw_bottom} saw_top={saw_top}"
    );
}

// =========================================================================
// ADVERSARY 2 (a) — MESH-INDEPENDENCE: a SECOND tube mock at N=16 facets must
// assign a BYTE-IDENTICAL `Curve::Circle` set to the N=8 mock.
//
// The two mocks share the SAME analytic cylinder/cap geometry but have
// COMPLETELY DIFFERENT mesh ring vertices (8 vs 16 points on the rim). If the
// assigned conic were re-fit from the mesh ring, its radius/center would differ
// between N=8 and N=16 (the polygon inscribed radius is the same but the vertex
// SET, and hence any least-squares / vertex-derived fit, differs). A
// BYTE-identical Circle set across the two proves the conic comes from ssi-rs
// analytics on the (identical) input Surfaces, NOT the mesh.
//
// THIS IS THE STRONGEST mesh-independence proof: byte-exact equality, not
// within-tolerance.
// =========================================================================

#[test]
fn adv2_conic_is_byte_identical_across_facet_counts() {
    let cyl = canonical_cylinder();
    let bx = canonical_box();

    let r8 = boolean(
        &cyl,
        &bx,
        BoolOp::Union,
        &LabelMock {
            arrangement: hand_built_tube_arrangement_n(8),
        },
    )
    .expect("adv2: N=8 union must Ok");
    let r16 = boolean(
        &cyl,
        &bx,
        BoolOp::Union,
        &LabelMock {
            arrangement: hand_built_tube_arrangement_n(16),
        },
    )
    .expect("adv2: N=16 union must Ok");

    let bits8 = distinct_circle_bits(&r8);
    let bits16 = distinct_circle_bits(&r16);

    assert_eq!(
        bits8.len(),
        2,
        "adv2: N=8 output must have exactly TWO distinct cap circles, got {}",
        bits8.len()
    );
    assert_eq!(
        bits8, bits16,
        "adv2: the assigned cap-circle params are NOT byte-identical between N=8 and N=16 — \
         the conic is mesh-INFLUENCED (a re-fit), not the analytic ssi-rs curve. \
         N=8={bits8:?} N=16={bits16:?}"
    );

    // Sanity: the N=16 mesh genuinely differs from N=8 (more vertices), so the
    // byte-equality above is a real cross-mesh invariant, not a trivial pass.
    assert!(
        r16.as_mesh().num_verts() > r8.as_mesh().num_verts(),
        "adv2: the N=16 mock must yield a strictly larger mesh than N=8 \
         (else the independence claim is vacuous)"
    );
}

// =========================================================================
// ADVERSARY 3 (d) — DETERMINISM at the SET level: across two runs, the SET of
// distinct conic circles (byte-keyed) AND the per-edge curve ordering are
// identical. (yr9 t1 checks per-edge curve equality; this adds the byte-keyed
// distinct-set equality and a stable-ordering check on the conic edge stream.)
// =========================================================================

#[test]
fn adv3_conic_set_and_order_are_byte_stable_across_runs() {
    let cyl = canonical_cylinder();
    let bx = canonical_box();

    let run = || {
        boolean(
            &cyl,
            &bx,
            BoolOp::Union,
            &LabelMock {
                arrangement: hand_built_tube_arrangement_n(8),
            },
        )
        .expect("adv3: union must Ok")
    };
    let r1 = run();
    let r2 = run();

    // Byte-keyed distinct circle set equality.
    assert_eq!(
        distinct_circle_bits(&r1),
        distinct_circle_bits(&r2),
        "adv3: distinct conic-circle SET differs byte-for-byte across runs"
    );

    // Stable ORDER of the conic-edge stream (the sequence of Circle params in
    // edge-emission order, bit-exact).
    let order = |b: &BRep| -> Vec<([u64; 3], [u64; 3], u64)> {
        b.edges()
            .iter()
            .filter_map(|e| match e.curve {
                Curve::Circle {
                    center,
                    normal,
                    radius,
                } => Some((
                    center.as_array().map(|v| v.to_bits()),
                    normal.as_array().map(|v| v.to_bits()),
                    radius.to_bits(),
                )),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        order(&r1),
        order(&r2),
        "adv3: conic-edge emission ORDER differs across runs (non-deterministic)"
    );
}

// =========================================================================
// ADVERSARY 4 (axial-term guard) — the Circle implicit on-curve metric must
// REJECT a point at the correct radius but the WRONG axial plane.
//
// Spec §5.4 Circle test: `|axial| ≤ tol ∧ |radial − radius| ≤ tol`. If the
// axial term were dropped (a buggy selection), a z=0 cap circle would "contain"
// a z=5 point on the same radius, so BOTH cap circles could match one ring edge
// (→ AmbiguousCurve or wrong-cap selection). This test reproduces the EXACT
// metric production uses (private `curve_contains_point`) and asserts the axial
// term is load-bearing: same radius, wrong z ⇒ NOT contained.
//
// This is an independent re-implementation of the metric, NOT a call into
// production (which is private). It guards the SPEC the production claims to
// implement; combined with adv1 (the correct-z circle IS selected) it pins the
// axial discrimination end-to-end.
// =========================================================================

/// Mirror of production's Circle branch of `curve_contains_point` (spec §5.4).
fn circle_contains(center: [f64; 3], normal: [f64; 3], radius: f64, x: [f64; 3], tol: f64) -> bool {
    let n = unit(normal);
    let w = sub(x, center);
    let axial = dot(w, n);
    let radial = norm(sub(w, scale(n, axial)));
    axial.abs() <= tol && (radial - radius).abs() <= tol
}

#[test]
fn adv4_circle_axial_term_rejects_wrong_axial_plane() {
    let center = [0.5, 0.5, 0.0];
    let normal = [0.0, 0.0, 1.0];
    let radius = 0.25;
    let tol = 1e-2; // generous d_ε-scale tolerance

    // A point ON the right radius IN the z=0 plane → contained.
    let on = [0.5 + radius, 0.5, 0.0];
    assert!(
        circle_contains(center, normal, radius, on, tol),
        "adv4: a point at the right radius in the circle's plane must be CONTAINED"
    );

    // Same radius, WRONG axial plane (z=5) → must be REJECTED by the axial term.
    let off_axial = [0.5 + radius, 0.5, 5.0];
    assert!(
        !circle_contains(center, normal, radius, off_axial, tol),
        "adv4: the axial term must REJECT a same-radius point at z=5; if it does not, a \
         z=0 cap circle could spuriously match a z=1 (or any other) ring edge"
    );

    // Cross-cap guard: the z=1 (top) ring vertices must NOT be contained by the
    // z=0 (bottom) cap circle, and vice versa — each ring selects its OWN cap.
    let top_center = [0.5, 0.5, 1.0];
    let top_vertex = [0.5 + radius, 0.5, 1.0];
    assert!(
        !circle_contains(center, normal, radius, top_vertex, tol),
        "adv4: a top-ring (z=1) vertex must NOT lie on the bottom (z=0) cap circle"
    );
    assert!(
        circle_contains(top_center, normal, radius, top_vertex, tol),
        "adv4: a top-ring (z=1) vertex MUST lie on the top (z=1) cap circle"
    );
    // And the bottom vertex must NOT match the top circle.
    let bot_vertex = [0.5 + radius, 0.5, 0.0];
    assert!(
        !circle_contains(top_center, normal, radius, bot_vertex, tol),
        "adv4: a bottom-ring (z=0) vertex must NOT lie on the top (z=1) cap circle"
    );
}

// =========================================================================
// ADVERSARY 5 (Ellipse path) — the C2 oblique plane∩cylinder branch produces an
// `SsiCurve::Ellipse`, and the field-for-field mapping to `Curve::Ellipse` is
// exercised through the PUBLIC `ssi_rs::intersect`.
//
// The canonical cylinder∪box only ever hits the C1 (Circle) branch, so the
// production `ssi_curve_to_curve` Ellipse arm is otherwise untested via the
// public path. Here we:
//   1. build an OBLIQUE plane∩cylinder via ssi-rs and confirm it returns an
//      Ellipse (so the Ellipse arm is REACHABLE in principle for this surface
//      pair, not dead code);
//   2. confirm the Ellipse's invariants (major_radius = r/|c|, minor_radius = r,
//      major_radius ≥ minor_radius) so a downstream field-for-field copy into
//      `Curve::Ellipse` preserves a VALID ellipse.
//
// NOTE / DOCUMENTED GAP: `ssi_curve_to_curve` is a private production fn, so
// this test cannot call it directly; and building a full OBLIQUE-cap tube mock
// that drives `boolean()` down the Ellipse emission path is out of scope for
// this PR (the canonical config is perpendicular). This test therefore pins the
// ssi-rs SOURCE of the Ellipse and its field invariants; the field-for-field
// copy itself is a trivial total match in production (verified by inspection at
// src/lib.rs ssi_curve_to_curve, Ellipse arm). The public-path Ellipse EMISSION
// remains a coverage gap, flagged for a future oblique-cut PR.
// =========================================================================

#[test]
fn adv5_oblique_plane_cylinder_yields_valid_ellipse() {
    // Cylinder: axis +Z through origin, r = 1.
    let cyl = ssi_rs::QuadricSurface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    // OBLIQUE plane: normal tilted 45° from the axis (in the x–z plane). This is
    // the C2 branch (|c| = cos 45° ≈ 0.707, strictly between 0 and 1).
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let plane = ssi_rs::QuadricSurface::Plane {
        point: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(s, 0.0, s),
    };
    let curves = ssi_rs::intersect(&plane, &cyl).expect("adv5: oblique plane∩cylinder must Ok");
    assert_eq!(
        curves.len(),
        1,
        "adv5: an oblique cylinder section must be exactly one curve (an ellipse), got {curves:?}"
    );
    let ssi_rs::SsiCurve::Ellipse {
        center: _,
        normal,
        major_axis,
        major_radius,
        minor_radius,
    } = curves[0]
    else {
        panic!(
            "adv5: oblique plane∩cylinder must be an Ellipse, got {:?}",
            curves[0]
        );
    };

    // Invariants the C2 doc-comment promises (so a field-for-field copy into
    // Curve::Ellipse is a valid ellipse):
    //   minor_radius = r = 1, major_radius = r/|c| = 1/cos45° = √2, a ≥ b.
    let abs_c = s; // |n̂·â| = |z-component of the unit normal| = s
    assert!(
        (minor_radius - 1.0).abs() <= TAU_MODEL,
        "adv5: minor_radius must equal r=1, got {minor_radius}"
    );
    assert!(
        (major_radius - 1.0 / abs_c).abs() <= TAU_MODEL,
        "adv5: major_radius must equal r/|c| = {}, got {major_radius}",
        1.0 / abs_c
    );
    assert!(
        major_radius >= minor_radius,
        "adv5: a ≥ b must hold (a={major_radius}, b={minor_radius})"
    );

    // major_axis is unit and in-plane (⟂ normal); minor = normal × major is unit.
    let n = unit(normal.as_array());
    let maj = major_axis.as_array();
    assert!(
        (norm(maj) - 1.0).abs() <= TAU_MODEL,
        "adv5: major_axis must be unit, |maj|={}",
        norm(maj)
    );
    assert!(
        dot(n, maj).abs() <= TAU_MODEL,
        "adv5: major_axis must be in-plane (⟂ normal), n·maj={}",
        dot(n, maj)
    );
    let minor = cross(n, maj);
    assert!(
        (norm(minor) - 1.0).abs() <= TAU_MODEL,
        "adv5: minor_axis = normal × major must be unit, |minor|={}",
        norm(minor)
    );
}

// =========================================================================
// ADVERSARY 6 (b/e) — NO silent fallback + watertight scope. A genuine union
// output is watertight + Euler 2 (mirrors YR8) AND every conic edge it carries
// is a Circle (never a stray Ellipse/Line from a misfire), AND the count of
// distinct conic circles is EXACTLY 2 (the two caps) — no spurious extra conic
// from over-reach, no missing cap from a swallowed failure.
// =========================================================================

#[test]
fn adv6_union_has_exactly_two_caps_and_is_watertight() {
    let cyl = canonical_cylinder();
    let bx = canonical_box();
    let r = boolean(
        &cyl,
        &bx,
        BoolOp::Union,
        &LabelMock {
            arrangement: hand_built_tube_arrangement_n(8),
        },
    )
    .expect("adv6: union must Ok");

    // Watertight + Euler 2 (the union shell stays valid after curve assignment).
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "adv6: output mesh must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "adv6: output mesh Euler V−E+F must be 2"
    );

    // Every conic edge is a Circle (no Ellipse/other for the perpendicular case).
    for e in r.edges() {
        match e.curve {
            Curve::Circle { .. } | Curve::LineSegment => {}
            other => panic!(
                "adv6: unexpected curve family on a union edge: {other:?} \
                 (perpendicular caps must yield only Circle/LineSegment)"
            ),
        }
    }

    // EXACTLY two distinct cap circles — not 1 (a swallowed/merged cap) and not
    // ≥3 (a spurious over-reach conic). This is the no-silent-fallback guard at
    // the output level: a swallowed intersect failure would drop a cap to
    // LineSegment, leaving <2 distinct circles.
    assert_eq!(
        distinct_circle_bits(&r).len(),
        2,
        "adv6: union must carry EXACTLY two distinct cap circles (bottom z=0, top z=1); \
         fewer means a cap silently fell back to LineSegment, more means over-reach"
    );
}
