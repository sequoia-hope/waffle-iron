//! Tangency pinch-vertex split at the shell gate (spec
//! `specs/yang_tangency_pinch_split.md`, task #86).
//!
//! The union of two equal-radius cylinders whose axes intersect meets
//! TANGENTIALLY at 2 isolated points; the boundary self-touches there. The
//! mesh boolean produces a vertex whose triangle star is TWO closed fans (a
//! pinch) — a valid solid whose manifold B-Rep is one vertex PER SHEET at the
//! same position. The increment splits every pinch vertex (≥2 closed fans)
//! into one vertex per fan BEFORE the shell gate, so the output presents the
//! honest χ=2 sphere.
//!
//! ## IMPORTANT — behaviour differs from the task's stated premise (measured)
//!
//! At the direct yang-rs unit level in THIS environment `boolean(Union)` does
//! NOT return `Err(NonManifoldOutput)` for these operands — it returns `Ok`
//! with a vertex-pinched mesh (edge-watertight, but χ=0: BOTH tangency points
//! welded to a single vertex each). The `s4-shell-euler` `NonManifoldOutput`
//! gate the spec references does not fire on these direct inputs. So the RED
//! discriminator here is the χ==2 / coincident-pair TOPOLOGY oracle, not an
//! Err — see the report to the lead.
//!
//! The perpendicular (90°) Steinmetz union — which the spec suggests adapting
//! from KV9-F1 — was MEASURED to produce NON-MANIFOLD EDGES (2 edges shared by
//! 4 triangles), a defect the vertex-fan split does NOT repair (spec §2 branch
//! table keeps non-manifold edges loud). It is therefore an INVALID oracle for
//! this increment and is NOT used as a positive union case. The 30° oblique
//! crossing (the C0058 corpus driver) is the only angle that produces the
//! clean isolated vertex pinch (other angles hit unrelated M5 SSI walls), so
//! both positive union oracles are on that class.
//!
//! Conventions (helpers, native_backend skip guard, cylinder_brep,
//! unpaired_half_edges / mesh_signed_volume) are re-declared verbatim from
//! `tests/kv9f1_tangency_junction.rs` — test binaries don't share modules.

use std::collections::{HashMap, HashSet};

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::Mesh;
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// Pure-Rust array math (verbatim from kv9f1_tangency_junction.rs).
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

// Cylinder B-Rep fixture (seam-edge encoding), verbatim from kv9f1.
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

/// Watertightness oracle (directed-edge balance), verbatim from kv9f1.
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

/// Signed volume of a closed triangle mesh (divergence theorem), verbatim.
fn mesh_signed_volume(mesh: &Mesh) -> f64 {
    let mut six_v = 0.0f64;
    for tri in &mesh.tris {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        six_v += dot(a, cross(b, c));
    }
    six_v / 6.0
}

/// Manifold-edge check: every UNDIRECTED edge is shared by exactly 2
/// triangles. A vertex pinch keeps all edges manifold (this returns 0); a
/// welded-along-an-edge tangency (the perpendicular defect) returns > 0. The
/// vertex split only repairs meshes for which this is 0, so the χ oracle below
/// is only meaningful when this holds.
fn nonmanifold_edges(mesh: &Mesh) -> usize {
    let mut counts: HashMap<(u32, u32), usize> = HashMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (s, e) = (tri[i], tri[j]);
            let key = if s < e { (s, e) } else { (e, s) };
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    counts.values().filter(|&&c| c != 2).count()
}

/// Whole-mesh Euler characteristic V − E + F of a CLOSED, edge-MANIFOLD
/// triangle mesh. V = count of triangle-referenced vertex indices (split
/// copies at identical positions are DISTINCT indices — the un-pinching), F =
/// T, E = 3T/2. A sphere welded at k points reads χ = 2 − k; the split output
/// reads χ = 2. Caller asserts `unpaired == 0` and `nonmanifold_edges == 0`
/// first, so E = 3T/2 holds.
fn mesh_euler_char(mesh: &Mesh) -> i64 {
    let mut used: HashSet<u32> = HashSet::new();
    for tri in &mesh.tris {
        for &i in tri {
            used.insert(i);
        }
    }
    let v = used.len() as i64;
    let f = mesh.tris.len() as i64;
    assert_eq!((3 * f) % 2, 0, "closed mesh must have an even 3·F");
    let e = 3 * f / 2;
    v - e + f
}

/// Position-bit-identical vertex groups of multiplicity > 1 (the split copies)
/// with their shared position. Today (welded pinch) → empty; after the split →
/// one entry per pinch, each multiplicity 2.
fn coincident_vertex_groups(mesh: &Mesh) -> Vec<([f64; 3], usize)> {
    let mut by_pos: HashMap<(u64, u64, u64), (usize, [f64; 3])> = HashMap::new();
    for v in &mesh.verts {
        let a = v.as_array();
        let key = (a[0].to_bits(), a[1].to_bits(), a[2].to_bits());
        let ent = by_pos.entry(key).or_insert((0, a));
        ent.0 += 1;
    }
    by_pos
        .values()
        .filter(|(c, _)| *c > 1)
        .map(|(c, pos)| (*pos, *c))
        .collect()
}

/// Two equal-R cylinders whose axes cross at the origin at `theta_deg` in the
/// y=0 plane (the C0058 class): axis A = +ẑ, axis B = (sinθ, 0, cosθ), both
/// centered on the origin. The lateral surfaces are tangent at (0, ±r, 0).
fn crossing_pair(r: f64, h: f64, theta_deg: f64) -> (BRep, BRep) {
    let th = theta_deg.to_radians();
    let axb = [th.sin(), 0.0, th.cos()];
    let a = cylinder_brep([0.0, 0.0, -h / 2.0], [0.0, 0.0, 1.0], r, h);
    let b = cylinder_brep(scale(axb, -h / 2.0), axb, r, h);
    (a, b)
}

