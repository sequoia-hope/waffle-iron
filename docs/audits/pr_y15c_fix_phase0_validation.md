# PR-Y15c-fix Phase 0 (v2) — Stage F multi-probe validation

**Author:** adversary-5 (rotated per `feedback_oracle_credibility_via_role_separation.md` — NOT adversary-3)
**Date:** 2026-05-04
**Spec:** `specs/yang_pr_y15c_fix_phase0_stage_f_repair.md`
**Diagnostic under review:** `docs/audits/pr_y15c_fix_phase0_diagnostic.md`
**Probe family:** Stage F (5 sub-stages, gated on `YANG_CONFORMAL_PROBE=1`,
tagged `[stage-f]`) at `crates/kernel/src/tessellation/mod.rs:4274-4373`.

## Verdict

**ACCEPT** — implementer-h's three-track anchor finding is empirically
airtight. Independent re-run reproduces all 10 case rows byte-for-byte;
the +28 Steiner-fan inflation is independently verified by inserting
exploratory F.1a / F.1b probes around the unprobed flip + Steiner stages
(then reverted); the mutation test confirms the [stage-f] probes are
load-bearing on row attribution; reconciliation arithmetic (pre-F.0 +
F.0→F.4 = PR-Y15c Stage E delta) holds for all 10 cases. The cohort
genuinely splits — three PRs (PR-Y15c-fix-1, PR-Y15c-fix-3,
PR-Y15c-fix-Phase0-v3) are the correct decomposition, not one.

## §1. Decision-tree verdict per case (F0031–F0040)

`YANG_CONFORMAL_PROBE=1 YANG_BOOLEAN=1 cargo test … batch_enclosed_subtract_fix
--ignored --nocapture --test-threads=1` emitted 100 [stage-f] lines
(20 calls × 5 sub-stages — matches implementer-h's "20-not-10"
canary observation). Result-mesh per-case attribution:

| Case | F.0 | F.1 | F.2 | F.3 | F.4 | Final unp | Dropper | Row |
|---|---:|---:|---:|---:|---:|---:|---|---:|
| F0031 | 40 | 36 | 36 | 36 | 36 | 12 | F.0→F.1 (−4)  | **1** |
| F0032 | 36 | 24 | 24 | 24 | 24 | 16 | F.0→F.1 (−12) | **1** |
| F0033 | 36 | 24 | 24 | 24 | 24 | 16 | F.0→F.1 (−12) | **1** |
| F0034 | 44 | 32 | 32 | 32 | 32 | 28 | F.0→F.1 (−12) | **1** |
| F0035 | 36 | 24 | 24 | 24 | 24 | 16 | F.0→F.1 (−12) | **1** |
| F0036 | 76 | 56 | 84 | 36 | 36 | 16 | F.2→F.3 (−48) | **3** |
| F0037 | 76 | 56 | 84 | 40 | 40 | 12 | F.2→F.3 (−44) | **3** |
| F0038 | 76 | 56 | 84 | 40 | 40 | 20 | F.2→F.3 (−44) | **3** |
| F0039 | 68 | 44 | 44 | 44 | 44 | 40 | F.0→F.1 (−24) | **1** |
| F0040 | 76 | 56 | 84 | 40 | 40 | 20 | F.2→F.3 (−44) | **3** |

**Byte-for-byte match against implementer-h's §"Cluster homogeneity"
table.** Row 1 fires on 6/10 (F0031–F0035, F0039), row 3 on 4/10
(F0036–F0038, F0040), row 5 concurrently on 10/10 via pre-F.0 −8 tri
loss (§5). Three-track attribution stands.

## §2. Cluster homogeneity expansion + sub-cluster verification

Sub-cluster split is REAL (not measurement artifact):

- **Sub-cluster A (6/10):** F0031–F0035, F0039 — F.0→F.1 step-down, then flat.
- **Sub-cluster B (4/10):** F0036–F0038, F0040 — F.0→F.1 down, F.1→F.2 +28 Steiner inflation, F.2→F.3 −44 to −48.

