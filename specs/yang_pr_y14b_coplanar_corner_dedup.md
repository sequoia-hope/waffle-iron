# PR-Y14b — Coplanar-preprocess corner-vertex deduplication

**Status:** SPEC (FIP §3.2 — Phase 1).
**Anchor empirical evidence:** `docs/audits/pr_y14a_conformal_findings.md`.
**Plan reference:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` PR-Y14b.
**Author:** spec author for PR-Y14b. Initial draft by adversary in
Phase 3 of PR-Y14a (per plan: spec writer for PR-Y14b is named after
findings memo lands and references the empirical evidence). The
spec-writer role for PR-Y14b will refine this draft per FIP §3.2.

---

## 1. Goal

Eliminate the multi-vertex canonical cluster pattern that
`split_brep_for_coplanar_pairs` introduces at coplanar-face
boundary corners. Specifically: when two or more coplanar pairs
each call `split_edge_at` to insert a new vertex at a position
that is geometrically identical (within nanometer tolerance) to
an already-inserted vertex, reuse the existing vertex instead of
creating a new one. The user-visible effect is that F0002 and
F0004 — both byte-identical clean reproducers of this pattern —
either:

- (a) pass under `YANG_BOOLEAN=1`, OR
- (b) fail at a strictly later pipeline stage with the conformal
  probe reporting `well_formed=true` at the previously-broken
  Stage A.

Either outcome satisfies the goal. An "unprincipled fix that
masks the symptom" (P9) is explicitly forbidden — see §6 below.

## 2. Parameters

This is a bug fix; no new user-facing parameters are introduced.

**Implementation parameters:**

| Parameter | Default | Units | Range | Source |
|---|---|---|---|---|
| Canonical-quantize scale | `crate::units::QUANT_NANOMETER_SCALE` (= `1e9`) | per-meter | fixed constant | reuse — same as `topology_extract.rs:375-393` and `oracles/conformal_mesh.rs::check_conformal` |
| Per-call canonical-key map | empty `BTreeMap<[i64;3], VertexIdx>` per `split_brep_for_coplanar_pairs` invocation | n/a | n/a | new (see §3) |

No `Cargo.toml` changes. No new public API. No new env vars (the
fix is unconditional once landed; it does NOT add a feature flag).

## 3. Branch Table

The fix introduces ONE new branch into
`split_brep_for_coplanar_pairs`'s overlap-vertex-insertion loop
(currently around `coplanar_preprocess.rs:515-526`). All other
branches in the function remain unchanged.

| Branch | Condition | Behavior | Telemetry counter |
|---|---|---|---|
| **A — found on existing vertex** (existing) | `ov` matches an existing B-Rep vertex within tolerance via `vertex_existing` path | reuse the existing `VertexIdx` | `COPLANAR_VERTS_SNAPPED_EXISTING.fetch_add(1)` |
| **B — found on existing edge interior, NEW canonical key** (new sub-branch within existing B) | `ov` lies inside an edge AND its canonical key has never been inserted before this call | call `split_edge_at(arena, edge_idx, ov)`; record canonical key → returned `VertexIdx` in the per-call dedup map | `COPLANAR_VERTS_VIA_SPLIT_EDGE.fetch_add(1)` (existing) |
| **C — found on existing edge interior, REPEAT canonical key** (NEW) | `ov` lies inside an edge AND its canonical key was already inserted earlier in the same `split_brep_for_coplanar_pairs` call | reuse the previously-inserted `VertexIdx`; do NOT call `split_edge_at` | new counter `COPLANAR_VERTS_DEDUPED_BY_CANON_KEY.fetch_add(1)` |
| **D — not found on any edge** (existing) | `ov` does not lie on any boundary edge within tolerance | drop with telemetry, log warning if `#[cfg(test)]` | `COPLANAR_VERTS_DROPPED.fetch_add(1)` (existing) |

The new branch C is INSERTED before branch B's `split_edge_at` call,
not as a parallel match arm — so the existing branch B logic is
unchanged for first-time canonical keys. The dedup map is local to
each `split_brep_for_coplanar_pairs` invocation; there is no global
state.

## 4. Invariants

These statements MUST hold post-fix and are individually
measurable:

**I1 — Canonical-key uniqueness in coplanar split-edge output.**
After `split_brep_for_coplanar_pairs` returns, no two distinct
`VertexIdx` values in the modified `arena` have the same nanometer
canonical key, **for vertices added via the
`split_brep_for_coplanar_pairs` path during this call**. (Existing
B-Rep vertices that pre-date the call are not in scope; they are
the input.)

