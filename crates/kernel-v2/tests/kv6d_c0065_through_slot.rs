//! C0065 pinned (2026-09-17): a closed torus (R = 1.2, r = 0.3, axis +z,
//! tube plane z = 0.5) minus a vertical square shaft `x ∈ [0.95, 1.45]`,
//! `|y| ≤ 0.25` that spans the tube's radial extent 0.9..1.5 all but the
//! two 0.05-deep bridges — a THROUGH-SLOT that leaves the tube's surface
//! with two windows (top and bottom of the tube) and the solid genus 2.
//!
//! Two capabilities meet here, both always-on:
//!
//! - **Yang §4.5.2 local refinement** (`yang-rs::boolean::refine_452`, spec
//!   `specs/yang_452_local_refinement.md` §7): the x = 1.45 wall grazes the
//!   torus 0.05 deep while the natural rim chord sags 0.038, so the mesh
//!   loop reached only |y| = 0.22 of the true 0.38 and never crossed the
//!   |y| = 0.25 clip walls; the out-of-domain relocation is the paper's own
//!   refinement trigger, and the op-level d_ε ladder converges at d_ε/4
//!   with all eight torus∩wall∩wall corners exact.
//! - **KV14 Slice F-4** (`yang_rs::tessellate_torus_patch`'s windows arm):
//!   the output torus face is the CLOSED tube minus two windows — no loop
//!   wraps either period, the "outer" loop bounds the complement — laid as
//!   one full period rectangle with both seam cuts clear of the windows.
//!
//! Before either landed this chain STOPped typed at Stage 4
//! (`OffCurveBeyondChordBand` v8, spec `yang_137_torus_plane_grazing_corner.md`).

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, Profile, RenderMesh,
};

const R_MAJ: f64 = 1.2;
const R_MIN: f64 = 0.3;
const ZC: f64 = 0.5;
/// Shaft walls: `x ∈ [X_IN, X_OUT]`, `|y| ≤ H`.
const X_IN: f64 = 0.95;
const X_OUT: f64 = 1.45;
const H: f64 = 0.25;

fn build(arena: &mut BrepArena) -> kernel_v2::SolidId {
    // Sketch plane normal +y (u along +z, v along +x); circle center world
    // (−1.2, 0, 0.5), radius 0.3; revolve axis +z through the origin.
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        Vector3::new(1.0, 0.0, 0.0),
        Point2::new(ZC, -R_MAJ),
        R_MIN,
    )
    .expect("circle profile");
    let torus = revolve(
        arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        2.0 * PI,
    )
    .expect("closed torus");

    let shaft_profile = Profile::new(
        Point3::new(0.0, 0.0, -1.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(X_IN, -H),
            Point2::new(X_OUT, -H),
            Point2::new(X_OUT, H),
            Point2::new(X_IN, H),
        ],
        vec![],
    )
    .expect("shaft profile");
    let shaft =
        extrude(arena, &shaft_profile, Vector3::new(0.0, 0.0, 3.0), 3.0).expect("shaft box");

    boolean_op(arena, torus.solid, shaft.solid, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("torus − shaft failed: {e:?}"))
}

