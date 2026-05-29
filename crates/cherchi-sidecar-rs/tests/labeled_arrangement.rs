//! M2 integration tests: the patched sidecar emits a `LabeledArrangement`.
//!
//! See `specs/yang_m2_labeled_arrangement.md`. Cases C1-C5:
//!   C1 — two overlapping cubes (non-coplanar): shapes + subdivision.
//!   C2 — acceptance oracle: `keep_set(op)` triangle set == stock
//!        `boolean(a,b,op)` result, for Union AND Subtract (I3, the GREEN bar).
//!   C3 — coplanar overlap: at least one tri with `surface.len() == 2` (I4).
//!   C4 — determinism: identical inputs → identical result (I5).
//!   C5 — binary absent: every test self-skips on `BinaryNotFound`.
//!
//! All tests self-skip when the binary isn't built/available (mirror the skip
//! idiom in `tests/smoke.rs`). Set `CHERCHI2022_BIN` or build per
//! `docs/sidecar/cherchi2022_build_guide.md`.

use std::sync::Mutex;
use std::time::Duration;

use cad_primitives::{BoolOp, Point3};
use cherchi_rs::{Mesh, MeshBoolean};
use cherchi_sidecar_rs::{labeled_arrangement, SidecarBoolean, SidecarError};

const TIMEOUT: Duration = Duration::from_secs(30);

/// Serializes every test that reads or mutates the `CHERCHI2022_BIN` env var.
/// `cargo test` runs tests in this file on multiple threads; without this,
/// `c5` (which sets a bogus path) can race a concurrent test resolving the env
/// at call time. Poison-tolerant so one failing test doesn't cascade.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Axis-aligned unit cube with min-corner at `origin`. 8 verts, 12 outward
/// tris. Same vertex/triangle pattern as `tests/smoke.rs`.
fn unit_cube_at(origin: [f64; 3]) -> Mesh {
    let [x, y, z] = origin;
    let verts = vec![
        p(x, y, z),
        p(x + 1.0, y, z),
        p(x + 1.0, y + 1.0, z),
        p(x, y + 1.0, z),
        p(x, y, z + 1.0),
        p(x + 1.0, y, z + 1.0),
        p(x + 1.0, y + 1.0, z + 1.0),
        p(x, y + 1.0, z + 1.0),
    ];
    let tris = vec![
        [0, 3, 2],
        [0, 2, 1],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [2, 3, 7],
        [2, 7, 6],
        [1, 2, 6],
        [1, 6, 5],
        [0, 4, 7],
        [0, 7, 3],
    ];
    Mesh::new(verts, tris)
}

/// Macro-free skip helper: returns `Some(value)` when available, prints SKIP +
/// returns `None` when the binary is absent. Panics on any other error (a
/// genuine failure must not be silently skipped — P9).
fn try_or_skip<T>(r: Result<T, SidecarError>, ctx: &str) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(SidecarError::BinaryNotFound { .. }) => {
            eprintln!("[labeled_arrangement {ctx}] SKIP: binary not found; set CHERCHI2022_BIN");
            None
        }
        Err(e) => panic!("labeled_arrangement ({ctx}) failed unexpectedly: {e:?}"),
    }
}

/// Canonicalize a triangle to a winding-insensitive, exact-coordinate key:
/// the 3 vertex coordinate-triples, each compared by EXACT f64 bit-pattern,
/// with the 3 triples sorted within the triangle. Both the keep_set mapping
/// and the stock boolean output come from the same binary's
/// `computeFinalExplicitResult` (identical coordinate rescale), so kept-tri
/// coordinates are bit-identical — no tolerance is used or needed.
fn canon_tri(mesh: &Mesh, tri: [u32; 3]) -> [[u64; 3]; 3] {
    let bits = |pt: &Point3| [pt.x().to_bits(), pt.y().to_bits(), pt.z().to_bits()];
    let mut coords = [
        bits(&mesh.verts[tri[0] as usize]),
        bits(&mesh.verts[tri[1] as usize]),
        bits(&mesh.verts[tri[2] as usize]),
    ];
    coords.sort();
    coords
}

/// Unordered multiset of canonical triangles, sorted so two multisets compare
/// by `==`. (Duplicate triangles, if any, are preserved by sorting rather than
/// dedup — a true multiset.)
fn canon_multiset(mesh: &Mesh) -> Vec<[[u64; 3]; 3]> {
    let mut v: Vec<_> = mesh.tris.iter().map(|&t| canon_tri(mesh, t)).collect();
    v.sort();
    v
}

/// C1: two overlapping cubes (non-coplanar interpenetration). The arrangement
/// must label every triangle and subdivide beyond the 24 input triangles.
#[test]
fn c1_two_overlapping_cubes_shapes_and_subdivision() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.5, 0.5]);
    let Some(la) = try_or_skip(labeled_arrangement(&a, &b, TIMEOUT), "c1") else {
        return;
    };

    let n = la.mesh.tris.len();
    // I1: all per-tri label vectors aligned 1:1 with mesh.tris.
    assert_eq!(la.surface.len(), n, "surface len must equal mesh.tris len");
    assert_eq!(la.inside.len(), n, "inside len must equal mesh.tris len");
    assert_eq!(la.patch.len(), n, "patch len must equal mesh.tris len");
    assert_eq!(la.num_inputs, 2, "binary boolean has num_inputs == 2");

    // I2: every surface non-empty; every inside has num_inputs entries.
    for t in 0..n {
        assert!(
            !la.surface[t].is_empty(),
            "surface[{t}] must be non-empty (I2)"
        );
        assert_eq!(
            la.inside[t].len(),
            2,
            "inside[{t}] must have exactly 2 entries (I2)"
        );
    }

    // Subdivision happened: more tris than 12 + 12 = 24 inputs.
    assert!(
        n > 24,
        "interpenetrating cubes must subdivide beyond 24 input tris; got {n}"
    );
}

