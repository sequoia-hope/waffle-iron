# PR-VIZ-3a — in-memory Yang stage capture + WASM bridge API

**Status:** SPEC (FIP §3 full feature; NOT §8 fix-spec). **Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` 0a. **Predecessor:** PR-VIZ-1 (`c4ba32d`, 2026-05-05) shipped file-based per-stage OBJ dumps. **Successor (separate PR):** PR-VIZ-3b — Svelte/Threlte `YangDebugPane.svelte`.

## 1. Goal
Expose Yang per-stage probe data through the WASM bridge so PR-VIZ-3b's in-app debug pane can render any feature's pipeline stages inside the existing Threlte viewport. WASM has no filesystem, so PR-VIZ-1's disk-dump path cannot serve in-app integration; this PR adds an in-memory capture path that **coexists** with the file dumps (both run when both gates are on). New WASM exports are callable via `__waffle.*`. Yang 2025 §4 Fig 2 motivates per-stage exposure.

## 2. Data types (`crates/kernel/src/boolean/yang_integration.rs`)
```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StageMesh {
    pub stage_tag: String,   // "A","Bb","B","C","E_lod=Render","F.0",...
    pub vertices: Vec<f32>,  // flat [x,y,z,...]; lossy f64→f32 ok (viewer-grade)
    pub indices: Vec<u32>,   // 3 per tri
    pub labels: Vec<u32>,    // per-tri; encoding per stage (§4)
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FeatureStageCapture {
    pub feature_id: String,           // Uuid as string for JSON
    pub stages: Vec<StageMesh>,       // ordered by emission
    pub failed_at_stage: Option<usize>,
}
```

## 3. Capture lifecycle
Thread-local mirrors PR-VIZ-1's `CURRENT_CASE_ID` at `yang_integration.rs:1569-1572`:
```rust
std::thread_local! {
    static CAPTURE_BUFFER: std::cell::RefCell<Option<Vec<StageMesh>>> =
        const { std::cell::RefCell::new(None) };
}
pub fn start_yang_capture()                        // buffer = Some(Vec::new())
pub fn drain_yang_capture() -> Vec<StageMesh>      // returns + sets None
pub(crate) fn record_stage(tag: &str, verts: &[f32], idx: &[u32], labels: &[u32])  // no-op when None
```
Capture-off path is `RefCell::borrow().is_some()` early-return — no Vec allocation. `randomized_runner` is single-threaded (PR-VIZ-1 §4), so thread-local is correct.

## 4. Per-stage label semantics (verbatim from plan §0a)
| Stage | Labels meaning | Encoding |
|---|---|---|
| A | origin (A=0, B=1) | `Vec<u32>` |
| Bb | origin + inside | `Vec<u32>` packed `(origin << 1) \| inside` |
| B | origin + inside | `Vec<u32>` packed `(origin << 1) \| inside` |
| C | origin (no inside) | `Vec<u32>` |
| E | face_id | `Vec<u32>` |
| F.0–F.4 | face_id | `Vec<u32>` |

App-side decode: `origin = lab >> 1; inside = lab & 1` where applicable. Differs from PR-VIZ-1 CSV (split columns); CSV is unchanged.

## 5. WASM bridge API (`crates/wasm-bridge/src/wasm_api.rs`)
Three new `#[wasm_bindgen]` exports, mirror `get_face_data`/`get_mesh_json` JSON-getter at `wasm_api.rs:128-180`, each wrapped in `catch_unwind`:
```rust
#[wasm_bindgen] pub fn set_yang_debug_capture(enabled: bool)
#[wasm_bindgen] pub fn get_yang_stages_json(feature_id: &str) -> String  // "null" if absent
#[wasm_bindgen] pub fn clear_yang_debug_captures()
```
`get_yang_stages_json` returns `serde_json::to_string(&FeatureStageCapture)`. JS exposure: `__waffle.setYangDebugCapture / getYangStages / clearYangDebugCaptures`.

## 6. EngineState additions (`crates/wasm-bridge/src/engine_state.rs:9-22`, default at line 142-146)
```rust
pub yang_debug_capture_enabled: bool,                                  // default false
pub yang_debug_captures: std::collections::HashMap<String, FeatureStageCapture>,  // default empty
```

## 7. Dispatch hook (`crates/wasm-bridge/src/dispatch.rs`)
Wrap `UiToEngine::AddFeature` (lines 90-94), `UiToEngine::EditFeature` (lines 96-102), and the `Sketch`-finish `add_feature` call (line 85). Pattern:
```rust
if state.yang_debug_capture_enabled { kernel::start_yang_debug_capture(); }
let result = state.engine.add_feature(name, operation, kb);
if state.yang_debug_capture_enabled {
    let stages = kernel::drain_yang_debug_capture();
    let failed_at = state.engine.errors.iter().find(|(id,_)| /*just-added*/).map(|_| stages.len().saturating_sub(1));
    if let Some(fid) = /*just-added id*/ {
        state.yang_debug_captures.insert(fid.to_string(),
            FeatureStageCapture{ feature_id: fid.to_string(), stages, failed_at_stage: failed_at });
    }
}
result?;
```
Cross-crate façade in `crates/kernel/src/lib.rs` (mirror `set_yang_stage_dump_case_id` at lines 27-29):
```rust
pub fn start_yang_debug_capture()  { boolean::yang_integration::start_yang_capture() }
pub fn drain_yang_debug_capture() -> Vec<boolean::yang_integration::StageMesh> {
    boolean::yang_integration::drain_yang_capture() }
```
Probe sites add an unconditional `record_stage(...)` call **after** the existing PR-VIZ-1 `if let Ok(dump_dir) = std::env::var("YANG_STAGE_DUMP")` block at:
- `topology_extract.rs:779-805` (Stage C)
- `topology_extract.rs:1700-1717` (Stage A)
- `topology_extract.rs:1864-1888` (Stage Bb)
- `topology_extract.rs:2005-2022` (Stage B)
- `yang_integration.rs:1041-1072` (Stage E)
- `tessellation/mod.rs:4274-4378` (F.0–F.4 via `dump_stage_f_viz` helper at 4393)

Outer guard remains `YANG_CONFORMAL_PROBE=1`; in-memory capture is gated solely by `CAPTURE_BUFFER.is_some()`.

## 8. Test plan (per FIP §4.2)
1. **Kernel unit** (`yang_integration.rs::tests`, ~40 LOC): `start_yang_capture()` → 2× `record_stage` → `drain_yang_capture()` returns 2-element Vec with right tags/verts/indices/labels; second drain returns empty.
2. **WASM-bridge integration** (`crates/wasm-bridge/tests/pr_viz_3a_capture.rs`, NEW, ~60 LOC): `set_yang_debug_capture(true)` → dispatch boolean feature → `get_yang_stages_json(feature_id)` JSON has non-empty `stages[].stage_tag` and each entry has non-empty `vertices`/`indices`.
3. **Negative**: capture off (default) → after dispatch → `get_yang_stages_json("any-id")` = `"null"`.
4. **Probe-off identity**: existing `yang_trace_f0002` passes byte-clean; PR-VIZ-1 file-dump behavior unchanged.

**Anchor pre-verification canary** (per `feedback_anchor_before_fix.md`): even though §8.1 calls `start_yang_capture` directly, implementer-o adds `eprintln!("[viz3a-canary]")` inside `start_yang_capture` and confirms it fires **before** writing the rest. Required because §8.2 reaches `start_yang_capture` through three layers (dispatch → kernel façade → boolean module) — no validated chain yet.

## 9. FIP roles
| Sub-phase | Agent | Writes |
|---|---|---|
| 0a Spec | spec-writer-l (NEW) | this spec |
| 0b Test author | test-author-c (NEW per FIP §3.2; ≠ spec-writer-l, ≠ implementer-o) | RED kernel test + NEW `crates/wasm-bridge/tests/pr_viz_3a_capture.rs` |
| 0c Implement | implementer-o | capture infra + 10 probe-site `record_stage` calls + EngineState fields + dispatch hook + 3 wasm exports + façade |
| 0d Adversary | adversary-11 (NEW, full role rotation per `feedback_oracle_credibility_via_role_separation.md`) | `docs/audits/pr_viz_3a_validation.md` |
| 0e Close-out | team-lead | WASM rebuild + memory + commit + push |
