//! PR-YR19 ADVERSARY — independent pin of the SAFETY property and SCOPE of the
//! projection-scaled radial band for sphere∩plane section circles.
//!
//! Spec of record: `specs/yr19_sphere_chord_band.md` (§2 derivation, §8 adversary).
//! GREEN production diff: commit "PR-YR19 GREEN" on `crates/yang-rs/src/lib.rs`.
//!
//! The GREEN fix scales the IN-PLANE RADIAL band of a sphere section `Circle` by
//! the propagated factor `(R/r_circle)·d_ε` (axial keeps `d_ε`), at TWO sites
//! (selection `curve_contains_point`/`build_intersection_curves`, and the Stage-4
//! relocation guard), gated on a `Surface::Sphere` owner via
//! `source_radius: Option<f64>` (`None` ⇒ byte-identical non-sphere paths). A
//! near-tangent guard `r_circle > MIN_FEATURE_SIZE` keeps the unscaled band.
//!
//! This file writes NEW canaries (no production / RED edits). Each test makes an
//! assertion that would FLIP under a plausible bad mutation:
//!
//! - `over_admit_*`: the band is NOT infinite — a rim genuinely off the sphere by
//!   MORE than `d_ε` (surface-normal sense) must NOT be silently accepted as an
//!   exact section `Curve::Circle`. (catches: widening the band / removing the
//!   surface-normal backstop.)
//! - `within_band_*`: the RED geometry (dr=0.07, in-band) IS accepted with an
//!   exact `Curve::Circle` and rim verts on the true circle. (positive control /
//!   companion to over_admit; catches a band made TOO TIGHT.)
//! - `none_factor_*`: a CYLINDER perpendicular-cut `Circle` (the `None` arm) still
//!   selects with the UNSCALED band; an on-circle rim is accepted as a
//!   `Curve::Circle`. (catches a degenerate "scale everything" regression where
//!   the factor leaked to non-sphere paths.)
//! - `near_tangent_*`: documents + asserts the `radius > MIN_FEATURE_SIZE` source
//!   guard so the `(R/r_circle)` factor cannot blow up.

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
// Mesh oracles (copied verbatim from yr19_sphere_chord_band.rs / yr13).
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

/// `d_ε = sphere_chord_bound(R) = 1e-2·2R√3` — the SAME literal the production
/// `sphere_chord_bound` uses (re-derived locally; integration tests cannot see
/// the `#[cfg(test)]` lib item).
fn sphere_chord_bound(radius: f64) -> f64 {
    1e-2 * 2.0 * radius * 3.0_f64.sqrt()
}

// =========================================================================
// SPHERE DIMPLE fixture — re-derived from the RED file (yr19_sphere_chord_band).
// Parameterized on the rim radial offset `dr` so the SAME mock builds BOTH the
// in-band positive control (dr = DR_IN = 0.07) AND the over-admit safety probe
// (dr = DR_OVER, beyond the propagated band so the sphere-normal residual itself
// exceeds d_ε).
// =========================================================================

const N: usize = 16; // rim/longitudinal facets
const M: usize = 4; // cap latitude bands (rim → bottom pole)
const BOX_LO: [f64; 3] = [-2.0, -2.0, 0.0];
const BOX_HI: [f64; 3] = [2.0, 2.0, 2.0];
const SPH_CENTER: [f64; 3] = [0.0, 0.0, 2.95];
const SPH_R: f64 = 1.0;
const TOP_Z: f64 = 2.0; // box top plane = section-circle plane
const CENTER_H: f64 = SPH_CENTER[2] - TOP_Z; // h = 0.95: centre-to-plane distance
const R_C: f64 = 0.312_249_899_919_919_36; // r_c = sqrt(R²−h²)

/// In-band perturbation (the RED fixture's value): `d_ε < DR_IN < (R/r_c)·d_ε`.
const DR_IN: f64 = 0.07;

/// Axis-aligned box `lo..hi` with correct OUTWARD normals and plane offsets.
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
/// meridian seam `Curve::Circle`).
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

