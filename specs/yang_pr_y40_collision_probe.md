# Yang PR-Y40 — Canonical-key collision probe at `remove_winding_insensitive_duplicates`

**Verdict in header: SHIP-INFRA + 6th-refutation framing.**

| Field | Value |
|---|---|
| Authors | team-lead, canary-y40, spec-y40 |
| Date | 2026-05-13 |
| Parent commit | `2752016` |
| Class | INFRASTRUCTURE-CLASS (env-gated probe; 0 LOC production logic) |
| Probe LOC | ~151 LOC additive in `crates/kernel/src/tessellation/repair.rs` |
| Target site | `remove_winding_insensitive_duplicates` (`repair.rs:502-616`), F.0 → F.1 stage transition |
| Verdict | **SHIP-INFRA + 6th-refutation framing** — PR-Y39 §2.5 attribution chain (16 D.1d tris lost at F.0→F.1) is empirically refuted; measured count is 4, not 16 |
| Cycle position | 9th PR cycle on F0020 Render LOD; 5 ABORTs (Y25/Y26/Y27/Y28/Y39) + 4 INFRA-SHIPs (Y36/Y37/Y38/Y40) |

---

## §1 Context

PR-Y39 (commit `2752016`) ABORTed at the canary phase. The attempted production fix at `remove_nonmanifold_topology_aware` (F.1 → F.2, the PR-Y28 §1 banked anchor for D.1d kids 218/232/233) did not fire on the load-bearing F0020 invocation: every NMM edge had `extra=[]`, the kids' triangles were already gone by entry to that function, and the F0020 unpaired count moved 40 → 40, not 40 → ~32 as the plan predicted.

The PR-Y39 canary memo §2.5 / §7 banked a new claim from a different probe: stage-f progression on F0020 inv006 shows 138 → 119 (19-tri drop) at F.0 → F.1, and Y36 inverse-probe `indices_emitted_dispatch` for D.1d kids 218/232/233 is 3+6+9 = 18. PR-Y39 §2.5 interpreted these 18 indices as 18 *triangles* and concluded "16 of the 19 F.0→F.1 drops are kids 218/232/233 colliding with earlier-emitted triangles from other kids." It banked PR-Y40 to instrument the actual function (`remove_winding_insensitive_duplicates`, the canonical-key dedup at `repair.rs:502-616`) and answer "WHICH kids win the 16 collisions" so a PR-Y41 production fix could pick between three candidate shapes (source-attribution policy, insert-order awareness, upstream dispatch-loop fix).

PR-Y40 added the requested probe in worktree, ran it on F0020 and the F0044/F0045/R0045/R0092 cohort, and produced a direct measurement at the empirically-correct anchor.

**The measurement refutes PR-Y39 §2.5's attribution chain at the level of basic accounting:** triangles ≠ indices. D.1d kids 218/232/233 emit 3+6+9 = 18 *indices* at dispatch, which is 1+2+3 = **6 triangles**, of which **4 lose** at F.0 → F.1 and 2 survive. The "16 D.1d-tri" frame was off by a factor of ~3-4× and cannot ground a PR-Y41 production fix.

PR-Y40 ships the probe as durable infrastructure (default-off byte-identical) and explicitly revises the PR-Y41 premise.

## §2 Why infra-class

The strategic escalation rule from `feedback_anchor_before_fix` ("3 wrong anchors in a row → stop bisecting, build a reference comparison") was triggered by PR-Y25/Y26/Y27 in early May and produced PR-Y29 (Cherchi C++ differential-diff harness). Since then, D.1-related work on F0020 Render LOD has continued under a discipline of empirical-anchor-before-production-commit:

- **PR-Y25 / Y26 / Y27 / Y28 / Y39** — five consecutive canary-stage ABORTs on D.1 fix candidates. Each cycle eliminated a candidate (Yang §4.4.1 mesh-updating, patch-dropout, dispatch loop sub-mechanisms, partition-and-remove Shape A/B/C/D).
- **PR-Y36 / Y37 / Y38** — three INFRA-only SHIPs adding source-face attribution probe (Y36 inverse), H1/H2/H3 classification (Y37), and grid-sensitivity gate (Y38) at sites *downstream* of `remove_winding_insensitive_duplicates`.
- **PR-Y40** — fourth INFRA-only SHIP. The probe at the F.0 → F.1 site that PR-Y39 banked as the actual drop location.

