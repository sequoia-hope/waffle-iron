//! Shared helpers for invoking the Cherchi 2022 `mesh_booleans` /
//! `mesh_booleans_inputcheck` sidecar binaries from test-harness tests.
//!
//! Originally introduced inline in
//! `tests/cherchi2022_reference_parity.rs` (PR-S1, commit `17792eb`); extracted
//! here for PR-S2's `cherchi_inputcheck_corpus_sweep.rs` so both tests share
//! one implementation of binary discovery and timed subprocess execution.
//! Parameterizing the timeout (vs. a const) is the only behavioral change vs.
//! PR-S1 — the reference-parity test still uses 30 s, the sweep uses 10 s.
//!
//! Refs: PR-S1 deliverables in `/home/claude/.claude/plans/reactive-juggling-sloth.md`,
//! PR-S2 spec at `specs/cherchi_inputcheck_corpus_sweep.md`.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Default location of the upstream `mesh_booleans` binary. The same
/// directory contains `mesh_booleans_inputcheck`.
pub const CHERCHI2022_BIN_DEFAULT: &str =
    "/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans";

/// Outcome of a timed subprocess invocation: completed (with the
/// original `Output`), or `TimedOut` after the kill, or a spawn-time error.
pub enum TimedRun {
    Completed(std::process::Output),
    TimedOut,
    SpawnFailed(String),
}

/// Resolve the sidecar binary path. Returns `None` (with an `eprintln`
/// explanation) if neither `CHERCHI2022_BIN` env nor the default exists —
/// callers should `return` cleanly so the test is treated as
/// configuration-skipped rather than a failure.
pub fn cherchi_bin() -> Option<PathBuf> {
    let path =
        std::env::var("CHERCHI2022_BIN").unwrap_or_else(|_| CHERCHI2022_BIN_DEFAULT.to_string());
    let p = PathBuf::from(&path);
    if !p.exists() {
        eprintln!(
            "[cherchi-sidecar] SKIP: Cherchi 2022 binary not found at `{}`. \
             Build it per upstream README and either symlink to the default \
             location or set CHERCHI2022_BIN.",
            path
        );
        return None;
    }
    Some(p)
}

/// Spawn a `Command` and either wait for it to finish within `timeout` or
/// kill it. Pipes stdout+stderr so the child doesn't block on a full pipe
/// buffer; collects them into the returned `Output` on completion.
///
/// Polls in 1-second intervals (or `timeout` itself if smaller than 1 s)
/// using `child.try_wait()`. No external crate (no `wait_timeout` dep).
/// The 30 s reference-parity test uses 5 s polls historically; 1 s here
/// gives finer granularity for the 10 s inputcheck timeout while still
/// being cheap.
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> TimedRun {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return TimedRun::SpawnFailed(e.to_string()),
    };
    let poll_interval = std::cmp::min(Duration::from_secs(1), timeout);
    // ceil-div so a 10 s timeout with 1 s polls gives 10 polls (not 9).
    let polls = ((timeout.as_millis() + poll_interval.as_millis() - 1)
        / poll_interval.as_millis().max(1)) as usize;
    for _ in 0..polls {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(poll_interval),
            Err(e) => return TimedRun::SpawnFailed(format!("try_wait failed: {}", e)),
        }
    }
    match child.try_wait() {
        Ok(Some(_)) => match child.wait_with_output() {
            Ok(out) => TimedRun::Completed(out),
            Err(e) => TimedRun::SpawnFailed(format!("wait_with_output failed: {}", e)),
        },
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            TimedRun::TimedOut
        }
    }
}

// ── Reference TOPOLOGY (2026-09-03, corner-transit inc-2c-3b-12b-11) ────────
//
// The categorized runner's `euler_target` is authored, not measured, and the
// voxel topology oracle (`assay::topology_oracle`) cannot settle a union whose
// operands GRAZE (measured on R0053/R0044: the lattice reading swings by tens
// with the sampling phase at every resolution). The sidecar's exact mesh
// boolean is the sanctioned reference (roadmap §6): union the operand
// tessellations through it and read `V − E + F` and the shell count off the
// result. Nothing below touches the kernel's boolean.

