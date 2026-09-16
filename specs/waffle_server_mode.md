# Waffle Iron Server Mode — Headless Kernel Host, Dual-Transport MCP, Viewer Sync

Status: **draft rev 0 (investigation + spec, no implementation)**, 2026-09-15.
Amends `specs/waffle_mcp_server.md` (rev 2). That spec's §7 rejected a
headless server for two reasons; §0.3 below answers both, and this spec
does not proceed if the answers are rejected.

Scope: Phase 1 is an audit of the code as it stands (commit `540e3620`).
Phases 2–4 are specs grounded in it. Nothing here is built.

---

## 0. Summary

| Question | Answer |
|---|---|
| Is the Rust core free of `wasm-bindgen`/`web-sys`? | **Yes.** They are target-gated deps of `wasm-bridge` only, used only in `wasm_api.rs` (`#[cfg(target_arch = "wasm32")]`). `dispatch`, `EngineState`, tessellation build natively today; 13 native test binaries already drive `dispatch`. |
| Is the JS layer rendering-only? | **No.** ≈ 19% kernel/document logic, ≈ 23% mixed, ≈ 58% render/host (§1.2). The JS store is **authoritative** for tabs, inactive tab trees, assembly trees, document metadata and the whole sketch session — a latent A2.1 violation. |
| Does the JS document logic run headless as-is? | **No.** It is a Svelte 5 runes module with module-global state (one document per process), toasts/settings/IndexedDB imported inline, three unguarded `window` uses, and FIFO response pairing with no request ids. |
| Is boolean evaluation the bottleneck? | **Yes, for real documents** — measured, §2.5. The Bike frame (44 features) takes 274 s to load in wasm (Node, no DOM) and 168 s natively on a 24-core desktop; 99% of that is feature rebuild. Phone timing is unmeasured (oracle H1). |
| Recommended host | **Native Rust host binary shipped inside the relay's platform wheels** (`uvx waffle-mcp-relay --kernel host` — no new install step), behind a target-independent document session in `wasm-bridge` that both the browser worker and the host call (§2.3). §2.4. |
| Prerequisite for any host | Move document authority (tabs, assemblies, metadata) and the agent tool semantics out of `store.svelte.js` into Rust, proven by differential tests against the current JS path. Without it every option either forks the logic or embeds a browser. |
| Viewer sync | **Stream meshes, never the op log.** Content-addressed mesh blobs + state-snapshot updates keyed by a monotonic revision; resync = one small manifest plus cache misses. §4. |

### 0.1 Constraints restated

- **C1** The browser-only path works with no server, no install, no
  behavioral change, and stays the default.
- **C2** Server mode is additive: a second entry point, not a replacement.
- **C3** One kernel, two hosts: no forked modeling or tool semantics.

### 0.2 Governance fit

- **A2.1** ("all authoritative model state must live in Rust/WASM") — server
  mode *requires* finishing A2.1: today tabs/assemblies/metadata live in JS
  (§1.3). The prerequisite in §2.3 is A2.1 compliance work, useful even if
  server mode is never shipped.
- **A2.2** ("Web Worker *or equivalent isolation*", explicit messages) — a
  host process speaking `UiToEngine`/`EngineToUi` over a pipe is equivalent
  isolation.
- **A2.3** (transfer mesh buffers, picking metadata, summaries; never B-Rep)
  — the viewer protocol transfers exactly that set (§4).
- **A2.4** (typed, versioned protocol) — two new versioned protocols:
  `waffle-host/1` (relay ↔ host) and `waffle-viewer/1` (host ↔ viewer).

### 0.3 Answering `waffle_mcp_server.md` §7

1. *"A second host of the engine, invisible to the user. It is a dev tool."*
   The viewer (§4) makes the agent's work visible in real time on any device,
   which is the rev-2 goal; the only change is where the kernel runs.
2. *"Its tool semantics would drift from the page's."* Correct for rev 1,
   which reimplemented tools natively. Here the tool semantics move into one
   Rust implementation that the page also executes (§3.3), so there is
   nothing to drift from. This is the load-bearing decision of the spec.

The rev-2 Phase 3 `--headless-app` (relay drives the real page in headless
Chromium) is evaluated as option 0 in §2.2.

---

## 1. Phase 1 — Audit findings

### 1.1 Rust

| Crate / file | Browser coupling | Notes |
|---|---|---|
| `cad-primitives`, `cherchi-rs`, `ssi-rs`, `yang-rs`, `kernel-v2`, `waffle-types`, `sketch-solver`, `modeling-ops`, `feature-engine`, `file-format`, `step-import` | **none** (`grep wasm_bindgen\|web_sys\|js_sys` over `crates/*/src`: no hits outside `wasm-bridge` and one gated line in a non-shipped sidecar) | `waffle-types` enables `uuid/js` only under `cfg(wasm32)` |
| `wasm-bridge/src/dispatch.rs` (1259), `engine_state.rs`, `messages.rs`, `tessellation_runner.rs`, `assembly_view.rs`, `face_refs.rs`, `stl_export.rs` | **none** | `cargo check -p wasm-bridge` (native) passes; `dispatch()` is the single message entry point |
| `wasm-bridge/src/wasm_api.rs` (981) | **all of it**: `#[wasm_bindgen]`, `js_sys::{Date, Float32Array, Uint32Array}`, `web_sys::console` | `process_message` = parse → `dispatch` → `tessellate_missing_meshes` → `attach_preview_mesh` → serialize. **≈ 600 lines are target-independent logic trapped in the gated file**: `collect_renderable_bodies`, `bodies_of_engine`, `get_body_metadata` naming, `build_face_entries`, `build_edge_entries`, ghost/assembly addressing. A second host would duplicate them — they must move to a shared module first (§2.3 step S0). |

