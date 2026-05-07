# PR-Y20-MODE-A — `HalfEdge.twin: Option<HalfEdgeIdx>`

**Author:** spec-writer-t · **Date:** 2026-05-06 · **Plan:** sub-phase 0b
**Canary:** `docs/audits/pr_y20_mode_a_canary.md` — MIXED, 91% NMM-
dominant. F0020 Ext.3 = 23 NMM + 8 MISSING; F0044 batch = 102 NMM +
2 MISSING; F0051 = 3 MISSING; F0030 = 0 Mode A. Scope per canary §4 =
NMM-branch; MISSING banked PR-Y21+. **FIP §3 + §8.**

## §1 Goal

Extend `HalfEdge.twin: HalfEdgeIdx` → `Option<HalfEdgeIdx>` to encode
non-manifold edges per Yang 2025 §4.4.2 directional-symmetry mandate.
In `topology_extract.rs` Step 7, the `[]` arm sets `twin = None` only
where `directed_edge_to_tris.get(&(v1,v0)).is_none()` (NMM). Validator
(`yang_integration.rs::validate_yang_result_topology`) ACCEPTS twin=None
only when canonical reverse is absent; PANICS otherwise (no silent
fallback per `feedback_yang_only.md`).

Post-fix per canary §5: F0020 spotlight **may stay Failed** (8 MISSING
residual); F0044 #7 likely Passed (100% NMM); #5+#6 improve 31+37→1+1;
F0051 stays Failed (pure MISSING); F0030 no change; Yang fast 10/157
→ 0 to 1+ depending on F0044 propagation. Layered-defect framing as
PR-Y19-MODE-B.

---

## §2 Reference parity contract (3 invariants)

**I1 (NMM directional-symmetry, Yang §4.4.2):** if `he_fwd` has canonical
`(v0, v1)` and `directed_edge_to_tris` lacks `(v1, v0)`, then
`arena.half_edges[he_fwd.0].twin == None`.
- *Paper:* Yang §4.4.2 mandates directional symmetry for MANIFOLD edges
  only; non-manifold geometrically allowed. Cherchi 2022 §5 is mesh-only,
  no half-edge concept.
- *Violation:* `topology_extract.rs:1241-1254` `[]` arm leaves twin at
  default `HalfEdgeIdx(0)` (set L1113); validator panics.
- *Test:* `pr_y20_mode_a_nmm_invariant` — F0044 spotlight; every HE
  matches I1 NMM-branch or I2 manifold-branch.

**I2 (manifold 1:1 preserved):** when `[the_one]` arm pairs up,
`he_fwd.twin = Some(he_rev)` AND `he_rev.twin = Some(he_fwd)`.
- *Paper:* Yang §4.4.2 + Mantyla §4.2 + Stroud §3.3.
- *Violation:* none on this path; mechanical `Some(_)` wrapping.
- *Test:* manifold branch of `pr_y20_mode_a_nmm_invariant`.

**I3 (validator distinguishes NMM from missing-edge bug):** PANICS on
`Some(t) AND he[t].twin != Some(self)` (broken symmetry); PANICS on
`None AND directed_edge_to_tris.contains_key(&(v1,v0))` (defect — rev
exists, pairing missed it); ACCEPTS `None AND !contains_key(&(v1,v0))`
(legitimate NMM).
- *Paper:* per `feedback_yang_only.md`, no silent fallbacks. Validator
  IS the contract distinguishing legitimate NMM from upstream defect.
- *Violation:* validator at `yang_integration.rs:1218-1247` lacks
  geometric-existence check; treats all twin-mismatch as defect.
- *Test:* `pr_y20_mode_a_validator_rejects_missing_edge_bug`
  (`#[should_panic]`) — synthetic arena with `twin=None` whose rev IS
  in `directed_edge_to_tris`; validator panics.

**Paper-extension framing** (`feedback_yang_brep_extension_over_cherchi_
pure_mesh.md`): Yang keeps half-edges through B-Rep reassembly; Cherchi
§5 is mesh-only. Mantyla/Stroud assume manifold input; real boolean
OUTPUT has non-manifold edges. `Option<HalfEdgeIdx>` = paper-faithful
Yang+B-Rep extension over Cherchi pure-mesh, NOT a fallback. See §10.

---

## §3 Type-system change

**Primary** `crates/kernel/src/topology/half_edge.rs:38-51`:
```rust
pub struct HalfEdge {
    pub origin: VertexIdx, pub edge: EdgeIdx,
    pub twin: Option<HalfEdgeIdx>,   // CHANGED from HalfEdgeIdx
    pub next: HalfEdgeIdx, pub prev: HalfEdgeIdx, pub loop_: LoopIdx,
}
```
Construction default `twin: None` (was `HalfEdgeIdx(0)`). Behavior-
preserving: Step 5/7 overwrite on `[the_one]`; the `[]` arm previously
left the sentinel — the bug.

