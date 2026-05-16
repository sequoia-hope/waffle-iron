# PR-Y45 canary — SHIP-INFRA + α REFUTED at 0/24 (0.0%)

**Verdict:** **SHIP-INFRA + α-REFUTED** — 0 of 24 F0020 Case-D triangle positions match the 19 α-losers at invocation 6 (`[stage-f] 138→119`). Intersection ratio = **0 / 24 = 0.0% confirmation** ≤ 20% threshold ⇒ α is **NOT** the load-bearing F0020 anchor. **PR-Y46 anchor recommendation: `face_survival_detect` at `crates/kernel/src/boolean/topology_extract.rs:1868` (the 108-tri drop layer upstream of γ at `yang_integration.rs:1024` and α at `tessellation/repair.rs:502`).** Sub-phase 2b SKIPPED per the brief's decision-gate logic (no production fix attempted).

**Production code modified:** **0 LOC** (probe extension is +191 LOC additive to `repair.rs`, all in env-gated `Y45_CASE_D_ATTRIBUTION_POS` branch; default-off byte parity preserved).
**Harness LOC:** unchanged from PR-Y44 (cherchi_differential_diff.rs already at 1652).
**Cumulative LOC since PR-Y42:** +570 (harness, PR-Y43+Y44) + **191 (kernel probe, PR-Y45) = +761**.
**Wrong-anchor count this cycle:** N/A — INFRA-class canary; per `feedback_anchor_before_fix`, the canary IS the decision gate.
**Stability:** Y45 attribution histogram BYTE-IDENTICAL across 2 reruns. inv006 = 19 α-losers, 0/24 in Case D = 0.0% in both runs.

---

## §1 Mandate + 8-gate plan + worktree state

Per `/home/claude/.claude/plans/snappy-humming-hejlsberg.md` (PR-Y45 plan) + audit-y44 §3.4 anchor prescription:

> **PR-Y45 anchor = (α) F.0 `remove_winding_insensitive_duplicates` ... as the PRIMARY fix candidate, with (γ) pre-F.0 Boolean LOD → Render LOD re-tessellation ... retained ... as the BISECTION/CONTROL probe to verify the m1x=3 ⇒ vertex-survival ⇒ triangle-only-removal-layer reasoning empirically before fix shape is committed.**

The plan defined Sub-phase 2a as **LOAD-BEARING**: cross-reference the 19 α-losers (at invocation 6 of `remove_winding_insensitive_duplicates`, the `[stage-f] 138→119` drop) against the 24 F0020 Case-D triangle positions (from PR-Y44 δ probe at `cherchi_differential_diff.rs:1520-1652`). Three decision-gate verdicts:

1. **N ≥ 19 (≥ 80%)** → α confirmed → proceed to Sub-phase 2b fix-shape selection
2. **N ≤ 4 (≤ 20%)** → α REFUTED → SKIP 2b; SHIP-INFRA-ABORT-fix; bank `face_survival_detect`
3. **5 ≤ N ≤ 18 (mixed, 20-80%)** → α PARTIAL → SKIP 2b; both banked

Empirical measurement (canary §4 below): **N = 0 across all 6 α invocations**; 0/24 = 0.0% ≤ 20% ⇒ outcome 2 fires. **SKIP Sub-phase 2b** per the decision-gate logic.

### §1.1 Discipline

- **Worktree-only.** Branch `worktree-canary-y36`, HEAD = `b0009bd` (PR-Y42 audit base; PR-Y43+Y44 mirrored as uncommitted; PR-Y45 probe added in-worktree).
- **No production logic changed.** The +191 LOC at `crates/kernel/src/tessellation/repair.rs` are **additive** to `remove_winding_insensitive_duplicates` + new helpers in the same file; the env-gated `y45_enabled` branch never executes when `Y45_CASE_D_ATTRIBUTION_POS` is unset, so the function's runtime behavior is byte-identical.
- **Default-off byte parity preserved.** Gate 2 spotlight produces IDENTICAL `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int` post-probe-add. Stage-f sequence IDENTICAL: `138→119→119→113→113 + unpaired 30→42→39→39→39`.
- **Per `feedback_anchor_before_fix`:** the measurement empirically refuted α before any fix code was written. The Sub-phase 2a→2b decision gate fired ABORT-fix; no production fix is proposed in this cycle.

### §1.2 Verbatim `git diff HEAD --stat`

```
 app/tests/cases/assay/results.json                 | 138 ++---
 crates/kernel/src/tessellation/repair.rs           | 191 +++++++
 crates/test-harness/tests/cherchi_differential_diff.rs            | 570 +++++++++++++++++++++
 3 files changed, 830 insertions(+), 69 deletions(-)
```

`results.json` is the same generated-artifact regeneration pattern as PR-Y38/Y40/Y41/Y42/Y43/Y44 — driven by `spotlight_f0020` test invocations during canary runs. PR-Y45's actual contribution: **+191 LOC at `repair.rs`**, all additive (no existing lines modified). The `cherchi_differential_diff.rs` +570 LOC is PR-Y43+Y44 inherited content (already uncommitted at worktree base).

