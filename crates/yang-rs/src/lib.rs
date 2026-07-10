//! Yang 2025 hybrid B-Rep / mesh boolean pipeline.
//!
//! ## Scope (aspirational)
//!
//! Implements the pipeline described in Yang et al. 2025, "A robust hybrid
//! Boolean operations method for mesh-and-surface hybrid models":
//!
//! - **Stage 0** (§4.5.5): Coplanar preprocessing
//! - **Stage 1** (§4.1): Bijective tessellation — PR-YR2: planar B-Reps;
//!   PR-YR7: cylinder; PR-YR12: sphere (Cone still rejects loudly)
//! - **Stage 2** (§4.2): Mesh boolean — delegate to `cherchi-rs`
//! - **Stage 3** (§4.3): SSI refinement — delegate to `ssi-rs`
//! - **Stage 4** (§4.4.1): Mesh updating — RELOCATION of intersection crossings
//!   onto the exact curve (+ §4.5.3 reversed-point sweep), watertightness
//!   inherited from the mesh boolean. The paper's CDT remesh / split-merge-insert
//!   is **NOT implemented** (deviation N2 in `docs/yang_deviations.md`); the
//!   sidecar's trimmed mesh is trusted and `check_watertight_2manifold` gates the
//!   output. Likewise §4.5.4 illegal-self-intersection removal is **NOT
//!   implemented** (deviation N6, roadmap-tracked).
//! - **Stage 5** (§4.4.2): Patch segmentation (flood-fill)
//! - **Stage 6** (§4.4.2): B-Rep reassembly
//!
//! ## Current implementation status (PR-YR5)
//!
//! - **Stage 1 PLANAR** (PR-YR2): `BRep::new(verts, edges, faces)`
//!   fan-triangulates each planar face from its first vertex; produces
//!   a 1:1 bijection (no Steiner points). Convex faces only; no inner
//!   loops; `Surface::Plane` only.
//! - **`boolean()` vertex provenance** (PR-YR3): every output mesh
//!   vertex is spatially matched against input A then B (within
//!   [`MATCH_TOLERANCE`]). On match, the corresponding input's
//!   `TessellationSource` is copied; unmatched verts get
//!   `TessellationSource::Intersection`.
//! - **`boolean()` triangle attribution** (PR-YR4): every output
//!   triangle is attributed to an input `(InputId, face_idx)` via
//!   majority-vote (≥2 of 3) over the vertices' provenance.
//!   Accessible via [`BRep::triangle_attribution`].
//! - **`boolean()` topology reconstruction** (PR-YR5): output `BRep`
//!   gets non-empty `vertices` (1:1 with mesh), `edges`, and `faces`
//!   via patch flood-fill on triangle attribution + boundary cycle
//!   recovery + surface inheritance from input faces.
//!   None-attributed (cut surface) triangles are intentionally
//!   skipped — output is a "kept-portions skeleton."
//! - **`BRep::from_mesh()` degenerate path** (PR-YR1 compat): empty
//!   topology; all-`Unknown` TessellationMap; empty
//!   TriangleAttributionMap.
//!
//! **Honest framing**: PR-YR3 + PR-YR4 + PR-YR5 are NOT real Yang
//! Stage 5/6. Real Stage 5/6 needs per-triangle labels from Stage 2's
//! arrangement which the C++ sidecar doesn't expose. The current
//! pipeline is a sidecar-feasible substitute.
//!
//! **PR-YR5 output is intentionally NOT 2-manifold** (rule-4
//! deviation): faces cover input-derived ("kept") portions only.
//! Cut-surface faces (`None`-attributed triangles → new BRepFaces with
//! reconstructed surfaces) are PR-YR6, which also re-enables the
//! 2-manifold contract.
//!
//! Banked for future PRs:
//! - PR-YR2b: ear-cutting for non-convex faces
//! - PR-YR2c: inner loops (holes) — currently → `NonManifoldOutput`
//! - PR-YR2d: curved surfaces (`Surface::Cylinder`, `Sphere`, NURBS)
//! - PR-YR2e: Steiner points + dε tolerance
//! - PR-YR2f: CDT at shared edges
//! - PR-YR4b: precomputed vertex→edge / edge→face incidence indices
//! - PR-YR5b: edge deduplication across faces (each face owns its edges in v1)
//! - PR-YR5c: inner-loop / hole support in patch boundary recovery
//! - PR-YR6: cut-surface face generation + 2-manifold validation
//! - PR-YR7+: edge curve recovery beyond `Curve::LineSegment`
//! - Real Stage 5/6: gated on labeled arrangement output
//!
//! ## Input / output
//!
//! - Input: two B-Rep solids (`BRep`)
//! - Output: one B-Rep solid
//! - Non-manifold detection is **not yet implemented** in PR-YR2.
//!
//! ## References
//!
//! - Yang et al. 2025 — `refs/text/yang2025_hybrid_boolean.txt`

// Stage 0 (Yang §4.5.5) coplanar-overlay geometric engine — M8 slice a
// (PR-YR25). NOT yet wired into `boolean()`; that's M8 slice b.
pub mod coplanar_overlay;
mod stage0;
// N2 increment 2: the §4.1.2 / Fig 6 per-triangle `d(T)` bound + its pinned
// parametric embedding. NOT yet wired into `stage4_relocate_and_correct`;
// that is N2-3. Spec: `specs/n2_stage4_dt_recompute.md`.
pub mod stage4_dt;
// N2 increment 1: the §4.4.1 mesh-updating primitive (Fig 11 split/merge/insert
// + interior-constraint CDT). NOT yet wired into `stage4_relocate_and_correct`;
// that is N2-3. Spec: `specs/n2_stage4_mesh_updating.md`.
mod boolean;
mod brep;
pub use boolean::boolean;
pub(crate) use boolean::*;
mod errors;
mod geom;
mod stage1_tessellate;
mod stage3_ssi;
pub(crate) use stage3_ssi::*;
mod stage4_correct;
mod stage5_topology;
pub(crate) use stage5_topology::*;
mod stage4_relocate;
pub use brep::{
    BRep, BRepEdge, BRepFace, BRepVertex, InputId, TessellationMap, TessellationSource,
    TriangleAttribution, TriangleAttributionMap, MATCH_TOLERANCE,
};
pub use stage1_tessellate::tessellate_torus_patch;
pub(crate) use stage1_tessellate::*;
pub(crate) use stage4_correct::*;
pub(crate) use stage4_relocate::*;
pub mod stage4_update;
pub use errors::{SsiRefinementError, Stage4InvalidReason, YangError};
pub(crate) use geom::{ellipse_param, ellipse_point, ellipse_tangent, surface_normal_at};
pub use geom::{hyperbola_point, parabola_point, signed_distance_to_surface, Curve, Surface};

pub use cad_primitives::{BoolOp, Point3, Vector3};
pub use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
pub use cherchi_rs::{Mesh, MeshBoolean};
pub use cherchi_rs::{NativeBoolean, NativeBooleanError};
// The constrained-Delaunay primitive, re-exported for the kernel-v2 render
// tessellation cores (its `tessellate.rs` patch/planar triangulation). kernel-v2
// may depend on yang-rs but NOT on cherchi-rs directly, so it consumes the CDT
// through this seam — the same pattern as `NativeBoolean` above and the torus
// UV-patch consumer's existing use of this primitive.
pub use cherchi_rs::triangulation::{
    cdt_polygon_with_holes, cdt_polygon_with_holes_floodfill, CdtError,
};
// `ArrangementError` is re-exported so that kernel-v2 (whose dep rules allow
// `yang-rs` but NOT `cherchi-rs`) can pattern-match the M8 boundary inside
// `NativeBooleanError::Arrangement` — specifically
// `ArrangementError::CoplanarPairDeferred`, which kernel-v2 maps to its
// typed `UnsupportedCoplanar` error. Public-surface addition only.
pub use cherchi_rs::ArrangementError;

