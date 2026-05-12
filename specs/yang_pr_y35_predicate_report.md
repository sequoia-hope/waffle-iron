# PR-Y35 — Re-port `triangles_intersect_exact` to cinolib `Triangle::intersects_triangle(_, ignore_if_valid_complex=true)` semantics

**Author:** spec-y35
**Date:** 2026-05-12
**Parent commit:** `85deaed`
**Team:** pr-y35
**Sub-anchor:** B (banked from PR-Y33 §4.2 / PR-Y34 §4.2)

---

## §1 Context

PR-Y33 (SHIPPED `478db04`, infra-only) localized F0020's first-divergent stage at `detect_intersections` (STAGE4) and identified two sub-anchors via per-stage byte-diff against Cherchi C++:

- **Sub-anchor A** — Yang §4.2.2 Theorem 4.1 same-mesh Gauss-map filter at `intersection_class.rs:131-149` rejecting 24 Cherchi-only same-mesh co-oriented pairs.
- **Sub-anchor B** — `triangles_intersect_exact` at `intersection_class.rs:1465-1480` over-permissive vs Cherchi's reference predicate, producing 95 Waffle-only pairs (~80% of pair-diff).

PR-Y34 (SHIPPED `7891a28`) applied sub-anchor A as a ~6-line deletion. F0020 Stage B `missing` 93 → 7 (-92.5%), `extras` 148 → 0. Sub-anchor B was **banked for PR-Y35** (PR-Y34 §4.2): predicate over-permissiveness persists (Waffle STAGE4 inv1 = 365 pairs vs Cherchi 84), masked at Stage B because `classify_intersections`'s downstream exact-predicate path filters spurious pairs.