/// χ of the render mesh welded by position (the assay oracle's reading):
/// V − E + F over the welded vertex set, counting only vertices some
/// triangle references (a UV-CDT patch leaves its dropped hole/exterior
/// Steiner points in the pool as unreferenced render vertices).
fn welded_euler(mesh: &RenderMesh) -> i64 {
    use std::collections::{BTreeMap, BTreeSet};
    let q = |x: f64| (x / 1e-9).round() as i64;
    let mut id_of: BTreeMap<(i64, i64, i64), u32> = BTreeMap::new();
    let mut welded: Vec<u32> = Vec::with_capacity(mesh.positions.len() / 3);
    for p in mesh.positions.chunks_exact(3) {
        let k = (q(p[0]), q(p[1]), q(p[2]));
        let n = id_of.len() as u32;
        welded.push(*id_of.entry(k).or_insert(n));
    }
    let mut used: BTreeSet<u32> = BTreeSet::new();
    let mut edges: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut faces = 0i64;
    for t in mesh.indices.chunks_exact(3) {
        let w = [
            welded[t[0] as usize],
            welded[t[1] as usize],
            welded[t[2] as usize],
        ];
        if w[0] == w[1] || w[1] == w[2] || w[2] == w[0] {
            continue;
        }
        faces += 1;
        used.extend(w);
        for (a, b) in [(w[0], w[1]), (w[1], w[2]), (w[2], w[0])] {
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    used.len() as i64 - edges.len() as i64 + faces
}

fn assert_watertight(mesh: &RenderMesh, what: &str) {
    use std::collections::HashMap;
    let q = |x: f64| (x / 1e-9).round() as i64;
    let key = |i: u32| {
        let k = (i as usize) * 3;
        (
            q(mesh.positions[k]),
            q(mesh.positions[k + 1]),
            q(mesh.positions[k + 2]),
        )
    };
    let mut count: HashMap<_, i64> = HashMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(a), key(b));
            if ka == kb {
                continue;
            }
            *count.entry((ka, kb)).or_insert(0) += 1;
            *count.entry((kb, ka)).or_insert(0) -= 1;
        }
    }
    let unpaired = count.values().filter(|&&c| c != 0).count();
    assert_eq!(unpaired, 0, "{what}: {unpaired} unpaired directed edges");
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
    let mut six = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six / 6.0
}

#[test]
fn torus_through_slot_is_genus_two_with_exact_corners() {
    let mut arena = BrepArena::new();
    let out = build(&mut arena);

    let report = validate_solid(&arena, out).expect("output validates");
    assert_eq!(report.shells, 1, "one connected shell");
    assert_eq!(
        report.faces, 5,
        "torus + the four shaft walls (the shaft's caps miss the tube)"
    );
    assert_eq!(
        report.rings, 1,
        "the torus face carries its second window as an inner loop"
    );

    let mesh = tessellate(&arena, out).expect("slotted torus tessellates");
    assert!(!mesh.indices.is_empty());
    assert_watertight(&mesh, "slotted torus");
    assert_eq!(
        welded_euler(&mesh),
        -2,
        "a through-slot leaving both bridges is genus 2 (χ = −2)"
    );

    // The eight exact torus∩wall∩wall corners are output vertices:
    // radial ρ = √(x² + H²), z = ZC ± √(r² − (ρ − R)²).
    for x in [X_IN, X_OUT] {
        let rho = (x * x + H * H).sqrt();
        let dz = (R_MIN * R_MIN - (rho - R_MAJ) * (rho - R_MAJ)).sqrt();
        for sy in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let c = [x, sy * H, ZC + sz * dz];
                let hit = mesh.positions.chunks_exact(3).any(|p| {
                    ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt()
                        <= 1e-9
                });
                assert!(hit, "exact corner {c:?} is not an output vertex");
            }
        }
    }

    // Volume: the full tube 2π²Rr² minus the slot's bite — the tube's
    // cross-section area inside the shaft's footprint, integrated over
    // |y| ≤ H (midpoint rule on (x, y), 800²: error ≪ the facet band).
    let n = 800;
    let (hx, hy) = ((X_OUT - X_IN) / n as f64, 2.0 * H / n as f64);
    let mut bite = 0.0;
    for i in 0..n {
        for j in 0..n {
            let x = X_IN + (i as f64 + 0.5) * hx;
            let y = -H + (j as f64 + 0.5) * hy;
            let rho = (x * x + y * y).sqrt();
            let d2 = R_MIN * R_MIN - (rho - R_MAJ) * (rho - R_MAJ);
            if d2 > 0.0 {
                bite += 2.0 * d2.sqrt() * hx * hy;
            }
        }
    }
    let exact = 2.0 * PI * PI * R_MAJ * R_MIN * R_MIN - bite;
    let vol = mesh_signed_volume(&mesh);
    assert!(
        (vol - exact).abs() <= 0.02 * exact,
        "slotted torus volume {vol} vs analytic {exact} (bite {bite})"
    );
}