**I2 — Sub-picometer drift is eliminated.** For any two i_overlay
overlap-vertex positions `ov_1`, `ov_2` from two different
coplanar pairs in the same call, if `quant(ov_1) == quant(ov_2)`,
then exactly one `split_edge_at` call is made for the pair, not
two.

**I3 — F0002 Stage A canon-0 cluster size = 1, not 8.** Re-running
the Phase-3 dump instrumentation (`YANG_CONFORMAL_DUMP_CANON0=1`)
on F0002 must report exactly one raw vertex mapping to canonical-0
post-fix.

**I4 — Conformal-probe Stage A `well_formed=true` for F0002 and
F0004.** Or, weaker: Stage A's `multi_paired_edges` count has
NO entry with `(v0=0, v1=0)` self-loop (the Phase-3 dominant
violation is gone). Either condition is the empirically-measurable
fix-correctness statement.

**I5 — No new `unpaired_directed_edges` introduced at Stage A.**
The dedup must not accidentally create boundary holes (e.g. by
removing a split that the topology depended on). If `unpaired`
count rises from 0 to >0 at Stage A on F0002/F0004, the fix is
wrong and must be reverted. This is the P9 "fix it right or don't
fix it" guard.

**I6 — Architectural integrity.** The fix MUST NOT add a new
boundary-chaining recovery path or tolerance escalation
(`A15.6`-banned techniques). It MUST be additive (a deduplication
lookup table) over the existing Euler-operator code path.

**I7 — Determinism preserved.** Two consecutive runs with
identical input produce byte-identical
`subdivided.verts`/`subdivided.tris_a`/`subdivided.tris_b` output.
The dedup is on canonical keys (deterministic) using a `BTreeMap`
(deterministic iteration order), not a `HashMap`.

## 5. Oracles

| Oracle | What it measures | Where |
|---|---|---|
| **`check_conformal` Stage A** (live) | Stage A `is_well_formed` field, `multi_paired_edges.len()`, presence of `(v0=0, v1=0)` self-loop | `crates/test-harness/tests/yang_conformal_probe.rs::f0002_conformal_probe_pinned` (must be UPDATED to assert post-fix state) |
| **`check_conformal` Stage A** (live, F0004) | Same as above for F0004 | `crates/test-harness/tests/yang_conformal_probe.rs::f0004_conformal_probe_pinned` (must be UPDATED) |
| **F0002 corpus pass-or-late-fail** | F0002 either passes OR fails at a stage strictly later than Stage A | `cargo test -p test-harness --test assay_randomized -- yang_fast --ignored` — F0002 status changes from `auto-union-failed` to either `Passed` OR a different failure mode |
| **Yang corpus regression guard** | Total Yang pass count does not decrease | `app/tests/cases/assay/results.json` post-PR pass count `≥` pre-PR pass count (currently 9/190 passed for `YANG_BOOLEAN=1`) |
| **Coplanar telemetry counter** (new) | `COPLANAR_VERTS_DEDUPED_BY_CANON_KEY` counter is non-zero on F0002 | New `[coplanar-tele]` line emitted at end of `split_brep_for_coplanar_pairs`; pin via test in PR-Y14b's test phase |
| **Determinism check** | Two runs of F0002 produce identical conformal-probe output lines | Add a determinism test in `yang_conformal_probe_diagnostics.rs` (adversary-owned file) that runs F0002 twice and compares stderr |
| **Architectural integrity check** | No new tolerance constants introduced beyond `QUANT_NANOMETER_SCALE`; no new env var; no new boundary-chaining or tolerance-escalation code | Reviewer checks the diff |

## 6. Failure Modes

**6.1 Invalid inputs.** Same as the existing
`split_brep_for_coplanar_pairs` behavior — empty polygons, zero-area
overlaps, etc., are already handled. The dedup adds no new failure
modes for these.

**6.2 Degenerate geometry — `ov` lies exactly on an existing
B-Rep vertex (not just near it).** Already handled by branch A
(`COPLANAR_VERTS_SNAPPED_EXISTING`). Dedup branch C does not
interact with this; both branches A and C produce the same
behavior (reuse existing vertex).

**6.3 Two pairs whose `ov` positions differ by MORE than nanometer
but represent the same logical corner.** This is OUT OF SCOPE for
PR-Y14b. If observed (probe reports `well_formed=false` with a
multi-vertex cluster spread over multiple canonical keys),
investigate as a separate PR — likely indicates a coarser
coplanar-detection tolerance issue upstream of
`split_brep_for_coplanar_pairs`.

