//! PR-YR19 RED — sphere∩plane chord-band metric consistency. A box − sphere
//! SHALLOW SMALL-CAP DIMPLE, distinct from YR15's GREAT-CIRCLE dimple: here the
//! sphere centre sits ABOVE the box top so only a small spherical cap dips into
//! the box, carving a shallow dimple. The section circle is SMALL (`r_c ≪ R`),
//! so the amplification `R/r_c` is large (≈ 3.20).
//!
//! Spec of record: `specs/yr19_sphere_chord_band.md` (§2 derivation, §6 RED).
//!
//! Geometry:
//!   box A: axis-aligned [-2,-2,0] .. [2,2,2] (reused `box_brep`).
//!   sphere B: center (0,0,2.95), R = 1. The sphere spans z ∈ [1.95, 3.95]; only
//!     the small cap z ∈ [1.95, 2] dips into the box. `box − sphere` carves a
//!     shallow spherical dimple (depth 0.05) in the top face.
//!   cut plane = box top z = 2. `h = center_z − 2 = 0.95`.
//!   section circle: center (0,0,2), normal (0,0,1), radius
//!     `r_c = sqrt(R² − h²) = sqrt(0.0975) ≈ 0.3122499`; so `R/r_c ≈ 3.2026`.
//!   `d_ε = sphere_chord_bound(R) = 1e-2·2R√3 ≈ 0.0346410` (R=1). This is BOTH
//!     the selection `tol` AND the Stage-4 `d_eps` for this fixture.
//!
//! THE LOAD-BEARING PERTURBATION (the crux): the RIM ring (cap ring 0, the
//! sphere∩plane intersection-edge endpoints) sits ON the cut plane (z = 2 exact,
//! plane distance = 0) but at radial distance `radial = r_c + dr` from the
//! section-circle centre, with `dr = 0.07`. This `dr` satisfies ALL of:
//!   - `dr > d_ε` → the CURRENT flat radial metric over-rejects;
//!   - `dr < (R/r_c)·d_ε ≈ 0.1109` → the propagated band admits it post-fix;
//!   - `d_sphere = |p − C| − R ≈ 0.02402 ≤ d_ε` → passes the YR18 on-both gate.
//!
//! The mock asserts each of these in `band_is_load_bearing` so the magnitude is
//! self-documenting. All OTHER cap rings (j=1..M) and the bottom pole sit on the
//! EXACT sphere; only ring 0's radius is the deliberate radial offset.
//!
//! RED status: today `boolean()` raises `AmbiguousCurve` (sphere∩plane returns a
//! single section `Circle`; the relocated rim endpoints pass the YR18 on-both
//! gate via the surface-normal metric but fail `curve_contains_point` because the
//! in-plane radial deviation `dr` exceeds the flat `tol` — `matched == 0`). The
//! GREEN sub-agent threads the propagated band `(R/r_c)·d_ε` through BOTH the
//! selection membership (`curve_contains_point` / `build_intersection_curves`)
//! and the Stage-4 relocation guard (`stage4_relocate_and_correct`), per spec §4.
//! The mock self-check (`mock_is_valid_genus0`) makes NO `boolean()` call, so it
//! PASSES today, proving the fixture is a valid genus-0 closed shell before the
//! boolean oracles exercise the (not-yet-fixed) band path.
//!
//! Oracles (spec §6; FAIL today, PASS after the GREEN production fix):
//!  1. Output contains the exact section `Curve::Circle` rim edge: center ≈
//!     (0,0,2), normal ≈ ±(0,0,1), radius == r_c to `TAU_MODEL`.
//!  2. The relocated rim vertices lie on the EXACT circle to `TAU_MODEL`
//!     (radial == r_c, on the cut plane z=2, on the sphere |x−C| == R).
//!  3. Watertight 2-manifold, χ == 2, signed volume > 0, 0 unpaired half-edges.
//!  4. Determinism (two byte-identical runs) + env-gated sidecar parity
//!     (LOUD skip when `CHERCHI2022_BIN` unset; else watertight + χ=2 +
//!     reversed `Surface::Sphere` cavity wall).

use std::collections::{HashMap, HashSet};
use std::error::Error;

