//! Measurement utilities for tessellation output. NOT repair — these only
//! observe the mesh, never modify it. Used in `[stage-f]` eprintln traces and
//! the assay oracle.

use crate::units::{TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN};
use std::collections::BTreeMap;

/// Count unpaired edges in a triangle mesh under oracle-grid quantization.
/// An edge is "paired" when it appears in exactly 2 triangles; anything else
/// (1, 3+) is unpaired. Returns the count of unpaired edges. Used for
/// watertightness diagnosis; does NOT modify the mesh.
pub(super) fn count_unpaired_in_mesh(vertices: &[f32], indices: &[u32]) -> usize {
    if vertices.is_empty() || indices.is_empty() {
        return 0;
    }
    let n_verts = vertices.len() / 3;
    let n_tris = indices.len() / 3;
    let max_abs = vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
    let grid = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let inv_grid = 1.0 / grid;
    type QPos = (i64, i64, i64);
    let quantize = |idx: u32| -> QPos {
        let i = idx as usize;
        if i >= n_verts {
            return (0, 0, 0);
        }
        (
            (vertices[i * 3] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 1] as f64 * inv_grid).round() as i64,
            (vertices[i * 3 + 2] as f64 * inv_grid).round() as i64,
        )
    };
    let make_edge = |a: QPos, b: QPos| -> (QPos, QPos) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };
    let mut edge_counts: BTreeMap<(QPos, QPos), usize> = BTreeMap::new();
    for t in 0..n_tris {
        let base = t * 3;
        let tri = [indices[base], indices[base + 1], indices[base + 2]];
        let qt = [quantize(tri[0]), quantize(tri[1]), quantize(tri[2])];
        if qt[0] == qt[1] || qt[1] == qt[2] || qt[0] == qt[2] {
            continue;
        }
        for e in 0..3 {
            *edge_counts
                .entry(make_edge(qt[e], qt[(e + 1) % 3]))
                .or_insert(0) += 1;
        }
    }
    edge_counts.values().filter(|&&c| c != 2).count()
}