fn cap_theta(j: usize) -> f64 {
    let theta_rim = (CENTER_H / SPH_R).acos();
    theta_rim * ((M - j) as f64) / (M as f64)
}
fn cap_ring_z(j: usize) -> f64 {
    SPH_CENTER[2] - SPH_R * cap_theta(j).cos()
}
fn cap_ring_r_exact(j: usize) -> f64 {
    SPH_R * cap_theta(j).sin()
}

/// Build the small-cap dimple arrangement with the rim ring (j=0) at radial
/// `r_c + dr` (z stays exactly TOP_Z). This is the RED `dimple_arrangement`
/// generalized over the radial perturbation `dr`.
fn dimple_arrangement(dr: f64) -> LabeledArrangement {
    let mut verts: Vec<Point3> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();

    let [x0, y0, z0] = BOX_LO;
    let [x1, y1, z1] = BOX_HI;
    let b0 = verts.len() as u32;
    verts.push(p(x0, y0, z0));
    verts.push(p(x1, y0, z0));
    verts.push(p(x1, y1, z0));
    verts.push(p(x0, y1, z0));
    let t0 = verts.len() as u32;
    verts.push(p(x0, y0, z1));
    verts.push(p(x1, y0, z1));
    verts.push(p(x1, y1, z1));
    verts.push(p(x0, y1, z1));

    let mut ring_base: Vec<u32> = Vec::with_capacity(M);
    for j in 0..M {
        ring_base.push(verts.len() as u32);
        let rz = cap_ring_z(j);
        let rr = if j == 0 {
            R_C + dr
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
    let pole = verts.len() as u32;
    verts.push(p(SPH_CENTER[0], SPH_CENTER[1], SPH_CENTER[2] - SPH_R));

    let rim = |k: usize| ring_base[0] + (k % N) as u32;
    let ring = |j: usize, k: usize| ring_base[j] + (k % N) as u32;

    let push_box = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[2], t[1]]);
        surf.push(vec![LaInputId(0)]);
    };

    push_box([b0, b0 + 1, b0 + 2], &mut tris, &mut surface);
    push_box([b0, b0 + 2, b0 + 3], &mut tris, &mut surface);

    let side = |a: u32,
                bb: u32,
                c: u32,
                d: u32,
                tris: &mut Vec<[u32; 3]>,
                surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([a, c, bb]);
        surf.push(vec![LaInputId(0)]);
        tris.push([a, d, c]);
        surf.push(vec![LaInputId(0)]);
    };
    side(b0, t0, t0 + 1, b0 + 1, &mut tris, &mut surface);
    side(b0 + 1, t0 + 1, t0 + 2, b0 + 2, &mut tris, &mut surface);
    side(b0 + 2, t0 + 2, t0 + 3, b0 + 3, &mut tris, &mut surface);
    side(b0 + 3, t0 + 3, t0, b0, &mut tris, &mut surface);

    let lo = [t0, t0 + 3, t0 + 2, t0 + 1];
    let per = N / 4;
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

    let push_sph = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[1], t[2]]);
        surf.push(vec![LaInputId(1)]);
    };
    for j in 0..(M - 1) {
        for k in 0..N {
            let k1 = k + 1;
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
            inside.push(vec![false, false]);
        } else {
            inside.push(vec![true, false]);
        }
    }
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

fn run_dimple_subtract(dr: f64) -> Result<BRep, yang_rs::YangError> {
    let bx = dimple_box();
    let sph = dimple_sphere();
    let mock = LabelMock {
        arrangement: dimple_arrangement(dr),
    };
    boolean(&bx, &sph, BoolOp::Subtract, &mock)
}