use cad_primitives::{BoolOp, Point3, Vector3};
use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
use cherchi_rs::{Mesh, MeshBoolean};
use yang_rs::{boolean, BRep, BRepEdge, BRepFace, BRepVertex, Curve, Surface};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// =========================================================================
// Pure-Rust array math (cad-primitives exposes only new/x/y/z/as_array).
// =========================================================================

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
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
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = norm(a);
    assert!(n > 0.0, "cannot normalize zero vector");
    scale(a, 1.0 / n)
}

// =========================================================================
// Mesh oracles (copied from yr15).
// =========================================================================

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

fn signed_volume(mesh: &Mesh) -> f64 {
    let mut acc = 0.0;
    for tri in &mesh.tris {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        let cx = b[1] * c[2] - b[2] * c[1];
        let cy = b[2] * c[0] - b[0] * c[2];
        let cz = b[0] * c[1] - b[1] * c[0];
        acc += a[0] * cx + a[1] * cy + a[2] * cz;
    }
    acc / 6.0
}

// =========================================================================
// Canonical config — a verified-closed box with a SHALLOW SMALL-CAP DIMPLE.
//   box A: axis-aligned [-2,-2,0] .. [2,2,2]
//   sphere B: center (0,0,2.95), radius 1. Only the cap z ∈ [1.95, 2] dips into
//     the box. `box − sphere` carves a shallow spherical dimple (depth 0.05) in
//     the top face. The rim is the SMALL section circle sphere ∩ box-top plane
//     (z=2, r=r_c≈0.3122, center (0,0,2)).
// =========================================================================

const N: usize = 16; // rim/longitudinal facets
const M: usize = 4; // cap latitude bands (rim → bottom pole)
const BOX_LO: [f64; 3] = [-2.0, -2.0, 0.0];
const BOX_HI: [f64; 3] = [2.0, 2.0, 2.0];
const SPH_CENTER: [f64; 3] = [0.0, 0.0, 2.95];
const SPH_R: f64 = 1.0;
const TOP_Z: f64 = 2.0; // box top plane = section-circle plane
const CENTER_H: f64 = SPH_CENTER[2] - TOP_Z; // h = 0.95: centre-to-plane distance
                                             // section circle radius r_c = sqrt(R² − h²) ≈ 0.3122499.
const R_C: f64 = 0.312_249_899_919_919_36;
// load-bearing radial perturbation of the rim ring (in-plane only; z stays = 2).
const DR: f64 = 0.07;

fn sph_surface() -> Surface {
    Surface::Sphere {
        center: p(SPH_CENTER[0], SPH_CENTER[1], SPH_CENTER[2]),
        radius: SPH_R,
    }
}

/// `d_ε = sphere_chord_bound(R) = 1e-2·2R√3` — the SAME literal the production
/// `sphere_chord_bound` uses (re-derived locally; integration tests cannot see
/// the `#[cfg(test)]` lib item).
fn sphere_chord_bound(radius: f64) -> f64 {
    1e-2 * 2.0 * radius * 3.0_f64.sqrt()
}

// =========================================================================
// Fixtures: box_brep (reused VERBATIM from yr15) and sphere_brep (the closed
// solid-sphere B-Rep). Integration tests cannot see #[cfg(test)] lib items, so
// these are local.
// =========================================================================

/// Axis-aligned box `lo..hi` with correct OUTWARD normals and plane offsets
/// (`n·x + d = 0`). All faces planar → `reversed: false`.
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
        [0, 1, 2, 3], // bottom (−z)
        [4, 7, 6, 5], // top (+z)
        [0, 4, 5, 1], // front (−y)
        [1, 5, 6, 2], // right (+x)
        [2, 6, 7, 3], // back (+y)
        [3, 7, 4, 0], // left (−x)
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
    // n·x + d = 0 ⇒ d = −n·(a point on the plane).
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
    BRep::new(verts, edges, faces).expect("box_brep: BRep::new failed")
}