/// C2 / I3 (the acceptance oracle, M2 GREEN bar): for Union AND Subtract, the
/// triangles selected by `keep_set(op)` form the SAME canonical multiset as the
/// stock `boolean(a,b,op)` result mesh.
#[test]
fn c2_keep_set_matches_stock_boolean_union_and_subtract() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.5, 0.5]);

    let Ok(sb) = SidecarBoolean::from_env() else {
        eprintln!("[labeled_arrangement c2] SKIP: binary not found; set CHERCHI2022_BIN");
        return;
    };
    let Some(la) = try_or_skip(labeled_arrangement(&a, &b, TIMEOUT), "c2") else {
        return;
    };

    for op in [BoolOp::Union, BoolOp::Subtract] {
        let stock = sb.boolean(&a, &b, op).expect("stock boolean failed");
        let stock_set = canon_multiset(&stock);

        // Map keep_set indices → their canonical triangles in the arrangement.
        let keep = la.keep_set(op);
        let mut kept: Vec<_> = keep
            .iter()
            .map(|&i| canon_tri(&la.mesh, la.mesh.tris[i]))
            .collect();
        kept.sort();

        assert_eq!(
            kept.len(),
            stock_set.len(),
            "{op:?}: keep_set tri count ({}) must equal stock result tri count ({})",
            kept.len(),
            stock_set.len()
        );
        assert_eq!(
            kept, stock_set,
            "{op:?}: keep_set canonical triangle multiset must equal stock boolean result"
        );
    }
}

/// C3 / I4: two cubes meeting at a shared coplanar face. Cubes at [0,0,0] and
/// [1,0,0] share the x=1 plane over the unit square y,z ∈ [0,1]. The
/// arrangement must mark at least one triangle on that shared plane as
/// multi-attributed (`surface.len() == 2`) — proving the surface label is a set,
/// not a scalar, before the shape is frozen.
///
/// Geometry note: [0,0,0] + [1,0,0] gives faces that exactly coincide on x=1
/// (full unit-square overlap), the cleanest coplanar-overlap arrangement.
#[test]
fn c3_coplanar_face_yields_multi_attribution() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([1.0, 0.0, 0.0]);
    let Some(la) = try_or_skip(labeled_arrangement(&a, &b, TIMEOUT), "c3") else {
        return;
    };

    let n = la.mesh.tris.len();
    assert_eq!(la.surface.len(), n, "surface len must equal mesh.tris len");

    let multi = (0..n).filter(|&t| la.surface[t].len() == 2).count();
    assert!(
        multi >= 1,
        "coplanar shared face must yield >=1 multi-attributed tri (surface.len()==2); \
         found {multi} of {n} tris"
    );
}

/// C4 / I5: determinism. Calling `labeled_arrangement` twice on identical
/// inputs yields identical results — mesh.verts, mesh.tris, surface, inside,
/// and patch all equal.
#[test]
fn c4_determinism_identical_results() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.5, 0.5]);

    let Some(la1) = try_or_skip(labeled_arrangement(&a, &b, TIMEOUT), "c4#1") else {
        return;
    };
    let la2 =
        labeled_arrangement(&a, &b, TIMEOUT).expect("second call must succeed once first did");

    assert_eq!(
        la1.mesh.verts, la2.mesh.verts,
        "mesh.verts must be identical across runs (I5)"
    );
    assert_eq!(
        la1.mesh.tris, la2.mesh.tris,
        "mesh.tris must be identical across runs (I5)"
    );
    assert_eq!(
        la1.surface, la2.surface,
        "surface labels must be identical across runs (I5)"
    );
    assert_eq!(
        la1.inside, la2.inside,
        "inside labels must be identical across runs (I5)"
    );
    assert_eq!(
        la1.patch, la2.patch,
        "patch ids must be identical across runs (I5)"
    );
}

/// C5: binary absent → producer returns `Err(SidecarError::BinaryNotFound)`.
/// Forces a definitely-missing path and asserts the specific variant (so the
/// self-skip idiom in the other tests is exercising the real failure mode).
#[test]
fn c5_binary_absent_returns_binary_not_found() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let saved = std::env::var("CHERCHI2022_BIN").ok();
    std::env::set_var("CHERCHI2022_BIN", "/definitely/not/a/path/that/exists");

    let a = unit_cube_at([0.0, 0.0, 0.0]);
    let b = unit_cube_at([0.5, 0.5, 0.5]);
    let result = labeled_arrangement(&a, &b, TIMEOUT);

    match saved {
        Some(v) => std::env::set_var("CHERCHI2022_BIN", v),
        None => std::env::remove_var("CHERCHI2022_BIN"),
    }

    match result {
        Err(SidecarError::BinaryNotFound { .. }) => {}
        other => panic!("expected BinaryNotFound for a missing binary, got {other:?}"),
    }
}
