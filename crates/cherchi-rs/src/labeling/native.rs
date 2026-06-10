//! Native `LabeledArrangement` producer + native `MeshBoolean` (PR-CR-BL3a) —
//! Cherchi 2022 §5, step 3 (boolean keep + explicit output).
//!
//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! Source: `code/booleans.cpp::booleanPipeline` /
//! `customBooleanPipeline` / `boolUnion` / `boolIntersection` /
//! `boolSubtraction` / `boolXOR` / `computeFinalExplicitResult`.
//!
//! This module ties the whole native pipeline together: concat the two input
//! meshes → `mesh_arrangement` (AR3b global conforming soup) →
//! `compute_all_patches` (BL1) → `compute_inside_out` (BL2) → assemble the
//! producer-agnostic [`LabeledArrangement`] contract → apply the
//! `keep_set(op)` keep-rules (already ported in `labeled_arrangement.rs`) →
//! emit the explicit output mesh.
//!
//! ## Coordinate emission (`computeFinalExplicitResult`)
//!
//! The C++ resolves every kept vertex via
//! `genericPoint::getApproxXYZCoordinates` (the lazy-exact → double
//! emission) and then DESCALES by the input multiplier, read back from the
//! last jolly point (`tm.vert(tm.numVerts()-1)->toExplicit3D().X()`,
//! booleans.cpp:1378-1379: `for(double &c : out_coords) c /= multiplier;`).
//! The port resolves each referenced vertex to its EXACT rational
//! coordinates (`aux_structure::exact_point_coords`, pure dashu), rounds to
//! the nearest f64, and divides by [`ArrangementSoup::multiplier`] — the
//! multiplier is a power of two (`compute_multiplier`), so the division is
//! exact and the result matches the C++'s lazy-exact → double emission as
//! closely as possible. Vertices are compacted in first-reference order
//! (the C++ `vertex_index` walk): the 5 jolly tail points and any
//! unreferenced vertices never appear in the output.
//!
//! ## Per-op orientation fix
//!
//! `boolSubtraction` flips kept triangles NOT on A's surface (the cavity
//! wall, booleans.cpp:1480-1485); `boolXOR` flips kept triangles with a
//! non-empty inside set (booleans.cpp:1506-1510). Union / intersection emit
//! original winding. The flips happen at *emission* (this module), not in
//! `keep_set` — the keep-rules stay pure label logic.
//!
//! ## Duplicated (coplanar) triangles — nothing to restore
//!
//! The C++ `customBooleanPipeline` re-adds the duplicated coplanar input
//! triangles (`addDuplicateTrisInfoInStructures` / `dupl_triangles`) before
//! the final result so coplanar-overlap regions survive the boolean. This
//! port's arrangement loudly DEFERS real coplanar-overlap pairs upstream
//! (`ArrangementError::CoplanarPairDeferred`, deviation N17 — Stage-0 / M8
//! handles coplanarity before the mesh boolean per Yang 2025 §4.5.5), so by
//! construction no duplicated triangles exist here and there is nothing to
//! restore.

use cad_primitives::{BoolOp, Point3};

use crate::arrangements::aux_structure::exact_point_coords;
use crate::arrangements::soup::{mesh_arrangement, ArrangementError, ArrangementSoup};
use crate::labeled_arrangement::{InputId, LabeledArrangement};
use crate::labeling::inside_out::{compute_inside_out, InsideOutError};
use crate::labeling::patches::{compute_all_patches, PatchError};
use crate::mesh::Mesh;

/// Loud failure surface — never silent (P9/P10). Wraps each upstream
/// stage's typed error so callers can tell WHERE the pipeline stopped.
#[derive(Debug, PartialEq)]
pub enum NativeBooleanError {
    /// The AR3b global arrangement failed (coplanar deferral, FFI shim
    /// missing, retriangulation walls, …).
    Arrangement(ArrangementError),
    /// BL1 patch flood-fill failed (label mismatch / count mismatch).
    Patches(PatchError),
    /// BL2 ray-cast in/out classification failed.
    InsideOut(InsideOutError),
    /// A referenced soup vertex could not be resolved to exact rational
    /// coordinates (degenerate implicit-point generators) — impossible for
    /// a valid arrangement, surfaced loudly instead of emitting garbage.
    UnresolvableVertex { vert: u32 },
}