**F0039 anomaly is REAL.** Per `app/tests/cases/assay/*.meta.json`:

```
F0031–F0035: extrude(rectangle,boss)+extrude(circle,cut) — Box-minus-cyl
F0036–F0040: extrude(circle,boss)+extrude(rectangle,cut) — Cyl-minus-box  ← all five
```

Implementer-h's "operand-order split" framing is misleading: F0036–F0040
are **all five** Cyl-minus-box per metadata, yet F0039 sits in
sub-cluster A. The actual splitting axis is **Steiner-fan eligibility**,
which depends on whether faces have non-manifold interior diagonals
after edge-flip. F0039 (F.0=68, intermediate vs sub-cluster B's uniform
F.0=76) does not trigger Steiner-fan, so F.1→F.2 is a no-op (F.1=44,
F.2=44 byte-identical) and the row-1 attribution holds. F0039's row-1
attribution is genuine, not an artifact.

Operand-mesh blocks read `tri_count=12 unpaired=0` flat across F.0–F.4
on all 10 cases (clean small-box operands). Result-mesh blocks carry
the non-trivial deltas. Attribution unambiguous via tri_count signature,
matching implementer-h's §"Spec ambiguity #2".

## §3. Mutation test — Stage F probe IS load-bearing

**Mutation:** Replaced `let tri_count = indices.len() / 3;` with
`let tri_count = 0usize;` at the F.2 probe site
(`tessellation/mod.rs:4344`), forcing every F.2 emission to report
`tri_count=0`.

**Result with mutation applied** (F0036 result-mesh, sub-cluster B):

```
[stage-f] sub=0 tri_count=76 unpaired=20
[stage-f] sub=1 tri_count=56 unpaired=52
[stage-f] sub=2 tri_count=0  unpaired=36   ← FORCED (real value 84)
[stage-f] sub=3 tri_count=40 unpaired=20
[stage-f] sub=4 tri_count=40 unpaired=20
```

`unpaired` continues to compute correctly (separating the mutated
field). **Diagnostic interpretation flips entirely:** decision-tree row
2 (F.1→F.2 drop ≥12) now spuriously fires (56→0 = −56 drop). Row 3
(F.2→F.3 drop ≥12) NO LONGER fires (F.2=0→F.3=40 is an INCREASE).
Implementer-h's row-3 attribution to
`remove_nonmanifold_duplicates_aggressive` would be invisible; would
mis-route to `remove_nonmanifold_topology_aware` — the WRONG anchor
(empirically a no-op on this cohort per §6). **Probe family is
load-bearing on row attribution; diagnostic depends on actual
measurement.** Mutation reverted, see §8.

## §4. Alternative-probe-site refutation

Per spec §6 (LOAD-BEARING per the v1 lesson): I MUST run any
cheaper-proxy idea myself BEFORE recommending it. Three candidates:

### §4.1 Probes inside the 4 removal functions — properly OUT OF SCOPE for Phase 0

Per-removal-decision logging belongs in PR-Y15c-fix-1 / fix-3 fix code
(WHY each tri is removed), not Phase 0 row attribution (which the
existing 5-probe family already delivers cleanly).

### §4.2 Probes between flip + Steiner stages — RAN IT; CONFIRMS Steiner-fan inflation

Implementer-h's §"Spec ambiguity #1" attributes the +28 inflation to
`retessellate_nonmanifold_faces_with_steiner_fan` from the docstring,
not direct measurement. **I ran it myself** by inserting exploratory
probes F.1a (after flip, `tessellation/mod.rs:4313`) and F.1b (after
Steiner-fan, `:4329`), rebuilt, re-ran F0031–F0040.

Sub-cluster B (F0036, F0040 identical pattern):

```
[stage-f] sub=1  tri_count=56   ← after dedup
[stage-f] sub=1a tri_count=56   ← after flip      (no-op)
[stage-f] sub=1b tri_count=84   ← after Steiner   (+28)
[stage-f] sub=2  tri_count=84   ← after topo-aware (no-op)
```