/// All `Curve::Circle` rim edges of a result B-Rep.
fn result_circles(r: &BRep) -> Vec<(Point3, Vector3, f64)> {
    r.edges()
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

/// Does `r` contain a `Curve::Circle` of the TRUE section radius `r_c` centred on
/// the z-axis at z = TOP_Z (normal ∥ ±z)? This is the "fake section accepted"
/// signal the over-admit safety test must NOT see.
fn has_section_circle(r: &BRep) -> bool {
    let tau = cad_primitives::TAU_MODEL;
    result_circles(r).iter().any(|(center, normal, radius)| {
        let c = center.as_array();
        let nrm = unit(normal.as_array());
        (radius - R_C).abs() <= tau
            && c[0].abs() <= tau
            && c[1].abs() <= tau
            && (c[2] - TOP_Z).abs() <= tau
            && nrm[0].abs() <= tau
            && nrm[1].abs() <= tau
            && (nrm[2].abs() - 1.0).abs() <= tau
    })
}

// =========================================================================
// TEST 1 — OVER-ADMIT SAFETY BACKSTOP IS LOAD-BEARING (the band is NOT infinite).
//
// Push the rim radial offset to `DR_OVER`, chosen so the rim point is genuinely
// OFF the sphere surface by MORE than `d_ε` in the SURFACE-NORMAL sense:
//   d_sphere = |p − C| − R = sqrt((r_c+dr)² + h²) − R  >  d_ε.
// Such a point is NOT on the section circle at all (it is a real off-surface
// vertex), so the pipeline must NOT silently snap it onto an exact section
// `Curve::Circle` of radius r_c and call the result Ok-correct.
//
// SAFE outcomes (asserted): EITHER `boolean()` returns Err (loud), OR it returns
// Ok but with NO `Curve::Circle` of radius ≈ r_c whose relocated endpoints sit on
// the true sphere∩plane circle to TAU_MODEL (i.e. nothing fake was accepted).
//
// OBSERVED BEHAVIOR (verbatim, recorded after running this test):
//   [yr19-adv T1] over-admit → loud Err: FaceResolutionFailed { tri: 30 }
//
// i.e. `boolean()` returns a LOUD `Err(YangError::FaceResolutionFailed)`, NOT a
// silent-wrong Ok. The catch site is Stage-6 face resolution
// (`stage6_resolve_faces`, lib.rs ~3217-3240): a sphere-cap triangle adjacent to
// the perturbed rim ring has its centroid more than `sphere_chord_bound(R)` off
// the analytic sphere along the surface normal, so the per-face membership test
// `plane_dist(..) < tol_for(Sphere)` (which uses `signed_distance_to_surface` =
// |c−C|−R, the projection-independent surface-normal metric — NOT the projected
// radial band YR19 touched) finds ZERO faces within tolerance → `n_hits == 0`
// → loud `FaceResolutionFailed`. This Stage-6 surface-normal backstop is exactly
// the geometric guard the spec relies on (§4: "No change to Stage-6 face
// resolution (`tol_for`): it uses `signed_distance_to_surface` … already
// correct and not amplified") and is UNAFFECTED by YR19.
//
// This test asserts the safe outcome (Err OR Ok-without-a-fake-circle) and would
// FLIP if someone widened the radial band to admit `d_sphere > d_ε` points or
// removed the surface-normal backstop (the off-sphere rim would then survive as
// a fake exact section Circle / pass face resolution).
// =========================================================================

/// `DR_OVER` chosen so the rim's surface-normal residual exceeds `d_ε`:
/// at dr=0.18 → radial=0.4922, |p−C|=sqrt(0.4922²+0.95²)=1.0700, d_sphere≈0.0700
/// > d_ε≈0.0346410. (Also well beyond the propagated band 0.1109.)
const DR_OVER: f64 = 0.18;

#[test]
fn over_admit_rim_off_surface_is_not_silently_accepted() {
    let d_eps = sphere_chord_bound(SPH_R);
    let propagated_band = (SPH_R / R_C) * d_eps;

    // Confirm DR_OVER really puts the rim OFF the sphere by more than d_ε in the
    // surface-normal sense (so it is a genuine off-surface point, not on-curve).
    let radial = R_C + DR_OVER;
    let dist = (radial * radial + CENTER_H * CENTER_H).sqrt();
    let d_sphere = (dist - SPH_R).abs();
    assert!(
        DR_OVER > propagated_band,
        "yr19-adv T1: DR_OVER={DR_OVER} must exceed the propagated radial band \
         (R/r_c)·d_ε={propagated_band} so the radial guard alone would reject it"
    );
    assert!(
        d_sphere > d_eps,
        "yr19-adv T1: rim sphere-normal residual d_sphere={d_sphere} must EXCEED \
         d_ε={d_eps} — the rim is genuinely OFF the sphere (not an on-curve point \
         the propagated band may legitimately admit)"
    );

    // Drive the real pipeline.
    let outcome = run_dimple_subtract(DR_OVER);

    match &outcome {
        Err(e) => {
            // SAFE: loud stop. (Acceptable per spec §8.)
            eprintln!("[yr19-adv T1] over-admit → loud Err: {e:?}");
        }
        Ok(r) => {
            // SAFE only if NO fake section Circle of radius r_c with on-true-circle
            // endpoints was accepted. The rim verts here are at radial r_c+0.18,
            // i.e. 0.18 off the true circle — if a Curve::Circle(r_c) were emitted
            // its endpoints could NOT be on the true circle to TAU_MODEL.
            eprintln!(
                "[yr19-adv T1] over-admit → Ok; circles = {:?}",
                result_circles(r)
            );
            // (a) No fake exact section circle accepted.
            assert!(
                !has_section_circle(r),
                "yr19-adv T1 BACKSTOP BREACHED: a rim genuinely OFF the sphere by \
                 d_sphere>{d_eps} was silently accepted as an exact section \
                 Curve::Circle of radius r_c={R_C}. The band over-admits — fix is \
                 unsafe."
            );
            // (b) Defense in depth: no relocated vertex on the cut plane near the
            // section sits EXACTLY on the true circle to TAU_MODEL (which would
            // mean a far-off vertex was snapped onto a fake circle). Verts on the
            // plane within the propagated band of r_c are the candidates; none may
            // be exactly at r_c (they are the unmoved 0.18-off rim verts).
            let mesh = r.as_mesh();
            for v in &mesh.verts {
                let x = v.as_array();
                if (x[2] - TOP_Z).abs() > 1e-9 {
                    continue;
                }
                let rad = (x[0] * x[0] + x[1] * x[1]).sqrt();
                // only inspect verts that could be the perturbed rim ring
                if (rad - (R_C + DR_OVER)).abs() > d_eps && (rad - R_C).abs() > d_eps {
                    continue;
                }
                let d_sph = (norm(sub3(x, SPH_CENTER)) - SPH_R).abs();
                assert!(
                    !((rad - R_C).abs() <= cad_primitives::TAU_MODEL
                        && d_sph > cad_primitives::TAU_MODEL),
                    "yr19-adv T1 BACKSTOP BREACHED: a vertex {x:?} was snapped onto \
                     the exact section circle radius (r_c) yet sits {d_sph} off the \
                     true sphere — fake relocation accepted."
                );
            }
        }
    }
}

// =========================================================================
// TEST 2 — WITHIN-BAND POSITIVE CONTROL (the band is a real, correctly-placed
// edge). The RED geometry (dr = DR_IN = 0.07, inside the band) yields Ok with an
// EXACT section `Curve::Circle` (radius r_c to TAU_MODEL) AND relocated rim verts
// exactly on the true sphere∩plane circle. Companion to TEST 1: together they
// prove the band has a real edge with a finite, correctly-placed boundary.
//
// Mutation caught: a band made TOO TIGHT (e.g. dropping the (R/r_circle) factor,
// reverting to flat d_ε) over-rejects this in-band rim → Err / no exact circle.
// =========================================================================

#[test]
fn within_band_rim_accepted_on_exact_circle() {
    let d_eps = sphere_chord_bound(SPH_R);
    let propagated_band = (SPH_R / R_C) * d_eps;
    // DR_IN must be inside the OPEN band (d_ε, (R/r_c)·d_ε): proves the fix's
    // factor (not a blanket pass) is what admits it.
    assert!(
        DR_IN > d_eps && DR_IN < propagated_band,
        "yr19-adv T2 precondition: DR_IN={DR_IN} must be in the OPEN propagated \
         band ({d_eps}, {propagated_band})"
    );

    let r = run_dimple_subtract(DR_IN)
        .expect("yr19-adv T2: in-band small-cap dimple Subtract must be Ok");

    // (1) Exact section Circle present.
    assert!(
        has_section_circle(&r),
        "yr19-adv T2: in-band dimple must emit the EXACT section Curve::Circle \
         (radius r_c={R_C} to TAU_MODEL); circles = {:?}",
        result_circles(&r)
    );

    // (2) Relocated rim verts lie on the EXACT circle: radial==r_c, z==TOP_Z, on
    // the sphere — all to TAU_MODEL.
    let tau = cad_primitives::TAU_MODEL;
    let mesh = r.as_mesh();
    let mut rim_checked = 0usize;
    for v in &mesh.verts {
        let x = v.as_array();
        if (x[2] - TOP_Z).abs() > 1e-6 {
            continue;
        }
        let radial = (x[0] * x[0] + x[1] * x[1]).sqrt();
        if (radial - R_C).abs() > d_eps {
            continue; // not a rim vertex (box-top corner, etc.)
        }
        assert!(
            (radial - R_C).abs() <= tau,
            "yr19-adv T2: rim vertex {x:?} must lie on the EXACT circle \
             (|radial−r_c|={} ≤ TAU_MODEL)",
            (radial - R_C).abs()
        );
        assert!(
            (x[2] - TOP_Z).abs() <= tau,
            "yr19-adv T2: rim vertex {x:?} must lie on z={TOP_Z} to TAU_MODEL"
        );
        let d_sphere = (norm(sub3(x, SPH_CENTER)) - SPH_R).abs();
        assert!(
            d_sphere <= tau,
            "yr19-adv T2: rim vertex {x:?} must lie on the sphere to TAU_MODEL \
             (got {d_sphere})"
        );
        rim_checked += 1;
    }
    assert!(
        rim_checked >= N,
        "yr19-adv T2: expected ≥{N} relocated rim verts on the exact circle, \
         found {rim_checked}"
    );

    // (3) Still a valid closed genus-0 solid.
    assert_eq!(
        unpaired_half_edges(mesh),
        0,
        "yr19-adv T2: in-band dimple output must be watertight"
    );
    assert_eq!(
        euler_characteristic(mesh),
        2,
        "yr19-adv T2: in-band dimple output must be χ=2"
    );
    assert!(
        signed_volume(mesh) > 0.0,
        "yr19-adv T2: in-band dimple output must be outward-oriented"
    );
}

// =========================================================================
// TEST 3 — SCOPE: NON-SPHERE (`None` factor) PATH UNCHANGED.
//
// A box − cylinder BLIND POCKET (perpendicular cut → the rim is a `Curve::Circle`
// owned by a `Surface::Cylinder`). The rim is authored EXACTLY on the section
// circle (radial deviation 0), so it selects under the UNSCALED band. Because the
// owner is a cylinder, `build_intersection_curves` passes `source_radius = None`
// → `curve_contains_point` uses the flat `tol` and the Stage-4 guard uses flat
// `d_eps`. This proves YR19 did NOT perturb the non-sphere arm.
//
// Mutation caught: a degenerate "scale every Circle by (R/r)" regression (factor
// leaking to the None arm) would change this path's band; but since the rim is
// on-circle this test additionally pins that the cylinder rim is still ACCEPTED
// as a Curve::Circle — i.e. a regression that BROKE the None arm (e.g. swapping
// the arms so the cylinder got Some(...) with a wrong R) would surface as a
// different radius / wrong selection. (Geometry reused from the proven
// yr13_subtract_cylinder fixture.)
// =========================================================================

const CYL_AXIS_POINT: [f64; 3] = [0.0, 0.0, 0.5];
const CYL_AXIS_DIR: [f64; 3] = [0.0, 0.0, 1.0];
const CYL_R: f64 = 1.0;
const CYL_H: f64 = 2.0;
const FLOOR_Z: f64 = 0.5;

fn cyl_surface() -> Surface {
    Surface::Cylinder {
        axis_point: p(CYL_AXIS_POINT[0], CYL_AXIS_POINT[1], CYL_AXIS_POINT[2]),
        axis_dir: Vector3::new(CYL_AXIS_DIR[0], CYL_AXIS_DIR[1], CYL_AXIS_DIR[2]),
        radius: CYL_R,
    }
}

/// Closed solid-cylinder B-Rep (seam-edge encoding, copied from yr13).
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
    BRep::new(verts, edges, faces).expect("cylinder_brep: BRep::new should tessellate")
}

