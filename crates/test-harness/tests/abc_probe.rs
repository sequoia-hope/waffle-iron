//! FEASIBILITY PROBE (scratch): can a real-world STEP model from the ABC
//! dataset be imported, welded into a closed mesh, and pushed through the
//! native cherchi-rs mesh boolean?
//!
//! Not a gate. `#[ignore]`d; deleted or promoted once the answer is recorded.
//!
//! Get the corpus with `scripts/fetch-abc-corpus.sh 0000 /tmp/abc`, then:
//!
//! ABC_DIR=/tmp/abc/chunk0000 ABC_N=200 \
//!   cargo test -p test-harness --test abc_probe --release \
//!   -- --ignored --nocapture abc_import_boolean_probe
//!
//! Its findings are recorded in `specs/boolean_hardening_external_corpus.md`
//! §10. The *vocabulary* and *exactness* questions it raised are measured
//! independently of truck by `scripts/si5_census.py` / `si5_exactness.py` —
//! see `specs/step_import_si5_exact_analytic_ingestion.md` §2.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cad_primitives::{BoolOp, Point3};
use cherchi_rs::boolean::MeshBoolean;
use cherchi_rs::{census, labeling::NativeBoolean, Mesh};
use waffle_types::kernel::import::ImportedBodyData;

/// Weld every face of every shell of an imported body into one indexed mesh.
///
/// STEP faces are tessellated independently, so the shared boundary between
/// two faces arrives as two coincident vertex runs. Quantize to `q` and merge.
fn weld(body: &ImportedBodyData, q: f64) -> Mesh {
    let mut verts: Vec<Point3> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut map: HashMap<(i64, i64, i64), u32> = HashMap::new();

    for shell in &body.shells {
        for face in &shell.faces {
            let base: Vec<u32> = (0..face.positions.len() / 3)
                .map(|i| {
                    let p = [
                        face.positions[3 * i],
                        face.positions[3 * i + 1],
                        face.positions[3 * i + 2],
                    ];
                    let key = (
                        (p[0] / q).round() as i64,
                        (p[1] / q).round() as i64,
                        (p[2] / q).round() as i64,
                    );
                    *map.entry(key).or_insert_with(|| {
                        verts.push(Point3::new(p[0], p[1], p[2]));
                        (verts.len() - 1) as u32
                    })
                })
                .collect();
            for t in face.indices.chunks_exact(3) {
                let tri = [
                    base[t[0] as usize],
                    base[t[1] as usize],
                    base[t[2] as usize],
                ];
                if tri[0] == tri[1] || tri[1] == tri[2] || tri[0] == tri[2] {
                    continue; // collapsed by the weld
                }
                tris.push(tri);
            }
        }
    }
    Mesh::new(verts, tris)
}

fn bbox(m: &Mesh) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for v in &m.verts {
        let c = [v.x(), v.y(), v.z()];
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    (lo, hi)
}

fn mesh_volume(m: &Mesh) -> f64 {
    let mut v = 0.0;
    for t in &m.tris {
        let a = m.verts[t[0] as usize];
        let b = m.verts[t[1] as usize];
        let c = m.verts[t[2] as usize];
        v += a.x() * (b.y() * c.z() - b.z() * c.y()) - a.y() * (b.x() * c.z() - b.z() * c.x())
            + a.z() * (b.x() * c.y() - b.y() * c.x());
    }
    v / 6.0
}

/// Rigid rotation about the three axes plus translation, in place.
fn place(m: &mut Mesh, deg: [f64; 3], t: [f64; 3]) {
    let (sx, cx) = deg[0].to_radians().sin_cos();
    let (sy, cy) = deg[1].to_radians().sin_cos();
    let (sz, cz) = deg[2].to_radians().sin_cos();
    for v in &mut m.verts {
        let (x, y, z) = (v.x(), v.y(), v.z());
        let (y, z) = (cx * y - sx * z, sx * y + cx * z);
        let (x, z) = (cy * x + sy * z, -sy * x + cy * z);
        let (x, y) = (cz * x - sz * y, sz * x + cz * y);
        *v = Point3::new(x + t[0], y + t[1], z + t[2]);
    }
}

