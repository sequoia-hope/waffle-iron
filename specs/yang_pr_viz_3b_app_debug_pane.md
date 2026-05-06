# PR-VIZ-3b — Svelte/Threlte app UI for Yang debug pane

**Status:** SPEC (FIP §3 full feature; modeling-affecting UI per §0). **Plan:** `/home/claude/.claude/plans/reactive-juggling-sloth.md` 0a. **Predecessor:** PR-VIZ-3a (`218d6e3`, 2026-05-05) shipped WASM bridge. **No Rust changes; no WASM rebuild.**

## 1. Goal
Svelte 5 component on top of PR-VIZ-3a's `__waffle.{setYangDebugCapture,getYangStages,clearYangDebugCaptures}` exports. Toolbar "Yang" button toggles a bottom-sheet pane mirroring AssayBrowser; the pane shows a feature dropdown → stage selector → mini three.js canvas rendering the selected stage's mesh. FeatureTree's ⚠ icon becomes a button; clicking it opens the pane and jumps to `failed_at_stage` if present. Capture is auto-armed on open, auto-disarmed + cleared on close. Default-off; zero overhead when closed. Yang 2025 §4 Fig 2 motivates per-stage exposure.

## 2. State additions (`app/src/lib/engine/store.svelte.js`)
4 `$state` + 5 exported helpers (~15 LOC). Mount conditionally in `+page.svelte`.
```javascript
let yangDebugVisible = $state(false);
let yangDebugFeatureId = $state(null);
let yangDebugStageIndex = $state(0);
let yangDebugCaptures = $state(new Map());  // feature_id → parsed FeatureStageCapture

export function getYangDebugVisible() { return yangDebugVisible; }
export function getYangDebugState() { return { visible: yangDebugVisible, featureId: yangDebugFeatureId, stageIndex: yangDebugStageIndex, captures: yangDebugCaptures }; }
export function toggleYangDebugPane() { /* arm/disarm via __waffle; flip visible */ }
export function hideYangDebugPane() { /* disarm + clear */ }
export function selectYangDebugFeature(featureId) { /* fetch + parse JSON; set initial stageIndex */ }
export function setYangDebugStageIndex(i) { yangDebugStageIndex = i; }
```
**Map reactivity (LOAD-BEARING):** when inserting a fresh capture, write `yangDebugCaptures = new Map(yangDebugCaptures).set(k, v)` — in-place `.set()` does NOT trigger Svelte 5 reactivity. Mirror `featureErrors` precedent at `store.svelte.js:46` (declaration) and L332-340 (the `newErrors = new Map(); ... ; featureErrors = newErrors;` reassignment pattern).

## 3. Component (`app/src/lib/ui/YangDebugPane.svelte`, NEW)
Structure:
- Visibility wrapper `{#if state.visible}…{/if}`
- Header: title "Yang Debug" + (mobile only) `use:bottomSheetResize` drag handle + close button (mirror `ab-header` at AssayBrowser.svelte:66-79)
- Body: feature `<select>` bound to `getFeatureTree().features` → stage `<select>` bound to `state.captures.get(state.featureId)?.stages` → mini canvas `<canvas bind:this={canvasEl}>`
- Mini canvas uses MANUAL three.js (mirror ThumbnailViewport.svelte:1-160, NOT nested Threlte Canvas):
  - `onMount`: `new THREE.WebGLRenderer({canvas: canvasEl})`, `Scene`, `PerspectiveCamera`, ambient + directional light, OrbitControls
  - `$effect`: rebuild `BufferGeometry` when `(state.featureId, state.stageIndex)` changes — parse `stages[stageIndex]`, set `position`/`index` attributes from raw `Float32Array`/`Uint32Array` arrays per CadModel.svelte:100-132 pattern, `computeVertexNormals()`, dispose previous geometry/material first
  - `onDestroy`: `renderer.dispose(); geometry?.dispose(); material?.dispose();`
- Failure marker: if `capture.failed_at_stage === stageIndex`, show red border + "FAILED HERE" label with `data-testid="yang-debug-failed-marker"`
- Per-stage info row: stage_tag, vertex count, triangle count
- CSS: mirror AssayBrowser.svelte:157-437 (right sidebar 300px desktop, 60vh bottom-sheet mobile via `@media (max-width: 768px)`); use existing CSS vars from `app.css:1-23` (`--bg-secondary`, `--border-color`, `--text-primary`, `--accent`)
- `data-testid` attributes: `yang-debug-pane`, `yang-debug-feature-select`, `yang-debug-stage-select`, `yang-debug-close`, `yang-debug-failed-marker`

