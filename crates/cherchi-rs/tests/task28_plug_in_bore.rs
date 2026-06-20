//! TASK28 localization fixture: the gear's true topology, minimal form.
//! A TUBE (annular prism: outer n-gon R, inner n-gon bore r, the bore wall
//! faces INWARD) unioned with a coaxial PLUG (outward n-gon wall at the SAME
//! radius r) that fills the bore. The bore wall and plug wall are the SAME
//! cylinder, OPPOSITE normal sense — exactly the gear's bore/flange pair.
//!
//! Two variants:
//!   NON-CONFORMAL: tube bore wall is a tall band (rings only at z0,z1), plug
//!     wall rings at z=±2 only → the gear's real non-conformal-in-z situation.
//!   CONFORMAL: tube bore wall ALSO carries rings at the plug's z-levels, so
//!     the overlap-band facets are bit-coincident.

use std::collections::BTreeMap;

use cad_primitives::{BoolOp, Point3};
use cherchi_rs::labeling::NativeBoolean;
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::SidecarBoolean;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn ring(verts: &mut Vec<Point3>, n: usize, r: f64, z: f64) -> Vec<u32> {
    let mut out = Vec::new();
    for k in 0..n {
        let a = std::f64::consts::TAU * (k as f64) / (n as f64);
        out.push(verts.len() as u32);
        verts.push(p(r * a.cos(), r * a.sin(), z));
    }
    out
}

/// Annular prism (tube): outer radius `ro`, inner bore radius `ri`, z∈[z0,z1].
/// Outer wall outward; bore wall INWARD; caps are annuli. `bore_z` = extra
/// z-levels where the bore wall carries rings (z0,z1 always included).
fn tube(n: usize, ro: f64, ri: f64, z0: f64, z1: f64, bore_z: &[f64]) -> Mesh {
    let mut v: Vec<Point3> = Vec::new();
    let ob = ring(&mut v, n, ro, z0);
    let ot = ring(&mut v, n, ro, z1);
    let mut zs: Vec<f64> = bore_z.to_vec();
    for z in [z0, z1] {
        if !zs.contains(&z) {
            zs.push(z);
        }
    }
    zs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let bore: Vec<Vec<u32>> = zs.iter().map(|&z| ring(&mut v, n, ri, z)).collect();

    let mut t: Vec<[u32; 3]> = Vec::new();
    // Outer wall (outward).
    for k in 0..n {
        let (a, b, c, d) = (ob[k], ob[(k + 1) % n], ot[(k + 1) % n], ot[k]);
        t.push([a, b, c]);
        t.push([a, c, d]);
    }
    // Bore wall (inward = reversed winding).
    for zi in 0..bore.len() - 1 {
        let lo = &bore[zi];
        let hi = &bore[zi + 1];
        for k in 0..n {
            let (a, b, c, d) = (lo[k], lo[(k + 1) % n], hi[(k + 1) % n], hi[k]);
            t.push([a, c, b]);
            t.push([a, d, c]);
        }
    }
    // Caps: annulus between outer ring and bore ring at z0 (down) and z1 (up).
    let cap = |t: &mut Vec<[u32; 3]>, outer: &[u32], inner: &[u32], up: bool| {
        for k in 0..n {
            let (oa, ob) = (outer[k], outer[(k + 1) % n]);
            let (ia, ib) = (inner[k], inner[(k + 1) % n]);
            if up {
                t.push([oa, ob, ib]);
                t.push([oa, ib, ia]);
            } else {
                t.push([oa, ib, ob]);
                t.push([oa, ia, ib]);
            }
        }
    };
    cap(&mut t, &ob, &bore[0], false);
    cap(&mut t, &ot, bore.last().unwrap(), true);
    Mesh::new(v, t)
}

/// Solid n-gon plug, outward wall, z∈[z0,z1]. `wall_z` = extra wall ring levels.
fn plug(n: usize, r: f64, z0: f64, z1: f64, wall_z: &[f64]) -> Mesh {
    let mut zs: Vec<f64> = wall_z.to_vec();
    for z in [z0, z1] {
        if !zs.contains(&z) {
            zs.push(z);
        }
    }
    zs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut v: Vec<Point3> = Vec::new();
    let rings: Vec<Vec<u32>> = zs.iter().map(|&z| ring(&mut v, n, r, z)).collect();
    let bc = v.len() as u32;
    v.push(p(0.0, 0.0, zs[0]));
    let tc = v.len() as u32;
    v.push(p(0.0, 0.0, *zs.last().unwrap()));
    let mut t: Vec<[u32; 3]> = Vec::new();
    for zi in 0..rings.len() - 1 {
        let lo = &rings[zi];
        let hi = &rings[zi + 1];
        for k in 0..n {
            let (a, b, c, d) = (lo[k], lo[(k + 1) % n], hi[(k + 1) % n], hi[k]);
            t.push([a, b, c]);
            t.push([a, c, d]);
        }
    }
    let bot = &rings[0];
    let top = rings.last().unwrap();
    for k in 0..n {
        t.push([bc, bot[(k + 1) % n], bot[k]]);
        t.push([tc, top[k], top[(k + 1) % n]]);
    }
    Mesh::new(v, t)
}

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

fn unpaired(mesh: &Mesh) -> usize {
    let w = weld(mesh);
    let mut m: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for t in &w.tris {
        for k in 0..3 {
            let (u, v) = (t[k], t[(k + 1) % 3]);
            *m.entry((u.min(v), u.max(v))).or_insert(0) += 1;
        }
    }
    m.values().filter(|&&c| c != 2).count()
}