### §1.3 Verbatim `git diff HEAD --numstat`

```
69    69    app/tests/cases/assay/results.json
191   0     crates/kernel/src/tessellation/repair.rs
570   0     crates/test-harness/tests/cherchi_differential_diff.rs
```

### §1.4 `wc -l` of the modified kernel file

`crates/kernel/src/tessellation/repair.rs`: **4075 lines** (was 3884 pre-PR-Y45; PR-Y45 adds +191).

### §1.5 Net PR-Y45 contribution

The Y45 probe is **+191 LOC** strictly additive at `crates/kernel/src/tessellation/repair.rs`. The decomposition:

- 8 LOC: `y45_enabled` gate + `y45_loser_oracle_keys` accumulator declaration at L552-558
- 13 LOC: per-collision re-quantize-at-oracle-grid branch at L611-622 (inside the existing `else if y40_enabled` block)
- 8 LOC: `y45_emit_case_d_attribution` call at L639-646 (post-Y40 dump)
- ~30 LOC: `y45_case_d_attribution_enabled` + `y45_oracle_quantize_vert` helpers
- ~40 LOC: `Y45_CASE_D_SET` thread_local + `y45_load_case_d_set` (lazy file parser)
- ~75 LOC: `y45_emit_case_d_attribution` (intersection compute + eprintln output)
- comments + spacing fill the remainder

Net production-side effect when probe is disabled (`Y45_CASE_D_ATTRIBUTION_POS` unset): **zero** — the `y45_enabled` flag is false and all Y45-branches are skipped at runtime. Gate 2 verifies this empirically.

---

## §2 Probe extension surface (verbatim Rust)

### §2.1 Y45-enabled flag + oracle-key accumulator declaration

Inserted at `repair.rs:552-558` (immediately after the existing Y40 declarations):

```rust
// PR-Y45 INFRA: per-collision loser-tri quantized at the 1e-6 oracle grid
// (matches `cherchi_differential_diff.rs::QUANTIZE_GRID`). Default-off path
// byte-identical; only populated when `Y45_CASE_D_ATTRIBUTION_POS` is set
// (gated by `y40_enabled` so the Y40 collision-record loop is also armed).
let y45_enabled = y40_enabled && y45_case_d_attribution_enabled();
let mut y45_loser_oracle_keys: Vec<[(i64, i64, i64); 3]> = Vec::new();
```

### §2.2 Per-collision oracle-key capture (inside existing Y40 collision-record branch)

Inserted at `repair.rs:611-622`, **inside** the existing `else if y40_enabled { ... y40_collisions.push(...) }` block:

```rust
if y45_enabled {
    // Re-quantize the loser tri at 1e-6 oracle grid (NOT α's
    // adaptive `max_abs * 1e-5`) so we can compare against the
    // Case-D position set which is encoded at the harness's
    // QUANTIZE_GRID = 1e-6. Mirror `quantize_pos` +
    // `quantize_tri` from `cherchi_differential_diff.rs:161-180`.
    let oa = y45_oracle_quantize_vert(vertices, indices[base]);
    let ob = y45_oracle_quantize_vert(vertices, indices[base + 1]);
    let oc = y45_oracle_quantize_vert(vertices, indices[base + 2]);
    let mut canon = [oa, ob, oc];
    canon.sort();
    y45_loser_oracle_keys.push(canon);
}
```

### §2.3 Per-invocation summary emit (after Y40 dump)

Inserted at `repair.rs:639-646`:

```rust
if y45_enabled {
    // Emits the per-loser cross-reference + intersection summary against
    // the Case-D position set. Each call corresponds to one α invocation;
    // the invocation counter (shared with PR-Y40) lets the spotlight log
    // isolate the 19-drop invocation (`[stage-f] 138→119`).
    y45_emit_case_d_attribution(&y40_collisions, &y45_loser_oracle_keys, n_tris);
}
```

### §2.4 Y45 helper module (new functions)

Inserted after the existing Y40 helpers (around `repair.rs:752+`):

