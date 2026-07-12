# Kernel Design Review — 2026-07-12

**Scope:** the kernel stack (`cad-primitives`, `waffle-types`, `cherchi-rs`, `ssi-rs`,
`yang-rs`, `kernel-v2`), its seams and test infrastructure, and the governance
documents themselves. GUI, sketch solver, and app-layer review deferred to a later
pass per the maintainer.

**Method:** seven parallel independent review agents (governance consistency,
kernel-v2 design, yang-rs pipeline accretion, cherchi-rs/predicates, contracts &
layering, test-suite health, tolerance/epsilon audit) plus a seams review
(ssi-rs, yang↔kernel-v2). Every High-severity single-source claim was
independently re-verified against the tree before inclusion. Baseline at review
time: HEAD `3568db09`, assay 241C/0W/50E/4U/0T, `./scripts/test.sh rewrite`
green (2134 passed / 0 failed / 12 ignored).

---

## 1. Overall verdict

**The principled design has NOT fractured.** This was the central question —
whether five weeks of corpus-case-driven fixes turned the paper-following
architecture into an accretion of hacks — and the evidence says no, on every
axis the constitution cares about:

- **No P9 violations found anywhere in the stack.** No tolerance was ever
  widened over time (verified by git archaeology on every suspicious value).
  No silent fallback converts a geometric failure into an approximate success.
  All 116 corpus-case-ID references in yang-rs production code and ~62 in
  kernel-v2 are provenance *comments* on general handlers — zero appear in
  live conditional logic. There is no `if case == R0091` anywhere.
- **Topology is decided exactly.** cherchi-rs's three-tier predicate cascade
  (f64 filter → interval → exact rational) has zero epsilon leakage into any
  exactness-documented path; the only epsilons are provably-sound filter
  bounds that can only fall through to exact arithmetic, never change an answer.
- **One tolerance philosophy, three physical tiers** (exact / TAU_WORK /
  TAU_MODEL, with scale-aware derived bounds as loud reject-guards). Not N
  improvised models.
- **The layering is real.** Compiler-enforced via Cargo deps; greps for
  `use kernel_v2::` / `use yang_rs::` / `use cherchi_rs::` outside the kernel
  stack return zero hits. No geometry-type duplication (one Point3, in
  cad-primitives). The three.js/WASM boundary is intact.
- **Visible P10 culture:** banked-but-deliberately-unwired handlers with
  written rationale, refusal-to-force guards, "derived, not widening"
  annotations throughout.

**What HAS degraded is the bookkeeping that keeps the design auditable, and
one structural seam.** The deviation log stopped recording three days ago
mid-campaign; the tolerance vocabulary exists but hasn't been *named* in one
place (an unnamed 1e-9 tier lives as ~80 inline literals); two documents that
governance points to as sources of truth (`waffle-types/units.rs`,
`specs/ssi_solver_matrix.md`) describe the deleted legacy kernel; several
load-bearing correctness oracles run in no CI tier; and the yang→kernel-v2
return seam leaks untyped chord runs that kernel-v2 repairs after the fact —
a small but real instance of the repair-layer pattern A15 warns about.

None of this requires redesign. It requires a bookkeeping-and-consolidation
pass **before** the final assay push, because each of these gaps is exactly
the kind of soil hacks grow in later.

---

## 2. Findings — Priority 1 (act before the assay push)

