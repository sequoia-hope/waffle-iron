//! STL export from RenderMesh — binary and ASCII formats.

use crate::helpers::HarnessError;
use kernel_fork::types::RenderMesh;

/// Export a RenderMesh as a binary STL file.
///
/// Binary STL format:
/// - 80-byte header (arbitrary text)
/// - u32 triangle count (little-endian)
/// - For each triangle: 3×f32 normal + 3×(3×f32 vertex) + u16 attribute = 50 bytes
pub fn export_binary_stl(mesh: &RenderMesh, name: &str) -> Result<Vec<u8>, HarnessError> {
    let tri_count = mesh.indices.len() / 3;
    if tri_count == 0 {
        return Err(HarnessError::StlError {
            reason: "mesh has no triangles".to_string(),
        });
    }

    // Validate indices
    let vertex_count = mesh.vertices.len() / 3;
    for &idx in &mesh.indices {
        if idx as usize >= vertex_count {
            return Err(HarnessError::StlError {
                reason: format!(
                    "index {} out of range (vertex count = {})",
                    idx, vertex_count
                ),
            });
        }
    }

    let file_size = 80 + 4 + tri_count * 50;
    let mut buf = Vec::with_capacity(file_size);

    // 80-byte header
    let header = format!("binary STL: {}", name);
    let header_bytes = header.as_bytes();
    buf.extend_from_slice(&header_bytes[..header_bytes.len().min(80)]);
    buf.resize(80, 0u8);

    // Triangle count
    buf.extend_from_slice(&(tri_count as u32).to_le_bytes());

    // Triangles
    for tri in mesh.indices.chunks(3) {
        let i0 = tri[0] as usize * 3;
        let i1 = tri[1] as usize * 3;
        let i2 = tri[2] as usize * 3;

        // Compute face normal from cross product
        let (ax, ay, az) = (
            mesh.vertices[i1] - mesh.vertices[i0],
            mesh.vertices[i1 + 1] - mesh.vertices[i0 + 1],
            mesh.vertices[i1 + 2] - mesh.vertices[i0 + 2],
        );
        let (bx, by, bz) = (
            mesh.vertices[i2] - mesh.vertices[i0],
            mesh.vertices[i2 + 1] - mesh.vertices[i0 + 1],
            mesh.vertices[i2 + 2] - mesh.vertices[i0 + 2],
        );
        let nx = ay * bz - az * by;
        let ny = az * bx - ax * bz;
        let nz = ax * by - ay * bx;
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        let (nx, ny, nz) = if len > 1e-12 {
            (nx / len, ny / len, nz / len)
        } else {
            (0.0f32, 0.0, 1.0)
        };

        // Normal
        buf.extend_from_slice(&nx.to_le_bytes());
        buf.extend_from_slice(&ny.to_le_bytes());
        buf.extend_from_slice(&nz.to_le_bytes());

        // 3 vertices
        for &idx in tri {
            let vi = idx as usize * 3;
            buf.extend_from_slice(&mesh.vertices[vi].to_le_bytes());
            buf.extend_from_slice(&mesh.vertices[vi + 1].to_le_bytes());
            buf.extend_from_slice(&mesh.vertices[vi + 2].to_le_bytes());
        }

        // Attribute byte count (unused)
        buf.extend_from_slice(&0u16.to_le_bytes());
    }

    Ok(buf)
}

/// Export a RenderMesh as an ASCII STL string.
pub fn export_ascii_stl(mesh: &RenderMesh, name: &str) -> Result<String, HarnessError> {
    let tri_count = mesh.indices.len() / 3;
    if tri_count == 0 {
        return Err(HarnessError::StlError {
            reason: "mesh has no triangles".to_string(),
        });
    }

    let vertex_count = mesh.vertices.len() / 3;
    for &idx in &mesh.indices {
        if idx as usize >= vertex_count {
            return Err(HarnessError::StlError {
                reason: format!(
                    "index {} out of range (vertex count = {})",
                    idx, vertex_count
                ),
            });
        }
    }

    let mut out = String::with_capacity(tri_count * 300);
    out.push_str(&format!("solid {}\n", name));

    for tri in mesh.indices.chunks(3) {
        let i0 = tri[0] as usize * 3;
        let i1 = tri[1] as usize * 3;
        let i2 = tri[2] as usize * 3;

        // Compute face normal
        let (ax, ay, az) = (
            mesh.vertices[i1] - mesh.vertices[i0],
            mesh.vertices[i1 + 1] - mesh.vertices[i0 + 1],
            mesh.vertices[i1 + 2] - mesh.vertices[i0 + 2],
        );
        let (bx, by, bz) = (
            mesh.vertices[i2] - mesh.vertices[i0],
            mesh.vertices[i2 + 1] - mesh.vertices[i0 + 1],
            mesh.vertices[i2 + 2] - mesh.vertices[i0 + 2],
        );
        let nx = ay * bz - az * by;
        let ny = az * bx - ax * bz;
        let nz = ax * by - ay * bx;
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        let (nx, ny, nz) = if len > 1e-12 {
            (nx / len, ny / len, nz / len)
        } else {
            (0.0f32, 0.0, 1.0)
        };

        out.push_str(&format!("  facet normal {} {} {}\n", nx, ny, nz));
        out.push_str("    outer loop\n");
        for &idx in tri {
            let vi = idx as usize * 3;
            out.push_str(&format!(
                "      vertex {} {} {}\n",
                mesh.vertices[vi],
                mesh.vertices[vi + 1],
                mesh.vertices[vi + 2]
            ));
        }
        out.push_str("    endloop\n");
        out.push_str("  endfacet\n");
    }

    out.push_str(&format!("endsolid {}\n", name));
    Ok(out)
}