**Steiner-fan inflation independently confirmed.** Both `flip_*` and
`remove_nonmanifold_topology_aware` are no-ops on tri_count for this
cohort. The +28 spike is exclusively Steiner-fan; the −44 to −48 drop
at F.3 is exclusively the aggressive removal. Reverted exploratory
probes; final diff byte-clean (§8). The 14-LOC F.1a/F.1b pattern is
documented here for any v3 spec wanting to subdivide F.1→F.2 further.

### §4.3 Probes inside per-face dispatch helpers — DEFERRED to v3 (correctly out of scope)

Per spec §"Out of scope" + diagnostic §"Reconciliation outcome",
pre-F.0 −8 investigation routes to PR-Y15c-fix-Phase0-v3.

## §5. Reconciliation independent confirmation

Per spec §8 deliverable 3: tri_drop summed across F.0 → F.4 MUST
match Stage E delta from PR-Y15c. Computed independently from my probe
run + PR-Y15c phase 0 diagnostic Stage C tri counts:

| Case | C tris | F.0 | Pre-F.0 Δ | F.4 | F.0→F.4 Δ | Total | E Δ | OK |
|---|---:|---:|---:|---:|---:|---:|---:|:---:|
| F0031 | 48 | 40 | **−8** | 36 | −4  | −12 | −12 | ✓ |
| F0032 | 44 | 36 | **−8** | 24 | −12 | −20 | −20 | ✓ |
| F0033 | 44 | 36 | **−8** | 24 | −12 | −20 | −20 | ✓ |
| F0034 | 52 | 44 | **−8** | 32 | −12 | −20 | −20 | ✓ |
| F0035 | 44 | 36 | **−8** | 24 | −12 | −20 | −20 | ✓ |
| F0036 | 84 | 76 | **−8** | 36 | −40 | −48 | −48 | ✓ |
| F0037 | 84 | 76 | **−8** | 40 | −36 | −44 | −44 | ✓ |
| F0038 | 84 | 76 | **−8** | 40 | −36 | −44 | −44 | ✓ |
| F0039 | 76 | 68 | **−8** | 44 | −24 | −32 | −32 | ✓ |
| F0040 | 84 | 76 | **−8** | 40 | −36 | −44 | −44 | ✓ |

**Pre-F.0 loss is uniformly −8 tris per case, all 10 cases.** Sum
reconciles to PR-Y15c Stage E delta exactly. Implementer-h's
reconciliation table independently confirmed; row 5 (pre-F.0 loss)
genuinely fires concurrently with rows 1 / 3.

The −8 uniformity (across operand-order, cohort sizes 36→76, both
sub-clusters) indicates a per-call constant — likely a single face or
edge in `tessellate_cylindrical_face_bounded` / `tessellate_planar_face_bounded`
/ `discretize_edges`. Implementer-h's PR-Y15c-fix-Phase0-v3 scope is
arithmetically correct.

## §6. Steiner-fan inflation verification (the +28 spike claim)

Implementer-h's claim was inferred from the docstring, not measured.
**Confirmed via §4.2 exploratory probes:**

| Case | F.1 | F.1a (after flip) | F.1b (after Steiner) | F.2 |
|---|---:|---:|---:|---:|
| F0036 | 56 | 56 | 84 (+28) | 84 |
| F0040 | 56 | 56 | 84 (+28) | 84 |