Each cycle eliminated a specific wrong attribution. PR-Y39 refuted the F.1→F.2 anchor; PR-Y40 refutes PR-Y39 §2.5's specific tri-count attribution at F.0 → F.1. The pattern is asymptotic: every probe narrows the search space and produces durable measurement scaffolding, while the 0-LOC production discipline (`feedback_anchor_before_fix`) prevents wasted production cycles on inferred-but-unmeasured fix shapes.

This is the disciplined response to a 9-cycle search with 5 wrong-anchor ABORTs: continue infra investment at the empirically-correct site until the load-bearing mechanism is observed directly, not inferred.

## §3 Probe design

The probe is contained within `crates/kernel/src/tessellation/repair.rs`. Production logic — `seen: HashSet<[QPos; 3]>` and the `if seen.insert(key) { keep } else { drop }` branches — is unchanged.

### §3.1 Probe state (verbatim from canary, repair.rs:540-548)

```rust
let y40_enabled = y40_collision_probe_enabled();
let mut y40_first_seen: std::collections::HashMap<[QPos; 3], Y40FirstSeen> =
    std::collections::HashMap::new();
let mut y40_collisions: Vec<Y40Collision> = Vec::new();
```

`y40_first_seen` is the probe's parallel attribution map; `seen` (production) drives behavior. Both are populated only when `Y40_COLLISION_PROBE=1`.

### §3.2 Probe types (verbatim from canary, repair.rs:618-632)

```rust
#[derive(Debug, Clone, Copy)]
struct Y40FirstSeen {
    face_id: u64,
    range_idx: usize,
    tri_offset: usize,
}

#[derive(Debug, Clone, Copy)]
struct Y40Collision {
    key: [(i64, i64, i64); 3],
    winner: Y40FirstSeen,
    loser: Y40FirstSeen,
}
```

### §3.3 Env gate + thread-local invocation counter (verbatim, repair.rs:634-649)

```rust
fn y40_collision_probe_enabled() -> bool {
    std::env::var("Y40_COLLISION_PROBE").as_deref() == Ok("1")
}

std::thread_local! {
    static Y40_INVOCATION_COUNTER: std::cell::RefCell<u64> =
        const { std::cell::RefCell::new(0) };
}
```

The counter assigns sequential invocation IDs per thread so per-call TSV files don't collide. The case ID comes from `crate::boolean::yang_integration::current_case_id()` (the same mechanism Y36/Y37/Y38 use).

### §3.4 Env-gated branch inside the existing loop (verbatim, repair.rs:565-597)

```rust
if seen.insert(key) {
    new_indices.push(indices[base]);
    new_indices.push(indices[base + 1]);
    new_indices.push(indices[base + 2]);
    if y40_enabled {
        y40_first_seen.insert(
            key,
            Y40FirstSeen {
                face_id: range.face_id.0,
                range_idx,
                tri_offset: t - tri_start,
            },
        );
    }
} else if y40_enabled {
    let winner = y40_first_seen.get(&key).copied().unwrap_or(/* sentinel */);
    y40_collisions.push(Y40Collision {
        key,
        winner,
        loser: Y40FirstSeen { face_id: range.face_id.0, range_idx, tri_offset: t - tri_start },
    });
}
```

The production `if seen.insert(key)` branch is preserved; the probe records the winner attribution inside that branch (only when enabled). The `else` branch is empty in production and only logs a collision record when the probe is enabled. Default-off path is byte-identical to baseline.

### §3.5 Output (per invocation, three TSV files)

When `Y40_COLLISION_PROBE_DIR` is set and the probe is enabled, the function calls `y40_write_collisions` after the dedup loop. It emits:

- `{case}_inv{NNN}_collisions.tsv` — one row per collision: `collision_idx, key_xa, key_ya, key_za, key_xb, key_yb, key_zb, key_xc, key_yc, key_zc, winner_face_id, winner_range_idx, winner_tri_off, loser_face_id, loser_range_idx, loser_tri_off`
- `{case}_inv{NNN}_histogram.tsv` — per-(winner_face_id, loser_face_id) pair counts
- `{case}_inv{NNN}_summary.tsv` — invocation totals, distinct winners/losers, per-loser histogram, per-winner histogram

