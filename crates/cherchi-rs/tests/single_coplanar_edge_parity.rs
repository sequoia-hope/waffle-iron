//! PR (deviation N13): REFERENCE PARITY of the native single-coplanar-edge
//! arrangement against the upstream C++ `mesh_booleans` binary (Cherchi 2022),
//! on the **edge-contained** sub-config this slice now classifies (was a loud
//! `Deferred(SingleCoplanarEdge)` / `CoplanarPairDeferred`).
//!
//! Per crate CLAUDE.md hard rule #2 and roadmap §6, reference parity IS the
//! correctness oracle for a port. The single-coplanar-edge sub-config is a
//! degenerate solid-solid contact (a measure-zero edge touch) for which the
//! BOOLEAN labeling diverges between backends (see the EXCLUDED_FIXTURES note
//! in `parity_native_vs_sidecar.rs`), so this suite compares at the
//! ARRANGEMENT level under the triangulation-INDEPENDENT vertex-set metric:
//! every vertex the native arrangement introduces (input verts + the
//! coplanar-edge contact points) must have a sidecar-arrangement vertex within
//! `VERT_TOL`, and vice-versa. The arrangement is defined for arbitrary inputs
//! in BOTH implementations, so the metric applies verbatim.
//!
//! ## Sidecar requirement — LOUD
//!
//! A missing `mesh_booleans` binary PANICS with an actionable message (P9: a
//! silently-skipped reference oracle is the worst failure mode).
//!
//! ## Scope
//!
//! The edge-CROSSING sub-config (a coplanar edge that enters/exits the other
//! triangle through its edges, needing the in-plane edge-edge jolly-LPI path)
//! is still loudly `Deferred(SingleCoplanarEdge)` and is NOT in this corpus —
//! it has no parity cell here by design (documented deferral).

use std::time::Duration;

use cad_primitives::Point3;
use cherchi_rs::native_labeled_arrangement;
use cherchi_rs::Mesh;
use cherchi_sidecar_rs::SidecarError;

