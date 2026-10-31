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

/// `KernelIntrospect::edge_polyline` is CURVE-AWARE: a cylinder's rim circle
/// (one seam vertex) samples to a closed `N + 1`-point chord polyline at the
/// canonical render density, and the straight seam is its two endpoints. The
/// engine's share-a-face footprint reads faces through this accessor — with
/// vertices alone a circular cap had ONE point and a cut sketched on it found
/// no target body.
#[test]
fn edge_polyline_samples_a_circle_rim_as_a_closed_chord_polyline() {
    let mut adapter = KernelV2Adapter::new();
    let face = stage_circle(&mut adapter, [0.0, 0.0, 0.0], (0.0, 0.0), 0.06);
    let handle = adapter
        .extrude_face(face, [0.0, 0.0, 1.0], 0.01)
        .expect("cylinder");

    let n_seg =
        crate::tessellate::circle_segment_count(crate::tessellate::RENDER_CHORD_TOLERANCE_REL);
    let mut closed_rims = 0;
    let mut straight = 0;
    for edge in adapter.list_edges(&handle) {
        let pl = adapter.edge_polyline(edge);
        if pl.len() == 2 {
            straight += 1;
            continue;
        }
        assert_eq!(
            pl.len(),
            n_seg as usize + 1,
            "rim polyline at render density"
        );
        assert_eq!(
            pl.first(),
            pl.last(),
            "a closed circle edge closes its polyline"
        );
        for p in &pl {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!((r - 0.06).abs() < 1e-12, "sample on the rim: r = {r}");
        }
        closed_rims += 1;
    }
    assert_eq!((closed_rims, straight), (2, 1), "two rims + the seam");
    assert!(
        adapter.edge_polyline(KernelId(0)).is_empty(),
        "a non-edge id is empty"
    );
}

// ── Connector frame resolver: analytic axes out of the contract ─────────

/// `entity_axis` reports the arena's stored analytic axis: the cylinder's
/// for the lateral face, each rim circle's for the rim edges, and `None`
/// for the planar caps and the straight seam edge (which have no axis).
///
/// The staged circle is centred at (u, v) = (0.5, 0.5) on the z = 0 plane
/// with x̂ = +X, so the axis runs through (0.5, 0.5, ·) along +Z.
#[test]
fn entity_axis_reports_the_cylinder_axis_and_the_rim_circles() {
    use waffle_types::kernel::AxisKind;

    let mut adapter = KernelV2Adapter::new();
    let face = stage_circle(&mut adapter, [0.0, 0.0, 0.0], (0.5, 0.5), 0.25);
    let handle = adapter
        .extrude_face(face, [0.0, 0.0, 1.0], 2.0)
        .expect("circle extrude succeeds");

    let axes: Vec<_> = adapter
        .list_faces(&handle)
        .into_iter()
        .filter_map(|f| adapter.entity_axis(f, TopoKind::Face))
        .collect();
    assert_eq!(
        axes.len(),
        1,
        "only the lateral face has an axis (caps do not)"
    );
    let ax = axes[0];
    assert_eq!(ax.kind, AxisKind::Cylindrical);
    assert_eq!(ax.radius, Some(0.25));
    assert!(
        ax.direction[2].abs() > 1.0 - 1e-12 && ax.direction[0].abs() < 1e-12,
        "axis along ±Z, got {:?}",
        ax.direction
    );
    // The reference point is ON the axis: its (x, y) is the circle centre,
    // whatever height the constructor anchored it at.
    assert!(
        (ax.origin[0] - 0.5).abs() < 1e-12 && (ax.origin[1] - 0.5).abs() < 1e-12,
        "axis point through the circle centre, got {:?}",
        ax.origin
    );

    let edge_axes: Vec<_> = adapter
        .list_edges(&handle)
        .into_iter()
        .filter_map(|e| adapter.entity_axis(e, TopoKind::Edge))
        .collect();
    assert_eq!(edge_axes.len(), 2, "two rim circles; the seam is straight");
    let mut heights: Vec<f64> = Vec::new();
    for a in &edge_axes {
        assert_eq!(a.kind, AxisKind::Circular);
        assert_eq!(a.radius, Some(0.25));
        assert!(
            (a.origin[0] - 0.5).abs() < 1e-12 && (a.origin[1] - 0.5).abs() < 1e-12,
            "rim centre on the axis, got {:?}",
            a.origin
        );
        assert!(a.direction[2].abs() > 1.0 - 1e-12, "rim normal along ±Z");
        heights.push(a.origin[2]);
    }
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(
        (heights[0] - 0.0).abs() < 1e-12 && (heights[1] - 2.0).abs() < 1e-12,
        "rims at the extrude's two ends, got {heights:?}"
    );
}

