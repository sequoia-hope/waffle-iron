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

/// Single-coplanar-edge edge-CROSSING fixture: cube A = [0,2]³ (top face
/// z = 2), tetra B with ONE bottom edge lying flush in A's top-face plane
/// (z = 2) but running from INSIDE the top face to OUTSIDE it, so the coplanar
/// edge properly CROSSES the top-face boundary (the singleCoplanarEdge
/// edge-crossing configuration this slice adds). The other two B vertices are
/// ABOVE (z > 2).
///
/// Coplanar edge endpoints: (1.0, 1.0, 2) INSIDE the [0,2]² top face and
/// (3.0, 1.0, 2) OUTSIDE it (x > 2). The edge crosses A's top-face boundary
/// x = 2 at (2.0, 1.0, 2). Apexes (1.3, 0.7, 3) and (1.6, 1.4, 3) above.
fn cube_and_edge_crossing_tetra() -> (Mesh, Mesh) {
    let a = boxx(0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let b = tetra(
        p(1.0, 1.0, 2.0),
        p(3.0, 1.0, 2.0),
        p(1.3, 0.7, 3.0),
        p(1.6, 1.4, 3.0),
    );
    (a, b)
}

/// Tilted-slab fixture (positive-VOLUME interpenetration with a coplanar
/// contact edge through A's interior): cube A = [0,2]³ and a box B = [1,3] ×
/// [0,2] × [0,2] whose z-faces are tilted by an exact 1/8-per-x slope, so B's
/// bottom-face x = 1 edge (1,0,0)-(1,2,0) lies EXACTLY in A's z = 0 plane and
/// runs through A's bottom-face strict interior (a `SingleCoplanarEdge`
/// crossing the diagonal), while B's untilted y-faces overlap A's y-planes.
/// Unlike the measure-zero edge-touch fixtures, this is a real positive-volume
/// boolean, so the arrangement vertex sets must match the C++ reference
/// exactly. (Previously this whole config was a loud `CoplanarPairDeferred`;
/// the tvX/edge-crossing slice now constructs it — pinned here against the
/// reference, and asserted conforming in `soup.rs`'s adversary suite.)
fn cube_and_tilted_slab() -> (Mesh, Mesh) {
    let a = boxx(0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let s = 0.125_f64; // exact in binary
    let bx = |xi: usize| if xi == 0 { 1.0 } else { 3.0 };
    let by = |yi: usize| if yi == 0 { 0.0 } else { 2.0 };
    let mut verts = Vec::new();
    for zi in 0..2 {
        for yi in 0..2 {
            for xi in 0..2 {
                let x = bx(xi);
                let y = by(yi);
                let zbase = if zi == 0 { 0.0 } else { 2.0 };
                verts.push(p(x, y, zbase + s * (x - 1.0)));
            }
        }
    }
    let cid = |xi: usize, yi: usize, zi: usize| (zi * 4 + yi * 2 + xi) as u32;
    let quad = |a: u32, b: u32, c: u32, d: u32| vec![[a, b, c], [a, c, d]];
    let mut tris: Vec<[u32; 3]> = Vec::new();
    tris.extend(quad(cid(0, 0, 0), cid(1, 0, 0), cid(1, 1, 0), cid(0, 1, 0)));
    tris.extend(quad(cid(0, 0, 1), cid(1, 0, 1), cid(1, 1, 1), cid(0, 1, 1)));
    tris.extend(quad(cid(0, 0, 0), cid(1, 0, 0), cid(1, 0, 1), cid(0, 0, 1)));
    tris.extend(quad(cid(0, 1, 0), cid(1, 1, 0), cid(1, 1, 1), cid(0, 1, 1)));
    tris.extend(quad(cid(0, 0, 0), cid(0, 1, 0), cid(0, 1, 1), cid(0, 0, 1)));
    tris.extend(quad(cid(1, 0, 0), cid(1, 1, 0), cid(1, 1, 1), cid(1, 0, 1)));
    (a, Mesh::new(verts, tris))
}

/// Single-coplanar-edge tvX_in_edge fixture: cube A = [0,2]³ (top face z = 2,
/// split into triangles T0 = (0,0,2),(2,0,2),(2,2,2) and T1 = (0,0,2),(2,2,2),
/// (0,2,2)). Tetra B has ONE bottom edge in the z = 2 plane running from
/// (1, 0.5, 2) — STRICTLY INSIDE T0 — out THROUGH A's corner (2, 0, 2) (a
/// VERTEX of T0, strictly inside B's coplanar edge) to (3, -0.5, 2) outside.
/// So the coplanar edge crosses the other triangle at one of ITS CORNERS — the
/// `tvX_in_edge` config: the crossing point is the exact o_t vertex (2,0,2),
/// not a jolly-LPI. Apexes (1.3, 0.7, 3) and (1.6, 0.4, 3) above.
///
/// The load-bearing new arrangement vertex is (1, 0.5, 2) on A's top face;
/// (2, 0, 2) already exists.
fn cube_and_edge_corner_crossing_tetra() -> (Mesh, Mesh) {
    let a = boxx(0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let b = tetra(
        p(1.0, 0.5, 2.0),
        p(3.0, -0.5, 2.0),
        p(1.3, 0.7, 3.0),
        p(1.6, 0.4, 3.0),
    );
    (a, b)
}

/// Single-coplanar-edge COLLINEAR-OVERLAP fixture: cube A = [0,2]³ (top face
/// z = 2; boundary edge v4-v5 from (0,0,2) to (2,0,2) along y = 0). Tetra B has
/// ONE bottom edge collinear with that boundary edge, running from (1, 0, 2)
/// — strictly inside edge v4-v5 — to (3, 0, 2) outside, so the coplanar edge
/// partially OVERLAPS A's top-face boundary edge. Overlap = [(1,0,2), (2,0,2)].
/// Apexes (1.5, 0.4, 3) and (2.2, 0.4, 3) above.
///
/// The load-bearing new arrangement vertex is (1, 0, 2) on edge v4-v5;
/// (2, 0, 2) already exists.
fn cube_and_edge_collinear_overlap_tetra() -> (Mesh, Mesh) {
    let a = boxx(0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let b = tetra(
        p(1.0, 0.0, 2.0),
        p(3.0, 0.0, 2.0),
        p(1.5, 0.4, 3.0),
        p(2.2, 0.4, 3.0),
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

/// Arrangement-level reference parity on the edge-CROSSING single-coplanar-
/// edge fixture (deviation N13, this slice): the coplanar edge crosses the
/// other triangle's boundary, producing an in-plane edge-edge crossing vertex
/// (the C++ `addEdgeCrossEdgeInters` jolly-LPI). Native arrangement vertex set
/// ≡ sidecar arrangement vertex set (Hausdorff-0 at `VERT_TOL`, both
/// directions). The crossing point on A's top-face boundary is the load-
/// bearing new vertex.
#[test]
fn crossing_single_coplanar_edge_arrangement_parity() {
    let (a, b) = cube_and_edge_crossing_tetra();

    let native = native_labeled_arrangement(&a, &b)
        .expect("native arrangement must classify the crossing coplanar edge, not defer");
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

/// Arrangement-level reference parity on the tvX_in_edge single-coplanar-edge
/// fixture (deviation N13, this slice): the coplanar edge exits the other
/// triangle THROUGH one of its corners (a degenerate crossing whose crossing
/// point is the exact o_t vertex, the C++ `tvX_in_edge` symbolic branch). The
/// previously-deferred config must now classify; native arrangement vertex set
/// ≡ sidecar arrangement vertex set (Hausdorff-0 at `VERT_TOL`, both
/// directions). The interior endpoint (1, 0.5, 2) is the load-bearing new
/// vertex.
#[test]
fn corner_crossing_single_coplanar_edge_arrangement_parity() {
    let (a, b) = cube_and_edge_corner_crossing_tetra();

    let native = native_labeled_arrangement(&a, &b).expect(
        "native arrangement must classify the corner-crossing (tvX_in_edge) coplanar edge, not defer",
    );
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

/// Arrangement-level reference parity on the tilted-slab fixture (positive-
/// volume interpenetration whose coplanar contact edge runs through A's
/// interior). The previously-deferred config must now classify; native
/// arrangement vertex set ≡ sidecar arrangement vertex set (Hausdorff-0 at
/// `VERT_TOL`, both directions).
#[test]
fn tilted_slab_through_interior_arrangement_parity() {
    let (a, b) = cube_and_tilted_slab();

    let native = native_labeled_arrangement(&a, &b)
        .expect("native arrangement must classify the tilted-slab coplanar edge, not defer");
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

/// Arrangement-level reference parity on the collinear-overlap single-coplanar-
/// edge fixture (deviation N13, this slice): the coplanar edge runs collinear
/// with an edge of the other triangle and partially overlaps it. The
/// previously-deferred config must now classify; native arrangement vertex set
/// ≡ sidecar arrangement vertex set (Hausdorff-0 at `VERT_TOL`, both
/// directions). The overlap-start vertex (1, 0, 2) is the load-bearing new
/// vertex.
#[test]
fn collinear_overlap_single_coplanar_edge_arrangement_parity() {
    let (a, b) = cube_and_edge_collinear_overlap_tetra();

    let native = native_labeled_arrangement(&a, &b)
        .expect("native arrangement must classify the collinear-overlap coplanar edge, not defer");
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
