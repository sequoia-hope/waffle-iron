//! Helper functions: error types, GeomRef constructors, profile builders, mesh math.

use std::collections::HashMap;

use kernel_fork::types::RenderMesh;
use uuid::Uuid;
use waffle_types::Role;
use waffle_types::*;

// ── Error Type ──────────────────────────────────────────────────────────────

/// Unified error type for the test harness.
#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error("feature not found: {name}")]
    FeatureNotFound { name: String },

    #[error("dispatch error: {message}")]
    DispatchError { message: String },

    #[error("no solid for feature: {name}")]
    NoSolid { name: String },

    #[error("assertion failed: {detail}")]
    AssertionFailed { detail: String },

    #[error("oracle failure ({oracle}): {detail}")]
    OracleFailure { oracle: String, detail: String },

    #[error("STL error: {reason}")]
    StlError { reason: String },

    #[error("engine error: {0}")]
    Engine(String),

    #[error("duplicate name: {name}")]
    DuplicateName { name: String },
}

// ── GeomRef Constructors ────────────────────────────────────────────────────

/// Create a GeomRef for a datum plane (e.g. XY, XZ, YZ).
pub fn datum_plane_ref(datum_id: Uuid) -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum { datum_id },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    }
}

/// Create a GeomRef for a face on a feature output, selected by role.
pub fn face_ref(feature_id: Uuid, role: Role, index: usize) -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role { role, index },
        policy: ResolvePolicy::Strict,
    }
}

/// Create a GeomRef for an edge using BestEffort resolution.
/// Useful when exact edge identity is unknown (e.g. for fillet targets).
pub fn edge_ref_best_effort(feature_id: Uuid) -> GeomRef {
    GeomRef {
        kind: TopoKind::Edge,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::FilletFace { index: 99 },
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
    }
}

/// Create a GeomRef for a body output of a feature (for boolean operations).
pub fn body_ref(feature_id: Uuid) -> GeomRef {
    GeomRef {
        kind: TopoKind::Solid,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
    }
}

// ── Profile Builders ────────────────────────────────────────────────────────

/// The result of building a sketch profile: entities, solved positions, and closed profiles.
pub type ProfileData = (
    Vec<SketchEntity>,
    HashMap<u32, (f64, f64)>,
    Vec<ClosedProfile>,
);

/// Build a rectangular sketch profile.
///
/// Returns (entities, solved_positions, profiles) ready for FinishSketch.
/// Points are numbered 1..=4, lines 10..=13.
pub fn rect_profile(x: f64, y: f64, w: f64, h: f64) -> ProfileData {
    let entities = vec![
        SketchEntity::Point {
            id: 1,
            x,
            y,
            construction: false,
        },
        SketchEntity::Point {
            id: 2,
            x: x + w,
            y,
            construction: false,
        },
        SketchEntity::Point {
            id: 3,
            x: x + w,
            y: y + h,
            construction: false,
        },
        SketchEntity::Point {
            id: 4,
            x,
            y: y + h,
            construction: false,
        },
        SketchEntity::Line {
            id: 10,
            start_id: 1,
            end_id: 2,
            construction: false,
        },
        SketchEntity::Line {
            id: 11,
            start_id: 2,
            end_id: 3,
            construction: false,
        },
        SketchEntity::Line {
            id: 12,
            start_id: 3,
            end_id: 4,
            construction: false,
        },
        SketchEntity::Line {
            id: 13,
            start_id: 4,
            end_id: 1,
            construction: false,
        },
    ];

    let mut positions = HashMap::new();
    positions.insert(1, (x, y));
    positions.insert(2, (x + w, y));
    positions.insert(3, (x + w, y + h));
    positions.insert(4, (x, y + h));

    let profiles = vec![ClosedProfile {
        entity_ids: vec![1, 2, 3, 4],
        is_outer: true,
    }];

    (entities, positions, profiles)
}