**6.4 The fix introduces a regression on a different corpus
case.** The Yang corpus regression guard (oracle in §5) catches
this. If pass count drops, revert the PR and re-investigate. The
"Don't Chase Regressions" memory rule applies: do NOT add
case-specific exemptions to make the regression test pass without
fixing the root cause.

**6.5 Stage A becomes well-formed but Stage B / Stage C reveal a
new defect.** This is the EXPECTED outcome category (b) from §1.
Acceptable. The conformal probe surfaces the next anchor; PR-Y14c
or beyond addresses it. PR-Y14b's spec must explicitly document
that this is acceptable in §1's goal statement (already done).

**6.6 The (a)/(b)-style dump shows the canon-0 cluster is reduced
but NOT to 1 (e.g. 8 → 3).** Indicates the fix is incomplete:
some `ov` computation path bypasses the dedup. Likely cause: a
second `split_edge_at` call site outside the audited loop. Search
`crates/kernel/src/boolean/coplanar_preprocess.rs` for all
`split_edge_at` callers and apply the same dedup.

## 7. Research Basis

**Yang et al. 2025 §4.5.5 (coplanar preprocessing)** — the paper
prescribes that the overlap region's boundary be a *shared trimmed
surface* with *identical sampling points* on both A's and B's
sides (Fig. 16 caption verbatim:
*"The common part and the other two parts share identical sampling
points on their boundaries."*). The Phase-3 finding shows our
implementation violates this: each pair produces its own
sampling-point copies. PR-Y14b's dedup is the operational
realization of "identical sampling points" at the kernel level.

**Cherchi et al. 2020 §5 (mesh arrangement guarantees)** — the
arrangement-output guarantee of *"well-formed simplicial complex"*
(every directed edge has exactly one reverse counterpart) is
predicated on the input being a manifold-watertight mesh per
Cherchi 2022 §2.3. F0002's pre-Cherchi mesh, with 8-way duplicate
corner verts, violates this precondition — Cherchi cannot recover
what coplanar preprocess broke. PR-Y14b restores the precondition.

**Reference parity (CLAUDE.md commit 4808f2e):** the fix should
be diff-tested against the Cherchi 2022 C++ sidecar
(`docs/audits/cherchi2022_sidecar_feasibility.md` — verdict GO,
disk-space caveat). The expected post-fix observation is:
Cherchi's reference implementation, fed the same post-coplanar-
preprocess mesh, produces a `well_formed=true` arrangement output.
Building the sidecar is the responsibility of the separate
PR-Y14c (or whatever PR follows the disk-space resolution); it is
NOT a blocker for PR-Y14b — PR-Y14b's correctness is verifiable
via the internal conformal probe oracle alone.

**No deviation from published technique.** The dedup logic is
canonical-quantize equality — a standard mesh-processing
preprocessing step. Yang 2025 implicitly assumes such canonicalization
when it speaks of "identical sampling points."

### 7a. Analytical vs. Approximate Method Justification

**Method:** Exact. The fix uses integer canonical keys (no
floating-point comparison), so vertex-equality decisions are
bit-exact. No tolerance widening is introduced.

**Surface-pair coverage:** N/A — this fix is a B-Rep topology
operation (vertex deduplication), not a surface-surface
intersection. A15 quadric-pair SSI requirements do not apply.

---

## 8. Implementation Sketch (informational, NOT spec)

The spec writer for PR-Y14b is the authoritative source for §1–§7.
This §8 is a non-binding sketch, retained for the implementer's
context. Do not interpret §8 as constraining the fix's structure.

```rust
// In split_brep_for_coplanar_pairs, before the `for (pair_idx, pair)
// in coplanar_pairs.iter_mut()` loop:
let mut canon_to_vertex: BTreeMap<[i64; 3], VertexIdx> = BTreeMap::new();

// In the inner overlap-vertex insertion loop, replace:
//     let v_new = split_edge_at(arena, edge_idx, ov);
//     COPLANAR_VERTS_VIA_SPLIT_EDGE.fetch_add(1, Ordering::Relaxed);
//     boundary_verts.push(v_new);
// with:
let scale = crate::units::QUANT_NANOMETER_SCALE;
let canon = [
    (ov[0] * scale).round() as i64,
    (ov[1] * scale).round() as i64,
    (ov[2] * scale).round() as i64,
];
if let Some(&v_existing) = canon_to_vertex.get(&canon) {
    COPLANAR_VERTS_DEDUPED_BY_CANON_KEY.fetch_add(1, Ordering::Relaxed);
    boundary_verts.push(v_existing);
} else {
    let v_new = split_edge_at(arena, edge_idx, ov);
    COPLANAR_VERTS_VIA_SPLIT_EDGE.fetch_add(1, Ordering::Relaxed);
    canon_to_vertex.insert(canon, v_new);
    boundary_verts.push(v_new);
}
```

(Adapt to the surrounding scope — `boundary_verts` and the
`#[cfg(test)]` eprintlns are existing; the `mef_ok` step downstream
of this loop already operates on `boundary_verts` and is
unaffected.)