Consequences: the kernel stack compiles for any target; native already works
(`r0088_loadproject_repro.rs` runs the identical `LoadProject` path natively
and succeeds at 6.79 GiB peak RSS where wasm32 traps on its 4 GiB ceiling).

### 1.2 JavaScript classification

Counted by two independent read-throughs (store file read in full; others per
file). LOC excludes the generated `engineSchemas.generated.js` (3,580).

| Area | Kernel / document | Mixed | Render / host | Total |
|---|---|---|---|---|
| `engine/store.svelte.js` | 2,140 | 4,025 | 2,245 | 8,414 |
| `engine/` others | 458 | 688 (`worker.js`, `bridge.js`) | 0 | 1,146 |
| `agent/` | 2,327 (executor, commands, queries, delta, results, summary, sketchInput, tool schemas) | 757 (`link.js`, `documents.js`, `export.js`) | 158 | 3,242 |
| `storage/` | 432 | 957 | 873 | 2,262 |
| `sketch/` | 2,634 (profiles, finishProfiles, bspline, chain, offset, constraint logic…) | 3,149 (`tools.js` state machines) | 3,568 | 9,351 |
| `viewport/`, `ui/`, `routes/` | 0 | 143 | 18,377 | 18,520 |
| **Total** | **≈ 8,000 (19%)** | **≈ 9,700 (23%)** | **≈ 25,200 (58%)** | **42,935** |

"Mixed" is overwhelmingly clean engine messages interleaved with
`showToast`, dialog state and camera events — separable, but not separated.

### 1.3 Where authority lived when this spec was written

**S2 is complete (2026-09-16); the first four rows have moved.** The tab list,
every inactive tab's tree, the assembly trees and the document's metadata are
the Rust `DocumentSession`'s now, and the JS store's `$state` is a mirror fed by
`ModelUpdated.document` (C4, invariant A2.1). The rows below are kept as the
survey that motivated the work — read them as "before S2", not as current.
The rows still marked JS *and* not struck through (sketch session, agent tool
semantics, face planes, undo persistence) are still accurate: tool semantics are
S3's job, and the interactive sketch session is explicitly not moving (§2.3).

| State | Authority | Evidence |
|---|---|---|
| Active tab feature tree, feature results, meshes, feature undo/redo, parameters, sources table, file composition | **Rust** | JS replaces `featureTree` wholesale on `ModelUpdated` (store 762–764); undo is `{type:'Undo'}` (7614); `SaveDocument` composes (v4 inv. 7) |
| ~~Tab list and every **inactive** tab's tree~~ → **Rust** (C3a) | ~~JS~~ | `SwitchTab{tab_id}` names a tab; `AddTab`/`CloseTab`/`RenameTab`/`MoveTab` drive `DocumentSession`; the store mirrors `ModelUpdated.document.tabs` |
| ~~Assembly trees (instances, connectors, mates)~~ → **Rust** (C3b) | ~~JS~~ | `OpenAssembly{tab_id}` only; `EditAssembly{tab_id, assembly}` carries the panel's edits in; the open tab's tree comes back as `DocumentInfo.assembly_tree` |
| ~~Document name, display unit~~ → **Rust** (C3c/C4); id + created stay the HOST's | **split** | `SetDocumentMeta{name?, display_unit?, id?, created?}`; the storage record is keyed by the identity (v4 P2-5), so the host mints and latches it and pushes it down |
| In-progress sketch session and sketch undo | **JS** | store 153–224, 7638–7772; profiles extracted in JS (`sketch/profiles.js`, a port of `sketch-solver/src/profiles.rs`) |
| Agent tool semantics (gates, rollback, delta, results) | **JS** | `executor.js:180 executeTool`, `commands.js:101 applyStep`, `delta.js` |
| Face planes, `bodyForRef` hit mapping used by agent queries | **JS, derived from JS mesh copies** | store 5159; `queries.js:43–49` |
| Undo history persistence | **nowhere** | not in `file-format`, not in drafts; lost on tab reload today |

### 1.4 Hard browser dependencies inside kernel/document logic

