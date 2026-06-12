//! PR-KV7 RED — boolean OUTPUT curve recovery ("output curve tagging").
//!
//! Today a curved boolean's output carries its surviving input-rim
//! boundaries as untagged chord polylines (`LineSegment` runs), so the
//! output cannot re-enter `to_yang_brep` — every curved boolean is a
//! one-shot (`UnsupportedCurvedBoolean` on the next op). The recovery
//! principle is the Yang-paper one: output surfaces are EXACT, so the true
//! boundary between a `Cylinder` face and an adjacent `Plane` face ⊥ its
//! axis IS `cylinder ∩ plane` — a computable exact circle. The chord
//! endpoints (Stage-1 Steiner samples and Stage-4-relocated junctions)
//! already lie on it exactly; the chords are mesh artifacts. Recovery
//! retags those edges and fuses the valence-2 chord chains back to B-Rep
//! granularity (closed rims → canonical full-circle edges, partial rims →
//! minor arcs, collinear seg runs → single segs).
//!
//! GREEN flips:
//! - chains through curved intermediates work (boss → planar cut; hole →
//!   second hole; boss → through-hole = tube),
//! - the recovered pocket/boss faces are B-Rep-granular (canonical 4-edge
//!   lateral loops, 1-edge cap rings), not 2N-chord polylines,
//! - `boolean_chains.rs` re-entry pins flip from typed wall to success.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, tessellate, validate_solid, BrepArena, Curve, Profile, RenderMesh,
};

fn boxx(a: &mut BrepArena, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> kernel_v2::SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x.0, y.0),
            Point2::new(x.1, y.0),
            Point2::new(x.1, y.1),
            Point2::new(x.0, y.1),
        ],
        vec![],
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .unwrap()
        .solid
}

fn cyl(a: &mut BrepArena, cx: f64, cy: f64, r: f64, z: (f64, f64)) -> kernel_v2::SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(cx, cy),
        r,
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .unwrap()
        .solid
}

fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let mut six_v = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

fn volume_of(a: &BrepArena, s: kernel_v2::SolidId) -> f64 {
    mesh_signed_volume(&tessellate(a, s).expect("tessellate"))
}

/// Boss union, then a planar pocket cut ELSEWHERE on the slab (never
/// touching the boss). The chain's second op only meets planar geometry —
/// the canonical "this obviously should work" case.
#[test]
fn boss_union_then_planar_pocket() {
    let mut a = BrepArena::new();
    let slab = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 1.0));
    let boss = cyl(&mut a, 2.0, 2.0, 0.8, (0.5, 2.0));
    let body = boolean_op(&mut a, slab, boss, BoolOp::Union)
        .unwrap_or_else(|e| panic!("boss union: {e:?}"));
    let pocket = boxx(&mut a, (0.2, 1.0), (0.2, 1.0), (0.5, 1.5));
    let out = boolean_op(&mut a, body, pocket, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("planar pocket after boss: {e:?}"));
    validate_solid(&a, out).expect("validates");
    let vol = volume_of(&a, out);
    // slab 16 + boss π·0.8²·1.5 − pocket overlap 0.8·0.8·0.5 = 16 + 0.96π − 0.32
    let boss_v = std::f64::consts::PI * 0.8 * 0.8 * 1.5;
    let expect = 16.0 + boss_v - 0.32;
    // Cylinder chord under-fill band on the boss only.
    assert!(
        vol <= expect + 1e-9 && vol >= expect - 0.05 * boss_v,
        "vol {vol} vs {expect}"
    );
}

/// Through-hole, then a SECOND through-hole elsewhere. Both ops are curved;
/// the second must re-ingest the first's output (with its pocket wall).
#[test]
fn through_hole_then_second_hole() {
    let mut a = BrepArena::new();
    let slab = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 1.0));
    let h1 = cyl(&mut a, 1.0, 1.0, 0.4, (-0.5, 1.5));
    let s1 = boolean_op(&mut a, slab, h1, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("first hole: {e:?}"));
    let h2 = cyl(&mut a, 3.0, 3.0, 0.4, (-0.5, 1.5));
    let out = boolean_op(&mut a, s1, h2, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("second hole after first: {e:?}"));
    validate_solid(&a, out).expect("validates");
    let vol = volume_of(&a, out);
    let hole_v = std::f64::consts::PI * 0.4 * 0.4 * 1.0;
    let expect = 16.0 - 2.0 * hole_v;
    // Holes are chord-inscribed → each removes slightly LESS than πr²h.
    assert!(
        vol >= expect - 1e-9 && vol <= expect + 2.0 * 0.05 * hole_v,
        "vol {vol} vs {expect}"
    );
}