**Validator** (`yang_integration.rs:1175-1316`): bounds check only when
`Some(t)`; twin-symmetry loop replaced by 3-arm I3 match; boundary-HE
counter (L1283-L1316) replaced by `is_none()` count, `n_boundary_he` →
`n_nmm_he`.

**Files touched + LOC** (grep: 150 reads + 27 writes across 16 files):

| File | r | w | LOC | pattern |
|---|---|---|---|---|
| **`topology/half_edge.rs`** | 0 | 0 | 3 | type def |
| **`boolean/topology_extract.rs`** | 83 | 7 | 25 | match; `[]` writes None |
| **`boolean/yang_integration.rs`** | 18 | 0 | 30 | I3 + plumbing |
| **`boolean/stitch.rs`** | 10 | 8 | 12 | `Some(_)` wrap |
| `waffle_kernel{,_tests}.rs` | 17 | 9 | 16 | `.expect` |
| `boolean/ssi_refinement.rs` | 17 | 1 | 12 | match |
| `boolean/{coplanar_preprocess,pipeline_oracles}.rs` | 16 | 1 | 14 | mixed |
| `tessellation/mod.rs` | 6 | 1 | 8 | `.expect` |
| `topology/{euler_ops,validate}.rs` + 6 mech sites | 9 | 0 | 11 | `.expect` |
| **Total** | **~150** | **~27** | **~131** | **16 files** |

LOC band ~120-150 (canary §5's ~40-50 was optimistic — every read site
adapts). Structural change in 4 **bold** files; rest mechanical.

**Patterns:** manifold-context reads → `.twin.expect("manifold-ctx:
…").0`; NMM-aware reads (validator, pairing) → `match he.twin { Some
=>…, None =>… }`. Pair-up writes wrap `Some(_)`; new NMM write at
`topology_extract.rs:1241-1254` `[]` arm; construction defaults at
L271+L1113 → `None`.

---

## §4 Blast-radius mitigation

**Validator plumbing** (load-bearing for I3): validator must check
`directed_edge_to_tris` for `twin=None` HEs. Two options:
- **A (preferred):** validator takes optional `&HashMap<(usize,usize),
  Vec<usize>>`. `Some` → strict NMM contract; `None` (post-boolean) →
  accepts any `twin=None`.
- **B:** plumb `Vec<(VertexIdx,VertexIdx)>` of expected-NMMs.
Implementer-x chooses in 0d.

**Mandatory grep workflow:** `grep -rn '\.twin\b' crates/kernel/src/`
+ `grep -rn '\.twin\s*='`. Audit each; pick `.expect`/match per read;
`Some(_)`/`None` per write. After every 5 files, `cargo build -p
kernel`; only then `cargo test -p kernel`.

**Orthogonal invariants preserved:** PR-Y19-MODE-B R3 routing (L864),
L808 soft-break, [twin-oracle] filter (adapts `he.twin.0 == *i` →
`he.twin == Some(self)`), PR-Y17-COPLANAR L264 panic, PR-Y16-INV
pre-validation oracle (L1312) — all STAY with internal Some/None
adaptation only.

---

## §5 Test plan (per FIP §4.2)

**Spotlight expectations** (canary §5):
- `spotlight_f0020`: **may STAY Failed**. 23/31 NMM resolved; 8/31
  MISSING residual → unpaired=8. Mark known-residual.
- `spotlight_f0044`: 31+37+36 unpaired → 1+1+0; #7 likely Passed;
  spotlight transitions Failed-many → Failed-with-2 OR Passed.
- `spotlight_f0051`: STAYS Failed (pure MISSING).
- `spotlight_f0030`: NO change (0 Mode A; downstream defect).

**New regression** `pr_y20_mode_a_nmm_invariant` in `crates/test-
harness/tests/yang_pr_y20_invariants.rs`. F0044 spotlight; every HE
matches: *manifold* `Some(t) AND he[t.0].twin == Some(HalfEdgeIdx(i))`
OR *NMM* `he.twin.is_none() AND directed_edge_to_tris.contains_key(&
(v0,v1)) AND !contains_key(&(v1,v0))`.

**New synthetic** `pr_y20_mode_a_validator_rejects_missing_edge_bug`
(`#[should_panic]`): arena with `twin=None` HE whose `(v1,v0)` IS in
synthetic `directed_edge_to_tris`; validator panics with "missing-
edge" message. Proves I3.