/// Build a circular sketch profile approximated as a polygon.
///
/// Returns (entities, solved_positions, profiles) ready for FinishSketch.
/// Center point ID = 1, perimeter points start at 2.
pub fn circle_profile(cx: f64, cy: f64, r: f64, segments: u32) -> ProfileData {
    let mut entities = Vec::new();
    let mut positions = HashMap::new();

    // Center point (construction — not part of profile boundary)
    entities.push(SketchEntity::Point {
        id: 1,
        x: cx,
        y: cy,
        construction: true,
    });
    positions.insert(1, (cx, cy));

    // Perimeter points
    let point_start = 2u32;
    for i in 0..segments {
        let angle = 2.0 * std::f64::consts::PI * (i as f64) / (segments as f64);
        let px = cx + r * angle.cos();
        let py = cy + r * angle.sin();
        let pid = point_start + i;
        entities.push(SketchEntity::Point {
            id: pid,
            x: px,
            y: py,
            construction: false,
        });
        positions.insert(pid, (px, py));
    }

    // Lines connecting perimeter points
    let line_start = 100u32;
    let mut entity_ids = Vec::new();
    for i in 0..segments {
        let lid = line_start + i;
        let start_id = point_start + i;
        let end_id = point_start + ((i + 1) % segments);
        entities.push(SketchEntity::Line {
            id: lid,
            start_id,
            end_id,
            construction: false,
        });
        entity_ids.push(point_start + i);
    }

    let profiles = vec![ClosedProfile {
        entity_ids,
        is_outer: true,
    }];

    (entities, positions, profiles)
}

// ── Mesh Math Utilities ─────────────────────────────────────────────────────

/// Compute axis-aligned bounding box of a RenderMesh. Returns (min, max).
pub fn mesh_bounding_box(mesh: &RenderMesh) -> ([f32; 3], [f32; 3]) {
    assert!(
        mesh.vertices.len() >= 3,
        "Mesh must have at least one vertex"
    );
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for chunk in mesh.vertices.chunks(3) {
        for i in 0..3 {
            min[i] = min[i].min(chunk[i]);
            max[i] = max[i].max(chunk[i]);
        }
    }
    (min, max)
}

/// Compute the signed volume of a triangle mesh using the divergence theorem.
///
/// For a closed (watertight) mesh, this returns the enclosed volume.
/// For open meshes, the result may be meaningless.
pub fn mesh_volume(mesh: &RenderMesh) -> f64 {
    let verts = &mesh.vertices;
    let indices = &mesh.indices;
    let mut volume = 0.0f64;

    for tri in indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let (i0, i1, i2) = (
            tri[0] as usize * 3,
            tri[1] as usize * 3,
            tri[2] as usize * 3,
        );
        if i0 + 2 >= verts.len() || i1 + 2 >= verts.len() || i2 + 2 >= verts.len() {
            continue;
        }

        let (x0, y0, z0) = (verts[i0] as f64, verts[i0 + 1] as f64, verts[i0 + 2] as f64);
        let (x1, y1, z1) = (verts[i1] as f64, verts[i1 + 1] as f64, verts[i1 + 2] as f64);
        let (x2, y2, z2) = (verts[i2] as f64, verts[i2 + 1] as f64, verts[i2 + 2] as f64);

        // Signed volume of tetrahedron formed by triangle and origin
        volume += x0 * (y1 * z2 - y2 * z1) + x1 * (y2 * z0 - y0 * z2) + x2 * (y0 * z1 - y1 * z0);
    }

    (volume / 6.0).abs()
}

/// Compute the total surface area of a triangle mesh.
pub fn mesh_surface_area(mesh: &RenderMesh) -> f64 {
    let verts = &mesh.vertices;
    let indices = &mesh.indices;
    let mut area = 0.0f64;

    for tri in indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let (i0, i1, i2) = (
            tri[0] as usize * 3,
            tri[1] as usize * 3,
            tri[2] as usize * 3,
        );
        if i0 + 2 >= verts.len() || i1 + 2 >= verts.len() || i2 + 2 >= verts.len() {
            continue;
        }

        let ax = verts[i1] as f64 - verts[i0] as f64;
        let ay = verts[i1 + 1] as f64 - verts[i0 + 1] as f64;
        let az = verts[i1 + 2] as f64 - verts[i0 + 2] as f64;
        let bx = verts[i2] as f64 - verts[i0] as f64;
        let by = verts[i2 + 1] as f64 - verts[i0 + 1] as f64;
        let bz = verts[i2 + 2] as f64 - verts[i0 + 2] as f64;

        // Cross product magnitude / 2
        let cx = ay * bz - az * by;
        let cy = az * bx - ax * bz;
        let cz = ax * by - ay * bx;
        area += (cx * cx + cy * cy + cz * cz).sqrt() / 2.0;
    }

    area
}