use std::io::{self, BufRead, Write};
use std::path::Path;

use waffle_types::kernel::RenderMesh;

/// Write a render mesh as a Wavefront OBJ (positions and triangles only).
pub fn write_obj(path: &Path, mesh: &RenderMesh) -> io::Result<()> {
    let mut f = io::BufWriter::new(std::fs::File::create(path)?);
    for v in mesh.vertices.chunks(3) {
        writeln!(f, "v {} {} {}", v[0], v[1], v[2])?;
    }
    for t in mesh.indices.chunks(3) {
        if t.len() == 3 {
            writeln!(f, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1)?;
        }
    }
    f.flush()
}

/// Positions and triangles of an OBJ file.
pub type ObjMesh = (Vec<[f64; 3]>, Vec<[u32; 3]>);

/// Read a Wavefront OBJ's positions and triangular faces (`v` / `f` lines;
/// `f a/b/c` forms accepted; polygons fanned).
pub fn read_obj(path: &Path) -> io::Result<ObjMesh> {
    let f = io::BufReader::new(std::fs::File::open(path)?);
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    for line in f.lines() {
        let line = line?;
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let mut p = [0.0f64; 3];
                for x in p.iter_mut() {
                    *x = it
                        .next()
                        .and_then(|s| s.parse().ok())
                        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad v line"))?;
                }
                verts.push(p);
            }
            Some("f") => {
                let idx: Vec<u32> = it
                    .map(|s| s.split('/').next().unwrap_or(s).parse::<u32>())
                    .collect::<Result<_, _>>()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad f line"))?;
                for w in 1..idx.len().saturating_sub(1) {
                    tris.push([idx[0] - 1, idx[w] - 1, idx[w + 1] - 1]);
                }
            }
            _ => {}
        }
    }
    Ok((verts, tris))
}

/// `V − E + F` and the shell structure of a triangle soup, after welding
/// vertices that coincide EXACTLY (bit-identical positions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceTopology {
    pub vertices: usize,
    pub edges: usize,
    pub faces: usize,
    /// `V − E + F`.
    pub chi: i64,
    /// Edge-connected components.
    pub shells: usize,
    /// Edges used by exactly one triangle.
    pub boundary_edges: usize,
    /// Edges used by three or more triangles.
    pub nonmanifold_edges: usize,
}

impl SurfaceTopology {
    /// `Σ (1 − g_i)`-style reading is not unique across shells; for ONE
    /// closed shell this is its genus.
    pub fn genus_if_one_shell(&self) -> Option<i64> {
        (self.shells == 1 && self.boundary_edges == 0 && self.nonmanifold_edges == 0)
            .then(|| (2 - self.chi) / 2)
    }
}

/// The topology of `tris` over `verts` (see [`SurfaceTopology`]).
pub fn surface_topology(verts: &[[f64; 3]], tris: &[[u32; 3]]) -> SurfaceTopology {
    use std::collections::{BTreeMap, HashMap};
    // Exact weld.
    let mut key_of: HashMap<[u64; 3], u32> = HashMap::new();
    let mut remap: Vec<u32> = Vec::with_capacity(verts.len());
    for v in verts {
        let k = [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()];
        let n = key_of.len() as u32;
        let id = *key_of.entry(k).or_insert(n);
        remap.push(id);
    }
    let mut used = vec![false; key_of.len()];
    let mut edge_count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut parent: Vec<u32> = (0..key_of.len() as u32).collect();
    fn find(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            let q = p[x as usize];
            p[x as usize] = p[q as usize];
            x = q;
        }
        x
    }
    let mut faces = 0usize;
    for t in tris {
        let a = remap[t[0] as usize];
        let b = remap[t[1] as usize];
        let c = remap[t[2] as usize];
        if a == b || b == c || a == c {
            continue; // degenerate after the weld
        }
        faces += 1;
        for &(p, q) in &[(a, b), (b, c), (c, a)] {
            used[p as usize] = true;
            *edge_count.entry((p.min(q), p.max(q))).or_default() += 1;
            let (rp, rq) = (find(&mut parent, p), find(&mut parent, q));
            if rp != rq {
                parent[rp as usize] = rq;
            }
        }
    }
    let vertices = used.iter().filter(|&&u| u).count();
    let edges = edge_count.len();
    let boundary_edges = edge_count.values().filter(|&&n| n == 1).count();
    let nonmanifold_edges = edge_count.values().filter(|&&n| n >= 3).count();
    let shells = (0..parent.len() as u32)
        .filter(|&v| used[v as usize] && find(&mut parent, v) == v)
        .count();
    SurfaceTopology {
        vertices,
        edges,
        faces,
        chi: vertices as i64 - edges as i64 + faces as i64,
        shells,
        boundary_edges,
        nonmanifold_edges,
    }
}