| Dependency | Where | Class |
|---|---|---|
| Svelte `$state` module globals (≈ 150) | store throughout | **structural**: one document per module instance; needs the Svelte compiler to load |
| `showToast`, `getSetting`, `$app/paths` imported into mutation paths | store 13, 23, 8; `commands.js:21` | structural-lite: must be injected |
| `Worker`/`self`/`postMessage`; glue `fetch(new URL(.., import.meta.url))`; blob-URL restart | `bridge.js:54–80`, `worker.js:90–94, 377–382`, `pkg/wasm_bridge.js:592–597` | shimmable (pass bytes to init; restart = new thread) |
| FIFO response pairing, **no request ids** | `bridge.js:155, 203` | structural for any transport that can reorder or push unsolicited frames |
| IndexedDB (drafts, default storage provider, git cache) | `drafts.js:32`, `indexeddb.js:14`, `git/cache.js:19` | structural for persistence (a host needs its own provider) |
| `window.confirm` in `document_open`/`document_new` | `agent/documents.js:78` | policy, not shim |
| Unguarded `window` | store 5498, 7179, 7488 | throws in Node |
| `crypto.randomUUID` unguarded | store 5552 | one-line shim |
| `sessionStorage`/`localStorage` | link, drafts, storage | trivially shimmable |
| Render-bound tools | `viewport_view`, `viewport_capture` (window events answered by `CameraControls`/`AgentCapture`), `selection_get` (user's viewport selection), `export_*` `deliver:"download"` | inherently need a viewer |

**Verified:** the shipped `app/static/pkg` wasm bundle (web target) runs
under plain Node 18.20.8 with no DOM when handed its bytes (§2.5 probe).
So the *engine* is portable to every candidate host; the *JS document layer*
is not portable to any of them without refactoring.

---

## 2. Phase 2 — Headless host

### 2.1 What a host must provide

1. Own one `EngineState` + `KernelV2Adapter` per open document.
2. Serialize all mutations (today's `withEngineLock`), assign revisions.
3. Execute agent tools with the same semantics as the page (§3.3).
4. Persist the document (autosave) to a host storage provider.
5. Publish state to viewers (§4).
6. Isolate kernel crashes/OOM from the MCP server (A2.2).

### 2.2 Options evaluated

| | 0. Headless Chromium running the real page (rev-2 Phase 3 `--headless-app`) | 1. Node process (wasm + JS glue) | 2. Native Rust binary | 3. Wasmtime/Wasmer embedded in the relay |
|---|---|---|---|---|
| Logic fork | none (it *is* the page) | none only if the store is refactored into a runes-free, host-injected package (≈ 10k lines touched) | none once §2.3 lands | none once §2.3 lands |
| Runs today? | nearly: F4 probes already drove the real page headless | **no** (§1.4 structural items) | engine yes; tool layer no | engine needs a non-`wasm-bindgen` build (WASI) |
| User install | Playwright + ≈ 150 MB Chromium download; ≈ 300 MB+ RSS idle | Node ≥ 20 (dev container is 18, EOL) **in addition to** `uv` for the Python relay, or rewrite the relay in Node | none extra if shipped in the relay's platform wheels (the `ruff`/`uv` model) | `wasmtime` wheel (pure pip dependency, prebuilt per platform by upstream) |
| Our CI cost | none | none | per-platform build matrix (linux x86_64/aarch64, macOS arm64/x86_64, windows x86_64) | one extra wasm target (`wasm32-wasip1`) |
| Performance | wasm (baseline) | wasm (V8, same as browser) | **native: 1.64× faster on the Bike frame** (§2.5) | ≈ wasm (Cranelift) |
| Memory ceiling | 4 GiB (wasm32) | 4 GiB | **none** (R0088 loads at 6.79 GiB) | 4 GiB |
| Crash isolation | good (browser process) | worker thread / child process | child process | wasm trap isolation in-process; OOM of the host allocator is not isolated |
| Viewer sync | still must be built, from inside a page | must be built | must be built | must be built |
| Satisfies §7 objection 2 | yes | only with the refactor | yes with §3.3 | yes with §3.3 |

Rejected:
- **Option 0** as the product path: the heaviest install (a browser download
  for a "headless" feature), 4 GiB ceiling, and a server that is a scripted
  browser tab is the fragility we are trying to leave. It remains the right
  tool for CI/batch (rev-2 Phase 3) and a possible **stopgap** (see §2.6).
- **Option 1**: its premise ("lowest friction if JS kernel logic is
  non-trivial") holds only if that JS runs headless unchanged. It does not
  (§1.4). Making it headless is the same size of refactor as moving authority
  to Rust, leaves A2.1 violated, and adds a second runtime next to the Python
  relay.
- **Option 3**: viable after §2.3 and friction-equivalent to option 2, but
  keeps the 4 GiB ceiling (the one documented class of documents that fail in
  the browser and would load on a server) and wasm speed, and needs a third
  build target without `wasm-bindgen` (the current bundle imports
  `Float32Array`, `Date.now`, `console`, and `getrandom`'s JS backend).
  Keep it as the fallback if the platform matrix proves too costly.

### 2.3 Prerequisite: one kernel, one document session (all options except 0)

Staged; each step leaves the browser path byte-identical in behavior (C1),
proven by the existing GUI suites plus the named oracle.

| Step | Change | Oracle |
|---|---|---|
| **S0** — landed 2026-09-15 | Move the ≈ 600 target-independent lines of `wasm_api.rs` (renderable-body collection, naming, face/edge entries) into a shared `wasm-bridge/src/render_view.rs`, and the message pipeline (parse → dispatch → tessellate → preview → serialize) into `wasm-bridge/src/process.rs` with the clock and logger injected; `wasm_api` becomes a pure binding shim | `wasm-bridge/tests/render_view_parity.{rs,mjs}` over 7 scenarios (5 corpus loads, an assembly, an in-context edit with ghosts). **Bundle, byte for byte:** the rebuilt bundle's census of every worker accessor equals the pre-move bundle's (`golden.json`). **Native vs bundle, structure:** same response types, body metadata and all counts; bytes differ across targets by design (§2.7 H3) |
| **S1** — landed 2026-09-16 | Request ids in the bridge (`{id, msg}` envelope; worker echoes `id`), replacing FIFO pairing. Needed by any multiplexed transport | `sketch-drawing-regression.spec.js` + agent-link specs green |
| **S2** — landed 2026-09-16 (C1–C4) | **Document session in Rust.** `EngineState` gains the tab list, inactive tab trees, assembly trees, document metadata, a per-tab undo stack, and a monotonic `revision`. New messages `AddTab`/`CloseTab`/`RenameTab`/`MoveTab`/`EditAssembly`/`SetDocumentMeta`; `SwitchTab` takes an id, not a tree. The JS store keeps its `$state` fields as **mirrors** refreshed from `ModelUpdated` (A2.1 compliant) | `format_tests` round trip; new `session_tests.rs`; GUI tabs/assembly specs unchanged |
| **S3** — C1 landed 2026-09-16 | **Agent tool semantics in Rust**: `wasm-bridge/src/tools/` implements `execute_tool(session, name, args, ctx) -> ToolResult` for every non-render tool (gates that are document state, rollback, `modelDelta`, results shaping; `sketch_create` uses `sketch-solver` profiles, which JS already ports). New message `UiToEngine::Tool{name, arguments, context}`. Host-only concerns stay per host (§3.3). Migrated tool by tool, **shadowed**: the page runs both JS and Rust and asserts equal `structuredContent` in dev builds until the JS version is deleted | per-tool differential oracle over O1–O22 scripts |
| **S4** | Host binary `waffle-host` (new crate `crates/waffle-host`, native only) wrapping the session | §2.7 oracles |

**S3 checkpoints.** **C1** (landed 2026-09-16) is the mechanism plus the first
tool: `UiToEngine::Tool` / `EngineToUi::ToolResult`, `crates/wasm-bridge/src/tools/`
with `execute_tool` and the `MIGRATED` list, the page-side shadow
(`executor.js` `setShadow`, differential in `app/tests/gui/agent-rust-tools.spec.js`),
and `model_summary`. Then, in order: **C2** the remaining pure reads
(`feature_get`); **C3** the tools that only wrap an existing engine message
(`body_measure`, `face_list`, `sketch_regions`, `expression_evaluate`); **C4**
the authoring core — `applyStep` (snapshot → dispatch → `ModelDelta` →
rollback-on-new-error) and the twelve tools built on it; **C5** `sketch_create`,
which additionally needs `buildFinishProfiles` ported (the one part of that path
with no Rust twin — `extract_profiles` already has one in `waffle_types`);
**C6** the export pair, whose `deliver:"download"` half stays in the page.

A migrated tool's JS body is deleted, and its name leaves `MIGRATED`, only once
the differential has run green over a model that exercises it — two agreeing
empty answers prove nothing, so the differential asserts its own call count.

Not moved, by §3.3: `viewport_view` / `viewport_capture`, `selection_get`, the
storage tools, and the provider and UI halves of the document and tab flows.

Not moved: the interactive sketch session (drawing tools, snapping, sketch
undo). Server mode v1 has no interactive sketching in the viewer (§4.9); the
agent's `sketch_create` is atomic and lives in S3.

### 2.4 Recommendation: native host binary, shipped in the relay's wheels

- **Process model:** the relay spawns `waffle-host` as a child process and
  talks `waffle-host/1` over its stdin/stdout (length-prefixed frames: JSON
  header + optional binary payload). A kernel abort or OOM kills the child,
  not the MCP server; the relay reports `EngineCrashed`, restarts the child,
  and reloads the last autosave (§4.8).
- **Distribution:** the Rust binary is placed in platform wheels of
  `waffle-mcp-relay` (`maturin` "bin" bindings or a prebuilt binary in
  `package-data`, as `ruff` ships). The user command stays
  `uvx waffle-mcp-relay …`; `uv` selects the wheel. A pure-Python sdist
  without the binary still supports page mode, so no platform is worse off
  than today. Pip-installed files are not quarantined the way browser
  downloads are; confirm there is no macOS Gatekeeper prompt on first run
  (oracle H6) before relying on it.
- **Why not in-process (pyo3):** in-process loses crash isolation, and a
  kernel abort would take the agent's MCP session with it.
- **Ports:** the host listens on nothing. The relay's existing port (flag →
  `$PORT` → `proj port` → fail loudly, per repo policy and rev-2 §10)
  carries both the page link and the viewer protocol, so there is no second
  port to register.

### 2.5 Measured: is boolean evaluation the bottleneck?

Probe (scratch, not committed): `LoadProject` of `Bike frame.waffle.json`
(44 features, v4, the document from `docs/notes/agent_bicycle_session_failures_2026_09_14.md`),
same machine (24 cores, load < 1), single-threaded in every case.

| Host | Load + tessellate | Peak memory |
|---|---|---|
| Headless Chromium, real app (F4 note, earlier run) | 204–218 s | — |
| Node 18.20.8, shipped wasm bundle, no DOM | 274 s (dispatch 272.8, tessellate 1.6) | 283 MiB wasm heap |
| Native release, same `dispatch` + tessellation | 168 s (dispatch 166.2, tessellate 1.5) | not captured (no `/usr/bin/time` here) |

Render payload after load: 17 bodies, 1,231,452 triangles,
30.4 MB raw typed arrays + face/edge JSON; gzip 20.1 MB, brotli-5
18.7 MB (§4.5).

Conclusions:
- Evaluation dominates: 99% of load time is `dispatch` (feature rebuild,
  i.e. booleans), 1% tessellation.
- Native is **1.64×** faster than wasm on this document, well above the
  10–30% assumed in the brief. (Node 18's older V8 is also slower than the
  earlier Chromium run; the Chromium number is from a different session and
  is not a controlled comparison.)
- The payload is large but a one-time cost per mesh: 30 MB crosses a LAN in
  seconds, versus minutes of kernel time. Over cellular it is not negligible
  (§4.5).
- Six thin-walled tubes carry 160k–290k triangles each (94% of all
  triangles). That density is a tessellation-policy question worth its own
  look: it costs viewer GPU memory and bandwidth in both modes.
- The **phone** factor is not measured. What is known without measuring is
  that the phone today must redo this whole rebuild after every tab kill,
  and that a kill during a minutes-long rebuild restarts it. Oracle H1
  measures the device time before anyone cites a number.
- Incremental rebuild (F7) already makes unrelated edits cheap; the
  expensive cases are load/restore and edits upstream of many booleans,
  which is precisely what a tab kill forces the phone to redo today.

**How to profile, when needed:**
- Native per-stage: build the probe (or `r0088_memory_profile.rs`) in
  release with `debug = 1`, then `perf record -g` / `samply record`; kernel
  STOP traces use the existing `YANG_*_TRACE` knobs.
- Per-feature attribution: prefix-truncate the feature list, one prefix per
  process (the `R0088_PREFIX` method; in-process loops mislead).
- Browser: `wasm-pack build --profiling` keeps names for the Chrome/Safari
  performance panel; `process_message` already logs `dispatch=`/`tess=`
  splits over 100 ms to the console.
- Phone: Safari Web Inspector (remote) timeline on the same document.

### 2.6 Optional stopgap

If mobile relief is wanted before S2–S3 land: option 0 (relay drives the
real page in headless Chromium on the desktop), with the phone attaching to
that page as a §4 viewer. It needs only the viewer protocol and no
authority refactor, at the cost of Chromium on the host. Take it only if S2–S3
slip; it is not a step toward S4.

### 2.7 Oracles (host)

- **H1** per-device rebuild time table (desktop native, desktop wasm, iPhone
  Safari) for the Bike frame and F0085.
- **H2** host vs page differential: every O1–O22 script produces identical
  `structuredContent` through `--kernel page` and `--kernel host`.
- **H3** tessellation determinism across processes: two host processes
  loading the same file produce byte-identical mesh blobs (prerequisite for
  §4.4's cache surviving restarts). Measured during S0 (2026-09-15):
  - Native render arrays, faces, edges and metadata are byte-identical
    across two native processes on all seven parity scenarios.
  - The `ModelUpdated.preview_mesh` is **not**: `decimate_mesh`
    (`feature-engine/src/preview_mesh.rs:84–105`) numbers output vertices in
    `HashMap` iteration order, which std randomizes per native process
    (wasm32 happens to be stable). Must be fixed (e.g. `BTreeMap` or
    first-seen order) before a host content-addresses previews.
  - **Native and wasm32 meshes differ at the bit level**: on F0061,
    near-zero normal components differ by ~1e-16, 20 vertex slots are
    permuted and a few triangulation near-ties flip; vertex multisets are
    equal. So a mesh id from the host never matches one the page computed
    for the same document. The viewer cache is keyed only by host output,
    so this is acceptable, but no design may assume cross-target mesh
    identity.
- **H4** crash isolation: forced abort in the host → relay alive, tool
  returns `EngineCrashed`, child restarted, document reloaded.
- **H5** R0088 loads in `--kernel host` (native ceiling).
- **H6** clean `uvx` first run on macOS arm64, Windows x86_64, Linux — no
  prompts, no extra install.

---

## 3. Phase 3 — Dual-transport MCP

### 3.1 Current single message layers (confirmed)

| Layer | Single entry point | Status |
|---|---|---|
| MCP → relay | `server.py:_call_tool` (213): schema validation, then `LinkServer.call` | single |
| relay → page | `link.py:call` (179): `call{id, tool, arguments}` → `result{content, structuredContent, isError}` | single |
| page tools | `executor.js:180 executeTool`: every page tool | single |
| page → engine | `sendAgentMessage` → `bridge.sendUngated` → `process_message` → `dispatch` | single, but **tool semantics sit above it in JS** |

So the *wire* layer is already single and host-agnostic. The *semantic*
layer is not reusable by a host; S3 consolidates it.

### 3.2 Transport selection

`waffle-mcp-relay --port N --kernel page|host [--documents DIR]`

- `--kernel page` (default): today's behavior, byte-for-byte.
- `--kernel host`: spawn `waffle-host`; page tools go to it; the WebSocket
  serves viewers (§4) instead of an executing page.

The relay gains one interface, `Backend.call(tool, arguments, progress,
cancel) -> ToolResult`, implemented by `PageBackend` (current `LinkServer`)
and `HostBackend` (child-process frames). `server.py` is otherwise unchanged:
same manifest, same schema validation, same error codes (§6.1 closed set),
plus two new codes, `ViewerUnavailable` and `HostCapability`.

### 3.3 Where each concern executes

| Concern | Page mode | Host mode |
|---|---|---|
| Tool semantics: validation beyond JSON schema, rollback, delta, result shaping | Rust `execute_tool` via worker | Rust `execute_tool` in host (**same code**) |
| Mutation serialization (engine lock) | store `withEngineLock` | host command queue |
| `UserBusy` gates | page UI state (sketch mode, dialogs) | only `rebuilding` (a queued user command from a viewer) |
| `AgentPaused` | agent bar in page | agent bar in any viewer → host |
| `selection_get` | page selection | the **focused viewer's** selection (most recent input); none attached → `ViewerUnavailable` |
| `viewport_view`, `viewport_capture` | page camera / canvas | forwarded to the focused visible viewer (`capture_request`); none → `ViewerUnavailable` |
| `export_*` `deliver:"download"` | browser download | host writes into `--documents DIR/exports/` and returns the path (`deliver:"inline"` unchanged) |
| Storage tools (`storage_list`, `document_open/save/new`) | IndexedDB / git providers | host file provider rooted at `--documents DIR` (git providers: `HostCapability` in v1) |
| `window.confirm` on unsaved changes | page dialog | host policy: autosave makes "unsaved" transient; `document_open` saves first |

The manifest gains `x-hosts: ["page","host"]` per tool; the relay filters
`tools/list` by the active kernel and emits `tools/list_changed` when it
changes. Tool **names, schemas and result shapes are identical** across
modes; only availability differs.

### 3.4 `waffle-host/1` frames (relay ↔ host, stdio)

| Direction | Frame | Fields |
|---|---|---|
| host → relay | `ready` | `protocol`, `host_build`, `epoch` (random per process) |
| relay → host | `tool` | `id`, `name`, `arguments`, `context{agent_name, progress}` |
| host → relay | `progress` / `result` | as rev-2 §2.3 |
| relay → host | `cancel` | `id` |
| relay ↔ host | `viewer` | opaque `waffle-viewer/1` frames multiplexed per `viewer_id` (the relay terminates the WebSocket and auth; the host owns sync state) |
| either | `bye` | `reason` |

---

## 4. Phase 4 — Viewer sync protocol (`waffle-viewer/1`)

### 4.1 Model

The host holds the authoritative document. A viewer holds **no
authoritative state**: a render cache plus viewer-local UI state (camera,
visibility toggles, panel layout). Therefore a viewer can be killed at any
moment without losing anything that matters, and recovery is a pure
function of (host state, viewer cache).

Every committed mutation increments `revision: u64`. The pair
`(epoch, revision)` names a document state; `epoch` changes when the host
process restarts or the document is reopened.

### 4.2 Stream meshes, not the op log — decision

| | Op log / feature tree replay | Computed meshes |
|---|---|---|
| Wire size | KB | up to ≈ 7 MB raw per changed body on the Bike frame (§2.5), 0 when unchanged |
| Client CPU | **re-runs every boolean** — the phone becomes the kernel again | decode + upload to GPU |
| Client memory | full kernel + wasm heap (up to 4 GiB ceiling) | render buffers only |
| Client needs the 9 MB wasm | yes | **no** |
| Consistency risk | client and host must be bit-identical kernels (versions, float paths) | none: the client draws what the host computed |
| Tab-kill resume | replay minutes of booleans | cached blobs, manifest only |

**Decision: stream meshes (plus the compact summaries A2.3 allows: tree,
errors, parameters, body metadata, picking ranges). The op log is never
executed by a viewer.** The feature tree is sent as *display data* for
the tree panel; it is never replayed.

### 4.3 Frames

Control frames are JSON text; blobs are binary frames
(`u32 header_len | JSON header | payload`).

| Dir | Frame | Fields |
|---|---|---|
| V→H | `attach` | `protocol`, `session` or `code`, `document_id?`, `have?: {epoch, revision}`, `visible: bool` |
| H→V | `welcome` | `viewer_id`, `session`, `epoch`, `host_build` |
| H→V | `snapshot` | `epoch`, `revision`, `document{id, name, tabs, active_tab, display_unit}`, `tree`, `errors`, `warnings`, `parameters`, `bodies: [BodyEntry]`, `activity{agent, tool?, paused}`, `selection` (this viewer's, if resumed) |
| H→V | `update` | `epoch`, `base_revision`, `revision`, then only the changed top-level keys of `snapshot` (latest-wins state, not ops); `bodies` as full list of `BodyEntry` when any body changed |
| H→V | `rebuild` | `state: started \| progress \| done`, `feature_id?`, `elapsed_ms` (from `tool` progress) |
| V→H | `want` | `mesh_ids: [..]` |
| H→V | `blob` (binary) | header `{mesh_id, encoding, byte_length}` |
| V→H | `select` | `geom_refs` (per-viewer selection; the most recent input marks this viewer *focused*) |
| V→H | `visible` | `bool` (page visibility; only visible viewers answer capture requests) |
| H→V / V→H | `capture_request` / `capture_result` | `id`, `view?`, `width`, `height` / `id`, `png_base64` or `error` |
| V→H | `command` (phase V3) | `id`, `tool`, `arguments` — user actions from the viewer go through the same tool layer and queue |
| both | `ping` / `pong`, `bye` | as rev-2 |

`BodyEntry = {body_id, name, feature_id, visible_default, mesh_id,
bbox, face_ranges_id, edge_id}`. Picking metadata (face ranges with
`GeomRef`s, edge entries) are their own content-addressed blobs so a rename
does not resend them.

### 4.4 Content-addressed meshes: the delta strategy

- `mesh_id = xxh3-128(canonical encoded bytes)`, computed once per
  tessellation in the host.
- F7 incremental rebuild keeps unchanged features' meshes, so their ids are
  unchanged. An `update` lists the full body set (tens of entries, KB); the
  viewer requests only the ids it does not hold. **The delta is at body
  granularity with zero diff machinery.**
- Viewer cache: memory LRU plus an IndexedDB store keyed by `mesh_id`
  (bounded, e.g. 256 MB, evicting oldest). Because keys are content hashes,
  the cache is valid across reconnects, host restarts (H3) and documents.
- Host keeps no per-revision history. Any gap (`base_revision` ≠ the
  viewer's revision, or `epoch` changed) ⇒ host sends `snapshot`. A snapshot
  is small by construction; blobs are the cost, and the cache absorbs it.

Rejected for v1:
- **Vertex-level mesh diffs.** Re-tessellating a modified body renumbers
  vertices; a diff of two tessellations is about as big as the second one.
- **Face-granular chunks** (hash each face's triangles separately, so a cut
  resends only the faces it touched). Promising, but its value depends on
  per-face tessellation being stable for untouched faces across a rebuild,
  which is unmeasured. Oracle V7 measures the face-hash hit rate on the
  corpus edit scripts; adopt it as encoding v2 only if the hit rate is high.

### 4.5 Encoding

Measured on the Bike frame load (§2.5): general-purpose compression does
little for float geometry. gzip reaches 0.66 and brotli-5 0.62 of 30.4 MB, a
38% saving, at 0.4–0.6 s of host CPU.

| Encoding | Content | Use |
|---|---|---|
| `raw/1` | `Float32` positions + normals, `Uint32` indices, edge `Float32`, as the worker transfers today | loopback / LAN |
| `mq/1` | quantized positions (16-bit per axis within the body bbox: ≤ 11 µm on a 0.7 m body; display only, all measurement stays on the host), oct-encoded 2-byte normals, `meshoptimizer` vertex/index codec (the glTF `EXT_meshopt_compression` scheme; fast JS/wasm decoder), then brotli | **any non-loopback viewer, v1** — the brotli result above shows compression alone is not enough over cellular. Its ratio on this document is **unmeasured**; V6 measures it before any number is quoted |
| `raw/1+br` | `raw/1` brotli-compressed | fallback if `mq/1` slips |

The index buffer stays full precision; face ranges and `GeomRef` picking are
index-range based, so quantization never changes what a click selects.

### 4.6 Connection lifecycle and the tab-kill scenario

1. User opens the viewer link → `attach{code}` → `welcome{session}` →
   `snapshot` → `want` for uncached ids → blobs → rendered.
2. Agent works: each committed tool → `update` (+ `rebuild` progress
   while the kernel runs). The viewer shows a live spinner with the feature
   name.
3. User switches to the terminal; iOS suspends, then kills the tab.
4. User returns; Safari reloads the URL. The viewer reads its session
   (below), paints the **last cached frame immediately** from IndexedDB (a
   stale badge), sends `attach{session, have}`.
5. Host: same epoch and revision → `welcome` only; otherwise `snapshot`.
   Viewer requests the few missing blobs. Visible result: the model as the
   agent left it, with no rebuild on the phone.

**Session storage on the viewer:** the session token goes in
`sessionStorage` and also in `localStorage` under the host URL with the
resume expiry, because an iOS tab *discard* does not reliably keep
`sessionStorage`. Tradeoff: another tab of the same browser could resume the
viewer session. That is acceptable for a read-mostly viewer and is **not**
acceptable once V3 `command` exists; at V3, commands require the
`sessionStorage` token (see §4.7).

**Reconnect/backoff:** reuse `link.js` policy verbatim: immediate on
`visibilitychange`/`pageshow`/`online`; otherwise 1, 2, 4 … 30 s with
±20% jitter while visible; never while hidden; `ping` probe with 5 s
timeout on wake (a frozen socket can look open). Host-side heartbeat 15/30 s
as rev-2.

**Backpressure / slow viewer:** per-viewer outbox holds at most one pending
`update` (coalesced, latest wins) and the blob queue. If the socket's
buffered amount exceeds a bound, the host drops the queued update and marks
the viewer for `snapshot` when it drains.

### 4.7 Auth

The rev-2 model carries over with three changes.

| Element | Viewer mode |
|---|---|
| Link | `<app-url>view?host=<wss url>&code=<code>` returned by `waffle_connect` in host mode (and printed by `--persistent-link`) |
| Code | 32 random bytes, single use, 300 s — or persistent (0600 file), as rev-2 |
| Consent | none needed to *view* (the viewer is not granting access to its own data); the page shows which host and document it is attached to |
| Session token | **HMAC(host secret, viewer_id, expiry)**, so tokens survive host restarts. The secret lives in `$XDG_STATE_HOME/waffle-mcp-relay/host-<port>.secret` (0600); deleting it revokes all viewers. Resume window 1800 s after last disconnect (rev-2 value) |
| Multiple viewers | each attach with a valid code or token gets its own `viewer_id`; `waffle_connect` can mint more codes without revoking live viewers (unlike page mode's single pairing) |
| Origin check | exact allow-list, as rev-2 I8 |
| Commands from viewers (V3) | require a token whose claims include `command`, minted only through a consent click in that viewer, and held only in `sessionStorage` |

**Serving the viewer from the relay (optional `--serve-app DIR`):** the
relay also serves the built app on its port, so page and WebSocket are
same-origin. This sidesteps the §6.3 local-network-access block and
mixed-content rules entirely. It is the recommended configuration for
phones over `tailscale serve`.

### 4.8 Host restart while a viewer is attached

- Socket drops → viewer keeps rendering the last frame with a
  "Reconnecting…" badge and disables nothing it did not already disable
  (a v1 viewer has no mutations).
- Durability: the host autosaves through `SaveDocument` to `--documents DIR`
  after every committed tool (debounced 1 s; and flushed before acknowledging
  `document_save`). A crash loses at most the last second of committed work.
  **The undo history is lost** on restart, exactly as it is on a browser tab
  reload today; persisting `UndoStack` is a separate, optional item.
- New process → new `epoch` → reload document → rebuild. Viewers reattach
  with their HMAC tokens; since `have.epoch` differs they get `snapshot`
  once the rebuild finishes (`rebuild` progress frames meanwhile). With H3
  holding, every blob is a cache hit.
- Relay restart (not just host): same path, as long as the secret file and
  the port are unchanged.

### 4.9 Multiple viewers

Designed in, implemented in V4:
- All viewers of a document receive the same `update` stream.
- Selection is per viewer; `selection_get` reads the focused viewer.
- Camera is per viewer; `viewport_capture` targets the focused visible viewer.
- Mutations (agent tools and V3 viewer commands) are linearized by the host
  queue; `revision` gives every viewer the same order.
- Presence (`viewers: [{viewer_id, focused, visible}]`) rides in
  `activity`.

Out of scope for this spec: interactive sketching in a viewer. It needs
solver round trips per drag with latency budgets, and the in-progress sketch
session would have to become host state. It can be revisited after V3.

### 4.10 Viewer build

The viewer is the existing app under a new `/view` route that **never
starts the engine worker** and never downloads the wasm. It reuses the
viewport, tree panel and toasts, with store mirrors fed by `snapshot`/`update`
instead of `ModelUpdated`. The default route and all of its behavior are
untouched (C1).

### 4.11 Oracles (viewer)

- **V1** kill-and-resume: Playwright closes the viewer page mid-agent-session
  and reopens it by URL; rendered body set and tree equal the host's at the
  current revision, with no `LoadProject` in the viewer.
- **V2** host restart during attach: kill the host child; viewer reattaches;
  state equals the last autosave; zero blob requests when H3 holds.
- **V3** gap: drop 50 updates on the wire; the viewer converges via
  `snapshot`.
- **V4** two viewers see identical revisions; selection stays per viewer.
- **V5** unchanged-body edit (rename, parameter change on an unrelated
  sketch) transfers zero blobs.
- **V6** bytes on the wire per edit over the O-scripts, per encoding.
- **V7** face-chunk hash hit rate (gates encoding v2).
- **V8** real iOS Safari: terminal ↔ browser switching for 10 minutes
  during an agent session, no loss (manual cell, as rev-2 O23).

---

## 5. Phasing

| Phase | Content | Exit |
|---|---|---|
| **P-A: A2.1 compliance** | S0, S1, S2 | browser suites green; `session_tests.rs`; no behavior change |
| **P-B: tools in Rust** | S3, shadowed tool by tool | H2-style differential green in page mode; JS tool bodies deleted |
| **P-C: host** | S4, relay `Backend` split, `--kernel host`, file provider, wheels | H1–H6 |
| **P-D: viewer v1** | `/view` route, `waffle-viewer/1` snapshot/update/blobs, `raw/1` + `mq/1`, cache, auth, reconnect | V1–V3, V5, V6, V8 |
| **P-E: viewer v2+** | multiple viewers (V4), capture forwarding, `command` (V3 frames), face-chunk encoding if V7 justifies | V4, V7 |

P-A and P-B are useful even if P-C is never built: they remove a governance
violation and the only place tool semantics could drift.

---

## 6. Tradeoffs stated

- **Cost is front-loaded in P-A/P-B** (moving ≈ 2–3k lines of document and
  tool logic into Rust, plus shadow testing). The alternative that avoids it
  (Node host running the JS store) costs a comparable refactor of the store,
  leaves authority in JS, and adds a runtime.
- **Native wheels cost CI**: a five-platform matrix. In exchange users get no
  extra install, 1.64× measured speed, and no 4 GiB ceiling. Wasmtime keeps one
  artifact and gives up the latter two.
- **Meshes over op log** trades bandwidth (MB per changed body, mitigated by
  content addressing and compression) for keeping the phone out of the
  kernel entirely, which is the point of server mode.
- **Viewer tokens in `localStorage`** trade some same-browser exposure for
  surviving iOS tab discards; mutations stay on `sessionStorage` tokens.
- **Undo history does not survive a host restart**, the same as a browser
  reload today.

## 7. Open questions

1. Should the host support several open documents at once (one
   `EngineState` per document) in v1, or one document per host process? The
   spec assumes one per process for v1 (simpler crash semantics).
2. Git storage providers in host mode need token handling outside the
   browser; v1 returns `HostCapability`.
3. Is persisting `UndoStack` (and making `Command` serializable) worth doing
   for both hosts, so tab reloads also keep undo?