/// Count mesh edges: returns (total_edges, boundary_edges).
///
/// A boundary edge is shared by exactly 1 triangle (not 2).
/// For a watertight mesh, boundary_edges should be 0.
pub fn count_mesh_edges(mesh: &RenderMesh) -> (usize, usize) {
    use std::collections::HashMap as Map;

    let mut edge_counts: Map<(u32, u32), usize> = Map::new();

    for tri in mesh.indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let key = (a.min(b), a.max(b));
            *edge_counts.entry(key).or_insert(0) += 1;
        }
    }

    let total = edge_counts.len();
    let boundary = edge_counts.values().filter(|&&c| c == 1).count();
    (total, boundary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_profile_has_correct_entity_count() {
        let (entities, positions, profiles) = rect_profile(0.0, 0.0, 10.0, 10.0);
        assert_eq!(entities.len(), 8); // 4 points + 4 lines
        assert_eq!(positions.len(), 4);
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].is_outer);
    }

    #[test]
    fn circle_profile_has_correct_segments() {
        let (entities, positions, profiles) = circle_profile(0.0, 0.0, 5.0, 8);
        // 1 center + 8 perimeter points + 8 lines = 17 entities
        assert_eq!(entities.len(), 17);
        assert_eq!(positions.len(), 9); // center + 8 perimeter
        assert_eq!(profiles.len(), 1);
    }

    #[test]
    fn bounding_box_of_unit_cube_mesh() {
        let mesh = RenderMesh {
            vertices: vec![
                0.0, 0.0, 0.0, // v0
                1.0, 0.0, 0.0, // v1
                1.0, 1.0, 0.0, // v2
                0.0, 1.0, 0.0, // v3
                0.0, 0.0, 1.0, // v4
                1.0, 0.0, 1.0, // v5
                1.0, 1.0, 1.0, // v6
                0.0, 1.0, 1.0, // v7
            ],
            normals: vec![0.0; 24],
            indices: vec![
                0, 1, 2, 0, 2, 3, // front
                4, 6, 5, 4, 7, 6, // back
                0, 4, 5, 0, 5, 1, // bottom
                2, 6, 7, 2, 7, 3, // top
                0, 3, 7, 0, 7, 4, // left
                1, 5, 6, 1, 6, 2, // right
            ],
            face_ranges: vec![],
        };
        let (min, max) = mesh_bounding_box(&mesh);
        assert_eq!(min, [0.0, 0.0, 0.0]);
        assert_eq!(max, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn surface_area_of_unit_cube() {
        let mesh = RenderMesh {
            vertices: vec![
                0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0,
                0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0,
            ],
            normals: vec![0.0; 24],
            indices: vec![
                0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 2, 6, 7, 2, 7, 3, 0, 3, 7, 0,
                7, 4, 1, 5, 6, 1, 6, 2,
            ],
            face_ranges: vec![],
        };
        let area = mesh_surface_area(&mesh);
        assert!(
            (area - 6.0).abs() < 1e-10,
            "Unit cube area should be 6.0, got {}",
            area
        );
    }

    #[test]
    fn mesh_edge_counts_unit_cube() {
        let mesh = RenderMesh {
            vertices: vec![
                0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0,
                0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0,
            ],
            normals: vec![0.0; 24],
            indices: vec![
                0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 2, 6, 7, 2, 7, 3, 0, 3, 7, 0,
                7, 4, 1, 5, 6, 1, 6, 2,
            ],
            face_ranges: vec![],
        };
        let (total, boundary) = count_mesh_edges(&mesh);
        // A cube with shared vertices: 18 unique edges (12 cube edges + 6 diagonals from triangulation)
        assert_eq!(total, 18);
        assert_eq!(boundary, 0, "Watertight cube should have 0 boundary edges");
    }
}
