//! PR-YR26 RED (M8 slice b) — duplicated-triangle restoration into the
//! in/out substrate (`ArrangementSoup::in_tris` / `in_labels`).
//!
//! ## Why this oracle exists
//!
//! Yang 2025 §4.5.5 Stage-0 generates IDENTICAL meshes on a coplanar-overlap
//! region for both solids. At cherchi prep those exact duplicates dedup into
//! ONE arrangement triangle with an OR-merged surface label `{A, B}` — which
//! is correct for the OUTPUT labels (`soup.labels`, the keep-rule input) but
//! WRONG for the ray-cast in/out substrate: `in_tris`/`in_labels` must
//! describe each input as a CLOSED single-label shell, because
//! `prune_intersections_and_sort_along_ray` skips any tested triangle whose
//! label shares an id with the casting patch's surface label
//! (`booleans.cpp:680` `tested_label & patch_surface_label`). A merged
//! `{A, B}` in-label makes BOTH shells look open at the overlap, breaking
//! crossing parity, and the merged copy carries only the FIRST solid's
//! winding, breaking the back-face orientation verdict for the other solid.
//!
//! The C++ reference therefore restores every removed duplicate before ray
//! casting: `customRemoveDegenerateAndDuplicatedTriangles`
//! (booleans.cpp:179-313) records `DuplTriInfo { t_id, l_id, w }` per dropped
//! duplicate, and `addDuplicateTrisInfoInStructures` (booleans.cpp:358-393)
//! appends a fresh copy with the duplicate's OWN single label and
//! `consistentWinding`-corrected winding (booleans.cpp:1530-1539), removing
//! that label bit from the surviving copy. This test pins the port of that
//! mechanism.
//!
//! ## Fixture
//!
//! Two stacked boxes A = [0,2]³ and B = [0,2]²×[2,4] sharing the z = 2 plane
//! EXACTLY, with IDENTICAL triangulations of the shared face (same vertex
//! coordinates, same diagonal, opposite winding — A's top faces +z, B's
//! bottom faces −z). This is exactly what the yang-rs Stage-0 overlay emits
//! for the full-overlap case.

use cherchi_rs::arrangements::soup::{mesh_arrangement, ArrangementSoup, Label};
use cherchi_rs::labeled_arrangement::InputId;

const A: InputId = InputId(0);
const B: InputId = InputId(1);

/// Axis-aligned box as 12 outward-wound triangles (the BL1/BL2 fixture
/// connectivity, so the shared-face diagonals of two stacked boxes line up).
fn boxx(ox: f64, oy: f64, oz: f64, sx: f64, sy: f64, sz: f64) -> (Vec<f64>, Vec<[u32; 3]>) {
    let p = |x: f64, y: f64, z: f64| (ox + x * sx, oy + y * sy, oz + z * sz);
    let corners = [
        p(0.0, 0.0, 0.0),
        p(1.0, 0.0, 0.0),
        p(1.0, 1.0, 0.0),
        p(0.0, 1.0, 0.0),
        p(0.0, 0.0, 1.0),
        p(1.0, 0.0, 1.0),
        p(1.0, 1.0, 1.0),
        p(0.0, 1.0, 1.0),
    ];
    let mut coords = Vec::with_capacity(24);
    for (x, y, z) in corners {
        coords.push(x);
        coords.push(y);
        coords.push(z);
    }
    let tris = vec![
        [0, 2, 1],
        [0, 3, 2], // bottom (−z)
        [4, 5, 6],
        [4, 6, 7], // top (+z)
        [0, 1, 5],
        [0, 5, 4], // front (y=lo)
        [2, 3, 7],
        [2, 7, 6], // back (y=hi)
        [1, 2, 6],
        [1, 6, 5], // right (x=hi)
        [3, 0, 4],
        [3, 4, 7], // left (x=lo)
    ];
    (coords, tris)
}

/// The stacked-box soup: A = [0,2]³, B = [0,2]²×[2,4]. The shared z=2 face
/// has bit-identical vertex coordinates and identical (sorted-key) triangles
/// with OPPOSITE winding — the §4.5.5 "identical meshes" configuration.
fn stacked_soup() -> ArrangementSoup {
    let (ca, ta) = boxx(0.0, 0.0, 0.0, 2.0, 2.0, 2.0);
    let (cb, tb) = boxx(0.0, 0.0, 2.0, 2.0, 2.0, 2.0);
    let mut coords = ca;
    coords.extend_from_slice(&cb);
    let off = 8u32;
    let mut tris = ta;
    tris.extend(tb.iter().map(|t| [t[0] + off, t[1] + off, t[2] + off]));
    let mut labels: Vec<Label> = vec![vec![A]; 12];
    labels.extend(std::iter::repeat_n(vec![B], 12));
    mesh_arrangement(&coords, &tris, &labels).expect("stacked identical-overlap arrangement")
}

/// Exact coordinates of a prepped input vertex (in_tris vertices are always
/// explicit input corners), descaled by the soup multiplier.
fn in_vert(soup: &ArrangementSoup, v: u32) -> [f64; 3] {
    use cherchi_rs::arrangements::fast_trimesh::VertexCoords;
    match &soup.verts[v as usize] {
        VertexCoords::Explicit(p) => [
            p.x() / soup.multiplier,
            p.y() / soup.multiplier,
            p.z() / soup.multiplier,
        ],
        other => panic!("in_tris vertex {v} is implicit: {other:?}"),
    }
}