/// Construct the PRODUCTION boolean backend: the native, in-process
/// cherchi-rs pipeline ([`NativeBoolean`]) — `mesh_arrangement` → labeling →
/// `keep_set(op)`. Reference parity vs the upstream C++ `mesh_booleans`
/// binary is the M6 gate (cherchi-rs `tests/parity_native_vs_sidecar.rs`);
/// the C++ subprocess sidecar (`cherchi-sidecar-rs`) is demoted to a
/// test-only parity oracle (PR-CR-BL3c).
///
/// Always `Some` since PR-CR-M7c: the predicates are clean-room pure Rust
/// (`cherchi-rs::predicates::indirect`) — there is no FFI stub build left to
/// guard against, and the backend is WASM-clean. The `Option` signature is
/// retained for the many existing
/// `let Some(nb) = yang_rs::native_backend() else { /* skip */ }` call
/// sites (their skip arms are now dead but harmless).
pub fn native_backend() -> Option<NativeBoolean> {
    Some(NativeBoolean)
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    // ── collapse_vertex membrane cancellation ────────────────────────────
    // Spec `specs/yang_collapse_membrane_cancellation.md` (task #121, the
    // N2/F0059 Stage-6 double-cover origin). A twin collapse can turn the
    // two-triangle pleat spanning the twin gap into an EXACT duplicate pair
    // with OPPOSITE windings — a zero-volume doubled flap that must cancel
    // (drop BOTH), restoring manifold edge counts.

    /// The minimal closed pleat: a sliver tetra {a,b,u,v} whose two large
    /// walls (a,b,u)/(a,v,b) become the opposite-winding duplicate after the
    /// twin collapse v→u. Indices 0..=3; positions are irrelevant to the
    /// combinatorial collapse but kept realistic (near-twin apexes).
    fn pleat_tetra_tris() -> Vec<[u32; 3]> {
        vec![[0, 1, 2], [1, 3, 2], [0, 2, 3], [0, 3, 1]]
    }

    fn membrane_fixture_verts() -> Vec<Point3> {
        vec![
            Point3::new(0.0, 0.0, 0.0),       // 0 = a
            Point3::new(1.0, 0.0, 0.0),       // 1 = b
            Point3::new(0.5, 0.4, 0.1),       // 2 = u (survivor twin)
            Point3::new(0.5, 0.4, 0.1000001), // 3 = v (victim twin)
            // Bystander tetra (a separate closed component that must be
            // preserved byte-for-byte through the cancellation).
            Point3::new(3.0, 0.0, 0.0), // 4
            Point3::new(4.0, 0.0, 0.0), // 5
            Point3::new(3.5, 1.0, 0.0), // 6
            Point3::new(3.5, 0.5, 1.0), // 7
        ]
    }

    fn bystander_tetra_tris() -> Vec<[u32; 3]> {
        vec![[4, 5, 6], [4, 6, 7], [4, 7, 5], [5, 7, 6]]
    }

    fn undirected_edge_counts(tris: &[[u32; 3]]) -> std::collections::BTreeMap<(u32, u32), u32> {
        let mut counts = std::collections::BTreeMap::new();
        for tri in tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (a, b) = (tri[i], tri[j]);
                let key = if a < b { (a, b) } else { (b, a) };
                *counts.entry(key).or_insert(0u32) += 1;
            }
        }
        counts
    }

    /// Cancellation branch: the pleat annihilates (both duplicate copies
    /// dropped), the bystander survives byte-identically, every remaining
    /// undirected edge is manifold count-2, and attribution stays lockstep.
    #[test]
    fn collapse_membrane_pleat_cancels_both_copies() {
        let mut tris = pleat_tetra_tris();
        tris.extend(bystander_tetra_tris());
        let mut mesh = Mesh::new(membrane_fixture_verts(), tris);
        let mut attribution: Vec<Option<TriangleAttribution>> = (0..mesh.tris.len())
            .map(|i| {
                Some(TriangleAttribution {
                    input: InputId::A,
                    face: i as u32,
                })
            })
            .collect();
        collapse_vertex(&mut mesh, &mut attribution, 3, 2);
        // The pleat's two gap slivers drop as degenerate; its two walls map
        // to the SAME sorted triple {0,1,2} with opposite windings — the
        // zero-volume flap — and must BOTH cancel. Only the bystander stays.
        assert_eq!(
            mesh.tris,
            bystander_tetra_tris(),
            "pleat must annihilate; bystander byte-identical"
        );
        assert_eq!(
            attribution
                .iter()
                .map(|a| a.expect("bystander attribution").face)
                .collect::<Vec<_>>(),
            vec![4, 5, 6, 7],
            "attribution must drop the cancelled pair in lockstep"
        );
        for ((a, b), n) in undirected_edge_counts(&mesh.tris) {
            assert_eq!(n, 2, "edge ({a},{b}) not manifold after cancellation");
        }
    }

    /// Same-winding branch: a genuine same-winding double cover is NOT a
    /// cancellable flap — both copies stay for the downstream loud STOPs.
    #[test]
    fn collapse_same_winding_duplicate_is_kept() {
        let mut tris = pleat_tetra_tris();
        // Flip the second wall so the post-collapse duplicates share one
        // winding: (0,3,1) → (0,1,3) maps to (0,1,2) — same cycle as wall 1.
        tris[3] = [0, 1, 3];
        tris.extend(bystander_tetra_tris());
        let mut mesh = Mesh::new(membrane_fixture_verts(), tris);
        let mut attribution: Vec<Option<TriangleAttribution>> = vec![None; mesh.tris.len()];
        collapse_vertex(&mut mesh, &mut attribution, 3, 2);
        let dup_count = mesh
            .tris
            .iter()
            .filter(|t| {
                let mut s = **t;
                s.sort_unstable();
                s == [0, 1, 2]
            })
            .count();
        assert_eq!(
            dup_count, 2,
            "same-winding duplicates must be left for downstream loudness"
        );
        assert_eq!(mesh.tris.len(), 6, "2 kept duplicates + 4 bystander tris");
    }

    /// No-duplicate branch: a clean twin collapse (split-pole octahedron —
    /// the twins own DISJOINT fan sectors) is byte-identical to the plain
    /// index-mapping semantics: seam tents drop as degenerate, fans merge,
    /// nothing cancels.
    #[test]
    fn collapse_without_duplicate_is_byte_identical() {
        // Equator 0..=3, south pole 4, north twins u=5 / v=6.
        let verts: Vec<Point3> = vec![
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(-1.0, 0.0, 0.0),
            Point3::new(0.0, -1.0, 0.0),
            Point3::new(0.0, 0.0, -1.0),
            Point3::new(0.0, 0.0, 1.0),
            Point3::new(0.0, 0.0, 1.0000001),
        ];
        let tris: Vec<[u32; 3]> = vec![
            // south fans
            [1, 0, 4],
            [2, 1, 4],
            [3, 2, 4],
            [0, 3, 4],
            // north: u covers sectors 01/12, v covers 23/30
            [0, 1, 5],
            [1, 2, 5],
            [2, 3, 6],
            [3, 0, 6],
            // seam tents at equator verts 2 and 0
            [5, 2, 6],
            [6, 0, 5],
        ];
        let mut mesh = Mesh::new(verts.clone(), tris);
        let mut attribution: Vec<Option<TriangleAttribution>> = vec![None; mesh.tris.len()];
        let dropped = collapse_vertex(&mut mesh, &mut attribution, 6, 5);
        assert_eq!(dropped, 2, "exactly the two seam tents drop as degenerate");
        let expected: Vec<[u32; 3]> = vec![
            [1, 0, 4],
            [2, 1, 4],
            [3, 2, 4],
            [0, 3, 4],
            [0, 1, 5],
            [1, 2, 5],
            [2, 3, 5],
            [3, 0, 5],
        ];
        assert_eq!(
            mesh.tris, expected,
            "clean collapse must not cancel anything"
        );
        assert_eq!(mesh.verts, verts, "collapse never touches vertex storage");
        for ((a, b), n) in undirected_edge_counts(&mesh.tris) {
            assert_eq!(n, 2, "edge ({a},{b}) not manifold after clean collapse");
        }
    }

    // ── rim junction derivation (N2/F0059 increment 2, banked) ──────────
    // Spec `specs/yang_rim_junction_insertion.md`. Fixture mirrors the
    // integration cylinder fixture (seam-edge encoding).

    fn rj_cylinder(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, height: f64) -> BRep {
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let crs = |a: [f64; 3], b: [f64; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let d = normalize3(axis_dir);
        let bot = axis_point;
        let top = [
            bot[0] + d[0] * height,
            bot[1] + d[1] * height,
            bot[2] + d[2] * height,
        ];
        let abs = [d[0].abs(), d[1].abs(), d[2].abs()];
        let world = if abs[0] <= abs[1] && abs[0] <= abs[2] {
            [1.0, 0.0, 0.0]
        } else if abs[1] <= abs[2] {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let e1 = normalize3(crs(d, world));
        let verts = vec![
            BRepVertex {
                point: Point3::new(
                    bot[0] + e1[0] * radius,
                    bot[1] + e1[1] * radius,
                    bot[2] + e1[2] * radius,
                ),
            },
            BRepVertex {
                point: Point3::new(
                    top[0] + e1[0] * radius,
                    top[1] + e1[1] * radius,
                    top[2] + e1[2] * radius,
                ),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(bot[0], bot[1], bot[2]),
                    normal: Vector3::new(-d[0], -d[1], -d[2]),
                    radius,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(top[0], top[1], top[2]),
                    normal: Vector3::new(d[0], d[1], d[2]),
                    radius,
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
                surface: Surface::Cylinder {
                    axis_point: Point3::new(axis_point[0], axis_point[1], axis_point[2]),
                    axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                    radius,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(-d[0], -d[1], -d[2]),
                    d: dot(d, bot),
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(d[0], d[1], d[2]),
                    d: -dot(d, top),
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        BRep::new(verts, edges, faces).expect("rj cylinder fixture builds")
    }

    /// The truncated-Steinmetz pair (h/2 < r): axes x and y crossing at
    /// each other's midpoints — the F0059 shape.
    fn rj_truncated_pair(r: f64, h: f64) -> (BRep, BRep) {
        (
            rj_cylinder([0.0, -h / 2.0, 0.0], [0.0, 1.0, 0.0], r, h),
            rj_cylinder([-h / 2.0, 0.0, 0.0], [1.0, 0.0, 0.0], r, h),
        )
    }

    /// F0059 class: each cap rim of each operand carries exactly the four
    /// lobe corners `(±h/2, ±√(r²−h²/4))`, exact on the rim circle AND on
    /// the other operand's lateral (spec oracle 1 + I2).
    #[test]
    fn rim_junctions_truncated_steinmetz_four_corners_per_cap() {
        let (r, h) = (0.35f64, 0.5f64);
        let (a, b) = rj_truncated_pair(r, h);
        let (map_a, map_b) = rim_junction_overrides(&a, &b);
        let w = (r * r - h * h / 4.0).sqrt();
        for (brep, map, other_axis_is_x) in [(&a, &map_a, true), (&b, &map_b, false)] {
            assert_eq!(
                map.keys().copied().collect::<Vec<_>>(),
                vec![0, 1],
                "both cap rims carry junctions"
            );
            for (&ei, pts) in map.iter() {
                assert_eq!(pts.len(), 4, "four lobe corners per cap rim");
                let Curve::Circle { center, radius, .. } = brep.edges()[ei as usize].curve else {
                    panic!("rim edge is a circle");
                };
                for p in pts {
                    let pa = p.as_array();
                    let ca = center.as_array();
                    let dd = [pa[0] - ca[0], pa[1] - ca[1], pa[2] - ca[2]];
                    let dist = (dd[0] * dd[0] + dd[1] * dd[1] + dd[2] * dd[2]).sqrt();
                    assert!(
                        (dist - radius).abs() <= 1e-12,
                        "I2: junction exactly on the rim circle"
                    );
                    // Exactly on the OTHER operand's lateral: distance to
                    // its axis (x or y axis through the origin) equals r.
                    let lat = if other_axis_is_x {
                        (pa[1] * pa[1] + pa[2] * pa[2]).sqrt()
                    } else {
                        (pa[0] * pa[0] + pa[2] * pa[2]).sqrt()
                    };
                    assert!(
                        (lat - r).abs() <= 1e-12,
                        "I2: junction exactly on the crossing lateral"
                    );
                    // The corner coordinates are the analytic lobe corners.
                    let along = if other_axis_is_x { pa[0] } else { pa[1] };
                    assert!(
                        (along.abs() - h / 2.0).abs() <= 1e-12,
                        "corner sits at ±h/2 along the crossing axis"
                    );
                    assert!(
                        (pa[2].abs() - w).abs() <= 1e-12,
                        "corner sits at ±√(r²−h²/4) in z"
                    );
                }
            }
        }
    }

    /// Rebuild plumbing (spec I1/I3): an empty override map rebuild is
    /// byte-identical; a real map plants every junction as a bit-exact
    /// Stage-1 mesh vertex.
    #[test]
    fn rebuilt_with_rim_overrides_identity_and_insertion() {
        let (a, b) = rj_truncated_pair(0.35, 0.5);
        let same = a
            .rebuilt_with_rim_overrides(&std::collections::BTreeMap::new())
            .expect("empty rebuild");
        assert_eq!(
            same.as_mesh(),
            a.as_mesh(),
            "I1: empty override map is byte-identical"
        );
        let (map_a, _) = rim_junction_overrides(&a, &b);
        let boosted = a
            .rebuilt_with_rim_overrides(&map_a)
            .expect("boosted rebuild");
        for pts in map_a.values() {
            for p in pts {
                assert!(
                    boosted.as_mesh().verts.iter().any(|q| q == p),
                    "junction {p:?} must be a bit-exact Stage-1 mesh vertex"
                );
            }
        }
    }

    /// kv9f1 class (h/2 > r): the seam never reaches the caps — no rim
    /// junctions, both maps empty (spec oracle 2 / branch row 1).
    #[test]
    fn rim_junctions_empty_when_seam_clears_caps() {
        let (a, b) = (
            rj_cylinder([0.0, -0.45, 0.0], [0.0, 1.0, 0.0], 0.2, 0.9),
            rj_cylinder([-0.45, 0.0, 0.0], [1.0, 0.0, 0.0], 0.2, 0.9),
        );
        let (map_a, map_b) = rim_junction_overrides(&a, &b);
        assert!(map_a.is_empty() && map_b.is_empty());
    }

    /// h/2 == r: each cap plane is exactly TANGENT to the other lateral —
    /// the tangency class is skipped (|δ| ≥ r_b), never inserted.
    #[test]
    fn rim_junctions_tangent_cap_plane_skipped() {
        let (a, b) = rj_truncated_pair_tangent();
        let (map_a, map_b) = rim_junction_overrides(&a, &b);
        assert!(map_a.is_empty() && map_b.is_empty());
    }

    fn rj_truncated_pair_tangent() -> (BRep, BRep) {
        let (r, h) = (0.35f64, 0.7f64);
        (
            rj_cylinder([0.0, -h / 2.0, 0.0], [0.0, 1.0, 0.0], r, h),
            rj_cylinder([-h / 2.0, 0.0, 0.0], [1.0, 0.0, 0.0], r, h),
        )
    }

    /// Candidates beyond the crossing lateral's axial extent are excluded
    /// (spec candidate filter 2): shifting B along its axis puts every
    /// infinite-LATERAL junction outside both operands' extents
    /// (a-rim × b-lateral would sit at x = ±0.245, outside b's
    /// [0.3, 0.65]; b-rim × a-lateral at y = ±0.302, outside a's
    /// [−0.25, 0.25]). The PLANE arm never fires here: cylinder rims are
    /// outside its cone-flanked v1 scope (the demonstrated-need gate —
    /// this population is proven healthy without insertion).
    #[test]
    fn rim_junctions_respect_lateral_extent() {
        let a = rj_cylinder([0.0, -0.25, 0.0], [0.0, 1.0, 0.0], 0.35, 0.5);
        let b = rj_cylinder([0.3, 0.0, 0.0], [1.0, 0.0, 0.0], 0.35, 0.5);
        let (map_a, map_b) = rim_junction_overrides(&a, &b);
        assert!(
            map_a.is_empty() && map_b.is_empty(),
            "lateral out-of-extent candidates excluded; cylinder rims outside \
             the plane arm's cone-flanked scope"
        );
    }

    // ── Increment 4: plane-face arm + coaxial azimuth propagation ────────
    // Spec `specs/yang_rim_junction_insertion.md` §4a/§4b — the
    // cone-hyperbola junction class (R0004/R0017/R0019/R0044/R0047/R0049):
    // coaxial cone-band rim circles crossing a PLANE face of the other
    // operand.

    /// Coaxial double-frustum lathe on the z-axis: rims (z=0, r0),
    /// (z=1, r1), (z=2, r2), two cone bands sharing the middle rim, planar
    /// caps at both ends. Adjacent radii must differ (genuine cones).
    fn rj_lathe(r0: f64, r1: f64, r2: f64) -> BRep {
        assert!(r0 != r1 && r1 != r2, "bands must be genuine cones");
        let verts = vec![
            BRepVertex {
                point: Point3::new(r0, 0.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(r1, 0.0, 1.0),
            },
            BRepVertex {
                point: Point3::new(r2, 0.0, 2.0),
            },
        ];
        let circle = |cz: f64, nz: f64, radius: f64| Curve::Circle {
            center: Point3::new(0.0, 0.0, cz),
            normal: Vector3::new(0.0, 0.0, nz),
            radius,
        };
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: circle(0.0, -1.0, r0),
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: circle(1.0, 1.0, r1),
            },
            BRepEdge {
                start: 2,
                end: 2,
                curve: circle(2.0, 1.0, r2),
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
        ];
        // Cone through profile points (ra, za)-(rb, zb): apex on the axis
        // where the linear radius profile reaches 0; axis_dir points from
        // the apex toward the band.
        let cone = |ra: f64, za: f64, rb: f64, zb: f64| -> Surface {
            let slope = (rb - ra) / (zb - za);
            let z_apex = za - ra / slope;
            let dir = if slope > 0.0 { 1.0 } else { -1.0 };
            Surface::Cone {
                apex: Point3::new(0.0, 0.0, z_apex),
                axis_dir: Vector3::new(0.0, 0.0, dir),
                half_angle: slope.abs().atan(),
            }
        };
        let faces = vec![
            BRepFace {
                surface: cone(r0, 0.0, r1, 1.0),
                outer_loop: vec![0, 3, 1, 3],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: cone(r1, 1.0, r2, 2.0),
                outer_loop: vec![1, 4, 2, 4],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -2.0,
                },
                outer_loop: vec![2],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        BRep::new(verts, edges, faces).expect("rj lathe fixture builds")
    }

    /// Axis-aligned box (the slab operand): 6 polygonal plane faces.
    fn rj_box(lo: [f64; 3], hi: [f64; 3]) -> BRep {
        let v = |x: f64, y: f64, z: f64| BRepVertex {
            point: Point3::new(x, y, z),
        };
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
        BRep::new(vertices, edges, faces).expect("rj box fixture builds")
    }

    /// §4a+§4b class oracle: every lathe rim crosses the slab's x = c face
    /// plane transversally → per rim, TWO direct junctions
    /// `(c, ±√(r²−c²), z)` PLUS the other rims' azimuths propagated
    /// exactly onto its own circle. All three rims present the SAME
    /// azimuth multiset (the Stage-1 band-strip alignment invariant I5).
    #[test]
    fn rim_junctions_plane_arm_lathe_slab_all_rims() {
        let (r0, r1, r2) = (1.0f64, 2.0, 0.8);
        let c = 0.75f64;
        let lathe = rj_lathe(r0, r1, r2);
        let slab = rj_box([c, -4.0, -0.5], [4.0, 4.0, 2.5]);
        let (map_l, map_s) = rim_junction_overrides(&lathe, &slab);
        assert!(map_s.is_empty(), "the slab has no circle rims");
        assert_eq!(
            map_l.keys().copied().collect::<Vec<_>>(),
            vec![0, 1, 2],
            "all three rims carry insertions"
        );
        let mut az_sets: Vec<Vec<f64>> = Vec::new();
        for (&ei, pts) in map_l.iter() {
            let Curve::Circle { center, radius, .. } = lathe.edges()[ei as usize].curve else {
                panic!("rim edge is a circle");
            };
            let cz = center.as_array()[2];
            // 2 direct junctions per rim + 2 propagated from each other rim.
            assert_eq!(pts.len(), 6, "rim {ei}: 2 direct + 4 propagated");
            let mut on_plane = 0usize;
            let mut azimuths: Vec<f64> = Vec::new();
            for pt in pts {
                let pa = pt.as_array();
                let rad = (pa[0] * pa[0] + pa[1] * pa[1]).sqrt();
                assert!(
                    (rad - radius).abs() <= 1e-12,
                    "I2/I5: point exactly on rim {ei}'s circle"
                );
                assert!((pa[2] - cz).abs() <= 1e-12, "point in rim {ei}'s plane");
                if (pa[0] - c).abs() <= 1e-12 {
                    on_plane += 1;
                    let w = (radius * radius - c * c).sqrt();
                    assert!(
                        (pa[1].abs() - w).abs() <= 1e-12,
                        "direct junction at (c, ±√(r²−c²), z)"
                    );
                }
                azimuths.push(pa[1].atan2(pa[0]).rem_euclid(2.0 * std::f64::consts::PI));
            }
            assert_eq!(on_plane, 2, "rim {ei}: exactly two direct junctions");
            azimuths.sort_by(f64::total_cmp);
            az_sets.push(azimuths);
        }
        for k in 1..az_sets.len() {
            assert_eq!(az_sets[k].len(), az_sets[0].len());
            for (a, b) in az_sets[k].iter().zip(az_sets[0].iter()) {
                assert!(
                    (a - b).abs() <= 1e-12,
                    "azimuth multisets align across coaxial rims"
                );
            }
        }
    }

    /// §4a containment: the slab shifted so its x-face plane still crosses
    /// the rim circles but OUTSIDE the face polygon → no insertion.
    #[test]
    fn rim_junctions_plane_arm_containment_outside_face() {
        let lathe = rj_lathe(1.0, 2.0, 0.8);
        let slab = rj_box([0.75, 2.5, -0.5], [4.0, 5.0, 2.5]);
        let (map_l, map_s) = rim_junction_overrides(&lathe, &slab);
        assert!(
            map_l.is_empty() && map_s.is_empty(),
            "crossings outside the face polygon must not insert"
        );
    }

    /// §4a parallel skip: a box whose only near face is PARALLEL to the rim
    /// planes (top face containing the middle rim's plane) → no section
    /// line, no insertion; its transversal side faces miss the circles.
    #[test]
    fn rim_junctions_plane_arm_parallel_plane_skipped() {
        let lathe = rj_lathe(1.0, 2.0, 0.8);
        let slab = rj_box([-4.0, -4.0, -1.0], [4.0, 4.0, 1.0]);
        let (map_l, map_s) = rim_junction_overrides(&lathe, &slab);
        assert!(
            map_l.is_empty() && map_s.is_empty(),
            "parallel planes have no transversal section line"
        );
    }

    /// §4b vocabulary gate: a full-circle rim owned by a TORUS face (the
    /// kv6d bent-tube profile rim) must never receive insertions — the
    /// band-strip propagation vocabulary covers Cone/Cylinder/Plane only.
    #[test]
    fn rim_junctions_group_gate_drops_torus_rims() {
        // 90° bent tube: torus center origin, axis +z, R=3, r=1 (the kv6d
        // fixture), profile rim e0 at center (3,0,0), normal +y, radius 1.
        let verts = vec![
            BRepVertex {
                point: Point3::new(4.0, 0.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 4.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(3.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 1.0, 0.0),
                    radius: 1.0,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 3.0, 0.0),
                    normal: Vector3::new(1.0, 0.0, 0.0),
                    radius: 1.0,
                },
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: 4.0,
                },
            },
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Torus {
                    center: Point3::new(0.0, 0.0, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    major_radius: 3.0,
                    minor_radius: 1.0,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, -1.0, 0.0),
                    d: 0.0,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(-1.0, 0.0, 0.0),
                    d: 0.0,
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        let tube = BRep::new(verts, edges, faces).expect("kv6d bent tube builds");
        // The slab's x = 3 face plane crosses profile rim e0 (center
        // (3,0,0), r=1, plane y=0) at (3, 0, ±1) — transversal, contained.
        let slab = rj_box([3.0, -0.5, -2.0], [5.0, 0.5, 2.0]);
        let (map_t, map_s) = rim_junction_overrides(&tube, &slab);
        assert!(
            map_t.is_empty() && map_s.is_empty(),
            "torus-owned rim groups must be dropped by the vocabulary gate"
        );
    }

    /// §4a arc extension (the measured corpus shape — partial revolves):
    /// a half-turn washer sector's OUTER arcs cross the slab plane at ONE
    /// in-sweep azimuth (the mirror root lies in the missing half); the
    /// junction is inserted there and NEVER at the out-of-sweep root, and
    /// §4b propagates the azimuth onto the INNER arcs exactly on-circle.
    #[test]
    fn rim_junctions_plane_arm_partial_arc_rims() {
        // Half-turn CONE-walled washer sector about +x (the plane arm's
        // v1 scope demands cone-flanked rims): trapezoid profile
        // (0,1.0)-(1,1.3)-(1,2.3)-(0,2.0), swept z ≥ 0 (angle π). Arcs:
        // e8 (r=1.0 @ x=0), e9 (r=1.3 @ x=1), e10 (r=2.3 @ x=1),
        // e11 (r=2.0 @ x=0), all centered on the x-axis with normal +x̂.
        let angle = std::f64::consts::PI;
        let prof = [(0.0, 1.0), (1.0, 1.3), (1.0, 2.3), (0.0, 2.0)];
        let mut verts: Vec<BRepVertex> = prof
            .iter()
            .map(|&(x, y)| BRepVertex {
                point: Point3::new(x, y, 0.0),
            })
            .collect();
        for &(x, y) in &prof {
            // Rotation by π about +x̂: (y, z) → (−y, z sign-flipped ≈ 0).
            let (c, s) = (angle.cos(), angle.sin());
            verts.push(BRepVertex {
                point: Point3::new(x, y * c, y * s),
            });
        }
        let seg = |a: u32, b: u32| BRepEdge {
            start: a,
            end: b,
            curve: Curve::LineSegment,
        };
        let mut edges = vec![
            seg(0, 1),
            seg(1, 2),
            seg(2, 3),
            seg(3, 0),
            seg(4, 5),
            seg(5, 6),
            seg(6, 7),
            seg(7, 4),
        ];
        for i in 0..4u32 {
            let (x, y) = prof[i as usize];
            edges.push(BRepEdge {
                start: i,
                end: i + 4,
                curve: Curve::Circle {
                    center: Point3::new(x, 0.0, 0.0),
                    normal: Vector3::new(1.0, 0.0, 0.0),
                    radius: y,
                },
            });
        }
        let (a0, a1, a2, a3) = (8u32, 9u32, 10u32, 11u32);
        let faces = vec![
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![0, 1, 2, 3],
                inner_loops: vec![],
                reversed: false,
            },
            // End cap after a π sweep: the z = 0 plane again, outward −ẑ
            // rotated → +ẑ... outward normal is R_x(π)·ẑ = −ẑ → (0,0,-1)?
            // The kv6b fixture computes (0, −sin α, cos α) = (0, 0, −1).
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![4, 5, 6, 7],
                inner_loops: vec![],
                reversed: false,
            },
            BRepFace {
                // Inner CONE wall (cavity sense): r = 1.0 @ x=0 → 1.3 @
                // x=1, slope 0.3, apex on the axis at x = −1.0/0.3.
                surface: Surface::Cone {
                    apex: Point3::new(-1.0 / 0.3, 0.0, 0.0),
                    axis_dir: Vector3::new(1.0, 0.0, 0.0),
                    half_angle: 0.3f64.atan(),
                },
                outer_loop: vec![0, a1, 4, a0],
                inner_loops: vec![],
                reversed: true,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(1.0, 0.0, 0.0),
                    d: -1.0,
                },
                outer_loop: vec![1, a2, 5, a1],
                inner_loops: vec![],
                reversed: false,
            },
            BRepFace {
                // Outer CONE wall: r = 2.0 @ x=0 → 2.3 @ x=1, slope 0.3,
                // apex at x = −2.0/0.3.
                surface: Surface::Cone {
                    apex: Point3::new(-2.0 / 0.3, 0.0, 0.0),
                    axis_dir: Vector3::new(1.0, 0.0, 0.0),
                    half_angle: 0.3f64.atan(),
                },
                outer_loop: vec![2, a3, 6, a2],
                inner_loops: vec![],
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(-1.0, 0.0, 0.0),
                    d: 0.0,
                },
                outer_loop: vec![3, a0, 7, a3],
                inner_loops: vec![],
                reversed: false,
            },
        ];
        let sector = BRep::new(verts, edges, faces).expect("washer sector builds");
        // Slab beyond y = −1.5: its y = −1.5 face plane crosses the OUTER
        // arcs (r = 2.3, 2.0) at z = +√(r² − 2.25) — only z > 0 is in the
        // sweep (the mirror root lies in the missing half). The inner arcs
        // (r = 1.0, 1.3) never reach y = −1.5 and receive only the
        // propagated cluster azimuths.
        let slab = rj_box([-1.0, -4.0, -4.0], [2.0, -1.5, 4.0]);
        let (map_x, map_s) = rim_junction_overrides(&sector, &slab);
        assert!(map_s.is_empty(), "the slab has no circle rims");
        assert_eq!(
            map_x.keys().copied().collect::<Vec<_>>(),
            vec![8, 9, 10, 11],
            "outer arcs carry direct junctions; inner arcs the propagated azimuths"
        );
        for (&ei, pts) in map_x.iter() {
            let Curve::Circle { center, radius, .. } = sector.edges()[ei as usize].curve else {
                panic!("arc edge is a circle");
            };
            // TWO clusters (one per outer arc's distinct junction azimuth),
            // both inside every arc's sweep window.
            assert_eq!(pts.len(), 2, "arc {ei}: both cluster azimuths inserted");
            let ca = center.as_array();
            for pt in pts {
                let pa = pt.as_array();
                assert!(pa[2] > 0.0, "arc {ei}: insertion inside the sweep window");
                let rad = ((pa[1] - ca[1]).powi(2) + (pa[2] - ca[2]).powi(2)).sqrt();
                assert!(
                    (rad - radius).abs() <= 1e-12,
                    "I2/I5: insertion exactly on arc {ei}'s circle"
                );
                assert!(
                    (pa[0] - ca[0]).abs() <= 1e-12,
                    "insertion in arc {ei}'s plane"
                );
            }
            if ei >= 10 {
                // Outer arcs contain their own DIRECT junction at
                // (x, −1.5, √(r²−2.25)) bit-near exactly.
                let w = (radius * radius - 2.25).sqrt();
                assert!(
                    pts.iter().any(|pt| {
                        let pa = pt.as_array();
                        (pa[1] + 1.5).abs() <= 1e-12 && (pa[2] - w).abs() <= 1e-12
                    }),
                    "outer arc {ei}: direct junction at (x, −1.5, √(r²−2.25)) missing"
                );
            }
        }
    }

    /// §4a disc containment: a cylinder's cap DISC (circle-bounded loop)
    /// admits only junctions within its radius — the R0019/R0044 shape.
    #[test]
    fn rim_junctions_plane_arm_disc_cap_containment() {
        let lathe = rj_lathe(1.0, 2.0, 0.8);
        // Cylinder along +x from x = 0.75, radius 1.3, centered at z = 1:
        // its x = 0.75 cap disc admits rim0's junction (distance 1.20 from
        // the cap center) and rim2's (1.04) but NOT rim1's (1.854 > 1.3).
        let cyl = rj_cylinder([0.75, 0.0, 1.0], [1.0, 0.0, 0.0], 1.3, 3.25);
        let (map_l, _map_c) = rim_junction_overrides(&lathe, &cyl);
        let c = 0.75f64;
        let cap_center = [0.75f64, 0.0, 1.0];
        // Every on-cap-plane insertion respects the disc radius.
        for pts in map_l.values() {
            for pt in pts {
                let pa = pt.as_array();
                if (pa[0] - c).abs() <= 1e-9 {
                    let dd = [
                        pa[0] - cap_center[0],
                        pa[1] - cap_center[1],
                        pa[2] - cap_center[2],
                    ];
                    let dist = (dd[0] * dd[0] + dd[1] * dd[1] + dd[2] * dd[2]).sqrt();
                    assert!(
                        dist <= 1.3 + 1e-9,
                        "on-cap junction outside the disc: {pa:?} (dist {dist})"
                    );
                }
            }
        }
        // The in-disc junctions on rim0 ARE inserted (red oracle).
        let w0 = (1.0f64 - c * c).sqrt();
        let rim0 = map_l.get(&0).expect("rim0 carries junctions");
        for sy in [-1.0f64, 1.0] {
            assert!(
                rim0.iter().any(|p| {
                    let pa = p.as_array();
                    (pa[0] - c).abs() <= 1e-9
                        && (pa[1] - sy * w0).abs() <= 1e-9
                        && pa[2].abs() <= 1e-9
                }),
                "rim0 in-disc junction (c, {sy}·√(1−c²), 0) missing"
            );
        }
        // And rim1's on-cap-plane candidates (outside the disc) are NOT.
        if let Some(rim1) = map_l.get(&1) {
            assert!(
                rim1.iter().all(|p| (p.as_array()[0] - c).abs() > 1e-9),
                "rim1 candidates on the cap plane must be rejected by the disc"
            );
        }
    }

    /// §4d: the certificate band is the TAU_WORK floor at unit scale,
    /// covers the measured ~1.2·ε·L ULP noise at the R0017 magnitude, and
    /// stays orders below every measured junction sagitta at its own
    /// scale (band monotonicity, spec I7).
    #[test]
    fn junction_certificate_band_is_scale_aware() {
        // Unit scale: the floor.
        let plane_unit = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -0.5,
        };
        assert_eq!(
            junction_certificate_band([0.1, 0.2, 0.5], plane_unit),
            cad_primitives::TAU_WORK
        );
        // R0017 magnitude (~4e3 coords, cone apex ~3e3): the measured
        // already-exact junction residual 1.36e-12 must certify, while
        // the measured chord sagitta 10.7 must stay ≥ 1e6× above.
        let cone_large = Surface::Cone {
            apex: Point3::new(-3216.2, -1481.6, 1664.5),
            axis_dir: Vector3::new(0.7596, 0.0, -0.6504),
            half_angle: 1.0477,
        };
        let band = junction_certificate_band([-3901.5, -2954.8, -2747.5], cone_large);
        assert!(
            band >= 1.36e-12,
            "covers evaluation-precision noise: {band}"
        );
        assert!(band <= 1e-10, "stays sub-sagitta by ≥6 orders: {band}");
        // R0047 micro magnitude (~3e-4): the floor rules, and the measured
        // 1.35e-7 sagitta can never certify.
        let cone_micro = Surface::Cone {
            apex: Point3::new(2.68e-4, -2.09e-4, 2.76e-4),
            axis_dir: Vector3::new(-0.4092, 0.0, -0.9124),
            half_angle: 0.5959,
        };
        let band_micro = junction_certificate_band([1.02e-4, -1.53e-4, 1.59e-4], cone_micro);
        assert_eq!(band_micro, cad_primitives::TAU_WORK);
        assert!(band_micro < 1.35e-7 / 1e4, "micro sagitta stays loud");
    }

    /// §4c: a group-consistent insertion (one azimuth on all three coaxial
    /// rims) tessellates the double-frustum watertight, with every inserted
    /// point a bit-exact Stage-1 mesh vertex.
    #[test]
    fn cone_bands_with_inserted_shared_rim_tessellate_watertight() {
        let lathe = rj_lathe(1.0, 2.0, 0.8);
        let th = 0.6f64;
        let mut map: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        for (ei, r, z) in [(0u32, 1.0f64, 0.0f64), (1, 2.0, 1.0), (2, 0.8, 2.0)] {
            map.insert(ei, vec![Point3::new(r * th.cos(), r * th.sin(), z)]);
        }
        let boosted = lathe
            .rebuilt_with_rim_overrides(&map)
            .expect("group-consistent insertion tessellates");
        let mesh = boosted.as_mesh();
        for pts in map.values() {
            for pt in pts {
                assert!(
                    mesh.verts.iter().any(|q| q == pt),
                    "inserted point {pt:?} must be a bit-exact mesh vertex"
                );
            }
        }
        // Watertight: every directed edge pairs with its reverse.
        let mut counts: std::collections::HashMap<(u32, u32), i64> =
            std::collections::HashMap::new();
        for tri in &mesh.tris {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                *counts.entry((tri[i], tri[j])).or_insert(0) += 1;
            }
        }
        for (&(s, e), &fwd) in &counts {
            let rev = counts.get(&(e, s)).copied().unwrap_or(0);
            assert_eq!(
                fwd, rev,
                "unpaired half-edge ({s},{e}) after shared-rim insertion"
            );
        }
    }

    // ── M5 surface-pair plumbing (Y1–Y3) ─────────────────────────────────

    fn qcyl(ap: [f64; 3], ad: [f64; 3], r: f64) -> ssi_rs::QuadricSurface {
        ssi_rs::QuadricSurface::Cylinder {
            axis_point: Point3::new(ap[0], ap[1], ap[2]),
            axis_dir: Vector3::new(ad[0], ad[1], ad[2]),
            radius: r,
        }
    }

    /// Y1: `SsiCurve::SurfacePair` maps to `Curve::SurfacePair` carrying both
    /// operands field-for-field as yang `Surface::Cylinder`s.
    #[test]
    fn m5_ssi_surface_pair_maps_to_curve_surface_pair() {
        let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let b = qcyl([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.5);
        let curve = ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a, b })
            .expect("cyl×cyl surface pair maps");
        match curve {
            Curve::SurfacePair {
                a: Surface::Cylinder { radius: ra, .. },
                b: Surface::Cylinder { radius: rb, .. },
            } => {
                assert_eq!(ra, 1.0);
                assert_eq!(rb, 0.5);
            }
            other => panic!("expected Curve::SurfacePair of two cylinders, got {other:?}"),
        }
    }

    /// Y1: a non-cylinder operand (no producer yet) rejects loudly.
    #[test]
    fn m5_surface_pair_non_cylinder_operand_rejected() {
        let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let plane = ssi_rs::QuadricSurface::Plane {
            point: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        assert!(ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a: cyl, b: plane }).is_err());
    }

    /// Y2: on-both-surfaces membership — a point exactly on the perpendicular
    /// unequal-R curve passes; a point off either cylinder by ≫ tol fails.
    #[test]
    fn m5_surface_pair_membership() {
        // x²+y²=1 ∧ x²+z²=¼ : point (0, 1, ½) lies on both.
        let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let b = qcyl([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.5);
        let sp = ssi_rs::SsiCurve::SurfacePair { a, b };
        assert!(curve_contains_point(
            &sp,
            Point3::new(0.0, 1.0, 0.5),
            1e-9,
            None
        ));
        // Off cylinder b radially by 0.1 ≫ tol.
        assert!(!curve_contains_point(
            &sp,
            Point3::new(0.0, 1.0, 0.6),
            1e-9,
            None
        ));
    }

    /// Y3: the surface-pair tangent at a point is `n̂_a × n̂_b`. At (0, 1, ½)
    /// the cylinder-a radial normal is +ŷ and cylinder-b radial normal is +ẑ,
    /// so the tangent is ±x̂.
    #[test]
    fn m5_surface_pair_tangent_is_normal_cross() {
        let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let b = qcyl([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.5);
        let sp = ssi_rs::SsiCurve::SurfacePair { a, b };
        let t = curve_tangent_at(&sp, Point3::new(0.0, 1.0, 0.5)).expect("transversal ⇒ tangent");
        assert!(t[0].abs() > 0.999, "tangent should be ±x̂, got {t:?}");
        assert!(t[1].abs() < 1e-9 && t[2].abs() < 1e-9);
    }

    /// Y3/Y4 failure mode: tangent (parallel normals) → no tangent (None), so
    /// the candidate stays non-tie-breakable and the loud stop stands.
    #[test]
    fn m5_surface_pair_tangent_none_at_tangency() {
        // Externally tangent unit cylinders touch along x=1,y=0: both normals
        // are ±x̂ on the contact line ⇒ parallel ⇒ no finite tangent.
        let a = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let b = qcyl([2.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let sp = ssi_rs::SsiCurve::SurfacePair { a, b };
        assert!(curve_tangent_at(&sp, Point3::new(1.0, 0.0, 0.0)).is_none());
    }

    // ── M5 cone-pair producer (Y1–Y3 with Cone operands) ─────────────────

    fn qcone(apex: [f64; 3], ad: [f64; 3], alpha: f64) -> ssi_rs::QuadricSurface {
        ssi_rs::QuadricSurface::Cone {
            apex: Point3::new(apex[0], apex[1], apex[2]),
            axis_dir: Vector3::new(ad[0], ad[1], ad[2]),
            half_angle: alpha,
        }
    }

    /// Y1: a cone-pair `SsiCurve::SurfacePair` maps to `Curve::SurfacePair`
    /// carrying both `Surface::Cone` operands field-for-field (cone-pair
    /// producer). A cyl×cone mixed pair maps too.
    #[test]
    fn m5_cone_pair_maps_to_curve_surface_pair() {
        let a = qcone(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_4,
        );
        let b = qcone([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 3.0_f64.atan());
        match ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a, b })
            .expect("cone×cone surface pair maps")
        {
            Curve::SurfacePair {
                a: Surface::Cone { half_angle: ha, .. },
                b: Surface::Cone { half_angle: hb, .. },
            } => {
                assert_eq!(ha, std::f64::consts::FRAC_PI_4);
                assert_eq!(hb, 3.0_f64.atan());
            }
            other => panic!("expected Curve::SurfacePair of two cones, got {other:?}"),
        }
        // Mixed cyl×cone also maps (both operand kinds supported).
        let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let cone = qcone(
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_4,
        );
        assert!(matches!(
            ssi_curve_to_curve(ssi_rs::SsiCurve::SurfacePair { a: cyl, b: cone }),
            Ok(Curve::SurfacePair {
                a: Surface::Cylinder { .. },
                b: Surface::Cone { .. }
            })
        ));
    }

    /// Y2: on-both-surfaces membership for a cone∩cylinder curve. The z-axis
    /// cone `radial = |h|·tan(π/4) = |h|` meets the z-axis cylinder `radial = 1`
    /// on the circle `radial = 1, h = ±1`; the point (1, 0, 1) lies on both.
    #[test]
    fn m5_cone_pair_membership() {
        let cone = qcone(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_4,
        );
        let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let sp = ssi_rs::SsiCurve::SurfacePair { a: cone, b: cyl };
        assert!(curve_contains_point(
            &sp,
            Point3::new(1.0, 0.0, 1.0),
            1e-9,
            None
        ));
        // Off the cone (h=1 needs radial=1, but radial here is 1.2) by ≫ tol.
        assert!(!curve_contains_point(
            &sp,
            Point3::new(1.2, 0.0, 1.0),
            1e-9,
            None
        ));
    }

    /// Y3: the cone-pair tangent at a transversal point is `n̂_a × n̂_b`. At
    /// (1, 0, 1) the π/4 cone normal is `(x̂ − ẑ)/√2` and the cylinder radial
    /// normal is `x̂`; their cross is ∓ŷ.
    #[test]
    fn m5_cone_pair_tangent_is_normal_cross() {
        let cone = qcone(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_4,
        );
        let cyl = qcyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let sp = ssi_rs::SsiCurve::SurfacePair { a: cone, b: cyl };
        let t = curve_tangent_at(&sp, Point3::new(1.0, 0.0, 1.0)).expect("transversal ⇒ tangent");
        assert!(t[1].abs() > 0.999, "tangent should be ±ŷ, got {t:?}");
        assert!(t[0].abs() < 1e-9 && t[2].abs() < 1e-9);
    }

    /// Y4: a perturbed near-curve point relocates onto both surfaces of a
    /// cone∩cylinder pair (the generic Newton engine handles Cone operands).
    #[test]
    fn m5_cone_pair_relocation_onto_both() {
        let cone = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: std::f64::consts::FRAC_PI_4,
        };
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        // Perturb the true curve point (1,0,1) off both surfaces.
        let p = relocate_onto_implicit_pair(Point3::new(1.02, 0.03, 0.98), cone, cyl)
            .expect("near-curve point relocates");
        assert!(signed_distance_to_surface(cone, p).unwrap().abs() < 1e-9);
        assert!(signed_distance_to_surface(cyl, p).unwrap().abs() < 1e-9);
    }

    // ── Case-IV phantom guard (spec `yang_case_iv_phantom_guard`) ────────

    /// Minimal solid cylinder B-Rep (two rims + seam) for the guard tests.
    fn guard_cyl(cx: f64, cy: f64, r: f64, h: f64) -> BRep {
        let verts = vec![
            BRepVertex {
                point: Point3::new(cx + r, cy, 0.0),
            },
            BRepVertex {
                point: Point3::new(cx + r, cy, h),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(cx, cy, 0.0),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(cx, cy, h),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
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
                surface: Surface::Cylinder {
                    axis_point: Point3::new(cx, cy, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -h,
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        BRep::new(verts, edges, faces).expect("guard cylinder")
    }

    /// The measured F0088 pair: a nested-disjoint tool inside the plate
    /// cylinder with gap 0.0115 < the natural N=14 sagitta — the guard must
    /// demand a finer N (34 at these radii: the smallest N with
    /// sag(R,N)+sag(r,N) ≤ gap/2).
    #[test]
    fn phantom_guard_nested_disjoint_demands_finer_n() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(1.2243, 0.0, 0.042871795720997065, 0.23);
        let n = phantom_min_rim_segments(&plate, &tool).expect("guard must fire");
        let gap = 1.2787008340600021 - 1.2243 - 0.042871795720997065;
        let sag = |r: f64, n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
        assert!(
            sag(1.2787008340600021, n) + sag(0.042871795720997065, n) <= gap / 2.0,
            "derived N={n} must clear the analytic gap with the factor-2 margin"
        );
        assert!(
            sag(1.2787008340600021, n - 1) + sag(0.042871795720997065, n - 1) > gap / 2.0,
            "derived N={n} must be MINIMAL (no over-refinement)"
        );
    }

    /// A crossing pair (the tool overlaps the plate wall) has no analytic
    /// gap — a real intersection curve exists and SSI refines it. No boost.
    #[test]
    fn phantom_guard_crossing_pair_is_silent() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(1.26, 0.0, 0.042871795720997065, 0.23);
        assert_eq!(phantom_min_rim_segments(&plate, &tool), None);
    }

    /// A far-disjoint pair derives a tiny N that both solids' natural
    /// Stage-1 N already satisfies — the self-limiting gate drops it.
    #[test]
    fn phantom_guard_far_pair_is_silent() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(0.3, 0.1, 0.042871795720997065, 0.23);
        assert_eq!(phantom_min_rim_segments(&plate, &tool), None);
    }

    /// Build one B-Rep carrying TWO cylinders (a plate wall + a hole at
    /// `(hx, hy)` with radius `hr`).
    fn two_cyl_brep(hx: f64, hy: f64, hr: f64) -> BRep {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(hx, hy, hr, 0.23);
        let mut verts = plate.vertices.clone();
        let mut edges = plate.edges.clone();
        let mut faces = plate.faces.clone();
        let (vo, eo) = (verts.len() as u32, edges.len() as u32);
        verts.extend(tool.vertices.iter().cloned());
        for e in &tool.edges {
            edges.push(BRepEdge {
                start: e.start + vo,
                end: e.end + vo,
                curve: e.curve,
            });
        }
        for f in &tool.faces {
            faces.push(BRepFace {
                surface: f.surface,
                outer_loop: f.outer_loop.iter().map(|&e| e + eo).collect(),
                inner_loops: Vec::new(),
                reversed: f.reversed,
            });
        }
        BRep::new(verts, edges, faces).expect("combined solid")
    }

    /// INTRA-solid pair (the chained F0088 output: hole 4's lateral 0.0115
    /// from the plate wall inside ONE solid): STAGE 1's own N selection must
    /// fold the pair's derived N in — otherwise ANY tessellation of the
    /// solid (input conversion included) puts the cap's outer-rim chords
    /// across the hole rim and the planar CDT gets crossing constraints
    /// (measured corpus F0088 ops 7/15, `CDT triangulation failed`). The
    /// near-rim solid must tessellate strictly denser than the same solid
    /// with its hole far from the wall.
    #[test]
    fn stage1_intra_solid_phantom_fold_densifies_rims() {
        let near = two_cyl_brep(1.2243, 0.0, 0.042871795720997065);
        let far = two_cyl_brep(0.3, 0.1, 0.042871795720997065);
        assert!(
            near.as_mesh().num_verts() > far.as_mesh().num_verts(),
            "near-rim solid must tessellate denser (near {} verts vs far {})",
            near.as_mesh().num_verts(),
            far.as_mesh().num_verts()
        );
        // And the cross-pair guard is silent for it — the intra fold lives
        // in Stage 1, not in the pair analysis.
        let partner = guard_cyl(10.0, 10.0, 0.1, 0.23);
        assert_eq!(phantom_min_rim_segments(&near, &partner), None);
    }

    /// An operand without B-Rep faces (the `from_mesh` chained-output
    /// degenerate) has no cylinder faces to scan — byte-identical path.
    #[test]
    fn phantom_guard_faceless_operand_is_silent() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let raw = BRep::from_mesh(plate.as_mesh().clone());
        assert_eq!(phantom_min_rim_segments(&plate, &raw), None);
        assert_eq!(phantom_min_rim_segments(&raw, &plate), None);
    }

    // R0072: position tie-break for near-coincident PARALLEL line candidates
    // (`select_disjoint_parallel_line`). Mirrors the instrumented R0072 edge
    // (2,143): two parallel generators whose endpoint-distance intervals are
    // disjoint → the lower (nearer) one is selected. The numbers are the live
    // probe values (cand0 ≈ 2.0e-5, cand1 ≈ 3.3e-5).
    #[test]
    fn r0072_parallel_line_position_tiebreak() {
        let dir = Vector3::new(
            0.539_214_627_766_961_7,
            -0.348_918_218_865_836_5,
            -0.766_487_874_493_543,
        );
        // Two parallel lines offset along a perpendicular `n̂` (⟂ dir), 2e-5 and
        // 3.3e-5 from the edge endpoints which sit on the origin segment.
        let n = {
            // any unit vector ⟂ dir
            let d = normalize3(dir.as_array());
            let t = [1.0, 0.0, 0.0];
            let dot = t[0] * d[0] + t[1] * d[1] + t[2] * d[2];
            let p = [t[0] - dot * d[0], t[1] - dot * d[1], t[2] - dot * d[2]];
            normalize3(p)
        };
        let line_at = |off: f64| (Point3::new(off * n[0], off * n[1], off * n[2]), dir);
        let cand0 = line_at(2.0e-5);
        let cand1 = line_at(3.3e-5);
        let p_s = Point3::new(0.0, 0.0, 0.0);
        let p_e = Point3::new(
            d_scale(dir, 1e-4)[0],
            d_scale(dir, 1e-4)[1],
            d_scale(dir, 1e-4)[2],
        );

        // Disjoint intervals → the nearer line (index 0) wins regardless of order.
        assert_eq!(
            select_disjoint_parallel_line(&[cand0, cand1], p_s, p_e),
            Some(0)
        );
        assert_eq!(
            select_disjoint_parallel_line(&[cand1, cand0], p_s, p_e),
            Some(1)
        );

        // OVERLAPPING intervals (generators merged below resolution) → no clear
        // winner → None (the caller keeps its loud `AmbiguousCurve`). Put the two
        // lines symmetrically about the segment so each endpoint is equidistant.
        let near_a = line_at(2.0e-5);
        let near_b = line_at(-2.0e-5);
        assert_eq!(
            select_disjoint_parallel_line(&[near_a, near_b], p_s, p_e),
            None
        );

        // NON-parallel candidates → None (the tangent discriminator's job).
        let crossing = (Point3::new(0.0, 0.0, 0.0), Vector3::new(n[0], n[1], n[2]));
        assert_eq!(
            select_disjoint_parallel_line(&[cand0, crossing], p_s, p_e),
            None
        );

        // Fewer than two candidates → None.
        assert_eq!(select_disjoint_parallel_line(&[cand0], p_s, p_e), None);
    }

    fn d_scale(v: Vector3, s: f64) -> [f64; 3] {
        let d = normalize3(v.as_array());
        [d[0] * s, d[1] * s, d[2] * s]
    }

    // PR-YR10 N3 regression (Yang §4.5.3): a U-turn at p_r — consecutive points
    // double back so v1 ≈ −v2 ⇒ |t̃| ≈ 0 — IS a reversal. The paper places the
    // collinear/degenerate-t̃ case WITHIN the reversal subset ("directly detect
    // the reversal, avoiding the angle comparisons"). p_b=(0,0,0) → p_r=(1,0,0)
    // → p_n=(0.5,0,0) reverses direction (v1=+x, v2=−x, t̃=0). The degenerate
    // branch must report a reversal. (Was the N3 logic inversion: returned
    // `false` = "healthy", silently failing to correct the very reversal §4.5.3
    // exists for; reachable whenever relocation produces an out-of-order point.)

    // PR-6 (coincident-cylinder rim conformal weld). Locks the two invariants
    // that make the curved-input rim weld a conformal exact-identity merge of
    // redundant reconstructions — NOT a tolerance bucket that could mask
    // unpaired edges (the reverted F0057 hazard):
    //   (1) two sub-ULP rim duplicates of one analytic point are BOTH on the
    //       cylinder (within the analytic band) AND within the cluster band,
    //       so they fuse;
    //   (2) two GENUINELY distinct rim points (≥ MIN_FEATURE_SIZE apart, here
    //       the ~1e-4 chord spacing) are on the cylinder but FAR outside the
    //       cluster band, so they never fuse.
    #[test]
    fn pr6_rim_weld_fuses_only_sub_ulp_duplicates() {
        let cyl = stage0::PairCylinder {
            axis_point: [0.0, 0.0, 0.0],
            axis_dir: [0.0, 0.0, 1.0],
            radius: 1.0,
            band: 1e-7,
            opposite: true,
        };
        let base = [1.0, 0.0, 0.3];
        // (1) A sub-ULP duplicate: perturb the in-plane coord by ~2 ULPs.
        let twin = [1.0 + 2.0 * f64::EPSILON, 0.0, 0.3];
        let scale = base
            .iter()
            .chain(twin.iter())
            .fold(0.0f64, |m, &c| m.max(c.abs()));
        let cluster_band = cad_primitives::TAU_WORK * (1.0 + scale);
        assert!(
            centroid_on_cylinder(base, &cyl) <= cyl.band,
            "base rim point must be on the cylinder"
        );
        assert!(
            centroid_on_cylinder(twin, &cyl) <= cyl.band,
            "sub-ULP twin must still be on the cylinder"
        );
        assert!(
            (0..3).all(|k| (base[k] - twin[k]).abs() <= cluster_band),
            "sub-ULP twin must be within the cluster band ⇒ fuses"
        );
        // (2) A genuinely distinct rim point ~1e-4 away along the rim: on the
        // cylinder, but FAR outside the cluster band ⇒ never fused.
        let theta = 1e-4_f64;
        let distinct = [theta.cos(), theta.sin(), 0.3];
        assert!(
            centroid_on_cylinder(distinct, &cyl) <= cyl.band,
            "the distinct rim point is also exactly on the cylinder"
        );
        assert!(
            (0..3).any(|k| (base[k] - distinct[k]).abs() > cluster_band),
            "a genuinely distinct rim point (≥ chord spacing) must lie OUTSIDE \
             the cluster band so the conformal weld never fuses it (no \
             tolerance-bucket masking)"
        );
    }

    // KV15 (spec `kv15_mixed_operand_planar_near_weld` §4): the mixed-operand
    // per-vertex near-weld. W2 — a planar-only femto pair (2 ULPs) fuses to
    // the min index; W3 — a curved-adjacent root never near-welds (kv9
    // junction-duplicate protection); W5 — genuinely distinct features
    // (≥ MIN_FEATURE_SIZE) sit far outside the band and never fuse.
    #[test]
    fn kv15_planar_femto_pair_welds_to_min_index() {
        let base = p(1.0, 0.0, 0.3);
        let twin = p(1.0 + 2.0 * f64::EPSILON, 0.0, 0.3);
        let verts = vec![base, twin];
        let mut weld = vec![0u32, 1u32];
        kv15_near_weld_pass(&verts, &mut weld, &[false, false]);
        assert_eq!(
            weld,
            vec![0, 0],
            "W2: a planar femto pair fuses, min-index survivor"
        );
    }

    #[test]
    fn kv15_curved_adjacent_root_never_near_welds() {
        let base = p(1.0, 0.0, 0.3);
        let twin = p(1.0 + 2.0 * f64::EPSILON, 0.0, 0.3);
        let verts = vec![base, twin];
        for flags in [[true, false], [false, true], [true, true]] {
            let mut weld = vec![0u32, 1u32];
            kv15_near_weld_pass(&verts, &mut weld, &flags);
            assert_eq!(
                weld,
                vec![0, 1],
                "W3: a curved-adjacent root (flags {flags:?}) must keep bit-exact \
                 identity — Stage-4 owns junction-duplicate collapse"
            );
        }
    }

    #[test]
    fn kv15_distinct_features_never_fuse() {
        // 1e-4 apart at coordinate scale ~1 — eight orders beyond the
        // TAU_WORK·(1+scale) band; the pair must never fuse (no
        // tolerance-bucket masking, the reverted-F0057 hazard).
        let verts = vec![p(1.0, 0.0, 0.3), p(1.0 + 1.0e-4, 0.0, 0.3)];
        let mut weld = vec![0u32, 1u32];
        kv15_near_weld_pass(&verts, &mut weld, &[false, false]);
        assert_eq!(
            weld,
            vec![0, 1],
            "W5: sub-floor is the mint-site's job; ≥-floor never fuses"
        );
    }

    /// KV15 spec W4 + §3 eligibility: only positively-proven all-planar
    /// descent yields an eligible (non-curved) vertex. Empty provenance,
    /// sentinel / out-of-range `tri_face` entries, an unknown face, and a
    /// non-planar face all mark every vertex of the triangle curved.
    #[test]
    fn kv15_eligibility_is_conservative() {
        let tris = vec![[0u32, 1, 2]];
        let planar_a = |k: u32, fi: u32| (k == 0 && fi == 7).then_some(true);
        // Positively proven planar descent → eligible.
        let src = vec![vec![(LaInputId(0), 0u32)]];
        assert_eq!(
            kv15_curved_touch(3, &tris, &src, &[7], &[], planar_a),
            vec![false; 3],
            "proven planar descent is eligible"
        );
        // Empty provenance (sidecar producer) → curved.
        assert_eq!(
            kv15_curved_touch(3, &tris, &[Vec::new()], &[7], &[], planar_a),
            vec![true; 3],
            "W4: empty provenance stays bit-exact"
        );
        // Sentinel tri_face entry → curved.
        assert_eq!(
            kv15_curved_touch(3, &tris, &src, &[u32::MAX], &[], planar_a),
            vec![true; 3],
            "sentinel face map entry stays bit-exact"
        );
        // Out-of-range local tri index → curved.
        let src_oob = vec![vec![(LaInputId(0), 9u32)]];
        assert_eq!(
            kv15_curved_touch(3, &tris, &src_oob, &[7], &[], planar_a),
            vec![true; 3],
            "out-of-range provenance stays bit-exact"
        );
        // Non-planar face → curved; input B routes through tri_face_b.
        let cyl_b = |k: u32, fi: u32| (k == 1 && fi == 3).then_some(false);
        let src_b = vec![vec![(LaInputId(1), 0u32)]];
        assert_eq!(
            kv15_curved_touch(3, &tris, &src_b, &[], &[3], cyl_b),
            vec![true; 3],
            "a curved-face descendant marks its vertices ineligible"
        );
        // Multi-parent (coplanar overlap): ONE curved parent poisons the tri.
        let mixed = vec![vec![(LaInputId(0), 0u32), (LaInputId(1), 0u32)]];
        let planar_a_cyl_b = |k: u32, fi: u32| ((k, fi) == (0, 7)).then_some(true).or(Some(false));
        assert_eq!(
            kv15_curved_touch(3, &tris, &mixed, &[7], &[3], planar_a_cyl_b),
            vec![true; 3],
            "any curved parent of a multi-parent tri stays bit-exact"
        );
    }

    // KV15b (spec `kv15b_mint_site_subresolution_collapse` §7): the
    // emission collapse of sub-`TAU_MODEL` intersection segments.
    fn kv15b_map(segs: &[(u32, u32)]) -> std::collections::BTreeMap<(u32, u32), Curve> {
        segs.iter()
            .map(|&(a, b)| ((a.min(b), a.max(b)), Curve::LineSegment))
            .collect()
    }

    #[test]
    fn kv15b_subresolution_intersection_segment_collapses() {
        // B1/I1: a 5e-8 intersection segment (0,1) collapses; min index
        // survives with its original bits; the degenerate tri drops.
        let twin = p(5.0e-8, 0.0, 0.0);
        let mut mesh = Mesh::new(
            vec![p(0.0, 0.0, 0.0), twin, p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
            vec![[0, 1, 3], [1, 2, 3]],
        );
        let mut attr = vec![None; 2];
        let map = kv15b_map(&[(0, 1)]);
        assert!(collapse_subresolution_intersection_segments(
            &mut mesh, &mut attr, &map
        ));
        assert_eq!(
            mesh.tris,
            vec![[0, 2, 3]],
            "degenerate tri dropped, twin remapped"
        );
        assert_eq!(
            mesh.verts[0],
            p(0.0, 0.0, 0.0),
            "I1: the survivor keeps its own exact coordinates"
        );
        assert_eq!(attr.len(), 1, "attribution stays in lockstep with tris");
    }

    #[test]
    fn kv15b_supraresolution_segment_untouched() {
        // B2/I2: 2e-7 ≥ TAU_MODEL — never collapses (a mutation widening the
        // band to MIN_FEATURE_SIZE must fail here: 2e-7 < 1e-6).
        let mut mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),
                p(2.0e-7, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
            ],
            vec![[0, 1, 3], [1, 2, 3]],
        );
        let mut attr = vec![None; 2];
        let map = kv15b_map(&[(0, 1)]);
        assert!(!collapse_subresolution_intersection_segments(
            &mut mesh, &mut attr, &map
        ));
        assert_eq!(
            mesh.tris,
            vec![[0, 1, 3], [1, 2, 3]],
            "B2: ≥ TAU_MODEL stays"
        );
    }

    #[test]
    fn kv15b_non_intersection_edge_untouched() {
        // B4/I3: the sub-TAU pair (0,1) is NOT an intersection segment —
        // inherited operand geometry (micro-profile corners) never collapses
        // (a mutation dropping the intersection-membership gate fails here).
        let mut mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),
                p(5.0e-8, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
            ],
            vec![[0, 1, 3], [1, 2, 3]],
        );
        let mut attr = vec![None; 2];
        let map = kv15b_map(&[(1, 2)]); // only the LONG edge is intersection
        assert!(!collapse_subresolution_intersection_segments(
            &mut mesh, &mut attr, &map
        ));
        assert_eq!(
            mesh.tris,
            vec![[0, 1, 3], [1, 2, 3]],
            "B4: a sub-TAU NON-intersection edge is inherited geometry — untouched"
        );
    }

    #[test]
    fn kv15b_twin_chain_resolves_to_single_survivor() {
        // B5: chain 0–1–2 with both links sub-TAU (5e-8 + 4e-8): both
        // collapse onto vertex 0 through the redirect (no chain drift beyond
        // the original twin cluster; exact-zero pairs B3 are never touched).
        let mut mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),
                p(5.0e-8, 0.0, 0.0),
                p(9.0e-8, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
            ],
            vec![[0, 1, 4], [1, 2, 4], [2, 3, 4]],
        );
        let mut attr = vec![None; 3];
        let map = kv15b_map(&[(0, 1), (1, 2)]);
        assert!(collapse_subresolution_intersection_segments(
            &mut mesh, &mut attr, &map
        ));
        assert_eq!(
            mesh.tris,
            vec![[0, 3, 4]],
            "B5: both twins collapse onto the min index; degenerate tris drop"
        );
    }

    // Spec `yang_stage6_sliver_topology` amendment 1 (S7): the
    // certainly-fatal chord split + null-excursion cancellation.
    fn s7_info(cycles: Vec<Vec<(u32, u32)>>) -> PatchInfo {
        PatchInfo {
            cycles,
            input: InputId::A,
            inherited: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            face_idx: 0,
            input_reversed: false,
            had_fold_sliver: false,
        }
    }

    fn s7_mesh() -> Mesh {
        Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),   // 0: chord start
                p(0.374, 0.0, 0.0), // 1: on the chord (exact)
                p(1.0, 0.0, 0.0),   // 2: chord end
                p(0.5, 1.0, 0.0),   // 3: apex of loop A
                p(0.5, -1.0, 0.0),  // 4: apex of loop B
                p(0.2, -1.0, 0.0),  // 5: apex of the second chord user (benign T)
            ],
            vec![[0, 2, 3], [1, 2, 4]],
        )
    }

    #[test]
    fn s7_fatal_chord_splits_and_spur_cancels() {
        // Loop A walks a spur (1→0) + the chord (0,2) over vertex 1; loop B
        // walks (2→1). Chord use-count 1, complementary {0,1}/{1,2} both
        // present → split at 1; the spur then cancels (amendment 1a) and A
        // emerges as the clean triangle 1→2→3→1.
        let infos = vec![
            s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
            s7_info(vec![vec![(2, 1), (1, 4), (4, 2)]]),
        ];
        let out = subdivide_loops_at_shared_vertices(&infos, &s7_mesh());
        assert_eq!(
            out[0][0],
            vec![(1, 2), (2, 3), (3, 1)],
            "S7: chord split at the on-segment vertex, spur cancelled"
        );
        assert_eq!(out[1][0], infos[1].cycles[0], "loop B untouched");
    }

    #[test]
    fn s7_benign_t_junction_untouched() {
        // The chord (0,2) is walked by TWO loops (use 2) while the
        // complementary chain {0,1}/{1,2} ALSO exists (loops A + C) — this
        // isolates the use==1 gate: a mutation dropping it splits here and
        // fails (the reference-parity guard for benign T-junctions).
        let infos = vec![
            s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
            s7_info(vec![vec![(2, 0), (0, 5), (5, 2)]]),
            s7_info(vec![vec![(2, 1), (1, 4), (4, 2)]]),
        ];
        let out = subdivide_loops_at_shared_vertices(&infos, &s7_mesh());
        assert_eq!(out[0][0], infos[0].cycles[0], "use-2 chord never splits");
        assert_eq!(out[1][0], infos[1].cycles[0]);
    }

    #[test]
    fn s7_missing_complementary_chain_untouched() {
        // No loop walks {1,2}: the complementary chain is absent, so the
        // split cannot certify a repair — S6 residue, unchanged.
        let infos = vec![
            s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
            s7_info(vec![vec![(0, 1), (1, 4), (4, 0)]]),
        ];
        let out = subdivide_loops_at_shared_vertices(&infos, &s7_mesh());
        assert_eq!(out[0][0], infos[0].cycles[0]);
    }

    #[test]
    fn s7_off_band_vertex_untouched() {
        // Vertex 1 lifted 1e-9 off the segment (> TAU_WORK): outside the
        // last-ulp band — no split (a mutation widening the band fails here).
        let mut mesh = s7_mesh();
        mesh.verts[1] = p(0.374, 1.0e-9, 0.0);
        let infos = vec![
            s7_info(vec![vec![(1, 0), (0, 2), (2, 3), (3, 1)]]),
            s7_info(vec![vec![(2, 1), (1, 4), (4, 2)]]),
        ];
        let out = subdivide_loops_at_shared_vertices(&infos, &mesh);
        assert_eq!(out[0][0], infos[0].cycles[0]);
    }

    // Spec `yang_s3_ellipse_rim_chord_bound` §7: the Stage-3 fallback bound
    // for ellipse-rim-only curved owners.
    #[test]
    fn s3_ellipse_rim_bound_is_max_major_radius_scaled() {
        // T2: mixed seg/ellipse edge list → 1e-2 · MAX major_radius (the
        // largest Stage-1 chain bound; a mutation picking min or the
        // minor_radius must fail).
        let ell = |a: f64, b: f64| BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::Ellipse {
                center: p(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                major_axis: Vector3::new(1.0, 0.0, 0.0),
                major_radius: a,
                minor_radius: b,
            },
        };
        let seg = BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        };
        let edges = vec![seg.clone(), ell(0.25, 0.2), ell(0.5, 0.1), seg];
        assert_eq!(
            ellipse_rim_chord_bound(&edges),
            Some(1e-2 * 0.5),
            "T2: the fallback is the LARGEST ellipse-chain bound"
        );
    }

    #[test]
    fn s3_ellipse_rim_bound_none_without_ellipses() {
        // T3: a seg-only owner has no fallback — the loud producer fault
        // stands (a mutation returning Some(TAU_WORK) here must fail).
        let seg = BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        };
        assert_eq!(
            ellipse_rim_chord_bound(&[seg]),
            None,
            "T3: no Circle and no Ellipse → producer fault preserved"
        );
    }

    #[test]
    fn kv15b_resolved_length_regrows_past_band_stays() {
        // B5 second half: after 1→0, segment (1,2) resolves to (0,2) at
        // 1.2e-7 ≥ TAU_MODEL — it must NOT collapse (single-sweep, no drift).
        let mut mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),
                p(5.0e-8, 0.0, 0.0),
                p(1.2e-7, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
            ],
            vec![[0, 1, 4], [1, 2, 4], [2, 3, 4]],
        );
        let mut attr = vec![None; 3];
        let map = kv15b_map(&[(0, 1), (1, 2)]);
        assert!(collapse_subresolution_intersection_segments(
            &mut mesh, &mut attr, &map
        ));
        assert_eq!(
            mesh.tris,
            vec![[0, 2, 4], [2, 3, 4]],
            "a segment whose RESOLVED length is ≥ TAU_MODEL stays (I2)"
        );
    }

    // Spec `yang_453_junction_protected_collapse` §3: the §4.5.3 collapse
    // victim is `p_n` on a same-curve run, but `p_r` when `p_n` is a curve
    // junction (the loop's curve changes at `p_n`).
    #[test]
    fn s453_collapse_removes_p_n_on_same_curve_run() {
        let circle = Curve::Circle {
            center: p(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let mut curves: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        curves.insert((1, 2), circle);
        curves.insert((2, 3), circle);
        let inc: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
            std::collections::BTreeMap::new();
        assert_eq!(
            reversal_collapse_direction(&curves, &inc, 1, 2, 3),
            (2, 1),
            "same curve beyond p_n ⇒ paper default: p_n is the victim"
        );
    }

    #[test]
    fn s453_collapse_protects_junction_p_n() {
        let circle = Curve::Circle {
            center: p(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let other = Curve::Circle {
            center: p(5.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.0,
        };
        let mut curves: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        curves.insert((1, 2), circle);
        curves.insert((2, 3), other);
        let inc: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
            std::collections::BTreeMap::new();
        assert_eq!(
            reversal_collapse_direction(&curves, &inc, 1, 2, 3),
            (1, 2),
            "curve changes at p_n ⇒ p_n is an exact curve-junction endpoint \
             and must survive; the overshooting p_r is the victim"
        );
        // Canonical-key robustness: descending vertex ids on both edges.
        let mut curves_rev: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        curves_rev.insert((7, 9), circle);
        curves_rev.insert((3, 7), other);
        assert_eq!(
            reversal_collapse_direction(&curves_rev, &inc, 9, 7, 3),
            (9, 7),
            "junction protection must hold under canonical (min,max) edge keys"
        );
    }

    // Spec §3c: straight-run reversal — branch table 4–7 on synthetic
    // curve + incidence maps. The seam runs along +x; vertex 1 (p_r) doubles
    // back to vertex 2 (p_n) at 0.5 (a U-turn on the run).
    #[test]
    fn s453c_line_run_reversal_branches() {
        use std::collections::BTreeMap;
        let mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.5, 0.0, 0.0),
                p(2.0, 0.0, 0.0),
            ],
            vec![],
        );
        let lo = std::f64::consts::FRAC_PI_4;
        let hi = 3.0 * std::f64::consts::FRAC_PI_4;
        let plane_a = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        let plane_b = Surface::Plane {
            normal: Vector3::new(0.0, 1.0, 0.0),
            d: 0.0,
        };
        let plane_c = Surface::Plane {
            normal: Vector3::new(0.0, 1.0, 1.0),
            d: 0.0,
        };
        let mut curves: BTreeMap<(u32, u32), Curve> = BTreeMap::new();
        curves.insert((0, 1), Curve::LineSegment);
        curves.insert((1, 2), Curve::LineSegment);
        curves.insert((2, 3), Curve::LineSegment);
        let pair = vec![(InputId::A, plane_a), (InputId::B, plane_b)];
        let pair_swapped = vec![(InputId::B, plane_b), (InputId::A, plane_a)];
        let pair_other = vec![(InputId::A, plane_a), (InputId::B, plane_c)];

        // Branch 7/6 precondition: same run through p_r (pair equality is
        // unordered), U-turn detected.
        let mut inc: BTreeMap<(u32, u32), Vec<(InputId, Surface)>> = BTreeMap::new();
        inc.insert((0, 1), pair.clone());
        inc.insert((1, 2), pair_swapped.clone());
        inc.insert((2, 3), pair.clone());
        assert!(
            is_reversed(&mesh, &curves, &inc, 0, 1, 2, lo, hi),
            "a U-turn on ONE straight seam run (unordered-equal pairs) is a \
             §4.5.3 reversal"
        );
        // Branch 7: same pair continues past p_n → paper default victim p_n.
        assert_eq!(reversal_collapse_direction(&curves, &inc, 1, 2, 3), (2, 1));
        // Branch 6: pair changes at p_n → p_n is the run junction; p_r is
        // the victim.
        inc.insert((2, 3), pair_other.clone());
        assert_eq!(reversal_collapse_direction(&curves, &inc, 1, 2, 3), (1, 2));

        // Branch 4: pair changes AT p_r → corner, never tested as a reversal
        // (even though the polyline doubles back).
        let mut inc4: BTreeMap<(u32, u32), Vec<(InputId, Surface)>> = BTreeMap::new();
        inc4.insert((0, 1), pair.clone());
        inc4.insert((1, 2), pair_other.clone());
        assert!(
            !is_reversed(&mesh, &curves, &inc4, 0, 1, 2, lo, hi),
            "a surface-pair change at p_r is a genuine corner, not a reversal"
        );

        // Branch 5: tangent/parallel pair (n_A × n_B ≈ 0) — cannot diagnose.
        // Use NON-doubling geometry so the U-turn arm doesn't fire first.
        let mesh5 = Mesh::new(
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(1.0, 1.0, 0.0)],
            vec![],
        );
        let coincident = vec![(InputId::A, plane_a), (InputId::B, plane_a)];
        let mut inc5: BTreeMap<(u32, u32), Vec<(InputId, Surface)>> = BTreeMap::new();
        inc5.insert((0, 1), coincident.clone());
        inc5.insert((1, 2), coincident.clone());
        assert!(
            !is_reversed(&mesh5, &curves, &inc5, 0, 1, 2, lo, hi),
            "a coincident-plane seam (§4.5.5) has no cross-product tangent — \
             healthy skip"
        );

        // Per-site eligibility: a run boundary (missing curve entry on one
        // side) is never a reversal site.
        let mut curves_gap: BTreeMap<(u32, u32), Curve> = BTreeMap::new();
        curves_gap.insert((1, 2), Curve::LineSegment);
        assert!(
            !is_reversed(&mesh, &curves_gap, &inc, 0, 1, 2, lo, hi),
            "p_r with a curve-less incident edge is a run boundary, not a site"
        );
        // Run END at p_n: curve(p_r,p_n) exists, curve(p_n,p_after) doesn't —
        // p_n survives, p_r is the victim.
        assert_eq!(
            reversal_collapse_direction(&curves_gap, &inc, 1, 2, 3),
            (1, 2),
            "the run's exact endpoint (no intersection edge beyond) survives"
        );
    }

    #[test]
    fn s453c_surface_normal_at_canonical() {
        let n = surface_normal_at(
            Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 2.0),
                d: 1.0,
            },
            p(5.0, 5.0, 5.0),
        )
        .expect("plane normal");
        assert!((n[2] - 1.0).abs() < 1e-15, "plane normal unit-normalized");

        let n = surface_normal_at(
            Surface::Cylinder {
                axis_point: p(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: 2.0,
            },
            p(2.0, 0.0, 7.0),
        )
        .expect("cylinder normal");
        assert!((n[0] - 1.0).abs() < 1e-15 && n[2].abs() < 1e-15);
        assert!(
            surface_normal_at(
                Surface::Cylinder {
                    axis_point: p(0.0, 0.0, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    radius: 2.0,
                },
                p(0.0, 0.0, 3.0),
            )
            .is_none(),
            "on-axis point has no radial direction"
        );

        let n = surface_normal_at(
            Surface::Sphere {
                center: p(1.0, 0.0, 0.0),
                radius: 5.0,
            },
            p(1.0, 3.0, 0.0),
        )
        .expect("sphere normal");
        assert!((n[1] - 1.0).abs() < 1e-15);

        // 45° cone: at a lateral point the normal is perpendicular to the
        // ruling direction and tilted 45° from the axis.
        let n = surface_normal_at(
            Surface::Cone {
                apex: p(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle: std::f64::consts::FRAC_PI_4,
            },
            p(1.0, 0.0, 1.0),
        )
        .expect("cone normal");
        let s = std::f64::consts::FRAC_1_SQRT_2;
        assert!((n[0] - s).abs() < 1e-12 && (n[2] + s).abs() < 1e-12);
    }

    // Spec §3b: §4.4.1(b) merge survivor ranking — junction > conic endpoint
    // > plain vertex; equal ranks keep the lower-index rule.
    #[test]
    fn s453_merge_survivor_prefers_exact_vertex() {
        use std::collections::BTreeSet;
        let junction: BTreeSet<u32> = [15u32].into_iter().collect();
        let conic: BTreeSet<u32> = [15u32, 20u32].into_iter().collect();

        // Conic endpoint (higher index) survives over a plain vertex — the
        // R0091 configuration, in BOTH argument orders.
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 8, 20),
            (8, 20)
        );
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 20, 8),
            (8, 20)
        );

        // Junction survives over a plain single-curve conic endpoint.
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 20, 15),
            (20, 15)
        );
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 15, 20),
            (20, 15)
        );

        // Equal rank (both plain): lower index survives — byte-identical to
        // the pre-fix behavior.
        assert_eq!(sub_feature_merge_direction(&junction, &conic, 4, 9), (9, 4));
        assert_eq!(sub_feature_merge_direction(&junction, &conic, 9, 4), (9, 4));
    }

    #[test]
    fn n3_degenerate_tangent_is_reversal() {
        let mesh = Mesh::new(
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.5, 0.0, 0.0)],
            vec![],
        );
        // Spec §3c per-site eligibility: p_r is a §4.5.3 site only when both
        // incident edges are intersection edges — give both a Circle entry on
        // the SAME curve (the original N3 fixture predates the site guard).
        let circle = Curve::Circle {
            center: p(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let mut curves: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        curves.insert((0, 1), circle);
        curves.insert((1, 2), circle);
        let lo = std::f64::consts::FRAC_PI_4;
        let hi = 3.0 * std::f64::consts::FRAC_PI_4;
        let inc: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
            std::collections::BTreeMap::new();
        assert!(
            is_reversed(&mesh, &curves, &inc, 0, 1, 2, lo, hi),
            "a 180° U-turn (degenerate t̃, Yang §4.5.3 collinear case) must be \
             detected as a reversal, not treated as healthy"
        );
    }

    // =====================================================================
    // M4 — demoted substitutes (test-only differential oracle).
    //
    // These were the production PR-YR3/YR4 spatial-match + majority-vote
    // attribution path. M3 replaced production attribution with real
    // LabeledArrangement labels; per roadmap rule #9 the substitutes are
    // RETAINED here as a second independent attribution method that
    // cross-checks the true-label path (the `m4_*` differential test).
    // Disagreement on a fixture localizes a label-path bug. Do NOT delete.
    // =====================================================================

    /// M4 oracle: try to match `target` against a vertex in `brep`'s mesh
    /// within `MATCH_TOLERANCE`. Returns the matched vertex's
    /// `TessellationSource` or `None`.
    fn match_against(brep: &BRep, target: Point3) -> Option<TessellationSource> {
        let tol2 = MATCH_TOLERANCE * MATCH_TOLERANCE;
        for (i, v) in brep.as_mesh().verts.iter().enumerate() {
            let dx = v.x() - target.x();
            let dy = v.y() - target.y();
            let dz = v.z() - target.z();
            if dx * dx + dy * dy + dz * dz <= tol2 {
                return Some(brep.tessellation_map().lookup(i as u32));
            }
        }
        None
    }

    /// M4 oracle: match `target` against A first, then B; track which
    /// input matched.
    fn match_with_input(
        a: &BRep,
        b: &BRep,
        target: Point3,
    ) -> (Option<InputId>, TessellationSource) {
        if let Some(src) = match_against(a, target) {
            return (Some(InputId::A), src);
        }
        if let Some(src) = match_against(b, target) {
            return (Some(InputId::B), src);
        }
        (None, TessellationSource::Intersection)
    }

    /// M4 oracle: the set of `(InputId, face_idx)` pairs that a single
    /// output vertex's provenance is compatible with.
    fn face_candidates(
        input: Option<InputId>,
        source: TessellationSource,
        a: &BRep,
        b: &BRep,
    ) -> Vec<(InputId, u32)> {
        let Some(input) = input else {
            return Vec::new();
        };
        let brep = match input {
            InputId::A => a,
            InputId::B => b,
        };
        match source {
            TessellationSource::BRepFace { face, .. } => vec![(input, face)],
            TessellationSource::BRepEdge { edge, .. } => brep
                .faces()
                .iter()
                .enumerate()
                .filter(|(_, f)| f.outer_loop.contains(&edge))
                .map(|(i, _)| (input, i as u32))
                .collect(),
            TessellationSource::BRepVertex(v) => brep
                .faces()
                .iter()
                .enumerate()
                .filter(|(_, f)| {
                    f.outer_loop.iter().any(|&e| {
                        let edge = &brep.edges()[e as usize];
                        edge.start == v || edge.end == v
                    })
                })
                .map(|(i, _)| (input, i as u32))
                .collect(),
            TessellationSource::Intersection | TessellationSource::Unknown => Vec::new(),
        }
    }

    /// M4 oracle: count votes per `(InputId, face)` across 3 candidate
    /// sets; return the highest-count pair reaching ≥2 votes (ties → lowest
    /// `(InputId, face)` lexicographic).
    fn majority_vote(sets: &[Vec<(InputId, u32)>; 3]) -> Option<TriangleAttribution> {
        use std::collections::BTreeMap;
        let mut counts: BTreeMap<(InputId, u32), u8> = BTreeMap::new();
        for set in sets {
            let mut uniq: Vec<(InputId, u32)> = set.clone();
            uniq.sort();
            uniq.dedup();
            for c in uniq {
                *counts.entry(c).or_insert(0) += 1;
            }
        }
        let mut best: Option<((InputId, u32), u8)> = None;
        for (key, &count) in &counts {
            if count < 2 {
                continue;
            }
            match best {
                None => best = Some((*key, count)),
                Some((_, bc)) if count > bc => best = Some((*key, count)),
                _ => {}
            }
        }
        best.map(|((input, face), _)| TriangleAttribution { input, face })
    }

    /// M4 oracle composite: run the full demoted substitute attribution
    /// (vertex provenance → per-vertex face candidates → majority vote)
    /// over `mesh`, producing a `TriangleAttributionMap`. This is exactly
    /// what the pre-M3 production `boolean()` computed internally; the
    /// reworked PR-YR4 substitute tests and the yr5_* reconstruction tests
    /// call it directly instead of routing through production `boolean()`
    /// (whose attribution is now the real-label path).
    fn substitute_attribution(mesh: &Mesh, a: &BRep, b: &BRep) -> TriangleAttributionMap {
        let mut inputs: Vec<Option<InputId>> = Vec::with_capacity(mesh.num_verts());
        let mut sources: Vec<TessellationSource> = Vec::with_capacity(mesh.num_verts());
        for &target in &mesh.verts {
            let (inp, src) = match_with_input(a, b, target);
            inputs.push(inp);
            sources.push(src);
        }
        let mut attributions = Vec::with_capacity(mesh.num_tris());
        for tri in &mesh.tris {
            let sets = [
                face_candidates(inputs[tri[0] as usize], sources[tri[0] as usize], a, b),
                face_candidates(inputs[tri[1] as usize], sources[tri[1] as usize], a, b),
                face_candidates(inputs[tri[2] as usize], sources[tri[2] as usize], a, b),
            ];
            attributions.push(majority_vote(&sets));
        }
        TriangleAttributionMap { attributions }
    }

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// An empty (0-triangle) `LabeledArrangement` for backend-dispatch
    /// tests that only care about the Ok/err control flow, not labels.
    fn empty_arrangement() -> LabeledArrangement {
        LabeledArrangement {
            mesh: Mesh::empty(),
            surface: Vec::new(),
            inside: Vec::new(),
            patch: Vec::new(),
            source: Vec::new(),
            num_inputs: 2,
        }
    }

    fn sample_mesh() -> Mesh {
        Mesh::new(
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
            vec![[0, 1, 2]],
        )
    }

    /// ADVERSARY (spec §2/I1, task #86): a vertex shared by ONE closed
    /// 3-triangle fan and ONE OPEN 2-triangle fan must NOT be split. The
    /// open fan's boundary edges (each incident to a single triangle) mean
    /// the star is not a union of closed disks, so the honest-split guard
    /// (`I1`) must leave the vertex — and the whole mesh — untouched, keeping
    /// the loud downstream gates in charge. This pins the closed-fan guard:
    /// the existing corpus/canonical union oracles cannot catch a weakened
    /// guard because their real pinch meshes have only closed fans.
    #[test]
    fn split_pinch_vertices_leaves_open_fan_untouched() {
        // Vertex 0 is the shared apex. Closed fan: (0,1,2),(0,2,3),(0,3,1)
        // — every 0-incident edge is 2-valent. Open fan: (0,4,5),(0,5,6) —
        // edges (0,4) and (0,6) are 1-valent (boundary). The two fans share
        // no vertex besides 0, so they are separate star components; a
        // guardless split would wrongly cut them into per-fan copies.
        let mut mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),  // 0 apex
                p(1.0, 0.0, 0.0),  // 1
                p(0.0, 1.0, 0.0),  // 2
                p(-1.0, 0.0, 0.0), // 3
                p(0.0, 0.0, 1.0),  // 4
                p(0.0, 0.0, 2.0),  // 5
                p(0.0, 0.0, 3.0),  // 6
            ],
            vec![[0, 1, 2], [0, 2, 3], [0, 3, 1], [0, 4, 5], [0, 5, 6]],
        );
        let before_verts = mesh.verts.len();
        let before_tris = mesh.tris.clone();
        let mut relocations: Vec<(u32, f64)> = Vec::new();
        let splits = split_pinch_vertices(&mut mesh, &mut relocations);
        assert_eq!(splits, 0, "open-fan vertex must not be split (I1 guard)");
        assert_eq!(
            mesh.verts.len(),
            before_verts,
            "open-fan split must not append vertices"
        );
        assert_eq!(
            mesh.tris, before_tris,
            "open-fan split must not rewrite triangle indices"
        );
    }

    /// ADVERSARY (spec §8/I4, task #86): a bowtie patch — two triangle lobes
    /// meeting at ONE mesh-manifold pinch vertex — must walk into TWO
    /// separate boundary cycles, one per lobe, NOT one chained self-crossing
    /// cycle. The pinch (vertex 3) is entered MID-walk with out-degree 2, and
    /// the wedge-correct continuation (stay in the incoming lobe) is
    /// deliberately the HIGHER-indexed outgoing edge, so lowest-first would
    /// cross into the other lobe and chain both loops into one cycle. This
    /// pins the wedge walk; the union oracles cannot catch a lowest-first
    /// regression because their post-split walks never hit a mid-walk pinch.
    #[test]
    fn patch_boundary_cycle_splits_bowtie_into_two_cycles() {
        // Lobe A = tri[3,6,0], Lobe B = tri[3,1,2], sharing pinch vertex 3.
        // Verts 4,5 are unused filler so index 6 is addressable.
        let mesh = Mesh::new(
            vec![
                p(1.0, 1.0, 0.0),  // 0
                p(-1.0, 0.0, 0.0), // 1
                p(-1.0, 1.0, 0.0), // 2
                p(0.0, 0.0, 0.0),  // 3 = pinch
                p(5.0, 5.0, 5.0),  // 4 filler
                p(6.0, 6.0, 6.0),  // 5 filler
                p(1.0, 0.0, 0.0),  // 6
            ],
            vec![[3, 6, 0], [3, 1, 2]],
        );
        let patch = Patch {
            attribution: TriangleAttribution {
                input: InputId::A,
                face: 0,
            },
            tri_indices: vec![0, 1],
        };
        let cycles =
            patch_boundary_cycle(&patch, &mesh).expect("bowtie patch boundary walk must succeed");
        assert_eq!(
            cycles.len(),
            2,
            "bowtie patch must split into 2 per-lobe cycles, not chain into \
             one; got {cycles:?}"
        );
        for c in &cycles {
            assert_eq!(c.len(), 3, "each lobe is a 3-edge triangle boundary");
        }
    }

    /// Backend whose `boolean()` always errors and which does NOT override
    /// the M3 `labeled_arrangement` trait method, so it surfaces through
    /// the default ("not supported") error. Used by
    /// `boolean_with_err_backend` to confirm `boolean()` maps a backend
    /// failure to `YangError::MeshBooleanFailed`.
    struct MockBackend;
    impl MeshBoolean for MockBackend {
        fn boolean(
            &self,
            _a: &Mesh,
            _b: &Mesh,
            _op: BoolOp,
        ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
            Err(Box::from("mock failure"))
        }
    }

    // ----- Group 2: yang-rs type construction -----

    #[test]
    fn surface_plane_construction() {
        let s = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -1.0,
        };
        match s {
            Surface::Plane { normal, d } => {
                assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(d, -1.0);
            }
            // `s` is constructed as `Plane`, so this arm is never hit; it
            // only satisfies exhaustiveness once curved variants are added.
            _ => panic!("expected Plane"),
        }
    }

    // ----- PR-YR6: curved Surface / Curve construction round-trips -----

    #[test]
    fn surface_sphere_construction() {
        let s = Surface::Sphere {
            center: p(1.0, 2.0, 3.0),
            radius: 5.0,
        };
        match s {
            Surface::Sphere { center, radius } => {
                assert_eq!(center, p(1.0, 2.0, 3.0));
                assert_eq!(radius, 5.0);
            }
            _ => panic!("expected Sphere"),
        }
    }

    #[test]
    fn surface_cylinder_construction() {
        let s = Surface::Cylinder {
            axis_point: p(1.0, 2.0, 3.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 4.0,
        };
        match s {
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                assert_eq!(axis_point, p(1.0, 2.0, 3.0));
                assert_eq!(axis_dir, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(radius, 4.0);
            }
            _ => panic!("expected Cylinder"),
        }
    }

    #[test]
    fn surface_cone_construction() {
        let s = Surface::Cone {
            apex: p(0.0, 0.0, 10.0),
            axis_dir: Vector3::new(0.0, 0.0, -1.0),
            half_angle: 0.5,
        };
        match s {
            Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                assert_eq!(apex, p(0.0, 0.0, 10.0));
                assert_eq!(axis_dir, Vector3::new(0.0, 0.0, -1.0));
                assert_eq!(half_angle, 0.5);
            }
            _ => panic!("expected Cone"),
        }
    }

    #[test]
    fn curve_circle_construction() {
        let c = Curve::Circle {
            center: p(1.0, 2.0, 3.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.5,
        };
        match c {
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                assert_eq!(center, p(1.0, 2.0, 3.0));
                assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(radius, 2.5);
            }
            _ => panic!("expected Circle"),
        }
    }

    #[test]
    fn curve_ellipse_construction() {
        let c = Curve::Ellipse {
            center: p(1.0, 2.0, 3.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            major_axis: Vector3::new(1.0, 0.0, 0.0),
            major_radius: 6.0,
            minor_radius: 3.0,
        };
        match c {
            Curve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => {
                assert_eq!(center, p(1.0, 2.0, 3.0));
                assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(major_axis, Vector3::new(1.0, 0.0, 0.0));
                assert_eq!(major_radius, 6.0);
                assert_eq!(minor_radius, 3.0);
            }
            _ => panic!("expected Ellipse"),
        }
    }

    // ----- PR-YR6: BRep::new loud-rejects curved surfaces -----

    /// Minimal well-formed single-triangle topology (3 verts, 3 edges, one
    /// face with a 3-edge outer loop). Mirrors the `brep_new_single_triangle`
    /// fixture exactly except the single face's surface is caller-supplied,
    /// so the ONLY variable across the loud-rejection tests is the surface.
    fn single_triangle_topology(
        surface: Surface,
    ) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface,
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        (verts, edges, faces)
    }

    #[test]
    fn brep_new_rejects_sphere_face() {
        // PR-YR12 migration: the sphere path is now implemented, but a sphere
        // face on a single *triangle* (no Circle meridian seam edge) lacks the
        // seam the sphere tessellation requires, so it is rejected as
        // MalformedTopology rather than CurvedSurfaceNotYetSupported. It must
        // STILL error loudly; only the error kind changed (mirrors the cylinder
        // migration above).
        let (verts, edges, faces) = single_triangle_topology(Surface::Sphere {
            center: p(0.0, 0.0, 0.0),
            radius: 1.0,
        });
        let result = BRep::new(verts, edges, faces);
        assert!(
            matches!(result, Err(YangError::MalformedTopology(_))),
            "expected MalformedTopology (sphere on a triangle lacks its meridian \
             seam Circle edge), got {result:?}"
        );
    }

    #[test]
    fn brep_new_rejects_cylinder_face() {
        // PR-YR7 migration: the cylinder lateral path is now implemented, but a
        // cylinder face on a single *triangle* (no Circle rim edges) lacks the
        // lateral's 2 required Circle rims, so it is rejected as
        // MalformedTopology rather than CurvedSurfaceNotYetSupported. It must
        // STILL error loudly; only the error kind changed.
        let (verts, edges, faces) = single_triangle_topology(Surface::Cylinder {
            axis_point: p(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        });
        let result = BRep::new(verts, edges, faces);
        assert!(
            matches!(result, Err(YangError::MalformedTopology(_))),
            "expected MalformedTopology (cylinder lateral on a triangle lacks its \
             2 Circle rim edges), got {result:?}"
        );
    }

    #[test]
    fn brep_new_rejects_cone_face() {
        // PR-YR16 migration: a Cone face on a *triangle* (no base-rim Circle the
        // cone tessellation path requires) is now MalformedTopology, mirroring the
        // cylinder/sphere-on-a-triangle rejection. It must STILL error loudly
        // (never silently succeed); only the error *kind* changed.
        let (verts, edges, faces) = single_triangle_topology(Surface::Cone {
            apex: p(0.0, 0.0, 1.0),
            axis_dir: Vector3::new(0.0, 0.0, -1.0),
            half_angle: 0.5,
        });
        let result = BRep::new(verts, edges, faces);
        assert!(
            matches!(result, Err(YangError::MalformedTopology(_))),
            "expected MalformedTopology (cone lateral on a triangle lacks its \
             base-rim Circle edge), got {result:?}"
        );
    }

    #[test]
    fn curve_line_segment_construction() {
        let c = Curve::LineSegment;
        assert_eq!(c, Curve::LineSegment);
    }

    #[test]
    fn brep_topology_construction() {
        let v = BRepVertex {
            point: p(0.0, 0.0, 0.0),
        };
        let e = BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        };
        let f = BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        };
        assert_eq!(v.point, p(0.0, 0.0, 0.0));
        assert_eq!(e.start, 0);
        assert_eq!(f.outer_loop.len(), 3);
    }

    #[test]
    fn tessellation_source_round_trip() {
        let src = TessellationSource::BRepVertex(7);
        match src {
            TessellationSource::BRepVertex(i) => assert_eq!(i, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tessellation_map_empty() {
        let m = TessellationMap::empty();
        assert_eq!(m.len(), 0);
        assert!(m.is_empty());
    }

    // ----- Group 3: from_mesh degenerate path -----

    #[test]
    fn from_mesh_preserves_mesh() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.as_mesh(), &m);
    }

    #[test]
    fn from_mesh_map_length_matches_verts() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.tessellation_map().len(), m.num_verts());
    }

    #[test]
    fn from_mesh_map_entries_all_unknown() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        for i in 0..b.tessellation_map().len() as u32 {
            assert_eq!(b.tessellation_map().lookup(i), TessellationSource::Unknown);
        }
    }

    // ----- Group 4: BRep::new Stage 1 happy paths -----

    fn plane_z_up() -> Surface {
        Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        }
    }

    #[test]
    fn brep_new_single_triangle() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 3);
        assert_eq!(b.num_tris(), 1);
        for i in 0..3u32 {
            assert_eq!(
                b.tessellation_map().lookup(i),
                TessellationSource::BRepVertex(i)
            );
        }
    }

    #[test]
    fn brep_new_quad_face() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 4);
        assert_eq!(b.num_tris(), 2); // 4-vert fan: 2 tris
    }

    #[test]
    fn brep_new_tetrahedron() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 0.0, 1.0),
            },
        ];
        // Edges of a tetrahedron: 6 edges between 4 vertices.
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            }, // 0
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            }, // 1
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            }, // 2
            BRepEdge {
                start: 0,
                end: 3,
                curve: Curve::LineSegment,
            }, // 3
            BRepEdge {
                start: 1,
                end: 3,
                curve: Curve::LineSegment,
            }, // 4
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::LineSegment,
            }, // 5
            // Reverse-direction edges for the loops (each tet face has 3 edges)
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            }, // 6
            BRepEdge {
                start: 3,
                end: 1,
                curve: Curve::LineSegment,
            }, // 7
            BRepEdge {
                start: 3,
                end: 2,
                curve: Curve::LineSegment,
            }, // 8
            BRepEdge {
                start: 1,
                end: 0,
                curve: Curve::LineSegment,
            }, // 9
            BRepEdge {
                start: 2,
                end: 1,
                curve: Curve::LineSegment,
            }, // 10
            BRepEdge {
                start: 0,
                end: 2,
                curve: Curve::LineSegment,
            }, // 11
        ];
        // 4 triangular faces. Each loop is 3 edges. Note: outer_loop's
        // start vertices must form a coherent cycle for fan-triangulation
        // to produce correct tris; we use edges 0,1,2 for the "bottom"
        // (verts 0→1→2), etc.
        let faces = vec![
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            }, // bottom (verts 0,1,2)
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![9, 3, 7],
                inner_loops: Vec::new(),
                reversed: false,
            }, // back (verts 1,0,3) - using 1→0,0→3,3→1
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![10, 4, 8],
                inner_loops: Vec::new(),
                reversed: false,
            }, // right (verts 2,1,3)
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![11, 5, 6],
                inner_loops: Vec::new(),
                reversed: false,
            }, // left (verts 0,2,3)
        ];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 4);
        assert_eq!(b.num_tris(), 4);
    }

    #[test]
    fn brep_new_unit_cube() {
        // 8 verts of a unit cube at origin.
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 0.0, 1.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 1.0),
            },
            BRepVertex {
                point: p(1.0, 1.0, 1.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 1.0),
            },
        ];
        // For PR-YR2 we don't need real edge dedup; just enumerate the
        // 24 directed edges we'll need (one per face boundary).
        // bottom face vertices: 0→3→2→1, edges 0:0→3, 1:3→2, 2:2→1, 3:1→0
        // (we just need fan_verts[0] to be the starting vertex of each
        // outer_loop)
        let edges: Vec<BRepEdge> = vec![
            // bottom face: 0, 3, 2, 1
            (0, 3),
            (3, 2),
            (2, 1),
            (1, 0),
            // top face: 4, 5, 6, 7
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            // south face: 0, 1, 5, 4
            (0, 1),
            (1, 5),
            (5, 4),
            (4, 0),
            // north face: 3, 7, 6, 2
            (3, 7),
            (7, 6),
            (6, 2),
            (2, 3),
            // east face: 1, 2, 6, 5
            (1, 2),
            (2, 6),
            (6, 5),
            (5, 1),
            // west face: 0, 4, 7, 3
            (0, 4),
            (4, 7),
            (7, 3),
            (3, 0),
        ]
        .into_iter()
        .map(|(s, e)| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        })
        .collect();
        let plane = plane_z_up();
        let faces = vec![
            BRepFace {
                surface: plane,
                outer_loop: vec![0, 1, 2, 3],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![4, 5, 6, 7],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![8, 9, 10, 11],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![12, 13, 14, 15],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![16, 17, 18, 19],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![20, 21, 22, 23],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 8);
        assert_eq!(b.num_tris(), 12); // 6 quads × 2 tris each
    }

    #[test]
    fn brep_new_bijection_is_one_to_one() {
        // Build a tetrahedron and confirm every mesh vertex i maps to
        // TessellationSource::BRepVertex(i).
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 0.0, 1.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let b = BRep::new(verts, edges, faces).unwrap();
        for i in 0..b.num_verts() as u32 {
            assert_eq!(
                b.tessellation_map().lookup(i),
                TessellationSource::BRepVertex(i),
                "vertex {i} should map to BRepVertex({i})"
            );
        }
    }

    // ----- Group 5: Error paths -----

    #[test]
    fn brep_new_face_with_too_few_edges_errors() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
        ];
        let edges = vec![BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        }];
        // 1-edge face — degenerate
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let err = BRep::new(verts, edges, faces).unwrap_err();
        match err {
            YangError::MalformedTopology(_) => {}
            other => panic!("expected MalformedTopology, got {:?}", other),
        }
    }

    #[test]
    fn brep_new_out_of_range_edge_index_errors() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        // Face references edge 99 — out of range
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 99],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let err = BRep::new(verts, edges, faces).unwrap_err();
        match err {
            YangError::MalformedTopology(_) => {}
            other => panic!("expected MalformedTopology, got {:?}", other),
        }
    }

    // ----- PR-YR1 backward-compat: existing boolean dispatch tests -----

    #[test]
    fn brep_from_mesh_as_mesh_round_trip() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.as_mesh(), &m);
    }

    #[test]
    fn brep_into_mesh_returns_wrapped() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.into_mesh(), m);
    }

    #[test]
    fn brep_counts_delegate_to_mesh() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.num_verts(), m.num_verts());
        assert_eq!(b.num_tris(), m.num_tris());
    }

    #[test]
    fn yang_error_display_non_empty() {
        for e in [
            YangError::NonManifoldInput,
            YangError::NonManifoldOutput,
            YangError::MeshBooleanFailed(Box::from("test")),
            YangError::MalformedTopology("test".to_string()),
        ] {
            let msg = format!("{}", e);
            assert!(!msg.is_empty(), "empty Display for {e:?}");
        }
    }

    #[test]
    fn yang_error_source_propagates() {
        let inner: Box<dyn Error + Send + Sync> = Box::from("inner");
        let e = YangError::MeshBooleanFailed(inner);
        let src = e.source().expect("source should be Some");
        assert_eq!(src.to_string(), "inner");
    }

    #[test]
    fn boolean_with_ok_backend() {
        // M3: boolean() consumes a LabeledArrangement. An empty arrangement
        // (0 tris) keeps nothing → empty output BRep, Ok.
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let backend = LabelMockBackend::new(empty_arrangement());
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        assert_eq!(r.num_verts(), 0);
    }

    #[test]
    fn boolean_with_err_backend() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend;
        match boolean(&a, &b, BoolOp::Union, &mock) {
            Err(YangError::MeshBooleanFailed(_)) => {}
            other => panic!("expected MeshBooleanFailed, got {:?}", other),
        }
    }

    #[test]
    fn boolean_dispatches_all_four_ops() {
        // M3: an empty arrangement is keep-set-empty for every op → Ok.
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        for op in [
            BoolOp::Union,
            BoolOp::Intersect,
            BoolOp::Subtract,
            BoolOp::Xor,
        ] {
            let backend = LabelMockBackend::new(empty_arrangement());
            assert!(boolean(&a, &b, op, &backend).is_ok(), "op {op:?}");
        }
    }

    // ----- PR-YR3: Group 1 — TessellationSource::Intersection variant -----

    #[test]
    fn intersection_variant_constructs_and_matches() {
        let s = TessellationSource::Intersection;
        match s {
            TessellationSource::Intersection => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn intersection_distinct_from_unknown() {
        assert_ne!(
            TessellationSource::Intersection,
            TessellationSource::Unknown
        );
    }

    // ----- PR-YR3: Group 2 — MATCH_TOLERANCE constant -----

    #[test]
    fn match_tolerance_is_1e_minus_9() {
        assert_eq!(MATCH_TOLERANCE, 1e-9);
    }

    // ----- PR-YR3: Group 3 — Spatial matching via mock backend -----

    /// Build a BRep with explicit topology (triangle) so its mesh has
    /// non-trivial TessellationMap entries (`BRepVertex(i)` for each i).
    fn triangle_brep() -> BRep {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        BRep::new(verts, edges, faces).unwrap()
    }

    // PR-YR3 spatial-vertex-provenance was REMOVED from production by M3
    // (production tessellation_map is now BRepVertex(i) 1:1 with the kept
    // sub-mesh). Per Manager policy (a), these tests are reworked to call
    // the now-#[cfg(test)] substitute helper `match_with_input` DIRECTLY,
    // preserving the substitute's coverage as the M4 oracle rather than
    // routing through production `boolean()`.

    #[test]
    fn boolean_input_a_verbatim_copies_a_map() {
        let a = triangle_brep();
        let b = triangle_brep();
        // Each of A's mesh verts matches input A's BRepVertex(i).
        for (i, &target) in a.as_mesh().verts.iter().enumerate() {
            let (input, src) = match_with_input(&a, &b, target);
            assert_eq!(input, Some(InputId::A), "vert {i} should match A");
            assert_eq!(
                src,
                TessellationSource::BRepVertex(i as u32),
                "output vertex {i}"
            );
        }
    }

    #[test]
    fn boolean_input_b_verbatim_copies_b_map() {
        let a = triangle_brep();
        // B has different vertices so A's spatial match fails first.
        let mut b_verts = a.vertices().to_vec();
        for v in &mut b_verts {
            v.point = Point3::new(v.point.x() + 10.0, v.point.y(), v.point.z());
        }
        let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
        for (i, &target) in b.as_mesh().verts.iter().enumerate() {
            let (input, src) = match_with_input(&a, &b, target);
            assert_eq!(input, Some(InputId::B), "vert {i} should match B");
            assert_eq!(
                src,
                TessellationSource::BRepVertex(i as u32),
                "output vertex {i} — should match input B's BRepVertex({i})"
            );
        }
    }

    #[test]
    fn boolean_all_new_coords_are_intersection() {
        let a = triangle_brep();
        let b = triangle_brep();
        // Coords far from both inputs → no match → Intersection.
        for target in [
            p(100.0, 100.0, 100.0),
            p(101.0, 100.0, 100.0),
            p(100.0, 101.0, 100.0),
        ] {
            let (input, src) = match_with_input(&a, &b, target);
            assert_eq!(input, None);
            assert_eq!(
                src,
                TessellationSource::Intersection,
                "novel coord should be Intersection"
            );
        }
    }

    #[test]
    fn boolean_mixed_match_and_intersection() {
        let a = triangle_brep();
        let b = triangle_brep();
        // 2 verts from A + 2 new coords.
        let expectations = [
            (p(0.0, 0.0, 0.0), TessellationSource::BRepVertex(0)),
            (p(1.0, 0.0, 0.0), TessellationSource::BRepVertex(1)),
            (p(99.0, 99.0, 0.0), TessellationSource::Intersection),
            (p(98.0, 98.0, 0.0), TessellationSource::Intersection),
        ];
        for (i, (target, expect)) in expectations.into_iter().enumerate() {
            let (_input, src) = match_with_input(&a, &b, target);
            assert_eq!(src, expect, "vertex {i}");
        }
    }

    // ----- PR-YR4: Group 1 — types -----

    #[test]
    fn input_id_ordering_and_derives() {
        assert!(InputId::A < InputId::B);
        assert_eq!(InputId::A, InputId::A);
        assert_ne!(InputId::A, InputId::B);
        assert_eq!(format!("{:?}", InputId::A), "A");
        assert_eq!(format!("{:?}", InputId::B), "B");
        // Copy
        let x = InputId::A;
        let y = x;
        assert_eq!(x, y);
    }

    #[test]
    fn triangle_attribution_construct_and_equality() {
        let t1 = TriangleAttribution {
            input: InputId::A,
            face: 7,
        };
        let t2 = TriangleAttribution {
            input: InputId::A,
            face: 7,
        };
        let t3 = TriangleAttribution {
            input: InputId::B,
            face: 7,
        };
        assert_eq!(t1, t2);
        assert_ne!(t1, t3);
        // Copy + accessors
        let t4 = t1;
        assert_eq!(t4.input, InputId::A);
        assert_eq!(t4.face, 7);
    }

    #[test]
    fn triangle_attribution_map_empty_and_len() {
        let m = TriangleAttributionMap::empty();
        assert_eq!(m.len(), 0);
        assert!(m.is_empty());
    }

    // ----- PR-YR4: Group 2 — algorithm via mock backend -----

    /// Two-face B-Rep where V0 is shared by F0 and F1; V1, V2 only in F0;
    /// V3, V4 only in F1. Used by tie-break + pure-input tests.
    fn two_face_shared_vertex_brep() -> BRep {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            }, // 0 — shared (F0 & F1)
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            }, // 1 — F0 only
            BRepVertex {
                point: p(1.0, 1.0, 0.0),
            }, // 2 — F0 only (moved off x-axis: was (2,0,0)) so F0 is a real triangle in z=0
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            }, // 3 — F1 only
            BRepVertex {
                point: p(0.0, 1.0, 1.0),
            }, // 4 — F1 only (moved off y-axis: was (0,2,0)) so F1 is a real triangle in x=0
        ];
        // F0 edges (triangle V0-V1-V2):
        // E0 V0→V1, E1 V1→V2, E2 V2→V0
        // F1 edges (triangle V0-V3-V4):
        // E3 V0→V3, E4 V3→V4, E5 V4→V0
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 0,
                end: 3,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 3,
                end: 4,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 4,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        // F0 lies in z=0 (normal +z); F1 now lies in x=0 (normal +x).
        let f0_plane = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        let f1_plane = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: 0.0,
        };
        let faces = vec![
            BRepFace {
                surface: f0_plane,
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            }, // F0
            BRepFace {
                surface: f1_plane,
                outer_loop: vec![3, 4, 5],
                inner_loops: Vec::new(),
                reversed: false,
            }, // F1
        ];
        BRep::new(verts, edges, faces).unwrap()
    }

    // PR-YR4 majority-vote ATTRIBUTION was REMOVED from production by M3
    // (production attributes via real LabeledArrangement labels + geometric
    // face resolution). Per Manager policy (a), these tests are reworked to
    // exercise the now-#[cfg(test)] substitute via `substitute_attribution`
    // DIRECTLY (not via production `boolean()`), preserving the substitute's
    // coverage as the M4 differential oracle.

    #[test]
    fn boolean_pure_a_attributes_to_a_faces() {
        // Pure-A: substitute over A's mesh. Each tri's verts are
        // BRepVertex(i) of A → per-vertex face incidence → majority vote
        // attributes each tri to its source face.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let attr = substitute_attribution(a.as_mesh(), &a, &b);
        assert_eq!(attr.len(), 2);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            }),
            "output tri 0 (F0 fan tri) should attribute to A's F0"
        );
        assert_eq!(
            attr.lookup(1),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 1
            }),
            "output tri 1 (F1 fan tri) should attribute to A's F1"
        );
    }

    #[test]
    fn boolean_pure_b_attributes_to_b_faces() {
        let a = two_face_shared_vertex_brep();
        // B is the same B-Rep, shifted so A's spatial match fails first.
        let mut b_verts = a.vertices().to_vec();
        for v in &mut b_verts {
            v.point = Point3::new(v.point.x() + 100.0, v.point.y(), v.point.z());
        }
        let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
        let attr = substitute_attribution(b.as_mesh(), &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::B,
                face: 0
            })
        );
        assert_eq!(
            attr.lookup(1),
            Some(TriangleAttribution {
                input: InputId::B,
                face: 1
            })
        );
    }

    #[test]
    fn boolean_all_new_coords_attribute_to_none() {
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        // A mesh with coords far from both inputs.
        let novel = Mesh::new(
            vec![
                p(1000.0, 1000.0, 1000.0),
                p(1001.0, 1000.0, 1000.0),
                p(1000.0, 1001.0, 1000.0),
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&novel, &a, &b);
        assert_eq!(attr.len(), 1);
        assert_eq!(
            attr.lookup(0),
            None,
            "all-new triangle should have None attribution"
        );
    }

    #[test]
    fn boolean_mixed_majority_wins() {
        // 2 verts match A's F0 + 1 novel → F0 attribution.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let mixed = Mesh::new(
            vec![
                p(1.0, 0.0, 0.0),       // matches a.verts[1] (F0 only)
                p(1.0, 1.0, 0.0),       // matches a.verts[2] (F0 only)
                p(1000.0, 0.0, 1000.0), // novel
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&mixed, &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            }),
            "2 A-F0-verts + 1 novel → majority F0"
        );
    }

    #[test]
    fn boolean_no_majority_returns_none() {
        // 1 A-vert + 1 B-vert + 1 novel → no majority, None.
        let a = two_face_shared_vertex_brep();
        let mut b_verts = a.vertices().to_vec();
        for v in &mut b_verts {
            v.point = Point3::new(v.point.x() + 100.0, v.point.y(), v.point.z());
        }
        let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
        let mixed = Mesh::new(
            vec![
                p(1.0, 0.0, 0.0),     // matches a.verts[1] (A, F0)
                p(101.0, 0.0, 0.0),   // matches b.verts[1] (B, F0)
                p(500.0, 500.0, 0.0), // novel
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&mixed, &a, &b);
        assert_eq!(
            attr.lookup(0),
            None,
            "1 A + 1 B + 1 novel → no 2-of-3 majority"
        );
    }

    #[test]
    fn boolean_tie_break_picks_lowest_face() {
        // Triangle (V0 shared, V1 F0-only, V3 F1-only) → candidates
        // {F0,F1}, {F0}, {F1}. Counts: F0=2, F1=2. Tie. Lowest face → F0.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let tie_mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // V0 — shared
                p(1.0, 0.0, 0.0), // V1 — F0 only
                p(0.0, 1.0, 0.0), // V3 — F1 only
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&tie_mesh, &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            }),
            "tie at count 2 between F0 and F1 → lowest face (F0)"
        );
    }

    // ----- PR-YR4: Group 3 — empty-topology degradation (substitute) -----

    #[test]
    fn boolean_both_inputs_from_mesh_all_none() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let attr = substitute_attribution(&sample_mesh(), &a, &b);
        assert_eq!(attr.len(), sample_mesh().num_tris());
        assert_eq!(
            attr.lookup(0),
            None,
            "from_mesh inputs have all-Unknown sources → all-None attribution"
        );
    }

    #[test]
    fn boolean_mixed_from_mesh_and_topologized() {
        // a has topology, b is from_mesh. Substitute over a's mesh.
        // Attribution should reflect a's per-tri face ownership.
        let a = two_face_shared_vertex_brep();
        let b = BRep::from_mesh(sample_mesh());
        let attr = substitute_attribution(a.as_mesh(), &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            })
        );
        assert_eq!(
            attr.lookup(1),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 1
            })
        );
    }

    // ----- PR-YR5: topology reconstruction -----
    //
    // `reconstruct_topology` is UNCHANGED production. Per Manager policy
    // (b), these tests previously routed through `boolean()` via the
    // boolean-only MockBackend (which M3 no longer drives); they are
    // reworked to build a `TriangleAttributionMap` via the #[cfg(test)]
    // substitute and call `reconstruct_topology` DIRECTLY — exercising the
    // same durable reconstruction logic without the removed substitute
    // production path.

    #[test]
    fn yr5_single_triangle_round_trip_produces_one_face() {
        // Pure-A on triangle_brep (1 face, 1 fan tri) → 1 face with 3
        // boundary edges + 3 vertices forming a closed cycle.
        let a = triangle_brep();
        let b = triangle_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(faces.len(), 1, "expected 1 BRepFace");
        assert_eq!(faces[0].outer_loop.len(), 3, "expected 3-edge loop");
        assert_eq!(edges.len(), 3, "expected 3 BRepEdges");
        assert_eq!(verts.len(), 3, "expected 3 BRepVertices");
        // Cycle closure
        let f = &faces[0];
        for i in 0..3 {
            let e_curr = &edges[f.outer_loop[i] as usize];
            let e_next = &edges[f.outer_loop[(i + 1) % 3] as usize];
            assert_eq!(
                e_curr.end, e_next.start,
                "cycle break at edge {i}: {} != {}",
                e_curr.end, e_next.start
            );
        }
    }

    #[test]
    fn yr5_two_face_round_trip_produces_two_faces() {
        // two_face_shared_vertex_brep has 2 triangular faces sharing only
        // V0; 2 output tris with different attributions (F0 vs F1) → 2
        // BRepFaces.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(faces.len(), 2, "expected 2 BRepFaces");
        for f in &faces {
            assert_eq!(f.outer_loop.len(), 3);
        }
    }

    #[test]
    fn yr5_disconnected_components_become_separate_faces() {
        // Two tris with the SAME attribution but NO shared vertex →
        // flood-fill leaves them as 2 patches → 2 faces. Regression guard
        // vs. naive attribution-bucketing.
        let a = triangle_brep();
        let b = triangle_brep();
        // 6 vertices = TWO copies of A's 3 verts at distinct indices.
        let dup = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // matches A.V0
                p(1.0, 0.0, 0.0), // matches A.V1
                p(0.0, 1.0, 0.0), // matches A.V2
                p(0.0, 0.0, 0.0), // duplicate matching A.V0 (different idx)
                p(1.0, 0.0, 0.0), // duplicate matching A.V1
                p(0.0, 1.0, 0.0), // duplicate matching A.V2
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        );
        let attr = substitute_attribution(&dup, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&dup, &attr, &a, &b).unwrap();
        assert_eq!(
            faces.len(),
            2,
            "disconnected same-attribution tris should be separate faces"
        );
    }

    #[test]
    fn yr5_none_attributed_tris_omitted_from_faces() {
        // tri 0 matches A's verts (Some(A, F0)); tri 1 is all novel coords
        // (None). reconstruct_topology should yield 1 face.
        let a = triangle_brep();
        let b = triangle_brep();
        let mixed = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // matches A.V0
                p(1.0, 0.0, 0.0), // matches A.V1
                p(0.0, 1.0, 0.0), // matches A.V2
                p(1000.0, 0.0, 0.0),
                p(1001.0, 0.0, 0.0),
                p(1000.0, 1.0, 0.0),
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        );
        let attr = substitute_attribution(&mixed, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&mixed, &attr, &a, &b).unwrap();
        assert_eq!(
            faces.len(),
            1,
            "None-attributed tris should not contribute faces"
        );
    }

    #[test]
    fn yr5_vertex_count_matches_mesh() {
        let a = triangle_brep();
        let b = triangle_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (verts, _e, _f) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(verts.len(), mesh.num_verts());
        for (i, v) in verts.iter().enumerate() {
            assert_eq!(v.point, mesh.verts[i]);
        }
    }

    #[test]
    fn yr5_surface_inherited_from_input() {
        let a = triangle_brep();
        let b = triangle_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(faces.len(), 1);
        assert_eq!(
            faces[0].surface,
            a.faces()[0].surface,
            "output face should inherit input A's surface"
        );
    }

    #[test]
    fn yr5_empty_input_produces_empty_face_set() {
        // Both inputs from_mesh → all-None attribution → no faces/edges.
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mesh = sample_mesh();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert!(
            faces.is_empty(),
            "all-None attribution should yield empty faces"
        );
        assert!(
            edges.is_empty(),
            "all-None attribution should yield empty edges"
        );
        // Vertices still populated 1:1 with mesh.
        assert_eq!(verts.len(), mesh.num_verts());
    }

    // ----- Stage-6 degenerate-sliver topology (spec yang_stage6_sliver_topology) -----
    //
    // Reproduces §2's measured structure at the unit level: a shared collinear
    // solid-edge chain a–c–d–b where two abutting faces subdivide it
    // DIFFERENTLY, and the arrangement keeps ZERO-AREA shim slivers along the
    // chord to stay watertight. One sliver is wound so its directed chord edge
    // DUPLICATES the real triangle's chord edge (sign-of-zero winding is
    // arbitrary) — the measured fold. Today `reconstruct_topology` dead-ends in
    // `patch_boundary_cycle` at `NonManifoldOutput`; the Stage-6 design (spec §4:
    // exclude degenerate tris from boundary derivation + loop T-subdivision) must
    // reassemble a 2-manifold output whose shared segments are each 2-covered.

    /// The shared solid edge is the y-axis (x=0, z=0): the intersection of the
    /// two abutting faces' planes z=0 (face 0, apex off +y in z=0) and x=0
    /// (face 1, apex off +y in x=0). Chain vertices a<c<d<b sit on the y-axis,
    /// exactly collinear, so every sliver along it is exactly zero-area.
    ///
    /// Vertex indices: 0=a 1=b 2=c 3=d 4=x1(face-0 apex) 5=x2(face-1 apex).
    fn sliver_fixture_mesh() -> Mesh {
        Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // 0 = a  (chain end)
                p(0.0, 3.0, 0.0), // 1 = b  (chain end)
                p(0.0, 1.0, 0.0), // 2 = c  (between a,b)
                p(0.0, 2.0, 0.0), // 3 = d  (between a,b)
                p(1.0, 1.5, 0.0), // 4 = x1 (face 0 apex, z=0 plane)
                p(0.0, 1.5, 1.0), // 5 = x2 (face 1 apex, x=0 plane)
            ],
            vec![
                // face 0 (z=0 plane, normal +z): ONE real triangle carrying the
                // whole chord b→a, plus two zero-area shim slivers wound so each
                // DUPLICATES the real directed chord edge b→a (1→0).
                [0, 4, 1], // T1 real: edges a→x1, x1→b, b→a
                [1, 0, 2], // S1 sliver: edges b→a (dup!), a→c, c→b
                [1, 0, 3], // S2 sliver: edges b→a (dup!), a→d, d→b
                // face 1 (x=0 plane, normal +x): the OTHER side subdivides the
                // chain a→c→d→b (opposite direction) via a fan from x2.
                [0, 2, 5], // edges a→c, c→x2, x2→a
                [2, 3, 5], // edges c→d, d→x2, x2→c
                [3, 1, 5], // edges d→b, b→x2, x2→d
            ],
        )
    }

    /// Attribution for `sliver_fixture_mesh`: face-0 patch = {T1,S1,S2},
    /// face-1 patch = {the three fan tris}. Built directly (in-module access to
    /// the private field) so the slivers land in face 0's patch deterministically
    /// — this is the measured N4-provenance placement (§2.3), not a geometric
    /// guess.
    fn sliver_fixture_attr() -> TriangleAttributionMap {
        let f0 = Some(TriangleAttribution {
            input: InputId::A,
            face: 0,
        });
        let f1 = Some(TriangleAttribution {
            input: InputId::A,
            face: 1,
        });
        TriangleAttributionMap {
            attributions: vec![f0, f0, f0, f1, f1, f1],
        }
    }

    /// Canonical undirected key.
    fn und(x: u32, y: u32) -> (u32, u32) {
        if x < y {
            (x, y)
        } else {
            (y, x)
        }
    }

    /// Multiset of undirected loop edges across ALL output faces, derived from
    /// each face's `outer_loop` (edge indices) via the returned edge table.
    fn loop_edge_counts(
        edges: &[BRepEdge],
        faces: &[BRepFace],
    ) -> std::collections::BTreeMap<(u32, u32), u32> {
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for f in faces {
            for &ei in &f.outer_loop {
                let e = &edges[ei as usize];
                *counts.entry(und(e.start, e.end)).or_insert(0) += 1;
            }
            for hole in &f.inner_loops {
                for &ei in hole {
                    let e = &edges[ei as usize];
                    *counts.entry(und(e.start, e.end)).or_insert(0) += 1;
                }
            }
        }
        counts
    }

    /// TARGET (spec §5 S2/S4). RED today: `reconstruct_topology` dead-ends at
    /// `NonManifoldOutput` because sliver S1's directed edge b→a duplicates
    /// real T1's b→a, unbalancing face 0's boundary walk. GREEN: slivers are
    /// excluded from boundary derivation (A) and face 0's chord is T-subdivided
    /// at c,d (B) so every shared segment is 2-covered.
    #[test]
    fn stage6_sliver_fold_reassembles_with_subdivided_chord() {
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let mesh = sliver_fixture_mesh();
        let attr = sliver_fixture_attr();

        let (_verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).expect(
            "Stage-6 sliver RED: reconstruction must succeed once zero-area slivers are \
             excluded from boundary derivation (spec §4A) — today it dead-ends at \
             NonManifoldOutput on the duplicated chord edge b→a",
        );

        // S2: both real faces survive (slivers carry no boundary of their own).
        assert_eq!(
            faces.len(),
            2,
            "expected 2 output faces (chord side + chain side)"
        );

        let counts = loop_edge_counts(&edges, &faces);

        // S4: the full chord (a,b) must NOT remain a raw loop edge — it is
        // T-subdivided at c,d.
        assert_eq!(
            counts.get(&und(0, 1)).copied().unwrap_or(0),
            0,
            "chord (a,b) must be subdivided at c,d, not carried as a single loop edge; \
             loop edges: {counts:?}"
        );
        // S4: every shared segment of the solid edge is used by exactly two
        // directed loop edges (2-manifold seam).
        for (name, key) in [("a–c", und(0, 2)), ("c–d", und(2, 3)), ("d–b", und(3, 1))] {
            assert_eq!(
                counts.get(&key).copied().unwrap_or(0),
                2,
                "shared segment {name} must be 2-covered across output loops; \
                 loop edges: {counts:?}"
            );
        }
    }

    /// S5 (spec §5): a patch made ENTIRELY of zero-area slivers cannot bound a
    /// face — it must stay loudly `NonManifoldOutput`, never silently emit a
    /// degenerate face. Passes today (the fold errors) and must remain Err
    /// through the fix (excluding all its triangles leaves no boundary).
    #[test]
    fn stage6_all_degenerate_patch_stays_loud() {
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        // A single patch of ONLY collinear slivers on the y-axis (no real tri).
        let mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // 0 = a
                p(0.0, 3.0, 0.0), // 1 = b
                p(0.0, 1.0, 0.0), // 2 = c
                p(0.0, 2.0, 0.0), // 3 = d
            ],
            vec![[1, 0, 2], [1, 0, 3]], // two zero-area slivers sharing (a,b)
        );
        let f0 = Some(TriangleAttribution {
            input: InputId::A,
            face: 0,
        });
        let attr = TriangleAttributionMap {
            attributions: vec![f0, f0],
        };
        assert!(
            reconstruct_topology(&mesh, &attr, &a, &b).is_err(),
            "an all-degenerate patch must stay loud (NonManifoldOutput) — it cannot bound a face"
        );
    }

    // ====================================================================
    // M3 — functional boolean via LabeledArrangement (Group A unit tests)
    //
    // These tests target the M3 rewire: boolean() must consume a real
    // `LabeledArrangement` from `backend.labeled_arrangement(..)`, select
    // result triangles via `keep_set(op)`, geometrically resolve each kept
    // triangle's source face (centroid-in-plane), and produce a FULL
    // attribution (every output triangle → Some). Spec:
    // specs/yang_m3_functional_boolean.md (I7 unique-face, F1/F2/F3).
    //
    // RED expectations until the Implementer lands M3:
    //   - `MeshBoolean::labeled_arrangement` trait method does not exist.
    //   - `YangError::FaceResolutionFailed { tri }` variant does not exist.
    //   - `LabeledArrangement` is not imported here yet.
    //   - current boolean() ignores labels → no full coverage.
    // ====================================================================

    use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};

    /// Mock backend that returns a hand-built `LabeledArrangement` from
    /// the (M3) `labeled_arrangement` trait method. `boolean()` is still
    /// required (object-safe trait) but is unused on the M3 path.
    struct LabelMockBackend {
        arrangement: LabeledArrangement,
    }
    impl LabelMockBackend {
        fn new(arrangement: LabeledArrangement) -> Self {
            Self { arrangement }
        }
    }
    impl MeshBoolean for LabelMockBackend {
        fn boolean(
            &self,
            _a: &Mesh,
            _b: &Mesh,
            _op: BoolOp,
        ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
            // Not exercised on the M3 path; return the arrangement mesh so
            // a stray call is at least well-formed.
            Ok(self.arrangement.mesh.clone())
        }
        // M3: the trait gains this method (default impl errors NotSupported);
        // this mock overrides it with a hand-built arrangement.
        fn labeled_arrangement(
            &self,
            _a: &Mesh,
            _b: &Mesh,
        ) -> Result<LabeledArrangement, Box<dyn Error + Send + Sync>> {
            Ok(self.arrangement.clone())
        }
    }

    /// Axis-aligned unit cube BRep at `origin` with correct OUTWARD face
    /// normals — minimal topology sufficient for geometric face
    /// resolution (centroid-in-plane). 8 verts, 24 edges, 6 quad faces.
    fn cube_brep(origin: [f64; 3]) -> BRep {
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
            [0, 1, 2, 3], // bottom (z)
            [4, 7, 6, 5], // top (z+1)
            [0, 4, 5, 1], // front (y)
            [1, 5, 6, 2], // right (x+1)
            [2, 6, 7, 3], // back (y+1)
            [3, 7, 4, 0], // left (x)
        ];
        let mut edges = Vec::new();
        let mut loops = Vec::new();
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
        let normals = [
            Vector3::new(0.0, 0.0, -1.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(0.0, -1.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(-1.0, 0.0, 0.0),
        ];
        // Plane convention n·x + d = 0. For a face on plane n·x = c the
        // offset is d = -c — WITH n the face's OUTWARD normal, so the three
        // negative-axis faces have c = -coord (e.g. bottom: n=(0,0,-1),
        // n·p = -z ⇒ d = z). The pre-2026-07-03 array had the sign flipped
        // on every face with a non-zero plane coordinate; it went unnoticed
        // because the historical bottom-quad arrangement only ever resolved
        // attribution against the origin cube's BOTTOM face (d = 0 either
        // way). The closed-shell fixture (rule-4 gate cycle) exercises all
        // six planes and unmasked it.
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
        BRep::new(verts, edges, faces).unwrap()
    }

    // N4 (1b): `BRep::new` must populate the per-triangle → owning-face map
    // (`tri_face`) 1:1 with the Stage-1 mesh triangles, with valid face indices
    // and every face owning ≥1 triangle. This is the provenance substrate that
    // lets `boolean()` attribute kept triangles to faces directly from cherchi's
    // `source` instead of geometric proximity. (The end-to-end correctness of
    // provenance attribution is covered by the full boolean suite / box fuzz,
    // which now runs provenance as the PRIMARY path.)
    #[test]
    fn brep_new_populates_tri_face_provenance() {
        let cube = cube_brep([0.0, 0.0, 0.0]);
        let tf = cube.tri_face();
        assert_eq!(
            tf.len(),
            cube.as_mesh().tris.len(),
            "tri_face must be 1:1 with the Stage-1 mesh triangles"
        );
        let nf = cube.faces().len() as u32;
        assert_eq!(nf, 6, "cube has 6 faces");
        let mut owned = vec![false; nf as usize];
        for (t, &f) in tf.iter().enumerate() {
            assert!(f < nf, "tri {t} → face {f} out of range (faces = {nf})");
            owned[f as usize] = true;
        }
        assert!(
            owned.iter().all(|&o| o),
            "every cube face must own ≥1 Stage-1 triangle"
        );

        // `from_mesh` has no Stage-1 face lineage → empty tri_face (→ geometric
        // fallback in attribution).
        let degenerate = BRep::from_mesh(cube.as_mesh().clone());
        assert!(
            degenerate.tri_face().is_empty(),
            "from_mesh BRep carries no provenance map"
        );
    }

    /// Centroid of a triangle.
    fn centroid(mesh: &Mesh, tri: [u32; 3]) -> Point3 {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        Point3::new(
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        )
    }

    /// Find the single face of `brep` whose plane contains `c` within
    /// TAU_WORK; panics if zero or >1 (the expected-attribution helper
    /// must be unambiguous for a well-posed fixture).
    fn resolve_face(brep: &BRep, c: Point3) -> u32 {
        let mut hit: Option<u32> = None;
        for (i, f) in brep.faces().iter().enumerate() {
            let Surface::Plane { normal, d } = f.surface else {
                continue;
            };
            let n = normal.as_array();
            let cc = c.as_array();
            let dist = (n[0] * cc[0] + n[1] * cc[1] + n[2] * cc[2] + d).abs();
            if dist < cad_primitives::TAU_WORK {
                assert!(hit.is_none(), "ambiguous: centroid on >1 face plane");
                hit = Some(i as u32);
            }
        }
        hit.expect("centroid lies on no face plane")
    }

    // ----- Group A.1: full attribution coverage + correctness -----

    /// Hand-built arrangement: cube A's full closed surface shell. The verts
    /// are A's exact 8 `BRepVertex` corners, so:
    /// - real-label path: each tri's centroid lies strictly inside exactly
    ///   one A face plane → I7 unique-face → full Some(A, face) attribution;
    /// - every patch boundary closes (per-face manifold cycles) and the
    ///   whole shell is watertight, matching the closed kept mesh a real
    ///   boolean produces;
    /// - the verts coincide with A's `BRepVertex`es, so the M4 substitute's
    ///   spatial matching also resolves each tri to its cube face
    ///   (vertex-face incidence majority), letting the differential oracle
    ///   agree.
    ///
    /// All `inside` all-false ⇒ all 12 tris kept by Union.
    fn arrangement_a_cube_shell() -> LabeledArrangement {
        // The full unit-cube SURFACE of `cube_brep([0,0,0])`: 12 outward-wound
        // tris, 2 per face. Historically this fixture was A's bottom quad only
        // (an open 2-tri sheet) — a mock shape no real boolean produces. The
        // 2026-07-03 gate cycle (spec `yang_kept_mesh_manifold_gate`, aborted
        // per P10 — see its §2b) closed it to model a real kept mesh; the
        // closed form is kept: it is strictly more faithful and it unmasked
        // the `cube_brep` plane-offset sign bug below. All consuming
        // assertions are computed FROM the fixture (keep-set count, geometric
        // face resolve, majority vote), so their intent is unchanged.
        let verts = vec![
            p(0.0, 0.0, 0.0), // 0
            p(1.0, 0.0, 0.0), // 1
            p(1.0, 1.0, 0.0), // 2
            p(0.0, 1.0, 0.0), // 3
            p(0.0, 0.0, 1.0), // 4
            p(1.0, 0.0, 1.0), // 5
            p(1.0, 1.0, 1.0), // 6
            p(0.0, 1.0, 1.0), // 7
        ];
        // Outward winding per face (−z, +z, −y, +y, −x, +x); every directed
        // edge pairs with its reverse ⇒ watertight 2-manifold (χ = 2).
        let tris = vec![
            [0u32, 3, 2],
            [0, 2, 1], // bottom z=0
            [4, 5, 6],
            [4, 6, 7], // top z=1
            [0, 1, 5],
            [0, 5, 4], // front y=0
            [2, 3, 7],
            [2, 7, 6], // back y=1
            [0, 4, 7],
            [0, 7, 3], // left x=0
            [1, 2, 6],
            [1, 6, 5], // right x=1
        ];
        let mesh = Mesh::new(verts, tris);
        // All on A's surface (solid 0), none on B; inside all-false ⇒ Union keeps.
        let surface = vec![vec![LaInputId(0)]; 12];
        let inside = vec![vec![false, false]; 12];
        let patch = vec![0u32, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5];
        LabeledArrangement {
            mesh,
            surface,
            inside,
            patch,
            source: Vec::new(),
            num_inputs: 2,
        }
    }

    #[test]
    fn m3_union_full_attribution_coverage() {
        // I7 + full-coverage: every kept output triangle resolves to Some.
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let backend = LabelMockBackend::new(la);
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();

        let attr = r.triangle_attribution();
        assert_eq!(
            attr.len(),
            r.num_tris(),
            "attribution length must equal output triangle count"
        );
        assert!(r.num_tris() > 0, "expected non-empty kept sub-mesh");
        for t in 0..attr.len() as u32 {
            assert!(
                attr.lookup(t).is_some(),
                "M3 requires FULL attribution: tri {t} is None (skeleton, not closed)"
            );
        }
    }

    #[test]
    fn m3_union_attribution_matches_geometric_face() {
        // F1: each kept tri attributes to the unique A-face plane its
        // centroid lies on (one of the cube shell's six faces).
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let mesh = la.mesh.clone();
        let backend = LabelMockBackend::new(la);
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        let attr = r.triangle_attribution();

        // The kept sub-mesh re-indexes verts but preserves triangle geometry.
        // For each output triangle, its centroid must lie on A's face that
        // the attribution names.
        for t in 0..r.num_tris() as u32 {
            let got = attr.lookup(t).expect("full coverage");
            assert_eq!(got.input, InputId::A, "tris are all on solid A's surface");
            let c = centroid(r.as_mesh(), r.as_mesh().tris[t as usize]);
            let expected_face = resolve_face(&a, c);
            assert_eq!(
                got.face, expected_face,
                "tri {t}: attributed face {} != geometric face {}",
                got.face, expected_face
            );
        }
        let _ = mesh; // keep capture explicit
    }

    #[test]
    fn m3_kept_submesh_is_keep_set_count() {
        // Stage 4: the kept sub-mesh must contain exactly keep_set(op) tris.
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let expected_kept = la.keep_set(BoolOp::Union).len();
        let backend = LabelMockBackend::new(la);
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        assert_eq!(
            r.num_tris(),
            expected_kept,
            "output mesh tri count must equal keep_set(Union) count"
        );
    }

    // ----- Group A.2: F2 / F3 error cases (P9: loud, never None) -----

    #[test]
    fn m3_coplanar_surface_len_two_errors_f2() {
        // F2: a kept tri whose surface label names BOTH solids (coplanar
        // overlap, len==2) → FaceResolutionFailed (out of scope, M8).
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B must NOT be input-coplanar with A (the gate fires
        // first, before the backend); the F2 condition under test is the
        // ARRANGEMENT-level multi-solid surface label, which the mock
        // fabricates below regardless of the input geometry.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let verts = vec![p(0.0, 0.0, 0.0), p(0.5, 0.0, 0.0), p(0.0, 0.5, 0.0)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            // surface names BOTH A and B (coplanar multi-solid) — F2.
            surface: vec![vec![LaInputId(0), LaInputId(1)]],
            inside: vec![vec![false, false]], // kept by Union
            patch: vec![0],
            source: Vec::new(),
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "F2 should name the offending tri index");
            }
            other => panic!("expected FaceResolutionFailed (F2), got {other:?}"),
        }
    }

    #[test]
    fn m3_centroid_off_all_planes_errors_f3() {
        // F3: a kept tri on solid A's surface whose centroid lies on NO
        // A-face plane → FaceResolutionFailed (loud, never None).
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        // Triangle floating at z=0.5 (interior; off every cube face plane).
        let verts = vec![p(0.25, 0.25, 0.5), p(0.5, 0.25, 0.5), p(0.25, 0.5, 0.5)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            surface: vec![vec![LaInputId(0)]], // claims solid A's surface
            inside: vec![vec![false, false]],  // kept by Union
            patch: vec![0],
            source: Vec::new(),
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "F3 should name the offending tri index");
            }
            other => panic!("expected FaceResolutionFailed (F3), got {other:?}"),
        }
    }

    /// N4 retirement (task #53, spec `specs/n4_retire_stage6_fallback.md`):
    /// on a provenance-CARRYING arrangement, a triangle whose provenance
    /// MISSES must fail loudly — never a silent geometric guess. The
    /// triangle lies ON A's bottom face plane, so the old geometric
    /// fallback would happily (mis)attribute it; the miss is a
    /// `NoSourceEntry` (its source names only input B while the surface
    /// label says A).
    #[test]
    fn n4_provenance_miss_errors_loudly() {
        let a = cube_brep([0.0, 0.0, 0.0]);
        let b = cube_brep([0.5, 0.3, 0.4]);
        let verts = vec![p(0.1, 0.1, 0.0), p(0.4, 0.1, 0.0), p(0.1, 0.4, 0.0)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            surface: vec![vec![LaInputId(0)]], // claims solid A's surface…
            inside: vec![vec![false, false]],  // kept by Union
            patch: vec![0],
            // …but provenance names only input B: a NoSourceEntry miss.
            source: vec![vec![(LaInputId(1), 0)]],
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "the miss should name the offending tri");
            }
            other => panic!("provenance miss must be loud (FaceResolutionFailed), got {other:?}"),
        }
    }

    /// N4 retirement: the `NoMap` miss reason (parent index beyond the
    /// input's `tri_face` map) is equally loud.
    #[test]
    fn n4_provenance_out_of_range_parent_errors_loudly() {
        let a = cube_brep([0.0, 0.0, 0.0]);
        let b = cube_brep([0.5, 0.3, 0.4]);
        let verts = vec![p(0.1, 0.1, 0.0), p(0.4, 0.1, 0.0), p(0.1, 0.4, 0.0)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            surface: vec![vec![LaInputId(0)]],
            inside: vec![vec![false, false]],
            patch: vec![0],
            // Parent index far beyond A's 12-triangle Stage-1 map: NoMap.
            source: vec![vec![(LaInputId(0), 9999)]],
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "the miss should name the offending tri");
            }
            other => panic!("provenance miss must be loud (FaceResolutionFailed), got {other:?}"),
        }
    }

    // ----- Group C: M4 differential oracle (real label vs substitute) -----

    #[test]
    fn m4_real_label_and_substitute_agree_on_pure_a() {
        // The (now test-only) substitute attribution and the real-label
        // path must agree on a pure-A fixture. Disagreement localizes a
        // label-path bug. The substitute is exercised here via the M4
        // test-only helpers (`match_with_input`/`face_candidates`/
        // `majority_vote`), which the Implementer relocates into the test
        // module. If those are not yet callable, this is a compile RED.
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let mesh = la.mesh.clone();
        let backend = LabelMockBackend::new(la);

        // Real-label path:
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        let attr = r.triangle_attribution();

        // Substitute path (vertex provenance + majority vote) over the
        // SAME kept sub-mesh:
        for t in 0..r.num_tris() {
            let tri = r.as_mesh().tris[t];
            let mut inputs = [None; 3];
            let mut sources = [TessellationSource::Unknown; 3];
            for (k, &vi) in tri.iter().enumerate() {
                let target = r.as_mesh().verts[vi as usize];
                let (inp, src) = match_with_input(&a, &b, target);
                inputs[k] = inp;
                sources[k] = src;
            }
            let sets = [
                face_candidates(inputs[0], sources[0], &a, &b),
                face_candidates(inputs[1], sources[1], &a, &b),
                face_candidates(inputs[2], sources[2], &a, &b),
            ];
            let substitute = majority_vote(&sets);
            let real = attr.lookup(t as u32);
            assert_eq!(
                real, substitute,
                "M4 differential: real-label tri {t} attribution {real:?} \
                 disagrees with substitute {substitute:?}"
            );
        }
        let _ = mesh;
    }

    // ───────────────────────────────────────────────────────────────────
    // PR-M8 disc-rim crossing — rim-override Stage-1 unit tests
    // ───────────────────────────────────────────────────────────────────

    /// A z-axis cylinder B-Rep: bottom cap (−z) at `z=base`, top cap (+z) at
    /// `z=base+h`, seam at +x, radius `r`. Two full-circle rims + one seam
    /// segment (mirrors the m8 test fixture).
    fn rt_cylinder(base: f64, h: f64, r: f64) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
        let v0 = Point3::new(r, 0.0, base);
        let v1 = Point3::new(r, 0.0, base + h);
        let verts = vec![BRepVertex { point: v0 }, BRepVertex { point: v1 }];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, base),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, base + h),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
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
                surface: Surface::Cylinder {
                    axis_point: Point3::new(0.0, 0.0, base),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: base,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -(base + h),
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        (verts, edges, faces)
    }

    /// An EMPTY rim-override map yields byte-identical verts AND tris to the
    /// plain `stage1_tessellate` for a plain cylinder — the uniform-rim path is
    /// 100% untouched.
    #[test]
    fn rim_override_empty_is_byte_identical() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
        let empty: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
        let overridden = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &empty, None)
            .expect("empty");
        assert_eq!(
            plain.verts.len(),
            overridden.verts.len(),
            "empty override must not add verts"
        );
        for (a, b) in plain.verts.iter().zip(&overridden.verts) {
            assert_eq!(a.as_array(), b.as_array(), "verts must be byte-identical");
        }
        assert_eq!(plain.tris, overridden.tris, "tris must be byte-identical");
    }

    /// Inserting a crossing point on BOTH rims (at the same geometric azimuth):
    /// both points appear bit-exactly on the top AND bottom rim rings, and the
    /// resulting cylinder mesh (caps + lateral) stays a closed 2-manifold.
    #[test]
    fn rim_override_inserts_into_both_rims_no_t_junction() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        // A point on each rim at azimuth 0.3 rad (NOT a uniform sample): radius
        // 0.5 in the rim's plane.
        let az = 0.3_f64;
        let (s, c) = az.sin_cos();
        let bottom_pt = Point3::new(0.5 * c, 0.5 * s, 0.0);
        let top_pt = Point3::new(0.5 * c, 0.5 * s, 1.0);
        let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        ov.insert(0, vec![bottom_pt]); // bottom rim = circle edge 0
        ov.insert(1, vec![top_pt]); // top rim = circle edge 1
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None)
            .expect("dual-rim override");

        // Both inserted points present bit-exactly in the vertex pool.
        let has = |p: Point3| t.verts.iter().any(|q| q.as_array() == p.as_array());
        assert!(has(bottom_pt), "bottom crossing point missing from mesh");
        assert!(has(top_pt), "top crossing point missing from mesh");

        // The mesh stays a closed 2-manifold (every undirected edge shared by
        // exactly two triangles).
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for tri in &t.tris {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        assert!(!counts.is_empty());
        assert!(
            counts.values().all(|&c| c == 2),
            "dual-rim override must keep the cylinder a closed 2-manifold"
        );
    }

    /// KV14 Slice A (spec `yang_stage1_curved_holed_patch`): a cylinder lateral
    /// PARTIAL patch (2 sweep arcs + 2 rulings) carrying an interior hole (an
    /// on-surface inner loop) must tessellate via the unroll+CDT path so the
    /// hole is EXCLUDED from the mesh. The pre-Slice-A partial-patch strip
    /// ignored `inner_loops` and paved over the hole (RED before the fix).
    #[test]
    fn lateral_holed_patch_excludes_hole() {
        use std::f64::consts::PI;
        let r = 1.0_f64;
        let on = |theta: f64, z: f64| Point3::new(r * theta.cos(), r * theta.sin(), z);
        // Sector theta in [0, PI], z in [0, 2] (a bounded patch with a clean
        // angular gap for the branch cut).
        let a = on(0.0, 0.0); // V0
        let b = on(PI, 0.0); // V1
        let c = on(PI, 2.0); // V2
        let d = on(0.0, 2.0); // V3
                              // Interior triangular hole around theta=PI/2, z=1 (all verts on-surface).
        let h0 = on(PI / 2.0 - 0.4, 0.7); // V4
        let h1 = on(PI / 2.0 + 0.4, 0.7); // V5
        let h2 = on(PI / 2.0, 1.3); // V6
        let verts = [a, b, c, d, h0, h1, h2]
            .into_iter()
            .map(|point| BRepVertex { point })
            .collect::<Vec<_>>();
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            }, // bottom arc A->B (CCW around +z, sweep PI)
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            }, // ruling B->C
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 2.0),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            }, // top arc C->D (CCW around -z, sweep PI back over [0,PI])
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            }, // ruling D->A
            BRepEdge {
                start: 4,
                end: 5,
                curve: Curve::LineSegment,
            }, // hole H0->H1
            BRepEdge {
                start: 5,
                end: 6,
                curve: Curve::LineSegment,
            }, // hole H1->H2
            BRepEdge {
                start: 6,
                end: 4,
                curve: Curve::LineSegment,
            }, // hole H2->H0
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: vec![vec![4, 5, 6]],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("holed lateral tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Param unroll (u = r*theta, v = axial); the axis is +z through origin,
        // so theta = atan2(y, x) is continuous over the [0, PI] sector.
        let param = |p: [f64; 3]| -> (f64, f64) { (r * p[1].atan2(p[0]), p[2]) };
        let huv = [
            param(h0.as_array()),
            param(h1.as_array()),
            param(h2.as_array()),
        ];
        let inside_hole = |u: f64, v: f64| -> bool {
            let (x0, y0) = huv[0];
            let (x1, y1) = huv[1];
            let (x2, y2) = huv[2];
            let d1 = (u - x1) * (y0 - y1) - (x0 - x1) * (v - y1);
            let d2 = (u - x2) * (y1 - y2) - (x1 - x2) * (v - y2);
            let d3 = (u - x0) * (y2 - y0) - (x2 - x0) * (v - y0);
            let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
            let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
            !(has_neg && has_pos)
        };

        // Oracle 1: no triangle centroid lies inside the hole.
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let (u, v) = param(cen);
            assert!(
                !inside_hole(u, v),
                "triangle centroid (u={u}, v={v}) lies inside the hole — hole was paved over"
            );
        }

        // Oracle 2: watertight patch — each hole boundary edge borders exactly
        // one triangle (a mesh boundary), never two.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let find = |p: [f64; 3]| -> u32 {
            t.verts
                .iter()
                .position(|q| {
                    let a = q.as_array();
                    (a[0] - p[0]).abs() < 1e-9
                        && (a[1] - p[1]).abs() < 1e-9
                        && (a[2] - p[2]).abs() < 1e-9
                })
                .map(|i| i as u32)
                .expect("hole vertex present in mesh")
        };
        let (gh0, gh1, gh2) = (
            find(h0.as_array()),
            find(h1.as_array()),
            find(h2.as_array()),
        );
        for (x, y) in [(gh0, gh1), (gh1, gh2), (gh2, gh0)] {
            let cnt = undirected.get(&(x.min(y), x.max(y))).copied().unwrap_or(0);
            assert_eq!(
                cnt, 1,
                "hole boundary edge ({x},{y}) must be a mesh boundary (appear once), got {cnt}"
            );
        }

        // Oracle 3: every triangle faces radially outward (reversed = false).
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            // radial = centroid projected off the +z axis through origin.
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(dot > 0.0, "triangle must face radially outward, dot={dot}");
        }
    }

    /// KV14 Slice E (spec `yang_stage1_curved_holed_patch`): a CONE lateral
    /// PARTIAL patch (a frustum sector) carrying an interior hole re-enters via
    /// the shared unroll+CDT path (cone isometric development), and the hole is
    /// KV14 Slice F: a POLOIDAL PERIODIC TORUS BAND (the corpus torus-boolean
    /// shape — probe KV14_TORUS_PROBE) re-enters Stage 1 via `tessellate_torus_band`
    /// → `tessellate_torus_patch`. Two full profile circles (at θ0, θ1) bound the
    /// band, one labeled outer, the opposite inner. A torus is not ruled in the
    /// toroidal direction, so the UV-CDT must sample interior toroidal rings onto
    /// the surface. Exact-area oracle: a full-φ band over Δθ has developable area
    /// 2π·R·rm·Δθ; watertightness oracle catches a cracked seam.
    #[test]
    fn torus_poloidal_band_two_encircling_profiles() {
        use std::f64::consts::PI;
        let major = 3.0_f64;
        let minor = 1.0_f64;
        let on = |theta: f64, phi: f64| {
            let rad = major + minor * phi.cos();
            Point3::new(rad * theta.cos(), rad * theta.sin(), minor * phi.sin())
        };
        let n = 24usize;
        let (th0, th1) = (0.2_f64, 1.4_f64);
        let mut verts: Vec<BRepVertex> = Vec::new();
        let circle_at = |theta: f64, verts: &mut Vec<BRepVertex>| -> Vec<u32> {
            let base = verts.len() as u32;
            for k in 0..n {
                let phi = 2.0 * PI * (k as f64) / (n as f64);
                verts.push(BRepVertex {
                    point: on(theta, phi),
                });
            }
            (0..n as u32).map(|k| base + k).collect()
        };
        let ring0 = circle_at(th0, &mut verts);
        let ring1 = circle_at(th1, &mut verts);
        let mut edges: Vec<BRepEdge> = Vec::new();
        let loop_of = |ring: &[u32], edges: &mut Vec<BRepEdge>| -> Vec<u32> {
            let base = edges.len() as u32;
            for k in 0..ring.len() {
                edges.push(BRepEdge {
                    start: ring[k],
                    end: ring[(k + 1) % ring.len()],
                    curve: Curve::LineSegment,
                });
            }
            (0..ring.len() as u32).map(|k| base + k).collect()
        };
        // Outer winds +φ; the inner (a hole boundary) winds −φ — opposite
        // poloidal wrap, as a real face's outer/inner loops are oriented (the
        // band seam bridge requires the two profiles wrap oppositely).
        let ring1_rev: Vec<u32> = ring1.iter().rev().copied().collect();
        let outer = loop_of(&ring0, &mut edges);
        let inner = loop_of(&ring1_rev, &mut edges);
        let faces = vec![BRepFace {
            surface: Surface::Torus {
                center: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                major_radius: major,
                minor_radius: minor,
            },
            outer_loop: outer,
            inner_loops: vec![inner],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("torus band tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        let tri_area = |tri: &[u32; 3]| -> f64 {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let nx = e1[1] * e2[2] - e1[2] * e2[1];
            let ny = e1[2] * e2[0] - e1[0] * e2[2];
            let nz = e1[0] * e2[1] - e1[1] * e2[0];
            0.5 * (nx * nx + ny * ny + nz * nz).sqrt()
        };
        let area: f64 = t.tris.iter().map(tri_area).sum();
        let band = 2.0 * PI * major * minor * (th1 - th0);
        assert!(
            area > 0.97 * band && area <= band + 1e-9,
            "torus band area {area} must fill 2π·R·rm·Δθ (≈{band}, inscribed)"
        );

        // Watertight: every undirected edge is shared by exactly 2 triangles OR
        // lies on the two profile-circle boundaries (a shared-with-cap rim). A
        // cracked seam would leave interior edges with count 1.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let theta_of = |g: u32| {
            let p = t.verts[g as usize].as_array();
            p[1].atan2(p[0])
        };
        for (&(x, y), &c) in &undirected {
            assert!(c <= 2, "edge ({x},{y}) covered {c} times (fold)");
            if c == 1 {
                // Only profile-rim edges (both ends at θ0 or both at θ1) may be
                // single-count (they border the adjacent cap, absent here).
                let (tx, ty) = (theta_of(x), theta_of(y));
                let on_rim = ((tx - th0).abs() < 1e-6 && (ty - th0).abs() < 1e-6)
                    || ((tx - th1).abs() < 1e-6 && (ty - th1).abs() < 1e-6);
                assert!(
                    on_rim,
                    "interior edge ({x},{y}) is a boundary — cracked seam in the band"
                );
            }
        }
    }

    /// EXCLUDED. Covers the cone `inner_loops` → CDT route (P4).
    #[test]
    fn cone_holed_patch_excludes_hole() {
        use std::f64::consts::PI;
        let tan_a = 0.5_f64;
        let half_angle = tan_a.atan();
        let (sa, ca) = (half_angle.sin(), half_angle.cos());
        let on = |theta: f64, z: f64| {
            let rr = z * tan_a;
            Point3::new(rr * theta.cos(), rr * theta.sin(), z)
        };
        // Sector theta in [0, PI], z in [1, 3] (a bounded frustum patch).
        let z0 = 1.0_f64;
        let z1 = 3.0_f64;
        let a = on(0.0, z0); // V0
        let b = on(PI, z0); // V1
        let c = on(PI, z1); // V2
        let d = on(0.0, z1); // V3
                             // Interior triangular hole around theta=PI/2, z=2 (on-surface).
        let h0 = on(PI / 2.0 - 0.4, 1.6); // V4
        let h1 = on(PI / 2.0 + 0.4, 1.6); // V5
        let h2 = on(PI / 2.0, 2.4); // V6
        let verts = [a, b, c, d, h0, h1, h2]
            .into_iter()
            .map(|point| BRepVertex { point })
            .collect::<Vec<_>>();
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, z0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: z0 * tan_a,
                },
            }, // bottom arc A->B
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            }, // ruling B->C
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, z1),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: z1 * tan_a,
                },
            }, // top arc C->D
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            }, // ruling D->A
            BRepEdge {
                start: 4,
                end: 5,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 5,
                end: 6,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 6,
                end: 4,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cone {
                apex: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: vec![vec![4, 5, 6]],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("holed cone tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Cone isometric development (ℓ = v/cosα, ψ = θ·sinα) — the same 2D
        // layout the tessellator uses (up to the branch-cut rotation, which does
        // not affect a point-in-triangle test).
        let param = |p: [f64; 3]| -> (f64, f64) {
            let ell = p[2].abs() / ca;
            let psi = p[1].atan2(p[0]) * sa;
            (ell * psi.cos(), ell * psi.sin())
        };
        let huv = [
            param(h0.as_array()),
            param(h1.as_array()),
            param(h2.as_array()),
        ];
        let inside_hole = |u: f64, v: f64| -> bool {
            let (x0, y0) = huv[0];
            let (x1, y1) = huv[1];
            let (x2, y2) = huv[2];
            let d1 = (u - x1) * (y0 - y1) - (x0 - x1) * (v - y1);
            let d2 = (u - x2) * (y1 - y2) - (x1 - x2) * (v - y2);
            let d3 = (u - x0) * (y2 - y0) - (x2 - x0) * (v - y0);
            let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
            let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
            !(has_neg && has_pos)
        };

        // Oracle 1: no triangle centroid lies inside the hole.
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let (u, v) = param(cen);
            assert!(
                !inside_hole(u, v),
                "cone triangle centroid (u={u}, v={v}) lies inside the hole — hole paved over"
            );
        }

        // Oracle 2: watertight — each hole boundary edge borders exactly one tri.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let find = |p: [f64; 3]| -> u32 {
            t.verts
                .iter()
                .position(|q| {
                    let a = q.as_array();
                    (a[0] - p[0]).abs() < 1e-9
                        && (a[1] - p[1]).abs() < 1e-9
                        && (a[2] - p[2]).abs() < 1e-9
                })
                .map(|i| i as u32)
                .expect("hole vertex present in mesh")
        };
        let (gh0, gh1, gh2) = (
            find(h0.as_array()),
            find(h1.as_array()),
            find(h2.as_array()),
        );
        for (x, y) in [(gh0, gh1), (gh1, gh2), (gh2, gh0)] {
            let cnt = undirected.get(&(x.min(y), x.max(y))).copied().unwrap_or(0);
            assert_eq!(
                cnt, 1,
                "hole boundary edge ({x},{y}) must be a mesh boundary (once), got {cnt}"
            );
        }

        // Oracle 3: every triangle faces radially outward (reversed = false).
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(
                dot > 0.0,
                "cone triangle must face radially outward, dot={dot}"
            );
        }
    }

    /// KV14 Slice B (spec `yang_stage1_curved_holed_patch`): a PERIODIC
    /// cylinder-wall strip whose boundary loops each ENCIRCLE the axis (a full
    /// 2π rim / intersection ring, |Σ Δθ| ≈ 2π). Real boolean outputs represent
    /// a windowed cylinder wall this way — one encircling loop labeled `outer`,
    /// the opposite rim labeled `inner`. Slice A's polygon-with-holes model
    /// unrolls a full rim to a zero-area horizontal line, so the CDT fails
    /// outright (RED before Slice B). Slice B classifies the two encircling
    /// loops as the strip's v-boundaries and lays them into ONE simple ribbon.
    #[test]
    fn periodic_strip_two_encircling_rims() {
        let r = 1.0_f64;
        let h = 2.0_f64;
        // Square cross-section sampling: 4 azimuths per rim (θ = 0, π/2, π,
        // 3π/2) → the exact lateral area is a 4-gon prism wall = 4·(r√2)·h.
        let bottom = [
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(-1.0, 0.0, 0.0),
            Point3::new(0.0, -1.0, 0.0),
        ];
        let top = [
            Point3::new(1.0, 0.0, h),
            Point3::new(0.0, 1.0, h),
            Point3::new(-1.0, 0.0, h),
            Point3::new(0.0, -1.0, h),
        ];
        let verts = bottom
            .iter()
            .chain(top.iter())
            .map(|&point| BRepVertex { point })
            .collect::<Vec<_>>();
        let arc = |start: u32, end: u32, z: f64| BRepEdge {
            start,
            end,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
        };
        // Bottom rim (outer): 4 CCW arcs winding +2π. Top rim (inner): likewise.
        let edges = vec![
            arc(0, 1, 0.0),
            arc(1, 2, 0.0),
            arc(2, 3, 0.0),
            arc(3, 0, 0.0),
            arc(4, 5, h),
            arc(5, 6, h),
            arc(6, 7, h),
            arc(7, 4, h),
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: vec![vec![4, 5, 6, 7]],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("periodic strip tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Oracle 1: total lateral area equals the exact 4-gon prism wall
        // (proves the strip covers the FULL 2π, no seam gap, no double cover).
        let tri_area = |tri: &[u32; 3]| -> f64 {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        };
        let area: f64 = t.tris.iter().map(tri_area).sum();
        // The strip is inscribed in the true cylinder wall (2π·r·h), so its area
        // approaches that from BELOW as sampling refines. A missing seam wedge
        // drops the area by a whole facet column (≈10% at this sampling), so a
        // 97% floor cleanly separates a full wrap from a gap — independent of
        // the exact arc-sample count.
        let full_wall = 2.0 * std::f64::consts::PI * r * h;
        assert!(
            area > 0.97 * full_wall && area <= full_wall + 1e-9,
            "strip area {area} must fill the full 2π wall (≈{full_wall}, inscribed)"
        );

        // Oracle 2: watertight ribbon — every mesh-boundary (count-1) edge lies
        // ENTIRELY on a rim (both endpoints at z=0 or both at z=h), and no edge
        // is covered more than twice. A seam gap leaves a vertical boundary edge
        // spanning z=0→z=h; a fold double-covers. Sampling-independent.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let on_rim = |z: f64| z.abs() < 1e-9 || (z - h).abs() < 1e-9;
        let mut boundary_edges = 0usize;
        for (&(x, y), &c) in &undirected {
            assert!(
                c <= 2,
                "edge ({x},{y}) covered {c} times (fold/double cover)"
            );
            if c == 1 {
                boundary_edges += 1;
                let zx = t.verts[x as usize].as_array()[2];
                let zy = t.verts[y as usize].as_array()[2];
                assert!(
                    on_rim(zx) && on_rim(zy) && (zx - zy).abs() < 1e-9,
                    "boundary edge ({x},{y}) at z=({zx},{zy}) is not a rim edge — seam gap"
                );
            }
        }
        assert!(boundary_edges > 0, "the tube strip has open rims");

        // Oracle 3: every triangle faces radially outward.
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(dot > 0.0, "triangle must face radially outward, dot={dot}");
        }
    }

    /// KV14 ellipse-arc re-entry (spec `kv14_ellipse_arc_reentry`): a PLANAR
    /// face whose loop mixes LineSegment + one `Curve::Ellipse` ARC (the
    /// oblique plane∩cylinder section a prior boolean leaves on a cap —
    /// R0006/F0076's planar-loop sub-kind) re-enters Stage 1 through the
    /// generalized curved CDT. The ellipse chain pre-pass samples the arc at
    /// the circle chord rule on `major_radius`; the sector tessellates
    /// watertight with the chorded area approaching the analytic sector area
    /// `½·a·b·Δt` from below.
    #[test]
    fn planar_ellipse_sector_reenters_stage1() {
        use std::f64::consts::FRAC_PI_2;
        let a = 2.0_f64; // major radius (along +x)
        let b = 1.0_f64; // minor radius (along +y)
                         // Quarter sector: ellipse arc from t=0 (2,0,0) to t=π/2 (0,1,0)
                         // (sweep π/2 < π — the guaranteed-minor-arc input convention), then
                         // two straight legs through the center.
        let verts = vec![
            BRepVertex {
                point: Point3::new(a, 0.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(0.0, b, 0.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 0.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::Ellipse {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    major_axis: Vector3::new(1.0, 0.0, 0.0),
                    major_radius: a,
                    minor_radius: b,
                },
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: vec![],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("ellipse sector tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Oracle 1 (on-surface): every vertex lies in the z=0 plane, and every
        // NON-endpoint vertex sourced from the ellipse edge satisfies the
        // ellipse implicit (x/a)² + (y/b)² = 1.
        let mut ellipse_steiner = 0usize;
        for (i, v) in t.verts.iter().enumerate() {
            let p = v.as_array();
            assert!(p[2].abs() < 1e-12, "vertex {i} off the sector plane");
            if let TessellationSource::BRepEdge { edge: 0, .. } = t.sources[i] {
                let r = (p[0] / a).powi(2) + (p[1] / b).powi(2);
                assert!(
                    (r - 1.0).abs() < 1e-9,
                    "ellipse sample {i} off the ellipse: implicit residual {r}"
                );
                ellipse_steiner += 1;
            }
        }
        assert!(
            ellipse_steiner >= 1,
            "the arc must be subdivided (chord rule), got {ellipse_steiner} interior samples"
        );

        // Oracle 2 (area): the chorded sector area approaches the analytic
        // `½·a·b·Δt` from BELOW (inscribed).
        let analytic = 0.5 * a * b * FRAC_PI_2;
        let area: f64 = t
            .tris
            .iter()
            .map(|tri| {
                let p0 = t.verts[tri[0] as usize].as_array();
                let p1 = t.verts[tri[1] as usize].as_array();
                let p2 = t.verts[tri[2] as usize].as_array();
                let e1 = [p1[0] - p0[0], p1[1] - p0[1]];
                let e2 = [p2[0] - p0[0], p2[1] - p0[1]];
                0.5 * (e1[0] * e2[1] - e1[1] * e2[0]).abs()
            })
            .sum();
        assert!(
            area <= analytic + 1e-9 && area > 0.985 * analytic,
            "sector area {area} vs analytic {analytic}"
        );

        // Oracle 3 (watertight cover): every undirected mesh edge is covered
        // once (boundary) or twice (interior) — no T-junction, no fold.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        for (&(x, y), &c) in &undirected {
            assert!(c <= 2, "edge ({x},{y}) covered {c} times");
        }
    }

    /// KV14 ellipse-arc re-entry: a planar cap bounded by a single FULL
    /// `Curve::Ellipse` loop (`start == end` — the complete oblique section)
    /// tessellates through the same chain + CDT path, area → π·a·b from below.
    #[test]
    fn planar_full_ellipse_cap_reenters_stage1() {
        let a = 2.0_f64;
        let b = 1.0_f64;
        let verts = vec![BRepVertex {
            point: Point3::new(a, 0.0, 0.0),
        }];
        let edges = vec![BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Ellipse {
                center: Point3::new(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                major_axis: Vector3::new(1.0, 0.0, 0.0),
                major_radius: a,
                minor_radius: b,
            },
        }];
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: vec![],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("full ellipse cap tessellation");
        let analytic = std::f64::consts::PI * a * b;
        let area: f64 = t
            .tris
            .iter()
            .map(|tri| {
                let p0 = t.verts[tri[0] as usize].as_array();
                let p1 = t.verts[tri[1] as usize].as_array();
                let p2 = t.verts[tri[2] as usize].as_array();
                let e1 = [p1[0] - p0[0], p1[1] - p0[1]];
                let e2 = [p2[0] - p0[0], p2[1] - p0[1]];
                0.5 * (e1[0] * e2[1] - e1[1] * e2[0]).abs()
            })
            .sum();
        assert!(
            area <= analytic + 1e-9 && area > 0.985 * analytic,
            "cap area {area} vs analytic {analytic}"
        );
    }

    /// KV14 ellipse-arc re-entry (curved-lateral sub-kind): a cylinder wall
    /// bounded below by a full circle rim and above by the full OBLIQUE
    /// ellipse (`plane ∩ cylinder`, R0095's vocabulary) routes through the
    /// holed-CDT periodic strip: both loops encircle the axis, the ellipse
    /// chain samples lie exactly ON the cylinder, and the wall area
    /// approaches `r·∫(h + k·cosθ)dθ = 2π·r·h` from below.
    #[test]
    fn lateral_oblique_ellipse_tube_reenters_stage1() {
        let r = 1.0_f64;
        let h = 2.0_f64; // ellipse-plane height at the axis
        let k = 0.5_f64; // slope: top plane z = h + k·x
                         // Oblique plane through (0,0,h) with unit normal (−sinφ, 0, cosφ),
                         // tanφ = k: section ellipse center (0,0,h), major axis (cosφ,0,sinφ),
                         // a = r/cosφ, b = r. P(t) = (r·cos t, r·sin t, h + k·r·cos t) — every
                         // sample is exactly on the cylinder.
        let cphi = 1.0 / (1.0 + k * k).sqrt();
        let sphi = k * cphi;
        let verts = vec![
            BRepVertex {
                point: Point3::new(r, 0.0, 0.0),
            },
            BRepVertex {
                point: Point3::new(r, 0.0, h + k * r),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Ellipse {
                    center: Point3::new(0.0, 0.0, h),
                    normal: Vector3::new(-sphi, 0.0, cphi),
                    major_axis: Vector3::new(cphi, 0.0, sphi),
                    major_radius: r / cphi,
                    minor_radius: r,
                },
            },
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0],
            inner_loops: vec![vec![1]],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces).expect("oblique ellipse tube");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Oracle 1: every vertex lies exactly on the cylinder (the ellipse
        // parameterization is on-surface by construction; the unroll must
        // not displace it).
        for (i, v) in t.verts.iter().enumerate() {
            let p = v.as_array();
            let rad = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!(
                (rad - r).abs() < 1e-9,
                "vertex {i} off the cylinder: radial {rad}"
            );
        }

        // Oracle 2: wall area → 2π·r·h from below (the k·cosθ term integrates
        // to zero over the full turn).
        let analytic = 2.0 * std::f64::consts::PI * r * h;
        let tri_area = |tri: &[u32; 3]| -> f64 {
            let p0 = t.verts[tri[0] as usize].as_array();
            let p1 = t.verts[tri[1] as usize].as_array();
            let p2 = t.verts[tri[2] as usize].as_array();
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        };
        let area: f64 = t.tris.iter().map(tri_area).sum();
        assert!(
            area > 0.97 * analytic && area <= analytic + 1e-9,
            "wall area {area} vs analytic {analytic} (inscribed)"
        );

        // Oracle 3: watertight ribbon — every boundary (count-1) edge lies
        // entirely on the bottom rim (z≈0) or on the ellipse plane
        // (z ≈ h + k·x); no edge covered more than twice.
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k3 in 0..3 {
                let (x, y) = (tri[k3], tri[(k3 + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let on_boundary = |g: u32| -> bool {
            let p = t.verts[g as usize].as_array();
            p[2].abs() < 1e-9 || (p[2] - (h + k * p[0])).abs() < 1e-9
        };
        for (&(x, y), &c) in &undirected {
            assert!(c <= 2, "edge ({x},{y}) covered {c} times (fold)");
            if c == 1 {
                assert!(
                    on_boundary(x) && on_boundary(y),
                    "boundary edge ({x},{y}) is not on a rim/ellipse — seam gap"
                );
            }
        }
    }

    /// KV14 Slice D (spec `yang_stage1_curved_holed_patch`): a cylinder lateral
    /// whose outer loop is NON-canonical — no full-circle rims and NOT the
    /// structured 2-arc partial-patch pattern — with NO holes. Real boolean
    /// outputs produce these when a prior op bites an irregular boundary into a
    /// partial patch (R0053 = [L,A,A,A,L,A,A,A]: each rim split into 3 arcs +
    /// 2 rulings). The pre-Slice-D dispatch walled these `MalformedTopology`
    /// ("found 0 full rims and 6 arcs"); Slice D routes them to the same
    /// unroll+CDT path (empty hole set), classifying the single winding-0 outer
    /// loop as a bounded partial patch.
    #[test]
    fn lateral_partial_patch_multi_arc_no_holes() {
        use std::f64::consts::PI;
        let r = 1.0_f64;
        let h = 2.0_f64;
        let on = |theta: f64, z: f64| Point3::new(r * theta.cos(), r * theta.sin(), z);
        // Sector theta in [0, PI] (a clean angular gap over (PI, 2PI) for the
        // branch cut), z in [0, h]. Each rim split into 3 arcs at PI/3, 2PI/3.
        // Outer loop: [A,A,A, L, A,A,A, L] = R0053's vocabulary (rotated).
        let b0 = on(0.0, 0.0); // V0
        let b1 = on(PI / 3.0, 0.0); // V1
        let b2 = on(2.0 * PI / 3.0, 0.0); // V2
        let b3 = on(PI, 0.0); // V3
        let t3 = on(PI, h); // V4
        let t2 = on(2.0 * PI / 3.0, h); // V5
        let t1 = on(PI / 3.0, h); // V6
        let t0 = on(0.0, h); // V7
        let verts = [b0, b1, b2, b3, t3, t2, t1, t0]
            .into_iter()
            .map(|point| BRepVertex { point })
            .collect::<Vec<_>>();
        // Bottom arcs sweep CCW about +z; top arcs sweep CCW about −z (returning
        // over [PI, 0]) so the loop nets zero axial winding (a bounded patch).
        let bot_arc = |start: u32, end: u32| BRepEdge {
            start,
            end,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
        };
        let top_arc = |start: u32, end: u32| BRepEdge {
            start,
            end,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, h),
                normal: Vector3::new(0.0, 0.0, -1.0),
                radius: r,
            },
        };
        let ruling = |start: u32, end: u32| BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        };
        let edges = vec![
            bot_arc(0, 1), // e0
            bot_arc(1, 2), // e1
            bot_arc(2, 3), // e2
            ruling(3, 4),  // e3 (V3->V4, up)
            top_arc(4, 5), // e4
            top_arc(5, 6), // e5
            top_arc(6, 7), // e6
            ruling(7, 0),  // e7 (V7->V0, down)
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 1, 2, 3, 4, 5, 6, 7],
            inner_loops: vec![],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces)
            .expect("Slice D multi-arc partial patch tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        // Oracle 1: total area equals the inscribed sector wall (r·PI)·h = PI·h.
        // A CDT that dropped the seam wedge or double-covered would miss/exceed
        // this; approached from BELOW since the arcs are chord-sampled.
        let tri_area = |tri: &[u32; 3]| -> f64 {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        };
        let area: f64 = t.tris.iter().map(tri_area).sum();
        let sector_wall = r * PI * h;
        assert!(
            area > 0.97 * sector_wall && area <= sector_wall + 1e-9,
            "patch area {area} must fill the PI sector wall (≈{sector_wall}, inscribed)"
        );

        // Oracle 2: watertight bounded patch — no interior holes, no fold. Every
        // count-1 boundary edge lies on the OUTER boundary: a rim (both ends at
        // z=0 or both at z=h) or a ruling (both ends at theta=0 or theta=PI).
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let theta_of = |p: [f64; 3]| p[1].atan2(p[0]);
        for (&(x, y), &c) in &undirected {
            assert!(
                c <= 2,
                "edge ({x},{y}) covered {c} times (fold/double cover)"
            );
            if c == 1 {
                let px = t.verts[x as usize].as_array();
                let py = t.verts[y as usize].as_array();
                let on_rim = (px[2].abs() < 1e-9 && py[2].abs() < 1e-9)
                    || ((px[2] - h).abs() < 1e-9 && (py[2] - h).abs() < 1e-9);
                let (tx, ty) = (theta_of(px), theta_of(py));
                let on_ruling = (tx.abs() < 1e-6 && ty.abs() < 1e-6)
                    || ((tx - PI).abs() < 1e-6 && (ty - PI).abs() < 1e-6);
                assert!(
                    on_rim || on_ruling,
                    "boundary edge ({x},{y}) is interior — hole or seam gap in a hole-free patch"
                );
            }
        }

        // Oracle 3: every triangle faces radially outward (reversed = false).
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(dot > 0.0, "triangle must face radially outward, dot={dot}");
        }
    }

    /// KV14 Slice E: a non-canonical CONE partial patch (multi-arc, no holes)
    /// re-enters the unroll+CDT path. A cone frustum sector [A,A,A,L,A,A,A,L]
    /// (R0020's vocabulary) with the u-scale varying by axial radius. Oracles:
    /// the patch fills the exact developable sector-frustum area (from below —
    /// chord-sampled), it is watertight and bounded (no interior hole), and it
    /// faces radially outward.
    #[test]
    fn cone_partial_patch_multi_arc_no_holes() {
        use std::f64::consts::PI;
        // Cone: apex at origin, axis +z, half-angle atan(0.5) (tan α = 0.5).
        let tan_a = 0.5_f64;
        let half_angle = tan_a.atan();
        let on = |theta: f64, z: f64| {
            let r = z * tan_a;
            Point3::new(r * theta.cos(), r * theta.sin(), z)
        };
        // Sector theta in [0, PI] (a clean gap over (PI, 2PI) for the branch
        // cut), between z=1 (r=0.5) and z=3 (r=1.5). Each rim split into 3 arcs.
        let z0 = 1.0_f64;
        let z1 = 3.0_f64;
        let b0 = on(0.0, z0);
        let b1 = on(PI / 3.0, z0);
        let b2 = on(2.0 * PI / 3.0, z0);
        let b3 = on(PI, z0);
        let t3 = on(PI, z1);
        let t2 = on(2.0 * PI / 3.0, z1);
        let t1 = on(PI / 3.0, z1);
        let t0 = on(0.0, z1);
        let verts = [b0, b1, b2, b3, t3, t2, t1, t0]
            .into_iter()
            .map(|point| BRepVertex { point })
            .collect::<Vec<_>>();
        // Bottom arcs sweep CCW about +z at radius r0; top arcs return over
        // [PI, 0] about −z at radius r1 (nets zero axial winding = bounded).
        let arc = |start: u32, end: u32, z: f64, up: bool| BRepEdge {
            start,
            end,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, z),
                normal: Vector3::new(0.0, 0.0, if up { 1.0 } else { -1.0 }),
                radius: z * tan_a,
            },
        };
        let ruling = |start: u32, end: u32| BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        };
        let edges = vec![
            arc(0, 1, z0, true),  // e0
            arc(1, 2, z0, true),  // e1
            arc(2, 3, z0, true),  // e2
            ruling(3, 4),         // e3 (up generator)
            arc(4, 5, z1, false), // e4
            arc(5, 6, z1, false), // e5
            arc(6, 7, z1, false), // e6
            ruling(7, 0),         // e7 (down generator)
        ];
        let faces = vec![BRepFace {
            surface: Surface::Cone {
                apex: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                half_angle,
            },
            outer_loop: vec![0, 1, 2, 3, 4, 5, 6, 7],
            inner_loops: vec![],
            reversed: false,
        }];
        let t = stage1_tessellate(&verts, &edges, &faces)
            .expect("Slice E cone multi-arc partial patch tessellation");
        assert!(!t.tris.is_empty(), "must produce triangles");

        let tri_area = |tri: &[u32; 3]| -> f64 {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        };
        let area: f64 = t.tris.iter().map(tri_area).sum();
        // Developable frustum-sector area over Δθ = PI:
        // (Δθ/2)·(r0+r1)·L, L = (z1−z0)/cosα.
        let r0 = z0 * tan_a;
        let r1 = z1 * tan_a;
        let cos_a = half_angle.cos();
        let slant = (z1 - z0) / cos_a;
        let sector_wall = (PI / 2.0) * (r0 + r1) * slant;
        assert!(
            area > 0.97 * sector_wall && area <= sector_wall + 1e-9,
            "cone patch area {area} must fill the frustum sector wall (≈{sector_wall}, inscribed)"
        );

        // Watertight bounded patch: every count-1 edge lies on the OUTER
        // boundary — a rim (both ends at z0 or both at z1) or a generator (both
        // ends at theta=0 or theta=PI).
        let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        for tri in &t.tris {
            for k in 0..3 {
                let (x, y) = (tri[k], tri[(k + 1) % 3]);
                *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        let theta_of = |p: [f64; 3]| p[1].atan2(p[0]);
        for (&(x, y), &c) in &undirected {
            assert!(
                c <= 2,
                "edge ({x},{y}) covered {c} times (fold/double cover)"
            );
            if c == 1 {
                let px = t.verts[x as usize].as_array();
                let py = t.verts[y as usize].as_array();
                let on_rim = ((px[2] - z0).abs() < 1e-9 && (py[2] - z0).abs() < 1e-9)
                    || ((px[2] - z1).abs() < 1e-9 && (py[2] - z1).abs() < 1e-9);
                let (tx, ty) = (theta_of(px), theta_of(py));
                let on_gen = (tx.abs() < 1e-6 && ty.abs() < 1e-6)
                    || ((tx - PI).abs() < 1e-6 && (ty - PI).abs() < 1e-6);
                assert!(
                    on_rim || on_gen,
                    "boundary edge ({x},{y}) is interior — hole or seam gap in a hole-free patch"
                );
            }
        }

        // Every triangle faces radially outward (reversed = false): positive
        // radial component (a cone normal is tilted but stays outward in r).
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(
                dot > 0.0,
                "cone triangle must face radially outward, dot={dot}"
            );
        }
    }

    /// KV14 Slice A edge case: a `reversed` holed lateral (a cavity/bore wall)
    /// excludes the hole AND faces radially INWARD, and a patch with TWO holes
    /// excludes both. Covers the `f.reversed` branch (P4) + multi-hole input.
    #[test]
    fn lateral_holed_patch_reversed_and_multi_hole() {
        use std::f64::consts::PI;
        let r = 1.0_f64;
        let on = |theta: f64, z: f64| Point3::new(r * theta.cos(), r * theta.sin(), z);
        let a = on(0.0, 0.0);
        let b = on(PI, 0.0);
        let c = on(PI, 2.0);
        let d = on(0.0, 2.0);
        // Two disjoint triangular holes in the sector.
        let h = |cz: f64| {
            [
                on(PI / 2.0 - 0.3, cz - 0.2),
                on(PI / 2.0 + 0.3, cz - 0.2),
                on(PI / 2.0, cz + 0.25),
            ]
        };
        let hole_a = h(0.6);
        let hole_b = h(1.4);
        let verts = [a, b, c, d]
            .into_iter()
            .chain(hole_a)
            .chain(hole_b)
            .map(|point| BRepVertex { point })
            .collect::<Vec<_>>();
        let mut edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 2.0),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        // Hole A verts = 4,5,6 ; hole B verts = 7,8,9.
        for (base, _) in [(4u32, ()), (7u32, ())] {
            edges.push(BRepEdge {
                start: base,
                end: base + 1,
                curve: Curve::LineSegment,
            });
            edges.push(BRepEdge {
                start: base + 1,
                end: base + 2,
                curve: Curve::LineSegment,
            });
            edges.push(BRepEdge {
                start: base + 2,
                end: base,
                curve: Curve::LineSegment,
            });
        }
        let faces = vec![BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: r,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: vec![vec![4, 5, 6], vec![7, 8, 9]],
            reversed: true,
        }];
        let t =
            stage1_tessellate(&verts, &edges, &faces).expect("reversed multi-hole tessellation");
        assert!(!t.tris.is_empty());

        let param = |p: [f64; 3]| -> (f64, f64) { (r * p[1].atan2(p[0]), p[2]) };
        let tri_of = |hole: &[Point3; 3]| {
            [
                param(hole[0].as_array()),
                param(hole[1].as_array()),
                param(hole[2].as_array()),
            ]
        };
        let inside = |uv: &[(f64, f64); 3], u: f64, v: f64| -> bool {
            let (x0, y0) = uv[0];
            let (x1, y1) = uv[1];
            let (x2, y2) = uv[2];
            let d1 = (u - x1) * (y0 - y1) - (x0 - x1) * (v - y1);
            let d2 = (u - x2) * (y1 - y2) - (x1 - x2) * (v - y2);
            let d3 = (u - x0) * (y2 - y0) - (x2 - x0) * (v - y0);
            !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
        };
        let uva = tri_of(&hole_a);
        let uvb = tri_of(&hole_b);
        for tri in &t.tris {
            let a = t.verts[tri[0] as usize].as_array();
            let b = t.verts[tri[1] as usize].as_array();
            let c = t.verts[tri[2] as usize].as_array();
            let cen = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let (u, v) = param(cen);
            assert!(
                !inside(&uva, u, v) && !inside(&uvb, u, v),
                "a hole was paved over"
            );
            // reversed ⇒ inward-facing: geometric normal · radial < 0.
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let dot = n[0] * cen[0] + n[1] * cen[1];
            assert!(
                dot < 0.0,
                "reversed cavity wall must face inward, dot={dot}"
            );
        }
    }

    /// M-C RED (spec `m8_stage0_band_scale_crossing_verts` §4 E-C1): two
    /// DISTINCT override points whose angular separation is far below the
    /// legacy merge_tol (band-close genuine crossings — the R0088/R0070
    /// twin population) must BOTH be inserted into the rim ring. Silently
    /// keeping only one desynchronizes the ring from the cap override that
    /// carries both points (T-junction holes, the measured M-C class). A
    /// bit-identical duplicate must still be deduplicated (E-C1b).
    #[test]
    fn rim_override_band_close_distinct_points_both_inserted() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        let r = 0.5_f64;
        let mk = |az: f64, z: f64| {
            let (s, c) = az.sin_cos();
            Point3::new(r * c, r * s, z)
        };
        // Two on-circle points ~2e-13 rad apart (distinct f64 coordinates,
        // far below uni_step·1e-6), on both rims for lateral balance.
        let (az1, az2) = (0.3_f64, 0.3_f64 + 2.0e-13);
        let (b1, b2) = (mk(az1, 0.0), mk(az2, 0.0));
        let (t1, t2) = (mk(az1, 1.0), mk(az2, 1.0));
        assert_ne!(b1.as_array(), b2.as_array(), "twin construction degenerate");
        let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        ov.insert(0, vec![b1, b2]);
        ov.insert(1, vec![t1, t2]);
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None)
            .expect("band-close distinct overrides must be accepted");
        for (name, p) in [("b1", b1), ("b2", b2), ("t1", t1), ("t2", t2)] {
            assert!(
                t.verts.iter().any(|q| q.as_array() == p.as_array()),
                "M-C RED — distinct band-close override {name} missing from the \
                 rim ring (silent merge_tol drop, spec §2)"
            );
        }
        // Ring stays a closed 2-manifold with the band-thin segments present.
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for tri in &t.tris {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        assert!(
            counts.values().all(|&c| c == 2),
            "band-close override insertion must keep the cylinder closed"
        );

        // E-C1b: a bit-identical duplicate is still dropped (no double vertex).
        // Balanced across both rims (the lateral azimuth-merge expectation).
        let mut dup: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        dup.insert(0, vec![b1, b1]);
        dup.insert(1, vec![t1, t1]);
        let td = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &dup, None)
            .expect("bit-identical duplicate override must be accepted");
        assert_eq!(
            td.verts
                .iter()
                .filter(|q| q.as_array() == t1.as_array())
                .count(),
            1,
            "bit-identical duplicate override must be deduplicated exactly once"
        );
    }

    /// Chained swiss-cheese wall 1 RED (task #62, spec
    /// `m8_holed_disc_coplanar_overlay` §8 increment 5): the azimuth-merge
    /// lateral pairing must be WRAP-AWARE. A RECOVERED B-Rep (boolean output
    /// re-entering a boolean) can carry one rim's seam vertex at azimuth
    /// exactly 0 while the other rim's sits a femto BELOW the +x axis
    /// (y = −ε): `atan2(…).rem_euclid(2π)` maps the latter to 2π−ε, sorting
    /// it LAST instead of FIRST, and the positional `bot[k] ↔ top[k]` pairing
    /// shifts by one slot — the F0086 step-2 wall
    /// (`azimuth-merge rims disagree at index 0 (bottom 0 vs top 0.4488)`).
    /// The two sorted rings are CIRCULAR sequences: pairing must align them
    /// by cyclic shift, not by absolute sort position.
    ///
    /// Fixture: rt-style cylinder whose TOP seam vertex is rotated a femto
    /// below the +x axis (y = −r·5e−16, on-circle within band), with one
    /// same-azimuth override pair on both rims to force the azimuth-merge
    /// path. Oracle: tessellation SUCCEEDS and stays a closed 2-manifold.
    /// RED today: MalformedTopology "rims disagree at index 0".
    #[test]
    fn rim_override_wrap_seam_cyclic_alignment() {
        let r = 0.5_f64;
        let eps_y = -r * 5.0e-16; // top seam vertex a femto BELOW the +x axis
        let v0 = Point3::new(r, 0.0, 0.0);
        let v1 = Point3::new(r, eps_y, 1.0);
        let verts = vec![BRepVertex { point: v0 }, BRepVertex { point: v1 }];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 1.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
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
                surface: Surface::Cylinder {
                    axis_point: Point3::new(0.0, 0.0, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -1.0,
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        // One override pair at the same geometric azimuth on both rims (not
        // near a uniform sample) — forces the azimuth-merge lateral path.
        let az = 0.3_f64;
        let (s, c) = az.sin_cos();
        let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        ov.insert(0, vec![Point3::new(r * c, r * s, 0.0)]);
        ov.insert(1, vec![Point3::new(r * c, r * s, 1.0)]);
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None).expect(
            "wrap-seam cylinder must tessellate — the azimuth-merge pairing \
             must align the rings cyclically, not by absolute sort position",
        );
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for tri in &t.tris {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        assert!(
            !counts.is_empty() && counts.values().all(|&c| c == 2),
            "wrap-seam cylinder must stay a closed 2-manifold"
        );
    }

    /// M8 holed-disc increment 3 RED (spec `m8_holed_disc_coplanar_overlay`
    /// §8): ULP-TWIN override points — two distinct points 1 ULP apart in x
    /// whose f64 seam-relative rim angles COLLIDE — must be ring-ordered by
    /// their EXACT angular order on BOTH rims, regardless of the caller's
    /// insertion order, and the lateral strip must pair each bottom twin with
    /// its same-azimuth top partner (no twisted quad). Today the slot sort
    /// falls back to insertion order on the f64 tie, and the two rims' frames
    /// have OPPOSITE orientations, so one rim always comes out mis-ordered →
    /// the cap fan walks U_lo–twinB–twinA–U_hi on one cap (wrong adjacency)
    /// and the wall strip twists (a self-intersecting Stage-0 mesh — the
    /// `annular_cap_under_disc` cherchi `SegmentNotLocatable` wall).
    ///
    /// Oracles (frame-independent, structural):
    /// - on each cap, the uniform sample at the LOWER global azimuth is
    ///   ring-adjacent to the LOWER-azimuth twin (and not to the other);
    /// - the lateral contains BOTH vertical edges (A_bot,A_top), (B_bot,B_top);
    /// - the full mesh stays a closed 2-manifold;
    /// - both insertion orders ([A,B] and [B,A]) yield the same triangle SET.
    #[test]
    fn rim_override_ulp_twins_exact_order_both_rims() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);

        // Pick the bottom-rim chord whose midpoint has the smallest |x| (near
        // the ±y axis, far from the seam at +x): there the azimuth derivative
        // dθ/dx = |y|/r² is maximal while ULP(θ-offset) is fixed, so a 1-ULP
        // x perturbation moves the angle by far LESS than one ULP of the
        // seam-relative offset → the f64 angles of the twins collide.
        let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
        let mut rim0: Vec<(f64, Point3)> = plain
            .sources
            .iter()
            .enumerate()
            .filter_map(|(i, src)| match src {
                TessellationSource::BRepEdge { edge: 0, t } => Some((*t, plain.verts[i])),
                _ => None,
            })
            .collect();
        rim0.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(rim0.len() >= 4, "bottom rim must have >=4 Steiner samples");
        let mut best: Option<([f64; 2], [f64; 2])> = None;
        for w in rim0.windows(2) {
            let (p0, p1) = (w[0].1.as_array(), w[1].1.as_array());
            let mid_x = 0.5 * (p0[0] + p1[0]);
            if best.is_none_or(|(a, b)| mid_x.abs() < 0.5 * (a[0] + b[0]).abs()) {
                best = Some(([p0[0], p0[1]], [p1[0], p1[1]]));
            }
        }
        let (e0, e1) = best.unwrap();
        let mx = 0.5 * (e0[0] + e1[0]);
        let my = 0.5 * (e0[1] + e1[1]);
        // The ULP twins: same y, x one ULP apart (the real Stage-0 twin shape:
        // two sweep-event columns from 1-ULP-different rim-sample x's).
        let xa = mx;
        let xb = f64::from_bits(mx.to_bits() + 1);
        assert_ne!(xa, xb, "twin construction degenerate");
        // Exact global-azimuth order: cross(A,B) = xa·my − my·xb = my·(xa−xb),
        // exact in f64 (adjacent-float subtraction is exact). Positive cross
        // means B is CCW of A, i.e. A has the LOWER azimuth.
        let a_first = my * (xa - xb) > 0.0;
        let (x_lo, x_hi) = if a_first { (xa, xb) } else { (xb, xa) };
        let tw_lo_b = Point3::new(x_lo, my, 0.0); // lower-azimuth twin, bottom
        let tw_hi_b = Point3::new(x_hi, my, 0.0);
        let tw_lo_t = Point3::new(x_lo, my, 1.0); // same azimuths on top rim
        let tw_hi_t = Point3::new(x_hi, my, 1.0);
        // Twin global azimuth (for locating each cap's bracketing uniform
        // samples — the top rim's samples are NOT bit-identical in (x,y) to
        // the bottom's, its frame flips, so each cap is searched on its own).
        let az_of = |x: f64, y: f64| y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI);
        let az_tw = az_of(mx, my);

        let run = |first: Point3, second: Point3, tfirst: Point3, tsecond: Point3| {
            let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
                std::collections::BTreeMap::new();
            ov.insert(0, vec![first, second]);
            ov.insert(1, vec![tfirst, tsecond]);
            stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None)
                .expect("ULP-twin overrides must be accepted")
        };

        let check = |t: &Stage1Tess, tag: &str| {
            let vid = |p: Point3| -> u32 {
                t.verts
                    .iter()
                    .position(|q| q.as_array() == p.as_array())
                    .unwrap_or_else(|| panic!("{tag}: point {p:?} missing from mesh"))
                    as u32
            };
            // The rim-E uniform samples bracketing the twin azimuth (the
            // twins' ring neighbours on that rim).
            let brackets = |edge: u32| -> (u32, u32) {
                let mut lo: Option<(f64, u32)> = None;
                let mut hi: Option<(f64, u32)> = None;
                for (i, src) in t.sources.iter().enumerate() {
                    if !matches!(src, TessellationSource::BRepEdge { edge: e, .. } if *e == edge) {
                        continue;
                    }
                    let a = t.verts[i].as_array();
                    // Skip the inserted twins themselves (also BRepEdge-tagged).
                    if a[1] == my && (a[0] == xa || a[0] == xb) {
                        continue;
                    }
                    let az = az_of(a[0], a[1]);
                    if az < az_tw {
                        if lo.is_none_or(|(b, _)| az > b) {
                            lo = Some((az, i as u32));
                        }
                    } else if hi.is_none_or(|(b, _)| az < b) {
                        hi = Some((az, i as u32));
                    }
                }
                (
                    lo.unwrap_or_else(|| panic!("{tag}: no uniform below twin on rim {edge}"))
                        .1,
                    hi.unwrap_or_else(|| panic!("{tag}: no uniform above twin on rim {edge}"))
                        .1,
                )
            };
            // Undirected edge sets: bottom cap (all z==0), top cap (all z==1),
            // lateral (z-spanning).
            let mut cap_b = std::collections::BTreeSet::new();
            let mut cap_t = std::collections::BTreeSet::new();
            let mut lat = std::collections::BTreeSet::new();
            let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
                std::collections::BTreeMap::new();
            for tri in &t.tris {
                let zs: Vec<f64> = tri
                    .iter()
                    .map(|&v| t.verts[v as usize].as_array()[2])
                    .collect();
                let bucket: &mut std::collections::BTreeSet<(u32, u32)> =
                    if zs.iter().all(|&z| z == 0.0) {
                        &mut cap_b
                    } else if zs.iter().all(|&z| z == 1.0) {
                        &mut cap_t
                    } else {
                        &mut lat
                    };
                for k in 0..3 {
                    let (a, b) = (tri[k], tri[(k + 1) % 3]);
                    let e = (a.min(b), a.max(b));
                    bucket.insert(e);
                    *counts.entry(e).or_insert(0) += 1;
                }
            }
            let e = |a: u32, b: u32| (a.min(b), a.max(b));
            for (cap, lo, hi, edge, z) in [
                (&cap_b, tw_lo_b, tw_hi_b, 0u32, 0.0),
                (&cap_t, tw_lo_t, tw_hi_t, 1u32, 1.0),
            ] {
                let (vlo, vhi) = (vid(lo), vid(hi));
                let (ulo, uhi) = brackets(edge);
                assert!(
                    cap.contains(&e(ulo, vlo)),
                    "{tag}: cap z={z} — lower uniform must be ring-adjacent to \
                     the LOWER-azimuth twin (exact order), edge missing"
                );
                assert!(
                    !cap.contains(&e(ulo, vhi)),
                    "{tag}: cap z={z} — lower uniform adjacent to the HIGHER \
                     twin: ring is in WRONG (insertion/tie) order"
                );
                assert!(
                    cap.contains(&e(uhi, vhi)),
                    "{tag}: cap z={z} — upper uniform must be ring-adjacent to \
                     the HIGHER-azimuth twin, edge missing"
                );
                assert!(
                    !cap.contains(&e(uhi, vlo)),
                    "{tag}: cap z={z} — upper uniform adjacent to the LOWER \
                     twin: ring is in WRONG (insertion/tie) order"
                );
            }
            // Untwisted wall: both same-azimuth vertical edges exist.
            let (blo, bhi) = (vid(tw_lo_b), vid(tw_hi_b));
            let (tlo, thi) = (vid(tw_lo_t), vid(tw_hi_t));
            assert!(
                lat.contains(&e(blo, tlo)),
                "{tag}: lateral misses vertical edge at the lower twin column \
                 (twisted quad — bottom twin paired with the WRONG top twin)"
            );
            assert!(
                lat.contains(&e(bhi, thi)),
                "{tag}: lateral misses vertical edge at the higher twin column \
                 (twisted quad — bottom twin paired with the WRONG top twin)"
            );
            assert!(
                counts.values().all(|&c| c == 2),
                "{tag}: mesh must stay a closed 2-manifold"
            );
            let mut tris: Vec<[[u64; 3]; 3]> = t
                .tris
                .iter()
                .map(|tri| {
                    let mut ps: [[u64; 3]; 3] = [[0; 3]; 3];
                    for (k, &v) in tri.iter().enumerate() {
                        let a = t.verts[v as usize].as_array();
                        ps[k] = [a[0].to_bits(), a[1].to_bits(), a[2].to_bits()];
                    }
                    ps.sort();
                    ps
                })
                .collect();
            tris.sort();
            tris
        };

        // Insertion order 1: exact order (lo, hi). Insertion order 2: reversed.
        // BOTH must produce the exact ring order (the sort may not fall back
        // to insertion order on the f64 angle tie) and the same geometry.
        let t1 = run(tw_lo_b, tw_hi_b, tw_lo_t, tw_hi_t);
        let g1 = check(&t1, "insertion (lo,hi)");
        let t2 = run(tw_hi_b, tw_lo_b, tw_hi_t, tw_lo_t);
        let g2 = check(&t2, "insertion (hi,lo)");
        assert_eq!(
            g1, g2,
            "ring order must be insertion-order independent (exact, not stable-tie)"
        );
    }

    /// A rim-crossing override lies on the tessellated rim POLYGON (a CHORD
    /// between two on-circle samples), so it sits radially INSIDE the analytic
    /// circle by up to the Stage-1 chord sagitta. The override validation must
    /// ACCEPT such a point (it is the same point the cap overlay uses — snapping
    /// it to the circle would mint a T-junction), while still rejecting a point
    /// that is OUTSIDE the circle or inside by MORE than the sagitta (a genuine
    /// off-rim fault). Regression for task #21 (the `is not on the circle`
    /// rejection that masked the same-normal crossing path).
    #[test]
    fn rim_override_accepts_chord_point_rejects_off_rim() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        let r = 0.5_f64;
        let az = 0.3_f64; // not a uniform sample
        let (s, c) = az.sin_cos();
        // Derive a point GUARANTEED on a chord of the actual tessellated top
        // rim (circle edge 1): the midpoint of two consecutive rim samples — its
        // radial deficit equals the exact Stage-1 chord sagitta for this N.
        let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
        let mut rim1: Vec<(f64, Point3)> = plain
            .sources
            .iter()
            .enumerate()
            .filter_map(|(i, src)| match src {
                TessellationSource::BRepEdge { edge: 1, t } => Some((*t, plain.verts[i])),
                _ => None,
            })
            .collect();
        rim1.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(rim1.len() >= 2, "top rim must have >=2 samples");
        let (p0, p1) = (rim1[0].1.as_array(), rim1[1].1.as_array());
        let mx = 0.5 * (p0[0] + p1[0]);
        let my = 0.5 * (p0[1] + p1[1]);
        let top_chord = Point3::new(mx, my, 1.0);
        // Same (x,y) on the BOTTOM rim plane (z=0): same global azimuth + same
        // radial deficit (the cylinder is axis-aligned), so inserting on BOTH
        // rims keeps the lateral azimuth-merge balanced.
        let bot_chord = Point3::new(mx, my, 0.0);
        let single = |e: u32, p: Point3| {
            let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
                std::collections::BTreeMap::new();
            ov.insert(e, vec![p]);
            ov
        };

        // (1) chord point (radial deficit = chord sagitta) → ACCEPTED + present.
        let mut both: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        both.insert(0, vec![bot_chord]);
        both.insert(1, vec![top_chord]);
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &both, None)
            .expect("a rim point on the tessellated chord must be accepted");
        assert!(
            t.verts.iter().any(|q| q.as_array() == top_chord.as_array()),
            "accepted chord point must appear in the mesh"
        );

        // (2) far INSIDE the circle (deficit 0.1 ≫ sagitta) → loud reject
        // (the off-rim validation fires before the lateral merge).
        let too_deep = Point3::new((r - 0.1) * c, (r - 0.1) * s, 1.0);
        assert!(
            matches!(
                stage1_tessellate_with_rim_overrides(
                    &verts,
                    &edges,
                    &faces,
                    &single(1, too_deep),
                    None
                ),
                Err(YangError::MalformedTopology(_))
            ),
            "a point far inside the rim circle must be rejected (off-rim fault)"
        );

        // (3) OUTSIDE the circle → loud reject.
        let outside = Point3::new((r + 0.01) * c, (r + 0.01) * s, 1.0);
        assert!(
            matches!(
                stage1_tessellate_with_rim_overrides(
                    &verts,
                    &edges,
                    &faces,
                    &single(1, outside),
                    None
                ),
                Err(YangError::MalformedTopology(_))
            ),
            "a point outside the rim circle must be rejected"
        );
    }

    // ── M8-intra: exactly-negated intra-solid coplanar exclusion ────────────
    // Spec `specs/m8_intra_opposite_plane_canonicalization.md` (FIP Phase 2,
    // RED). `scan_near_coplanar` is `pub(crate)`, so these unit tests reach it
    // directly.

    /// A minimal planar `BRepFace` with a valid CCW square loop in one plane,
    /// so `BRep::new`'s Stage-1 tessellation accepts it while `scan` reads the
    /// DECLARED `(normal, d)`.
    fn m8_intra_square_a() -> BRep {
        // Two coplanar squares (z = 3) with EXACTLY-negated plane values — a
        // stepped solid's shared plane carrying opposite outward normals. The
        // negation is value-exact AND exercises 0.0 == -0.0 in the normal's x/y
        // components (spec B6 / §6): F0 = ((0.0, 0.0, 1.0), -3.0),
        // F1 = ((-0.0, -0.0, -1.0), 3.0).
        let verts = vec![
            // F0 corners (CCW viewed from +z).
            BRepVertex {
                point: Point3::new(0.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 2.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 2.0, 3.0),
            },
            // F1 corners (same coords; wound CCW viewed from −z).
            BRepVertex {
                point: Point3::new(0.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 2.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 2.0, 3.0),
            },
        ];
        let seg = |s: u32, e: u32| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        };
        let edges = vec![
            seg(0, 1),
            seg(1, 2),
            seg(2, 3),
            seg(3, 0), // F0 (+z winding)
            seg(4, 7),
            seg(7, 6),
            seg(6, 5),
            seg(5, 4), // F1 (−z winding)
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -3.0,
                },
                outer_loop: vec![0, 1, 2, 3],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(-0.0, -0.0, -1.0),
                    d: 3.0,
                },
                outer_loop: vec![4, 5, 6, 7],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        BRep::new(verts, edges, faces).expect("intra-A BRep::new")
    }

    /// Solid B: a single tilted triangle whose AABB overlaps solid A's face
    /// region (x,y ∈ [0.5,1.5], z ∈ [2.5,3.5]) but shares NO plane with A — the
    /// "other operand reaches the shared-plane region" contact condition the
    /// intra gate keys on.
    fn m8_intra_overlapping_b() -> BRep {
        let verts = vec![
            BRepVertex {
                point: Point3::new(0.5, 0.5, 2.5),
            },
            BRepVertex {
                point: Point3::new(1.5, 0.5, 2.5),
            },
            BRepVertex {
                point: Point3::new(1.0, 1.5, 3.5),
            },
        ];
        let seg = |s: u32, e: u32| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        };
        let edges = vec![seg(0, 1), seg(1, 2), seg(2, 0)];
        // Tilted plane normal = (v1−v0)×(v2−v0), un-normalized is fine (scan
        // normalizes); it is not parallel to z, so no coplanar cross pair.
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, -1.0, 1.0),
                d: -2.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        BRep::new(verts, edges, faces).expect("intra-B BRep::new")
    }

    /// Spec B6 (RED): an intra-solid pair on EXACTLY-negated planes (two
    /// orientations of ONE plane) is benign and must NOT flag the intra gate,
    /// even though the other solid overlaps the region.
    ///
    /// RED today: the two faces' raw bits differ (n vs −n, d vs −d, and
    /// 0.0 vs −0.0), so the bit-identity exclusion does not fire and the
    /// near-coplanar band flags them → `scan.intra == Some(..)`.
    #[test]
    fn intra_exactly_negated_pair_is_excluded() {
        let a = m8_intra_square_a();
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "exactly-negated intra pair must be benign (B6), got {:?}",
            scan.intra
        );
    }

    /// Spec B7 (guard): a near-but-NOT-exactly-negated intra pair (one normal
    /// component drifted 1 ULP from exact negation) is the loud residue and
    /// MUST still flag. Passes today; pins that the B6 exclusion is exact-only.
    #[test]
    fn intra_one_ulp_off_negation_still_walls_guard() {
        let mut a = m8_intra_square_a();
        // Drift F1's z-normal component 1 ULP off exact negation.
        {
            let faces = a.faces();
            let Surface::Plane { normal, d } = faces[1].surface else {
                panic!("F1 not planar");
            };
            let n = normal.as_array();
            let drifted = f64::from_bits(n[2].to_bits().wrapping_add(1));
            // Rebuild A with the drifted F1 normal (BRep faces are not mutable
            // in place through the accessor).
            let verts = a.vertices().to_vec();
            let edges = a.edges().to_vec();
            let mut new_faces = a.faces().to_vec();
            new_faces[1].surface = Surface::Plane {
                normal: Vector3::new(n[0], n[1], drifted),
                d,
            };
            a = BRep::new(verts, edges, new_faces).expect("drifted intra-A BRep::new");
        }
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_some(),
            "a 1-ULP-off (not exactly negated) intra pair must still wall loud (B7)"
        );
    }

    // ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
    // Attacks on the exactly-negated intra exclusion in `scan_near_coplanar`.
    // Appended here (not in a new `tests/` file) because `scan_near_coplanar`
    // is `pub(crate)`. Purely additive; touches no existing test. Reuses the
    // `m8_intra_square_a` / `m8_intra_overlapping_b` helpers above.

    /// Rebuild solid A with a chosen F1 (upper-plane) normal/offset so an attack
    /// can inject exact bit patterns the accessor cannot mutate in place.
    fn m8_intra_a_with_f1(normal: Vector3, d: f64) -> BRep {
        let a = m8_intra_square_a();
        let verts = a.vertices().to_vec();
        let edges = a.edges().to_vec();
        let mut faces = a.faces().to_vec();
        faces[1].surface = Surface::Plane { normal, d };
        BRep::new(verts, edges, faces).expect("rebuilt intra-A")
    }

    /// FINDING (test strength). Spec §6 / B6 claim the exclusion uses f64 VALUE
    /// equality "so `0.0 == -0.0` matches — bit compare would not". The existing
    /// `intra_exactly_negated_pair_is_excluded` fixture puts −0.0 on F1's x/y,
    /// but for a −0.0 vs 0.0 pairing a *sign-flip-bit* compare
    /// (`a.to_bits() == b.to_bits() ^ SIGN`) gives the SAME answer as the value
    /// compare — so that test does NOT actually distinguish value from bit and
    /// SURVIVES the sign-flip-bit mutation. This fixture uses +0.0 on BOTH
    /// faces' x/y (0.0 vs 0.0), where value-negation still holds (0.0 == −0.0)
    /// but sign-flip-bit does NOT — a producer that emits +0.0 on both
    /// orientations (a hand-built / file-loaded solid that never ran
    /// `canonicalize_sibling_planes`) is a real input. This is the case that
    /// genuinely KILLS a bit-compare mutation.
    #[test]
    fn adversary_both_positive_zero_negation_excluded() {
        // F0 = ((0,0,1), −3); F1 = ((+0,+0,−1), +3): value-exact negation with
        // +0.0 (NOT −0.0) in x/y on BOTH faces.
        let a = m8_intra_a_with_f1(Vector3::new(0.0, 0.0, -1.0), 3.0);
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "value-exact negation with +0.0/+0.0 must be benign (B6), got {:?}",
            scan.intra
        );
    }

    /// Attack 5 (non-unit normals). Two faces on ONE geometric plane whose raw
    /// stored normals differ in magnitude (n vs −2n) are NOT exact value
    /// negations, so the B6 exclusion must NOT fire; the pair then normalizes to
    /// parallel-opposite-coplanar and — since B reaches the region — walls LOUD.
    /// The documented conservative residue; nothing crashes.
    #[test]
    fn adversary_nonunit_opposite_normals_still_wall() {
        // F1 = ((0,0,−2), 6): plane −2z + 6 = 0 ⇒ z = 3, opposite orientation of
        // F0's z = 3 plane, but stored non-unit.
        let a = m8_intra_a_with_f1(Vector3::new(0.0, 0.0, -2.0), 6.0);
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_some(),
            "non-unit opposite normals must not be excluded (conservative residue)"
        );
    }

    /// Attack 4 (plane through the origin). Both faces carry d = 0.0 and a zero
    /// x/y normal component; F1's normal is the value-negation of F0's. The
    /// value compare (0.0 == −0.0, and 0.0 == −0.0 on d) excludes it.
    #[test]
    fn adversary_plane_through_origin_negation_excluded() {
        // Move both squares to z = 0 so d = 0 on both faces, then negate F1.
        let mut a = m8_intra_square_a();
        {
            let mut verts = a.vertices().to_vec();
            for v in verts.iter_mut() {
                v.point = Point3::new(v.point.x(), v.point.y(), 0.0);
            }
            let edges = a.edges().to_vec();
            let mut faces = a.faces().to_vec();
            faces[0].surface = Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            };
            faces[1].surface = Surface::Plane {
                normal: Vector3::new(-0.0, -0.0, -1.0),
                d: -0.0,
            };
            a = BRep::new(verts, edges, faces).expect("origin-plane intra-A");
        }
        // B straddles z = 0 so its AABB overlaps the shared plane region.
        let b = {
            let verts = vec![
                BRepVertex {
                    point: Point3::new(0.5, 0.5, -0.5),
                },
                BRepVertex {
                    point: Point3::new(1.5, 0.5, -0.5),
                },
                BRepVertex {
                    point: Point3::new(1.0, 1.5, 0.5),
                },
            ];
            let seg = |s: u32, e: u32| BRepEdge {
                start: s,
                end: e,
                curve: Curve::LineSegment,
            };
            let edges = vec![seg(0, 1), seg(1, 2), seg(2, 0)];
            let faces = vec![BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, -1.0, 1.0),
                    d: 0.0,
                },
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            }];
            BRep::new(verts, edges, faces).expect("origin-plane B")
        };
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "through-origin value-negation (d = 0.0/−0.0) must be benign (B6), got {:?}",
            scan.intra
        );
    }

    /// Attack (asymmetry). The B6 exclusion is orientation-blind to which face
    /// is listed first: swapping F0/F1 (rep negated first) is still excluded.
    #[test]
    fn adversary_negation_exclusion_is_symmetric() {
        // A with F0 negated instead of F1: F0 = ((−0,−0,−1), 3), F1 = ((0,0,1), −3).
        let a = {
            let base = m8_intra_square_a();
            let verts = base.vertices().to_vec();
            let edges = base.edges().to_vec();
            let mut faces = base.faces().to_vec();
            faces[0].surface = Surface::Plane {
                normal: Vector3::new(-0.0, -0.0, -1.0),
                d: 3.0,
            };
            faces[1].surface = Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -3.0,
            };
            BRep::new(verts, edges, faces).expect("swapped intra-A")
        };
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "negation exclusion must be symmetric in face order, got {:?}",
            scan.intra
        );
    }
}