**Yang fast:** baseline 10/157. Target ≥10 AND any of: F0044 #7
GREEN / count → 11+. Otherwise F0020 anchor burns 1/3 (§6).

**Existing 1250+ tests:** zero regressions. Any failure = missed
read-site adaptation.

---

## §6 Anchor counters

- **F0020:** 0/3. STAYS if F0044 #7 GREEN OR Yang 10→11+; 1/3 burned
  if F0020 stays Failed AND no F0044 improvement AND Yang unchanged.
- **F0044:** 0/3 unchanged (1st F0044 anchor).
- **F0051:** 0/3 unchanged (PR-Y20 doesn't target).
- **L808 / L264 / R3 / [twin-oracle] filter:** ALL stay (orthogonal).

---

## §7 Anti-scope (explicit OUT)

- MISSING fix at L853 `is_boundary` predicate (13 cases banked PR-Y21+):
  7 F0020 non-conformal `(71,69)…(67,66)` + 6 degenerate triangles with
  repeated vertex indices.
- Non-conformal patch segmentation root cause (Step 5a `subdivide_mesh
  _pair`); degenerate-triangle filtering — both banked PR-Y21+.
- F0050 normals + Euler defect (different class). F0030 cohort (0 Mode A).
- 5 L264 panic cases (R0014/R0046/R0055/R0081/F0075).
- F0086 swiss-cheese, F0031–F0040 cylindrical, R0020/R0021, R0071.
- Removing PR-Y17 L264 panic / PR-Y19-MODE-B R3 / L808 / [twin-oracle].
- ManifoldPatchGraph, i_overlay, TAU_MODEL, perf opts, S-H clipping
  removal.

---

## §8 No-fallback (`feedback_yang_only.md`)

Validator MUST distinguish legitimate NMM (twin=None + rev ABSENT in
`directed_edge_to_tris`) from missing-edge defect (twin=None + rev
PRESENT). Latter panics; former passes. Do NOT add a code path
accepting any `twin=None` unconditionally — silently masks the 13
MISSING cases this PR banks.

Downstream consumers (tessellation, retess, brep_assembly) that
cannot handle `twin=None` at a manifold-context call site MUST
`.expect("manifold-context: <reason>")` — informative panic, NOT
silent default. MISSING fix is explicitly PR-Y21+; do NOT bundle
"while we're in here" mitigation here.

---

## §9 FIP role table

| Phase | Agent | Writes |
|---|---|---|
| 0a | canary-runner-7 (DONE) | canary memo |
| 0b | spec-writer-t (THIS) | this spec |
| 0c | test-author-k (NEW) | 2 RED tests |
| 0d | implementer-x (NEW) | type change ~131 LOC across 16 files |
| 0e | adversary-20 (NEW) | validation memo + cohort sweep + paper audit |
| 0f | team-lead | clippy/fmt/WASM/memory/commit |

---

## §10 Wrong-anchor count + paper-faithful framing

F0020 cycle 0/3 burned entering. Per `feedback_anchor_before_fix.md`
strategic-escalation (3 wrongs → reference comparison): the canary IS
the reference comparison — empirical NMM-vs-MISSING discrimination.
PR-Y20 targets the 91% mechanism. No-movement outcome suggests deeper
layer (downstream `twin=None` panic), not wrong anchor.

**Paper-faithful framing:** Yang §4.4.2 prescribes directional symmetry
for MANIFOLD edges; strict 1:1 across every directed edge is Mantyla/
Stroud manifold-CAD assumption that does NOT survive real boolean
output. `Option<HalfEdgeIdx>` is: (a) Yang+B-Rep extension over Cherchi
pure-mesh, (b) paper-faithful directional reading, (c) NOT a fallback
(I3 distinguishes legitimate-NMM from defect).

If adversary-20's paper audit reveals strict 1:1, fallback framing:
extension necessary because real boolean output diverges from paper
assumptions. **Spec commits to directional-symmetry reading; paper-
extension framing as fallback.**

---

**Open questions for implementer-x** (scope-internal; do NOT escalate
unless I1/I2/I3 contract changes): (1) plumbing A vs B (§4) — A
recommended; (2) `HalfEdge::default()` with `twin: None`? simplifies
~6 fixture sites; (3) `.expect` convention `"manifold-ctx: <reason>"`
recommended.

**Verification:** `git diff --stat` shows only this file (NEW); 10
sections non-empty; §3 lists 16 files + LOC band; §5 documents F0020
may stay Failed; §8 commits I3; §10 commits directional-symmetry.