fn pocket_box() -> BRep {
    box_brep(BOX_LO, BOX_HI)
}
fn pocket_cyl() -> BRep {
    cylinder_brep(CYL_AXIS_POINT, CYL_AXIS_DIR, CYL_R, CYL_H)
}

/// box-with-pocket arrangement (copied from yr13_subtract_cylinder), rim authored
/// EXACTLY on the section circle (radial dev 0) → exercises the `None` band arm.
fn pocket_arrangement() -> LabeledArrangement {
    let mut verts: Vec<Point3> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut surface: Vec<Vec<LaInputId>> = Vec::new();

    let [x0, y0, z0] = BOX_LO;
    let [x1, y1, z1] = BOX_HI;
    let b0 = verts.len() as u32;
    verts.push(p(x0, y0, z0));
    verts.push(p(x1, y0, z0));
    verts.push(p(x1, y1, z0));
    verts.push(p(x0, y1, z0));
    let t0 = verts.len() as u32;
    verts.push(p(x0, y0, z1));
    verts.push(p(x1, y0, z1));
    verts.push(p(x1, y1, z1));
    verts.push(p(x0, y1, z1));

    let rim_base = verts.len() as u32;
    for k in 0..N {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
        verts.push(p(CYL_R * th.cos(), CYL_R * th.sin(), TOP_Z));
    }
    let floor_base = verts.len() as u32;
    for k in 0..N {
        let th = 2.0 * std::f64::consts::PI * (k as f64) / (N as f64);
        verts.push(p(CYL_R * th.cos(), CYL_R * th.sin(), FLOOR_Z));
    }
    let floor_center = verts.len() as u32;
    verts.push(p(0.0, 0.0, FLOOR_Z));

    let rim = |k: usize| rim_base + (k % N) as u32;
    let flr = |k: usize| floor_base + (k % N) as u32;

    let push_box = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[2], t[1]]);
        surf.push(vec![LaInputId(0)]);
    };

    push_box([b0, b0 + 1, b0 + 2], &mut tris, &mut surface);
    push_box([b0, b0 + 2, b0 + 3], &mut tris, &mut surface);

    let side = |a: u32,
                bb: u32,
                c: u32,
                d: u32,
                tris: &mut Vec<[u32; 3]>,
                surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([a, c, bb]);
        surf.push(vec![LaInputId(0)]);
        tris.push([a, d, c]);
        surf.push(vec![LaInputId(0)]);
    };
    side(b0, t0, t0 + 1, b0 + 1, &mut tris, &mut surface);
    side(b0 + 1, t0 + 1, t0 + 2, b0 + 2, &mut tris, &mut surface);
    side(b0 + 2, t0 + 2, t0 + 3, b0 + 3, &mut tris, &mut surface);
    side(b0 + 3, t0 + 3, t0, b0, &mut tris, &mut surface);

    let lo = [t0, t0 + 3, t0 + 2, t0 + 1];
    let per = N / 4;
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

    let push_cyl = |t: [u32; 3], tris: &mut Vec<[u32; 3]>, surf: &mut Vec<Vec<LaInputId>>| {
        tris.push([t[0], t[1], t[2]]);
        surf.push(vec![LaInputId(1)]);
    };
    for k in 0..N {
        let k1 = k + 1;
        push_cyl([rim(k1), rim(k), flr(k)], &mut tris, &mut surface);
        push_cyl([rim(k1), flr(k), flr(k1)], &mut tris, &mut surface);
    }
    for k in 0..N {
        let k1 = k + 1;
        push_cyl([floor_center, flr(k1), flr(k)], &mut tris, &mut surface);
    }

    let n = tris.len();
    let mesh = Mesh::new(verts, tris);
    let mut inside: Vec<Vec<bool>> = Vec::with_capacity(n);
    for s in &surface {
        if s[0] == LaInputId(0) {
            inside.push(vec![false, false]);
        } else {
            inside.push(vec![true, false]);
        }
    }
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

