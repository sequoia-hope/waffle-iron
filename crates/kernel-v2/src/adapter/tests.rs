use super::*;

/// Build a staged unit-square profile and return its KernelId.
fn stage_unit_square(adapter: &mut KernelV2Adapter) -> KernelId {
    let profile = ClosedProfile {
        entity_ids: vec![0, 1, 2, 3],
        is_outer: true,
        vertex_ids: vec![0, 1, 2, 3],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let positions: HashMap<u32, (f64, f64)> = [
        (0, (0.0, 0.0)),
        (1, (1.0, 0.0)),
        (2, (1.0, 1.0)),
        (3, (0.0, 1.0)),
    ]
    .into_iter()
    .collect();
    let ids = adapter
        .make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("square profile stages");
    assert_eq!(ids.len(), 1);
    ids[0]
}

/// Count cylinder faces in the solid behind a handle.
fn cylinder_face_count(adapter: &KernelV2Adapter, handle: &KernelSolidHandle) -> usize {
    let sid = adapter.solid_of(handle).expect("solid");
    adapter
        .solid_faces(sid)
        .into_iter()
        .filter(|&f| {
            matches!(
                adapter.arena.face(f).map(|fc| fc.surface),
                Ok(Some(Surface::Cylinder { .. }))
            )
        })
        .count()
}

/// Two-triangle-faced imported test body sharing one boundary edge.
fn imported_test_data() -> waffle_types::kernel::ImportedBodyData {
    use waffle_types::kernel::{
        ImportedBodyData, ImportedEdgeData, ImportedFaceData, ImportedShellData, ImportedSurface,
    };
    let face = |z: f64| ImportedFaceData {
        surface: ImportedSurface::Plane {
            origin: [0.0, 0.0, z],
            normal: [0.0, 0.0, 1.0],
        },
        positions: vec![0.0, 0.0, z, 1.0, 0.0, z, 0.0, 1.0, z],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 2],
        edge_indices: vec![0],
    };
    ImportedBodyData {
        source_name: "test-import".into(),
        shells: vec![ImportedShellData {
            faces: vec![face(0.0), face(2.0)],
            edges: vec![ImportedEdgeData {
                polyline: vec![[0.0, 0.0, 0.0], [0.5, 0.0, 1.0], [1.0, 0.0, 2.0]],
            }],
        }],
        warnings: vec![],
    }
}

#[test]
fn imported_body_is_first_class_for_render_and_introspection() {
    let mut adapter = KernelV2Adapter::new();
    let handle = adapter
        .import_body(&imported_test_data())
        .expect("import succeeds");

    // Render: one range per face, indices offset per face.
    let mesh = adapter.tessellate(&handle, 0.001).expect("tessellates");
    assert_eq!(mesh.face_ranges.len(), 2);
    assert_eq!(mesh.indices.len(), 6);
    assert_eq!(mesh.vertices.len(), 18);
    assert_eq!(mesh.face_ranges[1].start_index, 3);
    assert!(mesh.indices[3..].iter().all(|&i| i >= 3));

    // Edge overlay: 3-point polyline pair-expands to 4 vertices.
    let edges = adapter.extract_edges(&handle, 0.001).expect("edges");
    assert_eq!(edges.edge_ranges.len(), 1);
    assert_eq!(edges.vertices.len(), 12);

    // Introspection round-trip.
    let faces = adapter.list_faces(&handle);
    assert_eq!(faces.len(), 2);
    let face_edges = adapter.face_edges(faces[0]);
    assert_eq!(face_edges.len(), 1);
    let ef = adapter.edge_faces(face_edges[0]);
    assert_eq!(ef.len(), 2, "shared edge bounds both faces");
    assert_eq!(adapter.face_neighbors(faces[0]), vec![faces[1]]);
    let (v0, v1) = adapter.edge_vertices(face_edges[0]);
    assert_ne!(v0, v1);

    // Signatures: planar vocabulary + geometry, matching the sketch-plane
    // resolver's expectations (surface_type == "planar", unit normal).
    let sig = adapter.compute_signature(faces[0], TopoKind::Face);
    assert_eq!(sig.surface_type.as_deref(), Some("planar"));
    assert_eq!(sig.normal, Some([0.0, 0.0, 1.0]));
    assert!((sig.area.unwrap() - 0.5).abs() < 1e-12);
    let all = adapter.compute_all_signatures(&handle, TopoKind::Face);
    assert_eq!(all.len(), 2);

    // No collision with arena ids: an arena solid built afterwards keeps
    // working through the same adapter.
    let face = stage_unit_square(&mut adapter);
    let solid = adapter
        .extrude_face(face, [0.0, 0.0, 1.0], 1.0)
        .expect("arena extrude beside imported body");
    assert_eq!(adapter.list_faces(&solid).len(), 6);

    // Booleans with an imported operand: typed NotSupported wall (SI2).
    let err = adapter.boolean_union(&handle, &solid).unwrap_err();
    assert!(
        matches!(&err, KernelError::NotSupported { operation } if operation.contains("SI2")),
        "want typed SI2 wall, got: {err:?}"
    );
}