const VERT_TOL: f64 = 1e-6;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Axis-aligned box (outward winding), origin corner + per-axis sizes.
fn boxx(ox: f64, oy: f64, oz: f64, sx: f64, sy: f64, sz: f64) -> Mesh {
    let c = |x: f64, y: f64, z: f64| p(ox + x * sx, oy + y * sy, oz + z * sz);
    let verts = vec![
        c(0.0, 0.0, 0.0),
        c(1.0, 0.0, 0.0),
        c(1.0, 1.0, 0.0),
        c(0.0, 1.0, 0.0),
        c(0.0, 0.0, 1.0),
        c(1.0, 0.0, 1.0),
        c(1.0, 1.0, 1.0),
        c(0.0, 1.0, 1.0),
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

/// Signed volume (divergence theorem); flips a tetra to outward winding.
fn signed_volume(m: &Mesh) -> f64 {
    m.tris
        .iter()
        .map(|t| {
            let a = m.verts[t[0] as usize];
            let b = m.verts[t[1] as usize];
            let c = m.verts[t[2] as usize];
            (a.x() * (b.y() * c.z() - b.z() * c.y()) - a.y() * (b.x() * c.z() - b.z() * c.x())
                + a.z() * (b.x() * c.y() - b.y() * c.x()))
                / 6.0
        })
        .sum()
}

fn tetra(a: Point3, b: Point3, c: Point3, d: Point3) -> Mesh {
    let mut m = Mesh::new(
        vec![a, b, c, d],
        vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    );
    if signed_volume(&m) < 0.0 {
        for t in &mut m.tris {
            t.swap(1, 2);
        }
    }
    m
}

/// Single-coplanar-edge CONTAINED fixture: cube A = [0,2]³ (top face z = 2),
/// tetra B with ONE bottom edge lying flush in A's top-face plane (z = 2),
/// STRICTLY INSIDE the top face, the other two B vertices ABOVE (z > 2). Thus
/// exactly one edge of B is coplanar with A's top face (the singleCoplanarEdge
/// configuration), contained — no edge crossing of A's top-face boundary.
///
/// Coplanar edge endpoints: (0.5, 0.5, 2) and (1.5, 1.0, 2) — both inside the
/// [0,2]² top face. Apexes (0.75, 0.6, 3) and (1.2, 1.3, 3) above.
fn cube_and_edge_contact_tetra() -> (Mesh, Mesh) {
    let a = boxx(0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let b = tetra(
        p(0.5, 0.5, 2.0),
        p(1.5, 1.0, 2.0),
        p(0.75, 0.6, 3.0),
        p(1.2, 1.3, 3.0),
    );
    (a, b)
}

/// Exact-coordinate vertex weld (bit-identical merge); returns the vertex list.
fn welded_verts(m: &Mesh) -> Vec<Point3> {
    use std::collections::BTreeMap;
    let mut index: BTreeMap<[u64; 3], ()> = BTreeMap::new();
    let mut verts = Vec::new();
    for v in &m.verts {
        let key = [v.x().to_bits(), v.y().to_bits(), v.z().to_bits()];
        if index.insert(key, ()).is_none() {
            verts.push(*v);
        }
    }
    verts
}

/// Every vertex of `from` has a vertex of `to` within `tol`. Returns the first
/// offender (Hausdorff-0, one direction).
fn vertex_cover_gap(from: &[Point3], to: &[Point3], tol: f64) -> Option<Point3> {
    let t2 = tol * tol;
    'outer: for v in from {
        for w in to {
            let dx = v.x() - w.x();
            let dy = v.y() - w.y();
            let dz = v.z() - w.z();
            if dx * dx + dy * dy + dz * dz <= t2 {
                continue 'outer;
            }
        }
        return Some(*v);
    }
    None
}

/// Loud sidecar arrangement handle (P9): PANICS if the binary is missing.
fn sidecar_arrangement(a: &Mesh, b: &Mesh) -> Mesh {
    match cherchi_sidecar_rs::labeled_arrangement(a, b, Duration::from_secs(60)) {
        Ok(la) => la.mesh,
        Err(SidecarError::BinaryNotFound { path }) => panic!(
            "reference-parity oracle unavailable: mesh_booleans binary not \
             found at {} (set CHERCHI2022_BIN or build per \
             docs/sidecar/cherchi2022_build_guide.md). Refusing to skip — \
             this suite is the single-coplanar-edge correctness gate.",
            path.display()
        ),
        Err(e) => panic!("sidecar labeled_arrangement failed: {e}"),
    }
}

/// Arrangement-level reference parity on the edge-contained single-coplanar-
/// edge fixture: native arrangement vertex set ≡ sidecar arrangement vertex
/// set (Hausdorff-0 at `VERT_TOL`, both directions). The contact points of the
/// coplanar edge are the load-bearing new vertices.
#[test]
fn contained_single_coplanar_edge_arrangement_parity() {
    let (a, b) = cube_and_edge_contact_tetra();

    let native = native_labeled_arrangement(&a, &b)
        .expect("native arrangement must classify the contained coplanar edge, not defer");
    let native_v = welded_verts(&native.mesh);
    let sidecar_v = welded_verts(&sidecar_arrangement(&a, &b));

    if let Some(v) = vertex_cover_gap(&native_v, &sidecar_v, VERT_TOL) {
        panic!(
            "native arrangement vertex {v:?} has no sidecar vertex within {VERT_TOL} \
             (native {} verts, sidecar {} verts)",
            native_v.len(),
            sidecar_v.len()
        );
    }
    if let Some(v) = vertex_cover_gap(&sidecar_v, &native_v, VERT_TOL) {
        panic!(
            "sidecar arrangement vertex {v:?} has no native vertex within {VERT_TOL} \
             (native {} verts, sidecar {} verts)",
            native_v.len(),
            sidecar_v.len()
        );
    }
}