### F1. Production `validate_solid` never checks face planarity for boolean outputs — HIGH
**Evidence:** `crates/kernel-v2/src/validate.rs:40-48` (rationale), `:699`,
`:1022` (`#[cfg(debug_assertions)]` gates); `error.rs:284-288` ("not a
production correctness gate").
**Why:** the rationale — "planarity is guaranteed by construction" — is true
for Euler-op constructors but **false for boolean outputs**, which re-enter
from the yang mesh pipeline carrying real-scale f64 noise. Production
`validate_solid` checks Newell *orientation* only, not vertex *coplanarity*,
so a "planar" face with off-plane loop vertices passes in release and the
defect surfaces downstream as tessellation self-intersections. This is
precisely the active F0064/R0051/F0067 class (task #146).
**Recommendation:** promote a scale-relative planarity/on-surface residual
check to production tier **for solids arriving via the boolean path** (keep
debug-only for constructor outputs). Task #146's whole class then fails loudly
at the validation boundary instead of deep in tessellation.

### F2. Load-bearing correctness oracles run in no CI tier — MEDIUM-HIGH
**Evidence:** `grep predicate-gen scripts/test.sh` → no hits;
`crates/predicate-gen/tests/generator.rs:225` (`checked_in_file_is_fresh`) is
the only guard that the 11,958-line `generated.rs` still equals generator
output, and the same suite holds the clean-room constant cross-checks against
FPG's/Cherchi's published filter bounds. Separately,
`cherchi-rs/tests/r0046_patch_label_parity.rs:142` and
`tests/stage0_operand_inputcheck.rs:116` are `#[ignore]`d "binding reference"
sidecar-parity oracles that **no tier ever runs** (`run_cargo_test` never
passes `--ignored`).
**Why:** a hand-edit to `generated.rs` — which its own header and CLAUDE.md
forbid — passes every tier unnoticed. The parity oracles CLAUDE.md declares
"not optional" are dormant unless a human remembers. This is how a port
silently diverges from its reference.
**Recommendation:** add `predicate-gen` to `RUST_REWRITE_CRATES`
(scripts/test.sh:41 — one line); either un-ignore the dormant parity tests
behind a sidecar-present check (the flagship `parity_native_vs_sidecar.rs`
already refuses to self-skip — extend that posture) or add a `parity` tier
that runs `-- --ignored`.

### F3. Tolerance constants defined twice; the file claiming A14 authority is 96% dead — MEDIUM-HIGH
**Evidence:** `TAU_MODEL`/`TAU_WORK`/`MIN_FEATURE_SIZE` defined in BOTH
`cad-primitives/src/lib.rs:23,27,31` (the live source) AND
`waffle-types/src/kernel/units.rs:11,19,24`, two crates with no dependency
edge between them; kernel-v2 links both. `units.rs`'s header claims to be
"the single source of truth for tolerance policy (A14)" yet ~48 of its ~50
constants have zero live uses — including `STITCH_ESCALATION_FACTORS =
[100.0, 500.0, 2000.0, 5000.0]` (`units.rs:336`), the literal
tolerance-escalation anti-pattern that killed the legacy kernel, deprecated
but still in the tree.
**Why:** an A14.3 violation by construction. The two TAU definitions agree
only by luck; tuning one during a campaign and not the other splits the weld
grid between the mesh side and the contract side with no compile error. And
the anti-pattern constant should not be findable in a live tree.
**Recommendation:** make cad-primitives the single owner; have waffle-types
depend on it and `pub use` the constants; delete the dead ~48 constants
(`STITCH_ESCALATION_FACTORS` first); repoint the A14 header comment.

### F4. The deviation log lapsed for the entire recent campaign — HIGH (process)
**Evidence:** `docs/yang_deviations.md` newest entry is N24 (2026-07-08);
zero entries for tasks #131–#146 (torus rim crossings, fused-emission
collapse, rim-override merge, re-entry CDT zigzag, circle×pp-line junction,
KV16b, sphere/torus revolve, M8 slices f/g). The N2 entry cites
`lib.rs ~2260-2479` — locations that no longer exist post-decomposition — and
still reads "Sign-off: pending" although the recent junction-closed-form
campaign *is* the N2 remediation.
**Why:** the log is the governance-mandated ledger that makes "the paper IS
the spec" auditable. Without it, a future session cannot distinguish the ~15
Stage-4 class handlers that are paper-faithful from locally-justified
departures without re-deriving each one. This is the concrete mechanism by
which principled work becomes indistinguishable from accretion.
**Recommendation:** one catch-up PR: refresh N2 (new anchors, status
"partially remediated", enumerate shipped handlers), add short entries for the
#131–#146 mechanisms and the new tolerance decisions (`2·d_ε/sinθ` corridor,
`1e-13` floor, `8εL` certificate). Then re-enforce the per-task logging rule.

