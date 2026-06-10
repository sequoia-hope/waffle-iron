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
//! ## Sidecar requirement — LOUD
//!
//! This suite is the M6 gate; silently green-without-the-oracle is the worst
//! failure mode (P9). A missing `mesh_booleans` binary PANICS with an
//! actionable message instead of self-skipping. (PR-CR-M7c: the former
//! `require_ffi_shim` guard is gone from this suite — the native side is
//! pure Rust and needs no FFI shim; only the subprocess sidecar binary is
//! required.)

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use cad_primitives::{BoolOp, Point3};
use cherchi_rs::arrangements::fast_trimesh::VertexCoords;
use cherchi_rs::labeling::NativeBoolean;
use cherchi_rs::{mesh_arrangement, native_labeled_arrangement, InputId, Mesh, MeshBoolean};
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
const EXCLUDED_FIXTURES: &[(&str, &str)] = &[
    (
        "stacked-coplanar-cubes (any coplanar-overlap face pair)",
        "real coplanar overlap: the native arrangement defers LOUDLY with \
         ArrangementError::CoplanarPairDeferred (deviation N17) — Yang Stage-0 \
         (M8) owns coplanarity per Yang 2025 §4.5.5, and the native pipeline \
         has no counterpart to the C++ dupl_triangles restoration. The \
         deferral itself is asserted in \
         excluded_coplanar_fixture_defers_loudly below and in \
         labeling::native::tests::coplanar_overlap_is_loudly_deferred.",
    ),
    (
        "edge-exactly-in-face-plane (singleCoplanarEdge degeneracy)",
        "same N13/N17 deferral family, found by this suite's RED run: a \
         triangle EDGE lying exactly in the other solid's face plane (orBA \
         sign triple with two zeros) is loudly deferred as \
         CoplanarPairDeferred { reason: SingleCoplanarEdge }. The original \
         rotated-45-cube placement (center x = 2.0 = A's east face plane, so \
         the ±s∓s bottom-face diagonal corners cancel exactly to x = 2) hit \
         this; the corpus fixture now uses center x = 2.1 to stay generic-\
         position. When the C++ checkSingleCoplanarEdgeIntersections port \
         lands, promote a deliberate edge-in-plane fixture into the corpus.",
    ),
    (
        "tpi-x-crossing-shells (BOOLEAN cells only — arrangement cell IS in \
         the suite)",
        "PR-CR-M7c-tpi: TPI vertices are STRUCTURALLY IMPOSSIBLE for a binary \
         boolean of two watertight, 2-manifold, non-self-intersecting solids \
         (see tpi_xcrossing docs below), so any fixture that produces TPIs \
         end-to-end necessarily violates the solid-input contract of the \
         BOOLEAN labeling stage — empirically both backends emit junk that \
         diverges (native union vol 8.0 vs sidecar 10.38, sidecar output has \
         odd-multiplicity open edges). The ARRANGEMENT stage, by contrast, is \
         defined for arbitrary soups in both implementations, and that is \
         where TPIs live — so the fixture is covered by the arrangement-level \
         parity cell tpi_xcrossing_arrangement_parity below, never by the \
         4-op boolean matrix.",
    ),
];

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

