#[allow(unused_imports)]
use super::*;

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
pub(crate) struct LabelMockBackend {
    arrangement: LabeledArrangement,
}
impl LabelMockBackend {
    pub(crate) fn new(arrangement: LabeledArrangement) -> Self {
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
pub(crate) fn cube_brep(origin: [f64; 3]) -> BRep {
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
pub(crate) fn brep_new_populates_tri_face_provenance() {
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
pub(crate) fn centroid(mesh: &Mesh, tri: [u32; 3]) -> Point3 {
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
pub(crate) fn resolve_face(brep: &BRep, c: Point3) -> u32 {
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
pub(crate) fn arrangement_a_cube_shell() -> LabeledArrangement {
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
        intersection_edges: Default::default(),
        num_inputs: 2,
    }
}

#[test]
pub(crate) fn m3_union_full_attribution_coverage() {
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
pub(crate) fn m3_union_attribution_matches_geometric_face() {
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
pub(crate) fn m3_kept_submesh_is_keep_set_count() {
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
pub(crate) fn m3_coplanar_surface_len_two_errors_f2() {
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
        intersection_edges: Default::default(),
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
pub(crate) fn m3_centroid_off_all_planes_errors_f3() {
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
        intersection_edges: Default::default(),
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
pub(crate) fn n4_provenance_miss_errors_loudly() {
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
        intersection_edges: Default::default(),
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
pub(crate) fn n4_provenance_out_of_range_parent_errors_loudly() {
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
        intersection_edges: Default::default(),
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
pub(crate) fn m4_real_label_and_substitute_agree_on_pure_a() {
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
pub(crate) fn rt_cylinder(
    base: f64,
    h: f64,
    r: f64,
) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
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
pub(crate) fn rim_override_empty_is_byte_identical() {
    let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
    let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
    let empty: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
    let overridden =
        stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &empty, None).expect("empty");
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
pub(crate) fn rim_override_inserts_into_both_rims_no_t_junction() {
    let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
    // A point on each rim at azimuth 0.3 rad (NOT a uniform sample): radius
    // 0.5 in the rim's plane.
    let az = 0.3_f64;
    let (s, c) = az.sin_cos();
    let bottom_pt = Point3::new(0.5 * c, 0.5 * s, 0.0);
    let top_pt = Point3::new(0.5 * c, 0.5 * s, 1.0);
    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
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
    let mut counts: std::collections::BTreeMap<(u32, u32), u32> = std::collections::BTreeMap::new();
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

// ====================================================================
// Task #143 (spec `m8_rim_override_uniform_merge`): an override that
// coincides with a uniform rim sample within the fused-emission identity
// (< TAU_MODEL) MERGES deliberately — the uniform slot takes the
// override's exact bits, ring length unchanged, no azimuth-merge routing.
// Real-scale coincidence, seam/endpoint collisions, and same-slot
// conflicts stay loud (fail closed).
// ====================================================================

/// Rotate a point about the +z axis by a tiny angle — the ULP-twin
/// generator (a fused survivor from the OTHER body's mirrored rim sits a
/// few ULPs off this rim's own uniform sample).
pub(crate) fn rot_z(p: Point3, delta: f64) -> Point3 {
    let a = p.as_array();
    let (s, c) = delta.sin_cos();
    Point3::new(a[0] * c - a[1] * s, a[0] * s + a[1] * c, a[2])
}

fn closed_2_manifold(tris: &[[u32; 3]]) -> bool {
    let mut counts: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in tris {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    !counts.is_empty() && counts.values().all(|&c| c == 2)
}

/// Spec row 2 (I1+I2): a ULP-twin override on an interior uniform slot
/// merges — ring length unchanged, the ring vertex takes the override's
/// exact bits, the displaced uniform bits vanish, the rim is NOT routed to
/// azimuth-merge, and the cylinder stays a closed 2-manifold.
#[test]
pub(crate) fn rim_override_ulp_twin_merges_onto_uniform_sample() {
    let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
    let empty: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    let (plain, _) = stage1_tessellate_inner(&verts, &edges, &faces, &empty, None).expect("plain");
    let ring = plain.chains[&0].clone();
    let n = ring.len();
    let up = plain.verts[ring[2] as usize];
    let twin = rot_z(up, 1e-15);
    assert_ne!(
        twin.as_array().map(f64::to_bits),
        up.as_array().map(f64::to_bits),
        "fixture: twin must differ in bits"
    );

    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov.insert(0, vec![twin]);
    let (t, inserted) = stage1_tessellate_inner(&verts, &edges, &faces, &ov, None)
        .expect("ULP-twin override must merge, not refuse");

    let ring2 = &t.chains[&0];
    assert_eq!(ring2.len(), n, "merge must not change ring length (I1)");
    let bits = |p: &Point3| p.as_array().map(f64::to_bits);
    assert!(
        t.verts.iter().any(|q| bits(q) == bits(&twin)),
        "merged ring must carry the override's exact bits (I2)"
    );
    assert!(
        !t.verts.iter().any(|q| bits(q) == bits(&up)),
        "displaced uniform sample bits must be gone (survivor is the shared point)"
    );
    assert!(
        !inserted.contains(&0),
        "a pure merge must NOT route the rim to azimuth-merge (I1)"
    );
    assert!(
        closed_2_manifold(&t.tris),
        "merged cylinder must stay a closed 2-manifold"
    );
}

/// Spec row 2 degenerate + I3: an override bit-exactly EQUAL to the
/// computed uniform sample merges as a no-op — verts AND tris are
/// byte-identical to the un-overridden tessellation.
#[test]
pub(crate) fn rim_override_bit_exact_uniform_merge_is_byte_identical() {
    let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
    let empty: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    let (plain, _) = stage1_tessellate_inner(&verts, &edges, &faces, &empty, None).expect("plain");
    let up = plain.verts[plain.chains[&0][2] as usize];

    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov.insert(0, vec![up]);
    let (t, inserted) = stage1_tessellate_inner(&verts, &edges, &faces, &ov, None)
        .expect("bit-exact override must merge");
    assert_eq!(plain.verts.len(), t.verts.len());
    for (a, b) in plain.verts.iter().zip(&t.verts) {
        assert_eq!(
            a.as_array(),
            b.as_array(),
            "verts must be byte-identical (I3)"
        );
    }
    assert_eq!(plain.tris, t.tris, "tris must be byte-identical (I3)");
    assert!(!inserted.contains(&0));
}

/// Spec row 3 (I4, fail closed): an override angularly inside the
/// coincidence band but ≥ TAU_MODEL away in 3D (a REAL-scale distinct
/// crossing grazing a uniform sample) stays the loud typed wall.
#[test]
pub(crate) fn rim_override_real_scale_uniform_coincidence_stays_loud() {
    let r = 100.0;
    let (verts, edges, faces) = rt_cylinder(0.0, 1.0, r);
    let empty: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    let (plain, _) = stage1_tessellate_inner(&verts, &edges, &faces, &empty, None).expect("plain");
    let ring = plain.chains[&0].clone();
    let n = ring.len();
    let uni_step = 2.0 * std::f64::consts::PI / (n as f64);
    let delta = 0.9 * (uni_step * 1.0e-6); // inside the angular trigger band
    assert!(
        r * delta > 2.0 * cad_primitives::TAU_MODEL,
        "fixture precondition: the graze must be real-scale (r·δ = {} vs TAU_MODEL)",
        r * delta
    );
    let up = plain.verts[ring[2] as usize];
    let graze = rot_z(up, delta);

    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov.insert(0, vec![graze]);
    let Err(err) = stage1_tessellate_inner(&verts, &edges, &faces, &ov, None) else {
        panic!("real-scale coincidence must stay loud (I4)");
    };
    assert!(
        format!("{err:?}").contains("merge refused"),
        "wrong error: {err:?}"
    );
}

/// Spec rows 4+5: a bit-identical repeat of a merged override dedups; a
/// DISTINCT second override claiming the same uniform slot is loud.
#[test]
pub(crate) fn rim_override_same_slot_repeat_dedups_conflict_is_loud() {
    let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
    let empty: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    let (plain, _) = stage1_tessellate_inner(&verts, &edges, &faces, &empty, None).expect("plain");
    let ring = plain.chains[&0].clone();
    let n = ring.len();
    let up = plain.verts[ring[2] as usize];
    let twin = rot_z(up, 1e-15);

    // Row 4: same bits twice → dedup, single merge.
    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov.insert(0, vec![twin, twin]);
    let (t, _) = stage1_tessellate_inner(&verts, &edges, &faces, &ov, None)
        .expect("bit-identical repeat must dedup");
    assert_eq!(t.chains[&0].len(), n);
    let bits = |p: &Point3| p.as_array().map(f64::to_bits);
    assert_eq!(
        t.verts.iter().filter(|q| bits(q) == bits(&twin)).count(),
        1,
        "exactly one copy of the merged point"
    );

    // Row 5: two DISTINCT points claiming one slot → loud.
    let twin2 = rot_z(up, 2e-15);
    assert_ne!(bits(&twin), bits(&twin2));
    let mut ov2: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov2.insert(0, vec![twin, twin2]);
    let Err(err) = stage1_tessellate_inner(&verts, &edges, &faces, &ov2, None) else {
        panic!("distinct overrides on one slot must be loud");
    };
    assert!(
        format!("{err:?}").contains("distinct"),
        "wrong error: {err:?}"
    );
}

/// Spec rows 6+7: the SEAM slot (k=0) is a B-Rep vertex — a bit-exact
/// override dedups (byte-identical output); a ULP-off override is loud
/// (replacing a B-Rep vertex's bits in one ring would desync every other
/// face sharing that vertex).
#[test]
pub(crate) fn rim_override_seam_bit_exact_dedups_ulp_off_is_loud() {
    let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
    let empty: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    let (plain, _) = stage1_tessellate_inner(&verts, &edges, &faces, &empty, None).expect("plain");
    let seam_pt = plain.verts[plain.chains[&0][0] as usize];

    // Row 6: bit-exact on the seam → dedup, byte-identical.
    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov.insert(0, vec![seam_pt]);
    let (t, inserted) = stage1_tessellate_inner(&verts, &edges, &faces, &ov, None)
        .expect("bit-exact seam override must dedup");
    assert_eq!(plain.verts.len(), t.verts.len());
    for (a, b) in plain.verts.iter().zip(&t.verts) {
        assert_eq!(a.as_array(), b.as_array());
    }
    assert_eq!(plain.tris, t.tris);
    assert!(!inserted.contains(&0));

    // Row 7: ULP-off the seam → loud.
    let seam_twin = rot_z(seam_pt, 1e-15);
    let mut ov2: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov2.insert(0, vec![seam_twin]);
    let Err(err) = stage1_tessellate_inner(&verts, &edges, &faces, &ov2, None) else {
        panic!("ULP-off seam override must stay loud");
    };
    assert!(format!("{err:?}").contains("seam"), "wrong error: {err:?}");
}

/// Half-cylinder lateral patch: two π arcs + two rulings (the KV14 fixture
/// shape without the hole) — the arc-chain override site's test bed.
pub(crate) fn arc_patch_fixture() -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    use std::f64::consts::PI;
    let r = 1.0_f64;
    let on = |theta: f64, z: f64| Point3::new(r * theta.cos(), r * theta.sin(), z);
    let verts = [on(0.0, 0.0), on(PI, 0.0), on(PI, 2.0), on(0.0, 2.0)]
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
    let faces = vec![BRepFace {
        surface: Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        },
        outer_loop: vec![0, 1, 2, 3],
        inner_loops: Vec::new(),
        reversed: false,
    }];
    (verts, edges, faces)
}

/// Spec row 2, ARC site: a ULP-twin override on an interior uniform slot of
/// an open arc CHAIN merges — chain length unchanged, override bits in.
#[test]
pub(crate) fn arc_chord_override_ulp_twin_merges_onto_uniform_slot() {
    let (verts, edges, faces) = arc_patch_fixture();
    let empty: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    let (plain, _) =
        stage1_tessellate_inner(&verts, &edges, &faces, &empty, None).expect("plain patch");
    let chain = plain.chains[&0].clone();
    assert!(
        chain.len() >= 3,
        "fixture: arc chain needs an interior slot"
    );
    let up = plain.verts[chain[1] as usize];
    let twin = rot_z(up, 1e-15);
    let bits = |p: &Point3| p.as_array().map(f64::to_bits);
    assert_ne!(bits(&twin), bits(&up));

    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov.insert(0, vec![twin]);
    let (t, _) = stage1_tessellate_inner(&verts, &edges, &faces, &ov, None)
        .expect("arc-slot ULP twin must merge, not refuse");
    let chain2 = &t.chains[&0];
    assert_eq!(
        chain2.len(),
        chain.len(),
        "merge must not change chain length (I1)"
    );
    assert!(
        t.verts.iter().any(|q| bits(q) == bits(&twin)),
        "merged chain must carry the override's exact bits (I2)"
    );
    assert!(
        !t.verts.iter().any(|q| bits(q) == bits(&up)),
        "displaced uniform arc sample bits must be gone"
    );
}

/// Spec rows 3/4/5, ARC site (adversarial): a real-scale graze of an arc
/// uniform slot stays loud; a bit-identical repeat of a merged arc override
/// dedups; two distinct overrides claiming one arc slot are loud.
#[test]
pub(crate) fn arc_chord_override_real_scale_and_conflict_walls() {
    let (verts, edges, faces) = arc_patch_fixture();
    let empty: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    let (plain, _) =
        stage1_tessellate_inner(&verts, &edges, &faces, &empty, None).expect("plain patch");
    let chain = plain.chains[&0].clone();
    let m = chain.len() - 1; // interior slots = m - 1
    let up = plain.verts[chain[1] as usize];
    let uni_step = std::f64::consts::PI / (m as f64);
    let delta = 0.9 * (uni_step * 1.0e-6);
    assert!(
        1.0 * delta > 2.0 * cad_primitives::TAU_MODEL,
        "fixture precondition: the graze must be real-scale (r·δ = {delta})"
    );

    // Row 3: real-scale graze → loud.
    let graze = rot_z(up, delta);
    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov.insert(0, vec![graze]);
    let Err(err) = stage1_tessellate_inner(&verts, &edges, &faces, &ov, None) else {
        panic!("real-scale arc coincidence must stay loud");
    };
    assert!(
        format!("{err:?}").contains("merge refused"),
        "wrong error: {err:?}"
    );

    // Row 4: bit-identical repeat dedups.
    let twin = rot_z(up, 1e-15);
    let bits = |p: &Point3| p.as_array().map(f64::to_bits);
    let mut ov2: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov2.insert(0, vec![twin, twin]);
    let (t, _) = stage1_tessellate_inner(&verts, &edges, &faces, &ov2, None)
        .expect("bit-identical arc repeat must dedup");
    assert_eq!(t.chains[&0].len(), chain.len());
    assert_eq!(t.verts.iter().filter(|q| bits(q) == bits(&twin)).count(), 1);

    // Row 5: two distinct overrides on one arc slot → loud.
    let twin2 = rot_z(up, 2e-15);
    assert_ne!(bits(&twin), bits(&twin2));
    let mut ov3: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov3.insert(0, vec![twin, twin2]);
    let Err(err) = stage1_tessellate_inner(&verts, &edges, &faces, &ov3, None) else {
        panic!("distinct arc overrides on one slot must be loud");
    };
    assert!(
        format!("{err:?}").contains("distinct"),
        "wrong error: {err:?}"
    );
}

/// Spec rows 6/7, ARC site (adversarial): a bit-exact override on the arc
/// START endpoint dedups (byte-identical output); a ULP-off one is loud.
#[test]
pub(crate) fn arc_chord_override_endpoint_bit_exact_dedups_ulp_off_is_loud() {
    let (verts, edges, faces) = arc_patch_fixture();
    let empty: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    let (plain, _) =
        stage1_tessellate_inner(&verts, &edges, &faces, &empty, None).expect("plain patch");
    let start_pt = verts[0].point; // edge 0 starts at B-Rep vertex 0

    // Row 6: bit-exact on the endpoint → dedup, byte-identical.
    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov.insert(0, vec![start_pt]);
    let (t, _) = stage1_tessellate_inner(&verts, &edges, &faces, &ov, None)
        .expect("bit-exact endpoint override must dedup");
    assert_eq!(plain.verts.len(), t.verts.len());
    for (a, b) in plain.verts.iter().zip(&t.verts) {
        assert_eq!(a.as_array(), b.as_array());
    }
    assert_eq!(plain.tris, t.tris);

    // Row 7: ULP-off the endpoint → loud.
    let start_twin = rot_z(start_pt, 1e-15);
    let mut ov2: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov2.insert(0, vec![start_twin]);
    let Err(err) = stage1_tessellate_inner(&verts, &edges, &faces, &ov2, None) else {
        panic!("ULP-off endpoint override must stay loud");
    };
    assert!(
        format!("{err:?}").contains("endpoint"),
        "wrong error: {err:?}"
    );
}

/// Adversarial interplay: one MERGED twin plus a genuinely INSERTED crossing
/// (propagated to BOTH rims, as Stage-0 always does) on the same cylinder —
/// the insert routes the lateral to azimuth-merge, the merged ring grows by
/// exactly one, all override points are present, and the mesh stays closed.
#[test]
pub(crate) fn rim_override_merge_plus_insert_coexist() {
    let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
    let empty: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    let (plain, _) = stage1_tessellate_inner(&verts, &edges, &faces, &empty, None).expect("plain");
    let ring = plain.chains[&0].clone();
    let n = ring.len();
    let up = plain.verts[ring[2] as usize];
    let twin = rot_z(up, 1e-15);
    // A genuine crossing far from every uniform sample, on the bottom rim,
    // and its axial projection on the top rim (the Stage-0 propagation).
    let insert_bot = rot_z(up, 0.4 * (2.0 * std::f64::consts::PI / n as f64));
    let ib = insert_bot.as_array();
    let insert_top = Point3::new(ib[0], ib[1], 1.0);

    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = Default::default();
    ov.insert(0, vec![twin, insert_bot]);
    ov.insert(1, vec![insert_top]);
    let (t, inserted) = stage1_tessellate_inner(&verts, &edges, &faces, &ov, None)
        .expect("merge + insert must coexist");
    let bits = |p: &Point3| p.as_array().map(f64::to_bits);
    assert_eq!(t.chains[&0].len(), n + 1, "exactly one inserted sample");
    assert!(t.verts.iter().any(|q| bits(q) == bits(&twin)));
    assert!(t.verts.iter().any(|q| bits(q) == bits(&insert_bot)));
    assert!(
        inserted.contains(&0),
        "a genuine insert must still route the rim to azimuth-merge"
    );
    assert!(
        closed_2_manifold(&t.tris),
        "merge + insert cylinder must stay a closed 2-manifold"
    );
}

/// KV14 Slice A seam — the R0040 shape (2026-09-05, `docs/yang_tail_triage.md`):
/// a bounded cylinder patch covering all but a NARROW wedge (here 0.04 rad
/// about θ = 0; R0040's was 10.8 of 210.5 units), each rim split into two
/// arcs at θ = π, closed by two rulings. The wedge is narrower than the rim
/// chord step, so the old "widest gap between boundary vertices" seam rule
/// could not find it — every chord gap tied and won, the cut landed inside
/// the face, and the unrolled polygon crossed itself (RED: "holed lateral
/// CDT failed"). The seam now comes from the outer loop's unwrapped azimuth
/// range and lands in the true wedge: the patch tessellates, its mapped
/// area is the developable `r·(2π − 0.04)·h`, and no triangle bridges the
/// wedge.
#[test]
pub(crate) fn lateral_partial_patch_seam_lands_in_a_wedge_narrower_than_its_rim_chords() {
    use std::f64::consts::PI;
    let r = 1.0_f64;
    let h = 0.5_f64;
    let delta = 0.02_f64;
    let on = |theta: f64, z: f64| Point3::new(r * theta.cos(), r * theta.sin(), z);
    // V0 (δ, 0) → V1 (π, 0) → V2 (2π − δ, 0) → V3 (2π − δ, h) → V4 (π, h) → V5 (δ, h)
    let verts = [
        on(delta, 0.0),
        on(PI, 0.0),
        on(-delta, 0.0),
        on(-delta, h),
        on(PI, h),
        on(delta, h),
    ]
    .into_iter()
    .map(|point| BRepVertex { point })
    .collect::<Vec<_>>();
    let circ = |z: f64, sign: f64| Curve::Circle {
        center: Point3::new(0.0, 0.0, z),
        normal: Vector3::new(0.0, 0.0, sign),
        radius: r,
    };
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 1,
            curve: circ(0.0, 1.0),
        }, // bottom δ → π (CCW about +z)
        BRepEdge {
            start: 1,
            end: 2,
            curve: circ(0.0, 1.0),
        }, // bottom π → 2π − δ
        BRepEdge {
            start: 2,
            end: 3,
            curve: Curve::LineSegment,
        }, // ruling at 2π − δ
        BRepEdge {
            start: 3,
            end: 4,
            curve: circ(h, -1.0),
        }, // top 2π − δ → π (CCW about −z)
        BRepEdge {
            start: 4,
            end: 5,
            curve: circ(h, -1.0),
        }, // top π → δ
        BRepEdge {
            start: 5,
            end: 0,
            curve: Curve::LineSegment,
        }, // ruling at δ
    ];
    let faces = vec![BRepFace {
        surface: Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: r,
        },
        outer_loop: vec![0, 1, 2, 3, 4, 5],
        inner_loops: vec![],
        reversed: false,
    }];
    let t = stage1_tessellate(&verts, &edges, &faces)
        .expect("a partial patch whose wedge is narrower than its rim chords tessellates");
    // The rim sampling must indeed be coarser than the wedge for this pin to
    // exercise the seam rule (otherwise the old gap scan would have found it).
    let n_rim = t
        .verts
        .iter()
        .filter(|p| p.as_array()[2].abs() < 1e-12)
        .count();
    let step = (2.0 * PI - 2.0 * delta) / (n_rim as f64 - 1.0);
    assert!(
        step > 2.0 * delta,
        "fixture: rim step {step:.4} must exceed the wedge {:.4}",
        2.0 * delta
    );
    // Developable area, and no triangle bridging the wedge (every triangle's
    // vertices lie within a chord step of azimuth, never across θ = 0).
    let mut area = 0.0;
    for tri in &t.tris {
        let p: Vec<[f64; 3]> = tri
            .iter()
            .map(|&i| t.verts[i as usize].as_array())
            .collect();
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
        let c = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        area += 0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        let th: Vec<f64> = p.iter().map(|q| q[1].atan2(q[0])).collect();
        let bridges = th.iter().any(|&a| a.abs() <= delta + 1e-9)
            && th.iter().any(|&a| a > 0.0)
            && th.iter().any(|&a| a < 0.0)
            && th.iter().all(|&a| a.abs() < PI / 2.0);
        assert!(!bridges, "triangle bridges the seam wedge: θ = {th:?}");
    }
    let expect = r * (2.0 * PI - 2.0 * delta) * h;
    assert!(
        (area - expect).abs() < 0.05 * expect,
        "mapped area {area} vs developable {expect}"
    );
}

/// KV14 Slice A (spec `yang_stage1_curved_holed_patch`): a cylinder lateral
/// PARTIAL patch (2 sweep arcs + 2 rulings) carrying an interior hole (an
/// on-surface inner loop) must tessellate via the unroll+CDT path so the
/// hole is EXCLUDED from the mesh. The pre-Slice-A partial-patch strip
/// ignored `inner_loops` and paved over the hole (RED before the fix).
#[test]
pub(crate) fn lateral_holed_patch_excludes_hole() {
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
pub(crate) fn torus_poloidal_band_two_encircling_profiles() {
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
    // The two profiles wrap the meridian OPPOSITELY (the band seam bridge
    // requires it), and they wind as a real face's loops do — CCW viewed from
    // outside along the outward normal (`BRepFace::outer_loop`): at the
    // outer equator of the θ = 0.2 ring the outward normal is radial and the
    // band lies toward +θ, so that ring must be walked −φ (material on the
    // left); the θ = 1.4 ring walks +φ. Before the 2026-09-03 class-A fix the
    // consumer laid the band on the SHORTER arc regardless of winding, and
    // this test had the windings the other way round.
    let ring0_rev: Vec<u32> = ring0.iter().rev().copied().collect();
    let outer = loop_of(&ring0_rev, &mut edges);
    let inner = loop_of(&ring1, &mut edges);
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

/// KV14 Slice F-3 fixture: a torus DISK face — one non-wrapping loop of 48
/// `LineSegment` chords (a (u, v) rectangle on the tube, 12 samples a side),
/// no inner loop — the census shape of R0032's re-entry wall (a torus∩cone
/// chord polyline, no analytic curve type). The loop's sense is decided by a
/// 3D WITNESS, not by the chart handedness the consumer asserts: the B-Rep
/// convention puts the material on the loop's LEFT about the face's outward
/// normal, so `n̂ × t̂` of the first chord must point toward an interior point
/// of the rectangle. `material_left = false` walks it the other way (the
/// complement's sense). Returns the B-Rep and the exact developable area of
/// the rectangle, r·Δv·[R·Δu + r·(sin u1 − sin u0)].
fn torus_disk_fixture(
    reversed: bool,
    material_left: bool,
) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>, f64) {
    let (major, minor) = (3.0_f64, 1.0_f64);
    let on = |u: f64, v: f64| {
        let rad = major + minor * u.cos();
        Point3::new(rad * v.cos(), rad * v.sin(), minor * u.sin())
    };
    let (u0, u1, v0, v1) = (0.2_f64, 1.2_f64, 0.5_f64, 1.8_f64);
    let ns = 12usize;
    let mut pts: Vec<Point3> = Vec::with_capacity(4 * ns);
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        pts.push(on(u0 + (u1 - u0) * t, v0));
    }
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        pts.push(on(u1, v0 + (v1 - v0) * t));
    }
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        pts.push(on(u1 - (u1 - u0) * t, v1));
    }
    for k in 0..ns {
        let t = k as f64 / ns as f64;
        pts.push(on(u0, v1 - (v1 - v0) * t));
    }
    // Witness: the torus's outward normal at the first vertex, the first
    // chord's direction, and an interior point of the rectangle.
    let (p0, p1) = (pts[0].as_array(), pts[1].as_array());
    let rho = (p0[0] * p0[0] + p0[1] * p0[1]).sqrt();
    let n = [
        p0[0] - major * p0[0] / rho,
        p0[1] - major * p0[1] / rho,
        p0[2],
    ];
    let t = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let left = [
        n[1] * t[2] - n[2] * t[1],
        n[2] * t[0] - n[0] * t[2],
        n[0] * t[1] - n[1] * t[0],
    ];
    let q = on(0.5 * (u0 + u1), 0.5 * (v0 + v1)).as_array();
    let w = [q[0] - p0[0], q[1] - p0[1], q[2] - p0[2]];
    let walk_is_left_about_torus_outward = w[0] * left[0] + w[1] * left[1] + w[2] * left[2] > 0.0;
    // The face's outward normal is the torus's, or its negation when
    // `reversed`; the loop must be material-left about THAT one.
    let this_walk_is_material_left = walk_is_left_about_torus_outward != reversed;
    if this_walk_is_material_left != material_left {
        pts.reverse();
    }
    let verts: Vec<BRepVertex> = pts.iter().map(|&p| BRepVertex { point: p }).collect();
    let n_v = verts.len() as u32;
    let edges: Vec<BRepEdge> = (0..n_v)
        .map(|k| BRepEdge {
            start: k,
            end: (k + 1) % n_v,
            curve: Curve::LineSegment,
        })
        .collect();
    let faces = vec![BRepFace {
        surface: Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: major,
            minor_radius: minor,
        },
        outer_loop: (0..n_v).collect(),
        inner_loops: vec![],
        reversed,
    }];
    let area = minor * (v1 - v0) * (major * (u1 - u0) + minor * (u1.sin() - u0.sin()));
    (verts, edges, faces, area)
}

/// Oracles shared by the torus-disk tests: exact developable area (inscribed
/// chords fall just below it, never above), the 48 boundary chords are the
/// ONLY single-count edges and each joins consecutive loop vertices (no slit,
/// no crack), Steiner vertices were minted and every vertex lies on the tube,
/// and every triangle faces the torus's outward normal (inward for a
/// `reversed` cavity wall).
fn check_torus_disk_mesh(t: &Stage1Tess, n_boundary: usize, exact_area: f64, reversed: bool) {
    let (major, minor) = (3.0_f64, 1.0_f64);
    assert!(
        t.verts.len() > n_boundary,
        "no Steiner vertices minted ({} verts)",
        t.verts.len()
    );
    for (i, p) in t.verts.iter().enumerate() {
        let a = p.as_array();
        let rho = (a[0] * a[0] + a[1] * a[1]).sqrt();
        let d = ((rho - major).powi(2) + a[2] * a[2]).sqrt() - minor;
        assert!(d.abs() < 1e-9, "vertex {i} off the torus by {d:.3e}");
    }
    let outward_at = |c: [f64; 3]| -> [f64; 3] {
        let rho = (c[0] * c[0] + c[1] * c[1]).sqrt().max(1e-300);
        [c[0] - major * c[0] / rho, c[1] - major * c[1] / rho, c[2]]
    };
    let mut area = 0.0;
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
        area += 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let cen = [
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        ];
        let o = outward_at(cen);
        let dot = n[0] * o[0] + n[1] * o[1] + n[2] * o[2];
        assert!(
            (dot > 0.0) != reversed,
            "triangle {tri:?} faces the wrong way (dot={dot:.3e}, reversed={reversed})"
        );
    }
    assert!(
        area >= 0.985 * exact_area && area <= exact_area * (1.0 + 1e-6),
        "disk area {area} vs exact {exact_area} (inscribed band 0.985…1)"
    );
    let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in &t.tris {
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
        }
    }
    let nb = n_boundary as u32;
    let mut single = 0usize;
    for (&(x, y), &c) in &undirected {
        assert!(c <= 2, "edge ({x},{y}) covered {c} times (fold)");
        if c == 1 {
            single += 1;
            let consecutive = x < nb && y < nb && ((x + 1) % nb == y || (y + 1) % nb == x);
            assert!(
                consecutive,
                "single-count edge ({x},{y}) is not a boundary chord — a crack or slit"
            );
        }
    }
    assert_eq!(single, n_boundary, "boundary chord count");
}

/// KV14 Slice F-3 (R0032): a hole-free torus lateral bounded by ONE
/// non-wrapping loop of `LineSegment` chords — none of the structured torus
/// vocabulary — re-enters Stage 1 as a DISK patch through
/// `tessellate_torus_face` → `tessellate_torus_band` → the UV-CDT's
/// 0-wrapping branch.
#[test]
pub(crate) fn torus_disk_patch_lone_chord_loop() {
    let (verts, edges, faces, exact) = torus_disk_fixture(false, true);
    let t = stage1_tessellate(&verts, &edges, &faces).expect("torus disk tessellation");
    check_torus_disk_mesh(&t, verts.len(), exact, false);
}

/// The `reversed` disk (a cavity wall: the face's outward normal is the
/// torus's inward normal, the loop walked accordingly) tessellates the same
/// region with every triangle pointing INTO the tube.
#[test]
pub(crate) fn torus_disk_patch_reversed_face_points_inward() {
    let (verts, edges, faces, exact) = torus_disk_fixture(true, true);
    let t = stage1_tessellate(&verts, &edges, &faces).expect("reversed torus disk tessellation");
    check_torus_disk_mesh(&t, verts.len(), exact, true);
}

/// P10 region check: the same loop walked in the COMPLEMENT's sense (material
/// on the right) bounds the torus minus the disk; filling the (u, v) polygon
/// interior would silently emit the wrong region, so the consumer must
/// decline — a typed `MalformedTopology`, not a mesh.
#[test]
pub(crate) fn torus_disk_patch_complement_sense_declines_typed() {
    let (verts, edges, faces, _) = torus_disk_fixture(false, false);
    match stage1_tessellate(&verts, &edges, &faces) {
        Err(YangError::MalformedTopology(msg)) => assert!(
            msg.contains("torus patch UV-CDT declined"),
            "unexpected wall text: {msg}"
        ),
        Err(e) => panic!("expected the typed torus-patch decline, got {e:?}"),
        Ok(t) => panic!(
            "complement-sense loop must not tessellate (got {} tris)",
            t.tris.len()
        ),
    }
}

/// Code review 2026-09-04 (apex-cone OPERAND, 84068638): the structured
/// `[rim_e]` apex-fan arm must honour `reversed` exactly like the frustum-band
/// and holed-CDT cone arms — kernel-v2 forwards the flag on a one-rim cone
/// lateral, so an apex CAVITY re-entering a chained boolean would otherwise
/// enter the Stage-1 mesh with every fan triangle pointing INTO the material
/// (a silent in/out parity corruption, never a STOP). Fixture: yang's own
/// PR-YR16 shape — one closed base-rim `Circle` shared with a planar cap and
/// a pre-seeded edge-less apex vertex. The outward cone normal at a fan
/// triangle's centroid is `cos α · r̂ − sin α · â`; every fan triangle's
/// winding normal must agree with it for `reversed == false` and oppose it
/// for `reversed == true`, on the SAME vertex set.
fn apex_cone_fixture(reversed: bool) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    let verts = vec![
        BRepVertex {
            point: Point3::new(1.0, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(0.0, 0.0, 1.0),
        },
    ];
    let edges = vec![BRepEdge {
        start: 0,
        end: 0,
        curve: Curve::Circle {
            center: Point3::new(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, -1.0),
            radius: 1.0,
        },
    }];
    let faces = vec![
        BRepFace {
            surface: Surface::Cone {
                apex: Point3::new(0.0, 0.0, 1.0),
                axis_dir: Vector3::new(0.0, 0.0, -1.0),
                half_angle: std::f64::consts::FRAC_PI_4,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed,
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
    ];
    (verts, edges, faces)
}

fn apex_fan_dots(t: &Stage1Tess) -> Vec<f64> {
    let apex = [0.0, 0.0, 1.0];
    let (cos_a, sin_a) = (
        std::f64::consts::FRAC_PI_4.cos(),
        std::f64::consts::FRAC_PI_4.sin(),
    );
    t.tris[t.face_tri_ranges[0].clone()]
        .iter()
        .map(|tri| {
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
            let rho = (cen[0] * cen[0] + cen[1] * cen[1]).sqrt().max(1e-300);
            // outward = cos α · r̂ − sin α · â with â = (0, 0, −1)
            let o = [cos_a * cen[0] / rho, cos_a * cen[1] / rho, sin_a];
            assert!(
                cen[2] < apex[2] && cen[2] > 0.0,
                "fan centroid {cen:?} is not between the rim and the apex"
            );
            n[0] * o[0] + n[1] * o[1] + n[2] * o[2]
        })
        .collect()
}

#[test]
pub(crate) fn apex_fan_faces_outward_and_reversed_faces_inward() {
    let (v, e, f) = apex_cone_fixture(false);
    let out = stage1_tessellate(&v, &e, &f).expect("apex cone tessellates");
    let dots = apex_fan_dots(&out);
    assert!(dots.len() >= 3, "apex fan has {} triangles", dots.len());
    assert!(
        dots.iter().all(|d| *d > 0.0),
        "an outward apex fan must face the cone's outward normal: {dots:?}"
    );

    let (v, e, f) = apex_cone_fixture(true);
    let cav = stage1_tessellate(&v, &e, &f).expect("reversed apex cone tessellates");
    assert_eq!(cav.verts, out.verts, "`reversed` must not move a vertex");
    assert_eq!(cav.tris.len(), out.tris.len());
    let dots = apex_fan_dots(&cav);
    assert!(
        dots.iter().all(|d| *d < 0.0),
        "a REVERSED apex fan (a conical cavity) must face INTO the cone: {dots:?}"
    );
}

/// Code review 2026-09-04 (Slice F-3 follow-through): Stage-6 geometric face
/// resolution `tol_for` keyed a torus face on the Circle-rim `band` alone, so
/// a Circle-free torus operand (the R0032 disk) resolved NO triangle on the
/// LINEAGE-LESS path — every kept triangle was a `FaceResolutionFailed`.
/// Production inputs never saw it (kernel-v2 lineage resolves first), but the
/// C++ sidecar parity oracle and the mock-label fixtures do. The arm now folds
/// in `torus_chord_bound(R, r)` for patch-path faces, as Stage 4 already did.
/// Pin: the disk's own Stage-1 mesh, labelled on A with NO source lineage,
/// must not fail at face resolution (whatever a later stage says about an
/// open sheet).
#[test]
pub(crate) fn lineage_less_torus_disk_operand_resolves_its_faces() {
    let (verts, edges, faces, _) = torus_disk_fixture(false, true);
    let a = BRep::new(verts, edges, faces).expect("disk brep");
    // B must OVERLAP A's extent: a disjoint pair takes the early
    // concatenation path and never resolves a face.
    let b = cube_brep([1.5, 2.5, 0.0]);
    let m = a.as_mesh();
    let n = m.tris.len();
    assert!(n > 0);
    let la = LabeledArrangement {
        mesh: Mesh::new(m.verts.clone(), m.tris.clone()),
        surface: vec![vec![LaInputId(0)]; n],
        inside: vec![vec![false, false]; n],
        patch: vec![0; n],
        source: Vec::new(),
        intersection_edges: Default::default(),
        num_inputs: 2,
    };
    let backend = LabelMockBackend::new(la);
    if let Err(YangError::FaceResolutionFailed { tri }) = boolean(&a, &b, BoolOp::Union, &backend) {
        panic!(
            "a lineage-less torus DISK operand must resolve its own triangles geometrically \
             (tri {tri} failed — the torus arm has no Circle-rim band and must use \
             torus_chord_bound)"
        );
    }
}

/// KV14 Slice F-3 chord band: an input whose only curved face is a torus DISK
/// of chords has NO `Curve::Circle` rim, so the rim-derived bound is `None` —
/// yet its Stage-1 mesh sags by the patch tessellator's own budget. The input
/// bound must fold `torus_chord_bound(R, r)` in for exactly the PATCH-path
/// faces (`torus_face_takes_patch_path`), and Stage 4's relocation budget
/// must then be `Some` against a planar partner (the `chord_band_none`
/// producer-fault STOP no longer fires). A STRUCTURED torus lateral (closed
/// profile circles + seam) is not a patch-path face.
#[test]
pub(crate) fn torus_patch_faces_carry_their_own_chord_band() {
    let (verts, edges, faces, _) = torus_disk_fixture(false, true);
    let (major, minor) = (3.0_f64, 1.0_f64);
    assert!(
        torus_face_takes_patch_path(&faces[0], &edges, major, minor),
        "a lone chord loop is a patch-path (disk) face"
    );
    assert_eq!(
        curved_chord_bound(&edges),
        None,
        "the disk carries no Circle rim (the case this arm exists for)"
    );
    let disk = BRep::new(verts, edges, faces).expect("disk brep");
    let expect = torus_chord_bound(major, minor);
    assert_eq!(
        input_curved_chord_bound(&disk),
        Some(expect),
        "the torus disk input reports the patch tessellator's own budget"
    );
    // Against an all-planar partner the relocation budget is the disk's.
    let slab = crate::tests_unit::n2_junction::rj_box([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]);
    assert_eq!(input_curved_chord_bound(&slab), None);
    assert_eq!(stage4_chord_band(&disk, &slab), Some(expect));

    // A structured lateral: one closed profile circle in its outer loop keeps
    // it on the (θ × φ) grid — no torus bound folded in.
    let structured = BRepFace {
        surface: Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: major,
            minor_radius: minor,
        },
        outer_loop: vec![0],
        inner_loops: vec![],
        reversed: false,
    };
    let prof = vec![BRepEdge {
        start: 0,
        end: 0,
        curve: Curve::Circle {
            center: Point3::new(major, 0.0, 0.0),
            normal: Vector3::new(0.0, 1.0, 0.0),
            radius: minor,
        },
    }];
    assert!(
        !torus_face_takes_patch_path(&structured, &prof, major, minor),
        "a closed profile circle marks the structured grid path"
    );
}

/// KV6d closed torus (spec `kv6d_closed_torus_revolve.md`): the CLOSED
/// `Surface::Torus` face — 1 seam anchor vertex, 2 closed seam circles
/// (poloidal profile radius r + toroidal outer equator radius R+r), outer
/// loop `[prof, eq, prof, eq]` (both twin traversals, as kernel-v2's
/// `to_yang_brep` emits) — tessellates via the doubly periodic grid.
/// Oracles: CLOSED watertight (every undirected edge count exactly 2 — a
/// crack OR a double cover both fail), single cover by total area
/// (4π²·R·r, inscribed), and χ = V − E + F = 0 (genus 1).
#[test]
pub(crate) fn torus_closed_full_turn_doubly_periodic() {
    use std::f64::consts::PI;
    let major = 3.0_f64;
    let minor = 1.0_f64;
    let v0 = Point3::new(major + minor, 0.0, 0.0);
    let verts = vec![BRepVertex { point: v0 }];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(major, 0.0, 0.0),
                normal: Vector3::new(0.0, 1.0, 0.0),
                radius: minor,
            },
        },
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(0.0, 0.0, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius: major + minor,
            },
        },
    ];
    let faces = vec![BRepFace {
        surface: Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: major,
            minor_radius: minor,
        },
        outer_loop: vec![0, 1, 0, 1],
        inner_loops: vec![],
        reversed: false,
    }];
    let t = stage1_tessellate(&verts, &edges, &faces).expect("closed torus tessellation");
    assert!(!t.tris.is_empty(), "must produce triangles");

    // Every undirected edge exactly twice: watertight AND single-cover.
    let mut undirected: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in &t.tris {
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            *undirected.entry((x.min(y), x.max(y))).or_insert(0) += 1;
        }
    }
    for (&(x, y), &c) in &undirected {
        assert_eq!(c, 2, "edge ({x},{y}) covered {c} times (crack or fold)");
    }