/// A planar face and a straight edge have no axis, and neither does an id
/// of the wrong kind or an imported (mesh-backed) body's face — the
/// importer keeps no axis parameters, and a fit would be a guess.
#[test]
fn entity_axis_is_none_without_analytic_geometry() {
    let mut adapter = KernelV2Adapter::new();
    let face = stage_unit_square(&mut adapter);
    let handle = adapter
        .extrude_face(face, [0.0, 0.0, 1.0], 1.0)
        .expect("box extrude");
    for f in adapter.list_faces(&handle) {
        assert!(adapter.entity_axis(f, TopoKind::Face).is_none(), "planar");
    }
    for e in adapter.list_edges(&handle) {
        assert!(adapter.entity_axis(e, TopoKind::Edge).is_none(), "straight");
    }
    let a_face = adapter.list_faces(&handle)[0];
    assert!(
        adapter.entity_axis(a_face, TopoKind::Edge).is_none(),
        "a face id asked for as an edge is None, not a panic"
    );

    let imported = adapter.import_body(&imported_test_data()).expect("import");
    for f in adapter.list_faces(&imported) {
        assert!(
            adapter.entity_axis(f, TopoKind::Face).is_none(),
            "an imported face carries a mesh, not an axis"
        );
    }
}

/// SI5 C6: an exact analytic shell enters through the trait and is an
/// ordinary arena solid — everything the mesh-backed tier refuses
/// (exact volume, analytic STEP export, a boolean) works, `solid_is_exact`
/// says so, and the render mesh is the kernel's own.
#[test]
fn an_exact_analytic_import_is_a_first_class_arena_solid() {
    use crate::ingest::fixtures;
    let mut adapter = KernelV2Adapter::new();
    let (rc, h, rho) = (3.0, 5.0, 0.75);
    let shell = fixtures::quarter_puck(rc, h, rho);
    let handle = adapter
        .import_analytic_shell(&shell)
        .expect("the quarter puck ingests");
    assert!(adapter.solid_is_exact(&handle));

    // Exact volume: the quarter of the rounded puck, in closed form.
    let got = adapter.solid_volume(&handle).expect("exact volume");
    let big = rc - rho;
    let want = (std::f64::consts::PI * rc * rc * (h - rho)
        + std::f64::consts::PI * big * big * rho
        + std::f64::consts::PI * std::f64::consts::PI * rho * rho * big / 2.0
        + 2.0 * std::f64::consts::PI * rho * rho * rho / 3.0)
        / 4.0;
    assert!(
        ((got - want) / want).abs() < 1e-12,
        "volume {got:.17e} vs {want:.17e}"
    );

    // Render + edges come from the kernel's own tessellator (face ranges
    // keyed by arena face ids, not imported slots).
    let mesh = adapter.tessellate(&handle, 0.001).expect("tessellates");
    assert_eq!(mesh.face_ranges.len(), shell.faces.len());
    assert!(adapter.extract_edges(&handle, 0.001).is_ok());

    // Analytic STEP export — the mesh tier's typed refusal does not apply.
    let step = adapter
        .export_step(&handle, "quarter_puck.step")
        .expect("exports analytically");
    assert!(
        step.contains("TOROIDAL_SURFACE"),
        "the fillet is written exactly"
    );

    // A boolean operand (the SI1 wall is for mesh-backed bodies only): the
    // unit box, translated to poke out of the puck's flat bottom, unioned
    // in. Exact volume: the puck's quarter plus the half of the box that
    // lies outside it.
    let face = stage_unit_square(&mut adapter);
    let unit = adapter
        .extrude_face(face, [0.0, 0.0, 1.0], 1.0)
        .expect("box extrudes");
    let block = adapter
        .transform_body(
            &unit,
            &waffle_types::kernel::RigidPlacement {
                rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                translation: [0.5, 0.5, -0.5],
            },
        )
        .expect("rigid move");
    let union = adapter
        .boolean_union(&handle, &block)
        .expect("exact import is an operand");
    let v_union = adapter
        .solid_volume(&union)
        .expect("exact volume of the result");
    let want_union = want + 0.5;
    assert!(
        ((v_union - want_union) / want_union).abs() < 1e-9,
        "union volume {v_union:.17e} vs {want_union:.17e}"
    );
}

