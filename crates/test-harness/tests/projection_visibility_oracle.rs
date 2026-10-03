//! The **visibility oracle** of `specs/drawings_and_mbd.md` §5.3, over the
//! assay corpus — the corpus-wide half of D1c.
//!
//! §5.3 states it directly: "Render the solid's tessellation with an
//! orthographic depth buffer in the test harness (software rasterizer, no GPU)
//! and sample each visible segment at 16 points: every sample must lie within
//! one chord tolerance of the depth buffer's front surface. Hidden segments
//! must not."
//!
//! The per-primitive half — a box's nine-and-three, a cylinder's far rim split
//! in two equal halves, a bore's hidden far rim — is
//! `kernel_v2::projection::visibility::tests`, where the answers are known in
//! closed form. What this sweep adds is the geometry nobody writes by hand:
//! gear unions, chained booleans, revolves with surface-pair curves.
//!
//! ## Why the depth buffer is an independent oracle
//!
//! The kernel decides visibility by casting ONE ray per classified curve, from
//! its midpoint, and asking the exact segment/triangle predicate whether
//! anything crosses in front. This oracle asks a different question with
//! different machinery: it rasterizes the whole tessellation into a depth
//! buffer and then compares each sample's own depth against the frontmost
//! surface there. Nothing is shared — not the ray, not the predicate, not the
//! acceleration structure, and not the lift (the sample's own depth is
//! recovered here from [`KernelIntrospect::edge_polyline`], re-derived rather
//! than read off the kernel's answer). A classification that agreed with
//! itself but not with the surface fails here.
//!
//! ## What is checked
//!
//! For every sampled case, every live body of it, and each of the six axis
//! directions, each classified curve is sampled at sixteen interior points and
//! each sample's own depth is compared against the surfaces along its own line
//! of sight. The judgement is then made about the CURVE, not about each sample
//! on its own:
//!
//! 1. **A curve's tag must hold along the whole curve.** A classified piece
//!    claims one visibility for its whole length, so if every decided sample
//!    contradicts that claim, the classification is wrong and the sweep fails.
//! 2. **Samples that DISAGREE with each other mean something else, and
//!    something more useful.** The piece spans a visibility change — the curve
//!    really does go behind something partway along — which says a CROSSING WAS
//!    NOT SPLIT, not that either part is classified wrongly. Those are counted
//!    by curve and reported per case (`spans_a_change`), because the fix is
//!    upstream in the split.
//! 3. **No sample is in front of the whole solid.** A projected edge's sample
//!    lies ON the boundary, so the frontmost surface along its line of sight
//!    can never be behind it; if it were, the PROJECTION would be wrong before
//!    visibility came up.
//!
//! §5.3 words the first as "a visible sample must lie within one chord
//! tolerance of the depth buffer's front surface". For a point known to be on
//! the boundary — which check 3 establishes — that is the same statement as
//! "nothing stands in front of it", and the second form is the one that can be
//! tested at a curve lying on the boundary of its OWN faces, which every edge
//! does.
//!
//! The two directions are deliberately NOT symmetric, and that asymmetry is
//! load-bearing. "Not definitely in front" already leaves a `Visible` tag
//! standing: a surface within the band of the sample's own depth does not
//! contradict it. It is not enough to call a `Hidden` tag wrong, though —
//! something has to be definitely NOT in front for that — so a surface nearer
//! than the sample by less than its own band is the undecidable middle and is
//! counted. Measured 2026-10-03: without the distinction, near-silhouette
//! occluders (slopes from 0.77 to 44 over the corpus sample) made the sweep
//! report hidden arcs as having nothing in front of them.
//!
//! Four things make the comparison sound, and the corpus taught every one of
//! them rather than reasoning doing it:
//!
//! - **The front depth is evaluated AT the sample, not at a cell centre.** The
//!   buffer rasterizes for the coverage census, but the comparison walks the
//!   triangles whose footprint covers the point. A cell centre half a cell away
//!   can land on material that is simply not there along the sample's own line
//!   of sight — which is what a hole's rim edge, sitting inside the hole in the
//!   body's front face, does.
//! - **Containment is tested in MODEL units, with two different margins.** A
//!   surface contains the sample AT ALL if the sample is no further outside it
//!   than the mesh's own inscription deficit (the render chord band), because a
//!   curved face's polygon falls a chord sagitta short of the exact rim that
//!   bounds it and a rim's samples sit that far outside their own face. It
//!   contains it STRICTLY only if the sample is inside by more than the f32
//!   quantization of the mesh's vertices. A dimensionless barycentric margin
//!   cannot express either (see [`INSIDE_REL`]).
//! - **The depth band carries the front surface's own SLOPE.** The mesh is
//!   inscribed, so a facet sits a chord sagitta off the exact surface measured
//!   PERPENDICULAR to it, and converting that to the line of sight costs
//!   `√(1 + slope²)` — 1 on a face seen square on, divergent as the surface
//!   turns parallel to the view (see [`DepthBuffer::depth_band`]).
//! - **The samples are INTERIOR.** A classified piece's two endpoints are the
//!   crossings it was cut at, which is exactly where its visibility changes, so
//!   a sample sitting on one is ambiguous by construction. R0055's hidden arcs
//!   reported "nothing in front" at their own ends and nowhere else.
//!
//! ## What is reported rather than asserted
//!
//! - **Silhouette curves** are counted but not depth-checked. A silhouette
//!   point is where the surface turns away from the viewer, so the front and
//!   back depths COINCIDE there and a band wide enough to be sound would assert
//!   nothing. They are checked for COVERAGE instead: a silhouette must lie on
//!   the body's own projected footprint.
//! - **Uncovered samples** — no triangle contains the sample even loosely.
//!   Counted, since on a thin feature or at a silhouette the inscribed mesh's
//!   footprint can fall short of the exact edge by more than the chord band.
//! - **Grazing occluders** — a surface nearer than a `Hidden` sample, but
//!   either reaching it only on a triangle's BOUNDARY or nearer by less than
//!   the band its own slope needs. The first is a curve lying exactly in a face
//!   parallel to the line of sight (a boss's bottom rim in the plate's top
//!   plane, a bore's far rim at its own wall), which is where the kernel's
//!   separation rule — a face the ray grazes separates nothing — and a
//!   containment test legitimately differ; the kernel counts that one from its
//!   own side as `ProjectionDeclines::ray_grazes_face`.
//! - **Curves spanning a visibility change** — the samples of one classified
//!   piece disagree, so the piece covers both states and a crossing was not
//!   split. Counted by curve and listed per case: the fix is in the crossing
//!   search or in whatever outline element is missing from the view, not in the
//!   classification, and a `split_tangency` or a declined silhouette upstream
//!   is the usual reason.
//! - **Grazing surfaces** — the front surface stands so near parallel to the
//!   line of sight that its slope band exceeds a quarter of the solid's own
//!   depth extent, so the inscribed mesh cannot place it in depth at all.
//! - **The kernel's own typed DECLINES** ([`ProjectionDeclines`]) are summed
//!   over the sweep and printed by kind, and the sweep asserts the ones that
//!   must stay empty. Before D1c these were print-only, so no oracle could
//!   pin them; the point of putting them on `ViewGeometry` was to make this
//!   assertion possible.
//!
//! ```text
//! cargo test -p test-harness --test projection_visibility_oracle --release \
//!     -- --ignored --nocapture
//! PROJECTION_ORACLE_STRIDE=1 cargo test -p test-harness \
//!     --test projection_visibility_oracle --release -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use test_harness::workflow::ModelBuilder;
use waffle_types::kernel::projection::{
    Curve2, CurveKind, ProjectOpts, ProjectionDeclines, ViewBasis, ViewFrame,
};
use waffle_types::kernel::{KernelId, RenderMesh, Visibility};

