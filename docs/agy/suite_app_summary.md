# Suite-App Summary (`agy/suite-app`)

**Worktree:** `/home/claude/wt/agy-suite-app`  
**Branch:** `agy/suite-app` (based on `246757b1`)  
**Scope:** Waffle Iron web-app fixes required to run as a static micro-app under `BASE_PATH=/apps/waffle` inside Boardroom (`AGY_BRIEF_SUITE_APP.md`), all 8 independent review fixes (`REVIEW_SUITE_APP_1.md`), all 4 Review 2 GUI fixes (`REVIEW_SUITE_APP_2.md`), all 3 Review 3 GUI fixes (`REVIEW_SUITE_APP_3.md`), and the final Review 4 GUI fix (`REVIEW_SUITE_APP_4.md`). Zero kernel or WASM crates were modified (`docs/yang_deviations.md` and `docs/audits/yang_2025_audit.md` untouched).

---

## 1. Deliverables & Review Fixes Built

### Round 1 — Initial Deliverables (`15d77fa6`..`db0e0d79`)

1. **A1 — Dev-Server API Clients and Vite Plugin Respect `base` (`15d77fa6`):**
   - `app/src/lib/engine/examplesApi.js` (`:10-13`, `:65-86`): prefixed `DEV_BASE = `${base}/api/examples`` (alongside `STATIC_BASE = `${base}/examples``).
   - `app/src/lib/engine/testCaseApi.js` (`:6-8`): prefixed `BASE = `${base}/api/test-cases``.
   - `app/src/lib/engine/assayCaseApi.js` (`:7-29`): prefixed `DEV_BASE = `${base}/api/assay-cases`` and required `(devRes.headers.get('content-type') || '').includes('application/json')` in `fetchWithFallback` so a static SPA host's `200 text/html` fallback falls through to `${base}/assay/*`.
   - `app/src/lib/vite-plugins/testCaseApi.js` (`:8-21`, `:67`, `:191`, `:257`, `:325`): added `normalizeBasePath(...)` and mounted all four Connect middlewares under `${basePrefix}/api/{test-cases,examples,assay-cases,sketch-cases}`.

2. **A2 — Relative Web App Manifest Paths (`9a923095`):**
   - `app/static/manifest.json` (`:4-5`): changed `"start_url"` to `"./"` and added `"scope": "./"`, resolving relative to the manifest URL (`/apps/waffle/manifest.json` -> `/apps/waffle/`).

3. **A3 — Base-Path Static Build Check (`850431df`, hardened in `76e0f95f`):**
   - `app/svelte.config.js` (`:3-11`): reads `const outDir = process.env.WAFFLE_BUILD_OUT_DIR || 'build'` and passes `pages: outDir, assets: outDir` to `@sveltejs/adapter-static`.
   - `app/scripts/check-base-path-build.mjs` (`scanBuildOutput` at `:96`, `runBasePathBuildCheck` at `:192`): builds directly into an isolated `mkdtempSync(join(tmpdir(), 'waffle-base-path-build-'))` directory via `WAFFLE_BUILD_OUT_DIR` (never touching `app/build`), never calls `process.exit` inside `try` so `finally` always cleans up `tmpRoot`, and scans every non-fixture text output file (`76` HTML/JS/CSS/JSON/SVG files) for root-absolute `/_app`, `/pkg`, `/api`, `/examples`, `/assay`, `/icons`, `/manifest.json`, app route strings (`/home`, `/doc`, `/open`, `/view`, `/agent`), HTML `href`/`src`/`action`, CSS `url(/...)`, and `manifest.json` `start_url`/`scope` outside `BASE_PATH`.
   - `app/src/lib/engine/bridge.js` (`:90`): replaced `wasmUrl.indexOf('/pkg/')` with `wasmUrl.replace(/\/pkg\/[^/]*$/, '')` so Vite's minifier does not emit a literal `"/pkg/"` string into client chunks.
   - **Recommended CI placement:** `scripts/test.sh`'s `fast` tier (`run_rust_fast`) runs zero `npm` builds, so `check-base-path-build.mjs` was not wired into `scripts/test.sh fast`. It belongs in `scripts/test.sh` `run_gui_fast` (shard `1/*`) and `.github/workflows/deploy.yml`.