## 4. WASM bridge wrappers
DIRECT `__waffle` calls — NOT message protocol via `bridge.send`. The PR-VIZ-3a exports are synchronous (`set_yang_debug_capture`, `get_yang_stages_json`, `clear_yang_debug_captures`); routing through `bridge.send` would add an async round-trip with no benefit. Wrappers live alongside the existing `__waffle` block at `store.svelte.js:469-507`. Add to that object:
```javascript
setYangDebugCapture: (b) => wasm.set_yang_debug_capture(b),
getYangStages: (id) => wasm.get_yang_stages_json(id),     // returns "null" if absent
clearYangDebugCaptures: () => wasm.clear_yang_debug_captures(),
```
The store's `selectYangDebugFeature(id)` calls `__waffle.getYangStages(id)`, parses (`JSON.parse`; the literal string `"null"` parses to JS `null`), and updates the Map via reassignment.

## 5. Toolbar integration (`app/src/lib/ui/Toolbar.svelte`)
Add ONE button after the Assay button at L650-652. Verbatim template to mirror:
```svelte
<button class="toolbar-btn" disabled={!ready} title="Yang Debug Pane"
    data-testid="toolbar-btn-yang-debug"
    onclick={() => toggleYangDebugPane()}>Yang</button>
```

## 6. FeatureTree error-icon click (`app/src/lib/ui/FeatureTree.svelte:409-411`)
Convert the static `<span class="error-indicator">` to a `<button class="error-indicator-btn">`:
```svelte
{#if featureErrors.get(feature.id)}
  <button class="error-indicator-btn"
          title={featureErrors.get(feature.id)}
          data-testid="feature-error-{i}"
          onclick={(e) => { e.stopPropagation(); selectYangDebugFeature(feature.id); if (!getYangDebugVisible()) toggleYangDebugPane(); }}>⚠</button>
{/if}
```
`e.stopPropagation()` is mandatory (the `data-testid` value `feature-error-{i}` is preserved verbatim from L410 to keep existing tests stable). Add minimal CSS for `.error-indicator-btn` to match the prior `.error-indicator` look.

## 7. Test plan (per FIP §4.2; `app/tests/gui/yang-debug-pane.spec.js`, NEW)
Helpers from `app/tests/gui/helpers/state.js` (`collectCrashErrors`, `expectNoAnyCrash`); pattern mirrors `extrude-second-depth.spec.js`. ALL 4 must FAIL on `218d6e3` (button/pane/handler don't exist).

1. **`test_yang_debug_toolbar_button_toggles_pane`** — click `[data-testid="toolbar-btn-yang-debug"]` → `[data-testid="yang-debug-pane"]` visible; click again → hidden.
2. **`test_yang_debug_capture_round_trip_via_ui`** — open pane (auto-arms) → create extrude → assert pane's feature dropdown shows the new feature → close pane (auto-disarms+clears).
3. **`test_yang_debug_canary_sketch_extrude_boolean`** (LOAD-BEARING per adversary-11 §6 + `feedback_adversary_recommendations_need_canary.md`) — open pane → sketch+rectangle → extrude → second extrude that overlaps (boolean) → assert second extrude's `capture.stages.length > 0`. **Empirically validates the `FinishSketch` partial-coverage gap from PR-VIZ-3a deviation #4.** If this test fails, escalate to team-lead — do NOT paper over by softening the assertion.
4. **`test_yang_debug_error_icon_click_jumps_to_failed_stage`** — drive a known-failing feature (e.g., a corpus case from F0031–F0040 cohort) → click ⚠ button in FeatureTree → assert pane opens + correct feature selected + stage selector shows `failed_at_stage` value.

All four use `expectNoAnyCrash(page, errors)` per CLAUDE.md.

## 8. FIP roles
| Sub-phase | Agent | Reads | Writes |
|---|---|---|---|
| 0a Spec | spec-writer-m | PR-VIZ-3a spec + validation; AssayBrowser, ThumbnailViewport, CadModel patterns; FIP §3; DoD §1 | this spec |
| 0b Test author | test-author-d (NEW per FIP §3.2 — disjoint from spec-writer-m and implementer-p) | this spec; existing GUI tests (esp. `extrude-second-depth.spec.js`); `helpers/state.js` | `app/tests/gui/yang-debug-pane.spec.js` (~120 LOC, 4 RED tests) |
| 0c Implement | implementer-p | this spec; tests from 0b; AssayBrowser/ThumbnailViewport/CadModel patterns; existing `__waffle` block | 5 files (4 modified + 1 new + mount in `+page.svelte`) |
| 0d Adversary | adversary-12 (NEW, full role rotation per `feedback_oracle_credibility_via_role_separation.md`) | all 0a–0c deliverables; PR-VIZ-3a validation as template | `docs/audits/pr_viz_3b_validation.md` (~120 LOC) |
| 0e Close-out | team-lead | all 0a–0d | memory updates + commit + push (no WASM rebuild needed) |
