use kernel_fork::RenderMesh;

/// Convert a `RenderMesh` to binary STL format.
///
/// Binary STL layout:
/// - 80 bytes: header
/// - 4 bytes: u32 LE triangle count
/// - Per triangle (50 bytes each):
///   - 12 bytes: normal vector (3 × f32 LE)
///   - 36 bytes: 3 vertices (3 × 3 × f32 LE)
///   - 2 bytes: attribute byte count (0u16)
pub fn render_mesh_to_stl(mesh: &RenderMesh) -> Vec<u8> {
    let tri_count = mesh.indices.len() / 3;
    let size = 84 + tri_count * 50;
    let mut buf = Vec::with_capacity(size);

    // 80-byte header
    let header = b"Waffle Iron STL Export";
    buf.extend_from_slice(header);
    buf.extend_from_slice(&[0u8; 80 - 22]); // zero-pad to 80 bytes

    // Triangle count (u32 LE)
    buf.extend_from_slice(&(tri_count as u32).to_le_bytes());

    for t in 0..tri_count {
        let i0 = mesh.indices[t * 3] as usize;
        let i1 = mesh.indices[t * 3 + 1] as usize;
        let i2 = mesh.indices[t * 3 + 2] as usize;

        let v0 = [
            mesh.vertices[i0 * 3],
            mesh.vertices[i0 * 3 + 1],
            mesh.vertices[i0 * 3 + 2],
        ];
        let v1 = [
            mesh.vertices[i1 * 3],
            mesh.vertices[i1 * 3 + 1],
            mesh.vertices[i1 * 3 + 2],
        ];
        let v2 = [
            mesh.vertices[i2 * 3],
            mesh.vertices[i2 * 3 + 1],
            mesh.vertices[i2 * 3 + 2],
        ];

        // Compute face normal via cross product of edges
        let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let nx = e1[1] * e2[2] - e1[2] * e2[1];
        let ny = e1[2] * e2[0] - e1[0] * e2[2];
        let nz = e1[0] * e2[1] - e1[1] * e2[0];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        let normal = if len > 1e-12 {
            [nx / len, ny / len, nz / len]
        } else {
            [0.0, 0.0, 0.0]
        };

        // Normal (3 × f32 LE)
        for c in &normal {
            buf.extend_from_slice(&c.to_le_bytes());
        }
        // Vertices (3 × 3 × f32 LE)
        for v in &[v0, v1, v2] {
            for c in v {
                buf.extend_from_slice(&c.to_le_bytes());
            }
        }
        // Attribute byte count
        buf.extend_from_slice(&0u16.to_le_bytes());
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stl_export_empty_mesh() {
        let mesh = RenderMesh {
            vertices: vec![],
            normals: vec![],
            indices: vec![],
            face_ranges: vec![],
        };
        let stl = render_mesh_to_stl(&mesh);
        assert_eq!(stl.len(), 84);
        // Header starts with "Waffle Iron STL Export"
        assert!(stl[..22].starts_with(b"Waffle Iron STL Export"));
        // Triangle count = 0
        assert_eq!(u32::from_le_bytes([stl[80], stl[81], stl[82], stl[83]]), 0);
    }

    #[test]
    fn stl_export_single_triangle() {
        let mesh = RenderMesh {
            vertices: vec![
                0.0, 0.0, 0.0, // v0
                1.0, 0.0, 0.0, // v1
                0.0, 1.0, 0.0, // v2
            ],
            normals: vec![],
            indices: vec![0, 1, 2],
            face_ranges: vec![],
        };
        let stl = render_mesh_to_stl(&mesh);
        // 84 header + 1 * 50 = 134
        assert_eq!(stl.len(), 134);
        // Triangle count = 1
        assert_eq!(u32::from_le_bytes([stl[80], stl[81], stl[82], stl[83]]), 1);

        // Normal should be (0, 0, 1) — cross product of (1,0,0)×(0,1,0)
        let nz = f32::from_le_bytes([stl[92], stl[93], stl[94], stl[95]]);
        assert!((nz - 1.0).abs() < 1e-6);
    }

    #[test]
    fn stl_export_multi_triangle() {
        // A quad made of 2 triangles
        let mesh = RenderMesh {
            vertices: vec![
                0.0, 0.0, 0.0, // v0
                1.0, 0.0, 0.0, // v1
                1.0, 1.0, 0.0, // v2
                0.0, 1.0, 0.0, // v3
            ],
            normals: vec![],
            indices: vec![0, 1, 2, 0, 2, 3],
            face_ranges: vec![],
        };
        let stl = render_mesh_to_stl(&mesh);
        // 84 header + 2 * 50 = 184
        assert_eq!(stl.len(), 184);
        assert_eq!(u32::from_le_bytes([stl[80], stl[81], stl[82], stl[83]]), 2);
    }
}
