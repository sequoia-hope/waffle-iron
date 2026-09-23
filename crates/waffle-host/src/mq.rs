//! `mq/1`: the compact mesh encoding (`specs/waffle_server_mode.md` §4.5) a
//! viewer asks for off the loopback. Positions and edge polylines are
//! quantized to 16 bits per axis inside the body's bounding box (≤ 11 µm on
//! a 0.7 m body — display precision; every measurement stays on the host),
//! normals are octahedral-encoded to 8 bits, and each stream goes through the
//! meshoptimizer vertex / index codec (the glTF `EXT_meshopt_compression`
//! scheme, whose decoder three.js already ships), then gzip. Picking is
//! untouched: the index buffer is exact and `face_ranges` / `edge_ranges`
//! ride in the header exactly as `raw/1` carries them.
//!
//! Layout: `u32 LE header_len (a multiple of 4) | JSON header, zero-padded |
//! gzip(positions ‖ normals ‖ indices ‖ edges)`, the four meshopt streams'
//! encoded lengths in the header. Positions are `u16 × 4` (x, y, z, 0),
//! stride 8; normals `i8 × 4` after the oct filter, stride 4; indices the
//! meshopt index stream over `u32`; edges `u16 × 4` like positions.
//!
//! Compression: the spec text names brotli; browsers decode gzip natively
//! (`DecompressionStream`) and brotli only with a library, and on float
//! geometry the two are within a few percent of each other (§4.5's
//! measurement: gzip 0.66, brotli-5 0.62), so gzip it is — the header says
//! `compression: "gzip"` so a later encoding can differ.

use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::{json, Value};

pub const ENCODING: &str = "mq/1";
const QUANT_BITS: u32 = 16;
const QUANT_MAX: f32 = 65535.0;
const NORMAL_BITS: i32 = 8;

/// A `raw/1` blob taken apart: the header and its four arrays.
pub struct Raw {
    pub header: Value,
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
    pub edges: Vec<f32>,
}

/// Parse a `raw/1` blob (`viewer.rs` writes it; this is the only reader on
/// the host side).
pub fn parse_raw(bytes: &[u8]) -> Option<Raw> {
    if bytes.len() < 4 {
        return None;
    }
    let header_len = u32::from_le_bytes(bytes[..4].try_into().ok()?) as usize;
    let header_bytes = bytes.get(4..4 + header_len)?;
    let end = header_bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(header_bytes.len());
    let header: Value = serde_json::from_slice(&header_bytes[..end]).ok()?;
    if header.get("encoding").and_then(Value::as_str) != Some(crate::viewer::ENCODING) {
        return None;
    }
    let vertex_count = header.get("vertex_count")?.as_u64()? as usize;
    let index_count = header.get("index_count")?.as_u64()? as usize;
    let edge_vertex_count = header.get("edge_vertex_count")?.as_u64()? as usize;
    let mut offset = 4 + header_len;
    let mut section = |n_words: usize| -> Option<&[u8]> {
        let chunk = bytes.get(offset..offset + n_words * 4)?;
        offset += n_words * 4;
        Some(chunk)
    };
    let f32s = |chunk: &[u8]| -> Vec<f32> {
        chunk
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    };
    let positions = f32s(section(vertex_count * 3)?);
    let normals = f32s(section(vertex_count * 3)?);
    let indices = section(index_count)?
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let edges = f32s(section(edge_vertex_count * 3)?);
    Some(Raw {
        header,
        positions,
        normals,
        indices,
        edges,
    })
}