/// Shared union topology oracle: watertight, edge-manifold, χ=2, and exactly
/// two multiplicity-2 coincident-position vertex groups (the two pinches
/// split per sheet). Returns the mesh's signed volume for the caller's
/// analytic band.
fn assert_pinch_split_union(a: &BRep, b: &BRep, sb: &dyn yang_rs::MeshBoolean, what: &str) -> f64 {
    let out = boolean(a, b, BoolOp::Union, sb)
        .unwrap_or_else(|e| panic!("pinch-split: {what} union must complete; failed with {e:?}"));
    let mesh = out.as_mesh();
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "pinch-split: {what} must be watertight"
    );
    assert_eq!(
        nonmanifold_edges(mesh),
        0,
        "pinch-split: {what} must be edge-manifold (the split repairs VERTEX \
         pinches only)"
    );
    assert_eq!(
        mesh_euler_char(mesh),
        2,
        "pinch-split: {what} split output must read the honest χ=2 sphere (a \
         welded pinch reads χ<2)"
    );
    let groups = coincident_vertex_groups(mesh);
    assert_eq!(
        groups.len(),
        2,
        "pinch-split: {what} must expose exactly 2 coincident-position vertex \
         groups (the split pinches), got {groups:?}"
    );
    for (pos, mult) in &groups {
        assert_eq!(
            *mult, 2,
            "pinch-split: {what} pinch at {pos:?} splits into 2 sheets"
        );
    }
    mesh_signed_volume(mesh)
}

// =========================================================================
// 1. Canonical (spec §4): equal-R cylinders, coplanar axes at 30°, symmetric
//    dimensions with an exact analytic union volume. r=0.4, h=4.0 (h fully
//    contains the intersection lens — MC 0.68356 vs 0.68267 in authoring).
// =========================================================================

#[test]
fn coplanar_30deg_symmetric_union_splits_pinches() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[pinch-split] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (r, h, theta_deg) = (0.4f64, 4.0f64, 30.0f64);
    let (a, b) = crossing_pair(r, h, theta_deg);
    let vol = assert_pinch_split_union(&a, &b, &sb, "30° symmetric");

    // The two pinches sit at the tangency points (0, ±r, 0).
    let groups = coincident_vertex_groups(boolean(&a, &b, BoolOp::Union, &sb).unwrap().as_mesh());
    for (pos, _) in &groups {
        assert!(
            pos[0].abs() <= 1e-9 && (pos[1].abs() - r).abs() <= 1e-9 && pos[2].abs() <= 1e-9,
            "pinch at {pos:?} is not a tangency point (0, ±{r}, 0)"
        );
    }

    // V = 2·πr²h − 16r³/(3·sinθ); θ = angle between the axes = 30°.
    let sin_theta = theta_deg.to_radians().sin();
    let v_cyl = std::f64::consts::PI * r * r * h;
    let expect = 2.0 * v_cyl - 16.0 * r * r * r / (3.0 * sin_theta);
    assert!(
        vol <= expect * 1.005 && vol >= 0.90 * expect,
        "pinch-split: 30° union volume {vol} vs analytic {expect} (chord band)"
    );
}

// =========================================================================
// 2. The C0058 corpus geometry (app/tests/cases/assay/C0058): r=0.4, axis A
//    +ẑ (depth 2 from origin), axis B (0.5,0,0.866) origin (-0.875,0,-0.5155)
//    depth 3.5 — the authored asymmetric pair. Meta expected_volume 2.08193.
// =========================================================================

#[test]
fn c0058_authored_geometry_union_splits_pinches() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[pinch-split] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let r = 0.4f64;
    let a = cylinder_brep([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], r, 2.0);
    let b = cylinder_brep(
        [-0.8749999999999999, 0.0, -0.5155444566227678],
        [0.49999999999999994, 0.0, 0.8660254037844387],
        r,
        3.5,
    );
    let vol = assert_pinch_split_union(&a, &b, &sb, "C0058 authored");

    // Corpus meta volume (chord-under-fill band; meta's own tol is 5%).
    let expect = 2.0819348684923513;
    assert!(
        vol <= expect * 1.005 && vol >= 0.90 * expect,
        "pinch-split: C0058 union volume {vol} vs meta {expect}"
    );
}

// =========================================================================
// 3. No-regression: the perpendicular SUBTRACT (kv9f1's oracle) stays green.
//    The subtract output has no pinch — the split is a no-op (I5). Same
//    assertions as kv9f1's `steinmetz_subtract_passes_stage4_with_volume_oracle`.
// =========================================================================

#[test]
fn steinmetz_subtract_stays_green() {
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[pinch-split] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let (r, h) = (0.2f64, 0.9f64);
    let a = cylinder_brep([0.0, 0.0, -h / 2.0], [0.0, 0.0, 1.0], r, h);
    let b = cylinder_brep([-h / 2.0, 0.0, 0.0], [1.0, 0.0, 0.0], r, h);
    let out = boolean(&a, &b, BoolOp::Subtract, &sb)
        .unwrap_or_else(|e| panic!("pinch-split: steinmetz subtract must stay green; got {e:?}"));
    let mesh = out.as_mesh();
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "pinch-split: subtract must be watertight"
    );
    let expect = std::f64::consts::PI * r * r * h - 16.0 * r * r * r / 3.0;
    let vol = mesh_signed_volume(mesh);
    assert!(
        vol <= expect * 1.005 && vol >= 0.90 * expect,
        "pinch-split: subtract volume {vol} vs analytic {expect} (chord under-fill band)"
    );
}