impl std::fmt::Display for NativeBooleanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arrangement(e) => write!(f, "native boolean: arrangement failed: {e:?}"),
            Self::Patches(e) => write!(f, "native boolean: patch flood-fill failed: {e:?}"),
            Self::InsideOut(e) => write!(f, "native boolean: in/out classification failed: {e:?}"),
            Self::UnresolvableVertex { vert } => {
                write!(
                    f,
                    "native boolean: vertex {vert} has unresolvable exact coordinates"
                )
            }
        }
    }
}

impl std::error::Error for NativeBooleanError {}

/// Run the full native Stage-2 pipeline on two input meshes and assemble
/// the producer-agnostic [`LabeledArrangement`]: the exact arrangement mesh
/// (explicit, DESCALED f64 coordinates) plus per-triangle `surface` /
/// `inside` / `patch` labels (invariants I1/I2).
///
/// `a` is solid 0, `b` is solid 1 (binary boolean; `num_inputs == 2`).
pub fn native_labeled_arrangement(
    a: &Mesh,
    b: &Mesh,
) -> Result<LabeledArrangement, NativeBooleanError> {
    // Concat a+b into (coords, tris, labels): a's tris carry [InputId(0)],
    // b's [InputId(1)] (the C++ booleanPipeline label setup).
    let mut coords = Vec::with_capacity(3 * (a.verts.len() + b.verts.len()));
    for p in a.verts.iter().chain(b.verts.iter()) {
        coords.push(p.x());
        coords.push(p.y());
        coords.push(p.z());
    }
    let off = a.verts.len() as u32;
    let mut tris = a.tris.clone();
    tris.extend(b.tris.iter().map(|t| [t[0] + off, t[1] + off, t[2] + off]));
    let mut labels = vec![vec![InputId(0)]; a.tris.len()];
    labels.extend(std::iter::repeat_n(vec![InputId(1)], b.tris.len()));

    // Arrangement (AR3b) → patches (BL1) → in/out (BL2), each loud.
    let soup =
        mesh_arrangement(&coords, &tris, &labels).map_err(NativeBooleanError::Arrangement)?;
    let patches = compute_all_patches(&soup).map_err(NativeBooleanError::Patches)?;
    let inner = compute_inside_out(&soup, &patches).map_err(NativeBooleanError::InsideOut)?;

    // Per-triangle labels: surface = the soup label (canonicalized sorted),
    // patch = the BL1 patch id, inside[t][k] = the triangle's patch's inner
    // label contains InputId(k).
    let n = soup.tris.len();
    let mut surface = Vec::with_capacity(n);
    let mut inside = Vec::with_capacity(n);
    let mut patch = Vec::with_capacity(n);
    for t in 0..n {
        let mut s = soup.labels[t].clone();
        s.sort_unstable();
        s.dedup();
        surface.push(s);
        let pid = patches.tri_to_patch[t];
        patch.push(pid);
        let inn = &inner[pid as usize];
        inside.push((0..2u32).map(|k| inn.contains(&InputId(k))).collect());
    }

    // Explicit DESCALED mesh over the REFERENCED vertices only, compacted
    // in first-reference order (the C++ computeFinalExplicitResult
    // `vertex_index` walk) — the jolly tail and any unreferenced vertices
    // never enter the output.
    let mut remap: Vec<Option<u32>> = vec![None; soup.verts.len()];
    let mut out_verts: Vec<Point3> = Vec::new();
    let mut out_tris: Vec<[u32; 3]> = Vec::with_capacity(n);
    for tri in &soup.tris {
        let mut g = [0u32; 3];
        for (k, &v) in tri.iter().enumerate() {
            g[k] = match remap[v as usize] {
                Some(id) => id,
                None => {
                    let id = out_verts.len() as u32;
                    out_verts.push(emit_vertex(&soup, v)?);
                    remap[v as usize] = Some(id);
                    id
                }
            };
        }
        out_tris.push(g);
    }

    Ok(LabeledArrangement {
        mesh: Mesh::new(out_verts, out_tris),
        surface,
        inside,
        patch,
        num_inputs: 2,
    })
}

/// Resolve one soup vertex to explicit DESCALED f64 coordinates: exact
/// rational evaluation (`exact_point_coords`, pure dashu) → nearest f64 →
/// divide by the soup's power-of-two multiplier (exact). Mirrors the C++
/// `getApproxXYZCoordinates` + `c /= multiplier` emission.
fn emit_vertex(soup: &ArrangementSoup, v: u32) -> Result<Point3, NativeBooleanError> {
    let xc = exact_point_coords(&soup.verts[v as usize])
        .ok_or(NativeBooleanError::UnresolvableVertex { vert: v })?;
    let m = soup.multiplier;
    let c = |i: usize| xc[i].to_f64().value() / m;
    Ok(Point3::new(c(0), c(1), c(2)))
}