4. **A4 — `DocumentStore` Revisions, Conflict Refusal, `watch()`, and Conflict UI (`849ad2da`):**
   - `app/src/lib/storage/types.js`: `get(id) -> { doc, rev } | null`, `put(doc, { base }) -> { rev } | { conflict: { theirs_rev } }`, optional `watch(fn) -> () => void`.
   - `app/src/lib/storage/indexeddb.js`: uses `gitBlobSha1(doc.json)` (`git-blob-sha1:<40 hex>`, WebCrypto SHA-1 computed *before* opening the single `readwrite` IndexedDB transaction so Promise yields never auto-commit the transaction) and `BroadcastChannel('waffle-iron-storage')` `watch(fn)`.
   - `app/src/lib/storage/git-provider.js` & `app/src/lib/storage/github.js`: explicit `options.base` check before any remote write, plus `BroadcastChannel`-backed `watch(fn)`.
   - `app/src/lib/ui/DocumentConflictDialog.svelte` & `app/src/lib/engine/store.svelte.js`: `reloadAfterConflict()` (`:7921`) and `saveConflictAsCopy()` (`:7980`), plus `if (e.defaultPrevented) return;` on window `Ctrl+S` in `app/src/routes/+page.svelte` (`:125`).

---

### Round 2 — Independent Review Fixes (`REVIEW_SUITE_APP_1.md`, 1 Atomic Commit + Targeted GUI Spec Each)

1. **Review Item 1 — Prevent agent-save / user-save lock-inversion deadlock (`1d2ead87`):**
   - `app/src/lib/engine/store.svelte.js` (`composeSaveSnapshot` `:10471`, `commitPendingAutoSave` `:10497`, `autosaveNow` `:10510`, `saveToProvider` `:10683`): composes `.waffle` JSON (`buildDocumentJson`) *outside* `saveChain` with monotonic `saveComposeSeq` / `lastSavedComposeSeq` ordering so stale snapshots never overwrite newer ones, queueing only draft/provider storage I/O inside `saveChain`.
   - `app/src/lib/agent/executor.js` (`committed` `:494-498`): passes `composeLock: withAgentLock` to `commitPendingAutoSave` so `engineLock` is held only during engine composition and released *before* awaiting `saveChain`.
   - **Targeted spec:** `app/tests/gui/agent-save-deadlock.spec.js`.

2. **Review Item 2 — Git rename writes new path with null-base check before deleting old path (`7ba5adf5`):**
   - `app/src/lib/storage/git-provider.js` (`put` `:284-398`): verifies the rename target path does not already exist on the remote (`currentTarget` null-base check — refusing before any write so unindexed files at the target name are never overwritten), writes the new path first with `existingSha = null`, commits `.waffle-index.json`, and deletes the old path last.
   - **Targeted spec:** `app/tests/gui/git-rename-safety.spec.js`.

3. **Review Item 3 — Compare-and-swap merge and retry on `.waffle-index.json` (`26acb3eb`):**
   - `app/src/lib/storage/git-provider.js` (`#indexSha`, `#mutateIndex` `:489-578`, used by `put` `:371` and `delete` `:413`): tracks the loaded `.waffle-index.json` revision (`#indexSha`), re-checks the remote index revision before writing, and on pre-write mismatch or HTTP `409`/`412`/`422`/GitLab `400` conflict re-reads the latest remote index, re-applies only the caller's entry mutation (`mutator`), and retries with the remote `sha` as base (up to `MAX_INDEX_CAS_ATTEMPTS = 6`) instead of clobbering concurrent entries/renames/deletions.
   - **Targeted spec:** `app/tests/gui/git-index-cas.spec.js`.

4. **Review Item 4 — Keep existing git entry filenames stable and never delete shared paths (`6fd32af5`):**
   - `app/src/lib/storage/git-provider.js` (`put` `:258-268`, `:381-398`, `delete` `:417-423`): preserves `existing.filename` whenever `existing.name === name` (disambiguating `-${id8}.waffle` only on new document creation or when *that* document is renamed), and checks the committed index before `deleteFile` in both `put` rename cleanup and `delete(docId)` so a path still referenced by another index entry is never deleted.
   - **Targeted spec:** `app/tests/gui/git-filename-stability.spec.js`.

5. **Review Item 5 — Real compare-and-swap on GitLab (`9473006a`):**
   - `app/src/lib/storage/git/hosts.js` (`gitlab.getFile` `:216-235`, `gitlab.putFile` `:237-263`, `gitlab.deleteFile` `:265-282`; also `github.getFile` `:110` and `gitea.getFile` `:326`): uses GitLab's `last_commit_id` (falling back to `blob_id`), throws `GitHostError(..., 'api_error')` rather than inventing `'exists'` if a metadata response lacks any revision identifier, and sends `last_commit_id: sha` on GitLab `PUT`/`DELETE` updates so concurrent branch commits are rejected server-side (`HTTP 400`, mapped to `{ conflict: { theirs_rev } }` in `git-provider.js:343`).
   - **Targeted spec:** `app/tests/gui/gitlab-cas.spec.js`.

