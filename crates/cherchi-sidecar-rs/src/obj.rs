//! OBJ file I/O for the sidecar wrapper.
//!
//! Public so consumers can dump meshes for inspection / fixture
//! capture / feeding hand-crafted OBJ through the binary.
//!
//! Triangulated faces only. `vn`/`vt`/material directives are
//! skipped. Face vertex tokens may be `i`, `i/j`, or `i/j/k`; only
//! the position index `i` is read.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use cad_primitives::Point3;
use cherchi_rs::Mesh;

/// Write a `Mesh` to an OBJ file. Triangle indices are 0-based in
/// memory; the on-disk OBJ uses 1-based per the format spec.
pub fn write_obj(path: &Path, mesh: &Mesh) -> io::Result<()> {
    let mut f = fs::File::create(path)?;
    for v in &mesh.verts {
        writeln!(f, "v {} {} {}", v.x(), v.y(), v.z())?;
    }
    for t in &mesh.tris {
        writeln!(f, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1)?;
    }
    Ok(())
}

/// Parse a triangulated OBJ into a `Mesh`. Indices in the on-disk
/// file are 1-based; the returned `Mesh.tris` are 0-based.
///
/// Errors:
/// - `InvalidData` if any face has != 3 vertices
/// - `InvalidData` if a vertex line has malformed coords
/// - `InvalidData` if any face vertex index overflows `u32`
///   (silent narrowing on >4G-vertex meshes would mis-truncate)
pub fn read_obj(path: &Path) -> io::Result<Mesh> {
    let file = fs::File::open(path)?;
    let mut verts: Vec<Point3> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        match parts.next() {
            Some("v") => {
                let coords: Vec<f64> = parts
                    .take(3)
                    .map(|s| s.parse::<f64>())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                if coords.len() != 3 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("v: expected 3 coords, got {}", coords.len()),
                    ));
                }
                verts.push(Point3::new(coords[0], coords[1], coords[2]));
            }
            Some("f") => {
                let indices: Result<Vec<u32>, io::Error> = parts
                    .map(|tok| {
                        let i_str = tok.split('/').next().unwrap_or(tok);
                        let one_based: usize = i_str.parse::<usize>().map_err(|e| {
                            io::Error::new(io::ErrorKind::InvalidData, e.to_string())
                        })?;
                        if one_based == 0 {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "OBJ face index 0 is invalid (1-based format)",
                            ));
                        }
                        let zero_based = one_based - 1;
                        u32::try_from(zero_based).map_err(|_| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("face vertex index {one_based} exceeds u32"),
                            )
                        })
                    })
                    .collect();
                let indices = indices?;
                if indices.len() != 3 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("non-triangle face with {} verts", indices.len()),
                    ));
                }
                tris.push([indices[0], indices[1], indices[2]]);
            }
            _ => { /* skip vn, vt, g, mtllib, usemtl, etc. */ }
        }
    }
    Ok(Mesh { verts, tris })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn tempfile(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cherchi-sidecar-rs-{}", name));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("test.obj")
    }

    #[test]
    fn round_trip_single_tri() {
        let path = tempfile("round_trip");
        let mesh = Mesh::new(
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
            vec![[0, 1, 2]],
        );
        write_obj(&path, &mesh).unwrap();
        let read = read_obj(&path).unwrap();
        assert_eq!(read, mesh);
    }

    #[test]
    fn non_triangle_face_is_invalid() {
        let path = tempfile("non_tri");
        // Manually write a 4-vertex face.
        std::fs::write(&path, b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n").unwrap();
        let err = read_obj(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn face_index_zero_is_invalid() {
        let path = tempfile("idx_zero");
        std::fs::write(&path, b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 0 1 2\n").unwrap();
        let err = read_obj(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