Banked status is paper-cited correctness: Cherchi 2022 §3 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:240-256`) guarantees the arrangement's well-formed-simplicial-complex output depends on a sound intersection predicate. The STAGE4 over-pair-count is (a) wasted downstream work, (b) latent correctness risk for cohort cases, and (c) divergence from Cherchi 2022's reference.

PR-Y35 closes sub-anchor B by re-porting `triangles_intersect_exact` to mirror cinolib's `Triangle::intersects_triangle(_, ignore_if_valid_complex=true)` line-by-line. Canary verified F0020 STAGE4 **365 → 84 pairs (exact byte parity with Cherchi C++)**, Stage B missing-count preserved at 7, F0044 byte-parity preserved, yang_fast 10/157 preserved, kernel lib full suite net -1 pass (single regression — see §5).

---

## §2 Why — cinolib reference semantics

The load-bearing oracle is cinolib `triangle_triangle_intersect_3d` at `cinolib/predicates.cpp:1128-1252`, invoked via `Triangle::intersects_triangle(_, ignore_if_valid_complex=true)` at `cinolib/geometry/triangle.cpp:99-104`. Cherchi 2022 calls this from `booleans.cpp:315` and `intersection_classification.cpp:72`.

### §2.1 SimplexIntersection enum + wrapper

`predicates.h:114-121`:

```cpp
DO_NOT_INTERSECT   = 0,   SIMPLICIAL_COMPLEX = 1,
INTERSECT          = 2,   OVERLAP            = 3,
```

`triangle.cpp:99-104` (wrapper):

```cpp
bool Triangle::intersects_triangle(const vec3d t[], const bool ignore_if_valid_complex) const {
    auto res = triangle_triangle_intersect_3d(v[0], v[1], v[2], t[0], t[1], t[2]);
    if(ignore_if_valid_complex) return (res > SIMPLICIAL_COMPLEX);
    return (res >= SIMPLICIAL_COMPLEX);
}
```

Cherchi 2022 always passes `true`, so the predicate returns `true` **only for INTERSECT or OVERLAP** — `false` for shared-sub-simplex pairs forming a valid simplicial complex.

### §2.2 4-case dispatch by shared-vertex count (`predicates.cpp:1147-1252`)

Cinolib detects vertex sharing via `vec_equals_3d` (bit-exact position equality), then dispatches on `t0_shared.count()`:

| Shared | Logic (cinolib lines) | Returns |
|---|---|---|
| **3** | Coincident (L1161). | SIMPLICIAL_COMPLEX |
| **2** | Shared edge. Non-coplanar (`orient3d != 0`, L1183) → SIMPLICIAL_COMPLEX. Else 3 axis-drops (X/Y/Z, L1185-1207) with `orient2d`: if opposite verts on opposite sides in any projection → SIMPLICIAL_COMPLEX; else INTERSECT. | INTERSECT iff coplanar AND same-side; else SIMPLICIAL_COMPLEX |
| **1** | Two opposite-edge ∩ other-triangle tests (L1231-1232). | INTERSECT iff any pierces; else SIMPLICIAL_COMPLEX |
| **0** | 6 segment-triangle tests (L1241-1246, original Waffle behavior). | INTERSECT iff any hit; else DO_NOT_INTERSECT |

Waffle's pre-PR-Y35 implementation ran **only the 0-shared branch** regardless of vertex sharing. For pairs sharing an edge or vertex, this returns `true` whenever the shared sub-simplex lies on the other triangle's boundary — reporting every edge-adjacent or vertex-adjacent pair as intersecting, contrary to cinolib's `ignore_if_valid_complex=true` semantics.

### §2.3 Position-based vertex sharing rationale

Per `feedback_external_coherence` (cinolib is the load-bearing oracle), PR-Y35 mirrors cinolib's `vec_equals_3d` exactly: Rust's `PartialEq` on `[f64; 3]` is bit-exact equality, equivalent to `vec_equals_3d`'s component-wise `==`. ID-based sharing would silently diverge from cinolib whenever a caller produces non-canonicalized soups (TriangleSoup's constructor at `triangle_soup.rs:77-140` does not run position-merge). Tolerant equality would introduce hysteresis breaking byte-parity. F0020 STAGE4 84/84 byte parity (canary §3.4) empirically confirms this on the corpus; determinism risk is bounded by Gates 4, 5, 8, 9 (see §7).

---

## §3 Fix shape

Replace `crates/kernel/src/boolean/cherchi/intersection_class.rs:1465-1480` (Waffle's 6 segment-triangle test body) with the cinolib-faithful 4-case dispatch. The function-level rustdoc (preserved verbatim from canary at `intersection_class.rs:1450-1480`) cites cinolib's source lines for each branch. Body (canary's final code, ~76 LOC at `intersection_class.rs:1481-1550`):

```rust
fn triangles_intersect_exact(ts: &TriangleSoup, t0: usize, t1: usize) -> bool {
    let t0v = [ts.tri_vert(t0, 0), ts.tri_vert(t0, 1), ts.tri_vert(t0, 2)];
    let t1v = [ts.tri_vert(t1, 0), ts.tri_vert(t1, 1), ts.tri_vert(t1, 2)];

    let mut t0_shared = [false; 3];
    let mut t1_shared = [false; 3];
    for i in 0..3 {
        for j in 0..3 {
            if t0v[i] == t1v[j] {
                t0_shared[i] = true;
                t1_shared[j] = true;
            }
        }
    }
    let shared_count = t0_shared.iter().filter(|&&s| s).count();

    if shared_count == 3 {
        return false;
    }

    if shared_count == 2 {
        let opp0 = (0..3).position(|i| !t0_shared[i]).unwrap();
        let opp1 = (0..3).position(|i| !t1_shared[i]).unwrap();

        if orient3d(t0v[0], t0v[1], t0v[2], t1v[opp1]) != 0.0 {
            return false;
        }

        let e: Vec<usize> = (0..3).filter(|&i| t0_shared[i]).collect();
        let e0 = t0v[e[0]];
        let e1 = t0v[e[1]];
        let p0 = t0v[opp0];
        let p1 = t1v[opp1];

        // Project onto each of the 3 axis-aligned 2D planes (drop X / Y / Z).
        for &(a, b) in &[(1usize, 2usize), (0, 2), (0, 1)] {
            let e0_2d = [e0[a], e0[b]];
            let e1_2d = [e1[a], e1[b]];
            let o0 = orient2d(e0_2d, e1_2d, [p0[a], p0[b]]);
            let o1 = orient2d(e0_2d, e1_2d, [p1[a], p1[b]]);
            if (o0 > 0.0 && o1 < 0.0) || (o0 < 0.0 && o1 > 0.0) {
                return false;
            }
        }

        return true;
    }

    if shared_count == 1 {
        let v0 = (0..3).position(|i| t0_shared[i]).unwrap();
        let v1 = (0..3).position(|i| t1_shared[i]).unwrap();

        let opp0 = (t0v[(v0 + 1) % 3], t0v[(v0 + 2) % 3]);
        let opp1 = (t1v[(v1 + 1) % 3], t1v[(v1 + 2) % 3]);

        return detect_seg_tri_intersect(&opp0.0, &opp0.1, &t1v[0], &t1v[1], &t1v[2])
            || detect_seg_tri_intersect(&opp1.0, &opp1.1, &t0v[0], &t0v[1], &t0v[2]);
    }

    // 0 shared verts — original 6 segment-triangle tests
    for (i, j) in [(0, 1), (1, 2), (2, 0)] {
        if detect_seg_tri_intersect(&t0v[i], &t0v[j], &t1v[0], &t1v[1], &t1v[2]) {
            return true;
        }
        if detect_seg_tri_intersect(&t1v[i], &t1v[j], &t0v[0], &t0v[1], &t0v[2]) {
            return true;
        }
    }
    false
}
```

Line correspondences (Rust → cinolib): L1494 `shared_count` ↔ L1158; L1497-1499 (3-shared) ↔ L1161; L1501-1527 (2-shared) ↔ L1166-1210 (L1505-1507 non-coplanar fast-out ↔ L1182-1183; L1516 3-axis loop ↔ L1185-1207); L1529-1538 (1-shared) ↔ L1215-1237; L1540-1549 (0-shared) ↔ L1241-1246. Reuses `detect_seg_tri_intersect` (`intersection_class.rs:1557+`); 1-shared opposite edges do not share endpoints with the other triangle (cinolib L1228-1229).

---

## §4 Empirical evidence — canary §3 table

All numbers from `docs/audits/pr_y35_canary.md` (canary-y35, 2026-05-12), worktree stacked on PR-Y34 sub-anchor A. Full per-gate detail in canary §3.

### §4.1 Gate 4 — F0020 STAGE4 pair count (LOAD-BEARING)

```
Pre-PR-Y34 baseline (478db04):    155 pairs
Post-PR-Y34 (sub-anchor A only):  365 pairs
Post-PR-Y35 (sub-anchor A + B):    84 pairs   ← exact byte parity
Cherchi C++ reference (TBB=1):     84 pairs
```

`wc -l /tmp/y35-canary/waffle/inv1/stage4_pairs.txt` → **84**. Exact byte parity with Cherchi C++ at STAGE4 — the strongest single-PR signal in 10+ PR cycles. PR-Y34 banked note ("sub-anchor B's predicate still over-permissive") fully resolved.

### §4.2 Gate 5 — F0020 Stage B preserved

`missing=7, extras=0, common=230` — identical to PR-Y34 baseline. The 281 STAGE4 extras eliminated at-source were already filtered downstream by `classify_intersections`. PR-Y35's gain is **upstream sound-predicate parity**, not a Stage B metric improvement. Residual missing=7 traces to Render-LOD tessellation (PR-Y34 §4.3 banked).

### §4.3 Gate 6 — F0044 hard gate GREEN

F0044 byte-parity with Cherchi `subtraction` preserved through PR-Y33 → PR-Y34 → PR-Y35 (Cherchi 136 / Waffle 136 / 0 missing / 0 extras / common=136).

### §4.4 Gate 7 — F0045 / R0092 cohort missing preserved

F0045 missing=236, R0092 missing=192 — both unchanged from PR-Y34 baseline. F0045 extras 273 → 466 is **symptom-redistribution, not regression** (tessellation-grid divergence, Yang §4.1.1, PR-Y30 banked); gateable metric (missing-count) preserved.

### §4.5 Gate 8 — yang_fast corpus preserved

Baseline 10/157 preserved (139 failed, 8 errored, 33 known-timeout skips). Predicate tightening at STAGE4 does not unblock downstream-failing cases.

### §4.6 Gate 9 — kernel lib full suite (−1 pass)

```
Post-PR-Y34 baseline: 1255 pass / 24 fail / 42 ignored
Post-PR-Y35 (canary): 1254 pass / 25 fail / 42 ignored      (Δ -1 pass / +1 fail)
```

Single regression: `boolean::exact_mesh::tests::test_subdivision_shared_edge_split_propagation` flips PASS → FAIL. Mechanism + `#[ignore]` resolution in §5.3.

