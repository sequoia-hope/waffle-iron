//! PR-4 MAKE-OR-BREAK gate: REFERENCE PARITY of the native cherchi-rs boolean
//! on **fully-coplanar overlapping** inputs against the upstream C++
//! `mesh_booleans` binary (Cherchi 2022), wrapped by
//! `cherchi_sidecar_rs::SidecarBoolean`.
//!
//! This is the test that two prior whole-port attempts FAILED silently: a
//! coplanar union that builds but DOUBLE-COUNTS the overlap (coaxial-cylinder
//! union vol 763 vs sidecar 750, 67 unpaired edges). The fix is the C++
//! pocket-level dedup keyed by boundary vertex-SET (`VisitedPocketRegistry`).
//!
//! GREEN ::= native boolean output equals the sidecar on every metric:
//! signed volume (the sidecar value — double-counting shows up here),
//! watertight 2-manifold (every edge paired, direction-balanced — a
//! double-counted overlap leaves unpaired edges), Euler characteristic, and
//! vertex-set Hausdorff-0. The metric is the same triangulation-independent
//! comparison used by `parity_native_vs_sidecar.rs` (studied for the harness).
//!
//! Fixtures (BOTH are fully-coplanar lateral / face overlaps — the previously
//! LOUD-deferred class):
//!   1. **coaxial-12gon-prisms** — two coaxial 12-gon "cylinders" r=5 with
//!      COINCIDENT lateral facets (same radius, same angular chords) but
//!      different z-extents (A: z∈[-5,5], B: z∈[-2,2], so B's laterals are a
//!      z-subsection of A's). The gear's exact case. UNION.
//!   2. **coaxial-4gon-prisms** — the same lateral-coincidence structure on
//!      a square prism (n=4): A r=2 z∈[0,4], B r=2 z∈[1,3]. UNION vol = 32.
//!      A simpler shape exercising the SAME pocket-dedup mechanism.
//!
//! ## Why NOT stacked cubes
//!
//! A first draft used cube [0,1]³ ∪ cube [0,1]²×[1,2] sharing the z=1 face.
//! That is the WRONG kind of fixture: the two coincident caps triangulate
//! along the same diagonal, so the overlap triangles are BIT-IDENTICAL
//! (opposite winding) and route through
//! `remove_degenerate_and_duplicated_triangles` / the dupl-triangles path,
//! NOT the pocket dedup. Worse, the C++ `mesh_booleans` reference ITSELF
//! produces a non-watertight 2.333-volume result on that raw input (verified
//! by direct binary invocation) — so it is not a valid parity oracle. The
//! lateral-coincidence prism fixtures are TRUE partial-overlap pockets
//! (B's facets are a z-subsection of A's, sharing the same vertical edges,
//! re-triangulated differently) which the sidecar resolves correctly
//! (750.0 / 32.0, watertight).
//!
//! Sidecar requirement is LOUD (P9): a missing binary PANICS, never skips.

use std::collections::BTreeMap;

use cad_primitives::{BoolOp, Point3};
use cherchi_rs::labeling::NativeBoolean;
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::{SidecarBoolean, SidecarError};

const REL_TOL: f64 = 1e-9;
const VERT_TOL: f64 = 1e-6;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

// ===========================================================================
// Fixture builders
// ===========================================================================

/// A regular `n`-gon prism: radius `r`, z-extent [z0, z1], n-gon vertices on
/// the angular grid `2πk/n` (k = 0..n). Outward-oriented watertight closed
/// surface: n lateral quads (2 tris each) + a top fan + a bottom fan.
fn ngon_prism(n: usize, r: f64, z0: f64, z1: f64) -> Mesh {
    let mut verts: Vec<Point3> = Vec::new();
    // bottom ring 0..n, top ring n..2n.
    for &z in &[z0, z1] {
        for k in 0..n {
            let a = std::f64::consts::TAU * (k as f64) / (n as f64);
            verts.push(p(r * a.cos(), r * a.sin(), z));
        }
    }
    // center vertices for the caps.
    let bot_c = verts.len() as u32;
    verts.push(p(0.0, 0.0, z0));
    let top_c = verts.len() as u32;
    verts.push(p(0.0, 0.0, z1));

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let b = |k: usize| (k % n) as u32; // bottom ring id
    let tp = |k: usize| (n + (k % n)) as u32; // top ring id
    for k in 0..n {
        // lateral quad split into two tris.
        tris.push([b(k), b(k + 1), tp(k + 1)]);
        tris.push([b(k), tp(k + 1), tp(k)]);
        // bottom cap.
        tris.push([bot_c, b(k + 1), b(k)]);
        // top cap.
        tris.push([top_c, tp(k), tp(k + 1)]);
    }
    oriented(Mesh::new(verts, tris))
}

