//! PR-CR-BL3b — M6 milestone gate: REFERENCE PARITY of the native cherchi-rs
//! boolean (`NativeBoolean`) against the upstream C++ `mesh_booleans` binary
//! (Cherchi 2022), wrapped by `cherchi_sidecar_rs::SidecarBoolean`.
//!
//! Per crate CLAUDE.md hard rule #2 and roadmap §6, reference parity IS the
//! correctness oracle: GREEN ::= the native port matches the sidecar on this
//! corpus. Internal stage oracles cannot detect a port that diverges from the
//! reference upstream of the oracle's check.
//!
//! ## Parity metric (triangulation-independent)
//!
//! The two implementations may triangulate the same arrangement differently
//! (and the C++ may use TBB internally), so the diff never compares triangle
//! lists. For each (fixture, op) cell, after an EXACT-coordinate vertex weld
//! on both outputs:
//!
//! 1. **Watertight 2-manifold** both: every undirected edge has exactly 2
//!    incident triangles, one per direction. Exception: `Xor` legitimately
//!    emits two shells sharing the intersection-curve edges (4 incident tris
//!    each, by Cherchi's `boolXOR` construction) — there we assert every edge
//!    has EVEN, direction-balanced multiplicity and that the SET of distinct
//!    edge multiplicities matches between native and sidecar. (The full
//!    multiset of multiplicities is triangulation-DEPENDENT — how many 2-tri
//!    vs 4-tri edges exist depends on how each side subdivides faces and the
//!    intersection curve — so the distinct-set + evenness + balance is the
//!    triangulation-invariant content.)
//! 2. **Signed volume** (divergence theorem) equal within 1e-9 RELATIVE,
//!    scale-floored by the combined-input bbox volume for near-zero results.
//! 3. **Surface area** equal within 1e-9 relative (same flooring, bbox area).
//! 4. **Euler characteristic** (V − E + F after the exact weld) equal — χ is
//!    invariant under re-triangulation of the same underlying complex.
//! 5. **Vertex-set match**: every native vertex has a sidecar vertex within
//!    1e-6 absolute (post-descale coordinates) and vice versa (Hausdorff-0 on
//!    vertex sets). Justified: Cherchi arrangements introduce NO points
//!    beyond the input vertices and the pairwise intersection points (LPI /
//!    TPI), and the constrained re-triangulation adds no Steiner points — so
//!    two correct implementations of the same arrangement must agree on the
//!    vertex SET even when they disagree on the triangulation. The 1e-6 slack
//!    only absorbs the lazy-exact → f64 emission rounding on each side.
//!
//! NO tolerance widening to make a cell pass (P9/P10): a failing cell is a
//! native bug, a harness bug, or a documented semantic difference — never a
//! looser threshold.
//!
//! ## Corpus
//!
//! 12 deterministic in-test fixtures × all 4 ops, all in generic position
//! (no coplanar input face pairs — see [`EXCLUDED_FIXTURES`]). Plus
//! presentation-invariance spot-checks: swapping the (a, b) argument order on
//! two fixtures must still match the sidecar called with the same swapped
//! order (and symmetric ops must match the unswapped native result under the
//! same metric). Note `Subtract(A,B)` and `Subtract(B,A)` differ by
//! definition — the swap checks compare like with like.
//!
//! ## Sidecar requirement — LOUD, per `require_ffi_shim` style
//!
//! This suite is the M6 gate; silently green-without-the-oracle is the worst
//! failure mode (P9). A missing `mesh_booleans` binary PANICS with an
//! actionable message instead of self-skipping (strictest existing pattern:
//! `arrangements::require_ffi_shim`).

#![cfg(feature = "indirect-predicates")]

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::{BoolOp, Point3};
use cherchi_rs::labeling::NativeBoolean;
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::{SidecarBoolean, SidecarError};

