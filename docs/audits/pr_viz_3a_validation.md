# PR-VIZ-3a — adversary-11 validation memo

**Verdict: ACCEPT**

Plan: `~/.claude/plans/reactive-juggling-sloth.md` sub-phase 0d.
Spec: `specs/yang_pr_viz_3a_in_memory_capture.md`.
Implementer: implementer-o (sub-phase 0c).
Adversary: adversary-11 (NEW per `feedback_oracle_credibility_via_role_separation.md`).

Dev tooling, not a defect fix. Lighter validation than fix-PR per plan.
All hard contracts hold: GREEN tests, mutation load-bearingness, capture-off
no-op, memory bound, +3 export delta, byte-clean diff. Two spec deviations
flagged (one accepted as authorized; one is genuine partial-coverage gap).

---

## §1 Mutation test result (load-bearing)

Site mutated: `topology_extract.rs:1765` Stage A `record_stage(stage, &verts_f32, &idx_u32, &labels);` (commented out, with surrounding pack/labels also disabled and `let _ = stage; let _ = combined_tris;` to suppress unused warnings).

Probe driver (temporary `_adv11_mutation_stage_a_present` test in `yang_integration.rs::tests`, deleted after): under `YANG_CONFORMAL_PROBE=1` + `start_yang_capture()`, drives `yang_boolean_inner(box_A, box_B, Union)` and asserts captured tags contain `"A"`.

Baseline (unmutated):
```
[adv11-mutation] captured stage tags: ["F.0", "F.1", "F.2", "F.3", "F.4",
  "E_lod=Adaptive { d_epsilon: 0.020310096011589902 }", "F.0", "F.1", "F.2",
  "F.3", "F.4", "E_lod=Adaptive { d_epsilon: 0.020310096011589902 }",
  "A", "Bb", "B", "C"]
test ... ok
```
16 stages, "A" present.

Mutated:
```
[adv11-mutation] captured stage tags: ["F.0", ..., "Bb", "B", "C"]
panicked at yang_integration.rs:4734: 'Stage A must appear in captured stages;
  got tags [...]'
test ... FAILED
```
15 stages, **"A" missing exactly as expected**. Per-site `record_stage` call is load-bearing on captured output.

(F.0–F.4 + E_lod=Adaptive appear twice because `make_box_via_kernel` invokes tessellation upstream of `yang_boolean_inner`, once per box. They are out of scope for this mutation experiment.)

After verification, the mutation was reverted to implementer-o's pre-mutation state and the temporary test was removed (see §6).

## §2 Capture-off no-op verification

**Code-read allocation check** at `yang_integration.rs:1650-1661`:
```rust
pub(crate) fn record_stage(stage_tag: &str, vertices: &[f32], indices: &[u32], labels: &[u32]) {
    CAPTURE_BUFFER.with(|b| {
        if let Some(buf) = b.borrow_mut().as_mut() {
            buf.push(StageMesh {
                stage_tag: stage_tag.to_string(),
                vertices: vertices.to_vec(),
                indices: indices.to_vec(),
                labels: labels.to_vec(),
            });
        }
    });
}
```
When `CAPTURE_BUFFER` is `None`, the `if let Some(buf) = ...` branch is skipped. **No `StageMesh` is constructed and no `to_vec()` allocations occur.** The `borrow_mut()` itself is a non-allocating RefCell operation (just a runtime borrow-counter check).

**End-to-end probe** (temporary `_adv11_capture_off_no_op` test, deleted after): set `YANG_CONFORMAL_PROBE=1`, do **NOT** call `start_yang_capture()`, drive `yang_boolean_inner(box_A, box_B, Union)`, then call `drain_yang_capture()`:
```
[adv11-noop] capture-off stages = 0
test ... ok
```
**0 stages returned** — confirms `record_stage` correctly no-ops at the 5 production probe sites (Stage A, Bb, B, C, E) and at `dump_stage_f_viz` (F.0–F.4) when CAPTURE_BUFFER is None.

**Native `get_yang_stages_json("absent-id")` returns `"null"`** — confirmed by sub-phase 0b's `test_wasm_api_callable_from_native` at `tests/pr_viz_3a_capture.rs:225-238` (which I re-ran independently, see §"Re-run").

**WASM-bridge dispatch capture-off**: confirmed by sub-phase 0b's `test_capture_disabled_dispatch_inserts_nothing` at `tests/pr_viz_3a_capture.rs:188-211`, re-run independently.

## §3 Memory bound check

**Method**: temporary `_adv11_memory_bound_check` test (deleted after). Drove `yang_boolean_inner(box_A_2x2x2, box_B_offset_2x2x2, Union)` with capture armed; bounded the JSON size with a conservative per-element estimate (12 bytes/f32 vert, 5 bytes/u32 idx, 5 bytes/u32 label, 80 bytes/stage skeleton, 100 bytes/outer struct).