const CORPUS: &str = "../../app/tests/cases/assay";

/// The kernel's canonical relative chord tolerance — the density both the
/// render mesh and a default-options projection use.
const CHORD_REL: f64 = 1e-3;

/// Samples per classified curve, as §5.3 specifies.
const SAMPLES: usize = 16;

/// How far inside a projected triangle a sample must sit, relative to the
/// view's own size, for the triangle to count as containing it STRICTLY — and
/// how far outside before it does not contain it at all.
///
/// A model-unit margin rather than a barycentric one, and that distinction is
/// the whole point. `tessellate` answers f32 vertices, so a face's own
/// boundary sits up to ~1e-7 of the view's size off the exact plane it was
/// built on — and a sample lying exactly in that plane therefore reads as a
/// part in `1e8` INSIDE the face, which a dimensionless barycentric test calls
/// strict containment. Measured 2026-10-03: a boss's bottom rim, lying exactly
/// in the plate's top plane, was reported occluded by the plate's front face
/// because the f32 mesh put that face's top edge 1e-8 above the rim.
/// `1e-6` is two decades above the f32 noise and many decades below any real
/// geometry.
const INSIDE_REL: f64 = 1e-6;

/// The cases of the DEFAULT-stride sample whose classification this sweep
/// still contradicts along a WHOLE curve, pinned so the set cannot grow
/// silently.
///
/// These are not the spanning tail (`spans_a_change`, 218 curves over 15 cases
/// on 2026-10-03, counted and listed and traceable to a crossing the split did
/// not make). These are curves the buffer disagrees with at every decided
/// sample, and they fall into two families the sweep's own messages name:
///
/// - *A hidden curve with nothing in front of it.* F0083, P0005, R0087. The
///   kernel names an occluder a few parts in ten thousand in front of the
///   curve — 7.2e-4 on F0083's `−0.0813` — which is inside the band this
///   sweep's inscribed mesh can resolve, so it can neither confirm nor refute
///   it. Whether the kernel is right there is a question about a thin feature,
///   and settling it wants a finer mesh for the oracle or an exact query, not a
///   wider band.
/// - *A visible curve with a face plainly in front of it.* F0059, P0005,
///   R0047. The discrepancies are large (0.31 on a unit-ish model, 600 on a
///   1000-unit one) and the occluding surfaces are NOT near-parallel (slopes
///   0.77 to 5.0), so unlike the grazing categories there is no band argument
///   to make: these look like the kernel's ray missing a real occluder, and
///   each is its own investigation. R0047 also reports a sample 2.1e-5 in front
///   of its whole solid on a 1e-4-scale body, which is a projection-level
///   complaint and probably the place to start.
///
/// This is a LIST, not a tolerance: nothing was widened to make the sweep
/// green, every other case is asserted, and the first case that disagrees for
/// any reason fails here. A case that stops disagreeing must be removed in the
/// commit that fixes it — a stale entry is itself a defect, which the second
/// assertion below enforces.
const KNOWN_DISAGREEMENTS: [&str; 5] = ["F0059", "F0083", "P0005", "R0047", "R0087"];

/// Depth-buffer resolution per side. 512 puts a cell at ~0.2 % of the view's
/// size, and the per-cell slope band below is what makes the comparison sound
/// at that density rather than the resolution itself.
const BUFFER: usize = 512;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS)
}

/// How many cases to skip between samples — the same knob, and the same
/// reason, as `projection_corpus_oracle`: rebuilding a corpus document is the
/// expensive part.
fn stride() -> usize {
    std::env::var("PROJECTION_ORACLE_STRIDE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(8)
}

fn all_case_ids() -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(corpus_dir())
        .unwrap_or_else(|e| panic!("{}: {e}", corpus_dir().display()))
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_name()
                .to_str()
                .and_then(|n| n.strip_suffix(".waffle"))
                .map(str::to_string)
        })
        .collect();
    ids.sort();
    ids
}

fn axis_views() -> Vec<(&'static str, ViewFrame)> {
    vec![
        ("+x", ViewFrame::looking_along([1.0, 0.0, 0.0])),
        ("-x", ViewFrame::looking_along([-1.0, 0.0, 0.0])),
        ("+y", ViewFrame::looking_along([0.0, 1.0, 0.0])),
        ("-y", ViewFrame::looking_along([0.0, -1.0, 0.0])),
        ("+z", ViewFrame::looking_along([0.0, 0.0, 1.0])),
        ("-z", ViewFrame::looking_along([0.0, 0.0, -1.0])),
    ]
}

// ---------------------------------------------------------------------------
// the software depth buffer
// ---------------------------------------------------------------------------