### F5. `specs/ssi_solver_matrix.md` (A15.4's source of truth) describes the deleted legacy implementation — HIGH (docs)
**Evidence:** the spec claims 12/15 pairs "done" including all torus pairs,
citing `Degree4*` curve variants and `crates/kernel/src/ssi/` line numbers.
Live reality: `ssi-rs/src/lib.rs:97` `QuadricSurface` has **no Torus**
(":96 — `Torus` arrives with its solver"), no `Degree4*` variants exist, and
degree-4 general position is `SurfacePair` or a typed `Err`.
`governance/ARCHITECTURAL_INVARIANTS.md` A15.4's own provenance note admits
the table is behind and asks for a re-audit "as M5 lands" — M5 landed
2026-07-08. `ARCHITECTURE.md:187` separately claims "all 15 integrated."
Three documents, three different answers.
**Why:** anyone planning SSI work from the matrix believes capabilities exist
that are unrepresentable in the current crate.
**Recommendation:** rewrite the matrix from the actual `intersect` dispatch;
re-audit the A15.4 table (its note already requests this); fix
ARCHITECTURE.md's claim. Until then treat the spec as void.

---

## 3. Findings — Priority 2 (architectural debt, schedule deliberately)

### F6. The yang→kernel-v2 return seam leaks untyped rims; `recover.rs` is a consumer-side repair layer — HIGH (architecture)
**Evidence:** `yang-rs/src/stage5_topology.rs:419` — edges not in
`intersection_curves` exit as untyped `LineSegment` chord runs, so a solid's
own surviving circular rim that got Steiner-subdivided degrades on output.
`kernel-v2/src/recover.rs:1-40` re-derives circles from the exact surfaces
after the fact — re-implementing ssi-rs's plane∩cylinder closed form
(`ssi-rs/src/lib.rs:615-624`) which kernel-v2 cannot reach through the
layering — and covers only that one curve class, silently passing chorded
ellipse/hyperbola/torus rims on bail (an A15.5 surface-tier erosion that
nothing reports). Related: `kernel-v2/src/boolean.rs:1066-1257`
(`canonicalize_vertices_to_planes`) runs exact-rational repairs on yang's
crooked output vertices — more consumer-side fixing of producer noise.
**Why:** this is the repair-layer pattern A15 exists to prevent, in embryo.
The paper types output boundaries at emission (they ARE surface∩surface);
yang has the surfaces and the tessellation map in scope; kernel-v2 does not.
The team already understands this at the yang level — the disjoint-union
passthrough (`yang-rs/src/boolean.rs:1771`) preserves curve vocabulary
bit-for-bit precisely to dodge this loss.
**Recommendation:** move rim re-typing into yang Stage 5/6 (it can call
`ssi_rs::intersect` legitimately); shrink `recover.rs` to a validation
assertion; evaluate pushing vertex canonicalization into yang output. As an
interim safety: make the `recover.rs` bail path loud (diagnostic when a
curved-geometry solid falls back to chords), per A8.2.

### F7. `kernel-v2/boolean.rs` hosts a duplicate topology validator — HIGH (maintainability)
**Evidence:** doc header claims thin conversion (`boolean.rs:1-91`), but
`from_yang_brep_indexed` (~1000 lines, `:1523-2528`) re-derives manifold
twin-pairing, Newell orientation, and Euler-Poincaré genus — then the result
*also* runs `validate_solid` (concession at `:46-58`). The pass-1 copy is
where special cases accrete (bigon admissions `:1713-1754`).
**Recommendation:** decompose into `boolean/{convert_in,convert_out,recover,
canonicalize}.rs`; assemble-then-validate-once against `validate_solid`.