/// The quantization frame: the body's bounding box over positions AND edge
/// polylines (an edge can lie a hair outside the triangle hull), with a
/// non-zero extent on every axis so a flat body still decodes.
fn frame(positions: &[f32], edges: &[f32]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in positions.chunks_exact(3).chain(edges.chunks_exact(3)) {
        for k in 0..3 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    let mut extent = [0.0f32; 3];
    for k in 0..3 {
        if !min[k].is_finite() {
            min[k] = 0.0;
            max[k] = 0.0;
        }
        extent[k] = (max[k] - min[k]).max(f32::EPSILON);
    }
    (min, extent)
}

fn quantize(points: &[f32], min: &[f32; 3], extent: &[f32; 3]) -> Vec<[u16; 4]> {
    points
        .chunks_exact(3)
        .map(|p| {
            let q = |k: usize| {
                ((p[k] - min[k]) / extent[k] * QUANT_MAX)
                    .round()
                    .clamp(0.0, QUANT_MAX) as u16
            };
            [q(0), q(1), q(2), 0]
        })
        .collect()
}

fn dequantize(q: &[[u16; 4]], min: &[f32; 3], extent: &[f32; 3]) -> Vec<f32> {
    let mut out = Vec::with_capacity(q.len() * 3);
    for v in q {
        for k in 0..3 {
            out.push(min[k] + f32::from(v[k]) / QUANT_MAX * extent[k]);
        }
    }
    out
}

/// Octahedral-encode unit normals to `i8 × 4` (x, y, z, w) with the meshopt
/// filter, so the viewer's `decodeFilterOct` restores them.
fn encode_normals(normals: &[f32]) -> Vec<[i8; 4]> {
    let count = normals.len() / 3;
    let mut input = Vec::with_capacity(count * 4);
    for n in normals.chunks_exact(3) {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let (x, y, z) = if len > 0.0 {
            (n[0] / len, n[1] / len, n[2] / len)
        } else {
            (0.0, 0.0, 1.0)
        };
        input.extend_from_slice(&[x, y, z, 0.0]);
    }
    let mut out = vec![[0i8; 4]; count];
    // SAFETY: `input` holds `count * 4` floats and `out` `count` vectors of
    // stride 4, exactly what the filter's contract asks for.
    unsafe {
        meshopt::ffi::meshopt_encodeFilterOct(
            out.as_mut_ptr().cast(),
            count,
            4,
            NORMAL_BITS,
            input.as_ptr(),
        );
    }
    out
}

fn decode_normals(encoded: &mut [[i8; 4]]) -> Vec<f32> {
    // SAFETY: stride 4, `count` vectors, as encoded above.
    unsafe {
        meshopt::ffi::meshopt_decodeFilterOct(encoded.as_mut_ptr().cast(), encoded.len(), 4);
    }
    let mut out = Vec::with_capacity(encoded.len() * 3);
    for v in encoded.iter() {
        for component in &v[..3] {
            out.push(f32::from(*component) / 127.0);
        }
    }
    out
}

/// `raw/1` bytes → `mq/1` bytes. `None` when the raw blob does not parse.
pub fn encode(raw_bytes: &[u8]) -> Option<Vec<u8>> {
    let raw = parse_raw(raw_bytes)?;
    let (min, extent) = frame(&raw.positions, &raw.edges);
    let vertex_count = raw.positions.len() / 3;
    let positions = meshopt::encode_vertex_buffer(&quantize(&raw.positions, &min, &extent)).ok()?;
    let normals = meshopt::encode_vertex_buffer(&encode_normals(&raw.normals)).ok()?;
    let indices = if raw.indices.is_empty() {
        Vec::new()
    } else {
        meshopt::encode_index_buffer(&raw.indices, vertex_count).ok()?
    };
    let edges = if raw.edges.is_empty() {
        Vec::new()
    } else {
        meshopt::encode_vertex_buffer(&quantize(&raw.edges, &min, &extent)).ok()?
    };
    let mut streams =
        Vec::with_capacity(positions.len() + normals.len() + indices.len() + edges.len());
    streams.extend_from_slice(&positions);
    streams.extend_from_slice(&normals);
    streams.extend_from_slice(&indices);
    streams.extend_from_slice(&edges);
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(&streams).ok()?;
    let payload = gz.finish().ok()?;

    let header = json!({
        "encoding": ENCODING,
        "compression": "gzip",
        "vertex_count": vertex_count,
        "index_count": raw.indices.len(),
        "edge_vertex_count": raw.edges.len() / 3,
        "quant_bits": QUANT_BITS,
        "normal_bits": NORMAL_BITS,
        "frame": { "min": min, "extent": extent },
        "streams": {
            "positions": positions.len(),
            "normals": normals.len(),
            "indices": indices.len(),
            "edges": edges.len(),
        },
        "face_ranges": raw.header.get("face_ranges").cloned().unwrap_or(Value::Array(Vec::new())),
        "edge_ranges": raw.header.get("edge_ranges").cloned().unwrap_or(Value::Array(Vec::new())),
    });
    let header = serde_json::to_vec(&header).ok()?;
    let padded = header.len().div_ceil(4) * 4;
    let mut out = Vec::with_capacity(4 + padded + payload.len());
    out.extend_from_slice(&(padded as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.resize(4 + padded, 0);
    out.extend_from_slice(&payload);
    Some(out)
}

/// `mq/1` bytes → the arrays a viewer draws (the reference decoder, used by
/// the tests to pin what the browser's meshopt decoder must reproduce).
pub fn decode(bytes: &[u8]) -> Option<Raw> {
    let header_len = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
    let header_bytes = bytes.get(4..4 + header_len)?;
    let end = header_bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(header_bytes.len());
    let header: Value = serde_json::from_slice(&header_bytes[..end]).ok()?;
    if header.get("encoding").and_then(Value::as_str) != Some(ENCODING) {
        return None;
    }
    let mut streams = Vec::new();
    GzDecoder::new(&bytes[4 + header_len..])
        .read_to_end(&mut streams)
        .ok()?;
    let vertex_count = header["vertex_count"].as_u64()? as usize;
    let index_count = header["index_count"].as_u64()? as usize;
    let edge_vertex_count = header["edge_vertex_count"].as_u64()? as usize;
    let len = |k: &str| header["streams"][k].as_u64().map(|n| n as usize);
    let (lp, ln, li, le) = (
        len("positions")?,
        len("normals")?,
        len("indices")?,
        len("edges")?,
    );
    let mut at = 0;
    let mut take = |n: usize| -> Option<&[u8]> {
        let s = streams.get(at..at + n)?;
        at += n;
        Some(s)
    };
    let f = |k: &str| -> Option<[f32; 3]> {
        let a = header["frame"][k].as_array()?;
        Some([
            a[0].as_f64()? as f32,
            a[1].as_f64()? as f32,
            a[2].as_f64()? as f32,
        ])
    };
    let (min, extent) = (f("min")?, f("extent")?);
    let q: Vec<[u16; 4]> = meshopt::decode_vertex_buffer(take(lp)?, vertex_count).ok()?;
    let positions = dequantize(&q, &min, &extent);
    let mut n: Vec<[i8; 4]> = meshopt::decode_vertex_buffer(take(ln)?, vertex_count).ok()?;
    let normals = decode_normals(&mut n);
    let indices: Vec<u32> = if index_count == 0 {
        Vec::new()
    } else {
        meshopt::decode_index_buffer(take(li)?, index_count).ok()?
    };
    let edges = if edge_vertex_count == 0 {
        Vec::new()
    } else {
        let q: Vec<[u16; 4]> = meshopt::decode_vertex_buffer(take(le)?, edge_vertex_count).ok()?;
        dequantize(&q, &min, &extent)
    };
    Some(Raw {
        header,
        positions,
        normals,
        indices,
        edges,
    })
}

/// Triangles with each rotated to start at its smallest index: the meshopt
/// index codec keeps triangle order and winding but not which vertex a
/// triangle starts on, which changes no face range and no pick.
pub fn canonical_triangles(indices: &[u32]) -> Vec<[u32; 3]> {
    indices
        .chunks_exact(3)
        .map(|t| {
            let k = (0..3).min_by_key(|&k| t[k]).unwrap();
            [t[k], t[(k + 1) % 3], t[(k + 2) % 3]]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_blob(positions: &[f32], normals: &[f32], indices: &[u32], edges: &[f32]) -> Vec<u8> {
        let header = json!({
            "encoding": crate::viewer::ENCODING,
            "vertex_count": positions.len() / 3,
            "index_count": indices.len(),
            "edge_vertex_count": edges.len() / 3,
            "face_ranges": [{ "start": 0, "count": indices.len() }],
            "edge_ranges": [],
        });
        let header = serde_json::to_vec(&header).unwrap();
        let padded = header.len().div_ceil(4) * 4;
        let mut out = Vec::new();
        out.extend_from_slice(&(padded as u32).to_le_bytes());
        out.extend_from_slice(&header);
        out.resize(4 + padded, 0);
        for v in positions.iter().chain(normals) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for i in indices {
            out.extend_from_slice(&i.to_le_bytes());
        }
        for e in edges {
            out.extend_from_slice(&e.to_le_bytes());
        }
        out
    }

    #[test]
    fn a_blob_round_trips_within_quantization_and_keeps_its_indices_exact() {
        let positions = [0.0, 0.0, 0.0, 0.7, 0.0, 0.0, 0.7, 0.3, 0.0, 0.0, 0.3, 0.02];
        let normals = [
            0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.577, 0.577, 0.577,
        ];
        let indices = [0u32, 1, 2, 0, 2, 3];
        let edges = [0.0, 0.0, 0.0, 0.7, 0.0, 0.0, 0.7, 0.0, 0.0, 0.7, 0.3, 0.0];
        let raw = raw_blob(&positions, &normals, &indices, &edges);
        let mq = encode(&raw).expect("encodes");
        let back = decode(&mq).expect("decodes");
        assert_eq!(
            canonical_triangles(&back.indices),
            canonical_triangles(&indices)
        );
        assert_eq!(back.header["face_ranges"][0]["count"], 6);
        // 16 bits over a 0.7 m extent: ≤ 11 µm.
        for (a, b) in back.positions.iter().zip(positions) {
            assert!((a - b).abs() <= 0.7 / 65535.0 + 1e-7, "{a} vs {b}");
        }
        for (a, b) in back.edges.iter().zip(edges) {
            assert!((a - b).abs() <= 0.7 / 65535.0 + 1e-7, "{a} vs {b}");
        }
        // 8-bit octahedral normals: within about a degree.
        for (n, m) in back.normals.chunks_exact(3).zip(normals.chunks_exact(3)) {
            let dot = n[0] * m[0] + n[1] * m[1] + n[2] * m[2];
            let lens = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
                * (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
            assert!(dot / lens > 0.9995, "normal {n:?} vs {m:?}");
        }
    }

    #[test]
    fn a_real_sized_body_shrinks_severalfold() {
        // A 64×64 grid: 4096 vertices, 7938 triangles — the size of a small
        // body's tessellation, where the header and gzip framing no longer
        // dominate (a four-vertex blob is LARGER in mq/1, and that is fine).
        let n = 64usize;
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        for j in 0..n {
            for i in 0..n {
                let (x, y) = (i as f32 * 0.01, j as f32 * 0.01);
                positions.extend_from_slice(&[x, y, (x * 7.0).sin() * 0.02]);
                let nx = -(x * 7.0).cos() * 0.14;
                let len = (nx * nx + 1.0).sqrt();
                normals.extend_from_slice(&[nx / len, 0.0, 1.0 / len]);
            }
        }
        let mut indices = Vec::new();
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = (j * n + i) as u32;
                indices.extend_from_slice(&[
                    a,
                    a + 1,
                    a + n as u32,
                    a + 1,
                    a + n as u32 + 1,
                    a + n as u32,
                ]);
            }
        }
        let raw = raw_blob(&positions, &normals, &indices, &positions[..n * 3 * 2]);
        let mq = encode(&raw).unwrap();
        let back = decode(&mq).unwrap();
        assert_eq!(
            canonical_triangles(&back.indices),
            canonical_triangles(&indices)
        );
        assert!(
            mq.len() * 3 < raw.len(),
            "mq {} vs raw {}",
            mq.len(),
            raw.len()
        );
    }

    #[test]
    fn a_flat_body_and_an_edgeless_body_still_encode() {
        let positions = [0.0, 0.0, 0.5, 1.0, 0.0, 0.5, 1.0, 1.0, 0.5];
        let normals = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        let raw = raw_blob(&positions, &normals, &[0, 1, 2], &[]);
        let back = decode(&encode(&raw).unwrap()).unwrap();
        assert!(back.edges.is_empty());
        for (a, b) in back.positions.iter().zip(positions) {
            assert!((a - b).abs() <= 1e-4, "{a} vs {b}");
        }
    }

    #[test]
    fn something_that_is_not_raw_is_refused() {
        assert!(encode(b"nope").is_none());
        assert!(decode(b"nope").is_none());
    }
}
