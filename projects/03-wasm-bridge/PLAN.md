# 03 — WASM Bridge: Plan

## Milestones

### M1: Build Pipeline ✅
- [x] Create bridge crate skeleton
- [x] Set up wasm-pack build for Rust engine crates
- [x] Verify wasm-bindgen output (4 exported functions: init, process_message, get_feature_tree, get_mesh_json)
- [x] WASM binary: 1.9 MB (release, wasm-opt)
- [x] Feature-gated sketch-solver (native-solver feature) — excluded from WASM build because libslvs C++ can't compile to wasm32-unknown-unknown

### M2: Message Types ✅
- [x] Implement `UiToEngine` serialization (JSON via serde_json)
- [x] Implement `EngineToUi` serialization (JSON for metadata)
- [x] Round-trip tests: serialize → deserialize for all message variants (7 serde tests)

### M3: Web Worker Setup ✅
- [x] Worker script that loads WASM module (js/worker.js)
- [x] postMessage handler for incoming commands
- [x] postMessage sender for outgoing results
- [x] Worker error handling (onerror)
- [x] Main-thread bridge API (js/bridge.js) with Promise-based send()

### M4: Command Dispatch ✅
- [x] Deserialize UiToEngine in Worker
- [x] Dispatch to appropriate engine function
- [x] Handle all command variants (feature ops, selection, hover, undo/redo, save/load)
- [x] Undo/Redo wired to feature-engine undo/redo
- [x] SaveProject: serializes feature tree to JSON via file-format
- [x] LoadProject: deserializes, replaces tree, rebuilds
- [x] SolveSketch: wired to sketch-solver (native builds only)
- [x] Full sketch workflow: BeginSketch → AddEntity → Solve → FinishSketch
- [x] ExportStep: not yet wired (requires TruckKernel, not generic KernelBundle)
- [x] ReorderFeature and RenameFeature message dispatch wired
- [x] Tests: 21 total (7 serde + 8 dispatch + 3 engine state + 3 sketch workflow)

### M5: Result Callback ✅
- [x] Serialize EngineToUi in Worker (JSON via serde_json in wasm_api.rs)
- [x] postMessage to main thread (worker.js sends response)
- [x] Main thread message handler (bridge.js Promise-based API + event handlers)
- [x] Covered by M3 worker/bridge implementation

### M6: Mesh Transfer (partial) ✅
- [x] Expose RenderMesh vertex/normal/index data as TypedArray views (get_mesh_vertices, get_mesh_normals, get_mesh_indices)
- [x] Transfer via postMessage with Transferable objects (worker.js collectMeshes())
- [x] Copy-from-WASM-view pattern (views invalidated by memory growth, copy to standalone ArrayBuffers)
- [x] get_mesh_count() helper to enumerate features with meshes
- [x] ModelUpdated responses automatically attach typed array mesh data
- [ ] Browser integration test (requires browser environment)
- [ ] Benchmark: measure transfer time for various mesh sizes (requires browser environment)

### M7: libslvs WASM Module ✅
- [x] Emscripten build: em++ compiles vendored SolveSpace C++ + mimalloc to slvs.wasm (226KB)
- [x] Worker loads libslvs via fetch+blob dynamic import (bypasses Vite bundling)
- [x] JS bridge (slvs-solver.js): maps sketch entities/constraints to slvs C API structs on Emscripten heap
- [x] SolveSketchLocal worker message type: intercepts solve requests, calls libslvs, returns solved positions
- [x] Store integration: triggerSolve() sends sketch state to worker, handles SketchSolved response
- [x] DOF counter displayed in status bar
- [ ] Browser integration test (requires browser environment)

### M8: Error Propagation (partial) ✅
- [x] Install console_error_panic_hook (in wasm_api.rs init())
- [x] Convert EngineError → EngineToUi::Error (via BridgeError in dispatch.rs)
- [x] Worker-level error forwarding (onerror handler in worker.js)
- [x] Convert solver errors → SketchSolved response with error status (via JS bridge)
- [x] Tests: dispatch errors verified (undo empty, delete nonexistent, unimplemented)