## §4 Empirical findings (load-bearing refutation)

### §4.1 F0020 inv006 — predicted vs measured (the refutation table)

`inv006` is the load-bearing F.0 → F.1 boolean-result repair pass. `n_tris_input = 138` byte-matches stage-f `sub=0`; `total_collisions = 19` byte-matches the `sub=0 → sub=1` delta (138 → 119).

| Quantity | PR-Y39 §2.5 predicted | PR-Y40 measured | Refutation |
|---|---|---|---|
| Total collisions at F.0 → F.1 (inv006) | 19 (implicit) | **19** | confirmed |
| D.1d-loser collisions (kids 218 / 232 / 233 as losers) | **16** (3 + 5 + 8) | **4** (1 + 1 + 2) | **REFUTED — off by 4×** |
| Other-kid losers | implied 3 | **15** | refuted |
| D.1d kids surviving F.1 | implied ~0 | **2** (kid 232 / kid 233 each survive one) | confirms PR-Y39 §2.3 downstream observation |

**Root cause of the off-by-4× error in PR-Y39 §2.5.** The Y36 inverse probe's `indices_emitted_dispatch` field is the index count, not the triangle count (each triangle = 3 indices; verified at `tessellation/mod.rs:4984`, `end_index - start_index`). PR-Y39 §2.5 read `218 → 3 / 232 → 6 / 233 → 9` as triangle counts and concluded "kids 218/232/233 emit 18 triangles." The actual triangle counts are **1 / 2 / 3 = 6 triangles**, of which:
- **4 lose** at F.0 → F.1 (kid 218: 1 of 1; kid 232: 1 of 2; kid 233: 2 of 3)
- **2 survive** (kid 232: 1; kid 233: 1)

The survival count of 2 matches PR-Y39 §2.3's downstream observation that at entry to `remove_nonmanifold_topology_aware`, kid 218 has 0 tris and kids 232/233 each have 1. The PR-Y40 measurement is internally consistent with PR-Y39's downstream count — the bug is specifically in PR-Y39 §2.5's interpretation of its own Y36 probe data.

### §4.2 Winner-kid distribution for the 4 D.1d-loser collisions

| Winner kid | D.1d-loser collisions won | % of 4 |
|---|---|---|
| 196 | 1 | 25.0% |
| 198 | 1 | 25.0% |
| 199 | 1 | 25.0% |
| 233 (self) | 1 | 25.0% |
| **Total** | **4** | **100%** |

Fully distributed across 4 distinct winners (3 cross-kid, 1 intra-kid). N = 4 is too small to ground a source-attribution policy. The cross-kid winners 196/198/199 have `face_total_tris` = 8 / 8 / 2 respectively — two are larger than the D.1d losers (consistent with "predatory large-face wins") but one (kid 199 with 2 tris) is the same size as kid 232 (2 tris). The "prefer smaller face_total_tris" Shape C signature does not produce a clean signal here.

### §4.3 Cohort context

| Case | Invocations | Max collisions (per inv) | D.1d-style signature? |
|---|---|---|---|
| **F0044** (PR-Y31 GREEN-extras) | 3 | 4 (load-bearing) | No — 2 symmetric pairs (19↔21, 20↔22), not "small loses to large" |
| **R0045** | 3 | 2 | No — single pair 476→477 |
| **F0045** | 10 (incl. one 13535-tri retess) | 13011 | No — different mechanism (coplanar overlap re-emission of huge planar faces) |
| **R0092** | 17 (incl. one 13692-tri retess) | 13368 | No — same as F0045 retess outlier |

The F0044 4-collision pattern at the load-bearing topo-extract invocation is two symmetric 2-collision pairs (kid 19 vs kid 21, kid 20 vs kid 22). This is not the D.1d signature (small-emitter losing to large-emitter). Consistent with PR-Y37's H1/H2/H3 finding that F0044 cohort is 0% D.1.

The F0045 and R0092 outliers (13K-tri retess-pass collisions) are a separate mechanism. Each is `(N-1)-self-collision` within a single huge face: kid 20 has ~5119 tris in its retess output, of which 1 wins and 5118 lose, all to the same canonical key. This is fully-degenerate quantization (all tris in a giant planar face quantize to the same grid cell at Render LOD). Different defect; banked but not within PR-Y40 scope.