/// Flip every triangle if the signed volume is negative (fixture-builder
/// guard: inputs must be outward-oriented closed surfaces).
fn oriented(mut m: Mesh) -> Mesh {
    let vol = signed_volume(&m);
    assert!(vol != 0.0, "fixture degenerate (zero signed volume)");
    if vol < 0.0 {
        for t in &mut m.tris {
            t.swap(1, 2);
        }
    }
    m
}

// ===========================================================================
// Metric helpers (copied in style from parity_native_vs_sidecar.rs)
// ===========================================================================

fn weld(mesh: &Mesh) -> Mesh {
    let mut index: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    let mut verts: Vec<Point3> = Vec::new();
    let mut remap: Vec<u32> = Vec::with_capacity(mesh.verts.len());
    for v in &mesh.verts {
        let key = [v.x().to_bits(), v.y().to_bits(), v.z().to_bits()];
        let id = *index.entry(key).or_insert_with(|| {
            verts.push(*v);
            (verts.len() - 1) as u32
        });
        remap.push(id);
    }
    let tris = mesh
        .tris
        .iter()
        .map(|t| {
            [
                remap[t[0] as usize],
                remap[t[1] as usize],
                remap[t[2] as usize],
            ]
        })
        .collect();
    Mesh::new(verts, tris)
}

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

fn unpaired_edge_count(mesh: &Mesh) -> usize {
    edge_stats(mesh).values().filter(|&&(c, _)| c != 2).count()
}

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

