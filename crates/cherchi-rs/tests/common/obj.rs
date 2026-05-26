//! Minimal triangulated-OBJ I/O for cherchi-rs sidecar tests.
//!
//! Test-only — the production cherchi-rs API works on triangle arrays
//! via `cad-primitives` types, not files. This module exists to bridge
//! to the C++ `mesh_booleans` binary which requires OBJ on disk.
//!
//! Triangulated faces only. `vn`/`vt`/material directives are skipped.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

/// `(verts, tris)` triangulated mesh. Faces are 0-indexed.
pub type TriMesh = (Vec<[f64; 3]>, Vec<[usize; 3]>);

/// Write a triangulated mesh to OBJ. `tris` are 0-indexed (OBJ on disk
/// is 1-indexed; we adjust on write).
pub fn write_obj(path: &Path, verts: &[[f64; 3]], tris: &[[usize; 3]]) -> io::Result<()> {
    let mut f = fs::File::create(path)?;
    for v in verts {
        writeln!(f, "v {} {} {}", v[0], v[1], v[2])?;
    }
    for t in tris {
        writeln!(f, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1)?;
    }
    Ok(())
}

/// Parse a triangulated OBJ. Skips comments + blank lines + non-v/f
/// directives. Face vertex tokens may be `i`, `i/j`, or `i/j/k`; only
/// the position index `i` is read. Returns 0-indexed faces.
/// Returns `InvalidData` if any face has != 3 vertices.
pub fn read_obj(path: &Path) -> io::Result<TriMesh> {
    let file = fs::File::open(path)?;
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[usize; 3]> = Vec::new();
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
                verts.push([coords[0], coords[1], coords[2]]);
            }
            Some("f") => {
                let indices: Vec<usize> = parts
                    .map(|tok| {
                        // Accept `i`, `i/j`, or `i/j/k` — take only `i`.
                        let i_str = tok.split('/').next().unwrap_or(tok);
                        i_str.parse::<usize>().map(|i| i - 1) // OBJ is 1-indexed
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                if indices.len() != 3 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("non-triangle face with {} verts", indices.len()),
                    ));
                }
                tris.push([indices[0], indices[1], indices[2]]);
            }
            _ => { /* ignore vn, vt, g, mtllib, usemtl, o, s, etc. */ }
        }
    }
    Ok((verts, tris))
}
