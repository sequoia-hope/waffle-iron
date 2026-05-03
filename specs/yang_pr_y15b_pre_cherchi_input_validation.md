# PR-Y15b — Pre-Cherchi input validation for the F0002-class minority

**Status:** SPEC (FIP §3.2 — Phase 1).
**Anchor empirical evidence:** `docs/audits/pr_s2_inputcheck_corpus_findings.md`
§3 (interesting cell — Waffle=Failed × Cherchi=valid) and §4
(`combined_failures` bucket — 40 unique cases).
**Reference parity required:** YES. Per CLAUDE.md commit `4808f2e`,
the strategic-escalation rule (PR12/PR13/PR-Y14a/b/c — four wrong
anchors before reference parity was instituted), and PR-S2's headline
finding that 78% of pre-Cherchi inputs are Cherchi-VALID, this PR's
correctness criterion includes Cherchi 2022 C++ sidecar
`mesh_booleans_inputcheck` differential confirmation on the 40
`combined_failures` cases. Sidecar already built per PR-S1 (commit
`17792eb`) — no separate build PR required.
**Supersedes/amends:** Supersedes the F0002-class scope of PR-Y14c
(`specs/yang_pr_y14c_cherchi_lpi_canonicalization.md`), which mis-anchored
the F0002 defect at Cherchi-internal LPI canonicalization. PR-S2's
sweep proved the defect is upstream of the Cherchi sidecar — the
inputcheck binary rejects Waffle's pre-Cherchi mesh outright on these
40 cases. The PR-Y14c body remains for audit trail (manager handles
the SUPERSEDED header).
**Plan reference:** `/home/claude/.claude/plans/reactive-juggling-sloth.md`
PR-S3 deliverable 2.

---

## 1. Goal

The 40 unique cases listed in PR-S2 §4 `combined_failures` bucket
produce pre-Cherchi A and B meshes that pass `mesh_booleans_inputcheck`
on all five axioms (M, W, LO, GO, I all `passed`). On success, those
cases either:

- (a) pass under `YANG_BOOLEAN=1`, OR
- (b) fail at a strictly later pipeline stage with the Cherchi
  sidecar reporting `valid` on both A and B.

Either outcome satisfies the goal. An "unprincipled fix that masks the
symptom" (P9, A15.6) is explicitly forbidden — see §6 and I4.