/// The native, pure-pipeline [`MeshBoolean`](crate::boolean::MeshBoolean)
/// backend: `mesh_arrangement` → BL1 → BL2 → `keep_set(op)` → explicit
/// output mesh. Same contract as `cherchi_sidecar_rs::SidecarBoolean`,
/// which serves as its differential-parity oracle.
pub struct NativeBoolean;

impl crate::boolean::MeshBoolean for NativeBoolean {
    fn boolean(
        &self,
        a: &Mesh,
        b: &Mesh,
        op: BoolOp,
    ) -> Result<Mesh, Box<dyn std::error::Error + Send + Sync>> {
        let la = native_labeled_arrangement(a, b)?;
        let keep = la.keep_set(op);

        // Emit the kept triangles with compacted vertices (first-reference
        // order), applying the per-op orientation fix at emission (the
        // boolSubtraction / boolXOR flip loops — see module docs).
        let mut remap: Vec<Option<u32>> = vec![None; la.mesh.verts.len()];
        let mut out_verts: Vec<Point3> = Vec::new();
        let mut out_tris: Vec<[u32; 3]> = Vec::with_capacity(keep.len());
        for t in keep {
            let mut tri = la.mesh.tris[t];
            if flip_at_emission(&la, op, t) {
                tri.swap(1, 2);
            }
            let mut g = [0u32; 3];
            for (k, &v) in tri.iter().enumerate() {
                g[k] = match remap[v as usize] {
                    Some(id) => id,
                    None => {
                        let id = out_verts.len() as u32;
                        out_verts.push(la.mesh.verts[v as usize]);
                        remap[v as usize] = Some(id);
                        id
                    }
                };
            }
            out_tris.push(g);
        }
        Ok(Mesh::new(out_verts, out_tris))
    }

    fn labeled_arrangement(
        &self,
        a: &Mesh,
        b: &Mesh,
    ) -> Result<LabeledArrangement, Box<dyn std::error::Error + Send + Sync>> {
        Ok(native_labeled_arrangement(a, b)?)
    }
}

/// Per-op orientation fix for a KEPT triangle `t`:
/// - `boolSubtraction` (booleans.cpp:1480-1485) flips kept triangles NOT on
///   A's surface (the cavity wall, which faces into A's interior);
/// - `boolXOR` (booleans.cpp:1506-1510) flips kept triangles whose inside
///   set is non-empty (each shell's wall toward the intersection region);
/// - union / intersection keep the original winding (no flip loop in the
///   C++ `boolUnion` / `boolIntersection`).
fn flip_at_emission(la: &LabeledArrangement, op: BoolOp, t: usize) -> bool {
    match op {
        BoolOp::Union | BoolOp::Intersect => false,
        BoolOp::Subtract => !la.surface[t].contains(&InputId(0)),
        BoolOp::Xor => la.inside[t].iter().any(|&b| b),
    }
}

