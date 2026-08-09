//! I1 reference-parity oracle for the patch-label tolerance cycle
//! (spec `specs/cherchi_patch_label_tolerance.md` §5). Dev-only, `#[ignore]`d and
//! sidecar-required per the established parity convention
//! (`parity_native_vs_sidecar.rs`); run with:
//!
//! ```text
//! cargo test -p cherchi-rs --test r0046_patch_label_parity -- --ignored
//! ```
//!
//! ## What it pins
//!
//! R0046's EXACT post-Stage-0 coplanar meshes (banked as
//! `tests/fixtures/r0046_stage0_{a,b}.obj` — a disc-cap × box-face coplanar
//! pair whose arrangement produces the merged `[A,B]` overlap sheet that walled
//! the native port on `PatchError::LabelMismatch`). After the L2a/L2b fix the
//! native `NativeBoolean` must SUCCEED and its Subtract output must match the
//! C++ `mesh_booleans` reference (via `SidecarBoolean`) — the binding I1 oracle
//! that the tolerant patch labeling produces reference-CORRECT geometry, not
//! merely a non-crashing result (spec §4 I1, §6 failure mode).
//!
//! ## Metric (triangulation- and coplanar-sheet-invariant)
//!
//! A coplanar boolean's raw mesh output is not cleanly 2-manifold (the doubled
//! overlap sheet), and native vs C++ subdivide the shared sheet's 2D region
//! differently (measured ~3.5e-3-apart on-sheet points), so the diff never
//! compares triangle lists, Euler characteristic, or the vertex set — those are
//! not invariant for coplanar output (which is why `parity_native_vs_sidecar`
//! excludes coplanar inputs). The binding metrics are signed volume (divergence
//! theorem) and surface area — both invariant under sheet re-triangulation and
//! both measured bit-identical here. NO tolerance widening (P9/P10): a failing
//! metric is a real divergence, per spec §6 STOP.

use std::path::PathBuf;

use cad_primitives::{BoolOp, Point3};
use cherchi_rs::labeling::NativeBoolean;
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::{obj, SidecarBoolean, SidecarError};

fn fixture(name: &str) -> Mesh {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    obj::read_obj(&path).unwrap_or_else(|e| panic!("read {name}: {e}"))
}

/// Loud sidecar handle — PANICS if the C++ binary is missing (a silently
/// skipped reference oracle is a false GREEN, P9), mirroring
/// `parity_native_vs_sidecar::sidecar`.
fn sidecar() -> SidecarBoolean {
    match SidecarBoolean::from_env() {
        Ok(sb) => sb,
        Err(SidecarError::BinaryNotFound { path }) => panic!(
            "reference-parity oracle unavailable: mesh_booleans binary not found at {} \
             (set CHERCHI2022_BIN or build per docs/sidecar/cherchi2022_build_guide.md). \
             Refusing to skip — this IS the I1 correctness gate.",
            path.display()
        ),
        Err(e) => panic!("sidecar setup failed: {e}"),
    }
}

fn weld(mesh: &Mesh) -> Mesh {
    use std::collections::BTreeMap;
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
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
        })
        .sum()
}

fn euler_characteristic(mesh: &Mesh) -> i64 {
    use std::collections::BTreeSet;
    let mut referenced = vec![false; mesh.verts.len()];
    let mut edges: BTreeSet<(u32, u32)> = BTreeSet::new();
    for t in &mesh.tris {
        for k in 0..3 {
            referenced[t[k] as usize] = true;
            let (u, v) = (t[k], t[(k + 1) % 3]);
            edges.insert((u.min(v), u.max(v)));
        }
    }
    let v = referenced.iter().filter(|&&r| r).count() as i64;
    v - edges.len() as i64 + mesh.tris.len() as i64
}

/// I1 (spec §4/§5): native Subtract on R0046's coplanar meshes SUCCEEDS (the
/// LabelMismatch wall is gone) and matches the C++ reference on the
/// triangulation-invariant metrics.
#[test]
#[ignore = "dev-only reference-parity oracle (requires the C++ mesh_booleans sidecar); \
            run explicitly: cargo test -p cherchi-rs --test r0046_patch_label_parity -- --ignored"]
fn r0046_subtract_matches_sidecar() {
    let a = fixture("r0046_stage0_a.obj");
    let b = fixture("r0046_stage0_b.obj");
    let sb = sidecar();
    let op = BoolOp::Subtract;

    // Core of this cycle: the native patch flood no longer walls on
    // LabelMismatch at the merged [A,B] coplanar sheet.
    let native = weld(
        &NativeBoolean
            .boolean(&a, &b, op)
            .expect("L2a: native Subtract must not wall on LabelMismatch"),
    );
    let reference = weld(&sb.boolean(&a, &b, op).expect("sidecar Subtract"));

    // Binding I1 metrics for a COPLANAR fixture = signed volume + surface area,
    // both triangulation-AND-coplanar-sheet-invariant. Euler and the vertex set
    // are NOT invariant here: native and C++ subdivide the shared overlap
    // sheet's 2D region differently (measured ~3.5e-3-apart on-sheet points,
    // zero net volume/area) — precisely why `parity_native_vs_sidecar` excludes
    // coplanar inputs. A bit-level volume+area match between two independent
    // implementations certifies the tolerant patch labeling kept the
    // reference-correct triangles (spec §4 I1, §6 keep-rule check). NO tolerance
    // widening (P9/P10): the measured agreement is ~1e-16 relative; the 1e-9
    // gate has margin but is not the reason it passes.
    let scale = {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for v in a.verts.iter().chain(b.verts.iter()) {
            let c = v.as_array();
            for k in 0..3 {
                lo[k] = lo[k].min(c[k]);
                hi[k] = hi[k].max(c[k]);
            }
        }
        (hi[0] - lo[0])
            .abs()
            .max((hi[1] - lo[1]).abs())
            .max((hi[2] - lo[2]).abs())
    };
    let (nv, rv) = (signed_volume(&native), signed_volume(&reference));
    let vol_floor = (scale * scale * scale).max(rv.abs());
    assert!(
        (nv - rv).abs() <= 1e-9 * vol_floor,
        "signed volume: native {nv:e} vs sidecar {rv:e} (floor {vol_floor:e})"
    );
    let (na, ra) = (surface_area(&native), surface_area(&reference));
    let area_floor = (scale * scale).max(ra.abs());
    assert!(
        (na - ra).abs() <= 1e-9 * area_floor,
        "surface area: native {na:e} vs sidecar {ra:e} (floor {area_floor:e})"
    );

    eprintln!(
        "[r0046-parity] volume native={nv:e} sidecar={rv:e} | area native={na:e} sidecar={ra:e} | \
         euler {}/{} tris {}/{} verts {}/{} (Euler/vertex-set differ = coplanar-sheet \
         triangulation, diagnostic only)",
        euler_characteristic(&native),
        euler_characteristic(&reference),
        native.tris.len(),
        reference.tris.len(),
        native.verts.len(),
        reference.verts.len(),
    );
}