### §4.4 The dominant mechanism at F.0 → F.1 (10 of 19 = 53%)

`collisions.tsv` rows 9–18 share the canonical key `(65051,-15817,-36086, 65051,-15817,-36086, 65051,-15817,-36086)` — all three vertices identical. This is **fully-degenerate triangles** (zero-area emissions where all three quantized vertices coincide), originating from kids 235 and 256 in F0020 inv006. The dominant mechanism at this site is *not* D.1d; it is degenerate-vert collapse upstream (likely planar-face dispatch path for cylinder caps or boss tops). This is the strongest empirical signal that the load-bearing D.1d drop is **upstream of `remove_winding_insensitive_duplicates`** — the canonical-key dedup is mostly catching degenerate-emission artifacts, not D.1d positional duplicates.

## §5 PR-Y41 anchor recommendation

### §5.1 The PR-Y39 §7 banked premise is empirically refuted

PR-Y39 §7 banked three PR-Y41 fix-shape candidates at `remove_winding_insensitive_duplicates`:
- (i) Source-attribution at canon-dedup preferring smaller `face_total_tris` kids
- (ii) Insert-order awareness
- (iii) Upstream dispatch-loop fix

PR-Y40's direct measurement at the same anchor refutes the premise that (i) or (ii) could meaningfully reduce F0020 unpaired count:
- Only **4 D.1d-loser collisions** exist at this site (not 16). N = 4 is too small to ground a policy.
- The dominant collision mechanism (53%) is fully-degenerate triangles, not D.1d positional duplicates. A source-attribution policy at this site does nothing for degenerate-triangle losses.
- D.1d's other 14 indices (= ~4-5 triangles, depending on which lose at each stage) **are not lost here**. They must be lost UPSTREAM of `remove_winding_insensitive_duplicates`.

### §5.2 Banked PR-Y41 candidates (each requires its own canary, NOT committed here)

1. **PR-Y41 candidate (n) — UPSTREAM dispatch-loop lossage canary (recommended).** Re-canary the F.−1 → F.0 transition: instrument the dispatch loop output (the raw mesh that feeds `remove_winding_insensitive_duplicates`) vs. the function's entry. Determine where the other ~12 D.1d-attributed dispatch indices (= 4-5 triangles) are lost. Likely candidates: degenerate-vert collapse at dispatch where 3 vertices quantize to one grid cell; or an upstream `remove_duplicate_triangles` (winding-sensitive) call. This is the empirically-supported next anchor.

2. **PR-Y41 candidate (o) — F.0 face_ranges tri-count audit.** Sum tri counts per kid by `(range.end_index - range.start_index) / 3` at *entry* to `remove_winding_insensitive_duplicates` and cross-reference Y36 inverse-probe `indices_emitted_dispatch / 3`. Validates the indices-vs-tris correction quantitatively across all cohort cases.