/// TPI X-crossing fixture "B": ONE mesh containing TWO disjoint clean box
/// shells that overlap EACH OTHER (PR-CR-M7c-tpi).
///
/// Why this shape: a TPI vertex is the common point of THREE input-triangle
/// planes, lying inside all three (closed) triangles — it is born when two
/// constraint segments t∩t₁ and t∩t₂ CROSS in the interior of a base
/// triangle t. With only two inputs, two of {t, t₁, t₂} come from the same
/// input, and a point common to two triangles of one input means that input's
/// surface touches itself there (any other contact — shared edge or shared
/// vertex — makes the segments MEET at an existing LPI/explicit vertex, a
/// V/T-junction, never a crossing). So the only way to force TPIs through the
/// production binary-input pipeline is an input whose surface crosses itself
/// while staying combinatorially watertight and 2-manifold: two clean shells
/// in one mesh, interpenetrating.
///
/// Geometry (all coordinates dyadic, axis-aligned ⇒ the TPI coordinates are
/// exactly representable):
///   A  = cube(0,0,0,2)                       — top face plane z = 2
///   B1 = box x∈[0.5,1.25] y∈[0.25,1.125] z∈[1,3]
///   B2 = box x∈[0.375,1.875] y∈[0.625,1.5] z∈[0.875,3.125]
/// B1 ∩ B2 ≠ ∅ and both pierce A's top face, so the B1×B2 wall-crossing
/// lines pierce z = 2 inside A's top-face triangles. Exactly TWO geometric
/// TPI points result, at the triple-plane meets
///   (x=0.5)  ∩ (y=0.625) ∩ (z=2) = (0.5,  0.625, 2)   and
///   (x=1.25) ∩ (y=0.625) ∩ (z=2) = (1.25, 0.625, 2)
/// (the remaining face-plane triples all miss at least one closed face: e.g.
/// y=1.5 > B1's y-max 1.125, x=0.375 < B1's x-min 0.5). Every other axis
/// offset is distinct per axis, so there are no coplanar input pairs and no
/// edge-in-plane degeneracies, and neither point lies on A's top-face
/// diagonal y = x.
fn two_overlapping_shells() -> Mesh {
    let b1 = boxx(0.5, 0.25, 1.0, 0.75, 0.875, 2.0);
    let b2 = boxx(0.375, 0.625, 0.875, 1.5, 0.875, 2.25);
    let mut verts = b1.verts.clone();
    verts.extend(b2.verts.iter().copied());
    let off = b1.verts.len() as u32;
    let mut tris = b1.tris.clone();
    tris.extend(b2.tris.iter().map(|t| [t[0] + off, t[1] + off, t[2] + off]));
    Mesh::new(verts, tris)
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
        // 9. 45°-rotated cube vs axis-aligned cube. Generic position needs
        //    care on TWO axes: the z-offset (1.2) keeps the z-normal face
        //    planes apart, and the x-center must NOT equal A's face plane
        //    x ∈ {0, 2} — at center x = 2.0 the rotated cube's ±s∓s
        //    bottom/top-face diagonal corners cancel exactly to x = 2,
        //    putting a B edge exactly in A's east face plane (the deferred
        //    singleCoplanarEdge degeneracy; see EXCLUDED_FIXTURES).
        (
            "rotated-45-cube",
            cube(0.0, 0.0, 0.0, 2.0),
            rotated_cube_45z(2.1, 1.0, 1.2, 1.0),
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
                    if op == BoolOp::Xor {
                        "even"
                    } else {
                        "exactly 2"
                    }
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
        let mults =
            |m: &Mesh| -> BTreeSet<usize> { edge_stats(m).values().map(|&(c, _)| c).collect() };
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
    let (cn, cs) = (
        euler_characteristic(&native),
        euler_characteristic(&sidecar),
    );
    if cn != cs {
        return Err(format!("Euler characteristic: native {cn} vs sidecar {cs}"));
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

/// Loud sidecar handle: PANICS when the C++ binary is missing — a
/// silently-skipped reference oracle is a false GREEN (P9).
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

// ===========================================================================
// TPI coverage (PR-CR-M7c-tpi): arrangement-level parity on an X-crossing
// fixture + the structural no-TPI theorem over the boolean corpus
// ===========================================================================

/// Run the production arrangement on the concatenated two-input soup (the
/// exact label setup `native_labeled_arrangement` uses) and count vertex
/// kinds: (explicit, lpi, tpi).
fn arrangement_vertex_census(a: &Mesh, b: &Mesh) -> (usize, usize, usize) {
    let mut coords = Vec::with_capacity(3 * (a.verts.len() + b.verts.len()));
    for v in a.verts.iter().chain(b.verts.iter()) {
        coords.extend_from_slice(&[v.x(), v.y(), v.z()]);
    }
    let off = a.verts.len() as u32;
    let mut tris = a.tris.clone();
    tris.extend(b.tris.iter().map(|t| [t[0] + off, t[1] + off, t[2] + off]));
    let mut labels = vec![vec![InputId(0)]; a.tris.len()];
    labels.extend(std::iter::repeat_n(vec![InputId(1)], b.tris.len()));
    let soup = mesh_arrangement(&coords, &tris, &labels).expect("arrangement must succeed");
    let mut census = (0usize, 0usize, 0usize);
    for v in &soup.verts {
        match v {
            VertexCoords::Explicit(_) => census.0 += 1,
            VertexCoords::Lpi { .. } => census.1 += 1,
            VertexCoords::Tpi { .. } => census.2 += 1,
        }
    }
    census
}

/// Loud sidecar arrangement handle (same P9 posture as [`sidecar`]).
fn sidecar_arrangement(a: &Mesh, b: &Mesh) -> Mesh {
    match cherchi_sidecar_rs::labeled_arrangement(a, b, Duration::from_secs(60)) {
        Ok(la) => la.mesh,
        Err(SidecarError::BinaryNotFound { path }) => panic!(
            "reference-parity oracle unavailable: mesh_booleans binary not \
             found at {} (set CHERCHI2022_BIN or build per \
             docs/sidecar/cherchi2022_build_guide.md). Refusing to skip.",
            path.display()
        ),
        Err(e) => panic!("sidecar labeled_arrangement failed: {e}"),
    }
}

/// ARRANGEMENT-level reference parity on the TPI X-crossing fixture, plus the
/// fixture-sanity tooth: the native arrangement MUST construct TPI vertices
/// (this is the only end-to-end production-pipeline TPI coverage — the
/// boolean corpus is TPI-free by the structural theorem below, so without
/// this test the createTPI path would regress silently).
///
/// Why arrangement-level and not the 4-op boolean matrix: see the
/// `tpi-x-crossing-shells` entry in [`EXCLUDED_FIXTURES`]. The arrangement is
/// defined for arbitrary (even self-crossing) soups in BOTH implementations,
/// so the triangulation-independent metrics apply verbatim; the boolean
/// labeling is not (winding-2 regions), and the backends demonstrably
/// diverge there.
///
/// Metrics (all triangulation-independent, no tolerance widening):
///   * vertex-set Hausdorff-0 at `VERT_TOL`, both directions — this is the
///     one that validates the native TPI COORDINATES against the C++
///     indirect-predicate TPIs end to end;
///   * the two analytically-known TPI points appear EXACTLY (they are
///     dyadic: axis-aligned plane triples) in the native output;
///   * total surface area at `REL_TOL` (bbox floor);
///   * Euler characteristic after exact weld;
///   * every welded edge has EVEN, direction-balanced multiplicity (both
///     input surfaces are closed and outward), and the per-multiplicity
///     histogram of edges with multiplicity ≥ 4 (the intersection-curve
///     sub-edges — a property of the complex, not the triangulation)
///     matches.
#[test]
fn tpi_xcrossing_arrangement_parity() {
    let a = cube(0.0, 0.0, 0.0, 2.0);
    let b = two_overlapping_shells();

    // ----- fixture-sanity tooth: the native arrangement constructs TPIs ----
    let (_, lpi, tpi) = arrangement_vertex_census(&a, &b);
    assert!(
        tpi >= 1,
        "TPI coverage lost: the X-crossing fixture no longer produces any \
         VertexCoords::Tpi in the native arrangement (lpi count {lpi})"
    );
    // Deterministic snapshot: 2 geometric TPI points, each constructed once
    // per base triangle that hosts the X-crossing (A-top, B1-wall, B2-wall →
    // 3 structurally distinct generator triples per point; the §7 global weld
    // interns by STRUCTURAL equality, so all 3 survive as soup vertices).
    // If a future slice normalizes TPI generator triples (deduping the three
    // representations to one), update this to 2 — but `tpi >= 1` above is the
    // load-bearing assertion.
    assert_eq!(
        tpi, 6,
        "TPI census changed — re-derive the fixture geometry before updating"
    );

    // ----- arrangement-level reference parity ------------------------------
    let native = weld(&native_labeled_arrangement(&a, &b).expect("native").mesh);
    let sidecar = weld(&sidecar_arrangement(&a, &b));

    // The two analytically-known TPI points, exact in the native output
    // (dyadic coordinates survive scale-up/descale and RBig→f64 rounding
    // exactly), within VERT_TOL in the sidecar's (OBJ-roundtripped) output.
    for tgt in [p(0.5, 0.625, 2.0), p(1.25, 0.625, 2.0)] {
        assert!(
            native.verts.contains(&tgt),
            "native arrangement is missing the exact TPI vertex {tgt:?}"
        );
        let near_tgt = |v: &Point3| {
            (v.x() - tgt.x()).powi(2) + (v.y() - tgt.y()).powi(2) + (v.z() - tgt.z()).powi(2)
                <= VERT_TOL * VERT_TOL
        };
        assert!(
            sidecar.verts.iter().any(near_tgt),
            "sidecar arrangement is missing the TPI vertex {tgt:?}"
        );
    }

    // Vertex-set Hausdorff-0, both directions.
    if let Some(v) = vertex_cover_gap(&native, &sidecar, VERT_TOL) {
        panic!("native arrangement vertex {v:?} has no sidecar vertex within {VERT_TOL}");
    }
    if let Some(v) = vertex_cover_gap(&sidecar, &native, VERT_TOL) {
        panic!("sidecar arrangement vertex {v:?} has no native vertex within {VERT_TOL}");
    }

    // Surface area parity.
    let (_, area_scale) = bbox_scales(&a, &b);
    let (an, as_) = (surface_area(&native), surface_area(&sidecar));
    let atol = REL_TOL * as_.abs().max(area_scale);
    assert!(
        (an - as_).abs() <= atol,
        "arrangement surface area: native {an:.15} vs sidecar {as_:.15} (tol {atol:.3e})"
    );

    // Euler characteristic (invariant under re-triangulation of the complex).
    assert_eq!(
        euler_characteristic(&native),
        euler_characteristic(&sidecar),
        "arrangement Euler characteristic mismatch"
    );

    // Edge structure: even + balanced everywhere on both sides; the ≥4
    // multiplicity histogram (intersection-curve sub-edges) matches.
    let mult_hist = |m: &Mesh, tag: &str| -> BTreeMap<usize, usize> {
        let mut hist: BTreeMap<usize, usize> = BTreeMap::new();
        for (edge, (count, balance)) in edge_stats(m) {
            assert!(
                count % 2 == 0,
                "{tag}: arrangement edge {edge:?} has ODD multiplicity {count}"
            );
            assert_eq!(
                balance, 0,
                "{tag}: arrangement edge {edge:?} direction-unbalanced"
            );
            if count >= 4 {
                *hist.entry(count).or_insert(0) += 1;
            }
        }
        hist
    };
    assert_eq!(
        mult_hist(&native, "native"),
        mult_hist(&sidecar, "sidecar"),
        "intersection-curve edge-multiplicity histograms differ"
    );
}

/// The structural no-TPI theorem, pinned as an invariant: every boolean
/// corpus fixture (two watertight, 2-manifold, non-self-intersecting solids)
/// produces an arrangement with ZERO TPI vertices.
///
/// Sketch: a TPI lies inside three closed input triangles t, t₁, t₂ whose
/// constraint segments t∩t₁ and t∩t₂ CROSS in t's interior. Two of the three
/// triangles share an input; a common point of two same-input triangles
/// means that input self-touches there, unless the triangles are adjacent —
/// and for adjacent triangles the common point sits on the shared edge (an
/// LPI of that edge with t's plane, where the two segments MEET end-to-end
/// as a V-junction, not a crossing: two distinct coplanar lines meet only
/// once, at that very point). Hence clean binary inputs ⇒ no TPIs.
///
/// If this test ever fails, a corpus change introduced TPIs from clean
/// inputs — that contradicts the theorem, so first suspect a fixture that is
/// secretly self-intersecting (or an arrangement bug), and if it is neither,
/// PROMOTE the fixture: it would be the better TPI-coverage cell.
#[test]
fn corpus_arrangements_are_tpi_free() {
    for (name, a, b) in corpus() {
        let (_, _, tpi) = arrangement_vertex_census(&a, &b);
        assert_eq!(
            tpi, 0,
            "[{name}] clean binary fixture produced {tpi} TPI vertices — \
             see the structural theorem in this test's docs"
        );
    }
}