/// Closed solid-sphere B-Rep (one `Surface::Sphere` face bounded by a single
/// meridian seam `Curve::Circle`). South pole `v0`, north pole `v1`.
fn sphere_brep(center: [f64; 3], radius: f64) -> BRep {
    let south = add(center, scale([0.0, 0.0, -1.0], radius));
    let north = add(center, scale([0.0, 0.0, 1.0], radius));

    let verts = vec![
        BRepVertex {
            point: p(south[0], south[1], south[2]),
        },
        BRepVertex {
            point: p(north[0], north[1], north[2]),
        },
    ];

    let edges = vec![BRepEdge {
        start: 0,
        end: 1,
        curve: Curve::Circle {
            center: p(center[0], center[1], center[2]),
            normal: Vector3::new(0.0, -1.0, 0.0),
            radius,
        },
    }];

    let faces = vec![BRepFace {
        surface: Surface::Sphere {
            center: p(center[0], center[1], center[2]),
            radius,
        },
        outer_loop: vec![0],
        inner_loops: Vec::new(),
        reversed: false,
    }];

    BRep::new(verts, edges, faces).expect("sphere_brep: BRep::new should tessellate the sphere")
}

fn dimple_box() -> BRep {
    box_brep(BOX_LO, BOX_HI)
}
fn dimple_sphere() -> BRep {
    sphere_brep(SPH_CENTER, SPH_R)
}

// =========================================================================
// Hand-built arrangement: FULL closed genus-0 result surface (box with a SHALLOW
// SMALL-CAP dimple), outward-from-result winding, N=16 longitudinal facets, M=4
// cap latitude bands. Verified watertight + χ=2 + positive volume (after the
// Subtract keep-set + flip_for_op) by the MANDATORY `mock_is_valid_genus0` self-
// check below.
//
// Box tris (label 0): surface=[A], inside=[false,false] (count 0) — kept by the
//   Subtract branch 1, NOT flipped.
// Sphere cap tris (label 1): surface=[B], inside=[true,false] (count 1) — kept
//   by the Subtract branch 2, FLIPPED by flip_for_op (swap tri[1]↔tri[2]).
//
// Cap latitude rings j=0..M measured by polar angle θ from the BOTTOM pole:
//   rim is at θ_rim = acos(h/R) = acos(0.95) ≈ 0.31756 rad; ring j (j=0..M) at
//   θ_j = θ_rim·(M−j)/M, giving z_j = center_z − R·cos(θ_j), r_j = R·sin(θ_j).
//   At j=0 (rim): z=2, r=r_c — but ring 0's radius is OVERRIDDEN to r_c+DR (the
//   load-bearing perturbation; z stays exactly 2). At j=M: z=1.95, r=0 (the
//   bottom pole, a single vertex). Rings 1..M-1 sit on the EXACT sphere.
//
// The box-top face becomes an ANNULUS with the rim ring (radius r_c+DR at z=2)
// as its hole — the SAME rim vertex indices are referenced by the box-top
// annulus inner ring and the cap's ring 0, so the relocation moves them once and
// both faces follow.
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

/// Cap latitude `j` polar angle θ (from the bottom pole): θ_j = θ_rim·(M−j)/M,
/// θ_rim = acos(h/R). j=0 is the rim (θ=θ_rim, z=TOP_Z); j=M is the bottom pole
/// (θ=0, z=center−R).
fn cap_theta(j: usize) -> f64 {
    let theta_rim = (CENTER_H / SPH_R).acos();
    theta_rim * ((M - j) as f64) / (M as f64)
}
fn cap_ring_z(j: usize) -> f64 {
    SPH_CENTER[2] - SPH_R * cap_theta(j).cos()
}
/// EXACT-sphere ring radius `R·sin(θ_j)`. Ring 0's radius is OVERRIDDEN to
/// `r_c + DR` by the caller (the load-bearing radial perturbation).
fn cap_ring_r_exact(j: usize) -> f64 {
    SPH_R * cap_theta(j).sin()
}