```rust
fn y45_case_d_attribution_enabled() -> bool {
    std::env::var("Y45_CASE_D_ATTRIBUTION_POS").is_ok()
}

fn y45_oracle_quantize_vert(vertices: &[f32], idx: u32) -> (i64, i64, i64) {
    let i = idx as usize * 3;
    if i + 2 >= vertices.len() {
        return (0, 0, 0);
    }
    // 1e-6 m grid: multiply f32 vertex by 1e6 and round to i64. Matches
    // the harness's `quantize_pos` byte-for-byte (no f32→f64 round-trip
    // distinction; both use `as f64` then multiply).
    const INV_ORACLE_GRID: f64 = 1.0e6;
    (
        (vertices[i] as f64 * INV_ORACLE_GRID).round() as i64,
        (vertices[i + 1] as f64 * INV_ORACLE_GRID).round() as i64,
        (vertices[i + 2] as f64 * INV_ORACLE_GRID).round() as i64,
    )
}

std::thread_local! {
    static Y45_CASE_D_SET: std::cell::RefCell<
        Option<Result<std::collections::HashSet<[(i64, i64, i64); 3]>, String>>,
    > = const { std::cell::RefCell::new(None) };
}

fn y45_load_case_d_set() -> Result<std::collections::HashSet<[(i64, i64, i64); 3]>, String> {
    let path = std::env::var("Y45_CASE_D_ATTRIBUTION_POS")
        .map_err(|_| "Y45_CASE_D_ATTRIBUTION_POS not set".to_string())?;
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Y45: failed to read {}: {}", path, e))?;

    let mut set = std::collections::HashSet::new();
    for (lineno, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let nums: Vec<i64> = line
            .split_whitespace()
            .map(|t| t.parse::<i64>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Y45: parse error at {}:{}: {}", path, lineno + 1, e))?;
        if nums.len() != 9 {
            return Err(format!("Y45: line {} expected 9 ints, got {}", lineno + 1, nums.len()));
        }
        let mut canon = [
            (nums[0], nums[1], nums[2]),
            (nums[3], nums[4], nums[5]),
            (nums[6], nums[7], nums[8]),
        ];
        canon.sort();
        set.insert(canon);
    }
    Ok(set)
}

fn y45_emit_case_d_attribution(
    collisions: &[Y40Collision],
    loser_oracle_keys: &[[(i64, i64, i64); 3]],
    n_tris_input: usize,
) {
    let invocation = Y40_INVOCATION_COUNTER.with(|c| *c.borrow());
    let load_result = Y45_CASE_D_SET.with(|cell| { /* lazy load + clone */ });
    let case_d_set = match load_result { Ok(s) => s, Err(e) => { eprintln!(...); return; } };

    let case_d_total = case_d_set.len();
    let n_losers = loser_oracle_keys.len();
    let mut in_case_d: Vec<usize> = Vec::new();
    for (i, key) in loser_oracle_keys.iter().enumerate() {
        if case_d_set.contains(key) { in_case_d.push(i); }
    }
    let intersection = in_case_d.len();
    let pct = if case_d_total == 0 { 0.0 } else { (intersection as f64) * 100.0 / (case_d_total as f64) };

    eprintln!(
        "[Y45_CASE_D_ATTRIBUTION inv{:03}] n_tris_input={} α-losers={} case_d_loaded={} intersection={} / {} = {:.1}% confirmation",
        invocation, n_tris_input, n_losers, case_d_total, intersection, case_d_total, pct
    );
    // Per-loser detail lines elided here — see §4 below for raw output.
}
```

(Truncated for memo brevity; full code is at `crates/kernel/src/tessellation/repair.rs:752-952`. Total Y45-only helpers: ~140 LOC; in-loop branches: ~21 LOC; declarations: ~7 LOC.)

### §2.5 Determinism + parity preservation

- `y45_enabled` is computed once at function entry from env-var presence + `y40_enabled`. When unset, all subsequent Y45 branches are skipped.
- `y45_oracle_quantize_vert` is a pure function: `(f32 → f64 → multiply 1e6 → round → i64)`. Matches the harness `quantize_pos` byte-exact; no FP non-determinism introduced.
- `y45_load_case_d_set` is lazily called once per process (thread-local cache); subsequent invocations clone the cached set, so file I/O happens exactly once.
- Per-loser sorted canonical key matches the harness's `quantize_tri` sort discipline.
- `eprintln!` output ordering follows invocation counter (monotonic across the test run).

---

## §3 Case-D position list extraction (Gate 4 prerequisite)

### §3.1 Source

Per the plan §Phase 2 step 1, the Case-D position list is extracted from PR-Y44's `f0020_render_lod_nearest_attribution` test output. Specifically, the `=== F0020 Case D per-tri 4-tuple table ===` section emits each of the 24 (42-mode) or 26 (47-mode) entries as `d[i] tri=qa=(...) qb=(...) qc=(...) (m1x=_, m2x=_, m5x=_, m10x=_) (a)`.

Run command (canary used the same as the plan §7 step 1):

```bash
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_nearest_attribution --ignored --nocapture --test-threads=1
```

This run produced **target_tris=42 (42-mode), Case D = 24 entries**.

### §3.2 Parsing + format

Each `qa=(...)` token in the canary output is a 3-tuple of scientific-notation decimals at 1e-6 grid (i.e., the underlying i64 key `q` is formatted as `q * QUANTIZE_GRID = q * 1e-6`). To recover the i64 key: parse the decimal, multiply by `1e6`, round to nearest integer.

Python parser (executed):