/// Run the sidecar boolean `op` (`union` / `intersection` / `subtraction` /
/// `xor`) over `inputs` (OBJ files, in order) into `output`. `Ok(())` when
/// the binary exited successfully and wrote the file.
pub fn sidecar_boolean(
    op: &str,
    inputs: &[PathBuf],
    output: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let bin = cherchi_bin().ok_or_else(|| "sidecar binary not found".to_string())?;
    let mut cmd = Command::new(bin);
    cmd.arg(op);
    for p in inputs {
        cmd.arg(p);
    }
    cmd.arg(output);
    match run_with_timeout(cmd, timeout) {
        TimedRun::Completed(out) => {
            if !out.status.success() {
                return Err(format!(
                    "sidecar exit {:?}: {}",
                    out.status.code(),
                    String::from_utf8_lossy(&out.stderr)
                        .chars()
                        .take(400)
                        .collect::<String>()
                ));
            }
            if !output.exists() {
                return Err("sidecar wrote no output file".into());
            }
            Ok(())
        }
        TimedRun::TimedOut => Err(format!("sidecar timed out after {timeout:?}")),
        TimedRun::SpawnFailed(e) => Err(format!("sidecar spawn failed: {e}")),
    }
}

#[cfg(test)]
mod topology_tests {
    use super::*;

    fn tet(o: [f64; 3]) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let v = vec![
            o,
            [o[0] + 1.0, o[1], o[2]],
            [o[0], o[1] + 1.0, o[2]],
            [o[0], o[1], o[2] + 1.0],
        ];
        let t = vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]];
        (v, t)
    }

    #[test]
    fn a_tetrahedron_is_a_sphere() {
        let (v, t) = tet([0.0; 3]);
        let s = surface_topology(&v, &t);
        assert_eq!(
            (s.vertices, s.edges, s.faces, s.chi, s.shells),
            (4, 6, 4, 2, 1)
        );
        assert_eq!((s.boundary_edges, s.nonmanifold_edges), (0, 0));
        assert_eq!(s.genus_if_one_shell(), Some(0));
    }

    #[test]
    fn two_tetrahedra_are_two_shells() {
        let (mut v, mut t) = tet([0.0; 3]);
        let (v2, t2) = tet([5.0, 0.0, 0.0]);
        let base = v.len() as u32;
        v.extend(v2);
        t.extend(t2.iter().map(|x| [x[0] + base, x[1] + base, x[2] + base]));
        let s = surface_topology(&v, &t);
        assert_eq!((s.chi, s.shells), (4, 2));
        assert_eq!(s.genus_if_one_shell(), None);
    }

    #[test]
    fn duplicate_positions_are_welded_and_open_edges_counted() {
        // A single triangle whose corners are listed twice.
        let v = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        ];
        let t = vec![[3, 4, 2]];
        let s = surface_topology(&v, &t);
        assert_eq!(
            (s.vertices, s.edges, s.faces, s.boundary_edges),
            (3, 3, 1, 3)
        );
    }

    #[test]
    fn obj_round_trip() {
        let dir = std::env::temp_dir().join(format!("wi_obj_rt_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mesh = RenderMesh {
            vertices: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals: vec![],
            indices: vec![0, 1, 2],
            face_ranges: vec![],
        };
        let p = dir.join("t.obj");
        write_obj(&p, &mesh).unwrap();
        let (v, t) = read_obj(&p).unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!(t, vec![[0, 1, 2]]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