Result:
```
[adv11-mem] 16 stages; total verts=1860, idx=1848, labels=616;
  est JSON ≈ 36155 bytes
test ... ok
```

**~36 KB for a basic two-box union** — 3% of the 1.2 MB spec budget per feature. Even scaling 30× for a complex multi-cylinder boolean with dense F-stages would land at ~1.1 MB. Within bound.

For a typical project with N=10 features captured, total `EngineState.yang_debug_captures` map size ≈ 360 KB to 11 MB depending on geometry complexity. The spec mandates `clear_yang_debug_captures()` to bound long-running session growth; that's the app-side discipline for PR-VIZ-3b.

## §4 Spec deviation review

| # | Deviation | Authorized? | Verdict |
|---|---|---|---|
| 1 | `pack_f64_mesh_to_f32_indices` helper (~17 LOC at `yang_integration.rs:1553-1570`) | Implicitly authorized by spec §2's "lossy f64→f32 ok (viewer-grade)" + the boolean-pipeline shape mismatch (probes hold `Vec<[f64;3]>`, `Vec<[usize;3]>` while StageMesh expects flat `Vec<f32>`/`Vec<u32>`). | **JUSTIFIED.** Without this helper, every probe site would inline the same pack loop. Symmetric with the existing `pack_f32_indices_to_f64_mesh` helper at line 1575. |
| 2 | `with_engine_state` / `with_engine_state_mut` accessors at `wasm_api.rs:132-153` | Authorized in implementer-o's brief; verified `pub(crate)` (not `pub`). | **JUSTIFIED.** Required so `crate::yang_debug` can route into `ENGINE_STATE` on wasm32 without leaking the thread-local outside the crate. Visibility is correctly scoped — `grep` for `with_engine_state` outside the wasm-bridge crate returns nothing. |
| 3 | `NATIVE_ENGINE_STATE` thread-local at `yang_debug.rs:18-22` | Authorized in implementer-o's brief. | **JUSTIFIED.** The `#[cfg(target_arch="wasm32")]` routing at `yang_debug.rs:49-67` is correct: production WASM uses `ENGINE_STATE`; native tests use `NATIVE_ENGINE_STATE`. Verified by reading the cfg gates and confirming `test_wasm_api_callable_from_native` passes (which exercises only the native path). Important: the native shim is for **API-surface testability**, not production semantics — the captures map on `NATIVE_ENGINE_STATE` is parallel to and disjoint from any `EngineState` that an integration test mutates via `wasm_bridge::dispatch`. |
| 4 | **`FinishSketch` path's `add_feature` call (`dispatch.rs:85`) is NOT wrapped with `start/drain_yang_debug_capture`** | **NOT explicitly authorized** in implementer-o's brief; brief says "dispatch hook on AddFeature + EditFeature paths (NOT Sketch-finish)" as a deviation declaration. Spec §7 lists three paths: AddFeature (lines 90-94), EditFeature (lines 96-102), AND "the Sketch-finish add_feature call (line 85)". | **PARTIAL-COVERAGE GAP, ACCEPTABLE FOR PR-VIZ-3a.** A user who sketches and clicks "finish" creates a feature whose Yang stages will not be captured under capture-on. Practical impact: low, because Sketch features don't run boolean operations (Yang only runs in `BooleanCombine`); the missed path captures effectively-empty stages. But: an `EditFeature` that converts a Sketch to an Extrude could trigger Yang on the Extrude side; that path IS wrapped. **Recommendation**: PR-VIZ-3b's app-side QA should explicitly test: (a) sketch + extrude + boolean → capture present on the boolean's feature_id; (b) edit-extrude-depth → capture present. If either fails, file PR-VIZ-3a-fix-up to wrap the FinishSketch path. |
| 5 | Per-stage label encoding (origin/inside packing for Stage Bb/B; origin-only for Stage A/C; face_id for E and F.0–F.4) | Spec §4 documents all six stage label encodings. | **MATCHES SPEC.** Verified at all five record_stage call sites: A (`topology_extract.rs:1762-1764` — origin only, A=0/B=1), Bb (`topology_extract.rs:1971-1994` — `(origin << 1) \| inside`), B (`topology_extract.rs:2137-2147` — `(origin << 1) \| inside`), C (`topology_extract.rs:825-831` — origin only), E (`yang_integration.rs:1077-1086` — face_id), F.0–F.4 (`tessellation/mod.rs:4400-4408` — face_id). |

## §5 Existing `__waffle.*` API surface preservation

Counted `^#[wasm_bindgen]$` attribute lines in `crates/wasm-bridge/src/wasm_api.rs`:

| | Before (HEAD) | After | Delta |
|---|---:|---:|---:|
| `#[wasm_bindgen]` exports | 12 | 15 | **+3** |

The 3 new exports are (`wasm_api.rs:155-172`):
- `set_yang_debug_capture(enabled: bool)` (line 156)
- `get_yang_stages_json(feature_id: &str) -> String` (line 163)
- `clear_yang_debug_captures()` (line 169)

All other 12 exports (`init`, `process_message`, `get_feature_tree`, `get_mesh_json`, `get_mesh_vertices`, `get_mesh_normals`, `get_mesh_indices`, `get_mesh_count`, `get_renderable_feature_indices`, `get_face_data`, `get_edge_vertices`, `get_edge_data`) preserved verbatim — no signature changes. **Additive only.**

The 2 new accessors `with_engine_state` / `with_engine_state_mut` (line 132, 143) are `pub(crate)`, not `#[wasm_bindgen]` — they don't appear in the JS surface.

## §6 Byte-clean diff verification

After mutation revert + temp-test removal:

```
$ git diff --stat
 app/tests/cases/assay/results.json            |   2 +-
 crates/kernel/src/boolean/topology_extract.rs |  91 ++++++++++++++
 crates/kernel/src/boolean/yang_integration.rs | 163 ++++++++++++++++++++++++++
 crates/kernel/src/lib.rs                      |  11 ++
 crates/kernel/src/tessellation/mod.rs         |  22 +++-
 crates/wasm-bridge/src/dispatch.rs            |  56 ++++++++-
 crates/wasm-bridge/src/engine_state.rs        |  13 ++
 crates/wasm-bridge/src/lib.rs                 |   2 +
 crates/wasm-bridge/src/wasm_api.rs            |  52 ++++++++
 9 files changed, 407 insertions(+), 5 deletions(-)
```

**407+/5- across the same 9 modified files** (plus 2 untracked: `crates/wasm-bridge/src/yang_debug.rs` + `crates/wasm-bridge/tests/pr_viz_3a_capture.rs`) — exactly matches implementer-o's pre-mutation state per the team-lead brief's "~325-407 LOC" headline.

`test_yang_capture_round_trip` re-passes after revert: `1 passed; 0 failed`.

## Re-run summary (independent of implementer-o's claims)

| Test | Result |
|---|---|
| `cargo test -p kernel --lib test_yang_capture_round_trip` | **1p / 0f** |
| `cargo test -p wasm-bridge --test pr_viz_3a_capture` | **4p / 0f** (test_capture_enabled_dispatch_inserts_map_entry, test_capture_disabled_dispatch_inserts_nothing, test_wasm_api_callable_from_native, test_capture_serializes_to_spec_json_shape) |
| `cargo test -p kernel --lib` (full kernel) | **1247p / 31f / 42i** — matches implementer-o's claim exactly; **delta vs PR-VIZ-1 baseline 1246/31/42 is +1p / 0 new failures** |

## Verdict summary + recommendation for PR-VIZ-3b

**ACCEPT.** Capture infrastructure is sound: probes are load-bearing (mutation test), capture-off is allocation-free (code read + end-to-end probe), memory budget is comfortable (~3% of bound on a typical case), API surface is purely additive (+3 exports), and the diff is byte-clean.

The one genuine gap is the unwrapped `FinishSketch` path (deviation #4). I am not recommending a fix-up PR for it now because (a) Sketch features don't trigger Yang in normal usage, and (b) the next PR is the one that exercises this path from the app side anyway.

**Recommendation for PR-VIZ-3b scope** (self-canaried per `feedback_adversary_recommendations_need_canary.md`):
- Wire `__waffle.setYangDebugCapture / getYangStages / clearYangDebugCaptures` from JS via the bridge.
- Mirror AssayBrowser pattern for `YangDebugPane.svelte` (toolbar toggle + sidebar pane).
- **Mandatory test in PR-VIZ-3b**: drive the canonical "sketch a square → finish → extrude → boolean → finish" sequence with capture armed; assert that the boolean feature's capture has `stages.len() > 0`. This is the canary that would catch the deviation #4 partial-coverage gap if it turns out load-bearing for the app side. **Self-canary**: the spec §8 test plan was already silent on this scenario, and adversary verification didn't surface it pre-implementation; running it in PR-VIZ-3b is therefore the cheapest way to get an empirical signal before adding more capture sites.
- Render selected stage's mesh (vertices + indices + per-tri color from labels) inside a mini Threlte canvas; reuse three.js `BufferGeometry` with `setAttribute('color', ...)` for label-coded shading.
- Out of scope for PR-VIZ-3b (defer to PR-VIZ-3c if needed): replay/scrub animation, side-by-side stage comparison, Cherchi sidecar diff overlay.