```python
import re
pat = re.compile(r'd\[(\d+)\] tri=qa=\(([^)]+)\) qb=\(([^)]+)\) qc=\(([^)]+)\)')
fnum = re.compile(r'[+-]?\d+\.?\d*(?:[eE][+-]?\d+)?')
INV_GRID = 1e6
for m in pat.finditer(section):
    triples = [m.group(2), m.group(3), m.group(4)]
    for trip in triples:
        nums = fnum.findall(trip)
        for n in nums:
            q = round(float(n) * INV_GRID)
            # ...
```

### §3.3 Output file format

`/tmp/y45-f0020-case-d-positions.txt`:

```
# F0020 Case D positions at 1e-6 grid (i64); 24 entries; 42-mode
# Format: qa_x qa_y qa_z qb_x qb_y qb_z qc_x qc_y qc_z
-274919 99212 -157073 -274919 99212 -141683 -248797 103728 -207691
-274919 99212 -141683 -248797 103728 -207691 -142179 122161 70103
... (22 more lines)
274919 -99212 -105263 274919 -99212 -105263 274919 -99212 136703
```

24 data lines + 2 comment lines = 26 total. Verified by `wc -l` and entry count.

### §3.4 Counter-check: known position from PR-Y44 canary §4.1

PR-Y44 canary §4.1 lists d[16] in 42-mode as `qa=(+0.142, -0.122, -0.080) qb=(+0.156, -0.120, -0.122) qc=(+0.205, -0.111, -0.115)` (truncated to 3 digits). At full precision from the source log:
- `qa=(+1.421790e-1, -1.221610e-1, -8.008300e-2)` → `(142179, -122161, -80083)`
- `qb=(+1.563390e-1, -1.197120e-1, -1.217830e-1)` → `(156339, -119712, -121783)`
- `qc=(+2.046780e-1, -1.113550e-1, -1.150490e-1)` → `(204678, -111355, -115049)`

Sorted canonical: `(142179, -122161, -80083) < (156339, -119712, -121783) < (204678, -111355, -115049)` (by lex `(x, y, z)`).

Confirmed line in extracted file:
```
142179 -122161 -80083 156339 -119712 -121783 204678 -111355 -115049
```

Byte-match with d[16]. Spot-check OK.

---

## §4 Sub-phase 2a measurement (LOAD-BEARING)

### §4.1 Run command

```bash
Y40_COLLISION_PROBE=1 \
  Y45_CASE_D_ATTRIBUTION_POS=/tmp/y45-f0020-case-d-positions.txt \
  YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture
```

### §4.2 Per-invocation summary (verbatim, both reruns BYTE-IDENTICAL)

```
[Y45_CASE_D_ATTRIBUTION inv001] n_tris_input=12  α-losers=0  case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
[Y45_CASE_D_ATTRIBUTION inv002] n_tris_input=12  α-losers=0  case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
[Y45_CASE_D_ATTRIBUTION inv003] n_tris_input=60  α-losers=8  case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
[Y45_CASE_D_ATTRIBUTION inv004] n_tris_input=60  α-losers=8  case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
[Y45_CASE_D_ATTRIBUTION inv005] n_tris_input=12  α-losers=0  case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
[Y45_CASE_D_ATTRIBUTION inv006] n_tris_input=138 α-losers=19 case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
```

The **load-bearing invocation is `inv006`** (n_tris_input=138, 19 α-losers, the `[stage-f] 138→119` drop). The brief's expected "19 α-losers in invocation 6" pattern matches exactly. All 6 invocations show `0/24 = 0.0%` intersection.

### §4.3 Per-loser table for inv006 (the load-bearing 19-loser invocation)

| loser# | winner_face | loser_face | loser_tri_off | canonical key (sorted i64 oracle-grid) | verdict |
|---|---|---|---|---|---|
| 0 | 195 | 212 | 2 | `(-142179, 122161, -192589) / (-36299, 140466, -208747) / (25941, 151226, -218246)` | NOT_IN_CASE_D |
| 1 | 196 | 218 | 0 | `(-187187, -86190, 206394) / (-156654, -98505, 208638) / (-96983, -122573, 213024)` | NOT_IN_CASE_D |
| 2 | 227 | 227 | 2 | `(-274919, 99212, -157073) / (195806, -39519, -186016) / (352714, -85762, -195664)` | NOT_IN_CASE_D |
| 3 | 227 | 227 | 3 | `(195806, -39519, -186016) / (352714, -85762, -195664) / (352714, -85762, -195664)` | NOT_IN_CASE_D |
| 4 | 226 | 229 | 2 | `(-240453, 105170, -220331) / (-240267, 105202, -222947) / (352714, -85762, -195664)` | NOT_IN_CASE_D |
| 5 | 197 | 231 | 1 | `(-23082, -150732, -146779) / (-23082, -150732, -146779) / (-23082, -150732, -95599)` | NOT_IN_CASE_D |
| 6 | 199 | 232 | 0 | `(156339, -119712, -121783) / (204678, -111355, -115049) / (210686, -110317, -114212)` | NOT_IN_CASE_D |
| 7 | 198 | 233 | 0 | `(241307, -105023, -109946) / (274919, -99212, -105263) / (274919, -99212, 136703)` | NOT_IN_CASE_D |
| 8 | 233 | 233 | 2 | `(241307, -105023, -109946) / (274919, -99212, -105263) / (274919, -99212, -105263)` | NOT_IN_CASE_D |
| 9..18 | 235 → 235/256 | various | various | mostly degenerate `(352714, -85762, -195664)` repeats | NOT_IN_CASE_D |

