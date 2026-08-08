//! An independent geometric oracle for the assay corpus.
//!
//! Spec: `specs/assay_independent_volume_oracle.md`. Motivation, in one line:
//! **159 of the 261 SUPPORTED_CORRECT verdicts carry no absolute geometric
//! check** — the whole F and R series — so "0 WRONG" is, for them, a claim
//! about topology (watertight, 2-manifold, Euler target, per-vertex
//! on-surface) and not about whether the boolean kept the right material.
//!
//! # What this measures, and what it deliberately does not
//!
//! The **boolean** is under test. The primitive constructors are not: an
//! extrude of a sketch is separately covered and is not the hard part. So the
//! oracle may use the kernel to build each operand **in isolation**, and must
//! not use it to combine them.
//!
//! Scope, stated up front rather than discovered later: this detects a wrong
//! **SET** (wrong patch survival, dropped cavity, extra material — percent-to-
//! 100 % volume errors), not a wrong **TOLERANCE** (micron-scale positional
//! defects, which are already the `strict-validation` per-vertex oracle's job).
//!
//! # Method
//!
//! Per operand solid, a **column scan**: for a column at `(x, y)` running along
//! +z, every triangle whose xy-projection contains the point contributes a
//! crossing at its plane's `z`, signed by the triangle normal's z-component.
//! Sorting the crossings and sweeping the winding number yields the exact
//! z-**intervals** inside that solid. Intervals compose under the op chain by
//! 1-D set algebra (union for a boss), and the volume is the interval length
//! summed over a grid of columns times the cell area.
//!
//! Exact in z, discretised only in (x, y) — much sharper than point sampling,
//! and the composition is exact per column rather than sampled. Both the
//! composed operand set and the kernel's own output run through the **same**
//! code path, so the discretisation error is common-mode and largely cancels
//! in the comparison.
//!
//! # Why the band is computed and never chosen
//!
//! An oracle that emits a false WRONG is worse than no oracle. The comparison
//! band comes from running the scan at `n` and `2n` and taking the observed
//! change as the grid residual — a measurement of this oracle's own error, not
//! a tuned constant. A case is reported only when its discrepancy exceeds that.
//!
//! # Determinism
//!
//! Columns sit at a fixed **irrational** fraction of the cell (the golden-ratio
//! fraction), not at cell centres. Corpus geometry is rational and axis-aligned
//! often enough that centre sampling would land exactly on faces and edges,
//! where a crossing is ambiguous and would be double-counted. No RNG, no clock
//! (Test Philosophy: tests must be deterministic).

use waffle_types::kernel::RenderMesh;

/// Fixed irrational offset within a cell — see the determinism note above.
const PHI_FRAC: f64 = 0.618_033_988_749_894_9;

/// A triangle soup in f64 with an XY bin index, supporting column queries.
pub struct SolidScan {
    verts: Vec<[f64; 3]>,
    tris: Vec<[u32; 3]>,
    /// Inclusive XY bounds of the geometry.
    pub min: [f64; 3],
    pub max: [f64; 3],
    nx: usize,
    ny: usize,
    cell: [f64; 2],
    bins: Vec<Vec<u32>>,
}