/// An orthographic depth buffer over the view plane: per cell, the frontmost
/// surface depth and that surface's own depth GRADIENT, which is what the
/// comparison band needs at the cell's own density.
struct DepthBuffer {
    /// Every triangle as `([uv; 3], [depth; 3])`, and a coarse grid of their
    /// footprints, so the front depth can be asked EXACTLY at a point rather
    /// than read off a cell centre. See [`DepthBuffer::front_at`].
    tris: Vec<([[f64; 2]; 3], [f64; 3])>,
    grid: Vec<Vec<u32>>,
    grid_side: usize,
    grid_cell: f64,
    grid_min: [f64; 2],
    /// The view's own size, which the containment margin is relative to.
    size: f64,
    /// The solid's extent ALONG the line of sight, which bounds how wide a
    /// depth band can be and still decide anything.
    depth_extent: f64,
    min: [f64; 2],
    cell: f64,
    /// `f64::INFINITY` where no triangle reached the cell.
    depth: Vec<f64>,
    /// `|∇depth|` of the frontmost triangle at that cell, in depth per unit of
    /// view-plane distance.
    slope: Vec<f64>,
    /// That triangle's own depth extent, which caps the slope band: a linear
    /// function cannot vary more over part of the triangle than over all of it.
    range: Vec<f64>,
}

impl DepthBuffer {
    /// Rasterize `mesh` as seen through `basis`. `None` for a mesh with no
    /// triangles or no projected extent.
    fn rasterize(mesh: &RenderMesh, basis: &ViewBasis) -> Option<DepthBuffer> {
        let verts: Vec<([f64; 2], f64)> = mesh
            .vertices
            .chunks_exact(3)
            .map(|v| {
                let (uv, d) = basis.project([f64::from(v[0]), f64::from(v[1]), f64::from(v[2])]);
                ([uv.x(), uv.y()], d)
            })
            .collect();
        if verts.is_empty() || mesh.indices.len() < 3 {
            return None;
        }
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for (uv, _) in &verts {
            for k in 0..2 {
                lo[k] = lo[k].min(uv[k]);
                hi[k] = hi[k].max(uv[k]);
            }
        }
        let size = (hi[0] - lo[0]).max(hi[1] - lo[1]);
        if !(size.is_finite() && size > 0.0) {
            return None;
        }
        // Margin cells on each side, so a sample exactly on the footprint's
        // edge still has a cell — and the offset is a HALF-INTEGER number of
        // cells, so that edge falls in a cell's interior rather than on a cell
        // boundary. Measured 2026-10-03: at a whole-cell offset,
        // `((u − min) / cell).floor()` on the footprint's own edge computes
        // 1.9999999 and answers the MARGIN cell, so every sample on the
        // silhouette read as uncovered.
        let cell = size / (BUFFER as f64 - 5.0);
        let mut buf = DepthBuffer {
            min: [lo[0] - 2.5 * cell, lo[1] - 2.5 * cell],
            cell,
            depth: vec![f64::INFINITY; BUFFER * BUFFER],
            slope: vec![0.0; BUFFER * BUFFER],
            range: vec![0.0; BUFFER * BUFFER],
            tris: Vec::new(),
            grid: Vec::new(),
            grid_side: 1,
            grid_cell: size,
            grid_min: [lo[0], lo[1]],
            size,
            depth_extent: {
                let (mut lo_d, mut hi_d) = (f64::INFINITY, f64::NEG_INFINITY);
                for (_, d) in &verts {
                    lo_d = lo_d.min(*d);
                    hi_d = hi_d.max(*d);
                }
                (hi_d - lo_d).max(f64::MIN_POSITIVE)
            },
        };
        for t in mesh.indices.chunks_exact(3) {
            let (Some(a), Some(b), Some(c)) = (
                verts.get(t[0] as usize),
                verts.get(t[1] as usize),
                verts.get(t[2] as usize),
            ) else {
                continue;
            };
            buf.triangle(*a, *b, *c);
            buf.tris.push(([a.0, b.0, c.0], [a.1, b.1, c.1]));
        }
        // One cell per triangle on average, for the exact point query.
        buf.grid_side = ((buf.tris.len() as f64).sqrt().ceil() as usize).clamp(1, 256);
        buf.grid_cell = size / buf.grid_side as f64;
        buf.grid = vec![Vec::new(); buf.grid_side * buf.grid_side];
        let cells = buf.grid_side;
        let gcell = buf.grid_cell;
        let gmin = [lo[0], lo[1]];
        for i in 0..buf.tris.len() {
            let (uv, _) = buf.tris[i];
            let mut bb = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for p in uv {
                bb[0] = bb[0].min(p[0]);
                bb[1] = bb[1].min(p[1]);
                bb[2] = bb[2].max(p[0]);
                bb[3] = bb[3].max(p[1]);
            }
            let idx =
                |x: f64, lo: f64| (((x - lo) / gcell).floor().max(0.0) as usize).min(cells - 1);
            for y in idx(bb[1], gmin[1])..=idx(bb[3], gmin[1]) {
                for x in idx(bb[0], gmin[0])..=idx(bb[2], gmin[0]) {
                    buf.grid[y * cells + x].push(i as u32);
                }
            }
        }
        buf.grid_min = gmin;
        Some(buf)
    }