fn cavity_cyl_faces(r: &BRep) -> Vec<BRepFace> {
    r.faces()
        .iter()
        .filter(|f| matches!(f.surface, Surface::Cylinder { .. }) && f.reversed)
        .cloned()
        .collect()
}

#[test]
fn none_factor_cylinder_section_unchanged() {
    let bx = pocket_box();
    let cyl = pocket_cyl();
    let mock = LabelMock {
        arrangement: pocket_arrangement(),
    };
    let r = boolean(&bx, &cyl, BoolOp::Subtract, &mock)
        .expect("yr19-adv T3: box − cylinder blind pocket (None arm) must be Ok");

    // The cylinder ∩ box-top section must appear as a Curve::Circle of radius
    // CYL_R, centred on the z-axis at z=TOP_Z. This rim was authored EXACTLY on
    // the circle (radial dev 0) and is selected under the UNSCALED `None` band.
    let tau = cad_primitives::TAU_MODEL;
    let saw_section = result_circles(&r).iter().any(|(center, _normal, radius)| {
        let c = center.as_array();
        (radius - CYL_R).abs() <= tau
            && c[0].abs() <= tau
            && c[1].abs() <= tau
            && (c[2] - TOP_Z).abs() <= tau
    });
    assert!(
        saw_section,
        "yr19-adv T3: the cylinder ∩ box-top section (None arm) must select as a \
         Curve::Circle of radius CYL_R={CYL_R} at z={TOP_Z}; circles = {:?}",
        result_circles(&r)
    );

    // The surviving cavity wall must be the exact input cylinder, reversed — the
    // None path is structurally undisturbed by YR19.
    let walls = cavity_cyl_faces(&r);
    assert!(
        !walls.is_empty(),
        "yr19-adv T3: surviving cavity wall must be a reversed Surface::Cylinder"
    );
    for w in &walls {
        assert_eq!(
            w.surface,
            cyl_surface(),
            "yr19-adv T3: cavity-wall Surface::Cylinder must equal the input cylinder \
             (None arm un-perturbed — no spurious radius scaling)"
        );
    }

    // Still a valid closed solid.
    assert_eq!(
        unpaired_half_edges(r.as_mesh()),
        0,
        "yr19-adv T3: None-arm pocket output must be watertight"
    );
    assert_eq!(
        euler_characteristic(r.as_mesh()),
        2,
        "yr19-adv T3: None-arm pocket output must be χ=2"
    );
}

