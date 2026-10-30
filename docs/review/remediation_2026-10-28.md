# Remediation plan for the 2026-10-28 independent review

**Source.** `REVIEW.md` at the repo root (review of `main` `6b968527`). Section
numbers below (§1.4, §3.2 …) are that file's. **Status at drafting
(2026-10-29):** no commit has landed since the review, so every finding stands
as written; CI is green on `6b968527` (Rust tests 1 h 05 m against a 120 min
limit); disk 90 %.

**What this plan is.** The review's findings turned into ordered, checkable
items with a "done when" for each. It does **not** replace
`docs/yang_faithfulness_roadmap.md` as the kernel's plan of record. The
kernel items below either feed that roadmap (the F2 owner, the F2 scope edit)
or are result-neutral robustness work it does not cover. Fillet, chamfer and
shell stay deferred and appear nowhere here.

## 0. Ordering logic

The review's recommendations have three dependencies that set the order:

1. **Gates before fixes.** Coverage that exists but gates nothing (≈73
   test-harness binaries with always-on tests, the sketch oracles) has to run
   in CI *before* the parallel lanes start landing fixes, or those fixes land
   ungated the way 10-27's did.
2. **A geometric oracle before F2's retirements.** F2 retires repair passes
   "one per corpus run, corpus held". The corpus today grades topology and
   volume, not geometry (§2.1; R0099 was graded CORRECT 7.3e-3 off its
   surface). An F2 retirement that moves a vertex off its surface would read as
   "held". So O1–O3 land before F2's first retirement, not alongside it.
3. **One format bump, not three.** Drawing annotation ids, paper-mm placement,
   persistent body keys (§3.3), dropping double-encoded region polylines and
   grouping derived data (§4.1) are all format changes. They ship as one bump
   (P4), with the byte-determinism fix (P3, format-neutral) first so the bump's
   own diffs are reviewable.

Every item lands red-first where a test can express it, and the checkbox
gets the commit hash. Kernel items obey the P2 ledger rule as usual.

## 1. Decisions (all five ADOPTED by the user, 2026-10-29)

