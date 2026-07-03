//! M8 Stage-0 operand diagnosis — Increment 0 of the inputcheck-clean
//! emission cycle (spec `specs/m8_stage0_inputcheck_clean_emission.md` §2/§6).
//!
//! ASSERTION-FREE measurement drivers, one per acceptance case
//! (R0046 / R0088 / F0063). Each replays its corpus case through the full
//! kernel-v2 dispatch with `YANG_STAGE0_DUMP_DIR` pointed at a per-case
//! directory, then, per dumped boolean op:
//!
//! - runs the native five-axiom census (`cherchi_rs::inputcheck::census`)
//!   on BOTH emitted operands AND both pre-Stage-0 Stage-1 meshes (`_pre`),
//!   printing an introduced-vs-inherited delta per defect class;
//! - runs the sidecar `mesh_booleans_inputcheck` reference oracle on the
//!   same meshes (skipped with a loud notice when the binary is absent);
//! - joins census offenders back to B-Rep faces through the dumped
//!   `tri_face` provenance CSVs.
//!
//! Env mutation is process-global: run with `--test-threads=1`.
//!
//! ```text
//! cargo test -p test-harness --test m8_stage0_operand_diagnosis -- \
//!     --ignored --nocapture --test-threads=1
//! ```
//!
//! Mechanism-attribution stderr (the Stage-0 probes `YANG_COPLANAR_PROBE`,
//! `YANG_RIMLAT_PROBE`, `RIM_SUBDIV_PROBE`) interleaves with the report
//! under `--nocapture`; the spec's §2 amendment reads both together.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use cherchi_rs::inputcheck::{census, NativeInputCheck};
use cherchi_sidecar_rs::obj::read_obj;
use test_harness::ModelBuilder;

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

fn dump_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/stage0_dump")
}

/// Replay one corpus case through the full kernel-v2 dispatch and return every
/// boolean failure (engine errors + `Auto-union failed` warnings). Mirrors
/// `m8_rim_clustering_campaign.rs`.
fn boolean_failures(case_id: &str) -> Vec<String> {
    let dir = assay_dir();
    let waffle_json = match fs::read_to_string(dir.join(format!("{case_id}.waffle"))) {
        Ok(s) => s,
        Err(e) => return vec![format!("cannot read {case_id}.waffle: {e}")],
    };

    let mut builder = ModelBuilder::kernel_v2();
    if let Err(e) = builder.load(&waffle_json) {
        return vec![format!("LoadProject failed: {e}")];
    }

    let mut failures: Vec<String> = builder
        .engine_errors()
        .iter()
        .map(|(id, msg)| format!("error {id}: {msg}"))
        .collect();
    failures.extend(
        builder
            .engine_warnings()
            .iter()
            .filter(|w| w.contains("Auto-union"))
            .cloned(),
    );
    failures
}

/// Replay with a hang guard (mirrors the campaign trackers; heavy exact
/// arithmetic cannot be killed in-process — a timeout is reported, and the
/// orphaned worker keeps running while the diagnosis moves on).
fn boolean_failures_with_timeout(case_id: &str, timeout: Duration) -> Vec<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = case_id.to_string();
    let handle = std::thread::spawn(move || {
        let _ = tx.send(boolean_failures(&worker));
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => {
            let _ = handle.join();
            r
        }
        Err(_) => vec![format!(
            "{case_id}: timeout after {}s (worker orphaned)",
            timeout.as_secs()
        )],
    }
}

/// Per-defect-class counts for the introduced-vs-inherited delta table.
fn class_counts(c: &NativeInputCheck) -> Vec<(&'static str, usize)> {
    vec![
        ("nonmanifold_edges", c.nonmanifold_edges.len()),
        ("nonmanifold_verts", c.nonmanifold_verts.len()),
        ("boundary_edges", c.boundary_edges.len()),
        ("misoriented_pairs", c.misoriented_pairs.len()),
        ("improper_pairs", c.improper_pairs.len()),
        ("unresolved_pairs", c.unresolved_pairs.len()),
        ("duplicate_tris", c.duplicate_tris.len()),
        ("index_degenerate", c.index_degenerate_tris.len()),
        ("collinear_degenerate", c.collinear_degenerate_tris.len()),
        ("vertex_twins", c.coincident_vert_twins.len()),
    ]
}