### M9: Latency Benchmarking
- [ ] Measure command round-trip time (UI → Worker → engine → Worker → UI)
- [ ] Measure mesh transfer time for 10K, 100K, 1M triangle meshes
- [ ] Document baseline performance
- [ ] Identify bottlenecks if any

### M10: Server-mode prep — S0 render view off the wasm gate ✅
Spec: `specs/waffle_server_mode.md` §2.3 (P-A).
- [x] `src/render_view.rs`: renderable bodies, body metadata/naming, face and edge entries, ghost baking, legacy per-feature accessors — target-independent (2026-09-15)
- [x] `src/process.rs`: the `process_message` pipeline with injected clock and logger (2026-09-15)
- [x] `src/wasm_api.rs` reduced to bindings; exported JS API unchanged (20 functions, same signatures and imports)
- [x] Oracle `tests/render_view_parity.rs` + `.mjs`: rebuilt bundle census byte-identical to the pre-move bundle; native census structurally equal to the bundle's
- [x] **S1: request ids in the bridge envelope** (2026-09-16): every send leaves
      as `{id, msg}` and the worker echoes the id back on the answer, so
      `bridge.js` pairs through a `Map` keyed by id instead of shifting a FIFO
      array. `init`/`ready` stay bare (pre-handshake, before pairing starts).
      Needed by any transport that can reorder or push unsolicited frames
      (spec §2.3 S1), i.e. every host transport.
      - Fixes a live mispairing, not only a future one: the worker's
        `onmessage` is `async` and **awaits** the blob-URL re-import on the
        crash-restart path, so a message delivered during that await is
        processed and answered first, and FIFO handed its answer to the
        request that was still restarting.
      - Behavior change, deliberate: an `Error` that answers no id is the
        worker's `self.onerror` (an uncaught failure outside `processMessage`).
        It now rejects **every** in-flight send, where FIFO rejected whichever
        was oldest and left the rest hanging forever.
      - Oracle: `sketch-drawing-regression` + `agent-link`/`agent-parity`/
        `agent-reconnect` (27 passed), then the whole gui-fast tier
        (349 passed, 233 s). No Rust change, so the bundle is untouched.
- Found by S1, **fixed 2026-09-16**: `crates/wasm-bridge/js/{bridge,worker}.js`
  was a dead copy of the pre-SvelteKit bridge (last touched 2026-02-09,
  `f6a68d4d`), still doing `_pendingCallbacks.shift()` — nothing built or
  imported it, and S1 proved it had drifted. Both deleted; this sub-project's
  `CLAUDE.md` Key Files list now points at the real `src/*.rs` and at the live
  `app/src/lib/engine/{bridge,worker}.js`. **M3 above names `js/worker.js` and
  `js/bridge.js` as historical record only — those paths no longer exist.**
- [ ] Known, not fixed: `feature_engine::preview_mesh::decimate_mesh` orders output by `HashMap` iteration, so a native process's `preview_mesh` varies run to run (spec §2.7 H3)
- [ ] Known, not fixed: `cargo clippy -p wasm-bridge --target wasm32-unknown-unknown` flags the `thread_local!` initializer in `wasm_api.rs` (pre-existing; CI does not lint wasm32)

### M11: Server-mode S2 — the document session in Rust
Spec: `specs/waffle_server_mode.md` §2.3 S2 (P-A). Staged as atomic
checkpoints — C1, C2, then C3a/C3b/C3c (C3 split once it turned out the store
and the engine were minting tab ids independently), then C4. The browser path
is green at every one. **COMPLETE 2026-09-16.**
- [x] **C1 — the session type, wired to nothing** (2026-09-16): `src/session.rs`
      `DocumentSession` owns the document metadata, the tab list, every
      inactive tab's tree, assembly trees, per-tab preview meshes, a per-tab
      undo history, and a monotonic `revision`. 15 unit tests.
      `feature_engine::Engine` gained `take_history`/`set_history` (the stack
      was private with no accessor, so a host could not park it per tab).