The user adopted every recommendation below as written ("adopt all five
recommendations and start phase 1"). The text is kept as the record of what
was signed.

- **D-1 Merge gate strength.** The review asks for a *blocking* gate. On this
  repo's commit-to-main workflow, the practical form is: the pre-push hook
  refuses a push when the latest `Rust tests` run on `main` is failed or
  cancelled, overridable only with `CI_GATE_OVERRIDE="<reason>"`, which is
  logged to `<git-common-dir>/gate-overrides.log`. Kernel-crate commits additionally need a
  recorded `test.sh rewrite` pass (≈490 s) on the branch. Recommendation: adopt
  both.
- **D-2 One owner for F2 and a cap on parallel kernel milestones** until F2
  lands (§4.3, §4.4 item 3). Parallel agents go to lanes A–H instead of F3, F5
  or F6. Recommendation: adopt; the roadmap already says F1, F3 and F5 wait on
  F2.
- **D-3 The 18 MB WASM bundle in git** (809 of 3,456 commits, 512 MiB pack).
  Options are build-in-CI plus Pages deploy (changes the CLAUDE.md rule "include
  the WASM bundle in the same commit") or Git LFS. History rewrite is a
  separate, further decision. Recommendation: build in CI and stop committing
  the bundle; leave history alone.
- **D-4 Worktree and `target/` pruning.** The plan needs permission to delete
  stale agent worktrees and `target/wt-*` directories. The rule it will follow
  is that nothing with uncommitted changes or unpushed commits is touched. In
  particular, `/home/claude/wt/agy-loft-l1b-kv2` holds the uncommitted L1b
  substrate and stays.
- **D-5 Deleting `STATUS.md`, `BACKLOG.md` and the husk directories**
  (`archive/`, `notable_plans/`, `patches/`, `reviews/`, `worker/`) rather than
  marking them historical.

## 2. Phase 1: gates and verified one-liners (week 1, no geometry)

### G: verification gates (§2.2, §4.3)

| ID | Item | Effort | Done when |
|---|---|---|---|
| G1 | ✅ Pre-push gate per D-1: a git hook `.githooks/pre-push` (binds every pusher, not only Claude Code), kernel content fingerprint `scripts/kernel_fingerprint.sh`, `test.sh rewrite/fast/full` records its pass | S | Done: dry runs block a cancelled CI run and an unstamped kernel range, pass a stamped one, a docs-only range and a non-main push; override logged |
| G2 | ✅ New CI job `test-harness` in `rust-test.yml`, parallel with `test`: the whole package in release, ignored tests skipped, corpus smoke skipped by name (no hand list) | S–M | Done: locally 99 binaries green in 483 s at 4 cores; CI result pending first run |
| G3 | ✅ Sketch oracles in CI — covered by G2 (they are test-harness binaries) | S | Done with G2 |
| G4 | ✅ Smoke-gate comment corrected (12 scenarios + 153 pins; 9 min 58 s on run 40652494180) | S | Done |
| G5 | ✅ Firefox and WebKit projects (opt-in `PW_CROSS_BROWSER=1`), allow-fail CI job `gui-fast-cross-browser` | S | Done; first-run failures to triage |
| G6 | ✅ Un-swallow the 37 GUI specs that wrap `waitForEntityCount`/`waitForFeatureCount` in `try/catch`, plus a grep gate in CI that fails on the pattern | S | Grep gate green; newly exposed reds fixed or quarantined with an issue |
| G7 | ✅ Pruning per D-4; record before/after disk | S | Done: 39 clean, merged agent worktrees and their branches removed; 19 `target/wt-*` dirs, `target/debug/incremental` and `target/debug/deps` files unwritten since 2026-10-20 deleted (none written after 2026-10-27). `target/` 254 GB → 29 GB; disk 90 % → 77 %. Kept: `wt/agy-f2-unzip-decimation`, `wt/f2-step1`, `wt/agy-loft-l1b-kv2`, `canary-y34`, `agent-a77cc34c25ff4072b` (each dirty or unmerged) |

### Q: verified quick fixes (each S, each with its red-first pin)

| ID | Fix | Review § | Pin |
|---|---|---|---|
| Q1 | ✅ Right-click Symmetric emits `entity_c`; Rust wants `symmetry_line` | §3.2-1 | GUI spec: symmetric about a line, then a second constraint still solves |
| Q2 | ✅ Circle tool drops radius ≤ 1 mm; slot resets below 1 mm | §3.2-3 | GUI: 0.4 mm-radius circle at mm units, click-click and click-drag |
| Q3 | ✅ Menu "Distance" applies 1.0 m (and `hDist \|\| 1.0`) | §3.2-3 | Unit test: default is the measured current distance |
| Q4 | ✅ Wheel zoom ignores `deltaMode` | §3.4 | Unit test on the normalizer for modes 0/1/2 |
| Q5 | ✅ Host-mode `viewport_view` drops `frame` | §3.5-3 | Relay test: `frame` reaches the viewer, result says `framed` |
| Q6 | ✅ SVG export paints are CSS variables | §3.3 | Test parses `export_svg` output and finds no `var(` |
| Q7 | ✅ Section views leak one solid per evaluate | §3.3 | 100 evaluates of a sectioned sheet leave arena and pid counts flat |
| Q8 | ✅ Same bad `filter` is `Internal` in `face_list`, `InvalidArguments` in `entity_list` | §3.5-5 | Tool test on both |
| Q9 | ✅ `rebuild.rs:293-297` decides consumption by matching the text "Auto-union failed" | §3.5-2, §4.1 | Typed flag on the result; the string match deleted |
| Q10 | ✅ `UnionAll{All}` and `PatternSeeds::All` stay stale under incremental rebuild | §4.1 | Engine test: edit a seed's sketch, union volume changes |
| Q11 | ✅ NaN coordinate reaches a predicate and panics (`orient.rs:145-146`) | §1.4 | Adapter refuses non-finite input with a typed error; test with a NaN extrude distance |
| Q12 | ✅ Two dummy `"Loft"` fixtures will collide with the real operation | §3.1 | Renamed to an obviously-fake op name |
| Q13 | ✅ Doc drift: `FILE_FORMAT.md` says v9 (code writes 15), schema golden titled v5 and CI step says v4, `relay/README.md` tool list, `TESTING.md` strict-tier claim and counts | §4.1, §3.5, §2.1 | Docs match code; no test |

### Found while doing Phase 1 (not in the review)

- **Fixed:** the sketch right-click menu could not apply anything by mouse
  (window `pointerdown` cleared the selection first; with Q1); the "Origin"
  snap label swallowed wheel zoom (`b3b72050`); a third metre floor, the
  slot's 0.5 m default width (with Q2); `pipeline.spec.js` save/load read the
  pre-multi-tab document shape (with G6); `Script` nodes whose argument-less
  `union_all()` is `UnionTargets::All` had the Q10 staleness too, as did
  `PatternMirror` (with Q10).
- **Open, red on main before Phase 1** (outside gui-fast): `sketch-constraint-operations.spec.js:72`,
  `sketch-polyline.spec.js:88`, `sketch-snap-click-bug.spec.js:35` (samples a
  point 5 m off-screen), `sketch-snap-click-regression.spec.js:292`, five
  `viewport-advanced.spec.js` zoom/orbit tests.
- **Open, red on main (found 2026-10-30, outside gui-fast):**
  `offset-plane.spec.js:198` (the extruded body's bbox centre misses the
  off-centre rectangle by more than 1e-4) and `planetary-gear.spec.js:79` (the
  extrude never reaches the feature count, 20 s timeout). Both fail with the
  spec, the WASM bundle and the app all at HEAD 02bab531+ (bisected while
  landing icon step 1), so they predate that work.
- **Fixed 2026-10-30 (test race):** relay-tier `agent-documents.spec.js:142`
  (S3) read `unsaved: false` in 3 of 5 relay runs since e9ff8bc9, after about
  37 green runs. Mechanism: the sketch's ModelUpdated arms the 3 s autosave;
  when the box's extrude outlasts it, the autosave fires mid-extrude, its
  `SetDocumentMeta` queues behind the extrude in the worker, the extrude's
  ModelUpdated re-arms the timer, and the compose then restores the timer it
  found, "not pending" (`buildDocumentJson`, by design: a save must not dirty
  the document). The following `SaveDocument` stores the box, so `false` was
  the true answer and no edit was lost. Slower extrudes on the loaded runner
  since the icon commits made it common. The test now stores the box with
  `document_save`, which answers only after any earlier compose has passed
  that point, then makes one quick user edit (a document variable) and checks
  inside that edit's 3 s. The previous note's "every ModelUpdated reschedules,
  so the window is under 1 s" was right but missed the compose already in
  flight.
- **Open:** drawing specs flake under 2–3 workers with "Execution context was
  destroyed … navigation" (baseline too; Vite re-optimization suspected, per
  the Playwright config comment).
- **Open, small:** the menu's "Length Ratio" applies 1.0 instead of the
  measured ratio.
- **Open, optional:** ~20 test-harness readers still match the text
  "Auto-union failed"; the text is unchanged, so they work, but they could
  read the typed `Diagnostics::auto_union_fell_back` flag.
- **Q11 diagnosis corrected:** on the four entries probed (extrude depth,
  profile point, transform, revolve angle) a NaN did NOT panic; inner
  checks refused it with an untyped `KernelError::Other`. No path reaching
  the cherchi-rs predicate was found. The fix is still the contract the
  review asked for: every adapter entry taking caller geometry refuses
  non-finite input as the new typed `KernelError::InvalidInput`.
- **Q7 side effect (good):** a released section cut rewinds the arena, so
  re-cutting mints the same pids and section-view cache keys stop drifting.
- **Open, small:** `test-harness/tests/section_corpus_oracle.rs` does not
  release its cuts (test-only).
- **Environment:** `uv` is not installed on the dev box, so
  `scripts/test.sh relay` cannot run locally (the suite runs from
  `relay/.venv`).

## 3. Phase 2: make "corpus held" mean geometry (weeks 1–2)

Blocks F2's first retirement (see §0 point 2).

| ID | Item | Effort | Done when |
|---|---|---|---|
| O1 | ✅ Harness on-surface oracle in `categorize`: every output loop vertex within the band of its own face's surface and of every adjacent face's surface, read through `KernelIntrospect` plus a surface accessor. Lands **record-only** first: one full corpus run census, then promoted to a verdict | S–M | R0099 at the pre-T8 commit categorizes WRONG; census of the current corpus recorded in `docs/yang_tail_triage.md` before promotion |
| O2 | ◐ record-only: Exact-versus-mesh volume: `kernel_v2::mass::solid_volume` against `mesh_signed_volume` within the chord band | S | Pin: an inverted patch below 0.5 % of body volume categorizes WRONG |
| O3 | `oracle::check_mesh_on_surfaces`: render vertices on their surface, centroids within tessellation tolerance | M | Wired into `categorize` after O1 |
| O4 | ✅ Strict validation tier tests planar loop vertices in release too (`validate/faces.rs:158-175`), and the cylinder lateral's inner loops | S | Or the doc says exactly what the tier checks; decide in the commit |
| O5 | ◐ record-only: Wire `volume_monotonicity` through `truncate_ops` prefix replays in the full run | M | A meta that violates it categorizes WRONG |
| O6 | ✅ Annihilated chain: a disjoint Intersect categorizes as a typed verdict, not CORRECT | S | `tests/prospect.rs` pin |
| O7 | ✅ Determinism tests compare a canonical geometry hash across three runs and `cherchi-rs/parallel` on/off | S | Both determinism binaries upgraded |
| O8 | ✅ Always-on `assay_metamorphic.rs` (rigid motion, inclusion–exclusion, commutativity on the 12 smoke scenarios plus ~20 cheap cases) and `assay_generative_pairs.rs` (20 two-body `strats_v2` cases) | S–M | In G2's job |
| O9 | ✅ Prospector dedupes by failure *site* rather than text (`prospect/mod.rs:343-375`) | S | Two cases with one site and different text collapse |

### Phase 2 results (2026-10-29)

- **Canonical corpus is now 357C / 3W / 9E / 5EE / 0T over 374.** The three
  WRONG cases are not regressions: R0015, R0026 and R0050 always shipped
  `LineSegment` chords bounding torus or cylinder faces (ledger N100 item 3,
  the `recover.rs` bail). O1 made them visible. Under CLAUDE.md's rule a
  WRONG preempts other kernel work, so they now outrank F2's next step.
  **Converted 2026-10-30** (corpus 360C / 0W / 9E / 5EE / 0T): the chords
  are now typed at Stage-5 emission from the two owner surfaces (ledger N60
  amendment, N104); F2 is next again.
- **O1 band** came from a record-only census: 357 clean cases ≤ 3.5e-10 of
  scale, the three defects ≥ 1.4e-3; band = Yang's d_p = 1e-7 relative.
  Red first: R0099 at the pre-T8 commit reads 3.39e-2 m off.
- **O2 stays record-only.** Its reading now carries the chord bound
  `|mesh − exact| / (tol · area)`. Over the 208 measurable CORRECT cases, two
  exceed 1: **F0054 (7.3)** and R0042 (1.01) — to investigate. **148 of 356
  CORRECT cases cannot be measured** (exact volume or area unavailable,
  e.g. torus faces): an integrator coverage gap.
- **O5 stays record-only.** 1,016 steps checked, 47 not covered, **35
  violations, every one a step whose exact volume did not change at all**,
  all in R-series generated metas. The lattice already agrees with the
  kernel's volume on these, so the likely fault is the generator's authored
  direction for ops that miss the body. Next: adjudicate each against the
  exact-membership chain before any verdict.
- **O6** caught an annihilated chain; the canonical run then showed P0023
  (two cubes meeting on an edge, `expected_solid_count: 0`) needs its empty
  result read as authored — fixed and pinned.
- **O4:** planar loop vertices are now checked in release under
  `strict-validation`; no verdict moved (agent's own full corpus run).
- **O7/O8 findings (quarantined with `#[ignore = "FINDING …"]`):** P0023's
  touching operands fail `boolean_union` with `InvalidBooleanOutput` in both
  orders; the exact-membership lattice disagrees with the kernel under rigid
  motion on C0059 and C0061 (the known rotation-dependence of the lattice on
  revolves). No non-determinism anywhere, serial or `cherchi-rs/parallel`.
- **O9:** findings regroup 64 → 53 across seeds 2–5. The true raising site
  never reaches the harness (errors arrive as strings); carrying it needs a
  typed site on the kernel error — a kernel change, not done here.
- **Not done in O4:** the cylinder lateral's inner-loop check.
- Agents in isolated worktrees cannot use `reserve -m sequoia-server` (their
  worktree guard refuses commands that run git in another checkout).

**Policy note for the roadmap's scoreboard:** once O1 is a verdict,
"corpus held" means category *and* on-surface; the roadmap's §2 gets one line
saying so.

## 4. Kernel lane: F2 with one owner (weeks 2–4)

The work list is the roadmap's F2 checklist; this plan adds only what the
review found missing from it.

- [ ] **K1. Name the small-loop splice in F2's scope.** Add to the F2 checklist:
  re-site `stage4_small_loop.rs`'s pre-arrangement local-chart CDT splice and
  its second Stage-2 run (N91 amendment S9) into the one §4.4.1 update. Without
  this line it becomes permanent (review §1.2 D8).
- [ ] **K2. F2 proceeds in the roadmap's order** under D-2, each retirement
  judged with O1–O3 in force.
- [ ] **K3. `LocalSurfaceChart` and `GaussMap` are recorded as F5's first
  consumers** in the F5 section, so the generic surface interface absorbs
  them rather than re-homing them (review §1.3).

Result-neutral robustness work that does not touch mechanisms and may run in
parallel with F2 (each needs the ledger only if it changes a result):

| ID | Item | Review § | Effort | Done when |
|---|---|---|---|---|
| K4 | Feature-gate the census probes and every `ASSAY_CASE` read out of release and WASM | §1.5, §4.2 | S | `grep ASSAY_CASE` in kernel crates only under `#[cfg(feature = "census")]`; WASM size recorded before/after |
| K5 | N100 item 8: delete or register the four release-active switches (`CHERCHI_GRAZE_AWARE_RAY`, `CHERCHI_SCE_SHORTCIRCUIT`, `YANG_S1_SELF_CONTACT`, `_CHART`) | §1.4 | S | Probe registry lists every remaining env read |
| K6 | N100 item 3: `kernel-v2/src/recover.rs:254-270` ships a curved rim as a chord silently; make it loud | §1.4 | S | Typed error; pin — **DONE 2026-10-30**: `validate_boolean_output_chords`, `ChordOffCurvedSurface`, `kernel-v2/tests/k6_boolean_output_chords.rs` |
| K7 | ~60 production `expect`/`unwrap` in yang-rs become typed `YangError` returns or carry an invariant comment at the site | §1.4 | M | `clippy::expect_used`/`unwrap_used` denied in yang-rs non-test code, with allows only on documented sites |
| K8 | 141 distinct `1e-N` literals in yang-rs production code become named bands from `units.rs` (`TAU_EVAL` etc.) | §1.4 | M | Byte-identical corpus; a lint script fails on new bare literals |
| K9 | Each F4 tracer constant (`stage4_small_loop.rs:68-280`, `stage4_tangency.rs:47-186`) gets a paper cite or a measured rationale, or is ledgered | §1.3 | M | Done alongside K1, not before |
| K10 | Result-neutral complexity fixes: BVH for `detect_improper_contacts`, no per-removal `compute_phase_a` recompute | §1.5 | M | Byte-identical corpus; timing recorded |
| K11 | Test rule: pin through the public mechanism (the sweep, the loop, `boolean_op`), never only through its helpers | §1.2 | S | Added to `docs/TESTING.md` and `crates/yang-rs/CLAUDE.md` |

Deferred until F2 lands (review §4.4 item 6): F5 step 3, F3, F6 unification,
L3 local recompute, and the f32 render-origin change (§1.4).

## 5. Parallel lanes (weeks 2–4, non-kernel)

### A: agent-facing failure reporting (§3.5)

| ID | Item | Effort | Done when |
|---|---|---|---|
| A1 | Yang STOP classes map to distinct `engine_error.kind` variants with face pids in `details`, replacing `KernelFailure { kernel: String }` | M | Tool test: `RefinementExhausted`, the coplanar wall and `TessellationFailed` are distinguishable without text |
| A2 | Typed `BooleanNoEffect` and `BodyConsumed` outcomes; disjoint Cut is no longer `isError: false` with no warning | S–M | Three silent paths are typed in tool tests |
| A3 | Fix `body_measure.closed` for boolean outputs (open since 09-17) | M | `closed` true on a bored extrude and a union |
| A4 | `body_validate` tool: closed, manifold, Euler, self-intersection, typed | M | Shares O1's surface accessor where possible |
| A5 | `batch` tool: ordered steps, all-or-nothing, one undo step, on the `author.rs` snapshot machinery | M | Mid-batch failure leaves the tree untouched |
| A6 | Richer `entity_list` predicates (radius, axis, length, adjacency, owning feature) | M | After A1–A5 |
| A7 | Correct the project memory's "≤ 4 calls then save" rule against the 09-14 probe | S | Memory updated |

### S: sketcher (§3.2), after Q1–Q3

| ID | Item | Effort | Done when |
|---|---|---|---|
| S1 | Dimensionless verdicts: residuals normalized to length; `SOLVE_TOL`/`MOVED_EPS` relative to sketch extent | M | Scale sweep at 1 µm, 1 mm, 1 m and 100 m detects a 0.1° parallel/perpendicular/tangent error at every scale |
| S2 | One angle convention: UI and solver agree on signed angles with a ±π wrap | S–M | Clockwise-pair angle test |
| S3 | Over-constraint colouring reads the solver's verdict instead of counting references | S | A fully constrained corner is not red |
| S4 | One constraint contract: JS builders generated from `engineSchemas.generated.js`; `mapConstraintForBridge` deleted | M | — |
| S5 | Cheap report: minimal conflicting set from one factorization; drop the super-cubic redundancy walk; coalesce drag solves | M | 100-point sketch solve timed before/after |
| S6 | JS-versus-Rust profile parity test over `app/tests/cases/sketch/`, then retire `sketch/profiles.js` with U4 | S then M | — |

### D: drawings (§3.3)

| ID | Item | Effort | Done when |
|---|---|---|---|
| D1 | Anchor identity: measure against the unsplit `project_edges` curve, add a piece/side discriminator for silhouettes | M | A shaft diameter across two silhouettes and a partly hidden edge both dimension; a dimension survives a later occluding feature |
| D2 | Multi-body occlusion in `Occluders` | M | Two bodies, the far edges `Hidden` |
| D3 | Annotation UUIDs, paper-mm `Placement2`, persistent body keys in `ViewSource.bodies` | M | Ships inside P4 |
| D4 | DXF gets a dashed `LTYPE` for HIDDEN; decide whether layout moves to Rust before attempting DXF dimensions | S then decision | Golden DXF |

### P: parametric model and file format (§4.1)

| ID | Item | Effort | Done when |
|---|---|---|---|
| P1 | Unit-pin the rebuild decision and the combine fold in feature-engine (the modules with zero `#[test]`) | M | Q10's bug is pinned at unit level too |
| P2 | Feature status instead of "error, no result, continue": positional consumers stop silently binding the previous body | S–M | Follows A2 |
| P3 | Byte-deterministic saves: `BTreeMap` for `body_names`, `provenance`, `body_materials`, `solved_positions`; sorted serializer | S | Save twice, identical bytes; format-neutral |
| P4 | **One format bump:** drop `outer`/`holes` when `outer_edges` exists, group derived blocks under one discardable key, and D3's fields | M | `Bike frame.waffle.json` re-saved and size recorded; reader floor documented |
| P5 | Rewrite `docs/PERSISTENT-NAMING.md` from `resolve.rs`, after reading Kripac 1997, Capoyleas–Chen–Hoffmann 1996 and Marcheix–Pierra 2002; add them to `REFERENCES.md` | M | `BestEffort` `KindFallback` binding is either removed or documented as a deliberate policy |

### U: GUI (§3.4), behaviour-neutral first

| ID | Item | Effort | Done when |
|---|---|---|---|
| U1 | Move `window.__waffle` to `engine/testApi.js` behind a DEV/test gate | S | Production build has no `__waffle` |
| U2 | Honest undo: "nothing to undo" toast; assembly history via the drawing snapshot pattern; agent cancel compares the whole document, not only the feature tree | M | Undo matrix over every mutating message family |
| U3 | Mirror discipline: `activeTabId` written only from engine answers | S | Mirror-invariant test after a failing `switchTab` |
| U4 | Store split, first two cuts: dialogs out, then sketch session out | M | No behaviour change; gui-full green |
| U5 | Remove the nine stale `fixme`s in `boolean-dialog.spec.js`; un-quarantine the drag-arc test | S | — |

### L: loft substrate, before any loft code beyond L1b (§3.1)

| ID | Item | Effort | Done when |
|---|---|---|---|
| L1 | Full-patch integrators (`bspline_surface_signed_volume_flux`, `bspline_surface_area`, `add_bspline_surface_face`) refuse a trimmed face with `CurvedGeometryMismatch` | S | Trimmed-face volume refusal test; lands in the L1b worktree before it merges |
| L2 | `specs/loft.md` §2.2 item 4 and §6 Q2 lose the `W_MIN` band and the centripetal retry, matching the Decisions block | S | — |
| L3 | Move normal, closest point and surface knot refinement into `nurbs-rs`; add homogeneous-R⁴ interpolation | M | kernel-v2 calls `nurbs-rs` for them |
| L4 | Seam topology from `he.twin`, not a 1e-12 coincidence test | S | — |

### H: repository and documentation hygiene (§4.2)

| ID | Item | Effort | Done when |
|---|---|---|---|
| H1 | `README.md` status rewritten; `STATUS.md`, `BACKLOG.md` per D-5; `AGENTS.md:110` stops naming `WaffleKernel` | S | — |
| H2 | `specs/README.md`: plan-of-record table, superseding the March `specs/STATUS.md` | S | `docs/INDEX.md` points at it |
| H3 | Regenerate `INTERFACES.md` §G from `traits.rs`; fillet/chamfer/shell become default-`NotSupported` trait methods and the stubs go | S | — |
| H4 | Root clutter: stray scripts to `scripts/`, tracked repro fixtures to `app/tests/cases/` or deleted | S | — |
| H5 | `default-members` excludes the dev-only sidecar crates | S | — |
| H6 | One union fold, one Main/Body rekey across modeling-ops and feature-engine; one `PreviewMesh` | M | — |
| H7 | WASM bundle per D-3 | S–M | — |

## 6. Explicitly not in this plan

- Speed-ups beyond the result-neutral ones in K10. The user's rule is "slow and
  correct first".
- Container change for `.waffle`. The review says to wait for the first binary
  payload.
- A multi-agent MCP design, a fastener library, `kicad_board_get`. These are
  real gaps (§3.5) but are feature work, not remediation.
- Replacing truck as the STEP reader.

## 7. Progress log

| Date | Items | Commit |
|---|---|---|
| 2026-10-29 | Plan drafted | 373f056a |
| 2026-10-29 | D-1…D-5 adopted; G1 push gate; G7 pruning (disk 90 % → 77 %); D-1/D-2 rules in CLAUDE.md and GEMINI.md | ecbc2bda |
| 2026-10-29 | G5 cross-browser; Q12 fixture rename; Q13 + G4 doc corrections; G2/G3 test-harness CI job | git log |
| 2026-10-29 | Q1–Q6 + G6 (app agent, `29a01b92`…`0537b338`); Q9 + Q10 merged with the WASM rebuild | this commit |
| 2026-10-29 | Q7 + Q8 + Q11 merged (rewrite tier 3605 passed on the kernel content) with the WASM rebuild. **Phase 1 complete.** | this commit |
| 2026-10-29 | Phase 2: O1, O4, O6–O9 landed; O2, O5 record-only; corpus 357C/3W/9E/5EE/0T (N100 item 3 now measured) | 647a4736 |
| 2026-10-30 | The three on-surface WRONG rows converted (Stage-5 emission typing, N60 amendment, N104); corpus 360C/0W/9E/5EE/0T with O1 in force. K6 (loud recover bail) still open — no measured case now reaches it | this commit |
| 2026-10-30 | K6: `validate_boolean_output_chords` refuses a `LineSegment` edge leaving its curved face (`ChordOffCurvedSurface`, N100 item 3 CLOSED). Canonical corpus 360C/0W/9E/5EE/0T, zero verdict moves, gate fired on no case (12 jobs, wall 479 s). Also: smoke pins followed the R0015/R0026/R0050 conversions (fa7c836e) | this commit |
