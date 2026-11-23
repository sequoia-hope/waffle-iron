//! Rigid-motion sweep of one corpus case: does the kernel give the same
//! result when the whole document is rotated and translated?
//!
//! The corpus, the prospector seeds and the suites hold each case in the
//! orientation it was authored in. A mechanism built against rounding, or a
//! defect that only rounding exposes, is invisible there: an axis-aligned
//! plane holds a coincident edge exactly, an oblique one cannot. This probe
//! moves a case by `SWEEP_N` seeded rigid motions (the prospector's own
//! transformer, `prospect::metamorphic`) and reads each moved build against
//! the unmoved one.
//!
//! The verdict is the KERNEL's, not the categorizer's (whose exact-membership
//! lattice is rotation-dependent on revolves). Per motion, one line:
//!
//! - `OK`: no engine error, no failed auto-union, and the same body count,
//!   per-body B-Rep Euler characteristic, volume and place as the unmoved
//!   build. Volume is exact where the kernel has a closed form (compared at
//!   1e-9 relative) and the render mesh's otherwise (compared at 1e-2, the
//!   mesh's own orientation noise). Place is `geo`: the moved render mesh
//!   carried back by the inverse motion, and the largest distance from a
//!   vertex of either mesh to the other's surface, as a fraction of the
//!   unmoved mesh's bounding-box diagonal — at most `GEO_BAND`.
//! - `ERR`: an engine error, or an auto-union that failed and left its
//!   operands as separate bodies (a warning in the app). Loud.
//! - `DIFF`: built without complaint and differs. Read these first: a
//!   different volume is a wrong model.
//!
//! ```text
//! SWEEP_CASE=P0054 SWEEP_N=4 cargo test -p test-harness \
//!   --test rigid_motion_sweep --release sweep -- --ignored --nocapture
//! ```
//!
//! `steps` builds the document truncated after each operation, unmoved and
//! under motion `SWEEP_K`, to find the first operation whose result differs.
//!
//! Use: before removing a kernel mechanism, run the cases it acts on (or the
//! whole corpus: about ten minutes at 14 jobs, four motions each) from a
//! tree with the mechanism and a tree without, and compare line by line.
//! Recorded findings: `docs/yang_tail_triage.md`, 2026-11-13.

use serde_json::Value;
use test_harness::assay::prospect::gen3::Rng;
use test_harness::assay::prospect::metamorphic::{rigid_motion, with_explicit_x_axes, RigidMotion};
use test_harness::assay::prospect::mutate::characteristic_length;
use test_harness::assay::volume_oracle_doc::truncate_ops;
use test_harness::ModelBuilder;

type Tri = [[f64; 3]; 3];

/// What one build measured.
struct Measure {
    /// Engine errors, then failed auto-unions.
    errs: Vec<String>,
    bodies: usize,
    /// Per live body, B-Rep V − E + F, sorted.
    chi: Vec<i64>,
    exact_volume: Result<f64, String>,
    mesh_volume: Result<f64, String>,
    /// The render meshes of every live body, as triangles.
    tris: Vec<Tri>,
    /// The canonical geometry hash of the live bodies
    /// (`assay::geometry_hash`), in this build's own frame: it compares two
    /// KERNELS on one motion, never two motions.
    hash: u64,
}

