//! N6 / §4.5.4 (task #173): PRODUCTION self-intersection gate for
//! boolean-path solids — the render-resolution layer of the two-layer
//! illegal-self-intersection detector (spec `specs/yang_173_selfx_detector.md`).
//!
//! ## Why render resolution, not the Stage-4 mesh
//!
//! The 2026-07-17 corpus measurement (spec §6) showed the two layers see
//! DISJOINT defect classes:
//! - The exact Stage-4 mesh test fires on relocation-minted seam chord
//!   crossings on 53 cases (33 CORRECT) — the §4.4/§4.5 artifacts whose
//!   paper remedy is REMOVAL by local refinement (#169 increment 2), not a
//!   STOP. It is banked as the `YANG_SELFX_PROBE` diagnostic in yang-rs.
//! - The C0116 class (B-Rep-level trimmed-surface penetration, ~5e-3 on a
//!   cyl×cyl graze) is SUB-SAGITTA in the coarse boolean mesh (sagitta
//!   ≈ 8.6e-3 at 12 segments) and only becomes observable where the true
//!   analytic surfaces are sampled finely — the render tessellation
//!   (sagitta = 1e-3·r).
//!
//! ## Calibration lineage
//!
//! The check is a semantics-identical port of the test-harness
//! `check_no_self_intersection` oracle (spec
//! `specs/inter_face_self_intersection_oracle.md`, PR-TH1 normalized
//! penetration depth, PR-KV11 vertex-adjacency skip), which every
//! SUPPORTED_CORRECT corpus case already passes on exactly this mesh (the
//! assay consumes the f32 cast of the mesh this gate checks in f64). Its
//! corpus-wide false-positive rate is therefore measured, not assumed.
//! Constants come from `waffle_types::kernel::units` — the same source the
//! oracle reads — so the two implementations cannot drift silently; the
//! oracle additionally remains in force in the assay as the differential
//! check on this port.
//!
//! The grazing band (`max_abs · TAU_WELD_MAX`, floored at
//! `TAU_COINCIDENT`) is a P10 SAFETY NET in the sanctioned direction only:
//! it can only convert a silent-wrong emission into a loud STOP, never
//! admit one (a false NEGATIVE below the band leaves behavior exactly as
//! it is today; there is no path where the band makes a wrong result
//! pass).

use waffle_types::kernel::units::{
    TAU_COINCIDENT, TAU_NORMALIZE_SQ, TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN, TAU_WELD_MAX,
};

use crate::arena::{BrepArena, FaceId, SolidId};
use crate::error::KernelV2Error;
use crate::tessellate::{tessellate, RenderMesh};

/// PRODUCTION gate: tessellate the boolean output solid at the canonical
/// render tolerance and reject it loudly if triangles from two different
/// faces penetrate each other beyond the grazing band. Called from the
/// boolean assembly boundary (`boolean::mod`), sibling of the F1 planarity
/// gate.
pub fn validate_boolean_output_self_intersection(
    arena: &BrepArena,
    solid: SolidId,
) -> Result<(), KernelV2Error> {
    // Timing is a native-only dev knob: `Instant::now` PANICS on
    // wasm32-unknown-unknown ("time not implemented"), so the clock may
    // only be read when the env var is set (env vars are always unset in
    // wasm). An unconditional `now()` here crashed every boolean in the
    // deployed app (2026-07-22).
    let timing = std::env::var_os("KV2_SELFX_TIME").is_some();
    let t0 = timing.then(std::time::Instant::now); // wasm-ok: env-gated
    let mesh = tessellate(arena, solid)?;
    let t_tess = t0.map(|t| t.elapsed());
    let t1 = timing.then(std::time::Instant::now); // wasm-ok: env-gated
    let hit = first_inter_face_penetration(&mesh);
    if let (Some(t_tess), Some(t1)) = (t_tess, t1) {
        eprintln!(
            "KV2_SELFX_TIME tris={} tess_ms={} scan_ms={}",
            mesh.indices.len() / 3,
            t_tess.as_millis(),
            t1.elapsed().as_millis()
        );
    }
    if let Some(v) = hit {
        return Err(KernelV2Error::SelfIntersectingBooleanOutput {
            face_a: v.face_a,
            face_b: v.face_b,
            penetrations: v.penetrations,
        });
    }
    Ok(())
}