**Steiner-fan attribution is REAL.** Side-narrowing:
`remove_nonmanifold_topology_aware` is empirically a no-op on tri_count
for the F0031–F0040 cohort. **Decision-tree row 2 is empirically
refuted for this cohort** — PR-Y15c-fix-2 (row 2's anchor) should NOT
be spec'd off this Phase 0.

## §7. Verification deltas vs implementer-h

Three minor deltas, none load-bearing:

1. **Operand-order framing in §"Sub-cluster A" is incomplete.**
   F0036–F0040 are ALL five Cyl-minus-box per metadata; F0039 is not
   the lone exception. The split axis is Steiner-fan eligibility
   (driven by F.0 complexity), not operand order. Row attribution is
   correct; only the rationale framing is misleading.

2. **Clippy baseline drift.** Implementer-h reports 91 warnings; my
   measurement of MAIN baseline (via `git stash` round-trip) = **92**.
   With probes applied: 92. Net delta = 0 either way; matches
   PR-Y15c phase 0 diagnostic §"Spec ambiguity #5". Stable drift since
   9a2406c, not implementer-h's defect.

3. **All other implementer-h data points match byte-for-byte:**
   per-case F.0–F.4 (10/10), Steiner-fan attribution (independently
   confirmed §6), reconciliation (independently confirmed §5),
   0 [stage-f] lines emitted with probe gate unset, 0 stage-f-canary
   residue.

## §8. Working-tree state

- **Mutation reverted** (F.2 `tri_count = 0usize` → `indices.len() / 3`).
- **Exploratory F.1a / F.1b probes reverted.**
- `git diff crates/kernel/src/tessellation/mod.rs --stat`:
  `35 insertions, 0 deletions` — matches implementer-h's commit
  byte-for-byte (5 stage-f probe blocks of 7 LOC each at L4274,
  L4292, L4341, L4353, L4368).
- `app/tests/cases/assay/results.json`: timestamp refresh from probe-on
  rerun; pass/fail unchanged at 11/179.
- `cargo clippy -p kernel --no-deps`: **92 warnings** (matches MAIN);
  net delta from implementer-h edits = 0.
- `rustfmt --check crates/kernel/src/tessellation/mod.rs`: clean (exit=0).
- Probe-off F0002 trace: `cargo test … yang_trace_f0002 --ignored
  --nocapture` → 1 passed; 0 [stage-f] lines emitted; 0 stage-f-canary
  residue.

## Verdict summary

**ACCEPT — proceed to PR-Y15c-fix-N (3 PRs).**

- 10/10 cases independently re-run; per-case F.0–F.4 table matches
  implementer-h byte-for-byte.
- Cluster split (6/10 row 1, 4/10 row 3, 10/10 row 5) is REAL; F0039
  anomaly genuinely sub-cluster A (Steiner-fan ineligible).
- Steiner-fan +28 inflation independently verified (exploratory F.1a/F.1b
  probes ran then reverted).
- Mutation test confirms diagnostic depends on measurement, not assertion.
- Reconciliation arithmetic holds for all 10 cases.
- Diff byte-clean; mutation + exploratory probes reverted.
- Side-narrowing: `remove_nonmanifold_topology_aware` is a no-op
  on tri_count for this cohort (row-2 anchor refuted).

**Recommendation for PR-Y15c-fix-N scope: THREE PRs, not one.**

1. **PR-Y15c-fix-1** (highest value — 6/10): scope to `repair.rs:502-574`
   `remove_winding_insensitive_duplicates`. Investigate why dedup keys
   treat 4–24 legitimate triangles as duplicates per case.
2. **PR-Y15c-fix-3** (4/10): scope to `repair.rs:1870-2154`
   `remove_nonmanifold_duplicates_aggressive`. Steiner-fan-vs-aggressive
   fight; either constrain aggressive or fix Steiner-fan output.
3. **PR-Y15c-fix-Phase0-v3** (10/10, constant −8 tris each): spec
   per-face dispatch probes inside `tessellate_cylindrical_face_bounded`
   / `tessellate_planar_face_bounded` / `discretize_edges`.
   Wrong-anchor count #2 territory per spec §5 row 5.

PRs 1 and 3 can run concurrently (different files, disjoint sub-cohorts).
Phase0-v3 sequential follow-up. A15.6 cross-domain coordination required
for all three (still inside `tessellation::`).

**Wrong-anchor count for PR-Y15c-fix arc:** still 1 of 3 (weld site
refuted at v1). v2 produced an empirically-pinned answer rather than a
refutation, so does NOT count against the strategic-escalation budget.