fn dimple_arrangement() -> LabeledArrangement {
    let mut verts: Vec<Point3> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();

    let [x0, y0, z0] = BOX_LO;
    let [x1, y1, z1] = BOX_HI;
    let b0 = verts.len() as u32;
    verts.push(p(x0, y0, z0)); // 0
    verts.push(p(x1, y0, z0)); // 1
    verts.push(p(x1, y1, z0)); // 2
    verts.push(p(x0, y1, z0)); // 3
    let t0 = verts.len() as u32;
    verts.push(p(x0, y0, z1)); // 4
    verts.push(p(x1, y0, z1)); // 5
    verts.push(p(x1, y1, z1)); // 6
    verts.push(p(x0, y1, z1)); // 7

    // Cap latitude rings j=0..M. Ring 0 (j=0) is the SECTION rim at z=TOP_Z, but
    // its radius is the PERTURBED `r_c + DR` (NOT the exact r_c) — the in-plane
    // load-bearing offset. Rings 1..M-1 sit on the EXACT sphere. Ring M is the
    // bottom pole (radius 0) — emitted as a single pole vertex, not a ring.
    let mut ring_base: Vec<u32> = Vec::with_capacity(M);
    for j in 0..M {
        ring_base.push(verts.len() as u32);
        let rz = cap_ring_z(j);
        let rr = if j == 0 {
            R_C + DR // PERTURBED rim radius (z stays exactly 2)
        } else {
            cap_ring_r_exact(j)
        };
        for k in 0..N {
            let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
            verts.push(p(
                SPH_CENTER[0] + rr * th.cos(),
                SPH_CENTER[1] + rr * th.sin(),
                rz,
            ));
        }
    }
    // Bottom pole (j=M): z = center_z − R = 1.95.
    let pole = verts.len() as u32;
    verts.push(p(SPH_CENTER[0], SPH_CENTER[1], SPH_CENTER[2] - SPH_R));

    // The rim ring IS cap ring 0.
    let rim = |k: usize| ring_base[0] + (k % N) as u32;
    let ring = |j: usize, k: usize| ring_base[j] + (k % N) as u32;

    // A real Cherchi arrangement is OUTWARD-oriented (positive signed volume).
    // We author each box face's triangles using the SAME geometric vertex
    // sequences as a CCW-from-outside box, then apply a single GLOBAL winding
    // reversal at the box emit closure (`push_box` swaps tri[1]↔tri[2] exactly
    // once) so the boolean OUTPUT comes out outward-oriented.
    let push_box = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[2], t[1]]); // global reversal → outward output
        surf.push(vec![LaInputId(0)]);
    };

    // === BOX BOTTOM (z=z0), outward −Z. Standard box face [0,1,2,3] winding.
    push_box([b0, b0 + 1, b0 + 2], &mut tris, &mut surface);
    push_box([b0, b0 + 2, b0 + 3], &mut tris, &mut surface);

    // === BOX 4 SIDES, outward horizontal (standard CCW-from-outside winding,
    // then globally reversed at emit).
    let side = |a: u32,
                bb: u32,
                c: u32,
                d: u32,
                tris: &mut Vec<[u32; 3]>,
                surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([a, c, bb]); // reversed [a,bb,c]
        surf.push(vec![LaInputId(0)]);
        tris.push([a, d, c]); // reversed [a,c,d]
        surf.push(vec![LaInputId(0)]);
    };
    side(b0, t0, t0 + 1, b0 + 1, &mut tris, &mut surface); // front −y
    side(b0 + 1, t0 + 1, t0 + 2, b0 + 2, &mut tris, &mut surface); // right +x
    side(b0 + 2, t0 + 2, t0 + 3, b0 + 3, &mut tris, &mut surface); // back +y
    side(b0 + 3, t0 + 3, t0, b0, &mut tris, &mut surface); // left −x

    // === BOX TOP ANNULUS (z=z1), outward +Z, with the section rim ring as its
    // hole. outer Lo=[t0,t0+3,t0+2,t0+1] (CW-from-above; edges oppose the side
    // faces); inner loop Li = rim DESCENDING (`li(s)=rim((N−s)%N)`) so the
    // outer-square cycle and the rim-ring hole wind in OPPOSITE rotational senses
    // (proper outer + hole). The inner-ring boundary edges run ASCENDING; the cap
    // therefore traverses the rim DESCENDING so the shared rim edges pair.
    let lo = [t0, t0 + 3, t0 + 2, t0 + 1];
    let per = N / 4; // 4 for N=16
    let li = |s: usize| rim((N - (s % N)) % N);
    for c in 0..4usize {
        let oa = lo[c];
        let ob = lo[(c + 1) % 4];
        let sa = c * per;
        let sb = (c + 1) * per;
        push_box([oa, ob, li(sb)], &mut tris, &mut surface);
        for s in (sa..sb).rev() {
            push_box([oa, li(s + 1), li(s)], &mut tris, &mut surface);
        }
    }

    // === SPHERE CAP (label 1) — rim ring (j=0, z=2) down to the bottom pole
    // (z=1.95). As a cavity wall the outward-from-result normal points TOWARD the
    // sphere CENTRE (UPWARD, into the dimple — the centre is ABOVE the box top).
    // The sphere/B tris are authored with the global reversal AND a pre-swap for
    // flip_for_op; the two swaps CANCEL, so the emit closure pushes the vertices
    // unswapped. flip_for_op(Subtract) then re-swaps these at compaction,
    // restoring their toward-centre winding (the SAME signal that sets
    // `reversed == true`).
    //
    // The top band's rim edges traverse the rim DESCENDING (rim(k+1)→rim(k)) —
    // opposite the box-top annulus inner ring (ASCENDING) so the shared rim edges
    // pair. Each band between ring j and ring j+1 is split into two triangles; the
    // final band (ring M-1 → pole) is a single triangle fan at the pole.
    let push_sph = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[1], t[2]]); // global-reversal ∘ pre-swap = identity
        surf.push(vec![LaInputId(1)]);
    };
    // Bands between full rings j and j+1 (for j = 0 .. M-2).
    for j in 0..(M - 1) {
        for k in 0..N {
            let k1 = k + 1;
            // toward-centre winding: upper ring (j) edges DESCENDING; lower ring
            // (j+1) edges ASCENDING so successive bands' shared edges pair.
            push_sph(
                [ring(j, k1), ring(j, k), ring(j + 1, k)],
                &mut tris,
                &mut surface,
            );
            push_sph(
                [ring(j, k1), ring(j + 1, k), ring(j + 1, k1)],
                &mut tris,
                &mut surface,
            );
        }
    }
    // Pole fan (ring M-1 → bottom pole).
    for k in 0..N {
        let k1 = k + 1;
        push_sph(
            [ring(M - 1, k1), ring(M - 1, k), pole],
            &mut tris,
            &mut surface,
        );
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let mut inside: Vec<Vec<bool>> = Vec::with_capacity(n);
    for s in &surface {
        if s[0] == LaInputId(0) {
            inside.push(vec![false, false]); // box surface: outside both
        } else {
            inside.push(vec![true, false]); // sphere cavity wall: inside A only
        }
    }
    let patch = vec![0u32; n];
    LabeledArrangement {
        mesh,
        surface,
        inside,
        patch,
        source: Vec::new(),
        num_inputs: 2,
    }
}