/// The mesh-backed tier still answers `false` — the exporter's "left out"
/// warning keys on this.
#[test]
fn a_mesh_backed_import_is_not_exact() {
    let mut adapter = KernelV2Adapter::new();
    let handle = adapter.import_body(&imported_test_data()).expect("import");
    assert!(!adapter.solid_is_exact(&handle));
}

// ── Q11: non-finite caller geometry is refused at the adapter boundary ──
//
// 2026-10-28 review §1.4: a NaN that reaches a cherchi-rs predicate reads as
// sign `Zero` and then panics converting to a rational — in WASM that kills
// the engine. The fix is at the DOOR: every entry that takes caller geometry
// answers a typed `KernelError::InvalidInput` naming the argument, before
// any arena work.

/// `r` is the typed refusal, and it names `argument`.
#[track_caller]
fn assert_refused_as_non_finite<T: std::fmt::Debug>(r: Result<T, KernelError>, argument: &str) {
    match r {
        Err(KernelError::InvalidInput {
            argument: got,
            reason,
        }) => {
            assert!(
                got.contains(argument),
                "the refusal must name `{argument}`, named `{got}` ({reason})"
            );
            assert!(reason.contains("finite"), "{reason}");
        }
        other => panic!("expected InvalidInput naming `{argument}`, got {other:?}"),
    }
}

#[test]
fn a_non_finite_extrude_distance_or_direction_is_a_typed_refusal() {
    let mut adapter = KernelV2Adapter::new();
    let face = stage_unit_square(&mut adapter);
    for depth in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_refused_as_non_finite(adapter.extrude_face(face, [0.0, 0.0, 1.0], depth), "depth");
    }
    assert_refused_as_non_finite(
        adapter.extrude_face(face, [0.0, f64::NAN, 1.0], 1.0),
        "direction",
    );
    // Refused before any work: the staged profile is still there, so the
    // same face extrudes once the caller sends a finite depth.
    let solid = adapter
        .extrude_face(face, [0.0, 0.0, 1.0], 1.0)
        .expect("the refusal consumed nothing");
    let v = adapter.solid_volume(&solid).expect("volume");
    assert!((v - 1.0).abs() < 1e-12, "unit cube, got {v}");
}