/// First inter-face penetration found in a render mesh, if any.
pub(crate) struct Penetration {
    pub face_a: FaceId,
    pub face_b: FaceId,
    /// Penetrating triangle pairs across (`face_a`, `face_b`) — the first
    /// offending face pair only (the gate stops at the first bad pair of
    /// faces; per-pair enumeration is the assay oracle's job).
    pub penetrations: usize,
}

/// Port of the oracle's mesh scan: per-face triangle groups, face-pair
/// AABB broad-phase, quantized-shared-vertex adjacency skip (≥1 shared
/// quantized vertex ⇒ legitimate contact at a shared boundary/junction —
/// PR-KV11: curved-face chords legitimately dip below a neighbour by up to
/// the sagitta, pivoting on the shared vertex), then the normalized-depth
/// Möller test.
pub(crate) fn first_inter_face_penetration(mesh: &RenderMesh) -> Option<Penetration> {
    if mesh.face_ranges.len() <= 1 || mesh.indices.is_empty() {
        return None;
    }

    let max_abs = mesh.positions.iter().fold(0.0_f64, |m, &c| m.max(c.abs()));
    let grid_size = (max_abs * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid_size;
    let quantize = |c: f64| -> i64 { (c * inv_grid).round() as i64 };

    let vert_pos = |idx: u32| -> [f64; 3] {
        let i = idx as usize * 3;
        [
            mesh.positions[i],
            mesh.positions[i + 1],
            mesh.positions[i + 2],
        ]
    };
    let vert_quant = |idx: u32| -> (i64, i64, i64) {
        let p = vert_pos(idx);
        (quantize(p[0]), quantize(p[1]), quantize(p[2]))
    };

    let depth_threshold = (max_abs * TAU_WELD_MAX).max(TAU_COINCIDENT);

    // Per-triangle records, computed ONCE (the scan visits tri pairs
    // na·nb times per overlapping face pair — recomputing quantization or
    // AABBs per pair is what made the first cut O(43s) on a 47k-tri mesh;
    // see spec §8).
    struct Tri {
        pos: [[f64; 3]; 3],
        quant: [(i64, i64, i64); 3],
        aabb_min: [f64; 3],
        aabb_max: [f64; 3],
    }
    struct FaceGroup {
        face: FaceId,
        tris: Vec<Tri>,
        aabb_min: [f64; 3],
        aabb_max: [f64; 3],
    }
    let mut groups: Vec<FaceGroup> = Vec::with_capacity(mesh.face_ranges.len());
    for fr in &mesh.face_ranges {
        let start = fr.start as usize;
        let end = (fr.start + fr.count) as usize;
        let mut aabb_min = [f64::MAX; 3];
        let mut aabb_max = [f64::MIN; 3];
        let mut tris = Vec::with_capacity((end - start) / 3);
        for tri in mesh.indices[start..end].chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            let pos = [vert_pos(tri[0]), vert_pos(tri[1]), vert_pos(tri[2])];
            let quant = [vert_quant(tri[0]), vert_quant(tri[1]), vert_quant(tri[2])];
            let mut t_min = [f64::MAX; 3];
            let mut t_max = [f64::MIN; 3];
            for p in &pos {
                for d in 0..3 {
                    t_min[d] = t_min[d].min(p[d]);
                    t_max[d] = t_max[d].max(p[d]);
                    aabb_min[d] = aabb_min[d].min(p[d]);
                    aabb_max[d] = aabb_max[d].max(p[d]);
                }
            }
            tris.push(Tri {
                pos,
                quant,
                aabb_min: t_min,
                aabb_max: t_max,
            });
        }
        groups.push(FaceGroup {
            face: fr.face,
            tris,
            aabb_min,
            aabb_max,
        });
    }

    let boxes_overlap = |a_min: &[f64; 3], a_max: &[f64; 3], b_min: &[f64; 3], b_max: &[f64; 3]| {
        (0..3).all(|d| a_max[d] >= b_min[d] && b_max[d] >= a_min[d])
    };

    for i in 0..groups.len() {
        for j in (i + 1)..groups.len() {
            let (ga, gb) = (&groups[i], &groups[j]);
            if !boxes_overlap(&ga.aabb_min, &ga.aabb_max, &gb.aabb_min, &gb.aabb_max) {
                continue;
            }
            let mut penetrations = 0usize;
            for tri_a in &ga.tris {
                // A triangle outside the other FACE's box can't pair with
                // any of its triangles.
                if !boxes_overlap(&tri_a.aabb_min, &tri_a.aabb_max, &gb.aabb_min, &gb.aabb_max) {
                    continue;
                }
                for tri_b in &gb.tris {
                    // Penetrating pairs necessarily have overlapping
                    // AABBs — pure pruning, no semantics change.
                    if !boxes_overlap(
                        &tri_a.aabb_min,
                        &tri_a.aabb_max,
                        &tri_b.aabb_min,
                        &tri_b.aabb_max,
                    ) {
                        continue;
                    }
                    if tri_a.quant.iter().any(|v| tri_b.quant.contains(v)) {
                        continue;
                    }
                    if triangles_intersect(&tri_a.pos, &tri_b.pos, depth_threshold) {
                        penetrations += 1;
                    }
                }
            }
            if penetrations > 0 {
                return Some(Penetration {
                    face_a: ga.face,
                    face_b: gb.face,
                    penetrations,
                });
            }
        }
    }
    None
}