/// Boss union, then a concentric through-hole — a tube. Exercises the
/// boss lateral, its consumed-rim circle on the slab top, AND a reversed
/// pocket wall through the recovered body.
#[test]
fn boss_union_then_concentric_hole() {
    let mut a = BrepArena::new();
    let slab = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 1.0));
    let boss = cyl(&mut a, 2.0, 2.0, 0.8, (0.5, 2.0));
    let body = boolean_op(&mut a, slab, boss, BoolOp::Union)
        .unwrap_or_else(|e| panic!("boss union: {e:?}"));
    let hole = cyl(&mut a, 2.0, 2.0, 0.4, (-0.5, 2.5));
    let out = boolean_op(&mut a, body, hole, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("concentric hole through boss: {e:?}"));
    validate_solid(&a, out).expect("validates");
    let vol = volume_of(&a, out);
    let boss_v = std::f64::consts::PI * 0.8 * 0.8 * 1.5;
    let hole_v = std::f64::consts::PI * 0.4 * 0.4 * 2.0; // through slab (1) + boss (1)
    let expect = 16.0 + boss_v - hole_v;
    assert!(
        (vol - expect).abs() <= 0.05 * (boss_v + hole_v),
        "vol {vol} vs {expect}"
    );
}

/// The recovered output must be B-Rep-granular, not a chord polyline:
/// after box − cylinder (through hole), the pocket-wall cylinder face's
/// outer loop must be the canonical 4-edge [rim, seam, rim, seam] form
/// with FULL-circle rim edges — and the slab's top face must carry a
/// 1-edge full-circle inner ring.
#[test]
fn recovered_pocket_is_canonical() {
    let mut a = BrepArena::new();
    let slab = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 1.0));
    let h = cyl(&mut a, 2.0, 2.0, 0.5, (-0.5, 1.5));
    let out = boolean_op(&mut a, slab, h, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("through hole: {e:?}"));
    validate_solid(&a, out).expect("validates");

    let mut cyl_faces = 0usize;
    let mut canonical_lateral = 0usize;
    let mut full_circle_half_edges = 0usize;
    for f_slot in &a.faces {
        let Some(f) = f_slot else { continue };
        if !matches!(f.surface, Some(kernel_v2::Surface::Cylinder { .. })) {
            continue;
        }
        cyl_faces += 1;
        let n = a
            .loop_half_edges(f.outer_loop)
            .expect("outer loop walks")
            .len();
        if n == 4 {
            canonical_lateral += 1;
        }
    }
    for h_slot in &a.half_edges {
        let Some(h) = h_slot else { continue };
        if let Curve::Circle { .. } = h.curve {
            full_circle_half_edges += 1;
        }
    }
    let full_circle_edges = full_circle_half_edges / 2;
    assert!(cyl_faces >= 1, "no cylinder face in output");
    assert_eq!(
        canonical_lateral, cyl_faces,
        "pocket lateral loops are not the canonical 4-edge form (chord polylines survived)"
    );
    assert!(
        full_circle_edges >= 2,
        "expected full-circle rim edges in the recovered output, found {full_circle_edges}"
    );
}

/// Chains must stay deterministic across runs.
#[test]
fn chained_curved_boolean_deterministic() {
    let build = || {
        let mut a = BrepArena::new();
        let slab = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 1.0));
        let h1 = cyl(&mut a, 1.0, 1.0, 0.4, (-0.5, 1.5));
        let s1 = boolean_op(&mut a, slab, h1, BoolOp::Subtract).expect("hole1");
        let h2 = cyl(&mut a, 3.0, 3.0, 0.4, (-0.5, 1.5));
        let s2 = boolean_op(&mut a, s1, h2, BoolOp::Subtract).expect("hole2");
        let m = tessellate(&a, s2).expect("tessellate");
        (a, m)
    };
    let (a1, m1) = build();
    let (a2, m2) = build();
    assert_eq!(a1, a2);
    assert_eq!(m1, m2);
}

/// Three deep: boss, hole through it, then a planar pocket.
#[test]
fn three_curved_chain() {
    let mut a = BrepArena::new();
    let slab = boxx(&mut a, (0.0, 4.0), (0.0, 4.0), (0.0, 1.0));
    let boss = cyl(&mut a, 2.0, 2.0, 0.8, (0.5, 2.0));
    let body = boolean_op(&mut a, slab, boss, BoolOp::Union).expect("boss");
    let hole = cyl(&mut a, 2.0, 2.0, 0.4, (-0.5, 2.5));
    let body = boolean_op(&mut a, body, hole, BoolOp::Subtract).expect("hole");
    let pocket = boxx(&mut a, (0.2, 1.0), (0.2, 1.0), (0.5, 1.5));
    let out = boolean_op(&mut a, body, pocket, BoolOp::Subtract).expect("pocket");
    validate_solid(&a, out).expect("validates");
    assert!(volume_of(&a, out) > 0.0);
}