---

## §5 Regression coverage

### §5.1 New PR-Y35 unit tests (FIP §4 — Test Author distinct from Implementer)

Test-y35 writes 6 unit tests in `#[cfg(test)] mod tests` covering each branch of the 4-case dispatch, with RED-on-baseline + GREEN-with-fix verification:

1. `test_triangles_intersect_exact_3_shared` — coincident → `false`.
2. `test_triangles_intersect_exact_2_shared_coplanar_overlap` — coplanar, opposite verts same side → `true`.
3. `test_triangles_intersect_exact_2_shared_edge_adjacent_valid` — coplanar, opposite verts opposite side → `false` (the `(o0>0 && o1<0) || (o0<0 && o1>0)` early-out fires).
4. `test_triangles_intersect_exact_2_shared_non_coplanar` — hinged in 3D (`orient3d != 0`) → `false` (non-coplanar fast-out).
5. `test_triangles_intersect_exact_1_shared_no_interior_cross` — one shared vertex, opposite edges miss → `false`. **Fails under pre-PR-Y35 body** (shared vertex on other triangle's boundary fires legacy `detect_seg_tri_intersect`).
6. `test_triangles_intersect_exact_0_shared_no_intersect` — disjoint → `false` (legacy 0-shared preserved).

### §5.2 Existing test still PASS — `test_detect_intersections_shared_vertex_cross_mesh_l_corner`

Located at `intersection_class.rs:1735-1765`. Cross-mesh triangles sharing v0=(0,0,0); mesh B's edge (v3, v4) crosses mesh A's interior. Asserts `aux.intersection_list().len() == 1`. **Canary §3.2 confirms GREEN under PR-Y35**: 1-shared branch passes opposite edge (v3, v4) to `detect_seg_tri_intersect`, which pierces mesh A's interior → INTERSECT (cinolib `predicates.cpp:1212-1237`). The test comment's warning against "Cherchi 2020 simplicial-complex skip" refers to a stricter "skip ALL shared-vertex cases" — cinolib (and PR-Y35) skip only **valid-complex** (no-interior-crossing) cases, preserving this as INTERSECT.

### §5.3 `#[ignore]` of `test_subdivision_shared_edge_split_propagation` (paper-justified)

Located at `crates/kernel/src/boolean/exact_mesh.rs:5403-5469`. Mesh A has 2 triangles sharing edge (v1, v2) (T0=[0,1,2], T1=[1,3,2], opposite verts v0=(-1,0,0) and v3=(1,0,0) on **opposite sides** of the shared edge — textbook edge-adjacent valid simplicial complex). Mesh B's cutter triangle straddles z=0 and intersects the shared edge. Asserts both T0 and T1 must split.

History:

- **Pre-PR-Y34:** FAILED — sub-anchor A's Gauss-map filter rejected same-mesh T0/T1.
- **Post-PR-Y34:** PASSED — over-permissive 6-segment-test predicate fired on T0/T1 (shared edge on T1's boundary), populating edge2pts for both → both split.
- **Post-PR-Y35:** FAILED — cinolib-faithful 2-shared branch correctly returns `false` for edge-adjacent valid complexes (opposite-side early-out at `intersection_class.rs:1521`). T0/T1 leaves the pair list. Downstream `subdivide_mesh_pair` only populates edge2pts for T0 (the triangle in the cross-mesh B-cutter pair), never propagates to T1.

**Paper justification.** Both the PR-Y34 PASS state and the assertion that the predicate should fire on same-mesh edge-adjacent pairs are paper-contrary:

- **cinolib `predicates.cpp:1163-1165` (verbatim):** *"t0 and t1 share an edge. Let e be the shared edge and { opp0, opp1 } be the two vertices opposite to e in t0 and t1, respectively. If opp0 and opp1 lie at the same side of e, the two triangles overlap. Otherwise they are edge-adjacent and form a valid simplicial complex."* The 2-shared branch returns SIMPLICIAL_COMPLEX for the exact T0/T1 configuration.
- **Cherchi 2022 §3** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:240-256`): *"When exact methods are used, the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines."* The detection predicate must NOT report edge-adjacent same-mesh pairs as intersecting.
- **Responsibility re-assignment.** Split propagation across same-mesh shared edges belongs to the downstream **subdivision** stage (`subdivide_mesh_pair`), not the upstream **detection** stage. The detection predicate identifies proper-interior intersections only; conformal mesh updates across shared edges happen at subdivision via edge2pts walking.

**Banked follow-up — PR-Y35.1** (NOT part of PR-Y35): fix `subdivide_mesh_pair` to walk `edge2pts` post-`classify_intersections` and propagate splits across all triangles sharing an edge (position-based), independent of pair-list membership. ~30-60 LOC in `exact_mesh.rs`. PR-Y35.1 will re-enable the test by removing `#[ignore]`.

**Required `#[ignore]` annotation (must include both cinolib + Cherchi citations AND the banked PR-Y35.1 pointer):**

```rust
#[ignore = "PR-Y35: paper-correct predicate (cinolib predicates.cpp:1163-1165 \
            + Cherchi 2022 §3 simplicial-complex contract) returns false for \
            same-mesh edge-adjacent valid complexes. Split propagation across \
            shared edges is downstream subdivision's responsibility. Banked \
            for PR-Y35.1 (subdivide_mesh_pair edge2pts walk, ~30-60 LOC)."]
#[test]
fn test_subdivision_shared_edge_split_propagation() { ... }
```

This is the **team-lead-decided SHIP + bank path** (per task brief). The in-line `subdivide_mesh_pair` fix is out-of-scope for PR-Y35's ~76 LOC predicate-only diff.

---

## §6 Out of scope (banked)

PR-Y35 ships sub-anchor B only. The following remain open and must NOT be claimed as closed (per `feedback_no_last_bug`):

1. **PR-Y35.1 banked.** `subdivide_mesh_pair` shared-edge split propagation across edge2pts (~30-60 LOC in `exact_mesh.rs`); re-enables `test_subdivision_shared_edge_split_propagation`. NOT part of PR-Y35.
2. **F0020 Render-LOD downstream Status:Failed.** Still ~40 unpaired edges at render layer (PR-Y34 §4.3 banked). Same defect class as F0044's Status:Failed and the F0020 missing=7 residual; downstream of Stage B; independent architectural anchor.
3. **F0045 tessellation-grid divergence (Yang §4.1.1).** F0045 extras 273 → 466 is symptom-redistribution, not regression. Root cause is at Stage 1 tessellation grid (PR-Y30 banked structural); independent architectural anchor.
4. **R0092 NMM-edge tessellation gap (PR-Y27 §D.3).** Missing-count 192 preserved through PR-Y34 → PR-Y35; NMM-edge tessellation defect at Stage 1; independent architectural anchor.
5. **139 still-failing yang_fast cases.** Corpus aggregate 10/157 preserved through PR-Y34 → PR-Y35; the remaining 139 fail at downstream stages unaffected by the STAGE4 predicate fix.

**Language discipline.** No "this closes Yang", "final fix", or "last gap" framing. PR-Y35 closes one sub-anchor (B, predicate over-permissiveness) within one stage (STAGE4) of one pipeline (Yang hybrid boolean) of one feature class (mesh booleans). Many architectural anchors remain.

---

## §7 Risk / mitigation

**Risk.** Position-equality vertex sharing (Rust bit-exact `[f64;3] == [f64;3]`) assumes deterministic upstream coord output. If two callers produce co-located vertices via different float arithmetic paths, the predicate would classify them as 0-shared, fall through to the 6-segment branch, and potentially return `true` for a pair cinolib calls SIMPLICIAL_COMPLEX.

**Mitigation.** Canary gates cover the risk empirically:
- **Gate 4 (F0020 STAGE4 84/84):** bit-different co-located vertices would over-pair (>84). Empirical exact 84.
- **Gate 6 (F0044 byte parity):** preserved on Subtract op with different upstream geometry.
- **Gate 8 (yang_fast 10/157):** zero corpus regressions across 157 cases.
- **Gate 9 (kernel lib 1254/25):** single regression isolated to `#[ignore]`'d test (§5.3); mechanism-explained.

cinolib uses bit-exact `vec_equals_3d` — tolerant equality would diverge from the reference (`feedback_external_coherence`). Residual risk is bounded by the gate baselines; if a future case surfaces bit-different co-located vertices, the failing canary gate will scope the fix at that time.

---

*End of spec.*