#[test]
fn import_body_rejects_empty_data() {
    let mut adapter = KernelV2Adapter::new();
    let empty = waffle_types::kernel::ImportedBodyData::default();
    assert!(adapter.import_body(&empty).is_err());
}

#[test]
fn d_shape_arc_profile_extrudes_with_cylinder_faces() {
    // KV12 Tier 2 (E4): a D-shape — diameter line then a semicircle arc —
    // reconstructs to an exact line/arc loop. The 180° arc exceeds the
    // arena's minor-arc limit, so reconstruction splits it into two <π
    // sub-arcs ⇒ two cylinder side patches. Drawn LINE-FIRST (as the GUI
    // does), the arc run wraps the closing vertex.
    let mut adapter = KernelV2Adapter::new();
    // v0=(-1,0), v1=(1,0) [diameter line], then arc samples over the top
    // back toward v0 at 30° steps (v6 = 150°; the closing edge v6→v0 is
    // the last arc span).
    let mut positions: HashMap<u32, (f64, f64)> = HashMap::new();
    positions.insert(0, (-1.0, 0.0));
    positions.insert(1, (1.0, 0.0));
    for (k, deg) in [
        (2u32, 30.0f64),
        (3, 60.0),
        (4, 90.0),
        (5, 120.0),
        (6, 150.0),
    ] {
        let t = deg.to_radians();
        positions.insert(k, (t.cos(), t.sin()));
    }
    let profile = ClosedProfile {
        entity_ids: vec![],
        is_outer: true,
        vertex_ids: vec![0, 1, 2, 3, 4, 5, 6],
        circle: None,
        spline_segments: vec![],
        // The arc covers vertices 1 → 0 (over the top); end < start wraps.
        arc_segments: vec![waffle_types::ArcSegment {
            start_vertex_index: 1,
            end_vertex_index: 0,
            center_u: 0.0,
            center_v: 0.0,
            radius: 1.0,
        }],
    };
    let ids = adapter
        .make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("D-shape stages");
    let handle = adapter
        .extrude_face(ids[0], [0.0, 0.0, 1.0], 2.0)
        .expect("D-shape extrudes");
    // Two cylinder patches (the split semicircle), not a chord polygon.
    assert_eq!(
        cylinder_face_count(&adapter, &handle),
        2,
        "split semicircle ⇒ 2 cylinder patches"
    );
}

