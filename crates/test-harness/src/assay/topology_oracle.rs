//! Independent TOPOLOGY oracle — the genus of a document's composed solid,
//! read from the ISOLATED operand solids rather than from the kernel's
//! boolean output.
//!
//! Why it exists. The corpus generator authors `euler_target = 2` for every
//! case (a single genus-0 shell). That is an assumption about the union of
//! the operations, not a measurement: a partial revolve is a C-shaped bar,
//! and two such bars around offset axes, or a bar whose ends a boss bridges,
//! legitimately form a handle (R0011 was adjudicated genus 1 by hand and its
//! meta corrected to `euler_target = 0`; R0003's genus 2 was adjudicated by
//! a density ladder — `session_2026_08_28e`). Each such adjudication was a
//! one-off. This module makes it an instrument: voxelise the SET UNION of
//! the operand solids (each rebuilt in isolation through kernel-v2's
//! primitive ops — the same trusted route the volume oracle uses), and read
//! the Euler characteristic of the resulting cubical complex.
//!
//! For a compact 3-manifold `M` with boundary, `χ(M) = χ(∂M) / 2`, so a
//! single handlebody of genus `g` reads `χ(M) = 1 − g`, and the mesh oracle's
//! `expected_chi` on the boundary surface is `2 · χ(M)`. The readout is
//! taken on a resolution LADDER: a value that is stable as the grid refines
//! is the union's topology; a value that changes is a feature the grid
//! cannot yet resolve (measured 2026-09-03 on R0053: a 3.4-unit radial gap
//! between two eccentric revolves aliases against 3.9-unit cells), and the
//! instrument says so by disagreeing with itself rather than by rounding.
//! [`readout`] therefore STREAMS the grid two layers at a time, so the
//! resolution is bounded by time rather than by memory.
//!
//! Scope matches the volume oracle's: all-BOSS chains, datum-anchored
//! sketches. Cut cases are not re-authored (see `volume_oracle_doc`).

use super::volume_oracle::SolidScan;

/// Occupancy of an axis-aligned voxel grid held in memory: cube `(i, j, k)`
/// is occupied when the solid contains its centre. The in-memory reference
/// for [`readout`]; fine for unit tests and small grids.
pub struct VoxelGrid {
    /// Cube counts per axis.
    pub n: [usize; 3],
    occ: Vec<bool>,
}

impl VoxelGrid {
    /// Build from a predicate — for unit tests and synthetic shapes.
    pub fn from_fn(n: [usize; 3], f: impl Fn(usize, usize, usize) -> bool) -> Self {
        let mut occ = vec![false; n[0] * n[1] * n[2]];
        for k in 0..n[2] {
            for j in 0..n[1] {
                for i in 0..n[0] {
                    occ[(k * n[1] + j) * n[0] + i] = f(i, j, k);
                }
            }
        }
        Self { n, occ }
    }

    /// Voxelise the SET UNION of `scans` at `n` cubes along each axis of the
    /// scans' joint bounding box. `None` when there is nothing to scan.
    pub fn from_scans(scans: &[&SolidScan], n: usize) -> Option<Self> {
        let cols = ColumnRanges::from_scans(scans, n, 0.5)?;
        let mut occ = vec![false; n * n * n];
        let mut layer = vec![false; n * n];
        for k in 0..n {
            cols.fill_layer(k, &mut layer);
            occ[k * n * n..(k + 1) * n * n].copy_from_slice(&layer);
        }
        Some(Self { n: [n, n, n], occ })
    }

    #[inline]
    fn idx(&self, i: usize, j: usize, k: usize) -> usize {
        (k * self.n[1] + j) * self.n[0] + i
    }

    /// Is cube `(i, j, k)` occupied? Out-of-range indices are empty.
    pub fn occupied(&self, i: usize, j: usize, k: usize) -> bool {
        i < self.n[0] && j < self.n[1] && k < self.n[2] && self.occ[self.idx(i, j, k)]
    }