fn load_tri_face(path: &std::path::Path) -> Option<Vec<u32>> {
    let s = fs::read_to_string(path).ok()?;
    Some(s.lines().filter_map(|l| l.trim().parse().ok()).collect())
}

/// Histogram of the B-Rep faces owning a set of triangles (through the
/// dumped `tri_face` CSV; `u32::MAX` = unattributed sentinel).
fn face_histogram(tris: &[u32], tri_face: Option<&[u32]>) -> String {
    let Some(tf) = tri_face else {
        return "(no tri_face map)".into();
    };
    let mut hist: BTreeMap<String, usize> = BTreeMap::new();
    for &t in tris {
        let f = tf.get(t as usize).copied();
        let key = match f {
            Some(u32::MAX) => "unattributed".into(),
            Some(f) => format!("face {f}"),
            None => "out-of-range".into(),
        };
        *hist.entry(key).or_insert(0) += 1;
    }
    format!("{hist:?}")
}

fn sidecar_line(mesh: &cherchi_rs::Mesh) -> String {
    match cherchi_sidecar_rs::inputcheck(mesh, Duration::from_secs(30)) {
        Ok(r) => format!(
            "manifold={} watertight={} local_orientation={} global_orientation={} \
             intersection_free={}",
            r.manifold,
            r.watertight,
            r.local_orientation,
            r.global_orientation,
            r.intersection_free
        ),
        Err(e) => format!("UNAVAILABLE ({e})"),
    }
}

/// Print one operand's full report: sidecar verdict, native census summary,
/// and per-class offender→face attribution.
fn report_operand(
    tag: &str,
    mesh: &cherchi_rs::Mesh,
    tri_face: Option<&[u32]>,
) -> NativeInputCheck {
    let c = census(&mesh.verts, &mesh.tris);
    println!(
        "  [{tag}] {} verts / {} tris",
        mesh.verts.len(),
        mesh.tris.len()
    );
    println!("    sidecar : {}", sidecar_line(mesh));
    for line in c.summary().lines() {
        println!("    native  : {line}");
    }
    // Offender → B-Rep face attribution (post-Stage-0 operands only).
    if !c.boundary_edges.is_empty() {
        let tris: Vec<u32> = c
            .boundary_edges
            .iter()
            .flat_map(|e| e.tris.clone())
            .collect();
        println!(
            "    boundary-edge tris → {}",
            face_histogram(&tris, tri_face)
        );
        for e in c.boundary_edges.iter().take(8) {
            let (u, v) = e.verts;
            let (p, q) = (mesh.verts[u as usize], mesh.verts[v as usize]);
            println!(
                "      edge ({u},{v}) ({},{},{})–({},{},{}) tris {:?}",
                p.x(),
                p.y(),
                p.z(),
                q.x(),
                q.y(),
                q.z(),
                e.tris
            );
        }
    }
    if !c.nonmanifold_edges.is_empty() {
        let tris: Vec<u32> = c
            .nonmanifold_edges
            .iter()
            .flat_map(|e| e.tris.clone())
            .collect();
        println!(
            "    nonmanifold-edge tris → {}",
            face_histogram(&tris, tri_face)
        );
    }
    if !c.misoriented_pairs.is_empty() {
        let tris: Vec<u32> = c
            .misoriented_pairs
            .iter()
            .flat_map(|&(a, b)| [a, b])
            .collect();
        println!("    misoriented tris → {}", face_histogram(&tris, tri_face));
    }
    if !c.improper_pairs.is_empty() {
        let tris: Vec<u32> = c.improper_pairs.iter().flat_map(|&(a, b)| [a, b]).collect();
        println!(
            "    improper-pair tris → {}",
            face_histogram(&tris, tri_face)
        );
        for &(a, b) in c.improper_pairs.iter().take(8) {
            println!(
                "      pair ({a},{b}) verts {:?} / {:?}",
                mesh.tris[a as usize], mesh.tris[b as usize]
            );
        }
    }
    if !c.index_degenerate_tris.is_empty() {
        println!(
            "    index-degenerate tris → {}",
            face_histogram(&c.index_degenerate_tris, tri_face)
        );
        for &t in c.index_degenerate_tris.iter().take(10) {
            let tri = mesh.tris[t as usize];
            let p = mesh.verts[tri[0] as usize];
            println!("      tri {t} {:?} @ ({},{},{})", tri, p.x(), p.y(), p.z());
        }
    }
    if !c.coincident_vert_twins.is_empty() {
        for &(a, b) in c.coincident_vert_twins.iter().take(8) {
            let p = mesh.verts[a as usize];
            println!(
                "    vertex twin ({a},{b}) @ ({},{},{})",
                p.x(),
                p.y(),
                p.z()
            );
        }
    }
    c
}