/// Simulate the Subtract keep-set + flip on the arrangement mesh: every triangle
/// is kept (box `inside` count 0; sphere `inside` count 1), so the output mesh is
/// the arrangement mesh with every LABEL-1 triangle's tri[1]/tri[2] swapped
/// (that is `flip_for_op` for Subtract on `InputId::B`). Used by the mandatory
/// `mock_is_valid_genus0` self-check (no `boolean()` call).
fn simulated_output_mesh(arr: &LabeledArrangement) -> Mesh {
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(arr.mesh.tris.len());
    for (i, tri) in arr.mesh.tris.iter().enumerate() {
        if arr.surface[i][0] == LaInputId(1) {
            tris.push([tri[0], tri[2], tri[1]]); // flip_for_op(Subtract) on B
        } else {
            tris.push(*tri);
        }
    }
    Mesh::new(arr.mesh.verts.clone(), tris)
}

fn run_subtract() -> BRep {
    let bx = dimple_box();
    let sph = dimple_sphere();
    let mock = LabelMock {
        arrangement: dimple_arrangement(),
    };
    boolean(&bx, &sph, BoolOp::Subtract, &mock)
        .expect("yr19: box − sphere SHALLOW SMALL-CAP DIMPLE Subtract must be Ok")
}

/// The surviving cavity-wall faces: `Surface::Sphere` with `reversed == true`.
fn cavity_wall_faces(r: &BRep) -> Vec<BRepFace> {
    r.faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Sphere { .. }) && f.reversed)
        .cloned()
        .collect()
}