The reproducer commitment (per §10) is the full 40-case set re-running
through PR-S2's sweep harness. Spot reproducers for development:
F0002, F0004, F0005, F0006 (clean F-cases, all in `combined_failures`)
and R0014, R0015, R0017 (R-cases, also in `combined_failures` per
PR-S2 §4). F0005 has a different Stage-A signature (16 unpaired + 153
multi vs F0002's 0 unpaired + 50 multi) — see §6.4 for handling
guidance if F0005 doesn't migrate to `valid`.

**Control reproducer:** F0003 (`pass-boss-only`, both sides
`non_watertight` per PR-S2 §4 — Waffle-Passing case with leaky
pre-Cherchi meshes that don't manifest as a Waffle failure because no
boolean follows). The fix MUST NOT regress F0003's `pass-boss-only`
Waffle status. Even if F0003's `non_watertight` mask does NOT migrate
to `valid` post-fix (out of scope per §9), F0003's `waffle_status`
in `app/tests/cases/assay/results.json` MUST remain `Passed`. F0003
guards against tessellation-side fixes that accidentally break
pass-boss-only paths.

## 2. Parameters

This is a bug fix; no new user-facing parameters are introduced.

**Implementation parameters (courtesy table — implementer's choice
which to consume; the spec authority is §1, §4, §10):**

| Parameter | Default | Source |
|---|---|---|
| Canonical-quantize scale | `crate::units::QUANT_NANOMETER_SCALE` (= `1e9`) | reuse — same as `topology_extract.rs:375-393`, `oracles/conformal_mesh.rs::check_conformal`, and PR-Y14b's coplanar dedup |
| Per-call canonical-key map (if used) | empty `BTreeMap<[i64;3], VertexIdx>` per outer pipeline invocation | new (see §3 for sites) |
| CDT triangulation primitive | existing kernel CDT (per Yang §4.1.2) | reuse — DO NOT introduce a new triangulation library |

No `Cargo.toml` changes. No new public API. No new env vars (the fix
is unconditional once landed; it does NOT add a feature flag — A15.6
forbids feature-gated correctness toggles in this pipeline).

## 3. Branch table

The PR-S2 TSV's `cherchi_detail` column groups the 51
`combined_failures` rows (40 unique cases) into the failure-mask
sub-classes below. Implementer SHALL re-tally from the TSV at
fix-time (the sweep mutates `results.json`; the TSV is the
ground-truth snapshot). The masks observed at spec-write time
(2026-05-03 sweep):

| Mask `(M,W,LO,GO,I)` | Rows | Sub-class | Fix-shape commitment |
|---|---:|---|---|
| `(0,1,0,0,1)` — W+I | 15 | Watertight + Intersection failure (no manifold issue) | Coplanar preprocess per Yang §4.5.5 — overlap region must be a SHARED trimmed surface with IDENTICAL sampling on A and B; the W+I co-failure indicates non-shared boundary samples that produce both leaks AND apparent self-intersection where A's and B's faces meet at the overlap rim. |
| `(1,1,0,0,1)` — M+W+I | 9 | F0002 reference signature | Tessellation per Yang §4.1.1 (per-face fan unification — corner verts must coincide bit-exactly across adjacent faces) AND coplanar preprocess per Yang §4.5.5. The 8-way canon-0 cluster from PR-Y14a's findings memo §1 lives here. |
| `parse-error` | 9 | Cherchi sidecar produces no parseable check-line output; classifier records as `parse-error`. Per TSV re-grep at spec time, this row covers AT LEAST 2 distinct sub-modes: **Mode A** (3 cases) — explicit `WARNING: adding duplicated poly!` emitted before any check line (manifold-class symptom — Cherchi's loader collapses a duplicate triangle and the loaded mesh diverges from the OBJ file); **Mode B** (6 cases) — empty `cherchi_detail` (root cause TBD by implementer; possibly the 200-char `cherchi_detail` truncation cap eating a long warning, possibly a different parse-failure mode entirely). | Implementer SHALL classify both modes as `combined_failures` per the TSV. Mode A's fix-shape is M+W+I per Yang §4.1.1. Mode B's fix-shape is TBD pending root-cause identification (re-run sidecar manually on a Mode B case; if Mode B is truncation, lift the 200-char cap in the sweep and re-classify; if Mode B is a distinct parse failure, the fix-shape may need to split — implementer's call whether to enumerate the second sub-shape inline or defer to PR-Y15b.1). |
| `(0,1,1,0,1)` — W+LO+I | 6 | Watertight + Local-Orientation + Intersection | Coplanar preprocess (overlap rim) AND local face-winding consistency on the rim per Yang §4.5.5 Fig. 16 (the shared trimmed surface inherits orientation from BOTH A and B; if A and B disagree, LO fires). |
| `(1,1,1,0,1)` — M+W+LO+I | 5 | All but Global-Orientation | Same as M+W+I plus the LO branch above. |
| `(1,1,1,1,1)` — all five | 3 | Maximally-malformed | Same as M+W+LO+I plus a global-orientation pass — usually a side whose overall winding got inverted by an Euler-op error during coplanar split; investigate `split_brep_for_coplanar_pairs` orientation-preservation invariants. |
| `(1,1,0,0,0)` — M+W only | 2 | Manifold + Watertight, no Intersection | Tessellation per Yang §4.1.1 — adjacent faces sharing a boundary edge must use identical edge-vertex sequences (T-junction prevention per Yang §4.1.2 CDT). M without I means edges have ≥3 incident triangles (non-manifold) but no triangle-triangle intersection — classic T-junction signature. |
| `(0,1,1,0,0)` — W+LO only | 1 | Watertight + Local-Orientation, no Intersection | Coplanar preprocess orientation handling on a non-self-intersecting boundary. |
| `(1,1,0,1,1)` — M+W+GO+I | 1 | Manifold + Watertight + Global-Orientation + Intersection (no LO) | Edge case — likely a per-component winding flip during coplanar split. Same fix family as the `(1,1,1,1,1)` case. |

Total: 51 rows / 40 unique cases (some cases contribute one row per
side; the side-asymmetric cases have one side `combined_failures` and
the other in another bucket — implementer SHALL also fix the
non-`combined_failures` side if it shifts during the fix; see I6).

The 9 `parse-error` rows are NOT a separate fix-shape — they are
M-class symptoms manifesting before Cherchi reaches the check phase.
The branch table groups them with M+W+I.

The fix touches THREE Waffle code areas, in order of expected blast
radius (per Yang §4.1.1, §4.1.2, §4.5.5):

1. **Tessellation per-face fan unification** (`crates/kernel/src/tessellation/`):
   adjacent faces of the same B-Rep solid must produce a single shared
   vertex at every shared corner. Likely site: the per-face fan
   triangulator emits per-face vertex copies that the
   `dedup_mesh_vertices` post-pass should collapse, but the dedup may
   be quantized too coarsely OR may run AFTER the manifold check
   point. Implementer SHALL verify the empirical anchor BEFORE coding
   per `feedback_anchor_before_fix.md` (eprintln canary at the
   suspected site, run F0002 with `YANG_DUMP_OBJ_BASE` set, confirm
   the canary fires).
2. **CDT boundary re-triangulation** (`crates/kernel/src/tessellation/`
   or wherever the per-face triangulator lives): per Yang §4.1.2,
   each face's boundary edge MUST be subdivided to match the adjacent
   face's edge vertex sequence — no T-junctions. The M+W-only sub-class
   above is the canonical T-junction signature.
3. **Coplanar preprocess shared-boundary sampling**
   (`crates/kernel/src/boolean/coplanar_preprocess.rs`,
   `split_brep_for_coplanar_pairs`): per Yang §4.5.5, the overlap
   region must produce IDENTICAL sample points on both A and B. PR-Y14b
   landed canon-key dedup at corner overlap verts; the W+I and W+LO
   sub-classes above suggest there are additional sample sites (interior
   subdivision points on shared edges, not just corners) where
   PR-Y14b's dedup did not apply. Per `feedback_anchor_before_fix.md`,
   implementer SHALL instrument and confirm before committing.

Each site uses a per-call `BTreeMap<[i64;3], VertexIdx>` lookup table
(determinism per I5). Branch additions are PURELY ADDITIVE — no
existing branch is removed or weakened. No tolerance constants beyond
`QUANT_NANOMETER_SCALE` are introduced (I4).

## 4. Invariants

These statements MUST hold post-fix and are individually measurable:

**I1 — Yang §4.1.1 bijective tessellation closed-watertight-manifold
output.** For every Waffle solid handed to `subdivide_mesh_pair_full_cherchi`,
the per-face tessellation collectively forms a closed watertight
manifold (every directed edge has exactly one reverse counterpart;
each undirected edge has exactly two incident triangles). Measured by
`mesh_booleans_inputcheck` reporting `Manifold check: passed` and
`Watertight check: passed`. Source: Yang 2025 §4.1.1.

**I2 — Yang §4.1.2 CDT boundary re-triangulation T-junction
prevention.** Adjacent faces share identical vertex sequences along
their shared boundary edge. No T-junctions at face boundaries.
Measured by the M+W-only sub-class of PR-S2's `combined_failures` —
post-fix, that sub-class drops to 0. Source: Yang 2025 §4.1.2.

**I3 — Yang §4.5.5 shared trimmed surface identical sampling.** For
any coplanar face pair `(f_a, f_b)`, the overlap region's boundary
samples are bit-identical between A's and B's tessellated meshes (no
sub-picometer drift between geometrically-identical points). Measured
by `mesh_booleans_inputcheck` reporting `Watertight check: passed`
AND `Intersection check: passed` on both A and B (the W+I co-failure
signature drops to 0). Source: Yang 2025 §4.5.5.

**I4 — A15.6 architectural integrity (no tolerance escalation, no
boundary chaining).** No new tolerance constants introduced beyond
`QUANT_NANOMETER_SCALE`. No `tau_weld` widening, no progressive
pairing, no boundary-edge chaining recovery path, no greedy
twin-pairing. The fix is purely additive (per-call canonical-key
dedup tables) over existing Euler-operator and tessellation paths.
Measured by reviewer diff inspection. Source: A15.6.

**I5 — Determinism preserved.** Two consecutive runs of
`cherchi_inputcheck_corpus_sweep` on the same source produce
byte-identical TSV output. Dedup uses `BTreeMap` (deterministic
iteration order), not `HashMap`. Source: P9 + the deterministic-test
philosophy in `CLAUDE.md`.

**I6 — No regression on the 78% Cherchi-valid cohort.** The 295
`valid` rows from PR-S2's sweep stay `valid` post-fix (the fix is
additive and cannot make a previously-valid mesh invalid). The 18
`non_watertight`, 4 `bad_orientation`, and 2 `self_intersecting`
single-axiom rows MAY shift to `valid` (welcome side effect) or stay
in their bucket (out-of-scope for PR-Y15b — see §9). Measured by
re-running the PR-S2 sweep post-fix and computing per-bucket diffs.
Source: PR-S2 §3 — the dominant defect is downstream; PR-Y15b must
not perturb it.