fn surface_area(mesh: &Mesh) -> f64 {
    mesh.tris
        .iter()
        .map(|t| {
            let a = mesh.verts[t[0] as usize].as_array();
            let b = mesh.verts[t[1] as usize].as_array();
            let c = mesh.verts[t[2] as usize].as_array();
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let nrm = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            0.5 * (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt()
        })
        .sum()
}

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

fn vertex_cover_gap(from: &Mesh, to: &Mesh, tol: f64) -> Option<Point3> {
    let t2 = tol * tol;
    'outer: for v in &from.verts {
        for w in &to.verts {
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

fn bbox_scales(a: &Mesh, b: &Mesh) -> (f64, f64) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in a.verts.iter().chain(b.verts.iter()) {
        let c = v.as_array();
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    let d = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let vol = d[0] * d[1] * d[2];
    let area = 2.0 * (d[0] * d[1] + d[1] * d[2] + d[2] * d[0]);
    (vol, area)
}

/// The triangulation-independent comparison; identical metric to the existing
/// parity suite. Returns Err(diagnostic) on the first failing metric.
fn compare_cell(
    native_raw: &Mesh,
    sidecar_raw: &Mesh,
    vol_scale: f64,
    area_scale: f64,
) -> Result<(), String> {
    let native = weld(native_raw);
    let sidecar = weld(sidecar_raw);

    if native.tris.is_empty() != sidecar.tris.is_empty() {
        return Err(format!(
            "one-sided empty result: native {} tris, sidecar {} tris",
            native.tris.len(),
            sidecar.tris.len()
        ));
    }
    if native.tris.is_empty() {
        return Ok(());
    }

    // (1) Watertight 2-manifold: every edge exactly 2 incident tris, balanced.
    for (name, mesh) in [("native", &native), ("sidecar", &sidecar)] {
        let unpaired = unpaired_edge_count(mesh);
        if unpaired != 0 {
            let first = edge_stats(mesh)
                .into_iter()
                .find(|(_, (c, _))| *c != 2)
                .map(|(e, (c, _))| format!("{e:?} has {c} incident tris"))
                .unwrap_or_default();
            return Err(format!(
                "{name}: NOT watertight — {unpaired} unpaired edges (e.g. {first})"
            ));
        }
        for (edge, (_, balance)) in edge_stats(mesh) {
            if balance != 0 {
                return Err(format!(
                    "{name}: edge {edge:?} direction-unbalanced (balance {balance})"
                ));
            }
        }
    }

    // (2) Signed volume (the double-count tell).
    let (vn, vs) = (signed_volume(&native), signed_volume(&sidecar));
    let vtol = REL_TOL * vs.abs().max(vol_scale);
    if (vn - vs).abs() > vtol {
        return Err(format!(
            "signed volume: native {vn:.9} vs sidecar {vs:.9} (tol {vtol:.3e})"
        ));
    }

    // (3) Surface area.
    let (an, as_) = (surface_area(&native), surface_area(&sidecar));
    let atol = REL_TOL * as_.abs().max(area_scale);
    if (an - as_).abs() > atol {
        return Err(format!(
            "surface area: native {an:.9} vs sidecar {as_:.9} (tol {atol:.3e})"
        ));
    }

    // (4) Euler characteristic.
    let (cn, cs) = (
        euler_characteristic(&native),
        euler_characteristic(&sidecar),
    );
    if cn != cs {
        return Err(format!("Euler characteristic: native {cn} vs sidecar {cs}"));
    }

    // (5) Vertex-set Hausdorff-0, both directions.
    if let Some(v) = vertex_cover_gap(&native, &sidecar, VERT_TOL) {
        return Err(format!(
            "native vertex {v:?} has no sidecar vertex within {VERT_TOL}"
        ));
    }
    if let Some(v) = vertex_cover_gap(&sidecar, &native, VERT_TOL) {
        return Err(format!(
            "sidecar vertex {v:?} has no native vertex within {VERT_TOL}"
        ));
    }

    Ok(())
}

fn sidecar() -> SidecarBoolean {
    match SidecarBoolean::from_env() {
        Ok(sb) => sb,
        Err(SidecarError::BinaryNotFound { path }) => panic!(
            "reference-parity oracle unavailable: mesh_booleans binary not found at {} \
             (set CHERCHI2022_BIN or build per docs/sidecar/cherchi2022_build_guide.md). \
             Refusing to skip — this is the PR-4 correctness gate.",
            path.display()
        ),
        Err(e) => panic!("sidecar setup failed: {e}"),
    }
}

/// Run UNION on one coplanar fixture and assert full parity, reporting both
/// volumes + unpaired-edge counts (the double-count tell) on the way.
fn run_union(name: &str, a: &Mesh, b: &Mesh) {
    let sb = sidecar();
    let (vol_scale, area_scale) = bbox_scales(a, b);
    let native = NativeBoolean
        .boolean(a, b, BoolOp::Union)
        .unwrap_or_else(|e| panic!("[{name}] native union failed: {e}"));
    let reference = sb
        .boolean(a, b, BoolOp::Union)
        .unwrap_or_else(|e| panic!("[{name}] SIDECAR union failed (harness): {e}"));

    let wn = weld(&native);
    let ws = weld(&reference);
    eprintln!(
        "[{name}] native union vol = {:.6} (unpaired edges {}), sidecar union vol = {:.6} (unpaired {})",
        signed_volume(&wn),
        unpaired_edge_count(&wn),
        signed_volume(&ws),
        unpaired_edge_count(&ws),
    );

    if let Err(msg) = compare_cell(&native, &reference, vol_scale, area_scale) {
        panic!(
            "[{name} × Union] parity FAILED: {msg}\n  (native vol {:.6}, sidecar vol {:.6})",
            signed_volume(&wn),
            signed_volume(&ws),
        );
    }
}

// ===========================================================================
// The gates
// ===========================================================================

/// MAKE-OR-BREAK: two coaxial 12-gon prisms with COINCIDENT laterals, UNION.
/// Prior attempts double-counted the lateral-facet overlap (763 vs 750).
#[test]
fn coaxial_12gon_prism_union_parity() {
    let a = ngon_prism(12, 5.0, -5.0, 5.0);
    let b = ngon_prism(12, 5.0, -2.0, 2.0);
    run_union("coaxial-12gon-prisms", &a, &b);
}

/// Simpler coplanar gate: coaxial SQUARE prisms with coincident laterals.
/// Same pocket-dedup mechanism as the 12-gon, smaller shape. Union vol = 32.
#[test]
fn coaxial_4gon_prism_union_parity() {
    let a = ngon_prism(4, 2.0, 0.0, 4.0);
    let b = ngon_prism(4, 2.0, 1.0, 3.0);
    run_union("coaxial-4gon-prisms", &a, &b);
}