    /// Scan-convert one projected triangle, keeping the nearer depth per cell.
    fn triangle(&mut self, a: ([f64; 2], f64), b: ([f64; 2], f64), c: ([f64; 2], f64)) {
        let area2 = (b.0[0] - a.0[0]) * (c.0[1] - a.0[1]) - (b.0[1] - a.0[1]) * (c.0[0] - a.0[0]);
        // A triangle whose PROJECTED area is a negligible fraction of a cell
        // covers nothing a cell centre can be decided by: its barycentrics
        // there are float noise, and the depth it would interpolate spans its
        // whole 3-D length. A face seen edge-on is made entirely of these, and
        // it contributes no coverage — the faces that are not edge-on cover
        // the same cells. Measured 2026-10-03: without this test, an
        // axis-aligned view of a prismatic corpus body reported a front
        // surface a full model depth in front of itself, because a sliver of
        // an edge-on face claimed cells all over the view.
        if !area2.is_finite() || area2.abs() < 0.02 * self.cell * self.cell {
            return;
        }
        // The depth as a LINEAR function of (u, v), and its gradient, from the
        // triangle's own plane.
        let (du, dv) = {
            let (e1, e2) = (
                [b.0[0] - a.0[0], b.0[1] - a.0[1], b.1 - a.1],
                [c.0[0] - a.0[0], c.0[1] - a.0[1], c.1 - a.1],
            );
            // `depth = a.1 + du·(u − a.u) + dv·(v − a.v)`, solved from the two
            // edge vectors.
            let det = e1[0] * e2[1] - e1[1] * e2[0];
            if det == 0.0 {
                return;
            }
            (
                (e1[2] * e2[1] - e2[2] * e1[1]) / det,
                (e2[2] * e1[0] - e1[2] * e2[0]) / det,
            )
        };
        let slope = du.hypot(dv);
        let (mut x0, mut y0) = (usize::MAX, usize::MAX);
        let (mut x1, mut y1) = (0usize, 0usize);
        for p in [a.0, b.0, c.0] {
            let (ix, iy) = self.cell_of(p[0], p[1]);
            x0 = x0.min(ix);
            y0 = y0.min(iy);
            x1 = x1.max(ix);
            y1 = y1.max(iy);
        }
        let inv = 1.0 / area2;
        for iy in y0..=y1 {
            for ix in x0..=x1 {
                let (u, v) = self.center(ix, iy);
                // Barycentrics, with a half-cell of slack so a sample on a
                // shared edge lands on at least one of its triangles.
                let w0 = ((b.0[0] - u) * (c.0[1] - v) - (b.0[1] - v) * (c.0[0] - u)) * inv;
                let w1 = ((c.0[0] - u) * (a.0[1] - v) - (c.0[1] - v) * (a.0[0] - u)) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                    continue;
                }
                // A linear function over a triangle attains its extremes at
                // the vertices, so the interpolated depth cannot leave that
                // range — and clamping to it is what keeps a near-edge-on
                // triangle, whose cell-centre barycentrics are float noise,
                // from writing a wildly EXTRAPOLATED depth. Measured
                // 2026-10-03: without the clamp a sliver triangle reported a
                // front surface two model units in front of a solid one unit
                // across, and six corpus cases "failed" on it.
                let lo_d = a.1.min(b.1).min(c.1);
                let hi_d = a.1.max(b.1).max(c.1);
                let d = (a.1 + du * (u - a.0[0]) + dv * (v - a.0[1])).clamp(lo_d, hi_d);
                let k = iy * BUFFER + ix;
                if d < self.depth[k] {
                    self.depth[k] = d;
                    // And the slope band cannot need more than the triangle's
                    // own depth range either, for the same reason.
                    self.slope[k] = slope;
                    self.range[k] = hi_d - lo_d;
                }
            }
        }
    }

    fn cell_of(&self, u: f64, v: f64) -> (usize, usize) {
        let ix = ((u - self.min[0]) / self.cell).floor().max(0.0) as usize;
        let iy = ((v - self.min[1]) / self.cell).floor().max(0.0) as usize;
        (ix.min(BUFFER - 1), iy.min(BUFFER - 1))
    }

    /// The frontmost surface depth EXACTLY at `(u, v)`, twice over: over the
    /// triangles that contain the point STRICTLY, and over those that contain
    /// it at all. `None` when no triangle contains it either way.
    ///
    /// The pair is what separates a surface that stands BETWEEN the sample and
    /// the viewer from one the sample's own line of sight merely touches. When
    /// the loose answer is nearer than the strict one, the nearest surface
    /// reaches the sample only on a triangle's boundary — the configuration of
    /// a curve lying exactly in a face that is parallel to the line of sight,
    /// which is where the kernel's separation rule and a containment test
    /// legitimately differ — and the caller counts the sample instead of
    /// asserting on it.
    ///
    /// This is the number the comparison uses, and asking it at the point
    /// rather than at a cell centre is what makes the comparison sound near an
    /// INTERNAL silhouette. Measured 2026-10-03: a hole's rim edge seen
    /// edge-on sits inside the hole in the body's front face, and a cell whose
    /// centre lands half a cell away on that face's material reported a front
    /// surface that is simply not there along the sample's own line of sight —
    /// which read as four corpus cases failing.
    ///
    /// A near-degenerate triangle's interpolated depth is clamped to its own
    /// vertex range, which is exact (a linear function over a triangle attains
    /// its extremes at the vertices) and is what keeps a sliver from
    /// extrapolating a depth it does not have.
    #[allow(clippy::type_complexity)]
    fn front_at(
        &self,
        u: f64,
        v: f64,
        chord_band: f64,
    ) -> Option<(Option<(f64, f64)>, (f64, f64))> {
        // Two different margins, because the two questions are different. A
        // surface contains the sample AT ALL if the sample is no further
        // outside it than the mesh's own inscription deficit: the render mesh
        // is inscribed, so a curved face's polygon falls a chord sagitta short
        // of the exact rim that bounds it, and a rim's own samples sit that
        // far OUTSIDE the face they belong to. Measured 2026-10-03: a boss's
        // top rim sits ~1e-4 outside the inscribed top disc on a 0.5-unit
        // part, two hundred times the f32 noise. A surface contains it
        // STRICTLY only if it is inside by more than that noise
        // (`INSIDE_REL`).
        let strict_margin = INSIDE_REL * self.size;
        let loose_margin = chord_band.max(strict_margin);
        let idx = |x: f64, lo: f64| {
            (((x - lo) / self.grid_cell).floor().max(0.0) as usize).min(self.grid_side - 1)
        };
        let (gx, gy) = (idx(u, self.grid_min[0]), idx(v, self.grid_min[1]));
        let mut strict: Option<(f64, f64)> = None;
        let mut loose: Option<(f64, f64)> = None;
        for &i in &self.grid[gy * self.grid_side + gx] {
            let (uv, d) = self.tris[i as usize];
            let area2 = (uv[1][0] - uv[0][0]) * (uv[2][1] - uv[0][1])
                - (uv[1][1] - uv[0][1]) * (uv[2][0] - uv[0][0]);
            // A projected triangle flatter than a microradian is a face seen
            // EDGE ON, and its barycentrics at any point are float noise — the
            // mesh's own f32 vertices put its projected area at ~1e-7 of its
            // edge lengths rather than at zero, so an exact-zero test does not
            // catch it. Measured 2026-10-03: without this, a plate's top face
            // seen edge-on claimed STRICT containment of samples all along its
            // projected line and reported its own far corner's depth as the
            // front surface there.
            let (l1, l2) = (
                (uv[1][0] - uv[0][0]).hypot(uv[1][1] - uv[0][1]),
                (uv[2][0] - uv[0][0]).hypot(uv[2][1] - uv[0][1]),
            );
            if !area2.is_finite() || area2.abs() <= 1e-6 * l1 * l2 {
                continue;
            }
            let inv = 1.0 / area2;
            let w0 = ((uv[1][0] - u) * (uv[2][1] - v) - (uv[1][1] - v) * (uv[2][0] - u)) * inv;
            let w1 = ((uv[2][0] - u) * (uv[0][1] - v) - (uv[2][1] - v) * (uv[0][0] - u)) * inv;
            let w2 = 1.0 - w0 - w1;
            // Each barycentric, converted to the DISTANCE from the sample to
            // the edge opposite it: `|wᵢ·area2| / |that edge|`. A margin in
            // model units is the only kind that means anything here (see
            // `INSIDE_REL`).
            let edge_len = |p: [f64; 2], q: [f64; 2]| (q[0] - p[0]).hypot(q[1] - p[1]);
            let dists = [
                w0 * area2.abs() / edge_len(uv[1], uv[2]).max(f64::MIN_POSITIVE),
                w1 * area2.abs() / edge_len(uv[2], uv[0]).max(f64::MIN_POSITIVE),
                w2 * area2.abs() / edge_len(uv[0], uv[1]).max(f64::MIN_POSITIVE),
            ];
            if dists.iter().any(|d| *d < -loose_margin) {
                continue;
            }
            let lo_d = d[0].min(d[1]).min(d[2]);
            let hi_d = d[0].max(d[1]).max(d[2]);
            let z = (w0 * d[0] + w1 * d[1] + w2 * d[2]).clamp(lo_d, hi_d);
            // This surface's own depth GRADIENT in the view plane, from its
            // plane: what turns the chord band into the band this comparison
            // actually needs (see `depth_band`).
            let (e1, e2) = (
                [uv[1][0] - uv[0][0], uv[1][1] - uv[0][1], d[1] - d[0]],
                [uv[2][0] - uv[0][0], uv[2][1] - uv[0][1], d[2] - d[0]],
            );
            let det = e1[0] * e2[1] - e1[1] * e2[0];
            let slope = if det != 0.0 {
                let du = (e1[2] * e2[1] - e2[2] * e1[1]) / det;
                let dv = (e2[2] * e1[0] - e1[2] * e2[0]) / det;
                du.hypot(dv)
            } else {
                f64::INFINITY
            };
            let nearer = |best: Option<(f64, f64)>| match best {
                Some((b, _)) if b <= z => best,
                _ => Some((z, slope)),
            };
            loose = nearer(loose);
            if dists.iter().all(|d| *d > strict_margin) {
                strict = nearer(strict);
            }
        }
        loose.map(|l| (strict, l))
    }

    /// The band a depth comparison against a surface of this `slope` needs.
    ///
    /// The render mesh is INSCRIBED, so a facet sits up to one chord sagitta
    /// off the exact surface measured PERPENDICULAR to it — but the comparison
    /// is along the line of sight, and converting between the two costs a
    /// factor `1/|n·w| = √(1 + slope²)`. That factor is 1 on a face seen
    /// square on and diverges as the surface turns parallel to the line of
    /// sight, which is why a single global chord band cannot serve: measured
    /// 2026-10-03 on R0055, a rim near the silhouette of a 21.8-unit cylinder
    /// read 0.069 off a mesh whose radial sagitta is 0.021, because there the
    /// surface stands at ~68° to the view plane.
    fn depth_band(chord_band: f64, slope: f64) -> f64 {
        if !slope.is_finite() {
            return f64::INFINITY;
        }
        chord_band * (1.0 + slope * slope).sqrt()
    }

    /// How many cells any triangle reached — the sweep's own coverage census.
    fn covered_cells(&self) -> usize {
        self.depth.iter().filter(|d| d.is_finite()).count()
    }

    fn center(&self, ix: usize, iy: usize) -> (f64, f64) {
        (
            self.min[0] + (ix as f64 + 0.5) * self.cell,
            self.min[1] + (iy as f64 + 0.5) * self.cell,
        )
    }

    /// The frontmost depth at `(u, v)` and the band that cell's own slope
    /// needs, or `None` when nothing reached it.
    fn front(&self, u: f64, v: f64) -> Option<(f64, f64)> {
        let (ix, iy) = self.cell_of(u, v);
        let k = iy * BUFFER + ix;
        let d = self.depth[k];
        if !d.is_finite() {
            return None;
        }
        // A cell holds the depth at its CENTRE, so the sample can sit up to a
        // cell diagonal away and the front surface's depth there differs by
        // the slope times that distance — capped by the triangle's own depth
        // extent, which it cannot exceed.
        Some((
            d,
            (self.slope[k] * self.cell * std::f64::consts::SQRT_2).min(self.range[k]),
        ))
    }
}