- [x] **C2 — `EngineState` owns the session** (2026-09-16): `state.session` is a
      `DocumentSession`. `LoadProject` builds it from the loaded file
      (`from_document`: metadata, tab list, every inactive tab's tree) and
      `NewDocument` resets it. `ModelUpdated` gained
      `document: Option<DocumentInfo>` — `{id, name, display_unit, tabs,
      active_tab, revision}`, never a tab's tree. No existing message's
      signature changed.
      - **The document's name and display unit have one home now.** The
        `project_name`/`display_unit` String fields are gone from
        `EngineState`; `project_name()`/`display_unit()` read the session, and
        `set_project_name()`/`set_display_unit()` write it. A document that
        states no unit still reads `"mm"`, as the old field defaulted to.
      - `SaveDocument` **adopts** the `{document, tabs, active_tab}` the UI
        hands over (after a successful save, never on failure). While the JS
        store owns the tab bar, a save is the only message that tells the
        session about a rename, a new tab or a switch. C3 deletes this.
      - **Known gap, closed by C3**: `SwitchTab` carries a tree, not a tab id,
        so the session cannot tell which tab became active; its `active_tab`
        is the last one a load or a save reported. Nothing reads the session's
        tab bar until C4, so this is a stale field, not a wrong screen.
      - Oracle: new `tests/document_session.rs` (6 tests: fresh session, load,
        new, save-adoption, one-home display unit, and that every
        `ModelUpdated` reports the session). Whole `wasm-bridge` suite green
        across all targets; `clippy --all-targets -D warnings` and
        `fmt --check` clean. Release render-view parity (the CI step) green:
        `structure()` compares response TYPES, body metadata and counts, not
        the response bytes, so a new `ModelUpdated` field does not move it —
        the byte-for-byte `.mjs` golden does shift, and C3 regenerates it.
- C3 is staged in three commits: the tab bar (C3a), the tree payloads (C3b),
  the save payload (C3c). Splitting was forced by a real hazard — see C3a's
  first bullet.
- [x] **C3a — the tab bar moves to the session** (2026-09-16): `SwitchTab`
      takes a `tab_id` instead of a tree; new `AddTab`/`CloseTab`/`RenameTab`/
      `MoveTab`; `SetDocumentMeta` replaces `SetDisplayUnit`, so the document's
      metadata has one message that writes it. `OpenAssembly` and
      `OpenPartInContext` gained a `tab_id` — REQUIRED, not cosmetic: opening
      an assembly or a part in context IS a tab switch, and neither could be
      expressed by the tree-carrying `SwitchTab`. With a stale `active_tab` the
      next `stash_active` writes the live tree onto the WRONG tab, which is
      silent data loss.
      - **The store had to stop minting tab ids, and that is why C3 split.**
        `initDocumentState` rewrote legacy ids to fresh UUIDs and the engine's
        v3→v4 migration minted its own; `SaveDocument` handed the store's back,
        so the engine's were discarded. Harmless while the tree rode the wire —
        fatal once a message NAMES a tab. The store now takes the engine's ids
        at the only two moments the correspondence is provable: right after a
        `LoadProject` (both lists built from the same file, same order) and
        once at the doc-less `/` bootstrap (one tab on each side).
      - **Trap, caught only by a GUI test:** the first cut reconciled
        POSITIONALLY on every `ModelUpdated`. That corrupts identity the moment
        the two lists differ in order or membership — after a `moveTab` the
        names were reordered correctly while the ids were stamped onto their
        neighbours, and a session id the store had never seen appeared in the
        list. Reconcile only where the pairing is provable; never "when the
        lengths happen to match".
      - Two latent bugs fixed on the way: `stash_active` parked the undo
        history under the id of a tab `close_tab` had just removed (a history
        outliving its tab), and `OpenPartInContext` set `context_view` /
        `engine.context` BEFORE the switch that can fail, leaving an edit
        context behind on a refused tab id.
      - Oracles: 170 `wasm-bridge` tests (3 new in `document_session.rs`);
        clippy `--all-targets -D warnings`; `fmt --check`; both release parity
        tests green against regenerated `scenarios.json` + `golden.json` — the
        scenarios now open a FIXED two-tab document, because a static JSON
        scenario cannot name a session-minted tab id (its document id and
        timestamps are pinned too, or `scenarios_fixture_is_current` would be
        permanently stale); GUI 27 targeted + gui-fast 349, all passed.
      - **gui-fast covers NONE of this** — it is sketch/viewport/feature-tree
        only, and was green the whole time the tab ids were corrupt. The specs
        that actually exercise C3 must be named explicitly:
        `agent-tabs-viewport`, `agent-documents`, `assembly`,
        `document-format-seam`, `document-identity`, `auto-restore`.
- [x] **C3b — the session supplies the trees** (2026-09-16): `OpenAssembly` is
      `{tab_id}` and `OpenPartInContext` is `{tab_id, assembly_tab_id,
      instance_path}`. The session holds the tab's assembly AND every part /
      sub-assembly tree its instances reference, so nothing but names crosses
      the wire. New `EditAssembly{tab_id, assembly}` carries the panel's edits
      INTO the session — without it, `OpenAssembly` would evaluate a stale
      assembly, which is why it had to land in the same commit.
      - **Measured, in the parity fixture:** `scenarios.json` −373/+11 lines.
        That is the payload this removes, and in the app it is per assembly
        EVALUATION, not once. `golden.json` moved ±18 lines — the render census
        barely changed, which is the right shape: C3b removes redundant
        transport, it does not touch geometry.
      - `DocumentSession` gained `assembly(id)` (refuses a non-Assembly tab
        loudly, BEFORE the switch, so a bad id cannot leave the session on a tab
        it could not open) and `set_features(id, tree)`.
      - **Trap for the tests:** `part_trees()` serves the LIVE engine tree for
        the active tab and the stored copy for every other. So a fixture's part
        must be `state.engine.tree` when its tab is the open one; calling
        `set_features` on the active tab is overwritten by the next stash. An
        instance naming a tab the session lacks renders NOTHING — which
        surfaced as `export_step_…` failing with "no mesh data available for
        export" after I took a tab id from a throwaway `EngineState`.
      - **Tests that assert only "no errors" cannot catch this**: an assembly
        of empty parts reports no errors. The geometry assertions (placements
        at z = 10 mm, the bore radius, `feature_results.len()`) are what pin it.
      - Scoped refs in the in-context test now name the real assembly tab
        (`RefScope::in_assembly(asm_tab, …)`), not the old `"asm"` literal —
        otherwise the scoped-plane assertions keep passing while proving
        nothing about context resolution.
      - Oracles: 170 `wasm-bridge` tests; clippy `--all-targets -D warnings`;
        `fmt --check`; both release parity tests against regenerated fixtures;
        GUI 27 targeted (46 s, incl. the whole `assembly.spec.js` panel path
        through `EditAssembly`) + gui-fast 349 (218 s).
- [x] **C3c — the session composes the file** (2026-09-16): `SaveDocument` is a
      UNIT variant. No document, no tabs, no active tab: `to_document` composes
      the file from the session, so v4 §4 inv. 7 ("one writer") is now
      literally true. `DocumentSession::adopt` is deleted with the payload.
      `attach_preview_mesh` records each thumbnail into its own tab, so
      previews survive a save without the store holding the only copy.
      - **`id` and `created` flow DOWN, not up** — the opposite of C3a's tab
        ids, and the GUI assertions are what settled it: the storage record is
        keyed by the document's identity (v4 P2-5), and
        `document-format-seam` (`written.document.id === state.documentId`),
        `document-identity` and `git-provider` all pin that. So the HOST mints
        and latches identity, `SetDocumentMeta` gained `id`/`created`, and only
        `modified` is engine-stamped at save time.
      - **Bug found by the GUI run: saving DIRTIED the document.** The sync
        `SetDocumentMeta` answers with a `ModelUpdated`, and every
        `ModelUpdated` calls `scheduleAutoSave()` — so a completed save left a
        timer armed, `hasPendingAutoSave()` stayed true, and the agent link
        refused the next call with `UnsavedChanges`. `saveDocumentOrThrow`
        cancels BEFORE `buildDocumentJson`, so its cancel could not help.
        `buildDocumentJson` now restores the prior timer state instead of
        blanket-cancelling: a real edit made during a save still gets its
        autosave, and saving a clean document leaves it clean.
      - **Bug found by the GUI run, latent since C3b: solved placements stopped
        reaching storage.** `editAssembly` strips derived placements before
        sending and `OpenAssembly` never wrote them back; while the store's own
        tab copy was what got saved this was invisible, and C3c's payload-free
        save turned it into a saved assembly that reopens UNPLACED. `dispatch`
        now writes `view.placements` into the session's assembly tab after each
        evaluation (`set_assembly_placements`) — right on the merits, since the
        engine is what derives them.
      - **Seam, closed by C4:** between a rename and the next save,
        `ModelUpdated.document.name` is stale. Nothing reads it yet, and every
        save path pushes metadata first, so it is invisible today.
      - Oracles: 170 `wasm-bridge` tests; clippy `--all-targets -D warnings`;
        `fmt --check`; both release parity tests; GUI 27 targeted + gui-fast
        349; bundle fingerprint verified equal after the final rebuild.
- [ ] **Defect in the byte-for-byte parity layer (found during C3c, not
      introduced by it).** `render_view_parity.mjs`'s "a bundle's census must
      equal `golden.json` exactly" has been unfalsifiable since **C2**, when
      `ModelUpdated` gained `document`: `DocumentInfo.id` is a `Uuid` minted
      per `EngineState` (`engine_state.rs:72`, and again on `NewDocument`), so
      the serialized response differs every run. MEASURED: two census runs over
      the same unchanged bundle differ in 5 of 9 `response` hashes with all 9
      lengths identical (a 36-char UUID swapped for another). The structural
      test is unaffected (`structure()` drops these hashes), which is why every
      checkpoint stayed green. Fix: normalize or exclude `document.id` in the
      census so the byte layer means something again. Until then, a moved
      `golden.json` response hash with unchanged `n` is NOISE, not a signal.
- [x] **C4 — the store is a mirror** (2026-09-16): `documentTabs`,
      `activeTabId`, `documentName` and `documentDisplayUnit` are written ONLY
      by `mirrorSessionDocument(msg.document)`. The second `.waffle` parser is
      gone: `initDocumentState` no longer reads the name, the unit, the tab
      list or the active tab — it keeps the storage record id, the share link,
      and the document identity/`created` that are the host's (C3c). A2.1:
      "JS must treat the engine as authoritative and must not duplicate state
      in a way that can diverge."
      - `DocumentInfo` gained `created` and `assembly_tree`. The second is not
        optional polish: the moment the mirror owns the tab list, the assembly
        panel loses its only source for the OPEN tab's tree
        (`getAssembly`/`editAssembly` read `tab.kind.assembly`). Only the open
        tab's — an inactive tab's assembly is not display data.
      - **C4a/C4b could not be split.** Planned as two commits; the mirror
        necessarily drops per-tab assembly content, so "C4a alone" is a
        knowingly-broken state with four failing assembly specs. Landed as one.
      - **A mirror and its former writers are never compatible.** Deferring the
        removal of the local writes in `addTab`/`closeTab`/`renameTab`/
        `moveTab` and the bootstrap tab mint — on the reasoning that they were
        "safe-but-redundant duplicates" — cost **13 of 27** targeted specs. The
        tab bar diagnosed it literally: `each_key_duplicate … at indexes 1 and
        2`. Remove the writers in the SAME change that adds the mirror.
      - **`WASM crash detected` meant no such thing**: `collectCrashErrors`
        reports any page error under that banner, and these were Svelte
        keyed-each errors from `TabBar.svelte`. Taken at face value it sends
        you into the kernel.
      - **`created` must NOT be mirrored.** Rust's `rfc3339_js` drops the
        milliseconds, so echoing the engine's copy rewrites a stored
        `…:05.000Z` as `…:05Z` on every open. It is read from the file once, in
        `initDocumentState`. (Removing the parser AND declining to mirror it
        first left it `null` — the fix has to keep exactly one source.)
      - **Regression caught and fixed, not test-patched:** `openDocumentRecord`
        cleared `autoRestoreState` BEFORE its first await, so the restore
        dialog (which renders on that state) vanished mid-load. Invisible while
        `initDocumentState` filled the tab list synchronously; with a mirror the
        tab bar is genuinely empty for that window. The clear moved into the
        `finally`. The bootstrap-offer race it guarded cannot happen —
        `handoffPending` stops the offer being set during an explicit open.
      - Oracles: 170 `wasm-bridge` tests; clippy `--all-targets -D warnings`;
        `fmt --check`; both release parity tests; GUI 27 targeted; gui-fast
        345/349 — the 4 are CONTENTION FLAKE, all 41 tests of those four spec
        files pass in isolation (39.2 s) on the same bundle, and every
        signature is a timeout/missing-UI, never a wrong value.
- ~~Found by C1, to fix in C2/C3~~ **FIXED by C3a** (the per-tab stack is live:
  `switch_tab` parks the outgoing tab's history and restores the incoming
  tab's): **undo used to leak across tabs** —
  `SwitchTab` replaces `engine.tree` and `rebuild_from_scratch` clears only
  results, never `undo_stack`, so an `Undo` after a switch pops a command
  recorded against the tab you just left. The per-tab stack is a behavior
  fix, not only a relocation.
- Found by C1: the JS `switchTab` writes `kind.features` without checking the
  tab kind, stamping an empty `features` key onto an Assembly tab that is
  then serialized (harmless — `TabKind` ignores unknown keys — but the JS and
  Rust views of an Assembly tab differ). The session refuses to do this.
- Found by C1: `feature_engine::preview_mesh::PreviewMesh` and
  `file_format::PreviewMesh` are structurally identical, nominally distinct,
  and have no conversion anywhere. JS never noticed (both are JSON on the
  wire); `DocumentSession::set_preview_mesh` converts field-wise.

## Blockers

- ~~Depends on kernel-fork (M6 needs tessellation output)~~ RESOLVED
- libslvs Emscripten build (M7) may have platform-specific issues
- M6 mesh TypedArray views need browser testing environment

## Interface Change Requests

(None)

## Notes

- Never serialize mesh vertices as JSON — always use TypedArray views.
- The two-WASM-module approach (Rust + libslvs) is a short-term solution. Long-term: port solver to pure Rust.
- sketch-solver is feature-gated (`native-solver`) because libslvs C++ code can't compile to wasm32-unknown-unknown without Emscripten.
- Removed unused sketch-solver dependency from feature-engine crate.
- WASM build command: two-step process (wasm-pack can't do -Zbuild-std):
  1. `cargo +nightly build -p wasm-bridge --target wasm32-unknown-unknown --release --no-default-features -Zbuild-std`
  2. `wasm-bindgen target/wasm32-unknown-unknown/release/wasm_bridge.wasm --out-dir crates/wasm-bridge/pkg --target web --no-typescript`