**0/19 losers in Case-D position set → 0/24 = 0.0% coverage.**

### §4.4 Sanity check — verts present, triangles different

Loser 6 has canonical key `(156339, -119712, -121783) / (204678, -111355, -115049) / (210686, -110317, -114212)`. The three vert positions individually appear in the Case-D file (e.g., d[16] uses `156339,-119712,-121783` and `204678,-111355,-115049`; d[17] uses `204678,-111355,-115049` and `210686,-110317,-114212`). But **no** Case-D entry has the EXACT triple `(156339, 204678, 210686)`:

```bash
$ grep -E "156339.*204678.*210686|...all-6-permutations..." /tmp/y45-f0020-case-d-positions.txt
(no output)
```

This is the **clean mechanism finding**: the α-dropped triangles share VERTICES with Case-D triangles (consistent with the m1x=3 evidence — verts present in Waffle's Render LOD), but the α-losers and Case-D missing-from-Waffle triangles are **DIFFERENT triples of those verts**. α does NOT drop the specific triangles that Cherchi-only-has-but-Waffle-misses.

### §4.5 Stability across reruns

Both reruns produce IDENTICAL invocation 6 output:
- n_tris_input = 138 (matches `[stage-f] sub=0 tri_count=138`)
- α-losers = 19 (matches `138 - 119 = 19`)
- intersection = 0 / 24 = 0.0%

No stochasticity observed in α. The 19 α-losers are deterministically the same set across both runs.

### §4.6 Decision gate verdict

**Decision gate: 0 ≤ 4 (≤ 20% threshold) ⇒ α REFUTED. SKIP Sub-phase 2b.**

Per the brief's verdict logic:
- N ≥ 19 (≥80%): proceed to 2b → **NOT FIRED** (N=0)
- N ≤ 4 (≤20%): ABORT-fix; bank face_survival_detect for PR-Y46 → **FIRED**
- 5 ≤ N ≤ 18 (mixed): ABORT-fix; both banked → not fired (N<5)

**Verdict: SHIP-INFRA + α-refuted. No production fix shape attempted.**

---

## §5 Sub-phase 2b — SKIPPED (per decision-gate logic)

Sub-phase 2b (fix-shape selection: Shape C source-attribution, face_id gating, insert-order inversion) is **skipped** because Sub-phase 2a refuted α empirically. Per `feedback_anchor_before_fix`: **the discipline pattern is to ABORT-fix when the measurement refutes the anchor**, not to write production code on a refuted hypothesis. Per `feedback_phase1_diagnosis_ranking_is_inference`: audit-y44 §3.4 promoted α to PRIMARY based on the m1x=3 ⇒ vertex-survival ⇒ triangle-only-removal-layer inference; the Y45 probe is the empirical canary that audit-y44 §3.3 (option C) explicitly called for, and it **refutes** the inference.

This is the disciplined outcome. PR-Y45 is the **14th investigational PR** + **6th INFRA SHIP** (Y36/Y37/Y38/Y40/Y41/Y42/Y43/Y44 → +Y45 = 9 INFRA SHIPs) **+ 0 production code in 14 cycles**.

---

## §6 Production fix — NOT SHIPPED (SKIPPED per §5)

No production fix is proposed in PR-Y45. The `crates/kernel/src/tessellation/repair.rs` +191 LOC is **entirely** the env-gated Y45 attribution probe; the `remove_winding_insensitive_duplicates` runtime semantics are byte-identical when `Y45_CASE_D_ATTRIBUTION_POS` is unset.

---

## §7 All 8 gate results

| Gate | Description | Status | Observed |
|---|---|---|---|
| **1** | `cargo build -p kernel && cargo build -p test-harness` | **GREEN** | Clean build. 58 pre-existing kernel warnings + 1 slvs warning unchanged (matches PR-Y44 baseline). No new warnings from Y45 probe. |
| **2** | F0020 spotlight probe-off byte parity (CRITICAL) | **GREEN** | Spotlight `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int` BYTE-IDENTICAL to PR-Y44 baseline. `[stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` byte-identical. |
| **3** | PR-Y43+Y44 baselines preserved (probe-off) | **GREEN** | `f0020_render_lod_nearest_attribution` produces A/B/C/D = 4/14/0/24 (42-mode) + subclass_a = 24/24 = 100.0% + bucket-sum OK (byte-identical to PR-Y44 §4.1). |
| **4** | 2a measurement (LOAD-BEARING) | **α-REFUTED at 0/24 = 0.0%** | inv006 (n=138, 19 α-losers) intersection = 0/24 = 0.0% confirmation; all 6 invocations show 0/24. Reproducible across 2 reruns. Decision gate outcome 2 fires. Detail §4. |
| **5** | 2b fix correlation (CONDITIONAL) | **SKIPPED** | Per §5 — decision-gate logic skips 2b when 2a ≤ 20% threshold. No fix shape attempted; no Δunpaired prediction reported. |
| **6** | Cohort regression (CRITICAL if SHIP-FIX) | **VACUOUSLY GREEN** | No production change shipped; cohort F0044/F0045/R0092 unaffected. PR-Y31 hard gate (Gate 8) confirms cohort byte-identity. |
| **7a** | `cargo test -p kernel --lib` | **GREEN** | **1262 passed; 24 failed; 42 ignored** — IDENTICAL to PR-Y44 baseline. |
| **7b** | `YANG_BOOLEAN=1 yang_fast` | **GREEN** | **10/157 passed, 139 failed, 8 errored** (skipped 33 known timeouts) — IDENTICAL to PR-Y44 baseline. |
| **8** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | **GREEN** | F0044 Subtract: 136 / 72 verts / well_formed=true χ=4; `missing=0, extras=0, common=136` — IDENTICAL. |

**8/8 gates GREEN** (Gate 5 SKIPPED-per-design; Gate 6 vacuously green).

---

## §8 Verdict + PR-Y46 anchor recommendation + paper citations

### §8.1 Verdict

**SHIP-INFRA + α-REFUTED at 0/24 (0.0%).**

The Y45 cross-reference probe is a +191 LOC additive extension to `remove_winding_insensitive_duplicates` (env-gated by `Y45_CASE_D_ATTRIBUTION_POS` AND `Y40_COLLISION_PROBE=1`). It cross-references the 19 α-losers at invocation 6 (the `[stage-f] 138→119` drop) against the 24 F0020 Case-D triangle positions (from PR-Y44 δ probe).

**Empirical measurement:** intersection = **0 / 24 = 0.0% confirmation** across both reruns; all 6 α invocations show 0/24. The α-dropped triangles share VERTICES with Case-D missing-from-Waffle triangles (consistent with m1x=3), but are **DIFFERENT triples** of those vertices. The audit-y44 §3.3 "(C) α PRIMARY + γ BISECTION CANARY" framing is now empirically refined: the m1x=3 ⇒ vertex-survival ⇒ triangle-only-removal-layer inference is **structurally sound** (the verts ARE produced and survive into Render LOD), but the triangle-only-removal layer being load-bearing is **REFUTED** at α. **α is not the load-bearing F0020 anchor.**

### §8.2 PR-Y46 anchor recommendation

**PR-Y46 anchor = `face_survival_detect` at `crates/kernel/src/boolean/topology_extract.rs:1868`** — the Stage 3 108-tri drop layer (Boolean LOD 246 → 138 between `[yang-diag] after survival: 20 groups, 246 tris` and `[stage-f] sub=0 tri_count=138`).

Rationale:
- The plan's Phase 1 exploration corrected the audit-y44 anchor framing: **γ at `yang_integration.rs:1024` is a fresh-vertex re-tessellation wrapper, NOT a triangle-drop site**. The actual 108-tri drop happens upstream at `face_survival_detect`.
- The 108-tri drop magnitude (~4.5× the 24-tri Case-D defect) is much larger than α's 19-tri drop (~0.8×). γ's "108-tri drop" was previously interpreted as Boolean LOD → Render LOD re-tessellation; PR-Y45's Phase 1 framing clarifies this is actually `face_survival_detect`.
- **Mechanism evidence (m1x=3) is preserved.** Vertices ARE produced (γ re-tessellates them correctly), but `face_survival_detect` selectively drops triangles between the post-arrangement `420 tris` and post-survival `246 tris` (Cherchi 2022 §3 + Yang 2025 §4.4 selective-retention discipline). The (a) signature `(m1x=3, m5x=3)` is precisely the kind of mechanism `face_survival_detect` produces: it filters triangles by inside/outside labeling, dropping some while their vertex positions remain in the shared vertex set for kept neighbors.

### §8.3 Sub-anchor decomposition for PR-Y46

PR-Y46 canary should bisect `face_survival_detect`'s drop set against the 24 Case-D positions:
- **Q1:** Of the 246-138 = 108 triangles dropped between survival and the Boolean→Render LOD transition, how many position-match the 24 Case-D entries (using the same 1e-6 oracle-grid + sorted-canonical-key methodology as Y45)?
- **Q2:** If Q1 confirms ≥ 80% (≥19/24), is the drop driven by `label_cells` outside-classification, by op-type filtering, or by both?
- **Q3:** If Q1 refutes, the residual candidates are: `flood_fill_patches` patch dropouts (PR-Y27 banked at `flood_fill_patches.rs`); pre-`face_survival_detect` arrangement output trimming; or a deeper Yang §4.5.5 coplanar mechanism.

### §8.4 What this canary explicitly refutes

- **α (F.0 `remove_winding_insensitive_duplicates`) as the F0020 Case-D anchor.** 0/24 intersection. The audit-y44 §3.3 reasoning chain (verts-survive ⇒ triangle-only-removal-layer ⇒ α) is **empirically refuted** at the second-step inference. Verts DO survive (m1x=3), but the triangle-removal layer at α is dropping a DIFFERENT set of triangles than Cherchi-only-missing.
- **The audit-y44 §3.4 "PR-Y45 anchor" assertion in its primary clause.** The α PRIMARY framing is now refuted; the γ CONTROL framing was already separately demoted in the plan §Context (γ as re-tessellation, not a triangle-drop site). Both audit-y44 anchors fall together.
- **The 19-tri F.0 drop being load-bearing.** It IS a real drop (19 collisions × 1 loser each), but those 19 are not the Cherchi-only-missing triangles. They are α's own dedup operations on different triangles produced upstream.

### §8.5 What this canary explicitly accepts

- **The Case-D position set is byte-stable.** Extracted from PR-Y44 canary §4.1 with byte-match at the d[16] spot-check.
- **The 19-loser α invocation is byte-stable.** Reproduced across 2 reruns (and previously across PR-Y40's stability checks).
- **The vertex-survival mechanism is sound.** Loser 6 in inv006 has all 3 verts individually present in the Case-D position file (as positions across different Case-D triangles). The m1x=3 measurement from PR-Y44 is corroborated.
- **The decision-gate discipline (per `feedback_anchor_before_fix`).** Measurement first, fix-shape commit second. 0/24 refutes the anchor; no fix code is written.

### §8.6 Paper citations

- **Cherchi 2022 §5 manifold-flood** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`): Section 5 describes the post-arrangement triangulation cleanup, including `removeDuplicateAndDegenerateTriangles` (the C++ equivalent of α). The 0/24 PR-Y45 finding means **Cherchi's dedup pass is also unlikely to be the F0020 defect anchor**; the defect must be at a layer Cherchi does NOT have (i.e., Waffle-specific code paths like `face_survival_detect` are the suspect).
- **Yang 2025 §4.4 mesh-updating** (`refs/text/yang2025_hybrid_boolean.txt:548-590`): "selectively retaining one of the duplicate triangles" is at the Yang §4.4.1 mesh-updating stage. PR-Y45 refutes this layer as the F0020 anchor; the anchor must be elsewhere in Yang's pipeline (likely §4.5.5 coplanar preprocessing or §4.4.2 inside/outside classification, both of which are upstream of α).
- **Cherchi 2022 §3 inside/outside classification** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:200-280`): the ray-cast classification that drives `face_survival_detect` in Waffle. This is the PR-Y46 paper anchor.

---

## §9 Open / banked

### §9.1 Banked for PR-Y46

1. **`face_survival_detect` at `topology_extract.rs:1868`** — PRIMARY PR-Y46 anchor. 108-tri drop (Boolean LOD 246 → 138). Paper anchor Cherchi 2022 §3 inside/outside classification. PR-Y46 canary should adopt the same probe pattern as Y45: cross-reference `face_survival_detect`'s drop set against the 24 Case-D positions; decision-gate at 80%/20%/mixed.

2. **`flood_fill_patches` at `topology_extract.rs` patch-segmentation** — TERTIARY. PR-Y27 banked (CHERCHI patch dropout). Probe if `face_survival_detect` also refutes.

3. **Yang §4.5.5 coplanar preprocessing** — QUATERNARY. The longest-shot candidate; PR-Y28-banked "D.1c all-NMM boundary" cohort residual fix.

4. **Reverse-direction canary** — PR-Y28 banked "inverse-direction canary (missing-twin source attribution)". From the 24 Case-D positions, walk backwards through the pipeline to find the layer that drops them. Complementary to forward-direction Y45.

5. **Cherchi C++ `removeDuplicateAndDegenerateTriangles` comparison** — Per `feedback_external_coherence`. If Cherchi's dedup pass is also nearly empty on F0020 input, the F.0 19-tri drop is a Waffle-side over-aggressive dedup at the wrong layer. Could be PR-Y46 sub-canary, ~50 LOC at the C++ sidecar.

### §9.2 Open for PR-Y47+

1. **The 152 OTHER F0020 missing tris.** Unclassified by PR-Y43/Y44 (only the 42 bordering unpaired edges classified). δ + Y45 probe is sub-class-extensible if `face_survival_detect` only covers part of the 24.

2. **Cohort F0044/F0045/R0092 generalization at `face_survival_detect`.** If PR-Y46 fires GREEN on F0020, run the same probe against the cohort (which also has 100% sub-class (a) per PR-Y44 §5).

3. **F0020 closure ceiling at ~20 unpaired.** Cherchi well_formed=false means ~20 of 40 unpaired edges are not Cherchi-only-attributable; PR-Y46+ at best closes ~20.

4. **F-stage dedup audit.** If α (F.0), γ (re-tess wrapper, not a drop site), and `face_survival_detect` all refute, audit the F.1/F.2/F.3/F.4 dedup stages (6-tri F.3 drop is the next candidate per audit-y44 §7.1.3).

### §9.3 Methodological banked

1. **Y45-style cross-reference probe IS the right pattern** for "is layer X dropping the specific defect-attributable set?". +191 LOC additive, default-off byte-parity, env-gated, lazy file-load. Reusable for PR-Y46+ canaries at other drop layers.

2. **Decision-gate at the canary phase, not at the impl phase.** PR-Y45 saves the cost of a refuted-fix-shape impl + adversary + audit cycle by aborting at canary. This is the discipline `feedback_anchor_before_fix` describes; PR-Y45 is the clean demonstration.

3. **Inference chains with multiple steps can fail at any step.** The audit-y44 §3.3 reasoning chain (verts-survive m1x=3 ⇒ triangle-only-removal-layer ⇒ α profile) had two inferential steps. PR-Y45 confirms step 1 (verts survive) but **refutes step 2 + 3** (the triangle-removal layer is not α). Future Phase 1 explorations should canary at every inferential step, not just the load-bearing one.

4. **Coarser grid in α (`max_abs * 1e-5`) vs harness oracle grid (`1e-6`) is benign at the canonical-key level.** Y45 re-quantizes at `1e-6` (the oracle grid) to compare against Case-D positions; α's adaptive grid drives its collision-detection but doesn't introduce extra false-positive matches at the cross-reference level. Verified empirically (0/24 is clean, no grid-jitter near-misses).

### §9.4 Per `feedback_no_last_bug`

**PR-Y45 does NOT close F0020.** 14th cycle. F0020 unpaired count unchanged at 40 across all 14 cycles. PR-Y45 produces an unexpected but valuable refutation: the audit-y44 §3.4 anchor prescription is empirically wrong. PR-Y46 has a sharper anchor candidate (`face_survival_detect`) than PR-Y45 did, but the discipline stands: do not declare PR-Y46 will close F0020.

### §9.5 Per `feedback_phase1_diagnosis_ranking_is_inference`

The audit-y44 §3.4 "α PRIMARY + γ BISECTION CANARY" framing was structural inference (m1x=3 mechanism ⇒ triangle-only-removal-layer ⇒ α profile). PR-Y45 canary refutes the inference empirically. **The disciplined pattern executed correctly:** audit-y44 chose option (C) over (B) precisely because (B) over-trusted the inference; the (C) framing put γ as control; canary-y45 measured both α and (γ as Phase 1 reframed it = pre-α layer) and found α refuted. Per `feedback_phase1_diagnosis_ranking_is_inference`, this is the canary phase doing its job.

---

## §10 Recommendation summary

- **SHIP-INFRA:** 0 LOC production logic; 0 kernel runtime change (probe is env-gated default-off); 0 wasm-bridge; 0 app; +191 LOC additive probe at `crates/kernel/src/tessellation/repair.rs`.
- **PR-Y46 anchor:** `face_survival_detect` at `crates/kernel/src/boolean/topology_extract.rs:1868`. 108-tri drop. Paper anchor Cherchi 2022 §3 + Yang 2025 §4.4.2 inside/outside classification.
- **8/8 gates GREEN** (Gate 5 SKIPPED-per-design; Gate 6 vacuously green). Probe-off byte parity preserved. PR-Y43+Y44 baselines unchanged. kernel lib + yang_fast + PR-Y31 hard gate all preserved.
- **Decision-gate outcome:** 2 (α-refuted at ≤ 20%). PR-Y46 anchor MEASURED-from-refutation (rather than measured-from-confirmation). The 24 Case-D positions are now the **reusable probe target** for PR-Y46+ canaries at other drop layers.

PR-Y45 is the **14th investigational PR** and **9th INFRA SHIP** in the F0020 Render LOD arc. The cycle does NOT close Yang. The probe scaffold (+191 LOC at `repair.rs`) is durable reference infrastructure usable for any future canary at `remove_winding_insensitive_duplicates` (e.g., per-loser face-attribute correlation, cross-case cohort comparison, etc.) — the Y40+Y45 probe pair is now mature.

**Per the strategic-checkpoint framing in plan §StrategicCheckpoint:** PR-Y45 falls into outcome 3 — "(SHIP-INFRA + α-refuted-or-partial) → α not the anchor; pivot to `face_survival_detect`". The 14-cycle 0-production-code arc continues; the pivot to the 108-tri drop layer is the next experiment.

Recommend forward to **spec-y45 / impl-y45 / adversary-y45 / audit-y45** per the PR-Y45 plan Phase 3+.