    /// Occupied cubes with at least one empty (or out-of-grid) face
    /// neighbour — the cubes whose membership the boundary decided. Their
    /// volume bounds the reading's volume error.
    pub fn surface_cubes(&self) -> usize {
        let mut count = 0usize;
        for k in 0..self.n[2] {
            for j in 0..self.n[1] {
                for i in 0..self.n[0] {
                    if !self.occ[self.idx(i, j, k)] {
                        continue;
                    }
                    let exposed = (i == 0 || !self.occ[self.idx(i - 1, j, k)])
                        || (i + 1 == self.n[0] || !self.occ[self.idx(i + 1, j, k)])
                        || (j == 0 || !self.occ[self.idx(i, j - 1, k)])
                        || (j + 1 == self.n[1] || !self.occ[self.idx(i, j + 1, k)])
                        || (k == 0 || !self.occ[self.idx(i, j, k - 1)])
                        || (k + 1 == self.n[2] || !self.occ[self.idx(i, j, k + 1)]);
                    if exposed {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    /// Number of occupied cubes.
    pub fn count(&self) -> usize {
        self.occ.iter().filter(|&&b| b).count()
    }

    /// Euler characteristic of the closed cubical complex spanned by the
    /// occupied cubes: `V − E + F − C` over the distinct lattice vertices,
    /// edges, faces and cubes they carry.
    pub fn euler_characteristic(&self) -> i64 {
        let m = [self.n[0] + 1, self.n[1] + 1, self.n[2] + 1];
        let vi = |i: usize, j: usize, k: usize| (k * m[1] + j) * m[0] + i;
        let total = m[0] * m[1] * m[2];
        let mut verts = vec![false; total];
        // Edge in direction d anchored at lattice vertex (i, j, k).
        let mut edges = vec![false; 3 * total];
        // Face normal to direction d anchored at lattice vertex (i, j, k).
        let mut faces = vec![false; 3 * total];
        let mut cubes = 0i64;
        for k in 0..self.n[2] {
            for j in 0..self.n[1] {
                for i in 0..self.n[0] {
                    if !self.occ[self.idx(i, j, k)] {
                        continue;
                    }
                    cubes += 1;
                    for dk in 0..2 {
                        for dj in 0..2 {
                            for di in 0..2 {
                                verts[vi(i + di, j + dj, k + dk)] = true;
                            }
                        }
                    }
                    // Four edges per direction: the anchor varies over the
                    // two other axes.
                    for (dj, dk) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        edges[vi(i, j + dj, k + dk)] = true;
                    }
                    for (di, dk) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        edges[total + vi(i + di, j, k + dk)] = true;
                    }
                    for (di, dj) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        edges[2 * total + vi(i + di, j + dj, k)] = true;
                    }
                    // Six faces: two per normal direction.
                    for d in 0..2 {
                        faces[vi(i + d, j, k)] = true; // normal x
                        faces[total + vi(i, j + d, k)] = true; // normal y
                        faces[2 * total + vi(i, j, k + d)] = true; // normal z
                    }
                }
            }
        }
        let count = |v: &[bool]| v.iter().filter(|&&b| b).count() as i64;
        count(&verts) - count(&edges) + count(&faces) - cubes
    }

    /// Connected components of the occupied cubes under FACE adjacency.
    pub fn components(&self) -> usize {
        let mut seen = vec![false; self.occ.len()];
        let mut comps = 0;
        let mut stack: Vec<(usize, usize, usize)> = Vec::new();
        for k in 0..self.n[2] {
            for j in 0..self.n[1] {
                for i in 0..self.n[0] {
                    let id = self.idx(i, j, k);
                    if !self.occ[id] || seen[id] {
                        continue;
                    }
                    comps += 1;
                    seen[id] = true;
                    stack.push((i, j, k));
                    while let Some((a, b, c)) = stack.pop() {
                        let nbrs = [
                            (a.wrapping_sub(1), b, c),
                            (a + 1, b, c),
                            (a, b.wrapping_sub(1), c),
                            (a, b + 1, c),
                            (a, b, c.wrapping_sub(1)),
                            (a, b, c + 1),
                        ];
                        for (x, y, z) in nbrs {
                            if self.occupied(x, y, z) {
                                let q = self.idx(x, y, z);
                                if !seen[q] {
                                    seen[q] = true;
                                    stack.push((x, y, z));
                                }
                            }
                        }
                    }
                }
            }
        }
        comps
    }

    /// Cube counts of the face-connected components, largest first — dust
    /// (a handful of cubes) tells a lattice sliver from a genuine body.
    pub fn component_sizes(&self) -> Vec<usize> {
        let mut seen = vec![false; self.occ.len()];
        let mut sizes = Vec::new();
        let mut stack: Vec<(usize, usize, usize)> = Vec::new();
        for k in 0..self.n[2] {
            for j in 0..self.n[1] {
                for i in 0..self.n[0] {
                    let id = self.idx(i, j, k);
                    if !self.occ[id] || seen[id] {
                        continue;
                    }
                    let mut size = 0usize;
                    seen[id] = true;
                    stack.push((i, j, k));
                    while let Some((a, b, c)) = stack.pop() {
                        size += 1;
                        let nbrs = [
                            (a.wrapping_sub(1), b, c),
                            (a + 1, b, c),
                            (a, b.wrapping_sub(1), c),
                            (a, b + 1, c),
                            (a, b, c.wrapping_sub(1)),
                            (a, b, c + 1),
                        ];
                        for (x, y, z) in nbrs {
                            if self.occupied(x, y, z) {
                                let q = self.idx(x, y, z);
                                if !seen[q] {
                                    seen[q] = true;
                                    stack.push((x, y, z));
                                }
                            }
                        }
                    }
                    sizes.push(size);
                }
            }
        }
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        sizes
    }

    /// The readout of this in-memory grid.
    pub fn readout(&self) -> TopologyReadout {
        TopologyReadout {
            n: self.n[0],
            cubes: self.count(),
            chi: self.euler_characteristic(),
            components: self.components(),
        }
    }
}

/// The union's occupied cube-index ranges per column, computed ONCE from the
/// scans' column walks (`SolidScan::column`, the volume oracle's own), in
/// CSR form. Sample points are `min + (idx + phase)·h` (`phase = ½` = centres).
struct ColumnRanges {
    n: usize,
    offsets: Vec<u32>,
    ranges: Vec<(u32, u32)>,
}

impl ColumnRanges {
    fn from_scans(scans: &[&SolidScan], n: usize, phase: f64) -> Option<Self> {
        if scans.is_empty() || n == 0 || !(0.0..1.0).contains(&phase) {
            return None;
        }
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for s in scans {
            for a in 0..3 {
                min[a] = min[a].min(s.min[a]);
                max[a] = max[a].max(s.max[a]);
            }
        }
        if (0..3).any(|a| (max[a] - min[a]).partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater))
        {
            return None;
        }
        let h: [f64; 3] = std::array::from_fn(|a| (max[a] - min[a]) / n as f64);
        let mut offsets = Vec::with_capacity(n * n + 1);
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        let mut col: Vec<(u32, u32)> = Vec::new();
        for j in 0..n {
            let y = min[1] + (j as f64 + phase) * h[1];
            for i in 0..n {
                let x = min[0] + (i as f64 + phase) * h[0];
                offsets.push(ranges.len() as u32);
                col.clear();
                for s in scans {
                    for (a, b) in s.column(x, y) {
                        // Sample points z_k = min_z + (k + phase) h_z inside (a, b).
                        let lo = ((a - min[2]) / h[2] - phase).ceil().max(0.0);
                        let hi = ((b - min[2]) / h[2] - phase).floor().min(n as f64 - 1.0);
                        if lo.is_finite() && hi.is_finite() && hi >= lo {
                            col.push((lo as u32, hi as u32));
                        }
                    }
                }
                // Merge the union's ranges so a layer test is a short scan.
                col.sort_unstable();
                let start = ranges.len();
                for &(lo, hi) in &col {
                    if ranges.len() > start {
                        let last = ranges.last_mut().unwrap();
                        if lo <= last.1 + 1 {
                            last.1 = last.1.max(hi);
                            continue;
                        }
                    }
                    ranges.push((lo, hi));
                }
            }
        }
        offsets.push(ranges.len() as u32);
        Some(Self { n, offsets, ranges })
    }

    /// Occupancy of layer `k` into `layer` (row-major `j * n + i`).
    fn fill_layer(&self, k: usize, layer: &mut [bool]) {
        let k = k as u32;
        for (c, cell) in layer.iter_mut().enumerate().take(self.n * self.n) {
            let (a, b) = (self.offsets[c] as usize, self.offsets[c + 1] as usize);
            *cell = self.ranges[a..b].iter().any(|&(lo, hi)| lo <= k && k <= hi);
        }
    }
}

/// Union–find over horizontal runs, for the streaming component count.
#[derive(Default)]
struct Dsu {
    parent: Vec<u32>,
}

impl Dsu {
    fn make(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(id);
        id
    }
    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let p = self.parent[x as usize];
            self.parent[x as usize] = self.parent[p as usize];
            x = p;
        }
        x
    }
    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra as usize] = rb;
        }
    }
    fn roots(&mut self) -> usize {
        (0..self.parent.len() as u32)
            .filter(|&x| self.find(x) == x)
            .count()
    }
}