    // χ = V − E + F = 0 for the torus.
    let used: std::collections::BTreeSet<u32> = t.tris.iter().flatten().copied().collect();
    let chi = used.len() as i64 - undirected.len() as i64 + t.tris.len() as i64;
    assert_eq!(chi, 0, "closed grid must be genus 1");

    // Single cover: total area fills the torus area 4π²Rr from below.
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
    let full = 4.0 * PI * PI * major * minor;
    assert!(
        area > 0.95 * full && area <= full + 1e-9,
        "closed torus area {area} must fill 4π²Rr (≈{full}, inscribed)"
    );

    // Bijective sources: seam-ring verts map to the two B-Rep edges, the
    // anchor to the B-Rep vertex, interior to the face.
    assert!(matches!(t.sources[0], TessellationSource::BRepVertex(0)));
    assert!(t
        .sources
        .iter()
        .any(|s| matches!(s, TessellationSource::BRepFace { .. })));
}

/// EXCLUDED. Covers the cone `inner_loops` → CDT route (P4).
#[test]
pub(crate) fn cone_holed_patch_excludes_hole() {
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
pub(crate) fn periodic_strip_two_encircling_rims() {
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
pub(crate) fn planar_ellipse_sector_reenters_stage1() {
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
pub(crate) fn planar_full_ellipse_cap_reenters_stage1() {
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
pub(crate) fn lateral_oblique_ellipse_tube_reenters_stage1() {
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
pub(crate) fn lateral_partial_patch_multi_arc_no_holes() {
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

/// Thin-band chart guard (R0044 face 166, 2026-09-05): a CONE band whose
/// two rims sit 0.02 apart in height (slant gap 0.0283 at r ≈ 10, α = 45°),
/// as a bounded half-band (two arcs per rim + two rulings) so it takes the
/// holed-lateral CDT path whose chart is the cone's isometric development.
/// At the natural rim density (N ≈ 14, sag 0.25 ≫ 0.028) the two rims'
/// chords interleave in the chart — RED before the guard, either as a loud
/// CDT failure or, if the flood-fill CDT paves the crossing (the R0040 pin
/// showed it can), as the rim-density / area / fold assertions below.
/// `face_rim_pair_phantom_n` now folds the
/// band's own demand (N ≥ 84) into the shared rim N: the band tessellates,
/// its area is the developed annular sector `½·Δθ·sin α·(ℓ₂² − ℓ₁²)` within
/// the corrugation the sampling allows (see the area assertion), no mesh
/// edge is covered more than twice, and the rims carry ≥ 60 segments per
/// turn (the outer rim's sag ≤ 0.0141 ⇒ N ≥ 60).
#[test]
pub(crate) fn thin_cone_band_tessellates_at_its_own_rim_density() {
    use std::f64::consts::PI;
    let alpha = PI / 4.0;
    let (h1, h2) = (10.0_f64, 10.02_f64);
    let (r1, r2) = (h1 * alpha.tan(), h2 * alpha.tan());
    let on = |r: f64, theta: f64, z: f64| Point3::new(r * theta.cos(), r * theta.sin(), z);
    // V0 (r1, 0) → V1 (r1, π/2) → V2 (r1, π) → V3 (r2, π) → V4 (r2, π/3) → V5 (r2, 0).
    // The upper rim splits at π/3, NOT π/2: arc chains sample from their own
    // start, so the two rims' vertices sit at DIFFERENT azimuths and their
    // coarse chords interleave in the chart (aligned rims give parallel
    // chords that never cross — R0044's rims are split at unrelated angles).
    let verts = [
        on(r1, 0.0, h1),
        on(r1, PI / 2.0, h1),
        on(r1, PI, h1),
        on(r2, PI, h2),
        on(r2, PI / 3.0, h2),
        on(r2, 0.0, h2),
    ]
    .into_iter()
    .map(|point| BRepVertex { point })
    .collect::<Vec<_>>();
    let circ = |z: f64, r: f64, sign: f64| Curve::Circle {
        center: Point3::new(0.0, 0.0, z),
        normal: Vector3::new(0.0, 0.0, sign),
        radius: r,
    };
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 1,
            curve: circ(h1, r1, 1.0),
        },
        BRepEdge {
            start: 1,
            end: 2,
            curve: circ(h1, r1, 1.0),
        },
        BRepEdge {
            start: 2,
            end: 3,
            curve: Curve::LineSegment,
        },
        BRepEdge {
            start: 3,
            end: 4,
            curve: circ(h2, r2, -1.0),
        },
        BRepEdge {
            start: 4,
            end: 5,
            curve: circ(h2, r2, -1.0),
        },
        BRepEdge {
            start: 5,
            end: 0,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![BRepFace {
        surface: Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: alpha,
        },
        outer_loop: vec![0, 1, 2, 3, 4, 5],
        inner_loops: vec![],
        reversed: false,
    }];
    let t = stage1_tessellate(&verts, &edges, &faces)
        .expect("a thin cone band tessellates at its own rim density");
    // Rim density: the lower rim's half-turn carries ≥ 30 segments (≥ 60/turn).
    let n_lower = t
        .verts
        .iter()
        .filter(|p| (p.as_array()[2] - h1).abs() < 1e-9)
        .count();
    assert!(
        n_lower >= 31,
        "lower rim vertices {n_lower} (need ≥ 31 for N ≥ 60)"
    );
    // Area: the developed annular half-sector.
    let (l1, l2) = (h1 / alpha.cos(), h2 / alpha.cos());
    let expect = 0.5 * PI * alpha.sin() * (l2 * l2 - l1 * l1);
    let mut area = 0.0;
    let mut cover: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
    for tri in &t.tris {
        let p: Vec<[f64; 3]> = tri
            .iter()
            .map(|&i| t.verts[i as usize].as_array())
            .collect();
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
        let c = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        area += 0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        for k in 0..3 {
            let (x, y) = (tri[k], tri[(k + 1) % 3]);
            *cover.entry((x.min(y), x.max(y))).or_insert(0) += 1;
        }
    }
    // The mesh is a CORRUGATION of the band: with the rims sampled at a sag
    // about half the band width (0.0137 vs 0.0283 — the guard's own margin),
    // each flat triangle between an inner chord and an outer vertex tilts
    // into the cone, so its area exceeds the surface patch it covers while
    // every vertex stays exactly on the surface (the paper's d_ε contract).
    // Measured +3.2 % at N = 60 (signed = unsigned area, no fold); pinned at
    // 5 %. (At N = 84 the excess was < 3 %.)
    assert!(
        (area - expect).abs() < 0.05 * expect,
        "band area {area} vs developed sector {expect}"
    );
    assert!(
        cover.values().all(|&c| c <= 2),
        "an edge is covered more than twice (fold)"
    );
}

/// KV14 Slice E: a non-canonical CONE partial patch (multi-arc, no holes)
/// re-enters the unroll+CDT path. A cone frustum sector [A,A,A,L,A,A,A,L]
/// (R0020's vocabulary) with the u-scale varying by axial radius. Oracles:
/// the patch fills the exact developable sector-frustum area (from below —
/// chord-sampled), it is watertight and bounded (no interior hole), and it
/// faces radially outward.
#[test]
pub(crate) fn cone_partial_patch_multi_arc_no_holes() {
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
pub(crate) fn lateral_holed_patch_reversed_and_multi_hole() {
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
    let t = stage1_tessellate(&verts, &edges, &faces).expect("reversed multi-hole tessellation");
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
pub(crate) fn rim_override_band_close_distinct_points_both_inserted() {
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
    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
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
    let mut counts: std::collections::BTreeMap<(u32, u32), u32> = std::collections::BTreeMap::new();
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
    let mut dup: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
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
pub(crate) fn rim_override_wrap_seam_cyclic_alignment() {
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
    let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
    ov.insert(0, vec![Point3::new(r * c, r * s, 0.0)]);
    ov.insert(1, vec![Point3::new(r * c, r * s, 1.0)]);
    let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None).expect(
        "wrap-seam cylinder must tessellate — the azimuth-merge pairing \
             must align the rings cyclically, not by absolute sort position",
    );
    let mut counts: std::collections::BTreeMap<(u32, u32), u32> = std::collections::BTreeMap::new();
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
pub(crate) fn rim_override_ulp_twins_exact_order_both_rims() {
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
pub(crate) fn rim_override_accepts_chord_point_rejects_off_rim() {
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
    let mut both: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
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
            stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &single(1, outside), None),
            Err(YangError::MalformedTopology(_))
        ),
        "a point outside the rim circle must be rejected"
    );
}

/// Chord-band regression for the boolean-output torus patch (2026-08-08
/// deficit-class fix, `docs/audits/volume_oracle_flags_anchored.md`): the
/// UV-CDT patch path must carry the STRUCTURED tessellator's sagitta band —
/// the pre-fix area-only refinement measured ~8× over it (the R0057/R0059
/// silent volume deficits). Quarter-tube patch on an R0057-scaled torus;
/// every emitted edge midpoint's distance to the surface is the sag.
#[test]
fn torus_patch_edges_meet_chord_band() {
    use std::f64::consts::PI;
    let (major, minor) = (52.0_f64, 34.0_f64);
    let n_seg = 71u32;
    let s = 2.0 * PI * minor / f64::from(n_seg); // minor-chord budget
    let center = Point3::new(0.0, 0.0, 0.0);
    let axis = Vector3::new(0.0, 0.0, 1.0);
    // u = meridian angle (minor circle), v = longitude about the axis.
    let on = |u: f64, v: f64| {
        let rad = major + minor * u.cos();
        Point3::new(rad * v.cos(), rad * v.sin(), minor * u.sin())
    };
    // Boundary rectangle u∈[0, π/2], v∈[0, 1.0 rad], sampled at the band's
    // own edge budgets (the production callers sample at chord density).
    let (u1, v1) = (PI / 2.0, 1.0_f64);
    let du = s / minor; // Δu per sample
    let dv = s / (major + minor); // Δv per sample (worst-radius chord)
    let (nu, nv) = ((u1 / du).ceil() as usize, (v1 / dv).ceil() as usize);
    let mut boundary: Vec<Point3> = Vec::new();
    for i in 0..nu {
        boundary.push(on(u1 * i as f64 / nu as f64, 0.0));
    }
    for j in 0..nv {
        boundary.push(on(u1, v1 * j as f64 / nv as f64));
    }
    for i in 0..nu {
        boundary.push(on(u1 * (nu - i) as f64 / nu as f64, v1));
    }
    for j in 0..nv {
        boundary.push(on(0.0, v1 * (nv - j) as f64 / nv as f64));
    }
    // Material-left about the outward normal is CW in this chart (KV14 Slice
    // F-3's region check); the rectangle above is walked CCW — reverse it.
    boundary.reverse();
    let (verts, tris) =
        crate::tessellate_torus_patch(center, axis, major, minor, &boundary, &[], s * s, false)
            .expect("torus patch tessellation");
    // Sag of an edge midpoint: distance to the torus surface.
    let sag_of = |p: Point3| -> f64 {
        let rho = (p.x() * p.x() + p.y() * p.y()).sqrt();
        (((rho - major).powi(2) + p.z() * p.z()).sqrt() - minor).abs()
    };
    let mut max_sag: f64 = 0.0;
    for t in &tris {
        for e in 0..3 {
            let a = verts[t[e] as usize];
            let b = verts[t[(e + 1) % 3] as usize];
            let m = Point3::new(
                (a.x() + b.x()) / 2.0,
                (a.y() + b.y()) / 2.0,
                (a.z() + b.z()) / 2.0,
            );
            max_sag = max_sag.max(sag_of(m));
        }
    }
    // Canonical minor-chord sagitta at n_seg=71.
    let band = minor * (1.0 - (PI / f64::from(n_seg)).cos());
    // Measured 2026-08-08 post-fix: max = 1.39×band (interior ≈ 1×; the
    // boundary strip's ½-cell clearance allows a little over). Pre-fix: ~8×.
    assert!(
        max_sag <= 2.0 * band,
        "torus patch edge sag {max_sag:.4} exceeds 2× the canonical band {band:.4}"
    );
    eprintln!(
        "torus_patch chord band: max_sag={max_sag:.4} band={band:.4} ratio={:.2}",
        max_sag / band
    );
}