The new `COPLANAR_VERTS_DEDUPED_BY_CANON_KEY` counter must be
declared alongside the existing
`COPLANAR_VERTS_VIA_SPLIT_EDGE` constant
(coplanar_preprocess.rs:~30 area), and emitted in the existing
`[coplanar-tele]` line at function exit (~line 395) for telemetry
parity.

The dedup is **per-call**, not global. Two consecutive
`split_brep_for_coplanar_pairs` invocations on different
`(solid_a, solid_b)` pairs each maintain their own `canon_to_vertex`
map.

---

## 9. Out of Scope

- Building the Cherchi 2022 C++ sidecar (separate PR; see
  `docs/audits/cherchi2022_sidecar_feasibility.md` and disk-space
  caveat).
- Touching `flood_fill_patches`, `topology_extract`,
  `subdivide_mesh_pair_full_cherchi`, or any Yang-pipeline
  downstream stage. The Phase-3 evidence rules these out as
  F0002/F0004 anchors.
- Investigating F0005's distinct failure mode. F0005 is NOT a
  coplanar-corner cluster case; its conformal-probe signature is
  dissimilar (16 unpaired + 153 multi at Stage A vs F0002's 0
  unpaired + 50 multi). PR-Y14c or later addresses F0005 with its
  own anchor evidence.
- Re-investigating the previously-claimed PR14 Render-LOD
  per-face byte-identity defect from
  `MEMORY.md/yang_implementation_status.md`. Phase-3 supersedes
  that anchor for F0002/F0004 specifically (see findings memo
  §5). The Render LOD anchor may still be the right anchor for
  F0005 or other cases; that is a separate investigation.
- Removing the deprecated S-H clipping pipeline (per A15.6,
  blocked on Yang being operational).

---

## 10. Verification (PR-Y14b pre-merge)

1. **PR-Y14a probe tests still pass** with probe off:
   `cargo test -p test-harness --test assay_randomized -- yang_fast --ignored`
   produces zero `[conformal-probe]` lines.
2. **F0002 + F0004 probe tests UPDATED** to assert post-fix Stage A
   state: `f0002_conformal_probe_pinned` and
   `f0004_conformal_probe_pinned` in
   `crates/test-harness/tests/yang_conformal_probe.rs` MUST be
   updated by PR-Y14b's test author to assert
   `well_formed=true` at Stage A (or the absence of the (0,0)
   self-loop) — the test is the live regression guard.
3. **F0002 + F0004 either pass OR fail-strictly-later** in the
   corpus. `app/tests/cases/assay/results.json` shows F0002/F0004
   either with `status=Passed` or with a NEW failure mode whose
   first conformal-probe break is at Stage B or later.
4. **Yang corpus pass count `≥`** pre-PR baseline (currently 9 in
   `results.json`). Adversary refreshes `results.json` and commits.
5. **`cargo clippy -p kernel --no-deps -- -D warnings`** clean.
6. **`cargo fmt --check`** clean.
7. **WASM rebuild** per CLAUDE.md WASM Rebuild Workflow — included
   in the same commit as the Rust changes.
8. **Architectural-integrity reviewer check:** the diff introduces
   no new env vars, no new tolerance constants, no new boundary-
   chaining recovery path, and no tolerance escalation. The change
   is purely additive (a per-call `BTreeMap` lookup table) over
   existing Euler-operator calls.

If verification 3 yields a NEW failure mode, the conformal probe
will name the new anchor; PR-Y14c is then specified at that
anchor. The "Never Claim Last Bug" memory rule applies — PR-Y14b
does not promise corpus-wide pass; it promises the F0002/F0004
Stage-A defect is gone and a regression-free corpus.