struct Loaded {
    id: String,
    mesh: Mesh,
    closed: bool,
    /// Diagnosis fields: is the non-closure a vertex-weld miss (fixable by
    /// tolerance) or a non-conformal edge discretisation (not fixable at all
    /// by welding, because the two sides sample the shared curve a different
    /// number of times)?
    boundary_edges: usize,
    nonmanifold_edges: usize,
    faces: usize,
    /// True when the source STEP has no b-spline/revolution/extrusion/offset
    /// surface — i.e. every face is analytic and its boundary curves are
    /// lines/circles the two sides could in principle sample identically.
    analytic_only: bool,
}

fn load(path: &PathBuf) -> Result<Loaded, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    // truck panics rather than erroring on some real-world input (e.g.
    // `sphere.rs:134 tolerance must be no less than 1e-6`), so import is a
    // catch_unwind boundary: a panic is a finding, not a dead run.
    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        step_import::parse_step(&text, &stem)
    }))
    .map_err(|e| {
        let m = e
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "?".into());
        format!("panic: {m}")
    })?;
    let body = parsed.map_err(|e| format!("import: {e}"))?;
    // Weld tolerance. truck tessellates every STEP face independently, so a
    // shared boundary arrives as two coincident-ish vertex runs; how tight a
    // weld still closes the shell is the measurement ABC_WELD exists for.
    let q: f64 = std::env::var("ABC_WELD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1e-9);
    let mesh = weld(&body, q);
    if mesh.tris.is_empty() {
        return Err("empty mesh".into());
    }
    let c = census(&mesh.verts, &mesh.tris);
    let closed = c.boundary_edges.is_empty() && c.nonmanifold_edges.is_empty();
    let analytic_only = !text.contains("B_SPLINE_SURFACE")
        && !text.contains("SURFACE_OF_REVOLUTION")
        && !text.contains("SURFACE_OF_LINEAR_EXTRUSION")
        && !text.contains("OFFSET_SURFACE");
    Ok(Loaded {
        id: stem,
        mesh,
        closed,
        boundary_edges: c.boundary_edges.len(),
        nonmanifold_edges: c.nonmanifold_edges.len(),
        faces: body.face_count(),
        analytic_only,
    })
}