// =========================================================================
// RED oracle tests (PR-CR-BL3a)
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangements::soup::mesh_arrangement;
    use crate::boolean::MeshBoolean;
    use crate::labeled_arrangement::InputId;
    use crate::labeling::patches::compute_all_patches;
    use cad_primitives::Point3;
    use std::collections::BTreeMap;

    const A: InputId = InputId(0);
    const B: InputId = InputId(1);

    // ----- fixtures (the BL1/BL2 suites' geometry, as `Mesh` inputs) -------

    /// Axis-aligned box with per-axis sizes, outward-wound (the BL2 fixture
    /// winding: bottom face [0,2,1]/[0,3,2] has normal −z).
    fn boxx_mesh(ox: f64, oy: f64, oz: f64, sx: f64, sy: f64, sz: f64) -> Mesh {
        let p = |x: f64, y: f64, z: f64| Point3::new(ox + x * sx, oy + y * sy, oz + z * sz);
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(1.0, 1.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
            p(1.0, 0.0, 1.0),
            p(1.0, 1.0, 1.0),
            p(0.0, 1.0, 1.0),
        ];
        let tris = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [2, 3, 7],
            [2, 7, 6],
            [1, 2, 6],
            [1, 6, 5],
            [3, 0, 4],
            [3, 4, 7],
        ];
        Mesh::new(verts, tris)
    }

    fn cube_mesh(ox: f64, oy: f64, oz: f64, s: f64) -> Mesh {
        boxx_mesh(ox, oy, oz, s, s, s)
    }

    /// The concat shape `native_labeled_arrangement` is specified to feed
    /// the arrangement (oracle-side copy, for the BL1 patch recomputation).
    fn concat_inputs(a: &Mesh, b: &Mesh) -> (Vec<f64>, Vec<[u32; 3]>, Vec<Vec<InputId>>) {
        let mut coords = Vec::with_capacity(3 * (a.verts.len() + b.verts.len()));
        for p in a.verts.iter().chain(b.verts.iter()) {
            coords.push(p.x());
            coords.push(p.y());
            coords.push(p.z());
        }
        let off = a.verts.len() as u32;
        let mut tris = a.tris.clone();
        tris.extend(b.tris.iter().map(|t| [t[0] + off, t[1] + off, t[2] + off]));
        let mut labels = vec![vec![A]; a.tris.len()];
        labels.extend(std::iter::repeat_n(vec![B], b.tris.len()));
        (coords, tris, labels)
    }

    // ----- independent truth helpers ---------------------------------------

    /// Signed volume by the divergence theorem: sum of signed tetra volumes
    /// of each triangle against the origin. Positive ⟺ outward-consistent
    /// closed surface.
    fn signed_volume(mesh: &Mesh) -> f64 {
        mesh.tris
            .iter()
            .map(|t| {
                let a = mesh.verts[t[0] as usize];
                let b = mesh.verts[t[1] as usize];
                let c = mesh.verts[t[2] as usize];
                (a.x() * (b.y() * c.z() - b.z() * c.y()) - a.y() * (b.x() * c.z() - b.z() * c.x())
                    + a.z() * (b.x() * c.y() - b.y() * c.x()))
                    / 6.0
            })
            .sum()
    }

    /// Per undirected edge: (incident-triangle count, directed balance).
    /// Balance 0 means each direction is used equally often — orientation
    /// consistency across the edge.
    fn edge_stats(mesh: &Mesh) -> BTreeMap<(u32, u32), (usize, i64)> {
        let mut m: BTreeMap<(u32, u32), (usize, i64)> = BTreeMap::new();
        for t in &mesh.tris {
            for k in 0..3 {
                let (u, v) = (t[k], t[(k + 1) % 3]);
                let key = (u.min(v), u.max(v));
                let e = m.entry(key).or_insert((0, 0));
                e.0 += 1;
                e.1 += if u < v { 1 } else { -1 };
            }
        }
        m
    }

    /// Watertight, 2-manifold, orientation-consistent: every undirected
    /// edge has exactly 2 incident triangles, one per direction.
    fn assert_watertight_manifold(mesh: &Mesh, what: &str) {
        assert!(
            !mesh.tris.is_empty(),
            "{what}: output mesh must be non-empty"
        );
        for (edge, (count, balance)) in edge_stats(mesh) {
            assert_eq!(
                count, 2,
                "{what}: edge {edge:?} must have exactly 2 incident tris"
            );
            assert_eq!(
                balance, 0,
                "{what}: edge {edge:?} must be used once per direction"
            );
        }
    }

    fn assert_rel_eq(got: f64, expect: f64, what: &str) {
        let tol = expect.abs() * 1e-9;
        assert!(
            (got - expect).abs() <= tol,
            "{what}: volume {got} != expected {expect} (tol {tol})"
        );
    }

    /// Euler characteristic V − E + F over the (referenced) mesh.
    fn euler_characteristic(mesh: &Mesh) -> i64 {
        let mut referenced: Vec<bool> = vec![false; mesh.verts.len()];
        for t in &mesh.tris {
            for &v in t {
                referenced[v as usize] = true;
            }
        }
        let v = referenced.iter().filter(|&&r| r).count() as i64;
        let e = edge_stats(mesh).len() as i64;
        let f = mesh.tris.len() as i64;
        v - e + f
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #1 — contract invariants on the corner-overlap cube
    // fixture (A=[0,2]³, B=[1,3]³): I1/I2, patch ids match a BL1
    // recomputation, surface labels sorted + match the soup's, mesh
    // verts finite + DESCALED + compact (no jolly, no unreferenced).
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn labeled_arrangement_contract_invariants() {
        crate::arrangements::require_ffi_shim();
        let a = cube_mesh(0.0, 0.0, 0.0, 2.0);
        let b = cube_mesh(1.0, 1.0, 1.0, 2.0);
        let la = native_labeled_arrangement(&a, &b).expect("native labeled arrangement");

        let n = la.mesh.tris.len();
        assert!(n > 0, "arrangement of overlapping cubes must be non-empty");
        assert_eq!(la.num_inputs, 2);

        // I1: per-tri arrays 1:1 with mesh.tris.
        assert_eq!(la.surface.len(), n, "I1: surface len");
        assert_eq!(la.inside.len(), n, "I1: inside len");
        assert_eq!(la.patch.len(), n, "I1: patch len");

        // I2 + canonicalization: surface non-empty + sorted; inside len 2.
        for t in 0..n {
            assert!(!la.surface[t].is_empty(), "I2: surface[{t}] non-empty");
            let mut sorted = la.surface[t].clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(la.surface[t], sorted, "surface[{t}] sorted + deduped");
            assert_eq!(la.inside[t].len(), 2, "I2: inside[{t}] len == num_inputs");
        }

        // Patch ids and surface labels match an independent BL1 recompute
        // over the same concat inputs.
        let (coords, tris, labels) = concat_inputs(&a, &b);
        let soup = mesh_arrangement(&coords, &tris, &labels).expect("arrangement");
        let patches = compute_all_patches(&soup).expect("patches");
        assert_eq!(n, soup.tris.len(), "one output tri per soup tri");
        assert_eq!(
            la.patch, patches.tri_to_patch,
            "patch ids must match the BL1 recomputation"
        );
        for t in 0..n {
            let mut want = soup.labels[t].clone();
            want.sort_unstable();
            assert_eq!(la.surface[t], want, "surface[{t}] matches the soup label");
        }

        // Mesh verts finite; every tri index in range.
        for (i, p) in la.mesh.verts.iter().enumerate() {
            assert!(
                p.x().is_finite() && p.y().is_finite() && p.z().is_finite(),
                "vert {i} must be finite: {p:?}"
            );
        }
        let mut referenced = vec![false; la.mesh.verts.len()];
        for t in &la.mesh.tris {
            for &v in t {
                referenced[v as usize] = true;
            }
        }
        // No unreferenced verts (jolly tail + orphans dropped): every
        // output vertex is used by some triangle, and the count equals the
        // number of distinct vertices the soup's triangles reference.
        assert!(
            referenced.iter().all(|&r| r),
            "every output vertex must be referenced by a triangle"
        );
        let mut soup_used: Vec<bool> = vec![false; soup.verts.len()];
        for t in &soup.tris {
            for &v in t {
                soup_used[v as usize] = true;
            }
        }
        let distinct = soup_used.iter().filter(|&&u| u).count();
        assert_eq!(
            la.mesh.verts.len(),
            distinct,
            "output verts == distinct referenced soup verts (jolly + orphans gone)"
        );

        // DESCALED coordinates: known input corners appear at their
        // ORIGINAL unscaled positions, and everything lies in the input
        // bounding box (scaled coords would be 4× out for this fixture).
        let has = |x: f64, y: f64, z: f64| {
            la.mesh
                .verts
                .iter()
                .any(|p| p.x() == x && p.y() == y && p.z() == z)
        };
        assert!(
            has(0.0, 0.0, 0.0),
            "cube A corner (0,0,0) at unscaled coords"
        );
        assert!(
            has(2.0, 2.0, 2.0),
            "cube A corner (2,2,2) at unscaled coords"
        );
        assert!(
            has(3.0, 3.0, 3.0),
            "cube B corner (3,3,3) at unscaled coords"
        );
        for p in &la.mesh.verts {
            for c in [p.x(), p.y(), p.z()] {
                assert!(
                    (0.0..=3.0).contains(&c),
                    "vert {p:?} outside the unscaled input bbox [0,3]³"
                );
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #2 — boolean ground truth on A=[0,2]³, B=[1,3]³ (overlap
    // volume 1): Union=15, Intersection=1, Subtraction(A−B)=7, each
    // watertight 2-manifold and outward-consistent (signed volume
    // POSITIVE, divergence theorem).
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn boolean_volumes_match_analytic_truth() {
        crate::arrangements::require_ffi_shim();
        let a = cube_mesh(0.0, 0.0, 0.0, 2.0);
        let b = cube_mesh(1.0, 1.0, 1.0, 2.0);

        for (op, expect, name) in [
            (BoolOp::Union, 15.0, "union"),
            (BoolOp::Intersect, 1.0, "intersection"),
            (BoolOp::Subtract, 7.0, "subtraction A-B"),
        ] {
            let out = NativeBoolean.boolean(&a, &b, op).expect(name);
            assert_watertight_manifold(&out, name);
            let vol = signed_volume(&out);
            assert!(
                vol > 0.0,
                "{name}: outward-consistent (positive signed volume), got {vol}"
            );
            assert_rel_eq(vol, expect, name);
        }

        // Xor keeps BOTH shells (A−B and B−A) in one mesh; they touch along
        // the intersection curve, whose edges carry 4 incident triangles
        // (2 per shell) — NOT 2-manifold there, by Cherchi's construction
        // (boolXOR keeps each wall triangle once, flipped when inside).
        // Assert what actually holds: every edge has an even incident count
        // (2 or 4) with balanced directions, and the signed volume is
        // vol(A) + vol(B) − 2·overlap = 8 + 8 − 2 = 14.
        let out = NativeBoolean.boolean(&a, &b, BoolOp::Xor).expect("xor");
        assert!(!out.tris.is_empty(), "xor: output mesh must be non-empty");
        for (edge, (count, balance)) in edge_stats(&out) {
            assert!(
                count == 2 || count == 4,
                "xor: edge {edge:?} must have 2 or 4 incident tris, got {count}"
            );
            assert_eq!(balance, 0, "xor: edge {edge:?} direction-balanced");
        }
        let vol = signed_volume(&out);
        assert!(vol > 0.0, "xor: positive signed volume, got {vol}");
        assert_rel_eq(vol, 14.0, "xor");
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #3 — through-cut: square peg B=[0.5,1.5]²×[−1,3] pierces
    // cube A=[0,2]³ along z. Subtraction(A−B) = 8 − 1·1·2 = 6;
    // watertight 2-manifold; genus-1 (torus-like) ⇒ χ = V−E+F = 0.
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn through_cut_subtraction_is_genus_one() {
        crate::arrangements::require_ffi_shim();
        let a = cube_mesh(0.0, 0.0, 0.0, 2.0);
        let b = boxx_mesh(0.5, 0.5, -1.0, 1.0, 1.0, 4.0);

        let out = NativeBoolean
            .boolean(&a, &b, BoolOp::Subtract)
            .expect("through-cut subtraction");
        assert_watertight_manifold(&out, "through-cut A-B");
        let vol = signed_volume(&out);
        assert!(vol > 0.0, "through-cut: positive signed volume, got {vol}");
        assert_rel_eq(vol, 6.0, "through-cut A-B");
        assert_eq!(
            euler_characteristic(&out),
            0,
            "through-cut result is a torus-like genus-1 surface (χ = 0)"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #4 — determinism: two runs produce byte-identical results
    // (PartialEq over f64 coords is exact equality).
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn two_runs_are_byte_identical() {
        crate::arrangements::require_ffi_shim();
        let a = cube_mesh(0.0, 0.0, 0.0, 2.0);
        let b = cube_mesh(1.0, 1.0, 1.0, 2.0);

        let la1 = native_labeled_arrangement(&a, &b).expect("run 1");
        let la2 = native_labeled_arrangement(&a, &b).expect("run 2");
        assert!(
            !la1.mesh.tris.is_empty(),
            "determinism fixture must produce a non-empty arrangement"
        );
        assert_eq!(la1, la2, "labeled arrangement deterministic");

        let m1 = NativeBoolean
            .boolean(&a, &b, BoolOp::Subtract)
            .expect("bool run 1");
        let m2 = NativeBoolean
            .boolean(&a, &b, BoolOp::Subtract)
            .expect("bool run 2");
        assert_eq!(m1, m2, "boolean output deterministic");
    }

    // ════════════════════════════════════════════════════════════════
    // Oracle #5 — loud typed errors: a real coplanar-overlap pair (two
    // stacked cubes overlapping on the z=2 plane) must surface as
    // NativeBooleanError::Arrangement(CoplanarPairDeferred), never as a
    // silent wrong result (deviation N17 / Stage-0 M8 deferral).
    // ════════════════════════════════════════════════════════════════
    #[test]
    fn coplanar_overlap_is_loudly_deferred() {
        crate::arrangements::require_ffi_shim();
        let a = cube_mesh(0.0, 0.0, 0.0, 2.0);
        let b = cube_mesh(1.0, 1.0, 2.0, 2.0); // bottom face overlaps A's top
        match native_labeled_arrangement(&a, &b) {
            Err(NativeBooleanError::Arrangement(ArrangementError::CoplanarPairDeferred {
                ..
            })) => {}
            other => panic!("expected Arrangement(CoplanarPairDeferred), got {other:?}"),
        }
    }
}