/// One rung of the ladder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyReadout {
    /// Cubes per axis.
    pub n: usize,
    /// Occupied cubes.
    pub cubes: usize,
    /// `χ` of the occupied cubical complex (`1 − genus` for one handlebody).
    pub chi: i64,
    /// Face-connected components (bodies).
    pub components: usize,
}

impl TopologyReadout {
    /// The boundary surface's Euler characteristic — what the mesh oracle's
    /// `expected_chi` names — for a cubical solid that is a 3-manifold.
    pub fn boundary_chi(&self) -> i64 {
        2 * self.chi
    }
}

/// Voxelise the union of `scans` at `n` cubes per axis and read its
/// topology, streaming two layers at a time: memory is `O(n²)`, so `n` is
/// bounded by time alone. Identical to [`VoxelGrid::readout`] (pinned by
/// test) — the lattice elements are counted at the layer that introduces
/// them, and components are unioned run by run.
pub fn readout(scans: &[&SolidScan], n: usize) -> Option<TopologyReadout> {
    readout_at(scans, n, 0.5)
}

/// [`readout`] with the sample point at fraction `phase` (in `[0, 1)`) of each
/// cell instead of its centre — the same solid on a SHIFTED lattice. A
/// topology that survives a phase change at fixed `n` is not an aliasing
/// artefact of one particular lattice.
pub fn readout_at(scans: &[&SolidScan], n: usize, phase: f64) -> Option<TopologyReadout> {
    let cols = ColumnRanges::from_scans(scans, n, phase)?;
    let nn = n * n;
    let mut prev = vec![false; nn];
    let mut cur = vec![false; nn];
    let mut both = vec![false; nn];
    let mut prev_lab = vec![u32::MAX; nn];
    let mut cur_lab = vec![u32::MAX; nn];
    let mut dsu = Dsu::default();
    let (mut verts, mut edges, mut faces, mut cubes) = (0i64, 0i64, 0i64, 0i64);
    for kz in 0..=n {
        std::mem::swap(&mut prev, &mut cur);
        std::mem::swap(&mut prev_lab, &mut cur_lab);
        if kz < n {
            cols.fill_layer(kz, &mut cur);
        } else {
            cur.fill(false);
        }
        for c in 0..nn {
            both[c] = prev[c] || cur[c];
        }
        let at = |l: &[bool], i: usize, j: usize| -> bool {
            // `usize::MAX` (from a wrapping −1) is out of range.
            i < n && j < n && l[j * n + i]
        };
        let (mut v, mut e, mut f) = (0i64, 0i64, 0i64);
        for j in 0..=n {
            let jm = j.wrapping_sub(1);
            for i in 0..=n {
                let im = i.wrapping_sub(1);
                // Lattice vertex (i, j, kz): any of its eight cubes.
                if at(&both, im, jm) || at(&both, i, jm) || at(&both, im, j) || at(&both, i, j) {
                    v += 1;
                }
                // x-edge from (i, j, kz): the four cubes around it.
                if i < n && (at(&both, i, jm) || at(&both, i, j)) {
                    e += 1;
                }
                // y-edge from (i, j, kz).
                if j < n && (at(&both, im, j) || at(&both, i, j)) {
                    e += 1;
                }
                // z-edge from (i, j, kz) up to kz + 1: the four cubes of `cur`.
                if kz < n
                    && (at(&cur, im, jm) || at(&cur, i, jm) || at(&cur, im, j) || at(&cur, i, j))
                {
                    e += 1;
                }
                // z-normal face at height kz over cell (i, j).
                if i < n && j < n && (at(&prev, i, j) || at(&cur, i, j)) {
                    f += 1;
                }
                // x-normal face at lattice x = i, cell j, between kz and kz + 1.
                if kz < n && j < n && (at(&cur, im, j) || at(&cur, i, j)) {
                    f += 1;
                }
                // y-normal face at lattice y = j, cell i.
                if kz < n && i < n && (at(&cur, i, jm) || at(&cur, i, j)) {
                    f += 1;
                }
            }
        }
        verts += v;
        edges += e;
        faces += f;
        if kz < n {
            cubes += cur.iter().filter(|&&b| b).count() as i64;
            // Label this layer's cubes by horizontal run, union across rows
            // and with the layer below.
            for j in 0..n {
                let mut run: Option<u32> = None;
                for i in 0..n {
                    let c = j * n + i;
                    if !cur[c] {
                        run = None;
                        cur_lab[c] = u32::MAX;
                        continue;
                    }
                    let id = match run {
                        Some(id) => id,
                        None => {
                            let id = dsu.make();
                            run = Some(id);
                            id
                        }
                    };
                    cur_lab[c] = id;
                    if j > 0 && cur[c - n] {
                        dsu.union(id, cur_lab[c - n]);
                    }
                    if prev[c] {
                        dsu.union(id, prev_lab[c]);
                    }
                }
            }
        }
    }
    Some(TopologyReadout {
        n,
        cubes: cubes as usize,
        chi: verts - edges + faces - cubes,
        components: dsu.roots(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use waffle_types::kernel::RenderMesh;

    #[test]
    fn solid_block_is_a_ball() {
        let g = VoxelGrid::from_fn([3, 3, 3], |_, _, _| true);
        assert_eq!(g.count(), 27);
        assert_eq!(g.euler_characteristic(), 1);
        assert_eq!(g.components(), 1);
    }

    #[test]
    fn single_cube_is_a_ball() {
        let g = VoxelGrid::from_fn([1, 1, 1], |_, _, _| true);
        // 8 vertices − 12 edges + 6 faces − 1 cube.
        assert_eq!(g.euler_characteristic(), 1);
    }

    #[test]
    fn square_ring_is_a_torus() {
        // 3×3×1 with the centre missing: one handle, χ = 0.
        let g = VoxelGrid::from_fn([3, 3, 1], |i, j, _| !(i == 1 && j == 1));
        assert_eq!(g.count(), 8);
        assert_eq!(g.euler_characteristic(), 0);
        assert_eq!(g.components(), 1);
    }

    #[test]
    fn thick_ring_is_still_a_torus() {
        // 5×5×2 with a 1×1 hole through both layers.
        let g = VoxelGrid::from_fn([5, 5, 2], |i, j, _| !(i == 2 && j == 2));
        assert_eq!(g.euler_characteristic(), 0);
        assert_eq!(g.components(), 1);
    }

    #[test]
    fn two_holes_read_genus_two() {
        // 7×3×1 bar with two separate 1×1 holes: χ = 1 − 2.
        let g = VoxelGrid::from_fn([7, 3, 1], |i, j, _| !(j == 1 && (i == 1 || i == 5)));
        assert_eq!(g.euler_characteristic(), -1);
        assert_eq!(g.components(), 1);
    }

    #[test]
    fn two_separate_blocks_read_two_balls() {
        let g = VoxelGrid::from_fn([5, 1, 1], |i, _, _| i != 2);
        assert_eq!(g.euler_characteristic(), 2);
        assert_eq!(g.components(), 2);
    }

    #[test]
    fn hollow_block_reads_a_cavity() {
        // 3×3×3 minus the centre: a thick spherical shell, ∂M = two spheres,
        // χ(M) = 2 with ONE face-connected body.
        let g = VoxelGrid::from_fn([3, 3, 3], |i, j, k| !(i == 1 && j == 1 && k == 1));
        assert_eq!(g.euler_characteristic(), 2);
        assert_eq!(g.components(), 1);
    }

    #[test]
    fn boundary_chi_is_twice_the_solid_chi() {
        let r = TopologyReadout {
            n: 1,
            cubes: 1,
            chi: 0,
            components: 1,
        };
        assert_eq!(r.boundary_chi(), 0);
        let r = TopologyReadout {
            n: 1,
            cubes: 1,
            chi: 1,
            components: 1,
        };
        assert_eq!(r.boundary_chi(), 2);
    }

    /// An axis-aligned box as a render mesh, wound outward (the column walk
    /// reads entering/leaving from the xy-projected winding).
    fn box_mesh(min: [f32; 3], max: [f32; 3]) -> RenderMesh {
        let p = |m: u8| -> [f32; 3] {
            [
                if m & 1 != 0 { max[0] } else { min[0] },
                if m & 2 != 0 { max[1] } else { min[1] },
                if m & 4 != 0 { max[2] } else { min[2] },
            ]
        };
        let mut vertices = Vec::new();
        for m in 0..8u8 {
            vertices.extend_from_slice(&p(m));
        }
        // Each face as two triangles, counter-clockwise seen from outside.
        let quads: [[u32; 4]; 6] = [
            [0, 2, 3, 1], // z = min (normal −z)
            [4, 5, 7, 6], // z = max (normal +z)
            [0, 1, 5, 4], // y = min (normal −y)
            [2, 6, 7, 3], // y = max (normal +y)
            [0, 4, 6, 2], // x = min (normal −x)
            [1, 3, 7, 5], // x = max (normal +x)
        ];
        let mut indices = Vec::new();
        for q in quads {
            indices.extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]);
        }
        RenderMesh {
            vertices,
            normals: vec![0.0; 24],
            indices,
            face_ranges: Vec::new(),
        }
    }

    fn scan(min: [f32; 3], max: [f32; 3]) -> SolidScan {
        SolidScan::from_render_mesh(&box_mesh(min, max)).unwrap()
    }

    #[test]
    fn streaming_readout_matches_the_in_memory_grid() {
        // A square frame of four boxes (union genus 1) plus a separate box.
        let scans = [
            scan([0.0, 0.0, 0.0], [10.0, 2.0, 2.0]),
            scan([0.0, 8.0, 0.0], [10.0, 10.0, 2.0]),
            scan([0.0, 0.0, 0.0], [2.0, 10.0, 2.0]),
            scan([8.0, 0.0, 0.0], [10.0, 10.0, 2.0]),
            scan([4.0, 4.0, 5.0], [6.0, 6.0, 7.0]),
        ];
        let refs: Vec<&SolidScan> = scans.iter().collect();
        for n in [5, 10, 20, 33] {
            let streamed = readout(&refs, n).unwrap();
            let grid = VoxelGrid::from_scans(&refs, n).unwrap().readout();
            assert_eq!(streamed, grid, "n = {n}");
            assert_eq!(
                (streamed.chi, streamed.components),
                (1, 2),
                "n = {n}: {streamed:?}"
            );
        }
    }

    #[test]
    fn a_shifted_lattice_reads_the_same_topology() {
        let scans = [
            scan([0.0, 0.0, 0.0], [10.0, 2.0, 2.0]),
            scan([0.0, 8.0, 0.0], [10.0, 10.0, 2.0]),
            scan([0.0, 0.0, 0.0], [2.0, 10.0, 2.0]),
            scan([8.0, 0.0, 0.0], [10.0, 10.0, 2.0]),
        ];
        let refs: Vec<&SolidScan> = scans.iter().collect();
        for phase in [0.1, 0.25, 0.5, 0.9] {
            let r = readout_at(&refs, 20, phase).unwrap();
            assert_eq!((r.chi, r.components), (0, 1), "phase {phase}: {r:?}");
        }
        assert!(readout_at(&refs, 20, 1.0).is_none());
    }

    #[test]
    fn one_box_reads_one_ball() {
        let s = scan([0.0, 0.0, 0.0], [3.0, 4.0, 5.0]);
        for n in [1, 4, 16] {
            let r = readout(&[&s], n).unwrap();
            assert_eq!((r.chi, r.components), (1, 1), "n = {n}: {r:?}");
            assert_eq!(r.cubes, n * n * n);
        }
    }

    #[test]
    fn overlapping_boxes_read_one_ball() {
        let a = scan([0.0, 0.0, 0.0], [6.0, 6.0, 6.0]);
        let b = scan([3.0, 3.0, 3.0], [9.0, 9.0, 9.0]);
        let r = readout(&[&a, &b], 18).unwrap();
        assert_eq!((r.chi, r.components), (1, 1), "{r:?}");
        // 6³ + 6³ − 3³ cubes of side ½.
        assert_eq!(r.cubes, 2 * 12 * 12 * 12 - 6 * 6 * 6);
    }
}