#[test]
#[ignore = "probe: needs ABC_DIR"]
fn abc_import_boolean_probe() {
    let Ok(dir) = std::env::var("ABC_DIR") else {
        eprintln!("ABC_DIR unset — skipping");
        return;
    };
    let n: usize = std::env::var("ABC_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let max_tris: usize = std::env::var("ABC_MAX_TRIS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200_000);
    // MEASURED 2026-09-30: an uncapped in-process import loop over ABC chunk
    // 0000 reached 41 GB RSS and 14 CPU-minutes on a single model before it
    // was killed. Real-world STEP has a 540 MB tail (p50 125 KB, p99 21 MB).
    // The production harness must budget import in a subprocess; the probe
    // caps by file size so it can finish at all.
    let max_bytes: u64 = std::env::var("ABC_MAX_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000_000);

    // Keep the panic hook quiet: a panicking import is an expected, counted
    // outcome here, and 300 backtraces would bury the histogram.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut all = 0usize;
    let mut oversize_bytes = 0usize;
    let mut files: Vec<PathBuf> = Vec::new();
    for e in walk(&PathBuf::from(&dir)) {
        if !e.extension().map(|x| x == "step").unwrap_or(false) {
            continue;
        }
        all += 1;
        match std::fs::metadata(&e) {
            Ok(m) if m.len() > max_bytes => {
                oversize_bytes += 1;
                continue;
            }
            _ => {}
        }
        files.push(e);
    }
    files.sort();
    files.truncate(n);
    eprintln!(
        "== probing {} models from {dir} (scanned {all}, skipped {oversize_bytes} over {max_bytes} bytes)",
        files.len()
    );

    // ── Stage 1: import ────────────────────────────────────────────────
    let t0 = Instant::now();
    let mut ok: Vec<Loaded> = Vec::new();
    let mut import_fail = 0usize;
    let mut not_closed = 0usize;
    let mut too_big = 0usize;
    let mut fail_kinds: HashMap<String, usize> = HashMap::new();
    let mut import_secs: Vec<(f64, String)> = Vec::new();
    for f in &files {
        let t = Instant::now();
        let r = load(f);
        import_secs.push((
            t.elapsed().as_secs_f64(),
            f.file_stem().unwrap().to_string_lossy().to_string(),
        ));
        match r {
            Ok(l) => {
                if l.mesh.tris.len() > max_tris {
                    too_big += 1;
                    continue;
                }
                if !l.closed {
                    not_closed += 1;
                }
                ok.push(l);
            }
            Err(e) => {
                import_fail += 1;
                // Signature: strip digits so "#1359" / "#240" collapse to one
                // class, then truncate.
                let sig: String = e
                    .chars()
                    .map(|c| if c.is_ascii_digit() { '#' } else { c })
                    .collect();
                let sig = sig.chars().take(90).collect::<String>();
                *fail_kinds.entry(sig).or_default() += 1;
            }
        }
    }
    eprintln!(
        "IMPORT  total={} ok={} failed={} not_closed={} oversize(>{}tris)={} in {:.1}s",
        files.len(),
        ok.len(),
        import_fail,
        not_closed,
        max_tris,
        too_big,
        t0.elapsed().as_secs_f64()
    );
    std::panic::set_hook(prev_hook);
    import_secs.sort_by(|a, b| b.0.total_cmp(&a.0));
    let tot: f64 = import_secs.iter().map(|(s, _)| s).sum();
    eprintln!(
        "        import cpu {:.1}s total; slowest: {}",
        tot,
        import_secs
            .iter()
            .take(5)
            .map(|(s, id)| format!("{id}={s:.1}s"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let mut kinds: Vec<_> = fail_kinds.iter().collect();
    kinds.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (k, n) in kinds {
        eprintln!("        x{n:<5} {k}");
    }

    // ── Diagnosis: why is anything open? ───────────────────────────────
    // A vertex-weld miss leaves a HANDFUL of boundary edges. A non-conformal
    // edge discretisation (the two faces sharing a curve sample it a
    // different number of times) leaves boundary edges proportional to the
    // face count, and NO weld tolerance can close it.
    let (mut an_t, mut an_c, mut fr_t, mut fr_c) = (0usize, 0usize, 0usize, 0usize);
    let mut open_bnd: Vec<usize> = Vec::new();
    let mut bnd_per_face: Vec<f64> = Vec::new();
    // An open model that ALSO has non-manifold edges is a third class: the weld
    // fused two sheets that should have stayed apart. Distinguishing it matters
    // because it is not a discretisation story at all.
    let mut open_nonmanifold = 0usize;
    for l in &ok {
        if l.analytic_only {
            an_t += 1;
            if l.closed {
                an_c += 1;
            }
        } else {
            fr_t += 1;
            if l.closed {
                fr_c += 1;
            }
        }
        if !l.closed {
            open_bnd.push(l.boundary_edges);
            if l.nonmanifold_edges > 0 {
                open_nonmanifold += 1;
            }
            if l.faces > 0 {
                bnd_per_face.push(l.boundary_edges as f64 / l.faces as f64);
            }
        }
    }
    let pct = |c: usize, t: usize| {
        if t == 0 {
            0.0
        } else {
            100.0 * c as f64 / t as f64
        }
    };
    eprintln!(
        "CLOSURE analytic-only {an_c}/{an_t} ({:.0}%)   has-freeform {fr_c}/{fr_t} ({:.0}%)",
        pct(an_c, an_t),
        pct(fr_c, fr_t)
    );
    open_bnd.sort_unstable();
    bnd_per_face.sort_by(f64::total_cmp);
    if !open_bnd.is_empty() {
        let q = |v: &Vec<usize>, f: f64| v[((v.len() - 1) as f64 * f) as usize];
        eprintln!(
            "        open models: boundary-edge count p10={} p50={} p90={} max={}",
            q(&open_bnd, 0.1),
            q(&open_bnd, 0.5),
            q(&open_bnd, 0.9),
            open_bnd[open_bnd.len() - 1]
        );
        eprintln!(
            "        boundary edges PER FACE p10={:.2} p50={:.2} p90={:.2}  \
             (≈0 ⇒ weld miss; ≳1 ⇒ non-conformal discretisation)",
            bnd_per_face[(bnd_per_face.len() - 1) / 10],
            bnd_per_face[(bnd_per_face.len() - 1) / 2],
            bnd_per_face[(bnd_per_face.len() - 1) * 9 / 10]
        );
        eprintln!(
            "        open models ALSO carrying non-manifold edges: {}/{} \
             (weld fused sheets that should stay apart)",
            open_nonmanifold,
            open_bnd.len()
        );
    }

    let closed: Vec<&Loaded> = ok.iter().filter(|l| l.closed).collect();
    eprintln!("        usable (closed manifold weld): {}", closed.len());
    if closed.len() < 2 {
        eprintln!("not enough closed models to pair — stopping");
        return;
    }

    // ── Stage 2: paired booleans with a random-ish overlapping placement ──
    let backend = NativeBoolean;
    let pairs = closed.len().min(24) / 2;
    let mut stats: HashMap<&str, usize> = HashMap::new();
    let mut ie_checked = 0usize;
    let mut ie_violations = 0usize;

    for i in 0..pairs {
        let a = closed[2 * i];
        let b = closed[2 * i + 1];
        let (alo, ahi) = bbox(&a.mesh);
        let (blo, bhi) = bbox(&b.mesh);
        // Put B's centre at A's centre, so they genuinely overlap.
        let ac = [
            (alo[0] + ahi[0]) / 2.0,
            (alo[1] + ahi[1]) / 2.0,
            (alo[2] + ahi[2]) / 2.0,
        ];
        let bc = [
            (blo[0] + bhi[0]) / 2.0,
            (blo[1] + bhi[1]) / 2.0,
            (blo[2] + bhi[2]) / 2.0,
        ];
        let mut bm = b.mesh.clone();
        place(
            &mut bm,
            [17.0, 31.0, 7.0],
            [ac[0] - bc[0], ac[1] - bc[1], ac[2] - bc[2]],
        );

        let va = mesh_volume(&a.mesh).abs();
        let vb = mesh_volume(&bm).abs();
        let mut vols: HashMap<&str, f64> = HashMap::new();

        for (name, op) in [
            ("union", BoolOp::Union),
            ("intersect", BoolOp::Intersect),
            ("subtract", BoolOp::Subtract),
        ] {
            let t = Instant::now();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                backend.boolean(&a.mesh, &bm, op)
            }));
            let dt = t.elapsed();
            match r {
                Err(_) => {
                    *stats.entry("PANIC").or_default() += 1;
                    eprintln!("  PANIC  {name} {} x {} ", a.id, b.id);
                }
                Ok(Err(e)) => {
                    *stats.entry("ERROR").or_default() += 1;
                    let m = format!("{e}");
                    eprintln!(
                        "  ERROR  {name} {} x {} ({:.1}s): {}",
                        a.id,
                        b.id,
                        dt.as_secs_f64(),
                        &m[..m.len().min(140)]
                    );
                }
                Ok(Ok(out)) => {
                    let c = census(&out.verts, &out.tris);
                    let watertight = c.boundary_edges.is_empty() && c.nonmanifold_edges.is_empty();
                    let v = mesh_volume(&out).abs();
                    vols.insert(name, v);
                    if watertight {
                        *stats.entry("OK").or_default() += 1;
                    } else {
                        *stats.entry("NOT_WATERTIGHT").or_default() += 1;
                        eprintln!(
                            "  OPEN   {name} {} x {} ({:.1}s) bnd={} nm={}",
                            a.id,
                            b.id,
                            dt.as_secs_f64(),
                            c.boundary_edges.len(),
                            c.nonmanifold_edges.len()
                        );
                    }
                    if dt > Duration::from_secs(30) {
                        eprintln!(
                            "  SLOW   {name} {} x {} {:.1}s",
                            a.id,
                            b.id,
                            dt.as_secs_f64()
                        );
                    }
                }
            }
        }

        // Inclusion–exclusion: vol(A∪B) + vol(A∩B) == vol(A) + vol(B)
        if let (Some(&u), Some(&x)) = (vols.get("union"), vols.get("intersect")) {
            ie_checked += 1;
            let lhs = u + x;
            let rhs = va + vb;
            let rel = (lhs - rhs).abs() / rhs.max(1e-30);
            if rel > 1e-6 {
                ie_violations += 1;
                eprintln!(
                    "  IE-VIOL {} x {}: |A∪B|+|A∩B|={lhs:.9e} vs |A|+|B|={rhs:.9e} rel={rel:.3e}",
                    a.id, b.id
                );
            }
        }
    }

    eprintln!("BOOLEAN pairs={pairs} outcomes={stats:?}");
    eprintln!("INCLUSION-EXCLUSION checked={ie_checked} violations={ie_violations}");
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}