**I7 — Architectural integrity: additive over existing Euler-op
code paths.** Fix sites use existing kernel `mef`/`mev`/`split_edge_at`
operators only. No new Euler operator. No bypass of arena topology
invariants. Source: A15 (Euler-operator B-Rep).

**I8 — Reference parity (mandatory per CLAUDE.md commit `4808f2e`).**
Post-fix, `mesh_booleans_inputcheck` reports `valid` (mask `00000` —
all 5 lines `passed`) on all 40 unique cases in PR-S2's
`combined_failures` bucket, both A and B sides. Measured by re-running
`cargo test -p test-harness --test cherchi_inputcheck_corpus_sweep
-- --ignored --nocapture --test-threads=1` and confirming the
`combined_failures` row in the TSV's bucket tally is 0.

I8 is the LOAD-BEARING external check. Internal stage oracles
(I1–I3) measure self-consistency under our own quantization; I8
measures whether Cherchi's reference accepts our output. Per the
strategic-escalation rule, four prior anchors (PR12, PR13, PR-Y14a/b,
PR-Y14c) all produced internally-coherent fixes that reference parity
later invalidated. PR-Y15b cannot ship without I8 holding on the full
40-case set.

## 5. Oracles

| Oracle | What it measures | Where |
|---|---|---|
| **PR-S2 corpus sweep** (existing) | The `combined_failures` bucket count (51 rows / 40 unique cases) drops to 0; the `valid` count rises by 51 (from 295 to 346); other buckets unchanged | `cargo test -p test-harness --test cherchi_inputcheck_corpus_sweep -- --ignored --nocapture --test-threads=1` produces `docs/audits/cherchi_inputcheck_sweep_post_y15b.tsv`; adversary diffs against `docs/audits/cherchi_inputcheck_sweep_2026-05-03.tsv` |
| **PR-S1 reference parity test** (existing) | `cherchi_smoke_two_tetrahedra_union` still passes (`well_formed=true`, χ=2). `f0002_cherchi_union_reference_parity` either completes (Cherchi accepts the post-fix mesh and returns within the 30s timeout — the test SHOULD detect this and assert `well_formed=true` on the union output) OR times out at a strictly later point in the pipeline (the F0002 mesh is now `valid` per inputcheck but the full `mesh_booleans union` still hits a different runaway — acceptable, document) | `cargo test -p test-harness --test cherchi2022_reference_parity -- --ignored --nocapture` |
| **PR-Y14a conformal probe Stage A on F0002** (existing, currently RED) | Post-fix, Stage A on F0002 reports `well_formed=true`, no `(0,0)` self-loop multi_paired entry, canon-0 cluster size = 1 | `crates/test-harness/tests/yang_conformal_probe.rs::f0002_conformal_probe_pinned` (PR-Y15b's test author updates the assertion to the post-fix state) |
| **PR-Y14a conformal probe Stage A on F0004** (existing, currently RED) | Symmetric to F0002 (F0004 ≡ F0002 byte-identical defect class per PR-Y14a §6) | `crates/test-harness/tests/yang_conformal_probe.rs::f0004_conformal_probe_pinned` |
| **Per-side parity check on full 40-case set** | For each of the 40 cases, both A and B sides report `valid` (not just one side) | New harness test `crates/test-harness/tests/pr_y15b_combined_failures_parity.rs` (PR-Y15b's test author writes; iterates the 40 cases and asserts both sides `valid`) |
| **Yang corpus regression guard** | Total Yang pass count does not decrease | `app/tests/cases/assay/results.json` post-PR pass count `≥` pre-PR baseline (currently 9 passed for `YANG_BOOLEAN=1` per `MEMORY.md`). Adversary refreshes `results.json` post-fix and diffs |
| **Architectural integrity check** | No new tolerance constants beyond `QUANT_NANOMETER_SCALE`; no new env var; no new boundary-chaining or tolerance-escalation code | Reviewer checks the diff |
| **Determinism check** | Two consecutive sweep runs produce byte-identical TSVs | `diff` between two `cherchi_inputcheck_corpus_sweep` invocations |

The PR-S2 sweep is the load-bearing oracle for I8. PR-Y14a's
conformal probe is the load-bearing oracle for the F0002-cluster
internal state. Both must pass.

## 6. Failure modes

**6.1 Some `combined_failures` cases migrate to `valid`, others
shift to `non_watertight` or `self_intersecting` (single-axiom).**
The fix partially addresses the failure mask but not all of it.
Acceptable as a partial fix IF the dropped sub-classes from §3 each
have documented rationale. Implementer SHALL list the remaining
single-axiom rows and propose follow-up PR-Y15b.1 (or fold into
PR-Y15c). The "Don't Chase Regressions" memory rule applies — do
NOT add case-specific exemptions to make I8 satisfy on `valid`-only
when some cases are still `non_watertight`.

**6.2 The fix introduces a regression on a previously-`valid` case
(I6 violated).** Catch via the determinism + per-bucket diff in §5.
If `valid` count drops below 295, revert and re-investigate. Likely
cause: the dedup over-collapsed verts that the topology depended on
(see PR-Y14b §6.6 for the analogous failure mode at coplanar-corner
dedup).

**6.3 The fix introduces a regression on the 78% downstream cohort
(F0031–F0040 stripe still fails Waffle).** This is EXPECTED and out
of scope for PR-Y15b. PR-Y15a's investigation phase localizes the
downstream anchor; PR-Y15a-fix addresses it. PR-Y15b's success
criterion is `combined_failures → 0`, NOT corpus-wide Yang-pass.

**6.4 F0005 fails to migrate to `valid` despite I8 satisfaction on
the other 39 cases.** F0005's Stage-A signature (16 unpaired + 153
multi vs F0002's 0 unpaired + 50 multi per PR-Y14b §9) suggests it
may belong to a distinct sub-defect class. Acceptable as a partial
fix; document F0005 in §9 as out-of-scope and refer to a future
PR-Y15b.2 with F0005-specific anchor instrumentation.

**6.5 Side-asymmetric cases — F0064, F0065, F0066, F0071.** These
4 cases are SIDE-ASYMMETRIC: one side lands in `combined_failures`
(e.g., F0064-A holds mask W+I per the TSV), and the other side lands
in a different bucket (`bad_orientation` for F0064/F0071,
`non_watertight` for F0065/F0066). They are therefore IN SCOPE for
PR-Y15b on the `combined_failures` side per §1, AND IN SCOPE on the
other side per I6 (which binds on `(case_id, side)` tuples, not on
`case_id` alone — the fix must address BOTH sides because the bucket
of classification varies per side). The bucket the non-`combined_failures`
side lands in (single-axiom `bad_orientation` or `non_watertight`)
will likely be addressed by the same fix-shape from §3 (a coplanar /
tessellation invariant violation on one side typically propagates an
orientation or watertight defect on the other), but if the second
side's bucket does not migrate to `valid` post-fix, that side becomes
PR-Y15c-eligible. Implementer SHALL run inputcheck on BOTH sides for
all 4 cases and report per-side migration outcomes in the post-fix
TSV diff.

**6.6 R0071 hang (separate defect class).** PR-S2 §5 documents R0071
as a true Waffle kernel hang (gear+revolve at scale 1.86e-4),
NOT a tessellation/coplanar input-axiom violation. R0071 is OUT OF
SCOPE for PR-Y15b; the kernel-hang anchor is a separate investigation.

**6.7 The Cherchi sidecar reports `valid` on a case that still fails
Waffle downstream.** This is the EXPECTED outcome for any case the
fix handles — the case migrates from `combined_failures` to `valid`,
and the Waffle pass/fail status either flips to `Passed` (success
case (a) per §1) OR stays `Failed` with the failure now belonging to
the 78% downstream cohort (success case (b) per §1, handed off to
PR-Y15a). Both are acceptable.

**6.8 PR-Y15b's I8 satisfies but reference oracle invalidates the
fix LATER (the next reference sweep finds a hidden defect)** —
expected per the strategic-escalation rule. The reference oracle
narrows over time; each fix iteration tightens. PR-Y15b's I8
satisfies the spec contract; future PRs may surface new
reference-disagreements that PR-Y15b's anchor was correct in spirit
but missed in detail.

## 7. Research basis

**Yang et al. 2025 [#24] §4.1.1 — Bijective tessellation.** The
paper specifies that tessellation MUST produce a closed watertight
manifold (a "well-formed input mesh" for the downstream Cherchi
arrangement, which in turn requires manifold + watertight +
intersection-free per Cherchi 2022 §3). PR-S2's `combined_failures`
bucket proves Waffle's tessellation violates this on 40 cases.

**Yang et al. 2025 [#24] §4.1.2 — CDT boundary re-triangulation.**
Yang explicitly prescribes Constrained Delaunay Triangulation along
face boundaries to prevent T-junctions when adjacent faces have
different interior subdivision densities. The M+W-only sub-class in
§3 (2 rows) is the canonical T-junction signature — manifold and
watertight both fail because edges have ≥3 incident triangles where
a T-junction met a fan corner.

**Yang et al. 2025 [#24] §4.5.5 — Coplanar preprocessing (Fig. 16).**
The paper prescribes that coplanar overlap regions be replaced with
a SHARED trimmed surface, with IDENTICAL sampling points on A's and
B's boundary. The W+I and W+LO sub-classes in §3 (21 + 6 = 27 rows
combined) are evidence Waffle's coplanar preprocess produces
non-shared samples that cause both leaks (W) and apparent
self-intersection (I) at the overlap rim. PR-Y14b shipped corner-only
dedup; PR-Y15b extends the principle to interior subdivision samples.

**Cherchi et al. 2022 [#38] §3 — Input axioms.** Cherchi's
algorithm assumes input is manifold + watertight + intersection-free
+ well-oriented (LO + GO). The `mesh_booleans_inputcheck` binary
enforces these axioms and is the load-bearing oracle for I8. Cherchi
2022's behavior on input-axiom-violating meshes is undefined (the
PR-S1 6-hour runaway on F0002 demonstrates this).

**Cherchi et al. 2020 [#9] §5 — Well-formed simplicial complex.**
The arrangement output of Cherchi 2020 is guaranteed to be a
well-formed simplicial complex IFF the input is a well-formed
manifold mesh. F0002's pre-Cherchi mesh, with its 8-way
duplicate-corner cluster, violates the input precondition; Cherchi
cannot recover what tessellation/coplanar broke. PR-Y15b restores
the precondition.

**Reference parity (CLAUDE.md commit `4808f2e`).** The Cherchi 2022
C++ sidecar (`mesh_booleans_inputcheck`, built per PR-S1) is the
load-bearing external check (I8). Internal oracles measure
self-consistency under our own quantization; reference parity
measures correctness against the published algorithm. The
strategic-escalation rule from `MEMORY.md/feedback_anchor_before_fix.md`
mandates reference parity for all PRs in the F0002-twin-pairing
class — PR-Y15b inherits this requirement.

### 7a. Analytical vs. approximate method justification

**Method:** Exact. Canonical-key dedup uses integer nanometer
quantization (no floating-point comparison); vertex-equality
decisions are bit-exact. CDT primitives (where used) are the existing
exact-arithmetic kernel CDT.

**Surface-pair coverage:** N/A — this is a B-Rep tessellation +
preprocess fix, not an SSI operation. A15.4 quadric-pair SSI
requirements do not apply.

## 8. Implementation guidance (informational, NOT spec)

The PR-Y15b spec is §1, §2, §3, §4, §5, §6, §7. This §8 is non-binding.

The three fix sites in §3 each follow the same pattern as PR-Y14b's
coplanar dedup:

```rust
// Per-call dedup map at the entry of the relevant outer function.
let mut canon_to_vertex: BTreeMap<[i64; 3], VertexIdx> = BTreeMap::new();

// At each vertex-creation site:
let scale = crate::units::QUANT_NANOMETER_SCALE;
let canon = [
    (pos[0] * scale).round() as i64,
    (pos[1] * scale).round() as i64,
    (pos[2] * scale).round() as i64,
];
if let Some(&v_existing) = canon_to_vertex.get(&canon) {
    counter_deduped.fetch_add(1, Ordering::Relaxed);
    boundary_verts.push(v_existing);
} else {
    let v_new = make_vertex(arena, pos);
    counter_created.fetch_add(1, Ordering::Relaxed);
    canon_to_vertex.insert(canon, v_new);
    boundary_verts.push(v_new);
}
```

Per `feedback_anchor_before_fix.md`, the implementer MUST add eprintln
canaries at each of the three suspected sites in §3 BEFORE writing
production code, run F0002 with `YANG_DUMP_OBJ_BASE` set, and confirm
each canary fires. If a site does not fire on F0002, the actual
F0002-defect anchor lies elsewhere; the implementer SHALL report this
finding and stop coding pending spec amendment.

## 9. Out of scope

- **The 78% downstream cohort.** PR-S2's "interesting cell" of 284
  Waffle=Failed × Cherchi=valid rows is PR-Y15a's territory (Phase-0
  investigation in `specs/yang_pr_y15a_downstream_investigation.md`).
  PR-Y15b's success is INDEPENDENT of the downstream cohort's status.
- **Render-LOD anchor for R0020/R0021.** A separate defect class per
  `MEMORY.md/yang_implementation_status.md`. Out of scope.
- **F0005 if it fails to migrate to `valid`.** F0005's Stage-A
  signature is dissimilar (16 unpaired + 153 multi). Per §6.4,
  F0005's anchor is potentially distinct; defer to PR-Y15b.2.
- **R0071 kernel hang.** A separate defect class (gear+revolve at
  scale 1.86e-4 — PR-S2 §5). Out of scope.
- **The 18 `non_watertight` single-axiom cases** (F0003, F0007–F0010,
  F0053, F0065, F0066, F0071, F0075, R0009, R0030, R0043, R0066 —
  PR-S2 §4). May shift to `valid` as a side effect (welcome) or stay
  (out of scope; PR-Y15c handles).
- **The 2 `self_intersecting` single-axiom cases** (F0063, F0068 —
  PR-S2 §4). Out of scope; PR-Y15c handles.
- **The single-axiom `bad_orientation` SIDES of F0064/F0065/F0066/F0071**
  — these 4 cases are side-asymmetric per §6.5; their
  `combined_failures` side IS in scope, the other side is in scope
  per I6 only if the same fix-shape resolves it. Otherwise PR-Y15c
  handles the residual single-axiom side.
- **Removing the deprecated S-H clipping pipeline** — per A15.6,
  blocked on Yang being operational. Out of scope.

## 10. Verification (PR-Y15b pre-merge)

1. **PR-S2 corpus sweep rerun** post-fix:
   ```
   cargo test -p test-harness --test cherchi_inputcheck_corpus_sweep -- --ignored --nocapture --test-threads=1
   ```
   Output TSV at `docs/audits/cherchi_inputcheck_sweep_post_y15b.tsv`.
   Adversary diffs vs `docs/audits/cherchi_inputcheck_sweep_2026-05-03.tsv`:
   - `combined_failures` count: 51 → 0 (I8 satisfied)
   - `valid` count: 295 → 346 (+51, matching the migration)
   - All other bucket counts unchanged ±0 (I6 satisfied; single-axiom
     migrations to `valid` are welcome side effects)
2. **Per-side parity test** passes:
   ```
   cargo test -p test-harness --test pr_y15b_combined_failures_parity -- --ignored --nocapture
   ```
   Test iterates the 40 cases from PR-S2 §4 and asserts both A and B
   report `Manifold check: passed`, `Watertight check: passed`,
   `Local Orientation check: passed`, `Global Orientation check: passed`,
   `Intersection check: passed`.
3. **PR-Y14a conformal probe Stage A** on F0002 + F0004 reports
   `well_formed=true` (or no `(0,0)` self-loop):
   ```
   cargo test -p test-harness --test yang_conformal_probe -- --ignored --nocapture
   ```
4. **PR-S1 smoke test** still passes:
   ```
   cargo test -p test-harness --test cherchi2022_reference_parity -- cherchi_smoke_two_tetrahedra_union --ignored --nocapture
   ```
5. **F0002 reference-parity test** either completes within Cherchi's
   30s timeout (success) OR times out at a STRICTLY LATER point (the
   inputcheck phase now passes; the runaway moved to a downstream
   stage):
   ```
   cargo test -p test-harness --test cherchi2022_reference_parity -- f0002_cherchi_union_reference_parity --ignored --nocapture
   ```
6. **Yang corpus regression guard:** post-fix `results.json` Yang
   pass count `≥` 9 (current baseline). Adversary refreshes
   `results.json` and commits.
7. **Determinism check:** two consecutive sweep runs produce
   byte-identical TSVs. `diff sweep1.tsv sweep2.tsv` is empty.
8. **`cargo clippy -p kernel --no-deps -- -D warnings`** clean (or
   matches the 92-warning baseline noted in PR-S1's verification).
9. **`cargo fmt --check`** clean.
10. **WASM rebuild** per CLAUDE.md WASM Rebuild Workflow — included
    in the same commit as the Rust changes.
11. **Architectural-integrity reviewer check:** the diff introduces
    no new env vars, no new tolerance constants beyond
    `QUANT_NANOMETER_SCALE`, no new boundary-chaining recovery path,
    no tolerance escalation, no greedy twin-pairing. The change is
    purely additive (per-call `BTreeMap` lookup tables) over existing
    Euler-operator and tessellation paths. (A15.6.)

If verification 1 yields `combined_failures > 0`, the fix is
incomplete; ship as PR-Y15b partial with explicit follow-up PR-Y15b.1
for the residual cases AND document which sub-class masks remain. If
verification 1 yields `valid > 346` (more than the expected +51
migration), some single-axiom cases also migrated — welcome side
effect; document but do not block on it.

The "Never Claim Last Bug" memory rule applies — PR-Y15b does not
promise corpus-wide Yang-pass; it promises the F0002-class
`combined_failures` cohort migrates to `valid` and sets up PR-Y15a
for the dominant downstream defect.