const ALL_OPS: [BoolOp; 4] = [
    BoolOp::Union,
    BoolOp::Intersect,
    BoolOp::Subtract,
    BoolOp::Xor,
];

/// Relative tolerance for volume / area parity.
const REL_TOL: f64 = 1e-9;
/// Absolute tolerance for the vertex-set Hausdorff-0 match.
const VERT_TOL: f64 = 1e-6;

/// Fixture families EXCLUDED from the corpus — loud deferrals, never silent
/// skips. Each entry is (name, reason).
const EXCLUDED_FIXTURES: &[(&str, &str)] = &[(
    "stacked-coplanar-cubes (any coplanar-overlap face pair)",
    "real coplanar overlap: the native arrangement defers LOUDLY with \
     ArrangementError::CoplanarPairDeferred (deviation N17) — Yang Stage-0 \
     (M8) owns coplanarity per Yang 2025 §4.5.5, and the native pipeline has \
     no counterpart to the C++ dupl_triangles restoration. The deferral \
     itself is asserted in excluded_coplanar_fixture_defers_loudly below and \
     in labeling::native::tests::coplanar_overlap_is_loudly_deferred.",
)];

// ===========================================================================
// Fixture builders (deterministic, generic position — no coplanar pairs)
// ===========================================================================

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Standard 12-tri box topology over 8 corners in the unit-cube corner order
/// (0,0,0),(1,0,0),(1,1,0),(0,1,0),(0,0,1),(1,0,1),(1,1,1),(0,1,1) — outward
/// winding (same fixture winding as the BL2/BL3a suites).
fn hexahedron(verts: Vec<Point3>) -> Mesh {
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

/// Axis-aligned box with origin corner + per-axis sizes.
fn boxx(ox: f64, oy: f64, oz: f64, sx: f64, sy: f64, sz: f64) -> Mesh {
    let c = |x: f64, y: f64, z: f64| p(ox + x * sx, oy + y * sy, oz + z * sz);
    hexahedron(vec![
        c(0.0, 0.0, 0.0),
        c(1.0, 0.0, 0.0),
        c(1.0, 1.0, 0.0),
        c(0.0, 1.0, 0.0),
        c(0.0, 0.0, 1.0),
        c(1.0, 0.0, 1.0),
        c(1.0, 1.0, 1.0),
        c(0.0, 1.0, 1.0),
    ])
}

fn cube(ox: f64, oy: f64, oz: f64, s: f64) -> Mesh {
    boxx(ox, oy, oz, s, s, s)
}

/// Cube of half-extent `h`, centered at `c`, rotated 45° about the z axis.
/// Same corner order / winding as `hexahedron` (rotation preserves it).
fn rotated_cube_45z(cx: f64, cy: f64, cz: f64, h: f64) -> Mesh {
    let s = h * std::f64::consts::FRAC_1_SQRT_2; // h·cos45 = h·sin45
    let u = [s, s, 0.0]; // rotated +x half-axis
    let v = [-s, s, 0.0]; // rotated +y half-axis
    let w = [0.0, 0.0, h]; // +z half-axis
    let corner = |a: f64, b: f64, c: f64| {
        p(
            cx + a * u[0] + b * v[0] + c * w[0],
            cy + a * u[1] + b * v[1] + c * w[1],
            cz + a * u[2] + b * v[2] + c * w[2],
        )
    };
    hexahedron(vec![
        corner(-1.0, -1.0, -1.0),
        corner(1.0, -1.0, -1.0),
        corner(1.0, 1.0, -1.0),
        corner(-1.0, 1.0, -1.0),
        corner(-1.0, -1.0, 1.0),
        corner(1.0, -1.0, 1.0),
        corner(1.0, 1.0, 1.0),
        corner(-1.0, 1.0, 1.0),
    ])
}

/// Tetrahedron over 4 vertices with combinatorially consistent faces, then a
/// global flip if the signed volume is negative — guarantees outward winding
/// for any non-degenerate vertex order.
fn tetra(a: Point3, b: Point3, c: Point3, d: Point3) -> Mesh {
    let m = Mesh::new(
        vec![a, b, c, d],
        vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    );
    oriented(m)
}

/// Octahedron: 6 axis vertices at distance `r` from center, 8 outward faces.
fn octahedron(cx: f64, cy: f64, cz: f64, r: f64) -> Mesh {
    let verts = vec![
        p(cx + r, cy, cz), // 0: +x
        p(cx - r, cy, cz), // 1: -x
        p(cx, cy + r, cz), // 2: +y
        p(cx, cy - r, cz), // 3: -y
        p(cx, cy, cz + r), // 4: +z
        p(cx, cy, cz - r), // 5: -z
    ];
    let tris = vec![
        [0, 2, 4],
        [2, 1, 4],
        [1, 3, 4],
        [3, 0, 4],
        [2, 0, 5],
        [1, 2, 5],
        [3, 1, 5],
        [0, 3, 5],
    ];
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

/// The 12-fixture corpus: (name, A, B). All generic-position (no coplanar
/// input face pairs, no point-touch contacts).
fn corpus() -> Vec<(&'static str, Mesh, Mesh)> {
    vec![
        // 1. Corner-overlap cubes (overlap volume 1).
        (
            "corner-overlap-cubes",
            cube(0.0, 0.0, 0.0, 2.0),
            cube(1.0, 1.0, 1.0, 2.0),
        ),
        // 2. Identical-size cubes, representable offsets on all 3 axes
        //    (distinct per axis so no face planes coincide).
        (
            "identical-size-offset-cubes",
            cube(0.0, 0.0, 0.0, 2.0),
            cube(1.0, 0.5, 0.25, 2.0),
        ),
        // 3. Enclosed cube: B strictly inside A (offsets break coplanarity).
        (
            "enclosed-cube",
            cube(0.0, 0.0, 0.0, 3.0),
            cube(1.0, 1.1, 0.9, 1.0),
        ),
        // 4. Disjoint cubes (intersection is legitimately EMPTY).
        (
            "disjoint-cubes",
            cube(0.0, 0.0, 0.0, 1.0),
            cube(2.5, 0.3, 0.2, 1.0),
        ),
        // 5. Through-cut peg along z (genus-1 subtraction).
        (
            "through-cut-peg-z",
            cube(0.0, 0.0, 0.0, 2.0),
            boxx(0.5, 0.5, -1.0, 1.0, 1.0, 4.0),
        ),
        // 6. Through-cut peg along x.
        (
            "through-cut-peg-x",
            cube(0.0, 0.0, 0.0, 2.0),
            boxx(-1.0, 0.5, 0.5, 4.0, 1.0, 1.0),
        ),
        // 7. Two genuinely interpenetrating tetrahedra (NOT the point-touch
        //    pair): identical shape translated by (0.3, 0.25, 0.2), so every
        //    face pair is parallel-offset (never coplanar) and B's near
        //    vertex is deep inside A.
        (
            "interpenetrating-tetrahedra",
            tetra(
                p(0.0, 0.0, 0.0),
                p(2.0, 0.0, 0.0),
                p(0.0, 2.0, 0.0),
                p(0.0, 0.0, 2.0),
            ),
            tetra(
                p(0.3, 0.25, 0.2),
                p(2.3, 0.25, 0.2),
                p(0.3, 2.25, 0.2),
                p(0.3, 0.25, 2.2),
            ),
        ),
        // 8. Cube ∩ tetra: one tetra vertex inside the cube, three poking
        //    out of three different faces.
        (
            "cube-tetra",
            cube(0.0, 0.0, 0.0, 2.0),
            tetra(
                p(1.0, 1.0, 1.0),
                p(3.0, 1.1, 1.2),
                p(1.1, 3.0, 1.3),
                p(1.2, 1.1, 3.0),
            ),
        ),
        // 9. 45°-rotated cube vs axis-aligned cube (z-offset so the
        //    z-normal face planes don't coincide).
        (
            "rotated-45-cube",
            cube(0.0, 0.0, 0.0, 2.0),
            rotated_cube_45z(2.0, 1.0, 1.2, 1.0),
        ),
        // 10. Octahedron vs cube: octa centered in the cube, all 6 apexes
        //     poking out of the 6 faces.
        (
            "octahedron-cube",
            cube(0.0, 0.0, 0.0, 2.0),
            octahedron(1.0, 1.0, 1.0, 1.2),
        ),
        // 11. Sliver overlap: boxes overlapping by 0.01 along x.
        (
            "sliver-overlap-boxes",
            cube(0.0, 0.0, 0.0, 1.0),
            boxx(0.99, 0.1, 0.15, 1.0, 0.7, 0.6),
        ),
        // 12. Non-representable offsets (0.3 / 0.7 / 0.9 have no exact
        //     binary representation — exercises the scale-up multiplier).
        (
            "non-representable-offset-boxes",
            cube(0.0, 0.0, 0.0, 2.0),
            cube(0.3, 0.7, 0.9, 2.0),
        ),
    ]
}

// ===========================================================================
// Metric helpers
// ===========================================================================

/// Exact-coordinate vertex weld: vertices with bit-identical (x, y, z) are
/// merged; triangles are re-indexed. No tolerance — both sides emit f64
/// coordinates, and the weld only removes index-level duplication.
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

/// Per undirected edge: (incident-triangle count, directed balance). Balance
/// 0 ⟺ each direction used equally often (orientation-consistent).
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

/// Signed volume by the divergence theorem (sum of signed origin-tetra
/// volumes). Positive ⟺ outward-consistent closed surface.
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

/// Total surface area (sum of triangle areas).
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

/// Euler characteristic V − E + F over a (welded) mesh.
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

/// Every vertex of `from` has a vertex of `to` within `tol` (one direction
/// of the Hausdorff-0 vertex-set check). Returns the first offender.
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

/// Combined-input bbox: scale floors for the relative tolerances on
/// near-zero results (volume floor = bbox volume, area floor = bbox area).
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

// ===========================================================================
// The parity comparison
// ===========================================================================

/// Compare one (fixture, op) cell: native vs sidecar output under the
/// triangulation-independent metric. Returns Err(diagnostic) on the first
/// failing metric.
fn compare_cell(
    op: BoolOp,
    native_raw: &Mesh,
    sidecar_raw: &Mesh,
    vol_scale: f64,
    area_scale: f64,
) -> Result<(), String> {
    let native = weld(native_raw);
    let sidecar = weld(sidecar_raw);

    // Legitimately-empty results (e.g. disjoint ∩) must be empty on BOTH
    // sides; one-sided emptiness is a parity failure.
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

    // (1) Manifoldness. Xor legitimately shares intersection-curve edges
    // between its two shells (4 incident tris); see module docs.
    for (name, mesh) in [("native", &native), ("sidecar", &sidecar)] {
        for (edge, (count, balance)) in edge_stats(mesh) {
            let count_ok = match op {
                BoolOp::Xor => count % 2 == 0,
                _ => count == 2,
            };
            if !count_ok {
                return Err(format!(
                    "{name}: edge {edge:?} has {count} incident tris \
                     (expected {} for {op:?})",
                    if op == BoolOp::Xor { "even" } else { "exactly 2" }
                ));
            }
            if balance != 0 {
                return Err(format!(
                    "{name}: edge {edge:?} direction-unbalanced (balance {balance})"
                ));
            }
        }
    }
    if op == BoolOp::Xor {
        let mults = |m: &Mesh| -> BTreeSet<usize> {
            edge_stats(m).values().map(|&(c, _)| c).collect()
        };
        let (mn, ms) = (mults(&native), mults(&sidecar));
        if mn != ms {
            return Err(format!(
                "xor edge-multiplicity sets differ: native {mn:?}, sidecar {ms:?}"
            ));
        }
    }

    // (2) Signed volume, 1e-9 relative (bbox-volume floor).
    let (vn, vs) = (signed_volume(&native), signed_volume(&sidecar));
    let vtol = REL_TOL * vs.abs().max(vol_scale);
    if (vn - vs).abs() > vtol {
        return Err(format!(
            "signed volume: native {vn:.15} vs sidecar {vs:.15} (tol {vtol:.3e})"
        ));
    }

    // (3) Surface area, 1e-9 relative (bbox-area floor).
    let (an, as_) = (surface_area(&native), surface_area(&sidecar));
    let atol = REL_TOL * as_.abs().max(area_scale);
    if (an - as_).abs() > atol {
        return Err(format!(
            "surface area: native {an:.15} vs sidecar {as_:.15} (tol {atol:.3e})"
        ));
    }

    // (4) Euler characteristic after exact weld.
    let (cn, cs) = (euler_characteristic(&native), euler_characteristic(&sidecar));
    if cn != cs {
        return Err(format!(
            "Euler characteristic: native {cn} vs sidecar {cs}"
        ));
    }

    // (5) Vertex-set Hausdorff-0 (both directions, 1e-6 absolute).
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

/// Loud sidecar handle: PANICS (require_ffi_shim style) when the C++ binary
/// is missing — a silently-skipped reference oracle is a false GREEN (P9).
fn sidecar() -> SidecarBoolean {
    match SidecarBoolean::from_env() {
        Ok(sb) => sb,
        Err(SidecarError::BinaryNotFound { path }) => panic!(
            "reference-parity oracle unavailable: mesh_booleans binary not \
             found at {} (set CHERCHI2022_BIN or build per \
             docs/sidecar/cherchi2022_build_guide.md). Refusing to skip — \
             this suite IS the M6 correctness gate.",
            path.display()
        ),
        Err(e) => panic!("sidecar setup failed: {e}"),
    }
}

/// Run all 4 ops for one fixture, accumulating per-cell failures so a single
/// run reports the full row of the parity matrix.
fn run_fixture(name: &str, a: &Mesh, b: &Mesh) {
    cherchi_rs::arrangements::require_ffi_shim();
    let sb = sidecar();
    let (vol_scale, area_scale) = bbox_scales(a, b);
    let mut failures: Vec<String> = Vec::new();
    for op in ALL_OPS {
        let native = match NativeBoolean.boolean(a, b, op) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("[{name} × {op:?}] native boolean failed: {e}"));
                continue;
            }
        };
        let reference = match sb.boolean(a, b, op) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("[{name} × {op:?}] SIDECAR failed (harness): {e}"));
                continue;
            }
        };
        if let Err(msg) = compare_cell(op, &native, &reference, vol_scale, area_scale) {
            failures.push(format!("[{name} × {op:?}] {msg}"));
        }
    }
    assert!(
        failures.is_empty(),
        "reference-parity failures ({}/4 cells):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

// ===========================================================================
// The corpus, one test per fixture (parallel; full matrix per run)
// ===========================================================================

macro_rules! fixture_test {
    ($test_name:ident, $idx:expr) => {
        #[test]
        fn $test_name() {
            let corpus = corpus();
            let (name, a, b) = &corpus[$idx];
            run_fixture(name, a, b);
        }
    };
}

fixture_test!(parity_corner_overlap_cubes, 0);
fixture_test!(parity_identical_size_offset_cubes, 1);
fixture_test!(parity_enclosed_cube, 2);
fixture_test!(parity_disjoint_cubes, 3);
fixture_test!(parity_through_cut_peg_z, 4);
fixture_test!(parity_through_cut_peg_x, 5);
fixture_test!(parity_interpenetrating_tetrahedra, 6);
fixture_test!(parity_cube_tetra, 7);
fixture_test!(parity_rotated_45_cube, 8);
fixture_test!(parity_octahedron_cube, 9);
fixture_test!(parity_sliver_overlap_boxes, 10);
fixture_test!(parity_non_representable_offset_boxes, 11);

// ===========================================================================
// Presentation invariance: concat-swap on two fixtures
// ===========================================================================

/// Swapped argument order must (a) still match the sidecar called with the
/// SAME swapped order on every op, and (b) for the symmetric ops
/// (Union/Intersect/Xor) match the unswapped native result under the
/// triangulation-independent metric. Subtract(B,A) ≠ Subtract(A,B) by
/// definition — it is covered by check (a) only.
fn run_swap_invariance(name: &str, a: &Mesh, b: &Mesh) {
    cherchi_rs::arrangements::require_ffi_shim();
    let sb = sidecar();
    let (vol_scale, area_scale) = bbox_scales(a, b);
    let mut failures: Vec<String> = Vec::new();
    for op in ALL_OPS {
        let swapped = match NativeBoolean.boolean(b, a, op) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("[{name}-swapped × {op:?}] native failed: {e}"));
                continue;
            }
        };
        // (a) parity against the sidecar with the same swapped order.
        match sb.boolean(b, a, op) {
            Ok(reference) => {
                if let Err(msg) = compare_cell(op, &swapped, &reference, vol_scale, area_scale) {
                    failures.push(format!("[{name}-swapped × {op:?}] vs sidecar: {msg}"));
                }
            }
            Err(e) => {
                failures.push(format!(
                    "[{name}-swapped × {op:?}] SIDECAR failed (harness): {e}"
                ));
            }
        }
        // (b) symmetric ops: swapped native ≡ unswapped native (same metric).
        if op != BoolOp::Subtract {
            match NativeBoolean.boolean(a, b, op) {
                Ok(unswapped) => {
                    if let Err(msg) = compare_cell(op, &swapped, &unswapped, vol_scale, area_scale)
                    {
                        failures.push(format!(
                            "[{name}-swapped × {op:?}] vs unswapped native: {msg}"
                        ));
                    }
                }
                Err(e) => {
                    failures.push(format!("[{name} × {op:?}] native failed: {e}"));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "swap-invariance failures:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn parity_swap_corner_overlap_cubes() {
    let corpus = corpus();
    let (name, a, b) = &corpus[0];
    run_swap_invariance(name, a, b);
}

#[test]
fn parity_swap_interpenetrating_tetrahedra() {
    let corpus = corpus();
    let (name, a, b) = &corpus[6];
    run_swap_invariance(name, a, b);
}

// ===========================================================================
// Excluded-fixture loudness: the exclusion list is enforced, not aspirational
// ===========================================================================

/// The EXCLUDED coplanar-overlap family must keep deferring LOUDLY — if the
/// native pipeline ever starts accepting it, this test fails and the fixture
/// must be PROMOTED into the corpus (with sidecar dupl_triangles parity
/// reconciled), never left silently un-tested.
#[test]
fn excluded_coplanar_fixture_defers_loudly() {
    cherchi_rs::arrangements::require_ffi_shim();
    assert!(
        !EXCLUDED_FIXTURES.is_empty(),
        "exclusion list documents the deferred families"
    );
    let a = cube(0.0, 0.0, 0.0, 2.0);
    let b = cube(1.0, 1.0, 2.0, 2.0); // bottom face overlaps A's top (z = 2)
    let err = NativeBoolean
        .boolean(&a, &b, BoolOp::Union)
        .expect_err("coplanar overlap must defer loudly, not produce a mesh");
    let msg = err.to_string();
    assert!(
        msg.contains("Coplanar"),
        "expected a CoplanarPairDeferred arrangement error, got: {msg}"
    );
}