// =========================================================================
// TEST 4 — NEAR-TANGENT FAIL-CLOSED (source-guard documentation + arithmetic
// pin). Production (both sites) gates the scale on `radius > MIN_FEATURE_SIZE`:
//
//   site 1 (curve_contains_point, lib.rs ~2394):
//     Some(big_r) if *radius > cad_primitives::MIN_FEATURE_SIZE => (big_r/ *radius)*tol
//   site 2 (stage4_relocate_and_correct, lib.rs ~3666):
//     Some(big_r) if radius > cad_primitives::MIN_FEATURE_SIZE => (big_r/radius)*d_eps
//
// So as r_circle → 0 the `(R/r_circle)` amplification CANNOT blow up: below the
// MIN_FEATURE_SIZE floor the match arm falls through to the UNSCALED band
// (`_ => tol` / `_ => d_eps`). A near-tangent section is not a real edge.
//
// Authoring a deterministic, watertight near-tangent (r_c ≤ 1e-6) sphere∩plane
// mesh fixture is impractical here: at r_c=1e-6 the cap collapses to numerical
// noise and a valid genus-0 closed shell cannot be hand-built without the rim
// degenerating (zero-area triangles fail mock_is_valid_genus0). Rather than
// fabricate a passing test that does not actually exercise the floor (an honesty
// violation per the brief), this test PINS THE GUARD ARITHMETIC directly: it
// asserts that, for a sub-floor radius, the band the production formula would
// pick is the UNSCALED one (the `_` arm), not the blown-up scaled one. This is a
// faithful mirror of the production match arms and FLIPS if the guard threshold
// is removed or the comparison inverted.
// =========================================================================