impl SolidScan {
    /// Build from a render mesh. Vertices are `f32` in `RenderMesh`; that costs
    /// ~1e-7 relative, seven orders below the set-level errors this oracle
    /// exists to find (§ "Method" scope note).
    pub fn from_render_mesh(m: &RenderMesh) -> Option<Self> {
        let nv = m.vertices.len() / 3;
        if nv == 0 || m.indices.len() < 3 {
            return None;
        }
        let verts: Vec<[f64; 3]> = (0..nv)
            .map(|i| {
                [
                    f64::from(m.vertices[3 * i]),
                    f64::from(m.vertices[3 * i + 1]),
                    f64::from(m.vertices[3 * i + 2]),
                ]
            })
            .collect();
        let tris: Vec<[u32; 3]> = m
            .indices
            .chunks(3)
            .filter(|c| c.len() == 3)
            .map(|c| [c[0], c[1], c[2]])
            .filter(|t| t.iter().all(|&i| (i as usize) < nv))
            .collect();
        if tris.is_empty() {
            return None;
        }
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for v in &verts {
            for a in 0..3 {
                min[a] = min[a].min(v[a]);
                max[a] = max[a].max(v[a]);
            }
        }
        // Bin count ~ sqrt(T), so a column touches O(sqrt(T)) triangles.
        let side = (tris.len() as f64).sqrt().ceil().max(1.0).min(256.0) as usize;
        let (nx, ny) = (side, side);
        let cell = [
            ((max[0] - min[0]) / nx as f64).max(f64::MIN_POSITIVE),
            ((max[1] - min[1]) / ny as f64).max(f64::MIN_POSITIVE),
        ];
        let mut bins = vec![Vec::new(); nx * ny];
        for (ti, t) in tris.iter().enumerate() {
            let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for &i in t {
                let v = verts[i as usize];
                for a in 0..2 {
                    lo[a] = lo[a].min(v[a]);
                    hi[a] = hi[a].max(v[a]);
                }
            }
            let i0 = bin_idx(lo[0], min[0], cell[0], nx);
            let i1 = bin_idx(hi[0], min[0], cell[0], nx);
            let j0 = bin_idx(lo[1], min[1], cell[1], ny);
            let j1 = bin_idx(hi[1], min[1], cell[1], ny);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    bins[j * nx + i].push(ti as u32);
                }
            }
        }
        Some(Self {
            verts,
            tris,
            min,
            max,
            nx,
            ny,
            cell,
            bins,
        })
    }

    /// The z-intervals inside this solid along the +z column at `(x, y)`.
    ///
    /// Winding-number sweep, not parity: a nested or multi-shell solid (a
    /// cavity, two bodies stacked along z) resolves correctly, and an outward
    /// -oriented mesh gives `nz < 0` on entry.
    pub fn column(&self, x: f64, y: f64) -> Vec<(f64, f64)> {
        let i = bin_idx(x, self.min[0], self.cell[0], self.nx);
        let j = bin_idx(y, self.min[1], self.cell[1], self.ny);
        let mut hits: Vec<(f64, i32)> = Vec::new();
        for &ti in &self.bins[j * self.nx + i] {
            let t = self.tris[ti as usize];
            let (a, b, c) = (
                self.verts[t[0] as usize],
                self.verts[t[1] as usize],
                self.verts[t[2] as usize],
            );
            // Signed area of the xy projection; also the normal's z-component
            // (times 2), so it doubles as the crossing direction.
            let area2 = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
            if area2 == 0.0 {
                continue; // vertical triangle: no column crosses it
            }
            let w0 = (b[0] - x) * (c[1] - y) - (c[0] - x) * (b[1] - y);
            let w1 = (c[0] - x) * (a[1] - y) - (a[0] - x) * (c[1] - y);
            let w2 = (a[0] - x) * (b[1] - y) - (b[0] - x) * (a[1] - y);
            let s = area2.signum();
            if w0 * s < 0.0 || w1 * s < 0.0 || w2 * s < 0.0 {
                continue;
            }
            // Barycentric interpolation of z at (x, y).
            let z = (w0 * a[2] + w1 * b[2] + w2 * c[2]) / area2;
            // Outward normal: nz = area2 / 2. Entering the solid ⇒ nz < 0.
            hits.push((z, if area2 < 0.0 { 1 } else { -1 }));
        }
        if hits.is_empty() {
            return Vec::new();
        }
        hits.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut out = Vec::new();
        let (mut w, mut start) = (0i32, 0.0f64);
        for (z, d) in hits {
            let prev = w;
            w += d;
            if prev <= 0 && w > 0 {
                start = z;
            } else if prev > 0 && w <= 0 {
                out.push((start, z));
            }
        }
        out
    }
}

fn bin_idx(v: f64, lo: f64, cell: f64, n: usize) -> usize {
    let k = ((v - lo) / cell).floor();
    if !k.is_finite() || k < 0.0 {
        0
    } else {
        (k as usize).min(n - 1)
    }
}

/// Total length of a disjoint, ascending interval list.
pub fn iv_len(a: &[(f64, f64)]) -> f64 {
    a.iter().map(|&(lo, hi)| (hi - lo).max(0.0)).sum()
}

/// Union of two disjoint ascending interval lists.
pub fn iv_union(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut all: Vec<(f64, f64)> = a.iter().chain(b.iter()).copied().collect();
    all.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for iv in all {
        match out.last_mut() {
            Some(last) if iv.0 <= last.1 => last.1 = last.1.max(iv.1),
            _ => out.push(iv),
        }
    }
    out
}