6. **Review Item 6 — Refuse silent import overwrite and fix UUID cross-key conflict (`2c72862d`):**
   - `app/src/lib/engine/store.svelte.js` (`loadProject` `:12239-12279`, `openDocumentRecord` `:8204-8217`, `replaceConflictedDocument` `:8044`): ignores foreign-keyed `/open` read-only link cache records (`existing.doc.link && existing.doc.id !== fileUuid`) on UUID lookup, targets `ownExisting.doc.id` when an older 8-char-keyed own record carries `fileUuid`, and initializes imports into an existing own record with `base = null` (`importedExistingTargetId`) so saving conflicts until the user explicitly chooses **Reload**, **Save as a copy**, or **Replace** (`[data-testid="conflict-replace-btn"]` in `DocumentConflictDialog.svelte:79-89`, rendered *only* when `conflict.fromImport && conflict.theirsRev !== null`).
   - `app/src/lib/agent/tools/documents.js` (`:141-171`), `app/src/lib/agent/documents.js` (`:182-254`), and `relay/src/waffle_mcp_relay/agent-tools.manifest.json`: added `replace` (`default: false`) to `document_import`; refuses importing into an existing own record with `SaveFailed` unless `replace: true` *and* the user confirms via `window.confirm(...)` (`UserDeclined` if dismissed); updated `document-identity.spec.js` and `agent-documents.spec.js`.
   - **Targeted spec:** `app/tests/gui/import-conflict.spec.js`.

7. **Review Item 7 — `check-base-path-build.mjs` `WAFFLE_BUILD_OUT_DIR` isolation, `finally` cleanup, and widened scan (`76e0f95f`):**
   - Wired `WAFFLE_BUILD_OUT_DIR` in `app/svelte.config.js`, removed all `process.exit` calls inside `try` in `app/scripts/check-base-path-build.mjs`, and widened `scanBuildOutput` to catch `/examples`, `/assay`, `/icons`, `/manifest.json`, `/home`, `/doc`, `/open`, `/view`, `/agent`, and CSS `url(/...)`.
   - **Targeted spec:** `app/tests/gui/base-path-build-scanner.spec.js`.

8. **Review Item 8 — Pre-branch draft restore, cross-session `localFallback`, home rename notice, and `openFromLocator` conflict coherence (`bb042fe2`):**
   - `app/src/lib/engine/store.svelte.js` (`resolveDraftBase` `:10159`, used in `restoreAutoSave` `:10190` and `reopenDocumentById` `:10579`): treats pre-branch drafts lacking `baseRev` (`draft.baseRev === undefined`) as adopting the stored revision at restore time only when `found.doc.json === draft.json`, else `null` so diverged drafts conflict rather than overwriting.
   - `app/src/lib/storage/indexeddb.js` (`:58`, `:345`) & `app/src/lib/engine/store.svelte.js` (`localFallbackSeedPromise` `:559`, `:9970-9988`, `putRecord` `:10627-10680`): stamps offline fallback writes with `localFallback: true`, seeds `localFallbackState` at session open from any leftover fallback record so subsequent sessions update it cleanly, and surfaces `documentConflict` if a local fallback write still conflicts.
   - `app/src/routes/home/+page.svelte` (`:16`, `:102-112`, `:153-165`): renders a dismissible `[data-testid="home-conflict-notice"]` alert banner on rename conflicts.
   - `app/src/lib/storage/open-link.js` (`:111-129`): re-reads `local.get(id)` when `local.put` conflicts and returns the winning stored record's coherent `{ id, json, link, rev, documentName }` instead of pairing the newly fetched text with `theirs_rev`.
   - **Targeted spec:** `app/tests/gui/storage-conflict-edge-cases.spec.js`.

---

### Round 3 — GUI Run Fixes (`REVIEW_SUITE_APP_2.md`, 1 Atomic Commit per Failure Item)

*(Follows reviewer commit `5841b34a`, which fixed the `collectCrashErrors` / `expectNoAnyCrash` import path to `./helpers/state.js` in three specs.)*