#[test]
fn a_non_finite_profile_point_is_a_typed_refusal() {
    let mut adapter = KernelV2Adapter::new();
    let square = |p: (f64, f64)| -> HashMap<u32, (f64, f64)> {
        [(0, (0.0, 0.0)), (1, (1.0, 0.0)), (2, p), (3, (0.0, 1.0))]
            .into_iter()
            .collect()
    };
    let profile = ClosedProfile {
        entity_ids: vec![0, 1, 2, 3],
        is_outer: true,
        vertex_ids: vec![0, 1, 2, 3],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    };
    for bad in [(f64::NAN, 1.0), (1.0, f64::INFINITY)] {
        assert_refused_as_non_finite(
            adapter.make_faces_from_profiles(
                std::slice::from_ref(&profile),
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &square(bad),
            ),
            "positions[2]",
        );
    }
    // The plane frame is caller geometry too.
    assert_refused_as_non_finite(
        adapter.make_faces_from_profiles(
            std::slice::from_ref(&profile),
            [f64::NAN, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &square((1.0, 1.0)),
        ),
        "plane_origin",
    );
    // A circle profile's radius.
    let circle = ClosedProfile {
        entity_ids: vec![7],
        is_outer: true,
        vertex_ids: vec![],
        circle: Some(waffle_types::kernel::CircleProfile {
            center_u: 0.0,
            center_v: 0.0,
            radius: f64::NAN,
        }),
        spline_segments: vec![],
        arc_segments: vec![],
    };
    assert_refused_as_non_finite(
        adapter.make_faces_from_profiles(
            &[circle],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &HashMap::new(),
        ),
        "circle.radius",
    );
}

#[test]
fn a_non_finite_transform_or_mirror_plane_is_a_typed_refusal() {
    let mut adapter = KernelV2Adapter::new();
    let face = stage_unit_square(&mut adapter);
    let solid = adapter
        .extrude_face(face, [0.0, 0.0, 1.0], 1.0)
        .expect("box");
    let mut placement = waffle_types::kernel::RigidPlacement::IDENTITY;
    placement.translation[1] = f64::NAN;
    assert_refused_as_non_finite(adapter.transform_body(&solid, &placement), "translation");
    let mut placement = waffle_types::kernel::RigidPlacement::IDENTITY;
    placement.rotation[2][0] = f64::INFINITY;
    assert_refused_as_non_finite(adapter.transform_body(&solid, &placement), "rotation[2]");
    assert_refused_as_non_finite(
        adapter.mirror_body(
            &solid,
            &waffle_types::kernel::MirrorPlane {
                point: [0.0, 0.0, 0.0],
                normal: [f64::NAN, 0.0, 0.0],
            },
        ),
        "normal",
    );
}

#[test]
fn every_other_geometry_door_refuses_a_nan_by_name() {
    use waffle_types::kernel::KernelProjection;
    let mut adapter = KernelV2Adapter::new();
    let face = stage_unit_square(&mut adapter);
    let solid = adapter
        .extrude_face(face, [0.0, 0.0, 1.0], 1.0)
        .expect("box");

    let face = stage_unit_square(&mut adapter);
    assert_refused_as_non_finite(
        adapter.revolve_face(face, [5.0, 0.0, 0.0], [0.0, 1.0, 0.0], f64::NAN),
        "angle",
    );
    assert_refused_as_non_finite(
        adapter.section_with_plane(&solid, [0.0, 0.0, f64::NAN], [0.0, 0.0, 1.0]),
        "plane_origin",
    );
    assert_refused_as_non_finite(
        adapter.pipe(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &[PipePathSegment::Line {
                a: (0.0, 0.0),
                b: (1.0, 0.0),
            }],
            f64::NAN,
            None,
        ),
        "radius",
    );
    assert_refused_as_non_finite(
        adapter.distance(
            &MeasureEntity::Solid(solid.clone()),
            &MeasureEntity::Point([f64::NAN, 0.0, 0.0]),
            &DistanceOpts::default(),
        ),
        "point",
    );
    assert_refused_as_non_finite(
        adapter.mass_properties(&solid, Some(f64::INFINITY)),
        "density",
    );
    assert_refused_as_non_finite(
        adapter.project(
            &solid,
            &ViewFrame {
                origin: [0.0, 0.0, 0.0],
                dir: [0.0, 0.0, f64::NAN],
                up: [0.0, 1.0, 0.0],
            },
            &ProjectOpts::default(),
        ),
        "dir",
    );
}

// ── Q7: a section's cut body is released, and the arena rewinds ──────────
//
// 2026-10-28 review §3.3: `section_with_plane` copies the cut body into the
// live arena and a drawing re-cuts on every evaluate, so without a release
// the arena and the pid space grew by one body per evaluate.

/// A unit cube from the staged unit square.
fn unit_cube(adapter: &mut KernelV2Adapter) -> KernelSolidHandle {
    use waffle_types::kernel::Kernel;
    let face = stage_unit_square(adapter);
    adapter
        .extrude_face(face, [0.0, 0.0, 1.0], 1.0)
        .expect("unit cube")
}

#[test]
fn releasing_section_cuts_in_reverse_rewinds_the_arena_exactly() {
    use waffle_types::kernel::KernelProjection;
    let mut adapter = KernelV2Adapter::new();
    let cube = unit_cube(&mut adapter);
    let before = adapter.arena.clone();
    let handles_before = adapter.solids.len();

    let lower = adapter
        .section_with_plane(&cube, [0.0, 0.0, 0.5], [0.0, 0.0, 1.0])
        .expect("lower half")
        .cut_solid
        .expect("material kept");
    let upper = adapter
        .section_with_plane(&cube, [0.0, 0.0, 0.5], [0.0, 0.0, -1.0])
        .expect("upper half")
        .cut_solid
        .expect("material kept");
    assert_ne!(adapter.arena, before, "a cut is a real body while it lives");

    adapter.release_section_cut(&upper).expect("release upper");
    adapter.release_section_cut(&lower).expect("release lower");
    // EQUAL, not merely the same size: no slot, no pid, no journal entry and
    // no face-pid entry is left behind.
    assert_eq!(adapter.arena, before);
    assert_eq!(adapter.solids.len(), handles_before);
    assert!(
        adapter.solid_volume(&lower).is_err(),
        "a released cut is dead"
    );
    // A second release of the same handle is refused, not a double free.
    assert!(adapter.release_section_cut(&lower).is_err());
}

#[test]
fn the_same_section_cut_twice_mints_the_same_pids() {
    use waffle_types::kernel::{KernelProjection, TopoKind};
    // A rewound arena re-cuts into the same pid range — which is what makes a
    // section view's cache key and its annotation anchors stable across
    // evaluates rather than drifting by one body's pids every time.
    let mut adapter = KernelV2Adapter::new();
    let cube = unit_cube(&mut adapter);
    let mut cut_pids = || {
        let cut = adapter
            .section_with_plane(&cube, [0.0, 0.0, 0.5], [0.0, 0.0, 1.0])
            .expect("cut")
            .cut_solid
            .expect("material kept");
        let pids: Vec<u64> = adapter
            .all_entity_pids(&cut, TopoKind::Face)
            .into_iter()
            .map(|(_, p)| p.pid)
            .collect();
        adapter.release_section_cut(&cut).expect("release");
        pids
    };
    let first = cut_pids();
    assert!(!first.is_empty());
    assert_eq!(cut_pids(), first);
}

#[test]
fn a_cut_released_out_of_order_is_killed_and_the_other_survives() {
    use waffle_types::kernel::KernelProjection;
    let mut adapter = KernelV2Adapter::new();
    let cube = unit_cube(&mut adapter);
    let live_before = adapter.arena.solids.iter().flatten().count();
    let faces_before = adapter.arena.num_faces();

    let lower = adapter
        .section_with_plane(&cube, [0.0, 0.0, 0.5], [0.0, 0.0, 1.0])
        .expect("lower")
        .cut_solid
        .expect("kept");
    let upper = adapter
        .section_with_plane(&cube, [0.0, 0.0, 0.5], [0.0, 0.0, -1.0])
        .expect("upper")
        .cut_solid
        .expect("kept");

    // Out of order: the LOWER cut is not the arena's tail, so it is killed
    // in place and the upper one is untouched.
    adapter.release_section_cut(&lower).expect("release lower");
    assert!(adapter.solid_volume(&lower).is_err());
    let v = adapter.solid_volume(&upper).expect("upper still lives");
    assert!(
        (v - 0.5).abs() < 1e-12,
        "upper half of a unit cube, got {v}"
    );

    adapter.release_section_cut(&upper).expect("release upper");
    assert_eq!(adapter.arena.solids.iter().flatten().count(), live_before);
    assert_eq!(adapter.arena.num_faces(), faces_before);
}

#[test]
fn a_cut_that_keeps_nothing_or_everything_leaves_the_arena_untouched() {
    use waffle_types::kernel::KernelProjection;
    let mut adapter = KernelV2Adapter::new();
    let cube = unit_cube(&mut adapter);
    let before = adapter.arena.clone();

    // Every point on the discarded side: no material, no body.
    let none = adapter
        .section_with_plane(&cube, [0.0, 0.0, -1.0], [0.0, 0.0, 1.0])
        .expect("a miss is typed");
    assert!(none.cut_solid.is_none());
    assert_eq!(adapter.arena, before);

    // Every point on the kept side: the INPUT comes back, and it is the
    // caller's model — never a releasable cut.
    let all = adapter
        .section_with_plane(&cube, [0.0, 0.0, 2.0], [0.0, 0.0, 1.0])
        .expect("a miss is typed");
    assert_eq!(all.cut_solid.as_ref().map(|h| h.raw()), Some(cube.raw()));
    assert_eq!(adapter.arena, before);
    assert!(adapter.release_section_cut(&cube).is_err());
    assert!(adapter.solid_volume(&cube).is_ok());
}

// ---- surface_conformance (remediation plan O1) ---------------------------

fn worst_deviation(adapter: &KernelV2Adapter, h: &KernelSolidHandle) -> f64 {
    adapter
        .surface_conformance(h)
        .expect("conformance measures an exact solid")
        .worst()
        .map(|f| f.max_distance)
        .expect("a solid has faces")
}

/// D-shape extrude (two cylinder patches bounded by ARC edges) — shared by
/// the conformance tests below.
fn d_shape(adapter: &mut KernelV2Adapter) -> KernelSolidHandle {
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
    adapter
        .extrude_face(ids[0], [0.0, 0.0, 1.0], 2.0)
        .expect("D-shape extrudes")
}

#[test]
fn constructed_and_boolean_solids_lie_on_their_surfaces() {
    let mut adapter = KernelV2Adapter::new();
    let cube = unit_cube(&mut adapter);
    let cyl_face = stage_circle(&mut adapter, [0.0, 0.0, 0.0], (0.5, 0.5), 0.25);
    let cyl = adapter
        .extrude_face(cyl_face, [0.0, 0.0, 1.0], 2.0)
        .expect("cylinder");
    let d = d_shape(&mut adapter);
    for (name, h) in [("cube", &cube), ("cylinder", &cyl), ("D-shape", &d)] {
        let c = adapter.surface_conformance(h).expect("measures");
        assert!(
            c.faces.iter().all(|f| f.samples > 0),
            "{name}: every face sampled"
        );
        let w = worst_deviation(&adapter, h);
        assert!(
            w <= 1e-12,
            "{name}: constructed solid off its surfaces by {w:e}"
        );
    }

    // A yang boolean output (intersection curves on both operands' surfaces).
    let mut adapter = KernelV2Adapter::new();
    let cyl_face = stage_circle(&mut adapter, [0.0, 0.0, -0.5], (0.5, 0.5), 0.25);
    let cyl = adapter
        .extrude_face(cyl_face, [0.0, 0.0, 1.0], 2.0)
        .expect("cylinder");
    let box_face = stage_unit_square(&mut adapter);
    let bx = adapter
        .extrude_face(box_face, [0.0, 0.0, 1.0], 1.0)
        .expect("box");
    let out = adapter.boolean_union(&cyl, &bx).expect("union");
    let w = worst_deviation(&adapter, &out);
    assert!(w <= 1e-9, "cylinder ∪ box off its surfaces by {w:e}");
}

#[test]
fn a_vertex_moved_off_its_surface_is_measured() {
    let mut adapter = KernelV2Adapter::new();
    let d = d_shape(&mut adapter);
    // Push the rim vertex at (1, 0, 0) radially outward by 1e-4. Moving it
    // along +x keeps it on the cap plane z = 0 and on the flat side y = 0,
    // so the ONLY surface it leaves is the cylinder, by exactly 1e-4.
    let target = adapter
        .arena
        .vertices
        .iter()
        .position(|v| {
            v.as_ref().is_some_and(|v| {
                (v.point.x() - 1.0).abs() < 1e-12
                    && v.point.y().abs() < 1e-12
                    && v.point.z().abs() < 1e-12
            })
        })
        .expect("the D-shape has a rim vertex at (1, 0, 0)");
    adapter.arena.vertices[target].as_mut().unwrap().point =
        cad_primitives::Point3::new(1.0 + 1e-4, 0.0, 0.0);
    let w = worst_deviation(&adapter, &d);
    assert!(
        (w - 1e-4).abs() < 1e-9,
        "the moved vertex must read 1e-4 off the cylinder, got {w:e}"
    );
}

#[test]
fn an_arc_shipped_as_a_chord_is_measured_at_its_middle() {
    // Ledger N100 item 3's shape: both ends on the cylinder, the edge
    // between them a straight chord. A vertex-only check reads zero.
    let mut adapter = KernelV2Adapter::new();
    let d = d_shape(&mut adapter);
    let arc = adapter
        .arena
        .half_edges
        .iter()
        .position(|h| {
            h.as_ref()
                .is_some_and(|h| matches!(h.curve, crate::arena::Curve::Arc { .. }))
        })
        .expect("the D-shape has arc edges");
    let twin = adapter.arena.half_edges[arc].as_ref().unwrap().twin;
    for h in [arc, twin.0 as usize] {
        adapter.arena.half_edges[h].as_mut().unwrap().curve = crate::arena::Curve::LineSegment;
    }
    let w = worst_deviation(&adapter, &d);
    // Each sub-arc spans 90°: the chord's middle sits 1 − cos 45° ≈ 0.293
    // inside the unit cylinder; seven interior samples include it exactly.
    let sagitta = 1.0 - std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        w > 0.9 * sagitta,
        "a chord standing in for an arc must read ~{sagitta:.3} off, got {w:e}"
    );
}