/// Möller-style triangle-triangle intersection with a normalized GEOMETRIC
/// penetration-depth guard (port of the oracle's `triangles_intersect`,
/// PR-TH1 semantics): a pair only counts as penetrating when EACH triangle
/// extends beyond `depth_threshold` on BOTH sides of the other's
/// supporting plane — contact confined within the band is grazing, not
/// penetration. Coplanar pairs are treated as non-intersecting.
///
/// Reference: Möller, "A Fast Triangle-Triangle Intersection Test",
/// JGT 2(2), 1997.
fn triangles_intersect(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3], depth_threshold: f64) -> bool {
    let cross = |u: [f64; 3], v: [f64; 3]| -> [f64; 3] {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let sub = |p: [f64; 3], q: [f64; 3]| -> [f64; 3] { [p[0] - q[0], p[1] - q[1], p[2] - q[2]] };
    let dot = |u: [f64; 3], v: [f64; 3]| -> f64 { u[0] * v[0] + u[1] * v[1] + u[2] * v[2] };
    let normalize = |n: [f64; 3]| -> Option<[f64; 3]> {
        let len = dot(n, n).sqrt();
        if len < TAU_NORMALIZE_SQ {
            None // degenerate triangle — no meaningful plane
        } else {
            Some([n[0] / len, n[1] / len, n[2] / len])
        }
    };

    let na = match normalize(cross(sub(a[1], a[0]), sub(a[2], a[0]))) {
        Some(n) => n,
        None => return false,
    };
    let da = dot(na, a[0]);
    let db: [f64; 3] = [dot(na, b[0]) - da, dot(na, b[1]) - da, dot(na, b[2]) - da];
    let db_min = db[0].min(db[1]).min(db[2]);
    let db_max = db[0].max(db[1]).max(db[2]);
    if db_min > -depth_threshold || db_max < depth_threshold {
        return false;
    }

    let nb = match normalize(cross(sub(b[1], b[0]), sub(b[2], b[0]))) {
        Some(n) => n,
        None => return false,
    };
    let d_b_plane = dot(nb, b[0]);
    let d_a: [f64; 3] = [
        dot(nb, a[0]) - d_b_plane,
        dot(nb, a[1]) - d_b_plane,
        dot(nb, a[2]) - d_b_plane,
    ];
    let da_min = d_a[0].min(d_a[1]).min(d_a[2]);
    let da_max = d_a[0].max(d_a[1]).max(d_a[2]);
    if da_min > -depth_threshold || da_max < depth_threshold {
        return false;
    }

    let dir = cross(na, nb);
    if dot(dir, dir) < TAU_NORMALIZE_SQ {
        // Planes (near-)parallel / coplanar — treat as non-intersecting.
        return false;
    }

    let abs_dir = [dir[0].abs(), dir[1].abs(), dir[2].abs()];
    let axis = if abs_dir[0] >= abs_dir[1] && abs_dir[0] >= abs_dir[2] {
        0
    } else if abs_dir[1] >= abs_dir[2] {
        1
    } else {
        2
    };
    let proj_a: [f64; 3] = [a[0][axis], a[1][axis], a[2][axis]];
    let proj_b: [f64; 3] = [b[0][axis], b[1][axis], b[2][axis]];

    match (
        compute_interval(&proj_a, &d_a),
        compute_interval(&proj_b, &db),
    ) {
        (Some((a_min, a_max)), Some((b_min, b_max))) => a_min < b_max && b_min < a_max,
        _ => false,
    }
}

/// Interval where a triangle's edges cross the opposing plane (projection
/// onto the dominant intersection-line axis). Port of the oracle's
/// `compute_interval`.
fn compute_interval(proj: &[f64; 3], dists: &[f64; 3]) -> Option<(f64, f64)> {
    let mut ts = Vec::with_capacity(2);
    for &(i, j) in &[(0usize, 1usize), (1, 2), (2, 0)] {
        let di = dists[i];
        let dj = dists[j];
        if (di > 0.0 && dj < 0.0) || (di < 0.0 && dj > 0.0) {
            ts.push(proj[i] + (proj[j] - proj[i]) * di / (di - dj));
        } else if di.abs() < TAU_NORMALIZE_SQ {
            ts.push(proj[i]);
        }
    }
    if ts.len() < 2
        && dists[2].abs() < TAU_NORMALIZE_SQ
        && (ts.is_empty() || (ts[0] - proj[2]).abs() > TAU_NORMALIZE_SQ)
    {
        ts.push(proj[2]);
    }
    if ts.len() >= 2 {
        let (a, b) = (ts[0], ts[1]);
        Some(if a <= b { (a, b) } else { (b, a) })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::FaceId;
    use crate::tessellate::FaceRange;

    /// Two single-triangle faces from flat data.
    fn two_face_mesh(positions: Vec<f64>) -> RenderMesh {
        assert_eq!(positions.len(), 18);
        RenderMesh {
            normals: vec![0.0; 18],
            indices: vec![0, 1, 2, 3, 4, 5],
            face_ranges: vec![
                FaceRange {
                    face: FaceId(0),
                    start: 0,
                    count: 3,
                },
                FaceRange {
                    face: FaceId(1),
                    start: 3,
                    count: 3,
                },
            ],
            positions,
        }
    }

    #[test]
    fn penetrating_faces_flagged() {
        // Oracle fixture: XY-plane triangle × XZ-plane triangle crossing
        // along the x-axis.
        let mesh = two_face_mesh(vec![
            -1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0, // face 0 (z=0)
            -1.0, 0.0, -1.0, 1.0, 0.0, -1.0, 0.0, 0.0, 1.0, // face 1 (y=0)
        ]);
        let v = first_inter_face_penetration(&mesh).expect("must flag penetration");
        assert_eq!((v.face_a, v.face_b), (FaceId(0), FaceId(1)));
        assert!(v.penetrations > 0);
    }

    #[test]
    fn shared_edge_adjacency_passes() {
        // Oracle fixture: two faces sharing edge (0,0,0)-(1,0,0) at
        // coordinate level (per-face duplicated vertices).
        let mesh = two_face_mesh(vec![
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        assert!(first_inter_face_penetration(&mesh).is_none());
    }

    #[test]
    fn shared_vertex_sagitta_dip_passes() {
        // PR-KV11 semantics: pair sharing ONE quantized vertex is skipped
        // even though the triangles geometrically cross near it.
        let mesh = two_face_mesh(vec![
            0.0, 0.0, 0.0, 1.0, -0.1, 0.0, 1.0, 0.1, 0.0, // fan around origin
            0.0, 0.0, 0.0, 1.0, 0.0, -0.1, 1.0, 0.0, 0.1, // pivots on shared vert
        ]);
        assert!(first_inter_face_penetration(&mesh).is_none());
    }

    #[test]
    fn grazing_contact_within_band_passes() {
        // Face 1 dips below face 0's plane by 1e-6 < threshold
        // (max_abs=2 ⇒ band = 2e-4): grazing, not penetration.
        let mesh = two_face_mesh(vec![
            -2.0, -2.0, 0.0, 2.0, -2.0, 0.0, 0.0, 2.0, 0.0, //
            -1.0, 0.5, 1.0, 1.0, 0.5, 1.0, 0.2, 0.5, -1e-6,
        ]);
        assert!(first_inter_face_penetration(&mesh).is_none());
    }

    #[test]
    fn deep_crossing_beyond_band_flagged() {
        // Same shape but the dip is 0.5 ≫ band: real penetration.
        let mesh = two_face_mesh(vec![
            -2.0, -2.0, 0.0, 2.0, -2.0, 0.0, 0.0, 2.0, 0.0, //
            -1.0, 0.5, 1.0, 1.0, 0.5, 1.0, 0.2, 0.5, -0.5,
        ]);
        assert!(first_inter_face_penetration(&mesh).is_some());
    }

    #[test]
    fn single_face_skipped() {
        let mesh = RenderMesh {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals: vec![0.0; 9],
            indices: vec![0, 1, 2],
            face_ranges: vec![FaceRange {
                face: FaceId(0),
                start: 0,
                count: 3,
            }],
        };
        assert!(first_inter_face_penetration(&mesh).is_none());
    }
}