fn delta_table(tag: &str, post: &NativeInputCheck, pre: &NativeInputCheck) {
    println!("  [{tag}] introduced-vs-inherited (post vs pre):");
    for ((name, p), (_, q)) in class_counts(post).into_iter().zip(class_counts(pre)) {
        if p != 0 || q != 0 {
            let marker = if p > q { "  ← INTRODUCED" } else { "" };
            println!("    {name:22} post {p:5}  pre {q:5}{marker}");
        }
    }
}

fn diagnose(case_id: &str) {
    let dump = dump_root().join(case_id);
    let _ = fs::remove_dir_all(&dump);
    fs::create_dir_all(&dump).expect("create dump dir");

    std::env::set_var("YANG_STAGE0_DUMP_DIR", &dump);
    std::env::set_var("YANG_COPLANAR_PROBE", "1");
    std::env::set_var("YANG_RIMLAT_PROBE", "1");
    std::env::set_var("RIM_SUBDIV_PROBE", "1");

    println!("════ {case_id} replay ════");
    let failures = boolean_failures_with_timeout(case_id, Duration::from_secs(200));

    std::env::remove_var("YANG_STAGE0_DUMP_DIR");
    std::env::remove_var("YANG_COPLANAR_PROBE");
    std::env::remove_var("YANG_RIMLAT_PROBE");
    std::env::remove_var("RIM_SUBDIV_PROBE");

    println!("wall strings ({}):", failures.len());
    for f in &failures {
        println!("  {f}");
    }

    // Collect op stems ({n:03}_{op}) from the dump.
    let mut stems: Vec<String> = fs::read_dir(&dump)
        .expect("read dump dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            name.strip_suffix("_meta.txt").map(str::to_string)
        })
        .collect();
    stems.sort();

    println!(
        "\n════ {case_id} operand census ({} ops dumped) ════",
        stems.len()
    );
    for stem in &stems {
        let meta = fs::read_to_string(dump.join(format!("{stem}_meta.txt"))).unwrap_or_default();
        let stage0 = meta.contains("stage0: true");
        println!("\n── {stem} (stage0: {stage0}) ──");
        for line in meta.lines() {
            if line.starts_with("pair_plane") {
                println!("  {line}");
            }
        }
        if !stage0 {
            println!("  (Stage 0 did not fire — operands are the Stage-1 meshes; skipping)");
            continue;
        }
        for side in ["a", "b"] {
            let post = read_obj(&dump.join(format!("{stem}_{side}.obj"))).expect("read post obj");
            let pre = read_obj(&dump.join(format!("{stem}_{side}_pre.obj"))).expect("read pre obj");
            let tf = load_tri_face(&dump.join(format!("{stem}_{side}.tri_face.csv")));
            let c_post = report_operand(&format!("{side} post"), &post, tf.as_deref());
            let c_pre = report_operand(&format!("{side} pre "), &pre, None);
            delta_table(side, &c_post, &c_pre);
        }
    }
}

#[test]
#[ignore = "M8 Stage-0 diagnosis driver (assertion-free measurement; spec \
            m8_stage0_inputcheck_clean_emission §2). Run with --ignored --nocapture \
            --test-threads=1"]
fn diagnose_r0046() {
    diagnose("R0046");
}

#[test]
#[ignore = "M8 Stage-0 diagnosis driver (assertion-free measurement; spec \
            m8_stage0_inputcheck_clean_emission §2)"]
fn diagnose_r0088() {
    diagnose("R0088");
}

#[test]
#[ignore = "M8 Stage-0 diagnosis driver (assertion-free measurement; spec \
            m8_stage0_inputcheck_clean_emission §2)"]
fn diagnose_f0063() {
    diagnose("F0063");
}