3. **PR-Y41 candidate (p) — banked: post-dispatch degenerate-triangle filter** (UPSTREAM of `remove_winding_insensitive_duplicates`). Would eliminate the 10/19 fully-degenerate collisions at F0020 inv006. Unproven to reduce F0020 unpaired count (degenerate tris likely don't contribute valid boundary edges regardless of whether they survive); run a canary first.

### §5.3 Recommended PR-Y41 anchor (load-bearing for the next cycle)

Candidate (n): the dispatch loop output, F.−1 → F.0. Do not re-use the now-refuted "16 collisions, who-wins-collisions" frame at F.0 → F.1. The probe scaffolding from PR-Y40 remains available for future at-site measurements; the next cycle's empirical question shifts upstream.

## §6 Strategic context — 9 PR cycles, 5 ABORTs, 4 INFRA-SHIPs

The F0020 Render LOD search has now spanned 9 PR cycles since PR-Y25:

| PR | Outcome | What was refuted |
|---|---|---|
| Y25 | ABORT (canary) | Yang §4.4.1 mesh-updating (Diagnosis B) was anchor candidate, not empirically the load-bearing site |
| Y26 | ABORT (canary) | Defect is missing triangles cohort-wide, not the 3 candidates from Phase 1 |
| Y27 | ABORT (canary) | `flood_fill_patches` drops zero SourceFaces; D.1 splits into 3 sub-mechanisms |
| Y28 | ABORT (canary) | D.1d kids 218/232/233 identified, but fix-shape canary refused commit |
| Y36 | INFRA SHIP | Y36 inverse-probe source-face attribution (downstream) |
| Y37 | INFRA SHIP | H1/H2/H3 classification refined |
| Y38 | INFRA SHIP | Grid-sensitivity oracle gate |
| Y39 | ABORT (canary) | F.1 → F.2 anchor (`remove_nonmanifold_topology_aware`) refuted; banked F.0 → F.1 |
| **Y40** | **INFRA SHIP — 6th-refutation framing** | **PR-Y39 §2.5's specific 16-tri attribution refuted; off by 4× (indices vs tris)** |

Each cycle eliminates a wrong anchor or attribution. The probe scaffolding accumulated at each INFRA SHIP is durable; it provides the empirical reference layer that successive cycles narrow over. The 5-ABORT-4-INFRA-SHIP ratio is the disciplined response under `feedback_anchor_before_fix` and `feedback_phase1_diagnosis_ranking_is_inference`. Continued infrastructure investment at empirically-correct sites is the working strategy.

## §7 Out of scope (banked, NOT addressed by PR-Y40)

- **F0020 Status:Failed.** 40 unpaired (39 boundary, 1 NMM), 8 degenerate, 10 self-intersections — unchanged. PR-Y40 ships zero production logic.
- **F0045 / R0092 retess-pass 13K-collision outliers.** Different defect (fully-degenerate Render-LOD quantization on huge planar faces). Banked for future scoping; out of scope here.
- **139 / 157 yang_fast failures.** Unchanged (Gate 8 confirms 10/157 baseline).
- **Cherchi TBB non-determinism.** Banked from PR-Y29 / Y30 / Y31. Use missing-count (deterministic) as gate; extras can drift.
- **Yang §4.4.1 mesh-updating (Diagnosis B from PR-Y25).** Banked as the long-term load-bearing layer; no infrastructure or production work here.

No "this closes Yang" or "this is the last bug" language. We do not know how many bugs remain (`feedback_no_last_bug`).

## §8 Risk / mitigation

### §8.1 Default-off byte parity is load-bearing

The probe lives inside the production function. If `Y40_COLLISION_PROBE != "1"`, the path through `remove_winding_insensitive_duplicates` must be byte-identical to the pre-PR-Y40 baseline. Canary §1.3 verified this on F0020 spotlight (Gate 2: same `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int`; same stage-f progression 138→119→119→113→113; same `unpaired` per-stage 30→42→39→39→39).

Implementation must re-run Gates 2 / 7 / 8 fresh on the live tree at commit time:
- Gate 2: `YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture` → 40 unpaired, 8 degen, 10 self-int
- Gate 7: `cargo test -p kernel --lib` → 1262 pass, 24 fail, 42 ignored (kernel baseline)
- Gate 8: `YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture --test-threads=1` → 10/157 pass

### §8.2 Probe-on overhead

The probe-on path allocates an additional `HashMap<[QPos;3], Y40FirstSeen>` and a `Vec<Y40Collision>` and writes three TSVs per invocation. For F0020 inv006 (138 tris, 19 collisions), the overhead is negligible. For the F0045 / R0092 retess outliers (13K collisions), the HashMap can grow to ~5000 entries and TSVs to ~13000 rows. This is dev-only output; not a concern.

### §8.3 Cumulative probe complexity is real

Across `repair.rs` (+151), `tessellation/mod.rs` (+711 from Y36/Y37 carry-over), and `oracle.rs` (+179 from Y38 carry-over), the kernel + test-harness now carry ~1041 LOC of env-gated probe code. This is durable scaffolding but represents cumulative complexity. PR-Y41+ should weigh probe refinement at any single site vs. revisiting the diagnostic strategy. If the next 2–3 cycles continue producing INFRA-only outcomes without converging on a load-bearing production fix, escalate to a different diagnostic frame (e.g., end-to-end Cherchi differential-diff at Render LOD, not just at Stage B).

### §8.4 Sample-size note for §4.2

The 4-distinct-winner distribution at F0020 inv006 (kids 196, 198, 199, 233-self) is N = 4. Any claim about "concentrated vs distributed" winner pattern requires more data. The PR-Y40 canary explicitly refuses to ground a source-attribution policy on this signal; the PR-Y41 candidate (n) at F.−1 → F.0 is the recommended next probe location, not a re-shaping of the F.0 → F.1 frame.

## §9 Paper citations

- **Yang 2025 §4.4.1** (`refs/text/yang2025_hybrid_boolean.txt:605-610`): "As the intersections on the surfaces are relocated and refined during the optimization, the bijectivity is essentially broken. Each intersection curve is no longer mapped to the corresponding intersection curve between the two meshes, thus causing gaps or self-intersections." This conformality-preservation concern is the long-term load-bearing context for D.1d kid loss; PR-Y40 measures one downstream site (canonical-key dedup) but the upstream cause is the mesh-updating layer.

- **Cherchi 2022 §3** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:240-320`): describes the pipeline as a two-step process (mesh arrangement + patch inside/outside classification). The `remove_winding_insensitive_duplicates` site is part of Waffle's Render LOD layer, downstream of Cherchi's arrangement stage. The canonical-key dedup is a Render-LOD-only operation and is outside the Cherchi paper's scope — this is consistent with `feedback_external_coherence`: the PR-Y40 probe IS the empirical reference at this layer; there is no upstream paper-cited oracle for canonical-key collision attribution.

The probe at `remove_winding_insensitive_duplicates` is empirical-only by necessity; no paper section governs its correctness. The reference here is direct measurement of collision attribution against the production function's existing behavior, not a paper-derived contract.

---

## Appendix A — Reproduction commands

```bash
# Gate 2 + 3 — default-off byte parity + probe fires on F0020
rm -rf /tmp/y40-probe && mkdir -p /tmp/y40-probe
Y40_COLLISION_PROBE=1 Y40_COLLISION_PROBE_DIR=/tmp/y40-probe \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture
# expect Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int
# expect 18 TSV files in /tmp/y40-probe (6 invocations × 3 files)

# Gate 4 — D.1d attribution verification (LOAD-BEARING refutation)
cat /tmp/y40-probe/F0020_inv006_summary.tsv
# expect total_collisions=19, loser kid 218=1, kid 232=1, kid 233=2 (D.1d total=4, NOT 16)

# Gate 5 — Winner-kid histogram for D.1d losers
awk -F'\t' '$14 ~ /^(218|232|233)$/ {print $11}' /tmp/y40-probe/F0020_inv006_collisions.tsv \
  | sort | uniq -c
# expect 1× 196, 1× 198, 1× 199, 1× 233

# Gate 6 — Cohort
rm -rf /tmp/y40-cohort && mkdir -p /tmp/y40-cohort
Y40_COLLISION_PROBE=1 Y40_COLLISION_PROBE_DIR=/tmp/y40-cohort \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0044 spotlight_r0045 --ignored --nocapture

# Gate 7 — kernel lib regression
cargo test -p kernel --lib
# expect 1262 passed; 24 failed; 42 ignored

# Gate 8 — yang_fast
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture --test-threads=1
# expect 10/157
```

## Appendix B — Critical files

- `crates/kernel/src/tessellation/repair.rs:502-616` (`remove_winding_insensitive_duplicates`) — probe insertion site (probe at L540-548, L569-597, L610-612 env-gated)
- `crates/kernel/src/tessellation/repair.rs:618-700+` — probe types, env gate, invocation counter, `y40_write_collisions` writer
- `docs/audits/pr_y40_canary.md` — load-bearing canary memo with refutation table
- `docs/audits/pr_y39_canary.md` §2.5 / §7 — refuted attribution chain (LOAD-BEARING context for §1)
- `docs/audits/pr_y28_abort.md` §1 — original D.1d kid identification (218 / 232 / 233; stage attribution now stale)
- `crates/kernel/src/tessellation/mod.rs:4180-4900` — Y36/Y37 source-face probes (read-only reference)
- `crates/test-harness/src/oracle.rs:185-264` — Y38 grid-sensitivity probe (read-only reference)