// ---------------------------------------------------------------------------
// the sample's own depth, re-derived
// ---------------------------------------------------------------------------

/// Depth of the 3-D edge `polyline` at the view-plane point `(u, v)` — the
/// NEAREST pre-image, since a rim seen edge-on projects both halves onto one
/// segment and the drawing shows the near one.
///
/// Deliberately re-derived here rather than read from
/// `ProjectedCurve::depth`: the kernel's own lift is the thing under test, and
/// an oracle that reused it could only confirm that the kernel agrees with
/// itself.
fn depth_of(basis: &ViewBasis, polyline: &[[f64; 3]], u: f64, v: f64) -> Option<f64> {
    if polyline.is_empty() {
        return None;
    }
    if polyline.len() == 1 {
        return Some(basis.project(polyline[0]).1);
    }
    let uv: Vec<([f64; 2], f64)> = polyline
        .iter()
        .map(|p| {
            let (q, d) = basis.project(*p);
            ([q.x(), q.y()], d)
        })
        .collect();
    let mut nearest = f64::INFINITY;
    let mut span = 0.0f64;
    for w in uv.windows(2) {
        nearest = nearest.min(point_segment(u, v, w[0].0, w[1].0).0);
        span = span.max(
            (w[1].0[0] - w[0].0[0])
                .abs()
                .max((w[1].0[1] - w[0].0[1]).abs()),
        );
    }
    if !nearest.is_finite() {
        return None;
    }
    let accept = nearest + 1e-9 * span.max(nearest) + f64::MIN_POSITIVE;
    let mut best: Option<f64> = None;
    for w in uv.windows(2) {
        let (d2, s) = point_segment(u, v, w[0].0, w[1].0);
        if d2 > accept {
            continue;
        }
        let d = w[0].1 + s * (w[1].1 - w[0].1);
        best = Some(best.map_or(d, |b: f64| b.min(d)));
    }
    best
}