// =========================================================================
// Band magnitude self-check — the perturbation `DR` is load-bearing: it lies in
// the OPEN band `(d_ε, (R/r_c)·d_ε)` AND the rim's sphere-normal residual is
// `≤ d_ε` (so it passes the YR18 on-both-surfaces gate). These assertions make
// the fixture self-documenting and guarantee it reproduces the metric bug.
// =========================================================================

#[test]
fn band_is_load_bearing() {
    let d_eps = sphere_chord_bound(SPH_R);
    let amplification = SPH_R / R_C;
    let propagated_band = amplification * d_eps;

    // r_c = sqrt(R² − h²); confirm the constant matches the derivation.
    let r_c_exact = (SPH_R * SPH_R - CENTER_H * CENTER_H).sqrt();
    assert!(
        (R_C - r_c_exact).abs() <= 1e-12,
        "yr19 band: R_C constant {R_C} must equal sqrt(R²−h²) = {r_c_exact}"
    );

    // (1) dr > d_ε: the CURRENT flat radial metric over-rejects the rim → today's
    //     AmbiguousCurve bug.
    assert!(
        DR > d_eps,
        "yr19 band: DR={DR} must EXCEED d_ε={d_eps} (so the flat radial metric \
         over-rejects today)"
    );
    // (2) dr < (R/r_c)·d_ε: the propagated band admits it post-fix.
    assert!(
        DR < propagated_band,
        "yr19 band: DR={DR} must be BELOW the propagated band (R/r_c)·d_ε = \
         {propagated_band} (so the §4 fix admits it)"
    );
    // (3) The actual sphere-normal residual of a rim vertex is ≤ d_ε (passes the
    //     YR18 on-both gate). Rim vertex p = (r_c+DR, 0, 2); |p − C| =
    //     sqrt((r_c+DR)² + h²).
    let radial = R_C + DR;
    let dist = (radial * radial + CENTER_H * CENTER_H).sqrt();
    let d_sphere = (dist - SPH_R).abs();
    assert!(
        d_sphere <= d_eps,
        "yr19 band: rim sphere-normal residual d_sphere={d_sphere} must be ≤ d_ε=\
         {d_eps} (so the rim passes the YR18 on-both-surfaces gate)"
    );

    // Documented magnitudes (so a future reader sees the band immediately):
    // d_ε ≈ 0.0346410, propagated ≈ 0.1109400, DR = 0.07, d_sphere ≈ 0.0240190,
    // R/r_c ≈ 3.2026.
    assert!(
        (d_eps - 0.034_641_016_151_377_5).abs() <= 1e-12,
        "yr19 band: d_ε drifted from the documented 0.0346410; got {d_eps}"
    );
    assert!(
        (propagated_band - 0.110_940_039_245_046).abs() <= 1e-9,
        "yr19 band: propagated band drifted from the documented 0.1109400; got \
         {propagated_band}"
    );
    assert!(
        (d_sphere - 0.024_019_035_950_401_1).abs() <= 1e-9,
        "yr19 band: d_sphere drifted from the documented 0.0240190; got {d_sphere}"
    );
}

// =========================================================================
// MANDATORY self-check — the authoritative fixture-validity gate. Builds the
// SIMULATED boolean output (keep-all + flip label-1) directly, NO boolean()
// call, and asserts the mock is a valid genus-0 closed shell: watertight, χ=2,
// outward-oriented. If this fails the whole RED test is meaningless, so the mock
// windings are iterated until it passes.
//
// This test PASSES today (no boolean() call → does not touch the band path);
// the boolean oracles below FAIL today (RED).
// =========================================================================