/// Signed volume (divergence theorem) of a label-filtered sub-shell of
/// `in_tris`. Positive ⟺ closed outward-oriented shell.
fn shell_signed_volume(soup: &ArrangementSoup, label: InputId) -> f64 {
    let mut vol = 0.0;
    for (t, tri) in soup.in_tris.iter().enumerate() {
        if !soup.in_labels[t].contains(&label) {
            continue;
        }
        let a = in_vert(soup, tri[0]);
        let b = in_vert(soup, tri[1]);
        let c = in_vert(soup, tri[2]);
        vol += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    vol
}

/// Watertight 2-manifold check on a label-filtered sub-shell of `in_tris`:
/// every undirected edge has exactly 2 incident triangles, one per direction.
fn assert_shell_watertight(soup: &ArrangementSoup, label: InputId, what: &str) {
    use std::collections::BTreeMap;
    let mut stats: BTreeMap<(u32, u32), (usize, i64)> = BTreeMap::new();
    let mut n = 0usize;
    for (t, tri) in soup.in_tris.iter().enumerate() {
        if !soup.in_labels[t].contains(&label) {
            continue;
        }
        n += 1;
        for k in 0..3 {
            let (u, v) = (tri[k], tri[(k + 1) % 3]);
            let e = stats.entry((u.min(v), u.max(v))).or_insert((0, 0));
            e.0 += 1;
            e.1 += if u < v { 1 } else { -1 };
        }
    }
    assert!(n > 0, "{what}: shell must be non-empty");
    for (edge, (count, balance)) in stats {
        assert_eq!(count, 2, "{what}: edge {edge:?} must have 2 incident tris");
        assert_eq!(balance, 0, "{what}: edge {edge:?} once per direction");
    }
}

fn assert_rel_eq(got: f64, expect: f64, what: &str) {
    let tol = expect.abs() * 1e-9;
    assert!(
        (got - expect).abs() <= tol,
        "{what}: {got} != expected {expect} (tol {tol})"
    );
}

// ════════════════════════════════════════════════════════════════════
// Oracle #1 — every in_label is a SINGLE input id after restoration
// (the C++ splits the merged {A,B} label back into one copy per input:
// booleans.cpp:388-391 `in_labels.push_back(new_label);
// in_labels[item.t_id][item.l_id] = false`).
// ════════════════════════════════════════════════════════════════════
#[test]
fn in_labels_are_single_id_after_restoration() {
    let soup = stacked_soup();
    assert_eq!(
        soup.in_tris.len(),
        soup.in_labels.len(),
        "in_labels 1:1 with in_tris"
    );
    for (t, l) in soup.in_labels.iter().enumerate() {
        assert_eq!(
            l.len(),
            1,
            "in_labels[{t}] = {l:?}: restoration must leave single-input \
             labels (merged {{A,B}} ⇒ both shells look open to the ray prune)"
        );
    }
    // The two dedup'd shared-face duplicates are restored: 12 + 12 input
    // tris, 2 dropped as duplicates, 2 restored ⇒ 24.
    assert_eq!(
        soup.in_tris.len(),
        24,
        "both copies of the 2 shared-face triangles must be present in in_tris"
    );
    let count = |l: InputId| {
        soup.in_labels
            .iter()
            .filter(|lab| lab.contains(&l))
            .count()
    };
    assert_eq!(count(A), 12, "A's closed shell has 12 triangles");
    assert_eq!(count(B), 12, "B's closed shell has 12 triangles");
}

// ════════════════════════════════════════════════════════════════════
// Oracle #2 — each restored shell is CLOSED and OUTWARD: watertight
// 2-manifold per label, signed volume +8 each (the restored copy must
// carry the duplicate's OWN winding via the `consistentWinding` w-flag,
// booleans.cpp:243/375-386 — B's bottom face points −z, not A's +z).
// ════════════════════════════════════════════════════════════════════
#[test]
fn restored_shells_are_closed_and_outward() {
    let soup = stacked_soup();
    assert_shell_watertight(&soup, A, "shell A");
    assert_shell_watertight(&soup, B, "shell B");
    assert_rel_eq(shell_signed_volume(&soup, A), 8.0, "shell A volume");
    assert_rel_eq(shell_signed_volume(&soup, B), 8.0, "shell B volume");
}

// ════════════════════════════════════════════════════════════════════
// Oracle #3 — the OUTPUT surface labels are untouched by restoration:
// the arrangement's z=2 overlap triangles keep the OR-merged {A, B}
// label (the keep-rule input, exactly as the C++ arrangement labels do).
// ════════════════════════════════════════════════════════════════════
#[test]
fn output_surface_labels_keep_or_merged_overlap() {
    let soup = stacked_soup();
    let multi: Vec<&Label> = soup.labels.iter().filter(|l| l.len() > 1).collect();
    assert_eq!(
        multi.len(),
        2,
        "the 2 shared-face triangles carry the multi-label {{A,B}}"
    );
    for l in multi {
        let mut c = l.clone();
        c.sort_unstable();
        assert_eq!(c, vec![A, B]);
    }
}

// ════════════════════════════════════════════════════════════════════
// Oracle #4 — determinism: two runs produce identical soups.
// ════════════════════════════════════════════════════════════════════
#[test]
fn restoration_is_deterministic() {
    let s1 = stacked_soup();
    let s2 = stacked_soup();
    assert_eq!(s1.in_tris, s2.in_tris);
    assert_eq!(s1.in_labels, s2.in_labels);
}