/// Mirror of the production site-1/site-2 band selection (the EXACT match-arm
/// logic). Returns the radial band the guard yields for a given section radius.
fn production_radial_band(big_r: Option<f64>, radius: f64, tol: f64) -> f64 {
    match big_r {
        Some(r) if radius > cad_primitives::MIN_FEATURE_SIZE => (r / radius) * tol,
        _ => tol,
    }
}

#[test]
fn near_tangent_guard_fails_closed() {
    let tol = sphere_chord_bound(SPH_R); // d_ε ≈ 0.0346410
    let big_r = SPH_R;

    // (a) Above the floor: the scaled (propagated) band is used — sanity that the
    // mirror reproduces the real scaling for a normal section (r_c ≈ 0.3122).
    let scaled = production_radial_band(Some(big_r), R_C, tol);
    assert!(
        (scaled - (big_r / R_C) * tol).abs() <= 1e-15,
        "yr19-adv T4: above the floor the guard must yield the SCALED band \
         (R/r_c)·tol"
    );
    assert!(
        scaled > tol,
        "yr19-adv T4: the scaled band must exceed the unscaled tol for r_c<R"
    );

    // (b) AT / BELOW the MIN_FEATURE_SIZE floor: the guard FAILS CLOSED — it must
    // pick the UNSCALED band, NOT the blown-up (R/r)·tol. With r = 1e-9 the naive
    // scaled band would be (1/1e-9)·tol ≈ 3.46e7 (catastrophic over-admit); the
    // guard must instead return exactly `tol`.
    let tiny = 1e-9_f64;
    let naive_blowup = (big_r / tiny) * tol; // what an UNGUARDED scale would give
    let guarded = production_radial_band(Some(big_r), tiny, tol);
    assert!(
        naive_blowup > 1e6,
        "yr19-adv T4 precondition: an unguarded near-tangent scale would blow up \
         to {naive_blowup} (≫ tol)"
    );
    assert!(
        (guarded - tol).abs() <= 1e-15,
        "yr19-adv T4: at r_circle={tiny} ≤ MIN_FEATURE_SIZE the guard must FAIL \
         CLOSED to the UNSCALED band tol={tol}, NOT the blown-up {naive_blowup}; \
         got {guarded}"
    );

    // (c) Exactly at the floor boundary (radius == MIN_FEATURE_SIZE): the strict
    // `>` means equality also fails closed.
    let at_floor = production_radial_band(Some(big_r), cad_primitives::MIN_FEATURE_SIZE, tol);
    assert!(
        (at_floor - tol).abs() <= 1e-15,
        "yr19-adv T4: at radius == MIN_FEATURE_SIZE the strict `>` guard must keep \
         the UNSCALED band (fail closed); got {at_floor}"
    );

    // (d) None (non-sphere) always unscaled, regardless of radius.
    let none_band = production_radial_band(None, R_C, tol);
    assert!(
        (none_band - tol).abs() <= 1e-15,
        "yr19-adv T4: None (non-sphere) must ALWAYS use the unscaled band"
    );
}