#[test]
fn mock_is_valid_genus0() {
    let arr = dimple_arrangement();
    let sim = simulated_output_mesh(&arr);

    let unpaired = unpaired_half_edges(&sim);
    assert_eq!(
        unpaired, 0,
        "yr19 self-check: simulated dimple output mesh must be watertight \
         (0 unpaired half-edges); got {unpaired}. Iterate the mock windings."
    );

    let chi = euler_characteristic(&sim);
    assert_eq!(
        chi, 2,
        "yr19 self-check: simulated small-cap-dimple output must be genus 0 \
         (χ=2); got χ={chi}. A dimpled box is still a topological sphere."
    );

    let vol = signed_volume(&sim);
    assert!(
        vol > 0.0,
        "yr19 self-check: simulated output must be OUTWARD-oriented (positive \
         signed volume); got {vol}. A negative volume means the mock is globally \
         inside-out."
    );
}

// =========================================================================
// Oracle 1 — exact section Circle rim (sphere ∩ box-top plane, SMALL circle).
//   center ≈ (0,0,2), normal ≈ ±(0,0,1), radius == r_c to TAU_MODEL.
// =========================================================================

#[test]
fn oracle1_section_circle_rim() {
    let r = run_subtract();

    let circles: Vec<(Point3, Vector3, f64)> = r
        .edges()
        .iter()
        .filter_map(|e| match e.curve {
            Curve::Circle {
                center,
                normal,
                radius,
            } => Some((center, normal, radius)),
            _ => None,
        })
        .collect();
    assert!(
        !circles.is_empty(),
        "yr19 O1: the sphere ∩ box-top section must appear as ≥1 Curve::Circle rim \
         edge; edges = {:?}",
        r.edges().iter().map(|e| &e.curve).collect::<Vec<_>>()
    );

    let tau = cad_primitives::TAU_MODEL;
    let mut saw_section = false;
    for (center, normal, radius) in &circles {
        let c = center.as_array();
        let nrm = unit(normal.as_array());
        // The section rim: radius == r_c, plane z=TOP_Z, normal ∥ ±(0,0,1).
        if (radius - R_C).abs() <= tau
            && (c[0]).abs() <= tau
            && (c[1]).abs() <= tau
            && (c[2] - TOP_Z).abs() <= tau
            && nrm[0].abs() <= tau
            && nrm[1].abs() <= tau
            && (nrm[2].abs() - 1.0).abs() <= tau
        {
            saw_section = true;
        }
    }
    assert!(
        saw_section,
        "yr19 O1: expected the SMALL section Circle (center ≈ (0,0,{TOP_Z}), \
         normal ≈ ±(0,0,1), radius == r_c={R_C}) to TAU_MODEL; circles = {circles:?}"
    );
}

// =========================================================================
// Oracle 2 — the relocated rim (intersection) vertices lie on the EXACT circle
// to TAU_MODEL: radial == r_c, on the cut plane z=TOP_Z, and on the sphere.
// Rim verts are identified geometrically: on the cut plane z≈TOP_Z AND radial ≈
// r_c after relocation.
// =========================================================================