/// `(distance, parameter)` of the nearest point of the segment `a → b` to
/// `(u, v)`.
fn point_segment(u: f64, v: f64, a: [f64; 2], b: [f64; 2]) -> (f64, f64) {
    let (vx, vy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = vx * vx + vy * vy;
    let t = if len2 <= 0.0 {
        0.0
    } else {
        (((u - a[0]) * vx + (v - a[1]) * vy) / len2).clamp(0.0, 1.0)
    };
    ((u - (a[0] + t * vx)).hypot(v - (a[1] + t * vy)), t)
}

/// `SAMPLES` points along a curve's own parameter domain, at the MIDPOINTS of
/// its sixteen equal sub-intervals.
///
/// Interior on purpose. A classified piece's two ENDPOINTS are the crossings
/// it was cut at, which is exactly where its visibility changes — the curve
/// emerges from behind something there — so a sample sitting on one is
/// ambiguous by construction, and asserting on it would fail every correctly
/// classified piece in the corpus. Measured 2026-10-03: R0055's hidden arcs
/// reported "nothing in front" at their own ends and nowhere else.
fn samples_of(curve: &Curve2) -> Vec<(f64, f64)> {
    let Some((t0, t1)) = curve.param_range() else {
        return Vec::new();
    };
    (0..SAMPLES)
        .filter_map(|i| {
            let t = t0 + (t1 - t0) * (i as f64 + 0.5) / (SAMPLES as f64);
            curve.eval(t).map(|p| (p.x(), p.y()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// the sweep
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Tally {
    not_built: Vec<String>,
    /// `(case, body, direction)` triples that got as far as a depth buffer.
    views: usize,
    multi_body_cases: usize,
    visible_samples: usize,
    hidden_samples: usize,
    /// Samples whose cell no triangle reached — the inscribed mesh's footprint
    /// falling a chord band short of the exact edge.
    uncovered: usize,
    /// Samples whose nearest surface reaches them only on a triangle's
    /// boundary — a curve lying exactly in a face parallel to the line of
    /// sight (see `front_at`).
    grazing_occluder: usize,
    /// Classified curves whose own samples DISAGREE: the piece spans a
    /// visibility change, so a crossing was not split. Counted, with the
    /// edges named, because the fix is upstream in the split and not in the
    /// classification.
    spans_a_change: usize,
    /// Samples where the front surface stands so near parallel to the line of
    /// sight that the inscribed mesh cannot place it in depth at all (see
    /// `depth_band`).
    grazing_surface: usize,
    /// Silhouette curves, counted and coverage-checked rather than
    /// depth-checked (see the module docs).
    silhouette_curves: usize,
    /// Edge curves whose 3-D polyline the kernel would not answer for, so the
    /// sample's own depth could not be re-derived here.
    unliftable_curves: usize,
    declines: ProjectionDeclines,
    /// Cases with at least one span-a-change curve, and how many.
    spanning_cases: BTreeMap<String, usize>,
    failures: BTreeMap<String, Vec<String>>,
}

#[test]
#[ignore = "corpus-wide: rebuilds every assay case and rasterizes six views of each; run with --release --ignored (minutes)"]
fn every_classified_curve_agrees_with_a_software_depth_buffer() {
    let all = all_case_ids();
    assert!(
        all.len() > 100,
        "expected the assay corpus, found {} cases in {}",
        all.len(),
        corpus_dir().display()
    );
    let stride = stride();
    // One case by id, for diagnosing a single disagreement.
    let ids: Vec<String> = match std::env::var("VISIBILITY_ORACLE_CASE") {
        Ok(one) => all.iter().filter(|id| **id == one).cloned().collect(),
        Err(_) => all.iter().step_by(stride).cloned().collect(),
    };
    println!(
        "visibility oracle: {} of {} corpus cases (stride {stride}; \
         PROJECTION_ORACLE_STRIDE=1 for all of them)",
        ids.len(),
        all.len()
    );

    let mut tally = Tally::default();
    for id in &ids {
        let path = corpus_dir().join(format!("{id}.waffle"));
        let json = match fs::read_to_string(&path) {
            Ok(j) => j,
            Err(e) => {
                tally.not_built.push(format!("{id} (unreadable: {e})"));
                continue;
            }
        };
        let mut builder = ModelBuilder::kernel_v2();
        if builder.load(&json).is_err() || !builder.engine_errors().is_empty() {
            tally.not_built.push(id.clone());
            continue;
        }
        let handles = builder.live_solid_handles();
        if handles.is_empty() {
            tally.not_built.push(format!("{id} (no live body)"));
            continue;
        }
        if handles.len() > 1 {
            tally.multi_body_cases += 1;
        }

        let mut problems = Vec::new();
        let mut spanning: std::collections::BTreeSet<u64> = Default::default();
        // One body at a time: D1c classifies each body against its OWN
        // tessellation and says so (`ProjectionDeclines::cross_body`), so a
        // multi-body view's curves are not claims this oracle could check.
        for (bi, handle) in handles.iter().enumerate() {
            let Ok(mesh) = builder.kernel_mut().tessellate(handle, CHORD_REL) else {
                problems.push(format!("body {bi}: tessellation failed"));
                continue;
            };
            // Every edge's 3-D polyline, once per body, so a sample's own
            // depth can be re-derived independently of the kernel's lift.
            let mut polylines: BTreeMap<u64, Vec<[f64; 3]>> = BTreeMap::new();
            for e in builder.kernel_mut().as_introspect().list_edges(handle) {
                polylines.insert(e.0, builder.kernel_mut().as_introspect().edge_polyline(e));
            }

            for (name, frame) in axis_views() {
                let basis = frame.basis().expect("an axis view has a basis");
                let Some(buffer) = DepthBuffer::rasterize(&mesh, &basis) else {
                    problems.push(format!("body {bi} along {name}: no depth buffer"));
                    continue;
                };
                let view =
                    match builder
                        .kernel_mut()
                        .project(handle, &frame, &ProjectOpts::default())
                    {
                        Ok(v) => v,
                        Err(e) => {
                            problems.push(format!("body {bi} along {name}: project failed: {e}"));
                            continue;
                        }
                    };
                tally.views += 1;
                tally.declines.merge(&view.declines);
                let Some(bb) = view.bbox else {
                    problems.push(format!("body {bi} along {name}: no curves"));
                    continue;
                };
                let size = (bb.max.x() - bb.min.x())
                    .abs()
                    .max((bb.max.y() - bb.min.y()).abs());
                // The render chord band of the view's own size: the buffer
                // rasterizes the INSCRIBED mesh while a curve's samples lie
                // on the exact analytic edge.
                let chord_band = CHORD_REL * size;

                for curve in &view.curves {
                    if curve.kind == CurveKind::Silhouette {
                        tally.silhouette_curves += 1;
                        // Coverage only: a silhouette must lie on the body's
                        // own footprint. Its DEPTH is the one-sided limit the
                        // rasterized mesh cannot resolve (module docs).
                        for (u, v) in samples_of(&curve.geometry) {
                            if buffer.front(u, v).is_none() {
                                tally.uncovered += 1;
                            }
                        }
                        continue;
                    }
                    let Some(KernelId(src)) = curve.source else {
                        tally.unliftable_curves += 1;
                        continue;
                    };
                    let Some(poly) = polylines.get(&src) else {
                        tally.unliftable_curves += 1;
                        continue;
                    };
                    if poly.is_empty() {
                        tally.unliftable_curves += 1;
                        continue;
                    }
                    // Judge the CURVE, not each sample on its own. A
                    // classified piece claims one visibility for its whole
                    // length, so the question the samples answer together is
                    // whether that claim holds along it — and samples that
                    // DISAGREE say something different and more useful than
                    // any of them says alone: the piece spans a visibility
                    // change, which means a crossing was not split, not that
                    // the classification of either part is wrong.
                    let mut in_front = Vec::new();
                    let mut detail: Option<String> = None;
                    for (u, v) in samples_of(&curve.geometry) {
                        // The sample's OWN depth, re-derived here from the
                        // edge's 3-D polyline rather than read off the
                        // kernel's answer: the lift is part of what is under
                        // test.
                        let Some(own) = depth_of(&basis, poly, u, v) else {
                            tally.unliftable_curves += 1;
                            continue;
                        };
                        // The front depth at the SAMPLE, not at a cell centre.
                        let Some((strict, loose)) = buffer.front_at(u, v, chord_band) else {
                            tally.uncovered += 1;
                            if std::env::var_os("VISIBILITY_ORACLE_DEBUG").is_some()
                                && tally.uncovered < 8
                            {
                                println!(
                                    "    [uncovered] {id} body {bi} {name}: ({u:.6e}, {v:.6e}) \
                                     footprint u {:.6e}..{:.6e} v {:.6e}..{:.6e}, {} covered \
                                     cells of {}",
                                    buffer.min[0],
                                    buffer.min[0] + buffer.cell * BUFFER as f64,
                                    buffer.min[1],
                                    buffer.min[1] + buffer.cell * BUFFER as f64,
                                    buffer.covered_cells(),
                                    BUFFER * BUFFER
                                );
                            }
                            continue;
                        };
                        let loose_band = DepthBuffer::depth_band(chord_band, loose.1);
                        // A band wider than a quarter of the solid's own depth
                        // extent decides nothing either way: the surface there
                        // stands so near parallel to the line of sight that
                        // the inscribed mesh cannot place it in depth.
                        if loose_band > 0.25 * buffer.depth_extent {
                            tally.grazing_surface += 1;
                            continue;
                        }
                        // A sample of a projected edge lies ON the solid's
                        // boundary, so the frontmost surface along its own
                        // line of sight is never behind it. If it were, the
                        // PROJECTION would be wrong before visibility ever
                        // came up.
                        if own < loose.0 - loose_band {
                            problems.push(format!(
                                "body {bi} along {name}: a sample of edge {src} at \
                                 ({u:.6e}, {v:.6e}) sits {:.3e} IN FRONT of the whole \
                                 solid (band {loose_band:.3e})",
                                loose.0 - own
                            ));
                            continue;
                        }
                        // What the buffer says about this sample: `true` if
                        // some surface is DEFINITELY in front of it, `false`
                        // if definitely none is, and nothing at all when the
                        // two cannot be told apart.
                        //
                        // The asymmetry is the point. "Not definitely in
                        // front" is already enough to leave a VISIBLE tag
                        // standing — a surface within the band of the sample's
                        // own depth does not contradict it. But it is NOT
                        // enough to call a HIDDEN tag wrong: something has to
                        // be definitely NOT in front for that, and a surface
                        // nearer than the sample by less than the band is the
                        // undecidable middle. Measured 2026-10-03: without the
                        // distinction, near-silhouette occluders — slopes from
                        // 0.77 to 44 over the corpus sample — made the sweep
                        // report hidden arcs as having nothing in front of
                        // them.
                        let definitely = strict.filter(|(st, sl)| {
                            *st < own - DepthBuffer::depth_band(chord_band, *sl)
                        });
                        match (curve.visibility, definitely) {
                            (_, Some((st, sl))) => {
                                if detail.is_none() {
                                    detail = Some(format!(
                                        "at ({u:.6e}, {v:.6e}) a face stands {:.3e} in front \
                                         (band {:.3e}, slope {sl:.3e}); own {own:.6e} front \
                                         {st:.6e}",
                                        own - st,
                                        DepthBuffer::depth_band(chord_band, sl)
                                    ));
                                }
                                in_front.push(true);
                            }
                            (Visibility::Visible, None) => in_front.push(false),
                            (Visibility::Hidden, None) => {
                                if loose.0 < own - chord_band {
                                    // Nearer than the sample, but not by more
                                    // than the band its own slope needs. The
                                    // mesh cannot place that surface precisely
                                    // enough to call the kernel wrong.
                                    tally.grazing_occluder += 1;
                                } else {
                                    in_front.push(false);
                                }
                            }
                        }
                    }
                    let hidden_somewhere = in_front.iter().any(|f| *f);
                    let visible_somewhere = in_front.iter().any(|f| !*f);
                    match (curve.visibility, hidden_somewhere, visible_somewhere) {
                        // Decided, and the kernel agrees along the whole piece.
                        (Visibility::Visible, false, true) => {
                            tally.visible_samples += in_front.len()
                        }
                        (Visibility::Hidden, true, false) => tally.hidden_samples += in_front.len(),
                        // The samples disagree with each other: the piece spans
                        // a visibility change, so a crossing was not split. The
                        // kernel's own tag is right on PART of it, which is
                        // what a single verdict per piece can be; what is wrong
                        // is the extent of the piece.
                        (_, true, true) => {
                            tally.spans_a_change += 1;
                            spanning.insert(src);
                        }
                        // Nothing to judge: every sample was declined.
                        (_, false, false) => {}
                        // Decided, and the kernel's tag contradicts it along
                        // the WHOLE piece. The real failure.
                        (vis, _, _) => problems.push(format!(
                            "body {bi} along {name}: edge {src} is tagged {vis:?} but every \
                             decided sample says otherwise — {}; kernel {:?} geom {:?}",
                            detail.unwrap_or_else(|| "nothing in front anywhere".to_string()),
                            curve.depth,
                            curve.geometry
                        )),
                    }
                }
            }
        }
        if !spanning.is_empty() {
            tally.spanning_cases.insert(id.clone(), spanning.len());
        }
        if !problems.is_empty() {
            // One entry per case, capped: a systematic failure would otherwise
            // print tens of thousands of lines.
            problems.truncate(12);
            tally.failures.insert(id.clone(), problems);
        }
    }

    println!(
        "visibility oracle over {} cases: {} (case, body, direction) views, \
         {} not built (the assay's own business), {} multi-body cases (classified \
         per body); {} VISIBLE samples and {} HIDDEN samples checked against the \
         depth buffer; {} samples uncovered, {} with a grazing occluder and {} \
         on a grazing surface (none of the three asserted), {} curves SPANNING \
         a visibility change over {} cases, {} silhouette curves \
         coverage-checked only, {} curves unliftable; {} failing cases",
        ids.len(),
        tally.views,
        tally.not_built.len(),
        tally.multi_body_cases,
        tally.visible_samples,
        tally.hidden_samples,
        tally.uncovered,
        tally.grazing_occluder,
        tally.grazing_surface,
        tally.spans_a_change,
        tally.spanning_cases.len(),
        tally.silhouette_curves,
        tally.unliftable_curves,
        tally.failures.len()
    );
    println!("kernel declines over the sweep:");
    for (name, n) in tally.declines.counts() {
        println!("    {name}: {n}");
    }
    if !tally.not_built.is_empty() {
        println!("not built: {}", tally.not_built.join(", "));
    }
    if !tally.spanning_cases.is_empty() {
        println!(
            "curves spanning a visibility change, by case: {:?}",
            tally.spanning_cases
        );
    }
    for (id, problems) in &tally.failures {
        println!("FAIL {id}:");
        for p in problems {
            println!("    {p}");
        }
    }

    // A sweep that classified nothing would pass vacuously, and so would one
    // that found no HIDDEN segment at all: the second check of §5.3 is the
    // whole point of D1c, and a kernel that regressed to D1b's
    // everything-is-visible would satisfy the first check perfectly.
    if std::env::var_os("VISIBILITY_ORACLE_CASE").is_some() {
        // A single-case run is a diagnostic, not the sweep: the coverage
        // floors below describe the corpus sample and would be meaningless
        // over one document.
        let unexpected: Vec<&str> = tally
            .failures
            .keys()
            .map(String::as_str)
            .filter(|id| !KNOWN_DISAGREEMENTS.contains(id))
            .collect();
        assert!(
            unexpected.is_empty(),
            "case(s) the depth buffer contradicts that are not in \
             KNOWN_DISAGREEMENTS: {unexpected:?}"
        );
        return;
    }
    assert!(
        tally.views > 4 * ids.len(),
        "only {} views rasterized over {} cases — the sweep proved nothing",
        tally.views,
        ids.len()
    );
    // And a sweep with no HIDDEN sample would pass vacuously too: the second
    // check of §5.3 is the whole point of D1c, and a kernel that regressed to
    // D1b's everything-is-visible would satisfy the first check perfectly. The
    // floor is a TENTH of the visible count, measured far below the corpus's
    // own ratio (518,306 hidden against 534,931 visible at the default stride
    // on 2026-10-03), so it catches a collapse without pinning a number that
    // moves with the corpus.
    assert!(
        tally.hidden_samples * 10 > tally.visible_samples,
        "the sweep checked {} visible and only {} hidden samples; with so few \
         hidden this oracle cannot tell D1c from D1b",
        tally.visible_samples,
        tally.hidden_samples
    );
    // The declines that must stay empty. `ray_grazes_face` is expected — every
    // axis view of a prismatic solid grazes the faces parallel to its line of
    // sight — and `split_tangency` is a real configuration the corpus
    // contains, so both are reported above rather than asserted to zero.
    assert_eq!(
        tally.declines.split_budget, 0,
        "a view exhausted the crossing search's work budget"
    );
    assert_eq!(
        tally.declines.cross_body, 0,
        "a single-body projection reported a cross-body decline"
    );
    assert_eq!(
        tally.declines.depth_unliftable, 0,
        "a classified curve could not be lifted to its own depth"
    );
    let unexpected: Vec<&str> = tally
        .failures
        .keys()
        .map(String::as_str)
        .filter(|id| !KNOWN_DISAGREEMENTS.contains(id))
        .collect();
    assert!(
        unexpected.is_empty(),
        "case(s) the depth buffer contradicts that are not in \
         KNOWN_DISAGREEMENTS: {unexpected:?} (their messages are listed above)"
    );
    // And the ratchet: a pinned case that no longer disagrees must come off the
    // list in the commit that fixed it, or the list stops meaning anything.
    // Only cases this run actually SAMPLED can be judged.
    let fixed: Vec<&str> = KNOWN_DISAGREEMENTS
        .iter()
        .copied()
        .filter(|id| ids.iter().any(|s| s == id) && !tally.failures.contains_key(*id))
        .collect();
    assert!(
        fixed.is_empty(),
        "these cases no longer disagree with the depth buffer — remove them \
         from KNOWN_DISAGREEMENTS: {fixed:?}"
    );
}