### F8. Stage-4 tolerance decisions fragmented across sites — MEDIUM-HIGH
**Evidence:** two different work floors for structurally identical certificate
bands — `TAU_WORK.max(8εL)` (`stage4_relocate.rs:95`) vs a magic
`1e-13.max(8εL)` (`:268`, comment admits the value was chosen to keep shipped
torus behavior byte-identical, and it is 10× tighter than TAU_WORK); two
tangency cutoffs 1000× apart for the same divergence guard
(`MIN_FEATURE_SIZE` at `:498` vs `1e-3` at `:564`); the corridor formula
`2.0*d_eps/sin_theta` copy-pasted at `stage4_correct.rs:1782,2032,2128,2621,
2815`. Stack-wide, an unnamed **1e-9 tier** (~80 inline literals in yang-rs +
kernel-v2 recover/adapter/boolean) governs retag/fuse/junction certification;
yang-rs's own `MATCH_TOLERANCE=1e-9` (`brep.rs:75`) is used only in tests
while production re-inlines the literal.
**Why:** the formulas are principled (the corridor is the paper's §4.4.2
projection band), but per-site thresholds are the exact place where a
principled algorithm silently acquires case-specific tuning. Five copies of a
tolerance formula = five places a future correction must find.
**Recommendation:** (a) extract one `tangent_plane_band()` helper named after
the paper construct; (b) add a `stage4::tol` module with named constants
(`TANGENCY_MIN_SIN`, `RELOC_WORK_FLOOR`) derived from central TAU_*, each
divergence justified in a deviation entry; (c) promote the 1e-9 tier to one
named cad-primitives constant (e.g. `TAU_EVAL`, "f64 construction/
normalization rounding band") and replace the ~80 literals.

### F9. God modules in kernel-v2; Stage-4 dispatch monolith in yang-rs — MEDIUM
**Evidence:** `kernel-v2/tessellate.rs` 6,340 lines (one 1,040-line fn,
`tessellate_developable_patch` `:2604-3638`, five internal passes; shared CDT
core `:2062-2500` not isolated); `boolean.rs` 4,243; `construct.rs` 3,311
with byte-identical seamed-lateral assembly triplicated
(`:2547-2619`/`:1167-1240`/`:2109-2185`) and the revolve axis-frame
triplicated (`:441-560`/`:922-972`/`:1989-2058`). `yang-rs/stage4_correct.rs`
4,319 lines: ~15 per-vertex class maps, a 63-arm relocation dispatch.
**Why:** same shape yang-rs's lib.rs had before its (successful, move-only)
decomposition; at this size the "single canonical tessellator" claims are
enforced only by comment, and triplicated wiring drifts independently.
**Recommendation:** repeat the proven yang-rs recipe: move-only splits
(`tessellate/{sampling,caps,quadric_lateral,uv_patch,cdt,developable,planar}`,
`construct/{extrude,revolve,…}`, `boolean/` per F7); extract
`assemble_seamed_lateral()` / `revolve_axis_frame()` (~190 duplicated lines);
group Stage-4 `vert_*` class maps behind an enum/table so the dispatch reads
as one algorithm over N surface-pair kinds. Do each as its own commit with
byte-identical assay verification, as before.

### F10. Degree-4 representation inconsistent inside ssi-rs — MEDIUM
**Evidence:** three general-position degree-4 solvers return the blessed M5
`SurfacePair` (`lib.rs:1323,1450,1596`); the two sphere pairs (sphere×cyl
`:1063`, sphere×cone `:1185`) still return
`Err(AnalyticalSolutionNotAvailable)` with pre-M5 "staged" comments; the
error's own doc (`:225-228`) still says it is "not triggerable."
**Why:** an offset sphere×cylinder boolean loudly fails where the
mathematically equivalent cyl×cyl succeeds; the hole is exactly the pairs a
sphere touches.
**Recommendation:** promote both sphere NC branches to `SurfacePair`
(kernel-v2/yang already carry `PairSurface::Sphere`), or document the
exclusion; fix the stale comments either way.

### F11. Junction closed-forms accreting without a shared primitive — MEDIUM
**Evidence:** hand-rolled line∩sphere quadratics, line∩plane-of-circle,
ellipse- and circle-junction closed forms scattered through
`stage4_relocate.rs:981`, `stage4_correct.rs:1643-1665,2528-2588`, added one
per task.
**Why:** each new junction class mints another bespoke quadratic to get
exactly right; no shared tested primitive to compose. (Charter note: these
are curve∩curve ops, distinct from ssi-rs's surface∩surface — consolidation
target is a yang-internal module, not ssi-rs.)
**Recommendation:** consolidate into one tested "analytic curve intersection /
point-on-curve" module before the next junction increment.

---

## 4. Findings — Priority 3 (hygiene; batch opportunistically)

### F12. ~117 env-gated probes in production src; ≥3 behavior-altering neuters ship in release — LOW-MEDIUM
`stage1_tessellate.rs:1257` (`TIEBREAK_NEUTER`), `:2582`
(`YANG_SHIFT_NEUTER`), `boolean.rs:1853` (`YANG_RIM_JUNCTION_DISABLE`) are
alternate geometry paths flippable by an env var in the release/WASM bundle
(kernel-v2's ~24 probes were verified individually inert; yang's tracing
probes default correct). Fix: registry doc (name → site → tracing vs
behavior-altering); gate neuters behind `#[cfg(debug_assertions)]` or a
`probes` feature; sweep probes belonging to closed tasks (`KV9_JUNCTION_PROBE`,
`YANG_T145_*`, and the committed "TEMP (uncommitted)" probe at
`tessellate.rs:2679`).

### F13. Three genuinely-red modeling-ops tests ignored with no milestone/tracking — MEDIUM
`modeling-ops/tests/op_tests.rs:305,722,1093` — "MockKernel union face-count
drift, needs separate triage." No tag means nothing will ever reap them, and
they cast doubt on MockKernel union semantics other tests rely on. Fix: file
a tracked task, put its id in the ignore reason, or fix the drift.

### F14. Determinism unproven for the new stack; the only report audits deleted code — MEDIUM
`docs/NONDETERMINISM-REPORT.md` is entirely about truck; its prescribed
digest-equality harness was never built. Residual `std::HashMap` iteration in
hot yang paths (~46 `.values()`/`.iter()` loops sampled across
`stage4_correct.rs`/`stage1_tessellate.rs`/`boolean.rs`) is latent
cross-process nondeterminism if any "first wins" decision rides on it
(A4.2/§8). Fix: rewrite the report against kernel-v2 and add the digest test;
audit the HashMap loops or switch to BTreeMap where order can matter.

### F15. Corpus `euler_target` has no automated self-consistency guard — LOW-MEDIUM
Both historical oracle authoring errors (R0099, R0006) were caught by manual
investigation. Independent oracles (watertightness, volume, penetration)
mitigate, but a meta-consistency test — derive χ independently from the
reference mesh, assert it matches `euler_target` per case — would have caught
both automatically. (`test-harness/src/assay/gen.rs:399`.)

### F16. Error-taxonomy asymmetries at the trait boundary — LOW
`waffle-types/types.rs:57-75` defines a 6-variant structured `BooleanError`
that `:111` flattens to `BooleanFailed{reason: String}` before any consumer
sees it (A6.2 regression: consumers must string-match); `KernelError` has
`FilletFailed`/`ShellFailed` but no `ChamferFailed`/`RevolveFailed`; and
fillet/chamfer/shell are *required* trait methods (traits.rs:77-98) with
hand-written stubs in both impls, despite being deferred indefinitely —
unlike every other unsupported op, which gets a default `Err(NotSupported)`
body. Fix: keep stage in `BooleanFailed{stage, reason}`; give the three
deferred ops default bodies; decide operation-keyed vs generic error naming.

### F17. Repo hygiene — LOW
Eight untracked `.waffle` repro/save files at repo root (one 1.2 MB,
`err.waffle`, from June) plus an untracked `superpowers.md` essay. Move repro
fixtures into a gitignored `fixtures/` or `scratch/` location (some, e.g.
`error_coplanar.waffle`, are referenced by tasks — keep them, but not at
root); decide whether superpowers.md belongs in docs/notes or out of tree.

### F18. GUI quarantines cite prose, not grep-able milestone tags — LOW
18 `test.fixme` in `app/tests/gui/` cluster on real walls (M8 coplanar,
boolean dialog, arc-drag) but lack the KV6/M8/M5 tags the reaping workflow
greps for. Fix: add tags to fixme titles.

---

## 5. Governance-document inconsistencies (dedicated, per request)

The four governance pillars (Constitution, Invariants, FIP, DoD) are
internally consistent with each other and with the precedence chains in
AGENTS.md/CLAUDE.md. The rot is in the descriptive layer, plus two
policy-vs-practice gaps that need a human decision:

| # | Issue | Evidence | Fix |
|---|---|---|---|
| G1 | **docs/TESTING.md documents the dead nightly+`panic=unwind` WASM workflow** — claims `catch_unwind` catches kernel panics in WASM; the stable build is panic=abort, the opposite. Actively dangerous to test authors. | TESTING.md:116-122 vs rust-toolchain.toml, .cargo/config.toml:5-8 | Rewrite the crash-detection section |
| G2 | **ARCHITECTURE.md "Current Kernel Status" describes the deleted `crates/kernel/` in present tense** ("980 kernel tests pass", "Phase 5 in progress", assay "190 cases" ×4 — corpus is 294) — in a §11-protected file read at every session start. | ARCHITECTURE.md:24,130,179-199 | Replace with kernel-v2 stack or move under an explicit HISTORICAL heading |
| G3 | **CLAUDE.md session-start checklist routes kernel agents into doubly-archived dossiers** (projects/01-kernel-fork → truck → deleted crates/kernel). | CLAUDE.md steps 6-7; projects/01-kernel-fork/CLAUDE.md:3-5 | Redirect kernel work to the roadmap + crate routing table; retire or re-banner projects/01..10 |
| G4 | **A15.4 SSI matrix stale (self-admitted)** and contradicts ARCHITECTURE.md's "all 15 integrated"; `specs/ssi_solver_matrix.md` void (see F5). | ARCHITECTURAL_INVARIANTS.md:442-470 | Re-audit table against ssi-rs (its own note requests this) |
| G5 | **`waffle-types/units.rs` claims A14 tolerance authority while 96% dead** (see F3) and `specs/boolean_tolerance_layering.md` targets the deleted truck/kernel-fork stack. | units.rs header; spec | Repoint A14 at cad-primitives; mark spec historical |
| G6 | **FIP 5-role separation (Test Author ≠ Implementer, P5) is not followed** by the actual solo-agent increment workflow; adversary-validation docs partially honor the spirit. Also FIP §5.2 assumes coverage gates §12 lists as "planned". | FIP:34-43; Constitution P5; git log | **Maintainer decision:** amend (solo-operator sequential-role variant) or conform. Leaving it silent makes tier-1 governance aspirational |
| G7 | **P2 "spec in /specs/ precedes work" vs roadmap-as-plan-of-record ambiguity** — many Yang increments ship on roadmap milestones + memory entries without a per-increment spec; docs never say whether that satisfies or waives P2 | Constitution:52-58 vs CLAUDE.md/roadmap §0.1 | State explicitly that a roadmap milestone entry + deviation-log entry constitutes the P2 artifact for Yang increments (or require specs) |
| G8 | **docs/INDEX.md (2026-03-09) omits two of four governance pillars** (DoD, Invariants), says "P1-P8" (now P1-P10), predates kernel-v2 | INDEX.md:1,22-25 | Regenerate |
| G9 | **INTERFACES.md missing six Kernel-trait methods** added in the last 3 weeks (import_body, make_face_from_region, face_provenance, boolean_union_multi, extract_edges, export_step) while CLAUDE.md calls it "the contract" | INTERFACES.md:962 vs traits.rs | Regenerate from traits.rs, or demote and point at `waffle_types::kernel` |
| G10 | **Test-tier drift**: docs/TESTING.md omits the `rewrite` and `assay` tiers; timing tables reference the deleted kernel crate | TESTING.md:48-56 vs scripts/test.sh | Regenerate from test.sh |
| G11 | Minor: ARCHITECTURE.md mis-cites #24 as "Barton et al." (it's Yang); lists "STEP I/O (AP203)" as a kernel capability (export is NotSupported; import is a separate crate); "clean-room" shorthand in CLAUDE.md flattens the real provenance (MIT-licensed *port* with per-file attribution for arrangement/labeling; true clean-room only for the LGPL-avoiding predicates — LICENSE-THIRD-PARTY.md has it right) | ARCHITECTURE.md:24,110 | One-line fixes |

---

## 6. What is healthy — preserve these

- **cherchi-rs exactness core**: textbook three-tier cascade, sound outward
  rounding, zero `unsafe`, real differential adversary tests, a flagship
  parity test that refuses to self-skip, and auditable clean-room provenance
  (independent derivation proven by matching published filter constants).
- **arena.rs / euler.rs**: zero unsafe/panic/unwrap/raw-index; tombstone slots
  with no id reuse; whole-arena invariant re-verified at every Euler-op exit.
- **kernel-v2 error taxonomy**: ~40 typed variants each documenting the
  invariant it guards and the governance rule it honors; NotSupported walls
  named by sub-reason; "never a silent chord approximation" contracts.
- **Module-to-paper mapping in yang-rs**: every stage header cites its paper
  section with `refs/text/` line ranges. Don't let decomposition erode this.
- **Corpus-ID discipline**: case IDs as provenance comments, never logic.
- **ssi-rs**: deterministic, reference-cited, principled *linear* branch
  gating (never squared/discriminant quantities), loud-NaN SurfacePair eval.
- **SurfacePair as a cross-crate contract**: same invariants carried through
  ssi-rs → yang-rs → kernel-v2; the A15 "implicit-but-exact" discipline done
  right.
- **The layering itself**: compiler-enforced, leak-free, with cad-primitives
  discipline ("if it has a fn doing computation it doesn't belong here").
- **Assay categorizer**: verdicts keyed on typed walls (with a regression test
  pinning the old feature-name mislabel); FULL/FAST results.json discipline.
- **P10 restraint artifacts**: the deliberately-unwired §4.4.1(b) survivor
  (`stage4_correct.rs:3091-3106`) with written rationale — link it to a
  tracking entry, but keep the pattern.

---

## 7. Suggested remediation sequence

1. **Before the assay push** (small, high leverage, all mechanical except the
   first): F1 production planarity gate → F2 wire oracles into CI →
   F3 tolerance-constant unification + dead-vocabulary deletion → F4
   deviation-log catch-up → F5/G4 SSI matrix rewrite. Plus G1 (the TESTING.md
   panic=unwind trap) — ten-minute fix, prevents a real future bug.
2. **Next refactor window**: F7+F9 decompositions (proven move-only recipe,
   byte-identical assay per increment), F8 tolerance naming (`TAU_EVAL`,
   `stage4::tol`), F6 rim-typing migration into yang Stage 5/6 (largest item;
   schedule as its own milestone — it removes recover.rs-class debt
   permanently).
3. **Background/batched**: F10-F18, remaining G-items, G6/G7 as explicit
   maintainer decisions.

---

*Review artifacts: seven agent reports synthesized 2026-07-12; verification
spot-checks in session transcript. Baseline: HEAD `3568db09`, rewrite tier
2134/0, assay 241C/0W/50E/4U/0T.*
