# Agent-link bicycle session — failure log (2026-09-14)

## Fix status (2026-09-15)

| # | Status | Fix | Regression test |
|---|---|---|---|
| F1 | FIXED | `feature-engine/src/rebuild.rs` `unit_normal()`: every kernel frame, face and default extrude direction is built from the unit normal; stored sketch data is untouched, so old documents are repaired too | `test-harness/tests/sketch_plane_normal_precision.rs::region_annulus_on_rounded_normal_is_a_valid_boolean_operand` (before the fix the tube failed at extrude with `VertexOffSurface` when region vertices are exact on the circle; in-session, with 6-7-decimal vertices, it built and the boolean rejected it) |
| F2 | FIXED | same as F1 | `…::circle_profile_extrude_on_rounded_normal_builds` |
| F3 | FIXED | `waffle-types/src/regions.rs` `profile_outline`: loops containing arcs/splines are outlined by sampling their entities (`chain_polylines`); `vertex_ids` turned arcs into chords | `regions::tests::arc_line_stadium_region_carries_profile_entity_ids` |
| F9 | FIXED (custody) | `rebuild.rs` `carry_untargeted_siblings()`: untargeted outputs of a partly-targeted feature are carried unchanged as extra `Body{n}` outputs with a warning | `feature-engine/tests/combine_sibling_outputs.rs` |
| F9b | FIXED | `feature-engine/src/lib.rs` `inherit_source_body_id()`: an explicit combine's `Main` inherits the name of the first resolved target's OWN output (not that feature's `Main`), and carried siblings inherit from the body they carry; `rebuild.rs` `untargeted_sibling_sources()` is the single ordered source for both the carried bodies and their names | `combine_sibling_outputs.rs::names_follow_the_targeted_body_and_the_carried_sibling` (RED showed `Some("Top tube")` for the down tube, the exact in-app symptom) |
| F10 | FIXED (re-scoped) | Root cause: `openDocumentRecord` settled the startup restore BEFORE `loadProject`, so the link resumed and read the blank bootstrap tree for the whole rebuild. Now it settles in a `finally` after the load, and `executeTool` refuses every call with `UserBusy {reason: 'loading' \| 'restoring'}` via `getDocumentLoadBusyReason()` (`app/src/lib/engine/store.svelte.js`, `app/src/lib/agent/executor.js`) | `app/tests/gui/agent-document-load-gate.spec.js` (verified RED with the gate disabled); agent-link / reconnect / documents / authoring specs still pass (23) |
| F7 | FIXED | `feature-engine/src/rebuild.rs` `Changed` + skip decision in `rebuild()`: a feature re-executes only if it changed, names (by any UUID in its definition) a feature that re-executed, finds an input by tree position (legacy most-recent / share-a-face targets, through-all, projected sketches, context-scoped refs) after one, or has no cached result or error; everything else keeps its result, mesh and error. Callers pass what changed (`lib.rs` `changed_feature`/`changed_by`; the parameter and context passes report changed feature ids). Reorder and full rebuilds still re-execute everything | `feature-engine/tests/incremental_rebuild.rs` (8 tests; the unrelated-edit, parameter and rename-undo cases verified RED first), `test-harness/tests/incremental_rebuild_kv2.rs` (kernel-v2: a box notching an annulus tube, V_cut/V_tube = 5/6; the kept Cut keeps its handle and 76,930-triangle mesh, and a later re-execution against the same arena matches a from-scratch rebuild. Debug build: adding the Cut 15.1 s; editing an unrelated upstream sketch 20 ms, versus 15.2 s with the change stashed, where the test is RED) |
| F4 | OPEN — not reproduced | None. Three candidate causes measured in the real app and ruled out (see F4 "Investigation") | — |
| F11 | FIXED | `kernel-v2/src/recover.rs` pass 2: the two feet of one canonical seam share one azimuth (a reused foot fixes its minted twin's; a non-ruling reused pair takes the arc fallback). Before, a foot reused at the outer wall's azimuth and one minted at the coaxial bore's put the seam 4.2e-12 off its ruling | `test-harness/tests/f11_disjoint_cut_thin_tube.rs` (r 14 mm disjoint Cut RED before, in debug and release; r 15 mm and notch controls green) |
| R0081 regression | FIXED (knife-edge latent stays OPEN) | Bisected to F1 (`579f831e`): renormalizing a sketch normal already unit to rounding (|n| − 1 = −1.1e-16) moved R0081's tilted frames one ulp and its chained revolve union stopped at Stage-4 `LocalRefinementRequired`. `feature-engine/src/rebuild.rs` `unit_normal` now keeps such normals bit-identical (≤ 4·EPSILON) | `rebuild::tests::unit_normal_keeps_a_normal_that_is_unit_to_rounding`, `unit_normal_normalizes_a_six_decimal_normal`; R0081 SUPPORTED_CORRECT and R0085 back to baseline detail in `single_case` |
| F5, F6, F8 | OPEN | — | — |

Failures hit while an agent built a bicycle frame and fork over the agent link
(MCP) in the "Bike frame" document (browser-local). Each entry is written so a
test case can be built from it: minimal repro, observed vs expected, evidence,
and what is and is not yet verified. Status: **OPEN** unless marked otherwise.

Conventions: world Z up, +X forward; units meters; all calls are agent-link MCP
tools unless stated.

---

## F1 — Sketch plane normal is stored un-normalized; region-path extrude then builds off-circle arc edges that the boolean rejects

**Severity:** high — silently produces bodies that look and measure fine but
can never be a boolean operand. Every cut against the frame tubes failed.

**Repro (minimal):**
1. `sketch_create` with `plane: {origin: [0,0,0], normal: [0.718286, 0, 0.695747]}`
   (|n| = 1 − 5×10⁻⁷, a 6-decimal rounding of a unit vector), entities: a point
   and two concentric circles r = 0.0159 and r = 0.015.
2. `feature_add` Extrude of the annulus sub-region (explicit `region` with arc
   `outer_edges`/`hole_edges`, `boundary_entity_ids: [1,2]`), `combine: NewBody`.
   → succeeds; `body_measure` volume is exact (π(R²−r²)·L).
3. `feature_add` Extrude `combine: Cut` with `targets: [that body]` using any
   tool that intersects it.
   → `FeatureRebuildFailed: … yang-rs rejected the converted input B-Rep:
   malformed B-Rep topology: circle edge 0: endpoint vertex 0 is not on the circle
   (radial 0.011100004621446195 vs radius 0.0111 …)`.

**Observed:** the radial error ratio equals the normal's length error
(e.g. 0.022000006910 / 0.022 = 1 + 3.14×10⁻⁷ for normal
(−0.292372, 0, 0.956305), |n| − 1 = 3.2×10⁻⁷). Supplying full-precision region
vertices did not change the error at all (identical digits), because
`resolve_extrude_regions` (`crates/feature-engine/src/rebuild.rs:1679`)
re-derives the region from the sketch whenever `boundary_entity_ids` is set.

**Expected:** either `sketch_create` / Sketch deserialization normalizes
`plane_normal`, or the plane frame used to embed region arcs normalizes its
basis; a body that extrudes successfully must be a valid boolean operand.

**Verified:** the error, the ratio match, and that precise vertices do not help.
**CONFIRMED (fix-by-input):** after `feature_edit` of the tool sketch
(`1a066c29…`) and the down-tube sketch (`8c2b7db8…`) to full-precision unit
normals (e.g. (−0.29237190652740075, 0, 0.9563046942651346)), the identical
head-tube Cut (symmetric 0.2 per side, targets top tube + down tube) succeeded:
`bodies_added: [6ef34640…/Main, 6ef34640…/Body:1]`. The top tube's sketch
normal was exactly (1, 0, 0) all along, and it was never the rejected operand.
**Not yet located:** the exact site where the un-normalized normal scales the
arc embedding.

**Test-case idea:** feature-engine rebuild test — sketch with a normal of length
1 ± 1e-6, annulus region extrude, then a Cut targeting it; assert the cut
succeeds (or that sketch creation rejects/normalizes the normal loudly).

---

## F2 — Whole-circle profile extrude rejects a 6-decimal normal (`ProfileCircleFrameNotOrthonormal`)

**Severity:** medium (loud, but an agent supplying rounded normals hits it).

**Repro:** a sketch whose `plane_normal` is (−0.292372, 0, 0.956305)
(session-1 sketch `1a066c29…`, one circle r = 0.022); `feature_add` Extrude with
`profile_entity_ids: [1]`, `symmetric: true`, `combine: Cut`, explicit targets.
→ `FeatureRebuildFailed: kernel error: kernel-v2 circle profile rejected:
ProfileCircleFrameNotOrthonormal`.

**Cause (code-read, not yet test-confirmed):** `CIRCLE_FRAME_ORTHONORMALITY_TOLERANCE
= 1e-9` (`crates/kernel-v2/src/profile.rs:101`, rejection at `:340`); the frame
is built from the stored normal without normalization, so a 3×10⁻⁷ length error
fails. Same root as F1 (un-normalized normal), different symptom: this path is
loud, F1's region path is silent until a boolean.

**Expected:** normalize once at the sketch-plane boundary; the kernel tolerance
is correct for its contract ("sketch planes supply normalized bases") — the
engine is not honoring that contract.

---

## F3 — Closed arc+line loop reported with `profile_entity_ids: null`

**Severity:** low (workaround: explicit region).

**Repro:** `sketch_create` on any plane with six pinned points and
Arc(center 0, 2→3), Line(3→4), Arc(center 1, 4→5), Line(5→2) (a stadium,
centers (±0.04, 0), r = 0.016). → result `regions: [{profile_entity_ids: null,
area_m2: 0.0033632}]`, although `solved_profiles` contains a single outer
profile with `entity_ids: [6,7,8,9]`.

**Expected:** a region equal to one whole closed loop carries its
`profile_entity_ids` (as polygons made only of Lines do — the dropout plates in
the same session got `[11..21]`).

---

## F4 — `PageDisconnected` mid-call during a burst of reads

**Repro context:** a single response issued 4 `feature_edit` (Sketch) + 8
`feature_get` + 1 Bash in parallel; the 8th `feature_get` returned
`PageDisconnected: The page disconnected while the call was in flight`.
`waffle_status` immediately after: `ready`, `app_build 2026-09-14 54b43f86`,
document name now "Bike frame" (was "Untitled" at session start — possibly a
user rename; not confirmed). `model_summary` showed no lost state.

**Not verified:** whether the burst caused the disconnect (tab backgrounded /
reloaded vs relay load). Worth a relay test that fires ~13 concurrent calls.

**Second occurrence (during heavy booleans):** 4 parallel `feature_add` Cuts
(seat-tube tool → 3 targets, BB tool → 4 targets, two plate slot cuts → 1 target
each), issued while the tree already held one 2-target cylinder Cut and every
rebuild took > 2 min. Three calls returned `PageDisconnected`, the fourth
`PageAway`. Whether any cut was committed is unknown until `model_summary`.
Suspects: main-thread WASM rebuild starving the page's socket keep-alive, or a
tab crash / OOM from the boolean. Needs a browser-console capture on repro.

**Third occurrence (tab RELOAD, draft restore):** with ~8 cylinder Cuts in the
tree (each rebuild minutes long), a batch of `body_measure` ×2 +
`body_rename` ×2 + one plate-slot `feature_add` Cut (right dropout → chainstay R,
seatstay R) ran. The measures and the first rename returned. The second rename
got `PageDisconnected`; the Cut got `UserBusy: Waffle Iron is rebuilding a change
the user made … the Waffle Iron tab reloaded since the previous call and
reopened its last work from the browser's draft`. So the tab reloaded (crash,
OOM or mobile tab kill) and the draft-restore rebuild is the "user change". The
growing boolean count per rebuild makes a WASM memory ceiling the lead suspect;
measure heap per rebuild on repro.

**Investigation (2026-09-15, not reproduced).**
Mechanism (code-read):
- The relay pings every 15 s and closes the socket after 30 s without a pong
  (`relay/src/waffle_mcp_relay/link.py:367-376`). That close fails every in-flight
  call with `PageDisconnected`.
- The session then stays `page_away` (`pairing.py`), so the next call waits
  `AWAY_WAIT_S` = 10 s and gets `PageAway`. That matches occurrence 2 exactly.
- Booleans run in the engine Web Worker (`app/src/lib/engine/worker.js`), but the
  page answers `ping` on its MAIN thread (`app/src/lib/agent/link.js:286`).
- So F4 needs either a main-thread stall > 30 s, or the tab being suspended,
  reloaded or killed.

Measured with the real relay and headless Chromium, recording 100 ms event-loop
gaps and long tasks, in a scratch probe that was not committed:
- **F0064** (`LoadProject` 20 s): 13-call bursts every 3 s during the rebuild.
  117/117 calls `ok`, max main-thread gap 68 ms, no heartbeat drop.
- **F0085** (`LoadProject` 212 s, 40 features, 18k-triangle body): 949/949
  calls `ok`, max gap 471 ms, no heartbeat drop.
- **6 thin-wall annulus tubes + 3 parallel explicit-target Cuts**, plus 80
  `model_summary` calls while the Cuts ran: all `ok`, max gap 28 ms. The tools
  missed the tubes, though (each Cut 0.2 s), so this is NOT bike scale.

Ruled out:
- (a) A burst of concurrent calls.
- (b) A long worker rebuild starving the pong.
- ~~(c) The tab `preview_mesh` explaining the 28 MB document~~ **WRONG, corrected
  below.**

**The document size — found and fixed (2026-09-15).** The user's export
(`Bike frame.waffle.json`, 23.8 MB) is almost all thumbnail:
- The tab `preview_mesh` is 10.4 MB of compact JSON: 180k triangles, the full
  render mesh of the last body.
- The 44 features take 97 KB.

Cause:
- `dispatch` builds `ModelUpdated`, including its decimated preview, BEFORE
  `wasm_api::process_message` tessellates new bodies. After a load or full
  rebuild, no feature has a mesh yet, so the preview was `None`.
- The page then fell back to `Array.from` of the whole last mesh into its `$state`
  tab (`store.svelte.js`).
- Every autosave deep-cloned and serialized that copy on the main thread.

Fix:
- `dispatch::attach_preview_mesh` recomputes the preview after tessellation.
- The page stores only the engine's preview, never the render mesh.

Test: `test-harness/tests/preview_mesh_kv2.rs`. It asserts the dispatch
response has no preview (the bug), and that a 200-gon prism's preview is
decimated below its render mesh after attaching.

Measured on the real document (headless, 24 cores), same probe before and after:

| | before (`1fb31cf0`) | after (`b4be84db`) |
|---|---|---|
| saved document | 23.8 MB | 420 KB (`buildDocumentJson` 11 ms) |
| preview triangles | 179,997 (the full body) | 1,277 (vertex clustering; 500 is its target, not a hard cap) |
| `LoadProject` | 204 s | 218 s |
| link calls during the load | 910/910 `ok` | 975/975 `ok` |
| main-thread stalls > 500 ms | 1.5 s at load completion, 1.1 s at autosave | none during the load (one 554 ms at page setup) |

Both runs are far below the 30 s heartbeat, so this fix is NOT claimed to fix
F4. It removes a 10 MB clone per autosave and ~23 MB per stored copy (record plus
draft), which matters most on a slow or memory-limited device.

Still open:
- **What makes "Bike frame" 28 MB.** The probe's 6-tube document is 76 KB, so the
  bulk is elsewhere in the tree or sources. A multi-MB `featureTree` held in Svelte
  `$state` and deep-cloned per autosave is a plausible main-thread stall.
  Unmeasured: needs the exported file.
- **Tab suspension, reload or OOM on the user's browser**, which occurrence 3
  confirms happened at least once.

Next evidence needed:
- The exported 28 MB `.waffle`, loaded under the same probe.
- A browser-console capture (`[worker]` timings, crash or OOM lines) from the next
  occurrence.

---

## F5 — First `viewport_capture` after `parameters_set` + `viewport_view(iso)` was blank

**Repro context:** `viewport_view {view: "iso"}` → `parameters_set` (adds 7
parameters, no geometry change) and `viewport_capture` issued in the same
parallel batch → an 800×1024 image of the background only, with a single
bright pixel at the center. The next capture (after another `viewport_view`) was
normal. Not reproduced; likely the capture raced the rebuild re-render.

---

## F7 — After one Cut exists, every edit anywhere takes > 120 s

**Observed:** once the head-tube Cut (two cylinder×cylinder subtracts) was in
the tree, three `feature_edit`s of *Sketch* features that sit upstream in the
tree but feed only unrelated NewBody extrudes each ran past the MCP 120 s
foreground limit (moved to background; completed ~2+ minutes later). Before the
Cut, the same kind of edit returned in well under a second.

**Suspected:** full-tree rebuild on every edit (no dependency pruning), so each
edit re-runs every boolean, serialized behind the page lock.
**Not measured yet:** the single-boolean wall time for this Cut; whether the
rebuild re-runs booleans whose inputs did not change.
**Test idea:** feature-engine rebuild test counting kernel boolean invocations
when an unrelated upstream sketch is edited (expect 0 re-runs, or a documented
cache).

---

## F8 — Seat-tube cope fails: `TessellationFailed … patch triangle collapsed at render precision`

**Severity:** high — blocks the seat cluster copes (kernel capability tail, not an input error).

**Repro (model state):** tree as of the head-tube Cut (`6ef34640…`), then
`feature_add` Extrude `combine: Cut` from sketch `1fc98604…` (origin 0, unit
normal along seat-tube axis (−0.2840149556049158, 0, 0.958819850124484), one
circle r = 0.0143 via explicit precise arc region), depth 0.6 one-sided,
targets `[6ef34640…/Main (top tube, already coped at the head tube),
e097249f…/Main (seatstay L), 35a47515…/Main (seatstay R)]`.
→ after > 120 s: `FeatureRebuildFailed: operation error: kernel error: boolean
operation failed: kernel-v2 boolean_subtract failed: TessellationFailed { face:
FaceId(194), reason: "patch triangle collapsed at render precision" }`.

**Geometry of the targets (for a harness fixture):**
- Top tube: annulus R 12.7 / r 11.8 mm along +X from (−0.150531, 0, 0.508175),
  starting ON the seat-tube axis (end cap disc fully inside the tool cylinder).
- Seatstays: annulus R 8 / r 7.2 mm from (−0.390633, ±0.069372, 0.090859) along
  (0.5338869746515809, ∓0.1371549934880182, 0.8343579603855193), length
  0.465689 — end centerline at (−0.142008, ±0.0055, 0.47941), i.e. 5.5 mm off the
  tool axis; the end cap lies inside the r 14.3 tool with ≈ 0.8 mm margin.
- Tool: solid cylinder r 14.3 mm, axis through the origin along the seat-tube
  direction, axial range 0 … 0.6 m.

**Bisection:** results below as each single-target cut is tried.
- Target `[top tube 6ef34640…/Main]` alone → **fails identically**
  (`TessellationFailed { face: FaceId(218), reason: "patch triangle collapsed at
  render precision" }`, > 120 s). The top tube is the output of a previous Cut
  (it carries a degree-4 cope patch at its front end, 0.54 m away from this cut),
  so candidates: chained boolean re-entry of a cope-patch body, or the
  cylinder×annulus cut at the seat tube itself.
- Target `[seatstay L e097249f…/Main]` alone (a fresh, never-cut NewBody) →
  **fails differently, fast:** `BooleanFailed("yang-rs: Stage-4 relocation region
  around vertex 32 is invalid: LocalRefinementRequired")` — the documented
  Stage-4 relocation-wall ERROR tail (`docs/yang_tail_triage.md`). Geometry: the
  stay (R 8 mm) is coped by a r 14.3 mm cylinder whose axis passes 5.5 mm from
  the stay's end centerline at ≈ 49° to the stay axis; the stay's bore
  (r 7.2 mm) also intersects the tool, giving two nested degree-4 cope curves
  ~0.8 mm apart.
- So the seat-cluster copes are blocked by kernel capability for BOTH target
  kinds (fresh stay: Stage-4 LRR; previously-coped top tube: tessellation
  collapse). Not worked around; recorded as-is.

---

## F9 — Cutting ONE output of a multi-body Cut feature silently deletes its sibling outputs

**Severity:** critical — silent loss of an unrelated body; no error, no warning.

**Repro (model state):**
1. Cut feature A (`6ef34640…`): head-tube tool, `targets: [top tube, down tube]`
   → outputs `A/Main` (top tube) and `A/Body:1` (down tube). Both measured fine.
2. `feature_add` Cut feature B (`dd3f32d6…`): BB-shell tool (solid r 20 mm along
   Y, symmetric 45 mm), `targets: [FeatureOutput{A, Body{index:1}}]` only.
   The tool does not come near `A/Main` (top tube is ~0.5 m away).
3. Result: `bodies_removed: [A/Main, A/Body:1]`, `bodies_added: [B/Main]`,
   `errors: []`, `warnings: []`.
4. `model_summary`: no top-tube body anywhere. `B/Main` measures as the coped
   **down tube** (bbox y ±0.0159, max x 0.4246) but carries the display name
   "Top tube" (A/Main's name was transferred to B/Main).

**Expected:** only `A/Body:1` is consumed; `A/Main` stays live (or is re-emitted
as a leftover of B) with its name; the body-name map follows the consumed body,
not output slot `Main`.

**Two bugs visible:** (a) sibling outputs of the anchoring feature are consumed
though not targeted; (b) the display name follows the `Main` slot rather than the
body identity.

**Root cause (code-read):** consumption is tracked per FEATURE, not per output:
`RebuildState::consumed_features: HashSet<Uuid>` ("Feature IDs whose solid was
consumed", `crates/feature-engine/src/rebuild.rs:108-110`), filled from
`find_consumed_feature_ids` (`:1108`) and inserted at `:174`/`:217`. Consuming
`A/Body:1` therefore marks all of A consumed, and `A/Main` stops being a live
body. The Cut dispatch itself (`dispatch_combine`, `:1549+`) only produces one
result per *target*, so the untargeted sibling has nowhere to go. Fix shape:
key consumption by `(feature_id, OutputKey)`; carry the display-name map by body
identity.

**Test:** feature-engine rebuild test — two-target Cut A; Cut B targeting only
`A/Body{1}` with a tool disjoint from `A/Main`; assert `A/Main` is still in the
live body list with its name.

**Recovery used:** `feature_delete` B (restores A's outputs). Workaround: target
ALL outputs of A in B (the tool-disjoint one should pass through unchanged).

---

## F10 — Tab reload during a long rebuild → agent link reports an EMPTY model

> **CORRECTION (2026-09-15):** the user exported the stored "Bike frame" record from
> the Home view (new ⋯ → Export .waffle): it is **28 MB**, so the 00:14:10 autosave
> did NOT write an empty tree. The data-loss hypothesis below is refuted as stated.
> What remains verified: after the draft restore, `model_summary` returned
> `features: []` for minutes. `summarizeModel` (`app/src/lib/agent/summary.js`) reads
> the page's `featureTree` store, which is empty until the restore rebuild's
> `ModelUpdated` arrives, while `SaveDocument` substitutes the engine's own tree
> (`dispatch.rs:323`), which was already loaded. **Real defect:** during a
> restore or rebuild the agent link answers with an empty model instead of
> `UserBusy`, and an agent can act on that (e.g. save, re-author). Severity: high for
> agents, not data loss. The autosave-during-restore race is still a
> plausible hazard, but it is unobserved; do not "fix" it without a repro (P9).

**Original severity estimate (superseded):** critical — likely loss of the whole document (stored copy AND draft).

**Sequence (UTC, 2026-09-14/15):**
1. Last explicit `document_save`: 23:21:46. After it, ~10 successful Cut features.
2. ~00:13: tab reloads mid-call (see F4, third occurrence); `feature_add` returns
   `UserBusy … tab reloaded … reopened its last work from the browser's draft`.
3. `waffle_status`: `ready`, document "Bike frame". `document_info`:
   `unsaved: false`, one Part tab. `model_summary` (three times over ~2 min):
   `features: []`, `bodies: []`, `parameters: []`.
4. `storage_list`: the stored record `0bbcd408…` has `modified` = 00:14:10.501 —
   written AFTER the reload.

**Code path:** `restoreAutoSave` (draft branch, `app/src/lib/engine/store.svelte.js:6973-6981`)
opens the draft, then calls `scheduleAutoSave()` unconditionally. `autosaveNow`
(`:7163`) → `buildDocumentJson` (`:6819`), which composes via the engine
(`bridge.send({type: 'SaveDocument'…})`), then writes BOTH `saveDraft` (the same
per-tab draft record it restored from) and `putRecord` (the stored document).
If the engine tree is empty or incomplete when the timer fires (restore rebuild
still running, or it failed or crashed), both copies are overwritten with that
state. **Confirmed in the bridge:** `UiToEngine::SaveDocument`
(`crates/wasm-bridge/src/dispatch.rs:311-323`) replaces the active Part tab's
`features` with `state.engine.tree.clone()` whatever the UI's tab snapshot
holds, so an empty engine tree is saved as an empty document. Nothing checks "the document shrank from N features to 0".

**Not yet verified:** the stored bytes themselves (reading them means
`document_open`, deferred to avoid a further overwrite); whether the restore
rebuild failed or was still running.

**Expected / fix shape:** never autosave while a restore or rebuild is pending;
refuse, or keep a backup, when a save would drop all features of a previously
non-empty document; keep the draft being restored from until a save of a
FULLY rebuilt model succeeds (or keep N prior drafts).

**Test ideas:** store test — restore a draft whose rebuild is slow or fails,
fire the autosave timer, assert the stored record and draft are unchanged. GUI
test — kill the page mid-rebuild of a multi-boolean document, reload, assert
the feature count survives.

---

## F11 — A box Cut that does not touch a thin-wall tube fails with `VertexOffSurface` (found while testing F7)

**Severity:** unknown (kernel-v2 input; not seen in the app yet).

**Repro (kernel-v2 through `wasm_bridge::dispatch`, debug build):** sketch on
z = 0 with concentric circles r = 0.0159 and r = 0.014 → NewBody extrude of the
annulus region, depth 0.3 (a tube on the Z axis). Tool sketch at z = 0.10,
`rect_profile(0.10, −0.05, 0.20, 0.10)` (x ∈ [0.10, 0.30], so the box is
DISJOINT from the tube) → Cut, depth 0.10, `targets: [tube/Main]`.
→ `boolean operation failed: kernel-v2 boolean_subtract failed: VertexOffSurface { face: FaceId(78) }`.

**Verified:** the same error (same face ids) with the F7 change stashed, and on
`rebuild_from_scratch`, so it is not an incremental-rebuild artifact. With
r_inner = 0.015 the same disjoint Cut succeeds.

**Status: FIXED (2026-09-15).**

What was measured:
- **Reproduces in release too:** `test-harness/tests/f11_disjoint_cut_thin_tube.rs`.
  Notch Cuts through the same r 14 mm tube pass; only the disjoint Cut failed.
- **The failing check** (`KV2_OFFSURF_PROBE`): `cyl-seam-not-ruling` on the tube's
  OUTER wall (r 15.9 mm). The seam is 4.16e-12 off its axis ruling, against a
  1e-12 band.
- **Yang's output is exact** (`KV2_OUT_VERT_PROBE`): the top seam vertex v32 and
  the bottom vertex v9 at 97.605633749° have bit-identical x/y. v9 is on no edge,
  though: the two outer rims are split at different azimuths.

Cause, in kernel-v2's `recover.rs` pass 2 (typed rims → canonical
`[rim, seam, rim, seam]` lateral), which picks each seam foot per rim against a
shared `theta_ref`:
- `theta_ref` came from the coaxial BORE lateral, anchored first, at
  97.605633764°.
- The top rim reused v32, which is within the ~1e-9 `BAND` of it.
- The bottom rim had no vertex there, so it MINTED a foot at the bore's azimuth.
- The two ends of one seam were therefore 2.6e-10 rad apart, which at r 15.9 mm
  is 4.2e-12 m.

Fix: both feet of a seam share one azimuth.
- A reused foot fixes the azimuth its minted twin takes.
- Two reused feet that are not a ruling within the validator's own bound
  (`SEAM_RULING_TOLERANCE` = `CURVED_SURFACE_DEBUG_TOLERANCE`) take the existing
  arc fallback.
- No vertex moves and no tolerance changed.

The seam can still sit up to `BAND` in azimuth from the coaxial reference, so the
C0117 phase lock (`s434_typed_rim_seam_mint`) holds.

Verification:
- **`s434_typed_rim_seam_mint`:** 5/5 in release.
- **`test.sh rewrite`:** kernel-v2, yang-rs, ssi-rs and cad-primitives are green.
  cherchi-rs is red ONLY on its FFI/sidecar parity tests: this host has no
  `/home/claude/cherchi2022` clone, so the tests hit a no-op stub and
  `BinaryNotFound`. cherchi-rs does not depend on kernel-v2.
- **Release assay:** 289C / 0W / 16E / 4EE / 0T + 3 UNSUPPORTED against the
  committed 290C baseline (`b9785bf6`, 2026-09-13). The two cases that differ are
  unchanged by this fix. Single-case A/B with HEAD's `recover.rs` versus F11
  gives identical verdicts and details:
  - R0081 is ERROR with `LocalRefinementRequired` at vertex 1350 either way.
  - R0085 is ERROR with `RelocationCrossedCarrierVertex` at vertex 368 either way.
  - F11 is corpus-neutral. See "R0081 regression" below.

---

## R0081 regression — CORRECT → ERROR since the 2026-09-13 corpus baseline (found while verifying F11)

**Severity:** medium. It is a corpus regression the CI smoke gate does not pin, so
CI stays green.

**Observed:**
- **R0081:** `Revolve 3: Auto-union failed … Stage-4 relocation region around vertex
  1350 is invalid: LocalRefinementRequired`. It was "all checks passed" in the
  committed `results.json` (`b9785bf6`).
- **R0085:** its ERROR detail changed too, from two failures (Revolve 2 at vertex 386,
  Revolve 3 non-2-manifold) to one (Revolve 2 at vertex 368).

**Verified:**
- Deterministic in `single_case` (124 s).
- Independent of F11 (A/B above).

**Suspects:** commits touching the engine/kernel path after `b9785bf6`: `cf9b799f`,
`bd026d4a`, `49321a98`, `5765514a`, `133db189`, `579f831e` (F1/F3/F9: unit sketch
normals reach the revolve frame too), `df592579` (F7), `b4be84db`.

**Bisect:**
- **`798967fc`** (the last commit before today's engine fixes): R0081
  SUPPORTED_CORRECT in `single_case` (192 s).
- **Remaining candidates:** `579f831e` (F1/F3/F9) and `df592579` (F7). `b4be84db`
  only recomputes the thumbnail preview after tessellation and changes page code
  the assay does not run.
- **`579f831e`** (F1/F3/F9): R0081 ERROR, identical detail (`LocalRefinementRequired`
  at vertex 1350) → **first bad commit**.

R0081 (a gear extrude, a rectangle cut, a gear revolve; no multi-output Cut, so
F9 is not involved) points at F1 (unit-normal frames) or F3 (arc-loop outlines by
curve sampling). Its sketches are two single `Gear` entities and one 4-line
rectangle, none of which takes F3's arc-loop outline path.

**Cause: F1, confirmed by A/B.** At `579f831e` with only `unit_normal` made an
identity, R0081 is SUPPORTED_CORRECT again (190 s).

What F1 changed here:
- R0081's shared tilted sketch normal has |n| − 1 = −1.1e-16.
- `unit_normal` rewrote all three components by 1 ulp, so every frame on the
  plane moved by one ulp.

Scope across the corpus:
- 67 of 312 cases carry such a normal. All are within 1 f64 EPSILON of unit
  length, and every one is bit-stable under a second normalization.
- The F1 defect class (a 6-decimal vector) is 3×10⁹ EPSILON off.

**Fix (2026-09-15):** `feature-engine/src/rebuild.rs` `unit_normal` returns a normal
BIT-IDENTICAL when ||n| − 1| ≤ `UNIT_TO_ROUNDING` (4·EPSILON), and normalizes it
otherwise.
- This is not a kernel acceptance band. It stops F1 from touching normals that
  are already unit to f64 rounding, where renormalizing only re-rounds them.
  Existing documents reach the kernel exactly as before F1.
- **Unit tests:** `rebuild::tests::unit_normal_keeps_a_normal_that_is_unit_to_rounding`
  (R0081's normal, bit-identical) and
  `unit_normal_normalizes_a_six_decimal_normal`.
- **F1's regression tests** (`sketch_plane_normal_precision`) stay green.
- **Single cases with the fix:**
  - R0081: SUPPORTED_CORRECT (188 s).
  - R0085: back to its committed two-failure detail (Revolve 2 at vertex 386,
    plus Revolve 3).

- **Full release assay with the fix (and F11):** 290C / 0W / 15E / 4EE / 0T +
  3 UNSUPPORTED, identical case-for-case to the committed baseline (0 differing
  cases).

**Latent kernel knife-edge (OPEN):** R0081's chained revolve union stops at Stage-4
`LocalRefinementRequired` when its sketch frame moves by ONE ulp. That is the
relocation-wall class of `docs/yang_tail_triage.md`. This fix restores the input
bits; it does not remove the sensitivity.

---

## F6 — View names assume Y-up; results for the same view name differed

**Observed:** for this Z-up model, `viewport_view {view: "top"}` gives the side
profile (camera +Y, `up: [0,0,-1]`, image upside down) and `"front"` looks
straight down. `"right"` (camera at +X, `up: [0,1,0]`) once produced a side
profile image and later, with more bodies present, a top-down-looking image —
not explained; cameras recorded: first `position [1.4268, ~0, 0.2605]`,
second `position [1.7837, 5.4e-6, 0.2605]`.

**Possible test:** a Z-up fixture part; assert each named view's camera
direction and image orientation.