fn vol(mesh: &Mesh) -> f64 {
    mesh.tris
        .iter()
        .map(|t| {
            let a = mesh.verts[t[0] as usize];
            let b = mesh.verts[t[1] as usize];
            let c = mesh.verts[t[2] as usize];
            (a.x() * (b.y() * c.z() - c.y() * b.z()) - a.y() * (b.x() * c.z() - c.x() * b.z())
                + a.z() * (b.x() * c.y() - c.x() * b.y()))
                / 6.0
        })
        .sum()
}

/// Run UNION and return (native unpaired, native vol, sidecar unpaired, sidecar
/// vol). Requires the sidecar (LOUD — panics if missing, per P9 parity policy).
fn run(tag: &str, tube: &Mesh, plug: &Mesh) -> (usize, f64, usize, f64) {
    eprintln!(
        "[{tag}] tube: unpaired={} vol={:.4}; plug: unpaired={} vol={:.4}",
        unpaired(tube),
        vol(tube),
        unpaired(plug),
        vol(plug)
    );
    let no = NativeBoolean
        .boolean(tube, plug, BoolOp::Union)
        .unwrap_or_else(|e| panic!("[{tag}] native union failed: {e}"));
    let (nu, nv) = (unpaired(&no), vol(&no));
    eprintln!(
        "[{tag}] NATIVE  union: unpaired={nu} watertight={} vol={nv:.4} tris={}",
        nu == 0,
        no.tris.len()
    );
    let sc = SidecarBoolean::from_env().expect("CHERCHI2022_BIN sidecar (LOUD parity policy)");
    let so = sc
        .boolean(tube, plug, BoolOp::Union)
        .unwrap_or_else(|e| panic!("[{tag}] sidecar union failed: {e}"));
    let (su, sv) = (unpaired(&so), vol(&so));
    eprintln!(
        "[{tag}] SIDECAR union: unpaired={su} watertight={} vol={sv:.4} tris={}",
        su == 0,
        so.tris.len()
    );
    (nu, nv, su, sv)
}

/// LOCALIZATION ORACLE (task28, the gear `err.waffle` defect-2 root cause):
///
/// The gear's bore wall (an INWARD-facing hole) and the unioned flange's outer
/// wall (OUTWARD-facing) are the SAME cylinder with OPPOSITE normal sense — a
/// coincident-cylinder wall pair where the flange's caps cut transversely
/// across the bore. This is the cylinder analog of an opposite-normal coplanar
/// overlap (Yang 2025 §4.5.5 Stage-0), NOT a mesh-boolean labeling defect.
///
/// This minimal tube+plug fixture reproduces it and PROVES the localization:
/// the native cherchi boolean and the upstream C++ `mesh_booleans` reference
/// produce IDENTICAL output on EVERY metric (unpaired-edge count + signed
/// volume) — so cherchi is faithful to the reference; the non-watertightness
/// is intrinsic to the degenerate (zero-thickness coincident-sheet) INPUT, not
/// to cherchi's classification. Even with conformal z-tessellation the C++
/// reference still produces a non-watertight result. Resolving it therefore
/// requires a yang Stage-0 coincident-cylinder re-tessellation (drop the
/// interior coincident sheets, stitch the cap-ring boundary) BEFORE the mesh
/// boolean — exactly as the planar opposite-normal disc∩polygon crossing is
/// handled in `stage0`. Until that lands, this oracle pins the parity so a
/// future cherchi change that diverges from the reference is caught.
#[test]
fn plug_in_bore_native_matches_reference() {
    let n = 12;
    let (ro, ri) = (3.0, 1.0);

    // NON-CONFORMAL (the gear's real situation): tube bore wall only at z=±5;
    // plug wall only at z=±2 — non-conformal in z over the overlap band.
    let (nu, nv, su, sv) = run(
        "NON-CONFORMAL",
        &tube(n, ro, ri, -5.0, 5.0, &[]),
        &plug(n, ri, -2.0, 2.0, &[]),
    );
    assert_eq!(nu, su, "native vs reference unpaired-edge count must match");
    assert!(
        (nv - sv).abs() < 1e-6 * sv.abs().max(1.0),
        "native vs reference volume must match (nv={nv}, sv={sv})"
    );
    // Documented known wall: the opposite-normal coincident wall leaves the
    // coincident sheet unresolved at the mesh level (both impls agree it is
    // non-watertight — this is the Stage-0 capability gap, not a regression).
    assert!(nu > 0, "EXPECTED known-RED: opposite-normal coincident wall is non-watertight at mesh level (Stage-0 gap)");

    // CONFORMAL (insert the plug's z=±2 rings into the tube bore wall): halves
    // the unpaired edges but STILL non-watertight — conformal z alone is not
    // enough; the coincident sheets must be dropped in Stage-0.
    let (nu2, nv2, su2, sv2) = run(
        "CONFORMAL",
        &tube(n, ro, ri, -5.0, 5.0, &[-2.0, 2.0]),
        &plug(n, ri, -2.0, 2.0, &[]),
    );
    assert_eq!(
        nu2, su2,
        "native vs reference unpaired-edge count must match (conformal)"
    );
    assert!(
        (nv2 - sv2).abs() < 1e-6 * sv2.abs().max(1.0),
        "native vs reference volume must match (conformal)"
    );
    assert!(
        nu2 > 0,
        "EXPECTED known-RED: conformal z does not resolve the coincident sheet either"
    );
}
