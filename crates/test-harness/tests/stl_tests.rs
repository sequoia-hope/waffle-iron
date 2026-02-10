//! Tests for STL export functionality.

use kernel_fork::types::RenderMesh;
use test_harness::stl::{export_ascii_stl, export_binary_stl};

fn make_triangle_mesh() -> RenderMesh {
    RenderMesh {
        vertices: vec![
            0.0, 0.0, 0.0, // v0
            1.0, 0.0, 0.0, // v1
            0.0, 1.0, 0.0, // v2
        ],
        normals: vec![
            0.0, 0.0, 1.0, // n0
            0.0, 0.0, 1.0, // n1
            0.0, 0.0, 1.0, // n2
        ],
        indices: vec![0, 1, 2],
        face_ranges: vec![kernel_fork::types::FaceRange {
            face_id: kernel_fork::KernelId(1),
            start_index: 0,
            end_index: 3,
        }],
    }
}

fn make_box_mesh() -> RenderMesh {
    // Simple box: 8 vertices, 12 triangles (2 per face)
    RenderMesh {
        vertices: vec![
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0,
            1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0,
        ],
        normals: vec![
            0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
        ],
        indices: vec![
            0, 1, 2, 0, 2, 3, // front
            4, 6, 5, 4, 7, 6, // back
            0, 4, 5, 0, 5, 1, // bottom
            2, 6, 7, 2, 7, 3, // top
            0, 3, 7, 0, 7, 4, // left
            1, 5, 6, 1, 6, 2, // right
        ],
        face_ranges: vec![],
    }
}

#[test]
fn binary_stl_header_is_80_bytes() {
    let mesh = make_triangle_mesh();
    let stl = export_binary_stl(&mesh, "test").unwrap();
    // Header is first 80 bytes
    assert!(stl.len() >= 80, "Binary STL must be at least 80 bytes");
    // Header should contain the name
    let header = String::from_utf8_lossy(&stl[..80]);
    assert!(header.contains("test"), "Header should contain solid name");
}

#[test]
fn binary_stl_file_size_formula() {
    let mesh = make_box_mesh();
    let stl = export_binary_stl(&mesh, "box").unwrap();
    let tri_count = mesh.indices.len() / 3;
    let expected_size = 80 + 4 + tri_count * 50;
    assert_eq!(
        stl.len(),
        expected_size,
        "Binary STL size = 80 + 4 + N*50 where N={}",
        tri_count
    );
}

#[test]
fn binary_stl_triangle_count_matches() {
    let mesh = make_box_mesh();
    let stl = export_binary_stl(&mesh, "box").unwrap();
    let tri_count = u32::from_le_bytes([stl[80], stl[81], stl[82], stl[83]]);
    assert_eq!(
        tri_count as usize,
        mesh.indices.len() / 3,
        "Triangle count in header should match"
    );
}

#[test]
fn ascii_stl_has_correct_keywords() {
    let mesh = make_triangle_mesh();
    let stl = export_ascii_stl(&mesh, "test_solid").unwrap();
    assert!(stl.starts_with("solid test_solid\n"));
    assert!(stl.ends_with("endsolid test_solid\n"));
    assert!(stl.contains("facet normal"));
    assert!(stl.contains("outer loop"));
    assert!(stl.contains("vertex"));
    assert!(stl.contains("endloop"));
    assert!(stl.contains("endfacet"));
}

#[test]
fn empty_mesh_returns_error() {
    let mesh = RenderMesh {
        vertices: vec![],
        normals: vec![],
        indices: vec![],
        face_ranges: vec![],
    };
    assert!(export_binary_stl(&mesh, "empty").is_err());
    assert!(export_ascii_stl(&mesh, "empty").is_err());
}

#[test]
fn invalid_index_returns_error() {
    let mesh = RenderMesh {
        vertices: vec![0.0, 0.0, 0.0], // Only 1 vertex
        normals: vec![0.0, 0.0, 1.0],
        indices: vec![0, 1, 2], // Indices 1 and 2 are out of range
        face_ranges: vec![],
    };
    assert!(export_binary_stl(&mesh, "bad").is_err());
    assert!(export_ascii_stl(&mesh, "bad").is_err());
}