#[test]
fn holed_arc_profile_extrudes_tier2_with_cylinder_wall() {
    // E4b: an arc-bearing OUTER with a hole routes through the exact Tier-2
    // path — the corner arc becomes a cylinder patch AND the hole is
    // carried (genus 1), not a Tier-1 chord fallback.
    let mut adapter = KernelV2Adapter::new();
    let mut positions: HashMap<u32, (f64, f64)> = HashMap::new();
    // Outer: a rounded-ish loop — square corners (0..4) plus a quarter arc
    // bulging at the top-right corner. Sample the arc at a few points.
    positions.insert(0, (0.0, 0.0));
    positions.insert(1, (4.0, 0.0));
    positions.insert(2, (4.0, 3.0)); // arc start (0° on centre (3,3) r1)
    let s = std::f64::consts::FRAC_1_SQRT_2; // 45° sample, exactly on the circle
    positions.insert(3, (3.0 + s, 3.0 + s));
    positions.insert(4, (3.0, 4.0)); // arc end (90°)
    positions.insert(5, (0.0, 4.0));
    let outer = ClosedProfile {
        entity_ids: vec![],
        is_outer: true,
        vertex_ids: vec![0, 1, 2, 3, 4, 5],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![waffle_types::ArcSegment {
            start_vertex_index: 2,
            end_vertex_index: 4,
            center_u: 3.0,
            center_v: 3.0,
            radius: 1.0,
        }],
    };
    // A square hole well inside the outer.
    for (k, p) in [
        (10u32, (1.0, 1.0)),
        (11, (2.0, 1.0)),
        (12, (2.0, 2.0)),
        (13, (1.0, 2.0)),
    ] {
        positions.insert(k, p);
    }
    let hole = ClosedProfile {
        entity_ids: vec![],
        is_outer: false,
        vertex_ids: vec![10, 11, 12, 13],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let ids = adapter
        .make_faces_from_profiles(
            &[outer, hole],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("holed arc profile stages");
    let handle = adapter
        .extrude_face(ids[0], [0.0, 0.0, 1.0], 2.0)
        .expect("holed arc profile extrudes (Tier 2)");
    // Tier 2: the corner quarter-arc survives as an exact cylinder patch.
    assert_eq!(
        cylinder_face_count(&adapter, &handle),
        1,
        "one corner cylinder patch on the holed solid"
    );
}

#[test]
fn extrude_square_produces_valid_box() {
    let mut adapter = KernelV2Adapter::new();
    let face = stage_unit_square(&mut adapter);
    let handle = adapter
        .extrude_face(face, [0.0, 0.0, 1.0], 2.0)
        .expect("extrude succeeds");

    // Introspection: a box has 6 faces, 12 edges, 8 vertices.
    assert_eq!(adapter.list_faces(&handle).len(), 6);
    assert_eq!(adapter.list_edges(&handle).len(), 12);
    assert_eq!(adapter.list_vertices(&handle).len(), 8);

    // Tessellation: 12 triangles, valid contiguous face ranges.
    let mesh = adapter.tessellate(&handle, 0.1).expect("tessellates");
    assert_eq!(mesh.indices.len() / 3, 12);
    assert_eq!(mesh.face_ranges.len(), 6);
    let mut expected_start = 0;
    for r in &mesh.face_ranges {
        assert_eq!(r.start_index, expected_start);
        expected_start = r.end_index;
    }
    assert_eq!(expected_start as usize, mesh.indices.len());

    // Edges: 12 segments.
    let edges = adapter.extract_edges(&handle, 0.1).expect("edges");
    assert_eq!(edges.edge_ranges.len(), 12);
    assert_eq!(edges.vertices.len(), 12 * 2 * 3);
}

#[test]
fn face_signatures_carry_area_and_normal() {
    let mut adapter = KernelV2Adapter::new();
    let face = stage_unit_square(&mut adapter);
    let handle = adapter.extrude_face(face, [0.0, 0.0, 1.0], 2.0).unwrap();
    let sigs = adapter.compute_all_signatures(&handle, TopoKind::Face);
    assert_eq!(sigs.len(), 6);
    // Total surface area of a 1×1×2 box = 2·(1·1) + 4·(1·2) = 10.
    let total: f64 = sigs.iter().map(|(_, s)| s.area.unwrap()).sum();
    assert!((total - 10.0).abs() < 1e-9, "total area {total}");
    for (_, s) in &sigs {
        let n = s.normal.unwrap();
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-9);
        assert_eq!(s.surface_type.as_deref(), Some("planar"));
    }
}

/// PR-KV5b flipped the circle-profile wall (circles now stage — see the
/// KV5b tests below); spline- and arc-SEGMENT profiles remain loudly
/// unsupported.
#[test]
fn spline_and_arc_segment_profiles_are_loudly_unsupported() {
    let mut adapter = KernelV2Adapter::new();
    let base = ClosedProfile {
        entity_ids: vec![],
        is_outer: true,
        vertex_ids: vec![],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let mut spline = base.clone();
    spline.spline_segments = vec![waffle_types::kernel::SplineSegment {
        start_point_index: 0,
        end_point_index: 1,
        control_points: vec![(0.0, 0.0), (1.0, 0.5), (2.0, 0.0)],
    }];
    let err = adapter
        .make_faces_from_profiles(
            &[spline],
            [0.0; 3],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &HashMap::new(),
        )
        .unwrap_err();
    assert!(matches!(err, KernelError::NotSupported { .. }), "{err:?}");

    let mut arc = base.clone();
    arc.arc_segments = vec![waffle_types::ArcSegment {
        start_vertex_index: 0,
        end_vertex_index: 1,
        center_u: 0.0,
        center_v: 0.0,
        radius: 1.0,
    }];
    let err = adapter
        .make_faces_from_profiles(
            &[arc],
            [0.0; 3],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &HashMap::new(),
        )
        .unwrap_err();
    assert!(matches!(err, KernelError::NotSupported { .. }), "{err:?}");
}

#[test]
fn revolve_fillet_chamfer_shell_are_loudly_unsupported() {
    // Revolve is SUPPORTED since PR-KV6a; what stays loud here is its
    // input validation (an axis along the plane NORMAL is invalid input
    // → plain error, not a capability wall)…
    let mut adapter = KernelV2Adapter::new();
    let face = stage_unit_square(&mut adapter);
    let err = adapter
        .revolve_face(face, [0.0; 3], [0.0, 0.0, 1.0], 360.0)
        .unwrap_err();
    assert!(matches!(err, KernelError::Other { .. }), "{err:?}");

    // …and the indefinitely-deferred operations.

    let face2 = stage_unit_square(&mut adapter);
    let handle = adapter.extrude_face(face2, [0.0, 0.0, 1.0], 1.0).unwrap();
    assert!(matches!(
        adapter.fillet_edges(&handle, &[], 0.1).unwrap_err(),
        KernelError::NotSupported { .. }
    ));
    assert!(matches!(
        adapter.chamfer_edges(&handle, &[], 0.1).unwrap_err(),
        KernelError::NotSupported { .. }
    ));
    assert!(matches!(
        adapter.shell(&handle, &[], 0.1).unwrap_err(),
        KernelError::NotSupported { .. }
    ));
}

// ── PR-KV5b RED: circle profiles through the legacy trait ──────────────

/// Stage a circle profile (legacy `CircleProfile` semantics: center in
/// sketch-plane (u, v) coordinates, radius in meters).
fn stage_circle(
    adapter: &mut KernelV2Adapter,
    origin: [f64; 3],
    center: (f64, f64),
    radius: f64,
) -> KernelId {
    let profile = ClosedProfile {
        entity_ids: vec![7],
        is_outer: true,
        vertex_ids: vec![],
        circle: Some(waffle_types::kernel::CircleProfile {
            center_u: center.0,
            center_v: center.1,
            radius,
        }),
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let ids = adapter
        .make_faces_from_profiles(
            &[profile],
            origin,
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &HashMap::new(),
        )
        .expect("circle profile stages (PR-KV5b)");
    assert_eq!(ids.len(), 1);
    ids[0]
}

fn render_mesh_volume(mesh: &RenderMesh) -> f64 {
    let mut vol = 0.0f64;
    let p = |i: u32| {
        let i = i as usize * 3;
        [
            mesh.vertices[i] as f64,
            mesh.vertices[i + 1] as f64,
            mesh.vertices[i + 2] as f64,
        ]
    };
    for t in mesh.indices.chunks(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        vol += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    vol
}

/// PR-KV5b: legacy CircleProfile → kernel-v2 cylinder, end to end
/// through the legacy trait: stage, extrude, introspect, tessellate,
/// extract edges. The canonical cylinder topology is 3 faces / 3 edges /
/// 2 vertices; the tessellated volume matches πr²h within kernel-v2's
/// render sagitta band (N = 71 at the canonical tolerance →
/// relative deficit 1 − (N/2π)·sin(2π/N) ≈ 6.5e-4).
#[test]
fn circle_profile_extrudes_to_cylinder_through_legacy_trait() {
    let mut adapter = KernelV2Adapter::new();
    let face = stage_circle(&mut adapter, [0.0, 0.0, 0.0], (0.5, 0.5), 0.25);
    let handle = adapter
        .extrude_face(face, [0.0, 0.0, 1.0], 2.0)
        .expect("circle extrude succeeds (PR-KV5b)");

    assert_eq!(adapter.list_faces(&handle).len(), 3, "two caps + lateral");
    assert_eq!(adapter.list_edges(&handle).len(), 3, "two rims + seam");
    assert_eq!(adapter.list_vertices(&handle).len(), 2, "seam vertices");

    let mesh = adapter.tessellate(&handle, 0.001).expect("tessellates");
    assert!(!mesh.indices.is_empty());
    let vol = render_mesh_volume(&mesh);
    let exact = std::f64::consts::PI * 0.25 * 0.25 * 2.0;
    assert!(
        (vol - exact).abs() <= 2e-3 * exact,
        "cylinder volume {vol} vs analytic {exact}"
    );

    let edges = adapter.extract_edges(&handle, 0.001).expect("edges");
    assert_eq!(edges.edge_ranges.len(), 3, "two rim polylines + one seam");
}

/// PR-KV5b: cylinder ∪ box through the legacy boolean trait (the
/// yang-proven yr8 configuration). Volume = box + the cylinder part
/// outside it, within the documented yang Stage-1 rim faceting band
/// (see kernel-v2 tests/kv5b_curved_boolean.rs module docs).
#[test]
fn boolean_union_cylinder_box_through_legacy_trait() {
    let mut adapter = KernelV2Adapter::new();
    let cyl_face = stage_circle(&mut adapter, [0.0, 0.0, -0.5], (0.5, 0.5), 0.25);
    let cyl = adapter
        .extrude_face(cyl_face, [0.0, 0.0, 1.0], 2.0)
        .expect("cylinder extrude");
    let box_face = stage_unit_square(&mut adapter);
    let bx = adapter
        .extrude_face(box_face, [0.0, 0.0, 1.0], 1.0)
        .expect("box extrude");

    let out = adapter
        .boolean_union(&cyl, &bx)
        .expect("cylinder ∪ box succeeds (PR-KV5b)");
    let mesh = adapter.tessellate(&out, 0.001).expect("tessellates");
    let vol = render_mesh_volume(&mesh);
    let cyl_term = std::f64::consts::PI * 0.25 * 0.25 * 1.0;
    let exact = 1.0 + cyl_term;
    assert!(
        (vol - exact).abs() <= 0.12 * cyl_term,
        "union volume {vol} vs analytic {exact}"
    );
}

/// Stage a `side × side` square at `(dx, dy)` (the unit-square stager,
/// parameterized) — for building two overlapping boxes.
fn stage_square(adapter: &mut KernelV2Adapter, dx: f64, dy: f64, side: f64) -> KernelId {
    let profile = ClosedProfile {
        entity_ids: vec![0, 1, 2, 3],
        is_outer: true,
        vertex_ids: vec![0, 1, 2, 3],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let positions: HashMap<u32, (f64, f64)> = [
        (0, (dx, dy)),
        (1, (dx + side, dy)),
        (2, (dx + side, dy + side)),
        (3, (dx, dy + side)),
    ]
    .into_iter()
    .collect();
    adapter
        .make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("square stages")[0]
}

#[test]
fn face_provenance_exposes_lineage_through_trait() {
    // KV13 F5: the KernelIntrospect contract surfaces a face's persistent
    // id + lineage root, walked through the boolean journal.
    let mut adapter = KernelV2Adapter::new();
    // A plain extrude: every face is its own lineage root.
    let a_face = stage_square(&mut adapter, 0.0, 0.0, 2.0);
    let a = adapter
        .extrude_face(a_face, [0.0, 0.0, 1.0], 2.0)
        .expect("box A");
    let mut operand_pids = std::collections::HashSet::new();
    for f in adapter.list_faces(&a) {
        let p = adapter.face_provenance(f).expect("box face has provenance");
        assert_eq!(
            p.pid, p.root_pid,
            "a constructor face is its own lineage root"
        );
        operand_pids.insert(p.pid);
    }
    let b_face = stage_square(&mut adapter, 1.0, 1.0, 2.0);
    let b = adapter
        .extrude_face(b_face, [0.0, 0.0, 1.0], 2.0)
        .expect("box B");
    for f in adapter.list_faces(&b) {
        operand_pids.insert(adapter.face_provenance(f).expect("prov").pid);
    }

    let out = adapter.boolean_union(&a, &b).expect("union");
    for f in adapter.list_faces(&out) {
        let p = adapter
            .face_provenance(f)
            .expect("union face has provenance");
        // The output face's OWN pid is fresh (not an operand's)...
        assert!(
            !operand_pids.contains(&p.pid),
            "output pid {} is fresh",
            p.pid
        );
        // ...but its lineage ROOT is one of the original boxes' faces —
        // resolving (at the Pid level) to the original extrude, not the
        // boolean.
        assert!(
            operand_pids.contains(&p.root_pid),
            "root_pid {} traces to an original box face",
            p.root_pid
        );
    }
}

#[test]
fn boolean_subtract_offset_boxes() {
    // Tool overlaps blank's corner region but NO coplanar face pairs:
    // every tool face plane is strictly inside or outside the blank.
    let mut adapter = KernelV2Adapter::new();
    let blank_face = stage_unit_square(&mut adapter);
    let blank = adapter
        .extrude_face(blank_face, [0.0, 0.0, 1.0], 1.0)
        .unwrap();

    // Tool: square at (0.4..1.4)², z from -0.3 to 0.6 — offset on all axes.
    let profile = ClosedProfile {
        entity_ids: vec![0, 1, 2, 3],
        is_outer: true,
        vertex_ids: vec![0, 1, 2, 3],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let positions: HashMap<u32, (f64, f64)> = [
        (0, (0.4, 0.4)),
        (1, (1.4, 0.4)),
        (2, (1.4, 1.4)),
        (3, (0.4, 1.4)),
    ]
    .into_iter()
    .collect();
    let tool_face = adapter
        .make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, -0.3],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .unwrap()[0];
    let tool = adapter
        .extrude_face(tool_face, [0.0, 0.0, 1.0], 0.9)
        .unwrap();

    let result = adapter
        .boolean_subtract(&blank, &tool)
        .expect("offset-box subtract succeeds");
    let mesh = adapter.tessellate(&result, 0.1).expect("tessellates");
    assert!(!mesh.indices.is_empty());

    // Volume check: blank 1.0 minus the overlap (0.6·0.6·0.6 = 0.216).
    let mut vol = 0.0f64;
    for t in mesh.indices.chunks(3) {
        let p = |i: u32| {
            let i = i as usize * 3;
            [
                mesh.vertices[i] as f64,
                mesh.vertices[i + 1] as f64,
                mesh.vertices[i + 2] as f64,
            ]
        };
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        vol += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    assert!(
        (vol - (1.0 - 0.216)).abs() < 1e-6,
        "subtract volume {vol}, expected 0.784"
    );
}