1. **Review 2 Item 1 — `document-conflict.spec.js:83` (`HTTP 405` on `DELETE git-zero.waffle`) (`13fa08b4`):**
   - **Root cause:** Updating `docId` from `'Git Zero'` (`git-zero.waffle`) to `'Git One'` (`git-one.waffle`) is a legitimate document rename (`existing.name !== name`), so `GitProvider.put` (`app/src/lib/storage/git-provider.js:284-299, 378-396`) rightly writes `git-one.waffle`, updates `.waffle-index.json`, and sends `DELETE git-zero.waffle`. The in-memory GitHub Contents API mock in `app/tests/gui/document-conflict.spec.js` only handled `GET` and `PUT`, returning `405` on `DELETE`.
   - **Fix:** Added `DELETE` handling (with `sha` CAS verification), `PUT` `sha` CAS verification, monotonic revision SHAs, and post-rename assertions (`!files.has('git-zero.waffle') && files.has('git-one.waffle')`) in `app/tests/gui/document-conflict.spec.js` (`:205-322`).

2. **Review 2 Item 2 — `import-conflict.spec.js:185` (`refusedNoFlag.isError === undefined`) (`12b47891`):**
   - **Root cause:** `DOCUMENT_COMMANDS.document_import` (`app/src/lib/agent/documents.js:206-216`) refuses silent overwrite by throwing `ToolFailure` via `fail('SaveFailed', ...)` without modifying storage (`gui-import-conflict-Import-2e238-ires-user-confirmed-replace-chromium.error-context.md` confirmed `"Stored Newer"` was never imported). `executeTool` (`app/src/lib/agent/executor.js:478-481`) is the layer that catches `ToolFailure` and returns `{ isError: true, structuredContent: { error } }`; `import-conflict.spec.js` called `DOCUMENT_COMMANDS.document_import` directly and caught `.catch((e) => e)`, serializing the raw `Error` instance (`isError === undefined`). Additionally, seeding `DOC_ID` before `page.goto('/')` leaves the startup restore prompt open (`getDocumentLoadBusyReason() === 'restoring'`), which `executeTool`'s load-gate check refuses with `EngineBusy` until dismissed.
   - **Fix:** Dismissed the startup restore prompt via `window.__waffle.discardAutoSave()` (preserving the stored IndexedDB record) and routed all three `document_import` calls through `window.__waffleAgentExecutor.executeTool('document_import', ...)` in `app/tests/gui/import-conflict.spec.js` (`:50-83`, `:211-269`).

3. **Review 2 Item 3 — `import-conflict.spec.js:282` (`window.__waffle.getDocumentConflict is not a function`) (`e207f4fd`):**
   - **Root cause:** `store.svelte.js:7911` exported `getDocumentConflict()` and `DocumentConflictDialog.svelte` rendered the `"Document Already Exists in Storage"` dialog on screen, but `window.__waffle` omitted `getDocumentConflict`.
   - **Fix:** Exposed `getDocumentConflict: () => (documentConflict ? { ...documentConflict } : null)` on `window.__waffle` in `app/src/lib/engine/store.svelte.js` (`:2043`).

4. **Review 2 Item 4 — `storage-conflict-edge-cases.spec.js:88` and `:167` (`waitForFunction` 15 s timeouts) (`e04e3f24`):**
   - **Root cause (`:88`):** `seedRawLegacyDraft` passed `modified: 3000` / `6000` (`1970-01-01`), and `initEngine()` ran `pruneDrafts()` (`DRAFT_MAX_AGE_MS = 30 days`) right *after* `findStartupRestore()`, deleting the offered draft before `restoreAutoSave()` read it (returning `false` and clearing `autoRestoreState`). Line `125` also queried `[data-testid="toast"]` instead of `ToastContainer.svelte`'s `[data-testid="toast-container"]`.
   - **Root cause (`:167`):** `window.__waffle.commitPendingAutoSave` was not exposed on `window.__waffle` (throwing `TypeError` inside the unawaited async IIFE), and `setProjectName()` sends `SetDocumentMeta` asynchronously to the worker so `autoSaveTimer` is only armed once `ModelUpdated` replies.
   - **Fix:** Moved `await pruneDrafts().catch(() => {})` *before* `findStartupRestore()` (`app/src/lib/engine/store.svelte.js:1291`), exposed `commitPendingAutoSave` on `window.__waffle` (`:2049`), used `Date.now()`-relative timestamps and `[data-testid="toast-container"]` in `:88`, and awaited `hasPendingAutoSave() === true` before `commitPendingAutoSave()` directly inside awaited `page.evaluate` calls in `:167` (`app/tests/gui/storage-conflict-edge-cases.spec.js:88-279`).

---