// ---- mesh_conformance (remediation plan O3) ------------------------------

#[test]
fn constructed_render_meshes_sit_on_their_surfaces() {
    let rel = crate::tessellate::RENDER_CHORD_TOLERANCE_REL;
    let mut adapter = KernelV2Adapter::new();
    let cube_face = stage_unit_square(&mut adapter);
    let cube = adapter
        .extrude_face(cube_face, [0.0, 0.0, 1.0], 1.0)
        .expect("cube");
    let cyl_face = stage_circle(&mut adapter, [0.0, 0.0, -0.5], (0.5, 0.5), 0.25);
    let cyl = adapter
        .extrude_face(cyl_face, [0.0, 0.0, 1.0], 2.0)
        .expect("cylinder");
    let d = d_shape(&mut adapter);
    // Largest radius of curvature on each solid: the centroid bound is the
    // render sagitta band d_ε = rel · r.
    for (name, h, r) in [
        ("cube", &cube, 0.0),
        ("cylinder", &cyl, 0.25),
        ("D-shape", &d, 1.0),
    ] {
        let c = adapter.mesh_conformance(h).expect("measures");
        assert!(
            c.faces.iter().all(|f| f.triangles > 0),
            "{name}: every face rendered"
        );
        for f in &c.faces {
            assert!(
                f.max_vertex_distance <= 1e-12,
                "{name}: render vertex {:e} off its surface",
                f.max_vertex_distance
            );
            assert!(
                f.max_centroid_distance <= rel * r + 1e-12,
                "{name}: centroid {:e} off, beyond the sagitta band {:e}",
                f.max_centroid_distance,
                rel * r
            );
        }
    }

    // A yang boolean output.
    let out = adapter.boolean_union(&cyl, &cube).expect("union");
    let c = adapter.mesh_conformance(&out).expect("measures");
    for f in &c.faces {
        assert!(f.max_vertex_distance <= 1e-9, "union vertex {f:?}");
        assert!(
            f.max_centroid_distance <= rel * 0.25 + 1e-9,
            "union centroid {f:?}"
        );
    }
}