fn triangles(meshes: &[waffle_types::kernel::RenderMesh]) -> Vec<Tri> {
    let mut out = Vec::new();
    for m in meshes {
        let at = |i: u32| -> Option<[f64; 3]> {
            let k = i as usize * 3;
            let v = m.vertices.get(k..k + 3)?;
            Some([v[0] as f64, v[1] as f64, v[2] as f64])
        };
        for t in m.indices.chunks_exact(3) {
            if let (Some(a), Some(b), Some(c)) = (at(t[0]), at(t[1]), at(t[2])) {
                out.push([a, b, c]);
            }
        }
    }
    out
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Squared distance from `p` to the triangle `t` (Ericson, *Real-Time
/// Collision Detection* §5.1.5: the closest point by Voronoi region).
fn point_triangle_distance2(p: [f64; 3], t: &Tri) -> f64 {
    let [a, b, c] = *t;
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let along =
        |o: [f64; 3], d: [f64; 3], s: f64| [o[0] + s * d[0], o[1] + s * d[1], o[2] + s * d[2]];
    let d2 = |q: [f64; 3]| dot(sub(p, q), sub(p, q));
    let (d1, d2a) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2a <= 0.0 {
        return d2(a);
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return d2(b);
    }
    let vc = d1 * d4 - d3 * d2a;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return d2(along(a, ab, d1 / (d1 - d3)));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return d2(c);
    }
    let vb = d5 * d2a - d1 * d6;
    if vb <= 0.0 && d2a >= 0.0 && d6 <= 0.0 {
        return d2(along(a, ac, d2a / (d2a - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return d2(along(b, sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let denom = va + vb + vc;
    if denom == 0.0 {
        return d2(a).min(d2(b)).min(d2(c));
    }
    let q = along(along(a, ab, vb / denom), ac, vc / denom);
    d2(q)
}

/// The triangles of a mesh in a uniform grid, for nearest-surface queries.
struct SurfaceGrid<'a> {
    tris: &'a [Tri],
    lo: [f64; 3],
    h: f64,
    n: [usize; 3],
    cells: Vec<Vec<u32>>,
}

impl<'a> SurfaceGrid<'a> {
    fn new(tris: &'a [Tri]) -> Self {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for p in tris.iter().flatten() {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let span = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max);
        let h = (span / 48.0).max(f64::MIN_POSITIVE);
        let n = [0, 1, 2].map(|k| (((hi[k] - lo[k]) / h).floor() as usize + 1).min(64));
        let mut grid = Self {
            tris,
            lo,
            h,
            n,
            cells: vec![Vec::new(); n[0] * n[1] * n[2]],
        };
        for (ti, t) in tris.iter().enumerate() {
            let mut a = [usize::MAX; 3];
            let mut b = [0usize; 3];
            for p in t {
                let c = grid.cell(*p);
                for k in 0..3 {
                    a[k] = a[k].min(c[k]);
                    b[k] = b[k].max(c[k]);
                }
            }
            for i in a[0]..=b[0] {
                for j in a[1]..=b[1] {
                    for k in a[2]..=b[2] {
                        let at = grid.index([i, j, k]);
                        grid.cells[at].push(ti as u32);
                    }
                }
            }
        }
        grid
    }

    /// The cell holding `p`, clamped into the grid.
    fn cell(&self, p: [f64; 3]) -> [usize; 3] {
        [0, 1, 2].map(|k| {
            let c = ((p[k] - self.lo[k]) / self.h).floor();
            if c.is_nan() || c < 0.0 {
                0
            } else {
                (c as usize).min(self.n[k] - 1)
            }
        })
    }

    fn index(&self, c: [usize; 3]) -> usize {
        (c[0] * self.n[1] + c[1]) * self.n[2] + c[2]
    }

    /// Distance from `p` to the nearest triangle: rings of cells about `p`'s
    /// own, outward, until no farther ring can hold a nearer triangle.
    fn distance(&self, p: [f64; 3]) -> f64 {
        let c = self.cell(p);
        let mut best = f64::INFINITY;
        let reach = self.n[0].max(self.n[1]).max(self.n[2]);
        for r in 0..=reach {
            // Every cell at ring r is at least (r − 1)·h from `p`.
            if r >= 1 && best.sqrt() <= (r as f64 - 1.0) * self.h {
                break;
            }
            let range = |k: usize| c[k].saturating_sub(r)..=(c[k] + r).min(self.n[k] - 1);
            for i in range(0) {
                for j in range(1) {
                    for k in range(2) {
                        let ring = i.abs_diff(c[0]).max(j.abs_diff(c[1])).max(k.abs_diff(c[2]));
                        if ring != r {
                            continue;
                        }
                        for &ti in &self.cells[self.index([i, j, k])] {
                            best = best.min(point_triangle_distance2(p, &self.tris[ti as usize]));
                        }
                    }
                }
            }
        }
        best.sqrt()
    }
}

/// The diagonal of a mesh's bounding box.
fn extent(tris: &[Tri]) -> f64 {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in tris.iter().flatten() {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    dot(sub(hi, lo), sub(hi, lo)).sqrt()
}

/// The largest distance from a vertex of either mesh to the other mesh's
/// surface, with `moved` carried back by the inverse of `m`. Two builds of
/// the same solid differ by their chord tolerances at most; a face one of
/// them lacks, or a sliver one of them adds, stands off by its own size.
fn surface_distance(reference: &[Tri], moved: &[Tri], m: &RigidMotion) -> Option<f64> {
    if reference.is_empty() || moved.is_empty() {
        return None;
    }
    let back = |p: [f64; 3]| -> [f64; 3] {
        let q = sub(p, m.t);
        [0, 1, 2].map(|k| m.r[0][k] * q[0] + m.r[1][k] * q[1] + m.r[2][k] * q[2])
    };
    let moved: Vec<Tri> = moved.iter().map(|t| t.map(back)).collect();
    let one_sided = |from: &[Tri], onto: &[Tri]| -> f64 {
        let grid = SurfaceGrid::new(onto);
        from.iter()
            .flatten()
            .map(|&p| grid.distance(p))
            .fold(0.0, f64::max)
    };
    Some(one_sided(reference, &moved).max(one_sided(&moved, reference)))
}

fn measure(doc: &Value) -> Measure {
    let waffle = serde_json::to_string(doc).unwrap();
    let mut b = ModelBuilder::kernel_v2();
    if let Err(e) = b.load(&waffle) {
        return Measure {
            errs: vec![format!("load: {e}")],
            bodies: 0,
            chi: vec![],
            exact_volume: Err("load".into()),
            mesh_volume: Err("load".into()),
            tris: Vec::new(),
            hash: 0,
        };
    }
    let handles = b.live_solid_handles();
    let k = b.kernel_ref().as_introspect();
    let mut chi: Vec<i64> = handles
        .iter()
        .map(|h| {
            k.list_vertices(h).len() as i64 - k.list_edges(h).len() as i64
                + k.list_faces(h).len() as i64
        })
        .collect();
    chi.sort_unstable();
    let exact_volume = handles
        .iter()
        .map(|h| k.solid_volume(h).map_err(|e| e.to_string()))
        .sum::<Result<f64, String>>();
    let mut errs: Vec<String> = b.engine_errors().iter().map(|e| format!("{e:?}")).collect();
    errs.extend(
        b.engine_warnings()
            .iter()
            .filter(|w| w.contains("Auto-union failed"))
            .map(|w| format!("WARNING {w}")),
    );
    let bodies = handles.len();
    let meshes = b
        .tessellate_live_with_tol(characteristic_length(doc) * 1e-4)
        .map_err(|e| e.to_string());
    let tris = meshes.as_deref().map(triangles).unwrap_or_default();
    let mesh_volume = meshes.map(|ms| {
        ms.iter()
            .map(test_harness::helpers::mesh_signed_volume)
            .sum::<f64>()
    });
    Measure {
        errs,
        bodies,
        chi,
        exact_volume,
        mesh_volume,
        tris,
        hash: test_harness::assay::geometry_hash::live_geometry_hash(&b),
    }
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / a.abs().max(f64::MIN_POSITIVE)).abs()
}

/// The largest `geo` two builds of one solid are let differ by.
///
/// `geo` reads the vertices of one render mesh (points of the surface)
/// against the chords of the other, so it is bounded by the render chord
/// band, not by zero: kernel-v2 renders every circle at a sagitta of 1e-3 of
/// its radius, whatever tolerance is asked for. Two bands, taken on the
/// body's diagonal. Measured over the corpus, four motions a case
/// (2026-11-15): 9.1e-4 at most over the 1,113 `OK` rows whose exact volumes
/// agree, 1.18e-3 at most over the 262 with no exact volume. A defect thinner
/// than the band is not seen: F0076's (2026-11-14) was a 0.649 mm overhang on
/// a body 1.21 m across, 5.4e-4.
const GEO_BAND: f64 = 2e-3;

fn verdict(reference: &Measure, moved: &Measure, geo: Option<f64>) -> &'static str {
    if !moved.errs.is_empty() {
        return "ERR";
    }
    if geo.is_some_and(|g| g.is_nan() || g > GEO_BAND) {
        return "DIFF";
    }
    let same_volume = match (
        &reference.exact_volume,
        &moved.exact_volume,
        &reference.mesh_volume,
        &moved.mesh_volume,
    ) {
        (Ok(a), Ok(b), _, _) => rel(*a, *b) < 1e-9,
        (Err(_), Err(_), Ok(a), Ok(b)) => rel(*a, *b) < 1e-2,
        _ => false,
    };
    if moved.bodies == reference.bodies && moved.chi == reference.chi && same_volume {
        "OK"
    } else {
        "DIFF"
    }
}

fn show(m: &Measure) -> String {
    let exact = match &m.exact_volume {
        Ok(v) => format!("{v:.12e}"),
        Err(_) => "n/a".into(),
    };
    let mesh = match &m.mesh_volume {
        Ok(v) => format!("{v:.6e}"),
        Err(e) => format!("ERR({})", e.chars().take(80).collect::<String>()),
    };
    format!(
        "bodies={} chi={:?} vol={exact} mesh={mesh} hash={:016x}",
        m.bodies, m.chi, m.hash
    )
}

fn load_case(id: &str) -> Value {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/tests/cases/assay");
    let text = std::fs::read_to_string(dir.join(format!("{id}.waffle")))
        .unwrap_or_else(|e| panic!("{id}: {e}"));
    serde_json::from_str(&text).unwrap()
}

fn case_document() -> (String, Value) {
    let id = std::env::var("SWEEP_CASE").expect("set SWEEP_CASE to a corpus case id");
    let doc = load_case(&id);
    (id, doc)
}

/// Motion `k` of a case: seeded by the case id, so two trees draw the same.
fn motion(id: &str, doc: &Value, k: u64) -> RigidMotion {
    let base = id.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |h, c| {
        (h ^ c as u64).wrapping_mul(0x0100_0000_01b3)
    });
    let seed = base ^ (k + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    RigidMotion::draw(&mut Rng::new(seed), characteristic_length(doc))
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

#[test]
#[ignore = "probe: set SWEEP_CASE (and SWEEP_N); prints one SWEEP line per motion"]
fn sweep() {
    let (id, doc) = case_document();
    let reference = with_explicit_x_axes(&doc).expect("explicit axes");
    std::env::set_var("ASSAY_CASE", format!("{id}#ref"));
    let m0 = measure(&reference);
    println!(
        "SWEEP {id} ref errs={} {} diag={:.3e}",
        m0.errs.len(),
        show(&m0),
        extent(&m0.tris)
    );
    for k in 0..env_u64("SWEEP_N", 4) {
        let motion = motion(&id, &doc, k);
        let moved = rigid_motion(&reference, &motion).expect("rigid motion");
        std::env::set_var("ASSAY_CASE", format!("{id}#{k}"));
        let m = measure(&moved);
        let first = m
            .errs
            .first()
            .map(|s| s.chars().take(260).collect::<String>())
            .unwrap_or_default();
        let geo = surface_distance(&m0.tris, &m.tris, &motion).map(|d| d / extent(&m0.tris));
        let shown = match geo {
            Some(g) => format!("geo={g:.3e}"),
            None => "geo=n/a".into(),
        };
        println!(
            "SWEEP {id} {k} {} {} {shown} {first}",
            verdict(&m0, &m, geo),
            show(&m)
        );
    }
}

#[test]
#[ignore = "probe: set SWEEP_CASE (and SWEEP_K); prints one STEPS line per operation"]
fn steps() {
    let (id, doc) = case_document();
    let reference = with_explicit_x_axes(&doc).expect("explicit axes");
    let motion = motion(&id, &doc, env_u64("SWEEP_K", 0));
    let moved = rigid_motion(&reference, &motion).expect("rigid motion");
    let mut last = (0usize, 0usize);
    for n in 1.. {
        let (Some(a), Some(b)) = (truncate_ops(&reference, n), truncate_ops(&moved, n)) else {
            break;
        };
        // `truncate_ops` returns the whole document once `n` passes its
        // operation count; stop when a step adds nothing.
        let size = (
            serde_json::to_string(&a).unwrap().len(),
            serde_json::to_string(&b).unwrap().len(),
        );
        if size == last {
            break;
        }
        last = size;
        let (ma, mb) = (measure(&a), measure(&b));
        let geo = surface_distance(&ma.tris, &mb.tris, &motion).map(|d| d / extent(&ma.tris));
        println!(
            "STEPS {id} ops={n} {} | unmoved: errs={} {} | moved: errs={} {}",
            verdict(&ma, &mb, geo),
            ma.errs.len(),
            show(&ma),
            mb.errs.len(),
            show(&mb)
        );
    }
}

/// P0027 moved rigidly is the unmoved body: the customer, which the corpus
/// does not hold, of the §4.4.1 update's reading of a boundary chain the
/// relocation put out of order (Fig. 11(a), `yang-rs`
/// `seed_fig11a_chain_folds`; ledger N2, `docs/yang_tail_triage.md`).
///
/// Under three of its four motions a relocated junction lands past the next
/// vertex of its crease. With that chain left as the mesh arrangement had it,
/// motion 0 builds a body that differs from the unmoved one by 8.06e-3 of its
/// diagonal from the fourth operation on, with the same body count, Euler
/// characteristics and mesh volume (measured 2026-11-21; the fold reading,
/// T3c, now stops that body).
#[test]
fn p0027_moved_rigidly_is_the_unmoved_body() {
    let id = "P0027";
    let doc = load_case(id);
    let reference = with_explicit_x_axes(&doc).expect("explicit axes");
    let m0 = measure(&reference);
    assert!(m0.errs.is_empty(), "unmoved: {:?}", m0.errs);
    for k in 0..4 {
        let motion = motion(id, &doc, k);
        let moved = rigid_motion(&reference, &motion).expect("rigid motion");
        let m = measure(&moved);
        let geo = surface_distance(&m0.tris, &m.tris, &motion).map(|d| d / extent(&m0.tris));
        assert_eq!(
            verdict(&m0, &m, geo),
            "OK",
            "motion {k}: {} geo={geo:?} errs={:?}",
            show(&m),
            m.errs
        );
    }
}

/// F0076's first three operations with the circle's sketch spun about its
/// own normal: the geometry is unchanged, only where the cylinder's rim
/// samples fall. The union's exact volume must be the document's at every
/// phase.
///
/// Measured 2026-11-14 (`docs/yang_tail_triage.md`): the cylinder overhangs
/// the box's side wall over a 12° arc of its base rim, and for the phases
/// with no regular rim sample inside that arc (10°–20° of every 30°) the
/// kernel returned 0.02937507081560 with 15 faces — the overhang's base
/// sliver absent and, under it, 1.7469e-7 of material neither operand has,
/// with no error. yang-rs's curve-domain certificate (T9, ledger N107) now
/// sends that site to §4.5.2 refinement. The expected value is independent
/// of the kernel: inclusion–exclusion of the convex pieces, the cylinder a
/// circumscribed polygon extrapolated over its side count.
#[test]
fn f0076_union_volume_is_the_same_at_every_rim_phase() {
    const VOLUME: f64 = 0.029_374_896_126_156;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../app/tests/cases/assay/F0076.waffle");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let reference = with_explicit_x_axes(&doc).expect("explicit axes");
    for deg in (0..60).step_by(5) {
        let mut spun = reference.clone();
        let features = spun["tabs"][0]["kind"]["features"]["features"]
            .as_array_mut()
            .expect("features");
        // The third sketch is the circle.
        let sketch = &mut features
            .iter_mut()
            .filter(|f| f["operation"]["type"] == "Sketch")
            .nth(2)
            .expect("third sketch")["operation"]["sketch"];
        let read = |v: &Value| -> [f64; 3] {
            let a = v.as_array().expect("vector");
            [
                a[0].as_f64().unwrap(),
                a[1].as_f64().unwrap(),
                a[2].as_f64().unwrap(),
            ]
        };
        let (n, u) = (read(&sketch["plane_normal"]), read(&sketch["plane_x_axis"]));
        let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = [n[0] / nl, n[1] / nl, n[2] / nl];
        let v = [
            n[1] * u[2] - n[2] * u[1],
            n[2] * u[0] - n[0] * u[2],
            n[0] * u[1] - n[1] * u[0],
        ];
        let (c, s) = (
            f64::from(deg).to_radians().cos(),
            f64::from(deg).to_radians().sin(),
        );
        sketch["plane_x_axis"] = serde_json::json!([
            c * u[0] + s * v[0],
            c * u[1] + s * v[1],
            c * u[2] + s * v[2]
        ]);
        let m = measure(&truncate_ops(&spun, 3).expect("three operations"));
        assert!(m.errs.is_empty(), "phase {deg}°: {:?}", m.errs);
        let volume = m
            .exact_volume
            .unwrap_or_else(|e| panic!("phase {deg}°: {e}"));
        assert!(
            rel(volume, VOLUME) <= 1e-11,
            "phase {deg}°: exact volume {volume:.14e}, the document's is {VOLUME:.14e}"
        );
    }
}

/// A cube's twelve triangles, its corner at `o`.
fn cube(o: [f64; 3], size: f64) -> Vec<Tri> {
    let v = |i: usize| -> [f64; 3] {
        [
            o[0] + size * (i & 1) as f64,
            o[1] + size * ((i >> 1) & 1) as f64,
            o[2] + size * ((i >> 2) & 1) as f64,
        ]
    };
    [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ]
    .iter()
    .flat_map(|q| [[v(q[0]), v(q[1]), v(q[2])], [v(q[0]), v(q[2]), v(q[3])]])
    .collect()
}

#[test]
fn the_grid_distance_is_the_brute_force_distance() {
    let mesh = cube([0.0; 3], 1.0);
    let grid = SurfaceGrid::new(&mesh);
    // Inside, on, just outside and far outside the cube; exact in binary.
    for p in [
        [0.5, 0.5, 0.25],
        [0.25, 0.75, 1.0],
        [1.5, 0.5, 0.5],
        [-3.0, -4.0, 0.5],
        [9.0, 9.0, 9.0],
    ] {
        let brute = mesh
            .iter()
            .map(|t| point_triangle_distance2(p, t))
            .fold(f64::INFINITY, f64::min)
            .sqrt();
        assert_eq!(grid.distance(p), brute, "{p:?}");
    }
    assert_eq!(grid.distance([0.5, 0.5, 0.25]), 0.25);
    assert_eq!(grid.distance([1.5, 0.5, 0.5]), 0.5);
    assert_eq!(grid.distance([-3.0, -4.0, 0.5]), 5.0);
}

#[test]
fn surface_distance_carries_the_moved_mesh_back_and_reads_both_ways() {
    let m = RigidMotion::about([1.0, 2.0, -0.5], 0.7, [3.0, -1.0, 2.0]);
    let reference = cube([0.0; 3], 1.0);
    let carry = |tris: &[Tri]| -> Vec<Tri> { tris.iter().map(|t| t.map(|p| m.apply(p))).collect() };
    // The same solid, moved: nothing but rounding.
    let same = surface_distance(&reference, &carry(&reference), &m).expect("distance");
    assert!(same < 1e-14, "{same}");
    // A second cube of side 0.5 standing 0.25 off one face: its far corners
    // are 0.75 from the first. Present in the moved build only, it is read
    // from the moved side; in the reference only, from the other.
    let mut extra = reference.clone();
    extra.extend(cube([1.25, 0.0, 0.0], 0.5));
    let added = surface_distance(&reference, &carry(&extra), &m).expect("distance");
    assert!((added - 0.75).abs() < 1e-14, "{added}");
    let lost = surface_distance(&extra, &carry(&reference), &m).expect("distance");
    assert!((lost - 0.75).abs() < 1e-14, "{lost}");
    assert_eq!(surface_distance(&reference, &[], &m), None);
    assert!((extent(&reference) - 3.0f64.sqrt()).abs() < 1e-15);
}