### Round 4 — GUI Re-Run Fixes (`REVIEW_SUITE_APP_3.md`, Regression First, 1 Atomic Commit Each)

1. **Review 3 Item 1 (Regression) — `document-format-seam.spec.js:55` (`Expected: "in", Received: "mm"`) (`d477b485`):**
   - **Root cause:** `openDocumentRecord` (`app/src/lib/engine/store.svelte.js:8178-8186`) previously ran `initDocumentState(docId, parsed, ...)` *before* `await loadProject(json, { silent: true })` (unlike the File→Open path at `:12283-12292`), setting `activeDocId = 'seam001'` and `documentCreated = T0` while the engine's `LoadProject` rebuild was still in flight and `documentDisplayUnit` was still `'mm'`. Commit `e04e3f24` widened that window by awaiting `pruneDrafts()` unconditionally between `engineReady = true` (`:1277`) and `window.__waffle` installation (`:1342`), allowing `loadPendingDocument()`'s `setInterval` to enter `openDocumentRecord` before `window.__waffle` existed and exposing the half-loaded state (`ModelUpdated` arrived ~50 ms later in the failure snapshot with `button "in"` and both tabs).
   - **Fix:** In `app/src/lib/engine/store.svelte.js`, incremented `documentSessionEpoch++` up front (invalidating outgoing saves), awaited `loadProject(json, { silent: true })` *before* calling `initDocumentState(docId, parsed, ...)` (`:8177-8195`), cleared `lastRebuildWarnings = new Set()` right before `sendRebuild({ type: 'LoadProject' })` (`:12223`, `:12284`) instead of inside `initDocumentState`, and only awaited `pruneDrafts()` in `initEngine()` when startup restore runs (`!handoffPending && restorePolicy !== 'never'`, `:1294-1307`).