#[test]
fn a_triangle_spanning_its_face_is_measured_at_its_centroid() {
    // Every vertex stays ON the cylinder; one triangle is re-pointed across
    // the face so it cuts through the solid. Vertices read zero, the
    // centroid reads the defect — the case the boundary oracle (O1) cannot
    // see.
    let mut adapter = KernelV2Adapter::new();
    let cyl_face = stage_circle(&mut adapter, [0.0, 0.0, 0.0], (0.0, 0.0), 1.0);
    let cyl = adapter
        .extrude_face(cyl_face, [0.0, 0.0, 1.0], 1.0)
        .expect("cylinder");
    let sid = adapter.solid_of(&cyl).unwrap();
    let mut mesh = crate::tessellate(&adapter.arena, sid).unwrap();
    let lateral = *mesh
        .face_ranges
        .iter()
        .find(|r| {
            matches!(
                adapter.arena.face(r.face).unwrap().surface,
                Some(crate::arena::Surface::Cylinder { .. })
            )
        })
        .expect("a lateral face");
    let clean = crate::conformance::mesh_conformance_of(&adapter.arena, &mesh).unwrap();
    // The vertex of the range farthest from the first triangle's first vertex
    // is (near) diametrically opposite it.
    let s = lateral.start as usize;
    let at = |i: u32| {
        let k = 3 * i as usize;
        [mesh.positions[k], mesh.positions[k + 1]]
    };
    let a = at(mesh.indices[s]);
    let far = mesh.indices[s..s + lateral.count as usize]
        .iter()
        .copied()
        .max_by(|&i, &j| {
            let (p, q) = (at(i), at(j));
            let dp = (p[0] - a[0]).hypot(p[1] - a[1]);
            let dq = (q[0] - a[0]).hypot(q[1] - a[1]);
            dp.total_cmp(&dq)
        })
        .unwrap();
    mesh.indices[s + 1] = far;
    let bad = crate::conformance::mesh_conformance_of(&adapter.arena, &mesh).unwrap();
    let pick = |v: &[crate::conformance::FaceMeshDeviation]| {
        *v.iter().find(|d| d.face == lateral.face).unwrap()
    };
    let (clean, bad) = (pick(&clean), pick(&bad));
    assert!(
        bad.max_vertex_distance <= 1e-12,
        "vertices stay on the surface"
    );
    assert!(
        bad.max_centroid_distance > 100.0 * clean.max_centroid_distance
            && bad.max_centroid_distance > 0.1,
        "a triangle cutting through the unit cylinder reads its centroid off: \
         clean {:e}, defect {:e}",
        clean.max_centroid_distance,
        bad.max_centroid_distance
    );
}