/// `a \ b` on disjoint ascending interval lists.
pub fn iv_diff(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &(mut lo, hi) in a {
        for &(blo, bhi) in b {
            if bhi <= lo || blo >= hi {
                continue;
            }
            if blo > lo {
                out.push((lo, blo.min(hi)));
            }
            lo = lo.max(bhi);
            if lo >= hi {
                break;
            }
        }
        if lo < hi {
            out.push((lo, hi));
        }
    }
    out
}

/// A grid volume plus the residual that bounds its own error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridVolume {
    pub volume: f64,
    /// `|V(2n) − V(n)|` — this oracle's measured discretisation error.
    pub residual: f64,
    pub n: usize,
}

/// Union-compose a set of operand scans and integrate, at grid `n` and `2n`.
///
/// `cut[k]` selects `\` instead of `∪` for operand `k`. The corpus's all-boss
/// population passes all-false; the cut path is exercised by the unit tests but
/// is NOT used against the corpus (re-authoring a cut tool faithfully needs the
/// engine's own `cut_eps` and target-dependent direction reversal — see the
/// spec's operand-drift risk).
pub fn composed_volume(scans: &[&SolidScan], cut: &[bool], n: usize) -> GridVolume {
    let v1 = composed_volume_at(scans, cut, n);
    let v2 = composed_volume_at(scans, cut, 2 * n);
    GridVolume {
        volume: v2,
        residual: (v2 - v1).abs(),
        n: 2 * n,
    }
}

fn composed_volume_at(scans: &[&SolidScan], cut: &[bool], n: usize) -> f64 {
    if scans.is_empty() || n == 0 {
        return 0.0;
    }
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for s in scans {
        for a in 0..2 {
            min[a] = min[a].min(s.min[a]);
            max[a] = max[a].max(s.max[a]);
        }
    }
    // NO padding. The integration domain is exactly the operand bbox union:
    // padding it would inflate `cell` while every column still landed inside
    // the solid, biasing an axis-aligned box high by the pad fraction (caught
    // by `unit_box_volume_converges`, which wanted 6.0 and got 6.000036).
    // Columns sit at `(i + PHI_FRAC)·h` with `PHI_FRAC` strictly in (0,1), so
    // none can land on the domain edge and no pad is needed for that either.
    let hx = (max[0] - min[0]) / n as f64;
    let hy = (max[1] - min[1]) / n as f64;
    let cell = hx * hy;
    let mut total = 0.0;
    for j in 0..n {
        let y = min[1] + (j as f64 + PHI_FRAC) * hy;
        for i in 0..n {
            let x = min[0] + (i as f64 + PHI_FRAC) * hx;
            let mut acc: Vec<(f64, f64)> = Vec::new();
            for (k, s) in scans.iter().enumerate() {
                let iv = s.column(x, y);
                acc = if cut.get(k).copied().unwrap_or(false) {
                    iv_diff(&acc, &iv)
                } else {
                    iv_union(&acc, &iv)
                };
            }
            total += iv_len(&acc) * cell;
        }
    }
    total
}