2. **Review 3 Item 2 — `import-conflict.spec.js:271` (`Expected: "Legacy Updated", Received: "Legacy Own"`) (`dbf3b5d2`):**
   - **Root cause:** `replaceConflictedDocument()` (and `saveConflictAsCopy()`) in `app/src/lib/engine/store.svelte.js` cleared `documentConflict = null` *before* `await saveDocumentOrThrow({ allowEmpty: true })`. Clicking `[data-testid="conflict-replace-btn"]` hid `DocumentConflictDialog` in one microtask while `buildDocumentJson()` and `IndexedDBStore.put` were still in flight (`Saved` toast appeared ~15 ms later in the failure snapshot).
   - **Fix:** Kept `documentConflict` set (with `DocumentConflictDialog`'s `busy` state disabling buttons) until `await saveDocumentOrThrow({ allowEmpty: true })` commits the replacement or copy to storage (`app/src/lib/engine/store.svelte.js:8019-8077`).

3. **Review 3 Item 3 — `storage-conflict-edge-cases.spec.js:88` (`Test timeout of 60000ms exceeded while setting up "context"`) (`7f9d3579`):**
   - **Root cause:** Under 6-worker load alongside `examples.spec.js` (Gravel bike / Eiffel Tower), Playwright's worker-scoped Chromium process stalled past the default 60 s timeout inside `browser.newContext` when preceding specs left live WASM/WebGL renderers open (the exact failure mode documented in `app/tests/gui/examples.spec.js:39-70`). Additionally, seeding pre-branch drafts on `/home` or live `/` ran alongside `+layout.svelte`'s `initEngine()` / `tabKey()` and `pagehide`'s `rememberTabKey()` / `flushAutoSave()`, and `base-path-build-scanner.spec.js:88` mutated `process.env.TMPDIR` and `process.env.PATH` inside the shared worker.
   - **Fix:** Configured `timeout: 180000` on multi-load describe blocks (`storage-conflict-edge-cases.spec.js`, `import-conflict.spec.js`, `document-conflict.spec.js`), added `afterEach(... page.goto('about:blank'))` on passing tests across all new suite-app browser specs to reclaim WASM/WebGL renderers immediately, seeded pre-branch drafts in `storage-conflict-edge-cases.spec.js:88` on `/manifest.json` (same origin, no SvelteKit layout), and passed `{ tmpBaseDir, env }` to `runBasePathBuildCheck()` (`app/scripts/check-base-path-build.mjs:192-215`) so `base-path-build-scanner.spec.js` never touches worker `process.env` or boots an unused browser.

---

### Round 5 — Final GUI Re-Run Fix (`REVIEW_SUITE_APP_4.md`, 108/109 Passed on `b60558fb`)

1. **Review 4 Item 1 — `import-conflict.spec.js:276` at line `358` (`Expected: null, Received: undefined`) (`4426f4a3`):**
   - **Root cause:** Tracing the app code path end to end confirmed the target record key and base revision were completely sound:
     - Both `/doc/[id]` (`app/src/routes/doc/[id]/+page.svelte:17-31`) and File→Open (`loadProjectFromFile` in `app/src/lib/engine/store.svelte.js:12258-12300`) call `provider.get(LEGACY_UUID)` (`IndexedDBStore.get` at `app/src/lib/storage/indexeddb.js:244-275`), which resolves the 8-char legacy record `{ doc: { id: 'legacy01', ... }, rev }` while ignoring foreign-keyed `link` caches (`store.svelte.js:12271-12274`), setting `targetId = 'legacy01'` with `base = null`.
     - Clicking **Replace** (`replaceConflictedDocument` at `store.svelte.js:8048-8066`) sets `activeDocRev = conflict.theirsRev` and awaits `saveDocumentOrThrow({ allowEmpty: true })`, updating primary key `'legacy01'` in place — which lines `354-356` of `import-conflict.spec.js` verified (`legacyAfter.json` had `name: 'Legacy Updated'` and `display_unit: 'cm'`).
     - Line `358` was reached for the first time in Round 4 (after Round 2 fixed `getDocumentConflict` at `:282` and Round 3 fixed `replaceConflictedDocument` dialog lifetime at `:355`), where `const duplicateAtUuid = await getDocumentFromDB(page, LEGACY_UUID); expect(duplicateAtUuid).toBeNull();` failed with `Received: undefined` because `getDocumentFromDB` (`app/tests/gui/helpers/waffle-test.js:167-187`, `@returns {Promise<object|undefined>}`) wraps raw `IDBObjectStore.get(docId)`, which resolves `undefined` (not `null`, unlike `IndexedDBStore.get`) when no record exists under key `LEGACY_UUID`.
   - **Fix:** Updated `app/tests/gui/import-conflict.spec.js` (`:357-366`) to assert `expect(duplicateAtUuid).toBeUndefined()` (matching `IDBObjectStore.get` for an absent primary key) and added a positive `getStore().get(LEGACY_UUID)` check asserting `{ doc: { id: 'legacy01' } }` with `name: 'Legacy Updated'`.

---

## 2. Verification Commands & Exact Output Lines Observed

1. **Static gates across all GUI specs and icons** (`node scripts/check-gui-swallowed-waits.mjs && node scripts/check-icons.mjs`):
   ```text
   check-gui-swallowed-waits: ok (279 files)
   check-icons: ok (112 names)
   ```

2. **Relay tool manifest parity & full Python relay test suite** (`PYTHONPATH=relay/src:relay/tests /home/claude/workspace/relay/.venv/bin/pytest relay/tests` after `node app/scripts/gen-agent-manifest.mjs`):
   ```text
   ======================= 125 passed, 1 skipped in 35.98s ========================
   ```

3. **Booked foreground `BASE_PATH=/apps/waffle` static build & widened scanner check after Review 3 fixes** (`timeout 5400 ~/.claude/bin/reserve -c 4 --here -l "base-path build check review 3" -- node app/scripts/check-base-path-build.mjs /apps/waffle`):
   ```text
   ✓ built in 11.81s
   > Using @sveltejs/adapter-static
     Wrote site to "/tmp/waffle-base-path-build-EpffKZ/out"
     ✔ done
   check-base-path-build: ok (BASE_PATH=/apps/waffle, 76 files checked)
   ```

---

## 3. Exact List of Playwright GUI Specs to Re-Run

### 3.1 Targeted Re-Run for Review 4 Fix (1 spec)

Only `app/tests/gui/import-conflict.spec.js` changed after `b60558fb` (where the other 24 non-relay specs — 108/109 tests — passed):
- `app/tests/gui/import-conflict.spec.js` *(Review 4 fix: `IDBObjectStore.get` `undefined` assertion + `getStore().get(LEGACY_UUID)` check at `:357-366`)*

Copy-pasteable booked re-run command from `/home/claude/workspace`:
```bash
(cd app && timeout 5400 ~/.claude/bin/reserve -c 2 --here -l "suite-app review-4 rerun" -- \
  npx playwright test --workers=1 tests/gui/import-conflict.spec.js)
```

*(If the 26th relay-backed spec `tests/gui/agent-documents.spec.js` has not been run yet, run it separately with `--workers=1` as shown in Section 3.2.)*

### 3.2 Full Suite-App Regression Set (25 non-relay specs + 1 relay spec)
- `app/tests/gui/document-conflict.spec.js` *(new in Round 1)*
- `app/tests/gui/document-storage.spec.js`
- `app/tests/gui/document-identity.spec.js` *(updated in Item 6)*
- `app/tests/gui/document-routing.spec.js`
- `app/tests/gui/auto-restore.spec.js`
- `app/tests/gui/home-page.spec.js`
- `app/tests/gui/git-provider.spec.js`
- `app/tests/gui/github-storage.spec.js`
- `app/tests/gui/open-from-link.spec.js`
- `app/tests/gui/agent-documents.spec.js` *(relay-backed; updated in Item 6)*
- `app/tests/gui/agent-document-load-gate.spec.js`
- `app/tests/gui/examples.spec.js`
- `app/tests/gui/test-case-browser.spec.js`
- `app/tests/gui/layout-overflow.spec.js`

#### New Targeted Regression Specs from Review 1 (8 specs, 1 per review item)
- `app/tests/gui/agent-save-deadlock.spec.js` *(Item 1: agent lock + `saveChain` ordering)*
- `app/tests/gui/git-rename-safety.spec.js` *(Item 2: write-before-delete + unindexed target null-base check)*
- `app/tests/gui/git-index-cas.spec.js` *(Item 3: `.waffle-index.json` CAS merge & retry)*
- `app/tests/gui/git-filename-stability.spec.js` *(Item 4: stable entry filenames + shared-path delete guard)*
- `app/tests/gui/gitlab-cas.spec.js` *(Item 5: GitLab `last_commit_id` / `blob_id` CAS)*
- `app/tests/gui/import-conflict.spec.js` *(Item 6: import conflict + Replace + 8-char/linked UUID lookup)*
- `app/tests/gui/base-path-build-scanner.spec.js` *(Item 7: `WAFFLE_BUILD_OUT_DIR` isolation + widened path/CSS scan)*
- `app/tests/gui/storage-conflict-edge-cases.spec.js` *(Item 8: pre-branch draft `baseRev`, `localFallback`, home rename notice, `openFromLocator`)*

#### Additional Related Specs (4 specs)
- `app/tests/gui/document-context-menu.spec.js`
- `app/tests/gui/document-format-seam.spec.js`
- `app/tests/gui/git-links.spec.js`
- `app/tests/gui/sketch-corpus-api.spec.js`

Copy-pasteable booked commands from `/home/claude/workspace`:
```bash
# Non-relay specs (25 files: 13 non-relay must-run + 8 new targeted + 4 related):
(cd app && timeout 5400 ~/.claude/bin/reserve -c 8 --here -l "suite-app gui specs" -- \
  npx playwright test --workers=\$RESERVED_CORES \
    tests/gui/document-conflict.spec.js \
    tests/gui/document-storage.spec.js \
    tests/gui/document-identity.spec.js \
    tests/gui/document-routing.spec.js \
    tests/gui/auto-restore.spec.js \
    tests/gui/home-page.spec.js \
    tests/gui/git-provider.spec.js \
    tests/gui/github-storage.spec.js \
    tests/gui/open-from-link.spec.js \
    tests/gui/agent-document-load-gate.spec.js \
    tests/gui/examples.spec.js \
    tests/gui/test-case-browser.spec.js \
    tests/gui/layout-overflow.spec.js \
    tests/gui/agent-save-deadlock.spec.js \
    tests/gui/git-rename-safety.spec.js \
    tests/gui/git-index-cas.spec.js \
    tests/gui/git-filename-stability.spec.js \
    tests/gui/gitlab-cas.spec.js \
    tests/gui/import-conflict.spec.js \
    tests/gui/base-path-build-scanner.spec.js \
    tests/gui/storage-conflict-edge-cases.spec.js \
    tests/gui/document-context-menu.spec.js \
    tests/gui/document-format-seam.spec.js \
    tests/gui/git-links.spec.js \
    tests/gui/sketch-corpus-api.spec.js)

# Relay-backed agent document spec:
(cd app && timeout 5400 ~/.claude/bin/reserve -c 4 --here -l "suite-app agent-documents spec" -- \
  npx playwright test --workers=1 tests/gui/agent-documents.spec.js)
```

---

## 4. What Was Not Finished & Remaining Limitations

- **Live Playwright browser execution:** Not run from this worktree per brief rule (`AGY_BRIEF_SUITE_APP.md:76-78`). All 26 specs listed in Section 3 are ready to run once `agy/suite-app` is checked out on the main workspace's port `5173` server.
- **GitHub Contents API multi-file atomicity:** Each GitHub/GitLab/Gitea REST Contents call (`putFile` on new path, `#mutateIndex` on `.waffle-index.json`, `deleteFile` on old path) is an individual commit rather than a single Git trees/refs transaction. `GitProvider.put` now writes the new file with a null-base check *before* updating `.waffle-index.json` (via CAS merge-and-retry in `#mutateIndex`, up to `MAX_INDEX_CAS_ATTEMPTS = 6`) and deletes the old file last (only if no other index entry references it), so a mid-rename network failure can leave an unreferenced old file behind rather than losing the document or clobbering concurrent `.waffle-index.json` entries. If >6 clients contend on `.waffle-index.json` simultaneously, `#mutateIndex` throws the host conflict error after exhausting retries.
- **`assayCaseApi.js` `fetchWithFallback` content-type check:** `app/src/lib/engine/assayCaseApi.js:24-25` requires `contentType.includes('application/json')` from `/api/assay-cases*` before accepting the dev response over static `${base}/assay/*`. All routes in `app/src/lib/vite-plugins/testCaseApi.js:257-321` return `application/json`.

---

## 5. For the Project Memory

- **SvelteKit `BASE_PATH` static verification (`WAFFLE_BUILD_OUT_DIR`):** `app/svelte.config.js` honors `WAFFLE_BUILD_OUT_DIR`, and `node app/scripts/check-base-path-build.mjs /apps/waffle` builds into a temporary directory without touching `app/build`, scanning all 76 built HTML/JS/CSS/JSON/SVG files for root-absolute `/_app`, `/pkg`, `/api`, `/examples`, `/assay`, `/icons`, `/manifest.json`, app routes (`/home`, `/doc`, `/open`, `/view`, `/agent`), HTML `href`/`src`/`action`, and CSS `url(/...)`.
- **Engine lock vs. `saveChain` ordering (`store.svelte.js` & `executor.js`):** Never call `buildDocumentJson()` (which acquires `engineLock` / `bridge.send`) from inside `saveChain`, and never await `saveChain` while holding `engineLock` (`withAgentLock`); compose snapshots outside `saveChain` with `composeLock` and order storage writes via `saveComposeSeq` / `lastSavedComposeSeq`.
- **Agent tool schema edits require regenerating `agent-tools.manifest.json`:** Editing any schema in `app/src/lib/agent/tools/*.js` must be paired with `node app/scripts/gen-agent-manifest.mjs` so `relay/src/waffle_mcp_relay/agent-tools.manifest.json` stays byte-identical (`relay/tests/test_manifest.py`).
- **GUI test pitfalls (`helpers/state.js`, `drafts.js`, `ToastContainer.svelte`, `setProjectName`):**
  - `collectCrashErrors` and `expectNoAnyCrash` live in `app/tests/gui/helpers/state.js` (not `helpers/waffle-test.js`).
  - `pruneDrafts()` drops any draft in `waffle-iron-drafts` older than `DRAFT_MAX_AGE_MS` (`30` days), so seeded test drafts must use `Date.now()`-relative `modified` timestamps rather than small integer epoch timestamps like `3000`.
  - Toast DOM assertions should target `[data-testid="toast-container"]` (or `[data-testid^="toast-item-"]` / `window.__waffle.getToasts()`), not `[data-testid="toast"]`.
  - `setProjectName()` sends `SetDocumentMeta` asynchronously to the worker; wait for `window.__waffle.hasPendingAutoSave() === true` before calling `window.__waffle.commitPendingAutoSave()`.
  - `openDocumentRecord()` must await `loadProject()` *before* calling `initDocumentState()`, so `activeDocId` never flips to the incoming document until `ModelUpdated` has already mirrored `display_unit` and `tabs`.
  - `/home` still mounts `+layout.svelte` and starts `initEngine()` (`tabKey()` + `pagehide` lifecycle); seed `sessionStorage['waffle-tab-key']` / raw IndexedDB drafts on a static asset of the same origin (`/manifest.json`), and navigate passing pages to `about:blank` in `afterEach` (`examples.spec.js:68-70`) so shared worker Chromium instances drop WASM/WebGL renderers promptly.
  - `getDocumentFromDB(page, id)` in `app/tests/gui/helpers/waffle-test.js:167-187` wraps raw `IDBObjectStore.get(id)` and resolves `undefined` (not `null`, unlike `IndexedDBStore.get(id)`) when no record exists under primary key `id`.