#[test]
fn oracle2_relocated_rim_on_exact_circle() {
    let r = run_subtract();
    let mesh = r.as_mesh();
    let tau = cad_primitives::TAU_MODEL;
    let circle_center = SPH_CENTER; // (0,0,2.95) in xy = (0,0); the section uses (0,0,TOP_Z)

    let mut rim_checked = 0usize;
    for v in &mesh.verts {
        let x = v.as_array();
        // candidate rim vertex: on the cut plane z = TOP_Z AND in-plane radial
        // near r_c (post-relocation). Use a generous radial gate (within d_ε of
        // r_c) to SELECT candidates; then assert the EXACT circle to TAU_MODEL.
        let on_plane = (x[2] - TOP_Z).abs() <= 1e-6;
        if !on_plane {
            continue;
        }
        let radial = (x[0] * x[0] + x[1] * x[1]).sqrt();
        // section-circle centre is on the z-axis (xy = 0).
        let _ = circle_center;
        if (radial - R_C).abs() > sphere_chord_bound(SPH_R) {
            continue; // not a rim vertex (e.g. a box-top outer corner)
        }
        // EXACT circle: |radial − r_c| ≤ TAU_MODEL.
        assert!(
            (radial - R_C).abs() <= tau,
            "yr19 O2: rim vertex {x:?} must lie on the EXACT section circle \
             (|radial − r_c| = {} ≤ TAU_MODEL)",
            (radial - R_C).abs()
        );
        // on the cut plane z = TOP_Z (to TAU_MODEL).
        assert!(
            (x[2] - TOP_Z).abs() <= tau,
            "yr19 O2: rim vertex {x:?} must lie on the cut plane z={TOP_Z} \
             (offset {} ≤ TAU_MODEL)",
            (x[2] - TOP_Z).abs()
        );
        // on the sphere |x − C| = R (to TAU_MODEL).
        let d_sphere = (norm(sub3(x, SPH_CENTER)) - SPH_R).abs();
        assert!(
            d_sphere <= tau,
            "yr19 O2: rim vertex {x:?} must lie on the sphere \
             (||x−C|−R| = {d_sphere} ≤ TAU_MODEL)"
        );
        rim_checked += 1;
    }
    assert!(
        rim_checked >= N,
        "yr19 O2: expected to witness ≥{N} relocated rim vertices on the exact \
         circle, found {rim_checked}"
    );
}

// =========================================================================
// Oracle 3 — watertight 2-manifold, χ = 2, signed_volume > 0.
// =========================================================================

#[test]
fn oracle3_watertight_euler_two() {
    let r = run_subtract();
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr19 O3: dimple output mesh must be watertight (0 unpaired half-edges)"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr19 O3: small-cap-dimple output must be genus 0 (χ = 2)"
    );
    // Outward-oriented solid (not inside-out): POSITIVE signed volume (≈ box
    // 4×4×2 = 32 minus a SHALLOW cap, ≈ 32).
    let vol = signed_volume(r.as_mesh());
    assert!(
        vol > 0.0,
        "yr19 O3: result must be outward-oriented (positive signed volume), got {vol}"
    );
}

// =========================================================================
// Oracle 4 — determinism + env-gated sidecar parity (LOUD skip).
// =========================================================================

#[test]
fn oracle4_determinism_and_sidecar_parity() {
    // (a) Determinism: two run_subtract() runs must be byte-identical in verts,
    // tris, and per-face (surface, reversed).
    let r1 = run_subtract();
    let r2 = run_subtract();
    assert_eq!(
        r1.as_mesh().verts,
        r2.as_mesh().verts,
        "yr19 O4a: vertex set must be deterministic"
    );
    assert_eq!(
        r1.as_mesh().tris,
        r2.as_mesh().tris,
        "yr19 O4a: triangle set must be deterministic"
    );
    assert_eq!(
        r1.faces().len(),
        r2.faces().len(),
        "yr19 O4a: face count must be deterministic"
    );
    for (f1, f2) in r1.faces().iter().zip(r2.faces()) {
        assert_eq!(f1.surface, f2.surface, "yr19 O4a: face surface differs");
        assert_eq!(f1.reversed, f2.reversed, "yr19 O4a: face reversed differs");
    }

    // (b) Env-gated sidecar parity (LOUD skip when unset).
    let Some(sb) = yang_rs::native_backend() else {
        eprintln!("[yang-rs yr19] SKIP: native FFI shim not linked (stub build)");
        return;
    };
    let bx = dimple_box();
    let sph = dimple_sphere();
    let r = boolean(&bx, &sph, BoolOp::Subtract, &sb)
        .expect("yr19 O4b: sidecar-backed small-cap-dimple Subtract must be Ok");
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr19 O4b: sidecar-backed output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr19 O4b: sidecar-backed small-cap-dimple output must be χ = 2 (genus 0)"
    );
    assert!(
        !cavity_wall_faces(&r).is_empty(),
        "yr19 O4b: sidecar-backed output must carry a reversed Surface::Sphere cavity wall"
    );
    // The cavity wall must be the exact input sphere params.
    let want = sph_surface();
    for w in &cavity_wall_faces(&r) {
        assert_eq!(
            w.surface, want,
            "yr19 O4b: cavity-wall Surface::Sphere must equal the input sphere"
        );
    }
}