/// Grid volume of ONE scan — the kernel's output, through the same code path,
/// so the discretisation error is common-mode with the composed operand set.
pub fn scan_volume(s: &SolidScan, n: usize) -> GridVolume {
    composed_volume(&[s], &[false], n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Axis-aligned box as a closed outward-oriented triangle mesh.
    fn box_mesh(lo: [f64; 3], hi: [f64; 3]) -> RenderMesh {
        let c = [
            [lo[0], lo[1], lo[2]],
            [hi[0], lo[1], lo[2]],
            [hi[0], hi[1], lo[2]],
            [lo[0], hi[1], lo[2]],
            [lo[0], lo[1], hi[2]],
            [hi[0], lo[1], hi[2]],
            [hi[0], hi[1], hi[2]],
            [lo[0], hi[1], hi[2]],
        ];
        let faces: [[usize; 4]; 6] = [
            [0, 3, 2, 1], // -z
            [4, 5, 6, 7], // +z
            [0, 1, 5, 4], // -y
            [3, 7, 6, 2], // +y
            [0, 4, 7, 3], // -x
            [1, 2, 6, 5], // +x
        ];
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for f in faces {
            let base = (vertices.len() / 3) as u32;
            for &vi in &f {
                for a in 0..3 {
                    vertices.push(c[vi][a] as f32);
                }
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        RenderMesh {
            vertices,
            normals: Vec::new(),
            indices,
            face_ranges: Vec::new(),
        }
    }

    fn scan(lo: [f64; 3], hi: [f64; 3]) -> SolidScan {
        SolidScan::from_render_mesh(&box_mesh(lo, hi)).expect("scan")
    }

    #[test]
    fn unit_box_volume_converges() {
        let s = scan([0.0, 0.0, 0.0], [1.0, 2.0, 3.0]);
        let v = scan_volume(&s, 64);
        assert!(
            (v.volume - 6.0).abs() < 1e-6,
            "got {} residual {}",
            v.volume,
            v.residual
        );
    }

    #[test]
    fn a_column_through_a_box_reports_its_z_span() {
        let s = scan([0.0, 0.0, -1.0], [2.0, 2.0, 4.0]);
        let iv = s.column(1.0, 1.0);
        assert_eq!(iv.len(), 1);
        assert!((iv[0].0 - -1.0).abs() < 1e-9 && (iv[0].1 - 4.0).abs() < 1e-9);
    }

    #[test]
    fn a_column_outside_the_box_is_empty() {
        let s = scan([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!(s.column(5.0, 5.0).is_empty());
    }

    /// Two boxes stacked along z with a gap — the winding sweep must report
    /// TWO intervals, which is what parity would also give but a naive
    /// "first hit / last hit" would not.
    #[test]
    fn disjoint_shells_along_z_give_two_intervals() {
        let mut m = box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let m2 = box_mesh([0.0, 0.0, 2.0], [1.0, 1.0, 3.0]);
        let base = (m.vertices.len() / 3) as u32;
        m.vertices.extend_from_slice(&m2.vertices);
        m.indices.extend(m2.indices.iter().map(|i| i + base));
        let s = SolidScan::from_render_mesh(&m).unwrap();
        let iv = s.column(0.5, 0.5);
        assert_eq!(iv.len(), 2, "{iv:?}");
        assert!(iv_len(&iv) - 2.0 < 1e-9);
    }

    #[test]
    fn union_of_overlapping_boxes_is_not_double_counted() {
        let a = scan([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]); // 8
        let b = scan([1.0, 1.0, 1.0], [3.0, 3.0, 3.0]); // 8, overlap 1
        let v = composed_volume(&[&a, &b], &[false, false], 96);
        assert!(
            (v.volume - 15.0).abs() < 2e-2,
            "got {} (residual {})",
            v.volume,
            v.residual
        );
    }

    #[test]
    fn difference_removes_exactly_the_overlap() {
        let a = scan([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]); // 8
        let b = scan([1.0, 1.0, -1.0], [3.0, 3.0, 3.0]); // covers a's +x+y quadrant fully in z
        let v = composed_volume(&[&a, &b], &[false, true], 96);
        assert!((v.volume - 6.0).abs() < 2e-2, "got {}", v.volume);
    }

    #[test]
    fn disjoint_union_is_additive() {
        let a = scan([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let b = scan([5.0, 0.0, 0.0], [6.0, 1.0, 1.0]);
        let v = composed_volume(&[&a, &b], &[false, false], 128);
        assert!((v.volume - 2.0).abs() < 2e-2, "got {}", v.volume);
    }

    #[test]
    fn the_residual_shrinks_as_the_grid_refines() {
        let a = scan([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let b = scan([1.0, 1.0, 1.0], [3.0, 3.0, 3.0]);
        let coarse = composed_volume(&[&a, &b], &[false, false], 24);
        let fine = composed_volume(&[&a, &b], &[false, false], 192);
        assert!(
            fine.residual <= coarse.residual,
            "coarse {} fine {}",
            coarse.residual,
            fine.residual
        );
    }

    #[test]
    fn interval_algebra_union() {
        let u = iv_union(&[(0.0, 1.0), (3.0, 4.0)], &[(0.5, 3.5)]);
        assert_eq!(u, vec![(0.0, 4.0)]);
    }

    #[test]
    fn interval_algebra_difference_splits() {
        let d = iv_diff(&[(0.0, 10.0)], &[(2.0, 3.0), (5.0, 6.0)]);
        assert_eq!(d, vec![(0.0, 2.0), (3.0, 5.0), (6.0, 10.0)]);
    }

    #[test]
    fn difference_by_a_covering_interval_is_empty() {
        assert!(iv_diff(&[(1.0, 2.0)], &[(0.0, 5.0)]).is_empty());
    }

    #[test]
    fn an_empty_mesh_yields_no_scan() {
        let m = RenderMesh {
            vertices: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
            face_ranges: Vec::new(),
        };
        assert!(SolidScan::from_render_mesh(&m).is_none());
    }
}
